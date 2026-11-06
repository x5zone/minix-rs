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
        ParseError::BadLuMaxTime => "bad live update max time",
        ParseError::NullLuState => "null live update state",
        ParseError::BadHeapPrealloc => "bad heap prealloc bytes",
        ParseError::BadAsrCount => "bad ASR count",
        ParseError::BadRestarts => "bad number of restarts",
        ParseError::MajorAlreadySet => "major already set",
        ParseError::UnknownOptionalArgument => "unknown optional argument given",
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
    // C `:756-816` / `:819-833`: the `up` family carries the packed
    // `rs_start` image; the label verbs carry the label pointer.
    match request {
        Request::Up | Request::Run | Request::Update | Request::Edit => {
            match prepare_up_image(&parsed) {
                Ok(image) => {
                    msg.set_rs_req_payload(image.address(), 0);
                }
                Err(problem) => {
                    support::warn(format!("couldn't parse config: {problem}\n").as_bytes());
                    support::terminate(1);
                }
            }
        }
        Request::Down
        | Request::Refresh
        | Request::Restart
        | Request::Clone
        | Request::Unclone
        | Request::Fi => {
            if let Some(label) = &parsed.label {
                msg.set_rs_req_payload(label.as_ptr() as u64, 0);
            }
        }
        Request::Sysctl | Request::Shutdown => {}
    }
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

/// C `parse_config` (`parse.c:1229-1255`) + the `up` image assembly: pick the
/// config file (custom, package dir, conf dir, then the default), read the
/// passwd database for the `service` entry, and pack the request.
fn prepare_up_image(args: &Args) -> Result<minix_service::RequestImage, String> {
    use minix_proctools::configfile::config_read_text;
    use minix_service::{
        PATH_SYSTEM_CONF, PATH_SYSTEM_CONF_DIR, PATH_SYSTEM_CONF_PKG_DIR, RsStartDefaults,
        build_up_request, progname_of,
    };

    let path = args
        .path
        .as_deref()
        .ok_or_else(|| String::from("no binary path"))?;
    let progname = args
        .progname
        .clone()
        .unwrap_or_else(|| String::from(progname_of(path)));

    let (raw, file) = if let Some(custom) = &args.config {
        (
            support::read_file(custom).map_err(|_| format!("cannot read {custom}"))?,
            custom.clone(),
        )
    } else {
        let pkg = format!("{PATH_SYSTEM_CONF_PKG_DIR}/{progname}");
        let dir = format!("{PATH_SYSTEM_CONF_DIR}/{progname}");
        if let Ok(text) = support::read_file(&pkg) {
            (text, pkg)
        } else if let Ok(text) = support::read_file(&dir) {
            (text, dir)
        } else {
            (
                support::read_file(PATH_SYSTEM_CONF)
                    .map_err(|_| String::from("cannot read the system config"))?,
                String::from(PATH_SYSTEM_CONF),
            )
        }
    };
    let text = String::from_utf8_lossy(&raw).into_owned();
    let users_raw = support::read_file("/etc/passwd").unwrap_or_default();
    let users = String::from_utf8_lossy(&users_raw).into_owned();
    let lines = config_read_text(&text, &file, 0, &|_| None).map_err(|e| e.message)?;
    // `parse.c:1162`: the uid default is the `service` passwd entry's.
    let defaults = RsStartDefaults {
        service_uid: minix_service::resolve_service_uid(&users),
        ..RsStartDefaults::default()
    };
    build_up_request(&lines, &progname, &defaults, args, Some(users.as_str()))
        .map_err(|e| e.message)
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
