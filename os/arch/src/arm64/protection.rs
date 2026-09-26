//! ARM64 (aarch64) protection mechanism implementation
//!
//! Implements `ProtectionArch` for ARM64. ARM64 does not use segment
//! descriptors (GDT) like x86 — privilege levels are managed by the
//! CPU's exception level mechanism (EL0/EL1) and the SP_EL1 register.
//!
//! # ARM64 privilege model
//!
//! - EL0: User mode (unprivileged)
//! - EL1: Kernel mode (supervisor)
//! - EL2: Hypervisor (not used by Minix-RS)
//! - EL3: Secure monitor (not used by Minix-RS)
//!
//! On exception entry (SVC/IRQ/FIQ/SError), the CPU automatically:
//! 1. Saves PSTATE to SPSR_EL1
//! 2. Saves return address to ELR_EL1
//! 3. Switches SP from SP_EL0 to SP_EL1
//! 4. Sets PSTATE.M to EL1h
//! 5. Vectors through VBAR_EL1 + offset
//!
//! # OS-level concerns → ARM64 mechanism mapping
//!
//! The `ProtectionArch` trait exposes OS-level concerns; this file maps
//! each to its concrete ARM64 implementation:
//!
//! | OS concern (trait method) | ARM64 mechanism (this file's impl) |
//! |---------------------------|--------------------------------------|
//! | `init` (establish protection) | no-op under the EL1h model — `SPSel=1` makes `SP_EL1` the *active* stack pointer, which the higher-half `jump_to_kmain` already loaded with `mov sp, kern_stack_top`; a banked `msr SP_EL1, ...` is architecturally UNDEFINED at `SPSel=1` and traps |
//! | `set_kernel_stack` (switch kernel stack) | no-op under EL1h — the active SP is the exception stack, so relocating it from a returning Rust fn would orphan the caller frame; the switch is owned by the vector-entry trampoline (`trap_stub`) |
//! | `load` (make protection effective) | `isb` (instruction synchronization barrier) — no separate load needed since SP_EL1 is active immediately |
//! | `init_ap` (AP startup) | no-op under EL1h — each AP's stack is established by its own early-entry assembly before reaching Rust |
//!
//! # Why ARM64 has no GDT/TSS
//!
//! Unlike x86-64 (which requires GDT to reference TSS descriptor for
//! kernel stack switches), ARM64 uses a single dedicated register
//! (`SP_EL1`) per exception level. This eliminates the indirection
//! table requirement and makes protection setup O(1) register writes.
//!
//! C: prot_init() — earm/protect.c:77

use crate::protection::{ProtectionArch, Privilege};
use minix_types::VirBytes;
use core::arch::asm;

/// ARM64 privilege level representation.
///
/// ARM64 uses Exception Levels (EL) for privilege separation.
/// Only EL0 (user) and EL1 (kernel) are used by Minix-RS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AArch64PrivilegeLevel(u8);

impl AArch64PrivilegeLevel {
    /// Exception Level 1 — kernel/supervisor mode.
    pub const EL1: Self = Self(1);
    /// Exception Level 0 — user/unprivileged mode.
    pub const EL0: Self = Self(0);

    pub(crate) const fn get(self) -> u8 {
        self.0
    }
}

/// ARM64 protection state.
///
/// On ARM64, the primary protection configuration is:
/// - SP_EL1: Kernel stack pointer for exception entry
/// - VBAR_EL1: Exception vector base address (set by TrapEntryArch)
///
/// Unlike x86-64, there is no GDT, IDT, or TSS. The CPU handles
/// privilege transitions automatically based on exception level.
pub struct AArch64Protection {
    /// Number of CPUs initialized (for SMP tracking).
    cpu_count: u32,
}

impl ProtectionArch for AArch64Protection {
    type PrivilegeLevel = AArch64PrivilegeLevel;

    const KERNEL_PRIVILEGE: AArch64PrivilegeLevel = AArch64PrivilegeLevel::EL1;
    const USER_PRIVILEGE: AArch64PrivilegeLevel = AArch64PrivilegeLevel::EL0;

    fn to_privilege(level: AArch64PrivilegeLevel) -> Privilege {
        match level {
            AArch64PrivilegeLevel::EL1 => Privilege::Kernel,
            _ => Privilege::User,
        }
    }

