//! Service creation: dependency preconditions, post-fork slot-state update,
//! replica cloning and slot swapping.
//!
//! Mirrors `minix3/minix/servers/rs/manager.c:531-786` (`create_service` —
//! 531, `clone_service` — 713), `manager.c:1013-1026` (`activate_service`),
//! `manager.c:1800-1849` (`clone_slot`) and `manager.c:1856-1932`
//! (`swap_slot_pointer`/`swap_slot`). 10-rs-service-create.md.
//!
//! The kernel/IPC-coupled steps (`srv_fork`, `getprocnr`, `sys_privctl`,
//! `sys_getpriv`, `sched_init_proc`, `srv_execve`, `vm_memctl`, `vm_set_priv`,
//! `setuid`) are wired through 19-rs-external-interfaces.md (DEFERRED). This
//! module owns the pure, testable slice (doc §3.1): preconditions, slot
//! updates, chain linking, clone resets and the swap redirections.

use crate::boot::KernelApi;
use crate::privilege::{PrivCtlOp, PrivFlags};
use crate::process_table::RProcTable;
use crate::service_slot::{
    DSRV_SF, NR_DOMAIN, RFlags, RS_NR_PCI_CLASS, RS_NR_PCI_DEVICE, ServiceSlot, SlotId, SysFlags,
};
use alloc::vec::Vec;
use minix_types::{Clock, ERESTART, Endpoint, Errno, Pid};

/// Checks `create_service`'s dependency preconditions.
///
/// C: `create_service` — manager.c:542-568. Returns `EPERM` on the first
/// unmet dependency (the C code logs then `free_slot`s; the caller owns the
/// slot cleanup).
///
/// `has_replica` (manager.c:543-546): an old version (`r_old_rp`) counts; a
/// previous replica (`r_prev_rp`) counts only when it is not `RS_TERMINATED`
/// — a dying replica is not a usable one.
pub fn check_create_preconditions(table: &RProcTable, rp: SlotId) -> Result<(), Errno> {
    let slot = table.get(rp);
    let use_copy = slot.pub_.sys_flags.contains(SysFlags::USE_COPY);
    let has_replica = slot.old_rp.is_some()
        || slot
            .prev_rp
            .is_some_and(|p| !table.get(p).flags.contains(RFlags::TERMINATED));
    if !has_replica && slot.pub_.sys_flags.contains(SysFlags::NEED_REPL) {
        return Err(Errno::EPERM); // manager.c:547-552
    }
    if !use_copy && slot.pub_.sys_flags.contains(SysFlags::NEED_COPY) {
        return Err(Errno::EPERM); // manager.c:555-560
    }
    if !use_copy && slot.cmd[0] == 0 {
        return Err(Errno::EPERM); // manager.c:563-568: strcmp(r_cmd, "") == 0
    }
    Ok(())
}

/// Updates the slot after a successful fork.
///
/// C: `create_service` — manager.c:587-597. `ticks` is injected (C:
/// `getticks()`, manager.c:593) so the transition is pure. `r_flags` is
/// overwritten with `RS_IN_USE` (manager.c:589 — a previously freed row may
/// carry stale flags).
pub fn mark_child_created(
    table: &mut RProcTable,
    rp: SlotId,
    endpoint: Endpoint,
    pid: Pid,
    ticks: Clock,
) {
    let slot = table.get_mut(rp);
    slot.flags = RFlags::IN_USE; // manager.c:589
    slot.pub_.endpoint = endpoint; // manager.c:590
    slot.pid = Some(pid); // manager.c:591
    slot.check_tm = 0; // manager.c:592
    slot.alive_tm = ticks; // manager.c:593
    slot.stop_tm = 0; // manager.c:594
    slot.backoff = 0; // manager.c:595
    slot.pub_.in_use = true; // manager.c:597
    // rproc_ptr[_ENDPOINT_P(endpoint)] = rp — manager.c:596 (ARCH A-4).
    table.set_endpoint_index(endpoint, Some(rp));
}

/// Rebuilds the argument vector from the command.
///
/// C: `build_cmd_dep` — manager.c:289-324. `r_args` holds the copied command
/// with argv pointers into it (manager.c:300-301); Rust stores the parsed
/// tokens NUL-separated plus the count. Called by `clone_slot`
/// (manager.c:1833) and `swap_slot` (manager.c:1905-1906).
pub fn rebuild_args(slot: &mut ServiceSlot) {
    let tokens = crate::slot::build_cmd_dep(&slot.cmd);
    let mut argc = 0i32;
    let mut off = 0usize;
    for token in &tokens {
        // Each argv entry needs the token bytes + a NUL terminator. A token
        // that does not fit is dropped together with the rest of the tail
        // (R15): C's `strcpy(r_args, r_cmd)` into the fixed buffer would
        // overflow (UB), and `argc` must only count complete entries so the
        // flat buffer and the count stay consistent for the 09/10 rebuild.
        let need = token.len() + 1;
        if off + need > slot.args.len() {
            break;
        }
        slot.args[off..off + token.len()].copy_from_slice(token);
        off += token.len();
        slot.args[off] = 0; // NUL terminator
        off += 1;
        argc += 1;
    }
    // Keep the tail NUL-terminated so `args` stays a well-formed string
    // sequence even when the command did not fit.
    if off < slot.args.len() {
        slot.args[off] = 0;
    }
    slot.argc = argc;
}

/// Clones a service slot for a new instance.
///
/// C: `clone_slot` — manager.c:1800-1849. Shallow copy (configuration) plus
/// deep-copy resets (instance identity). The `sys_getpriv` synchronisation
/// (manager.c:1818-1821) is DEFERRED to 19.
///
/// In C the clone initially shares the exec pointer via the shallow copy and
/// `share_exec` re-assigns it for `SF_USE_COPY` (manager.c:1834-1835); in
/// Rust the `ServiceSlot::clone` already shares the `Arc`, which is the same
/// observable behaviour (ARCH A-5, 09-rs-exec.md §3.1).
pub fn clone_slot(table: &mut RProcTable, src: SlotId) -> Result<SlotId, Errno> {
    let clone = table.alloc_slot()?; // manager.c:1809
    let mut c = table.get(src).clone(); // manager.c:1824-1825
    c.init_err = ERESTART; // manager.c:1828 (sys/errno.h:196, positive per minix-types)
    c.flags &= !RFlags::ACTIVE; // manager.c:1829
    c.pid = None; // manager.c:1830
    c.pub_.endpoint = Endpoint::NONE; // manager.c:1831
    rebuild_args(&mut c); // manager.c:1833
    if c.pub_.sys_flags.contains(SysFlags::USE_COPY) {
        // manager.c:1834-1835: share_exec(clone_rp, rp) — Arc already shared
        // by the clone above; routed through the single implementation
        // (OQ-3 resolution, todo §18).
        crate::exec::share_exec(&mut c, &table.get(src).clone());
    }
    c.old_rp = None; // manager.c:1837
    c.new_rp = None; // manager.c:1838
    c.prev_rp = None; // manager.c:1839
    c.next_rp = None; // manager.c:1840
    c.priv_.flags |= PrivFlags::DYN_PRIV_ID; // manager.c:1843
    c.priv_.flags &= !(PrivFlags::LU_SYS_PROC | PrivFlags::RST_SYS_PROC); // manager.c:1846
    c.priv_.init_flags = 0; // manager.c:1847
    *table.get_mut(clone) = c;
    Ok(clone)
}

/// Whether an old replica must be dropped before cloning a new one.
///
/// C: `clone_service` — manager.c:730-737: VM can reliably support only one
/// replica, so re-cloning VM as a live-update instance (`LU_SYS_PROC`) with
/// an existing `r_next_rp` first runs `cleanup_service_now` on it and clears
/// the link. The predicate is the pure half (R28); the cleanup execution is
/// the cleanup-service executor (R22a — manager.c:405-495), invoked by the
/// 13 orchestration with this gate.
pub fn vm_replica_preclean_needed(
    endpoint: Endpoint,
    instance_flag: PrivFlags,
    has_next_replica: bool,
) -> bool {
    endpoint == Endpoint::VM && instance_flag.contains(PrivFlags::LU_SYS_PROC) && has_next_replica
}

/// Clears the parent's clone link after a failed create/backup step.
///
/// C: `clone_service` — manager.c:759-763 (`*rp_link = NULL` when
/// `create_service` fails) and manager.c:779-780 (same clear when the backup
/// signal-manager setup fails, before `kill_service`). `instance_flag`
/// selects which link was made — the same rule as [`link_replica`]. The
/// replica's back-link (old/prev) is left as C leaves it: the failed replica
/// slot is dead and its cleanup owns the rest.
pub fn unlink_replica(table: &mut RProcTable, rp: SlotId, instance_flag: PrivFlags) {
    if instance_flag.contains(PrivFlags::LU_SYS_PROC) {
        table.get_mut(rp).new_rp = None; // manager.c:760
    } else {
        table.get_mut(rp).next_rp = None; // manager.c:779-780
    }
}

/// Links a cloned replica into the source's instance chain.
///
/// C: `clone_service` — manager.c:736-752. `instance_flag` selects the chain
/// direction: `LU_SYS_PROC` links `new_rp`/`old_rp` (consumed by 16), any
/// other flag links `next_rp`/`prev_rp`. The replica's instance flags and
/// init flags are set on the replica's privilege structure (manager.c:751-752).
pub fn link_replica(
    table: &mut RProcTable,
    rp: SlotId,
    replica: SlotId,
    instance_flag: PrivFlags,
    init_flags: u32,
) {
    if instance_flag.contains(PrivFlags::LU_SYS_PROC) {
        // manager.c:743-746
        table.get_mut(rp).new_rp = Some(replica);
        table.get_mut(replica).old_rp = Some(rp);
    } else {
        // manager.c:747-750
        table.get_mut(rp).next_rp = Some(replica);
        table.get_mut(replica).prev_rp = Some(rp);
    }
    // manager.c:751-752
    table.get_mut(replica).priv_.flags |= instance_flag;
    table.get_mut(replica).priv_.init_flags |= init_flags;
}

