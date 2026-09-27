//! ARM64 (aarch64) protection mechanism implementation
//!
//! Implements `ProtectionArch` for ARM64. ARM64 does not use segment
//! descriptors (GDT) like x86 — privilege levels are managed by the
//! CPU's exception level mechanism (EL0/EL1) and the SP_EL1 register.
//!
//! # ARM64 privilege model
//!
//! - EL0: User mode (unprivileged)
//! - EL1: Kernel mode (supervisor)
//! - EL2: Hypervisor (not used by Minix-RS)
//! - EL3: Secure monitor (not used by Minix-RS)
//!
//! On exception entry (SVC/IRQ/FIQ/SError), the CPU automatically:
//! 1. Saves PSTATE to SPSR_EL1
//! 2. Saves return address to ELR_EL1
//! 3. Switches SP from SP_EL0 to SP_EL1
//! 4. Sets PSTATE.M to EL1h
//! 5. Vectors through VBAR_EL1 + offset
//!
//! # OS-level concerns → ARM64 mechanism mapping
//!
//! The `ProtectionArch` trait exposes OS-level concerns; this file maps
//! each to its concrete ARM64 implementation:
//!
//! | OS concern (trait method) | ARM64 mechanism (this file's impl) |
//! |---------------------------|--------------------------------------|
//! | `init` (establish protection) | program `TPIDR_EL1` with this CPU's kernel-stack base — the EL1h model has no hardware stack switch (x86 reloads `RSP` from `TSS.sp0` on every CPL3→0), so `restore_to_user` reloads `SP_EL1` from `TPIDR_EL1` before `eret` (NK4-C §1.113); `SPSel=1` makes `SP_EL1` the *active* stack pointer, so a banked `msr SP_EL1, ...` is architecturally UNDEFINED at `SPSel=1` and traps |
//! | `set_kernel_stack` (switch kernel stack) | no-op under EL1h — the active SP is the exception stack, so relocating it from a returning Rust fn would orphan the caller frame; the stack base rides `TPIDR_EL1` and is reloaded per trap return (see `init`) |
//! | `load` (make protection effective) | `isb` (instruction synchronization barrier) — no separate load needed since SP_EL1 is active immediately |
//! | `init_ap` (AP startup) | program this AP's `TPIDR_EL1` with its own kernel-stack base (per-CPU register; run on the AP so it writes its own bank) |
//!
//! # Why ARM64 has no GDT/TSS
//!
//! Unlike x86-64 (which requires GDT to reference TSS descriptor for
//! kernel stack switches), ARM64 uses a single dedicated register
//! (`SP_EL1`) per exception level. This eliminates the indirection
//! table requirement and makes protection setup O(1) register writes.
//!
//! C: prot_init() — earm/protect.c:77

use crate::protection::{ProtectionArch, Privilege};
use minix_types::VirBytes;
use core::arch::asm;

/// ARM64 privilege level representation.
///
/// ARM64 uses Exception Levels (EL) for privilege separation.
/// Only EL0 (user) and EL1 (kernel) are used by Minix-RS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AArch64PrivilegeLevel(u8);

impl AArch64PrivilegeLevel {
    /// Exception Level 1 — kernel/supervisor mode.
    pub const EL1: Self = Self(1);
    /// Exception Level 0 — user/unprivileged mode.
    pub const EL0: Self = Self(0);

    pub(crate) const fn get(self) -> u8 {
        self.0
    }
}

/// ARM64 protection state.
///
/// On ARM64, the primary protection configuration is:
/// - Kernel stack: the active `SP_EL1` (`SPSel=1`), whose per-CPU base is
///   recorded in `TPIDR_EL1` and reloaded by `restore_to_user` before every
///   `eret` (NK4-C §1.113) — there is no hardware bank switch like x86 TSS.
/// - VBAR_EL1: Exception vector base address (set by TrapEntryArch)
///
/// Unlike x86-64, there is no GDT, IDT, or TSS. The CPU handles
/// privilege transitions automatically based on exception level.
pub struct AArch64Protection {
    /// Number of CPUs initialized (for SMP tracking).
    cpu_count: u32,
}

