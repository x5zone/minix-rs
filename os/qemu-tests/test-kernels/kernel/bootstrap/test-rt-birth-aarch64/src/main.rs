//! Test: first minix-rt freestanding user binary on aarch64 (edge1 K12b —
//! the aarch64 leg of the three-arch CPL3 birth chain; AAVMF carrier).
//!
//! Mirrors the x86 `test-rt-birth` and riscv64 carriers: the kernel loads
//! the embedded `rt-birth` ELF through the production boot path
//! (`init_proc_and_boot`'s VM branch: `load_vm_elf` segments + stack +
//! ps_strings + `build_cpu_context`), publishes the MINIX_KERNINFO page
//! (`init_kerninfo`), and hands the CPU over via `switch_to_user`. The
//! birth chain (aarch64 `_start` → `rt_birth`) parses the descriptor,
//! queries kerninfo through the real `svc` trap, emits its evidence on the
//! kernel console through SYS_DIAGCTL, and finally forces a panic whose
//! render routes through the same diagnostic channel.
//!
//! # aarch64 trap leg (the K12b deliverable)
//!
//! The production aarch64 `VBAR_EL1` vector (K9) is a diagnostic-only stub
//! — no EL0 syscall entry exists in the production kernel yet. This carrier
//! supplies the missing leg itself: after `init_protection` it re-points
//! `VBAR_EL1` at a carrier vector table whose EL0-sync slot handles SVC:
//! save the EL0 register file → Rust dispatch → `eret` (ELR_EL1 already
//! holds the instruction AFTER `svc`, so no PC fix-up) implementing the two
//! calls the birth chain needs:
//!   - `MINIX_KERNINFO` (6): OK + kerninfo page VA in x1,
//!   - KERNEL_CALL leg (0): the message rides at x0 (`m_type` = call);
//!     SYS_DIAGCTL code 1 renders the user buffer on the kernel console.
//! Everything else answers `-ENOSYS`. The register ABI is documented in
//! `minix-sys/src/arch_trap.rs` (x8 = call number, x0/x1 operands and
//! return pair).
//!
//! # Boot flow
//!
//! AAVMF (QEMU virt UEFI firmware) → this .efi → hand-built
//! `BootPrepareResult` → production phases in kmain order. Deviations from
//! the x86 sibling, each bounded: clock init skipped (a no-tick,
//! single-process story), `smp_init` skipped (one core runs everything; SMP
//! parity rides the K11 round). The platform global is installed through
//! `init_from_kinfo` (aarch64 uses ACPI/UEFI handoff, not a raw DTB pointer).
//!
//! PASS = the run script (qemu-tests/test-rt-birth-aarch64.sh) finds the
//! birth chain's serial markers (descriptor parse, kerninfo ready +
//! user_sp, `RT-BIRTH MAIN OK`, panic render check).

#![no_std]
#![no_main]

extern crate alloc;

use core::arch::asm;
use core::panic::PanicInfo;
use core::alloc::{GlobalAlloc, Layout};

use minix_arch::arm64::paging::AArch64Paging;
use minix_arch::pt_alloc;
use minix_types::{PhysBytes, VirBytes};
use minix_plat::arm64::early_console;
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

/// QEMU virt DRAM window, sized to the `-m 256M` the run script passes.
/// A static window (not the UEFI map) is deliberate: the boot-DM coverage
/// and the page-table walk channel must span the LOADER_DATA pool our own
/// page-table pages come from, which `build_memmap()` (CONVENTIONAL only)
/// would leave out — live: walking the root page through the kernel DM
/// took an EL1 translation fault at far = DM + 0x4cb43000.
static MEMMAP: [MemoryRegion; 1] = [MemoryRegion {
    base: PhysBytes(0x4000_0000),
    len: 0x1000_0000,
}];

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



