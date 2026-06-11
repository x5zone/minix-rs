//! Interrupt controller architecture abstraction
//!
//! Defines the trait interface for interrupt controller hardware operations
//! and shared types for IRQ management.
//!
//! # Design decisions (see 05-exception-interrupt.md §3.2, §3.4, §3.8)
//!
//! - **InterruptController trait** (§3.2): Abstracts mask/unmask/ack/eoi
//!   operations. Replaces C's `hw_intr` macro with runtime polymorphism.
//! - **IrqVector vs InterruptVector** (§3.8): Two distinct types prevent
//!   confusing hardware IRQ numbers with IDT vector indices.
//! - **IrqAction enum** (§3.4): Replaces C's sentinel-value return convention
//!   (0 = not done, non-zero = done) with a typed enum.
//! - **IrqPolicy bitflags** (§3.4): Replaces C's `irq_policy_t` unsigned long
//!   with type-safe bitflags.

/// Hardware IRQ vector number.
///
/// Distinct from `InterruptVector` (IDT vector index). On x86-64,
/// `IrqVector(0)` maps to `InterruptVector(0x50)` via `IRQ0_VECTOR`.
/// On ARM64/RISC-V, the mapping is architecture-specific.
///
/// C: interrupt.h:35-37 (NR_IRQ_VECTORS)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct IrqVector(pub(crate) u8);

impl IrqVector {
    pub const fn new(value: u8) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u8 {
        self.0
    }
}

/// IRQ hook identifier (bitmask for active tracking).
///
/// Each hook on an IRQ line gets a unique bit in the `irq_actids` bitmap.
/// Allocated by finding the lowest unset bit.
///
/// C: hook->id — type.h:22
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IrqId(pub(crate) u32);

impl IrqId {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u32 {
        self.0
    }
}

/// IRQ notification identifier.
///
/// Returned to the driver when an IRQ fires, so the driver can
/// correlate the notification with its registered hook.
///
/// C: hook->notify_id — type.h:25
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IrqNotifyId(pub(crate) u32);

impl IrqNotifyId {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u32 {
        self.0
    }
}

/// IRQ policy flags.
///
/// C: IRQ_REENABLE — com.h:308
bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct IrqPolicy: u32 {
        const REENABLE = 0x001;
    }
}

/// IRQ handler action (return value from handler callback).
///
/// C: generic_handler() returns hook->policy & IRQ_REENABLE
///    (non-zero = completed, zero = not completed)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrqAction {
    Completed,
    NotCompleted,
}

/// Architecture abstraction for interrupt controller operations.
///
/// Manages masking, unmasking, acknowledging, and signaling end-of-interrupt
/// for hardware interrupt lines.
///
/// # Architecture mapping
///
/// | Method       | x86-64 (APIC)        | ARM64 (GICv3)      | RISC-V (PLIC)    |
/// |-------------|----------------------|--------------------|--------------------|
/// | `init()`    | Initialize LAPIC +   | Initialize GIC     | Initialize PLIC    |
/// |             | IOAPIC, mask all     | distributor +      | + CLINT, mask all  |
/// |             |                      | redistributors     |                    |
/// | `mask()`    | IOAPIC mask bit      | GICD_ICENABLER     | PLIC enable=0      |
/// | `unmask()`  | IOAPIC unmask bit    | GICD_ISENABLER     | PLIC enable=1      |
/// | `ack()`     | LAPIC EOI            | Read IAR (ACK)     | Read claim (ACK)   |
/// | `eoi()`     | LAPIC EOI write      | Write EOIR         | Write complete     |
/// | `mask_all()`| IOAPIC mask all      | GICD_ICENABLER=all | PLIC threshold=max |
///
/// C: hw_intr_mask/unmask/ack — hw_intr.h:22-24/45-47
pub trait InterruptController: Sized {
    /// Initialize the interrupt controller.
    ///
    /// Called once during kernel startup. After this call, all IRQ lines
    /// are masked and no interrupts will be delivered.
    ///
    /// C: intr_init() — i8259.c:28 (PIC) / apic.c (APIC)
    fn init(&mut self);

