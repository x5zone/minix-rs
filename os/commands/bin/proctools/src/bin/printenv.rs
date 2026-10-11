//! Minix-RS `printenv` — the doing half over `minix_proctools::printenv`.
//!
//! Ground truth: `minix3/usr.bin/printenv/printenv.c`. The environment lookup
//! and formatting live in the library; this program reads the real environment
//! (`minix_rt::crt0::envs` via the `envs()` seam), rejects any option operand
//! (C's `getopt(argc, argv, "")` errors on the first `-`), then prints either
//! every `KEY=VALUE` line (no operand), one variable's value (one name), or a
//! usage/argument error. Exit status: 0 on success, 1 on usage error, a name
//! containing `=`, or an unset variable.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::string::String;
use alloc::vec::Vec;
use minix_proctools::printenv::{lookup, name_is_invalid};

fn fail(message: &str) -> ! {
    let mut line = String::from("printenv: ");
    line.push_str(message);
    line.push('\n');
    support::warn(line.as_bytes());
    support::terminate(1)
}

fn usage() -> ! {
    support::warn(b"usage: printenv [name]\n");
    support::terminate(1);
}

/// The whole program body; both `main` forms call it and it never returns.
fn run() -> ! {
    let argv: Vec<String> = support::args();
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();

    // getopt("", ...) stops at the first operand; a leading `-x` is a usage
    // error and `--` ends the option scan.
    let mut i = 1;
    if i < args.len() {
        let first = args[i];
        if first == "--" {
            i += 1;
        } else if first.starts_with('-') && first.len() > 1 {
            usage();
        }
    }
    let names: Vec<&str> = args[i..].to_vec();

    let env: Vec<Vec<u8>> = support::envs();
    let env_refs: Vec<&[u8]> = env.iter().map(|e| e.as_slice()).collect();

    if names.is_empty() {
        let mut out: Vec<u8> = Vec::new();
        for &entry in &env_refs {
            out.extend_from_slice(entry);
            out.push(b'\n');
        }
        support::emit(&out);
        support::terminate(0);
    }
    if names.len() != 1 {
        usage();
    }
    let name = names[0];
    if name_is_invalid(name.as_bytes()) {
        fail(&alloc::format!("Invalid environment variable {}", name));
    }
    match lookup(&env_refs, name.as_bytes()) {
        Some(value) => {
            let mut out = value.to_vec();
            out.push(b'\n');
            support::emit(&out);
            support::terminate(0);
        }
        None => support::terminate(1),
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
