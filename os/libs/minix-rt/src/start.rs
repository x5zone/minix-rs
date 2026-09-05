//! Program entry: from the first instruction to the `main` function call.
//!
//! When the kernel jumps to a freshly loaded program, execution does not
//! start inside `main`. It starts at a tiny machine-code stub (C: `__start`
//! in `minix3/lib/csu/arch/x86_64/crt0.S`, 7 instructions) which forwards
//! three values to a C function named `___start`
//! (`minix3/lib/csu/common/crt0-common.c:144-192`):
//!
//! 1. The cleanup callback supplied by the shared program loader (always
//!    absent for statically linked programs).
//! 2. The loader object descriptor (likewise absent when statically linked).
//! 3. A pointer to the process string descriptor (always present; the
//!    function stops immediately when it is null).
//!
//! `___start` then publishes the environment pointer, derives the short
//! program name, runs the C library initialization, runs the ordered lists of
//! startup functions, registers the shutdown functions, and finally calls
//! `main` and passes its return value to `exit`.
//!
//! This module captures that sequence as an explicit, testable state machine
//! instead of a literal translation of the C control flow. The machine-code
//! stub itself stays in assembly (it must align the stack before any Rust
//! code may run); everything from the descriptor check onwards is ordinary
//! Rust that unit tests can drive directly.
//!
//! # Relation to Redox and to general operating system practice
//!
//! Redox ships the same idea in its `linker` crate: parse the startup
//! information the kernel left behind, run the startup function lists, call
//! `main`, pass the result to `exit`. The Minix variant differs in one
//! important way: Linux and Redox place the argument count, the argument
//! pointers, and the environment pointers directly on the initial stack,
//! while Minix passes a single pointer to a descriptor structure plus two
//! loader values in registers. This module therefore takes the descriptor as
//! its input rather than re-parsing a raw stack image.

use minix_types::Errno;

/// Sentinel environment pointer value used to detect a user override.
///
/// C: `char **environ = (char **) 0x53535353;`
/// (`minix3/minix/lib/libc/sys/environ.c:16`). The startup routine treats this
/// exact value as "nobody supplied an environment yet". The comment in the C
/// file explains why the value repeats one byte four times: only the lowest
/// two bytes are compared, because the pointer width and byte order differ
/// across machines, so every byte carries the same pattern to make the test
/// work everywhere.
pub const UNINITIALIZED_ENVIRON_SENTINEL: u64 = 0x5353_5353;

/// Short program name used when the argument list is empty.
///
/// C: `static char empty_string[] = "";` with
/// `__progname` initially pointing at it
/// (`minix3/lib/csu/common/crt0-common.c:80-81`). A program started with no
/// arguments has no name to display, so diagnostics fall back to the empty
/// string rather than to a null pointer.
pub const EMPTY_PROGRAM_NAME: &str = "";

/// Validated view of the process string descriptor passed to the entry point.
///
/// C: the `ps_strings` parameter of `___start`
/// (`minix3/lib/csu/common/crt0-common.c:144-148`) combined with
/// `struct ps_strings` (`minix3/sys/sys/exec.h:111-116`).
/// The C code stops the whole process when the pointer is null
/// (`_FATAL("ps_strings missing\n")`); this type makes that outcome explicit
/// by refusing to construct from a null address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntryDescriptor {
    /// Address of the descriptor structure (never zero).
    pub descriptor_address: u64,
    /// Number of argument strings.
    pub argument_count: usize,
    /// Address of the first argument pointer.
    pub argument_list: u64,
    /// Number of environment strings.
    pub environment_count: usize,
    /// Address of the first environment pointer.
    pub environment_list: u64,
}

/// Why the entry point refused to start the program.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryError {
    /// The descriptor pointer was null; there is no argument or environment
    /// information to work with.
    MissingDescriptor,
    /// A count field was negative in the raw C view and could not be
    /// represented.
    InvalidCount,
}

