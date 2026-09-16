//! Program birth chain — the crt0 counterpart (documents 02 + 03 wiring).
//!
//! C ground truth: `minix3/lib/csu/arch/x86_64/crt0.S` (stack alignment +
//! argument marshalling into `___start`) and
//! `minix3/lib/csu/common/crt0-common.c` `___start` (descriptor check →
//! publish → environ → progname → library init → main → exit).
//!
//! # Entry ABI at `_start` (x86-64)
//!
//! Established by the kernel's `build_cpu_context` (arch/src/x86_64/boot.rs):
//! `RSP` is the initial stack pointer, `RBX` carries the `struct ps_strings`
//! pointer — C's third entry value. `RDX`/`RCX` are C's cleanup-function and
//! dynamic-loader slots; both are always zero here (the kernel zeroes them,
//! and this rewrite is statically linked with no rtld), so the stub consumes
//! only `RBX` instead of marshalling all three registers into arguments the
//! way C does (`movq %rdx,%rdi; movq %rcx,%rsi; movq %rbx,%rdx`).
//!
//! # Birth sequence (each stage named — the order is the semantics)
//!
//! 1. **descriptor check**: a zero `ps_strings` pointer means there is no
//!    legal process image (C: `_FATAL("ps_strings missing")` — and the
//!    Minix build compiles the write away, leaving bare `_exit(1)`).
//! 2. **library init**: the allocator one-shot guard (C `_libc_init`'s
//!    single-run role, reduced to what this runtime has).
//! 3. **runtime init**: the kernel information page query through the real
//!    trap (document 03; C: the libc constructor's `ipc_minix_kerninfo`).
//!    Failure is *not* fatal: the state records what happened and `main`
//!    decides (C init.c:22-26 "not fatal" semantics — see V1-P1-4).
//! 4. **publish**: argv/envp descriptor and the program short name (byte
//!    scan for the last `/`, C crt0-common.c:158-167) become the statics
//!    the accessors below serve.
//! 5. **main**: no-argument Rust shape; the vectors live behind
//!    [`argv_bytes`]/[`progname`] (the std `main()` + `env::args()` split,
//!    not C's `main(argc, argv, envp)`).
//! 6. **exit**: the return value goes to `minix_sys::exit` (PM_EXIT via the
//!    direct trap; C `exit(main(...))`).

use minix_types::Errno;

use crate::handoff::ProcessStrings;
use crate::init::RuntimeState;

/// Raw `struct ps_strings` image on the initial stack.
///
/// C: `sys/sys/exec.h` — `ps_argvstr`@0, `ps_nargvstr`@8, `ps_envstr`@16,
/// `ps_nenvstr`@24 (LP64: two pointers, two i32, tail padding to 32).
/// The kernel's `load_vm_elf` writes exactly these four fields.
#[repr(C)]
struct PsStringsRaw {
    argv_str: u64,
    n_argv: i32,
    env_str: u64,
    n_env: i32,
}

/// Reads and validates the descriptor at `addr`.
///
/// Zero address maps to `ENOEXEC` (no descriptor, no legal process image —
/// document 02 §4); negative counts map to `EINVAL` through
/// `ProcessStrings::from_raw`. A wild non-null pointer faults — the kernel
/// turns that into SIGSEGV, the same verdict C would reach.
unsafe fn read_process_strings(addr: u64) -> Result<ProcessStrings, Errno> {
    if addr == 0 {
        return Err(Errno::from_i32(minix_types::ENOEXEC));
    }
    // SAFETY: birth runs once, before any other user code touched the
    // stack the kernel mapped; `addr` is the kernel-supplied descriptor
    // pointer. A garbage (non-null) value faults by design.
    let raw = unsafe { &*(addr as *const PsStringsRaw) };
    ProcessStrings::from_raw(raw.argv_str, raw.n_argv, raw.env_str, raw.n_env)
}

