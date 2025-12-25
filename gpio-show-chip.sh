#!/bin/bash

# Show GPIO sim chip information
# Displays chip status, configuration, and pin states

CHIP_ID="${CHIP_ID:-gpiochip1}"
CONFIGFS_DIR="/sys/kernel/config/gpio-sim"
DEVICE_NAME="${DEVICE_NAME:-speedskating-timer}"

# Colors for output (only if output is a terminal)
if [[ -t 1 ]]; then
    RED='\033[0;31m'
    GREEN='\033[0;32m'
    YELLOW='\033[1;33m'
    BLUE='\033[0;34m'
    NC='\033[0m' # No Color
else
    RED=''
    GREEN=''
    YELLOW=''
    BLUE=''
    NC=''
fi

echo "GPIO Sim Chip Information"
echo "========================="
echo ""

# 1. Check if chip exists
SIM_PATH=$(find /sys/devices/platform -name "$CHIP_ID" 2>/dev/null | head -n 1)

if [[ -z "$SIM_PATH" ]]; then
    echo -e "${RED}Error: GPIO chip '$CHIP_ID' not found${NC}"
    echo ""
    echo "Available gpio-sim chips:"
    for chip in /dev/gpiochip*; do
        if [[ -c "$chip" ]]; then
            chip_name=$(basename "$chip")
            sim_check=$(find /sys/devices/platform -name "$chip_name" 2>/dev/null | grep gpio-sim | head -n 1)
            if [[ -n "$sim_check" ]]; then
                echo "  - $chip_name"
            fi
        fi
    done
    exit 1
fi

echo -e "${GREEN}✓ Chip found: $CHIP_ID${NC}"
echo "  Path: $SIM_PATH"
echo "  Device: /dev/$CHIP_ID"
echo ""

# 2. Check configfs configuration
if [[ -d "$CONFIGFS_DIR/$DEVICE_NAME" ]]; then
    echo -e "${BLUE}Configfs Configuration:${NC}"
    echo "  Device name: $DEVICE_NAME"
    
    if [[ -f "$CONFIGFS_DIR/$DEVICE_NAME/live" ]]; then
        live_status=$(cat "$CONFIGFS_DIR/$DEVICE_NAME/live" 2>/dev/null)
        if [[ "$live_status" == "1" ]]; then
            echo -e "  Status: ${GREEN}ACTIVE${NC}"
        else
            echo -e "  Status: ${YELLOW}Inactive${NC}"
        fi
    fi
    
    # Show bank configuration
    for bank_dir in "$CONFIGFS_DIR/$DEVICE_NAME"/*; do
        if [[ -d "$bank_dir" ]] && [[ "$(basename "$bank_dir")" != "live" ]]; then
            bank_name=$(basename "$bank_dir")
            echo "  Bank: $bank_name"
            if [[ -f "$bank_dir/num_lines" ]]; then
                num_lines=$(cat "$bank_dir/num_lines" 2>/dev/null)
                echo "    Lines: $num_lines"
            fi
            if [[ -f "$bank_dir/chip_label" ]]; then
                chip_label=$(cat "$bank_dir/chip_label" 2>/dev/null)
                echo "    Label: $chip_label"
            fi
        fi
    done
    echo ""
else
    echo -e "${YELLOW}Warning: Configfs device '$DEVICE_NAME' not found${NC}"
    echo ""
fi

# 3. Show pin states
echo -e "${BLUE}Pin States:${NC}"

# Find all sim_gpio directories
pin_dirs=$(find "$SIM_PATH" -type d -name "sim_gpio*" 2>/dev/null | sort -V)

if [[ -z "$pin_dirs" ]]; then
    echo "  No pins found"
else
    printf "  %-6s %-10s %-10s %-10s\n" "Pin" "Value" "Pull" "Direction"
    echo "  $(printf '%.0s-' {1..40})"
    
    for pin_dir in $pin_dirs; do
        pin_num=$(basename "$pin_dir" | sed 's/sim_gpio//')
        
        # Get value
        value_file="$pin_dir/value"
        if [[ -f "$value_file" ]]; then
            value=$(cat "$value_file" 2>/dev/null | tr -d '\n')
        else
            value="N/A"
        fi
        
        # Get pull
        pull_file="$pin_dir/pull"
        if [[ -f "$pull_file" ]]; then
            pull=$(cat "$pull_file" 2>/dev/null | tr -d '\n')
            case "$pull" in
                0) pull_str="down" ;;
                1) pull_str="up" ;;
                2) pull_str="none" ;;
                *) pull_str="$pull" ;;
            esac
        else
            pull_str="N/A"
        fi
        
        # Get direction (if available)
        direction_file="$pin_dir/direction"
        if [[ -f "$direction_file" ]]; then
            direction=$(cat "$direction_file" 2>/dev/null | tr -d '\n')
        else
            direction="N/A"
        fi
        
        # Color code the value
        if [[ "$value" == "1" ]]; then
            value_display="${GREEN}$value${NC}"
        elif [[ "$value" == "0" ]]; then
            value_display="${RED}$value${NC}"
        else
            value_display="$value"
        fi
        
        # Use printf with %b to interpret escape sequences in the value
        # Note: The width specifier counts escape sequences, so we pad manually if needed
        printf "  %-6s %b %-10s %-10s\n" "$pin_num" "$value_display" "$pull_str" "$direction"
    done
    echo ""
fi
