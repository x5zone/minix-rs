//! Minix-RS head — first lines of a stream.
//!
//! Ground truth: `minix3/usr.bin/head/head.c` (NetBSD), `head [-n count |
//! -count] [-qv] [-c count] [file ...]` with a default of ten lines. The
//! window keeps borrowed lines for however many the count asks for and no more
//! than the input offered (`window.rs` `HeadWindow`), so any count is honoured.
//! With no operand head reads standard input;
//! with operands it prints the first `count` lines of each file, prefixing a
//! `==> name <==` banner when there is more than one file (always under `-v`,
//! never under `-q`), with a blank line between files. Operands are opened
//! with `fopen`, so `-` is a literal file name (not stdin, unlike `cut`), and
//! an unopenable file warns and is skipped (exit status stays nonzero). Byte
//! mode `-c N` emits the first `N` raw bytes of each file, ignoring the line
//! count (`head.c:149-167`); `N` must be positive.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;
use alloc::format;
use alloc::vec;

use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_sys::{close, open, read, Fd};
use minix_textfilter::window::{parse_head_args, HeadArgError, HeadArgs, HeadWindow, LineWindow};

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    let HeadArgs {
        count,
        bytecnt,
        qflag,
        vflag,
        operands,
    } = match parse_head_args(&refs[1..]) {
        Ok(parsed) => parsed,
        Err(HeadArgError::Usage) => usage(),
        Err(HeadArgError::IllegalCount { byte, value }) => {
            let kind = if byte { "byte" } else { "line" };
            support::warn(format!("head: illegal {kind} count -- {value}\n").as_bytes());
            support::terminate(1);
        }
    };

    // `HeadWindow::new` refuses only a zero count (a zero-line head is not a
    // window request). C prints nothing for `-n 0`; refusing it here is the
    // line-based behaviour this shell documents rather than C's.
    if bytecnt == 0 && HeadWindow::new(count).is_err() {
        support::terminate(1);
    }

    // C `if (*argv) for each file: fopen, banner when (vflag || (qflag==0 &&
    // argc>1)) with a leading blank between files, then head(); else head(stdin)`.
    let show_banner = vflag || (!qflag && operands.len() > 1);
    let mut first = true;
    let mut status = 0;

    if operands.is_empty() {
        head_stream(support::STDIN, count, bytecnt);
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
        head_stream(fd, count, bytecnt);
        //[SD19-gate] SD-19: close 后 fd 不再使用
        #[allow(clippy::let_underscore_must_use)]
        let _ = close(fd);
    }
    support::terminate(status)
}

/// Emit the head of one open descriptor: the first `bytecnt` raw bytes in byte
/// mode (`head.c:149-167`), else the first `count` lines through the window.
fn head_stream(fd: Fd, count: usize, bytecnt: usize) {
    let input = read_fd(fd);
    if bytecnt > 0 {
        support::emit(first_bytes_of(&input, bytecnt));
        return;
    }
    let mut window = match HeadWindow::new(count) {
        Ok(window) => window,
        Err(_) => support::terminate(1),
    };
    for line in support::lines_of(&input) {
        window.push(line);
    }
    let want = (window.seen() as usize).min(count);
    let mut survivors: Vec<&str> = vec![""; want];
    let drained = window.drain(&mut survivors);
    for line in &survivors[..drained] {
        let mut out = line.as_bytes().to_vec();
        out.push(b'\n');
        support::emit(&out);
    }
}

/// The first `bytecnt` bytes of `input`, or all of it when shorter — C's byte
/// mode `fread`s up to the remaining count per file (`head.c:149-167`).
fn first_bytes_of(input: &[u8], bytecnt: usize) -> &[u8] {
    &input[..bytecnt.min(input.len())]
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

/// `head`'s usage line, exactly `head.c`'s `usage()` (exit 1).
fn usage() -> ! {
    support::warn(b"usage: head [-n lines] [file ...]\n");
    support::terminate(1);
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

#[cfg(test)]
mod tests {
    use super::first_bytes_of;

    #[test]
    fn first_bytes_truncates_to_count() {
        assert_eq!(first_bytes_of(b"abcdef", 3), b"abc");
    }

    #[test]
    fn first_bytes_short_input_returns_all() {
        assert_eq!(first_bytes_of(b"ab", 10), b"ab");
    }

    #[test]
    fn first_bytes_of_empty_is_empty() {
        assert_eq!(first_bytes_of(b"", 5), b"");
    }
}
