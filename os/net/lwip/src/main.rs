//! net server (lwip) entry: startup chain, then the four-road event loop.
//!
//! Production wiring: kernel traps via `minix-sys`, SEF interception via
//! `minix-sef`, road bookkeeping via `minix-netdriver`. The per-road
//! business work lives in `ProductionHandler` — each method is the landing
//! spot for the matching implementation round (stack attachment, MIB
//! tree, filter device; see 17-stage-net/todo.md).

fn main() {
    // Mount arguments from the rc script (`up lwip -dev /dev/bpf ...`):
    // `-dev` is followed by a filter device path and may repeat. The paths are
    // consumed when the filter device is mounted; this shell just collects them
    // and hands them to the handler for safekeeping.
    let mut dev_paths: Vec<String> = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "-dev" {
            if let Some(path) = args.next() {
                dev_paths.push(path);
            }
        }
    }

    // C `sef_startup`'s birth conversation (`sef.c:125-141`): block on RS
    // until the RS_INIT request arrives, run the fresh hook, and report
    // over the sendrec reply (`sef_init.c:458-466`). The fresh hook's body
    // in this build stays the loop-driven startup chain (`lwip.c:203-263`'s
    // thirteen steps run stage by stage inside `run`), so the birth reply
    // reports OK before the chain runs — the registered shape of
    // E-NETSTART's adoption slice; a chain failure surfaces through the
    // loop's own exit instead of the birth reply.
    let mut sef_ipc = SefAdapter;
    let birth = minix_sef::sef_await_init(
        &mut sef_ipc,
        &mut |req, addr, len| {
            let r = minix_sys::syscall::sys_statectl(
                &minix_sys::syscall::DirectKernelCallTransport,
                req,
                addr,
                len,
            );
            if r < 0 { Err(-r) } else { Ok(r) }
        },
        &mut |t| match t {
            0 | 2 => 0, // SEF_INIT_FRESH / SEF_INIT_RESTART — sef.h:93-95
            _ => minix_types::ENOSYS,
        },
    );
    // C panics when the birth conversation fails ("unable to ipc_receive
    // from RS", sef.c:132) — the same fail-fast here.
    if let Err(e) = birth {
        panic!("lwip: unable to complete the SEF birth conversation: {e}");
    }

    let mut ipc = minix_net_lwip::server::KernelIpc {
        transport: minix_sys::ipc::DirectTrapTransport,
    };
    let mut handler = ProductionHandler::new(&dev_paths);
    let mut table = minix_netdriver::socktable::SockTable::new();
    let mut startup = minix_net_lwip::startup::Startup::new();
    // A terminal error (persistent transport corruption or a failed startup
    // chain) has only one way out here: exit. C's counterpart is a panic
    // (lwip.c:325) — not an observable behavior difference; logging is runtime
    // diagnostics.
    //[SD19-gate] The main entry exits after run; nothing consumes the Result.
    #[allow(clippy::let_underscore_must_use)]
    let _ = minix_net_lwip::server::run(&mut ipc, &mut handler, &mut table, &mut startup);
}

/// Per-road work for the running service. The loop wiring is complete here;
/// each road's implementation body lands with its own round (buffer and stack
/// attachment are still pending, and the RS lifecycle handshake is in place as
/// the `wait_for_init` gate).
struct ProductionHandler {
    /// Filter device paths collected via `-dev` (consumed when the filter is
    /// mounted).
    dev_paths: Vec<String>,
    /// Random seed for the first step of the thirteen-step chain (clock mixed
    /// with the PID, the seeding surface in `lwip.c`).
    seed: u64,
    /// Stage ordinal of the startup chain (`Startup` calls
    /// [`Self::startup_step`] once per stage; seven stages cover C's thirteen
    /// steps: `lwip.c:203-263`).
    stage: usize,
    /// The stack proper (built in step 2, `lwip.c:208-210`; from step 13 on the
    /// timers count as armed — smoltcp folds timer service into `poll`, see
    /// `25-smoltcp-shim.md` §3).
    stack: Option<Box<dyn minix_net_lwip::lwip_port::Stack>>,
    /// Zero point of the monotonic clock: every wall time instant is a count of
    /// milliseconds (C `sys_now`'s time base), converted on the host side here.
    epoch: std::time::Instant,
    /// Copy seam (the transfer verb between user grants and stack buffers).
    copy: Box<dyn minix_net_lwip::sockops::CopyTransport>,
    /// Caller-identity seam (the root gate for RAW socket creation; fail-closed
    /// in production, canned identity injected in tests).
    identity: Box<dyn minix_net_lwip::sockops::IdentitySource>,
    /// Ledger of pending request notes (receive/connect/accept sites, the
    /// objects of the ready scan).
    pending: minix_net_lwip::sockops::PendingTables,
    /// Outgoing replies for pending resumptions (C's `reply` fires at any point
    /// in event handling; this model collects them into a queue and the loop
    /// tail sends them non-blockingly via `take_wake_replies`).
    wake_replies: Vec<(minix_types::Endpoint, minix_types::Message)>,
    /// Multicast membership registry (step 4 `mcast_init`, `mcast.c:55-66`).
    mcast: Option<minix_net_lwip::mcast::McastRegistry>,
    /// Routing table (step 9 `route_init`, `route.c:248`).
    routes: Option<minix_net_lwip::route::RouteTable>,
}

impl ProductionHandler {
    fn new(dev_paths: &[String]) -> Self {
        ProductionHandler {
            dev_paths: dev_paths.to_vec(),
            seed: 0,
            stage: 0,
            stack: None,
            epoch: std::time::Instant::now(),
            copy: Box::new(minix_net_lwip::sockops::SysCopyTransport),
            identity: Box::new(minix_net_lwip::sockops::FailClosedIdentity),
            pending: minix_net_lwip::sockops::PendingTables::default(),
            wake_replies: Vec::new(),
            mcast: None,
            routes: None,
        }
    }

    /// Milliseconds of the current instant (the wall's time shape; see the wall
    /// note in `lwip_port.rs`).
    fn now_millis(&self) -> u64 {
        self.epoch.elapsed().as_millis() as u64
    }
}

impl minix_net_lwip::server::NetHandler for ProductionHandler {
    fn startup_step(&mut self) -> bool {
        // Step-by-step landing points of the thirteen-step chain
        // (`lwip.c:203-263`; the per-step "implemented / waiting for what"
        // table is in 03-lwip-main-init.md §2.2): seven stages cover the
        // thirteen steps; implemented steps do real work and steps whose round
        // has not come are let through in order with their flags left blank —
        // the shape of the chain and the advancement order stay unchanged.
        self.stage += 1;
        match self.stage {
            // C steps 1 and 2 share a stage (`StartupStage::LibraryReady`:
            // seed and stack library ready). Step 1 seeds (clock mixed with the
            // PID, `lwip.c:203-206`); step 2 constructs the stack proper
            // (`lwip_init`, `lwip.c:208-210`) — the seed goes to the stack for
            // its random surface (TCP initial sequence numbers, the counterpart
            // of C's srand48). Steps 3/4 (event library and the three helpers)
            // follow in a later round.
            1 => {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default();
                let pid = minix_sys::pm::getpid_via(&minix_sys::ipc::DirectTrapTransport)
                    .unwrap_or(0) as u32;
                self.seed = now
                    .as_secs()
                    .wrapping_add(now.subsec_micros() as u64)
                    .wrapping_add(pid as u64)
                    .wrapping_mul(0x9E37_79B9_7F4A_7C15);
                // The wall's stack is held behind the `Stack` trait — swapping
                // stack implementations touches only this line.
                let wall: Box<dyn minix_net_lwip::lwip_port::Stack> =
                    Box::new(minix_net_lwip::stack::SmoltcpStack::<
                        minix_net_lwip::stack::TrunkDevice,
                    >::new(
                        self.seed, self.now_millis() as i64
                    ));
                self.stack = Some(wall);
            }
            // C step 13 owns the seventh stage alone
            // (`StartupStage::Running`: arm the timers and set the running
            // flag, `lwip.c:255-263`). On the smoltcp side, arming means "push
            // once before entering the main loop", so do one initial check here
            // to honor the semantics of C's comment (the per-iteration
            // `check_lwip_timer` bit gets wired with the main-loop round).
            7 => {
                let now = self.now_millis();
                if let Some(stack) = self.stack.as_mut() {
                    stack.poll(now);
                }
            }
            // C steps 3 and 4 share a stage (`StartupStage::FrameworkReady`:
            // event library and the three helpers ready). The event library's
            // concrete mechanism lives in `SockTable` (built by the main
            // function and passed into the loop), and the memory-pool library is
            // built there; here we add the multicast membership registry
            // (`mcast_init`, `mcast.c:55-66`). The initial-sequence helper is in
            // the same place inside the stack (the seed was handed to the stack
            // in step 2, see the deviation table in `24-liblwip-port.md` §1.5).
            2 => {
                self.mcast = Some(minix_net_lwip::mcast::McastRegistry::new());
            }
            // C steps 8..10 share a stage (`StartupStage::LowSocketsReady`):
            // the routing table is cleared and set up in `route_init`
            // (`route.c:248`); socket creation for the rtsock/lnksock domains is
            // wired in a later round.
            5 => {
                self.routes = Some(minix_net_lwip::route::RouteTable::new());
            }
            // C steps 6/7 (network interfaces) wait for the 16-stage data path;
            // the remaining steps stay blank.
            _ => {}
        }
        true
    }

