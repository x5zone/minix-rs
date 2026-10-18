//! NDEV message wire layouts: where each field sits on the wire.
//!
//! C correspondence: the message structures in
//! `minix3/minix/include/minix/ipc.h` —
//! `mess_ndev_netdriver_init` (`ipc.h:1594`),
//! `mess_ndev_netdriver_conf` (`ipc.h:1602`),
//! `mess_ndev_netdriver_transfer` (`ipc.h:1610`),
//! `mess_ndev_netdriver_status_reply` (`ipc.h:1618`),
//! `mess_netdriver_ndev_init_reply` (`ipc.h:1632`),
//! `mess_netdriver_ndev_reply` (`ipc.h:1646`), and
//! `mess_netdriver_ndev_status` (`ipc.h:1656`). Each fills the 56-byte
//! union body (`_ASSERT_MSG_SIZE` in the C header); the message's source
//! and type precede it, so the union starts at byte 8.
//!
//! [`protocol`](crate::protocol) owns the numbers; this module owns their
//! positions. Every offset is pinned by a `#[repr(C)]` mirror of the C
//! structure in the tests below, so a layout drift fails a host test
//! instead of silently misreading a peer.

use minix_types::Message;

/// Bytes before the union body in a [`Message`] (`m_source` plus `m_type`).
pub const PAYLOAD: usize = 8;

/// Size of the union body (the 64-byte message minus source and type).
pub const UNION_SIZE: usize = 56;

/// `NDEV_INIT` request (`mess_ndev_netdriver_init`).
pub mod init_request {
    /// Request id, echoed back in the initialization reply.
    pub const ID: usize = super::PAYLOAD;
}

/// `NDEV_CONF` request (`mess_ndev_netdriver_conf`).
pub mod conf_request {
    /// Request id, echoed back in the configuration reply.
    pub const ID: usize = super::PAYLOAD;
    /// Which settings this request carries (`NDEV_SET_*` bits).
    pub const SET: usize = super::PAYLOAD + 4;
    /// Mode bits (`NDEV_MODE_*`).
    pub const MODE: usize = super::PAYLOAD + 8;
    /// Grant of the multicast address list.
    pub const MCAST_GRANT: usize = super::PAYLOAD + 12;
    /// Multicast address count.
    pub const MCAST_COUNT: usize = super::PAYLOAD + 16;
    /// Capability bits.
    pub const CAPS: usize = super::PAYLOAD + 20;
    /// Driver-specific flags.
    pub const FLAGS: usize = super::PAYLOAD + 24;
    /// Media type.
    pub const MEDIA: usize = super::PAYLOAD + 28;
    /// New hardware address, six bytes.
    pub const HWADDR: usize = super::PAYLOAD + 32;
}

/// `NDEV_SEND` / `NDEV_RECV` request (`mess_ndev_netdriver_transfer`).
pub mod transfer_request {
    /// Request id, echoed back in the transfer reply.
    pub const ID: usize = super::PAYLOAD;
    /// Number of live vector elements (one through eight).
    pub const COUNT: usize = super::PAYLOAD + 4;
    /// First grant slot (`cp_grant_id_t` array).
    pub const GRANT: usize = super::PAYLOAD + 8;
    /// Distance between grant slots.
    pub const GRANT_STRIDE: usize = 4;
    /// First length slot (`uint16_t` array).
    pub const LEN: usize = super::PAYLOAD + 40;
    /// Distance between length slots.
    pub const LEN_STRIDE: usize = 2;
}

/// `NDEV_STATUS_REPLY` request (`mess_ndev_netdriver_status_reply`).
pub mod status_reply_request {
    /// Id of the status report being acknowledged (zero today).
    pub const ID: usize = super::PAYLOAD;
}

/// `NDEV_INIT_REPLY` (`mess_netdriver_ndev_init_reply`).
pub mod init_reply {
    /// Request id echoed from the initialization request.
    pub const ID: usize = super::PAYLOAD;
    /// Link state (`NDEV_LINK_*`).
    pub const LINK: usize = super::PAYLOAD + 4;
    /// Active media.
    pub const MEDIA: usize = super::PAYLOAD + 8;
    /// Capability bits.
    pub const CAPS: usize = super::PAYLOAD + 12;
    /// Device name, NUL-padded (`NDEV_NAME_MAX` bytes).
    pub const NAME: usize = super::PAYLOAD + 16;
    /// Hardware address bytes (`NDEV_HWADDR_MAX`).
    pub const HWADDR: usize = super::PAYLOAD + 32;
    /// Significant hardware address bytes.
    pub const HWADDR_LEN: usize = super::PAYLOAD + 38;
    /// Outgoing queue depth the driver accepts.
    pub const MAX_SEND: usize = super::PAYLOAD + 39;
    /// Incoming queue depth the driver accepts.
    pub const MAX_RECV: usize = super::PAYLOAD + 40;
}

