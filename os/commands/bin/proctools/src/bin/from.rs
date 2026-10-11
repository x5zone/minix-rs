//! Minix-RS `from` — the doing half over `minix_proctools::from`.
//!
//! Ground truth: `minix3/usr.bin/from/from.c`. The record scan and the BSD
//! sender matcher live in the library; this program parses `-f file` /
//! `-s sender`, resolves the mailbox path exactly as `from.c:94-114` does,
//! reads the file, and prints each matching record.
//!
//! Path resolution order (`from.c:87-114`): the `-f` flag wins; otherwise an
//! operand names the user and the mailbox is `/var/mail/<operand>`; otherwise
//! the `MAIL` environment variable is used verbatim; otherwise `LOGNAME` or
//! `USER` yields `/var/mail/<name>`. The final C fallback is `getpwuid` of the
//! real uid — no `getpwuid` wrapper exists in this tree, so when none of the
//! above resolve we report the error and exit 1 (declared unwired).
//!
//! The sender is lowercased once before the scan (`from.c:78-79`); the
//! matcher aborts at the first whitespace, so the sender must be the first
//! token of a `From ` line (see the library docs).

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use minix_proctools::from::FromRecords;
use minix_proctools::printenv::lookup;
use minix_sys::{Fd, close, open, read};

/// C `_PATH_MAILDIR` (`minix3/include/paths.h:80`).
const MAILDIR: &str = "/var/mail";

fn usage() -> ! {
    support::warn(b"usage: from [-f file] [-s sender] [user]\n");
    support::terminate(1);
}

/// Read every byte of `path`, exiting on failure like C's `err(1, ...)`.
fn read_mailbox(path: &str) -> Vec<u8> {
    let fd: Fd = match open(path, 0, 0) {
        Ok(fd) => fd,
        Err(_) => {
            support::warn(format!("from: can't read {path}\n").as_bytes());
            support::terminate(1);
        }
    };
    let mut image = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match read(fd, &mut chunk) {
            Ok(0) => break,
            Ok(n) => image.extend_from_slice(&chunk[..n]),
            Err(_) => {
                support::warn(format!("from: can't read {path}\n").as_bytes());
                support::terminate(1);
            }
        }
    }
    let _ = close(fd);
    image
}

/// Value of an environment variable among `env` (reusing the `printenv`
/// matcher), as an owned `String`.
fn env_value(env: &[&[u8]], name: &str) -> Option<String> {
    lookup(env, name.as_bytes()).map(|v| String::from_utf8_lossy(v).into_owned())
}

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();

    // getopt(argc, argv, "f:s:"): both `-f` and `-s` take a value, either
    // attached (`-ffile`) or as the following operand; any other option is a
    // usage error (`from.c:81-83` default arm). `--` ends option parsing.
    let mut file: Option<String> = None;
    let mut sender: Option<Vec<u8>> = None;
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
            let value = if !attached.is_empty() {
                Some(attached)
            } else if i + 1 < args.len() {
                i += 1;
                Some(args[i])
            } else {
                usage() // option requires an argument
            };
            match letter {
                b'f' => file = Some(value.unwrap().to_string()),
                b's' => {
                    sender = Some(
                        value
                            .unwrap()
                            .bytes()
                            .map(|c| c.to_ascii_lowercase())
                            .collect(),
                    )
                }
                _ => usage(),
            }
            i += 1;
            continue;
        }
        operands.push(a);
        i += 1;
    }

    // Resolve the mailbox path (`from.c:94-114`).
    let env: Vec<Vec<u8>> = support::envs();
    let env_refs: Vec<&[u8]> = env.iter().map(|e| e.as_slice()).collect();
    let login = env_value(&env_refs, "LOGNAME").or_else(|| env_value(&env_refs, "USER"));
    let path: String = if let Some(f) = file {
        f
    } else if let Some(user) = operands.first() {
        format!("{MAILDIR}/{user}")
    } else if let Some(mail) = env_value(&env_refs, "MAIL") {
        mail
    } else if let Some(name) = login {
        format!("{MAILDIR}/{name}")
    } else {
        support::warn(b"from: no password file entry for you\n");
        support::terminate(1);
    };

    let content = read_mailbox(&path);
    for record in FromRecords::new(&content, sender.as_deref()) {
        support::emit(record);
    }
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
