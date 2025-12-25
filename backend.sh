#!/bin/bash
# Common backend script setup for NATS credentials
# Source this script to get BACKEND_ARGS array with common NATS credential arguments

DIR="$(dirname "${BASH_SOURCE[0]}")"

# Set up common NATS credential arguments
BACKEND_ARGS=(
    --nats-creds "../.nats-creds/server.creds"
    --account-seed "../.nats-creds/account.seed"
    --frontend-user-seed "../.nats-creds/frontend.seed"
    --auth-config "auth.yaml"
)

