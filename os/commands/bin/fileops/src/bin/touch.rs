//! Minix-RS `touch` — the doing half over `minix_fileops::touch`.
//!
//! Ground truth: `minix3/usr.bin/touch/touch.c`. `touch file` sets the access
//! and modification times to the current time and, when the file does not
//! exist, creates it (`-c` suppresses creation; `-a`/`-m` restrict which of the
//! two times is changed). `-t [[CC]YY]MMDDhhmm[.SS]` and the obsolete bare
//! 8/10-digit form set an explicit time; the field parsing rides
//! [`minix_fileops::touch::parse_time_spec`] (see that module for the UTC
//! timezone caveat).
//!
//! `-r file` is wired: C's `stime_file()` (`touch.c:318-327`) is one `stat` of
//! the reference plus the two times it carries. `-d datetime` is wired too:
//! C's `stime_arg0` (`touch.c:214-222`) hands the argument to libutil's
//! `parsedate`, whose grammar, lexer and UTC conversion this tree ports in
//! [`minix_fileops::parsedate`]. `-h` touches the link itself through
//! `AT_SYMLINK_NOFOLLOW`. The GNU long spellings (`--date`, `--reference`)
//! are not options at all in C's `getopt` string, so they answer `usage()`
//! exactly as C does. Real file creation and time-setting need a writable
//! root, so the run is gated with the write-class commands (`E5`).

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use minix_sys::vfs::{AT_FDCWD, AT_SYMLINK_NOFOLLOW};
use minix_sys::{Stat, close, fstat, gettimeofday, lstat, open, stat, utimens, utimensat};

/// C's usage line (`touch.c` end of file): capital `Usage:`, the program
/// name, and the long spellings that getopt itself does not accept.
fn usage() -> ! {
    let argv = support::args();
    let prog = argv
        .first()
        .map(|arg| arg.rsplit('/').next().unwrap_or(arg))
        .unwrap_or("touch");
    support::warn(
        format!(
            "Usage: {prog} [-acfhm] [-d|--date datetime] [-r|--reference file] [-t time] file ...\n"
        )
        .as_bytes(),
    );
    support::terminate(1)
}

/// `fcntl.h` open flags (Minix values, matching the other write-side binaries).
const O_WRONLY: i32 = 0x0000_0001;
const O_CREAT: i32 = 0x0000_0200;
/// `sys/stat.h` `DEFFILEMODE` (creation mode before umask), C `0666`.
const DEFFILEMODE: u32 = 0o666;

