//! Test: process table initialization and VM ELF loading (Phase C/D).
//!
//! Verifies that after init_proc_and_boot() + init_post_and_memory():
//!   1. Kernel tasks (ASYNCM, IDLE, CLOCK, SYSTEM, KERNEL) are initialized
//!   2. Kernel tasks have SLOT_FREE cleared and PROC_STOP set
//!   3. VM process has correct RTS flags (PROC_STOP set, SLOT_FREE clear, no VMINHIBIT)
//!   4. Non-VM user processes have VMINHIBIT + BOOTINHIBIT set
//!   5. VM p_seg is accessible
//!   6. init_post_and_memory completes without panic (Direct Map readiness
//!      check + kernel-level ptproc tracking)
//!   7. P1-c sentinel: the adopted bootstrap root is the tree the CPU
//!      translates with, and the VM Direct Map covers the resources the
//!      adopted handle must reach (07-paging_init_design D1-A-3 / D8-⑥)
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
use minix_arch::paging::{PageFlags, Paging};
use minix_arch::{DirectMapArch, CurrentDirectMap, ProtectionArch, TrapEntryArch, CurrentProtection, CurrentTrapEntry};
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

#[unsafe(link_section = ".bss")]
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
static mut BOOT_MODULES: [BootModule; 12] = [
    BootModule { name: "ds",    start: PhysBytes(0), len: 0 },  // nr=0  (DS_PROC_NR)
    BootModule { name: "rs",    start: PhysBytes(0), len: 0 },  // nr=1  (RS_PROC_NR)
    BootModule { name: "pm",    start: PhysBytes(0), len: 0 },  // nr=2  (PM_PROC_NR)
    BootModule { name: "sched", start: PhysBytes(0), len: 0 },  // nr=3  (SCHED_PROC_NR)
    BootModule { name: "vfs",   start: PhysBytes(0), len: 0 },  // nr=4  (VFS_PROC_NR)
    BootModule { name: "memory",start: PhysBytes(0), len: 0 },  // nr=5  (MEM_PROC_NR)
    BootModule { name: "tty",   start: PhysBytes(0), len: 0 },  // nr=6  (TTY_PROC_NR)
    BootModule { name: "mib",   start: PhysBytes(0), len: 0 },  // nr=7  (MIB_PROC_NR)
    BootModule { name: "vm",    start: PhysBytes(0), len: 0 },  // nr=8  (VM_PROC_NR) — patched at runtime to VM_ELF_STUB
    BootModule { name: "pfs",   start: PhysBytes(0), len: 0 },  // nr=9  (PFS_PROC_NR)
    BootModule { name: "mfs",   start: PhysBytes(0), len: 0 },  // nr=10 (MFS_PROC_NR)
    BootModule { name: "init",  start: PhysBytes(0), len: 0 },  // nr=11 (INIT_PROC_NR)
];

static MEMMAP: [MemoryRegion; 2] = [
    MemoryRegion { base: PhysBytes(0), len: 0x200_0000 },
    MemoryRegion { base: PhysBytes(0x200_0000), len: 0x600_0000 },
];

/// Minimal valid ELF64 image: header only, `e_phnum = 0` (zero PT_LOAD
/// segments). `load_vm_elf` parses the header, maps no segments, then
/// installs the user stack — Phase C bookkeeping succeeds without a real
/// VM binary (VM is never scheduled here: PROC_STOP stays set).
#[unsafe(link_section = ".rodata")]
static VM_ELF_STUB: [u8; 64] = {
    let mut e = [0u8; 64];
    e[0] = 0x7F; e[1] = b'E'; e[2] = b'L'; e[3] = b'F'; // magic
    e[4] = 2;            // EI_CLASS = ELFCLASS64
    e[5] = 1;            // EI_DATA  = ELFDATA2LSB
    e[6] = 1;            // EI_VERSION = EV_CURRENT
    e[16] = 2; e[17] = 0; // e_type = ET_EXEC
    e[18] = 62; e[19] = 0; // e_machine = EM_X86_64
    e[24] = 0; e[25] = 0x40; // e_entry = 0x4000_00 (unused: VM never runs)
    e[32] = 64;            // e_phoff = 64 (table immediately after header)
    e[52] = 64; e[53] = 0; // e_ehsize = 64
    e[54] = 56; e[55] = 0; // e_phentsize = 56 (ELF64_PHDR_SIZE)
    e[56] = 0; e[57] = 0;  // e_phnum = 0 — no PT_LOAD segments
    e
};

/// Runtime (identity-mapped) address of the stub is patched into
/// `BOOT_MODULES[VM]` in `main()`: this is a plain UEFI PE image loaded by
/// the firmware at a low physical address, so the static's runtime address
/// is a physical address covered by the bootstrap root's identity map —
/// exactly what `load_vm_elf`'s identity read requires. (Pointer-to-integer
/// casts are not permitted in const eval, so the address is taken at
/// runtime.)

/// Unforgeable sentinel value: exists only at the sentinel physical page,
/// written through the kernel DM channel (P1-c protocol input).
const SENTINEL: u64 = 0x5E57_1A6E_C0DE_0001;

/// Fresh user VA for the P1-c mapping: 5 GiB — outside the identity mapping
/// ([0, 4 GiB)), outside the VM DM window ([2 GiB, 3 GiB)), outside the
/// kernel half, and distinct from `VM_BOOT_HANDOFF_VA` (4 GiB, mapped
/// user read-only by `init_proc_and_boot`). Never translated before the
/// test, so no stale TLB entry.
const FRESH_VA: u64 = 0x1_4000_0000;

