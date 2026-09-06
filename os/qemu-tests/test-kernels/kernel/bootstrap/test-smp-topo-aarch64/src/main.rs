//! Test: topology discovery pin (SMP bring-up S-2, ladder L1) — aarch64/DTB.
//!
//! **Current status: SKIP (blocked, see below).**
//!
//! The DTB parser itself (`device_tree.rs`) is architecture-independent and
//! is pinned by the riscv64 variant of this test, which reaches the same
//! `parse_by_kind` → `DeviceTreeDesc::parse` path via OpenSBI's a1 handoff.
//!
//! Why aarch64 cannot reach the parser today — three independent blockers,
//! each verified empirically on QEMU virt + AAVMF (Ubuntu 2024.02) with
//! `-smp 4`:
//!
//! 1. The AAVMF firmware does **not** install the FDT as a UEFI
//!    config-table entry (8 entries scanned, none with the FDT GUID
//!    b1b621d2-f19c-41c5-8310-daa6f018a8d3).
//! 2. `minix-platform`'s ACPI module is `x86_64`-gated, so there is no ACPI
//!    (MADT/GICC) fallback on aarch64 — `parse_by_kind`'s RSDP arm does not
//!    even compile in for this target.
//! 3. QEMU's fw_cfg MMIO window (0x902_0000) is unmapped at UEFI
//!    boot-services stage under AAVMF — a direct read raises a synchronous
//!    exception.
//!
//! Unblocking path: S-2b (kernel-side platform-discovery fix — extend
//! `parse_by_kind`'s RSDP arm to aarch64 and/or add a DTB handoff that does
//! not rely on the config table). This test converts SKIP → full assertions
//! at that point; the assertion code is already written and compiles.

#![no_std]
#![no_main]

use core::arch::asm;
use core::cell::Cell;
use core::panic::PanicInfo;
use minix_plat::arm64::early_console;
use uefi::prelude::*;

// UEFI test kernels allocate only while boot services are alive; boot-shim's
// pool allocator covers exactly that window (same rationale as test-memmap).
#[global_allocator]
static ALLOCATOR: boot_shim::uefi_helpers::UefiPoolAllocator =
    boot_shim::uefi_helpers::UefiPoolAllocator;

#[entry]
fn main() -> Status {
    early_console::write_str("### test_smp_topo (aarch64): topology discovery pin\n");

    // Empirical scan, kept as a live regression tripwire: if a future AAVMF
    // starts installing the FDT config table (or a QEMU/AAVMF upgrade changes
    // the handoff), this line says so and the SKIP rationale must be
    // revisited. The DTB GUID matches boot-shim's own DEVICE_TREE_GUID.
    let dtb_guid = uefi::guid!("b1b621d2-f19c-41c5-8310-daa6f018a8d3");
    let fdt_found = Cell::new(false);
    system::with_config_table(|entries| {
        for e in entries {
            if e.guid == dtb_guid {
                fdt_found.set(true);
            }
        }
    });
    if fdt_found.get() {
        // The blocker disappeared — the SKIP note above is stale. Do not
        // silently PASS: the assertion code is behind S-2b and must be
        // re-enabled deliberately.
        early_console::write_str("### FAIL: FDT config table now present — revisit SKIP rationale (S-2b) ###\n");
        loop { unsafe { asm!("wfe", options(nomem, nostack)); } }
    }

    early_console::write_str("  config table carries no FDT (verified) — aarch64 discovery blocked\n");
    early_console::write_str("  blocker 1: AAVMF does not install FDT as config table entry\n");
    early_console::write_str("  blocker 2: minix-platform acpi module is x86_64-gated (no MADT/GICC fallback)\n");
    early_console::write_str("  blocker 3: fw_cfg MMIO unmapped at UEFI boot-services stage\n");
    early_console::write_str("  DTB parser coverage: provided by test-smp-topo-riscv64 (same parse path)\n");
    early_console::write_str("### TEST_RESULT: SKIP test-smp-topo-aarch64 (blocked by S-2b) ###\n");
    loop { unsafe { asm!("wfe", options(nomem, nostack)); } }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    early_console::write_str("### PANIC ###\n");
    loop { unsafe { asm!("wfe", options(nomem, nostack)); } }
}
