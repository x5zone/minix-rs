//! Block driver trait, routing rules, and the server state machine.
//!
//! C correspondence: the callback table in
//! `minix3/minix/include/minix/blockdriver.h:21-35`, the single-threaded
//! adapters and announce in `minix3/minix/lib/libblockdriver/driver.c`, the
//! queue-based loop in `driver_st.c:25-89`, the multi-threaded worker
//! dispatch in `driver_mt.c:70-200` (modeled, not replicated), and the
//! partition entry `partition()` in `drvlib.c`.

use super::protocol::{
    BAD_MINOR, BdevRequest, BlockDriverType, DeviceExtent, DeviceId, DeviceMinor, NO_IOCTL_HOOK,
    NO_TRANSFER_HOOK, NOT_DISK, OpenDeviceSet, PartitionGeometry, PartitionStyle, RequestId,
    SUCCESS, is_block_request,
};
use minix_types::{EINTR, EINVAL, OK};

/// Router classification for one incoming block message.
///
/// Mirrors the character [`crate::driver`] shape: notifications never take
/// the reply path, stale requests are dropped silently after a restart, and
/// decoded requests run through the open gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// A notification: run the matching callback, never reply.
    Notify(NotifyKind),
    /// A block request for a device nobody opened since the restart.
    Stale,
    /// A block request that may run.
    Request(BdevRequest),
    /// Anything else: run the generic callback, never reply.
    Other,
}

/// Notification kinds reaching a block driver.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotifyKind {
    /// Hardware interrupt notification (carries the interrupt mask).
    Hardware(u32),
    /// Clock alarm notification (carries the timer stamp).
    Clock(i64),
    /// Any other non-request message.
    Other,
}

/// Block device behavior: one method per framework callback.
///
/// C: `struct blockdriver` (`blockdriver.h:21-35`) with eleven members. As
/// with characters, every member may be null in C; here every method has a
/// default body reproducing the C default:
///
/// - open and close default to success;
/// - transfer (read, write, gather, scatter) defaults to input-output
///   error;
/// - control defaults to "inappropriate operation";
/// - cleanup, geometry, partition lookup, interrupt, alarm, generic, and
///   device-number mapping default to doing nothing (returning "not a
///   disk" or an empty answer where a value is required).
///
/// The vectored pair (gather, scatter) shares one transfer method with a
/// direction flag and a scatter flag, because the C adapters funnel all
/// four data requests into one callback shape.
pub trait BlockDriver {
    /// Driver kind: disk-like devices answer partition requests.
    fn driver_type(&self) -> BlockDriverType {
        BlockDriverType::Disk
    }

    /// Open hook (`bdr_open`).
    fn open(&mut self, minor: DeviceMinor, access: i32) -> i32 {
        let _ = (minor, access);
        SUCCESS
    }

    /// Close hook (`bdr_close`).
    fn close(&mut self, minor: DeviceMinor) -> i32 {
        let _ = minor;
        SUCCESS
    }

    /// Transfer hook (`bdr_transfer`): read, write, gather, and scatter.
    ///
    /// `do_write` selects the direction; `vectored` selects the
    /// gather/scatter pair; `count` is bytes for the contiguous pair and
    /// element count for the vectored pair. The seven parameters mirror the
    /// C callback signature one-to-one so call sites stay comparable with
    /// the C adapters; packing them into a struct would hide that
    /// correspondence.
    #[allow(clippy::too_many_arguments)]
    fn transfer(
        &mut self,
        minor: DeviceMinor,
        do_write: bool,
        position: u64,
        count: u64,
        flags: i32,
        id: RequestId,
        vectored: bool,
    ) -> i64 {
        let _ = (minor, do_write, position, count, flags, id, vectored);
        NO_TRANSFER_HOOK as i64
    }

    /// Control hook (`bdr_ioctl`).
    fn ioctl(&mut self, minor: DeviceMinor, request: u64, grant: u64, user: i64) -> i32 {
        let _ = (minor, request, grant, user);
        NO_IOCTL_HOOK
    }

    /// Cleanup hook (`bdr_cleanup`): release per-open resources.
    fn cleanup(&mut self) {}

    /// Partition lookup (`bdr_part`): extent of one minor device.
    fn partition(&mut self, minor: DeviceMinor) -> Option<DeviceExtent> {
        let _ = minor;
        None
    }

    /// Geometry report (`bdr_geometry`).
    fn geometry(&mut self, minor: DeviceMinor) -> Option<PartitionGeometry> {
        let _ = minor;
        None
    }

    /// Interrupt hook (`bdr_intr`).
    fn interrupt(&mut self, mask: u32) {
        let _ = mask;
    }

