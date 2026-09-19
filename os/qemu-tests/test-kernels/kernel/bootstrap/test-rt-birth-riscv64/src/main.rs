//! Test: first minix-rt freestanding user binary on riscv64 (edge1 K12b —
//! the riscv64 leg of the three-arch CPL3 birth chain; OpenSBI carrier).
//!
//! Mirrors the x86 `test-rt-birth` carrier: the kernel loads the embedded
//! `rt-birth` ELF through the production boot path (`init_proc_and_boot`'s
//! VM branch: `load_vm_elf` segments + stack + ps_strings +
//! `build_cpu_context`), publishes the MINIX_KERNINFO page
//! (`init_kerninfo`), and hands the CPU over via `switch_to_user`. The
//! birth chain (riscv64 `_start` → `rt_birth`) parses the descriptor,
//! queries kerninfo through the REAL ecall trap, emits its evidence on the
//! kernel console through SYS_DIAGCTL, and finally forces a panic whose
//! render routes through the same diagnostic channel.
//!
//! # riscv64 trap leg (the K12b deliverable)
//!
//! The production riscv64 `stvec` entry (K9) is a Direct-mode diagnostic
//! stub — no U-mode syscall entry exists in the production kernel yet.
//! This carrier supplies the missing leg itself: after `init_protection`
//! it re-points `stvec` at a U-ecall handler (`csrrw sp, sscratch, sp`
//! kernel-stack switch → frame save → Rust dispatch → `sepc += 4` →
//! sret) implementing the two calls the birth chain needs:
//!   - `MINIX_KERNINFO` (6): OK + kerninfo page VA in a1,
//!   - KERNEL_CALL leg (0): the message rides at a0 (`m_type` = call);
//!     SYS_DIAGCTL code 1 renders the user buffer on the kernel console.
//! Everything else answers `-ENOSYS`. The register ABI is documented in
//! `minix-sys/src/arch_trap.rs` (a7 = call number, a0/a1 operands and
//! return pair).
//!
//! # Boot flow
//!
//! OpenSBI (-bios default) → `-kernel` at 0x80200000 → BSS zero + stack
//! setup (K10 carrier shape) → hand-built `BootPrepareResult` (no
//! U-Boot/BootFileTable in this scenario — hello-boot-riscv64 shape) →
//! production phases in kmain order. Deviations from the x86 sibling,
//! each bounded: clock init skipped (the birth chain is a no-tick,
//! single-process story), `smp_init` skipped (no AP bring-up — one hart
//! runs everything; AP parity rides the K11 round).
//!
//! PASS = the run script (qemu-tests/test-rt-birth-riscv64.sh) finds the
//! birth chain's serial markers (descriptor parse, kerninfo ready +
//! user_sp, `RT-BIRTH MAIN OK`, panic render check) — every marker is
//! user CPL3 output that crossed the ecall boundary twice (DIAGCTL out).

#![no_std]
#![no_main]

extern crate alloc;

use core::arch::asm;
use core::panic::PanicInfo;
use core::alloc::{GlobalAlloc, Layout};

use minix_arch::riscv64::paging::Riscv64Paging;
use minix_arch::pt_alloc;
use minix_types::{PhysBytes, VirBytes};
use minix_plat::riscv64::early_console;
use minix_kernel::boot_alloc;
use minix_boot::{BootPrepareResult, KernelInfo, BootModule, MemoryRegion};

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

static MEMMAP: [MemoryRegion; 1] = [MemoryRegion {
    base: PhysBytes(DRAM_BASE),
    len: 0x1000_0000, // 256 MB — keep QEMU `-m` in sync
}];

/// The rt-birth user image, embedded at kernel build time (the run script
/// builds `rt-birth` for riscv64gc-unknown-none-elf first and passes the
/// artifact path in `RT_BIRTH_ELF_PATH`). The kernel identity-maps its own
/// image, so the blob is readable through VA = PA during `load_vm_elf`.
static RT_BIRTH_ELF: &[u8] = include_bytes!(env!("RT_BIRTH_ELF_PATH"));

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

