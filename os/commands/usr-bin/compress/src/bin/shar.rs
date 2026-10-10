//! Minix-RS `shar` — the doing half over `minix_compress::shar`.
//!
//! Ground truth: `minix3/usr.bin/shar/shar.sh` (a shell script). `shar`
//! writes a self-extracting `sh` archive to standard output; the whole
//! byte-level shape of that output lives in the library [`generate`], so this
//! program only classifies each operand and reads its contents:
//!
//! - no operands → the script prints `usage: shar file ...` (through `echo`,
//!   i.e. on standard output, not stderr) and exits 1 (`shar.sh:35-38`);
//! - a directory operand (`[ -d $i ]` true) → a `mkdir -p` stanza;
//! - anything else → a here-doc stanza carrying the file body (`shar.sh:59-67`).
//!
//! The `#`-tab index and the trailing `exit` / blank line are emitted by the
//! library; here the loop is just `stat` + (for files) `open`/`read`.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::string::String;
use alloc::vec::Vec;
use minix_compress::shar::{generate, SharEntry};
use minix_sys::{read, Fd, Stat};

// File-type bits (`minix3/sys/sys/stat.h`); `shar.sh`'s `[ -d $i ]` test is
// exactly `st_mode & S_IFMT == S_IFDIR`.
const S_IFMT: u32 = 0o170_000;
const S_IFDIR: u32 = 0o040_000;

/// Read a whole descriptor into a vector (`read` until EOF).
fn slurp(fd: Fd) -> Vec<u8> {
    let mut data = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match read(fd, &mut chunk) {
            Ok(0) => return data,
            Ok(n) => data.extend_from_slice(&chunk[..n]),
            Err(_) => return data,
        }
    }
}

/// The whole program body; both `main` forms call it and it never returns.
fn run() -> ! {
    let argv: Vec<String> = support::args();
    let operands: Vec<&str> = argv.iter().skip(1).map(String::as_str).collect();

    if operands.is_empty() {
        // `shar.sh` echoes the usage line on standard output, then exits 1.
        support::emit(b"usage: shar file ...\n");
        support::terminate(1);
    }

    // Classify each operand the way the script's `for` loop branches on
    // `[ -d $i ]`: an existing directory, or — for everything else, including
    // a name that cannot be opened — a file here-doc whose body is the bytes
    // the script would have fed to `sed 's/^/X/'` (nothing, when unreadable).
    // Names are owned here so the borrowed `SharEntry` bodies can point at
    // the gathered contents in a single pass.
    struct Classified {
        name: String,
        is_dir: bool,
        body: Vec<u8>,
    }
    let mut classified: Vec<Classified> = Vec::new();
    for operand in &operands {
        let mut st: Stat = unsafe { core::mem::zeroed() };
        let is_dir = minix_sys::stat(operand, &mut st)
            .map(|_| st.st_mode & S_IFMT == S_IFDIR)
            .unwrap_or(false);
        let body = if is_dir {
            Vec::new()
        } else {
            match minix_sys::open(operand, 0, 0) {
                Ok(fd) => {
                    let data = slurp(fd);
                    let _ = minix_sys::close(fd);
                    data
                }
                Err(_) => {
                    // The script's `sed` would complain to stderr and emit an
                    // empty body; mirror that observable shape.
                    let mut line = String::from("shar: cannot read ");
                    line.push_str(operand);
                    line.push('\n');
                    support::warn(line.as_bytes());
                    Vec::new()
                }
            }
        };
        classified.push(Classified {
            name: String::from(*operand),
            is_dir,
            body,
        });
    }

    let entries: Vec<SharEntry> = classified
        .iter()
        .map(|c| {
            if c.is_dir {
                SharEntry::Dir(&c.name)
            } else {
                SharEntry::File {
                    name: &c.name,
                    body: &c.body,
                }
            }
        })
        .collect();

    let archive = generate(&entries);
    support::emit(&archive);
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
