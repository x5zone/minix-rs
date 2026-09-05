//! Character driver trait, routing rules, and the server state machine.
//!
//! C correspondence: the callback table in
//! `minix3/minix/include/minix/chardriver.h:10-24`, the per-request adapters
//! `do_open` through `do_select` in
//! `minix3/minix/lib/libchardriver/chardriver.c:279-433`, the reply builder
//! `chardriver_reply` (`chardriver.c:195-274`), the router
//! `chardriver_process` (`chardriver.c:455-532`), the out-of-band replies
//! `chardriver_reply_task` and `chardriver_reply_select`
//! (`chardriver.c:129-172`), and the main loop `chardriver_task` with
//! `chardriver_terminate` (`chardriver.c:537-570`).

use super::protocol::{
    BAD_MINOR, BLOCK_OPEN_MISMATCH, CDEV_CLONED, CDEV_CTTY, CdevRequest, DeviceMinor,
    NO_CANCEL_HOOK, NO_IOCTL_HOOK, NO_SELECT_HOOK, NO_TRANSFER_HOOK, OBSOLETE_SUSPEND,
    OpenDeviceSet, RESTART_MARKER, RequestId, SUCCESS, SUPPRESS_REPLY, is_char_request,
};
use minix_types::{EINTR, EINVAL, OK};

/// Raw block-side open message type, answered with "no such device".
///
/// C: `BDEV_OPEN` (`com.h:970`) special-cased in `chardriver_process`
/// (`chardriver.c:488-492`). Value `0x500` (block base plus zero).
pub const BLOCK_OPEN_MESSAGE: i32 = 0x500;

/// Notification source: hardware interrupt controller.
///
/// C: `HARDWARE` endpoint checked in `chardriver_process`
/// (`chardriver.c:465-470`). Interrupt notifications carry a bit mask of
/// pending interrupt lines and are routed to the interrupt callback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotifySource {
    /// Hardware interrupt notification (carries the interrupt mask).
    Hardware(u32),
    /// Clock alarm notification (carries the timer stamp).
    Clock(i64),
    /// Any other message that is not a driver request.
    Other,
}

/// How the router classifies one incoming message.
///
/// C: the branch structure of `chardriver_process` (`chardriver.c:464-532`).
/// Notifications and unknown messages take the no-reply path; character
/// requests take the reply path after the open-gate check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// A notification: run the matching callback, never reply.
    Notify(NotifySource),
    /// A block-side open sent to a character driver: answer "no device".
    BlockOpen,
    /// A character request for a device nobody opened since the restart:
    /// drop silently unless it is itself an open.
    Stale,
    /// A character request that may run; carries the decoded request.
    Request(CdevRequest),
    /// Anything else: run the generic callback, never reply.
    Other,
}

/// What the server should do after a handler returns.
///
/// C: `chardriver_reply` (`chardriver.c:195-274`). Most results become a
/// reply; three markers change the behavior: "answer later" sends nothing
/// now, "restarted" sends nothing at all, and the obsolete suspend marker is
/// rejected loudly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplyDecision {
    /// Send a normal reply carrying this status code.
    Reply(i32),
    /// Send nothing now; the handler parked the request and will answer
    /// through [`ReplyPlan::parked_task`] or the cancel path.
    Parked,
    /// Send nothing: the server restarted and the caller was already told
    /// through other means.
    SwallowedRestart,
}

/// Decide the reply behavior for a handler result on a request.
///
/// C: the head of `chardriver_reply` (`chardriver.c:203-233`).
/// Only read, write, control, and cancel requests may be parked; any other
/// request that answers "answer later" is a driver bug. The obsolete
/// suspend marker and the restart marker keep their C meaning.
pub fn reply_decision(request: CdevRequest, result: i32) -> ReplyDecision {
    if result == SUPPRESS_REPLY {
        match request {
            CdevRequest::Read | CdevRequest::Write | CdevRequest::Ioctl | CdevRequest::Cancel => {
                ReplyDecision::Parked
            }
            _ => ReplyDecision::Reply(EINVAL),
        }
    } else if result == OBSOLETE_SUSPEND {
        ReplyDecision::Reply(EINVAL)
    } else if result == RESTART_MARKER {
        ReplyDecision::SwallowedRestart
    } else {
        ReplyDecision::Reply(result)
    }
}

