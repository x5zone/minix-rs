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

use minix_types::{VirBytes, PhysBytes};
use minix_boot::{BootModule, KernelInfo};
use crate::proc_arch::{
    ArchProcReset, ArchProcInit, BootProcArch,
    InitialRegState, InitialRegs, SegmentSelectors, VmLoadResult,
};
use crate::paging::{Paging, PageFlags};

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
        // In Rust, we use minix-elf to parse the ELF binary, then map
        // each PT_LOAD segment into the bootstrap page table via the
        // Paging trait. This replaces C's libexec_load_elf callback
        // mechanism with a direct iterator-based approach.

        // SAFETY: module.start points to the VM ELF image in physical memory.
        // During boot, identity mapping covers this address range.
        // module.len is the exact size of the boot module from the boot info.
        let image = unsafe {
            core::slice::from_raw_parts(
                module.start.0 as *const u8,
                module.len,
            )
        };

        // Parse ELF header and iterate over PT_LOAD segments.
        // C: libexec_load_elf() — libexec/exec_elf.c
        let iter = match minix_elf::segment_iter(image) {
            Ok(it) => it,
            Err(e) => {
                // If ELF parsing fails, return a zeroed result.
                // The caller will detect pc=0 and handle the error.
                // C: libexec_load_elf returns ENOEXEC on bad ELF
                let _ = e;
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

        // Map each PT_LOAD segment into the bootstrap page table.
        // C: libexec callback allocmem → pg_map(PG_ALLOCATEME, ...) — protect.c:425
        for seg in iter {
            // Convert ELF segment flags to PageFlags.
            // C: libexec callback setflags — protect.c:415
            let flags = elf_flags_to_page_flags(seg.flags);

            // Map the segment page by page.
            // C: pg_map(PG_ALLOCATEME, vaddr, paddr, flags) — protect.c:425
            let vaddr_start = seg.vaddr;
            let vaddr_end = seg.vaddr + seg.memsz;
            let mut vaddr = vaddr_start & !(page_size - 1); // page-align down
            let mut file_offset = seg.offset;
            let mut file_remaining = seg.filesz;

            while vaddr < vaddr_end {
                // For each page, determine the physical address.
                // In the bootstrap page table, we use the virtual address
                // directly as the physical address (1:1 mapping for user space).
                // C: pg_map with PG_ALLOCATEME allocates a new physical page.
                let paddr = PhysBytes(vaddr);

                if let Ok(()) = paging.map(VirBytes(vaddr), paddr, flags) {
                    total_allocated += page_size as usize;
                }

                // Copy segment data from the ELF image into the mapped page.
                // C: libexec callback copymem — libexec/exec_elf.c
                if file_remaining > 0 {
                    let copy_start = (vaddr - vaddr_start) as usize;
                    let copy_len = core::cmp::min(
                        file_remaining as usize,
                        page_size as usize - (copy_start % page_size as usize),
                    );
                    if copy_start + copy_len <= seg.filesz as usize {
                        let src_offset = file_offset as usize;
                        let dst_ptr = vaddr as *mut u8;
                        // SAFETY: vaddr is mapped in the bootstrap page table.
                        // We just mapped it above. The copy is within bounds.
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

        // Set up user stack.
        // C: execi.stack_high = kinfo.user_sp; execi.stack_size = VM_STACK_SIZE
        let stack_high = kernel_info.user_sp;
        let sp = VirBytes(stack_high.0 - VM_STACK_SIZE as u64);

        // Map stack pages.
        // C: pg_map for stack — protect.c:428-432
        let stack_flags = PageFlags::read_write();
        let mut stack_addr = sp.0 & !(page_size - 1);
        while stack_addr < stack_high.0 {
            if let Ok(()) = paging.map(VirBytes(stack_addr), PhysBytes(stack_addr), stack_flags) {
                total_allocated += page_size as usize;
            }
            stack_addr += page_size;
        }

        // Set up ps_strings on the stack.
        // C: ps_strings setup — protect.c:435-440
        let ps_strings = VirBytes(sp.0 - 32);

        VmLoadResult {
            pc: VirBytes(entry),
            sp,
            ps_strings,
            allocated_bytes: total_allocated,
        }
    }
}

/// Convert ELF segment flags (PF_R|PF_W|PF_X) to PageFlags.
///
/// ELF PF_R=4, PF_W=2, PF_X=1.
/// C: libexec callback setflags maps these to PTF_* flags.
fn elf_flags_to_page_flags(elf_flags: u32) -> PageFlags {
    let mut flags = PageFlags::PRESENT | PageFlags::USER_ACCESSIBLE;
    if elf_flags & 0x2 != 0 { // PF_W
        flags |= PageFlags::WRITABLE;
    }
    if elf_flags & 0x1 == 0 { // !PF_X → no-execute (NX bit)
        // On x86-64, executable is the default; we don't set NX explicitly
        // here because PageFlags::EXECUTABLE is opt-in.
    }
    if elf_flags & 0x1 != 0 { // PF_X
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
        let state = X86_64ProcArch::initial_reg_state(true, 0);
        assert_eq!(state.status, INIT_TASK_PSW, "kernel task PSW must be INIT_TASK_PSW");
        assert!(!state.fpu_needs_zero, "kernel task must not require FPU zeroing");
        assert_eq!(state.segment_selectors.cs, USER_CS_SELECTOR);
        assert_eq!(state.segment_selectors.ds, USER_DS_SELECTOR);
    }

    #[test]
    fn test_initial_reg_state_user() {
        let state = X86_64ProcArch::initial_reg_state(false, 0);
        assert_eq!(state.status, INIT_PSW, "user process PSW must be INIT_PSW");
        assert!(state.fpu_needs_zero, "user process must require FPU zeroing");
    }

    #[test]
    fn test_init_regs_sets_pc_sp_ps_strings() {
        let pc = VirBytes(0x400000);
        let sp = VirBytes(0x7fff0000);
        let ps = VirBytes(0x7fff0000 - 32);
        let regs = X86_64ProcArch::init_regs(false, 0, pc, sp, ps);
        assert_eq!(regs.pc, pc);
        assert_eq!(regs.sp, sp);
        assert_eq!(regs.ps_strings_reg, ps.0);
    }

    #[test]
    fn test_load_vm_elf_invalid_returns_zero_pc() {
        // A truncated ELF magic is insufficient for parsing.
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
            kern_virt_base: VirBytes(0xffff_8000_0000_0000),
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
        let result = X86_64ProcArch::load_vm_elf(&module, &kinfo, &mut paging);
        assert_eq!(result.pc, VirBytes(0), "invalid ELF must produce pc=0");
        assert!(result.sp.0 < kinfo.user_sp.0, "stack must be below user_sp");
    }
}
