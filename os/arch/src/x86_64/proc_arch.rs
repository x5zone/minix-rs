//! x86-64 process architecture implementation
//!
//! Implements `ArchProcReset`, `ArchProcInit`, and `BootProcArch` for x86-64.
//!
//! # Register conventions
//!
//! - PC: `rip` (instruction pointer)
//! - SP: `rsp` (stack pointer)
//! - ps_strings: `rbx` (callee-saved, used to pass ps_strings address)
//!
//! C: arch_system.c:146-192 (arch_proc_reset)
//! C: memory.c:722-733 (arch_proc_init)
//! C: protect.c:388-456 (arch_boot_proc)

use minix_types::VirBytes;
use minix_boot::{BootModule, KernelInfo};
use crate::proc_arch::{
    ArchProcReset, ArchProcInit, BootProcArch,
    InitialRegState, InitialRegs, SegmentSelectors, VmLoadResult,
};
use crate::paging::Paging;

/// x86-64 process architecture implementation.
///
/// Provides architecture-specific process initialization for x86-64,
/// including register state setup and VM ELF loading.
pub struct X86_64ProcArch;

/// Initial PSW (RFLAGS) for kernel tasks.
///
/// # Architecture evolution (32-bit → 64-bit)
///
/// Minix3 32-bit: `INIT_TASK_PSW = 0x1200` (IOPL=1, IF=1).
/// Kernel tasks ran in ring 1; IOPL=1 allowed them I/O access.
///
/// minix-rs 64-bit: `0x1202` (IOPL=1, IF=1, bit1=1).
/// In 64-bit mode, only ring 0 and ring 3 exist. Kernel tasks run in
/// ring 0, where I/O access is always permitted regardless of IOPL.
/// IOPL=1 is preserved for semantic consistency with Minix3.
///
/// C: INIT_TASK_PSW — i386/include/archconst.h:119 (0x1200)
const INIT_TASK_PSW: u64 = 0x1202; // IOPL=1, IF=1, bit1=1

/// Initial PSW (RFLAGS) for user processes.
///
/// # Architecture evolution (32-bit → 64-bit)
///
/// Minix3 32-bit: `INIT_PSW = 0x0200` (IOPL=0, IF=1).
/// minix-rs 64-bit: `0x0202` (IOPL=0, IF=1, bit1=1).
/// Bit 1 is always set in RFLAGS (64-bit requirement).
///
/// C: INIT_PSW — i386/include/archconst.h:118 (0x0200)
const INIT_PSW: u64 = 0x0202; // IOPL=0, IF=1, bit1=1

/// User code segment selector (Ring 3).
/// C: USER_CS_SELECTOR — protect.c
const USER_CS_SELECTOR: u64 = 0x1B; // GDT index 3, RPL=3

/// User data segment selector (Ring 3).
/// C: USER_DS_SELECTOR — protect.c
const USER_DS_SELECTOR: u64 = 0x23; // GDT index 4, RPL=3

/// VM process number.
/// C: VM_PROC_NR — minix/com.h:67
#[allow(dead_code)]
const VM_PROC_NR: i32 = 8;

/// Default user stack size for VM (64 KB).
/// C: execi.stack_size = 64 * 1024 — protect.c:411
const VM_STACK_SIZE: usize = 64 * 1024;

impl ArchProcReset for X86_64ProcArch {
    fn initial_reg_state(is_kernel: bool, proc_nr: i32) -> InitialRegState {
        // C: arch_system.c:146-192
        //
        // arch_proc_reset(pr):
        // 1. For user processes (p_nr >= 0): zero FPU state
        // 2. Clear register state (memset &reg, 0)
        // 3. Set PSW based on kernel/user type:
        //    - Kernel: INIT_TASK_PSW (0x1200, 32-bit; 0x1202, 64-bit)
        //    - User:   INIT_PSW      (0x0200, 32-bit; 0x0202, 64-bit)
        // 4. Set segment selectors: CS=USER_CS_SELECTOR, rest=USER_DS_SELECTOR
        // 5. arch_proc_setcontext(pr, &reg, 0, KTS_FULLCONTEXT)
        //
        // In Rust, we return the initial register state as a pure value.
        // The kernel layer applies it to the process's trap frame.

        let _ = proc_nr;
        let status = if is_kernel { INIT_TASK_PSW } else { INIT_PSW };
        let fpu_needs_zero = !is_kernel; // C: user processes only (p_nr >= 0)

        InitialRegState {
            status,
            segment_selectors: SegmentSelectors {
                cs: USER_CS_SELECTOR,
                ds: USER_DS_SELECTOR,
                ss: USER_DS_SELECTOR,
                es: USER_DS_SELECTOR,
                fs: USER_DS_SELECTOR,
                gs: USER_DS_SELECTOR,
            },
            fpu_needs_zero,
        }
    }
}

impl ArchProcInit for X86_64ProcArch {
    fn init_regs(
        is_kernel: bool,
        proc_nr: i32,
        pc: VirBytes,
        sp: VirBytes,
        ps_strings: VirBytes,
    ) -> InitialRegs {
        // C: memory.c:722-733
        //
        // arch_proc_init(pr, ip, sp, ps_str, name):
        //   arch_proc_reset(pr);
        //   strlcpy(pr->p_name, name, sizeof(pr->p_name));
        //   pr->p_reg.pc = ip;
        //   pr->p_reg.sp = sp;
        //   pr->p_reg.bx = ps_str;   // x86-64: rbx = ps_strings
        //
        // In Rust, we return the PC, SP, and ps_strings register values.
        // The kernel layer calls ArchProcReset::initial_reg_state() first,
        // then applies these values on top. The process name is set by the
        // kernel layer (not arch responsibility).

        let _ = (is_kernel, proc_nr);
        InitialRegs {
            pc,                          // rip
            sp,                          // rsp
            ps_strings_reg: ps_strings.0, // rbx
        }
    }
}

impl BootProcArch for X86_64ProcArch {
    fn load_vm_elf<P: Paging>(
        module: &BootModule,
        kernel_info: &KernelInfo,
        paging: &mut P,
    ) -> VmLoadResult {
        // C: protect.c:388-456
        //
        // arch_boot_proc for VM:
        // 1. Set up exec_info with stack_high, stack_size, hdr, etc.
        // 2. Call libexec_load_elf(&execi) — parses ELF, allocates pages,
        //    copies segments into bootstrap page table
        // 3. Set up ps_strings on the stack
        // 4. Call arch_proc_init(rp, execi.pc, sp, ps_str, "vm")
        //
        // In Rust, we implement a simplified ELF loader that:
        // 1. Parses ELF header and program headers
        // 2. For each PT_LOAD segment: allocates pages, maps in bootstrap
        //    page table, copies segment data
        // 3. Sets up user stack
        // 4. Returns entry point and stack pointer

        let _ = (module, kernel_info, paging);

        // Placeholder: actual ELF loading requires the `object` crate
        // or a custom ELF parser. For now, return a stub result.
        let stack_high = kernel_info.user_sp;
        let sp = VirBytes(stack_high.0 - VM_STACK_SIZE as u64);

        VmLoadResult {
            pc: VirBytes(0), // Will be set from ELF entry point
            sp,
            ps_strings: VirBytes(sp.0 - 32), // ps_strings above SP
            allocated_bytes: 0,
        }
    }
}
