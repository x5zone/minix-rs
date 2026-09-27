//! Boot handoff contract between the UEFI/PE boot-shim and the standalone
//! high-half kernel image (NK4-C §1.115, 方案 A / OQ-N6 交接).
//!
//! ## Why this exists
//!
//! Historically the kernel crate was linked *inline* into the boot-shim PE and
//! entered in-process via `minix_kernel::arch_boot`, so the kernel executed at
//! the PE's runtime physical address (low half, VA==PA). AArch64's
//! `switch_address_space` swaps TTBR0 to a per-process root and issues
//! `tlbi alle1is`; because the kernel code lived in the TTBR0 low-half identity
//! window, the very first root switch destroyed the kernel's own instruction
//! fetch mapping (§1.114 forensics: `elr`/`vbar` pinned at `0x43e0_xxxx`).
//!
//! 方案 A fixes this at the root: the boot-shim builds the bootstrap page
//! tables, enables paging, then *absolutely jumps* into the standalone
//! `kernel.elf` image's high-half `_start` (linked at `KERN_VIRT_BASE`, resident
//! in TTBR1, which is never switched). The kernel then owns the entire boot from
//! that point and executes entirely in the high half — immune to root switches.
//!
//! The two images (boot-shim PE and `kernel.elf`) each link their *own* copy of
//! `minix_kernel`, so they do **not** share `.bss` globals. Everything the
//! high-half kernel needs that the boot-shim established in physical memory is
//! therefore carried through this [`BootHandoff`] blob, which the boot-shim
//! writes to a bump-allocated, identity-mapped physical page and passes to
//! `_start` in `x0`.
//!
//! ## Layout stability
//!
//! `#[repr(C)]` fixes the field order/offsets of [`BootHandoff`] and of
//! `KernelInfo` (and its nested `MemoryRegion` / `BootModule` /
//! `PlatformDescSource`) so the boot-shim's write and the kernel image's read
//! agree. This is *not* a versioned ABI across arbitrary toolchains: the
//! `&'static [T]` fields' internal (ptr, len) order is still Rust-toolchain-
//! defined, so the contract holds because boot-shim and `kernel.elf` are always
//! produced by the *same* `xtask image` run (identical rustc + `--release`
//! profile). `KernelInfo` is `Copy` and its slice payloads point into stable,
//! identity-mapped physical memory (the boot-services pool survives EBS for the
//! boot window), so a raw struct copy is sound: the kernel image reads the
//! slices through the still-live TTBR0 identity mapping before the first root
//! switch.

use crate::kernel_info::KernelInfo;

/// Cross-image boot handoff payload (§1.115 方案 A).
///
/// Written by the boot-shim to a physical page, passed to the high-half kernel
/// image `_start` in `x0` as a `*const BootHandoff`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct BootHandoff {
    /// The boot `KernelInfo`, by value. Its slice payloads point into stable
    /// physical memory that remains identity-mapped (VA==PA) in the bootstrap
    /// root's TTBR0 half until the first `switch_address_space`.
    pub kernel_info: KernelInfo,

    /// Physical address of the bootstrap page-table root the boot-shim built
    /// and loaded into TTBR0/TTBR1. The kernel image wraps it via
    /// `Paging::from_active_root` (no zero-fill) to keep adding mappings to the
    /// *same* table.
    pub root_page: u64,

    /// Boot bump allocator `[base, end)` and the cursor `next` after the
    /// boot-shim finished building tables and wrote this blob. The kernel image
    /// resumes allocation from `bump_next` so it never re-hands-out a page the
    /// boot-shim's page tables (or this blob) already occupy.
    pub bump_base: u64,
    pub bump_next: u64,
    pub bump_end: u64,
}

// The blob is bump-allocated one 4KiB page; if the contract ever outgrows a
// page the boot-shim's single `boot_pt_alloc()` handoff would silently write
// past it. Fail the build instead of corrupting the next bump allocation.
const _: () = assert!(core::mem::size_of::<BootHandoff>() <= 4096);
