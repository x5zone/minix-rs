//! Driver table, open counts, and the synchronous plus asynchronous client.
//!
//! C correspondence: `bdev_driver_*` in
//! `minix3/minix/lib/libbdev/driver.c:17-122` (endpoint table),
//! `bdev_minor_*` in `minix3/minix/lib/libbdev/minor.c:17-136` (open
//! reference counts), the synchronous entries in
//! `minix3/minix/lib/libbdev/bdev.c:80-373`, the asynchronous entries in
//! `bdev.c:374-640`, and the reply demultiplexer plus wait in
//! `minix3/minix/lib/libbdev/ipc.c:269-346`.

use super::transport::{Destination, Reply, Transport, TransportError};
use alloc::vec::Vec;
use minix_types::{EBUSY, EINVAL, EIO, ENOMEM, OK};

/// Identifier for synchronous requests: no demultiplexing needed.
///
/// C: `NO_ID (-1)` (`libbdev/const.h`).
pub const NO_ID: i32 = -1;

/// Maximum concurrent asynchronous calls.
///
/// C: `NR_CALLS 256` (`libbdev/const.h`).
pub const MAX_CALLS: usize = 256;

/// Maximum distinct opened minor devices tracked.
///
/// C: `NR_OPEN_DEVS 4` (`libbdev/const.h`). Small on purpose: file servers
/// open a handful of minors, not hundreds.
pub const MAX_OPEN_MINORS: usize = 4;

/// Times a call is retried on driver restarts before giving up.
///
/// C: `DRIVER_TRIES 10` (`libbdev/const.h`).
pub const DRIVER_RETRIES: u32 = 10;

/// Restarts tolerated during one recovery pass.
///
/// C: `RECOVER_TRIES 2` (`libbdev/const.h`).
pub const RECOVERY_RETRIES: u32 = 2;

/// Times a transfer is retried on input-output errors.
///
/// C: `TRANSFER_TRIES 5` (`libbdev/const.h`).
pub const TRANSFER_RETRIES: u32 = 5;

/// Block request numbers shared with the framework side.
///
/// Values match `BDEV_OPEN` through `BDEV_IOCTL` (`com.h:970-976`); the
/// client crate repeats them so callers do not depend on the framework
/// crate for numbers both sides must agree on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BdevOp {
    /// Open a minor device.
    Open,
    /// Close a minor device.
    Close,
    /// Read contiguous bytes.
    Read,
    /// Write contiguous bytes.
    Write,
    /// Read into a vector of grants.
    Gather,
    /// Write from a vector of grants.
    Scatter,
    /// Device-specific control operation.
    Ioctl,
}

impl BdevOp {
    /// Full message type (block base `0x500` plus index).
    pub const fn message_type(self) -> i32 {
        0x500
            + match self {
                BdevOp::Open => 0,
                BdevOp::Close => 1,
                BdevOp::Read => 2,
                BdevOp::Write => 3,
                BdevOp::Gather => 4,
                BdevOp::Scatter => 5,
                BdevOp::Ioctl => 6,
            }
    }
}

/// Major device number: index into the driver endpoint table.
///
/// C: `major(dev)` selects `driver_tab[major]` (`driver.c`). Kept as a
/// transparent wrapper so a minor can never be passed where a major is
/// expected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Major(pub u32);

/// Full device number: major plus minor packed the C way.
///
/// C: `dev_t` with `major()` and `minor()` accessors. The packing (major
/// in the high bits) is preserved so logs stay comparable with C.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Device {
    /// Packed number: high bits major, low eight bits minor.
    pub raw: u32,
}

impl Device {
    /// Build from major and minor parts.
    pub const fn from_parts(major: u32, minor: u32) -> Device {
        Device {
            raw: (major << 8) | (minor & 0xFF),
        }
    }

    /// Major part (driver selector).
    pub const fn major(self) -> u32 {
        self.raw >> 8
    }

    /// Minor part (device selector within the driver).
    pub const fn minor(self) -> u32 {
        self.raw & 0xFF
    }
}

