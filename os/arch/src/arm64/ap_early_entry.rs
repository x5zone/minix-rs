//! aarch64 AP early stub — its assembly body mirrors the riscv64 half.
//!
//! # Entry state (PSCI CPU_ON, SMC64 form — see `smp.rs::boot_ap`)
//!
//! - x0 = context_id (the bootstrap cookie; `boot_ap` hands the entry
//!   cookie through)
//! - PC = the physical entry address `boot_ap` passed as x2
//! - EL1, MMU **off**, caches in their reset state
//!
//! Unlike the riscv64 half there is **no boot election**: on the UEFI
//! carrier only one CPU is ever dispatched into boot services (the boot
//! processor); the secondaries sit in the firmware PSCI parking loop and
//! are extracted purely by `CPU_ON`. The wiring therefore self-skips via
//! the kernel's identity anchor (`current_cpu_id`) instead of an election
//! cell.
//!
//! # Stub obligations (frozen)
//!
//! 1. `dsb ish` — consumer-side ordering barrier before the first read.
//! 2. Read the `ApBootstrap` record (arch-local static storage;
//!    `adrp+add` PC-relative resolves to the runtime address — a
//!    physical address while the MMU is off, same argument as the
//!    riscv64 `la`).
//! 3. TTBR0/TTBR1 ← the record's page-table root (the kernel pins both
//!    TTBRs to the same root — `paging.rs::enable`; MAIR ← the kernel's
//!    `0xFF`; TCR ← the same constant expression `enable()` builds).
//! 4. `dsb sy` + `isb`; SCTLR.M = 1 + `isb` (MMU on — the boot root
//!    covers the kernel image both identity and high, so the PC remains
//!    valid across the enable).
//! 5. `sp` ← the record's per-AP stack; x0 ← the record's high VA;
//!    branch to the `ap_early_entry` Rust convergence point.
//!
//! # Identity-coverage verification (aarch64 half)
//!
//! The stub and record live inside the kernel image; the boot-shim root
//! covers the image **both at its high VMA and identity** plus the DM
//! windows. The aarch64 layout is a fixed-offset model exactly like the
//! riscv64 one (`aarch64.ld`: KERNEL_VIRT_BASE=0xFFFF800000000000,
//! KERNEL_PHYS_BASE=0x40200000, every section's LMA = VMA − KVIRT +
//! KPHYS) — so `entry_start_pa` is a link-time constant subtraction, not
//! a dynamic-loader query.
//!
//! # Deviation from the riscv64 half (deliberate)
//!
//! No UART step marker anywhere: on the riscv half, device-address
//! assumptions broke with the root-table shape (the A3 write faulted
//! there), and `AP_ARRIVED` is the gate's authoritative marker anyway.
//! The convergence snapshots the record, stores the arrival marker, and then
//! hands off to the kernel-registered AP tail; without a registered tail it
//! parks in `wfi` (the bare-ladder test-kernel form).

use crate::arch::ap_early_entry::ApBootstrap;
use core::sync::atomic::{AtomicPtr, Ordering};

/// Kernel-registered AP continuation (the same split as the x86 ladder):
/// the arch convergence hands the per-CPU bring-up policy to the kernel.
/// Before registration the AP parks, so no tail can run before the kernel
/// installed one.
type ApTailFn = unsafe extern "C" fn(logical_id: u32, hw_id: u64, kernel_stack_top_va: u64) -> !;
static AP_TAIL: AtomicPtr<()> = AtomicPtr::new(core::ptr::null_mut());

/// Register the kernel-side AP tail. Must run before the first PSCI
/// `CPU_ON` (`smp_init` does).
pub fn register_ap_tail(f: ApTailFn) {
    AP_TAIL.store(f as *mut (), Ordering::Release);
}

/// Early hart election cell. On a QEMU direct `-kernel` ELF boot every CPU
/// enters at the same entry, the same model as riscv fw_dynamic — the
/// kernel-image `_start` elects a leader with LD/EXCL and stores MPIDR+1;
/// 0 = not elected. On the UEFI/boot-shim chain a single CPU enters the
/// entry point, so the election always goes to it and the cell's semantics
/// are unchanged. Placed in .data (the loader writes the initial value 0
/// per filesz — not a NOBITS accident); no_mangle so the kernel-image
/// stub's asm can reference it literally and the kernel can read it.
#[unsafe(link_section = ".data")]
#[unsafe(no_mangle)]
pub static BOOT_HART_ELECTED: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

