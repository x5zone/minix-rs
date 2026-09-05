//! Internet-layer socket policy: address kinds, buffer rules, option bounds.
//!
//! C correspondence: `minix3/minix/net/lwip/ipsock.c` (761 lines) with the
//! shared socket layout in `minix3/minix/net/lwip/ipsock.h:1-60`. Socket
//! storage, interface lookups, and user-memory copies stay in the service
//! binary. This module owns the portion that can be decided from numbers
//! alone: which address kind a socket has, which buffer sizes pass, and which
//! option values pass.
//!
//! The design follows the same split the Linux socket layer and the Redox
//! network stack use: validation helpers are pure and total (they answer yes
//! or no for every input), while the service binary owns tables, locks, and
//! input-output. Callers convert the boolean answer into the Minix wire error
//! at the message boundary, so the library never invents error numbers.

/// Largest value of a one-byte option field (time to live, type of service).
pub const MAX_BYTE_OPTION: u32 = 255;

/// Sentinel meaning the default hop limit (`-1`, `ipsock.c:558-559`).
pub const DEFAULT_HOP_LIMIT: i32 = -1;

/// Smallest hop limit value that can be set explicitly.
pub const MIN_HOP_LIMIT: i32 = 0;

/// Address kind of one internet socket (`ipsock_get_type`,
/// `ipsock.c:102-112`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressKind {
    /// Internet Protocol version 4 addresses only.
    Version4,
    /// Internet Protocol version 6 addresses only.
    Version6,
    /// Either version (a version 6 socket that also accepts mapped
    /// version 4 addresses).
    Either,
}

/// Map the socket flags to an address kind (`ipsock.c:106-111`).
///
/// The original reads two flag bits: `IPF_IPV6` (`ipsock.h:21`) and
/// `IPF_V6ONLY` (`ipsock.h:22`). A socket without the version 6 bit is
/// version 4 only. A version 6 socket with the only bit set is version 6
/// only. A version 6 socket without the only bit accepts either version.
/// Two booleans have four combinations but only three outcomes; the fourth
/// combination (not version 6, but version 6 only) still means version 4,
/// which is why an enumeration with three variants is more honest than two
/// independent booleans.
pub fn address_kind(is_version6: bool, is_version6_only: bool) -> AddressKind {
    if !is_version6 {
        AddressKind::Version4
    } else if is_version6_only {
        AddressKind::Version6
    } else {
        AddressKind::Either
    }
}

/// Whether a time-to-live or type-of-service value passes
/// (`ipsock_setsockopt`, `ipsock.c:518-530`: range 0 through 255, otherwise
/// the invalid argument error).
pub fn byte_option_allowed(value: u32) -> bool {
    value <= MAX_BYTE_OPTION
}

/// Whether a version 6 hop limit passes (`ipsock.c:550-559`): the default
/// sentinel through the largest byte value. The sentinel asks the stack to
/// use the system default (`IP_DEFAULT_TTL`); the service translates it after
/// validation.
pub fn hop_limit_allowed(value: i32) -> bool {
    value == DEFAULT_HOP_LIMIT || (MIN_HOP_LIMIT..=MAX_BYTE_OPTION as i32).contains(&value)
}

/// Whether a send or receive buffer size passes a caller-supplied bound.
///
/// The internet layer itself does not fix the bound; each protocol fills an
/// `ipopts` limit table (`ipsock.h:48-58`, fields `sndmin`, `sndmax`,
/// `rcvmin`, `rcvmax`) and the socket layers check against it
/// (`ipsock_setsockopt`, `ipsock.c:484-510`; `ipsock_getsockopt`,
/// `ipsock.c:628-644`). This helper owns the closed-interval comparison so
/// every protocol shares one implementation.
pub fn buffer_size_allowed(value: usize, minimum: usize, maximum: usize) -> bool {
    minimum <= value && value <= maximum
}

/// Whether toggling the version 6 only flag is currently meaningful.
///
/// The original records the flag at creation (`ipsock_socket`,
/// `ipsock.c:127-130`) and documents that changing it after the socket is
/// bound has no effect (`ipsock.c:587-589`: "has no effect once bound").
/// The service binary enforces the timing; this helper documents the rule in
/// one place: only an unbound version 6 socket can change the flag.
pub fn v6only_change_allowed(is_version6: bool, is_bound: bool) -> bool {
    is_version6 && !is_bound
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kind_mapping_matches_getter() {
        assert_eq!(address_kind(false, false), AddressKind::Version4);
        assert_eq!(address_kind(false, true), AddressKind::Version4);
        assert_eq!(address_kind(true, true), AddressKind::Version6);
        assert_eq!(address_kind(true, false), AddressKind::Either);
    }

    #[test]
    fn test_byte_options_span_zero_to_max() {
        assert_eq!(MAX_BYTE_OPTION, 255);
        assert!(byte_option_allowed(0));
        assert!(byte_option_allowed(255));
        assert!(!byte_option_allowed(256));
    }

    #[test]
    fn test_hop_limit_accepts_default_sentinel() {
        assert_eq!(DEFAULT_HOP_LIMIT, -1);
        assert!(hop_limit_allowed(-1));
        assert!(hop_limit_allowed(0));
        assert!(hop_limit_allowed(64));
        assert!(hop_limit_allowed(255));
        assert!(!hop_limit_allowed(-2));
        assert!(!hop_limit_allowed(256));
    }

    #[test]
    fn test_buffer_interval_is_closed_on_both_ends() {
        assert!(buffer_size_allowed(1, 1, 131_072));
        assert!(buffer_size_allowed(131_072, 1, 131_072));
        assert!(!buffer_size_allowed(0, 1, 131_072));
        assert!(!buffer_size_allowed(131_073, 1, 131_072));
    }

    #[test]
    fn test_v6only_change_only_before_bind() {
        assert!(v6only_change_allowed(true, false));
        assert!(!v6only_change_allowed(true, true));
        assert!(!v6only_change_allowed(false, false));
    }
}
