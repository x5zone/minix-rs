//! The VBOX message protocol: request and reply numbers plus the field
//! offsets inside the IPC message.
//!
//! C correspondence: `minix3/minix/include/minix/vboxif.h` (the two base
//! numbers, the four requests, the one reply, and the field macros
//! `VBOX_CONN`/`VBOX_GRANT`/`VBOX_COUNT`/`VBOX_RESULT`/`VBOX_CODE`/
//! `VBOX_FUNCTION`/`VBOX_ID`). The C macros name fields of the `mess_2`
//! union; the offsets here are derived from the Rust union's own
//! [`minix_types::MessageM2`] layout, whose `m2i1`..`m2l2` slots sit at
//! union bytes 0, 4, 8, 16 and 24 (C's `mess_2` carries an extra leading
//! `m2ll1` word, so its absolute numbers differ by eight; both ends of
//! this protocol are Rust, and the test below pins the concrete values so
//! a layout change cannot pass silently).

/// The request range's base (`VBOX_RQ_BASE`).
pub const VBOX_REQUEST_BASE: i32 = 0x1600;
/// The reply range's base (`VBOX_RS_BASE`).
pub const VBOX_REPLY_BASE: i32 = 0x1680;

/// Open a connection (`VBOX_OPEN`).
pub const VBOX_OPEN: i32 = VBOX_REQUEST_BASE;
/// Close a connection (`VBOX_CLOSE`).
pub const VBOX_CLOSE: i32 = VBOX_REQUEST_BASE + 1;
/// Perform a call (`VBOX_CALL`).
pub const VBOX_CALL: i32 = VBOX_REQUEST_BASE + 2;
/// Cancel an ongoing call (`VBOX_CANCEL`).
pub const VBOX_CANCEL: i32 = VBOX_REQUEST_BASE + 3;
/// The single reply type (`VBOX_REPLY`).
pub const VBOX_REPLY: i32 = VBOX_REPLY_BASE;

/// The message union payload begins at byte 8 of [`minix_types::Message`].
pub const PAYLOAD: usize = 8;

/// Field offsets inside the message.
pub mod cwire {
    use super::PAYLOAD;
    use core::mem::offset_of;

    /// `VBOX_CONN` = `m2_i1`: connection identifier.
    pub const CONN: usize = PAYLOAD + offset_of!(minix_types::MessageM2, m2i1);
    /// `VBOX_GRANT` = `m2_i2`: grant of a buffer or name.
    pub const GRANT: usize = PAYLOAD + offset_of!(minix_types::MessageM2, m2i2);
    /// `VBOX_COUNT` = `m2_i3`: bytes or elements.
    pub const COUNT: usize = PAYLOAD + offset_of!(minix_types::MessageM2, m2i3);
    /// `VBOX_FUNCTION` = `m2_l1`: the function call number.
    pub const FUNCTION: usize = PAYLOAD + offset_of!(minix_types::MessageM2, m2l1);
    /// `VBOX_ID` = `m2_l2`: the opaque request identifier.
    pub const ID: usize = PAYLOAD + offset_of!(minix_types::MessageM2, m2l2);
    /// `VBOX_RESULT` = `m2_i1` on a reply: the POSIX result.
    pub const RESULT: usize = CONN;
    /// `VBOX_CODE` = `m2_i2` on a reply: the device result code.
    pub const CODE: usize = GRANT;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_numbers_match_vboxif_header() {
        assert_eq!(VBOX_REQUEST_BASE, 0x1600);
        assert_eq!(VBOX_REPLY_BASE, 0x1680);
        assert_eq!(VBOX_OPEN, 0x1600);
        assert_eq!(VBOX_CLOSE, 0x1601);
        assert_eq!(VBOX_CALL, 0x1602);
        assert_eq!(VBOX_CANCEL, 0x1603);
        assert_eq!(VBOX_REPLY, 0x1680);
        // IS_VBOX_RQ/IS_VBOX_RS use a 0x7f mask over the base.
        assert_eq!(VBOX_OPEN & !0x7f, VBOX_REQUEST_BASE);
        assert_eq!(VBOX_REPLY & !0x7f, VBOX_REPLY_BASE);
    }

    #[test]
    fn test_field_offsets_pin_the_rust_union_layout() {
        // Rust MessageM2: m2i1 @0, m2i2 @4, m2i3 @8, m2l1 @16, m2l2 @24
        // inside the union; the union starts at message byte 8.
        assert_eq!(cwire::CONN, 8);
        assert_eq!(cwire::GRANT, 12);
        assert_eq!(cwire::COUNT, 16);
        assert_eq!(cwire::FUNCTION, 24);
        assert_eq!(cwire::ID, 32);
        assert_eq!(cwire::RESULT, cwire::CONN);
        assert_eq!(cwire::CODE, cwire::GRANT);
    }
}
