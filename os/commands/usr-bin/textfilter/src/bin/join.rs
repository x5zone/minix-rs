//! Minix-RS `join` — the doing half over `minix_textfilter::join`.
//!
//! Ground truth: `minix3/usr.bin/join/join.c`. The merge and field logic are
//! the library (`join_lines`/`JoinOptions`); this program parses the option
//! words, reads the two file operands (a `-` names standard input), feeds the
//! newline-stripped lines to the library, and emits the result.
//!
//! Wired: default join, `-t` (delimiter), `-1`/`-2` (per-file join field),
//! `-a 1`/`-a 2` (unmatched lines), `-o` (output field list, `0`/`F.N`), `-e`
//! (empty-field replacement), `-j field` (both join fields at once — the BSD
//! alias for `-1 field -2 field`) and `-v fileno` (that file's unpairable lines
//! only, with matched output suppressed). `-a` and `-v` are mutually exclusive
//! (`join.c:234-235`). Option values take either form the BSD `getopt`
//! (`join.c:133`) allows: attached (`-t,`, `-a1`, `-o0,1`) or as the following
//! argument (`-t ,`). Declared residual: the C `obsolete(argv)` pre-pass
//! (`join.c:132`) that rewrites legacy bare `-a` (both files) and `-j[12]` forms
//! — not modelled here.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use minix_textfilter::join::{JoinError, join_lines, parse_join_args};

/// Read a whole standard-input stream (a `-` operand).
fn read_stdin() -> Vec<u8> {
    let mut out = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match minix_sys::read(support::STDIN, &mut chunk) {
            Ok(0) => break,
            Ok(n) => out.extend_from_slice(&chunk[..n]),
            Err(_) => break,
        }
    }
    out
}

/// Split a buffer into newline-stripped lines, dropping the empty tail that
/// a newline-terminated file leaves.
fn line_refs(buf: &[u8]) -> Vec<&[u8]> {
    let mut lines: Vec<&[u8]> = buf.split(|&b| b == b'\n').collect();
    if lines.last() == Some(&&b""[..]) {
        lines.pop();
    }
    lines
}

/// C 的 `errx` 那一族：程序名（`getprogname()`）、短语、不缀句子，写完退 1。
fn fail(prog: &str, message: &str) -> ! {
    support::warn(format!("{prog}: {message}\n").as_bytes());
    support::terminate(1)
}

/// `join.c:630-637`：`fprintf(stderr, "usage: %s ...\n            ...\n", getprogname())`——
/// **两行**，名字只出现在第一行（第二行十二个空格的缩进把它接在第 12 列下面）。
fn usage(prog: &str) -> ! {
    support::warn(
        format!("usage: {prog} [-a fileno | -v fileno] [-e string] [-j fileno field]\n").as_bytes(),
    );
    support::warn(b"            [-o list] [-t char] [-1 field] [-2 field] file1 file2\n");
    support::terminate(1)
}

/// Map a [`JoinError`] to C's `warnx` line for that branch (each `join.c` arm
/// warns then calls `usage()`), then exit 1.
///
/// The two lines C emits are still collapsed to the specific message here: the
/// per-option wording C uses (`"-1 option field number less than 1"`,
/// `"illegal field number -- <值>"`, …) needs the option letter and its argument
/// text, which the deciding half's `JoinError` throws away — registered as the
/// second slice of this batch rather than half-wired with invented wording.
fn join_usage_fail(prog: &str, e: JoinError) -> ! {
    match e {
        // `join.c:236-237`：`if (argc != 2) usage();`——本壳的 parse 一并管选项形状。
        JoinError::Usage => usage(prog),
        JoinError::FieldLessThan1 => fail(prog, "option field number less than 1"),
        JoinError::IllegalFieldNumber => fail(prog, "illegal field number"),
        JoinError::FileNumberNot12 => fail(prog, "option file number not 1 or 2"),
        JoinError::IllegalFileNumber => fail(prog, "illegal file number"),
        JoinError::BadTabChar => fail(prog, "illegal tab character specification"),
        JoinError::BadOutputField => fail(prog, "malformed -o option field"),
        // `join.c:235`：这一支是 `errx`，印完就走，没有 usage。
        JoinError::AAndVMutuallyExclusive => fail(prog, "-a and -v options mutually exclusive"),
    }
}

/// The whole program body; both `main` forms call it and it never returns.
fn run() -> ! {
    let argv: Vec<String> = support::args();
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();
    // C 的 `join` 不调 `setprogname`；`getprogname()` 仍由
    // `minix3/lib/csu/common/crt0-common.c:156-165` 从 `argv[0]` 起好那一段，
    // 所以它的 `errx`/`err`/`usage` 三族都从那一段开头。
    let prog = support::prog_name(&argv);

    let (opts, files) = match parse_join_args(&args[1..]) {
        Ok(a) => (a.opts, a.files),
        Err(e) => join_usage_fail(prog, e),
    };

    // `join.c:250-251`：两个操数都是 `-` 时两个 `FILE*` 都等于 `stdin`，C 直接
    // `errx(1, "only one input file may be stdin")`——本壳此前没有这一支，会把同一条
    // 输入读两遍。C 在 `slurp` 之前护住它，这里也放在两次读之前。
    if files[0] == "-" && files[1] == "-" {
        fail(prog, "only one input file may be stdin");
    }

    let b1 = if files[0] == "-" {
        read_stdin()
    } else {
        // `join.c:241-244`: `fopen` 失手就是 `err(1, "%s", *argv)`——操数名与那个号的
        // 句子一起出，然后整个程序就此结束。
        match support::read_file(files[0]) {
            Ok(bytes) => bytes,
            Err(code) => {
                support::warn_line(prog, files[0], code);
                support::terminate(1);
            }
        }
    };
    let b2 = if files[1] == "-" {
        read_stdin()
    } else {
        match support::read_file(files[1]) {
            Ok(bytes) => bytes,
            Err(code) => {
                support::warn_line(prog, files[1], code);
                support::terminate(1);
            }
        }
    };

    let out = join_lines(&line_refs(&b1), &line_refs(&b2), &opts);
    let mut rendered = Vec::new();
    for line in &out {
        rendered.extend_from_slice(line);
        rendered.push(b'\n');
    }
    support::emit(&rendered);
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
