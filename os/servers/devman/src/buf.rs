//! Output buffer with skip semantics (doc 06-event-buf).
//!
//! C: `minix3/minix/servers/devman/buf.c` (129 lines) —
//! `buf_init` / `buf_printf` / `buf_append` / `buf_result` over four
//! statics (`buf`, `left`, `used`, `skip`).
//!
//! The struct replaces the globals (multi-instance safe; C's single
//! static buffer is a single-threaded accident, not a requirement).
//! `skip` is the `off_t` read offset: the first `skip` produced bytes
//! are discarded, then up to `left` bytes are kept — this is how
//! offset reads and EOF-consumption fall out of one mechanism (06 §2.2).

use alloc::vec::Vec;
use minix_types::Errno;

use crate::hooks::BUF_SIZE;

/// Owned output buffer, `BUF_SIZE` bytes.
pub struct Buf {
    data: Vec<u8>,
    left: usize,
    used: usize,
    skip: usize,
}

impl Buf {
    /// Allocate the `BUF_SIZE` working area.
    /// (C's area is the vtreefs static; fallible here → `ENOMEM`.)
    pub fn new() -> Result<Self, Errno> {
        let mut data = Vec::new();
        data.try_reserve(BUF_SIZE).map_err(|_| Errno::ENOMEM)?;
        data.resize(BUF_SIZE, 0);
        Ok(Buf {
            data,
            left: 0,
            used: 0,
            skip: 0,
        })
    }

    /// C: `buf_init(ptr, len, start)` (buf.c:15-28) — fresh cursors for
    /// one read: keep at most `min(len, BUF_SIZE - 1)` bytes past `start`.
    /// (The `-1` is the vsnprintf NUL bay, buf.c:18-21.)
    pub fn init(&mut self, len: usize, start: usize) {
        self.skip = start;
        self.left = len.min(BUF_SIZE - 1);
        self.used = 0;
    }

    /// Append rendered bytes honoring skip-then-cap.
    /// Shared tail of `printf`/`append` (buf.c:61-84 + :98-116 unified:
    /// skip-consume → cap-by-left → copy → advance).
    fn emit(&mut self, mut bytes: &[u8]) {
        if self.left == 0 {
            return;
        }
        if self.skip > 0 {
            if self.skip >= bytes.len() {
                self.skip -= bytes.len();
                return;
            }
            bytes = &bytes[self.skip..];
            self.skip = 0;
        }
        // C: `assert(skip == 0)` past this point (buf.c:76).
        debug_assert_eq!(self.skip, 0);
        let take = bytes.len().min(self.left);
        self.data[self.used..self.used + take].copy_from_slice(&bytes[..take]);
        self.used += take;
        self.left -= take;
    }

    /// C: `buf_printf(fmt, …)` (buf.c:33-85) for the only formats in the
    /// tree — `%s` and `%%` (callers: `"%s"` in `devman_event_read`,
    /// `"%s\n"` in `devman_static_info_read`). Anything else is copied
    /// literally (documented subset, not silent truncation: unknown
    /// conversions have no writer in-tree, and tests lock `%s`/`%%`).
    /// Inputs are ≤ `DEVMAN_STRING_LEN + 1` by construction (03/06), so
    /// render-then-emit is observably identical to C's capped-vsnprintf
    /// path (06 §2.2 bound argument).
    pub fn printf(&mut self, fmt: &str, arg: &str) {
        if self.left == 0 {
            return;
        }
        let mut out = Vec::new();
        let mut chars = fmt.chars();
        while let Some(c) = chars.next() {
            if c != '%' {
                out.push(c as u8);
                continue;
            }
            match chars.next() {
                Some('s') => out.extend_from_slice(arg.as_bytes()),
                Some('%') => out.push(b'%'),
                // Documented subset: unknown conversions pass through
                // literally (no writer emits them; see module docs).
                Some(o) => {
                    out.push(b'%');
                    let mut b = [0u8; 4];
                    out.extend_from_slice(o.encode_utf8(&mut b).as_bytes());
                }
                None => out.push(b'%'),
            }
        }
        self.emit(&out);
    }

    /// C: `buf_append(data, len)` (buf.c:90-117) — raw bytes through the
    /// same skip/cap funnel.
    pub fn append(&mut self, data: &[u8]) {
        self.emit(data);
    }

    /// C: `buf_result()` (buf.c:122-128) — produced bytes, NUL excluded.
    pub fn result(&self) -> &[u8] {
        &self.data[..self.used]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_caps_len() {
        // C: left = MIN(len, BUF_SIZE - 1) (buf.c:26).
        let mut b = Buf::new().unwrap();
        b.init(99999, 0);
        b.append(b"x");
        assert_eq!(b.result().len(), 1);
        let mut b2 = Buf::new().unwrap();
        b2.init(99999, 0);
        b2.append(&alloc::vec![b'y'; BUF_SIZE]);
        assert_eq!(b2.result().len(), BUF_SIZE - 1);
    }

    #[test]
    fn printf_s_and_newline() {
        // The only two formats in-tree: "%s" and "%s\n".
        let mut b = Buf::new().unwrap();
        b.init(64, 0);
        b.printf("%s", "ADD ./devices/usb/ 0x00000001");
        assert_eq!(b.result(), b"ADD ./devices/usb/ 0x00000001");
        let mut b2 = Buf::new().unwrap();
        b2.init(64, 0);
        b2.printf("%s\n", "dev_type=USB");
        assert_eq!(b2.result(), b"dev_type=USB\n");
    }

    #[test]
    fn skip_eats_output_then_eof() {
        // Offset reads: skip discards, exhaustion yields empty (06 §2.4).
        let mut b = Buf::new().unwrap();
        b.init(64, 5);
        b.printf("%s", "abcdefghij");
        assert_eq!(b.result(), b"fghij");
        let mut b2 = Buf::new().unwrap();
        b2.init(64, 10);
        b2.printf("%s", "abcdefghij");
        assert_eq!(b2.result(), b"");
    }

    #[test]
    fn left_zero_short_circuits() {
        // C: `if (left == 0) return` (buf.c:40,95).
        let mut b = Buf::new().unwrap();
        b.init(0, 0);
        b.printf("%s", "x");
        b.append(b"y");
        assert_eq!(b.result(), b"");
    }
}
