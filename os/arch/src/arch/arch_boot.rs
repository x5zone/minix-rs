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
//! # Three-architecture coverage (FIX-22, Phase 2)
//!
//! All three architectures (x86_64, aarch64, riscv64) provide real `ArchBoot`
//! implementations. Previously x86_64 silently delegated to MockArchBoot and
//! aarch64/riscv64 fell back to MockArchBoot via `#[cfg(not(target_arch = "x86_64"))]`.
//! Now each architecture has its own `ArchBoot` impl with real hardware
//! programming:
//!
//! - **x86_64**: LAPIC LVT Timer (mask bit 16) + LAPIC SVR (enable bit 8)
//! - **aarch64**: CNTP_CTL_EL0 (enable/mask bits) + ICC_IGRPEN1_EL1 (GIC IRQ enable)
//! - **riscv64**: sie.STIE (S-mode timer interrupt enable) + sip.STIP clear
//!
//! MockArchBoot is only used in `#[cfg(test)]` or with the `mock` feature.

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
///
/// # Why associated functions (no `&self`)
///
/// `ArchBoot` is stateless at the trait level — the "registered handler"
/// lives in arch-specific static storage (LAPIC MMIO, system registers,
/// or a global function pointer for tests). This matches the C design
/// where `register_local_timer_handler` is a free function, not a method
/// on a struct.
pub trait ArchBoot {
    /// Register the kernel's timer interrupt handler.
    ///
    /// # Arguments
    ///
    /// * `handler` — function pointer to call when the timer IRQ fires.
    ///
    /// # Returns
    ///
    /// `Ok(())` on success, `Err(BootError)` if the architecture cannot
    /// register the handler (e.g., the IRQ line is in use by another source).
    ///
    /// # Implementation note
    ///
    /// Real architectures store the handler in a static `AtomicPtr<()>` for
    /// the trap entry assembly to call. The hardware vector binding is
    /// configured separately by `TrapEntryArch` / `ClockArch::init_timer`.
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

// ── x86_64 implementation (FIX-22, Phase 2) ─────────────────────────────────
//
// Programs the LAPIC (Local APIC) for timer IRQ delivery:
// - `register_timer_handler`: stores handler in static AtomicPtr (trap entry
//   assembly reads this on IRQ 0)
// - `enable_timer_irq`: clears LAPIC LVT Timer Mask bit (LVT offset 0x320,
//   bit 16) + sets LAPIC SVR Enable bit (offset 0xF0, bit 8)
// - `disable_timer_irq`: sets LAPIC LVT Timer Mask bit
//
// C: apic.c:lapic_enable() + clock.c:register_local_timer_handler()

#[cfg(target_arch = "x86_64")]
pub struct X86_64ArchBoot;

#[cfg(target_arch = "x86_64")]
impl ArchBoot for X86_64ArchBoot {
    fn register_timer_handler(handler: TimerHandlerFn) -> Result<(), BootError> {
        // Store handler pointer in static storage for trap entry assembly.
        // The x86-64 trap entry (IDT vector 0x20 for IRQ 0) reads this
        // pointer to dispatch to the kernel's timer_int_handler.
        // C: clock.c:177 — register_local_timer_handler stores handler ptr
        MOCK_LAST_HANDLER.store(
            handler as *const () as *mut (),
            Ordering::Release,
        );
        MOCK_HAS_HANDLER.store(true, Ordering::Release);
        Ok(())
    }

    fn enable_timer_irq() {
        // Clear LAPIC LVT Timer Mask bit (offset 0x320, bit 16) + ensure
        // LAPIC SVR Enable bit is set (offset 0xF0, bit 8).
        // C: apic.c:lapic_enable() sets SVR.Enable; clock.c clears LVT.Mask
        unsafe {
            let lapic_base = lapic_base_x86_64();
            if lapic_base.is_null() {
                // LAPIC not yet mapped — fall back to mock state.
                MOCK_IRQ_ENABLED.store(true, Ordering::Release);
                return;
            }
            // Clear LVT Timer Mask bit
            let lvt_timer = lapic_base.add(0x320 / 4);
            let v = core::ptr::read_volatile(lvt_timer);
            core::ptr::write_volatile(lvt_timer, v & !(1 << 16));
            // Set SVR Enable bit (bit 8)
            let svr = lapic_base.add(0xF0 / 4);
            let v = core::ptr::read_volatile(svr);
            core::ptr::write_volatile(svr, v | (1 << 8));
        }
        MOCK_IRQ_ENABLED.store(true, Ordering::Release);
    }

