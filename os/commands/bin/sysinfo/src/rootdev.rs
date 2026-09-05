//! Root device discovery.
//!
//! Ground truth: `minix3/minix/commands/printroot/printroot.c`. The device
//! directory is `/dev/` (near line 25), the fallback for an undiscoverable
//! root is `/dev/unknown` (near line 27). Discovery stats the root directory,
//! opens the device directory, and returns the first block device whose device
//! number matches the root device number (near line 45); when nothing matches
//! it reports the fallback with a failure status (near line 55). Directory
//! walking stays with the execution layer behind the [`DeviceDir`] trait.

use crate::SysinfoError;

/// Device directory searched for the root device.
pub const DEVICE_DIRECTORY: &str = "/dev/";

/// Fallback name when no device matches.
pub const UNKNOWN_DEVICE: &str = "/dev/unknown";

/// One device directory entry (name plus device number).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceEntry<'a> {
    /// File name inside the device directory (`sda0`, `c0d0p0`, and so on).
    pub name: &'a str,
    /// Device number of the entry.
    pub device: u64,
    /// True when the entry is a block device (only those can hold a root).
    pub is_block: bool,
}

/// Device directory behind discovery.
pub trait DeviceDir<'a> {
    /// Entry at `index`, or `None` when the index is past the end.
    fn entry(&self, index: usize) -> Option<DeviceEntry<'a>>;
    /// Number of entries.
    fn len(&self) -> usize;
    /// True when the directory holds no entries.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Directory backed by a borrowed slice.
pub struct SliceDeviceDir<'a> {
    entries: &'a [DeviceEntry<'a>],
}

impl<'a> SliceDeviceDir<'a> {
    /// Build a directory over borrowed entries.
    pub fn new(entries: &'a [DeviceEntry<'a>]) -> Self {
        SliceDeviceDir { entries }
    }
}

impl<'a> DeviceDir<'a> for SliceDeviceDir<'a> {
    fn entry(&self, index: usize) -> Option<DeviceEntry<'a>> {
        self.entries.get(index).copied()
    }

    fn len(&self) -> usize {
        self.entries.len()
    }
}

/// Empty directory (nothing can match).
pub struct EmptyDeviceDir;

impl<'a> DeviceDir<'a> for EmptyDeviceDir {
    fn entry(&self, _index: usize) -> Option<DeviceEntry<'a>> {
        None
    }

    fn len(&self) -> usize {
        0
    }
}

/// Discovery outcome: the device name plus whether it was really found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Discovery<'a> {
    /// Device name (`/dev/sda0`) or the fallback (`/dev/unknown`).
    pub name: &'a str,
    /// True when a block device matched (false means the fallback).
    pub found: bool,
}

/// Find the root device: first block entry whose number matches `root_device`.
pub fn discover_root<'a, D: DeviceDir<'a>>(
    dir: &D,
    root_device: u64,
    out_name: &'a mut [u8],
) -> Result<Discovery<'a>, SysinfoError> {
    let mut index = 0;
    while let Some(entry) = dir.entry(index) {
        index += 1;
        if entry.is_block && entry.device == root_device {
            write_device_name(out_name, entry.name)?;
            let name = core::str::from_utf8(&out_name[..DEVICE_DIRECTORY.len() + entry.name.len()])
                .map_err(|_| SysinfoError::InvalidArgument)?;
            return Ok(Discovery { name, found: true });
        }
    }
    write_device_name(out_name, "unknown")?;
    let name = core::str::from_utf8(&out_name[..DEVICE_DIRECTORY.len() + "unknown".len()])
        .map_err(|_| SysinfoError::InvalidArgument)?;
    Ok(Discovery { name, found: false })
}

fn write_device_name(out: &mut [u8], leaf: &str) -> Result<(), SysinfoError> {
    let needed = DEVICE_DIRECTORY.len() + leaf.len();
    if out.len() < needed {
        return Err(SysinfoError::InvalidArgument);
    }
    out[..DEVICE_DIRECTORY.len()].copy_from_slice(DEVICE_DIRECTORY.as_bytes());
    out[DEVICE_DIRECTORY.len()..needed].copy_from_slice(leaf.as_bytes());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries() -> [DeviceEntry<'static>; 3] {
        [
            DeviceEntry { name: "tty", device: 7, is_block: false },
            DeviceEntry { name: "sda0", device: 42, is_block: true },
            DeviceEntry { name: "sda1", device: 43, is_block: true },
        ]
    }

    #[test]
    fn test_matching_block_found() {
        let list = entries();
        let dir = SliceDeviceDir::new(&list);
        let mut out = [0u8; 32];
        let found = discover_root(&dir, 42, &mut out).unwrap();
        assert!(found.found);
        assert_eq!(found.name, "/dev/sda0");
    }

    #[test]
    fn test_character_device_never_matches() {
        let list = entries();
        let dir = SliceDeviceDir::new(&list);
        let mut out = [0u8; 32];
        let found = discover_root(&dir, 7, &mut out).unwrap();
        assert!(!found.found);
        assert_eq!(found.name, UNKNOWN_DEVICE);
    }

    #[test]
    fn test_empty_dir_falls_back() {
        let dir = EmptyDeviceDir;
        let mut out = [0u8; 32];
        let found = discover_root(&dir, 42, &mut out).unwrap();
        assert!(!found.found);
        assert_eq!(found.name, UNKNOWN_DEVICE);
    }

    #[test]
    fn test_small_buffer_rejected() {
        let list = entries();
        let dir = SliceDeviceDir::new(&list);
        let mut out = [0u8; 4];
        assert_eq!(
            discover_root(&dir, 42, &mut out),
            Err(SysinfoError::InvalidArgument)
        );
    }
}
