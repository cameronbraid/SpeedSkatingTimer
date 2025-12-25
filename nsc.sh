#!/bin/bash
# Wrapper script for nsc (NATS System Configuration) tool
# Configures nsc to use .nats-creds directory for all operations

set -e

# Directory for storing NATS credentials (same as nats.sh)
CREDS_DIR="${CREDS_DIR:-./.nats-creds}"

# Check if nsc is available
if ! command -v nsc &> /dev/null; then
    echo "Error: nsc (NATS CLI) not found. Please install it:"
    echo "  from https://github.com/nats-io/nsc/releases"
    exit 1
fi

# Set nsc store directory to use our custom directory (not ~/.local)
NSC_STORE="$CREDS_DIR/.nsc"
NSC_KEYS="$CREDS_DIR/.nsc/keys"
mkdir -p "$NSC_STORE"
mkdir -p "$NSC_KEYS"

# Set environment variable - NKEYS_PATH should point to parent of 'keys' directory
export NKEYS_PATH="$CREDS_DIR/.nsc"

# Configure nsc to use our custom store directory
# This updates the nsc.json config file
nsc env --store "$NSC_STORE" > /dev/null 2>&1 || true

# Find nsc config file location (usually in XDG config dir or ~/.config)
NSC_CONFIG=""
if [ -n "$XDG_CONFIG_HOME" ]; then
    NSC_CONFIG="$XDG_CONFIG_HOME/nats/nsc/nsc.json"
elif [ -d ~/.config ]; then
    NSC_CONFIG=~/.config/nats/nsc/nsc.json
fi

# Update nsc config to use our store directory
if [ -n "$NSC_CONFIG" ] && [ -f "$NSC_CONFIG" ]; then
    # Escape forward slashes for sed
    ESCAPED_STORE=$(echo "$NSC_STORE" | sed 's/[\/&]/\\&/g')
    # Update store_root and clear operator/account references
    sed -i.bak "s|\"store_root\":\"[^\"]*\"|\"store_root\":\"$ESCAPED_STORE\"|g; s/,\"operator\":\"[^\"]*\"//g; s/,\"account\":\"[^\"]*\"//g" "$NSC_CONFIG" 2>/dev/null || true
fi

# Set default operator and account context if they exist
OPERATOR_NAME="${OPERATOR_NAME:-OPERATOR}"
ACCOUNT_NAME="${ACCOUNT_NAME:-ACCOUNT}"

# Set operator context if operator exists
if [ -d "$NSC_STORE/$OPERATOR_NAME" ]; then
    nsc env -o "$OPERATOR_NAME" > /dev/null 2>&1 || true
    
    # Set account context if account exists
    if [ -d "$NSC_STORE/$OPERATOR_NAME/accounts/$ACCOUNT_NAME" ]; then
        nsc env -a "$ACCOUNT_NAME" > /dev/null 2>&1 || true
    fi
fi

# Print configuration info if no arguments provided
if [ $# -eq 0 ]; then
    echo "NATS System Configuration (nsc) wrapper"
    echo "========================================"
    echo ""
    echo "Store directory: $NSC_STORE"
    echo "Keys directory: $NSC_KEYS"
    echo "NKEYS_PATH: $NKEYS_PATH"
    echo ""
    exec nsc "help"

fi

# Pass through all arguments to nsc
# Use --keystore-dir for commands that support it
exec nsc "$@"