impl EntryError {
    /// Maps the failure to the closest Minix3 error number.
    ///
    /// A missing startup descriptor means the executable image or the
    /// loading step is malformed; the closest executable-format error is
    /// `ENOEXEC`. A negative count means the caller passed a malformed
    /// argument vector; that is `EINVAL`. Both values come from
    /// `minix3/sys/sys/errno.h`, so no new error code is invented.
    pub const fn to_errno(self) -> Errno {
        match self {
            EntryError::MissingDescriptor => {
                Errno::from_i32(minix_types::ENOEXEC)
            }
            EntryError::InvalidCount => Errno::from_i32(minix_types::EINVAL),
        }
    }
}

impl EntryDescriptor {
    /// Builds the descriptor, rejecting a null address or negative counts.
    pub fn new(
        descriptor_address: u64,
        argument_count: i32,
        argument_list: u64,
        environment_count: i32,
        environment_list: u64,
    ) -> Result<Self, EntryError> {
        if descriptor_address == 0 {
            return Err(EntryError::MissingDescriptor);
        }
        if argument_count < 0 || environment_count < 0 {
            return Err(EntryError::InvalidCount);
        }
        Ok(EntryDescriptor {
            descriptor_address,
            argument_count: argument_count as usize,
            argument_list,
            environment_count: environment_count as usize,
            environment_list,
        })
    }
}

/// Extracts the short program name from a full executable path.
///
/// C: the loop in `___start` (`minix3/lib/csu/common/crt0-common.c:156-165`)
/// that points `__progname` at the full first argument and then advances past
/// every `/` character, so `/usr/bin/hello` becomes `hello`. When the
/// argument list is empty (`argv[0]` is null), the C code keeps the empty
/// string.
///
/// The function takes the already-split first argument rather than scanning
/// raw memory, which keeps the slash search pure and unit testable. Byte-wise
/// scanning is used instead of character decoding so that non-Unicode path
/// bytes pass through unchanged, matching the C behavior of comparing raw
/// `char` values.
pub fn short_program_name(first_argument: Option<&[u8]>) -> &[u8] {
    match first_argument {
        None => EMPTY_PROGRAM_NAME.as_bytes(),
        Some(path) => {
            let mut start = 0;
            for (index, byte) in path.iter().enumerate() {
                if *byte == b'/' {
                    start = index + 1;
                }
            }
            &path[start..]
        }
    }
}

/// Reports whether the environment pointer still holds the sentinel.
///
/// C: the startup file `environ.c` pre-fills `environ` with
/// `0x53535353`, and the startup routine is expected to overwrite it with the
/// real environment list from the descriptor. A test or a diagnostic tool can
/// use this predicate to tell "still uninitialized" apart from "initialized
/// to an empty environment" (a genuine null pointer).
pub const fn environ_is_uninitialized(environment_address: u64) -> bool {
    // The C sentinel is a 32-bit pointer value; on a 64-bit address space it
    // occupies the low half while the high half is zero.
    environment_address == UNINITIALIZED_ENVIRON_SENTINEL
}

/// One ordered list of startup or shutdown functions.
///
/// C keeps four such lists as weak linker symbols so that statically linked
/// programs without any registered function still link successfully:
/// `preinit_array`, `init_array`, and `fini_array`
/// (`minix3/lib/csu/common/crt0-common.c:106-117`), plus the legacy
/// `.ctors` list for toolchains without array support
/// (`minix3/lib/csu/common/crtbegin.c:92-96`).
/// Each list is simply a sequence of function pointers executed in order;
/// the only differences are when each list runs and whether it runs in
/// forward or reverse order.
///
/// This type erases that history into one uniform runner: a slice of function
/// pointers plus the traversal direction. Tests supply plain Rust functions;
/// the real binary will supply linker-provided symbol ranges through the same
/// interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrayDirection {
    /// Execute from the first element to the last (startup lists).
    Forward,
    /// Execute from the last element to the first (legacy constructor list).
    Reverse,
}

