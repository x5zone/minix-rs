//! devman client library: driver-side registration + bind handling
//! (doc 10-libdevman-client).
//!
//! C: `minix3/minix/lib/libdevman/generic.c` (275 lines) — `save_string` +
//! `serialize_dev` + `devman_add_device` / `devman_del_device` /
//! `devman_init` + `do_bind` / `do_unbind` + `devman_handle_msg`, plus
//! `local.h` (`devman_dev`, `DEVMAN_DEV_NAME_LEN 32`).
//!
//! Entry boundary: kernel crossings (grant issue/revoke, `sendrec`, DS
//! lookup) are [`ClientTransport`] + `ds_lookup` injections — this module
//! starts from "transport ready". All C `panic`s become [`ClientError`]
//! variants ([ARCH:A-7]); drivers decide fail-fast, the library doesn't.

use alloc::string::String;
use alloc::vec::Vec;
use minix_types::{
    DEVMAN_ADD_DEV, DEVMAN_BIND, DEVMAN_DEL_DEV, DEVMAN_REPLY, DEVMAN_UNBIND, Endpoint, Errno,
    Message, MessageM4, MessageUnion,
};

/// C: `DEVMAN_DEV_NAME_LEN 32` (`local.h:7`) — client name array.
/// `snprintf` truncates silently; [`truncate_name`] reproduces it.
pub const DEV_NAME_LEN: usize = 32;

/// C: `int (*)(void *data, endpoint_t ep)` (`devman.h:57`, lib side).
/// The `data` pointer is lib-internal in practice (usb.c passes its own
/// `cb_data` holding dev/interface ids). Rust threads the device id
/// instead — layers needing richer context (11's interface index) resolve
/// it from their own registry. Returns `Result` (C `int` errno → `Errno`).
pub type BindCallback = fn(dev_id: i32, ep: Endpoint) -> Result<(), Errno>;

/// One driver-side device (C: `struct devman_dev`, `local.h:9`).
/// `name` is pre-truncated to 31 chars at construction ([`truncate_name`]);
/// `dev_id` is `None` until the server assigns it (`devman_add_device`
/// stores it back, generic.c:139).
pub struct ClientDevice {
    pub name: String,
    pub parent_id: i32,
    pub attrs: Vec<(String, String)>,
    pub dev_id: Option<i32>,
    pub bind_cb: Option<BindCallback>,
    pub unbind_cb: Option<BindCallback>,
}

impl ClientDevice {
    pub fn new(name: &str, parent_id: i32) -> Self {
        ClientDevice {
            name: truncate_name(name),
            parent_id,
            attrs: Vec::new(),
            dev_id: None,
            bind_cb: None,
            unbind_cb: None,
        }
    }

    pub fn add_attr(&mut self, name: &str, data: &str) {
        self.attrs.push((String::from(name), String::from(data)));
    }
}

/// C: `snprintf(name, 32, …)` truncation, explicit.
pub fn truncate_name(name: &str) -> String {
    if name.len() >= DEV_NAME_LEN {
        String::from(&name[..DEV_NAME_LEN - 1])
    } else {
        String::from(name)
    }
}

/// Client failures. C `panic`s on every one of these paths
/// (generic.c:110/125/129/135/165/169/175/196); Rust reports them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientError {
    /// Transport failed (sendrec/grant). C: `panic("could not talk…")`.
    Transport,
    /// Server replied non-`REPLY` type. C: `panic("illegal response…")`.
    BadReply,
    /// Server replied an error result. C: `panic("could (add|delete)…")`.
    /// Carries the server's errno.
    Rejected(Errno),
}

/// Kernel crossings the client needs, injected (test fakes provided).
pub trait ClientTransport {
    /// Issue a read grant for `buf` (C: `cpf_grant_direct(…, CPF_READ)`).
    fn grant(&mut self, buf: &[u8]) -> Result<i32, ClientError>;
    /// Revoke it (C: `cpf_revoke`).
    fn revoke(&mut self, grant: i32);
    /// Round trip with devman (C: `ipc_sendrec(devman_ep, &msg)`).
    /// Transport failure → `ClientError::Transport` (C panics).
    fn sendrec(&mut self, ep: Endpoint, msg: &mut Message) -> Result<(), ClientError>;
}

