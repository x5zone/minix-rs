//! Process architecture abstraction
//!
//! Defines trait interfaces for architecture-specific process operations:
//! - `ArchProcReset`: Reset process register state to safe defaults
//! - `ArchProcInit`: Initialize a process with a specific entry point and stack
//! - `BootProcArch`: Architecture-specific boot process initialization
//!
//! # Design decisions (see 05-proc-init-boot-proc.md §3)
//!
//! - **Three-trait split** (§3.2-3.4): `ArchProcReset` handles register
//!   defaults, `ArchProcInit` handles entry-point setup, `BootProcArch`
//!   handles the full boot_proc flow including VM ELF loading.
//! - **OS-semantic method names** (§3.3): Methods describe OS needs (reset,
//!   init, boot_proc), not architecture-specific register names.

use minix_types::VirBytes;
use minix_boot::{KernelInfo, BootModule};
use crate::paging::Paging;

/// Architecture abstraction for resetting process register state.
///
/// Called during process table initialization and when a process slot
/// is being recycled. Sets register state to safe defaults appropriate
/// for the architecture.
///
/// # Architecture mapping
///
/// | Register     | x86-64                    | ARM64              | RISC-V          |
/// |--------------|---------------------------|--------------------|-----------------|
/// | Status reg   | RFLAGS: IOPL=0, IF=1      | SPSR: EL0t/EL1h    | sstatus: SPP/SPIE|
/// | CS           | USER_CS_SELECTOR          | N/A                | N/A             |
/// | DS/SS/ES/FS/GS| USER_DS_SELECTOR         | N/A                | N/A             |
/// | FPU/ExtReg   | Zeroed for user procs     | N/A (lazy)         | N/A (lazy)      |
///
/// C: arch_proc_reset() — arch_system.c:146 (x86) / arch_system.c:42 (ARM)
pub trait ArchProcReset {
    /// Reset architecture-specific process state to default values.
    ///
    /// Sets register state to safe defaults:
    /// - x86-64: CS/DS/SS/ES/FS/GS = USER selectors, RFLAGS = INIT_PSW
    /// - aarch64: SPSR = EL0t (user) or EL1h (kernel task)
    /// - riscv64: sstatus = SPP=0, SPIE=1 (user) or SPP=1, SPIE=0 (kernel)
    ///
    /// # Arguments
    ///
    /// * `is_kernel` - true for kernel tasks (p_nr < 0), false for user processes
    /// * `proc_nr` - process number, used for FPU state allocation on x86-64
    fn reset(is_kernel: bool, proc_nr: i32);
}

/// Architecture abstraction for initializing a process with a specific
/// entry point and stack pointer.
///
/// Called after `ArchProcReset` to set the process's initial PC, SP,
/// and architecture-specific argument register (ps_strings).
///
/// # Architecture mapping
///
/// | Register  | x86-64 | ARM64 | RISC-V |
/// |-----------|--------|-------|--------|
/// | PC        | rip    | pc    | sepc   |
/// | SP        | rsp    | sp    | sp     |
/// | ps_strings| rbx    | r0    | a0     |
///
/// C: arch_proc_init() — memory.c:722 (x86) / memory.c:627 (ARM)
pub trait ArchProcInit: ArchProcReset {
    /// Initialize a process with a specific entry point and stack pointer.
    ///
    /// This is a two-step operation:
    /// 1. Call `Self::reset()` to set safe register defaults
    /// 2. Set PC, SP, and the ps_strings argument register
    ///
    /// # Arguments
    ///
    /// * `is_kernel` - true for kernel tasks
    /// * `proc_nr` - process number
    /// * `pc` - Entry point (virtual address)
    /// * `sp` - Stack pointer (virtual address)
    /// * `ps_strings` - Address of ps_strings struct on the stack
    /// * `name` - Process name
    fn init(
        is_kernel: bool,
        proc_nr: i32,
        pc: VirBytes,
        sp: VirBytes,
        ps_strings: VirBytes,
        name: &str,
    );
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
    fn reset(is_kernel: bool, proc_nr: i32) {
        let _ = (is_kernel, proc_nr);
    }
}

#[cfg(feature = "mock")]
impl ArchProcInit for MockProcArch {
    fn init(
        is_kernel: bool,
        proc_nr: i32,
        pc: VirBytes,
        sp: VirBytes,
        ps_strings: VirBytes,
        name: &str,
    ) {
        let _ = (is_kernel, proc_nr, pc, sp, ps_strings, name);
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
