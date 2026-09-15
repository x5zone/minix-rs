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
use crate::hooks::ServerConfig;
use crate::ipc::{apply_reply_with_id, result, DevmanMsg};
use crate::structs::{DeviceId, Event};
use crate::vtreefs::{Incoming, InodeContent, Ino, Request, Reply, Transport, VTreeFs};
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

/// Owned devman server: framework + device tree + events inode.
/// The device tree is built **lazily** on the first successful mount
/// (C main.c:36-43 `static int first` → `devman_init_devices`); until
/// then `devices` is `None` and the `Option` itself is the once-guard
/// (DM-P1-3). The events queue rides on its inode (`InodeContent::Events`,
/// DM-P1-5); `events_ino` is the handle `push_event` writes through.
pub struct Server {
    vtreefs: VTreeFs,
    devices: Option<DeviceTree>,
    events_ino: Option<Ino>,
}

impl Server {
    /// C: `main` + `run_vtreefs` init half — framework only. The device
    /// tree and events file appear at first mount (`ensure_devices`),
    /// exactly where C's `init_hook` runs (mount.c:24-25).
    pub fn new(config: &ServerConfig) -> Result<Self, Errno> {
        let vtreefs = VTreeFs::new(config)?;
        Ok(Server {
            vtreefs,
            devices: None,
            events_ino: None,
        })
    }

    /// Build the device tree + events file once, on the first successful
    /// mount (C main.c:36-43: `if (first) devman_init_devices(); first = 0;`).
    /// The `Option::is_none` check is the guard — nothing can interleave
    /// (single-threaded event loop), so exactly one build ever happens.
    /// Failure propagates as the mount's error (A-7: C `panic`s in the
    /// same spot, vtreefs.c init sequence).
    fn ensure_devices(&mut self) -> Result<(), Errno> {
        if self.devices.is_some() {
            return Ok(());
        }
        let devices = DeviceTree::new(self.vtreefs.tree_mut(), default_file_stat())?;
        let events_ino = self
            .vtreefs
            .lookup(self.vtreefs.tree().root(), "events")?;
        self.devices = Some(devices);
        self.events_ino = Some(events_ino);
        Ok(())
    }

    /// Queue an ADD/REMOVE event line on the events inode's own content
    /// (DM-P1-5: no side table, no cookie — the inode is the file).
    fn push_event(&mut self, ev: Event) {
        let Some(ino) = self.events_ino else {
            return; // pre-init: no events file exists to receive anything
        };
        if let Some(InodeContent::Events(q)) = self.vtreefs.tree_mut().content_mut(ino) {
            let _ = q.push(ev);
        }
    }

