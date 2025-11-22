// drivers/rp1_phy.rs
//
// RP1 PHY bring-up helper (MDIO + auto-negotiation)



#[derive(Debug)]
pub enum PhyError {
    Bar0NotProgrammed,
    MdioTimeout,
    NoPhyFound,
    AutonegTimeout,
}

pub struct Rp1Phy {
    mdio_base: usize,
}

impl Rp1Phy {
    /// RP1 の MDIO ブロックのベースアドレスを計算
    ///   BAR0 (RP1) + 0x10000 = MACB base
    ///   MACB base + 0x200   = MDIO block (NPHY/NDATA)
    pub fn new() -> Result<Self, PhyError> {
        // MACB base is 0x60_0010_0000 (CPU view of RP1 BAR1 + 0x10_0000)
        // MDIO offset is 0x200
        let mdio_base = 0x60_0010_0200;
        Ok(Rp1Phy { mdio_base })
    }

    /// NPHY/NDATA のオフセット（RP1 固有）
    const NPHY: usize = 0x14;
    const NDATA: usize = 0x18;

    unsafe fn mdio_read(&self, phy: u8, reg: u8) -> Result<u16, PhyError> {
        use crate::drivers::timer::TIMER;

        // CMD: bit15=start, bits14:7=phy, bits6:2=reg, bits1:0=op(2=read)
        let cmd: u32 = (1 << 15) | ((phy as u32) << 7) | ((reg as u32) << 2) | 2;
        core::ptr::write_volatile(
            (self.mdio_base + Self::NPHY) as *mut u32,
            cmd,
        );

        // busy クリア待ち (bit0 == 0)
        for _ in 0..1000 {
            let v = core::ptr::read_volatile((self.mdio_base + Self::NPHY) as *const u32);
            if (v & 1) == 0 {
                let data =
                    core::ptr::read_volatile((self.mdio_base + Self::NDATA) as *const u32) as u16;
                return Ok(data);
            }
            TIMER.delay_us(10);
        }
        Err(PhyError::MdioTimeout)
    }

    unsafe fn mdio_write(&self, phy: u8, reg: u8, val: u16) -> Result<(), PhyError> {
        use crate::drivers::timer::TIMER;

        // NDATA に data + DONE ビットを書いてから NPHY にコマンド
        core::ptr::write_volatile(
            (self.mdio_base + Self::NDATA) as *mut u32,
            0x8000_0000u32 | (val as u32),
        );
        // op=0(write)
        let cmd: u32 = ((phy as u32) << 7) | ((reg as u32) << 2) | 0;
        core::ptr::write_volatile(
            (self.mdio_base + Self::NPHY) as *mut u32,
            cmd,
        );

        for _ in 0..1000 {
            let v = core::ptr::read_volatile((self.mdio_base + Self::NPHY) as *const u32);
            if (v & 1) == 0 {
                return Ok(());
            }
            TIMER.delay_us(10);
        }
        Err(PhyError::MdioTimeout)
    }

    /// PHY を探す（0..31 をスキャン）: BMSR が 0xFFFF/0 以外のところ
    pub unsafe fn detect_phy(&self) -> Result<u8, PhyError> {
        const MII_BMSR: u8 = 0x01;

        crate::print_str("[PHY] scanning MDIO addresses 0..31\n");
        for phy in 0..32u8 {
            if let Ok(val) = self.mdio_read(phy, MII_BMSR) {
                if val != 0xFFFF && val != 0 {
                    crate::print_str("[PHY] found at addr ");
                    crate::print_dec(phy as usize);
                    crate::print_str(" (BMSR=0x");
                    crate::print_hex(val as usize);
                    crate::print_str(")\n");
                    return Ok(phy);
                }
            }
        }
        Err(PhyError::NoPhyFound)
    }

    /// オートネゴ開始 + link up 待ち
    pub unsafe fn autoneg_and_wait(&self, phy: u8) -> Result<(), PhyError> {
        use crate::drivers::timer::TIMER;

        // MII 定数
        const MII_BMCR: u8 = 0x00;
        const MII_BMSR: u8 = 0x01;
        const MII_ANAR: u8 = 0x04;
        const MII_GBCR: u8 = 0x09;

        const BMCR_ANENABLE: u16 = 1 << 12;
        const BMCR_RESTARTAN: u16 = 1 << 9;

        const BMSR_LINK_STATUS: u16 = 1 << 2;
        const BMSR_AUTONEG_COMPLETE: u16 = 1 << 5;

        crate::print_str("[PHY] auto-negotiation setup\n");

        // advertise 10/100 + 1000
        // ANAR: 10/100 full/half + pause (ほぼ Linux と同じパターン)
        let _ = self.mdio_write(phy, MII_ANAR, 0x01E1);
        // GBCR: 1000base-T full/half
        let _ = self.mdio_write(phy, MII_GBCR, 0x0300);

        // BMCR 読み出し → ANENABLE / RESTARTAN セット
        let mut bmcr = self.mdio_read(phy, MII_BMCR).unwrap_or(0);
        bmcr |= BMCR_ANENABLE | BMCR_RESTARTAN;
        let _ = self.mdio_write(phy, MII_BMCR, bmcr);

        crate::print_str("[PHY] waiting for autoneg + link\n");

        // BMSR は latched なので毎回2回読む
        for i in 0..100 {
            TIMER.delay_ms(100);

            let _ = self.mdio_read(phy, MII_BMSR); // latch clear
            if let Ok(bmsr) = self.mdio_read(phy, MII_BMSR) {
                crate::print_str("[PHY] poll ");
                crate::print_dec(i as usize);
                crate::print_str(": BMSR=0x");
                crate::print_hex(bmsr as usize);
                crate::print_str("\n");

                if (bmsr & BMSR_LINK_STATUS) != 0 && (bmsr & BMSR_AUTONEG_COMPLETE) != 0 {
                    crate::print_str("[PHY] link UP & autoneg complete\n");
                    return Ok(());
                }
            }
        }

        crate::print_str("[PHY] autoneg timeout (link did not come up)\n");
        Err(PhyError::AutonegTimeout)
    }

    /// 単純な bring-up シークエンス一発叩き用ヘルパ
    pub unsafe fn bring_up(&self) -> Result<(), PhyError> {
        let phy = self.detect_phy()?;

        // デバッグ用に ID も出しておく
        if let Ok(id1) = self.mdio_read(phy, 2) {
            crate::print_str("[PHY] ID1=0x");
            crate::print_hex(id1 as usize);
            crate::print_str("\n");
        }
        if let Ok(id2) = self.mdio_read(phy, 3) {
            crate::print_str("[PHY] ID2=0x");
            crate::print_hex(id2 as usize);
            crate::print_str("\n");
        }

        self.autoneg_and_wait(phy)
    }
}
