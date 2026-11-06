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

/// `S_IF*` file-type bits (`minix3/sys/sys/stat.h`: `S_IFMT 0170000`,
/// `S_IFCHR 0020000`, `S_IFBLK 0060000`).
const S_IFMT: u32 = 0o170000;
const S_IFCHR: u32 = 0o020000;
const S_IFBLK: u32 = 0o060000;

/// C `parse_arguments`' `-dev` branch (`minix-service.c:527-545`): stat the
/// special file, require a block or character device, refuse a second major
/// source, and take `major(st_rdev)` — `((dev & 0x000fff00) >> 8)`.
fn resolve_dev_major(args: &mut Args, app: &str) {
    let Some(dev) = args.dev.clone() else {
        return;
    };
    let mut st = minix_sys::Stat::default();
    if let Err(e) = minix_sys::stat(&dev, &mut st) {
        support::warn(format!("{dev}: cannot stat\n").as_bytes());
        for line in usage_lines("couldn't get status of device", app) {
            support::warn(line.as_bytes());
        }
        support::terminate(e.to_i32());
    }
    let kind = st.st_mode & S_IFMT;
    if kind != S_IFBLK && kind != S_IFCHR {
        for line in usage_lines("special file is not a device", app) {
            support::warn(line.as_bytes());
        }
        support::terminate(minix_sys::EINVAL);
    }
    if args.major.is_some_and(|major| major != 0) {
        for line in usage_lines("major already set", app) {
            support::warn(line.as_bytes());
        }
        support::terminate(minix_sys::EINVAL);
    }
    args.major = Some(((st.st_rdev & 0x000f_ff00) >> 8) as i64);
}

/// `-period`/`-maxtime`'s `HZ` suffix: C multiplies by `system_hz`
/// (`sysconf(_SC_CLK_TCK)`, `:588-604`). The library keeps the raw ticks;
/// the bin owns the clock-rate lookup. `-maxtime`'s "0 → no timeout" (-1)
/// is preserved untouched.
fn scale_hz_suffixes(args: &mut Args) {
    if !(args.period_hz || args.lu_maxtime_hz) {
        return;
    }
    let hz = minix_sys::syscall::sys_get_hz(&minix_sys::syscall::DirectKernelCallTransport)
        .ok()
        .filter(|hz| *hz > 0)
        .unwrap_or(1);
    let hz = i64::from(hz);
    if args.period_hz
        && let Some(period) = args.period
    {
        args.period = Some(period * hz);
    }
    if args.lu_maxtime_hz
        && let Some(maxtime) = args.lu_maxtime
        && maxtime > 0
    {
        args.lu_maxtime = Some(maxtime * hz);
    }
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
        ParseError::BadIpcBlacklist(errno) => {
            for line in usage_lines("bad IPC blacklist filter", app) {
                support::warn(line.as_bytes());
            }
            support::terminate(errno.to_i32());
        }
        ParseError::BadIpcWhitelist(errno) => {
            for line in usage_lines("bad IPC whitelist filter", app) {
                support::warn(line.as_bytes());
            }
            support::terminate(errno.to_i32());
        }
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

    let mut parsed: Args = match minix_service::parse_args(&args[1.min(args.len())..]) {
        Ok(a) => a,
        Err(e) => report_parse_error(app, &e),
    };
    // C `:527-545`: `-dev` resolves through `stat` + `major()`; `:591-604`:
    // `-period`/`-maxtime` with an `HZ` suffix scale by `system_hz`.
    resolve_dev_major(&mut parsed, app);
    scale_hz_suffixes(&mut parsed);
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
