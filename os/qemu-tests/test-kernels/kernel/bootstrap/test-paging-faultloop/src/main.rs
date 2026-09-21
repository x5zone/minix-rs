//! Test: E5(d) page-fault complete loop smoke (edge1 K17, C-4).
//!
//! Mechanically replays the VM paging acceptance loop from edge_todo.md
//! (E5(d), 2026-09-08 addition): a process page-faults → the kernel arm
//! resolves the fault → **the handler writes the process's hardware PTE**
//! → resume → the faulting instruction re-executes successfully. The
//! explicit assertion item is the anti-livelock property: without the PTE
//! write the same address re-faults forever, which this carrier turns
//! into a deterministic FAIL (fault counter cap) instead of a hang.
//!
//! # VM participation boundary (C-4)
//!
//! The real loop hands the fault to the VM server over IPC (VM_PAGEFAULT
//! → VM resolves → VM writes the PTE). That leg needs VM 参战 and hangs
//! on edge4 真机联调. VM's paging API (`pagetable`/`vm_self_map`) is
//! `pub(crate)` (only `VmServer`/`BootMemRegion`/`boot` are exported), so
//! a test kernel cannot link it — this carrier keeps every mechanical
//! stage of the loop but plays the VM role in the kernel arm: adopt the
//! boot root as the one self page table, map/query/unmap legs, then the
//! fault loop with the production `page_fault` helpers (RTS_PAGEFAULT
//! set/clear + `build_vm_pagefault_msg`) standing at the kernel↔VM
//! boundary. The produced `Message` is consumed by the carrier's
//! stand-in resolver — the shape that the real VM handler will consume
//! once edge3 exposes a linkable surface (gap registered per C-4).
//!
//! # Boot flow
//!
//! Identical to test-user-trap through the scheduler hand-off (paging →
//! protection → clock → proc table → SMP state → BKL → smp_init), plus
//! one carrier-specific step right after `init_protection`: re-register
//! the trap dispatcher through `register_trap_dispatchers` — vector 14
//! from CPL3 routes to the carrier's fault arm, every other vector
//! forwards to the production `x86_trap_dispatch_body` (clock ticks keep
//! flowing production code).
//!
//! # Stages
//!
//! - **adopt** (init_vm_self_pt analog, A1 adoption): the live boot root
//!   becomes the one self page table — adopted, never re-created.
//! - **map/query/unmap legs**: map SELF_TEST_VA through the adopted
//!   root, query the leaf PTE back (PA + PRESENT match), unmap, query
//!   again (not-present) — the vm_self_mappages/vm_self_query/
//!   vm_self_unmappages mechanical analogs.
//! - **fault loop**: the CPL3 payload reads FAULT_VA (intentionally
//!   unmapped) → #PF (user, not-present, err=4) → carrier arm: RTS
//!   flag set → VM_PAGEFAULT message built → stand-in resolve (frame
//!   from the carrier pool, pattern-filled) → **hardware PTE write** →
//!   invlpg → RTS flag cleared → return. The stub's iretq re-executes
//!   the read; the payload stores the read-back pattern and a 0xBEEF
//!   completion marker into the user mailbox.
//!
//! # PASS determination
//!
//! The run script (qemu-tests/test-paging-faultloop.sh) reads two
//! mailboxes through the gdbstub (payload spins at CPL3 with the live
//! CR3, so user VAs and the kernel identity page are both readable):
//!   user mailbox (VA 0x1_0001_0000, phys 0x400_1000):
//!   [+0x00] == 0x5A5A5A5A5A5A5A5A  fault-loop read-back (right frame)
//!   [+0x08] == 0xBEEF              payload completion marker
//!   kernel box (identity VA 0x400_5000):
//!   [+0x00] == 1                   fault count (== 1: no re-fault —
//!                                  the E5(d) anti-livelock assertion)
//!   [+0x08] == 0xFEED              adopt + map/query/unmap legs OK

#![no_std]
#![no_main]

extern crate alloc;

use core::arch::asm;
use core::panic::PanicInfo;
use core::alloc::{GlobalAlloc, Layout};

