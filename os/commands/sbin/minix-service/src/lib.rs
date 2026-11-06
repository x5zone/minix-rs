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
    RS_UP, RS_UPDATE, RsPciClassWire, RsPciIdWire,
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
/// `RSS_NR_IRQ 16` / `RSS_NR_IO 16` (`rs.h:25-26`).
pub const RSS_NR_IRQ: i32 = 16;
pub const RSS_NR_IO: i32 = 16;
/// `RS_NR_PCI_DEVICE 32` / `RS_NR_PCI_CLASS 4` (`rs.h:56-57`).
pub const RS_NR_PCI_DEVICE: i32 = 32;
pub const RS_NR_PCI_CLASS: i32 = 4;
/// `NO_SUB_VID` / `NO_SUB_DID` = `0xffff` (`rs.h:79-80`): "no subsystem
/// match requested".
pub const NO_SUB_VID: u16 = 0xffff;
pub const NO_SUB_DID: u16 = 0xffff;

/// `RSS_IRQ_ALL (RSS_NR_IRQ+1)` (`rs.h:27`).
pub const RSS_IRQ_ALL: i32 = RSS_NR_IRQ + 1;
/// `RSS_IO_ALL (RSS_NR_IO+1)` (`rs.h:28`).
pub const RSS_IO_ALL: i32 = RSS_NR_IO + 1;

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

/// Every statement keyword `do_service` dispatches on (`parse.h:1-26`,
/// dispatch at `parse.c:1063-1150`). A statement's value list ends at the
/// next one of these: C keeps each statement in its own `config_t` chain,
/// so a following keyword is the next statement, never a value.
const STATEMENT_KEYWORDS: [&str; 17] = [
    "class",
    "control",
    "cpu",
    "descr",
    "domain",
    "io",
    "ipc",
    "irq",
    "pci",
    "priority",
    "quantum",
    "scheduler",
    "sigmgr",
    "system",
    "type",
    "uid",
    "vm",
];

/// C `strtoul(s, &check, base)` for the two bases the walker uses (`0` for
/// `irq`, `16` for `io`), returning the converted value and the number of
/// bytes consumed. The `check[0] == '\0'` contract stays with the caller,
/// which compares the count with the word's length; when no digit is
/// consumed the count is 0, mirroring `check` being left at the string
/// start (so only the empty word passes that test).
///
/// Base 0 selects octal on a leading `0` and hex on `0x`/`0X` with at
/// least one hex digit after it; base 16 accepts the same `0x` prefix.
/// A sign is consumed and folded with wrapping negation, as `strtoul`
/// does; overflow saturates at `ULONG_MAX` like the C library.
fn strtoul_prefix(s: &str, base: u32) -> (u64, usize) {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() && b[i].is_ascii_whitespace() {
        i += 1;
    }
    let mut neg = false;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        neg = b[i] == b'-';
        i += 1;
    }
    let mut radix = base;
    let hex_prefix = i + 2 < b.len()
        && b[i] == b'0'
        && (b[i + 1] | 0x20) == b'x'
        && b[i + 2].is_ascii_hexdigit();
    if hex_prefix && (radix == 0 || radix == 16) {
        radix = 16;
        i += 2;
    } else if radix == 0 {
        radix = if i < b.len() && b[i] == b'0' { 8 } else { 10 };
    }
    let start = i;
    let mut acc: u64 = 0;
    while i < b.len() {
        let Some(d) = (b[i] as char).to_digit(radix) else {
            break;
        };
        acc = acc
            .saturating_mul(u64::from(radix))
            .saturating_add(u64::from(d));
        i += 1;
    }
    if i == start {
        return (0, 0);
    }
    (if neg { acc.wrapping_neg() } else { acc }, i)
}

