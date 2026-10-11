//! Minix-RS tail — last lines of a stream.
//!
//! Ground truth: `minix3/usr.bin/tail/tail.c` (NetBSD), `tail [-n count |
//! -count] [file ...]` with a default of ten lines. The ring capacity is fixed
//! at 32 (the library's `TailWindow`), so counts above 32 are rejected as out
//! of range — a declared cap. With no operand tail reads standard input; with
//! operands it prints the last `count` lines of each file, prefixing a
//! `==> name <==` banner when there is more than one file (this NetBSD `tail`
//! has no `-q`/`-v` to change that) with a blank line between files, and an
//! unopenable/unstatable file warns and is skipped (exit status stays
//! nonzero). Operands open with `fopen`, so `-` is a literal file name.
//!
//! Not modelled (rejected rather than mistaken for an operand): the byte mode
//! `-c` and block mode `-b` (the ring is line-based), the reverse mode `-r`,
//! the follow modes `-f`/`-F`, the forward-offset form `tail -n +N`, and the
//! obsolete bare-number syntax beyond `-N`.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;
use alloc::format;
use alloc::vec;

use alloc::string::{String, ToString};
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_sys::{close, open, read, Fd};
use minix_textfilter::window::{LineWindow, TailWindow};

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let mut count: usize = 10;
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
            "-r" | "-f" | "-F" | "-c" | "-b" => {
                support::warn(
                    b"tail: this option is not modelled (line-based ring, forward only)\n",
                );
                support::terminate(1);
            }
            _ if a.starts_with('-')
                && a.len() > 1
                && a[1..].bytes().all(|b| b.is_ascii_digit()) =>
            {
                count = parse_count(&a[1..]);
            }
            _ => operands.push(a.to_string()),
        }
        index += 1;
    }

    // The fixed-capacity ring rejects 0 and counts above 32 (declared cap).
    if TailWindow::new(count).is_err() {
        support::terminate(1);
    }

    // C `for each file: fopen+fstat (ierr on failure), banner when argc>1,
    // forward()`. No operands read stdin.
    let show_banner = operands.len() > 1;
    let mut first = true;
    let mut status = 0;

    if operands.is_empty() {
        tail_stream(support::STDIN, count);
        support::terminate(0);
    }
    for operand in &operands {
        let fd = match open(operand, 0, 0) {
            Ok(fd) => fd,
            Err(_) => {
                support::warn(format!("tail: {operand}\n").as_bytes());
                status = 1;
                continue;
            }
        };
        if show_banner {
            let sep = if first { "" } else { "\n" };
            support::emit(format!("{sep}==> {operand} <==\n").as_bytes());
        }
        first = false;
        tail_stream(fd, count);
        //[SD19-gate] SD-19: close 后 fd 不再使用
        #[allow(clippy::let_underscore_must_use)]
        let _ = close(fd);
    }
    support::terminate(status)
}

/// Emit the last `count` lines of one open descriptor (forward mode).
fn tail_stream(fd: Fd, count: usize) {
    let mut window = match TailWindow::new(count) {
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
    // The forward-offset form `-n +N` is not modelled; reject a leading `+`
    // rather than silently folding it into a plain count.
    if word.starts_with('+') {
        support::warn(b"tail: -n +N forward-offset is not modelled\n");
        support::terminate(1);
    }
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
