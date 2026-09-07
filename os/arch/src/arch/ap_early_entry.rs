//! AP early entry ABI — the cross-assembly value-passing contract (S-3a).
//!
//! Frozen design: smp_todo §3.2 (2026-09-06 GPT-review revision). The three
//! architectures converge on one Rust entry point that receives a *physical
//! address*; all per-AP information travels inside an `ApBootstrap` record.
//!
//! # Object boundary (v7 #4)
//!
//! `ApBootstrap` is a **layout type only** — `#[repr(C)]` shape for the
//! cross-assembly ABI, no storage semantics of its own. Runtime storage
//! differs by architecture:
//!
//! - **x86**: the record is embedded at a fixed offset in the copied
//!   `ApEarlyEntryImage` (code section + data section, copied below 1 MiB —
//!   the SIPI landing requirement). BSP fills fields, hands off, AP reads.
//! - **aarch64/riscv64**: the record is writable static storage inside the
//!   kernel image (no copy; firmware starts the AP at the image's physical
//!   address directly).
//!
//! # Lifecycle invariant (v5 P0-1 / v6 #4)
//!
//! `boot_ack` (Release) published by the AP == "the AP has performed its
//! last read of the current record". Everything the AP needs afterwards is
//! copied into locals *before* the ack. BSP observing the ack may rewrite
//! the record for the next AP — the happens-before edge of the single-image
//! serial-reuse discipline.

/// Per-AP bootstrap record — the cross-assembly ABI shape.
///
/// Field order and width are frozen: the assembly stubs read these offsets,
/// and the x86 image layout embeds the record at a fixed offset inside the
/// copied blob. Adding a field is an ABI review, not a refactor.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ApBootstrap {
    /// Topology array index (CpuId) of the AP being started.
    pub logical_id: u32,
    /// Reserved padding — keeps `hw_id` 8-byte aligned inside the record
    /// (the asm stubs read fields at fixed offsets; no hidden padding).
    pub _pad: u32,
    /// Hardware ID of the AP (APIC ID / MPIDR / hartid).
    pub hw_id: u64,
    /// BSP page-table root, physical. MMU-off read on x86 (must be
    /// <4GiB — see §3.2 invariant; asserted by the BSP when filling).
    pub page_table_root_pa: u64,
    /// Per-AP kernel stack top, virtual. Dereferenced only after MMU on.
    pub kernel_stack_top_va: u64,
    /// Link-time VA of `ap_early_entry` (used on x86 by the 64-bit stub's
    /// handoff to the Rust entry).
    pub rust_entry_va: u64,
}

// The asm stubs index fields at fixed offsets — pin the layout.
const _: () = {
    use core::mem::offset_of;
    assert!(core::mem::size_of::<ApBootstrap>() == 40);
    assert!(offset_of!(ApBootstrap, logical_id) == 0);
    assert!(offset_of!(ApBootstrap, hw_id) == 8);
    assert!(offset_of!(ApBootstrap, page_table_root_pa) == 16);
    assert!(offset_of!(ApBootstrap, kernel_stack_top_va) == 24);
    assert!(offset_of!(ApBootstrap, rust_entry_va) == 32);
};

/// Mailbox magic written by the BSP when the record is ready to consume.
pub const BOOT_MAGIC_SENT: u64 = 0x5350_4D42_5245_4144; // "SPMBREAD" (LE)
/// Mailbox magic written back by the AP stub after consuming the record.
pub const BOOT_MAGIC_ACK: u64 = 0x4143_4B42_4F4F_5453; // "ACKBOOTS" (LE)

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ap_bootstrap_layout_frozen() {
        assert_eq!(core::mem::size_of::<ApBootstrap>(), 40);
        assert_eq!(core::mem::align_of::<ApBootstrap>(), 8);
    }

    #[test]
    fn test_mailbox_magics_distinct() {
        assert_ne!(BOOT_MAGIC_SENT, BOOT_MAGIC_ACK);
    }
}
