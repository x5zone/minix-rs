//! Describe verdicts: lengths, packing, set-guards.
//!
//! Mirrors the pure halves of `mib_copyout_desc` / `mib_describe`
//! (`tree.c:925-1090`). Staging into the scratch page, copying, and
//! `strdup` are arena/transport effects; judging lengths, packing, and
//! the six set-guards is done here.
//!
//! 11-mib-query-describe.md.

use minix_types::{CTLTYPE_NODE, SYSCTL_TYPEMASK};

use super::auth::CallAuth;

/// Header size of the exchange description: three lanes.
///
/// C: `offsetof(struct sysctldesc, descr_str)` — sys/sys/sysctl.h:1456
/// (`num` + `ver` + `len`, 4 bytes each).
pub const DESC_HEADER: u64 = 12;

/// Alignment of packed descriptions. C: 4 (`sizeof(int32_t)`,
/// `__sysc_desc_roundup`, sys/sys/sysctl.h:1449).
pub const DESC_ALIGN: u64 = 4;

/// Description payload length: terminator included, empty if none.
///
/// C: `mib_copyout_desc` length half — tree.c:937-941. `None` (no
/// description) still occupies one NUL — "no description" and "empty
/// description" are indistinguishable on the wire, by design.
pub const fn desc_len(desc_len_no_nul: Option<u64>) -> u64 {
    match desc_len_no_nul {
        Some(n) => n + 1,
        None => 1,
    }
}

/// Round a length up to the packing alignment.
///
/// C: `__sysc_desc_roundup(x)` — sys/sys/sysctl.h:1449.
pub const fn roundup_desc(x: u64) -> u64 {
    (x + (DESC_ALIGN - 1)) & !(DESC_ALIGN - 1)
}

/// Packed size of one description entry: header + text, aligned.
///
/// C: `size += offsetof(...)` then `roundup2(size, sizeof(int32_t))` —
/// tree.c:956-965. Inter-entry garbage after alignment is the caller's
/// own data ("fine because it is userland's own data", :961-964).
pub const fn packed_size(text_len_incl_nul: u64) -> u64 {
    roundup_desc(DESC_HEADER + text_len_incl_nul)
}

/// Whether a staged description fits the scratch page.
///
/// C: `assert(sizeof(*scd) + size <= sizeof(scratch))` — tree.c:943.
/// An assert, not an error: `MAXDESCLEN` (1024, 03) bounds every input,
/// so overflow here is a programming bug, never a user input.
pub const fn fits_scratch(text_len_incl_nul: u64, scratch: u64) -> bool {
    DESC_HEADER + text_len_incl_nul <= scratch
}

/// Set-description guards: six bars, one verdict.
///
/// C: `mib_describe` set path — tree.c:994-1031. Private nodes refuse
/// strangers (`EPERM`, :994-996); setting needs superuser (`:1003-1005`);
/// mount points are busy (`EBUSY`, :1012-1014 — "arguably unnecessary"
/// per the comment, kept anyway); existing descriptions refuse
/// (`EPERM`, :1016-1018 — set-once, NetBSD has no set-at-all, :751-754);
/// permanent nodes refuse (`EPERM`, :1020-1022); stale versions refuse
/// (`EINVAL`, :1029-1031 — checked only on set, NetBSD parity, :1025-1027).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetDescRefusal {
    /// Stranger at a private node. C: `:994-996` → `EPERM`.
    Private,
    /// Non-superuser setting. C: `:1003-1005` → `EPERM`.
    NeedSuperuser,
    /// Mount point. C: `:1012-1014` → `EBUSY`.
    MountBusy,
    /// Description already present. C: `:1016-1018` → `EPERM`.
    AlreadySet,
    /// Permanent node. C: `:1020-1022` → `EPERM`.
    Permanent,
    /// Staged version stale. C: `:1029-1031` → `EINVAL`.
    VersionStale,
}