/// Program this CPU's kernel-stack base into `TPIDR_EL1` (NK4-C §1.113).
///
/// The EL1h model (`SPSel=1`) has no hardware switch-to-kernel-stack: x86
/// reloads `RSP` from `TSS.sp0` on every CPL3→0 transition, but aarch64
/// keeps running on the single `SP_EL1` that `jump_to_kmain` loaded once.
/// `restore_to_user` therefore reloads `SP_EL1` from `TPIDR_EL1` right
/// before `eret`, so each return-to-user parks the stack back at its base
/// and the next trap starts fresh instead of ratcheting one scheduler frame
/// deeper toward `.bss`.
///
/// **Register ownership**: `TPIDR_EL1` is claimed *exclusively* by this
/// per-CPU kernel-stack base as of NK4-C §1.113. An earlier plan used it as
/// a per-CPU id carrier (see `arch/smp.rs` / `kernel/clock.rs` notes, which
/// must not `msr tpidr_el1, cpu_id`); any future per-CPU id must pick a
/// different vehicle (e.g. `TPIDR_EL0`, an MPIDR lookup table, or
/// `SmpState::cpu_local`). `ap_cpu_id_readback` on aarch64 currently returns
/// a constant 0 and does **not** read `TPIDR`.
///
/// # Safety
///
/// `base` must be this CPU's live kernel-stack top (the same value
/// `jump_to_kmain` installed in `SP`), 16-byte aligned. Writing `TPIDR_EL1`
/// is only valid at EL1.
fn set_entry_sp(base: VirBytes) {
    // SAFETY: at EL1 with `SPSel=1`; `TPIDR_EL1` is a scratch RW system
    // register owned by the stack-base scheme (see the ownership note above).
    // The value is the caller-provided per-CPU stack top.
    //
    // `preserves_flags` documents that the write touches no condition flags.
    // A system-register write with an input operand is emitted unconditionally
    // by LLVM (Rust `asm!` is side-effecting by default and never dead-code
    // eliminated), so no extra `volatile` is needed here.
    //
    // No self-contained `isb`: the write is made visible to the later
    // `mrs tpidr_el1` in `restore_to_user` by the `isb` in `load()`, which
    // `init_protection` runs immediately after `init` (kernel/lib.rs).
    unsafe {
        asm!("msr tpidr_el1, {}", in(reg) base.0, options(nomem, preserves_flags));
    }
}

impl ProtectionArch for AArch64Protection {
    type PrivilegeLevel = AArch64PrivilegeLevel;

    const KERNEL_PRIVILEGE: AArch64PrivilegeLevel = AArch64PrivilegeLevel::EL1;
    const USER_PRIVILEGE: AArch64PrivilegeLevel = AArch64PrivilegeLevel::EL0;

    fn to_privilege(level: AArch64PrivilegeLevel) -> Privilege {
        match level {
            AArch64PrivilegeLevel::EL1 => Privilege::Kernel,
            _ => Privilege::User,
        }
    }

    fn from_privilege(privilege: Privilege) -> AArch64PrivilegeLevel {
        match privilege {
            Privilege::Kernel => AArch64PrivilegeLevel::EL1,
            Privilege::User => AArch64PrivilegeLevel::EL0,
        }
    }

    fn init(cpu_id: u32, kernel_stack_top: VirBytes) -> Self {
        // EL1h model (`SPSel=1`): `SP_EL1` *is* the active stack pointer, so
        // it is not bank-accessible via `msr SP_EL1, Xt` — ARMv8 D1.5 declares
        // that encoding UNDEFINED when `SPSel=1`, and executing it raises a
        // synchronous exception (this is exactly the boot trap that pinned
        // NK4-C §1.109). The kernel stack the exception entry uses is the
        // running SP, which the higher-half transition already established
        // with `mov sp, kern_stack_top`
        // (kernel/src/arch/aarch64/higher_half.rs). Because EL1h has no
        // hardware stack switch on trap entry (contrast x86 `TSS.sp0`), the
        // per-CPU base is parked in `TPIDR_EL1` here so `restore_to_user` can
        // reload `SP_EL1` from it before every `eret` — closing the scheduler
        // stack ratchet that NK4-C §1.113 pinned.
        //
        // `base == 0` is a sentinel, not a stack: the non-x86 `with_protection`
        // helper fabricates a throwaway `init(0, VirBytes::new(0))` purely to
        // reach `load()`/`init_ap`, and those calls would otherwise clobber the
        // live `TPIDR_EL1` back to 0 (observed: first user `svc` stores its
        // frame at `-16` → silent fault). A kernel stack never lives at VA 0,
        // so skipping the write there keeps the throwaway path side-effect-free.
        if kernel_stack_top.0 != 0 {
            set_entry_sp(kernel_stack_top);
        }
        Self { cpu_count: cpu_id + 1 }
    }

