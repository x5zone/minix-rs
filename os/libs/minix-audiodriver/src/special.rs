//! Special files: which minor device maps to which sub-devices.
//!
//! C correspondence: `special_file_t` and the `get_special_file` lookup
//! (`audio_fw.h:57-64`, `audio_fw.c` tail): each minor carries one read
//! channel, one write channel, and the owning ioctl channel; `-1` means
//! "not this direction". The lookup is a linear scan exactly like C's.

/// One special file: minor device to channel mapping (`special_file_t`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpecialFile {
    /// Minor device number.
    pub minor: i32,
    /// Read sub-device channel, or [`NO_CHANNEL`].
    pub read_chan: i32,
    /// Write sub-device channel, or [`NO_CHANNEL`].
    pub write_chan: i32,
    /// Channel owning ioctls, or [`NO_CHANNEL`].
    pub ioctl_chan: i32,
}

/// Marker for "no channel" (`NO_CHANNEL`, `audio_fw.h:60`).
pub const NO_CHANNEL: i32 = -1;

/// Look up the special file serving this minor device
/// (`get_special_file`, `audio_fw.c` tail): first match wins, `None`
/// when the minor belongs to nobody.
pub fn get_special_file(files: &[SpecialFile], minor: i32) -> Option<&SpecialFile> {
    files.iter().find(|file| file.minor == minor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lookup_finds_first_matching_minor() {
        let files = [
            SpecialFile { minor: 0, read_chan: NO_CHANNEL, write_chan: 0, ioctl_chan: 0 },
            SpecialFile { minor: 0, read_chan: 1, write_chan: 1, ioctl_chan: 1 },
            SpecialFile { minor: 1, read_chan: NO_CHANNEL, write_chan: NO_CHANNEL, ioctl_chan: 2 },
        ];
        assert_eq!(get_special_file(&files, 0).unwrap().write_chan, 0);
        assert_eq!(get_special_file(&files, 1).unwrap().ioctl_chan, 2);
        assert!(get_special_file(&files, 9).is_none());
    }
}
