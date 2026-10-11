//! Minix-RS hexdump — ASCII, decimal, hex, octal dump.
//!
//! Ground truth: `minix3/usr.bin/hexdump/hexsyntax.c` +
//! `display.c`, usage `hexdump [-bcCdovx] [-e fmt] [-f fmt_file]
//! [-n length] [-s skip] [file ...]`. Format options accumulate:
//! every block prints one row per selected format, like the C's FS
//! list. The deciding half (block iteration, `*` suppression, field
//! padding) is the library's `hexdump` module. The `-e`/`-f` format
//! mini-language is a registered remaining corner and exits with an
//! explicit message. File operands are opened in turn and chained into
//! one byte stream (`display.c` `next` `freopen`s each onto stdin, so the
//! address runs continuously and `-s` skips from the front); with no
//! operands the stream is stdin, and a bare `-` is a literal file name,
//! not a stdin alias. An unopenable operand warns and flags the exit
//! value (`exitval = 1`) and the chain continues. `-s skip` and `-n
//! length` bound the displayed range (skip advances past the front of the
//! stream, then at most `length` bytes are shown, clamped to the input). The
//! hosted-versus-target seams are the echo template's
//! (`os/commands/bin/fileops/src/bin/echo.rs`).


#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;
use alloc::format;

use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::hexdump::{
    parse_skip, render_row, row_spec, walk_blocks, BlockEvent, DumpFormat, HexError, HexOptions,
};
use minix_sys::read;

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let mut formats: Vec<DumpFormat> = Vec::new();
    let mut options = HexOptions::default();
    let mut skip: u64 = 0;

    let mut i = 1;
    while i < argv.len() {
        let arg = argv[i].clone();
        if !arg.starts_with('-') || arg.len() == 1 {
            break;
        }
        let chars: Vec<char> = arg.chars().skip(1).collect();
        let mut k = 0usize;
        while k < chars.len() {
            match chars[k] {
                'b' => formats.push(DumpFormat::OctalBytes),
                'c' => formats.push(DumpFormat::Chars),
                'C' => formats.push(DumpFormat::Canonical),
                'd' => formats.push(DumpFormat::Decimal),
                'o' => formats.push(DumpFormat::OctalShort),
                'x' => formats.push(DumpFormat::Hex),
                'v' => options.verbose = true,
                'n' => {
                    let value = match flag_value(&argv, &mut i, &chars, &mut k) {
                        Some(v) => v,
                        None => usage(),
                    };
                    match value.parse::<i64>() {
                        Ok(n) if n >= 0 => options.length = Some(n as usize),
                        _ => {
                            support::warn(format!("hexdump: {}: bad length value\n", value).as_bytes());
                            support::terminate(1);
                        }
                    }
                }
                's' => {
                    let value = match flag_value(&argv, &mut i, &chars, &mut k) {
                        Some(v) => v,
                        None => usage(),
                    };
                    match parse_skip(&value) {
                        Ok(n) => skip = n,
                        Err(HexError::BadSkip(_)) => {
                            support::warn(format!("hexdump: {}: bad skip value\n", value).as_bytes());
                            support::terminate(1);
                        }
                        Err(_) => unreachable!(),
                    }
                }
                'e' | 'f' => {
                    support::warn(
                        b"hexdump: the -e/-f format language is not implemented yet (registered corner)\n",
                    );
                    support::terminate(1);
                }
                _ => usage(),
            }
            k += 1;
        }
        i += 1;
    }
    if formats.is_empty() {
        formats.push(DumpFormat::Default);
    }

    let operands: Vec<&str> = argv[i..].iter().map(|s| s.as_str()).collect();
    // `display.c` `next` chains every operand through `freopen` onto stdin,
    // so the file operands form one continuous byte stream with a single
    // running address (and `-s` skips from that stream's front, as `start`
    // below). With no operands the stream is stdin; a bare `-` is a literal
    // file name, not a stdin alias. An unopenable operand warns and flags the
    // exit value, and the chain continues (`next`, `exitval = 1`).
    let mut input = Vec::new();
    let mut rval = 0;
    if operands.is_empty() {
        let mut chunk = [0u8; 4096];
        loop {
            match read(support::STDIN, &mut chunk) {
                Ok(0) => break,
                Ok(n) => input.extend_from_slice(&chunk[..n]),
                Err(_) => break,
            }
        }
    } else {
        for operand in &operands {
            match support::read_file(operand) {
                Ok(data) => input.extend_from_slice(&data),
                Err(()) => {
                    support::warn(format!("hexdump: {operand}\n").as_bytes());
                    rval = 1;
                }
            }
        }
    }
    // `doskip` consumes `skip` bytes, then `-n length` bounds the display
    // (`hexdump.c` `eaddress = address + length`); both clamp to the input.
    let (start, end) = display_range(input.len(), skip as usize, options.length);
    let data = &input[start..end];

    // display(): each block prints one row per accumulated format.
    let specs: Vec<_> = formats.iter().map(|f| (row_spec(*f), *f)).collect();
    let mut out: Vec<u8> = Vec::new();
    for event in walk_blocks(data, options.verbose) {
        match event {
            BlockEvent::Block { addr, bytes } => {
                let mut scratch: Vec<u8> = Vec::new();
                for (spec, _) in &specs {
                    render_row(bytes, addr, data.len(), spec, &mut scratch);
                }
                out.extend_from_slice(&scratch);
            }
            BlockEvent::Star => out.extend_from_slice(b"*\n"),
        }
    }
    if !data.is_empty() {
        // endfu: one final address line, in the first format's width.
        let width = specs.first().map(|(s, _)| s.gutter_width).unwrap_or(7);
        out.extend_from_slice(format!("{:0w$x}\n", data.len(), w = width).as_bytes());
    }
    support::emit(&out);
    support::terminate(rval);
}

fn flag_value(argv: &[String], i: &mut usize, chars: &[char], k: &mut usize) -> Option<String> {
    *k += 1;
    if *k < chars.len() {
        return Some(chars[*k..].iter().collect());
    }
    *i += 1;
    argv.get(*i).cloned()
}

fn usage() -> ! {
    support::warn(b"usage: hexdump [-bcCdovx] [-e fmt] [-f fmt_file] [-n length] [-s skip] [file ...]\n");
    support::terminate(1);
}


/// The byte range to display given `-s skip` and `-n length`: skip advances
/// past the front of the (chained) input, then at most `length` bytes are
/// shown. Mirrors `hexdump.c`'s `doskip` (running address) and
/// `eaddress = address + length` (display stops there), clamped to the input.
fn display_range(input_len: usize, skip: usize, length: Option<usize>) -> (usize, usize) {
    let start = skip.min(input_len);
    let available = input_len - start;
    let take = length.map_or(available, |n| n.min(available));
    (start, start + take)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_skip_no_length_shows_all() {
        assert_eq!(display_range(100, 0, None), (0, 100));
    }

    #[test]
    fn skip_advances_the_front() {
        assert_eq!(display_range(100, 10, None), (10, 100));
    }

    #[test]
    fn length_bounds_the_display() {
        assert_eq!(display_range(100, 0, Some(16)), (0, 16));
    }

    #[test]
    fn skip_then_length_compose() {
        // `-s 40 -n 10` shows bytes [40, 50).
        assert_eq!(display_range(100, 40, Some(10)), (40, 50));
    }

    #[test]
    fn range_clamps_to_the_input() {
        // length past the end and skip past the end both clamp.
        assert_eq!(display_range(100, 90, Some(50)), (90, 100));
        assert_eq!(display_range(100, 150, Some(10)), (100, 100));
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
