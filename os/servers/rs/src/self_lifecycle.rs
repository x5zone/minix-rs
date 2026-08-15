//! RS self-lifecycle special cases (pure slice).
//!
//! Mirrors `minix3/minix/servers/rs/main.c:436-590` (boot self-upgrade,
//! `sef_cb_init_restart`/`sef_cb_init_lu`), `update.c:230-366`
//! (`srv_update`/`update_service`/`rollback_service`), and
//! `utility.c:387-412` + `manager.c:760-780` (`update_sig_mgrs`).
//! 18-rs-self-lifecycle.md.
//!
//! The action hooks (`srv_fork`/`getprocnr`/`sys_privctl`/`sched_init_proc`/
//! `vm_memctl`/`vm_update`/`sys_update`/`sys_whoami`/`cpf_reload` — 19,
//! `end_update` — 16, `init_service` — 12, `clone_slot`/`swap_slot`/
//! `activate_service`/`cleanup_service` — 10/15) are stated as call sites.
//! This module owns the role split, branch decisions and assertions.

use minix_types::Endpoint;

use crate::privilege::PrivFlags;
use crate::process_table::RupdateFlags;
use crate::service_slot::SysFlags;

/// Whether an update swaps kernel/VM slots before the in-RS swap.
///
/// C: `RS_DONTSWAP=0` / `RS_SWAP=1` — const.h:79-80.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwapFlag {
    /// No pre-swap; the slots are already in place. C: `RS_DONTSWAP` — const.h:79.
    DontSwap,
    /// Pre-swap via `srv_update` before the in-RS `swap_slot`. C: `RS_SWAP` — const.h:80.
    Swap,
}

/// Whether `update_service` runs `srv_update` first.
///
/// C: update.c:287-288 — `if (swap_flag == RS_SWAP) srv_update(...)`.
pub fn should_pre_swap(flag: SwapFlag) -> bool {
    flag == SwapFlag::Swap
}

/// The rollback swap flag.
///
/// C: update.c:355 — the new instance still initializing
/// (`RS_INIT_PENDING`) → `RS_DONTSWAP`, otherwise `RS_SWAP`.
pub fn rollback_swap_flag(init_pending: bool) -> SwapFlag {
    if init_pending {
        SwapFlag::DontSwap
    } else {
        SwapFlag::Swap
    }
}

/// Which slot-exchange channel `srv_update` uses.
///
/// C: `srv_update` — update.c:230-257: a VM source only performs the kernel
/// part (`sys_update`, with `SYS_UPD_ROLLBACK` for rollbacks); a non-VM
/// source uses `vm_update` unless VM is part of an in-progress
/// multi-component update and has not finished initializing yet (VM handles
/// the swap itself at state-transfer time then).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SrvUpdateAction {
    /// Kernel-only slot swap. C: `sys_update` — update.c:240-245.
    /// `rollback` mirrors `sys_upd_flags & SF_VM_ROLLBACK` → `SYS_UPD_ROLLBACK`.
    SysUpdate { rollback: bool },
    /// Full slot swap through VM. C: `vm_update` — update.c:246-250.
    VmUpdate,
    /// Skip: VM handles the swap at state-transfer/rollback time.
    /// C: update.c:251-254.
    Skip,
}

/// Classifies the `srv_update` channel.
///
/// C: update.c:230-257. `is_vm_multi` is `RUPDATE_IS_UPD_VM_MULTI()`
/// (const.h:113), `vm_init_done` is `RUPDATE_IS_VM_INIT_DONE()`
/// (const.h:107).
pub fn srv_update_action(
    src_e: Endpoint,
    is_vm_multi: bool,
    vm_init_done: bool,
    sys_upd_flags: SysFlags,
) -> SrvUpdateAction {
    if src_e == Endpoint::VM {
        SrvUpdateAction::SysUpdate {
            rollback: sys_upd_flags.contains(SysFlags::VM_ROLLBACK),
        }
    } else if !is_vm_multi || vm_init_done {
        SrvUpdateAction::VmUpdate
    } else {
        SrvUpdateAction::Skip
    }
}

/// Whether the caller is the new or the old RS instance during the boot
/// self-upgrade.
///
/// C: main.c:456 — `pid == 0` → the forked child is the new RS instance;
/// the parent keeps running until it yields to it (main.c:477-489).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelfUpgradeRole {
    /// The forked child: performs `update_service(RS_SWAP)` + `cpf_reload`
    /// + cleanup + memory pin. C: main.c:456-475.
    NewInstance,
    /// The original instance: sets up privileges and yields. C: main.c:477-489.
    OldInstance,
}

