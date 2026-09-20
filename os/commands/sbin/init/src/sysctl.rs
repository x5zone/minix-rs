//! Securelevel and init.root interaction.
//!
//! Covers `minix3/sbin/init/init.c:544-618` and `1811-1900`.
//! ARCH A-4 (securelevel) and A-5 (init.root): the machine answers come
//! through [`InitHost`], whose live implementation asks the MIB service
//! via `minix_sys`'s sysctl face. Design contract:
//! `.design/12-design.v1.md §1.1-§1.2`.

use crate::host::InitHost;

/// Read the kernel security level.
///
/// `Ok(None)` covers "the node does not exist" — the C
/// `getsecuritylevel` answer of -1 on a kernel without securelevel
/// support (init.c:575-576, 587). An `Err` (live host: ENOSYS) means
/// the question cannot be asked yet; callers treat it like `None`
/// plus a warning.
pub fn get_securitylevel(host: &dyn InitHost) -> Option<i32> {
    host.securitylevel().ok().flatten()
}

/// Lower the security level to `level` (C: `setsecuritylevel`,
/// init.c:595-618).
///
/// `Ok(false)` covers both "unsupported" and "already at that level" —
/// the two C no-op shapes. Single-user does this downgrade at
/// init.c:723-725 when the level is above zero.
pub fn set_securitylevel(host: &mut dyn InitHost, level: i32) -> bool {
    matches!(host.set_securitylevel(level), Ok(true))
}

/// Whether to chroot before running `/etc/rc` (C: `shouldchroot`,
/// init.c:1859-1900).
///
/// C reads the `init.root` sysctl node, recreates it on ENOENT, and
/// chroots only when the value names a non-`/` directory. The
/// node-recreate half is A-5; here the host answers, and an unreadable
/// node counts as "no chroot" — the value C starts from when the node
/// does not exist.
pub fn should_chroot(host: &dyn InitHost) -> bool {
    match host.init_root() {
        Ok(Some(rootdir)) => !rootdir.is_empty() && rootdir != "/",
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::ScriptHost;

    #[test]
    fn test_absent_node_means_no_level() {
        let host = ScriptHost::default();
        assert_eq!(get_securitylevel(&host), None);
    }

    #[test]
    fn test_set_same_level_is_noop() {
        let mut host = ScriptHost::default();
        host.securitylevel = Some(1);
        assert!(!set_securitylevel(&mut host, 1));
        assert!(host.securitylevel_sets.is_empty());
    }

    #[test]
    fn test_single_user_downgrades() {
        // C: if level > 0, set 0 (init.c:723-725).
        let mut host = ScriptHost::default();
        host.securitylevel = Some(1);
        if get_securitylevel(&host).unwrap_or(0) > 0 {
            set_securitylevel(&mut host, 0);
        }
        assert_eq!(get_securitylevel(&host), Some(0));
        assert_eq!(host.securitylevel_sets, vec![0]);
    }

    #[test]
    fn test_chroot_matrix() {
        let mut host = ScriptHost::default();
        host.root = Some("/newroot".into());
        assert!(should_chroot(&host));
        host.root = Some("/".into());
        assert!(!should_chroot(&host));
        host.root = Some("".into());
        assert!(!should_chroot(&host));
        // Unreadable node: no chroot, no crash.
        let bare = ScriptHost::default();
        assert!(!should_chroot(&bare));
    }
}
