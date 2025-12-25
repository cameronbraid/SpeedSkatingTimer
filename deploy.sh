#!/bin/bash
# Deployment script for Speed Skating Timer
# Assumes this script is run from the project root directory
# Syncs app files from dist/ to target and ensures services are installed and running
#
# Usage: ./deploy.sh

set -o errexit
set -o nounset
set -o pipefail
# set -o xtrace  # Uncomment for debug output

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

readonly TARGET_USER="${TARGET_USER:-pi}"
readonly TARGET_HOSTNAME="${TARGET_HOSTNAME:-rpi}"
readonly TARGET_HOST="${TARGET_USER}@${TARGET_HOSTNAME}"
readonly APP_DIR="/home/${TARGET_USER}/app"
readonly DIST_DIR="dist"

# Check that we're in the right location (project root)
if [ ! -d "$DIST_DIR" ]; then
    echo "Error: dist/ directory not found"
    echo "This script must be run from the project root directory"
    echo "Expected structure: project_root/dist/"
    exit 1
fi

# Check for required files in dist
if [ ! -f "$DIST_DIR/server" ]; then
    echo "Error: dist/server binary not found"
    echo "Please run build.sh first to create the distribution"
    exit 1
fi

if [ ! -f "$DIST_DIR/run.sh" ]; then
    echo "Error: dist/run.sh not found"
    echo "Please run build.sh first to create the distribution"
    exit 1
fi

# Check for required service files in etc/
if [ ! -f "etc/SpeedSkating.service" ]; then
    echo "Error: etc/SpeedSkating.service not found"
    exit 1
fi

if [ ! -f "etc/nats.service" ]; then
    echo "Error: etc/nats.service not found"
    exit 1
fi

echo "Copying files to ${TARGET_HOST}:${APP_DIR}..."

# Copy all files first: dist directory and service files
rsync -avz --delete \
    --exclude='*.log' \
    --exclude='*.tmp' \
    --filter='protect config/nats-creds/jetstream' \
    "$DIST_DIR/" ${TARGET_HOST}:${APP_DIR}/

scp -q etc/SpeedSkating.service etc/nats.service ${TARGET_HOST}:${APP_DIR}/

echo "Running remote setup..."

# Run all remote commands in a single SSH session
ssh -t ${TARGET_HOST} "bash -s" << 'REMOTE_SCRIPT'
set -e

APP_DIR="/home/pi/app"

# Check if nats-server is installed, install if not
if ! command -v nats-server &> /dev/null && [ ! -f /usr/bin/nats-server ]; then
    echo "NATS server not found. Installing..."
    
    # Check for required tools
    if ! command -v curl &> /dev/null; then
        echo "Installing curl..."
        sudo apt-get update && sudo apt-get install -y curl || exit 1
    fi
    
    ARCH=$(uname -m)
    case $ARCH in
        armv6l)
            NATS_ARCH="arm6"
            ;;
        armv7l)
            NATS_ARCH="arm7"
            ;;
        aarch64|arm64)
            NATS_ARCH="arm64"
            ;;
        *)
            echo "Error: Unsupported architecture: $ARCH"
            exit 1
            ;;
    esac
    
    # Try to get latest version, fallback to fixed version
    NATS_VERSION=$(curl -s https://api.github.com/repos/nats-io/nats-server/releases/latest 2>/dev/null | grep '"tag_name"' | head -1 | cut -d '"' -f 4)
    if [ -z "$NATS_VERSION" ]; then
        # Fallback to a known working version
        NATS_VERSION="v2.10.22"
        echo "Using fallback version: ${NATS_VERSION}"
    else
        echo "Using latest version: ${NATS_VERSION}"
    fi
    
    # Download and install
    DOWNLOAD_URL="https://github.com/nats-io/nats-server/releases/download/${NATS_VERSION}/nats-server-${NATS_VERSION}-linux-${NATS_ARCH}.tar.gz"
    echo "Downloading NATS server ${NATS_VERSION} for ${NATS_ARCH}..."
    cd /tmp
    curl -fL -o nats-server.tar.gz "${DOWNLOAD_URL}" || { echo "Download failed from: ${DOWNLOAD_URL}"; exit 1; }
    tar -xzf nats-server.tar.gz || exit 1
    sudo mv nats-server-${NATS_VERSION}-linux-${NATS_ARCH}/nats-server /usr/bin/nats-server || exit 1
    sudo chmod +x /usr/bin/nats-server || exit 1
    rm -rf nats-server-${NATS_VERSION}-linux-${NATS_ARCH} nats-server.tar.gz
    echo "NATS server installed successfully"
else
    echo "NATS server already installed: $(nats-server --version 2>&1 | head -1)"
fi

# Install and restart services
echo "Installing systemd services..."
sudo cp ${APP_DIR}/nats.service /lib/systemd/system/nats.service
sudo cp ${APP_DIR}/SpeedSkating.service /lib/systemd/system/SpeedSkating.service
sudo systemctl daemon-reload

echo "Starting services..."
sudo systemctl enable nats.service SpeedSkating.service
sudo systemctl restart nats.service
sudo systemctl restart SpeedSkating.service

# Verify services started
if ! systemctl is-active --quiet nats.service; then
    echo "ERROR: NATS service failed to start"
    sudo journalctl -u nats.service --no-pager -n 10
    exit 1
fi

if ! systemctl is-active --quiet SpeedSkating.service; then
    echo "ERROR: SpeedSkating service failed to start"
    sudo journalctl -u SpeedSkating.service --no-pager -n 10
    exit 1
fi

echo ""
echo "Services status:"
systemctl status nats.service --no-pager -l | head -n 3
systemctl status SpeedSkating.service --no-pager -l | head -n 3
REMOTE_SCRIPT

echo ""
echo "Deployment complete!"
