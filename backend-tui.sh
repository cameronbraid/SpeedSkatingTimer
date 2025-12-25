#!/bin/bash

DIR="$(dirname "${BASH_SOURCE[0]}")"
source "$DIR/backend.sh"

cd $DIR/backend
cargo run --bin tui -- --nats-creds "../.nats-creds/server.creds"
