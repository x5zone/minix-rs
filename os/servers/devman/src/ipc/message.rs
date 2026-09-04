//! DEVMAN message shapes: field accessors and reply construction
//! (doc 05-devm-message-contract).
//!
//! C: `minix/include/minix/com.h:846-866` field macros +
//! `device.c:213-219` (`do_reply`) + `bind.c:11,63` (RS-only gate).
//! Numeric authority lives in `minix_types` (`DEVMAN_*`, `RS_PROC_NR`);
//! this module is the *role-dependent view* over the shared `m4` words:
//!
//! | Phase | m4l1 | m4l2 | m4l3 |
//! |---|---|---|---|
//! | ADD/DEL request | `GRANT_ID` | `GRANT_SIZE` | — |
//! | BIND/UNBIND request | — | `DEVICE_ID` | `ENDPOINT` (RS-set, forwarded) |
//! | reply | `RESULT` | (`DEVICE_ID` on ADD success) | — |
//!
//! Same words, different roles per phase — the table above is the whole
//! reason this module exists instead of raw `m4l*` reads at call sites.

use minix_types::{Endpoint, Errno, Message, MessageM4, DEVMAN_REPLY, RS_PROC_NR};

/// Read the `m4` payload view of a message.
///
/// SAFETY: Minix IPC always delivers full 64-byte messages, so every
/// payload view is backed by real bytes; `MessageM4` is plain-data
/// `repr(C)`. Same precedent as `Message::payload_ref` in `minix-types`.
fn m4(msg: &Message) -> MessageM4 {
    unsafe { msg.m_u.m_m4 }
}

/// C: `msg->DEVMAN_GRANT_ID` (`m4_l1`, com.h:859).
/// Wire values are 32-bit by protocol; the `i64 → i32` cast is
/// value-preserving for in-range senders (same narrowing the kernel
/// applies reading `m4_l1` as `long` on 32-bit Minix3).
pub fn grant_id(msg: &Message) -> i32 {
    m4(msg).m4l1 as i32
}

/// C: `msg->DEVMAN_GRANT_SIZE` (`m4_l2`, com.h:860).
pub fn grant_size(msg: &Message) -> i32 {
    m4(msg).m4l2 as i32
}

/// C: `msg->DEVMAN_ENDPOINT` (`m4_l3`, com.h:862) — set by RS on
/// BIND/UNBIND, forwarded by devman to the driver (09).
pub fn request_endpoint(msg: &Message) -> Endpoint {
    Endpoint(m4(msg).m4l3 as i32)
}

/// C: `msg->DEVMAN_DEVICE_ID` (`m4_l2`, com.h:863) — same word as
/// `GRANT_SIZE`, reply/request-role-dependent (see module table).
pub fn device_id(msg: &Message) -> i32 {
    m4(msg).m4l2 as i32
}

/// C: `msg->DEVMAN_RESULT` (`m4_l1`, com.h:864) — same word as `GRANT_ID`.
pub fn result(msg: &Message) -> i32 {
    m4(msg).m4l1 as i32
}

/// C: `do_reply` (device.c:213-219) — stamp `DEVMAN_REPLY` + result onto
/// the *incoming* message (mutated in place, like C; `m_source` is
/// untouched so the transport still knows the destination).
/// The actual send is transport business (`ipc_send`, async — C never
/// `sendrec`s a reply); 07–09 call this then hand the message out.
pub fn apply_reply(msg: &mut Message, res: i32) {
    msg.m_type = DEVMAN_REPLY;
    // SAFETY: union *construction* is safe Rust (only reads are unsafe);
    // all other words are zeroed (C leaves stale request words behind —
    // zeroing is the hygienic equivalent, 05 §3.4).
    msg.m_u = minix_types::MessageUnion {
        m_m4: MessageM4 {
            m4l1: res as i64,
            ..MessageM4::default()
        },
    };
}

/// C: `src != RS_PROC_NR → EPERM` (bind.c:14,63, `src = m->m_source`).
/// Subtlety callers must honor: on `Err`, C writes `RESULT = EPERM` into
/// the message and returns **without sending any reply** — the sender
/// hears nothing. So `Err` means "stamp EPERM for the record, send
/// nothing" (09 implements exactly this; 05 §3.3 locks it in a test).
pub fn check_rs(source: Endpoint) -> Result<(), Errno> {
    if source == RS_PROC_NR {
        Ok(())
    } else {
        Err(Errno::EPERM)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minix_types::{DEVMAN_ADD_DEV, DEVMAN_BASE, DEVMAN_BIND, DEVMAN_DEL_DEV, DEVMAN_UNBIND};

    fn request(m_type: i32, l1: i64, l2: i64, l3: i64) -> Message {
        Message {
            m_source: Endpoint(7),
            m_type,
            m_u: minix_types::MessageUnion {
                m_m4: MessageM4 {
                    m4l1: l1,
                    m4l2: l2,
                    m4l3: l3,
                    ..MessageM4::default()
                },
            },
        }
    }

    #[test]
    fn consts_match_com_h() {
        // minix-types is the numeric authority (test_devman_messages);
        // here: the devman view imports the same values.
        assert_eq!(DEVMAN_BASE, 0x1200);
        assert_eq!(DEVMAN_ADD_DEV + 1, DEVMAN_DEL_DEV);
        assert_eq!(DEVMAN_BIND + 1, DEVMAN_UNBIND);
    }

    #[test]
    fn field_views_share_words_per_phase() {
        // ADD request phase: grant words.
        let add = request(DEVMAN_ADD_DEV, 11, 256, 0);
        assert_eq!(grant_id(&add), 11);
        assert_eq!(grant_size(&add), 256);
        // Reply phase: same words read as result / device id.
        let mut rep = add;
        apply_reply(&mut rep, 0);
        assert_eq!(rep.m_type, DEVMAN_REPLY);
        assert_eq!(result(&rep), 0);
        assert_eq!(rep.m_source, Endpoint(7)); // untouched, like C
        // BIND request phase: endpoint + device id.
        let bind = request(DEVMAN_BIND, 0, 5, 9);
        assert_eq!(device_id(&bind), 5);
        assert_eq!(request_endpoint(&bind), Endpoint(9));
    }

    #[test]
    fn rs_gate_permits_only_rs() {
        assert_eq!(check_rs(RS_PROC_NR), Ok(()));
        assert_eq!(check_rs(Endpoint(7)), Err(Errno::EPERM));
        assert_eq!(check_rs(Endpoint(0)), Err(Errno::EPERM));
    }
}
