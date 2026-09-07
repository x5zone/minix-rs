//! Test: AP alive verification (SMP bring-up S-3d, ladder L2) — x86_64.
//!
//! The first real-machine execution of the S-3b ladder: the BSP installs
//! the early entry image at `AP_STARTUP_PA` (0x8000), fills the bootstrap
//! record (root = the current UEFI identity CR3, per-AP stack, the test's
//! Rust entry), then sends INIT-SIPI-SIPI via `SmpArch::boot_ap`. The AP
//! climbs 16→32→64, reads the record, publishes its handshake bit, parks.
//!
//! PASS = the BSP observes the AP's `boot_ack` bit within the timeout
//! window. This is the L2 rung of the QEMU ladder (smp_todo §6).
//!
//! Note (test/hardware seam): the record fill writes through the UEFI
//! identity map directly (PA == VA) instead of the arch's Direct-Map-based
//! `fill_bootstrap` — the test runs before any kernel DM window exists.
//! Store sequence and offsets mirror `fill_bootstrap` exactly.

#![no_std]
#![no_main]

use core::arch::asm;
use core::panic::PanicInfo;
use core::sync::atomic::{AtomicU64, Ordering};
use minix_arch::arch::ap_early_entry::{ApBootstrap, BOOT_MAGIC_SENT};
use minix_arch::smp::SmpArch;
use minix_arch::x86_64::ap_early_entry::{OFF_MAGIC, OFF_RECORD, AP_STARTUP_PA};
use minix_arch::x86_64::smp::X86_64SmpArch;
use minix_plat::x86_64::early_console;
use uefi::prelude::*;

#[global_allocator]
static ALLOCATOR: boot_shim::uefi_helpers::UefiPoolAllocator =
    boot_shim::uefi_helpers::UefiPoolAllocator;

/// The AP's acknowledgement: bit n set == AP with logical_id n reached the
/// Rust entry and consumed the bootstrap record.
static BOOT_ACK: AtomicU64 = AtomicU64::new(0);

/// Diagnostic markers (S-3d bring-up): the AP writes progress codes here.
/// 0xA1 = reached the 64-bit Rust entry; 0xA2 = magic OK, ack published;
/// 0xA4 = magic mismatch (value in the low byte is the observed magic's
/// first byte). The BSP prints this at timeout.
static MARKER: AtomicU64 = AtomicU64::new(0);

/// Per-AP kernel stack (64 KiB, BSS). The ladder loads its top via the
/// record; under the UEFI identity map the static's VA == PA, so the AP
/// (running on the test's CR3) can use it directly.
static mut AP_STACK: [u8; 0x10000] = [0u8; 0x10000];

/// The AP's Rust convergence point — where the ladder's `jmp rax` lands.
/// Mirrors the frozen `ap_early_entry` contract: snapshot what the record
/// says, publish the handshake, park with interrupts off.
unsafe extern "C" fn ap_entry_test(bootstrap_pa: usize) -> ! {
    let magic = core::ptr::read_volatile((bootstrap_pa + OFF_MAGIC) as *const u64);
    MARKER.store(0xA1, Ordering::Release);
    if magic == BOOT_MAGIC_SENT {
        // logical_id = 1 (the first AP) → bit 1.
        BOOT_ACK.fetch_or(1 << 1, Ordering::Release);
        MARKER.store(0xA2, Ordering::Release);
    } else {
        MARKER.store(0xA4 | (magic as u64 & 0xFF), Ordering::Release);
    }
    // Park: no IDT, no IRQ reality until S-8 — stay quiet.
    loop {
        asm!("cli", options(nomem, nostack));
        asm!("hlt", options(nomem, nostack));
    }
}

fn fail(msg: &str) -> ! {
    early_console::write_str("### FAIL: ");
    early_console::write_str(msg);
    early_console::write_str("\n");
    loop { unsafe { asm!("cli", options(nomem, nostack)); } }
}

/// LAPIC spurious-interrupt register: enable the LAPIC (bit 8) so INIT/SIPI
/// generation works even if the firmware left it disabled.
const LAPIC_BASE: usize = 0xFEE0_0000;