    fn from_privilege(privilege: Privilege) -> AArch64PrivilegeLevel {
        match privilege {
            Privilege::Kernel => AArch64PrivilegeLevel::EL1,
            Privilege::User => AArch64PrivilegeLevel::EL0,
        }
    }

    fn init(cpu_id: u32, kernel_stack_top: VirBytes) -> Self {
        // EL1h model (`SPSel=1`): `SP_EL1` *is* the active stack pointer, so
        // it is not bank-accessible via `msr SP_EL1, Xt` — ARMv8 D1.5 declares
        // that encoding UNDEFINED when `SPSel=1`, and executing it raises a
        // synchronous exception (this is exactly the boot trap that pinned
        // NK4-C §1.109). The kernel stack the exception entry will use is
        // therefore the running SP, which the higher-half transition already
        // established with `mov sp, kern_stack_top`
        // (kernel/src/arch/aarch64/higher_half.rs). There is nothing to
        // program here: `kernel_stack_top` is reference-only (it equals the
        // live SP).
        let _ = kernel_stack_top;
        Self { cpu_count: cpu_id + 1 }
    }

    fn set_kernel_stack(&mut self, _cpu_id: u32, stack_top: VirBytes) {
        // EL1h model: the kernel's exception stack *is* the active SP, so a
        // per-context stack switch cannot be performed by relocating SP from
        // a returning Rust function — `mov sp, ...` here would orphan the
        // caller frame (the function epilogue would pop x29/x30 from the new
        // stack). The outgoing SP is saved by the switch trampoline and the
        // incoming task's SP is reloaded in the vector entry path
        // (`arm64/trap_stub.rs` frames on the EL1 entry stack), which is the
        // sole owner of the EL1h stack switch. This trait method is therefore
        // a documented no-op on AArch64; `stack_top` is accepted for trait
        // shape parity with x86 (where it writes TSS.sp0, pure data).
        let _ = stack_top;
    }

    fn load(&self) {
        // On ARM64, VBAR_EL1 is set by TrapEntryArch::load() after
        // the exception vector table is initialized. The kernel's active
        // stack (SP_EL1 under EL1h) is established by the higher-half
        // transition (`mov sp, kern_stack_top` in
        // kernel/src/arch/aarch64/higher_half.rs), not by ProtectionArch::init.
        // There is no separate "load" operation needed for protection
        // structures on ARM64.
        //
        // SAFETY: ISB is always safe — it is an instruction synchronization
        // barrier that ensures previous system register writes are visible.
        unsafe {
            asm!("isb");
        }
    }

    fn init_ap(&self, cpu_id: u32, kernel_stack_top: VirBytes) {
        // EL1h model (see `set_kernel_stack`): each AP's kernel stack is
        // established by its own early-entry assembly before it reaches Rust,
        // so relocating SP from this returning method would corrupt the AP's
        // caller frame. No-op here; the per-CPU stack is owned by the AP entry
        // trampoline. `kernel_stack_top`/`cpu_id` accepted for trait-shape
        // parity with x86 (per-CPU TSS/GS programming).
        let _ = kernel_stack_top;
        let _ = cpu_id;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn privilege_level_roundtrip() {
        assert_eq!(
            AArch64Protection::to_privilege(AArch64PrivilegeLevel::EL1),
            Privilege::Kernel
        );
        assert_eq!(
            AArch64Protection::to_privilege(AArch64PrivilegeLevel::EL0),
            Privilege::User
        );
        assert_eq!(
            AArch64Protection::from_privilege(Privilege::Kernel),
            AArch64PrivilegeLevel::EL1
        );
        assert_eq!(
            AArch64Protection::from_privilege(Privilege::User),
            AArch64PrivilegeLevel::EL0
        );
    }

    #[test]
    fn protection_has_cpu_count() {
        // AArch64Protection only tracks cpu_count; the exception stack is the
        // active SP_EL1 (a hardware register, no per-instance table). init()
        // is a no-op under the EL1h model and this module is aarch64-only, so
        // the host unit test just verifies the struct can be constructed.
        let prot = AArch64Protection { cpu_count: 1 };
        assert_eq!(prot.cpu_count, 1);
    }
}
