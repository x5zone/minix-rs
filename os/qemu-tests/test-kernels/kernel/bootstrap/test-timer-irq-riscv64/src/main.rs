//! Test: timer IRQ hardware half-loop on riscv64 (E-3ARCHTRAP / NK3 — the
//! riscv64 leg of the x86 `test-timer-irq` carrier; OpenSBI carrier).
//!
//! Verifies the full interrupt delivery chain with the REAL production
//! wiring: SBI-armed S-mode timer (mtimecmp under OpenSBI's PMP) →
//! `sie.STIE` + `sstatus.SIE` gates → the production `stvec` kernel leg
//! (`riscv64_kernel_trap_vector`: full frame save below the interrupted
//! kernel sp, no carrier-supplied trap code anywhere) → the kernel
//! dispatch body (`trap_dispatch::riscv64_kernel_body`) → SBI re-arm +
//! `IrqManager` hook chain → `clock_irq_handler` → uptime advances.
//!
//! Boot flow mirrors the x86 sibling: OpenSBI (-bios default) → `-kernel`
//! at 0x80200000 → BSS zero + stack setup (K10 carrier shape) →
//! hand-built `BootPrepareResult` (DTB from OpenSBI's a1) → paging
//! (arch_boot_impl) → platform desc → init_protection (production trap
//! legs registered + stvec loaded) → init_clock_and_interrupts →
//! init_proc_and_boot + init_smp_state → boot_init_timer → BKL held →
//! sstatus.SIE → poll `get_monotonic()`.
//!
//! PASS = uptime reaches 5 ticks with interrupts delivered through the
//! production kernel leg (the scheduler is NOT entered; BKL held
//! throughout, which is the C "IRQ handlers run under the interrupted
//! context's BKL" model — the same PASS shape as x86 test-timer-irq).
//!
//! Honest scope: the user leg (ecall bridge) is wired in the production
//! stvec but unreachable here — no U-mode code exists in this carrier;
//! the rt-birth carriers own that proof until their carrier-supplied
//! legs retire.

#![no_std]
#![no_main]

extern crate alloc;

use core::alloc::{GlobalAlloc, Layout};
use core::arch::asm;
use core::panic::PanicInfo;

use minix_arch::pt_alloc;
use minix_arch::riscv64::paging::Riscv64Paging;
use minix_boot::{BootModule, BootPrepareResult, KernelInfo, MemoryRegion};
use minix_kernel::boot_alloc;
use minix_plat::riscv64::early_console;
use minix_types::{PhysBytes, VirBytes};

// ── Bump allocator on a static heap (hello-boot-riscv64 shape) ──
#[unsafe(link_section = ".bss")]
static mut HEAP: [u8; 0x10000] = [0u8; 0x10000];

static mut HEAP_PTR: usize = 0;

struct BootAllocator;

unsafe impl GlobalAlloc for BootAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        static mut GUARD: u8 = 0;
        let _ = &raw mut GUARD; // single-threaded boot: bump is unsynced by design
        unsafe {
            let base = core::ptr::addr_of_mut!(HEAP) as usize;
            let heap_len = 0x10000;
            let current = core::ptr::read_volatile(&raw mut HEAP_PTR);
            let align = layout.align();
            let size = layout.size();
            let aligned = (current + align - 1) & !(align - 1);
            let next = aligned + size;
            if next > heap_len || aligned < current {
                return core::ptr::null_mut();
            }
            core::ptr::write_volatile(&raw mut HEAP_PTR, next);
            (base + aligned) as *mut u8
        }
    }
    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}
}

#[global_allocator]
static ALLOCATOR: BootAllocator = BootAllocator;

// ── QEMU virt constants (hello-boot-riscv64 shape) ──
const DRAM_BASE: u64 = 0x8000_0000;
/// The kernel image load address — QEMU `-kernel` hands control to OpenSBI,
/// which jumps here (Domain0 Next Address).
const KERNEL_LOAD_PA: u64 = 0x8020_0000;

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

static MEMMAP: [MemoryRegion; 1] = [MemoryRegion {
    base: PhysBytes(DRAM_BASE),
    len: 0x1000_0000, // 256 MB — keep QEMU `-m` in sync
}];

/// Minimal valid ELF64 image (header only, e_phnum = 0) — the x86
/// test-timer-irq rationale: Phase C bookkeeping succeeds, VM is never
/// scheduled. e_machine = EM_RISCV (243).
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
    e[18] = 243;
    e[19] = 0; // e_machine = EM_RISCV
    e[24] = 0;
    e[25] = 0x80;
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

// ── Boot assembly (K10 carrier shape: BSS zero before any flag read) ──
core::arch::global_asm!(
    ".section .text.init",
    ".global _start",
    "_start:",
    // Zero .bss: OpenSBI hands over without clearing the kernel image's
    // BSS (K10 round-1 finding — garbage reads fake readiness).
    "    la t0, __bss_start",
    "    la t1, __bss_end",
    "1:",
    "    bgeu t0, t1, 2f",
    "    sd zero, 0(t0)",
    "    addi t0, t0, 8",
    "    j 1b",
    "2:",
    "    la sp, __stack_top",
    "    call rust_main",
    "1:",
    "    wfi",
    "    j 1b",
);

/// BSP stack: 64 KB, BSS. Top is at __stack_top.
static mut STACK: [u8; 0x10000] = [0u8; 0x10000];

core::arch::global_asm!(
    ".global __stack_top",
    ".set __stack_top, {stack_top} + 0x10000",
    stack_top = sym STACK,
);

