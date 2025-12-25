#!/bin/bash
# Run script for Speed Skating Timer server (built version)
# Assumes this script is run from the dist directory
# Fails immediately if required files are missing

set -e

# Paths relative to dist directory
SERVER_BINARY="./server"
NATS_CREDS="config/nats-creds/server.creds"
ACCOUNT_SEED="config/nats-creds/account.seed"
FRONTEND_SEED="config/nats-creds/frontend.seed"
AUTH_CONFIG="conf/auth.yaml"
GPIO_CONFIG="conf/gpio.yaml"
FRONTEND_DIR="frontend"

# Check required files exist - fail fast
if [ ! -f "$SERVER_BINARY" ]; then
    echo "Error: Server binary not found at $SERVER_BINARY"
    exit 1
fi

if [ ! -f "$ACCOUNT_SEED" ]; then
    echo "Error: Account seed file not found at $ACCOUNT_SEED"
    exit 1
fi

if [ ! -f "$FRONTEND_SEED" ]; then
    echo "Error: Frontend seed file not found at $FRONTEND_SEED"
    exit 1
fi

if [ ! -f "$AUTH_CONFIG" ]; then
    echo "Error: Auth config file not found at $AUTH_CONFIG"
    exit 1
fi

if [ ! -d "$FRONTEND_DIR" ]; then
    echo "Error: Frontend directory not found at $FRONTEND_DIR"
    exit 1
fi

if [ ! -f "$NATS_CREDS" ]; then
    echo "Error: NATS credentials file not found at $NATS_CREDS"
    exit 1
fi

# Build command arguments
ARGS=()
ARGS+=(--nats-creds "$NATS_CREDS")
ARGS+=(--account-seed "$ACCOUNT_SEED")
ARGS+=(--frontend-user-seed "$FRONTEND_SEED")
ARGS+=(--auth-config "$AUTH_CONFIG")
ARGS+=(--frontend-dir "$FRONTEND_DIR")
ARGS+=(gpio --config "$GPIO_CONFIG")

# Run the server with all required arguments and any additional user arguments
exec "$SERVER_BINARY" "${ARGS[@]}" "$@"

