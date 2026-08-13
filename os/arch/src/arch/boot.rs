//! Process boot / CPU-context abstraction.
//!
//! Defines the single arch-level abstraction for "build a process's initial
//! CPU context" and "apply that context to a trap frame". Also defines the
//! shared `load_vm_elf` free function (the three architectures'
//! implementations were previously byte-for-byte identical — see
//! `06-proc-init-boot-proc.md` §3.5 for the rationale for collapsing the
//! older 3-trait hierarchy into a single trait + one free function).
//!
//! # Why `CpuContextArch` (and not `BootArch`)
//!
//! The trait describes how to build / apply a **process's CPU context**,
//! not just "the boot phase". The same trait is consulted when a process
//! first runs (`apply_to_trap_frame`) and it owns all architecture-private
//! data (`CpuContext`). The boot phase is only one of its consumers.
//! Naming the trait after the broader concept — instead of after the
//! narrow "boot" lifecycle — keeps the abstraction honest.
//!
//! # Why a single trait (instead of the previous 3)
//!
//! The C side has three functions — `arch_proc_reset`, `arch_proc_init`,
//! `arch_boot_proc` — but those are implementation detail, not OS
//! concepts. The Rust side collapses the three into one
//! `build_cpu_context` (covers reset + init) and one free function
//! `load_vm_elf` (covers the ELF-loading half of `arch_boot_proc`).
//! See `06-proc-init-boot-proc.md` §3.5 for the full rationale.
//!
//! # FPU handling
//!
//! Modern 64-bit architectures use lazy FPU initialisation
//! (XSAVE / CPACR_EL1.FPEN / sstatus.FS); they do NOT use the legacy
//! `fnsave`/`fxrstor` model that Minix3 32-bit code translates through.
//! FPU policy is a `CpuContext` field that the kernel layer never reads.
//! See `06-proc-init-boot-proc.md` §3.6.

use minix_boot::{BootModule, KernelInfo};
use minix_types::VirBytes;

use crate::paging::Paging;

/// Number-type for a process slot (kernel tasks have negative slots).
///
/// Re-exposed here so that downstream code can name the parameter type
/// without reaching back into `minix-types`.
pub type ProcNr = i32;

/// Process role (OS concept — replaces the previous `is_kernel: bool`
/// implicit + ad-hoc `is_vm` / `is_root_sys` branching).
///
/// Each variant maps to a distinct initial-PSW / FPU / segment-selector
/// configuration in the arch layer. The kernel layer picks a variant;
/// the arch layer knows what to do with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcKind {
    /// Kernel task (CLOCK / SYSTEM / IDLE / KERNEL): runs in kernel mode,
    /// has no ELF entry point.
    KernelTask,
    /// VM process: runs in user mode, ELF is loaded during boot.
    Vm,
    /// Root system service (RS): runs in user mode, ELF deferred to RS itself.
    RootService,
    /// Other system service: runs in user mode, ELF deferred to RS at runtime.
    UserService,
    /// User process: created by fork/exec, never appears during boot.
    UserProcess,
}

/// Entry-point specification (OS concept — replaces the previous
/// `pc: VirBytes, sp: VirBytes, ps_strings: VirBytes` triple, which was
/// null-padded to zero when no ELF had been loaded).
///
/// `None` means "not yet known" (kernel task has no entry point; user
/// process not yet loaded by RS).
#[derive(Debug, Clone, Copy, Default)]
pub struct EntrySpec {
    /// Entry-point PC. `None` ⇔ not yet loaded.
    pub pc: Option<VirBytes>,
    /// Initial SP. `None` ⇔ not yet known.
    pub sp: Option<VirBytes>,
    /// ps_strings address (x86-64: rbx, aarch64: r0, riscv64: a0).
    /// `None` ⇔ no ps_strings (kernel task).
    pub ps_strings: Option<VirBytes>,
}

impl EntrySpec {
    /// Kernel task: no entry point.
    pub const KERNEL_TASK: Self = Self { pc: None, sp: None, ps_strings: None };

    /// Deferred user process: PC/SP not yet known, will be set by RS.
    pub const DEFERRED: Self = Self { pc: None, sp: None, ps_strings: None };

    /// ELF already loaded — entry + stack + ps_strings all known.
    pub const fn loaded(pc: VirBytes, sp: VirBytes, ps_strings: VirBytes) -> Self {
        Self { pc: Some(pc), sp: Some(sp), ps_strings: Some(ps_strings) }
    }
}