    /// Alarm hook (`bdr_alarm`).
    fn alarm(&mut self, stamp: i64) {
        let _ = stamp;
    }

    /// Generic hook (`bdr_other`).
    fn other(&mut self, message_type: i32) {
        let _ = message_type;
    }

    /// Device-number mapping (`bdr_device`): minor to queue device.
    fn device(&mut self, minor: DeviceMinor) -> Option<DeviceId> {
        let _ = minor;
        None
    }
}

/// Classify one message before running any handler.
///
/// The open gate matches the character framework: after an announce, only
/// an open request is accepted for an unrecorded device.
pub fn classify(
    is_notify: bool,
    notify: Option<NotifyKind>,
    message_type: i32,
    minor: Option<DeviceMinor>,
    opened: &OpenDeviceSet,
) -> Route {
    if is_notify {
        return Route::Notify(notify.unwrap_or(NotifyKind::Other));
    }
    let Some(request) = BdevRequest::decode(message_type) else {
        return Route::Other;
    };
    if is_block_request(message_type) {
        match minor {
            Some(device) if !opened.contains_raw(device.0) => {
                if request == BdevRequest::Open {
                    return Route::Request(request);
                }
                return Route::Stale;
            }
            None => return Route::Other,
            _ => {}
        }
    }
    Route::Request(request)
}

/// Server state for one block driver: run flag plus open tracking.
///
/// C: the `running` flag and open table shared by `driver.c`,
/// `driver_st.c`, and `driver_mt.c`. The queue-worker topology of the
/// multi-threaded loop is not replicated; the single event loop plus the
/// bounded pending queue (protocol module) covers both C loops' admission
/// policy.
pub struct BlockServer {
    running: bool,
    opened: OpenDeviceSet,
}

impl BlockServer {
    /// Fresh server, as before the first announce.
    pub fn new() -> BlockServer {
        BlockServer {
            running: false,
            opened: OpenDeviceSet::new(),
        }
    }

    /// Announce readiness: start running and forget pre-restart opens.
    ///
    /// C: `blockdriver_announce` (publishes the `drv.blk.` event and clears
    /// the table; the publication itself stays in the service crate).
    pub fn announce(&mut self) {
        self.running = true;
        self.opened.clear();
    }

    /// Stop after the current request (`blockdriver_terminate`).
    pub fn terminate(&mut self) {
        self.running = false;
    }

    /// True while the event loop should keep receiving.
    pub fn is_running(&self) -> bool {
        self.running
    }

    /// Open-device set for the router.
    pub fn opened(&self) -> &OpenDeviceSet {
        &self.opened
    }

    /// Mutable open-device set.
    pub fn opened_mut(&mut self) -> &mut OpenDeviceSet {
        &mut self.opened
    }

    /// Handle one receive outcome from the transport.
    ///
    /// Same fail-stop policy as characters: an interrupt while stopping
    /// ends the loop quietly, any other transport error aborts.
    pub fn note_receive(&mut self, result: Result<(), i32>) -> LoopAction {
        match result {
            Ok(()) => LoopAction::Dispatch,
            Err(code) if code == EINTR && !self.running => LoopAction::Stop,
            Err(_) => LoopAction::Abort,
        }
    }
}

impl Default for BlockServer {
    fn default() -> Self {
        BlockServer::new()
    }
}

/// What the event loop does next after one receive outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopAction {
    /// A message arrived: route it.
    Dispatch,
    /// Termination was requested while blocked: leave the loop.
    Stop,
    /// The receive itself failed: stop immediately.
    Abort,
}

/// Partition-style decision helper: which styles need table parsing.
///
/// C: `partition()` returns early when `bdr_part` yields null
/// (`drvlib.c`); floppy style additionally skips table parsing because the
/// whole medium is one partition.
pub const fn needs_table_parse(style: PartitionStyle, has_part: bool) -> bool {
    if !has_part {
        return false;
    }
    !matches!(style, PartitionStyle::Floppy)
}

/// Geometry refusal for non-disk drivers.
pub const fn not_disk_error() -> i32 {
    NOT_DISK
}

/// Error for a message with no usable minor number.
pub const fn bad_minor_error() -> i32 {
    BAD_MINOR
}

/// Success marker for the announce path.
pub const fn announce_ok() -> i32 {
    OK
}

/// Translate a C-style negative error into an invalid-argument guard.
///
/// Used at the process-function boundary where C would panic on an unknown
/// request inside the reply builder; the Rust side reports invalid argument
/// instead of stopping the process.
pub const fn unknown_request_error() -> i32 {
    EINVAL
}

