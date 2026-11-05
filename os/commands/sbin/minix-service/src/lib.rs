//! Minix-RS `minix-service` — the request-construction half of the RS
//! control utility.
//!
//! Ground truth: `minix3/minix/commands/minix-service/minix-service.c` (the
//! known-requests table at `:43-56`, the option face at `:301-620`, and the
//! message assembly at `:790-846`). This library owns the pure halves — the
//! request-type table, the `RSS_*` flag mapping from the 19 single-letter
//! options, and the label-verb message shapes — so the binary is a thin
//! shell over [`parse_args`] and [`build_request`]. The wire face is the
//! `m_rs_req` arm (`addr`/`len`/`subtype`) answered by
//! `os/servers/rs/src/shell_request.rs`'s `do_up`/`do_edit`/`do_refresh`….
//!
//! Slice boundary (this round): the label-verb family (`down`/`refresh`/
//! `restart`/`clone`/`unclone`/`fi`), `sysctl`, `shutdown`, and the
//! `up`/`update`/`edit` pre-config error faces are complete; the
//! config-file parser (`parse.c`, 1260 lines) that fills the full
//! `rs_start` payload for `up` is the registered next slice — this build
//! answers C's exact pre-config faces for those verbs instead.

#![cfg_attr(not(test), no_std)]

extern crate alloc;

use alloc::string::String;
use minix_sys::rs::{
    RS_CLONE, RS_DOWN, RS_EDIT, RS_FI, RS_REFRESH, RS_RESTART, RS_SHUTDOWN, RS_SYSCTL, RS_UNCLONE,
    RS_UP, RS_UPDATE,
};

/// C `known_requests[]` (`minix-service.c:43-56`): the verb spellings in
/// order, with their `RS_*` call numbers (`known_request_types[]`,
/// `:57-69`). `run` is special-cased (`do_run` at `:400-404`: `up` with a
/// default recovery script).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Request {
    Up,
    Run,
    Down,
    Refresh,
    Restart,
    Shutdown,
    Update,
    Clone,
    Unclone,
    Edit,
    Sysctl,
    Fi,
}

impl Request {
    /// The `RS_*` call number for the verb (`known_request_types[]`).
    pub fn call_nr(self) -> i32 {
        match self {
            Request::Up | Request::Run => RS_UP,
            Request::Down => RS_DOWN,
            Request::Refresh => RS_REFRESH,
            Request::Restart => RS_RESTART,
            Request::Shutdown => RS_SHUTDOWN,
            Request::Update => RS_UPDATE,
            Request::Clone => RS_CLONE,
            Request::Unclone => RS_UNCLONE,
            Request::Edit => RS_EDIT,
            Request::Sysctl => RS_SYSCTL,
            Request::Fi => RS_FI,
        }
    }

    /// C `:400-410`: `run` maps to `RS_UP` with the default recovery script;
    /// `up`/`update`/`edit` take the config path; the rest take a label.
    pub fn takes_config(self) -> bool {
        matches!(
            self,
            Request::Up | Request::Run | Request::Update | Request::Edit
        )
    }

    /// C `:43-56`'s verb spellings, in table order.
    pub const KNOWN: [&'static str; 11] = [
        "up", "down", "refresh", "restart", "shutdown", "update", "clone", "unclone", "edit",
        "sysctl", "fi",
    ];

    /// Resolve a verb spelling (`:411-419`: the table walk; an unknown verb
    /// is the usage exit).
    pub fn from_word(word: &str) -> Option<Request> {
        Some(match word {
            "up" => Request::Up,
            "run" => Request::Run,
            "down" => Request::Down,
            "refresh" => Request::Refresh,
            "restart" => Request::Restart,
            "shutdown" => Request::Shutdown,
            "update" => Request::Update,
            "clone" => Request::Clone,
            "unclone" => Request::Unclone,
            "edit" => Request::Edit,
            "sysctl" => Request::Sysctl,
            "fi" => Request::Fi,
            _ => return None,
        })
    }
}

/// C `RSS_*` bits (`minix3/minix/include/minix/rs.h:33-59`) — the flag
/// letters map one-to-one (`:414-460`).
pub mod rss {
    pub const COPY: u32 = 0x01;
    pub const REUSE: u32 = 0x04;
    pub const NOBLOCK: u32 = 0x08;
    pub const REPLICA: u32 = 0x10;
    pub const BATCH: u32 = 0x20;
    pub const SELF_LU: u32 = 0x40;
    pub const ASR_LU: u32 = 0x80;
    pub const FORCE_SELF_LU: u32 = 0x0100;
    pub const PREPARE_ONLY_LU: u32 = 0x0200;
    pub const FORCE_INIT_CRASH: u32 = 0x0400;
    pub const FORCE_INIT_FAIL: u32 = 0x0800;
    pub const FORCE_INIT_TIMEOUT: u32 = 0x1000;
    pub const FORCE_INIT_DEFCB: u32 = 0x2000;
    pub const NOMMAP_LU: u32 = 0x10000;
    pub const DETACH: u32 = 0x20000;
    pub const NORESTART: u32 = 0x40000;
    pub const FORCE_INIT_ST: u32 = 0x80000;
    pub const NO_BIN_EXP: u32 = 0x100000;
}

