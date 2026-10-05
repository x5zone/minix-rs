//! Hardware Abstraction Layer
//!
//! Provides cross-architecture hardware mechanism abstractions and trait interfaces.
//! Concrete implementations are provided by each architecture module (mock, x86_64, arm64, riscv64).
//!
//! # Crate structure
//!
//! - **CPU ISA traits** (this crate): paging, protection, trap entry, exception, clock, etc.
//! - **Board-level platform traits** (`minix-plat`): early console, interrupt controller
//!
//! # Design principles
//!
//! 1. **Distributed definition**: Each feature module defines its own traits (e.g. paging, interrupts, timers)
//! 2. **Centralized implementation**: All traits are implemented within the arch crate
//! 3. **Architecture-independent**: OS code depends only on traits, not on specific hardware

#![cfg_attr(not(feature = "runtime-window"), no_std)]

extern crate alloc;

pub mod arch;

#[cfg(target_arch = "x86_64")]
pub mod x86_64;
#[cfg(target_arch = "aarch64")]
pub mod arm64;
#[cfg(target_arch = "riscv64")]
pub mod riscv64;
// Sv39 走表纯逻辑层：真机随 riscv64 编译；宿主测试（runtime-window 默认
// 特性）单编译，供 walk_read 对抗测试（NK4C §续-349）。
#[cfg(any(target_arch = "riscv64", all(test, feature = "runtime-window")))]
pub mod riscv64_walk;

// ── Re-export board-level platform abstractions from minix-plat ──
pub use minix_plat::{
    EarlyConsole, InterruptRouter, PerCpuInterruptUnit, IrqVector, IrqId, IrqNotifyId, IrqPolicy, IrqAction,
    NR_IRQ_VECTORS, NR_IRQ_HOOKS, CurrentInterruptController, CurrentEarlyConsole,
};

#[cfg(feature = "runtime-window")]
pub use minix_plat::{MockInterruptController, MockEarlyConsole};

// ── Backward-compatible re-exports of CPU ISA modules ──
pub use arch::paging;
pub use arch::paging_ext;
pub use arch::pt_alloc;
pub use arch::direct_map;
pub use arch::frame;
pub use arch::pte_walk_arch;
pub use arch::protection;
pub use arch::trap_entry;
pub use arch::trap_return;
pub use arch::exception;
pub use arch::exception_dispatcher;
pub use arch::clock;
pub use arch::fpu_arch;
pub use arch::signal_context;
pub use arch::smp;
pub use arch::cpu_identity;
pub use arch::arch_init;
pub use arch::timer_irq_gate;
pub use arch::boot;
pub use arch::stacktrace;
pub use arch::tlb_arch;

pub use paging_ext::{PagingWithId, HugePages};
pub use direct_map::DirectMapArch;
pub use arch::dm_coverage::{DmCoverageArch, DmRange, establish_dm_range};
pub use pte_walk_arch::PteWalkArch;
#[cfg(feature = "runtime-window")]
pub use pte_walk_arch::MockPteWalk;
pub use protection::{ProtectionArch, Privilege, InterruptVector};
#[cfg(feature = "runtime-window")]
pub use protection::MockProtection;
pub use trap_entry::TrapEntryArch;
#[cfg(feature = "runtime-window")]
pub use trap_entry::MockTrapEntry;
pub use trap_return::TrapReturnArch;
#[cfg(feature = "runtime-window")]
pub use trap_return::MockTrapReturn;
pub use exception::{ExceptionArch, FaultContext, RecoveryPoint};
pub use exception_dispatcher::{
    ExceptionDispatcher, ExceptionOutcome, ExceptionClass, ExceptionSignal,
};
pub use clock::{ClockArch, DEFAULT_HZ};
pub use timer_irq_gate::TimerIrqGate;
#[cfg(feature = "runtime-window")]
pub use clock::MockClockArch;
pub use fpu_arch::{FpuArch, MockFpuArch, MockFpuState};
pub use signal_context::{
    SignalContext, SignalInfo, MockSignalContext,
    MockSigContext, MockSigFrame, MockSigCpuContext,
    SC_MAGIC, MF_FPU_INITIALIZED, MF_CONTEXT_SET, X86_FLAGS_USER,
    KTS_NONE, KTS_INT_HARD, KTS_INT_ORIG, KTS_FULLCONTEXT, KTS_SYSCALL,
};
pub use arch::trap_style::{ReturnSequence, TrapStyle};
pub use smp::SmpArch;
pub use arch_init::ArchInit;
pub use boot::{
    CpuContextArch, EntrySpec, ProcKind, ProcNr,
    ProcessLoad, VmLoadResult, VmLoadError, load_process_elf, load_vm_elf,
};
pub use stacktrace::StacktraceArch;
pub use arch::current::{Arch, CurrentArch};

#[cfg(feature = "runtime-window")]
pub use paging::mock::MockPaging;

#[cfg(feature = "runtime-window")]
pub use paging::mock::MockAsid;

