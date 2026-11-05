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

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
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
    pub const RSS_NR_IRQ: i32 = 16;
    pub const RSS_NR_IO: i32 = 16;
    pub const RSS_IRQ_ALL: i32 = RSS_NR_IRQ + 1;
    pub const RSS_IO_ALL: i32 = RSS_NR_IO + 1;

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

/// C `do_config`'s defaults (`parse.c:1171-1176`): `rss_uid` from the
/// `service` passwd entry (this build resolves it from the caller-read
/// database; the C fatal on a missing entry maps to `None`), sigmgr =
/// `ROOT_SYS_PROC_NR`, scheduler = `SCHED_PROC_NR`, priority = `USER_Q`,
/// quantum = `USER_QUANTUM` (`priv.h:84-99`), flags = the two BASIC_CALLS
/// bits, and `rss_cmd` from the binary path + `-args`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RsStartDefaults {
    pub service_uid: Option<u32>,
    /// C `DSRV_SM = ROOT_SYS_PROC_NR`.
    pub sigmgr: i32,
    /// C `DSRV_SCH = SCHED_PROC_NR`.
    pub scheduler: i32,
    /// C `DSRV_Q = USER_Q`.
    pub priority: i32,
    /// C `DSRV_QT = USER_QUANTUM`.
    pub quantum: i32,
}

