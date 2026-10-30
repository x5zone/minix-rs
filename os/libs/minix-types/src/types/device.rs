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

/// Transfer flags: no flags set (`BDEV_NOFLAGS`, `com.h:986`).
pub const BDEV_NOFLAGS: i32 = 0x00;

/// Transfer flag: force the write through to the medium (`BDEV_FORCEWRITE`,
/// `com.h:987`).
pub const BDEV_FORCEWRITE: i32 = 0x01;

/// Transfer flag: do not send a page address (`BDEV_NOPAGE`, `com.h:988`).
pub const BDEV_NOPAGE: i32 = 0x02;

/// LP64 field offsets of `mess_vfs_lchardriver_openclose` (VFS -> char driver).
///
/// C: `{ endpoint_t id; endpoint_t user; devminor_t minor; int access; }`
/// (ipc.h:2228-2235) — `minor` is **not** first; `id`/`user` come before it.
/// `cdev_opcl` (cdev.c:198-208) fills `minor` and `id`; only the open direction
/// additionally fills `user` and `access`.
pub mod lchardriver_openclose_off {
    /// `endpoint_t id` (the caller's endpoint).
    pub const ID: usize = 0;
    /// `endpoint_t user` (filled only by the open direction).
    pub const USER: usize = 4;
    /// `devminor_t minor` (the minor device number actually being opened).
    pub const MINOR: usize = 8;
    /// `int access` (`CDEV_R_BIT`/`CDEV_W_BIT`/`CDEV_NOCTTY`).
    pub const ACCESS: usize = 12;
}

/// LP64 field offsets of `mess_vfs_lchardriver_readwrite` (VFS -> char driver).
///
/// C: `{ off_t pos; cp_grant_id_t grant; size_t count; unsigned long request;
/// int flags; endpoint_t id; endpoint_t user; devminor_t minor; }`
/// (ipc.h:2238-2249) — read, write, and ioctl **share** this one struct:
/// `cdev_io` (cdev.c:319-330) uses `request`/`user` for ioctl and
/// `pos`/`count` for read/write.
pub mod lchardriver_readwrite_off {
    /// `off_t pos` (read/write position; ioctl leaves it unset).
    pub const POS: usize = 0;
    /// `cp_grant_id_t grant` (the magic grant of the user buffer).
    pub const GRANT: usize = 8;
    /// `size_t count` (read/write byte count; ioctl leaves it unset).
    pub const COUNT: usize = 16;
    /// `unsigned long request` (the ioctl request code).
    pub const REQUEST: usize = 24;
    /// `int flags` (`CDEV_NONBLOCK`).
    pub const FLAGS: usize = 32;
    /// `endpoint_t id`.
    pub const ID: usize = 36;
    /// `endpoint_t user` (filled only by ioctl).
    pub const USER: usize = 40;
    /// `devminor_t minor`.
    pub const MINOR: usize = 44;
}

/// LP64 field offsets of a driver reply (driver -> VFS): `{ int status; int id; }`.
///
/// The character family's `mess_lchardriver_vfs_reply` (ipc.h:943-948) and the
/// block family's `mess_lblockdriver_lbdev_reply` (ipc.h:356-361) have the same
/// field order, but C declares them as two separate structs — only the offset
/// table is shared here; the semantics are not merged.
pub mod driver_reply_off {
    /// `int status` (**status is the first word**).
    pub const STATUS: usize = 0;
    /// `int id` (declared `uint32_t` in the character family, same value range).
    pub const ID: usize = 4;
}

