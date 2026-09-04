//! DS client library: the caller-side half of the protocol.
//!
//! Mirrors `lib/libsys/ds.c` (`minix3/minix/lib/libsys/ds.c`, 219 lines).
//! 12-ds-client-library.md.
//!
//! The module owns the pure half and nothing else: which grant size and
//! direction each call needs, how string buffers are terminated, which
//! flags each helper assembles, and which reply lanes each helper reads.
//! Transport (`cpf_grant_direct` / `_taskcall` / `cpf_revoke`, A-8) stays
//! out — `minix-sys` owns the road; this module packs the luggage.
//!
//! Three contracts future transport code must keep:
//!
//! 1. **Grant sizing** (`do_invoke_ds`, ds.c:7-33): `CHECK` and
//!    `RETRIEVE_LABEL` lend an 80-byte *writable* key buffer (the name
//!    comes back); every other call lends `strlen + 1` *readable*
//!    bytes (the name goes out).
//! 2. **NUL discipline**: `publish_str` terminates in place before
//!    lending (`value[length-1] = '\0'`, ds.c:82-87); `retrieve_str`
//!    terminates after copying back (`value[length-1] = '\0'`,
//!    ds.c:150-157). The terminator is part of the length on the wire
//!    in both directions.
//! 3. **Reply reuse** (`ds_check`, ds.c:209-219): the check reply
//!    arrives *in the request lanes* (`m_ds_req.flags` = entry type
//!    mask, `m_ds_req.owner` = publisher endpoint) — not in
//!    `m_ds_reply`.
//!
//! Single-threaded event loop: pure functions, no shared state. All
//! helpers are `no_std`-clean (no allocation, no syscalls).

use minix_types::{DS_CHECK, DS_RETRIEVE_LABEL, DS_MAX_KEYLEN, DsFlags};

/// Which way a grant leans.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrantDirection {
    /// The store reads through it (caller lends its name / bytes out).
    /// C: `CPF_READ` — ds.c:19.
    Read,
    /// The store writes through it (caller lends room for the answer).
    /// C: `CPF_WRITE` — ds.c:15.
    Write,
}

/// Size and direction of the key grant (`do_invoke_ds`, ds.c:13-19).
///
/// `CHECK` and `RETRIEVE_LABEL` receive into an 80-byte roomy buffer;
/// every other call sends `strlen(name) + 1` bytes. `name_len` is the
/// `strlen` *without* terminator; the `+ 1` is added here, once, so no
/// caller can forget it.
pub const fn key_grant(call: i32, name_len: usize) -> (usize, GrantDirection) {
    if call == DS_CHECK || call == DS_RETRIEVE_LABEL {
        (DS_MAX_KEYLEN, GrantDirection::Write)
    } else {
        (name_len + 1, GrantDirection::Read)
    }
}

/// Wire length of a string publish (`ds_publish_str`, ds.c:79-87).
///
/// `strlen + 1`: the terminator travels. Returns the length to grant.
/// The in-place `value[length-1] = '\0'` (:84) is transport code's
/// duty — see [`terminate`] for the exact byte contract.
pub const fn publish_str_len(strlen: usize) -> usize {
    strlen + 1
}

/// Wire length of a string retrieve (`ds_retrieve_str`, ds.c:150-157).
///
/// The caller offers `len_str` text bytes; one more moves for the
/// terminator (`length = len_str + 1`, :153). After the copy lands,
/// transport terminates — see [`terminate`].
pub const fn retrieve_str_len(len_str: usize) -> usize {
    len_str + 1
}

/// Enforce the terminator (`value[length-1] = '\0'`, ds.c:84/:155).
///
/// Both string helpers end by pinning a NUL over the last byte moved:
/// publish pins before lending (so the store never sees an
/// unterminated lane), retrieve pins after receiving (so the caller
/// never reads one). Empty buffers pin nothing — `length == 0` leaves
/// the lane untouched, matching C's `value[-1]`-would-be-UB avoidance
/// by construction (C never passes 0: `strlen + 1 >= 1` always).
pub fn terminate(buffer: &mut [u8], length: usize) {
    if length == 0 || buffer.is_empty() {
        return;
    }
    let at = (length - 1).min(buffer.len() - 1);
    buffer[at] = 0;
}

