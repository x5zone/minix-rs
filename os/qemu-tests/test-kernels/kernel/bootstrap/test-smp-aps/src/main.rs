//! Test: multi-AP bring-up.
//!
//! Verifies the production `smp_init` orchestration end-to-end: the BSP runs
//! the C-parity serial handshake (bootstrap install → INIT/SIPI → bounded
//! ack wait → READY set on the AP's behalf) for EVERY AP in the topology; each AP executes the
//! kernel-registered tail (init_ap on its own core → per-CPU SYSCALL
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

use boot_shim::uefi_helpers;
use core::alloc::{GlobalAlloc, Layout};
use core::arch::asm;
use core::panic::PanicInfo;
use minix_arch::pt_alloc;
use minix_arch::x86_64::paging::X86_64Paging;
use minix_boot::{BootModule, KernelInfo, MemoryRegion, PlatformDescSource};
use minix_kernel::boot_alloc;
use minix_plat::x86_64::early_console;
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

/// User-space boot modules (matching C's kinfo.module_list[]).
/// Same shape as test-proc-init: kernel tasks are hardcoded in
/// KERNEL_TASKS[]; only the VM slot is patched to the ELF stub so
/// init_proc_and_boot's VM-load step has something well-formed to parse.
static mut BOOT_MODULES: [BootModule; 12] = [
    BootModule {
        name: "ds",
        start: PhysBytes(PLACEHOLDER_SPAN),
        len: 64,
    },
    BootModule {
        name: "rs",
        start: PhysBytes(PLACEHOLDER_SPAN),
        len: 64,
    },
    BootModule {
        name: "pm",
        start: PhysBytes(PLACEHOLDER_SPAN),
        len: 64,
    },
    BootModule {
        name: "sched",
        start: PhysBytes(PLACEHOLDER_SPAN),
        len: 64,
    },
    BootModule {
        name: "vfs",
        start: PhysBytes(PLACEHOLDER_SPAN),
        len: 64,
    },
    BootModule {
        name: "memory",
        start: PhysBytes(PLACEHOLDER_SPAN),
        len: 64,
    },
    BootModule {
        name: "tty",
        start: PhysBytes(PLACEHOLDER_SPAN),
        len: 64,
    },
    BootModule {
        name: "mib",
        start: PhysBytes(PLACEHOLDER_SPAN),
        len: 64,
    },
    BootModule {
        name: "vm",
        start: PhysBytes(PLACEHOLDER_SPAN),
        len: 64,
    }, // patched at runtime
    BootModule {
        name: "pfs",
        start: PhysBytes(PLACEHOLDER_SPAN),
        len: 64,
    },
    BootModule {
        name: "mfs",
        start: PhysBytes(PLACEHOLDER_SPAN),
        len: 64,
    },
    BootModule {
        name: "init",
        start: PhysBytes(PLACEHOLDER_SPAN),
        len: 64,
    },
];

/// Placeholder module span: real RAM inside MEMMAP (name-only slots at
/// Phase C are DEFERRED; the span exists so the boot identity windows are
/// real `[start, end)` ranges, never empty).
const PLACEHOLDER_SPAN: u64 = 0x100_0000;

static MEMMAP: [MemoryRegion; 2] = [
    MemoryRegion {
        base: PhysBytes(0),
        len: 0x200_0000,
    },
    MemoryRegion {
        base: PhysBytes(0x200_0000),
        len: 0x600_0000,
    },
];

/// Minimal valid ELF64 image (header only, e_phnum = 0) — same rationale
/// as test-proc-init: Phase C bookkeeping succeeds, VM is never scheduled.
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
    e[18] = 62;
    e[19] = 0; // e_machine = EM_X86_64
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

