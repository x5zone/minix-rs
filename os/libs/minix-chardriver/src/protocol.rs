//! Character request protocol: numbers, replies, flags, and open tracking.
//!
//! C correspondence: the message layout comment at the top of
//! `minix3/minix/lib/libchardriver/chardriver.c:1-44`, the request constants
//! in `minix3/minix/include/minix/com.h:919-956`, the announce and open-set
//! helpers in `chardriver.c:52-124`, and `MAX_NR_OPEN_DEVICES` in
//! `minix3/minix/include/minix/driver.h:41`.

use minix_types::{EBADF, EDONTREPLY, EINVAL, EIO, ENOTTY, ENXIO, ERESTART, OK};

/// Base of the character request range.
///
/// C: `CDEV_RQ_BASE 0x400` (`com.h:919`). A message whose type, masked with
/// "not low seven bits", equals this base is a character request; see
/// [`is_char_request`].
pub const CDEV_REQUEST_BASE: i32 = 0x400;

/// Base of the character reply range.
///
/// C: `CDEV_RS_BASE` (`com.h:934`). Replies live in a separate range so a
/// reply can never be mistaken for a new request.
pub const CDEV_REPLY_BASE: i32 = 0x500;

/// Maximum number of minor devices remembered as opened.
///
/// C: `MAX_NR_OPEN_DEVICES 256` (`driver.h:41`). The framework records every
/// minor device number that has been opened so a restarted driver can reject
/// stale requests for devices nobody opened since the restart.
pub const MAX_OPEN_DEVICES: usize = 256;

/// Flag: do not suspend the input or output request.
///
/// C: `CDEV_NONBLOCK 0x01` (`com.h:946`). When set, the driver must answer
/// immediately instead of parking the request for a later reply.
pub const CDEV_NONBLOCK: i32 = 0x01;

/// Flag bit in an open result: the device number was cloned.
///
/// C: `CDEV_CLONED 0x20000000` (`com.h:955`). When the open callback returns
/// a non-negative value with this bit set, the remaining bits carry the new
/// minor device number, which the framework records as opened as well.
pub const CDEV_CLONED: i32 = 0x2000_0000;

/// Flag bit in an open result: the device is the controlling terminal.
///
/// C: `CDEV_CTTY 0x40000000` (`com.h:956`). Masked off together with
/// [`CDEV_CLONED`] when extracting the cloned minor number.
pub const CDEV_CTTY: i32 = 0x4000_0000;

/// Re-exported outcome codes so callers match on one vocabulary.
pub use minix_types::{EDONTREPLY as NO_REPLY, ERESTART as RESTARTED};

/// Character request kind, one variant per request number.
///
/// C: `CDEV_OPEN` through `CDEV_SELECT` (`com.h:926-932`), each defined as
/// the request base plus a small index from zero to six. The numeric index
/// is preserved by [`CdevRequest::index`] so logs stay comparable with C.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CdevRequest {
    /// Open a minor device (`CDEV_OPEN`, base plus zero).
    Open,
    /// Close a minor device (`CDEV_CLOSE`, base plus one).
    Close,
    /// Read bytes into a caller grant (`CDEV_READ`, base plus two).
    Read,
    /// Write bytes from a caller grant (`CDEV_WRITE`, base plus three).
    Write,
    /// Device-specific control operation (`CDEV_IOCTL`, base plus four).
    Ioctl,
    /// Cancel a parked request (`CDEV_CANCEL`, base plus five).
    Cancel,
    /// Poll readiness without transferring data (`CDEV_SELECT`, base plus six).
    Select,
}

impl CdevRequest {
    /// Small index of the request (zero for open through six for select).
    pub const fn index(self) -> i32 {
        match self {
            CdevRequest::Open => 0,
            CdevRequest::Close => 1,
            CdevRequest::Read => 2,
            CdevRequest::Write => 3,
            CdevRequest::Ioctl => 4,
            CdevRequest::Cancel => 5,
            CdevRequest::Select => 6,
        }
    }

    /// Full message type of the request (base plus index).
    pub const fn message_type(self) -> i32 {
        CDEV_REQUEST_BASE + self.index()
    }

    /// Decode a raw message type into a request, or reject it.
    ///
    /// Returns `None` for anything outside the seven known requests, which
    /// the caller reports as "invalid argument", matching the C default arm
    /// that panics on unknown requests inside the reply builder
    /// (`chardriver.c:269-270`) after the process function has already
    /// filtered non-requests to the generic handler.
    pub const fn decode(message_type: i32) -> Option<CdevRequest> {
        match message_type - CDEV_REQUEST_BASE {
            0 => Some(CdevRequest::Open),
            1 => Some(CdevRequest::Close),
            2 => Some(CdevRequest::Read),
            3 => Some(CdevRequest::Write),
            4 => Some(CdevRequest::Ioctl),
            5 => Some(CdevRequest::Cancel),
            6 => Some(CdevRequest::Select),
            _ => None,
        }
    }
}

