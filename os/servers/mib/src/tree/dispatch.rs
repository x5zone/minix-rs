//! Name resolution dispatch: one verdict per tree level.
//!
//! Mirrors `mib_dispatch` (`tree.c:1332-1474`) minus the effects (child
//! fetch, remote calls, handler invocation, reads/writes). Each loop
//! iteration consumes one name component and judges exactly one thing;
//! the walker (10's follow-up with the arena) executes the verdicts.
//!
//! 10-mib-dispatch.md.

use minix_types::{
    CTL_CREATE, CTL_DESCRIBE, CTL_DESTROY, CTL_QUERY, CTLTYPE_NODE, EINVAL, EISDIR, ENOENT,
    ENOTDIR, EOPNOTSUPP, SYSCTL_TYPEMASK,
};

use super::flag::CTLFLAG_REMOTE;

use super::super::auth::{CallAuth, check_write};

/// Meta-operation: a negative name component that ends resolution.
///
/// C: `switch (id)` — tree.c:1368-1381. Negative ids are never regular
/// children (:1354-1359 — handler subpaths like PROC2 excepted, 18);
/// a meta-id must be the *last* component (`:1362-1366`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetaOp {
    /// Enumerate children. C: `CTL_QUERY` → `mib_query` (11).
    Query,
    /// Create a node. C: `CTL_CREATE` → `mib_create` (08).
    Create,
    /// Destroy a node. C: `CTL_DESTROY` → `mib_destroy` (08).
    Destroy,
    /// Fetch descriptions. C: `CTL_DESCRIBE` → `mib_describe` (11).
    Describe,
}

/// Judge a negative name component.
///
/// `remaining` counts components *after* this one. Trailing garbage
/// after a meta-id refuses (`EINVAL`, :1365-1366 — the op consumes the
/// rest of the name, so leftovers name nothing); `CREATESYM`/`MMAP`/any
/// other negative speaks `EOPNOTSUPP` (:1377-1380, A-9).
pub const fn judge_meta(id: i32, remaining: u32) -> Result<MetaOp, i32> {
    if remaining > 0 {
        return Err(EINVAL);
    }
    match id {
        CTL_QUERY => Ok(MetaOp::Query),
        CTL_CREATE => Ok(MetaOp::Create),
        CTL_DESTROY => Ok(MetaOp::Destroy),
        CTL_DESCRIBE => Ok(MetaOp::Describe),
        _ => Err(EOPNOTSUPP),
    }
}

/// What one resolved child asks the walker to do.
///
/// C: one loop iteration minus the descent-visibility check —
/// tree.c:1384-1470. Visibility (`:1389`, 07's `can_see`) runs *before*
/// this, in the walker: find → visible? → judge. Ordered exactly as C
/// judges the rest: remote (:1404), leaf/function shape (:1425-1431),
/// name-exhaustion (:1437), write bars (:1446-1458), terminal action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LevelVerdict {
    /// No such child. C: `:1385-1386` → `ENOENT`.
    NotFound,
    /// Remote node: relay, remembering whether local restart is allowed.
    /// C: `:1404-1416` (`can_restart = PARENT` read *before* the call —
    /// the node may be gone when it returns, :1405-1406).
    RemoteCall {
        /// Local subtree survives unmount (node is `PARENT`).
        can_restart: bool,
    },
    /// Leaf with name left over. C: `:1437-1438` → `ENOTDIR`.
    LeafOverflow,
    /// Write refused at the landing. C: `:1446-1458` → `EPERM`.
    WriteDenied,
    /// Handler owns it from here. C: `:1461-1462` → `node_func(...)`.
    CallFunc,
    /// Plain data leaf: generic read/write. C: `:1465-1467`.
    Readwrite {
        /// Verify callback present. C: `has_verify ? verify : NULL`.
        verify: bool,
    },
    /// Plain parent, name remains: descend. C: `:1469-1470` loop.
    Descend,
}

/// Judge one resolved child.
///
/// Inputs are precomputed shape facts (the walker reads them off the
/// node; `has_func_ptr` is "func/verify pointer set" as appropriate).
/// Write bars reuse 07's [`check_write`] — same two bars, one
/// implementation (`:1446-1458` ≡ 07's contract).
#[allow(clippy::too_many_arguments)]
pub const fn judge_level(
    is_leaf: bool,
    remote: bool,
    can_restart: bool,
    has_func: bool,
    has_verify: bool,
    remaining: u32,
    has_new: bool,
    node_flags: u32,
    auth: CallAuth,
) -> LevelVerdict {
    if remote {
        return LevelVerdict::RemoteCall { can_restart };
    }
    if is_leaf && remaining > 0 {
        return LevelVerdict::LeafOverflow;
    }
    if (is_leaf || has_func) && has_new && check_write(node_flags, true, auth).is_err() {
        return LevelVerdict::WriteDenied;
    }
    if has_func {
        return LevelVerdict::CallFunc;
    }
    if is_leaf {
        return LevelVerdict::Readwrite { verify: has_verify };
    }
    LevelVerdict::Descend
}