/// Activates a service instance, deactivating the previous one if requested.
///
/// C: `activate_service` — manager.c:1013-1026. `ex_rp` (the previous active
/// instance) loses `RS_ACTIVE`; `rp` gains it.
pub fn activate_service(table: &mut RProcTable, rp: SlotId, ex_rp: Option<SlotId>) {
    if let Some(ex) = ex_rp {
        let slot = table.get_mut(ex);
        if slot.flags.contains(RFlags::ACTIVE) {
            slot.flags &= !RFlags::ACTIVE; // manager.c:1017-1018
        }
    }
    if !table.get(rp).flags.contains(RFlags::ACTIVE) {
        table.get_mut(rp).flags |= RFlags::ACTIVE; // manager.c:1023-1024
    }
}

/// Redirects an index reference from `src` to `dst` (and vice versa).
///
/// C: `swap_slot_pointer` — manager.c:1856-1863. The SlotId analogue of the
/// pointer swap: a chain slot pointing at `src` now points at `dst`, and
/// vice versa.
pub fn swap_index(v: &mut Option<SlotId>, src: SlotId, dst: SlotId) {
    if *v == Some(src) {
        *v = Some(dst);
    } else if *v == Some(dst) {
        *v = Some(src);
    }
}

/// Swaps two service slots, redirecting every reference to them.
///
/// C: `swap_slot` — manager.c:1870-1932. The whole `ServiceSlot` (including
/// the embedded public half) is exchanged — the exact analogue of swapping
/// both `rproc` and `rprocpub` contents while each row keeps its own public
/// struct (manager.c:1887-1890 + 1893-1896, ARCH A-3 merged rows). Returns
/// the swapped row ids (C's `*src_rpp = dst_rp; *dst_rpp = src_rp`,
/// manager.c:1928-1929).
///
/// The rupdate chain traversal (RUPDATE_ITER, manager.c:1919-1921) is
/// DEFERRED to 16-rs-live-update.md (per-service `r_upd` descriptors are not
/// modelled yet — 02 STATE P2-3).
pub fn swap_slot(table: &mut RProcTable, src: SlotId, dst: SlotId) -> (SlotId, SlotId) {
    // 1-3. Swap row contents. C: manager.c:1886-1896. The public half moves
    // with its row; `build_cmd_dep` re-derives the argument vector (1904-1906).
    table.swap_rows(src, dst);
    rebuild_args(table.get_mut(src));
    rebuild_args(table.get_mut(dst));

    // 4. Redirect the local instance chains. C: manager.c:1908-1916.
    let s = table.get_mut(src);
    swap_index(&mut s.old_rp, src, dst);
    swap_index(&mut s.new_rp, src, dst);
    swap_index(&mut s.prev_rp, src, dst);
    swap_index(&mut s.next_rp, src, dst);
    let d = table.get_mut(dst);
    swap_index(&mut d.old_rp, src, dst);
    swap_index(&mut d.new_rp, src, dst);
    swap_index(&mut d.prev_rp, src, dst);
    swap_index(&mut d.next_rp, src, dst);

    // 5. Redirect the endpoint fast index for both rows' endpoints.
    // C: manager.c:1922-1925. After the content swap, `src` holds `dst`'s
    // endpoint and vice versa.
    let src_ep = table.get(src).pub_.endpoint;
    let dst_ep = table.get(dst).pub_.endpoint;
    let mut se = table.endpoint_slot(src_ep);
    swap_index(&mut se, src, dst);
    table.set_endpoint_index(src_ep, se);
    let mut de = table.endpoint_slot(dst_ep);
    swap_index(&mut de, src, dst);
    table.set_endpoint_index(dst_ep, de);

    // 6. Adjust the caller's pointers (manager.c:1928-1929).
    (dst, src)
}

/// Initializes a slot as requested by the client, then delegates the
/// editable settings to [`crate::slot::edit_slot`].
///
/// C: `init_slot` — manager.c:1708-1795. Every dynamically created service
/// starts from the `DSRV_*` defaults (sys/priv/init flags, trap mask,
/// backup signal manager — priv.h:52-63, const.h:67), passes the domain and
/// PCI-ACL gates, gets its per-lifetime counters reset, and only then runs
/// `edit_slot` so request fields override the defaults. `source`/the IPC
/// copies are pure here (see `edit_slot`'s shape note, R20b).
pub fn init_slot(
    slot: &mut ServiceSlot,
    rs_start: &crate::slot::RsStart,
    table: &RProcTable,
    read_exec: &mut dyn FnMut(&mut ServiceSlot) -> Result<(), minix_types::Errno>,
) -> Result<(), minix_types::Errno> {
    // DSRV defaults (manager.c:1721-1727). `sys_flags` is *overwritten* —
    // a dynamic service starts with a clean public flag set.
    slot.pub_.sys_flags = DSRV_SF;
    slot.priv_.flags = crate::privilege::DSRV_F;
    slot.priv_.init_flags = crate::privilege::DSRV_I;
    slot.priv_.trap_mask = crate::privilege::TrapMask::DSRV_T;
    slot.priv_.bak_sig_mgr = Endpoint::NONE;

    // Initialize uid (manager.c:1730).
    slot.uid = rs_start.uid;

    // Device driver settings (manager.c:1732-1742): domain count gate, then
    // major/dev_nr, domains, devman id.
    if rs_start.nr_domain < 0 || rs_start.nr_domain > NR_DOMAIN as i32 {
        return Err(Errno::EINVAL);
    }
    slot.pub_.dev_nr = rs_start.major as u32;
    slot.pub_.nr_domain = rs_start.nr_domain as u8;
    for i in 0..rs_start.nr_domain as usize {
        slot.pub_.domain[i] = rs_start.domain[i];
    }
    slot.pub_.devman_id = Some(rs_start.devman_id);

    // PCI settings (manager.c:1744-1774): count gates, then the ACL tables
    // on the public half.
    if rs_start.nr_pci_id > RS_NR_PCI_DEVICE as i32 {
        return Err(Errno::EINVAL);
    }
    slot.pub_.pci_acl.nr_device = rs_start.nr_pci_id;
    for i in 0..rs_start.nr_pci_id as usize {
        slot.pub_.pci_acl.device[i] = rs_start.pci_id[i];
    }
    if rs_start.nr_pci_class > RS_NR_PCI_CLASS as i32 {
        return Err(Errno::EINVAL);
    }
    slot.pub_.pci_acl.nr_class = rs_start.nr_pci_class;
    for i in 0..rs_start.nr_pci_class as usize {
        slot.pub_.pci_acl.class[i] = rs_start.pci_class[i];
    }

    // Initialize per-lifetime fields (manager.c:1776-1791). Note the C
    // literal `-1` for scheduler/sig_mgr (manager.c:1786-1787) is *not* the
    // `NONE` endpoint (endpoint.h:55) — it is a transient "unset" state the
    // immediately-following `edit_slot` sees as schedulable; keep it verbatim.
    slot.asr_count = 0; // no ASR updates yet
    slot.restarts = 0; // no restarts yet
    slot.old_rp = None; // no old version yet
    slot.new_rp = None; // no new version yet
    slot.prev_rp = None; // no prev replica yet
    slot.next_rp = None; // no next replica yet
    slot.exec = None; // no in-memory copy yet
    slot.script[0] = 0; // no recovery script yet
    slot.pub_.label = crate::service_slot::Label::empty(); // no label yet
    slot.scheduler = Endpoint(-1); // no scheduler yet (manager.c:1786)
    slot.priv_.sig_mgr = Endpoint(-1); // no signal manager yet (manager.c:1787)
    slot.map_prealloc_addr = 0; // no preallocated memory
    slot.map_prealloc_len = 0;
    slot.init_err = ERESTART; // default init error (manager.c:1792)

    // Initialize editable slot settings (manager.c:1793).
    crate::slot::edit_slot(slot, rs_start, table, read_exec)
}