/// C: main.c:456 — `pid == 0` is the new RS instance.
pub fn self_upgrade_role(pid: i32) -> SelfUpgradeRole {
    if pid == 0 {
        SelfUpgradeRole::NewInstance
    } else {
        SelfUpgradeRole::OldInstance
    }
}

/// Whether a restarting RS ends the update in progress.
///
/// C: main.c:521-522 — `SRV_IS_UPDATING(old_rs_rp)` →
/// `end_update(ERESTART, RS_REPLY)` (mechanism: 16).
pub fn should_end_update_on_restart(old_is_updating: bool) -> bool {
    old_is_updating
}

/// The four `sef_cb_init_lu` invariants.
///
/// C: main.c:580-583 — `RUPDATE_IS_UPDATING()` ∧
/// `RUPDATE_IS_INITIALIZING()` ∧ `num_rpupds > 0` ∧
/// `num_init_ready_pending > 0`. The C code `assert`s them; the Rust
/// decision exposes the conjunction so the caller chooses the fail-closed
/// policy.
pub fn lu_init_invariants(
    flags: RupdateFlags,
    num_rpupds: usize,
    num_init_ready_pending: usize,
) -> bool {
    flags.contains(RupdateFlags::UPDATING)
        && flags.contains(RupdateFlags::INITIALIZING)
        && num_rpupds > 0
        && num_init_ready_pending > 0
}

/// Whether a replica slot is an RS-restart instance.
///
/// C: manager.c:766-767 — `(s_flags & (ROOT_SYS_PROC|RST_SYS_PROC)) ==
/// (ROOT_SYS_PROC|RST_SYS_PROC)`: a root system process that is also a
/// restarted instance (const.h:151,154).
pub fn is_rs_restart_replica(priv_flags: PrivFlags) -> bool {
    priv_flags.contains(PrivFlags::ROOT_SYS_PROC | PrivFlags::RST_SYS_PROC)
}

/// Signal-manager update for one instance.
///
/// C: `update_sig_mgrs` — utility.c:387-412: `sig_mgr` is passed as `SELF`
/// (resolved by the kernel at update time); `bak_sig_mgr` is the backup
/// signal manager endpoint or `NONE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SigMgrUpdate {
    /// C: `s_sig_mgr` — the primary signal manager (`SELF`).
    pub sig_mgr: Endpoint,
    /// C: `s_bak_sig_mgr` — the backup signal manager.
    pub bak_sig_mgr: Endpoint,
}

/// The backup signal-manager pair for an RS-restart replica.
///
/// C: manager.c:771-773 — the running RS gets the replica as backup
/// (`update_sig_mgrs(rs_rp, SELF, replica_endpoint)`), the replica has no
/// backup (`update_sig_mgrs(replica_rp, SELF, NONE)`). Returns `None` when
/// the replica is not an RS-restart instance.
pub fn sig_mgr_updates(
    rs_restart_replica: bool,
    replica_endpoint: Endpoint,
) -> Option<(SigMgrUpdate, SigMgrUpdate)> {
    if !rs_restart_replica {
        return None;
    }
    Some((
        SigMgrUpdate {
            sig_mgr: Endpoint::SELF,
            bak_sig_mgr: replica_endpoint,
        },
        SigMgrUpdate {
            sig_mgr: Endpoint::SELF,
            bak_sig_mgr: Endpoint::NONE,
        },
    ))
}

