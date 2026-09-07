//! Test: topology discovery pin (SMP bring-up S-2/S-2b, ladder L1) — aarch64/ACPI.
//!
//! **S-2b unblocked (2026-09-07): the discovery path is ACPI, not DTB.**
//!
//! Empirically verified on QEMU virt + AAVMF (Ubuntu 2024.02, `-smp 4`):
//! the config table carries **no FDT entry** under either the boot-shim's
//! legacy GUID or the UEFI-spec `EFI_DTB_TABLE_GUID` (b1b621d5-19f0-4008-
//! b30d-b83e8133f39b) — but it **does carry the ACPI 2.0 RSDP GUID**
//! (`ACPI2_GUID`). The discovery chain is therefore: config table → RSDP →
//! `AcpiDesc::parse` (MADT **GICC** records, MPIDR as hw_id) — the same
//! three-step shape as the x86_64 variant of this test, with GICC instead
//! of LAPIC records.
//!
//! S-2b changes that made this work (all in minix-platform):
//! 1. `kind.rs`: the `RSDP` arm of `parse_by_kind` now compiles in for
//!    aarch64 (was x86_64-only).
//! 2. `acpi.rs`: `GicdNotFound` error variant added (the aarch64 MADT
//!    completeness gate referenced a never-declared variant); the IOAPIC
//!    record write is x86-gated to match its accumulator.
//! 3. `lib.rs`/`global.rs`: the `acpi` module and `PlatformDescEnum::Acpi`
//!    variant join aarch64.
//!
//! The DTB parser itself stays covered by the riscv64 variant (same
//! `device_tree.rs`); if a future AAVMF starts installing the FDT config
//! table, the DTB arm of `find_platform_sources` picks it up first by
//! design — this test would then pin the DTB chain instead (both arms end
//! in the same `cpu_topology()` contract below).

#![no_std]
#![no_main]

use core::arch::asm;
use core::panic::PanicInfo;
use minix_plat::arm64::early_console;
use minix_boot::PlatformDescSource;
use minix_platform::{parse_by_kind, PlatformDesc};
use uefi::prelude::*;

// UEFI test kernels allocate only while boot services are alive; boot-shim's
// pool allocator covers exactly that window (same rationale as test-memmap).
#[global_allocator]
static ALLOCATOR: boot_shim::uefi_helpers::UefiPoolAllocator =
    boot_shim::uefi_helpers::UefiPoolAllocator;

fn fail(msg: &str) -> ! {
    early_console::write_str("### FAIL: ");
    early_console::write_str(msg);
    early_console::write_str("\n");
    loop { unsafe { asm!("wfe", options(nomem, nostack)); } }
}

#[entry]
fn main() -> Status {
    early_console::write_str("### test_smp_topo (aarch64): topology discovery pin (ACPI via S-2b)\n");

    // 1. Discover platform sources through boot-shim's real scan
    //    (ACPI2 RSDP preferred over the absent DTB on this firmware).
    let sources = boot_shim::find_platform_sources();
    early_console::write_str("  platform sources: ");
    early_console::write_hex(sources.len() as u64);
    early_console::write_str("\n");
    if sources.is_empty() {
        fail("no platform source discovered (config table empty of RSDP/DTB)");
    }

    // 2. Parse through the same handoff path the kernel uses:
    //    kind tag → parser dispatch (`parse_by_kind`).
    let source = &sources[0];
    let desc = match unsafe { parse_by_kind(*source) } {
        Ok(d) => d,
        Err(_) => fail("platform source parse failed"),
    };
    let topo = desc.cpu_topology();
    early_console::write_str("  nr_cpus = ");
    early_console::write_hex(topo.nr_cpus as u64);
    early_console::write_str(", bsp mpdir/hw_id = ");
    early_console::write_hex(topo.bsp_id as u64);
    early_console::write_str("\n");

    // 3. Contract assertion: QEMU runs with -smp 4.
    if topo.nr_cpus != 4 {
        fail("nr_cpus != 4");
    }

    // 4. Contract assertion: hw_ids pairwise distinct (MPIDR on aarch64).
    for i in 0..topo.nr_cpus as usize {
        for j in 0..i {
            if topo.cpus[i].hw_id == topo.cpus[j].hw_id {
                fail("duplicate hw_id in topology");
            }
        }
    }

    // 5. Contract assertion: BSP hw_id is one of the discovered CPUs.
    let mut bsp_found = false;
    for cpu in &topo.cpus[..topo.nr_cpus as usize] {
        if cpu.hw_id == u64::from(topo.bsp_id) {
            bsp_found = true;
        }
    }
    if !bsp_found {
        fail("bsp_id not among discovered cpus");
    }

    early_console::write_str("### TEST_RESULT: PASS test-smp-topo-aarch64 ###\n");
    loop { unsafe { asm!("wfe", options(nomem, nostack)); } }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    early_console::write_str("### PANIC ###\n");
    loop { unsafe { asm!("wfe", options(nomem, nostack)); } }
}
