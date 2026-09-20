//! Minix-RS uniq — adjacent duplicate filter.
//!
//! Ground truth: `minix3/usr.bin/uniq/uniq.c` (NetBSD), `uniq [-c | -d |
//! -u] [-i] [-f fields] [-s chars] [input [output]]`. The run grouping,
//! mode selection, and count formatting are the library's (`Uniq`,
//! `UniqMode`, `Run::format`, which prints the count prefix without the
//! trailing newline — the doing half adds it). Input is stdin, split into
//! newline-stripped lines before the run handler sees them. The
//! case-fold (`-i`) and field-skip (`-f`, `-s`) comparisons are not in
//! the library yet and are reported as usage failures rather than being
//! silently ignored; file operands wait for the gated open-existing call.


#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::uniq::{Uniq, UniqMode};

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let mut mode = UniqMode::All;
    let mut mode_seen = false;
    let mut index = 1;
    while index < argv.len() {
        let option = argv[index].as_str();
        if !option.starts_with('-') || option.len() == 1 {
            unsupported_operand(option);
        }
        for flag in option[1..].bytes() {
            let next = match flag {
                b'c' => UniqMode::Counted,
                b'd' => UniqMode::Repeated,
                b'u' => UniqMode::Unique,
                // -i, -f, -s land here: comparisons the library does not
                // model yet (edge of this batch, see 07 §4.4).
                _ => support::terminate(1),
            };
            if mode_seen && mode != next {
                // Composed modes (-c with -d/-u) need library support.
                support::terminate(1);
            }
            mode = next;
            mode_seen = true;
        }
        index += 1;
    }

    let input = support::read_stdin();
    let lines = support::lines_of(&input);
    let mut handler = Uniq::new(mode);
    let mut out = [0u8; 4096];
    for line in &lines {
        if let Some(run) = handler.push(line) {
            report(&mut handler, &run, &mut out);
        }
    }
    if let Some(run) = handler.finish() {
        report(&mut handler, &run, &mut out);
    }
    support::terminate(0);
}

fn report(handler: &mut Uniq<'_>, run: &minix_textfilter::uniq::Run<'_>, out: &mut [u8]) {
    if !handler.keep(run) {
        return;
    }
    match handler.format(run, out) {
        Ok(len) => {
            support::emit(&out[..len]);
            support::emit(b"\n");
        }
        Err(_) => support::terminate(1),
    }
}

fn unsupported_operand(option: &str) -> ! {
    // File operands need the gated open-existing call; the write-operand
    // form likewise waits for the create path.
    support::emit(b"uniq: file operands wait for the open call (edge E-CMDSYSFACE)\n");
    let _ = option;
    support::terminate(1);
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
