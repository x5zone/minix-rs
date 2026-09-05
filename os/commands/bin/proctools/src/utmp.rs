//! Login record (`utmp`) parsing behind `who`, `w`, and `last`.
//!
//! Ground truth: `minix3/lib/libc/compat/include/utmp.h` (`struct utmp50`:
//! terminal line, user name, host, 32 bit time). One record per login
//! session, fixed width fields, blank padded, not newline terminated. The
//! readers (`who` prints who is on, `w` adds what they do, `last` walks the
//! archive backwards) all start from these rows; process activity and time
//! formatting stay with their owners.

use crate::ProcError;

/// Field widths of one login record, in bytes.
pub const LINE_WIDTH: usize = 8;
/// User name field width.
pub const NAME_WIDTH: usize = 8;
/// Host field width.
pub const HOST_WIDTH: usize = 16;
/// Whole record width: line, name, host, plus a 4 byte time.
pub const RECORD_WIDTH: usize = LINE_WIDTH + NAME_WIDTH + HOST_WIDTH + 4;

/// One parsed login record, borrowed from the database bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UtmpEntry<'a> {
    /// Terminal line (`console`, `ttyc1`, ...), trailing blanks trimmed.
    pub line: &'a str,
    /// User name, trailing blanks trimmed (empty means logged out).
    pub name: &'a str,
    /// Remote host, trailing blanks trimmed (empty means local).
    pub host: &'a str,
    /// Login time in seconds since the epoch, little endian on disk.
    pub time: u32,
}

impl<'a> UtmpEntry<'a> {
    /// True when the session is still logged in (a name is present).
    pub fn active(&self) -> bool {
        !self.name.is_empty()
    }
}

/// Trim trailing blanks and zero bytes from a fixed width field. Interior
/// blanks survive (they are data, not padding).
fn field(bytes: &[u8]) -> &str {
    let mut end = bytes.len();
    while end > 0 && (bytes[end - 1] == b' ' || bytes[end - 1] == 0) {
        end -= 1;
    }
    core::str::from_utf8(&bytes[..end]).unwrap_or("")
}

/// Parse one record from `record` (exactly `RECORD_WIDTH` bytes).
pub fn parse_record(record: &[u8]) -> Result<UtmpEntry<'_>, ProcError> {
    if record.len() != RECORD_WIDTH {
        return Err(ProcError::InvalidArgument);
    }
    let time = u32::from_le_bytes([
        record[LINE_WIDTH + NAME_WIDTH + HOST_WIDTH],
        record[LINE_WIDTH + NAME_WIDTH + HOST_WIDTH + 1],
        record[LINE_WIDTH + NAME_WIDTH + HOST_WIDTH + 2],
        record[LINE_WIDTH + NAME_WIDTH + HOST_WIDTH + 3],
    ]);
    Ok(UtmpEntry {
        line: field(&record[..LINE_WIDTH]),
        name: field(&record[LINE_WIDTH..LINE_WIDTH + NAME_WIDTH]),
        host: field(&record[LINE_WIDTH + NAME_WIDTH..LINE_WIDTH + NAME_WIDTH + HOST_WIDTH]),
        time,
    })
}

/// Walk a whole database image, calling `visit` for each valid record.
/// Short trailing bytes are an error (a torn write must not silently drop
/// a session); an empty image visits nothing.
pub fn walk_database<'a>(image: &'a [u8], mut visit: impl FnMut(UtmpEntry<'a>)) -> Result<usize, ProcError> {
    if !image.len().is_multiple_of(RECORD_WIDTH) {
        return Err(ProcError::InvalidArgument);
    }
    let mut count = 0;
    for chunk in image.chunks_exact(RECORD_WIDTH) {
        visit(parse_record(chunk)?);
        count += 1;
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record() -> [u8; RECORD_WIDTH] {
        let mut bytes = [b' '; RECORD_WIDTH];
        bytes[..7].copy_from_slice(b"console");
        bytes[8..12].copy_from_slice(b"root");
        bytes[16..21].copy_from_slice(b"local");
        bytes[32..36].copy_from_slice(&1_700_000_000u32.to_le_bytes());
        bytes
    }

    #[test]
    fn test_fields_trimmed() {
        let bytes = record();
        let entry = parse_record(&bytes).unwrap();
        assert_eq!(entry.line, "console");
        assert_eq!(entry.name, "root");
        assert_eq!(entry.host, "local");
        assert_eq!(entry.time, 1_700_000_000);
        assert!(entry.active());
    }

    #[test]
    fn test_logged_out_empty_name() {
        let mut bytes = record();
        for byte in bytes[8..16].iter_mut() {
            *byte = b' ';
        }
        let entry = parse_record(&bytes).unwrap();
        assert_eq!(entry.name, "");
        assert!(!entry.active());
    }

    #[test]
    fn test_short_record_rejected() {
        assert_eq!(
            parse_record(&[0u8; 10]).map(|_| ()),
            Err(ProcError::InvalidArgument)
        );
    }

    #[test]
    fn test_walk_counts_and_visits() {
        let first = record();
        let mut second = record();
        second[8..12].copy_from_slice(b"bob ");
        let mut image = [0u8; RECORD_WIDTH * 2];
        image[..RECORD_WIDTH].copy_from_slice(&first);
        image[RECORD_WIDTH..].copy_from_slice(&second);
        let mut names: [&str; 2] = ["", ""];
        let mut index = 0;
        let count = walk_database(&image, |entry| {
            names[index] = entry.name;
            index += 1;
        })
        .unwrap();
        assert_eq!(count, 2);
        assert_eq!(names, ["root", "bob"]);
    }

    #[test]
    fn test_torn_image_rejected() {
        assert_eq!(
            walk_database(&[0u8; RECORD_WIDTH + 1], |_| {}).map(|_| ()),
            Err(ProcError::InvalidArgument)
        );
    }
}
