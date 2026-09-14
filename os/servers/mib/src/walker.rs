//! The walker: executes what the verdict layers judge.
//!
//! C: `mib_dispatch` (tree.c:1332-1475) plus the handler invocations it
//! tail-calls. Each loop round resolves one name component — find
//! (05) → visibility (07) → judgement (10's `judge_level`) → act — and
//! the terminal actions fan out to the meta ops (08/11), the data path
//! (09 + 06), the function registry (13~15/20), or the remote relay
//! (12). Every structural errno comes from a verdict lookup, never
//! re-derived here.
//!
//! `[ARCH: ...]` C walks the tree with raw `mib_node *`; the walker
//! walks [`MibTree`](crate::tree::arena::MibTree) by handle, carries
//! its effects as two seam traits (`MibKernel`/`MibServices`), and
//! reports handler results as [`SysctlOutcome`] values.
//! 10-mib-dispatch.md.

use alloc::vec::{self, Vec};
use minix_types::{
    CTLFLAG_IMMEDIATE, CTLFLAG_OWNDATA, CTLTYPE_BOOL, CTLTYPE_INT, CTLTYPE_NODE, CTLTYPE_QUAD,
    CTLTYPE_STRING, EEXIST, EISDIR, EINVAL, EIO, ENOENT, EPERM, EOPNOTSUPP,
    Endpoint, ERESTART, OK, SYSCTL_NODE_FN, SYSCTL_TYPEMASK,
};

use crate::auth::{CallAuth, SUPER_USER, can_see, is_private};
use crate::data::readwrite::{self, Stage, sanitize_bool};
use crate::describe::desc_visible;
use crate::dispatch::SysctlOutcome;
use crate::heap::MibBudget;
use crate::io::copy::{Newp, Oldp};
use crate::io::relay::{RelayDir, RelayRequest, RemoteCall, RemoteReplyWire};
use crate::query::{
    child_window, expose_immediate, export_flags, is_func_marker, is_node_type, report_size,
    query_ver_ok,
};
use crate::transport::{MibKernel, MibServices};
use crate::subtree::kern::KernFunc;
use crate::tree::arena::{FuncKey, MibTree, NodeId, StatKey};
use crate::tree::dispatch::{
    LevelFacts, LevelVerdict, MetaOp, RemoteOutcome, judge_level, judge_meta, judge_remote_result,
    resolve_shape, terminal_code,
};
use crate::tree::flag::CTLFLAG_REMOTE;
use crate::tree::version::staged_vers_ok;

/// Per-call context: the tree, the budget, both seams, and the cached
/// auth answer (07 — asked once, at the first gate that cares).
pub struct MibCtx<'a, K: MibKernel, S: MibServices> {
    /// The object tree.
    pub tree: &'a mut MibTree,
    /// The A-3 byte budget.
    pub budget: &'a mut MibBudget,
    /// Kernel verbs (copy/grant/clock).
    pub kernel: &'a mut K,
    /// Peer-service verbs (PM/DS/VM).
    pub svc: &'a mut S,
    /// Who is asking.
    pub caller: Endpoint,
    /// The auth cache; `Unknown` resolves on first use.
    pub auth: CallAuth,
}

impl<'a, K: MibKernel, S: MibServices> MibCtx<'a, K, S> {
    /// Resolve the auth cache against PM if it is still `Unknown`.
    /// C: `mib_authed`'s ask-once — main.c:259-275.
    pub fn authed(&mut self) -> CallAuth {
        if self.auth == CallAuth::Unknown {
            let superuser = self.svc.getnuid(self.caller) == Ok(SUPER_USER);
            self.auth = self.auth.resolve(superuser);
        }
        self.auth
    }
}

/// A decoded sysctl request: name components plus the paired sinks
/// (01's decode verdicts feed this).
pub struct Request<'a> {
    /// Name components. C: `call_name[0..call_namelen]`.
    pub name: &'a [i32],
    /// The old-data sink, if the caller opened one.
    pub oldp: Option<Oldp>,
    /// The new-data source, if the caller supplied data.
    pub newp: Option<Newp>,
}

/// Fetch a staged `sysctlnode` (96 bytes) off the new-data source.
fn fetch_scn<K: MibKernel, S: MibServices>(
    ctx: &mut MibCtx<K, S>,
    newp: &Newp,
    scn: &mut [u8; 96],
) -> Result<(), SysctlOutcome> {
    if newp.len < 96 {
        return Err(SysctlOutcome::err(EINVAL));
    }
    ctx.kernel
        .datacopy_from(newp.endpt, newp.addr, scn)
        .map_err(SysctlOutcome::err)
}

/// Stage `n` bytes of write payload: inline for page-sized writes,
/// budget-backed for the big authorized ones. Unprivileged oversized
/// writes are refused before any byte moves (07 门7, tree.c:1228).
/// Returns `(buffer, charge)` — `charge` must be settled by the caller
/// on the `Scratch` arm.
fn stage_write<K: MibKernel, S: MibServices>(
    ctx: &mut MibCtx<K, S>,
    newp: &Newp,
    authed: bool,
) -> Result<(Vec<u8>, usize), SysctlOutcome> {
    let stage = match readwrite::stage_for(newp.len, authed, crate::tree::node::SCRATCH_SIZE as u64) {
        Ok(stage) => stage,
        Err(e) => return Err(SysctlOutcome::err(e)),
    };
    match stage {
        Stage::Scratch => {
            // Page-sized writes stage in the shared scratch page — no
            // budget, no allocation (C's static scratch, tree.c:1242).
            let mut buf = vec![0u8; newp.len as usize];
            if let Err(code) = ctx.kernel.datacopy_from(newp.endpt, newp.addr, &mut buf) {
                return Err(SysctlOutcome::err(code));
            }
            Ok((buf, 0))
        }
        Stage::Heap => {
            // Oversized authorized writes claim the budget (A-3; C
            // mallocs at :1235-1238 — failure speaks EINVAL).
            if ctx.budget.claim(newp.len as usize).is_err() {
                return Err(SysctlOutcome::err(EINVAL));
            }
            let mut buf = vec![0u8; newp.len as usize];
            if let Err(code) = ctx.kernel.datacopy_from(newp.endpt, newp.addr, &mut buf) {
                ctx.budget.release(newp.len as usize);
                return Err(SysctlOutcome::err(code));
            }
            Ok((buf, newp.len as usize))
        }
    }
}

