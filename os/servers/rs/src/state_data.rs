//! LU state-data migration (pure slice).
//!
//! Mirrors `minix3/minix/servers/rs/manager.c:174-284` (`init_state_data`)
//! and the state-data shapes in `include/minix/rs.h:58-59,88-100` +
//! `include/minix/ipc_filter.h`. 17-rs-state-data.md.
//!
//! The action hooks (`sys_datacopy`/`malloc`/`free`/`cpf_grant_direct`/
//! `cpf_revoke`/`ds_retrieve_label_endpt`) are wired through
//! 19-rs-external-interfaces.md; `cpf_reload` is the RS self-update path
//! (18-rs-self-lifecycle.md); the `init_state_data` orchestration is a
//! call site of 16-rs-live-update.md (request.c:814). This module owns the
//! data shapes, the E2BIG/EINVAL/ESRCH gates and the label→endpoint
//! resolution.

use crate::live_update::SEF_LU_STATE_EVAL;
use minix_types::{Endpoint, Errno};

/// C: `IPCF_MAX_ELEMENTS` — ipc_filter.h (`NR_SYS_PROCS` = 64, config.h:32,
/// × 2).
pub const IPCF_MAX_ELEMENTS: usize = 128;

/// C: `sizeof(struct rs_state_data)` — rs.h:93-100, **x86-64 target layout**
/// (fields: size 8 + ptr 8 + size 8 + int 4 + ptr 8 + size 8 + int 4 = 48,
/// plus two 4-byte alignment pads before the pointers and a 4-byte tail
/// pad → 56). The C source tree is i386 where this struct is 28 bytes;
/// the rewrite targets x86-64, so the wire constant is 56 (ARCH A-14,
/// 17-rs-state-data.md §3.2).
///
/// The size gate in `init_state_data` (manager.c:190) compares against this
/// C wire size; the Rust struct is a pure decision model, not C-layout
/// compatible.
pub const RS_STATE_DATA_SIZE: usize = 56;

/// C: `sizeof(struct rs_ipc_filter_el)` — rs.h:88-92: `int flags` +
/// `char m_label[16]` + `int m_type` = 24 bytes.
pub const RS_IPCF_FILTER_EL_SIZE: usize = 24;

/// C: `rs_ipc_filter_size` — manager.c:181: a filter block is
/// `IPCF_MAX_ELEMENTS` elements of `rs_ipc_filter_el`.
pub const RS_IPCF_FILTER_BLOCK_SIZE: usize = IPCF_MAX_ELEMENTS * RS_IPCF_FILTER_EL_SIZE;

/// C: `sizeof(ipc_filter_el_t)` — ipc_filter.h: three `int`s.
pub const IPCF_EL_SIZE: usize = 12;

/// C: `VM_RS_UPDATE` — com.h:736 (`VM_RQ_BASE` = 0xC00, com.h:627, + 41).
pub const VM_RS_UPDATE: i32 = 0xC29;

/// C: `ANY_USR` — ipc_filter.h: `_ENDPOINT(1, _ENDPOINT_P(ANY))`.
pub const ANY_USR: Endpoint = Endpoint::from_generation_slot(1, Endpoint::ANY.slot());
/// C: `ANY_SYS` — ipc_filter.h: `_ENDPOINT(2, _ENDPOINT_P(ANY))`.
pub const ANY_SYS: Endpoint = Endpoint::from_generation_slot(2, Endpoint::ANY.slot());
/// C: `ANY_TSK` — ipc_filter.h: `_ENDPOINT(3, _ENDPOINT_P(ANY))`.
pub const ANY_TSK: Endpoint = Endpoint::from_generation_slot(3, Endpoint::ANY.slot());

bitflags::bitflags! {
    /// IPC filter element flags.
    ///
    /// C: `IPCF_*` — ipc_filter.h.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct IpcfFlags: i32 {
        /// Match on the message source.
        /// C: `IPCF_MATCH_M_SOURCE` — ipc_filter.h.
        const MATCH_M_SOURCE = 0x1;
        /// Match on the message type.
        /// C: `IPCF_MATCH_M_TYPE` — ipc_filter.h.
        const MATCH_M_TYPE = 0x2;
        /// Black-list element.
        /// C: `IPCF_EL_BLACKLIST` — ipc_filter.h.
        const EL_BLACKLIST = 0x4;
        /// White-list element.
        /// C: `IPCF_EL_WHITELIST` — ipc_filter.h.
        const EL_WHITELIST = 0x8;
    }
}