/// Runs every function in the list in the requested direction.
///
/// Each function pointer is a plain address-free Rust function, so running
/// the list cannot fail and needs no error return. A null entry cannot occur:
/// the slice length already encodes how many entries exist, which removes the
/// C sentinel value `(fptr_t) -1` used in `crtbegin.c`.
pub fn run_function_array(functions: &[fn()], direction: ArrayDirection) {
    match direction {
        ArrayDirection::Forward => {
            for function in functions {
                function();
            }
        }
        ArrayDirection::Reverse => {
            for function in functions.iter().rev() {
                function();
            }
        }
    }
}

/// Guards one-time library initialization.
///
/// C calls `_libc_init()` explicitly from `___start`
/// (`minix3/lib/csu/common/crt0-common.c:177`) and the function defends
/// itself against a second call with a static flag
/// (`minix3/lib/libc/misc/initfini.c:87-90`: `if (libc_initialised) return;`).
/// The guard below is the same idea expressed as a reusable type: the first
/// call to [`RunOnce::run`] executes the callback and reports that it ran;
/// later calls skip the callback and report that initialization had already
/// happened.
///
/// The type is deliberately not a global: the caller owns the guard, which
/// keeps unit tests independent of each other. The single global guard used
/// by the real entry point lives in the runtime initialization module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RunOnce {
    completed: bool,
}

impl RunOnce {
    /// Creates an uncompleted guard.
    pub const fn new() -> Self {
        RunOnce { completed: false }
    }

    /// Runs `initialize` once; returns true when it actually ran.
    pub fn run(&mut self, initialize: impl FnOnce()) -> bool {
        if self.completed {
            return false;
        }
        initialize();
        self.completed = true;
        true
    }

    /// Reports whether initialization has already happened.
    pub const fn is_completed(self) -> bool {
        self.completed
    }
}

/// Ordered startup stages from descriptor check to the `main` call.
///
/// The order reproduces `___start`
/// (`minix3/lib/csu/common/crt0-common.c:150-191`):
/// descriptor check, environment publication, program name derivation,
/// dynamic loader check (skipped for static linking, see below), C library
/// initialization, pre-initialization list, shutdown registration, startup
/// list, then the `main` call whose return value becomes the exit status.
///
/// The dynamic loader branch (`rtld_DYNAMIC != NULL`, `crt0-common.c:167-175`)
/// intentionally has no stage here: statically linked Minix programs never
/// take it, and this runtime only supports static linking (architecture
/// decision A-1 in the stage plan). Keeping a dead stage would suggest a
/// capability the runtime does not have.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupStage {
    /// Verify the descriptor pointer is non-null.
    CheckDescriptor,
    /// Publish the environment pointer from the descriptor.
    PublishEnvironment,
    /// Derive the short program name from the first argument.
    DeriveProgramName,
    /// Run the one-time C library initialization.
    LibraryInit,
    /// Run the pre-initialization function list.
    RunPreInit,
    /// Register the shutdown function list for `exit`.
    RegisterShutdown,
    /// Run the startup function list.
    RunInit,
    /// Call `main` and forward its return value to `exit`.
    CallMain,
}