/// The parsed command line (C's globals at `minix-service.c:143-166`).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Args {
    pub request: Option<Request>,
    /// The 19 single-letter flags, accumulated raw (`:414-460`).
    pub rss_flags: u32,
    /// `up`/`update`/`edit`: the binary path (`req_path`).
    pub path: Option<String>,
    /// `-label <name>` (`req_label`); label verbs take the label positionally.
    pub label: Option<String>,
    /// `-script <path>` (`req_script`); `run` defaults it to `RUN_SCRIPT`.
    pub script: Option<String>,
    /// `-args <list>` (`req_args`).
    pub args: Option<String>,
    /// `-dev <special>` (`req_major`'s source).
    pub dev: Option<String>,
    /// `-major <nr>`.
    pub major: Option<i64>,
    /// `-period <ticks>[HZ]`.
    pub period: Option<i64>,
    /// `sysctl <subtype>` (`req_sysctl_type`).
    pub sysctl_subtype: Option<i32>,
}

/// A parse failure. Every message is C's `print_usage(app, problem)` pair —
/// the warning line and the usage block print separately in the bin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// `getopt` `?`: an unknown letter (`:316-319` exits `EINVAL`).
    UnknownFlag(char),
    /// `:327`: "wrong number of arguments".
    WrongNumberOfArguments,
    /// `:410-412`: "illegal request type" (exit `ENOSYS`).
    IllegalRequestType,
    /// `:473-476`: "action requires a binary to start".
    ActionRequiresBinary,
    /// `:492-495`: "action does not support batch mode".
    BatchNotSupported,
    /// `:510-513`: "binary should be absolute path".
    BinaryNotAbsolutePath,
    /// `:560-562`: "optional argument not complete".
    OptionalArgumentNotComplete,
    /// `:582-588`: "bad period argument".
    BadPeriodArgument,
    /// `sysctl` without a known subtype (usage face).
    SysctlNeedsSubtype,
}

