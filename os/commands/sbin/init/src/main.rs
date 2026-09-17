//! Init: the first user process (PID 1, boot chain terminus).
//!
//! Entry flow (`minix3/sbin/init/init.c:229-367`): identity check, new
//! session, device probe, flag parsing, signal registration, stdio
//! cleanup, securelevel probe, then `transition()` (see 02).

mod clean_ttys;
mod contracts;
mod driver;
mod entry;
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

use driver::{DriverState, TTYS_PATH};
use entry::{InitialState, decide_entry, parse_boot_args};
use host::{InitHost, MinixSysHost};
use state_machine::StateKind;
use std::sync::Arc;

fn main() {
    minix_rt::init();
    let mut host = MinixSysHost;

    // C step 1: identity gate (init.c:242-249). getuid has no client
    // wrapper yet (E-INITSYS ②) — say so once and continue; pid 1 is
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
    // path (P1-2).
    let argv: Vec<String> = (0..minix_rt::crt0::argv_count())
        .filter_map(|i| minix_rt::crt0::argv_bytes(i))
        .map(|b| String::from_utf8_lossy(b).into_owned())
        .collect();
    let (boot_args, boot_warnings) = parse_boot_args(&argv);
    for warning in &boot_warnings {
        log::warning(&mut host, warning);
    }
    let decision = decide_entry(&boot_args, console_ok);

    // C step 5: signal registration (init.c:310-334). ENOSYS until
    // E-INITSYS ① — the machine runs with default dispositions until
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
        db: crate::session_db::HashMapDb::default(),
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
    let _ = TTYS_PATH; // read per read_ttys step through the seam
    driver::run_transition(&mut host, &mut state, initial);
}
