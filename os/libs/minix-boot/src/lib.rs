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

pub mod kernel_info;
pub mod boot_form;
pub mod boot_shim;
pub mod handoff_v1;
pub mod handoff_v1_read;
pub mod handoff_v1_write;
pub mod memmap_source;
pub mod platform;

pub use kernel_info::*;
pub use boot_form::{BootForm, ALL_BOOT_FORMS};
pub use boot_shim::*;
pub use handoff_v1::*;
pub use handoff_v1_read::*;
pub use handoff_v1_write::*;
pub use memmap_source::{DtbMemoryMap, MemoryMapSource, MultibootMemoryMap, dtb_covers_conventional};
pub use platform::{
    PlatformDescKind, PlatformDescSource, PlatformDesc, PlatformSource,
    InterruptControllerDesc, TimerDesc, ConsoleDesc,
    CpuTopology, CpuInfo, ArchMiscDesc, MAX_CPUS,
    PlatformParseError,
    DTB, RSDP,
};
