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
use alloc::string::{String, ToString};
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::sort::{ParseError, compare, merge_sorted, parse_options};

fn run() -> ! {
    let argv: Vec<Vec<u8>> = support::args_bytes();
    // Every C diagnostic line starts with `getprogname()` (`sort.c:401-412`
    // usage, `:261-285` errx/err), so the name cannot be hard-coded as "sort".
    // The two `&str` doors `err_line` and `line_bytes` need the program name
    // (a registered limitation); this is the one fold site. Operand and record
    // delimiter echoes have their own byte-assembled legs and do not pass
    // through here.
    let prog = String::from_utf8_lossy(support::prog_name_bytes(&argv)).into_owned();
    let words: Vec<&[u8]> = argv[1..].iter().map(Vec::as_slice).collect();
    let (options, operands) = match parse_options(&words) {
        Ok((options, operands)) => (options, operands),
        Err(error) => parse_fail(&prog, error),
    };
    // C reads each input operand in file order (`-` / an unrecognised leading
    // dash names standard input) and concatenates their lines; no operand reads
    // standard input. `-c` accepts at most one input file (sort.c too-many guard).
    if options.check && operands.len() > 1 {
        // `errx(2, "too many input files for -c option")` (`sort.c:261`): that
        // sentence alone, no usage, and it does **not** append an `errno`
        // sentence.
        support::warn(format!("{prog}: too many input files for -c option\n").as_bytes());
        support::terminate(2);
    }
    let mut runs: Vec<Vec<String>> = Vec::new();
    if operands.is_empty() {
        runs.push(split_records(&support::read_stdin(), options.record_delim));
    } else {
        for operand in operands {
            let input = if operand == b"-" {
                support::read_stdin()
            } else {
                read_operand(&prog, operand)
            };
            runs.push(split_records(&input, options.record_delim));
        }
    }
    let mut lines: Vec<String> = runs.iter().flatten().cloned().collect();

    if options.check {
        check(&prog, &lines, &options);
        support::terminate(0);
    }

    if options.merge {
        let refs: Vec<&[String]> = runs.iter().map(|r| r.as_slice()).collect();
        lines = merge_sorted(&refs, &options);
    } else {
        // C's key tie rule is only complete at the sorting step: the comparison
        // half breaks ties by whole record only under `-S`, otherwise it returns
        // Equal. Which way a tie goes depends on C's direction bit: by default
        // it stably keeps input order (the reference binary's `-k 2,2` on four
        // lines with the same key gives `b a c d`, exactly input order), while
        // with global `-r` the same readings invert to `d c a b`. `sort_by` is
        // stable by itself and cannot produce the inverted half, so the input
        // index is carried into the comparison explicitly: ascending index
        // forward in normal order, descending under `-r`.
        let mut indexed: Vec<(usize, String)> = lines.drain(..).enumerate().collect();
        indexed.sort_by(|(ia, a), (ib, b)| {
            compare(a, b, &options).then_with(|| {
                if options.reverse {
                    ib.cmp(ia)
                } else {
                    ia.cmp(ib)
                }
            })
        });
        lines = indexed.into_iter().map(|(_, line)| line).collect();
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
        // C's `putrec` (`files.c:194`/`:215`) is "write the record and then
        // append one `REC_D`", not "append a separator except after the last
        // record": with `-R` the output **ends with the delimiter** (the
        // reference binary turns `b,a,d,c\n` with `-R ,` into `a,b,c\n,d,`, and
        // that trailing comma is written by the ground truth).
        // `putchar(REC_D)`: `int` in, `unsigned char` out, so -1 writes `0xff`.
        payload.push(options.record_delim as u8);
    }
    write_output(&prog, &options, &payload);
    support::terminate(0);
}