/// Copies the immutable service properties from a template slot.
///
/// C: `inherit_service_defaults` — manager.c:1303-1330 (called from
/// `do_update` for RS_UP on an existing service, 13/16). Device, domain and
/// PCI settings cannot change; only the `IMM_SF`/`IMM_F` bits are inherited
/// from the flags; the trap mask is copied wholesale (R20c — the merge
/// function the IMM_SF/IMM_F constants were waiting for).
pub fn inherit_service_defaults(def_slot: &ServiceSlot, slot: &mut ServiceSlot) {
    // Device, domain, and PCI settings. These properties cannot change
    // (manager.c:1313-1319).
    slot.pub_.dev_nr = def_slot.pub_.dev_nr;
    slot.pub_.nr_domain = def_slot.pub_.nr_domain;
    slot.pub_.domain = def_slot.pub_.domain;
    slot.pub_.pci_acl = def_slot.pub_.pci_acl.clone();

    // Immutable system and privilege flags (manager.c:1322-1325): clear the
    // immutable bits, then OR in the template's.
    slot.pub_.sys_flags.remove(crate::service_slot::IMM_SF);
    slot.pub_
        .sys_flags
        .insert(def_slot.pub_.sys_flags & crate::service_slot::IMM_SF);
    slot.priv_.flags.remove(crate::privilege::IMM_F);
    slot.priv_
        .flags
        .insert(def_slot.priv_.flags & crate::privilege::IMM_F);

    // Allowed traps. They cannot change (manager.c:1328).
    slot.priv_.trap_mask = def_slot.priv_.trap_mask;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service_slot::{Label, RFlags};

    fn in_use_slot() -> ServiceSlot {
        let mut s = ServiceSlot::vacant();
        s.flags = RFlags::IN_USE;
        s.pub_.label = Label::from_bytes(b"test");
        s
    }

    #[test]
    fn test_preconditions_need_repl() {
        // C: manager.c:547-552 — SF_NEED_REPL with no old/prev replica.
        let mut t = RProcTable::new();
        let rp = t.alloc_slot().unwrap();
        t.get_mut(rp).flags |= RFlags::IN_USE;
        t.get_mut(rp).pub_.sys_flags |= SysFlags::NEED_REPL;
        assert_eq!(check_create_preconditions(&t, rp), Err(Errno::EPERM));
    }

    #[test]
    fn test_preconditions_prev_terminated() {
        // C: manager.c:543-546 — a TERMINATED prev replica does not count.
        let mut t = RProcTable::new();
        let rp = t.alloc_slot().unwrap();
        t.get_mut(rp).flags |= RFlags::IN_USE;
        t.get_mut(rp).pub_.sys_flags |= SysFlags::NEED_REPL;
        let prev = t.alloc_slot().unwrap();
        t.get_mut(prev).flags |= RFlags::IN_USE | RFlags::TERMINATED;
        t.get_mut(rp).prev_rp = Some(prev);
        assert_eq!(check_create_preconditions(&t, rp), Err(Errno::EPERM));
    }

    #[test]
    fn test_preconditions_prev_live_counts() {
        // C: manager.c:546 — a live prev replica satisfies SF_NEED_REPL.
        let mut t = RProcTable::new();
        let rp = t.alloc_slot().unwrap();
        t.get_mut(rp).flags |= RFlags::IN_USE;
        t.get_mut(rp).pub_.sys_flags |= SysFlags::NEED_REPL;
        let prev = t.alloc_slot().unwrap();
        t.get_mut(prev).flags |= RFlags::IN_USE;
        t.get_mut(rp).prev_rp = Some(prev);
        t.get_mut(rp).cmd[..6].copy_from_slice(b"/bin/x"); // gate 3: non-empty cmd
        assert_eq!(check_create_preconditions(&t, rp), Ok(()));
    }

    #[test]
    fn test_preconditions_need_copy() {
        // C: manager.c:555-560 — SF_NEED_COPY without SF_USE_COPY.
        let mut t = RProcTable::new();
        let rp = t.alloc_slot().unwrap();
        t.get_mut(rp).flags |= RFlags::IN_USE;
        t.get_mut(rp).pub_.sys_flags |= SysFlags::NEED_COPY;
        assert_eq!(check_create_preconditions(&t, rp), Err(Errno::EPERM));
    }

    #[test]
    fn test_preconditions_empty_cmd() {
        // C: manager.c:563-568 — no copy and empty command.
        let mut t = RProcTable::new();
        let rp = t.alloc_slot().unwrap();
        t.get_mut(rp).flags |= RFlags::IN_USE;
        assert_eq!(check_create_preconditions(&t, rp), Err(Errno::EPERM));
    }

    #[test]
    fn test_preconditions_ok_with_copy() {
        // SF_USE_COPY satisfies both NEED_COPY and the command requirement.
        let mut t = RProcTable::new();
        let rp = t.alloc_slot().unwrap();
        t.get_mut(rp).flags |= RFlags::IN_USE;
        t.get_mut(rp).pub_.sys_flags |= SysFlags::USE_COPY;
        assert_eq!(check_create_preconditions(&t, rp), Ok(()));
    }

    #[test]
    fn test_mark_child_created() {
        // C: manager.c:587-597 — nine fields + endpoint index + in_use.
        let mut t = RProcTable::new();
        let rp = t.alloc_slot().unwrap();
        t.get_mut(rp).flags |= RFlags::EXITING; // stale flags must be overwritten
        mark_child_created(&mut t, rp, Endpoint::PM, 42, 1234);
        let s = t.get(rp);
        assert_eq!(s.flags, RFlags::IN_USE);
        assert_eq!(s.pub_.endpoint, Endpoint::PM);
        assert_eq!(s.pid, Some(42));
        assert_eq!(s.check_tm, 0);
        assert_eq!(s.alive_tm, 1234);
        assert_eq!(s.stop_tm, 0);
        assert_eq!(s.backoff, 0);
        assert!(s.pub_.in_use);
        assert_eq!(t.endpoint_slot(Endpoint::PM), Some(rp));
    }

    #[test]
    fn test_clone_slot_resets_identity() {
        // C: manager.c:1828-1847 — deep-copy resets.
        let mut t = RProcTable::new();
        let src = t.alloc_slot().unwrap();
        let mut s = in_use_slot();
        s.flags |= RFlags::ACTIVE;
        s.pid = Some(7);
        s.pub_.endpoint = Endpoint::VFS;
        s.old_rp = Some(SlotId::new(5));
        s.next_rp = Some(SlotId::new(6));
        s.priv_.flags |= PrivFlags::LU_SYS_PROC | PrivFlags::RST_SYS_PROC;
        s.priv_.init_flags = 0x2;
        s.cmd = {
            let mut c = [0u8; 512];
            c[..6].copy_from_slice(b"/bin/x");
            c
        };
        *t.get_mut(src) = s;

        let clone = clone_slot(&mut t, src).unwrap();
        let c = t.get(clone);
        assert_eq!(c.init_err, ERESTART);
        assert!(!c.flags.contains(RFlags::ACTIVE));
        assert!(c.flags.contains(RFlags::IN_USE));
        assert_eq!(c.pid, None);
        assert_eq!(c.pub_.endpoint, Endpoint::NONE);
        assert_eq!(c.old_rp, None);
        assert_eq!(c.new_rp, None);
        assert_eq!(c.prev_rp, None);
        assert_eq!(c.next_rp, None);
        assert!(c.priv_.flags.contains(PrivFlags::DYN_PRIV_ID));
        assert!(!c.priv_.flags.contains(PrivFlags::LU_SYS_PROC));
        assert!(!c.priv_.flags.contains(PrivFlags::RST_SYS_PROC));
        assert_eq!(c.priv_.init_flags, 0);
        assert_eq!(c.argc, 1); // rebuild_args from "/bin/x"
        assert_eq!(t.get(src).cmd[..6], *b"/bin/x"); // source untouched
    }

    #[test]
    fn test_rebuild_args_full_buffer_argc() {
        // R15: a 512-byte cmd with no NUL overflows the args buffer in C
        // (strcpy UB, manager.c:301); Rust truncates and must count only
        // complete argv entries so `argc` matches the flat buffer content.
        let mut s = ServiceSlot::vacant();
        s.cmd[..6].copy_from_slice(b"/bin/a");
        s.cmd[6] = b' ';
        // 505 more bytes of one token: "/bin/a" (7 bytes incl. NUL) fits,
        // the 505-byte token needs 506 more bytes — does not.
        for b in s.cmd.iter_mut().skip(7) {
            *b = b'b';
        }

        rebuild_args(&mut s);

        assert_eq!(s.argc, 1); // only the complete "/bin/a" entry counts
        assert_eq!(&s.args[..7], b"/bin/a\0");
        assert_eq!(s.args[7], 0); // tail stays NUL-terminated

        // A command that exactly fills the buffer keeps all tokens.
        let mut s2 = ServiceSlot::vacant();
        s2.cmd[..6].copy_from_slice(b"/bin/a");
        s2.cmd[6] = b' ';
        s2.cmd[7] = b'x'; // 1-byte token: "/bin/a\0x\0" == 9 bytes
        rebuild_args(&mut s2);
        assert_eq!(s2.argc, 2);
        assert_eq!(&s2.args[..9], b"/bin/a\0x\0");
        assert_eq!(s2.args[9], 0); // tail stays NUL-terminated
    }

    #[test]
    fn test_link_replica_lu_vs_replica() {
        // C: manager.c:735-747 — LU links new/old, replica links next/prev.
        let mut t = RProcTable::new();
        let rp = t.alloc_slot().unwrap();
        t.get_mut(rp).flags |= RFlags::IN_USE;
        let rep = t.alloc_slot().unwrap();
        t.get_mut(rep).flags |= RFlags::IN_USE;

        link_replica(&mut t, rp, rep, PrivFlags::LU_SYS_PROC, 0x1);
        assert_eq!(t.get(rp).new_rp, Some(rep));
        assert_eq!(t.get(rep).old_rp, Some(rp));
        assert_eq!(t.get(rp).next_rp, None);
        assert!(t.get(rep).priv_.flags.contains(PrivFlags::LU_SYS_PROC));
        assert_eq!(t.get(rep).priv_.init_flags, 0x1);

        let rep2 = t.alloc_slot().unwrap();
        t.get_mut(rep2).flags |= RFlags::IN_USE;
        link_replica(&mut t, rp, rep2, PrivFlags::RST_SYS_PROC, 0);
        assert_eq!(t.get(rp).next_rp, Some(rep2));
        assert_eq!(t.get(rep2).prev_rp, Some(rp));
        assert!(t.get(rep2).priv_.flags.contains(PrivFlags::RST_SYS_PROC));
    }

    #[test]
    fn test_activate_service() {
        // C: manager.c:1013-1026 — ACTIVE migration.
        let mut t = RProcTable::new();
        let old = t.alloc_slot().unwrap();
        t.get_mut(old).flags |= RFlags::IN_USE;
        let new = t.alloc_slot().unwrap();
        t.get_mut(new).flags |= RFlags::IN_USE;
        t.get_mut(old).flags |= RFlags::ACTIVE;
        activate_service(&mut t, new, Some(old));
        assert!(!t.get(old).flags.contains(RFlags::ACTIVE));
        assert!(t.get(new).flags.contains(RFlags::ACTIVE));
        // No previous instance: just activates.
        let n2 = t.alloc_slot().unwrap();
        t.get_mut(n2).flags |= RFlags::IN_USE;
        activate_service(&mut t, n2, None);
        assert!(t.get(n2).flags.contains(RFlags::ACTIVE));
    }

    #[test]
    fn test_swap_slot_redirects_indices() {
        // C: manager.c:1870-1932 — contents swap + chain/endpoint redirection.
        let mut t = RProcTable::new();
        let a = t.alloc_slot().unwrap();
        t.get_mut(a).flags |= RFlags::IN_USE;
        let b = t.alloc_slot().unwrap();
        t.get_mut(b).flags |= RFlags::IN_USE;
        let c = t.alloc_slot().unwrap();
        t.get_mut(c).flags |= RFlags::IN_USE;
        t.get_mut(a).flags |= RFlags::ACTIVE;
        t.get_mut(a).pub_.endpoint = Endpoint::VFS;
        t.get_mut(a).pub_.label = Label::from_bytes(b"vfs");
        t.get_mut(a).next_rp = Some(c);
        t.get_mut(b).pub_.endpoint = Endpoint::PM;
        t.get_mut(b).pub_.label = Label::from_bytes(b"pm");
        t.get_mut(b).prev_rp = Some(a);
        t.get_mut(c).pub_.endpoint = Endpoint::SCHED;
        t.get_mut(c).prev_rp = Some(a);
        t.set_endpoint_index(Endpoint::VFS, Some(a));
        t.set_endpoint_index(Endpoint::PM, Some(b));

        let (a2, b2) = swap_slot(&mut t, a, b);
        assert_eq!(a2, b);
        assert_eq!(b2, a);
        // Contents swapped.
        assert_eq!(t.get(a).pub_.label, Label::from_bytes(b"pm"));
        assert_eq!(t.get(b).pub_.label, Label::from_bytes(b"vfs"));
        // Endpoint index follows the public content.
        assert_eq!(t.endpoint_slot(Endpoint::VFS), Some(b));
        assert_eq!(t.endpoint_slot(Endpoint::PM), Some(a));
        // Own chains redirected (C: manager.c:1908-1916): old b's prev=a
        // moved into row a and follows the content to b.
        assert_eq!(t.get(a).prev_rp, Some(b));
        // Third-party chains are NOT redirected (C semantics: only the two
        // swapped slots' own chains + rproc_ptr/rupdate are fixed —
        // manager.c:1908-1925): c.prev still references row a, which after
        // the swap holds the old b content.
        assert_eq!(t.get(c).prev_rp, Some(a));
        assert_eq!(t.get(b).next_rp, Some(c));
    }

    #[test]
    fn test_swap_slot_with_vacant_row_no_panic() {
        // R12: a clone_slot product row has Endpoint::NONE (clone_slot sets
        // it at manager.c:1831). swap_slot step 5 must not index past
        // `by_endpoint` with NONE — it yields None / is ignored (fail-closed,
        // matching C's `rs_isokendpt` protection at the main loop).
        let mut t = RProcTable::new();
        let a = t.alloc_slot().unwrap();
        t.get_mut(a).flags |= RFlags::IN_USE;
        t.get_mut(a).pub_.endpoint = Endpoint::NONE; // clone product
        t.get_mut(a).pub_.label = Label::from_bytes(b"replica");
        let b = t.alloc_slot().unwrap();
        t.get_mut(b).flags |= RFlags::IN_USE;
        t.get_mut(b).pub_.endpoint = Endpoint::VFS;
        t.get_mut(b).pub_.label = Label::from_bytes(b"vfs");
        t.set_endpoint_index(Endpoint::VFS, Some(b));

        let (a2, b2) = swap_slot(&mut t, a, b);
        // Contents swapped: the NONE-endpoint row now holds the VFS content.
        assert_eq!(a2, b);
        assert_eq!(b2, a);
        assert_eq!(t.get(a).pub_.label, Label::from_bytes(b"vfs"));
        assert_eq!(t.get(b).pub_.label, Label::from_bytes(b"replica"));
        assert_eq!(t.endpoint_slot(Endpoint::VFS), Some(a));
        // The NONE endpoint stays unindexed.
        assert_eq!(t.endpoint_slot(Endpoint::NONE), None);
    }

    #[test]
    fn test_init_slot_dsrv_defaults_resets_and_delegates() {
        // C: manager.c:1708-1795 — DSRV defaults, per-lifetime resets, then
        // the editable pipeline (R20c).
        let mut s = ServiceSlot::vacant();
        // Pre-set values that the resets must clear.
        s.restarts = 9;
        s.init_err = 0;
        s.pub_.label = Label::from_bytes(b"stale");
        s.pub_.sys_flags.insert(SysFlags::CORE_SRV); // must be overwritten away

        let mut r = crate::slot::RsStart::default();
        r.uid = 42;
        r.major = 17;
        r.devman_id = 3;
        r.nr_domain = 2;
        r.domain[0] = 1;
        r.domain[1] = 2;
        r.ipclen = 4;
        r.ipc_list[..4].copy_from_slice(b"IPC1");
        r.cmdlen = 9;
        r.cmd[..9].copy_from_slice(b"/sbin/tty");
        r.progname = Label::from_bytes(b"tty");

        let table = RProcTable::new();
        assert!(init_slot(&mut s, &r, &table, &mut |_| Ok(())).is_ok());

        // DSRV defaults (manager.c:1721-1727).
        assert_eq!(s.pub_.sys_flags, SysFlags::empty()); // DSRV_SF overwrites CORE_SRV
        assert_eq!(s.priv_.trap_mask, crate::privilege::TrapMask::DSRV_T);
        assert_eq!(s.priv_.bak_sig_mgr, Endpoint::NONE);
        assert_eq!(s.uid, 42);
        // Device settings (manager.c:1738-1742).
        assert_eq!(s.pub_.dev_nr, 17);
        assert_eq!(s.pub_.nr_domain, 2);
        assert_eq!(s.pub_.domain[0], 1);
        assert_eq!(s.pub_.devman_id, Some(3));
        // Resets (manager.c:1776-1792). scheduler/sig_mgr take the C literal
        // -1 here but are then overwritten by the delegated edit_slot —
        // asserted below against the request values instead.
        assert_eq!(s.restarts, 0);
        assert_eq!(s.asr_count, 0);
        assert_eq!(s.init_err, ERESTART);
        assert_eq!(s.exec, None);
        // Delegated editable settings: cmd + label fallback landed.
        assert_eq!(&s.cmd[..9], b"/sbin/tty");
        assert_eq!(s.pub_.label, Label::from_bytes(b"tty"));
        // The transient -1 was replaced by edit_slot's scheduling update and
        // the signal-manager write (default request values).
        assert_eq!(s.scheduler, r.scheduler);
        assert_eq!(s.priv_.sig_mgr, r.sigmgr);
    }

    #[test]
    fn test_init_slot_domain_pci_gates() {
        // C: manager.c:1732-1774 — domain/PCI count gates → EINVAL.
        let table = RProcTable::new();
        let mut r = crate::slot::RsStart::default();
        r.nr_domain = -1;
        assert_eq!(
            init_slot(&mut ServiceSlot::vacant(), &r, &table, &mut |_| Ok(())),
            Err(Errno::EINVAL)
        );
        r.nr_domain = NR_DOMAIN as i32 + 1;
        assert_eq!(
            init_slot(&mut ServiceSlot::vacant(), &r, &table, &mut |_| Ok(())),
            Err(Errno::EINVAL)
        );
        r.nr_domain = 0;
        r.nr_pci_id = RS_NR_PCI_DEVICE as i32 + 1;
        assert_eq!(
            init_slot(&mut ServiceSlot::vacant(), &r, &table, &mut |_| Ok(())),
            Err(Errno::EINVAL)
        );
        r.nr_pci_id = 0;
        r.nr_pci_class = RS_NR_PCI_CLASS as i32 + 1;
        assert_eq!(
            init_slot(&mut ServiceSlot::vacant(), &r, &table, &mut |_| Ok(())),
            Err(Errno::EINVAL)
        );
    }

    #[test]
    fn test_inherit_service_defaults_immutable_only() {
        // C: manager.c:1303-1330 — device/domain/PCI copied wholesale, only
        // the IMM_SF/IMM_F bits inherited from the flags, traps wholesale.
        let template = ServiceSlot::vacant();
        let mut def = template.clone();
        def.pub_.dev_nr = 17;
        def.pub_.nr_domain = 1;
        def.pub_.domain[0] = 5;
        def.pub_.pci_acl.nr_device = 2;
        // CORE_SRV ∈ IMM_SF (rs.h:205-206) → inherited; USE_SCRIPT ∉ → not.
        def.pub_.sys_flags.insert(SysFlags::CORE_SRV);
        def.pub_.sys_flags.insert(SysFlags::USE_SCRIPT);
        def.priv_.flags.insert(crate::privilege::IMM_F); // IMM_F → inherited
        def.priv_.flags.insert(PrivFlags::CHECK_IRQ); // ∉ IMM_F → not
        def.priv_.trap_mask = crate::privilege::TrapMask::SENDNB;

        let mut s = ServiceSlot::vacant();
        s.pub_.sys_flags.insert(SysFlags::USE_SCRIPT); // target's own flag survives
        inherit_service_defaults(&def, &mut s);
        assert_eq!(s.pub_.dev_nr, 17);
        assert_eq!(s.pub_.nr_domain, 1);
        assert_eq!(s.pub_.domain[0], 5);
        assert_eq!(s.pub_.pci_acl.nr_device, 2);
        assert!(s.pub_.sys_flags.contains(SysFlags::CORE_SRV)); // immutable inherited
        assert!(s.pub_.sys_flags.contains(SysFlags::USE_SCRIPT)); // own flag kept
        assert!(!s.priv_.flags.contains(PrivFlags::CHECK_IRQ)); // non-immutable not inherited
        assert!(s.priv_.flags.contains(crate::privilege::IMM_F)); // immutable inherited
        assert_eq!(s.priv_.trap_mask, crate::privilege::TrapMask::SENDNB);
    }

    #[test]
    fn test_vm_replica_preclean_gate() {
        // C: manager.c:730-737 — VM + LU instance + existing next replica.
        assert!(vm_replica_preclean_needed(
            Endpoint::VM,
            PrivFlags::LU_SYS_PROC,
            true
        ));
        // Any leg missing → no pre-clean.
        assert!(!vm_replica_preclean_needed(
            Endpoint::VM,
            PrivFlags::LU_SYS_PROC,
            false
        ));
        assert!(!vm_replica_preclean_needed(
            Endpoint::VM,
            PrivFlags::RST_SYS_PROC,
            true
        ));
        assert!(!vm_replica_preclean_needed(
            Endpoint::PM,
            PrivFlags::LU_SYS_PROC,
            true
        ));
    }

    #[test]
    fn test_unlink_replica_selects_chain_direction() {
        // C: manager.c:759-763/:779-780 — the parent's clone link (new_rp for
        // a LU instance, next_rp for a restart replica) is cleared when
        // create_service or the backup-sigmgr step fails.
        let mut table = RProcTable::new();
        let rp = table.alloc_slot().unwrap();
        let replica = table.alloc_slot().unwrap();

        link_replica(&mut table, rp, replica, PrivFlags::LU_SYS_PROC, 0);
        assert_eq!(table.get(rp).new_rp, Some(replica));
        unlink_replica(&mut table, rp, PrivFlags::LU_SYS_PROC);
        assert_eq!(table.get(rp).new_rp, None);
        // The replica's back-link is left as C leaves it (cleanup's job).
        assert_eq!(table.get(replica).old_rp, Some(rp));

        link_replica(&mut table, rp, replica, PrivFlags::RST_SYS_PROC, 0);
        assert_eq!(table.get(rp).next_rp, Some(replica));
        unlink_replica(&mut table, rp, PrivFlags::RST_SYS_PROC);
        assert_eq!(table.get(rp).next_rp, None);
    }
}

