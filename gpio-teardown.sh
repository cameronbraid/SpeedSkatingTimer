#!/bin/bash

# GPIO Teardown Script
# Deactivates and removes the GPIO chip configuration created by gpio-provision.sh

CHIP_ID="${CHIP_ID:-gpiochip1}"
CONFIGFS_DIR="/sys/kernel/config/gpio-sim"
DEVICE_NAME="${DEVICE_NAME:-speedskating-timer}"

# 1. Check if configfs is mounted
if ! mountpoint -q /sys/kernel/config; then
    echo "configfs is not mounted. Nothing to tear down." >&2
    exit 0
fi

# 2. Check if the device directory exists
if [[ ! -d "$CONFIGFS_DIR/$DEVICE_NAME" ]]; then
    echo "GPIO device '$DEVICE_NAME' not found in configfs. Nothing to tear down." >&2
    exit 0
fi

# 3. Check if the chip still exists
SIM_PATH=$(find /sys/devices/platform -name "$CHIP_ID" 2>/dev/null | head -n 1)

if [[ -n "$SIM_PATH" ]]; then
    echo "Deactivating GPIO chip '$CHIP_ID'..." >&2
    
    # Deactivate the device (this removes the GPIO chip)
    if [[ -f "$CONFIGFS_DIR/$DEVICE_NAME/live" ]]; then
        echo "0" | sudo tee "$CONFIGFS_DIR/$DEVICE_NAME/live" > /dev/null || {
            echo "Warning: Failed to deactivate GPIO device" >&2
        }
        
        # Wait a moment for the device to be removed
        sleep 0.5
        
        # Verify the chip was removed
        SIM_PATH=$(find /sys/devices/platform -name "$CHIP_ID" 2>/dev/null | head -n 1)
        if [[ -z "$SIM_PATH" ]]; then
            echo "GPIO chip '$CHIP_ID' deactivated successfully" >&2
        else
            echo "Warning: GPIO chip '$CHIP_ID' may still be active" >&2
        fi
    fi
fi

# 4. Remove the device directory from configfs
if [[ -d "$CONFIGFS_DIR/$DEVICE_NAME" ]]; then
    echo "Removing GPIO device configuration..." >&2
    
    # Remove bank directories first (if any exist)
    for bank_dir in "$CONFIGFS_DIR/$DEVICE_NAME"/*; do
        if [[ -d "$bank_dir" ]]; then
            sudo rmdir "$bank_dir" 2>/dev/null || {
                echo "Warning: Failed to remove bank directory: $bank_dir" >&2
            }
        fi
    done
    
    # Remove the device directory
    sudo rmdir "$CONFIGFS_DIR/$DEVICE_NAME" 2>/dev/null || {
        echo "Warning: Failed to remove device directory (may not be empty or may have been removed already)" >&2
    }
    
    if [[ ! -d "$CONFIGFS_DIR/$DEVICE_NAME" ]]; then
        echo "GPIO device configuration removed successfully" >&2
    fi
fi

echo "GPIO teardown complete" >&2