/// A service-declared filter element (label form).
///
/// C: `struct rs_ipc_filter_el` — rs.h:88-92. The `m_label` is the raw
/// string form (DS label, `ANY_*` or a decimal endpoint); the wire copy is
/// wired at 19.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceIpcFilterEl<'a> {
    /// C: `flags` — the `IPCF_*` flags.
    pub flags: IpcfFlags,
    /// C: `m_label` — the message-source label (rs.h:90). UTF-8 contract
    /// (R11): C stores raw bytes; Rust requires UTF-8 at the message
    /// boundary (19), non-UTF-8 labels are rejected fail-closed — the DS
    /// label namespace is service names (ASCII in practice), and
    /// `parse::<i32>()`/`strcmp`-style matching stay type-safe.
    pub m_label: &'a str,
    /// C: `m_type` — the message type to match.
    pub m_type: i32,
}

/// A parsed filter element (endpoint form, kernel layout).
///
/// C: `ipc_filter_el_t` — ipc_filter.h. `m_source` is meaningful only when
/// `MATCH_M_SOURCE` is set; the Rust default `Endpoint::NONE` replaces C's
/// 0 (the PM endpoint) so an unset source can never be mistaken for a
/// valid endpoint (ARCH A-14, 17-rs-state-data.md §4.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IpcFilterEl {
    /// C: `flags` — the `IPCF_*` flags.
    pub flags: IpcfFlags,
    /// C: `m_source` — the resolved message source.
    pub m_source: Endpoint,
    /// C: `m_type` — the message type to match.
    pub m_type: i32,
}

/// The `init_state_data` whole-package size gate.
///
/// C: manager.c:190 — `src->size != sizeof(struct rs_state_data)` →
/// `E2BIG`.
pub fn validate_state_data_size(size: usize) -> Result<(), Errno> {
    if size != RS_STATE_DATA_SIZE {
        Err(Errno::E2BIG)
    } else {
        Ok(())
    }
}

/// The eval-expression precondition.
///
/// C: manager.c:196-198 — `SEF_LU_STATE_EVAL` with a missing or empty
/// `eval_addr`/`eval_len` → `EINVAL`. Other prepare states skip eval
/// migration entirely.
pub fn validate_eval(
    prepare_state: i32,
    has_eval_addr: bool,
    eval_len: usize,
) -> Result<(), Errno> {
    if prepare_state == SEF_LU_STATE_EVAL && (eval_len == 0 || !has_eval_addr) {
        Err(Errno::EINVAL)
    } else {
        Ok(())
    }
}

/// Counts the filter blocks in a source buffer.
///
/// C: manager.c:213-216 — `ipcf_els_size % rs_ipc_filter_size` (a block is
/// `IPCF_MAX_ELEMENTS` elements) → `E2BIG`; otherwise the block count.
pub fn num_ipc_filter_blocks(ipcf_els_size: usize) -> Result<usize, Errno> {
    if !ipcf_els_size.is_multiple_of(RS_IPCF_FILTER_BLOCK_SIZE) {
        Err(Errno::E2BIG)
    } else {
        Ok(ipcf_els_size / RS_IPCF_FILTER_BLOCK_SIZE)
    }
}

/// Size of the destination filter buffer.
///
/// C: manager.c:218-222 — `sizeof(ipc_filter_el_t) * IPCF_MAX_ELEMENTS` per
/// block, plus one extra block when the source is VM (the fallback entry).
pub fn ipcf_els_buff_size(num_filters: usize, src_is_vm: bool) -> usize {
    let blocks = num_filters + usize::from(src_is_vm);
    IPCF_EL_SIZE * IPCF_MAX_ELEMENTS * blocks
}

/// Resolves a filter label to an endpoint.
///
/// C: manager.c:238-261 — the four mutually-exclusive fallbacks: DS lookup
/// (`ds_retrieve_label_endpt`, external contract wired at 19), the `ANY_*`
/// special sources, a decimal endpoint (`strtol`, full-string consumption +
/// no overflow), else `ESRCH`.
pub fn parse_label(
    label: &str,
    ds_lookup: impl Fn(&str) -> Option<Endpoint>,
) -> Result<Endpoint, Errno> {
    if let Some(ep) = ds_lookup(label) {
        return Ok(ep);
    }
    match label {
        "ANY_USR" => return Ok(ANY_USR),
        "ANY_SYS" => return Ok(ANY_SYS),
        "ANY_TSK" => return Ok(ANY_TSK),
        _ => {}
    }
    // C: strtol(label, &buff, 10); errno || *buff != "" → ESRCH
    // (manager.c:260-263). `parse::<i32>()` consumes the whole string and
    // fails on overflow, matching both checks. Empty-label divergence
    // (ARCH A-14): C's strtol("") returns 0 with no error (→ PM endpoint),
    // Rust fails closed with ESRCH. UTF-8 divergence (R11): C matches raw
    // bytes, Rust requires UTF-8 (fail-closed at the 19 boundary).
    label.parse::<i32>().map(Endpoint).map_err(|_| Errno::ESRCH)
}