/// C's output leg, `sort.c:320-379`, in C's order of operations. Without `-o` it is
/// `stdout`. With `-o` the question is whether the name already exists and is not a
/// device: if so, C refuses an unwritable one, spools the result into a
/// `<path>XXXXXX` temporary, copies the old file's permission bits onto it, unlinks
/// the old name, hard-links the temporary to it and only then unlinks the temporary;
/// if not, C opens the name straight through with `"w"`. The temporary is why a
/// sort that dies halfway leaves the previous file intact rather than an empty one.
fn write_output(prog: &str, options: &minix_textfilter::sort::Options, payload: &[u8]) {
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
            // `sort.c:354-355` in C is `err(2, "output file %s", outfile)`: the
            // sentence is followed by the `strerror` sentence (the old form
            // printed only up to the name).
            Err(code) => {
                support::warn_line(prog, &format!("output file {target}"), code);
                support::terminate(2);
            }
        },
        Output::ViaTemp { mode, .. } => {
            // `access(outpath, W_OK)` runs first (`sort.c:336-337`). That arm is
            // `err(2, "%s", outpath)`: program name, path and the `strerror`
            // sentence — all three, not a bare path.
            if let Err(code) = support::writable(&target) {
                support::warn_line(prog, &target, code);
                support::terminate(2);
            }
            let seed = support::epoch_micros().unwrap_or(0);
            let mut temp = target.clone();
            let mut sink = None;
            // Keep the last attempt's number for the report: C is
            // `err(2, "Cannot create temporary file `%s'", toutpath)`
            // (`sort.c:340-341`), followed by `strerror`. Failures while trying
            // names are not reported; only an exhausted loop is (unlike C's
            // `mkstemp`, which tries once, this tree derives candidate names
            // from a time seed, so collisions in those six characters must be
            // skipped here).
            let mut last_error = minix_sys::Errno::EEXIST;
            for attempt in 0..64 {
                temp = String::from_utf8_lossy(&temp_candidate(target.as_bytes(), seed, attempt))
                    .into_owned();
                match support::create_exclusive(&temp) {
                    Ok(opened) => {
                        sink = Some(opened);
                        break;
                    }
                    Err(code) => last_error = code,
                }
            }
            let Some(sink) = sink else {
                support::warn_line(
                    prog,
                    &format!("Cannot create temporary file `{temp}'"),
                    last_error,
                );
                support::terminate(2);
            };
            if sink.finish(payload).is_err() {
                // This shell reports the write failure here; C's arm is a
                // failed `fdopen(outfd, "w")` (`:350-351`). The two are not the
                // same event (in C a write failure is silently discarded by
                // stdio at exit). The shape is left alone this round; only
                // registered.
                support::warn(format!("sort: Cannot open temporary file `{temp}'\n").as_bytes());
                support::terminate(2);
            }
            // `access(outfile, F_OK)` before the mode change (`sort.c:364-365`),
            // then the original file's permission bits are applied to the
            // temporary (`:373-374`), and only then does it change names
            // (`:376-380`).
            if let Err(code) = support::exists(&temp) {
                // C's arm is also `err(2, "%s", outfile)`: a vanished temporary
                // must still report its errno number.
                support::warn_line(prog, &temp, code);
                support::terminate(2);
            }
            if let Err(code) = support::set_mode(&temp, mode) {
                // `err(2, "cannot chmod %s: output left in %s", outpath, outfile)`
                // (`sort.c:373-374`): both names are in the sentence, followed
                // by the errno number.
                support::warn_line(
                    prog,
                    &format!("cannot chmod {target}: output left in {temp}"),
                    code,
                );
                support::terminate(2);
            }
            // C ignores a failed `unlink(outpath)` (`:376`); so does this.
            let _ = support::remove_path(&target);
            if let Err(code) = support::hard_link(&temp, &target) {
                // `err(2, "cannot link %s: output left in %s", outpath, outfile)`
                // (`sort.c:379-380`).
                support::warn_line(
                    prog,
                    &format!("cannot link {target}: output left in {temp}"),
                    code,
                );
                support::terminate(2);
            }
            let _ = support::remove_path(&temp);
        }
    }
}

