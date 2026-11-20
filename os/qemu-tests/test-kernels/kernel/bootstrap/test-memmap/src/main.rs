//! Test: KernelInfo.memmap reflects the real physical memory layout.
//!
//! Verifies that after UEFI boot, the memory map obtained via boot_helpers
//! contains at least one CONVENTIONAL region. If memmap is empty or has
//! no free RAM, the kernel cannot allocate memory for page tables.

#![no_std]
#![no_main]

use core::arch::asm;
use core::panic::PanicInfo;
use minix_plat::x86_64::early_console;
use boot_shim::uefi_helpers;
use uefi::prelude::*;

// UEFI test kernels allocate only while boot services are alive (build_memmap
// etc.); boot-shim's pool allocator covers exactly that window. See
// uefi_helpers::UefiPoolAllocator for why the registration lives here and not
// in boot-shim itself.
#[global_allocator]
static ALLOCATOR: boot_shim::uefi_helpers::UefiPoolAllocator =
    boot_shim::uefi_helpers::UefiPoolAllocator;

/// Raw `GetMemoryMap` scratch buffer for the I-29-9 probe: the snapshots
/// must not allocate (a pool buffer would shift the very pages under test).
#[repr(C, align(8))]
struct MapBuf([u8; 64 * 1024]);
static mut MAP_BUF: MapBuf = MapBuf([0; 64 * 1024]);

/// One raw memory-map snapshot's shape (descriptor count and stride).
struct RawSnap {
    n: usize,
    desc_size: usize,
}

/// Take a raw `GetMemoryMap` snapshot into [`MAP_BUF`]; no allocation.
fn raw_snapshot() -> RawSnap {
    unsafe {
        let st = uefi::table::system_table_raw().expect("i29-9: system table");
        let bt = (*st.as_ptr()).boot_services;
        let mut size = 64 * 1024usize;
        let mut key = 0usize;
        let mut desc_size = 0usize;
        let mut desc_version = 0u32;
        let buf = core::ptr::addr_of_mut!(MAP_BUF).cast::<u8>();
        let status = ((*bt).get_memory_map)(
            core::ptr::addr_of_mut!(size),
            buf.cast(),
            core::ptr::addr_of_mut!(key),
            core::ptr::addr_of_mut!(desc_size),
            core::ptr::addr_of_mut!(desc_version),
        );
        assert!(!status.is_error(), "i29-9: GetMemoryMap failed");
        assert!(desc_size >= 40, "i29-9: descriptor stride too small");
        RawSnap {
            n: size / desc_size,
            desc_size,
        }
    }
}

/// UEFI memory-type of the page containing `pa`, read from [`MAP_BUF`]
/// (v1 descriptor offsets: type 0, physical start 8, page count 24).
fn raw_type_at(snap: &RawSnap, pa: u64) -> Option<u32> {
    unsafe {
        let base = core::ptr::addr_of!(MAP_BUF).cast::<u8>();
        for i in 0..snap.n {
            let d = base.add(i * snap.desc_size);
            let ty = (d as *const u32).read_unaligned();
            let start = (d.add(8) as *const u64).read_unaligned();
            let pages = (d.add(24) as *const u64).read_unaligned();
            if pa >= start && pa < start + pages * 4096 {
                return Some(ty);
            }
        }
        None
    }
}