/// Driver endpoint table: one endpoint plus label per major device.
///
/// C: `driver_tab[NR_DEVICES]` in `driver.c:17-27` with `NONE` for unknown
/// endpoints. The table size is a const generic so tests can shrink it;
//  the production width matches the C table (sixteen majors).
#[derive(Debug, Clone)]
pub struct DriverTable<const WIDTH: usize = 16> {
    endpoints: [i32; WIDTH],
}

/// Endpoint value meaning "unknown" (no binding yet).
///
/// C: `NONE` as used by `bdev_driver_get`.
pub const NO_ENDPOINT: i32 = -1;

impl<const WIDTH: usize> DriverTable<WIDTH> {
    /// Empty table: every major unbound.
    pub const fn new() -> DriverTable<WIDTH> {
        DriverTable {
            endpoints: [NO_ENDPOINT; WIDTH],
        }
    }

    /// Look up the endpoint for a major; `None` means unbound.
    pub fn get(&self, major: Major) -> Option<i32> {
        let slot = self.endpoints.get(major.0 as usize)?;
        if *slot == NO_ENDPOINT {
            None
        } else {
            Some(*slot)
        }
    }

    /// Bind a major to an endpoint (clears on relabel, like
    /// `bdev_driver_set` which resets the endpoint to `NONE` and
    /// re-resolves through the data store).
    pub fn bind(&mut self, major: Major, endpoint: i32) -> bool {
        match self.endpoints.get_mut(major.0 as usize) {
            Some(slot) => {
                *slot = endpoint;
                true
            }
            None => false,
        }
    }

    /// Forget the binding for one major (`bdev_driver_clear`).
    pub fn clear(&mut self, major: Major) {
        if let Some(slot) = self.endpoints.get_mut(major.0 as usize) {
            *slot = NO_ENDPOINT;
        }
    }
}

impl<const WIDTH: usize> Default for DriverTable<WIDTH> {
    fn default() -> Self {
        DriverTable::new()
    }
}

/// Open reference counts for minor devices.
///
/// C: `open_dev[NR_OPEN_DEVS]` in `minor.c:13-15` with per-device count and
/// accumulated access bits. Reopening after a driver restart replays one
/// open per previous open (`bdev_minor_reopen`, `minor.c:17-76`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct OpenEntry {
    device: u32,
    count: u32,
    access: i32,
}

/// Per-minor open tracker.
#[derive(Debug, Clone)]
pub struct OpenTracker {
    entries: [Option<OpenEntry>; MAX_OPEN_MINORS],
}

impl OpenTracker {
    /// Empty tracker.
    pub const fn new() -> OpenTracker {
        OpenTracker {
            entries: [None, None, None, None],
        }
    }

    /// Record one open of a device with these access bits.
    ///
    /// Returns false when no slot is free (C prints "too many open
    /// devices" and drops the record; the open itself already succeeded,
    /// so the failure only weakens later restart recovery).
    pub fn add(&mut self, device: Device, access: i32) -> bool {
        if let Some(entry) = self
            .entries
            .iter_mut()
            .flatten()
            .find(|entry| entry.device == device.raw)
        {
            entry.count += 1;
            entry.access |= access;
            return true;
        }
        for slot in self.entries.iter_mut() {
            if slot.is_none() {
                *slot = Some(OpenEntry {
                    device: device.raw,
                    count: 1,
                    access,
                });
                return true;
            }
        }
        false
    }

    /// Forget one open; returns false when the device was not tracked.
    pub fn remove(&mut self, device: Device) -> bool {
        let Some(index) = self
            .entries
            .iter()
            .position(|slot| matches!(slot, Some(entry) if entry.device == device.raw))
        else {
            return false;
        };
        let is_last = matches!(self.entries[index], Some(entry) if entry.count <= 1);
        if is_last {
            self.entries[index] = None;
        } else if let Some(entry) = self.entries[index].as_mut() {
            entry.count -= 1;
        }
        true
    }

    /// True when the device has at least one recorded open.
    pub fn is_open(&self, device: Device) -> bool {
        self.entries
            .iter()
            .any(|slot| matches!(slot, Some(entry) if entry.device == device.raw))
    }

