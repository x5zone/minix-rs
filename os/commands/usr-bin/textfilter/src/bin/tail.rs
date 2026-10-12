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
//! Not modelled (rejected rather than mistaken for an operand): the follow modes
//! `-f`/`-F`, the forward-offset form `tail -n +N`, and the obsolete bare-number
//! syntax beyond `-N`. Byte mode `-c N`, block mode `-b N`, and reverse mode
//! `-r` are modelled — C `tail.c`/`reverse.c`: `-r` prints lines bottom-up
//! (`REVERSE` reverses every line, `RLINES` = an explicit `-n` reverses the last
//! `count`), while `-r` combined with `-c`/`-b` is the `RBYTES` byte-window line
//! reverse (`reverse_bytes`).

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
    let mut bytecnt: usize = 0;
    let mut rflag = false;
    let mut n_seen = false;
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
                n_seen = true;
            }
            "-c" => {
                index += 1;
                if index >= argv.len() {
                    support::terminate(1);
                }
                let n = parse_count(&argv[index]);
                if n == 0 {
                    support::terminate(1);
                }
                bytecnt = n;
            }
            "-b" => {
                index += 1;
                if index >= argv.len() {
                    support::terminate(1);
                }
                let n = parse_count(&argv[index]);
                if n == 0 {
                    support::terminate(1);
                }
                bytecnt = block_bytes(n);
            }
            "-r" => rflag = true,
            "-f" | "-F" => {
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
                n_seen = true;
            }
            _ => operands.push(a.to_string()),
        }
        index += 1;
    }

    // The fixed-capacity ring rejects 0 and counts above 32 (declared cap).
    // Byte mode ignores the line count, so it skips this ring-capacity guard.
    if bytecnt == 0 && TailWindow::new(count).is_err() {
        support::terminate(1);
    }

    // C `for each file: fopen+fstat (ierr on failure), banner when argc>1,
    // forward()`. No operands read stdin.
    let show_banner = operands.len() > 1;
    let mut first = true;
    let mut status = 0;

    if operands.is_empty() {
        tail_stream(support::STDIN, count, bytecnt, rflag, n_seen);
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
        tail_stream(fd, count, bytecnt, rflag, n_seen);
        //[SD19-gate] SD-19: close 后 fd 不再使用
        #[allow(clippy::let_underscore_must_use)]
        let _ = close(fd);
    }
    support::terminate(status)
}

/// Emit the tail of one open descriptor. Byte mode (`-c`/`-b`) prints the last
/// `bytecnt` raw bytes. Reverse mode `-r` prints lines bottom-up: `RLINES` (an
/// explicit `-n`) the last `count` lines reversed, else `REVERSE` every line
/// reversed. The default forward path keeps the last `count` lines via the ring.
fn tail_stream(fd: Fd, count: usize, bytecnt: usize, rflag: bool, n_seen: bool) {
    let input = read_fd(fd);
    if bytecnt > 0 {
        // `-r` with a byte/block count is C `RBYTES` (`reverse.c` `r_reg` clamps
        // the backward walk to `off` bytes): print the trailing `bytecnt`-byte
        // window's lines bottom-up. Without `-r`, `-c`/`-b` prints raw last bytes.
        if rflag {
            for line in reverse_bytes(&input, bytecnt) {
                emit_line(line);
            }
        } else {
            support::emit(last_bytes_of(&input, bytecnt));
        }
        return;
    }
    let lines = support::lines_of(&input);
    if rflag {
        for line in reverse_selection(&lines, count, n_seen) {
            emit_line(line);
        }
        return;
    }
    let mut window = match TailWindow::new(count) {
        Ok(window) => window,
        Err(_) => support::terminate(1),
    };
    for line in lines {
        window.push(line);
    }
    let mut survivors: Vec<&str> = vec![""; count.min(32)];
    let drained = window.drain(&mut survivors);
    for line in &survivors[..drained] {
        emit_line(line);
    }
}

fn emit_line(line: &str) {
    let mut out = line.as_bytes().to_vec();
    out.push(b'\n');
    support::emit(&out);
}

