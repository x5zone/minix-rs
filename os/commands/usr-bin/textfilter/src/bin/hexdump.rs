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

use minix_sys::read;
use minix_textfilter::hexdump::{
    BlockEvent, DumpFormat, HexError, HexOptions, parse_skip, render_row, row_spec, walk_blocks,
};

fn run() -> ! {
    let argv: Vec<Vec<u8>> = support::args_bytes();
    let mut formats: Vec<DumpFormat> = Vec::new();
    let mut options = HexOptions::default();
    let mut skip: u64 = 0;

    let mut i = 1;
    while i < argv.len() {
        let arg: &[u8] = &argv[i];
        if arg.first() != Some(&b'-') || arg.len() == 1 {
            break;
        }
        let flags = &arg[1..];
        let mut k = 0usize;
        while k < flags.len() {
            match flags[k] {
                b'b' => formats.push(DumpFormat::OctalBytes),
                b'c' => formats.push(DumpFormat::Chars),
                b'C' => formats.push(DumpFormat::Canonical),
                b'd' => formats.push(DumpFormat::Decimal),
                b'o' => formats.push(DumpFormat::OctalShort),
                b'x' => formats.push(DumpFormat::Hex),
                b'v' => options.verbose = true,
                b'n' => {
                    let value = match flag_value(&argv, &mut i, flags, &mut k) {
                        Some(v) => v,
                        None => usage(),
                    };
                    match parse_i64(value) {
                        Some(n) if n >= 0 => options.length = Some(n as usize),
                        _ => {
                            let mut line: Vec<u8> = b"hexdump: ".to_vec();
                            line.extend_from_slice(value);
                            line.extend_from_slice(b": bad length value\n");
                            support::warn(&line);
                            support::terminate(1);
                        }
                    }
                }
                b's' => {
                    let value = match flag_value(&argv, &mut i, flags, &mut k) {
                        Some(v) => v,
                        None => usage(),
                    };
                    match parse_skip(value) {
                        Ok(n) => skip = n,
                        Err(HexError::BadSkip) => {
                            let mut line: Vec<u8> = b"hexdump: ".to_vec();
                            line.extend_from_slice(value);
                            line.extend_from_slice(b": bad skip value\n");
                            support::warn(&line);
                            support::terminate(1);
                        }
                        Err(_) => unreachable!(),
                    }
                }
                b'e' | b'f' => {
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

    let operands: Vec<&[u8]> = argv[i..].iter().map(Vec::as_slice).collect();
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
            // The open entry wants `&str` (registered under `ARGV-BYTE-PATH-IN-LIBS`),
            // so the lossy fold happens only at this point.
            let shown = String::from_utf8_lossy(operand).into_owned();
            match support::read_file(&shown) {
                Ok(data) => input.extend_from_slice(&data),
                Err(_) => {
                    let mut line: Vec<u8> = b"hexdump: ".to_vec();
                    line.extend_from_slice(operand);
                    line.push(b'\n');
                    support::warn(&line);
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

/// The `-n`/`-s` value: the rest of this token, or the next word — both handed
/// back as argv's **raw bytes**.
fn flag_value<'a>(
    argv: &'a [Vec<u8>],
    i: &mut usize,
    flags: &'a [u8],
    k: &mut usize,
) -> Option<&'a [u8]> {
    *k += 1;
    if *k < flags.len() {
        return Some(&flags[*k..]);
    }
    *i += 1;
    argv.get(*i).map(Vec::as_slice)
}

/// The `-n` value test follows C's `strtoll` and accepts ASCII only: an optional
/// sign, then digits up to the first non-digit. A bare byte is not a digit either
/// before or after the fold, so folding cannot change the verdict on this leg,
/// only the byte string in the message.
fn parse_i64(bytes: &[u8]) -> Option<i64> {
    let mut i = 0usize;
    let negative = match bytes.first() {
        Some(b'-') => {
            i += 1;
            true
        }
        Some(b'+') => {
            i += 1;
            false
        }
        _ => false,
    };
    let mut value: i64 = 0;
    let mut digits = 0usize;
    while let Some(digit @ b'0'..=b'9') = bytes.get(i) {
        value = value
            .checked_mul(10)?
            .checked_add(i64::from(digit - b'0'))?;
        i += 1;
        digits += 1;
    }
    if digits == 0 {
        return None;
    }
    Some(if negative { -value } else { value })
}

fn usage() -> ! {
    support::warn(
        b"usage: hexdump [-bcCdovx] [-e fmt] [-f fmt_file] [-n length] [-s skip] [file ...]\n",
    );
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
