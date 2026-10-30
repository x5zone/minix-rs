//! The shared network driver pump: one message at a time, one pump for
//! every card.
//!
//! C correspondence: `netdriver_task` and `netdriver_process`
//! (`netdriver.c:763-993`) plus the request handlers (`do_init`,
//! `do_conf`, `do_transfer`, `do_status_reply`, `netdriver_send`,
//! `netdriver_recv`, `send_status`). In C the pump lives once in
//! `libnetdriver` and every card calls `netdriver_task` with its callback
//! table; this module is that shared pump in Rust. The loop shell and the
//! RS birth handshake stay in [`minix_driver_rt::runtime::DriverRuntime`];
//! a card binary builds [`NetService`] over its [`NetDriver`] face and
//! hands it to the runtime.
//!
//! Where the C framework's shapes do not survive the Rust face contract,
//! the deviation is stated at the site: the face's `receive` reports zero
//! for "no packet now" (the C framework distinguishes that from a
//! discarded runt with its `do { } while (r == 0)` retry), so a zero
//! return parks the request and a runt keeps the request parked after the
//! packet is dropped. `SUSPEND` (-998) parks both directions, as in C.
//!
//! The framework's [`NetServer`] owns the init gate, the up flag, link and
//! statistics; this pump owns the pending entries (caller, request id,
//! byte count) because the C queue stores `netdriver_data`, not just the
//! id. The data-plane vector copies stay with the card behind the
//! [`NetDriver`] face, except the configuration's multicast list, which
//! the framework copies itself (`do_conf`, `netdriver.c:494-510`).

use minix_driver_rt::runtime::DriverHandler;
use minix_driver_rt::transport::DriverTransport;
use minix_types::{Endpoint, Message, EINTR, OK, SUSPEND};

use crate::driver::{
    classify, multicast_fallback, InitReport, LinkReport, NetDriver, NetServer, NotifyKind, Route,
};
use crate::protocol::{
    HardwareAddress, NdevReply, NdevRequest, StatKind, MULTICAST_LIST_MAX,
    NDEV_ETH_PACKET_MAX_TAGGED, NDEV_ETH_PACKET_MIN, NDEV_HWADDR_MAX, NDEV_IOV_MAX,
    NDEV_MODE_DOWN, NDEV_MODE_MCAST_ALL, NDEV_MODE_MCAST_LIST, NDEV_NAME_MAX, NDEV_SET_CAPS,
    NDEV_SET_FLAGS, NDEV_SET_HWADDR, NDEV_SET_MEDIA, NDEV_SET_MODE, RECV_QUEUE_BOUND,
    SEND_QUEUE_BOUND,
};
use crate::wire;

/// The instance number every pump reports.
///
/// C reads it from the environment (`env_parse("instance", ...)`,
/// `netdriver.c:889`) and appends it to the device name; the boot-table
/// seam that would supply an override has not landed, so every pump names
/// instance zero, the same name a C driver reports without an environment
/// override.
const INSTANCE: u32 = 0;

/// The notification family shares the 0x1000 base with the source in the
/// low byte (C `is_ipc_notify`, `const.h`).
const fn is_ipc_notify(message_type: i32) -> bool {
    (message_type & !0xff) == 0x1000
}

/// One parked send or receive request: who waits, for which id, and how
/// many bytes the vector described.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PendingTransfer {
    caller: Endpoint,
    id: u32,
    size: usize,
}

/// Fixed-size FIFO of parked transfer requests (`netdriver_data`'s queue
/// role in C).
#[derive(Debug, Clone)]
struct PendingQueue<const BOUND: usize> {
    entries: [Option<PendingTransfer>; BOUND],
    head: usize,
    len: usize,
}

impl<const BOUND: usize> PendingQueue<BOUND> {
    const fn new() -> Self {
        PendingQueue {
            entries: [None; BOUND],
            head: 0,
            len: 0,
        }
    }

    fn push(&mut self, entry: PendingTransfer) -> bool {
        if self.len == BOUND {
            return false;
        }
        let slot = (self.head + self.len) % BOUND;
        self.entries[slot] = Some(entry);
        self.len += 1;
        true
    }

    fn peek(&self) -> Option<&PendingTransfer> {
        if self.len == 0 {
            None
        } else {
            self.entries[self.head].as_ref()
        }
    }

    fn pop(&mut self) -> Option<PendingTransfer> {
        if self.len == 0 {
            return None;
        }
        let entry = self.entries[self.head].take();
        self.head = (self.head + 1) % BOUND;
        self.len -= 1;
        entry
    }

    fn clear(&mut self) {
        self.entries = [None; BOUND];
        self.head = 0;
        self.len = 0;
    }
}