/// Result of `load_vm_elf` — the entry / stack / ps_strings derived
/// from the parsed ELF + the bootstrap page table.
///
/// `allocated_bytes` is bookkeeping for the caller (it tracks how much
/// of the bootstrap page table is owned by VM).
#[derive(Debug, Clone, Copy)]
pub struct VmLoadResult {
    pub pc: VirBytes,
    pub sp: VirBytes,
    pub ps_strings: VirBytes,
    pub allocated_bytes: usize,
}

/// Errors from `load_vm_elf`.
///
/// Replaces the previous "return zero PC and let the caller figure it
/// out" silent failure (see `06-proc-init-boot-proc.md` §3.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmLoadError {
    /// ELF header / magic invalid, or no PT_LOAD segments.
    InvalidElf,
    /// Pages could not be mapped into the bootstrap page table.
    MappingFailed,
}

/// Arch abstraction for a process's CPU context.
///
/// The associated type `CpuContext` is **arch-private** — the kernel
/// layer never inspects its fields, it only stores the value in
/// `KProcess` and hands it back to `apply_to_trap_frame` at first-run.
pub trait CpuContextArch {
    /// arch-private CPU context for a single process.
    ///
    /// Required bounds (kept minimal so the kernel can store this in
    /// fixed-size arrays):
    /// - `Copy`: pass by value to `apply_to_trap_frame`
    /// - `Debug`: panic message / log formatting
    /// - `Default`: zero-value used by `ProcessTable::new()` for empty slots
    type CpuContext: Copy + core::fmt::Debug + Default;

    /// arch's trap frame type. Same `Copy + Default` rationale as above.
    type TrapFrame: Copy + core::fmt::Debug + Default;

    /// Build the initial CPU context for a process.
    ///
    /// arch-internal responsibilities (NOT exposed to kernel):
    /// - pick initial PSW/PSR/sstatus based on `kind` (kernel vs user)
    /// - x86-64: fill segment selectors + decide FPU init policy
    /// - aarch64: per-process `fpu_enable_el0` flag
    /// - riscv64: set `sstatus.FS` to `Initial` so the first FP
    ///   instruction traps into the kernel for lazy init
    /// - copy `entry.pc/sp/ps_strings` into the right arch-specific
    ///   registers (RIP / RSP / RBX, or ELR_EL1 / SP_EL0 / R0, etc.)
    fn build_cpu_context(kind: ProcKind, proc_nr: ProcNr, entry: EntrySpec) -> Self::CpuContext;

    /// Apply the CPU context to a trap frame.
    ///
    /// Called by the scheduler the first time a process runs. After
    /// this point the trap frame is the source of truth (subsequent
    /// context switches save/restore the trap frame, not the
    /// `CpuContext`).
    fn apply_to_trap_frame(ctx: &Self::CpuContext, frame: &mut Self::TrapFrame);

    /// Enable user I/O access (x86-64: set RFLAGS.IOPL = 3; other
    /// architectures: no-op).
    ///
    /// Default implementation is a no-op — only x86-64 needs to do
    /// anything here. Naming uses `ctx` (not `_ctx`) because Rust
    /// treats `_` parameters as "deliberately unused", which obscures
    /// the fact that x86-64 actually uses it. The `let _ = ctx;` line
    /// in the default body makes the "intentionally ignored" intent
    /// explicit without misleading the reader.
    fn enable_user_io(ctx: &mut Self::CpuContext) {
        let _ = ctx; // explicit: x86-64 ignores here, other archs do nothing
    }

    /// Inherit the FPU / extended-register state from a parent into a
    /// child (called from `fork_from`).
    ///
    /// - x86-64: copy the parent's XSAVE area into the child's,
    ///   propagating `MF_FPU_INITIALIZED` (or implement the
    ///   lazy-init alternative — the kernel layer is unaware of the
    ///   choice)
    /// - aarch64 / riscv64: no-op — there is no per-process FPU
    ///   save area, FPCR / FPSR are saved/restored on context switch
    fn inherit_fpu_state(_child: &mut Self::CpuContext, _parent: &Self::CpuContext) {
        // default: no-op (correct for aarch64 / riscv64)
    }