/// The rt-birth user image, embedded at kernel build time (the run script
/// builds `rt-birth` for aarch64-unknown-none first and passes the artifact
/// path in `RT_BIRTH_ELF_PATH`). The kernel identity-maps its own image, so
/// the blob is readable through VA = PA during `load_vm_elf`.
static RT_BIRTH_ELF: &[u8] = include_bytes!(env!("RT_BIRTH_ELF_PATH"));

// ── EL0 SVC vector table (the K12b aarch64 trap leg) ──
//
// VBAR_EL1 must be 2 KiB aligned. Sixteen 128-byte slots; only the
// EL0-sync slot (index 8, offset 0x400) carries real work — the others
// branch to a halt loop (a fault from the wrong level is a carrier bug
// and should stop loudly rather than run on).
//
// Frame layout on the kernel stack (34 × 8 bytes): x0..x30 at indices
// 0..=30, elr at 31, spsr at 32. `sp` is saved/restored around the frame
// (the birth chain's own stack pointer travels in x-— actually the saved
// register file's sp is not in x0-x30; the EL0 sp comes from SP_EL0, which
// this carrier does not need to preserve because the birth chain never
// returns to EL0 from a trap it did not itself cause — it spins in its
// panic handler. The handler therefore only needs x0-x30 + elr + spsr.
/// Kernel stack for the EL0→EL1 vector (32 KiB). The vector loads its top
/// on entry (see the handler comment: SP_EL1 is not installable here).
static mut KSTACK: [u8; 0x8000] = [0u8; 0x8000];

core::arch::global_asm!(
    ".global rt_birth_kstack_top",
    ".set rt_birth_kstack_top, {top} + 0x8000",
    top = sym KSTACK,
);

core::arch::global_asm!(
    ".section .text.vec",
    ".globl rt_birth_vectors",
    ".align 11",              // 2 KiB alignment (VBAR_EL1 contract)
    "rt_birth_vectors:",
    // ── 0x000: Current EL with SP0 (sync) ──
    "b 1f",
    ".align 7",
    // ── 0x080: Current EL with SP0 (irq) ──
    "b 1f",
    ".align 7",
    // ── 0x100: Current EL with SP0 (fiq) ──
    "b 1f",
    ".align 7",
    // ── 0x180: Current EL with SP0 (serror) ──
    "b 1f",
    ".align 7",
    // ── 0x200: Current EL with SPx (sync) — kernel-side fault diag ──
    "b rt_birth_el1_sync",
    ".align 7",
    // ── 0x280: Current EL with SPx (irq) ──
    "b 1f",
    ".align 7",
    // ── 0x300: Current EL with SPx (fiq) ──
    "b 1f",
    ".align 7",
    // ── 0x380: Current EL with SPx (serror) ──
    "b 1f",
    ".align 7",
    // ── 0x400: Lower EL using AArch64 (sync) — the SVC entry ──
    "b rt_birth_el0_sync",
    ".align 7",
    // ── 0x480: Lower EL using AArch64 (irq) ──
    "b 1f",
    ".align 7",
    // ── 0x500: Lower EL using AArch64 (fiq) ──
    "b 1f",
    ".align 7",
    // ── 0x580: Lower EL using AArch64 (serror) ──
    "b 1f",
    ".align 7",
    // ── 0x600: Lower EL using AArch32 (sync) ──
    "b 1f",
    ".align 7",
    // ── 0x680: Lower EL using AArch32 (irq) ──
    "b 1f",
    ".align 7",
    // ── 0x700: Lower EL using AArch32 (fiq) ──
    "b 1f",
    ".align 7",
    // ── 0x780: Lower EL using AArch32 (serror) ──
    "b 1f",
    // 1: wrong-level halt (also the fall-through target of unused slots).
    "1:",
    "wfe",
    "b 1b",
);

