//! The aggregate architecture description: `trait Arch` + `CurrentArch`.
//!
//! # Why this module exists (E1)
//!
//! The crate exposes ~20 fine-grained mechanism traits (paging, clock, TLB,
//! FPU, SMP, …), each resolved to a concrete implementor by a `Current*`
//! type-alias family in the crate root. That mechanism works and is kept,
//! but the "which architecture am I running on" concept is implicit — it
//! only exists as 20 independent `cfg` selections that happen to agree.
//! `trait Arch` makes the architecture instance an explicit, named thing:
//! one trait, one `CurrentArch` anchor, every mechanism family hanging off
//! it as an associated type.
//!
//! # Why associated types, not supertrait bounds
//!
//! An earlier sketch proposed `trait Arch: Paging + ClockArch + … {}`. That
//! form does not fit this crate's reality:
//!
//! 1. **Handle families are stateful.** `Paging` wraps an active page-table
//!    root (`Paging::from_active_root`), `ClockArch::new` builds a per-CPU
//!    timer handle, `ArchInit::new` a per-CPU init handle. They cannot be
//!    implemented by a unit `X86_64Arch` struct without faking state.
//! 2. **Supertrait bounds force forwarding impls.** `X86_64Arch: SmpArch`
//!    would need `impl SmpArch for X86_64Arch { fn send_sched_ipi(…) {
//!    X86_64SmpArch::send_sched_ipi(…) } }` for every static trait on every
//!    arch — a second dispatch layer that can drift from the real one.
//!
//! With associated types, each family keeps its original implementor and
//! its original dispatch path; `Arch` is the bundle that names them. This
//! mirrors how Rust platforms compose capability families (one aggregate
//! trait whose associated types are the per-family HAL handles).
//!
//! # Selection matrix
//!
//! `CurrentArch` mirrors the crate-root `Current*` aliases exactly: under
//! the `mock` feature (host tests), mock families select the `Mock*`
//! implementors while the hardware-touch-free families (CPU context,
//! signal context, exception frames, timer IRQ gate, arch init) select the
//! host-arch implementors — same as the crate-root aliases. The test at the
//! bottom pins this agreement by `TypeId` comparison.
//!
//! # Adoption policy (E1 OQ ruling)
//!
//! The fine-grained `Current*` aliases remain the dispatch mechanism for
//! existing call sites; both styles may coexist long-term. New kernel-side
//! code (S-4/S-5 SMP work onward) should prefer `CurrentArch` as the
//! single anchor.

use crate::arch::{
    arch_init::ArchInit, boot::CpuContextArch, clock::ClockArch, cpu_identity::CpuIdentityArch,
    direct_map::DirectMapArch, dm_coverage::DmCoverageArch, exception::ExceptionArch,
    fpu_arch::FpuArch, paging::Paging, pte_walk_arch::PteWalkArch, protection::ProtectionArch,
    signal_context::SignalContext, smp::SmpArch, stacktrace::StacktraceArch,
    timer_irq_gate::TimerIrqGate, tlb_arch::TlbArch, trap_entry::TrapEntryArch,
    trap_return::TrapReturnArch,
};

/// The aggregate architecture description.
///
/// One impl per architecture (plus one for the mock build). Each
/// associated type names the implementor of one mechanism family; the
/// trait bounds state what a family must provide. No methods of its own —
/// dispatch goes through the family traits as before.
pub trait Arch: Sized {
    /// CPU context construction + stack unwinding (one implementor serves
    /// both, mirroring `CurrentCpuContextArch` == `CurrentStacktraceArch`).
    type CpuCtx: CpuContextArch + StacktraceArch;
    /// Exception/ trap frame introspection (`Frame` = the crate-root
    /// `CurrentTrapFrame`).
    type Exception: ExceptionArch;
    /// Inter-processor interrupts and CPU halt.
    type Smp: SmpArch;
    /// CPU-wide TLB invalidation (the `Paging`-independent flush path).
    type Tlb: TlbArch;
    /// "Which CPU am I running on" probe.
    type CpuId: CpuIdentityArch;
    /// Unmasking/masking the timer IRQ at the arch boundary.
    type IrqGate: TimerIrqGate;
    /// Direct-map window constants and PA↔VA translations.
    type Dm: DirectMapArch;

    /// Privilege-level description and kernel/user stack initialisation.
    type Protection: ProtectionArch;
    /// Trap/syscall entry-point configuration handle.
    type TrapEntry: TrapEntryArch;
    /// Return-to-usermode path handle.
    type TrapReturn: TrapReturnArch;
    /// Per-CPU timer handle (ticks, one-shot, deadline).
    type Clock: ClockArch;
    /// FPU enablement and context save/restore (`State` = the crate-root
    /// `CurrentFpuState`).
    type Fpu: FpuArch;
    /// Signal frame construction for user-space signal delivery.
    type Signal: SignalContext;
    /// Per-CPU architecture init handle (GDT/TSS/GS_BASE family).
    type Init: ArchInit;

