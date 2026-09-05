//! Socket-device request protocol: seventeen requests, six replies.
//!
//! C correspondence: the request base (`SDEV_RQ_BASE 0x1900`,
//! `com.h:1037`) with seventeen requests (`SDEV_SOCKET` through
//! `SDEV_SELECT`, `com.h:1044-1060`), the reply base
//! (`SDEV_RS_BASE 0x1980`, `com.h:1038`) with six replies
//! (`com.h:1063-1068`), the request guard (`IS_SDEV_RQ`,
//! `com.h:1040`), the reply guard (`IS_SDEV_RS`, `com.h:1041`),
//! and the operation flags (`SDEV_OP_RD/WR/ERR/NOTIFY`,
//! `com.h:1075-1078`). The suspendability rule (which requests may
//! wait, `sockdriver.c:10-26`) is modeled alongside: waiting
//! requests need the resume machinery owned by the event library.
//!
//! Message packing stays in the service binary; this module owns the
//! numbering half: which value means what and which request may wait.

/// Base of the socket-device request range (`SDEV_RQ_BASE`).
pub const SDEV_RQ_BASE: u32 = 0x1900;

/// Base of the socket-device reply range (`SDEV_RS_BASE`).
pub const SDEV_RS_BASE: u32 = 0x1980;

/// Request sent by the virtual file system to a socket driver.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum SdevRequest {
    /// Open a socket (`SDEV_SOCKET`, base plus 0).
    Socket = SDEV_RQ_BASE,
    /// Open a connected pair (`SDEV_SOCKETPAIR`, base plus 1).
    SocketPair = SDEV_RQ_BASE + 1,
    /// Bind an address (`SDEV_BIND`, base plus 2).
    Bind = SDEV_RQ_BASE + 2,
    /// Connect to a peer (`SDEV_CONNECT`, base plus 3).
    Connect = SDEV_RQ_BASE + 3,
    /// Listen for peers (`SDEV_LISTEN`, base plus 4).
    Listen = SDEV_RQ_BASE + 4,
    /// Accept a peer (`SDEV_ACCEPT`, base plus 5).
    Accept = SDEV_RQ_BASE + 5,
    /// Send data (`SDEV_SEND`, base plus 6).
    Send = SDEV_RQ_BASE + 6,
    /// Receive data (`SDEV_RECV`, base plus 7).
    Receive = SDEV_RQ_BASE + 7,
    /// Device control (`SDEV_IOCTL`, base plus 8).
    Ioctl = SDEV_RQ_BASE + 8,
    /// Set an option (`SDEV_SETSOCKOPT`, base plus 9).
    SetSockOpt = SDEV_RQ_BASE + 9,
    /// Get an option (`SDEV_GETSOCKOPT`, base plus 10).
    GetSockOpt = SDEV_RQ_BASE + 10,
    /// Get the local address (`SDEV_GETSOCKNAME`, base plus 11).
    GetSockName = SDEV_RQ_BASE + 11,
    /// Get the peer address (`SDEV_GETPEERNAME`, base plus 12).
    GetPeerName = SDEV_RQ_BASE + 12,
    /// Shut one direction down (`SDEV_SHUTDOWN`, base plus 13).
    Shutdown = SDEV_RQ_BASE + 13,
    /// Close the socket (`SDEV_CLOSE`, base plus 14).
    Close = SDEV_RQ_BASE + 14,
    /// Cancel a waiting call (`SDEV_CANCEL`, base plus 15).
    Cancel = SDEV_RQ_BASE + 15,
    /// Wait for readiness (`SDEV_SELECT`, base plus 16).
    Select = SDEV_RQ_BASE + 16,
}

/// Answer flowing back from a socket driver.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum SdevReply {
    /// Generic answer (`SDEV_REPLY`, reply base plus 0).
    Reply = SDEV_RS_BASE,
    /// Socket created (`SDEV_SOCKET_REPLY`, plus 1).
    SocketReply = SDEV_RS_BASE + 1,
    /// Peer accepted (`SDEV_ACCEPT_REPLY`, plus 2).
    AcceptReply = SDEV_RS_BASE + 2,
    /// Data received (`SDEV_RECV_REPLY`, plus 3).
    ReceiveReply = SDEV_RS_BASE + 3,
    /// Readiness, first half (`SDEV_SELECT1_REPLY`, plus 4).
    SelectReply1 = SDEV_RS_BASE + 4,
    /// Readiness, second half (`SDEV_SELECT2_REPLY`, plus 5).
    SelectReply2 = SDEV_RS_BASE + 5,
}

/// Whether a raw message number is a socket-device request
/// (`IS_SDEV_RQ`: all but the low seven bits equal the base).
pub fn is_sdev_request(raw: u32) -> bool {
    raw & !0x7f == SDEV_RQ_BASE
}

/// Whether a raw message number is a socket-device reply
/// (`IS_SDEV_RS`).
pub fn is_sdev_reply(raw: u32) -> bool {
    raw & !0x7f == SDEV_RS_BASE
}

/// Whether a request may wait for a later event
/// (`sockdriver.c:10-26`): waiting requests need suspend and resume;
/// the rest always answer at once. Cancel carries no answer at all.
pub fn may_suspend(request: SdevRequest) -> bool {
    match request {
        SdevRequest::Bind
        | SdevRequest::Connect
        | SdevRequest::Accept
        | SdevRequest::Send
        | SdevRequest::Receive
        | SdevRequest::Ioctl
        | SdevRequest::Close
        | SdevRequest::Select => true,
        SdevRequest::Socket
        | SdevRequest::SocketPair
        | SdevRequest::Listen
        | SdevRequest::SetSockOpt
        | SdevRequest::GetSockOpt
        | SdevRequest::GetSockName
        | SdevRequest::GetPeerName
        | SdevRequest::Shutdown
        | SdevRequest::Cancel => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bases_match_com_header() {
        assert_eq!(SDEV_RQ_BASE, 0x1900);
        assert_eq!(SDEV_RS_BASE, 0x1980);
    }

    #[test]
    fn test_seventeen_requests_numbered_in_order() {
        assert_eq!(SdevRequest::Socket as u32, 0x1900);
        assert_eq!(SdevRequest::Select as u32, 0x1910);
        assert_eq!(SdevRequest::GetPeerName as u32, 0x190C);
        assert_eq!(SdevRequest::Shutdown as u32, 0x190D);
    }

    #[test]
    fn test_six_replies_follow_their_base() {
        assert_eq!(SdevReply::Reply as u32, 0x1980);
        assert_eq!(SdevReply::SelectReply2 as u32, 0x1985);
    }

    #[test]
    fn test_guards_accept_own_range_only() {
        assert!(is_sdev_request(0x1900));
        assert!(is_sdev_request(0x1910));
        assert!(!is_sdev_request(0x1980));
        assert!(!is_sdev_reply(0x1910));
        assert!(is_sdev_reply(0x1985));
        assert!(!is_sdev_request(0x0400));
    }

    #[test]
    fn test_suspend_table_matches_driver_comment() {
        assert!(may_suspend(SdevRequest::Connect));
        assert!(may_suspend(SdevRequest::Receive));
        assert!(may_suspend(SdevRequest::Select));
        assert!(!may_suspend(SdevRequest::Socket));
        assert!(!may_suspend(SdevRequest::Listen));
        assert!(!may_suspend(SdevRequest::Shutdown));
        assert!(!may_suspend(SdevRequest::Cancel));
    }
}
