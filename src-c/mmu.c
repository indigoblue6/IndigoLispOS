// mmu.c - MMU Setup for AArch64 (Raspberry Pi 5 / BCM2712)

typedef unsigned char  uint8_t;
typedef unsigned int   uint32_t;
typedef unsigned long  uint64_t;

// ---- Page table entry attributes ----
#define PTE_VALID           (1UL << 0)
#define PTE_TABLE           (1UL << 1)   // table descriptor
#define PTE_PAGE            (1UL << 1)   // page descriptor
#define PTE_BLOCK           (0UL << 1)   // block descriptor

#define PTE_NS              (1UL << 5)   // Non-secure
#define PTE_AF              (1UL << 10)  // Access flag
#define PTE_nG              (0UL << 11)  // Global

#define PTE_SHARE_OUTER     (2UL << 8)   // Outer shareable
#define PTE_SHARE_INNER     (3UL << 8)   // Inner shareable

// ---- Memory attributes indices (for MAIR_EL1) ----
#define MAIR_IDX_DEVICE_nGnRnE  0  // strongly ordered device
#define MAIR_IDX_DEVICE_nGnRE   1  // device (read gather allowed)
#define MAIR_IDX_NORMAL_NC      2  // normal, non-cacheable
#define MAIR_IDX_NORMAL         3  // normal, WB cacheable

// attr index → PTE bits[4:2]
#define PTE_ATTR(idx)       ((uint64_t)(idx) << 2)

// ---- Access permissions ----
#define PTE_AP_RW_EL1       (0UL << 6)   // RW, EL1 only
#define PTE_AP_RW_ALL       (1UL << 6)   // RW, EL0/EL1
#define PTE_AP_RO_EL1       (2UL << 6)   // RO, EL1 only
#define PTE_AP_RO_ALL       (3UL << 6)   // RO, EL0/EL1

// ---- UXN/PXN ----
#define PTE_UXN             (1UL << 54)
#define PTE_PXN             (1UL << 53)

// 1GB block size at L1
#define L1_BLOCK_SIZE       (1UL << 30)

// ---- Special regions ----

// RP1 outbound window (CPU view) used by PCIe driver
//   CPU: 0x0000_0060_0000_0000 .. ( + 256MiB )
//   L1 index = 0x6000_0000_00 >> 30 = 0x180
#define PCIE_CPU_WINDOW_BASE   0x6000000000UL
#define PCIE_CPU_WINDOW_L1_IDX 0x180

// PCIe outbound window around 0x1F00_0000_00 (diagnostic)
#define PCIE_OUTBOUND_BASE     0x1F00000000UL
#define PCIE_OUTBOUND_L1_IDX   0x7C

// VideoCore property mailbox (BCM2712 / Pi5) – ARM-side physical
//  ※ 今回は 0x1000_00B880 を使用（timed が読めているアドレス）
//     ここを変更すれば、別の VC mailbox 物理アドレスにも対応可。
#define VC_MAILBOX_PHYS        0x100000B880UL

// ---- Page tables (4KB aligned) ----
static uint64_t page_table_l0[512]        __attribute__((aligned(4096)));
static uint64_t page_table_l1[512]        __attribute__((aligned(4096)));
static uint64_t page_table_l2_high[512]   __attribute__((aligned(4096))); // 0xC0000000.. (2MB blocks)
static uint64_t page_table_l2_vc[512]     __attribute__((aligned(4096))); // VC mailbox L2 (1GB region)
static uint64_t page_table_l3_vc[512]     __attribute__((aligned(4096))); // VC mailbox L3 (4KB page)

// ---- UART debug helpers (from kernel.c) ----
extern void uart_puts(const char* s);
extern void uart_puthex32(unsigned int v);
extern void uart_puthex64(unsigned long v);