/// Program short name: everything after the last `/` in argv[0].
///
/// Byte scan, no character decoding (C crt0-common.c:158-167 walks bytes
/// and remembers the position after each slash; an empty argv[0] yields
/// the empty slice — C falls back to `empty_string`, same content).
fn progname_of(argv0: &[u8]) -> &[u8] {
    let mut start = 0;
    for (i, &byte) in argv0.iter().enumerate() {
        if byte == b'/' {
            start = i + 1;
        }
    }
    &argv0[start..]
}

/// Everything the birth chain publishes about this process.
struct Birth {
    runtime: Option<RuntimeState>,
    ps: Option<ProcessStrings>,
    /// Address of the short-name first byte and its length (points into
    /// the initial stack's argv[0] storage, which lives for the whole
    /// process — same lifetime reasoning as C's `__progname`).
    progname: (*const u8, usize),
}

impl Birth {
    const fn new() -> Self {
        Birth {
            runtime: None,
            ps: None,
            progname: (core::ptr::null(), 0),
        }
    }
}

/// Shared-cell wrapper: the birth chain writes once, accessors read.
/// Single-threaded process contract (same as the allocator pool in lib.rs).
struct BirthCell(core::cell::UnsafeCell<Birth>);
unsafe impl Sync for BirthCell {}
static BIRTH: BirthCell = BirthCell(core::cell::UnsafeCell::new(Birth::new()));

/// Runtime state recorded at birth: how the kernel information page query
/// ended and which communication table is active.
///
/// `None` before the birth chain stage 3 has run (or in `std` builds where
/// the caller initializes by hand). Copy-out on purpose: callers cannot
/// hold a borrow into the cell.
pub fn runtime_state() -> Option<RuntimeState> {
    // SAFETY: single-threaded process; the write happened in stage 4 of
    // the birth chain, every later call only reads.
    unsafe { &*BIRTH.0.get() }.runtime
}

/// Program short name (argv[0] after the last `/`), empty until birth
/// stage 4 published it. Empty slice — not None — for the no-argv case,
/// matching C's `__progname = empty_string` fallback.
pub fn progname() -> &'static [u8] {
    // SAFETY: single-threaded read of the once-written cell; the bytes
    // live on the initial stack for the whole process lifetime.
    let (ptr, len) = unsafe { &*BIRTH.0.get() }.progname;
    if ptr.is_null() {
        &[]
    } else {
        unsafe { core::slice::from_raw_parts(ptr, len) }
    }
}

/// Number of argument strings the kernel passed in the descriptor.
pub fn argv_count() -> usize {
    // SAFETY: single-threaded read of the once-written cell.
    unsafe { &*BIRTH.0.get() }
        .ps
        .as_ref()
        .map_or(0, |ps| ps.argument_count)
}

/// The i-th argument string's bytes (without the NUL terminator).
///
/// Reads through the argv array the kernel placed on the initial stack;
/// `None` past the end or on a NULL slot (the array is NUL-pointer
/// terminated like C's `argv`).
pub fn argv_bytes(index: usize) -> Option<&'static [u8]> {
    // SAFETY: single-threaded read of the once-written cell; the argv
    // array and its strings live on the initial stack for the whole
    // process lifetime (the kernel maps that stack read-only-ish and no
    // one rewrites it after birth).
    let ps = unsafe { &*BIRTH.0.get() }.ps.as_ref()?;
    if index >= ps.argument_count {
        return None;
    }
    let slot = unsafe { ((ps.argument_list + (index as u64) * 8) as *const u64).read() };
    if slot == 0 {
        return None;
    }
    let mut length = 0usize;
    // SAFETY: argv strings are NUL-terminated bytes on the initial stack
    // (loader contract, C sys/exec.h ps_strings consumers).
    let mut cursor = slot as *const u8;
    unsafe {
        while *cursor.add(length) != 0 {
            length += 1;
        }
    }
    Some(unsafe { core::slice::from_raw_parts(slot as *const u8, length) })
}

