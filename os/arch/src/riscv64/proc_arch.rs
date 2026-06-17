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

use minix_types::{VirBytes, PhysBytes};
use minix_boot::{BootModule, KernelInfo};
use crate::proc_arch::{
    ArchProcReset, ArchProcInit, BootProcArch,
    InitialRegState, InitialRegs, SegmentSelectors, VmLoadResult,
};
use crate::paging::{Paging, PageFlags};

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

        // SAFETY: module.start points to the VM ELF image in physical memory.
        // During boot, identity mapping covers this address range.
        let image = unsafe {
            core::slice::from_raw_parts(
                module.start.0 as *const u8,
                module.len,
            )
        };

        let iter = match minix_elf::segment_iter(image) {
            Ok(it) => it,
            Err(_) => {
                let stack_high = kernel_info.user_sp;
                let sp = VirBytes(stack_high.0 - VM_STACK_SIZE as u64);
                return VmLoadResult {
                    pc: VirBytes(0),
                    sp,
                    ps_strings: VirBytes(sp.0 - 32),
                    allocated_bytes: 0,
                };
            }
        };

        let entry = minix_elf::entry_point(image).unwrap_or(0);
        let page_size = P::PAGE_SIZE as u64;
        let mut total_allocated: usize = 0;

        for seg in iter {
            let flags = elf_flags_to_page_flags(seg.flags);
            let vaddr_start = seg.vaddr;
            let vaddr_end = seg.vaddr + seg.memsz;
            let mut vaddr = vaddr_start & !(page_size - 1);
            let mut file_offset = seg.offset;
            let mut file_remaining = seg.filesz;

            while vaddr < vaddr_end {
                let paddr = PhysBytes(vaddr);
                if let Ok(()) = paging.map(VirBytes(vaddr), paddr, flags) {
                    total_allocated += page_size as usize;
                }
                if file_remaining > 0 {
                    let copy_start = (vaddr - vaddr_start) as usize;
                    let copy_len = core::cmp::min(
                        file_remaining as usize,
                        page_size as usize - (copy_start % page_size as usize),
                    );
                    if copy_start + copy_len <= seg.filesz as usize {
                        let src_offset = file_offset as usize;
                        let dst_ptr = vaddr as *mut u8;
                        unsafe {
                            core::ptr::copy_nonoverlapping(
                                image.as_ptr().add(src_offset),
                                dst_ptr,
                                copy_len,
                            );
                        }
                        file_offset += copy_len as u64;
                        file_remaining -= copy_len as u64;
                    }
                }
                vaddr += page_size;
            }
        }

        let stack_high = kernel_info.user_sp;
        let sp = VirBytes(stack_high.0 - VM_STACK_SIZE as u64);
        let stack_flags = PageFlags::read_write();
        let mut stack_addr = sp.0 & !(page_size - 1);
        while stack_addr < stack_high.0 {
            if let Ok(()) = paging.map(VirBytes(stack_addr), PhysBytes(stack_addr), stack_flags) {
                total_allocated += page_size as usize;
            }
            stack_addr += page_size;
        }

        let ps_strings = VirBytes(sp.0 - 32);
        VmLoadResult {
            pc: VirBytes(entry),
            sp,
            ps_strings,
            allocated_bytes: total_allocated,
        }
    }
}

/// Convert ELF segment flags to PageFlags.
fn elf_flags_to_page_flags(elf_flags: u32) -> PageFlags {
    let mut flags = PageFlags::PRESENT | PageFlags::USER_ACCESSIBLE;
    if elf_flags & 0x2 != 0 {
        flags |= PageFlags::WRITABLE;
    }
    if elf_flags & 0x1 != 0 {
        flags |= PageFlags::EXECUTABLE;
    }
    flags
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MockPaging;

    #[test]
    fn test_initial_reg_state_kernel() {
        let state = Riscv64ProcArch::initial_reg_state(true, 0);
        assert_eq!(state.status, INIT_TASK_SSTATUS, "kernel task sstatus must be INIT_TASK_SSTATUS");
        assert!(!state.fpu_needs_zero, "RISC-V uses lazy FPU init");
        assert_eq!(state.segment_selectors, SegmentSelectors::default());
    }

    #[test]
    fn test_initial_reg_state_user() {
        let state = Riscv64ProcArch::initial_reg_state(false, 0);
        assert_eq!(state.status, INIT_SSTATUS, "user process sstatus must be INIT_SSTATUS");
        assert!(!state.fpu_needs_zero, "RISC-V uses lazy FPU init");
    }

    #[test]
    fn test_init_regs_sets_pc_sp_ps_strings() {
        let pc = VirBytes(0x400000);
        let sp = VirBytes(0x7fff0000);
        let ps = VirBytes(0x7fff0000 - 32);
        let regs = Riscv64ProcArch::init_regs(false, 0, pc, sp, ps);
        assert_eq!(regs.pc, pc);
        assert_eq!(regs.sp, sp);
        assert_eq!(regs.ps_strings_reg, ps.0);
    }

    #[test]
    fn test_load_vm_elf_invalid_returns_zero_pc() {
        static BAD_ELF: [u8; 4] = [0x7f, b'E', b'L', b'F'];
        let module = BootModule {
            name: "fake-vm",
            start: PhysBytes(BAD_ELF.as_ptr() as u64),
            len: BAD_ELF.len(),
        };
        static EMPTY_MEMMAP: [minix_boot::MemoryRegion; 0] = [];
        static EMPTY_MODULES: [BootModule; 0] = [];
        let kinfo = KernelInfo {
            memmap: &EMPTY_MEMMAP,
            kern_virt_base: VirBytes(0xffff_ffff_c000_0000),
            kern_phys_base: PhysBytes(0),
            kern_size: 0,
            free_upper_idx: None,
            user_sp: VirBytes(0x7fff_ffff_ffff_0000),
            kern_stack_top: VirBytes(0),
            syscall_entry: VirBytes(0),
            boot_modules: &EMPTY_MODULES,
            bootstrap_start: PhysBytes(0),
            bootstrap_len: 0,
        };
        let mut paging = MockPaging::new().unwrap();
        let result = Riscv64ProcArch::load_vm_elf(&module, &kinfo, &mut paging);
        assert_eq!(result.pc, VirBytes(0), "invalid ELF must produce pc=0");
        assert!(result.sp.0 < kinfo.user_sp.0);
    }
}
