#!/bin/bash
# Build script for Speed Skating Timer
# Compiles backend in release mode, builds frontend, and creates dist folder
#
# Usage: ./build.sh [--target TARGET] [--gpio GPIO_CONFIG]
#   --target, -t: Target platform (linux|rpi). Default: linux
#   --gpio, -g:   Path to GPIO config file. Default: backend/gpio.yaml

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# Parse command line arguments
TARGET="linux"
GPIO_CONFIG="backend/gpio.yaml"
while [[ $# -gt 0 ]]; do
    case $1 in
        --target|-t)
            TARGET="$2"
            shift 2
            ;;
        --gpio|-g)
            GPIO_CONFIG="$2"
            shift 2
            ;;
        --help|-h)
            echo "Usage: $0 [--target TARGET] [--gpio GPIO_CONFIG]"
            echo "  --target, -t: Target platform (linux|rpi). Default: linux"
            echo "  --gpio, -g:   Path to GPIO config file. Default: backend/gpio.yaml"
            echo ""
            echo "Examples:"
            echo "  $0 --target rpi --gpio backend/gpio-rpi-5.yaml"
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            echo "Use --help for usage information"
            exit 1
            ;;
    esac
done

# Set Rust target triple based on platform
case $TARGET in
    linux)
        RUST_TARGET=""
        TARGET_DIR="release"
        ;;
    rpi)
        RUST_TARGET="arm-unknown-linux-musleabihf"
        TARGET_DIR="arm-unknown-linux-musleabihf/release"
        ;;
    *)
        echo "Error: Unknown target '$TARGET'. Supported targets: linux, rpi"
        exit 1
        ;;
esac

# Validate required files and directories exist - fail early
echo "Validating required files and directories..."
if [ ! -d ".nats-creds" ]; then
    echo "Error: .nats-creds directory not found"
    exit 1
fi

if [ ! -f "backend/auth.yaml" ]; then
    echo "Error: backend/auth.yaml not found"
    exit 1
fi

if [ ! -f "$GPIO_CONFIG" ]; then
    echo "Error: GPIO config not found: $GPIO_CONFIG"
    exit 1
fi

if [ ! -f "etc/run.sh" ]; then
    echo "Error: etc/run.sh not found"
    exit 1
fi

if [ ! -f "etc/SpeedSkating.service" ]; then
    echo "Error: etc/SpeedSkating.service not found"
    exit 1
fi

if [ ! -f "etc/nats.service" ]; then
    echo "Error: etc/nats.service not found"
    exit 1
fi


echo "Building Speed Skating Timer for target: $TARGET"
if [ -n "$RUST_TARGET" ]; then
    echo "Rust target triple: $RUST_TARGET"
fi
echo "GPIO config: $GPIO_CONFIG"

# Clean previous dist folder
if [ -d "dist" ]; then
    echo "Cleaning previous dist folder..."
    rm -rf dist
fi

# Create dist directory structure
mkdir -p dist

# Build backend in release mode
echo "Building backend in release mode..."
cd backend
if [ -n "$RUST_TARGET" ]; then
    "$HOME/.cargo/bin/cross" build --release --target "$RUST_TARGET"
else
    cargo build --release
fi
cd ..

# Copy backend binary
echo "Copying backend binary..."
cp "backend/target/$TARGET_DIR/server" dist/server

# Build frontend
echo "Building frontend..."
cd frontend
pnpm build
cd ..

# Copy frontend dist files
echo "Copying frontend files..."
mkdir -p dist/frontend
cp -r frontend/dist/* dist/frontend/

# Copy support files
echo "Copying support files..."

# Copy .nats-creds directory to config/nats-creds
mkdir -p dist/config
echo "Copying .nats-creds directory to config/nats-creds..."
cp -r .nats-creds dist/config/nats-creds

# Copy YAML configuration files
mkdir -p dist/conf
cp backend/auth.yaml dist/conf/
cp "$GPIO_CONFIG" dist/conf/gpio.yaml

# Copy run script
echo "Copying run script..."
cp etc/run.sh dist/run.sh
chmod +x dist/run.sh

echo ""
echo "Build complete! Distribution files are in the 'dist' folder:"
echo ""
echo "Application files (dist/):"
echo "  - Binary: dist/server"
echo "  - Frontend: dist/frontend/index.html and dist/frontend/assets/"
echo "  - Config: dist/config/nats-creds/ (includes nats-server.conf), dist/conf/auth.yaml, dist/conf/gpio.yaml"
echo "  - Run script: dist/run.sh"
echo ""
echo "Deployment:"
echo "  - Run from project root: ./deploy.sh"
echo "  - Service files are read from etc/ during deployment"