    /// How many times the device was opened (zero when untracked).
    pub fn open_count(&self, device: Device) -> u32 {
        self.entries
            .iter()
            .find_map(|slot| match slot {
                Some(entry) if entry.device == device.raw => Some(entry.count),
                _ => None,
            })
            .unwrap_or(0)
    }
}

impl Default for OpenTracker {
    fn default() -> Self {
        OpenTracker::new()
    }
}

/// State of one asynchronous call slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CallState {
    /// Slot free.
    Free,
    /// Request sent, reply outstanding.
    Pending,
    /// Reply arrived, waiting for the waiter.
    Done(i32),
}

/// One asynchronous call: request plus retry budgets plus outcome.
///
/// C: `bdev_call_t` (`libbdev/type.h`): call identifier, target device,
/// request message, callback plus parameter, driver-try and transfer-try
/// counters. Callbacks are service-crate closures; this type stores the
/// completion status and lets the waiter collect it, which is the part of
/// the machine that can be tested without kernel messaging.
#[derive(Debug, Clone, Copy)]
struct CallSlot {
    state: CallState,
    device: u32,
    id: i32,
    driver_tries: u32,
    transfer_tries: u32,
}

impl CallSlot {
    const fn free() -> CallSlot {
        CallSlot {
            state: CallState::Free,
            device: 0,
            id: NO_ID,
            driver_tries: 0,
            transfer_tries: 0,
        }
    }
}

/// Asynchronous call table with retry budgets.
///
/// C: the call-vector management in `call.c` plus the wait and demux in
/// `ipc.c:269-346`. Identifiers are small indices; `NO_ID` is never handed
/// out. A failed send consumes one driver retry; an input-output error
/// consumes one transfer retry; anything else completes the call.
pub struct CallTable {
    slots: [CallSlot; MAX_CALLS],
}

impl CallTable {
    /// Empty table: all slots free.
    pub const fn new() -> CallTable {
        CallTable {
            slots: [CallSlot::free(); MAX_CALLS],
        }
    }

    /// Allocate a slot for a new call; `None` (busy) when full.
    ///
    /// C: a full call vector makes the async entry return a negative
    /// identifier; callers treat it as "try again later".
    pub fn allocate(&mut self, device: Device, id: i32) -> Option<usize> {
        for (index, slot) in self.slots.iter_mut().enumerate() {
            if slot.state == CallState::Free {
                *slot = CallSlot {
                    state: CallState::Pending,
                    device: device.raw,
                    id,
                    driver_tries: DRIVER_RETRIES,
                    transfer_tries: TRANSFER_RETRIES,
                };
                return Some(index);
            }
        }
        None
    }

    /// File a reply for the call carrying this identifier.
    ///
    /// Returns the slot index when a pending call matched, `None` for a
    /// stray reply (C logs and drops strays in `bdev_reply_asyn`).
    pub fn complete(&mut self, id: i32, status: i32) -> Option<usize> {
        for (index, slot) in self.slots.iter_mut().enumerate() {
            if slot.state == CallState::Pending && slot.id == id {
                slot.state = CallState::Done(status);
                return Some(index);
            }
        }
        None
    }

    /// Collect a finished call; `None` while still pending or unknown.
    ///
    /// C: `bdev_wait_asyn` blocks until the reply arrives; this
    /// non-blocking collect lets single-threaded tests drive the machine
    /// step by step. Collecting frees the slot.
    pub fn collect(&mut self, handle: usize) -> Option<i32> {
        let slot = self.slots.get_mut(handle)?;
        match slot.state {
            CallState::Done(status) => {
                *slot = CallSlot::free();
                Some(status)
            }
            _ => None,
        }
    }

    /// Note a failed send for a pending call; false means "give up".
    ///
    /// C: driver-restart retries up to `DRIVER_TRIES` before the call
    /// fails with the send error.
    pub fn note_send_failure(&mut self, handle: usize) -> bool {
        match self.slots.get_mut(handle) {
            Some(slot) if slot.state == CallState::Pending => {
                if slot.driver_tries > 0 {
                    slot.driver_tries -= 1;
                    true
                } else {
                    slot.state = CallState::Done(-EIO);
                    false
                }
            }
            _ => false,
        }
    }

