//! Test: verify high-half kernel mapping after arch_boot_impl.
//!
//! After arch_boot_impl, the kernel's high-half virtual address range
//! [kern_virt_base, kern_virt_base + kern_size) should be mapped to
//! [kern_phys_base, kern_phys_base + kern_size). We write a sentinel
//! at a known low address, read it back via the high-half alias, and
//! compare.
//!
//! N-5 probe (print-only, does not affect PASS): a LOADER_DATA page allocated
//! *after* the memmap snapshot below 4 GiB is filled with a tagged pattern and
//! checksummed before EBS, after EBS and after paging is enabled. `replayed`
//! means the bootstrap root replays the 1:1 mapping and the bytes survive the
//! switch; `in_snapshot_free` reports whether the snapshot's conventional list
//! still advertises that physical range as free (the reuse precondition).

#![no_std]
#![no_main]

use core::arch::asm;
use core::panic::PanicInfo;
use minix_arch::x86_64::paging::X86_64Paging;
use minix_plat::x86_64::early_console;
use minix_kernel::boot_alloc;
use minix_arch::pt_alloc;
use minix_types::{PhysBytes, VirBytes};
use minix_boot::{BootPrepareResult, KernelInfo};
use boot_shim::uefi_helpers;
use uefi::boot::AllocateType;
use uefi::mem::memory_map::MemoryType;
use uefi::prelude::*;

/// Sentinel value written to low memory, read back via high-half alias.
const SENTINEL: u64 = 0xdeadbeef_cafe0001;

/// N-5 probe: pattern word with a position tag, so partial overwrites also
/// show up in the checksum.
const PROBE_WORD: u64 = 0xA5A5_0000_0000_0000;

/// Sum of the 512 words at `pa` (volatile so the probe measures memory, not
/// the compiler's view of it).
fn probe_checksum(pa: u64) -> u64 {
    let mut sum = 0u64;
    for i in 0..512u64 {
        // SAFETY: `pa` is a page this kernel allocated and filled itself.
        let w = unsafe { core::ptr::read_volatile((pa as *const u64).add(i as usize)) };
        sum = sum.wrapping_add(w);
    }
    sum
}

// UEFI test kernels allocate only while boot services are alive (build_memmap
// etc.); boot-shim's pool allocator covers exactly that window. See
// uefi_helpers::UefiPoolAllocator for why the registration lives here and not
// in boot-shim itself.
#[global_allocator]
static ALLOCATOR: boot_shim::uefi_helpers::UefiPoolAllocator =
    boot_shim::uefi_helpers::UefiPoolAllocator;

