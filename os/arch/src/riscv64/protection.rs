//! RISC-V 64-bit (Sv39) protection mechanism implementation
//!
//! Implements `ProtectionArch` for RISC-V 64-bit. RISC-V uses a simpler
//! privilege model than x86 — there are no segment descriptors (GDT) or
//! descriptor tables (IDT). Privilege levels are managed by the CPU's
//! mode bits (S-mode/U-mode) and the sscratch CSR.
//!
//! # RISC-V privilege model
//!
//! - U-mode: User mode (unprivileged)
//! - S-mode: Supervisor mode (kernel)
//! - M-mode: Machine mode (not used by Minix-RS after boot)
//!
//! On trap entry (ecall/external interrupt/exception), the CPU:
//! 1. Sets pc = stvec (supervisor trap vector base)
//! 2. Saves pc to sepc (supervisor exception program counter)
//! 3. Saves privilege mode to sstatus.SPP
//! 4. Sets privilege mode to S-mode
//! 5. Disables interrupts (sstatus.SPIE = sstatus.SIE, SIE = 0)
//!
//! The kernel is responsible for swapping sp with sscratch on entry
//! and restoring it on exit (sret). This is done in the trap handler
//! assembly code, not by hardware.
//!
//! Note: Minix3 does not have a RISC-V port. The semantics are derived
//! from the RISC-V Privileged Specification and the same higher-half
//! principle used by x86/ARM.

use crate::protection::{ProtectionArch, Privilege};
use minix_types::VirBytes;
use core::arch::asm;

/// RISC-V privilege level representation.
///
/// RISC-V uses two privilege modes for OS operation:
/// S-mode (supervisor) and U-mode (user).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Riscv64PrivilegeLevel(u8);

impl Riscv64PrivilegeLevel {
    /// Supervisor mode — kernel privilege level.
    pub const S_MODE: Self = Self(1);
    /// User mode — unprivileged level.
    pub const U_MODE: Self = Self(0);

    pub(crate) const fn get(self) -> u8 {
        self.0
    }
}

/// RISC-V 64-bit protection state.
///
/// On RISC-V, the primary protection configuration is:
/// - sscratch: Holds the kernel stack pointer, swapped with sp on trap entry
/// - sstatus: Controls interrupt enable and tracks previous privilege mode
/// - stvec: Trap vector base address (set by TrapEntryArch)
///
/// Unlike x86-64, there is no GDT, IDT, or TSS. The CPU handles
/// privilege transitions based on the trap mechanism defined in the
/// RISC-V Privileged Specification.
pub struct Riscv64Protection {
    /// Number of CPUs initialized (for SMP tracking).
    cpu_count: u32,
}

impl ProtectionArch for Riscv64Protection {
    type PrivilegeLevel = Riscv64PrivilegeLevel;

    const KERNEL_PRIVILEGE: Riscv64PrivilegeLevel = Riscv64PrivilegeLevel::S_MODE;
    const USER_PRIVILEGE: Riscv64PrivilegeLevel = Riscv64PrivilegeLevel::U_MODE;

    fn to_privilege(level: Riscv64PrivilegeLevel) -> Privilege {
        match level {
            Riscv64PrivilegeLevel::S_MODE => Privilege::Kernel,
            _ => Privilege::User,
        }
    }

    fn from_privilege(privilege: Privilege) -> Riscv64PrivilegeLevel {
        match privilege {
            Privilege::Kernel => Riscv64PrivilegeLevel::S_MODE,
            Privilege::User => Riscv64PrivilegeLevel::U_MODE,
        }
    }

    fn init(cpu_id: u32, kernel_stack_top: VirBytes) -> Self {
        // SAFETY: CSR write to sscratch is safe because:
        // - We are in S-mode (supervisor), required for CSR access.
        // - sscratch holds the kernel stack pointer for U→S transitions.
        // - kernel_stack_top is a valid kernel virtual address.
        unsafe {
            asm!("csrw sscratch, {}", in(reg) kernel_stack_top.get());
        }

        Self { cpu_count: cpu_id + 1 }
    }

    fn set_kernel_stack(&mut self, _cpu_id: u32, stack_top: VirBytes) {
        // SAFETY: Same as init() — sscratch write in S-mode with valid address.
        unsafe {
            asm!("csrw sscratch, {}", in(reg) stack_top.get());
        }
    }

    fn load(&self) {
        // On RISC-V, sscratch is already set by init().
        // stvec is set by TrapEntryArch::load().
        // There is no separate "load" operation for protection
        // structures — CSRs take effect immediately on write.
    }

    fn init_ap(&self, cpu_id: u32, kernel_stack_top: VirBytes) {
        // SAFETY: Same as init() — sscratch write in S-mode with valid address.
        unsafe {
            asm!("csrw sscratch, {}", in(reg) kernel_stack_top.get());
        }
        let _ = cpu_id;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn privilege_level_values() {
        assert_eq!(Riscv64PrivilegeLevel::S_MODE.get(), 1);
        assert_eq!(Riscv64PrivilegeLevel::U_MODE.get(), 0);
    }

    #[test]
    fn privilege_level_roundtrip() {
        assert_eq!(
            Riscv64Protection::to_privilege(Riscv64PrivilegeLevel::S_MODE),
            Privilege::Kernel
        );
        assert_eq!(
            Riscv64Protection::to_privilege(Riscv64PrivilegeLevel::U_MODE),
            Privilege::User
        );
        assert_eq!(
            Riscv64Protection::from_privilege(Privilege::Kernel),
            Riscv64PrivilegeLevel::S_MODE
        );
        assert_eq!(
            Riscv64Protection::from_privilege(Privilege::User),
            Riscv64PrivilegeLevel::U_MODE
        );
    }

    #[test]
    fn kernel_privilege_is_s_mode() {
        assert_eq!(Riscv64Protection::KERNEL_PRIVILEGE, Riscv64PrivilegeLevel::S_MODE);
    }

    #[test]
    fn user_privilege_is_u_mode() {
        assert_eq!(Riscv64Protection::USER_PRIVILEGE, Riscv64PrivilegeLevel::U_MODE);
    }

    #[test]
    fn protection_has_cpu_count() {
        // Riscv64Protection only tracks cpu_count; sscratch is a hardware CSR.
        // Verify the struct can be constructed manually.
        let prot = Riscv64Protection { cpu_count: 1 };
        assert_eq!(prot.cpu_count, 1);
    }
}
