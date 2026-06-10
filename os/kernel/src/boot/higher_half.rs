//! Higher-half kernel transition abstraction.
//!
//! After paging is enabled, the CPU still executes at low addresses
//! (identity-mapped). This trait provides the architecture-specific
//! mechanism to switch the stack pointer and jump to the kernel's
//! high virtual address entry point (kmain).
//!
//! Corresponds to Minix3 head.S:78-87 (x86) / head.S:34-40 (ARM).
//!
//! See 02-higher-half-kernel.md §3.4 and §4.3 for design rationale.

use minix_types::{KernelInfo, VirBytes};

/// Abstraction for the higher-half kernel transition.
///
/// Each architecture implements this trait to provide the low-level
/// mechanism for switching from identity-mapped execution to the
/// kernel's high virtual address space.
///
/// # Why a trait instead of `#[cfg(target_arch)]`?
///
/// 1. **Architecture isolation**: Each arch's implementation is completely
///    different (x86-64 uses `mov rsp + call`, aarch64 uses `mov sp + br`,
///    riscv64 uses `mv sp + jalr`). The trait encapsulates these differences.
/// 2. **Uniform call site**: `arch_boot_impl` calls `HigherHalf::jump_to_kmain()`
///    without `#[cfg]` conditionals.
/// 3. **Testability**: Mock implementations can verify call sequencing without
///    executing real instructions.
pub trait HigherHalf {
    /// Perform the higher-half transition: switch stack to high address
    /// and jump to kmain.
    ///
    /// # Arguments
    ///
    /// - `kinfo`: Pointer to KernelInfo, passed as the first argument to kmain
    /// - `stack_top`: Virtual address of the kernel stack top (high address)
    ///
    /// # Safety
    ///
    /// Caller must ensure:
    /// - Paging is enabled with both identity and kernel high mappings
    /// - `kinfo` pointer is valid and accessible at high address
    /// - `stack_top` is a valid, 16-byte-aligned virtual address in the
    ///   kernel's high-half mapping
    /// - This is called exactly once, from the boot CPU
    unsafe fn jump_to_kmain(kinfo: &KernelInfo, stack_top: VirBytes) -> !;
}