/// Planned out-of-band reply, built without touching any message buffer.
///
/// C: `chardriver_reply_task` (`chardriver.c:129-148`) and
/// `chardriver_reply_select` (`chardriver.c:153-172`). Both helpers refuse
/// the "answer later" and "suspend" markers with a panic; this type refuses
/// them by construction because [`ReplyPlan::valid`] is the only way to
/// build one and it returns `None` for those markers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReplyPlan {
    kind: ReplyPlanKind,
    status: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReplyPlanKind {
    Task(RequestId),
    SelectNotify(DeviceMinor),
}

impl ReplyPlan {
    /// Plan a late answer to a parked task request.
    pub const fn parked_task(id: RequestId, status: i32) -> Option<ReplyPlan> {
        if status == SUPPRESS_REPLY || status == OBSOLETE_SUSPEND {
            return None;
        }
        Some(ReplyPlan {
            kind: ReplyPlanKind::Task(id),
            status,
        })
    }

    /// Plan a readiness notification for an earlier select poll.
    pub const fn select_notify(minor: DeviceMinor, status: i32) -> Option<ReplyPlan> {
        if status == SUPPRESS_REPLY || status == OBSOLETE_SUSPEND {
            return None;
        }
        Some(ReplyPlan {
            kind: ReplyPlanKind::SelectNotify(minor),
            status,
        })
    }

    /// Status carried by the planned reply.
    pub const fn status(self) -> i32 {
        self.status
    }

    /// True for a parked-task reply, false for a select notification.
    pub const fn is_task(self) -> bool {
        matches!(self.kind, ReplyPlanKind::Task(_))
    }
}

/// Character device behavior: one method per framework callback.
///
/// C: `struct chardriver` (`chardriver.h:10-24`) with ten function pointers.
/// Every pointer may be null in C; here every method has a default body that
/// reproduces the C default for a missing hook, so a device only overrides
/// what it actually implements:
///
/// - open and close default to success (`do_open`, `do_close` return `OK`
///   when the hook is null: `chardriver.c:287-288,316-317`);
/// - read and write default to input-output error (`do_transfer` returns
///   `EIO`: `chardriver.c:355`);
/// - control defaults to "inappropriate operation" (`do_ioctl` returns
///   `ENOTTY`: `chardriver.c:375`);
/// - cancel defaults to "let it finish" (`do_cancel` returns `EDONTREPLY`:
///   `chardriver.c:403`);
/// - select defaults to "bad file" (`do_select` returns `EBADF`:
///   `chardriver.c:425`);
/// - interrupt, alarm, and generic handlers default to ignoring the message.
///
/// Why one trait instead of several small ones: the router keys every
/// request by number and needs a single home for all ten behaviors, exactly
/// like the C table. Splitting the trait would not remove any routing; it
/// would only hide "this request has no handler" until run time. The
/// Redox scheme trait makes the same choice for the same reason: one mount
/// point, one implementation, one table.
pub trait CharDriver {
    /// Open hook (`cdr_open`). May return a cloned minor number with the
    /// clone and controlling-terminal bits set; the server records it.
    fn open(&mut self, minor: DeviceMinor, access: i32, user: i64) -> i32 {
        let _ = (minor, access, user);
        SUCCESS
    }

    /// Close hook (`cdr_close`).
    fn close(&mut self, minor: DeviceMinor) -> i32 {
        let _ = minor;
        SUCCESS
    }

    /// Read hook (`cdr_read`). Returns bytes moved, or a negative error.
    fn read(
        &mut self,
        minor: DeviceMinor,
        position: u64,
        grant: u64,
        size: usize,
        flags: i32,
        id: RequestId,
    ) -> i64 {
        let _ = (minor, position, grant, size, flags, id);
        NO_TRANSFER_HOOK as i64
    }

