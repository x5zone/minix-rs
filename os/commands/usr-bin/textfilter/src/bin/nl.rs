//! Minix-RS `nl` — the doing half over `minix_textfilter::nl`.
//!
//! Ground truth: `minix3/usr.bin/nl/nl.c`. Numbering, formats and the page
//! delimiter are the library (`nl_lines`); this program parses the flags,
//! reads the file operands (standard input when none), strips newlines into
//! lines, and emits the numbered result.
//!
//! Wired: the three logical-page sections with their numbering types via
//! `-b`/`-h`/`-f` (a/t/n), `-v`/`-i` (start, step), `-w` (number field width),
//! `-l` (blank-run grouping for `-b a`, C `nblank`), `-n ln|rn|rz` (format),
//! `-p` (do not restart numbering at a page delimiter), `-d` (set the 2-byte
//! page delimiter, `\:` by default) and `-s` (set the separator bytes between
//! the number field and the text). The option loop runs on the raw argv bytes:
//! the delimiter, the separator and the seven `-- %s` echoes carry those bytes
//! unchanged, because two of them are printed into every output line. Declared
//! residual: `number_regex` (`-b p…`) — requesting it reports "option not
//! wired" and exits 1.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use minix_regex::matcher::RegexMatcher;
use minix_textfilter::GetoptReport;
use minix_textfilter::nl::{NlOptions, NumFormat, NumberType, nl_lines};

// The standard-input read seam goes through `bin_support::read_stdin()` (the
// host uses `std::io::stdin`, the real machine `minix_sys::read`). This file
// used to carry a local version calling bare `read(0)`, which read nothing on
// the host — exactly what a newly added end-to-end gate exposed on the spot.

/// C's `errx(EXIT_FAILURE, …)`: one line `<prog>: <sentence>`, **no usage
/// appended**, exit 1. The sentence comes back as **bytes** — seven of these
/// sentences extend `optarg` (or the whole type string) at the end, and that
/// stretch is the raw argv bytes; any character decoding would change its
/// length.
fn errx(prog: &str, sentence: &[u8]) -> ! {
    let mut line = Vec::with_capacity(prog.len() + sentence.len() + 3);
    line.extend_from_slice(prog.as_bytes());
    line.extend_from_slice(b": ");
    line.extend_from_slice(sentence);
    line.push(b'\n');
    support::warn(&line);
    support::terminate(1)
}

/// Splice one sentence as "prefix + raw argv bytes + suffix". C's seven
/// `-- %s` sentences share this shape, each inside its own `errx`:
/// `invalid delim argument` (test at `nl.c:163`, sentence at `:165`),
/// `invalid incr argument` (`:182`), `invalid num argument` (`:191`),
/// `illegal format` (`:203`), `invalid startnum value` (`:214`),
/// `invalid width value` (`:223`), `illegal %s line numbering type -- %s`
/// (`:393`, whose prefix also carries a section name taken from
/// `numbering_properties[]`'s `name`, `:73-75`).
fn quoted(prefix: &[u8], value: &[u8], suffix: &[u8]) -> Vec<u8> {
    let mut message = Vec::with_capacity(prefix.len() + value.len() + suffix.len());
    message.extend_from_slice(prefix);
    message.extend_from_slice(value);
    message.extend_from_slice(suffix);
    message
}

/// C's `usage()` (`nl.c:400-405`): two lines, the first ending in `\n\t`;
/// `exit(EXIT_FAILURE)`.
fn usage(prog: &str) -> ! {
    support::warn(
        format!(
            "Usage: {prog} [-p] [-b type] [-d delim] [-f type] [-h type] [-i incr] [-l num]\n\t[-n format] [-s sep] [-v startnum] [-w width] [file]\n"
        )
        .as_bytes(),
    );
    support::terminate(1)
}

/// Appended **after** libc `getopt`'s own line (`getopt.c:101-104` unknown
/// letter, `:132-136` missing value for a valued option; both pass
/// `getprogname()`). This is the whole family's shared shape: one line belongs
/// to libc, and the command's own usage follows it.
fn getopt_leg(report: GetoptReport, prog: &str) -> ! {
    // The letter position goes through `line_bytes`: `getopt.c:79`'s
    // `optopt = *place++` takes one byte at a time and `:103`'s `%c` writes it
    // out unchanged, so `-é` (`c3 a9`) reports the first byte `c3`. This
    // shell's ring takes it by byte too (`char::from(bytes[pos])`; the ring
    // lives entirely in the thin shell), which is what makes the two agree.
    support::warn(&report.line_bytes(prog));
    usage(prog)
}

