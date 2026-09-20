//! Minix-RS hexdump — ASCII, decimal, hex, octal dump.
//!
//! Ground truth: `minix3/usr.bin/hexdump/hexsyntax.c` +
//! `display.c`, usage `hexdump [-bcCdovx] [-e fmt] [-f fmt_file]
//! [-n length] [-s skip] [file ...]`. Format options accumulate:
//! every block prints one row per selected format, like the C's FS
//! list. The deciding half (block iteration, `*` suppression, field
//! padding) is the library's `hexdump` module. The `-e`/`-f` format
//! mini-language is a registered remaining corner and exits with an
//! explicit message. File operands are rejected until the
//! open-existing seam lands (edge E-CMDSYSFACE); stdin works. The
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
    if operands.iter().any(|s| *s != "-") {
        support::warn(
            b"hexdump: file operands wait on the open-existing seam (edge E-CMDSYSFACE)\n",
        );
        support::terminate(1);
    }

    let mut input = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match read(support::STDIN, &mut chunk) {
            Ok(0) => break,
            Ok(n) => input.extend_from_slice(&chunk[..n]),
            Err(_) => break,
        }
    }
    // doskip consumes `skip` bytes before any block is shown.
    let start = (skip as usize).min(input.len());
    let data = &input[start..];

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
    support::terminate(0);
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


#[cfg(all(not(test), target_os = "none"))]
#[unsafe(no_mangle)]
extern "Rust" fn main() -> i32 {
    run()
}

#[cfg(any(test, not(target_os = "none")))]
fn main() {
    run()
}
