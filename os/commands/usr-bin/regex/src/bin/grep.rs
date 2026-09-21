//! Minix-RS grep — pattern search over stdin.
//!
//! Ground truth: `minix3/usr.bin/grep/grep.c` (NetBSD). The option and
//! pattern grammar, and the exit-status truth table, are the library's
//! (`parse_grep_args`, `exit_code`); line selection composes the compiled
//! pattern with the parsed options. Supported here: `-E`/`-G`/`-F` (fixed
//! strings through metacharacter quoting into the basic engine), `-v`,
//! `-n`, `-c`, `-q`, `-x`, `-l`, and repeatable `-e`. The `-i` and `-w`
//! flags parse but the engine carries no case folding or boundary
//! assertion yet, so they are rejected with an explicit message instead
//! of silently misbehaving (see 08 §4.4). File operands wait for the
//! gated open-existing call; with stdin they would print
//! `(standard input)` for `-l`.


#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_regex::grep::{exit_code, parse_grep_args, SearchMode};
use minix_regex::pattern::{compile_basic, compile_extended, Pattern};

/// Quotes the basic-engine metacharacters so a fixed string compiles to
/// itself (`-F` without a separate fixed-string engine).
fn quote_meta(word: &str) -> String {
    let mut quoted = String::with_capacity(word.len() * 2);
    for ch in word.chars() {
        if matches!(ch, '.' | '[' | ']' | '*' | '^' | '$' | '\\') {
            quoted.push('\\');
        }
        quoted.push(ch);
    }
    quoted
}

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let words: Vec<&str> = argv.iter().map(String::as_str).collect();
    let (options, pattern_words, pattern_count, operands) =
        match parse_grep_args(&words[1..]) {
            Ok(parsed) => parsed,
            Err(_) => {
                support::emit(b"grep: bad pattern or option\n");
                support::terminate(2);
            }
        };
    if options.ignore_case || options.word {
        support::emit(b"grep: -i and -w wait for engine support (see 08 sec 4.4)\n");
        support::terminate(2);
    }
    if !operands.is_empty() {
        support::emit(b"grep: file operands wait for the open call (edge E-CMDSYSFACE)\n");
        support::terminate(2);
    }

    let mut patterns: Vec<Pattern> = Vec::new();
    for &word in &pattern_words[..pattern_count] {
        let pattern = match options.mode {
            SearchMode::Extended => compile_extended(word),
            // Fixed strings quote every metacharacter, then ride the basic
            // engine; the basic metacharacter set is what needs quoting.
            SearchMode::Fixed => compile_basic(&quote_meta(word)),
            SearchMode::Basic => compile_basic(word),
        };
        match pattern {
            Ok(pattern) => patterns.push(pattern),
            Err(_) => {
                support::emit(b"grep: bad pattern or option\n");
                support::terminate(2);
            }
        }
    }

    let input = support::read_stdin();
    let lines = support::lines_of(&input);
    let mut matched_any = false;
    let mut selected_count: u64 = 0;
    let mut printed_input_name = false;
    let mut out = [0u8; 24];
    for (ordinal, line) in lines.iter().enumerate() {
        let hit = patterns.iter().any(|pattern| {
            if options.whole_line {
                pattern.is_full_match(line)
            } else {
                pattern.is_match(line)
            }
        });
        let selected = hit != options.invert;
        if !selected {
            continue;
        }
        matched_any = true;
        selected_count += 1;
        if options.files_only && !printed_input_name {
            support::emit(b"(standard input)\n");
            printed_input_name = true;
        }
        if options.quiet {
            continue;
        }
        if options.count_only {
            continue;
        }
        if options.show_numbers {
            let number = support::utoa_u64(ordinal as u64 + 1, &mut out);
            support::emit(&out[..number]);
            support::emit(b":");
        }
        let mut line_out = line.as_bytes().to_vec();
        line_out.push(b'\n');
        support::emit(&line_out);
    }
    if options.count_only && !options.quiet {
        let number = support::utoa_u64(selected_count, &mut out);
        support::emit(&out[..number]);
        support::emit(b"\n");
    }
    support::terminate(exit_code(matched_any, false, options.quiet));
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
