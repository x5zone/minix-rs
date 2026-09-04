//! Init: the first user process (PID 1, boot chain terminus).
//!
//! Entry flow (`minix3/sbin/init/init.c:229-367`): identity check, new
//! session, device probe, flag parsing, signal registration (see 02),
//! stdio cleanup, securelevel probe (see 12), then `transition()` (see 02).

mod clean_ttys;
mod contracts;
mod entry;
mod log;
mod multi_user;
mod runcom;
mod session;
mod session_db;
mod shutdown;
mod single_user;
mod state_machine;
mod sysctl;
mod ttys;
mod utmp;

use entry::{DeviceEnsureOutcome, DeviceProbe, FakeDeviceProbe, InitialState, decide_entry, parse_boot_args};

fn main() {
    minix_rt::init();

    // S6: parse flags (pure; warnings go to the log channel in 03).
    let argv: Vec<String> = std::env::args().collect();
    let (boot_args, _warnings) = parse_boot_args(&argv);

    // S4: device probe. Live probe lands with minix_sys syscalls;
    // assume console present until then (normal boot path).
    let probe = FakeDeviceProbe {
        present: true,
        ensure_outcome: DeviceEnsureOutcome::Ok,
    };
    let console_ok = match probe.ensure_devices() {
        DeviceEnsureOutcome::Ok => true,
        DeviceEnsureOutcome::FellBackToSingleUser | DeviceEnsureOutcome::Failed => false,
    };

    let decision = decide_entry(&boot_args, console_ok);

    // S7/S8 + transition() land in 02 (state machine). For now, record
    // the decision and wait so the binary has well-defined behaviour.
    match decision.initial {
        InitialState::Runcom => {}
        InitialState::SingleUser => {}
    }

    loop {
        std::thread::park();
    }
}