use minix_arch::pt_alloc;
use minix_arch::x86_64::paging::X86_64Paging;
use minix_arch::x86_64::trap_stub::TrapFrame;
use minix_arch::{register_trap_dispatchers, CpuContextArch, CurrentCpuContextArch, EntrySpec, ProcKind};
use minix_plat::x86_64::early_console;
use minix_platform;
use minix_kernel::boot_alloc;
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

/// Minimal valid ELF64 image (header only, e_phnum = 0) — Phase C
/// bookkeeping succeeds, VM is never scheduled as a real server.
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

// ── E5(d) carrier address plan ──
// User VAs sit above 4 GiB, outside the boot identity map (VA = PA for
// the first 4 GiB) so map() builds fresh PTEs; physical backing sits in
// MEMMAP region 2 (32–128 MiB), far from the kernel image (2 MiB) and
// boot bump allocations. Same layout rationale as test-user-trap.

const USER_CODE_VA: u64 = 0x1_0000_0000;
const USER_DATA_VA: u64 = 0x1_0001_0000;
const USER_STACK_VA: u64 = 0x1_0002_0000;
const USER_CODE_PHYS: u64 = 0x0400_0000; // 64 MiB — region 2
const USER_DATA_PHYS: u64 = 0x0400_1000;
const USER_STACK_PHYS: u64 = 0x0400_2000;

/// The faulting address: intentionally left unmapped until the carrier
/// arm resolves the #PF by writing the leaf PTE.
const FAULT_VA: u64 = 0x1_0003_0000;
/// Frame the stand-in resolver hands to the faulting address.
const FAULT_FRAME_PHYS: u64 = 0x0400_3000;
/// Byte pattern the resolver fills the frame with — the payload's
/// re-executed read must return exactly this (proves the PTE maps the
/// RIGHT frame, not just any present page).
const FRAME_PATTERN: u64 = 0x5A5A_5A5A_5A5A_5A5A;

/// map/query/unmap stage target (vm_self_mappages/query/unmappages
/// mechanical analog over the adopted root).
const SELF_TEST_VA: u64 = 0x1_0004_0000;
const SELF_TEST_PHYS: u64 = 0x0400_4000;

/// Kernel-side result box (identity VA == PA, supervisor-only): the run
/// script reads it through the gdbstub without symbol resolution.
const KBOX_VA: u64 = 0x0400_5000;
const KBOX_LEGS_OK: u64 = 0xFEED;
// The payload's completion marker (0xBEEF) lives in USER_PAYLOAD's mov
// immediate — the script asserts it from the user mailbox, so no Rust
// constant is needed here.

/// CPL3 payload (hand-assembled x86-64):
/// ```text
/// mov  rax, [FAULT_VA]      ; #PF here (user read of not-present page)
/// mov  [MB+0x00], rax       ; re-executed read: frame pattern lands
/// mov  rax, 0xBEEF          ; completion marker
/// mov  [MB+0x08], rax
/// jmp  $                    ; spin; kernel ticks continue
/// ```
/// The store after the faulting read only executes if the carrier arm
/// mapped the page and the stub resumed — the re-executed `mov rax, [m]`
/// returning the resolver's frame pattern is the loop-closed proof.
#[unsafe(link_section = ".rodata")]
static USER_PAYLOAD: [u8; 39] = {
    let mut p = [0u8; 39];
    let mut i = 0;
    // mov rax, [FAULT_VA] — opcode 48 A1 + moffs64
    p[i] = 0x48; i += 1;
    p[i] = 0xA1; i += 1;
    let mut b = FAULT_VA.to_le_bytes();
    p[i] = b[0]; p[i + 1] = b[1]; p[i + 2] = b[2]; p[i + 3] = b[3];
    p[i + 4] = b[4]; p[i + 5] = b[5]; p[i + 6] = b[6]; p[i + 7] = b[7];
    i += 8;
    // mov [MB+0x00], rax — opcode 48 A3 + moffs64
    p[i] = 0x48; i += 1;
    p[i] = 0xA3; i += 1;
    b = USER_DATA_VA.to_le_bytes();
    p[i] = b[0]; p[i + 1] = b[1]; p[i + 2] = b[2]; p[i + 3] = b[3];
    p[i + 4] = b[4]; p[i + 5] = b[5]; p[i + 6] = b[6]; p[i + 7] = b[7];
    i += 8;
    // mov rax, 0xBEEF — 48 C7 C0 imm32 (sign-extended)
    p[i] = 0x48; p[i + 1] = 0xC7; p[i + 2] = 0xC0;
    p[i + 3] = 0xEF; p[i + 4] = 0xBE; p[i + 5] = 0x00; p[i + 6] = 0x00;
    i += 7;
    // mov [MB+0x08], rax
    p[i] = 0x48; i += 1;
    p[i] = 0xA3; i += 1;
    b = (USER_DATA_VA + 8).to_le_bytes();
    p[i] = b[0]; p[i + 1] = b[1]; p[i + 2] = b[2]; p[i + 3] = b[3];
    p[i + 4] = b[4]; p[i + 5] = b[5]; p[i + 6] = b[6]; p[i + 7] = b[7];
    i += 8;
    // jmp $ — EB FE
    p[i] = 0xEB; p[i + 1] = 0xFE;
    p
};

