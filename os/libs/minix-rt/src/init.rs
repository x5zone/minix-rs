//! Runtime initialization: publishing kernel information and communication vectors.
//!
//! The C runtime performs this work in a constructor function named
//! `__minix_init` (`minix3/minix/lib/libc/sys/init.c:20-32`) that runs before
//! the program entry point finishes its startup sequence. The constructor
//! does three things in order:
//!
//! 1. Asks the kernel communication layer for the kernel information page
//!    pointer (`ipc_minix_kerninfo(&_minix_kerninfo)`).
//! 2. Checks that the returned page carries the expected magic number
//!    (`KERNINFO_MAGIC`). A mismatch clears the global pointer back to null.
//! 3. When the page advertises communication vectors
//!    (`MINIX_KIF_IPCVECS` set and the pointer non-null), copies the whole
//!    vector table into the global `_minix_ipcvecs` so that every later
//!    `ipc_send` / `ipc_sendrec` call goes through the installed functions.
//!
//! This module expresses the same three steps without hidden globals and
//! without a linker constructor. The kernel query travels through an explicit
//! [`KerninfoSource`] trait, the validation reuses the handoff module, and
//! the installed state is an explicit [`RuntimeState`] value that tests can
//! inspect. A tiny global holder with an initialization guard is provided for
//! the real binary; unit tests use plain local values instead.
//!
//! # Why not a literal translation
//!
//! The C version relies on three mechanisms that do not fit Rust practice:
//! a linker constructor attribute (`__attribute__((__constructor__))`), a
//! mutable global pointer written from two places, and weak-symbol default
//! function tables. Redox solves the same problem differently: its runtime
//! initialization is an explicit function called from the entry point, with
//! the communication handles passed as arguments rather than pulled from
//! globals. This module follows the Redox shape (explicit initialization with
//! visible inputs and outputs) while preserving the exact Minix validation
//! order and magic number semantics.

use crate::handoff::{ValidatedKernInfo, KIF_IPC_VECTORS};
use minix_types::Errno;

/// Address of the communication vector table installed at startup.
///
/// The C table `struct minix_ipcvecs` (`minix3/minix/include/minix/ipc.h:2786`)
/// holds seven function pointers: send, receive, send-and-receive, nonblocking
/// send, notify, kernel call, and asynchronous send. Copying the C layout
/// pointer-for-pointer would reproduce C calling conventions in Rust. This
/// module instead records which table is active: the default direct-trap
/// table published by the kernel, or a test double supplied by unit tests.
/// Real message sending is the job of the communication mechanism module; here
/// only the selection ("which table did initialization install") is modeled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpcTableSelection {
    /// No table is active yet (the state before initialization runs, or an
    /// explicit caller choice in tests).
    None,
    /// The kernel-published table is active.
    KernelPublished,
    /// A test double is active (unit tests only).
    TestDouble,
}

/// How the kernel information page ended up in the runtime state.
///
/// This is the typed counterpart of C's `_minix_kerninfo` global
/// (`init.c:6`): C collapses "the kernel refused the query" and "the magic
/// is wrong" into one NULL pointer, and every consumer re-checks the
/// pointer. The situations stay distinguishable here because the
/// distinction costs one enum variant and buys a diagnosable state; none of
/// them is fatal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KerninfoAvailability {
    /// The kernel query itself reported failure (nonzero status from the
    /// `ipc_minix_kerninfo` equivalent). C: `init.c:22-26` clears the
    /// global and keeps running.
    QueryFailed(i32),
    /// The returned page carried the wrong magic number, so it was
    /// discarded. C: same branch, same continue.
    BadMagic {
        /// Value actually present in the magic field.
        found: u32,
    },
    /// The page is present and passed validation.
    Available(ValidatedKernInfo),
}

/// Explicit runtime state produced by initialization.
///
/// The C version keeps two globals: `_minix_kerninfo` (the page pointer,
/// `init.c:6`) and `_minix_ipcvecs` (the active vector table, defaulting to
/// the direct-trap functions, `init.c:10-18`). This structure is the same
/// information with ownership made visible: how the page ended up (present,
/// refused by the kernel, or rejected by validation), and which table is
/// active.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeState {
    /// How the kernel information page ended up.
    pub kerninfo: KerninfoAvailability,
    /// Which communication table is active.
    pub ipc_table: IpcTableSelection,
}