/// C's `strtol(optarg, &ep, 10)` + `if (*ep != '\0')`: skip leading
/// whitespace, take one sign, accept decimal digits only, and **anything left
/// at the end (whitespace included) is an error**. Overflow saturates to the
/// `i64` endpoint, with the same effect as C's `LONG_MIN/LONG_MAX && errno`
/// branch (both reject).
fn strtol10(value: &[u8]) -> Option<i64> {
    let bytes = value;
    let mut pos = 0usize;
    while pos < bytes.len() && matches!(bytes[pos], b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c) {
        pos += 1;
    }
    let mut negative = false;
    if pos < bytes.len() && matches!(bytes[pos], b'+' | b'-') {
        negative = bytes[pos] == b'-';
        pos += 1;
    }
    let first = pos;
    let mut acc: i64 = 0;
    while pos < bytes.len() && bytes[pos].is_ascii_digit() {
        let digit = i64::from(bytes[pos] - b'0');
        // Overflow is rejected (in C this is the `LONG_MIN/LONG_MAX && errno
        // != 0` branch, which errors the same way).
        acc = acc.checked_mul(10)?.checked_add(digit)?;
        pos += 1;
    }
    if pos == first || pos != bytes.len() {
        return None;
    }
    Some(if negative { -acc } else { acc })
}

/// C's `strtoul(optarg, &ep, 10)` (the `-l` branch): no sign accepted,
/// otherwise as above.
fn strtoul10(value: &[u8]) -> Option<u64> {
    let bytes = value;
    let mut pos = 0usize;
    while pos < bytes.len() && matches!(bytes[pos], b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c) {
        pos += 1;
    }
    let first = pos;
    let mut acc: u64 = 0;
    while pos < bytes.len() && bytes[pos].is_ascii_digit() {
        acc = acc
            .checked_mul(10)?
            .checked_add(u64::from(bytes[pos] - b'0'))?;
        pos += 1;
    }
    if pos == first || pos != bytes.len() {
        return None;
    }
    Some(acc)
}