/// Build the device name the C framework forms: base name plus the
/// instance digits (`netdriver_set_name`, `netdriver.c:829-851`).
fn device_name(base: &str, instance: u32) -> [u8; NDEV_NAME_MAX] {
    let mut name = [0u8; NDEV_NAME_MAX];
    let bytes = base.as_bytes();
    // C asserts the base leaves four bytes for the digits.
    let len = bytes.len().min(NDEV_NAME_MAX - 4);
    name[..len].copy_from_slice(&bytes[..len]);
    let mut at = len;
    if instance >= 100 {
        name[at] = b'0' + (instance / 100) as u8;
        at += 1;
    }
    if instance >= 10 {
        name[at] = b'0' + ((instance % 100) / 10) as u8;
        at += 1;
    }
    name[at] = b'0' + (instance % 10) as u8;
    name
}

/// A card's service: the face plus the framework state.
pub struct NetService<D: NetDriver> {
    face: D,
    server: NetServer,
    init_report: InitReport,
    /// Where status reports travel (`status_endpt` in C).
    status_endpoint: Option<Endpoint>,
    /// A status report is in flight and unacknowledged.
    pending_status: bool,
    /// The link state changed since the last report.
    pending_link: bool,
    sends: PendingQueue<SEND_QUEUE_BOUND>,
    recvs: PendingQueue<RECV_QUEUE_BOUND>,
}

impl<D: NetDriver> NetService<D> {
    /// Build the service, running the card's initialization callback once.
    ///
    /// C: `netdriver_init` calls `ndr_init` before the loop starts and
    /// keeps the report for every later `NDEV_INIT` reply
    /// (`netdriver.c:852-925`).
    pub fn new(mut face: D) -> Self {
        let init_report = face.init(INSTANCE);
        let mut server = NetServer::new();
        server.mark_running();
        NetService {
            face,
            server,
            init_report,
            status_endpoint: None,
            pending_status: false,
            pending_link: false,
            sends: PendingQueue::new(),
            recvs: PendingQueue::new(),
        }
    }

    /// The framework state (tests and the binary's diagnostics).
    pub fn server(&self) -> &NetServer {
        &self.server
    }

    /// The card face.
    pub fn face(&self) -> &D {
        &self.face
    }

    /// The card face, mutably.
    pub fn face_mut(&mut self) -> &mut D {
        &mut self.face
    }

    /// Request a link/media refresh and a status report.
    ///
    /// C: `netdriver_link` (`netdriver.c:626-632`), called by card code
    /// when it sees the link move.
    pub fn note_link<T: DriverTransport>(&mut self, transport: &mut T) {
        self.pending_link = true;
        self.send_status(transport);
    }

    /// Account one statistics increment and report it when possible.
    ///
    /// C: `netdriver_stat_*` (`netdriver.c:638-682`); a zero count changes
    /// nothing.
    pub fn note_stat<T: DriverTransport>(&mut self, transport: &mut T, kind: StatKind, count: u32) {
        self.server.note_stat(kind, count);
        self.send_status(transport);
    }

    /// Refresh the link report from the card.
    fn update_link(&mut self) {
        let report: LinkReport = self.face.link();
        self.server.note_link(report);
        self.pending_link = false;
    }

    /// Send the pending status report if one is due and none is in flight.
    ///
    /// C: `send_status` (`netdriver.c:575-620`): no endpoint or an
    /// unacknowledged report both suppress the attempt; sending zeroes the
    /// statistics accumulators so the next report carries increments.
    fn send_status<T: DriverTransport>(&mut self, transport: &mut T) {
        if self.pending_status {
            return;
        }
        let Some(endpoint) = self.status_endpoint else {
            return;
        };
        if !self.pending_link && !self.server.stats_pending() {
            return;
        }
        if self.pending_link {
            self.update_link();
        }
        let stats = *self.server.stats();
        let mut message = Message {
            m_type: NdevReply::Status.message_type(),
            ..Message::default()
        };
        wire::write_u32(&mut message, wire::status::ID, 0);
        wire::write_u32(&mut message, wire::status::LINK, self.server.link().wire());
        wire::write_u32(&mut message, wire::status::MEDIA, self.server.media());
        wire::write_u32(&mut message, wire::status::OERROR, stats.output_errors);
        wire::write_u32(&mut message, wire::status::COLL, stats.collisions);
        wire::write_u32(&mut message, wire::status::IERROR, stats.input_errors);
        wire::write_u32(&mut message, wire::status::IQDROP, stats.input_drops);
        //[SD19-gate] reply/status sends are fire-and-forget: C panics when
        // send_reply fails; the single-threaded pump drops the failure as
        // documented instead of wedging.
        #[allow(clippy::let_underscore_must_use)]
        let _ = transport.asynsend(endpoint, &mut message);
        self.pending_status = true;
        self.server.note_status_sent();
        self.server.clear_stats();
    }

