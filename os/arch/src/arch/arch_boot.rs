//! Boot-time architecture abstraction
//!
//! Encapsulates the arch-specific work that happens after `ClockArch::init_timer`
//! but before the timer IRQ can be serviced. Specifically, this trait gives the
//! kernel a way to:
//! 1. **Register the BSP's timer handler**: the handler function called when
//!    the local timer IRQ fires. On x86-64 this is registered into the IOAPIC
//!    as IRQ 0; on aarch64 / riscv64 it is the per-CPU timer vector.
//! 2. **Mask/unmask the timer IRQ line**: the kernel may want to disable
//!    timer interrupts temporarily (e.g., during boot before the scheduler
//!    is ready).
//!
//! # C source path
//!
//! ```c
//! // clock.c:294 — boot_cpu_init_timer
//! int boot_cpu_init_timer(int freq) {
//!     ...
//!     if (register_local_timer_handler(timer_int_handler) != OK)
//!         return EINVAL;
//!     ...
//! }
//!
//! // clock.c:177 — register_local_timer_handler
//! int register_local_timer_handler(irq_handler_t handler) {
//!     // Architecture-specific; binds handler to IRQ 0 / vector offset
//! }
//! ```
//!
//! # Rust design
//!
//! We separate "what handler is registered" from "how it is registered":
//! - `ClockArch::init_timer(hz)` configures the hardware (already done in
//!   the existing trait).
//! - `ArchBoot::register_timer_handler` binds a handler function pointer
//!   to the hardware vector. This is **the missing piece** in the Rust
//!   rewrite — the kernel needs a trait method to tell the architecture
//!   "here is the function to call when the timer fires".
//!
//! The handler signature is fixed at `fn(IrqVector, IrqId) -> IrqAction`
//! to match the kernel's `IrqManager::register_hook` signature (this is
//! the function pointer type the kernel's IRQ chain expects).

use minix_types::Endpoint;

/// Re-export of the IRQ handler signature used by `IrqManager`.
///
/// Kept here as a stable alias so callers do not need to depend on the
/// kernel crate to write ArchBoot impls.
pub type TimerHandlerFn = fn(
    irq: minix_plat::IrqVector,
    id: minix_plat::IrqId,
) -> minix_plat::IrqAction;

/// Boot-time architecture interface: register the BSP's timer handler
/// and optionally mask the timer IRQ.
///
/// C: `boot_cpu_init_timer` + `register_local_timer_handler`
/// (clock.c:294 / 177) on x86, equivalent on aarch64 / riscv64.
pub trait ArchBoot {
    /// Register the kernel's timer interrupt handler.
    ///
    /// # Arguments
    ///
    /// * `handler` — function pointer to call when the timer IRQ fires.
    ///
    /// # Returns
    ///
    /// `Ok(())` on success, `Err(())` if the architecture cannot register
    /// the handler (e.g., the IRQ line is in use by another source).
    ///
    /// # Implementation note
    ///
    /// Real architectures would call into arch-specific IRQ binding
    /// (e.g., `irq_bind_local_timer` on x86, or PMP setup on RISC-V).
    /// The mock implementation records the handler for inspection by
    /// tests.
    fn register_timer_handler(handler: TimerHandlerFn) -> Result<(), BootError>;

    /// Unmask the timer IRQ line so timer interrupts are delivered.
    ///
    /// Called after `register_timer_handler` to enable timer delivery.
    fn enable_timer_irq();

    /// Mask the timer IRQ line.
    ///
    /// Used during critical sections where timer delivery is undesirable.
    fn disable_timer_irq();
}

/// Errors that `ArchBoot` can return.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootError {
    /// The handler could not be registered (IRQ busy, missing slot, etc.).
    RegistrationFailed,
    /// The architecture does not support the operation.
    Unsupported,
}

// ── Mock implementation ────────────────────────────────────────────────────
//
// Used in test environments (no QEMU). Records the most recent handler
// for inspection.

use core::sync::atomic::{AtomicBool, AtomicPtr, Ordering};

/// Whether the mock has a registered handler.
static MOCK_HAS_HANDLER: AtomicBool = AtomicBool::new(false);

/// Most recent handler pointer (as usize to fit in AtomicPtr).
static MOCK_LAST_HANDLER: AtomicPtr<()> = AtomicPtr::new(core::ptr::null_mut());

/// Whether the timer IRQ is enabled in the mock.
static MOCK_IRQ_ENABLED: AtomicBool = AtomicBool::new(false);

/// Mock `ArchBoot` for unit tests.
pub struct MockArchBoot;

impl ArchBoot for MockArchBoot {
    fn register_timer_handler(_handler: TimerHandlerFn) -> Result<(), BootError> {
        // Record the handler for test inspection.
        MOCK_HAS_HANDLER.store(true, Ordering::Release);
        MOCK_LAST_HANDLER.store(
            _handler as *const () as *mut (),
            Ordering::Release,
        );
        Ok(())
    }

    fn enable_timer_irq() {
        MOCK_IRQ_ENABLED.store(true, Ordering::Release);
    }

