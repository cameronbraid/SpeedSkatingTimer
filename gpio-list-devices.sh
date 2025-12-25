#!/bin/bash

# List all gpio-sim devices in configfs

CONFIGFS_DIR="/sys/kernel/config/gpio-sim"

if ! mountpoint -q /sys/kernel/config; then
    echo "configfs is not mounted"
    exit 1
fi

if [[ ! -d "$CONFIGFS_DIR" ]]; then
    echo "No gpio-sim devices found"
    exit 0
fi

echo "GPIO sim devices in configfs:"
echo "=============================="
for device_dir in "$CONFIGFS_DIR"/*; do
    if [[ -d "$device_dir" ]]; then
        device_name=$(basename "$device_dir")
        echo ""
        echo "Device: $device_name"
        echo "  Path: $device_dir"
        
        # Check if it's live
        if [[ -f "$device_dir/live" ]]; then
            live_status=$(cat "$device_dir/live" 2>/dev/null)
            if [[ "$live_status" == "1" ]]; then
                echo "  Status: ACTIVE"
            else
                echo "  Status: Inactive"
            fi
        fi
        
        # List banks
        for bank_dir in "$device_dir"/*; do
            if [[ -d "$bank_dir" ]] && [[ "$(basename "$bank_dir")" != "live" ]]; then
                bank_name=$(basename "$bank_dir")
                echo "  Bank: $bank_name"
                if [[ -f "$bank_dir/num_lines" ]]; then
                    num_lines=$(cat "$bank_dir/num_lines" 2>/dev/null)
                    echo "    Lines: $num_lines"
                fi
            fi
        done
    fi
done

echo ""
echo "Active GPIO chips:"
echo "=================="
for chip in /dev/gpiochip*; do
    if [[ -c "$chip" ]]; then
        chip_name=$(basename "$chip")
        # Try to find if it's from gpio-sim
        sim_path=$(find /sys/devices/platform -name "$chip_name" 2>/dev/null | grep gpio-sim | head -n 1)
        if [[ -n "$sim_path" ]]; then
            echo "  $chip_name -> gpio-sim (found at $sim_path)"
        else
            echo "  $chip_name -> hardware/other"
        fi
    fi
done

