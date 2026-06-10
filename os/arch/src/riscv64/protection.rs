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
        // Set sscratch to the kernel stack top.
        // On trap from U-mode, the trap handler assembly code
        // swaps sp and sscratch to obtain the kernel stack pointer.
        //
        // This is the RISC-V equivalent of x86's TSS.sp0:
        // it provides the kernel stack for privilege transitions.
        //
        // C: No direct Minix3 equivalent (Minix3 has no RISC-V port).
        // Equivalent to tss_init() setting tss.sp0 on x86.
        unsafe {
            asm!("csrw sscratch, {}", in(reg) kernel_stack_top.get());
        }

        Self { cpu_count: cpu_id + 1 }
    }

    fn set_kernel_stack(&mut self, _cpu_id: u32, stack_top: VirBytes) {
        // Update sscratch with the new kernel stack top.
        // This is called when switching to a different process,
        // as each process has its own kernel stack.
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
        // Per-CPU initialization for Application Processors (hart).
        // Set sscratch for this hart's kernel stack.
        unsafe {
            asm!("csrw sscratch, {}", in(reg) kernel_stack_top.get());
        }
        let _ = cpu_id;
    }
}