#[entry]
fn main() -> Status {
    // I-43 二层修复：UEFI 固件把 PE 加载到运行期地址——内核的恒等窗口
    // 必须覆盖真实镜像而非硬编码基址（BOOT-SCOPE 2a 之后窗口是最小的）。
    let (image_lo, image_hi) = uefi_helpers::shim_image_window();
    let image_base = image_lo & !0x001F_FFFF;
    let image_end = (image_hi + 0x001F_FFFF) & !0x001F_FFFF;

    early_console::write_str(
        "### test_smp_aps (x86_64): S-5 L4 — production smp_init multi-AP bring-up\n",
    );

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
    static mut PLATFORM_SOURCES: [PlatformDescSource; 4] = [
        PlatformDescSource::new(minix_boot::RSDP, PhysBytes(0));
        4
    ];
    // SAFETY: single-threaded boot; the array is filled once here and the
    // slice is read for the rest of the run.
    let platform_sources: &'static [PlatformDescSource] = unsafe {
        let sources = &mut *core::ptr::addr_of_mut!(PLATFORM_SOURCES);
        let n = uefi_helpers::find_platform_sources_into(sources);
        &sources[..n]
    };

    let kernel_info = KernelInfo {
        memmap: &MEMMAP,
        kern_virt_base: VirBytes(0xFFFF_8000_0000_0000),
        kern_phys_base: PhysBytes(image_base),
        kern_size: image_end - image_base,
        boot_modules,
        platform_sources,
        param_buf: &[],
        reserved_regions: &[],
        memmap_sources: &[],
        authority: minix_boot::InputObjectId::UefiMemoryMap,
    };


    uefi_helpers::exit_boot_services();
    unsafe {
        BOOT_SERVICES_EXITED = true;
    }

    // 2. Paging (Phase A).
    boot_alloc::init_boot_pt_alloc(bump_base, bump_end);
    pt_alloc::register(boot_alloc::boot_pt_alloc);
    let info = minix_kernel::arch_boot_impl::<minix_arch::arch::boot_arch::X86_64BootArch>(&kernel_info, root_page);
    early_console::write_str("  paging enabled\n");

    // Phase A.5: platform discovery FIRST (kmain order) — init_protection
    // reads the topology to program the BSP's GS area.
    unsafe { minix_platform::init_from_kinfo(&kernel_info) };

    // 3. Production Phase B: protection + trap entry (fills the
    // PROTECTION/TRAP_ENTRY globals — the AP tail's init_ap consumes them).
    minix_kernel::init_protection(&kernel_info);
    early_console::write_str("  protection + IDT live (production phase)\n");

    // 4. Clock + interrupt controller (kmain Phase B order).
    minix_kernel::init_clock_and_interrupts();
    early_console::write_str("  clock + controller initialized\n");

    // 5. Process table (Phase C) + SMP state (Phase D).
    minix_kernel::init_proc_and_boot(&kernel_info);
    minix_kernel::init_smp_state();
    early_console::write_str("  proc table + smp state initialized\n");

    // 6. Orchestration: serial per-AP handshake + wait_for_aps BKL
    // dance. C main.c:149 parity — the BKL is acquired ONCE before
    // smp_init and stays held (wait_for_aps's dance preserves it).
    minix_kernel::smp::bkl_lock().transfer();
    // Test-only: arm the PIT BEFORE smp_init so smp_init's per-AP bounded
    // wait is wall-anchored (uptime ticks flow). Production kmain runs
    // smp_init before bsp_finish_booting's boot_init_timer (C order) — its
    // deadline there falls to the raw-TSC backstop.
    minix_kernel::boot_init_timer();
    // IF=1 during smp_init: the per-AP bounded wait's uptime clock needs
    // PIT ticks to flow (the tick handler inherits this held BKL).
    unsafe {
        core::arch::asm!("sti", options(nomem, nostack));
    }
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

    // **Deterministic** criterion section for the scheduler hand-off and the
    // per-CPU local timer: every AP has
    // entered the shared scheduling loop (`scheduler_loop` sets a per-CPU flag
    // at entry), and every AP's local LAPIC one-shot (vector 0xF1) has been
    // serviced at least once. The BSP does not set that flag — it runs this
    // test body. Interrupts stay enabled (IF=1); the BKL-inheritance convention
    // of the tick handler chain matches test-timer-irq, but the wait below **no
    // longer holds the lock**; see the unlock note immediately after.
    unsafe {
        core::arch::asm!("sti", options(nomem, nostack));
    }
    // BKL witness: the two health assertions above do not read lock-protected
    // data, and the wait below **must release the BKL first**. The old shape
    // polled while holding it, which distorted two things at once: an AP must
    // contend for the BKL to enter `scheduler_loop`, and an AP's 0xF1 local tick
    // handler must inherit the BKL to service and rearm the one-shot — a starved
    // AP could show `in_loop=1` with `ticks=0` (reproduced on real hardware).
    // That is why the old version only printed and never asserted. The current
    // shape: release the BKL → poll against the TSC budget → assert.
    minix_kernel::smp::bkl_unlock();
    // The budget is in TSC cycles (after releasing the BKL there is no
    // lock-protected clock available): 120G cycles — QEMU TSC commonly runs
    // 1–3.5 GHz, so roughly 35–120s of wall time. Exceeding the budget is not a
    // fuzzy in-loop outcome but an immediate FAIL — the "TCG starvation"
    // WARNING is exactly what this gate item exists to eliminate.
    const WAIT_BUDGET_CYCLES: u64 = 120_000_000_000;
    let tsc0 = minix_kernel::clock::read_tsc();
    let (aps_in_loop, aps_ticked) = loop {
        let mut aps_in_loop = true;
        let mut aps_ticked = false;
        for cpu in 1..topo.nr_cpus as u32 {
            let entered = unsafe { minix_kernel::smp_state_boot_unchecked() }
                .cpu_local(minix_kernel::proc::CpuId::new_unchecked(cpu))
                .map(|l| l.sched_loop_entered)
                .unwrap_or(false);
            let ticks = minix_kernel::clock::LOCAL_TICK_COUNT[cpu as usize % 16]
                .load(core::sync::atomic::Ordering::Relaxed);
            if !entered {
                aps_in_loop = false;
            }
            if ticks > 0 {
                // "At least one secondary core counted a local tick" — see the
                // criterion comment below: an idle AP deliberately stops its
                // local timer, so a zero count on one core is not a defect.
                aps_ticked = true;
            }
        }
        if (aps_in_loop && aps_ticked)
            || minix_kernel::clock::read_tsc().wrapping_sub(tsc0) > WAIT_BUDGET_CYCLES
        {
            break (aps_in_loop, aps_ticked);
        }
        core::hint::spin_loop();
    };
    for cpu in 0..topo.nr_cpus as u32 {
        let entered = unsafe { minix_kernel::smp_state_boot_unchecked() }
            .cpu_local(minix_kernel::proc::CpuId::new_unchecked(cpu))
            .map(|l| l.sched_loop_entered)
            .unwrap_or(false);
        let ticks = minix_kernel::clock::LOCAL_TICK_COUNT[cpu as usize % 16]
            .load(core::sync::atomic::Ordering::Relaxed);
        early_console::write_str("  cpu ");
        early_console::write_hex(cpu as u64);
        early_console::write_str(" in_loop: ");
        early_console::write_hex(entered as u64);
        early_console::write_str(" ticks: ");
        early_console::write_hex(ticks);
        early_console::write_str("\n");
    }
    if !aps_in_loop {
        early_console::write_str(
            "  FAIL: AP did not all enter the scheduling loop with the BKL released\n",
        );
        fail();
    }
    early_console::write_str("  all APs entered the scheduling loop\n");
    // Assertion form of the per-CPU local timer contract: the secondary
    // cores' LAPIC one-shot (vector 0xF1, armed by `init_local_timer` at
    // the tail of `init_ap`) really did fire — no longer "a human reads
    // the serial numbers".
    // The criterion is "at least one AP counted a tick". Why not "every AP
    // counted one": an AP with no runnable process parks in the idle loop and
    // deliberately stops its local timer (`context_stop_idle` — the wake arm
    // restarts it when work arrives). Measurements consistently show cpu1/cpu2
    // count while cpu3 reads 0 with `in_loop=1`, and removing the BSP clamp
    // (C2E) did not change that: cpu3 had no process to wake it. A per-core
    // tick>0 criterion would therefore assert the scheduler's idle policy,
    // not the timer path; this gate stays at "at least one AP". A future gate
    // that assigns work to every AP can tighten it to per-core.
    if !aps_ticked {
        early_console::write_str(
            "  FAIL: no AP local LAPIC tick fired within the budget (see per-cpu ticks)\n",
        );
        fail();
    }
    early_console::write_str("  AP local LAPIC ticks observed (deterministic gate)\n");

    early_console::write_str("### TEST_RESULT: PASS test-smp-aps ###\n");
    loop {
        unsafe {
            asm!("cli", options(nomem, nostack));
        }
    }
}

fn fail() -> ! {
    early_console::write_str("### TEST_RESULT: FAIL test-smp-aps ###\n");
    loop {
        unsafe {
            asm!("hlt", options(nomem, nostack));
        }
    }
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
    loop {
        unsafe {
            asm!("hlt", options(nomem, nostack));
        }
    }
}
