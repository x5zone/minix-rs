//! Firmware-agnostic boot preparation result.
//!
//! Produced by the U-Boot production leg's `opensbi_helpers::prepare_boot`.
//! The kernel does **not** depend on this type — its entry surface is
//! `arch_boot` / `enter_kernel_image_v1` (doc 01 §3.2).

use crate::boot_storage::BootReport;
use minix_types::{PhysBytes, PhysRange};

/// Firmware-agnostic result of boot preparation.
///
/// Contains everything the kernel needs to set up paging and jump to kmain:
/// - `report`: borrowed view over the caller-owned [`BootStorage`] (memory
///   map, kernel location, boot modules, platform sources, parameter bytes,
///   provenance records)
/// - `root_page`: physical address of the root page table page
/// - `bump_base` / `bump_end`: bump region for boot-stage page table
///   allocation
/// - `arena_pa`: the handoff arena the shim reserved from the same pool
///
/// This struct is produced by the boot leg's `prepare_boot` and consumed **in
/// the same address space**. The cross-image authority is the wire protocol
/// `BootHandoffV1` ([`crate::handoff_v1`]); the report is the sender-side
/// semantic view the wire is encoded from. Its lifetime is tied to the
/// caller's storage, so it cannot outlive the owner (R18 D-1).
pub struct BootPrepareResult<'a> {
    pub report: BootReport<'a>,
    pub root_page: PhysBytes,
    /// Bump region `[start, end)` (physical).
    pub bump: PhysRange,
    /// Handoff arena base (physical), allocated from the same bump pool after
    /// the module landings.
    pub arena_pa: PhysBytes,
}
