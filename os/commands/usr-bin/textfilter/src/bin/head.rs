//! Minix-RS head — first lines of a stream.
//!
//! Ground truth: `minix3/usr.bin/head/head.c` (NetBSD), `head [-n count |
//! -count] [-qv] [-c count] [file ...]` with a default of ten lines. The
//! window capacity is fixed at 32 (the library keeps borrowed lines in a
//! fixed array, `window.rs` `HeadWindow`), so counts above 32 are rejected as
//! out of range — a declared cap. With no operand head reads standard input;
//! with operands it prints the first `count` lines of each file, prefixing a
//! `==> name <==` banner when there is more than one file (always under `-v`,
//! never under `-q`), with a blank line between files. Operands are opened
//! with `fopen`, so `-` is a literal file name (not stdin, unlike `cut`), and
//! an unopenable file warns and is skipped (exit status stays nonzero). The
//! byte mode `-c` is not modelled (the window is line-based) and is rejected.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;
use alloc::format;
use alloc::vec;

use alloc::string::{String, ToString};
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_sys::{close, open, read, Fd};
use minix_textfilter::window::{HeadWindow, LineWindow};

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let mut count: usize = 10;
    let mut qflag = false;
    let mut vflag = false;
    let mut operands: Vec<String> = Vec::new();
    let mut index = 1;
    let mut no_more_opts = false;
    while index < argv.len() {
        let a = argv[index].as_str();
        if no_more_opts {
            operands.push(a.to_string());
            index += 1;
            continue;
        }
        match a {
            "--" => no_more_opts = true,
            "-n" => {
                index += 1;
                if index >= argv.len() {
                    support::terminate(1);
                }
                count = parse_count(&argv[index]);
            }
            "-q" => qflag = true,
            "-v" => vflag = true,
            "-c" => {
                support::warn(b"head: -c byte mode is not modelled\n");
                support::terminate(1);
            }
            _ if a.starts_with('-')
                && a.len() > 1
                && a[1..].bytes().all(|b| b.is_ascii_digit()) =>
            {
                // Obsolete `-N` line count (head.c `obsolete`).
                count = parse_count(&a[1..]);
            }
            _ => operands.push(a.to_string()),
        }
        index += 1;
    }

    // Reject counts the fixed-capacity window cannot hold (also rejects 0, as
    // `HeadWindow::new` does — the current line-based behaviour).
    if HeadWindow::new(count).is_err() {
        support::terminate(1);
    }

    // C `if (*argv) for each file: fopen, banner when (vflag || (qflag==0 &&
    // argc>1)) with a leading blank between files, then head(); else head(stdin)`.
    let show_banner = vflag || (!qflag && operands.len() > 1);
    let mut first = true;
    let mut status = 0;

    if operands.is_empty() {
        head_stream(support::STDIN, count);
        support::terminate(0);
    }
    for operand in &operands {
        let fd = match open(operand, 0, 0) {
            Ok(fd) => fd,
            Err(_) => {
                support::warn(format!("head: {operand}\n").as_bytes());
                status = 1;
                continue;
            }
        };
        if show_banner {
            let sep = if first { "" } else { "\n" };
            support::emit(format!("{sep}==> {operand} <==\n").as_bytes());
        }
        first = false;
        head_stream(fd, count);
        //[SD19-gate] SD-19: close 后 fd 不再使用
        #[allow(clippy::let_underscore_must_use)]
        let _ = close(fd);
    }
    support::terminate(status)
}

/// Emit the first `count` lines of one open descriptor.
fn head_stream(fd: Fd, count: usize) {
    let mut window = match HeadWindow::new(count) {
        Ok(window) => window,
        Err(_) => support::terminate(1),
    };
    let input = read_fd(fd);
    for line in support::lines_of(&input) {
        window.push(line);
    }
    let mut survivors: Vec<&str> = vec![""; count.min(32)];
    let drained = window.drain(&mut survivors);
    for line in &survivors[..drained] {
        let mut out = line.as_bytes().to_vec();
        out.push(b'\n');
        support::emit(&out);
    }
}

/// Read a descriptor to the end.
fn read_fd(fd: Fd) -> Vec<u8> {
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

fn parse_count(word: &str) -> usize {
    match word.parse::<usize>() {
        Ok(value) => value,
        Err(_) => support::terminate(1),
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
