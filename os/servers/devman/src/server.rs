//! Server assembly: the lifecycle through one entry (doc 09, binding path).
//!
//! 07/08/09 handlers are injectable free functions (no globals — tests
//! drive them directly). This module assembles them over owned state so
//! the full device lifecycle (ADD → event → BIND → UNBIND → DEL) runs
//! through a single [`Server::run`] — the one dispatch loop for both FS
//! requests and DEVMAN messages (DM-P1-2; C's `fsdriver_task` is the
//! same single entry). Production transport (kernel IPC) is the only
//! missing piece for `main` (P1-6, narrowed to transport wiring).

use alloc::vec::Vec;
use minix_types::{
    Endpoint, Errno, Message, MessageM4, MessageUnion, DEVMAN_BIND, DEVMAN_UNBIND,
};

use crate::add_device::do_add;
use crate::bind::{do_bind, do_unbind, on_bind_response, on_unbind_response, Action};
use crate::del_device::do_del;
use crate::device_tree::{default_file_stat, DeviceTree};
use crate::files::{register_file, EventFile, FileEntry};
use crate::hooks::{FsHooks, ServerConfig};
use crate::ipc::{apply_reply_with_id, result, DevmanMsg};
use crate::structs::{DeviceId, Event};
use crate::vtreefs::{Incoming, Request, Reply, Transport, VTreeFs};
use crate::wire::parse_device;

/// Transport-executable outcome of one message.
/// `Reply` maps to `apply_reply` + async send (05 §2.3); `Forward` maps
/// to `ipc_sendrec(owner)` with the response routed back into
/// `on_bind_response` / `on_unbind_response`; `Nothing` sends nothing
/// (EPERM path, 05 §2.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutAction {
    Reply {
        dest: Endpoint,
        outcome: Result<DeviceId, Errno>,
    },
    Forward {
        owner: Endpoint,
        bind: bool,
        device: DeviceId,
        endpoint: Endpoint,
    },
    Nothing,
}

/// Owned devman server: framework + device tree + events-file cookie.
/// (The file table itself stays in 06's process store; the cookie is
/// the handle — same split as 04's `binding`.)
pub struct Server {
    vtreefs: VTreeFs,
    devices: DeviceTree,
    events_cookie: usize,
}

impl Server {
    /// C: `main` + `run_vtreefs` init half + `devman_init_devices` —
    /// framework, device tree, and the events file registration.
    pub fn new(config: &ServerConfig, hooks: FsHooks) -> Result<Self, Errno> {
        let mut vtreefs = VTreeFs::new(config, hooks)?;
        let devices = DeviceTree::new(vtreefs.tree_mut(), default_file_stat())?;
        let cookie = register_file(FileEntry {
            kind: crate::files::FileKind::Events(EventFile {
                queue: crate::event_queue::EventQueue::new(),
            }),
        })?;
        Ok(Server {
            vtreefs,
            devices,
            events_cookie: cookie,
        })
    }

    fn push_event(&self, ev: Event) {
        let cookie = self.events_cookie;
        crate::files::with_files(|s| {
            let queue = s.get_mut(cookie).and_then(|entry| match &mut entry.kind {
                crate::files::FileKind::Events(f) => Some(&mut f.queue),
                _ => None,
            });
            if let Some(q) = queue {
                let _ = q.push(ev);
            }
        });
    }

