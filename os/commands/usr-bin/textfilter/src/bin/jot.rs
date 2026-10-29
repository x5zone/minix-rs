//! Minix-RS jot — print sequential or random data.
//!
//! Ground truth: `minix3/usr.bin/jot/jot.c` (NetBSD), usage
//! `jot [-cnr] [-b word] [-p precision] [-s string] [-w word]
//! [reps [begin [end [step | seed]]]]`. The deciding half (right-to-
//! left operand derivation, format selection, rendering) is the
//! library's `jot` module; this program parses the command line and
//! drains the value stream to stdout. The `-r` seed defaults to the
//! hosted time/pid mix until the target time seam lands; libc
//! `random()` is not reproduced bit-exactly (registered corner). The
//! hosted-versus-target seams are the echo template's
//! (`os/commands/bin/fileops/src/bin/echo.rs`): argv via
//! `std::env::args`, termination via the host runtime.
//!
//! 前门有四类出口，句式各有主人：libc `getopt` 的两行（集外字母、带值字母缺值）由
//! [`minix_textfilter::GetoptReport`] 单源且后面必接一行 usage（`jot.c:151-152` 的
//! `default: usage()`）；`-p` 的值坏、`reps`/`step` 坏、操作数超过四个都是 `errx(3)` 的
//! 单独一行，句子由 `jot.c` 自己写、程序名跟着 `getprogname()`，都不接 usage；
//! 一个操作数都没有（含光写 `--`）是 usage，没有 libc 那一行。

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::jot::{
    HAVE_BEGIN, HAVE_ENDER, HAVE_REPS, HAVE_STEP, Jot, JotArgError, JotError, get_prec,
    operand_to_number, parse_jot_args, parse_reps, parse_step, seed_from_step,
};