    /// Send one `{ id, result }` framework reply.
    fn reply<T: DriverTransport>(
        &mut self,
        transport: &mut T,
        caller: Endpoint,
        kind: NdevReply,
        id: u32,
        result: i32,
    ) {
        let mut message = Message {
            m_type: kind.message_type(),
            ..Message::default()
        };
        wire::write_u32(&mut message, wire::ndev_reply::ID, id);
        wire::write_i32(&mut message, wire::ndev_reply::RESULT, result);
        //[SD19-gate] reply/status sends are fire-and-forget: C panics when
        // send_reply fails; the single-threaded pump drops the failure as
        // documented instead of wedging.
        #[allow(clippy::let_underscore_must_use)]
        let _ = transport.asynsend(caller, &mut message);
    }

    /// Handle one delivered message: route it, then run the road's handler.
    pub fn dispatch<T: DriverTransport>(&mut self, transport: &mut T, msg: &Message) {
        let notification = is_ipc_notify(msg.m_type);
        let notify = if notification {
            Some(match msg.m_source {
                Endpoint::HARDWARE => {
                    NotifyKind::Hardware(wire::read_u32(msg, wire::notify::INTERRUPTS))
                }
                Endpoint::CLOCK => NotifyKind::Tick,
                _ => NotifyKind::Other,
            })
        } else {
            None
        };
        match classify(notification, notify, msg.m_type, &self.server) {
            Ok(Route::Notify(kind)) => match kind {
                NotifyKind::Hardware(mask) => self.face.interrupt(mask),
                NotifyKind::Tick => self.face.tick(),
                NotifyKind::Other => self.face.other(msg.m_type),
            },
            Ok(Route::Request(request)) => self.handle_request(request, msg, transport),
            Ok(Route::Other) => self.face.other(msg.m_type),
            // The init gate: C drops every non-init request until the
            // stack's first initialization, without a reply
            // (`netdriver.c:800-806`).
            Err(_) => {}
        }
    }

    /// Run one decoded request.
    fn handle_request<T: DriverTransport>(
        &mut self,
        request: NdevRequest,
        msg: &Message,
        transport: &mut T,
    ) {
        match request {
            NdevRequest::Init => self.handle_init(transport, msg),
            NdevRequest::Configure => self.handle_conf(transport, msg),
            NdevRequest::Send => self.handle_transfer(true, transport, msg),
            NdevRequest::Receive => self.handle_transfer(false, transport, msg),
            NdevRequest::StatusReply => self.handle_status_reply(transport, msg),
            // The reserved control slot falls through to the generic hook
            // in C (`netdriver_process`'s default arm).
            NdevRequest::Ioctl => self.face.other(msg.m_type),
        }
    }

    /// Process an initialization request (`do_init`, `netdriver.c:684-760`).
    fn handle_init<T: DriverTransport>(&mut self, transport: &mut T, msg: &Message) {
        let caller = msg.m_source;
        let id = wire::read_u32(msg, wire::init_request::ID);
        // C's dispatcher clears the init expectation before the handler
        // runs; `do_init` then forgets every parked request and the old
        // status endpoint.
        self.server.note_init();
        self.sends.clear();
        self.recvs.clear();
        self.pending_status = false;
        self.status_endpoint = Some(caller);
        self.update_link();

        let hardware = self.init_report.hardware;
        let name = device_name(self.face.name(), INSTANCE);
        let mut reply = Message {
            m_type: NdevReply::Init.message_type(),
            ..Message::default()
        };
        wire::write_u32(&mut reply, wire::init_reply::ID, id);
        wire::write_u32(&mut reply, wire::init_reply::LINK, self.server.link().wire());
        wire::write_u32(&mut reply, wire::init_reply::MEDIA, self.server.media());
        wire::write_u32(&mut reply, wire::init_reply::CAPS, self.init_report.capabilities);
        wire::write_array(&mut reply, wire::init_reply::NAME, &name);
        wire::write_array(&mut reply, wire::init_reply::HWADDR, &hardware.bytes);
        wire::write_u8(&mut reply, wire::init_reply::HWADDR_LEN, hardware.len as u8);
        wire::write_u8(&mut reply, wire::init_reply::MAX_SEND, SEND_QUEUE_BOUND as u8);
        wire::write_u8(&mut reply, wire::init_reply::MAX_RECV, RECV_QUEUE_BOUND as u8);
        //[SD19-gate] reply/status sends are fire-and-forget: C panics when
        // send_reply fails; the single-threaded pump drops the failure as
        // documented instead of wedging.
        #[allow(clippy::let_underscore_must_use)]
        let _ = transport.asynsend(caller, &mut reply);

        // C pushes a pending statistics report to the new stack
        // immediately (`do_init`'s tail).
        if self.server.stats_pending() {
            self.send_status(transport);
        }
    }

