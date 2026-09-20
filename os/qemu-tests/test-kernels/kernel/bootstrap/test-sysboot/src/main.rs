//! Test: multi-process boot carrier (S42 ④ / C-27) — the kernel boots TWO
//! user images through the production boot path and the two exchange the
//! first user↔user IPC round trip on real machine.
//!
//! Layout:
//!   - VM boot slot (proc_nr 8, endpoint 8) runs `sysboot-rx`: parks in
//!     `receive(ANY)`, answers the exchange, parks again.
//!   - RS slot (proc_nr 2, endpoint 2) runs `sysboot-tx`: `sendrec` to
//!     endpoint 8, comes back with the reply.
//!
//! The VM image is loaded by `init_proc_and_boot` itself (the production
//! VM branch). The RS image is loaded by this carrier right after, with
//! the exact machinery that branch uses (`load_vm_elf` on the active
//! bootstrap root + `VmBootAllocator`), then the slot's CPU context is
//! rebuilt for the loaded entry (`build_cpu_context` — the same call
//! `init_proc_and_boot` makes for boot processes). The two images link at
//! different bases (RX 0x140000000, TX 0x180000000) so both coexist in
//! the shared bootstrap address space without a segment collision.
//!
//! PASS = the run script (qemu-tests/test-sysboot.sh) finds the exchange
//! markers on serial: RX UP → TX UP → RX GOT → RX DONE → TX GOT →
//! TX DONE (the TX UP after RX UP ordering is not required — either
//! process may be dispatched first; blocking semantics make both orders
//! converge).
//!
//! Boot flow: identical to test-rt-birth through smp_init (BKL held),
//! then: VM and RS slots unstopped (`rts_unset PROC_STOP`), and
//! `switch_to_user()` hands the CPU over into the production scheduler
//! loop.

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
    BootModule { name: "rs",    start: PhysBytes(0), len: 0 },  // ← sysboot-tx
    BootModule { name: "pm",    start: PhysBytes(0), len: 0 },
    BootModule { name: "sched", start: PhysBytes(0), len: 0 },
    BootModule { name: "vfs",   start: PhysBytes(0), len: 0 },
    BootModule { name: "memory",start: PhysBytes(0), len: 0 },
    BootModule { name: "tty",   start: PhysBytes(0), len: 0 },
    BootModule { name: "mib",   start: PhysBytes(0), len: 0 },
    BootModule { name: "vm",    start: PhysBytes(0), len: 0 },  // ← sysboot-rx
    BootModule { name: "pfs",   start: PhysBytes(0), len: 0 },
    BootModule { name: "mfs",   start: PhysBytes(0), len: 0 },
    BootModule { name: "init",  start: PhysBytes(0), len: 0 },
];

static MEMMAP: [MemoryRegion; 2] = [
    MemoryRegion { base: PhysBytes(0), len: 0x200_0000 },
    MemoryRegion { base: PhysBytes(0x200_0000), len: 0x600_0000 },
];

/// Page-aligned blob wrapper: boot-module regions enter the VM handoff's
/// A2 deduction record, which asserts page-aligned starts (vm_handoff.rs)
/// — a bare `&[u8]` include has alignment 1 and its address lands wherever
/// the EFI image's rodata puts it.
#[repr(C, align(4096))]
struct AlignedBlob<const N: usize>([u8; N]);

/// The receive half (VM boot slot), embedded at kernel build time.
static SYSBOOT_RX_ELF: AlignedBlob<{ include_bytes!(env!("SYSBOOT_RX_ELF_PATH")).len() }> =
    AlignedBlob(*include_bytes!(env!("SYSBOOT_RX_ELF_PATH")));
/// The send half (RS slot), embedded at kernel build time. Same alignment
/// contract as [`SYSBOOT_RX_ELF`].
static SYSBOOT_TX_ELF: AlignedBlob<{ include_bytes!(env!("SYSBOOT_TX_ELF_PATH")).len() }> =
    AlignedBlob(*include_bytes!(env!("SYSBOOT_TX_ELF_PATH")));