    /// Write hook (`cdr_write`). Returns bytes moved, or a negative error.
    fn write(
        &mut self,
        minor: DeviceMinor,
        position: u64,
        grant: u64,
        size: usize,
        flags: i32,
        id: RequestId,
    ) -> i64 {
        let _ = (minor, position, grant, size, flags, id);
        NO_TRANSFER_HOOK as i64
    }

    /// Control hook (`cdr_ioctl`).
    fn ioctl(
        &mut self,
        minor: DeviceMinor,
        request: u64,
        grant: u64,
        flags: i32,
        user: i64,
        id: RequestId,
    ) -> i32 {
        let _ = (minor, request, grant, flags, user, id);
        NO_IOCTL_HOOK
    }

    /// Cancel hook (`cdr_cancel`).
    fn cancel(&mut self, minor: DeviceMinor, id: RequestId) -> i32 {
        let _ = (minor, id);
        NO_CANCEL_HOOK
    }

    /// Poll hook (`cdr_select`).
    fn select(&mut self, minor: DeviceMinor, ops: u32) -> i32 {
        let _ = (minor, ops);
        NO_SELECT_HOOK
    }

    /// Interrupt hook (`cdr_intr`): a hardware mask arrived.
    fn interrupt(&mut self, mask: u32) {
        let _ = mask;
    }

    /// Alarm hook (`cdr_alarm`): a clock stamp arrived.
    fn alarm(&mut self, stamp: i64) {
        let _ = stamp;
    }

    /// Generic hook (`cdr_other`): anything the router does not recognize.
    fn other(&mut self, message_type: i32) {
        let _ = message_type;
    }
}

