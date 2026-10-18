//! AMD DEV register map: capability offsets and DEV function numbers.
//!
//! C correspondence: `amddev.c:22-46` — the capability-space offsets
//! (`DEV_OP 4`, `DEV_DATA 8`, the function selector's shift), the eight
//! DEV functions (`DEVF_BASE_LO` through `DEVF_ERR_ADDR_HI`), the
//! capability-word masks (`DEVF_CAP_MAPS_MASK` and friends), and the
//! exclusion-table geometry (`0x100000/8` bytes with the first
//! `0x37000/8` set to all ones); plus the PCI capability-list constants
//! (`arch/i386/include/pci.h:14-143`: `PSR_CAPPTR`, `PCI_CAPPTR`,
//! `PCI_CP_MASK`, `CAP_T_SECURE_DEV`, `CAP_SD_INFO`, `CAP_SD_SUBTYPE_MASK`,
//! `CAP_T_SD_DEV`).

/// Request-select register offset from the capability pointer (`DEV_OP`).
pub const DEV_OP: u8 = 4;
/// Function part of the request-select value (`DEV_OP_FUNC_SHIFT`).
pub const DEV_OP_FUNC_SHIFT: u32 = 8;
/// Data register offset from the capability pointer (`DEV_DATA`).
pub const DEV_DATA: u8 = 8;

/// Function: table base, low part (`DEVF_BASE_LO`).
pub const DEVF_BASE_LO: u32 = 0;
/// Function: table base, high part (`DEVF_BASE_HI`).
pub const DEVF_BASE_HI: u32 = 1;
/// Function: device exclusion vector (`DEVF_MAP`).
pub const DEVF_MAP: u32 = 2;
/// Function: capability word (`DEVF_CAP`).
pub const DEVF_CAP: u32 = 3;
/// Capability: map count field (`DEVF_CAP_MAPS_MASK`).
pub const DEVF_CAP_MAPS_MASK: u32 = 0x00FF_0000;
/// Capability: map count shift.
pub const DEVF_CAP_MAPS_SHIFT: u32 = 16;
/// Capability: domain count field (`DEVF_CAP_DOMS_MASK`).
pub const DEVF_CAP_DOMS_MASK: u32 = 0x0000_FF00;
/// Capability: domain count shift.
pub const DEVF_CAP_DOMS_SHIFT: u32 = 8;
/// Capability: revision field (`DEVF_CAP_REV_MASK`).
pub const DEVF_CAP_REV_MASK: u32 = 0x0000_00FF;
/// Capability: revision shift.
pub const DEVF_CAP_REV_SHIFT: u32 = 0;
/// Function: control register (`DEVF_CR`).
pub const DEVF_CR: u32 = 4;
/// Function: error status (`DEVF_ERR_STATUS`).
pub const DEVF_ERR_STATUS: u32 = 5;
/// Function: error address, low part (`DEVF_ERR_ADDR_LO`).
pub const DEVF_ERR_ADDR_LO: u32 = 6;
/// Function: error address, high part (`DEVF_ERR_ADDR_HI`).
pub const DEVF_ERR_ADDR_HI: u32 = 7;

/// Value written to the control register at init (`amddev.c:158`).
pub const CONTROL_VALUE: u32 = 0x10 | 0x8 | 0x4 | 1;

/// Exclusion table size in bytes (`0x100000 / 8`).
pub const TABLE_SIZE: usize = 0x100000 / 8;
/// Bytes of the table pre-set to all ones (`0x37000 / 8`).
pub const TABLE_SET_BYTES: usize = 0x37000 / 8;

/// PCI status: capabilities-list bit (`PSR_CAPPTR`).
pub const PSR_CAPPTR: u16 = 0x0010;
/// Capability pointer register (`PCI_CAPPTR`).
pub const PCI_CAPPTR: u8 = 0x34;
/// Capability pointer mask (`PCI_CP_MASK`).
pub const PCI_CP_MASK: u8 = 0xFC;
/// Capability id: secure device (`CAP_T_SECURE_DEV`).
pub const CAP_T_SECURE_DEV: u8 = 0x0F;
/// Offset from the capability pointer: subtype info (`CAP_SD_INFO`).
pub const CAP_SD_INFO: u8 = 2;
/// Subtype mask (`CAP_SD_SUBTYPE_MASK`).
pub const CAP_SD_SUBTYPE_MASK: u8 = 0x0F;
/// Subtype: AMD DEV (`CAP_T_SD_DEV`).
pub const CAP_T_SD_DEV: u8 = 0;

/// The IOMMU map request (`IOMMU_MAP`, bus base plus thirty-two).
pub const IOMMU_MAP: i32 = 0x300 + 32;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constants_match_the_driver_header() {
        assert_eq!(DEV_OP, 4);
        assert_eq!(DEV_DATA, 8);
        assert_eq!(DEV_OP_FUNC_SHIFT, 8);
        assert_eq!(DEVF_MAP, 2);
        assert_eq!(DEVF_CAP, 3);
        assert_eq!(CONTROL_VALUE, 0x1D);
        assert_eq!(TABLE_SIZE, 131072);
        assert_eq!(TABLE_SET_BYTES, 28160);
        assert_eq!(IOMMU_MAP, 0x320);
        assert_eq!(PSR_CAPPTR, 0x0010);
        assert_eq!(PCI_CP_MASK, 0xFC);
        assert_eq!(CAP_T_SECURE_DEV, 0x0F);
        assert_eq!(CAP_SD_INFO, 2);
        assert_eq!(CAP_T_SD_DEV, 0);
    }
}
