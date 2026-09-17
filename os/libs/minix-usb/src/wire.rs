//! USB message field slots: which long argument carries what.
//!
//! C correspondence: the field aliases under the USB request numbers
//! (`com.h:813-845`) — `USB_GRANT_ID`/`USB_GRANT_SIZE` on send-urb,
//! `USB_URB_ID`/`USB_RESULT` on completion, `USB_DEV_ID`/`USB_DRIVER_EP`/
//! `USB_INTERFACES` on announce, `USB_RB_INIT_NAME` (the `m3_ca1` string)
//! on init, and `USB_INFO_TYPE`/`USB_INFO_VALUE` on info requests. The
//! opcode *numbers* live in [`crate::protocol`]; this module is the other
//! half of the contract: the payload slots.
//!
//! The USB family speaks through the M4 payload arm (four long arguments
//! plus padding, `MessageM4`); the init name rides the M3 character arm.
//! These helpers type the slots so no caller hand-indexes union fields.

use alloc::string::String;
use minix_types::Message;

/// Fill a send-urb request: the grant carrying the transfer and its size.
///
/// C: `USB_GRANT_ID m4_l1`, `USB_GRANT_SIZE m4_l2` (`com.h:830-831`).
pub fn fill_send_urb(msg: &mut Message, grant_id: i64, grant_size: i64) {
    let arm = unsafe { &mut msg.m_u.m_m4 };
    arm.m4l1 = grant_id;
    arm.m4l2 = grant_size;
}

/// Read a send-urb request's grant slots (symmetric to
/// [`fill_send_urb`]).
pub fn split_send_urb(msg: &Message) -> Option<(i64, i64)> {
    let arm = unsafe { &msg.m_u.m_m4 };
    Some((arm.m4l1, arm.m4l2))
}

/// Fill an urb-completion report: which block finished, with what result.
///
/// C: `USB_URB_ID m4_l1`, `USB_RESULT m4_l2` (`com.h:833-834`).
pub fn fill_complete_urb(msg: &mut Message, urb_id: i64, result: i64) {
    let arm = unsafe { &mut msg.m_u.m_m4 };
    arm.m4l1 = urb_id;
    arm.m4l2 = result;
}

/// Read an urb-completion report (symmetric to [`fill_complete_urb`]).
pub fn split_complete_urb(msg: &Message) -> Option<(i64, i64)> {
    let arm = unsafe { &msg.m_u.m_m4 };
    Some((arm.m4l1, arm.m4l2))
}

/// Fill a device announcement: device identity, announcing driver, and
/// the interface count.
///
/// C: `USB_DEV_ID m4_l1`, `USB_DRIVER_EP m4_l2`, `USB_INTERFACES m4_l3`
/// (`com.h:835-837`).
pub fn fill_announce_dev(msg: &mut Message, dev_id: i64, driver_ep: i64, interfaces: i64) {
    let arm = unsafe { &mut msg.m_u.m_m4 };
    arm.m4l1 = dev_id;
    arm.m4l2 = driver_ep;
    arm.m4l3 = interfaces;
}

/// Read a device announcement (symmetric to [`fill_announce_dev`]).
pub fn split_announce_dev(msg: &Message) -> Option<(i64, i64, i64)> {
    let arm = unsafe { &msg.m_u.m_m4 };
    Some((arm.m4l1, arm.m4l2, arm.m4l3))
}

/// Fill an info request: what is being asked and the value payload.
///
/// C: `USB_INFO_TYPE m4_l1`, `USB_INFO_VALUE m4_l2` (`com.h:840-841`).
pub fn fill_info(msg: &mut Message, info_type: i64, info_value: i64) {
    let arm = unsafe { &mut msg.m_u.m_m4 };
    arm.m4l1 = info_type;
    arm.m4l2 = info_value;
}

/// Read an info request (symmetric to [`fill_info`]).
pub fn split_info(msg: &Message) -> Option<(i64, i64)> {
    let arm = unsafe { &msg.m_u.m_m4 };
    Some((arm.m4l1, arm.m4l2))
}

/// The init handshake's driver name, carried in the M3 character arm
/// (`USB_RB_INIT_NAME m3_ca1`, `com.h:838`).
///
/// Returns `None` for bytes that are not valid until a NUL (the C code
/// copies the fixed-size field and trusts the sender to NUL-terminate).
pub fn split_init_name(msg: &Message) -> Option<String> {
    let name = unsafe { &msg.m_u.m_m3.m3ca1 };
    let end = name.iter().position(|byte| *byte == 0)?;
    if end == 0 {
        return None;
    }
    Some(String::from_utf8_lossy(&name[..end]).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;
    use alloc::vec;

    #[test]
    fn test_send_urb_and_completion_round_trip() {
        let mut msg = Message::default();
        fill_send_urb(&mut msg, 0x1234, 512);
        assert_eq!(split_send_urb(&msg), Some((0x1234, 512)));
        fill_complete_urb(&mut msg, 0x1234, -5);
        assert_eq!(split_complete_urb(&msg), Some((0x1234, -5)));
    }

    #[test]
    fn test_announce_carries_three_slots() {
        let mut msg = Message::default();
        fill_announce_dev(&mut msg, 42, 7, 2);
        assert_eq!(split_announce_dev(&msg), Some((42, 7, 2)));
    }

    #[test]
    fn test_info_slots_round_trip() {
        let mut msg = Message::default();
        fill_info(&mut msg, 1, 0x55);
        assert_eq!(split_info(&msg), Some((1, 0x55)));
    }

    #[test]
    fn test_init_name_stops_at_nul() {
        let mut msg = Message::default();
        let name = b"usb_storage";
        let arm = unsafe { &mut msg.m_u.m_m3 };
        arm.m3ca1[..name.len()].copy_from_slice(name);
        arm.m3ca1[name.len()] = 0;
        assert_eq!(split_init_name(&msg), Some("usb_storage".to_string()));
    }
}
