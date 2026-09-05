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
//!   Newtypes (per 22-privilege.md §4.4 D5 — suffix `Mask` is part
//!   of the type name).
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
//! # Storage in KPriv
//!
//! `KPriv` stores these types directly (06-proc-init-boot-proc.md §4.6,
//! 8-substructure layout): `PrivFlags::s_flags: ProcessCapability`,
//! `PrivIpc::s_trap_mask: TrapMask`, `PrivIpc::s_ipc_to: IpcMask`,
//! `PrivIpc::s_k_call_mask: KCallMask`. The wire boundary keeps the raw
//! Minix3 widths (`PrivUpdateRequest.s_flags: u16`, `.s_trap_mask: u16`,
//! `.s_ipc_to: u64`, `.s_k_call_mask: [u32; SYS_CALL_MASK_SIZE]`) and
//! converts via each type's `from_wire` / `to_wire` codec.

use bitflags::bitflags;

bitflags! {
    /// Per-process capability bitmask (was C's `s_flags`; field at `priv.h:24`,
    /// flag bit values at `const.h:143-154`, predefined combos at `priv.h:36-50`).
    ///
    /// The 11 low bits adopt the C wire layout (`const.h:143-154`) 1:1, so the
    /// wire codec ([`Self::from_wire`]/[`Self::to_wire`]) is per-bit lossless;
    /// the Rust extension bits (no C `s_flags` bit; 22-privilege.md §4.4 D4)
    /// live above the 16-bit wire range and never cross the wire. The `*_F`
    /// constants are the C predefined *combos* (`priv.h:36-50`) — OR
    /// combinations of atomic bits, not independent bits.
    ///
    /// The flags are exactly the OS-level capabilities a process
    /// can hold; nothing in this enum is arch-specific.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct ProcessCapability: u32 {
        // ── C atomic bits (const.h:143-154 — positions = wire layout) ──
        /// Kernel tasks are not preemptible. C: `PREEMPTIBLE` (const.h:143).
        const PREEMPTIBLE     = 0x0000_0002;
        /// CPU time accounted. C: `BILLABLE` (const.h:144).
        const BILLABLE        = 0x0000_0004;
        /// Privilege id assigned dynamically. C: `DYN_PRIV_ID` (const.h:145).
        const DYN_PRIV_ID     = 0x0000_0008;
        /// System process (has its own priv structure; allowed to grant/revoke).
        /// C: `SYS_PROC` (const.h:147).
        const SYS_PROC        = 0x0000_0010;
        /// Check if I/O request is allowed. C: `CHECK_IO_PORT` (const.h:148).
        const CHECK_IO_PORT   = 0x0000_0020;
        /// Check if IRQ can be used. C: `CHECK_IRQ` (const.h:149).
        const CHECK_IRQ       = 0x0000_0040;
        /// Check if (VM) mem map request is allowed. C: `CHECK_MEM` (const.h:150).
        const CHECK_MEM       = 0x0000_0080;
        /// Root system process instance. C: `ROOT_SYS_PROC` (const.h:151).
        const ROOT_SYS_PROC   = 0x0000_0100;
        /// VM system process instance. C: `VM_SYS_PROC` (const.h:152).
        const VM_SYS_PROC     = 0x0000_0200;
        /// Live-updated system process instance. C: `LU_SYS_PROC` (const.h:153).
        const LU_SYS_PROC     = 0x0000_0400;
        /// Restarted system process instance. C: `RST_SYS_PROC` (const.h:154).
        const RST_SYS_PROC    = 0x0000_0800;
        // ── Rust extension bits (no C `s_flags` bit; 22-privilege.md §4.4 D4).
        // Above the 16-bit wire range — kernel-side only, stripped by to_wire.
        /// Rust extension: process may be killed.
        const KILL            = 0x0001_0000;
        /// Rust extension: process receives signals as a system process.
        const SIGS_SYS        = 0x0002_0000;
        /// Rust extension: process owns an ID (VM memory-map ownership).
        const OWN_ID          = 0x0004_0000;
        // ── C predefined combos (priv.h:36-50) — ORs of atomics, not bits ──
        /// Idle task. C: `IDL_F = SYS_PROC | BILLABLE` (priv.h:36).
        const IDL_F           = Self::SYS_PROC.bits() | Self::BILLABLE.bits();
        /// Other kernel tasks. C: `TSK_F = SYS_PROC` (priv.h:44).
        const TSK_F           = Self::SYS_PROC.bits();
        /// System services. C: `SRV_F = SYS_PROC | PREEMPTIBLE` (priv.h:45).
        const SRV_F           = Self::SYS_PROC.bits() | Self::PREEMPTIBLE.bits();
        /// Dynamic system services. C: `DSRV_F = SRV_F | DYN_PRIV_ID` (priv.h:46).
        const DSRV_F          = Self::SRV_F.bits() | Self::DYN_PRIV_ID.bits();
        /// Root system proc. C: `RSYS_F = SRV_F | ROOT_SYS_PROC` (priv.h:47).
        const RSYS_F          = Self::SRV_F.bits() | Self::ROOT_SYS_PROC.bits();
        /// VM. C: `VM_F = SYS_PROC | VM_SYS_PROC` (priv.h:48).
        const VM_F            = Self::SYS_PROC.bits() | Self::VM_SYS_PROC.bits();
        /// User processes. C: `USR_F = BILLABLE | PREEMPTIBLE` (priv.h:49).
        const USR_F           = Self::BILLABLE.bits() | Self::PREEMPTIBLE.bits();
    }
}

