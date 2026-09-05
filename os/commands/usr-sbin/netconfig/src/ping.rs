//! Echo probing arithmetic and options.
//!
//! Ground truth: `minix3/sbin/ping/ping.c`. The checksum function `in_cksum`
//! sits at line 1266 (ones-complement sum over sixteen bit words, trailing odd
//! byte padded, folds carried). Requests are built with `ICMP_ECHO` (near
//! line 897) and stamped with the checksum (near line 909); replies match
//! `ICMP_ECHOREPLY` (near line 1029). The packet counter `npackets` bounds the
//! run. The execution layer owns the raw socket and the clock; this module
//! owns the arithmetic and the option words.

use crate::NetconfigError;

/// Internet Control Message Protocol message kinds used by probing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EchoKind {
    /// Echo request (sent by the prober).
    Request,
    /// Echo reply (returned by the target).
    Reply,
}

/// Message kind number on the wire.
pub fn echo_kind_number(kind: EchoKind) -> u8 {
    match kind {
        EchoKind::Request => 8,
        EchoKind::Reply => 0,
    }
}

/// Ones-complement checksum over `words` plus an optional trailing odd byte.
///
/// This is the `in_cksum` algorithm from `ping.c:1266`: accumulate sixteen bit
/// words in a wide register, add the trailing odd byte shifted up when the
/// length is odd, fold the carries back into sixteen bits, then complement.
/// The checksum field itself must read zero while computing.
pub fn internet_checksum(words: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    let mut index = 0;
    while index + 1 < words.len() {
        let word = ((words[index] as u32) << 8) | words[index + 1] as u32;
        sum = sum.wrapping_add(word);
        index += 2;
    }
    if index < words.len() {
        sum = sum.wrapping_add((words[index] as u32) << 8);
    }
    while (sum >> 16) != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    !(sum as u16)
}

/// Probe options (packet count, wait between packets, payload size).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PingOptions {
    /// Packets to send (`None` means run until interrupted).
    pub count: Option<u32>,
    /// Milliseconds between packets.
    pub interval_ms: u32,
    /// Payload bytes per packet.
    pub size: u16,
}

impl Default for PingOptions {
    fn default() -> Self {
        PingOptions {
            count: None,
            interval_ms: 1000,
            size: 56,
        }
    }
}

/// Parse a decimal packet count (must be strictly positive).
pub fn parse_ping_count(word: &str) -> Result<u32, NetconfigError> {
    parse_decimal_u32(word).and_then(|value| {
        if value == 0 {
            Err(NetconfigError::InvalidArgument)
        } else {
            Ok(value)
        }
    })
}

/// Parse a decimal interval in seconds (whole seconds, at least one).
pub fn parse_ping_interval(word: &str) -> Result<u32, NetconfigError> {
    parse_decimal_u32(word).and_then(|value| {
        if value == 0 {
            Err(NetconfigError::InvalidArgument)
        } else {
            Ok(value.saturating_mul(1000))
        }
    })
}

fn parse_decimal_u32(word: &str) -> Result<u32, NetconfigError> {
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
    Ok(value)
}

/// Round trip summary (sent, received, minimum, average, maximum).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PingSummary {
    /// Packets transmitted.
    pub sent: u32,
    /// Packets answered.
    pub received: u32,
    /// Fastest round trip in milliseconds.
    pub min_ms: u32,
    /// Slowest round trip in milliseconds.
    pub max_ms: u32,
    /// Sum of round trips in milliseconds (divide by received for the mean).
    pub total_ms: u64,
}

/// Loss percentage (sent minus received over sent, zero when none sent).
pub fn loss_percent(summary: PingSummary) -> u32 {
    if summary.sent == 0 {
        return 0;
    }
    ((summary.sent - summary.received) as u64 * 100 / summary.sent as u64) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kind_numbers() {
        assert_eq!(echo_kind_number(EchoKind::Request), 8);
        assert_eq!(echo_kind_number(EchoKind::Reply), 0);
    }

    #[test]
    fn test_checksum_of_empty_is_all_ones() {
        assert_eq!(internet_checksum(&[]), 0xFFFF);
    }

    #[test]
    fn test_checksum_round_trip() {
        // A header with the checksum field filled must verify to zero.
        let mut header = [0x08u8, 0x00, 0x00, 0x00, 0x12, 0x34, 0x00, 0x01];
        let sum = internet_checksum(&header);
        header[2] = (sum >> 8) as u8;
        header[3] = (sum & 0xFF) as u8;
        assert_eq!(internet_checksum(&header), 0);
    }

    #[test]
    fn test_checksum_covers_odd_tail() {
        let even = internet_checksum(&[0x01, 0x02, 0x03, 0x04]);
        let odd = internet_checksum(&[0x01, 0x02, 0x03]);
        assert_ne!(even, odd);
    }

    #[test]
    fn test_count_parses() {
        assert_eq!(parse_ping_count("4"), Ok(4));
        assert_eq!(parse_ping_count("0"), Err(NetconfigError::InvalidArgument));
        assert_eq!(parse_ping_count(""), Err(NetconfigError::InvalidArgument));
        assert_eq!(parse_ping_count("4x"), Err(NetconfigError::InvalidArgument));
    }

    #[test]
    fn test_interval_converts_to_milliseconds() {
        assert_eq!(parse_ping_interval("2"), Ok(2000));
        assert_eq!(
            parse_ping_interval("0"),
            Err(NetconfigError::InvalidArgument)
        );
    }

    #[test]
    fn test_loss_percent() {
        let summary = PingSummary {
            sent: 4,
            received: 3,
            min_ms: 1,
            max_ms: 5,
            total_ms: 9,
        };
        assert_eq!(loss_percent(summary), 25);
        assert_eq!(loss_percent(PingSummary::default()), 0);
    }
}
