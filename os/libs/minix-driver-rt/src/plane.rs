//! The transfer data plane: what a read/write hook may do with the
//! caller's bytes.
//!
//! C correspondence: the two facts `bdr_transfer`/`cdr_read`/`cdr_write`
//! receive besides the request itself — the caller's endpoint and the
//! grant the caller lent (`iov->iov_addr` / the `grant` parameter) — plus
//! the calls the hook issues itself in C: `sys_safecopyfrom`/
//! `sys_safecopyto` for the bytes, `cpf_grant_direct`/`cpf_revoke` and
//! `ipc_sendrec` when the bytes have to travel through another server
//! first (an i2c bus driver, a downstream block driver).
//!
//! Why a seam and not the transport itself: the hooks' semantics are
//! "move the caller's bytes", and the transport's surface is wider than
//! that (labels, signals, clocks, delays). The narrower trait is also what
//! makes a hook testable without an IPC world: a test implements
//! [`TransferPlane`] on a scratch object and asserts the exact copies.
//!
//! Both frameworks that have read/write hooks (block and character) face
//! the same gap; the plane is defined here, next to the transport, so one
//! definition serves both.

use minix_types::{Endpoint, Message};

use crate::transport::DriverTransport;

/// The grant-and-IPC surface a device leg needs to hand a local buffer to
/// another server (C: `cpf_grant_direct` + `ipc_sendrec` + `cpf_revoke`).
///
/// Implemented for every [`DriverTransport`] by the blanket impl below, so
/// device-leg helpers written once accept either a plain transport (a
/// service's own IPC face) or a transfer's plane (a hook's data face).
pub trait GrantIpc {
    /// Allocate a direct grant over this process's own memory
    /// (`cpf_grant_direct`).
    fn grant_direct(
        &mut self,
        dest: Endpoint,
        address: u64,
        len: usize,
        write: bool,
    ) -> Result<i32, i32>;

    /// Release a grant (`cpf_revoke`).
    fn grant_revoke(&mut self, grant: i32) -> Result<(), i32>;

    /// Send-and-receive (`ipc_sendrec`).
    fn send_rec(&mut self, dst: Endpoint, msg: &mut Message) -> Result<(), i32>;
}

impl<T: DriverTransport + ?Sized> GrantIpc for T {
    fn grant_direct(
        &mut self,
        dest: Endpoint,
        address: u64,
        len: usize,
        write: bool,
    ) -> Result<i32, i32> {
        DriverTransport::grant_direct(self, dest, address, len, write)
    }

    fn grant_revoke(&mut self, grant: i32) -> Result<(), i32> {
        DriverTransport::grant_revoke(self, grant)
    }

    fn send_rec(&mut self, dst: Endpoint, msg: &mut Message) -> Result<(), i32> {
        DriverTransport::send_rec(self, dst, msg)
    }
}

/// One request's data plane: who asked, which grant they lent, and the
/// byte movements (`sys_safecopyfrom`/`sys_safecopyto`) the hook used to
/// issue itself.
pub trait TransferPlane: GrantIpc {
    /// The caller (`endpt`): a proxy driver names it when forwarding.
    fn caller(&self) -> Endpoint;

    /// Copy bytes out of the caller's grant into `buf`
    /// (`sys_safecopyfrom`): a write request pulls its payload this way.
    fn copy_from_caller(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), i32>;

    /// Copy bytes from `buf` into the caller's grant (`sys_safecopyto`):
    /// a read request delivers its answer this way.
    fn copy_to_caller(&mut self, offset: u64, buf: &[u8]) -> Result<(), i32>;
}

/// The plane over a real transport: the caller, the grant, and the
/// transport that performs the copies.
pub struct Plane<'a, T: DriverTransport + ?Sized> {
    transport: &'a mut T,
    caller: Endpoint,
    grant: i32,
}

