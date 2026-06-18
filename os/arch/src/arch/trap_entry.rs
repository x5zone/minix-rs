//! Trap entry abstraction: OS-level concern of "how CPU enters kernel"
//!
//! # Design principle
//!
//! This trait abstracts the **OS-level concern** of "configuring how the
//! CPU enters the kernel in response to traps (interrupts, exceptions,
//! system calls)", hiding the **architecture-specific mechanism** (IDT +
//! SYSCALL MSR on x86-64, VBAR_EL1 + SVC on aarch64, stvec + ecall on
//! riscv64) inside each implementation.
//!
//! - **OS layer cares about WHAT**: "I want to install a handler" /
//!   "I want trap entry to be effective" / "I want a syscall entry point".
//!   The OS does NOT care about the concrete mechanism.
//! - **Architecture layer handles HOW**: each `TrapEntryArch` impl decides
//!   whether to use IDT, VBAR_EL1, or stvec. The OS code never references
//!   these names directly.
//!
//! This is why we use a trait rather than `enum Arch { X86_64, ... }` +
//! `match`: the trait enforces that **OS code never references hardware-
//! specific types** (IDT, MSR, VBAR_EL1, stvec). A `match` over an enum
//! would make architecture boundaries visible at the call site, violating
//! the "OS code is architecture-agnostic" rule.
//!
//! # What is abstracted (the 4 OS-level concerns)
//!
//! 1. **Install trap entry** (`init`)
//!    — OS says "set up the trap entry table for this CPU"; arch decides
//!    the table format (IDT gate descriptors / exception vector base /
//!    stvec mode)
//! 2. **Configure syscall entry** (`configure_syscall`)
//!    — OS says "this is the syscall entry point"; on x86-64 this writes
//!    the SYSCALL MSR (LSTAR); on aarch64/riscv64 this is a no-op (SVC/
//!    ecall share the same exception vector)
//! 3. **Make trap entry effective** (`load` / `load_ap`)
//!    — OS says "write the trap entry config to hardware"; arch decides
//!    the register writes (lidt + isb, msr VBAR_EL1 + isb, csrw stvec)
//! 4. **Set individual handlers** (`set_handler`)
//!    — OS says "install handler for vector V with privilege P"; arch
//!    decides the entry format (IDT gate / VBAR offset / stvec vector)
//!
//! # Special case: SYSCALL configuration
//!
//! Although `configure_syscall` is x86-64-specific (only x86-64 needs MSR
//! configuration for SYSCALL entry), it lives in this trait because its
//! **OS-level concern is generic** ("configure the syscall entry point").
//! aarch64/riscv64 implement it as no-op because their SVC/ecall use the
//! same exception vector as other traps — no separate configuration
//! needed. This is the "encapsulate differences" principle in action.
//!
//! # Design rationale in design doc
//!
//! See `notes/rewrite/fork-syscall-rewrite/03-stage-kernel/03-kmain-cstart.md`:
//! - §3.1 (Two-trait split) — why `TrapEntryArch` is separate from
//!   `ProtectionArch` (entry mechanism vs access control are different
//!   concerns with different init order)
//! - §1.3 (跨特权级的统一流程) — the OS-level trap flow (user→kernel
//!   transition) the trait implements
//! - §1.4 (CPU 视角三问 / 第二问) — "异常/syscall 跳哪" is exactly what
//!   this trait answers

use minix_types::VirBytes;

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
/// # Initialization order
///
/// `ProtectionArch::load()` must be called before `TrapEntryArch::init()`,
/// because trap entry descriptors (e.g., IDT gate descriptors on x86-64)
/// reference segment selectors that must be valid in the GDT.
///
/// C: idt_init() — protect.c:260
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

    /// Register a handler for a specific interrupt vector.
    ///
    /// Used for dynamic handler registration (e.g., device drivers
    /// registering IRQ handlers). The DPL (Descriptor Privilege Level)
    /// controls whether the vector can be triggered from user mode.
    ///
    /// C: gate_table_pic[] / gate_table_exceptions[] — protect.c:260
    fn set_handler(
        &mut self,
        vector: InterruptVector,
        handler: VirBytes,
        user_accessible: bool,
    );
}