impl ProcessCapability {
    /// Decode a Minix3 wire `s_flags` value (C `bitchunk_t`, `u16`, bit
    /// layout `const.h:143-154`). Unknown bits are dropped; the Rust
    /// extension bits can never appear on the wire.
    pub const fn from_wire(wire: u16) -> Self {
        Self::from_bits_truncate(wire as u32)
    }

    /// Encode to the Minix3 wire `s_flags` width (`u16`). The Rust-only
    /// extension bits (above bit 15) are dropped — they have no C
    /// counterpart to represent.
    pub const fn to_wire(self) -> u16 {
        (self.bits() & 0xFFFF) as u16
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

    /// `true` ⇔ process is a system service (RS, VM, or any `SRV_F`
    /// driver). Requires `SYS_PROC` — user processes share `PREEMPTIBLE`
    /// (`USR_F = BILLABLE | PREEMPTIBLE`, priv.h:49) but are not services;
    /// kernel tasks carry `SYS_PROC` without any service bit
    /// (`TSK_F = SYS_PROC` exactly, priv.h:44) and are also excluded —
    /// C tells kernel tasks apart by `iskerneln(p_nr)`, not by `s_flags`.
    pub fn is_system_service(self) -> bool {
        self.contains(Self::SYS_PROC)
            && self.intersects(Self::PREEMPTIBLE | Self::ROOT_SYS_PROC | Self::VM_SYS_PROC)
    }
}

/// Capability template — "starter kit" of capabilities for a category of process.
///
/// Replaces the previous two-step `assign_static` +
/// `configure_boot_priv` boot flow (06-proc-init-boot-proc.md §3.2). Picking
/// a template once is correct by construction; you cannot forget to
/// set the IPC mask or to deny kernel calls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityTemplate {
    /// Idle kernel task: runs only when nothing else can.
    /// C: `IDL_F` flag set; `ipc_to = NO_M` (cannot SEND to other endpoints),
    /// `trap_mask = TSK_T = 0` (no IPC traps), no kcalls.
    Idle,
    /// Ordinary kernel task (CLOCK / SYSTEM / KERNEL / ASYNCM).
    /// C: `TSK_F` flag set; `ipc_to = NO_M` (cannot SEND to other endpoints).
    /// `trap_mask`: C gives CLOCK/SYSTEM `CSK_T = (1 << RECEIVE)` (RECEIVE-only IPC
    /// endpoint), other kernel tasks `TSK_T = 0`. `trap_mask()` here returns the
    /// generic role default (`NONE`); the CLOCK/SYSTEM `CSK_T` exception is applied
    /// by `PrivTable::grant_capability` where the `proc_nr` is known (doc 06 §3.2).
    /// No kcalls.
    KernelTask,
    /// VM server. C: `VM_F`; ALL_M IPC targets, ALL_C kernel calls.
    Vm,
    /// Root system service (RS). C: `RSYS_F`; ALL_M / ALL_C.
    RootService,
    /// Process that has no privilege yet (slot reserved, waiting for
    /// RS to grant capabilities at runtime). C: no flags; NO_M / NO_C.
    Deferred,
}