// ── CurrentPaging type alias ──
//
// When `mock` feature is enabled (tests), always use `MockPaging` regardless
// of host `target_arch`. This prevents tests running on x86_64 host from
// selecting the real `X86_64Paging` (which touches Direct Map memory and
// would SIGSEGV outside QEMU). For the real kernel build (no `mock`
// feature), select by `target_arch`.
#[cfg(feature = "runtime-window")]
pub type CurrentPaging = MockPaging;
#[cfg(all(not(feature = "runtime-window"), target_arch = "x86_64"))]
pub type CurrentPaging = crate::x86_64::paging::X86_64Paging;
#[cfg(all(not(feature = "runtime-window"), target_arch = "aarch64"))]
pub type CurrentPaging = crate::arm64::paging::AArch64Paging;
#[cfg(all(not(feature = "runtime-window"), target_arch = "riscv64"))]
pub type CurrentPaging = crate::riscv64::paging::Riscv64Paging;

#[cfg(feature = "runtime-window")]
pub use direct_map::MockDirectMap;

#[cfg(target_arch = "x86_64")]
pub use direct_map::X86_64DirectMap;
#[cfg(target_arch = "aarch64")]
pub use direct_map::AArch64DirectMap;
#[cfg(target_arch = "riscv64")]
pub use direct_map::Riscv64DirectMap;

#[cfg(feature = "runtime-window")]
pub type CurrentDirectMap = MockDirectMap;
#[cfg(all(not(feature = "runtime-window"), target_arch = "x86_64"))]
pub type CurrentDirectMap = X86_64DirectMap;
#[cfg(all(not(feature = "runtime-window"), target_arch = "aarch64"))]
pub type CurrentDirectMap = AArch64DirectMap;
#[cfg(all(not(feature = "runtime-window"), target_arch = "riscv64"))]
pub type CurrentDirectMap = Riscv64DirectMap;

// ── CurrentDmCoverage type alias ──
//
// Selects the architecture-specific `DmCoverageArch` implementor (DM
// coverage establishment on the bootstrap root). Like `CurrentPteWalk`, the
// implementor is a ZST: the operation is static boot-time table surgery, not
// a per-handle page-table service.
#[cfg(feature = "runtime-window")]
pub use arch::dm_coverage::mock::MockDmCoverage;
#[cfg(all(not(feature = "runtime-window"), target_arch = "x86_64"))]
pub use x86_64::paging::X86_64DmCoverage;
#[cfg(all(not(feature = "runtime-window"), target_arch = "aarch64"))]
pub use arm64::paging::AArch64DmCoverage;
#[cfg(all(not(feature = "runtime-window"), target_arch = "riscv64"))]
pub use riscv64::paging::Riscv64DmCoverage;

#[cfg(feature = "runtime-window")]
pub type CurrentDmCoverage = MockDmCoverage;
#[cfg(all(not(feature = "runtime-window"), target_arch = "x86_64"))]
pub type CurrentDmCoverage = X86_64DmCoverage;
#[cfg(all(not(feature = "runtime-window"), target_arch = "aarch64"))]
pub type CurrentDmCoverage = AArch64DmCoverage;
#[cfg(all(not(feature = "runtime-window"), target_arch = "riscv64"))]
pub type CurrentDmCoverage = Riscv64DmCoverage;

// ── CurrentPteWalk type aliases ──
//
// Selects the architecture-specific `PteWalkArch` implementor at compile
// time. The kernel's cross-space copy code (`vm.rs::lookup_in_table`)
// uses `CurrentPteWalk` as the generic type parameter, so no
// `#[cfg(target_arch)]` leaks into kernel code (Phase 2 of the
// TODO/DEFERRED completion plan).
#[cfg(feature = "runtime-window")]
pub type CurrentPteWalk = MockPteWalk;
#[cfg(all(not(feature = "runtime-window"), target_arch = "x86_64"))]
pub type CurrentPteWalk = crate::x86_64::paging::X86_64PteWalk;
#[cfg(all(not(feature = "runtime-window"), target_arch = "aarch64"))]
pub type CurrentPteWalk = crate::arm64::paging::AArch64PteWalk;
#[cfg(all(not(feature = "runtime-window"), target_arch = "riscv64"))]
pub type CurrentPteWalk = crate::riscv64::paging::Riscv64PteWalk;

// ── CurrentProtection type aliases ──
//
// Mock branch prevents tests on x86_64 host from touching real GDT/TSS
// registers (FIX-06: R-11). Matches the pattern used by CurrentPaging,
// CurrentDirectMap, CurrentClockArch, etc.
#[cfg(feature = "runtime-window")]
pub type CurrentProtection = MockProtection;
#[cfg(all(not(feature = "runtime-window"), target_arch = "x86_64"))]
pub type CurrentProtection = crate::x86_64::protection::X86_64Protection;
#[cfg(all(not(feature = "runtime-window"), target_arch = "aarch64"))]
pub type CurrentProtection = crate::arm64::protection::AArch64Protection;
#[cfg(all(not(feature = "runtime-window"), target_arch = "riscv64"))]
pub type CurrentProtection = crate::riscv64::protection::Riscv64Protection;

