#!/bin/bash
# deploy.sh - Deploy kernel to Raspberry Pi SD card

set -e

KERNEL_IMAGE="kernel8.img"
SD_MOUNT="${SD_MOUNT:-}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${GREEN}IndigoLispOS Deployment Script${NC}"
echo "================================"

# Check if kernel image exists
if [ ! -f "$KERNEL_IMAGE" ]; then
    echo -e "${RED}Error: $KERNEL_IMAGE not found${NC}"
    echo "Please run 'make' first to build the kernel"
    exit 1
fi

# Check if SD_MOUNT is set
if [ -z "$SD_MOUNT" ]; then
    echo -e "${YELLOW}SD card mount point not specified${NC}"
    echo ""
    echo "Please set SD_MOUNT environment variable:"
    echo "  export SD_MOUNT=/path/to/sd/card"
    echo "  ./deploy.sh"
    echo ""
    echo "Or specify directly:"
    echo "  SD_MOUNT=/media/user/boot ./deploy.sh"
    exit 1
fi

# Check if mount point exists
if [ ! -d "$SD_MOUNT" ]; then
    echo -e "${RED}Error: Directory $SD_MOUNT does not exist${NC}"
    exit 1
fi

# Check if we have write permission
if [ ! -w "$SD_MOUNT" ]; then
    echo -e "${RED}Error: No write permission for $SD_MOUNT${NC}"
    echo "You may need to run with sudo"
    exit 1
fi

# Show kernel info
KERNEL_SIZE=$(stat -f%z "$KERNEL_IMAGE" 2>/dev/null || stat -c%s "$KERNEL_IMAGE")
echo "Kernel image: $KERNEL_IMAGE"
echo "Size: $KERNEL_SIZE bytes"
echo "Destination: $SD_MOUNT"
echo ""

# Confirm deployment
read -p "Deploy kernel to SD card? (y/N) " -n 1 -r
echo
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo "Deployment cancelled"
    exit 0
fi

# Copy kernel
echo -e "${GREEN}Copying kernel...${NC}"
cp -v "$KERNEL_IMAGE" "$SD_MOUNT/"

# Sync to ensure write
echo -e "${GREEN}Syncing...${NC}"
sync

echo ""
echo -e "${GREEN}✓ Deployment successful!${NC}"
echo ""
echo "Next steps:"
echo "1. Safely eject the SD card"
echo "2. Insert into Raspberry Pi 5"
echo "3. Connect UART serial (115200 baud)"
echo "4. Power on"
echo ""
echo "Expected output:"
echo "  IndigoLispOS v0.2"
echo "  Initializing..."
echo "  Welcome to IndigoLispOS REPL"
echo "  > "