/// Encode a device exactly like `serialize_dev` (generic.c:36-99):
/// 16-byte header (`count`, `parent`, name offset, subsystem 0) +
/// 16-byte entries (`type` **0**, name/data offsets, `req_nr` 0) +
/// NUL-terminated strings. `subsystem_offset`/`req_nr` encode 0 —
/// hygienic zeros where C leaves malloc garbage (03 §2.4; byte-compatible
/// because the server never reads either word).
pub fn encode_device(dev: &ClientDevice) -> Vec<u8> {
    let count = dev.attrs.len();
    let mut buf = alloc::vec![0u8; 16 + count * 16];
    // Offsets first (borrows end before the header/entry writes below).
    let mut strings = Vec::new();
    let mut push = |s: &str| -> u32 {
        let off = (16 + count * 16 + strings.len()) as u32;
        strings.extend_from_slice(s.as_bytes());
        strings.push(0);
        off
    };
    let name_off = push(&dev.name);
    let mut eoffs = Vec::new();
    for (n, d) in &dev.attrs {
        eoffs.push((push(n), push(d)));
    }
    buf[0..4].copy_from_slice(&(count as i32).to_le_bytes());
    buf[4..8].copy_from_slice(&dev.parent_id.to_le_bytes());
    buf[8..12].copy_from_slice(&name_off.to_le_bytes());
    // offset 12 (subsystem): 0 (see docs above).
    for (i, (no, dob)) in eoffs.iter().enumerate() {
        let base = 16 + i * 16;
        buf[base..base + 4].copy_from_slice(&0u32.to_le_bytes()); // STATIC
        buf[base + 4..base + 8].copy_from_slice(&no.to_le_bytes());
        buf[base + 8..base + 12].copy_from_slice(&dob.to_le_bytes());
        // req_nr: 0 (see docs above).
    }
    buf.extend_from_slice(&strings);
    buf
}

/// Look up devman's endpoint (C: `ds_retrieve_label_endpt("devman")`,
/// generic.c:193; panic message even says "usb_init" — copy-paste, 10 §2.4).
/// Injected for testability; production passes the DS call.
pub fn init(ds_lookup: impl FnOnce(&str) -> Result<Endpoint, Errno>) -> Result<Endpoint, Errno> {
    ds_lookup("devman")
}

/// C: `devman_add_device` (generic.c:102-149) — encode → grant →
/// sendrec → validate (`REPLY` + result 0) → store id → revoke.
/// Every C `panic` is the matching `ClientError` (see enum docs).
/// (The `dev_list` insert is the caller's `Vec::push` — ownership
/// replaces the TAILQ; A-2.)
pub fn add_device(
    t: &mut impl ClientTransport,
    devman: Endpoint,
    dev: &mut ClientDevice,
) -> Result<i32, ClientError> {
    let buf = encode_device(dev);
    let gid = t.grant(&buf)?;
    let mut msg = Message {
        m_source: Endpoint(0),
        m_type: DEVMAN_ADD_DEV,
        m_u: MessageUnion {
            m_m4: MessageM4 {
                m4l1: gid as i64,
                m4l2: buf.len() as i64,
                ..MessageM4::default()
            },
        },
    };
    let r = sendrec_checked(t, devman, &mut msg);
    t.revoke(gid);
    let id = r?;
    dev.dev_id = Some(id);
    Ok(id)
}

/// Shared sendrec + reply validation (ADD and DEL paths identical).
fn sendrec_checked(
    t: &mut impl ClientTransport,
    devman: Endpoint,
    msg: &mut Message,
) -> Result<i32, ClientError> {
    t.sendrec(devman, msg)?;
    if msg.m_type != DEVMAN_REPLY {
        return Err(ClientError::BadReply);
    }
    let res = unsafe { msg.m_u.m_m4 }.m4l1 as i32;
    if res != 0 {
        return Err(ClientError::Rejected(Errno::from_i32(res)));
    }
    // Reply-phase m4l2 doubles as DEVICE_ID (05 §1.1 phase table).
    Ok(unsafe { msg.m_u.m_m4 }.m4l2 as i32)
}

/// C: `devman_del_device` (generic.c:154-183) — id-only message, same
/// triple validation; list removal is the caller's `Vec` retain (A-2).
pub fn del_device(
    t: &mut impl ClientTransport,
    devman: Endpoint,
    dev_id: i32,
) -> Result<(), ClientError> {
    let mut msg = Message {
        m_source: Endpoint(0),
        m_type: DEVMAN_DEL_DEV,
        m_u: MessageUnion {
            m_m4: MessageM4 {
                m4l2: dev_id as i64,
                ..MessageM4::default()
            },
        },
    };
    sendrec_checked(t, devman, &mut msg)?;
    Ok(())
}