    fn disable_timer_irq() {
        MOCK_IRQ_ENABLED.store(false, Ordering::Release);
    }
}

/// Test helper: was a handler registered?
#[cfg(test)]
pub fn mock_has_handler() -> bool {
    MOCK_HAS_HANDLER.load(Ordering::Acquire)
}

/// Test helper: is the mock timer IRQ enabled?
#[cfg(test)]
pub fn mock_irq_enabled() -> bool {
    MOCK_IRQ_ENABLED.load(Ordering::Acquire)
}

/// Test helper: reset the mock state (call at the start of each test).
#[cfg(test)]
pub fn mock_arch_boot_reset() {
    MOCK_HAS_HANDLER.store(false, Ordering::Release);
    MOCK_LAST_HANDLER.store(core::ptr::null_mut(), Ordering::Release);
    MOCK_IRQ_ENABLED.store(false, Ordering::Release);
}

// ── x86_64 implementation ──────────────────────────────────────────────────
//
// For x86-64, the actual timer handler registration goes through the
// IOAPIC + Local APIC, which is implemented in arch-specific code (the
// `x86_64` module in the kernel crate). The `ArchBoot::register_timer_handler`
// here is a thin wrapper that records the handler for later binding when
// the IOAPIC setup runs. Real IRQ delivery happens via the assembly
// trap entry (`trap_entry.rs`).

#[cfg(target_arch = "x86_64")]
pub struct X86_64ArchBoot;

#[cfg(target_arch = "x86_64")]
impl ArchBoot for X86_64ArchBoot {
    fn register_timer_handler(_handler: TimerHandlerFn) -> Result<(), BootError> {
        // In the real kernel build, this would invoke `irq_bind_local_timer`
        // which programs the IOAPIC's RTE for IRQ 0 (or the LAPIC's LVT
        // timer entry). For now we record the handler and let the trap
        // entry invoke it directly.
        MockArchBoot::register_timer_handler(_handler)
    }

    fn enable_timer_irq() {
        MockArchBoot::enable_timer_irq()
    }

    fn disable_timer_irq() {
        MockArchBoot::disable_timer_irq()
    }
}

/// Compile-time alias for the current architecture's `ArchBoot` impl.
///
/// Defaults to `MockArchBoot` in test mode and `X86_64ArchBoot` for
/// x86_64 production builds.
#[cfg(target_arch = "x86_64")]
pub type CurrentArchBoot = X86_64ArchBoot;

#[cfg(not(target_arch = "x86_64"))]
pub type CurrentArchBoot = MockArchBoot;

/// Convenience wrapper: register a handler and unmask the IRQ in one call.
///
/// # C source
///
/// `boot_cpu_init_timer` does both:
///
/// ```c
/// boot_cpu_init_timer(int freq) {
///     ...
///     if (register_local_timer_handler(timer_int_handler) != OK)
///         return EINVAL;
///     return OK;
/// }
/// ```
///
/// In the Rust rewrite, the architecture binding (register_handler) and
/// the IRQ unmasking (enable_timer_irq) are split so the kernel can
/// control when interrupts start. We provide a convenience function that
/// does both.
pub fn boot_init_timer<AB: ArchBoot>(handler: TimerHandlerFn) -> Result<(), BootError> {
    AB::register_timer_handler(handler)?;
    AB::enable_timer_irq();
    Ok(())
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_timer_handler(
        _irq: minix_plat::IrqVector,
        _id: minix_plat::IrqId,
    ) -> minix_plat::IrqAction {
        minix_plat::IrqAction::Completed
    }

    #[test]
    fn test_mock_register_timer_handler() {
        mock_arch_boot_reset();
        let result = <MockArchBoot as ArchBoot>::register_timer_handler(dummy_timer_handler);
        assert!(result.is_ok());
        assert!(mock_has_handler());
    }

    #[test]
    fn test_mock_enable_disable_timer_irq() {
        mock_arch_boot_reset();
        assert!(!mock_irq_enabled());
        <MockArchBoot as ArchBoot>::enable_timer_irq();
        assert!(mock_irq_enabled());
        <MockArchBoot as ArchBoot>::disable_timer_irq();
        assert!(!mock_irq_enabled());
    }

    #[test]
    fn test_boot_init_timer_runs_both() {
        mock_arch_boot_reset();
        let result = boot_init_timer::<MockArchBoot>(dummy_timer_handler);
        assert!(result.is_ok());
        assert!(mock_has_handler());
        assert!(mock_irq_enabled());
    }

    #[test]
    fn test_current_arch_boot_is_mock_or_x86() {
        // Compile-time check: CurrentArchBoot is either MockArchBoot or
        // X86_64ArchBoot depending on the target arch.
        fn _check_works<AB: ArchBoot>() {}
        _check_works::<CurrentArchBoot>();
    }

    #[test]
    fn test_boot_error_variants() {
        // Just verify the enum variants exist and are comparable.
        let _ = BootError::RegistrationFailed;
        let _ = BootError::Unsupported;
        assert_ne!(BootError::RegistrationFailed, BootError::Unsupported);
    }
}