    /// One non-filesystem message through its typed decode (05) to its
    /// handler (07/08/09). `msg` arrives pre-classified
    /// ([`DevmanMsg::classify`] marries the routing enum to the payload
    /// words — the phase table's "which word means what" question is
    /// answered before this signature, not inside it); `None` — the
    /// Ignored types, 05 §2.6 — sends nothing.
    pub fn handle_other(&mut self, source: Endpoint, msg: Option<DevmanMsg>) -> Vec<OutAction> {
        let msg = match msg {
            Some(m) => m,
            None => return alloc::vec![OutAction::Nothing],
        };
        match msg {
            DevmanMsg::Add { body } => {
                let parsed = match parse_device(&body) {
                    Ok((_, p)) => p,
                    Err(_) => {
                        return alloc::vec![OutAction::Reply {
                            dest: source,
                            outcome: Err(Errno::EINVAL),
                        }]
                    }
                };
                let parent = parsed.parent;
                let mut sunk = Vec::new();
                let outcome = do_add(
                    &mut self.devices,
                    self.vtreefs.tree_mut(),
                    parent,
                    &parsed,
                    source,
                    &mut |ev| sunk.push(ev),
                );
                for ev in sunk {
                    self.push_event(ev);
                }
                alloc::vec![OutAction::Reply {
                    dest: source,
                    outcome,
                }]
            }
            DevmanMsg::Del { device } => {
                let mut sunk = Vec::new();
                let outcome = do_del(
                    &mut self.devices,
                    self.vtreefs.tree_mut(),
                    device,
                    &mut |ev| sunk.push(ev),
                )
                .map(|_| device);
                for ev in sunk {
                    self.push_event(ev);
                }
                alloc::vec![OutAction::Reply {
                    dest: source,
                    outcome,
                }]
            }
            DevmanMsg::Bind { device, driver } => {
                match do_bind(&self.devices, source, device, driver) {
                    Action::Forward { owner, device, endpoint, .. } => {
                        alloc::vec![OutAction::Forward { owner, bind: true, device, endpoint }]
                    }
                    Action::Reply(outcome) => alloc::vec![OutAction::Reply {
                        dest: source,
                        outcome: outcome.map(|_| device),
                    }],
                    Action::Dropped => alloc::vec![OutAction::Nothing],
                }
            }
            DevmanMsg::Unbind { device, driver } => {
                match do_unbind(&self.devices, source, device, driver) {
                    Action::Forward { owner, device, endpoint, .. } => {
                        alloc::vec![OutAction::Forward { owner, bind: false, device, endpoint }]
                    }
                    Action::Reply(outcome) => alloc::vec![OutAction::Reply {
                        dest: source,
                        outcome: outcome.map(|_| device),
                    }],
                    Action::Dropped => alloc::vec![OutAction::Nothing],
                }
            }
        }
    }

    /// Driver answer to a bind forward (transport routes it here).
    /// C replies to RS with the outcome (bind.c:47-48).
    pub fn answer_bind(
        &mut self,
        device: DeviceId,
        driver: Result<(), Errno>,
    ) -> OutAction {
        let outcome = on_bind_response(&mut self.devices, device, driver);
        OutAction::Reply {
            dest: minix_types::RS_PROC_NR,
            outcome: outcome.map(|_| device),
        }
    }

    /// Driver answer to an unbind forward (bind.c:101-102).
    pub fn answer_unbind(
        &mut self,
        device: DeviceId,
        driver: Result<(), Errno>,
    ) -> OutAction {
        let outcome = on_unbind_response(
            &mut self.devices,
            self.vtreefs.tree_mut(),
            device,
            driver,
        );
        OutAction::Reply {
            dest: minix_types::RS_PROC_NR,
            outcome: outcome.map(|_| device),
        }
    }

    /// Test/support inspection.
    pub fn devices(&self) -> &DeviceTree {
        &self.devices
    }

    /// The server's single dispatch loop (C `fsdriver_task` shape,
    /// table.c:6-24): FS requests and DEVMAN messages through one
    /// [`Transport`], one match, no side doors. This replaces the
    /// rewrite-era pair `VTreeFs::run` + bare `handle_other`, whose
    /// `message_hook` arm silently dropped DEVMAN traffic (DM-P1-2).
    /// The transport is a dumb pipe — this method stamps replies,
    /// builds forwards, and routes driver answers back through the
    /// response halves.
    pub fn run(&mut self, transport: &mut impl Transport) {
        while let Some(incoming) = transport.next() {
            match incoming {
                Incoming::Fs(req) => transport.reply(self.process_fs(req)),
                Incoming::Devman { source, msg } => {
                    self.process_devman(source, msg, transport)
                }
            }
        }
    }