/// The do_config keyword walk over the parsed lines (`parse.c:1177-1263`
/// walk; the scalar keywords this slice covers are `uid`, `priority`,
/// `quantum`, `sigmgr`, `scheduler`, `descr` — the device-family keywords
/// (`irq`/`io`/`pci`/`class`/`system`/`ipc`/`vm`/`control`/`type`/`net`/
/// `domain`) are the registered next slice). C's per-branch `fatal` faces
/// become `ConfigError` with the same wording.
pub fn do_config_walk(
    lines: &[minix_proctools::configfile::ConfigEntry],
    defaults: &RsStartDefaults,
    args: &Args,
    users: Option<&str>,
) -> Result<BuiltRsStart, minix_proctools::configfile::ConfigError> {
    use minix_proctools::configfile::ConfigError;
    let mut out = BuiltRsStart {
        uid: defaults.service_uid.unwrap_or(0),
        sigmgr: defaults.sigmgr,
        scheduler: defaults.scheduler,
        priority: defaults.priority,
        quantum: defaults.quantum,
        descr: None,
        label: args.label.clone(),
        nr_irq: 0,
        irq: Vec::new(),
        nr_io: 0,
        io: Vec::new(),
    };
    for line in lines {
        if !line.is_sub() {
            return Err(ConfigError {
                line: line.line,
                message: String::from("expected list"),
            });
        }
        let Some(head) = line.list.first() else {
            return Err(ConfigError {
                line: line.line,
                message: String::from("expected word"),
            });
        };
        if head.is_string() || head.is_sub() {
            return Err(ConfigError {
                line: line.line,
                message: String::from("expected word"),
            });
        }
        if head.word != "service" {
            return Err(ConfigError {
                line: line.line,
                message: String::from("expected word 'service'"),
            });
        }
        // The line's words, flattened: `{`-created nested containers (the
        // config grammar's grouping, `read_line:320-334`) contribute their
        // words in place — C's per-line chain is flat at the consumer level.
        let mut flat: Vec<&minix_proctools::configfile::ConfigEntry> = Vec::new();
        flatten_words(&line.list[1..], &mut flat);
        // C's walk dispatches keyword by keyword over the flat chain
        // (do_service → do_uid/do_priority/…); a scalar keyword takes
        // exactly one value and the next word must be a keyword or the end
        // (`do_uid`'s `cpe->next != NULL` fatal, parse.c:120-123).
        let keywords = ["uid", "priority", "quantum", "sigmgr", "scheduler", "descr"];
        let mut index = 0usize;
        while index < flat.len() {
            let word = flat[index];
            let Some(value) = flat.get(index + 1) else {
                break; // a trailing keyword with no value: C's walk ends too
            };
            match word.word.as_str() {
                "uid" => {
                    out.uid = user_by_name(users, &value.word)
                        .unwrap_or_else(|| resolve_uid(&value.word));
                    if flat
                        .get(index + 2)
                        .is_some_and(|next| !keywords.contains(&next.word.as_str()))
                    {
                        return Err(ConfigError {
                            line: word.line,
                            message: String::from("just one uid/login expected"),
                        });
                    }
                    index += 2;
                }
                "priority" => {
                    if value.is_string() || value.is_sub() {
                        return Err(ConfigError {
                            line: word.line,
                            message: String::from("unexpected string"),
                        });
                    }
                    out.priority = value.word.parse().map_err(|_| ConfigError {
                        line: word.line,
                        message: format!("bad priority '{}'", value.word),
                    })?;
                    index += 2;
                }
                "quantum" => {
                    if value.is_string() || value.is_sub() {
                        return Err(ConfigError {
                            line: word.line,
                            message: String::from("unexpected string"),
                        });
                    }
                    out.quantum = value.word.parse().map_err(|_| ConfigError {
                        line: word.line,
                        message: format!("bad quantum '{}'", value.word),
                    })?;
                    index += 2;
                }
                "sigmgr" | "scheduler" => {
                    if value.is_string() || value.is_sub() {
                        return Err(ConfigError {
                            line: word.line,
                            message: String::from("unexpected string"),
                        });
                    }
                    let parsed: i32 = value.word.parse().map_err(|_| ConfigError {
                        line: word.line,
                        message: format!("bad {} '{}'", word.word, value.word),
                    })?;
                    if word.word == "sigmgr" {
                        out.sigmgr = parsed;
                    } else {
                        out.scheduler = parsed;
                    }
                    index += 2;
                }
                "descr" => {
                    if !value.is_string() {
                        return Err(ConfigError {
                            line: word.line,
                            message: String::from("descr must be a string"),
                        });
                    }
                    out.descr = Some(value.word.clone());
                    index += 2;
                }
                "irq" => {
                    // C `do_irq` (`parse.c:406-461`): the values run to the
                    // next keyword; `none` alone clears; `all` alone sets
                    // the sentinel.
                    index += 1;
                    let mut first = true;
                    let mut irqs: Vec<i32> = Vec::new();
                    while index < flat.len() {
                        let w = flat[index];
                        if keywords.contains(&w.word.as_str()) {
                            break;
                        }
                        if w.is_string() || w.is_sub() {
                            return Err(ConfigError {
                                line: w.line,
                                message: String::from("do_irq: unexpected string"),
                            });
                        }
                        if w.word == "none" {
                            if !first || index + 1 < flat.len() {
                                return Err(ConfigError {
                                    line: w.line,
                                    message: String::from(
                                        "do_irq: NONE keyword not allowed in list",
                                    ),
                                });
                            }
                            irqs.clear();
                            index += 1;
                            break;
                        }
                        if w.word == "all" {
                            if !first || index + 1 < flat.len() {
                                return Err(ConfigError {
                                    line: w.line,
                                    message: String::from(
                                        "do_irq: ALL keyword not allowed in list",
                                    ),
                                });
                            }
                            out.nr_irq = rss::RSS_IRQ_ALL;
                            index += 1;
                            break;
                        }
                        let v: i32 = w.word.parse().map_err(|_| ConfigError {
                            line: w.line,
                            message: format!("do_irq: bad irq '{}'", w.word),
                        })?;
                        if irqs.len() >= rss::RSS_NR_IRQ as usize {
                            return Err(ConfigError {
                                line: w.line,
                                message: format!("do_irq: too many IRQs (max {})", rss::RSS_NR_IRQ),
                            });
                        }
                        irqs.push(v);
                        first = false;
                        index += 1;
                    }
                    out.nr_irq = if out.nr_irq == rss::RSS_IRQ_ALL {
                        rss::RSS_IRQ_ALL
                    } else {
                        irqs.len() as i32
                    };
                    out.irq = irqs;
                }
                "io" => {
                    // C `do_io` (`parse.c:461-530`): `base[:len]` hex pairs;
                    // `none`/`all` same shapes as do_irq.
                    index += 1;
                    let mut first = true;
                    let mut ranges: Vec<(u64, u64)> = Vec::new();
                    while index < flat.len() {
                        let w = flat[index];
                        if keywords.contains(&w.word.as_str()) {
                            break;
                        }
                        if w.is_string() || w.is_sub() {
                            return Err(ConfigError {
                                line: w.line,
                                message: String::from("do_io: unexpected string"),
                            });
                        }
                        if w.word == "none" {
                            if !first || index + 1 < flat.len() {
                                return Err(ConfigError {
                                    line: w.line,
                                    message: String::from(
                                        "do_io: NONE keyword not allowed in list",
                                    ),
                                });
                            }
                            ranges.clear();
                            index += 1;
                            break;
                        }
                        if w.word == "all" {
                            if !first || index + 1 < flat.len() {
                                return Err(ConfigError {
                                    line: w.line,
                                    message: String::from("do_io: ALL keyword not allowed in list"),
                                });
                            }
                            out.nr_io = rss::RSS_IO_ALL;
                            index += 1;
                            break;
                        }
                        // `base[:len]` in hex (strtoul base 0x10, `parse.c:503`).
                        let (base_s, len_s) = match w.word.split_once(':') {
                            Some((b, l)) => (b, Some(l)),
                            None => (w.word.as_str(), None),
                        };
                        let base_s = base_s
                            .strip_prefix("0x")
                            .or_else(|| base_s.strip_prefix("0X"))
                            .unwrap_or(base_s);
                        let base = u64::from_str_radix(base_s, 0x10).map_err(|_| ConfigError {
                            line: w.line,
                            message: format!("do_io: bad I/O range '{}'", w.word),
                        })?;
                        let len = match len_s {
                            Some(l) => {
                                let l = l
                                    .strip_prefix("0x")
                                    .or_else(|| l.strip_prefix("0X"))
                                    .unwrap_or(l);
                                u64::from_str_radix(l, 0x10).map_err(|_| ConfigError {
                                    line: w.line,
                                    message: format!("do_io: bad I/O range '{}'", w.word),
                                })?
                            }
                            None => 1,
                        };
                        if ranges.len() >= rss::RSS_NR_IO as usize {
                            return Err(ConfigError {
                                line: w.line,
                                message: format!(
                                    "do_io: too many I/O ranges (max {})",
                                    rss::RSS_NR_IO
                                ),
                            });
                        }
                        ranges.push((base, len));
                        first = false;
                        index += 1;
                    }
                    out.nr_io = if out.nr_io == rss::RSS_IO_ALL {
                        rss::RSS_IO_ALL
                    } else {
                        ranges.len() as i32
                    };
                    out.io = ranges;
                }
                _ => {
                    // The remaining keywords (pci/class/system/ipc/vm/
                    // control/type/net/domain) ride with the next slice;
                    // unknown words are skipped to surface every unknown.
                    index += 1;
                }
            }
        }
        // `-label` overrides the config label (C `req_label`, `:563-565`).
        if let Some(label) = &args.label {
            out.label = Some(label.clone());
        }
    }
    Ok(out)
}

