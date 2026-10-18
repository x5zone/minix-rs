//! Ti1225 register definitions: PCI attribute offsets and socket CSRs.
//!
//! C correspondence: `ti1225.h` (PCI attribute-space offsets `ti1225.h:8-18`,
//! the CardBus socket register block `ti1225.h:20-32`, the mask members
//! `ti1225.h:34-36`, the card-present bits `ti1225.h:38-56`, and the
//! socket control bits `ti1225.h:58-70`) plus the bridge-control pair in
//! the PCI header (`CBB_BRIDGECTRL 0x3E`, `CBB_BC_INTEXCA 0x80`,
//! `CBB_BC_CRST 0x40`, `arch/i386/include/pci.h:109-111`).

/// CardBus/ExCA memory base address (`TI_CB_BASEADDR`).
pub const TI_CB_BASEADDR: u8 = 0x10;
/// PCI bus number (`TI_PCI_BUS_NR`).
pub const TI_PCI_BUS_NR: u8 = 0x18;
/// CardBus bus number (`TI_CB_BUS_NR`).
pub const TI_CB_BUS_NR: u8 = 0x19;
/// Subordinate bus number (`TI_SO_BUS_NR`).
pub const TI_SO_BUS_NR: u8 = 0x1A;
/// PC Card 16-bit legacy-mode base address (`TI_LEGACY_BA`).
pub const TI_LEGACY_BA: u8 = 0x44;
/// System control register (`TI_SYSCTRL`).
pub const TI_SYSCTRL: u8 = 0x80;
/// Multifunction routing (`TI_MF_ROUTE`).
pub const TI_MF_ROUTE: u8 = 0x8C;
/// Card control register (`TI_CARD_CTRL`).
pub const TI_CARD_CTRL: u8 = 0x91;
/// Functional interrupt flag in the card control register (`TI_CCR_IFG`).
pub const TI_CCR_IFG: u8 = 0x01;
/// Device control register (`TI_DEV_CTRL`).
pub const TI_DEV_CTRL: u8 = 0x92;
/// Bridge control register (`CBB_BRIDGECTRL`).
pub const CBB_BRIDGECTRL: u8 = 0x3E;
/// Interrupts routed to ExCAs (`CBB_BC_INTEXCA`).
pub const CBB_BC_INTEXCA: u16 = 0x80;
/// Assert the cardbus reset line (`CBB_BC_CRST`).
pub const CBB_BC_CRST: u16 = 0x40;

/// Socket register offsets inside the mapped aperture (`struct csr`,
/// `ti1225.h:20-32`).
pub mod csr {
    /// Event bits.
    pub const EVENT: usize = 0x00;
    /// Event mask (`CM_*` bits).
    pub const MASK: usize = 0x04;
    /// Card-present and card-type bits (`CP_*`).
    pub const PRESENT: usize = 0x08;
    /// Forced event bits.
    pub const FORCE_EVENT: usize = 0x0C;
    /// Socket control (`CC_*`).
    pub const CONTROL: usize = 0x10;
    /// Power control.
    pub const POWER: usize = 0x20;
}

/// Mask bits written at init (`ti1225.c:230-231`).
pub const CM_PWRMASK: u32 = 0x0000_0008;
/// Card-detect mask pair (`CM_CDMASK`).
pub const CM_CDMASK: u32 = 0x0000_0006;
/// Card-status mask (`CM_CSTSMASK`).
pub const CM_CSTSMASK: u32 = 0x0000_0001;

