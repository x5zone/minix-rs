//! Service assembly: the five driver requests and the daemon's answers.
//!
//! C correspondence: the server dispatch (`usb_server.c:731-758`: the five
//! `USB_RQ_*` requests) and the daemon's own loop (`base/usbd.c`). The
//! loop shell and the RS birth handshake live in the shared driver
//! runtime; the controller traffic sits behind [`HcdLeg`].

use minix_driver_rt::runtime::DriverHandler;
use minix_driver_rt::transport::DriverTransport;
use minix_types::{Message, OK};

use crate::char_face::UsbdFace;
use crate::hcd::HcdLeg;
use crate::protocol::{DaemonMessage, DriverRequest, decode_driver_request};

/// The notification family shares the 0x1000 base with the source in the
/// low byte (C `is_ipc_notify`, `const.h`).
const fn is_ipc_notify(message_type: i32) -> bool {
    (message_type & !0xff) == 0x1000
}

/// The USB host daemon service.
pub struct UsbdService<H: HcdLeg> {
    face: UsbdFace,
    hcd: H,
    /// Drivers registered with the daemon (`USB_RQ_INIT`).
    registered: alloc::vec::Vec<i32>,
}

impl<H: HcdLeg> UsbdService<H> {
    /// A service over a fresh face and the given controller seam.
    pub fn new(hcd: H) -> Self {
        UsbdService {
            face: UsbdFace::new(),
            hcd,
            registered: alloc::vec::Vec::new(),
        }
    }

    /// The face (tests and the binary's diagnostics).
    pub fn face(&self) -> &UsbdFace {
        &self.face
    }

    /// The controller seam, mutably.
    pub fn hcd_mut(&mut self) -> &mut H {
        &mut self.hcd
    }

    /// The drivers registered so far.
    pub fn registered(&self) -> &[i32] {
        &self.registered
    }

    /// Handle one delivered message.
    pub fn dispatch<T: DriverTransport>(&mut self, transport: &mut T, msg: &Message) {
        if is_ipc_notify(msg.m_type) {
            // The controller's interrupt path is the seam's business; the
            // daemon has nothing to do here yet.
            return;
        }
        let Some(request) = decode_driver_request(msg.m_type as u32) else {
            return;
        };
        let status = match request {
            DriverRequest::Init => {
                if !self.registered.contains(&msg.m_source.0) {
                    self.registered.push(msg.m_source.0);
                }
                OK
            }
            DriverRequest::Deinit => {
                self.registered
                    .retain(|endpoint| *endpoint != msg.m_source.0);
                OK
            }
            DriverRequest::SendUrb => {
                // The request block's roster entry is registered; the
                // transfer itself goes to the controller seam, whose
                // unbound default refuses rather than completing.
                let mut request_msg = *msg;
                let id = self.face.submit_bulk_urb(&mut request_msg);
                match self.hcd.bulk_transfer(1, &mut [0u8; 0], false) {
                    Ok(_) => {
                        self.face.pending_remove(id);
                        OK
                    }
                    Err(status) => {
                        self.face.pending_remove(id);
                        status
                    }
                }
            }
            DriverRequest::CancelUrb => OK,
            DriverRequest::SendInfo => OK,
        };
        self.reply(transport, msg.m_source, status);
    }

    /// Answer one driver request with the daemon's general reply.
    fn reply<T: DriverTransport>(
        &mut self,
        transport: &mut T,
        caller: minix_types::Endpoint,
        status: i32,
    ) {
        let mut reply = Message {
            m_type: DaemonMessage::Reply as i32,
            ..Message::default()
        };
        let base = ::core::ptr::addr_of_mut!(reply) as *mut u8;
        // SAFETY: the reply's documented status slot (`com.h:829-840`).
        unsafe {
            ::core::ptr::copy_nonoverlapping(status.to_le_bytes().as_ptr(), base.add(8), 4);
        }
        //[SD19-gate] reply sending is best effort: C panics when send fails; the single-threaded pump drops it as documented instead of hanging.
        #[allow(clippy::let_underscore_must_use)]
        let _ = transport.asynsend(caller, &mut reply);
    }
}

impl<H: HcdLeg, T: DriverTransport> DriverHandler<T> for UsbdService<H> {
    fn handle(&mut self, transport: &mut T, msg: &Message) {
        self.dispatch(transport, msg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hcd::UnboundHcd;
    use alloc::vec::Vec;
    use minix_types::Endpoint;

    struct FakeTransport {
        replies: Vec<(Endpoint, i32, i32)>,
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
            let base = ::core::ptr::addr_of!(*msg) as *const u8;
            let mut bytes = [0u8; 4];
            // SAFETY: the reply's status slot.
            unsafe { ::core::ptr::copy_nonoverlapping(base.add(8), bytes.as_mut_ptr(), 4) };
            self.replies
                .push((dst, msg.m_type, i32::from_le_bytes(bytes)));
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
    }

    fn request(kind: DriverRequest, caller: Endpoint) -> Message {
        Message {
            m_type: kind as i32,
            m_source: caller,
            ..Message::default()
        }
    }

    #[test]
    fn test_register_unregister_and_the_urb_refusal() {
        let mut svc = UsbdService::new(UnboundHcd);
        let mut transport = FakeTransport {
            replies: Vec::new(),
        };
        svc.dispatch(&mut transport, &request(DriverRequest::Init, Endpoint(20)));
        assert_eq!(svc.registered(), &[20]);
        assert_eq!(transport.replies.last().unwrap().2, OK);

        // A URB submission reaches the controller seam, which refuses; the
        // roster entry is cleaned up and the caller sees the refusal.
        svc.dispatch(
            &mut transport,
            &request(DriverRequest::SendUrb, Endpoint(20)),
        );
        let last = transport.replies.last().unwrap();
        assert_eq!(last.1, DaemonMessage::Reply as i32);
        assert_eq!(last.2, -(minix_types::ENOSYS));
        assert!(
            svc.face().pending.is_empty(),
            "no request block left behind"
        );

        svc.dispatch(
            &mut transport,
            &request(DriverRequest::Deinit, Endpoint(20)),
        );
        assert!(svc.registered().is_empty());
    }
}
