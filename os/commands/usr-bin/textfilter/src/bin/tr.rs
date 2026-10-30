//! Minix-RS tr — translate, delete or squeeze characters.
//!
//! Ground truth: `minix3/usr.bin/tr/tr.c` (NetBSD) + `str.c`; the command
//! line is `tr [-cs] string1 string2` / `tr [-c] -d string1` /
//! `tr [-c] -s string1` / `tr [-c] -ds string1 string2`. The three operations
//! (translate, delete, squeeze) and `-c` complement live in the library's
//! `translate`/`delete`/`squeeze`/`parse_set`; this program only owns the
//! front door and the data movement.
//!
//! The front door splits, as C does, into three families of sentences and two
//! exit codes:
//! 1. The loop (`tr.c:68-82`, `getopt("cds")`) can produce **exactly one
//!    unknown-letter report** — `case '?': default: usage();` (`:79-81`)
//!    exits inside the loop, so the libc line is followed immediately by the
//!    **whole four-line** usage block, status 1. The missing-value arm is
//!    unreachable (none of the three letters takes a value).
//! 2. Of the four usage lines (`tr.c:276-282`), the program name is
//!    **hardcoded in the string**, and they go to **standard error**; only
//!    the libc line's prefix follows `getprogname()`.
//! 3. The command's own `errx(1, ...)` sentences exit 1, one line each, with
//!    no usage block: this side has `tr: empty string2` (`tr.c:187-188`: the
//!    translation path takes string2's first character and reports when there
//!    is none). The set-syntax sentences on the `str.c` side have the same
//!    shape; `parse_set` returns `SetError::Grammar` and each is printed
//!    verbatim here: `unknown class <name>`, `Sequences only valid in
//!    string2`, `Misplaced sequence asterisk`, `illegal sequence count`. The
//!    two `[=c=]` equivalence-class sentences still exit 1 silently.
//!
//! Arity and combination checks all go to usage: zero operands, one operand
//! without `-d`/`-s`, three or more, `-ds` with one too few, `-d` with one too
//! many — each measured against the reference binary. The loop does not
//! reorder: in `tr a -d` the `-d` is the second set name, and the `-` in
//! `tr -` is a set name.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::tr::{
    Set2Tail, SetError, SetSite, TrArgs, TrError, delete, parse_set, parse_tr_args, squeeze,
    translate, translate_and_squeeze, usage_block,
};

fn run() -> ! {
    let argv: Vec<Vec<u8>> = support::args_bytes();
    let prog = String::from_utf8_lossy(support::prog_name_bytes(&argv)).into_owned();
    let words: Vec<&[u8]> = argv[1..].iter().map(Vec::as_slice).collect();
    let TrArgs {
        complement,
        delete: dflag,
        squeeze: sflag,
        operands,
    } = match parse_tr_args(&words) {
        Ok(parsed) => parsed,
        Err(error) => fail(&prog, &error),
    };

    // string1 does not need the tail criterion (that one decides "what the
    // domain is"); only string2 does: when a set expression ends in a class,
    // exhaustion pads with OOBCH, not with the last byte.
    let (set1, _) = expand(&prog, operands[0], SetSite::String1, complement);
    let (set2, set2_tail) = if operands.len() == 2 {
        expand(&prog, operands[1], SetSite::String2, false)
    } else {
        (Vec::new(), Set2Tail::LastByte)
    };
    // `tr.c:187-188`: only the translation path (including `-s` translation)
    // takes string2's first character up front and reports when there is
    // none; the `-ds` and `-d` paths never touch this criterion.
    if !dflag && operands.len() == 2 && set2.is_empty() {
        support::warn(format!("{prog}: empty string2\n").as_bytes());
        support::terminate(1);
    }

    let input = support::read_stdin();
    if dflag && sflag {
        // `tr -ds string1 string2`: delete the characters in string1, then
        // squeeze adjacent repeats of the characters in string2.
        let stage = run_filter(&input, |buf, out| delete(buf, &set1, out));
        emit(run_filter(&stage, |buf, out| squeeze(buf, &set2, out)));
    } else if dflag {
        emit(run_filter(&input, |buf, out| delete(buf, &set1, out)));
    } else if sflag && operands.len() == 1 {
        emit(run_filter(&input, |buf, out| squeeze(buf, &set1, out)));
    } else if sflag {
        // C's leg is **one pass**: the mapped `int` value is compared directly
        // against `string2[]` and `lastch` (`tr.c:239-246`), so splitting this
        // into translate + squeeze would conflate OOBCH with a real NUL.
        emit(run_filter(&input, |buf, out| {
            translate_and_squeeze(buf, &set1, &set2, set2_tail, out)
        }));
    } else {
        emit(run_filter(&input, |buf, out| {
            translate(buf, &set1, &set2, set2_tail, out)
        }));
    }
    support::terminate(0)
}

