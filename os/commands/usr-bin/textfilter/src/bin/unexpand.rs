//! Minix-RS unexpand — spaces to tabs.
//!
//! Ground truth: `minix3/usr.bin/unexpand/unexpand.c` (NetBSD). The
//! deciding half (the tabify walk, `-t` stop lists, the `-a`
//! whole-line form) is the library's `unexpand` module. With no operand the
//! program tabifies standard input; with operands it tabifies each file in turn
//!（C `unexpand.c:127-135` 的 `do … while (argc > 0)`：每支档名 `freopen` 进 stdin、
//! 逐行走 `tabify`，行游标按档重置），打不开的档是致命的
//! `err(EXIT_FAILURE, "Cannot open `%s'", …)`。

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::GetoptReport;
use minix_textfilter::unexpand::{
    StopRejection, UnexpandArgs, UnexpandError, UnexpandOptions, parse_unexpand_args, unexpand,
};

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let prog = support::prog_name(&argv);
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    let UnexpandArgs { options, operands } = match parse_unexpand_args(&refs[1..]) {
        Ok(parsed) => parsed,
        Err(error) => arg_error(prog, error),
    };
    // C 在选项环之后、读输入之前把档位表打到 stderr（`unexpand.c:124-125` 的
    // `for (i = 0; i < nstops; i++) fprintf(stderr, "%lu %zu\n", i, tabstops[i])`）——
    // 那是真源里留着的一行调试输出，但它就在外部行为里，参照件逐形量过
    // （`-t 4` → `0 4`；`-t 4,8` → `0 4` 加 `1 8`），所以本壳照搬而不替 C「修正」。
    for (index, stop) in options.stops.iter().enumerate() {
        support::warn(format!("{index} {stop}\n").as_bytes());
    }

    // 读缝走 `bin_support` 的双生面：本文件以前直接拿裸 `minix_sys::open`/`read`，
    // **宿主上读不到任何输入**（同族第六例：`nl`/`fold`/`rev`/`colrm` 的读腿、
    // `split` 的写腿），所以它的输出从来没有被进程级测过。
    if operands.is_empty() {
        emit_tabified(&support::read_stdin(), &options);
        support::terminate(0);
    }
    for operand in &operands {
        // C 的 `freopen(argv[0], "r", stdin)` 失败是 `err(EXIT_FAILURE, "Cannot open `%s'")`
        // （`unexpand.c:129-130`）：程序名、带反引号的档名、`strerror` 句子三段齐备
        // （旧写法少了第三段），退 1。
        match support::read_file(operand) {
            Ok(input) => emit_tabified(&input, &options),
            Err(code) => {
                support::warn_line(prog, &format!("Cannot open `{operand}'"), code);
                support::terminate(1);
            }
        }
    }
    support::terminate(0)
}

/// Tabifies one collected stream and writes it out.
///
/// C 一路 `putchar`，行尾那个换行是输入里的字节（`fgetln` 把它算进长度），
/// 所以**末行没有换行时也不补**（参照件：输入 `no trailing newline` 出去仍是
/// 无换行的一行）。旧写法每行都补一个换行，末行会被凭空加长。
fn emit_tabified(input: &[u8], options: &UnexpandOptions) {
    let text = String::from_utf8_lossy(input);
    let lines = unexpand(text.as_ref(), options);
    let mut out = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        out.extend_from_slice(line.as_bytes());
        let last = index + 1 == lines.len();
        // 只有「最后一行且输入本身不以换行结尾」才不补换行，其余一律照 C 写出输入里的换行。
        if !last || input.ends_with(b"\n") {
            out.push(b'\n');
        }
    }
    support::emit(&out);
}

/// 各支出口的报文与退码，逐支照 C。
fn arg_error(prog: &str, error: UnexpandError<'_>) -> ! {
    match error {
        UnexpandError::Usage => usage(prog),
        // 集外字母与缺值：libc 那一行在前（`getopt.c:101-104`、`:132-136`），
        // C 的 `default: usage()` 在后（`unexpand.c:116-118`）—— 两行。
        UnexpandError::UnknownOption(letter) => {
            support::warn(&GetoptReport::UnknownOption(letter).line_bytes(prog));
            usage(prog)
        }
        UnexpandError::MissingValue(letter) => {
            support::warn(&GetoptReport::MissingValue(letter).line_bytes(prog));
            usage(prog)
        }
        UnexpandError::BadStops(StopRejection::Invalid(piece)) => {
            // C `errx(EXIT_FAILURE, "Invalid tabstop `%s'", tab)`：两段形、不缀 strerror。
            support::warn(format!("{prog}: Invalid tabstop `{piece}'\n").as_bytes());
            support::terminate(1);
        }
        UnexpandError::BadStops(StopRejection::NotIncreasing { spec, previous }) => {
            support::warn(
                format!("{prog}: Bad tabstop spec `{spec}', must be greater than the previous `{previous}'\n")
                    .as_bytes(),
            );
            support::terminate(1);
        }
    }
}

/// C 的 `usage()`（`unexpand.c:69-73`）：**首字母大写**的 `Usage:`、程序名跟着
/// `getprogname()`、带 `[file ...]`、退 1。旧写法是小写 `usage:`、`[-t tablist]`
/// 且少了 `[file ...]`。
fn usage(prog: &str) -> ! {
    support::warn(format!("Usage: {prog} [-a] [-t tabstop] [file ...]\n").as_bytes());
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
