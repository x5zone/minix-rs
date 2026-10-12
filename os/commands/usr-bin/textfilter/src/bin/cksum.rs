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
use minix_textfilter::cksum::{
    cksum_crc, csum1, csum2, parse_check_line, parse_cksum_args, sum_display_length, CheckEntry,
    CksumError, SumKind,
};

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    // `argv[0]` is the program name; C's getopt loop starts at `argv[1]`.
    let request = match parse_cksum_args(&refs[1..]) {
        Ok(request) => request,
        Err(CksumError::InvalidAlgorithm) => {
            support::warn(b"cksum: invalid algorithm\n");
            support::terminate(2);
        }
        Err(CksumError::CryptoUnsupported) => {
            support::warn(b"cksum: -a crypto algorithms are not modelled yet\n");
            support::terminate(2);
        }
        Err(CksumError::RequiresHash(flag)) => {
            // C's `requirehash` aborts when `-p/-s/-t/-x` is given without a
            // hash; the CRC/old-sum path never selects one.
            support::warn(format!("cksum: -{flag} requires a hash algorithm\n").as_bytes());
            support::terminate(2);
        }
        Err(CksumError::UnknownOption(flag)) => {
            support::warn(format!("cksum: illegal option -- {flag}\n").as_bytes());
            usage();
        }
    };

    if request.do_check {
        // `-c` consumes only the first operand as the check file (extra
        // operands are ignored), mirroring C's `argv[0]`-or-stdin choice.
        support::terminate(check_files(&request.operands, request.check_warn));
    }

    let mut status = 0;
    if request.operands.is_empty() {
        // C computes over stdin and prints the line with no filename.
        let input = read_all(support::STDIN);
        emit_line(request.kind, &input, None);
        support::terminate(status);
    }
    for operand in request.operands.iter().copied() {
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
        emit_line(request.kind, &input, Some(operand));
    }
    support::terminate(status)
}

/// `cksum -c`: verify the checksums listed in a check file (the first operand,
/// or standard input when there is none). Faithful to C's `do_check`
/// (`cksum.c:283-460`): a line is silent on success, prints `name: FAILED` on a
/// mismatch or an unreadable file, and the whole run exits 1 if any line failed.
/// The `name (file) = digest` hash grammar and a spaceless "bogus format" line
/// are counted as failures (the hash needs a crypto primitive we do not carry),
/// warned on only under `-w`. `-c` on an unreadable check file is C's `err(1,
/// "Cannot read %s")` — a hard exit 1.
fn check_files(operands: &[&str], check_warn: bool) -> i32 {
    let from_operand = !operands.is_empty();
    let fd = if from_operand {
        match open(operands[0], 0, 0) {
            Ok(fd) => fd,
            Err(_) => {
                support::warn(format!("cksum: Cannot read {}\n", operands[0]).as_bytes());
                return 1;
            }
        }
    } else {
        support::STDIN
    };
    let bytes = read_all(fd);
    if from_operand {
        //[SD19-gate] SD-19: close 后 fd 不再使用
        #[allow(clippy::let_underscore_must_use)]
        let _ = close(fd);
    }
    let Ok(text) = core::str::from_utf8(&bytes) else {
        return 1;
    };
    // C reads line-by-line with `fgets`, so a trailing newline terminates the
    // last real line and yields no phantom empty line; peel exactly one.
    let body = text.strip_suffix('\n').unwrap_or(text);
    let mut failed = false;
    if !body.is_empty() {
        for line in body.split('\n') {
            match parse_check_line(line) {
                CheckEntry::Hash => {
                    if check_warn {
                        support::warn(format!("cksum: unknown hash: {line}\n").as_bytes());
                    }
                    failed = true;
                }
                CheckEntry::Malformed => {
                    if check_warn {
                        support::warn(
                            format!("cksum: bogus format: {line}. Skipping...\n").as_bytes(),
                        );
                    }
                    failed = true;
                }
                CheckEntry::Checksum { crc, name } => {
                    let ok = match open(name, 0, 0) {
                        Err(_) => {
                            if check_warn {
                                support::warn(format!("cksum: {name}\n").as_bytes());
                            }
                            false
                        }
                        Ok(data_fd) => {
                            let data = read_all(data_fd);
                            //[SD19-gate] SD-19: close 后 fd 不再使用
                            #[allow(clippy::let_underscore_must_use)]
                            let _ = close(data_fd);
                            cksum_crc(&data).0 == crc
                        }
                    };
                    if !ok {
                        support::emit(format!("{name}: FAILED\n").as_bytes());
                        failed = true;
                    }
                }
            }
        }
    }
    i32::from(failed)
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
