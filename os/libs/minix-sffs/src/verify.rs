//! Freshness decisions: what a host answer means for a cached node
//! (`verify.c`).
//!
//! The host may change at any moment, so every cached node is guilty
//! until re-checked: the framework asks the host for the path's current
//! attributes and compares. A vanished path deletes the node; a changed
//! file kind deletes the node and reports the path as stale (valid path,
//! wrong node); anything else propagates the host's own error. Only a
//! full match keeps the node.

use minix_types::{ENOENT, ENOTDIR, Errno};

/// Host answer for one path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostAnswer {
    /// Attributes match the cache (caller compares kinds separately).
    Match {
        /// Whether the host says directory.
        is_directory: bool,
    },
    /// Host reports missing or not-a-directory.
    Gone {
        /// The host's own code (missing or not-a-directory).
        code: Errno,
    },
    /// Any other host failure, passed through untouched.
    Failed {
        /// The host's own code.
        code: Errno,
    },
}

/// What the framework does with a node after asking the host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Freshness {
    /// Keep the node; path and node both valid.
    Fresh,
    /// Delete the node; the path itself is gone.
    DeleteNode,
    /// Delete the node but flag the path as stale: the path is valid,
    /// the node behind it changed kind (`verify_path`,
    /// `verify.c:47-53`, the `stale` out-parameter).
    DeleteNodeButStale,
    /// Propagate the host error; the node stays for next time.
    Propagate(Errno),
}

/// Decide freshness (`verify_path`, `verify.c:17-56`): the caller always
/// asks for the mode bit first, so kind comparison is always possible.
pub const fn decide(cached_is_directory: bool, answer: HostAnswer) -> Freshness {
    match answer {
        HostAnswer::Match { is_directory } => {
            if cached_is_directory == is_directory {
                Freshness::Fresh
            } else {
                Freshness::DeleteNodeButStale
            }
        }
        HostAnswer::Gone { .. } => Freshness::DeleteNode,
        HostAnswer::Failed { code } => Freshness::Propagate(code),
    }
}

/// The wire code a lookup reports for each outcome.
pub const fn outcome_code(outcome: Freshness, gone: Errno) -> Errno {
    match outcome {
        Freshness::Fresh => Errno::from_i32(0),
        Freshness::DeleteNode => gone,
        Freshness::DeleteNodeButStale => Errno::from_i32(ENOENT),
        Freshness::Propagate(code) => code,
    }
}

/// Host codes that delete the node (`verify.c:41`).
pub const fn deletes_node(code: Errno) -> bool {
    code.to_i32() == ENOENT || code.to_i32() == ENOTDIR
}

#[cfg(test)]
mod tests {
    use super::*;
    use minix_types::{EACCES, EIO};

    #[test]
    fn test_match_keeps_or_stales() {
        assert_eq!(
            decide(true, HostAnswer::Match { is_directory: true }),
            Freshness::Fresh
        );
        assert_eq!(
            decide(true, HostAnswer::Match { is_directory: false }),
            Freshness::DeleteNodeButStale
        );
    }

    #[test]
    fn test_gone_deletes() {
        assert_eq!(
            decide(false, HostAnswer::Gone { code: Errno::from_i32(ENOENT) }),
            Freshness::DeleteNode
        );
        assert_eq!(
            outcome_code(Freshness::DeleteNodeButStale, Errno::from_i32(ENOTDIR)),
            Errno::from_i32(ENOENT)
        );
    }

    #[test]
    fn test_other_errors_pass_through() {
        assert_eq!(
            decide(true, HostAnswer::Failed { code: Errno::from_i32(EIO) }),
            Freshness::Propagate(Errno::from_i32(EIO))
        );
        assert!(deletes_node(Errno::from_i32(ENOENT)));
        assert!(deletes_node(Errno::from_i32(ENOTDIR)));
        assert!(!deletes_node(Errno::from_i32(EACCES)));
    }
}