/// `NDEV_SEND_REPLY` / `NDEV_RECV_REPLY` / `NDEV_CONF_REPLY`
/// (`mess_netdriver_ndev_reply`).
pub mod ndev_reply {
    /// Request id echoed back.
    pub const ID: usize = super::PAYLOAD;
    /// Result: bytes moved for transfers, `OK` for configuration.
    pub const RESULT: usize = super::PAYLOAD + 4;
}

/// `NDEV_STATUS` report (`mess_netdriver_ndev_status`).
pub mod status {
    /// Status id (zero today).
    pub const ID: usize = super::PAYLOAD;
    /// Link state.
    pub const LINK: usize = super::PAYLOAD + 4;
    /// Active media.
    pub const MEDIA: usize = super::PAYLOAD + 8;
    /// Output error count.
    pub const OERROR: usize = super::PAYLOAD + 12;
    /// Collision count.
    pub const COLL: usize = super::PAYLOAD + 16;
    /// Input error count.
    pub const IERROR: usize = super::PAYLOAD + 20;
    /// Input queue drop count.
    pub const IQDROP: usize = super::PAYLOAD + 24;
}

/// Notification payload: the hardware interrupt mask (`mess_notify`).
pub mod notify {
    /// Interrupt mask carried by a harness notification.
    pub const INTERRUPTS: usize = super::PAYLOAD + 8;
}

/// Read `N` raw bytes from the message at `offset`.
fn read_bytes<const N: usize>(msg: &Message, offset: usize) -> [u8; N] {
    let base = ::core::ptr::addr_of!(*msg) as *const u8;
    let mut bytes = [0u8; N];
    // SAFETY: `Message` is a `#[repr(C)]` plain-old-data union; every
    // offset comes from the pinned wire layout above and stays inside the
    // fixed-size body.
    unsafe {
        ::core::ptr::copy_nonoverlapping(base.add(offset), bytes.as_mut_ptr(), N);
    }
    bytes
}

/// Write raw bytes into the message at `offset`.
fn write_bytes(msg: &mut Message, offset: usize, bytes: &[u8]) {
    let base = ::core::ptr::addr_of_mut!(*msg) as *mut u8;
    // SAFETY: same invariants as [`read_bytes`]; writers only pass offsets
    // and lengths from the pinned layouts.
    unsafe {
        ::core::ptr::copy_nonoverlapping(bytes.as_ptr(), base.add(offset), bytes.len());
    }
}

/// Read an unsigned byte.
pub fn read_u8(msg: &Message, offset: usize) -> u8 {
    read_bytes::<1>(msg, offset)[0]
}

/// Read a little-endian `u16`.
pub fn read_u16(msg: &Message, offset: usize) -> u16 {
    u16::from_le_bytes(read_bytes(msg, offset))
}

/// Read a little-endian `u32`.
pub fn read_u32(msg: &Message, offset: usize) -> u32 {
    u32::from_le_bytes(read_bytes(msg, offset))
}

/// Read a little-endian `i32`.
pub fn read_i32(msg: &Message, offset: usize) -> i32 {
    i32::from_le_bytes(read_bytes(msg, offset))
}

/// Read a fixed byte array.
pub fn read_array<const N: usize>(msg: &Message, offset: usize) -> [u8; N] {
    read_bytes(msg, offset)
}

/// Write an unsigned byte.
pub fn write_u8(msg: &mut Message, offset: usize, value: u8) {
    write_bytes(msg, offset, &[value]);
}

/// Write a little-endian `u32`.
pub fn write_u32(msg: &mut Message, offset: usize, value: u32) {
    write_bytes(msg, offset, &value.to_le_bytes());
}

/// Write a little-endian `i32`.
pub fn write_i32(msg: &mut Message, offset: usize, value: i32) {
    write_bytes(msg, offset, &value.to_le_bytes());
}

