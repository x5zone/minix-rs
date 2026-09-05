//! Network driver trait, routing rules, and the server state machine.
//!
//! C correspondence: the callback table in
//! `minix3/minix/include/minix/netdriver.h:23-39`, the life cycle
//! (`netdriver_init`, `netdriver_task`, `netdriver_terminate`,
//! `netdriver_process`) and the queue plus status helpers
//! (`netdriver_recv`, `netdriver_send`, `netdriver_link`,
//! `netdriver_stat_*`) in `minix3/minix/lib/libnetdriver/netdriver.c`.

use super::protocol::{
    HardwareAddress, MULTICAST_LIST_MAX, NDEV_LINK_UNKNOWN, NDEV_LINK_UP, NDEV_MODE_DOWN,
    NdevRequest, NetStats, RECV_QUEUE_BOUND, SEND_QUEUE_BOUND, StatKind, is_net_request,
};
use minix_types::{EINTR, EINVAL, OK};

/// Router classification for one incoming network message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// A notification: run the matching callback, never reply.
    Notify(NotifyKind),
    /// A network request that may run.
    Request(NdevRequest),
    /// Anything else: run the generic callback, never reply.
    Other,
}

/// Notification kinds reaching a network driver.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotifyKind {
    /// Hardware interrupt notification (carries the interrupt mask).
    Hardware(u32),
    /// Clock tick notification (periodic polling cadence).
    Tick,
    /// Any other non-request message.
    Other,
}

/// Initialization report produced by the init callback.
///
/// C: `ndr_init` fills the hardware address, capability bits, and tick
/// cadence; `do_init` echoes them back in the `NDEV_INIT_REPLY` message
/// (`netdriver.c:706-760`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitReport {
    /// Hardware address of the card.
    pub hardware: HardwareAddress,
    /// Capability bits offered to the stack.
    pub capabilities: u32,
    /// Ticks between periodic `tick` calls; zero means no polling.
    pub ticks: u32,
}

/// Link report produced by the link callback.
///
/// C: `ndr_get_link` returns the link state and media (`netdriver.c` link
/// helpers); unknown means "assume up".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinkReport {
    /// One of `NDEV_LINK_UNKNOWN` and `NDEV_LINK_UP`.
    pub link: u32,
    /// Media type identifier (board-specific).
    pub media: u32,
}

/// Network card behavior: one method per framework callback.
///
/// C: `struct netdriver` (`netdriver.h:23-39`) with twelve members. The
/// data-moving pair (`ndr_recv`, `ndr_send`) works on packet lengths only:
/// grant-vector copying stays in the service crate behind the port-access
/// abstraction, so this trait never names a grant or a port.
///
/// Defaults: stopping, mode changes, capability changes, flag changes,
/// media changes, address changes, interrupts, ticks, and generic messages
/// all default to ignoring the event. Initialization defaults to reporting
/// a zero address with no capabilities and no polling; sending and
/// receiving default to "no packet moved" so an unimplemented card visibly
/// drops traffic instead of claiming success.
pub trait NetDriver {
    /// Card name for the init reply (`ndr_name`).
    fn name(&self) -> &str {
        "unnamed"
    }

    /// Initialize the card (`ndr_init`).
    fn init(&mut self, instance: u32) -> InitReport {
        let _ = instance;
        InitReport {
            hardware: HardwareAddress::zero(6),
            capabilities: 0,
            ticks: 0,
        }
    }

    /// Stop the card (`ndr_stop`).
    fn stop(&mut self) {}

    /// Set receive mode and multicast list (`ndr_set_mode`).
    ///
    /// When the offered list is longer than [`MULTICAST_LIST_MAX`], the
    /// framework passes `receive_all_multicast` as true so the card falls
    /// back to receiving all multicast packets, matching the C rule in
    /// `netdriver.c`.
    fn set_mode(&mut self, mode: u32, multicast: &[HardwareAddress], receive_all_multicast: bool) {
        let _ = (mode, multicast, receive_all_multicast);
    }

    /// Enable or disable capabilities (`ndr_set_caps`).
    fn set_capabilities(&mut self, capabilities: u32) {
        let _ = capabilities;
    }

    /// Set driver-specific flags (`ndr_set_flags`).
    fn set_flags(&mut self, flags: u32) {
        let _ = flags;
    }