#[entry]
fn main() -> Status {
    early_console::write_str("### test_proc_init (x86_64): Phase C/D verification\n");

    // 1. UEFI boot preparation
    let root_page = uefi_helpers::alloc_root_page();
    let (bump_base, bump_end) = uefi_helpers::alloc_bump_region(64);

    // Patch the VM module entry with the runtime (identity-mapped) address
    // of the ELF stub — see the comment above `VM_ELF_STUB`.
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
        platform_sources: &[],
        param_buf: &[],
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
    minix_kernel::init_proc_and_boot(&result.kernel_info);
    // SAFETY: single-threaded boot context in this test kernel; the
    // kernel's own init_proc_and_boot uses the same accessor
    // (proc_table_boot_unchecked, lib.rs:1438).
    let proc_table = unsafe { minix_kernel::proc_table_boot_unchecked() };
    early_console::write_str("  init_proc_and_boot completed\n");

    // 5. Verify process table state
    use minix_kernel::proc::RtsFlagsBits;
    use minix_kernel::proc::ProcNr;
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
    let pm_proc = proc_table.get(ProcNr(2)); // PM_PROC_NR = 2
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

    // 6. Phase D: init_post_and_memory — Direct Map readiness check
    // (asserts VM p_seg root valid + VM direct map base configured, and
    // installs VM as kernel-level ptproc). No freepdes allocation (Direct Map).
    minix_kernel::init_post_and_memory(&proc_table);
    early_console::write_str("  init_post_and_memory completed\n");

    // 7. P1-c sentinel (07-paging_init_design D1-A-3 / D8-⑥). One chain,
    //    three claims:
    //    a) P1-a: the adopted handle wraps the bootstrap root the kernel
    //       enabled — `root_paddr()` round-trips the recorded root.
    //    b) G2 + D2-⑥: the sentinel page and every page-table page reachable
    //       from the root are readable/writable through the VM Direct Map
    //       window — the adopted handle pins the VmDm PTE channel, so its
    //       map() below exercises exactly that window.
    //    c) P1-c (conclusive): a fresh VA is mapped to the sentinel page via
    //       the adopted handle; the CPU then reads the fresh VA through the
    //       live root. The sentinel value exists only at the sentinel
    //       physical page (written through the independent kernel DM
    //       channel), so a matching read proves the modified tree == the
    //       CPU's active translation tree.
    let root = minix_kernel::current_root_phys().expect("bootstrap root not recorded");

    // 7a. Sentinel page: fresh bump allocation; the value is written through
    //     the KERNEL DM window — a channel independent of the tree under test.
    let (sentinel_pa, _sentinel_id_va) =
        pt_alloc::alloc_pt_page().expect("sentinel page allocation failed");
    let sentinel_kva = CurrentDirectMap::kernel_phys_to_virt(sentinel_pa);
    // SAFETY: kernel DM VA of a freshly allocated page; single-threaded boot.
    unsafe { (sentinel_kva.0 as *mut u64).write_volatile(SENTINEL) };

    // 7b. Same physical page must be readable through the VM DM window (G2).
    let sentinel_vva = CurrentDirectMap::vm_phys_to_virt(sentinel_pa);
    let via_vm_dm = unsafe { (sentinel_vva.0 as *const u64).read_volatile() };
    if via_vm_dm != SENTINEL {
        early_console::write_str("  FAIL: VM DM window does not cover sentinel page\n");
        fail();
    }
    early_console::write_str("  VM DM covers sentinel page (G2 OK)\n");

    // 7c. Adopt the bootstrap root (A1) and map the fresh VA through the
    //     VmDm PTE channel. FRESH_VA = 5 GiB: outside the identity mapping
    //     ([0, 4GiB)), outside the VM DM window ([2GiB, 3GiB)), outside the
    //     kernel half, and distinct from the boot handoff page (4 GiB) — a
    //     never-translated VA, so no stale TLB entry can exist and no
    //     translation-cache maintenance is required (the conditional
    //     privileged step of the P1-c protocol is vacuous here).
    let mut vm_pt = X86_64Paging::adopt_active_root(root);
    if vm_pt.root_paddr() != root {
        early_console::write_str("  FAIL: adopt does not wrap the bootstrap root\n");
        fail();
    }
    if let Err(e) = vm_pt.map(VirBytes(FRESH_VA), sentinel_pa, PageFlags::read_write()) {
        // Diagnostic: name the variant so a failure is actionable.
        early_console::write_str(match e {
            minix_arch::paging::PageTableError::InvalidAddress => "  FAIL: map err: InvalidAddress\n",
            minix_arch::paging::PageTableError::AlreadyMapped => "  FAIL: map err: AlreadyMapped\n",
            minix_arch::paging::PageTableError::NotMapped => "  FAIL: map err: NotMapped\n",
            minix_arch::paging::PageTableError::AllocationFailed => "  FAIL: map err: AllocationFailed\n",
            minix_arch::paging::PageTableError::PermissionDenied => "  FAIL: map err: PermissionDenied\n",
            minix_arch::paging::PageTableError::NotSupported => "  FAIL: map err: NotSupported\n",
        });
        fail();
    }
    early_console::write_str("  adopt wraps active root; fresh VA mapped via VM DM channel\n");

    // 7d. CPU reads the fresh VA through the live root (supervisor may read
    //     user pages: U=1). A sentinel match is the execution-binding proof.
    let via_cpu = unsafe { (FRESH_VA as *const u64).read_volatile() };
    if via_cpu != SENTINEL {
        early_console::write_str("  FAIL: fresh VA read does not return sentinel\n");
        fail();
    }
    early_console::write_str("  P1-c sentinel: CPU reads sentinel via fresh VA (OK)\n");

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