impl RuntimeState {
    /// Reports whether a genuine kernel information page is available.
    ///
    /// This mirrors the C consumer-side check `_minix_kerninfo != NULL`
    /// that guards every kerninfo-dependent service.
    pub const fn is_ready(&self) -> bool {
        matches!(self.kerninfo, KerninfoAvailability::Available(_))
    }
}

/// Source of the kernel information page pointer.
///
/// C calls `ipc_minix_kerninfo(&_minix_kerninfo)` directly, which traps into
/// the kernel. That trap cannot run in a unit test, so this trait abstracts
/// the query: the real implementation performs the trap, while tests return
/// canned headers. The trait has two behaviors across implementations (trap
/// vs. canned data), which keeps the abstraction justified under the project
/// rule that a trait needs at least two behaviorally different
/// implementations.
pub trait KerninfoSource {
    /// Queries the kernel information page.
    ///
    /// The return value mirrors the C convention: `Ok(header)` carries the
    /// leading page fields on success, while `Err(code)` carries the nonzero
    /// status the C function would return. The magic number inside `header`
    /// is still unchecked at this point; validation is a separate step.
    fn query_kerninfo(&self) -> Result<crate::handoff::KernInfoHeader, i32>;
}

/// Direct-trap source used by the real binary.
///
/// This type performs no work itself; it marks the intent that the query goes
/// through the real kernel trap. The actual trap instruction sequence lives in
/// the communication mechanism module (it owns the hardware boundary), and
/// this marker lets the initialization logic name that choice in tests and in
/// documentation without duplicating the trap code here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DirectTrapSource;

impl KerninfoSource for DirectTrapSource {
    fn query_kerninfo(&self) -> Result<crate::handoff::KernInfoHeader, i32> {
        // E1 slice 4: issue the real MINIX_KERNINFO query through the
        // direct transport's vector-33 trap. The kernel hands back the
        // user-mapped info page address through the secondary return
        // register; the header fields are read from that page. Hosted
        // test builds keep the explicit EIO — there is no kernel behind
        // the trap there (the CannedSource is the test seam).
        #[cfg(all(target_arch = "x86_64", feature = "real-trap"))]
        {
            use minix_sys::ipc::{DirectTrapTransport, IpcTransport as _};
            let page = DirectTrapTransport
                .query_kerninfo_page()
                .map_err(|status| status.0)?;
            if page == 0 {
                return Err(minix_types::EIO);
            }
            // SAFETY: the kernel published and user-mapped the page before
            // returning its address (a mapping the boot handoff owns);
            // the page is read-only from user mode.
            let info = unsafe { &*(page as *const minix_types::MinixKerninfo) };
            return Ok(crate::handoff::KernInfoHeader {
                magic: info.kerninfo_magic,
                flags: info.ki_flags,
                ipc_vectors_address: info.minix_ipcvecs,
                user_info_address: info.kuserinfo,
            });
        }
        #[cfg(not(all(target_arch = "x86_64", feature = "real-trap")))]
        {
            Err(minix_types::EIO)
        }
    }
}

/// Canned source used by unit tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CannedSource {
    /// Result the query should return.
    pub result: Result<crate::handoff::KernInfoHeader, i32>,
}

impl KerninfoSource for CannedSource {
    fn query_kerninfo(&self) -> Result<crate::handoff::KernInfoHeader, i32> {
        self.result
    }
}

