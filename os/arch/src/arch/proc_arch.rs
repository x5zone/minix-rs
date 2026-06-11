//! Process architecture abstraction
//!
//! Defines trait interfaces for architecture-specific process operations:
//! - `ArchProcReset`: Return safe default register state for a new process
//! - `ArchProcInit`: Return initial PC, SP, and ps_strings register values
//! - `BootProcArch`: Architecture-specific boot process initialization
//!
//! # Design decisions (see 05-proc-init-boot-proc.md §3)
//!
//! - **Pure functional trait design** (§3.1): Traits return values
//!   (`InitialRegState`, `InitialRegs`) rather than modifying process
//!   structs. The kernel layer owns process struct mutation.
//! - **Three-trait split** (§3.2-3.4): `ArchProcReset` handles register
//!   defaults, `ArchProcInit` handles entry-point setup, `BootProcArch`
//!   handles the full boot_proc flow including VM ELF loading.
//! - **OS-semantic types** (§3.1): `InitialRegState` and `InitialRegs`
//!   use OS-semantic names (status, segment_selectors, pc, sp) rather
//!   than arch-specific register names.

use minix_types::VirBytes;
use minix_boot::{KernelInfo, BootModule};
use crate::paging::Paging;

/// Initial register state for a new process (arch_proc_reset equivalent).
///
/// Returned by `ArchProcReset::initial_reg_state()`. The kernel layer
/// applies these values to the process's trap frame.
///
/// C: arch_proc_reset() sets PSW + segment selectors + FPU state
///
/// # Architecture mapping
///
/// | Field             | x86-64                    | ARM64              | RISC-V          |
/// |-------------------|---------------------------|--------------------|-----------------|
/// | status            | RFLAGS: IOPL=0, IF=1      | SPSR: EL0t/EL1h    | sstatus: SPP/SPIE|
/// | segment_selectors | CS=0x1B, DS/SS=0x23       | All zero (default) | All zero (default)|
/// | fpu_needs_zero    | true for user procs       | false (lazy)       | false (lazy)    |
#[derive(Debug, Clone, Copy)]
pub struct InitialRegState {
    /// Status register value (RFLAGS/x86-64, SPSR/aarch64, sstatus/riscv64).
    pub status: u64,
    /// Segment selectors (x86-64 only; zero for aarch64/riscv64).
    pub segment_selectors: SegmentSelectors,
    /// Whether the FPU/ExtReg state needs to be zeroed.
    pub fpu_needs_zero: bool,
}

/// x86-64 segment selectors. aarch64/riscv64 use Default (all zero).
#[derive(Debug, Clone, Copy, Default)]
pub struct SegmentSelectors {
    pub cs: u64,
    pub ds: u64,
    pub ss: u64,
    pub es: u64,
    pub fs: u64,
    pub gs: u64,
}

/// Initial PC, SP, and ps_strings register values (arch_proc_init equivalent).
///
/// Returned by `ArchProcInit::init_regs()`. The kernel layer writes these
/// into the process's trap frame.
///
/// C: arch_proc_init() sets pr->p_reg.pc, pr->p_reg.sp, pr->p_reg.bx/retreg/a0
#[derive(Debug, Clone, Copy)]
pub struct InitialRegs {
    /// Program counter (entry point).
    pub pc: VirBytes,
    /// Stack pointer.
    pub sp: VirBytes,
    /// ps_strings register value (x86-64: rbx, aarch64: r0, riscv64: a0).
    pub ps_strings_reg: u64,
}

/// Architecture abstraction for returning initial process register state.
///
/// Called during process table initialization and when a process slot
/// is being recycled. Returns the initial register state for the arch.
///
/// C: arch_proc_reset() — arch_system.c:146 (x86) / arch_system.c:42 (ARM)
pub trait ArchProcReset {
    /// Return the initial register state for a new process.
    ///
    /// The kernel layer applies the returned `InitialRegState` to the
    /// process's trap frame. This separation ensures arch layer doesn't
    /// need to know about the kernel's process struct layout.
    ///
    /// # Arguments
    ///
    /// * `is_kernel` - true for kernel tasks (p_nr < 0), false for user processes
    /// * `proc_nr` - process number, used for FPU state allocation on x86-64
    fn initial_reg_state(is_kernel: bool, proc_nr: i32) -> InitialRegState;
}

