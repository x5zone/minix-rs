//! The VBOX service: the device leg (host time, events) and the HGCM
//! connection state machine.
//!
//! C correspondence: `vbox.c` (the init sequence, `vbox_update_time`, and
//! `vbox_intr`) and `hgcm.c` (the four operations, the connection table,
//! and the request lifecycle). One message at a time: a single-threaded
//! event loop with no shared mutable state across threads.
//!
//! Two legs meet here. The *device* leg runs on the driver's own request
//! page: the periodic alarm asks the host for the time and advances the
//! system clock when it has drifted, and a hardware notification
//! acknowledges the device's events. The *HGCM* leg is the driver's
//! service interface: a caller opens a connection to a host service,
//! performs calls with up to eight parameters, cancels them, and closes
//! the connection, while the device may answer a request asynchronously
//! (the `VMMDEV_HGCM_REQ_DONE` flag plus an event) instead of inline.

use minix_driver_rt::runtime::DriverHandler;
use minix_driver_rt::transport::DriverTransport;
use minix_types::{
    EDONTREPLY, EFAULT, EGENERIC, EINTR, EINVAL, EMFILE, ENFILE, ENOMEM, ENOSYS, Endpoint, Message,
    OK,
};

use crate::backdoor::{Backdoor, VumapPhys};
use crate::device::{DEFAULT_DRIFT, DEFAULT_INTERVAL, convert_result, should_advance};
use crate::protocol::{self, cwire};
use crate::vmmdev::{self, BUF_SIZE, bytes, caller_dir, caller_type, wire};

/// Maximum number of HGCM connections (`MAX_CONNS`).
pub const MAX_CONNS: usize = 4;
/// Concurrent requests per connection (`MAX_REQS`).
pub const MAX_REQS: usize = 2;
/// Parameters per request (`MAX_PARAMS`).
pub const MAX_PARAMS: usize = 8;
/// How many physical spans one grant mapping may return (`MAPVEC_NR`).
pub const MAPVEC_NR: usize = 64;
/// The page size the page list splits on (`PAGE_SIZE`).
pub const PAGE_SIZE: u64 = 4096;

/// The notification family shares the 0x1000 base with the source in the
/// low byte (C `is_ipc_notify`, `const.h`).
const fn is_ipc_notify(message_type: i32) -> bool {
    (message_type & !0xff) == 0x1000
}

/// HGCM connection states (C's `STATE_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    /// The slot is unused.
    Free,
    /// A connect request is in flight.
    Opening,
    /// The connection is established.
    Open,
    /// A disconnect request is in flight.
    Closing,
}

/// One request slot (C's anonymous per-request struct).
struct Req {
    /// The slot is in flight.
    busy: bool,
    /// The request page (C's `ptr`; C allocates it lazily, the rewrite
    /// with the service).
    buffer: [u8; BUF_SIZE],
    /// The page's physical address (C's `addr`).
    phys: u64,
    /// The caller's request identifier (C's `id`).
    id: i64,
    /// The caller's parameter grant (C's `grant`).
    grant: i32,
    /// The parameter count (C's `count`).
    count: i32,
    /// The caller's parameter copy (`vbox_param_t param[MAX_PARAMS]`).
    params: [u8; MAX_PARAMS * wire::CALLER_PARAM_SIZE],
}

impl Req {
    fn new() -> Self {
        Req {
            busy: false,
            buffer: [0; BUF_SIZE],
            phys: 0,
            id: 0,
            grant: 0,
            count: 0,
            params: [0; MAX_PARAMS * wire::CALLER_PARAM_SIZE],
        }
    }
}

/// One connection (C's `hgcm_conn[]` element).
struct Conn {
    state: State,
    /// The owning endpoint (C's `endpt`; kept after a release, because the
    /// open quota counts it).
    endpt: Endpoint,
    /// The device-assigned client identifier (C's `client_id`).
    client_id: u32,
    reqs: [Req; MAX_REQS],
}

impl Conn {
    fn new() -> Self {
        Conn {
            state: State::Free,
            // C's static storage zeroes the endpoint to slot zero; `NONE`
            // is the honest "never assigned" value, and no caller can
            // hold it, so the quota counts only slots an endpoint used.
            endpt: Endpoint::NONE,
            client_id: 0,
            reqs: core::array::from_fn(|_| Req::new()),
        }
    }
}

/// The VBOX service.
pub struct VboxService<B: Backdoor> {
    backdoor: B,
    /// The host-time check period in seconds (C `interval`, default 1).
    interval: i64,
    /// The host-time lead that triggers an update (C `drift`, default 2).
    drift: i64,
    /// The alarm period in ticks (`ticks = sys_hz() * interval`).
    ticks: u64,
    /// The driver's own request page (C's `vir_ptr`).
    device: [u8; BUF_SIZE],
    /// Its physical address (C's `phys_ptr`).
    device_phys: u64,
    conns: [Conn; MAX_CONNS],
}

impl<B: Backdoor> VboxService<B> {
    /// A service with the C defaults (`DEFAULT_INTERVAL`,
    /// `DEFAULT_DRIFT`).
    pub fn new(backdoor: B) -> Self {
        Self::with_options(backdoor, DEFAULT_INTERVAL, DEFAULT_DRIFT)
    }

    /// A service with explicit period and drift (C's `-args` options).
    pub fn with_options(backdoor: B, interval: i64, drift: i64) -> Self {
        VboxService {
            backdoor,
            interval,
            drift,
            ticks: 0,
            device: [0; BUF_SIZE],
            device_phys: 0,
            conns: core::array::from_fn(|_| Conn::new()),
        }
    }

    /// C `vbox_init`'s device tail (`vbox.c:117-140`): resolve every
    /// request page's physical address, report the guest to the host,
    /// compute the alarm period, and run the first time update.
    ///
    /// The C init panics when the device does not answer the report; the
    /// rewrite refuses to start instead, and the binary returns, which is
    /// the same fate RS observes from a failed fresh-start init.
    pub fn start(&mut self) -> Result<(), i32> {
        let Self {
            backdoor,
            device,
            device_phys,
            conns,
            ..
        } = self;
        *device_phys = backdoor.physical(device.as_ptr())?;
        for conn in conns.iter_mut() {
            for req in conn.reqs.iter_mut() {
                req.phys = backdoor.physical(req.buffer.as_ptr())?;
            }
        }

        // The guest report: version and OS type.
        bytes::write_u32(
            &mut self.device,
            wire::REPORT_ADD_VERSION,
            vmmdev::GUEST_VERSION,
        );
        bytes::write_u32(
            &mut self.device,
            wire::REPORT_OS_TYPE,
            vmmdev::GUEST_OS_TYPE,
        );
        match self.device_request(vmmdev::REQ_REPORT_GUEST_INFO, wire::REPORT_SIZE) {
            Ok(vmmdev::ERR_OK) => {}
            // C panics "backdoor device not functioning".
            Ok(_) | Err(_) => return Err(-minix_types::ENODEV),
        }

        // C reads the clock frequency once, here.
        let hz = self.backdoor.hz()?;
        self.ticks = u64::from(hz) * self.interval as u64;

        // The first update runs immediately: a significant time change
        // would otherwise interfere with the rc scripts.
        self.sync_time();
        Ok(())
    }

    /// One request on the driver's own page.
    fn device_request(&mut self, kind: u32, size: usize) -> Result<i32, i32> {
        let Self {
            backdoor,
            device,
            device_phys,
            ..
        } = self;
        request(backdoor, device, *device_phys, kind, size)
    }

    /// C `vbox_update_time` (`vbox.c:57-81`): ask the host for the time
    /// and advance the system clock when it has drifted far enough, then
    /// rearm the alarm.
    ///
    /// C panics when the device write itself fails; the rewrite ignores
    /// the failure (there is no log facility) and keeps the loop alive —
    /// the next tick retries, and the driver's HGCM half stays available.
    fn sync_time(&mut self) {
        let result = self.device_request(vmmdev::REQ_HOST_TIME, wire::HOST_TIME_SIZE);
        if let Ok(vmmdev::ERR_OK) = result {
            let host = (bytes::read_u64(&self.device, wire::HOST_TIME_TIME) / 1000) as i64;
            if let Ok(current) = self.backdoor.now_seconds()
                && should_advance(current, host, self.drift)
            {
                // A failed set leaves the clock alone; the next check
                // tries again.
                let _ = self.backdoor.set_time(host);
            }
        }
        // C re-arms the alarm on every path through this function.
        let _ = self.backdoor.set_alarm(self.ticks);
    }

