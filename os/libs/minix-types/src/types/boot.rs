//! Boot image types.
//!
//! Types for boot-time process information, shared across kernel and services.
//!
//! Corresponds to Minix3's `struct boot_image` in `minix/include/minix/type.h`.

use crate::Endpoint;

pub const PROC_NAME_LEN: usize = 16;
pub const NR_BOOT_PROCS: usize = 32;

/// Boot-time process information.
///
/// Set in kernel/table.c and passed to services during boot.
/// Used by VM, PM, RS, and IS services.
///
/// Corresponds to Minix3's `struct boot_image` in `minix/include/minix/type.h`.
#[derive(Debug, Clone, Copy)]
pub struct BootImage {
    pub proc_nr: i32,
    pub proc_name: [u8; PROC_NAME_LEN],
    pub endpoint: Endpoint,
    pub start_addr: u64,
    pub len: u64,
}

impl BootImage {
    pub const fn empty() -> Self {
        Self {
            proc_nr: 0,
            proc_name: [0; PROC_NAME_LEN],
            endpoint: Endpoint::NONE,
            start_addr: 0,
            len: 0,
        }
    }

    pub fn name(&self) -> &str {
        let len = self.proc_name.iter().position(|&b| b == 0).unwrap_or(PROC_NAME_LEN);
        core::str::from_utf8(&self.proc_name[..len]).unwrap_or("<invalid>")
    }
}

impl Default for BootImage {
    fn default() -> Self {
        Self::empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_boot_image_empty() {
        let img = BootImage::empty();
        assert_eq!(img.proc_nr, 0);
        assert_eq!(img.endpoint, Endpoint::NONE);
        assert_eq!(img.start_addr, 0);
        assert_eq!(img.len, 0);
    }

    #[test]
    fn test_boot_image_name() {
        let mut img = BootImage::empty();
        img.proc_name[0] = b'p';
        img.proc_name[1] = b'm';
        img.proc_name[2] = 0;
        assert_eq!(img.name(), "pm");
    }

    #[test]
    fn test_boot_image_name_truncated() {
        let mut img = BootImage::empty();
        for i in 0..PROC_NAME_LEN {
            img.proc_name[i] = b'a';
        }
        assert_eq!(img.name(), "aaaaaaaaaaaaaaaa");
    }
}