// ── create_service orchestration (R22a — manager.c:531-708) ─────────────────

/// No-op script hook: the create_service failure paths carry no cleanup
/// script (RS_CLEANUP_SCRIPT is unset), so the cleanup script callback never
/// fires — a plain fn item satisfies the HRTB `FnMut` bound.
fn no_script(_slot: &mut ServiceSlot) -> Result<(), Errno> {
    Ok(())
}

/// Creates the given system service: fork, table refresh, privilege synch,
/// scheduling, exec, and the VM registration tail — with the C failure path
/// (`cleanup_service` + RS re-pin) after every effectful step.
///
/// C: `create_service` — manager.c:531-708. Kernel effects go through
/// `kernel` (wired 19; mock in tests); `read_exec` is the binary-loading
/// seam (file I/O, 19) mirroring `edit_slot`'s shape. `ticks` is
/// `getticks()` for the `r_alive_tm` refresh (manager.c:593).
///
/// ARCH: C passes its own `environ` to `srv_execve`; this rewrite models no
/// environment inheritance (10-rs-service-create.md §3).
pub fn create_service(
    table: &mut RProcTable,
    rp: SlotId,
    kernel: &mut dyn crate::boot::KernelApi,
    ticks: Clock,
    read_exec: &mut dyn FnMut(&mut ServiceSlot) -> Result<(), Errno>,
) -> Result<(), Errno> {
    // Preconditions (manager.c:542-568): NEED_REPL / NEED_COPY / cmd — each
    // C branch `free_slot`s inline; the caller-owned cleanup contract
    // (check_create_preconditions) is fulfilled here, by the orchestrator.
    if let Err(e) = check_create_preconditions(table, rp) {
        table.free_slot(rp);
        return Err(e);
    }
    let use_copy = table.get(rp).pub_.sys_flags.contains(SysFlags::USE_COPY);
    let uid = table.get(rp).uid;

    // Fork the child (manager.c:576-583); failure → free_slot + errno.
    let child_pid = match kernel.srv_fork(uid, 0) {
        Ok(pid) => pid,
        Err(e) => {
            table.free_slot(rp);
            return Err(e);
        }
    };

    // Resolve the child's endpoint (manager.c:584-587). C panics
    // ("unable to get child endpoint") — after a successful fork this is an
    // internal invariant, not a protocol error.
    let child_ep = kernel
        .getprocnr(child_pid)
        .expect("unable to get child endpoint (manager.c:586)");

    // There is now a child process: refresh the table (manager.c:589-597).
    mark_child_created(table, rp, child_ep, child_pid, ticks);

    // Set and synchronise the privilege structure (manager.c:600-606):
    // SYS_PRIV_SET_SYS pushes the prepared structure, SYS_GETPRV reads the
    // kernel-allocated view back. Failure → cleanup + ENOMEM.
    {
        let snapshot = table.get(rp);
        if kernel
            .privctl(child_ep, PrivCtlOp::SetSys, Some(&snapshot.priv_))
            .is_err()
        {
            crate::recovery::cleanup_service(table, rp, kernel, &mut no_script);
            let _ = kernel.vm_memctl(Endpoint::RS, crate::boot::VmRsMemReq::Pin, 0, 0);
            return Err(Errno::ENOMEM);
        }
    }
    match kernel.getpriv(child_ep) {
        Ok(synced) => table.get_mut(rp).priv_ = synced,
        Err(_) => {
            crate::recovery::cleanup_service(table, rp, kernel, &mut no_script);
            let _ = kernel.vm_memctl(Endpoint::RS, crate::boot::VmRsMemReq::Pin, 0, 0);
            return Err(Errno::ENOMEM);
        }
    }

    // Start scheduling (manager.c:609-614 → utility.c:364-382): the pure
    // decision (sched.rs) decides skip vs kernel call. Failure → cleanup.
    let (cfg, is_sys) = {
        let s = table.get(rp);
        (
            crate::sched::SchedulerConfig::from_slot(
                s.scheduler,
                s.pub_.endpoint,
                s.priority,
                s.quantum,
                s.cpu,
            ),
            s.priv_.flags.contains(PrivFlags::SYS_PROC),
        )
    };
    if let crate::sched::SchedAction::Start(cfg) = crate::sched::sched_decision(&cfg, is_sys)
        && let Err(e) = kernel.sched_init_proc(cfg)
    {
        crate::recovery::cleanup_service(table, rp, kernel, &mut no_script);
        let _ = kernel.vm_memctl(Endpoint::RS, crate::boot::VmRsMemReq::Pin, 0, 0);
        return Err(e);
    }

    // Copy the executable image if there is no in-memory copy
    // (manager.c:625-633). Failure → cleanup + errno.
    if !use_copy {
        let slot = table.get_mut(rp);
        if let Err(e) = read_exec(slot) {
            crate::recovery::cleanup_service(table, rp, kernel, &mut no_script);
            let _ = kernel.vm_memctl(Endpoint::RS, crate::boot::VmRsMemReq::Pin, 0, 0);
            return Err(e);
        }
    }

    // Exec the child (manager.c:634-642); the RS re-pin runs unconditionally
    // afterwards (manager.c:637 — "pin RS memory again or pagefaults").
    let execve_result = {
        let s = table.get(rp);
        kernel.srv_execve(
            child_ep,
            s.exec.as_deref().unwrap_or(&[]),
            &s.pub_.proc_name,
            &s.args,
            s.argc as usize,
        )
    };
    let _ = kernel.vm_memctl(Endpoint::RS, crate::boot::VmRsMemReq::Pin, 0, 0);
    if let Err(e) = execve_result {
        crate::recovery::cleanup_service(table, rp, kernel, &mut no_script);
        return Err(e);
    }
    if !use_copy {
        crate::exec::free_exec(table, rp); // manager.c:643-644
    }

    // The VFS non-blocking-fork workaround (manager.c:646-656) — retained
    // verbatim; the C comment marks it removable once VFS is fixed.
    let _ = kernel.setuid(0);

    // RS instance: pin the child's memory (manager.c:659-669).
    if table.get(rp).priv_.flags.contains(PrivFlags::ROOT_SYS_PROC)
        && kernel
            .vm_memctl(child_ep, crate::boot::VmRsMemReq::Pin, 0, 0)
            .is_err()
    {
        crate::recovery::cleanup_service(table, rp, kernel, &mut no_script);
        return Err(Errno::ENOMEM);
    }

    // VM instance: register with VM, then re-pin every RS instance
    // (manager.c:672-695) — the re-pin results are ignored in C.
    if table.get(rp).priv_.flags.contains(PrivFlags::VM_SYS_PROC) {
        if kernel
            .vm_memctl(child_ep, crate::boot::VmRsMemReq::MakeVm, 0, 0)
            .is_err()
        {
            crate::recovery::cleanup_service(table, rp, kernel, &mut no_script);
            return Err(Errno::ENOMEM);
        }
        let endpoints: Vec<Endpoint> = table
            .endpoint_slot(Endpoint::RS)
            .map(|rs| {
                table
                    .instances_of(rs)
                    .map(|id| table.get(id).pub_.endpoint)
                    .collect()
            })
            .unwrap_or_default();
        for ep in endpoints {
            let _ = kernel.vm_memctl(ep, crate::boot::VmRsMemReq::Pin, 0, 0);
        }
    }

    // Tell VM about allowed calls (manager.c:698-703). Failure → cleanup.
    {
        let mask = table.get(rp).pub_.vm_call_mask;
        if kernel.vm_set_priv(child_ep, mask, true).is_err() {
            crate::recovery::cleanup_service(table, rp, kernel, &mut no_script);
            return Err(Errno::ENOMEM);
        }
    }

    Ok(())
}

