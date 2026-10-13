//! Minix-RS sort — key-based line sorting.
//!
//! Ground truth: `minix3/usr.bin/sort/` (NetBSD). The deciding half —
//! `-k` key specifications, the `b d f i M n r` modifiers, `-n` numeric
//! comparison, `-t` delimiters, `-r`, `-u`, and `-c` — is the library's
//! `sort` module; the doing half collects the operands, orders the lines
//! in memory (the C's external merge over temporary files is the
//! execution layer for inputs beyond memory), and emits lines with a
//! trailing newline. Each input operand is read in file order as its own
//! run (a `-` names standard input; no operand reads standard input); the
//! default concatenates and sorts them, and `-m` merges the runs assuming
//! each is already ordered (`merge_sorted`). `-o` writes to a file instead of
//! standard output, replacing an existing one through a temporary exactly as
//! `sort.c:320-379` does; `-` and the file operands keep their existing shapes.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::sort::{compare, merge_sorted, parse_options};

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let words: Vec<&str> = argv[1..].iter().map(String::as_str).collect();
    let (options, operands) = match parse_options(&words) {
        Ok((options, operands)) => (options, operands),
        Err(_) => {
            support::warn(b"sort: bad option\n");
            support::terminate(2);
        }
    };
    // C reads each input operand in file order (`-` / an unrecognised leading
    // dash names standard input) and concatenates their lines; no operand reads
    // standard input. `-c` accepts at most one input file (sort.c too-many guard).
    if options.check && operands.len() > 1 {
        support::warn(b"sort: too many input files for -c option\n");
        support::terminate(2);
    }
    let mut runs: Vec<Vec<String>> = Vec::new();
    if operands.is_empty() {
        runs.push(split_lines(&support::read_stdin()));
    } else {
        for operand in operands {
            let input = if operand == "-" {
                support::read_stdin()
            } else {
                read_operand(operand)
            };
            runs.push(split_lines(&input));
        }
    }
    let mut lines: Vec<String> = runs.iter().flatten().cloned().collect();

    if options.check {
        check(&lines, &options);
        support::terminate(0);
    }

    if options.merge {
        let refs: Vec<&[String]> = runs.iter().map(|r| r.as_slice()).collect();
        lines = merge_sorted(&refs, &options);
    } else {
        lines.sort_by(|a, b| compare(a, b, &options));
    }
    if options.unique {
        lines.dedup_by(|a, b| compare(a, b, &options) == core::cmp::Ordering::Equal);
    }
    // The whole result goes to one stream: C opens it as `outfp` before sorting and
    // hands that handle to `fsort`/`fmerge` (`sort.c:357-363`), so `-o` cannot be
    // reached at all on the `-c` path, which exited above.
    let mut payload: Vec<u8> = Vec::new();
    for line in &lines {
        payload.extend_from_slice(line.as_bytes());
        payload.push(b'\n');
    }
    write_output(&options, &payload);
    support::terminate(0);
}

