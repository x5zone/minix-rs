//! Character-driver framework contract: the shared front door of input.
//!
//! C: `minix3/minix/lib/libchardriver/chardriver.c` (600 lines) plus
//! `minix3/minix/include/minix/chardriver.h` (36 lines). This library is
//! *shared* infrastructure — the input server, the terminal driver, and every
//! hardware character driver all run their main loop through it. The input
//! server contributes only a seven-slot callback table (document 01); every
//! judgment in this module (which message goes where, when a reply is owed,
//! who counts as already open) is the framework's, identical for every
//! driver that uses it.
//!
//! Three judgments, each a pure function so each can be tested without any
//! message transport:
//!
//! 1. **Classification** ([`classify_request`]): what kind of arrival is this?
//! 2. **Restart gate** ([`gate_character_request`]): has this device been
//!    opened since the last restart, or is this a stale request from before
//!    it?
//! 3. **Reply discipline** ([`decide_reply`]): does the handler's answer get
//!    sent, held back, or rejected as a framework violation?
//!
//! Corresponding document: `02-chardriver-framework.md`.

use minix_types::{EDONTREPLY, ERESTART};

// ── Message numbers (com.h) ──

/// Base of character-device request numbers (`CDEV_RQ_BASE = 0x400`).
pub const CHARACTER_REQUEST_BASE: i32 = 0x400;
/// Base of character-device response numbers (`CDEV_RS_BASE = 0x480`).
pub const CHARACTER_RESPONSE_BASE: i32 = 0x480;
/// A block-device open request (`BDEV_OPEN = BDEV_RQ_BASE + 0 = 0x500`).
///
/// The framework answers these itself with an error instead of passing them
/// to the character handlers (see [`Incoming::BlockOpen`]).
pub const BLOCK_DEVICE_OPEN: i32 = 0x500;

/// How many minors the already-opened set remembers.
///
/// C: `MAX_NR_OPEN_DEVICES 256` (`driver.h:41`). The set is a plain array —
/// 256 entries fit comfortably, and restarts clear it rather than grow it.
pub const MAX_OPEN_DEVICES: usize = 256;

// ── Request and response kinds ──

/// The seven character-device requests (`CDEV_*`, `com.h:926-932`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum CharacterRequest {
    /// Open a minor device (`CDEV_OPEN = 0x400`).
    Open = 0x400,
    /// Close a minor device (`CDEV_CLOSE = 0x401`).
    Close = 0x401,
    /// Read bytes into the caller's grant (`CDEV_READ = 0x402`).
    Read = 0x402,
    /// Write bytes from the caller's grant (`CDEV_WRITE = 0x403`).
    Write = 0x403,
    /// Device control operation (`CDEV_IOCTL = 0x404`).
    Control = 0x404,
    /// Cancel a parked request (`CDEV_CANCEL = 0x405`).
    Cancel = 0x405,
    /// Ask whether operations would succeed (`CDEV_SELECT = 0x406`).
    Select = 0x406,
}

impl CharacterRequest {
    /// Interprets a message type; non-character types are `None`.
    ///
    /// C: `IS_CDEV_RQ(type)` (`com.h:922`) gates the same decision in
    /// `chardriver_process` (`chardriver.c:494`).
    pub const fn from_message_type(message_type: i32) -> Option<Self> {
        match message_type {
            0x400 => Some(CharacterRequest::Open),
            0x401 => Some(CharacterRequest::Close),
            0x402 => Some(CharacterRequest::Read),
            0x403 => Some(CharacterRequest::Write),
            0x404 => Some(CharacterRequest::Control),
            0x405 => Some(CharacterRequest::Cancel),
            0x406 => Some(CharacterRequest::Select),
            _ => None,
        }
    }