/// Runs the three initialization steps in the C order and yields the state.
///
/// Initialization never fails fatally — that is the C contract. The
/// constructor `__minix_init` (`init.c:20-32`) answers a failed query or a
/// wrong magic number by clearing `_minix_kerninfo` to NULL and letting the
/// program continue with the default vector table it preinstalled
/// (`init.c:10-18`). The function below carries the same information
/// without hidden globals:
///
/// 1. Query the source for the page header. A query failure records
///    [`KerninfoAvailability::QueryFailed`] and keeps `fallback_table`
///    active — the caller's standing table plays the role of C's
///    preinstalled default.
/// 2. Validate the magic number (once, through
///    [`ValidatedKernInfo::new`]). A mismatch records
///    [`KerninfoAvailability::BadMagic`] and likewise keeps the fallback.
/// 3. When the validated page advertises communication vectors (flag bit
///    set and pointer non-null, `init.c:27-31`), select the
///    kernel-published table; otherwise keep the fallback. The C code
///    copies the whole table struct (`_minix_ipcvecs =
///    *info->minix_ipcvecs`); recording the selection is the equivalent
///    decision without copying raw function pointers.
///
/// The `fallback_table` parameter carries the table that was active before
/// this run (normally the default direct-trap table). Passing it explicitly
/// keeps the function pure: it never reads a hidden global.
pub fn initialize_runtime(
    source: &impl KerninfoSource,
    fallback_table: IpcTableSelection,
) -> RuntimeState {
    let validated = match source.query_kerninfo() {
        Ok(header) => match ValidatedKernInfo::new(header) {
            Ok(valid) => valid,
            Err(mismatch) => {
                return RuntimeState {
                    kerninfo: KerninfoAvailability::BadMagic {
                        found: mismatch.found,
                    },
                    ipc_table: fallback_table,
                };
            }
        },
        Err(code) => {
            return RuntimeState {
                kerninfo: KerninfoAvailability::QueryFailed(code),
                ipc_table: fallback_table,
            };
        }
    };
    let table = if validated.header().flags & KIF_IPC_VECTORS != 0
        && validated.header().ipc_vectors_address != 0
    {
        IpcTableSelection::KernelPublished
    } else {
        fallback_table
    };
    RuntimeState {
        kerninfo: KerninfoAvailability::Available(validated),
        ipc_table: table,
    }
}

/// Thread-local storage model for user-space programs.
///
/// C on 32-bit Intel Minix has no user-space thread-local storage: the NetBSD
/// base it was ported from does not use processor segment registers for
/// threads on that architecture, so every thread shares one global `errno`
/// cell (see `minix3/lib/libc/gen/_errno.c:44-55`, which returns `&errno`
/// directly in the non-threaded build). On 64-bit Intel hardware the
/// processor provides a dedicated segment register for thread-local data, so
/// a Rust runtime can offer real per-thread storage. This enumeration names
/// the choice explicitly so documentation and code cannot drift apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadLocalModel {
    /// One shared cell for the whole process (matches the 32-bit C behavior).
    SharedCell,
    /// Real per-thread storage through the processor thread pointer.
    ThreadPointer,
}

/// Error number slot policy.
///
/// C exposes a single global integer named `errno` plus an accessor function
/// `__errno()` that returns its address (`minix3/lib/libc/gen/_errno.c:44-55`).
/// Every library function reports failure by returning -1 and writing the
/// reason into that global. Rust reports failure with `Result<T, Errno>`
/// instead, so no global cell is needed on the happy path. This enumeration
/// records the policy: new code returns `Result`, while the one compatibility
/// helper ([`errno_to_negative`]) translates a typed error back into the C
/// negative-number convention at the narrow boundary where Rust calls into C
/// or answers a C caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrnoPolicy {
    /// New code: return `Result<T, Errno>`.
    TypedResult,
    /// Boundary helper: convert `Errno` to the C negative convention.
    NegativeBoundary,
}

/// Converts a typed error into the C negative-number convention.
///
/// C library functions in `minix3/minix/lib/libc/sys/` return -1 on failure
/// and store the positive error number in `errno`; the lower system call
/// layer returns the negated error directly. This helper performs exactly
/// that negation (`-(error value)`) for the boundary in either direction, so
/// the conversion lives in one tested place instead of being re-typed with a
/// bare minus sign at every call site.
pub const fn errno_to_negative(error: Errno) -> i32 {
    -error.to_i32()
}

