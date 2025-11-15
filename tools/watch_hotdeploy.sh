#!/bin/bash
# watch_hotdeploy.sh - Watch for file changes and auto hot-deploy

set -e

KERNEL_IMAGE="kernel8.img"
RPI_IP="${RPI_IP:-192.168.10.110}"
RPI_PORT="${RPI_PORT:-8888}"
TOOLS_DIR="$(dirname "$0")"
PROJECT_ROOT="$(dirname "$TOOLS_DIR")"

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

echo -e "${GREEN}IndigoLispOS Hot Deploy Watcher${NC}"
echo "=================================="
echo -e "Target: ${BLUE}$RPI_IP:$RPI_PORT${NC}"
echo -e "Watching: ${BLUE}src-rust/, src-c/, boot/, drivers/, lisp/${NC}"
echo ""
echo "Press Ctrl+C to stop"
echo ""

# Check if inotify-tools is installed
if ! command -v inotifywait &> /dev/null; then
    echo -e "${YELLOW}Warning: inotifywait not found${NC}"
    echo "Install with: sudo apt install inotify-tools"
    echo ""
    echo "Falling back to polling mode..."
    
    # Polling mode fallback
    last_mtime=0
    
    while true; do
        # Check modification time of source directories
        current_mtime=$(find src-rust src-c boot drivers lisp -type f -name "*.rs" -o -name "*.c" -o -name "*.S" 2>/dev/null | xargs stat -c %Y 2>/dev/null | sort -n | tail -1)
        
        if [ "$current_mtime" != "$last_mtime" ] && [ "$last_mtime" != "0" ]; then
            echo -e "${YELLOW}Change detected!${NC}"
            
            # Build
            echo "Building..."
            if make; then
                echo -e "${GREEN}Build successful${NC}"
                
                # Deploy
                echo "Hot deploying..."
                if python3 "$TOOLS_DIR/hotdeploy_send.py" "$KERNEL_IMAGE" "$RPI_IP" "$RPI_PORT"; then
                    echo -e "${GREEN}Hot deploy complete!${NC}"
                else
                    echo -e "${YELLOW}Hot deploy failed${NC}"
                fi
            else
                echo -e "${YELLOW}Build failed${NC}"
            fi
            
            echo ""
        fi
        
        last_mtime=$current_mtime
        sleep 2
    done
else
    # inotifywait mode
    while true; do
        # Wait for file changes
        inotifywait -r -e modify,create,delete \
            --exclude '(\.git|target|build|\.swp)' \
            src-rust/ src-c/ boot/ drivers/ lisp/ 2>/dev/null
        
        echo -e "${YELLOW}Change detected!${NC}"
        
        # Small delay to catch rapid successive changes
        sleep 0.5
        
        # Build
        echo "Building..."
        if make; then
            echo -e "${GREEN}Build successful${NC}"
            
            # Deploy
            echo "Hot deploying..."
            if python3 "$TOOLS_DIR/hotdeploy_send.py" "$KERNEL_IMAGE" "$RPI_IP" "$RPI_PORT"; then
                echo -e "${GREEN}Hot deploy complete!${NC}"
            else
                echo -e "${YELLOW}Hot deploy failed${NC}"
            fi
        else
            echo -e "${YELLOW}Build failed${NC}"
        fi
        
        echo ""
    done
fi