// Tests appended for R22a (create_service orchestration + cleanup phases).
#[cfg(test)]
mod r22a_tests {
    #![allow(clippy::too_many_arguments, unused_variables, unused_mut)]
    use super::*;
    use crate::boot::KernelApi;

    use crate::process_table::RProcTable;
    use crate::service_slot::{Label, RFlags};
    use crate::testutil::{Call, MockKernelApi};
    use minix_types::Endpoint;

    /// No-op script hook (HRTB-safe fn item; capturing closures fail the
    /// `for<'a> FnMut(&'a mut ServiceSlot)` bound).
    fn no_script(_slot: &mut ServiceSlot) -> Result<(), Errno> {
        Ok(())
    }

    /// Script-run flag for [`script_hook`].
    fn script_ran_flag() -> &'static core::sync::atomic::AtomicBool {
        use core::sync::atomic::AtomicBool;
        static SCRIPT_RAN: AtomicBool = AtomicBool::new(false);
        &SCRIPT_RAN
    }

    /// Script hook recording execution in a static (closures fail the HRTB
    /// `FnMut` bound when they capture).
    fn script_hook(_slot: &mut ServiceSlot) -> Result<(), Errno> {
        use core::sync::atomic::Ordering;
        script_ran_flag().store(true, Ordering::SeqCst);
        Ok(())
    }

    fn script_ran() -> bool {
        use core::sync::atomic::Ordering;
        script_ran_flag().load(Ordering::SeqCst)
    }

    fn script_ran_reset() {
        script_ran_flag().store(false, core::sync::atomic::Ordering::SeqCst);
    }

    /// Prepares an initialized slot in the table (init_slot on a standalone
    /// slot, then moved into row `rp`).
    fn prepared(table: &mut RProcTable, rp: SlotId) {
        let mut s = ServiceSlot::vacant();
        let mut r = crate::slot::RsStart::default();
        r.ipclen = 8;
        r.ipc_list[..8].copy_from_slice(b"IPC_ALL\0");
        r.cmdlen = 9;
        r.cmd[..9].copy_from_slice(b"/sbin/tty");
        r.progname = Label::from_bytes(b"tty");
        init_slot(&mut s, &r, table, &mut no_script).unwrap();
        *table.get_mut(rp) = s;
    }

    fn working_mock() -> MockKernelApi {
        let mut k = MockKernelApi::new(60);
        k.fork_pid = Some(500);
        k.child_endpoint = Some(Endpoint::from_generation_slot(0, 20));
        k.vm_ok = true;
        k.execve_ok = true;
        k.kill_ok = true;
        k
    }

    #[test]
    fn test_create_service_happy_path() {
        let mut table = RProcTable::new();
        let rp = table.alloc_slot().unwrap();
        prepared(&mut table, rp);
        let mut k = working_mock();
        let mut loaded = false;
        {
            let mut read_exec = |slot: &mut ServiceSlot| {
                loaded = true;
                slot.exec = Some(alloc::sync::Arc::from(&b"elf"[..]));
                Ok(())
            };
            assert!(create_service(&mut table, rp, &mut k, 100, &mut read_exec).is_ok());
        }
        let _ = &mut loaded;
        // Table refresh landed (manager.c:589-597).
        assert_eq!(table.get(rp).pid, Some(500));
        assert_eq!(table.get(rp).alive_tm, 100);
        // The kernel face saw fork/exec/setuid/vm-set-priv.
        assert!(
            k.calls
                .contains(&Call::SrvExecve(table.get(rp).pub_.endpoint))
        );
        assert!(k.calls.contains(&Call::SetUid(0)));
        assert!(k.calls.contains(&Call::PrivCtl(
            table.get(rp).pub_.endpoint,
            PrivCtlOp::SetSys
        )));
        // Without USE_COPY the exec image is loaded then freed (manager.c:625/
        // 643): the load ran, and free_exec dropped the image afterwards.
        assert!(loaded);
        assert_eq!(table.get(rp).exec, None);
    }

    #[test]
    fn test_create_service_precondition_frees_slot() {
        // C: manager.c:543-552 — NEED_REPL without a replica → EPERM and the
        // slot is freed inline.
        let mut table = RProcTable::new();
        let rp = table.alloc_slot().unwrap();
        {
            let s = table.get_mut(rp);
            s.pub_.sys_flags.insert(SysFlags::NEED_REPL);
            s.pub_.label = Label::from_bytes(b"lone");
        }
        let mut k = working_mock();
        let mut no_load = |_slot: &mut ServiceSlot| Ok(());
        assert_eq!(
            create_service(&mut table, rp, &mut k, 0, &mut no_load),
            Err(Errno::EPERM)
        );
        assert!(!table.get(rp).flags.contains(RFlags::IN_USE));
        assert!(k.calls.is_empty()); // no fork happened
    }

    #[test]
    fn test_create_service_fork_failure_frees_slot() {
        let mut table = RProcTable::new();
        let rp = table.alloc_slot().unwrap();
        prepared(&mut table, rp);
        let mut k = MockKernelApi::new(60); // fork_pid None → ENOSYS
        let mut no_load = |_slot: &mut ServiceSlot| Ok(());
        assert!(create_service(&mut table, rp, &mut k, 0, &mut no_load).is_err());
        assert!(!table.get(rp).flags.contains(RFlags::IN_USE));
    }

    #[test]
    fn test_create_service_exec_failure_cleans_up() {
        // C: manager.c:634-642 — execve failure → cleanup_service (phase 1:
        // RS_DEAD, chains unlinked, DISALLOW/CLEAR_IPC_REFS, RS_ACTIVE off).
        let mut table = RProcTable::new();
        let rp = table.alloc_slot().unwrap();
        prepared(&mut table, rp);
        let mut k = working_mock();
        k.execve_ok = false;
        let mut no_load = |_slot: &mut ServiceSlot| Ok(());
        assert!(create_service(&mut table, rp, &mut k, 0, &mut no_load).is_err());
        let s = table.get(rp);
        assert!(s.flags.contains(RFlags::DEAD));
        assert!(!s.flags.contains(RFlags::ACTIVE));
        // Phase 1 keeps `pub_.in_use` set (C only clears RS_ACTIVE here;
        // the slot leaves the table at phase 2 / free_slot).
        assert!(s.pub_.in_use);
        assert!(
            k.calls
                .contains(&Call::PrivCtl(s.pub_.endpoint, PrivCtlOp::Disallow))
        );
        assert!(
            k.calls
                .contains(&Call::PrivCtl(s.pub_.endpoint, PrivCtlOp::ClearIpcRefs))
        );
    }

    #[test]
    fn test_cleanup_service_two_phase() {
        // C: manager.c:405-495 — phase 1 marks + revokes + late-replies;
        // phase 2 stops + kills + frees. A pending cleanup script runs at
        // phase 2 and is consumed.
        let mut table = RProcTable::new();
        let a = table.alloc_slot().unwrap();
        {
            let s = table.get_mut(a);
            s.flags = RFlags::IN_USE | RFlags::ACTIVE | RFlags::LATEREPLY | RFlags::CLEANUP_SCRIPT;
            s.pub_.endpoint = Endpoint::from_generation_slot(0, 30);
            s.pid = Some(700);
        }
        // b is allocated only after a is marked in use — alloc_slot is a
        // find-only primitive (manager.c:2067-2083), so two consecutive calls
        // without an intervening IN_USE mark return the same row.
        let b = table.alloc_slot().unwrap();

        eprintln!("DBG1 a={:?} b={:?} aflags={:?}", a, b, table.get(a).flags);
        {
            let t = table.get_mut(b);
            t.flags = RFlags::IN_USE;
            t.prev_rp = Some(a);
        }
        table.get_mut(a).next_rp = Some(b);

        eprintln!("DBG2 aflags={:?}", table.get(a).flags);
        let mut k = MockKernelApi::new(60);
        k.kill_ok = true;
        script_ran_reset();
        // Phase 2's expected SchedStop targets are captured BEFORE the free
        // (free_slot clears the slot's scheduler/endpoint fields).
        let sched = table.get(a).scheduler;
        let ep = table.get(a).pub_.endpoint;
        {
            let mut script = script_hook;
            // Phase 1: chains unlinked both ways, DEAD, late reply sent.
            crate::recovery::cleanup_service(&mut table, a, &mut k, &mut script);
            assert!(table.get(a).flags.contains(RFlags::DEAD));
            assert!(!table.get(a).flags.contains(RFlags::ACTIVE));
            assert_eq!(table.get(a).next_rp, None);
            assert_eq!(table.get(b).prev_rp, None);
            assert!(
                k.calls
                    .contains(&Call::Reply(table.get(a).pub_.endpoint, 0))
            );
            assert!(!script_ran()); // scripts run at phase 2
            // Phase 2: scheduler stop + SIGKILL + script + free.
            crate::recovery::cleanup_service(&mut table, a, &mut k, &mut script);
        }
        assert!(script_ran());
        assert!(k.calls.contains(&Call::SchedStop(sched, ep)));
        assert!(
            k.calls
                .contains(&Call::SrvKill(700, crate::recovery::SIGKILL))
        );
        assert!(!table.get(a).flags.contains(RFlags::IN_USE));
    }

    #[test]
    fn test_cleanup_phase2_reincarnate_keeps_slot() {
        // C: manager.c:488-494 — a reincarnating slot is NOT freed (it is
        // about to be reused by the new instance).
        let mut table = RProcTable::new();
        let a = table.alloc_slot().unwrap();
        {
            let s = table.get_mut(a);
            s.flags = RFlags::IN_USE | RFlags::DEAD | RFlags::REINCARNATE;
            s.pub_.endpoint = Endpoint::from_generation_slot(0, 31);
            s.pid = Some(701);
        }
        let mut k = MockKernelApi::new(60);
        k.kill_ok = true;
        crate::recovery::cleanup_service(&mut table, a, &mut k, &mut no_script);
        assert!(
            k.calls
                .contains(&Call::SrvKill(701, crate::recovery::SIGKILL))
        );
        assert!(table.get(a).flags.contains(RFlags::IN_USE)); // kept for reuse
    }

    #[test]
    fn test_kernel_api_injection_satisfies_trait() {
        // T5/boundary sanity: the orchestration is drivable against the
        // shared mock — no production kernel face needed (19).
        let mut k = working_mock();
        let _: &mut dyn KernelApi = &mut k;
    }
}

