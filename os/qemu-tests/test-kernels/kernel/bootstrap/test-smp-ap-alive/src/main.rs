//! Test: AP alive verification (SMP bring-up S-3d, ladder L2) — x86_64.
//!
//! BSP installs the ladder blob at 0x8000, fills the bootstrap record in
//! the scratch page at 0x9000, sends INIT-SIPI. The AP climbs 16→32→64
//! and publishes its handshake bit. PASS = BSP observes the ack bit.

#![no_std]
#![no_main]

use core::arch::asm;
use core::panic::PanicInfo;
use core::sync::atomic::{AtomicU64, Ordering};
use minix_arch::arch::ap_early_entry::{ApBootstrap, BOOT_MAGIC_SENT};
use minix_arch::smp::SmpArch;
use minix_arch::x86_64::ap_early_entry::install_at;
use minix_arch::x86_64::smp::X86_64SmpArch;
use minix_plat::x86_64::early_console;
use uefi::prelude::*;

#[global_allocator]
static ALLOCATOR: boot_shim::uefi_helpers::UefiPoolAllocator =
    boot_shim::uefi_helpers::UefiPoolAllocator;

/// Bit n set == AP logical_id n reached the Rust entry.
static BOOT_ACK: AtomicU64 = AtomicU64::new(0);

static mut AP_STACK: [u8; 0x10000] = [0u8; 0x10000];

unsafe extern "C" fn ap_entry(_bootstrap_pa: usize) -> ! {
    BOOT_ACK.fetch_or(1 << 1, Ordering::Release);
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

#[entry]
fn main() -> Status {
    early_console::write_str("### test_smp_ap_alive (x86_64): S-3d\n");

    // Enable LAPIC.
    unsafe {
        let spur = (0xFEE0_0000_usize + 0xF0_usize) as *mut u32;
        spur.write_volatile(0x1FF);
    }

    // Read UEFI CR3 (identity map root).
    let root: u64;
    unsafe { core::arch::asm!("mov {}, cr3", out(reg) root, options(nomem)); }

    // Install the ladder blob at 0x8000.
    unsafe { install_at(0x8000) };

    // Fill the ApBootstrap record in the scratch page at 0x9008.
    let record = ApBootstrap {
        logical_id: 1,
        _pad: 0,
        hw_id: 0x11,
        page_table_root_pa: root,
        kernel_stack_top_va: (unsafe { core::ptr::addr_of!(AP_STACK) as usize } as u64) + 0x10000,
        rust_entry_va: ap_entry as u64,
    };
    unsafe {
        // Serialize the record to scratch page at 0x9008, then publish
        // magic at 0x9000 (fill_bootstrap contract, minus the DM lookup).
        core::ptr::write_volatile(0x9008 as *mut ApBootstrap, record);
        core::ptr::write_volatile(0x9000 as *mut u64, BOOT_MAGIC_SENT);
    }

    // Send INIT-SIPI to wake AP 1.
    <X86_64SmpArch as SmpArch>::boot_ap(1, 0x8000);

    // Observe the handshake.
    let mut spins: u64 = 0;
    while BOOT_ACK.load(Ordering::Acquire) == 0 {
        spins += 1;
        if spins > 20_000_000 {
            fail("AP did not publish boot_ack within timeout");
        }
        core::hint::spin_loop();
    }

    early_console::write_str("### TEST_RESULT: PASS test-smp-ap-alive ###\n");
    loop { unsafe { asm!("cli", options(nomem, nostack)); } }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    early_console::write_str("### PANIC ###\n");
    loop { unsafe { asm!("cli", options(nomem, nostack)); } }
}
