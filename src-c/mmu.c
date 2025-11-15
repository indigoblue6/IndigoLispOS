// mmu.c - MMU Setup for AArch64 (Raspberry Pi 5)

// Type definitions (replacing stdint.h)
typedef unsigned char uint8_t;
typedef unsigned int uint32_t;
typedef unsigned long uint64_t;

// Page table entry attributes
#define PTE_VALID           (1UL << 0)
#define PTE_TABLE           (1UL << 1)
#define PTE_PAGE            (1UL << 1)
#define PTE_BLOCK           (0UL << 1)
#define PTE_NS              (1UL << 5)   // Non-secure
#define PTE_AF              (1UL << 10)  // Access flag
#define PTE_nG              (0UL << 11)  // Global
#define PTE_SHARE_OUTER     (2UL << 8)   // Outer shareable
#define PTE_SHARE_INNER     (3UL << 8)   // Inner shareable

// Memory attributes indices (for MAIR_EL1)
#define MAIR_IDX_DEVICE_nGnRnE  0  // Device memory
#define MAIR_IDX_DEVICE_nGnRE   1  // Device memory (read gathering)
#define MAIR_IDX_NORMAL_NC      2  // Normal memory, non-cacheable
#define MAIR_IDX_NORMAL         3  // Normal memory, write-back cacheable

// Attribute index to PTE bits
#define PTE_ATTR(idx)       ((uint64_t)(idx) << 2)

// Access permissions
#define PTE_AP_RW_EL1       (0UL << 6)   // Read/Write, EL1 only
#define PTE_AP_RW_ALL       (1UL << 6)   // Read/Write, all ELs
#define PTE_AP_RO_EL1       (2UL << 6)   // Read-only, EL1 only
#define PTE_AP_RO_ALL       (3UL << 6)   // Read-only, all ELs

// UXN/PXN bits
#define PTE_UXN             (1UL << 54)  // Unprivileged execute never
#define PTE_PXN             (1UL << 53)  // Privileged execute never

// Page table (aligned to 4KB)
static uint64_t page_table_l0[512] __attribute__((aligned(4096)));
static uint64_t page_table_l1_low[512] __attribute__((aligned(4096)));
static uint64_t page_table_l1_high[512] __attribute__((aligned(4096)));

