//! Test: multi-AP bring-up (SMP bring-up S-5, L4).
//!
//! Verifies the production `smp_init` orchestration end-to-end: the BSP runs
//! the C-parity serial handshake (bootstrap install → INIT/SIPI → bounded
//! ack wait → READY 代置) for EVERY AP in the topology; each AP executes the
//! kernel-registered tail (S-4 init_ap on its own core → per-CPU SYSCALL
//! MSRs → ap_finish_booting online bit); `wait_for_aps` completes via the
//! BKL dance.
//!
//! Boot flow: UEFI → paging → init_protection (production phase: fills the
//! PROTECTION/TRAP_ENTRY globals the AP tail consumes) → init_clock_and_
//! interrupts → init_proc_and_boot → init_smp_state → smp_init → masks.
//!
//! PASS = `boot_ack_mask == expected_cpu_mask && online_mask ==
//! expected_cpu_mask` (v4 #3 health layer; the production wait itself only
//! requires online == boot_ack — the degrade-and-continue path).

#![no_std]
#![no_main]

extern crate alloc;

use core::arch::asm;
use core::panic::PanicInfo;
use core::alloc::{GlobalAlloc, Layout};
use minix_arch::x86_64::paging::X86_64Paging;
use minix_plat::x86_64::early_console;
use minix_kernel::boot_alloc;
use minix_platform;
use minix_arch::pt_alloc;
use minix_types::{PhysBytes, VirBytes};
use minix_boot::{BootPrepareResult, KernelInfo, BootModule, MemoryRegion};
use boot_shim::uefi_helpers;
use uefi::prelude::*;

// ── Hybrid allocator: UEFI boot services → bump after exit ──
#[unsafe(link_section = ".bss")]
static mut HEAP: [u8; 0x200000] = [0u8; 0x200000]; // 2 MiB

static mut BOOT_SERVICES_EXITED: bool = false;

struct HybridAllocator;

unsafe impl GlobalAlloc for HybridAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        unsafe {
            if !BOOT_SERVICES_EXITED {
                let size = if layout.align() > 8 { layout.size() + layout.align() } else { layout.size() };
                match uefi::boot::allocate_pool(uefi::mem::memory_map::MemoryType::LOADER_DATA, size) {
                    Ok(ptr) => ptr.as_ptr() as *mut u8,
                    Err(_) => core::ptr::null_mut(),
                }
            } else {
                static mut HEAP_PTR: usize = 0;
                let align = layout.align();
                let size = layout.size();
                let base = core::ptr::addr_of_mut!(HEAP) as usize;
                let heap_len = 0x200000;
                let current = HEAP_PTR;
                let aligned = (current + align - 1) & !(align - 1);
                let next = aligned + size;
                if next > heap_len || aligned < current {
                    return core::ptr::null_mut();
                }
                HEAP_PTR = next;
                (base + aligned) as *mut u8
            }
        }
    }
    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}
}

#[global_allocator]
static ALLOCATOR: HybridAllocator = HybridAllocator;

/// User-space boot modules (matching C's kinfo.module_list[]).
/// Same shape as test-proc-init: kernel tasks are hardcoded in
/// KERNEL_TASKS[]; only the VM slot is patched to the ELF stub so
/// init_proc_and_boot's VM-load step has something well-formed to parse.
static mut BOOT_MODULES: [BootModule; 12] = [
    BootModule { name: "ds",    start: PhysBytes(0), len: 0 },
    BootModule { name: "rs",    start: PhysBytes(0), len: 0 },
    BootModule { name: "pm",    start: PhysBytes(0), len: 0 },
    BootModule { name: "sched", start: PhysBytes(0), len: 0 },
    BootModule { name: "vfs",   start: PhysBytes(0), len: 0 },
    BootModule { name: "memory",start: PhysBytes(0), len: 0 },
    BootModule { name: "tty",   start: PhysBytes(0), len: 0 },
    BootModule { name: "mib",   start: PhysBytes(0), len: 0 },
    BootModule { name: "vm",    start: PhysBytes(0), len: 0 },  // patched at runtime
    BootModule { name: "pfs",   start: PhysBytes(0), len: 0 },
    BootModule { name: "mfs",   start: PhysBytes(0), len: 0 },
    BootModule { name: "init",  start: PhysBytes(0), len: 0 },
];

static MEMMAP: [MemoryRegion; 2] = [
    MemoryRegion { base: PhysBytes(0), len: 0x200_0000 },
    MemoryRegion { base: PhysBytes(0x200_0000), len: 0x600_0000 },
];

/// Minimal valid ELF64 image (header only, e_phnum = 0) — same rationale
/// as test-proc-init: Phase C bookkeeping succeeds, VM is never scheduled.
#[unsafe(link_section = ".rodata")]
static VM_ELF_STUB: [u8; 64] = {
    let mut e = [0u8; 64];
    e[0] = 0x7F; e[1] = b'E'; e[2] = b'L'; e[3] = b'F';
    e[4] = 2; e[5] = 1; e[6] = 1;
    e[16] = 2; e[17] = 0;   // e_type = ET_EXEC
    e[18] = 62; e[19] = 0;  // e_machine = EM_X86_64
    e[24] = 0; e[25] = 0x40;
    e[32] = 64;
    e[52] = 64; e[53] = 0;
    e[54] = 56; e[55] = 0;
    e[56] = 0; e[57] = 0;   // e_phnum = 0
    e
};