/// Character reply kind.
///
/// C: `CDEV_REPLY`, `CDEV_SEL1_REPLY`, `CDEV_SEL2_REPLY` (`com.h:935-937`).
/// Ordinary requests share one general reply; the two select paths use
/// dedicated reply shapes so the polling service can tell an immediate
/// answer from a later readiness notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CdevReplyKind {
    /// General reply for open, close, read, write, input-output control,
    /// and cancel (`CDEV_REPLY`).
    General,
    /// Immediate answer to a select poll (`CDEV_SEL1_REPLY`).
    SelectImmediate,
    /// Later readiness notification for a select poll (`CDEV_SEL2_REPLY`).
    SelectNotify,
}

/// Returns true when a raw message type is a character request.
///
/// C: `IS_CDEV_RQ(type)` (`com.h:922`), which checks that every bit except
/// the low seven equals the request base. The mask keeps the check working
/// for the whole small request family with one comparison.
pub const fn is_char_request(message_type: i32) -> bool {
    (message_type & !0x7f) == CDEV_REQUEST_BASE
}

/// Minor device number of a character device.
///
/// C: `devminor_t` (an unsigned minor number carried in every request).
/// Kept as a transparent wrapper so a bare integer can never be passed
/// where a device number is expected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DeviceMinor(pub u32);

/// Opaque identifier matching one parked request with its later reply.
///
/// C: `cdev_id_t` (`chardriver.h`), an unsigned value echoed back in every
/// general reply. The framework never interprets it; it only carries it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RequestId(pub u32);

/// Set of minor devices opened since the last announce.
///
/// C: `open_devs` plus `next_open_devs_slot` plus the three helpers
/// `clear_open_devs`, `is_open_dev`, `set_open_dev` (`chardriver.c:54-94`).
/// The C code panics when the table overflows; this type reports the same
/// situation as a boolean so the server — not the library — decides whether
/// a full table is fatal for its device.
#[derive(Debug, Clone)]
pub struct OpenDeviceSet {
    slots: [u32; MAX_OPEN_DEVICES],
    len: usize,
}

impl OpenDeviceSet {
    /// Empty set, as right after an announce.
    pub const fn new() -> OpenDeviceSet {
        OpenDeviceSet {
            slots: [0; MAX_OPEN_DEVICES],
            len: 0,
        }
    }

    /// Forget every recorded device (fresh start or restart).
    ///
    /// C: `clear_open_devs` (`chardriver.c:61-65`).
    pub fn clear(&mut self) {
        self.len = 0;
    }

    /// Number of recorded devices.
    pub fn len(&self) -> usize {
        self.len
    }

    /// True when the set holds no device.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// True when the device was recorded before.
    ///
    /// C: `is_open_dev` (`chardriver.c:70-80`), a linear scan, which is
    /// fine for at most a few hundred entries on a slow control path.
    pub fn contains(&self, minor: DeviceMinor) -> bool {
        self.slots[..self.len].contains(&minor.0)
    }

    /// Record a device; returns false when the table is already full.
    ///
    /// C: `set_open_dev` (`chardriver.c:85-94`), which panics on overflow.
    /// Returning false instead of panicking keeps the "no reply" and
    /// "error reply" policy in the server layer where the message context
    /// is available.
    pub fn insert(&mut self, minor: DeviceMinor) -> bool {
        if self.contains(minor) {
            return true;
        }
        if self.len >= MAX_OPEN_DEVICES {
            return false;
        }
        self.slots[self.len] = minor.0;
        self.len += 1;
        true
    }
}

impl Default for OpenDeviceSet {
    fn default() -> Self {
        OpenDeviceSet::new()
    }
}

/// Default result when a device provides no read or write callback.
///
/// C: `EIO` in `do_transfer` (`chardriver.c:355`). A character device that
/// cannot move bytes is an input-output error, not "unimplemented": the
/// request itself is valid, the device simply cannot serve it.
pub const NO_TRANSFER_HOOK: i32 = EIO;

/// Default result when a device provides no control callback.
///
/// C: `ENOTTY` in `do_ioctl` (`chardriver.c:375`). "Inappropriate control
/// operation for this device" is exactly what the control request means.
pub const NO_IOCTL_HOOK: i32 = ENOTTY;

/// Default result when a device provides no poll callback.
///
/// C: `EBADF` in `do_select` (`chardriver.c:425`). The polling service
/// treats this as "this device cannot be polled".
pub const NO_SELECT_HOOK: i32 = EBADF;

/// Default result when a device provides no cancel callback.
///
/// C: `EDONTREPLY` in `do_cancel` (`chardriver.c:403`): without a cancel
/// hook the framework lets the parked request run to completion instead of
/// answering cancel itself.
pub const NO_CANCEL_HOOK: i32 = EDONTREPLY;

