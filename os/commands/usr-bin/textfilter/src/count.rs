//! The `wc` counters: lines, words, bytes, longest line.
//!
//! Ground truth: `minix3/usr.bin/wc/wc.c` (354 lines). Counting rules:
//! a line ends at a newline byte; a word is a maximal run of non blank
//! bytes (blank = space, tab, newline, carriage return, vertical tab, form
//! feed); every byte counts including the newline; the longest line length
//! excludes its newline. The counter feeds on arbitrary byte chunks, so
//! the caller never holds a whole file to count it.

use crate::TextError;

/// Running `wc` totals over a stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Counter {
    /// Newline bytes seen.
    pub lines: u64,
    /// Maximal non blank runs seen.
    pub words: u64,
    /// All bytes seen.
    pub bytes: u64,
    /// Longest line so far, excluding its newline.
    pub longest: u64,
    in_word: bool,
    current: u64,
}

impl Counter {
    /// An empty counter.
    pub fn new() -> Self {
        Counter::default()
    }

    /// Feed one chunk; chunks may split anywhere, even mid word.
    pub fn add(&mut self, chunk: &[u8]) -> Result<(), TextError> {
        for byte in chunk {
            self.bytes = self.bytes.checked_add(1).ok_or(TextError::InvalidArgument)?;
            if *byte == b'\n' {
                self.lines = self.lines.checked_add(1).ok_or(TextError::InvalidArgument)?;
                if self.current > self.longest {
                    self.longest = self.current;
                }
                self.current = 0;
                self.in_word = false;
            } else {
                self.current = self.current.checked_add(1).ok_or(TextError::InvalidArgument)?;
                if is_blank(*byte) {
                    self.in_word = false;
                } else if !self.in_word {
                    self.in_word = true;
                    self.words = self.words.checked_add(1).ok_or(TextError::InvalidArgument)?;
                }
            }
        }
        Ok(())
    }

    /// Totals including a final unterminated line in the longest measure.
    pub fn finish(mut self) -> Self {
        if self.current > self.longest {
            self.longest = self.current;
        }
        self.in_word = false;
        self
    }
}

fn is_blank(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | 0x0B | 0x0C)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_counts() {
        let mut counter = Counter::new();
        counter.add(b"hello world\nfoo\n").unwrap();
        let done = counter.finish();
        assert_eq!(done.lines, 2);
        assert_eq!(done.words, 3);
        assert_eq!(done.bytes, 16);
        assert_eq!(done.longest, 11);
    }

    #[test]
    fn test_chunk_split_mid_word() {
        let mut whole = Counter::new();
        whole.add(b"ab cd\n").unwrap();
        let mut split = Counter::new();
        split.add(b"ab ").unwrap();
        split.add(b"cd\n").unwrap();
        assert_eq!(whole.finish(), split.finish());
    }

    #[test]
    fn test_unterminated_last_line() {
        let mut counter = Counter::new();
        counter.add(b"abc").unwrap();
        let done = counter.finish();
        assert_eq!((done.lines, done.words, done.bytes, done.longest), (0, 1, 3, 3));
    }

    #[test]
    fn test_empty_input() {
        let done = Counter::new().finish();
        assert_eq!((done.lines, done.words, done.bytes), (0, 0, 0));
    }
}