    /// Process a configuration request (`do_conf`, `netdriver.c:494-570`).
    fn handle_conf<T: DriverTransport>(&mut self, transport: &mut T, msg: &Message) {
        let caller = msg.m_source;
        let id = wire::read_u32(msg, wire::conf_request::ID);
        let set = wire::read_u32(msg, wire::conf_request::SET);
        let mut mode = wire::read_u32(msg, wire::conf_request::MODE);

        // Taking the interface down comes first: other settings may need
        // the device stopped.
        if set & NDEV_SET_MODE != 0 && mode == NDEV_MODE_DOWN {
            self.face.set_mode(mode, &[], false);
        }
        if set & NDEV_SET_CAPS != 0 {
            self.face.set_capabilities(wire::read_u32(msg, wire::conf_request::CAPS));
        }
        if set & NDEV_SET_FLAGS != 0 {
            self.face.set_flags(wire::read_u32(msg, wire::conf_request::FLAGS));
        }
        if set & NDEV_SET_MEDIA != 0 {
            self.face.set_media(wire::read_u32(msg, wire::conf_request::MEDIA));
        }
        if set & NDEV_SET_HWADDR != 0 {
            let mut address = HardwareAddress::zero(NDEV_HWADDR_MAX);
            address.bytes = wire::read_array(msg, wire::conf_request::HWADDR);
            self.face.set_hardware_address(&address);
        }
        if set & NDEV_SET_MODE != 0 && mode != NDEV_MODE_DOWN {
            let count = wire::read_u32(msg, wire::conf_request::MCAST_COUNT) as usize;
            if mode & NDEV_MODE_MCAST_LIST != 0 && multicast_fallback(count) {
                mode &= !NDEV_MODE_MCAST_LIST;
                mode |= NDEV_MODE_MCAST_ALL;
            }
            if mode & NDEV_MODE_MCAST_LIST != 0 {
                let grant = wire::read_i32(msg, wire::conf_request::MCAST_GRANT);
                let mut raw = [0u8; MULTICAST_LIST_MAX * NDEV_HWADDR_MAX];
                let bytes = count.min(MULTICAST_LIST_MAX) * NDEV_HWADDR_MAX;
                transport
                    .copy_from_grant(caller, grant, 0, &mut raw[..bytes])
                    .unwrap_or_else(|error| {
                        panic!("netdriver: unable to copy multicast list: {error}")
                    });
                let list: [HardwareAddress; MULTICAST_LIST_MAX] =
                    ::core::array::from_fn(|index| {
                        let mut address = HardwareAddress::zero(NDEV_HWADDR_MAX);
                        address.bytes.copy_from_slice(
                            &raw[index * NDEV_HWADDR_MAX..(index + 1) * NDEV_HWADDR_MAX],
                        );
                        address
                    });
                self.face
                    .set_mode(mode, &list[..count.min(MULTICAST_LIST_MAX)], false);
            } else {
                self.face.set_mode(mode, &[], false);
            }
        }

        // C always answers OK: the caller cannot act on a failure.
        self.reply(transport, caller, NdevReply::Conf, id, OK);

        // Taking the device down aborts every parked transfer with EINTR.
        if set & NDEV_SET_MODE != 0 {
            if mode == NDEV_MODE_DOWN {
                while let Some(pending) = self.sends.pop() {
                    self.reply(transport, pending.caller, NdevReply::Send, pending.id, EINTR);
                }
                while let Some(pending) = self.recvs.pop() {
                    self.reply(transport, pending.caller, NdevReply::Recv, pending.id, EINTR);
                }
                self.server.set_up(false);
            } else {
                self.server.set_up(true);
            }
        }
    }