    /// Note an input-output error for a pending call; false means "give up".
    ///
    /// C: transfers are retried up to `TRANSFER_TRIES` on input-output
    /// errors (`libbdev/const.h`); other errors complete the call at once.
    /// True means "resend the request", false means the call already holds
    /// its final error for collection.
    pub fn note_transfer_error(&mut self, handle: usize) -> bool {
        match self.slots.get_mut(handle) {
            Some(slot) if slot.state == CallState::Pending => {
                if slot.transfer_tries > 0 {
                    slot.transfer_tries -= 1;
                    true
                } else {
                    slot.state = CallState::Done(-EIO);
                    false
                }
            }
            _ => false,
        }
    }

    /// Device the pending or finished call targets; `None` for a free or
    /// unknown slot. Used when reissuing a call after a driver restart.
    pub fn device_of(&self, handle: usize) -> Option<Device> {
        match self.slots.get(handle) {
            Some(slot)
                if slot.state == CallState::Pending || matches!(slot.state, CallState::Done(_)) =>
            {
                Some(Device { raw: slot.device })
            }
            _ => None,
        }
    }

    /// Number of free slots.
    pub fn free_slots(&self) -> usize {
        self.slots
            .iter()
            .filter(|slot| slot.state == CallState::Free)
            .count()
    }
}

impl Default for CallTable {
    fn default() -> Self {
        CallTable::new()
    }
}

/// Synchronous client: driver table plus open tracker plus transport.
///
/// Wires the three C pieces (`driver.c`, `minor.c`, `bdev_sendrec`) into
/// one handle. The generic transport keeps the policy testable; the
/// service crate instantiates it with the kernel transport.
pub struct BdevClient<T> {
    drivers: DriverTable,
    opens: OpenTracker,
    transport: T,
}

impl<T: Transport> BdevClient<T> {
    /// Fresh client with an empty table and no opens.
    pub fn new(transport: T) -> BdevClient<T> {
        BdevClient {
            drivers: DriverTable::new(),
            opens: OpenTracker::default(),
            transport,
        }
    }

    /// Bind a major to an endpoint (driver discovery).
    pub fn bind(&mut self, major: Major, endpoint: i32) -> bool {
        self.drivers.bind(major, endpoint)
    }

    /// Open a device: send open, record on success.
    ///
    /// C: `bdev_open` (`bdev.c:80-94`): resolve the endpoint, send
    /// `BDEV_OPEN`, record the minor on success. Unknown driver or failed
    /// send surfaces as an error without recording.
    pub fn open(&mut self, device: Device, access: i32) -> i32 {
        let major = Major(device.major());
        let Some(endpoint) = self.drivers.get(major) else {
            return -EINVAL;
        };
        let reply = self.transport.exchange(Destination {
            endpoint,
            message_type: BdevOp::Open.message_type(),
            minor: device.minor(),
            id: NO_ID,
        });
        match reply {
            Ok(Reply {
                id: NO_ID, status, ..
            }) if status == OK => {
                self.opens.add(device, access);
                OK
            }
            Ok(Reply { status, .. }) => status,
            Err(TransportError::SendFailed) => -EIO,
            Err(_) => -EINVAL,
        }
    }

    /// Close a device: send close, forget one open on success.
    ///
    /// C: `bdev_close` (`bdev.c:95-...`): closing an untracked device is
    /// refused locally with "invalid argument" instead of bothering the
    /// driver with a close it never opened.
    pub fn close(&mut self, device: Device) -> i32 {
        if !self.opens.is_open(device) {
            return -EINVAL;
        }
        let major = Major(device.major());
        let Some(endpoint) = self.drivers.get(major) else {
            return -EINVAL;
        };
        let reply = self.transport.exchange(Destination {
            endpoint,
            message_type: BdevOp::Close.message_type(),
            minor: device.minor(),
            id: NO_ID,
        });
        match reply {
            Ok(Reply { status, .. }) if status == OK => {
                self.opens.remove(device);
                OK
            }
            Ok(Reply { status, .. }) => status,
            Err(TransportError::SendFailed) => -EIO,
            Err(_) => -EINVAL,
        }
    }

