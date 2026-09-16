//! Test: user-mode trap bridge bring-up (E1 slice 5; edge E1).
//!
//! Verifies the full user → kernel → user round trip through the vector-33
//! IPC gate: the kernel maps a user code page (CPL3) containing an
//! `int 0x21` payload, builds a user context for the VM boot process,
//! enters the scheduling loop (`switch_to_user`), and the payload's trap
//! round-trips through `x86_ipc_dispatch_body` — an undefined call number
//! (99) must come back as EBADCALL(209) in RAX, and a MINIX_KERNINFO probe
//! (call 6, page unpublished in this test) must also return EBADCALL
//! (C: proc.c:602-606 default branch / proc.c:687-689 kerninfo-not-ready).
//!
//! Boot flow: identical to test-smp-aps through smp_init (multi-AP online,
//! both schedulers live), then: user pages mapped via the live root
//! (X86_64Paging + USER flag), payload written through the identity map,
//! VM boot process context built (`build_cpu_context(ProcKind::Vm, …)`),
//! `rts_unset(PROC_STOP)` makes it runnable, and `switch_to_user()` hands
//! the CPU over.
//!
//! PASS = GDB reads the user mailbox (physical 0x400_1000, mapped at
//! VA 0x1_0001_0000):
//!   [+0x00] == 209    round-1 errno (undefined call 99 → EBADCALL)
//!   [+0x08] == 0xDEAD payload completion marker
//!   [+0x18] == 209    round-2 errno (MINIX_KERNINFO, page unpublished)
//! The run script (qemu-tests/test-user-trap.sh) drives QEMU+GDB and
//! asserts the mailbox.

#![no_std]
#![no_main]

extern crate alloc;

use core::arch::asm;
use core::panic::PanicInfo;
use core::alloc::{GlobalAlloc, Layout};

use minix_arch::x86_64::paging::X86_64Paging;
use minix_arch::{CpuContextArch, CurrentCpuContextArch, EntrySpec, ProcKind};
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

// ── E1 slice 5: user payload + mailbox ──

/// User VAs live above 4 GiB, outside the boot identity map (VA = PA for
/// the first 4 GiB) so `map()` builds fresh PTEs instead of colliding with
/// identity entries. Physical backing sits in MEMMAP region 2 (32–128 MiB),
/// far from the kernel image (2 MiB) and the boot bump allocations.
const USER_CODE_VA: u64 = 0x1_0000_0000;
const USER_DATA_VA: u64 = 0x1_0001_0000;
const USER_STACK_VA: u64 = 0x1_0002_0000;
const USER_CODE_PHYS: u64 = 0x0400_0000; // 64 MiB — region 2
const USER_DATA_PHYS: u64 = 0x0400_1000;
const USER_STACK_PHYS: u64 = 0x0400_2000;
const MSG_VA: u64 = 0x1_0001_0100;   // GetInfo message in the data page
const HZ_BUF_VA: u64 = 0x1_0001_0200; // hz result buffer

/// The CPL3 payload (hand-assembled x86-64):
/// ```text
/// mov  ecx, 99                     ; undefined IPC call number
/// int  0x21                        ; → kernel arm → EBADCALL(209) in RAX
/// mov  [0x4001_0000 + 0x00], rax   ; mailbox[0] = errno
/// mov  r11, 0xDEAD
/// mov  [0x4001_0000 + 0x08], r11   ; mailbox[1] = completion marker
/// mov  ecx, 6                      ; MINIX_KERNINFO (page unpublished)
/// int  0x21                        ; → EBADCALL(209) again
/// mov  [0x4001_0000 + 0x18], rax   ; mailbox[2] = errno
/// jmp  $                           ; spin; kernel ticks continue
/// ```
/// Register ABI per the approved E1 design (decision 2): RCX = call
/// number, RAX = errno return. The two mailbox stores after the first
/// trap prove the kernel dispatched, returned through the stub, and the
/// payload kept running in CPL3.
#[unsafe(link_section = ".rodata")]
static USER_PAYLOAD: [u8; 85] = [0xB9, 0x63, 0x00, 0x00, 0x00, 0xCD, 0x21, 0x48, 0xA3, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x48, 0xC7, 0xC0, 0xAD, 0xDE, 0x00, 0x00, 0x48, 0xA3, 0x08, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x48, 0xBF, 0x00, 0x01, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x0F, 0x05, 0x48, 0xA1, 0x00, 0x02, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x48, 0xA3, 0x10, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0xB9, 0x06, 0x00, 0x00, 0x00, 0xCD, 0x21, 0x48, 0xA3, 0x18, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0xEB, 0xFE];