    /// Process a send or receive request (`do_transfer`,
    /// `netdriver.c:387-492`).
    fn handle_transfer<T: DriverTransport>(
        &mut self,
        write: bool,
        transport: &mut T,
        msg: &Message,
    ) {
        let caller = msg.m_source;
        let id = wire::read_u32(msg, wire::transfer_request::ID);
        let count = wire::read_u32(msg, wire::transfer_request::COUNT) as usize;
        if count == 0 || count > NDEV_IOV_MAX {
            panic!("netdriver: bad I/O vector count: {count}");
        }
        let mut size = 0usize;
        for index in 0..count {
            let len = wire::read_u16(
                msg,
                wire::transfer_request::LEN + index * wire::transfer_request::LEN_STRIDE,
            ) as usize;
            if len == 0 {
                panic!("netdriver: zero-length I/O vector element");
            }
            size += len;
        }
        if size < NDEV_ETH_PACKET_MIN || (!write && size < NDEV_ETH_PACKET_MAX_TAGGED) {
            panic!("netdriver: invalid I/O vector size: {size}");
        }

        let entry = PendingTransfer { caller, id, size };
        let queued = if write {
            self.sends.push(entry)
        } else {
            self.recvs.push(entry)
        };
        if !queued {
            panic!(
                "netdriver: too many concurrent {} requests",
                if write { "send" } else { "receive" }
            );
        }

        // A down interface aborts the request immediately; the stack uses
        // this while draining its queues.
        if !self.server.is_up() {
            let pending = if write { self.sends.pop() } else { self.recvs.pop() };
            if let Some(pending) = pending {
                let kind = if write { NdevReply::Send } else { NdevReply::Recv };
                self.reply(transport, pending.caller, kind, pending.id, EINTR);
            }
            return;
        }
        if write {
            self.pump_sends(transport);
        } else {
            self.pump_recvs(transport);
        }
    }

    /// Resume sending while the card takes packets
    /// (`netdriver_send`, `netdriver.c:312-340`).
    fn pump_sends<T: DriverTransport>(&mut self, transport: &mut T) {
        while let Some(pending) = self.sends.peek().copied() {
            let result = self.face.send(pending.size);
            if result == SUSPEND {
                break;
            }
            if result < 0 {
                panic!("netdriver: driver reported send failure: {result}");
            }
            self.sends.pop();
            self.reply(transport, pending.caller, NdevReply::Send, pending.id, result);
        }
    }

    /// Resume receiving while the card produces packets
    /// (`netdriver_recv`, `netdriver.c:239-310`).
    fn pump_recvs<T: DriverTransport>(&mut self, transport: &mut T) {
        while let Some(pending) = self.recvs.peek().copied() {
            let result = self.face.receive(pending.size);
            if result == i64::from(SUSPEND) || result == 0 {
                break;
            }
            if result < 0 {
                panic!("netdriver: driver reported receive failure: {result}");
            }
            // C drops runts and calls the card again for the same request
            // (`do { ... } while (r == 0)`); the retry happens in the next
            // loop turn here.
            if (result as usize) < NDEV_ETH_PACKET_MIN {
                continue;
            }
            if result as usize > pending.size {
                panic!("netdriver: oversized packet returned: {result}");
            }
            self.recvs.pop();
            self.reply(
                transport,
                pending.caller,
                NdevReply::Recv,
                pending.id,
                result as i32,
            );
        }
    }

    /// Process a status acknowledgement (`do_status_reply`,
    /// `netdriver.c:622-641`).
    fn handle_status_reply<T: DriverTransport>(&mut self, transport: &mut T, msg: &Message) {
        let Some(endpoint) = self.status_endpoint else {
            return;
        };
        if msg.m_source != endpoint {
            return;
        }
        let id = wire::read_u32(msg, wire::status_reply_request::ID);
        if !self.pending_status || id != 0 {
            panic!("netdriver: unexpected status reply");
        }
        self.pending_status = false;
        if self.pending_link || self.server.stats_pending() {
            self.send_status(transport);
        }
    }
}