    /// C `vbox_intr` (`vbox.c:160-183`): acknowledge the device's events
    /// and, when the HGCM bit is set, check every connection for a
    /// completed request.
    fn intr<T: DriverTransport>(&mut self, transport: &mut T) {
        bytes::write_u32(&mut self.device, wire::EVENTS_MASK, 0);
        let result = self.device_request(vmmdev::REQ_ACKNOWLEDGE_EVENTS, wire::EVENTS_SIZE);
        let Ok(vmmdev::ERR_OK) = result else {
            // The device is not answering; C returns without re-enabling
            // the interrupt, and so does this path.
            return;
        };
        if bytes::read_u32(&self.device, wire::EVENTS_MASK) & vmmdev::EVENT_HGCM != 0 {
            self.check_all(transport);
        }
        // C panics when the re-enable fails (the interrupt stays masked
        // until the next event); the seam reports ENOSYS today, which
        // this path ignores for the same reason as the failure above.
        let _ = self.backdoor.irq_enable();
    }

    /// C `hgcm_intr` (`hgcm.c:707-714`): check every live connection.
    fn check_all<T: DriverTransport>(&mut self, transport: &mut T) {
        for conn in 0..MAX_CONNS {
            if self.conns[conn].state != State::Free {
                self.check_conn(transport, conn);
            }
        }
    }

    /// C `check_conn` (`hgcm.c:594-617`): complete every request the
    /// device has marked done and reply to its caller.
    fn check_conn<T: DriverTransport>(&mut self, transport: &mut T, conn: usize) {
        for req in 0..MAX_REQS {
            if !self.conns[conn].reqs[req].busy {
                continue;
            }
            let flags = bytes::read_u32(&self.conns[conn].reqs[req].buffer, wire::HGCM_FLAGS);
            if flags & vmmdev::HGCM_REQ_DONE == 0 {
                continue;
            }
            let mut code = 0;
            let result = self.finish_req(transport, conn, req, &mut code);
            let endpt = self.conns[conn].endpt;
            let id = self.conns[conn].reqs[req].id;
            self.send_reply(transport, endpt, result, code, id);
        }
    }

    /// Handle one delivered message.
    pub fn dispatch<T: DriverTransport>(&mut self, transport: &mut T, msg: &Message) {
        if is_ipc_notify(msg.m_type) {
            match msg.m_source {
                Endpoint::HARDWARE => self.intr(transport),
                Endpoint::CLOCK => self.sync_time(),
                // C logs notifications from other sources and drops them.
                _ => {}
            }
            return;
        }
        self.hgcm_message(transport, msg);
    }

    /// C `hgcm_message` (`hgcm.c:673-690`): route one request and answer
    /// its sender unless the answer is "not yet".
    fn hgcm_message<T: DriverTransport>(&mut self, transport: &mut T, msg: &Message) {
        let mut code = vmmdev::ERR_GENERIC;
        let result = match msg.m_type {
            protocol::VBOX_OPEN => self.do_open(transport, msg, &mut code),
            protocol::VBOX_CLOSE => self.do_close(transport, msg, &mut code),
            protocol::VBOX_CALL => self.do_call(transport, msg, &mut code),
            protocol::VBOX_CANCEL => self.do_cancel(msg),
            _ => ENOSYS,
        };
        if result != EDONTREPLY {
            self.send_reply(
                transport,
                msg.m_source,
                result,
                code,
                read_i64(msg, cwire::ID),
            );
        }
    }

    /// C `send_reply` (`hgcm.c:73-93`): the POSIX result, the device code,
    /// and the caller's identifier.
    fn send_reply<T: DriverTransport>(
        &mut self,
        transport: &mut T,
        endpt: Endpoint,
        result: i32,
        code: i32,
        id: i64,
    ) {
        let mut reply = Message {
            m_type: protocol::VBOX_REPLY,
            ..Message::default()
        };
        write_i32(&mut reply, cwire::RESULT, result);
        write_i32(&mut reply, cwire::CODE, code);
        write_i64(&mut reply, cwire::ID, id);
        //[SD19-gate] reply sending is best effort: in C a failing send_reply
        // only prints one line; the single-threaded pump drops it as documented
        // instead of stalling.
        #[allow(clippy::let_underscore_must_use)]
        let _ = transport.asynsend(endpt, &mut reply);
    }

    /// C `do_open` (`hgcm.c:331-395`): bind a connection slot to the
    /// caller and ask the host for the service.
    fn do_open<T: DriverTransport>(
        &mut self,
        transport: &mut T,
        msg: &Message,
        code: &mut i32,
    ) -> i32 {
        let count = read_i32(msg, cwire::COUNT);
        if count < 0 || count > vmmdev::HGCM_NAME_SIZE as i32 {
            return EINVAL;
        }

        // Find a free slot; make sure the sender is not using up half of
        // the slots (C's `MAX(MAX_CONNS / 2, 2)` is two).
        let mut conn = None;
        let mut used = 0;
        for (index, slot) in self.conns.iter().enumerate() {
            if conn.is_none() && slot.state == State::Free {
                conn = Some(index);
            }
            if slot.endpt == msg.m_source {
                used += 1;
            }
        }
        if used >= (MAX_CONNS / 2).max(2) {
            return EMFILE;
        }
        let Some(conn) = conn else {
            return ENFILE;
        };

        // Initialize the connection: both request slots go idle (C's
        // `alloc_req` then always finds the first; the pages exist).
        self.conns[conn].state = State::Opening;
        self.conns[conn].endpt = msg.m_source;
        for req in self.conns[conn].reqs.iter_mut() {
            req.busy = false;
        }
        let req = 0;

        // The connect request: the service location and the name.
        let grant = read_i32(msg, cwire::GRANT);
        {
            let Self { conns, .. } = self;
            let slot = &mut conns[conn].reqs[req];
            bytes::write_u32(&mut slot.buffer, wire::HGCM_FLAGS, 0);
            bytes::write_u32(
                &mut slot.buffer,
                wire::HGCM_RESULT,
                vmmdev::ERR_GENERIC as u32,
            );
            bytes::write_u32(
                &mut slot.buffer,
                wire::CONNECT_TYPE,
                vmmdev::HGCM_SVCLOC_LOCALHOST_EXISTING,
            );
            if count > 0 {
                let mut name = [0u8; vmmdev::HGCM_NAME_SIZE];
                if let Err(status) =
                    transport.copy_from_grant(msg.m_source, grant, 0, &mut name[..count as usize])
                {
                    // C frees the connection before failing.
                    conns[conn].state = State::Free;
                    return status;
                }
                slot.buffer[wire::CONNECT_NAME..wire::CONNECT_NAME + count as usize]
                    .copy_from_slice(&name[..count as usize]);
            }
            // C forces the last name byte to zero.
            slot.buffer[wire::CONNECT_NAME + vmmdev::HGCM_NAME_SIZE - 1] = 0;
        }

        let result = self.start_req(
            conn,
            req,
            vmmdev::REQ_HGCM_CONNECT,
            wire::CONNECT_SIZE,
            read_i64(msg, cwire::ID),
            code,
        );
        if result != OK && result != EDONTREPLY {
            self.conns[conn].state = State::Free;
            return result;
        }
        if result == OK {
            self.finish_req(transport, conn, req, code)
        } else {
            result
        }
    }

    /// C `do_close` (`hgcm.c:400-450`): cancel whatever is in flight and
    /// ask the host to release the client identifier.
    fn do_close<T: DriverTransport>(
        &mut self,
        transport: &mut T,
        msg: &Message,
        code: &mut i32,
    ) -> i32 {
        let conn = read_i32(msg, cwire::CONN);
        if conn < 0 || conn as usize >= MAX_CONNS {
            return EINVAL;
        }
        let conn = conn as usize;
        if self.conns[conn].endpt != msg.m_source || self.conns[conn].state != State::Open {
            return EINVAL;
        }

        // Cancel any ongoing requests.
        for req in 0..MAX_REQS {
            if self.conns[conn].reqs[req].busy {
                self.cancel_req(conn, req);
            }
        }

        bytes::write_u32(
            &mut self.conns[conn].reqs[0].buffer,
            wire::DISCONNECT_CLIENT_ID,
            self.conns[conn].client_id,
        );
        let result = self.start_req(
            conn,
            0,
            vmmdev::REQ_HGCM_DISCONNECT,
            wire::DISCONNECT_SIZE,
            read_i64(msg, cwire::ID),
            code,
        );
        if result != OK && result != EDONTREPLY {
            // Neither side can do anything with a disconnection failure.
            self.conns[conn].state = State::Free;
            return OK;
        }
        self.conns[conn].state = State::Closing;
        if result == OK {
            self.finish_req(transport, conn, 0, code)
        } else {
            result
        }
    }

