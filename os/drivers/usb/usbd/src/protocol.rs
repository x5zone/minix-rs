//! USB request protocol: the five driver-to-host requests plus answers.
//!
//! C correspondence: the request constants (`USB_RQ_INIT`,
//! `USB_RQ_DEINIT`, `USB_RQ_SEND_URB`, `USB_RQ_CANCEL_URB`,
//! `USB_RQ_SEND_INFO` on `USB_BASE 0x1100`, `com.h:813-820`), the
//! reply (`USB_REPLY`, `com.h:821`), the three host-to-driver
//! notifications (`USB_COMPLETE_URB`, `USB_ANNOUCE_DEV`,
//! `USB_WITHDRAW_DEV`, `com.h:825-827`), and the message field map
//! (`com.h:829-840`). The client wrappers (`usb_send_urb`,
//! `usb_cancle_urb`, `usb_init`, `usb_send_info`, `usb.c:20-255`)
//! send these requests; the server dispatch (`usb_server.c:731-758`)
//! answers them. Grant plumbing and endpoint traffic stay out; this
//! module owns the numbering half: which value means what.

/// Base of the USB request range (`USB_BASE`, `com.h:813`).
pub const USB_BASE: u32 = 0x1100;

/// Request sent by a USB driver to the host daemon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum DriverRequest {
    /// Register this driver (`USB_RQ_INIT`, base plus 0).
    Init = USB_BASE,
    /// Unregister this driver (`USB_RQ_DEINIT`, base plus 1).
    Deinit = USB_BASE + 1,
    /// Submit one USB request block (`USB_RQ_SEND_URB`, base plus 2).
    SendUrb = USB_BASE + 2,
    /// Cancel a submitted request block (`USB_RQ_CANCEL_URB`, base plus 3).
    CancelUrb = USB_BASE + 3,
    /// Report driver information (`USB_RQ_SEND_INFO`, base plus 4).
    SendInfo = USB_BASE + 4,
}

/// Answer flowing back from the host daemon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum DaemonMessage {
    /// Generic reply to a driver request (`USB_REPLY`, base plus 5).
    Reply = USB_BASE + 5,
    /// One request block finished (`USB_COMPLETE_URB`, base plus 6).
    CompleteUrb = USB_BASE + 6,
    /// A device appeared (`USB_ANNOUCE_DEV`, base plus 7).
    AnnounceDevice = USB_BASE + 7,
    /// A device vanished (`USB_WITHDRAW_DEV`, base plus 8).
    WithdrawDevice = USB_BASE + 8,
}

/// Decode a raw message number into a driver request, if it is one.
pub fn decode_driver_request(raw: u32) -> Option<DriverRequest> {
    match raw {
        x if x == DriverRequest::Init as u32 => Some(DriverRequest::Init),
        x if x == DriverRequest::Deinit as u32 => Some(DriverRequest::Deinit),
        x if x == DriverRequest::SendUrb as u32 => Some(DriverRequest::SendUrb),
        x if x == DriverRequest::CancelUrb as u32 => Some(DriverRequest::CancelUrb),
        x if x == DriverRequest::SendInfo as u32 => Some(DriverRequest::SendInfo),
        _ => None,
    }
}

/// Decode a raw message number into a daemon message, if it is one.
pub fn decode_daemon_message(raw: u32) -> Option<DaemonMessage> {
    match raw {
        x if x == DaemonMessage::Reply as u32 => Some(DaemonMessage::Reply),
        x if x == DaemonMessage::CompleteUrb as u32 => Some(DaemonMessage::CompleteUrb),
        x if x == DaemonMessage::AnnounceDevice as u32 => Some(DaemonMessage::AnnounceDevice),
        x if x == DaemonMessage::WithdrawDevice as u32 => Some(DaemonMessage::WithdrawDevice),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base_matches_com_header() {
        assert_eq!(USB_BASE, 0x1100);
    }

    #[test]
    fn test_five_requests_numbered_in_order() {
        assert_eq!(DriverRequest::Init as u32, 0x1100);
        assert_eq!(DriverRequest::Deinit as u32, 0x1101);
        assert_eq!(DriverRequest::SendUrb as u32, 0x1102);
        assert_eq!(DriverRequest::CancelUrb as u32, 0x1103);
        assert_eq!(DriverRequest::SendInfo as u32, 0x1104);
    }

    #[test]
    fn test_daemon_messages_follow_requests() {
        assert_eq!(DaemonMessage::Reply as u32, 0x1105);
        assert_eq!(DaemonMessage::CompleteUrb as u32, 0x1106);
        assert_eq!(DaemonMessage::AnnounceDevice as u32, 0x1107);
        assert_eq!(DaemonMessage::WithdrawDevice as u32, 0x1108);
    }

    #[test]
    fn test_decode_round_trips_and_rejects_foreign() {
        assert_eq!(decode_driver_request(0x1102), Some(DriverRequest::SendUrb));
        assert_eq!(decode_driver_request(0x1105), None);
        assert_eq!(decode_daemon_message(0x1107), Some(DaemonMessage::AnnounceDevice));
        assert_eq!(decode_daemon_message(0x1102), None);
        assert_eq!(decode_driver_request(0x400), None);
    }
}