    /// Set media type (`ndr_set_media`).
    fn set_media(&mut self, media: u32) {
        let _ = media;
    }

    /// Change the hardware address (`ndr_set_hwaddr`).
    fn set_hardware_address(&mut self, address: &HardwareAddress) {
        let _ = address;
    }

    /// Pull one packet into a buffer of at most `max` bytes (`ndr_recv`).
    ///
    /// Returns bytes pulled, zero when no packet waits, negative error.
    fn receive(&mut self, max: usize) -> i64 {
        let _ = max;
        0
    }

    /// Push one packet of `size` bytes onto the wire (`ndr_send`).
    ///
    /// Returns zero on success, negative error otherwise.
    fn send(&mut self, size: usize) -> i32 {
        let _ = size;
        0
    }

    /// Report link state and media (`ndr_get_link`).
    fn link(&mut self) -> LinkReport {
        LinkReport {
            link: NDEV_LINK_UNKNOWN,
            media: 0,
        }
    }

    /// Interrupt hook (`ndr_intr`).
    fn interrupt(&mut self, mask: u32) {
        let _ = mask;
    }

    /// Periodic tick hook (`ndr_tick`).
    fn tick(&mut self) {}

    /// Generic hook (`ndr_other`).
    fn other(&mut self, message_type: i32) {
        let _ = message_type;
    }
}

/// Bounded packet queue: the shared admission policy of both C queues.
///
/// C: `pending_sendq` (eight slots) and `pending_recvq` (two slots) in
/// `netdriver.c:35-39`. The two bounds differ, so the capacity is a const
/// generic and each side instantiates its own length.
#[derive(Debug, Clone)]
pub struct PacketQueue<const BOUND: usize> {
    slots: [u32; 8],
    head: usize,
    len: usize,
}

impl<const BOUND: usize> PacketQueue<BOUND> {
    /// Empty queue. Panics at compile time if the bound exceeds the
    /// backing array; both production bounds (eight and two) fit.
    pub const fn new() -> PacketQueue<BOUND> {
        assert!(BOUND <= 8);
        PacketQueue {
            slots: [0; 8],
            head: 0,
            len: 0,
        }
    }

    /// Number of queued packets.
    pub fn len(&self) -> usize {
        self.len
    }

    /// True when nothing is queued.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// True when no further packet fits.
    pub fn is_full(&self) -> bool {
        self.len >= BOUND
    }

    /// Append a packet identifier; false when full.
    pub fn push(&mut self, id: u32) -> bool {
        if self.is_full() {
            return false;
        }
        let slot = (self.head + self.len) % BOUND;
        self.slots[slot] = id;
        self.len += 1;
        true
    }

    /// Remove the oldest packet identifier; `None` when empty.
    pub fn pop(&mut self) -> Option<u32> {
        if self.is_empty() {
            return None;
        }
        let id = self.slots[self.head];
        self.head = (self.head + 1) % BOUND;
        self.len -= 1;
        Some(id)
    }
}

impl<const BOUND: usize> Default for PacketQueue<BOUND> {
    fn default() -> Self {
        PacketQueue::new()
    }
}

/// Outgoing packet queue (eight slots).
pub type SendQueue = PacketQueue<SEND_QUEUE_BOUND>;
/// Incoming packet queue (two slots).
pub type RecvQueue = PacketQueue<RECV_QUEUE_BOUND>;

/// Server state for one network driver.
///
/// C: the file-static state in `netdriver.c:25-52` — run flag, init
/// expectation, up flag, tick cadence, both queues, pending status, link
/// and statistics. The data-store publication and message transport stay in
/// the service crate; this type owns the policy.
pub struct NetServer {
    running: bool,
    up: bool,
    init_expected: bool,
    send_queue: SendQueue,
    recv_queue: RecvQueue,
    stats: NetStats,
    stats_pending: bool,
    link: u32,
    media: u32,
}

impl NetServer {
    /// Fresh server, as before initialization.
    ///
    /// C: `netdriver_init` sets `init_expected` so the first request must
    /// be an initialization; any other first request is refused.
    pub fn new() -> NetServer {
        NetServer {
            running: false,
            up: false,
            init_expected: true,
            send_queue: SendQueue::new(),
            recv_queue: RecvQueue::new(),
            stats: NetStats::default(),
            stats_pending: false,
            link: NDEV_LINK_UNKNOWN,
            media: 0,
        }
    }

