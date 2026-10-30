//! Minix-RS vis — visible encoding of stdin and file operands.
//!
//! Ground truth: `minix3/usr.bin/vis/vis.c` plus `minix3/usr.bin/vis/foldit.c`
//! (NetBSD). The deciding half
//! (the pass-through test, the extra list, the C-style escapes, the octal
//! form and the column arithmetic of folding) is the library's `vis` module; this
//! program encodes stdin or each file operand and prints. All fifteen of the C's
//! getopt letters `bcde:F:fhlMmNnoSstw` (`vis.c:73-135`) are modelled:
//! `-c` selects C-style escapes, `-o` three-digit octal, `-b` (`VIS_NOSLASH`)
//! drops the backslash prefix, `-e` names extra bytes, `-l` ends each line with
//! `\$`, `-s` passes backspace/bell/carriage return through, `-t`/`-w` put a
//! tab/space/newline on the extra list, `-S` and `-M` name the shell, glob and
//! whitespace magic, `-N` asks for the C locale that this byte-oriented engine
//! already uses, `-n` copies the stream with each backslash doubled, `-f` folds
//! the output at 80 columns and `-F width` folds at a given width — and, as in the
//! C, `-F` also switches on the end-of-line marker without switching on folding,
//! while `-f` alone keeps the 80-column default. `-h` and `-m` switch to HTTP
//! percent-encoding and MIME quoted-printable, and C refuses the two together
//! (`vis.c:142-144`); `-m` also brings the fold width down to 76 when it is still
//! the default 80 (`vis.c:110-111`) and makes the hidden newline `=` plus a newline
//! instead of a backslash plus one. With operands each is opened, encoded into one
//! output stream, and an unopenable operand warns and flags the exit
//! status (`vis.c:151-164`); a bare `-` is a literal file name (`fopen`), not a
//! stdin alias. Without operands stdin is the source.
//!
//! The command line reaches the ring as bytes (`args_bytes`), and three legs pass
//! those bytes straight through: the `-e` value seeds the extra list — C stores
//! `extra = optarg` (`vis.c:87`) and `makeextralist`
//! (`lib/libc/gen/vis.c:351-384`) lists each byte as `(wchar_t)(u_char)src[i]`, so
//! a folded value names three different bytes and changes what gets encoded (the
//! one judgment leg of this family); the libc option line names the first byte of
//! the flag (`getopt.c:79,103`); an unopenable operand is reported through
//! `warn("%s", *argv)` (`vis.c:157`) with the name as raw bytes. The program-name
//! token still folds once on its way into `&str` mouths
//! (`ARGV-BYTE-PATH-IN-LIBS`), and the usage line's name is C's own
//! `getprogname()` against this shell's hardcoded `vis` — registered as
//! `VIS-USAGE-PROGNAME-HARDCODED`.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::string::{String, ToString};
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::GetoptReport;
use minix_textfilter::vis::{
    VisOptions, copy_doubling_backslashes, encode_chunk, fold_marker, foldit,
};

/// What the getopt scan produces: the encoder's options, the `none` and `fold`
/// counters the command keeps outside `eflags`, the fold width those counters
/// share, and the operands. `fold` and `foldwidth` are separate fields exactly as
/// in C (`vis.c:58`), because `-F` sets the width and the line marker while `-f`
/// alone sets the folding — so `-F 40` without `-f` marks lines and does not fold.
struct Plan<'a> {
    options: VisOptions,
    none: bool,
    fold: bool,
    foldwidth: usize,
    operands: Vec<&'a [u8]>,
}

