//! Relay verdicts: who may read/write whose bytes, and how failures speak.
//!
//! Mirrors the pure halves of `mib_relay_oldp` / `mib_relay_newp`
//! (`main.c:204-252`). Grant *creation* (`cpf_grant_magic`) is an effect
//! owned by the transport (A-12); this module judges direction, presence,
//! and the failure code — creation failure must never speak `ENOMEM`
//! (`:208`, `:236`: "must not be ENOMEM").
//!
//! 06-mib-copy-io.md.

use minix_types::{CPF_READ, CPF_WRITE, EINVAL, GrantId};

/// Invalid grant: no region behind it. C: `GRANT_INVALID` — safecopies.h:52.
pub const GRANT_INVALID: GrantId = -1;

/// Whether a grant id names a region. C: `GRANT_VALID(g)` — safecopies.h:53.
pub const fn grant_valid(g: GrantId) -> bool {
    g > GRANT_INVALID
}

/// Relay direction: which way the service may move the bytes.
///
/// C: `CPF_WRITE` for old regions (the service writes answers into the
/// user's sink — main.c:216-217), `CPF_READ` for new regions (the service
/// reads what the user wrote — :241-242). The names read from MIB's side:
/// "I grant you write into my old sink."
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelayDir {
    /// Service may write (old-data sink). C: `CPF_WRITE`.
    Write,
    /// Service may read (new-data source). C: `CPF_READ`.
    Read,
}

impl RelayDir {
    /// Wire flag for the grant. C: `CPF_WRITE`/`CPF_READ` — safecopies.h:64-65.
    pub const fn flag(self) -> i32 {
        match self {
            Self::Write => CPF_WRITE,
            Self::Read => CPF_READ,
        }
    }
}

/// A relayed region: present with a length, or absent.
///
/// `None` grant = `GRANT_INVALID` (matches the `Option::None` convention
/// at `minix-types` `GrantId`). Presence is judged here; the grant id
/// itself is filled by the transport at relay time (12).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RelayRegion {
    /// Grant when present. C: `*grantp` (`GRANT_INVALID` when shut).
    pub grant: Option<GrantId>,
    /// Region length (0 when shut). C: `*lenp`.
    pub len: u64,
}

impl RelayRegion {
    /// Judge an old-data region for relay (`main.c:210-227`).
    ///
    /// Shut sinks relay as invalid + zero (`:221-224`); open ones relay
    /// their length with a write grant to be created (`:215-220`).
    pub const fn relay_old(sink: Option<(u32, u64)>) -> Self {
        match sink {
            None => Self {
                grant: None,
                len: 0,
            },
            Some((_, len)) => Self {
                grant: Some(0),
                len,
            },
        }
    }

    /// Judge a new-data region for relay (`main.c:235-252`).
    /// Mirror of [`RelayRegion::relay_old`] with read direction.
    pub const fn relay_new(data: Option<(u32, u64)>) -> Self {
        match data {
            None => Self {
                grant: None,
                len: 0,
            },
            Some((_, len)) => Self {
                grant: Some(0),
                len,
            },
        }
    }
}

/// Failure code for grant creation. C: "must not be ENOMEM" —
/// main.c:208,236. Allocation pressure must never masquerade as a small
/// sink (01 §1.4): sinks speak `ENOMEM`, allocators speak `EINVAL`.
/// Callers convert `GrantOutcome::Failed` with this, never by invention.
pub const RELAY_FAIL: i32 = EINVAL;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grant_validity() {
        // C: GRANT_VALID(g) = g > GRANT_INVALID (safecopies.h:52-53).
        assert_eq!(GRANT_INVALID, -1);
        assert!(!grant_valid(-1));
        assert!(grant_valid(0));
        assert!(grant_valid(41));
    }

    #[test]
    fn test_relay_directions() {
        // Old regions grant write, new regions grant read (main.c:216-217,241-242).
        assert_eq!(RelayDir::Write.flag(), CPF_WRITE);
        assert_eq!(RelayDir::Read.flag(), CPF_READ);
        assert_eq!((CPF_WRITE, CPF_READ), (2, 1));
    }

    #[test]
    fn test_relay_presence() {
        // Shut regions relay invalid + zero (main.c:221-224,246-249).
        assert_eq!(
            RelayRegion::relay_old(None),
            RelayRegion {
                grant: None,
                len: 0
            }
        );
        assert_eq!(
            RelayRegion::relay_new(None),
            RelayRegion {
                grant: None,
                len: 0
            }
        );
        // Open regions relay their length; the id comes from transport.
        assert_eq!(RelayRegion::relay_old(Some((0x1000, 64))).len, 64);
        assert_eq!(RelayRegion::relay_new(Some((0x3000, 9))).len, 9);
        // Creation failure speaks EINVAL, never ENOMEM (main.c:208,236).
        assert_eq!(RELAY_FAIL, EINVAL);
    }
}