/// Judge a set-description request (`Ok`) or name the bar.
#[allow(clippy::too_many_arguments)]
pub const fn check_set_desc(
    private: bool,
    auth: CallAuth,
    remote_node: bool,
    has_desc: bool,
    permanent: bool,
    staged_ver: u32,
    node_ver: u32,
) -> Result<(), (SetDescRefusal, i32)> {
    use minix_types::{EBUSY, EINVAL, EPERM};
    if private && !auth.is_authed() {
        return Err((SetDescRefusal::Private, EPERM));
    }
    if !auth.is_authed() {
        return Err((SetDescRefusal::NeedSuperuser, EPERM));
    }
    if remote_node {
        return Err((SetDescRefusal::MountBusy, EBUSY));
    }
    if has_desc {
        return Err((SetDescRefusal::AlreadySet, EPERM));
    }
    if permanent {
        return Err((SetDescRefusal::Permanent, EPERM));
    }
    if staged_ver != 0 && staged_ver != node_ver {
        return Err((SetDescRefusal::VersionStale, EINVAL));
    }
    Ok(())
}

/// Whether the flag word names a node-type node (describe's extra rules).
pub const fn is_node_flags(flags: u32) -> bool {
    flags & SYSCTL_TYPEMASK == CTLTYPE_NODE
}

/// Whether a private description hides from this caller.
///
/// Same predicate as 07's `can_see`, restated where C restates it
/// (`:934-935` returns *zero length*, not an error — bulk enumeration
/// skips silently rather than aborting the whole array).
pub const fn desc_visible(private: bool, auth: CallAuth) -> bool {
    use super::auth::can_see;
    can_see(private, auth)
}

#[cfg(test)]
mod tests {
    use super::*;
    use minix_types::{EBUSY, EINVAL, EPERM};

    #[test]
    fn test_desc_lengths() {
        // Terminator included; none still costs one NUL (:937-941).
        assert_eq!(desc_len(Some(0)), 1);
        assert_eq!(desc_len(Some(9)), 10);
        assert_eq!(desc_len(None), 1);
        // Packing: header + text, aligned to 4 (:956-965).
        assert_eq!(packed_size(1), 16);
        assert_eq!(packed_size(4), 16);
        assert_eq!(packed_size(5), 20);
        assert_eq!(roundup_desc(13), 16);
        // MAXDESCLEN fits scratch with room (:943, 03 SCRATCH_SIZE).
        assert!(fits_scratch(1024, 4096));
        assert!(!fits_scratch(4096, 4096));
    }

    #[test]
    fn test_set_desc_guards() {
        let yes = CallAuth::Yes;
        let no = CallAuth::No;
        // Six bars, in C order (:994-1031).
        assert_eq!(
            check_set_desc(true, no, false, false, false, 0, 5),
            Err((SetDescRefusal::Private, EPERM))
        );
        assert_eq!(
            check_set_desc(false, no, false, false, false, 0, 5),
            Err((SetDescRefusal::NeedSuperuser, EPERM))
        );
        assert_eq!(
            check_set_desc(false, yes, true, false, false, 0, 5),
            Err((SetDescRefusal::MountBusy, EBUSY))
        );
        assert_eq!(
            check_set_desc(false, yes, false, true, false, 0, 5),
            Err((SetDescRefusal::AlreadySet, EPERM))
        );
        assert_eq!(
            check_set_desc(false, yes, false, false, true, 0, 5),
            Err((SetDescRefusal::Permanent, EPERM))
        );
        assert_eq!(
            check_set_desc(false, yes, false, false, false, 6, 5),
            Err((SetDescRefusal::VersionStale, EINVAL))
        );
        assert_eq!(
            check_set_desc(false, yes, false, false, false, 5, 5),
            Ok(())
        );
        assert_eq!(
            check_set_desc(false, yes, false, false, false, 0, 5),
            Ok(())
        );
        // Private descriptions hide by zero length, not error (:934-935).
        assert!(!desc_visible(true, no));
        assert!(desc_visible(true, yes));
        assert!(desc_visible(false, no));
    }
}
