//! Output staging buffer with read-offset skipping (`buf.c`).
//!
//! Every content generator writes the whole file from the start; the
//! buffer drops the first `skip` bytes (the read offset) and keeps at most
//! the requested length. A one-byte margin stays reserved because the C
//! formatting primitive needs room for its terminator (`buf.c:11-15`);
//! this buffer keeps the same margin so capacities behave identically.

extern crate alloc;

use alloc::vec::Vec;

/// Staging buffer for one read call (`buf.c` state plus output).
pub struct ProcBuf {
    /// Bytes kept for the caller (at most the requested length).
    kept: Vec<u8>,
    /// Read offset still to skip.
    skip: u64,
    /// Requested length still free.
    left: usize,
}

impl ProcBuf {
    /// Start a fresh buffer for a read of `length` bytes at `offset`.
    pub fn new(length: usize, offset: u64, capacity: usize) -> Self {
        Self {
            kept: Vec::new(),
            skip: offset,
            left: length.min(capacity.saturating_sub(1)),
        }
    }

    /// Append formatted text, applying the skip window (`buf_printf`).
    pub fn push_str(&mut self, text: &str) {
        self.push_bytes(text.as_bytes());
    }

    /// Append raw bytes (`buf_append`).
    pub fn push_bytes(&mut self, data: &[u8]) {
        if self.left == 0 {
            return;
        }
        let mut data = data;
        if self.skip > 0 {
            if self.skip >= data.len() as u64 {
                self.skip -= data.len() as u64;
                return;
            }
            data = &data[self.skip as usize..];
            self.skip = 0;
        }
        let take = data.len().min(self.left);
        self.kept.extend_from_slice(&data[..take]);
        self.left -= take;
    }

    /// Append a signed decimal integer.
    pub fn push_i64(&mut self, value: i64) {
        let mut digits = [0u8; 20];
        let text = format_i64(value, &mut digits);
        self.push_str(text);
    }

    /// Append an unsigned decimal integer.
    pub fn push_u64(&mut self, value: u64) {
        let mut digits = [0u8; 20];
        let text = format_u64(value, &mut digits);
        self.push_str(text);
    }

    /// Bytes produced for the caller (`buf_result`).
    pub fn result(&self) -> &[u8] {
        &self.kept
    }

    /// Bytes produced, owned.
    pub fn into_bytes(self) -> Vec<u8> {
        self.kept
    }
}

/// Decimal rendering without formatting machinery (no standard library).
fn format_u64(mut value: u64, digits: &mut [u8; 20]) -> &str {
    let mut end = 20;
    if value == 0 {
        end -= 1;
        digits[end] = b'0';
    } else {
        while value > 0 {
            end -= 1;
            digits[end] = b'0' + (value % 10) as u8;
            value /= 10;
        }
    }
    core::str::from_utf8(&digits[end..]).unwrap_or("?")
}

/// Signed decimal rendering.
fn format_i64(value: i64, digits: &mut [u8; 20]) -> &str {
    if value < 0 {
        let mut end = 20;
        let mut rest = value.unsigned_abs();
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
        end -= 1;
        digits[end] = b'-';
        core::str::from_utf8(&digits[end..]).unwrap_or("?")
    } else {
        format_u64(value as u64, digits)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn test_full_output_without_offset() {
        let mut buf = ProcBuf::new(64, 0, 4097);
        buf.push_str("hello");
        buf.push_bytes(b" world");
        assert_eq!(buf.result(), b"hello world");
    }

    #[test]
    fn test_offset_skips_head() {
        let mut buf = ProcBuf::new(64, 6, 4097);
        buf.push_str("hello world");
        assert_eq!(buf.result(), b"world");
    }

    #[test]
    fn test_offset_across_pushes() {
        let mut buf = ProcBuf::new(64, 8, 4097);
        buf.push_str("hello ");
        buf.push_str("world!");
        assert_eq!(buf.result(), b"rld!");
    }

    #[test]
    fn test_length_truncates_tail() {
        let mut buf = ProcBuf::new(5, 0, 4097);
        buf.push_str("hello world");
        assert_eq!(buf.result(), b"hello");
    }

    #[test]
    fn test_integers_render_decimal() {
        let mut buf = ProcBuf::new(64, 0, 4097);
        buf.push_i64(-42);
        buf.push_str(" ");
        buf.push_u64(100);
        assert_eq!(buf.result(), b"-42 100");
    }

    #[test]
    fn test_zero_renders() {
        let mut buf = ProcBuf::new(64, 0, 4097);
        buf.push_u64(0);
        assert_eq!(buf.result(), b"0");
        let _ = vec![0u8; 1];
    }
}