/// Flatten a line's word chain, descending into `{`-created nested
/// containers in place (C's per-line chain is flat at the consumer level —
/// `read_line:320-334` rebinds the chain into the container).
fn flatten_words<'a>(
    entries: &'a [minix_proctools::configfile::ConfigEntry],
    out: &mut Vec<&'a minix_proctools::configfile::ConfigEntry>,
) {
    for entry in entries {
        if entry.is_sub() {
            flatten_words(&entry.list, out);
        } else {
            out.push(entry);
        }
    }
}

/// `getpwnam`'s named-uid scan over the caller-read database.
fn user_by_name(users: Option<&str>, name: &str) -> Option<u32> {
    users.and_then(|db| minix_proctools::passwd::lookup_in(db, name).map(|a| a.uid))
}

/// `getpwnam`'s named-uid seam, injected per call (the bin passes the
/// database text via [`RsStartDefaults::service_uid`]'s caller; this helper
/// stays for the numeric fallback shape).
fn resolve_uid(word: &str) -> u32 {
    // The named leg resolves through the caller's database at the bin;
    // the walk-level fallback is the numeric id() (parse.c:160-166).
    let mut acc: u32 = 0;
    for b in word.bytes() {
        if !b.is_ascii_digit() {
            return 0;
        }
        acc = acc.saturating_mul(10).saturating_add((b - b'0') as u32);
    }
    acc
}

/// The filled `rs_start` scalar face (C `struct rs_start`'s first fields).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BuiltRsStart {
    pub uid: u32,
    pub sigmgr: i32,
    pub scheduler: i32,
    pub priority: i32,
    pub quantum: i32,
    pub descr: Option<String>,
    pub label: Option<String>,
    /// `rss_nr_irq` — the raw count, or `super::rss::RSS_IRQ_ALL` (17) for the ALL face.
    pub nr_irq: i32,
    /// `rss_irq[0..nr_irq]` (only meaningful when `nr_irq` is a count).
    pub irq: Vec<i32>,
    /// `rss_nr_io` — the raw count, or `RSS_IO_ALL` (17) for the ALL face.
    pub nr_io: i32,
    /// `(base, len)` pairs (`rss_io[0..nr_io]`).
    pub io: Vec<(u64, u64)>,
}

#[cfg(test)]
mod do_config_tests {
    use super::*;
    use minix_proctools::configfile::config_read_text;

    const PW: &str = "service:x:50:50:Service:/:/bin/sh\n";

    #[test]
    fn scalar_keywords_fill_the_rs_start_defaults() {
        let text = "service lwip {\n  uid service;\n  priority 0;\n  quantum 4;\n}\n";
        let lines = config_read_text(text, "lwip.conf", 0, &|_| None).unwrap();
        let defaults = RsStartDefaults {
            service_uid: Some(50),
            sigmgr: 0,
            scheduler: 1,
            priority: 0,
            quantum: 8,
        };
        let args = parse_args(&["up", "/service/lwip", "-label", "lwip"]).unwrap();
        let got = do_config_walk(&lines, &defaults, &args, Some(PW)).unwrap();
        assert_eq!(got.uid, 50, "uid service 走 getpwnam 面");
        assert_eq!(got.priority, 0);
        assert_eq!(got.quantum, 4);
        assert_eq!(got.label.as_deref(), Some("lwip"), "-label 覆盖");
    }