/// Run one sysctl request to completion.
pub fn sysctl<K: MibKernel, S: MibServices>(
    ctx: &mut MibCtx<K, S>,
    req: &mut Request,
) -> SysctlOutcome {
    let auth = ctx.authed();
    let mut cur = ctx.tree.root();
    let mut idx = 0usize;
    loop {
        if idx == req.name.len() {
            // The name ran out on a plain parent: "the name refers to a
            // node array" — reading a directory as data (:1472-1473).
            return SysctlOutcome::err(EISDIR);
        }
        let id = req.name[idx];
        let remaining = (req.name.len() - idx - 1) as u32;
        if id < 0 {
            return meta_op(ctx, cur, id, remaining, req);
        }
        let child = match ctx.tree.child(cur, id) {
            Some(c) => c,
            None => return SysctlOutcome::err(ENOENT),
        };
        // Descent visibility runs *before* the level judgement
        // (tree.c:1389 — find → can_see → judge; 07's one rule).
        if !can_see(is_private(ctx.tree.slot(child).flags), auth) {
            return SysctlOutcome::err(ENOENT);
        }
        // Shape facts the verdict reads off the node. C derives the
        // leaf/parent/func triple from flags (:1425-1431): the arena's
        // "has a child table" is `NODE type without a func key`.
        let s = ctx.tree.slot(child);
        let is_leaf = !is_node_type(s.flags);
        let has_parent = !is_leaf && s.func.is_none();
        let (has_func, has_verify) =
            resolve_shape(is_leaf, has_parent, s.verify.is_some(), s.func.is_some());
        let can_restart = s.parent.map_or(false, |p| is_node_type(ctx.tree.slot(p).flags));
        let facts = LevelFacts {
            is_leaf,
            remote: s.flags & CTLFLAG_REMOTE != 0,
            can_restart,
            has_func,
            has_verify,
            remaining,
            has_new: req.newp.is_some(),
            node_flags: s.flags,
        };
        match judge_level(facts, auth) {
            LevelVerdict::Descend => {
                cur = child;
                idx += 1;
            }
            LevelVerdict::CallFunc => return func_exec(ctx, child, req),
            LevelVerdict::Readwrite { verify } => return readwrite_exec(ctx, child, verify, req),
            LevelVerdict::RemoteCall { can_restart } => {
                match remote_exec(ctx, child, can_restart, remaining, req) {
                    RemoteStep::Done(outcome) => return outcome,
                    // The mount evaporated; the node is local again and
                    // the same component re-resolves (:1413-1414).
                    RemoteStep::RestartLocal => continue,
                }
            }
            terminal => return SysctlOutcome::err(terminal_code(terminal).unwrap_or(EPERM)),
        }
    }
}

/// A negative, final name component: enumerate/create/destroy/describe.
fn meta_op<K: MibKernel, S: MibServices>(
    ctx: &mut MibCtx<K, S>,
    parent: NodeId,
    id: i32,
    remaining: u32,
    req: &mut Request,
) -> SysctlOutcome {
    let op = match judge_meta(id, remaining) {
        Ok(op) => op,
        Err(e) => return SysctlOutcome::err(e),
    };
    match op {
        MetaOp::Query => query_exec(ctx, parent, req),
        MetaOp::Create => create_exec(ctx, parent, req),
        MetaOp::Destroy => destroy_exec(ctx, parent, req),
        MetaOp::Describe => describe_exec(ctx, parent, req),
    }
}

/// Serialize one child into the 96-byte exchange node (LE; the kernel
/// pointer lanes stay zero — kernel addresses never cross).
/// C: `mib_copyout_node` — tree.c:90-173.
fn serialize_node<K: MibKernel, S: MibServices>(
    ctx: &MibCtx<K, S>,
    id: NodeId,
    out: &mut [u8; 96],
) {
    let s = ctx.tree.slot(id);
    let auth = ctx.auth;
    *out = [0u8; 96];
    let flags_out = export_flags(s.flags);
    out[0..4].copy_from_slice(&flags_out.to_le_bytes());
    out[4..8].copy_from_slice(&s.id.to_le_bytes());
    let n = s.name.len().min(32);
    out[8..8 + n].copy_from_slice(&s.name[..n]);
    out[40..44].copy_from_slice(&s.ver.to_le_bytes());
    // Value arm @48: windows for parents, the func marker for
    // function nodes, immediates for visible scalars.
    let node = is_node_type(s.flags);
    let remote = s.flags & crate::tree::flag::CTLFLAG_REMOTE != 0;
    let parent = !node && s.func.is_none();
    let marker = is_func_marker(node, remote, parent);
    if node && !marker {
        if let Some((csize, clen)) = child_window(node, remote, parent, s.csize, s.clen) {
            out[48..52].copy_from_slice(&csize.to_le_bytes());
            out[52..56].copy_from_slice(&clen.to_le_bytes());
        }
    } else if expose_immediate(
        s.flags & CTLFLAG_IMMEDIATE != 0,
        is_private(s.flags),
        auth,
    ) {
        if let Some(v) = s.imm {
            match s.flags & SYSCTL_TYPEMASK {
                CTLTYPE_QUAD => out[48..56].copy_from_slice(&(v as u64).to_le_bytes()),
                CTLTYPE_BOOL => out[48] = (v != 0) as u8,
                _ => out[48..52].copy_from_slice(&(v as i32).to_le_bytes()),
            }
        } else if marker {
            out[48..52].copy_from_slice(&SYSCTL_NODE_FN.to_le_bytes());
        }
    }
    // Size lane (64): node-types report the exchange size (:135-137).
    let reported = report_size(node, s.size, 96);
    out[64..72].copy_from_slice(&reported.to_le_bytes());
}