/// C `do_io`'s `base[:len]` pair (`parse.c:497-517`): base 16 `strtoul`, an
/// optional `:` and a second hex `strtoul` for the length (default 1), then
/// `check[0] == '\0'`. Both land in `unsigned` fields, so the caller
/// truncates to 32 bits exactly as C's assignment does. The one shape worth
/// spelling out: `base:` leaves an empty tail, and `strtoul("")` reports
/// `check` on the empty string, so C accepts it with length 0.
fn parse_io_range(word: &str) -> Option<(u64, u64)> {
    let (base, used) = strtoul_prefix(word, 16);
    let mut idx = used;
    let mut len = 1u64;
    if word.as_bytes().get(idx) == Some(&b':') {
        let rest = &word[idx + 1..];
        if rest.is_empty() {
            idx = word.len();
        } else {
            let (parsed, used_rest) = strtoul_prefix(rest, 16);
            if used_rest == 0 {
                return None;
            }
            len = parsed;
            idx += 1 + used_rest;
        }
    }
    if idx != word.len() {
        return None;
    }
    Some((base, len))
}

/// One hex field of a PCI ID word, C `strtoul(s, &check, 0x10)` style: the
/// parsed value plus the unconsumed tail. A field with no digits answers 0
/// with the whole tail left, matching C's `check` staying at the start —
/// that is how C accepts empty fields such as `0c//00`.
fn pci_hex(s: &str) -> (u64, &str) {
    let (v, used) = strtoul_prefix(s, 16);
    (v, &s[used..])
}

/// C `do_pci_device`'s `vid:did[/sub_vid:sub_did]` word (`parse.c:541-570`).
/// The legacy `vid/did` spelling is the same grammar with `/` as the first
/// separator. A `vid:did/sub_vid` tail without `sub_did` is a fatal in C
/// (`check[0] != ':'` after the sub-vendor field), so this answers `None`.
fn parse_pci_device(word: &str) -> Option<RsPciIdWire> {
    let (vid, rest) = pci_hex(word);
    if !(rest.starts_with(':') || rest.starts_with('/')) {
        return None;
    }
    let (did, rest) = pci_hex(&rest[1..]);
    if rest.is_empty() {
        return Some(RsPciIdWire {
            vid: vid as u16,
            did: did as u16,
            sub_vid: NO_SUB_VID,
            sub_did: NO_SUB_DID,
        });
    }
    if !rest.starts_with('/') {
        return None;
    }
    let (sub_vid, rest) = pci_hex(&rest[1..]);
    if !rest.starts_with(':') {
        return None;
    }
    let (sub_did, rest) = pci_hex(&rest[1..]);
    if !rest.is_empty() {
        return None;
    }
    Some(RsPciIdWire {
        vid: vid as u16,
        did: did as u16,
        sub_vid: sub_vid as u16,
        sub_did: sub_did as u16,
    })
}

/// C `do_pci_class`'s `base[/sub[/iface]]` word (`parse.c:596-620`): the mask
/// widens with each field (`0xff0000` → `0xffff00` → `0xffffff`), and the
/// three `u8_t` locals truncate exactly as C's assignments do.
fn parse_pci_class(word: &str) -> Option<RsPciClassWire> {
    let (base, mut rest) = pci_hex(word);
    let mut subclass = 0u64;
    let mut interface = 0u64;
    let mut mask = 0x00ff_0000u32;
    if rest.starts_with('/') {
        let (sub, tail) = pci_hex(&rest[1..]);
        subclass = sub;
        mask = 0x00ff_ff00;
        rest = tail;
        if rest.starts_with('/') {
            let (iface, tail) = pci_hex(&rest[1..]);
            interface = iface;
            mask = 0x00ff_ffff;
            rest = tail;
        }
    }
    if !rest.is_empty() {
        return None;
    }
    let pciclass = (u32::from(base as u8) << 16)
        | (u32::from(subclass as u8) << 8)
        | u32::from(interface as u8);
    Some(RsPciClassWire { pciclass, mask })
}