/// User mailbox physical page (data page): [+0x00] round-1 errno,
/// [+0x08] completion marker, [+0x18] round-2 errno. Read by the run
/// script through GDB.
const MAILBOX_MAGIC: u64 = 0xDEAD;
const EBADCALL: u64 = 209;

#[entry]
fn main() -> Status {
    early_console::write_str("### test_user_trap (x86_64): E1 slice 5 — user-mode int-33 trap bridge\n");

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

    // 3. Production Phase B: protection + trap entry (IDT live — the
    // vector-33 gate is installed with DPL=3 by install_idt_handlers).
    minix_kernel::init_protection(&result.kernel_info);
    early_console::write_str("  protection + IDT live (production phase)\n");

    // 4. Clock + interrupt controller (kmain Phase B order).
    minix_kernel::init_clock_and_interrupts();
    early_console::write_str("  clock + controller initialized\n");

    // 5. Process table (Phase C) + SMP state (Phase D).
    early_console::write_str("  calling init_proc_and_boot\n");
    minix_kernel::init_proc_and_boot(&result.kernel_info);
    early_console::write_str("  init_proc_and_boot done\n");
    minix_kernel::init_smp_state();
    early_console::write_str("  proc table + smp state initialized\n");

    // 6. smp_init: APs online (same as test-smp-aps; the user process may
    // be picked by any CPU's scheduler — the restore machinery is
    // per-CPU symmetric). BKL acquired once and held (C main.c:149).
    minix_kernel::smp::bkl_lock().transfer();
    unsafe { core::arch::asm!("sti", options(nomem, nostack)); }
    minix_kernel::smp::smp_init();
    early_console::write_str("  smp_init completed\n");

    // ── E1 slice 5: user process setup ──

    // (a) Map the three user pages by walking the LIVE root through the
    // identity map (every page-table page lives below 4 GiB, so VA = PA is
    // kernel-writable). The X86_64Paging handle is NOT used here: its PTE
    // channel is the kernel Direct Map window, which a bootstrap test
    // kernel has not populated — writing through it page-faults (observed:
    // vector 14, kernel mode, in the first bring-up attempt).
    unsafe {
        early_console::write_str("  map: code\n");
        map_user_page(result.root_page.0, USER_CODE_VA, USER_CODE_PHYS, false, true);
        early_console::write_str("  map: data\n");
        map_user_page(result.root_page.0, USER_DATA_VA, USER_DATA_PHYS, true, false);
        early_console::write_str("  map: stack\n");
        map_user_page(result.root_page.0, USER_STACK_VA, USER_STACK_PHYS, true, false);
    }
    early_console::write_str("  user pages mapped (code/data/stack, CPL3)\n");

    // (b) Zero the data mailbox via the identity map (VA = PA below 4 GiB,
    // kernel-writable), then copy the payload into the code page.
    // SAFETY: the identity map makes the first 4 GiB kernel-writable; the
    // physical pages were chosen inside MEMMAP region 2, past the kernel
    // image and boot allocations.
    unsafe {
        core::ptr::write_bytes(0x400_1000 as *mut u8, 0, 0x1000);
        core::ptr::copy_nonoverlapping(
            core::ptr::addr_of!(USER_PAYLOAD) as *const u8,
            0x400_0000 as *mut u8,
            USER_PAYLOAD.len(),
        );
    }
    early_console::write_str("  payload written (identity map)\n");

    // (c) Build the user context for the VM boot process and make it
    // runnable. C: protect.c USER_CS/init context (build_cpu_context
    // ProcKind::Vm = CPL3 CS + user RFLAGS).
    let ctx = <CurrentCpuContextArch as CpuContextArch>::build_cpu_context(
        ProcKind::Vm,
        minix_kernel::proc::ProcNr(8),
        EntrySpec::loaded(
            VirBytes(USER_CODE_VA),
            VirBytes(USER_STACK_VA),
            VirBytes(0), // ps_strings unused by the payload
        ),
    );
    {
        let table = unsafe { minix_kernel::proc_table_boot_unchecked() };
        let vm = table
            .get_mut(minix_kernel::proc::ProcNr(8))
            .expect("VM boot proc exists");
        vm.set_boot_cpu_context(ctx);
        // C: arch_proc_setcontext marks the context KTS_FULLCONTEXT
        // (protect.c:438) — the restore path requires the recorded style
        // to select the full register-file return sequence.
        vm.trap_style = minix_kernel::PublicTrapStyle::FullContext;
        // Boot procs start PROC_STOP (boot inhibit); clearing it enqueues
        // the process (kmain Phase 4, lib.rs:2053-2059).
        table.rts_unset(
            minix_kernel::proc::ProcNr(8),
            minix_kernel::proc::RtsFlagsBits::PROC_STOP,
        );
    }
    early_console::write_str("  VM boot proc: user context set, runnable\n");

    // ── E8: pre-build the GetInfo GET_HZ message for the syscall leg ──
    // The kernel's kernel_call reads this message from user memory (RDI)
    // and writes the hz value to val_ptr. GET_HZ = 18, GETINFO = 26.
    unsafe {
        let msg_base = (USER_DATA_VA + 0x100) as *mut u64;
        // m_type = SYS_GETINFO(26)
        core::ptr::write_volatile(msg_base, 26);
        // m_lsys_krn_sys_getinfo.request = GET_HZ(18) at arm offset 0 (msg+8)
        core::ptr::write_volatile(msg_base.add(1), 18);
        // val_ptr = data_va + 0x200 at arm offset 8 (msg+16)
        core::ptr::write_volatile(msg_base.add(2), (USER_DATA_VA + 0x200) as u64);
        // val_len = 4 at arm offset 16 (msg+24)
        core::ptr::write_volatile((USER_DATA_VA + 0x100 + 24) as *mut u32, 4);
        // Zero the hz buffer
        core::ptr::write_bytes((USER_DATA_VA + 0x200) as *mut u8, 0, 8);
    }
    early_console::write_str("  GetInfo GET_HZ message pre-built\n");

    // 7. Hand the CPU over — the scheduling loop picks the user process
    // and restores it to CPL3; the payload's `int 0x21` traps into
    // `x86_ipc_dispatch_body` (E1 slice 1 arm).
    early_console::write_str("  entering scheduler (switch_to_user)\n");
    minix_kernel::switch_to_user();
}