impl<'a, T: DriverTransport + ?Sized> Plane<'a, T> {
    /// The plane for one request: `caller` asked, `grant` is the grant
    /// they lent (the block request's `GRANT` field, the character
    /// request's `grant` parameter).
    pub fn new(transport: &'a mut T, caller: Endpoint, grant: i32) -> Self {
        Plane {
            transport,
            caller,
            grant,
        }
    }

    /// The grant id (C's `iov->iov_addr`), for a driver that forwards the
    /// caller's buffer instead of copying it.
    pub fn grant(&self) -> i32 {
        self.grant
    }
}

impl<T: DriverTransport + ?Sized> GrantIpc for Plane<'_, T> {
    fn grant_direct(
        &mut self,
        dest: Endpoint,
        address: u64,
        len: usize,
        write: bool,
    ) -> Result<i32, i32> {
        self.transport.grant_direct(dest, address, len, write)
    }

    fn grant_revoke(&mut self, grant: i32) -> Result<(), i32> {
        self.transport.grant_revoke(grant)
    }

    fn send_rec(&mut self, dst: Endpoint, msg: &mut Message) -> Result<(), i32> {
        self.transport.send_rec(dst, msg)
    }
}

impl<T: DriverTransport + ?Sized> TransferPlane for Plane<'_, T> {
    fn caller(&self) -> Endpoint {
        self.caller
    }

    fn copy_from_caller(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), i32> {
        self.transport
            .copy_from_grant(self.caller, self.grant, offset, buf)
    }

    fn copy_to_caller(&mut self, offset: u64, buf: &[u8]) -> Result<(), i32> {
        self.transport
            .copy_to_grant(self.caller, self.grant, offset, buf)
    }
}

/// A plane whose data leg is not there: every request refuses with
/// `ENOSYS`.
///
/// For tests of hooks that exercise their count or control policy rather
/// than the byte movement (a hook that ignores the plane cannot silently
/// "succeed" at copying), and for a driver whose data leg is a registered
/// seam: it refuses instead of fabricating a transfer.
pub struct NullPlane;

impl GrantIpc for NullPlane {
    fn grant_direct(
        &mut self,
        _dest: Endpoint,
        _address: u64,
        _len: usize,
        _write: bool,
    ) -> Result<i32, i32> {
        Err(-minix_types::ENOSYS)
    }

    fn grant_revoke(&mut self, _grant: i32) -> Result<(), i32> {
        Err(-minix_types::ENOSYS)
    }

    fn send_rec(&mut self, _dst: Endpoint, _msg: &mut Message) -> Result<(), i32> {
        Err(-minix_types::ENOSYS)
    }
}

impl TransferPlane for NullPlane {
    fn caller(&self) -> Endpoint {
        Endpoint::NONE
    }

    fn copy_from_caller(&mut self, _offset: u64, _buf: &mut [u8]) -> Result<(), i32> {
        Err(-minix_types::ENOSYS)
    }