// EL1-side fault diagnostic: without it a kernel-mode fault would enter
// the silent halt slot and the carrier would look hung (live-observed:
// the boot died after "kerninfo page published" with no firmware report
// because our own VBAR_EL1 was already installed).
core::arch::global_asm!(
    ".section .text.el1sync",
    ".globl rt_birth_el1_sync",
    ".align 2",
    "rt_birth_el1_sync:",
    "sub sp, sp, #(4*8)",
    "str x0, [sp, #0]",
    "str x1, [sp, #8]",
    "str x2, [sp, #16]",
    "str x30, [sp, #24]",
    "mrs x0, esr_el1",
    "mrs x1, elr_el1",
    "mrs x2, far_el1",
    "bl rt_birth_el1_fault_report",
    "ldr x30, [sp, #24]",
    "add sp, sp, #(4*8)",
    "1:",
    "wfe",
    "b 1b",
);

core::arch::global_asm!(
    ".section .text.el0sync",
    ".globl rt_birth_el0_sync",
    ".align 2",
    "rt_birth_el0_sync:",
    // Establish this handler's kernel stack FIRST: SP_EL1 (the stack the
    // CPU switches to on EL0→EL1 entry) is firmware-owned on AAVMF and
    // was never installable, so the vector loads its own .bss stack top.
    "adrp x9, rt_birth_kstack_top",
    "add x9, x9, :lo12:rt_birth_kstack_top",
    "mov sp, x9",
    "sub sp, sp, #(34*8)",
    "stp x0, x1, [sp, #0]",
    "stp x2, x3, [sp, #16]",
    "stp x4, x5, [sp, #32]",
    "stp x6, x7, [sp, #48]",
    "stp x8, x9, [sp, #64]",
    "stp x10, x11, [sp, #80]",
    "stp x12, x13, [sp, #96]",
    "stp x14, x15, [sp, #112]",
    "stp x16, x17, [sp, #128]",
    "stp x18, x19, [sp, #144]",
    "stp x20, x21, [sp, #160]",
    "stp x22, x23, [sp, #176]",
    "stp x24, x25, [sp, #192]",
    "stp x26, x27, [sp, #208]",
    "stp x28, x29, [sp, #224]",
    "str x30, [sp, #240]",
    "mrs x9, elr_el1",
    "str x9, [sp, #248]",
    "mrs x9, spsr_el1",
    "str x9, [sp, #256]",
    "mov x0, sp",
    "bl rt_birth_svc_dispatch",
    "ldr x9, [sp, #256]",
    "msr spsr_el1, x9",
    "ldr x9, [sp, #248]",
    "msr elr_el1, x9",
    "ldp x0, x1, [sp, #0]",
    "ldp x2, x3, [sp, #16]",
    "ldp x4, x5, [sp, #32]",
    "ldp x6, x7, [sp, #48]",
    "ldp x8, x9, [sp, #64]",
    "ldp x10, x11, [sp, #80]",
    "ldp x12, x13, [sp, #96]",
    "ldp x14, x15, [sp, #112]",
    "ldp x16, x17, [sp, #128]",
    "ldp x18, x19, [sp, #144]",
    "ldp x20, x21, [sp, #160]",
    "ldp x22, x23, [sp, #176]",
    "ldp x24, x25, [sp, #192]",
    "ldp x26, x27, [sp, #208]",
    "ldp x28, x29, [sp, #224]",
    "ldr x30, [sp, #240]",
    "add sp, sp, #(34*8)",
    "eret",
);

/// Frame indices for the registers the dispatch reads/writes.
const IDX_X0: usize = 0;
const IDX_X1: usize = 1;
const IDX_X8: usize = 8;
const IDX_ELR: usize = 31;
const IDX_SPSR: usize = 32;

/// MINIX_KERNINFO call number (C ipcconst.h:12).
const KERNINFO_NR: usize = 6;
/// KERNEL_CALL message-leg trap number.
const KERNEL_CALL_TRAP_NR: usize = 0;
/// -ENOSYS.
const ENOSYS: u64 = 38;
/// DIAGCTL code 1 = DIAG console output.
const DIAGCTL_CODE_DIAG: u64 = 1;
/// DIAGBUFSIZE cap.
const DIAGBUF_MAX: usize = 128;
/// ESR_EL1 EC field shift/mask (exception class at bits [31:26]).
const ESR_EC_SHIFT: u64 = 26;
const ESR_EC_MASK: u64 = 0x3F;
/// EC 0x15 = SVC instruction execution in AArch64 state.
const EC_SVC_AARCH64: u64 = 0x15;

