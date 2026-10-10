//! Minix-RS `pwd` — the doing half over `minix_fileops::pwd`.
//!
//! Ground truth: `minix3/bin/pwd/pwd.c`. The physical path comes straight from
//! `getcwd`; the `-L` (logical) path first trusts a well-formed `$PWD` (the pure
//! string gate is [`minix_fileops::pwd::env_pwd_usable`]) that resolves to the
//! same `st_dev`/`st_ino` as `.`, and falls back to `getcwd` otherwise. Reads
//! only (no filesystem writes), so unlike the write-side commands it is not the
//! writable-root gate — the real run just needs the process's cwd.
//!
//! Wired: `pwd [-L|-P]` (default `-P` physical, matching C's `lFlag = 0`). Any
//! leftover operand or unrecognised flag is a usage error.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::string::String;
use alloc::vec::Vec;
use minix_fileops::pwd::env_pwd_usable;
use minix_sys::{Stat, getcwd, stat};

fn usage() -> ! {
    support::warn(b"usage: pwd [-L | -P]\n");
    support::terminate(1);
}

fn stat_dot() -> Option<(u64, u64)> {
    let mut st: Stat = unsafe { core::mem::zeroed() };
    stat(".", &mut st).ok().map(|_| (st.st_dev, st.st_ino))
}

/// The value of `PWD` in the environment, if present.
fn env_pwd(env: &[Vec<u8>]) -> Option<&[u8]> {
    const PREFIX: &[u8] = b"PWD=";
    env.iter().find_map(|e| e.strip_prefix(PREFIX))
}

/// The whole program body; both `main` forms call it and it never returns.
fn run() -> ! {
    let argv: Vec<String> = support::args();
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();

    let mut logical = false;
    let mut i = 1;
    while i < args.len() {
        let a = args[i];
        if a == "--" {
            i += 1;
            break;
        }
        if a.starts_with('-') && a.len() > 1 {
            match &a[1..] {
                "L" => logical = true,
                "P" => logical = false,
                _ => usage(),
            }
            i += 1;
            continue;
        }
        break;
    }
    if i != args.len() {
        usage();
    }

    let mut out: Vec<u8> = Vec::new();
    if logical {
        let env = support::envs();
        if let Some(pwd) = env_pwd(&env).filter(|p| env_pwd_usable(p)) {
            let pwd_str = String::from_utf8_lossy(pwd).into_owned();
            if let Some((dot_dev, dot_ino)) = stat_dot() {
                let mut st: Stat = unsafe { core::mem::zeroed() };
                if stat(&pwd_str, &mut st).is_ok() && st.st_dev == dot_dev && st.st_ino == dot_ino {
                    out.extend_from_slice(pwd);
                    out.push(b'\n');
                    support::emit(&out);
                    support::terminate(0);
                }
            }
        }
    }

    let mut buf = [0u8; 4096];
    match getcwd(&mut buf) {
        Ok(path) => {
            out.extend_from_slice(path);
            out.push(b'\n');
            support::emit(&out);
            support::terminate(0);
        }
        Err(_) => {
            support::warn(b"pwd: cannot get current directory\n");
            support::terminate(1);
        }
    }
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
