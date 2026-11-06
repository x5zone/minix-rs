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
    RS_UP, RS_UPDATE, RsIoRangeWire, RsLabelWire, RsPciClassWire, RsPciIdWire, RsStartWire,
    RsStateDataWire, encode_rs_start, rs_start_off,
};
use minix_sys::syscall::{
    KERNEL_CALL, NR_SYS_CALLS, SYS_ABORT, SYS_DEVIO, SYS_IOPENABLE, SYS_IRQCTL, SYS_KILL,
    SYS_MEMSET, SYS_PADCONF, SYS_PHYSCOPY, SYS_PRIVCTL, SYS_READBIOS, SYS_SDEVIO, SYS_STIME,
    SYS_TRACE, SYS_UMAP, SYS_UMAP_REMOTE, SYS_VDEVIO, SYS_VIRCOPY, SYS_VMCTL, SYS_VUMAP,
};
use minix_sys::vm::{
    NR_VM_CALLS, VM_ADDDMA, VM_CLEARCACHE, VM_DELDMA, VM_EXEC_NEWMEM, VM_EXIT, VM_FORGETCACHEPAGE,
    VM_FORK, VM_GETDMA, VM_GETPHYS, VM_GETREF, VM_GETRUSAGE, VM_INFO, VM_MAPCACHEPAGE, VM_PROCCTL,
    VM_REMAP, VM_REMAP_RO, VM_RQ_BASE, VM_RS_MEMCTL, VM_RS_PREPARE, VM_RS_SET_PRIV, VM_RS_UPDATE,
    VM_SETCACHEPAGE, VM_SHM_UNMAP, VM_VFS_MMAP, VM_VFS_REPLY, VM_WILLEXIT,
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
/// C `DSRV_SM`/`DSRV_SCH`/`DSRV_Q`/`DSRV_QT` (`priv.h:83-99`): the clean
/// defaults `do_config` installs — `ROOT_SYS_PROC_NR` (= `RS_PROC_NR`, 2),
/// `SCHED_PROC_NR` (4), `USER_Q` ((`NR_SCHED_QUEUES`-1)/2 = 7), and
/// `USER_QUANTUM` (200) (`config.h:66-74`, `com.h:63,77`).
pub const DSRV_SM: i32 = 2;
pub const DSRV_SCH: i32 = 4;
pub const DSRV_Q: i32 = 7;
pub const DSRV_QT: i32 = 200;

/// C `strrchr(req_path, '/') + 1` (`minix-service.c:743-752`): the program
/// name is the path's basename.
pub fn progname_of(path: &str) -> &str {
    match path.rfind('/') {
        Some(i) => &path[i + 1..],
        None => path,
    }
}

/// Appends one string to the image and answers its absolute caller-space
/// address and length.
fn place_string(bytes: &mut Vec<u8>, base: u64, text: &str) -> (u64, u64) {
    let start = bytes.len();
    bytes.extend_from_slice(text.as_bytes());
    (base + start as u64, text.len() as u64)
}

/// A complete `up` request image: the `rs_start` struct followed by every
/// string it points at, in one caller-space buffer. `decode_rs_start` reads
/// the leading struct back; RS copies the whole image out of the caller's
/// space with `sys_datacopy`.
pub struct RequestImage {
    pub bytes: Vec<u8>,
}

impl RequestImage {
    /// The struct's address — the value `m_rs_req.addr` carries.
    pub fn address(&self) -> u64 {
        self.bytes.as_ptr() as u64
    }
}

/// C `main`'s `RS_UP`/`RS_UPDATE`/`RS_EDIT` assembly
/// (`minix-service.c:746-816`): run the config walk for `block_label`, apply
/// the post-walk defaults, then pack the struct and its strings into one
/// image whose internal pointers are absolute.
pub fn build_up_request(
    lines: &[minix_proctools::configfile::ConfigEntry],
    block_label: &str,
    defaults: &RsStartDefaults,
    args: &Args,
    users: Option<&str>,
) -> Result<RequestImage, minix_proctools::configfile::ConfigError> {
    use minix_proctools::configfile::ConfigError;
    let mut built = do_config_walk(lines, Some(block_label), defaults, args, users)?;
    // `parse.c:1240-1246`: with no `ipc` statement the request carries
    // `RSS_IPC_ALL_SYS`.
    let ipc = built
        .ipc
        .take()
        .unwrap_or_else(|| String::from(RSS_IPC_ALL_SYS));
    let path = args.path.as_deref().unwrap_or("");
    // `:746-753`: `command` is `path + " " + args`; `progname` is
    // `-progname` or the path's basename.
    let cmd = format!("{path} {}", args.args.as_deref().unwrap_or(""));
    let progname = args
        .progname
        .clone()
        .unwrap_or_else(|| String::from(progname_of(path)));
    // `:776-787`: `rss_label` defaults to the progname; the target label
    // stays empty unless given.
    let label = built.label.clone().unwrap_or_else(|| progname.clone());
    let trg_label = args.trg_label.clone().unwrap_or_default();
    let script = args.script.clone().unwrap_or_default();
    let control = built.control.clone();

    let strings = cmd.len()
        + script.len()
        + label.len()
        + trg_label.len()
        + ipc.len()
        + progname.len()
        + control.iter().map(String::len).sum::<usize>();
    let mut bytes: Vec<u8> = Vec::new();
    if bytes
        .try_reserve_exact(rs_start_off::SIZE + strings)
        .is_err()
    {
        return Err(ConfigError {
            line: 0,
            message: String::from("no memory for the request image"),
        });
    }
    bytes.resize(rs_start_off::SIZE, 0);
    // The capacity above guarantees the buffer never moves, so the base
    // taken here stays valid while the strings are appended.
    let base = bytes.as_ptr() as u64;
    let (cmd_addr, cmd_len) = place_string(&mut bytes, base, &cmd);
    let (script_addr, script_len) = place_string(&mut bytes, base, &script);
    let (label_addr, label_len) = place_string(&mut bytes, base, &label);
    let (trg_addr, trg_len) = place_string(&mut bytes, base, &trg_label);
    let (ipc_addr, ipc_len) = place_string(&mut bytes, base, &ipc);
    let (prog_addr, prog_len) = place_string(&mut bytes, base, &progname);
    let mut control_labels = [RsLabelWire::default(); RS_NR_CONTROL as usize];
    for (i, text) in control.iter().take(RS_NR_CONTROL as usize).enumerate() {
        let (addr, len) = place_string(&mut bytes, base, text);
        control_labels[i] = RsLabelWire { addr, len };
    }

    let mut wire = RsStartWire {
        flags: built.flags | args.rss_flags,
        ..RsStartWire::default()
    };
    wire.cmd_addr = cmd_addr;
    wire.cmd_len = cmd_len;
    wire.uid = built.uid;
    wire.sigmgr = built.sigmgr;
    wire.scheduler = built.scheduler;
    wire.priority = built.priority;
    wire.quantum = built.quantum;
    wire.major = args.major.unwrap_or(0) as i32;
    wire.period = args.period.unwrap_or(0);
    wire.script_addr = script_addr;
    wire.script_len = script_len;
    wire.asr_count = -1; // C `req_asr_count = -1` (`:164`)
    wire.restarts = 0;
    wire.heap_prealloc_bytes = 0;
    wire.map_prealloc_bytes = 0;
    wire.nr_irq = built.nr_irq;
    for (i, value) in built.irq.iter().take(16).enumerate() {
        wire.irq[i] = *value;
    }
    wire.nr_io = built.nr_io;
    for (i, (b, l)) in built.io.iter().take(16).enumerate() {
        wire.io[i] = RsIoRangeWire {
            base: *b as u32,
            len: *l as u32,
        };
    }
    wire.nr_pci_id = built.nr_pci_id;
    for (i, value) in built.pci_id.iter().take(32).enumerate() {
        wire.pci_id[i] = *value;
    }
    wire.nr_pci_class = built.nr_pci_class;
    for (i, value) in built.pci_class.iter().take(4).enumerate() {
        wire.pci_class[i] = *value;
    }
    wire.system = built.system;
    wire.vm = built.vm;
    wire.label = RsLabelWire {
        addr: label_addr,
        len: label_len,
    };
    wire.trg_label = RsLabelWire {
        addr: trg_addr,
        len: trg_len,
    };
    wire.ipc_addr = ipc_addr;
    wire.ipc_len = ipc_len;
    wire.nr_control = built.nr_control;
    wire.control = control_labels;
    // C sets `size = sizeof(rss_state_data)` (`:788-790`); the wire layout
    // spans seven fields = 56 bytes.
    wire.state_data = RsStateDataWire {
        size: 56,
        ..RsStateDataWire::default()
    };
    wire.progname_addr = prog_addr;
    wire.progname_len = prog_len;
    wire.nr_domain = built.nr_domain;
    for (i, value) in built.domain.iter().take(8).enumerate() {
        wire.domain[i] = *value;
    }
    // The capacity reserved above makes these arguments in bounds.
    let _ = encode_rs_start(&mut bytes[..rs_start_off::SIZE], &wire);
    Ok(RequestImage { bytes })
}

/// The default config file and its per-service search directories
/// (`minix3/minix/include/minix/paths.h:7-9`), and the passwd entry the
/// defaults come from (`SERVICE_LOGIN`, `priv.h`).
pub const PATH_SYSTEM_CONF: &str = "/etc/system.conf";
pub const PATH_SYSTEM_CONF_DIR: &str = "/etc/system.conf.d";
pub const PATH_SYSTEM_CONF_PKG_DIR: &str = "/usr/pkg/etc/system.conf.d";
pub const SERVICE_LOGIN: &str = "service";

/// `MAX_CLASS_RECURS 100` (`parse.c:39`): nesting bound for `class`
/// inheritance. C checks `class_recurs > MAX_CLASS_RECURS` at entry
/// (`:49-54`), so the deepest accepted chain is 101 calls.
pub const MAX_CLASS_RECURS: usize = 100;

/// `RS_NR_CONTROL 8` (`rs.h:55`) and `NR_DOMAIN 8` (`config.h:61`): the
/// two fixed-size label arrays the matching statements fill.
pub const RS_NR_CONTROL: i32 = 8;
pub const NR_DOMAIN: i32 = 8;
/// `PF_MAX` (`sys/sys/socket.h:223`, = `AF_MAX`): the `domain` range's
/// upper bound; `D` must satisfy `0 < D < PF_MAX`.
pub const PF_MAX: i32 = 35;

/// `RSS_IPC_ALL` / `RSS_IPC_ALL_SYS` (`rs.h:29-30`) — the sentinel strings a
/// lone `ipc ALL` / `ipc ALL_SYS` stands for.
pub const RSS_IPC_ALL: &str = "IPC_ALL";
pub const RSS_IPC_ALL_SYS: &str = "IPC_ALL_SYS";

/// `RSS_SYS_BASIC_CALLS` / `RSS_VM_BASIC_CALLS` (`rs.h:46-47`) — the two
/// default bits `do_config` installs before the walk (`parse.c:1171-1176`);
/// `system NONE` and `vm NONE` clear them.
pub const RSS_SYS_BASIC_CALLS: u32 = 0x4000;
pub const RSS_VM_BASIC_CALLS: u32 = 0x8000;

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
    /// `-progname <name>` (`req_progname`).
    pub progname: Option<String>,
    /// `-trglabel <name>` (`req_trg_label`).
    pub trg_label: Option<String>,
    /// `-config <path>` (`req_config`); setting it also marks the file as
    /// custom (`custom_config_file`, `minix-service.c:573-576`).
    pub config: Option<String>,
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
            "-progname" => {
                index += 1;
                let v = args
                    .get(index)
                    .ok_or(ParseError::OptionalArgumentNotComplete)?;
                out.progname = Some(String::from(*v));
            }
            "-trglabel" => {
                index += 1;
                let v = args
                    .get(index)
                    .ok_or(ParseError::OptionalArgumentNotComplete)?;
                out.trg_label = Some(String::from(*v));
            }
            "-config" => {
                index += 1;
                let v = args
                    .get(index)
                    .ok_or(ParseError::OptionalArgumentNotComplete)?;
                out.config = Some(String::from(*v));
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
#[derive(Debug, Clone, PartialEq, Eq)]
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
    /// `rss_flags` — C's clean defaults set `RSS_VM_BASIC_CALLS |
    /// RSS_SYS_BASIC_CALLS` (`parse.c:1171-1176`).
    pub flags: u32,
}

impl Default for RsStartDefaults {
    /// C's clean defaults (`parse.c:1160-1176`): `DSRV_SM`/`DSRV_SCH`/
    /// `DSRV_Q`/`DSRV_QT` plus the two BASIC_CALLS flag bits.
    fn default() -> Self {
        Self {
            service_uid: None,
            sigmgr: DSRV_SM,
            scheduler: DSRV_SCH,
            priority: DSRV_Q,
            quantum: DSRV_QT,
            flags: RSS_VM_BASIC_CALLS | RSS_SYS_BASIC_CALLS,
        }
    }
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

/// `system_tab` (`parse.c:828-849`): the `system` statement's call labels
/// and kernel call numbers. The bit index is `number - KERNEL_CALL`.
const SYSTEM_CALLS: [(&str, i32); 19] = [
    ("PRIVCTL", SYS_PRIVCTL),
    ("TRACE", SYS_TRACE),
    ("KILL", SYS_KILL),
    ("UMAP", SYS_UMAP),
    ("VIRCOPY", SYS_VIRCOPY),
    ("PHYSCOPY", SYS_PHYSCOPY),
    ("UMAP_REMOTE", SYS_UMAP_REMOTE),
    ("VUMAP", SYS_VUMAP),
    ("IRQCTL", SYS_IRQCTL),
    ("DEVIO", SYS_DEVIO),
    ("SDEVIO", SYS_SDEVIO),
    ("VDEVIO", SYS_VDEVIO),
    ("ABORT", SYS_ABORT),
    ("IOPENABLE", SYS_IOPENABLE),
    ("READBIOS", SYS_READBIOS),
    ("STIME", SYS_STIME),
    ("VMCTL", SYS_VMCTL),
    ("MEMSET", SYS_MEMSET),
    ("PADCONF", SYS_PADCONF),
];

/// `vm_table` (`parse.c:729-758`): the `vm` statement's labels and VM call
/// numbers. `PUSH_SIG` carries 0 in C (`:734`) — a placeholder that sets no
/// bit (`else if (vm_table[i].call_nr)`).
const VM_CALLS: [(&str, u32); 26] = [
    ("EXIT", VM_EXIT),
    ("FORK", VM_FORK),
    ("EXEC_NEWMEM", VM_EXEC_NEWMEM),
    ("PUSH_SIG", 0),
    ("WILLEXIT", VM_WILLEXIT),
    ("ADDDMA", VM_ADDDMA),
    ("DELDMA", VM_DELDMA),
    ("GETDMA", VM_GETDMA),
    ("REMAP", VM_REMAP),
    ("REMAP_RO", VM_REMAP_RO),
    ("SHM_UNMAP", VM_SHM_UNMAP),
    ("GETPHYS", VM_GETPHYS),
    ("GETREF", VM_GETREF),
    ("RS_SET_PRIV", VM_RS_SET_PRIV),
    ("INFO", VM_INFO),
    ("RS_UPDATE", VM_RS_UPDATE),
    ("RS_MEMCTL", VM_RS_MEMCTL),
    ("PROCCTL", VM_PROCCTL),
    ("MAPCACHEPAGE", VM_MAPCACHEPAGE),
    ("SETCACHEPAGE", VM_SETCACHEPAGE),
    ("FORGETCACHEPAGE", VM_FORGETCACHEPAGE),
    ("CLEARCACHE", VM_CLEARCACHE),
    ("VFS_MMAP", VM_VFS_MMAP),
    ("VFS_REPLY", VM_VFS_REPLY),
    ("GETRUSAGE", VM_GETRUSAGE),
    ("RS_PREPARE", VM_RS_PREPARE),
];

/// C `atoi` (`strtol(s, NULL, 10)` truncated to `int`): optional sign and
/// leading digits, the tail ignored, junk answers 0. The `domain` keyword
/// falls back to this when the word is not a protocol-family label.
fn atoi(s: &str) -> i32 {
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
    let mut acc: i64 = 0;
    while i < b.len() && b[i].is_ascii_digit() {
        acc = acc
            .saturating_mul(10)
            .saturating_add(i64::from(b[i] - b'0'));
        i += 1;
    }
    let v = if neg { -acc } else { acc };
    v.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

/// `domain_tab` (`parse.c:946-985`): protocol-family labels and their
/// `PF_*` numbers (`sys/sys/socket.h:150-223`). `PF_UNSPEC` is deliberately
/// absent (`:950`), and the range check rejects `0`.
const DOMAIN_TABLE: [(&str, i32); 32] = [
    ("LOCAL", 1),
    ("INET", 2),
    ("IMPLINK", 3),
    ("PUP", 4),
    ("CHAOS", 5),
    ("NS", 6),
    ("ISO", 7),
    ("ECMA", 8),
    ("DATAKIT", 9),
    ("CCITT", 10),
    ("SNA", 11),
    ("DECnet", 12),
    ("DLI", 13),
    ("LAT", 14),
    ("HYLINK", 15),
    ("APPLETALK", 16),
    ("OROUTE", 17),
    ("LINK", 18),
    ("XTP", 19),
    ("COIP", 20),
    ("CNT", 21),
    ("RTIP", 22),
    ("IPX", 23),
    ("INET6", 24),
    ("PIP", 25),
    ("ISDN", 26),
    ("NATM", 27),
    ("ARP", 28),
    ("KEY", 29),
    ("BLUETOOTH", 31),
    ("MPLS", 33),
    ("ROUTE", 34),
];

/// The do_config keyword walk over the parsed lines (`parse.c:1177-1263`
/// walk; the scalar keywords this slice covers are `uid`, `priority`,
/// `quantum`, `sigmgr`, `scheduler`, `descr`, the device-family keywords
/// `irq`, `io` and `pci` (`pci device` / `pci class`), the call-mask
/// keywords `system` and `vm`, the IPC contact list `ipc`, the two label
/// lists `control` and `domain`, the service `type`, and the `class`
/// inheritance face — every keyword `do_service` dispatches on is covered).
/// C's per-branch `fatal` faces become `ConfigError` with the same wording.
pub fn do_config_walk(
    lines: &[minix_proctools::configfile::ConfigEntry],
    label: Option<&str>,
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
        flags: defaults.flags,
        system: 0,
        vm: 0,
        ipc: None,
        nr_control: 0,
        control: Vec::new(),
        nr_domain: 0,
        domain: Vec::new(),
        service_type: None,
    };
    let mut selected = 0usize;
    for line in lines {
        if let Some(want) = label {
            // C `do_config` (`parse.c:1195-1225`) walks only the block
            // whose label matches; the full config stays available to
            // `class` lookups.
            let matched = line.is_sub()
                && line
                    .list
                    .get(1)
                    .is_some_and(|w| !w.is_string() && !w.is_sub() && w.word == want);
            if !matched {
                continue;
            }
            selected += 1;
        }
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
        walk_flat(&flat, &mut out, users, lines, 0)?;

        // `-label` overrides the config label (C `req_label`, `:563-565`).
        if let Some(label) = &args.label {
            out.label = Some(label.clone());
        }
    }
    if label.is_some() && selected == 0 {
        // C prints `minix-service: service '<label>' not found in '<file>'`
        // and exits (`parse.c:1230-1235`); the walk reports it as a config
        // error (the bin owns the exit).
        return Err(ConfigError {
            line: 0,
            message: format!("service '{}' not found", label.unwrap_or("")),
        });
    }
    Ok(out)
}

/// The keyword dispatch over one service block's flat word chain — C's
/// `do_service` (`parse.c:1027-1150`) plus the per-keyword handlers.
/// `depth` counts `class` nesting (C's `class_recurs`, `parse.c:39-56`);
/// the recursion re-enters here with the referenced block's words.
fn walk_flat(
    flat: &[&minix_proctools::configfile::ConfigEntry],
    out: &mut BuiltRsStart,
    users: Option<&str>,
    lines: &[minix_proctools::configfile::ConfigEntry],
    depth: usize,
) -> Result<(), minix_proctools::configfile::ConfigError> {
    use minix_proctools::configfile::ConfigError;
    let mut index = 0usize;
    while index < flat.len() {
        let word = flat[index];
        let Some(value) = flat.get(index + 1) else {
            break; // a trailing keyword with no value: C's walk ends too
        };
        match word.word.as_str() {
            "uid" => {
                out.uid =
                    user_by_name(users, &value.word).unwrap_or_else(|| resolve_uid(&value.word));
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
                                message: String::from("do_irq: NONE keyword not allowed in list"),
                            });
                        }
                        index += 1;
                        break;
                    }
                    if w.word == "ALL" {
                        if !first || !at_list_end {
                            return Err(ConfigError {
                                line: w.line,
                                message: String::from("do_irq: ALL keyword not allowed in list"),
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
                                message: String::from("do_io: NONE keyword not allowed in list"),
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
            "system" => {
                // C `do_system` (`parse.c:852-914`): `BASIC` keeps the
                // default, `NONE` clears `RSS_SYS_BASIC_CALLS`, `ALL`
                // fills the vector, and labels set
                // `SYS_* - KERNEL_CALL` bits.
                index += 1;
                let mut first = true;
                while index < flat.len() {
                    let w = flat[index];
                    if STATEMENT_KEYWORDS.contains(&w.word.as_str()) {
                        break;
                    }
                    if w.is_string() || w.is_sub() {
                        return Err(ConfigError {
                            line: w.line,
                            message: String::from("do_system: unexpected string"),
                        });
                    }
                    let at_list_end = index + 1 >= flat.len()
                        || STATEMENT_KEYWORDS.contains(&flat[index + 1].word.as_str());
                    if w.word == "BASIC" {
                        if !first || !at_list_end {
                            // C prints `KW_NONE` here (`parse.c:877-879`)
                            // — a faithful quirk.
                            return Err(ConfigError {
                                line: w.line,
                                message: String::from(
                                    "do_system: NONE keyword not allowed in list",
                                ),
                            });
                        }
                        index += 1;
                        break;
                    }
                    if w.word == "NONE" {
                        if !first || !at_list_end {
                            return Err(ConfigError {
                                line: w.line,
                                message: String::from(
                                    "do_system: NONE keyword not allowed in list",
                                ),
                            });
                        }
                        out.flags &= !RSS_SYS_BASIC_CALLS;
                        index += 1;
                        break;
                    }
                    if w.word == "ALL" {
                        if !first || !at_list_end {
                            return Err(ConfigError {
                                line: w.line,
                                message: String::from("do_system: ALL keyword not allowed in list"),
                            });
                        }
                        for i in 0..NR_SYS_CALLS {
                            out.system |= 1u64 << (i as u32);
                        }
                        index += 1;
                        break;
                    }
                    if let Some(&(_, call)) =
                        SYSTEM_CALLS.iter().find(|(label, _)| *label == w.word)
                    {
                        out.system |= 1u64 << ((call - KERNEL_CALL) as u32);
                    }
                    // Unknown names warn and continue in C
                    // (`parse.c:895-898`); the walk has no warning
                    // channel yet (registered deviation).
                    first = false;
                    index += 1;
                }
            }
            "vm" => {
                // C `do_vm` (`parse.c:760-826`): same face as `system`
                // over the VM vector; `PUSH_SIG`'s zero number sets no
                // bit.
                index += 1;
                let mut first = true;
                while index < flat.len() {
                    let w = flat[index];
                    if STATEMENT_KEYWORDS.contains(&w.word.as_str()) {
                        break;
                    }
                    if w.is_string() || w.is_sub() {
                        return Err(ConfigError {
                            line: w.line,
                            message: String::from("do_vm: unexpected string"),
                        });
                    }
                    let at_list_end = index + 1 >= flat.len()
                        || STATEMENT_KEYWORDS.contains(&flat[index + 1].word.as_str());
                    if w.word == "BASIC" {
                        if !first || !at_list_end {
                            // C prints `KW_NONE` here too
                            // (`parse.c:779-783`).
                            return Err(ConfigError {
                                line: w.line,
                                message: String::from("do_vm: NONE keyword not allowed in list"),
                            });
                        }
                        index += 1;
                        break;
                    }
                    if w.word == "NONE" {
                        if !first || !at_list_end {
                            return Err(ConfigError {
                                line: w.line,
                                message: String::from("do_vm: NONE keyword not allowed in list"),
                            });
                        }
                        out.flags &= !RSS_VM_BASIC_CALLS;
                        index += 1;
                        break;
                    }
                    if w.word == "ALL" {
                        if !first || !at_list_end {
                            return Err(ConfigError {
                                line: w.line,
                                message: String::from("do_vm: ALL keyword not allowed in list"),
                            });
                        }
                        for i in 0..NR_VM_CALLS {
                            out.vm |= 1u64 << i;
                        }
                        index += 1;
                        break;
                    }
                    match VM_CALLS.iter().find(|(label, _)| *label == w.word) {
                        // `PUSH_SIG`'s zero number sets no bit.
                        Some(&(_, call)) if call != 0 => {
                            out.vm |= 1u64 << (call - VM_RQ_BASE);
                        }
                        _ => {}
                    }
                    first = false;
                    index += 1;
                }
            }
            "ipc" => {
                // C `do_ipc` (`parse.c:658-706`): one space-separated
                // contact list; `ALL`/`ALL_SYS` must stand alone and
                // expand to the sentinel strings; a second `ipc`
                // statement is the `req_ipc is set` fatal.
                index += 1;
                let mut first = true;
                let mut parts: Vec<&str> = Vec::new();
                while index < flat.len() {
                    let w = flat[index];
                    if STATEMENT_KEYWORDS.contains(&w.word.as_str()) {
                        break;
                    }
                    if w.is_string() || w.is_sub() {
                        return Err(ConfigError {
                            line: w.line,
                            message: String::from("do_ipc: unexpected string"),
                        });
                    }
                    let at_list_end = index + 1 >= flat.len()
                        || STATEMENT_KEYWORDS.contains(&flat[index + 1].word.as_str());
                    let word = if w.word == "ALL" || w.word == "ALL_SYS" {
                        if !first || !at_list_end {
                            return Err(ConfigError {
                                line: w.line,
                                message: format!("do_ipc: {} keyword not allowed in list", w.word),
                            });
                        }
                        if w.word == "ALL" {
                            RSS_IPC_ALL
                        } else {
                            RSS_IPC_ALL_SYS
                        }
                    } else {
                        &w.word
                    };
                    parts.push(word);
                    first = false;
                    index += 1;
                }
                if out.ipc.is_some() {
                    // C's fatal carries no source position
                    // (`parse.c:703-704`); the statement's line is the
                    // closest anchor this walker has.
                    return Err(ConfigError {
                        line: word.line,
                        message: String::from("do_ipc: req_ipc is set"),
                    });
                }
                out.ipc = Some(parts.join(" "));
            }
            "control" => {
                // C `do_control` (`parse.c:916-942`): every word is a
                // control label; the local count restarts per
                // statement, so a repeated `control` replaces the list.
                index += 1;
                let mut labels: Vec<String> = Vec::new();
                while index < flat.len() {
                    let w = flat[index];
                    if STATEMENT_KEYWORDS.contains(&w.word.as_str()) {
                        break;
                    }
                    if w.is_string() || w.is_sub() {
                        return Err(ConfigError {
                            line: w.line,
                            message: String::from("do_control: unexpected string"),
                        });
                    }
                    if labels.len() >= RS_NR_CONTROL as usize {
                        return Err(ConfigError {
                            line: w.line,
                            message: format!(
                                "do_control: RS_NR_CONTROL is too small ({} needed)",
                                labels.len() + 1
                            ),
                        });
                    }
                    labels.push(w.word.clone());
                    index += 1;
                }
                out.nr_control = labels.len() as i32;
                out.control = labels;
            }
            "domain" => {
                // C `do_domain` (`parse.c:989-1024`): labels resolve
                // through `domain_tab`, anything else through `atoi`;
                // the result must satisfy `0 < D < PF_MAX`. The count
                // restarts per statement (replace, not append).
                index += 1;
                let mut domains: Vec<i32> = Vec::new();
                while index < flat.len() {
                    let w = flat[index];
                    if STATEMENT_KEYWORDS.contains(&w.word.as_str()) {
                        break;
                    }
                    if w.is_string() || w.is_sub() {
                        return Err(ConfigError {
                            line: w.line,
                            message: String::from("do_domain: unexpected string"),
                        });
                    }
                    if domains.len() >= NR_DOMAIN as usize {
                        return Err(ConfigError {
                            line: w.line,
                            message: format!(
                                "do_domain: NR_DOMAIN is too small ({} needed)",
                                domains.len() + 1
                            ),
                        });
                    }
                    let domain = DOMAIN_TABLE
                        .iter()
                        .find(|(name, _)| *name == w.word)
                        .map(|&(_, number)| number)
                        .unwrap_or_else(|| atoi(&w.word));
                    if domain <= 0 || domain >= PF_MAX {
                        return Err(ConfigError {
                            line: w.line,
                            message: format!("do_domain: unknown domain {}", w.word),
                        });
                    }
                    domains.push(domain);
                    index += 1;
                }
                out.nr_domain = domains.len() as i32;
                out.domain = domains;
            }
            "type" => {
                // C `do_type` (`parse.c:198-225`): exactly one value,
                // and the only accepted type is `net`; a second `type`
                // statement is fatal.
                index += 1;
                let Some(value) = flat.get(index) else {
                    break;
                };
                // `cpe->next != NULL` — the value must stand alone.
                if let Some(next) = flat.get(index + 1)
                    && !STATEMENT_KEYWORDS.contains(&next.word.as_str())
                {
                    return Err(ConfigError {
                        line: value.line,
                        message: String::from("do_type: just one type value expected"),
                    });
                }
                if value.is_string() || value.is_sub() {
                    return Err(ConfigError {
                        line: value.line,
                        message: String::from("do_type: unexpected string"),
                    });
                }
                if out.service_type.is_some() {
                    return Err(ConfigError {
                        line: value.line,
                        message: String::from("do_type: another type"),
                    });
                }
                if value.word != "net" {
                    return Err(ConfigError {
                        line: value.line,
                        message: String::from("do_type: odd type"),
                    });
                }
                out.service_type = Some(value.word.clone());
                index += 1;
            }
            "class" => {
                // C `do_class` (`parse.c:45-114`): each label names
                // another service entry whose statements apply as well
                // (resource inheritance); the lookup walks the whole
                // config and re-enters the dispatch on the found block.
                if depth > MAX_CLASS_RECURS {
                    return Err(ConfigError {
                        line: word.line,
                        message: format!(
                            "do_class: nesting level too high for class '{}'",
                            flat.get(index + 1).map_or("", |w| w.word.as_str())
                        ),
                    });
                }
                index += 1;
                while index < flat.len() {
                    let w = flat[index];
                    if STATEMENT_KEYWORDS.contains(&w.word.as_str()) {
                        break;
                    }
                    if w.is_string() || w.is_sub() {
                        // C's string fatal reuses the `do_uid` wording
                        // (`parse.c:67-69`) — kept verbatim.
                        return Err(ConfigError {
                            line: w.line,
                            message: String::from("do_uid: unexpected string"),
                        });
                    }
                    let target = lines.iter().find(|line| {
                        line.is_sub()
                            && line.list.len() >= 2
                            && !line.list[0].is_string()
                            && !line.list[0].is_sub()
                            && line.list[0].word == "service"
                            && !line.list[1].is_string()
                            && !line.list[1].is_sub()
                            && line.list[1].word == w.word
                    });
                    let Some(target) = target else {
                        return Err(ConfigError {
                            line: w.line,
                            message: format!("do_class: no entry found for class '{}'", w.word),
                        });
                    };
                    let mut inner: Vec<&minix_proctools::configfile::ConfigEntry> = Vec::new();
                    flatten_words(&target.list[2..], &mut inner);
                    walk_flat(&inner, out, users, lines, depth + 1)?;
                    index += 1;
                }
            }
            _ => {
                // Unknown words (including a service name that is not a
                // statement keyword) are skipped to surface every
                // unknown.
                index += 1;
            }
        }
    }
    Ok(())
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

/// `getpwnam(SERVICE_LOGIN)` (`parse.c:1162`): the `service` entry's uid
/// from the caller-read passwd text — the bin feeds this into
/// [`RsStartDefaults::service_uid`] the way C's `do_config` calls
/// `getpwnam` before the walk.
pub fn resolve_service_uid(users: &str) -> Option<u32> {
    user_by_name(Some(users), SERVICE_LOGIN)
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
    /// `rss_flags` — starts at the two BASIC_CALLS bits (`system NONE` /
    /// `vm NONE` clear them).
    pub flags: u32,
    /// `rss_system` — kernel-call mask, bit *i* is call `KERNEL_CALL + i`.
    pub system: u64,
    /// `rss_vm` — VM-call mask, bit *i* is call `VM_RQ_BASE + i`.
    pub vm: u64,
    /// `rss_ipc` — the space-separated IPC target list (`do_config` falls
    /// back to `RSS_IPC_ALL_SYS` when no `ipc` statement set it,
    /// `parse.c:1240-1246`; that fallback belongs to the bin's build step).
    pub ipc: Option<String>,
    /// `rss_nr_control` / `rss_control` — control-endpoint labels
    /// (`RS_NR_CONTROL` slots; a repeated statement replaces the list).
    pub nr_control: i32,
    pub control: Vec<String>,
    /// `rss_nr_domain` / `rss_domain` — socket protocol families
    /// (`NR_DOMAIN` slots; a repeated statement replaces the list).
    pub nr_domain: i32,
    pub domain: Vec<i32>,
    /// `rs_config.type` (`config.h:5`): the service type. Only `net` is
    /// accepted (`do_type`, `parse.c:198-225`); `None` when absent.
    pub service_type: Option<String>,
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
            flags: RSS_VM_BASIC_CALLS | RSS_SYS_BASIC_CALLS,
        };
        let args = parse_args(&["up", "/service/lwip", "-label", "lwip"]).unwrap();
        let got = do_config_walk(&lines, None, &defaults, &args, Some(PW)).unwrap();
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
        let e = do_config_walk(&lines, None, &defaults, &args, Some(PW)).unwrap_err();
        assert_eq!(e.message, "just one uid/login expected");
        let text = "service x {\n  priority abc;\n}\n";
        let lines = config_read_text(text, "t", 0, &|_| None).unwrap();
        let e = do_config_walk(&lines, None, &defaults, &args, Some(PW)).unwrap_err();
        assert_eq!(e.message, "bad priority 'abc'");
    }

    #[test]
    fn the_head_must_be_the_service_word() {
        let text = "widget x {\n}\n";
        let lines = config_read_text(text, "t", 0, &|_| None).unwrap();
        let defaults = RsStartDefaults::default();
        let args = parse_args(&["up", "/bin/x"]).unwrap();
        let e = do_config_walk(&lines, None, &defaults, &args, Some(PW)).unwrap_err();
        assert_eq!(e.message, "expected word 'service'");
    }

    #[test]
    fn descr_takes_a_quoted_string() {
        let text = "service x {\n  descr \"the x server\";\n}\n";
        let lines = config_read_text(text, "t", 0, &|_| None).unwrap();
        let defaults = RsStartDefaults::default();
        let args = parse_args(&["up", "/bin/x"]).unwrap();
        let got = do_config_walk(&lines, None, &defaults, &args, Some(PW)).unwrap();
        assert_eq!(got.descr.as_deref(), Some("the x server"));
    }

    /// `walk_block(text, label)` feeds only the named service block through
    /// `do_config_walk` — the C selection `do_config` performs before
    /// `do_service` (needed for the `class` inheritance tests).
    fn walk_block(
        text: &str,
        label: &str,
    ) -> Result<BuiltRsStart, minix_proctools::configfile::ConfigError> {
        let lines = config_read_text(text, "t", 0, &|_| None).unwrap();
        let defaults = RsStartDefaults::default();
        let args = parse_args(&["up", "/bin/x"]).unwrap();
        do_config_walk(&lines, Some(label), &defaults, &args, Some(PW))
    }

    /// `walk(text)` with the C defaults — the harness the device-family
    /// tests below share.
    fn walk(text: &str) -> Result<BuiltRsStart, minix_proctools::configfile::ConfigError> {
        let lines = config_read_text(text, "t", 0, &|_| None).unwrap();
        let defaults = RsStartDefaults::default();
        let args = parse_args(&["up", "/bin/x"]).unwrap();
        do_config_walk(&lines, None, &defaults, &args, Some(PW))
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
    fn system_and_vm_all_fill_the_whole_vectors() {
        // `parse.c:886-893` / `:803-810`: ALL sets every bit the C array
        // holds (58 kernel calls, 49 VM calls).
        let got = walk("service x {\n  system ALL;\n  vm ALL;\n}\n").unwrap();
        assert_eq!(got.system, (1u64 << (NR_SYS_CALLS as u32)) - 1);
        assert_eq!(got.vm, (1u64 << NR_VM_CALLS) - 1);
    }

    #[test]
    fn system_and_vm_none_clear_their_basic_flags() {
        // `parse.c:881-885` / `:798-802`: NONE clears the BASIC_CALLS bit
        // and leaves the vector empty; the other family's bit survives.
        let got = walk("service x {\n  system NONE;\n}\n").unwrap();
        assert_eq!(got.system, 0);
        assert_eq!(got.flags & RSS_SYS_BASIC_CALLS, 0);
        assert_eq!(got.flags & RSS_VM_BASIC_CALLS, RSS_VM_BASIC_CALLS);
        let got = walk("service x {\n  vm NONE;\n}\n").unwrap();
        assert_eq!(got.vm, 0);
        assert_eq!(got.flags & RSS_VM_BASIC_CALLS, 0);
        assert_eq!(got.flags & RSS_SYS_BASIC_CALLS, RSS_SYS_BASIC_CALLS);
    }

    #[test]
    fn system_and_vm_named_calls_set_their_bits() {
        // Bit index = call number − vector base (`parse.c:899-900`,
        // `:814-817`).
        let got = walk("service x {\n  system PRIVCTL KILL;\n  vm VFS_MMAP;\n}\n").unwrap();
        assert_eq!(
            got.system,
            (1u64 << ((SYS_PRIVCTL - KERNEL_CALL) as u32))
                | (1u64 << ((SYS_KILL - KERNEL_CALL) as u32))
        );
        assert_eq!(got.vm, 1u64 << (VM_VFS_MMAP - VM_RQ_BASE));
    }

    #[test]
    fn system_basic_is_the_default_noop_and_push_sig_sets_no_bit() {
        // BASIC breaks without touching anything (the defaults already
        // carry the flag); `PUSH_SIG`'s zero number sets no bit
        // (`parse.c:734`, `:814`).
        let got = walk("service x {\n  system BASIC;\n  vm BASIC;\n}\n").unwrap();
        assert_eq!(got.system, 0);
        assert_eq!(got.vm, 0);
        assert_eq!(got.flags, RSS_VM_BASIC_CALLS | RSS_SYS_BASIC_CALLS);
        let got = walk("service x {\n  vm PUSH_SIG;\n}\n").unwrap();
        assert_eq!(got.vm, 0);
    }

    #[test]
    fn unknown_call_names_are_ignored() {
        // C warns and continues (`parse.c:895-898`, `:810-812`); the walk
        // has no warning channel, so the observable face is "no bit".
        let got = walk("service x {\n  system FOO BAR;\n  vm NOPE;\n}\n").unwrap();
        assert_eq!(got.system, 0);
        assert_eq!(got.vm, 0);
    }

    #[test]
    fn system_vm_error_faces_match_the_c_fatals() {
        let e = walk("service x {\n  system ALL KILL;\n}\n").unwrap_err();
        assert_eq!(e.message, "do_system: ALL keyword not allowed in list");
        // C prints KW_NONE for the BASIC fatal too (`parse.c:877-879`).
        let e = walk("service x {\n  system BASIC KILL;\n}\n").unwrap_err();
        assert_eq!(e.message, "do_system: NONE keyword not allowed in list");
        let e = walk("service x {\n  vm NONE VFS_MMAP;\n}\n").unwrap_err();
        assert_eq!(e.message, "do_vm: NONE keyword not allowed in list");
        let e = walk("service x {\n  system \"x\";\n}\n").unwrap_err();
        assert_eq!(e.message, "do_system: unexpected string");
    }

    #[test]
    fn ipc_list_joins_labels_with_single_spaces() {
        // `parse.c:686-699`: each word is appended after one space; `list+1`
        // drops the leading space, so the face is a single-space join.
        let got = walk("service x {\n  ipc SYSTEM pm vfs;\n}\n").unwrap();
        assert_eq!(got.ipc.as_deref(), Some("SYSTEM pm vfs"));
    }

    #[test]
    fn ipc_all_and_all_sys_are_the_sentinel_strings() {
        // `rs.h:29-30`: `ipc ALL;` stands for "IPC_ALL", `ipc ALL_SYS;` for
        // "IPC_ALL_SYS".
        let got = walk("service x {\n  ipc ALL;\n}\n").unwrap();
        assert_eq!(got.ipc.as_deref(), Some(RSS_IPC_ALL));
        let got = walk("service x {\n  ipc ALL_SYS;\n}\n").unwrap();
        assert_eq!(got.ipc.as_deref(), Some(RSS_IPC_ALL_SYS));
        assert_eq!(RSS_IPC_ALL, "IPC_ALL");
        assert_eq!(RSS_IPC_ALL_SYS, "IPC_ALL_SYS");
    }

    #[test]
    fn ipc_all_must_stand_alone() {
        let e = walk("service x {\n  ipc SYSTEM ALL;\n}\n").unwrap_err();
        assert_eq!(e.message, "do_ipc: ALL keyword not allowed in list");
        let e = walk("service x {\n  ipc ALL_SYS pm;\n}\n").unwrap_err();
        assert_eq!(e.message, "do_ipc: ALL_SYS keyword not allowed in list");
        let e = walk("service x {\n  ipc ALL ALL;\n}\n").unwrap_err();
        assert_eq!(e.message, "do_ipc: ALL keyword not allowed in list");
    }

    #[test]
    fn a_second_ipc_statement_is_the_c_fatal() {
        // `parse.c:703-704`: the request's IPC list may be set only once.
        let e = walk("service x {\n  ipc SYSTEM;\n  ipc ALL;\n}\n").unwrap_err();
        assert_eq!(e.message, "do_ipc: req_ipc is set");
    }

    #[test]
    fn ipc_list_ends_at_the_next_statement() {
        let got = walk("service x {\n  ipc SYSTEM pm;\n  irq 4;\n}\n").unwrap();
        assert_eq!(got.ipc.as_deref(), Some("SYSTEM pm"));
        assert_eq!(got.irq, vec![4]);
        let e = walk("service x {\n  ipc \"SYSTEM\";\n}\n").unwrap_err();
        assert_eq!(e.message, "do_ipc: unexpected string");
    }

    #[test]
    fn control_labels_are_collected_and_replaced() {
        // `parse.c:916-942`: every word is a label; the count restarts per
        // statement, so a repeated `control` replaces the list.
        // Labels like `pci` cannot appear here: it is also a statement
        // keyword, and the flat walk ends a list at the next statement
        // keyword (registered deviation, batch 548).
        let got = walk("service x {\n  control at_wini devman;\n}\n").unwrap();
        assert_eq!(got.nr_control, 2);
        assert_eq!(got.control, vec!["at_wini", "devman"]);
        let got = walk("service x {\n  control a;\n  control b;\n}\n").unwrap();
        assert_eq!(got.nr_control, 1);
        assert_eq!(got.control, vec!["b"]);
    }

    #[test]
    fn control_limit_is_rs_nr_control() {
        let mut text = String::from("service x {\n  control");
        for n in 0..=RS_NR_CONTROL {
            text.push_str(&format!(" l{n}"));
        }
        text.push_str(";\n}\n");
        let e = walk(&text).unwrap_err();
        assert_eq!(
            e.message,
            "do_control: RS_NR_CONTROL is too small (9 needed)"
        );
    }

    #[test]
    fn domain_labels_and_numbers_take_the_c_face() {
        // `parse.c:1006-1018`: labels resolve through `domain_tab`
        // (case-sensitive, `DECnet` included), anything else through
        // `atoi`.
        let got = walk("service x {\n  domain INET LOCAL 2 DECnet;\n}\n").unwrap();
        assert_eq!(got.nr_domain, 4);
        assert_eq!(got.domain, vec![2, 1, 2, 12]);
        let got = walk("service x {\n  domain 24;\n}\n").unwrap();
        assert_eq!(got.domain, vec![24]);
        // `atoi` stops at the tail: `2x` is 2.
        let got = walk("service x {\n  domain 2x;\n}\n").unwrap();
        assert_eq!(got.domain, vec![2]);
    }

    #[test]
    fn domain_range_and_unknown_faces() {
        // `0 < D < PF_MAX` (35); labels are case-sensitive, so a lowercase
        // `inet` falls through `atoi` to 0 and is rejected.
        let e = walk("service x {\n  domain FOO;\n}\n").unwrap_err();
        assert_eq!(e.message, "do_domain: unknown domain FOO");
        let e = walk("service x {\n  domain inet;\n}\n").unwrap_err();
        assert_eq!(e.message, "do_domain: unknown domain inet");
        let e = walk("service x {\n  domain 0;\n}\n").unwrap_err();
        assert_eq!(e.message, "do_domain: unknown domain 0");
        let e = walk("service x {\n  domain 35;\n}\n").unwrap_err();
        assert_eq!(e.message, "do_domain: unknown domain 35");
        let got = walk("service x {\n  domain 34;\n}\n").unwrap();
        assert_eq!(got.domain, vec![34]);
    }

    #[test]
    fn domain_limit_is_nr_domain() {
        let mut text = String::from("service x {\n  domain");
        for n in 1..=(NR_DOMAIN + 1) {
            text.push_str(&format!(" {n}"));
        }
        text.push_str(";\n}\n");
        let e = walk(&text).unwrap_err();
        assert_eq!(e.message, "do_domain: NR_DOMAIN is too small (9 needed)");
    }

    #[test]
    fn control_and_domain_lists_end_at_the_next_statement() {
        let got = walk("service x {\n  control at_wini;\n  domain INET;\n  irq 4;\n}\n").unwrap();
        assert_eq!(got.control, vec!["at_wini"]);
        assert_eq!(got.domain, vec![2]);
        assert_eq!(got.irq, vec![4]);
        let e = walk("service x {\n  control \"x\";\n}\n").unwrap_err();
        assert_eq!(e.message, "do_control: unexpected string");
        let e = walk("service x {\n  domain \"INET\";\n}\n").unwrap_err();
        assert_eq!(e.message, "do_domain: unexpected string");
    }

    #[test]
    fn type_net_sets_the_service_type() {
        // The real driver configs carry `type net;` as their first
        // statement (`minix3/minix/drivers/net/e1000/e1000.conf:3`).
        let got = walk("service x {\n  type net;\n  irq 4;\n}\n").unwrap();
        assert_eq!(got.service_type.as_deref(), Some("net"));
        assert_eq!(got.irq, vec![4]);
        assert_eq!(walk("service x {\n}\n").unwrap().service_type, None);
    }

    #[test]
    fn type_error_faces_match_the_c_fatals() {
        let e = walk("service x {\n  type foo;\n}\n").unwrap_err();
        assert_eq!(e.message, "do_type: odd type");
        let e = walk("service x {\n  type net net;\n}\n").unwrap_err();
        assert_eq!(e.message, "do_type: just one type value expected");
        let e = walk("service x {\n  type net;\n  type net;\n}\n").unwrap_err();
        assert_eq!(e.message, "do_type: another type");
        let e = walk("service x {\n  type \"net\";\n}\n").unwrap_err();
        assert_eq!(e.message, "do_type: unexpected string");
    }

    #[test]
    fn dbg_multi() {
        for text in [
            "service a {\n  irq 4;\n};\nservice b {\n  io 3f8:8;\n};\n",
            "service a {\n  irq 4;\n}\nservice b {\n  io 3f8:8;\n}\n",
        ] {
            let lines = config_read_text(text, "t", 0, &|_| None);
            println!(
                "TEXT ok={} len={:?}",
                lines.is_ok(),
                lines.as_ref().map(|v| v.len())
            );
        }
    }

    #[test]
    fn class_inherits_another_blocks_statements() {
        // C's `do_config` selects one block by label, then `do_class`
        // re-runs `do_service` over the referenced block (`parse.c:45-114`).
        // Feeding only the child block mirrors that selection.
        let text = "service base {\n  irq 4;\n  system ALL;\n};\nservice child {\n  class base;\n  io 3f8:8;\n};\n";
        let got = walk_block(text, "child").unwrap();
        assert_eq!(got.irq, vec![4]);
        assert_eq!(got.system, (1u64 << (NR_SYS_CALLS as u32)) - 1);
        assert_eq!(got.io, vec![(0x3f8, 8)]);
    }

    #[test]
    fn class_inheritance_is_transitive() {
        let text = "service mid {\n  class base;\n};\nservice base {\n  irq 7;\n};\nservice child {\n  class mid;\n};\n";
        let got = walk_block(text, "child").unwrap();
        assert_eq!(got.irq, vec![7]);
    }

    #[test]
    fn class_missing_entry_is_fatal() {
        let text = "service child {\n  class nope;\n}\n";
        let e = walk_block(text, "child").unwrap_err();
        assert_eq!(e.message, "do_class: no entry found for class 'nope'");
    }

    #[test]
    fn class_nesting_is_bounded() {
        // `MAX_CLASS_RECURS` = 100 (`parse.c:39`); a self-referencing
        // class hits the bound instead of looping.
        let text = "service a {\n  class a;\n}\n";
        let e = walk_block(text, "a").unwrap_err();
        assert_eq!(e.message, "do_class: nesting level too high for class 'a'");
    }

    #[test]
    fn up_image_carries_the_config_and_its_strings() {
        use minix_sys::rs::decode_rs_start;
        // C `minix-service.c:746-816`: the image is the `rs_start` struct
        // followed by every string it points at.
        let text = "service e1000 {\n  type net;\n  system ALL;\n  ipc SYSTEM pm;\n};\n";
        let lines = config_read_text(text, "t", 0, &|_| None).unwrap();
        let args = parse_args(&["up", "/sbin/e1000", "-label", "e1000"]).unwrap();
        let defaults = RsStartDefaults {
            service_uid: resolve_service_uid(PW),
            ..RsStartDefaults::default()
        };
        let img = build_up_request(&lines, "e1000", &defaults, &args, Some(PW)).unwrap();
        let wire = decode_rs_start(&img.bytes).unwrap();
        assert_eq!(wire.uid, 50, "uid service 走 passwd 面");
        assert_eq!(wire.sigmgr, DSRV_SM);
        assert_eq!(wire.scheduler, DSRV_SCH);
        assert_eq!(wire.priority, DSRV_Q);
        assert_eq!(wire.quantum, DSRV_QT);
        assert_eq!(wire.flags, defaults.flags | args.rss_flags);
        assert_eq!(wire.system, (1u64 << (NR_SYS_CALLS as u32)) - 1);
        assert_eq!(wire.asr_count, -1, "C req_asr_count 默认 -1");
        assert_eq!(wire.state_data.size, 56);
        // Every pointer the struct carries points inside the image.
        let read = |addr: u64, len: u64| {
            let off = (addr - img.address()) as usize;
            String::from_utf8(img.bytes[off..off + len as usize].to_vec()).unwrap()
        };
        assert_eq!(read(wire.cmd_addr, wire.cmd_len), "/sbin/e1000 ");
        assert_eq!(read(wire.progname_addr, wire.progname_len), "e1000");
        assert_eq!(read(wire.label.addr, wire.label.len), "e1000");
        assert_eq!(read(wire.ipc_addr, wire.ipc_len), "SYSTEM pm");
        assert_eq!(wire.control[0].len, 0);
    }

    #[test]
    fn up_image_defaults_ipc_label_and_progname() {
        use minix_sys::rs::decode_rs_start;
        // No `ipc` statement → `RSS_IPC_ALL_SYS` (`parse.c:1240-1246`);
        // no `-label` → the progname; `-progname` overrides the basename.
        let text = "service lwip {\n};\n";
        let lines = config_read_text(text, "t", 0, &|_| None).unwrap();
        let defaults = RsStartDefaults::default();
        let args = parse_args(&["up", "/service/lwip"]).unwrap();
        let img = build_up_request(&lines, "lwip", &defaults, &args, Some(PW)).unwrap();
        let wire = decode_rs_start(&img.bytes).unwrap();
        let read = |addr: u64, len: u64| {
            let off = (addr - img.address()) as usize;
            String::from_utf8(img.bytes[off..off + len as usize].to_vec()).unwrap()
        };
        assert_eq!(read(wire.ipc_addr, wire.ipc_len), RSS_IPC_ALL_SYS);
        assert_eq!(read(wire.label.addr, wire.label.len), "lwip");
        assert_eq!(read(wire.progname_addr, wire.progname_len), "lwip");
        let args = parse_args(&[
            "up",
            "/service/lwip",
            "-progname",
            "lwip2",
            "-trglabel",
            "t",
        ])
        .unwrap();
        let img = build_up_request(&lines, "lwip", &defaults, &args, Some(PW)).unwrap();
        let wire = decode_rs_start(&img.bytes).unwrap();
        let read = |addr: u64, len: u64| {
            let off = (addr - img.address()) as usize;
            String::from_utf8(img.bytes[off..off + len as usize].to_vec()).unwrap()
        };
        assert_eq!(read(wire.progname_addr, wire.progname_len), "lwip2");
        assert_eq!(read(wire.trg_label.addr, wire.trg_label.len), "t");
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
