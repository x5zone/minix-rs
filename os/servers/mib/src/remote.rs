//! Remote endpoint verdicts: slots, labels, replies, death.
//!
//! Mirrors the pure halves of `mib_remote_init` / `mib_down` /
//! `mib_get_label` / `mib_do_register` / `mib_register` /
//! `mib_do_deregister` / `mib_deregister` / `mib_remote_info` /
//! `mib_remote_call` (`remote.c:24-477`). Table surgery, label fetch,
//! grant creation, and `ipc_sendrec` are arena/transport effects; slot
//! choice, bounds, reply checks, and outcome mapping are judged here.
//!
//! 12-mib-remote-subtrees.md.

use minix_types::{EDONTREPLY, ENAMETOOLONG, ENOSYS};

/// Max remote services (endpoint table size).
/// C: `MIB_ENDPTS (1U << MIB_EID_BITS)` — remote.c:25.
pub const MIB_ENDPTS: usize = 32;

/// Max service label size, NUL included. C: `MIB_LABEL_MAX` — remote.c:28.
pub const MIB_LABEL_MAX: usize = 16;

/// One endpoint-table row, as the verdicts see it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EndptSlot {
    /// Occupant endpoint (`None` = `NONE`/free). C: `endpts[i].endpt`.
    pub endpt: Option<i32>,
    /// Registered label. C: `endpts[i].label` (only meaningful if occupied).
    pub label: Label,
}

/// Fixed 16-byte label (NUL-terminated C string on the wire).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Label {
    /// Bytes including the terminator.
    pub bytes: [u8; MIB_LABEL_MAX],
}

impl Label {
    /// Build from a byte string; overlong labels refuse (`ENAMETOOLONG`
    /// mirrors `mib_get_label`'s bound, remote.c:95-99 — the DS key is
    /// longer, the *slot* holds 16).
    pub const fn from_bytes(src: &[u8]) -> Option<Self> {
        if src.len() + 1 > MIB_LABEL_MAX {
            return None;
        }
        let mut bytes = [0u8; MIB_LABEL_MAX];
        let mut i = 0;
        while i < src.len() {
            bytes[i] = src[i];
            i += 1;
        }
        Some(Self { bytes })
    }

    /// Byte-wise equality (labels compare by content, remote.c:129).
    pub const fn equals(self, other: Self) -> bool {
        let mut i = 0;
        while i < MIB_LABEL_MAX {
            if self.bytes[i] != other.bytes[i] {
                return false;
            }
            i += 1;
        }
        true
    }
}

/// Where a registering service lands in the table.
///
/// C: `mib_do_register` slot search — remote.c:124-148. Same endpoint =
/// re-register (slot kept); same label on a *different* endpoint = the
/// old one died, reap first (`mib_down`, :128-136); otherwise first free
/// slot; none free = table full (logged, silently dropped — one-way).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotVerdict {
    /// Known endpoint: reuse the slot. C: `:126-127`.
    Reuse {
        /// Table index.
        eid: usize,
    },
    /// Same label, new endpoint: reap this slot first, then reuse it.
    /// C: `:128-136` (`mib_down` + asserts emptied).
    ReapThenReuse {
        /// Table index to reap.
        eid: usize,
    },
    /// Fresh service: take this free slot. C: `:136-147`.
    Fresh {
        /// Table index.
        eid: usize,
    },
    /// Table full: drop the request (one-way, nobody to tell).
    /// C: `:140-148` (printf + return).
    Full,
}

