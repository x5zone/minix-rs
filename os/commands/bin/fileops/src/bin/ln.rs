//! Minix-RS `ln` — the doing half over `minix_fileops::ln`.
//!
//! Ground truth: `minix3/bin/ln/ln.c`. The option/operand bookkeeping lives in
//! `minix_fileops::ln`; this program supplies the filesystem side — `main`'s
//! argument shape plus `linkit`'s `lstat`/`stat`/`unlink`/`link`/`symlink`
//! sequence and the `-i` prompt — reproducing `ln.c:100-210` behaviour.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::format;
use alloc::string::String;
use minix_fileops::ln::{LnFlags, OperandPlan, basename, plan_operands};
use minix_sys::{Stat, lstat, read, stat, symlink, unlink};

// POSIX file-type bits, matching C `sys/sys/stat.h`: `_S_IFMT 0170000`,
// `_S_IFDIR 0040000`, `_S_IFLNK 0120000`.
const S_IFMT: u32 = 0o170000;
const S_IFDIR: u32 = 0o040000;
const S_IFLNK: u32 = 0o120000;

fn s_isdir(m: u32) -> bool {
    m & S_IFMT == S_IFDIR
}
fn s_islnk(m: u32) -> bool {
    m & S_IFMT == S_IFLNK
}

fn zeroed() -> Stat {
    // SAFETY: Stat is a plain byte struct of integers; all-zero is a valid
    // bit pattern that `stat`/`lstat` overwrite on success.
    unsafe { core::mem::zeroed() }
}

fn warn_err(path: &str) {
    support::warn(format!("ln: {path}\n").as_bytes());
}

/// C `linkit`: create one link from `target` at `source`, honouring the dir
/// append, force/interactive, and `-s`. Returns true on failure (exit code).
fn linkit(target: &str, source_in: &str, isdir: bool, f: &LnFlags) -> bool {
    // Hard links require the target to exist (ln.c:151-155).
    if !f.symbolic {
        let mut st = zeroed();
        if stat(target, &mut st).is_err() {
            warn_err(target);
            return true;
        }
    }

    // If the destination is a directory, append the target's basename.
    let mut dir_case = isdir;
    if !dir_case {
        let mut sb = zeroed();
        if lstat(source_in, &mut sb).is_ok() && s_isdir(sb.st_mode) {
            dir_case = true;
        }
    }
    if !dir_case && !f.no_follow {
        let mut sb = zeroed();
        if stat(source_in, &mut sb).is_ok() && s_isdir(sb.st_mode) {
            dir_case = true;
        }
    }
    let mut source = String::from(source_in);
    if dir_case {
        source = format!("{}/{}", source_in, basename(target));
    }

    // Does the destination already exist? (lstat, so a dangling symlink counts.)
    let mut sb = zeroed();
    let exists = lstat(&source, &mut sb).is_ok();

    if exists && f.force {
        if unlink(&source).is_err() {
            warn_err(&source);
            return true;
        }
    } else if exists && f.interactive {
        support::warn(format!("replace {} ? ", source).as_bytes());
        let mut buf = [0u8; 64];
        let n = read(0, &mut buf).unwrap_or(0);
        let yes = n > 0 && (buf[0] == b'y' || buf[0] == b'Y');
        if !yes {
            support::warn(b"not replaced\n");
            return true;
        }
        if unlink(&source).is_err() {
            warn_err(&source);
            return true;
        }
    }

    let made = if f.symbolic {
        symlink(target, &source)
    } else {
        // `link` is provided by minix-sys as the hard-link syscall.
        minix_sys::link(target, &source)
    };
    if made.is_err() {
        warn_err(&source);
        return true;
    }

    if f.verbose {
        let arrow = if f.symbolic { '-' } else { '=' };
        let line = format!("{} {} {}\n", source, arrow, target);
        support::emit(line.as_bytes());
    }
    false
}

fn run() -> ! {
    let argv: alloc::vec::Vec<String> = support::args();
    let args: alloc::vec::Vec<&str> = argv.iter().map(String::as_str).collect();

    let (flags, operands) = match LnFlags::parse(&args[1.min(args.len())..]) {
        Some(v) => v,
        None => {
            usage();
        }
    };

    match plan_operands(&operands) {
        OperandPlan::Usage => usage(),
        OperandPlan::ImplicitCwd { target } => {
            // ln target  ->  link into "."
            if linkit(target, ".", true, &flags) {
                support::terminate(1);
            }
            support::terminate(0);
        }
        OperandPlan::Pair { target, source } => {
            if linkit(target, source, false, &flags) {
                support::terminate(1);
            }
            support::terminate(0);
        }
        OperandPlan::ToDirectory { targets, dir } => {
            // ln target… directory  (ln.c:114-129)
            let mut sb = zeroed();
            if flags.no_follow && lstat(dir, &mut sb).is_ok() && s_islnk(sb.st_mode) {
                warn_err(dir);
                support::terminate(1);
            }
            let mut sb = zeroed();
            if stat(dir, &mut sb).is_err() {
                warn_err(dir);
                support::terminate(1);
            }
            if !s_isdir(sb.st_mode) {
                usage();
            }
            let mut exitval = false;
            for t in &targets {
                exitval |= linkit(t, dir, false, &flags);
            }
            support::terminate(if exitval { 1 } else { 0 });
        }
    }
}

fn usage() -> ! {
    support::warn(b"usage:\tln [-fhinsv] file1 file2\n\tln [-fhinsv] file ... directory\n");
    support::terminate(1);
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
