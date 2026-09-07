//! Unit-test + boot-banner support — arch-dispatched re-export.
//!
//! Follows the [`crate::qemu_virt`] house pattern: each arch submodule owns
//! its concrete descriptor construction and banner labels; this file only
//! re-exports the current arch's items with file-level `#[cfg]`, so kernel
//! code consumes them with zero `#[cfg(target_arch)]` (B-X first batch).
//!
//! Replaces the kernel-side trio of `new_test_interrupt_controller` cfg
//! arms and the `ARCH_NAME`/`SP_LABEL`/`PC_LABEL`/`FP_LABEL` cfg blocks.

#[cfg(target_arch = "x86_64")]
pub use crate::arch::x86_64::{
    unit_test_irq_desc, ARCH_NAME, FP_LABEL, PC_LABEL, REACHED_BANNER, SP_LABEL,
};
#[cfg(target_arch = "aarch64")]
pub use crate::arch::aarch64::{
    unit_test_irq_desc, ARCH_NAME, FP_LABEL, PC_LABEL, REACHED_BANNER, SP_LABEL,
};
#[cfg(target_arch = "riscv64")]
pub use crate::arch::riscv64::{
    unit_test_irq_desc, ARCH_NAME, FP_LABEL, PC_LABEL, REACHED_BANNER, SP_LABEL,
};