/// Birth failure: no descriptor, no legal image.
///
/// C `_FATAL` writes to stderr then `_exit(1)`, and the Minix build
/// compiles the write away — the honest reduction is a bare exit with the
/// visible code 1.
fn birth_fail() -> ! {
    minix_sys::exit(1)
}

/// The real birth function the entry stub calls with the `ps_strings`
/// pointer (see the module-level entry ABI). Runs the named stages and
/// never returns.
#[cfg(all(not(test), not(feature = "std"), target_arch = "x86_64"))]
unsafe extern "C" fn rt_birth(ps_strings: u64) -> ! {
    use crate::init::{initialize_runtime, DirectTrapSource, IpcTableSelection};

    // Stage 1 — descriptor check (C: ps_strings == NULL → _FATAL).
    let ps = match unsafe { read_process_strings(ps_strings) } {
        Ok(ps) => ps,
        Err(_) => birth_fail(),
    };

    // Stage 2 — library init (allocator one-shot; C _libc_init's guard).
    crate::init();

    // Stage 3 — runtime init: the kerninfo query goes through the real
    // int-33 trap (DirectTrapSource). Fallback table `None`: the kernel
    // published no IPC vector table (ki_flags has no IPCVECS in this
    // rewrite), so the linked-in minix-sys direct transports are the
    // senders — the C "in-libc fallback" shape.
    let state = initialize_runtime(&DirectTrapSource, IpcTableSelection::None);

    // Stage 4 — publish: short name from argv[0] (may be absent when the
    // kernel passed an empty vector, as boot-time images do).
    let (progname_ptr, progname_len) = match argv_storage_of(&ps) {
        Some(argv0) => {
            let name = progname_of(argv0);
            (name.as_ptr(), name.len())
        }
        None => (core::ptr::null(), 0),
    };
    // SAFETY: the single birth write; ordered before any accessor by the
    // program itself (nothing else runs before `main`).
    unsafe {
        *BIRTH.0.get() = Birth {
            runtime: Some(state),
            ps: Some(ps),
            progname: (progname_ptr, progname_len),
        };
    }

    // Stage 5 — main (no-argument Rust shape; vectors via accessors).
    unsafe extern "Rust" {
        fn main() -> i32;
    }
    let exit_code = unsafe { main() };

    // Stage 6 — exit (C: exit(main(...)) reduced to the PM_EXIT send).
    minix_sys::exit(exit_code)
}

/// The argv[0] string bytes behind the descriptor, if any.
fn argv_storage_of(ps: &ProcessStrings) -> Option<&'static [u8]> {
    if ps.argument_count == 0 {
        return None;
    }
    // SAFETY: the argv array lives on the initial stack (loader contract);
    // slot 0 exists because the count is positive.
    let first = unsafe { (ps.argument_list as *const u64).read() };
    if first == 0 {
        return None;
    }
    let mut length = 0usize;
    // SAFETY: argv strings are NUL-terminated on the initial stack.
    unsafe {
        while *(first as *const u8).add(length) != 0 {
            length += 1;
        }
    }
    Some(unsafe { core::slice::from_raw_parts(first as *const u8, length) })
}

// ── Entry stub (x86-64) ─────────────────────────────────────────────────
//
// C counterpart: `lib/csu/arch/x86_64/crt0.S` — `andq $~15,%rsp; subq $8,
// %rsp; movq %rdx,%rdi; movq %rcx,%rsi; movq %rbx,%rdx; jmp ___start`.
// Differences, each with a reason: only `RBX` is marshalled (the other two
// C slots cannot be nonzero — see the module docs); `call` instead of
// `jmp` (the birth function is an ordinary Rust `extern "C"` fn, so the
// stack must carry the return address, and the `and` alone provides the
// System V 16-byte pre-call alignment that C's jmp-into-frame shape
// produced with the extra `subq`).