    /// Whether this request may park (answer later instead of now).
    ///
    /// C: `chardriver_reply` (`chardriver.c:203-223`) accepts the
    /// hold-back sentinel only for task requests that move bulk data or wait
    /// (read, write, control, cancel). Open, close, and select always answer
    /// immediately — parking one would leave the caller blocked with no
    /// wake-up path.
    pub const fn may_park(self) -> bool {
        match self {
            CharacterRequest::Read
            | CharacterRequest::Write
            | CharacterRequest::Control
            | CharacterRequest::Cancel => true,
            CharacterRequest::Open | CharacterRequest::Close | CharacterRequest::Select => false,
        }
    }
}

/// The three character-device responses (`com.h:935-937`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum CharacterResponse {
    /// General answer carrying a status (`CDEV_REPLY = 0x480`).
    General = 0x480,
    /// Immediate answer to a select query (`CDEV_SEL1_REPLY = 0x481`).
    SelectImmediate = 0x481,
    /// Later notification for a select waiter (`CDEV_SEL2_REPLY = 0x482`).
    SelectNotify = 0x482,
}

// ── Access, flag, and select bits (com.h) ──

/// Open requested read access (`CDEV_R_BIT = 0x01`).
pub const ACCESS_READ: i32 = 0x01;
/// Open requested write access (`CDEV_W_BIT = 0x02`).
pub const ACCESS_WRITE: i32 = 0x02;
/// Open must not take the controlling terminal (`CDEV_NOCTTY = 0x04`).
pub const ACCESS_NO_CONTROLLING_TERMINAL: i32 = 0x04;

/// No transfer flags (`CDEV_NOFLAGS = 0x00`).
pub const TRANSFER_NO_FLAGS: i32 = 0x00;
/// Do not park the request; fail instead (`CDEV_NONBLOCK = 0x01`).
pub const TRANSFER_NON_BLOCKING: i32 = 0x01;

/// Select for readability (`CDEV_OP_RD = 0x01`).
pub const SELECT_READ: i32 = 0x01;
/// Select for writability (`CDEV_OP_WR = 0x02`).
pub const SELECT_WRITE: i32 = 0x02;
/// Select for error condition (`CDEV_OP_ERR = 0x04`).
pub const SELECT_ERROR: i32 = 0x04;
/// Remember me and notify later (`CDEV_NOTIFY = 0x08`).
pub const SELECT_NOTIFY: i32 = 0x08;

/// Open answer flag: the device is a fresh clone (`CDEV_CLONED`).
pub const OPEN_CLONED: i32 = 0x2000_0000;
/// Open answer flag: the device is the controlling terminal (`CDEV_CTTY`).
pub const OPEN_CONTROLLING_TERMINAL: i32 = 0x4000_0000;

// ── Judgment 1: classification ──

/// Where an arriving message goes.
///
/// C: `chardriver_process` (`chardriver.c:455-532`). Notifications are never
/// answered; block opens are denied; character requests go through the
/// restart gate to a handler; everything else belongs to the driver's
/// catch-all (`cdr_other`), which the framework also leaves unanswered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Incoming {
    /// A character-device request for the restart gate and a handler.
    CharacterRequest(CharacterRequest),
    /// A block-device open aimed at a character driver: deny with `ENXIO`.
    ///
    /// C: `do_block_open` (`chardriver.c:438-450`). Without this denial, a
    /// caller that opens a block-device node bound to a character driver
    /// would block forever waiting for an answer that never comes.
    BlockOpen,
    /// Anything else: the driver's catch-all sees it, the framework sends
    /// no reply.
    OtherMessage,
}

/// Source of a notification arrival.
///
/// C: `chardriver_process` (`chardriver.c:464-482`) sorts notifications by
/// sender: hardware interrupts go to the interrupt hook, clock ticks to the
/// alarm hook, everything else to the catch-all. The input server registers
/// neither an interrupt nor an alarm hook (its table holds seven slots, not
/// ten — document 01), so only [`NotifySource::Other`] reaches input code;
/// the other two variants exist so the classification stays total for every
/// framework user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotifySource {
    /// A hardware interrupt (`HARDWARE` sender).
    HardwareInterrupt,
    /// A clock tick (`CLOCK` sender).
    ClockAlarm,
    /// Any other sender (for input: the data-store server announcing driver
    /// arrivals and departures — document 11).
    Other,
}