    fn copy_to_caller(&mut self, _offset: u64, _buf: &[u8]) -> Result<(), i32> {
        Err(-minix_types::ENOSYS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use alloc::vec::Vec;

    /// A transport that records what the plane routes to it.
    struct Recorder {
        from_bytes: Vec<u8>,
        to_bytes: Vec<u8>,
        calls: Vec<(Endpoint, i32, u64, usize)>,
        granted: Vec<(Endpoint, u64, usize, bool)>,
        revoked: Vec<i32>,
        sent: Vec<Endpoint>,
    }

    impl Recorder {
        fn new() -> Self {
            Recorder {
                from_bytes: Vec::new(),
                to_bytes: Vec::new(),
                calls: Vec::new(),
                granted: Vec::new(),
                revoked: Vec::new(),
                sent: Vec::new(),
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
        fn send_rec(&mut self, dst: Endpoint, _msg: &mut Message) -> Result<(), i32> {
            self.sent.push(dst);
            Ok(())
        }
        fn asynsend(&mut self, _dst: Endpoint, _msg: &mut Message) -> Result<(), i32> {
            Ok(())
        }
        fn copy_from_grant(
            &mut self,
            granter: Endpoint,
            grant: i32,
            offset: u64,
            buf: &mut [u8],
        ) -> Result<(), i32> {
            self.calls.push((granter, grant, offset, buf.len()));
            buf.copy_from_slice(&self.from_bytes[..buf.len()]);
            Ok(())
        }
        fn copy_to_grant(
            &mut self,
            granter: Endpoint,
            grant: i32,
            offset: u64,
            buf: &[u8],
        ) -> Result<(), i32> {
            self.calls.push((granter, grant, offset, buf.len()));
            self.to_bytes.extend_from_slice(buf);
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
            self.granted.push((dest, address, len, write));
            Ok(7)
        }
        fn grant_revoke(&mut self, grant: i32) -> Result<(), i32> {
            self.revoked.push(grant);
            Ok(())
        }
    }

    #[test]
    fn test_plane_routes_copies_to_the_caller_grant() {
        let mut transport = Recorder::new();
        transport.from_bytes = vec![1, 2, 3, 4];
        {
            let mut plane = Plane::new(&mut transport, Endpoint(20), 42);
            assert_eq!(plane.caller(), Endpoint(20));
            assert_eq!(plane.grant(), 42);
            let mut buf = [0u8; 4];
            assert_eq!(plane.copy_from_caller(0x10, &mut buf), Ok(()));
            assert_eq!(buf, [1, 2, 3, 4]);
            assert_eq!(plane.copy_to_caller(0x20, &[9, 8, 7]), Ok(()));
            // The grant-and-IPC half reaches the same transport.
            assert_eq!(plane.grant_direct(Endpoint(6), 0x1000, 64, true), Ok(7));
            assert_eq!(plane.grant_revoke(7), Ok(()));
            let mut msg = Message::default();
            assert_eq!(plane.send_rec(Endpoint(6), &mut msg), Ok(()));
        }
        assert_eq!(transport.calls.len(), 2);
        assert_eq!(transport.calls[0], (Endpoint(20), 42, 0x10, 4));
        assert_eq!(transport.calls[1], (Endpoint(20), 42, 0x20, 3));
        assert_eq!(transport.to_bytes, vec![9, 8, 7]);
        assert_eq!(transport.granted, vec![(Endpoint(6), 0x1000, 64, true)]);
        assert_eq!(transport.revoked, vec![7]);
        assert_eq!(transport.sent, vec![Endpoint(6)]);
    }

    #[test]
    fn test_null_plane_refuses_instead_of_faking_a_transfer() {
        let mut plane = NullPlane;
        assert_eq!(plane.caller(), Endpoint::NONE);
        let mut buf = [0u8; 4];
        assert_eq!(
            plane.copy_from_caller(0, &mut buf),
            Err(-minix_types::ENOSYS)
        );
        assert_eq!(plane.copy_to_caller(0, &buf), Err(-minix_types::ENOSYS));
        assert_eq!(
            plane.grant_direct(Endpoint(6), 0, 1, false),
            Err(-minix_types::ENOSYS)
        );
        assert_eq!(
            plane.send_rec(Endpoint(6), &mut Message::default()),
            Err(-minix_types::ENOSYS)
        );
    }

    #[test]
    fn test_plane_is_usable_as_a_grant_ipc() {
        // The device legs are written against `GrantIpc`; a plane and a
        // plain transport both satisfy it (the blanket impl), so one leg
        // serves the service's own face and a hook's data face.
        fn leg<T: GrantIpc + ?Sized>(ipc: &mut T) -> Result<i32, i32> {
            ipc.grant_direct(Endpoint(6), 0x2000, 8, false)
        }
        let mut transport = Recorder::new();
        assert_eq!(leg(&mut transport), Ok(7));
        let mut plane = Plane::new(&mut transport, Endpoint(20), 42);
        // The same leg over the plane, as a trait object (the shape a
        // hook receives).
        let as_dyn: &mut dyn TransferPlane = &mut plane;
        assert_eq!(leg(as_dyn), Ok(7));
        assert_eq!(transport.granted.len(), 2);
    }
}
