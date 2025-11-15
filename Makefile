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

# Output
KERNEL = kernel8.img
ELF = $(BUILD_DIR)/kernel8.elf

# Source files
ASM_SOURCES = $(BOOT_DIR)/boot.S
C_SOURCES = $(SRC_C_DIR)/kernel.c $(SRC_C_DIR)/uart.c $(SRC_C_DIR)/mmu.c
RUST_LIB = $(RUST_DIR)/target/aarch64-unknown-none/release/libindigo_lisp_os.a

# Object files
ASM_OBJECTS = $(patsubst $(BOOT_DIR)/%.S,$(BUILD_DIR)/%.o,$(ASM_SOURCES))
C_OBJECTS = $(patsubst $(SRC_C_DIR)/%.c,$(BUILD_DIR)/%.o,$(C_SOURCES))

.PHONY: all clean rust qemu deploy

all: $(KERNEL)

# Build Rust library
rust:
	cd $(RUST_DIR) && $(RUSTC) build --release --target aarch64-unknown-none

# Compile assembly
$(BUILD_DIR)/%.o: $(BOOT_DIR)/%.S
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

# Run in QEMU
qemu: $(KERNEL)
	qemu-system-aarch64 \
		-M raspi3b \
		-kernel $(KERNEL) \
		-serial stdio \
		-nographic

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

# Clean build artifacts
clean:
	rm -rf $(BUILD_DIR) $(KERNEL)
	cd $(RUST_DIR) && cargo clean

# Help
help:
	@echo "IndigoLispOS Build System"
	@echo ""
	@echo "Targets:"
	@echo "  all     - Build kernel8.img (default)"
	@echo "  rust    - Build Rust components only"
	@echo "  qemu    - Run in QEMU emulator"
	@echo "  deploy  - Deploy to SD card (set SD_MOUNT=/path/to/sd)"
	@echo "  clean   - Remove all build artifacts"
	@echo "  help    - Show this help message"