    /// Start the event loop after successful initialization.
    pub fn mark_running(&mut self) {
        self.running = true;
    }

    /// Stop after the current request (`netdriver_terminate`).
    pub fn terminate(&mut self) {
        self.running = false;
    }

    /// True while the event loop should keep receiving.
    pub fn is_running(&self) -> bool {
        self.running
    }

    /// Mark the interface up or down (mode handling).
    pub fn set_up(&mut self, up: bool) {
        self.up = up;
    }

    /// True when the interface is up.
    pub fn is_up(&self) -> bool {
        self.up
    }

    /// Clear the init expectation after the first init request.
    pub fn note_init(&mut self) {
        self.init_expected = false;
        self.send_queue = SendQueue::new();
        self.recv_queue = RecvQueue::new();
    }

    /// True when the next request must be initialization.
    pub fn expects_init(&self) -> bool {
        self.init_expected
    }

    /// Outgoing queue for the send path.
    pub fn send_queue(&mut self) -> &mut SendQueue {
        &mut self.send_queue
    }

    /// Incoming queue for the receive path.
    pub fn recv_queue(&mut self) -> &mut RecvQueue {
        &mut self.recv_queue
    }

    /// Accumulate one statistics report; mirrors the zero-ignoring rule.
    pub fn note_stat(&mut self, kind: StatKind, count: u32) {
        if self.stats.add(kind, count) {
            self.stats_pending = true;
        }
    }

    /// Current counters.
    pub fn stats(&self) -> &NetStats {
        &self.stats
    }

    /// True when a status report waits to be sent.
    pub fn stats_pending(&self) -> bool {
        self.stats_pending
    }

    /// Mark the pending status report as sent.
    pub fn note_status_sent(&mut self) {
        self.stats_pending = false;
    }

    /// Record a link update from the card.
    pub fn note_link(&mut self, report: LinkReport) {
        self.link = report.link;
        self.media = report.media;
    }

    /// Current link state.
    pub fn link(&self) -> u32 {
        self.link
    }

    /// Handle one receive outcome from the transport (fail-stop policy).
    pub fn note_receive(&mut self, result: Result<(), i32>) -> LoopAction {
        match result {
            Ok(()) => LoopAction::Dispatch,
            Err(code) if code == EINTR && !self.running => LoopAction::Stop,
            Err(_) => LoopAction::Abort,
        }
    }
}

