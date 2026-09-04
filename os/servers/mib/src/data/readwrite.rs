//! Data read/write verdicts: lanes, sizes, staging, bools.
//!
//! Mirrors the pure halves of `mib_getptr` / `mib_read` / `mib_write` /
//! `mib_readwrite` (`tree.c:1098-1325`). Pointer chasing, copying, and
//! freeing are arena/transport effects; every *decision* — which lane,
//! which length, scratch or heap, terminate or refuse — is judged here.
//! Verify callbacks run handler-side (09's follow-up per subtree);
//! this module judges their boolean answer.
//!
//! 09-mib-data-access.md.

use minix_types::{
    CTLFLAG_IMMEDIATE, CTLTYPE_BOOL, CTLTYPE_INT, CTLTYPE_QUAD, CTLTYPE_STRING, CTLTYPE_STRUCT,
    EINVAL, EPERM, OK, SYSCTL_TYPEMASK,
};

/// Where a node's bytes live: the read/write lane.
///
/// C: `mib_getptr` — tree.c:1101-1124. Immediate scalars live in the
/// node; strings/structs always live behind the pointer (immediate
/// strings are impossible → `NULL`, :1116-1117); unknown types have no
/// lane at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PtrLane {
    /// Value stored in the node body. C: `&node->node_bool/int/quad`.
    Immediate,
    /// Value behind `node_data`. C: `return node->node_data`.
    External,
    /// No lane: read/write refuses. C: `return NULL`.
    Absent,
}

/// Judge the lane for a flag word.
pub const fn getptr_lane(flags: u32) -> PtrLane {
    match flags & SYSCTL_TYPEMASK {
        CTLTYPE_BOOL | CTLTYPE_INT | CTLTYPE_QUAD => {
            if flags & CTLFLAG_IMMEDIATE != 0 {
                PtrLane::Immediate
            } else {
                PtrLane::External
            }
        }
        CTLTYPE_STRING | CTLTYPE_STRUCT => {
            if flags & CTLFLAG_IMMEDIATE != 0 {
                PtrLane::Absent
            } else {
                PtrLane::External
            }
        }
        _ => PtrLane::Absent,
    }
}

/// Read length: strings measure live, others report the field width.
///
/// C: `mib_read` length half — tree.c:1140-1146. Strings report
/// `strlen + 1` (the terminator travels — readers size buffers with it);
/// everything else reports `node_size`. Past `SSIZE_MAX` refuses: the
/// length travels back as `ssize_t`, so unrepresentable lengths are
/// `EINVAL`, not truncation. (`SSIZE_MAX` is 64-bit here — an ARCH
/// widening; no real node approaches it, the check is parity.)
pub const fn read_len(ty: u32, node_size: u64, live_str_len: u64) -> Result<u64, i32> {
    let len = if ty == CTLTYPE_STRING {
        live_str_len + 1
    } else {
        node_size
    };
    if len > i64::MAX as u64 {
        return Err(EINVAL);
    }
    Ok(len)
}

/// Write size check: exact for scalars/structs, capped for strings.
///
/// C: `mib_write` size half — tree.c:1187-1208. Non-strings demand an
/// exact match (a short write names different data, 06 §1.3); strings
/// demand fit (`newlen <= size` — the terminator check comes later, at
/// stage time); unknown types refuse.
pub const fn write_size_ok(ty: u32, newlen: u64, node_size: u64) -> bool {
    match ty {
        CTLTYPE_BOOL | CTLTYPE_INT | CTLTYPE_QUAD | CTLTYPE_STRUCT => newlen == node_size,
        CTLTYPE_STRING => newlen <= node_size,
        _ => false,
    }
}

