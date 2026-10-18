//! Block-family message wire layouts: where each field sits on the wire.
//!
//! C correspondence: `mess_lbdev_lblockdriver_msg` (`ipc.h:338-353`) for
//! every block request and `mess_lblockdriver_lbdev_reply`
//! (`ipc.h:356-361`) for the `{ status; id; }` reply. The C structure is
//! packed on the 32-bit Minix ABI; this 64-bit rendering keeps the same
//! 56-byte union body and gives the eight-byte `request` field its
//! natural alignment, so it sits at byte 40 with four bytes of padding
//! after `user` (the same repack the character and network families use).
//!
//! [`protocol`](crate::protocol) owns the numbers; this module owns their
//! positions, pinned by `#[repr(C)]` mirrors in the tests below.

use minix_types::Message;

/// Bytes before the union body in a [`Message`] (`m_source` plus `m_type`).
pub const PAYLOAD: usize = 8;

/// Size of the union body (the 64-byte message minus source and type).
pub const UNION_SIZE: usize = 56;

/// One block request (`mess_lbdev_lblockdriver_msg`).
///
/// Every request type carries the whole flat structure; the request type
/// alone decides which fields are meaningful.
pub mod request {
    /// Byte position (`pos`, `off_t`).
    pub const POS: usize = super::PAYLOAD;
    /// Device minor (`minor`).
    pub const MINOR: usize = super::PAYLOAD + 8;
    /// Request id, echoed back in the reply (`id`).
    pub const ID: usize = super::PAYLOAD + 12;
    /// Open access mode (`access`).
    pub const ACCESS: usize = super::PAYLOAD + 16;
    /// Byte count for read/write, element count for gather/scatter
    /// (`count`).
    pub const COUNT: usize = super::PAYLOAD + 20;
    /// Buffer grant (`grant`).
    pub const GRANT: usize = super::PAYLOAD + 24;
    /// Request flags (`flags`).
    pub const FLAGS: usize = super::PAYLOAD + 28;
    /// Calling user endpoint (`user`).
    pub const USER: usize = super::PAYLOAD + 32;
    /// Control request code (`request`).
    pub const REQUEST: usize = super::PAYLOAD + 40;
}

/// The block reply (`mess_lblockdriver_lbdev_reply`).
pub mod reply {
    /// Result: bytes moved, zero, or an error code.
    pub const STATUS: usize = super::PAYLOAD;
    /// Request id echoed from the request.
    pub const ID: usize = super::PAYLOAD + 4;
}

/// The character open-close request, as far as a block driver needs it.
///
/// C: `mess_vfs_lchardriver_openclose` — a character open request that a
/// block driver refuses (`do_char_open`, `driver.c:383-403`).
pub mod char_openclose {
    /// Request id, echoed back on the character reply lane.
    pub const ID: usize = super::PAYLOAD;
}

