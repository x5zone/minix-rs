//! Service assembly: startup, the forwarded open/close, the rule controls,
//! and the shared block pump.
//!
//! C correspondence: `fbd.c`'s `sef_cb_init_fresh` (`fbd.c:51-87`: parse
//! `label`/`minor`, resolve the lower driver, allocate the buffer,
//! announce), `fbd_open`/`fbd_close` (`fbd.c:139-182`, forwarded to the
//! lower driver), `fbd_ioctl`'s rule branch (`fbd.c:191-200` →
//! `rule_ctl`, `rule.c:15-120`), and the loop shell `blockdriver_task`
//! (`fbd.c:118-130`). The loop and the RS birth handshake live in the
//! shared driver runtime; the pump (restart gate, open gate, reply shapes)
//! lives in `minix-blockdriver::pump`.
//!
//! Two jobs are the service's because the hooks cannot do them: forwarding
//! open/close needs the lower driver's endpoint (resolved here at startup),
//! and the rule controls need the caller's grant decoded out of the
//! request (the control hook carries no data plane).

use minix_bdev::client::{BdevOp, NO_ID};
use minix_bdev::driver::Link;
use minix_bdev::transport::{Destination, Transport};
use minix_blockdriver::protocol::{BdevRequest, DeviceMinor};
use minix_blockdriver::pump::BlockService;
use minix_blockdriver::wire;
use minix_driver_rt::runtime::DriverHandler;
use minix_driver_rt::transport::DriverTransport;
use minix_types::{BDEV_REPLY, EDONTREPLY, Endpoint, Message};

use crate::face::FbdFace;
use crate::protocol::{FBDCADDRULE, FBDCDELRULE, FBDCGETRULE, RULE_SIZE, decode_rule, encode_rule};

/// The notification family shares the 0x1000 base with the source in the
/// low byte (C `is_ipc_notify`, `const.h`).
const fn is_ipc_notify(message_type: i32) -> bool {
    (message_type & !0xff) == 0x1000
}

/// The FBD service: forwarded opens and controls first, then the pump.
pub struct FbdService {
    pump: BlockService<FbdFace>,
}

impl FbdService {
    /// A service over a fresh face.
    pub fn new(face: FbdFace) -> Self {
        FbdService {
            pump: BlockService::new(face),
        }
    }

    /// The face (tests and the binary's diagnostics).
    pub fn face(&self) -> &FbdFace {
        self.pump.face()
    }

    /// The face, mutably.
    pub fn face_mut(&mut self) -> &mut FbdFace {
        self.pump.face_mut()
    }

    /// Handle one delivered message.
    pub fn dispatch<T: DriverTransport>(&mut self, transport: &mut T, msg: &Message) {
        if !is_ipc_notify(msg.m_type) {
            // The lower driver's endpoint must be known before any forward.
            if self.pump.face().downstream() < 0 {
                self.pump.dispatch(transport, msg);
                return;
            }
            if msg.m_type == BdevRequest::Open.message_type() {
                let access = wire::read_i32(msg, wire::request::ACCESS);
                let id = wire::read_i32(msg, wire::request::ID);
                let minor = wire::read_u32(msg, wire::request::MINOR);
                // C's `fbd_open` forwards unconditionally and does no
                // open bookkeeping of its own (`fbd.c:139-163`), so the
                // forward is one exchange, not the client library's
                // tracked open.
                let _ = minor;
                let status = self.forward(transport, BdevOp::Open, NO_ID, access);
                if status == 0 {
                    // The pump owns the restart gate's open set.
                    self.pump.note_open(DeviceMinor(minor));
                }
                self.reply(transport, msg.m_source, status, id);
                return;
            }
            if msg.m_type == BdevRequest::Close.message_type() {
                let id = wire::read_i32(msg, wire::request::ID);
                let status = self.forward(transport, BdevOp::Close, NO_ID, 0);
                self.reply(transport, msg.m_source, status, id);
                return;
            }
            if msg.m_type == BdevRequest::Ioctl.message_type() {
                let request = wire::read_u64(msg, wire::request::REQUEST);
                let id = wire::read_i32(msg, wire::request::ID);
                if let Some(status) = self.rule_control(transport, msg, request) {
                    self.reply(transport, msg.m_source, status, id);
                    return;
                }
            }
        }
        self.pump.dispatch(transport, msg);
    }

