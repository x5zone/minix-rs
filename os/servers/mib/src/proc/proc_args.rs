//! KERN_PROC_ARGS: process arguments and environment.
//!
//! Mirrors the pure halves of `mib_kern_proc_args` (proc.c:918-1176).
//! The slot lookup comes from 16 (`get_mslot`), the zombie rule matches
//! 18, and the copy-out loop stays with the transport layer. What is
//! decided here: which of the four requests it is, the argument checks,
//! the size estimate, the page-copy budget, and the page-walk math.
//! Reading the target process memory (`sys_datacopy` per page) and the
//! output batching loop are transport effects (A-12).
//!
//! 19-mib-proc-args.md.

use minix_types::{
    EINVAL, EOPNOTSUPP, ESRCH, KERN_PROC_ARGV, KERN_PROC_ENV, KERN_PROC_NARGV, KERN_PROC_NENV,
};

/// Which of the four argument requests this is.
///
/// C: the `switch (req)` in `mib_kern_proc_args` — proc.c:939-947.
/// Anything else is "not supported" (not "bad argument": the shape is
/// fine, the operation is not).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArgsReq {
    /// Argument vector strings. C: `KERN_PROC_ARGV`.
    Argv,
    /// Environment strings. C: `KERN_PROC_ENV`.
    Env,
    /// Number of argument strings. C: `KERN_PROC_NARGV`.
    Nargv,
    /// Number of environment strings. C: `KERN_PROC_NENV`.
    Nenv,
}

/// Decode the request; unknown numbers are not supported.
///
/// C: `switch (req)` with `default: return EOPNOTSUPP` — proc.c:939-947.
pub const fn decode_req(req: i32) -> Result<ArgsReq, i32> {
    match req {
        KERN_PROC_ARGV => Ok(ArgsReq::Argv),
        KERN_PROC_ENV => Ok(ArgsReq::Env),
        KERN_PROC_NARGV => Ok(ArgsReq::Nargv),
        KERN_PROC_NENV => Ok(ArgsReq::Nenv),
        _ => Err(EOPNOTSUPP),
    }
}

/// Check the name: exactly two components (PID, request).
///
/// C: `call_namelen != 2 → EINVAL` — proc.c:933-934.
pub const fn check_args_namelen(namelen: u32) -> Result<(), i32> {
    if namelen != 2 {
        return Err(EINVAL);
    }
    Ok(())
}

/// Check the looked-up slot: a miss or a zombie is "no such process".
///
/// C: `get_mslot` miss or zombie → `ESRCH` — proc.c:952-956. Same rule
/// as 18's single-PID check, reused directly (see
/// [`super::proc2::check_pid_slot`]).
pub use super::proc2::check_pid_slot as check_args_slot;

/// Whether this is a count-only estimate call.
///
/// C: `oldp == NULL && (req == NARGV || req == NENV)` returns
/// `sizeof(count)` without touching the target — proc.c:958-960.
pub const fn is_count_estimate(oldp_null: bool, req: ArgsReq) -> bool {
    oldp_null
        && match req {
            ArgsReq::Nargv | ArgsReq::Nenv => true,
            ArgsReq::Argv | ArgsReq::Env => false,
        }
}

/// Upper size estimate for the answer.
///
/// C: `max = roundup(MIN(frame_len, ARG_MAX), PAGE_SIZE)` —
/// proc.c:981. The frame length comes from the original exec call; the
/// cap comes from the exec size limit. `setproctitle(3)` may point
/// outside the frame, but that data stays under two pages, which the
/// round-up absorbs (:967-980). Both limits travel as parameters: they
/// belong to the exec headers, not to this module.
pub const fn max_estimate(frame_len: u64, arg_max: u64, page_size: u64) -> u64 {
    let capped = if frame_len < arg_max {
        frame_len
    } else {
        arg_max
    };
    ((capped + page_size - 1) / page_size) * page_size
}

/// Page-copy budget for the string walk.
///
/// C: `copybudget = (ARG_MAX / PAGE_SIZE) * 2` — proc.c:1051. A rogue
/// process could lay out two-byte strings each spanning two pages and
/// force up to a gigabyte of copying at 256K `ARG_MAX` (:1023-1029);
/// the budget stops the walk. Vector-page copies do not count: they
/// are linear anyway (:1027-1029).
pub const fn copy_budget(arg_max: u64, page_size: u64) -> u64 {
    (arg_max / page_size) * 2
}

/// Whether the vector walk can start.
///
/// C: `trunc_page(vaddr) == 0 || vaddr % sizeof(char *) != 0` returns 0
/// — proc.c:1040-1041. A null-page vector or a misaligned vector means
/// "no strings", reported as an empty answer, not an error.
pub const fn walk_can_start(vaddr: u64, page_size: u64, ptr_size: u64) -> bool {
    vaddr / page_size != 0 && vaddr % ptr_size == 0
}

/// Cap the caller's length to the estimate.
///
/// C: `if (oldlen > max) oldlen = max` — proc.c:1048-1049.
pub const fn cap_oldlen(oldlen: u64, max: u64) -> u64 {
    if oldlen > max {
        max
    } else {
        oldlen
    }
}

