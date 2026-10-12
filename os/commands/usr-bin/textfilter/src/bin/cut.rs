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

use alloc::string::{String, ToString};
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_sys::{close, open, read, Fd};
use minix_textfilter::cut::{parse_list, select_bytes, select_fields};

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let mut bytes_list: Option<String> = None;
    let mut fields_list: Option<String> = None;
    let mut delimiter: u8 = b'\t';
    let mut suppress = false;
    let mut operands: Vec<String> = Vec::new();
    let mut index = 1;
    while index < argv.len() {
        let mut option = argv[index].as_str();
        let mut inline: Option<String> = None;
        if option.starts_with('-') && option.len() > 2 {
            // `-Nlist` and `-dX` inline forms: split the option from its
            // argument the way getopt would have.
            inline = Some(option[2..].to_string());
            option = &option[..2];
        }
        match option {
            "-b" | "-c" => {
                let list = take_value(&argv, &mut index, inline);
                if fields_list.is_some() {
                    conflict();
                }
                bytes_list = Some(list);
            }
            "-f" => {
                let list = take_value(&argv, &mut index, inline);
                if bytes_list.is_some() {
                    conflict();
                }
                fields_list = Some(list);
            }
            "-d" => {
                let value = take_value(&argv, &mut index, inline);
                let mut bytes = value.bytes();
                delimiter = match (bytes.next(), bytes.next()) {
                    (Some(one), None) => one,
                    _ => support::terminate(1),
                };
            }
            "-s" => suppress = true,
            // `-n`: accepted and ignored for compatibility (`cut.c:112-113`,
            // `case 'n': break`) — it must not fall through to the operand list.
            "-n" => {}
            _ => {
                // An unrecognised `-X` is C's `case '?'` → `usage()`, not a
                // file operand (previously any dash token leaked into `files`).
                if option.starts_with('-') && option.len() > 1 {
                    usage();
                }
                operands.push(argv[index].clone());
            }
        }
        index += 1;
    }

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

/// Resolves the value of an option given separately (`-f 1`) or inline
/// (`-f1`), advancing the index past a consumed separate argument.
fn take_value(argv: &[String], index: &mut usize, inline: Option<String>) -> String {
    match inline {
        Some(value) => value,
        None => {
            *index += 1;
            if *index >= argv.len() {
                support::terminate(1);
            }
            argv[*index].clone()
        }
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
