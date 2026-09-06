//! Test: topology discovery pin (SMP bring-up S-2, ladder L1) — x86_64/MADT.
//!
//! Pins the D-36 upper half under real QEMU `-smp 4`: the UEFI configuration
//! table hands us the RSDP, `parse_by_kind` walks XSDT → MADT, and the
//! resulting `CpuTopology` must describe exactly the CPUs QEMU exposed —
//! four of them, unique APIC IDs, BSP among the discovered set.
//!
//! Contract vs. observation: `nr_cpus == 4`, ID uniqueness and BSP membership
//! are the test contract. The concrete APIC ID values (QEMU default {0,1,2,3})
//! are printed as machine-specific observation only — QEMU `-smp
//! sockets=…,cores=…` topologies may legally differ and must not fail the
//! parser pin.

#![no_std]
#![no_main]

use core::arch::asm;
use core::cell::Cell;
use core::panic::PanicInfo;
use minix_plat::x86_64::early_console;
use minix_boot::PlatformDescSource;
use minix_platform::PlatformDesc;
use minix_platform::kind::{parse_by_kind, RSDP};
use minix_types::PhysBytes;
use uefi::prelude::*;
use uefi::table::cfg::{ACPI2_GUID, ACPI_GUID};

// UEFI test kernels allocate only while boot services are alive; boot-shim's
// pool allocator covers exactly that window (same rationale as test-memmap).
#[global_allocator]
static ALLOCATOR: boot_shim::uefi_helpers::UefiPoolAllocator =
    boot_shim::uefi_helpers::UefiPoolAllocator;

fn fail(msg: &str) -> ! {
    early_console::write_str("### FAIL: ");
    early_console::write_str(msg);
    early_console::write_str("\n");
    loop { unsafe { asm!("hlt", options(nomem, nostack)); } }
}

#[entry]
fn main() -> Status {
    early_console::write_str("### test_smp_topo: discovering topology (RSDP → MADT)...\n");

    // 1. RSDP physical address from the UEFI configuration table
    //    (ACPI 2.0 preferred, 1.0 fallback). Must run before ExitBootServices.
    let rsdp = Cell::new(None);
    system::with_config_table(|entries| {
        for e in entries {
            if e.guid == ACPI2_GUID || e.guid == ACPI_GUID {
                rsdp.set(Some(e.address as u64));
                break;
            }
        }
    });
    let Some(rsdp_phys) = rsdp.get() else {
        fail("no RSDP in UEFI config table");
    };
    early_console::write_str("  RSDP @ ");
    early_console::write_hex(rsdp_phys);
    early_console::write_str("\n");

    // 2. Parse through the same handoff path the kernel uses: kind tag →
    //    parser dispatch (`parse_by_kind`), not an ACPI-specific shortcut.
    let source = PlatformDescSource::new(RSDP, PhysBytes(rsdp_phys));
    let desc = match unsafe { parse_by_kind(source) } {
        Ok(d) => d,
        Err(_) => fail("ACPI/MADT parse failed"),
    };
    let topo = desc.cpu_topology();
    early_console::write_str("  nr_cpus = ");
    early_console::write_hex(topo.nr_cpus as u64);
    early_console::write_str(", bsp hw_id = ");
    early_console::write_hex(topo.bsp_id as u64);
    early_console::write_str("\n");

    // 3. Contract assertion: QEMU runs with -smp 4.
    if topo.nr_cpus != 4 {
        fail("nr_cpus != 4");
    }

    // 4. Contract assertion: hardware IDs pairwise distinct — a parser that
    //    collapses LAPIC entries or double-counts one fails here.
    for i in 0..topo.nr_cpus as usize {
        for j in 0..i {
            if topo.cpus[i].hw_id == topo.cpus[j].hw_id {
                fail("duplicate hw_id among discovered CPUs");
            }
        }
    }

    // 5. Contract assertion: BSP hardware ID belongs to the discovered set —
    //    a MADT walk that skips the BSP entry (or invents IDs) fails here.
    let mut bsp_found = false;
    for i in 0..topo.nr_cpus as usize {
        if topo.cpus[i].hw_id == topo.bsp_id as u64 {
            bsp_found = true;
        }
    }
    if !bsp_found {
        fail("bsp hw_id not among discovered CPUs");
    }

    // 6. Machine-specific observation (printed, never asserted): QEMU's
    //    default topology assigns APIC IDs {0,1,2,3}.
    for i in 0..topo.nr_cpus as usize {
        early_console::write_str("  cpu[");
        early_console::write_hex(i as u64);
        early_console::write_str("] hw_id = ");
        early_console::write_hex(topo.cpus[i].hw_id);
        if topo.cpus[i].hw_id == topo.bsp_id as u64 {
            early_console::write_str(" (BSP)");
        }
        early_console::write_str("\n");
    }

    early_console::write_str("### TEST_RESULT: PASS test-smp-topo ###\n");
    loop { unsafe { asm!("hlt", options(nomem, nostack)); } }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    early_console::write_str("### PANIC ###\n");
    loop { unsafe { asm!("hlt", options(nomem, nostack)); } }
}
