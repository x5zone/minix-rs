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
//! # What is abstracted (the 4 OS-level concerns)
//!
//! 1. **Establish protection** (`init`)
//!    — OS says "set up protection for this CPU"; arch decides the
//!    mechanism (GDT/CSR registers/exception vector base)
//! 2. **Switch kernel stack** (`set_kernel_stack`)
//!    — OS says "when transitioning to kernel, use this stack"; arch
//!    decides where to store it (TSS.sp0, SP_EL1, sscratch)
//! 3. **Make protection effective** (`load`)
//!    — OS says "write the protection config to hardware"; arch decides
//!    the register writes (lgdt/ltr, msr, csrw)
//! 4. **AP (application processor) startup** (`init_ap`)
//!    — OS says "initialize a new CPU the same way as BSP"; arch handles
//!    per-CPU differences (AP doesn't need global GDT sync)
//!
//! # Design rationale in design doc
//!
//! See `rewrite-notes/01-stage-kernel/03-kmain-cstart.md`:
//! - §3.2 (two-trait split, OS-semantic method names) — why the two traits
//!   are separate and why methods describe OS needs rather than architecture
//!   concepts
//! - §1.2 (the three questions from the CPU perspective) — the OS-level
//!   questions the trait answers
//! - §3.1 (why x86 still keeps a GDT/TSS) — the TSS descriptor must be
//!   referenced via a GDT entry (an ISA constraint that is INTENTIONALLY
//!   hidden from OS code via the trait)

use minix_types::VirBytes;

// Re-export InterruptVector from trap_entry module for backward compatibility.
// InterruptVector logically belongs to trap entry (it identifies IDT/vector
// entries), but is re-exported here because some protection code references it.
pub use crate::trap_entry::InterruptVector;


/// Architecture abstraction for hardware protection mechanisms.
///
/// Manages privilege levels, kernel stack setup for privilege transitions,
/// and loading protection structures into hardware.
///
/// # Architecture mapping
///
/// | Method              | x86-64                    | ARM64              | RISC-V          |
/// |---------------------|---------------------------|--------------------|-----------------|
/// | `init()`            | Clear GDT, fill segment   | Park stack base in | Configure       |
/// |                     | descriptors, create TSS   | TPIDR_EL1 (EL1h)   | sscratch, set   |
/// |                     |                           |                    | up trap regs    |
/// | `set_kernel_stack()`| Update TSS.sp0            | no-op (single SP,  | Update sscratch |
/// |                     |                           | reloaded in eret)  |                 |
/// | `load()`            | lgdt, ltr, reload        | isb (VBAR_EL1 set  | csrw sscratch,  |
/// |                     | segment registers         | by TrapEntryArch)  | ensure stvec    |
/// | `init_ap()`         | Per-CPU TSS/GDT entry,    | Per-CPU TPIDR_EL1  | Per-CPU         |
/// |                     | load selectors            | stack base         | sscratch        |
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
    /// # Per-CPU invariant
    ///
    /// The state is per-CPU: `cpu_id` names the CPU whose entry state is
    /// being set, and an implementation must never funnel it through one
    /// scalar shared by all harts. The return path refreshes the current
    /// CPU's entry state through `TrapReturnArch::prepare_user_return`
    /// before every user return; a shared cell lets one CPU's bring-up
    /// overwrite another CPU's entry stack (the riscv64 SMP bug this
    /// contract now forbids).
    ///
    /// C: tss_init() sets tss.sp0 — protect.c:175
    fn set_kernel_stack(&mut self, cpu_id: u32, stack_top: VirBytes);

    /// Rebuild descriptor-table entries that embed this instance's address.
    ///
    /// Called by the OS layer after the instance has moved into its final
    /// global location and before [`Self::load`]. Architectures whose
    /// protection state is register-only (aarch64/riscv64) inherit the
    /// no-op default.
    fn refresh_descriptors(&mut self) {}

    /// Program the per-CPU entry base used by the trap path.
    ///
    /// Called once on the BSP after [`Self::load`]. x86-64 writes the GS
    /// area (the SYSCALL entry reads its kernel stack from `gs:0x0`);
    /// aarch64/riscv64 inherit the no-op default because `init` /
    /// `set_kernel_stack` already wrote `TPIDR_EL1` / `sscratch`.
    fn program_entry_base(&self, _cpu_id: u32, _kernel_stack_top: VirBytes) {}

    /// Load protection structures into hardware registers.
    ///
    /// After this call, the CPU enforces the protection boundaries
    /// defined by the initialized structures.
    ///
    /// Called once per CPU on its resident instance. x86-64: `ltr` marks
    /// the TSS descriptor busy in memory; a second `load()` on the same
    /// instance raises #GP at `ltr` (the x86-64 loader intercepts this
    /// before `lgdt` and reports the named cause), while loading a
    /// different instance silently swaps GDTR/TR (production has one
    /// resident instance; carriers must not load multiple instances).
    ///
    /// C: prot_load_selectors() — protect.c:298
    ///    (lgdt, lldt, ltr, reload CS/DS/ES/FS/GS/SS)
    fn load(&self);

    /// Initialize protection for an Application Processor (AP).
    ///
    /// Called once per AP during SMP bringup. Loads the per-CPU protection
    /// structures the BSP prepared (x86-64: the per-CPU TSS descriptor and
    /// `sp0`, written by `set_kernel_stack` before the SIPI; aarch64/riscv64:
    /// per-CPU register state). The same per-CPU invariant as
    /// [`set_kernel_stack`](Self::set_kernel_stack) applies: no shared
    /// entry-stack scalar.
    ///
    /// C: tss_init(cpu, stack) + prot_load_selectors() — called from mpx.S
    fn init_ap(&self, cpu_id: u32, kernel_stack_top: VirBytes);
}

/// Mock implementation for testing — no hardware interaction.
///
/// All methods are no-ops.
#[cfg(feature = "runtime-window")]
pub struct MockProtection;

#[cfg(feature = "runtime-window")]
impl ProtectionArch for MockProtection {
    fn init(_cpu_id: u32, _kernel_stack_top: VirBytes) -> Self {
        Self
    }

    fn set_kernel_stack(&mut self, _cpu_id: u32, _stack_top: VirBytes) {}

    fn load(&self) {}

    fn init_ap(&self, _cpu_id: u32, _kernel_stack_top: VirBytes) {}
}