/// Classifies one arrival.
///
/// `is_notify` mirrors the transport's notify bit (`is_ipc_notify`); block
/// opens are detected by number. Pure function: no transport, no global
/// state, fully testable.
pub const fn classify_request(message_type: i32, is_notify: bool) -> Option<Incoming> {
    if is_notify {
        return None; // Notifications classify by sender, see NotifySource.
    }
    match CharacterRequest::from_message_type(message_type) {
        Some(request) => Some(Incoming::CharacterRequest(request)),
        None => {
            if message_type == BLOCK_DEVICE_OPEN {
                Some(Incoming::BlockOpen)
            } else {
                Some(Incoming::OtherMessage)
            }
        }
    }
}

// ── Judgment 2: the restart gate ──

/// The restart gate's verdict for one character request.
///
/// C: `chardriver_process` (`chardriver.c:503-513`). After a restart, callers
/// may still hold grants and request identifiers from before the crash.
/// Serving those would answer the wrong generation of a caller, so the
/// framework drops every request for a device that has not been opened since
/// the restart — except the open itself, which is both recorded and served.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateVerdict {
    /// Serve the request (the device was opened after the restart).
    Serve,
    /// Record the open, then serve it (first open since the restart).
    RecordAndServe,
    /// Drop the request silently: no handler call, no reply.
    DropAsStale,
}

/// Applies the restart gate.
///
/// `already_open` is whether the minor is in the already-opened set.
pub const fn gate_character_request(request: CharacterRequest, already_open: bool) -> GateVerdict {
    if already_open {
        return GateVerdict::Serve;
    }
    match request {
        CharacterRequest::Open => GateVerdict::RecordAndServe,
        _ => GateVerdict::DropAsStale,
    }
}

/// The already-opened set: which minors opened since the last restart.
///
/// C: `open_devs` plus `next_open_devs_slot` (`chardriver.c:54-94`): a plain
/// array with linear search, cleared on every announce. Linear search is
/// kept deliberately — 256 entries at most, restarts clear the set, and a
/// hash table would trade a dependency and hashing surprises for no
/// measurable gain on a per-message path this cold.
#[derive(Debug, Clone, Copy)]
pub struct OpenDeviceSet {
    /// Minors in opening order; only `len` entries are meaningful.
    minors: [i32; MAX_OPEN_DEVICES],
    /// How many entries of `minors` are meaningful.
    len: usize,
}

impl OpenDeviceSet {
    /// The cleared set: nothing opened yet (also the post-restart state).
    ///
    /// C: `clear_open_devs` (`chardriver.c:61-65`) runs on every announce.
    pub const fn cleared() -> Self {
        Self {
            minors: [0; MAX_OPEN_DEVICES],
            len: 0,
        }
    }

    /// Whether the minor was opened since the last restart.
    ///
    /// C: `is_open_dev` (`chardriver.c:70-80`).
    pub fn contains(self, minor: i32) -> bool {
        self.minors[0..self.len].contains(&minor)
    }

    /// Records a minor as opened.
    ///
    /// C: `set_open_dev` (`chardriver.c:85-94`), which crashes the driver
    /// when the 256 slots run out — losing track of opens would silently
    /// drop legitimate requests, so C refuses to continue. Rust reports the
    /// overflow instead of crashing the server: `false` means "not
    /// recorded", and the caller (a future open path) must answer with an
    /// error rather than serve an untracked device.
    pub fn record(&mut self, minor: i32) -> bool {
        if self.len >= MAX_OPEN_DEVICES {
            return false;
        }
        self.minors[self.len] = minor;
        self.len += 1;
        true
    }