/// I-29-9 six-step probe (design note `01-design-note-memmap-consolidation`
/// §2.5.8): the third-party `boot::memory_map` buffer is a function-local
/// `MemoryMapOwned`; its `Drop` calls `free_pool` while boot services are
/// still alive. The design question is whether the drop returns the backing
/// pages to `CONVENTIONAL` before the next `GetMemoryMap` snapshot — if it
/// does, the classifier's reserved table over-reports memory that is
/// actually free.
///
/// Steps:
/// 1. raw snapshot before the allocation under test (static buffer, so the
///    snapshot itself allocates nothing and cannot shadow the probe);
/// 2. allocate the object under test (`boot::memory_map`);
/// 3. record its buffer PA and length;
/// 4. drop it (`free_pool` inside);
/// 5. raw snapshot after the drop;
/// 6. compare the types covering the buffer range in both snapshots.
fn i29_9_probe() {
    use uefi::mem::memory_map::{MemoryMap, MemoryType};

    const PAGE: u64 = 4096;
    let pre = raw_snapshot();
    let probe = uefi::boot::memory_map(MemoryType::LOADER_DATA).expect("i29-9: probe allocation");
    let pptr = probe.buffer().as_ptr() as u64;
    let plen = probe.buffer().len() as u64;
    let first = pptr & !(PAGE - 1);
    let last = (pptr + plen - 1) & !(PAGE - 1);
    let pre_first = raw_type_at(&pre, first);
    let pre_last = raw_type_at(&pre, last);

    early_console::write_str("[i29-9] probe buf=");
    early_console::write_hex(pptr);
    early_console::write_str(" len=");
    early_console::write_hex(plen);
    early_console::write_str("\n");
    early_console::write_str("[i29-9] pre first=");
    early_console::write_hex(first);
    early_console::write_str(" type=");
    early_console::write_hex(pre_first.unwrap_or(0xffff_ffff) as u64);
    early_console::write_str(" last=");
    early_console::write_hex(last);
    early_console::write_str(" type=");
    early_console::write_hex(pre_last.unwrap_or(0xffff_ffff) as u64);
    early_console::write_str(" n=");
    early_console::write_hex(pre.n as u64);
    early_console::write_str("\n");

    drop(probe);

    let post = raw_snapshot();
    let post_first = raw_type_at(&post, first);
    let post_last = raw_type_at(&post, last);
    early_console::write_str("[i29-9] post first type=");
    early_console::write_hex(post_first.unwrap_or(0xffff_ffff) as u64);
    early_console::write_str(" last type=");
    early_console::write_hex(post_last.unwrap_or(0xffff_ffff) as u64);
    early_console::write_str(" n=");
    early_console::write_hex(post.n as u64);
    early_console::write_str("\n");

    // UEFI type values: 2 = LOADER_DATA, 7 = CONVENTIONAL.
    const LOADER_DATA: u32 = 2;
    const CONVENTIONAL: u32 = 7;
    let verdict = match (pre_first, pre_last, post_first, post_last) {
        (Some(CONVENTIONAL), Some(CONVENTIONAL), Some(CONVENTIONAL), Some(CONVENTIONAL)) => {
            "RECLAIMED (reserved table would over-report)"
        }
        (_, _, Some(LOADER_DATA), Some(LOADER_DATA)) => "STILL_POOL (current behavior correct)",
        (_, _, Some(_), Some(_)) => "PARTIAL/OTHER (inspect the printed types)",
        _ => "INCONCLUSIVE (range not described)",
    };
    early_console::write_str("[i29-9] verdict=");
    early_console::write_str(verdict);
    early_console::write_str("\n");
}

#[entry]
fn main() -> Status {
    early_console::write_str("### test_memmap: verifying UEFI memory map...\n");

    // I-29-9 six-step probe (design §2.5.8) — before the production-shape
    // snapshot below, so its allocations do not perturb what this carrier
    // asserts about the classifier.
    i29_9_probe();

    // 1. Get UEFI memory map (before ExitBootServices)
    let memmap = uefi_helpers::build_memmap();

    // Assertion 1: memmap is non-empty
    if memmap.is_empty() {
        early_console::write_str("### FAIL: memmap is empty\n");
        loop { unsafe { asm!("hlt", options(nomem, nostack)); } }
    }
    early_console::write_str("  memmap entries: ");
    early_console::write_hex(memmap.len() as u64);
    early_console::write_str("\n");

    // Assertion 2: at least one region has non-zero size
    let total: u64 = memmap.iter().map(|r| r.len as u64).sum();
    if total == 0 {
        early_console::write_str("### FAIL: total memmap size is 0\n");
        loop { unsafe { asm!("hlt", options(nomem, nostack)); } }
    }
    early_console::write_str("  total CONVENTIONAL memory: ");
    early_console::write_hex(total);
    early_console::write_str("\n");

    // Assertion 3: at least one region starts below 4GB (expected in QEMU)
    let mut has_low_mem = false;
    for region in memmap {
        let r_start = region.base.0;
        let r_end = r_start + region.len as u64;
        early_console::write_str("  region: ");
        early_console::write_hex(r_start);
        early_console::write_str("..");
        early_console::write_hex(r_end);
        early_console::write_str("\n");
        if r_start < 0x1_0000_0000 {
            has_low_mem = true;
        }
    }
    if !has_low_mem {
        early_console::write_str("### FAIL: no CONVENTIONAL region below 4GB\n");
        loop { unsafe { asm!("hlt", options(nomem, nostack)); } }
    }

    early_console::write_str("  memmap is valid: non-empty, has free RAM below 4GB\n");
    early_console::write_str("### TEST_RESULT: PASS test-memmap ###\n");
    loop { unsafe { asm!("hlt", options(nomem, nostack)); } }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    early_console::write_str("### PANIC ###\n");
    loop { unsafe { asm!("hlt", options(nomem, nostack)); } }
}
