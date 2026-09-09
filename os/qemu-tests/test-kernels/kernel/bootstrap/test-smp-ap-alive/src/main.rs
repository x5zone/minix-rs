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
    early_console::write_str("### AP IN RUST (ladder complete)\n");
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
    // S-3d diagnostic: the ladder's stage-mark byte at 0x7000 names the
    // dead stage (0xA1 real-mode entry .. 0xA7 Rust entry, 0 = never ran).
    let stage = unsafe { core::ptr::read_volatile(0x6F00 as *const u8) };
    let ip = unsafe { core::ptr::read_volatile(0x6F04 as *const u16) };
    let cs = unsafe { core::ptr::read_volatile(0x6F06 as *const u16) };
    early_console::write_str("### AP stage: 0x");
    early_console::write_hex(stage as u64);
    early_console::write_str(" #UD at CS:IP = 0x");
    early_console::write_hex(((cs as u64) << 16) | ip as u64);
    early_console::write_str("\n");
    loop { unsafe { asm!("cli", options(nomem, nostack)); } }
}

#[entry]
fn main() -> Status {
    early_console::write_str("### test_smp_ap_alive (x86_64): S-3d\n");

    early_console::write_str("### SCRATCH_LIN (linked): 0x");
    early_console::write_hex(minix_arch::x86_64::ap_early_entry::SCRATCH_LIN as u64);
    early_console::write_str(" AP_BASE: 0x");
    early_console::write_hex(minix_arch::x86_64::ap_early_entry::AP_STARTUP_PA);
    early_console::write_str("\n");

    // Enable LAPIC — and diagnose its mode first (S-3d): if OVMF left the
    // LAPIC in x2APIC mode (IA32_APIC_BASE bit 10), xAPIC MMIO writes are
    // ignored and the INIT-SIPI below would silently never send. Print
    // APIC_BASE and an MMIO round-trip (LAPIC_ID read).
    let apic_base: u64;
    unsafe {
        let lo: u32;
        let hi: u32;
        core::arch::asm!(
            "rdmsr", in("ecx") 0x1Bu32, out("eax") lo, out("edx") hi,
            options(nomem, nostack)
        );
        apic_base = ((hi as u64) << 32) | lo as u64;
    }
    early_console::write_str("### APIC_BASE: 0x");
    early_console::write_hex(apic_base);
    early_console::write_str("\n");
    let lapic_id = unsafe { core::ptr::read_volatile((0xFEE0_0000_usize + 0x20) as *const u32) };
    early_console::write_str("### LAPIC_ID (mmio): 0x");
    early_console::write_hex(lapic_id as u64);
    early_console::write_str("\n");

    // Enable LAPIC.
    unsafe {
        let spur = (0xFEE0_0000_usize + 0xF0_usize) as *mut u32;
        spur.write_volatile(0x1FF);
    }

    // Read UEFI CR3 (identity map root).
    let root: u64;
    unsafe { core::arch::asm!("mov {}, cr3", out(reg) root, options(nomem)); }

    // Fill the ApBootstrap record in the scratch page at 0x9008.
    let record = ApBootstrap {
        logical_id: 1,
        _pad: 0,
        hw_id: 0x11,
        page_table_root_pa: root,
        kernel_stack_top_va: (unsafe { core::ptr::addr_of!(AP_STACK) as usize } as u64) + 0x10000,
        rust_entry_va: ap_entry as u64,
    };
    // S-3d root cause #2 (2026-09-09): the hand-rolled stores above wrote
    // only record + magic — the GDT descriptor/table at 0x9030/0x9110
    // stayed garbage, so the ladder's lgdt loaded a broken descriptor and
    // the CS load at the far jump triple-faulted the AP (stage mark stuck
    // at 0xA2, looping on the second SIPI). Use fill_bootstrap, which
    // writes the GDT too and publishes the magic last.
    unsafe { install_at(0x5000) };
    unsafe { minix_arch::x86_64::ap_early_entry::fill_bootstrap(0x6000, &record); }

    // S-3d diagnosis: arm a real-mode #UD collector. IVT[6] (0x18) gains a
    // handler at 0x7400 that pops FLAGS/CS/IP off the real-mode frame,
    // records the faulting CS:IP at 0x7004/0x7006, marks 0x7000 = 0xEE and
    // halts. If the AP #UDs anywhere in the ladder, the BSP timeout print
    // names the exact faulting address.
    unsafe {
        let handler: [u8; 16] = [
            0x58,             // pop  ax (flags)
            0x58,             // pop  ax (IP)
            0xA3, 0x04, 0x6F, // mov  [0x7004], ax
            0x58,             // pop  ax (CS)
            0xA3, 0x06, 0x6F, // mov  [0x7006], ax
            0xC6, 0x06, 0x00, 0x6F, 0xEE, // mov byte [0x7000], 0xEE
            0xFA,             // cli
            0xF4,             // hlt
        ];
        core::ptr::copy_nonoverlapping(handler.as_ptr(), 0x7400 as *mut u8, 16);
        core::ptr::write_volatile(0x18 as *mut u16, 0x7400); // IVT[6] offset
        core::ptr::write_volatile(0x1A as *mut u16, 0x0000); // IVT[6] segment
        core::ptr::write_volatile(0x7004 as *mut u16, 0);
        core::ptr::write_volatile(0x7006 as *mut u16, 0);
    }

    // Clear the ladder stage-mark byte, then send INIT-SIPI.
    unsafe { core::ptr::write_volatile(0x6F00 as *mut u8, 0); }
    // Send INIT-SIPI to wake AP 1.
    <X86_64SmpArch as SmpArch>::boot_ap(1, 0x5000);

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
fn panic(info: &PanicInfo) -> ! {
    early_console::write_str("### PANIC ###\n");
    if let Some(loc) = info.location() {
        early_console::write_str("### at ");
        early_console::write_str(loc.file());
        early_console::write_str(":");
        early_console::write_hex(loc.line() as u64);
        early_console::write_str("\n");
    }
    let msg = info.message();
    early_console::write_str("### msg: ");
    // core::fmt Arguments 不能轻易手打：打印其 pieces 指针不足以诊断，
    // 直接用 format 不行（no_std 无 alloc）——退而打印 location 已够。
    let _ = msg;
    early_console::write_str("\n");
    loop { unsafe { asm!("cli", options(nomem, nostack)); } }
}