    /// FS half: framework ops with their (error-carrying) replies. The
    /// transport maps `Err` onto the fsdriver reply's status word — no
    /// sentinel bytes, no silent success (the old `Reply` swallowed
    /// mount/lookup/readdir errors as `Ino(0)`/empty lists).
    fn process_fs(&mut self, req: Request) -> Reply {
        match req {
            Request::Mount { is_root } => Reply::Mounted(self.vtreefs.mount(is_root)),
            Request::Unmount => {
                self.vtreefs.unmount();
                Reply::Unmounted
            }
            Request::Lookup { dir, name } => Reply::Found(self.vtreefs.lookup(dir, &name)),
            Request::Read { ino, len, pos } => Reply::Data(self.vtreefs.read(ino, len, pos)),
            Request::Readdir { dir, start } => Reply::Entries(self.vtreefs.readdir(dir, start)),
        }
    }

    /// DEVMAN half: run the handler, then execute its [`OutAction`]s.
    /// Replies are stamped here (`apply_reply_with_id`, 05 §2.3) and
    /// handed to the transport; forwards round-trip synchronously (C
    /// `ipc_sendrec` in the handler, bind.c:32/80) and the driver's
    /// answer re-enters through the response halves (09).
    fn process_devman(
        &mut self,
        source: Endpoint,
        msg: Option<DevmanMsg>,
        transport: &mut impl Transport,
    ) {
        for action in self.handle_other(source, msg) {
            match action {
                OutAction::Reply { dest, outcome } => {
                    transport.send(dest, &stamp_reply(dest, outcome));
                }
                OutAction::Forward {
                    owner,
                    bind,
                    device,
                    endpoint,
                } => {
                    let mut m = build_forward(bind, device, endpoint);
                    let driver_result = transport
                        .sendrec(owner, &mut m)
                        .and_then(|_| decode_driver_result(&m));
                    let answer = if bind {
                        self.answer_bind(device, driver_result)
                    } else {
                        self.answer_unbind(device, driver_result)
                    };
                    if let OutAction::Reply { dest, outcome } = answer {
                        transport.send(dest, &stamp_reply(dest, outcome));
                    }
                }
                OutAction::Nothing => {}
            }
        }
    }
}

/// Stamp a `DEVMAN_REPLY` (05 §2.3): the RESULT word plus — on ADD
/// success — the new DEVICE_ID (DM-P2-1's dual-word reply). C mutates
/// the incoming message in place; the decoded pipeline no longer holds
/// it, so the reply is built fresh (`m_source` is kernel-overwritten on
/// receipt; setting it to the destination keeps the value meaningful).
fn stamp_reply(dest: Endpoint, outcome: Result<DeviceId, Errno>) -> Message {
    let (res, id) = match outcome {
        Ok(id) => (0, Some(id.0 as i32)),
        Err(e) => (e.to_i32(), None),
    };
    let mut m = Message {
        m_source: dest,
        m_type: 0,
        m_u: MessageUnion::default(),
    };
    apply_reply_with_id(&mut m, res, id);
    m
}

/// Build the BIND/UNBIND message forwarded to a device owner. C reuses
/// the RS message verbatim — m_type stays, DEVICE_ID/ENDPOINT words ride
/// along (bind.c:24-25/:74-75); same bytes, built explicitly here.
fn build_forward(bind: bool, device: DeviceId, endpoint: Endpoint) -> Message {
    Message {
        m_source: Endpoint(0),
        m_type: if bind { DEVMAN_BIND } else { DEVMAN_UNBIND },
        m_u: MessageUnion {
            m_m4: MessageM4 {
                m4l2: device.0 as i64,
                m4l3: endpoint.0 as i64,
                ..MessageM4::default()
            },
        },
    }
}