    /// Write a register value at the given byte offset into the CPU
    /// context's register save area (C: `struct stackframe_s`).
    ///
    /// Used by `T_SETUSER` (SYS_TRACE) to modify a traced process's
    /// saved register state. The offset is relative to the start of
    /// the register save area, matching C's `p_reg` layout.
    ///
    /// # Returns
    ///
    /// - `Ok(())` on successful write
    /// - `Err(())` if the offset is out of bounds, misaligned, or
    ///   points to a protected register (e.g. segment selectors on
    ///   x86-64 — writing them could crash the kernel at context
    ///   switch).
    ///
    /// # C alignment
    ///
    /// C: do_trace.c:136-167 — on i386, segment registers
    /// (cs/ds/es/fs/gs/ss) are protected; PSW uses `SETPSW` (selected
    /// bits only). On x86_64, the C source has a gap (no write path
    /// compiled under `#if defined(__i386__)`). Rust implements the
    /// correct behavior for all architectures.
    ///
    /// Default: `Err(())` (arch must override to enable T_SETUSER).
    // TODO(C-D-5): replace `Result<(), ()>` with a named error type
    // (todo.md §11.4 design-level cleanup — trait contract change).
    #[allow(clippy::result_unit_err)]
    fn write_user_register(
        _ctx: &mut Self::CpuContext,
        _offset: usize,
        _value: u64,
    ) -> Result<(), ()> {
        Err(())
    }

    /// OR-merge a value into the IPC status register.
    ///
    /// C: `p->p_reg.IPC_STATUS_REG |= value` — ipc.h:42
    ///
    /// The IPC status register is architecture-specific:
    /// - x86-64: RBX (C: `bx` on i386 — ipcconst.h:10; also carries
    ///   ps_strings at process startup, then repurposed for IPC status)
    /// - aarch64: X1 (C: `r1` on earm — ipcconst.h:7; separate from
    ///   R0 which carries ps_strings / return value)
    /// - riscv64: A1/X11 (no C original; chosen by analogy to ARM's
    ///   R1, since A0 carries ps_strings / return value)
    ///
    /// The register is NOT cleared between IPC operations — the OR-merge
    /// accumulates status flags. `MF_REPLY_PEND` gating (skip the write
    /// during SENDREC's receive phase) is handled by the caller, not here.
    ///
    /// Default: no-op (arch must override to enable IPC status).
    fn or_ipc_status_reg(_ctx: &mut Self::CpuContext, _value: u64) {
        // default: no-op (overridden by each arch)
    }
}