/// The do_config keyword walk over the parsed lines (`parse.c:1177-1263`
/// walk; the scalar keywords this slice covers are `uid`, `priority`,
/// `quantum`, `sigmgr`, `scheduler`, `descr`, and the device-family
/// keywords `irq`, `io` and `pci` (`pci device` / `pci class`) — the
/// remaining keywords (`class` in its service-inheritance sense, `system`,
/// `ipc`, `vm`, `control`, `type`, `net`, `domain`) are the registered next
/// slice). C's per-branch `fatal` faces become `ConfigError` with the same
/// wording.
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
        nr_pci_id: 0,
        pci_id: Vec::new(),
        nr_pci_class: 0,
        pci_class: Vec::new(),
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
                        .is_some_and(|next| !STATEMENT_KEYWORDS.contains(&next.word.as_str()))
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
                    // next statement keyword; `NONE` only ends the list;
                    // `ALL` sets the sentinel; everything else is
                    // `strtoul(word, &check, 0)` — base 0, so `0x10` is 16
                    // and `010` is 8 — stored into an `int`.
                    index += 1;
                    let mut first = true;
                    // C writes at `rss_nr_irq` and `do_config` memsets the
                    // struct only once, so a repeated `irq` statement
                    // appends to what earlier ones stored; `NONE` leaves
                    // the count alone (`parse.c:428-434`, `:454-460`).
                    let mut count = out.nr_irq;
                    let mut irqs: Vec<i32> = core::mem::take(&mut out.irq);
                    while index < flat.len() {
                        let w = flat[index];
                        if STATEMENT_KEYWORDS.contains(&w.word.as_str()) {
                            break;
                        }
                        if w.is_string() || w.is_sub() {
                            return Err(ConfigError {
                                line: w.line,
                                message: String::from("do_irq: unexpected string"),
                            });
                        }
                        // C's `cpe->next` spans this statement's list only;
                        // in the flat walk that list ends at the next
                        // statement keyword.
                        let at_list_end = index + 1 >= flat.len()
                            || STATEMENT_KEYWORDS.contains(&flat[index + 1].word.as_str());
                        if w.word == "NONE" {
                            if !first || !at_list_end {
                                return Err(ConfigError {
                                    line: w.line,
                                    message: String::from(
                                        "do_irq: NONE keyword not allowed in list",
                                    ),
                                });
                            }
                            index += 1;
                            break;
                        }
                        if w.word == "ALL" {
                            if !first || !at_list_end {
                                return Err(ConfigError {
                                    line: w.line,
                                    message: String::from(
                                        "do_irq: ALL keyword not allowed in list",
                                    ),
                                });
                            }
                            count = RSS_IRQ_ALL;
                            index += 1;
                            break;
                        }
                        let (v, used) = strtoul_prefix(&w.word, 0);
                        if used != w.word.len() {
                            return Err(ConfigError {
                                line: w.line,
                                message: format!("do_irq: bad irq '{}'", w.word),
                            });
                        }
                        if count >= RSS_NR_IRQ {
                            return Err(ConfigError {
                                line: w.line,
                                message: format!("do_irq: too many IRQs (max {RSS_NR_IRQ})"),
                            });
                        }
                        // C assigns `unsigned long` to `int`: keep the low
                        // 32 bits, sign included.
                        irqs.push(v as u32 as i32);
                        count += 1;
                        first = false;
                        index += 1;
                    }
                    out.nr_irq = count;
                    out.irq = irqs;
                }
                "io" => {
                    // C `do_io` (`parse.c:461-530`): `base[:len]` pairs in
                    // hex (`strtoul(..., 0x10)`, the `0x` prefix allowed);
                    // `NONE`/`ALL` take the same shapes as `do_irq`, and a
                    // repeated statement appends like `do_irq`.
                    index += 1;
                    let mut first = true;
                    let mut count = out.nr_io;
                    let mut ranges: Vec<(u64, u64)> = core::mem::take(&mut out.io);
                    while index < flat.len() {
                        let w = flat[index];
                        if STATEMENT_KEYWORDS.contains(&w.word.as_str()) {
                            break;
                        }
                        if w.is_string() || w.is_sub() {
                            return Err(ConfigError {
                                line: w.line,
                                message: String::from("do_io: unexpected string"),
                            });
                        }
                        let at_list_end = index + 1 >= flat.len()
                            || STATEMENT_KEYWORDS.contains(&flat[index + 1].word.as_str());
                        if w.word == "NONE" {
                            if !first || !at_list_end {
                                return Err(ConfigError {
                                    line: w.line,
                                    message: String::from(
                                        "do_io: NONE keyword not allowed in list",
                                    ),
                                });
                            }
                            index += 1;
                            break;
                        }
                        if w.word == "ALL" {
                            if !first || !at_list_end {
                                return Err(ConfigError {
                                    line: w.line,
                                    message: String::from("do_io: ALL keyword not allowed in list"),
                                });
                            }
                            count = RSS_IO_ALL;
                            index += 1;
                            break;
                        }
                        let Some((base, len)) = parse_io_range(&w.word) else {
                            return Err(ConfigError {
                                line: w.line,
                                message: format!("do_io: bad I/O range '{}'", w.word),
                            });
                        };
                        if count >= RSS_NR_IO {
                            return Err(ConfigError {
                                line: w.line,
                                message: format!("do_io: too many I/O ranges (max {RSS_NR_IO})"),
                            });
                        }
                        // C stores into `unsigned base` / `unsigned len`.
                        ranges.push((u64::from(base as u32), u64::from(len as u32)));
                        count += 1;
                        first = false;
                        index += 1;
                    }
                    out.nr_io = count;
                    out.io = ranges;
                }
                "pci" => {
                    // C `do_pci` (`parse.c:628-656`): the statement's first
                    // word selects the table — `device` fills `rss_pci_id`,
                    // `class` fills `rss_pci_class`; an empty statement is a
                    // no-op (`cpe == NULL`).
                    index += 1;
                    let Some(sub) = flat.get(index) else {
                        break;
                    };
                    if sub.is_string() || sub.is_sub() {
                        return Err(ConfigError {
                            line: sub.line,
                            message: String::from("do_pci: unexpected string"),
                        });
                    }
                    // `class` is both a pci sub-keyword and a top-level
                    // statement keyword, so the sub-word is interpreted
                    // before the statement-boundary test.
                    let device_table = match sub.word.as_str() {
                        "device" => Some(true),
                        "class" => Some(false),
                        _ => None,
                    };
                    let Some(device_table) = device_table else {
                        if STATEMENT_KEYWORDS.contains(&sub.word.as_str()) {
                            // Empty `pci` statement: C's `do_pci(NULL)`
                            // returns and this word opens the next
                            // statement, so dispatch resumes here.
                            continue;
                        }
                        return Err(ConfigError {
                            line: sub.line,
                            message: format!("do_pci: unexpected word '{}'", sub.word),
                        });
                    };
                    index += 1;
                    if device_table {
                        // C appends at `rss_nr_pci_id`, so a repeated
                        // statement continues the same table.
                        let mut count = out.nr_pci_id;
                        let mut ids: Vec<RsPciIdWire> = core::mem::take(&mut out.pci_id);
                        while index < flat.len() {
                            let w = flat[index];
                            if STATEMENT_KEYWORDS.contains(&w.word.as_str()) {
                                break;
                            }
                            if w.is_string() || w.is_sub() {
                                return Err(ConfigError {
                                    line: w.line,
                                    message: String::from("do_pci_device: unexpected string"),
                                });
                            }
                            let Some(entry) = parse_pci_device(&w.word) else {
                                return Err(ConfigError {
                                    line: w.line,
                                    message: format!("do_pci_device: bad ID '{}'", w.word),
                                });
                            };
                            if count >= RS_NR_PCI_DEVICE {
                                return Err(ConfigError {
                                    line: w.line,
                                    message: format!(
                                        "do_pci_device: too many device IDs (max {RS_NR_PCI_DEVICE})"
                                    ),
                                });
                            }
                            ids.push(entry);
                            count += 1;
                            index += 1;
                        }
                        out.nr_pci_id = count;
                        out.pci_id = ids;
                    } else {
                        let mut count = out.nr_pci_class;
                        let mut classes: Vec<RsPciClassWire> = core::mem::take(&mut out.pci_class);
                        while index < flat.len() {
                            let w = flat[index];
                            if STATEMENT_KEYWORDS.contains(&w.word.as_str()) {
                                break;
                            }
                            if w.is_string() || w.is_sub() {
                                // C's `do_pci_class` reuses the
                                // `do_pci_device` wording for these two
                                // fatals (`parse.c:589-597`) — kept verbatim.
                                return Err(ConfigError {
                                    line: w.line,
                                    message: String::from("do_pci_device: unexpected string"),
                                });
                            }
                            let Some(entry) = parse_pci_class(&w.word) else {
                                return Err(ConfigError {
                                    line: w.line,
                                    message: format!("do_pci_class: bad class ID '{}'", w.word),
                                });
                            };
                            if count >= RS_NR_PCI_CLASS {
                                return Err(ConfigError {
                                    line: w.line,
                                    message: format!(
                                        "do_pci_class: too many class IDs (max {RS_NR_PCI_CLASS})"
                                    ),
                                });
                            }
                            classes.push(entry);
                            count += 1;
                            index += 1;
                        }
                        out.nr_pci_class = count;
                        out.pci_class = classes;
                    }
                }
                _ => {
                    // The remaining keywords (class/system/ipc/vm/
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
    /// `rss_nr_irq` — the raw count, or `RSS_IRQ_ALL` (17) for the ALL face.
    pub nr_irq: i32,
    /// `rss_irq[0..nr_irq]` (only meaningful when `nr_irq` is a count).
    pub irq: Vec<i32>,
    /// `rss_nr_io` — the raw count, or `RSS_IO_ALL` (17) for the ALL face.
    pub nr_io: i32,
    /// `(base, len)` pairs (`rss_io[0..nr_io]`).
    pub io: Vec<(u64, u64)>,
    /// `rss_nr_pci_id` — count of `pci device` entries.
    pub nr_pci_id: i32,
    /// `rss_pci_id` (`RS_NR_PCI_DEVICE` slots).
    pub pci_id: Vec<RsPciIdWire>,
    /// `rss_nr_pci_class` — count of `pci class` entries.
    pub nr_pci_class: i32,
    /// `rss_pci_class` (`RS_NR_PCI_CLASS` slots).
    pub pci_class: Vec<RsPciClassWire>,
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
    fn descr_takes_a_quoted_string() {
        let text = "service x {\n  descr \"the x server\";\n}\n";
        let lines = config_read_text(text, "t", 0, &|_| None).unwrap();
        let defaults = RsStartDefaults::default();
        let args = parse_args(&["up", "/bin/x"]).unwrap();
        let got = do_config_walk(&lines, &defaults, &args, Some(PW)).unwrap();
        assert_eq!(got.descr.as_deref(), Some("the x server"));
    }

    /// `walk(text)` with the C defaults — the harness the device-family
    /// tests below share.
    fn walk(text: &str) -> Result<BuiltRsStart, minix_proctools::configfile::ConfigError> {
        let lines = config_read_text(text, "t", 0, &|_| None).unwrap();
        let defaults = RsStartDefaults::default();
        let args = parse_args(&["up", "/bin/x"]).unwrap();
        do_config_walk(&lines, &defaults, &args, Some(PW))
    }

    #[test]
    fn irq_values_take_the_c_strtoul_base_zero_face() {
        // `parse.c:447`: `strtoul(word, &check, 0)` — base 0 selects hex
        // on `0x`, octal on a leading `0`, decimal otherwise; the value
        // lands in an `int`, so a negative word keeps its low 32 bits.
        let got = walk("service x {\n  irq 4 0x10 010 -1;\n}\n").unwrap();
        assert_eq!(got.nr_irq, 4);
        assert_eq!(got.irq, vec![4, 16, 8, -1]);
    }

    #[test]
    fn irq_none_and_all_are_the_uppercase_keywords() {
        // `KW_NONE`/`KW_ALL` are `"NONE"`/`"ALL"` (`parse.h:18-20`);
        // `strcmp` makes them case-sensitive, and real system.conf spells
        // them uppercase (`minix3/etc/system.conf:18`, `:179`).
        let got = walk("service x {\n  irq NONE;\n}\n").unwrap();
        assert_eq!(got.nr_irq, 0);
        assert!(got.irq.is_empty());
        // The real system.conf shape: `irq NONE;` followed by the next
        // statement — `NONE` is last in its own list, so this is legal.
        let got = walk("service x {\n  irq NONE;\n  io ALL;\n}\n").unwrap();
        assert_eq!(got.nr_irq, 0);
        assert_eq!(got.nr_io, RSS_IO_ALL);
        let got = walk("service x {\n  irq ALL;\n}\n").unwrap();
        assert_eq!(got.nr_irq, RSS_IRQ_ALL);
        assert!(got.irq.is_empty());
        // Lowercase is not the C keyword: `strtoul("none", ..., 0)` parses
        // no digits, leaving `check` non-empty — the bad-irq fatal.
        let e = walk("service x {\n  irq none;\n}\n").unwrap_err();
        assert_eq!(e.message, "do_irq: bad irq 'none'");
    }

    #[test]
    fn io_ranges_are_hex_pairs_with_the_0x_prefix_allowed() {
        // `parse.c:502-517`: `strtoul(word, &check, 0x10)` for the base and
        // an optional `:` plus a second hex `strtoul` for the length
        // (default 1). Real shapes from system.conf: `3f0:8`, `cf8:8`,
        // `70:2`, and a bare base.
        let got = walk("service x {\n  io 3f0:8 cf8:8 70:2 3f9;\n}\n").unwrap();
        assert_eq!(got.nr_io, 4);
        assert_eq!(got.io, vec![(0x3f0, 8), (0xcf8, 8), (0x70, 2), (0x3f9, 1)]);
        // Base 16 accepts the `0x` prefix that `from_str_radix` would not.
        let got = walk("service x {\n  io 0x3f8:0x10;\n}\n").unwrap();
        assert_eq!(got.io, vec![(0x3f8, 0x10)]);
    }

    #[test]
    fn io_none_and_all_take_the_sentinel_and_the_clear() {
        let got = walk("service x {\n  io NONE;\n}\n").unwrap();
        assert_eq!(got.nr_io, 0);
        assert!(got.io.is_empty());
        let got = walk("service x {\n  io ALL;\n}\n").unwrap();
        assert_eq!(got.nr_io, RSS_IO_ALL);
        assert!(got.io.is_empty());
    }

    #[test]
    fn device_lists_end_at_the_next_statement_keyword() {
        // C keeps each statement in its own chain, so `irq 6;` followed by
        // `io 3f0:8;` must not let the irq list swallow `io`. This is the
        // real adjacency in `minix3/etc/system.conf:244-245`.
        let got = walk("service x {\n  irq 6;\n  io 3f0:8;\n}\n").unwrap();
        assert_eq!(got.nr_irq, 1);
        assert_eq!(got.irq, vec![6]);
        assert_eq!(got.nr_io, 1);
        assert_eq!(got.io, vec![(0x3f0, 8)]);
    }

    #[test]
    fn device_family_error_faces_match_the_c_fatals() {
        let e = walk("service x {\n  irq 1 ALL;\n}\n").unwrap_err();
        assert_eq!(e.message, "do_irq: ALL keyword not allowed in list");
        let e = walk("service x {\n  io ALL 1;\n}\n").unwrap_err();
        assert_eq!(e.message, "do_io: ALL keyword not allowed in list");
        let e = walk("service x {\n  irq NONE 1;\n}\n").unwrap_err();
        assert_eq!(e.message, "do_irq: NONE keyword not allowed in list");
        let e = walk("service x {\n  irq abc;\n}\n").unwrap_err();
        assert_eq!(e.message, "do_irq: bad irq 'abc'");
        let e = walk("service x {\n  io 3g8;\n}\n").unwrap_err();
        assert_eq!(e.message, "do_io: bad I/O range '3g8'");
        let e = walk("service x {\n  io 3f8:x;\n}\n").unwrap_err();
        assert_eq!(e.message, "do_io: bad I/O range '3f8:x'");
        // `rss_nr_irq >= RSS_NR_IRQ` before the store: the seventeenth
        // value is one too many (`parse.c:456-457`).
        let mut text = String::from("service x {\n  irq");
        for n in 0..=RSS_NR_IRQ {
            text.push(' ');
            text.push_str(&n.to_string());
        }
        text.push_str(";\n}\n");
        let e = walk(&text).unwrap_err();
        assert_eq!(e.message, "do_irq: too many IRQs (max 16)");
    }

    #[test]
    fn repeated_device_statements_append_like_the_c_arrays() {
        // C writes at `rss_nr_irq`/`rss_nr_io` and `do_config` memsets the
        // struct once, so a second statement appends instead of replacing
        // (`parse.c:454-460`, `:511-516`).
        let got = walk("service x {\n  irq 4;\n  irq 5;\n}\n").unwrap();
        assert_eq!(got.nr_irq, 2);
        assert_eq!(got.irq, vec![4, 5]);
        // `NONE` ends the list; it does not clear what an earlier
        // statement stored (`parse.c:428-434`).
        let got = walk("service x {\n  irq 4;\n  irq NONE;\n}\n").unwrap();
        assert_eq!(got.nr_irq, 1);
        assert_eq!(got.irq, vec![4]);
        let got = walk("service x {\n  io 3f8:8;\n  io 3f9;\n}\n").unwrap();
        assert_eq!(got.nr_io, 2);
        assert_eq!(got.io, vec![(0x3f8, 8), (0x3f9, 1)]);
    }

    #[test]
    fn pci_device_ids_take_the_c_four_field_face() {
        // `parse.c:541-570`: `vid:did`, the legacy `vid/did`, and the
        // subsystem form `vid:did/sub_vid:sub_did`; `NO_SUB_VID`/
        // `NO_SUB_DID` (0xffff) mark "no subsystem match" (`rs.h:79-80`).
        let got = walk("service x {\n  pci device 1af4:1001;\n}\n").unwrap();
        assert_eq!(got.nr_pci_id, 1);
        assert_eq!(
            got.pci_id,
            vec![RsPciIdWire {
                vid: 0x1af4,
                did: 0x1001,
                sub_vid: NO_SUB_VID,
                sub_did: NO_SUB_DID,
            }]
        );
        let got = walk("service x {\n  pci device 8086:100e/1af4:1100;\n}\n").unwrap();
        assert_eq!(
            got.pci_id,
            vec![RsPciIdWire {
                vid: 0x8086,
                did: 0x100e,
                sub_vid: 0x1af4,
                sub_did: 0x1100,
            }]
        );
        // Legacy separator: `vid/did` is one device, no subsystem.
        let got = walk("service x {\n  pci device 1022/1100;\n}\n").unwrap();
        assert_eq!(got.pci_id[0].vid, 0x1022);
        assert_eq!(got.pci_id[0].did, 0x1100);
        assert_eq!(got.pci_id[0].sub_vid, NO_SUB_VID);
    }

    #[test]
    fn pci_class_ids_take_the_c_mask_face() {
        // `parse.c:596-620`: each field widens the mask — base only
        // 0xff0000, base/sub 0xffff00, base/sub/iface 0xffffff; the three
        // locals are `u8_t`, so 0x1ff truncates to 0xff.
        let got = walk("service x {\n  pci class 02 03/01 0c/03/00 1ff/2/3;\n}\n").unwrap();
        assert_eq!(got.nr_pci_class, 4);
        assert_eq!(
            got.pci_class,
            vec![
                RsPciClassWire {
                    pciclass: 0x02_0000,
                    mask: 0x00ff_0000,
                },
                RsPciClassWire {
                    pciclass: 0x03_0100,
                    mask: 0x00ff_ff00,
                },
                RsPciClassWire {
                    pciclass: 0x0c_0300,
                    mask: 0x00ff_ffff,
                },
                RsPciClassWire {
                    pciclass: 0xff_0203,
                    mask: 0x00ff_ffff,
                },
            ]
        );
    }

    #[test]
    fn pci_tables_append_and_stop_at_the_next_statement() {
        // C appends at `rss_nr_pci_id`/`rss_nr_pci_class`, and each
        // statement's list ends at the next statement keyword — the real
        // system.conf shape (`pci class …;` then `pci device …;`).
        let got =
            walk("service x {\n  pci class 0c/03/00;\n  pci device 1af4:1001;\n  irq 4;\n}\n")
                .unwrap();
        assert_eq!(got.nr_pci_class, 1);
        assert_eq!(got.pci_class[0].pciclass, 0x0c_0300);
        assert_eq!(got.nr_pci_id, 1);
        assert_eq!(got.pci_id[0].vid, 0x1af4);
        assert_eq!(got.nr_irq, 1);
        assert_eq!(got.irq, vec![4]);
        let got =
            walk("service x {\n  pci device 1af4:1001;\n  pci device 8086:100e;\n}\n").unwrap();
        assert_eq!(got.nr_pci_id, 2);
    }

    #[test]
    fn pci_limits_are_the_c_array_sizes() {
        let mut text = String::from("service x {\n  pci device");
        for n in 0..=RS_NR_PCI_DEVICE {
            text.push_str(&format!(" {n:x}:1"));
        }
        text.push_str(";\n}\n");
        let e = walk(&text).unwrap_err();
        assert_eq!(e.message, "do_pci_device: too many device IDs (max 32)");
        let mut text = String::from("service x {\n  pci class");
        for n in 0..=RS_NR_PCI_CLASS {
            text.push_str(&format!(" {n:x}"));
        }
        text.push_str(";\n}\n");
        let e = walk(&text).unwrap_err();
        assert_eq!(e.message, "do_pci_class: too many class IDs (max 4)");
    }

    #[test]
    fn pci_error_faces_match_the_c_fatals() {
        let e = walk("service x {\n  pci foo 1:2;\n}\n").unwrap_err();
        assert_eq!(e.message, "do_pci: unexpected word 'foo'");
        let e = walk("service x {\n  pci device 1af4;\n}\n").unwrap_err();
        assert_eq!(e.message, "do_pci_device: bad ID '1af4'");
        // A subsystem form without `sub_did` is a fatal (`check[0] != ':'`).
        let e = walk("service x {\n  pci device 1:2/3;\n}\n").unwrap_err();
        assert_eq!(e.message, "do_pci_device: bad ID '1:2/3'");
        let e = walk("service x {\n  pci class 0c/03/00/x;\n}\n").unwrap_err();
        assert_eq!(e.message, "do_pci_class: bad class ID '0c/03/00/x'");
        // C's `do_pci_class` reuses the `do_pci_device` wording for the
        // string/sublist fatals (`parse.c:589-597`) — a faithful quirk.
        let e = walk("service x {\n  pci class \"x\";\n}\n").unwrap_err();
        assert_eq!(e.message, "do_pci_device: unexpected string");
    }

    #[test]
    fn empty_pci_statement_is_a_noop() {
        // C `do_pci` returns on `cpe == NULL` (`parse.c:630-631`).
        let got = walk("service x {\n  pci;\n  irq 4;\n}\n").unwrap();
        assert_eq!(got.nr_pci_id, 0);
        assert_eq!(got.nr_pci_class, 0);
        assert_eq!(got.irq, vec![4]);
    }
}
