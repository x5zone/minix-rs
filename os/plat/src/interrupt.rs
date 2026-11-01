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
/// | `ack()`     | No claim step (no-op)| Read IAR (ACK)     | Read claim (ACK)   |
/// | `eoi()`     | LAPIC EOI write      | Write EOIR         | Write complete     |
/// | `mask_all()`| IOAPIC mask all      | GICD_ICENABLER=all | PLIC threshold=max |
///
/// C: hw_intr_mask/unmask/ack — hw_intr.h:22-24/45-47
/// Global routing plane — owns IRQ-line enable/disable state (BSP-owned,
/// BKL-serialized, lives in the global `IrqManager`).
///
/// This trait was split out of the old monolithic `InterruptController` along
/// the semantic seam its own implementations already exhibited -- all six
/// `ack`/`eoi` impls across the three architectures ignored the `irq`
/// parameter, while `mask`/`unmask` genuinely route lines. Routing is *global*
/// state (one routing table per machine); the claim/complete pair moved to
/// [`PerCpuInterruptUnit`].
///
/// C: intr_init + hw_intr enable/disable ops (kernel/interrupt.h driver
/// table).
pub trait InterruptRouter: Sized + Send + Sync {
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

    /// Initialize the interrupt controller (all lines masked afterwards).
    fn init(&mut self);

    /// Mask (disable) an IRQ line.
    fn mask(&mut self, irq: IrqVector);

    /// Unmask (enable) an IRQ line.
    fn unmask(&mut self, irq: IrqVector);

    /// Mask all IRQ lines.
    fn mask_all(&mut self);

    /// Set a hardware priority for an IRQ line (riscv64 PLIC: 0 = never
    /// delivered, >=1 = deliverable once above the context threshold).
    /// Controllers without a priority register (x86 8259 is flat, aarch64 GIC
    /// uses its defaults) keep the no-op default.
    fn set_irq_priority(&mut self, _irq: IrqVector, _priority: u32) {}
}

/// Per-CPU interrupt unit — the claim/complete half of the protocol
/// (called on the interrupting CPU).
///
/// The claim identity is *per-CPU* state: two CPUs claiming simultaneously
/// must never share it. The current single instance lives in the global
/// `IrqManager` and is safe because the BKL serializes every dispatch;
/// when per-CPU dispatch lands, instances move to
/// `CpuLocal` and this trait's ownership follows.
///
/// C: the claim/complete ops of the arch APIC/GIC/PLIC driver.
pub trait PerCpuInterruptUnit: Send {
    /// Per-context claim (PD-22 step 2, riscv64): claim through the
    /// hart's own S-mode PLIC context instead of the boot-time default.
    /// Default = the plain single-context face (mock/x86/aarch64 have no
    /// per-hart context concept; aarch64 completes via system registers).
    fn claim_context(&mut self, _context: usize) -> Option<u32> {
        self.claim()
    }

    /// Per-context completion (pairs with [`Self::claim_context`]).
    fn complete_context(&mut self, _context: usize, claimed: Option<u32>) {
        self.complete(claimed);
    }

    /// Acknowledge (claim) the interrupt, before any handler runs, and
    /// return the claimed identity.
    ///
    /// GIC: reads `ICC_IAR1_EL1` — the read itself acknowledges and yields
    /// the 24-bit masked INTID; the GIC spurious INTID (1023) yields
    /// `None`. PLIC: reads the claim register — the source ID, with 0
    /// ("nothing pending") yielding `None`. x86 APIC: no claim step and no
    /// identity (the gate delivered the vector; the LAPIC EOI is the whole
    /// handshake), so `None` — but here `None` means "nothing to write
    /// back", not "nothing to complete": [`PerCpuInterruptUnit::complete`]
    /// still EOIs. The x86 implementation previously
    /// wrote the LAPIC EOI in `claim` — that is the *completion*, not a
    /// claim, and dispatching it early would have re-opened the interrupt
    /// before handling.
    ///
    /// The identity is deliberately `u32` rather than `IrqVector`: it is
    /// the raw hardware claim token (GIC INTIDs reach beyond 255 via SPI
    /// ranges — truncating it through a `u8` vector would corrupt the
    /// paired completion), and the two namespaces are distinct — the
    /// routing vector keys the hook chains, the claim token is what the
    /// completion register expects back. The typed claim->completion pairing
    /// hardens that protocol lesson and lets `IrqManager::dispatch` skip
    /// completing spurious claims; an intid-driven dispatch lane (arm64/riscv64
    /// trap entries) can later consume the same return value directly.
    fn claim(&mut self) -> Option<u32>;

    /// Signal end-of-interrupt processing, paired with `claim`.
    ///
    /// GIC/PLIC: write back the SAME identity `claim` returned — the
    /// pairing is the protocol (completing identity 0 or a foreign INTID
    /// is not a legal completion); `None` (spurious claim read)
    /// completes nothing. x86: the value is ignored — the write is the
    /// global LAPIC EOI, and it happens even when `claim` returned `None`,
    /// because the gate-driven flow has no claim token to pair with.
    fn complete(&mut self, claimed: Option<u32>);

    /// Enable a software-generated interrupt (SGI) on the CPU whose
    /// architecture hardware id is `hw_id`.
    ///
    /// Only aarch64 GICv3 needs this per-CPU gate (`GICR_ISENABLER0` in
    /// that CPU's redistributor). x86 delivers IPIs through the LAPIC ICR
    /// and riscv64 through SBI `send_ipi`; neither has an enable bit, so
    /// their default is a no-op.
    fn enable_sgi(&mut self, _hw_id: usize, _intid: u8) {}
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
    // Note: this module's original 10 tests were mostly "wrapper round-trip +
    // trivial derived property" self-referential tests, e.g.
    // `IrqVector::new(32).get() == 32`, `policy.bits() == 0x001`,
    // `NR_IRQ_VECTORS == 64`, and similar. Such assertions always pass and are
    // worthless for catching regressions. Newtype wrapper correctness is
    // guaranteed by its constructor/accessor signatures
    // (`pub const fn new/get`) and a type mix-up is caught at compile time; the
    // bits() behavior of bitflags is guaranteed by the `bitflags!` macro; enum
    // derived traits are guaranteed by `#[derive(...)]`. This module currently
    // has **no behavior to unit-test** -- the real behavior tests live in each
    // arch's `interrupt.rs` as `test_new_from_*_descriptor` /
    // `test_new_clamps_nr_irqs_to_max`.
}