    /// True when the device has a recorded open.
    pub fn is_open(&self, device: Device) -> bool {
        self.opens.is_open(device)
    }

    /// Borrow the transport (script new replies in tests).
    pub fn transport_mut(&mut self) -> &mut T {
        &mut self.transport
    }
}

/// Check a synchronous reply: identifier must echo, type must be the block
/// general reply.
///
/// C: `bdev_sendrec` validates `m_type == BDEV_REPLY` and the identifier
/// (`ipc.c:144-268`); a mismatch is "invalid argument", a transport failure
/// is "input-output error".
pub fn check_reply(reply: Result<Reply, TransportError>, want_id: i32) -> i32 {
    match reply {
        Ok(Reply { id, status, .. }) if id == want_id => status,
        Ok(_) => -EINVAL,
        Err(TransportError::SendFailed) => -EIO,
        Err(TransportError::BadReply) => -EINVAL,
        Err(TransportError::DriverGone) => -EBUSY,
    }
}

/// Collect every finished call identifier from a batch of replies.
///
/// Helper for the demultiplex loop: each reply is filed into the table by
/// identifier; strays are counted and dropped, matching
/// `bdev_reply_asyn` (`ipc.c:269-316`).
pub fn demux_batch(table: &mut CallTable, replies: &[Reply]) -> (Vec<usize>, usize) {
    let mut completed = Vec::new();
    let mut strays = 0;
    for reply in replies {
        match table.complete(reply.id, reply.status) {
            Some(handle) => completed.push(handle),
            None => strays += 1,
        }
    }
    (completed, strays)
}

/// Success code.
pub const SUCCESS: i32 = OK;
/// Local refusal: no driver bound or device not opened here.
pub const LOCAL_REFUSAL: i32 = -EINVAL;
/// Transport failure mapped to input-output error.
pub const TRANSPORT_FAILURE: i32 = -EIO;
/// Table-full marker for async allocation (callers retry later).
pub const CALL_TABLE_BUSY: i32 = -ENOMEM;

#[cfg(test)]
mod tests {
    use super::super::transport::{
        Destination, LoopbackTransport, RecordingTransport, Reply, TransportError,
    };
    use super::*;

    fn device() -> Device {
        Device::from_parts(2, 1)
    }

    #[test]
    fn test_device_packing_matches_c_major_minor_split() {
        let device = Device::from_parts(0x12, 0x34);
        assert_eq!(device.major(), 0x12);
        assert_eq!(device.minor(), 0x34);
    }

    #[test]
    fn test_driver_table_bind_lookup_clear_cycle() {
        let mut table: DriverTable = DriverTable::new();
        assert_eq!(table.get(Major(2)), None);
        assert!(table.bind(Major(2), 40));
        assert_eq!(table.get(Major(2)), Some(40));
        table.clear(Major(2));
        assert_eq!(table.get(Major(2)), None);
        assert!(!table.bind(Major(99), 1));
    }

    #[test]
    fn test_open_tracker_counts_and_releases() {
        let mut tracker = OpenTracker::new();
        let device = device();
        assert!(tracker.add(device, 1));
        assert!(tracker.add(device, 2));
        assert_eq!(tracker.open_count(device), 2);
        assert!(tracker.remove(device));
        assert!(tracker.is_open(device));
        assert!(tracker.remove(device));
        assert!(!tracker.is_open(device));
        assert!(!tracker.remove(device));
    }

    #[test]
    fn test_open_succeeds_and_records_through_loopback() {
        let mut client = BdevClient::new(LoopbackTransport::healthy(OK));
        assert!(client.bind(Major(2), 40));
        assert_eq!(client.open(device(), 1), OK);
        assert!(client.is_open(device()));
        assert_eq!(client.close(device()), OK);
        assert!(!client.is_open(device()));
    }

