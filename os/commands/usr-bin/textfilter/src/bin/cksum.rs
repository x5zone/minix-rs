//! Minix-RS cksum — checksums and byte counts.
//!
//! Ground truth: `minix3/usr.bin/cksum/` (NetBSD). The deciding half —
//! the POSIX CRC, the two historic sums, and the `%u %lld` output
//! format (print.c:52-55) — is the library's `cksum` module; this
//! program reads stdin, computes, and prints "{crc} {length}". The
//! C's `-a` crypto algorithms (md5/sha1/sha256/sha384/sha512) need
//! crypto primitives and are declared unsupported; file operands wait
//! for the gated open-existing call.

#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::cksum::{cksum_crc, csum1, csum2};
use minix_sys::read;

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    let mut algorithm: u8 = 0;
    let mut index = 1;
    while index < argv.len() {
        let arg = argv[index].as_str();
        if arg == "-o" {
            index += 1;
            if index >= argv.len() {
                usage();
            }
            match argv[index].as_str() {
                "1" => algorithm = 1,
                "2" => algorithm = 2,
                _ => {
                    support::warn(b"cksum: invalid algorithm\n");
                    support::terminate(2);
                }
            }
        } else if arg == "-a" {
            // The crypto family (md5/sha1/sha256/sha384/sha512) waits
            // for crypto primitives.
            support::warn(b"cksum: -a crypto algorithms are not modelled yet\n");
            support::terminate(2);
        } else {
            support::warn(b"cksum: file operands wait for the open call (edge E-CMDSYSFACE)\n");
            support::terminate(2);
        }
        index += 1;
    }

    let mut input = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match read(support::STDIN, &mut chunk) {
            Ok(0) => break,
            Ok(count) => input.extend_from_slice(&chunk[..count]),
            Err(_) => break,
        }
    }
    let (value, length) = match algorithm {
        0 => cksum_crc(&input),
        1 => csum1(&input),
        _ => csum2(&input),
    };

    // Output format: "%lu %lld\n" (print.c:52-55, no filename for
    // stdin).
    support::emit(format!("{} {}\n", value, length).as_bytes());
    support::terminate(0);
}

fn usage() -> ! {
    support::warn(b"usage: cksum [-o 1 | -o 2] \n");
    support::terminate(2);
}
