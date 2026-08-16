//! Interrupt controller board-level abstraction
//!
//! Defines the trait interface for interrupt controller hardware operations
//! and shared types for IRQ management.
//!
//! # Design decisions (see 14-exception-interrupt.md §3.2, §3.4, §3.8)
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
pub struct IrqVector(pub u8);

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
pub struct IrqId(pub u32);

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
pub struct IrqNotifyId(pub u32);

impl IrqNotifyId {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u32 {
        self.0
    }
}

// IRQ policy flags.
//
// C: IRQ_REENABLE — com.h:308
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
/// # Instance-based design (see 04-platform-discovery.md §3.4)
///
/// `InterruptController` is **instance-based**: `new(desc)` stores parsed
/// hardware base addresses in instance fields. This replaces the old design
/// that hardcoded addresses per arch or required a separate `set_base()`
/// call. Upper layers obtain the descriptor from
/// `minix_platform::platform_desc()`.
///
/// # Architecture mapping
///
/// | Method       | x86-64 (APIC)        | ARM64 (GICv3)      | RISC-V (PLIC)    |
/// |-------------|----------------------|--------------------|--------------------|
/// | `new()`     | downcast to `ApicDesc`| downcast to      | downcast to        |
/// |             | store LAPIC+IOAPIC   | `Gicv3Desc`, store | `PlicDesc`, store  |
/// |             | base                 | GICD+GICR base     | PLIC base+context  |
/// | `init()`    | Initialize LAPIC +   | Initialize GIC     | Initialize PLIC    |
/// |             | IOAPIC, mask all     | distributor +      | + CLINT, mask all  |
/// |             |                      | redistributors     |                    |
/// | `mask()`    | IOAPIC mask bit      | GICD_ICENABLER     | PLIC enable=0      |
/// |             |                      | (SPI) / GICR_      |                    |
/// |             |                      | ICENABLER0 (PPI)   |                    |
/// | `unmask()`  | IOAPIC unmask bit    | GICD_ISENABLER     | PLIC enable=1      |
/// |             |                      | (SPI) / GICR_      |                    |
/// |             |                      | ISENABLER0 (PPI)   |                    |
/// | `ack()`     | LAPIC EOI            | Read IAR (ACK)     | Read claim (ACK)   |
/// | `eoi()`     | LAPIC EOI write      | Write EOIR         | Write complete     |
/// | `mask_all()`| IOAPIC mask all      | GICD_ICENABLER=all | PLIC threshold=max |
///
/// C: hw_intr_mask/unmask/ack — hw_intr.h:22-24/45-47
pub trait InterruptController: Sized + Send + Sync {
    /// Create an instance from an interrupt controller descriptor.
    ///
    /// Stores the hardware base addresses from the descriptor into instance
    /// fields. Called once during `init_clock_and_interrupts()` after
    /// `PlatformContext` is initialized.
    ///
    /// # Panics
    ///
    /// May panic if `desc` does not downcast to the architecture's expected
    /// concrete `InterruptControllerDesc` implementor (e.g. x86-64 expects
    /// `ApicDesc`). Upper layers guarantee the correct type is passed.
    fn new(desc: &dyn minix_platform::InterruptControllerDesc) -> Self;

    /// Initialize the interrupt controller.
    fn init(&mut self);

    /// Mask (disable) an IRQ line.
    fn mask(&mut self, irq: IrqVector);

    /// Unmask (enable) an IRQ line.
    fn unmask(&mut self, irq: IrqVector);

    /// Acknowledge receipt of an interrupt.
    fn ack(&mut self, irq: IrqVector);

    /// Signal end-of-interrupt processing.
    fn eoi(&mut self, irq: IrqVector);

    /// Mask all IRQ lines.
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
    // 注：本模块原 10 个测试多为"包装器往返 + 平凡派生属性"（Pattern #38
    // 自指测试），例如 `IrqVector::new(32).get() == 32`、`policy.bits() == 0x001`、
    // `NR_IRQ_VECTORS == 64` 等。这些断言永远通过，对发现回归无价值。
    // Newtype 包装正确性由其构造/访问器签名（`pub const fn new/get`）保证，
    // 编译期即可检测类型错位；bitflags 的 bits() 行为由 `bitflags!` 宏保证；
    // enum 派生 trait 由 `#[derive(...)]` 保证。本模块**目前无行为可单元测试**——
    // 真实行为测试在各架构 `interrupt.rs` 的 `test_new_from_*_descriptor` /
    // `test_new_clamps_nr_irqs_to_max` 中。
}
