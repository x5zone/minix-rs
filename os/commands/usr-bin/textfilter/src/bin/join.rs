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
//! argument (`-t ,`). The C `obsolete(argv)` pre-pass (`join.c:132`) is modelled:
//! legacy bare `-a` becomes the synthetic option letter `\01` (both files emit
//! unpaired lines) and `-j1`/`-j2` become `-1`/`-2`, with malformed `-j3`/`-j12`
//! refused before the option ring ever starts.

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

/// 写出「程序名 + 那一句」这一行。**不退**——C 的 `warnx` 印完继续跑，`errx` 才 `exit`，
/// 两者的差别由调用方决定（`fail` 退、`warn_then_usage` 接 usage）。
fn diagnose(prog: &str, message: &[u8]) {
    let mut line = Vec::with_capacity(prog.len() + message.len() + 3);
    line.extend_from_slice(prog.as_bytes());
    line.extend_from_slice(b": ");
    line.extend_from_slice(message);
    line.push(b'\n');
    support::warn(&line);
}

/// C 的 `errx` 那一族：程序名（`getprogname()`）、短语、不缀句子，写完退 1。短语交回的是
/// **字节**——`join.c:145`/`:156`/`:176`/`:190`/`:592` 那五支把 `optarg` 或整个 token 拼进
/// 句子，那一段是 argv 的原字节，经任何字符解码都会改长度。
fn fail(prog: &str, message: &[u8]) -> ! {
    diagnose(prog, message);
    support::terminate(1)
}

