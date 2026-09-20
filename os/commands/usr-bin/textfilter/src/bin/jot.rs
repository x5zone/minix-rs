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


#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use alloc::format;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::jot::{
    get_prec, operand_to_number, parse_reps, parse_step, Jot, JotError, JotOptions, HAVE_BEGIN,
    HAVE_ENDER, HAVE_REPS, HAVE_STEP,
};

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let mut options = JotOptions::default();
    let mut seed_word: Option<String> = None;
    let mut i = 1;
    while i < argv.len() {
        let arg = argv[i].clone();
        if !arg.starts_with('-') || arg.len() == 1 {
            break;
        }
        let chars: Vec<char> = arg.chars().skip(1).collect();
        let mut k = 0usize;
        while k < chars.len() {
            let c = chars[k];
            match c {
                'c' => options.chardata = true,
                'n' => options.nofinalnl = true,
                'r' => options.randomize = true,
                'b' | 'w' => {
                    let value = flag_value(&argv, &mut i, &chars, &mut k);
                    if value.is_none() {
                        usage();
                    }
                    options.boring |= c == 'b';
                    options.word = value.unwrap();
                }
                's' => {
                    let value = flag_value(&argv, &mut i, &chars, &mut k);
                    if value.is_none() {
                        usage();
                    }
                    options.sep = value.unwrap();
                }
                'p' => {
                    let value = flag_value(&argv, &mut i, &chars, &mut k);
                    match value.and_then(|v| v.parse::<usize>().ok()) {
                        Some(p) => options.prec = Some(p),
                        None => {
                            support::warn(b"jot: Bad precision value\n");
                            support::terminate(1);
                        }
                    }
                }
                _ => usage(),
            }
            k += 1;
        }
        i += 1;
    }

    // Examine args right to left, falling through the cases
    // (jot.c:176-220). `-` keeps the default for that slot.
    let operands: Vec<&str> = argv[i..].iter().map(|s| s.as_str()).collect();
    if operands.len() > 4 {
        support::warn(
            format!("jot: Too many arguments.  What do you mean by {}?\n", operands[4]).as_bytes(),
        );
        support::terminate(1);
    }
    let mut inferred_begin = 0usize;
    let mut inferred_ender = 0usize;
    if let Some(reps_arg) = operands.first().filter(|s| **s != "-") {
        match parse_reps(reps_arg) {
            Ok(r) => {
                options.reps = r;
                options.have |= HAVE_REPS;
            }
            Err(_) => bad_reps(reps_arg),
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
            Err(_) => bad_step(step_arg),
        }
    }
    // Maximum precision of begin/ender (jot.c:200-203).
    options.prec = Some(options.prec.unwrap_or(0).max(inferred_begin).max(inferred_ender));

    let seed: u64 = match seed_word {
        Some(s) => match parse_step(&s) {
            Ok(v) if v >= 0.0 => v as u64,
            _ => bad_step(&s),
        },
        None => {
            // Clock + pid mix (C jot seeds from time(2)); the clock half
            // rides the `support::epoch_micros` twin, the pid half the
            // kernel getpid wrapper both sides.
            support::epoch_micros().unwrap_or(0)
                ^ ((minix_sys::getpid().unwrap_or(0) as u64) << 16)
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
            support::warn(error_text(err).as_bytes());
            support::terminate(1);
        }
    }
}

/// The value of a value-taking flag: the rest of this argument, or
/// the next argument (getopt semantics).
fn flag_value(argv: &[String], i: &mut usize, chars: &[char], k: &mut usize) -> Option<String> {
    *k += 1;
    if *k < chars.len() {
        return Some(chars[*k..].iter().collect());
    }
    *i += 1;
    argv.get(*i).cloned()
}

fn error_text(err: JotError) -> String {
    match err {
        JotError::BadPrecision => "jot: Bad precision value\n".to_string(),
        JotError::BadReps(s) => format!("jot: Bad reps value:  {}\n", s),
        JotError::BadStep(s) => format!("jot: Bad step value:  {}\n", s),
        JotError::BadFormat(m) => format!("jot: {}\n", m),
        JotError::ImpossibleStepsize => "jot: Impossible stepsize\n".to_string(),
        JotError::MustSpecifyBegin => "jot: Must specify begin if reps == 0\n".to_string(),
        JotError::InfiniteUnbounded => "jot: Infinite sequences cannot be bounded\n".to_string(),
        JotError::TooManyArgs(s) => format!("jot: Too many arguments.  What do you mean by {}?\n", s),
    }
}

fn bad_reps(arg: &str) -> ! {
    support::warn(error_text(JotError::BadReps(arg.to_string())).as_bytes());
    support::terminate(1);
}

fn bad_step(arg: &str) -> ! {
    support::warn(error_text(JotError::BadStep(arg.to_string())).as_bytes());
    support::terminate(1);
}

fn usage() -> ! {
    support::warn(
        b"usage: jot [-cnr] [-b word] [-p precision] [-s string] [-w word] [reps [begin [end [step | seed]]]]\n",
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
