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
use minix_types::{PhysFrame, VirBytes};

use crate::paging::Paging;
use crate::arch::frame::{PhysAccess, VmBootAllocator};

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
    /// `VmBootAllocator` ran out of frames for the VM image.
    OutOfMemory,
    /// Pages could not be mapped into the bootstrap page table.
    MappingFailed,
}

/// Why a user register write (T_SETUSER) failed.
///
/// Replaces the former `Result<(), ()>` (C-D-5): the arch layer knows the
/// reason, the syscall layer decides the errno (both variants map to
/// EFAULT today, matching C's do_trace.c).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteUserRegError {
    /// Offset is misaligned or outside the register file.
    BadAddress,
    /// Register is protected from user writes (segment selectors on
    /// x86-64 — altering them could crash the kernel on restore).
    Protected,
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
    /// Default: `Err(WriteUserRegError::Protected)` (arch must override
    /// to enable T_SETUSER).
    fn write_user_register(
        _ctx: &mut Self::CpuContext,
        _offset: usize,
        _value: u64,
    ) -> Result<(), WriteUserRegError> {
        Err(WriteUserRegError::Protected)
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
/// # Parameters
///
/// - `vm_alloc`: a one-shot allocator cutting *physical* frames for the
///   VM image out of a verified [`VmBootRegion`]. `load_vm_elf` chooses
///   the PA; the caller constructs `VmBootAllocator` from a
///   `VmBootRegion` the kernel has already verified (`VmBootRegion` ∩
///   reserved = ∅). PA is thus decoupled from the ELF VA — the load
///   does **not** assume `paddr == vaddr`.
/// - `access`: converts each allocated frame's PA to a kernel-accessible
///   VA so the loader can zero + copy file bytes. `DirectMapArch` (via
///   `CurrentDirectMap`) is the production implementation; tests use a
///   mock.
///
/// # Errors
///
/// Returns `Err(VmLoadError::InvalidElf)` when the ELF cannot be
/// parsed. `Err(VmLoadError::OutOfMemory)` when `vm_alloc` is
/// exhausted. `Err(MappingFailed)` is reserved for paging failures.
///
/// C corresponds to `libexec_load_elf()` + the page-mapping portion
/// of `arch_boot_proc()` (`protect.c:388` x86 / `protect.c:115` ARM).
/// C picks frames via `pg_map(PG_ALLOCATEME, ...)` →
/// `pg_alloc_page(cbi)` (pg_utils.c:276-289); this function picks them
/// via `VmBootAllocator`.
///
/// Design: VM Bootstrap Memory Handoff in [`frame.rs`](frame.rs). Per page
/// the loader does `alloc → zero → copy → map`: the frame is fully constructed
/// before it enters the VM address space.
pub fn load_vm_elf<P: Paging, A: PhysAccess>(
    module: &BootModule,
    kernel_info: &KernelInfo,
    paging: &mut P,
    vm_alloc: &mut VmBootAllocator,
    access: &A,
) -> Result<VmLoadResult, VmLoadError> {
    /// Default VM user-stack size (matches Minix3).
    const VM_STACK_SIZE: u64 = 64 * 1024;

    /// `sizeof(struct ps_strings)` on 64-bit LP64: two pointers (16 B)
    /// + two ints (8 B) = 24 B, padded to 8-byte alignment → 32 B.
    ///
    /// C: protect.c:413 (`sp -= sizeof(struct ps_strings)`).
    const PS_STRINGS_SIZE: u64 = 32;

    /// The three words below ps_strings — argc, argv, envp: two
    /// pointers + one int. C: protect.c:417.
    const ARGC_ARGV_ENVP: u64 =
        2 * core::mem::size_of::<usize>() as u64 + core::mem::size_of::<i32>() as u64;

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

    // Map each PT_LOAD segment page-by-page. Direction D: the allocator
    // picks a *physical* frame each page; the ELF VA is mapped onto it
    // (PA ≠ VA by design). Per page the loader does
    // `alloc → zero → copy → map` so a frame is fully constructed before
    // it enters the VM address space.
    for seg in iter {
        let flags = elf_flags_to_page_flags(seg.flags);
        let page_mask = page_size - 1;
        let vaddr_start = seg.vaddr;
        let vaddr_end = seg.vaddr + seg.memsz;
        // C contract: `pg_map` asserts `!(vaddr % PAGE_SIZE)` for
        // PG_ALLOCATEME (pg_utils.c:277). Link-script-produced ELF
        // vaddrs are page-aligned; a non-aligned vaddr would make
        // `copy_start = vaddr - vaddr_start` below underflow, so enforce
        // the same contract as C (debug builds).
        debug_assert!(
            vaddr_start % page_size == 0,
            "load_vm_elf: PT_LOAD vaddr must be page-aligned (C: pg_utils.c:277)"
        );
        let mut vaddr = vaddr_start & !page_mask; // page-align down
        let mut file_offset = seg.offset;
        let mut file_remaining = seg.filesz;

        while vaddr < vaddr_end {
            // ① Allocate a physical frame (PA chosen by the allocator,
            //    not derived from the VA).
            let frame = vm_alloc
                .alloc_page()
                .map_err(|_| VmLoadError::OutOfMemory)?;
            // ② Kernel-accessible destination for zero + copy.
            //    `PhysAccess` guarantees the whole frame is accessible.
            let dst = access.frame_virt(frame);

            // ③ Zero the entire frame first: covers unaligned first/last
            //    pages and `.bss` (memsz > filesz) tail.
            //    SAFETY: `dst` is a kernel-accessible VA covering one
            //    whole frame (PhysAccess contract); PAGE_SIZE bytes are
            //    in range.
            unsafe {
                core::ptr::write_bytes(dst.0 as *mut u8, 0, page_size as usize);
            }

            // ④ Copy file-backed bytes for this page (may be partial).
            if file_remaining > 0 {
                let copy_start = (vaddr - vaddr_start) as usize;
                let copy_len = core::cmp::min(
                    file_remaining as usize,
                    page_size as usize - ((vaddr - vaddr_start) % page_size) as usize,
                );
                if copy_start + copy_len <= seg.filesz as usize {
                    let src_offset = file_offset as usize;
                    let dst_ptr = dst.0 as *mut u8;
                    // SAFETY: `dst` is in range (verified by PhysAccess);
                    // `copy_len` is bounded by the remaining ELF bytes
                    // and the page size.
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

            // ⑤ Install the VM mapping only after the frame is fully
            //    constructed: VM VA → allocated PA.
            paging
                .map(VirBytes(vaddr), frame.start(), flags)
                .map_err(|_| VmLoadError::MappingFailed)?;
            total_allocated += page_size as usize;

            vaddr += page_size;
        }
    }

    // Map the user stack: [stack_high - VM_STACK_SIZE, stack_high).
    // C: `execi.stack_size = 64 * 1024` (protect.c:402) — libexec
    // preallocates the same 64 KiB region.
    // R-07 (2026-08-12): Use getter method (preferred API).
    let stack_high = kernel_info.user_sp();
    let stack_bottom = stack_high.0 - VM_STACK_SIZE;

    let stack_flags = crate::paging::PageFlags::read_write();
    let stack_align = stack_bottom & !(page_size - 1);
    let mut stack_addr = stack_align;
    while stack_addr < stack_high.0 {
        // User stack frames also come from the bootstrap allocator
        // (direction D: PA is chosen, not assumed VA == PA).
        let frame = vm_alloc
            .alloc_page()
            .map_err(|_| VmLoadError::OutOfMemory)?;
        let dst = access.frame_virt(frame);
        // Zero stack page (the ps_strings/copy below assume clean pages).
        // SAFETY: `dst` covers a full frame (PhysAccess contract).
        unsafe {
            core::ptr::write_bytes(dst.0 as *mut u8, 0, page_size as usize);
        }
        paging
            .map(VirBytes(stack_addr), frame.start(), stack_flags)
            .map_err(|_| VmLoadError::MappingFailed)?;
        total_allocated += page_size as usize;
        stack_addr += page_size;
    }

    // Initial stack layout, mirroring C's `arch_boot_proc`
    // (protect.c:410-427): the ps_strings struct sits 32 bytes below
    // `stack_high`, and the SP is taken down a further 20 bytes (two
    // pointers + one int) so the startup code sees argc/argv/envp.
    let ps_strings = VirBytes(stack_high.0 - PS_STRINGS_SIZE); // stack_high - 32
    let sp = VirBytes(ps_strings.0 - ARGC_ARGV_ENVP);          // stack_high - 52

    // Resolve the kernel VA of the *top* stack page (`stack_high -
    // page_size`). The bootstrap allocator's frame order is
    // implementation-defined (currently descending — see frame.rs);
    // asking the paging implementation avoids depending on which
    // frame the loop happened to allocate last. Mirrors C's field
    // assignments (protect.c:419-422): `ps_argvstr` points at the
    // argc word, `ps_envstr` at the argv word, both count fields are
    // 0 (no arguments or environment are passed to VM).
    //
    // SAFETY: `ps_strings` and `sp` sit inside the top stack page
    // that the loop above mapped. The stores below write into the
    // frame-backing memory, not the (possibly unmapped) VM VA.
    let top_page_va = VirBytes(stack_high.0 - page_size);
    let top_page_pa = paging.query(top_page_va)
        .ok_or(VmLoadError::MappingFailed)?
        .0;
    let top_frame_kva = access.frame_virt(PhysFrame::new(top_page_pa));
    unsafe {
        let frame_offset = ps_strings.0 - (stack_high.0 - page_size);
        let psp = (top_frame_kva.0 + frame_offset) as *mut u8;
        let argvstr = sp.0 + core::mem::size_of::<i32>() as u64;     // stack_high - 48
        let envstr = argvstr + core::mem::size_of::<usize>() as u64; // stack_high - 40
        (psp as *mut u64).write(argvstr);        // ps_argvstr
        (psp.add(8) as *mut i32).write(0);       // ps_nargvstr
        (psp.add(16) as *mut u64).write(envstr); // ps_envstr
        (psp.add(24) as *mut i32).write(0);      // ps_nenvstr
    }

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

        // Frame source and access mock — the ELF is invalid, so the
        // allocator/access are never actually used; they just satisfy
        // the signature.
        let region = crate::arch::frame::VmBootRegion::new(
            PhysBytes(0x1000),
            PhysBytes(0x1_0000),
        )
        .unwrap();
        let regions = crate::arch::frame::VmBootRegions::from_sorted(&[region])
            .expect("test: single region is descending & disjoint");
        let mut vm_alloc = crate::arch::frame::VmBootAllocator::new(regions);
        struct NoAccess;
        impl crate::arch::frame::PhysAccess for NoAccess {
            fn phys_to_virt(&self, _phys: PhysBytes) -> VirBytes {
                VirBytes(0)
            }
        }
        let access = NoAccess;
        let result = load_vm_elf(&module, &kinfo, &mut paging, &mut vm_alloc, &access);
        assert_eq!(result.err(), Some(VmLoadError::InvalidElf));
    }

    /// Minimal valid ELF64 image: one PT_LOAD segment with
    /// filesz = memsz = 0, so `load_vm_elf` maps/copies nothing for the
    /// segments — only the stack setup (the part under test) runs.
    fn build_minimal_elf() -> [u8; 0x80] {
        let mut image = [0u8; 0x80];

        // ELF64 header (64 bytes).
        image[0..4].copy_from_slice(&[0x7f, b'E', b'L', b'F']);
        image[4] = 2; // ELFCLASS64
        image[5] = 1; // ELFDATA2LSB
        image[6] = 1; // EI_VERSION
        image[16..18].copy_from_slice(&2u16.to_le_bytes());   // ET_EXEC
        image[18..20].copy_from_slice(&62u16.to_le_bytes());  // EM_X86_64
        image[20..24].copy_from_slice(&1u32.to_le_bytes());   // e_version
        image[32..40].copy_from_slice(&64u64.to_le_bytes());  // e_phoff
        image[52..54].copy_from_slice(&64u16.to_le_bytes());  // e_ehsize
        image[54..56].copy_from_slice(&56u16.to_le_bytes());  // e_phentsize
        image[56..58].copy_from_slice(&1u16.to_le_bytes());   // e_phnum

        // One PT_LOAD program header (56 bytes at offset 64).
        let ph = &mut image[64..120];
        ph[0..4].copy_from_slice(&1u32.to_le_bytes());        // PT_LOAD
        ph[4..8].copy_from_slice(&5u32.to_le_bytes());        // PF_R|PF_X
        ph[48..56].copy_from_slice(&0x1000u64.to_le_bytes()); // p_align
        // p_offset / p_vaddr / p_paddr / p_filesz / p_memsz stay 0.

        image
    }

    #[test]
    fn load_vm_elf_places_stack_and_ps_strings_like_c() {
        use minix_types::{PhysBytes, PhysFrame, VirBytes};
        use crate::arch::frame::{
            PhysAccess, VmBootAllocator, VmBootRegion, VmBootRegions,
        };

        let image = build_minimal_elf();
        let module = BootModule {
            name: "fake-vm",
            start: PhysBytes(image.as_ptr() as u64),
            len: image.len(),
        };

        // Mock the kernel direct-map window: a heap-allocated buffer whose base
        // is the "kernel VA" of PA 0. Frame PA 0x1000.. maps onto
        // buf+0x1000..., so `PhysAccess::phys_to_virt` is a linear offset.
        let buf: Box<[u8]> = vec![0u8; 0x200000].into_boxed_slice(); // 2 MiB
        let dm_base = buf.as_ptr() as u64;

        struct MockAccess {
            dm_base: u64,
        }
        impl PhysAccess for MockAccess {
            fn phys_to_virt(&self, phys: PhysBytes) -> VirBytes {
                VirBytes(self.dm_base + phys.get())
            }
        }
        let access = MockAccess { dm_base };

        // Region [0x1000, 0x200000) carries the entire VM image plus
        // the 64 KiB stack — enough for this minimal-ELF test.
        let region = VmBootRegion::new(PhysBytes(0x1000), PhysBytes(0x200000)).unwrap();
        let regions = VmBootRegions::from_sorted(&[region])
            .expect("test: single region is descending & disjoint");
        let mut vm_alloc = VmBootAllocator::new(regions);

        let user_sp = VirBytes(0x19000); // VM VA stack high

        let kinfo = KernelInfo {
            memmap: &[minix_boot::MemoryRegion {
                base: PhysBytes(0x1000),
                len: 0x200000,
            }],
            kern_virt_base: VirBytes(0xffff_8000_0000_0000),
            kern_phys_base: PhysBytes(0),
            kern_size: 0,
            free_upper_idx: None,
            user_sp,
            kern_stack_top: VirBytes(0),
            syscall_entry: VirBytes(0),
            boot_modules: &[],
            bootstrap_start: PhysBytes(0),
            bootstrap_len: 0,
            platform_sources: &[],
            param_buf: &[],
        };
        let mut paging = crate::paging::mock::MockPaging::new().unwrap();

        let result = load_vm_elf(&module, &kinfo, &mut paging, &mut vm_alloc, &access)
            .expect("minimal ELF must load");

        // Public geometry the loader contract promises.
        assert_eq!(result.ps_strings.0, user_sp.0 - 32); // stack_high - 32
        assert_eq!(result.sp.0, user_sp.0 - 52); // stack_high - (32 + 20)
        assert!(result.allocated_bytes >= 64 * 1024);

        // ---- happy path through `paging.query` + `PhysAccess` ----
        //
        // The loader writes ps_strings via:
        //   paging.query(stack_high - page_size) → top stack PA
        //   access.frame_virt(top PA)           → top stack KVA
        //   *(KVA + (ps_strings - (stack_high - page_size))) = ps_argvstr etc.
        // Reproduce that exact lookup and read back the four ps_strings
        // fields. This proves the production path actually stores the
        // expected values at the right offsets.
        let top_va = VirBytes(user_sp.0 - 0x1000);
        let (top_pa, _flags) = paging
            .query(top_va)
            .expect("top stack page must be mapped");
        let top_frame_kva = access.frame_virt(PhysFrame::new(top_pa));
        let frame_offset = result.ps_strings.0 - (user_sp.0 - 0x1000);
        let psp = (top_frame_kva.0 + frame_offset) as *const u8;
        // SAFETY: `psp` points into `buf` (the mock direct-map window)
        // at the offset the loader just wrote; the bytes are live
        // (`load_vm_elf` holds the only reference and the buffer is
        // not dropped before this read).
        unsafe {
            // C `arch_boot_proc` (protect.c:419-422):
            //   ps_argvstr = sp + sizeof(int)          (stack_high - 48)
            //   ps_nargvstr = 0
            //   ps_envstr  = ps_argvstr + sizeof(void *)
            //   ps_nenvstr = 0
            // The loader writes these four fields; the startup code
            // (later, outside this loader) pushes argc/argv/envp at
            // addresses below ps_strings when VM actually starts —
            // that is *not* the loader's responsibility.
            let argvstr = result.sp.0 + core::mem::size_of::<i32>() as u64;
            let envstr = argvstr + core::mem::size_of::<usize>() as u64;
            assert_eq!((psp as *const u64).read(), argvstr);
            assert_eq!((psp.add(8) as *const i32).read(), 0);
            assert_eq!((psp.add(16) as *const u64).read(), envstr);
            assert_eq!((psp.add(24) as *const i32).read(), 0);
        }
    }

    /// Builds an ELF64 with one PT_LOAD: vaddr aligned, filesz spans
    /// partial pages, memsz > filesz (`.bss`).
    ///
    /// Layout: phdr at 0x40, file data at offset 0x200.. (immediately
    /// after the phdr), `filesz` bytes of 0xAB markers, then `.bss`.
    /// `image` is 0x2000 bytes — filesz must keep `0x200 + filesz <= 0x2000`.
    fn build_bss_elf(filesz: u64, memsz: u64, vaddr: u64) -> [u8; 0x2000] {
        assert!(0x200 + filesz as usize <= 0x2000, "filesz too large");
        let mut image = [0u8; 0x2000];
        image[0..4].copy_from_slice(&[0x7f, b'E', b'L', b'F']);
        image[4] = 2;
        image[5] = 1;
        image[6] = 1;
        image[16..18].copy_from_slice(&2u16.to_le_bytes());   // ET_EXEC
        image[18..20].copy_from_slice(&62u16.to_le_bytes());  // EM_X86_64
        image[20..24].copy_from_slice(&1u32.to_le_bytes());
        image[32..40].copy_from_slice(&64u64.to_le_bytes());  // e_phoff
        image[52..54].copy_from_slice(&64u16.to_le_bytes());  // e_ehsize
        image[54..56].copy_from_slice(&56u16.to_le_bytes());  // e_phentsize
        image[56..58].copy_from_slice(&1u16.to_le_bytes());   // e_phnum
        let ph = &mut image[64..120];
        ph[0..4].copy_from_slice(&1u32.to_le_bytes());        // PT_LOAD
        ph[4..8].copy_from_slice(&6u32.to_le_bytes());        // PF_R|PF_W
        ph[8..16].copy_from_slice(&0x200u64.to_le_bytes());   // p_offset
        ph[16..24].copy_from_slice(&vaddr.to_le_bytes());     // p_vaddr
        ph[24..32].copy_from_slice(&vaddr.to_le_bytes());     // p_paddr
        ph[32..40].copy_from_slice(&filesz.to_le_bytes());    // p_filesz
        ph[40..48].copy_from_slice(&memsz.to_le_bytes());     // p_memsz
        ph[48..56].copy_from_slice(&0x1000u64.to_le_bytes()); // p_align
        // Marker bytes at the file offset so the copy is observable.
        for i in 0..filesz as usize {
            image[0x200 + i] = 0xAB;
        }
        image
    }

    #[test]
    fn load_vm_elf_zeroes_bss_and_copies_filesz() {
        use minix_types::{PhysBytes, VirBytes};
        use crate::arch::frame::{PhysAccess, VmBootAllocator, VmBootRegion};

        // filesz = 0x1800 (> PAGE_SIZE), memsz = 0x3000 (> filesz).
        // Layout:
        //   page0 [0x400000, 0x401000): 0x1000 file bytes (all 0xAB)
        //   page1 [0x401000, 0x402000): 0x800 file + 0x800 bss
        //   page2 [0x402000, 0x403000): 0x1000 bss (zero)
        let filesz = 0x1800;
        let memsz = 0x3000;
        let vaddr = 0x400000u64;
        let image = build_bss_elf(filesz, memsz, vaddr);
        let module = BootModule {
            name: "fake-vm",
            start: PhysBytes(image.as_ptr() as u64),
            len: image.len(),
        };

        // Mock direct map: heap buffer, PA 0x1000.. maps onto
        // buf+0x1000... (vec! avoids the 2 MiB array on the test stack).
        let buf: Box<[u8]> = vec![0u8; 0x200000].into_boxed_slice();
        let dm_base = buf.as_ptr() as u64;
        struct MockAccess {
            dm_base: u64,
        }
        impl PhysAccess for MockAccess {
            fn phys_to_virt(&self, phys: PhysBytes) -> VirBytes {
                VirBytes(self.dm_base + phys.get())
            }
        }
        let access = MockAccess { dm_base };

        let region = VmBootRegion::new(PhysBytes(0x1000), PhysBytes(0x200000)).unwrap();
        let regions = crate::arch::frame::VmBootRegions::from_sorted(&[region])
            .expect("test: single region is descending & disjoint");
        let mut vm_alloc = VmBootAllocator::new(regions);

        let kinfo = KernelInfo {
            memmap: &[minix_boot::MemoryRegion {
                base: PhysBytes(0x1000),
                len: 0x200000,
            }],
            kern_virt_base: VirBytes(0xffff_8000_0000_0000),
            kern_phys_base: PhysBytes(0),
            kern_size: 0,
            free_upper_idx: None,
            user_sp: VirBytes(0x19000),
            kern_stack_top: VirBytes(0),
            syscall_entry: VirBytes(0),
            boot_modules: &[],
            bootstrap_start: PhysBytes(0),
            bootstrap_len: 0,
            platform_sources: &[],
            param_buf: &[],
        };
        let mut paging = crate::paging::mock::MockPaging::new().unwrap();

        load_vm_elf(&module, &kinfo, &mut paging, &mut vm_alloc, &access)
            .expect("bss ELF must load");

        // `VmBootAllocator` hands out frames in descending order (from
        // `region.end` downward); the loader's per-page loop calls
        // `alloc_page()` in the order the VA range advances, so the
        // first mapped ELF page (lowest VA) is at the highest PA. Pages
        // live at PA 0x1FF000, 0x1FE000, 0x1FD000 (one PAGE_SIZE step
        // apart, descending).
        let p0 = (dm_base + 0x1FF000) as *const u8;
        unsafe {
            for i in 0..0x1000usize {
                assert_eq!(*p0.add(i), 0xAB, "page0[{}] not file byte", i);
            }
            // page1: [0, 0x800) file, [0x800, 0x1000) zeroed.
            let p1 = (dm_base + 0x1FE000) as *const u8;
            for i in 0..0x800usize {
                assert_eq!(*p1.add(i), 0xAB, "page1[{}] not file byte", i);
            }
            for i in 0x800..0x1000usize {
                assert_eq!(*p1.add(i), 0, "page1[{}] bss not zero", i);
            }
            // page2: all zero.
            let p2 = (dm_base + 0x1FD000) as *const u8;
            for i in 0..0x1000usize {
                assert_eq!(*p2.add(i), 0, "page2[{}] bss not zero", i);
            }
        }

        // Paging must map VM VA → the allocated PA (VA != PA here):
        // vaddr 0x400000 → PA 0x1FF000 (highest PA), vaddr+1 →
        // 0x1FE000, etc.
        let (pa0, _) = paging.query(VirBytes(vaddr)).expect("vaddr mapped");
        assert_eq!(pa0, PhysBytes(0x1FF000));
        let (pa1, _) = paging.query(VirBytes(vaddr + 0x1000)).expect("vaddr+1 mapped");
        assert_eq!(pa1, PhysBytes(0x1FE000));
        let (pa2, _) = paging.query(VirBytes(vaddr + 0x2000)).expect("vaddr+2 mapped");
        assert_eq!(pa2, PhysBytes(0x1FD000));

        // Mapping was only installed for 3 segment pages — the stack is
        // separate. Query a non-segment address should be unmapped.
        assert!(paging.query(VirBytes(vaddr + 0x3000)).is_none());
    }

    /// Verify cross-region descending allocation: feed
    /// `load_vm_elf` two descending-ordered regions and confirm that
    /// the loader's per-page `alloc_page()` calls return frames from
    /// the high region first (descending PA within it), then from
    /// the low region (descending PA within it), and that the
    /// cumulative PA sequence is strictly descending across the
    /// region boundary.
    ///
    /// High region: PA [0x100000, 0x200000) — 256 frames. Low region:
    /// PA [0x010000, 0x011000) — 1 frame. The 3-frame BSS PT_LOAD
    /// and 16-frame 64 KiB stack both fit in the high region (19 ≤
    /// 256), so the low region is never consumed; this confirms the
    /// descending-within-region monotonicity. A separate
    /// frame-level test (`allocator_crosses_to_next_region_when_exhausted`)
    /// covers the cross-region jump.
    #[test]
    fn load_vm_elf_allocates_strictly_descending_across_regions() {
        use minix_types::{PhysBytes, VirBytes};
        use crate::arch::frame::{
            PhysAccess, VmBootAllocator, VmBootRegion, VmBootRegions,
        };

        // Same ELF as the bss test: 3 PT_LOAD pages at VA 0x400000.
        let filesz = 0x1800u64;
        let memsz = 0x3000u64;
        let vaddr = 0x400000u64;
        let image = build_bss_elf(filesz, memsz, vaddr);
        let module = BootModule {
            name: "fake-vm",
            start: PhysBytes(image.as_ptr() as u64),
            len: image.len(),
        };

        // Two regions: high (lots of room for ELF + 64 KiB stack) and
        // low (4 KiB scratch we never reach).
        let high = VmBootRegion::new(PhysBytes(0x100000), PhysBytes(0x200000))
            .expect("region must be valid");
        let low = VmBootRegion::new(PhysBytes(0x10000), PhysBytes(0x11000))
            .expect("region must be valid");
        let regions = VmBootRegions::from_sorted(&[high, low])
            .expect("test setup: descending disjoint");
        assert_eq!(regions.len(), 2);
        assert_eq!(regions[0].start, PhysBytes(0x100000));
        assert_eq!(regions[1].start, PhysBytes(0x010000));
        let mut vm_alloc = VmBootAllocator::new(regions);

        // 4 MiB mock direct map is enough for the high region.
        let buf: Box<[u8]> = vec![0u8; 0x400000].into_boxed_slice();
        let dm_base = buf.as_ptr() as u64;
        struct MockAccess {
            dm_base: u64,
        }
        impl PhysAccess for MockAccess {
            fn phys_to_virt(&self, phys: PhysBytes) -> VirBytes {
                VirBytes(self.dm_base + phys.get())
            }
        }
        let access = MockAccess { dm_base };

        let kinfo = KernelInfo {
            memmap: &[minix_boot::MemoryRegion {
                base: PhysBytes(0x10000),
                len: 0x1F0000,
            }],
            kern_virt_base: VirBytes(0xffff_8000_0000_0000),
            kern_phys_base: PhysBytes(0),
            kern_size: 0,
            free_upper_idx: None,
            user_sp: VirBytes(0x19000),
            kern_stack_top: VirBytes(0),
            syscall_entry: VirBytes(0),
            boot_modules: &[],
            bootstrap_start: PhysBytes(0),
            bootstrap_len: 0,
            platform_sources: &[],
            param_buf: &[],
        };
        let mut paging = crate::paging::mock::MockPaging::new().unwrap();

        load_vm_elf(&module, &kinfo, &mut paging, &mut vm_alloc, &access)
            .expect("multi-region ELF must load");

        // Helper: read VA → PA map and assert strictly descending
        // across the 3 ELF pages.
        let pa0 = paging.query(VirBytes(vaddr)).expect("page0 mapped").0;
        let pa1 = paging.query(VirBytes(vaddr + 0x1000))
            .expect("page1 mapped").0;
        let pa2 = paging.query(VirBytes(vaddr + 0x2000))
            .expect("page2 mapped").0;
        // All three came from the high region (it has 0x100 frames,
        // way more than 3 + 16 stack), so they are at PA
        // 0x1FF000, 0x1FE000, 0x1FD000 in that order.
        assert_eq!(pa0, PhysBytes(0x1FF000));
        assert_eq!(pa1, PhysBytes(0x1FE000));
        assert_eq!(pa2, PhysBytes(0x1FD000));
        // And strictly descending.
        assert!(pa0.0 > pa1.0 && pa1.0 > pa2.0);

        // Sanity: stack frames were all allocated from the high
        // region (plenty of room), so the allocator never crossed
        // into the low region (current_region_idx remains 0).
        assert_eq!(vm_alloc.current_region_idx(), 0);

        // Sanity: total hand-out equals the full ELF+stack footprint.
        let pages_consumed = 3 + 16;
        assert_eq!(vm_alloc.used_frames() as usize, pages_consumed);
    }

    #[test]
    fn load_vm_elf_out_of_memory_returns_err() {
        use minix_types::{PhysBytes, VirBytes};
        use crate::arch::frame::{PhysAccess, VmBootAllocator, VmBootRegion};

        // Minimal ELF: one PT_LOAD with filesz = memsz = 0 → the segment
        // loop allocates nothing, so only the stack loop allocates. With
        // a 1-frame region the first stack page succeeds and the second
        // fails → OutOfMemory.
        let image = build_minimal_elf();
        let module = BootModule {
            name: "fake-vm",
            start: PhysBytes(image.as_ptr() as u64),
            len: image.len(),
        };

        // Mock access that maps every PA onto one writable arena page
        // (the loader zeroes the frame that *does* get allocated).
        let mut arena = Box::new([0u8; 0x1000]);
        let arena_base = arena.as_mut_ptr() as u64;
        struct ArenaAccess {
            base: u64,
        }
        impl PhysAccess for ArenaAccess {
            fn phys_to_virt(&self, _phys: PhysBytes) -> VirBytes {
                VirBytes(self.base)
            }
        }
        let access = ArenaAccess { base: arena_base };

        // 1 page of frames: first stack page gets it, second → OOM.
        let region = VmBootRegion::new(PhysBytes(0x1000), PhysBytes(0x2000)).unwrap();
        let regions = crate::arch::frame::VmBootRegions::from_sorted(&[region])
            .expect("test: single region is descending & disjoint");
        let mut vm_alloc = VmBootAllocator::new(regions);

        let kinfo = KernelInfo {
            memmap: &[],
            kern_virt_base: VirBytes(0xffff_8000_0000_0000),
            kern_phys_base: PhysBytes(0),
            kern_size: 0,
            free_upper_idx: None,
            user_sp: VirBytes(0x19000),
            kern_stack_top: VirBytes(0),
            syscall_entry: VirBytes(0),
            boot_modules: &[],
            bootstrap_start: PhysBytes(0),
            bootstrap_len: 0,
            platform_sources: &[],
            param_buf: &[],
        };
        let mut paging = crate::paging::mock::MockPaging::new().unwrap();

        let result = load_vm_elf(&module, &kinfo, &mut paging, &mut vm_alloc, &access);
        assert_eq!(result.err(), Some(VmLoadError::OutOfMemory));
    }
}