// ── U-mode ecall handler (the K12b riscv64 trap leg) ──
//
// Frame layout on the kernel stack (34 × 8 bytes): x1..x31 at indices
// 1..=31 (x0 is hardwired zero and has no slot), sepc at 32, sstatus at
// 33. The sscratch swap parks the user sp while the handler runs on the
// kernel stack top (programmed by init_protection's riscv64 leg).
core::arch::global_asm!(
    ".section .text.uecall",
    ".globl uecall_handler",
    ".align 2",
    "uecall_handler:",
    "    csrrw sp, sscratch, sp",
    "    addi sp, sp, -34*8",
    "    sd ra, 1*8(sp)",
    "    sd gp, 3*8(sp)",
    "    sd tp, 4*8(sp)",
    "    sd t0, 5*8(sp)",
    "    sd t1, 6*8(sp)",
    "    sd t2, 7*8(sp)",
    "    sd s0, 8*8(sp)",
    "    sd s1, 9*8(sp)",
    "    sd a0, 10*8(sp)",
    "    sd a1, 11*8(sp)",
    "    sd a2, 12*8(sp)",
    "    sd a3, 13*8(sp)",
    "    sd a4, 14*8(sp)",
    "    sd a5, 15*8(sp)",
    "    sd a6, 16*8(sp)",
    "    sd a7, 17*8(sp)",
    "    sd s2, 18*8(sp)",
    "    sd s3, 19*8(sp)",
    "    sd s4, 20*8(sp)",
    "    sd s5, 21*8(sp)",
    "    sd s6, 22*8(sp)",
    "    sd s7, 23*8(sp)",
    "    sd s8, 24*8(sp)",
    "    sd s9, 25*8(sp)",
    "    sd s10, 26*8(sp)",
    "    sd s11, 27*8(sp)",
    "    sd t3, 28*8(sp)",
    "    sd t4, 29*8(sp)",
    "    sd t5, 30*8(sp)",
    "    sd t6, 31*8(sp)",
    "    csrr t0, sepc",
    "    sd t0, 32*8(sp)",
    "    csrr t0, sstatus",
    "    sd t0, 33*8(sp)",
    "    mv a0, sp",
    "    call uecall_dispatch",
    // Frame-restore mirror (sepc/sstatus first — they are CSRs, then the
    // GPRs; sp comes from sscratch after the swap below).
    "    ld t0, 33*8(sp)",
    "    csrw sstatus, t0",
    "    ld t0, 32*8(sp)",
    "    csrw sepc, t0",
    "    ld ra, 1*8(sp)",
    "    ld gp, 3*8(sp)",
    "    ld tp, 4*8(sp)",
    "    ld t0, 5*8(sp)",
    "    ld t1, 6*8(sp)",
    "    ld t2, 7*8(sp)",
    "    ld s0, 8*8(sp)",
    "    ld s1, 9*8(sp)",
    "    ld a0, 10*8(sp)",
    "    ld a1, 11*8(sp)",
    "    ld a2, 12*8(sp)",
    "    ld a3, 13*8(sp)",
    "    ld a4, 14*8(sp)",
    "    ld a5, 15*8(sp)",
    "    ld a6, 16*8(sp)",
    "    ld a7, 17*8(sp)",
    "    ld s2, 18*8(sp)",
    "    ld s3, 19*8(sp)",
    "    ld s4, 20*8(sp)",
    "    ld s5, 21*8(sp)",
    "    ld s6, 22*8(sp)",
    "    ld s7, 23*8(sp)",
    "    ld s8, 24*8(sp)",
    "    ld s9, 25*8(sp)",
    "    ld s10, 26*8(sp)",
    "    ld s11, 27*8(sp)",
    "    ld t3, 28*8(sp)",
    "    ld t4, 29*8(sp)",
    "    ld t5, 30*8(sp)",
    "    ld t6, 31*8(sp)",
    "    addi sp, sp, 34*8",
    "    csrrw sp, sscratch, sp",
    "    sret",
);

/// sstatus.SUM (bit 18): S-mode may access user-mode pages while set.
const SSTATUS_SUM: u64 = 1 << 18;
/// sstatus read mask for the write-back below (SUM belongs to the
/// S-mode-writable field set; the other bits are restored as read).
const SSTATUS_SUM_MASK: u64 = !SSTATUS_SUM;
/// MINIX_KERNINFO call number (C ipcconst.h:12; mirrors
/// `minix_sys::arch_trap::KERNINFO_NR`).
const KERNINFO_NR: usize = 6;
/// KERNEL_CALL message-leg trap number (mirrors
/// `minix_sys::arch_trap::KERNEL_CALL_TRAP_NR`).
const KERNEL_CALL_TRAP_NR: usize = 0;
/// -ENOSYS: the handler answers unimplemented calls with it.
const ENOSYS: u64 = 38;
/// DIAGCTL code 1 = DIAG console output (kernel `dispatch_diagctl` code 1).
const DIAGCTL_CODE_DIAG: u64 = 1;
/// DIAGBUFSIZE cap — mirrors the production dispatch_diagctl buffer bound.
const DIAGBUF_MAX: usize = 128;