#[cfg(all(not(test), not(feature = "std"), target_arch = "x86_64"))]
#[unsafe(no_mangle)]
#[unsafe(naked)]
pub extern "C" fn _start() -> ! {
    // SAFETY: naked entry — RSP and RBX are the kernel-established process
    // state (build_cpu_context); no Rust prologue has run.
    core::arch::naked_asm!(
        "and rsp, -16",
        "mov rdi, rbx",
        "call {birth}",
        "ud2",
        birth = sym rt_birth,
    );
}

#[cfg(test)]
mod crt0_tests {
    use super::*;

    /// Descriptor parse: happy path — build the raw image in host memory
    /// (same layout the kernel's load_vm_elf writes) and parse it back.
    #[test]
    fn test_read_process_strings_round_trip() {
        let argv0 = b"rt-birth\0";
        let argv = [argv0.as_ptr() as u64, 0];
        let env: [u64; 1] = [0];
        let raw = PsStringsRaw {
            argv_str: argv.as_ptr() as u64,
            n_argv: 1,
            env_str: env.as_ptr() as u64,
            n_env: 0,
        };
        let addr = &raw as *const PsStringsRaw as u64;
        let ps = unsafe { read_process_strings(addr) }.expect("descriptor parses");
        assert_eq!(ps.argument_count, 1);
        assert_eq!(ps.environment_count, 0);
        assert_eq!(ps.argument_list, argv.as_ptr() as u64);
        let argv0 = argv_storage_of(&ps).expect("argv[0] present");
        assert_eq!(argv0, b"rt-birth");
        assert_eq!(progname_of(argv0), b"rt-birth");
    }

    /// Zero descriptor pointer → ENOEXEC (document 02 §4: missing
    /// descriptor, no legal process image).
    #[test]
    fn test_read_process_strings_null_is_enoexec() {
        let err = unsafe { read_process_strings(0) }.unwrap_err();
        assert_eq!(err.to_i32(), minix_types::ENOEXEC);
    }

    /// Negative counts → EINVAL (via ProcessStrings::from_raw; the kernel
    /// never writes negatives, a corrupted descriptor must not wrap).
    #[test]
    fn test_read_process_strings_negative_count_is_einval() {
        let raw = PsStringsRaw {
            argv_str: 0x1000,
            n_argv: -1,
            env_str: 0x2000,
            n_env: 0,
        };
        let addr = &raw as *const PsStringsRaw as u64;
        let err = unsafe { read_process_strings(addr) }.unwrap_err();
        assert_eq!(err.to_i32(), minix_types::EINVAL);
    }

    /// Short-name scan (C crt0-common.c:158-167): last component, no
    /// slash → whole string, empty → empty.
    #[test]
    fn test_progname_of_byte_scan() {
        assert_eq!(progname_of(b"/usr/bin/mfs"), b"mfs");
        assert_eq!(progname_of(b"mfs"), b"mfs");
        assert_eq!(progname_of(b"a/b/c"), b"c");
        assert_eq!(progname_of(b""), b"");
        // Trailing slash: C's scan leaves the position after the last
        // slash — an empty tail is an empty name, not a panic.
        assert_eq!(progname_of(b"/bin/"), b"");
    }

    /// NULL argv slot inside a positive count: accessor reports None
    /// rather than returning a string (C would read argv[0]==NULL and
    /// take the empty_string branch — we guard the slot, not the bytes).
    /// The slot must exist in real memory: a NULL *value* is readable,
    /// unlike a bogus array *address* (that faults by design).
    #[test]
    fn test_argv_storage_null_first_slot_is_none() {
        let argv: [u64; 2] = [0, 0];
        let raw = PsStringsRaw {
            argv_str: argv.as_ptr() as u64,
            n_argv: 1,
            env_str: argv.as_ptr() as u64,
            n_env: 0,
        };
        let ps =
            ProcessStrings::from_raw(raw.argv_str, raw.n_argv, raw.env_str, raw.n_env).unwrap();
        assert!(argv_storage_of(&ps).is_none());
    }
}
