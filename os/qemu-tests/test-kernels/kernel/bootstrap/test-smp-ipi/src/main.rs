//! Test: IPI round-trip (SMP bring-up).
//!
//! Verifies the SMP mechanism's first real cross-CPU round-trip: after
//! smp_init brings all APs online and into the scheduling loop, the BSP
//! issues a synchronous schedule IPI to AP1 (schedule_sync: set flags →
//! send → release BKL → wait pending-clear → re-acquire). The AP wakes from
//! its idle halt on the 0xF0 IPI vector, and the trap dispatcher routes it
//! to `sched_handler_full`, which applies and CLEARS the pending flags —
//! the BSP's completion signal.
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
use minix_boot::{KernelInfo, BootModule, MemoryRegion};
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
    BootModule { name: "ds",    start: PhysBytes(PLACEHOLDER_SPAN), len: 64 },
    BootModule { name: "rs",    start: PhysBytes(PLACEHOLDER_SPAN), len: 64 },
    BootModule { name: "pm",    start: PhysBytes(PLACEHOLDER_SPAN), len: 64 },
    BootModule { name: "sched", start: PhysBytes(PLACEHOLDER_SPAN), len: 64 },
    BootModule { name: "vfs",   start: PhysBytes(PLACEHOLDER_SPAN), len: 64 },
    BootModule { name: "memory",start: PhysBytes(PLACEHOLDER_SPAN), len: 64 },
    BootModule { name: "tty",   start: PhysBytes(PLACEHOLDER_SPAN), len: 64 },
    BootModule { name: "mib",   start: PhysBytes(PLACEHOLDER_SPAN), len: 64 },
    BootModule { name: "vm",    start: PhysBytes(PLACEHOLDER_SPAN), len: 64 },  // patched at runtime
    BootModule { name: "pfs",   start: PhysBytes(PLACEHOLDER_SPAN), len: 64 },
    BootModule { name: "mfs",   start: PhysBytes(PLACEHOLDER_SPAN), len: 64 },
    BootModule { name: "init",  start: PhysBytes(PLACEHOLDER_SPAN), len: 64 },
];

/// Placeholder module span: real RAM inside MEMMAP (name-only slots at
/// Phase C are DEFERRED; the span exists so the boot identity windows are
/// real `[start, end)` ranges, never empty).
const PLACEHOLDER_SPAN: u64 = 0x100_0000;

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
    // I-43 二层修复：UEFI 固件把 PE 加载到运行期地址——内核的恒等窗口
    // 必须覆盖真实镜像而非硬编码基址（BOOT-SCOPE 2a 之后窗口是最小的）。
    let (image_lo, image_hi) = uefi_helpers::shim_image_window();
    let image_base = image_lo & !0x001F_FFFF;
    let image_end = (image_hi + 0x001F_FFFF) & !0x001F_FFFF;

    early_console::write_str("### test_smp_ipi (x86_64): S-10 L6 — BSP↔AP schedule IPI round-trip\n");

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
    static mut PLATFORM_SOURCES: [minix_boot::PlatformDescSource; 4] = [
        minix_boot::PlatformDescSource::new(minix_boot::RSDP, PhysBytes(0));
        4
    ];
    // SAFETY: single-threaded boot; the array is filled once here and the
    // slice is read for the rest of the run.
    let platform_sources: &'static [minix_boot::PlatformDescSource] = unsafe {
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
        source_records: &[],
        authority: minix_boot::RamAuthority::UEFI,
    };


    uefi_helpers::exit_boot_services();
    unsafe { BOOT_SERVICES_EXITED = true; }

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

    // BSP → AP1 synchronous schedule IPI round-trip. AP1 is in
    // the scheduling loop's idle halt (IF=1); the 0xF0 IPI wakes it and the
    // trap dispatcher routes it to sched_handler_full, which clears the
    // pending flags — schedule_sync's return IS the round-trip proof.
    use minix_arch::{CurrentSmpArch, SmpArch as _};
    use minix_kernel::proc::CpuId;
    use minix_kernel::smp::SchedIpiFlags;
    {
        let table = unsafe { minix_kernel::proc_table_boot_unchecked() };
        let smp = unsafe { minix_kernel::smp_state_boot_unchecked() };
        early_console::write_str("  sending schedule_sync to AP1 (SAVE_CTX)...\n");
        smp.schedule_sync::<CurrentSmpArch>(
            table,
            CpuId::new_unchecked(1),
            smp.bsp_cpu_id(),
            minix_kernel::proc::proc_nr::IDLE,
            SchedIpiFlags::SAVE_CTX,
        );
        early_console::write_str("  schedule_sync returned (pending cleared by AP1)\n");
    }

    // Every AP entered the shared scheduling loop (per-CPU flag set
    // at `scheduler_loop` entry); the BSP did not (it runs this test body
    // instead). Serial-visible per-CPU activity marker.
    let mut aps_in_loop = true;
    for cpu in 0..topo.nr_cpus as u32 {
        let entered = unsafe { minix_kernel::smp_state_boot_unchecked() }
            .cpu_local(minix_kernel::proc::CpuId::new_unchecked(cpu))
            .map(|l| l.sched_loop_entered)
            .unwrap_or(false);
        let is_ap = cpu != 0;
        early_console::write_str("  cpu ");
        early_console::write_hex(cpu as u64);
        early_console::write_str(" in_loop: ");
        early_console::write_hex(entered as u64);
        early_console::write_str("\n");
        if is_ap && !entered {
            aps_in_loop = false;
        }
    }
    if !aps_in_loop {
        early_console::write_str("  FAIL: an AP did not enter the scheduling loop\n");
        fail();
    }
    early_console::write_str("  all APs entered the scheduling loop\n");

    early_console::write_str("### TEST_RESULT: PASS test-smp-ipi ###\n");
    loop { unsafe { asm!("cli", options(nomem, nostack)); } }
}

fn fail() -> ! {
    early_console::write_str("### TEST_RESULT: FAIL test-smp-ipi ###\n");
    loop { unsafe { asm!("hlt", options(nomem, nostack)); } }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    early_console::write_str("### PANIC in test-smp-ipi: ");
    if let Some(loc) = info.location() {
        early_console::write_str(loc.file());
        early_console::write_str(":");
        early_console::write_hex(loc.line() as u64);
    }
    early_console::write_str(" ###\n");
    loop { unsafe { asm!("hlt", options(nomem, nostack)); } }
}
