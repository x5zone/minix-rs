//! Init: the first user process (PID 1, boot chain terminus).
//!
//! Entry flow (`minix3/sbin/init/init.c:229-367`): identity check, new
//! session, device probe, flag parsing, signal registration, stdio
//! cleanup, securelevel probe, then `transition()` (see 02).

// Mechanical gate: every discarded `Result` must carry a written reason.
#![warn(clippy::let_underscore_must_use)]

// The on-target user image targets `x86_64-unknown-none` (the rt-birth
// precedent; the workspace .cargo/config configures static relocation for
// that target). The binary is freestanding PID 1 on that side: the entry
// goes through minix-rt's `_start` birth chain
// (os/libs/minix-rt/src/crt0.rs:334, RBX carries the ps_strings
// descriptor), and this crate supplies its own panic handler (bottom of
// this file).
// Gating uses `target_os = "none"` rather than `not(test)`: a hosted
// (linux-gnu/musl) development build must keep the std form to link —
// rustc always passes Scrt1.o for hosted targets (its `_start` needs
// `main` + `__libc_start_main`), so no_std + no_main cannot link on the
// host; the freestanding form holds only on the real target. Under
// `cfg(test)` the build is std like the hosted one: the test binary links
// real std, whose panic_impl/entry are mutually exclusive with minix-rt's
// no_std-only items (an E0152 deadlock, resolved by the workspace
// declaration plus the dev-dependency declaration together).
#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

// The containers live in `alloc` (not the std prelude) so the module
// tree is `no_std`-movable: `alloc::sync::Arc` and `alloc::collections`
// resolve identically in the std test build and in a freestanding build.
// `#[macro_use]` carries the `vec!`/`format!` macros crate-wide; the
// container TYPES (`Vec`/`String`/...) are imported per module from
// `alloc`, mirroring how `alloc::sync::Arc` is already imported.
#[macro_use]
extern crate alloc;

mod clean_ttys;
mod contracts;
mod driver;
mod entry;
mod execve;
mod host;
mod log;
mod multi_user;
mod password;
mod runcom;
mod session;
mod session_db;
mod signal_state;
mod shutdown;
mod single_user;
mod state_machine;
mod sysctl;
mod ttys;
mod utmp;
mod wait;

use alloc::{string::String, vec::Vec};
use alloc::string::ToString;
use driver::{DriverState, TTYS_PATH};
use entry::{InitialState, decide_entry, parse_boot_args};
use host::{InitHost, MinixSysHost};
use state_machine::StateKind;
use alloc::sync::Arc;

/// PID 1's startup body: the eight steps wired up (the Rust side of C
/// init.c:229-367), ending in a call to `run_transition` (`-> !`, the state
/// machine never returns). Both entry forms share this function; see the
/// separately cfg-gated `main`s below.
fn run_init() -> ! {
    minix_rt::init();
    let mut host = MinixSysHost::default();
    // C step 1: identity gate (init.c:242-249). getuid has no client
    // wrapper yet (E-INITSYS 2) — say so once and continue; pid 1 is
    // still verified whenever getpid answers.
    match host.getuid() {
        Ok(0) => {}
        Ok(uid) => log::emergency(&mut host, &format!("init must run as root (uid {uid})")),
        Err(e) => log::warning(&mut host, &format!("identity check unavailable: {e}")),
    }
    match host.getpid() {
        Ok(1) | Err(_) => {}
        Ok(pid) => {
            log::emergency(&mut host, &format!("init already running (pid {pid})"));
            host.exit_process(1);
        }
    }

    // C step 2: session leadership (init.c:255).
    if let Err(e) = host.setsid() {
        log::warning(&mut host, &format!("setsid unavailable: {e}"));
    }

    // C step 3: device probe (init.c:269-270). ENOSYS counts as "no
    // console" and forces single-user, the same fallback as C's
    // missing console.
    let console_ok = entry::ensure_console(&mut host, single_user::CONSOLE_PATH);

    // C step 4: flag parsing (init.c:287-303). Args come from the
    // birth descriptor via minix-rt — no std env anywhere on the boot
    // path.
    let argv: Vec<String> = (0..minix_rt::crt0::argv_count())
        .filter_map(minix_rt::crt0::argv_bytes)
        .map(|b| String::from_utf8_lossy(b).into_owned())
        .collect();
    let (boot_args, boot_warnings) = parse_boot_args(&argv);
    for warning in &boot_warnings {
        log::warning(&mut host, warning);
    }
    let decision = decide_entry(&boot_args, console_ok);

    // C step 5: signal registration (init.c:310-334). ENOSYS until
    // E-INITSYS 1 — the machine runs with default dispositions until
    // then, which the boot log says out loud.
    if let Err(e) = host.register_handlers(&host::default_signal_spec()) {
        log::warning(&mut host, &format!("signal registration unavailable: {e}"));
    }

    // C step 6: close fds 0/1/2 (init.c:339-341).
    if let Err(e) = host.close_std_fds() {
        log::warning(&mut host, &format!("stdio cleanup unavailable: {e}"));
    }

    // C step 7: securelevel probe (init.c:353) — the level seeds the
    // single-user password gate (doc 12). C reads -1 when the kernel
    // has no level support.
    let from_securitylevel = host.securitylevel().ok().flatten().unwrap_or(-1);

    // C step 8: transition() — never returns. The password verifier is
    // filled by the password module (ARCH A-12); until then the gate
    // stays closed off, matching an empty `pw_passwd`.
    let signals = Arc::new(signal_state::SignalState::default());
    let mut state = DriverState {
        boot_args,
        mode: decision.runcom_mode,
        console_ok,
        console_secure: false,
        from_securitylevel,
        sessions: Vec::new(),
        db: crate::session_db::SessionMapDb::default(),
        signals: signals.clone(),
        sessions_seen: false,
        did_multiuser_chroot: false,
        rootdir: "/".to_string(),
        // C step 7b: read root's hash for the single-user gate
        // (getpwnam, init.c:733) — an unreadable passwd means no gate,
        // exactly the C shape when getpwnam fails.
        root_verify: host
            .read_file("/etc/passwd")
            .ok()
            .and_then(|body| password::root_password_hash(&body))
            .and_then(|hash| password::build_verifier(&hash)),
    };

    let initial = match decision.initial {
        InitialState::Runcom => StateKind::Runcom,
        InitialState::SingleUser => StateKind::SingleUser,
    };
    //[SD19-gate] best effort: reading TTYS_PATH through the seam may fail; the result is intentionally dropped here
    #[allow(clippy::let_underscore_must_use)]
    let _ = TTYS_PATH; // read per read_ttys step through the seam
    driver::run_transition(&mut host, &mut state, initial);
}