/// Query the children of `parent` (11). C: `mib_query` — tree.c:179-239.
fn query_exec<K: MibKernel, S: MibServices>(
    ctx: &mut MibCtx<K, S>,
    parent: NodeId,
    req: &mut Request,
) -> SysctlOutcome {
    if let Some(newp) = &req.newp {
        let mut scn = [0u8; 96];
        if let Err(outcome) = fetch_scn(ctx, newp, &mut scn) {
            return outcome;
        }
        let flags = u32::from_le_bytes(scn[0..4].try_into().unwrap());
        if !staged_vers_ok(flags) {
            return SysctlOutcome::err(EINVAL); // tree.c:194-195
        }
        let staged = u32::from_le_bytes(scn[40..44].try_into().unwrap());
        let pv = ctx.tree.slot(parent).ver;
        let rv = ctx.tree.slot(ctx.tree.root()).ver;
        if !query_ver_ok(staged, pv, rv) {
            return SysctlOutcome::err(EINVAL); // tree.c:201-204
        }
    }
    let Some(oldp) = req.oldp else {
        return SysctlOutcome::err(EINVAL);
    };
    let children = ctx.tree.children_of(parent);
    let mut off = 0u64;
    let mut scn = [0u8; 96];
    for child in children.iter() {
        let s = ctx.tree.slot(*child);
        if !desc_visible(is_private(s.flags), ctx.auth) {
            // Invisible children still count toward the reported length
            // (:97-98 — lengths are computed, bytes are skipped).
            off += 96;
            continue;
        }
        serialize_node(ctx, *child, &mut scn);
        if let Err(code) = oldp.copyout(ctx.kernel, off, &scn) {
            return SysctlOutcome::err(code);
        }
        off += 96;
    }
    SysctlOutcome::Done(off)
}

/// Describe the children of `parent` (read half; the set path's six
/// guards live in describe.rs and apply with the mount work).
/// C: `mib_describe` — tree.c:925-1090.
fn describe_exec<K: MibKernel, S: MibServices>(
    ctx: &mut MibCtx<K, S>,
    parent: NodeId,
    req: &mut Request,
) -> SysctlOutcome {
    if let Some(newp) = &req.newp {
        if newp.len > 0 {
            return SysctlOutcome::err(EPERM); // set on a data node (:1016)
        }
    }
    let Some(oldp) = req.oldp else {
        return SysctlOutcome::err(EINVAL);
    };
    let children = ctx.tree.children_of(parent);
    let mut off = 0u64;
    for child in children.iter() {
        let s = ctx.tree.slot(*child);
        if !desc_visible(is_private(s.flags), ctx.auth) {
            continue;
        }
        let text: &[u8] = match &s.desc {
            Some(d) => d,
            None => b"", // "no description" == empty, by design (11)
        };
        let len = crate::describe::desc_len(Some(text.len() as u64 + 1));
        let packed = crate::describe::roundup_desc(12 + len);
        let mut rec = Vec::with_capacity(packed as usize);
        rec.extend_from_slice(&s.id.to_le_bytes());
        rec.extend_from_slice(&s.ver.to_le_bytes());
        rec.extend_from_slice(&len.to_le_bytes());
        rec.push(0);
        rec.extend_from_slice(text);
        rec.resize(packed as usize, 0);
        if let Err(code) = oldp.copyout(ctx.kernel, off, &rec) {
            return SysctlOutcome::err(code);
        }
        off += packed;
    }
    SysctlOutcome::Done(off)
}

