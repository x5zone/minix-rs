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

use minix_textfilter::window::{HeadArgError, HeadArgs, HeadWindow, LineWindow, parse_head_args};

fn run() -> ! {
    let argv: Vec<String> = support::args();
    // C 的诊断行与 usage 行都以 `getprogname()` 开头（`head.c:200-202` 把程序名
    // 当 `%s` 递进去），所以不能写死命令名。
    let prog = support::prog_name(&argv);
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    let HeadArgs {
        count,
        bytecnt,
        qflag,
        vflag,
        operands,
    } = match parse_head_args(&refs[1..]) {
        Ok(parsed) => parsed,
        // libc 的那一行在前，usage 在后（`getopt.c:101-104`、`:132-136`）。
        Err(HeadArgError::Getopt(report)) => {
            support::warn(report.line(prog).as_bytes());
            usage(prog)
        }
        Err(HeadArgError::IllegalCount { byte, value }) => {
            // C `errx(1, "illegal byte count -- %s", optarg)`（`head.c:92-102`）：
            // 走 `vwarnx`，所以后面不缀 `errno` 句子。
            let kind = if byte { "byte" } else { "line" };
            support::warn(format!("{prog}: illegal {kind} count -- {value}\n").as_bytes());
            support::terminate(1);
        }
    };

    // C `if (*argv) for each file: fopen, banner when (vflag || (qflag==0 &&
    // argc>1)) with a leading blank between files, then head(); else head(stdin).
    // The banner comes after the successful open because C `continue`s past a file
    // it cannot open without printing one.
    let show_banner = vflag || (!qflag && operands.len() > 1);
    let mut first = true;
    let mut status = 0;

    if operands.is_empty() {
        head_stream(&support::read_stdin(), count, bytecnt);
        support::terminate(0);
    }
    for operand in &operands {
        let data = match support::read_file(operand) {
            Ok(data) => data,
            // C `head.c:124-127` 是 `warn("%s", *argv)` + `eval = 1` + `continue`：
            // 程序名、操作数、`strerror` 句子三个成分缺一不可（旧写法把 `warn(3)`
            // 当成 `warnx(3)` 用）。
            Err(code) => {
                support::warn_line(prog, operand, code);
                status = 1;
                continue;
            }
        };
        if show_banner {
            let sep = if first { "" } else { "\n" };
            support::emit(format!("{sep}==> {operand} <==\n").as_bytes());
        }
        first = false;
        head_stream(&data, count, bytecnt);
    }
    support::terminate(status)
}

/// Emit the head of one stream: the first `bytecnt` raw bytes in byte mode
/// (`head.c`'s `fread` loop), else the first `count` lines through the window.
fn head_stream(input: &[u8], count: usize, bytecnt: usize) {
    if bytecnt > 0 {
        support::emit(first_bytes_of(input, bytecnt));
        return;
    }
    // `parse_head_args` has already refused a non-positive count with C's
    // `illegal line count`/`illegal byte count` message (`head.c:90-103`), so
    // the window always opens here.
    let Ok(mut window) = HeadWindow::new(count) else {
        support::terminate(1);
    };
    for line in support::lines_of(input) {
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

/// `head` 的 usage 行，逐字对位 `head.c:199-203`（`"usage: %s [-n lines] [file ...]\n"`
/// 加 `getprogname()`，退 1）。注意 C 这一行里根本没有 `-c`/`-q`/`-v` 三个字母——
/// 上游就少列了，本壳照C而不替它修正清单。
fn usage(prog: &str) -> ! {
    support::warn(format!("usage: {prog} [-n lines] [file ...]\n").as_bytes());
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
