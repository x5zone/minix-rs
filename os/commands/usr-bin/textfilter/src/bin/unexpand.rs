//! Minix-RS unexpand — spaces to tabs.
//!
//! Ground truth: `minix3/usr.bin/unexpand/unexpand.c` (NetBSD). The
//! deciding half (the tabify walk, `-t` stop lists, the `-a`
//! whole-line form) is the library's `unexpand` module. With no operand the
//! program tabifies standard input; with operands it tabifies each file in turn
//! (the line walk restarting per file), and a file it cannot open is fatal (C
//! `err(EXIT_FAILURE)`).

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_sys::{close, open, read};
use minix_textfilter::unexpand::{parse_stops, unexpand, UnexpandOptions};

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let mut options = UnexpandOptions::default();
    let mut operands: Vec<String> = Vec::new();
    let mut index = 1;
    while index < argv.len() {
        let arg = argv[index].as_str();
        if arg == "-a" {
            options.all = true;
        } else if arg == "-t" {
            index += 1;
            if index >= argv.len() {
                usage();
            }
            match parse_stops(&argv[index]) {
                Some(stops) => options.stops = stops,
                None => {
                    support::warn(b"unexpand: bad tab stop spec\n");
                    support::terminate(1);
                }
            }
        } else if arg.starts_with('-') && arg.len() > 1 {
            usage();
        } else {
            operands.push(argv[index].clone());
        }
        index += 1;
    }

    // C `do { if (argc>0) freopen(argv[0]); while (fgetln) convertline } while
    // (argc > 0)`: no operand reads stdin once, otherwise each file is tabified
    // in turn (the `unexpand` line walk restarts per file, so no tab-column
    // state leaks across files), and an unopenable file is fatal (C
    // `err(EXIT_FAILURE)`).
    if operands.is_empty() {
        unexpand_stream(support::STDIN, &options);
        support::terminate(0);
    }
    for operand in &operands {
        let fd = match open(operand, 0, 0) {
            Ok(fd) => fd,
            Err(_) => {
                support::warn(format!("unexpand: Cannot open `{operand}'\n").as_bytes());
                support::terminate(1);
            }
        };
        unexpand_stream(fd, &options);
        //[SD19-gate] SD-19: close 后 fd 不再使用
        #[allow(clippy::let_underscore_must_use)]
        let _ = close(fd);
    }
    support::terminate(0)
}

/// Read a descriptor to the end and emit each tabified line.
fn unexpand_stream(fd: i32, options: &UnexpandOptions) {
    let mut input = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match read(fd, &mut chunk) {
            Ok(0) => break,
            Ok(count) => input.extend_from_slice(&chunk[..count]),
            Err(_) => break,
        }
    }
    let text = String::from_utf8_lossy(&input);
    for line in unexpand(&text, options) {
        let mut out = line.into_bytes();
        out.push(b'\n');
        support::emit(&out);
    }
}

fn usage() -> ! {
    support::warn(b"usage: unexpand [-a] [-t tablist]\n");
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