/// Architecture abstraction for returning initial process PC/SP/ps_strings.
///
/// Called after `ArchProcReset` to get the process's entry point and
/// stack pointer register values.
///
/// C: arch_proc_init() — memory.c:722 (x86) / memory.c:627 (ARM)
pub trait ArchProcInit: ArchProcReset {
    /// Return the initial PC, SP, and ps_strings register values.
    ///
    /// The kernel layer writes these into the process's trap frame.
    /// The caller is responsible for calling `ArchProcReset::initial_reg_state()`
    /// first to get the base register state.
    ///
    /// # Architecture mapping
    ///
    /// | Register  | x86-64 | ARM64 | RISC-V |
    /// |-----------|--------|-------|--------|
    /// | PC        | rip    | pc    | sepc   |
    /// | SP        | rsp    | sp    | sp     |
    /// | ps_strings| rbx    | r0    | a0     |
    ///
    /// # Arguments
    ///
    /// * `is_kernel` - true for kernel tasks
    /// * `proc_nr` - process number
    /// * `pc` - Entry point (virtual address)
    /// * `sp` - Stack pointer (virtual address)
    /// * `ps_strings` - Address of ps_strings struct on the stack
    fn init_regs(
        is_kernel: bool,
        proc_nr: i32,
        pc: VirBytes,
        sp: VirBytes,
        ps_strings: VirBytes,
    ) -> InitialRegs;
}

/// Result of loading a VM ELF binary into the bootstrap page table.
pub struct VmLoadResult {
    /// Entry point (virtual address) from the ELF header.
    pub pc: VirBytes,
    /// Stack pointer after ps_strings setup.
    pub sp: VirBytes,
    /// ps_strings address on the stack.
    pub ps_strings: VirBytes,
    /// Total bytes allocated for VM in the bootstrap page table.
    pub allocated_bytes: usize,
}

/// Architecture abstraction for boot process initialization.
///
/// Handles the architecture-specific aspects of initializing boot
/// processes. The most important operation is loading the VM ELF
/// binary into the bootstrap page table.
///
/// # Architecture mapping
///
/// All architectures share the same high-level flow:
/// 1. Skip kernel tasks (p_nr < 0)
/// 2. For VM: load ELF, set PC/SP/ps_strings
/// 3. For other user processes: no-op (ELF loading done by RS at runtime)
///
/// The architecture-specific differences are in `ArchProcInit`:
/// which registers hold PC, SP, and ps_strings.
///
/// C: arch_boot_proc() — protect.c:388 (x86) / protect.c:115 (ARM)
pub trait BootProcArch: ArchProcInit {
    /// Load a VM ELF binary into the bootstrap page table.
    ///
    /// This is the core operation of `arch_boot_proc` for the VM process.
    /// It:
    /// 1. Parses the ELF binary from the boot module
    /// 2. Allocates physical pages and maps them in the bootstrap page table
    /// 3. Copies ELF segments into the mapped pages
    /// 4. Sets up the user stack with ps_strings
    /// 5. Returns the entry point, stack pointer, and ps_strings address
    ///
    /// C: libexec_load_elf() + pg_map(PG_ALLOCATEME, ...) — protect.c:425
    fn load_vm_elf<P: Paging>(
        module: &BootModule,
        kernel_info: &KernelInfo,
        paging: &mut P,
    ) -> VmLoadResult;
}

// ── Mock implementation ──

#[cfg(feature = "mock")]
pub struct MockProcArch;

#[cfg(feature = "mock")]
impl ArchProcReset for MockProcArch {
    fn initial_reg_state(is_kernel: bool, proc_nr: i32) -> InitialRegState {
        let _ = (is_kernel, proc_nr);
        InitialRegState {
            status: 0,
            segment_selectors: SegmentSelectors::default(),
            fpu_needs_zero: false,
        }
    }
}

#[cfg(feature = "mock")]
impl ArchProcInit for MockProcArch {
    fn init_regs(
        is_kernel: bool,
        proc_nr: i32,
        pc: VirBytes,
        sp: VirBytes,
        ps_strings: VirBytes,
    ) -> InitialRegs {
        let _ = (is_kernel, proc_nr);
        InitialRegs {
            pc,
            sp,
            ps_strings_reg: ps_strings.0,
        }
    }
}

#[cfg(feature = "mock")]
impl BootProcArch for MockProcArch {
    fn load_vm_elf<P: Paging>(
        module: &BootModule,
        kernel_info: &KernelInfo,
        paging: &mut P,
    ) -> VmLoadResult {
        let _ = (module, kernel_info, paging);
        VmLoadResult {
            pc: VirBytes(0),
            sp: VirBytes(0),
            ps_strings: VirBytes(0),
            allocated_bytes: 0,
        }
    }
}