/// Error used when a block-side open reaches a character driver.
///
/// C: `ENXIO` in `do_block_open` (`chardriver.c:446`). A block device node
/// accidentally bound to a character driver has no such device here.
pub const BLOCK_OPEN_MISMATCH: i32 = ENXIO;

/// Error used when a request carries no usable minor number.
///
/// C: `EINVAL` returned by `chardriver_get_minor` (`chardriver.c:598`) for
/// message types outside the seven requests.
pub const BAD_MINOR: i32 = EINVAL;

/// Success code.
pub const SUCCESS: i32 = OK;

/// Reply-suppression sentinel: the handler will answer later, or the cancel
/// path intentionally sends nothing.
///
/// C: `EDONTREPLY` (see `chardriver.c:203-223`).
pub const SUPPRESS_REPLY: i32 = EDONTREPLY;

/// Obsolete suspend marker, still rejected loudly when seen.
///
/// C: `SUSPEND` (`com.h:1151`); `chardriver.c:225-226` panics on it because
/// the old synchronous protocol was retired in 2013.
pub const OBSOLETE_SUSPEND: i32 = minix_types::SUSPEND;

/// Restart marker: never sent back to the caller.
///
/// C: `ERESTART` (`chardriver.c:228-233`). The only possible caller, the
/// virtual file system service, learns about restarts through other means.
pub const RESTART_MARKER: i32 = ERESTART;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_indices_match_c_offsets() {
        assert_eq!(CdevRequest::Open.message_type(), 0x400);
        assert_eq!(CdevRequest::Close.message_type(), 0x401);
        assert_eq!(CdevRequest::Read.message_type(), 0x402);
        assert_eq!(CdevRequest::Write.message_type(), 0x403);
        assert_eq!(CdevRequest::Ioctl.message_type(), 0x404);
        assert_eq!(CdevRequest::Cancel.message_type(), 0x405);
        assert_eq!(CdevRequest::Select.message_type(), 0x406);
    }

    #[test]
    fn test_decode_round_trips_all_seven_requests() {
        let all = [
            CdevRequest::Open,
            CdevRequest::Close,
            CdevRequest::Read,
            CdevRequest::Write,
            CdevRequest::Ioctl,
            CdevRequest::Cancel,
            CdevRequest::Select,
        ];
        for request in all {
            assert_eq!(CdevRequest::decode(request.message_type()), Some(request));
        }
    }

    #[test]
    fn test_decode_rejects_surrounding_values() {
        assert_eq!(CdevRequest::decode(CDEV_REQUEST_BASE - 1), None);
        assert_eq!(CdevRequest::decode(CDEV_REQUEST_BASE + 7), None);
        assert_eq!(CdevRequest::decode(0), None);
    }

    #[test]
    fn test_request_classifier_accepts_family_mask() {
        assert!(is_char_request(CdevRequest::Open.message_type()));
        assert!(is_char_request(CdevRequest::Select.message_type()));
        assert!(!is_char_request(0x500));
        assert!(!is_char_request(0));
    }

    #[test]
    fn test_open_set_records_and_finds_devices() {
        let mut set = OpenDeviceSet::new();
        assert!(set.is_empty());
        assert!(set.insert(DeviceMinor(3)));
        assert!(set.insert(DeviceMinor(7)));
        assert_eq!(set.len(), 2);
        assert!(set.contains(DeviceMinor(3)));
        assert!(!set.contains(DeviceMinor(4)));
    }

    #[test]
    fn test_open_set_insert_is_idempotent() {
        let mut set = OpenDeviceSet::new();
        assert!(set.insert(DeviceMinor(9)));
        assert!(set.insert(DeviceMinor(9)));
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn test_open_set_clear_forgets_everything_after_restart() {
        let mut set = OpenDeviceSet::new();
        set.insert(DeviceMinor(1));
        set.insert(DeviceMinor(2));
        set.clear();
        assert!(set.is_empty());
        assert!(!set.contains(DeviceMinor(1)));
    }

    #[test]
    fn test_default_hook_codes_match_c_behavior() {
        assert_eq!(NO_TRANSFER_HOOK, EIO);
        assert_eq!(NO_IOCTL_HOOK, ENOTTY);
        assert_eq!(NO_SELECT_HOOK, EBADF);
        assert_eq!(NO_CANCEL_HOOK, EDONTREPLY);
        assert_eq!(BLOCK_OPEN_MISMATCH, ENXIO);
        assert_eq!(BAD_MINOR, EINVAL);
    }

    #[test]
    fn test_clone_flag_constants_match_com_header() {
        assert_eq!(CDEV_NONBLOCK, 0x01);
        assert_eq!(CDEV_CLONED as u32, 0x2000_0000);
        assert_eq!(CDEV_CTTY as u32, 0x4000_0000);
    }
}