#[entry]
fn main() -> Status {
    early_console::write_str("### test_smp_aps (x86_64): S-5 L4 — production smp_init multi-AP bring-up\n");

    // 1. UEFI boot preparation (test-proc-init shape).
    let root_page = uefi_helpers::alloc_root_page();
    let (bump_base, bump_end) = uefi_helpers::alloc_bump_region(64);
    // SAFETY: single-threaded boot; BOOT_MODULES is read-only afterwards.
    let boot_modules: &'static [BootModule] = unsafe {
        let mods = &mut *core::ptr::addr_of_mut!(BOOT_MODULES);
        mods[8] = BootModule {
            name: "vm",
            start: PhysBytes(core::ptr::addr_of!(VM_ELF_STUB) as usize as u64),
            len: 64,
        };
        mods
    };

    // Must run before exit_boot_services (reads the UEFI config table for
    // the ACPI RSDP); the kernel's init_from_kinfo parses the first source
    // that succeeds — release builds have no QemuVirt fallback.
    let platform_sources = uefi_helpers::find_platform_sources();

    let kernel_info = KernelInfo {
        memmap: &MEMMAP,
        kern_virt_base: VirBytes(0xFFFF_8000_0000_0000),
        kern_phys_base: PhysBytes(0x200_000),
        kern_size: 0x200_000,
        free_upper_idx: Some(280),
        user_sp: VirBytes(0x0000_7fff_ffff_f000),
        kern_stack_top: VirBytes(0xFFFF_8000_0020_0000),
        syscall_entry: VirBytes(0xFFFF_8000_0010_0000),
        boot_modules,
        bootstrap_start: PhysBytes(0),
        bootstrap_len: 0,
        platform_sources,
        param_buf: &[],
    };

    let result = BootPrepareResult {
        kernel_info,
        root_page,
        bump_base,
        bump_end,
    };

    uefi_helpers::exit_boot_services();
    unsafe { BOOT_SERVICES_EXITED = true; }

    // 2. Paging (Phase A).
    boot_alloc::init_boot_pt_alloc(result.bump_base, result.bump_end);
    pt_alloc::register(boot_alloc::boot_pt_alloc);
    let info = minix_kernel::arch_boot_impl::<X86_64Paging>(&result.kernel_info, result.root_page);
    early_console::write_str("  paging enabled\n");

    // Phase A.5: platform discovery FIRST (kmain order) — init_protection
    // reads the topology to program the BSP's GS area.
    unsafe { minix_platform::init_from_kinfo(&result.kernel_info) };

    // 3. Production Phase B: protection + trap entry (fills the
    // PROTECTION/TRAP_ENTRY globals — the AP tail's init_ap consumes them).
    minix_kernel::init_protection(&result.kernel_info);
    early_console::write_str("  protection + IDT live (production phase)\n");

    // 4. Clock + interrupt controller (kmain Phase B order).
    minix_kernel::init_clock_and_interrupts();
    early_console::write_str("  clock + controller initialized\n");

    // 5. Process table (Phase C) + SMP state (Phase D).
    minix_kernel::init_proc_and_boot(&result.kernel_info);
    minix_kernel::init_smp_state();
    early_console::write_str("  proc table + smp state initialized\n");

    // 6. S-5 orchestration: serial per-AP handshake + wait_for_aps BKL
    // dance. C main.c:149 parity — the BKL is acquired ONCE before
    // smp_init and stays held (wait_for_aps's dance preserves it).
    minix_kernel::smp::bkl_lock().transfer();
    minix_kernel::smp::smp_init();
    early_console::write_str("  smp_init completed (wait_for_aps returned)\n");

    // 7. Health assertions (v4 #3): both masks must equal the full expected
    // CPU mask — BSP bit + every topology AP bit.
    let topo = minix_platform::platform_desc().cpu_topology();
    let expected = (1u64 << topo.nr_cpus) - 1;
    let smp = unsafe { minix_kernel::smp_state_boot_unchecked() };
    let ack = smp.boot_ack_mask_value();
    let online = smp.online_mask_value();
    early_console::write_str("  expected: ");
    early_console::write_hex(expected);
    early_console::write_str(" boot_ack: ");
    early_console::write_hex(ack);
    early_console::write_str(" online: ");
    early_console::write_hex(online);
    early_console::write_str("\n");
    if ack != expected || online != expected {
        early_console::write_str("  FAIL: mask mismatch\n");
        fail();
    }
    early_console::write_str("  boot_ack == online == expected (all APs online)\n");

    early_console::write_str("### TEST_RESULT: PASS test-smp-aps ###\n");
    loop { unsafe { asm!("cli", options(nomem, nostack)); } }
}

fn fail() -> ! {
    early_console::write_str("### TEST_RESULT: FAIL test-smp-aps ###\n");
    loop { unsafe { asm!("hlt", options(nomem, nostack)); } }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    early_console::write_str("### PANIC in test-smp-aps: ");
    if let Some(loc) = info.location() {
        early_console::write_str(loc.file());
        early_console::write_str(":");
        early_console::write_hex(loc.line() as u64);
    }
    early_console::write_str(" ###\n");
    loop { unsafe { asm!("hlt", options(nomem, nostack)); } }
}