static mut KMSG: [u8; 64] = [0u8; 64];
static mut DIAGBUF: [u8; DIAGBUF_MAX] = [0u8; DIAGBUF_MAX];

/// The svc dispatch — called from `rt_birth_el0_sync` with the frame base.
///
/// # Safety
///
/// `frame` points at the 34-slot frame the asm handler built on this CPU's
/// kernel stack; single-core carrier, no concurrent handlers.
#[unsafe(no_mangle)]
unsafe extern "C" fn rt_birth_svc_dispatch(frame: *mut u64) {
    let esr: u64;
    unsafe {
        asm!("mrs {}, esr_el1", out(reg) esr, options(nomem, nostack));
    }
    let ec = (esr >> ESR_EC_SHIFT) & ESR_EC_MASK;
    if ec != EC_SVC_AARCH64 {
        let far: u64;
        unsafe {
            asm!("mrs {}, far_el1", out(reg) far, options(nomem, nostack));
        }
        early_console::write_str("  [svc] fatal: not an SVC ec=");
        early_console::write_hex(ec);
        early_console::write_str(" esr=");
        early_console::write_hex(esr);
        early_console::write_str(" elr=");
        early_console::write_hex(*frame.add(IDX_ELR));
        early_console::write_str(" far=");
        early_console::write_hex(far);
        early_console::write_str(" spsr=");
        early_console::write_hex(*frame.add(IDX_SPSR));
        early_console::write_str("\n");
        {
            let sctlr: u64;
            let ttbr0: u64;
            unsafe {
                asm!("mrs {}, sctlr_el1", out(reg) sctlr, options(nomem, nostack));
                asm!("mrs {}, ttbr0_el1", out(reg) ttbr0, options(nomem, nostack));
            }
            early_console::write_str("  [svc] sctlr=");
            early_console::write_hex(sctlr);
            early_console::write_str(" ttbr0=");
            early_console::write_hex(ttbr0);
            early_console::write_str("\n");
        }
        loop {
            unsafe { asm!("wfe", options(nomem, nostack)); }
        }
    }

    // `svc` sets ELR_EL1 to the NEXT instruction (AArch64 exception
    // semantics), so no PC fix-up is needed — the eret resumes past it.
    let call_nr = *frame.add(IDX_X8) as usize;
    match call_nr {
        KERNINFO_NR => {
            *frame.add(IDX_X0) = 0;
            *frame.add(IDX_X1) = minix_kernel::kerninfo::KERNINFO_USER_VA;
        }
        KERNEL_CALL_TRAP_NR => {
            // User memory (the message struct and the DIAGCTL buffer) is
            // accessible from EL1 with the PAN-safe read/write path: set
            // SPSR/CurrentEL PAN clear for the window via PSTATE.PAN.
            // PAN (Privileged Access Never) blocks EL1 access to EL0 pages;
            // the birth chain's message and DIAGCTL buffer live in user
            // memory, so clear it for the window and restore after. PAN is
            // SPSR_EL1 bit 22 (PSTATE.PAN); the mnemonic `msr pan, #imm`
            // is not accepted by the integrated assembler, so the bit is
            // flipped through a register.
            unsafe {
                asm!(
                    "mrs {t}, spsr_el1",
                    "bic {t}, {t}, #(1 << 22)",
                    "msr spsr_el1, {t}",
                    t = out(reg) _,
                    options(nomem, nostack),
                );
            }
            let ret = kernel_call_leg(*frame.add(IDX_X0), &raw mut KMSG, &raw mut DIAGBUF);
            unsafe {
                asm!(
                    "mrs {t}, spsr_el1",
                    "orr {t}, {t}, #(1 << 22)",
                    "msr spsr_el1, {t}",
                    t = out(reg) _,
                    options(nomem, nostack),
                );
            }
            *frame.add(IDX_X0) = ret;
        }
        _ => {
            *frame.add(IDX_X0) = ENOSYS;
            *frame.add(IDX_X1) = 0;
        }
    }
}

