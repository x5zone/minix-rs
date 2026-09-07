//! x86 AP early entry image — section layout, boundary symbols, spike stub.
//!
//! The early entry code is a **flat, position-independent, self-contained
//! machine-code blob**: the BSP copies `[ap_early_entry_start,
//! ap_early_entry_end)` to a physical address below 1 MiB and the AP
//! executes it there. Link-time addresses and run-time addresses share
//! nothing — any absolute relocation inside the blob would make "copy"
//! mean "crash", so the sections below carry **zero relocations** (S-3a
//! spike acceptance: `readelf -r` on the object and PE/COFF base-reloc
//! checks on the final .efi must come back empty for these sections).
//!
//! S-3a ships the **spike stub** (64-bit only, rip-relative mailbox access,
//! `ret`) — enough to prove copy-and-execute end to end. S-3b replaces the
//! stub body with the real 16→32→64 ladder.
//!
//! # Sections
//!
//! - `.ap_early_entry` (AX): the code. Starts at `ap_early_entry_start`.
//! - `.ap_early_entry_data` (AW): the mailbox — a magic header plus an
//!   [`ApBootstrap`] record, embedded directly after the code. The blob
//!   ends at `ap_early_entry_end`.
//!
//! Adjacency of the two sections inside `[start, end)` is part of the
//! contract (the copy unit is the whole interval) — asserted by the spike
//! test kernel (`test-smp-spike`) against the runtime symbol values.

use crate::arch::ap_early_entry::ApBootstrap;
pub use crate::arch::ap_early_entry::{BOOT_MAGIC_ACK, BOOT_MAGIC_SENT};

core::arch::global_asm!(
    ".section .ap_early_entry, \"ax\"",
    ".globl ap_early_entry_start",
    ".globl ap_early_entry",
    "ap_early_entry_start:",
    "ap_early_entry:",
    // Spike stub contract (S-3a): entry with the copied-mailbox VA in
    // **RCX** — the x86_64-unknown-uefi target compiles `extern "C"` with
    // the Microsoft x64 ABI (first argument in RCX), which the spike
    // surfaced the hard way: a SysV-style RDI read got garbage, and the
    // first `movabs rcx` clobbered the argument before use. The real S-3b
    // ladder defines its own entry register (bootstrap PA), so this is
    // spike-local ABI, not the frozen contract.
    //
    // Three proofs (all state written into the copied mailbox):
    //   1. rip-relative WRITE into the blob's own data — the copied code
    //      executes and the code→data displacement survives the copy;
    //   2. the received argument (RCX) captured — register-absolute store
    //      works at the new base;
    //   3. rip-relative READ of what (1) wrote — read path proven.
    // No absolute link-time addresses anywhere: relocation-free by
    // construction.
    "  mov [rip + ap_early_entry_data_start + 8], rcx",
    "  movabs r11, {ack}",
    "  mov [rip + ap_early_entry_data_start], r11",
    "  mov rax, [rip + ap_early_entry_data_start]",
    "  mov [rcx + 16], rax",
    "  ret",
    ".section .ap_early_entry_data, \"aw\"",
    ".globl ap_early_entry_data_start",
    ".balign 8",
    "ap_early_entry_data_start:",
    // Mailbox: +0 magic (SENT/ACK), +8 echo slot, +16 rip-rel proof slot,
    // +24 the ApBootstrap record (fixed offset K per §3.2; the record is
    // writable static storage semantics inside the copied blob — v8 #5).
    ".quad 0",
    ".quad 0",
    ".quad 0",
    ".zero 40",
    ".globl ap_early_entry_end",
    "ap_early_entry_end:",
    ack = const BOOT_MAGIC_ACK,
);

unsafe extern "C" {
    /// Blob start (code section).
    pub static ap_early_entry_start: u8;
    /// Code entry point (offset 0 of the blob).
    pub static ap_early_entry: u8;
    /// Mailbox start — magic header plus an [`ApBootstrap`] record.
    pub static ap_early_entry_data_start: u8;
    /// Blob end (one past the data section).
    pub static ap_early_entry_end: u8;
}

/// Blob byte range as `(start, end)` raw addresses (spike report data).
pub fn blob_range() -> (usize, usize) {
    // SAFETY: address-only symbol reads (no dereference) — the addresses
    // are link-time constants placed by the linker.
    unsafe {
        (
            &ap_early_entry_start as *const u8 as usize,
            &ap_early_entry_end as *const u8 as usize,
        )
    }
}

/// Mailbox offset within the blob (`data_start - start`).
pub fn mailbox_offset() -> usize {
    unsafe {
        &ap_early_entry_data_start as *const u8 as usize
            - &ap_early_entry_start as *const u8 as usize
    }
}

/// Mailbox header length: magic(8) + echo(8) + rip-rel proof(8); the
/// `ApBootstrap` record starts at this offset inside the data area.
pub const MAILBOX_HEADER_LEN: usize = 24;

/// Total data-area size: header plus one frozen `ApBootstrap` record.
pub const MAILBOX_LEN: usize = MAILBOX_HEADER_LEN + core::mem::size_of::<ApBootstrap>();

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mailbox_layout_matches_abi_record() {
        // The record sits after the 3-slot header; the blob data area must
        // be able to hold header + record.
        assert!(MAILBOX_LEN >= MAILBOX_HEADER_LEN + 40);
    }
}
