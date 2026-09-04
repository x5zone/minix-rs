//! Securelevel and init.root interaction (deferred gaps).
//!
//! Covers `minix3/sbin/init/init.c:544-618` and `1811-1900`.
//! ARCH A-4 (securelevel) and A-5 (init.root) are deferred: the
//! traits below define the contract; live sysctl wiring lands with
//! the kernel service.
//! Design contract: `.design/12-design.v1.md §1.1-§1.2`.

/// Secure-level boundary (C: has/get/setsecuritylevel).
pub trait SecureLevel {
    fn present(&self) -> bool;
    fn get(&self) -> Option<i32>;
    fn set(&mut self, level: i32) -> bool;
}

/// In-memory fake level for tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FakeSecureLevel {
    pub supported: bool,
    pub level: i32,
    pub sets: Vec<i32>,
}

impl SecureLevel for FakeSecureLevel {
    fn present(&self) -> bool {
        self.supported
    }

    fn get(&self) -> Option<i32> {
        if !self.supported {
            // C: return -1 when unsupported (init.c:575-576, 587).
            return None;
        }
        Some(self.level)
    }

    fn set(&mut self, level: i32) -> bool {
        if !self.supported || level == self.level {
            return false;
        }
        self.level = level;
        self.sets.push(level);
        true
    }
}

/// Whether to chroot (C: `shouldchroot`, init.c:1896-1899).
pub fn should_chroot(rootdir: &str) -> bool {
    !rootdir.is_empty() && rootdir != "/"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_absent_returns_minus_one() {
        let lvl = FakeSecureLevel {
            supported: false,
            level: 0,
            sets: Vec::new(),
        };
        assert_eq!(lvl.get(), None);
    }

    #[test]
    fn test_set_same_noop() {
        let mut lvl = FakeSecureLevel {
            supported: true,
            level: 1,
            sets: Vec::new(),
        };
        assert!(!lvl.set(1));
        assert!(lvl.sets.is_empty());
    }

    #[test]
    fn test_single_user_downgrades() {
        let mut lvl = FakeSecureLevel {
            supported: true,
            level: 1,
            sets: Vec::new(),
        };
        // single_user: if level > 0, set 0 (init.c:723-725).
        if lvl.get().unwrap_or(0) > 0 {
            lvl.set(0);
        }
        assert_eq!(lvl.get(), Some(0));
    }

    #[test]
    fn test_should_chroot_matrix() {
        assert!(should_chroot("/newroot"));
        assert!(!should_chroot("/"));
        assert!(!should_chroot(""));
    }

    #[test]
    fn test_root_slash_no_chroot() {
        assert!(!should_chroot("/"));
    }
}