    /// Page-table service (map/unmap/query on an active root).
    type Paging: Paging;
    /// Software PTE walk over an established tree.
    type PteWalk: PteWalkArch;
    /// Direct-map coverage establishment on the bootstrap root.
    type DmCoverage: DmCoverageArch;
}

/// x86-64 architecture bundle. Exists only on `target_arch = "x86_64"`
/// (the underlying modules are gated the same way).
#[cfg(target_arch = "x86_64")]
pub struct X86_64Arch;

#[cfg(target_arch = "x86_64")]
impl Arch for X86_64Arch {
    type CpuCtx = crate::x86_64::boot::X86_64CpuContextArch;
    type Exception = crate::x86_64::exception::X86_64ExceptionFrame;
    type Smp = crate::x86_64::smp::X86_64SmpArch;
    type Tlb = crate::x86_64::tlb::X86_64TlbArch;
    type CpuId = crate::x86_64::cpu_identity::X86_64CpuIdentity;
    type IrqGate = crate::x86_64::timer_irq_gate::X86_64TimerIrqGate;
    type Dm = crate::arch::direct_map::X86_64DirectMap;
    type Protection = crate::x86_64::protection::X86_64Protection;
    type TrapEntry = crate::x86_64::trap_entry::X86_64TrapEntry;
    type TrapReturn = crate::x86_64::trap_return::X86_64TrapReturn;
    type Clock = crate::x86_64::clock::X86_64ClockArch;
    type Fpu = crate::x86_64::fpu::X86_64FpuArch;
    type Signal = crate::x86_64::signal::X86_64SignalContext;
    type Init = crate::x86_64::arch_init::X86_64ArchInit;
    type Paging = crate::x86_64::paging::X86_64Paging;
    type PteWalk = crate::x86_64::paging::X86_64PteWalk;
    type DmCoverage = crate::x86_64::paging::X86_64DmCoverage;
}

/// AArch64 architecture bundle.
#[cfg(target_arch = "aarch64")]
pub struct AArch64Arch;

#[cfg(target_arch = "aarch64")]
impl Arch for AArch64Arch {
    type CpuCtx = crate::arm64::boot::AArch64CpuContextArch;
    type Exception = crate::arm64::exception::AArch64ExceptionFrame;
    type Smp = crate::arm64::smp::AArch64SmpArch;
    type Tlb = crate::arm64::tlb::AArch64TlbArch;
    type CpuId = crate::arm64::cpu_identity::AArch64CpuIdentity;
    type IrqGate = crate::arm64::timer_irq_gate::AArch64TimerIrqGate;
    type Dm = crate::arch::direct_map::AArch64DirectMap;
    type Protection = crate::arm64::protection::AArch64Protection;
    type TrapEntry = crate::arm64::trap_entry::AArch64TrapEntry;
    type TrapReturn = crate::arm64::trap_return::AArch64TrapReturn;
    type Clock = crate::arm64::clock::AArch64ClockArch;
    type Fpu = crate::arm64::fpu::AArch64FpuArch;
    type Signal = crate::arm64::signal::AArch64SignalContext;
    type Init = crate::arm64::arch_init::AArch64ArchInit;
    type Paging = crate::arm64::paging::AArch64Paging;
    type PteWalk = crate::arm64::paging::AArch64PteWalk;
    type DmCoverage = crate::arm64::paging::AArch64DmCoverage;
}

/// RISC-V (RV64) architecture bundle.
#[cfg(target_arch = "riscv64")]
pub struct Riscv64Arch;

#[cfg(target_arch = "riscv64")]
impl Arch for Riscv64Arch {
    type CpuCtx = crate::riscv64::boot::Riscv64CpuContextArch;
    type Exception = crate::riscv64::exception::Riscv64ExceptionFrame;
    type Smp = crate::riscv64::smp::Riscv64SmpArch;
    type Tlb = crate::riscv64::tlb::Riscv64TlbArch;
    type CpuId = crate::riscv64::cpu_identity::Riscv64CpuIdentity;
    type IrqGate = crate::riscv64::timer_irq_gate::Riscv64TimerIrqGate;
    type Dm = crate::arch::direct_map::Riscv64DirectMap;
    type Protection = crate::riscv64::protection::Riscv64Protection;
    type TrapEntry = crate::riscv64::trap_entry::Riscv64TrapEntry;
    type TrapReturn = crate::riscv64::trap_return::Riscv64TrapReturn;
    type Clock = crate::riscv64::clock::Riscv64ClockArch;
    type Fpu = crate::riscv64::fpu::Riscv64FpuArch;
    type Signal = crate::riscv64::signal::Riscv64SignalContext;
    type Init = crate::riscv64::arch_init::Riscv64ArchInit;
    type Paging = crate::riscv64::paging::Riscv64Paging;
    type PteWalk = crate::riscv64::paging::Riscv64PteWalk;
    type DmCoverage = crate::riscv64::paging::Riscv64DmCoverage;
}

