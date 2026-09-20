//! Test: timer IRQ hardware half-loop on aarch64 (E-3ARCHTRAP / NK3 — the
//! aarch64 leg of the x86 `test-timer-irq` carrier; AAVMF carrier).
//!
//! Verifies the full interrupt delivery chain with the REAL production
//! wiring: Generic Timer CNTP (CVAL one-shot) → CNTP_CTL gate + GICv3
//! (GICR_ISENABLER0 bit 30, ICC_PMR/IGRPEN1) → the production VBAR_EL1
//! current-EL IRQ leg (full frame save below the interrupted kernel sp —
//! no carrier-supplied trap table anywhere) → the kernel dispatch body
//! (`trap_dispatch::aarch64_kernel_body`) → ICC_IAR1 claim →
//! `IrqManager` hook chain → `clock_irq_handler` → ICC_EOIR1 complete →
//! CNTP re-arm → uptime advances.
//!
//! # Platform deviation (bounded, live-probed — rt-birth-aarch64 finding)
//!
//! `init_protection` drives `AArch64Protection::init`, whose body writes
//! SP_EL1; AAVMF rejects that write from its firmware-owned EL1 context
//! (the fault was probe-bracketed to exactly that instruction). This
//! carrier therefore performs the trap-entry half of init_protection by
//! hand — `install_trap_stubs` + `register_trap_dispatchers` +
//! `TrapEntryArch::load()` (a VBAR_EL1 write, proven legal on AAVMF) —
//! and skips the SP_EL1 write. The current-EL legs frame on the
//! interrupted kernel stack and never touch SP_EL1, so nothing else in
//! the chain needs it.
//!
//! Boot flow mirrors the x86 sibling: AAVMF → this .efi → hand-built
//! `BootPrepareResult` → paging (arch_boot_impl) → platform desc
//! (ACPI/UEFI handoff) → trap entry (production legs registered + VBAR
//! loaded) → init_clock_and_interrupts (GIC + software clock) →
//! init_proc_and_boot + init_smp_state → boot_init_timer → BKL held →
//! DAIF.I cleared → poll `get_monotonic()`.
//!
//! PASS = uptime reaches 5 ticks with interrupts delivered through the
//! production kernel leg (the scheduler is NOT entered; BKL held
//! throughout — the same PASS shape as x86 test-timer-irq).
//!
//! Honest scope: the lower-EL legs (SVC bridge + SP_EL0 exchange) are
//! wired in the production table but unreachable here — no EL0 code
//! exists in this carrier; the rt-birth-aarch64 carrier owns the EL0
//! proof until its carrier-supplied table retires.

#![no_std]
#![no_main]

extern crate alloc;

use core::alloc::{GlobalAlloc, Layout};
use core::panic::PanicInfo;

