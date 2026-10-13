//! Minix-RS vis — visible encoding of stdin and file operands.
//!
//! Ground truth: `minix3/usr.bin/vis/vis.c` (NetBSD). The deciding half
//! (the pass-through test, the extra list, the C-style escapes and the octal
//! form) is the library's `vis` module; this program encodes stdin or each file
//! operand and prints. Eleven of the C's fifteen getopt letters
//! `bcde:F:fhlMmNnoSstw` (`vis.c:73-135`) are modelled: `-c` selects C-style
//! escapes, `-o` three-digit octal, `-b` (`VIS_NOSLASH`) drops the backslash
//! prefix, `-e` names extra bytes, `-l` ends each line with `\$`, `-s` passes
//! backspace/bell/carriage return through, `-t`/`-w` put a tab/space/newline on
//! the extra list, `-S` and `-M` name the shell, glob and whitespace magic, `-N`
//! asks for the C locale that this byte-oriented engine already uses, and `-n`
//! copies the stream with each backslash doubled. `-f`, `-F`, `-h` and `-m` are
//! rejected rather than silently ignored. With operands each is opened, encoded
//! into one output stream, and an unopenable operand warns and flags the exit
//! status (`vis.c:151-164`); a bare `-` is a literal file name (`fopen`), not a
//! stdin alias. Without operands stdin is the source.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::vis::{VisOptions, copy_doubling_backslashes, encode};

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let (options, none, operands) = match parse_vis_args(&argv) {
        Ok(parsed) => parsed,
        Err(message) => {
            support::warn(message.as_bytes());
            support::terminate(1);
        }
    };

    // `vis.c:151-164`: each operand is opened and encoded into one output
    // stream; an unopenable operand warns and flags the exit status and the
    // run continues. Without operands stdin is the source. `-n` is tested first
    // in C's `process()` loop, so it overrides `-l` and the encoding.
    let mut encoded: Vec<u8> = Vec::new();
    let mut rval = 0;
    let mut consume = |data: &[u8]| {
        if none {
            encoded.extend_from_slice(&copy_doubling_backslashes(data));
        } else {
            encoded.extend_from_slice(encode(data, &options).as_bytes());
        }
    };
    if operands.is_empty() {
        consume(&support::read_stdin());
    } else {
        for operand in &operands {
            match support::read_file(operand) {
                Ok(data) => consume(&data),
                Err(()) => {
                    support::warn(format!("vis: {operand}\n").as_bytes());
                    rval = 1;
                }
            }
        }
    }
    support::emit(&encoded);
    support::terminate(rval);
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

/// C's usage line, `vis.c:136-139` (note the capital `U`; exit status 1).
const USAGE: &str = "Usage: vis [-bcfhlMmNnoSstw] [-e extra] [-F foldwidth] [file ...]\n";

/// Fold `argv` into [`VisOptions`] the way `getopt(argc, argv, "bcde:F:fhlMmNnoSstw")`
/// does (`vis.c:73-135`): letters cluster inside one token (`-cb`), the two
/// value-taking letters `e` and `F` read the rest of their token (`-ex`, `-F40`)
/// or the following word (`-e x`), `--` ends option scanning, and a lone `-` is
/// an operand. Each `-e` overwrites the previous one, as C's `extra = optarg`
/// assignment does.
fn parse_vis_args(argv: &[String]) -> Result<(VisOptions, bool, Vec<&str>), String> {
    let mut options = VisOptions::default();
    let mut none = false;
    let mut operands: Vec<&str> = Vec::new();
    let mut index = 1usize;
    let mut scanning = true;
    while index < argv.len() {
        let token = argv[index].as_str();
        let bytes = token.as_bytes();
        if !scanning || bytes.len() < 2 || bytes[0] != b'-' {
            operands.push(token);
            index += 1;
            continue;
        }
        if token == "--" {
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
                b'e' => {
                    // Value: the rest of this token, or the next word.
                    index += 1;
                    options.extra = if pos + 1 < bytes.len() {
                        token.as_bytes()[pos + 1..].to_vec()
                    } else if index < argv.len() {
                        argv[index].as_bytes().to_vec()
                    } else {
                        // C's getopt reports a missing option argument as `?`.
                        return Err(String::from(USAGE));
                    };
                    pos = bytes.len();
                    continue;
                }
                b'f' | b'h' | b'm' | b'F' => {
                    // Known to C, not to this deciding half yet: refuse rather
                    // than print an encoding the user did not ask for.
                    let letter = bytes[pos] as char;
                    return Err(format!("vis: flag -{letter} is not modelled yet\n"));
                }
                _ => return Err(String::from(USAGE)), // includes `-d`: C compiles it out too
            }
            pos += 1;
        }
        index += 1;
    }
    Ok((options, none, operands))
}
