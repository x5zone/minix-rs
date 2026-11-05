//! Minix-RS `minix-service` — the thin shell over the request builder.
//!
//! Ground truth: `minix3/minix/commands/minix-service/minix-service.c` —
//! the verb table (`:43-69`), the option face (`:301-460`), the message
//! assembly (`:790-846`), and the usage block (`:177-224`). The deciding
//! halves live in `minix_service` ([`parse_args`]/[`build_request`]); this
//! program prints the usage faces, hands the built request to RS over one
//! `sendrec`, and exits with the reply's `m_type` as the status.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use minix_service::{Args, ParseError, Request, build_request};

fn usage_lines(problem: &str, app: &str) -> alloc::vec::Vec<String> {
    // C `print_usage` (`:177-224`): the warning line, the usage block, and
    // the option legend. The legend lines are C-verbatim.
    let mut lines = alloc::vec::Vec::new();
    lines.push(format!("Warning, {problem}\n"));
    lines.push(String::from("Usage:\n"));
    lines.push(format!(
        "    {app} [-c -r -n -p -q -a -o -s -x -y -z -d -u -m -e -f -t -b] (up|run|edit|update) <binary|SELF> [-args <args>] [-dev <special>] [-major <major_nr>] [-devmanid <dev_id>] [-period <ticks>] [-script <path>] [-label <name>] [-progname <name>] [-config <path>] [-lustate <state value|eval_expression>] [-lumaxtime <time>] [-heapprealloc <bytes>] [-mapprealloc <bytes>] [-trglabel <name>] [-asrcount <count>] [-restarts <restarts>]\n"
    ));
    for verb in ["down", "refresh", "restart", "clone", "unclone", "fi"] {
        lines.push(format!("    {app} {verb} <label>\n"));
    }
    lines.push(format!(
        "    {app} sysctl <srv_status|upd_start|upd_run|upd_stop|upd_status>\n"
    ));
    lines.push(format!("    {app} shutdown\n"));
    lines
}

fn report_parse_error(app: &str, e: &ParseError) -> ! {
    let problem = match e {
        ParseError::UnknownFlag(c) => {
            support::warn(format!("{app}: getopt failed: {c}\n").as_bytes());
            support::terminate(1);
        }
        ParseError::WrongNumberOfArguments => "wrong number of arguments",
        ParseError::IllegalRequestType => {
            // C `:410-412` exits ENOSYS.
            support::warn(b"Warning, illegal request type\n");
            support::terminate(minix_sys::ENOSYS);
        }
        ParseError::ActionRequiresBinary => "action requires a binary to start",
        ParseError::BatchNotSupported => "action does not support batch mode",
        ParseError::BinaryNotAbsolutePath => "binary should be absolute path",
        ParseError::OptionalArgumentNotComplete => "optional argument not complete",
        ParseError::BadPeriodArgument => "bad period argument",
        ParseError::SysctlNeedsSubtype => "sysctl needs a subtype",
    };
    for line in usage_lines(problem, app) {
        support::warn(line.as_bytes());
    }
    support::terminate(1)
}

fn real_main() -> ! {
    let argv: Vec<String> = support::args();
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();
    let app = args.first().copied().unwrap_or("minix-service");

    let parsed: Args = match minix_service::parse_args(&args[1.min(args.len())..]) {
        Ok(a) => a,
        Err(e) => report_parse_error(app, &e),
    };
    let request = parsed.request.unwrap_or(Request::Shutdown);

    // C `:839-843`: `_syscall(RS_PROC_NR, request, &m)` — one sendrec to RS;
    // the reply's m_type is the result.
    let mut msg = build_request(request, &parsed);
    // C `:839-843`: `_syscall(RS_PROC_NR, request, &m)` — the minix-sys
    // top-level `sendrec` wrapper (the sanctioned command face).
    let reply = minix_sys::sendrec(minix_sys::Endpoint::RS, &mut msg);
    match reply {
        Ok(()) => {
            // C `:845`: `result = m.m_type`.
            support::terminate(msg.m_type)
        }
        Err(e) => {
            // C `failure()` (`:227-232`): the strerror sentence is not
            // reproduced — the IpcError face carries no errno (the recorded
            // shape); the C exit code family is kept at 1.
            let reason = match e {
                minix_sys::IpcError::InvalidEndpoint => "invalid endpoint",
                minix_sys::IpcError::WouldBlock => "would block",
                minix_sys::IpcError::Interrupted => "interrupted",
                minix_sys::IpcError::NoPerm => "permission denied",
            };
            support::warn(
                format!("Request 0x{:x} to RS failed: {reason}\n", request.call_nr()).as_bytes(),
            );
            support::terminate(1);
        }
    }
}

#[cfg(all(not(test), target_os = "none"))]
#[unsafe(no_mangle)]
extern "Rust" fn main() -> i32 {
    real_main()
}

#[cfg(any(test, not(target_os = "none")))]
fn main() {
    real_main()
}