/// 把 `prefix` 与 argv 原字节接成一句（C 的 `"… -- %s"` 那五支共用）。
fn quoted(prefix: &str, value: &[u8]) -> Vec<u8> {
    let mut message = Vec::with_capacity(prefix.len() + value.len());
    message.extend_from_slice(prefix.as_bytes());
    message.extend_from_slice(value);
    message
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

/// C 的 `warnx` 那一族在这里的形状：先一句缀程名的话，再接那两行 usage，退 1
/// （`join.c:141-142` 等八支都是 `warnx(...)` 紧跟 `usage()`）。
fn warn_then_usage(prog: &str, message: &[u8]) -> ! {
    diagnose(prog, message);
    usage(prog)
}

/// 把决定半交回的 [`JoinError`] 落成 C 那支自己的报文。**三支 `warnx`+`usage()`、
/// 三支 `errx`（不接 usage）、一支只出 usage** ——分岔本体（哪个字母、哪个值、
/// 接不接 usage）都在决定半交的变体参数里，所以壳不需要自己发明文案：
/// `join.c:141`/`:145`/`:152`/`:156`/`:171`/`:176`/`:186`/`:190`/`:202`/`:217`/`:222`
/// 与 `fieldarg` 的 `:527-532` 逐支对应。
fn join_error_fail(prog: &str, e: JoinError<'_>) -> ! {
    match e {
        // `join.c:236-237`：`if (argc != 2) usage();`——只打两行 usage，不缀诊断行。
        JoinError::Usage => usage(prog),
        // 表外字母与带值选项缺值：libc 先印自己那一句（`getopt.c:101-104`、
        // `:132-136`），再落 `join.c:226-228` 的 `case '?'` → 同样那两行 usage。
        JoinError::Getopt(report) => {
            // 字母位走 `line_bytes`：`src/join.rs` 的取法本就按字节（`b[1] as char`），而 libc
            // 那一行的字母位本来是一个字节（`getopt.c:79` 的 `optopt = *place++`、`:103` 的
            // `%c`）。参照件 `tmp/join-407/join_ref` 现量 `-é` 是 `join: unknown option -- ` +
            // 单个 `c3` + 换行，再跟那两行 usage。
            support::warn(&report.line_bytes(prog));
            usage(prog)
        }
        // 句子把选项字母写在前面（`"-1 option field number less than 1"`）。
        JoinError::FieldLessThan1(opt) => {
            warn_then_usage(prog, format!("{opt} option field number less than 1").as_bytes())
        }
        JoinError::FileNumberNot12(opt) => {
            warn_then_usage(prog, format!("{opt} option file number not 1 or 2").as_bytes())
        }
        // 这两支 C 不写字母，只把 `optarg` 跟在一对短线后（`"illegal field number -- %s"`）：
        // 值段是 argv 原字节，所以句子按字节拼。
        JoinError::IllegalFieldNumber(value) => {
            warn_then_usage(prog, quoted("illegal field number -- ", value).as_slice())
        }
        JoinError::IllegalFileNumber(value) => {
            warn_then_usage(prog, quoted("illegal file number -- ", value).as_slice())
        }
        // `-t` 那一支同样不写字母（C 也只写 `"illegal tab character specification"`）。
        JoinError::BadTabChar => warn_then_usage(prog, b"illegal tab character specification"),
        // 以下三支 C 走 `errx`：印完就走，没有 usage。
        JoinError::MalformedOutputField => fail(prog, b"malformed -o option field"),
        JoinError::FieldNumbersAre1Based => fail(prog, b"field numbers are 1 based"),
        JoinError::AAndVMutuallyExclusive => fail(prog, b"-a and -v options mutually exclusive"),
        // legacy `-j` 的畸形形：C 在 `obsolete()` 里 `errx(1, "illegal option -- %s", ap)`
        // （`join.c:592`），句子带整个 token（含前导短线与原字节尾巴），**不接 usage**。
        JoinError::IllegalOptionToken(token) => {
            fail(prog, quoted("illegal option -- ", token).as_slice())
        }
    }
}

/// The whole program body; both `main` forms call it and it never returns.
fn run() -> ! {
    let argv: Vec<Vec<u8>> = support::args_bytes();
    let args: Vec<&[u8]> = argv.iter().map(Vec::as_slice).collect();
    // C 的 `join` 不调 `setprogname`；`getprogname()` 仍由
    // `minix3/lib/csu/common/crt0-common.c:156-165` 从 `argv[0]` 起好那一段，
    // 所以它的 `errx`/`err`/`usage` 三族都从那一段开头。程序名交回原字节，只有下面
    // 两条 `&str` 口（开档、`err(1, "%s", name)`）与 usage 那两行需要文本形时折一次。
    let prog = String::from_utf8_lossy(support::prog_name_bytes(&argv));

    let (opts, files) = match parse_join_args(&args[1..]) {
        Ok(a) => (a.opts, a.files),
        Err(e) => join_error_fail(&prog, e),
    };

    // `join.c:250-251`：两个操数都是 `-` 时两个 `FILE*` 都等于 `stdin`，C 直接
    // `errx(1, "only one input file may be stdin")`——本壳此前没有这一支，会把同一条
    // 输入读两遍。C 在 `slurp` 之前护住它，这里也放在两次读之前。`-` 的判据按字节，
    // 因此它与「档名有没有被折过」无关。
    if files[0] == b"-" && files[1] == b"-" {
        fail(&prog, b"only one input file may be stdin");
    }

    // `join.c:241-249`：操数是 `-` 就读标准输入，否则 `fopen`；失手就是
    // `err(1, "%s", *argv)`——操数名与那个号的句子一起出，程序就此结束（**致命**，
    // 第二个操数连试都不试）。开档与那一行都要借 `&str` 口，所以在这之前折一次
    //（在册 `ARGV-BYTE-PATH-IN-LIBS`）。
    let read_operand = |name: &[u8]| -> Vec<u8> {
        if name == b"-" {
            return read_stdin();
        }
        let path = String::from_utf8_lossy(name);
        match support::read_file(&path) {
            Ok(bytes) => bytes,
            Err(code) => {
                support::warn_line(&prog, &path, code);
                support::terminate(1);
            }
        }
    };
    let b1 = read_operand(files[0]);
    let b2 = read_operand(files[1]);

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
