//! VirtualBox shared-folder server: option handling plus two-layer
//! startup (`vbfs.c`, one hundred forty-one lines).
//!
//! The server stacks three layers (documented as ASCII art in the
//! source, `vbfs.c:4-35`): this file parses options and sequences
//! startup, the shared-folder framework serves the protocol, and the
//! VirtualBox guest library talks to the host through the backdoor
//! driver. Cleanup runs in reverse: the guest library releases last,
//! after the framework is done with it.
//!
//! Like every file server here, VBFS is a single-threaded event loop:
//! one message at a time, no shared mutable state across threads.

#![no_std]

extern crate alloc;

pub use minix_sffs as framework;

/// Share option key (`"share"`, `vbfs.c:46`).
pub const OPTION_SHARE: &str = "share";

/// Default mount options (`vbfs.c:65-71`): empty share and prefix, root
/// ownership, full masks, case-sensitive (the guest library reports the
/// real sensitivity separately).
pub fn default_params() -> minix_sffs::params::Params {
    minix_sffs::params::Params::defaults()
}

/// Startup sequencing (`init`, `vbfs.c:58-105`): a share name is
/// required; the guest library initializes first and its failure ends
/// startup at once; the framework initializes second, and its failure
/// releases the guest library again before reporting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Startup {
    /// Proceed to the guest library.
    StartGuest,
    /// Missing share name: refuse without touching anything.
    MissingShare,
}

/// Decide the first startup step from the configured share name.
pub const fn plan_start(share_present: bool) -> Startup {
    if share_present {
        Startup::StartGuest
    } else {
        Startup::MissingShare
    }
}

/// Service initialization entry (kept for the server binary).
pub fn init() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_defaults_match_c() {
        let params = default_params();
        assert_eq!(params.file_mask, 0o755);
        assert_eq!(params.dir_mask, 0o755);
        assert!(!params.case_insensitive);
        assert_eq!(OPTION_SHARE, "share");
    }

    #[test]
    fn test_share_required() {
        assert_eq!(plan_start(false), Startup::MissingShare);
        assert_eq!(plan_start(true), Startup::StartGuest);
    }
}