// ── Bootstrap allocation bounds ──
//
// End of the boot identity-mapped window: `arch_boot_impl` maps PA
// `[0, END)` identity on the bootstrap root (C: pg_identity() maps
// 1024 × 4MiB = 4 GiB — pg_utils.c:162). Bootstrap allocations that must
// stay identity-write-reachable (the page-table root, the boot bump
// region) have to sit below this end.
pub const BOOT_IDENTITY_MAP_END: u64 = 0x1_0000_0000;

/// Highest PA *end* any bootstrap-tree allocation (page-table root, boot
/// bump region) may use: `min(BOOT_IDENTITY_MAP_END, VM DM window PA end)`.
///
/// Below this bound the bootstrap root is simultaneously
/// - identity-write-reachable — the DM establishment write channel
///   (`VA = PA`) works, and
/// - representable in the VM DM window — VM self page-table walks can
///   read/write root and PT pages through the window
///
/// (07-paging_init_design §6.1 资格过滤 ②). The boot-shims enforce this at
/// allocation time; the kernel re-validates at DM establishment time.
pub const fn boot_dm_admissible_end() -> u64 {
    // `core::cmp::min` is not const-stable; compare explicitly.
    if BOOT_IDENTITY_MAP_END < CurrentDirectMap::VM_DIRECT_MAP_SIZE {
        BOOT_IDENTITY_MAP_END
    } else {
        CurrentDirectMap::VM_DIRECT_MAP_SIZE
    }
}

/// MMIO pages the kernel keeps touching through LOW virtual addresses
/// (VA = PA) while running on a VM-built process page table (NK4-A
/// fix27, complements `KernelInfo::reserved_regions`).
///
/// These are NOT in the UEFI memory map's RAM descriptors (device
/// memory is `EfiMemoryMappedIO`, deliberately excluded from the
/// reserved snapshot because the windows are huge and uncachable), yet
/// the kernel dereferences them at plain physical addresses — e.g. the
/// local APIC EOI write on every timer tick
/// (`os/arch/src/x86_64/clock.rs`: `lapic_base as *mut u32` with the
/// default 0xFEE00000). A process page table missing them faults the
/// first tick after a CR3 switch.
///
/// x86-64: the IO-APIC (0xFEC0_0000) and local-APIC (0xFEE0_0000)
/// pages at their QEMU/default bases. Other architectures and the mock
/// window: empty — their interrupt controllers are not dereferenced at
/// fixed low VAs on this boot path yet.
#[cfg(target_arch = "x86_64")]
pub const fn kernel_identity_mmio_regions() -> &'static [(u64, u64)] {
    &[
        (0x0FEC0_0000, 4096), // IO-APIC
        (0x0FEE0_0000, 4096), // local APIC (default base)
    ]
}

/// Non-x86-64: no fixed low-VA MMIO the kernel dereferences yet
/// (see the x86-64 doc).
#[cfg(not(target_arch = "x86_64"))]
pub const fn kernel_identity_mmio_regions() -> &'static [(u64, u64)] {
    &[]
}

// ── CurrentTrapEntry type aliases ──
//
// Mock branch prevents tests on x86_64 host from touching real IDT
// registers (FIX-06: R-11).
#[cfg(feature = "runtime-window")]
pub type CurrentTrapEntry = MockTrapEntry;
#[cfg(all(not(feature = "runtime-window"), target_arch = "x86_64"))]
pub type CurrentTrapEntry = crate::x86_64::trap_entry::X86_64TrapEntry;
#[cfg(all(not(feature = "runtime-window"), target_arch = "aarch64"))]
pub type CurrentTrapEntry = crate::arm64::trap_entry::AArch64TrapEntry;
#[cfg(all(not(feature = "runtime-window"), target_arch = "riscv64"))]
pub type CurrentTrapEntry = crate::riscv64::trap_entry::Riscv64TrapEntry;

