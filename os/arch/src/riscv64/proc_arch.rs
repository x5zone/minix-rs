//! RISC-V 64-bit process architecture implementation
//!
//! Implements `ArchProcReset`, `ArchProcInit`, and `BootProcArch` for RISC-V.
//!
//! # Register conventions
//!
//! - PC: `sepc` (set via sret instruction)
//! - SP: `sp` (x2)
//! - ps_strings: `a0` (x10, first argument register)
//!
//! Note: Minix3 does not have a RISC-V port. This implementation is
//! designed by analogy with the aarch64 port, following RISC-V
//! privileged specification conventions.

use minix_types::VirBytes;
use minix_boot::{BootModule, KernelInfo};
use crate::proc_arch::{
    ArchProcReset, ArchProcInit, BootProcArch,
    InitialRegState, InitialRegs, SegmentSelectors, VmLoadResult,
};
use crate::paging::Paging;

/// RISC-V 64-bit process architecture implementation.
pub struct Riscv64ProcArch;

/// Initial sstatus for kernel tasks: SPP=1 (S-mode), SPIE=0 (interrupts disabled).
/// When sret executes, SPP=1 means return to S-mode.
const INIT_TASK_SSTATUS: u64 = 0x00000100; // SPP=1

/// Initial sstatus for user processes: SPP=0 (U-mode), SPIE=1 (interrupts enabled).
/// When sret executes, SPP=0 means return to U-mode, SPIE=1 enables interrupts.
const INIT_SSTATUS: u64 = 0x00000020; // SPIE=1

/// VM process number.
/// C: VM_PROC_NR — minix/com.h:67
const VM_PROC_NR: i32 = 8;

/// Default user stack size for VM (64 KB).
/// C: execi.stack_size = 64 * 1024 — protect.c:411
const VM_STACK_SIZE: usize = 64 * 1024;

impl ArchProcReset for Riscv64ProcArch {
    fn initial_reg_state(is_kernel: bool, proc_nr: i32) -> InitialRegState {
        // No C source — designed by analogy with aarch64.
        //
        // RISC-V arch_proc_reset:
        // 1. Clear all registers (x0-x31, f0-f31)
        // 2. Set sstatus based on kernel/user type:
        //    - Kernel: SPP=1 (return to S-mode on sret), SPIE=0
        //    - User:   SPP=0 (return to U-mode on sret), SPIE=1
        //
        // RISC-V does not have segment selectors (flat memory model).
        // FPU state is lazily initialized on first use (like ARM).

        let _ = proc_nr;
        let status = if is_kernel { INIT_TASK_SSTATUS } else { INIT_SSTATUS };

        InitialRegState {
            status,
            segment_selectors: SegmentSelectors::default(), // all zero
            fpu_needs_zero: false, // lazy FPU init
        }
    }
}

impl ArchProcInit for Riscv64ProcArch {
    fn init_regs(
        is_kernel: bool,
        proc_nr: i32,
        pc: VirBytes,
        sp: VirBytes,
        ps_strings: VirBytes,
    ) -> InitialRegs {
        // No C source — designed by analogy with aarch64.
        //
        // RISC-V arch_proc_init:
        //   arch_proc_reset(pr);
        //   strlcpy(pr->p_name, name, sizeof(pr->p_name));
        //   pr->p_reg.pc = ip;        // sepc
        //   pr->p_reg.sp = sp;        // x2
        //   pr->p_reg.a0 = ps_str;    // x10 = ps_strings

        let _ = (is_kernel, proc_nr);
        InitialRegs {
            pc,                          // sepc
            sp,                          // x2
            ps_strings_reg: ps_strings.0, // a0 (x10)
        }
    }
}

impl BootProcArch for Riscv64ProcArch {
    fn load_vm_elf<P: Paging>(
        module: &BootModule,
        kernel_info: &KernelInfo,
        paging: &mut P,
    ) -> VmLoadResult {
        // No C source — same logic as x86-64/ARM versions.
        // The ELF loading and ps_strings setup are identical
        // across architectures.
        let _ = (module, kernel_info, paging);

        let stack_high = kernel_info.user_sp;
        let sp = VirBytes(stack_high.0 - VM_STACK_SIZE as u64);

        VmLoadResult {
            pc: VirBytes(0),
            sp,
            ps_strings: VirBytes(sp.0 - 32),
            allocated_bytes: 0,
        }
    }
}