    /// C `do_call` (`hgcm.c:543-632`): copy the caller's parameters in,
    /// rewrite them for the device, and start the call.
    fn do_call<T: DriverTransport>(
        &mut self,
        transport: &mut T,
        msg: &Message,
        code: &mut i32,
    ) -> i32 {
        let conn = read_i32(msg, cwire::CONN);
        if conn < 0 || conn as usize >= MAX_CONNS {
            return EINVAL;
        }
        let conn = conn as usize;
        if self.conns[conn].endpt != msg.m_source || self.conns[conn].state != State::Open {
            return EINVAL;
        }

        let count = read_i32(msg, cwire::COUNT);
        // Hardening: C copies `count * sizeof(vbox_param_t)` bytes into an
        // eight-entry array without a bound check, so a count above eight
        // walks over the driver's own storage. No valid caller can observe
        // the refusal (the caller's array has no bound either).
        if count < 0 || count as usize > MAX_PARAMS {
            return EINVAL;
        }

        let req = match self.alloc_req(conn) {
            Ok(req) => req,
            Err(status) => return status,
        };

        // The caller's parameter grant and count.
        let grant = read_i32(msg, cwire::GRANT);
        self.conns[conn].reqs[req].grant = grant;
        self.conns[conn].reqs[req].count = count;
        if count > 0 {
            let end = count as usize * wire::CALLER_PARAM_SIZE;
            if let Err(status) = transport.copy_from_grant(
                msg.m_source,
                grant,
                0,
                &mut self.conns[conn].reqs[req].params[..end],
            ) {
                return status;
            }
        }

        // The call request: client, function, count, then the parameters.
        let client_id = self.conns[conn].client_id;
        let function = read_i32(msg, cwire::FUNCTION) as u32;
        let mut size = wire::CALL_SIZE + wire::PARAM_SIZE * count as usize;
        {
            let Self {
                backdoor, conns, ..
            } = self;
            let slot = &mut conns[conn].reqs[req];
            bytes::write_u32(&mut slot.buffer, wire::HGCM_FLAGS, 0);
            bytes::write_u32(
                &mut slot.buffer,
                wire::HGCM_RESULT,
                vmmdev::ERR_GENERIC as u32,
            );
            bytes::write_u32(&mut slot.buffer, wire::CALL_CLIENT_ID, client_id);
            bytes::write_u32(&mut slot.buffer, wire::CALL_FUNCTION, function);
            bytes::write_u32(&mut slot.buffer, wire::CALL_COUNT, count as u32);

            for index in 0..count as usize {
                let param = index * wire::CALLER_PARAM_SIZE;
                let out = wire::CALL_PARAMS + index * wire::PARAM_SIZE;
                let type_ = bytes::read_u32(&slot.params, param + wire::CALLER_PARAM_TYPE) as i32;
                match type_ {
                    caller_type::U32 => {
                        bytes::write_u32(
                            &mut slot.buffer,
                            out + wire::PARAM_TYPE,
                            vmmdev::HGCM_PARAM_U32,
                        );
                        let value = bytes::read_u32(&slot.params, param + wire::CALLER_PARAM_VALUE);
                        bytes::write_u32(&mut slot.buffer, out + wire::PARAM_VALUE, value);
                    }
                    caller_type::U64 => {
                        bytes::write_u32(
                            &mut slot.buffer,
                            out + wire::PARAM_TYPE,
                            vmmdev::HGCM_PARAM_U64,
                        );
                        let value = bytes::read_u64(&slot.params, param + wire::CALLER_PARAM_VALUE);
                        bytes::write_u64(&mut slot.buffer, out + wire::PARAM_VALUE, value);
                    }
                    caller_type::PTR => {
                        bytes::write_u32(
                            &mut slot.buffer,
                            out + wire::PARAM_TYPE,
                            vmmdev::HGCM_PARAM_PAGELIST,
                        );
                        // The device reads `{size, offset}` from the value
                        // slot; the page list itself goes at the running
                        // offset, which is what `offset` reports.
                        let ptr_size = bytes::read_u64(&slot.params, param + wire::CALLER_PTR_SIZE);
                        bytes::write_u32(
                            &mut slot.buffer,
                            out + wire::PARAM_VALUE,
                            ptr_size as u32,
                        );
                        bytes::write_u32(
                            &mut slot.buffer,
                            out + wire::PARAM_VALUE + 4,
                            size as u32,
                        );
                        if let Err(status) =
                            store_pages(backdoor, msg.m_source, slot, param, &mut size)
                        {
                            return status;
                        }
                    }
                    _ => return EINVAL,
                }
            }
        }

        let result = self.start_req(
            conn,
            req,
            vmmdev::REQ_HGCM_CALL,
            size,
            read_i64(msg, cwire::ID),
            code,
        );
        if result != OK && result != EDONTREPLY {
            return result;
        }
        if result == OK {
            self.finish_req(transport, conn, req, code)
        } else {
            result
        }
    }

    /// C `do_cancel` (`hgcm.c:637-670`): cancel the caller's ongoing
    /// request with the given identifier.
    fn do_cancel(&mut self, msg: &Message) -> i32 {
        let conn = read_i32(msg, cwire::CONN);
        if conn < 0 || conn as usize >= MAX_CONNS {
            return EINVAL;
        }
        let conn = conn as usize;
        if self.conns[conn].endpt != msg.m_source || self.conns[conn].state != State::Open {
            return EINVAL;
        }

        let id = read_i64(msg, cwire::ID);
        let mut found = None;
        for req in 0..MAX_REQS {
            if self.conns[conn].reqs[req].busy && self.conns[conn].reqs[req].id == id {
                found = Some(req);
                break;
            }
        }
        let Some(req) = found else {
            // C answers EINVAL for a synchronous caller and stays silent
            // for an asynchronous one (to avoid two answers to one
            // request). The transport carries no IPC status, so the
            // synchronous branch is taken: silence toward a synchronous
            // caller would deadlock it, while this protocol's clients
            // always send-and-receive, so no asynchronous caller exists
            // to receive a stray reply.
            return EINVAL;
        };

        self.cancel_req(conn, req);
        EINTR
    }

    /// C `alloc_req` (`hgcm.c:146-171`): the first idle request slot.
    ///
    /// C can also fail on allocation (ENOMEM); the pages exist for the
    /// service's whole life, so only the busy check can fail here.
    fn alloc_req(&self, conn: usize) -> Result<usize, i32> {
        for (index, req) in self.conns[conn].reqs.iter().enumerate() {
            if !req.busy {
                return Ok(index);
            }
        }
        Err(EMFILE)
    }

    /// C `start_req` (`hgcm.c:198-233`): stamp the request page and hand
    /// it to the device.
    ///
    /// The request may complete inline (the result is read back
    /// immediately) or asynchronously (the result is `EDONTREPLY` and the
    /// slot stays busy until an event arrives); either way the slot is
    /// marked busy before returning. C panics when the device write
    /// itself fails; the rewrite reports the failure instead.
    fn start_req(
        &mut self,
        conn: usize,
        req: usize,
        kind: u32,
        size: usize,
        id: i64,
        code: &mut i32,
    ) -> i32 {
        let Self {
            backdoor, conns, ..
        } = self;
        let slot = &mut conns[conn].reqs[req];
        bytes::write_u32(&mut slot.buffer, wire::HGCM_FLAGS, 0);
        bytes::write_u32(
            &mut slot.buffer,
            wire::HGCM_RESULT,
            vmmdev::ERR_GENERIC as u32,
        );
        let result = match request(backdoor, &mut slot.buffer, slot.phys, kind, size) {
            Ok(result) => result,
            Err(status) => return status,
        };
        *code = result;
        let status = convert_result(result);
        if status != OK && status != EDONTREPLY {
            return status;
        }
        slot.busy = true;
        slot.id = id;
        status
    }

    /// C `cancel_req` (`hgcm.c:207-228`): the request page's physical
    /// address identifies the request to cancel; the answer is ignored
    /// (cancellation races are inherent) and the slot goes idle.
    fn cancel_req(&mut self, conn: usize, req: usize) {
        let Self {
            backdoor, conns, ..
        } = self;
        let slot = &mut conns[conn].reqs[req];
        bytes::write_u32(&mut slot.buffer, wire::HGCM_FLAGS, 0);
        bytes::write_u32(
            &mut slot.buffer,
            wire::HGCM_RESULT,
            vmmdev::ERR_GENERIC as u32,
        );
        let _ = request(
            backdoor,
            &mut slot.buffer,
            slot.phys,
            vmmdev::REQ_HGCM_CANCEL,
            wire::CANCEL_SIZE,
        );
        slot.busy = false;
    }