/// Compute the leaf/function shape facts C derives per level.
///
/// C: tree.c:1425-1431. Leaves read `VERIFY`/func-pointer (`:1426-1427`
/// — a leaf with `VERIFY` never *also* takes the func path); non-leaves
/// read `PARENT` (`:1430` — function subtrees have no `PARENT`, so the
/// func test doubles as the parent test, saving bytes per node, :1421).
/// Returns `(has_func, has_verify)`.
pub const fn resolve_shape(
    is_leaf: bool,
    has_parent: bool,
    has_verify_bit: bool,
    func_set: bool,
) -> (bool, bool) {
    if is_leaf {
        let verify = has_verify_bit;
        (func_set && !verify, verify)
    } else {
        (!has_parent, false)
    }
}

/// Outcome of a relayed remote call.
///
/// C: tree.c:1410-1416. Anything but `ERESTART` returns as-is (success
/// included); `ERESTART` without restart rights is a dead end (`ENOENT`
/// — the mount evaporated with no local fallback); with rights, the
/// walker continues locally (the node is local again by then, :1414).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteOutcome {
    /// Return this code to the caller. C: `:1411` first arm.
    Return(i32),
    /// Mount gone but local subtree survives: keep resolving locally.
    RestartLocal,
}

/// Judge a remote call's result.
///
/// C: tree.c:1410-1416. Anything but the restart sentinel returns as-is
/// (success included); the sentinel without restart rights is a dead end
/// (`ENOENT` — the mount evaporated with no local fallback); with rights,
/// the walker continues locally (the node is local again by then, :1414).
/// `restart_code` is the sentinel C compares against (normally ERESTART);
/// the parameter keeps the verdict testable without call-number imports.
pub const fn judge_remote_result(code: i32, restart_code: i32, can_restart: bool) -> RemoteOutcome {
    if code != restart_code {
        return RemoteOutcome::Return(code);
    }
    if !can_restart {
        return RemoteOutcome::Return(ENOENT);
    }
    RemoteOutcome::RestartLocal
}

/// Errno for the terminal verdicts; action verdicts carry none.
///
/// C: `ENOENT` (:1386), `ENOTDIR` (:1438), `EPERM` (write bars — 07).
/// `WriteDenied` maps here too so the walker never re-derives the bars'
/// code. Action verdicts (`RemoteCall`, `CallFunc`, `Readwrite`,
/// `Descend`) return `None`: their outcome is computed, not looked up.
pub const fn terminal_code(v: LevelVerdict) -> Option<i32> {
    use super::super::auth;
    match v {
        LevelVerdict::NotFound => Some(ENOENT),
        LevelVerdict::LeafOverflow => Some(ENOTDIR),
        LevelVerdict::WriteDenied => Some(auth::WRITE_DENIED),
        LevelVerdict::RemoteCall { .. }
        | LevelVerdict::CallFunc
        | LevelVerdict::Readwrite { .. }
        | LevelVerdict::Descend => None,
    }
}
/// Verdict when the name runs out on a plain parent.
///
/// C: `:1472-1473` → `EISDIR` ("the name refers to a node array" —
/// reading a directory as data).
pub const EISDIR_EMPTY: i32 = EISDIR;

/// Whether a flag word is a leaf (non-`NODE` type).
pub const fn is_leaf_flags(flags: u32) -> bool {
    flags & SYSCTL_TYPEMASK != CTLTYPE_NODE
}