#[entry]
fn main() -> Status {
    early_console::write_str("### test_smp_ap_alive (x86_64): AP boot + handshake (S-3d)\n");

    // 1. Enable the BSP LAPIC (spurious-interrupt register, bit 8 = enable),
    //    then read it back — a dead MMIO window reads all-ones/all-zero,
    //    which would explain a hang in the IPI path.
    unsafe {
        let spur = (LAPIC_BASE + 0xF0) as *mut u32;
        spur.write_volatile(0x1FF);
        let rb = spur.read_volatile();
        early_console::write_str("  lapic spurious readback = ");
        early_console::write_hex(rb as u64);
        early_console::write_str("\n");
        let icr = (LAPIC_BASE + 0x300) as *mut u32;
        early_console::write_str("  lapic icr-low readback = ");
        early_console::write_hex(icr.read_volatile() as u64);
        early_console::write_str("\n");
    }
    early_console::write_str("  before boot_ap\n");

    // 2. Current root page table: the UEFI identity map — the AP continues
    //    at the low identity addresses the ladder leaves it at.
    let root: u64;
    unsafe { core::arch::asm!("mov {}, cr3", out(reg) root, options(nomem)); }
    early_console::write_str("  root(cr3) = "); early_console::write_hex(root);
    early_console::write_str("\n");

    // 3. Install the early entry image + fill the bootstrap record.
    unsafe { minix_arch::x86_64::ap_early_entry::install_at(AP_STARTUP_PA as usize) };
    let base = AP_STARTUP_PA as usize;
    let record = ApBootstrap {
        logical_id: 1,
        _pad: 0,
        hw_id: 1,
        page_table_root_pa: root,
        kernel_stack_top_va: (core::ptr::addr_of!(AP_STACK) as usize) as u64 + 0x10000,
        rust_entry_va: ap_entry_test as usize as u64,
    };
    // The data area sits at fixed blob offsets (hand-frozen layout —
    // 16-smp §9); mirror fill_bootstrap exactly.
    let mbox = base + OFF_MAGIC;
    unsafe {
        core::ptr::write_volatile(mbox as *mut u64, BOOT_MAGIC_SENT);
        core::ptr::write_volatile(
            (mbox + (OFF_RECORD - OFF_MAGIC)) as *mut ApBootstrap,
            record,
        );
        core::ptr::write_volatile(mbox as *mut u64, BOOT_MAGIC_SENT);
    }
    // The publisher fence (x86 mfence, §3.9) is inside boot_ap — S-3c fix.

    // 4. Wake AP 1: INIT-SIPI-SIPI through the arch trait (the S-3b ladder
    //    runs on the AP; it publishes BOOT_ACK bit 1 from the Rust entry).
    <X86_64SmpArch as SmpArch>::boot_ap(1, AP_STARTUP_PA as usize);

    // 5. BSP observes the handshake with a bounded wait (§3.4: bounded
    //    per-AP startup timeout — the count is a generous spin).
    // Bounded wait — sized for TCG (emulated) execution: the runner kills
    // QEMU at ~30 s, so the loop must exhaust in a few seconds under
    // emulation, not real-hardware timings.
    let mut spins: u64 = 0;
    while BOOT_ACK.load(Ordering::Acquire) == 0 {
        spins += 1;
        if spins > 4_000_000 {
            // LAPIC diagnostics at timeout: delivery status (ICR bit 12)
            // and the error status register (write 0 first per SDM §10.5.3)
            // — a set ESR bit 2 (send illegal vector) would mean the SIPI
            // was rejected rather than lost.
            unsafe {
                let icr = (LAPIC_BASE + 0x300) as *mut u32;
                let esr = (LAPIC_BASE + 0x280) as *mut u32;
                esr.write_volatile(0);
                let esrv = esr.read_volatile();
                early_console::write_str("  icr@timeout = ");
                early_console::write_hex(icr.read_volatile() as u64);
                early_console::write_str(" esr = ");
                early_console::write_hex(esrv as u64);
                early_console::write_str("\n");
            }
            early_console::write_str("  marker = ");
            early_console::write_hex(MARKER.load(Ordering::Acquire));
            early_console::write_str(" ack = ");
            early_console::write_hex(BOOT_ACK.load(Ordering::Acquire));
            early_console::write_str("\n");
            fail("AP did not publish boot_ack within the timeout window");
        }
        core::hint::spin_loop();
    }

    early_console::write_str("  marker = "); early_console::write_hex(MARKER.load(Ordering::Acquire));
    early_console::write_str("\n  AP published boot_ack (bit 1) — ladder verified\n");
    early_console::write_str("### TEST_RESULT: PASS test-smp-ap-alive ###\n");
    loop { unsafe { asm!("cli", options(nomem, nostack)); } }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    early_console::write_str("### PANIC ###\n");
    loop { unsafe { asm!("cli", options(nomem, nostack)); } }
}