impl StartupStage {
    /// Returns the full startup order as a fixed array.
    pub const fn ordered() -> [StartupStage; 8] {
        [
            StartupStage::CheckDescriptor,
            StartupStage::PublishEnvironment,
            StartupStage::DeriveProgramName,
            StartupStage::LibraryInit,
            StartupStage::RunPreInit,
            StartupStage::RegisterShutdown,
            StartupStage::RunInit,
            StartupStage::CallMain,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn test_null_descriptor_is_rejected() {
        match EntryDescriptor::new(0, 1, 0x1000, 0, 0x2000) {
            Err(EntryError::MissingDescriptor) => {}
            other => panic!("expected MissingDescriptor, got {:?}", other),
        }
    }

    #[test]
    fn test_negative_counts_are_rejected() {
        assert_eq!(
            EntryDescriptor::new(0x3000, -1, 0x1000, 0, 0x2000),
            Err(EntryError::InvalidCount)
        );
        assert_eq!(
            EntryDescriptor::new(0x3000, 0, 0x1000, -1, 0x2000),
            Err(EntryError::InvalidCount)
        );
    }

    #[test]
    fn test_valid_descriptor_keeps_all_fields() {
        let descriptor = EntryDescriptor::new(0x3000, 2, 0x1000, 1, 0x2000).unwrap();
        assert_eq!(descriptor.argument_count, 2);
        assert_eq!(descriptor.environment_count, 1);
        assert_eq!(descriptor.argument_list, 0x1000);
        assert_eq!(descriptor.environment_list, 0x2000);
    }

    #[test]
    fn test_short_name_strips_directory_prefix() {
        assert_eq!(short_program_name(Some(b"/usr/bin/hello")), b"hello");
        assert_eq!(short_program_name(Some(b"hello")), b"hello");
        assert_eq!(short_program_name(Some(b"/")), b"");
        assert_eq!(short_program_name(None), b"");
    }

    #[test]
    fn test_short_name_keeps_trailing_bytes_after_last_slash() {
        // Non-Unicode bytes pass through untouched, like the C char loop.
        assert_eq!(short_program_name(Some(b"/bin/\xFF\xFE")), b"\xFF\xFE");
    }

    #[test]
    fn test_environ_sentinel_detection() {
        assert!(environ_is_uninitialized(UNINITIALIZED_ENVIRON_SENTINEL));
        assert!(!environ_is_uninitialized(0));
        assert!(!environ_is_uninitialized(0x2000));
    }

    static CALL_ORDER: AtomicUsize = AtomicUsize::new(0);
    static CALL_LOG: [AtomicUsize; 3] = [
        AtomicUsize::new(99),
        AtomicUsize::new(99),
        AtomicUsize::new(99),
    ];

    fn record_first() {
        CALL_LOG[CALL_ORDER.fetch_add(1, Ordering::SeqCst)].store(0, Ordering::SeqCst);
    }
    fn record_second() {
        CALL_LOG[CALL_ORDER.fetch_add(1, Ordering::SeqCst)].store(1, Ordering::SeqCst);
    }
    fn record_third() {
        CALL_LOG[CALL_ORDER.fetch_add(1, Ordering::SeqCst)].store(2, Ordering::SeqCst);
    }

    fn reset_call_log() {
        CALL_ORDER.store(0, Ordering::SeqCst);
        for slot in &CALL_LOG {
            slot.store(99, Ordering::SeqCst);
        }
    }

    #[test]
    fn test_forward_array_runs_in_order() {
        reset_call_log();
        run_function_array(
            &[record_first, record_second, record_third],
            ArrayDirection::Forward,
        );
        assert_eq!(CALL_LOG[0].load(Ordering::SeqCst), 0);
        assert_eq!(CALL_LOG[1].load(Ordering::SeqCst), 1);
        assert_eq!(CALL_LOG[2].load(Ordering::SeqCst), 2);
    }

    #[test]
    fn test_reverse_array_runs_backwards() {
        reset_call_log();
        run_function_array(
            &[record_first, record_second, record_third],
            ArrayDirection::Reverse,
        );
        assert_eq!(CALL_LOG[0].load(Ordering::SeqCst), 2);
        assert_eq!(CALL_LOG[1].load(Ordering::SeqCst), 1);
        assert_eq!(CALL_LOG[2].load(Ordering::SeqCst), 0);
    }

    #[test]
    fn test_run_once_executes_exactly_once() {
        let mut guard = RunOnce::new();
        let mut calls = 0;
        assert!(guard.run(|| calls += 1));
        assert!(!guard.run(|| calls += 1));
        assert_eq!(calls, 1);
        assert!(guard.is_completed());
    }

    #[test]
    fn test_startup_order_contains_eight_stages_ending_with_main() {
        let stages = StartupStage::ordered();
        assert_eq!(stages.len(), 8);
        assert_eq!(stages[0], StartupStage::CheckDescriptor);
        assert_eq!(stages[7], StartupStage::CallMain);
    }

    #[test]
    fn test_entry_errors_map_to_documented_errnos() {
        assert_eq!(
            EntryError::MissingDescriptor.to_errno(),
            Errno::from_i32(minix_types::ENOEXEC)
        );
        assert_eq!(
            EntryError::InvalidCount.to_errno(),
            Errno::from_i32(minix_types::EINVAL)
        );
    }
}
