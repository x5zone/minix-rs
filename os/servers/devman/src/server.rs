//! Server assembly: the lifecycle through one entry (doc 09, binding path).
//!
//! 07/08/09 handlers are injectable free functions (no globals — tests
//! drive them directly). This module assembles them over owned state so
//! the full device lifecycle (ADD → event → BIND → UNBIND → DEL) runs
//! through a single `handle_other`, exactly the binding-path diagram in
//! 09 §1.3 made executable. Production transport (kernel IPC) will call
//! `handle_other` per message and execute the returned [`OutAction`]s;
//! until `minix-sys` implements it, this assembly is fully tested but
//! unwired (P1-6, narrowed to transport — same status as 02's `run`).

use alloc::vec::Vec;
use minix_types::{Endpoint, Errno};

use crate::add_device::do_add;
use crate::bind::{do_bind, do_unbind, on_bind_response, on_unbind_response, Action};
use crate::del_device::do_del;
use crate::device_tree::{default_file_stat, DeviceTree};
use crate::files::{register_file, EventFile, FileEntry};
use crate::hooks::{FsHooks, ServerConfig};
use crate::ipc::dispatch;
use crate::ipc::Handler;
use crate::structs::{DeviceId, Event};
use crate::vtreefs::VTreeFs;
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

    /// One non-filesystem message through dispatch (05) to its handler
    /// (07/08/09). `body` is the already-copied grant payload for ADD
    /// (transport owns the safecopy, 05 §2.2); BIND/UNBIND carry
    /// `(device_id, endpoint)` as words (05 §1.1 phase table).
    pub fn handle_other(
        &mut self,
        m_type: i32,
        source: Endpoint,
        body: &[u8],
        word2: i32,
        word3: Endpoint,
    ) -> Vec<OutAction> {
        match dispatch(m_type) {
            Handler::Add => {
                let parsed = match parse_device(body) {
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
            Handler::Del => {
                let id = DeviceId(word2 as u32);
                let mut sunk = Vec::new();
                let outcome = do_del(
                    &mut self.devices,
                    self.vtreefs.tree_mut(),
                    id,
                    &mut |ev| sunk.push(ev),
                )
                .map(|_| id);
                for ev in sunk {
                    self.push_event(ev);
                }
                alloc::vec![OutAction::Reply {
                    dest: source,
                    outcome,
                }]
            }
            Handler::Bind => match do_bind(&self.devices, source, DeviceId(word2 as u32), word3) {
                Action::Forward { owner, device, endpoint, .. } => {
                    alloc::vec![OutAction::Forward { owner, bind: true, device, endpoint }]
                }
                Action::Reply(outcome) => alloc::vec![OutAction::Reply {
                    dest: source,
                    outcome: outcome.map(|_| DeviceId(word2 as u32)),
                }],
                Action::Dropped => alloc::vec![OutAction::Nothing],
            },
            Handler::Unbind => {
                match do_unbind(&self.devices, source, DeviceId(word2 as u32), word3) {
                    Action::Forward { owner, device, endpoint, .. } => {
                        alloc::vec![OutAction::Forward { owner, bind: false, device, endpoint }]
                    }
                    Action::Reply(outcome) => alloc::vec![OutAction::Reply {
                        dest: source,
                        outcome: outcome.map(|_| DeviceId(word2 as u32)),
                    }],
                    Action::Dropped => alloc::vec![OutAction::Nothing],
                }
            }
            Handler::Ignored => alloc::vec![OutAction::Nothing],
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use minix_types::{
        DEVMAN_ADD_DEV, DEVMAN_BIND, DEVMAN_DEL_DEV, DEVMAN_UNBIND, RS_PROC_NR,
    };

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
        let acts = srv.handle_other(DEVMAN_ADD_DEV, Endpoint(9), &wire_usb(), 0, Endpoint(0));
        let id = match acts[..] {
            [OutAction::Reply { dest: Endpoint(9), outcome: Ok(id) }] => id,
            ref other => panic!("ADD failed: {other:?}"),
        };
        assert_eq!(id, DeviceId(1));
        // BIND (RS only): forward to the owner.
        let acts = srv.handle_other(DEVMAN_BIND, RS_PROC_NR, &[], id.0 as i32, Endpoint(4));
        match acts[..] {
            [OutAction::Forward { owner: Endpoint(9), bind: true, .. }] => {}
            ref other => panic!("BIND failed: {other:?}"),
        }
        // Non-RS bind: nothing.
        let acts = srv.handle_other(DEVMAN_BIND, Endpoint(9), &[], id.0 as i32, Endpoint(4));
        assert_eq!(acts, alloc::vec![OutAction::Nothing]);
        // UNBIND → forward; driver OK tested at bind.rs level.
        let acts = srv.handle_other(DEVMAN_UNBIND, RS_PROC_NR, &[], id.0 as i32, Endpoint(4));
        assert!(matches!(acts[..], [OutAction::Forward { bind: false, .. }]));
        // DEL → reply Ok + REMOVE queued (queue asserted at 06 level).
        let acts = srv.handle_other(DEVMAN_DEL_DEV, Endpoint(9), &[], id.0 as i32, Endpoint(0));
        assert!(matches!(
            acts[..],
            [OutAction::Reply { outcome: Ok(_), .. }]
        ));
        assert!(srv.devices().get(id).is_none());
    }

    #[test]
    fn unknown_is_nothing() {
        // 05 §2.6: unmatched → run nothing, reply nothing.
        let mut srv = server();
        let acts = srv.handle_other(0x1202, Endpoint(9), &[], 0, Endpoint(0));
        assert_eq!(acts, alloc::vec![OutAction::Nothing]);
    }
}
