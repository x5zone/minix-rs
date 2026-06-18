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
//! | `init` (establish protection) | `msr SP_EL1, ...` (kernel stack) — must save/restore SP because SPSel=1 makes SP_EL1 the active SP |
//! | `set_kernel_stack` (switch kernel stack) | Same `msr SP_EL1, ...` with save/restore dance |
//! | `load` (make protection effective) | `isb` (instruction synchronization barrier) — no separate load needed since SP_EL1 is active immediately |
//! | `init_ap` (AP startup) | Same `msr SP_EL1, ...` + `isb` to ensure visibility on the new AP |
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
        // SAFETY: MSR write to SP_EL1 is safe because:
        // - We are executing at EL1 (kernel mode), required for MSR access.
        // - SP_EL1 is the dedicated register for EL1 stack pointer,
        //   used automatically on exception entry from EL0.
        // - kernel_stack_top is a valid kernel virtual address.
        //
        // CRITICAL: When SPSel=1 (the default), SP_EL1 IS the current stack
        // pointer. Writing to SP_EL1 changes the active SP immediately.
        // We must save the old SP, write the new value, then restore SP
        // to avoid corrupting the caller's stack frame.
        unsafe {
            asm!(
                "mov {tmp}, sp",          // save current SP
                "msr SP_EL1, {newval}",   // write new SP_EL1 (also changes current SP!)
                "mov sp, {tmp}",          // restore current SP from saved value
                tmp = out(reg) _,
                newval = in(reg) kernel_stack_top.get(),
                options(nostack, preserves_flags),
            );
        }

        Self { cpu_count: cpu_id + 1 }
    }

    fn set_kernel_stack(&mut self, _cpu_id: u32, stack_top: VirBytes) {
        // SAFETY: Same as init() — SP_EL1 write at EL1 with valid address.
        // Must save/restore SP to avoid stack corruption (see init() comment).
        unsafe {
            asm!(
                "mov {tmp}, sp",
                "msr SP_EL1, {newval}",
                "mov sp, {tmp}",
                tmp = out(reg) _,
                newval = in(reg) stack_top.get(),
                options(nostack, preserves_flags),
            );
        }
    }

    fn load(&self) {
        // On ARM64, VBAR_EL1 is set by TrapEntryArch::load() after
        // the exception vector table is initialized. SP_EL1 is already
        // set by init(). There is no separate "load" operation needed
        // for protection structures on ARM64.
        //
        // SAFETY: ISB is always safe — it is an instruction synchronization
        // barrier that ensures previous system register writes are visible.
        unsafe {
            asm!("isb");
        }
    }

    fn init_ap(&self, cpu_id: u32, kernel_stack_top: VirBytes) {
        // SAFETY: Same as init() — SP_EL1 write at EL1 with valid address.
        // Must save/restore SP to avoid stack corruption (see init() comment).
        // ISB ensures the write is visible before returning.
        unsafe {
            asm!(
                "mov {tmp}, sp",
                "msr SP_EL1, {newval}",
                "mov sp, {tmp}",
                "isb",
                tmp = out(reg) _,
                newval = in(reg) kernel_stack_top.get(),
                options(nostack, preserves_flags),
            );
        }
        let _ = cpu_id;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn privilege_level_values() {
        assert_eq!(AArch64PrivilegeLevel::EL1.get(), 1);
        assert_eq!(AArch64PrivilegeLevel::EL0.get(), 0);
    }

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
    fn kernel_privilege_is_el1() {
        assert_eq!(AArch64Protection::KERNEL_PRIVILEGE, AArch64PrivilegeLevel::EL1);
    }

    #[test]
    fn user_privilege_is_el0() {
        assert_eq!(AArch64Protection::USER_PRIVILEGE, AArch64PrivilegeLevel::EL0);
    }

    #[test]
    fn el1_maps_to_kernel() {
        assert_eq!(
            AArch64Protection::to_privilege(AArch64PrivilegeLevel::EL1),
            Privilege::Kernel
        );
    }

    #[test]
    fn el0_maps_to_user() {
        assert_eq!(
            AArch64Protection::to_privilege(AArch64PrivilegeLevel::EL0),
            Privilege::User
        );
    }

    #[test]
    fn protection_has_cpu_count() {
        // AArch64Protection only tracks cpu_count; SP_EL1 is a hardware register.
        // We can verify the struct exists and init() returns a valid instance
        // (but cannot call init() in unit tests because it uses msr SP_EL1).
        // Instead, verify the type can be constructed manually.
        let prot = AArch64Protection { cpu_count: 1 };
        assert_eq!(prot.cpu_count, 1);
    }
}
