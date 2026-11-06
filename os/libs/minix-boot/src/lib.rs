#![no_std]
#![warn(clippy::let_underscore_must_use)]

//! Minix3 boot protocol types.
//!
//! Provides the data structures and traits shared between boot-shim and kernel
//! during the boot handoff phase. These are **not** IPC protocol types —
//! they describe the boot-time contract (memory map, kernel location, modules).

#[cfg(test)]
extern crate alloc;
// Host tests check on-disk source paths (`BootForm::source_path` existence);
// `std` is test-only, the production build stays `#![no_std]`.
#[cfg(test)]
extern crate std;

pub mod boot_form;
pub mod boot_params;
pub mod boot_shim;
pub mod fdt_layout;
pub mod handoff_v1;
pub mod handoff_v1_read;
pub mod handoff_v1_write;
pub mod kernel_info;
pub mod memmap_source;
pub mod platform;

pub use boot_form::{ALL_BOOT_FORMS, BootForm};
pub use boot_params::*;
pub use boot_shim::*;
pub use handoff_v1::*;
pub use handoff_v1_read::*;
pub use handoff_v1_write::*;
pub use kernel_info::*;
pub use memmap_source::{
    DtbMemoryMap, PhysicalRange, SourceAuditError, SourceKind, SourceOrigin, SourceRecord,
    audit_source_coverage, dtb_covers_conventional,
};
pub use platform::{
    ArchMiscDesc, ConsoleDesc, CpuInfo, CpuTopology, DTB, InterruptControllerDesc, MAX_CPUS,
    PlatformDesc, PlatformDescKind, PlatformDescSource, PlatformParseError, PlatformSource, RSDP,
    TimerDesc,
};

/// The devicetree specification requires the blob to start on an 8-byte
/// boundary. Byte-wise parsing tolerates a misaligned blob, but accepting one
/// silently would mean trusting a shape the firmware contract forbids, so the
/// DTB-pointer boot paths refuse it before the first read.
pub const fn dtb_blob_aligned(addr: u64) -> bool {
    addr % 8 == 0
}

/// Upper bound this project accepts for a devicetree blob. The header's
/// `totalsize` is firmware-provided; without a bound the caller would have to
/// trust it before it can even build a slice.
pub const DTB_MAX_LEN: u32 = 64 * 1024 * 1024;

/// `totalsize` must cover at least the 40-byte header and stay within
/// [`DTB_MAX_LEN`].
pub const fn dtb_len_ok(total: u32) -> bool {
    total >= 40 && total <= DTB_MAX_LEN
}

#[cfg(test)]
mod dtb_input_tests {
    use super::*;

    #[test]
    fn alignment_accepts_multiples_of_eight_only() {
        for addr in [0u64, 8, 0x1000, 0x8000_0000] {
            assert!(dtb_blob_aligned(addr), "{addr:#x}");
        }
        for addr in [1u64, 4, 7, 0x8000_0004] {
            assert!(!dtb_blob_aligned(addr), "{addr:#x}");
        }
    }

    #[test]
    fn length_bound_covers_header_and_caps_the_blob() {
        assert!(!dtb_len_ok(0));
        assert!(!dtb_len_ok(39));
        assert!(dtb_len_ok(40));
        assert!(dtb_len_ok(DTB_MAX_LEN));
        assert!(!dtb_len_ok(DTB_MAX_LEN + 1));
    }
}