use boot_shim::uefi_helpers;
use minix_arch::arm64::paging::AArch64Paging;
use minix_arch::pt_alloc;
use minix_boot::{BootModule, BootPrepareResult, KernelInfo, MemoryRegion};
use minix_kernel::boot_alloc;
use minix_plat::arm64::early_console;
use minix_platform;
use minix_types::{PhysBytes, VirBytes};
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
                let size = if layout.align() > 8 {
                    layout.size() + layout.align()
                } else {
                    layout.size()
                };
                match uefi::boot::allocate_pool(
                    uefi::mem::memory_map::MemoryType::LOADER_DATA,
                    size,
                ) {
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

/// QEMU virt DRAM window, sized to the `-m 256M` the run script passes
/// (rt-birth-aarch64 shape: a static window, not the UEFI map, so the
/// boot-DM coverage spans our own page-table pages).
static MEMMAP: [MemoryRegion; 1] = [MemoryRegion {
    base: PhysBytes(0x4000_0000),
    len: 0x1000_0000,
}];

static mut BOOT_MODULES: [BootModule; 12] = [
    BootModule {
        name: "ds",
        start: PhysBytes(0),
        len: 0,
    },
    BootModule {
        name: "rs",
        start: PhysBytes(0),
        len: 0,
    },
    BootModule {
        name: "pm",
        start: PhysBytes(0),
        len: 0,
    },
    BootModule {
        name: "sched",
        start: PhysBytes(0),
        len: 0,
    },
    BootModule {
        name: "vfs",
        start: PhysBytes(0),
        len: 0,
    },
    BootModule {
        name: "memory",
        start: PhysBytes(0),
        len: 0,
    },
    BootModule {
        name: "tty",
        start: PhysBytes(0),
        len: 0,
    },
    BootModule {
        name: "mib",
        start: PhysBytes(0),
        len: 0,
    },
    BootModule {
        name: "vm",
        start: PhysBytes(0),
        len: 0,
    }, // patched at runtime
    BootModule {
        name: "pfs",
        start: PhysBytes(0),
        len: 0,
    },
    BootModule {
        name: "mfs",
        start: PhysBytes(0),
        len: 0,
    },
    BootModule {
        name: "init",
        start: PhysBytes(0),
        len: 0,
    },
];

/// Minimal valid ELF64 image (header only, e_phnum = 0) — the x86
/// test-timer-irq rationale: Phase C bookkeeping succeeds, VM is never
/// scheduled. e_machine = EM_AARCH64 (183).
#[unsafe(link_section = ".rodata")]
static VM_ELF_STUB: [u8; 64] = {
    let mut e = [0u8; 64];
    e[0] = 0x7F;
    e[1] = b'E';
    e[2] = b'L';
    e[3] = b'F';
    e[4] = 2;
    e[5] = 1;
    e[6] = 1;
    e[16] = 2;
    e[17] = 0; // e_type = ET_EXEC
    e[18] = 183;
    e[19] = 0; // e_machine = EM_AARCH64
    e[24] = 0;
    e[25] = 0x40;
    e[32] = 64;
    e[52] = 64;
    e[53] = 0;
    e[54] = 56;
    e[55] = 0;
    e[56] = 0;
    e[57] = 0; // e_phnum = 0
    e
};

const PASS_TICKS: u64 = 5;

#[entry]
fn main() -> Status {
    early_console::write_str(
        "### test_timer_irq (aarch64): CNTP → GIC → production VBAR leg → uptime\n",
    );

    // 1. UEFI boot preparation (rt-birth-aarch64 shape).
    let root_page = uefi_helpers::alloc_root_page();
    let (bump_base, bump_end) = uefi_helpers::alloc_bump_region(512);
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

    let platform_sources = uefi_helpers::find_platform_sources();

    let memmap: &'static [MemoryRegion] = &MEMMAP;

    // Kernel image range from the live image address (rt-birth-aarch64
    // finding: UEFI relocates the .efi anywhere in DRAM).
    let image_base = (main as usize as u64) & !0x001f_ffff; // 2 MiB aligned

    let kernel_info = KernelInfo {
        memmap,
        kern_virt_base: VirBytes(0xFFFF_8000_0000_0000),
        kern_phys_base: PhysBytes(image_base),
        kern_size: 0x200_000,
        free_upper_idx: Some(280),
        user_sp: VirBytes(0x0000_7fff_ffff_f000),
        // Kernel-stack TOP record (the lower-EL legs would switch to it;
        // this kernel-only carrier never enters EL0).
        kern_stack_top: VirBytes(image_base + 0x200_000),
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
    unsafe {
        BOOT_SERVICES_EXITED = true;
    }

    // 2. Paging (Phase A).
    boot_alloc::init_boot_pt_alloc(result.bump_base, result.bump_end);
    pt_alloc::register(boot_alloc::boot_pt_alloc);
    let _info =
        minix_kernel::arch_boot_impl::<AArch64Paging>(&result.kernel_info, result.root_page);
    early_console::write_str("  paging enabled\n");

    // Phase A.5: platform discovery (ACPI/UEFI handoff — kmain order).
    unsafe { minix_platform::init_from_kinfo(&result.kernel_info) };
    early_console::write_str("  platform desc installed\n");

    // 3. Trap entry — the trap-entry half of init_protection, performed
    // by hand (see the module doc: AAVMF rejects the SP_EL1 write in
    // `AArch64Protection::init`). Order matters: the dispatch bodies are
    // registered strictly before the table goes live (S-8 stage
    // invariant — no live gate with an empty handler).
    use minix_arch::{
        CurrentTrapEntry, TrapEntryArch, install_trap_stubs, register_trap_dispatchers,
    };
    let mut trap = CurrentTrapEntry::init();
    install_trap_stubs(&mut trap);
    register_trap_dispatchers(
        minix_kernel::trap_dispatch::aarch64_kernel_body,
        minix_kernel::trap_dispatch::aarch64_user_body,
    );
    trap.load();
    early_console::write_str("  production VBAR_EL1 legs live (no empty slot)\n");

    // 4. Clock + interrupt controller (kmain Phase B order): GICv3
    // (distributor + redistributor + CPU interface, mask_all) + software
    // clock into CLOCK_STATE.
    minix_kernel::init_clock_and_interrupts();
    early_console::write_str("  clock + controller initialized (GIC masked)\n");

    // 5. Process table (Phase C) + SMP state — clock_irq_handler reads
    // both for accounting.
    minix_kernel::init_proc_and_boot(&result.kernel_info);
    minix_kernel::init_smp_state();
    early_console::write_str("  proc table + smp state initialized\n");

    // 6. boot_cpu_init_timer parity: arm CNTP (gate kept closed),
    // register the clock hook (first-handler rule unmasks GICR bit 30),
    // open the CNTP_CTL gate.
    minix_kernel::boot_init_timer();
    early_console::write_str("  CNTP armed, clock hook registered, GICR bit 30 + CTL open\n");

    // 7. Hold the BKL (C: kernel runs with it held; IRQ handlers run
    // under the interrupted context's lock), enable IRQs, poll uptime.
    // SAFETY: the guard is leaked on purpose — this carrier's loop IS
    // the BKL owner for the rest of its life (C main.c BKL_LOCK parity).
    let guard = minix_kernel::smp::bkl_lock();
    core::mem::forget(guard);
    let section = unsafe { minix_kernel::smp::BklSection::assume_held() };
    // DAIF.I = bit 2 of DAIF → `daifclr #2` unmasks IRQ delivery.
    unsafe {
        core::arch::asm!("msr daifclr, #2", options(nomem, nostack));
    }
    early_console::write_str("  IRQs unmasked — polling uptime\n");

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
            early_console::write_str("### TEST_RESULT: PASS test-timer-irq-aarch64 ###\n");
            loop {
                // SAFETY: privileged mask; PASS already printed.
                unsafe {
                    core::arch::asm!("msr daifset, #2", options(nomem, nostack));
                }
            }
        }
        core::hint::spin_loop();
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    early_console::write_str("### PANIC in test-timer-irq-aarch64: ");
    if let Some(loc) = info.location() {
        early_console::write_str(loc.file());
        early_console::write_str(":");
        early_console::write_hex(loc.line() as u64);
    }
    early_console::write_str(" ");
    // Payload matters for boot-path diagnosis. Minimal core::fmt adapter
    // over EarlyConsole.
    struct Console;
    impl core::fmt::Write for Console {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            early_console::write_str(s);
            Ok(())
        }
    }
    let _ = core::fmt::write(&mut Console, format_args!("{}\n", info.message()));
    early_console::write_str(" ###\n");
    loop {
        unsafe {
            core::arch::asm!("wfe", options(nomem, nostack));
        }
    }
}