    fn keep_running(&mut self) -> bool {
        true
    }

    fn notify_clock(
        &mut self,
        table: &mut minix_netdriver::socktable::SockTable,
        _tick: &minix_types::Message,
    ) {
        // Clock tick checks the main timers (C `expire_timers`, `lwip.c:328`):
        // smoltcp folds "service expired timers" into advancement, and the
        // shim's `poll` does it all in one call (`25-smoltcp-shim.md` §4).
        let now = self.now_millis();
        if let Some(stack) = self.stack.as_mut() {
            stack.poll(now);
        }
        // Pending requests with a deadline expire: answer with a timeout error
        // in the generic reply shape (the timer half of C's `sockevent`; the
        // producers of the `Resume`/`RetestSelect`/`Alarm` wakeups are wired
        // with the family-operation round — none exist yet).
        let timed_out = table.poll_timers(now);
        for action in timed_out {
            if let minix_netdriver::socktable::WakeAction::TimedOut { id, continuation } = action {
                // Drop the note for that site (a timed-out receive never gets
                // its data resumed).
                self.pending.recvs.retain(|note| {
                    !(note.sock_id == id.raw() && note.caller == continuation.caller())
                });
                let reply = minix_net_lwip::sockops::simple_reply(0, -(minix_types::ETIMEDOUT));
                self.wake_replies.push((continuation.caller(), reply));
            }
        }
        // Ready scan: receive notes the stack reports readable are resumed
        // here (the producer half of the Resume wakeup).
        if let Some(stack) = self.stack.as_mut() {
            let completed = minix_net_lwip::sockops::ready_scan(
                stack.as_mut(),
                self.copy.as_mut(),
                table,
                &mut self.pending,
            );
            self.wake_replies.extend(completed);
        }
    }

    fn notify_dev_mgr(&mut self) {}

    fn management(&mut self, _msg: &minix_types::Message) -> Option<minix_types::Message> {
        None
    }

    fn socket_device(
        &mut self,
        table: &mut minix_netdriver::socktable::SockTable,
        msg: &minix_types::Message,
    ) -> Option<minix_types::Message> {
        // Service half of the socket road: currently translates `SDEV_SOCKET`
        // (socket creation; C does not suspend here and answers immediately);
        // the remaining request families cross the wall one by one with their
        // own family rounds and are answered as not wired (ENOSYS) in the
        // generic reply shape — VFS-side continuations map unknown reply
        // numbers to EIO, so the generic shape is never misread as data.
        use minix_net_lwip::sockops;
        let Some(stack) = self.stack.as_mut() else {
            return Some(sockops::simple_reply(0, -(minix_types::ENOSYS)));
        };
        if msg.m_type == minix_sockdriver::sdev::SdevRequest::Socket as i32 {
            let reply = match sockops::decode_socket(msg) {
                Some(req) => {
                    let is_root = self.identity.is_root(minix_types::Endpoint(req.user_endpt));
                    match sockops::open_socket(stack.as_mut(), table, &req, is_root) {
                        Ok(id) => sockops::socket_reply(req.req_id, id),
                        Err(e) => sockops::socket_reply(req.req_id, -e.abs()),
                    }
                }
                None => sockops::simple_reply(0, -(minix_types::EINVAL)),
            };
            return Some(reply);
        }
        // Eight family operations cross the wall (bind/connect/getsockname/
        // send/receive/listen/accept/getpeername); dispatch goes by the class of
        // the on-the-wire socket to the UDP road or the TCP road.
        // SetSockOpt/GetSockOpt have their own shape and use the options road.
        use minix_sockdriver::sdev::SdevRequest;
        // The two server-side classes (RT/LNK) send and receive over their own
        // message roads: RT = the routing table's RTM frame surface (write adds
        // and deletes, read exports), LNK = no data surface (answers EOPNOTSUPP
        // per C's NULL-hook semantics; registered). sockopt/ioctl already answer
        // honestly by server-side class on their respective roads.
        if matches!(
            msg.m_type,
            x if x == SdevRequest::Send as i32 || x == SdevRequest::Receive as i32
        ) {
            // SAFETY: sock_id is at @4 in the sendrecv shape.
            let sock_id = unsafe { i32::from_le_bytes(msg.m_u.raw[4..8].try_into().unwrap()) };
            let class =
                minix_netdriver::sockid::SockId::from_raw(sock_id).and_then(|id| id.class());
            match class {
                Some(minix_netdriver::sockid::SockClass::Rt) => {
                    return sockops::rt_road(
                        self.routes.as_mut(),
                        self.copy.as_mut(),
                        self.identity.as_ref(),
                        msg.m_source,
                        msg,
                    );
                }
                Some(minix_netdriver::sockid::SockClass::Lnk) => {
                    return sockops::lnk_road(msg);
                }
                _ => {}
            }
        }
        if matches!(
            msg.m_type,
            x if x == SdevRequest::Bind as i32
                || x == SdevRequest::Connect as i32
                || x == SdevRequest::GetSockName as i32
                || x == SdevRequest::Send as i32
                || x == SdevRequest::Receive as i32
                || x == SdevRequest::Listen as i32
                || x == SdevRequest::Accept as i32
                || x == SdevRequest::GetPeerName as i32
        ) {
            return sockops::translate(
                stack.as_mut(),
                self.copy.as_mut(),
                table,
                &mut self.pending,
                msg.m_source,
                msg,
            );
        }
        if matches!(
            msg.m_type,
            x if x == SdevRequest::SetSockOpt as i32
                || x == SdevRequest::GetSockOpt as i32
        ) {
            return sockops::sockopt_road(stack.as_mut(), self.copy.as_mut(), msg.m_source, msg);
        }
        if msg.m_type == SdevRequest::Ioctl as i32 {
            return sockops::ioctl_road(stack.as_mut(), self.copy.as_mut(), msg.m_source, msg);
        }
        if msg.m_type == SdevRequest::Close as i32 {
            return sockops::close_socket(stack.as_mut(), table, msg);
        }
        if msg.m_type == SdevRequest::Shutdown as i32 {
            return sockops::shutdown_socket(stack.as_mut(), msg);
        }
        if msg.m_type == SdevRequest::Select as i32 {
            return sockops::select_road(
                stack.as_mut(),
                table,
                &mut self.pending,
                msg.m_source,
                msg,
            );
        }
        if msg.m_type == SdevRequest::Cancel as i32 {
            // C: cancel has no reply — silence leaves the floor to the
            // original request's reply.
            sockops::cancel_road(table, &mut self.pending, msg);
            return None;
        }
        if msg.m_type == SdevRequest::SocketPair as i32 {
            // Paired socket creation follows the rtsock/uds semantics round —
            // answered as not wired for now.
            // SAFETY(test): req_id is in the first cell.
            let req_id = unsafe { i32::from_le_bytes(msg.m_u.raw[0..4].try_into().unwrap()) };
            return Some(sockops::simple_reply(req_id, -(minix_types::ENOSYS)));
        }
        // Remaining sdev requests: generic shape + ENOSYS (req_id is in the
        // first cell in both shapes).
        // SAFETY: { req_id@0 } — both reply families share the first cell.
        let req_id = unsafe { i32::from_le_bytes(msg.m_u.raw[0..4].try_into().unwrap()) };
        Some(sockops::simple_reply(req_id, -(minix_types::ENOSYS)))
    }

    fn bpf_device(&mut self, _msg: &minix_types::Message) -> Option<minix_types::Message> {
        None
    }

    fn net_device_reply(&mut self, _msg: &minix_types::Message) -> Option<minix_types::Message> {
        None
    }

    fn unexpected(&mut self, _msg: &minix_types::Message, _is_notify: bool) {}