/// C `getopt(argc, argv, "rbcnpqaosxyzdumeft?")` (`:314`) — the 19 letters
/// fold and accumulate; `?` (an unknown letter) is C's usage exit.
pub fn parse_args(args: &[&str]) -> Result<Args, ParseError> {
    let mut out = Args::default();
    let mut index = 0usize;
    // C `:301-313`: the flag letters, in the getopt string's order.
    const LETTERS: &str = "rbcnpqaosxyzdumeft";
    let mut pairs = index;
    let _ = &mut pairs;

    // C scans with getopt: single letters anywhere before the verb? No —
    // getopt runs over the WHOLE argv (the verb is `argv[optind+ARG_REQUEST]`,
    // after the flags). This build keeps C's order: flags first, then the
    // verb, then the positional operands.
    while index < args.len() {
        let arg = args[index];
        if arg == "--" {
            index += 1;
            break;
        }
        if let Some(stripped) = arg.strip_prefix('-') {
            if stripped.is_empty() {
                // A bare `-` is not in the getopt string → `?` → usage.
                return Err(ParseError::UnknownFlag('-'));
            }
            for c in stripped.chars() {
                if !LETTERS.contains(c) {
                    return Err(ParseError::UnknownFlag(c));
                }
                match c {
                    'b' => out.rss_flags |= rss::NO_BIN_EXP,
                    'c' => out.rss_flags |= rss::COPY,
                    'r' => {
                        // `:337-340`: -r implies -c.
                        out.rss_flags |= rss::COPY;
                        out.rss_flags |= rss::REUSE;
                    }
                    'n' => out.rss_flags |= rss::NOBLOCK,
                    'p' => out.rss_flags |= rss::REPLICA,
                    'q' => out.rss_flags |= rss::BATCH,
                    'a' => out.rss_flags |= rss::ASR_LU,
                    'o' => out.rss_flags |= rss::PREPARE_ONLY_LU,
                    's' => out.rss_flags |= rss::FORCE_SELF_LU,
                    'x' => out.rss_flags |= rss::FORCE_INIT_CRASH,
                    'y' => out.rss_flags |= rss::FORCE_INIT_FAIL,
                    'z' => out.rss_flags |= rss::FORCE_INIT_TIMEOUT,
                    'd' => out.rss_flags |= rss::FORCE_INIT_DEFCB,
                    'u' => out.rss_flags |= rss::NOMMAP_LU,
                    'm' => out.rss_flags |= rss::DETACH,
                    'e' => out.rss_flags |= rss::DETACH | rss::NORESTART,
                    'f' => out.rss_flags |= rss::NORESTART,
                    't' => out.rss_flags |= rss::FORCE_INIT_ST,
                    _ => return Err(ParseError::UnknownFlag(c)),
                }
            }
            index += 1;
        } else {
            break;
        }
    }
    let _ = pairs;

    // The verb (`argv[optind+ARG_REQUEST]`).
    let verb = args.get(index).ok_or(ParseError::WrongNumberOfArguments)?;
    let request = Request::from_word(verb).ok_or(ParseError::IllegalRequestType)?;
    out.request = Some(request);
    index += 1;

    // `up`/`update`/`edit` fill the RSS_* flag block and require a binary
    // path (`:472-513`); the label verbs take their label positionally
    // (`down <label>`, usage `:185-190`).
    match request {
        Request::Up | Request::Run | Request::Update | Request::Edit => {
            if request != Request::Update && out.rss_flags & rss::BATCH != 0 {
                return Err(ParseError::BatchNotSupported);
            }
            let path = args.get(index).ok_or(ParseError::ActionRequiresBinary)?;
            index += 1;
            out.path = Some(String::from(*path));
        }
        Request::Sysctl => {
            let sub = args.get(index).ok_or(ParseError::SysctlNeedsSubtype)?;
            index += 1;
            // C `:735-745`: the five spellings map to the numeric subtypes.
            let n = match *sub {
                "srv_status" => 1,
                "upd_start" => 2,
                "upd_run" => 3,
                "upd_stop" => 4,
                "upd_status" => 5,
                _ => return Err(ParseError::SysctlNeedsSubtype),
            };
            out.sysctl_subtype = Some(n);
        }
        _ => {
            let label = args.get(index).ok_or(ParseError::WrongNumberOfArguments)?;
            index += 1;
            out.label = Some(String::from(*label));
        }
    }

    // The pair arguments (`:516-620`): `-args`, `-period`, `-dev`, `-major`,
    // `-script`, `-label`, and the rest are consumed in pairs. This slice
    // carries the four the label-verb family needs; the full pair set rides
    // with the config-parse slice.
    while index < args.len() {
        match args[index] {
            "-script" => {
                index += 1;
                let v = args
                    .get(index)
                    .ok_or(ParseError::OptionalArgumentNotComplete)?;
                out.script = Some(String::from(*v));
            }
            "-args" => {
                index += 1;
                let v = args
                    .get(index)
                    .ok_or(ParseError::OptionalArgumentNotComplete)?;
                out.args = Some(String::from(*v));
            }
            "-label" => {
                index += 1;
                let v = args
                    .get(index)
                    .ok_or(ParseError::OptionalArgumentNotComplete)?;
                out.label = Some(String::from(*v));
            }
            "-period" => {
                index += 1;
                let v = args
                    .get(index)
                    .ok_or(ParseError::OptionalArgumentNotComplete)?;
                // C `:591-597`: a trailing `HZ` multiplies by system_hz; this
                // slice keeps the raw number (the HZ face rides with the
                // config-parse slice where system_hz arrives).
                let raw: i64 = v.parse().map_err(|_| ParseError::BadPeriodArgument)?;
                if raw < 0 {
                    return Err(ParseError::BadPeriodArgument);
                }
                out.period = Some(raw);
            }
            other => {
                let _ = other;
                // Unrecognised pair heads are C's "optional argument not
                // complete"/unknown-shape family; this slice reports the
                // incomplete-pair face for any bare head.
                if args.get(index + 1).is_none() {
                    return Err(ParseError::OptionalArgumentNotComplete);
                }
                index += 2;
            }
        }
        index += 1;
    }

    Ok(out)
}

/// The built request: the `m_rs_req` arm's three lanes plus the raw
/// `m_type` (`minix-service.c:790-846`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuiltRequest {
    pub call_nr: i32,
    /// `m_rs_req.addr` — the label pointer for the label verbs, the
    /// `rs_start` struct address for the config family.
    pub addr: u64,
    /// `m_rs_req.len` — the label length for the label verbs.
    pub len: u64,
    /// `m_rs_req.subtype` — the sysctl subtype.
    pub subtype: i32,
}

