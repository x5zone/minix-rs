//! Minix-RS cut — select bytes or fields per line.
//!
//! Ground truth: `minix3/usr.bin/cut/cut.c` (NetBSD), `cut -b list |
//! -c list | -f list [-d delim] [-s] [file ...]`. The list grammar and the
//! byte/field selection are the library's (`parse_list`,
//! `select_bytes`, `select_fields`); `-b` and `-c` coincide here because
//! the selection is byte-grained (the C distinction matters only for
//! multibyte locales, which this build does not carry). With no operand lines
//! come from stdin; each `-` operand reads stdin and each file operand is
//! opened, and a file that cannot be opened warns and is skipped (the exit
//! status stays nonzero, C `cut.c:138-142`). A malformed list or a missing
//! selection exits 1; a delimiter must be one byte.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;
use alloc::format;
use alloc::vec;

use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_sys::{Fd, close, open, read};
use minix_textfilter::cut::{CutError, parse_cut_args, parse_list, select_bytes, select_fields};

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();
    // argv folding lives in the deciding half (`parse_cut_args`): BSD
    // `getopt("b:c:d:f:sn")` with attached-or-next list/delimiter values and
    // flag clustering. The old inline whole-token matcher mishandled a flag
    // clustered with a value option (`cut -sf1-3` silently dropped the field
    // list) and `-ns` (dropped the `-s` suppress).
    let parsed = match parse_cut_args(&args[1..]) {
        Ok(parsed) => parsed,
        Err(e) => arg_error(e),
    };
    let bytes_list: Option<String> = parsed.bytes_list.map(String::from);
    let fields_list: Option<String> = parsed.fields_list.map(String::from);
    let delimiter = parsed.delimiter;
    let suppress = parsed.suppress;
    let operands: Vec<String> = parsed.operands.iter().map(|s| String::from(*s)).collect();

    let selecting_bytes = bytes_list.is_some();
    let list_text = match (bytes_list, fields_list) {
        (Some(list), None) => list,
        (None, Some(list)) => list,
        _ => usage(),
    };
    let parsed = match parse_list(&list_text) {
        Ok((ranges, count)) => (ranges, count),
        Err(_) => support::terminate(1),
    };
    let ranges = &parsed.0[..parsed.1];

    let process = |input: &[u8]| {
        for line in support::lines_of(input) {
            let mut out = vec![0u8; line.len() + 1];
            let selection = if selecting_bytes {
                select_bytes(line, ranges, &mut out)
            } else {
                select_fields(line, ranges, delimiter, suppress, &mut out)
            };
            match selection {
                Ok(len) => {
                    support::emit(&out[..len]);
                    support::emit(b"\n");
                }
                Err(_) => support::terminate(1),
            }
        }
    };

    // C `if (*argv) for each operand fcn(...) else fcn(stdin)`: `-` and the
    // empty operand list both read stdin; a file is opened per operand, and an
    // unopenable file warns and is skipped (exit status stays nonzero).
    let mut status = 0;
    if operands.is_empty() {
        process(&support::read_stdin());
    }
    for operand in &operands {
        if operand == "-" {
            process(&support::read_stdin());
            continue;
        }
        let fd = match open(operand, 0, 0) {
            Ok(fd) => fd,
            Err(_) => {
                support::warn(format!("cut: {operand}\n").as_bytes());
                status = 1;
                continue;
            }
        };
        let input = read_fd(fd);
        //[SD19-gate] SD-19: close 后 fd 不再使用
        #[allow(clippy::let_underscore_must_use)]
        let _ = close(fd);
        process(&input);
    }
    support::terminate(status)
}

/// Read a descriptor to the end.
fn read_fd(fd: Fd) -> Vec<u8> {
    let mut input = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match read(fd, &mut chunk) {
            Ok(0) => break,
            Ok(count) => input.extend_from_slice(&chunk[..count]),
            Err(_) => break,
        }
    }
    input
}

/// Map a [`CutError`] from `parse_cut_args` to the exact C error path (the
/// message and exit code) the old in-bin matcher used, then terminate.
fn arg_error(e: CutError) -> ! {
    match e {
        CutError::Usage => usage(),
        CutError::Conflict => conflict(),
        // A `-d` value that is not exactly one byte: C warns and exits;
        // the old matcher simply terminated nonzero (unchanged here).
        CutError::BadDelimiter => support::terminate(1),
    }
}

fn conflict() -> ! {
    support::emit(b"cut: only one type of list may be specified\n");
    support::terminate(1);
}

/// C `usage()` (`cut.c:290-296`): a missing/extra-ambiguous selection or an
/// unrecognised option. Exit 1.
fn usage() -> ! {
    support::emit(b"usage: cut -b list [-n] | -c list | -f list [-d delim] [-s] [file ...]\n");
    support::terminate(1);
}

// Standard input descriptor retained for the no_std sweep (the collection
// helper owns the read today).
const _: Fd = 0;

#[cfg(all(not(test), target_os = "none"))]
#[unsafe(no_mangle)]
extern "Rust" fn main() -> i32 {
    run()
}

#[cfg(any(test, not(target_os = "none")))]
fn main() {
    run()
}