    /// Mask (disable) an IRQ line.
    ///
    /// C: hw_intr_mask(irq) — hw_intr.h:22/45
    fn mask(&mut self, irq: IrqVector);

    /// Unmask (enable) an IRQ line.
    ///
    /// C: hw_intr_unmask(irq) — hw_intr.h:23/46
    fn unmask(&mut self, irq: IrqVector);

    /// Acknowledge receipt of an interrupt.
    ///
    /// On some architectures, this reads the interrupt ID from the
    /// controller (e.g., ARM64 GIC IAR register). On x86-64, this
    /// is the same as `eoi()`.
    ///
    /// C: hw_intr_ack(irq) — hw_intr.h:24/47
    fn ack(&mut self, irq: IrqVector);

    /// Signal end-of-interrupt processing.
    ///
    /// Called after all handlers for this IRQ have completed.
    /// On x86-64, this writes to the LAPIC EOI register.
    ///
    /// C: hw_intr_ack(irq) — hw_intr.h:24/47
    fn eoi(&mut self, irq: IrqVector);

    /// Mask all IRQ lines.
    ///
    /// Called during early boot to ensure no interrupts fire
    /// before handlers are registered.
    ///
    /// C: hw_intr_disable_all() — hw_intr.h:33/50
    fn mask_all(&mut self);
}

/// Maximum number of IRQ vectors.
///
/// C: NR_IRQ_VECTORS — interrupt.h:35-37
/// 64-bit: always 64 (APIC mode)
pub const NR_IRQ_VECTORS: usize = 64;

/// Maximum number of IRQ hooks (system-wide).
///
/// C: NR_IRQ_HOOKS — config.h:59/61
pub const NR_IRQ_HOOKS: usize = 64;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_irq_vector_new() {
        let v = IrqVector::new(32);
        assert_eq!(v.get(), 32);
    }

    #[test]
    fn test_irq_vector_const() {
        const V: IrqVector = IrqVector::new(0);
        assert_eq!(V.get(), 0);
    }

    #[test]
    fn test_irq_vector_boundaries() {
        let min = IrqVector::new(0);
        let max = IrqVector::new(63);
        assert_eq!(min.get(), 0);
        assert_eq!(max.get(), 63);
    }

    #[test]
    fn test_irq_vector_equality() {
        let a = IrqVector::new(5);
        let b = IrqVector::new(5);
        let c = IrqVector::new(10);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn test_irq_id_new() {
        let id = IrqId::new(1);
        assert_eq!(id.get(), 1);
    }

    #[test]
    fn test_irq_id_const() {
        const ID: IrqId = IrqId::new(0);
        assert_eq!(ID.get(), 0);
    }

    #[test]
    fn test_irq_notify_id_new() {
        let nid = IrqNotifyId::new(42);
        assert_eq!(nid.get(), 42);
    }

    #[test]
    fn test_irq_notify_id_const() {
        const NID: IrqNotifyId = IrqNotifyId::new(255);
        assert_eq!(NID.get(), 255);
    }

    #[test]
    fn test_irq_policy_reenable() {
        let policy = IrqPolicy::REENABLE;
        assert!(policy.contains(IrqPolicy::REENABLE));
        assert_eq!(policy.bits(), 0x001);
    }

    #[test]
    fn test_irq_policy_empty() {
        let policy = IrqPolicy::empty();
        assert_eq!(policy.bits(), 0);
    }

    #[test]
    fn test_nr_irq_constants() {
        assert_eq!(NR_IRQ_VECTORS, 64);
        assert_eq!(NR_IRQ_HOOKS, 64);
    }

    #[test]
    fn test_irq_action_discriminants() {
        // Ensure Completed and NotCompleted are distinct
        assert!(matches!(IrqAction::Completed, IrqAction::Completed));
        assert!(matches!(IrqAction::NotCompleted, IrqAction::NotCompleted));
        assert_ne!(
            format!("{:?}", IrqAction::Completed),
            format!("{:?}", IrqAction::NotCompleted)
        );
    }

    #[test]
    fn test_irq_vector_debug_format() {
        let v = IrqVector::new(7);
        let debug = format!("{:?}", v);
        assert!(debug.contains("IrqVector"));
    }
}
