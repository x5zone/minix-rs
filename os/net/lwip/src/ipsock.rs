//! Internet socket base: address kind, buffer bounds, option ranges.
//!
//! C correspondence: the address-kind mapping (`ipsock_get_type`,
//! `ipsock.c:103-111`: fourth-version only, sixth-version only, or
//! either), the buffer storage (`sndbuf`/`rcvbuf` carried in
//! `struct ipsock`, `ipsock.h:8-9`, set at creation,
//! `ipsock.c:132-133`), and the option ranges (time-to-live and
//! type-of-service between zero and the largest byte value,
//! `ipsock.c:518-530`; sixth-version hop limit between minus one
//! and the largest byte value, minus one meaning the default,
//! `ipsock.c:550-559`).
//!
//! Socket storage stays in the service binary; this module owns the
//! pure bounds half: which kind, how big a buffer, which option
//! values pass.

/// Largest value of a one-byte option field.
pub const MAX_BYTE_OPTION: u32 = 255;

/// Sentinel meaning the default hop limit (`-1`, `ipsock.c:558-559`).
pub const DEFAULT_HOP_LIMIT: i32 = -1;

/// Address kind of one internet socket (`ipsock_get_type`,
/// `ipsock.c:103-111`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressKind {
    /// Fourth-version addresses only.
    Version4,
    /// Sixth-version addresses only.
    Version6,
    /// Either version.
    Either,
}

/// Map the sixth-version-only flag to an address kind
/// (`ipsock.c:106-111`).
pub fn address_kind(ipv6: bool, version6_only: bool) -> AddressKind {
    if !ipv6 {
        AddressKind::Version4
    } else if version6_only {
        AddressKind::Version6
    } else {
        AddressKind::Either
    }
}

/// Whether a time-to-live or type-of-service value passes
/// (`ipsock.c:518-530`).
pub fn byte_option_allowed(value: u32) -> bool {
    value <= MAX_BYTE_OPTION
}

/// Whether a sixth-version hop limit passes (`ipsock.c:550-559`):
/// minus one (default) through the largest byte value.
pub fn hop_limit_allowed(value: i32) -> bool {
    value == DEFAULT_HOP_LIMIT || (0..=MAX_BYTE_OPTION as i32).contains(&value)
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
        assert!(hop_limit_allowed(64));
        assert!(!hop_limit_allowed(-2));
        assert!(!hop_limit_allowed(256));
    }
}
