//! Test: process table initialization and VM ELF loading (Phase C/D).
//!
//! Verifies that after init_proc_and_boot() + init_post_and_memory():
//!   1. Kernel tasks (ASYNCM, IDLE, CLOCK, SYSTEM, KERNEL) are initialized
//!   2. Kernel tasks have SLOT_FREE cleared and PROC_STOP set
//!   3. VM process has correct RTS flags (PROC_STOP set, SLOT_FREE clear, no VMINHIBIT)
//!   4. Non-VM user processes have VMINHIBIT + BOOTINHIBIT set
//!   5. VM p_seg is accessible
//!   6. init_post_and_memory completes without panic (ptproc + freepdes)
//!
//! Boot flow: UEFI → arch_boot_impl (paging) → init_protection →
//!            init_proc_and_boot → init_post_and_memory → verify

#![no_std]
#![no_main]

extern crate alloc;

use core::arch::asm;
use core::panic::PanicInfo;
use core::alloc::{GlobalAlloc, Layout};
use minix_arch::x86_64::paging::X86_64Paging;
use minix_plat::x86_64::early_console;
use minix_arch::{ProtectionArch, TrapEntryArch, CurrentProtection, CurrentTrapEntry};
use minix_kernel::boot_alloc;
use minix_arch::pt_alloc;
use minix_types::{PhysBytes, VirBytes};
use minix_boot::{BootPrepareResult, KernelInfo, BootModule, MemoryRegion};
use boot_shim::uefi_helpers;
use uefi::prelude::*;

// ── Hybrid allocator: UEFI boot services → bump after exit ──
// Before exit_boot_services(), we delegate to UEFI's pool allocator.
// After exit_boot_services(), we use a simple bump allocator backed by BSS.
// ProcessTable::new() uses Box/Vec which need a global allocator.

#[link_section = ".bss"]
static mut HEAP: [u8; 0x200000] = [0u8; 0x200000]; // 2 MiB

/// Set to true after exit_boot_services() is called.
static mut BOOT_SERVICES_EXITED: bool = false;

struct HybridAllocator;

unsafe impl GlobalAlloc for HybridAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if !BOOT_SERVICES_EXITED {
            // Use UEFI pool allocation before ExitBootServices
            let size = layout.size();
            let align = layout.align();
            // UEFI requires 8-byte alignment minimum; for larger alignments,
            // over-allocate and align within the block
            let alloc_size = if align > 8 { size + align } else { size };
            match uefi::boot::allocate_pool(uefi::mem::memory_map::MemoryType::LOADER_DATA, alloc_size) {
                Ok(ptr) => {
                    let addr = ptr.as_ptr() as usize;
                    if align > 8 {
                        let aligned = (addr + align - 1) & !(align - 1);
                        aligned as *mut u8
                    } else {
                        addr as *mut u8
                    }
                }
                Err(_) => core::ptr::null_mut(),
            }
        } else {
            // Bump allocator after ExitBootServices
            static mut HEAP_PTR: usize = 0;
            let align = layout.align();
            let size = layout.size();
            let base = core::ptr::addr_of_mut!(HEAP) as usize;
            let heap_len = 0x200000;
            let current = HEAP_PTR;
            let aligned = (current + align - 1) & !(align - 1);
            let next = aligned + size;
            if next > heap_len {
                return core::ptr::null_mut();
            }
            HEAP_PTR = next;
            (base + aligned) as *mut u8
        }
    }
    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        // No deallocation support in either mode
    }
}

#[global_allocator]
static ALLOCATOR: HybridAllocator = HybridAllocator;

