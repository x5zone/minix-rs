//! Minix-RS `mkdir` — the doing half over `minix_fileops::mkdir` and
//! `minix_fileops::mode`.
//!
//! Ground truth: `minix3/bin/mkdir/mkdir.c`. Options `-p` (create parents) and
//! `-m mode` (an octal or symbolic mode via `setmode`/`getmode`). Without `-p`
//! each operand is created with `mkdir(2)`; a created directory whose mode
//! carries the sticky/setuid/setgid bits is re-applied with `chmod`. With `-p`
//! every missing component is created outermost first (intermediates get
//! `u+wx`), and a component that cannot be created is `stat`ed: "already a
//! directory" continues the walk, "exists but is not a directory" stops with a
//! failure. The pure walk, mode, and fix-up decisions live in
//! [`minix_fileops::mkdir`]; this shell owns the system calls and the exit code.
//!
//! A write command: creating directories needs a writable root, so the real
//! run is gated with the rest of the write-class commands (`C-57`/E5). The
//! decision half and the operand walk are host/compile verified here.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use minix_sys::{chmod, mkdir, stat, Stat};

fn usage() -> ! {
    support::warn(b"usage: mkdir [-p] [-m mode] dirname ...\n");
    support::terminate(1);
}

/// `sys/stat.h` file-type mask and directory bits.
const S_IFMT: u32 = 0o170_000;
const S_IFDIR: u32 = 0o040_000;

fn is_dir(st: &Stat) -> bool {
    st.st_mode & S_IFMT == S_IFDIR
}

/// `mkdir.c` non-`-p` branch: create one operand, then `chmod` when the mode
/// carries bits outside the nine permission bits. Returns whether it succeeded.
fn create_one(path: &str, mode: u16) -> bool {
    if mkdir(path, mode as u32).is_err() {
        support::warn(format!("mkdir: {path}\n").as_bytes());
        return false;
    }
    if minix_fileops::mkdir::needs_extra_bit_fixup(mode) && chmod(path, mode as u32).is_err() {
        support::warn(format!("mkdir: {path}\n").as_bytes());
        return false;
    }
    true
}

/// `mkdir.c` `mkpath`: create every component outermost first. Mirrors C's
/// "mkdir failed → stat to tell a pre-existing directory from a real error"
/// rule; the walk stops on the first component that neither creates nor is an
/// existing directory.
fn create_parents(path: &str, mode: u16, dir_mode: u16) -> bool {
    let mut ok = true;
    minix_fileops::mkdir::for_each_parent(path, &mut |prefix, is_last| {
        if !ok {
            return;
        }
        let m = if is_last { mode } else { dir_mode };
        if mkdir(prefix, m as u32).is_ok() {
            if is_last
                && minix_fileops::mkdir::needs_extra_bit_fixup(mode)
                && chmod(prefix, mode as u32).is_err()
            {
                support::warn(format!("mkdir: {prefix}\n").as_bytes());
                ok = false;
            }
            return;
        }
        // mkdir failed: was it already a directory (keep walking) or a real
        // error (stop)? C uses the mkdir error when stat also fails, and
        // ENOTDIR when the name exists but is not a directory.
        let mut st: Stat = unsafe { core::mem::zeroed() };
        if stat(prefix, &mut st).is_err() {
            support::warn(format!("mkdir: {prefix}\n").as_bytes());
            ok = false;
        } else if !is_dir(&st) {
            support::warn(format!("mkdir: {prefix}: Not a directory\n").as_bytes());
            ok = false;
        }
        // else: an existing directory — nothing to do for this component.
    });
    ok
}

/// The whole program body; both `main` forms call it and it never returns.
fn run() -> ! {
    let argv: Vec<String> = support::args();
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();

    let mut pflag = false;
    let mut mode = minix_fileops::mkdir::DEFAULT_MODE;
    let mut i = 1;
    while i < args.len() {
        let a = args[i];
        if a == "--" {
            i += 1;
            break;
        }
        // A lone "-" or a bare operand ends option scanning.
        if a.len() < 2 || !a.starts_with('-') {
            break;
        }
        let bytes = a.as_bytes();
        let mut k = 1;
        while k < bytes.len() {
            match bytes[k] {
                b'p' => pflag = true,
                b'm' => {
                    let rest = &a[k + 1..];
                    let spec = if !rest.is_empty() {
                        rest
                    } else {
                        i += 1;
                        match args.get(i) {
                            Some(s) => *s,
                            None => usage(),
                        }
                    };
                    mode = minix_fileops::mkdir::parse_mode_spec(spec).unwrap_or_else(|_| {
                        support::warn(format!("mkdir: Cannot set file mode `{spec}'\n").as_bytes());
                        support::terminate(1);
                    });
                    k = bytes.len(); // the remainder was the mode value
                }
                _ => usage(),
            }
            k += 1;
        }
        i += 1;
    }

    if i >= args.len() {
        usage();
    }
    let dir_mode = minix_fileops::mkdir::intermediate_mode(mode);

    let mut exitval = 0;
    for operand in &args[i..] {
        let done = if pflag {
            create_parents(operand, mode, dir_mode)
        } else {
            create_one(operand, mode)
        };
        if !done {
            exitval = 1;
        }
    }
    support::terminate(exitval)
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