/// Mock bundle for host tests (the `mock` feature). Families with a mock
/// implementor select the mock; the hardware-touch-free families without
/// one (CPU context, exception frame, signal context, arch init, timer IRQ
/// gate) select the host-arch implementor — exactly like the crate-root
/// `Current*` aliases do under `mock`.
#[cfg(feature = "mock")]
pub struct MockArch;

#[cfg(all(feature = "mock", target_arch = "x86_64"))]
impl Arch for MockArch {
    type CpuCtx = crate::x86_64::boot::X86_64CpuContextArch;
    type Exception = crate::x86_64::exception::X86_64ExceptionFrame;
    type Signal = crate::x86_64::signal::X86_64SignalContext;
    type Init = crate::x86_64::arch_init::X86_64ArchInit;
    type IrqGate = crate::x86_64::timer_irq_gate::X86_64TimerIrqGate;
    type Smp = crate::arch::smp::MockSmpArch;
    type Tlb = crate::arch::tlb_arch::MockTlbArch;
    type CpuId = crate::arch::cpu_identity::mock::MockCpuIdentity;
    type Dm = crate::arch::direct_map::MockDirectMap;
    type Protection = crate::arch::protection::MockProtection;
    type TrapEntry = crate::arch::trap_entry::MockTrapEntry;
    type TrapReturn = crate::arch::trap_return::MockTrapReturn;
    type Clock = crate::arch::clock::MockClockArch;
    type Fpu = crate::arch::fpu_arch::MockFpuArch;
    type Paging = crate::arch::paging::mock::MockPaging;
    type PteWalk = crate::arch::pte_walk_arch::MockPteWalk;
    type DmCoverage = crate::arch::dm_coverage::mock::MockDmCoverage;
}

#[cfg(all(feature = "mock", target_arch = "aarch64"))]
impl Arch for MockArch {
    type CpuCtx = crate::arm64::boot::AArch64CpuContextArch;
    type Exception = crate::arm64::exception::AArch64ExceptionFrame;
    type Signal = crate::arm64::signal::AArch64SignalContext;
    type Init = crate::arm64::arch_init::AArch64ArchInit;
    type IrqGate = crate::arm64::timer_irq_gate::AArch64TimerIrqGate;
    type Smp = crate::arch::smp::MockSmpArch;
    type Tlb = crate::arch::tlb_arch::MockTlbArch;
    type CpuId = crate::arch::cpu_identity::mock::MockCpuIdentity;
    type Dm = crate::arch::direct_map::MockDirectMap;
    type Protection = crate::arch::protection::MockProtection;
    type TrapEntry = crate::arch::trap_entry::MockTrapEntry;
    type TrapReturn = crate::arch::trap_return::MockTrapReturn;
    type Clock = crate::arch::clock::MockClockArch;
    type Fpu = crate::arch::fpu_arch::MockFpuArch;
    type Paging = crate::arch::paging::mock::MockPaging;
    type PteWalk = crate::arch::pte_walk_arch::MockPteWalk;
    type DmCoverage = crate::arch::dm_coverage::mock::MockDmCoverage;
}

#[cfg(all(feature = "mock", target_arch = "riscv64"))]
impl Arch for MockArch {
    type CpuCtx = crate::riscv64::boot::Riscv64CpuContextArch;
    type Exception = crate::riscv64::exception::Riscv64ExceptionFrame;
    type Signal = crate::riscv64::signal::Riscv64SignalContext;
    type Init = crate::riscv64::arch_init::Riscv64ArchInit;
    type IrqGate = crate::riscv64::timer_irq_gate::Riscv64TimerIrqGate;
    type Smp = crate::arch::smp::MockSmpArch;
    type Tlb = crate::arch::tlb_arch::MockTlbArch;
    type CpuId = crate::arch::cpu_identity::mock::MockCpuIdentity;
    type Dm = crate::arch::direct_map::MockDirectMap;
    type Protection = crate::arch::protection::MockProtection;
    type TrapEntry = crate::arch::trap_entry::MockTrapEntry;
    type TrapReturn = crate::arch::trap_return::MockTrapReturn;
    type Clock = crate::arch::clock::MockClockArch;
    type Fpu = crate::arch::fpu_arch::MockFpuArch;
    type Paging = crate::arch::paging::mock::MockPaging;
    type PteWalk = crate::arch::pte_walk_arch::MockPteWalk;
    type DmCoverage = crate::arch::dm_coverage::mock::MockDmCoverage;
}

