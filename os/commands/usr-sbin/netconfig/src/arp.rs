//! Neighbor table vocabulary.
//!
//! Ground truth: `minix3/usr.sbin/arp/arp.c`. Neighbor entries are managed
//! through the routing socket: lookup with `RTM_GET` (near line 315), creation
//! with `RTM_ADD` (near line 350), removal with `RTM_DELETE` (near line 434).
//! Deleting a missing entry reports `ESRCH` (near line 690). The execution
//! layer owns the socket; this module owns the verbs and the entry shape.

use crate::NetconfigError;

/// Neighbor table verbs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArpVerb {
    /// Show entries.
    Show,
    /// Add an entry.
    Add,
    /// Delete an entry.
    Delete,
    /// Set (add or replace) an entry.
    Set,
}

/// Parse a neighbor verb word.
pub fn parse_arp_verb(word: &str) -> Result<ArpVerb, NetconfigError> {
    match word {
        "-a" | "show" => Ok(ArpVerb::Show),
        "-d" | "delete" => Ok(ArpVerb::Delete),
        "-s" | "set" => Ok(ArpVerb::Set),
        "add" => Ok(ArpVerb::Add),
        _ => Err(NetconfigError::InvalidArgument),
    }
}

/// One neighbor entry (network address plus link address).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArpEntry<'a> {
    /// Network address in dotted decimal.
    pub network: &'a str,
    /// Link address in colon hexadecimal (`aa:bb:cc:dd:ee:ff`).
    pub link: &'a str,
    /// Interface name.
    pub interface: &'a str,
    /// True when the entry is permanent (never expires).
    pub permanent: bool,
}

/// Check that a link address holds six colon separated byte pairs.
pub fn check_link_address(text: &str) -> Result<(), NetconfigError> {
    let bytes = text.as_bytes();
    if bytes.len() != 17 {
        return Err(NetconfigError::InvalidArgument);
    }
    for (index, byte) in bytes.iter().enumerate() {
        if index % 3 == 2 {
            if *byte != b':' {
                return Err(NetconfigError::InvalidArgument);
            }
        } else if !byte.is_ascii_hexdigit() {
            return Err(NetconfigError::InvalidArgument);
        }
    }
    Ok(())
}

/// Check that a dotted decimal address holds four dot separated parts.
pub fn check_dotted_decimal(text: &str) -> Result<(), NetconfigError> {
    if text.is_empty() {
        return Err(NetconfigError::InvalidArgument);
    }
    let mut parts = 0;
    let mut digits = 0;
    let mut value: u32 = 0;
    for byte in text.bytes() {
        if byte == b'.' {
            if digits == 0 || value > 255 {
                return Err(NetconfigError::InvalidArgument);
            }
            parts += 1;
            digits = 0;
            value = 0;
        } else if byte.is_ascii_digit() {
            value = value * 10 + (byte - b'0') as u32;
            digits += 1;
            if digits > 3 || value > 255 {
                return Err(NetconfigError::InvalidArgument);
            }
        } else {
            return Err(NetconfigError::InvalidArgument);
        }
    }
    if digits == 0 || parts != 3 {
        return Err(NetconfigError::InvalidArgument);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verbs_parse() {
        assert_eq!(parse_arp_verb("-a"), Ok(ArpVerb::Show));
        assert_eq!(parse_arp_verb("show"), Ok(ArpVerb::Show));
        assert_eq!(parse_arp_verb("-d"), Ok(ArpVerb::Delete));
        assert_eq!(parse_arp_verb("-s"), Ok(ArpVerb::Set));
        assert_eq!(
            parse_arp_verb("explode"),
            Err(NetconfigError::InvalidArgument)
        );
    }

    #[test]
    fn test_link_address_accepted() {
        assert_eq!(check_link_address("aa:bb:cc:dd:ee:ff"), Ok(()));
        assert_eq!(check_link_address("AA:BB:CC:DD:EE:FF"), Ok(()));
    }

    #[test]
    fn test_link_address_rejected() {
        assert_eq!(
            check_link_address("aa:bb:cc:dd:ee"),
            Err(NetconfigError::InvalidArgument)
        );
        assert_eq!(
            check_link_address("aa-bb-cc-dd-ee-ff"),
            Err(NetconfigError::InvalidArgument)
        );
        assert_eq!(
            check_link_address("gg:bb:cc:dd:ee:ff"),
            Err(NetconfigError::InvalidArgument)
        );
    }

    #[test]
    fn test_dotted_decimal_accepted() {
        assert_eq!(check_dotted_decimal("192.168.1.1"), Ok(()));
        assert_eq!(check_dotted_decimal("10.0.0.1"), Ok(()));
    }

    #[test]
    fn test_dotted_decimal_rejected() {
        assert_eq!(
            check_dotted_decimal("999.1.1.1"),
            Err(NetconfigError::InvalidArgument)
        );
        assert_eq!(
            check_dotted_decimal("192.168.1"),
            Err(NetconfigError::InvalidArgument)
        );
        assert_eq!(
            check_dotted_decimal("192.168.1.1.1"),
            Err(NetconfigError::InvalidArgument)
        );
        assert_eq!(
            check_dotted_decimal(""),
            Err(NetconfigError::InvalidArgument)
        );
    }
}