/// C's output leg, `sort.c:320-379`, in C's order of operations. Without `-o` it is
/// `stdout`. With `-o` the question is whether the name already exists and is not a
/// device: if so, C refuses an unwritable one, spools the result into a
/// `<path>XXXXXX` temporary, copies the old file's permission bits onto it, unlinks
/// the old name, hard-links the temporary to it and only then unlinks the temporary;
/// if not, C opens the name straight through with `"w"`. The temporary is why a
/// sort that dies halfway leaves the previous file intact rather than an empty one.
fn write_output(options: &minix_textfilter::sort::Options, payload: &[u8]) {
    use minix_textfilter::sort::{Output, OutputProbe, decide_output, temp_candidate};

    // The path is probed only when `-o` was given, as C's `lstat` sits inside the
    // `else if (outpath)` branch (`sort.c:324`).
    let given = options.output.as_deref();
    let target = given.map_or_else(String::new, |bytes| {
        String::from_utf8_lossy(bytes).into_owned()
    });
    let probe = given
        .and_then(|_| support::probe_path(&target))
        .map(|(mode, is_device)| OutputProbe { mode, is_device });
    match decide_output(given, probe) {
        Output::StandardOutput => support::emit(payload),
        Output::Direct { .. } => match support::create_sink(&target) {
            Ok(sink) => {
                // C writes through stdio and never inspects the result; the flush at
                // exit can fail just as silently, so no message is invented here.
                let _ = sink.finish(payload);
            }
            Err(()) => {
                support::warn(format!("sort: output file {target}\n").as_bytes());
                support::terminate(2);
            }
        },
        Output::ViaTemp { mode, .. } => {
            // `access(outpath, W_OK)` first, with C's plain path message
            // (`sort.c:336-337`).
            if !support::writable(&target) {
                support::warn(format!("sort: {target}\n").as_bytes());
                support::terminate(2);
            }
            let seed = support::epoch_micros().unwrap_or(0);
            let mut temp = target.clone();
            let mut sink = None;
            for attempt in 0..64 {
                temp = String::from_utf8_lossy(&temp_candidate(target.as_bytes(), seed, attempt))
                    .into_owned();
                if let Ok(opened) = support::create_exclusive(&temp) {
                    sink = Some(opened);
                    break;
                }
            }
            let Some(sink) = sink else {
                support::warn(format!("sort: Cannot create temporary file `{temp}'\n").as_bytes());
                support::terminate(2);
            };
            if sink.finish(payload).is_err() {
                support::warn(format!("sort: Cannot open temporary file `{temp}'\n").as_bytes());
                support::terminate(2);
            }
            // `access(outfile, F_OK)` before the mode copy (`sort.c:358-361`), then
            // the old file's own permission bits land on the temporary
            // (`:366-370`), and only then does the destination name move
            // (`:372-379`).
            if support::probe_path(&temp).is_none() {
                support::warn(format!("sort: {temp}\n").as_bytes());
                support::terminate(2);
            }
            if support::set_mode(&temp, mode).is_err() {
                support::warn(
                    format!("sort: cannot chmod {target}: output left in {temp}\n").as_bytes(),
                );
                support::terminate(2);
            }
            let _ = support::remove_path(&target);
            if support::hard_link(&temp, &target).is_err() {
                support::warn(
                    format!("sort: cannot link {target}: output left in {temp}\n").as_bytes(),
                );
                support::terminate(2);
            }
            let _ = support::remove_path(&temp);
        }
    }
}

/// Splits collected input into lines, dropping the trailing newline of
/// every line and the final empty piece after a terminating newline.
fn split_lines(input: &[u8]) -> Vec<String> {
    if input.is_empty() {
        return Vec::new();
    }
    let text = core::str::from_utf8(input).unwrap_or("");
    let mut pieces: Vec<String> = text.split('\n').map(String::from).collect();
    if input.ends_with(b"\n") {
        pieces.pop();
    }
    pieces
}

/// Read an input file to the end; an unopenable file is a status-2 error
/// (the C's `access(argv[i], R_OK)` guard at `sort.c:284` and the `fopen` failure
/// in `files.c`). The open-and-read itself is the shared `bin_support` twin, so the
/// hosted door and the target run take the same path through the seam.
fn read_operand(path: &str) -> Vec<u8> {
    match support::read_file(path) {
        Ok(data) => data,
        Err(()) => {
            support::warn(format!("sort: {path}\n").as_bytes());
            support::terminate(2);
        }
    }
}

/// The `-c` mode: every adjacent pair must be in order, and with `-u`
/// no pair may compare equal. The first violation is reported by name
/// and the status leaves 1 (the C's `warn` + `exit(1)` shape).
fn check(lines: &[String], options: &minix_textfilter::sort::Options) {
    let mut previous: Option<&str> = None;
    for line in lines {
        if let Some(prev) = previous {
            match compare(prev, line, options) {
                core::cmp::Ordering::Greater => {
                    support::warn(b"sort: disorder: ");
                    support::warn(line.as_bytes());
                    support::warn(b"\n");
                    support::terminate(1);
                }
                core::cmp::Ordering::Equal if options.unique => {
                    support::warn(b"sort: duplicate: ");
                    support::warn(line.as_bytes());
                    support::warn(b"\n");
                    support::terminate(1);
                }
                _ => {}
            }
        }
        previous = Some(line);
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
