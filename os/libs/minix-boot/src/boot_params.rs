//! Boot-parameter normalization for `KernelInfo.param_buf`.
//!
//! The channel is the C-compatible fixed array `char param_buf[1024]`: a
//! sequence of NUL-terminated `key=value` records. This module owns the one
//! normalization rule every boot leg shares, so the UEFI LoadOptions leg and
//! the DTB `/chosen/bootargs` leg cannot drift into two formats.
//!
//! Rules (frozen by the design):
//! - input records are separated by NUL **or** ASCII whitespace (UEFI passes
//!   NUL-terminated records; DTB bootargs is a space-separated string);
//! - every record must be printable ASCII with a non-empty key and a `=`;
//! - `ARCH` is generated from the compile target, `BOARD` only when the leg
//!   has a real value — otherwise the key is omitted, never faked;
//! - external input may not override the generated `ARCH`/`BOARD` keys;
//! - duplicate ordinary keys: the first record wins, later ones are rejected;
//! - capacity is enforced per whole record: a record that does not fit is
//!   dropped together with every later record (no half `key=value`).

/// Capacity of the C-compatible `param_buf` channel (bytes).
pub const PARAM_BUF_CAP: usize = 1024;

/// Result of one normalization pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ParamStats {
    /// Bytes written to the output buffer (always <= [`PARAM_BUF_CAP`]).
    pub bytes: usize,
    /// Malformed, non-ASCII or internal-key records that were skipped.
    pub rejected: usize,
    /// Whole records dropped because the remaining capacity was too small.
    pub dropped: usize,
}

/// Name of the compile-target architecture, used for the generated `ARCH`
/// key. The value is display/diagnostic data: no in-repo consumer reads it
/// (the C-side `env_get_param` users are the only historical readers).
pub const fn arch_name() -> &'static str {
    #[cfg(target_arch = "x86_64")]
    {
        "x86_64"
    }
    #[cfg(target_arch = "aarch64")]
    {
        "aarch64"
    }
    #[cfg(target_arch = "riscv64")]
    {
        "riscv64"
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64", target_arch = "riscv64")))]
    {
        "unknown"
    }
}

fn is_separator(byte: u8) -> bool {
    byte == 0 || byte == b' ' || byte == b'\t' || byte == b'\r' || byte == b'\n'
}

fn is_printable_ascii(bytes: &[u8]) -> bool {
    bytes.iter().all(|&byte| (0x20..=0x7e).contains(&byte))
}

/// Append one generated record, returning the new length or `None` when it
/// does not fit.
fn append_record(out: &mut [u8], len: usize, record: &[u8]) -> Option<usize> {
    let end = len.checked_add(record.len())?.checked_add(1)?;
    if end > out.len() {
        return None;
    }
    for (dst, src) in out[len..len + record.len()].iter_mut().zip(record.iter()) {
        *dst = *src;
    }
    out[len + record.len()] = 0;
    Some(end)
}

/// True when the already-written output contains `key` as a whole record key.
fn output_has_key(out: &[u8], len: usize, key: &[u8]) -> bool {
    let mut start = 0usize;
    while start < len {
        let end = match out[start..len].iter().position(|&byte| byte == 0) {
            Some(offset) => start + offset,
            None => return false,
        };
        let record = &out[start..end];
        if let Some(eq) = record.iter().position(|&byte| byte == b'=') {
            if &record[..eq] == key {
                return true;
            }
        }
        start = end + 1;
    }
    false
}