unsafe fn kernel_call_leg(msg_user: u64, kmsg: *mut [u8; 64], diagbuf: *mut [u8; DIAGBUF_MAX]) -> u64 {
    if msg_user == 0 {
        return minix_types::EINVAL as u64;
    }
    unsafe {
        core::ptr::copy_nonoverlapping(msg_user as *const u8, kmsg.cast(), 64);
    }
    let m_type = i32::from_le_bytes([(*kmsg)[4], (*kmsg)[5], (*kmsg)[6], (*kmsg)[7]]);
    if m_type != minix_types::SYS_DIAGCTL {
        return ENOSYS;
    }
    let code = u32::from_le_bytes([(*kmsg)[8], (*kmsg)[9], (*kmsg)[10], (*kmsg)[11]]) as u64;
    if code != DIAGCTL_CODE_DIAG {
        return ENOSYS;
    }
    let buf = u64::from_le_bytes([
        (*kmsg)[16], (*kmsg)[17], (*kmsg)[18], (*kmsg)[19],
        (*kmsg)[20], (*kmsg)[21], (*kmsg)[22], (*kmsg)[23],
    ]);
    let len = u64::from_le_bytes([
        (*kmsg)[24], (*kmsg)[25], (*kmsg)[26], (*kmsg)[27],
        (*kmsg)[28], (*kmsg)[29], (*kmsg)[30], (*kmsg)[31],
    ]) as usize;
    let n = len.min(DIAGBUF_MAX);
    if buf == 0 || n == 0 {
        return 0;
    }
    unsafe {
        core::ptr::copy_nonoverlapping(buf as *const u8, diagbuf as *mut u8, n);
    }
    let bytes: &[u8] = &*diagbuf;
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(n);
    match core::str::from_utf8(&bytes[..end]) {
        Ok(s) => early_console::write_str(s),
        Err(_) => early_console::write_str("(non-utf8 diag)\n"),
    }
    0
}

/// Kernel-mode (EL1) fault reporter — see the `rt_birth_el1_sync` stub.
#[unsafe(no_mangle)]
extern "C" fn rt_birth_el1_fault_report(esr: u64, elr: u64, far: u64) {
    early_console::write_str("  [el1] fault esr=");
    early_console::write_hex(esr);
    early_console::write_str(" elr=");
    early_console::write_hex(elr);
    early_console::write_str(" far=");
    early_console::write_hex(far);
    early_console::write_str("\n");
}

/// The carrier vector-table symbol (asm-defined).
unsafe extern "C" {
    static rt_birth_vectors: u8;
}