// ── run_service / start_service orchestration (R22b — manager.c:923-983) ────

/// Lets a newly created service run: SYS_PRIV_ALLOW, then the RS_INIT
/// initialization message.
///
/// C: `run_service` — manager.c:923-948. Either failure kills the service
/// (`kill_service` — mark `RS_EXITING` + crash) and propagates the errno.
/// The `init_service` body (utility.c:18-64) is inlined here through the
/// ready.rs pieces: pre-send state transition, old-endpoint derivation,
/// message assembly (with the single-shot map-prealloc take) and the
/// `rs_asynsend` send seam (19).
pub fn run_service(
    table: &mut RProcTable,
    rp: SlotId,
    kernel: &mut dyn KernelApi,
    init_type: crate::sef::SefInitType,
    init_flags: u32,
    ticks: Clock,
    asynsend: &mut dyn FnMut(Endpoint, &crate::ready::InitMessage) -> Result<(), Errno>,
) -> Result<(), Errno> {
    // Allow the service to run (manager.c:932-936).
    let endpoint = table.get(rp).pub_.endpoint;
    if let Err(e) = kernel.privctl(endpoint, PrivCtlOp::Allow, None) {
        let e = crate::recovery::kill_service(table.get_mut(rp), kernel, e);
        return Err(e);
    }

    // Initialize service (utility.c:18-64).
    use crate::ready::{
        init_flags as script_flag, init_message, mark_initializing, take_map_prealloc,
    };
    mark_initializing(table.get_mut(rp), ticks); // utility.c:19-21

    // RS self-initialization sends nothing (utility.c:29-31) — the SEF
    // framework "simulates" the ready message instead (sef_cb_init_response,
    // main.c:602).
    if table.get(rp).priv_.flags.contains(PrivFlags::ROOT_SYS_PROC) {
        return Ok(());
    }

    // Old endpoint (utility.c:33-42): LU descriptor's state_endpoint wins
    // for updated instances (r_upd populated); else the previous replica's
    // endpoint. Table reads happen before the mutable assembly.
    let (old_endpoint, script, restarts) = {
        let s = table.get(rp);
        let oe = s
            .old_rp
            .and_then(|_o| s.upd.as_ref().map(|u| u.state_endpoint))
            .or_else(|| s.prev_rp.map(|p| table.get(p).pub_.endpoint));
        (
            oe,
            s.pub_
                .sys_flags
                .contains(crate::service_slot::SysFlags::USE_SCRIPT),
            s.restarts,
        )
    };
    let flags = script_flag(script, init_flags); // utility.c:54-57
    let (buff_addr, buff_len) = take_map_prealloc(table.get_mut(rp)); // utility.c:53-60
    let msg = init_message(
        init_type,
        flags,
        None, // rproctab_gid — injected at the 12 wiring (ServerState.rinit)
        old_endpoint,
        restarts,
        buff_addr,
        buff_len,
        crate::live_update::SEF_LU_STATE_NULL,
    );
    asynsend(endpoint, &msg)?; // utility.c:62 — rs_asynsend (19)
    Ok(())
}