    /// How many minors are currently recorded.
    pub const fn len(self) -> usize {
        self.len
    }

    /// Whether nothing is recorded (fresh boot or just restarted).
    pub const fn is_empty(self) -> bool {
        self.len == 0
    }
}

// ── Judgment 3: reply discipline ──

/// What the framework does with a handler's answer.
///
/// C: `chardriver_reply` (`chardriver.c:195-274`). Most answers are sent.
/// Two are held back, for different reasons — and one combination is a
/// framework violation that C answers with a crash.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplyDecision {
    /// Send the answer to the caller now.
    Send,
    /// Send nothing: the request parked and will be answered later through
    /// the wake-up channel (document 07).
    ///
    /// C: the `EDONTREPLY` sentinel (`chardriver.c:203-223`).
    Parked,
    /// Send nothing: the server restarted and the caller learns that through
    /// other channels.
    ///
    /// C: the `ERESTART` early return (`chardriver.c:232-233`). The caller is
    /// the virtual file system, which is not ready to handle a restart
    /// error, so the framework stays silent rather than confuse it.
    Restarted,
    /// The handler parked a request that must always answer immediately
    /// (open, close, or select). C crashes here
    /// (`chardriver.c:220-221`); Rust names the violation so the main loop
    /// can log it and answer with an error instead of dying.
    InvalidParking,
}

/// Applies the reply discipline to one handler answer.
///
/// `status` is the handler's raw answer (`OK`, an errno, `EDONTREPLY`, or
/// `ERESTART`) in the `minix_types` user-space positive convention (C spells
/// these `_SIGN 203` / `_SIGN 200`, negative under `_SYSTEM`; the meaning is
/// identical). Pure function over (request, status).
pub const fn decide_reply(request: CharacterRequest, status: i32) -> ReplyDecision {
    if status == EDONTREPLY {
        if request.may_park() {
            return ReplyDecision::Parked;
        }
        return ReplyDecision::InvalidParking;
    }
    if status == ERESTART {
        return ReplyDecision::Restarted;
    }
    ReplyDecision::Send
}

/// An immediate task answer: status plus the request identifier it echoes.
///
/// C: `mess_lchardriver_vfs_reply` (`ipc.h:939-948`): `CDEV_REPLY` carries
/// `status` and the request `id` so the caller can match the answer to one
/// of possibly several outstanding requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskAnswer {
    /// Handler outcome (`OK` or an errno).
    pub status: i32,
    /// Echo of the request identifier.
    pub request_id: u32,
}

impl TaskAnswer {
    /// The message type carrying this answer (`CDEV_REPLY`).
    pub const fn message_type(self) -> i32 {
        CharacterResponse::General as i32
    }
}

/// An immediate select answer: status plus the minor it concerns.
///
/// C: `mess_lchardriver_vfs_sel1` (`ipc.h:950-957`): `CDEV_SEL1_REPLY`
/// carries `status` and `minor` (select has no request identifier; the minor
/// plays that role).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectAnswer {
    /// Readiness outcome for the queried operations.
    pub status: i32,
    /// The minor the query concerned.
    pub minor: i32,
}

impl SelectAnswer {
    /// The message type carrying this answer (`CDEV_SEL1_REPLY`).
    pub const fn message_type(self) -> i32 {
        CharacterResponse::SelectImmediate as i32
    }
}

/// A later select notification for a recorded waiter.
///
/// C: `mess_lchardriver_vfs_sel2` (`ipc.h:959-965`): `CDEV_SEL2_REPLY`
/// carries the same two fields as the immediate answer but travels through
/// the asynchronous wake-up channel (`chardriver_reply_select`,
/// `chardriver.c:153-172`), not the request/reply path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectNotification {
    /// Readiness outcome for the waited-for operations.
    pub status: i32,
    /// The minor that became ready.
    pub minor: i32,
}