// ── E5(d) fault arm state ──
// Single-flight: the loop closes (or fails) before any other fault can
// arrive — the handler runs with IF cleared (interrupt gate) and the
// payload is the only CPL3 code in the system.

/// The adopted self page table (live boot root PA). `None` before adopt.
static mut ADOPTED_ROOT: Option<u64> = None;
/// #PF count for FAULT_VA. > 1 means the PTE write failed to stick —
/// the E5(d) anti-livelock assertion is violated.
static mut FAULT_COUNT: u32 = 0;

fn adopted_root() -> u64 {
    unsafe { *core::ptr::addr_of!(ADOPTED_ROOT) }
        .expect("carrier: root not adopted yet")
}

/// Read CR2 — the #PF faulting linear address. Same privileged read as
/// `X86_64ExceptionArch::page_fault_address` (exception.rs:58-74).
fn read_cr2() -> u64 {
    let cr2: u64;
    // SAFETY: CR2 reads are side-effect-free; nomem/nostack match the
    // arch helper's contract.
    unsafe {
        asm!("mov {}, cr2", out(reg) cr2, options(nomem, nostack, preserves_flags));
    }
    cr2
}

/// Walk the adopted root to the leaf PTE for `va`, allocating zeroed
/// intermediate tables from `pt_alloc` when `alloc` is set. Returns the
/// leaf slot address (identity-map writable: every page-table page lives
/// below 4 GiB). Intermediate entries created here carry PRESENT|RW|USER
/// — the boot identity chain is supervisor-only and U/S is ANDed per
/// level, exactly the fix test-user-trap's mapper applies.
fn walk_leaf(root: u64, va: u64, alloc: bool) -> Option<*mut u64> {
    const PRESENT: u64 = 1;
    const RW: u64 = 2;
    const USER: u64 = 4;
    let addr_mask = 0x000f_ffff_ffff_f000;

    let mut table = root;
    for shift in [39u64, 30, 21] {
        let idx = ((va >> shift) & 511) as usize;
        let entry = (table + idx as u64 * 8) as *mut u64;
        let cur = unsafe { core::ptr::read_volatile(entry) };
        if cur & PRESENT == 0 {
            if !alloc {
                return None;
            }
            let (p, v) = pt_alloc::alloc_pt_page().expect("carrier: pt page");
            unsafe { core::ptr::write_bytes(v.0 as *mut u8, 0, 4096); }
            unsafe { core::ptr::write_volatile(entry, p.0 | PRESENT | RW | USER); }
            table = p.0;
        } else {
            // Existing intermediate (boot identity built these without
            // the user bit) — OR the user bit in, same as test-user-trap.
            unsafe { core::ptr::write_volatile(entry, cur | USER); }
            table = cur & addr_mask;
        }
    }
    let idx = ((va >> 12) & 511) as usize;
    Some((table + idx as u64 * 8) as *mut u64)
}

fn invlpg(va: u64) {
    // SAFETY: invlpg invalidates one translation; no memory access.
    unsafe { asm!("invlpg [{}]", in(reg) va, options(nostack, preserves_flags)); }
}

