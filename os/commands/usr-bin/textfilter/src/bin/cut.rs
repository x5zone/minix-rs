//! Minix-RS cut — select bytes or fields per line.
//!
//! Ground truth: `minix3/usr.bin/cut/cut.c` (NetBSD), `cut -b list |
//! -c list | -f list [-d delim] [-s] [file ...]`. The list grammar and the
//! byte/field selection are the library's (`parse_list`,
//! `select_bytes`, `select_fields`); `-b` and `-c` coincide here because
//! the selection is byte-grained (the C distinction matters only for
//! multibyte locales, which this build does not carry). Lines come from
//! stdin — the file operands need the open-existing call that is still
//! gated on the 64-bit path message layout. A malformed list or a missing
//! selection exits 1; a delimiter must be one byte.

#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::cut::{parse_list, select_bytes, select_fields};
use minix_sys::Fd;

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    let mut bytes_list: Option<String> = None;
    let mut fields_list: Option<String> = None;
    let mut delimiter: u8 = b'\t';
    let mut suppress = false;
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
            _ => {
                // File operands need the gated open-existing call.
                support::emit(b"cut: file operands wait for the open call (edge E-CMDSYSFACE)\n");
                support::terminate(1);
            }
        }
        index += 1;
    }

    let selecting_bytes = bytes_list.is_some();
    let list_text = match (bytes_list, fields_list) {
        (Some(list), None) => list,
        (None, Some(list)) => list,
        _ => {
            support::emit(b"usage: cut -b list | -c list | -f list [-d delim] [-s]\n");
            support::terminate(1);
        }
    };
    let parsed = match parse_list(&list_text) {
        Ok((ranges, count)) => (ranges, count),
        Err(_) => support::terminate(1),
    };
    let ranges = &parsed.0[..parsed.1];

    let input = support::read_stdin();
    for line in support::lines_of(&input) {
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
    support::terminate(0);
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

// Standard input descriptor retained for the no_std sweep (the collection
// helper owns the read today).
const _: Fd = 0;