fn run() -> ! {
    let argv: Vec<Vec<u8>> = support::args_bytes();
    // `jot` 不读标准输入也不开档（`jot.c` 没有一条 `fopen`），所以本支是整个参数缝上
    // **唯一没有操作数侧 `&str` 口**的薄壳：程序名之外一处都不用折。
    let prog = String::from_utf8_lossy(support::prog_name_bytes(&argv));
    let args: Vec<&[u8]> = argv.iter().map(Vec::as_slice).collect();
    let mut seed_word: Option<&[u8]> = None;
    // 选项环（`jot.c:150` 的 `"b:cnp:rs:w:"`）。libc 那两行与 `usage()` 是**两行**，
    // 而 `-p` 的 `errx` 只有自己那一行——两支不是一支的详略版。
    //
    // 字母位走 `line_bytes`：`src/jot.rs` 的取法本就按字节（`char::from(bytes[pos])`、
    // 缺值那一支也是 `char::from(ch)`），非 ASCII 字母经 `line` 会被 UTF-8 编成两字节。参照件
    // `tmp/jot-317/jot_ref` 现量 `-é`、`-cé` 都是 `jot: unknown option -- ` + 单个 `c3` +
    // 换行，机制在 `getopt.c:79` 的 `optopt = *place++` 与 `:103` 的 `%c`。
    let (mut options, operands) = match parse_jot_args(&args[1..]) {
        Ok(parsed) => (parsed.options, parsed.operands),
        Err(JotArgError::Getopt(report)) => {
            support::warn(&report.line_bytes(&prog));
            usage(&prog);
        }
        Err(JotArgError::BadPrecision) => {
            // `jot.c:164` 那句是固定句，程序名跟着 `getprogname()`。
            diagnose(&prog, b"Bad precision value");
            support::terminate(1);
        }
    };

    // Examine args right to left, falling through the cases
    // (jot.c:184-226). `-` keeps the default for that slot.
    // 这个 `switch (argc)` 排在任何操作数被解读之前，所以「一个操作数都没有」走 usage、
    // 「五个以上」走那句 `Too many arguments`，两者都不先看第一个操作数的脸色。
    if operands.is_empty() {
        usage(&prog);
    }
    if operands.len() > 4 {
        // `jot.c:223-225`：第五个操作数起只有一句 `errx`，报的是**第五个**（`argv[4]`），
        // 前四个一个都不解读。
        too_many_arguments(&prog, operands[4]);
    }
    let mut inferred_begin = 0usize;
    let mut inferred_ender = 0usize;
    if let Some(reps_arg) = operands.first().filter(|s| **s != b"-") {
        match parse_reps(reps_arg) {
            Ok(r) => {
                options.reps = r;
                options.have |= HAVE_REPS;
            }
            Err(_) => bad_reps(&prog, reps_arg),
        }
    }
    if let Some(begin_arg) = operands.get(1).filter(|s| **s != b"-") {
        // 空操作数在 C 是把下标算成 `-1` 的越界读（`jot.c:196`），值取决于那块内存前面
        // 是什么；宿主参照件现量 `jot 3 '' 5` 交回 `1 3 5`，也就是那一格留着缺省，本层
        // 按这个现量留缺省而不复制未定义行为（登记 `JOT-EMPTY-OPERAND-READS-OOB`）。
        if let Some(value) = operand_to_number(begin_arg) {
            options.begin = value;
        }
        options.have |= HAVE_BEGIN;
        if options.prec.is_none() {
            inferred_begin = get_prec(begin_arg);
            options.prec = Some(inferred_begin);
        }
    }
    if let Some(ender_arg) = operands.get(2).filter(|s| **s != b"-") {
        if let Some(value) = operand_to_number(ender_arg) {
            options.ender = value;
        }
        options.have |= HAVE_ENDER;
        inferred_ender = get_prec(ender_arg);
    }
    if let Some(step_arg) = operands.get(3).filter(|s| **s != b"-") {
        match parse_step(step_arg) {
            Ok(s) => {
                options.step = s;
                options.have |= HAVE_STEP;
                if options.randomize {
                    seed_word = Some(step_arg);
                }
            }
            Err(_) => bad_step(&prog, step_arg),
        }
    }
    // Maximum precision of begin/ender (jot.c:206-209).
    options.prec = Some(
        options
            .prec
            .unwrap_or(0)
            .max(inferred_begin)
            .max(inferred_ender),
    );

    let seed: u64 = match seed_word {
        // `jot.c:118` 是 `srandom((unsigned long) step)`：先按 C 的截断取整，再按无符号回绕。
        // 所以**负数与小数都是合法种子**（`-5` 能用，`-0.5` 截成 0），本层此前那句
        // `v >= 0.0` 把参照件接受的形报成 `Bad step value` 是自创的判据；`strtod`
        // 的全串检查（`parse_step`）没动，坏值照样报同一句。
        Some(s) => match parse_step(s) {
            Ok(v) => seed_from_step(v),
            Err(_) => bad_step(&prog, s),
        },
        None => {
            // Clock + pid mix (C jot seeds from time(2)); the clock half
            // rides the `support::epoch_micros` twin, the pid half the
            // kernel getpid wrapper both sides.
            support::epoch_micros().unwrap_or(0) ^ ((minix_sys::getpid().unwrap_or(0) as u64) << 16)
        }
    };

    match Jot::new(options, seed) {
        Ok(mut jot) => {
            let mut out: Vec<u8> = Vec::new();
            while let Some(item) = jot.next() {
                out.extend_from_slice(&item);
                match jot.separator_after() {
                    Some(sep) => out.extend_from_slice(sep),
                    None => break,
                }
                // Unbounded runs (reps == 0) drain until the write
                // side fails; flush periodically.
                if out.len() >= 4096 && !support::write_ok(&out) {
                    support::terminate(1);
                }
                if out.len() >= 4096 {
                    out.clear();
                }
            }
            if !jot.nofinalnl() {
                out.push(b'\n');
            }
            support::emit(&out);
            support::terminate(0);
        }
        Err(err) => {
            support::warn(&error_bytes(&prog, &err));
            support::terminate(1);
        }
    }
}