/// Rust entry — single-hart carrier (a0 boot hart id, a1 DTB, per OpenSBI;
/// the hart id is unused, the DTB feeds the platform parse).
#[unsafe(no_mangle)]
extern "C" fn rust_main(_boot_hart: u64, dtb_phys: u64) -> ! {
    early_console::write_str(
        "### test_timer_irq (riscv64): SBI timer → production stvec kernel leg → uptime\n",
    );

    // 1. Hand-built BootPrepareResult (hello-boot-riscv64 shape).
    let root_page = PhysBytes(bump(1).expect("root page"));
    // 256 pages (1 MB): the identity map's 2-MiB fallback granularity
    // spends a handful of tables (rt-birth-riscv64 budget finding).
    let bump_base = bump(256).expect("bump region");
    let bump_end = bump_base + 256 * 4096;

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

    let kernel_info = KernelInfo {
        memmap: &MEMMAP,
        kern_virt_base: VirBytes(DRAM_BASE),
        kern_phys_base: PhysBytes(KERNEL_LOAD_PA),
        kern_size: 0x200_000,
        free_upper_idx: None,
        user_sp: VirBytes(0x0000_003f_ffff_f000),
        // Kernel-stack TOP for the sscratch swap on U→S entry (the user
        // leg is wired but unreachable in this kernel-only carrier).
        kern_stack_top: VirBytes(KERNEL_LOAD_PA + 0x200_000),
        syscall_entry: VirBytes(DRAM_BASE),
        boot_modules,
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

    // 2. Paging (Phase A): identity + Sv39 enable (production path).
    boot_alloc::init_boot_pt_alloc(result.bump_base, result.bump_end);
    pt_alloc::register(boot_alloc::boot_pt_alloc);
    let _info =
        minix_kernel::arch_boot_impl::<Riscv64Paging>(&result.kernel_info, result.root_page);
    early_console::write_str("  paging enabled\n");

    // Phase A.5: platform discovery — the DTB pointer OpenSBI passed in
    // a1 feeds the same parse the kernel lane uses (K10 carrier shape).
    {
        use minix_boot::PlatformDescSource;
        use minix_platform::kind::{DTB, parse_by_kind};
        if dtb_phys == 0 {
            panic!("OpenSBI did not pass a DTB pointer in a1");
        }
        let source = PlatformDescSource::new(DTB, PhysBytes(dtb_phys));
        let desc =
            unsafe { parse_by_kind(source) }.unwrap_or_else(|e| panic!("DTB parse failed: {e:?}"));
        unsafe { minix_platform::init(desc) };
        early_console::write_str("  platform desc installed (DTB)\n");
    }

    // 3. Protection + trap entry (production Phase B) — the REAL legs:
    // init_protection registers riscv64_kernel_body/riscv64_user_body and
    // `TrapEntryArch::load()` points stvec at
    // riscv64_kernel_trap_vector. No carrier-supplied trap code follows
    // (the rt-birth carrier's post-init stvec re-point is gone — the
    // production leg is the thing under test).
    minix_kernel::init_protection(&result.kernel_info);
    early_console::write_str("  protection live; production kernel leg on stvec\n");

    // 4. Clock + interrupt controller (kmain Phase B order): PLIC init
    // (mask_all) + software clock into CLOCK_STATE.
    minix_kernel::init_clock_and_interrupts();
    early_console::write_str("  clock + controller initialized (PLIC masked)\n");

    // 5. Process table (Phase C) + SMP state — clock_irq_handler reads
    // both for accounting.
    minix_kernel::init_proc_and_boot(&result.kernel_info);
    minix_kernel::init_smp_state();
    early_console::write_str("  proc table + smp state initialized\n");

    // 6. boot_cpu_init_timer parity: arm the timer through the SBI TIME
    // ecall, register the clock hook (first-handler rule; the CPU-local
    // timer has no PLIC line), open sie.STIE.
    minix_kernel::boot_init_timer();
    early_console::write_str("  SBI timer armed, clock hook registered, STIE open\n");

    // 7. Hold the BKL (C: kernel runs with it held; IRQ handlers run
    // under the interrupted context's lock), enable S-mode interrupts,
    // poll uptime.
    // SAFETY: the guard is leaked on purpose — this carrier's loop IS
    // the BKL owner for the rest of its life (C main.c BKL_LOCK parity).
    let guard = minix_kernel::smp::bkl_lock();
    core::mem::forget(guard);
    let section = unsafe { minix_kernel::smp::BklSection::assume_held() };
    // sstatus.SIE = bit 1 (the smp-ipi carrier's proven enable sequence;
    // sret semantics do not apply — we never leave S-mode here).
    unsafe {
        asm!("csrs sstatus, {}", in(reg) 2u64, options(nomem, nostack));
    }
    early_console::write_str("  SIE=1 — polling uptime\n");

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
            early_console::write_str("### TEST_RESULT: PASS test-timer-irq-riscv64 ###\n");
            loop {
                // SAFETY: privileged halt; PASS already printed.
                unsafe {
                    asm!("csrc sstatus, {}", in(reg) 2u64, options(nomem, nostack));
                }
            }
        }
        core::hint::spin_loop();
    }
}

fn bump(num_pages: usize) -> Option<u64> {
    // Boot bump allocations sit at DRAM + 32 MB (past the kernel image),
    // ending before 64 MB — hello-boot-riscv64's layout.
    static mut BUMP_PTR: u64 = DRAM_BASE + 0x0200_0000;
    const BUMP_END: u64 = DRAM_BASE + 0x0400_0000;
    unsafe {
        let need = (num_pages as u64) * 4096;
        let current = core::ptr::read_volatile(&raw const BUMP_PTR);
        if current + need > BUMP_END {
            return None;
        }
        core::ptr::write_volatile(&raw mut BUMP_PTR, current + need);
        Some(current)
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    early_console::write_str("### PANIC in test-timer-irq-riscv64: ");
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
            asm!("wfi", options(nomem, nostack));
        }
    }
}
