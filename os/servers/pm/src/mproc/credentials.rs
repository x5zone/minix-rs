//! 权限凭证定义
//!
//! 提供进程的 UID/GID 凭证管理

use minix_types::{Uid, Gid, IdSet};

/// 最大补充组数量
pub const NGROUPS_MAX: usize = 16;

/// 权限凭证
///
/// 存储进程的用户和组身份信息
///
/// # Minix3 映射
/// - `mp_realuid, mp_effuid, mp_svuid` → `user: IdSet<Uid>`
/// - `mp_realgid, mp_effgid, mp_svgid` → `group: IdSet<Gid>`
/// - `mp_supgroups` → `supplemental_groups`
/// - `mp_ngroups` → `ngroups`
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Credentials {
    /// 用户 ID 三元组
    pub user: IdSet<Uid>,
    /// 组 ID 三元组
    pub group: IdSet<Gid>,
    /// 补充组列表
    pub supplemental_groups: [Gid; NGROUPS_MAX],
    /// 补充组数量
    pub ngroups: usize,
}

impl Credentials {
    /// 创建新的凭证
    ///
    /// # 参数
    /// - `real_uid`: 真实用户 ID
    /// - `real_gid`: 真实组 ID
    ///
    /// effective 和 saved ID 会被设置为相同的值
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
    
    /// 检查是否是超级用户
    ///
    /// 超级用户的 effective UID 为 0
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
