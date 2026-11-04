//! Minix-RS `env` — the doing half over `minix_proctools::env`.
//!
//! Ground truth: `minix3/usr.bin/env/env.c`. The option scan, the operand
//! split, and the `putenv` merge live in `minix_proctools::env`; this program
//! supplies the system legs: read the real environment (`envs()`), then either
//! print it (`env.c:87-88`) or — when a command follows the assignments — exec
//! it (`env.c:78-85`).
//!
//! Wired: `env [-i] [name=value ...]` with no command prints the (possibly
//! cleaned, possibly amended) environment, one `NAME=VALUE` per line, and
//! exits 0. This is read-environment + write-stdout only, so like `printenv`
//! it is not gated on the writable-root E5 check.
//!
//! `env name=value command …` execs the command under the modified
//! environment (C `env.c:78-85` calls `execvp`): the search and frame
//! submission ride `minix-shell`'s `exec_run` module, the same code `sh`
//! uses for external commands. A successful exec replaces this process;
//! a failed one answers C's `err` line and exit code (ENOENT -> 127,
//! anything else -> 126).

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::string::String;
use alloc::vec::Vec;
use minix_proctools::env::{build_env, command_start, parse_flags};
use minix_shell::exec_run::{DEFAULT_PATH, exec_search};
use minix_sys::Errno;

fn usage() -> ! {
    support::warn(b"usage: env [-i] [name=value ...] [command]\n");
    support::terminate(1);
}

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();
    let operands = &args[1.min(args.len())..];

    let (clean, opt_end) = match parse_flags(operands) {
        Some(v) => v,
        None => usage(),
    };
    let rest = &operands[opt_end..];
    let k = command_start(rest);
    let assignments = &rest[..k];
    let command = &rest[k..];

    // -i clears the inherited environment before the assignments are applied.
    let base: Vec<Vec<u8>> = if clean { Vec::new() } else { support::envs() };
    let base_refs: Vec<&[u8]> = base.iter().map(|e| e.as_slice()).collect();

    let merged = build_env(&base_refs, assignments);

    // C `env.c:78-85`: with a command operand, `execvp` it under the
    // modified environment (the merged list supplies the `PATH` the search
    // reads, and the child's whole environment). A successful exec never
    // returns; the failure path answers `env: <command>: <sentence>` with
    // ENOENT -> 127 and everything else -> 126.
    if !command.is_empty() {
        let argv: Vec<&str> = command.to_vec();
        let envp: Vec<String> = merged
            .iter()
            .map(|entry| String::from_utf8_lossy(entry).into_owned())
            .collect();
        let envp_refs: Vec<&str> = envp.iter().map(String::as_str).collect();
        let path = envp
            .iter()
            .find_map(|entry| entry.strip_prefix("PATH="))
            .or(Some(DEFAULT_PATH));
        let code = exec_search(argv[0], &argv, &envp_refs, path);
        // C's `err(3)`: program name, the command, that number's sentence.
        let errno = Errno::from_i32(code);
        let need = minix_sys::err_line("env", argv[0], errno, &mut []);
        let mut line = alloc::vec![0u8; need];
        let len = minix_sys::err_line("env", argv[0], errno, &mut line);
        support::warn(&line[..len.min(line.len())]);
        support::warn(b"\n");
        support::terminate(if code == minix_sys::errno::ENOENT {
            127
        } else {
            126
        });
    }

    let mut out: Vec<u8> = Vec::new();
    for entry in &merged {
        out.extend_from_slice(entry);
        out.push(b'\n');
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