/// Socket supports Y.Y volts (`CP_YVSOCKET`).
pub const CP_YVSOCKET: u32 = 0x8000_0000;
/// Socket supports X.X volts (`CP_XVSOCKET`).
pub const CP_XVSOCKET: u32 = 0x4000_0000;
/// Socket supports 3 V (`CP_3VSOCKET`).
pub const CP_3VSOCKET: u32 = 0x2000_0000;
/// Socket supports 5 V (`CP_5VSOCKET`).
pub const CP_5VSOCKET: u32 = 0x1000_0000;
/// Card wants Y.Y volts (`CP_YVCARD`).
pub const CP_YVCARD: u32 = 0x0000_2000;
/// Card wants X.X volts (`CP_XVCARD`).
pub const CP_XVCARD: u32 = 0x0000_1000;
/// Card wants 3 V (`CP_3VCARD`).
pub const CP_3VCARD: u32 = 0x0000_0800;
/// Card wants 5 V (`CP_5VCARD`).
pub const CP_5VCARD: u32 = 0x0000_0400;
/// Bad Vcc request (`CP_BADVCCREQ`).
pub const CP_BADVCCREQ: u32 = 0x0000_0200;
/// Data lost (`CP_DATALOST`).
pub const CP_DATALOST: u32 = 0x0000_0100;
/// Not a card (`CP_NOTACARD`).
pub const CP_NOTACARD: u32 = 0x0000_0080;
/// IREQ interrupt (`CP_IREQCINT`).
pub const CP_IREQCINT: u32 = 0x0000_0040;
/// CardBus card (`CP_CBCARD`).
pub const CP_CBCARD: u32 = 0x0000_0020;
/// 16-bit card (`CP_16BITCARD`).
pub const CP_16BITCARD: u32 = 0x0000_0010;
/// Power cycle in progress (`CP_PWRCYCLE`).
pub const CP_PWRCYCLE: u32 = 0x0000_0008;
/// Card detect 2 (`CP_CDETECT2`).
pub const CP_CDETECT2: u32 = 0x0000_0004;
/// Card detect 1 (`CP_CDETECT1`).
pub const CP_CDETECT1: u32 = 0x0000_0002;
/// Card status change (`CP_CARDSTS`).
pub const CP_CARDSTS: u32 = 0x0000_0001;

/// Vcc control field (`CC_VCCCTRL`).
pub const CC_VCCCTRL: u32 = 0x0000_0070;
/// Vcc off (`CC_VCC_OFF`).
pub const CC_VCC_OFF: u32 = 0x0000_0000;
/// Vcc 5 V (`CC_VCC_5V`).
pub const CC_VCC_5V: u32 = 0x0000_0020;
/// Vcc 3 V (`CC_VCC_3V`).
pub const CC_VCC_3V: u32 = 0x0000_0030;
/// Vcc X.X V (`CC_VCC_XV`).
pub const CC_VCC_XV: u32 = 0x0000_0040;
/// Vcc Y.Y V (`CC_VCC_YV`).
pub const CC_VCC_YV: u32 = 0x0000_0050;

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::{offset_of, size_of};

    /// The C-layout mirror of `struct csr` (`ti1225.h:20-32`).
    #[repr(C)]
    struct MirrorCsr {
        event: u32,
        mask: u32,
        present: u32,
        force_event: u32,
        control: u32,
        res0: u32,
        res1: u32,
        res2: u32,
        power: u32,
    }

    #[test]
    fn test_csr_layout_matches_the_c_struct() {
        assert_eq!(size_of::<MirrorCsr>(), 0x24);
        assert_eq!(offset_of!(MirrorCsr, event), csr::EVENT);
        assert_eq!(offset_of!(MirrorCsr, mask), csr::MASK);
        assert_eq!(offset_of!(MirrorCsr, present), csr::PRESENT);
        assert_eq!(offset_of!(MirrorCsr, force_event), csr::FORCE_EVENT);
        assert_eq!(offset_of!(MirrorCsr, control), csr::CONTROL);
        assert_eq!(offset_of!(MirrorCsr, power), csr::POWER);
    }

    #[test]
    fn test_bits_match_the_header() {
        assert_eq!(CM_PWRMASK | CM_CDMASK | CM_CSTSMASK, 0x0F);
        assert_eq!(CP_CDETECT1 | CP_CDETECT2, 0x06);
        assert_eq!(CC_VCCCTRL, 0x70);
        assert_eq!(CC_VCC_5V, 0x20);
        assert_eq!(CC_VCC_3V, 0x30);
        assert_eq!(CC_VCC_XV, 0x40);
        assert_eq!(CC_VCC_YV, 0x50);
        assert_eq!(CBB_BC_INTEXCA, 0x80);
        assert_eq!(CBB_BC_CRST, 0x40);
        assert_eq!(TI_CB_BUS_NR, 0x19);
        assert_eq!(TI_LEGACY_BA, 0x44);
    }
}