/// Frame indices for the registers the dispatch reads/writes.
const IDX_A0: usize = 10;
const IDX_A1: usize = 11;
const IDX_A7: usize = 17;
const IDX_SEPC: usize = 32;
const IDX_SSTATUS: usize = 33;

/// Kernel-side copy of the incoming user message (64 bytes: m_source i32,
/// m_type i32, 56-byte payload union) and the diag buffer.
static mut KMSG: [u8; 64] = [0u8; 64];
static mut DIAGBUF: [u8; DIAGBUF_MAX] = [0u8; DIAGBUF_MAX];

/// The ecall dispatch — called from `uecall_handler` with the frame base.
///
/// # Safety
///
/// `frame` points at the 34-slot frame the asm handler built on this
/// CPU's kernel stack; single-hart carrier, no concurrent handlers.
#[unsafe(no_mangle)]
unsafe extern "C" fn uecall_dispatch(frame: *mut u64) {
    let scause: u64;
    unsafe {
        asm!("csrr {}, scause", out(reg) scause, options(nomem, nostack));
    }
    // Only a U-mode ecall (cause 8, no interrupt bit) is serviced here.
    // Anything else — a fault — must NOT advance sepc (that would skip a
    // faulting instruction and corrupt the stream): report and halt so
    // the serial log carries the diagnosis.
    const CAUSE_ECALL_UMODE: u64 = 8;
    if scause != CAUSE_ECALL_UMODE {
        let stval: u64;
        unsafe {
            asm!("csrr {}, stval", out(reg) stval, options(nomem, nostack));
        }
        early_console::write_str("  [uecall] fatal: not a U-ecall scause=");
        early_console::write_hex(scause);
        early_console::write_str(" sepc=");
        early_console::write_hex(*frame.add(IDX_SEPC));
        early_console::write_str(" stval=");
        early_console::write_hex(stval);
        early_console::write_str("\n");
        loop {
            unsafe { asm!("wfi", options(nomem, nostack)); }
        }
    }
    let call_nr = *frame.add(IDX_A7) as usize;

    // `ecall` does not advance sepc — step past the 4-byte instruction or
    // the sret re-executes it and the trap loops forever.
    *frame.add(IDX_SEPC) = (*frame.add(IDX_SEPC)).wrapping_add(4);

    // Read sstatus, clear SUM for the write-back mirror (the asm restores
    // the full CSR; this keeps a SUM set only for the user-memory window).
    let sstatus = *frame.add(IDX_SSTATUS);

    match call_nr {
        // MINIX_KERNINFO: OK + the published user page VA in a1.
        KERNINFO_NR => {
            *frame.add(IDX_A0) = 0;
            *frame.add(IDX_A1) = minix_kernel::kerninfo::KERNINFO_USER_VA;
        }
        // KERNEL_CALL message leg: the message rides at a0.
        KERNEL_CALL_TRAP_NR => {
            // Enable SUM for the user-memory window (the message struct
            // and the DIAGCTL buffer live in the user image/stack).
            unsafe {
                asm!("csrs sstatus, {}", in(reg) SSTATUS_SUM, options(nomem, nostack));
            }
            let ret = kernel_call_leg(*frame.add(IDX_A0), &raw mut KMSG, &raw mut DIAGBUF);
            // Clear SUM again before the frame write-back restores sstatus.
            unsafe {
                asm!("csrc sstatus, {}", in(reg) SSTATUS_SUM, options(nomem, nostack));
            }
            *frame.add(IDX_A0) = ret;
            // sstatus restore keeps the handler's clear-on-exit honest.
            *frame.add(IDX_SSTATUS) = sstatus & SSTATUS_SUM_MASK;
        }
        _ => {
            // Not a U-mode ecall (page-fault / abort from U-mode, or an
            // S-mode trap): read the live scause and print it with the
            // faulting PC so a re-executing fault is visible.
            let scause: u64;
            let stval: u64;
            unsafe {
                asm!("csrr {}, scause", out(reg) scause, options(nomem, nostack));
                asm!("csrr {}, stval", out(reg) stval, options(nomem, nostack));
            }
            early_console::write_str("  [uecall] unexpected trap scause=");
            early_console::write_hex(scause);
            early_console::write_str(" sepc=");
            early_console::write_hex(*frame.add(IDX_SEPC));
            early_console::write_str(" stval=");
            early_console::write_hex(stval);
            early_console::write_str(" sstatus=");
            early_console::write_hex(*frame.add(IDX_SSTATUS));
            early_console::write_str("\n");
            *frame.add(IDX_A0) = ENOSYS;
            *frame.add(IDX_A1) = 0;
        }
    }
}