// ── S-8 trap-stub facade (smp_todo.md §3.7) ──
//
// `install_trap_stubs` writes the arch's real asm entry addresses into every
// gate of the table; `syscall_entry_va` reports the SYSCALL entry address for
// `TrapEntryArch::configure_syscall` (x86-64 LSTAR). Together they turn
// `TrapEntryArch::load()` from "deferred" into a safe call: after both run,
// no gate has an empty handler (the S-8 stage invariant).
//
// aarch64/riscv64 entry shape (updated by E-3ARCHTRAP, 2026-09-20): both
// archs use a single fixed asm vector (VBAR_EL1 table / stvec direct mode
// with kernel+user legs), where `set_handler` is already a documented no-op
// — there is nothing to install per-vector, so `install_trap_stubs` is a
// documented no-op there. Their asm symbols (`exc_vector_table`,
// `riscv64_{kernel,user}_trap_vector`) are defined by `global_asm!` in the
// same modules that declare them (the K9 defusal; the earlier S-8
// inventory finding "declared but never defined" is historical), and the
// production frame-save legs + dispatch thunks live in each arch's
// `trap_stub`.
#[cfg(feature = "runtime-window")]
pub fn install_trap_stubs(_entry: &mut CurrentTrapEntry) {}
#[cfg(all(not(feature = "runtime-window"), target_arch = "x86_64"))]
pub fn install_trap_stubs(entry: &mut CurrentTrapEntry) {
    crate::x86_64::trap_stub::install_idt_handlers(entry);
}
#[cfg(all(not(feature = "runtime-window"), any(target_arch = "aarch64", target_arch = "riscv64")))]
pub fn install_trap_stubs(_entry: &mut CurrentTrapEntry) {
    // Fixed asm tables (VBAR_EL1 / stvec): nothing per-vector to install —
    // `set_handler` is a documented no-op on both archs, and the asm
    // symbols are defined by `global_asm!` in the same modules that
    // declare them (K9), so `load()` cannot link-fail. The production
    // frame-save legs + dispatch thunks live in each arch's `trap_stub`.
}

/// SYSCALL entry address for `TrapEntryArch::configure_syscall`.
///
/// x86-64: the LSTAR asm entry (`x86_syscall_entry`). aarch64: the
/// lower-EL AArch64 synchronous slot (VBAR + 0x400, the SVC entry);
/// riscv64: the user-leg `stvec` entry — their SYSCALL (SVC/ecall) shares
/// the exception vector, so `configure_syscall` is a no-op and the value
/// is metadata only (C parity: the kernel sources the entry from its own
/// asm label, protect.c:189-205 — not from the boot-provided
/// `KernelInfo.syscall_entry`, which is reference-only metadata).
#[cfg(feature = "runtime-window")]
pub fn syscall_entry_va() -> minix_types::VirBytes {
    minix_types::VirBytes::new(0)
}
#[cfg(all(not(feature = "runtime-window"), target_arch = "x86_64"))]
pub fn syscall_entry_va() -> minix_types::VirBytes {
    crate::x86_64::trap_stub::syscall_entry_va()
}
#[cfg(all(not(feature = "runtime-window"), target_arch = "aarch64"))]
pub fn syscall_entry_va() -> minix_types::VirBytes {
    crate::arm64::trap_stub::syscall_entry_va()
}
#[cfg(all(not(feature = "runtime-window"), target_arch = "riscv64"))]
pub fn syscall_entry_va() -> minix_types::VirBytes {
    crate::riscv64::trap_stub::user_trap_vector_va()
}

/// Persist an interrupted user register file into the per-process saved
/// context (E1 trap bridge — design decision 3: single user-state truth).
/// Pure register-file copying (no IDT/MSR hardware), so the x86-64
/// implementation stays live under `runtime-window` — hosted integration
/// tests exercise the E1 arm against the real context layout; only gate
/// *registration* is mocked there.
#[cfg(target_arch = "x86_64")]
pub fn save_frame_to_context(
    frame: &x86_64::trap_stub::TrapFrame,
    ctx: &mut CurrentCpuContext,
) {
    crate::x86_64::trap_stub::save_frame_to_context(frame, ctx);
}
#[cfg(target_arch = "aarch64")]
pub fn save_frame_to_context(
    frame: &arm64::trap_stub::AArch64TrapFrame,
    ctx: &mut CurrentCpuContext,
) {
    crate::arm64::trap_stub::save_frame_to_context(frame, ctx);
}
#[cfg(target_arch = "riscv64")]
pub fn save_frame_to_context(
    frame: &riscv64::trap_stub::Riscv64TrapFrame,
    ctx: &mut CurrentCpuContext,
) {
    crate::riscv64::trap_stub::save_frame_to_context(frame, ctx);
}
#[cfg(not(any(
    target_arch = "x86_64",
    target_arch = "aarch64",
    target_arch = "riscv64"
)))]
pub fn save_frame_to_context(_frame: &(), _ctx: &mut CurrentCpuContext) {}

/// Pull the IPC status register from a process's saved context into the
/// outgoing trap frame (E1: the stub's iretq must restore the up-to-date
/// RBX — delivery paths OR status into the saved context, which the entry
/// save happened before). x86-64: RBX. Other archs: no-op.
#[cfg(target_arch = "x86_64")]
pub fn sync_status_register_to_frame(
    ctx: &CurrentCpuContext,
    frame: &mut x86_64::trap_stub::TrapFrame,
) {
    crate::x86_64::trap_stub::sync_status_register_to_frame(ctx, frame);
}
#[cfg(target_arch = "aarch64")]
pub fn sync_status_register_to_frame(
    ctx: &CurrentCpuContext,
    frame: &mut arm64::trap_stub::AArch64TrapFrame,
) {
    crate::arm64::trap_stub::sync_status_register_to_frame(ctx, frame);
}
#[cfg(target_arch = "riscv64")]
pub fn sync_status_register_to_frame(
    ctx: &CurrentCpuContext,
    frame: &mut riscv64::trap_stub::Riscv64TrapFrame,
) {
    crate::riscv64::trap_stub::sync_status_register_to_frame(ctx, frame);
}
#[cfg(not(any(
    target_arch = "x86_64",
    target_arch = "aarch64",
    target_arch = "riscv64"
)))]
pub fn sync_status_register_to_frame(_ctx: &CurrentCpuContext, _frame: &mut ()) {}