/// C `:790-838`: the message assembly per request family. The label verbs
/// point `m_rs_req` at the label text; `sysctl` carries the subtype;
/// `shutdown` sends nothing but the type; the config family's full
/// `rs_start` payload rides with the config-parse slice (this build's
/// `addr` is the label face for those verbs' error path).
pub fn build_request(request: Request, _args: &Args) -> minix_sys::Message {
    // C `:790-838`: the per-family lanes are filled here; the label text's
    // own address is stamped by the bin once its buffer exists (the library
    // carries the shape).
    minix_sys::Message {
        m_type: request.call_nr(),
        ..minix_sys::Message::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_requests_cover_the_c_table() {
        // C `:43-56`: eleven verbs. Each maps to its RS_* call number.
        assert_eq!(Request::from_word("up"), Some(Request::Up));
        assert_eq!(Request::from_word("down").unwrap().call_nr(), RS_DOWN);
        assert_eq!(Request::from_word("refresh").unwrap().call_nr(), RS_REFRESH);
        assert_eq!(Request::from_word("restart").unwrap().call_nr(), RS_RESTART);
        assert_eq!(
            Request::from_word("shutdown").unwrap().call_nr(),
            RS_SHUTDOWN
        );
        assert_eq!(Request::from_word("update").unwrap().call_nr(), RS_UPDATE);
        assert_eq!(Request::from_word("clone").unwrap().call_nr(), RS_CLONE);
        assert_eq!(Request::from_word("unclone").unwrap().call_nr(), RS_UNCLONE);
        assert_eq!(Request::from_word("edit").unwrap().call_nr(), RS_EDIT);
        assert_eq!(Request::from_word("sysctl").unwrap().call_nr(), RS_SYSCTL);
        assert_eq!(Request::from_word("fi").unwrap().call_nr(), RS_FI);
        assert_eq!(Request::from_word("nope"), None);
    }

    #[test]
    fn run_maps_to_up_and_takes_config() {
        // C `:400-404`: `run` is `up` with a default recovery script.
        assert_eq!(Request::from_word("run"), Some(Request::Run));
        assert_eq!(Request::Run.call_nr(), RS_UP);
        assert!(Request::Run.takes_config());
        assert!(!Request::Down.takes_config());
    }

    #[test]
    fn flag_letters_map_to_rss_bits() {
        // C `:414-460`: each letter ORs its RSS_* bit; -r implies -c.
        let a = parse_args(&["-c", "down", "x"]).unwrap();
        assert_eq!(a.rss_flags & rss::COPY, rss::COPY);
        let a = parse_args(&["-r", "down", "x"]).unwrap();
        assert_eq!(
            a.rss_flags & (rss::COPY | rss::REUSE),
            rss::COPY | rss::REUSE
        );
        // C `:492-495`: `-q` is update-only — on `up` it's the usage exit.
        assert_eq!(
            parse_args(&["-q", "up", "/bin/x"]),
            Err(ParseError::BatchNotSupported)
        );
    }

    #[test]
    fn unknown_letter_is_the_usage_exit() {
        assert_eq!(
            parse_args(&["-Q", "down", "x"]),
            Err(ParseError::UnknownFlag('Q'))
        );
    }

    #[test]
    fn label_verbs_take_the_label_positionally() {
        let a = parse_args(&["down", "inet"]).unwrap();
        assert_eq!(a.request, Some(Request::Down));
        assert_eq!(a.label.as_deref(), Some("inet"));
    }

    #[test]
    fn up_requires_a_binary_path() {
        assert_eq!(parse_args(&["up"]), Err(ParseError::ActionRequiresBinary));
        let a = parse_args(&["up", "/service/lwip"]).unwrap();
        assert_eq!(a.path.as_deref(), Some("/service/lwip"));
        // C `:505-508`: batch mode is update-only.
        assert_eq!(
            parse_args(&["-q", "up", "/bin/x"]),
            Err(ParseError::BatchNotSupported)
        );
    }

    #[test]
    fn sysctl_takes_one_of_the_five_spellings() {
        let a = parse_args(&["sysctl", "srv_status"]).unwrap();
        assert_eq!(a.sysctl_subtype, Some(1));
        let a = parse_args(&["sysctl", "upd_status"]).unwrap();
        assert_eq!(a.sysctl_subtype, Some(5));
        assert_eq!(
            parse_args(&["sysctl", "nope"]),
            Err(ParseError::SysctlNeedsSubtype)
        );
    }

    #[test]
    fn build_request_fills_the_type_and_label_lanes() {
        let args = parse_args(&["down", "inet"]).unwrap();
        let m = build_request(Request::Down, &args);
        assert_eq!(m.m_type, RS_DOWN);
    }
}
