//! SMP (Symmetric Multi-Processing) architecture abstraction.
//!
//! Defines the `SmpArch` trait for hardware-specific SMP operations:
//! inter-processor interrupts (IPI), CPU halting, IPI acknowledgement,
//! and application processor (AP) boot.
//!
//! # Design decisions (see 16-smp.md §3 D7)
//!
//! - **Static dispatch via associated functions**: Each architecture provides
//!   a zero-sized type (e.g. `X86_64SmpArch`) that implements `SmpArch`.
//!   Callers parameterize over `<A: SmpArch>` so the compiler monomorphizes
//!   every call site — zero vtable overhead on the IPI hot path.
//! - **No `#[cfg(target_arch)]` in kernel code**: The kernel's SMP module
//!   (`os/kernel/src/smp.rs`) depends only on the trait; the concrete type
//!   alias `CurrentSmpArch` selects the right backend at compile time.
//!
//! # Architecture mapping
//!
//! | Method | x86-64 | ARM64 | RISC-V |
//! |--------|--------|-------|--------|
//! | `send_sched_ipi` | LAPIC ICR write | GIC GICD_SGIR write | SBI `send_ipi` ecall |
//! | `halt_cpu` | `hlt` instruction | `wfi` instruction | `wfi` instruction |
//! | `ack_ipi` | LAPIC EOI write | GIC ICC_EOIR write | (SBI handles ack) |
//! | `boot_ap` | INIT + SIPI via ICR | PSCI `CPU_ON` | SBI HSM extension |
//!
//! C: `arch_send_smp_schedule_ipi()` / `arch_smp_halt_cpu()` / `ipi_ack()`
//!    — smp.c:58-65, arch/i386/smp.c

/// Architecture abstraction for SMP hardware operations.
///
/// Each architecture provides a zero-sized implementor type. All methods are
/// associated functions (no `&self`) because:
///
/// 1. The hardware base addresses (LAPIC MMIO, GIC distributor) are fixed
///    at boot and do not vary per call — they are stored in a per-arch
///    static set during `arch_init`.
/// 2. Static dispatch via generics (`<A: SmpArch>`) lets the compiler
///    inline and eliminate the call entirely, which matters on the IPI
///    hot path (every scheduler preemption).
///
/// C: arch-specific functions called from smp.c:
/// - `arch_send_smp_schedule_ipi(cpu)` — smp.c:65
/// - `arch_smp_halt_cpu()` — smp.c:60
/// - `ipi_ack()` — smp.c:58,198
/// - AP boot protocol — arch/i386/smp.c
pub trait SmpArch {
    /// Send a schedule IPI to the target CPU.
    ///
    /// C: `arch_send_smp_schedule_ipi(cpu)` — smp.c:65
    ///
    /// x86_64: write APIC ICR (Interrupt Command Register)
    /// aarch64: write GIC GICD_SGIR (Software Generated Interrupt Register)
    /// riscv64: SBI `send_ipi` ecall (hart mask)
    fn send_sched_ipi(cpu: u32);

    /// Halt the current CPU (called by smp_ipi_halt_handler).
    ///
    /// C: `arch_smp_halt_cpu()` — smp.c:60
    ///
    /// x86_64: `hlt` instruction
    /// aarch64: `wfi` instruction
    /// riscv64: `wfi` instruction
    fn halt_cpu();

    /// Enable interrupts, then halt until the next interrupt arrives.
    ///
    /// This is the **idle-loop** halt — distinct from [`SmpArch::halt_cpu`]
    /// (the IPI-halt variant, called from interrupt context where the
    /// interrupt flag is already managed by the entry path).
    ///
    /// C: `halt_cpu()` — klib.S:407-414 (`sti; hlt`). The STI is essential:
    /// the scheduler enters `idle()` with interrupts disabled (kernel
    /// context), and `hlt` sleeps until an interrupt — without enabling
    /// first, the CPU would sleep forever. The pairing also guarantees
    /// exactly one interrupt is taken after the halt: the IRET of that
    /// interrupt restores the pre-halt RFLAGS (IF=1 only up to the next
    /// `cli`), so the handler runs with IF cleared (klib.S comment:
    /// "interrupt handlers make sure that the interrupts are disabled when
    /// we get here").
    ///
    /// x86_64: `sti; hlt`
    /// aarch64: `msr daifclr, #2` (unmask IRQ) + `wfi`
    /// riscv64: `csrs sstatus, SIE` + `wfi`
    fn idle_halt();