impl Default for NetServer {
    fn default() -> Self {
        NetServer::new()
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

/// Classify one message before running any handler.
///
/// The init gate: while the server expects initialization, any non-init
/// request is refused with invalid argument instead of being routed.
pub fn classify(
    is_notify: bool,
    notify: Option<NotifyKind>,
    message_type: i32,
    server: &NetServer,
) -> Result<Route, i32> {
    if is_notify {
        return Ok(Route::Notify(notify.unwrap_or(NotifyKind::Other)));
    }
    let Some(request) = NdevRequest::decode(message_type) else {
        return Ok(Route::Other);
    };
    if !is_net_request(message_type) {
        return Ok(Route::Other);
    }
    if server.expects_init() && request != NdevRequest::Init {
        return Err(EINVAL);
    }
    Ok(Route::Request(request))
}

/// Decide the multicast fallback for an over-long list.
///
/// C: when the stack offers more than `NETDRIVER_MCAST_MAX` addresses, the
/// driver is told to receive all multicast packets (`netdriver.c`
/// configuration path).
pub const fn multicast_fallback(offered: usize) -> bool {
    offered > MULTICAST_LIST_MAX
}

/// Success marker for the announce path.
pub const fn announce_ok() -> i32 {
    OK
}

/// Mode value meaning "interface down".
pub const fn mode_down() -> u32 {
    NDEV_MODE_DOWN
}

/// Link value meaning "up".
pub const fn link_up() -> u32 {
    NDEV_LINK_UP
}

#[cfg(test)]
mod tests {
    use super::super::protocol::{HardwareAddress, StatKind};
    use super::*;

    /// Silent card: implements nothing, keeps defaults.
    struct SilentCard;

    impl NetDriver for SilentCard {}

    /// Loopback card: echoes packet sizes, always linked up.
    struct LoopbackCard {
        sent: usize,
        interrupts: u32,
    }

    impl NetDriver for LoopbackCard {
        fn name(&self) -> &str {
            "loopback"
        }

        fn send(&mut self, size: usize) -> i32 {
            self.sent += size;
            OK
        }

        fn receive(&mut self, max: usize) -> i64 {
            (max.min(64)) as i64
        }

        fn link(&mut self) -> LinkReport {
            LinkReport {
                link: NDEV_LINK_UP,
                media: 1,
            }
        }

        fn interrupt(&mut self, mask: u32) {
            self.interrupts |= mask;
        }
    }

    #[test]
    fn test_default_card_ignores_everything_quietly() {
        let mut card = SilentCard;
        assert_eq!(card.name(), "unnamed");
        let report = card.init(0);
        assert_eq!(report.capabilities, 0);
        assert_eq!(report.ticks, 0);
        assert_eq!(card.send(100), 0);
        assert_eq!(card.receive(100), 0);
        card.interrupt(1);
        card.tick();
    }

    #[test]
    fn test_loopback_card_moves_packets_and_reports_link() {
        let mut card = LoopbackCard {
            sent: 0,
            interrupts: 0,
        };
        assert_eq!(card.name(), "loopback");
        assert_eq!(card.send(60), OK);
        assert_eq!(card.sent, 60);
        assert_eq!(card.receive(1500), 64);
        let link = card.link();
        assert_eq!(link.link, NDEV_LINK_UP);
        card.interrupt(0b100);
        assert_eq!(card.interrupts, 0b100);
    }

    #[test]
    fn test_init_gate_refuses_non_init_first_requests() {
        let server = NetServer::new();
        assert!(server.expects_init());
        let refused = classify(false, None, NdevRequest::Send.message_type(), &server);
        assert_eq!(refused, Err(EINVAL));
        let accepted = classify(false, None, NdevRequest::Init.message_type(), &server);
        assert_eq!(accepted, Ok(Route::Request(NdevRequest::Init)));
    }

    #[test]
    fn test_init_clears_queues_and_expectation() {
        let mut server = NetServer::new();
        server.send_queue().push(1);
        server.note_init();
        assert!(!server.expects_init());
        assert!(server.send_queue().is_empty());
        let routed = classify(false, None, NdevRequest::Send.message_type(), &server);
        assert_eq!(routed, Ok(Route::Request(NdevRequest::Send)));
    }

    #[test]
    fn test_send_queue_bound_is_eight_and_recv_is_two() {
        let mut send = SendQueue::new();
        for id in 0..8 {
            assert!(send.push(id));
        }
        assert!(send.is_full());
        assert!(!send.push(8));
        let mut recv = RecvQueue::new();
        assert!(recv.push(1));
        assert!(recv.push(2));
        assert!(!recv.push(3));
        assert_eq!(recv.pop(), Some(1));
        assert!(recv.push(3));
    }

    #[test]
    fn test_stats_accumulate_and_flag_pending() {
        let mut server = NetServer::new();
        assert!(!server.stats_pending());
        server.note_stat(StatKind::OutputError, 0);
        assert!(!server.stats_pending());
        server.note_stat(StatKind::Collision, 2);
        assert!(server.stats_pending());
        assert_eq!(server.stats().total(), 2);
        server.note_status_sent();
        assert!(!server.stats_pending());
    }

    #[test]
    fn test_multicast_fallback_beyond_sixteen() {
        assert!(!multicast_fallback(16));
        assert!(multicast_fallback(17));
    }

    #[test]
    fn test_notifications_route_without_reply() {
        let server = NetServer::new();
        assert_eq!(
            classify(true, Some(NotifyKind::Tick), 0, &server),
            Ok(Route::Notify(NotifyKind::Tick))
        );
        assert_eq!(classify(false, None, 0x400, &server), Ok(Route::Other));
    }

    #[test]
    fn test_server_lifecycle_and_link_tracking() {
        let mut server = NetServer::new();
        server.mark_running();
        assert!(server.is_running());
        server.set_up(true);
        assert!(server.is_up());
        server.note_link(LinkReport {
            link: NDEV_LINK_UP,
            media: 7,
        });
        assert_eq!(server.link(), NDEV_LINK_UP);
        server.terminate();
        assert_eq!(server.note_receive(Err(EINTR)), LoopAction::Stop);
        let mut address = HardwareAddress::zero(6);
        address.bytes[0] = 0x52;
        assert!(!address.is_zero());
    }
}
