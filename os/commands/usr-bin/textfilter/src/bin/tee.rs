//! Minix-RS tee — fan stdin out to stdout and file operands.
//!
//! Ground truth: `minix3/usr.bin/tee/tee.c` (NetBSD): `-a` appends
//! instead of truncating, `-i` ignores interrupts, stdin copies to
//! stdout and every operand file, and open/write failures warn while
//! the status leaves 1. File creation waits for the gated create face
//! (edge E-CMDSYSFACE), so this build warns per operand and still
//! serves stdout; `-i`'s signal face is likewise a later batch.

#[path = "../bin_support.rs"]
mod support;

use minix_sys::read;

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    let mut operands: Vec<&str> = Vec::new();
    let mut index = 1;
    while index < argv.len() {
        let arg = argv[index].as_str();
        if arg == "-a" || arg == "-i" {
            // Both flags parse; `-a` only matters once file writes land
            // and `-i`'s signal face is a later batch.
            support::warn(b"tee: -a/-i wait for the file and signal faces\n");
            support::terminate(1);
        } else if arg.starts_with('-') && arg.len() > 1 {
            support::warn(b"usage: tee [-ai] [file ...]\n");
            support::terminate(1);
        } else {
            operands.push(arg);
        }
        index += 1;
    }
    for operand in &operands {
        support::warn(b"tee: file operands wait for the open call (edge E-CMDSYSFACE)\n");
    }

    let mut chunk = [0u8; 4096];
    loop {
        match read(support::STDIN, &mut chunk) {
            Ok(0) => break,
            Ok(count) => {
                let slice = &chunk[..count];
                // Standard output first, then (gated) file targets —
                // tee.c:109-123's fan-out with the partial-write loop.
                let mut written = 0usize;
                while written < count {
                    match minix_sys::write(support::STDOUT, &slice[written..]) {
                        Ok(0) => break,
                        Ok(n) => written += n as usize,
                        Err(_) => break,
                    }
                }
                if written < count {
                    support::terminate(1);
                }
            }
            Err(_) => {
                support::warn(b"tee: read error\n");
                support::terminate(1);
            }
        }
    }
    support::terminate(0);
}