/// Write a fixed byte array.
pub fn write_array(msg: &mut Message, offset: usize, bytes: &[u8]) {
    write_bytes(msg, offset, bytes);
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::{offset_of, size_of};

    // C-layout mirrors. Field order, types, and trailing padding copy
    // `ipc.h` literally; `_ASSERT_MSG_SIZE` becomes the size assertion.

    #[repr(C)]
    struct MessengerInit {
        id: u32,
        padding: [u8; 52],
    }

    #[repr(C)]
    struct MessengerConf {
        id: u32,
        set: u32,
        mode: u32,
        mcast_grant: i32,
        mcast_count: u32,
        caps: u32,
        flags: u32,
        media: u32,
        hwaddr: [u8; 6],
        padding: [u8; 18],
    }

    #[repr(C)]
    struct MessengerTransfer {
        id: u32,
        count: u32,
        grant: [i32; 8],
        len: [u16; 8],
    }

    #[repr(C)]
    struct MessengerStatusReply {
        id: u32,
        padding: [u8; 52],
    }

    #[repr(C)]
    struct MessengerInitReply {
        id: u32,
        link: u32,
        media: u32,
        caps: u32,
        name: [u8; 16],
        hwaddr: [u8; 6],
        hwaddr_len: u8,
        max_send: u8,
        max_recv: u8,
        padding: [u8; 15],
    }

    #[repr(C)]
    struct MessengerReply {
        id: u32,
        result: i32,
        padding: [u8; 48],
    }

    #[repr(C)]
    struct MessengerStatus {
        id: u32,
        link: u32,
        media: u32,
        oerror: u32,
        coll: u32,
        ierror: u32,
        iqdrop: u32,
        padding: [u8; 28],
    }

    #[test]
    fn test_request_layouts_match_the_c_structures() {
        assert_eq!(size_of::<MessengerInit>(), UNION_SIZE);
        assert_eq!(offset_of!(MessengerInit, id), init_request::ID - PAYLOAD);

        assert_eq!(size_of::<MessengerConf>(), UNION_SIZE);
        assert_eq!(offset_of!(MessengerConf, id), conf_request::ID - PAYLOAD);
        assert_eq!(offset_of!(MessengerConf, set), conf_request::SET - PAYLOAD);
        assert_eq!(offset_of!(MessengerConf, mode), conf_request::MODE - PAYLOAD);
        assert_eq!(
            offset_of!(MessengerConf, mcast_grant),
            conf_request::MCAST_GRANT - PAYLOAD
        );
        assert_eq!(
            offset_of!(MessengerConf, mcast_count),
            conf_request::MCAST_COUNT - PAYLOAD
        );
        assert_eq!(offset_of!(MessengerConf, caps), conf_request::CAPS - PAYLOAD);
        assert_eq!(offset_of!(MessengerConf, flags), conf_request::FLAGS - PAYLOAD);
        assert_eq!(offset_of!(MessengerConf, media), conf_request::MEDIA - PAYLOAD);
        assert_eq!(offset_of!(MessengerConf, hwaddr), conf_request::HWADDR - PAYLOAD);

        assert_eq!(size_of::<MessengerTransfer>(), UNION_SIZE);
        assert_eq!(offset_of!(MessengerTransfer, id), transfer_request::ID - PAYLOAD);
        assert_eq!(offset_of!(MessengerTransfer, count), transfer_request::COUNT - PAYLOAD);
        assert_eq!(offset_of!(MessengerTransfer, grant), transfer_request::GRANT - PAYLOAD);
        assert_eq!(offset_of!(MessengerTransfer, len), transfer_request::LEN - PAYLOAD);
        assert_eq!(
            size_of::<i32>(),
            transfer_request::GRANT_STRIDE,
            "grant slots are cp_grant_id_t (int32_t)"
        );
        assert_eq!(
            size_of::<u16>(),
            transfer_request::LEN_STRIDE,
            "length slots are uint16_t"
        );

        assert_eq!(size_of::<MessengerStatusReply>(), UNION_SIZE);
        assert_eq!(
            offset_of!(MessengerStatusReply, id),
            status_reply_request::ID - PAYLOAD
        );
    }

    #[test]
    fn test_reply_layouts_match_the_c_structures() {
        assert_eq!(size_of::<MessengerInitReply>(), UNION_SIZE);
        assert_eq!(offset_of!(MessengerInitReply, id), init_reply::ID - PAYLOAD);
        assert_eq!(offset_of!(MessengerInitReply, link), init_reply::LINK - PAYLOAD);
        assert_eq!(offset_of!(MessengerInitReply, media), init_reply::MEDIA - PAYLOAD);
        assert_eq!(offset_of!(MessengerInitReply, caps), init_reply::CAPS - PAYLOAD);
        assert_eq!(offset_of!(MessengerInitReply, name), init_reply::NAME - PAYLOAD);
        assert_eq!(offset_of!(MessengerInitReply, hwaddr), init_reply::HWADDR - PAYLOAD);
        assert_eq!(
            offset_of!(MessengerInitReply, hwaddr_len),
            init_reply::HWADDR_LEN - PAYLOAD
        );
        assert_eq!(
            offset_of!(MessengerInitReply, max_send),
            init_reply::MAX_SEND - PAYLOAD
        );
        assert_eq!(
            offset_of!(MessengerInitReply, max_recv),
            init_reply::MAX_RECV - PAYLOAD
        );

        assert_eq!(size_of::<MessengerReply>(), UNION_SIZE);
        assert_eq!(offset_of!(MessengerReply, id), ndev_reply::ID - PAYLOAD);
        assert_eq!(offset_of!(MessengerReply, result), ndev_reply::RESULT - PAYLOAD);

        assert_eq!(size_of::<MessengerStatus>(), UNION_SIZE);
        assert_eq!(offset_of!(MessengerStatus, id), status::ID - PAYLOAD);
        assert_eq!(offset_of!(MessengerStatus, link), status::LINK - PAYLOAD);
        assert_eq!(offset_of!(MessengerStatus, media), status::MEDIA - PAYLOAD);
        assert_eq!(offset_of!(MessengerStatus, oerror), status::OERROR - PAYLOAD);
        assert_eq!(offset_of!(MessengerStatus, coll), status::COLL - PAYLOAD);
        assert_eq!(offset_of!(MessengerStatus, ierror), status::IERROR - PAYLOAD);
        assert_eq!(offset_of!(MessengerStatus, iqdrop), status::IQDROP - PAYLOAD);
    }

    #[test]
    fn test_transfer_vector_round_trip() {
        let mut msg = Message::default();
        write_u32(&mut msg, transfer_request::ID, 0x1234_5678);
        write_u32(&mut msg, transfer_request::COUNT, 2);
        write_i32(&mut msg, transfer_request::GRANT, 7);
        write_i32(&mut msg, transfer_request::GRANT + transfer_request::GRANT_STRIDE, 8);
        write_bytes(&mut msg, transfer_request::LEN, &60u16.to_le_bytes());
        write_bytes(
            &mut msg,
            transfer_request::LEN + transfer_request::LEN_STRIDE,
            &100u16.to_le_bytes(),
        );

        assert_eq!(read_u32(&msg, transfer_request::ID), 0x1234_5678);
        assert_eq!(read_u32(&msg, transfer_request::COUNT), 2);
        assert_eq!(read_i32(&msg, transfer_request::GRANT), 7);
        assert_eq!(
            read_i32(&msg, transfer_request::GRANT + transfer_request::GRANT_STRIDE),
            8
        );
        assert_eq!(
            read_u16(&msg, transfer_request::LEN),
            60
        );
        assert_eq!(
            read_u16(&msg, transfer_request::LEN + transfer_request::LEN_STRIDE),
            100
        );
    }

    #[test]
    fn test_init_reply_byte_fields_round_trip() {
        let mut msg = Message::default();
        write_u32(&mut msg, init_reply::LINK, 1);
        write_u8(&mut msg, init_reply::HWADDR_LEN, 6);
        write_u8(&mut msg, init_reply::MAX_SEND, 8);
        write_u8(&mut msg, init_reply::MAX_RECV, 2);
        write_array(&mut msg, init_reply::NAME, b"lance0\0");
        write_array(&mut msg, init_reply::HWADDR, &[0x52, 0x54, 0, 1, 2, 3]);

        assert_eq!(read_u32(&msg, init_reply::LINK), 1);
        assert_eq!(read_u8(&msg, init_reply::HWADDR_LEN), 6);
        assert_eq!(read_u8(&msg, init_reply::MAX_SEND), 8);
        assert_eq!(read_u8(&msg, init_reply::MAX_RECV), 2);
        assert_eq!(&read_array::<7>(&msg, init_reply::NAME), b"lance0\0");
        assert_eq!(
            read_array::<6>(&msg, init_reply::HWADDR),
            [0x52, 0x54, 0, 1, 2, 3]
        );
    }
}