    fn take_wake_replies(&mut self) -> Vec<(minix_types::Endpoint, minix_types::Message)> {
        core::mem::take(&mut self.wake_replies)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Calling trait methods requires the trait in scope.
    use minix_net_lwip::server::NetHandler as _;

    /// The seven-step startup chain runs to completion: step 1 seeds, step 2
    /// builds the stack proper, step 13 does the initial check — chain shape and
    /// advancement order are unchanged (landing table in
    /// 03-lwip-main-init.md §2.2).
    #[test]
    fn test_startup_chain_builds_stack_and_polls() {
        let mut handler = ProductionHandler::new(&[]);
        for _ in 0..7 {
            assert!(handler.startup_step(), "启动链七步都应成功");
        }
        assert_ne!(handler.seed, 0, "步 1 已播种");
        assert!(handler.stack.is_some(), "步 2 已构造栈本体");
        // Clock-tick road: advancement after arming does not blow up (no
        // sockets and no timers in the stack, so it idles).
        let mut table = minix_netdriver::socktable::SockTable::new();
        handler.notify_clock(&mut table, &minix_types::Message::default());
    }

    /// End to end on the socket road: SDEV_SOCKET arrives -> a socket is opened
    /// in the stack + a namespace slot is allocated -> SocketReply carries the
    /// socket id; unwired request families answer ENOSYS in the generic shape.
    #[test]
    fn test_socket_road_opens_and_answers_enosys_for_unwired() {
        use minix_net_lwip::server::NetHandler as _;
        let mut handler = ProductionHandler::new(&[]);
        for _ in 0..7 {
            handler.startup_step();
        }
        let mut table = minix_netdriver::socktable::SockTable::new();
        let mut msg = minix_types::Message::default();
        msg.m_type = minix_sockdriver::sdev::SdevRequest::Socket as i32;
        // SAFETY(test): { req_id@0; domain@4; type@8 }(PF_INET=2,
        // SOCK_STREAM=1,sys/sys/socket.h:179).
        unsafe {
            let raw = &mut msg.m_u.raw;
            raw[0..4].copy_from_slice(&77i32.to_le_bytes());
            raw[4..8].copy_from_slice(&2i32.to_le_bytes());
            raw[8..12].copy_from_slice(&1i32.to_le_bytes());
        }
        let reply = handler.socket_device(&mut table, &msg).expect("建户有回复");
        assert_eq!(
            reply.m_type,
            minix_sockdriver::sdev::SdevReply::SocketReply as i32
        );
        // SAFETY(test): { req_id@0; sock_id@4 }.
        let raw = unsafe { &reply.m_u.raw };
        assert_eq!(i32::from_le_bytes(raw[0..4].try_into().unwrap()), 77);
        let sock_id = i32::from_le_bytes(raw[4..8].try_into().unwrap());
        assert!(sock_id >= 0, "建户成功给非负套接字号");
        let id = minix_netdriver::sockid::SockId::from_raw(sock_id).unwrap();
        assert_eq!(
            id.class_base(),
            minix_netdriver::sockid::SOCKID_TCP,
            "流类型归 TCP 类"
        );
        assert!(table.contains(id), "服务表已登记");
        // Unwired request families (SocketPair follows the paired semantics
        // round): generic shape + ENOSYS (an honest refusal, never a fake
        // success). The wired Connect/Listen are no longer used as negative
        // examples — they go through translation and the copy seam / stack
        // errors and each has a real answer; SetSockOpt/GetSockOpt are wired
        // with the options round over their own options road.
        let mut other = minix_types::Message::default();
        other.m_type = minix_sockdriver::sdev::SdevRequest::SocketPair as i32;
        // SAFETY(test): req_id is in the first cell in both shapes.
        unsafe {
            other.m_u.raw[0..4].copy_from_slice(&99i32.to_le_bytes());
        }
        let reply2 = handler.socket_device(&mut table, &other).expect("有回复");
        assert_eq!(
            reply2.m_type,
            minix_sockdriver::sdev::SdevReply::Reply as i32
        );
        // SAFETY(test): status is in the second cell.
        let raw2 = unsafe { &reply2.m_u.raw };
        assert_eq!(
            i32::from_le_bytes(raw2[4..8].try_into().unwrap()),
            -(minix_types::ENOSYS)
        );
    }

    /// UDP data path end to end (loopback device): create socket -> bind ->
    /// sendto -> receive suspends -> clock round advances (frame loops back,
    /// ready scan wakes) -> resumes `SDEV_RECV_REPLY`, with both the data and
    /// the peer address copied into the canned write ledger.
    #[test]
    fn test_udp_sendto_recvfrom_loopback_through_road() {
        use minix_net_lwip::server::NetHandler as _;
        let mut handler = ProductionHandler::new(&[]);
        // Swap in the loopback stack (mounted interface address) and box it:
        // the wall type is unchanged and the test sees the whole chain.
        let mut loop_stack =
            minix_net_lwip::stack::SmoltcpStack::<minix_net_lwip::stack::LoopDevice>::with_device(
                0x5EED,
                minix_net_lwip::stack::LoopDevice::new(),
                0,
            );
        loop_stack.add_address_v4([127, 0, 0, 1], 8, 0);
        handler.stack = Some(Box::new(loop_stack));
        // canned copy: grant 1 = bind address (127.0.0.1:7777), grant 2 = data
        // to send; grants 3/4 are the outbound buffers for the received address
        // and data.
        let mut canned = minix_net_lwip::sockops::CannedCopyTransport::default();
        let mut sa = [0u8; 16];
        sa[0] = 16;
        sa[1] = 2; // AF_INET
        sa[2..4].copy_from_slice(&7777u16.to_be_bytes());
        sa[4..8].copy_from_slice(&[127, 0, 0, 1]);
        canned.from.push((1, sa.to_vec()));
        canned.from.push((2, vec![9, 9, 9, 9]));
        handler.copy = Box::new(canned);

        let mut table = minix_netdriver::socktable::SockTable::new();
        fn road(
            handler: &mut ProductionHandler,
            table: &mut minix_netdriver::socktable::SockTable,
            msg: minix_types::Message,
        ) -> Option<minix_types::Message> {
            handler.socket_device(table, &msg)
        }

        // 1) Create a UDP socket (PF_INET=2, SOCK_DGRAM=2).
        let mut open = minix_types::Message::default();
        open.m_type = minix_sockdriver::sdev::SdevRequest::Socket as i32;
        // SAFETY(test): { req_id@0; domain@4; type@8 }.
        unsafe {
            open.m_u.raw[0..4].copy_from_slice(&5i32.to_le_bytes());
            open.m_u.raw[4..8].copy_from_slice(&2i32.to_le_bytes());
            open.m_u.raw[8..12].copy_from_slice(&2i32.to_le_bytes());
        }
        let reply = road(&mut handler, &mut table, open).expect("建户有回复");
        // SAFETY(test): sock_id@4.
        let sock_id = i32::from_le_bytes(unsafe { &reply.m_u.raw }[4..8].try_into().unwrap());
        assert!(sock_id >= 0);

        // 2) Bind 127.0.0.1:7777.
        let mut bind = minix_types::Message::default();
        bind.m_type = minix_sockdriver::sdev::SdevRequest::Bind as i32;
        // SAFETY(test): { req_id@0; sock_id@4; grant@8; len@12 }.
        unsafe {
            bind.m_u.raw[0..4].copy_from_slice(&6i32.to_le_bytes());
            bind.m_u.raw[4..8].copy_from_slice(&sock_id.to_le_bytes());
            bind.m_u.raw[8..12].copy_from_slice(&1i32.to_le_bytes());
            bind.m_u.raw[12..16].copy_from_slice(&16i32.to_le_bytes());
        }
        let reply = road(&mut handler, &mut table, bind).expect("绑定有回复");
        assert_eq!(
            reply.m_type,
            minix_sockdriver::sdev::SdevReply::Reply as i32
        );
        // SAFETY(test): status is in the second cell.
        assert_eq!(
            i32::from_le_bytes(unsafe { &reply.m_u.raw }[4..8].try_into().unwrap()),
            0,
            "绑定成功"
        );

        // 3) sendto itself on 7777: data grant 2, address grant 1.
        let mut send = minix_types::Message::default();
        send.m_type = minix_sockdriver::sdev::SdevRequest::Send as i32;
        // SAFETY(test): sendrecv field order (ipc.h:2305-2317).
        unsafe {
            send.m_u.raw[0..4].copy_from_slice(&7i32.to_le_bytes());
            send.m_u.raw[4..8].copy_from_slice(&sock_id.to_le_bytes());
            send.m_u.raw[8..12].copy_from_slice(&2i32.to_le_bytes());
            send.m_u.raw[16..24].copy_from_slice(&4usize.to_le_bytes());
            send.m_u.raw[32..36].copy_from_slice(&1i32.to_le_bytes());
            send.m_u.raw[36..40].copy_from_slice(&16i32.to_le_bytes());
            send.m_u.raw[40..44].copy_from_slice(&100i32.to_le_bytes());
        }
        let reply = road(&mut handler, &mut table, send).expect("发送有回复");
        // SAFETY(test): status is in the second cell (number of bytes sent).
        assert_eq!(
            i32::from_le_bytes(unsafe { &reply.m_u.raw }[4..8].try_into().unwrap()),
            4
        );

        // 4) recvfrom: no packet -> suspend (no reply, note recorded).
        let mut recv = minix_types::Message::default();
        recv.m_type = minix_sockdriver::sdev::SdevRequest::Receive as i32;
        // SAFETY(test): sendrecv field order; flags@60 = 0 (blocking semantics).
        unsafe {
            recv.m_u.raw[0..4].copy_from_slice(&8i32.to_le_bytes());
            recv.m_u.raw[4..8].copy_from_slice(&sock_id.to_le_bytes());
            recv.m_u.raw[8..12].copy_from_slice(&4i32.to_le_bytes());
            recv.m_u.raw[16..24].copy_from_slice(&64usize.to_le_bytes());
            recv.m_u.raw[32..36].copy_from_slice(&3i32.to_le_bytes());
            recv.m_u.raw[36..40].copy_from_slice(&16i32.to_le_bytes());
            recv.m_u.raw[40..44].copy_from_slice(&100i32.to_le_bytes());
        }
        let caller = minix_types::Endpoint::from_generation_slot(1, 0);
        let mut recv_msg = recv;
        recv_msg.m_source = caller;
        assert!(
            road(&mut handler, &mut table, recv_msg).is_none(),
            "空收挂起：不回复"
        );
        assert_eq!(handler.pending.recvs.len(), 1, "留言条在账");

        // 5) Clock round: the frame loops back + the ready scan resumes (pull
        // the epoch forward to make time advance).
        for _ in 0..10 {
            handler.epoch = handler.epoch - std::time::Duration::from_millis(200);
            handler.notify_clock(&mut table, &minix_types::Message::default());
        }
        assert!(
            handler.pending.recvs.is_empty(),
            "留言条已被 ready-scan 消化"
        );
        let replies = handler.take_wake_replies();
        assert_eq!(replies.len(), 1, "挂起的 receive 得到续答");
        assert_eq!(replies[0].0, caller);
        assert_eq!(
            replies[0].1.m_type,
            minix_sockdriver::sdev::SdevReply::ReceiveReply as i32
        );
        // SAFETY(test): { req_id@0; status@4; ctl_len@8; addr_len@12 }.
        let raw = unsafe { &replies[0].1.m_u.raw };
        assert_eq!(
            i32::from_le_bytes(raw[0..4].try_into().unwrap()),
            8,
            "req_id 原样带回"
        );
        assert_eq!(
            i32::from_le_bytes(raw[4..8].try_into().unwrap()),
            4,
            "收到 4 字节"
        );
        assert_eq!(
            u32::from_le_bytes(raw[12..16].try_into().unwrap()),
            16,
            "地址 16 字节"
        );
    }

    /// TCP peer setup helper: the server creates+binds+listens, the client
    /// creates+connects, then accept — the clock round drives the handshake and
    /// both resumptions, returning `(server id, client id, accepted id)`.
    fn setup_tcp_pair(
        handler: &mut ProductionHandler,
        table: &mut minix_netdriver::socktable::SockTable,
    ) -> (i32, i32, i32) {
        use minix_net_lwip::server::NetHandler as _;
        fn road(
            handler: &mut ProductionHandler,
            table: &mut minix_netdriver::socktable::SockTable,
            msg: minix_types::Message,
        ) -> Option<minix_types::Message> {
            handler.socket_device(table, &msg)
        }
        let server_ep = minix_types::Endpoint::from_generation_slot(1, 0);
        let client_ep = minix_types::Endpoint::from_generation_slot(1, 1);
        let mut sa = [0u8; 16];
        sa[0] = 16;
        sa[1] = 2;
        sa[2..4].copy_from_slice(&7777u16.to_be_bytes());
        sa[4..8].copy_from_slice(&[127, 0, 0, 1]);
        let open_tcp = |req_id: i32, source: minix_types::Endpoint| {
            let mut m = minix_types::Message::default();
            m.m_type = minix_sockdriver::sdev::SdevRequest::Socket as i32;
            m.m_source = source;
            // SAFETY(test): { req_id@0; domain@4; type@8 }.
            unsafe {
                m.m_u.raw[0..4].copy_from_slice(&req_id.to_le_bytes());
                m.m_u.raw[4..8].copy_from_slice(&2i32.to_le_bytes());
                m.m_u.raw[8..12].copy_from_slice(&1i32.to_le_bytes());
            }
            m
        };
        let reply = road(handler, table, open_tcp(1, server_ep)).expect("服务端建户");
        let server_id = i32::from_le_bytes(unsafe { &reply.m_u.raw }[4..8].try_into().unwrap());
        let mut bind = minix_types::Message::default();
        bind.m_type = minix_sockdriver::sdev::SdevRequest::Bind as i32;
        bind.m_source = server_ep;
        // SAFETY(test): addr shape.
        unsafe {
            bind.m_u.raw[0..4].copy_from_slice(&2i32.to_le_bytes());
            bind.m_u.raw[4..8].copy_from_slice(&server_id.to_le_bytes());
            bind.m_u.raw[8..12].copy_from_slice(&1i32.to_le_bytes());
            bind.m_u.raw[12..16].copy_from_slice(&16i32.to_le_bytes());
        }
        road(handler, table, bind);
        let mut listen = minix_types::Message::default();
        listen.m_type = minix_sockdriver::sdev::SdevRequest::Listen as i32;
        listen.m_source = server_ep;
        // SAFETY(test): simple shape.
        unsafe {
            listen.m_u.raw[0..4].copy_from_slice(&3i32.to_le_bytes());
            listen.m_u.raw[4..8].copy_from_slice(&server_id.to_le_bytes());
            listen.m_u.raw[8..12].copy_from_slice(&1i32.to_le_bytes());
        }
        road(handler, table, listen);
        let reply = road(handler, table, open_tcp(4, client_ep)).expect("客户端建户");
        let client_id = i32::from_le_bytes(unsafe { &reply.m_u.raw }[4..8].try_into().unwrap());
        let mut connect = minix_types::Message::default();
        connect.m_type = minix_sockdriver::sdev::SdevRequest::Connect as i32;
        connect.m_source = client_ep;
        // SAFETY(test): addr shape.
        unsafe {
            connect.m_u.raw[0..4].copy_from_slice(&5i32.to_le_bytes());
            connect.m_u.raw[4..8].copy_from_slice(&client_id.to_le_bytes());
            connect.m_u.raw[8..12].copy_from_slice(&2i32.to_le_bytes());
            connect.m_u.raw[12..16].copy_from_slice(&16i32.to_le_bytes());
        }
        assert!(road(handler, table, connect).is_none(), "阻塞连接挂起");
        let mut accept = minix_types::Message::default();
        accept.m_type = minix_sockdriver::sdev::SdevRequest::Accept as i32;
        accept.m_source = server_ep;
        // SAFETY(test): addr shape (grant 3 = outbound buffer for the peer
        // address).
        unsafe {
            accept.m_u.raw[0..4].copy_from_slice(&6i32.to_le_bytes());
            accept.m_u.raw[4..8].copy_from_slice(&server_id.to_le_bytes());
            accept.m_u.raw[8..12].copy_from_slice(&3i32.to_le_bytes());
            accept.m_u.raw[12..16].copy_from_slice(&16i32.to_le_bytes());
            accept.m_u.raw[16..20].copy_from_slice(&server_ep.0.to_le_bytes());
        }
        assert!(road(handler, table, accept).is_none(), "受理挂起");
        for _ in 0..12 {
            handler.epoch = handler.epoch - std::time::Duration::from_millis(200);
            handler.notify_clock(table, &minix_types::Message::default());
        }
        let replies = handler.take_wake_replies();
        assert_eq!(replies.len(), 2, "连接与受纳各一条续答");
        let accept_reply_msg = replies
            .iter()
            .find(|(to, m)| {
                *to == server_ep
                    && m.m_type == minix_sockdriver::sdev::SdevReply::AcceptReply as i32
            })
            .expect("受纳续答");
        let accepted_id = i32::from_le_bytes(
            unsafe { &accept_reply_msg.1.m_u.raw }[4..8]
                .try_into()
                .unwrap(),
        );
        (server_id, client_id, accepted_id)
    }

    /// TCP control plane end to end (loopback device, everything over the
    /// roads): the server creates+binds+listens and the client creates+connects
    /// (suspends), the server accepts (suspends) — the clock round drives the
    /// handshake and the ready scan; connect resumes with 0 and accept resumes
    /// with the new socket id plus the peer address.
    #[test]
    fn test_tcp_connect_accept_loopback_through_road() {
        use minix_net_lwip::server::NetHandler as _;
        let mut handler = ProductionHandler::new(&[]);
        let mut loop_stack =
            minix_net_lwip::stack::SmoltcpStack::<minix_net_lwip::stack::LoopDevice>::with_device(
                0x5EED,
                minix_net_lwip::stack::LoopDevice::new(),
                0,
            );
        loop_stack.add_address_v4([127, 0, 0, 1], 8, 0);
        handler.stack = Some(Box::new(loop_stack));
        let mut canned = minix_net_lwip::sockops::CannedCopyTransport::default();
        let mut sa = [0u8; 16];
        sa[0] = 16;
        sa[1] = 2; // AF_INET
        sa[2..4].copy_from_slice(&7777u16.to_be_bytes());
        sa[4..8].copy_from_slice(&[127, 0, 0, 1]);
        canned.from.push((1, sa.to_vec()));
        canned.from.push((2, sa.to_vec()));
        handler.copy = Box::new(canned);

        let mut table = minix_netdriver::socktable::SockTable::new();
        fn road(
            handler: &mut ProductionHandler,
            table: &mut minix_netdriver::socktable::SockTable,
            msg: minix_types::Message,
        ) -> Option<minix_types::Message> {
            handler.socket_device(table, &msg)
        }
        let server_ep = minix_types::Endpoint::from_generation_slot(1, 0);
        let client_ep = minix_types::Endpoint::from_generation_slot(1, 1);

        // Socket-creation helper: PF_INET + SOCK_STREAM (req_id distinguishes
        // the two ends).
        let open_tcp = |req_id: i32| {
            let mut m = minix_types::Message::default();
            m.m_type = minix_sockdriver::sdev::SdevRequest::Socket as i32;
            m.m_source = minix_types::Endpoint::from_generation_slot(1, 0);
            // SAFETY(test): { req_id@0; domain@4; type@8 }.
            unsafe {
                m.m_u.raw[0..4].copy_from_slice(&req_id.to_le_bytes());
                m.m_u.raw[4..8].copy_from_slice(&2i32.to_le_bytes());
                m.m_u.raw[8..12].copy_from_slice(&1i32.to_le_bytes());
            }
            m
        };

        // Server: create socket -> bind -> listen.
        let reply = road(&mut handler, &mut table, open_tcp(1)).expect("服务端建户有回复");
        let server_id = i32::from_le_bytes(unsafe { &reply.m_u.raw }[4..8].try_into().unwrap());
        assert!(server_id >= 0);
        let mut bind = minix_types::Message::default();
        bind.m_type = minix_sockdriver::sdev::SdevRequest::Bind as i32;
        bind.m_source = server_ep;
        // SAFETY(test): addr shape { req_id@0; sock_id@4; grant@8; len@12 }.
        unsafe {
            bind.m_u.raw[0..4].copy_from_slice(&2i32.to_le_bytes());
            bind.m_u.raw[4..8].copy_from_slice(&server_id.to_le_bytes());
            bind.m_u.raw[8..12].copy_from_slice(&1i32.to_le_bytes());
            bind.m_u.raw[12..16].copy_from_slice(&16i32.to_le_bytes());
        }
        let reply = road(&mut handler, &mut table, bind).expect("绑定有回复");
        assert_eq!(
            i32::from_le_bytes(unsafe { &reply.m_u.raw }[4..8].try_into().unwrap()),
            0,
            "绑定成功"
        );
        let mut listen = minix_types::Message::default();
        listen.m_type = minix_sockdriver::sdev::SdevRequest::Listen as i32;
        listen.m_source = server_ep;
        // SAFETY(test): simple shape { req_id@0; sock_id@4; param@8 }.
        unsafe {
            listen.m_u.raw[0..4].copy_from_slice(&3i32.to_le_bytes());
            listen.m_u.raw[4..8].copy_from_slice(&server_id.to_le_bytes());
            listen.m_u.raw[8..12].copy_from_slice(&1i32.to_le_bytes());
        }
        let reply = road(&mut handler, &mut table, listen).expect("监听有回复");
        assert_eq!(
            i32::from_le_bytes(unsafe { &reply.m_u.raw }[4..8].try_into().unwrap()),
            0
        );

        // Client: create socket -> connect (blocking semantics -> suspend).
        let mut open2 = open_tcp(4);
        open2.m_source = client_ep;
        let reply = road(&mut handler, &mut table, open2).expect("客户端建户有回复");
        let client_id = i32::from_le_bytes(unsafe { &reply.m_u.raw }[4..8].try_into().unwrap());
        assert!(client_id >= 0);
        let mut connect = minix_types::Message::default();
        connect.m_type = minix_sockdriver::sdev::SdevRequest::Connect as i32;
        connect.m_source = client_ep;
        // SAFETY(test): addr shape.
        unsafe {
            connect.m_u.raw[0..4].copy_from_slice(&5i32.to_le_bytes());
            connect.m_u.raw[4..8].copy_from_slice(&client_id.to_le_bytes());
            connect.m_u.raw[8..12].copy_from_slice(&2i32.to_le_bytes());
            connect.m_u.raw[12..16].copy_from_slice(&16i32.to_le_bytes());
        }
        assert!(
            road(&mut handler, &mut table, connect).is_none(),
            "阻塞 connect 挂起：不回复"
        );
        assert_eq!(handler.pending.connects.len(), 1, "连接留言条在账");

        // Server accept: no connection yet -> suspend; with a client SYN in
        // flight it waits for the ready scan.
        let mut accept = minix_types::Message::default();
        accept.m_type = minix_sockdriver::sdev::SdevRequest::Accept as i32;
        accept.m_source = server_ep;
        // SAFETY(test): addr shape (grant 3 = outbound buffer for the peer
        // address).
        unsafe {
            accept.m_u.raw[0..4].copy_from_slice(&6i32.to_le_bytes());
            accept.m_u.raw[4..8].copy_from_slice(&server_id.to_le_bytes());
            accept.m_u.raw[8..12].copy_from_slice(&3i32.to_le_bytes());
            accept.m_u.raw[12..16].copy_from_slice(&16i32.to_le_bytes());
            accept.m_u.raw[16..20].copy_from_slice(&server_ep.0.to_le_bytes());
        }
        assert!(
            road(&mut handler, &mut table, accept).is_none(),
            "受理挂起：等握手完成"
        );
        assert_eq!(handler.pending.accepts.len(), 1);

        // Clock round: the handshake advances + the ready scan sends both
        // resumptions.
        for _ in 0..12 {
            handler.epoch = handler.epoch - std::time::Duration::from_millis(200);
            handler.notify_clock(&mut table, &minix_types::Message::default());
        }
        assert!(handler.pending.connects.is_empty(), "连接留言条已消化");
        assert!(handler.pending.accepts.is_empty(), "受纳留言条已消化");
        let replies = handler.take_wake_replies();
        assert_eq!(replies.len(), 2, "连接与受纳各一条续答");
        // Connect resumption: back to the client, generic shape with status 0.
        let connect_reply = replies
            .iter()
            .find(|(to, m)| {
                *to == client_ep && m.m_type == minix_sockdriver::sdev::SdevReply::Reply as i32
            })
            .expect("连接续答回客户端");
        assert_eq!(
            i32::from_le_bytes(
                unsafe { &connect_reply.1.m_u.raw }[4..8]
                    .try_into()
                    .unwrap()
            ),
            0,
            "建连完成"
        );
        // Accept resumption: back to the server, AcceptReply carries the new
        // socket id and status 0.
        let accept_reply_msg = replies
            .iter()
            .find(|(to, m)| {
                *to == server_ep
                    && m.m_type == minix_sockdriver::sdev::SdevReply::AcceptReply as i32
            })
            .expect("受纳续答回服务端");
        // SAFETY(test): { req_id@0; sock_id@4; status@8; len@12 }.
        let raw = unsafe { &accept_reply_msg.1.m_u.raw };
        assert_eq!(
            i32::from_le_bytes(raw[0..4].try_into().unwrap()),
            6,
            "req_id 原样"
        );
        let new_sock_id = i32::from_le_bytes(raw[4..8].try_into().unwrap());
        assert!(new_sock_id >= 0, "受纳给新套接字号");
        assert_eq!(i32::from_le_bytes(raw[8..12].try_into().unwrap()), 0);
        // The new socket is registered in the service table (TCP class).
        let id = minix_netdriver::sockid::SockId::from_raw(new_sock_id).unwrap();
        assert_eq!(id.class_base(), minix_netdriver::sockid::SOCKID_TCP);
        assert!(table.contains(id), "受纳套接字已登记");
        // The peer address was copied into the server's address buffer (canned
        // write ledger grant 3, assertable via canned.written — this test uses
        // len=16 in the accept reply as the criterion).
    }

    /// Request-surface wrap-up: the root gate for RAW socket creation (identity
    /// seam), server-side socket creation for the RT/LNK domains, and the CLOSE
    /// closure across all classes (closing the stack socket and removing it from
    /// the service table, with slot reuse).
    #[test]
    fn test_request_surface_completion() {
        use minix_net_lwip::server::NetHandler as _;
        let mut handler = ProductionHandler::new(&[]);
        // Seven startup steps: step 2 builds the stack proper (creation and
        // closing both go through the stack).
        for _ in 0..7 {
            handler.startup_step();
        }
        let mut table = minix_netdriver::socktable::SockTable::new();

        // RAW: non-root EACCES (fail-closed identity seam); root is let through
        // and gets a RAW class id.
        let raw_open = |handler: &mut ProductionHandler,
                        table: &mut minix_netdriver::socktable::SockTable| {
            let mut m = minix_types::Message::default();
            m.m_type = minix_sockdriver::sdev::SdevRequest::Socket as i32;
            // SAFETY(test): { req_id@0; domain@4; type@8 }.
            unsafe {
                m.m_u.raw[0..4].copy_from_slice(&11i32.to_le_bytes());
                m.m_u.raw[4..8].copy_from_slice(&2i32.to_le_bytes());
                m.m_u.raw[8..12].copy_from_slice(&3i32.to_le_bytes());
            }
            handler.socket_device(table, &m).expect("建户有回复")
        };
        handler.identity = Box::new(minix_net_lwip::sockops::CannedIdentity(false));
        let reply = raw_open(&mut handler, &mut table);
        assert_eq!(
            i32::from_le_bytes(unsafe { &reply.m_u.raw }[4..8].try_into().unwrap()),
            -(minix_types::EACCES),
            "非根 RAW 拒绝"
        );
        handler.identity = Box::new(minix_net_lwip::sockops::CannedIdentity(true));
        let reply = raw_open(&mut handler, &mut table);
        let raw_id = i32::from_le_bytes(unsafe { &reply.m_u.raw }[4..8].try_into().unwrap());
        assert!(raw_id >= 0);
        assert_eq!(
            minix_netdriver::sockid::SockId::from_raw(raw_id)
                .unwrap()
                .class_base(),
            minix_netdriver::sockid::SOCKID_RAW,
            "根 RAW 放行得 RAW 类号"
        );

        // RT/LNK: server-side socket creation (does not enter the stack), each
        // class id in its own place.
        for (req_id, pf, base) in [
            (12i32, 34i32, minix_netdriver::sockid::SOCKID_RT),
            (13, 18, minix_netdriver::sockid::SOCKID_LNK),
        ] {
            let mut m = minix_types::Message::default();
            m.m_type = minix_sockdriver::sdev::SdevRequest::Socket as i32;
            // SAFETY(test): { req_id@0; domain@4; type@8 }.
            unsafe {
                m.m_u.raw[0..4].copy_from_slice(&req_id.to_le_bytes());
                m.m_u.raw[4..8].copy_from_slice(&pf.to_le_bytes());
                m.m_u.raw[8..12].copy_from_slice(&2i32.to_le_bytes());
            }
            let reply = handler
                .socket_device(&mut table, &m)
                .expect("服务侧建户有回复");
            let id = i32::from_le_bytes(unsafe { &reply.m_u.raw }[4..8].try_into().unwrap());
            assert!(
                id >= 0,
                "PF_ROUTE/PF_LINK 服务侧建户放行（rtsock/lnksock 的消息面随后批）"
            );
            assert_eq!(
                minix_netdriver::sockid::SockId::from_raw(id)
                    .unwrap()
                    .class_base(),
                base
            );
        }

        // CLOSE: create a TCP socket then close it — the table entry is removed
        // and the stack slot is reused (reopening gets the same index).
        let mut open = minix_types::Message::default();
        open.m_type = minix_sockdriver::sdev::SdevRequest::Socket as i32;
        // SAFETY(test): { req_id@0; domain@4; type@8 }.
        unsafe {
            open.m_u.raw[0..4].copy_from_slice(&14i32.to_le_bytes());
            open.m_u.raw[4..8].copy_from_slice(&2i32.to_le_bytes());
            open.m_u.raw[8..12].copy_from_slice(&1i32.to_le_bytes());
        }
        let reply = handler
            .socket_device(&mut table, &open)
            .expect("TCP 建户有回复");
        let tcp_id = i32::from_le_bytes(unsafe { &reply.m_u.raw }[4..8].try_into().unwrap());
        assert!(tcp_id >= 0);
        let mut close = minix_types::Message::default();
        close.m_type = minix_sockdriver::sdev::SdevRequest::Close as i32;
        // SAFETY(test): simple shape { req_id@0; sock_id@4 }.
        unsafe {
            close.m_u.raw[0..4].copy_from_slice(&15i32.to_le_bytes());
            close.m_u.raw[4..8].copy_from_slice(&tcp_id.to_le_bytes());
        }
        let reply = handler
            .socket_device(&mut table, &close)
            .expect("关闭有回复");
        assert_eq!(
            i32::from_le_bytes(unsafe { &reply.m_u.raw }[4..8].try_into().unwrap()),
            0
        );
        assert!(
            !table.contains(minix_netdriver::sockid::SockId::from_raw(tcp_id).unwrap()),
            "关闭后服务表摘除"
        );
        let reply = handler
            .socket_device(&mut table, &open)
            .expect("重开有回复");
        let reopened = i32::from_le_bytes(unsafe { &reply.m_u.raw }[4..8].try_into().unwrap());
        assert_eq!(reopened, tcp_id, "槽位回收复用");
    }

    /// TCP stream data path end to end: on the accepted connection the client
    /// sends (bytes entering the stack are reported back) -> the server receives
    /// (suspends, no data yet) -> the clock round advances (data crosses the
    /// wire, ready scan wakes) -> the resumption reports the byte count and the
    /// data goes into the canned write ledger.
    #[test]
    fn test_tcp_stream_send_recv_loopback_through_road() {
        use minix_net_lwip::server::NetHandler as _;
        let mut handler = ProductionHandler::new(&[]);
        let mut loop_stack =
            minix_net_lwip::stack::SmoltcpStack::<minix_net_lwip::stack::LoopDevice>::with_device(
                0x5EED,
                minix_net_lwip::stack::LoopDevice::new(),
                0,
            );
        loop_stack.add_address_v4([127, 0, 0, 1], 8, 0);
        handler.stack = Some(Box::new(loop_stack));
        let mut canned = minix_net_lwip::sockops::CannedCopyTransport::default();
        let mut sa = [0u8; 16];
        sa[0] = 16;
        sa[1] = 2;
        sa[2..4].copy_from_slice(&7777u16.to_be_bytes());
        sa[4..8].copy_from_slice(&[127, 0, 0, 1]);
        canned.from.push((1, sa.to_vec()));
        canned.from.push((2, sa.to_vec()));
        // grant 5 = data the client sends; grant 6 = outbound buffer for the
        // data the server receives.
        canned.from.push((5, vec![1, 2, 3, 4]));
        handler.copy = Box::new(canned);

        let mut table = minix_netdriver::socktable::SockTable::new();
        let (server_id, client_id, accepted_id) = setup_tcp_pair(&mut handler, &mut table);
        assert!(accepted_id >= 0);

        // Client send: data grant 5, 4 bytes (no addr, stream semantics).
        let mut send = minix_types::Message::default();
        send.m_type = minix_sockdriver::sdev::SdevRequest::Send as i32;
        send.m_source = minix_types::Endpoint::from_generation_slot(1, 1);
        // SAFETY(test): sendrecv field order (data@8/16, flags@44).
        unsafe {
            send.m_u.raw[0..4].copy_from_slice(&7i32.to_le_bytes());
            send.m_u.raw[4..8].copy_from_slice(&client_id.to_le_bytes());
            send.m_u.raw[8..12].copy_from_slice(&5i32.to_le_bytes());
            send.m_u.raw[16..24].copy_from_slice(&4usize.to_le_bytes());
        }
        let reply = handler
            .socket_device(&mut table, &send)
            .expect("发送有回复");
        assert_eq!(
            i32::from_le_bytes(unsafe { &reply.m_u.raw }[4..8].try_into().unwrap()),
            4,
            "发送即回实际入栈字节数"
        );

        // Server receives on the accepted socket: the data has not crossed the
        // wire yet -> suspend.
        let mut recv = minix_types::Message::default();
        recv.m_type = minix_sockdriver::sdev::SdevRequest::Receive as i32;
        recv.m_source = minix_types::Endpoint::from_generation_slot(1, 0);
        // SAFETY(test): sendrecv field order (data@8/16).
        unsafe {
            recv.m_u.raw[0..4].copy_from_slice(&8i32.to_le_bytes());
            recv.m_u.raw[4..8].copy_from_slice(&accepted_id.to_le_bytes());
            recv.m_u.raw[8..12].copy_from_slice(&6i32.to_le_bytes());
            recv.m_u.raw[16..24].copy_from_slice(&64usize.to_le_bytes());
        }
        assert!(
            handler.socket_device(&mut table, &recv).is_none(),
            "空收挂起：不回复"
        );
        assert_eq!(handler.pending.tcp_recvs.len(), 1, "TCP 接收留言条在账");

        // Clock round: data crosses the wire + the ready scan wakes the
        // resumption.
        for _ in 0..10 {
            handler.epoch = handler.epoch - std::time::Duration::from_millis(200);
            handler.notify_clock(&mut table, &minix_types::Message::default());
        }
        assert!(handler.pending.tcp_recvs.is_empty(), "留言条已消化");
        let replies = handler.take_wake_replies();
        assert_eq!(replies.len(), 1, "挂起的 receive 得到续答");
        assert_eq!(
            replies[0].1.m_type,
            minix_sockdriver::sdev::SdevReply::ReceiveReply as i32
        );
        // SAFETY(test): { req_id@0; status@4 }.
        let raw = unsafe { &replies[0].1.m_u.raw };
        assert_eq!(
            i32::from_le_bytes(raw[0..4].try_into().unwrap()),
            8,
            "req_id 原样"
        );
        assert_eq!(
            i32::from_le_bytes(raw[4..8].try_into().unwrap()),
            4,
            "收到 4 字节"
        );
    }

    /// Options road across the wall, e2e: create a TCP socket ->
    /// SetSockOpt(SO_KEEPALIVE) answers 0 -> GetSockOpt(SO_KEEPALIVE) answers
    /// length 4 with the value copied into the canned write ledger.
    #[test]
    fn test_sockopt_road_roundtrip_through_socket_device() {
        use minix_net_lwip::server::NetHandler as _;
        let mut handler = ProductionHandler::new(&[]);
        for _ in 0..7 {
            handler.startup_step();
        }
        let mut table = minix_netdriver::socktable::SockTable::new();
        let mut canned = minix_net_lwip::sockops::CannedCopyTransport::default();
        canned.from.push((1, 1i32.to_le_bytes().to_vec()));
        handler.copy = Box::new(canned);

        // Create a TCP socket.
        let mut open = minix_types::Message::default();
        open.m_type = minix_sockdriver::sdev::SdevRequest::Socket as i32;
        // SAFETY(test): { req_id@0; domain@4; type@8 }.
        unsafe {
            open.m_u.raw[0..4].copy_from_slice(&1i32.to_le_bytes());
            open.m_u.raw[4..8].copy_from_slice(&2i32.to_le_bytes());
            open.m_u.raw[8..12].copy_from_slice(&1i32.to_le_bytes());
        }
        let reply = handler.socket_device(&mut table, &open).expect("建户");
        let sock_id = i32::from_le_bytes(unsafe { &reply.m_u.raw }[4..8].try_into().unwrap());

        // SetSockOpt(SOL_SOCKET, SO_KEEPALIVE, 1): success answers 0.
        let mut set = minix_types::Message::default();
        set.m_type = minix_sockdriver::sdev::SdevRequest::SetSockOpt as i32;
        // SAFETY(test): { req_id@0; sock_id@4; level@8; name@12; grant@16; len@20 }.
        unsafe {
            let raw = &mut set.m_u.raw;
            raw[0..4].copy_from_slice(&2i32.to_le_bytes());
            raw[4..8].copy_from_slice(&sock_id.to_le_bytes());
            raw[8..12].copy_from_slice(&0xffffi32.to_le_bytes());
            raw[12..16].copy_from_slice(&0x0008i32.to_le_bytes());
            raw[16..20].copy_from_slice(&1i32.to_le_bytes());
            raw[20..24].copy_from_slice(&4i32.to_le_bytes());
        }
        let reply = handler.socket_device(&mut table, &set).expect("设置有回复");
        assert_eq!(
            i32::from_le_bytes(unsafe { &reply.m_u.raw }[4..8].try_into().unwrap()),
            0,
            "保活使能成功"
        );

        // GetSockOpt: reports a copied-out length of 4; the value 1 is in the
        // canned write ledger.
        let mut get = minix_types::Message::default();
        get.m_type = minix_sockdriver::sdev::SdevRequest::GetSockOpt as i32;
        // SAFETY(test): getset field order (grant 2 = outbound buffer).
        unsafe {
            let raw = &mut get.m_u.raw;
            raw[0..4].copy_from_slice(&3i32.to_le_bytes());
            raw[4..8].copy_from_slice(&sock_id.to_le_bytes());
            raw[8..12].copy_from_slice(&0xffffi32.to_le_bytes());
            raw[12..16].copy_from_slice(&0x0008i32.to_le_bytes());
            raw[16..20].copy_from_slice(&2i32.to_le_bytes());
            raw[20..24].copy_from_slice(&4i32.to_le_bytes());
        }
        let reply = handler.socket_device(&mut table, &get).expect("查询有回复");
        assert_eq!(
            i32::from_le_bytes(unsafe { &reply.m_u.raw }[4..8].try_into().unwrap()),
            4,
            "回复成功码 = 拷出长度（值面由 sockops 单元测试钉住）"
        );
    }

    /// ioctl road dispatch, e2e: setting FIONBIO answers 0 (the flag really
    /// lands on the wall), an unknown command answers ENOTTY — both the dispatch
    /// line and the request shapes are on the right road.
    #[test]
    fn test_ioctl_road_dispatched_through_socket_device() {
        use minix_net_lwip::server::NetHandler as _;
        let mut handler = ProductionHandler::new(&[]);
        for _ in 0..7 {
            handler.startup_step();
        }
        let mut table = minix_netdriver::socktable::SockTable::new();
        let mut canned = minix_net_lwip::sockops::CannedCopyTransport::default();
        canned.from.push((1, 1i32.to_le_bytes().to_vec()));
        handler.copy = Box::new(canned);

        // Create a TCP socket.
        let mut open = minix_types::Message::default();
        open.m_type = minix_sockdriver::sdev::SdevRequest::Socket as i32;
        // SAFETY(test): { req_id@0; domain@4; type@8 }.
        unsafe {
            open.m_u.raw[0..4].copy_from_slice(&1i32.to_le_bytes());
            open.m_u.raw[4..8].copy_from_slice(&2i32.to_le_bytes());
            open.m_u.raw[8..12].copy_from_slice(&1i32.to_le_bytes());
        }
        let reply = handler.socket_device(&mut table, &open).expect("建户");
        let sock_id = i32::from_le_bytes(unsafe { &reply.m_u.raw }[4..8].try_into().unwrap());

        // FIONBIO (0x8004667E, _IOW('f',126,int)) set: answers 0.
        let mut ioctl = minix_types::Message::default();
        ioctl.m_type = minix_sockdriver::sdev::SdevRequest::Ioctl as i32;
        // SAFETY(test): { req_id@0; sock_id@4; request@8(8B); grant@16 }.
        unsafe {
            let raw = &mut ioctl.m_u.raw;
            raw[0..4].copy_from_slice(&2i32.to_le_bytes());
            raw[4..8].copy_from_slice(&sock_id.to_le_bytes());
            raw[8..16].copy_from_slice(&0x8004_667Eu64.to_le_bytes());
            raw[16..20].copy_from_slice(&1i32.to_le_bytes());
        }
        let reply = handler
            .socket_device(&mut table, &ioctl)
            .expect("ioctl 有回复");
        assert_eq!(
            i32::from_le_bytes(unsafe { &reply.m_u.raw }[4..8].try_into().unwrap()),
            0,
            "FIONBIO 置位成功"
        );

        // Unknown command on the same socket: ENOTTY.
        let mut bogus = ioctl;
        // SAFETY(test): req_id and request fields.
        unsafe {
            let raw = &mut bogus.m_u.raw;
            raw[0..4].copy_from_slice(&3i32.to_le_bytes());
            raw[8..16].copy_from_slice(&0x2000_7466u64.to_le_bytes());
        }
        let reply = handler.socket_device(&mut table, &bogus).expect("有回复");
        assert_eq!(
            i32::from_le_bytes(unsafe { &reply.m_u.raw }[4..8].try_into().unwrap()),
            -(minix_types::ENOTTY)
        );
    }

    /// RT domain message surface, e2e (the convergence criterion for the RT
    /// socket work): create a PF_ROUTE socket -> write RTM_ADD into the table
    /// (answers the accepted byte count) -> read exports an RTM_GET frame that
    /// finds the route -> the second of two deletes answers ESRCH. The root gate
    /// (non-root ADD rejects with EPERM) is pinned as well.
    #[test]
    fn test_rtsock_route_message_surface_through_road() {
        use minix_net_lwip::server::NetHandler as _;
        let mut handler = ProductionHandler::new(&[]);
        for _ in 0..7 {
            handler.startup_step();
        }
        // ADD requires root identity: canned lets it through.
        handler.identity = Box::new(minix_net_lwip::sockops::CannedIdentity(true));
        let mut table = minix_netdriver::socktable::SockTable::new();

        // Create a PF_ROUTE socket (34).
        let mut open = minix_types::Message::default();
        open.m_type = minix_sockdriver::sdev::SdevRequest::Socket as i32;
        // SAFETY(test): { req_id@0; domain@4; type@8 }.
        unsafe {
            open.m_u.raw[0..4].copy_from_slice(&1i32.to_le_bytes());
            open.m_u.raw[4..8].copy_from_slice(&34i32.to_le_bytes());
            open.m_u.raw[8..12].copy_from_slice(&2i32.to_le_bytes());
        }
        let reply = handler.socket_device(&mut table, &open).expect("RT 建户");
        let rt_id = i32::from_le_bytes(unsafe { &reply.m_u.raw }[4..8].try_into().unwrap());
        assert!(rt_id >= 0);

        // Write RTM_ADD for 10.1.0.0/16 gw 10.0.0.1 (168-byte frame).
        let mut add = vec![0u8; 168];
        add[0..2].copy_from_slice(&168u16.to_le_bytes());
        add[2] = 4; // RTM_VERSION
        add[3] = 0x1; // RTM_ADD
        add[4..6].copy_from_slice(&2u16.to_le_bytes()); // ifdev
        add[12..16].copy_from_slice(&0x7i32.to_le_bytes()); // DST|GATEWAY|NETMASK
        add[120] = 16;
        add[121] = 2;
        add[124..128].copy_from_slice(&[10, 1, 0, 0]);
        add[136] = 16;
        add[137] = 2;
        add[140..144].copy_from_slice(&[10, 0, 0, 1]);
        add[152] = 16;
        add[153] = 2;
        add[156..160].copy_from_slice(&[255, 255, 0, 0]);
        let mut canned = minix_net_lwip::sockops::CannedCopyTransport::default();
        canned.from.push((2, add.clone()));
        handler.copy = Box::new(canned);

        let mut send = minix_types::Message::default();
        send.m_type = minix_sockdriver::sdev::SdevRequest::Send as i32;
        // SAFETY(test): sendrecv field order (data_grant@8, data_len@16).
        unsafe {
            let raw = &mut send.m_u.raw;
            raw[0..4].copy_from_slice(&2i32.to_le_bytes());
            raw[4..8].copy_from_slice(&rt_id.to_le_bytes());
            raw[8..12].copy_from_slice(&2i32.to_le_bytes());
            raw[16..24].copy_from_slice(&168usize.to_le_bytes());
        }
        let reply = handler.socket_device(&mut table, &send).expect("写有回复");
        assert_eq!(
            i32::from_le_bytes(unsafe { &reply.m_u.raw }[4..8].try_into().unwrap()),
            168,
            "整帧受纳"
        );

        // read: export an RTM_GET frame that finds the route just added.
        let mut recv = minix_types::Message::default();
        recv.m_type = minix_sockdriver::sdev::SdevRequest::Receive as i32;
        // SAFETY(test): sendrecv field order (data_grant@8, data_len@16).
        unsafe {
            let raw = &mut recv.m_u.raw;
            raw[0..4].copy_from_slice(&3i32.to_le_bytes());
            raw[4..8].copy_from_slice(&rt_id.to_le_bytes());
            raw[8..12].copy_from_slice(&3i32.to_le_bytes());
            raw[16..24].copy_from_slice(&256usize.to_le_bytes());
        }
        let reply = handler.socket_device(&mut table, &recv).expect("读有回复");
        assert_eq!(
            reply.m_type,
            minix_sockdriver::sdev::SdevReply::ReceiveReply as i32
        );
        let n = i32::from_le_bytes(unsafe { &reply.m_u.raw }[4..8].try_into().unwrap());
        assert_eq!(n, 168, "一帧导出（帧内容由 rtsock 的往返测试钉住）");

        // Non-root RTM_ADD rejection: switch the canned identity and send the
        // same frame -> EPERM.
        handler.identity = Box::new(minix_net_lwip::sockops::CannedIdentity(false));
        add[3] = 0x1;
        let mut canned2 = minix_net_lwip::sockops::CannedCopyTransport::default();
        canned2.from.push((2, add.clone()));
        handler.copy = Box::new(canned2);
        let reply = handler.socket_device(&mut table, &send).expect("有回复");
        assert_eq!(
            i32::from_le_bytes(unsafe { &reply.m_u.raw }[4..8].try_into().unwrap()),
            -(minix_types::EPERM),
            "非根 ADD 拒（rtsock.c:545-550 的根门）"
        );

        // Delete: restore root identity, write RTM_DELETE (type 0x2) -> success;
        // deleting again answers ESRCH. After the delete, read answers EAGAIN
        // (empty table).
        handler.identity = Box::new(minix_net_lwip::sockops::CannedIdentity(true));
        let mut del = add;
        del[3] = 0x2;
        let mut canned3 = minix_net_lwip::sockops::CannedCopyTransport::default();
        canned3.from.push((2, del.clone()));
        handler.copy = Box::new(canned3);
        let reply = handler.socket_device(&mut table, &send).expect("删有回复");
        assert_eq!(
            i32::from_le_bytes(unsafe { &reply.m_u.raw }[4..8].try_into().unwrap()),
            168
        );
        let mut canned4 = minix_net_lwip::sockops::CannedCopyTransport::default();
        canned4.from.push((2, del));
        handler.copy = Box::new(canned4);
        let reply = handler
            .socket_device(&mut table, &send)
            .expect("再删有回复");
        assert_eq!(
            i32::from_le_bytes(unsafe { &reply.m_u.raw }[4..8].try_into().unwrap()),
            -(minix_types::ESRCH),
            "删不存在的路由回 ESRCH"
        );
        let mut canned5 = minix_net_lwip::sockops::CannedCopyTransport::default();
        handler.copy = Box::new(canned5);
        let reply = handler
            .socket_device(&mut table, &recv)
            .expect("空读有回复");
        assert_eq!(
            i32::from_le_bytes(unsafe { &reply.m_u.raw }[4..8].try_into().unwrap()),
            -(minix_types::EAGAIN),
            "空表诚实立即回（无通告队列，登记差异）"
        );
    }

    /// Two-shape select reply, e2e: select(read) on a UDP socket with no data
    /// suspends -> after the data crosses the wire the ready scan returns the
    /// first shape (readiness bits); after cancel drops the note, silence.
    #[test]
    fn test_select_and_cancel_lifecycle_through_road() {
        use minix_net_lwip::server::NetHandler as _;
        let mut handler = ProductionHandler::new(&[]);
        let mut loop_stack =
            minix_net_lwip::stack::SmoltcpStack::<minix_net_lwip::stack::LoopDevice>::with_device(
                0x5EED,
                minix_net_lwip::stack::LoopDevice::new(),
                0,
            );
        loop_stack.add_address_v4([127, 0, 0, 1], 8, 0);
        handler.stack = Some(Box::new(loop_stack));
        let mut canned = minix_net_lwip::sockops::CannedCopyTransport::default();
        let mut sa = [0u8; 16];
        sa[0] = 16;
        sa[1] = 2;
        sa[2..4].copy_from_slice(&7777u16.to_be_bytes());
        sa[4..8].copy_from_slice(&[127, 0, 0, 1]);
        canned.from.push((1, sa.to_vec()));
        canned.from.push((2, vec![5, 5]));
        handler.copy = Box::new(canned);

        let mut table = minix_netdriver::socktable::SockTable::new();
        let vfs = minix_types::Endpoint::from_generation_slot(1, 0);

        // Create a UDP socket + bind 7777.
        let mut open = minix_types::Message::default();
        open.m_type = minix_sockdriver::sdev::SdevRequest::Socket as i32;
        // SAFETY(test): { req_id@0; domain@4; type@8 }.
        unsafe {
            open.m_u.raw[0..4].copy_from_slice(&1i32.to_le_bytes());
            open.m_u.raw[4..8].copy_from_slice(&2i32.to_le_bytes());
            open.m_u.raw[8..12].copy_from_slice(&2i32.to_le_bytes());
        }
        let reply = handler.socket_device(&mut table, &open).expect("建户");
        let sock_id = i32::from_le_bytes(unsafe { &reply.m_u.raw }[4..8].try_into().unwrap());
        let mut bind = minix_types::Message::default();
        bind.m_type = minix_sockdriver::sdev::SdevRequest::Bind as i32;
        // SAFETY(test): addr shape.
        unsafe {
            bind.m_u.raw[0..4].copy_from_slice(&2i32.to_le_bytes());
            bind.m_u.raw[4..8].copy_from_slice(&sock_id.to_le_bytes());
            bind.m_u.raw[8..12].copy_from_slice(&1i32.to_le_bytes());
            bind.m_u.raw[12..16].copy_from_slice(&16i32.to_le_bytes());
        }
        let _ = handler.socket_device(&mut table, &bind);

        // select(read): no data -> suspend (no reply).
        let mut select = minix_types::Message::default();
        select.m_type = minix_sockdriver::sdev::SdevRequest::Select as i32;
        select.m_source = vfs;
        // SAFETY(test): { sock_id@0; ops@4 }(SDEV_OP_RD=0x01).
        unsafe {
            select.m_u.raw[0..4].copy_from_slice(&sock_id.to_le_bytes());
            select.m_u.raw[4..8].copy_from_slice(&1i32.to_le_bytes());
        }
        assert!(
            handler.socket_device(&mut table, &select).is_none(),
            "未就绪 select 挂起"
        );
        assert_eq!(handler.pending.selects.len(), 1);

        // cancel: drop the note and stay silent (C sdev.c:952, the
        // req_id=who_e semantics).
        let mut cancel = minix_types::Message::default();
        cancel.m_type = minix_sockdriver::sdev::SdevRequest::Cancel as i32;
        // SAFETY(test): simple shape { req_id@0=who_e; sock_id@4 }.
        unsafe {
            cancel.m_u.raw[0..4].copy_from_slice(&vfs.0.to_le_bytes());
            cancel.m_u.raw[4..8].copy_from_slice(&sock_id.to_le_bytes());
        }
        assert!(
            handler.socket_device(&mut table, &cancel).is_none(),
            "cancel 无回复（sdev.c:31-32）"
        );
        assert!(
            handler.pending.selects.is_empty(),
            "cancel 摘除 select 等待"
        );

        // select again -> sendto feeds data -> clock round -> first-shape reply
        // (readiness bit RD).
        assert!(
            handler.socket_device(&mut table, &select).is_none(),
            "重挂 select"
        );
        let mut send = minix_types::Message::default();
        send.m_type = minix_sockdriver::sdev::SdevRequest::Send as i32;
        // SAFETY(test): sendrecv field order (data@8/16, addr@32/36/40).
        unsafe {
            send.m_u.raw[0..4].copy_from_slice(&9i32.to_le_bytes());
            send.m_u.raw[4..8].copy_from_slice(&sock_id.to_le_bytes());
            send.m_u.raw[8..12].copy_from_slice(&2i32.to_le_bytes());
            send.m_u.raw[16..24].copy_from_slice(&2usize.to_le_bytes());
            send.m_u.raw[32..36].copy_from_slice(&1i32.to_le_bytes());
            send.m_u.raw[36..40].copy_from_slice(&16i32.to_le_bytes());
            send.m_u.raw[40..44].copy_from_slice(&vfs.0.to_le_bytes());
        }
        let reply = handler
            .socket_device(&mut table, &send)
            .expect("发送有回复");
        assert_eq!(
            i32::from_le_bytes(unsafe { &reply.m_u.raw }[4..8].try_into().unwrap()),
            2
        );
        for _ in 0..10 {
            handler.epoch = handler.epoch - std::time::Duration::from_millis(200);
            handler.notify_clock(&mut table, &minix_types::Message::default());
        }
        let replies = handler.take_wake_replies();
        assert_eq!(replies.len(), 1, "select 一型续答");
        assert_eq!(replies[0].0, vfs);
        assert_eq!(
            replies[0].1.m_type,
            minix_sockdriver::sdev::SdevReply::SelectReply1 as i32
        );
        // SAFETY(test): { sock_id@0; status@4 } — the readiness bits include
        // read.
        let raw = unsafe { &replies[0].1.m_u.raw };
        assert_eq!(i32::from_le_bytes(raw[0..4].try_into().unwrap()), sock_id);
        assert_eq!(
            i32::from_le_bytes(raw[4..8].try_into().unwrap()) & 1,
            1,
            "就绪位含 SDEV_OP_RD"
        );
    }
}

/// `minix-sef`'s receive-side adapter over the trap transport (the same
/// shape as fs-rt's `SeIpcAdapter`).
struct SefAdapter;

impl minix_sef::SefIpc for SefAdapter {
    fn receive(
        &mut self,
        src: minix_types::Endpoint,
        msg: &mut minix_types::Message,
    ) -> Result<i32, i32> {
        use minix_sys::ipc::IpcTransport;
        minix_sys::ipc::DirectTrapTransport
            .receive(src, msg)
            .map(|status| status.0 as i32)
            .map_err(|t| t.0)
    }

    fn notify(&mut self, dest: minix_types::Endpoint) -> Result<(), i32> {
        use minix_sys::ipc::IpcTransport;
        minix_sys::ipc::DirectTrapTransport
            .notify(dest)
            .map_err(|t| t.0)
    }

    fn sendrec(
        &mut self,
        dest: minix_types::Endpoint,
        msg: &mut minix_types::Message,
    ) -> Result<i32, i32> {
        use minix_sys::ipc::IpcTransport;
        minix_sys::ipc::DirectTrapTransport
            .sendrec(dest, msg)
            .map(|_| 0)
            .map_err(|t| t.0)
    }
}
