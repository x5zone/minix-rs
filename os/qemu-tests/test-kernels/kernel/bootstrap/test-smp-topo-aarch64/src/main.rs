//! Test: topology discovery pin — aarch64/ACPI.
//!
//! **The discovery path is ACPI, not DTB on AAVMF.**
//!
//! Empirically verified on QEMU virt + AAVMF (Ubuntu 2024.02, `-smp 4`):
//! the config table carries **no FDT entry** under either the UEFI-spec
//! `EFI_DEVICE_TREE_GUID` (b1b621d5-f19c-41a5-830b-d9152c69aae0) or the
//! boot-shim's legacy GUID (b1b621d2-f19c-41c5-8310-daa6f018a8d3) — but it
//! **does carry the ACPI 2.0 RSDP GUID** (`ACPI2_GUID`). The discovery chain
//! is therefore: config table → RSDP → `AcpiDesc::parse` (MADT **GICC**
//! records, Processor UID as hw_id) — the same
//! three-step shape as the x86_64 variant of this test, with GICC instead
//! of LAPIC records.
//!
//! Changes that made this work (all in minix-platform):
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

/// Diagnostic print: `parse_by_kind` flattens inner errors into
/// `PlatformParseError::AcpiParse`, so a failing serial log shows only
/// "parse failed" and cannot distinguish a missing RSDP, an absent MADT, or a
/// rejection by `check_gic_madt` (the aarch64 GIC gate). Here the same RSDP
/// goes through `AcpiDesc::parse` again to print each variant. **The criterion
/// is unchanged** — after printing, the original fail branch still runs.
fn dump_acpi_parse_error(source: &PlatformDescSource) {
    if !matches!(source.kind(), minix_platform::RSDP) {
        early_console::write_str("  diag: source kind is not RSDP, acpi dump skipped\n");
        return;
    }
    early_console::write_str("  diag: AcpiParseError = ");
    // SAFETY: the same phys_addr that `parse_by_kind` just dereferenced, and
    // still inside the boot-services lifetime window (this test kernel never
    // exits boot services).
    match unsafe { minix_platform::AcpiDesc::parse(source.phys_addr().0 as usize) } {
        Ok(_) => early_console::write_str("unexpected Ok (dispatch/parser disagree)\n"),
        Err(minix_platform::AcpiParseError::BadRsdpSignature) => {
            early_console::write_str("BadRsdpSignature\n");
        }
        Err(minix_platform::AcpiParseError::NoXsdtPointer) => {
            early_console::write_str("NoXsdtPointer\n");
        }
        Err(minix_platform::AcpiParseError::MadtNotFound) => {
            early_console::write_str("MadtNotFound\n");
        }
        Err(minix_platform::AcpiParseError::MadtTooShort) => {
            early_console::write_str("MadtTooShort\n");
        }
        Err(minix_platform::AcpiParseError::GicdNotFound) => {
            early_console::write_str("GicdNotFound\n");
        }
        Err(minix_platform::AcpiParseError::GicrNotFound) => {
            early_console::write_str("GicrNotFound\n");
        }
        Err(minix_platform::AcpiParseError::GicVersionUnsupported(v)) => {
            early_console::write_str("GicVersionUnsupported(");
            early_console::write_hex(u64::from(v));
            early_console::write_str(")\n");
        }
        Err(_) => early_console::write_str("other AcpiParseError\n"),
    }
}

#[entry]
fn main() -> Status {
    early_console::write_str("### test_smp_topo (aarch64): topology discovery pin (ACPI)\n");

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
        Err(_) => {
            dump_acpi_parse_error(source);
            fail("platform source parse failed")
        }
    };
    let topo = desc.cpu_topology();
    early_console::write_str("  nr_cpus = ");
    early_console::write_hex(topo.nr_cpus as u64);
    early_console::write_str(", bsp mpidr/hw_id = ");
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

    // 5. Contract assertion: the firmware's BSP hint is one of the
    //    discovered CPUs.
    let mut bsp_found = false;
    for cpu in &topo.cpus[..topo.nr_cpus as usize] {
        if cpu.hw_id == u64::from(topo.bsp_id) {
            bsp_found = true;
        }
    }
    if !bsp_found {
        fail("bsp_id not among discovered cpus");
    }

    // 5b. Contract assertion: the CPU executing this test (MPIDR_EL1.Aff0)
    //     is in the discovered set. `bsp_id` is only a hint.
    let mpidr: u64;
    unsafe { asm!("mrs {}, mpidr_el1", out(reg) mpidr) };
    let aff0 = mpidr & 0xff;
    let mut self_found = false;
    for cpu in &topo.cpus[..topo.nr_cpus as usize] {
        if cpu.hw_id == aff0 {
            self_found = true;
        }
    }
    if !self_found {
        fail("executing CPU MPIDR.Aff0 not among discovered cpus");
    }

    early_console::write_str("### TEST_RESULT: PASS test-smp-topo-aarch64 ###\n");
    loop { unsafe { asm!("wfe", options(nomem, nostack)); } }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    early_console::write_str("### PANIC ###\n");
    loop { unsafe { asm!("wfe", options(nomem, nostack)); } }
}