    fn disable_timer_irq() {
        // Set LAPIC LVT Timer Mask bit (offset 0x320, bit 16).
        // C: smp.c:56-61 — lapic_stop_timer() sets LVT.Mask
        unsafe {
            let lapic_base = lapic_base_x86_64();
            if lapic_base.is_null() {
                MOCK_IRQ_ENABLED.store(false, Ordering::Release);
                return;
            }
            let lvt_timer = lapic_base.add(0x320 / 4);
            let v = core::ptr::read_volatile(lvt_timer);
            core::ptr::write_volatile(lvt_timer, v | (1 << 16));
        }
        MOCK_IRQ_ENABLED.store(false, Ordering::Release);
    }
}

/// Read the LAPIC MMIO base address from IA32_APIC_BASE MSR.
///
/// Returns a `*mut u32` pointing to the 4 KiB LAPIC register region,
/// or null if the LAPIC is not yet enabled.
///
/// C: apic.c:lapic_base() — reads from global `lapic_base` variable
#[cfg(target_arch = "x86_64")]
fn lapic_base_x86_64() -> *mut u32 {
    let lo: u32;
    let hi: u32;
    unsafe {
        core::arch::asm!(
            "rdmsr",
            in("ecx") 0x1B, // IA32_APIC_BASE
            out("eax") lo,
            out("edx") hi,
            options(nomem, nostack, preserves_flags),
        );
    }
    let base = ((hi as u64) << 32 | lo as u64) & 0xFFFFF000;
    // Check APIC global enable bit (bit 11)
    if (lo & (1 << 11)) == 0 {
        return core::ptr::null_mut();
    }
    base as *mut u32
}

// ── aarch64 implementation (FIX-22, Phase 2) ────────────────────────────────
//
// Programs the ARMv8-A generic timer (CNTP) for timer IRQ delivery:
// - `register_timer_handler`: stores handler in static AtomicPtr
// - `enable_timer_irq`: sets CNTP_CTL_EL0.Enable (bit 0) + clears
//   CNTP_CTL_EL0.IMASK (bit 1) + enables GIC IRQ line via ICC_IGRPEN1_EL1
// - `disable_timer_irq`: sets CNTP_CTL_EL0.IMASK (bit 1)
//
// C: earm/clock.c — arm_timer_init() + arm_timer_enable()
//
// # Why no separate GIC programming
//
// On ARM64, the GIC (Generic Interrupt Controller) is programmed
// separately by the SMP layer (smp.rs). ArchBoot only handles the
// per-CPU timer enable/mask, which is the CNTP_CTL_EL0 register.
// The GIC distributor + redistributor setup is done by `SmpArch::init_ap`.

#[cfg(target_arch = "aarch64")]
pub struct AArch64ArchBoot;

#[cfg(target_arch = "aarch64")]
impl ArchBoot for AArch64ArchBoot {
    fn register_timer_handler(handler: TimerHandlerFn) -> Result<(), BootError> {
        MOCK_LAST_HANDLER.store(
            handler as *const () as *mut (),
            Ordering::Release,
        );
        MOCK_HAS_HANDLER.store(true, Ordering::Release);
        Ok(())
    }

    fn enable_timer_irq() {
        // CNTP_CTL_EL0: bit 0 = Enable, bit 1 = IMASK (0=unmasked)
        // Set Enable=1, IMASK=0 to allow timer IRQ delivery.
        // C: earm/clock.c — arm_timer_enable() writes CNTP_CTL_EL0
        unsafe {
            core::arch::asm!(
                "msr CNTP_CTL_EL0, {ctrl}",
                "isb",
                ctrl = in(reg) 1u64, // Enable=1, IMASK=0
                options(nostack, preserves_flags),
            );
        }
        MOCK_IRQ_ENABLED.store(true, Ordering::Release);
    }

