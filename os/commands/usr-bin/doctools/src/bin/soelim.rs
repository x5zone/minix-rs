//! Minix-RS `soelim` — the doing half over `minix_doctools::soelim`.
//!
//! Ground truth: `minix3/usr.bin/soelim/soelim.c`. The `.so` scan is the
//! crate's faithful byte machine; this program supplies the filesystem side
//! that `soelim.c` puts in `openpath`/`initpath`/`addpath`: the option `-I`
//! collects a directory search list (seeded with `.`), each non-option
//! operand (or standard input when there are none) is read and expanded, and
//! every `.so <name>` is resolved against the list — absolute names open
//! directly. Output is written to standard output.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::string::String;
use alloc::vec::Vec;
use minix_doctools::soelim::expand;
use minix_sys::{Fd, close, open, read};

/// Drain an already-open descriptor to EOF. Shared by files and stdin.
fn read_fd(fd: Fd) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match read(fd, &mut chunk) {
            Ok(0) => break,
            Ok(n) => out.extend_from_slice(&chunk[..n]),
            Err(_) => return None,
        }
    }
    Some(out)
}

/// Read a whole file's bytes, or `None` if it cannot be opened.
fn read_all(path: &str) -> Option<Vec<u8>> {
    let fd: Fd = open(path, 0, 0).ok()?;
    let result = read_fd(fd);
    let _ = close(fd);
    result
}

/// Resolve `.so <name>` against the search list (`soelim.c:openpath`):
/// absolute names (or an empty list) open directly, otherwise each directory
/// is prefixed in turn (empty dir means the name alone) and the first that
/// opens wins.
fn open_in_paths(dirs: &[String], name: &str) -> Option<Vec<u8>> {
    if name.starts_with('/') || dirs.is_empty() {
        return read_all(name);
    }
    for dir in dirs {
        let full = if dir.is_empty() {
            String::from(name)
        } else {
            let mut s = String::from(dir.as_str());
            s.push('/');
            s.push_str(name);
            s
        };
        if let Some(bytes) = read_all(&full) {
            return Some(bytes);
        }
    }
    None
}

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();

    // getopt(argc, argv, "I:"): `-I dir` collects a search directory; any
    // other option is a usage error (soelim.c:150-153). The list is seeded
    // with `.` (soelim.c:142 addpath(&p, ".")), BEFORE `-I` entries.
    let mut dirs: Vec<String> = alloc::vec![String::from(".")];
    let mut operands: Vec<&str> = Vec::new();
    let mut options_done = false;
    let mut i = 1;
    while i < args.len() {
        let a = args[i];
        if options_done {
            operands.push(a);
            i += 1;
            continue;
        }
        if a == "--" {
            options_done = true;
            i += 1;
            continue;
        }
        if a.len() >= 2 && a.starts_with('-') {
            let letter = a.as_bytes()[1];
            let attached = &a[2..];
            if letter == b'I' {
                let dir = if !attached.is_empty() {
                    i += 1;
                    attached
                } else if i + 1 < args.len() {
                    i += 2;
                    args[i - 1]
                } else {
                    support::warn(b"usage: soelim [-I<dir>] [files...]\n");
                    support::terminate(1);
                };
                dirs.push(String::from(dir));
                continue;
            }
            support::warn(b"usage: soelim [-I<dir>] [files...]\n");
            support::terminate(1);
        }
        operands.push(a);
        i += 1;
    }

    // soelim.c:156-166: no operands -> stdin; else each operand in order.
    let sources: Vec<&str> = if operands.is_empty() {
        alloc::vec!["-"]
    } else {
        operands
    };

    let mut out: Vec<u8> = Vec::new();
    for &src in &sources {
        // A `-` operand (and the no-operand default) reads standard input
        // (fd 0), exactly as `soelim.c`'s `soee = stdin` for STDIN_NAME.
        let input = if src == "-" {
            read_fd(0).unwrap_or_default()
        } else {
            match open_in_paths(&dirs, src) {
                Some(bytes) => bytes,
                None => {
                    // soelim.c:173 warn("Cannot open `%s'") and return -1; the
                    // operand contributes nothing, exit status set by main.
                    let mut msg = String::from("soelim: cannot open ");
                    msg.push_str(src);
                    msg.push('\n');
                    support::warn(msg.as_bytes());
                    support::terminate(1);
                }
            }
        };
        expand(&input, &mut out, &mut |name| open_in_paths(&dirs, name));
    }

    support::emit(&out);
    support::terminate(0);
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