/// Per-AP bootstrap record — arch-local static storage (the arm form has
/// no <1MiB copy constraint, firmware starts the AP directly at an image
/// physical address). BSP fills via [`record`] before `boot_ap`; the
/// AP-side stub reads it PC-relative (`adrp+add` resolves to the runtime
/// address while the MMU is off).
#[unsafe(no_mangle)]
pub static mut AP_BOOTSTRAP_RECORD: ApBootstrap = ApBootstrap {
    logical_id: 0,
    _pad: 0,
    hw_id: 0,
    page_table_root_pa: 0,
    kernel_stack_top_va: 0,
    rust_entry_va: 0,
};

/// BSP-side fill window (before `boot_ap`). `&mut` access is exclusive
/// by the boot sequence: the BSP writes while the AP is not yet running
/// (the riscv-half parity note applies verbatim).
#[allow(static_mut_refs)]
pub fn record() -> &'static mut ApBootstrap {
    unsafe { &mut AP_BOOTSTRAP_RECORD }
}

/// Physical entry address of the AP stub (`boot_ap`'s `entry` argument).
///
/// Fixed-offset link model (`aarch64.ld`): every section's LMA = VMA −
/// KERNEL_VIRT_BASE + KERNEL_PHYS_BASE, so the stub's physical address
/// is its link address minus the constant delta. PSCI CPU_ON's
/// entry_point must be physical (the AP starts with the MMU off).
pub fn entry_start_pa() -> usize {
    const KVIRT: usize = 0xFFFF_8000_0000_0000;
    // KPHYS = 0x4020_0000 (KERNEL_PHYS_BASE in aarch64.ld). This constant
    // was once mistyped as 0x0402_0000 (one underscore slot off = one
    // missing zero, a 16x value error): the AP was sent to a garbage
    // address and died with an undef, and stub, record and channel all
    // being correct did not help. The constant must be checked against
    // the linker script.
    const KPHYS: usize = 0x4020_0000;
    let va = ap_early_entry_start as usize;
    va - (KVIRT - KPHYS)
}

unsafe extern "C" {
    /// Physical entry of the stub (`boot_ap`'s `start_addr`).
    fn ap_early_entry_start();
}

