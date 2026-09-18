//! Test: timer IRQ hardware half-loop (SMP bring-up S-8, L3).
//!
//! Verifies the full D-46 interrupt delivery chain with the real production
//! wiring: PIT → IOAPIC RTE (vector 0x50) → CPU → S-8 asm stub →
//! `IrqManager::dispatch` → `clock_irq_handler` → uptime advances.
//!
//! Boot flow: UEFI → paging (arch_boot_impl) → protection load → S-8 trap
//! entry (install_trap_stubs + register kernel dispatch bodies + load) →
//! init_clock_and_interrupts → init_proc_and_boot → boot_init_timer →
//! BKL held → sti → poll `get_monotonic()`.
//!
//! PASS = uptime reaches 5 ticks with interrupts delivered through the
//! S-8 stub path (the scheduler is NOT entered; BKL held throughout, which
//! is the C "IRQ handlers run under the interrupted context's BKL" model).
//!
//! S-8 boundary (honest scope): the syscall leg (LSTAR body) and user-frame
//! reschedule exits are wired but unreachable here — no CPL3 code exists
//! before S-6/S-7.

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

const PASS_TICKS: u64 = 5;

#[entry]
fn main() -> Status {
    early_console::write_str("### test_timer_irq (x86_64): S-8 L3 — PIT→IOAPIC→stub→IrqManager→uptime\n");

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

    // 3. Protection (Phase B, first half).
    let prot = CurrentProtection::init(0, info.kern_stack_top);
    prot.load();
    early_console::write_str("  protection loaded\n");

    // 4. S-8 trap entry: real stubs into every gate, kernel dispatch bodies
    // registered, LSTAR pointed at the kernel's own SYSCALL asm entry, and
    // only then lidt. Order matters: registration strictly before load (no
    // live gate may have an empty handler — S-8 stage invariant).
    let mut trap = CurrentTrapEntry::init();
    minix_arch::install_trap_stubs(&mut trap);
    minix_arch::register_trap_dispatchers(
        minix_kernel::trap_dispatch::x86_trap_dispatch_body,
        minix_kernel::trap_dispatch::x86_syscall_dispatch_body,
    );
    trap.configure_syscall(minix_arch::syscall_entry_va());
    trap.load();
    early_console::write_str("  IDT live with S-8 stubs (no empty gate)\n");

    // 5. Clock + interrupt controller (kmain Phase B order) — PIT IRQ line
    // stays masked here; the controller mask_all is the D-59 boot contract.
    unsafe { minix_platform::init_from_kinfo(&result.kernel_info) };
    minix_kernel::init_clock_and_interrupts();
    early_console::write_str("  clock + controller initialized (all lines masked)\n");

    // 6. Process table (Phase C) + SMP state (Phase D) — clock_irq_handler
    // reads both for accounting.
    minix_kernel::init_proc_and_boot(&result.kernel_info);
    minix_kernel::init_smp_state();
    early_console::write_str("  proc table + smp state initialized\n");

    // 7. boot_cpu_init_timer parity: program PIT, register clock hook
    // (first-handler rule unmasks the IOAPIC line), open gates.
    minix_kernel::boot_init_timer();
    early_console::write_str("  PIT programmed, clock hook registered, gate open\n");

    // 8. Hold the BKL (C: kernel runs with it held; IRQ handlers run under
    // the interrupted context's lock), enable interrupts, poll uptime.
    // SAFETY: the guard is leaked on purpose — this test kernel's loop IS
    // the BKL owner for the rest of its life (C main.c BKL_LOCK parity).
    let guard = minix_kernel::smp::bkl_lock();
    core::mem::forget(guard);
    // BklSection witness (the forgotten guard above leaves the BKL held —
    // this test kernel's loop IS the owner for the rest of its life).
    let section = unsafe { minix_kernel::smp::BklSection::assume_held() };
    unsafe { asm!("sti", options(nomem, nostack)); }
    early_console::write_str("  IF=1 — polling uptime\n");

    let start = minix_kernel::clock::get_monotonic(&section);
    let target = start + PASS_TICKS;
    let mut last = start;
    loop {
        let now = minix_kernel::clock::get_monotonic(&section);
        if now != last {
            last = now;
            early_console::write_str("  tick: uptime = ");
            early_console::write_hex(now);
            early_console::write_str("\n");
        }
        if now >= target {
            early_console::write_str("### TEST_RESULT: PASS test-timer-irq ###\n");
            loop { unsafe { asm!("cli", options(nomem, nostack)); } }
        }
        core::hint::spin_loop();
    }
}

fn fail() -> ! {
    early_console::write_str("### TEST_RESULT: FAIL test-timer-irq ###\n");
    loop { unsafe { asm!("hlt", options(nomem, nostack)); } }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    early_console::write_str("### PANIC in test-timer-irq: ");
    if let Some(loc) = info.location() {
        early_console::write_str(loc.file());
        early_console::write_str(":");
        early_console::write_hex(loc.line() as u64);
    }
    early_console::write_str(" ###\n");
    loop { unsafe { asm!("hlt", options(nomem, nostack)); } }
}