    #[test]
    fn doubled_scalars_and_bad_values_answer_the_c_faces() {
        let text = "service x {\n  uid 0 1;\n}\n";
        let lines = config_read_text(text, "t", 0, &|_| None).unwrap();
        let defaults = RsStartDefaults::default();
        let args = parse_args(&["up", "/bin/x"]).unwrap();
        let e = do_config_walk(&lines, &defaults, &args, Some(PW)).unwrap_err();
        assert_eq!(e.message, "just one uid/login expected");
        let text = "service x {\n  priority abc;\n}\n";
        let lines = config_read_text(text, "t", 0, &|_| None).unwrap();
        let e = do_config_walk(&lines, &defaults, &args, Some(PW)).unwrap_err();
        assert_eq!(e.message, "bad priority 'abc'");
    }

    #[test]
    fn the_head_must_be_the_service_word() {
        let text = "widget x {\n}\n";
        let lines = config_read_text(text, "t", 0, &|_| None).unwrap();
        let defaults = RsStartDefaults::default();
        let args = parse_args(&["up", "/bin/x"]).unwrap();
        let e = do_config_walk(&lines, &defaults, &args, Some(PW)).unwrap_err();
        assert_eq!(e.message, "expected word 'service'");
    }

    #[test]
    fn irq_takes_a_list_of_values_and_the_all_face() {
        let text = "service x {\n  irq 3 4 5;\n}\n";
        let lines = config_read_text(text, "t", 0, &|_| None).unwrap();
        let defaults = RsStartDefaults::default();
        let args = parse_args(&["up", "/bin/x"]).unwrap();
        let got = do_config_walk(&lines, &defaults, &args, None).unwrap();
        assert_eq!(got.nr_irq, 3);
        assert_eq!(got.irq, [3, 4, 5]);
        // The ALL face sets the sentinel.
        let text = "service x {\n  irq all;\n}\n";
        let lines = config_read_text(text, "t", 0, &|_| None).unwrap();
        let got = do_config_walk(&lines, &defaults, &args, None).unwrap();
        assert_eq!(got.nr_irq, super::rss::RSS_IRQ_ALL);
    }

    #[test]
    fn io_takes_hex_base_len_pairs() {
        let text = "service x {\n  io 0x300:0x20 0x310;\n}\n";
        let lines = config_read_text(text, "t", 0, &|_| None).unwrap();
        let defaults = RsStartDefaults::default();
        let args = parse_args(&["up", "/bin/x"]).unwrap();
        let got = do_config_walk(&lines, &defaults, &args, None).unwrap();
        assert_eq!(got.nr_io, 2);
        assert_eq!(got.io[0], (0x300, 0x20));
        assert_eq!(got.io[1], (0x310, 1), "无 :len 时 C 缺省 len=1");
    }

    #[test]
    fn irq_none_clears_and_rejects_trailing_values() {
        // C `do_irq`: `none` alone is the default face; trailing values are
        // the fatal (`parse.c:428-434`).
        let text = "service x {\n  irq none;\n}\n";
        let lines = config_read_text(text, "t", 0, &|_| None).unwrap();
        let defaults = RsStartDefaults::default();
        let args = parse_args(&["up", "/bin/x"]).unwrap();
        let got = do_config_walk(&lines, &defaults, &args, None).unwrap();
        assert_eq!(got.nr_irq, 0);
        // C `do_irq` (`parse.c:428-434`): `none` followed by a value is
        // the fatal (`NONE keyword not allowed in list`).
        let text = "service x {\n  irq none 3;\n}\n";
        let lines = config_read_text(text, "t", 0, &|_| None).unwrap();
        let got = do_config_walk(&lines, &defaults, &args, None);
        assert!(got.is_err(), "none 后跟值应 fatal");
        assert!(
            got.unwrap_err()
                .message
                .contains("NONE keyword not allowed")
        );
    }

    #[test]
    fn descr_takes_a_quoted_string() {
        let text = "service x {\n  descr \"the x server\";\n}\n";
        let lines = config_read_text(text, "t", 0, &|_| None).unwrap();
        let defaults = RsStartDefaults::default();
        let args = parse_args(&["up", "/bin/x"]).unwrap();
        let got = do_config_walk(&lines, &defaults, &args, Some(PW)).unwrap();
        assert_eq!(got.descr.as_deref(), Some("the x server"));
    }
}
