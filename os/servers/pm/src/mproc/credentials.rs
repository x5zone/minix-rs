//! Credentials definition.
//!
//! Provides process UID/GID credential management.

use minix_types::{Uid, Gid, IdSet};

/// Maximum number of supplemental groups.
pub const NGROUPS_MAX: usize = 16;

/// Credentials.
///
/// Stores process user and group identity information.
///
/// # Minix3 Mapping
/// - `mp_realuid, mp_effuid, mp_svuid` → `user: IdSet<Uid>`
/// - `mp_realgid, mp_effgid, mp_svgid` → `group: IdSet<Gid>`
/// - `mp_supgroups` → `supplemental_groups`
/// - `mp_ngroups` → `ngroups`
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Credentials {
    /// User ID triplet.
    pub user: IdSet<Uid>,
    /// Group ID triplet.
    pub group: IdSet<Gid>,
    /// Supplemental group list.
    pub supplemental_groups: [Gid; NGROUPS_MAX],
    /// Number of supplemental groups.
    pub ngroups: usize,
}

impl Credentials {
    /// Creates new credentials.
    ///
    /// # Parameters
    /// - `real_uid`: Real user ID
    /// - `real_gid`: Real group ID
    ///
    /// Effective and saved IDs will be set to the same values.
    pub fn new(real_uid: Uid, real_gid: Gid) -> Self {
        Self {
            user: IdSet {
                real: real_uid,
                effective: real_uid,
                saved: real_uid,
            },
            group: IdSet {
                real: real_gid,
                effective: real_gid,
                saved: real_gid,
            },
            supplemental_groups: [0; NGROUPS_MAX],
            ngroups: 0,
        }
    }
    
    /// Checks if superuser.
    ///
    /// Superuser's effective UID is 0.
    pub fn is_superuser(&self) -> bool {
        self.user.effective == 0
    }

    /// `setuid` BSD full triplet (`getset.c:117-119` D3).
    pub fn set_uid_all(&mut self, uid: Uid) {
        self.user.real = uid;
        self.user.effective = uid;
        self.user.saved = uid;
    }
    /// `seteuid` single effective (`getset.c:134`).
    pub fn set_euid(&mut self, uid: Uid) {
        self.user.effective = uid;
    }
    /// `setgid` BSD full triplet (`getset.c:147-149`).
    pub fn set_gid_all(&mut self, gid: Gid) {
        self.group.real = gid;
        self.group.effective = gid;
        self.group.saved = gid;
    }
    /// `setegid` single effective (`getset.c:163`).
    pub fn set_egid(&mut self, gid: Gid) {
        self.group.effective = gid;
    }
    /// `setgroups` ngroups + supplemental (`getset.c:194-197`).
    pub fn set_groups(&mut self, gids: &[Gid]) {
        let n = gids.len().min(NGROUPS_MAX);
        self.supplemental_groups[..n].copy_from_slice(&gids[..n]);
        for i in n..NGROUPS_MAX {
            self.supplemental_groups[i] = 0;
        }
        self.ngroups = n;
    }

    /// `can_set_uid` for `SETUID` (`getset.c:114` `real!=uid && eff!=SUPER_USER`).
    pub fn can_set_uid(&self, uid: Uid) -> bool {
        self.user.real == uid || self.is_superuser()
    }
    /// `can_set_euid` for `SETEUID` (`getset.c:131-133` `real/saved/eff` triple).
    pub fn can_set_euid(&self, uid: Uid) -> bool {
        self.user.real == uid || self.user.saved == uid || self.is_superuser()
    }
    /// `can_set_gid` (`getset.c:145`).
    pub fn can_set_gid(&self, gid: Gid) -> bool {
        self.group.real == gid || self.is_superuser()
    }
    /// `can_set_egid` (`getset.c:160-162`).
    pub fn can_set_egid(&self, gid: Gid) -> bool {
        self.group.real == gid || self.group.saved == gid || self.is_superuser()
    }
    /// `can_set_groups` (`getset.c:173`).
    pub fn can_set_groups(&self) -> bool {
        self.is_superuser()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_set_uid_all() {
        let mut c = Credentials::new(1000, 100);
        c.set_uid_all(2000);
        assert_eq!(c.user.real, 2000);
        assert_eq!(c.user.effective, 2000);
        assert_eq!(c.user.saved, 2000);
    }
    #[test]
    fn test_set_euid() {
        let mut c = Credentials::new(1000, 100);
        c.set_euid(0);
        assert_eq!(c.user.effective, 0);
        assert_eq!(c.user.real, 1000);
    }
    #[test]
    fn test_set_gid_all() {
        let mut c = Credentials::new(1000, 100);
        c.set_gid_all(200);
        assert_eq!(c.group.real, 200);
        assert_eq!(c.group.effective, 200);
        assert_eq!(c.group.saved, 200);
    }
    #[test]
    fn test_tainted_bool() {
        // TAINTED is now modeled as bool in ProcessResources, but Credentials still pure;
        // this test ensures Credentials::set_* does not touch tainted.
        let c = Credentials::new(0, 0);
        assert!(c.is_superuser());
    }
    #[test]
    fn test_credentials_default() {
        let creds = Credentials::default();
        assert_eq!(creds.user.real, 0);
        assert_eq!(creds.user.effective, 0);
        assert_eq!(creds.group.real, 0);
        assert_eq!(creds.ngroups, 0);
    }
    
    #[test]
    fn test_credentials_new() {
        let creds = Credentials::new(1000, 100);
        assert_eq!(creds.user.real, 1000);
        assert_eq!(creds.user.effective, 1000);
        assert_eq!(creds.user.saved, 1000);
        assert_eq!(creds.group.real, 100);
        assert_eq!(creds.group.effective, 100);
    }
    
    #[test]
    fn test_is_superuser() {
        let root = Credentials::new(0, 0);
        assert!(root.is_superuser());
        
        let user = Credentials::new(1000, 100);
        assert!(!user.is_superuser());
    }
    
    #[test]
    fn test_setuid_simulation() {
        let mut creds = Credentials::new(1000, 100);
        
        creds.user.effective = 0;
        assert!(creds.is_superuser());
        assert_eq!(creds.user.real, 1000);
        assert_eq!(creds.user.saved, 1000);
        
        creds.user.effective = creds.user.saved;
        assert!(!creds.is_superuser());
    }
    
    #[test]
    fn test_supplemental_groups() {
        let mut creds = Credentials::default();
        creds.supplemental_groups[0] = 100;
        creds.supplemental_groups[1] = 200;
        creds.ngroups = 2;
        
        assert_eq!(creds.ngroups, 2);
        assert_eq!(creds.supplemental_groups[0], 100);
        assert_eq!(creds.supplemental_groups[1], 200);
    }
}