#[entry]
fn main() -> Status {
    early_console::write_str("### test_kernel_map: verify high-half mapping\n");

    // 1. UEFI boot preparation — use individual helpers (no kernel.elf needed)
    let memmap = uefi_helpers::build_memmap();
    let root_page = uefi_helpers::alloc_root_page();
    let (bump_base, bump_end) = uefi_helpers::alloc_bump_region(64);

    let kern_virt_base: u64 = 0xFFFF_8000_0000_0000;
    let kern_phys_base: u64 = 0x200_000; // 2MB-aligned for 2MB huge pages
    let kern_size: u64 = 0x200_000;

    let kernel_info = KernelInfo {
        memmap,
        kern_virt_base: VirBytes(kern_virt_base),
        kern_phys_base: PhysBytes(kern_phys_base),
        kern_size,
        free_upper_idx: None,
        user_sp: VirBytes(0x0000_7fff_ffff_f000),
        kern_stack_top: VirBytes(kern_virt_base + kern_size as u64),
        syscall_entry: VirBytes(kern_virt_base),
        boot_modules: &[],
        bootstrap_start: PhysBytes(0),
        bootstrap_len: 0,
        platform_sources: &[],
        param_buf: &[],
        reserved_regions: &[],
    };

    let result = BootPrepareResult {
        kernel_info,
        root_page,
        bump_base,
        bump_end,
    };

    // N-5 probe: a LOADER_DATA page allocated *after* the memmap snapshot,
    // below 4 GiB — the historical shape of the shim's pool payload pages
    // (allocated after GetMemoryMap, holding KernelInfo slice bytes). The
    // three checksums below discriminate "the new root does not replay the
    // 1:1 mapping" from "the page itself was zeroed".
    let probe_page = uefi::boot::allocate_pages(
        AllocateType::MaxAddress(0x1_0000_0000 - 0x1000),
        MemoryType::LOADER_DATA,
        1,
    )
    .expect("n5 probe: allocate_pages failed");
    let probe_pa = probe_page.as_ptr() as u64;
    for i in 0..512u64 {
        // SAFETY: fresh LOADER_DATA page; identity/firmware mapping is live.
        unsafe {
            core::ptr::write_volatile((probe_pa as *mut u64).add(i as usize), PROBE_WORD | i);
        }
    }
    let probe_pre = probe_checksum(probe_pa);

    // --- Below this point, UEFI boot services are gone ---
    uefi_helpers::exit_boot_services();
    let probe_post_ebs = probe_checksum(probe_pa);

    // 2. Register boot-stage page table allocator
    boot_alloc::init_boot_pt_alloc(result.bump_base, result.bump_end);
    pt_alloc::register(boot_alloc::boot_pt_alloc);

    // 3. Run arch_boot_impl — identity map + high-half map + enable paging
    let _info = minix_kernel::arch_boot_impl::<X86_64Paging>(&result.kernel_info, result.root_page);
    let probe_post_enable = probe_checksum(probe_pa);

    early_console::write_str("  n5: pa=0x");
    early_console::write_hex(probe_pa);
    early_console::write_str(" pre=0x");
    early_console::write_hex(probe_pre);
    early_console::write_str(" post_ebs=0x");
    early_console::write_hex(probe_post_ebs);
    early_console::write_str(" post_enable=0x");
    early_console::write_hex(probe_post_enable);
    if probe_post_ebs == probe_pre && probe_post_enable == probe_pre {
        early_console::write_str(" verdict=replayed");
    } else if probe_post_enable == 0 {
        early_console::write_str(" verdict=zeroed");
    } else {
        early_console::write_str(" verdict=changed");
    }
    // Structural half of the reuse hypothesis: a page allocated *after* the
    // snapshot is absent from the snapshot's reserved set, so if the snapshot
    // listed its physical range as conventional, the page holds live payload
    // while being advertised as free — the exact precondition for a later
    // allocator (VM PMM) to hand it out and zero it.
    let in_free = result.kernel_info.memmap.iter().any(|r| {
        probe_pa >= r.base.0 && probe_pa < r.base.0 + r.len as u64
    });
    early_console::write_str(" in_snapshot_free=");
    early_console::write_hex(in_free as u64);
    early_console::write_str("\n");

    // 4. Verify high-half mapping: write sentinel at a low physical address,
    //    read it back via the high-half virtual alias.
    //    High-half alias: kern_virt_base + (phys_addr - kern_phys_base)
    let test_phys: u64 = kern_phys_base + 0x1000; // offset 4KB into kernel region
    let test_virt: u64 = kern_virt_base + 0x1000; // same offset from kern_virt_base
    let low_addr = test_phys as *mut u64;
    let high_addr = test_virt as *const u64;

    unsafe {
        core::ptr::write_volatile(low_addr, SENTINEL);
    }

    early_console::write_str("  sentinel (low addr): ");
    early_console::write_hex(SENTINEL);
    early_console::write_str("\n");

    let read_val = unsafe { core::ptr::read_volatile(high_addr) };
    early_console::write_str("  read (high alias): ");
    early_console::write_hex(read_val);
    early_console::write_str("\n");

    if read_val == SENTINEL {
        early_console::write_str("  high-half mapping: OK\n");
        early_console::write_str("### TEST_RESULT: PASS test-kernel-map ###\n");
    } else {
        early_console::write_str("  high-half mapping: MISMATCH\n");
        early_console::write_str("### TEST_RESULT: FAIL test-kernel-map ###\n");
    }

    loop { unsafe { asm!("hlt", options(nomem, nostack)); } }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    early_console::write_str("### PANIC ###\n");
    loop { unsafe { asm!("hlt", options(nomem, nostack)); } }
}