/// Judge the slot for `(endpt, label)` over the table.
pub fn locate_slot(slots: &[EndptSlot], endpt: i32, label: Label) -> SlotVerdict {
    let mut free: Option<usize> = None;
    let mut i = 0;
    while i < slots.len() {
        match slots[i].endpt {
            Some(e) if e == endpt => return SlotVerdict::Reuse { eid: i },
            Some(_) if slots[i].label.equals(label) => {
                return SlotVerdict::ReapThenReuse { eid: i };
            }
            None => {
                if free.is_none() {
                    free = Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    match free {
        Some(eid) => SlotVerdict::Fresh { eid },
        None => SlotVerdict::Full,
    }
}

/// Judge a register message's call shape (one-way gate).
///
/// C: `mib_register` head — remote.c:210-211. Blocking senders hear
/// `ENOSYS` (crossed traffic would deadlock, :202-209); everyone else
/// proceeds to the label check. Deregister shares the gate (:295-297).
pub const fn register_gate(is_sendrec: bool) -> Result<(), i32> {
    if is_sendrec {
        return Err(ENOSYS);
    }
    Ok(())
}

/// Judge the message-level mount-path bound.
///
/// C: `mib_register` bounds check — remote.c:221-224. Past the `mib[8]`
/// lane count the request is silently dropped (`EDONTREPLY`, one-way).
/// (02's `MountRequest::decode_register` judges the same bound at the
/// wire layer; this is the message-layer restatement where C restates it.)
pub const fn register_bound_ok(miblen: u32) -> Result<(), i32> {
    if miblen > 8 {
        return Err(EDONTREPLY);
    }
    Ok(())
}

/// Locate a deregistering endpoint's slot.
///
/// C: `mib_do_deregister` search — remote.c:245-255. Unknown endpoints
/// are silently ignored (one-way, debug-logged at most).
pub fn dereg_slot(slots: &[EndptSlot], endpt: i32) -> Option<usize> {
    let mut i = 0;
    while i < slots.len() {
        if slots[i].endpt == Some(endpt) {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Judge a service reply: right type, right id, then the status.
///
/// C: `mib_remote_info` tail (:359-364) and `mib_remote_call` tail
/// (:461-464) share the shape: wrong message type → `EINVAL`, nonzero
/// `req_id` → `EINVAL` (replies echo id 0 — `:344`, `:425`, reserved for
/// future async), else the service's status travels (including
/// `ERESTART`, 10's contract).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplyCheck {
    /// Correct reply: deliver the service status.
    Deliver(i32),
    /// Wrong message type. C: `:359`, `:461` → `EINVAL`.
    WrongType,
    /// Unexpected request id. C: `:361`, `:463` → `EINVAL`.
    WrongId,
}

/// Check a reply (`is_reply` = type is `COMMON_MIB_REPLY`).
pub const fn check_reply(is_reply: bool, req_id: u32, status: i32) -> ReplyCheck {
    if !is_reply {
        return ReplyCheck::WrongType;
    }
    if req_id != 0 {
        return ReplyCheck::WrongId;
    }
    ReplyCheck::Deliver(status)
}

/// Caller-is-root flag for relayed calls: `1`/`0`, no more.
///
/// C: `m.m_mib_lsys_call.flags = !!mib_authed(call)` + `TODO: define
/// flags` — remote.c:434. The TODO is C's, quoted here so the wire bit
/// is not mistaken for a designed flag set: today it is exactly one
/// boolean.
pub const fn caller_flag(authed: bool) -> u32 {
    authed as u32
}

/// Label length verdict for the endpoint table.
///
/// C: `mib_get_label` bound — remote.c:94-99. DS keys are longer than
/// the 16-byte slot; overflow speaks `ENAMETOOLONG` ("should never
/// happen", :96 — a corrupt DS, not a user input).
pub const fn label_fits(key_len_incl_nul: usize) -> Result<(), i32> {
    if key_len_incl_nul > MIB_LABEL_MAX {
        return Err(ENAMETOOLONG);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slot(endpt: Option<i32>, name: &[u8]) -> EndptSlot {
        EndptSlot {
            endpt,
            label: Label::from_bytes(name).unwrap(),
        }
    }

    #[test]
    fn test_locate_slot() {
        // Same endpoint reuses (remote.c:126-127).
        let t = [
            slot(Some(7), b"ipc"),
            slot(None, b""),
            slot(Some(9), b"lwip"),
        ];
        assert_eq!(
            locate_slot(&t, 7, Label::from_bytes(b"ipc").unwrap()),
            SlotVerdict::Reuse { eid: 0 }
        );
        // Same label, new endpoint: reap first (:128-136).
        assert_eq!(
            locate_slot(&t, 11, Label::from_bytes(b"lwip").unwrap()),
            SlotVerdict::ReapThenReuse { eid: 2 }
        );
        // Fresh service takes the first free slot (:136-137).
        assert_eq!(
            locate_slot(&t, 12, Label::from_bytes(b"uds").unwrap()),
            SlotVerdict::Fresh { eid: 1 }
        );
        // Full table drops silently (:140-148).
        let full = [slot(Some(1), b"a"), slot(Some(2), b"b")];
        assert_eq!(
            locate_slot(&full, 3, Label::from_bytes(b"c").unwrap()),
            SlotVerdict::Full
        );
        assert_eq!(MIB_ENDPTS, 32);
    }

    #[test]
    fn test_gates_and_bounds() {
        // One-way gate shared by register/deregister (:210-211, :296-297).
        assert_eq!(register_gate(true), Err(ENOSYS));
        assert_eq!(register_gate(false), Ok(()));
        // Message-level path bound (:221-224).
        assert_eq!(register_bound_ok(8), Ok(()));
        assert_eq!(register_bound_ok(9), Err(EDONTREPLY));
        // Unknown endpoints deregister into the void (:245-255).
        let t = [slot(Some(7), b"ipc")];
        assert_eq!(dereg_slot(&t, 7), Some(0));
        assert_eq!(dereg_slot(&t, 8), None);
        // Slot labels hold 16 with NUL (:28, :94-99).
        assert_eq!(MIB_LABEL_MAX, 16);
        assert!(Label::from_bytes(b"123456789012345").is_some());
        assert!(Label::from_bytes(b"1234567890123456").is_none());
        assert_eq!(label_fits(16), Ok(()));
        assert_eq!(label_fits(17), Err(ENAMETOOLONG));
    }

    #[test]
    fn test_reply_and_flag() {
        // Wrong type, wrong id, then status travels (:359-364, :461-464).
        assert_eq!(check_reply(false, 0, 0), ReplyCheck::WrongType);
        assert_eq!(check_reply(true, 41, 0), ReplyCheck::WrongId);
        assert_eq!(check_reply(true, 0, -200), ReplyCheck::Deliver(-200));
        // Caller flag is one boolean; the TODO is C's (:434).
        assert_eq!(caller_flag(true), 1);
        assert_eq!(caller_flag(false), 0);
    }
}