/// Flags each helper assembles (one line per `ds_*` entry point).
///
/// Publish helpers OR their type arm over caller ornament flags
/// (`OVERWRITE` and friends pass through untouched); retrieve and
/// delete helpers name the arm alone; subscribe and check pass flags
/// through verbatim.
pub mod flags {
    use minix_types::DsFlags;

    /// `ds_publish_label`: label arm over ornaments — ds.c:36-43.
    pub const fn publish_label(extra: DsFlags) -> DsFlags {
        DsFlags::from_bits_truncate(DsFlags::TYPE_LABEL.bits() | extra.bits())
    }

    /// `ds_publish_u32`: number arm over ornaments — ds.c:46-53.
    pub const fn publish_u32(extra: DsFlags) -> DsFlags {
        DsFlags::from_bits_truncate(DsFlags::TYPE_U32.bits() | extra.bits())
    }

    /// `ds_publish_str`: string arm over ornaments — ds.c:79-87.
    pub const fn publish_str(extra: DsFlags) -> DsFlags {
        DsFlags::from_bits_truncate(DsFlags::TYPE_STR.bits() | extra.bits())
    }

    /// `ds_publish_mem`: memory arm over ornaments — ds.c:89-92.
    pub const fn publish_mem(extra: DsFlags) -> DsFlags {
        DsFlags::from_bits_truncate(DsFlags::TYPE_MEM.bits() | extra.bits())
    }

    /// Read-back arms (`ds_retrieve_u32`, `ds_retrieve_label_endpt`,
    /// `ds_delete_*`): the arm alone — ds.c:103-197.
    pub const fn arm_only(arm: DsFlags) -> DsFlags {
        arm
    }
}

/// The check reply, read back from the request lanes (`ds_check`).
///
/// C reuses `m_ds_req.flags` (entry type mask) and `m_ds_req.owner`
/// (publisher endpoint) as the reply (ds.c:209-219). This struct names
/// that reuse so transport code cannot mistake it for `m_ds_reply`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CheckReply {
    /// Entry type mask. C: `*type = m_ds_req.flags` — ds.c:215.
    pub entry_type: DsFlags,
    /// Publisher endpoint. C: `*owner_e = m_ds_req.owner` — ds.c:216.
    pub owner: i32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use minix_types::{DS_DELETE, DS_PUBLISH, DS_RETRIEVE, DS_SUBSCRIBE};

    #[test]
    fn test_check_and_label_lend_roomy_writable_grant() {
        // CHECK / RETRIEVE_LABEL receive: 80 bytes, writable (ds.c:13-16).
        assert_eq!(key_grant(DS_CHECK, 3), (80, GrantDirection::Write));
        assert_eq!(
            key_grant(DS_RETRIEVE_LABEL, 3),
            (80, GrantDirection::Write)
        );
    }

    #[test]
    fn test_other_calls_lend_strlen_plus_one_readable() {
        // Everything else sends: strlen + 1, readable (ds.c:17-19).
        assert_eq!(key_grant(DS_PUBLISH, 3), (4, GrantDirection::Read));
        assert_eq!(key_grant(DS_RETRIEVE, 0), (1, GrantDirection::Read));
        assert_eq!(key_grant(DS_DELETE, 79), (80, GrantDirection::Read));
        assert_eq!(key_grant(DS_SUBSCRIBE, 5), (6, GrantDirection::Read));
    }

    #[test]
    fn test_string_lengths_carry_terminator() {
        assert_eq!(publish_str_len(5), 6);
        assert_eq!(retrieve_str_len(5), 6);
    }

    #[test]
    fn test_terminate_pins_last_byte() {
        // `value[length-1] = '\0'` (ds.c:84/:155).
        let mut buf = [b'a', b'b', b'c', b'd'];
        terminate(&mut buf, 4);
        assert_eq!(buf, [b'a', b'b', b'c', 0]);
        // Empty length: untouched, no underflow.
        let mut buf = [b'a'];
        terminate(&mut buf, 0);
        assert_eq!(buf, [b'a']);
    }

    #[test]
    fn test_flag_assembly() {
        assert!(flags::publish_u32(DsFlags::OVERWRITE).contains(DsFlags::TYPE_U32));
        assert!(flags::publish_u32(DsFlags::OVERWRITE).contains(DsFlags::OVERWRITE));
        assert_eq!(flags::arm_only(DsFlags::TYPE_MEM), DsFlags::TYPE_MEM);
    }
}