/// Read the driver's RESULT word out of an answered forward (C reads
/// `m->DEVMAN_RESULT` after the sendrec returns, bind.c:37/:85).
fn decode_driver_result(m: &Message) -> Result<(), Errno> {
    match result(m) {
        0 => Ok(()),
        e => Err(Errno::from_i32(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::structs::DeviceState;
    use crate::vtreefs::{Incoming, Ino, Request, Reply, VecTransport};
    use minix_types::{DEVMAN_ADD_DEV, DEVMAN_BIND, DEVMAN_DEL_DEV, DEVMAN_REPLY, DEVMAN_UNBIND, Errno, RS_PROC_NR};

    fn server() -> Server {
        let cfg = ServerConfig::devman_default(crate::hooks::RootStat::devman_root());
        Server::new(&cfg, FsHooks::empty()).unwrap()
    }

    fn wire_usb() -> Vec<u8> {
        let mut buf = alloc::vec![0u8; 16 + 16];
        buf[0..4].copy_from_slice(&1i32.to_le_bytes());
        buf[4..8].copy_from_slice(&0i32.to_le_bytes());
        let mut s = Vec::new();
        let mut push = |t: &str| -> u32 {
            let o = (buf.len() + s.len()) as u32;
            s.extend_from_slice(t.as_bytes());
            s.push(0);
            o
        };
        let no = push("usb");
        let an = push("dev_type");
        let ad = push("USB_DEV");
        buf[8..12].copy_from_slice(&no.to_le_bytes());
        buf[16..20].copy_from_slice(&0u32.to_le_bytes());
        buf[20..24].copy_from_slice(&an.to_le_bytes());
        buf[24..28].copy_from_slice(&ad.to_le_bytes());
        buf.extend_from_slice(&s);
        buf
    }

    #[test]
    fn lifecycle_add_bind_unbind_del() {
        // 09 §1.3 binding path, executable: ADD → BIND → UNBIND → DEL.
        let mut srv = server();
        // ADD (driver endpoint 9).
        let acts = srv.handle_other(
            Endpoint(9),
            DevmanMsg::classify(DEVMAN_ADD_DEV, &wire_usb(), 0, Endpoint(0)),
        );
        let id = match acts[..] {
            [OutAction::Reply { dest: Endpoint(9), outcome: Ok(id) }] => id,
            ref other => panic!("ADD failed: {other:?}"),
        };
        assert_eq!(id, DeviceId(1));
        // BIND (RS only): forward to the owner.
        let acts =
            srv.handle_other(RS_PROC_NR, DevmanMsg::classify(DEVMAN_BIND, &[], id.0 as i32, Endpoint(4)));
        match acts[..] {
            [OutAction::Forward { owner: Endpoint(9), bind: true, .. }] => {}
            ref other => panic!("BIND failed: {other:?}"),
        }
        // Non-RS bind: nothing.
        let acts =
            srv.handle_other(Endpoint(9), DevmanMsg::classify(DEVMAN_BIND, &[], id.0 as i32, Endpoint(4)));
        assert_eq!(acts, alloc::vec![OutAction::Nothing]);
        // UNBIND → forward; driver OK tested at bind.rs level.
        let acts = srv.handle_other(
            RS_PROC_NR,
            DevmanMsg::classify(DEVMAN_UNBIND, &[], id.0 as i32, Endpoint(4)),
        );
        assert!(matches!(acts[..], [OutAction::Forward { bind: false, .. }]));
        // DEL → reply Ok + REMOVE queued (queue asserted at 06 level).
        let acts =
            srv.handle_other(Endpoint(9), DevmanMsg::classify(DEVMAN_DEL_DEV, &[], id.0 as i32, Endpoint(0)));
        assert!(matches!(
            acts[..],
            [OutAction::Reply { outcome: Ok(_), .. }]
        ));
        assert!(srv.devices().get(id).is_none());
    }

    #[test]
    fn unknown_is_nothing() {
        // 05 §2.6: unmatched → run nothing, reply nothing. classify maps
        // the A-6 codes (0x1202 = ADD_BUS) to None; the server answers
        // None with a single Nothing.
        let mut srv = server();
        let acts = srv.handle_other(Endpoint(9), DevmanMsg::classify(0x1202, &[], 0, Endpoint(0)));
        assert_eq!(acts, alloc::vec![OutAction::Nothing]);
    }

    #[test]
    fn run_fs_replies_carry_errors() {
        // DM-P1-2: the unified loop's FS half carries errors inside the
        // Reply — no sentinel bytes, no silent Ino(0) success.
        let mut srv = server();
        let mut t = VecTransport::new(alloc::vec![
            Incoming::Fs(Request::Mount { is_root: true }),
            Incoming::Fs(Request::Mount { is_root: false }),
            Incoming::Fs(Request::Lookup {
                dir: Ino(1),
                name: String::from("nope"),
            }),
        ]);
        srv.run(&mut t);
        assert_eq!(
            t.replies,
            alloc::vec![
                Reply::Mounted(Err(Errno::EINVAL)),
                Reply::Mounted(Ok(Ino(1))),
                Reply::Found(Err(Errno::ENOENT)),
            ]
        );
    }

    #[test]
    fn run_devman_add_sends_stamped_dual_word_reply() {
        // ADD through the loop: the server stamps the reply
        // (apply_reply_with_id) and the dumb-pipe transport moves it —
        // RESULT=0 (m4_l1) + DEVICE_ID (m4_l2), C device.c:270→:213-219.
        let mut srv = server();
        let wire = wire_usb();
        let mut t = VecTransport::new(alloc::vec![Incoming::Devman {
            source: Endpoint(9),
            msg: DevmanMsg::classify(DEVMAN_ADD_DEV, &wire, 0, Endpoint(0)),
        }]);
        srv.run(&mut t);
        assert_eq!(t.sent.len(), 1);
        let (dest, msg) = &t.sent[0];
        assert_eq!(*dest, Endpoint(9));
        assert_eq!(msg.m_type, DEVMAN_REPLY);
        assert_eq!(unsafe { msg.m_u.m_m4 }.m4l1, 0);
        assert_eq!(unsafe { msg.m_u.m_m4 }.m4l2, 1);
        assert!(t.replies.is_empty()); // DEVMAN traffic never uses Reply
    }

    #[test]
    fn run_bind_forward_roundtrip() {
        // BIND through the loop: the server builds the forward (BIND
        // words intact), the transport sendrecs the owner, the driver's
        // RESULT re-enters answer_bind, RS hears the stamped reply.
        let mut srv = server();
        let wire = wire_usb();
        srv.handle_other(
            Endpoint(9),
            DevmanMsg::classify(DEVMAN_ADD_DEV, &wire, 0, Endpoint(0)),
        );
        let mut t = VecTransport::new(alloc::vec![Incoming::Devman {
            source: RS_PROC_NR,
            msg: DevmanMsg::classify(DEVMAN_BIND, &[], 1, Endpoint(4)),
        }]);
        t.sendrec_script.push_back(Ok(0));
        srv.run(&mut t);
        // Forward to the owner: BIND + words intact (C bind.c:24-32).
        assert_eq!(t.sendrecs.len(), 1);
        let (owner, fwd) = &t.sendrecs[0];
        assert_eq!(*owner, Endpoint(9));
        assert_eq!(fwd.m_type, DEVMAN_BIND);
        assert_eq!(unsafe { fwd.m_u.m_m4 }.m4l2, 1);
        assert_eq!(unsafe { fwd.m_u.m_m4 }.m4l3, 4);
        // Reply to RS: RESULT 0 (driver OK → BOUND).
        assert_eq!(t.sent.len(), 1);
        let (dest, rep) = &t.sent[0];
        assert_eq!(*dest, RS_PROC_NR);
        assert_eq!(rep.m_type, DEVMAN_REPLY);
        assert_eq!(unsafe { rep.m_u.m_m4 }.m4l1, 0);
        assert_eq!(
            srv.devices().get(DeviceId(1)).unwrap().state,
            DeviceState::Bound
        );
    }

    #[test]
    fn run_bind_forward_driver_error_reaches_rs() {
        // Driver refuses the bind: no state change, and the driver's
        // errno is exactly what RS hears (C bind.c:37-43).
        let mut srv = server();
        let wire = wire_usb();
        srv.handle_other(
            Endpoint(9),
            DevmanMsg::classify(DEVMAN_ADD_DEV, &wire, 0, Endpoint(0)),
        );
        let mut t = VecTransport::new(alloc::vec![Incoming::Devman {
            source: RS_PROC_NR,
            msg: DevmanMsg::classify(DEVMAN_BIND, &[], 1, Endpoint(4)),
        }]);
        t.sendrec_script.push_back(Ok(5)); // driver RESULT = 5 (EIO)
        srv.run(&mut t);
        let (_, rep) = &t.sent[0];
        assert_eq!(unsafe { rep.m_u.m_m4 }.m4l1, 5);
        assert_eq!(
            srv.devices().get(DeviceId(1)).unwrap().state,
            DeviceState::Unbound
        );
    }
}