/// Staging choice: scratch or heap.
///
/// C: `mib_write` staging — tree.c:1220-1242. In-place updates are
/// forbidden (a halfway-failed copy would corrupt the node, :1171-1175),
/// so bytes stage aside first: scratch fits `newlen + 1` (the +1 stages
/// a self-added terminator, :1211-1214), bigger needs a heap buffer —
/// which unprivileged callers are refused (`EPERM`, :1224-1229: page-sized
/// temp buffers per untrusted call would let anyone squeeze the server).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// Stage in the shared scratch page.
    Scratch,
    /// Stage in a fresh heap buffer (privileged only).
    Heap,
}

/// Judge the staging for a write of `newlen` bytes.
/// `scratch` is 03's `SCRATCH_SIZE` (4096).
pub const fn stage_for(newlen: u64, authed: bool, scratch: u64) -> Result<Stage, i32> {
    if newlen + 1 > scratch {
        if !authed {
            return Err(EPERM);
        }
        Ok(Stage::Heap)
    } else {
        Ok(Stage::Scratch)
    }
}

/// String finalization: terminate, then fit.
///
/// C: tree.c:1273-1288. A full-size write without a terminator refuses
/// (ours would not fit — :1274-1277); otherwise `src[newlen] = '\0'`
/// (`:1286`, safe: staging reserved the +1) and `strlcpy` keeps at most
/// `node_size`. Mid-string NULs are fine — trailing garbage never copies
/// out (`:1280-1285`, readers stop at the first NUL).
pub const fn finalize_string(newlen: u64, node_size: u64, last_is_nul: bool) -> Result<(), i32> {
    if newlen == node_size && !last_is_nul {
        return Err(EINVAL);
    }
    Ok(())
}

/// Boolean sanitize: nonzero is true, without inspecting the value.
///
/// C: `b[0] = (bool)src[0]` — tree.c:1265 (create path :707 identical).
/// Reading a `_Bool` holding e.g. 2 is UB (the comment's "cannot test
/// directly", :1255-1264 — plus the compile-time size assert `:1163`,
/// which pins `sizeof(bool) == 1` as an ABI wall); converting through a
/// byte never forms the out-of-range `_Bool`.
pub const fn sanitize_bool(byte: u8) -> bool {
    byte != 0
}

/// Judge a verify callback's answer: absent passes, `false` refuses.
///
/// C: `if (r == OK && verify != NULL && !verify(...)) r = EINVAL` —
/// tree.c:1247-1248. The call itself runs handler-side; only the verdict
/// shape lives here.
pub const fn apply_verify(answer: Option<bool>) -> Result<(), i32> {
    match answer {
        None | Some(true) => Ok(()),
        Some(false) => Err(EINVAL),
    }
}

/// Combine read-then-write: read first, write second, report the old len.
///
/// C: `mib_readwrite` — tree.c:1315-1324. Read failure short-circuits
/// (no write attempted); write failure replaces the outcome; success
/// reports the *old* length (the reply contract, 01 §2.4).
pub const fn readwrite_combine(read_len: i64, write_code: i32) -> i64 {
    if read_len < 0 {
        return read_len;
    }
    if write_code != OK {
        return write_code as i64;
    }
    read_len
}

