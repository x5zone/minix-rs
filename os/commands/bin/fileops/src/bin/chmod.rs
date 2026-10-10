//! Minix-RS `chmod` — the doing half over `minix_fileops::mode`.
//!
//! Ground truth: `minix3/bin/chmod/chmod.c`. The mode-text→bits translation
//! (BSD `setmode`/`getmode`) already lives in the library as
//! [`minix_fileops::mode::apply_mode`], octal and symbolic alike; this program
//! supplies the missing syscall leg: for each operand it reads the file's
//! current mode (`stat`), folds the spec onto it (`apply_mode`), and writes the
//! result (`chmod`), which is exactly C's `getmode(set, st_mode)` then
//! `change_mode(...)` (`chmod.c:215-216`).
//!
//! Wired: `chmod [-f] [-v] mode file ...`. A bare `-[rwxstugoX]` (the entire
//! argument) is the *mode*, not a flag — reproduced from C's `--optind`
//! correction (`chmod.c:127-133`), so `chmod -r file` drops the read bit.
//! `-f` continues past errors (only sets the exit status), `-v` prints each
//! applied mode. Real-machine effect is the writable-root gate (`C-57`/E5), as
//! for every write-side command; the parser is host-tested in `mode.rs`.
//!
//! Declared residual: `-R` (recursive via `fts`), `-h`/`-H`/`-L`/`-P` (symlink
//! traversal needing `lchmod`/`fts`) and `--reference` are reported as "not
//! wired" — those hinge on unwrapped facilities, not a semantics choice.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::string::String;
use alloc::vec::Vec;
use minix_fileops::mode::apply_mode;
use minix_sys::{Stat, chmod, stat};

fn warn(bytes: &[u8]) {
    let mut line = String::from("chmod: ");
    line.push_str(core::str::from_utf8(bytes).unwrap_or("<invalid>"));
    line.push('\n');
    support::warn(line.as_bytes());
}

fn usage() -> ! {
    support::warn(b"usage: chmod [-f] [-v] mode file ...\n");
    support::terminate(1);
}

/// Is `arg` a bare `-[perm]` *mode* string (e.g. `-r`) rather than an option?
/// C treats `-X` for any single permission letter as the mode operand.
fn is_bare_mode(arg: &str) -> bool {
    let b = arg.as_bytes();
    b.len() == 2 && b[0] == b'-' && matches!(b[1], b'r' | b'w' | b'x' | b's' | b't' | b'u' | b'g' | b'o')
}

fn apply_one(path: &str, spec: &str) -> Result<(), ()> {
    let mut st: Stat = unsafe { core::mem::zeroed() };
    stat(path, &mut st).map_err(|_| ())?;
    let base = (st.st_mode & 0o7777) as u16;
    let mode = apply_mode(base, spec).map_err(|_| ())?;
    chmod(path, mode as u32).map_err(|_| ())
}

/// The whole program body; both `main` forms call it and it never returns.
fn run() -> ! {
    let argv: Vec<String> = support::args();
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();

    let mut force = false;
    let mut verbose = false;
    let mut i = 1;
    while i < args.len() {
        let a = args[i];
        if a == "--" {
            i += 1;
            break;
        }
        if a.starts_with('-') && a.len() > 1 && !is_bare_mode(a) {
            match &a[1..] {
                "f" => force = true,
                "v" => verbose = true,
                "R" | "h" | "H" | "L" | "P" => {
                    warn(b"recursive/symlink modes not wired (fts/lchmod)");
                    support::terminate(1);
                }
                _ => usage(),
            }
            i += 1;
            continue;
        }
        break;
    }

    // Operands: the mode string, then one or more files.
    if args.len() - i < 2 {
        usage();
    }
    let spec = args[i];
    let files = &args[i + 1..];

    let mut status = 0;
    for path in files {
        match apply_one(path, spec) {
            Ok(()) => {
                if verbose {
                    let mut msg = String::from(*path);
                    msg.push_str(" mode set to ");
                    msg.push_str(spec);
                    msg.push('\n');
                    support::emit(msg.as_bytes());
                }
            }
            Err(()) => {
                if !force {
                    let mut msg = String::from(*path);
                    msg.push_str(": cannot change mode");
                    warn(msg.as_bytes());
                    status = 1;
                }
            }
        }
    }
    support::terminate(status);
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