/// Read back the saved IPC return register of a process's saved user
/// context (E1 slice 2 test seam — the write side is kernel
/// `set_ipc_return_code`, which goes through the arch's
/// `set_ipc_return_reg` trait method; the register file itself is
/// arch-private). Routed through the same trait so the readback matches
/// the write on every arch (NK4-C §1.113: the old x86-only `offset 80`
/// reader returned 0 on aarch64, masking the X0/X7 write mismatch).
pub fn ipc_return_code(ctx: &CurrentCpuContext) -> u64 {
    use crate::arch::boot::CpuContextArch as _;
    <CurrentCpuContextArch as CpuContextArch>::ipc_return_reg(ctx)
}

/// Read back the saved IPC status register of a process's saved user
/// context (test seam, same shape as [`ipc_return_code`] — the write
/// side is kernel `ipc_status_*` / the plain-RECEIVE prologue clear;
/// the register file itself is arch-private). Mock/other-arch: 0.
#[cfg(target_arch = "x86_64")]
pub fn ipc_status_register(ctx: &CurrentCpuContext) -> u64 {
    crate::x86_64::trap_stub::ipc_status_register(ctx)
}
#[cfg(not(target_arch = "x86_64"))]
pub fn ipc_status_register(_ctx: &CurrentCpuContext) -> u64 {
    0
}
// (syscall_entry_va for aarch64/riscv64 lives in the per-arch cfg blocks
// above — the merged `any(aarch64, riscv64)` stub returning 0 was removed
// with E-3ARCHTRAP: returning a fake entry address masked the fact that no
// production trap legs existed.)

/// Register the kernel-side trap/syscall dispatch bodies with the x86-64
/// entry stubs (see `x86_64::trap_stub` for the gate rationale). Must run
/// before `TrapEntryArch::load()`; a no-op on the mock/other-arch branches
/// where the stubs do not exist.
#[cfg(feature = "runtime-window")]
pub fn register_trap_dispatchers(
    _trap: unsafe extern "C" fn(&mut x86_64::trap_stub::TrapFrame),
    _syscall: unsafe extern "C" fn(&mut x86_64::trap_stub::TrapFrame),
) {
}
#[cfg(all(not(feature = "runtime-window"), target_arch = "x86_64"))]
pub fn register_trap_dispatchers(
    trap: unsafe extern "C" fn(&mut x86_64::trap_stub::TrapFrame),
    syscall: unsafe extern "C" fn(&mut x86_64::trap_stub::TrapFrame),
) {
    crate::x86_64::trap_stub::register_dispatchers(trap, syscall);
}
#[cfg(all(not(feature = "runtime-window"), target_arch = "aarch64"))]
pub fn register_trap_dispatchers(
    trap: unsafe extern "C" fn(&mut arm64::trap_stub::AArch64TrapFrame, u64) -> u64,
    syscall: unsafe extern "C" fn(&mut arm64::trap_stub::AArch64TrapFrame, u64) -> u64,
) {
    // Slot semantics: `trap` = current-EL (kernel) leg, `syscall` =
    // lower-EL (user) leg — the vector table splits by origin group. The
    // second operand is the exception class (sync/IRQ) the slot ran. The
    // u64 return is the §1.113 park decision.
    crate::arm64::trap_stub::register_dispatchers(trap, syscall);
}

/// Register the diverging park-and-reschedule entry for the lower-EL
/// (user) dispatch legs (NK4-C §1.113 aarch64, 续-75 riscv64) with the
/// arch's `EL0BODY`/user-leg entry stub; the park branch jumps here after
/// unwinding a blocked-receiver frame. No-op elsewhere: the switch-after-
/// pop path is an EL1h/single-kernel-stack concern (x86 diverges safely
/// via TSS.sp0 reload).
#[cfg(not(all(
    not(feature = "runtime-window"),
    any(target_arch = "aarch64", target_arch = "riscv64")
)))]
pub fn register_resched_entry(_f: unsafe extern "C" fn() -> !) {}
#[cfg(all(
    not(feature = "runtime-window"),
    any(target_arch = "aarch64", target_arch = "riscv64")
))]
pub fn register_resched_entry(f: unsafe extern "C" fn() -> !) {
    #[cfg(target_arch = "aarch64")]
    crate::arm64::trap_stub::register_resched_entry(f);
    #[cfg(target_arch = "riscv64")]
    crate::riscv64::trap_stub::register_resched_entry(f);
}
#[cfg(all(not(feature = "runtime-window"), target_arch = "riscv64"))]
pub fn register_trap_dispatchers(
    trap: unsafe extern "C" fn(&mut riscv64::trap_stub::Riscv64TrapFrame),
    syscall: unsafe extern "C" fn(&mut riscv64::trap_stub::Riscv64TrapFrame) -> u64,
) {
    // Slot semantics: `trap` = kernel leg (S-origin), `syscall` = user
    // leg (U-origin) — the stvec legs split by interrupted privilege. The
    // user leg's u64 return is the 续-75 park decision (§1.113 twin); the
    // kernel leg never parks and keeps the unit shape.
    crate::riscv64::trap_stub::register_dispatchers(trap, syscall);
}