/// Splits collected input into records on C's `REC_D` (`-R`, default `'\n'`),
/// dropping the delimiter itself and the final empty piece after a terminating
/// delimiter — that is what C's `getrec` (`files.c:113-124`) does: reaching a
/// delimiter closes the current record, and returning 0 when not a single byte
/// was read before EOF (so `a,b,` is two records, not three). The record
/// content itself is left untouched: under `-R ,` the last record of
/// `b,a,d,c\n` is `c\n` (newline included), because a newline is just an
/// ordinary byte in that shape.
fn split_records(input: &[u8], rec_d: i32) -> Vec<String> {
    if input.is_empty() {
        return Vec::new();
    }
    // C's `getrec` compares `int ch = getc(f)` (the bytes 0..=255) with
    // `int REC_D` (`files.c:113-124`), and `REC_D` arriving via the `-R`
    // character path is a **sign-extended `char`** — so `-R<ff>` gives a
    // negative number in C that never matches, making the whole input one
    // record. Byte-for-byte reconciliation requires both sides to share that
    // shape, so this cannot split by `u8` equality.
    let cuts = |byte: u8| i32::from(byte) == rec_d;
    let mut pieces: Vec<String> = input
        .split(|byte| cuts(*byte))
        .map(|chunk| String::from_utf8_lossy(chunk).into_owned())
        .collect();
    if input.last().is_some_and(|&last| cuts(last)) {
        pieces.pop();
    }
    pieces
}

/// Read an input file to the end; an unopenable file is a status-2 error
/// (the C's `access(argv[i], R_OK)` guard at `sort.c:284` and the `fopen` failure
/// in `files.c`). The open-and-read itself is the shared `bin_support` twin, so the
/// hosted door and the target run take the same path through the seam.
fn read_operand(prog: &str, path: &[u8]) -> Vec<u8> {
    // Only the open step folds once (the `&str` door of `minix_sys::open`, a
    // registered limitation); the message leg does not fold.
    let shown = String::from_utf8_lossy(path);
    match support::read_file(&shown) {
        Ok(data) => data,
        // C `sort.c:285` is `err(2, "%s", argv[i])`: program name, operand and
        // the `strerror` sentence — all three (the old form used `err(3)` as if
        // it were `errx(3)` and kept only the first two). The three parts are
        // assembled here, so the name slot carries the raw argv bytes;
        // `warn_line` goes through `err_line(&str)` and would fold raw bytes
        // into U+FFFD, which does not match the reference binary byte for byte
        // (the same shape was measured for `pr`).
        Err(code) => {
            let mut line = Vec::new();
            line.extend_from_slice(prog.as_bytes());
            line.extend_from_slice(b": ");
            line.extend_from_slice(path);
            line.extend_from_slice(b": ");
            line.extend_from_slice(minix_sys::Strerror(code).to_string().as_bytes());
            line.push(b'\n');
            support::warn(&line);
            support::terminate(2);
        }
    }
}

