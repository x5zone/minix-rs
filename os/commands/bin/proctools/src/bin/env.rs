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
//! Declared residual: `env name=value command …` execs the command under the
//! modified environment in C. The exec delivery chain is not wired in this
//! tree (the same NS5 gate that blocks `cat`/`xargs` running children), so a
//! command operand is reported as "not wired" and exits 1 rather than silently
//! printing — an exec that does not happen must not masquerade as success.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::string::String;
use alloc::vec::Vec;
use minix_proctools::env::{build_env, command_start, parse_flags};

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

    // `env … command` needs execvp under the modified environment; the exec
    // delivery chain is a declared residual (NS5), so refuse rather than print.
    if !command.is_empty() {
        support::warn(b"env: running a command is not wired (exec delivery chain, NS5)\n");
        support::terminate(1);
    }

    // -i clears the inherited environment before the assignments are applied.
    let base: Vec<Vec<u8>> = if clean { Vec::new() } else { support::envs() };
    let base_refs: Vec<&[u8]> = base.iter().map(|e| e.as_slice()).collect();

    let merged = build_env(&base_refs, assignments);
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