/// LP64 field offsets of `mess_lbdev_lblockdriver_msg` (VFS -> block driver).
///
/// C: `{ off_t pos; int minor; int id; int access; int count;
/// cp_grant_id_t grant; int flags; endpoint_t user; unsigned long request; }`
/// (ipc.h:338-353). **`pos` is the first slot**, so every field shifts later
/// than in the character family's `mess_vfs_lchardriver_openclose`: `minor` is at
/// 8 rather than 0, `access` at 16 rather than 8 — sending a block message with
/// the character family's positions leaves the driver holding garbage ("minor
/// device 0, invalid grant").
pub mod lblockdriver_msg_off {
    /// `off_t pos` (read/write position; always 0 for open/close/ioctl).
    pub const POS: usize = 0;
    /// `int minor` (the minor device number actually being operated on).
    pub const MINOR: usize = 8;
    /// `int id` (identifier for an asynchronous request).
    pub const ID: usize = 12;
    /// `int access` (`BDEV_R_BIT`/`BDEV_W_BIT`, filled only by open).
    pub const ACCESS: usize = 16;
    /// `int count` (read/write byte count).
    pub const COUNT: usize = 20;
    /// `cp_grant_id_t grant`.
    pub const GRANT: usize = 24;
    /// `int flags`.
    pub const FLAGS: usize = 28;
    /// `endpoint_t user` (filled by ioctl).
    pub const USER: usize = 32;
    /// `unsigned long request` (the ioctl request code).
    pub const REQUEST: usize = 40;
}

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

    /// Block family pinned to `com.h:963-988`.
    #[test]
    fn test_bdev_numbers_match_com_header() {
        assert_eq!(BDEV_RQ_BASE, 0x500);
        assert_eq!(BDEV_RS_BASE, 0x580);
        assert_eq!(BDEV_REPLY, 0x580);
        assert_eq!(BDEV_R_BIT, 0x01);
        assert_eq!(BDEV_W_BIT, 0x02);
        assert_eq!(BDEV_NOFLAGS, 0x00);
        assert_eq!(BDEV_FORCEWRITE, 0x01);
        assert_eq!(BDEV_NOPAGE, 0x02);
    }

    /// The two families stay in disjoint ranges: a reply base of one must
    /// never fall inside the other's request window (the CDEV_REPLY_BASE
    /// 0x500 copy-paste class of bug — the number belongs to BDEV_RQ_BASE).
    #[test]
    fn test_families_do_not_overlap() {
        assert_ne!(CDEV_REPLY_BASE, BDEV_RQ_BASE);
        assert!(CDEV_REPLY_BASE < BDEV_RQ_BASE);
    }

    /// Every driver message field offset is pinned (ipc.h's struct declarations):
    /// the two families' structs have **different shapes** — the character
    /// family's `openclose` starts with `id` and the block family with `off_t
    /// pos`, so applying one family's positions to the other shifts the whole
    /// row.
    #[test]
    fn test_driver_message_offsets_match_c_structs() {
        // Character family open/close: id@0, user@4, minor@8, access@12
        // (ipc.h:2228-2235).
        assert_eq!(lchardriver_openclose_off::ID, 0);
        assert_eq!(lchardriver_openclose_off::USER, 4);
        assert_eq!(lchardriver_openclose_off::MINOR, 8);
        assert_eq!(lchardriver_openclose_off::ACCESS, 12);
        // Character family read/write/ioctl share one struct (ipc.h:2238-2249).
        assert_eq!(lchardriver_readwrite_off::POS, 0);
        assert_eq!(lchardriver_readwrite_off::GRANT, 8);
        assert_eq!(lchardriver_readwrite_off::COUNT, 16);
        assert_eq!(lchardriver_readwrite_off::REQUEST, 24);
        assert_eq!(lchardriver_readwrite_off::FLAGS, 32);
        assert_eq!(lchardriver_readwrite_off::ID, 36);
        assert_eq!(lchardriver_readwrite_off::USER, 40);
        assert_eq!(lchardriver_readwrite_off::MINOR, 44);
        // Both families' replies have the same shape: status in the first word
        // (ipc.h:356-361 / :943-948).
        assert_eq!(driver_reply_off::STATUS, 0);
        assert_eq!(driver_reply_off::ID, 4);
        // Block family: `pos` is the first slot, so minor lands at 8
        // (ipc.h:338-353).
        assert_eq!(lblockdriver_msg_off::POS, 0);
        assert_eq!(lblockdriver_msg_off::MINOR, 8);
        assert_eq!(lblockdriver_msg_off::ID, 12);
        assert_eq!(lblockdriver_msg_off::ACCESS, 16);
        assert_eq!(lblockdriver_msg_off::COUNT, 20);
        assert_eq!(lblockdriver_msg_off::GRANT, 24);
        assert_eq!(lblockdriver_msg_off::FLAGS, 28);
        assert_eq!(lblockdriver_msg_off::USER, 32);
        assert_eq!(lblockdriver_msg_off::REQUEST, 40);
    }
}
