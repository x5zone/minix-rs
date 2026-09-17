//! Minix-RS unvis — decode visible-encoded stdin.
//!
//! Ground truth: `minix3/usr.bin/unvis/unvis.c` (NetBSD): the decoder
//! state machine is the library's `vis::decode`; this program reads
//! stdin, decodes, and prints. File operands wait for the gated
//! open-existing call; the HTTP/MIME flag families are later batches.

#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::vis::decode;
use minix_sys::read;

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    for arg in &argv[1..] {
        if arg.starts_with('-') && arg.len() > 1 {
            support::warn(b"unvis: flag families beyond the default decoder are later batches\n");
            support::terminate(1);
        }
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
    let text = std::str::from_utf8(&input).unwrap_or("");
    let decoded = decode(text);
    support::emit(&decoded);
    support::terminate(0);
}
