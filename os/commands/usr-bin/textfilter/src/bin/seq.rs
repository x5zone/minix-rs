//! Minix-RS seq — number sequences.
//!
//! Ground truth: `minix3/usr.bin/seq/seq.c` (NetBSD). The deciding half
//! (operand shapes, the direction-following default increment, the
//! wrong-direction errors) is the library's `seq` module; this program
//! generates the numbers and prints them with the separator after each
//! one and the terminator after the last (seq.c:180-192). The floating
//! generation and `-f` formats of the C are declared unsupported by the
//! engine (the same float-rendering adjudication as printf's); the
//! errors leave with status 1 (`errx(1, ...)` in the C).


#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::seq::{generate, parse, render_output, SeqError};

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let words: Vec<&str> = argv[1..].iter().map(String::as_str).collect();
    let options = match parse(&words) {
        Ok(options) => options,
        Err(error) => fail(&error),
    };
    let values = match generate(&options) {
        Ok(values) => values,
        Err(error) => fail(&error),
    };
    support::emit(render_output(&values, &options).as_bytes());
    support::terminate(0);
}

fn fail(error: &SeqError) -> ! {
    support::warn(&seq_error_message(error));
    support::terminate(1);
}

fn seq_error_message(error: &SeqError) -> Vec<u8> {
    match error {
        SeqError::MissingLast => b"seq: missing last operand\n".to_vec(),
        SeqError::InvalidNumber(word) => {
            let mut message = b"seq: invalid ".to_vec();
            message.extend_from_slice(word.as_bytes());
            message.extend_from_slice(b" argument\n");
            message
        }
        SeqError::ZeroIncrement => b"seq: zero increment\n".to_vec(),
        SeqError::NeedsPositiveIncrement => {
            b"seq: needs positive increment\n".to_vec()
        }
        SeqError::NeedsNegativeDecrement => {
            b"seq: needs negative decrement\n".to_vec()
        }
        SeqError::FloatNotModelled => {
            b"seq: -f formats wait for the float renderer\n".to_vec()
        }
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