#[cfg(all(not(feature = "runtime-window"), target_arch = "x86_64"))]
pub use x86_64::trap_stub::TrapFrame;

/// S-4 per-CPU bring-up helpers (§3.3 per-CPU MSR contract). No-ops on the
/// mock/other-arch branches; on x86-64 these write the CURRENT CPU's MSRs —
/// the AP wiring calls them from the AP itself.
#[cfg(feature = "runtime-window")]
pub fn ap_write_syscall_msrs(_entry: minix_types::VirBytes) {}
#[cfg(all(not(feature = "runtime-window"), target_arch = "x86_64"))]
pub fn ap_write_syscall_msrs(entry: minix_types::VirBytes) {
    crate::x86_64::trap_stub::write_syscall_msrs(entry);
}
#[cfg(all(not(feature = "runtime-window"), any(target_arch = "aarch64", target_arch = "riscv64")))]
pub fn ap_write_syscall_msrs(_entry: minix_types::VirBytes) {}

/// Enable THIS CPU's LAPIC (S-10: the AP-side per-CPU half of controller
/// init — the LAPIC is disabled after INIT and deaf to IPIs until enabled).
/// No-op on mock/other-arch branches.
#[cfg(feature = "runtime-window")]
pub fn ap_enable_lapic() {}
#[cfg(all(not(feature = "runtime-window"), target_arch = "x86_64"))]
pub fn ap_enable_lapic() {
    // SAFETY: the AP enables its own LAPIC during bring-up, before any IPI
    // can target it (see the plat function's safety contract).
    unsafe { minix_plat::x86_64::interrupt::X86_64InterruptController::enable_current_cpu_lapic(); }
}
#[cfg(all(not(feature = "runtime-window"), any(target_arch = "aarch64", target_arch = "riscv64")))]
pub fn ap_enable_lapic() {}

/// Read this CPU's per-CPU identity (x86-64: `gs:0x10` written by
/// `program_gs`; other archs return 0 — their identity path is per-CPU
/// register based and lands with their own S-4 lanes).
#[cfg(feature = "runtime-window")]
pub fn ap_cpu_id_readback() -> u64 {
    0
}
#[cfg(all(not(feature = "runtime-window"), target_arch = "x86_64"))]
pub fn ap_cpu_id_readback() -> u64 {
    crate::x86_64::trap_stub::gs_cpu_id()
}
#[cfg(all(not(feature = "runtime-window"), any(target_arch = "aarch64", target_arch = "riscv64")))]
pub fn ap_cpu_id_readback() -> u64 {
    0
}

// ── CurrentClockArch type aliases ──
#[cfg(feature = "runtime-window")]
pub type CurrentClockArch = MockClockArch;
#[cfg(all(not(feature = "runtime-window"), target_arch = "x86_64"))]
pub type CurrentClockArch = crate::x86_64::clock::X86_64ClockArch;
#[cfg(all(not(feature = "runtime-window"), target_arch = "aarch64"))]
pub type CurrentClockArch = crate::arm64::clock::AArch64ClockArch;
#[cfg(all(not(feature = "runtime-window"), target_arch = "riscv64"))]
pub type CurrentClockArch = crate::riscv64::clock::Riscv64ClockArch;

// ── CurrentCpuIdentity type aliases ──
//
// Selects the per-arch CPU identity probe at compile time (same mock-first
// pattern as CurrentPaging/CurrentClockArch: tests run on the x86_64 host
// and must not execute `cpuid`/`mrs`/`ecall`).
#[cfg(feature = "runtime-window")]
pub type CurrentCpuIdentity = arch::cpu_identity::mock::MockCpuIdentity;
#[cfg(all(not(feature = "runtime-window"), target_arch = "x86_64"))]
pub type CurrentCpuIdentity = crate::x86_64::cpu_identity::X86_64CpuIdentity;
#[cfg(all(not(feature = "runtime-window"), target_arch = "aarch64"))]
pub type CurrentCpuIdentity = crate::arm64::cpu_identity::AArch64CpuIdentity;
#[cfg(all(not(feature = "runtime-window"), target_arch = "riscv64"))]
pub type CurrentCpuIdentity = crate::riscv64::cpu_identity::Riscv64CpuIdentity;


