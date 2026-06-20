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

use minix_types::{VirBytes, PhysBytes};
use minix_boot::{BootModule, KernelInfo};
use crate::proc_arch::{
    ArchProcReset, ArchProcInit, BootProcArch,
    InitialRegState, InitialRegs, SegmentSelectors, VmLoadResult,
};
use crate::paging::{Paging, PageFlags};

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
    fn initial_reg_state(is_kernel: bool, proc_nr: i32) -> InitialRegState {
        // C: earm/arch_system.c:42-60
        //
        // arch_proc_reset(pr):
        //   memset(&pr->p_reg, 0, sizeof(pr->p_reg));
        //   if(iskerneln(pr->p_nr))
        //     pr->p_reg.psr = INIT_TASK_PSR;  // EL1h, 0x3C5
        //   else
        //     pr->p_reg.psr = INIT_PSR;       // EL0t, 0x0
        //
        // AArch64 does not initialize FPU state in arch_proc_reset.
        // FPU/VFP state is lazily initialized on first use.
        // AArch64 has no segment selectors (flat memory model).

        let _ = proc_nr;
        let status = if is_kernel { INIT_TASK_PSR } else { INIT_PSR };

        InitialRegState {
            status,
            segment_selectors: SegmentSelectors::default(), // all zero
            fpu_needs_zero: false, // lazy FPU init
        }
    }
}

impl ArchProcInit for AArch64ProcArch {
    fn init_regs(
        is_kernel: bool,
        proc_nr: i32,
        pc: VirBytes,
        sp: VirBytes,
        ps_strings: VirBytes,
    ) -> InitialRegs {
        // C: earm/memory.c:627-638
        //
        // arch_proc_init(pr, ip, sp, ps_str, name):
        //   arch_proc_reset(pr);
        //   strcpy(pr->p_name, name);
        //   pr->p_reg.pc = ip;
        //   pr->p_reg.sp = sp;
        //   pr->p_reg.retreg = ps_str;  // aarch64: r0 = ps_strings

        let _ = (is_kernel, proc_nr);
        InitialRegs {
            pc,                          // pc (set via ELR_EL1)
            sp,                          // sp
            ps_strings_reg: ps_strings.0, // r0 (retreg)
        }
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
        // Same high-level flow as x86-64: parse ELF, map segments,
        // set up stack, return entry point. The ELF loading and
        // ps_strings setup are identical across architectures.
        // Only arch_proc_init differs (which register holds ps_strings).

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
        let state = AArch64ProcArch::initial_reg_state(true, 0);
        assert_eq!(state.status, INIT_TASK_PSR, "kernel task PSR must be INIT_TASK_PSR");
        assert!(!state.fpu_needs_zero, "AArch64 uses lazy FPU init");
        assert_eq!(state.segment_selectors, SegmentSelectors::default());
    }

    #[test]
    fn test_initial_reg_state_user() {
        let state = AArch64ProcArch::initial_reg_state(false, 0);
        assert_eq!(state.status, INIT_PSR, "user process PSR must be INIT_PSR");
        assert!(!state.fpu_needs_zero, "AArch64 uses lazy FPU init");
    }

    #[test]
    fn test_init_regs_sets_pc_sp_ps_strings() {
        let pc = VirBytes(0x400000);
        let sp = VirBytes(0x7fff0000);
        let ps = VirBytes(0x7fff0000 - 32);
        let regs = AArch64ProcArch::init_regs(false, 0, pc, sp, ps);
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
            kern_virt_base: VirBytes(0xffff_0000_0000_0000),
            kern_phys_base: PhysBytes(0),
            kern_size: 0,
            free_upper_idx: None,
            user_sp: VirBytes(0x7fff_ffff_ffff_0000),
            kern_stack_top: VirBytes(0),
            syscall_entry: VirBytes(0),
            boot_modules: &EMPTY_MODULES,
            bootstrap_start: PhysBytes(0),
            bootstrap_len: 0,
            platform_descriptor: None,
        };
        let mut paging = MockPaging::new().unwrap();
        let result = AArch64ProcArch::load_vm_elf(&module, &kinfo, &mut paging);
        assert_eq!(result.pc, VirBytes(0), "invalid ELF must produce pc=0");
        assert!(result.sp.0 < kinfo.user_sp.0);
    }
}