/// Whether a flag word marks a remote node.
pub const fn is_remote_flags(flags: u32) -> bool {
    flags & CTLFLAG_REMOTE != 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use minix_types::{CTLFLAG_ANYWRITE, CTLFLAG_READWRITE, CTLTYPE_INT};

    #[test]
    fn test_judge_meta() {
        // Four ops, trailing garbage refused, rest unsupported (:1365-1380).
        assert_eq!(judge_meta(CTL_QUERY, 0), Ok(MetaOp::Query));
        assert_eq!(judge_meta(CTL_CREATE, 0), Ok(MetaOp::Create));
        assert_eq!(judge_meta(CTL_DESTROY, 0), Ok(MetaOp::Destroy));
        assert_eq!(judge_meta(CTL_DESCRIBE, 0), Ok(MetaOp::Describe));
        assert_eq!(judge_meta(CTL_QUERY, 2), Err(EINVAL));
        assert_eq!(judge_meta(-4, 0), Err(EOPNOTSUPP));
        assert_eq!(judge_meta(-6, 0), Err(EOPNOTSUPP));
        assert_eq!(judge_meta(-99, 0), Err(EOPNOTSUPP));
    }

    #[test]
    fn test_resolve_shape() {
        // Leaves: verify bit wins over func pointer (:1426-1427).
        assert_eq!(resolve_shape(true, false, true, true), (false, true));
        assert_eq!(resolve_shape(true, false, false, true), (true, false));
        assert_eq!(resolve_shape(true, false, false, false), (false, false));
        // Non-leaves: no PARENT means function (:1430).
        assert_eq!(resolve_shape(false, true, false, false), (false, false));
        assert_eq!(resolve_shape(false, false, false, true), (true, false));
    }

    #[test]
    fn test_judge_level_terminals() {
        let rw = CTLFLAG_READWRITE;
        // Leaf with name left: ENOTDIR (:1437-1438).
        assert_eq!(
            judge_level(
                true,
                false,
                false,
                false,
                false,
                2,
                false,
                rw,
                CallAuth::Yes
            ),
            LevelVerdict::LeafOverflow
        );
        // Write bars deny (07 contract, :1446-1458).
        assert_eq!(
            judge_level(true, false, false, false, false, 0, true, 0, CallAuth::Yes),
            LevelVerdict::WriteDenied
        );
        assert_eq!(
            judge_level(true, false, false, false, false, 0, true, rw, CallAuth::No),
            LevelVerdict::WriteDenied
        );
        // Func landing calls out (:1461-1462).
        assert_eq!(
            judge_level(true, false, false, true, false, 0, false, rw, CallAuth::No),
            LevelVerdict::CallFunc
        );
        // Plain leaf reads/writes, verify flag carried (:1465-1467).
        assert_eq!(
            judge_level(true, false, false, false, true, 0, false, rw, CallAuth::No),
            LevelVerdict::Readwrite { verify: true }
        );
        // Plain parent descends (:1469-1470).
        assert_eq!(
            judge_level(
                false,
                false,
                false,
                false,
                false,
                3,
                false,
                rw,
                CallAuth::No
            ),
            LevelVerdict::Descend
        );
        // Remote short-circuits before shape (:1404).
        assert_eq!(
            judge_level(false, true, true, false, false, 3, false, rw, CallAuth::No),
            LevelVerdict::RemoteCall { can_restart: true }
        );
        // ANYWRITE waives the uid bar (07).
        assert_eq!(
            judge_level(
                true,
                false,
                false,
                false,
                false,
                0,
                true,
                rw | CTLFLAG_ANYWRITE,
                CallAuth::No
            ),
            LevelVerdict::Readwrite { verify: false }
        );
    }

    #[test]
    fn test_judge_remote_result() {
        use minix_types::ERESTART;
        // Non-restart codes return as-is, success included (:1410-1411).
        assert_eq!(
            judge_remote_result(0, ERESTART, true),
            RemoteOutcome::Return(0)
        );
        assert_eq!(
            judge_remote_result(-22, ERESTART, false),
            RemoteOutcome::Return(-22)
        );
        // ERESTART without rights is a dead end (:1411 → ENOENT).
        assert_eq!(
            judge_remote_result(ERESTART, ERESTART, false),
            RemoteOutcome::Return(ENOENT)
        );
        // With rights, keep resolving locally (:1413).
        assert_eq!(
            judge_remote_result(ERESTART, ERESTART, true),
            RemoteOutcome::RestartLocal
        );
    }

    #[test]
    fn test_empty_is_dir() {
        // Name spent on a plain parent: EISDIR (:1472-1473).
        assert_eq!(EISDIR_EMPTY, EISDIR);
        assert!(is_leaf_flags(CTLTYPE_INT));
        assert!(!is_leaf_flags(CTLTYPE_NODE));
        assert!(is_remote_flags(CTLFLAG_REMOTE));
        // Terminal verdicts map to their codes; actions carry none.
        assert_eq!(terminal_code(LevelVerdict::NotFound), Some(ENOENT));
        assert_eq!(terminal_code(LevelVerdict::LeafOverflow), Some(ENOTDIR));
        assert_eq!(
            terminal_code(LevelVerdict::WriteDenied),
            Some(crate::auth::WRITE_DENIED)
        );
        assert_eq!(terminal_code(LevelVerdict::Descend), None);
        assert_eq!(terminal_code(LevelVerdict::CallFunc), None);
    }
}
