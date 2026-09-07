//! Test: AP early entry image toolchain spike (SMP bring-up S-3a) — x86_64.
//!
//! Proves the S-3a acceptance chain end to end: a `global_asm!`-defined
//! `.ap_early_entry` code section plus `.ap_early_entry_data` mailbox
//! section is (1) linked into the final PE/COFF image as real sections,
//! (2) free of relocations (checked offline with `objdump -h/-t/-r`, see
//! the commit's spike report), (3) **copyable** — the byte range
//! `[ap_early_entry_start, ap_early_entry_end)` copied to a fresh buffer
//! executes there and talks through its mailbox.
//!
//! Frozen ABI: `ApBootstrap` (minix_arch::arch::ap_early_entry) per
//! smp_todo §3.2. This spike runs the stub in 64-bit mode only — the
//! 16→32→64 ladder is S-3b's real body; the copy-and-execute property
//! under test here is ladder-independent.

#![no_std]
#![no_main]

use core::arch::asm;
use core::panic::PanicInfo;
use minix_arch::arch::ap_early_entry::BOOT_MAGIC_SENT;
use minix_arch::x86_64::ap_early_entry::{
    blob_range, mailbox_offset, BOOT_MAGIC_ACK, MAILBOX_HEADER_LEN,
};
extern crate alloc;
use minix_plat::x86_64::early_console;
use uefi::prelude::*;

#[global_allocator]
static ALLOCATOR: boot_shim::uefi_helpers::UefiPoolAllocator =
    boot_shim::uefi_helpers::UefiPoolAllocator;

fn fail(msg: &str) -> ! {
    early_console::write_str("### FAIL: ");
    early_console::write_str(msg);
    early_console::write_str("\n");
    loop { unsafe { asm!("int3", options(nomem, nostack)); } }
}

unsafe extern "C" {
    fn ap_early_entry(bootstrap_mailbox_va: usize);
}

#[entry]
fn main() -> Status {
    early_console::write_str("### test_smp_spike (x86_64): AP early entry copy-and-execute spike\n");

    // 1. Blob geometry — the spike report's core numbers.
    let (start, end) = blob_range();
    let off = mailbox_offset();
    let size = end - start;
    early_console::write_str("  image_start = "); early_console::write_hex(start as u64);
    early_console::write_str("\n  image_end   = "); early_console::write_hex(end as u64);
    early_console::write_str("\n  image_size  = "); early_console::write_hex(size as u64);
    early_console::write_str("\n  mailbox_off = "); early_console::write_hex(off as u64);
    early_console::write_str("\n");

    // 2. Adjacency contract (v8 #6 record): the mailbox must sit inside
    //    the blob and no foreign section may appear between the two — the
    //    SPIKE FINDING is that lld-link places `.ap_early_entry_data` at
    //    the next 4 KiB boundary (off = 0x1000 observed), so [start, end)
    //    is 4160 bytes of code + page-aligned gap + data. Self-contained
    //    as a copy unit; the gap costs a page of low memory — recorded,
    //    not "fixed" (two protections need two sections; page alignment
    //    is the linker's section characteristic).
    if !(size > 0 && off > 0 && off < size) {
        fail("section adjacency broken (mailbox not inside blob)");
    }
    if size > 8192 {
        fail("blob larger than two pages — copy unit unreasonable");
    }

    // 3. Copy the blob to a fresh buffer — the run-time address has nothing
    //    to do with the link-time addresses.
    let mut copy = alloc::vec![0u8; size];
    copy.copy_from_slice(unsafe {
        core::slice::from_raw_parts(start as *const u8, size)
    });
    let copy_base = copy.as_mut_ptr() as usize;
    let copy_mbox = copy_base + off;
    early_console::write_str("  copy_base   = "); early_console::write_hex(copy_base as u64);
    early_console::write_str("\n");

    // Diagnostic: original vs copy, first 24 bytes (buffered single write).
    let dump = |tag: &str, p: usize| {
        let mut buf = [b' '; 4 + 24 * 2 + 2];
        let mut w = 0usize;
        let hexc = |x: u8| -> u8 { if x < 10 { b'0' + x } else { b'a' + (x - 10) } };
        for b in 0..24usize {
            let v = unsafe { core::ptr::read_volatile((p + b) as *const u8) };
            buf[w] = hexc(v >> 4); w += 1;
            buf[w] = hexc(v & 0xF); w += 1;
        }
        early_console::write_str(tag);
        early_console::write_str(core::str::from_utf8(&buf[..w]).unwrap_or("?"));
        early_console::write_str("\n");
    };
    dump("  orig[0..24] = ", start);
    dump("  copy[0..24] = ", copy_base);

    // 4. BSP writes the SENT magic into the copied mailbox (§3.9: publish
    //    after fill; the spike runs one AP-equivalent call synchronously).
    let mbox = copy_mbox as *mut u64;
    unsafe {
        mbox.write_volatile(BOOT_MAGIC_SENT);
    }

    // 5. Execute the COPY at offset 0, passing the copied mailbox VA.
    let entry: unsafe extern "C" fn(usize) =
        unsafe { core::mem::transmute(copy_base) };
    unsafe { entry(copy_mbox) };

    // 6. Three proofs, read back from the copied mailbox.
    let ack = unsafe { core::ptr::read_volatile(mbox) };
    if ack != BOOT_MAGIC_ACK {
        early_console::write_str("  proof1 read ack="); early_console::write_hex(ack);
        early_console::write_str("\n");
        let dump = |tag: &str, p: usize| {
            let mut b2 = [b' '; 4 + 24 * 2];
            let mut w = 0usize;
            let hx = |x: u8| -> u8 { if x < 10 { b'0' + x } else { b'a' + (x - 10) } };
            for i in 0..24usize {
                let v = unsafe { core::ptr::read_volatile((p + i) as *const u8) };
                b2[w] = hx(v >> 4); w += 1;
                b2[w] = hx(v & 0xF); w += 1;
            }
            early_console::write_str(tag);
            early_console::write_str(core::str::from_utf8(&b2[..w]).unwrap_or("?"));
            early_console::write_str("\n");
        };
        dump("  mbox[0..24] = ", copy_mbox);
        fail("proof 1 failed: copied code did not write ACK");
    }
    let echo = unsafe { core::ptr::read_volatile(mbox.add(1)) };
    if echo != copy_mbox as u64 {
        fail("proof 2 failed: register-absolute echo mismatch");
    }
    let rip_proof = unsafe { core::ptr::read_volatile(mbox.add(2)) };
    if rip_proof != BOOT_MAGIC_ACK {
        fail("proof 3 failed: rip-relative span did not survive the copy");
    }
    early_console::write_str("  proofs: ack ✓ echo ✓ rip-rel ✓\n");
    early_console::write_str("### TEST_RESULT: PASS test-smp-spike ###\n");
    loop { unsafe { asm!("int3", options(nomem, nostack)); } }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    early_console::write_str("### PANIC ###\n");
    loop { unsafe { asm!("int3", options(nomem, nostack)); } }
}