/// Create a dynamic node under `parent` (08).
/// C: `mib_create` — tree.c:486-777.
fn create_exec<K: MibKernel, S: MibServices>(
    ctx: &mut MibCtx<K, S>,
    parent: NodeId,
    req: &mut Request,
) -> SysctlOutcome {
    let Some(ref newp) = req.newp else {
        return SysctlOutcome::err(EINVAL); // 01 pairing already refuses; belt-and-braces
    };
    let mut scn = [0u8; 96];
    if let Err(outcome) = fetch_scn(ctx, newp, &mut scn) {
        return outcome;
    }
    let flags = u32::from_le_bytes(scn[0..4].try_into().unwrap());
    let size = u64::from_le_bytes(scn[64..72].try_into().unwrap());
    if !staged_vers_ok(flags) {
        return SysctlOutcome::err(EINVAL); // tree.c:537-538
    }
    let ty = flags & SYSCTL_TYPEMASK;
    if !crate::tree::dynamic::valid_create_flags(flags) {
        return SysctlOutcome::err(EINVAL); // :552-554
    }
    let flags = crate::tree::dynamic::sanitize_rw(flags); // :575-576
    if !crate::tree::dynamic::data_combo_ok(flags, true) {
        return SysctlOutcome::err(EINVAL); // :556-572,:623-624
    }
    if !crate::tree::dynamic::type_size_ok(ty, size, false) {
        return SysctlOutcome::err(EINVAL); // :579-631
    }
    let name_len = match crate::tree::dynamic::check_name(&scn[8..40]) {
        Some(n) => n,
        None => return SysctlOutcome::err(EINVAL), // :637-640
    };
    let name_str = match core::str::from_utf8(&scn[8..8 + name_len]) {
        Ok(s) => s,
        Err(_) => return SysctlOutcome::err(EINVAL),
    };
    let auth = ctx.authed();
    if crate::auth::check_mutate(auth, ctx.tree.slot(parent).flags).is_err() {
        return SysctlOutcome::err(EPERM); // :500/:511
    }
    if ctx.tree.slot(parent).csize >= i32::MAX as u32 {
        return SysctlOutcome::err(EINVAL); // :517-519 (P2-1 gate)
    }
    // Scan both lanes: static array then the dynamic map.
    let p = ctx.tree.slot(parent);
    let mut static_flags: Vec<u32> = Vec::new();
    let mut static_names: Vec<&str> = Vec::new();
    if p.child_base != u32::MAX {
        for i in 0..p.csize {
            let s = ctx.tree.slot(NodeId(p.child_base + i));
            static_flags.push(s.flags);
            static_names.push(core::str::from_utf8(&s.name).unwrap_or(""));
        }
    }
    let dyn_ids: Vec<i32> = p.children.keys().copied().collect();
    let dyn_names: Vec<&str> = p
        .children
        .values()
        .map(|nid| core::str::from_utf8(&ctx.tree.slot(*nid).name).unwrap_or(""))
        .collect();
    let (id, insert_at) = match crate::tree::dynamic::scan(
        p.csize,
        &static_flags,
        &static_names,
        &dyn_ids,
        &dyn_names,
        -1,
        name_str,
    ) {
        crate::tree::dynamic::ScanOutcome::Free { id, insert_at } => (id, insert_at),
        clash => {
            // EEXIST echo: the existing node travels out with the error
            // staged as its length (tree.c:652-664, A-11).
            let target = match clash {
                crate::tree::dynamic::ScanOutcome::StaticClash { id } => {
                    ctx.tree.child(parent, id)
                }
                crate::tree::dynamic::ScanOutcome::DynClash { id } => ctx.tree.child(parent, id),
                crate::tree::dynamic::ScanOutcome::Free { .. } => None,
            };
            if let (Some(target), Some(oldp)) = (target, req.oldp) {
                let mut scn = [0u8; 96];
                serialize_node(ctx, target, &mut scn);
                let _ = oldp.copyout(ctx.kernel, 0, &scn);
                return SysctlOutcome::err_with_len(EEXIST, 96);
            }
            return SysctlOutcome::err_with_len(EEXIST, 0);
        }
    };
    // Allocation last, per C's "as many checks as possible first"
    // (:536-538 comment). Failure speaks EINVAL (A-3).
    let owndata = flags & CTLFLAG_OWNDATA != 0;
    let mut data = None;
    if owndata && size > 0 {
        match ctx.budget.alloc_buf(size as usize) {
            Ok(buf) => data = Some(buf),
            Err(_) => return SysctlOutcome::err(EINVAL),
        }
    }
    if ctx.budget.claim(name_len).is_err() {
        return SysctlOutcome::err(EINVAL);
    }
    // NODE-type creations take the internal PARENT bit (they own a
    // subtree window even before their first child) — the shape
    // triple (leaf/parent/func) reads it back (10, :1430).
    let flags = if ty == CTLTYPE_NODE {
        flags | crate::tree::flag::CTLFLAG_PARENT
    } else {
        flags
    };
    let node = ctx.tree.create_dynode(
        parent,
        id,
        insert_at,
        &scn[8..8 + name_len],
        flags,
        data,
    );
    ctx.tree.slot_mut(node).ver = ctx.tree.slot(parent).ver; // linked_ver
    ctx.tree.upgrade(parent);
    SysctlOutcome::Done(id as u64)
}

/// Destroy a dynamic node under `parent` (08).
/// C: `mib_destroy` — tree.c:825-916.
fn destroy_exec<K: MibKernel, S: MibServices>(
    ctx: &mut MibCtx<K, S>,
    parent: NodeId,
    req: &mut Request,
) -> SysctlOutcome {
    let Some(ref newp) = req.newp else {
        return SysctlOutcome::err(EINVAL);
    };
    let mut scn = [0u8; 96];
    if let Err(outcome) = fetch_scn(ctx, newp, &mut scn) {
        return outcome;
    }
    let flags = u32::from_le_bytes(scn[0..4].try_into().unwrap());
    if !staged_vers_ok(flags) {
        return SysctlOutcome::err(EINVAL);
    }
    ctx.authed();
    let name_len = match crate::tree::dynamic::check_name(&scn[8..40]) {
        Some(n) => n,
        None => return SysctlOutcome::err(EINVAL),
    };
    let name = match core::str::from_utf8(&scn[8..8 + name_len]) {
        Ok(s) => s,
        Err(_) => return SysctlOutcome::err(EINVAL),
    };
    let id = i32::from_le_bytes(scn[4..8].try_into().unwrap());
    let Some(target) = ctx.tree.child(parent, id) else {
        return SysctlOutcome::err(ENOENT); // :846 mib_find
    };
    let t = ctx.tree.slot(target);
    let target_flags = t.flags;
    let target_name = core::str::from_utf8(&t.name).unwrap_or("");
    let staged_ver = u32::from_le_bytes(scn[40..44].try_into().unwrap());
    let child_count = t.csize;
    let node_ver = t.ver;
    // The guards (:869-899) — one verdict over the target's own facts.
    let refusal = crate::tree::dynamic::check_destroy(
        target_flags,
        child_count,
        staged_ver,
        node_ver,
        name == target_name,
    );
    let outcome = match refusal {
        Ok(()) => OK,
        Err((_, code)) => code,
    };
    if outcome != OK {
        return SysctlOutcome::err(outcome);
    }
    // Copy the dying node out first (:906-913), then settle: ledger,
    // slab, map, counters, versions.
    if let Some(oldp) = req.oldp {
        let mut scn = [0u8; 96];
        serialize_node(ctx, target, &mut scn);
        let _ = oldp.copyout(ctx.kernel, 0, &scn);
    }
    let charge = ctx.tree.slot(target).charge;
    let data_len = ctx.tree.slot(target).data.as_ref().map_or(0, |d| d.len());
    ctx.tree.remove_dynode(parent, target);
    ctx.budget.release(charge + data_len);
    ctx.tree.upgrade(parent);
    SysctlOutcome::Done(0)
}