/// The whole program body; both `main` forms call it and it never returns.
fn run() -> ! {
    let argv: Vec<String> = support::args();
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();
    let prog = support::prog_name(&argv);

    let mut aflag = false;
    let mut hflag = false;
    let mut mflag = false;
    let mut cflag = false;
    let mut timeset = false;
    let mut spec: &str = "";
    // C's `-t` and `-r` arms both write the same `ts[2]` (`touch.c:114-121`), so
    // **the later flag owns both times** and the order on the command line is
    // observable. `reference` holds what the last `-r` stated;
    // `reference_last` says whether that `-r` came after the last `-t`.
    let mut reference: Option<[(i64, i64); 2]> = None;
    let mut reference_last = false;
    // `-d`'s parsed instant, and whether it came after the last `-t`.
    let mut date_seconds: Option<i64> = None;
    let mut date_last = false;

    // getopt "acd:fhmr:t:" — options consume until the first bare operand.
    let mut i = 1;
    while i < args.len() {
        let a = args[i];
        if a == "--" {
            i += 1;
            break;
        }
        if a.len() >= 2 && a.starts_with('-') {
            let bytes = a.as_bytes();
            let mut k = 1;
            while k < bytes.len() {
                match bytes[k] {
                    b'a' => aflag = true,
                    b'c' => cflag = true,
                    b'f' => {} // no-op, kept for SYNOPSIS compatibility
                    b'm' => mflag = true,
                    // `-h` (`touch.c:108-109`, `:133-139`): touch the
                    // symlink itself — `lstat` for the exists-check and the
                    // `AT_SYMLINK_NOFOLLOW` form for the change, with
                    // creation suppressed (`cflag`).
                    b'h' => hflag = true,
                    // `case 'd': timeset = 1; stime_arg0(optarg, ts);`
                    // (`touch.c:102-105`): libutil's free-form parser, whose
                    // UTC answer becomes both timestamps.
                    b'd' => {
                        let text = {
                            let rest = &a[k + 1..];
                            if !rest.is_empty() {
                                rest
                            } else {
                                i += 1;
                                match args.get(i) {
                                    Some(s) => *s,
                                    None => usage(),
                                }
                            }
                        };
                        let now = gettimeofday().map(|(sec, _)| sec).unwrap_or(0);
                        match minix_fileops::parsedate::parsedate(text, now) {
                            // `touch.c:216-218` treats `-1` as unparsable even
                            // when the parser really meant that instant.
                            Some(sec) if sec != -1 => date_seconds = Some(sec),
                            _ => {
                                support::warn(
                                    format!("touch: Could not parse `{text}'\n").as_bytes(),
                                );
                                support::terminate(1);
                            }
                        }
                        timeset = true;
                        reference_last = false;
                        date_last = true;
                        k = bytes.len();
                    }
                    b'r' => {
                        // `case 'r': timeset = 1; stime_file(optarg, ts);`
                        // (`touch.c:114-116`): the reference is stated **inside
                        // the option loop**, so an unstatable one ends the run
                        // with C's three-part `err(1, "%s", fname)` before any
                        // operand is created or touched. Access comes from
                        // `st_atimespec`, modification from `st_mtimespec`.
                        let name = {
                            let rest = &a[k + 1..];
                            if !rest.is_empty() {
                                rest
                            } else {
                                i += 1;
                                match args.get(i) {
                                    Some(s) => *s,
                                    None => usage(),
                                }
                            }
                        };
                        let mut sb: Stat = unsafe { core::mem::zeroed() };
                        match stat(name, &mut sb) {
                            Ok(_) => {
                                reference = Some([
                                    (sb.st_atim.tv_sec, sb.st_atim.tv_nsec),
                                    (sb.st_mtim.tv_sec, sb.st_mtim.tv_nsec),
                                ]);
                            }
                            Err(code) => {
                                support::warn_line(prog, name, code);
                                support::terminate(1);
                            }
                        }
                        timeset = true;
                        reference_last = true;
                        k = bytes.len();
                    }
                    b't' => {
                        let rest = &a[k + 1..];
                        spec = if !rest.is_empty() {
                            rest
                        } else {
                            i += 1;
                            match args.get(i) {
                                Some(s) => *s,
                                None => usage(),
                            }
                        };
                        timeset = true;
                        reference_last = false;
                        date_last = false;
                        k = bytes.len();
                    }
                    _ => usage(),
                }
                k += 1;
            }
            i += 1;
        } else if a.starts_with("--") {
            // Not a long-option parser: C's `getopt` sees the second `-` as
            // an unknown option letter and answers usage.
            usage();
        } else {
            break;
        }
    }

    let mut operands: Vec<&str> = args[i..].to_vec();
    if operands.is_empty() {
        usage();
    }

    // The obsolete form: a bare 8/10-digit first operand, only when a time is
    // not already set and at least two operands follow (`touch.c`).
    if !timeset && operands.len() > 1 && minix_fileops::touch::is_obsolete_time_spec(operands[0]) {
        spec = operands[0];
        operands.remove(0);
        timeset = true;
    }

    // Resolve the target time. C starts from clock_gettime(CLOCK_REALTIME) for
    // both fields; here gettimeofday gives (sec, usec) which becomes (sec,
    // usec*1000) nanoseconds, and `-t` overrides both to the parsed second.
    let (now_sec, now_usec) = gettimeofday().unwrap_or((0, 0));
    let now_ts = (now_sec, now_usec * 1000);
    // `-t`/obsolete decode to a second, `-r` to the pair the reference's `stat`
    // handed back, and which of them owns the times is C's last-writer-wins rule
    // — a pure function, so the rule has a gate even though writing the times
    // needs the real root the whole write class of this crate is gated on.
    let parsed = if timeset && !reference_last {
        if date_last {
            date_seconds
        } else {
            match minix_fileops::touch::parse_time_spec(now_sec, spec) {
                Ok(sec) => Some(sec),
                Err(_) => {
                    support::warn(
                        b"touch: out of range or illegal time specification: [[CC]YY]MMDDhhmm[.SS]\n",
                    );
                    support::terminate(1);
                }
            }
        }
    } else {
        None
    };
    let target = minix_fileops::touch::chosen_times(reference, reference_last, parsed, now_ts);

    let mut errors = 0;
    for path in &operands {
        let mut sb: Stat = unsafe { core::mem::zeroed() };
        let mut ts = target;
        // `-h` asks about the link itself (`touch.c:133-139`).
        let existed = if hflag {
            lstat(path, &mut sb)
        } else {
            stat(path, &mut sb)
        };
        if existed.is_err() {
            // `cflag` (`touch.c:162-178`): with `-h` a missing path is
            // skipped, never created.
            if hflag {
                continue;
            }
            if cflag {
                continue;
            }
            // Create the file. A freshly created file already carries the
            // current time, so when no time was requested we are done.
            match open(path, O_WRONLY | O_CREAT, DEFFILEMODE) {
                Ok(fd) => {
                    #[allow(clippy::let_underscore_must_use)]
                    let _ = fstat(fd, &mut sb);
                    #[allow(clippy::let_underscore_must_use)]
                    let _ = close(fd);
                }
                Err(_) => {
                    support::warn(format!("touch: {path}\n").as_bytes());
                    errors = 1;
                    continue;
                }
            }
            if !timeset {
                continue;
            }
        } else {
            // The file exists: a field whose flag is off keeps its current time.
            if !aflag {
                ts[0] = (sb.st_atim.tv_sec, sb.st_atim.tv_nsec);
            }
            if !mflag {
                ts[1] = (sb.st_mtim.tv_sec, sb.st_mtim.tv_nsec);
            }
        }

        let changed = if hflag {
            utimensat(AT_FDCWD, path, Some(ts), AT_SYMLINK_NOFOLLOW)
        } else {
            utimens(path, Some(ts))
        };
        if changed.is_ok() {
            continue;
        }
        if timeset {
            errors = 1;
            support::warn(format!("touch: {path}\n").as_bytes());
        }
        // System V / POSIX fallback: a NULL time set means "now", and needs only
        // write permission rather than ownership.
        let now_changed = if hflag {
            utimensat(AT_FDCWD, path, None, AT_SYMLINK_NOFOLLOW)
        } else {
            utimens(path, None)
        };
        if now_changed.is_ok() {
            continue;
        }
        errors = 1;
        support::warn(format!("touch: {path}\n").as_bytes());
    }

    support::terminate(errors)
}

#[cfg(all(not(test), target_os = "none"))]
#[unsafe(no_mangle)]
extern "Rust" fn main() -> i32 {
    run()
}

#[cfg(any(test, not(target_os = "none")))]
fn main() {
    run()
}
