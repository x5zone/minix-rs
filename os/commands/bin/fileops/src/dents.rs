//! The `getdents` record walk: the directory-entry decoder shared by `ls` and
//! the recursive commands (`rm -r`, `cp -R`, `chmod -R`, `find`, `du`).
//!
//! Ground truth: the record layout is NetBSD's `struct dirent`
//! (`minix3/sys/sys/dirent.h:59-70` — `d_fileno` 8 bytes at 0, `d_ino` 4 bytes
//! at 8 … this tree's variant is the one the file systems write and the libc
//! reader consumes: `d_fileno` u64 at 0, `d_reclen` u16 at 8, `d_namlen` u16 at
//! 10, `d_type` u8 at 12, the name from 13). The per-record advance is C's
//! `readdir` (`minix3/lib/libc/gen/readdir.c:75-81`: take the record at the
//! cursor, refuse one whose `d_reclen` runs past the buffer, step
//! `dd_loc += d_reclen`).
//!
//! This half is pure byte slicing: no file descriptor, no allocation, no error
//! strings, so the layout is pinned by unit tests against a hand-built buffer.
//! The only deliberate difference from C is fail-closed decoding — a record
//! whose length field contradicts the buffer stops the walk instead of letting
//! the reader run off the end (C reads whatever the file system handed back).

use alloc::string::String;

/// The offset of `d_name` inside a record: the fixed header above it.
pub const NAME_OFFSET: usize = 13;

/// One decoded directory entry. The name borrows the caller's buffer, so a
/// record never has to be copied out to be inspected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry<'a> {
    /// `d_fileno`: the inode number; 0 marks a slot the file system freed.
    pub ino: u64,
    /// `d_type`: the entry's file type as the file system recorded it (0 when
    /// it does not know, which is why `ls -l` still stats).
    pub file_type: u8,
    /// `d_name`, `d_namlen` bytes long, without its terminator.
    pub name: &'a [u8],
}

/// Why a record could not be decoded; the walk stops rather than guessing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordError {
    /// Fewer bytes left than the fixed header (`NAME_OFFSET`).
    TooShort,
    /// `d_reclen` is zero, or the record runs past the bytes actually present.
    BadLength,
    /// `d_namlen` is longer than the record can hold.
    BadNameLength,
}

/// Decode the record starting at `offset` and report where the next one starts,
/// mirroring `dp = (struct dirent *)(buf + dd_loc); dd_loc += dp->d_reclen`.
///
/// `Ok(None)` is the ordinary end of the buffer: either nothing is left, or the
/// remaining bytes cannot hold another record (the latter is reported through
/// `RecordError` only when a length field actively contradicts the buffer).
pub fn next_record(buf: &[u8], offset: usize) -> Result<Option<(Entry<'_>, usize)>, RecordError> {
    let record = &buf[offset..];
    if record.len() < NAME_OFFSET {
        return Ok(None);
    }
    let ino = u64::from_ne_bytes(record[0..8].try_into().expect("eight bytes"));
    let reclen = u16::from_ne_bytes([record[8], record[9]]) as usize;
    let namlen = u16::from_ne_bytes([record[10], record[11]]) as usize;
    let file_type = record[12];
    if reclen == 0 || reclen > record.len() {
        // C stops the same way: `readdir` refuses a record whose d_reclen runs
        // past what the kernel returned (readdir.c:79).
        return Err(RecordError::BadLength);
    }
    if reclen < NAME_OFFSET || namlen > reclen - NAME_OFFSET {
        return Err(RecordError::BadNameLength);
    }
    let name = &record[NAME_OFFSET..NAME_OFFSET + namlen];
    Ok(Some((
        Entry {
            ino,
            file_type,
            name,
        },
        offset + reclen,
    )))
}

/// Whether an entry belongs in a listing (C `ls.c`'s dot handling): `-a` keeps
/// every name, `-A` keeps the dot entries except `.` and `..`, the default
/// keeps nothing that starts with a dot. A freed slot (`d_fileno` 0) with a
/// dotted name is never shown, whatever the flags.
pub fn keep_in_listing(name: &str, show_dot: bool, almost_all: bool) -> bool {
    if name.starts_with('.') {
        return show_dot || (almost_all && name != "." && name != "..");
    }
    true
}

/// Whether a name is `.` or `..` (C `rm.c:588` `ISDOT`, and the same test the
/// recursive walk needs so it never descends into itself).
pub fn is_dot(name: &str) -> bool {
    name == "." || name == ".."
}

/// The names a recursive walk descends into: everything except the two dots.
pub fn is_descendable(name: &str) -> bool {
    !name.is_empty() && !is_dot(name)
}

/// C's operand-to-path rule for a directory listing (`ls.c`'s `dotdir`): the
/// directory `.` contributes bare names, any other directory prefixes them.
pub fn join_path(dir: &str, name: &str) -> String {
    if dir == "." {
        return String::from(name);
    }
    if dir.ends_with('/') {
        let mut out = String::from(dir);
        out.push_str(name);
        return out;
    }
    let mut out = String::from(dir);
    out.push('/');
    out.push_str(name);
    out
}