/// The line order C `tail -r` prints: `RLINES` (an explicit `-n`) selects the
/// last `count` lines then reverses them; bare `-r` (`REVERSE`) reverses every
/// line (`reverse.c` `r_reg` walks from the end, stopping after `off` lines when
/// `RLINES`, else at the start).
fn reverse_selection<'a>(lines: &[&'a str], count: usize, n_seen: bool) -> Vec<&'a str> {
    let selected: &[&'a str] = if n_seen {
        &lines[lines.len().saturating_sub(count)..]
    } else {
        lines
    };
    selected.iter().rev().copied().collect()
}

/// C `tail -r` with `-c`/`-b` (`RBYTES`, `reverse.c` `r_reg`): the lines of the
/// trailing `bytecnt`-byte window, printed bottom-up. The window can begin
/// mid-line; r_reg still emits that leading fragment (its leftover `WR`), which
/// is precisely what a byte-window line split yields, so the reverse is exact.
fn reverse_bytes(input: &[u8], bytecnt: usize) -> Vec<&str> {
    let window = last_bytes_of(input, bytecnt);
    let lines = support::lines_of(window);
    reverse_selection(&lines, 0, false)
}

/// The last `bytecnt` bytes of `input`, or all of it when shorter — C `tail -c`
/// prints the final N bytes of each file.
fn last_bytes_of(input: &[u8], bytecnt: usize) -> &[u8] {
    &input[input.len().saturating_sub(bytecnt)..]
}

/// C `tail -b N` counts in 512-byte blocks (`tail.c` `ARG(512, FBYTES, RBYTES)`):
/// `N` blocks are `N * 512` bytes, saturating rather than wrapping.
fn block_bytes(n: usize) -> usize {
    n.saturating_mul(512)
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

#[cfg(test)]
mod tests {
    use super::{block_bytes, last_bytes_of, reverse_bytes, reverse_selection};

    #[test]
    fn last_bytes_truncates_to_count() {
        assert_eq!(last_bytes_of(b"abcdef", 3), b"def");
    }

    #[test]
    fn last_bytes_short_input_returns_all() {
        assert_eq!(last_bytes_of(b"ab", 10), b"ab");
    }

    #[test]
    fn last_bytes_of_empty_is_empty() {
        assert_eq!(last_bytes_of(b"", 5), b"");
    }

    #[test]
    fn block_bytes_is_512_per_block() {
        assert_eq!(block_bytes(3), 1536);
        // The byte path then yields the last N blocks worth of bytes.
        assert_eq!(last_bytes_of(b"x", block_bytes(1)), b"x");
    }

    #[test]
    fn reverse_selection_all_reverses_every_line() {
        let lines = ["1", "2", "3"];
        assert_eq!(reverse_selection(&lines, 10, false), ["3", "2", "1"]);
    }

    #[test]
    fn reverse_selection_last_n_reversed() {
        // -r -n 2 on [1,2,3,4] -> last 2 (3,4) reversed -> 4,3.
        let lines = ["1", "2", "3", "4"];
        assert_eq!(reverse_selection(&lines, 2, true), ["4", "3"]);
    }

    #[test]
    fn reverse_selection_count_exceeding_input_is_all() {
        let lines = ["a", "b"];
        assert_eq!(reverse_selection(&lines, 5, true), ["b", "a"]);
    }

    #[test]
    fn reverse_bytes_reverses_lines_of_trailing_window() {
        // -r -c on "aaa\nbbb\nccc\n": last 8 bytes = "bbb\nccc\n" -> ccc,bbb.
        let input = b"aaa\nbbb\nccc\n";
        assert_eq!(reverse_bytes(input, 8), ["ccc", "bbb"]);
        // window starts mid-line (last 7 = "bb\nccc\n"); r_reg emits the leading
        // fragment too, so "bb" survives as its own line.
        assert_eq!(reverse_bytes(input, 7), ["ccc", "bb"]);
        // bytecnt past the input reverses the whole file's lines.
        assert_eq!(reverse_bytes(input, 99), ["ccc", "bbb", "aaa"]);
    }
}
