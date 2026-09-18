//! Device-family wire numbers: the character and block driver protocol
//! families that more than one crate consumes.
//!
//! C correspondence: the driver request/reply bases and flag bits of
//! `minix3/minix/include/minix/com.h:915-1000` — the character family
//! (`CDEV_*`, com.h:915-957) and the block family (`BDEV_*`,
//! com.h:963-1000). These are the numbers two unrelated crates must agree
//! on byte for byte: the driver-side framework (`minix-chardriver`,
//! `minix-blockdriver`) builds requests with them and the virtual file
//! system (`servers/vfs`) classifies replies with them. The families with
//! a single consumer (NDEV in the netdriver, RTCDEV in the readclock
//! driver, the USB request numbers in the usb wire module) stay with
//! their owner — distribution is not duplication.
//!
//! Every constant is the C value verbatim, `i32` to match the message
//! type lane. The pin tests at the bottom of this module lock each number
//! to its `com.h` line; a drift breaks the build here, not on the wire.

/// Base of the character-driver request range
/// (`CDEV_REQUEST_BASE`, `com.h:915` = 0x400).
pub const CDEV_REQUEST_BASE: i32 = 0x400;

/// Base of the character-driver reply range
/// (`CDEV_REPLY_BASE`, `com.h:920` = 0x480).
pub const CDEV_REPLY_BASE: i32 = 0x480;

/// Do not suspend the I/O request (`CDEV_NONBLOCK`, `com.h:946`).
pub const CDEV_NONBLOCK: i32 = 0x01;

/// Reply carries a fresh minor number (`CDEV_CLONED`, `com.h:955`).
pub const CDEV_CLONED: i32 = 0x2000_0000;

/// Reply grants the controlling terminal (`CDEV_CTTY`, `com.h:956`).
pub const CDEV_CTTY: i32 = 0x4000_0000;

/// Base of the block-driver request range
/// (`BDEV_RQ_BASE`, `com.h:963` = 0x500).
pub const BDEV_RQ_BASE: i32 = 0x500;

/// Base of the block-driver reply range
/// (`BDEV_RS_BASE`, `com.h:964` = 0x580).
pub const BDEV_RS_BASE: i32 = 0x580;

/// The only reply type a block driver sends
/// (`BDEV_REPLY`, `com.h:979` = reply base plus zero).
pub const BDEV_REPLY: i32 = 0x580;

/// Request offset: open (`BDEV_OPEN_OFF`; message selectors ride
/// `BDEV_RQ_BASE` + offset, `com.h:963-976`; open = 0).
pub const BDEV_OPEN_OFF: i32 = 0;

/// Request offset: close (`BDEV_CLOSE_OFF` = 1).
pub const BDEV_CLOSE_OFF: i32 = 1;

/// Request offset: ioctl (`BDEV_IOCTL_OFF` = 6).
pub const BDEV_IOCTL_OFF: i32 = 6;

/// Open access bit: read (`CDEV_R_BIT`, `com.h:940`).
pub const CDEV_R_BIT: i32 = 0x01;

/// Open access bit: write (`CDEV_W_BIT`, `com.h:941`).
pub const CDEV_W_BIT: i32 = 0x02;

/// Open access bit: refuse controlling-terminal adoption
/// (`CDEV_NOCTTY`, `com.h:942`).
pub const CDEV_NOCTTY: i32 = 0x04;

/// Select readiness: readable (`SELECT_READ`, `com.h:939-957` family).
pub const SELECT_READ: i32 = 0x01;

/// Select readiness: writable (`SELECT_WRITE`).
pub const SELECT_WRITE: i32 = 0x02;

/// Select readiness: error pending (`SELECT_ERROR`).
pub const SELECT_ERROR: i32 = 0x04;

/// Deliver a select-2 notification on readiness (`SELECT_NOTIFY`).
pub const SELECT_NOTIFY: i32 = 0x08;

/// Open access: read intent (`ACCESS_READ`, `com.h:939-941` family).
pub const ACCESS_READ: i32 = 0x01;

/// Open access: write intent (`ACCESS_WRITE`).
pub const ACCESS_WRITE: i32 = 0x02;

/// Open access: skip controlling-terminal adoption
/// (`ACCESS_NO_CONTROLLING_TERMINAL`).
pub const ACCESS_NO_CONTROLLING_TERMINAL: i32 = 0x04;

/// Transfer flags: blocking (`TRANSFER_NO_FLAGS`).
pub const TRANSFER_NO_FLAGS: i32 = 0x00;

/// Transfer flags: non-blocking (`TRANSFER_NON_BLOCKING`).
pub const TRANSFER_NON_BLOCKING: i32 = 0x01;

/// Open access bit: read (`BDEV_R_BIT`, `com.h:982`).
pub const BDEV_R_BIT: i32 = 0x01;

/// Open access bit: write (`BDEV_W_BIT`, `com.h:983`).
pub const BDEV_W_BIT: i32 = 0x02;

#[cfg(test)]
mod tests {
    use super::*;

    /// Character family pinned to `com.h:915-957`.
    #[test]
    fn test_cdev_numbers_match_com_header() {
        assert_eq!(CDEV_REQUEST_BASE, 0x400);
        assert_eq!(CDEV_REPLY_BASE, 0x480);
        assert_eq!(CDEV_NONBLOCK, 0x01);
        assert_eq!(CDEV_CLONED, 0x2000_0000);
        assert_eq!(CDEV_CTTY, 0x4000_0000);
        assert_eq!(CDEV_R_BIT, 0x01);
        assert_eq!(CDEV_W_BIT, 0x02);
        assert_eq!(CDEV_NOCTTY, 0x04);
    }

    /// Block family pinned to `com.h:963-983`.
    #[test]
    fn test_bdev_numbers_match_com_header() {
        assert_eq!(BDEV_RQ_BASE, 0x500);
        assert_eq!(BDEV_RS_BASE, 0x580);
        assert_eq!(BDEV_REPLY, 0x580);
        assert_eq!(BDEV_R_BIT, 0x01);
        assert_eq!(BDEV_W_BIT, 0x02);
    }

    /// The two families stay in disjoint ranges: a reply base of one must
    /// never fall inside the other's request window (the CDEV_REPLY_BASE
    /// 0x500 copy-paste class of bug — the number belongs to BDEV_RQ_BASE).
    #[test]
    fn test_families_do_not_overlap() {
        assert_ne!(CDEV_REPLY_BASE, BDEV_RQ_BASE);
        assert!(CDEV_REPLY_BASE < BDEV_RQ_BASE);
    }
}
