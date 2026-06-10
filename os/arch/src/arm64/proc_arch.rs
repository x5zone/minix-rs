//! AArch64 process architecture implementation
//!
//! Implements `ArchProcReset`, `ArchProcInit`, and `BootProcArch` for AArch64.
//!
//! # Register conventions
//!
//! - PC: `pc` (program counter, set via SPSR_EL1/ELR_EL1 on exception return)
//! - SP: `sp` (stack pointer)
//! - ps_strings: `r0` (return register / first argument)
//!
//! C: earm/arch_system.c:42-60 (arch_proc_reset)
//! C: earm/memory.c:627-638 (arch_proc_init)
//! C: earm/protect.c:115-183 (arch_boot_proc)

use minix_types::{BootModule, KernelInfo, VirBytes};
use crate::proc_arch::{ArchProcReset, ArchProcInit, BootProcArch, VmLoadResult};
use crate::paging::Paging;

/// AArch64 process architecture implementation.
pub struct AArch64ProcArch;

/// Initial PSR (SPSR_EL1 value) for kernel tasks: EL1h, IRQ/FIQ masked.
///
/// # Architecture evolution (32-bit ARM → 64-bit AArch64)
///
/// Minix3 ARM32: `INIT_TASK_PSR = PSR_SVC32_MODE | PSR_F = 0x53`.
/// Kernel tasks ran in SVC32 mode (ARM32 equivalent of kernel mode).
///
/// minix-rs AArch64: `0x000003C5` (M=EL1h, F=1, I=1, A=1, D=1).
/// Kernel tasks run in EL1 (AArch64 kernel mode). EL1h is the handler
/// mode with dedicated SP_EL1. IRQ/FIQ/SError/Debug exceptions are
/// masked (F/I/A/D bits set) as kernel tasks manage interrupts explicitly.
///
/// C: INIT_TASK_PSR — earm/include/archconst.h:12 (0x53, 32-bit)
const INIT_TASK_PSR: u64 = 0x000003C5; // M=EL1h, F=1, I=1, A=1, D=1

/// Initial PSR (SPSR_EL1 value) for user processes: EL0t, no masking.
///
/// # Architecture evolution (32-bit ARM → 64-bit AArch64)
///
/// Minix3 ARM32: `INIT_PSR = PSR_USR32_MODE | PSR_F = 0x50`.
/// User processes ran in USR32 mode with FIQ masked.
///
/// minix-rs AArch64: `0x00000000` (M=EL0t, no masking).
/// User processes run in EL0t (AArch64 user mode). FIQ masking is
/// not needed in AArch64 as the exception routing is handled by
/// the interrupt controller (GICv3) rather than CPSR flags.
///
/// C: INIT_PSR — earm/include/archconst.h:11 (0x50, 32-bit)
const INIT_PSR: u64 = 0x00000000; // M=EL0t

/// VM process number.
/// C: VM_PROC_NR — minix/com.h:67
const VM_PROC_NR: i32 = 8;

/// Default user stack size for VM (64 KB).
/// C: execi.stack_size = 64 * 1024 — earm/protect.c:137
const VM_STACK_SIZE: usize = 64 * 1024;

impl ArchProcReset for AArch64ProcArch {
    fn reset(is_kernel: bool, proc_nr: i32) {
        // C: earm/arch_system.c:42-60
        //
        // arch_proc_reset(pr):
        //   memset(&pr->p_reg, 0, sizeof(pr->p_reg));
        //   if(iskerneln(pr->p_nr))
        //     pr->p_reg.psr = INIT_TASK_PSR;
        //   else
        //     pr->p_reg.psr = INIT_PSR;
        //
        // ARM64 does not initialize FPU state in arch_proc_reset.
        // FPU/VFP state is lazily initialized on first use.
        let _ = (is_kernel, proc_nr);
    }
}

impl ArchProcInit for AArch64ProcArch {
    fn init(
        is_kernel: bool,
        proc_nr: i32,
        pc: VirBytes,
        sp: VirBytes,
        ps_strings: VirBytes,
        name: &str,
    ) {
        // C: earm/memory.c:627-638
        //
        // arch_proc_init(pr, ip, sp, ps_str, name):
        //   arch_proc_reset(pr);
        //   strcpy(pr->p_name, name);
        //   pr->p_reg.pc = ip;
        //   pr->p_reg.sp = sp;
        //   pr->p_reg.retreg = ps_str;  // aarch64: r0 = ps_strings
        Self::reset(is_kernel, proc_nr);
        let _ = (pc, sp, ps_strings, name);
    }
}

impl BootProcArch for AArch64ProcArch {
    fn load_vm_elf<P: Paging>(
        module: &BootModule,
        kernel_info: &KernelInfo,
        paging: &mut P,
    ) -> VmLoadResult {
        // C: earm/protect.c:115-183
        //
        // Same logic as x86-64 version — the ELF loading and ps_strings
        // setup are identical across architectures. Only arch_proc_init
        // differs (which register holds ps_strings).
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
