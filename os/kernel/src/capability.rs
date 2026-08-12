//! Process capability / privilege template abstraction.
//!
//! This module defines the OS-level concepts that go into the
//! per-process privilege (KPriv) structure:
//!
//! - [`ProcessCapability`] — the per-process capability bitmask
//!   (was C's `s_flags`).
//! - [`CapabilityTemplate`] — a "starter kit" of capabilities that
//!   applies to a category of process (kernel task, idle, VM,
//!   root service, deferred). Replaces the previous
//!   `assign_static` + `configure_boot_priv` two-step.
//! - [`TrapMask`] / [`IpcMask`] / [`KCallMask`] — the three mask
//!   Newtypes (per §12.6 — suffix `Mask` is part of the type name).
//!
//! # Why templates (and not ad-hoc flag setting)
//!
//! The previous `configure_boot_priv(priv_id, flags, init_flags,
//! trap_mask, ipc_to, k_call_mask, sig_mgr)` API asks the caller to
//! remember and provide all 7 fields for every category of process.
//! That API is repetitive and easy to get wrong (e.g., missing a
//! mask).
//!
//! The template API: pick one of the 5 stock templates
//! (`KernelTask` / `Idle` / `Vm` / `RootService` / `Deferred`),
//! hand it to [`PrivTable::grant_capability`], and the table fills
//! in the right masks for that category. Customization beyond the
//! stock templates is opt-in.
//!
//! # Why Newtypes for the masks
//!
//! `u64` is too permissive a type for an IPC allowlist. A Newtype
//! makes the type system enforce "this is a [`KCallMask`]":
//!
//! - can't accidentally pass a [`KCallMask`] where an [`IpcMask`] is
//!   expected (compile error, not silent wrong-target IPC).
//! - the bit-layout details (which bit means which kernel call /
//!   IPC target) are owned by this module's `new()` / `from_*`
//!   constructors, not open-coded at every callsite.
//!
//! # Coexistence with KPriv / PrivTable
//!
//! This module is **additive** in this iteration: it defines the
//! vocabulary but does not yet replace the existing
//! `KPriv::s_flags` / `s_trap_mask` / `s_ipc_to` / `s_k_call_mask`
//! fields. The plan (06-design-final.md §5 + §12.9) is to split
//! `KPriv` into 6 substructures (`capability` / `signals` / `io` /
//! `mem` / `irq` / `runtime`), at which point `ProcessCapability`
//! becomes the type of the `capability` substructure's flag field
//! and `TrapMask`/`IpcMask`/`KCallMask` move into their natural
//! homes. Until that migration, the existing fields stay as-is.

use bitflags::bitflags;

bitflags! {
    /// Per-process capability bitmask (was C's `s_flags`; field at `priv.h:24`,
    /// flag bit values at `const.h:143-154`, predefined combos at `priv.h:36-50`).
    ///
    /// The flags are exactly the OS-level capabilities a process
    /// can hold; nothing in this enum is arch-specific.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct ProcessCapability: u32 {
        /// Process is a system process (allowed to grant/revoke).
        /// C: `SYS_PROC` (0x01)
        const SYS_PROC          = 0x0000_0001;
        /// Process can be killed.
        /// C: `KILL` (0x02)
        const KILL              = 0x0000_0002;
        /// Process receives signals as a system process.
        /// C: `SIGS_SYS` (0x04)
        const SIGS_SYS          = 0x0000_0004;
        /// Process owns an ID (used by VM for memory-map ownership).
        /// C: `OWN_ID` (0x08)
        const OWN_ID            = 0x0000_0008;
        /// Process is billable (CPU time accounted).
        /// C: `BILLABLE` (0x10)
        const BILLABLE          = 0x0000_0010;
        /// Process is the idle task.
        /// C: `IDL_F` (0x20)
        const IDL_F             = 0x0000_0020;
        /// Process is a system service (RS / VM / etc.).
        /// C: `SRV_F` (0x40)
        const SRV_F             = 0x0000_0040;
        /// Process is the root system service (RS).
        /// C: `RSYS_F` (0x80)
        const RSYS_F            = 0x0000_0080;
        /// Process is the VM server.
        /// C: `VM_F` (0x100)
        const VM_F              = 0x0000_0100;
        /// Process is a kernel task (TSK_F).
        /// C: `TSK_F` (0x200)
        const TSK_F             = 0x0000_0200;
    }
}

impl ProcessCapability {
    /// `true` ⇔ process is a kernel task (negative `p_nr`).
    pub const fn is_kernel_task(self) -> bool {
        self.contains(Self::TSK_F)
    }

