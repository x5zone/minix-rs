//! Driver-side links: the block *client* leg a driver uses to forward
//! requests downstream.
//!
//! C correspondence: `minix3/minix/drivers/storage/fbd/fbd.c`'s three
//! forwarding bodies — `fbd_open`/`fbd_close` (`fbd.c:139-182`: build a
//! `BDEV_OPEN`/`BDEV_CLOSE` message, `ipc_sendrec` to the real driver,
//! require a `BDEV_REPLY` back) and the transfer forwarder
//! (`fbd_transfer_copy`, `fbd.c:268-...`: `ipc_sendrec` with a grant on the
//! staging buffer). The C driver does this with raw message code; the
//! rewrite builds the same nine-lane request out of a [`Destination`] and
//! runs it over the driver's own transport.
//!
//! Two carriers, because a proxy reaches IPC two ways: [`Link`] over the
//! driver's [`DriverTransport`] (the service's control requests), and
//! [`PlaneLink`] over a transfer's [`TransferPlane`] (the data plane a
//! transfer hook holds). Both implement [`Transport`] and [`GrantIssuer`],
//! so one forwarding leg serves both call sites.

use minix_blockdriver::wire;
use minix_driver_rt::plane::TransferPlane;
use minix_driver_rt::transport::DriverTransport;
use minix_types::{Endpoint, Message};

use crate::client::BdevOp;
use crate::transport::{BDEV_REPLY, Destination, GrantIssuer, Reply, Transport, TransportError};

/// The block request's message type for a destination.
fn message_for(destination: &Destination) -> Message {
    let mut message = Message {
        m_type: destination.message_type,
        ..Message::default()
    };
    // The nine lanes of `mess_lbdev_lblockdriver_msg`, at the offsets the
    // block framework reads (`minix_blockdriver::wire`).
    wire::write_u64(&mut message, wire::request::POS, destination.position);
    wire::write_u32(&mut message, wire::request::MINOR, destination.minor);
    wire::write_i32(&mut message, wire::request::ID, destination.id);
    wire::write_i32(&mut message, wire::request::ACCESS, destination.access);
    wire::write_i32(&mut message, wire::request::COUNT, destination.bytes as i32);
    wire::write_i32(&mut message, wire::request::GRANT, destination.grant);
    wire::write_i32(&mut message, wire::request::FLAGS, destination.flags);
    wire::write_i32(&mut message, wire::request::USER, destination.user);
    wire::write_u64(&mut message, wire::request::REQUEST, destination.request);
    message
}

/// Validate a reply the way the client does: the block general reply, the
/// identifier echoed back, and the status in its lane.
fn reply_from(message: &Message) -> Result<Reply, TransportError> {
    if message.m_type != BDEV_REPLY {
        return Err(TransportError::BadReply);
    }
    Ok(Reply {
        message_type: message.m_type,
        id: wire::read_i32(message, wire::reply::ID),
        status: wire::read_i32(message, wire::reply::STATUS),
    })
}

/// The forwarding leg over the driver's own transport.
pub struct Link<'a, T: DriverTransport> {
    transport: &'a mut T,
}

impl<'a, T: DriverTransport> Link<'a, T> {
    /// A link that forwards over `transport`.
    pub fn new(transport: &'a mut T) -> Self {
        Link { transport }
    }
}

impl<T: DriverTransport> Transport for Link<'_, T> {
    fn exchange(&mut self, destination: Destination) -> Result<Reply, TransportError> {
        let mut message = message_for(&destination);
        self.transport
            .send_rec(Endpoint(destination.endpoint), &mut message)
            .map_err(|_| TransportError::SendFailed)?;
        reply_from(&message)
    }
}

impl<T: DriverTransport> GrantIssuer for Link<'_, T> {
    fn issue_grant(&mut self, endpoint: i32, address: u64, bytes: u32, write: bool) -> i32 {
        self.transport
            .grant_direct(Endpoint(endpoint), address, bytes as usize, write)
            .unwrap_or(-1)
    }
}