// ── CurrentFpuArch type aliases ──
//
// Selects the architecture-specific `FpuArch` implementor at compile time.
// When `mock` feature is enabled (tests), uses `MockFpuArch` (all no-ops).
#[cfg(feature = "runtime-window")]
pub type CurrentFpuArch = MockFpuArch;
#[cfg(all(not(feature = "runtime-window"), target_arch = "x86_64"))]
pub type CurrentFpuArch = crate::x86_64::fpu::X86_64FpuArch;
#[cfg(all(not(feature = "runtime-window"), target_arch = "aarch64"))]
pub type CurrentFpuArch = crate::arm64::fpu::AArch64FpuArch;
#[cfg(all(not(feature = "runtime-window"), target_arch = "riscv64"))]
pub type CurrentFpuArch = crate::riscv64::fpu::Riscv64FpuArch;

// ── CurrentFpuState type aliases ──
//
// Selects the architecture-specific FPU state buffer type. This is the
// `State` associated type of `CurrentFpuArch`. Stored per-process in
// `KProcess.fpu_state` for save/restore during context switches and
// signal handling.
//
// # Sizes
//
// - mock (test):     0 bytes (ZST)
// - x86-64 (FXSAVE): 512 bytes
// - aarch64 (FPSIMD): 528 bytes
// - riscv64 (F/D):   264 bytes
//
// C: `p_seg.fpu_state[FPU_XFP_SIZE]` — kernel/proc.h
#[cfg(feature = "runtime-window")]
pub type CurrentFpuState = MockFpuState;
#[cfg(all(not(feature = "runtime-window"), target_arch = "x86_64"))]
pub type CurrentFpuState = crate::x86_64::fpu::X86_64FpuState;
#[cfg(all(not(feature = "runtime-window"), target_arch = "aarch64"))]
pub type CurrentFpuState = crate::arm64::fpu::AArch64FpuState;
#[cfg(all(not(feature = "runtime-window"), target_arch = "riscv64"))]
pub type CurrentFpuState = crate::riscv64::fpu::Riscv64FpuState;

// ── CurrentSignalContext type aliases ──
//
// Selects the architecture-specific `SignalContext` implementor at compile
// time. Unlike other arch traits, `SignalContext` has NO mock variant
// because its associated type `CpuContext` MUST match `CurrentCpuContext`
// (the type stored in `KProcess.cpu_context`). Since `CurrentCpuContext`
// has no mock variant, `CurrentSignalContext` must also use the real arch
// implementation. This is safe because `SignalContext` methods are pure
// data transformations (no hardware access).
//
// `MockSignalContext` remains available for arch-crate-internal tests that
// don't need to interoperate with `KProcess`.
#[cfg(target_arch = "x86_64")]
pub type CurrentSignalContext = crate::x86_64::signal::X86_64SignalContext;
#[cfg(target_arch = "aarch64")]
pub type CurrentSignalContext = crate::arm64::signal::AArch64SignalContext;
#[cfg(target_arch = "riscv64")]
pub type CurrentSignalContext = crate::riscv64::signal::Riscv64SignalContext;

// ── CurrentArchInit type aliases ──
#[cfg(target_arch = "x86_64")]
pub type CurrentArchInit = crate::x86_64::arch_init::X86_64ArchInit;
#[cfg(target_arch = "aarch64")]
pub type CurrentArchInit = crate::arm64::arch_init::AArch64ArchInit;
#[cfg(target_arch = "riscv64")]
pub type CurrentArchInit = crate::riscv64::arch_init::Riscv64ArchInit;

// ── CurrentTimerIrqGate type aliases ──
//
// Selects the architecture-specific `TimerIrqGate` implementor at compile
// time. `TimerIrqGate` has only static methods (no instance state), so the
// selection follows the `CurrentArchInit` pattern (no mock variant).
#[cfg(target_arch = "x86_64")]
pub type CurrentTimerIrqGate = crate::x86_64::timer_irq_gate::X86_64TimerIrqGate;
#[cfg(target_arch = "aarch64")]
pub type CurrentTimerIrqGate = crate::arm64::timer_irq_gate::AArch64TimerIrqGate;
#[cfg(target_arch = "riscv64")]
pub type CurrentTimerIrqGate = crate::riscv64::timer_irq_gate::Riscv64TimerIrqGate;

// ── CurrentCpuContextArch / CpuContext / TrapFrame type aliases ──
//
// Replaces the old `CurrentBootProcArch` (see 06-proc-init-boot-proc.md §3.5
// for the renaming rationale — the abstraction is "process's CPU
// context", not just the boot phase).
#[cfg(target_arch = "x86_64")]
pub type CurrentCpuContextArch = crate::x86_64::boot::X86_64CpuContextArch;
#[cfg(target_arch = "aarch64")]
pub type CurrentCpuContextArch = crate::arm64::boot::AArch64CpuContextArch;
#[cfg(target_arch = "riscv64")]
pub type CurrentCpuContextArch = crate::riscv64::boot::Riscv64CpuContextArch;