/// E5(d) stage 1-2: adopt the live root and run the map/query/unmap
/// legs over it (init_vm_self_pt / vm_self_mappages / vm_self_query /
/// vm_self_unmappages mechanical analogs). Returns false on any leg
/// mismatch — the run script turns that into FAIL via the kernel box.
fn run_self_pt_legs(root: u64) -> bool {
    // Adopt (exactly once — a second "self" page table would split the
    // address-space identity, the same constraint VmSelfPageTable's
    // no-Clone design enforces).
    unsafe {
        let slot = core::ptr::addr_of_mut!(ADOPTED_ROOT);
        if (*slot).is_some() {
            early_console::write_str("  FAIL: adopt ran twice\n");
            return false;
        }
        *slot = Some(root);
    }
    early_console::write_str("  adopt: live root = ");
    early_console::write_hex(root);
    early_console::write_str("\n");

    // map: SELF_TEST_VA → SELF_TEST_PHYS
    let Some(leaf) = walk_leaf(root, SELF_TEST_VA, true) else {
        early_console::write_str("  FAIL: map leg walk\n");
        return false;
    };
    const PRESENT: u64 = 1;
    const RW: u64 = 2;
    const USER: u64 = 4;
    const NX: u64 = 1 << 63;
    unsafe {
        core::ptr::write_volatile(leaf, SELF_TEST_PHYS | PRESENT | RW | USER | NX);
    }
    invlpg(SELF_TEST_VA);

    // query: the leaf must name SELF_TEST_PHYS, present
    let q1 = unsafe { core::ptr::read_volatile(leaf as *const u64) };
    if q1 & 0x000f_ffff_ffff_f000 != SELF_TEST_PHYS || q1 & PRESENT == 0 {
        early_console::write_str("  FAIL: query leg after map\n");
        return false;
    }
    early_console::write_str("  map + query: PTE names the mapped frame\n");

    // unmap + query: the leaf must read not-present
    unsafe { core::ptr::write_volatile(leaf, 0u64); }
    invlpg(SELF_TEST_VA);
    let q2 = unsafe { core::ptr::read_volatile(leaf as *const u64) };
    if q2 & PRESENT != 0 {
        early_console::write_str("  FAIL: query leg after unmap\n");
        return false;
    }
    early_console::write_str("  unmap + query: PTE not-present\n");
    true
}

/// The carrier's trap-dispatch wrapper — registered right after
/// `init_protection` through the production `register_trap_dispatchers`
/// seam. Vector 14 from CPL3 is the E5(d) kernel arm; everything else
/// (clock ticks, IPIs, other exceptions) forwards to the production
/// body unchanged.
///
/// # Safety
///
/// Same contract as `x86_trap_dispatch_body`: `frame` points at the
/// TrapFrame built by the asm stub on this CPU's kernel stack.
unsafe extern "C" fn faultloop_trap_dispatch(frame: &mut TrapFrame) {
    const PAGE_FAULT_VECTOR: u64 = 14;
    if frame.vector == PAGE_FAULT_VECTOR && frame.cs & 3 == 3 {
        carrier_pagefault(frame);
        // Plain return: the stub's iretq restores the interrupted CPL3
        // state and the faulting instruction re-executes — the E5(d)
        // resume leg. No scheduler round-trip: the flag choreography
        // below is complete before this returns.
        return;
    }
    unsafe { minix_kernel::trap_dispatch::x86_trap_dispatch_body(frame); }
}

