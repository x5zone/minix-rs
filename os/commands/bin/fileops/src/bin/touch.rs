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
//! Declared not-wired, because they need infrastructure this tree does not
//! provide: `-d` (a free-form date string, C `parsedate`), `-r` (copy the times
//! of a reference file), `-h` (`lutimens`, which needs the symlink-not-following
//! face), and the `--date` / `--reference` long options. Each reports and exits
//! 1 rather than silently diverging. Real file creation and time-setting need a
//! writable root, so the run is gated with the write-class commands (`E5`).

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use minix_sys::{close, fstat, gettimeofday, open, stat, utimens, Stat};

fn usage() -> ! {
    support::warn(b"usage: touch [-acfhm] [-d date] [-r file] [-t time] file ...\n");
    support::terminate(1);
}

fn not_wired(flag: &str) -> ! {
    support::warn(format!("touch: {flag} not wired\n").as_bytes());
    support::terminate(1);
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

    let mut aflag = false;
    let mut mflag = false;
    let mut cflag = false;
    let mut timeset = false;
    let mut spec: &str = "";

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
                    b'h' => not_wired("-h"),
                    b'd' => not_wired("-d"),
                    b'r' => not_wired("-r"),
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
                        k = bytes.len();
                    }
                    _ => usage(),
                }
                k += 1;
            }
            i += 1;
        } else if a.starts_with("--") {
            not_wired(a);
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
    let target = if timeset {
        match minix_fileops::touch::parse_time_spec(now_sec, spec) {
            Ok(sec) => (sec, 0),
            Err(_) => {
                support::warn(
                    b"touch: out of range or illegal time specification: [[CC]YY]MMDDhhmm[.SS]\n",
                );
                support::terminate(1);
            }
        }
    } else {
        now_ts
    };

    let mut errors = 0;
    for path in &operands {
        let mut sb: Stat = unsafe { core::mem::zeroed() };
        let mut ts = [target, target];
        if stat(path, &mut sb).is_err() {
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

        if utimens(path, Some(ts)).is_ok() {
            continue;
        }
        if timeset {
            errors = 1;
            support::warn(format!("touch: {path}\n").as_bytes());
        }
        // System V / POSIX fallback: a NULL time set means "now", and needs only
        // write permission rather than ownership.
        if utimens(path, None).is_ok() {
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