#[entry]
fn main() -> Status {
    early_console::write_str("### test_rt_birth (aarch64): first minix-rt user binary on AAVMF\n");

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

    let memmap: &'static [MemoryRegion] = &MEMMAP;

    // Kernel image range, computed from the live image address: UEFI
    // relocates the .efi anywhere in DRAM, and both the boot-DM exclusion
    // and the VM bootstrap region selector key off `kern_phys_base`. The
    // hello-boot-aarch64 constant (0x4020_0000) was wrong for this image
    // and left the running image eligible for bootstrap allocations.
    let image_base = (main as usize as u64) & !0x001f_ffff; // 2 MiB aligned

    let kernel_info = KernelInfo {
        memmap,
        kern_virt_base: VirBytes(0xFFFF_8000_0000_0000),
        kern_phys_base: PhysBytes(image_base),
        kern_size: 0x200_000,
        free_upper_idx: Some(280),
        user_sp: VirBytes(0x0000_7fff_ffff_f000),
        // Kernel-stack TOP for SP_EL1. The carrier does not install
        // SP_EL1 (AAVMF rejects the write — see the protection-phase
        // note); its vector loads `rt_birth_kstack_top` itself. The value
        // here is the carrier's own stack top so the record matches what
        // the trap leg actually uses.
        kern_stack_top: VirBytes(0x0000_0000_4781_0000),
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
    let _info = minix_kernel::arch_boot_impl::<AArch64Paging>(&result.kernel_info, result.root_page);
    early_console::write_str("  paging enabled\n");

    // Phase A.5: platform discovery FIRST (kmain order).
    unsafe { minix_platform::init_from_kinfo(&result.kernel_info) };
    early_console::write_str("  platform desc installed\n");

    // 3. Protection + trap entry.
    //
    // DEVIATION from the x86/riscv64 siblings (platform constraint, live
    // probed): the earlier production `init_protection` drove
    // `AArch64Protection::init`, whose body then was a single `msr SP_EL1`
    // write. On AAVMF that instruction takes a Synchronous Exception from
    // the firmware's EL1 context — the UEFI firmware owns SP_EL1 (it runs
    // with SPSel=1 on its own EL1 stack) and the write is rejected; the
    // probe bracketing pinned the fault to exactly that instruction
    // (fault PC == the `msr`, no output between the two markers). This is
    // the same root cause fixed in the production tree (NK4-C §1.109):
    // `AArch64Protection::init` is now a no-op under the EL1h model, since
    // `msr SP_EL1` is architecturally UNDEFINED when SPSel=1.
    //
    // The carrier therefore skips the production call and performs the
    // one thing the trap leg actually needs: install its own vector
    // table. The kernel stack for EL0→EL1 entry is established INSIDE
    // the vector (the handler loads it on entry), so no SP_EL1 write is
    // required at all. The production SP_EL1 hand-over belongs to the
    // real `jump_to_kmain` path, not to this bootstrap carrier.
    early_console::write_str("  protection: skipped (SP_EL1 is firmware-owned on AAVMF)\n");
    // The carrier's EL0-sync vector takes over VBAR_EL1 (the K9 diagnostic
    // table keeps no state).
    // SAFETY: the vector table is 2-KiB aligned (asm .align 11); VBAR_EL1
    // takes the base address with the low 11 bits ignored.
    unsafe {
        let vbar = &rt_birth_vectors as *const u8 as u64;
        asm!("msr vbar_el1, {}", in(reg) vbar, options(nomem, nostack));
    }
    early_console::write_str("  protection live; carrier vectors on VBAR_EL1\n");

    // 4. Kernel information page (Phase B.5).
    minix_kernel::init_kerninfo(&result.kernel_info);
    early_console::write_str("  kerninfo page published\n");

    // 5. Process table (Phase C) — the VM branch loads the embedded
    //    rt-birth ELF.
    minix_kernel::init_proc_and_boot(&result.kernel_info);
    minix_kernel::init_smp_state();
    early_console::write_str("  proc table + smp state initialized\n");

    // 6. BKL held; make the VM boot process (rt-birth) runnable.
    minix_kernel::smp::bkl_lock().transfer();
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

    // 7. Hand the CPU over — eret into the ELF entry at 5 GiB, EL0.
    early_console::write_str("  entering scheduler (switch_to_user)\n");
    minix_kernel::switch_to_user();
}

#[allow(dead_code)]
fn fail() -> ! {
    early_console::write_str("### TEST_RESULT: FAIL test-rt-birth-aarch64 ###\n");
    loop { unsafe { asm!("wfe", options(nomem, nostack)); } }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    early_console::write_str("### PANIC in test-rt-birth-aarch64: ");
    if let Some(loc) = info.location() {
        early_console::write_str(loc.file());
        early_console::write_str(":");
        early_console::write_hex(loc.line() as u64);
    }
    early_console::write_str(" ");
    struct Console;
    impl core::fmt::Write for Console {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            early_console::write_str(s);
            Ok(())
        }
    }
    let _ = core::fmt::write(&mut Console, format_args!("{}\n", info.message()));
    early_console::write_str(" ###\n");
    loop { unsafe { asm!("wfe", options(nomem, nostack)); } }
}
