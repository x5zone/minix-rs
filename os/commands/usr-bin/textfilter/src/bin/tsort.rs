//! Minix-RS tsort — topological sort.
//!
//! Ground truth: `minix3/usr.bin/tsort/tsort.c` (NetBSD). The deciding
//! half (pair parsing, the removal passes, cycle detection and the
//! one-node cycle break) is the library's `tsort` module; this program reads
//! whitespace-separated token pairs from its single file operand or stdin,
//! prints the order, and reports cycles on standard error ("cycle in data" plus
//! the members — suppressed by `-q`; `-d`/`-l` are accepted no-ops as in C). An
//! odd token count leaves with status 1 ("odd data count", tsort.c:183).

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::tsort::{TsortArgs, parse_args, tsort};

fn run() -> ! {
    let argv: Vec<Vec<u8>> = support::args_bytes();
    let refs: Vec<&[u8]> = argv.iter().map(Vec::as_slice).collect();
    let TsortArgs { quiet, file } = match parse_args(&refs[1..]) {
        Ok(args) => args,
        Err(_) => usage(),
    };

    // C `switch(argc)`: no operand reads stdin, one operand reads that file
    // (`err(1, "%s")` when it cannot be opened), more than one is a usage error.
    let input = match file {
        Some(path) => {
            // The open call takes `&str` (registered `ARGV-BYTE-PATH-IN-LIBS`), so the
            // fold happens only at this point; what the message reads out is the original
            // bytes.
            let shown = String::from_utf8_lossy(path).into_owned();
            match support::read_file(&shown) {
                Ok(bytes) => bytes,
                Err(_) => {
                    let mut line: Vec<u8> = b"tsort: ".to_vec();
                    line.extend_from_slice(path);
                    line.push(b'\n');
                    support::warn(&line);
                    support::terminate(1);
                }
            }
        }
        None => support::read_stdin(),
    };
    let text = core::str::from_utf8(&input).unwrap_or("");
    let tokens: Vec<&str> = text.split_whitespace().collect();
    let result = match tsort(&tokens) {
        Ok(result) => result,
        Err(_) => {
            support::warn(b"tsort: odd data count\n");
            support::terminate(1);
        }
    };
    for cycle in &result.cycles {
        if quiet {
            continue;
        }
        let mut message = b"cycle in data\n".to_vec();
        for member in &cycle.members {
            message.extend_from_slice(member.as_bytes());
            message.push(b'\n');
        }
        support::warn(&message);
    }
    for name in &result.order {
        let mut out = name.as_bytes().to_vec();
        out.push(b'\n');
        support::emit(&out);
    }
    support::terminate(0);
}

fn usage() -> ! {
    support::warn(b"usage: tsort [-dlq] [file]\n");
    support::terminate(2);
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