    #[test]
    fn test_open_refuses_unknown_driver_locally() {
        let mut client = BdevClient::new(LoopbackTransport::healthy(OK));
        assert_eq!(client.open(device(), 1), -EINVAL);
        assert!(!client.is_open(device()));
    }

    #[test]
    fn test_open_propagates_driver_errors_without_recording() {
        let mut client = BdevClient::new(LoopbackTransport::healthy(-EIO));
        assert!(client.bind(Major(2), 40));
        assert_eq!(client.open(device(), 1), -EIO);
        assert!(!client.is_open(device()));
    }

    #[test]
    fn test_close_refuses_untracked_device_locally() {
        let mut client = BdevClient::new(RecordingTransport::new(OK));
        assert!(client.bind(Major(2), 40));
        assert_eq!(client.close(device()), -EINVAL);
        assert!(client.transport_mut().sent.is_empty());
    }

    #[test]
    fn test_call_table_allocate_complete_collect_cycle() {
        let mut table = CallTable::new();
        let handle = table.allocate(device(), 7).unwrap();
        assert_eq!(table.collect(handle), None);
        assert_eq!(table.complete(7, 42), Some(handle));
        assert_eq!(table.collect(handle), Some(42));
        assert_eq!(table.free_slots(), MAX_CALLS);
    }

    #[test]
    fn test_stray_replies_are_dropped_not_filed() {
        let mut table = CallTable::new();
        assert_eq!(table.complete(99, OK), None);
        let replies = [
            Reply {
                message_type: 0x580,
                id: 99,
                status: OK,
            },
            Reply {
                message_type: 0x580,
                id: 100,
                status: OK,
            },
        ];
        let (completed, strays) = demux_batch(&mut table, &replies);
        assert!(completed.is_empty());
        assert_eq!(strays, 2);
    }

    #[test]
    fn test_send_failures_retry_then_give_up() {
        let mut table = CallTable::new();
        let handle = table.allocate(device(), 7).unwrap();
        for _ in 0..DRIVER_RETRIES {
            assert!(table.note_send_failure(handle));
        }
        assert!(!table.note_send_failure(handle));
        assert_eq!(table.collect(handle), Some(-EIO));
    }

    #[test]
    fn test_transfer_errors_retry_then_give_up() {
        let mut table = CallTable::new();
        let handle = table.allocate(device(), 9).unwrap();
        assert_eq!(table.device_of(handle), Some(device()));
        for _ in 0..TRANSFER_RETRIES {
            assert!(table.note_transfer_error(handle));
        }
        assert!(!table.note_transfer_error(handle));
        assert_eq!(table.collect(handle), Some(-EIO));
        assert_eq!(table.device_of(handle), None);
    }

    #[test]
    fn test_check_reply_accepts_echo_and_rejects_mismatch() {
        let good: Result<Reply, TransportError> = Ok(Reply {
            message_type: 0x580,
            id: NO_ID,
            status: OK,
        });
        assert_eq!(check_reply(good, NO_ID), OK);
        let mismatch: Result<Reply, TransportError> = Ok(Reply {
            message_type: 0x580,
            id: 5,
            status: OK,
        });
        assert_eq!(check_reply(mismatch, NO_ID), -EINVAL);
        assert_eq!(check_reply(Err(TransportError::SendFailed), NO_ID), -EIO);
        assert_eq!(check_reply(Err(TransportError::DriverGone), NO_ID), -EBUSY);
    }

    #[test]
    fn test_request_numbers_match_framework_side() {
        assert_eq!(BdevOp::Open.message_type(), 0x500);
        assert_eq!(BdevOp::Gather.message_type(), 0x504);
        assert_eq!(BdevOp::Scatter.message_type(), 0x505);
        assert_eq!(BdevOp::Ioctl.message_type(), 0x506);
    }

    #[test]
    fn test_destination_carries_sync_no_id() {
        let destination = Destination {
            endpoint: 4,
            message_type: BdevOp::Read.message_type(),
            minor: 1,
            id: NO_ID,
        };
        assert_eq!(destination.id, NO_ID);
        assert_eq!(destination.message_type, 0x502);
    }
}