/// Walk the live root and install a 4 KiB user mapping (VA → PA).
///
/// The identity map (VA = PA below 4 GiB) makes every page-table page
/// kernel-writable, so the walk touches PTEs directly — no Direct Map
/// window needed in a bootstrap test kernel. New intermediate tables come
/// from the registered `pt_alloc` (zero-filled, identity-addressed).
///
/// # Safety
///
/// `root_phys` must be the CR3-root of the currently active page table,
/// and `pa` a valid, exclusively-owned physical page.
unsafe fn map_user_page(root_phys: u64, va: u64, pa: u64, writable: bool, exec: bool) {
    const PRESENT: u64 = 1;
    const RW: u64 = 2;
    const USER: u64 = 4;
    const NX: u64 = 1 << 63;

    let i4 = ((va >> 39) & 511) as usize;
    let i3 = ((va >> 30) & 511) as usize;
    let i2 = ((va >> 21) & 511) as usize;
    let i1 = ((va >> 12) & 511) as usize;

    let mut table = root_phys;
    for idx in [i4, i3, i2] {
        let entry = (table + idx as u64 * 8) as *mut u64;
        let cur = core::ptr::read_volatile(entry);
        let next = if cur & PRESENT == 0 {
            let (p, v) = pt_alloc::alloc_pt_page().expect("user map: pt page");
            core::ptr::write_bytes(v.0 as *mut u8, 0, 4096);
            core::ptr::write_volatile(entry, p.0 | PRESENT | RW | USER);
            p.0
        } else {
            // The boot identity map created these intermediate entries
            // WITHOUT the user bit — a leaf with U cannot grant access
            // through a supervisor-only intermediate level (U/S is ANDed
            // per level), which is exactly the 0x15 fetch violation seen
            // in the first bring-up run. OR the user bit in.
            core::ptr::write_volatile(entry, cur | USER);
            cur & 0x000f_ffff_ffff_f000
        };
        table = next;
    }
    let leaf = (table + i1 as u64 * 8) as *mut u64;
    let bits = PRESENT | USER | if writable { RW } else { 0 } | if exec { 0 } else { NX };
    core::ptr::write_volatile(leaf, pa | bits);
}

fn fail() -> ! {
    early_console::write_str("### TEST_RESULT: FAIL test-user-trap ###\n");
    loop { unsafe { asm!("hlt", options(nomem, nostack)); } }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    early_console::write_str("### PANIC in test-user-trap: ");
    if let Some(loc) = info.location() {
        early_console::write_str(loc.file());
        early_console::write_str(":");
        early_console::write_hex(loc.line() as u64);
    }
    early_console::write_str(" ###\n");
    loop { unsafe { asm!("hlt", options(nomem, nostack)); } }
}