/// The currently-compiling architecture bundle — the single `cfg` anchor.
///
/// Under the `mock` feature (host tests) this is [`MockArch`]; real builds
/// select by `target_arch`, mirroring the crate-root `Current*` aliases.
#[cfg(feature = "mock")]
pub type CurrentArch = MockArch;
#[cfg(all(not(feature = "mock"), target_arch = "x86_64"))]
pub type CurrentArch = X86_64Arch;
#[cfg(all(not(feature = "mock"), target_arch = "aarch64"))]
pub type CurrentArch = AArch64Arch;
#[cfg(all(not(feature = "mock"), target_arch = "riscv64"))]
pub type CurrentArch = Riscv64Arch;

#[cfg(test)]
mod tests {
    use super::*;
    use core::any::TypeId;

    fn tid<T: 'static>() -> TypeId {
        TypeId::of::<T>()
    }

    #[test]
    fn test_current_arch_implements_arch() {
        fn assert_arch<A: Arch>() {}
        assert_arch::<CurrentArch>();
    }

    // The aggregate is only as good as its agreement with the crate-root
    // `Current*` aliases. Each assertion pins one family: if someone
    // rewires the aggregate or an alias independently, this test breaks.
    #[test]
    fn test_arch_families_match_current_alias_selections() {
        assert_eq!(tid::<<CurrentArch as Arch>::CpuCtx>(), tid::<crate::CurrentCpuContextArch>());
        assert_eq!(
            tid::<<CurrentArch as Arch>::Exception>(),
            tid::<crate::CurrentTrapFrame>()
        );
        assert_eq!(tid::<<CurrentArch as Arch>::Smp>(), tid::<crate::CurrentSmpArch>());
        assert_eq!(tid::<<CurrentArch as Arch>::Tlb>(), tid::<crate::CurrentTlbArch>());
        assert_eq!(
            tid::<<CurrentArch as Arch>::CpuId>(),
            tid::<crate::CurrentCpuIdentity>()
        );
        assert_eq!(
            tid::<<CurrentArch as Arch>::IrqGate>(),
            tid::<crate::CurrentTimerIrqGate>()
        );
        assert_eq!(tid::<<CurrentArch as Arch>::Dm>(), tid::<crate::CurrentDirectMap>());
        assert_eq!(
            tid::<<CurrentArch as Arch>::Protection>(),
            tid::<crate::CurrentProtection>()
        );
        assert_eq!(
            tid::<<CurrentArch as Arch>::TrapEntry>(),
            tid::<crate::CurrentTrapEntry>()
        );
        assert_eq!(
            tid::<<CurrentArch as Arch>::TrapReturn>(),
            tid::<crate::CurrentTrapReturnArch>()
        );
        assert_eq!(tid::<<CurrentArch as Arch>::Clock>(), tid::<crate::CurrentClockArch>());
        assert_eq!(tid::<<CurrentArch as Arch>::Fpu>(), tid::<crate::CurrentFpuArch>());
        assert_eq!(
            tid::<<CurrentArch as Arch>::Signal>(),
            tid::<crate::CurrentSignalContext>()
        );
        assert_eq!(tid::<<CurrentArch as Arch>::Init>(), tid::<crate::CurrentArchInit>());
        assert_eq!(tid::<<CurrentArch as Arch>::Paging>(), tid::<crate::CurrentPaging>());
        assert_eq!(tid::<<CurrentArch as Arch>::PteWalk>(), tid::<crate::CurrentPteWalk>());
        assert_eq!(
            tid::<<CurrentArch as Arch>::DmCoverage>(),
            tid::<crate::CurrentDmCoverage>()
        );
    }

    #[test]
    fn test_derived_state_types_match_alias_selections() {
        // `CurrentFpuState` is documented as the `State` of `CurrentFpuArch`;
        // the aggregate exposes the same type through `Arch::Fpu`.
        type CurrFpuState = <<CurrentArch as Arch>::Fpu as FpuArch>::State;
        assert_eq!(tid::<CurrFpuState>(), tid::<crate::CurrentFpuState>());
        // `CurrentCpuContext` is the `CpuContext` associated type of
        // `CurrentCpuContextArch`; the aggregate reaches it via `CpuCtx`.
        type CurrCpuContext = <<CurrentArch as Arch>::CpuCtx as CpuContextArch>::CpuContext;
        assert_eq!(tid::<CurrCpuContext>(), tid::<crate::CurrentCpuContext>());
    }
}