/// The KERNEL_CALL leg: copy the 64-byte message from user memory, serve
/// SYS_DIAGCTL (code 1 renders the user buffer on the kernel console),
/// return the reply code.
unsafe fn kernel_call_leg(msg_user: u64, kmsg: *mut [u8; 64], diagbuf: *mut [u8; DIAGBUF_MAX]) -> u64 {
    if msg_user == 0 {
        return minix_types::EINVAL as u64;
    }
    // SAFETY: SUM is set (caller) — the message lives in the user image
    // page range, mapped user-rw in the bootstrap root; single hart.
    unsafe {
        core::ptr::copy_nonoverlapping(msg_user as *const u8, kmsg.cast(), 64);
    }
    let m_type = i32::from_le_bytes([
        (*kmsg)[4], (*kmsg)[5], (*kmsg)[6], (*kmsg)[7],
    ]);
    if m_type != minix_types::SYS_DIAGCTL {
        return ENOSYS;
    }
    let code = u32::from_le_bytes([
        (*kmsg)[8], (*kmsg)[9], (*kmsg)[10], (*kmsg)[11],
    ]) as u64;
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
    // SAFETY: SUM is set; the buffer is a user buffer passed by the birth
    // chain (the caller's own memory).
    unsafe {
        core::ptr::copy_nonoverlapping(buf as *const u8, (diagbuf as *mut u8), n);
    }
    // The birth chain emits NUL-free text lines; render lossily (an
    // interior NUL just truncates the line — diagnostics, not data).
    let bytes: &[u8] = &*diagbuf;
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(n);
    match core::str::from_utf8(&bytes[..end]) {
        Ok(s) => early_console::write_str(s),
        Err(_) => early_console::write_str("(non-utf8 diag)\n"),
    }
    0
}

/// The U-ecall handler symbol (asm-defined in this module).
unsafe extern "C" {
    fn uecall_handler();
}

