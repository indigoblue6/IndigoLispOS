// mmu.c - AArch64 MMU Setup for Raspberry Pi 5 (BCM2712)
//
// ポイント:
//  - 4KB granule, 48-bit VA, 40-bit PA (IPS=2)
//  - VA = PA のアイデンティティマッピング
//  - RAM(0x0..0x3FFFFFFF) は Normal WB, 内部の 0x0100_0000..0x01FF_FFFF だけ Device
//  - SoC low peripherals: 0x4000_0000..0x7FFF_FFFF -> Device
//  - RP1 outbound: 0x1F00_0000_00.. (4x1GB) -> Device
//  - PCIe CPU window: 0x6000_0000_00.. (1GB) -> Device
//  - VC mailbox: 0x1000_00B880 -> L3 4KB Device-nGnRnE page

typedef unsigned char      u8;
typedef unsigned short     u16;
typedef unsigned int       u32;
typedef unsigned long long u64;



// ---- Page table entry attributes ----
#define PTE_VALID           (1ULL << 0)
#define PTE_TABLE           (1ULL << 1)   // table descriptor
#define PTE_PAGE            (1ULL << 1)   // page descriptor
#define PTE_BLOCK           (0ULL << 1)   // block descriptor

#define PTE_NS              (1ULL << 5)   // Non-secure
#define PTE_AF              (1ULL << 10)  // Access flag
#define PTE_nG              (0ULL << 11)  // Global

#define PTE_SHARE_OUTER     (2ULL << 8)   // Outer shareable
#define PTE_SHARE_INNER     (3ULL << 8)   // Inner shareable

// ---- Memory attributes indices (for MAIR_EL1) ----
#define MAIR_IDX_DEVICE_nGnRnE  0  // strongly-ordered device
#define MAIR_IDX_DEVICE_nGnRE   1  // device (read-gather allowed)
#define MAIR_IDX_NORMAL_NC      2  // normal, non-cacheable
#define MAIR_IDX_NORMAL         3  // normal, WB cacheable

// attr index → PTE bits[4:2]
#define PTE_ATTR(idx)       ((u64)(idx) << 2)

// ---- Access permissions ----
#define PTE_AP_RW_EL1       (0ULL << 6)   // RW, EL1 only
#define PTE_AP_RW_ALL       (1ULL << 6)   // RW, EL0/EL1
#define PTE_AP_RO_EL1       (2ULL << 6)   // RO, EL1 only
#define PTE_AP_RO_ALL       (3ULL << 6)   // RO, EL0/EL1

// ---- UXN/PXN ----
#define PTE_UXN             (1ULL << 54)
#define PTE_PXN             (1ULL << 53)

// ---- L1 block size (1GB) & L2 block size (2MB) ----
#define L1_BLOCK_SIZE       (1ULL << 30)
#define L2_BLOCK_SIZE       (1ULL << 21)

// ---- Special regions -------------------------------------------------

// RP1 outbound window (CPU view) used by PCIe driver
//   CPU: 0x0000_0060_0000_0000 .. ( + 1GB )
//   L1 index = 0x6000_0000_00 >> 30 = 0x180
#define PCIE_CPU_WINDOW_BASE   0x6000000000ULL
#define PCIE_CPU_WINDOW_L1_IDX 0x180

// PCIe outbound window around 0x1F00_0000_00 (diagnostic / RP1 peripherals)
#define PCIE_OUTBOUND_BASE     0x1F00000000ULL
#define PCIE_OUTBOUND_L1_IDX   0x7C

// VideoCore property mailbox (BCM2712 / Pi5) – ARM-side physical
#define VC_MAILBOX_PHYS        0x100000B880ULL

// ---- Page tables (4KB aligned) ---------------------------------------

static u64 page_table_l0[512]        __attribute__((aligned(4096)));
static u64 page_table_l1[512]        __attribute__((aligned(4096)));
static u64 page_table_l2_low[512]    __attribute__((aligned(4096))); // 0x00000000..0x3FFFFFFF
static u64 page_table_l2_ecam[512]   __attribute__((aligned(4096))); // 0x100000000..0x13FFFFFFF (for ECAM)
static u64 page_table_l2_high[512]   __attribute__((aligned(4096))); // 0xC0000000..0xFFFF_FFFF
static u64 page_table_l2_vc[512]     __attribute__((aligned(4096))); // VC mailbox L2
static u64 page_table_l3_vc[512]     __attribute__((aligned(4096))); // VC mailbox L3