/// One registered device for message handling (id + callbacks).
/// (C scans the global `dev_list`; Rust scans a caller-owned slice —
/// same lookup, no global.)
pub struct HandledDevice {
    pub dev_id: i32,
    pub bind_cb: Option<BindCallback>,
    pub unbind_cb: Option<BindCallback>,
}

/// C: `devman_handle_msg` + `do_bind`/`do_unbind` (generic.c:207-275).
/// Non-devman senders are ignored silently (`return 0`, :261-264 —
/// the client-side mirror of the server's EPERM-no-reply, 05 §2.4).
/// Found + callback → run it, reply its result; found without callback
/// or missing → reply `ENODEV` (:224-226/:250-252 — indistinguishable,
/// like C). Returns `true` iff handled (C's 1/0).
/// Replies go through `respond` (C: `ipc_send(devman_ep, m)`).
pub fn handle_msg(
    msg: &mut Message,
    devman: Endpoint,
    devices: &[HandledDevice],
    respond: &mut dyn FnMut(&Message),
) -> bool {
    // SAFETY: full-message m4 view (same precedent as 05's message.rs).
    if msg.m_source != devman {
        return false;
    }
    let want_bind = match msg.m_type {
        t if t == DEVMAN_BIND => true,
        t if t == DEVMAN_UNBIND => false,
        _ => return false,
    };
    let m4 = unsafe { msg.m_u.m_m4 };
    let id = m4.m4l2 as i32;
    let ep = Endpoint(m4.m4l3 as i32);
    // Reply assembly mirrors C exactly (m_type = REPLY, RESULT set).
    // Found-but-no-callback and missing are indistinguishable (ENODEV),
    // like C (:224-226/:250-252).
    let code = match devices.iter().find(|d| d.dev_id == id) {
        Some(d) => {
            let cb = if want_bind { d.bind_cb } else { d.unbind_cb };
            match cb {
                Some(f) => f(id, ep).err().map(|e| e.to_i32()).unwrap_or(0),
                None => Errno::ENODEV.to_i32(),
            }
        }
        None => Errno::ENODEV.to_i32(),
    };
    msg.m_type = DEVMAN_REPLY;
    // Union field *assignment* is safe Rust (only reads are unsafe).
    msg.m_u.m_m4.m4l1 = code as i64;
    respond(msg);
    true
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use std::vec::Vec;

    struct FakeTransport {
        /// Scripted (m_type, result, device_id) per sendrec call.
        script: Vec<(i32, i32, i32)>,
        pub log: Vec<Message>,
        pub grants: i32,
        pub revoked: Vec<i32>,
    }

    impl FakeTransport {
        fn new(script: Vec<(i32, i32, i32)>) -> Self {
            FakeTransport {
                script,
                log: Vec::new(),
                grants: 0,
                revoked: Vec::new(),
            }
        }
    }

    impl ClientTransport for FakeTransport {
        fn grant(&mut self, _buf: &[u8]) -> Result<i32, ClientError> {
            self.grants += 1;
            Ok(self.grants)
        }

        fn revoke(&mut self, grant: i32) {
            self.revoked.push(grant);
        }

        fn sendrec(&mut self, _ep: Endpoint, msg: &mut Message) -> Result<(), ClientError> {
            self.log.push(*msg);
            let (t, res, id) = self.script.remove(0);
            msg.m_type = t;
            msg.m_u.m_m4.m4l1 = res as i64;
            msg.m_u.m_m4.m4l2 = id as i64;
            Ok(())
        }
    }

    fn ok_add() -> (i32, i32, i32) {
        (DEVMAN_REPLY, 0, 7)
    }

    #[test]
    fn encode_matches_serialize_dev() {
        // Layout mirror of serialize_dev (generic.c:36-99): header +
        // entries + strings; type 0; subsystem/req_nr 0.
        let mut dev = ClientDevice::new("usb", 0);
        dev.add_attr("dev_type", "USB_DEV");
        let buf = encode_device(&dev);
        assert_eq!(&buf[0..4], &1i32.to_le_bytes());
        assert_eq!(&buf[4..8], &0i32.to_le_bytes());
        assert_eq!(&buf[16..20], &0u32.to_le_bytes()); // STATIC
        assert_eq!(&buf[12..16], &[0, 0, 0, 0]); // subsystem 0
        assert!(buf.ends_with(b"USB_DEV\0"));
    }

    #[test]
    fn name_truncates_at_31() {
        // C: snprintf(name, 32, …) (local.h:7 + usb.c:168).
        let long = "x".repeat(40);
        assert_eq!(truncate_name(&long).len(), 31);
        assert_eq!(truncate_name("usb"), "usb");
    }

    #[test]
    fn add_happy_path() {
        let mut t = FakeTransport::new(std::vec![ok_add()]);
        let mut dev = ClientDevice::new("usb", 0);
        dev.add_attr("dev_type", "USB_DEV");
        let id = add_device(&mut t, Endpoint(2), &mut dev).unwrap();
        assert_eq!(id, 7);
        assert_eq!(dev.dev_id, Some(7));
        // Grant issued and revoked (C: cpf_grant_direct + cpf_revoke).
        assert_eq!(t.revoked, std::vec![1]);
        // Request shape: ADD + grant words.
        let sent = &t.log[0];
        assert_eq!(sent.m_type, DEVMAN_ADD_DEV);
    }

    #[test]
    fn add_maps_failures() {
        // Server rejection → Rejected(errno), not panic.
        let mut t = FakeTransport::new(std::vec![(DEVMAN_REPLY, 19, 0)]);
        let mut dev = ClientDevice::new("usb", 0);
        assert_eq!(
            add_device(&mut t, Endpoint(2), &mut dev),
            Err(ClientError::Rejected(Errno::ENODEV))
        );
        assert_eq!(dev.dev_id, None);
        // Non-REPLY → BadReply.
        let mut t2 = FakeTransport::new(std::vec![(DEVMAN_ADD_DEV, 0, 0)]);
        let mut dev2 = ClientDevice::new("usb", 0);
        assert_eq!(
            add_device(&mut t2, Endpoint(2), &mut dev2),
            Err(ClientError::BadReply)
        );
    }

    #[test]
    fn del_happy_path() {
        let mut t = FakeTransport::new(std::vec![(DEVMAN_REPLY, 0, 0)]);
        del_device(&mut t, Endpoint(2), 7).unwrap();
        assert_eq!(t.log[0].m_type, DEVMAN_DEL_DEV);
    }

    #[test]
    fn handle_msg_gate_and_dispatch() {
        // Non-devman sender: silent false (generic.c:261-264).
        let devs = [HandledDevice {
            dev_id: 7,
            bind_cb: Some(|_, _| Ok(())),
            unbind_cb: None,
        }];
        let mut replies = Vec::new();
        let mut msg = Message {
            m_source: Endpoint(99),
            m_type: DEVMAN_BIND,
            m_u: MessageUnion {
                m_m4: MessageM4 {
                    m4l2: 7,
                    m4l3: 4,
                    ..MessageM4::default()
                },
            },
        };
        assert!(!handle_msg(&mut msg, Endpoint(2), &devs, &mut |m| replies.push(*m)));
        assert!(replies.is_empty());
        // BIND from devman: callback runs, REPLY posted.
        msg.m_source = Endpoint(2);
        assert!(handle_msg(&mut msg, Endpoint(2), &devs, &mut |m| replies.push(*m)));
        assert_eq!(replies.len(), 1);
        assert_eq!(replies[0].m_type, DEVMAN_REPLY);
        // Missing id → ENODEV reply (indistinguishable from no-cb, like C).
        let mut msg2 = Message {
            m_source: Endpoint(2),
            m_type: DEVMAN_UNBIND,
            m_u: MessageUnion {
                m_m4: MessageM4 {
                    m4l2: 42,
                    m4l3: 4,
                    ..MessageM4::default()
                },
            },
        };
        assert!(handle_msg(&mut msg2, Endpoint(2), &devs, &mut |m| replies.push(*m)));
        assert_eq!(replies.len(), 2);
    }

    #[test]
    fn init_resolves_endpoint() {
        // C: ds_retrieve_label_endpt("devman") (generic.c:193).
        assert_eq!(init(|_| Ok(Endpoint(2))), Ok(Endpoint(2)));
        assert_eq!(init(|_| Err(Errno::ENODEV)), Err(Errno::ENODEV));
    }
}