#[cfg(target_arch = "x86_64")]
pub type CurrentCpuContext = crate::x86_64::boot::X86_64CpuContext;
#[cfg(target_arch = "aarch64")]
pub type CurrentCpuContext = crate::arm64::boot::AArch64CpuContext;
#[cfg(target_arch = "riscv64")]
pub type CurrentCpuContext = crate::riscv64::boot::Riscv64CpuContext;

#[cfg(target_arch = "x86_64")]
pub type CurrentTrapFrame = crate::x86_64::exception::X86_64ExceptionFrame;
#[cfg(target_arch = "aarch64")]
pub type CurrentTrapFrame = crate::arm64::exception::AArch64ExceptionFrame;
#[cfg(target_arch = "riscv64")]
pub type CurrentTrapFrame = crate::riscv64::exception::Riscv64ExceptionFrame;

// ── CurrentSmpArch type aliases ──
//
// Selects the architecture-specific `SmpArch` implementor at compile time.
// The kernel's SMP module (`os/kernel/src/smp.rs`) uses `CurrentSmpArch`
// as the generic type parameter, so no `#[cfg(target_arch)]` leaks into
// kernel code (D7 in 16-smp.md).
#[cfg(feature = "runtime-window")]
pub type CurrentSmpArch = crate::arch::smp::MockSmpArch;
#[cfg(all(not(feature = "runtime-window"), target_arch = "x86_64"))]
pub type CurrentSmpArch = crate::x86_64::smp::X86_64SmpArch;
#[cfg(all(not(feature = "runtime-window"), target_arch = "aarch64"))]
pub type CurrentSmpArch = crate::arm64::smp::AArch64SmpArch;
#[cfg(all(not(feature = "runtime-window"), target_arch = "riscv64"))]
pub type CurrentSmpArch = crate::riscv64::smp::Riscv64SmpArch;

// ── TlbArch re-exports + CurrentTlbArch type alias (FIX-24, Phase 5) ──
//
// `TlbArch` provides CPU-wide TLB invalidation (flush_all / flush_addr)
// as static methods, used by `dispatch_vmctl` for SVMCTL_FLUSHTLB and
// SVMCTL_INVLPG. Unlike `Paging::flush_tlb` (instance method), `TlbArch`
// operates on the *current* CPU's TLB without a Paging instance.
pub use tlb_arch::{mock_reset_active_root, MockTlbArch, TlbArch};

#[cfg(feature = "runtime-window")]
pub type CurrentTlbArch = MockTlbArch;
#[cfg(all(not(feature = "runtime-window"), target_arch = "x86_64"))]
pub type CurrentTlbArch = crate::x86_64::tlb::X86_64TlbArch;
#[cfg(all(not(feature = "runtime-window"), target_arch = "aarch64"))]
pub type CurrentTlbArch = crate::arm64::tlb::AArch64TlbArch;
#[cfg(all(not(feature = "runtime-window"), target_arch = "riscv64"))]
pub type CurrentTlbArch = crate::riscv64::tlb::Riscv64TlbArch;

// ── CurrentTrapReturnArch type alias ──
//
// Selects the architecture-specific `TrapReturnArch` implementor (the
// return-path twin of `CurrentTrapEntry`). Mock build uses `MockTrapReturn`
// (panics on dispatch — see `arch/trap_return.rs`); tests exercise the
// scheduling stages, never the real mode switch.
#[cfg(feature = "runtime-window")]
pub type CurrentTrapReturnArch = crate::arch::trap_return::MockTrapReturn;
#[cfg(all(not(feature = "runtime-window"), target_arch = "x86_64"))]
pub type CurrentTrapReturnArch = crate::x86_64::trap_return::X86_64TrapReturn;
#[cfg(all(not(feature = "runtime-window"), target_arch = "aarch64"))]
pub type CurrentTrapReturnArch = crate::arm64::trap_return::AArch64TrapReturn;
#[cfg(all(not(feature = "runtime-window"), target_arch = "riscv64"))]
pub type CurrentTrapReturnArch = crate::riscv64::trap_return::Riscv64TrapReturn;

// ── CurrentStacktraceArch type aliases ──
//
// Selects the architecture-specific `StacktraceArch` implementor at compile
// time. Used by `proc_stacktrace` in the kernel's diagnostic path
// (SYS_DIAGCTL DIAGCTL_CODE_STACKTRACE). See `arch/stacktrace.rs`.
//
// Same pattern as `CurrentCpuContextArch` — no mock variant because
// `StacktraceArch: CpuContextArch` and tests run on the host arch.
#[cfg(target_arch = "x86_64")]
pub type CurrentStacktraceArch = crate::x86_64::boot::X86_64CpuContextArch;
#[cfg(target_arch = "aarch64")]
pub type CurrentStacktraceArch = crate::arm64::boot::AArch64CpuContextArch;
#[cfg(target_arch = "riscv64")]
pub type CurrentStacktraceArch = crate::riscv64::boot::Riscv64CpuContextArch;
