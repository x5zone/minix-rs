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
use alloc::string::{String, ToString};
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::jot::{
    HAVE_BEGIN, HAVE_ENDER, HAVE_REPS, HAVE_STEP, Jot, JotArgError, JotError, get_prec,
    operand_to_number, parse_jot_args, parse_reps, parse_step, seed_from_step,
};

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let prog = support::prog_name(&argv);
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();
    let mut seed_word: Option<String> = None;
    // 选项环（`jot.c:150` 的 `"b:cnp:rs:w:"`）。libc 那两行与 `usage()` 是**两行**，
    // 而 `-p` 的 `errx` 只有自己那一行——两支不是一支的详略版。
    let (mut options, operands) = match parse_jot_args(&args[1..]) {
        Ok(parsed) => (parsed.options, parsed.operands),
        Err(JotArgError::Getopt(report)) => {
            support::warn(report.line(prog).as_bytes());
            usage(prog);
        }
        Err(JotArgError::BadPrecision) => {
            support::warn(format!("{prog}: Bad precision value\n").as_bytes());
            support::terminate(1);
        }
    };

    // Examine args right to left, falling through the cases
    // (jot.c:184-226). `-` keeps the default for that slot.
    // 这个 `switch (argc)` 排在任何操作数被解读之前，所以「一个操作数都没有」走 usage、
    // 「五个以上」走那句 `Too many arguments`，两者都不先看第一个操作数的脸色。
    if operands.is_empty() {
        usage(prog);
    }
    if operands.len() > 4 {
        too_many_arguments(prog, operands[4]);
    }
    let mut inferred_begin = 0usize;
    let mut inferred_ender = 0usize;
    if let Some(reps_arg) = operands.first().filter(|s| **s != "-") {
        match parse_reps(reps_arg) {
            Ok(r) => {
                options.reps = r;
                options.have |= HAVE_REPS;
            }
            Err(_) => bad_reps(prog, reps_arg),
        }
    }
    if let Some(begin_arg) = operands.get(1).filter(|s| **s != "-") {
        options.begin = operand_to_number(begin_arg);
        options.have |= HAVE_BEGIN;
        if options.prec.is_none() {
            inferred_begin = get_prec(begin_arg);
            options.prec = Some(inferred_begin);
        }
    }
    if let Some(ender_arg) = operands.get(2).filter(|s| **s != "-") {
        options.ender = operand_to_number(ender_arg);
        options.have |= HAVE_ENDER;
        inferred_ender = get_prec(ender_arg);
    }
    if let Some(step_arg) = operands.get(3).filter(|s| **s != "-") {
        match parse_step(step_arg) {
            Ok(s) => {
                options.step = s;
                options.have |= HAVE_STEP;
                if options.randomize {
                    seed_word = Some(step_arg.to_string());
                }
            }
            Err(_) => bad_step(prog, step_arg),
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
        Some(s) => match parse_step(&s) {
            Ok(v) => seed_from_step(v),
            Err(_) => bad_step(prog, &s),
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
                out.extend_from_slice(item.as_bytes());
                match jot.separator_after() {
                    Some(sep) => out.extend_from_slice(sep.as_bytes()),
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
            support::warn(error_text(prog, err).as_bytes());
            support::terminate(1);
        }
    }
}

/// `errx(3)` 的三段句子里程序名归 `getprogname()`，所以这几支都跟着 `argv[0]`。
fn error_text(prog: &str, err: JotError) -> String {
    match err {
        JotError::BadPrecision => format!("{prog}: Bad precision value\n"),
        JotError::BadReps(s) => format!("{prog}: Bad reps value:  {s}\n"),
        JotError::BadStep(s) => format!("{prog}: Bad step value:  {s}\n"),
        JotError::BadFormat(m) => format!("{prog}: {m}\n"),
        JotError::ImpossibleStepsize => format!("{prog}: Impossible stepsize\n"),
        JotError::MustSpecifyBegin => format!("{prog}: Must specify begin if reps == 0\n"),
        JotError::InfiniteUnbounded => format!("{prog}: Infinite sequences cannot be bounded\n"),
        JotError::TooManyArgs(s) => {
            format!("{prog}: Too many arguments.  What do you mean by {s}?\n")
        }
    }
}

fn bad_reps(prog: &str, arg: &str) -> ! {
    support::warn(error_text(prog, JotError::BadReps(arg.to_string())).as_bytes());
    support::terminate(1);
}

fn bad_step(prog: &str, arg: &str) -> ! {
    support::warn(error_text(prog, JotError::BadStep(arg.to_string())).as_bytes());
    support::terminate(1);
}

/// `jot.c:223-225`：第五个操作数起只有一句 `errx`，报的是**第五个**操作数
/// （`argv[4]`，即 `argv += optind` 之后的下标 4），前四个一个都不解读。
fn too_many_arguments(prog: &str, fifth: &str) -> ! {
    support::warn(error_text(prog, JotError::TooManyArgs(fifth.to_string())).as_bytes());
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
