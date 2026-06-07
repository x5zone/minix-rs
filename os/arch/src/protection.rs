//! Protection architecture abstraction
//!
//! Defines the trait interface for hardware protection mechanisms:
//! - Privilege levels (Ring 0/3, EL0/EL1, U/S-mode)
//! - Kernel stack setup for privilege transitions
//! - Protection structure initialization and loading
//!
//! # Design decisions (see 04-protection.md §3)
//!
//! - **Two-trait split** (§3.1): `ProtectionArch` handles "who can access what"
//!   (access control), `TrapEntryArch` handles "how to enter the kernel"
//!   (entry mechanism). They have different initialization order dependencies.
//! - **OS-semantic method names** (§3.3): Methods describe OS needs, not
//!   architecture-specific concepts. x86-64's GDT/TSS are implementation
//!   details hidden inside `X86_64Protection`.
//! - **PrivilegeLevel as associated type** (§3.2): Each architecture has its
//!   own privilege level representation with hardware encoding, convertible
//!   to the common `Privilege` enum.

use minix_types::VirBytes;

/// Common privilege level enum for architecture-agnostic OS code.
///
/// All supported architectures use exactly two privilege levels:
/// kernel (supervisor) and user. This enum provides a unified
/// representation that OS code can use without knowing the
/// architecture-specific encoding.
///
/// C: INTR_PRIVILEGE(0) / USER_PRIVILEGE(3) — archconst.h:33-34
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Privilege {
    /// Kernel / supervisor privilege level.
    /// x86-64: Ring 0; ARM64: EL1; RISC-V: S-mode.
    Kernel,
    /// User / unprivileged level.
    /// x86-64: Ring 3; ARM64: EL0; RISC-V: U-mode.
    User,
}

/// Architecture abstraction for hardware protection mechanisms.
///
/// Manages privilege levels, kernel stack setup for privilege transitions,
/// and loading protection structures into hardware.
///
/// # Architecture mapping
///
/// | Method              | x86-64                    | ARM64              | RISC-V          |
/// |---------------------|---------------------------|--------------------|-----------------|
/// | `init()`            | Clear GDT, fill segment   | Configure SP_EL0/  | Configure       |
/// |                     | descriptors, create TSS   | SP_EL1, set up     | sscratch, set   |
/// |                     |                           | exception regs     | up trap regs    |
/// | `set_kernel_stack()`| Update TSS.sp0            | Update SP_EL1      | Update sscratch |
/// | `load()`            | lgdt, lldt, ltr, reload   | msr SP_EL1, ensure | csrw sscratch,  |
/// |                     | segment registers         | VBAR_EL1 set       | ensure stvec    |
/// | `init_ap()`         | Per-CPU TSS/GDT entry,    | Per-CPU SP_EL1     | Per-CPU         |
/// |                     | load selectors            |                    | sscratch        |
///
/// # Initialization order
///
/// `ProtectionArch::init()` + `load()` must complete before
/// `TrapEntryArch::init()` is called, because trap entry descriptors
/// (e.g., IDT gate descriptors on x86-64) reference segment selectors
/// that must be valid in the GDT.
///
/// C: prot_init() — protect.c:321
pub trait ProtectionArch: Sized {
    /// Architecture-specific privilege level representation.
    ///
    /// x86-64: Ring 0 (Kernel) / Ring 3 (User) — encoded in segment
    ///         selector RPL field and descriptor DPL field.
    /// ARM64:  EL1 (Kernel) / EL0 (User) — encoded in SPSR_EL1.M.
    /// RISC-V: S-mode (Kernel) / U-mode (User) — encoded in sstatus.SPP.
    type PrivilegeLevel: Copy + Eq + core::fmt::Debug;

    /// Kernel privilege level.
    /// x86-64: Ring 0; ARM64: EL1; RISC-V: S-mode.
    const KERNEL_PRIVILEGE: Self::PrivilegeLevel;

    /// User privilege level.
    /// x86-64: Ring 3; ARM64: EL0; RISC-V: U-mode.
    const USER_PRIVILEGE: Self::PrivilegeLevel;

    /// Convert architecture-specific privilege level to common enum.
    fn to_privilege(level: Self::PrivilegeLevel) -> Privilege;

    /// Convert common privilege enum to architecture-specific level.
    fn from_privilege(privilege: Privilege) -> Self::PrivilegeLevel;

    /// Initialize protection structures for the boot CPU (BSP).
    ///
    /// Called once during `cstart()`, before `TrapEntryArch::init()`.
    ///
    /// C: prot_init() — protect.c:321
    ///    (clears GDT/IDT, sets descriptor table pointers, fills GDT
    ///     entries, calls tss_init() for BSP)
    fn init(cpu_id: u32, kernel_stack_top: VirBytes) -> Self;

    /// Set the kernel stack pointer for privilege level transitions.
    ///
    /// When a transition from user mode to kernel mode occurs (interrupt,
    /// exception, or system call), the CPU automatically switches to this
    /// kernel stack.
    ///
    /// C: tss_init() sets tss.sp0 — protect.c:175
    fn set_kernel_stack(&mut self, cpu_id: u32, stack_top: VirBytes);

    /// Load protection structures into hardware registers.
    ///
    /// After this call, the CPU enforces the protection boundaries
    /// defined by the initialized structures.
    ///
    /// C: prot_load_selectors() — protect.c:298
    ///    (lgdt, lldt, ltr, reload CS/DS/ES/FS/GS/SS)
    fn load(&self);

