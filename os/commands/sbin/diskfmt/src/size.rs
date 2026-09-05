//! Human size parsing behind the `newfs`/`mkfs` size options.
//!
//! Sizes arrive as decimal numbers with an optional unit suffix: bare
//! digits are bytes, `K` multiplies by 1024, `M` by 1024 squared, `G` by
//! 1024 cubed (binary units, matching the tools). Overflow is an error,
//! never a wraparound (a wrapped size would format the wrong volume — the
//! most destructive silent failure in this domain).

use crate::DiskError;

/// Parse `10M`, `512K`, `1G`, or a bare byte count into bytes.
pub fn parse_size(text: &str) -> Result<u64, DiskError> {
    if text.is_empty() {
        return Err(DiskError::InvalidArgument);
    }
    let (digits, factor) = match text.as_bytes().last() {
        Some(b'K') | Some(b'k') => (&text[..text.len() - 1], 1024u64),
        Some(b'M') | Some(b'm') => (&text[..text.len() - 1], 1024u64 * 1024),
        Some(b'G') | Some(b'g') => (&text[..text.len() - 1], 1024u64 * 1024 * 1024),
        _ => (text, 1),
    };
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(DiskError::InvalidArgument);
    }
    let mut value: u64 = 0;
    for byte in digits.bytes() {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add((byte - b'0') as u64))
            .ok_or(DiskError::InvalidArgument)?;
    }
    value.checked_mul(factor).ok_or(DiskError::InvalidArgument)
}

/// Whole sectors covered by `bytes` at `sector_size` bytes per sector,
/// rounding up (a partial tail sector still occupies its sector).
pub fn sectors_for(bytes: u64, sector_size: u64) -> Result<u64, DiskError> {
    if sector_size == 0 {
        return Err(DiskError::InvalidArgument);
    }
    Ok(bytes / sector_size + u64::from(!bytes.is_multiple_of(sector_size)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_units() {
        assert_eq!(parse_size("512"), Ok(512));
        assert_eq!(parse_size("10K"), Ok(10 * 1024));
        assert_eq!(parse_size("10M"), Ok(10 * 1024 * 1024));
        assert_eq!(parse_size("1G"), Ok(1024 * 1024 * 1024));
        assert_eq!(parse_size("1k"), Ok(1024));
    }

    #[test]
    fn test_overflow_rejected() {
        assert_eq!(
            parse_size("18446744073709551615G"),
            Err(DiskError::InvalidArgument)
        );
        assert_eq!(parse_size(""), Err(DiskError::InvalidArgument));
        assert_eq!(parse_size("M"), Err(DiskError::InvalidArgument));
        assert_eq!(parse_size("12X"), Err(DiskError::InvalidArgument));
    }

    #[test]
    fn test_sector_rounding() {
        assert_eq!(sectors_for(512, 512), Ok(1));
        assert_eq!(sectors_for(513, 512), Ok(2));
        assert_eq!(sectors_for(0, 512), Ok(0));
        assert_eq!(sectors_for(100, 0), Err(DiskError::InvalidArgument));
    }
}