    /// Acknowledge an IPI.
    ///
    /// C: `ipi_ack()` — smp.c:58,198
    ///
    /// x86_64: write LAPIC EOI register
    /// aarch64: write GIC ICC_EOIR (End of Interrupt Register)
    /// riscv64: no-op (SBI handles acknowledgement internally)
    fn ack_ipi();

    /// Boot an Application Processor (AP).
    ///
    /// C: arch/i386/smp.c — INIT + SIPI protocol
    ///
    /// x86_64: Send INIT IPI, wait, send SIPI with start vector
    /// aarch64: PSCI `CPU_ON` call (SMC/HVC)
    /// riscv64: SBI HSM extension (`hart_start`)
    ///
    /// # Arguments
    /// * `cpu` - AP CPU ID to boot
    /// * `entry` - Physical address of AP entry point (trampoline)
    fn boot_ap(cpu: u32, entry: usize);

    /// Pause the CPU in a busy-wait loop (hint to CPU, not a trap).
    /// C: `arch_pause()` — smp.c:46
    ///
    /// All architectures use `core::hint::spin_loop()` (unified).
    fn pause() {
        core::hint::spin_loop();
    }

    /// Return the current CPU's ID.
    ///
    /// C: `cpuid` macro — arch/i386/include/arch_smp.h:11.
    ///
    /// C reads the CPU ID from the top of the current kernel stack (each
    /// CPU has a private kernel stack; the ID is stored at the stack's
    /// highest address). Rust uses per-architecture mechanisms:
    ///
    /// - x86_64: read from GS segment base (per-CPU GSBASE stores CPU ID)
    /// - aarch64: read from TPIDR_EL1 (per-CPU thread pointer)
    /// - riscv64: read from CSR `sscratch` (per-CPU scratch register)
    ///
    /// Returns 0 (BSP) if SMP is not yet initialized or the architecture
    /// does not support SMP.
    fn current_cpu() -> u32;
}

/// Mock `SmpArch` implementor for unit tests and `feature = "mock"` builds.
///
/// All methods are no-ops — sufficient for testing SMP scheduling logic
/// without real hardware. The kernel's test suite (`smp.rs` tests module)
/// uses this type as the generic parameter `<A: SmpArch>`.
#[cfg(feature = "mock")]
#[derive(Debug, Clone, Copy)]
pub struct MockSmpArch;

#[cfg(feature = "mock")]
impl SmpArch for MockSmpArch {
    fn send_sched_ipi(_cpu: u32) { /* no-op for test */ }
    fn halt_cpu() { /* no-op for test */ }
    fn idle_halt() { /* no-op for test */ }
    fn ack_ipi() { /* no-op for test */ }
    fn boot_ap(_cpu: u32, _entry: usize) { /* no-op for test */ }
    fn current_cpu() -> u32 { 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify the trait can be implemented for a zero-sized type.
    #[test]
    fn test_trait_object_safety() {
        struct DummyArch;
        impl SmpArch for DummyArch {
            fn send_sched_ipi(_cpu: u32) {}
            fn halt_cpu() {}
            fn idle_halt() {}
            fn ack_ipi() {}
            fn boot_ap(_cpu: u32, _entry: usize) {}
            fn current_cpu() -> u32 { 0 }
        }
        // Verify associated functions are callable.
        DummyArch::send_sched_ipi(0);
        DummyArch::ack_ipi();
        DummyArch::halt_cpu();
        DummyArch::boot_ap(0, 0);
        DummyArch::pause();
    }

    /// Verify MockSmpArch implements SmpArch.
    #[cfg(feature = "mock")]
    #[test]
    fn test_mock_smp_arch_impl() {
        MockSmpArch::send_sched_ipi(1);
        MockSmpArch::ack_ipi();
        MockSmpArch::halt_cpu();
        MockSmpArch::boot_ap(1, 0x1000);
        MockSmpArch::pause();
    }
}