    /// Initialize protection for an Application Processor (AP).
    ///
    /// Called once per AP during SMP bringup. Creates per-CPU protection
    /// structures (e.g., TSS entry in GDT) and loads them.
    ///
    /// C: tss_init(cpu, stack) + prot_load_selectors() — called from mpx.S
    fn init_ap(&self, cpu_id: u32, kernel_stack_top: VirBytes);
}

/// Interrupt/exception vector number.
///
/// Wraps a u8 vector number with type safety. On x86-64, this
/// corresponds to an IDT vector index (0-255). On ARM64/RISC-V,
/// the meaning is architecture-specific but the type is shared.
///
/// C: interrupt.h:21-25, archconst.h:38-49
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InterruptVector(pub u8);

impl InterruptVector {
    pub const fn new(value: u8) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u8 {
        self.0
    }
}

pub const DIVIDE_ERROR: InterruptVector           = InterruptVector(0);
pub const DEBUG: InterruptVector                  = InterruptVector(1);
pub const NMI: InterruptVector                    = InterruptVector(2);
pub const BREAKPOINT: InterruptVector             = InterruptVector(3);
pub const OVERFLOW: InterruptVector               = InterruptVector(4);
pub const BOUNDS_CHECK: InterruptVector           = InterruptVector(5);
pub const INVALID_OPCODE: InterruptVector         = InterruptVector(6);
pub const DEVICE_NOT_AVAILABLE: InterruptVector   = InterruptVector(7);
pub const DOUBLE_FAULT: InterruptVector           = InterruptVector(8);
pub const COPROCESSOR_SEGMENT_OVERRUN: InterruptVector = InterruptVector(9);
pub const INVALID_TSS: InterruptVector            = InterruptVector(10);
pub const SEGMENT_NOT_PRESENT: InterruptVector    = InterruptVector(11);
pub const STACK_FAULT: InterruptVector            = InterruptVector(12);
pub const GENERAL_PROTECTION: InterruptVector     = InterruptVector(13);
pub const PAGE_FAULT: InterruptVector             = InterruptVector(14);

pub const KERN_CALL_VECTOR: InterruptVector       = InterruptVector(32);
pub const IPC_VECTOR: InterruptVector             = InterruptVector(33);

/// Architecture abstraction for trap entry configuration.
///
/// Manages how the CPU enters the kernel in response to interrupts,
/// exceptions, and system calls. On x86-64, this includes the IDT
/// (Interrupt Descriptor Table) and SYSCALL MSR configuration.
/// On ARM64/RISC-V, the exception/trap vector serves all three purposes.
///
/// # Design decisions (see 04-protection.md §3.1)
///
/// Two-trait split: `ProtectionArch` handles "who can access what"
/// (access control), `TrapEntryArch` handles "how to enter the kernel"
/// (entry mechanism). They have different initialization order:
/// `ProtectionArch::load()` must complete before `TrapEntryArch::init()`,
/// because IDT gate descriptors reference segment selectors that must
/// be valid in the GDT.
///
/// # Architecture mapping
///
/// | Method               | x86-64                    | ARM64              | RISC-V          |
/// |----------------------|---------------------------|--------------------|-----------------|
/// | `init()`             | Fill IDT with gate_table  | Set VBAR_EL1 to    | Set stvec to    |
/// |                      | exceptions[] and pic[]    | exception vectors  | trap vector     |
/// | `configure_syscall()`| Enable SYSCALL via MSR    | No-op (SVC uses    | No-op (ecall    |
/// |                      | (STAR, LSTAR, SFMASK,     | same VBAR_EL1)     | uses same       |
/// |                      |  EFER.SCE)                |                    | stvec)          |
/// | `load()`             | lidt                      | Ensure VBAR_EL1    | Ensure stvec    |
/// | `load_ap()`          | lidt (shared IDT)         | Per-CPU VBAR_EL1   | Per-CPU stvec   |
pub trait TrapEntryArch: Sized {
    /// Initialize the trap entry table with architecture-specific handlers.
    ///
    /// Fills the table with handler addresses for CPU exceptions,
    /// hardware interrupts, and system call vectors.
    ///
    /// C: idt_init() — protect.c:260
    ///    (fills IDT with gate_table_exceptions[] and gate_table_pic[])
    fn init() -> Self;

    /// Configure the system call entry mechanism.
    ///
    /// x86-64: Enable SYSCALL/SYSRET via MSR — sets STAR, LSTAR, SFMASK,
    ///         and enables EFER.SCE. Called after `init()` and before `load()`.
    ///         The x86-64 implementation should verify CPU support via
    ///         CPUID.80000001H:EDX.SCE before writing MSRs.
    /// ARM64:  No-op — SVC instruction uses the exception vector set by `init()`.
    /// RISC-V: No-op — ecall instruction uses the trap vector set by `init()`.
    ///
    /// C: tss_init() lines 189-205 — SYSCALL MSR setup
    fn configure_syscall(&mut self, entry_point: VirBytes);

    /// Load the trap entry table into hardware.
    ///
    /// After this call, the CPU will route interrupts, exceptions, and
    /// system calls through the configured entry points.
    ///
    /// C: idt_reload() — protect.c:268
    ///    (x86_lidt(&idt_desc))
    fn load(&self);

    /// Load the trap entry table on an AP.
    ///
    /// On x86-64, the IDT is shared across CPUs, so this just reloads
    /// the IDTR. On ARM64/RISC-V, each CPU has its own VBAR_EL1/stvec.
    fn load_ap(&self);
}