// ----------------------------------------------------------------------
// MMU init
// ----------------------------------------------------------------------
void mmu_init(void)
{
    uart_puts("mmu: init start\n");

    // ---- Clear page tables ----
    for (int i = 0; i < 512; i++) {
        page_table_l0[i]      = 0;
        page_table_l1[i]      = 0;
        page_table_l2_high[i] = 0;
        page_table_l2_vc[i]   = 0;
        page_table_l3_vc[i]   = 0;
    }
    uart_puts("mmu: page tables cleared\n");

    // ---- Pre-MMU physical probes ----
    {
        volatile uint32_t *p;
        uint32_t v;

        // legacy VC mailbox-ish (ほぼ 0xFFFFFFFF)
        p = (volatile uint32_t *)0xFE00B880UL;
        v = *p;
        uart_puts("mmu: preprobe FE00B880 -> ");
        uart_puthex32(v);
        uart_puts("\n");

        // BCM2712 VC property mailbox候補 (今回使うアドレス)
        p = (volatile uint32_t *)0x100000B880UL;
        v = *p;
        uart_puts("mmu: preprobe 100000B880 -> ");
        uart_puthex32(v);
        uart_puts("\n");

        // 比較用（Circle ログでよく見る 0x107C8000）
        p = (volatile uint32_t *)0x107C8000UL;
        v = *p;
        uart_puts("mmu: preprobe 107C8000 -> ");
        uart_puthex32(v);
        uart_puts("\n");
    }

    // ---- L0: entry 0 only（VA 0x0〜0x1FF_FFFF_FFFF をカバー）----
    page_table_l0[0] = ((uint64_t)page_table_l1) | PTE_VALID | PTE_TABLE;
    uart_puts("mmu: l0[0] set\n");

    // ---- L1: main layout ----

    // 0) 0x00000000 - 0x3FFFFFFF : RAM (Normal, WB)
    page_table_l1[0] =
        (0x00000000UL) |
        PTE_VALID | PTE_BLOCK | PTE_AF |
        PTE_ATTR(MAIR_IDX_NORMAL) |
        PTE_AP_RW_EL1 |
        PTE_SHARE_INNER;
    uart_puts("mmu: l1[0] normal RAM set\n");

    // 1) 0x40000000 - 0x7FFFFFFF : SoC low peripherals (Device)
    page_table_l1[1] =
        (0x40000000UL) |
        PTE_VALID | PTE_BLOCK | PTE_AF |
        PTE_ATTR(MAIR_IDX_DEVICE_nGnRE) |
        PTE_AP_RW_EL1 |
        PTE_PXN | PTE_UXN;
    uart_puts("mmu: l1[1] low-peripherals 1GB device block set\n");

    // 2) RP1 PCIe CPU window 0x6000_0000_00 .. (1GB Device block)
    page_table_l1[PCIE_CPU_WINDOW_L1_IDX] =
        (PCIE_CPU_WINDOW_BASE & 0xFFFFFFFFF0000000UL) |
        PTE_VALID | PTE_BLOCK | PTE_AF |
        PTE_ATTR(MAIR_IDX_DEVICE_nGnRE) |
        PTE_AP_RW_EL1 |
        PTE_SHARE_INNER |
        PTE_PXN | PTE_UXN;
    uart_puts("mmu: l1[0x180] PCIe CPU window mapped\n");

    // 3) High peripherals (0xC0000000..0xFFFFFFFF) via L2, 2MB device blocks
    for (int i = 0; i < 512; i++) {
        uint64_t phys = 0xC0000000UL + ((uint64_t)i << 21); // 2MB steps
        page_table_l2_high[i] =
            phys |
            PTE_VALID | PTE_BLOCK | PTE_AF |
            PTE_ATTR(MAIR_IDX_DEVICE_nGnRE) |
            PTE_AP_RW_EL1 |
            PTE_PXN | PTE_UXN;
    }
    page_table_l1[3] = ((uint64_t)page_table_l2_high) | PTE_VALID | PTE_TABLE;
    uart_puts("mmu: l1[3] -> high-peripherals L2\n");

    // FE00_0000 の L2 index をログ
    {
        int fe_idx = (int)((0xFE000000UL >> 21) & 0x1FF);
        uart_puts("mmu: FE00_0000 L2 idx=");
        uart_puthex32(fe_idx);
        uart_puts("\n");
        uart_puts("  l2_high[");
        uart_puthex32(fe_idx);
        uart_puts("]=");
        uart_puthex64(page_table_l2_high[fe_idx]);
        uart_puts("\n");
    }

    // 4) PCIe outbound window around 0x1F00_0000_00 (diagnostic)
    page_table_l1[PCIE_OUTBOUND_L1_IDX] =
        (PCIE_OUTBOUND_BASE & 0xFFFFFFFFF0000000UL) |
        PTE_VALID | PTE_BLOCK | PTE_AF |
        PTE_ATTR(MAIR_IDX_DEVICE_nGnRE) |
        PTE_AP_RW_EL1 |
        PTE_SHARE_INNER |
        PTE_PXN | PTE_UXN;

    // 5) VC mailbox: 0x100000B880 を 4KB Device page で張る
    //    ここは 1GB ブロックではなく L2/L3 を使う。
    {
        // index 計算（L1: bits[38:30], L2: bits[29:21], L3: bits[20:12]）
        const uint64_t phys = VC_MAILBOX_PHYS;
        const int vc_l1_idx = (int)((phys >> 30) & 0x1FF); // 1GB slot
        const int vc_l2_idx = (int)((phys >> 21) & 0x1FF); // 2MB slot
        const int vc_l3_idx = (int)((phys >> 12) & 0x1FF); // 4KB slot

        // 念のため、L1[vc_l1_idx] の 1GB block を潰すログ
        if (page_table_l1[vc_l1_idx] != 0) {
            uart_puts("mmu: l1[");
            uart_puthex32(vc_l1_idx);
            uart_puts("] 1GB block REMOVED for VC mailbox table\n");
        }

        // L3: VC mailbox page
        page_table_l3_vc[vc_l3_idx] =
            (phys & ~0xFFFUL) |
            PTE_VALID | PTE_PAGE | PTE_AF |
            PTE_ATTR(MAIR_IDX_DEVICE_nGnRnE) |
            PTE_AP_RW_EL1 |
            PTE_PXN | PTE_UXN;

        // L2: その slot だけ L3 を指す
        page_table_l2_vc[vc_l2_idx] =
            ((uint64_t)page_table_l3_vc) |
            PTE_VALID | PTE_TABLE;

        // L1: 1 エントリで VC mailbox 用の L2 を指す
        page_table_l1[vc_l1_idx] =
            ((uint64_t)page_table_l2_vc) |
            PTE_VALID | PTE_TABLE;

        uart_puts("mmu: VC mailbox mapping\n");
        uart_puts("  phys = 0x");
        uart_puthex64(phys);
        uart_puts("\n  L1 idx = 0x");
        uart_puthex32(vc_l1_idx);
        uart_puts(" L2 idx = 0x");
        uart_puthex32(vc_l2_idx);
        uart_puts(" L3 idx = 0x");
        uart_puthex32(vc_l3_idx);
        uart_puts("\n");
    }

    // ---- MAIR_EL1 ----
    uint64_t mair = 0;
    mair |= (0x00UL << (MAIR_IDX_DEVICE_nGnRnE * 8)); // Device-nGnRnE
    mair |= (0x04UL << (MAIR_IDX_DEVICE_nGnRE * 8));  // Device-nGnRE
    mair |= (0x44UL << (MAIR_IDX_NORMAL_NC * 8));     // Normal non-cache
    mair |= (0xFFUL << (MAIR_IDX_NORMAL     * 8));    // Normal WB
    __asm__ volatile("msr mair_el1, %0" :: "r"(mair));
    uart_puts("mmu: MAIR set\n");
    {
        uint64_t r;
        __asm__ volatile("mrs %0, mair_el1" : "=r"(r));
        uart_puts("mmu: MAIR_EL1=");
        uart_puthex64(r);
        uart_puts("\n");
    }

    // ---- TCR_EL1 ----
    // 4KB granule, 48-bit VA, 48-bit PA（IPS=5）
    uint64_t tcr = 0;

    // T0 (TTBR0_EL1)
    tcr |= (16UL << 0);   // T0SZ = 16 (48-bit VA)
    tcr |= (0UL  << 6);   // reserved
    tcr |= (0UL  << 7);   // EPD0 = 0
    tcr |= (3UL  << 8);   // IRGN0 = 3 (WB, write-allocate)
    tcr |= (3UL  << 10);  // ORGN0 = 3
    tcr |= (3UL  << 12);  // SH0 = 3 (inner shareable)
    tcr |= (2UL  << 14);  // TG0 = 2 (4KB)

    // T1 (TTBR1_EL1) – 使っていないが一応 valid な設定に
    tcr |= (16UL << 16);  // T1SZ = 16
    tcr |= (0UL  << 22);  // A1 = 0
    tcr |= (0UL  << 23);  // EPD1 = 0
    tcr |= (3UL  << 24);  // IRGN1 = 3
    tcr |= (3UL  << 26);  // ORGN1 = 3
    tcr |= (3UL  << 28);  // SH1 = 3
    tcr |= (2UL  << 30);  // TG1 = 2 (4KB)

    // IPS: 48-bit PA
    tcr |= (5UL << 32);   // IPS = 5

    __asm__ volatile("msr tcr_el1, %0" :: "r"(tcr));
    uart_puts("mmu: TCR set\n");
    {
        uint64_t r;
        __asm__ volatile("mrs %0, tcr_el1" : "=r"(r));
        uart_puts("mmu: TCR_EL1=");
        uart_puthex64(r);
        uart_puts("\n");
    }

    // ---- TTBR0_EL1 ----
    __asm__ volatile("msr ttbr0_el1, %0" :: "r"((uint64_t)page_table_l0));
    uart_puts("mmu: TTBR0 set\n");

    // ---- Barrier ----
    __asm__ volatile("dsb sy");
    __asm__ volatile("isb");

    // ---- SCTLR_EL1 : enable MMU + caches ----
    uint64_t sctlr;
    __asm__ volatile("mrs %0, sctlr_el1" : "=r"(sctlr));
    sctlr |= (1UL << 0);   // M : MMU enable
    sctlr |= (1UL << 2);   // C : data cache
    sctlr |= (1UL << 12);  // I : instruction cache
    __asm__ volatile("msr sctlr_el1, %0" :: "r"(sctlr));
    uart_puts("mmu: SCTLR (MMU enabled)\n");

    __asm__ volatile("dsb sy");
    __asm__ volatile("isb");

    // ---- Post-MMU probes ----
    uart_puts("mmu: post-MMU probes\n");
    {
        volatile uint32_t *addr;
        uint32_t v;

        addr = (volatile uint32_t *)0xFE00B880UL;
        v = *addr;
        uart_puts("FE00B880 -> ");
        uart_puthex32(v);
        uart_puts("\n");

        addr = (volatile uint32_t *)0x100000B880UL;
        v = *addr;
        uart_puts("100000B880 -> ");
        uart_puthex32(v);
        uart_puts("\n");

        addr = (volatile uint32_t *)0x107C8000UL;
        v = *addr;
        uart_puts("107C8000 -> ");
        uart_puthex32(v);
        uart_puts("\n");
    }
}