/// Whether a flag word names a plain data leaf (readwrite's domain).
/// C: "a regular data node is a leaf node" — tree.c:1303-1306.
pub const fn is_data_leaf(flags: u32) -> bool {
    matches!(
        flags & SYSCTL_TYPEMASK,
        CTLTYPE_BOOL | CTLTYPE_INT | CTLTYPE_QUAD | CTLTYPE_STRING | CTLTYPE_STRUCT
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use minix_types::{CTLFLAG_READWRITE, CTLTYPE_NODE};

    #[test]
    fn test_getptr_lanes() {
        // Immediate scalars live inside (tree.c:1102-1113).
        assert_eq!(
            getptr_lane(CTLTYPE_INT | CTLFLAG_IMMEDIATE),
            PtrLane::Immediate
        );
        assert_eq!(
            getptr_lane(CTLTYPE_QUAD | CTLFLAG_IMMEDIATE),
            PtrLane::Immediate
        );
        // Non-immediate scalars and all strings/structs live outside.
        assert_eq!(getptr_lane(CTLTYPE_INT), PtrLane::External);
        assert_eq!(getptr_lane(CTLTYPE_STRING), PtrLane::External);
        // Immediate strings are impossible; unknown types have no lane.
        assert_eq!(
            getptr_lane(CTLTYPE_STRING | CTLFLAG_IMMEDIATE),
            PtrLane::Absent
        );
        assert_eq!(getptr_lane(CTLTYPE_NODE), PtrLane::Absent);
        // Nibble 0 names no type (all six types are 1..6).
        assert_eq!(getptr_lane(CTLFLAG_READWRITE), PtrLane::Absent);
    }

    #[test]
    fn test_read_len() {
        // Strings measure live + terminator; others report width (:1140-1143).
        assert_eq!(read_len(CTLTYPE_STRING, 64, 5), Ok(6));
        assert_eq!(read_len(CTLTYPE_STRING, 64, 0), Ok(1));
        assert_eq!(read_len(CTLTYPE_INT, 4, 0), Ok(4));
        assert_eq!(read_len(CTLTYPE_STRUCT, 128, 0), Ok(128));
    }

    #[test]
    fn test_write_size_ok() {
        // Exact for scalars/structs, capped for strings (:1193-1208).
        assert!(write_size_ok(CTLTYPE_INT, 4, 4));
        assert!(!write_size_ok(CTLTYPE_INT, 3, 4));
        assert!(write_size_ok(CTLTYPE_STRING, 64, 64));
        assert!(write_size_ok(CTLTYPE_STRING, 5, 64));
        assert!(!write_size_ok(CTLTYPE_STRING, 65, 64));
        assert!(!write_size_ok(CTLTYPE_NODE, 0, 0));
        assert!(!write_size_ok(99, 4, 4));
    }

    #[test]
    fn test_stage_for() {
        // Scratch fits newlen + 1; bigger needs privilege (:1220-1229).
        assert_eq!(stage_for(0, false, 4096), Ok(Stage::Scratch));
        assert_eq!(stage_for(4095, false, 4096), Ok(Stage::Scratch));
        assert_eq!(stage_for(4096, false, 4096), Err(EPERM));
        assert_eq!(stage_for(4096, true, 4096), Ok(Stage::Heap));
        assert_eq!(stage_for(1 << 40, true, 4096), Ok(Stage::Heap));
    }

    #[test]
    fn test_finalize_string() {
        // Full without terminator refuses; the rest terminates (:1274-1288).
        assert_eq!(finalize_string(64, 64, false), Err(EINVAL));
        assert_eq!(finalize_string(64, 64, true), Ok(()));
        assert_eq!(finalize_string(5, 64, false), Ok(()));
        assert_eq!(finalize_string(0, 64, false), Ok(()));
    }

    #[test]
    fn test_sanitize_bool_and_verify() {
        // ABI wall: bool must be one byte (tree.c:1163 array trick).
        assert_eq!(core::mem::size_of::<bool>(), 1);
        // Nonzero is true without forming the value (:1265).
        assert!(sanitize_bool(1));
        assert!(sanitize_bool(2));
        assert!(!sanitize_bool(0));
        // Absent passes, false refuses (:1247-1248).
        assert_eq!(apply_verify(None), Ok(()));
        assert_eq!(apply_verify(Some(true)), Ok(()));
        assert_eq!(apply_verify(Some(false)), Err(EINVAL));
    }

    #[test]
    fn test_readwrite_combine() {
        // Read-then-write, report old length (:1315-1324).
        assert_eq!(readwrite_combine(64, OK), 64);
        assert_eq!(readwrite_combine(-22, OK), -22);
        assert_eq!(readwrite_combine(64, EPERM), EPERM as i64);
        assert_eq!(readwrite_combine(-22, EPERM), -22);
    }
}