/// Which local buffer already holds a string page.
///
/// C: the `ppage == vpage / ppage == spage / fetch` chain —
/// proc.c:1086-1102. The walk keeps two fetched pages: the vector page
/// and at most one string page. A string pointer landing on either
/// reuses it; anything else spends one unit of copy budget on a fresh
/// fetch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageHome {
    /// The vector page already holds it. C: `buf = vbuf`.
    Vector,
    /// The kept string page already holds it. C: `buf = sbuf`.
    KeptString,
    /// Fetch a new string page (spends budget). C: the `else` arm.
    Fetch,
}

/// Locate the string page among the kept buffers.
///
/// C: proc.c:1086-1102. `vpage == 0`/`spage == 0` mean "nothing kept
/// yet" — page zero never holds strings (see [`walk_can_start`]).
pub const fn locate_page(ppage: u64, vpage: u64, spage: u64) -> PageHome {
    if ppage == vpage {
        return PageHome::Vector;
    }
    if spage != 0 && ppage == spage {
        return PageHome::KeptString;
    }
    PageHome::Fetch
}

/// Split one page fragment at the string end.
///
/// C: `memchr(p, '\0', pleft)` — proc.c:1114-1121. `nul_at` is the
/// offset of the terminator inside this fragment, if any. Returns the
/// bytes this fragment contributes and whether the string ends here.
pub const fn fragment_split(pleft: u64, nul_at: Option<u64>) -> (u64, bool) {
    match nul_at {
        Some(at) => (at + 1, true),
        None => (pleft, false),
    }
}

/// Cap one fragment to the remaining answer budget.
///
/// C: `if (off + olen + bytes > oldlen) bytes = oldlen - off - olen` —
/// proc.c:1124-1125.
pub const fn cap_fragment(off: u64, olen: u64, bytes: u64, oldlen: u64) -> u64 {
    if off + olen + bytes > oldlen {
        oldlen - off - olen
    } else {
        bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_req_decode() {
        // Request numbers (sysctl.h:677-680); unknown is unsupported.
        assert_eq!(decode_req(1), Ok(ArgsReq::Argv));
        assert_eq!(decode_req(2), Ok(ArgsReq::Nargv));
        assert_eq!(decode_req(3), Ok(ArgsReq::Env));
        assert_eq!(decode_req(4), Ok(ArgsReq::Nenv));
        assert_eq!(decode_req(0), Err(EOPNOTSUPP));
        assert_eq!(decode_req(5), Err(EOPNOTSUPP));
        // Two name components (proc.c:933-934).
        assert_eq!(check_args_namelen(2), Ok(()));
        assert_eq!(check_args_namelen(1), Err(EINVAL));
        assert_eq!(check_args_namelen(3), Err(EINVAL));
        // Miss or zombie is "no such process" (:952-956).
        assert_eq!(check_args_slot(false, false), Ok(()));
        assert_eq!(check_args_slot(true, false), Err(ESRCH));
        assert_eq!(check_args_slot(false, true), Err(ESRCH));
    }

    #[test]
    fn test_estimate_math() {
        // Count-only estimates skip the target (:958-960).
        assert!(is_count_estimate(true, ArgsReq::Nargv));
        assert!(is_count_estimate(true, ArgsReq::Nenv));
        assert!(!is_count_estimate(true, ArgsReq::Argv));
        assert!(!is_count_estimate(false, ArgsReq::Nargv));
        // Estimate: capped, then rounded up to whole pages (:981).
        assert_eq!(max_estimate(100, 262_144, 4096), 4096);
        assert_eq!(max_estimate(5000, 262_144, 4096), 8192);
        assert_eq!(max_estimate(300_000, 262_144, 4096), 262_144);
        assert_eq!(max_estimate(262_144, 262_144, 4096), 262_144);
        // Budget: twice the page count at the limit (:1051).
        assert_eq!(copy_budget(262_144, 4096), 128);
        assert_eq!(copy_budget(4096, 4096), 2);
        // Caller length capped to the estimate (:1048-1049).
        assert_eq!(cap_oldlen(99, 50), 50);
        assert_eq!(cap_oldlen(30, 50), 30);
    }

    #[test]
    fn test_walk_guards() {
        // Null page or misaligned vector: empty answer, not an error.
        assert!(!walk_can_start(0, 4096, 8));
        assert!(!walk_can_start(4096 + 3, 4096, 8));
        assert!(walk_can_start(4096, 4096, 8));
        // Zero kept page never matches (see walk_can_start).
        assert_eq!(locate_page(0, 8192, 0), PageHome::Fetch);
        assert_eq!(locate_page(8192, 8192, 0), PageHome::Vector);
        assert_eq!(locate_page(12288, 8192, 12288), PageHome::KeptString);
        assert_eq!(locate_page(16384, 8192, 12288), PageHome::Fetch);
        // Fragment split at the terminator, or whole fragment.
        assert_eq!(fragment_split(100, Some(9)), (10, true));
        assert_eq!(fragment_split(100, Some(0)), (1, true));
        assert_eq!(fragment_split(100, None), (100, false));
        // Fragment capped to the remaining budget.
        assert_eq!(cap_fragment(0, 0, 50, 100), 50);
        assert_eq!(cap_fragment(80, 0, 50, 100), 20);
    }
}
