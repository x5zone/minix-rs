//! Minix-RS grep — pattern search over stdin and file operands.
//!
//! Ground truth: `minix3/minix/usr.bin/grep/grep.c` (NetBSD), with the
//! per-file walk in `util.c`/`file.c`. The option and pattern grammar, and
//! the exit-status truth table, are the library's (`parse_grep_args`,
//! `exit_code`); line selection composes the compiled pattern with the
//! parsed options. Supported here: `-E`/`-G`/`-F` (fixed strings through
//! metacharacter quoting into the basic engine), `-v`, `-n`, `-c`, `-q`,
//! `-x`, `-l`, `-w` (via [`minix_regex::pattern::Pattern::find_word`], the
//! `grep.c:85,186` word-boundary strategy), `-h`/`-H` (suppress / force the
//! filename header), and repeatable `-e`. The `-i` flag parses but the engine
//! carries no case folding yet, so it is rejected with an explicit message
//! instead of silently misbehaving (see
//! 08 §4.4). With file operands, each is opened in turn and scanned
//! (`grep.c` `main`); the filename heads a match per `GrepOptions::prefix_shown`
//! (`grep.c:493-494`), `-l` prints the name regardless (and, like C, implies
//! quiet: no line body), `-c` prints `file:count` when the name would show,
//! and line numbers restart per file. With no operands grep reads stdin,
//! labelled `(standard input)`. An unopenable operand is non-fatal: it sets
//! the file error and the run continues (`procfile`), folded into the exit
//! status by `exit_code`. NetBSD grep opens operands through `fopen`, so a
//! bare `-` is a literal file name, not a stdin alias.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_regex::grep::{GrepOptions, SearchMode, exit_code, parse_grep_args};
use minix_regex::pattern::{Pattern, compile_basic, compile_extended};

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
    // Same convention as `sed`: C's `warn`/`err` open with `getprogname()`
    // (`vwarnc.c:3`), the last component of `argv[0]`.
    let prog = support::prog_name(&argv);
    let words: Vec<&str> = argv.iter().map(String::as_str).collect();
    let (options, pattern_words, pattern_count, operands) = match parse_grep_args(&words[1..]) {
        Ok(parsed) => parsed,
        Err(_) => {
            support::emit(format!("{prog}: bad pattern or option\n").as_bytes());
            support::terminate(2);
        }
    };
    if options.ignore_case {
        support::emit(
            format!("{prog}: -i waits for engine case-folding (see 08 sec 4.4)\n").as_bytes(),
        );
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
                support::emit(format!("{prog}: bad pattern or option\n").as_bytes());
                support::terminate(2);
            }
        }
    }

    // `grep.c` main: no operands reads stdin (labelled "(standard input)"),
    // otherwise each operand is opened and scanned in turn. The filename
    // heads a match only when more than one operand is given (`argc <= 1`
    // turns the header off in C); `-l` prints the label regardless.
    let prefix_name = options.prefix_shown(operands.len());
    let mut matched_any = false;
    let mut file_error = false;
    if operands.is_empty() {
        let input = support::read_stdin();
        let lines = support::lines_of(&input);
        let mut rendered = Vec::new();
        if grep_stream(
            &lines,
            "(standard input)",
            prefix_name,
            &patterns,
            &options,
            &mut rendered,
        ) {
            matched_any = true;
        }
        support::emit(&rendered);
    } else {
        for operand in operands {
            let data = match support::read_file(operand) {
                Ok(data) => data,
                Err(code) => {
                    // `procfile` warns (there is no `-s` here) and keeps
                    // going; the failure only shows up in the exit status.
                    support::warn_name(&prog, operand, code);
                    file_error = true;
                    continue;
                }
            };
            let lines = support::lines_of(&data);
            let mut rendered = Vec::new();
            if grep_stream(
                &lines,
                operand,
                prefix_name,
                &patterns,
                &options,
                &mut rendered,
            ) {
                matched_any = true;
            }
            support::emit(&rendered);
        }
    }
    support::terminate(exit_code(matched_any, file_error, options.quiet));
}

