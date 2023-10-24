#!/bin/bash

set -o errexit
set -o nounset
set -o pipefail
set -o xtrace

readonly TARGET_HOST=pi@raspberrypi.local

scp SpeedSkating-Backend.service ${TARGET_HOST}:/home/pi
ssh -t ${TARGET_HOST} 'sudo bash -c "cp /home/pi/SpeedSkating-Backend.service /lib/systemd/system/SpeedSkating-Backend.service && systemctl daemon-reload && systemctl start SpeedSkating-Backend.service && sudo systemctl enable SpeedSkating-Backend.service"'