    /// Forward one control request to the lower driver (`fbd_open`/
    /// `fbd_close`, `fbd.c:139-182`): one exchange, reply checked for the
    /// block general reply and the identifier.
    fn forward<T: DriverTransport>(
        &self,
        transport: &mut T,
        op: BdevOp,
        id: i32,
        access: i32,
    ) -> i32 {
        let mut link = Link::new(transport);
        // The forward targets the lower driver's own minor
        // (`driver_minor`, `fbd.c:146`), not the caller's.
        let destination = Destination::control(
            self.pump.face().downstream(),
            op.message_type(),
            self.pump.face().minor(),
            id,
        )
        .with_access(access);
        minix_bdev::client::check_reply(link.exchange(destination), id)
    }

    /// The three rule controls (`rule_ctl`, `rule.c:15-120`); `None` means
    /// "not ours".
    fn rule_control<T: DriverTransport>(
        &mut self,
        transport: &mut T,
        msg: &Message,
        request: u64,
    ) -> Option<i32> {
        let caller = msg.m_source;
        let grant = wire::read_i32(msg, wire::request::GRANT);
        match request {
            FBDCADDRULE => {
                let mut raw = [0u8; RULE_SIZE];
                if let Err(status) = transport.copy_from_grant(caller, grant, 0, &mut raw) {
                    return Some(status);
                }
                let rule = match decode_rule(&raw) {
                    Ok(rule) => rule,
                    Err(status) => return Some(status),
                };
                Some(match self.pump.face_mut().add_rule(rule) {
                    Ok(number) => number,
                    Err(status) => status,
                })
            }
            FBDCDELRULE => {
                let mut raw = [0u8; 4];
                if let Err(status) = transport.copy_from_grant(caller, grant, 0, &mut raw) {
                    return Some(status);
                }
                let number = i32::from_le_bytes(raw);
                Some(match self.pump.face_mut().delete_rule(number) {
                    Ok(()) => 0,
                    Err(status) => status,
                })
            }
            FBDCGETRULE => {
                // The caller names the rule to fetch.
                let mut raw = [0u8; 4];
                if let Err(status) = transport.copy_from_grant(caller, grant, 0, &mut raw) {
                    return Some(status);
                }
                let number = i32::from_le_bytes(raw);
                match self.pump.face().get_rule(number) {
                    Ok(rule) => {
                        let encoded = encode_rule(number, &rule);
                        Some(match transport.copy_to_grant(caller, grant, 0, &encoded) {
                            Ok(()) => 0,
                            Err(status) => status,
                        })
                    }
                    Err(status) => Some(status),
                }
            }
            _ => None,
        }
    }

    /// Answer one forwarded request (`BDEV_REPLY` with the status and id).
    fn reply<T: DriverTransport>(
        &mut self,
        transport: &mut T,
        caller: Endpoint,
        status: i32,
        id: i32,
    ) {
        if status == EDONTREPLY {
            return;
        }
        let mut reply = Message {
            m_type: BDEV_REPLY,
            ..Message::default()
        };
        wire::write_i32(&mut reply, wire::reply::STATUS, status);
        wire::write_i32(&mut reply, wire::reply::ID, id);
        //[SD19-gate] 回复发送尽力而为：C 的 send_panic 失败即 panic；单线程泵按文档丢弃而不是卡死。
        #[allow(clippy::let_underscore_must_use)]
        let _ = transport.asynsend(caller, &mut reply);
    }
}

