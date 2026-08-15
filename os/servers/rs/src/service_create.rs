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

use crate::privilege::PrivFlags;
use crate::process_table::RProcTable;
use crate::service_slot::{RFlags, ServiceSlot, SlotId, SysFlags};
use minix_types::{Clock, EPERM, ERESTART, Endpoint, Pid};

/// Checks `create_service`'s dependency preconditions.
///
/// C: `create_service` — manager.c:542-568. Returns `EPERM` on the first
/// unmet dependency (the C code logs then `free_slot`s; the caller owns the
/// slot cleanup).
///
/// `has_replica` (manager.c:543-546): an old version (`r_old_rp`) counts; a
/// previous replica (`r_prev_rp`) counts only when it is not `RS_TERMINATED`
/// — a dying replica is not a usable one.
pub fn check_create_preconditions(table: &RProcTable, rp: SlotId) -> Result<(), i32> {
    let slot = table.get(rp);
    let use_copy = slot.pub_.sys_flags.contains(SysFlags::USE_COPY);
    let has_replica = slot.old_rp.is_some()
        || slot
            .prev_rp
            .is_some_and(|p| !table.get(p).flags.contains(RFlags::TERMINATED));
    if !has_replica && slot.pub_.sys_flags.contains(SysFlags::NEED_REPL) {
        return Err(EPERM); // manager.c:547-552
    }
    if !use_copy && slot.pub_.sys_flags.contains(SysFlags::NEED_COPY) {
        return Err(EPERM); // manager.c:555-560
    }
    if !use_copy && slot.cmd[0] == 0 {
        return Err(EPERM); // manager.c:563-568: strcmp(r_cmd, "") == 0
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
    slot.argc = tokens.len() as i32;
    let mut off = 0usize;
    for token in &tokens {
        // Label::from_bytes truncates at RS_MAX_LABEL_LEN and NUL-pads; the
        // logical length is the first NUL (service_slot.rs:202-208).
        let raw = token.as_bytes();
        let len = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
        let bytes = &raw[..len];
        let remaining = slot.args.len() - off;
        let n = bytes.len().min(remaining.saturating_sub(1));
        slot.args[off..off + n].copy_from_slice(&bytes[..n]);
        off += n;
        if off < slot.args.len() {
            slot.args[off] = 0; // NUL terminator
            off += 1;
        }
    }
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
pub fn clone_slot(table: &mut RProcTable, src: SlotId) -> Result<SlotId, i32> {
    let clone = table.alloc_slot()?; // manager.c:1809
    let mut c = table.get(src).clone(); // manager.c:1824-1825
    c.init_err = ERESTART; // manager.c:1828 (sys/errno.h:196, positive per minix-types)
    c.flags &= !RFlags::ACTIVE; // manager.c:1829
    c.pid = None; // manager.c:1830
    c.pub_.endpoint = Endpoint::NONE; // manager.c:1831
    rebuild_args(&mut c); // manager.c:1833
    if c.pub_.sys_flags.contains(SysFlags::USE_COPY) {
        // manager.c:1834-1835: share_exec(clone_rp, rp) — Arc already shared
        // by the clone above; explicit for faithfulness.
        c.exec = table.get(src).exec.clone();
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
        assert_eq!(check_create_preconditions(&t, rp), Err(EPERM));
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
        assert_eq!(check_create_preconditions(&t, rp), Err(EPERM));
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
        assert_eq!(check_create_preconditions(&t, rp), Err(EPERM));
    }

    #[test]
    fn test_preconditions_empty_cmd() {
        // C: manager.c:563-568 — no copy and empty command.
        let mut t = RProcTable::new();
        let rp = t.alloc_slot().unwrap();
        t.get_mut(rp).flags |= RFlags::IN_USE;
        assert_eq!(check_create_preconditions(&t, rp), Err(EPERM));
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
}