    fn set_kernel_stack(&mut self, _cpu_id: u32, stack_top: VirBytes) {
        // EL1h model: the kernel's exception stack *is* the active SP, so a
        // per-context stack switch cannot be performed by relocating SP from
        // a returning Rust function — `mov sp, ...` here would orphan the
        // caller frame (the function epilogue would pop x29/x30 from the new
        // stack). AArch64 has no per-task kernel stack: every task shares the
        // single per-CPU stack base recorded in `TPIDR_EL1` (see
        // [`set_entry_sp`]), and the one and only SP reload happens in
        // `restore_to_user` (trap_return.rs), which unconditionally rebases
        // `SP_EL1` from that base before `eret` — not from a saved per-task
        // context. This trait method is therefore a documented no-op; doing
        // the reload here (inside a returning Rust fn) would corrupt the
        // caller's frame. `stack_top` is accepted for trait shape parity with
        // x86 (where it writes TSS.sp0, pure data safe to store from C code).
        let _ = stack_top;
    }

    fn load(&self) {
        // On ARM64, VBAR_EL1 is set by TrapEntryArch::load() after
        // the exception vector table is initialized. The kernel's active
        // stack (SP_EL1 under EL1h) is established by the higher-half
        // transition (`mov sp, kern_stack_top` in
        // kernel/src/arch/aarch64/higher_half.rs), not by ProtectionArch::init.
        // There is no separate "load" operation needed for protection
        // structures on ARM64.
        //
        // SAFETY: ISB is always safe — it is an instruction synchronization
        // barrier that ensures previous system register writes are visible.
        unsafe {
            asm!("isb");
        }
    }

    fn init_ap(&self, cpu_id: u32, kernel_stack_top: VirBytes) {
        // Park this AP's own kernel-stack base in its `TPIDR_EL1` (a per-CPU
        // register — `init_ap` runs on the AP, so the write lands in the AP's
        // bank). The AP's live `SP_EL1` was already loaded by its early-entry
        // assembly; this is the reload value `restore_to_user` uses when the
        // AP returns to user, mirroring `init` on the BSP. `cpu_id` is
        // reference-only (the base already encodes the stack identity). Same
        // `base == 0` throwaway-instance guard as `init`.
        if kernel_stack_top.0 != 0 {
            set_entry_sp(kernel_stack_top);
        }
        let _ = cpu_id;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn privilege_level_roundtrip() {
        assert_eq!(
            AArch64Protection::to_privilege(AArch64PrivilegeLevel::EL1),
            Privilege::Kernel
        );
        assert_eq!(
            AArch64Protection::to_privilege(AArch64PrivilegeLevel::EL0),
            Privilege::User
        );
        assert_eq!(
            AArch64Protection::from_privilege(Privilege::Kernel),
            AArch64PrivilegeLevel::EL1
        );
        assert_eq!(
            AArch64Protection::from_privilege(Privilege::User),
            AArch64PrivilegeLevel::EL0
        );
    }

    #[test]
    fn protection_has_cpu_count() {
        // AArch64Protection only tracks cpu_count; the exception stack is the
        // active SP_EL1 (a hardware register, no per-instance table). `init()`
        // now writes the per-CPU stack base into TPIDR_EL1 (guarded against
        // the `base == 0` throwaway sentinel), but that is a real EL1 system
        // register write invalid in a host test — so this test constructs the
        // struct directly and just verifies the cpu_count field.
        let prot = AArch64Protection { cpu_count: 1 };
        assert_eq!(prot.cpu_count, 1);
    }
}