// ---- UART debug helpers (from kernel.c) ----
extern void uart_puts(const char* s);
extern void uart_puthex32(unsigned int v);
extern void uart_puthex64(unsigned long v);

// ======================================================================
//  Helper: map L1 block (1GB)
// ======================================================================

static void map_l1_block(u32 l1_idx, u64 phys_base, u64 attr, u64 xn)
{
    page_table_l1[l1_idx] =
        (phys_base & 0xFFFFFFFFF0000000ULL) |
        PTE_VALID | PTE_BLOCK | PTE_AF |
        attr |
        PTE_AP_RW_EL1 |
        PTE_SHARE_INNER |
        xn;
}

// ======================================================================
//  MMU init
// ======================================================================

void mmu_init(void)
{
    uart_puts("mmu: init start\n");

    // ---- Clear page tables ----
    for (int i = 0; i < 512; i++) {
        page_table_l0[i]      = 0;
        page_table_l1[i]      = 0;
        page_table_l2_low[i]  = 0;
        page_table_l2_ecam[i] = 0;
        page_table_l2_high[i] = 0;
        page_table_l2_vc[i]   = 0;
        page_table_l3_vc[i]   = 0;
    }
    uart_puts("mmu: page tables cleared\n");

    // ---- Pre-MMU physical probes (for sanity) ----
    {
        volatile u32 *p;
        u32 v;

        p = (volatile u32 *)0xFE00B880ULL;
        v = *p;
        uart_puts("mmu: preprobe FE00B880 -> ");
        uart_puthex32(v);
        uart_puts("\n");

        p = (volatile u32 *)0x100000B880ULL;
        v = *p;
        uart_puts("mmu: preprobe 100000B880 -> ");
        uart_puthex32(v);
        uart_puts("\n");

        p = (volatile u32 *)0x107C8000ULL;
        v = *p;
        uart_puts("mmu: preprobe 107C8000 -> ");
        uart_puthex32(v);
        uart_puts("\n");
    }

    // ---- L0: entry 0 only（VA 0x0〜0x1FF_FFFF_FFFF をカバー）----
    page_table_l0[0] = ((u64)page_table_l1) | PTE_VALID | PTE_TABLE;
    uart_puts("mmu: l0[0] set\n");

    // ==================================================================
    //  L1[0] → L2_low (0x00000000..0x3FFFFFFF)
    //  - 基本は Normal WB, Inner Shareable
    //  - ただし 0x0100_0000..0x01FF_FFFF は Device (GIC-400, local peripherals)
    //  - PCIe ECAM: 0x1060_0000..0x107F_FFFF (2MB block containing 0x107C0000)
    // ==================================================================

    for (int i = 0; i < 512; i++) {
        u64 phys = (u64)i * L2_BLOCK_SIZE;  // 2MB step
        u64 attr = PTE_ATTR(MAIR_IDX_NORMAL);
        u64 xn   = 0;

        // 0x0100_0000 (16MB) .. 0x0200_0000 (32MB) → Device
        // i = 0x0100_0000 / 2MB = 8 ~ 15
        if (i >= 8 && i < 16) {
            attr = PTE_ATTR(MAIR_IDX_DEVICE_nGnRE);
            xn   = PTE_PXN | PTE_UXN;
        }
        
        // PCIe ECAM: 0x107C0000..0x107CFFFF (64KB)
        // 2MB-aligned block: 0x10600000..0x107FFFFF
        // L2 index = 0x10600000 / 0x200000 = 0x83
        if (i == 0x83) {
            attr = PTE_ATTR(MAIR_IDX_DEVICE_nGnRnE);  // Strongly ordered
            xn   = PTE_PXN | PTE_UXN;
        }
        
        page_table_l2_low[i] =
            phys |
            PTE_VALID | PTE_BLOCK | PTE_AF |
            attr |
            PTE_AP_RW_EL1 |
            PTE_SHARE_INNER |
            xn;
    }

    page_table_l1[0] = ((u64)page_table_l2_low) | PTE_VALID | PTE_TABLE;
    uart_puts("mmu: l1[0] -> low-mem L2 (RAM + GIC/device + ECAM)\n");

    // ==================================================================
    //  L1[1] : 0x4000_0000..0x7FFF_FFFF
    //  - SoC low peripherals (Device)
    // ==================================================================
    map_l1_block(
        1,
        0x40000000ULL,
        PTE_ATTR(MAIR_IDX_DEVICE_nGnRE),
        PTE_PXN | PTE_UXN
    );
    uart_puts("mmu: l1[1] low-peripherals 1GB device block set\n");

    // ==================================================================
    //  RP1 PCIe CPU window: 0x6000_0000_00.. (1GB Device)
    // ==================================================================
    map_l1_block(
        PCIE_CPU_WINDOW_L1_IDX,
        PCIE_CPU_WINDOW_BASE,
        PTE_ATTR(MAIR_IDX_DEVICE_nGnRE),
        PTE_PXN | PTE_UXN
    );
    uart_puts("mmu: l1[0x180] PCIe CPU window mapped\n");

    // ==================================================================
    //  High peripherals: 0xC000_0000..0xFFFF_FFFF → L2_high, 全部 Device
    // ==================================================================
    for (int i = 0; i < 512; i++) {
        u64 phys = 0xC0000000ULL + ((u64)i * L2_BLOCK_SIZE);
        page_table_l2_high[i] =
            phys |
            PTE_VALID | PTE_BLOCK | PTE_AF |
            PTE_ATTR(MAIR_IDX_DEVICE_nGnRE) |
            PTE_AP_RW_EL1 |
            PTE_PXN | PTE_UXN;
    }

    page_table_l1[3] = ((u64)page_table_l2_high) | PTE_VALID | PTE_TABLE;
    uart_puts("mmu: l1[3] -> high-peripherals L2 (Device)\n");

    {
        int fe_idx = (int)((0xFE000000ULL >> 21) & 0x1FF);
        uart_puts("mmu: FE00_0000 L2 idx=");
        uart_puthex32(fe_idx);
        uart_puts("\n  l2_high[");
        uart_puthex32(fe_idx);
        uart_puts("]=");
        uart_puthex64(page_table_l2_high[fe_idx]);
        uart_puts("\n");
    }

    // ==================================================================
    //  RP1 outbound window: 0x1F00_0000_00.. (4x1GB, identity)
    //  - Circle 同様に RP1 ペリフェラルを直叩きできるようにする
    // ==================================================================
    for (int i = 0; i < 4; i++) {
        u64 base = PCIE_OUTBOUND_BASE + ((u64)i << 30);  // 1GB steps
        int l1_idx = PCIE_OUTBOUND_L1_IDX + i;

        map_l1_block(
            (u32)l1_idx,
            base,
            PTE_ATTR(MAIR_IDX_DEVICE_nGnRE),
            PTE_PXN | PTE_UXN
        );
    }
    uart_puts("mmu: l1[0x7C-0x7F] PCIe outbound (RP1) mapped (4x1GB)\n");

    // ==================================================================
    //  VC mailbox: 0x100000B880 を 4KB Device-nGnRnE page として貼る
    // ==================================================================
    {
        const u64 phys = VC_MAILBOX_PHYS;
        const int vc_l1_idx = (int)((phys >> 30) & 0x1FF); // bits[38:30]
        const int vc_l2_idx = (int)((phys >> 21) & 0x1FF); // bits[29:21]
        const int vc_l3_idx = (int)((phys >> 12) & 0x1FF); // bits[20:12]

        if (page_table_l1[vc_l1_idx] != 0) {
            uart_puts("mmu: l1[");
            uart_puthex32(vc_l1_idx);
            uart_puts("] 1GB block REMOVED for VC mailbox table\n");
        }

        // L3 entry for mailbox page
        page_table_l3_vc[vc_l3_idx] =
            (phys & ~0xFFFULL) |
            PTE_VALID | PTE_PAGE | PTE_AF |
            PTE_ATTR(MAIR_IDX_DEVICE_nGnRnE) |
            PTE_AP_RW_EL1 |
            PTE_PXN | PTE_UXN;

        // L2 entry pointing to L3
        page_table_l2_vc[vc_l2_idx] =
            ((u64)page_table_l3_vc) |
            PTE_VALID | PTE_TABLE;

        // L1 entry pointing to L2
        page_table_l1[vc_l1_idx] =
            ((u64)page_table_l2_vc) |
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

    // ==================================================================
    //  MAIR_EL1 設定
    // ==================================================================
    u64 mair = 0;
    mair |= (0x00ULL << (MAIR_IDX_DEVICE_nGnRnE * 8)); // Device-nGnRnE
    mair |= (0x04ULL << (MAIR_IDX_DEVICE_nGnRE  * 8)); // Device-nGnRE
    mair |= (0x44ULL << (MAIR_IDX_NORMAL_NC     * 8)); // Normal non-cache
    mair |= (0xFFULL << (MAIR_IDX_NORMAL        * 8)); // Normal WB

    __asm__ volatile("msr mair_el1, %0" :: "r"(mair));
    uart_puts("mmu: MAIR set\n");
    {
        u64 r;
        __asm__ volatile("mrs %0, mair_el1" : "=r"(r));
        uart_puts("mmu: MAIR_EL1=");
        uart_puthex64(r);
        uart_puts("\n");
    }

    // ==================================================================
    //  TCR_EL1 設定
    //  - 4KB granule, 48-bit VA (T0SZ=T1SZ=16)
    //  - 40-bit PA (IPS=2) - Pi5/BCM2712 の物理アドレス幅に合わせる
    // ==================================================================
    u64 tcr = 0;

    // T0 (TTBR0_EL1)
    tcr |= (16ULL << 0);   // T0SZ = 16 → 48-bit VA
    tcr |= (0ULL  << 6);   // reserved
    tcr |= (0ULL  << 7);   // EPD0 = 0
    tcr |= (3ULL  << 8);   // IRGN0 = 3 (WB, WA)
    tcr |= (3ULL  << 10);  // ORGN0 = 3
    tcr |= (3ULL  << 12);  // SH0 = 3 (inner shareable)
    tcr |= (2ULL  << 14);  // TG0 = 2 (4KB)

    // T1 (TTBR1_EL1) – 使わないが valid に
    tcr |= (16ULL << 16);  // T1SZ = 16
    tcr |= (0ULL  << 22);  // A1 = 0
    tcr |= (0ULL  << 23);  // EPD1 = 0
    tcr |= (3ULL  << 24);  // IRGN1 = 3
    tcr |= (3ULL  << 26);  // ORGN1 = 3
    tcr |= (3ULL  << 28);  // SH1 = 3
    tcr |= (2ULL  << 30);  // TG1 = 2 (4KB)

    // IPS: 40-bit physical address (BCM2712)
    tcr |= (2ULL << 32);   // IPS = 2 (40-bit)

    __asm__ volatile("msr tcr_el1, %0" :: "r"(tcr));
    uart_puts("mmu: TCR set\n");
    {
        u64 r;
        __asm__ volatile("mrs %0, tcr_el1" : "=r"(r));
        uart_puts("mmu: TCR_EL1=");
        uart_puthex64(r);
        uart_puts("\n");
    }

    // ==================================================================
    //  TTBR0_EL1
    // ==================================================================
    __asm__ volatile("msr ttbr0_el1, %0" :: "r"((u64)page_table_l0));
    uart_puts("mmu: TTBR0 set\n");

    // Barrier
    __asm__ volatile("dsb sy");
    __asm__ volatile("isb");

    // ==================================================================
    //  SCTLR_EL1 : enable MMU + caches
    // ==================================================================
    u64 sctlr;
    __asm__ volatile("mrs %0, sctlr_el1" : "=r"(sctlr));
    sctlr |= (1ULL << 0);   // M : MMU enable
    sctlr |= (1ULL << 2);   // C : data cache
    sctlr |= (1ULL << 12);  // I : instruction cache
    __asm__ volatile("msr sctlr_el1, %0" :: "r"(sctlr));
    uart_puts("mmu: SCTLR (MMU enabled)\n");

    __asm__ volatile("dsb sy");
    __asm__ volatile("isb");

    // ==================================================================
    //  Post-MMU probes
    // ==================================================================
    uart_puts("mmu: post-MMU probes\n");
    {
        volatile u32 *addr;
        u32 v;

        addr = (volatile u32 *)0xFE00B880ULL;
        v = *addr;
        uart_puts("FE00B880 -> ");
        uart_puthex32(v);
        uart_puts("\n");

        addr = (volatile u32 *)0x100000B880ULL;
        v = *addr;
        uart_puts("100000B880 -> ");
        uart_puthex32(v);
        uart_puts("\n");

        addr = (volatile u32 *)0x107C8000ULL;
        v = *addr;
        uart_puts("107C8000 -> ");
        uart_puthex32(v);
        uart_puts("\n");
    }
}