#[entry]
fn main() -> Status {
    early_console::write_str("### test_sysboot (x86_64): multi-process boot carrier\n");

    // 1. UEFI boot preparation (test-rt-birth shape).
    let root_page = uefi_helpers::alloc_root_page();
    let (bump_base, bump_end) = uefi_helpers::alloc_bump_region(512);
    // SAFETY: single-threaded boot; BOOT_MODULES is read-only afterwards.
    let boot_modules: &'static [BootModule] = unsafe {
        let mods = &mut *core::ptr::addr_of_mut!(BOOT_MODULES);
        mods[8] = BootModule {
            name: "vm",
            start: PhysBytes(SYSBOOT_RX_ELF.0.as_ptr() as usize as u64),
            len: SYSBOOT_RX_ELF.0.len(),
        };
        mods[1] = BootModule {
            name: "rs",
            start: PhysBytes(SYSBOOT_TX_ELF.0.as_ptr() as usize as u64),
            len: SYSBOOT_TX_ELF.0.len(),
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
    let _info = minix_kernel::arch_boot_impl::<X86_64Paging>(&result.kernel_info, result.root_page);
    early_console::write_str("  paging enabled\n");

    // Phase A.5: platform discovery FIRST (kmain order).
    unsafe { minix_platform::init_from_kinfo(&result.kernel_info) };

    // 3. Production Phase B: protection + trap entry + clock.
    minix_kernel::init_protection(&result.kernel_info);
    early_console::write_str("  protection + IDT live (production phase)\n");

    minix_kernel::init_clock_and_interrupts();
    early_console::write_str("  clock + controller initialized\n");

    // 4.5. Kernel information page (kmain Phase B.5).
    minix_kernel::init_kerninfo(&result.kernel_info);
    early_console::write_str("  kerninfo page published\n");

    // 5. Process table (Phase C) — the VM branch loads sysboot-rx into the
    // VM boot process; the RS slot gets its privilege + zeroed entry (the
    // real RS image would be exec'd later; this carrier loads it below).
    early_console::write_str("  calling init_proc_and_boot\n");
    minix_kernel::init_proc_and_boot(&result.kernel_info);
    early_console::write_str("  init_proc_and_boot done\n");
    minix_kernel::init_smp_state();
    early_console::write_str("  proc table + smp state initialized\n");

    // 6. BKL held (C main.c:149); single CPU is enough.
    minix_kernel::smp::bkl_lock().transfer();
    unsafe { core::arch::asm!("sti", options(nomem, nostack)); }
    minix_kernel::smp::smp_init();
    early_console::write_str("  smp_init completed\n");

    // 7. Load sysboot-tx into the RS slot with the production machinery —
    // a PER-IMAGE root (`load_process_elf`, OQ-N6 ② verdict): the fresh
    // root inherits the supervisor half of the live bootstrap root, then
    // maps the image's segments + stack. Unlike the old shared-root
    // recipe the VM branch runs (which can host exactly one image — the
    // second stack mapping collided, C-27 real-machine evidence), the TX
    // image may reuse the same stack window and link base as RX. The
    // slot's `p_seg.phys_root` records the new root; the scheduler's
    // address-space switch (C proc.c:349 对位) installs it at dispatch.
    {
        use minix_arch::CurrentPaging;
        use minix_arch::frame::{VmBootAllocator, VmBootRegion};
        use minix_arch::{CpuContextArch, CurrentCpuContextArch, EntrySpec, ProcKind, load_process_elf};


        let regions = {
            // Frames MUST come from the A2 post-deduction free list, not
            // the raw boot memmap: init_proc_and_boot's first load
            // consumed frames (segments + stack + PT pages + handoff) the
            // raw map still advertises. Re-allocating them zeroes/copies
            // over the RX image's memory — RX then executes corrupted
            // bytes with an intact page table (the #UD this carrier hit
            // on real machine once the per-image-root load unblocked the
            // second load). C parity: every pg_alloc_page consumer walks
            // the MUTATED memmap (pg_utils.c:138-160), so the second
            // loader never sees spent frames. Deduction already covers
            // the kernel image, bump region, VM's bootstrap frames and
            // the reserved module blobs — no extra exclusions needed.
            let (free_regions, n_free) = minix_kernel::vm_handoff::vm_pmm_free_regions();
            VmBootRegion::select_multi(&free_regions[..n_free], &[])
                .expect("test-sysboot: bootstrap region selection failed")
                .unwrap_or_else(|| panic!("test-sysboot: no free memory for the TX load"))
        };
        let mut vm_alloc = VmBootAllocator::new(regions);

        let root_phys = minix_kernel::current_root_phys()
            .expect("test-sysboot: bootstrap root not set — arch_boot_impl must run first");
        let access = minix_arch::CurrentDirectMap::default();
        let tx_module = &result.kernel_info.boot_modules()[1]; // "rs"
        let tx = load_process_elf::<CurrentPaging, _>(
            tx_module,
            &result.kernel_info,
            root_phys,
            &mut vm_alloc,
            &access,
        )
        .unwrap_or_else(|e| panic!("test-sysboot: TX ELF load failed: {e:?}"));
        early_console::write_str("  TX ELF loaded into the RS slot (per-image root)\n");
        {
        {
            // [fix] PDPT-slot merge for the kernel's LOW-half footprint.
            // This test kernel executes identity-mapped (UEFI-era shape:
            // kernel code at ~0xdb9xxxxx, PML4 entry 0 — the USER half),
            // so the entry-range supervisor inherit cannot carry it: the
            // same entry also hosts the first image's user mappings. The
            // merge copies the kernel-owned 1 GB PDPT slots from the
            // bootstrap root into the fresh root, leaving the images'
            // own slots (link base 0x140000000 → slot 5; stack → 511)
            // untouched:
            //   slot(kernel_va)   — supervisor code/data + IDT/GDT targets
            //   slot(KERNINFO_USER_VA) — the published kernel-info page
            //     (read-only): the birth chain's kerninfo query returns
            //     this VA and the payload dereferences it immediately —
            //     without the slot the read page-faults in the new root
            //     (CR2 = 0x200000000 observed on real machine).
            // Production higher-half kernels need none of this: their
            // supervisor footprint IS entries 256..512, already inherited.
            use minix_arch::frame::PhysAccess as _;
            let kernel_va = main as *const () as u64;
            let kern_slot = ((kernel_va >> 30) & 0x1ff) as usize;
            let kerninfo_slot =
                ((minix_kernel::kerninfo::KERNINFO_USER_VA >> 30) & 0x1ff) as usize;
            let src_pml4 = access.phys_to_virt(PhysBytes(root_phys.0)).0 as *const u64;
            let dst_pml4 = access.phys_to_virt(PhysBytes(tx.root.0)).0 as *mut u64;
            // Source entry 0 → bootstrap's PDPT; dest entry 0 → the fresh
            // root's own PDPT (walk_alloc created it while mapping the
            // image). Copy ONLY the kernel-owned slots.
            let src_pdpt =
                unsafe { src_pml4.add(0).read() } & 0x000f_ffff_ffff_f000;
            let dst_pdpt =
                unsafe { dst_pml4.add(0).read() } & 0x000f_ffff_ffff_f000;
            if src_pdpt != 0 && dst_pdpt != 0 {
                let s = access.phys_to_virt(PhysBytes(src_pdpt)).0 as *const u64;
                let d = access.phys_to_virt(PhysBytes(dst_pdpt)).0 as *mut u64;
                for slot in [kern_slot, kerninfo_slot] {
                    unsafe {
                        let entry = s.add(slot).read();
                        d.add(slot).write(entry);
                    }
                }
                early_console::write_str("  identity merge: slots ");
                early_console::write_hex(kern_slot as u64);
                early_console::write_str(" + ");
                early_console::write_hex(kerninfo_slot as u64);
                early_console::write_str("\n");
            }
        }
        }

        // Rebuild the RS slot's CPU context for the loaded entry — the same
        // call init_proc_and_boot makes for boot processes (the slot was
        // built with a zeroed entry: the real RS execs its image later, this
        // carrier hands it the ELF directly).
        let table = unsafe { minix_kernel::proc_table_boot_unchecked() };
        {
            let rs = table
                .get_mut(minix_kernel::proc::proc_nr::RS_PROC_NR)
                .expect("RS boot proc exists");
            rs.cpu_context =
                <CurrentCpuContextArch as CpuContextArch>::build_cpu_context(
                    ProcKind::RootService,
                    minix_kernel::proc::proc_nr::RS_PROC_NR,
                    EntrySpec::loaded(tx.pc, tx.sp, tx.ps_strings),
                );
            // Record the fresh root so the scheduler's address-space
            // switch (C `switch_address_space(p)`, proc.c:349 — klib.S
            // __switch_address_space 对位) installs it when the RS slot
            // is dispatched. C 对位: `p_seg.p_cr3` (x86) / `p_ttbr` (ARM).
            rs.p_seg.phys_root = tx.root;
            rs.trap_style = minix_kernel::PublicTrapStyle::FullContext;
        }
        {
            let vm = table
                .get_mut(minix_kernel::proc::proc_nr::VM_PROC_NR)
                .expect("VM boot proc exists");
            vm.trap_style = minix_kernel::PublicTrapStyle::FullContext;
        }

        // Make both slots runnable — the production scheduler (switch_to_user)
        // picks by priority; blocking semantics converge either order.
        // init_proc_and_boot stamps the boot-stamp flag set on boot
        // processes (C: main.c:226 NO_PRIV|NO_QUANTUM, main.c:263-265
        // VMINHIBIT|BOOTINHIBIT, main.c:272 PROC_STOP — "not scheduled
        // until VM has set up a pagetable"). In THIS carrier the kernel
        // boot loader already built TX's address space (the per-image
        // root above), so the carrier stands in for the release half that
        // VM's pagetable arm (C: pagetable.c:933 VMCTL_VMINHIBIT_CLEAR)
        // and RS's service-up would perform: clear the WHOLE stamp set for
        // the RS slot. Clearing PROC_STOP alone left TX carrying
        // NO_PRIV|NO_QUANTUM|VMINHIBIT|BOOTINHIBIT — is_runnable() false
        // forever, never enqueued, never dispatched (the silent no-TX
        // this carrier hit on real machine).
        table.rts_unset(
            minix_kernel::proc::proc_nr::VM_PROC_NR,
            minix_kernel::proc::RtsFlagsBits::PROC_STOP,
        );
        table.rts_unset(
            minix_kernel::proc::proc_nr::RS_PROC_NR,
            minix_kernel::proc::RtsFlagsBits::PROC_STOP
                | minix_kernel::proc::RtsFlagsBits::VMINHIBIT
                | minix_kernel::proc::RtsFlagsBits::BOOTINHIBIT
                | minix_kernel::proc::RtsFlagsBits::NO_PRIV
                | minix_kernel::proc::RtsFlagsBits::NO_QUANTUM,
        );
    }
    early_console::write_str("  RX (vm) + TX (rs) slots runnable\n");

    // 8. Hand the CPU over — the production scheduler loop multiplexes the
    // two processes through the IPC exchange.
    early_console::write_str("  entering scheduler (switch_to_user)\n");
    minix_kernel::switch_to_user();
}

#[allow(dead_code)]
fn fail() -> ! {
    early_console::write_str("### TEST_RESULT: FAIL test-sysboot ###\n");
    loop { unsafe { core::arch::asm!("hlt", options(nomem, nostack)); } }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    early_console::write_str("### PANIC in test-sysboot: ");
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