impl SelectNotification {
    /// The message type carrying this notification (`CDEV_SEL2_REPLY`).
    pub const fn message_type(self) -> i32 {
        CharacterResponse::SelectNotify as i32
    }
}

// ── Announce effects ──

/// One effect of announcing the server after a fresh start or a restart.
///
/// C: `chardriver_announce` (`chardriver.c:99-124`) does three things in
/// order, and the order matters: first unblock callers stuck on the dead
/// generation, then publish the arrival, then forget the old opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnnounceEffect {
    /// Ask the kernel to release callers blocked on the previous generation
    /// (`sys_statectl(SYS_STATE_CLEAR_IPC_REFS)`).
    ReleaseBlockedCallers,
    /// Publish the `drv.chr.<label>` arrival marker for the virtual file
    /// system (`ds_publish_u32` with `DS_DRIVER_UP`).
    PublishArrival,
    /// Forget every previously opened minor (the restart gate starts over).
    ForgetOpenDevices,
}

/// The announce sequence, in C order.
pub const fn announce_effects() -> [AnnounceEffect; 3] {
    [
        AnnounceEffect::ReleaseBlockedCallers,
        AnnounceEffect::PublishArrival,
        AnnounceEffect::ForgetOpenDevices,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_numbers_match_c() {
        // C: `com.h:915-937` (bases, requests, responses).
        assert_eq!(CHARACTER_REQUEST_BASE, 0x400);
        assert_eq!(CHARACTER_RESPONSE_BASE, 0x480);
        assert_eq!(CharacterRequest::Open as i32, 0x400);
        assert_eq!(CharacterRequest::Select as i32, 0x406);
        assert_eq!(CharacterResponse::General as i32, 0x480);
        assert_eq!(CharacterResponse::SelectImmediate as i32, 0x481);
        assert_eq!(CharacterResponse::SelectNotify as i32, 0x482);
        assert_eq!(BLOCK_DEVICE_OPEN, 0x500);
        // C: `driver.h:41`.
        assert_eq!(MAX_OPEN_DEVICES, 256);
    }

    #[test]
    fn test_flag_bits_match_c() {
        // C: `com.h:939-957`.
        assert_eq!(ACCESS_READ, 0x01);
        assert_eq!(ACCESS_WRITE, 0x02);
        assert_eq!(ACCESS_NO_CONTROLLING_TERMINAL, 0x04);
        assert_eq!(TRANSFER_NO_FLAGS, 0x00);
        assert_eq!(TRANSFER_NON_BLOCKING, 0x01);
        assert_eq!(SELECT_READ, 0x01);
        assert_eq!(SELECT_WRITE, 0x02);
        assert_eq!(SELECT_ERROR, 0x04);
        assert_eq!(SELECT_NOTIFY, 0x08);
        assert_eq!(OPEN_CLONED, 0x2000_0000);
        assert_eq!(OPEN_CONTROLLING_TERMINAL, 0x4000_0000);
    }

    #[test]
    fn test_classify_request_routes_like_chardriver_process() {
        // Character requests reach the gate ...
        for raw in 0x400..=0x406 {
            let request = CharacterRequest::from_message_type(raw).unwrap();
            assert_eq!(
                classify_request(raw, false),
                Some(Incoming::CharacterRequest(request))
            );
        }
        // ... a block open is denied, not served ...
        assert_eq!(
            classify_request(BLOCK_DEVICE_OPEN, false),
            Some(Incoming::BlockOpen)
        );
        // ... anything else belongs to the catch-all ...
        assert_eq!(
            classify_request(0x1234, false),
            Some(Incoming::OtherMessage)
        );
        // ... and notifications never enter this classifier (they sort by
        // sender — `chardriver.c:464-482`).
        assert_eq!(classify_request(0x402, true), None);
        // Out-of-range numbers just below and above the request window.
        assert_eq!(CharacterRequest::from_message_type(0x3FF), None);
        assert_eq!(CharacterRequest::from_message_type(0x407), None);
    }

    #[test]
    fn test_restart_gate_matches_c() {
        // Opened devices serve everything (`chardriver.c:506` passes).
        for raw in 0x400..=0x406 {
            let request = CharacterRequest::from_message_type(raw).unwrap();
            assert_eq!(gate_character_request(request, true), GateVerdict::Serve);
        }
        // Unopened devices serve only the open, which is recorded first
        // (`chardriver.c:506-513`).
        assert_eq!(
            gate_character_request(CharacterRequest::Open, false),
            GateVerdict::RecordAndServe
        );
        for raw in 0x401..=0x406 {
            let request = CharacterRequest::from_message_type(raw).unwrap();
            assert_eq!(
                gate_character_request(request, false),
                GateVerdict::DropAsStale
            );
        }
    }

    #[test]
    fn test_open_device_set_behaves_like_c_array() {
        let mut set = OpenDeviceSet::cleared();
        assert!(set.is_empty());
        assert!(!set.contains(1));
        assert!(set.record(1));
        assert!(set.record(64));
        assert!(set.contains(1));
        assert!(set.contains(64));
        assert!(!set.contains(2));
        assert_eq!(set.len(), 2);
        // Restart clears everything (`chardriver_announce` tail).
        set = OpenDeviceSet::cleared();
        assert!(set.is_empty());
        assert!(!set.contains(1));
    }

    #[test]
    fn test_decide_reply_matches_chardriver_reply() {
        // Ordinary answers are sent (`chardriver.c:235-273`).
        assert_eq!(decide_reply(CharacterRequest::Read, 0), ReplyDecision::Send);
        assert_eq!(decide_reply(CharacterRequest::Open, 6), ReplyDecision::Send);
        // The hold-back sentinel parks only the four parkable requests
        // (`chardriver.c:203-223`).
        for raw in [0x402, 0x403, 0x404, 0x405] {
            let request = CharacterRequest::from_message_type(raw).unwrap();
            assert_eq!(
                decide_reply(request, EDONTREPLY),
                ReplyDecision::Parked,
                "request {raw:#X} must park"
            );
        }
        // Parking open, close, or select is a framework violation (C crashes
        // at `chardriver.c:220-221`; Rust names it).
        for raw in [0x400, 0x401, 0x406] {
            let request = CharacterRequest::from_message_type(raw).unwrap();
            assert_eq!(
                decide_reply(request, EDONTREPLY),
                ReplyDecision::InvalidParking,
                "request {raw:#X} must not park"
            );
        }
        // Restart answers stay silent for every request kind
        // (`chardriver.c:232-233`).
        for raw in 0x400..=0x406 {
            let request = CharacterRequest::from_message_type(raw).unwrap();
            assert_eq!(decide_reply(request, ERESTART), ReplyDecision::Restarted);
        }
    }

    #[test]
    fn test_answer_message_types_match_c() {
        // C: `CDEV_REPLY` / `CDEV_SEL1_REPLY` / `CDEV_SEL2_REPLY`.
        let task = TaskAnswer {
            status: 0,
            request_id: 7,
        };
        assert_eq!(task.message_type(), 0x480);
        let select = SelectAnswer {
            status: 1,
            minor: 0,
        };
        assert_eq!(select.message_type(), 0x481);
        let notify = SelectNotification {
            status: 1,
            minor: 0,
        };
        assert_eq!(notify.message_type(), 0x482);
    }

    #[test]
    fn test_announce_effects_follow_c_order() {
        // C: `chardriver_announce` (`chardriver.c:99-124`) — unblock first
        // (or the old callers stay stuck), publish second, forget last.
        assert_eq!(
            announce_effects(),
            [
                AnnounceEffect::ReleaseBlockedCallers,
                AnnounceEffect::PublishArrival,
                AnnounceEffect::ForgetOpenDevices,
            ]
        );
    }
}