impl<T: DriverTransport> DriverHandler<T> for FbdService {
    fn handle(&mut self, transport: &mut T, msg: &Message) {
        self.dispatch(transport, msg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{ACTION_ERROR, FLAG_READ, wire as rule_wire};
    use crate::rules::FaultAction;
    use alloc::vec;
    use alloc::vec::Vec;

    /// A transport that serves the caller's grants and records replies.
    struct FakeTransport {
        grants: Vec<u8>,
        copied_out: Vec<Vec<u8>>,
        replies: Vec<(Endpoint, i32, i32, i32)>,
        /// Downstream requests: message type, minor, access.
        forwarded: Vec<(i32, u32, i32)>,
        downstream_status: i32,
    }

    impl FakeTransport {
        fn new(grants: Vec<u8>) -> Self {
            FakeTransport {
                grants,
                copied_out: Vec::new(),
                replies: Vec::new(),
                forwarded: Vec::new(),
                downstream_status: 0,
            }
        }

        fn last_copy(&self) -> &[u8] {
            self.copied_out.last().expect("a copy happened")
        }
    }

    impl DriverTransport for FakeTransport {
        fn receive(&mut self, _msg: &mut Message) -> Result<(), i32> {
            Err(-minix_types::EIO)
        }
        fn send(&mut self, _dst: Endpoint, _msg: &mut Message) -> Result<(), i32> {
            Ok(())
        }
        fn send_rec(&mut self, _dst: Endpoint, msg: &mut Message) -> Result<(), i32> {
            self.forwarded.push((
                msg.m_type,
                wire::read_u32(msg, wire::request::MINOR),
                wire::read_i32(msg, wire::request::ACCESS),
            ));
            let id = wire::read_i32(msg, wire::request::ID);
            let count = wire::read_i32(msg, wire::request::COUNT);
            msg.m_type = minix_bdev::transport::BDEV_REPLY;
            // A transfer reply carries the byte count; a scripted non-zero
            // status stands in for an error.
            let status = if self.downstream_status != 0 {
                -self.downstream_status
            } else {
                count
            };
            wire::write_i32(msg, wire::reply::STATUS, status);
            wire::write_i32(msg, wire::reply::ID, id);
            Ok(())
        }
        fn asynsend(&mut self, dst: Endpoint, msg: &mut Message) -> Result<(), i32> {
            self.replies.push((
                dst,
                msg.m_type,
                wire::read_i32(msg, wire::reply::STATUS),
                wire::read_i32(msg, wire::reply::ID),
            ));
            Ok(())
        }
        fn copy_from_grant(
            &mut self,
            _granter: Endpoint,
            _grant: i32,
            _offset: u64,
            buf: &mut [u8],
        ) -> Result<(), i32> {
            buf.copy_from_slice(&self.grants[..buf.len()]);
            Ok(())
        }
        fn copy_to_grant(
            &mut self,
            _granter: Endpoint,
            _grant: i32,
            _offset: u64,
            buf: &[u8],
        ) -> Result<(), i32> {
            self.copied_out.push(buf.to_vec());
            Ok(())
        }
        fn publish_label(&mut self, _label: &str) -> Result<(), i32> {
            Ok(())
        }
        fn grant_direct(
            &mut self,
            _dest: Endpoint,
            _address: u64,
            _len: usize,
            _write: bool,
        ) -> Result<i32, i32> {
            Ok(5)
        }
    }

    fn service() -> FbdService {
        let mut face = FbdFace::new(3);
        face.set_downstream(6);
        FbdService::new(face)
    }

    fn ioctl_message(request: u64, grant: i32, id: i32) -> Message {
        let mut msg = Message {
            m_type: BdevRequest::Ioctl.message_type(),
            m_source: Endpoint(20),
            ..Message::default()
        };
        wire::write_u64(&mut msg, wire::request::REQUEST, request);
        wire::write_i32(&mut msg, wire::request::GRANT, grant);
        wire::write_i32(&mut msg, wire::request::ID, id);
        msg
    }

    #[test]
    fn test_rule_controls_add_get_and_delete() {
        let mut svc = service();
        // A rule with the error action over [0x100, 0x200).
        let mut raw = [0u8; RULE_SIZE];
        raw[rule_wire::START..rule_wire::START + 8].copy_from_slice(&0x100u64.to_le_bytes());
        raw[rule_wire::END..rule_wire::END + 8].copy_from_slice(&0x200u64.to_le_bytes());
        raw[rule_wire::FLAGS..rule_wire::FLAGS + 4].copy_from_slice(&FLAG_READ.to_le_bytes());
        raw[rule_wire::ACTION..rule_wire::ACTION + 4]
            .copy_from_slice(&(ACTION_ERROR as u32).to_le_bytes());
        raw[rule_wire::ERROR_CODE..rule_wire::ERROR_CODE + 4]
            .copy_from_slice(&(minix_types::EIO as u32).to_le_bytes());

        let mut transport = FakeTransport::new(raw.to_vec());
        svc.dispatch(&mut transport, &ioctl_message(FBDCADDRULE, 5, 1));
        // The reply carries the rule's 1-based number as the status.
        assert_eq!(transport.replies.last().unwrap().2, 1);
        assert_eq!(svc.face().rule_count(), 1);

        // GET copies the rule back out.
        let mut transport = FakeTransport::new(1i32.to_le_bytes().to_vec());
        svc.dispatch(&mut transport, &ioctl_message(FBDCGETRULE, 5, 2));
        assert_eq!(transport.replies.last().unwrap().2, 0);
        assert_eq!(transport.last_copy().len(), RULE_SIZE);
        assert_eq!(
            u64::from_le_bytes(
                transport.last_copy()[rule_wire::START..rule_wire::START + 8]
                    .try_into()
                    .unwrap()
            ),
            0x100
        );
        assert_eq!(
            i32::from_le_bytes(
                transport.last_copy()[rule_wire::NUM..rule_wire::NUM + 4]
                    .try_into()
                    .unwrap()
            ),
            1
        );

        // DELETE removes it, and a second delete reports ENOENT.
        let mut transport = FakeTransport::new(1i32.to_le_bytes().to_vec());
        svc.dispatch(&mut transport, &ioctl_message(FBDCDELRULE, 5, 3));
        assert_eq!(transport.replies.last().unwrap().2, 0);
        assert_eq!(svc.face().rule_count(), 0);
        svc.dispatch(&mut transport, &ioctl_message(FBDCDELRULE, 5, 4));
        assert_eq!(transport.replies.last().unwrap().2, minix_types::ENOENT);
    }

    #[test]
    fn test_open_and_close_are_forwarded_downstream() {
        let mut svc = service();
        let mut transport = FakeTransport::new(Vec::new());
        let mut msg = Message {
            m_type: BdevRequest::Open.message_type(),
            m_source: Endpoint(20),
            ..Message::default()
        };
        wire::write_u32(&mut msg, wire::request::MINOR, 0);
        wire::write_i32(&mut msg, wire::request::ACCESS, 2);
        wire::write_i32(&mut msg, wire::request::ID, 9);
        svc.dispatch(&mut transport, &msg);
        assert_eq!(transport.replies.last().unwrap().2, 0);
        assert_eq!(
            transport.forwarded[0],
            (BdevRequest::Open.message_type(), 3, 2),
            "the open reached the lower driver's minor with its access mode"
        );

        let mut msg = Message {
            m_type: BdevRequest::Close.message_type(),
            m_source: Endpoint(20),
            ..Message::default()
        };
        wire::write_u32(&mut msg, wire::request::MINOR, 0);
        wire::write_i32(&mut msg, wire::request::ID, 10);
        svc.dispatch(&mut transport, &msg);
        assert_eq!(transport.replies.last().unwrap().2, 0);
        assert_eq!(transport.forwarded[1].0, BdevRequest::Close.message_type());
    }

    #[test]
    fn test_a_foreign_control_falls_through_to_the_pump() {
        let mut svc = service();
        let mut transport = FakeTransport::new(Vec::new());
        // The device must be open: the pump's restart gate answers
        // ERESTART to a control on a device nobody opened.
        let mut open = Message {
            m_type: BdevRequest::Open.message_type(),
            m_source: Endpoint(20),
            ..Message::default()
        };
        wire::write_u32(&mut open, wire::request::MINOR, 0);
        wire::write_i32(&mut open, wire::request::ID, 1);
        svc.dispatch(&mut transport, &open);
        svc.dispatch(&mut transport, &ioctl_message(0x1234, 5, 2));
        assert_eq!(transport.replies.last().unwrap().2, minix_types::ENOTTY);
    }

    #[test]
    fn test_a_transfer_reaches_the_pump_and_the_face_rules_apply() {
        let mut svc = service();
        // Open through the pump first so the restart gate passes.
        let mut transport = FakeTransport::new(Vec::new());
        let mut open = Message {
            m_type: BdevRequest::Open.message_type(),
            m_source: Endpoint(20),
            ..Message::default()
        };
        wire::write_u32(&mut open, wire::request::MINOR, 0);
        wire::write_i32(&mut open, wire::request::ID, 1);
        svc.dispatch(&mut transport, &open);
        assert_eq!(svc.face().rule_count(), 0);
        // A read with no rules forwards the whole request.
        let mut read = Message {
            m_type: BdevRequest::Read.message_type(),
            m_source: Endpoint(20),
            ..Message::default()
        };
        wire::write_u32(&mut read, wire::request::MINOR, 0);
        wire::write_i32(&mut read, wire::request::ID, 2);
        wire::write_u64(&mut read, wire::request::POS, 0x40);
        wire::write_i32(&mut read, wire::request::COUNT, 16);
        wire::write_i32(&mut read, wire::request::GRANT, 5);
        svc.dispatch(&mut transport, &read);
        let last = transport.replies.last().unwrap();
        assert_eq!((last.1, last.2), (BDEV_REPLY, 16));
        assert_eq!(transport.last_copy().len(), 16);
        let _ = FaultAction::Error(0);
        let _ = vec![0u8; 0];
    }
}