fn run() -> ! {
    let argv: Vec<Vec<u8>> = support::args_bytes();
    // The program name folds once (both `line_bytes` and the usage line take `&str`,
    // registered `ARGV-BYTE-PATH-IN-LIBS`; the usage line's name is a separate registered
    // item, `VIS-USAGE-PROGNAME-HARDCODED`). Operand names and the `-e` value do not pass
    // through here: the former is built bytewise into the three-part `warn`, the latter
    // goes into the extra list as the whole raw string.
    let prog = String::from_utf8_lossy(support::prog_name_bytes(&argv)).into_owned();
    let refs: Vec<&[u8]> = argv.iter().map(Vec::as_slice).collect();
    let plan = match parse_vis_args(&refs, &prog) {
        Ok(parsed) => parsed,
        Err(message) => {
            support::warn(&message);
            support::terminate(1);
        }
    };

    // `vis.c:151-164`: each operand is opened and encoded into one output
    // stream; an unopenable operand warns and flags the exit status and the
    // run continues. Without operands stdin is the source. `-n` is tested first
    // in C's `process()` loop, so it overrides `-l` and the encoding.
    let mut encoded: Vec<u8> = Vec::new();
    let mut rval = 0;
    // The marker pair is chosen once per run from the flag C tests: both
    // `foldit.c:63` and `vis.c:289` read `eflags & VIS_MIMESTYLE`.
    let marker = fold_marker(plan.options.mime);
    // C's `static int col` (`vis.c:170`): the fold column survives across chunks
    // *and* across file operands, so it lives out here rather than in `consume`.
    let mut col = 0usize;
    let Plan {
        options,
        none,
        fold,
        foldwidth,
        operands,
    } = &plan;
    let mut consume = |data: &[u8]| {
        // One chunk per input byte, the way `process()` fills `buff`, then the
        // fold decision on that chunk, then the chunk itself (`vis.c:202-281`).
        // `nul[] = "\0"` with `cp = nul + 1` is C's seed for the closing test
        // (`vis.c:171-172`), so an empty operand ends on the marker.
        let mut last: u8 = 0;
        for (index, &byte) in data.iter().enumerate() {
            let next = data.get(index + 1).copied().unwrap_or(0);
            let chunk = if *none {
                copy_doubling_backslashes(&[byte])
            } else {
                encode_chunk(byte, next, options).into_bytes()
            };
            if *fold {
                let (markers, carried) = foldit(&chunk, col, *foldwidth);
                for _ in 0..markers {
                    encoded.extend_from_slice(marker);
                }
                col = carried;
            }
            encoded.extend_from_slice(&chunk);
            last = chunk[chunk.len() - 1];
        }
        // `vis.c:288-289`: a partial line is terminated with a hidden newline,
        // and C does not reset the column when it writes it.
        if *fold && last != b'\n' {
            encoded.extend_from_slice(marker);
        }
    };
    if operands.is_empty() {
        consume(&support::read_stdin());
    } else {
        for &operand in operands {
            // The open mouth takes `&str` (registered `ARGV-BYTE-PATH-IN-LIBS`), so the
            // fold happens only at the moment of the call; what the message reads out is
            // the original bytes.
            let shown = String::from_utf8_lossy(operand).into_owned();
            match support::read_file(&shown) {
                Ok(data) => consume(&data),
                Err(code) => {
                    // C calls `warn("%s", *argv)` (`vis.c:157`): the three-part
                    // `vis: file: strerror`, then it continues with the next operand and
                    // sets the exit status to 1 (`:158`). An earlier version printed only
                    // two parts (`vis: file`), missing the `strerror` part.
                    warn_can_open(&prog, operand, code);
                    rval = 1;
                }
            }
        }
    }
    support::emit(&encoded);
    support::terminate(rval);
}