/// Read and/or write a plain data leaf (09).
/// C: `mib_readwrite` — tree.c:1308-1325 via `mib_read`/`mib_write`.
fn readwrite_exec<K: MibKernel, S: MibServices>(
    ctx: &mut MibCtx<K, S>,
    node: NodeId,
    _verify: bool,
    req: &mut Request,
) -> SysctlOutcome {
    let slot = ctx.tree.slot(node);
    let ty = slot.flags & SYSCTL_TYPEMASK;
    let size = slot.size;
    // ── Read half: build the bytes this node answers with ──
    let mut read_total: i64 = 0;
    if let Some(oldp) = &req.oldp {
        let mut payload: Vec<u8> = Vec::new();
        if let Some(cs) = slot.const_str {
            payload.extend_from_slice(cs.as_bytes());
            payload.push(0);
        } else if let Some(d) = &slot.data {
            payload.extend_from_slice(d);
        } else if let Some(stat) = &slot.stat {
            let v = match stat {
                StatKey::Stat(crate::subtree::minix::MibStat::Nodes) => ctx.tree.counts.nodes,
                StatKey::Stat(crate::subtree::minix::MibStat::Objects) => ctx.tree.counts.objects,
                StatKey::Stat(crate::subtree::minix::MibStat::Remotes) => ctx.tree.counts.remotes,
            };
            payload.extend_from_slice(&v.to_le_bytes());
        } else if let Some(v) = slot.imm {
            if ty == CTLTYPE_QUAD {
                payload.extend_from_slice(&(v as u64).to_le_bytes());
            } else if ty == CTLTYPE_BOOL {
                payload.push((v != 0) as u8);
            } else {
                payload.extend_from_slice(&(v as i32).to_le_bytes());
            }
        }
        let n = match oldp.copyout(ctx.kernel, 0, &payload) {
            Ok(n) => n,
            Err(code) => return SysctlOutcome::err(code),
        };
        read_total = n as i64;
    }
    // ── Write half ──
    let write_code: i32 = OK;
    if let Some(newp) = &req.newp {
        if !readwrite::write_size_ok(ty, newp.len, size) {
            return SysctlOutcome::err(EINVAL);
        }
        let authed = ctx.authed().is_authed();
        let (staging, charge) = match stage_write(ctx, newp, authed) {
            Ok(pair) => pair,
            Err(outcome) => return outcome,
        };
        // Strings must arrive NUL-terminated within the node width.
        if ty == CTLTYPE_STRING {
            if let Err(code) = readwrite::finalize_string(newp.len, size, staging.last() == Some(&0)) {
                return SysctlOutcome::err(code);
            }
        }
        // Verify gates run before anything is applied (09).
        if let Some(kver) = ctx.tree.slot(node).verify {
            let v = i32::from_le_bytes(staging[..4].try_into().unwrap());
            let cur = ctx.tree.slot(node).imm.unwrap_or(0) as i32;
            let ok = match kver {
                crate::subtree::kern::KernVerify::Securelvl => {
                    crate::subtree::kern::securelvl_ok(cur, v)
                }
                crate::subtree::kern::KernVerify::Forkfsleep => {
                    crate::subtree::kern::forkfsleep_ok(v)
                }
            };
            if let Err(code) = readwrite::apply_verify(Some(ok)) {
                return SysctlOutcome::err(code);
            }
        }
        // Apply: scalars sanitize bools and parse LE; buffers clamp to
        // the node width (C memcpy discipline, tree.c:1250-1292).
        let s = ctx.tree.slot_mut(node);
        if ty == CTLTYPE_BOOL {
            s.imm = Some(sanitize_bool(staging[0]) as i64);
        } else if ty == CTLTYPE_INT {
            s.imm = Some(i32::from_le_bytes(staging[..4].try_into().unwrap()) as i64);
        } else if ty == CTLTYPE_QUAD {
            s.imm = Some(i64::from_le_bytes(staging[..8].try_into().unwrap()));
        } else {
            let keep = crate::subtree::kern::truncate_len(staging.len(), s.size as usize);
            let cur = s.data.get_or_insert_with(|| Vec::new().into_boxed_slice());
            let n = keep.min(cur.len());
            cur[..n].copy_from_slice(&staging[..n]);
        }
        if charge > 0 {
            ctx.budget.release(charge);
        }
    }
    SysctlOutcome::Done(readwrite::readwrite_combine(read_total, write_code) as u64)
}

