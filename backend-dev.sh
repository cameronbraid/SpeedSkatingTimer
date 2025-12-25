#!/bin/bash

DIR="$(dirname "${BASH_SOURCE[0]}")"
source "$DIR/backend.sh"

cd $DIR/backend
cargo run --bin server -- "${BACKEND_ARGS[@]}" --dev --simulator