    /// C `finish_req` (`hgcm.c:233-345`): a request has completed; act on
    /// the connection's state and, for a call, copy the answers back into
    /// the caller's parameters.
    fn finish_req<T: DriverTransport>(
        &mut self,
        transport: &mut T,
        conn: usize,
        req: usize,
        code: &mut i32,
    ) -> i32 {
        self.conns[conn].reqs[req].busy = false;

        let result = bytes::read_u32(&self.conns[conn].reqs[req].buffer, wire::HGCM_RESULT) as i32;
        *code = result;
        let mut status = convert_result(result);
        // The request has finished, so it cannot still be in progress.
        if status == EDONTREPLY {
            status = EGENERIC;
        }

        match self.conns[conn].state {
            State::Free => {
                // C asserts this arm unreachable; the state machine never
                // completes a request on a free connection.
                status
            }
            State::Opening => {
                if status == OK {
                    self.conns[conn].client_id = bytes::read_u32(
                        &self.conns[conn].reqs[req].buffer,
                        wire::CONNECT_CLIENT_ID,
                    );
                    self.conns[conn].state = State::Open;
                    // The connection index is the success answer: it is
                    // what the caller passes to the later calls.
                    conn as i32
                } else {
                    self.conns[conn].state = State::Free;
                    status
                }
            }
            State::Closing => {
                // Neither side can do anything with a disconnection
                // failure; C logs it.
                self.conns[conn].state = State::Free;
                OK
            }
            State::Open => {
                if status == OK {
                    let count = self.conns[conn].reqs[req].count.max(0) as usize;
                    for index in 0..count {
                        let param = index * wire::CALLER_PARAM_SIZE;
                        let out = wire::CALL_PARAMS + index * wire::PARAM_SIZE;
                        match bytes::read_u32(
                            &self.conns[conn].reqs[req].params,
                            param + wire::CALLER_PARAM_TYPE,
                        ) as i32
                        {
                            caller_type::U32 => {
                                let value = bytes::read_u32(
                                    &self.conns[conn].reqs[req].buffer,
                                    out + wire::PARAM_VALUE,
                                );
                                bytes::write_u32(
                                    &mut self.conns[conn].reqs[req].params,
                                    param + wire::CALLER_PARAM_VALUE,
                                    value,
                                );
                            }
                            caller_type::U64 => {
                                let value = bytes::read_u64(
                                    &self.conns[conn].reqs[req].buffer,
                                    out + wire::PARAM_VALUE,
                                );
                                bytes::write_u64(
                                    &mut self.conns[conn].reqs[req].params,
                                    param + wire::CALLER_PARAM_VALUE,
                                    value,
                                );
                            }
                            // Buffer parameters travel through the
                            // device's own page lists, not this copy.
                            _ => {}
                        }
                    }
                    if count > 0 {
                        let end = count * wire::CALLER_PARAM_SIZE;
                        return match transport.copy_to_grant(
                            self.conns[conn].endpt,
                            self.conns[conn].reqs[req].grant,
                            0,
                            &self.conns[conn].reqs[req].params[..end],
                        ) {
                            Ok(()) => status,
                            Err(failure) => failure,
                        };
                    }
                }
                status
            }
        }
    }
}

impl<B: Backdoor, T: DriverTransport> DriverHandler<T> for VboxService<B> {
    fn handle(&mut self, transport: &mut T, msg: &Message) {
        self.dispatch(transport, msg);
    }
}

/// C `vbox_request` (`vbox.c:33-48`): stamp the request header, hand the
/// page's physical address to the backdoor port, and read the device's
/// result back out of the page.
fn request<B: Backdoor>(
    backdoor: &mut B,
    buffer: &mut [u8; BUF_SIZE],
    phys: u64,
    kind: u32,
    size: usize,
) -> Result<i32, i32> {
    bytes::write_u32(buffer, wire::HEADER_SIZE_FIELD, size as u32);
    bytes::write_u32(buffer, wire::HEADER_VERSION, vmmdev::BACKDOOR_VERSION);
    bytes::write_u32(buffer, wire::HEADER_TYPE, kind);
    bytes::write_u32(buffer, wire::HEADER_RESULT, vmmdev::ERR_GENERIC as u32);
    backdoor.outl(phys as u32)?;
    Ok(bytes::read_u32(buffer, wire::HEADER_RESULT) as i32)
}

/// C `store_pages` (`hgcm.c:349-470`): append the page list for one
/// buffer parameter at the running offset and grow it.
///
/// The list is built from the spans the backdoor maps the caller's grant
/// into: the first span's leading partial page is absorbed by the page
/// offset, every full page becomes one address, and a span longer than a
/// page is split across iterations. The page list cannot exceed the
/// request page; C reports ENOMEM when it would.
fn store_pages<B: Backdoor>(
    backdoor: &mut B,
    endpt: Endpoint,
    slot: &mut Req,
    param: usize,
    size: &mut usize,
) -> Result<(), i32> {
    let ptr_size = bytes::read_u64(&slot.params, param + wire::CALLER_PTR_SIZE);
    if ptr_size == 0 {
        // Empty buffers are allowed and carry no page list.
        return Ok(());
    }
    let grant = bytes::read_u32(&slot.params, param + wire::CALLER_PTR_GRANT) as i32;
    let ptr_off = bytes::read_u64(&slot.params, param + wire::CALLER_PTR_OFF);
    let dir = bytes::read_u32(&slot.params, param + wire::CALLER_PTR_DIR);

    let list = *size;
    let mut flags = 0;
    if dir & caller_dir::IN != 0 {
        flags |= vmmdev::HGCM_FLAG_FROM_HOST;
    }
    if dir & caller_dir::OUT != 0 {
        flags |= vmmdev::HGCM_FLAG_TO_HOST;
    }
    bytes::write_u32(&mut slot.buffer, list + wire::PAGE_LIST_FLAGS, flags);
    bytes::write_u16(&mut slot.buffer, list + wire::PAGE_LIST_OFFSET, 0);
    bytes::write_u16(&mut slot.buffer, list + wire::PAGE_LIST_COUNT, 0);
    *size += wire::PAGE_LIST_HEADER;
    if *size > BUF_SIZE {
        return Err(ENOMEM);
    }

    let mut access = 0;
    if dir & caller_dir::IN != 0 {
        access |= minix_sys::syscall::VUA_WRITE;
    }
    if dir & caller_dir::OUT != 0 {
        access |= minix_sys::syscall::VUA_READ;
    }

    let mut offset = 0u64;
    let mut first = true;
    loop {
        let mut spans = [VumapPhys::default(); MAPVEC_NR];
        let count = backdoor.map_grant(
            endpt,
            grant,
            ptr_off + ptr_size,
            ptr_off + offset,
            access,
            &mut spans,
        )?;
        if count == 0 {
            // A non-empty buffer that maps to nothing: the C code would
            // index the empty vector.
            return Err(EFAULT);
        }
        let mut span_size = 0u64;
        for span in &spans[..count] {
            span_size += span.size;
        }

        // VirtualBox wants aligned page addresses plus an offset into the
        // first page: absorb the first span's partial page.
        let mut skip = 0u64;
        if first {
            skip = spans[0].addr & (PAGE_SIZE - 1);
            spans[0].addr -= skip;
            spans[0].size += skip;
            bytes::write_u16(&mut slot.buffer, list + wire::PAGE_LIST_OFFSET, skip as u16);
            first = false;
        }

        // How many page addresses this round needs (C's
        // `(skip + size + PAGE_SIZE - 1) / PAGE_SIZE`).
        let pages = (skip + span_size).div_ceil(PAGE_SIZE);
        *size += wire::PAGE_LIST_ADDR_SIZE * pages as usize;
        if *size > BUF_SIZE {
            return Err(ENOMEM);
        }

        // Store them, splitting any span that covers more than a page.
        let mut stored = bytes::read_u16(&slot.buffer, list + wire::PAGE_LIST_COUNT) as usize;
        let mut index = 0usize;
        for _ in 0..pages {
            bytes::write_u64(
                &mut slot.buffer,
                list + wire::PAGE_LIST_ADDR + stored * wire::PAGE_LIST_ADDR_SIZE,
                spans[index].addr,
            );
            stored += 1;
            if spans[index].size > PAGE_SIZE {
                spans[index].addr += PAGE_SIZE;
                spans[index].size -= PAGE_SIZE;
            } else {
                index += 1;
            }
        }
        bytes::write_u16(
            &mut slot.buffer,
            list + wire::PAGE_LIST_COUNT,
            stored as u16,
        );

        offset += span_size;
        if offset >= ptr_size {
            break;
        }
    }

    Ok(())
}

