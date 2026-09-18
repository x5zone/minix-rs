//! Message-transport abstraction for the block-device client.
//!
//! C correspondence: `bdev_senda` and `bdev_sendrec` in
//! `minix3/minix/lib/libbdev/ipc.c:119-268`. The C code calls the kernel
//! message primitives directly, which makes it untestable off-target. This
//! module keeps the retry and recovery policy (which belongs to the
//! library) behind a trait, so tests can substitute a loopback transport
//! while production wires the real kernel primitives in the service crate.
//!
//! Hardware and kernel message details must never leak into the client
//! policy: the transport sees request bytes and returns reply bytes, and
//! nothing else crosses the boundary.

/// Destination of one block request: driver endpoint plus message bytes.
///
/// Endpoints are plain integers here; the service crate translates them to
/// the kernel endpoint type at the boundary.
///
/// The three data lanes (`position`/`bytes`/`grant`) are zero/`NO_GRANT`
/// for control requests (open/close/ioctl) and carry the transfer geometry
/// for reads and writes: the byte offset on the device, the transfer
/// length, and the id of the grant that exposes the caller's buffer to the
/// driver. Grant ids are produced by a [`GrantIssuer`] on the service
/// side; this crate only ever echoes one back to the driver.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Destination {
    /// Driver endpoint handle.
    pub endpoint: i32,
    /// Request message type (one of the `BDEV_*` numbers).
    pub message_type: i32,
    /// Minor device number the request targets.
    pub minor: u32,
    /// Caller-chosen identifier echoed back in the reply.
    pub id: i32,
    /// Byte offset of the transfer on the device (data lanes; else 0).
    pub position: u64,
    /// Transfer length in bytes (data lanes; else 0).
    pub bytes: u32,
    /// Grant id exposing the caller's buffer (data lanes; else
    /// [`NO_GRANT`]).
    pub grant: i32,
}

/// Grant id meaning "no buffer granted" (control requests, and transfers
/// that address memory by other means).
pub const NO_GRANT: i32 = -1;

impl Destination {
    /// A control destination: endpoint, type, minor, id; data lanes zeroed.
    pub const fn control(endpoint: i32, message_type: i32, minor: u32, id: i32) -> Self {
        Destination { endpoint, message_type, minor, id, position: 0, bytes: 0, grant: NO_GRANT }
    }

    /// A data destination for one read/write transfer.
    pub const fn data(
        endpoint: i32,
        message_type: i32,
        minor: u32,
        id: i32,
        position: u64,
        bytes: u32,
        grant: i32,
    ) -> Self {
        Destination { endpoint, message_type, minor, id, position, bytes, grant }
    }
}

impl Default for Destination {
    fn default() -> Self {
        Self::control(0, 0, 0, 0)
    }
}

/// Issues grants that expose caller memory to a block driver.
///
/// The service crate implements this over its grant table (kernel
/// `sys_grant` semantics: one direction, one length, one lifetime per
/// grant). The client library composes the grant *plan* — which buffers,
/// which direction — and calls out here for ids, keeping the kernel
/// boundary behind a seam the tests can double.
pub trait GrantIssuer {
    /// Grants `bytes` starting at `address` to the driver at `endpoint`.
    ///
    /// `write` is the direction *the driver* moves data: `true` for a
    /// write transfer (driver reads the buffer), `false` for a read
    /// transfer (driver writes the buffer) — the same cross the cdev
    /// grant direction uses.
    fn issue_grant(&mut self, endpoint: i32, address: u64, bytes: u32, write: bool) -> i32;
}

/// Reply returned by the transport for one request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reply {
    /// Reply message type (normally the block general reply).
    pub message_type: i32,
    /// Identifier echoed from the request.
    pub id: i32,
    /// Status code carried by the reply.
    pub status: i32,
}

/// The only reply type a block driver sends (`BDEV_REPLY`).
///
/// C: `BDEV_REPLY (BDEV_RS_BASE + 0)` with `BDEV_RS_BASE 0x580`
/// (`com.h:964,979`). Every reply entering the client passes through
/// [`check_reply`]-style validation, which rejects anything else before the
/// identifier is even looked at. Single authority:
/// `minix_types::types::device` (edge E-DEVWIRE).
pub use minix_types::BDEV_REPLY;

/// Transport error: the message never reached the driver or no usable reply
/// came back. Distinct from a reply that arrived carrying an error status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportError {
    /// The send itself failed (driver unreachable).
    SendFailed,
    /// The reply has an unexpected shape (wrong type or identifier).
    BadReply,
    /// The driver restarted mid-call and recovery gave up.
    DriverGone,
}