/// Target entry: the exact shape of the crt0 birth-chain Consumer contract
/// (crt0.rs Consumer contract: `#[no_mangle]` carries the symbol name —
/// `unsafe extern "Rust" { fn main() -> i32; }` resolves by name without
/// checking the signature, so a `()` form links too but nobody writes the
/// return register, and stage-6's `exit(main())` reads garbage; the rt-birth
/// precedent is main.rs:117). `run_init` diverges, so the i32 slot is never
/// produced — the contract only requires the ABI shape to agree.
#[cfg(all(not(test), target_os = "none"))]
#[unsafe(no_mangle)]
extern "Rust" fn main() -> i32 {
    run_init()
}

/// Host entry: std builds go through rustc's start glue, so `main`'s return
/// type must implement `Termination` — `i32` does not (E0277 observed on
/// rustc 1.94), `()` does; `run_init` diverges, so this function never
/// returns either, and the exit-code path exists only at the type level.
#[cfg(any(test, not(target_os = "none")))]
fn main() {
    run_init()
}

/// PID 1's panic handler (the gate matches the crate's freestanding form,
/// see the top of the file).
///
/// Ruling: init uses minix-rt's unified rendering —
/// `minix_rt::diag::format_panic_report` (stack buffer, no allocator, no
/// system call, the same form as minix-rt lib.rs:287) — plus an
/// init-specific ending: PID 1 must not spin and hang the whole machine
/// (the no-hook branch of minix-rt's built-in handler has already moved to
/// the SYS_DIAGCTL+exit ladder, dated 2026-09-21; this handler keeps the same
/// ladder in direct form for PID 1, without depending on the hook registry),
/// so after rendering, the report goes to the kernel diagnostic console
/// through SYS_DIAGCTL (C's panic-dies-at-once semantics, the emit→exit
/// ladder of minix3/minix/lib/libsys/panic.c:51-58), followed by a non-zero
/// `exit` — the kernel sees "init died" just as in the C world.
///
/// minix-rt's own `#[panic_handler]` sits behind the `panic-handler` feature
/// (minix-rt lib.rs:279), and this crate does not enable it — exactly one
/// handler in the graph, never E0152. Under host/std builds this function is
/// compiled out entirely (std's panic_impl takes over).
#[cfg(all(not(test), target_os = "none"))]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo<'_>) -> ! {
    let mut buffer = [0u8; 128];
    let length = minix_rt::diag::format_panic_report(
        info.location().map(|location| (location.file(), location.line())),
        info.message(),
        &mut buffer,
    );
    // A rendering failure cannot panic from here (format_panic_report
    // silently truncates bytes beyond the buffer); a non-UTF-8 message is
    // replaced by an honest placeholder. A failed console send is not
    // retried — a kernel call failing on the panic path has no more
    // meaningful destination, and the visible outcome of the exit (init
    // going away) is unchanged.
    let message = core::str::from_utf8(&buffer[..length])
        .unwrap_or("init panicked (non-utf8 message)");
    //[SD19-gate] best effort: a failed diagnostic write has no better destination on the panic path; the exit below still makes init's death visible
    #[allow(clippy::let_underscore_must_use)]
    let _ = minix_sys::syscall::sys_diagctl_write(
        &minix_sys::syscall::DirectKernelCallTransport,
        message,
    );
    minix_sys::exit(1)
}
