#!/bin/bash

# Helper script to test PipeWire audio connections

set -e

echo "=== PipeWire Connection Test Helper ==="
echo

# Check if pipewire tools are available
if ! command -v pw-cli &> /dev/null; then
    echo "⚠️  Warning: pw-cli not found. Install pipewire-utils for device listing."
    echo
fi

# Function to list devices
list_devices() {
    echo "Available PipeWire Audio Nodes:"
    echo "================================"
    echo
    
    if command -v pw-dump &> /dev/null; then
        echo "All Audio Nodes:"
        echo "----------------"
        pw-dump | jq -r '.[] | select(.type == "PipeWire:Interface:Node" and .info.props."media.class" != null) | "\(.info.props."node.name") [\(.info.props."media.class")]"' 2>/dev/null | sort
        echo
    elif command -v pw-cli &> /dev/null; then
        echo "All Audio Nodes:"
        echo "----------------"
        pw-cli list-objects Node 2>/dev/null | awk '/node.name =/{gsub(/.*= "|"/, ""); print}'
        echo
    else
        echo "Install pipewire-utils to see device listing:"
        echo "  Ubuntu/Debian: sudo apt install pipewire-utils"
        echo "  Fedora: sudo dnf install pipewire-utils"
        echo "  Arch: sudo pacman -S pipewire-utils"
    fi
    echo
    echo "Tip: Use the exact node.name value with the test tool"
    echo
}

# Function to run test
run_test() {
    local device="$1"
    
    echo "Building test application..."
    cargo build --bin pipewire_test --release
    echo
    
    if [ -z "$device" ]; then
        echo "Running test with DEFAULT device..."
        cargo run --bin pipewire_test --release
    else
        echo "Running test with device: $device"
        cargo run --bin pipewire_test --release -- "$device"
    fi
}

# Parse command line arguments
case "$1" in
    list|--list|-l)
        list_devices
        ;;
    help|--help|-h)
        echo "Usage:"
        echo "  $0                          # Run test with default device"
        echo "  $0 [device_name]            # Run test with specific device"
        echo "  $0 list                     # List available devices"
        echo "  $0 help                     # Show this help"
        echo
        echo "Examples:"
        echo "  $0 list"
        echo "  $0 'alsa_output.pci-0000_00_1f.3.analog-stereo.monitor'"
        echo "  $0"
        echo
        ;;
    *)
        run_test "$1"
        ;;
esac
