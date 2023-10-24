#!/bin/bash

set -o errexit
set -o nounset
set -o pipefail
set -o xtrace

readonly TARGET_HOST=pi@raspberrypi.local
readonly TARGET_PATH=/home/pi/frontend/
readonly SOURCE_PATH=./dist/

pnpm vite build
rsync -a ${SOURCE_PATH} ${TARGET_HOST}:${TARGET_PATH} 
