//! Minix-RS cksum — checksums and byte counts.
//!
//! Ground truth: `minix3/usr.bin/cksum/` (NetBSD). The deciding half —
//! the POSIX CRC, the two historic sums, the block-rounding of the printed
//! length, and the `%lu %lld` output format (`print.c` `pcrc`/`psum1`/`psum2`)
//! — is the library's `cksum` module. This program reads each file operand (or
//! standard input when none) and prints `{crc} {length}` plus ` {name}` for a
//! file; the C's `-a` crypto algorithms (md5/sha1/sha256/sha384/sha512) need
//! crypto primitives and are declared unsupported.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;
use alloc::format;

use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_sys::{close, open, read};
use minix_textfilter::cksum::{cksum_crc, csum1, csum2, sum_display_length, SumKind};

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let mut kind = SumKind::Crc;
    let mut operands: Vec<String> = Vec::new();
    let mut index = 1;
    while index < argv.len() {
        let arg = argv[index].as_str();
        if arg == "-o" {
            index += 1;
            if index >= argv.len() {
                usage();
            }
            kind = match argv[index].as_str() {
                "1" => SumKind::Old1,
                "2" => SumKind::Old2,
                _ => {
                    support::warn(b"cksum: invalid algorithm\n");
                    support::terminate(2);
                }
            };
        } else if arg == "-a" {
            // The crypto family (md5/sha1/sha256/sha384/sha512) waits
            // for crypto primitives.
            support::warn(b"cksum: -a crypto algorithms are not modelled yet\n");
            support::terminate(2);
        } else {
            operands.push(argv[index].clone());
        }
        index += 1;
    }

    let mut status = 0;
    if operands.is_empty() {
        // C computes over stdin and prints the line with no filename.
        let input = read_all(support::STDIN);
        emit_line(kind, &input, None);
        support::terminate(status);
    }
    for operand in &operands {
        let fd = match open(operand, 0, 0) {
            Ok(fd) => fd,
            Err(_) => {
                // C warns and skips the file, no output line for it.
                support::warn(format!("cksum: {operand}\n").as_bytes());
                status = 1;
                continue;
            }
        };
        let input = read_all(fd);
        //[SD19-gate] SD-19: close 后 fd 不再使用
        #[allow(clippy::let_underscore_must_use)]
        let _ = close(fd);
        emit_line(kind, &input, Some(operand.as_str()));
    }
    support::terminate(status)
}

/// Read a descriptor to the end (the C `crc`/`csum` read the whole input).
fn read_all(fd: i32) -> Vec<u8> {
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

/// Compute the selected sum over `data` and print C's `"%lu %lld"` line (with a
/// trailing ` name` for a file operand), the length folded per `print.c`.
fn emit_line(kind: SumKind, data: &[u8], name: Option<&str>) {
    let (value, bytes) = match kind {
        SumKind::Crc => cksum_crc(data),
        SumKind::Old1 => csum1(data),
        SumKind::Old2 => csum2(data),
    };
    let shown = sum_display_length(kind, bytes);
    match name {
        Some(n) => support::emit(format!("{value} {shown} {n}\n").as_bytes()),
        None => support::emit(format!("{value} {shown}\n").as_bytes()),
    }
}

fn usage() -> ! {
    support::warn(b"usage: cksum [-o 1 | -o 2] \n");
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