/// The forwarding leg over a transfer's data plane.
///
/// A transfer hook holds no transport, but its plane carries exactly the
/// two primitives the downstream request needs (send-and-receive, and a
/// grant over the driver's staging buffer), so a proxy forwards from inside
/// its own transfer.
pub struct PlaneLink<'a> {
    plane: &'a mut dyn TransferPlane,
}

impl<'a> PlaneLink<'a> {
    /// A link that forwards over `plane`.
    pub fn new(plane: &'a mut dyn TransferPlane) -> Self {
        PlaneLink { plane }
    }
}

impl Transport for PlaneLink<'_> {
    fn exchange(&mut self, destination: Destination) -> Result<Reply, TransportError> {
        let mut message = message_for(&destination);
        self.plane
            .send_rec(Endpoint(destination.endpoint), &mut message)
            .map_err(|_| TransportError::SendFailed)?;
        reply_from(&message)
    }
}

impl GrantIssuer for PlaneLink<'_> {
    fn issue_grant(&mut self, endpoint: i32, address: u64, bytes: u32, write: bool) -> i32 {
        self.plane
            .grant_direct(Endpoint(endpoint), address, bytes as usize, write)
            .unwrap_or(-1)
    }
}

/// The request type of one forwarded transfer: `BDEV_READ`/`BDEV_WRITE`
/// (a proxy sends contiguous requests, see the crate's module note on the
/// direct/vectored forms).
pub const fn transfer_type(write: bool) -> i32 {
    if write {
        BdevOp::Write.message_type()
    } else {
        BdevOp::Read.message_type()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;
    use core::cell::RefCell;

    /// A transport that records the request and answers with a scripted
    /// reply.
    struct Recorder {
        requests: RefCell<Vec<Message>>,
        destinations: RefCell<Vec<Endpoint>>,
        reply: RefCell<Option<Message>>,
        grants: RefCell<Vec<(Endpoint, u64, usize, bool)>>,
    }

    impl Recorder {
        fn new(reply: Message) -> Self {
            Recorder {
                requests: RefCell::new(Vec::new()),
                destinations: RefCell::new(Vec::new()),
                reply: RefCell::new(Some(reply)),
                grants: RefCell::new(Vec::new()),
            }
        }
    }

    impl DriverTransport for Recorder {
        fn receive(&mut self, _msg: &mut Message) -> Result<(), i32> {
            Err(-minix_types::EIO)
        }
        fn send(&mut self, _dst: Endpoint, _msg: &mut Message) -> Result<(), i32> {
            Ok(())
        }
        fn send_rec(&mut self, dst: Endpoint, msg: &mut Message) -> Result<(), i32> {
            self.destinations.borrow_mut().push(dst);
            self.requests.borrow_mut().push(*msg);
            if let Some(reply) = self.reply.borrow_mut().take() {
                *msg = reply;
            }
            Ok(())
        }
        fn asynsend(&mut self, _dst: Endpoint, _msg: &mut Message) -> Result<(), i32> {
            Ok(())
        }
        fn copy_from_grant(
            &mut self,
            _granter: Endpoint,
            _grant: i32,
            _offset: u64,
            _buf: &mut [u8],
        ) -> Result<(), i32> {
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
        fn grant_direct(
            &mut self,
            dest: Endpoint,
            address: u64,
            len: usize,
            write: bool,
        ) -> Result<i32, i32> {
            self.grants.borrow_mut().push((dest, address, len, write));
            Ok(9)
        }
    }

    fn reply(status: i32, id: i32) -> Message {
        let mut message = Message {
            m_type: BDEV_REPLY,
            ..Message::default()
        };
        wire::write_i32(&mut message, wire::reply::STATUS, status);
        wire::write_i32(&mut message, wire::reply::ID, id);
        message
    }

    #[test]
    fn test_link_fills_all_nine_lanes_and_reads_the_reply() {
        let mut transport = Recorder::new(reply(0, 7));
        let mut link = Link::new(&mut transport);
        // (endpoint, message_type, minor, id, position, bytes, grant)
        let destination = Destination::data(42, BdevOp::Read.message_type(), 1, 7, 0x2000, 512, 5);
        let outcome = link.exchange(destination);
        assert_eq!(
            outcome.map(|r| (r.message_type, r.id, r.status)),
            Ok((BDEV_REPLY, 7, 0))
        );
        let sent = transport.requests.borrow()[0];
        assert_eq!(transport.destinations.borrow()[0], Endpoint(42));
        assert_eq!(sent.m_type, BdevOp::Read.message_type());
        assert_eq!(wire::read_u64(&sent, wire::request::POS), 0x2000);
        assert_eq!(wire::read_u32(&sent, wire::request::MINOR), 1);
        assert_eq!(wire::read_i32(&sent, wire::request::ID), 7);
        assert_eq!(wire::read_i32(&sent, wire::request::COUNT), 512);
        assert_eq!(wire::read_i32(&sent, wire::request::GRANT), 5);
    }

    #[test]
    fn test_link_forwards_the_control_lanes_and_refuses_bad_replies() {
        // An ioctl destination carries request/grant/user.
        let mut transport = Recorder::new(reply(0, 0));
        let mut link = Link::new(&mut transport);
        let destination = Destination::control(6, BdevOp::Ioctl.message_type(), 2, 0).as_ioctl(
            0xC020_4600,
            11,
            21,
        );
        assert!(link.exchange(destination).is_ok());
        let sent = transport.requests.borrow()[0];
        assert_eq!(wire::read_u64(&sent, wire::request::REQUEST), 0xC020_4600);
        assert_eq!(wire::read_i32(&sent, wire::request::GRANT), 11);
        assert_eq!(wire::read_i32(&sent, wire::request::USER), 21);

        // A reply of the wrong type is a bad reply, not a status.
        let mut wrong = Recorder::new(Message {
            m_type: 0,
            ..Message::default()
        });
        let mut link = Link::new(&mut wrong);
        assert_eq!(
            link.exchange(Destination::control(6, BdevOp::Open.message_type(), 0, 0)),
            Err(TransportError::BadReply)
        );

        // The grant issuer hands the address and direction to the transport.
        let mut transport = Recorder::new(reply(0, 0));
        let mut link = Link::new(&mut transport);
        assert_eq!(link.issue_grant(6, 0x3000, 64, true), 9);
        assert_eq!(
            transport.grants.borrow()[0],
            (Endpoint(6), 0x3000, 64, true)
        );
    }

    #[test]
    fn test_plane_link_forwards_from_a_transfer_hook() {
        use minix_driver_rt::plane::Plane;

        let mut transport = Recorder::new(reply(0, 0));
        {
            let mut plane = Plane::new(&mut transport, Endpoint(20), 5);
            let mut link = PlaneLink::new(&mut plane);
            assert!(
                link.exchange(Destination::control(6, BdevOp::Close.message_type(), 3, 0))
                    .is_ok()
            );
            assert_eq!(link.issue_grant(6, 0x4000, 128, false), 9);
            // The plane's own caller is untouched by the forwarded request.
            assert_eq!(plane.caller(), Endpoint(20));
        }
        let sent = transport.requests.borrow()[0];
        assert_eq!(sent.m_type, BdevOp::Close.message_type());
        assert_eq!(wire::read_u32(&sent, wire::request::MINOR), 3);
        assert_eq!(
            transport.grants.borrow()[0],
            (Endpoint(6), 0x4000, 128, false)
        );
    }

    #[test]
    fn test_transfer_type_selects_the_contiguous_pair() {
        assert_eq!(transfer_type(false), BdevOp::Read.message_type());
        assert_eq!(transfer_type(true), BdevOp::Write.message_type());
    }
}