    /// `true` ⇔ process is the idle task.
    pub const fn is_idle(self) -> bool {
        self.contains(Self::IDL_F)
    }

    /// `true` ⇔ process is the VM server.
    pub const fn is_vm(self) -> bool {
        self.contains(Self::VM_F)
    }

    /// `true` ⇔ process is the root system service (RS).
    pub const fn is_root_service(self) -> bool {
        self.contains(Self::RSYS_F)
    }

    /// `true` ⇔ process is a system service (RS or VM).
    pub fn is_system_service(self) -> bool {
        self.intersects(Self::RSYS_F | Self::VM_F | Self::SRV_F)
    }
}

/// Capability template — "starter kit" of capabilities for a category of process.
///
/// Replaces the previous two-step `assign_static` +
/// `configure_boot_priv` boot flow (06-design-final.md §3.6). Picking
/// a template once is correct by construction; you cannot forget to
/// set the IPC mask or to deny kernel calls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityTemplate {
    /// Idle kernel task: runs only when nothing else can.
    /// C: `IDL_F` flag set; no IPC, no kcalls.
    Idle,
    /// Ordinary kernel task (CLOCK / SYSTEM / KERNEL / ASYNCM).
    /// C: `TSK_F` flag set; no IPC, no kcalls.
    KernelTask,
    /// VM server. C: `VM_F`; ALL_M IPC targets, ALL_C kernel calls.
    Vm,
    /// Root system service (RS). C: `RSYS_F`; ALL_M / ALL_C.
    RootService,
    /// Process that has no privilege yet (slot reserved, waiting for
    /// RS to grant capabilities at runtime). C: no flags; NO_M / NO_C.
    Deferred,
}

/// Errors returned by [`PrivTable::grant_capability`] (06-design-final.md §3.6).
///
/// Replaces the previous `Option<PrivId>` return, which conflated
/// "out of slots" with "slot already occupied" and gave callers no way
/// to distinguish transient (retry-able) from permanent (bug) failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityError {
    /// `proc_nr` is out of the `[−NR_TASKS, NR_PROCS)` range.
    /// C: `get_priv` returns `ENONENT` — caller bug, not retry-able.
    InvalidProcNr,
    /// The static priv slot for `proc_nr` is already occupied by another
    /// process. C: `get_priv` returns `EBUSY` — caller bug (double-init).
    SlotOccupied,
    /// `NR_SYS_PROCS` slots are all in use. Only happens for dynamic
    /// (runtime) grants, not boot-time static grants.
    /// C: `get_priv` with `p_priv->s_id` from `dyn_priv_id()` returns `ENONENT`.
    NoFreeSlots,
}

impl CapabilityTemplate {
    /// Convert template to a concrete [`ProcessCapability`] set.
    pub fn capabilities(self) -> ProcessCapability {
        match self {
            Self::Idle => ProcessCapability::IDL_F | ProcessCapability::BILLABLE,
            Self::KernelTask => ProcessCapability::TSK_F | ProcessCapability::BILLABLE,
            Self::Vm => ProcessCapability::VM_F | ProcessCapability::SRV_F | ProcessCapability::BILLABLE,
            Self::RootService => ProcessCapability::RSYS_F | ProcessCapability::SRV_F | ProcessCapability::BILLABLE,
            Self::Deferred => ProcessCapability::empty(),
        }
    }

    /// Trap mask for this template (was C's `s_trap_mask`).
    pub const fn trap_mask(self) -> TrapMask {
        match self {
            // Kernel tasks + idle: no traps (kernel runs untrapped).
            Self::Idle | Self::KernelTask => TrapMask::NONE,
            // VM / RS / Deferred: no kernel-trap restrictions (they
            // run in user mode; traps come from CPU exceptions,
            // which are handled uniformly).
            Self::Vm | Self::RootService | Self::Deferred => TrapMask::NONE,
        }
    }

    /// IPC-target allowlist for this template (was C's `s_ipc_to`).
    pub const fn ipc_mask(self) -> IpcMask {
        match self {
            Self::Idle | Self::KernelTask => IpcMask::NONE,
            Self::Vm | Self::RootService => IpcMask::ALL,
            Self::Deferred => IpcMask::NONE,
        }
    }

    /// Kernel-call allowlist for this template (was C's `s_k_call_mask`).
    pub const fn kcall_mask(self) -> KCallMask {
        match self {
            Self::Idle | Self::KernelTask => KCallMask::NONE,
            Self::Vm | Self::RootService => KCallMask::ALL,
            Self::Deferred => KCallMask::NONE,
        }
    }
}