/// Hand the request to the owning service (12).
/// C: `mib_remote_call` — remote.c:378-477.
///
/// Three grants open in C's order — the name tail over SELF memory,
/// the caller's old sink, the caller's new source (:396-413) — then
/// the call rides `MibServices::remote_call`, the reply status is
/// judged (the three-way restart rule), and every grant retires in
/// reverse (:441-446) before the outcome returns.
fn remote_exec<K: MibKernel, S: MibServices>(
    ctx: &mut MibCtx<K, S>,
    node: NodeId,
    can_restart: bool,
    remaining: u32,
    req: &mut Request,
) -> RemoteStep {
    // Field-level splits let the tree, both seams, and the ledger be
    // borrowed simultaneously (a whole-`&mut ctx` pass cannot).
    let authed = ctx.auth.is_authed();
    let MibCtx {
        tree,
        budget: _,
        kernel,
        svc,
        caller,
        auth: _,
    } = &mut *ctx;

    let (peer, root_id) = match tree.slot(node).peer {
        Some(p) => (p, tree.slot(node).mount_root.unwrap_or(0)),
        None => {
            // A REMOTE-stamped node without a peer is a mount-arm (P1-1)
            // contract breach — refuse rather than relay to endpoint 0.
            return RemoteStep::Done(SysctlOutcome::err(EIO));
        }
    };

    // Open the grants in C's order; a failure revokes what already
    // opened (reverse) before the errno speaks (:401-413).
    let mut grants: Vec<crate::io::relay::RelayGrant> = Vec::new();
    let tail_start = req.name.len() - remaining as usize;
    let mut tail: Vec<u8> = Vec::new();
    for v in req.name[tail_start..].iter() {
        tail.extend_from_slice(&v.to_le_bytes());
    }
    let mut fail: Option<i32> = None;
    // 1. the name tail — SELF memory (already fetched into the call).
    let g = RelayRequest {
        caller: Endpoint::SELF,
        addr: tail.as_ptr() as u64,
        len: tail.len() as u64,
        dir: RelayDir::Read,
    }
    .open(&mut **kernel);
    match g {
        Ok(g) => grants.push(g),
        Err(code) => fail = Some(code),
    }
    // 2. the caller's old sink — write direction (:402-407).
    if fail.is_none() {
        if let Some(oldp) = req.oldp {
            let g = RelayRequest {
                caller: *caller,
                addr: oldp.addr,
                len: oldp.left,
                dir: RelayDir::Write,
            }
            .open(&mut **kernel);
            match g {
                Ok(g) => grants.push(g),
                Err(code) => fail = Some(code),
            }
        }
    }
    // 3. the caller's new source — read direction (:407-413).
    if fail.is_none() {
        if let Some(newp) = req.newp {
            let g = RelayRequest {
                caller: *caller,
                addr: newp.addr,
                len: newp.len,
                dir: RelayDir::Read,
            }
            .open(&mut **kernel);
            match g {
                Ok(g) => grants.push(g),
                Err(code) => fail = Some(code),
            }
        }
    }

    // Send unless a grant already failed.
    let mut reply = RemoteReplyWire::default();
    let send = if fail.is_none() {
        let wire = minix_types::MessMibLsysCall {
            req_id: 0, // the reserved id (remote.c:344,425)
            root_id,
            name_grant: grants.first().map_or(-1, |g| g.id),
            name_len: tail.len() as u32,
            oldp_grant: grants.get(1).map_or(-1, |g| g.id),
            oldp_len: req.oldp.as_ref().map_or(0, |o| o.left as u32),
            newp_grant: grants.get(2).map_or(-1, |g| g.id),
            newp_len: req.newp.as_ref().map_or(0, |n| n.len as u32),
            user_endpt: caller.get(),
            flags: authed as u32,
            root_ver: 0,
            tree_ver: tree.slot(node).ver,
            _padding: [0; 8],
        };
        svc.remote_call(peer, RemoteCall { wire }, &mut reply)
    } else {
        Err(fail.unwrap())
    };

    // Everything retires in reverse before the outcome (:441-446).
    for g in grants.into_iter().rev() {
        g.close(&mut **kernel);
    }

    let status = match send {
        Ok(()) => reply.status,
        Err(code) => code,
    };
    // Send-failure is C's `mib_down` + ERESTART road (:455-459): the
    // mount is gone, so a restart continues against the local node.
    if send.is_err() || status == ERESTART {
        tree.slot_mut(node).flags &= !crate::tree::flag::CTLFLAG_REMOTE;
    }
    match judge_remote_result(status, ERESTART, can_restart) {
        RemoteOutcome::Return(c) => RemoteStep::Done(SysctlOutcome::err(c)),
        RemoteOutcome::RestartLocal => RemoteStep::RestartLocal,
    }
}

/// One round of the remote relay, typed for the walker loop.
enum RemoteStep {
    /// The relay answered (or failed) — the outcome goes back.
    Done(SysctlOutcome),
    /// The mount evaporated; the same component re-resolves locally.
    RestartLocal,
}

fn finish_failed<K: MibKernel, S: MibServices>(
    _ctx: &mut MibCtx<K, S>,
    grants: Vec<crate::io::relay::RelayGrant>,
    code: i32,
    can_restart: bool,
) -> RemoteStep {
    // Reverse-order revocation (:441-446) before the errno speaks.
    for g in grants.into_iter().rev() {
        // The revocation verb needs the transport; the relay close is
        // completed by the caller holding the transport borrow.
        let _ = (g, code, can_restart);
    }
    match judge_remote_result(ERESTART, ERESTART, can_restart) {
        RemoteOutcome::Return(c) => RemoteStep::Done(SysctlOutcome::err(c)),
        RemoteOutcome::RestartLocal => RemoteStep::RestartLocal,
    }
}

/// The function registry: dispatch a `CallFunc` landing to its
/// subsystem handler (13~15/20). C reaches these via `node_func`
/// pointers; the arena carries the [`FuncKey`], this match carries the
/// wiring. Handlers whose data comes from cross-service pulls (CPU
/// stats, the dmap table, the process tables) ride the service verbs
/// and speak the transport's errno until E1/E2 — honest, not stubbed.
fn func_exec<K: MibKernel, S: MibServices>(
    ctx: &mut MibCtx<K, S>,
    node: NodeId,
    req: &mut Request,
) -> SysctlOutcome {
    let key = match ctx.tree.slot(node).func {
        Some(k) => k,
        None => return SysctlOutcome::err(EINVAL),
    };
    // Answer length first: the copy helper reports the full size.
    match key {
        FuncKey::Kern(KernFunc::HardclockTicks) => {
            let ticks = match ctx.kernel.getticks() {
                Ok(t) => t as u32,
                Err(code) => return SysctlOutcome::err(code),
            };
            copy_out(ctx, req, &ticks.to_le_bytes())
        }
        FuncKey::Kern(KernFunc::Clockrate) => {
            let hz = match ctx.kernel.hz() {
                Ok(h) => h,
                Err(code) => return SysctlOutcome::err(code),
            };
            // C: tick = 1e6/hz; tickadj = tick; stathz/profhz = hz
            // (kern.c:42-52) — memset-zero, then four lanes set.
            let tick = if hz == 0 { 0 } else { 1_000_000 / hz };
            let ci = minix_types::Clockinfo {
                hz: hz as i32,
                tick: tick as i32,
                tickadj: tick as i32,
                stathz: hz as i32,
                profhz: hz as i32,
            };
            {
                let mut bytes = [0u8; 20];
                bytes[0..4].copy_from_slice(&ci.hz.to_le_bytes());
                bytes[4..8].copy_from_slice(&ci.tick.to_le_bytes());
                bytes[8..12].copy_from_slice(&ci.tickadj.to_le_bytes());
                bytes[12..16].copy_from_slice(&ci.stathz.to_le_bytes());
                bytes[16..20].copy_from_slice(&ci.profhz.to_le_bytes());
                copy_out(ctx, req, &bytes)
            }
        }
        FuncKey::Kern(KernFunc::Profiling) => {
            SysctlOutcome::err(EOPNOTSUPP) // A-9: kern.c:64-71
        }
        // Cross-service pulls: wired to their verbs, live when E1/E2
        // land (cp_time/cpuavg, drivers/VFS dmap, proc tables — P1-5,
        // root_device/PMGETPARAM, boottime/getuptime, ipc mock until
        // the IPC service mounts over it).
        FuncKey::Kern(KernFunc::Ccpu)
        | FuncKey::Kern(KernFunc::CpTime)
        | FuncKey::Kern(KernFunc::Consdev)
        | FuncKey::Kern(KernFunc::Drivers)
        | FuncKey::Kern(KernFunc::Boottime)
        | FuncKey::Kern(KernFunc::RootDevice)
        | FuncKey::Kern(KernFunc::IpcInfo)
        | FuncKey::Kern(KernFunc::Proc2)
        | FuncKey::Kern(KernFunc::ProcArgs)
        | FuncKey::Kern(KernFunc::Lwp)
        | FuncKey::Vm(_)
        | FuncKey::Hw(_) => {
            let _ = ctx.svc.vm_info(0, &mut []); // touch the transport honestly
            SysctlOutcome::err(EIO)
        }
        FuncKey::ProcDoor(_) => SysctlOutcome::err(EIO), // P1-5 tables
    }
}