/// Reads a 32-bit field out of a message's payload.
fn read_i32(msg: &Message, offset: usize) -> i32 {
    // SAFETY: byte-level read of the union payload at a documented offset.
    let raw = unsafe { msg.m_u.raw };
    i32::from_ne_bytes(raw[offset..offset + 4].try_into().unwrap())
}

/// Reads a 64-bit field out of a message's payload.
fn read_i64(msg: &Message, offset: usize) -> i64 {
    // SAFETY: byte-level read of the union payload at a documented offset.
    let raw = unsafe { msg.m_u.raw };
    i64::from_ne_bytes(raw[offset..offset + 8].try_into().unwrap())
}

/// Writes a 32-bit field into a message's payload.
fn write_i32(msg: &mut Message, offset: usize, value: i32) {
    // SAFETY: byte-level write of the union payload at a documented offset.
    unsafe {
        msg.m_u.raw[offset..offset + 4].copy_from_slice(&value.to_ne_bytes());
    }
}

/// Writes a 64-bit field into a message's payload.
fn write_i64(msg: &mut Message, offset: usize, value: i64) {
    // SAFETY: byte-level write of the union payload at a documented offset.
    unsafe {
        msg.m_u.raw[offset..offset + 8].copy_from_slice(&value.to_ne_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::boxed::Box;
    use std::collections::HashMap;
    use std::vec;
    use std::vec::Vec;

    /// What the device does with one request: the result code plus
    /// whatever it writes into the page.
    type Responder = Box<dyn Fn(u32, &mut [u8; BUF_SIZE]) -> i32>;

    /// A scripted backdoor: answers each request through a closure that
    /// mutates the request page the way the device would, records
    /// everything the service did, and hands out fake physical addresses
    /// and grant spans.
    struct FakeBackdoor {
        hz: u32,
        now: i64,
        /// Physical addresses handed out, one per `physical` call.
        phys_next: u64,
        /// The pages the service asked physical addresses for, keyed by
        /// the address the backdoor will see in `outl` — the device's view
        /// of memory.
        pages: HashMap<u32, *mut u8>,
        /// Every device request: kind, size, and the page exactly as the
        /// service stamped it (before the device's answer).
        requests: Vec<(u32, usize, Vec<u8>)>,
        /// The device's behaviour: result code plus whatever the device
        /// writes into the page for that request kind.
        responder: Option<Responder>,
        times_set: Vec<i64>,
        alarms: Vec<u64>,
        irq_enables: usize,
        /// The spans `map_grant` hands back.
        spans: Vec<VumapPhys>,
        map_calls: Vec<(Endpoint, i32, u64, u64, i32)>,
    }

    impl FakeBackdoor {
        fn new() -> Self {
            FakeBackdoor {
                hz: 100,
                now: 1_000_000,
                phys_next: 0x1000_0000,
                pages: HashMap::new(),
                requests: Vec::new(),
                responder: None,
                times_set: Vec::new(),
                alarms: Vec::new(),
                irq_enables: 0,
                spans: Vec::new(),
                map_calls: Vec::new(),
            }
        }
    }

    impl Backdoor for FakeBackdoor {
        fn outl(&mut self, value: u32) -> Result<(), i32> {
            // The device reads the request out of the page the address
            // names and answers by writing back into it.
            let page = *self
                .pages
                .get(&value)
                .expect("the backdoor only sees registered pages");
            // SAFETY(test): the registered pointer is a live page of the
            // service under test, which outlives this call.
            let page = unsafe { &mut *(page as *mut [u8; BUF_SIZE]) };
            let kind = bytes::read_u32(page, wire::HEADER_TYPE);
            let size = bytes::read_u32(page, wire::HEADER_SIZE_FIELD) as usize;
            self.requests.push((kind, size, page.to_vec()));
            let result = match &self.responder {
                Some(responder) => responder(kind, page),
                None => vmmdev::ERR_OK,
            };
            bytes::write_u32(page, wire::HEADER_RESULT, result as u32);
            Ok(())
        }

        fn physical(&mut self, buffer: *const u8) -> Result<u64, i32> {
            let phys = self.phys_next;
            self.phys_next += PAGE_SIZE;
            self.pages.insert(phys as u32, buffer as *mut u8);
            Ok(phys)
        }

        fn now_seconds(&mut self) -> Result<i64, i32> {
            Ok(self.now)
        }

        fn set_time(&mut self, seconds: i64) -> Result<(), i32> {
            self.times_set.push(seconds);
            Ok(())
        }

        fn set_alarm(&mut self, ticks: u64) -> Result<(), i32> {
            self.alarms.push(ticks);
            Ok(())
        }

        fn irq_enable(&mut self) -> Result<(), i32> {
            self.irq_enables += 1;
            Ok(())
        }

        fn hz(&mut self) -> Result<u32, i32> {
            Ok(self.hz)
        }

        fn map_grant(
            &mut self,
            granter: Endpoint,
            grant: i32,
            grant_size: u64,
            offset: u64,
            access: i32,
            out: &mut [VumapPhys],
        ) -> Result<usize, i32> {
            self.map_calls
                .push((granter, grant, grant_size, offset, access));
            let count = self.spans.len().min(out.len());
            out[..count].copy_from_slice(&self.spans[..count]);
            Ok(count)
        }
    }

    /// The message-side transport: records replies and serves scripted
    /// grant contents.
    struct FakeTransport {
        /// Replies: message type, result, code, identifier.
        replies: Vec<(Endpoint, i32, i32, i32, i64)>,
        /// Contents `copy_from_grant` hands back, keyed by grant.
        grants: HashMap<i32, Vec<u8>>,
        /// Every `copy_to_grant`: caller, grant, bytes.
        copies: Vec<(Endpoint, i32, Vec<u8>)>,
    }

    impl FakeTransport {
        fn new() -> Self {
            FakeTransport {
                replies: Vec::new(),
                grants: HashMap::new(),
                copies: Vec::new(),
            }
        }
    }

    impl DriverTransport for FakeTransport {
        fn receive(&mut self, _msg: &mut Message) -> Result<(), i32> {
            Err(-minix_types::EIO)
        }
        fn send(&mut self, _dst: Endpoint, _msg: &mut Message) -> Result<(), i32> {
            Ok(())
        }
        fn send_rec(&mut self, _dst: Endpoint, _msg: &mut Message) -> Result<(), i32> {
            Ok(())
        }
        fn asynsend(&mut self, dst: Endpoint, msg: &mut Message) -> Result<(), i32> {
            self.replies.push((
                dst,
                msg.m_type,
                read_i32(msg, cwire::RESULT),
                read_i32(msg, cwire::CODE),
                read_i64(msg, cwire::ID),
            ));
            Ok(())
        }
        fn copy_from_grant(
            &mut self,
            _granter: Endpoint,
            grant: i32,
            _offset: u64,
            buf: &mut [u8],
        ) -> Result<(), i32> {
            match self.grants.get(&grant) {
                Some(bytes) => {
                    buf.copy_from_slice(&bytes[..buf.len()]);
                    Ok(())
                }
                None => Err(-minix_types::EFAULT),
            }
        }
        fn copy_to_grant(
            &mut self,
            granter: Endpoint,
            grant: i32,
            _offset: u64,
            buf: &[u8],
        ) -> Result<(), i32> {
            self.copies.push((granter, grant, Vec::from(buf)));
            Ok(())
        }
        fn publish_label(&mut self, _label: &str) -> Result<(), i32> {
            Ok(())
        }
    }

    /// Builds one caller parameter (`vbox_param_t`).
    fn caller_u32(value: u32) -> Vec<u8> {
        let mut param = vec![0u8; wire::CALLER_PARAM_SIZE];
        param[wire::CALLER_PARAM_TYPE..wire::CALLER_PARAM_TYPE + 4]
            .copy_from_slice(&(caller_type::U32 as u32).to_le_bytes());
        param[wire::CALLER_PARAM_VALUE..wire::CALLER_PARAM_VALUE + 4]
            .copy_from_slice(&value.to_le_bytes());
        param
    }

    fn caller_u64(value: u64) -> Vec<u8> {
        let mut param = vec![0u8; wire::CALLER_PARAM_SIZE];
        param[wire::CALLER_PARAM_TYPE..wire::CALLER_PARAM_TYPE + 4]
            .copy_from_slice(&(caller_type::U64 as u32).to_le_bytes());
        param[wire::CALLER_PARAM_VALUE..wire::CALLER_PARAM_VALUE + 8]
            .copy_from_slice(&value.to_le_bytes());
        param
    }

    fn caller_ptr(grant: i32, off: u64, size: u64, dir: u32) -> Vec<u8> {
        let mut param = vec![0u8; wire::CALLER_PARAM_SIZE];
        param[wire::CALLER_PARAM_TYPE..wire::CALLER_PARAM_TYPE + 4]
            .copy_from_slice(&(caller_type::PTR as u32).to_le_bytes());
        param[wire::CALLER_PTR_GRANT..wire::CALLER_PTR_GRANT + 4]
            .copy_from_slice(&grant.to_le_bytes());
        param[wire::CALLER_PTR_OFF..wire::CALLER_PTR_OFF + 8].copy_from_slice(&off.to_le_bytes());
        param[wire::CALLER_PTR_SIZE..wire::CALLER_PTR_SIZE + 8]
            .copy_from_slice(&size.to_le_bytes());
        param[wire::CALLER_PTR_DIR..wire::CALLER_PTR_DIR + 4].copy_from_slice(&dir.to_le_bytes());
        param
    }

    /// A request message of the given type with the fields set.
    fn request_message(
        kind: i32,
        caller: Endpoint,
        conn: i32,
        grant: i32,
        count: i32,
        function: i32,
        id: i64,
    ) -> Message {
        let mut msg = Message {
            m_type: kind,
            m_source: caller,
            ..Message::default()
        };
        write_i32(&mut msg, cwire::CONN, conn);
        write_i32(&mut msg, cwire::GRANT, grant);
        write_i32(&mut msg, cwire::COUNT, count);
        write_i32(&mut msg, cwire::FUNCTION, function);
        write_i64(&mut msg, cwire::ID, id);
        msg
    }

    fn notify(source: Endpoint) -> Message {
        Message {
            m_type: 0x1000,
            m_source: source,
            ..Message::default()
        }
    }

    #[test]
    fn test_start_reports_the_guest_and_runs_the_first_time_update() {
        let mut backdoor = FakeBackdoor::new();
        backdoor.now = 1_000_000;
        let mut service = VboxService::new(backdoor);

        // The responder fills the host time (5 seconds ahead of `now`) so
        // the drift rule fires, and acknowledges everything else.
        service.backdoor.responder = Some(Box::new(|kind, page| {
            if kind == vmmdev::REQ_HOST_TIME {
                bytes::write_u64(page, wire::HOST_TIME_TIME, 1_000_005_000);
            }
            vmmdev::ERR_OK
        }));
        assert_eq!(service.start(), Ok(()));

        let (kind, size, page) = service.backdoor.requests[0].clone();
        assert_eq!(kind, vmmdev::REQ_REPORT_GUEST_INFO);
        assert_eq!(size, wire::REPORT_SIZE);
        assert_eq!(
            bytes::read_u32(&page, wire::REPORT_ADD_VERSION),
            vmmdev::GUEST_VERSION
        );
        assert_eq!(
            bytes::read_u32(&page, wire::REPORT_OS_TYPE),
            vmmdev::GUEST_OS_TYPE
        );
        assert_eq!(
            bytes::read_u32(&page, wire::HEADER_TYPE),
            vmmdev::REQ_REPORT_GUEST_INFO
        );
        assert_eq!(
            bytes::read_u32(&page, wire::HEADER_VERSION),
            vmmdev::BACKDOOR_VERSION
        );
        assert_eq!(
            bytes::read_u32(&page, wire::HEADER_SIZE_FIELD) as usize,
            size
        );

        // The first time update ran and moved the clock forward.
        let (kind, size, _) = service.backdoor.requests[1].clone();
        assert_eq!(kind, vmmdev::REQ_HOST_TIME);
        assert_eq!(size, wire::HOST_TIME_SIZE);
        assert_eq!(service.backdoor.times_set, vec![1_000_005]);
        // The alarm period is hz times the interval (default one second).
        assert_eq!(service.backdoor.alarms, vec![100]);

        // Every request page got a distinct physical address.
        assert_eq!(service.device_phys, 0x1000_0000);
    }

    #[test]
    fn test_sync_time_never_moves_backward_and_rearms_the_alarm() {
        let mut backdoor = FakeBackdoor::new();
        backdoor.now = 1_000_000;
        let mut service = VboxService::new(backdoor);
        started(&mut service);
        service.backdoor.responder = Some(Box::new(|kind, page| {
            if kind == vmmdev::REQ_HOST_TIME {
                // One second ahead: below the drift threshold.
                bytes::write_u64(page, wire::HOST_TIME_TIME, 1_000_001_000);
            }
            vmmdev::ERR_OK
        }));
        service.sync_time();
        assert!(service.backdoor.times_set.is_empty());
        assert_eq!(service.backdoor.alarms.len(), 1);

        // The host clock behind the guest's never moves time backward.
        service.backdoor.responder = Some(Box::new(|kind, page| {
            if kind == vmmdev::REQ_HOST_TIME {
                bytes::write_u64(page, wire::HOST_TIME_TIME, 999_000_000);
            }
            vmmdev::ERR_OK
        }));
        service.sync_time();
        assert!(service.backdoor.times_set.is_empty());

        // A failed host-time request leaves the clock alone too.
        service.backdoor.responder = Some(Box::new(|kind, _page| {
            if kind == vmmdev::REQ_HOST_TIME {
                vmmdev::ERR_GENERIC
            } else {
                vmmdev::ERR_OK
            }
        }));
        service.sync_time();
        assert!(service.backdoor.times_set.is_empty());
        assert_eq!(
            service.backdoor.alarms.len(),
            3,
            "the alarm is rearmed every time"
        );
    }

    /// Registers a service's request pages with the backdoor and clears
    /// the records `start` leaves behind, so each test sees only its own
    /// traffic.
    ///
    /// The service must already sit in its final location: `start`
    /// resolves the pages' physical addresses from their addresses, and a
    /// later move would leave the backdoor looking at the old place.
    fn started(service: &mut VboxService<FakeBackdoor>) {
        assert_eq!(service.start(), Ok(()));
        service.backdoor.requests.clear();
        service.backdoor.times_set.clear();
        service.backdoor.alarms.clear();
    }

    /// Opens a connection through the service with a synchronous answer
    /// from the device.
    fn open_connection(
        service: &mut VboxService<FakeBackdoor>,
        transport: &mut FakeTransport,
        caller: Endpoint,
    ) -> i32 {
        transport.grants.insert(7, b"VBoxService\0".to_vec());
        let msg = request_message(protocol::VBOX_OPEN, caller, 0, 7, 12, 0, 0);
        service.dispatch(transport, &msg);
        let reply = transport.replies.last().unwrap();
        reply.2
    }

    #[test]
    fn test_open_returns_the_connection_index_and_the_device_client_id() {
        let mut backdoor = FakeBackdoor::new();
        backdoor.responder = Some(Box::new(|kind, page| {
            if kind == vmmdev::REQ_HGCM_CONNECT {
                // The device fills the client identifier and reports
                // success inline.
                bytes::write_u32(page, wire::CONNECT_CLIENT_ID, 0x1234);
                bytes::write_u32(page, wire::HGCM_RESULT, vmmdev::ERR_OK as u32);
            }
            vmmdev::ERR_OK
        }));
        let mut service = VboxService::new(backdoor);
        started(&mut service);
        let mut transport = FakeTransport::new();

        assert_eq!(
            open_connection(&mut service, &mut transport, Endpoint(20)),
            OK
        );
        let (kind, size, page) = service.backdoor.requests.last().unwrap().clone();
        assert_eq!(kind, vmmdev::REQ_HGCM_CONNECT);
        assert_eq!(size, wire::CONNECT_SIZE);
        assert_eq!(
            bytes::read_u32(&page, wire::CONNECT_TYPE),
            vmmdev::HGCM_SVCLOC_LOCALHOST_EXISTING
        );
        assert_eq!(
            &page[wire::CONNECT_NAME..wire::CONNECT_NAME + 12],
            b"VBoxService\0"
        );
        assert_eq!(page[wire::CONNECT_NAME + vmmdev::HGCM_NAME_SIZE - 1], 0);

        // The reply: connection index as the result, the device code, and
        // the caller's identifier.
        let reply = transport.replies.last().unwrap();
        assert_eq!(reply.0, Endpoint(20));
        assert_eq!(reply.1, protocol::VBOX_REPLY);
        assert_eq!(reply.2, 0, "connection index zero");
        assert_eq!(reply.3, vmmdev::ERR_OK);
        assert_eq!(reply.4, 0, "the caller's identifier is echoed");

        // A second connection from the same endpoint is allowed, a third
        // is not (the endpoint may not use up half of the four slots).
        assert_eq!(
            open_connection(&mut service, &mut transport, Endpoint(20)),
            1
        );
        assert_eq!(
            open_connection(&mut service, &mut transport, Endpoint(20)),
            EMFILE
        );
    }

    #[test]
    fn test_open_refusals_and_the_close_lifecycle() {
        let mut backdoor = FakeBackdoor::new();
        backdoor.responder = Some(Box::new(|kind, page| {
            if kind == vmmdev::REQ_HGCM_CONNECT {
                bytes::write_u32(page, wire::CONNECT_CLIENT_ID, 0x77);
                bytes::write_u32(page, wire::HGCM_RESULT, vmmdev::ERR_OK as u32);
            }
            if kind == vmmdev::REQ_HGCM_DISCONNECT {
                bytes::write_u32(page, wire::HGCM_RESULT, vmmdev::ERR_OK as u32);
            }
            vmmdev::ERR_OK
        }));
        let mut service = VboxService::new(backdoor);
        started(&mut service);
        let mut transport = FakeTransport::new();

        // A name longer than the field is refused.
        let msg = request_message(
            protocol::VBOX_OPEN,
            Endpoint(20),
            0,
            7,
            vmmdev::HGCM_NAME_SIZE as i32 + 1,
            0,
            0,
        );
        service.dispatch(&mut transport, &msg);
        assert_eq!(transport.replies.last().unwrap().2, EINVAL);

        // A caller with no free slot left gets ENFILE once four other
        // endpoints hold one each; the answer is the connection index.
        for caller in 20..24 {
            let index = open_connection(&mut service, &mut transport, Endpoint(caller));
            assert_eq!(index, caller - 20);
        }
        assert_eq!(
            open_connection(&mut service, &mut transport, Endpoint(30)),
            ENFILE
        );

        // Close the first connection: the device sees the client id.
        let msg = request_message(protocol::VBOX_CLOSE, Endpoint(20), 0, 0, 0, 0, 0);
        service.dispatch(&mut transport, &msg);
        let reply = transport.replies.last().unwrap();
        assert_eq!(reply.2, OK);
        let (kind, _, page) = service.backdoor.requests.last().unwrap().clone();
        assert_eq!(kind, vmmdev::REQ_HGCM_DISCONNECT);
        assert_eq!(bytes::read_u32(&page, wire::DISCONNECT_CLIENT_ID), 0x77);

        // Closing from a different endpoint, or closing twice, is invalid.
        let msg = request_message(protocol::VBOX_CLOSE, Endpoint(21), 0, 0, 0, 0, 0);
        service.dispatch(&mut transport, &msg);
        assert_eq!(transport.replies.last().unwrap().2, EINVAL);
        let msg = request_message(protocol::VBOX_CLOSE, Endpoint(20), 0, 0, 0, 0, 0);
        service.dispatch(&mut transport, &msg);
        assert_eq!(transport.replies.last().unwrap().2, EINVAL);

        // A call on a closed connection is invalid as well.
        let msg = request_message(protocol::VBOX_CALL, Endpoint(20), 0, 0, 0, 0, 0);
        service.dispatch(&mut transport, &msg);
        assert_eq!(transport.replies.last().unwrap().2, EINVAL);
    }

    #[test]
    fn test_async_call_completes_through_the_interrupt() {
        let mut backdoor = FakeBackdoor::new();
        backdoor.responder = Some(Box::new(|kind, page| match kind {
            vmmdev::REQ_HGCM_CONNECT => {
                bytes::write_u32(page, wire::CONNECT_CLIENT_ID, 0x1);
                bytes::write_u32(page, wire::HGCM_RESULT, vmmdev::ERR_OK as u32);
                vmmdev::ERR_OK
            }
            vmmdev::REQ_HGCM_CALL => {
                // The device writes the answers and marks the request
                // done, but the call itself is still in progress.
                bytes::write_u32(page, wire::HGCM_RESULT, vmmdev::ERR_OK as u32);
                bytes::write_u32(page, wire::HGCM_FLAGS, vmmdev::HGCM_REQ_DONE);
                let first = wire::CALL_PARAMS;
                bytes::write_u32(page, first + wire::PARAM_VALUE, 0xdead_beef);
                let second = wire::CALL_PARAMS + wire::PARAM_SIZE;
                bytes::write_u64(page, second + wire::PARAM_VALUE, 0x0123_4567_89ab_cdef);
                vmmdev::ERR_HGCM_ASYNC_EXEC
            }
            vmmdev::REQ_ACKNOWLEDGE_EVENTS => {
                bytes::write_u32(page, wire::EVENTS_MASK, vmmdev::EVENT_HGCM);
                vmmdev::ERR_OK
            }
            _ => vmmdev::ERR_OK,
        }));
        let mut service = VboxService::new(backdoor);
        started(&mut service);
        let mut transport = FakeTransport::new();
        assert_eq!(
            open_connection(&mut service, &mut transport, Endpoint(20)),
            OK
        );
        let replies_before = transport.replies.len();

        // The call: one 32-bit parameter and one 64-bit parameter.
        let mut params = caller_u32(0x1111);
        params.extend(caller_u64(0x2222_3333_4444_5555));
        transport.grants.insert(9, params);
        let msg = request_message(protocol::VBOX_CALL, Endpoint(20), 0, 9, 2, 42, 7);
        service.dispatch(&mut transport, &msg);
        // Still in progress: no reply yet.
        assert_eq!(transport.replies.len(), replies_before);
        let (kind, size, page) = service.backdoor.requests.last().unwrap().clone();
        assert_eq!(kind, vmmdev::REQ_HGCM_CALL);
        assert_eq!(size, wire::CALL_SIZE + 2 * wire::PARAM_SIZE);
        assert_eq!(bytes::read_u32(&page, wire::CALL_FUNCTION), 42);
        assert_eq!(bytes::read_u32(&page, wire::CALL_COUNT), 2);
        assert_eq!(bytes::read_u32(&page, wire::CALL_CLIENT_ID), 0x1);
        let first = wire::CALL_PARAMS;
        assert_eq!(
            bytes::read_u32(&page, first + wire::PARAM_TYPE),
            vmmdev::HGCM_PARAM_U32
        );
        assert_eq!(bytes::read_u32(&page, first + wire::PARAM_VALUE), 0x1111);
        let second = wire::CALL_PARAMS + wire::PARAM_SIZE;
        assert_eq!(
            bytes::read_u32(&page, second + wire::PARAM_TYPE),
            vmmdev::HGCM_PARAM_U64
        );
        assert_eq!(
            bytes::read_u64(&page, second + wire::PARAM_VALUE),
            0x2222_3333_4444_5555
        );

        // The hardware notification completes it: the answers are copied
        // back into the caller's parameters and the reply carries the
        // device's code and the caller's identifier.
        service.dispatch(&mut transport, &notify(Endpoint::HARDWARE));
        let reply = transport.replies.last().unwrap();
        assert_eq!(reply.0, Endpoint(20));
        assert_eq!(reply.2, OK);
        assert_eq!(reply.3, vmmdev::ERR_OK);
        assert_eq!(reply.4, 7, "the caller's identifier comes back");
        assert_eq!(service.backdoor.irq_enables, 1);
        let (granter, grant, bytes) = transport.copies.last().unwrap();
        assert_eq!(*granter, Endpoint(20));
        assert_eq!(*grant, 9);
        assert_eq!(bytes.len(), 2 * wire::CALLER_PARAM_SIZE);
        assert_eq!(
            bytes[wire::CALLER_PARAM_VALUE..wire::CALLER_PARAM_VALUE + 4],
            0xdead_beefu32.to_le_bytes()
        );
        assert_eq!(
            bytes[wire::CALLER_PARAM_SIZE + wire::CALLER_PARAM_VALUE
                ..wire::CALLER_PARAM_SIZE + wire::CALLER_PARAM_VALUE + 8],
            0x0123_4567_89ab_cdefu64.to_le_bytes()
        );

        // A second notification with nothing pending changes nothing.
        let replies = transport.replies.len();
        service.dispatch(&mut transport, &notify(Endpoint::HARDWARE));
        assert_eq!(transport.replies.len(), replies);
    }

    #[test]
    fn test_call_parameters_page_list_and_refusals() {
        let mut backdoor = FakeBackdoor::new();
        backdoor.responder = Some(Box::new(|kind, page| {
            if kind == vmmdev::REQ_HGCM_CONNECT {
                bytes::write_u32(page, wire::CONNECT_CLIENT_ID, 0x1);
                bytes::write_u32(page, wire::HGCM_RESULT, vmmdev::ERR_OK as u32);
            }
            if kind == vmmdev::REQ_HGCM_CALL {
                bytes::write_u32(page, wire::HGCM_RESULT, vmmdev::ERR_OK as u32);
            }
            vmmdev::ERR_OK
        }));
        // One span of two pages, starting mid-page: the list must absorb
        // the offset and split the second page.
        backdoor.spans = vec![VumapPhys {
            addr: 0x2000_0100,
            size: 0x1f00,
        }];
        let mut service = VboxService::new(backdoor);
        started(&mut service);
        let mut transport = FakeTransport::new();
        assert_eq!(
            open_connection(&mut service, &mut transport, Endpoint(20)),
            OK
        );

        // A buffer parameter with both directions.
        let params = caller_ptr(11, 0x40, 0x1000, caller_dir::IN | caller_dir::OUT);
        transport.grants.insert(9, params);
        let msg = request_message(protocol::VBOX_CALL, Endpoint(20), 0, 9, 1, 1, 0);
        service.dispatch(&mut transport, &msg);
        assert_eq!(transport.replies.last().unwrap().2, OK);

        let (kind, size, page) = service.backdoor.requests.last().unwrap().clone();
        assert_eq!(kind, vmmdev::REQ_HGCM_CALL);
        let list = wire::CALL_SIZE + wire::PARAM_SIZE;
        assert_eq!(
            size,
            list + wire::PAGE_LIST_HEADER + 2 * wire::PAGE_LIST_ADDR_SIZE
        );
        // The parameter points at the list and carries the buffer's size.
        assert_eq!(
            bytes::read_u32(&page, wire::CALL_PARAMS + wire::PARAM_TYPE),
            vmmdev::HGCM_PARAM_PAGELIST
        );
        assert_eq!(
            bytes::read_u32(&page, wire::CALL_PARAMS + wire::PARAM_VALUE),
            0x1000
        );
        assert_eq!(
            bytes::read_u32(&page, wire::CALL_PARAMS + wire::PARAM_VALUE + 4) as usize,
            list
        );
        // The list: both direction flags, the first page's offset, two
        // aligned addresses.
        assert_eq!(
            bytes::read_u32(&page, list + wire::PAGE_LIST_FLAGS),
            vmmdev::HGCM_FLAG_FROM_HOST | vmmdev::HGCM_FLAG_TO_HOST
        );
        assert_eq!(bytes::read_u16(&page, list + wire::PAGE_LIST_OFFSET), 0x100);
        assert_eq!(bytes::read_u16(&page, list + wire::PAGE_LIST_COUNT), 2);
        assert_eq!(
            bytes::read_u64(&page, list + wire::PAGE_LIST_ADDR),
            0x2000_0000
        );
        assert_eq!(
            bytes::read_u64(
                &page,
                list + wire::PAGE_LIST_ADDR + wire::PAGE_LIST_ADDR_SIZE
            ),
            0x2000_1000
        );
        // The grant mapping was asked for the whole buffer from its own
        // offset, with the direction's access bits.
        let (granter, grant, grant_size, offset, access) =
            service.backdoor.map_calls.last().unwrap();
        assert_eq!(*granter, Endpoint(20));
        assert_eq!(*grant, 11);
        assert_eq!(*grant_size, 0x40 + 0x1000);
        assert_eq!(*offset, 0x40);
        assert_eq!(
            *access,
            minix_sys::syscall::VUA_READ | minix_sys::syscall::VUA_WRITE
        );

        // Too many parameters is refused (C would overflow its array).
        let msg = request_message(
            protocol::VBOX_CALL,
            Endpoint(20),
            0,
            9,
            MAX_PARAMS as i32 + 1,
            1,
            0,
        );
        service.dispatch(&mut transport, &msg);
        assert_eq!(transport.replies.last().unwrap().2, EINVAL);

        // An unknown parameter type is refused.
        let mut bad = vec![0u8; wire::CALLER_PARAM_SIZE];
        bad[wire::CALLER_PARAM_TYPE..wire::CALLER_PARAM_TYPE + 4]
            .copy_from_slice(&9u32.to_le_bytes());
        transport.grants.insert(9, bad);
        let msg = request_message(protocol::VBOX_CALL, Endpoint(20), 0, 9, 1, 1, 0);
        service.dispatch(&mut transport, &msg);
        assert_eq!(transport.replies.last().unwrap().2, EINVAL);

        // Both request slots busy: the third call gets EMFILE. The first
        // call is left in flight by the responder (synchronous success
        // leaves the slot idle), so script two asynchronous ones instead.
        service.backdoor.responder = Some(Box::new(|kind, page| {
            if kind == vmmdev::REQ_HGCM_CALL {
                vmmdev::ERR_HGCM_ASYNC_EXEC
            } else {
                let _ = page;
                vmmdev::ERR_OK
            }
        }));
        let params = caller_u32(1);
        transport.grants.insert(9, params);
        let msg = request_message(protocol::VBOX_CALL, Endpoint(20), 0, 9, 1, 1, 10);
        service.dispatch(&mut transport, &msg);
        let msg = request_message(protocol::VBOX_CALL, Endpoint(20), 0, 9, 1, 1, 11);
        service.dispatch(&mut transport, &msg);
        let msg = request_message(protocol::VBOX_CALL, Endpoint(20), 0, 9, 1, 1, 12);
        service.dispatch(&mut transport, &msg);
        assert_eq!(transport.replies.last().unwrap().2, EMFILE);
    }

    #[test]
    fn test_cancel_semantics() {
        let mut backdoor = FakeBackdoor::new();
        backdoor.responder = Some(Box::new(|kind, page| {
            if kind == vmmdev::REQ_HGCM_CONNECT {
                bytes::write_u32(page, wire::CONNECT_CLIENT_ID, 0x1);
                bytes::write_u32(page, wire::HGCM_RESULT, vmmdev::ERR_OK as u32);
            }
            if kind == vmmdev::REQ_HGCM_CALL {
                return vmmdev::ERR_HGCM_ASYNC_EXEC;
            }
            vmmdev::ERR_OK
        }));
        let mut service = VboxService::new(backdoor);
        started(&mut service);
        let mut transport = FakeTransport::new();
        assert_eq!(
            open_connection(&mut service, &mut transport, Endpoint(20)),
            OK
        );

        // Start an asynchronous call, then cancel it by identifier.
        transport.grants.insert(9, caller_u32(1));
        let msg = request_message(protocol::VBOX_CALL, Endpoint(20), 0, 9, 1, 1, 55);
        service.dispatch(&mut transport, &msg);
        let msg = request_message(protocol::VBOX_CANCEL, Endpoint(20), 0, 0, 0, 0, 55);
        service.dispatch(&mut transport, &msg);
        assert_eq!(transport.replies.last().unwrap().2, EINTR);
        let (kind, size, _) = service.backdoor.requests.last().unwrap().clone();
        assert_eq!(kind, vmmdev::REQ_HGCM_CANCEL);
        assert_eq!(size, wire::CANCEL_SIZE);

        // Cancelling an identifier nobody is waiting on is invalid; so is
        // cancelling on a connection that was never opened.
        let msg = request_message(protocol::VBOX_CANCEL, Endpoint(20), 0, 0, 0, 0, 56);
        service.dispatch(&mut transport, &msg);
        assert_eq!(transport.replies.last().unwrap().2, EINVAL);
        let msg = request_message(protocol::VBOX_CANCEL, Endpoint(20), 3, 0, 0, 0, 55);
        service.dispatch(&mut transport, &msg);
        assert_eq!(transport.replies.last().unwrap().2, EINVAL);

        // An unknown message type is refused.
        let msg = request_message(protocol::VBOX_REQUEST_BASE + 9, Endpoint(20), 0, 0, 0, 0, 0);
        service.dispatch(&mut transport, &msg);
        assert_eq!(transport.replies.last().unwrap().2, ENOSYS);

        // A clock notification syncs the time; other sources are dropped.
        let before = service.backdoor.requests.len();
        service.dispatch(&mut transport, &notify(Endpoint::CLOCK));
        assert_eq!(service.backdoor.requests.len(), before + 1);
        assert_eq!(
            service.backdoor.requests.last().unwrap().0,
            vmmdev::REQ_HOST_TIME
        );
        service.dispatch(&mut transport, &notify(Endpoint(44)));
        assert_eq!(service.backdoor.requests.len(), before + 1);
    }
}