    /// One non-filesystem message through its typed decode (05) to its
    /// handler (07/08/09). `msg` arrives pre-classified
    /// ([`DevmanMsg::classify`] marries the routing enum to the payload
    /// words — the phase table's "which word means what" question is
    /// answered before this signature, not inside it); `None` — the
    /// Ignored types, 05 §2.6 — sends nothing.
    pub fn handle_other(&mut self, source: Endpoint, msg: Option<DevmanMsg>) -> Vec<OutAction> {
        let Some(msg) = msg else {
            return alloc::vec![OutAction::Nothing];
        };
        // Pre-init traffic: the device database does not exist before the
        // first mount (DM-P1-3). Unreachable in practice — VFS mounts
        // devman before any driver can reach it — and fail-closed where C
        // would walk an uninitialized BSS `root_dev`.
        let Some(devices) = self.devices.as_mut() else {
            return match msg {
                DevmanMsg::Add { .. } | DevmanMsg::Del { .. } | DevmanMsg::Bind { .. }
                | DevmanMsg::Unbind { .. } => {
                    alloc::vec![OutAction::Reply {
                        dest: source,
                        outcome: Err(Errno::ENODEV),
                    }]
                }
            };
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
                    devices,
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
                    devices,
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
                match do_bind(devices, source, device, driver) {
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
                match do_unbind(devices, source, device, driver) {
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
        let Some(devices) = self.devices.as_mut() else {
            // Unreachable: forwards only exist post-init (handle_other's
            // pre-init guard answers before any forward is produced).
            return OutAction::Nothing;
        };
        let outcome = on_bind_response(devices, device, driver);
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
        let Some(devices) = self.devices.as_mut() else {
            return OutAction::Nothing; // same unreachability as answer_bind
        };
        let outcome = on_unbind_response(
            devices,
            self.vtreefs.tree_mut(),
            device,
            driver,
        );
        OutAction::Reply {
            dest: minix_types::RS_PROC_NR,
            outcome: outcome.map(|_| device),
        }
    }

    /// Inspection: the device tree, if the first mount has happened
    /// (lazy init, DM-P1-3).
    pub fn devices(&self) -> Option<&DeviceTree> {
        self.devices.as_ref()
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
            Request::Mount { is_root } => {
                let mounted = self.vtreefs.mount(is_root);
                // C: fs_mount calls init_hook on success (mount.c:24-25);
                // the guarded tree build is devman's hook body (main.c:36-43).
                Reply::Mounted(mounted.and_then(|ino| self.ensure_devices().map(|_| ino)))
            }
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

    /// A mounted server: lazy init (DM-P1-3) needs the first mount before
    /// the device database exists, so handler tests drive one mount first.
    fn server() -> Server {
        let cfg = ServerConfig::devman_default(crate::hooks::RootStat::devman_root());
        let mut srv = Server::new(&cfg).unwrap();
        mount(&mut srv);
        srv
    }

    fn mount(srv: &mut Server) {
        let mut t =
            VecTransport::new(alloc::vec![Incoming::Fs(Request::Mount { is_root: false })]);
        srv.run(&mut t);
        assert!(matches!(t.replies[..], [Reply::Mounted(Ok(_))]));
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
        assert!(srv.devices().unwrap().get(id).is_none());
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
    fn lazy_init_devices_appear_at_first_mount() {
        // DM-P1-3: pre-mount the device database does not exist — lookup
        // of `devices` misses (the inode appears only when init runs at
        // mount, C main.c:36-43 via mount.c:24-25) and DEVMAN traffic
        // fails closed with ENODEV instead of walking an uninitialized
        // tree. After the mount everything works; a second mount does
        // NOT re-init (ids keep counting — C's `static int first`).
        let cfg = ServerConfig::devman_default(crate::hooks::RootStat::devman_root());
        let mut srv = Server::new(&cfg).unwrap();
        assert!(srv.devices().is_none());

        let mut t = VecTransport::new(alloc::vec![Incoming::Fs(Request::Lookup {
            dir: Ino(1),
            name: String::from("devices"),
        })]);
        srv.run(&mut t);
        assert_eq!(t.replies, alloc::vec![Reply::Found(Err(Errno::ENOENT))]);

        let acts = srv.handle_other(
            Endpoint(9),
            DevmanMsg::classify(DEVMAN_ADD_DEV, &wire_usb(), 0, Endpoint(0)),
        );
        assert!(matches!(
            acts[..],
            [OutAction::Reply {
                outcome: Err(Errno::ENODEV),
                ..
            }]
        ));

        mount(&mut srv);
        assert!(srv.devices().is_some());
        let acts = srv.handle_other(
            Endpoint(9),
            DevmanMsg::classify(DEVMAN_ADD_DEV, &wire_usb(), 0, Endpoint(0)),
        );
        assert!(matches!(
            acts[..],
            [OutAction::Reply {
                outcome: Ok(_),
                ..
            }]
        ));

        // Remove the device, mount again, re-add: the id continues from
        // where the counter was (2), proving the second mount did not
        // rebuild the tree (a re-init would hand out 1 again).
        let acts = srv.handle_other(
            Endpoint(9),
            DevmanMsg::classify(DEVMAN_DEL_DEV, &[], 1, Endpoint(0)),
        );
        assert!(matches!(
            acts[..],
            [OutAction::Reply {
                outcome: Ok(_),
                ..
            }]
        ));
        mount(&mut srv);
        let acts = srv.handle_other(
            Endpoint(9),
            DevmanMsg::classify(DEVMAN_ADD_DEV, &wire_usb(), 0, Endpoint(0)),
        );
        match acts[..] {
            [OutAction::Reply {
                outcome: Ok(id), ..
            }] => assert_eq!(id, DeviceId(2), "second mount must not re-init"),
            ref other => panic!("re-add failed: {other:?}"),
        }
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
            srv.devices().unwrap().get(DeviceId(1)).unwrap().state,
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
            srv.devices().unwrap().get(DeviceId(1)).unwrap().state,
            DeviceState::Unbound
        );
    }
}