// ── Mask Newtypes (§12.6) ────────────────────────────────────────

/// Trap mask: bit i set ⇔ process is allowed to receive trap i.
/// Was C's `s_trap_mask` (`kernel/priv.h:34`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TrapMask(u32);

impl TrapMask {
    pub const NONE: Self = Self(0);
    pub const ALL: Self = Self(u32::MAX);

    pub const fn from_bits(bits: u32) -> Self {
        Self(bits)
    }

    pub const fn bits(self) -> u32 {
        self.0
    }

    pub const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }
}

/// IPC-target allowlist: bit i set ⇔ process is allowed to send IPC to sys_id i.
/// Was C's `s_ipc_to` (`kernel/priv.h:35`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct IpcMask(u64);

impl IpcMask {
    pub const NONE: Self = Self(0);
    pub const ALL: Self = Self(u64::MAX);

    pub const fn from_bits(bits: u64) -> Self {
        Self(bits)
    }

    pub const fn bits(self) -> u64 {
        self.0
    }

    pub const fn may_send_to(self, sys_id: u8) -> bool {
        if sys_id >= 64 {
            return false;
        }
        (self.0 & (1u64 << sys_id)) != 0
    }
}

/// Kernel-call allowlist: bit i set ⇔ process is allowed to make kernel call i.
/// Was C's `s_k_call_mask` (`kernel/priv.h:38`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct KCallMask(u64);

impl KCallMask {
    pub const NONE: Self = Self(0);
    pub const ALL: Self = Self(u64::MAX);

    pub const fn from_bits(bits: u64) -> Self {
        Self(bits)
    }

    pub const fn bits(self) -> u64 {
        self.0
    }

    pub const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_is_kernel_task_only_tsk_f() {
        let t = CapabilityTemplate::KernelTask.capabilities();
        assert!(t.is_kernel_task());
        assert!(!t.is_idle());
        assert!(!t.is_vm());
        assert!(!t.is_root_service());
        assert!(!t.is_system_service());
    }

    #[test]
    fn capability_idle_has_idl_f_and_billable() {
        let t = CapabilityTemplate::Idle.capabilities();
        assert!(t.is_idle());
        assert!(t.contains(ProcessCapability::BILLABLE));
        assert!(!t.is_kernel_task());
    }

    #[test]
    fn capability_vm_has_vm_f_and_is_system_service() {
        let t = CapabilityTemplate::Vm.capabilities();
        assert!(t.is_vm());
        assert!(t.is_system_service());
        assert!(!t.is_root_service());
        assert!(!t.is_kernel_task());
    }

    #[test]
    fn capability_root_service_has_rsys_f() {
        let t = CapabilityTemplate::RootService.capabilities();
        assert!(t.is_root_service());
        assert!(t.is_system_service());
    }

    #[test]
    fn capability_deferred_is_empty() {
        let t = CapabilityTemplate::Deferred.capabilities();
        assert_eq!(t, ProcessCapability::empty());
    }

    #[test]
    fn template_kcall_mask_idle_is_none() {
        assert_eq!(CapabilityTemplate::Idle.kcall_mask(), KCallMask::NONE);
        assert_eq!(CapabilityTemplate::KernelTask.kcall_mask(), KCallMask::NONE);
    }

    #[test]
    fn template_kcall_mask_vm_is_all() {
        assert_eq!(CapabilityTemplate::Vm.kcall_mask(), KCallMask::ALL);
        assert_eq!(CapabilityTemplate::RootService.kcall_mask(), KCallMask::ALL);
    }

    #[test]
    fn ipc_mask_may_send_to() {
        let m = IpcMask::from_bits(0b101);
        assert!(m.may_send_to(0));
        assert!(!m.may_send_to(1));
        assert!(m.may_send_to(2));
        assert!(!m.may_send_to(3));
        assert!(!m.may_send_to(64), "out-of-range sys_id is denied");
    }

    #[test]
    fn trap_mask_contains() {
        let a = TrapMask::from_bits(0b1010);
        let b = TrapMask::from_bits(0b0010);
        assert!(a.contains(b));
        assert!(!b.contains(a));
    }

    #[test]
    fn kcall_mask_default_is_none() {
        let m = KCallMask::default();
        assert_eq!(m, KCallMask::NONE);
        assert_eq!(m.bits(), 0);
    }
}