impl<D: NetDriver, T: DriverTransport> DriverHandler<T> for NetService<D> {
    fn handle(&mut self, transport: &mut T, msg: &Message) {
        self.dispatch(transport, msg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    use crate::protocol::{LinkState, NDEV_MODE_UP};

    /// Scripted transport recording every reply and multicast copy.
    struct Scripted {
        sent: Vec<(Endpoint, Message)>,
        copied: Vec<(Endpoint, i32, usize)>,
        copy_fails: bool,
    }

    impl Scripted {
        fn new() -> Self {
            Scripted {
                sent: Vec::new(),
                copied: Vec::new(),
                copy_fails: false,
            }
        }
    }

    impl DriverTransport for Scripted {
        fn receive(&mut self, _msg: &mut Message) -> Result<(), i32> {
            Err(-minix_types::EIO)
        }
        fn send(&mut self, dst: Endpoint, msg: &mut Message) -> Result<(), i32> {
            self.sent.push((dst, *msg));
            Ok(())
        }
        fn send_rec(&mut self, dst: Endpoint, msg: &mut Message) -> Result<(), i32> {
            self.sent.push((dst, *msg));
            Ok(())
        }
        fn asynsend(&mut self, dst: Endpoint, msg: &mut Message) -> Result<(), i32> {
            self.sent.push((dst, *msg));
            Ok(())
        }
        fn copy_from_grant(
            &mut self,
            granter: Endpoint,
            grant: i32,
            offset: u64,
            buf: &mut [u8],
        ) -> Result<(), i32> {
            self.copied.push((granter, grant, offset as usize));
            if self.copy_fails {
                return Err(-minix_types::EFAULT);
            }
            for (index, byte) in buf.iter_mut().enumerate() {
                // A recognizable address pattern: index bytes.
                *byte = index as u8;
            }
            Ok(())
        }
        fn copy_to_grant(
            &mut self,
            _granter: Endpoint,
            _grant: i32,
            _offset: u64,
            _buf: &[u8],
        ) -> Result<(), i32> {
            Ok(())
        }
        fn publish_label(&mut self, _label: &str) -> Result<(), i32> {
            Ok(())
        }
    }

    /// Scriptable card: records calls, serves a canned receive script.
    struct TestCard {
        recv_script: Vec<i64>,
        sent_bytes: usize,
        interrupts: u32,
        ticks: u32,
        modes: Vec<(u32, usize, bool)>,
        caps: u32,
        flags: u32,
        media: u32,
        hardware: HardwareAddress,
        link: LinkState,
        link_media: u32,
    }

    impl TestCard {
        fn new() -> Self {
            let mut hardware = HardwareAddress::zero(6);
            hardware.bytes = [0x52, 0x54, 0x00, 0x12, 0x34, 0x56];
            TestCard {
                recv_script: Vec::new(),
                sent_bytes: 0,
                interrupts: 0,
                ticks: 0,
                modes: Vec::new(),
                caps: 0,
                flags: 0,
                media: 0,
                hardware,
                link: LinkState::Up,
                link_media: 3,
            }
        }
    }

    impl NetDriver for TestCard {
        fn name(&self) -> &str {
            "testcard"
        }
        fn init(&mut self, _instance: u32) -> InitReport {
            InitReport {
                hardware: self.hardware,
                capabilities: 0x01,
                ticks: 2,
            }
        }
        fn send(&mut self, size: usize) -> i32 {
            self.sent_bytes += size;
            OK
        }
        fn receive(&mut self, _max: usize) -> i64 {
            if self.recv_script.is_empty() {
                0
            } else {
                self.recv_script.remove(0)
            }
        }
        fn link(&mut self) -> LinkReport {
            LinkReport {
                link: self.link,
                media: self.link_media,
            }
        }
        fn interrupt(&mut self, mask: u32) {
            self.interrupts |= mask;
        }
        fn tick(&mut self) {
            self.ticks += 1;
        }
        fn set_mode(&mut self, mode: u32, multicast: &[HardwareAddress], receive_all: bool) {
            self.modes.push((mode, multicast.len(), receive_all));
        }
        fn set_capabilities(&mut self, capabilities: u32) {
            self.caps = capabilities;
        }
        fn set_flags(&mut self, flags: u32) {
            self.flags = flags;
        }
        fn set_media(&mut self, media: u32) {
            self.media = media;
        }
        fn set_hardware_address(&mut self, address: &HardwareAddress) {
            self.hardware = *address;
        }
    }

    fn request(kind: NdevRequest, caller: Endpoint) -> Message {
        Message {
            m_type: kind.message_type(),
            m_source: caller,
            ..Message::default()
        }
    }

    fn init_message(caller: Endpoint, id: u32) -> Message {
        let mut msg = request(NdevRequest::Init, caller);
        wire::write_u32(&mut msg, wire::init_request::ID, id);
        msg
    }

    fn conf_message(caller: Endpoint, id: u32, set: u32, mode: u32) -> Message {
        let mut msg = request(NdevRequest::Configure, caller);
        wire::write_u32(&mut msg, wire::conf_request::ID, id);
        wire::write_u32(&mut msg, wire::conf_request::SET, set);
        wire::write_u32(&mut msg, wire::conf_request::MODE, mode);
        msg
    }

    fn transfer_message(
        kind: NdevRequest,
        caller: Endpoint,
        id: u32,
        lengths: &[u16],
    ) -> Message {
        let mut msg = request(kind, caller);
        wire::write_u32(&mut msg, wire::transfer_request::ID, id);
        wire::write_u32(&mut msg, wire::transfer_request::COUNT, lengths.len() as u32);
        for (index, len) in lengths.iter().enumerate() {
            wire::write_array(
                &mut msg,
                wire::transfer_request::LEN + index * wire::transfer_request::LEN_STRIDE,
                &len.to_le_bytes(),
            );
        }
        msg
    }

    fn status_reply_message(caller: Endpoint, id: u32) -> Message {
        let mut msg = request(NdevRequest::StatusReply, caller);
        wire::write_u32(&mut msg, wire::status_reply_request::ID, id);
        msg
    }

    fn notification(source: Endpoint, interrupts: u32) -> Message {
        let mut msg = Message {
            m_type: 0x1000,
            m_source: source,
            ..Message::default()
        };
        wire::write_u32(&mut msg, wire::notify::INTERRUPTS, interrupts);
        msg
    }

    fn last(transport: &Scripted) -> &Message {
        &transport.sent.last().expect("a reply").1
    }

    #[test]
    fn test_non_init_request_before_init_is_dropped_silently() {
        let mut service = NetService::new(TestCard::new());
        let mut transport = Scripted::new();
        let msg = transfer_message(NdevRequest::Send, Endpoint(5), 1, &[100]);
        service.dispatch(&mut transport, &msg);
        assert!(transport.sent.is_empty(), "the init gate answers nothing");
    }

    #[test]
    fn test_init_reply_reports_identity_and_queue_bounds() {
        let mut service = NetService::new(TestCard::new());
        let mut transport = Scripted::new();
        let msg = init_message(Endpoint(5), 0xAB);
        service.dispatch(&mut transport, &msg);

        assert_eq!(transport.sent.len(), 1);
        let (dst, reply) = &transport.sent[0];
        assert_eq!(*dst, Endpoint(5));
        assert_eq!(reply.m_type, NdevReply::Init.message_type());
        assert_eq!(wire::read_u32(reply, wire::init_reply::ID), 0xAB);
        assert_eq!(wire::read_u32(reply, wire::init_reply::LINK), 1);
        assert_eq!(wire::read_u32(reply, wire::init_reply::MEDIA), 3);
        assert_eq!(wire::read_u32(reply, wire::init_reply::CAPS), 0x01);
        assert_eq!(
            &wire::read_array::<16>(reply, wire::init_reply::NAME)[..10],
            b"testcard0\0"
        );
        assert_eq!(
            wire::read_array::<6>(reply, wire::init_reply::HWADDR),
            [0x52, 0x54, 0x00, 0x12, 0x34, 0x56]
        );
        assert_eq!(wire::read_u8(reply, wire::init_reply::HWADDR_LEN), 6);
        assert_eq!(wire::read_u8(reply, wire::init_reply::MAX_SEND), 8);
        assert_eq!(wire::read_u8(reply, wire::init_reply::MAX_RECV), 2);
    }

    #[test]
    fn test_interrupt_and_tick_notifications_reach_the_card() {
        let mut service = NetService::new(TestCard::new());
        let mut transport = Scripted::new();
        let msg = notification(Endpoint::HARDWARE, 0b1010);
        service.dispatch(&mut transport, &msg);
        assert_eq!(service.face().interrupts, 0b1010);

        let tick = notification(Endpoint::CLOCK, 0);
        service.dispatch(&mut transport, &tick);
        assert_eq!(service.face().ticks, 1);
        assert!(transport.sent.is_empty(), "notifications never reply");
    }

    #[test]
    fn test_send_while_down_answers_eintr() {
        let mut service = NetService::new(TestCard::new());
        let mut transport = Scripted::new();
        service.dispatch(&mut transport, &init_message(Endpoint(5), 1));
        service.dispatch(
            &mut transport,
            &conf_message(Endpoint(5), 2, NDEV_SET_MODE, NDEV_MODE_DOWN),
        );
        let msg = transfer_message(NdevRequest::Send, Endpoint(5), 3, &[100]);
        service.dispatch(&mut transport, &msg);
        assert_eq!(last(&transport).m_type, NdevReply::Send.message_type());
        assert_eq!(wire::read_u32(last(&transport), wire::ndev_reply::ID), 3);
        assert_eq!(
            wire::read_i32(last(&transport), wire::ndev_reply::RESULT),
            EINTR
        );
    }

    #[test]
    fn test_send_while_up_sums_the_vector_and_reaches_the_card() {
        let mut service = NetService::new(TestCard::new());
        let mut transport = Scripted::new();
        service.dispatch(&mut transport, &init_message(Endpoint(5), 1));
        service.dispatch(
            &mut transport,
            &conf_message(Endpoint(5), 2, NDEV_SET_MODE, NDEV_MODE_UP),
        );
        let msg = transfer_message(NdevRequest::Send, Endpoint(5), 3, &[60, 100]);
        service.dispatch(&mut transport, &msg);
        assert_eq!(service.face().sent_bytes, 160);
        assert_eq!(last(&transport).m_type, NdevReply::Send.message_type());
        assert_eq!(wire::read_u32(last(&transport), wire::ndev_reply::ID), 3);
        assert_eq!(wire::read_i32(last(&transport), wire::ndev_reply::RESULT), OK);
    }

    #[test]
    fn test_recv_reply_carries_the_packet_length() {
        let mut service = NetService::new(TestCard::new());
        let mut transport = Scripted::new();
        service.dispatch(&mut transport, &init_message(Endpoint(5), 1));
        service.dispatch(
            &mut transport,
            &conf_message(Endpoint(5), 2, NDEV_SET_MODE, NDEV_MODE_UP),
        );
        service.face_mut().recv_script.push(64);
        let msg = transfer_message(NdevRequest::Receive, Endpoint(5), 3, &[1600]);
        service.dispatch(&mut transport, &msg);
        assert_eq!(last(&transport).m_type, NdevReply::Recv.message_type());
        assert_eq!(wire::read_u32(last(&transport), wire::ndev_reply::ID), 3);
        assert_eq!(
            wire::read_i32(last(&transport), wire::ndev_reply::RESULT),
            64
        );
    }

    #[test]
    fn test_short_packet_is_dropped_and_the_request_stays() {
        let mut service = NetService::new(TestCard::new());
        let mut transport = Scripted::new();
        service.dispatch(&mut transport, &init_message(Endpoint(5), 1));
        service.dispatch(
            &mut transport,
            &conf_message(Endpoint(5), 2, NDEV_SET_MODE, NDEV_MODE_UP),
        );
        service.face_mut().recv_script.push(30);
        service.face_mut().recv_script.push(64);
        let msg = transfer_message(NdevRequest::Receive, Endpoint(5), 3, &[1600]);
        service.dispatch(&mut transport, &msg);
        // init reply, conf reply, then exactly one receive reply.
        assert_eq!(transport.sent.len(), 3);
        assert_eq!(
            wire::read_i32(last(&transport), wire::ndev_reply::RESULT),
            64
        );
    }

    #[test]
    fn test_conf_multicast_fallback_beyond_sixteen() {
        let mut service = NetService::new(TestCard::new());
        let mut transport = Scripted::new();
        service.dispatch(&mut transport, &init_message(Endpoint(5), 1));
        let mut msg = conf_message(
            Endpoint(5),
            2,
            NDEV_SET_MODE,
            NDEV_MODE_MCAST_LIST,
        );
        wire::write_u32(&mut msg, wire::conf_request::MCAST_COUNT, 17);
        service.dispatch(&mut transport, &msg);
        let (mode, listed, receive_all) = *service.face().modes.last().unwrap();
        assert_eq!(mode & NDEV_MODE_MCAST_LIST, 0);
        assert_ne!(mode & NDEV_MODE_MCAST_ALL, 0);
        assert_eq!(listed, 0);
        assert!(!receive_all);
        assert!(transport.copied.is_empty(), "fallback skips the copy");
    }

    #[test]
    fn test_conf_multicast_list_is_copied_from_the_grant() {
        let mut service = NetService::new(TestCard::new());
        let mut transport = Scripted::new();
        service.dispatch(&mut transport, &init_message(Endpoint(5), 1));
        let mut msg = conf_message(
            Endpoint(5),
            2,
            NDEV_SET_MODE,
            NDEV_MODE_MCAST_LIST,
        );
        wire::write_u32(&mut msg, wire::conf_request::MCAST_COUNT, 2);
        wire::write_i32(&mut msg, wire::conf_request::MCAST_GRANT, 77);
        service.dispatch(&mut transport, &msg);
        assert_eq!(transport.copied.len(), 1);
        assert_eq!(transport.copied[0], (Endpoint(5), 77, 0));
        let (mode, listed, _) = *service.face().modes.last().unwrap();
        assert_ne!(mode & NDEV_MODE_MCAST_LIST, 0);
        assert_eq!(listed, 2);
    }

    #[test]
    fn test_status_report_waits_for_its_acknowledgement() {
        let caller = Endpoint(5);
        let mut service = NetService::new(TestCard::new());
        let mut transport = Scripted::new();
        service.dispatch(&mut transport, &init_message(caller, 1));
        assert_eq!(transport.sent.len(), 1);

        service.note_link(&mut transport);
        assert_eq!(transport.sent.len(), 2);
        assert_eq!(last(&transport).m_type, NdevReply::Status.message_type());
        assert_eq!(wire::read_u32(last(&transport), wire::status::LINK), 1);
        assert_eq!(wire::read_u32(last(&transport), wire::status::MEDIA), 3);

        // A second change while the first report is unacknowledged stays
        // queued behind it (C pacing rule).
        service.note_stat(&mut transport, StatKind::Collision, 2);
        assert_eq!(transport.sent.len(), 2);

        service.dispatch(&mut transport, &status_reply_message(caller, 0));
        assert_eq!(transport.sent.len(), 3);
        assert_eq!(last(&transport).m_type, NdevReply::Status.message_type());
        assert_eq!(wire::read_u32(last(&transport), wire::status::COLL), 2);
        assert_eq!(wire::read_u32(last(&transport), wire::status::OERROR), 0);
    }
}
