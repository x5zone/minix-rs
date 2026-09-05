//! Network interface configuration vocabulary.
//!
//! Ground truth: `minix3/sbin/ifconfig/ifconfig.c`. Interface flags are read
//! with `SIOCGIFFLAGS` (near line 1050) and written with `SIOCSIFFLAGS` (near
//! line 1059); the maximum transmission unit is written with `SIOCSIFMTU`
//! (near line 1173). The `up` word sets the `IFF_UP` flag, the `down` word
//! clears it. The execution layer owns the socket and the control calls; this
//! module owns the words and the numbers.

use crate::NetconfigError;

/// Interface flags carried in an interface request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct InterfaceFlags {
    bits: u16,
}

impl InterfaceFlags {
    const UP: u16 = 1;
    const RUNNING: u16 = 2;
    const PROMISCUOUS: u16 = 4;
    const MULTICAST: u16 = 8;
    const DEBUG: u16 = 16;

    /// Empty flag set (interface administratively down).
    pub fn empty() -> Self {
        InterfaceFlags { bits: 0 }
    }

    /// Administrative state (set by `up` and `down`).
    pub fn is_up(self) -> bool {
        self.bits & Self::UP != 0
    }

    /// Operational state (reported by the driver, never set by hand).
    pub fn is_running(self) -> bool {
        self.bits & Self::RUNNING != 0
    }

    /// Promiscuous reception (set by packet capture tools).
    pub fn is_promiscuous(self) -> bool {
        self.bits & Self::PROMISCUOUS != 0
    }
}

/// Apply one flag word (`up`, `down`, `promisc`, `-promisc`, `debug`,
/// `-debug`, `multicast`) to a flag set.
pub fn apply_flag_word(flags: &mut InterfaceFlags, word: &str) -> Result<(), NetconfigError> {
    match word {
        "up" => {
            flags.bits |= InterfaceFlags::UP;
            Ok(())
        }
        "down" => {
            flags.bits &= !InterfaceFlags::UP;
            Ok(())
        }
        "promisc" => {
            flags.bits |= InterfaceFlags::PROMISCUOUS;
            Ok(())
        }
        "-promisc" => {
            flags.bits &= !InterfaceFlags::PROMISCUOUS;
            Ok(())
        }
        "multicast" => {
            flags.bits |= InterfaceFlags::MULTICAST;
            Ok(())
        }
        "debug" => {
            flags.bits |= InterfaceFlags::DEBUG;
            Ok(())
        }
        "-debug" => {
            flags.bits &= !InterfaceFlags::DEBUG;
            Ok(())
        }
        _ => Err(NetconfigError::InvalidArgument),
    }
}

/// Smallest maximum transmission unit accepted on Ethernet style links.
pub const MINIMUM_MTU: u32 = 68;

/// Largest maximum transmission unit accepted without jumbo frame support.
pub const MAXIMUM_MTU: u32 = 9000;

/// Parse a maximum transmission unit value in decimal.
pub fn parse_mtu(word: &str) -> Result<u32, NetconfigError> {
    if word.is_empty() {
        return Err(NetconfigError::InvalidArgument);
    }
    let mut value: u32 = 0;
    for byte in word.bytes() {
        if !byte.is_ascii_digit() {
            return Err(NetconfigError::InvalidArgument);
        }
        value = value
            .checked_mul(10)
            .and_then(|scaled| scaled.checked_add((byte - b'0') as u32))
            .ok_or(NetconfigError::InvalidArgument)?;
    }
    if !(MINIMUM_MTU..=MAXIMUM_MTU).contains(&value) {
        return Err(NetconfigError::InvalidArgument);
    }
    Ok(value)
}

/// One interface configuration request (name, flags, transmission unit).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InterfaceRequest<'a> {
    /// Interface name (`eth0`, `lo0`, and so on).
    pub name: &'a str,
    /// Desired flag set.
    pub flags: InterfaceFlags,
    /// Desired maximum transmission unit, or `None` when unchanged.
    pub mtu: Option<u32>,
}

/// Build a request from words; at least the interface name is required.
pub fn parse_interface_request<'a>(words: &[&'a str]) -> Result<InterfaceRequest<'a>, NetconfigError> {
    if words.is_empty() || words[0].is_empty() {
        return Err(NetconfigError::InvalidArgument);
    }
    let mut request = InterfaceRequest {
        name: words[0],
        flags: InterfaceFlags::empty(),
        mtu: None,
    };
    let mut index = 1;
    while index < words.len() {
        match words[index] {
            "mtu" => {
                index += 1;
                if index >= words.len() {
                    return Err(NetconfigError::InvalidArgument);
                }
                request.mtu = Some(parse_mtu(words[index])?);
            }
            word => apply_flag_word(&mut request.flags, word)?,
        }
        index += 1;
    }
    Ok(request)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_up_and_down_toggle() {
        let mut flags = InterfaceFlags::empty();
        apply_flag_word(&mut flags, "up").unwrap();
        assert!(flags.is_up());
        apply_flag_word(&mut flags, "down").unwrap();
        assert!(!flags.is_up());
    }

    #[test]
    fn test_promiscuous_toggles() {
        let mut flags = InterfaceFlags::empty();
        apply_flag_word(&mut flags, "promisc").unwrap();
        assert!(flags.is_promiscuous());
        apply_flag_word(&mut flags, "-promisc").unwrap();
        assert!(!flags.is_promiscuous());
    }

    #[test]
    fn test_unknown_word_rejected() {
        let mut flags = InterfaceFlags::empty();
        assert_eq!(
            apply_flag_word(&mut flags, "turbo"),
            Err(NetconfigError::InvalidArgument)
        );
    }

    #[test]
    fn test_mtu_bounds_enforced() {
        assert_eq!(parse_mtu("1500"), Ok(1500));
        assert_eq!(parse_mtu("67"), Err(NetconfigError::InvalidArgument));
        assert_eq!(parse_mtu("9001"), Err(NetconfigError::InvalidArgument));
        assert_eq!(parse_mtu(""), Err(NetconfigError::InvalidArgument));
        assert_eq!(parse_mtu("15oo"), Err(NetconfigError::InvalidArgument));
    }

    #[test]
    fn test_request_parses() {
        let words = ["eth0", "up", "mtu", "1500"];
        let request = parse_interface_request(&words).unwrap();
        assert_eq!(request.name, "eth0");
        assert!(request.flags.is_up());
        assert_eq!(request.mtu, Some(1500));
    }

    #[test]
    fn test_request_needs_name() {
        assert_eq!(
            parse_interface_request(&[]),
            Err(NetconfigError::InvalidArgument)
        );
        assert_eq!(
            parse_interface_request(&[""]),
            Err(NetconfigError::InvalidArgument)
        );
    }

    #[test]
    fn test_request_rejects_dangling_mtu() {
        let words = ["eth0", "mtu"];
        assert_eq!(
            parse_interface_request(&words),
            Err(NetconfigError::InvalidArgument)
        );
    }
}