/// The three-part form of C's `warn("%s", *argv)`, with the file-name slot handed back
/// as the raw argv bytes (the fourth hand-assembled helper in this family; the shape's
/// authority is still `minix-types::err_line`, whose `&str` mouth is registered).
fn warn_can_open(prog: &str, name: &[u8], code: minix_sys::Errno) {
    let mut line = Vec::new();
    line.extend_from_slice(prog.as_bytes());
    line.extend_from_slice(b": ");
    line.extend_from_slice(name);
    line.extend_from_slice(b": ");
    line.extend_from_slice(minix_sys::Strerror(code).to_string().as_bytes());
    line.push(b'\n');
    support::warn(&line);
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

/// C's usage text (`vis.c:136-139`, note the capital `U`; exit 1).
const USAGE: &str = "Usage: vis [-bcfhlMmNnoSstw] [-e extra] [-F foldwidth] [file ...]\n";

/// Joins the libc `getopt` line and this program's usage into two lines (the sentence
/// has a single source in [`GetoptReport`], shared with the rest of the family).
///
/// It returns a byte string rather than a `String`: the letter slot of the libc line is
/// one bare byte (`optopt = *place++` at `getopt.c:79` takes one byte at a time, and
/// `%c` at `:103` writes it unchanged). A host without `vis(3)` cannot serve as a
/// reference, so the truth for this branch comes from the nine shells of the family
/// already pinned against C references, plus the C source itself.
fn libc_line(report: GetoptReport, prog: &str) -> Vec<u8> {
    [report.line_bytes(prog), USAGE.as_bytes().to_vec()].concat()
}

/// Fold `argv` into a [`Plan`] the way `getopt(argc, argv, "bcde:F:fhlMmNnoSstw")`
/// does (`vis.c:73-135`): every letter of that string is handled, letters cluster
/// inside one token (`-cb`), the two
/// value-taking letters `e` and `F` read the rest of their token (`-ex`, `-F40`)
/// or the following word (`-e x`), `--` ends option scanning, and a lone `-` is
/// an operand. Each `-e` overwrites the previous one, as C's `extra = optarg`
/// assignment does; each `-F` overwrites the width the same way.
///
/// `argv` arrives as bytes and the `-e` value is handed to the encoder **as those
/// bytes**: C stores `extra = optarg` (`vis.c:87`) and `makeextralist`
/// (`lib/libc/gen/vis.c:351-384`) puts each byte of it on the extra list as
/// `(wchar_t)(u_char)src[i]` whenever `VIS_NOLOCALE` is set or the multibyte decode
/// fails, so the extra list is a set of *bytes* and a folded value names three
/// different bytes. That makes this the one leg in the family where the fold changes
/// what gets encoded, not merely what is printed — see the face test that pins it.
fn parse_vis_args<'a>(argv: &'a [&'a [u8]], prog: &str) -> Result<Plan<'a>, Vec<u8>> {
    let mut options = VisOptions::default();
    let mut none = false;
    let mut fold = false;
    // `vis.c:58`: `foldwidth` starts at 80 and `-f` leaves it there; only `-F`
    // moves it, and only `-f` switches folding on.
    let mut foldwidth = 80usize;
    let mut operands: Vec<&[u8]> = Vec::new();
    let mut index = 1usize;
    let mut scanning = true;
    while index < argv.len() {
        let bytes = argv[index];
        if !scanning || bytes.len() < 2 || bytes[0] != b'-' {
            operands.push(bytes);
            index += 1;
            continue;
        }
        if bytes == b"--" {
            scanning = false;
            index += 1;
            continue;
        }
        let mut pos = 1usize;
        while pos < bytes.len() {
            match bytes[pos] {
                b'c' => options.cstyle = true,
                b'o' => options.octal = true,
                b'b' => options.no_slash = true,
                b'l' => options.mark_eol = true,
                b'n' => none = true,
                b's' => options.safe = true,
                b't' => options.tab = true,
                // `VIS_WHITE` is the three whitespace bits together
                // (`include/vis.h:52`), so `-w` sets all three.
                b'w' => {
                    options.sp = true;
                    options.tab = true;
                    options.nl = true;
                }
                b'S' => options.shell = true,
                // `-h` is `VIS_HTTPSTYLE` and `-m` is `VIS_MIMESTYLE`
                // (`vis.c:99-101,108-112`). `-m` additionally moves the fold width
                // from the default 80 to 76, and only when it is still 80 — an
                // explicit `-F` before it keeps its own width, exactly as C's
                // `if (foldwidth == 80) foldwidth = 76;` does.
                b'h' => options.http = true,
                b'm' => {
                    options.mime = true;
                    if foldwidth == 80 {
                        foldwidth = 76;
                    }
                }
                // `-M` is `VIS_META` = `VIS_WHITE | VIS_GLOB | VIS_SHELL`
                // (`include/vis.h:67`); it is the only way `vis(1)` reaches the
                // glob set, which has no letter of its own.
                b'M' => {
                    options.sp = true;
                    options.tab = true;
                    options.nl = true;
                    options.shell = true;
                    options.glob = true;
                }
                // `-N` selects the C-locale path of `ISGRAPH` and of the `-e`
                // string decoding (`lib/libc/gen/vis.c:122-124,361`). This
                // engine reads bytes and never reassembles characters, so it has
                // no second path to select; the flag is accepted and sets nothing
                // — see the module header of `src/vis.rs` and the face test that
                // pins the resulting output.
                b'N' => {}
                // `-f` switches folding on and leaves the width at whatever it
                // already is (`vis.c:96-98`) — the C's `fold++`, with 80 the
                // starting `foldwidth`.
                b'f' => fold = true,
                b'e' => {
                    // Value: the rest of this token, or the next word.
                    index += 1;
                    options.extra = if pos + 1 < bytes.len() {
                        bytes[pos + 1..].to_vec()
                    } else if index < argv.len() {
                        argv[index].to_vec()
                    } else {
                        // C's getopt prints a line of its own on a missing value
                        // (`getopt.c:132-136`) and only then hands `?` to the option loop,
                        // which falls into `default: usage()` — so this is two lines.
                        return Err(libc_line(GetoptReport::MissingValue('e'), prog));
                    };
                    pos = bytes.len();
                    continue;
                }
                b'F' => {
                    // Value: the rest of this token, or the next word (`vis.c:89-95`).
                    index += 1;
                    let word = if pos + 1 < bytes.len() {
                        &bytes[pos + 1..]
                    } else if index < argv.len() {
                        argv[index]
                    } else {
                        return Err(libc_line(GetoptReport::MissingValue('F'), prog));
                    };
                    // C's check is `atoi(optarg) < 5`, so a word with no digits at
                    // all converts to zero and is refused here too.
                    let width = parse_atoi(word);
                    if width < 5 {
                        return Err("vis: can't fold lines to less than 5 cols\n"
                            .as_bytes()
                            .to_vec());
                    }
                    foldwidth = width as usize;
                    // The same case also runs `markeol++`: `-F` marks end of line,
                    // whether or not folding is on.
                    options.mark_eol = true;
                    pos = bytes.len();
                    continue;
                }
                // `-d` takes no value: in `vis.c:73`'s string the `:` hangs on `e`, not
                // on `d` (`"bcde:F:fhlMmNnoSstw"` splits into
                // `b c d e: F: f h l M m N n o S s t w`), so getopt recognizes it and
                // reports no missing-value line; but `case 'd'` exists only in a `DEBUG`
                // build, so in a shipped build it falls into `default:` and prints one
                // usage line with exit 1 (all four shapes `-d`, `-d x`, `-cd` and `-dZ`
                // print only the usage line). An earlier version treated it as a
                // value-taking letter, so a trailing `-d` produced libc's extra
                // missing-value line.
                b'd' => return Err(USAGE.as_bytes().to_vec()),
                _ => {
                    // Unknown letter: libc's `getopt` prints one line first
                    // (`getopt.c:101-104`; the option string does not start with `:`, so
                    // `opterr` is in effect), and then `case '?'`/`default` adds usage.
                    // An earlier version printed only usage, losing the line the other
                    // shells of this family (head/tail/uniq/cut/join/sort) all have.
                    return Err(libc_line(
                        GetoptReport::UnknownOption(bytes[pos] as char),
                        prog,
                    ));
                }
            }
            pos += 1;
        }
        index += 1;
    }
    // `vis.c:142-144`: the two styles are refused *after* the option loop and
    // before any operand is opened, so this error outranks an unreadable file.
    if options.http && options.mime {
        return Err("vis: Can't specify -m and -h at the same time\n"
            .as_bytes()
            .to_vec());
    }
    Ok(Plan {
        options,
        none,
        fold,
        foldwidth,
        operands,
    })
}

/// C's `atoi` (`vis.c:90` feeds it `-F`'s argument): leading space is skipped, one
/// sign is taken, the digits are read until the first byte that is not one, and
/// whatever was collected is the answer — a word with no digits converts to zero,
/// and trailing garbage is ignored (`-F40x` is 40). Saturation stands in for C's
/// undefined overflow, which cannot matter here: a value that saturates is either
/// far above or far below the `< 5` check it is about to meet.
fn parse_atoi(word: &[u8]) -> i64 {
    let mut cursor = 0usize;
    // `isspace` as the C's `atoi` sees it: space and the five control whites.
    while word
        .get(cursor)
        .copied()
        .is_some_and(|byte| byte.is_ascii_whitespace())
    {
        cursor += 1;
    }
    let negative = match word.get(cursor).copied() {
        Some(b'-') => {
            cursor += 1;
            true
        }
        Some(b'+') => {
            cursor += 1;
            false
        }
        _ => false,
    };
    let mut value: i64 = 0;
    while let Some(digit @ b'0'..=b'9') = word.get(cursor).copied() {
        value = value
            .saturating_mul(10)
            .saturating_add(i64::from(digit - b'0'));
        cursor += 1;
    }
    if negative { -value } else { value }
}