/// Errors returned by [`PrivTable::grant_capability`] (06-proc-init-boot-proc.md §3.2).
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
            // C template mapping (priv.h:36-49 + main.c:179-217): the *_F
            // constants are the C predefined combos, so each arm is the
            // exact flag set C writes to `s_flags`. Only IDL_F carries
            // BILLABLE; TSK_F/VM_F/RSYS_F/SRV_F do not.
            Self::Idle => ProcessCapability::IDL_F,
            Self::KernelTask => ProcessCapability::TSK_F,
            Self::Vm => ProcessCapability::VM_F | ProcessCapability::SRV_F,
            Self::RootService => ProcessCapability::RSYS_F | ProcessCapability::SRV_F,
            Self::Deferred => ProcessCapability::empty(),
        }
    }

    /// Trap mask for this template (was C's `s_trap_mask`).
    ///
    /// **Returns the default for the role** — i.e. the trap_mask a
    /// template would have if it were the *generic* kernel task.
    /// The CLOCK/SYSTEM exception (`CSK_T = (1 << RECEIVE)`) is applied
    /// in `PrivTable::grant_capability`, where the `proc_nr` is known
    /// (mirroring C's `main.c:218-219`:
    /// `(proc_nr == CLOCK || proc_nr == SYSTEM) ? CSK_T : TSK_T`).
    /// This keeps the template self-contained: roles own their defaults,
    /// `PrivTable` owns the per-process exceptions.
    pub const fn trap_mask(self) -> TrapMask {
        match self {
            // Idle + generic kernel task + deferred: no traps (kernel runs untrapped).
            Self::Idle | Self::KernelTask | Self::Deferred => TrapMask::NONE,
            // VM / RS: SRV_T = ~0 — all traps allowed (C: main.c:204-217).
            // TrapMask::ALL is the effective all-ones form C's `short
            // s_trap_mask = ~0` has after int promotion; `to_wire()` at the
            // wire boundary stores 0xFFFF, `from_wire()` sign-extends back.
            Self::Vm | Self::RootService => TrapMask::ALL,
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

// ── Mask Newtypes (22-privilege.md §4.4 D5) ──────────────────────

/// Trap mask: bit i set ⇔ process is allowed to receive trap i.
/// Was C's `s_trap_mask` (`kernel/priv.h:34`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TrapMask(u32);

impl TrapMask {
    pub const NONE: Self = Self(0);
    pub const ALL: Self = Self(u32::MAX);
    /// C: `CSK_T = (1 << RECEIVE)` (priv.h:59, RECEIVE=2) — only CLOCK/SYSTEM
    /// receive this; all other kernel tasks get `TSK_T = 0` (`priv.h:60`).
    /// `CSK_T` lets CLOCK/SYSTEM act as a RECEIVE-only IPC endpoint
    /// (e.g. PM → CLOCK via `notify(CLOCK)` from the alarm path).
    pub const RECEIVE: Self = Self(1 << 2);

    pub const fn from_bits(bits: u32) -> Self {
        Self(bits)
    }

    pub const fn bits(self) -> u32 {
        self.0
    }

    pub const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    /// True if this mask admits any IPC trap beyond RECEIVE — C:
    /// `s_trap_mask & ~(1 << RECEIVE)` (system.c:326-328). A RECEIVE-only
    /// endpoint (CLOCK/SYSTEM with `CSK_T`) can never reply or initiate a
    /// send, so `set_sendto_bit` skips granting the reciprocal bit to it.
    pub const fn allows_more_than_receive(self) -> bool {
        (self.0 & !Self::RECEIVE.0) != 0
    }

    /// Decode from the wire `u16` (C `short s_trap_mask`, priv.h:34).
    ///
    /// C sign-extends the `short` to `int` at every read (proc.c:552), so
    /// the stored `0xFFFF` of `SRV_T = ~0` acts as all-ones including the
    /// SENDA bit (16). Decoding with sign extension puts the mask into that
    /// effective form once at the wire boundary; [`Self::to_wire`] truncates
    /// back losslessly.
    pub const fn from_wire(wire: u16) -> Self {
        Self((wire as i16) as u32)
    }

    /// Encode to the wire `u16` (inverse of [`Self::from_wire`]; the
    /// sign-extension bits above bit 15 are dropped).
    pub const fn to_wire(self) -> u16 {
        self.0 as u16
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

    /// Grant bit `idx` — C: `set_sys_bit(map, id)` (`kernel/const.h:24`).
    ///
    /// Indices at or above 64 are ignored: `s_ipc_to` covers exactly the
    /// `NR_SYS_PROCS` (≤ 64) privilege ids, and an out-of-range grant is a
    /// caller bug the kernel must not mask into silent state corruption.
    pub const fn set_bit(mut self, idx: usize) -> Self {
        if idx < 64 {
            self.0 |= 1u64 << idx;
        }
        self
    }

    /// Revoke bit `idx` — C: `unset_sys_bit(map, id)` (`kernel/const.h:26`).
    pub const fn unset_bit(mut self, idx: usize) -> Self {
        if idx < 64 {
            self.0 &= !(1u64 << idx);
        }
        self
    }

    /// Test bit `idx` — C: `get_sys_bit(map, id)` (`kernel/const.h:20`).
    pub const fn has_bit(self, idx: usize) -> bool {
        if idx >= 64 {
            return false;
        }
        (self.0 & (1u64 << idx)) != 0
    }

    /// Bitwise union — C accumulates `s_ipc_to` with `set_sys_bit`, e.g.
    /// do_update.c:107-112 copies src's target mask into dst's with
    /// `dst |= src` semantics.
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub const fn may_send_to(self, sys_id: u8) -> bool {
        self.has_bit(sys_id as usize)
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

    /// Decode from the wire `[u32; SYS_CALL_MASK_SIZE]` (C
    /// `bitchunk_t s_k_call_mask[2]`, priv.h:38; `com.h:272` — low word
    /// first).
    pub const fn from_wire(words: [u32; crate::kpriv::SYS_CALL_MASK_SIZE]) -> Self {
        Self(words[0] as u64 | ((words[1] as u64) << 32))
    }

    /// Encode to the wire `[u32; SYS_CALL_MASK_SIZE]` (low word first;
    /// inverse of [`Self::from_wire`]).
    pub const fn to_wire(self) -> [u32; crate::kpriv::SYS_CALL_MASK_SIZE] {
        [(self.0 & 0xFFFF_FFFF) as u32, (self.0 >> 32) as u32]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_kernel_task_is_tsk_f_combo() {
        // C: TSK_F = SYS_PROC exactly (priv.h:44) — a combo, so a kernel
        // task's flag set is bare SYS_PROC without PREEMPTIBLE/BILLABLE.
        let t = CapabilityTemplate::KernelTask.capabilities();
        assert!(t.contains(ProcessCapability::SYS_PROC));
        assert_eq!(t, ProcessCapability::TSK_F);
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
        // Idle is a SYS_PROC, but not a service (no PREEMPTIBLE).
        assert!(t.contains(ProcessCapability::SYS_PROC));
        assert!(!t.is_system_service());
    }

    #[test]
    fn capability_is_system_service_truth_table() {
        // C combos (priv.h:36-49): services are SYS_PROC && (PREEMPTIBLE |
        // ROOT_SYS_PROC | VM_SYS_PROC). USR_F shares PREEMPTIBLE with SRV_F
        // but lacks SYS_PROC — must not classify as a service. Kernel tasks
        // (TSK_F) and idle (IDL_F) carry SYS_PROC without a service bit.
        assert!(ProcessCapability::SRV_F.is_system_service());
        assert!(ProcessCapability::RSYS_F.is_system_service());
        assert!(ProcessCapability::VM_F.is_system_service());
        assert!(!ProcessCapability::USR_F.is_system_service());
        assert!(!ProcessCapability::TSK_F.is_system_service());
        assert!(!ProcessCapability::IDL_F.is_system_service());
        assert!(!ProcessCapability::empty().is_system_service());
    }

    #[test]
    fn capability_vm_has_vm_f_and_is_system_service() {
        let t = CapabilityTemplate::Vm.capabilities();
        assert!(t.is_vm());
        assert!(t.is_system_service());
        assert!(!t.is_root_service());
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

    #[test]
    fn process_capability_wire_round_trip() {
        // C wire layout (const.h:143-154): from_wire/to_wire are per-bit
        // lossless for every atomic bit.
        for bits in [0x002u16, 0x004, 0x008, 0x010, 0x020, 0x040, 0x080, 0x100, 0x200, 0x400, 0x800]
        {
            let caps = ProcessCapability::from_wire(bits);
            assert_eq!(caps.to_wire(), bits, "wire round trip failed for {bits:#x}");
        }
        // A combined value decodes to the same atomic bits.
        let caps = ProcessCapability::from_wire(0x112); // PREEMPTIBLE|SYS_PROC
        assert!(caps.contains(ProcessCapability::PREEMPTIBLE));
        assert!(caps.contains(ProcessCapability::SYS_PROC));
        assert_eq!(caps.to_wire(), 0x112);
        // Rust extension bits never cross the wire.
        let extended = ProcessCapability::SYS_PROC | ProcessCapability::KILL;
        assert_eq!(extended.to_wire(), 0x010);
    }

    #[test]
    fn trap_mask_wire_sign_extension() {
        // C: `short s_trap_mask = ~0` (SRV_T) stored as 0xFFFF; the int
        // promotion at every read (proc.c:552) makes bit 16 (SENDA) set.
        let all = TrapMask::from_wire(0xFFFF);
        assert_eq!(all, TrapMask::ALL);
        assert!(all.contains(TrapMask::from_bits(1u32 << 16)));
        // Round trip is lossless.
        assert_eq!(TrapMask::ALL.to_wire(), 0xFFFF);
        assert_eq!(TrapMask::RECEIVE.to_wire(), 1 << 2);
        assert_eq!(TrapMask::from_wire(TrapMask::RECEIVE.to_wire()), TrapMask::RECEIVE);
    }

    #[test]
    fn kcall_mask_wire_round_trip() {
        // C: NO_C → [0, 0]; ALL_C → [0xFFFF_FFFF; 2] (low word first).
        assert_eq!(KCallMask::NONE.to_wire(), [0, 0]);
        assert_eq!(KCallMask::ALL.to_wire(), [0xFFFF_FFFF; 2]);
        assert_eq!(KCallMask::from_wire([0xFFFF_FFFF; 2]), KCallMask::ALL);
        let m = KCallMask::from_wire([0xFF, 0]);
        assert!(m.contains(KCallMask::from_bits(1 << 7)));
        assert!(!m.contains(KCallMask::from_bits(1 << 32)));
        assert_eq!(KCallMask::from_wire(m.to_wire()), m);
    }

    #[test]
    fn ipc_mask_union() {
        let a = IpcMask::from_bits(1 << 5);
        let b = IpcMask::from_bits(1 << 9);
        let u = a.union(b);
        assert!(u.may_send_to(5));
        assert!(u.may_send_to(9));
        assert!(!a.may_send_to(9));
    }
}
