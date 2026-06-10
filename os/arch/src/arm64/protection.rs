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
        // Set SP_EL1 to the kernel stack top for exception entry.
        // When an exception occurs (IRQ/SVC/etc.), the CPU automatically
        // switches from SP_EL0 to SP_EL1.
        //
        // C: prot_init() sets SP_EL1 implicitly through tss_init equivalent
        unsafe {
            asm!("msr sp_el1, {}", in(reg) kernel_stack_top.get());
        }

        Self { cpu_count: cpu_id + 1 }
    }

    fn set_kernel_stack(&mut self, _cpu_id: u32, stack_top: VirBytes) {
        unsafe {
            asm!("msr sp_el1, {}", in(reg) stack_top.get());
        }
    }

    fn load(&self) {
        // On ARM64, VBAR_EL1 is set by TrapEntryArch::load() after
        // the exception vector table is initialized. SP_EL1 is already
        // set by init(). There is no separate "load" operation needed
        // for protection structures on ARM64.
        //
        // Ensure instruction synchronization after any system register
        // writes that may have occurred.
        unsafe {
            asm!("isb");
        }
    }

    fn init_ap(&self, cpu_id: u32, kernel_stack_top: VirBytes) {
        // Per-CPU initialization for Application Processors.
        // Set SP_EL1 for this CPU's exception entry stack.
        unsafe {
            asm!("msr sp_el1, {}", in(reg) kernel_stack_top.get());
            asm!("isb");
        }
        let _ = cpu_id;
    }
}