/// The E5(d) loop in carrier form: RTS flag → VM_PAGEFAULT message →
/// resolve (frame + **hardware PTE write**) → RTS clear → resume.
fn carrier_pagefault(frame: &mut TrapFrame) {
    let fault_va = read_cr2();
    early_console::write_str("  #PF: va=");
    early_console::write_hex(fault_va);
    early_console::write_str(" err=");
    early_console::write_hex(frame.errcode);
    early_console::write_str("\n");

    if fault_va != FAULT_VA {
        early_console::write_str("  FAIL: fault at unexpected address\n");
        fail();
    }
    // user + read + not-present → err = 0b100. A different code means
    // the payload faulted for another reason (wiring bug, not E5(d)).
    if frame.errcode != 0x4 {
        early_console::write_str("  FAIL: unexpected #PF error code\n");
        fail();
    }

    let count = unsafe {
        let slot = core::ptr::addr_of_mut!(FAULT_COUNT);
        *slot += 1;
        *slot
    };
    if count > 1 {
        // The explicit E5(d) assertion: without a PTE write the same
        // address re-faults forever. One re-entry = the map did not
        // stick = live-lock in the making → deterministic FAIL.
        early_console::write_str("  FAIL: re-fault on the same address — live-lock (E5(d) anti-livelock)\n");
        fail();
    }

    // (a) RTS_PAGEFAULT on the faulting process (C: exception.c:115).
    // (b) the kernel↔VM boundary artifact: the message the real loop
    //     would mini_send to VM; the stand-in resolver consumes it.
    let endpoint = {
        let table = unsafe { minix_kernel::proc_table_boot_unchecked() };
        let vm = table
            .get_mut(minix_kernel::proc::ProcNr(8))
            .expect("VM boot proc exists");
        minix_kernel::page_fault::set_pagefault_pending(vm, fault_va);
        vm.p_endpoint
    };
    let msg = minix_kernel::page_fault::build_vm_pagefault_msg(
        endpoint,
        fault_va,
        frame.errcode as u32,
    );

    // (c) stand-in VM resolve: frame from the carrier pool, patterned.
    // SAFETY: FAULT_FRAME_PHYS is inside MEMMAP region 2, exclusively
    // owned by this carrier; the identity map makes it writable.
    unsafe {
        let fp = FAULT_FRAME_PHYS as *mut u64;
        for w in 0..512 {
            core::ptr::write_volatile(fp.add(w), FRAME_PATTERN);
        }
    }

    // (d) THE G-V12-8 leg: write the process's hardware PTE. The leaf
    // maps FAULT_VA → the resolved frame, user-writable, non-exec.
    let root = adopted_root();
    let leaf = match walk_leaf(root, fault_va, true) {
        Some(l) => l,
        None => {
            early_console::write_str("  FAIL: fault-arm PTE walk\n");
            fail();
        }
    };
    const PRESENT: u64 = 1;
    const RW: u64 = 2;
    const USER: u64 = 4;
    const NX: u64 = 1 << 63;
    unsafe {
        core::ptr::write_volatile(leaf, FAULT_FRAME_PHYS | PRESENT | RW | USER | NX);
    }
    invlpg(fault_va);
    early_console::write_str("  resolve: frame patterned, PTE written, invlpg\n");

    // (e) VM ack analog: clear RTS_PAGEFAULT (C: do_vmctl.c:32-35) —
    // must report was-set, or the state machine was crossed wrongly.
    let was_set = {
        let table = unsafe { minix_kernel::proc_table_boot_unchecked() };
        let vm = table
            .get_mut(minix_kernel::proc::ProcNr(8))
            .expect("VM boot proc exists");
        minix_kernel::page_fault::clear_pagefault_pending(vm)
    };
    if !was_set {
        early_console::write_str("  FAIL: RTS_PAGEFAULT was not set on entry\n");
        fail();
    }

    // (f) publish the loop's observable state: msg type sanity (the
    // consumed message must BE a VM_PAGEFAULT) + kernel box for the
    // script's gdbstub assertions.
    if msg.m_type != minix_types::VM_PAGEFAULT as i32 {
        early_console::write_str("  FAIL: message type is not VM_PAGEFAULT\n");
        fail();
    }
    write_kbox(count as u64, KBOX_LEGS_OK);
    early_console::write_str("  ack: RTS_PAGEFAULT cleared — resuming CPL3\n");
}

fn write_kbox(fault_count: u64, legs: u64) {
    // SAFETY: KBOX_VA is an identity-mapped page below 4 GiB, reserved
    // for this carrier (past kernel image, bump allocs, and the user
    // backing frames).
    unsafe {
        core::ptr::write_volatile(KBOX_VA as *mut u64, fault_count);
        core::ptr::write_volatile((KBOX_VA + 8) as *mut u64, legs);
    }
}

