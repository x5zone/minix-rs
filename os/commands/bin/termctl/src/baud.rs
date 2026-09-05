//! Speed number/baud tables in both directions.
//!
//! Ground truth: `stty` sets speeds through `cfsetospeed` (`stty.c:137`,
//! `key.c:258`) and reports them as `speed %d baud` (`print.c:71-73`).
//! The C library maps speed constants (`B9600` and friends) to baud
//! numbers; this module maps baud numbers both ways over the standard
//! serial speed set (0, 50, 75, 110, 134, 150, 200, 300, 600, 1200, 1800,
//! 2400, 4800, 9600, 19200, 38400, 57600, 115200, 230400). Speed 0 means
//! "hang up" (drop the line), not "stop", a classic pitfall documented by
//! a dedicated test.

/// All speeds this module recognises, in increasing order.
pub const SPEEDS: [u32; 19] = [
    0, 50, 75, 110, 134, 150, 200, 300, 600, 1200, 1800, 2400, 4800, 9600, 19200, 38400, 57600,
    115200, 230400,
];

/// True when `baud` is a recognised speed.
pub fn valid_speed(baud: u32) -> bool {
    SPEEDS.contains(&baud)
}

/// Parse a speed word (`9600`, `115200`) into its baud number.
pub fn parse_speed(text: &str) -> Result<u32, crate::TermError> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err(crate::TermError::InvalidArgument);
    }
    let mut value: u32 = 0;
    for byte in text.bytes() {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add((byte - b'0') as u32))
            .ok_or(crate::TermError::InvalidArgument)?;
    }
    if valid_speed(value) {
        Ok(value)
    } else {
        Err(crate::TermError::InvalidArgument)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_common_speeds_parse() {
        assert_eq!(parse_speed("9600"), Ok(9600));
        assert_eq!(parse_speed("115200"), Ok(115200));
        assert_eq!(parse_speed("0"), Ok(0));
    }

    #[test]
    fn test_zero_means_hangup_not_stop() {
        // Speed 0 is valid input (hang up the line); it is a decision for
        // the execution layer, never a parse error.
        assert!(valid_speed(0));
    }

    #[test]
    fn test_unknown_speeds_rejected() {
        assert_eq!(parse_speed("9610"), Err(crate::TermError::InvalidArgument));
        assert_eq!(parse_speed("abc"), Err(crate::TermError::InvalidArgument));
        assert_eq!(parse_speed(""), Err(crate::TermError::InvalidArgument));
    }
}
