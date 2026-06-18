//! Protection abstraction: OS-level concerns of CPU protection
//!
//! # Design principle
//!
//! This trait abstracts the **OS-level concern** of "establishing and
//! enforcing CPU protection", hiding the **architecture-specific mechanism**
//! (GDT/TSS on x86-64, VBAR_EL1 on aarch64, sscratch on riscv64) inside
//! each implementation.
//!
//! - **OS layer cares about WHAT**: "I want to establish protection" /
//!   "I want to switch the kernel stack" / "I want protection to be
//!   effective". The OS does NOT care about the concrete mechanism.
//! - **Architecture layer handles HOW**: each `ProtectionArch` impl decides
//!   whether to use GDT, VBAR_EL1, or sscratch. This is intentional
//!   encapsulation, not a design choice the OS layer should be aware of.
//!
//! This is why we use a trait rather than `enum Arch { X86_64, ... }` +
//! `match`: the trait enforces that **OS code never references hardware-
//! specific types** (GDT, TSS, sscratch). A `match` over an enum would
//! make the architecture boundaries visible at the call site, violating
//! the "OS code is architecture-agnostic" rule.
//!
//! # What is abstracted (the 5 OS-level concerns)
//!
//! 1. **Privilege level representation** (`PrivilegeLevel` associated type)
//!    — OS cares about "kernel vs user", not Ring/EL/Mode encodings
//! 2. **Establish protection** (`init`)
//!    — OS says "set up protection for this CPU"; arch decides the
//!    mechanism (GDT/CSR registers/exception vector base)
//! 3. **Switch kernel stack** (`set_kernel_stack`)
//!    — OS says "when transitioning to kernel, use this stack"; arch
//!    decides where to store it (TSS.sp0, SP_EL1, sscratch)
//! 4. **Make protection effective** (`load`)
//!    — OS says "write the protection config to hardware"; arch decides
//!    the register writes (lgdt/ltr, msr, csrw)
//! 5. **AP (application processor) startup** (`init_ap`)
//!    — OS says "initialize a new CPU the same way as BSP"; arch handles
//!    per-CPU differences (AP doesn't need global GDT sync)
//!
//! # Design rationale in design doc
//!
//! See `notes/rewrite/fork-syscall-rewrite/03-stage-kernel/03-kmain-cstart.md`:
//! - §3.1 (Two-trait split) — why `ProtectionArch` and `TrapEntryArch` are
//!   separate traits
//! - §3.2 (PrivilegeLevel as associated type) — why privilege encoding is
//!   an associated type, not a fixed enum
//! - §3.3 (OS-semantic method names) — why methods describe OS needs, not
//!   architecture concepts
//! - §1.4 (CPU 视角三问) — the OS-level questions the trait answers
//! - §1.7 (x86 为什么还保留 GDT) — why GDT/TSS still exist on x86-64
//!   (TSS descriptor must be referenced via GDT entry — this is an ISA
//!   constraint that is INTENTIONALLY hidden from OS code via the trait)

use minix_types::VirBytes;

// Re-export InterruptVector from trap_entry module for backward compatibility.
// InterruptVector logically belongs to trap entry (it identifies IDT/vector
// entries), but is re-exported here because some protection code references it.
pub use crate::trap_entry::InterruptVector;

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