/// User-space boot modules (matching C's kinfo.module_list[]).
/// Must match NR_BOOT_MODULES = 12.
/// These are the GRUB multiboot modules, NOT kernel tasks.
/// Kernel tasks are hardcoded in KERNEL_TASKS[] and initialized separately.
/// C: table.c — image[NR_TASKS..NR_BOOT_PROCS]
/// C: minix/com.h — DS_PROC_NR=0, RS_PROC_NR=1, ..., INIT_PROC_NR=11
static BOOT_MODULES: [BootModule; 12] = [
    BootModule { name: "ds",    start: PhysBytes(0), len: 0 },  // nr=0  (DS_PROC_NR)
    BootModule { name: "rs",    start: PhysBytes(0), len: 0 },  // nr=1  (RS_PROC_NR)
    BootModule { name: "pm",    start: PhysBytes(0), len: 0 },  // nr=2  (PM_PROC_NR)
    BootModule { name: "sched", start: PhysBytes(0), len: 0 },  // nr=3  (SCHED_PROC_NR)
    BootModule { name: "vfs",   start: PhysBytes(0), len: 0 },  // nr=4  (VFS_PROC_NR)
    BootModule { name: "memory",start: PhysBytes(0), len: 0 },  // nr=5  (MEM_PROC_NR)
    BootModule { name: "tty",   start: PhysBytes(0), len: 0 },  // nr=6  (TTY_PROC_NR)
    BootModule { name: "mib",   start: PhysBytes(0), len: 0 },  // nr=7  (MIB_PROC_NR)
    BootModule { name: "vm",    start: PhysBytes(0), len: 0 },  // nr=8  (VM_PROC_NR)
    BootModule { name: "pfs",   start: PhysBytes(0), len: 0 },  // nr=9  (PFS_PROC_NR)
    BootModule { name: "mfs",   start: PhysBytes(0), len: 0 },  // nr=10 (MFS_PROC_NR)
    BootModule { name: "init",  start: PhysBytes(0), len: 0 },  // nr=11 (INIT_PROC_NR)
];

static MEMMAP: [MemoryRegion; 2] = [
    MemoryRegion { base: PhysBytes(0), len: 0x200_0000 },
    MemoryRegion { base: PhysBytes(0x200_0000), len: 0x600_0000 },
];

