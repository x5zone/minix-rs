//! VMware shared-folder server: option handling plus two-layer
//! startup (`hgfs.c`, one hundred six lines).
//!
//! The shape mirrors the VirtualBox twin: this file parses options and
//! sequences startup, the shared-folder framework serves the protocol,
//! and the VMware guest library talks to the host. Two differences:
//! no share name is required (the whole shared area mounts at once),
//! and case handling is an explicit mount option pair (`icase` sets,
//! `noicase` clears, last one wins).
//!
//! Like every file server here, HGFS is a single-threaded event loop:
//! one message at a time, no shared mutable state across threads.

#![no_std]

extern crate alloc;

pub use minix_sffs as framework;

/// Case-insensitive option key (`"icase"`, `hgfs.c:25`).
pub const OPTION_INSENSITIVE: &str = "icase";
/// Case-sensitive option key (`"noicase"`, `hgfs.c:26`).
pub const OPTION_SENSITIVE: &str = "noicase";

/// Default mount options (`hgfs.c:41-46`): empty prefix, root
/// ownership, full masks, case-sensitive.
pub fn default_params() -> minix_sffs::params::Params {
    minix_sffs::params::Params::defaults()
}

/// Fold an option stream into the case flag (`hgfs.c:25-26`): each
/// occurrence sets or clears, so the last occurrence wins; anything
/// else leaves the flag alone.
pub fn apply_case_option(case_insensitive: &mut bool, key: &str) {
    if key == OPTION_INSENSITIVE {
        *case_insensitive = true;
    } else if key == OPTION_SENSITIVE {
        *case_insensitive = false;
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
        assert!(!params.case_insensitive);
    }

    #[test]
    fn test_case_option_last_wins() {
        let mut flag = false;
        apply_case_option(&mut flag, OPTION_INSENSITIVE);
        assert!(flag);
        apply_case_option(&mut flag, "prefix");
        assert!(flag);
        apply_case_option(&mut flag, OPTION_SENSITIVE);
        assert!(!flag);
    }
}
