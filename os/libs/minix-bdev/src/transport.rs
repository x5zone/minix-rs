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
                message_type: 0x580,
                id: destination.id.wrapping_add(1),
                status: self.status,
            });
        }
        Ok(Reply {
            message_type: 0x580,
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
                message_type: 0x580,
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
            })
            .unwrap();
        assert_ne!(first.id, 11);
        let second = transport
            .exchange(Destination {
                endpoint: 5,
                message_type: 0x502,
                minor: 0,
                id: 11,
            })
            .unwrap();
        assert_eq!(second.id, 11);
    }

    #[test]
    fn test_recording_transport_replays_script_then_fallback() {
        let mut transport = RecordingTransport::new(-5);
        transport.push(Err(TransportError::SendFailed));
        transport.push(Ok(Reply {
            message_type: 0x580,
            id: 3,
            status: 0,
        }));
        let destination = Destination {
            endpoint: 1,
            message_type: 0x500,
            minor: 0,
            id: 3,
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
