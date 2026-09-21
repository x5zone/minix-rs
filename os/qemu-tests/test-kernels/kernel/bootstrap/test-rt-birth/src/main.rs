//! Test: first minix-rt freestanding user binary on real machine (edge E1
//! slice 5 acceptance; 14-stage-runtime V1-P1-1 step 2; E-KERNINFO
//! end-to-end).
//!
//! The kernel loads the `rt-birth` ELF through the production boot path —
//! `init_proc_and_boot`'s VM branch (`load_vm_elf`: segments, 64 KiB stack,
//! C-shaped ps_strings, `build_cpu_context` with RSP/RBX) — then makes the
//! VM boot process runnable. The scheduler hands the CPU over at the ELF
//! entry (`_start`, RBX = ps_strings pointer), and the minix-rt birth chain
//! runs: descriptor parse → allocator init → MINIX_KERNINFO trap query →
//! publish → `main` → console output through SYS_DIAGCTL → forced panic
//! through the user-space diagnostic hook.
//!
//! The binary is embedded at kernel build time: the run script builds
//! `rt-birth` first (x86_64-unknown-none) and passes the artifact path in
//! `RT_BIRTH_ELF_PATH` (plain `cargo build -p test-rt-birth` fails with a
//! clear include error — set the env, or use the script).
//!
//! PASS = the run script (qemu-tests/test-rt-birth.sh) finds the birth
//! chain's serial markers (argv/progname parse, kerninfo ready + user_sp,
//! `RT-BIRTH MAIN OK`, panic render check).
//!
//! Boot flow: identical to test-user-trap through smp_init (BKL held),
//! then: VM boot process unstopped (`rts_unset PROC_STOP`), and
//! `switch_to_user()` hands the CPU over.

#![no_std]
#![no_main]

extern crate alloc;

use core::panic::PanicInfo;
use core::alloc::{GlobalAlloc, Layout};

use minix_arch::x86_64::paging::X86_64Paging;
use minix_arch::pt_alloc;
use minix_types::{PhysBytes, VirBytes};
use minix_plat::x86_64::early_console;
use minix_kernel::boot_alloc;
use minix_platform;
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
                (base + aligned) as *mut u8
            }
        }
    }
    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}
}

#[global_allocator]
static ALLOCATOR: HybridAllocator = HybridAllocator;

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

/// The rt-birth user image, embedded at kernel build time (see the crate
/// docs for the build contract). The kernel identity-maps its own image,
/// so the blob is readable through VA = PA during `load_vm_elf` — the same
/// mechanism the UEFI stub relied on in the other bootstrap tests.
static RT_BIRTH_ELF: &[u8] = include_bytes!(env!("RT_BIRTH_ELF_PATH"));