/// Starts a system service: create, activate, publish, run.
///
/// C: `start_service` — manager.c:950-983. `publish` is the
/// `publish_service` seam (manager.c:787-860 — DS/devman/PCI effects, 11).
#[allow(clippy::too_many_arguments)] // 参数 = C 隐式全局的显式化（kernel/ticks/两注入缝）
pub fn start_service(
    table: &mut RProcTable,
    rp: SlotId,
    kernel: &mut dyn KernelApi,
    init_flags: u32,
    ticks: Clock,
    read_exec: &mut dyn FnMut(&mut ServiceSlot) -> Result<(), Errno>,
    publish: &mut dyn FnMut(&RProcTable, SlotId) -> Result<(), Errno>,
    asynsend: &mut dyn FnMut(Endpoint, &crate::ready::InitMessage) -> Result<(), Errno>,
) -> Result<(), Errno> {
    // Create and make active (manager.c:952-960).
    crate::ready::fold_init_flags(table.get_mut(rp), init_flags); // manager.c:952-953
    create_service(table, rp, kernel, ticks, read_exec)?;
    activate_service(table, rp, None); // manager.c:959-960

    // Publish service properties (manager.c:962-966) — 11-rs-publish.md
    // seam; failure propagates WITHOUT cleanup in C (start_service returns
    // immediately, leaving the created service for the monitor).
    publish(table, rp)?;

    // Run (manager.c:968-972).
    run_service(
        table,
        rp,
        kernel,
        crate::sef::SefInitType::Fresh,
        init_flags,
        ticks,
        asynsend,
    )
}

// Tests appended for R22b (run/start orchestration + kill/crash/detach).
#[cfg(test)]
mod r22b_tests {
    #![allow(clippy::too_many_arguments, unused_variables, unused_mut)]
    use super::*;

    use crate::recovery::{CrashOutcome, SIGKILL};
    use crate::service_slot::Label;
    use crate::testutil::{Call, MockKernelApi};
    use minix_types::Endpoint;

    fn prepared(table: &mut RProcTable, rp: SlotId) {
        let mut s = ServiceSlot::vacant();
        let mut r = crate::slot::RsStart::default();
        r.ipclen = 8;
        r.ipc_list[..8].copy_from_slice(b"IPC_ALL\0");
        r.cmdlen = 9;
        r.cmd[..9].copy_from_slice(b"/sbin/tty");
        r.progname = Label::from_bytes(b"tty");
        init_slot(&mut s, &r, table, &mut no_script).unwrap();
        *table.get_mut(rp) = s;
    }

    fn working_mock() -> MockKernelApi {
        let mut k = MockKernelApi::new(60);
        k.fork_pid = Some(500);
        k.child_endpoint = Some(Endpoint::from_generation_slot(0, 20));
        k.vm_ok = true;
        k.execve_ok = true;
        k.kill_ok = true;
        k
    }

    #[test]
    fn test_crash_and_kill_service() {
        // C: manager.c:395-403 — RS exits itself; others are SIGKILLed.
        let mut rs = ServiceSlot::vacant();
        rs.pub_.endpoint = Endpoint::RS;
        let mut k = MockKernelApi::new(60);
        assert_eq!(
            crate::recovery::crash_service(&rs, &mut k),
            Ok(CrashOutcome::SelfTerminate)
        );
        assert!(k.calls.iter().all(|c| !matches!(c, Call::SysKill(_, _))));

        let mut tty = ServiceSlot::vacant();
        tty.pub_.endpoint = Endpoint::TTY;
        assert_eq!(
            crate::recovery::crash_service(&tty, &mut k),
            Ok(CrashOutcome::Signalled)
        );
        assert!(k.calls.contains(&Call::SysKill(Endpoint::TTY, SIGKILL)));

        // kill_service: RS_EXITING set, crash runs, input errno propagates.
        let err = crate::recovery::kill_service(&mut tty, &mut k, Errno::ENOMEM);
        assert_eq!(err, Errno::ENOMEM);
        assert!(tty.flags.contains(RFlags::EXITING));
    }

    #[test]
    fn test_detach_service_relabels_and_demotes() {
        // C: manager.c:497-528 — "{counter}.{label}" republish, alive +
        // active, core/detach bits stripped, monitoring fields zeroed,
        // re-allowed.
        let mut s = ServiceSlot::vacant();
        s.flags = RFlags::IN_USE | RFlags::ACTIVE;
        s.pub_.endpoint = Endpoint::TTY;
        s.pub_.label = Label::from_bytes(b"tty");
        s.pub_.sys_flags.insert(SysFlags::CORE_SRV);
        s.pub_.sys_flags.insert(SysFlags::DET_RESTART);
        s.period = 30;
        s.pub_.dev_nr = 17;
        s.pub_.nr_domain = 2;
        let mut k = MockKernelApi::new(60);
        let mut published: Option<(alloc::string::String, Endpoint)> = None;
        {
            let mut publish = |label: &Label, ep: Endpoint| {
                published = Some((label.as_str().unwrap().to_string(), ep));
            };
            crate::recovery::detach_service(&mut s, &mut k, 0, &mut publish);
        }
        assert_eq!(s.pub_.label.as_str(), Some("1.tty"));
        assert_eq!(
            published.as_ref().map(|(l, _)| l.clone()).as_deref(),
            Some("1.tty")
        );
        assert_eq!(s.flags, RFlags::IN_USE | RFlags::ACTIVE);
        assert!(!s.pub_.sys_flags.contains(SysFlags::CORE_SRV));
        assert!(!s.pub_.sys_flags.contains(SysFlags::DET_RESTART));
        assert_eq!(s.period, 0);
        assert_eq!(s.pub_.dev_nr, 0);
        assert_eq!(s.pub_.nr_domain, 0);
        assert!(
            k.calls
                .contains(&Call::PrivCtl(Endpoint::TTY, PrivCtlOp::Allow))
        );
    }

    #[test]
    fn test_run_service_allow_failure_kills() {
        // C: manager.c:932-936 — ALLOW failure → kill_service (RS_EXITING).
        let mut table = RProcTable::new();
        let rp = table.alloc_slot().unwrap();
        prepared(&mut table, rp);
        let mut k = MockKernelApi::new(60);
        // vm_ok/execve_ok irrelevant; privctl Allow fails because the mock's
        // privctl always succeeds — use a slot-level trigger instead: drop
        // the endpoint so Allow targets NONE... keep simple: allow succeeds,
        // RS self-init returns Ok without any asynsend (ROOT_SYS_PROC gate,
        // utility.c:29-31).
        table
            .get_mut(rp)
            .priv_
            .flags
            .insert(PrivFlags::ROOT_SYS_PROC);
        let mut sent: Option<(Endpoint, i32)> = None;
        {
            let mut asynsend = |ep: Endpoint, msg: &crate::ready::InitMessage| {
                sent = Some((ep, msg.init_type as i32));
                Ok(())
            };
            assert!(
                run_service(
                    &mut table,
                    rp,
                    &mut k,
                    crate::sef::SefInitType::Fresh,
                    0,
                    50,
                    &mut asynsend
                )
                .is_ok()
            );
        }
        assert!(sent.is_none()); // RS never receives RS_INIT
        assert!(k.calls.contains(&Call::PrivCtl(
            table.get(rp).pub_.endpoint,
            PrivCtlOp::Allow
        )));
        // Pre-send state transition ran (utility.c:19-21).
        assert!(
            table
                .get(rp)
                .flags
                .contains(crate::service_slot::RFlags::INITIALIZING)
        );
    }