/// Parses one source filter element into kernel form.
///
/// C: manager.c:240-270 — `m_source` is resolved only when
/// `MATCH_M_SOURCE` is set, `m_type` is copied only when `MATCH_M_TYPE` is
/// set; otherwise the fields keep their defaults.
pub fn parse_filter_el(
    el: &SourceIpcFilterEl<'_>,
    ds_lookup: impl Fn(&str) -> Option<Endpoint>,
) -> Result<IpcFilterEl, Errno> {
    let m_source = if el.flags.contains(IpcfFlags::MATCH_M_SOURCE) {
        parse_label(el.m_label, ds_lookup)?
    } else {
        Endpoint::NONE
    };
    let m_type = if el.flags.contains(IpcfFlags::MATCH_M_TYPE) {
        el.m_type
    } else {
        0
    };
    Ok(IpcFilterEl {
        flags: el.flags,
        m_source,
        m_type,
    })
}

/// The VM fallback entry appended for VM sources.
///
/// C: manager.c:271-277 — `WHITELIST | MATCH_M_SOURCE | MATCH_M_TYPE`,
/// source = RS, type = `VM_RS_UPDATE`: VM must still reach RS during the
/// update even if its own filter rules would filter RS out.
pub fn vm_fallback_entry() -> IpcFilterEl {
    IpcFilterEl {
        flags: IpcfFlags::EL_WHITELIST | IpcfFlags::MATCH_M_SOURCE | IpcfFlags::MATCH_M_TYPE,
        m_source: Endpoint::RS,
        m_type: VM_RS_UPDATE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_state_data_size() {
        // C: manager.c:190 — size != sizeof(rs_state_data) → E2BIG.
        assert_eq!(validate_state_data_size(RS_STATE_DATA_SIZE), Ok(()));
        assert_eq!(validate_state_data_size(0), Err(Errno::E2BIG));
        assert_eq!(validate_state_data_size(55), Err(Errno::E2BIG));
        assert_eq!(validate_state_data_size(57), Err(Errno::E2BIG));
    }

    #[test]
    fn test_validate_eval() {
        // C: manager.c:196-198 — EVAL + missing addr/len → EINVAL.
        assert_eq!(
            validate_eval(SEF_LU_STATE_EVAL, false, 0),
            Err(Errno::EINVAL)
        );
        assert_eq!(
            validate_eval(SEF_LU_STATE_EVAL, true, 0),
            Err(Errno::EINVAL)
        );
        assert_eq!(
            validate_eval(SEF_LU_STATE_EVAL, false, 8),
            Err(Errno::EINVAL)
        );
        assert_eq!(validate_eval(SEF_LU_STATE_EVAL, true, 8), Ok(()));
        // Non-EVAL states skip the eval migration.
        assert_eq!(validate_eval(0, false, 0), Ok(()));
        assert_eq!(validate_eval(1, true, 8), Ok(()));
    }

    #[test]
    fn test_num_ipc_filter_blocks() {
        // C: manager.c:213-216 — block = 24 * 128 bytes; non-multiple → E2BIG.
        assert_eq!(num_ipc_filter_blocks(0), Ok(0));
        assert_eq!(num_ipc_filter_blocks(RS_IPCF_FILTER_BLOCK_SIZE), Ok(1));
        assert_eq!(num_ipc_filter_blocks(2 * RS_IPCF_FILTER_BLOCK_SIZE), Ok(2));
        assert_eq!(
            num_ipc_filter_blocks(RS_IPCF_FILTER_BLOCK_SIZE - 1),
            Err(Errno::E2BIG)
        );
        assert_eq!(
            num_ipc_filter_blocks(RS_IPCF_FILTER_BLOCK_SIZE + 1),
            Err(Errno::E2BIG)
        );
    }

    #[test]
    fn test_ipcf_els_buff_size() {
        // C: manager.c:218-222 — VM adds one extra block.
        let block = IPCF_EL_SIZE * IPCF_MAX_ELEMENTS;
        assert_eq!(ipcf_els_buff_size(1, false), block);
        assert_eq!(ipcf_els_buff_size(1, true), 2 * block);
        assert_eq!(ipcf_els_buff_size(3, false), 3 * block);
    }

    #[test]
    fn test_parse_label() {
        // C: manager.c:238-261 — DS hit first, then ANY_*, then decimal.
        let ds = |label: &str| -> Option<Endpoint> { (label == "vm").then_some(Endpoint::VM) };
        assert_eq!(parse_label("vm", ds), Ok(Endpoint::VM));
        assert_eq!(parse_label("ANY_USR", ds), Ok(ANY_USR));
        assert_eq!(parse_label("ANY_SYS", ds), Ok(ANY_SYS));
        assert_eq!(parse_label("ANY_TSK", ds), Ok(ANY_TSK));
        // Decimal endpoint; strtol consumes the whole string.
        assert_eq!(parse_label("2", ds), Ok(Endpoint::RS));
        assert_eq!(parse_label("0", ds), Ok(Endpoint::PM));
        // Trailing garbage / non-numeric / overflow → ESRCH.
        assert_eq!(parse_label("123x", ds), Err(Errno::ESRCH));
        assert_eq!(parse_label("abc", ds), Err(Errno::ESRCH));
        assert_eq!(parse_label("999999999999", ds), Err(Errno::ESRCH));
        assert_eq!(parse_label("", ds), Err(Errno::ESRCH));
    }

    #[test]
    fn test_parse_filter_el() {
        // C: manager.c:229-267 — MATCH flags gate the fields.
        let ds = |_label: &str| None;
        let el = SourceIpcFilterEl {
            flags: IpcfFlags::MATCH_M_SOURCE | IpcfFlags::MATCH_M_TYPE,
            m_label: "ANY_SYS",
            m_type: 5,
        };
        let parsed = parse_filter_el(&el, ds).unwrap();
        assert_eq!(parsed.m_source, ANY_SYS);
        assert_eq!(parsed.m_type, 5);
        assert_eq!(parsed.flags, el.flags);

        // Source-only match: type keeps its default.
        let src_only = SourceIpcFilterEl {
            flags: IpcfFlags::MATCH_M_SOURCE,
            m_label: "2",
            m_type: 5,
        };
        let parsed = parse_filter_el(&src_only, ds).unwrap();
        assert_eq!(parsed.m_source, Endpoint::RS);
        assert_eq!(parsed.m_type, 0);

        // No match flags: both fields keep their defaults.
        let no_match = SourceIpcFilterEl {
            flags: IpcfFlags::EL_WHITELIST,
            m_label: "ANY_USR",
            m_type: 5,
        };
        let parsed = parse_filter_el(&no_match, ds).unwrap();
        assert_eq!(parsed.m_source, Endpoint::NONE);
        assert_eq!(parsed.m_type, 0);

        // Unresolvable label with MATCH_M_SOURCE → ESRCH.
        let bad = SourceIpcFilterEl {
            flags: IpcfFlags::MATCH_M_SOURCE,
            m_label: "no-such-label",
            m_type: 0,
        };
        assert_eq!(parse_filter_el(&bad, ds), Err(Errno::ESRCH));
    }

    #[test]
    fn test_vm_fallback_entry() {
        // C: manager.c:271-277.
        let el = vm_fallback_entry();
        assert_eq!(
            el.flags,
            IpcfFlags::EL_WHITELIST | IpcfFlags::MATCH_M_SOURCE | IpcfFlags::MATCH_M_TYPE
        );
        assert_eq!(el.m_source, Endpoint::RS);
        assert_eq!(el.m_type, VM_RS_UPDATE);
    }

    #[test]
    fn test_any_endpoints() {
        // C: ipc_filter.h — _ENDPOINT(1..3, _ENDPOINT_P(ANY)); with
        // MAX_NR_TASKS = 1023 (com.h:55), ANY = 0x7C00.
        assert_eq!(ANY_USR.get(), 0xFC00);
        assert_eq!(ANY_SYS.get(), 0x17C00);
        assert_eq!(ANY_TSK.get(), 0x1FC00);
    }

    #[test]
    fn test_constants() {
        // C: ipc_filter.h + config.h:32 + rs.h:58 + sef.h:217 + com.h:736.
        assert_eq!(IPCF_MAX_ELEMENTS, 128);
        assert_eq!(crate::service_slot::RS_MAX_LABEL_LEN, 16);
        assert_eq!(RS_STATE_DATA_SIZE, 56);
        assert_eq!(RS_IPCF_FILTER_EL_SIZE, 24);
        assert_eq!(RS_IPCF_FILTER_BLOCK_SIZE, 3072);
        assert_eq!(IPCF_EL_SIZE, 12);
        assert_eq!(SEF_LU_STATE_EVAL, 4);
        assert_eq!(VM_RS_UPDATE, 0xC29);
    }
}