/// Whether an RS rollback needs the VM slot exchange.
///
/// C: update.c:342-345 — after `sys_whoami`, only a process that is not RS
/// itself (`me != RS_PROC_NR`) performs `vm_update(SF_VM_ROLLBACK)`;
/// RS rolling back into itself only swaps the in-RS slots.
pub fn rollback_needs_vm_update(me_endpoint: Endpoint) -> bool {
    me_endpoint != Endpoint::RS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_self_upgrade_role() {
        // C: main.c:456 — pid == 0 → new instance.
        assert_eq!(self_upgrade_role(0), SelfUpgradeRole::NewInstance);
        assert_eq!(self_upgrade_role(1), SelfUpgradeRole::OldInstance);
        assert_eq!(self_upgrade_role(12345), SelfUpgradeRole::OldInstance);
    }

    #[test]
    fn test_should_pre_swap() {
        // C: update.c:287-288 — only RS_SWAP pre-swaps.
        assert!(should_pre_swap(SwapFlag::Swap));
        assert!(!should_pre_swap(SwapFlag::DontSwap));
    }

    #[test]
    fn test_rollback_swap_flag() {
        // C: update.c:355 — init pending → DONTSWAP, else SWAP.
        assert_eq!(rollback_swap_flag(true), SwapFlag::DontSwap);
        assert_eq!(rollback_swap_flag(false), SwapFlag::Swap);
    }

    #[test]
    fn test_srv_update_action() {
        // C: update.c:230-257 — three mutually exclusive branches.
        // VM source → kernel-only sys_update.
        assert_eq!(
            srv_update_action(Endpoint::VM, false, false, SysFlags::empty()),
            SrvUpdateAction::SysUpdate { rollback: false }
        );
        // SF_VM_ROLLBACK sets the rollback bit.
        assert_eq!(
            srv_update_action(Endpoint::VM, false, false, SysFlags::VM_ROLLBACK),
            SrvUpdateAction::SysUpdate { rollback: true }
        );
        // Non-VM, no VM multi → vm_update.
        assert_eq!(
            srv_update_action(Endpoint::PM, false, false, SysFlags::VM_ROLLBACK),
            SrvUpdateAction::VmUpdate
        );
        // Non-VM, VM multi but VM init done → vm_update.
        assert_eq!(
            srv_update_action(Endpoint::PM, true, true, SysFlags::empty()),
            SrvUpdateAction::VmUpdate
        );
        // Non-VM, VM multi, VM not init done → skip.
        assert_eq!(
            srv_update_action(Endpoint::PM, true, false, SysFlags::empty()),
            SrvUpdateAction::Skip
        );
    }

    #[test]
    fn test_should_end_update_on_restart() {
        // C: main.c:521-522 — only when the old RS is RS_UPDATING.
        assert!(should_end_update_on_restart(true));
        assert!(!should_end_update_on_restart(false));
    }

    #[test]
    fn test_lu_init_invariants() {
        // C: main.c:580-583 — four conjunctions.
        let ok = RupdateFlags::UPDATING | RupdateFlags::INITIALIZING;
        assert!(lu_init_invariants(ok, 1, 1));
        // Each failure flips the result.
        assert!(!lu_init_invariants(RupdateFlags::INITIALIZING, 1, 1));
        assert!(!lu_init_invariants(RupdateFlags::UPDATING, 1, 1));
        assert!(!lu_init_invariants(ok, 0, 1));
        assert!(!lu_init_invariants(ok, 1, 0));
        assert!(!lu_init_invariants(RupdateFlags::empty(), 0, 0));
    }

    #[test]
    fn test_is_rs_restart_replica() {
        // C: manager.c:766-767 — (flags & 0x900) == 0x900.
        let rs_restart = PrivFlags::ROOT_SYS_PROC | PrivFlags::RST_SYS_PROC;
        assert!(is_rs_restart_replica(rs_restart));
        assert!(is_rs_restart_replica(
            rs_restart | PrivFlags::SYS_PROC | PrivFlags::PREEMPTIBLE
        ));
        // Root only, or restarted only, or neither.
        assert!(!is_rs_restart_replica(PrivFlags::ROOT_SYS_PROC));
        assert!(!is_rs_restart_replica(PrivFlags::RST_SYS_PROC));
        assert!(!is_rs_restart_replica(PrivFlags::empty()));
    }

    #[test]
    fn test_sig_mgr_updates() {
        // C: manager.c:771-773 — RS gets the replica as backup; the replica
        // has none.
        assert_eq!(sig_mgr_updates(false, Endpoint::RS), None);
        let (rs, replica) = sig_mgr_updates(true, Endpoint::PM).unwrap();
        assert_eq!(
            rs,
            SigMgrUpdate {
                sig_mgr: Endpoint::SELF,
                bak_sig_mgr: Endpoint::PM,
            }
        );
        assert_eq!(
            replica,
            SigMgrUpdate {
                sig_mgr: Endpoint::SELF,
                bak_sig_mgr: Endpoint::NONE,
            }
        );
    }

    #[test]
    fn test_rollback_needs_vm_update() {
        // C: update.c:342-345 — me != RS_PROC_NR → vm_update(SF_VM_ROLLBACK).
        assert!(!rollback_needs_vm_update(Endpoint::RS));
        assert!(rollback_needs_vm_update(Endpoint::PM));
        assert!(rollback_needs_vm_update(Endpoint::VM));
    }

    #[test]
    fn test_constants() {
        // C: const.h:79-80 + rs.h:198 + const.h:151,154.
        assert_eq!(
            PrivFlags::ROOT_SYS_PROC.bits() | PrivFlags::RST_SYS_PROC.bits(),
            0x900
        );
        assert_eq!(SysFlags::VM_ROLLBACK.bits(), 0x080);
        assert_eq!(RupdateFlags::UPDATING.bits(), 0x080);
        assert_eq!(RupdateFlags::INITIALIZING.bits(), 0x040);
    }
}
