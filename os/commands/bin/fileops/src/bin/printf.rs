//! Minix-RS printf — runtime formatting over operands.
//!
//! Ground truth: `minix3/usr.bin/printf/printf.c` (NetBSD), main at lines
//! 125-175: the format is reused across operands, `\c` halts, and the
//! status leaves 1 when a conversion warned. The engine (flags, width,
//! precision, `d i o u x X s c %b %%`, and the format escapes) is the
//! library's `printf` module. Floating conversions are declared
//! unsupported by the engine (see its module header) and are reported as
//! errors rather than rendering wrong output.

#[path = "../bin_support.rs"]
mod support;

use minix_fileops::printf::{format, FormatError};

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    if argv.len() < 2 {
        support::warn(b"Usage: printf format [argument ...]\n");
        support::terminate(1);
    }
    let operands: Vec<&str> = argv[2..].iter().map(String::as_str).collect();
    let status = match format(&operands, &argv[1], &mut |piece: &[u8]| {
        support::write_ok(piece)
    }, &mut |diag: &[u8]| support::warn(diag)) {
        Ok(status) => status,
        Err(FormatError::MissingFormatCharacter) => {
            support::warn(b"missing format character\n");
            1
        }
        Err(FormatError::InvalidDirective(spec)) => {
            let mut line = Vec::new();
            line.extend_from_slice(spec.as_bytes());
            line.extend_from_slice(b": invalid directive\n");
            support::warn(&line);
            1
        }
        Err(FormatError::FloatNotModelled(_)) => {
            support::warn(b"printf: floating-point conversions are not modelled yet\n");
            1
        }
        Err(FormatError::WriteFailed) | Err(FormatError::TooLong) => {
            support::warn(b"print failed\n");
            1
        }
    };
    support::terminate(status);
}