// ── AP early stub (the assembly body of the obligations list) ──────────────
//
// Entry state (PSCI CPU_ON SMC64): x0 = cookie, PC = start_addr, EL1, MMU off.
//
// Five obligations: dsb ish (consumer-side fence) → adrp+add the record's
// runtime address and read its three fields → TTBR0/TTBR1 to one root +
// MAIR 0xFF + the TCR constant (same value as paging.rs::enable; the
// constant is injected through global_asm const template parameters —
// hand-written immediates are ruled out by a past head-arithmetic mistake)
// → dsb sy + isb, SCTLR.M = 1, isb (mirrors enable()'s enabling sequence:
// only the M bit is set, RES1 bits keep their reset values) → sp/x0/br
// into the convergence point.
core::arch::global_asm!(
    r#"
    .section .text.ap_early_entry, "ax"
    .globl ap_early_entry_start
    ap_early_entry_start:
        dsb ish                         // consumer-side fence (S3.9)
        adrp x2, AP_BOOTSTRAP_RECORD
        add  x2, x2, :lo12:AP_BOOTSTRAP_RECORD
        ldr  x3, [x2, #16]              // page_table_root_pa
        ldr  x4, [x2, #24]              // kernel_stack_top_va
        ldr  x5, [x2, #32]              // rust_entry_va
        msr  ttbr0_el1, x3              // kernel pins both TTBRs to one root
        msr  ttbr1_el1, x3
        mov  x6, #0xff                  // MAIR — same constant as enable()
        msr  mair_el1, x6
        movz x6, #{tcr_a}               // TCR — same constant expression as
        movk x6, #{tcr_b}, lsl #16      //   enable() (T0SZ=T1SZ=16 halves)
        movk x6, #{tcr_c}, lsl #32
        msr  tcr_el1, x6
        dsb  sy
        isb
        mrs  x6, sctlr_el1
        orr  x6, x6, #1                 // SCTLR.M = 1 (mirror enable())
        msr  sctlr_el1, x6
        isb
        // B2=MMU-on 后仍可取指执行（恒等映射覆盖桩的实证）；缺失=根表
        // 恒等映射缺失（riscv A2/A3 同族取证位）。
        ldr x9, =0x09000000
        mov x10, #0x42                  // 'B'
        strb w10, [x9]
        mov x10, #0x32                  // '2'
        strb w10, [x9]
        mov  sp, x4                     // per-AP kernel stack (high VA)
        movz x6, #{d_a}                 // x0 = record HIGH VA =
        movk x6, #{d_b}, lsl #16        //   record PA + (KVIRT - KPHYS);
        movk x6, #{d_c}, lsl #32        //   reachable through the kernel
        movk x6, #{d_d}, lsl #48        //   mapping now that MMU is on
        add  x0, x2, x6
        br   x5                         // jump to Rust convergence (high VA)
    "#,
    tcr_a = const (AP_TCR & 0xffff),
    tcr_b = const ((AP_TCR >> 16) & 0xffff),
    tcr_c = const ((AP_TCR >> 32) & 0xffff),
    d_a = const (VA_DELTA & 0xffff),
    d_b = const ((VA_DELTA >> 16) & 0xffff),
    d_c = const ((VA_DELTA >> 32) & 0xffff),
    d_d = const ((VA_DELTA >> 48) & 0xffff),
);

/// TCR_EL1 value the stub installs — the same constant expression
/// `paging.rs::enable()` builds (T0SZ=T1SZ=16, both 48-bit halves;
/// IRGN/ORGN/RSH shares as in enable). Kept next to the asm so the two
/// cannot drift apart; the pin test asserts the assembly matches this
/// constant.
const AP_TCR: u64 = (16u64 << 0)
    | (0 << 6)
    | (0 << 7)
    | (1 << 8)
    | (1 << 10)
    | (3 << 12)
    | (0 << 14)
    | (16u64 << 16)
    | (0 << 22)
    | (0 << 23)
    | (1 << 24)
    | (1 << 26)
    | (3 << 28)
    | (2 << 30)
    | (5 << 32)
    | (0 << 36)
    | (0 << 37);

/// Record PA → record high-VA delta, identical to `entry_start_pa`'s
/// link-model constants (KVIRT − KPHYS). The same KPHYS typo (0x0402_0000)
/// once survived at this second site: x0 was computed to an invalid
/// high-half address, the convergence point's first read faulted, and
/// AP_ARRIVED never got set. Check the constants against aarch64.ld.
const VA_DELTA: usize = 0xFFFF_8000_0000_0000 - 0x4020_0000;

/// Convergence-point arrival marker (the observation surface for the gate
/// criterion "a secondary core observably reaches the convergence point";
/// same name and meaning as the riscv half). 0 = not arrived; 1 = arrived
/// (the Rust view after the Release store).
pub static AP_ARRIVED: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);

/// Convergence point — the AP jumps here from the stub (x0 = the record's
/// high VA).
///
/// The aarch64 form of the same-named riscv function: snapshot the record
/// fields into locals (so a BSP rewrite for the next hart cannot interfere),
/// set the arrival marker (Release), then hand off to the kernel tail if one
/// is registered; without a tail it parks (wfi) — the bare-ladder form used
/// by the bring-up test kernels.
///
/// Two deliberate differences from the riscv half: (1) no UART marker (a
/// lesson: device addresses depend on the root-table shape and are
/// unreliable; AP_ARRIVED is the authoritative criterion); (2) a0/x0
/// carries the record's **high VA** (the stub already converted it), which
/// the kernel root mapping can reach.
pub unsafe extern "C" fn ap_early_entry(bootstrap_va: usize) -> ! {
    let record = unsafe { &*(bootstrap_va as *const ApBootstrap) };
    // Consume the snapshot before publishing the arrival marker: the BSP is
    // serial and may rewrite the record for the next hart the moment it sees
    // AP_ARRIVED. All three fields the tail needs are local from here on.
    let logical = unsafe { core::ptr::read_volatile(&record.logical_id) };
    let hw_id = unsafe { core::ptr::read_volatile(&record.hw_id) };
    let stack_top = unsafe { core::ptr::read_volatile(&record.kernel_stack_top_va) };
    // Arrival marker (Release: when a later BSP read sees 1, the snapshot
    // reads above have completed).
    core::sync::atomic::AtomicUsize::store(&AP_ARRIVED, 1, core::sync::atomic::Ordering::Release);

    let tail = AP_TAIL.load(Ordering::Acquire);
    if !tail.is_null() {
        // Tail divergence — the kernel continuation owns the CPU from here
        // (per-CPU identity, trap entry, timer, finish handshake, scheduler).
        let f = unsafe { core::mem::transmute::<*mut (), ApTailFn>(tail) };
        unsafe { f(logical, hw_id, stack_top) }
    }
    loop {
        unsafe {
            core::arch::asm!("wfi", options(nomem, nostack));
        }
    }
}