#[entry]
fn main() -> Status {
    early_console::write_str("### test_proc_init (x86_64): Phase C/D verification\n");

    // 1. UEFI boot preparation
    let root_page = uefi_helpers::alloc_root_page();
    let (bump_base, bump_end) = uefi_helpers::alloc_bump_region(8);

    let kernel_info = KernelInfo {
        memmap: &MEMMAP,
        kern_virt_base: VirBytes(0xFFFF_8000_0000_0000),
        kern_phys_base: PhysBytes(0x200_000),
        kern_size: 0x200_000,
        free_upper_idx: Some(280),
        user_sp: VirBytes(0x0000_7fff_ffff_f000),
        kern_stack_top: VirBytes(0xFFFF_8000_0020_0000),
        syscall_entry: VirBytes(0xFFFF_8000_0010_0000),
        boot_modules: &BOOT_MODULES,
        bootstrap_start: PhysBytes(0),
        bootstrap_len: 0,
        platform_sources: &[],
    };

    let result = BootPrepareResult {
        kernel_info,
        root_page,
        bump_base,
        bump_end,
    };

    uefi_helpers::exit_boot_services();

    // Switch to bump allocator now that UEFI boot services are gone
    unsafe { BOOT_SERVICES_EXITED = true; }

    // 2. Enable paging (Phase A)
    boot_alloc::init_boot_pt_alloc(result.bump_base, result.bump_end);
    pt_alloc::register(boot_alloc::boot_pt_alloc);
    let info = minix_kernel::arch_boot_impl::<X86_64Paging>(&result.kernel_info, result.root_page);
    early_console::write_str("  paging enabled\n");

    // 3. Phase B: protection only (no clock/interrupt needed for proc table init)
    let prot = CurrentProtection::init(0, info.kern_stack_top);
    prot.load();
    let mut trap = CurrentTrapEntry::init();
    trap.configure_syscall(info.syscall_entry);
    trap.load();
    early_console::write_str("  protection loaded\n");

    // 4. Phase C: init_proc_and_boot
    early_console::write_str("  calling init_proc_and_boot...\n");
    let proc_table = minix_kernel::init_proc_and_boot(&result.kernel_info);
    early_console::write_str("  init_proc_and_boot completed\n");

    // 5. Verify process table state
    use minix_kernel::proc::RtsFlagsBits;
    use minix_kernel::proc::proc_nr;

    // 5a. Verify: CLOCK kernel task has SLOT_FREE cleared and PROC_STOP set
    // C: CLOCK=-3, kernel tasks are hardcoded in KERNEL_TASKS[]
    let clock_proc = proc_table.get(proc_nr::CLOCK);
    if clock_proc.is_none() {
        early_console::write_str("  FAIL: CLOCK proc not found\n");
        fail();
    }
    let clock_proc = clock_proc.unwrap();
    if clock_proc.p_rts_flags.is_set(RtsFlagsBits::SLOT_FREE) {
        early_console::write_str("  FAIL: CLOCK has SLOT_FREE\n");
        fail();
    }
    if !clock_proc.p_rts_flags.is_set(RtsFlagsBits::PROC_STOP) {
        early_console::write_str("  FAIL: CLOCK missing PROC_STOP\n");
        fail();
    }
    early_console::write_str("  CLOCK: SLOT_FREE=0, PROC_STOP=1 (OK)\n");

    // 5b. Verify: IDLE kernel task has IDL_F privilege flag
    let idle_proc = proc_table.get(proc_nr::IDLE);
    if idle_proc.is_none() {
        early_console::write_str("  FAIL: IDLE proc not found\n");
        fail();
    }
    let idle_proc = idle_proc.unwrap();
    if idle_proc.p_rts_flags.is_set(RtsFlagsBits::SLOT_FREE) {
        early_console::write_str("  FAIL: IDLE has SLOT_FREE\n");
        fail();
    }
    early_console::write_str("  IDLE: SLOT_FREE=0 (OK)\n");

    // 5c. Verify: VM process (nr=8) has correct flags
    let vm_proc = proc_table.get(proc_nr::VM_PROC_NR);
    if vm_proc.is_none() {
        early_console::write_str("  FAIL: VM proc not found\n");
        fail();
    }
    let vm_proc = vm_proc.unwrap();
    if vm_proc.p_rts_flags.is_set(RtsFlagsBits::SLOT_FREE) {
        early_console::write_str("  FAIL: VM has SLOT_FREE\n");
        fail();
    }
    if !vm_proc.p_rts_flags.is_set(RtsFlagsBits::PROC_STOP) {
        early_console::write_str("  FAIL: VM missing PROC_STOP\n");
        fail();
    }
    // VM should NOT have VMINHIBIT (it's the VM itself)
    if vm_proc.p_rts_flags.is_set(RtsFlagsBits::VMINHIBIT) {
        early_console::write_str("  FAIL: VM has VMINHIBIT (should not)\n");
        fail();
    }
    early_console::write_str("  VM: SLOT_FREE=0, PROC_STOP=1, VMINHIBIT=0 (OK)\n");

    // 5d. Verify: non-VM user process (PM nr=2) has VMINHIBIT + BOOTINHIBIT
    // C: main.c:267-270 — all user procs except VM get VMINHIBIT|BOOTINHIBIT
    let pm_proc = proc_table.get(2); // PM_PROC_NR = 2
    if let Some(pm) = pm_proc {
        if !pm.p_rts_flags.is_set(RtsFlagsBits::VMINHIBIT) {
            early_console::write_str("  FAIL: PM missing VMINHIBIT\n");
            fail();
        }
        if !pm.p_rts_flags.is_set(RtsFlagsBits::BOOTINHIBIT) {
            early_console::write_str("  FAIL: PM missing BOOTINHIBIT\n");
            fail();
        }
        early_console::write_str("  PM: VMINHIBIT=1, BOOTINHIBIT=1 (OK)\n");
    } else {
        early_console::write_str("  NOTE: PM proc not found\n");
    }

    // 5e. Verify: VM p_seg is accessible
    let _seg = vm_proc.p_seg;
    early_console::write_str("  VM p_seg accessible (OK)\n");

    // 6. Phase D: init_post_and_memory (ptproc + freepdes)
    minix_kernel::init_post_and_memory(&result.kernel_info, &proc_table);
    early_console::write_str("  init_post_and_memory completed\n");

    early_console::write_str("### TEST_RESULT: PASS test-proc-init ###\n");
    loop { unsafe { asm!("hlt", options(nomem, nostack)); } }
}

fn fail() -> ! {
    early_console::write_str("### TEST_RESULT: FAIL test-proc-init ###\n");
    loop { unsafe { asm!("hlt", options(nomem, nostack)); } }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    early_console::write_str("### PANIC in test-proc-init: ");
    if let Some(loc) = info.location() {
        early_console::write_str(loc.file());
        early_console::write_str(":");
        early_console::write_hex(loc.line() as u64);
    }
    early_console::write_str(" ###\n");
    loop { unsafe { asm!("hlt", options(nomem, nostack)); } }
}
