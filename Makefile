# IndigoLispOS Makefile
# Target: Raspberry Pi 5 (AArch64)

# Tools
CC = aarch64-linux-gnu-gcc
LD = aarch64-linux-gnu-ld
OBJCOPY = aarch64-linux-gnu-objcopy
RUSTC = cargo

# Directories
BUILD_DIR = build
BOOT_DIR = boot
SRC_C_DIR = src-c
ARCH_DIR = arch/aarch64
RUST_DIR = src-rust

# Flags
CFLAGS = -Wall -O2 -ffreestanding -nostdinc -nostdlib -nostartfiles
LDFLAGS = -nostdlib

# Hot deploy config (can be overridden)
RPI_IP ?= 192.168.10.110
RPI_PORT ?= 8888

# Output
KERNEL = kernel8.img
ELF = $(BUILD_DIR)/kernel8.elf

# Source files
ASM_SOURCES = $(BOOT_DIR)/boot.S $(ARCH_DIR)/interrupts.S
C_SOURCES = $(SRC_C_DIR)/kernel.c $(SRC_C_DIR)/uart.c $(SRC_C_DIR)/mmu.c
RUST_LIB = $(RUST_DIR)/target/aarch64-unknown-none/release/libindigo_lisp_os.a

# Object files
ASM_OBJECTS = $(BUILD_DIR)/boot.o $(BUILD_DIR)/interrupts.o
C_OBJECTS = $(patsubst $(SRC_C_DIR)/%.c,$(BUILD_DIR)/%.o,$(C_SOURCES))

.PHONY: all clean rust deploy hotdeploy watch-hotdeploy

all: $(KERNEL)

# Build Rust library
rust:
	BUILD_TIMESTAMP="$$(date '+%Y-%m-%d %H:%M:%S')" && \
	cd $(RUST_DIR) && BUILD_TIMESTAMP="$$BUILD_TIMESTAMP" $(RUSTC) build --release --target aarch64-unknown-none

# Compile assembly from boot dir
$(BUILD_DIR)/boot.o: $(BOOT_DIR)/boot.S
	@mkdir -p $(BUILD_DIR)
	$(CC) $(CFLAGS) -c $< -o $@

# Compile assembly from arch dir
$(BUILD_DIR)/interrupts.o: $(ARCH_DIR)/interrupts.S
	@mkdir -p $(BUILD_DIR)
	$(CC) $(CFLAGS) -c $< -o $@

# Compile C sources
$(BUILD_DIR)/%.o: $(SRC_C_DIR)/%.c
	@mkdir -p $(BUILD_DIR)
	$(CC) $(CFLAGS) -c $< -o $@

# Link everything
$(ELF): $(ASM_OBJECTS) $(C_OBJECTS) rust
	$(LD) -T $(ARCH_DIR)/linker.ld $(LDFLAGS) $(ASM_OBJECTS) $(C_OBJECTS) $(RUST_LIB) -o $@

# Create kernel image
$(KERNEL): $(ELF)
	$(OBJCOPY) -O binary $< $@
	@echo "✓ Built $(KERNEL)"

# Deploy to SD card (adjust path as needed)
deploy: $(KERNEL)
	@echo "Deploying to SD card..."
	@if [ -z "$(SD_MOUNT)" ]; then \
		echo "Error: Set SD_MOUNT variable to your SD card mount point"; \
		exit 1; \
	fi
	cp $(KERNEL) $(SD_MOUNT)/
	sync
	@echo "✓ Deployed to $(SD_MOUNT)"

# Hot deploy over network
hotdeploy: $(KERNEL)
	@echo "Hot deploying kernel..."
	@python3 tools/hotdeploy_send.py $(KERNEL) $(RPI_IP) $(RPI_PORT)

# Watch for changes and auto hot-deploy
watch-hotdeploy:
	@./tools/watch_hotdeploy.sh

# Clean build artifacts
clean:
	rm -rf $(BUILD_DIR) $(KERNEL)
	cd $(RUST_DIR) && cargo clean

# Help
help:
	@echo "IndigoLispOS Build System"
	@echo ""
	@echo "Targets:"
	@echo "  all            - Build kernel8.img (default)"
	@echo "  rust           - Build Rust components only"
	@echo "  deploy         - Deploy to SD card (set SD_MOUNT=/path/to/sd)"
	@echo "  hotdeploy      - Hot deploy over network (set RPI_IP=192.168.1.100)"
	@echo "  watch-hotdeploy - Watch files and auto hot-deploy on changes"
	@echo "  clean          - Remove all build artifacts"
	@echo "  help           - Show this help message"
	@echo ""
	@echo "Hot Deploy Usage:"
	@echo "  make hotdeploy RPI_IP=192.168.1.100 RPI_PORT=8888"
	@echo "  make watch-hotdeploy"