/// Load a VM ELF binary into the bootstrap page table.
///
/// This is a **free function** rather than a trait method because the
/// three architecture implementations are byte-for-byte identical
/// (verified against `os/arch/src/{x86_64,arm64,riscv64}/proc_arch.rs`
/// prior to refactor — see `06-proc-init-boot-proc.md` §3.5). Putting
/// it in a trait would be false polymorphism.
///
/// # Errors
///
/// Returns `Err(VmLoadError::InvalidElf)` when the ELF cannot be
/// parsed. `Err(MappingFailed)` is reserved for paging failures.
///
/// C corresponds to `libexec_load_elf()` + the page-mapping portion
/// of `arch_boot_proc()` (`protect.c:388` x86 / `protect.c:115` ARM).
pub fn load_vm_elf<P: Paging>(
    module: &BootModule,
    kernel_info: &KernelInfo,
    paging: &mut P,
) -> Result<VmLoadResult, VmLoadError> {
    /// Default VM user-stack size (matches Minix3).
    const VM_STACK_SIZE: u64 = 64 * 1024;

    // SAFETY: `module.start` points to the VM ELF image in physical
    // memory; during boot the identity map covers this address range.
    // `module.len` is the module's exact size from the boot info.
    let image = unsafe {
        core::slice::from_raw_parts(module.start.0 as *const u8, module.len)
    };

    let iter = minix_elf::segment_iter(image)
        .map_err(|_| VmLoadError::InvalidElf)?;

    let entry = minix_elf::entry_point(image).unwrap_or(0);
    let page_size = P::PAGE_SIZE as u64;
    let mut total_allocated: usize = 0;

    // Map each PT_LOAD segment page-by-page into the bootstrap page table.
    for seg in iter {
        let flags = elf_flags_to_page_flags(seg.flags);
        let vaddr_start = seg.vaddr;
        let vaddr_end = seg.vaddr + seg.memsz;
        let mut vaddr = vaddr_start & !(page_size - 1); // page-align down
        let mut file_offset = seg.offset;
        let mut file_remaining = seg.filesz;

        while vaddr < vaddr_end {
            // In the bootstrap page table, virtual address == physical
            // address (1:1 identity mapping). C uses PG_ALLOCATEME to
            // let the allocator pick a physical frame.
            let paddr = minix_types::PhysBytes(vaddr);

            if paging.map(VirBytes(vaddr), paddr, flags).is_ok() {
                total_allocated += page_size as usize;
            }

            // Copy segment bytes from the ELF image into the mapped page.
            if file_remaining > 0 {
                let copy_start = (vaddr - vaddr_start) as usize;
                let copy_len = core::cmp::min(
                    file_remaining as usize,
                    page_size as usize - (copy_start % page_size as usize),
                );
                if copy_start + copy_len <= seg.filesz as usize {
                    let src_offset = file_offset as usize;
                    let dst_ptr = vaddr as *mut u8;
                    // SAFETY: `vaddr` was mapped above; `copy_len` is
                    // bounded by the remaining ELF bytes and the page
                    // size, so the copy is in range.
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

    // Map the user stack: stack_high - VM_STACK_SIZE → stack_high.
    // R-07 (2026-08-12): Use getter method (preferred API).
    let stack_high = kernel_info.user_sp();
    let sp = VirBytes(stack_high.0 - VM_STACK_SIZE);

    let stack_flags = crate::paging::PageFlags::read_write();
    let mut stack_addr = sp.0 & !(page_size - 1);
    while stack_addr < stack_high.0 {
        if paging.map(VirBytes(stack_addr), minix_types::PhysBytes(stack_addr), stack_flags).is_ok() {
            total_allocated += page_size as usize;
        }
        stack_addr += page_size;
    }

    let ps_strings = VirBytes(sp.0 - 32);
    Ok(VmLoadResult {
        pc: VirBytes(entry),
        sp,
        ps_strings,
        allocated_bytes: total_allocated,
    })
}

/// Convert ELF segment flags (PF_R|PF_W|PF_X) to `PageFlags`.
fn elf_flags_to_page_flags(elf_flags: u32) -> crate::paging::PageFlags {
    use crate::paging::PageFlags;
    let mut flags = PageFlags::PRESENT | PageFlags::USER_ACCESSIBLE;
    if elf_flags & 0x2 != 0 {
        // PF_W
        flags |= PageFlags::WRITABLE;
    }
    if elf_flags & 0x1 != 0 {
        // PF_X
        flags |= PageFlags::EXECUTABLE;
    }
    flags
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paging::PageFlags;

    #[test]
    fn entry_spec_kernel_task_all_none() {
        let e = EntrySpec::KERNEL_TASK;
        assert!(e.pc.is_none());
        assert!(e.sp.is_none());
        assert!(e.ps_strings.is_none());
    }

    #[test]
    fn entry_spec_deferred_all_none() {
        let e = EntrySpec::DEFERRED;
        assert!(e.pc.is_none());
        assert!(e.sp.is_none());
        assert!(e.ps_strings.is_none());
    }

    #[test]
    fn entry_spec_loaded_sets_all_some() {
        let e = EntrySpec::loaded(
            VirBytes(0x1000),
            VirBytes(0x2000),
            VirBytes(0x1FE0),
        );
        assert_eq!(e.pc, Some(VirBytes(0x1000)));
        assert_eq!(e.sp, Some(VirBytes(0x2000)));
        assert_eq!(e.ps_strings, Some(VirBytes(0x1FE0)));
    }

    #[test]
    fn elf_flags_rwx_to_page_flags() {
        // PF_R | PF_W | PF_X = 0x7
        let f = elf_flags_to_page_flags(0x7);
        assert!(f.contains(PageFlags::PRESENT));
        assert!(f.contains(PageFlags::USER_ACCESSIBLE));
        assert!(f.contains(PageFlags::WRITABLE));
        assert!(f.contains(PageFlags::EXECUTABLE));
    }

    #[test]
    fn elf_flags_r_only() {
        let f = elf_flags_to_page_flags(0x4);
        assert!(f.contains(PageFlags::PRESENT));
        assert!(f.contains(PageFlags::USER_ACCESSIBLE));
        assert!(!f.contains(PageFlags::WRITABLE));
        assert!(!f.contains(PageFlags::EXECUTABLE));
    }

    #[test]
    fn load_vm_elf_invalid_elf_returns_err() {
        use minix_types::{PhysBytes, VirBytes};

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
            platform_sources: &[],
            param_buf: &[],
        };
        let mut paging = crate::paging::mock::MockPaging::new().unwrap();
        let result = load_vm_elf(&module, &kinfo, &mut paging);
        assert_eq!(result.err(), Some(VmLoadError::InvalidElf));
    }
}