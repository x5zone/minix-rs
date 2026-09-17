//! Init: the first user process (PID 1, boot chain terminus).
//!
//! Entry flow (`minix3/sbin/init/init.c:229-367`): identity check, new
//! session, device probe, flag parsing, signal registration (see 02),
//! stdio cleanup, securelevel probe (see 12), then `transition()` (see 02).

mod clean_ttys;
mod contracts;
mod entry;
mod host;
mod log;
mod multi_user;
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

use entry::{InitialState, decide_entry, parse_boot_args};
use host::MinixSysHost;

fn main() {
    minix_rt::init();

    // S6: parse flags (pure; warnings go to the log channel in 03).
    let argv: Vec<String> = std::env::args().collect();
    let (boot_args, _warnings) = parse_boot_args(&argv);

    // S4: device probe through the host seam. On the live host this is
    // an honest ENOSYS until E-INITSYS lands open/stat, which counts as
    // "no console" — the same single-user fallback as the C boot path
    // (init.c:269-270).
    let console_ok = entry::console_present(&MinixSysHost, "/dev/console");

    let decision = decide_entry(&boot_args, console_ok);

    // S7/S8 + transition() land with the state machine wiring. For
    // now, record the decision and wait so the binary has
    // well-defined behaviour.
    match decision.initial {
        InitialState::Runcom => {}
        InitialState::SingleUser => {}
    }

    loop {
        std::thread::park();
    }
}