#[entry]
fn main() -> Status {
    early_console::write_str("### test_rt_birth (x86_64): first minix-rt user binary on machine\n");

    // 1. UEFI boot preparation (test-user-trap shape).
    let root_page = uefi_helpers::alloc_root_page();
    let (bump_base, bump_end) = uefi_helpers::alloc_bump_region(512);
    // SAFETY: single-threaded boot; BOOT_MODULES is read-only afterwards.
    let boot_modules: &'static [BootModule] = unsafe {
        let mods = &mut *core::ptr::addr_of_mut!(BOOT_MODULES);
        mods[8] = BootModule {
            name: "vm",
            start: PhysBytes(RT_BIRTH_ELF.as_ptr() as usize as u64),
            len: RT_BIRTH_ELF.len(),
        };
        mods
    };

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
        reserved_regions: &[],
    };

    let result = BootPrepareResult {
        kernel_info,
        root_page,
        bump_base,
        bump_end,
    };

    uefi_helpers::exit_boot_services();
    unsafe { BOOT_SERVICES_EXITED = true; }

    // 2. Paging (Phase A) — identity + higher-half + Direct Map coverage.
    minix_kernel::store_kernel_info(&result.kernel_info);
    boot_alloc::init_boot_pt_alloc(result.bump_base, result.bump_end);
    pt_alloc::register(boot_alloc::boot_pt_alloc);
    let info = minix_kernel::arch_boot_impl::<X86_64Paging>(&result.kernel_info, result.root_page);
    early_console::write_str("  paging enabled\n");

    // Phase A.5: platform discovery FIRST (kmain order).
    unsafe { minix_platform::init_from_kinfo(&result.kernel_info) };

    // 3. Production Phase B: protection + trap entry + clock.
    minix_kernel::init_protection(&result.kernel_info);
    early_console::write_str("  protection + IDT live (production phase)\n");

    minix_kernel::init_clock_and_interrupts();
    early_console::write_str("  clock + controller initialized\n");

    // 4.5. Kernel information page (kmain Phase B.5) — publishes the
    // MINIX_KERNINFO page the birth chain will trap-query.
    minix_kernel::init_kerninfo(&result.kernel_info);
    early_console::write_str("  kerninfo page published\n");

    // 5. Process table (Phase C) — the VM branch loads the embedded
    // rt-birth ELF into the VM boot process (segments, stack, ps_strings,
    // boot CPU context) and builds the VmBootHandoff page.
    early_console::write_str("  calling init_proc_and_boot\n");
    minix_kernel::init_proc_and_boot(&result.kernel_info);
    early_console::write_str("  init_proc_and_boot done\n");
    minix_kernel::init_smp_state();
    early_console::write_str("  proc table + smp state initialized\n");

    // 6. BKL held (C main.c:149); single CPU is enough — the birth chain
    // is a one-process story.
    minix_kernel::smp::bkl_lock().transfer();
    unsafe { core::arch::asm!("sti", options(nomem, nostack)); }
    minix_kernel::smp::smp_init();
    early_console::write_str("  smp_init completed\n");

    // 7. Make the VM boot process (running the rt-birth image) runnable.
    // The boot context is a full register file (build_cpu_context), so the
    // restore machinery must be told the recorded style — same contract as
    // test-user-trap's payload context.
    {
        let table = unsafe { minix_kernel::proc_table_boot_unchecked() };
        {
            let vm = table
                .get_mut(minix_kernel::proc::proc_nr::VM_PROC_NR)
                .expect("VM boot proc exists");
            vm.trap_style = minix_kernel::PublicTrapStyle::FullContext;
        }
        table.rts_unset(
            minix_kernel::proc::proc_nr::VM_PROC_NR,
            minix_kernel::proc::RtsFlagsBits::PROC_STOP,
        );
    }
    early_console::write_str("  VM boot proc (rt-birth): runnable\n");

    // 8. Hand the CPU over — the scheduler picks the rt-birth process and
    // restores it to CPL3 at the ELF entry.
    early_console::write_str("  entering scheduler (switch_to_user)\n");
    minix_kernel::switch_to_user();
}

#[allow(dead_code)]
fn fail() -> ! {
    early_console::write_str("### TEST_RESULT: FAIL test-rt-birth ###\n");
    loop { unsafe { core::arch::asm!("hlt", options(nomem, nostack)); } }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    early_console::write_str("### PANIC in test-rt-birth: ");
    if let Some(loc) = info.location() {
        early_console::write_str(loc.file());
        early_console::write_str(":");
        early_console::write_hex(loc.line() as u64);
    }
    early_console::write_str(" ");
    // Payload matters for boot-path diagnosis (e.g. which load_vm_elf
    // error variant fired). Minimal core::fmt adapter over EarlyConsole.
    struct Console;
    impl core::fmt::Write for Console {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            early_console::write_str(s);
            Ok(())
        }
    }
    let _ = core::fmt::write(&mut Console, format_args!("{}\n", info.message()));
    early_console::write_str(" ###\n");
    loop { unsafe { core::arch::asm!("hlt", options(nomem, nostack)); } }
}