#[entry]
fn main() -> Status {
    early_console::write_str("### test_paging_faultloop (x86_64): E5(d) page-fault complete loop\n");

    // 1. UEFI boot preparation (test-user-trap shape).
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

    // 2. Paging (Phase A).
    boot_alloc::init_boot_pt_alloc(result.bump_base, result.bump_end);
    pt_alloc::register(boot_alloc::boot_pt_alloc);
    let info = minix_kernel::arch_boot_impl::<X86_64Paging>(&result.kernel_info, result.root_page);
    early_console::write_str("  paging enabled\n");

    // Phase A.5: platform discovery FIRST (kmain order).
    unsafe { minix_platform::init_from_kinfo(&result.kernel_info) };

    // 3. Production Phase B: protection + trap entry. Then the carrier
    // seam: re-register the dispatcher so vector-14-from-CPL3 lands in
    // the E5(d) arm while every other vector keeps production routing.
    minix_kernel::init_protection(&result.kernel_info);
    register_trap_dispatchers(faultloop_trap_dispatch, minix_kernel::trap_dispatch::x86_syscall_dispatch_body);
    early_console::write_str("  protection + IDT live; fault-loop dispatcher registered\n");

    // 4. Clock + interrupt controller (kmain Phase B order).
    minix_kernel::init_clock_and_interrupts();
    early_console::write_str("  clock + controller initialized\n");

    // 5. Process table (Phase C) + SMP state (Phase D).
    minix_kernel::init_proc_and_boot(&result.kernel_info);
    minix_kernel::init_smp_state();
    early_console::write_str("  proc table + smp state initialized\n");

    // 6. BKL held (C main.c:149) + APs online (test-user-trap order).
    minix_kernel::smp::bkl_lock().transfer();
    unsafe { core::arch::asm!("sti", options(nomem, nostack)); }
    minix_kernel::smp::smp_init();
    early_console::write_str("  smp_init completed\n");

    // ── E5(d) stages ──

    // Adopt + map/query/unmap legs over the live root.
    if !run_self_pt_legs(result.root_page.0) {
        write_kbox(0, 0);
        fail();
    }

    // Map the three user pages (code exec; data/stack rw; CPL3).
    unsafe {
        early_console::write_str("  map: code\n");
        map_user_page(result.root_page.0, USER_CODE_VA, USER_CODE_PHYS, true, true);
        early_console::write_str("  map: data\n");
        map_user_page(result.root_page.0, USER_DATA_VA, USER_DATA_PHYS, true, false);
        early_console::write_str("  map: stack\n");
        map_user_page(result.root_page.0, USER_STACK_VA, USER_STACK_PHYS, true, false);
    }
    early_console::write_str("  user pages mapped (FAULT_VA intentionally absent)\n");

    // Zero the mailbox + copy the payload (identity map writes).
    // SAFETY: identity map makes the first 4 GiB kernel-writable; the
    // physical pages sit in MEMMAP region 2, exclusively owned here.
    unsafe {
        core::ptr::write_bytes(USER_DATA_PHYS as *mut u8, 0, 0x1000);
        core::ptr::copy_nonoverlapping(
            core::ptr::addr_of!(USER_PAYLOAD) as *const u8,
            USER_CODE_PHYS as *mut u8,
            USER_PAYLOAD.len(),
        );
    }
    early_console::write_str("  payload written (identity map)\n");

    // VM boot process: CPL3 user context, runnable (test-user-trap shape).
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
        // (protect.c:438) — the restore path selects the full register
        // file return sequence by this style record.
        vm.trap_style = minix_kernel::PublicTrapStyle::FullContext;
        table.rts_unset(
            minix_kernel::proc::ProcNr(8),
            minix_kernel::proc::RtsFlagsBits::PROC_STOP,
        );
    }
    early_console::write_str("  VM boot proc: user context set, runnable\n");

    // Hand the CPU over. The payload's first read faults through vector
    // 14 into the carrier arm; the loop must close without a re-fault.
    early_console::write_str("  entering scheduler (switch_to_user)\n");
    minix_kernel::switch_to_user();
}

/// Install a 4 KiB user mapping in the live root (test-user-trap's
/// mapper, unchanged shape: identity-map walk, pt_alloc intermediates,
/// USER bit ORed into existing intermediate entries).
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
    early_console::write_str("### TEST_RESULT: FAIL test-paging-faultloop ###\n");
    loop { unsafe { asm!("hlt", options(nomem, nostack)); } }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    early_console::write_str("### PANIC in test-paging-faultloop: ");
    if let Some(loc) = info.location() {
        early_console::write_str(loc.file());
        early_console::write_str(":");
        early_console::write_hex(loc.line() as u64);
    }
    early_console::write_str(" ###\n");
    loop { unsafe { asm!("hlt", options(nomem, nostack)); } }
}