/// Normalize `input` into `out`, generating `ARCH` (always) and `BOARD` (only
/// when `board` is `Some`) ahead of the external records.
pub fn normalize_param_buf(input: &[u8], board: Option<&str>, out: &mut [u8]) -> ParamStats {
    let cap = out.len().min(PARAM_BUF_CAP);
    let out = &mut out[..cap];
    // Manual zeroing: the bare-metal boot faces link without libc, and a
    // compiler-emitted `memset` here would turn a missing symbol into a hang.
    for byte in out.iter_mut() {
        *byte = 0;
    }

    let mut stats = ParamStats::default();
    let mut len = 0usize;

    let mut arch = [0u8; 32];
    let mut arch_len = 0usize;
    for &byte in b"ARCH=" {
        arch[arch_len] = byte;
        arch_len += 1;
    }
    for &byte in arch_name().as_bytes() {
        arch[arch_len] = byte;
        arch_len += 1;
    }
    match append_record(out, len, &arch[..arch_len]) {
        Some(next) => len = next,
        None => return stats,
    }

    if let Some(board) = board {
        let value = board.as_bytes();
        if !value.is_empty() && is_printable_ascii(value) && !value.contains(&b'=') {
            let mut record = [0u8; 64];
            let mut total = 0usize;
            for &byte in b"BOARD=" {
                record[total] = byte;
                total += 1;
            }
            for &byte in value {
                record[total] = byte;
                total += 1;
            }
            if total < record.len() {
                match append_record(out, len, &record[..total]) {
                    Some(next) => len = next,
                    None => return stats,
                }
            } else {
                stats.rejected += 1;
            }
        } else {
            stats.rejected += 1;
        }
    }

    let mut dropped = false;
    for record in input.split(|&byte| is_separator(byte)) {
        if record.is_empty() {
            continue;
        }
        let valid = is_printable_ascii(record)
            && record
                .iter()
                .position(|&byte| byte == b'=')
                .is_some_and(|eq| eq > 0);
        if !valid {
            stats.rejected += 1;
            continue;
        }
        let eq = record.iter().position(|&byte| byte == b'=').expect("checked above");
        let key = &record[..eq];
        if key.eq_ignore_ascii_case(b"ARCH") || key.eq_ignore_ascii_case(b"BOARD") {
            stats.rejected += 1;
            continue;
        }
        if dropped || output_has_key(out, len, key) {
            stats.rejected += 1;
            continue;
        }
        match append_record(out, len, record) {
            Some(next) => len = next,
            None => {
                dropped = true;
                stats.dropped += 1;
            }
        }
    }

    stats.bytes = len;
    stats
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    fn records(out: &[u8], stats: ParamStats) -> Vec<&str> {
        let mut result = Vec::new();
        let mut start = 0usize;
        while start < stats.bytes {
            let end = out[start..stats.bytes]
                .iter()
                .position(|&byte| byte == 0)
                .map(|offset| start + offset)
                .expect("records are NUL-terminated");
            result.push(core::str::from_utf8(&out[start..end]).unwrap());
            start = end + 1;
        }
        result
    }

    #[test]
    fn generates_arch_and_board_first() {
        let mut out = [0u8; PARAM_BUF_CAP];
        let stats = normalize_param_buf(b"FOO=bar", Some("qemu"), &mut out);
        assert!(stats.bytes > 0);
        assert_eq!(stats.rejected, 0);
        assert_eq!(stats.dropped, 0);
        let records = records(&out, stats);
        assert!(records[0].starts_with("ARCH="));
        assert_eq!(records[1], "BOARD=qemu");
        assert_eq!(records[2], "FOO=bar");
    }

    #[test]
    fn splits_nul_and_whitespace_records() {
        let mut out = [0u8; PARAM_BUF_CAP];
        let stats = normalize_param_buf(b"A=1\0B=2 C=3\tD=4\r\nE=5", None, &mut out);
        let records = records(&out, stats);
        assert_eq!(&records[1..], &["A=1", "B=2", "C=3", "D=4", "E=5"]);
    }

    #[test]
    fn rejects_internal_key_override() {
        let mut out = [0u8; PARAM_BUF_CAP];
        let stats = normalize_param_buf(b"ARCH=evil\0board=fake\0GOOD=1", Some("real"), &mut out);
        let records = records(&out, stats);
        assert_eq!(stats.rejected, 2);
        assert!(records.contains(&"GOOD=1"));
        assert!(!records.iter().any(|record| record.contains("evil")));
        assert!(!records.iter().any(|record| record.contains("fake")));
    }

    #[test]
    fn space_separated_generated_keys_are_rejected() {
        let mut out = [0u8; PARAM_BUF_CAP];
        let stats = normalize_param_buf(b"ARCH=probe BOARD=qemu FOO=bar", None, &mut out);
        let records = records(&out, stats);
        assert_eq!(stats.rejected, 2);
        assert!(records.contains(&"FOO=bar"));
    }

    #[test]
    fn rejects_malformed_and_non_ascii() {
        let mut out = [0u8; PARAM_BUF_CAP];
        let stats = normalize_param_buf(b"NOEQUALS\0=empty\0K=\xff\0OK=1", None, &mut out);
        let records = records(&out, stats);
        assert_eq!(stats.rejected, 3);
        assert_eq!(&records[1..], &["OK=1"]);
    }

    #[test]
    fn first_duplicate_key_wins() {
        let mut out = [0u8; PARAM_BUF_CAP];
        let stats = normalize_param_buf(b"K=first\0K=second", None, &mut out);
        let records = records(&out, stats);
        assert!(records.contains(&"K=first"));
        assert!(!records.contains(&"K=second"));
        assert_eq!(stats.rejected, 1);
    }

    #[test]
    fn drops_tail_records_whole_at_capacity() {
        let mut out = [0u8; 32];
        let stats = normalize_param_buf(b"AAAAAAAAAA=1 BBBBBBBBBB=2 CCCCCCCCCC=3", None, &mut out);
        assert!(stats.dropped >= 1);
        assert!(stats.bytes <= out.len());
        // No half record: the buffer either ends at a NUL or at a whole record.
        if stats.bytes > 0 {
            assert_eq!(out[stats.bytes - 1], 0);
        }
    }

    #[test]
    fn empty_input_yields_arch_only() {
        let mut out = [0u8; PARAM_BUF_CAP];
        let stats = normalize_param_buf(b"", None, &mut out);
        let records = records(&out, stats);
        assert_eq!(records.len(), 1);
        assert!(records[0].starts_with("ARCH="));
    }
}