/// The whole program body; both `main` forms call it, and it does not return.
fn run() -> ! {
    let argv: Vec<Vec<u8>> = support::args_bytes();
    // `getprogname()` prints `argv[0]`'s raw bytes; only `warn_line` (the
    // open-failure sentence borrows `err_line`'s `&str` mouth) and the program
    // name in the usage line need the textual form, each folded once
    // (registered as `ARGV-BYTE-PATH-IN-LIBS`).
    let prog = String::from_utf8_lossy(support::prog_name_bytes(&argv));
    let args: Vec<&[u8]> = argv.iter().map(Vec::as_slice).collect();

    let mut opts = NlOptions::default();
    let mut files: Vec<Vec<u8>> = Vec::new();
    let mut index = 1usize;
    // C's option string is `"pb:d:f:h:i:l:n:s:v:w:"` (`nl.c:149`): only `p`
    // takes no value. Scanning letter by letter allows clusters (`-pb`), a value
    // attached to this token (`-ba`) or taken from the next word (`-b a`), the
    // same shape as libc `getopt` — the old form scanned "a whole token as one
    // letter", so both `-ba` and `-pb t` fell into the unknown-letter branch.
    let mut scanning = true;
    while index < args.len() {
        let token = args[index];
        if !scanning || token.len() < 2 || token[0] != b'-' {
            files.push(token.to_vec());
            index += 1;
            continue;
        }
        if token == b"--" {
            scanning = false;
            index += 1;
            continue;
        }
        let bytes = token;
        let mut pos = 1usize;
        while pos < bytes.len() {
            let letter = char::from(bytes[pos]);
            // Take one option's value: the rest of this token takes precedence
            // (minus the leading whitespace getopt allows), otherwise the next
            // word; if neither exists it is libc's "option requires an
            // argument".
            macro_rules! value {
                ($c:literal) => {{
                    let mut rest = &token[pos + 1..];
                    if rest.is_empty() {
                        index += 1;
                        match args.get(index) {
                            Some(word) => rest = word,
                            // `getopt_leg` does not return, so `return` cannot
                            // be written here (it would be an unreachable
                            // expression).
                            None => getopt_leg(GetoptReport::MissingValue($c), &prog),
                        }
                    } else {
                        // The attached-value form skips leading blanks/tabs.
                        // **libc does not**: `getopt.c:118` is
                        // `optarg = __UNCONST(place)`, handing back the whole
                        // remainder. Measured on `nl -s' x'`, the reference
                        // build hands back `     1 xa` while this shell returns
                        // `     1xa`. This point does not move with the byte
                        // semantics of this round; it is an independent
                        // criterion, registered as
                        // `NL-ATTACHED-VALUE-LEADING-BLANK-STRIPPED`.
                        while matches!(rest.first(), Some(b' ' | b'\t')) {
                            rest = &rest[1..];
                        }
                    }
                    pos = bytes.len();
                    rest
                }};
            }
            match letter {
                'p' => opts.restart = false,
                'b' | 'f' | 'h' => {
                    // C's `parse_numbering` (the function at `nl.c:357`, the
                    // `switch (argstr[0])` at `:362`) looks only at the first
                    // character and reports the rest as part of the whole
                    // string; each of the three sections' sentences carries its
                    // own name (`numbering_properties[]`'s name: footer / body /
                    // header, `nl.c:73-75`).
                    let value = value!('b');
                    let name = match letter {
                        'b' => "body",
                        'f' => "footer",
                        _ => "header",
                    };
                    let kind = match value.first() {
                        Some(b'a') => NumberType::All,
                        Some(b'n') => NumberType::None,
                        Some(b't') => NumberType::Nonempty,
                        // `p<regex>` (C `nl.c:371-389`): compile the rest
                        // as a basic regular expression; a bad pattern takes
                        // C's `errx` sentence `<name> expr: <regerror> --
                        // <pattern>` with the regerror text for REG_BADPAT
                        // (`regerror.c:138`).
                        Some(b'p') => {
                            let pattern = String::from_utf8_lossy(&value[1..]).into_owned();
                            match RegexMatcher::basic(&pattern) {
                                Ok(matcher) => NumberType::Regex(alloc::boxed::Box::new(matcher)),
                                Err(_) => {
                                    // C's `errx` prefixes the program name.
                                    let mut line: Vec<u8> = format!(
                                        "{prog}: {name} expr: invalid regular expression -- "
                                    )
                                    .into_bytes();
                                    line.extend_from_slice(&value[1..]);
                                    line.push(b'\n');
                                    support::warn(&line);
                                    support::terminate(1);
                                }
                            }
                        }
                        _ => errx(
                            &prog,
                            quoted(
                                format!("illegal {name} line numbering type -- ").as_bytes(),
                                value,
                                b"",
                            )
                            .as_slice(),
                        ),
                    };
                    match letter {
                        'b' => opts.body = kind,
                        'f' => opts.footer = kind,
                        _ => opts.header = kind,
                    }
                }
                'd' => {
                    // C takes the first two characters as the delimiter and
                    // errors only on a **third** one (`nl.c:157-169`); an empty
                    // value leaves the default alone (both
                    // `if (optarg[n] != '\0')` tests fail). With fewer than two
                    // characters C's three `if`s read `optarg[1]`/`optarg[2]`,
                    // past the terminator — which is why the reference build
                    // errors on `-d\xff` and, for `-d ""`, takes the **first
                    // byte of the argv that follows in memory** as the second
                    // delimiter. This layer follows the intent of those three
                    // `if`s instead of copying the out-of-bounds read;
                    // registered as `NL-DELIM-LENGTH-RULE`.
                    let value = value!('d');
                    let chars = value;
                    match chars.len() {
                        0 => {}
                        1 => opts.delim[0] = chars[0],
                        2 => {
                            opts.delim[0] = chars[0];
                            opts.delim[1] = chars[1];
                        }
                        _ => errx(
                            &prog,
                            quoted(b"invalid delim argument -- ", value, b"").as_slice(),
                        ),
                    }
                }
                'i' => {
                    let value = value!('i');
                    // C's `-i` (`nl.c:176-183`): `strtol` base 10, then the two
                    // tests `*ep != '\0'` and `(val == LONG_MIN || val ==
                    // LONG_MAX) && errno != 0`, then `incr = (int)val`. Minix's
                    // userland `long` is 32 bits on i386, so the accepted domain
                    // and the cast agree on `int`: a value outside it takes the
                    // same sentence as trailing junk does. **There is no sign
                    // check** — a negative increment counts backwards, and zero
                    // repeats a number (`NL-SIGNED-COUNTERS` closed here).
                    match strtol10(value).and_then(|n| i32::try_from(n).ok()) {
                        Some(n) => opts.incr = n,
                        None => errx(
                            &prog,
                            quoted(b"invalid incr argument -- ", value, b"").as_slice(),
                        ),
                    }
                }
                'l' => {
                    let value = value!('l');
                    match strtoul10(value) {
                        Some(n) => opts.nblank = n as usize,
                        None => errx(
                            &prog,
                            quoted(b"invalid num argument -- ", value, b"").as_slice(),
                        ),
                    }
                }
                'n' => {
                    let value = value!('n');
                    // The three format names are compared as **byte strings**
                    // (`strcmp(optarg, "ln")` and its two siblings at
                    // `nl.c:195-199`).
                    opts.format = match value {
                        b"ln" => NumFormat::Left,
                        b"rn" => NumFormat::Right,
                        b"rz" => NumFormat::RightZero,
                        _ => errx(&prog, quoted(b"illegal format -- ", value, b"").as_slice()),
                    };
                }
                's' => {
                    // The `-s` value is the output content itself (`nl.c:340`'s
                    // `printf("%s%s", sep, buffer)`), so this path cannot have a
                    // second shape: the `Vec<u8>` returned is exactly the bytes
                    // printed.
                    opts.sep = value!('s').to_vec();
                }
                'v' => {
                    let value = value!('v');
                    // C's `-v` (`nl.c:208-216`) is the same `strtol` shape as
                    // `-i` with the sentence `invalid startnum value -- %s`, and
                    // the store is again `startnum = (int)val` — so a negative
                    // first number of a page is legal and the accepted domain is
                    // `int`.
                    match strtol10(value).and_then(|n| i32::try_from(n).ok()) {
                        Some(n) => opts.startnum = n,
                        None => errx(
                            &prog,
                            quoted(b"invalid startnum value -- ", value, b"").as_slice(),
                        ),
                    }
                }
                'w' => {
                    let value = value!('w');
                    match strtol10(value) {
                        Some(n) if n > 0 => opts.width = n as usize,
                        // C's test is `if (!(width > 0))` (`nl.c:225-228`), and
                        // the sentence prints that value too; "cannot convert to
                        // a number" and "converted but <= 0" are two different
                        // sentences and must not be merged.
                        Some(n) => errx(
                            &prog,
                            format!("width argument must be > 0 -- {n}").as_bytes(),
                        ),
                        None => errx(
                            &prog,
                            quoted(b"invalid width value -- ", value, b"").as_slice(),
                        ),
                    }
                }
                _ => getopt_leg(GetoptReport::UnknownOption(letter), &prog),
            }
            pos += 1;
        }
        index += 1;
    }
    // After the option ring C accepts only 0 or 1 operands; one more is
    // `usage()` (`nl.c:239-249`), and this branch does **not** carry libc's line
    // (getopt did not report it).
    if files.len() > 1 {
        usage(&prog);
    }

    // Read every operand (a `-` names standard input); concatenate in order.
    let mut input: Vec<u8> = Vec::new();
    if files.is_empty() {
        input = support::read_stdin();
    } else {
        for f in &files {
            if f == b"-" {
                input.extend(support::read_stdin());
            } else {
                // Fold once before the two `&str` mouths (the open one and the
                // `err` sentence); the file name's **content** is unaffected,
                // only the name position is folded (registered as
                // `ARGV-BYTE-PATH-IN-LIBS`).
                let path = String::from_utf8_lossy(f);
                // C's branch is `if (strcmp(argv[0], "-") != 0 && freopen(argv[0], "r", stdin)
                // == NULL) err(EXIT_FAILURE, "Cannot open `%s'", argv[0])` (`nl.c:243-245`):
                // program name, **backticked operand** and `strerror` sentence all
                // present, exit 1, no usage appended; the `-` shape skips
                // `freopen`, so it reads real standard input. The success side
                // must append the bytes to `input`: C redirects the named file
                // onto stdin, and the whole pipeline has only the one read seam
                // `fgets(..., stdin)` (`:288`), so file content and standard
                // input take the same numbering leg. This used to write only
                // `if let Err(...)`, discarding the `Ok` `Vec` on the spot, so
                // `nl file` printed nothing and still exited 0 (a shape compared
                // byte by byte against the reference build; registered as
                // `NL-OPERAND-CONTENT-DROPPED`).
                match support::read_file(&path) {
                    Ok(bytes) => input.extend(bytes),
                    Err(code) => {
                        support::warn_line(&prog, &format!("Cannot open `{path}'"), code);
                        support::terminate(1);
                    }
                }
            }
        }
    }

    let mut lines: Vec<&[u8]> = input.split(|&b| b == b'\n').collect();
    if lines.last() == Some(&&b""[..]) {
        lines.pop();
    }

    let out = nl_lines(&lines, &opts);
    // C's numbering leg reads with `fgets`, which brings the line-ending newline
    // along, and prints with `printf("%s%s", sep, buffer)` as is (`nl.c:288`,
    // `:340`); a last line without a newline has none. This branch splits lines
    // by byte itself (it must tolerate non-UTF-8), so it uses the same criterion
    // as `lines_of` rather than restating it here.
    let keep_final_newline = minix_textfilter::window::final_line_terminated(&input);
    let mut rendered = Vec::new();
    for (index, line) in out.iter().enumerate() {
        rendered.extend_from_slice(line);
        if index + 1 < out.len() || keep_final_newline {
            rendered.push(b'\n');
        }
    }
    support::emit(&rendered);
    support::terminate(0);
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
