//! Page addressing: read slices, write slices, address width.
//!
//! C correspondence: the read slice (`cat24c256_read128`,
//! `cat24c256.c:251-293`, at most one bus-buffer per call), the
//! read loop (`cat24c256_read`, `cat24c256.c:296-319`, stepping 128
//! bytes), the write slice (`cat24c256_write16`,
//! `cat24c256.c:322-364`, one or two address bytes depending on the
//! no-page flag, `cat24c256.c:342-350`), the write loop
//! (`cat24c256_write`, `cat24c256.c:367-390`, stepping 16 bytes to
//! avoid crossing a page wrap), and the device size (32768 bytes,
//! `cat24c256.c:427-428`).
//!
//! Bus traffic stays in the service binary; this module owns the
//! pure slicing half: how a transfer splits and how wide each
//! address is.

/// Bytes read per bus call (`cat24c256_read128`).
pub const READ_SLICE: u64 = 128;

/// Bytes written per bus call (`cat24c256_write`, stepping 16).
pub const WRITE_SLICE: u64 = 16;

/// Device size in bytes (32K, `cat24c256.c:427-428`).
pub const DEVICE_SIZE: u64 = 32768;

/// Split a read of `total` bytes into slice lengths
/// (`cat24c256_read`, `cat24c256.c:296-319`).
pub fn split_read(total: u64) -> alloc::vec::Vec<u64> {
    split(total, READ_SLICE)
}

/// Split a write of `total` bytes into slice lengths
/// (`cat24c256_write`, `cat24c256.c:367-390`).
pub fn split_write(total: u64) -> alloc::vec::Vec<u64> {
    split(total, WRITE_SLICE)
}

/// Address width of one slice: one byte when the no-page flag is
/// set, two bytes otherwise (`cat24c256.c:342-350`).
pub fn address_width(no_page: bool) -> usize {
    if no_page { 1 } else { 2 }
}

fn split(total: u64, slice: u64) -> alloc::vec::Vec<u64> {
    let mut out = alloc::vec::Vec::new();
    let mut remaining = total;
    while remaining > 0 {
        let step = remaining.min(slice);
        out.push(step);
        remaining -= step;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slices_match_driver_steps() {
        assert_eq!(READ_SLICE, 128);
        assert_eq!(WRITE_SLICE, 16);
        assert_eq!(DEVICE_SIZE, 32768);
    }

    #[test]
    fn test_read_splits_at_128() {
        assert_eq!(split_read(300), alloc::vec![128, 128, 44]);
        assert_eq!(split_read(0), alloc::vec![]);
    }

    #[test]
    fn test_write_splits_at_16() {
        assert_eq!(split_write(40), alloc::vec![16, 16, 8]);
    }

    #[test]
    fn test_address_width_follows_no_page_flag() {
        assert_eq!(address_width(true), 1);
        assert_eq!(address_width(false), 2);
    }
}