void mmu_init(void) {
    // Initialize page tables to zero
    for (int i = 0; i < 512; i++) {
        page_table_l0[i] = 0;
        page_table_l1_low[i] = 0;
        page_table_l1_high[i] = 0;
    }
    
    // L0 entry 0: Points to L1 table for addresses 0x0000_0000_0000_0000 - 0x0000_01FF_FFFF_FFFF (512GB)
    page_table_l0[0] = ((uint64_t)page_table_l1_low) | PTE_VALID | PTE_TABLE;
    
    // L0 entry for 0x1F00000000 region (RP1 PCIe outbound window)
    // Address 0x1F00000000 = bits [47:39] = 0xF8 >> 3 = entry 0x1F
    page_table_l0[0x1F] = ((uint64_t)page_table_l1_high) | PTE_VALID | PTE_TABLE;
    
    // L1 entries for low memory (0-4GB): Identity map with 1GB blocks
    // Entry 0: 0x00000000 - 0x3FFFFFFF (1GB) - Normal memory for RAM
    page_table_l1_low[0] = 0x00000000UL | PTE_VALID | PTE_BLOCK | PTE_AF |
                           PTE_ATTR(MAIR_IDX_NORMAL) | PTE_AP_RW_EL1 |
                           PTE_SHARE_INNER;
    
    // Entry 1: 0x40000000 - 0x7FFFFFFF (1GB) - Device memory for peripherals
    page_table_l1_low[1] = 0x40000000UL | PTE_VALID | PTE_BLOCK | PTE_AF |
                           PTE_ATTR(MAIR_IDX_DEVICE_nGnRnE) | PTE_AP_RW_EL1 |
                           PTE_PXN | PTE_UXN;
    
    // Entry 4: 0x100000000 - 0x13FFFFFFF (1GB) - Device memory for BCM2712 peripherals
    page_table_l1_low[4] = 0x100000000UL | PTE_VALID | PTE_BLOCK | PTE_AF |
                           PTE_ATTR(MAIR_IDX_DEVICE_nGnRnE) | PTE_AP_RW_EL1 |
                           PTE_PXN | PTE_UXN;
    
    // L1 entries for high memory (0x1F00000000 region): RP1 PCIe outbound window
    // Entry 0: 0x1F00000000 - 0x1F3FFFFFFF (1GB) - Device memory for RP1
    page_table_l1_high[0] = 0x1F00000000UL | PTE_VALID | PTE_BLOCK | PTE_AF |
                            PTE_ATTR(MAIR_IDX_DEVICE_nGnRnE) | PTE_AP_RW_EL1 |
                            PTE_PXN | PTE_UXN;
    
    // Configure MAIR_EL1 (Memory Attribute Indirection Register)
    uint64_t mair = 0;
    mair |= (0x00UL << (MAIR_IDX_DEVICE_nGnRnE * 8)); // Device-nGnRnE
    mair |= (0x04UL << (MAIR_IDX_DEVICE_nGnRE * 8));  // Device-nGnRE
    mair |= (0x44UL << (MAIR_IDX_NORMAL_NC * 8));     // Normal, non-cacheable
    mair |= (0xFFUL << (MAIR_IDX_NORMAL * 8));        // Normal, write-back cacheable
    
    __asm__ volatile("msr mair_el1, %0" : : "r"(mair));
    
    // Configure TCR_EL1 (Translation Control Register)
    uint64_t tcr = 0;
    tcr |= (16UL << 0);   // T0SZ = 16 (48-bit address space)
    tcr |= (0UL << 6);    // Reserved
    tcr |= (0UL << 7);    // EPD0 = 0 (enable TTBR0_EL1)
    tcr |= (3UL << 8);    // IRGN0 = 3 (inner write-back cacheable)
    tcr |= (3UL << 10);   // ORGN0 = 3 (outer write-back cacheable)
    tcr |= (3UL << 12);   // SH0 = 3 (inner shareable)
    tcr |= (2UL << 14);   // TG0 = 2 (4KB granule)
    tcr |= (16UL << 16);  // T1SZ = 16
    tcr |= (0UL << 22);   // A1 = 0 (TTBR0 defines ASID)
    tcr |= (0UL << 23);   // EPD1 = 0
    tcr |= (3UL << 24);   // IRGN1 = 3
    tcr |= (3UL << 26);   // ORGN1 = 3
    tcr |= (3UL << 28);   // SH1 = 3
    tcr |= (2UL << 30);   // TG1 = 2
    tcr |= (0UL << 32);   // IPS = 0 (32-bit physical address)
    
    __asm__ volatile("msr tcr_el1, %0" : : "r"(tcr));
    
    // Set TTBR0_EL1 (Translation Table Base Register)
    __asm__ volatile("msr ttbr0_el1, %0" : : "r"((uint64_t)page_table_l0));
    
    // Ensure all writes complete
    __asm__ volatile("dsb sy");
    __asm__ volatile("isb");
    
    // Enable MMU in SCTLR_EL1
    uint64_t sctlr;
    __asm__ volatile("mrs %0, sctlr_el1" : "=r"(sctlr));
    sctlr |= (1UL << 0);  // M bit: Enable MMU
    sctlr |= (1UL << 2);  // C bit: Enable data cache
    sctlr |= (1UL << 12); // I bit: Enable instruction cache
    __asm__ volatile("msr sctlr_el1, %0" : : "r"(sctlr));
    
    __asm__ volatile("dsb sy");
    __asm__ volatile("isb");
}