/// 写出「程序名 + `: ` + 那一句」这一行。**不退**——C 的 `errx` 自己 `exit`，本层的
/// 退出处在调用方，两支（`Bad precision value` 与渲染失败那一族）共用这一个写法出口。
fn diagnose(prog: &str, message: &[u8]) {
    let mut line = Vec::with_capacity(prog.len() + message.len() + 3);
    line.extend_from_slice(prog.as_bytes());
    line.extend_from_slice(b": ");
    line.extend_from_slice(message);
    line.push(b'\n');
    support::warn(&line);
}

/// 把句子的前后段与 argv 的原字节接起来（C 那四支带 `%s` 的句子共用：`jot.c:189`、
/// `:215`、`:225`、`:385`/`:396`）。值段不经任何字符解码——一枚裸 `ff` 在句子里就是
/// 一枚 `ff`，折过的形态会多出两枚字节。
fn quoted(prefix: &[u8], value: &[u8], suffix: &[u8]) -> Vec<u8> {
    let mut message = Vec::with_capacity(prefix.len() + value.len() + suffix.len());
    message.extend_from_slice(prefix);
    message.extend_from_slice(value);
    message.extend_from_slice(suffix);
    message
}

/// `errx(3)` 那句的一整行：程序名跟着 `getprogname()`，短语按上表逐支取 C 的原句
/// （两个空格的那两句、带反引号的那一句、问号结尾的那一句都是逐字对位）。
fn error_bytes(prog: &str, err: &JotError) -> Vec<u8> {
    let message = match err {
        JotError::BadPrecision => b"Bad precision value".to_vec(),
        JotError::BadReps(s) => quoted(b"Bad reps value:  ", s, b""),
        JotError::BadStep(s) => quoted(b"Bad step value:  ", s, b""),
        JotError::InvalidFormat(word) => quoted(b"unknown or invalid format `", word, b"'"),
        JotError::FormatWantsString => b"cannot convert numeric data to strings".to_vec(),
        JotError::ImpossibleStepsize => b"Impossible stepsize".to_vec(),
        JotError::MustSpecifyBegin => b"Must specify begin if reps == 0".to_vec(),
        JotError::InfiniteUnbounded => b"Infinite sequences cannot be bounded".to_vec(),
        JotError::TooManyArgs(s) => quoted(b"Too many arguments.  What do you mean by ", s, b"?"),
    };
    let mut line = Vec::with_capacity(prog.len() + message.len() + 3);
    line.extend_from_slice(prog.as_bytes());
    line.extend_from_slice(b": ");
    line.extend_from_slice(&message);
    line.push(b'\n');
    line
}

fn bad_reps(prog: &str, arg: &[u8]) -> ! {
    support::warn(&error_bytes(prog, &JotError::BadReps(arg.to_vec())));
    support::terminate(1);
}

fn bad_step(prog: &str, arg: &[u8]) -> ! {
    support::warn(&error_bytes(prog, &JotError::BadStep(arg.to_vec())));
    support::terminate(1);
}

/// `jot.c:223-225`：第五个操作数起只有一句 `errx`，报的是**第五个**操作数
/// （`argv[4]`，即 `argv += optind` 之后的下标 4），前四个一个都不解读。
fn too_many_arguments(prog: &str, fifth: &[u8]) -> ! {
    support::warn(&error_bytes(prog, &JotError::TooManyArgs(fifth.to_vec())));
    support::terminate(1);
}

fn usage(prog: &str) -> ! {
    support::warn(
        format!(
            "usage: {prog} [-cnr] [-b word] [-p precision] [-s string] [-w word] \
             [reps [begin [end [step | seed]]]]\n"
        )
        .as_bytes(),
    );
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
