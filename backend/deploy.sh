#!/bin/bash

set -o errexit
set -o nounset
set -o pipefail
set -o xtrace

readonly TARGET_HOST=pi@raspberrypi.local
readonly TARGET_PATH=/home/pi/SpeedSkating-Backend
readonly TARGET_ARCH=arm-unknown-linux-musleabihf
readonly SOURCE_PATH=./target/${TARGET_ARCH}/release/SpeedSkating-Backend

cargo build --release --target=${TARGET_ARCH}
rsync ${SOURCE_PATH} ${TARGET_HOST}:${TARGET_PATH}
ssh -t ${TARGET_HOST} sudo systemctl restart SpeedSkating-Backend.service
