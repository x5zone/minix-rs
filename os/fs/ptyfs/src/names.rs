//! Decimal names for slave nodes (`make_name` and `parse_name`,
//! `ptyfs.c:54-100`).
//!
//! A slave with index `i` is named by the decimal text of `i`, and a name
//! is valid exactly when it could have been produced that way: digits
//! only, no leading zeroes (except the single digit zero itself), and no
//! arithmetic overflow. The empty name parses as index zero in C, but
//! lookup never passes empty names down (it asserts non-empty and
//! handles dot first), so this module rejects empty input as absent.

/// Render an index as its decimal name (`make_name`, `ptyfs.c:54-66`).
/// The caller supplies the buffer bound; a rendering that does not fit
/// reports overlong instead of truncating.
pub fn render(index: u32) -> Result<DecimalName, super::PtyError> {
    let mut digits = [0u8; 12];
    let mut end = 12;
    let mut rest = index;
    if rest == 0 {
        end -= 1;
        digits[end] = b'0';
    } else {
        while rest > 0 {
            end -= 1;
            digits[end] = b'0' + (rest % 10) as u8;
            rest /= 10;
        }
    }
    let text = core::str::from_utf8(&digits[end..]).map_err(|_| super::PtyError::NameTooLong)?;
    if text.len() >= 12 {
        return Err(super::PtyError::NameTooLong);
    }
    Ok(DecimalName { text: digits, length: text.len() })
}

/// A rendered decimal name plus its text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecimalName {
    /// Inline digit storage (twelve bytes cover any 32-bit value).
    text: [u8; 12],
    /// Bytes in use at the end of `text`.
    length: usize,
}

impl DecimalName {
    /// Name bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.text[12 - self.length..]
    }

    /// Owned bytes.
    pub fn into_bytes(self) -> alloc::vec::Vec<u8> {
        self.as_bytes().to_vec()
    }
}

/// Parse a user-supplied name (`parse_name`, `ptyfs.c:75-100`).
/// Returns the index on success, or absent when the name is not a
/// canonical decimal rendering: empty, non-digit, leading zero, or
/// overflowing 32-bit arithmetic.
pub fn parse(name: &[u8]) -> Option<u32> {
    if name.is_empty() {
        return None;
    }
    let mut index: u32 = 0;
    for (position, byte) in name.iter().enumerate() {
        if !byte.is_ascii_digit() {
            return None;
        }
        // No leading zeroes past the first digit.
        if position != 0 && index == 0 {
            return None;
        }
        let digit = (byte - b'0') as u32;
        index = index.checked_mul(10)?.checked_add(digit)?;
    }
    Some(index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_zero_and_values() {
        assert_eq!(render(0).unwrap().as_bytes(), b"0");
        assert_eq!(render(7).unwrap().as_bytes(), b"7");
        assert_eq!(render(31).unwrap().as_bytes(), b"31");
        assert_eq!(render(u32::MAX).unwrap().as_bytes(), b"4294967295");
    }

    #[test]
    fn test_parse_round_trip() {
        for index in [0u32, 1, 9, 10, 31, 100, 4294967295] {
            let name = render(index).unwrap();
            assert_eq!(parse(name.as_bytes()), Some(index));
        }
    }

    #[test]
    fn test_parse_rejects() {
        assert_eq!(parse(b""), None);
        assert_eq!(parse(b"abc"), None);
        assert_eq!(parse(b"1a"), None);
        // Leading zeroes are not canonical.
        assert_eq!(parse(b"00"), None);
        assert_eq!(parse(b"01"), None);
        // Past 32-bit range overflows.
        assert_eq!(parse(b"4294967296"), None);
        assert_eq!(parse(b"99999999999999999999"), None);
    }
}