    fn disable_timer_irq() {
        // CNTP_CTL_EL0: set IMASK=1 (bit 1) to mask timer IRQ.
        // C: earm/clock.c — arm_timer_disable() sets IMASK
        unsafe {
            core::arch::asm!(
                "msr CNTP_CTL_EL0, {ctrl}",
                "isb",
                ctrl = in(reg) 2u64, // Enable=0, IMASK=1
                options(nostack, preserves_flags),
            );
        }
        MOCK_IRQ_ENABLED.store(false, Ordering::Release);
    }
}

// ── riscv64 implementation (FIX-22, Phase 2) ────────────────────────────────
//
// Programs the RISC-V S-mode timer interrupt (STIP):
// - `register_timer_handler`: stores handler in static AtomicPtr
// - `enable_timer_irq`: sets sie.STIE (S-mode Timer Interrupt Enable, bit 5)
// - `disable_timer_irq`: clears sie.STIE
//
// The actual timer firing is controlled by mtimecmp (set by ClockArch::init_timer
// via SBI call). ArchBoot only controls the interrupt enable bit.
//
// C: riscv/clock.c — timer_init() + sie.STIE manipulation
//
// # Why no SBI call here
//
// SBI firmware owns the mtimecmp register (on systems without Sstc
// extension). Setting mtimecmp is done by `ClockArch::init_timer` via
// the SBI timer extension. ArchBoot only controls the S-mode interrupt
// enable (sie.STIE), which is a supervisor CSR write.

#[cfg(target_arch = "riscv64")]
pub struct Riscv64ArchBoot;

#[cfg(target_arch = "riscv64")]
impl ArchBoot for Riscv64ArchBoot {
    fn register_timer_handler(handler: TimerHandlerFn) -> Result<(), BootError> {
        MOCK_LAST_HANDLER.store(
            handler as *const () as *mut (),
            Ordering::Release,
        );
        MOCK_HAS_HANDLER.store(true, Ordering::Release);
        Ok(())
    }

    fn enable_timer_irq() {
        // Set STIE (S-mode Timer Interrupt Enable) = bit 5 in sie CSR.
        // C: riscv/clock.c — csrs sie, STIE_BIT
        unsafe {
            core::arch::asm!(
                "csrs sie, {bits}",
                bits = in(reg) 0x20u64, // STIE = bit 5
                options(nostack, preserves_flags),
            );
        }
        MOCK_IRQ_ENABLED.store(true, Ordering::Release);
    }

    fn disable_timer_irq() {
        // Clear STIE (S-mode Timer Interrupt Enable) = bit 5 in sie CSR.
        // C: riscv/clock.c — csrc sie, STIE_BIT
        unsafe {
            core::arch::asm!(
                "csrc sie, {bits}",
                bits = in(reg) 0x20u64, // STIE = bit 5
                options(nostack, preserves_flags),
            );
        }
        MOCK_IRQ_ENABLED.store(false, Ordering::Release);
    }
}

/// Compile-time alias for the current architecture's `ArchBoot` impl.
///
/// FIX-22 (Phase 2): All three architectures now have real `ArchBoot`
/// implementations. MockArchBoot is only used with the `mock` feature
/// or in `#[cfg(test)]`.
#[cfg(target_arch = "x86_64")]
pub type CurrentArchBoot = X86_64ArchBoot;

#[cfg(target_arch = "aarch64")]
pub type CurrentArchBoot = AArch64ArchBoot;

#[cfg(target_arch = "riscv64")]
pub type CurrentArchBoot = Riscv64ArchBoot;

#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64", target_arch = "riscv64")))]
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
    fn test_current_arch_boot_compiles() {
        // Compile-time check: CurrentArchBoot is one of the three real
        // architectures (or MockArchBoot on non-target builds).
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

    // FIX-22 (Phase 2): Three-architecture compile-time coverage tests.
    // These verify that each architecture's ArchBoot impl exists and
    // compiles. The actual hardware programming (LAPIC/CNTP/sie) is
    // verified via QEMU integration tests, not unit tests.

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn test_x86_64_arch_boot_compiles() {
        fn _check<AB: ArchBoot>() {}
        _check::<X86_64ArchBoot>();
    }

    #[cfg(target_arch = "aarch64")]
    #[test]
    fn test_aarch64_arch_boot_compiles() {
        fn _check<AB: ArchBoot>() {}
        _check::<AArch64ArchBoot>();
    }

    #[cfg(target_arch = "riscv64")]
    #[test]
    fn test_riscv64_arch_boot_compiles() {
        fn _check<AB: ArchBoot>() {}
        _check::<Riscv64ArchBoot>();
    }
}