/// Converts a C negative return back into a typed error.
///
/// When the raw return value is zero or positive it is a success value, not
/// an error; when negative, its negation is the Minix3 error number. Returns
/// `None` for success so callers handle the two cases with a single match.
pub const fn negative_to_errno(raw_return: i32) -> Option<Errno> {
    if raw_return >= 0 {
        None
    } else {
        Some(Errno::from_i32(-raw_return))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handoff::{KernInfoHeader, KIF_USER_INFO, KERNINFO_MAGIC};

    fn header_with(flags: u32, ipc_address: u64) -> KernInfoHeader {
        KernInfoHeader {
            magic: KERNINFO_MAGIC,
            flags,
            ipc_vectors_address: ipc_address,
            user_info_address: 0x2000,
        }
    }

    #[test]
    fn test_successful_init_installs_kernel_table() {
        let source = CannedSource {
            result: Ok(header_with(KIF_IPC_VECTORS, 0x1000)),
        };
        let state = initialize_runtime(&source, IpcTableSelection::TestDouble);
        assert!(state.is_ready());
        assert!(matches!(
            state.kerninfo,
            KerninfoAvailability::Available(_)
        ));
        assert_eq!(state.ipc_table, IpcTableSelection::KernelPublished);
    }

    #[test]
    fn test_missing_vector_advertisement_keeps_fallback_table() {
        let source = CannedSource {
            result: Ok(header_with(0, 0)),
        };
        let state = initialize_runtime(&source, IpcTableSelection::TestDouble);
        assert!(state.is_ready());
        assert_eq!(state.ipc_table, IpcTableSelection::TestDouble);
    }

    #[test]
    fn test_flag_without_pointer_keeps_fallback_table() {
        // Matches the C double condition: flag alone is not sufficient.
        let source = CannedSource {
            result: Ok(header_with(KIF_IPC_VECTORS, 0)),
        };
        let state = initialize_runtime(&source, IpcTableSelection::TestDouble);
        assert_eq!(state.ipc_table, IpcTableSelection::TestDouble);
    }

    #[test]
    fn test_query_failure_degrades_and_keeps_fallback_table() {
        // C: init.c:22-26 — a failed query clears the page pointer and the
        // program continues on the preinstalled default table. The fallback
        // the caller passed in plays that role, so it survives the failure
        // instead of collapsing to "no table".
        let source = CannedSource { result: Err(-5) };
        let state = initialize_runtime(&source, IpcTableSelection::KernelPublished);
        assert!(!state.is_ready());
        assert_eq!(
            state.kerninfo,
            KerninfoAvailability::QueryFailed(-5)
        );
        assert_eq!(state.ipc_table, IpcTableSelection::KernelPublished);
    }

    #[test]
    fn test_wrong_magic_degrades_and_keeps_fallback_table() {
        let source = CannedSource {
            result: Ok(KernInfoHeader {
                magic: 0x0,
                flags: KIF_IPC_VECTORS | KIF_USER_INFO,
                ipc_vectors_address: 0x1000,
                user_info_address: 0x2000,
            }),
        };
        let state = initialize_runtime(&source, IpcTableSelection::KernelPublished);
        assert!(!state.is_ready());
        assert_eq!(
            state.kerninfo,
            KerninfoAvailability::BadMagic { found: 0x0 }
        );
        assert_eq!(state.ipc_table, IpcTableSelection::KernelPublished);
    }

    #[test]
    fn test_direct_trap_source_reports_unreachable_in_hosted_tests() {
        let source = DirectTrapSource;
        let state = initialize_runtime(&source, IpcTableSelection::None);
        assert!(matches!(
            state.kerninfo,
            KerninfoAvailability::QueryFailed(_)
        ));
        assert!(!state.is_ready());
    }

    #[test]
    fn test_errno_negative_round_trip() {
        let error = Errno::from_i32(minix_types::EINVAL);
        let raw = errno_to_negative(error);
        assert_eq!(raw, -22);
        assert_eq!(negative_to_errno(raw), Some(error));
        assert_eq!(negative_to_errno(0), None);
        assert_eq!(negative_to_errno(5), None);
    }
}