/// Scans one labelled stream, rendering selections under the parsed options
/// into `out` and returning whether any line was selected. Line numbers
/// restart per stream (`grep.c` `procfile` resets `line_no`); the caller folds
/// the per-stream result into the run-wide match flag that drives the exit
/// status. `prefix_name` decides the `file:` header on matches and counts;
/// `-l` prints the label on the first selection and suppresses the line body
/// (C folds `-l` into `-q`). Rendering stays pure (it writes into `out`, never
/// a descriptor) so the multi-file rules are unit-testable on the host; the
/// binary owns the emit.
fn grep_stream(
    lines: &[&str],
    label: &str,
    prefix_name: bool,
    patterns: &[Pattern],
    options: &GrepOptions,
    out: &mut Vec<u8>,
) -> bool {
    let mut selected_count: u64 = 0;
    let mut printed_name = false;
    let mut number_buf = [0u8; 24];
    for (ordinal, line) in lines.iter().enumerate() {
        let hit = patterns.iter().any(|pattern| {
            if options.whole_line {
                pattern.is_full_match(line)
            } else if options.word {
                pattern.find_word(line).is_some()
            } else {
                pattern.is_match(line)
            }
        });
        let selected = hit != options.invert;
        if !selected {
            continue;
        }
        selected_count += 1;
        if options.files_only && !printed_name {
            out.extend_from_slice(label.as_bytes());
            out.push(b'\n');
            printed_name = true;
        }
        // Quiet (and, in C, `-l` implies it), count-only, and files-only
        // print no line body; the count is flushed after the scan.
        if options.quiet || options.count_only || options.files_only {
            continue;
        }
        if prefix_name {
            out.extend_from_slice(label.as_bytes());
            out.push(b':');
        }
        if options.show_numbers {
            let number = support::utoa_u64(ordinal as u64 + 1, &mut number_buf);
            out.extend_from_slice(&number_buf[..number]);
            out.push(b':');
        }
        out.extend_from_slice(line.as_bytes());
        out.push(b'\n');
    }
    if options.count_only && !options.quiet {
        if prefix_name {
            out.extend_from_slice(label.as_bytes());
            out.push(b':');
        }
        let number = support::utoa_u64(selected_count, &mut number_buf);
        out.extend_from_slice(&number_buf[..number]);
        out.push(b'\n');
    }
    selected_count > 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(edit: impl FnOnce(&mut GrepOptions)) -> GrepOptions {
        let mut options = GrepOptions::default();
        edit(&mut options);
        options
    }

    fn render(
        lines: &[&str],
        label: &str,
        prefix: bool,
        patterns: &[Pattern],
        options: &GrepOptions,
    ) -> (bool, String) {
        let mut out = Vec::new();
        let matched = grep_stream(lines, label, prefix, patterns, options, &mut out);
        (matched, String::from_utf8(out).unwrap())
    }

    #[test]
    fn single_operand_has_no_filename_prefix() {
        // C drops the header for argc <= 1 (`main` sets hflag).
        let patterns = [compile_basic("foo").unwrap()];
        let (matched, text) = render(&["a foo", "bar"], "f.txt", false, &patterns, &opts(|_| {}));
        assert!(matched);
        assert_eq!(text, "a foo\n");
    }

    #[test]
    fn multiple_operands_prefix_each_match() {
        let patterns = [compile_basic("foo").unwrap()];
        let (matched, text) = render(&["a foo", "bar"], "f.txt", true, &patterns, &opts(|_| {}));
        assert!(matched);
        assert_eq!(text, "f.txt:a foo\n");
    }

    #[test]
    fn line_numbers_restart_per_stream_and_prefix_combines() {
        let patterns = [compile_basic("x").unwrap()];
        let numbering = opts(|options| options.show_numbers = true);
        // Two separate streams; each numbers from 1 (procfile resets line_no).
        let (_, first) = render(&["no", "x1", "x2"], "one", true, &patterns, &numbering);
        let (_, second) = render(&["x3"], "two", true, &patterns, &numbering);
        assert_eq!(first, "one:2:x1\none:3:x2\n");
        assert_eq!(second, "two:1:x3\n");
    }

    #[test]
    fn count_only_prefixes_with_the_name_when_shown() {
        let patterns = [compile_basic("foo").unwrap()];
        let counting = opts(|options| options.count_only = true);
        let (matched, text) = render(&["foo", "foo", "bar"], "f.txt", true, &patterns, &counting);
        assert!(matched);
        assert_eq!(text, "f.txt:2\n");
    }

    #[test]
    fn count_without_prefix_is_a_bare_number() {
        let patterns = [compile_basic("foo").unwrap()];
        let counting = opts(|options| options.count_only = true);
        let (_, text) = render(&["foo", "nope"], "f.txt", false, &patterns, &counting);
        assert_eq!(text, "1\n");
    }

    #[test]
    fn files_only_prints_the_label_once_and_no_body() {
        let patterns = [compile_basic("foo").unwrap()];
        let names = opts(|options| options.files_only = true);
        let (matched, text) = render(&["foo", "foo"], "f.txt", false, &patterns, &names);
        assert!(matched);
        assert_eq!(text, "f.txt\n");
    }

    #[test]
    fn no_match_returns_false_and_prints_nothing() {
        let patterns = [compile_basic("zzz").unwrap()];
        let (matched, text) = render(&["foo", "bar"], "f.txt", true, &patterns, &opts(|_| {}));
        assert!(!matched);
        assert_eq!(text, "");
    }

    #[test]
    fn invert_selects_nonmatching_lines() {
        let patterns = [compile_basic("foo").unwrap()];
        let inverted = opts(|options| options.invert = true);
        let (matched, text) = render(&["foo", "bar"], "f.txt", false, &patterns, &inverted);
        assert!(matched);
        assert_eq!(text, "bar\n");
    }

    #[test]
    fn quiet_reports_the_match_but_emits_nothing() {
        let patterns = [compile_basic("foo").unwrap()];
        let quiet = opts(|options| options.quiet = true);
        let (matched, text) = render(&["foo"], "f.txt", false, &patterns, &quiet);
        assert!(matched);
        assert_eq!(text, "");
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