/// Message-transport behavior for one caller.
///
/// Production implements this with the kernel send primitives; tests use
/// the loopback or recording doubles below. One method covers both the
/// synchronous path (`bdev_sendrec`) and the asynchronous send half
/// (`bdev_senda`): whether the caller blocks is a caller-side decision, not
/// a transport property.
pub trait Transport {
    /// Send one request and return the reply.
    fn exchange(&mut self, destination: Destination) -> Result<Reply, TransportError>;
}

/// Loopback transport for tests: answers every request from a fixed status.
///
/// Models a healthy driver: the reply echoes the request identifier and
/// carries the configured status. A second mode answers "wrong identifier"
/// once, exercising the bad-reply path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoopbackTransport {
    /// Status carried by every reply.
    pub status: i32,
    /// When true, the next reply carries a mismatched identifier.
    pub mismatch_once: bool,
}

impl LoopbackTransport {
    /// Healthy driver answering with this status.
    pub const fn healthy(status: i32) -> LoopbackTransport {
        LoopbackTransport {
            status,
            mismatch_once: false,
        }
    }
}

impl Transport for LoopbackTransport {
    fn exchange(&mut self, destination: Destination) -> Result<Reply, TransportError> {
        if self.mismatch_once {
            self.mismatch_once = false;
            return Ok(Reply {
                message_type: BDEV_REPLY,
                id: destination.id.wrapping_add(1),
                status: self.status,
            });
        }
        Ok(Reply {
            message_type: BDEV_REPLY,
            id: destination.id,
            status: self.status,
        })
    }
}

/// Recording transport for tests: logs requests, replays queued replies.
#[derive(Debug, Default)]
pub struct RecordingTransport {
    /// Every destination sent, in order.
    pub sent: alloc::vec::Vec<Destination>,
    /// Queued outcomes, consumed first-in first-out.
    pub replies: alloc::vec::Vec<Result<Reply, TransportError>>,
    /// Fallback status when the queue runs dry.
    pub fallback: i32,
}

impl RecordingTransport {
    /// Fresh transport answering with this status once the queue is empty.
    pub fn new(fallback: i32) -> RecordingTransport {
        RecordingTransport {
            sent: alloc::vec::Vec::new(),
            replies: alloc::vec::Vec::new(),
            fallback,
        }
    }

    /// Enqueue one scripted outcome.
    pub fn push(&mut self, outcome: Result<Reply, TransportError>) {
        self.replies.push(outcome);
    }
}

impl Transport for RecordingTransport {
    fn exchange(&mut self, destination: Destination) -> Result<Reply, TransportError> {
        self.sent.push(destination);
        if self.replies.is_empty() {
            return Ok(Reply {
                message_type: BDEV_REPLY,
                id: destination.id,
                status: self.fallback,
            });
        }
        self.replies.remove(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_loopback_echoes_identifier_and_status() {
        let mut transport = LoopbackTransport::healthy(0);
        let reply = transport
            .exchange(Destination {
                endpoint: 5,
                message_type: 0x502,
                minor: 0,
                id: 11,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(reply.id, 11);
        assert_eq!(reply.status, 0);
    }

    #[test]
    fn test_loopback_mismatch_mode_fires_once() {
        let mut transport = LoopbackTransport {
            status: 0,
            mismatch_once: true,
        };
        let first = transport
            .exchange(Destination {
                endpoint: 5,
                message_type: 0x502,
                minor: 0,
                id: 11,
                ..Default::default()
            })
            .unwrap();
        assert_ne!(first.id, 11);
        let second = transport
            .exchange(Destination {
                endpoint: 5,
                message_type: 0x502,
                minor: 0,
                id: 11,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(second.id, 11);
    }

    #[test]
    fn test_recording_transport_replays_script_then_fallback() {
        let mut transport = RecordingTransport::new(-5);
        transport.push(Err(TransportError::SendFailed));
        transport.push(Ok(Reply {
            message_type: BDEV_REPLY,
            id: 3,
            status: 0,
        }));
        let destination = Destination {
            endpoint: 1,
            message_type: 0x500,
            minor: 0,
            id: 3,
                ..Default::default()
            };
        assert_eq!(
            transport.exchange(destination),
            Err(TransportError::SendFailed)
        );
        assert_eq!(transport.exchange(destination).unwrap().status, 0);
        assert_eq!(transport.exchange(destination).unwrap().status, -5);
        assert_eq!(transport.sent.len(), 3);
    }
}
