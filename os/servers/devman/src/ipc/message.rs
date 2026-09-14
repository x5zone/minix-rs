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

use alloc::vec::Vec;
use minix_types::{Endpoint, Errno, Message, MessageM4, DEVMAN_REPLY, RS_PROC_NR};

use super::{dispatch, Handler};
use crate::structs::DeviceId;

/// A fully decoded DEVMAN request: the routing enum ([`Handler`]) married
/// to its payload words. The production classifier builds this from the
/// raw message (grant-copy bytes for ADD, word fields elsewhere); the
/// `Ignored` types — unknown codes, `DEVMAN_REPLY`, the five declared-but-
/// never-cased A-6 codes — decode to `None`, and the caller sends nothing
/// (05 §2.6). Callers never touch `m4` words for DEVMAN traffic: the
/// phase table above stops at this enum.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DevmanMsg {
    /// `DEVMAN_ADD_DEV` — the already-copied grant payload (transport
    /// owns the safecopy, 05 §2.2).
    Add { body: Vec<u8> },
    /// `DEVMAN_DEL_DEV` — device id as `DEVICE_ID`/m4_l2.
    Del { device: DeviceId },
    /// `DEVMAN_BIND` — target device plus the RS-set driver endpoint
    /// (`m4_l2`/`m4_l3`, 05 §1.1 phase table).
    Bind { device: DeviceId, driver: Endpoint },
    /// `DEVMAN_UNBIND` — same word layout as BIND.
    Unbind { device: DeviceId, driver: Endpoint },
}

impl DevmanMsg {
    /// Decode one raw message into its typed shape, or `None` when no
    /// handler owns the type ([`dispatch`] → `Handler::Ignored`).
    pub fn classify(m_type: i32, body: &[u8], word2: i32, word3: Endpoint) -> Option<DevmanMsg> {
        match dispatch(m_type) {
            Handler::Add => Some(DevmanMsg::Add { body: Vec::from(body) }),
            Handler::Del => Some(DevmanMsg::Del {
                device: DeviceId(word2 as u32),
            }),
            Handler::Bind => Some(DevmanMsg::Bind {
                device: DeviceId(word2 as u32),
                driver: word3,
            }),
            Handler::Unbind => Some(DevmanMsg::Unbind {
                device: DeviceId(word2 as u32),
                driver: word3,
            }),
            Handler::Ignored => None,
        }
    }
}

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
    apply_reply_with_id(msg, res, None);
}

/// C: `do_reply` with the ADD-success DEVICE_ID fill. C splits the stamp
/// in two: the caller writes `msg->DEVMAN_DEVICE_ID` (= m4_l2) *before*
/// `do_reply` sets `DEVMAN_RESULT` (= m4_l1) and sends (device.c:270 →
/// :213-219) — the ADD reply is a two-word answer (result + new id).
/// Rust makes that implicit pre-fill an explicit parameter instead:
/// `Some(id)` reproduces the C bytes on the ADD path, `None` covers
/// DEL/BIND/UNBIND where C ships the request's stale m4 words back and
/// Rust zeroes them (hygienic equivalent, 05 §3.4).
pub fn apply_reply_with_id(msg: &mut Message, res: i32, device_id: Option<i32>) {
    msg.m_type = DEVMAN_REPLY;
    // SAFETY: union *construction* is safe Rust (only reads are unsafe);
    // all other words are zeroed (C leaves stale request words behind —
    // zeroing is the hygienic equivalent, 05 §3.4).
    msg.m_u = minix_types::MessageUnion {
        m_m4: MessageM4 {
            m4l1: res as i64,
            m4l2: device_id.map(i64::from).unwrap_or(0),
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
    use minix_types::{
        DEVMAN_ADD_BUS, DEVMAN_ADD_DEV, DEVMAN_BASE, DEVMAN_BIND, DEVMAN_DEL_DEV, DEVMAN_UNBIND,
    };

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

    #[test]
    fn classify_marries_routing_to_payload() {
        // DM-P2-2: DevmanMsg::classify is dispatch + the phase-table word
        // roles in one step — callers never touch raw words.
        let add = DevmanMsg::classify(DEVMAN_ADD_DEV, &[1, 2, 3], 0, Endpoint(0));
        assert_eq!(add, Some(DevmanMsg::Add { body: alloc::vec![1, 2, 3] }));
        assert_eq!(
            DevmanMsg::classify(DEVMAN_BIND, &[], 5, Endpoint(9)),
            Some(DevmanMsg::Bind { device: DeviceId(5), driver: Endpoint(9) })
        );
        assert_eq!(
            DevmanMsg::classify(DEVMAN_DEL_DEV, &[], 7, Endpoint(0)),
            Some(DevmanMsg::Del { device: DeviceId(7) })
        );
        // Ignored types (ADD_BUS = the first A-6 code, REPLY, garbage)
        // decode to None — the server answers that with a single Nothing.
        for t in [DEVMAN_ADD_BUS, DEVMAN_REPLY, 0, -1] {
            assert_eq!(DevmanMsg::classify(t, &[], 0, Endpoint(0)), None, "type {t}");
        }
    }

    #[test]
    fn add_reply_carries_device_id_dual_word() {
        // DM-P2-1: the ADD reply is a two-word answer — RESULT (m4_l1)
        // plus the new DEVICE_ID (m4_l2), C device.c:270 → :213-219. The
        // explicit parameter replaces C's implicit pre-fill; the None
        // path keeps the zeroing hygiene (05 §3.4).
        let mut rep = request(DEVMAN_ADD_DEV, 11, 256, 0);
        apply_reply_with_id(&mut rep, 0, Some(7));
        assert_eq!(rep.m_type, DEVMAN_REPLY);
        assert_eq!(result(&rep), 0);
        assert_eq!(rep.m_source, Endpoint(7)); // untouched, like C
        assert_eq!(unsafe { rep.m_u.m_m4 }.m4l2, 7);
        // DEL/BIND/UNBIND replies carry no id: m4_l2 zeroes (C ships the
        // request's stale word back — documented divergence, 05 §3.4).
        let mut plain = request(DEVMAN_DEL_DEV, 3, 9, 0);
        apply_reply(&mut plain, 0);
        assert_eq!(result(&plain), 0);
        assert_eq!(unsafe { plain.m_u.m_m4 }.m4l2, 0);
    }
}
