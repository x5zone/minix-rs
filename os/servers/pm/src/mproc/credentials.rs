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
}

#[cfg(test)]
mod tests {
    use super::*;
    
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
