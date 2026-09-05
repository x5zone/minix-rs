//! Filesystem table row parsing (`fstab`).
//!
//! One row per mountable filesystem, six whitespace separated fields:
//! device, mount point, filesystem type, comma separated options, dump
//! frequency, check pass number. Comment lines (`#` first) and blank lines
//! carry no rows. The pass number drives check ordering (see [`crate::order`]):
//! 0 means "never check" (memory and network filesystems), 1 is the root
//! (checked first and alone), higher numbers check later.

use crate::MountError;

/// One parsed table row, borrowed from the line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FstabEntry<'a> {
    /// Device or remote path (`/dev/c0d0p0s0`, `procfs`, ...).
    pub device: &'a str,
    /// Where it mounts (`/`, `/usr`, ...).
    pub mount_point: &'a str,
    /// Filesystem type (`mfs`, `ext2`, `procfs`, ...).
    pub fs_type: &'a str,
    /// Raw option list text (`rw,noexec`, ...); parsed by [`crate::options`].
    pub options: &'a str,
    /// Dump frequency (0 means never).
    pub dump: u32,
    /// Check pass number (0 means skip).
    pub pass: u32,
}

/// Parse one table line; comments and blanks yield `Ok(None)`. Exactly six
/// fields are required (a seventh word is an error, not ignored).
pub fn parse_fstab_line<'a>(line: &'a str) -> Result<Option<FstabEntry<'a>>, MountError> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return Ok(None);
    }
    let mut words = trimmed.split_whitespace();
    let device = words.next().ok_or(MountError::InvalidArgument)?;
    let mount_point = words.next().ok_or(MountError::InvalidArgument)?;
    let fs_type = words.next().ok_or(MountError::InvalidArgument)?;
    let options = words.next().ok_or(MountError::InvalidArgument)?;
    let dump = parse_number(words.next().ok_or(MountError::InvalidArgument)?)?;
    let pass = parse_number(words.next().ok_or(MountError::InvalidArgument)?)?;
    if words.next().is_some() {
        return Err(MountError::InvalidArgument);
    }
    if device.is_empty() || mount_point.is_empty() || fs_type.is_empty() {
        return Err(MountError::InvalidArgument);
    }
    Ok(Some(FstabEntry {
        device,
        mount_point,
        fs_type,
        options,
        dump,
        pass,
    }))
}

fn parse_number(text: &str) -> Result<u32, MountError> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err(MountError::InvalidArgument);
    }
    let mut value: u32 = 0;
    for byte in text.bytes() {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add((byte - b'0') as u32))
            .ok_or(MountError::InvalidArgument)?;
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_root_row() {
        let entry = parse_fstab_line("/dev/c0d0p0s0 / mfs rw 1 1").unwrap().unwrap();
        assert_eq!(entry.device, "/dev/c0d0p0s0");
        assert_eq!(entry.mount_point, "/");
        assert_eq!(entry.fs_type, "mfs");
        assert_eq!(entry.options, "rw");
        assert_eq!((entry.dump, entry.pass), (1, 1));
    }

    #[test]
    fn test_proc_row_skips_checks() {
        let entry = parse_fstab_line("procfs /proc procfs ro 0 0").unwrap().unwrap();
        assert_eq!((entry.dump, entry.pass), (0, 0));
    }

    #[test]
    fn test_comment_and_blank_skipped() {
        assert_eq!(parse_fstab_line("# device mp type opts dump pass"), Ok(None));
        assert_eq!(parse_fstab_line(""), Ok(None));
    }

    #[test]
    fn test_wrong_field_count_rejected() {
        assert_eq!(
            parse_fstab_line("/dev/x / mfs rw 1"),
            Err(MountError::InvalidArgument)
        );
        assert_eq!(
            parse_fstab_line("/dev/x / mfs rw 1 1 extra"),
            Err(MountError::InvalidArgument)
        );
    }

    #[test]
    fn test_non_numeric_trailer_rejected() {
        assert_eq!(
            parse_fstab_line("/dev/x / mfs rw x 1"),
            Err(MountError::InvalidArgument)
        );
    }
}
