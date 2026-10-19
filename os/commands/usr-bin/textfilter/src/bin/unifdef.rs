//! Minix-RS unifdef — selectively remove C preprocessor conditionals.
//!
//! Ground truth: `minix3/usr.bin/unifdef/unifdef.c`, usage
//! `unifdef [-cdeklst] [-o output] [-Dsym[=val]] [-Usym]
//! [-iDsym[=val]] [-iUsym] ... [file]`. The deciding half (the ten-
//! state #if machine and expression evaluator) is the library's
//! `unifdef` module; this program parses the command line, reads the
//! input (stdin or one file), and reports the exit status: 0 when the
//! input was already clean, 1 when lines were dropped, 2 on trouble.
//! `-d` (debugging) and `-e` (obfuscated-directive tolerance) are
//! accepted and ignored — the DODGY table columns they gate are a
//! registered corner. File access goes through `bin_support` 的双生面
//! （标准输入走 `read_stdin`，操数档走 `read_file`，`-o` 走 `write_file`）。
//! The hosted-versus-target seams are the echo template's
//! (`os/commands/bin/fileops/src/bin/echo.rs`).
//!
//! 前门有三类出口，句式各有主人，而且**这一支的 usage 退的是 2 不是 1**
//! （`unifdef.c:373-379` 那句 `exit(2)`）：① 集外字母与带值字母缺值都先由 libc 印一行
//! （`getopt.c:101-104`、`:132-136`），后面必接 usage；② `-i` 的后随字母不是 `D`/`U`、
//! `-D`/`-U` 的符号名不合 `addsym` 的形，两处 C 都**只打 usage 一行**，没有自己的句子；
//! ③ 环结束后先查「一个符号都没有且没给 `-s`」——`warnx` 一句 + usage 共两行，这一支排在
//! 操数判定之前；两个以上操数是一句 `unifdef: can only do one file`，开不了的操数档名是
//! `err(2, "can't open %s", …)` 的三段形。usage 那一行的程序名**写死在串里**
//! （`unifdef.c:375-377` 没有 `getprogname()`），而 libc 那两行与 `err`/`warnx` 的前缀
//! 跟着 `getprogname()`——同一支命令里两行来历不同。

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::unifdef::{Unifdef, UnifdefArgError, UnifdefError, parse_unifdef_args};

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let prog = support::prog_name(&argv);
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();
    // 选项环（`unifdef.c:273` 的 `"i:D:U:I:o:cdeklst"`）。libc 那两行后面必接 usage，
    // 而 `-i` 的后随字母坏了、符号名不合 `addsym` 的形这两支，C 只有 usage 一行。
    //
    // 字母位走 `line_bytes`：`src/unifdef.rs` 的取法本就按字节（`char::from(bytes[pos])`），
    // 非 ASCII 字母经 `line` 会被 UTF-8 编成两字节。参照件 `tmp/unifdef-318/unifdef_ref` 现量
    // `-é` 与 `-cé` 都是 `unifdef: unknown option -- ` + 单个 `c3` + 换行再跟 usage，
    // 机制在 `getopt.c:79` 的 `optopt = *place++` 与 `:103` 的 `%c`。
    let (options, operands) = match parse_unifdef_args(&args[1..]) {
        Ok(parsed) => (parsed.options, parsed.operands),
        Err(UnifdefArgError::Getopt(report)) => {
            support::warn(&report.line_bytes(prog));
            usage();
        }
        Err(UnifdefArgError::Usage) => usage(),
        Err(UnifdefArgError::TooManySymbols) => {
            support::warn(format!("{prog}: too many symbols\n").as_bytes());
            support::terminate(2);
        }
    };

    // `unifdef.c:327-330`：符号表空且没给 `-s` 时先 `warnx` 一句再接 usage，
    // 这一支排在操数判定之前，所以 `unifdef -- -z` 报的是它而不是「只能一个档」。
    if options.symbols.is_empty() && !options.symlist {
        support::warn(format!("{prog}: must -D or -U at least one symbol\n").as_bytes());
        usage();
    }
    // `unifdef.c:331-332`：两个以上操数是 `errx(2, "can only do one file")` 单独一行。
    if operands.len() > 1 {
        support::warn(format!("{prog}: can only do one file\n").as_bytes());
        support::terminate(2);
    }

    // `unifdef.c:333-341`：一个操数且不是裸 `-` 才开档；裸 `-` 与没有操数同样读标准输入。
    // 开不了的那句是 `err(2, "can't open %s", filename)` 三段形（程序名、操数原文、
    // `strerror` 句子），退码是 2 而不是 1。
    let input = match operands.first().copied() {
        None | Some("-") => support::read_stdin(),
        Some(path) => match support::read_file(path) {
            Ok(bytes) => bytes,
            Err(code) => {
                support::warn_line(prog, &format!("can't open {path}"), code);
                support::terminate(2);
            }
        },
    };
    let text = String::from_utf8_lossy(&input).into_owned();
    // `-o` 的值在操数判定之前就被环收下（`unifdef.c:311-312`），这里先抄一份，
    // 因为 `options` 整体交进决定半之后就拿不到了。
    let output_path = options.output.clone();

    match Unifdef::new(options).run(&text) {
        Ok(outcome) => {
            match output_path.as_deref() {
                Some(path) => {
                    // C 那句是 `err(2, "can't open %s", ofilename)` 三段形；本层的
                    // `write_file` 没有 errno 通道（`Result<(), ()>`），所以这里只能打两段，
                    // 差的那一段登记为 `UNIFDEF-O-ERRNO-LEG`。
                    if support::write_file(path, &outcome.output).is_err() {
                        support::warn(format!("{prog}: can't open {path}\n").as_bytes());
                        support::terminate(2);
                    }
                }
                None => support::emit(&outcome.output),
            }
            support::terminate(outcome.exitstat);
        }
        Err(err) => {
            support::warn(error_text(&err).as_bytes());
            support::terminate(2);
        }
    }
}

fn error_text(err: &UnifdefError) -> String {
    match err {
        UnifdefError::EofInComment => "unifdef: EOF in comment\n".to_string(),
        UnifdefError::InappropriateElif {
            line,
            if_line,
            depth,
        } => {
            if *depth == 0 {
                format!("unifdef: {}: Inappropriate #elif\n", line)
            } else {
                format!(
                    "unifdef: {}: Inappropriate #elif (#if line {} depth {})\n",
                    line, if_line, depth
                )
            }
        }
        UnifdefError::InappropriateElse { line } => {
            format!("unifdef: {}: Inappropriate #else\n", line)
        }
        UnifdefError::InappropriateEndif { line } => {
            format!("unifdef: {}: Inappropriate #endif\n", line)
        }
        UnifdefError::PrematureEof => "unifdef: Premature EOF\n".to_string(),
        UnifdefError::ObfuscatedControlLine { line } => {
            format!("unifdef: {}: Obfuscated preprocessor control line\n", line)
        }
        UnifdefError::TooDeep { line } => {
            format!("unifdef: {}: Too many levels of nesting\n", line)
        }
    }
}

fn usage() -> ! {
    support::warn(
        b"usage: unifdef [-cdeklst] [-o output] [-Dsym[=val]] [-Usym] [-iDsym[=val]] [-iUsym] ... [file]\n",
    );
    support::terminate(2);
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