/// Rust entry — single-hart carrier (a0 boot hart id, a1 DTB, per OpenSBI;
/// both unused: the boot info is hand-built for this fixed platform).
#[unsafe(no_mangle)]
extern "C" fn rust_main(_boot_hart: u64, dtb_phys: u64) -> ! {
    early_console::write_str("### test_rt_birth (riscv64): first minix-rt user binary on OpenSBI\n");

    // 1. Hand-built BootPrepareResult (hello-boot-riscv64 shape — no
    //    U-Boot/BootFileTable in the QEMU -kernel scenario).
    let root_page = PhysBytes(bump(1).expect("root page"));
    // 256 pages (1 MB): the identity map's 2-MiB fallback granularity
    // (kern_phys_base is not 1-GiB aligned on this lane) spends a handful
    // of tables, and the ELF/stack/kerninfo maps spend a few more — the
    // 8-page hello-boot budget exhausted mid-load (MappingFailed).
    let bump_base = bump(256).expect("bump region");
    let bump_end = bump_base + 256 * 4096;

    // SAFETY: single-threaded boot; BOOT_MODULES is read-only afterwards.
    let boot_modules: &'static [BootModule] = unsafe {
        let mods = &mut *core::ptr::addr_of_mut!(BOOT_MODULES);
        mods[8] = BootModule {
            name: "vm",
            // The slice's data pointer — addr_of! on the static itself
            // would take the address of the (ptr, len) fat pair.
            start: PhysBytes(RT_BIRTH_ELF.as_ptr() as usize as u64),
            len: RT_BIRTH_ELF.len(),
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
        // Kernel-stack TOP for the sscratch swap on U→S entry: the image
        // end (+2 MiB), NOT the image base — the first value put the
        // handler's frame stores right on top of .text at 0x80200000
        // (live: store faults walking the handler's own instructions).
        kern_stack_top: VirBytes(KERNEL_LOAD_PA + 0x200_000),
        syscall_entry: VirBytes(DRAM_BASE),
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

    // 2. Paging (Phase A): identity + Sv39 enable (production path).
    boot_alloc::init_boot_pt_alloc(result.bump_base, result.bump_end);
    pt_alloc::register(boot_alloc::boot_pt_alloc);
    let _info = minix_kernel::arch_boot_impl::<Riscv64Paging>(&result.kernel_info, result.root_page);
    early_console::write_str("  paging enabled\n");

    // Phase A.5: platform discovery — the DTB pointer OpenSBI passed in
    // a1 feeds the same parse the kernel lane uses (K10 carrier shape).
    // Later phases (scheduler anchors, timer gates) read the frozen
    // global; skipping this freeze is the Frozen::get-before-freeze panic.
    {
        use minix_boot::PlatformDescSource;
        use minix_platform::kind::{parse_by_kind, DTB};
        if dtb_phys == 0 {
            panic!("OpenSBI did not pass a DTB pointer in a1");
        }
        let source = PlatformDescSource::new(DTB, PhysBytes(dtb_phys));
        let desc = unsafe { parse_by_kind(source) }
            .unwrap_or_else(|e| panic!("DTB parse failed: {e:?}"));
        unsafe { minix_platform::init(desc) };
        early_console::write_str("  platform desc installed (DTB)\n");
    }

    // 3. Protection + trap entry (production Phase B — stvec = the K9
    //    diagnostic vector). The carrier's U-ecall handler takes over
    //    stvec right after; the diagnostic vector keeps no state.
    minix_kernel::init_protection(&result.kernel_info);
    // SAFETY: the stvec write points at the asm handler defined in this
    // module's `global_asm!`; the frame protocol is that handler's.
    unsafe {
        let handler = uecall_handler as *const () as usize;
        asm!(
            "csrw stvec, {h}",
            h = in(reg) handler,
            options(nomem, nostack)
        );
    }
    // Program sscratch for the U→S stack swap: the production
    // Riscv64Protection::init writes it, but this carrier owns the
    // handler and its stack convention, so it (re)writes the CSR
    // explicitly — kernel-stack top = image end + 2 MiB (identity-mapped
    // DRAM, far above the loaded image).
    {
        let kstack: u64 = KERNEL_LOAD_PA + 0x200_000;
        unsafe { core::arch::asm!("csrw sscratch, {}", in(reg) kstack, options(nomem, nostack)); }
        let ss: u64;
        unsafe { core::arch::asm!("csrr {}, sscratch", out(reg) ss, options(nomem, nostack)); }
        early_console::write_str("  sscratch = ");
        early_console::write_hex(ss);
        early_console::write_str("\n");
    }
    early_console::write_str("  protection live; uecall handler on stvec\n");

    // 4. Kernel information page (production Phase B.5) — publishes the
    //    page the birth chain will trap-query.
    minix_kernel::init_kerninfo(&result.kernel_info);
    early_console::write_str("  kerninfo page published\n");

    // 5. Process table (Phase C) — the VM branch loads the embedded
    //    rt-birth ELF (segments, 64 KiB stack, ps_strings, boot context).
    minix_kernel::init_proc_and_boot(&result.kernel_info);
    minix_kernel::init_smp_state();
    early_console::write_str("  proc table + smp state initialized\n");

    // 6. BKL held (C main.c:149); make the VM boot process (rt-birth)
    //    runnable. Full-context style per the boot-context contract.
    early_console::write_str("  acquiring BKL\n");
    minix_kernel::smp::bkl_lock().transfer();
    early_console::write_str("  BKL held\n");
    {
        let table = unsafe { minix_kernel::proc_table_boot_unchecked() };
        early_console::write_str("  table up\n");
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

    // 7. Hand the CPU over — sret into the ELF entry at 5 GiB, U-mode.
    early_console::write_str("  entering scheduler (switch_to_user)\n");
    minix_kernel::switch_to_user();
}

fn bump(num_pages: usize) -> Option<u64> {
    // Boot bump allocations sit at DRAM + 32 MB (past the kernel image and
    // the embedded ELF), ending before 64 MB — hello-boot-riscv64's layout.
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

fn fail() -> ! {
    early_console::write_str("### TEST_RESULT: FAIL test-rt-birth-riscv64 ###\n");
    loop { unsafe { asm!("wfi", options(nomem, nostack)); } }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    early_console::write_str("### PANIC in test-rt-birth-riscv64: ");
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
    loop { unsafe { asm!("wfi", options(nomem, nostack)); } }
}
