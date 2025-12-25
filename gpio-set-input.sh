#!/bin/bash

# USAGE: ./set_input.sh <LINE_NUMBER> <VALUE>
# Example: ./set_input.sh 0 1

LINE=$1
VALUE=$2
CHIP_ID="gpiochip1"

# 1. Check arguments
if [[ -z "$LINE" ]] || [[ -z "$VALUE" ]]; then
    echo "Usage: $0 <line_number> <value (0 or 1)>"
    echo "Example: $0 0 1"
    exit 1
fi

# 2. Check if GPIO chip exists
SIM_PATH=$(find /sys/devices/platform -name "$CHIP_ID" 2>/dev/null | head -n 1)
if [[ -z "$SIM_PATH" ]]; then
    echo "Error: GPIO chip '$CHIP_ID' not found. Please provision it first using gpio-provision.sh"
    exit 1
fi

# 5. Construct the path to the specific line's 'pull' file
# The standard structure is: .../gpiochip1/sim_gpioX/pull
TARGET_FILE="$SIM_PATH/sim_gpio$LINE/pull"

# 6. Verify the file exists
if [[ ! -f "$TARGET_FILE" ]]; then
    echo "Error: Line $LINE does not exist on this chip."
    echo "Path checked: $TARGET_FILE"
    exit 1
fi


echo "Setting Line $LINE to $VALUE in $TARGET_FILE"
sudo sh -c "echo $VALUE > $TARGET_FILE"