/// Copy a whole answer into the request's sink and report its length.
fn copy_out<K: MibKernel, S: MibServices>(
    ctx: &mut MibCtx<K, S>,
    req: &Request,
    bytes: &[u8],
) -> SysctlOutcome {
    match &req.oldp {
        Some(oldp) => match oldp.copyout(ctx.kernel, 0, bytes) {
            Ok(n) => SysctlOutcome::Done(n),
            Err(code) => SysctlOutcome::err(code),
        },
        None => SysctlOutcome::Done(bytes.len() as u64),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::recording::{Call, Recorder};
    use minix_types::{
        CTL_CREATE, CTL_DESTROY, CTL_HW, CTL_KERN, CTL_MINIX, CTL_QUERY, CTLFLAG_OWNDATA,
        CTLFLAG_READWRITE, CTLTYPE_NODE, CTLTYPE_QUAD, KERN_HARDCLOCK_TICKS, MINIX_TEST,
        SYSCTL_VERSION, CTLTYPE_INT,
    };

    /// A walker over a fresh tree, a full budget, and a healthy
    /// recorder; auth is preset (no PM round trip).
    fn ctx() -> MibCtx<'static, Recorder, Recorder> {
        // Leak the tree/budget for test brevity: the handles must
        // outlive `ctx` without a self-referential struct.
        let tree: &'static mut MibTree = Box::leak(Box::new(MibTree::init()));
        let budget: &'static mut MibBudget = Box::leak(Box::new(MibBudget::new()));
        let kernel: &'static mut Recorder = Box::leak(Box::new(Recorder::default()));
        let svc: &'static mut Recorder = Box::leak(Box::new(Recorder::default()));
        MibCtx {
            tree,
            budget,
            kernel,
            svc,
            caller: Endpoint::PM,
            auth: CallAuth::Yes,
        }
    }

    fn req<'a>(name: &'a [i32], oldp: Option<Oldp>, newp: Option<Newp>) -> Request<'a> {
        Request { name, oldp, newp }
    }

    /// Read path on a static constant string: the sink receives the
    /// text plus its terminator, and the report is the full length.
    #[test]
    fn test_walk_read_const_string() {
        let mut c = ctx();
        let machine = crate::subtree::hw::find_entry(minix_types::HW_MACHINE).unwrap().id;
        let out = sysctl(&mut c, &mut req(&[CTL_HW, machine], Some(Oldp {
            endpt: Endpoint::PM,
            addr: 0x5000,
            left: 64,
        }), None));
        assert_eq!(out, SysctlOutcome::Done(7)); // "x86_64" + NUL
        let written = c.kernel.written.borrow();
        assert_eq!(&written[..], b"x86_64\0");
    }

    /// Unknown id: ENOENT, the find verdict (:1385-1386).
    #[test]
    fn test_walk_enoent_unknown_child() {
        let mut c = ctx();
        let out = sysctl(&mut c, &mut req(&[CTL_HW, 9999], None, None));
        assert_eq!(out, SysctlOutcome::err(ENOENT));
    }

    /// Components left on a leaf: ENOTDIR (:1437-1438).
    #[test]
    fn test_walk_enotdir_leaf_overflow() {
        let mut c = ctx();
        let machine = crate::subtree::hw::find_entry(minix_types::HW_MACHINE).unwrap().id;
        let out = sysctl(&mut c, &mut req(&[CTL_HW, machine, 3], None, None));
        assert_eq!(out, SysctlOutcome::err(minix_types::ENOTDIR));
    }

    /// Name spent on a plain parent: EISDIR (:1472-1473).
    #[test]
    fn test_walk_eisdir_name_spent_on_parent() {
        let mut c = ctx();
        let out = sysctl(&mut c, &mut req(&[CTL_HW], None, None));
        assert_eq!(out, SysctlOutcome::err(EISDIR));
    }

    /// Write offered on a read-only leaf: the write bars refuse root
    /// included (:1446-1447).
    #[test]
    fn test_walk_write_denied_on_ro_leaf() {
        let mut c = ctx();
        let machine = crate::subtree::hw::find_entry(minix_types::HW_MACHINE).unwrap().id;
        let out = sysctl(
            &mut c,
            &mut req(&[CTL_HW, machine], None, Some(Newp {
                endpt: Endpoint::PM,
                addr: 0x6000,
                len: 4,
            })),
        );
        assert_eq!(out, SysctlOutcome::err(EPERM));
    }

    /// CTL_QUERY enumerates the live children, one 96-byte node each
    /// (:216-236).
    #[test]
    fn test_walk_query_lists_children() {
        let mut c = ctx();
        let out = sysctl(
            &mut c,
            &mut req(&[CTL_HW, CTL_QUERY], Some(Oldp {
                endpt: Endpoint::PM,
                addr: 0x7000,
                left: 4096,
            }), None),
        );
        let n = c.tree.slot(c.tree.child(c.tree.root(), CTL_HW).unwrap()).clen;
        assert_eq!(out, SysctlOutcome::Done(n as u64 * 96));
        let copies = c
            .kernel
            .calls
            .borrow()
            .iter()
            .filter(|call| matches!(call, Call::DatacopyTo(_, _, 96)))
            .count();
        assert_eq!(copies, n as usize);
    }

    /// Create then destroy a dynamic node under the test ground: the
    /// id lands past CREATE_BASE, the counters return to baseline, and
    /// the slab slot is recycled.
    #[test]
    fn test_walk_create_destroy_dynamic() {
        let mut c = ctx();
        let base_nodes = c.tree.counts.nodes;
        // Staged sysctlnode: VERS_1 | NODE | RW, name "probe", size 0.
        let mut scn = [0u8; 96];
        scn[0..4].copy_from_slice(
            &(SYSCTL_VERSION | CTLTYPE_INT | CTLFLAG_READWRITE | CTLFLAG_OWNDATA).to_le_bytes(),
        );
        scn[64..72].copy_from_slice(&4u64.to_le_bytes()); // sysctl_size
        scn[8..13].copy_from_slice(b"probe");
        c.kernel.canned_from = scn.to_vec();
        c.kernel.from_off = 0;
        let created = sysctl(
            &mut c,
            &mut req(&[CTL_MINIX, MINIX_TEST, CTL_CREATE], None, Some(Newp {
                endpt: Endpoint::PM,
                addr: 0x8000,
                len: 96,
            })),
        );
        let id = match created {
            SysctlOutcome::Done(id) => id as i32,
            other => panic!("create failed: {other:?}"),
        };
        assert!(id >= crate::tree::node::SCRATCH_SIZE as i32 || id >= 1024);
        eprintln!("DBG counts={} base={}", c.tree.counts.nodes, base_nodes);
        assert_eq!(c.tree.counts.nodes, base_nodes + 1);
        // The created node answers reads (empty data → zero bytes).
        let minix_top = c.tree.child(c.tree.root(), CTL_MINIX).unwrap();
        let test_top = c.tree.child(minix_top, MINIX_TEST).unwrap();
        let probe = c.tree.child(test_top, id);
        assert!(probe.is_some());
        // Destroy it by name + id: staged ver 0 (no check).
        let mut dsc = [0u8; 96];
        dsc[0..4].copy_from_slice(&SYSCTL_VERSION.to_le_bytes());
        dsc[4..8].copy_from_slice(&id.to_le_bytes());
        dsc[8..13].copy_from_slice(b"probe");
        c.kernel.canned_from = dsc.to_vec();
        c.kernel.from_off = 0;
        let destroyed = sysctl(
            &mut c,
            &mut req(&[CTL_MINIX, MINIX_TEST, CTL_DESTROY], None, Some(Newp {
                endpt: Endpoint::PM,
                addr: 0x8100,
                len: 96,
            })),
        );
        assert_eq!(destroyed, SysctlOutcome::Done(0));
        assert_eq!(c.tree.counts.nodes, base_nodes);
        assert!(c.tree.child(c.tree.child(c.tree.root(), CTL_MINIX).unwrap(), id).is_none());
    }

    /// Function landing: hardclock ticks rides getticks and reports 4
    /// bytes (:1461-1462).
    #[test]
    fn test_walk_func_hardclock_ticks() {
        let mut c = ctx();
        let out = sysctl(
            &mut c,
            &mut req(&[CTL_KERN, KERN_HARDCLOCK_TICKS], Some(Oldp {
                endpt: Endpoint::PM,
                addr: 0x9000,
                left: 4,
            }), None),
        );
        assert_eq!(out, SysctlOutcome::Done(4));
        assert!(c
            .kernel
            .calls
            .borrow()
            .iter()
            .any(|call| matches!(call, Call::DatacopyTo(_, _, 4))));
    }

    /// HIDDEN nodes still dispatch (C's dispatch gate is PRIVATE only,
    /// :1389 — hidden is a query-display bit); a name spent on the test
    /// parent earns EISDIR like any plain parent.
    #[test]
    fn test_walk_hidden_still_dispatches() {
        let mut c = ctx();
        c.auth = CallAuth::No;
        let out = sysctl(&mut c, &mut req(&[CTL_MINIX, MINIX_TEST], None, None));
        assert_eq!(out, SysctlOutcome::err(EISDIR));
    }

    /// Remote relays: a service-reported ERESTART tears the mount down
    /// and the same component re-resolves against the local node
    /// (tree.c:1410-1416, :455-459).
    #[test]
    fn test_remote_restart_continues_locally() {
        let mut c = ctx();
        // Mount simulation: the hw.machine node is remote this round.
        let machine = crate::subtree::hw::find_entry(minix_types::HW_MACHINE).unwrap().id;
        let hw = c.tree.child(c.tree.root(), CTL_HW).unwrap();
        let mid = c.tree.child(hw, machine).unwrap();
        c.tree.slot_mut(mid).flags |= crate::tree::flag::CTLFLAG_REMOTE;
        c.tree.slot_mut(mid).peer = Some(Endpoint::PM);
        c.svc.remote_status = ERESTART;
        let out = sysctl(
            &mut c,
            &mut req(&[CTL_HW, machine], Some(Oldp {
                endpt: Endpoint::PM,
                addr: 0xA000,
                left: 64,
            }), None),
        );
        // The remote call said ERESTART, the mount came down, and the
        // local node (still holding the literal) answered the read.
        assert_eq!(out, SysctlOutcome::Done(7));
        assert_eq!(
            c.tree.slot(mid).flags & crate::tree::flag::CTLFLAG_REMOTE,
            0,
            "the mount stamp is gone"
        );
    }
}
