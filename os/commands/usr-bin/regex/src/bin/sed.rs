//! Minix-RS sed — substitute scripts over stdin.
//!
//! Ground truth: `minix3/usr.bin/sed/sed/` (NetBSD process.c). The
//! substitution engine is the library's (`parse_subst` + `apply`, which
//! mirror `process.c:418-422` including the empty-match advance). This
//! build carries the `s` command only, with its `p` flag and the `-n`
//! option interacting exactly as the C: without `-n` every result line
//! prints; with `-n` only lines whose `p` flag fired print. Chained
//! commands (`;`), addresses, and the other verbs are not modelled and
//! are rejected with an explicit message instead of being ignored. File
//! operands wait for the gated open-existing call.


#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;
use alloc::vec;

use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_regex::pattern::{compile_basic, Pattern};
use minix_regex::sed::{apply, parse_subst};

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let mut quiet = false;
    let mut script: Option<&str> = None;
    for arg in &argv[1..] {
        if arg == "-n" {
            quiet = true;
        } else if arg.starts_with('-') && arg.len() > 1 {
            support::emit(b"sed: unsupported option\n");
            support::terminate(1);
        } else if script.is_none() {
            script = Some(arg);
        } else {
            support::emit(b"sed: file operands wait for the open call (edge E-CMDSYSFACE)\n");
            support::terminate(1);
        }
    }
    let script = match script {
        Some(script) => script,
        None => {
            support::emit(b"usage: sed [-n] script\n");
            support::terminate(1);
        }
    };
    // Only the substitute verb is modelled; anything else is a loud
    // failure, never a silent pass-through.
    if !script.starts_with('s') {
        support::emit(b"sed: only s (substitute) scripts are modelled (see 08 sec 4.4)\n");
        support::terminate(1);
    }
    let (subst, consumed) = match parse_subst(&script[1..]) {
        Ok(parsed) => parsed,
        Err(_) => {
            support::emit(b"sed: bad s command\n");
            support::terminate(1);
        }
    };
    if consumed != script.len() - 1 {
        support::emit(b"sed: chained commands are not modelled (see 08 sec 4.4)\n");
        support::terminate(1);
    }
    let pattern: Pattern = match compile_basic(subst.pattern_text) {
        Ok(pattern) => pattern,
        Err(_) => {
            support::emit(b"sed: bad s command\n");
            support::terminate(1);
        }
    };

    let input = support::read_stdin();
    let mut out = vec![0u8; 4 * 1024 * 1024];
    for line in support::lines_of(&input) {
        let (len, replaced) = apply(&pattern, &subst, line, &mut out);
        let prints = !quiet || (replaced && subst.print);
        if prints {
            let mut result = out[..len].to_vec();
            result.push(b'\n');
            support::emit(&result);
        }
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