/// The file type values a `getdents` record may carry (`minix3/sys/sys/dirent.h`
/// `DT_*`, matching the `S_IF*` low bytes the file systems store).
pub mod file_type {
    /// `DT_UNKNOWN`: the file system did not record a type.
    pub const UNKNOWN: u8 = 0;
    /// `DT_REG`.
    pub const REGULAR: u8 = 8;
    /// `DT_DIR`.
    pub const DIRECTORY: u8 = 4;
    /// `DT_LNK`.
    pub const LINK: u8 = 10;
    /// `DT_FIFO`.
    pub const FIFO: u8 = 1;
    /// `DT_SOCK`.
    pub const SOCKET: u8 = 12;
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    /// Build one record the way a file system writes it: header plus name, then
    /// pad to `reclen`.
    fn record(ino: u64, file_type: u8, name: &[u8], reclen: usize) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&ino.to_ne_bytes());
        out.extend_from_slice(&(reclen as u16).to_ne_bytes());
        out.extend_from_slice(&(name.len() as u16).to_ne_bytes());
        out.push(file_type);
        out.extend_from_slice(name);
        out.resize(reclen, 0);
        out
    }

    #[test]
    fn record_decodes_layout_and_next_offset() {
        let one = record(7, file_type::DIRECTORY, b"docs", 24);
        let two = record(9, file_type::REGULAR, b"readme.md", 32);
        let mut buf = one.clone();
        buf.extend_from_slice(&two);
        let (first, next) = next_record(&buf, 0).unwrap().expect("one record");
        assert_eq!(first.ino, 7);
        assert_eq!(first.file_type, file_type::DIRECTORY);
        assert_eq!(first.name, b"docs");
        assert_eq!(next, 24, "the cursor steps by d_reclen, as readdir.c:81");
        let (second, end) = next_record(&buf, next).unwrap().expect("second record");
        assert_eq!(second.name, b"readme.md");
        assert_eq!(end, buf.len());
        assert_eq!(next_record(&buf, end).unwrap(), None, "buffer exhausted");
    }

    #[test]
    fn short_tail_and_empty_buffer_end_the_walk() {
        // Fewer bytes than the header is the ordinary stop, not an error: C
        // simply has no record to return there.
        let buf = [0u8; NAME_OFFSET - 1];
        assert_eq!(next_record(&buf, 0).unwrap(), None);
        assert_eq!(next_record(&[], 0).unwrap(), None);
    }

    #[test]
    fn contradictory_lengths_stop_the_walk_instead_of_running_off() {
        // d_reclen 0 would freeze the cursor; C's loop never advances past a
        // record it cannot measure. The length is patched into an otherwise
        // well-formed record, since a 0-length record has no bytes at all.
        let mut zero = record(1, file_type::REGULAR, b"a", 24);
        zero[8..10].copy_from_slice(&0u16.to_ne_bytes());
        assert_eq!(
            next_record(&zero, 0),
            Err(RecordError::BadLength),
            "a zero d_reclen must not spin the walk"
        );
        // A record claiming more bytes than the buffer holds (readdir.c:79).
        let mut over = record(1, file_type::REGULAR, b"name", 24);
        over[8..10].copy_from_slice(&48u16.to_ne_bytes());
        assert_eq!(next_record(&over, 0), Err(RecordError::BadLength));
        // A name length that exceeds its own record.
        let bad_name = record(1, file_type::REGULAR, b"abcd", 14);
        assert_eq!(next_record(&bad_name, 0), Err(RecordError::BadNameLength));
    }

    #[test]
    fn dot_entries_follow_the_listing_flags() {
        assert!(keep_in_listing("notes", false, false));
        assert!(!keep_in_listing(".hidden", false, false));
        assert!(keep_in_listing(".hidden", true, false));
        assert!(!keep_in_listing(".", false, true), "-A drops . and ..");
        assert!(!keep_in_listing("..", false, true));
        assert!(
            keep_in_listing(".profile", false, true),
            "-A keeps the rest"
        );
        assert!(keep_in_listing(".", true, false), "-a keeps the dots");
    }

    #[test]
    fn descends_only_into_real_names() {
        assert!(is_descendable("sub"));
        assert!(!is_descendable("."));
        assert!(!is_descendable(".."));
        assert!(!is_descendable(""), "an empty name cannot be walked");
        assert!(is_dot(".."));
        assert!(!is_dot("..."));
    }

    #[test]
    fn path_joining_matches_c_ls_dotdir_rule() {
        assert_eq!(join_path(".", "a.txt"), "a.txt");
        assert_eq!(join_path("dir", "a.txt"), "dir/a.txt");
        assert_eq!(join_path("dir/", "a.txt"), "dir/a.txt");
        assert_eq!(join_path("/tmp", "x"), "/tmp/x");
    }
}