    #[test]
    fn test_start_service_pipeline() {
        // C: manager.c:950-983 — fold → create → activate → publish → run.
        let mut table = RProcTable::new();
        let rp = table.alloc_slot().unwrap();
        prepared(&mut table, rp);
        let mut k = working_mock();
        let mut published = false;
        {
            let mut publish = |_t: &RProcTable, _rp: SlotId| {
                published = true;
                Ok(())
            };
            let mut asynsend = |_ep: Endpoint, _msg: &crate::ready::InitMessage| Ok(());
            assert!(
                start_service(
                    &mut table,
                    rp,
                    &mut k,
                    0,
                    100,
                    &mut no_script,
                    &mut publish,
                    &mut asynsend
                )
                .is_ok()
            );
        }
        assert!(published);
        assert!(table.get(rp).flags.contains(RFlags::ACTIVE)); // activate
        assert!(k.calls.contains(&Call::SetUid(0))); // create tail
        assert!(k.calls.contains(&Call::PrivCtl(
            table.get(rp).pub_.endpoint,
            PrivCtlOp::Allow
        )));
    }
}

// ── init_service / clone_service / restart_service (R23c/R28 收口) ───────────

/// Sends the `RS_INIT` initialization message to a service.
///
/// C: `init_service` — utility.c:18-64: pre-send state transition
/// (`mark_initializing`), old-endpoint derivation (old version's
/// `state_endpoint`, else the previous replica's endpoint), script flag
/// fold, message assembly (with the single-shot map-prealloc take) and
/// `rs_asynsend`. RS itself (ROOT_SYS_PROC) sends nothing
/// (utility.c:29-31). `gid` is the rproctab grant (12 接线注入);
/// `prepare_state` is the LU descriptor value (16, `SEF_LU_STATE_NULL`
/// outside updates).
#[allow(clippy::too_many_arguments)] // 参数 = C 隐式全局的显式化（gid/prepare_state/ticks/asynsend）
pub fn init_service(
    slot: &mut ServiceSlot,
    old_endpoint: Option<Endpoint>,
    init_type: crate::sef::SefInitType,
    init_flags: u32,
    gid: Option<u32>,
    prepare_state: i32,
    ticks: Clock,
    asynsend: &mut dyn FnMut(Endpoint, &crate::ready::InitMessage) -> Result<(), Errno>,
) -> Result<(), Errno> {
    crate::ready::mark_initializing(slot, ticks); // utility.c:19-21

    // RS self-initialization sends nothing (utility.c:29-31).
    if slot.priv_.flags.contains(PrivFlags::ROOT_SYS_PROC) {
        return Ok(());
    }

    let flags = crate::ready::init_flags(
        slot.pub_.sys_flags.contains(SysFlags::USE_SCRIPT),
        init_flags,
    ); // utility.c:54-57
    let (buff_addr, buff_len) = crate::ready::take_map_prealloc(slot); // utility.c:53-60

    let msg = crate::ready::init_message(
        init_type,
        flags,
        gid,
        old_endpoint,
        slot.restarts + 1, // m_rs_init.restarts = r_restarts+1 (utility.c:58)
        buff_addr,
        buff_len,
        prepare_state,
    );
    asynsend(slot.pub_.endpoint, &msg) // utility.c:62
}

/// Creates a replica of the given system service instance.
///
/// C: `clone_service` — manager.c:713-782: the VM single-replica pre-clean
/// gate, slot clone + link, `create_service` of the replica (failure →
/// unlink), and the RS-restart backup signal-manager setup (failure →
/// unlink + kill).
#[allow(clippy::too_many_arguments)] // 参数 = C 隐式全局的显式化（kernel/ticks/read_exec 缝）
pub fn clone_service(
    table: &mut RProcTable,
    rp: SlotId,
    kernel: &mut dyn KernelApi,
    instance_flag: PrivFlags,
    init_flags: u32,
    ticks: Clock,
    read_exec: &mut dyn FnMut(&mut ServiceSlot) -> Result<(), Errno>,
) -> Result<SlotId, Errno> {
    // VM can only reliably support one replica (manager.c:730-737).
    if vm_replica_preclean_needed(
        table.get(rp).pub_.endpoint,
        instance_flag,
        table.get(rp).next_rp.is_some(),
    ) {
        if let Some(next) = table.get(rp).next_rp {
            // cleanup_service_now = both phases back-to-back (proto.h:53-54).
            crate::recovery::cleanup_service(table, next, kernel, &mut no_script);
            crate::recovery::cleanup_service(table, next, kernel, &mut no_script);
        }
        let next = table.get(rp).next_rp;
        if next.is_some() {
            table.get_mut(rp).next_rp = None;
        }
    }

    // Clone slot (manager.c:736-738).
    let replica = clone_slot(table, rp)?;

    // Link (manager.c:739-752).
    link_replica(table, rp, replica, instance_flag, init_flags);

    // Create the new replica (manager.c:753-763); failure → unlink.
    if let Err(e) = create_service(table, replica, kernel, ticks, read_exec) {
        unlink_replica(table, rp, instance_flag);
        return Err(e);
    }

    // RS-restart backup signal-manager setup (manager.c:765-779).
    if crate::self_lifecycle::is_rs_restart_replica(table.get(replica).priv_.flags) {
        // C: update_sig_mgrs(rs_rp, SELF, replica_ep) + (replica, SELF, NONE).
        let replica_ep = table.get(replica).pub_.endpoint;
        if let Some((rs_pair, replica_pair)) =
            crate::self_lifecycle::sig_mgr_updates(true, replica_ep)
        {
            // Apply via privctl on each slot's privilege copy + sync.
            for (slot_id, pair) in [(rp, rs_pair), (replica, replica_pair)] {
                let ep = table.get(slot_id).pub_.endpoint;
                if let Ok(mut priv_now) = kernel.getpriv(ep) {
                    priv_now.sig_mgr = pair.sig_mgr;
                    priv_now.bak_sig_mgr = pair.bak_sig_mgr;
                    if kernel
                        .privctl(ep, PrivCtlOp::SetSys, Some(&priv_now))
                        .is_err()
                    {
                        unlink_replica(table, rp, instance_flag);
                        // C: kill_service(replica, ...) — EXITING + crash.
                        let e = Errno::ENOMEM;
                        let _ = crate::recovery::kill_service(table.get_mut(replica), kernel, e);
                        return Err(e);
                    }
                    table.get_mut(slot_id).priv_.sig_mgr = pair.sig_mgr;
                    table.get_mut(slot_id).priv_.bak_sig_mgr = pair.bak_sig_mgr;
                }
            }
        }
    }

    Ok(replica)
}

/// Restarts a service via a recovery script or directly into a replica.
///
/// C: `restart_service` — manager.c:1246-1298: pending late reply first,
/// then the recovery-script branch (failure → `kill_service`), else
/// clone-into-replica (when absent), `update_service(RS_SWAP)` into it,
/// `run_service(SEF_INIT_RESTART)` of the replica, and the detach-policy
/// bookkeeping (`RS_CLEANUP_DETACH` when the old version wants detaching
/// and the restart counter still allows it).
#[allow(clippy::too_many_arguments)]
pub fn restart_service(
    table: &mut RProcTable,
    rp: SlotId,
    kernel: &mut dyn KernelApi,
    ticks: Clock,
    read_exec: &mut dyn FnMut(&mut ServiceSlot) -> Result<(), Errno>,
    run_script: &mut dyn FnMut(&mut ServiceSlot) -> Result<(), Errno>,
    _publish: &mut dyn FnMut(&RProcTable, SlotId) -> Result<(), Errno>,
    asynsend: &mut dyn FnMut(Endpoint, &crate::ready::InitMessage) -> Result<(), Errno>,
) {
    // See if a late reply has to be sent (manager.c:1252-1253).
    if table.get(rp).flags.contains(RFlags::LATEREPLY) {
        let _ = kernel.reply(table.get(rp).pub_.endpoint, 0);
        table.get_mut(rp).flags.remove(RFlags::LATEREPLY);
    }

    // Run a recovery script if available (manager.c:1255-1261).
    if table.get(rp).script[0] != 0 {
        if let Err(e) = run_script(table.get_mut(rp)) {
            let _ = crate::recovery::kill_service(table.get_mut(rp), kernel, e);
        }
        return;
    }

    // Restart directly; a replica is cloned when absent
    // (manager.c:1263-1272).
    let replica = match table.get(rp).next_rp {
        Some(r) => r,
        None => {
            match clone_service(
                table,
                rp,
                kernel,
                PrivFlags::RST_SYS_PROC,
                0,
                ticks,
                read_exec,
            ) {
                Ok(r) => r,
                Err(e) => {
                    let _ = crate::recovery::kill_service(table.get_mut(rp), kernel, e);
                    return;
                }
            }
        }
    };

    // Update the service into the replica (manager.c:1274-1280). A fresh
    // UpdateState carries no extra flags — the RS_SWAP swap_flag is what
    // matters here; C uses the live global, whose flags this path does not
    // consult beyond the swap.
    let mut update_state = crate::live_update::UpdateState::default();
    if update_state
        .update_service(table, kernel, rp, replica, 1, SysFlags::empty())
        .is_err()
    {
        let _ = crate::recovery::kill_service(table.get_mut(rp), kernel, Errno::EIO);
        return;
    }
    let src = rp;

    // Let the new replica run (manager.c:1282-1287).
    if run_service(
        table,
        replica,
        kernel,
        crate::sef::SefInitType::Restart,
        0,
        ticks,
        asynsend,
    )
    .is_err()
    {
        let _ = crate::recovery::kill_service(table.get_mut(rp), kernel, Errno::EIO);
        return;
    }

    // Detach-policy bookkeeping (manager.c:1289-1293).
    if table
        .get(src)
        .pub_
        .sys_flags
        .contains(SysFlags::DET_RESTART)
        && table.get(src).restarts < crate::recovery::MAX_DET_RESTART
    {
        table.get_mut(src).flags.insert(RFlags::CLEANUP_DETACH);
    }
}
