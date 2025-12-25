#!/bin/bash

# GPIO Setup Script
# Ensures gpio-sim module is loaded and the GPIO chip is configured
# Outputs the SIM_PATH to stdout on success

CHIP_ID="${CHIP_ID:-gpiochip1}"
CONFIGFS_DIR="/sys/kernel/config/gpio-sim"
DEVICE_NAME="${DEVICE_NAME:-speedskating-timer}"
BANK_NAME="${BANK_NAME:-bank0}"
NUM_LINES="${NUM_LINES:-16}"  # Enough for pins 0-15

# 1. Ensure gpio-sim module is loaded
if ! lsmod | grep -q "^gpio_sim"; then
    echo "Loading gpio-sim module..." >&2
    sudo modprobe gpio-sim || {
        echo "Error: Failed to load gpio-sim module. Is it available?" >&2
        exit 1
    }
fi

# 2. Mount configfs if not already mounted
if ! mountpoint -q /sys/kernel/config; then
    echo "Mounting configfs..." >&2
    sudo mount -t configfs none /sys/kernel/config || {
        echo "Error: Failed to mount configfs" >&2
        exit 1
    }
fi

# 3. Check if chip already exists, if not create it
SIM_PATH=$(find /sys/devices/platform -name "$CHIP_ID" 2>/dev/null | head -n 1)

if [[ -z "$SIM_PATH" ]]; then
    echo "GPIO chip '$CHIP_ID' not found. Creating it..." >&2
    
    # Check if gpiochip0 exists - if it does, our new chip will be gpiochip1
    # If it doesn't, we may need to create a dummy one first
    EXISTING_CHIP0=$(find /sys/devices/platform -name "gpiochip0" 2>/dev/null | head -n 1)
    
    # Create device directory if it doesn't exist
    if [[ ! -d "$CONFIGFS_DIR/$DEVICE_NAME" ]]; then
        sudo mkdir -p "$CONFIGFS_DIR/$DEVICE_NAME" || {
            echo "Error: Failed to create device directory" >&2
            exit 1
        }
    fi
    
    # Create bank directory if it doesn't exist
    if [[ ! -d "$CONFIGFS_DIR/$DEVICE_NAME/$BANK_NAME" ]]; then
        sudo mkdir -p "$CONFIGFS_DIR/$DEVICE_NAME/$BANK_NAME" || {
            echo "Error: Failed to create bank directory" >&2
            exit 1
        }
    fi
    
    # Set number of lines
    echo "$NUM_LINES" | sudo tee "$CONFIGFS_DIR/$DEVICE_NAME/$BANK_NAME/num_lines" > /dev/null || {
        echo "Error: Failed to set number of lines" >&2
        exit 1
    }
    
    # Activate the device (this creates the actual GPIO chip)
    echo "1" | sudo tee "$CONFIGFS_DIR/$DEVICE_NAME/live" > /dev/null || {
        echo "Error: Failed to activate GPIO device" >&2
        exit 1
    }
    
    # Wait a moment for the device to be created
    sleep 0.5
    
    # Verify the chip was created and find its actual name
    SIM_PATH=$(find /sys/devices/platform -name "$CHIP_ID" 2>/dev/null | head -n 1)
    if [[ -z "$SIM_PATH" ]]; then
        # Check if it was created with a different name
        CREATED_CHIP=$(find /sys/devices/platform -type d -name "gpiochip*" 2>/dev/null | grep gpio-sim | head -n 1 | xargs basename)
        if [[ -n "$CREATED_CHIP" ]]; then
            echo "Warning: Created chip is named '$CREATED_CHIP' but expected '$CHIP_ID'" >&2
            echo "You may need to update gpio.yaml to use /dev/$CREATED_CHIP" >&2
            CHIP_ID="$CREATED_CHIP"
            SIM_PATH=$(find /sys/devices/platform -name "$CHIP_ID" 2>/dev/null | head -n 1)
        else
            echo "Error: GPIO chip was not created successfully" >&2
            exit 1
        fi
    fi
    
    if [[ -z "$SIM_PATH" ]]; then
        echo "Error: Could not locate created GPIO chip" >&2
        exit 1
    fi
    
    echo "GPIO chip '$CHIP_ID' created successfully at $SIM_PATH" >&2
fi

# Output the SIM_PATH to stdout (for the calling script to capture)
echo "$SIM_PATH"

