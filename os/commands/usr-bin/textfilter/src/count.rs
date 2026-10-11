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
    /// Multibyte characters seen (`-m`): bytes that are not a UTF-8
    /// continuation byte, i.e. one per scalar under any UTF-8 locale.
    pub chars: u64,
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
            self.bytes = self
                .bytes
                .checked_add(1)
                .ok_or(TextError::InvalidArgument)?;
            // A character begins at any byte that is not a continuation
            // (`10xxxxxx`); ASCII and every lead byte count once.
            if byte & 0xC0 != 0x80 {
                self.chars = self
                    .chars
                    .checked_add(1)
                    .ok_or(TextError::InvalidArgument)?;
            }
            if *byte == b'\n' {
                self.lines = self
                    .lines
                    .checked_add(1)
                    .ok_or(TextError::InvalidArgument)?;
                if self.current > self.longest {
                    self.longest = self.current;
                }
                self.current = 0;
                self.in_word = false;
            } else {
                self.current = self
                    .current
                    .checked_add(1)
                    .ok_or(TextError::InvalidArgument)?;
                if is_blank(*byte) {
                    self.in_word = false;
                } else if !self.in_word {
                    self.in_word = true;
                    self.words = self
                        .words
                        .checked_add(1)
                        .ok_or(TextError::InvalidArgument)?;
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

/// Which single count fills `wc.c`'s shared "character" output column. `wc` has
/// one slot there, filled by bytes (`-c`) or multibyte characters (`-m`), never
/// both; the two flags are mutually exclusive in the C option parser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CharField {
    /// Neither `-c` nor `-m`: the column is not printed.
    Off,
    /// `-c`: print the byte count.
    Bytes,
    /// `-m`: print the character count.
    Chars,
}

/// Width of each count column, the `7` in C's `WCFMT " %7llu"`.
const COUNT_FIELD_WIDTH: usize = 7;

/// Append one right-justified count column (a leading blank then the value in
/// [`COUNT_FIELD_WIDTH`] columns, blank-padded; a wider value simply overflows).
fn put_count(out: &mut [u8], start: usize, value: u64) -> Result<usize, TextError> {
    let mut digits = [0u8; 20];
    let mut len = 0usize;
    let mut v = value;
    if v == 0 {
        digits[0] = b'0';
        len = 1;
    } else {
        while v > 0 {
            digits[len] = b'0' + (v % 10) as u8;
            v /= 10;
            len += 1;
        }
    }
    let pad = COUNT_FIELD_WIDTH.saturating_sub(len);
    let needed = start + 1 + pad + len;
    if out.len() < needed {
        return Err(TextError::InvalidArgument);
    }
    let mut i = start;
    out[i] = b' ';
    i += 1;
    for _ in 0..pad {
        out[i] = b' ';
        i += 1;
    }
    let mut d = len;
    while d > 0 {
        d -= 1;
        out[i] = digits[d];
        i += 1;
    }
    Ok(i)
}

/// Format one `wc` output line into `out`, mirroring `wc.c` `print_counts`: each
/// requested count as ` %7llu` in the order newlines, words, the byte-or-char
/// column, longest; then ` <name>` and a newline for a file operand or total,
/// or just a newline for the nameless standard-input form. Returns the byte
/// count written or an error when `out` is too small.
#[allow(clippy::too_many_arguments)]
pub fn format_counts(
    out: &mut [u8],
    show_lines: bool,
    show_words: bool,
    char_field: CharField,
    show_longest: bool,
    counts: &Counter,
    name: Option<&[u8]>,
) -> Result<usize, TextError> {
    let mut n = 0usize;
    if show_lines {
        n = put_count(out, n, counts.lines)?;
    }
    if show_words {
        n = put_count(out, n, counts.words)?;
    }
    match char_field {
        CharField::Off => {}
        CharField::Bytes => n = put_count(out, n, counts.bytes)?,
        CharField::Chars => n = put_count(out, n, counts.chars)?,
    }
    if show_longest {
        n = put_count(out, n, counts.longest)?;
    }
    if let Some(name) = name {
        let needed = n + 1 + name.len() + 1;
        if out.len() < needed {
            return Err(TextError::InvalidArgument);
        }
        out[n] = b' ';
        n += 1;
        out[n..n + name.len()].copy_from_slice(name);
        n += name.len();
    }
    if out.len() < n + 1 {
        return Err(TextError::InvalidArgument);
    }
    out[n] = b'\n';
    Ok(n + 1)
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
        assert_eq!(
            (done.lines, done.words, done.bytes, done.longest),
            (0, 1, 3, 3)
        );
    }

    #[test]
    fn test_empty_input() {
        let done = Counter::new().finish();
        assert_eq!((done.lines, done.words, done.bytes), (0, 0, 0));
    }

    #[test]
    fn test_multibyte_chars_count_scalars_not_bytes() {
        // "h\u{e9}llo" is 6 bytes but 5 code points; a 3-byte CJK char is one.
        let mut counter = Counter::new();
        counter.add("héllo".as_bytes()).unwrap();
        let done = counter.finish();
        assert_eq!((done.bytes, done.chars), (6, 5));
        let mut cjk = Counter::new();
        cjk.add("漢字".as_bytes()).unwrap();
        let done = cjk.finish();
        assert_eq!((done.bytes, done.chars), (6, 2));
    }

    fn fmt(counts: &Counter, show: (bool, bool, CharField, bool), name: Option<&[u8]>) -> String {
        let mut out = [0u8; 128];
        let n = format_counts(&mut out, show.0, show.1, show.2, show.3, counts, name).unwrap();
        String::from_utf8_lossy(&out[..n]).into_owned()
    }

    #[test]
    fn test_format_default_three_columns_with_width() {
        // Default newlines, words, bytes; each " %7llu" => a blank + 7-wide field.
        let mut c = Counter::new();
        c.add(b"a bb\nccc\n").unwrap(); // 2 lines, 3 words, 9 bytes
        let c = c.finish();
        assert_eq!(
            fmt(&c, (true, true, CharField::Bytes, false), None),
            "       2       3       9\n"
        );
    }

    #[test]
    fn test_format_name_column_appended() {
        let mut c = Counter::new();
        c.add(b"x y\n").unwrap(); // 1 line, 2 words, 4 bytes
        let c = c.finish();
        assert_eq!(
            fmt(&c, (true, true, CharField::Bytes, false), Some(b"f.txt")),
            "       1       2       4 f.txt\n"
        );
    }

    #[test]
    fn test_format_char_field_selection() {
        // -m uses the character count, -c the byte count; a single column.
        let mut c = Counter::new();
        c.add("héllo".as_bytes()).unwrap(); // 6 bytes, 5 chars, 0 lines, 1 word
        let c = c.finish();
        assert_eq!(
            fmt(&c, (false, false, CharField::Chars, false), None),
            "       5\n"
        );
        assert_eq!(
            fmt(&c, (false, false, CharField::Bytes, false), None),
            "       6\n"
        );
    }

    #[test]
    fn test_format_wide_value_overflows_field() {
        // A 12345678 count (> 7 digits) just grows past the field width.
        let mut big = Counter::new();
        big.lines = 12_345_678;
        assert_eq!(
            fmt(&big, (true, false, CharField::Off, false), None),
            " 12345678\n"
        );
    }

    #[test]
    fn test_format_small_buffer_errors() {
        let c = Counter::new().finish();
        let mut tiny = [0u8; 4];
        assert!(format_counts(&mut tiny, true, true, CharField::Bytes, false, &c, None).is_err());
    }
}