/// Notification payload (`m_notify`).
pub mod notify {
    /// Clock timestamp (`timestamp`).
    pub const TIMESTAMP: usize = super::PAYLOAD;
    /// Hardware interrupt mask (`interrupts`).
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

/// Read a little-endian `u32`.
pub fn read_u32(msg: &Message, offset: usize) -> u32 {
    u32::from_le_bytes(read_bytes(msg, offset))
}

/// Read a little-endian `i32`.
pub fn read_i32(msg: &Message, offset: usize) -> i32 {
    i32::from_le_bytes(read_bytes(msg, offset))
}

/// Read a little-endian `u64`.
pub fn read_u64(msg: &Message, offset: usize) -> u64 {
    u64::from_le_bytes(read_bytes(msg, offset))
}

/// Read a little-endian `i64`.
pub fn read_i64(msg: &Message, offset: usize) -> i64 {
    i64::from_le_bytes(read_bytes(msg, offset))
}

/// Write a little-endian `i32`.
pub fn write_i32(msg: &mut Message, offset: usize, value: i32) {
    write_bytes(msg, offset, &value.to_le_bytes());
}

/// Write a little-endian `u32`.
pub fn write_u32(msg: &mut Message, offset: usize, value: u32) {
    write_bytes(msg, offset, &value.to_le_bytes());
}

/// Write a little-endian `u64`.
pub fn write_u64(msg: &mut Message, offset: usize, value: u64) {
    write_bytes(msg, offset, &value.to_le_bytes());
}

/// Write a little-endian `i64`.
pub fn write_i64(msg: &mut Message, offset: usize, value: i64) {
    write_bytes(msg, offset, &value.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::{offset_of, size_of};

    // C field order and types; the request field's alignment is the
    // 64-bit rendering. `_ASSERT_MSG_SIZE` becomes the size assertion.

    #[repr(C)]
    struct Messenger {
        pos: i64,
        minor: i32,
        id: i32,
        access: i32,
        count: i32,
        grant: i32,
        flags: i32,
        user: i32,
        request: u64,
        padding: [u8; 8],
    }

    #[repr(C)]
    struct MessengerReply {
        status: i32,
        id: i32,
        padding: [u8; 48],
    }

    #[test]
    fn test_request_layout_matches_the_c_structure() {
        assert_eq!(size_of::<Messenger>(), UNION_SIZE);
        assert_eq!(offset_of!(Messenger, pos), request::POS - PAYLOAD);
        assert_eq!(offset_of!(Messenger, minor), request::MINOR - PAYLOAD);
        assert_eq!(offset_of!(Messenger, id), request::ID - PAYLOAD);
        assert_eq!(offset_of!(Messenger, access), request::ACCESS - PAYLOAD);
        assert_eq!(offset_of!(Messenger, count), request::COUNT - PAYLOAD);
        assert_eq!(offset_of!(Messenger, grant), request::GRANT - PAYLOAD);
        assert_eq!(offset_of!(Messenger, flags), request::FLAGS - PAYLOAD);
        assert_eq!(offset_of!(Messenger, user), request::USER - PAYLOAD);
        assert_eq!(offset_of!(Messenger, request), request::REQUEST - PAYLOAD);
        // The widths the C header spells out; a wrong width here is how a
        // count and a grant get read as one number.
        assert_eq!(size_of::<i32>(), 4, "count/grant/flags/user are ints");
        assert_eq!(size_of::<u64>(), 8, "request is unsigned long");
    }

    #[test]
    fn test_reply_layout_matches_the_c_structure() {
        assert_eq!(size_of::<MessengerReply>(), UNION_SIZE);
        assert_eq!(offset_of!(MessengerReply, status), reply::STATUS - PAYLOAD);
        assert_eq!(offset_of!(MessengerReply, id), reply::ID - PAYLOAD);
    }

    #[test]
    fn test_count_and_grant_round_trip_independently() {
        let mut msg = Message::default();
        write_i32(&mut msg, request::MINOR, 7);
        write_i32(&mut msg, request::ID, 0x55);
        write_i32(&mut msg, request::COUNT, 512);
        write_i32(&mut msg, request::GRANT, 77);
        write_i32(&mut msg, request::FLAGS, 0x1_0000);
        write_i32(&mut msg, request::USER, 9);
        write_u64(&mut msg, request::REQUEST, 0xDEAD_BEEF);

        assert_eq!(read_u32(&msg, request::MINOR), 7);
        assert_eq!(read_i32(&msg, request::ID), 0x55);
        assert_eq!(read_i32(&msg, request::COUNT), 512);
        assert_eq!(read_i32(&msg, request::GRANT), 77);
        assert_eq!(read_i32(&msg, request::FLAGS), 0x1_0000);
        assert_eq!(read_i32(&msg, request::USER), 9);
        assert_eq!(read_u64(&msg, request::REQUEST), 0xDEAD_BEEF);
    }
}
