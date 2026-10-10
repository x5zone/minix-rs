//! Minix-RS `uuencode` — the doing half over `minix_compress::uu`.
//!
//! Ground truth: `minix3/usr.bin/uuencode/uuencode.c`. The byte packing lives
//! in the library (`encode_stream` for the classic body, `base64_stream` for
//! `-m`); this program reads the input, derives the file mode, writes the
//! `begin`/`end` (or `begin-base64`/`====`) framing, and streams the result to
//! standard output.
//!
//! Wired: `uuencode [input] name` — with an `input` operand the mode is
//! `st_mode & 0777` (C's `fstat` after `freopen`, `uuencode.c:92-95`); with
//! only a `name` the input is standard input and the mode is `0666 & ~umask
//! (0666)` (`uuencode.c:99-100`). `-m` selects base64 (`uuencode.c:107-110`).
//! The octal mode is rendered without a leading zero, exactly as `printf("%o")`
//! does.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::string::String;
use alloc::vec::Vec;
use minix_compress::uu::{base64_stream, encode_stream};
use minix_sys::{read, Fd, Stat};

fn fail(message: &str) -> ! {
    let mut line = String::from("uuencode: ");
    line.push_str(message);
    line.push('\n');
    support::warn(line.as_bytes());
    support::terminate(1)
}

fn usage() -> ! {
    fail("usage: uuencode [-m] [inputfile] headername > encodedfile")
}

/// Render a value in octal with no leading zero, the way C's `%o` prints it.
fn octal(mut n: u32) -> Vec<u8> {
    if n == 0 {
        return Vec::from(&b"0"[..]);
    }
    let mut digits = Vec::new();
    while n > 0 {
        digits.push(b'0' + (n & 7) as u8);
        n >>= 3;
    }
    digits.reverse();
    digits
}

/// Read a whole descriptor into a vector (`read` until EOF).
fn slurp(fd: Fd) -> Vec<u8> {
    let mut data = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match read(fd, &mut chunk) {
            Ok(0) => return data,
            Ok(n) => data.extend_from_slice(&chunk[..n]),
            Err(_) => fail("read failed"),
        }
    }
}

/// The whole program body; both `main` forms call it and it never returns.
fn run() -> ! {
    let argv: Vec<String> = support::args();
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();

    let mut base64 = false;
    let mut i = 1;
    // getopt consumes leading options; the first non-option begins the operands.
    while i < args.len() {
        let a = args[i];
        if a == "--" {
            i += 1;
            break;
        }
        if a == "-m" {
            base64 = true;
            i += 1;
            continue;
        }
        if a.starts_with('-') && a.len() > 1 {
            fail("unknown option");
        }
        break;
    }
    let operands: Vec<&str> = args[i..].to_vec();

    let (mode, data, name) = match operands.len() {
        2 => {
            let (input, name) = (operands[0], operands[1]);
            let mut st: Stat = unsafe { core::mem::zeroed() };
            minix_sys::stat(input, &mut st).unwrap_or_else(|_| fail("cannot stat input"));
            let fd = minix_sys::open(input, 0, 0).unwrap_or_else(|_| fail("cannot open input"));
            let data = slurp(fd);
            let _ = minix_sys::close(fd);
            (st.st_mode & 0o777, data, name)
        }
        1 => {
            let old = minix_sys::umask(0o666).unwrap_or(0);
            (0o666 & !old, slurp(support::STDIN), operands[0])
        }
        _ => usage(),
    };

    let mut out: Vec<u8> = Vec::new();
    if base64 {
        out.extend(b"begin-base64 ");
    } else {
        out.extend(b"begin ");
    }
    out.extend(octal(mode));
    out.push(b' ');
    out.extend(name.as_bytes());
    out.push(b'\n');
    if base64 {
        out.extend(base64_stream(&data));
    } else {
        out.extend(encode_stream(&data));
    }
    if base64 {
        out.extend(b"====\n");
    } else {
        out.extend(b"end\n");
    }

    support::emit(&out);
    support::terminate(0);
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
