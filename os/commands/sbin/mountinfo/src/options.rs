//! Mount option list parsing (`-o` and the table's option field).
//!
//! Ground truth: the `-o` flag in `minix3/minix/commands/mount/mount.c`
//! (line 47) hands an option string to the mount call. Recognised words:
//! `ro` (read only), `rw` (read write), `noexec` (no execution),
//! `nosuid` (ignore set identifiers), `sync` (synchronous writes),
//! `noatime` (no access time updates), `nodev` (no device files). Unknown
//! words are an error (a misspelled option must not silently mount with
//! weaker guarantees — the classic footgun this strictness prevents).

use crate::MountError;

/// One parsed option list as bit flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MountOptions {
    bits: u16,
}

impl MountOptions {
    const READ_ONLY: u16 = 1;
    const NO_EXEC: u16 = 2;
    const NO_SUID: u16 = 4;
    const SYNC: u16 = 8;
    const NO_ATIME: u16 = 16;
    const NO_DEV: u16 = 32;

    /// Empty option set (read write, everything allowed).
    pub fn empty() -> Self {
        MountOptions { bits: 0 }
    }

    /// True for read only mounts.
    pub fn read_only(self) -> bool {
        self.bits & Self::READ_ONLY != 0
    }

    /// True when execution is refused.
    pub fn no_exec(self) -> bool {
        self.bits & Self::NO_EXEC != 0
    }

    /// True when set identifiers are ignored.
    pub fn no_suid(self) -> bool {
        self.bits & Self::NO_SUID != 0
    }

    /// True for synchronous writes.
    pub fn sync_writes(self) -> bool {
        self.bits & Self::SYNC != 0
    }

    /// True when access times stay untouched.
    pub fn no_atime(self) -> bool {
        self.bits & Self::NO_ATIME != 0
    }

    /// True when device files refuse to work.
    pub fn no_dev(self) -> bool {
        self.bits & Self::NO_DEV != 0
    }
}

/// Parse a comma separated option list (`rw,noexec`). Empty words (from a
/// doubled comma) are rejected, matching the strictness rule above.
pub fn parse_options(text: &str) -> Result<MountOptions, MountError> {
    let mut options = MountOptions::empty();
    if text.is_empty() {
        return Err(MountError::InvalidArgument);
    }
    for word in text.split(',') {
        match word {
            "ro" => options.bits |= MountOptions::READ_ONLY,
            "rw" => {}
            "noexec" => options.bits |= MountOptions::NO_EXEC,
            "nosuid" => options.bits |= MountOptions::NO_SUID,
            "sync" => options.bits |= MountOptions::SYNC,
            "noatime" => options.bits |= MountOptions::NO_ATIME,
            "nodev" => options.bits |= MountOptions::NO_DEV,
            _ => return Err(MountError::InvalidArgument),
        }
    }
    Ok(options)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_common_lists() {
        let options = parse_options("rw,noexec,nosuid").unwrap();
        assert!(!options.read_only());
        assert!(options.no_exec());
        assert!(options.no_suid());
        assert!(!options.sync_writes());
    }

    #[test]
    fn test_read_only() {
        assert!(parse_options("ro").unwrap().read_only());
    }

    #[test]
    fn test_unknown_word_rejected() {
        // A misspelled option must fail loudly, never mount degraded.
        assert_eq!(
            parse_options("rw,noexce"),
            Err(MountError::InvalidArgument)
        );
        assert_eq!(parse_options(""), Err(MountError::InvalidArgument));
        assert_eq!(
            parse_options("rw,,ro"),
            Err(MountError::InvalidArgument)
        );
    }
}