/// Turns the deciding half's [`ParseError`] into the messages of C's matching
/// arm. The three arms differ:
/// - libc prints one line first, then the **whole usage block**, exit 2
///   (`sort.c:252`'s `usage(NULL)`; the message body lives in
///   `getopt.c:101-104`/`:132-136`);
/// - `sort: <one of the five sentences>`, exit 2, **no usage** (the `errx` calls
///   at `init.c:159-217`; the sentence text has a single source in
///   `sort.rs:KeySpecError::sentence`) — the old form squeezed the five
///   sentences into one `illegal field descriptor`;
/// - `sort: multiple field delimiters` followed by the whole usage block, exit
///   2 (`sort.c:214-215`'s `usage("multiple field delimiters")`, through
///   `usage()`'s `%s: %s` first sentence).
fn parse_fail(prog: &str, error: ParseError) -> ! {
    match error {
        ParseError::Getopt(report) => {
            // The letter slot goes through `line_bytes`: the extraction inside
            // the ring in `src/sort.rs` has already switched to bytes, and the
            // two sides only agree in that shape.
            support::warn(&report.line_bytes(prog));
            usage(prog, None)
        }
        ParseError::KeySpec(spec) => {
            support::warn(format!("{prog}: {}\n", spec.sentence()).as_bytes());
            support::terminate(2);
        }
        ParseError::MultipleFieldDelimiters => usage(prog, Some("multiple field delimiters")),
        // The three arms below follow the shapes of `sort.c:223-242` and
        // `:217-218`: `multiple record delimiters` goes through `usage()` (one
        // sentence plus the whole usage block), while the two `errx` sentences
        // (clash and invalid) are one line with **no usage**; all three exit 2.
        ParseError::MultipleRecordDelimiters => usage(prog, Some("multiple record delimiters")),
        ParseError::RecordFieldDelimiterClash => {
            support::warn(format!("{prog}: record/field delimiter clash\n").as_bytes());
            support::terminate(2);
        }
        ParseError::InvalidRecordDelimiter(value) => {
            // C is `errx(2, "invalid record delimiter %s", optarg)`
            // (`sort.c:233-238`): the echo slot is the raw argv bytes (the
            // backslash prefix was already skipped by `optarg++`, so what is
            // reported is the remaining segment).
            let mut line = Vec::new();
            line.extend_from_slice(prog.as_bytes());
            line.extend_from_slice(b": invalid record delimiter ");
            line.extend_from_slice(&value);
            line.push(b'\n');
            support::warn(&line);
            support::terminate(2);
        }
    }
}

/// C `usage()` (`sort.c:399-412`): an optional `"%s: %s\n"` first sentence, then
/// that **one** output line. The line is assembled from two `fprintf` calls, the
/// first without a newline and the second starting with thirteen spaces, so the
/// ground truth's usage looks like "one line with a wide gap in it" — spacing
/// and column items are preserved verbatim (those thirteen spaces were counted
/// out of the `.rodata` of the compiled `sort.o`, not copied by hand). Exit 2.
fn usage(prog: &str, message: Option<&str>) -> ! {
    if let Some(message) = message {
        support::warn(format!("{prog}: {message}\n").as_bytes());
    }
    support::warn(
        format!(
            "usage: {prog} [-bcdfHilmnrSsu] [-k field1[,field2]] [-o output] [-R char] [-T dir]             [-t char] [file ...]\n"
        )
        .as_bytes(),
    );
    support::terminate(2)
}

/// The `-c` mode: every adjacent pair must be in order, and with `-u`
/// no pair may compare equal. The first violation is reported by name.
/// The two sentences of the `-c` leg correspond word for word to C's
/// `msort.c:order()`: disorder is `errx(1, "found disorder: %s", …)`
/// (`msort.c:403`), and under `-u` an equal key is
/// `errx(1, "found non-uniqueness: %s", …)` (`:405-407`). Both exit 1 and
/// **append no usage**; the program name comes from the last segment of
/// `argv[0]` (`errx` adds it), and the single `%s` holds the **later record**
/// being reported — C writes a `\0` over the record's trailing delimiter
/// before printing (`crec->data[crec->length-1] = 0;`, `:402`/`:406`), so the
/// output is exactly one line per record. The shell used to write
/// `sort: disorder: ` and `sort: duplicate: `, both invented wording, with the
/// program name hard-coded as `sort`; this round changed them according to the
/// reference binary's readings: `sort: found disorder: a` and
/// `sort: found non-uniqueness: a`, both exiting 1.
fn check(prog: &str, lines: &[String], options: &minix_textfilter::sort::Options) {
    let mut previous: Option<&str> = None;
    for line in lines {
        if let Some(prev) = previous {
            // The criteria follow C's order: first `0 < cmp(prec, crec)`
            // (disorder), then `UNIQUE && !c` (duplicate). Both read the **same**
            // comparison, so the pair is not compared twice.
            match compare(prev, line, options) {
                core::cmp::Ordering::Greater => {
                    support::warn(format!("{prog}: found disorder: {line}\n").as_bytes());
                    support::terminate(1);
                }
                core::cmp::Ordering::Equal if options.unique => {
                    support::warn(format!("{prog}: found non-uniqueness: {line}\n").as_bytes());
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
