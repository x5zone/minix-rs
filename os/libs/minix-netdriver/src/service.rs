//! Message arrival classification shared by the two network services.
//!
//! C correspondence: the `main` loop dispatch of the lwip service
//! (`minix3/minix/net/lwip/lwip.c:293-382`): notifications split by source
//! (clock ticks expire timers, a data-store notification signals card
//! drivers up/down), management information base requests come from the
//! MIB server, the virtual file system brings socket-device and character
//! or block device requests, and network device replies arrive from card
//! drivers. Everything else is logged and dropped.
//!
//! Classification is by source endpoint first and message type second —
//! the same request number means different things from different senders,
//! so a type-only router would misfile messages.

use crate::protocol::{is_net_reply, is_net_request};
use crate::sdev::is_sdev_request;
use minix_types::{Endpoint, Message};

/// One arrival sorted into the road it must travel. The message travels
/// with the road; the loop hands it to the road's handler.
#[derive(Debug, Clone)]
pub enum Arrival {
    /// Clock notification: expire the timer list (`lwip.c:328-330`).
    NotifyClock,
    /// Data-store notification: card drivers went up and/or down
    /// (`lwip.c:331-335`).
    NotifyDevMgr,
    /// Any other notification: unexpected, logged and dropped
    /// (`lwip.c:336-343`).
    NotifyOther,
    /// Management information base request from the MIB server
    /// (`lwip.c:349-352`).
    Management(Message),
    /// Socket-device request from the virtual file system (`IS_SDEV_RQ`,
    /// `lwip.c:354-357`).
    SocketDevice(Message),
    /// Character or block device request from the virtual file system —
    /// the packet-filter device's road (`lwip.c:358-361`). Whether the
    /// type is in the character/block ranges is decided by the caller,
    /// which owns those framework guards.
    BpfDevice(Message),
    /// A network device driver's reply or status report (`IS_NDEV_RS`,
    /// `lwip.c:366-371`).
    NetDeviceReply(Message),
    /// Anything else: unexpected, logged and dropped (`lwip.c:372-377`).
    Unexpected(Message),
}

/// Sort one arrival into its road.
///
/// `is_notify` is the kernel status bit (`is_ipc_notify`); `from_vfs_devices`
/// says the message came from the virtual file system with a type in one of
/// the character/block request ranges (the lwip service tests
/// `IS_CDEV_RQ`/`IS_BDEV_RQ` with the constants owned by the driver
/// frameworks).
pub fn classify(source: Endpoint, m_type: i32, is_notify: bool, from_vfs_devices: bool) -> Arrival {
    if is_notify {
        return if source == Endpoint::CLOCK {
            Arrival::NotifyClock
        } else if source == Endpoint::DS {
            Arrival::NotifyDevMgr
        } else {
            Arrival::NotifyOther
        };
    }
    if source == Endpoint::MIB {
        return Arrival::Management(Message { m_source: source, m_type, ..Message::default() });
    }
    if source == Endpoint::VFS {
        if is_sdev_request(m_type as u32) {
            return Arrival::SocketDevice(Message { m_source: source, m_type, ..Message::default() });
        }
        if from_vfs_devices {
            return Arrival::BpfDevice(Message { m_source: source, m_type, ..Message::default() });
        }
        return Arrival::Unexpected(Message { m_source: source, m_type, ..Message::default() });
    }
    if is_net_request(m_type) || is_net_reply(m_type) {
        return Arrival::NetDeviceReply(Message { m_source: source, m_type, ..Message::default() });
    }
    Arrival::Unexpected(Message { m_source: source, m_type, ..Message::default() })
}

/// Sort a decoded message by its own source and type — the form the event
/// loop actually consumes.
pub fn classify_message(message: &Message, is_notify: bool, from_vfs_devices: bool) -> Arrival {
    classify(message.m_source, message.m_type, is_notify, from_vfs_devices)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(source: Endpoint, m_type: i32) -> Message {
        Message { m_source: source, m_type, ..Message::default() }
    }

    const SDEV_SOCKET: i32 = crate::sdev::SdevRequest::Socket as i32;
    const NDEV_INIT_REPLY: i32 = crate::protocol::NDEV_REPLY_BASE;

    #[test]
    fn test_notifications_split_by_source() {
        assert!(matches!(
            classify(Endpoint::CLOCK, 0, true, false),
            Arrival::NotifyClock
        ));
        assert!(matches!(
            classify(Endpoint::DS, 0, true, false),
            Arrival::NotifyDevMgr
        ));
        assert!(matches!(
            classify(Endpoint::PM, 0, true, false),
            Arrival::NotifyOther
        ));
    }

    #[test]
    fn test_vfs_road_splits_by_request_range() {
        assert!(matches!(
            classify(Endpoint::VFS, SDEV_SOCKET, false, false),
            Arrival::SocketDevice(_)
        ));
        assert!(
            matches!(
                classify(Endpoint::VFS, 0x403, false, true),
                Arrival::BpfDevice(_)
            ),
            "字符请求范围加设备来源标记走过滤器路"
        );
        assert!(
            matches!(
                classify(Endpoint::VFS, 0x1234, false, false),
                Arrival::Unexpected(_)
            ),
            "VFS 来的陌生号不进任何路"
        );
    }

    #[test]
    fn test_management_and_device_roads() {
        assert!(matches!(
            classify(Endpoint::MIB, 1, false, false),
            Arrival::Management(_)
        ));
        assert!(matches!(
            classify(Endpoint::ANY, NDEV_INIT_REPLY, false, false),
            Arrival::NetDeviceReply(_)
        ));
        assert!(matches!(
            classify(Endpoint::ANY, 0x7654, false, false),
            Arrival::Unexpected(_)
        ));
    }

    #[test]
    fn test_classifier_prefers_source_over_type() {
        assert!(
            matches!(
                classify_message(&message(Endpoint::PM, SDEV_SOCKET), false, false),
                Arrival::Unexpected(_)
            ),
            "SDEV 号从非 VFS 来源到达即意外——来源优先于类型"
        );
    }
}