/// Run one filter step: the output buffer is sized input length + 1 (every
/// step only deletes or keeps length, squeezing likewise); on failure it exits
/// 1 — the same shape as C's `setup` calling `exit` when a set is bad.
fn run_filter(
    input: &[u8],
    once: impl Fn(&[u8], &mut [u8]) -> Result<usize, minix_textfilter::TextError>,
) -> Vec<u8> {
    let mut out = vec![0u8; input.len() + 1];
    match once(input, &mut out) {
        Ok(len) => out[..len].to_vec(),
        Err(_) => {
            support::terminate(1);
        }
    }
}

fn emit(bytes: Vec<u8>) {
    if !bytes.is_empty() {
        support::emit(&bytes);
    }
}

/// Expand a set expression into the bytes it actually contains; with `-c`,
/// list every byte the expression does **not** contain.
///
/// The syntax sentence has the shape of C's `errx(1, ...)`: program name,
/// colon, space, verbatim sentence; standard error, exit 1, and **no** usage
/// block after it. Whether the expression sits in string1 or string2 decides
/// which sentence it hits, so the set site is a parameter, not a coincidence.
fn expand(prog: &str, text: &[u8], site: SetSite, complement: bool) -> (Vec<u8>, Set2Tail) {
    let mut expanded = [0u8; 256];
    let (used, tail) = match parse_set(text, site, &mut expanded) {
        Ok((used, tail)) => (used, tail),
        Err(SetError::Grammar(grammar)) => {
            support::warn(&grammar_line(prog, &grammar));
            support::terminate(1);
        }
        Err(_) => {
            support::terminate(1);
        }
    };
    if !complement {
        return (expanded[..used].to_vec(), tail);
    }
    let mut member = [false; 256];
    for &byte in &expanded[..used] {
        member[byte as usize] = true;
    }
    (
        (0..=255u8).filter(|byte| !member[*byte as usize]).collect(),
        tail,
    )
}

/// The "program name + syntax sentence" line: the sentence is assembled from
/// bytes, because `unknown class <name>` echoes **the bytes from the
/// expression** (`str.c:253`'s `%.*s` carries a length); folding through a
/// lossy text conversion once would turn that byte string into replacement
/// characters.
fn grammar_line(prog: &str, grammar: &minix_textfilter::tr::SetGrammar) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(prog.as_bytes());
    out.extend_from_slice(b": ");
    out.extend_from_slice(&grammar.sentence_bytes());
    out.push(b'\n');
    out
}

/// Both error exits share one four-line usage block; the only difference is
/// whether the libc line precedes it.
fn fail(prog: &str, error: &TrError) -> ! {
    match error {
        TrError::Usage { reports } => {
            for report in reports {
                // Byte form: the letter slot in the libc line is a bare byte
                // (`getopt.c:79`'s `optopt = *place++` takes one byte at a
                // time, and `:103` writes it out verbatim with `%c`). The
                // reference binary measured `tr -é a b` printing
                // `tr: unknown option -- ` + `c3` + newline; rendering it as
                // a `char` would add a second byte (U+00C3 is `c3 83` in
                // UTF-8).
                support::warn(&report.line_bytes(prog));
            }
            support::warn(usage_block());
            support::terminate(1);
        }
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