#[cfg(test)]
mod tests {
    use super::super::protocol::{BdevRequest, BlockDriverType, DeviceMinor, RequestId};
    use super::*;

    /// Minimal disk: implements nothing, keeps C defaults.
    struct SilentDisk;

    impl BlockDriver for SilentDisk {}

    /// Memory disk: answers transfers from a fixed-size buffer model.
    struct MemoryDisk {
        blocks: u64,
    }

    impl BlockDriver for MemoryDisk {
        fn driver_type(&self) -> BlockDriverType {
            BlockDriverType::Disk
        }

        fn transfer(
            &mut self,
            _minor: DeviceMinor,
            _do_write: bool,
            position: u64,
            count: u64,
            _flags: i32,
            _id: RequestId,
            _vectored: bool,
        ) -> i64 {
            if position + count > self.blocks * 512 {
                return -(minix_types::EIO as i64);
            }
            count as i64
        }

        fn partition(&mut self, minor: DeviceMinor) -> Option<DeviceExtent> {
            if minor.0 == 0 {
                Some(DeviceExtent {
                    base: 0,
                    size: self.blocks * 512,
                })
            } else {
                None
            }
        }
    }

    #[test]
    fn test_default_hooks_reproduce_c_missing_hook_codes() {
        let mut device = SilentDisk;
        assert_eq!(device.driver_type(), BlockDriverType::Disk);
        assert_eq!(device.open(DeviceMinor(0), 0), OK);
        assert_eq!(device.close(DeviceMinor(0)), OK);
        assert_eq!(
            device.transfer(DeviceMinor(0), false, 0, 512, 0, RequestId(0), false),
            NO_TRANSFER_HOOK as i64
        );
        assert_eq!(device.ioctl(DeviceMinor(0), 0, 0, 0), NO_IOCTL_HOOK);
        assert_eq!(device.partition(DeviceMinor(0)), None);
        assert_eq!(device.device(DeviceMinor(0)), None);
    }

    #[test]
    fn test_memory_disk_serves_inside_geometry_and_refuses_outside() {
        let mut device = MemoryDisk { blocks: 8 };
        assert_eq!(
            device.transfer(DeviceMinor(0), false, 0, 512, 0, RequestId(1), false),
            512
        );
        assert!(device.transfer(DeviceMinor(0), false, 8 * 512, 512, 0, RequestId(1), false) < 0);
        let extent = device.partition(DeviceMinor(0)).unwrap();
        assert!(extent.contains(0, 512));
        assert_eq!(device.partition(DeviceMinor(3)), None);
    }

    #[test]
    fn test_restart_gate_matches_character_policy() {
        let mut server = BlockServer::new();
        server.announce();
        let minor = Some(DeviceMinor(2));
        assert_eq!(
            classify(
                false,
                None,
                BdevRequest::Write.message_type(),
                minor,
                server.opened()
            ),
            Route::Stale
        );
        assert_eq!(
            classify(
                false,
                None,
                BdevRequest::Open.message_type(),
                minor,
                server.opened()
            ),
            Route::Request(BdevRequest::Open)
        );
        server.opened_mut().insert_raw(2);
        assert_eq!(
            classify(
                false,
                None,
                BdevRequest::Write.message_type(),
                minor,
                server.opened()
            ),
            Route::Request(BdevRequest::Write)
        );
    }

    #[test]
    fn test_notifications_and_unknown_messages_take_no_reply_paths() {
        let server = BlockServer::new();
        assert_eq!(
            classify(
                true,
                Some(NotifyKind::Hardware(1)),
                0,
                None,
                server.opened()
            ),
            Route::Notify(NotifyKind::Hardware(1))
        );
        assert_eq!(
            classify(false, None, 0x400, None, server.opened()),
            Route::Other
        );
    }

    #[test]
    fn test_table_parse_needed_only_for_table_styles() {
        assert!(!needs_table_parse(PartitionStyle::Floppy, true));
        assert!(needs_table_parse(PartitionStyle::Primary, true));
        assert!(needs_table_parse(PartitionStyle::Sub, true));
        assert!(!needs_table_parse(PartitionStyle::Primary, false));
    }

    #[test]
    fn test_server_lifecycle_matches_c_loop() {
        let mut server = BlockServer::new();
        assert!(!server.is_running());
        server.announce();
        assert!(server.is_running());
        server.terminate();
        assert_eq!(server.note_receive(Ok(())), LoopAction::Dispatch);
        assert_eq!(server.note_receive(Err(EINTR)), LoopAction::Stop);
        assert_eq!(server.note_receive(Err(EINVAL)), LoopAction::Abort);
    }
}