/// Classify one message before running any handler.
///
/// Inputs mirror what `chardriver_process` sees (`chardriver.c:455-456`):
/// whether the transport reports a notification, the raw message type, and
/// — for requests — the minor device number. The open-device set implements
/// the restart gate (`chardriver.c:503-514`): after a restart, only an open
/// request is accepted for an unrecorded device; any other request for an
/// unrecorded device is dropped without a reply so a stale caller cannot
/// wedge the virtual file system service.
pub fn classify(
    is_notify: bool,
    notify: Option<NotifySource>,
    message_type: i32,
    minor: Option<DeviceMinor>,
    opened: &OpenDeviceSet,
) -> Route {
    if is_notify {
        return Route::Notify(notify.unwrap_or(NotifySource::Other));
    }
    if message_type == BLOCK_OPEN_MESSAGE {
        return Route::BlockOpen;
    }
    let Some(request) = CdevRequest::decode(message_type) else {
        return Route::Other;
    };
    if is_char_request(message_type) {
        match minor {
            Some(device) if !opened.contains(device) => {
                if request == CdevRequest::Open {
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

/// Extract the possibly-cloned minor number from an open result.
///
/// C: `do_open` (`chardriver.c:297-302`). A non-negative result with the
/// clone bit set carries the new minor in the remaining bits; anything else
/// leaves the requested device unchanged.
pub fn cloned_minor(result: i32, requested: DeviceMinor) -> DeviceMinor {
    if result >= 0 && (result & CDEV_CLONED) != 0 {
        DeviceMinor((result & !(CDEV_CLONED | CDEV_CTTY)) as u32)
    } else {
        requested
    }
}

/// Server state for one character driver: run flag plus open tracking.
///
/// C: the file-static `running` flag (`chardriver.c:52`) with
/// `chardriver_terminate` (`chardriver.c:537-544`) clearing it, and the
/// open-device set (`chardriver.c:54-56`). The receive loop itself
/// (`chardriver_task`, `chardriver.c:549-570`) stays in the service crate
/// because only it owns the transport; this type owns the policy: when to
/// keep looping, when a receive error is fatal, and when an interrupt means
/// "stop now".
pub struct CharServer {
    running: bool,
    opened: OpenDeviceSet,
}

impl CharServer {
    /// Fresh server, as before the first announce.
    pub fn new() -> CharServer {
        CharServer {
            running: false,
            opened: OpenDeviceSet::new(),
        }
    }

    /// Announce readiness: start running and forget pre-restart opens.
    ///
    /// C: `chardriver_announce` (`chardriver.c:99-124`) publishes the
    /// "driver up" event and calls `clear_open_devs`. The data-store
    /// publication itself stays in the service crate; this method owns the
    /// state half of the announce.
    pub fn announce(&mut self) {
        self.running = true;
        self.opened.clear();
    }

    /// Stop after the current request (`chardriver_terminate`).
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

    /// Mutable open-device set (record opens, clear on restart).
    pub fn opened_mut(&mut self) -> &mut OpenDeviceSet {
        &mut self.opened
    }

    /// Handle one receive outcome from the transport.
    ///
    /// C: the loop body in `chardriver_task` (`chardriver.c:560-569`).
    /// A transport error of "interrupted" while stopping ends the loop
    /// quietly; any other transport error is fatal, because the framework
    /// treats a failed receive as unrecoverable and stops rather than
    /// serving with unknown state.
    pub fn note_receive(&mut self, result: Result<(), i32>) -> LoopAction {
        match result {
            Ok(()) => LoopAction::Dispatch,
            Err(code) if code == EINTR && !self.running => LoopAction::Stop,
            Err(_) => LoopAction::Abort,
        }
    }
}

impl Default for CharServer {
    fn default() -> Self {
        CharServer::new()
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

/// Error for a block-side open answered by a character driver.
pub const fn block_open_error() -> i32 {
    BLOCK_OPEN_MISMATCH
}

/// Error for a message with no usable minor number.
pub const fn bad_minor_error() -> i32 {
    BAD_MINOR
}

/// Success marker for the announce path.
pub const fn announce_ok() -> i32 {
    OK
}

#[cfg(test)]
mod tests {
    use super::super::protocol::{CdevRequest, DeviceMinor, OpenDeviceSet, RequestId};
    use super::*;

    /// Minimal device: implements nothing, keeps C defaults.
    struct SilentDevice;

    impl CharDriver for SilentDevice {}

    /// Echo device: answers open, close, and poll; counts interrupts.
    struct EchoDevice {
        interrupts: u32,
    }

    impl CharDriver for EchoDevice {
        fn open(&mut self, _minor: DeviceMinor, _access: i32, _user: i64) -> i32 {
            OK
        }

        fn select(&mut self, _minor: DeviceMinor, _ops: u32) -> i32 {
            OK
        }

        fn interrupt(&mut self, mask: u32) {
            self.interrupts |= mask;
        }
    }

    #[test]
    fn test_default_hooks_reproduce_c_missing_hook_codes() {
        let mut device = SilentDevice;
        assert_eq!(device.open(DeviceMinor(1), 0, 0), OK);
        assert_eq!(device.close(DeviceMinor(1)), OK);
        assert_eq!(
            device.read(DeviceMinor(1), 0, 0, 10, 0, RequestId(1)),
            NO_TRANSFER_HOOK as i64
        );
        assert_eq!(
            device.write(DeviceMinor(1), 0, 0, 10, 0, RequestId(1)),
            NO_TRANSFER_HOOK as i64
        );
        assert_eq!(
            device.ioctl(DeviceMinor(1), 0, 0, 0, 0, RequestId(1)),
            NO_IOCTL_HOOK
        );
        assert_eq!(device.cancel(DeviceMinor(1), RequestId(1)), NO_CANCEL_HOOK);
        assert_eq!(device.select(DeviceMinor(1), 0), NO_SELECT_HOOK);
    }

    #[test]
    fn test_notifications_route_without_reply() {
        let opened = OpenDeviceSet::new();
        assert_eq!(
            classify(true, Some(NotifySource::Hardware(0b11)), 0, None, &opened),
            Route::Notify(NotifySource::Hardware(0b11))
        );
        assert_eq!(
            classify(true, Some(NotifySource::Clock(42)), 0, None, &opened),
            Route::Notify(NotifySource::Clock(42))
        );
    }

    #[test]
    fn test_block_open_is_answered_no_such_device() {
        let opened = OpenDeviceSet::new();
        assert_eq!(
            classify(false, None, BLOCK_OPEN_MESSAGE, None, &opened),
            Route::BlockOpen
        );
        assert_eq!(block_open_error(), minix_types::ENXIO);
    }

    #[test]
    fn test_restart_gate_drops_stale_non_open_requests() {
        let opened = OpenDeviceSet::new();
        let minor = Some(DeviceMinor(5));
        assert_eq!(
            classify(
                false,
                None,
                CdevRequest::Read.message_type(),
                minor,
                &opened
            ),
            Route::Stale
        );
        assert_eq!(
            classify(
                false,
                None,
                CdevRequest::Open.message_type(),
                minor,
                &opened
            ),
            Route::Request(CdevRequest::Open)
        );
    }

    #[test]
    fn test_opened_devices_pass_the_restart_gate() {
        let mut server = CharServer::new();
        server.announce();
        server.opened_mut().insert(DeviceMinor(5));
        assert_eq!(
            classify(
                false,
                None,
                CdevRequest::Read.message_type(),
                Some(DeviceMinor(5)),
                server.opened()
            ),
            Route::Request(CdevRequest::Read)
        );
    }

    #[test]
    fn test_unknown_message_goes_to_generic_handler() {
        let opened = OpenDeviceSet::new();
        assert_eq!(classify(false, None, 0x1234, None, &opened), Route::Other);
    }

    #[test]
    fn test_parked_results_only_for_waitable_requests() {
        assert_eq!(
            reply_decision(CdevRequest::Read, SUPPRESS_REPLY),
            ReplyDecision::Parked
        );
        assert_eq!(
            reply_decision(CdevRequest::Cancel, SUPPRESS_REPLY),
            ReplyDecision::Parked
        );
        assert_eq!(
            reply_decision(CdevRequest::Open, SUPPRESS_REPLY),
            ReplyDecision::Reply(EINVAL)
        );
        assert_eq!(
            reply_decision(CdevRequest::Read, RESTART_MARKER),
            ReplyDecision::SwallowedRestart
        );
        assert_eq!(
            reply_decision(CdevRequest::Read, OK),
            ReplyDecision::Reply(OK)
        );
    }

    #[test]
    fn test_late_reply_plans_reject_sentinel_status() {
        assert!(ReplyPlan::parked_task(RequestId(7), SUPPRESS_REPLY).is_none());
        assert!(ReplyPlan::parked_task(RequestId(7), OBSOLETE_SUSPEND).is_none());
        let plan = ReplyPlan::parked_task(RequestId(7), OK).unwrap();
        assert!(plan.is_task());
        assert_eq!(plan.status(), OK);
        let notify = ReplyPlan::select_notify(DeviceMinor(2), OK).unwrap();
        assert!(!notify.is_task());
    }

    #[test]
    fn test_cloned_open_result_records_new_minor() {
        let requested = DeviceMinor(3);
        let cloned = CDEV_CLONED | 11;
        assert_eq!(cloned_minor(cloned, requested), DeviceMinor(11));
        assert_eq!(cloned_minor(OK, requested), requested);
        assert_eq!(cloned_minor(-EINTR, requested), requested);
    }

    #[test]
    fn test_server_announce_and_terminate_cycle() {
        let mut server = CharServer::new();
        assert!(!server.is_running());
        server.announce();
        assert!(server.is_running());
        server.terminate();
        assert!(!server.is_running());
        assert_eq!(server.note_receive(Ok(())), LoopAction::Dispatch);
        assert_eq!(server.note_receive(Err(EINTR)), LoopAction::Stop);
        assert_eq!(server.note_receive(Err(EINVAL)), LoopAction::Abort);
    }

    #[test]
    fn test_echo_device_handles_select_and_interrupts() {
        let mut device = EchoDevice { interrupts: 0 };
        assert_eq!(device.select(DeviceMinor(1), 0b101), OK);
        device.interrupt(0b010);
        assert_eq!(device.interrupts, 0b010);
    }
}
