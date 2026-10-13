//! Minix-RS unvis — decode visible-encoded stdin and file operands.
//!
//! Ground truth: `minix3/usr.bin/unvis/unvis.c` (NetBSD). The decoder state
//! machine is the library's `vis::unvis_byte`/`vis::unvis_end`; this program
//! gathers the flags, then drives the machine one input byte at a time and prints
//! what comes back — the same shape as the C's `process()` loop.
//!
//! All four of the C's getopt letters of `"eHhm"` are modelled: `-e`
//! (`VIS_NOESCAPE`) takes the backslash away as an escape introducer, `-h`
//! (`VIS_HTTP1808`) reads `%hh`, `-m` (`VIS_MIMESTYLE`) reads `=HH` and the soft
//! line break, and `-H` (`VIS_HTTP1866`) reads the entity references `&#nn;` and
//! `&name;` (`unvis.c:59-71`). Like the C, the modes are not all combinable:
//! `unvis.c:80-89` refuses `VIS_MIMESTYLE` together with `-h` and/or `-H` before
//! opening anything, so that error outranks an unreadable file, while `-h` and `-H`
//! together are fine — both introducers are then live.
//!
//! With operands each is opened and decoded into one output stream; an unopenable
//! operand warns and is skipped and the exit status still returns success — unlike
//! `vis`, which flags it (`unvis.c:91-104`). Without operands stdin is the source,
//! reported under the name `<stdin>`.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::vis::{DecodeOptions, DecodeOutcome, DecodeState, unvis_byte, unvis_end};

/// C's usage line, `unvis.c:75-78`: `Usage: %s [-e] [-Hh | -m] [file ...]`, with
/// the vertical bar inside the brackets exactly as the C writes it.
const USAGE: &str = "Usage: unvis [-e] [-Hh | -m] [file ...]\n";

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let (options, operands) = match parse_unvis_args(&argv) {
        Ok(parsed) => parsed,
        Err(message) => {
            support::warn(message.as_bytes());
            support::terminate(1);
        }
    };

    // `unvis.c:91-104`: every operand feeds the same output stream and the exit
    // status stays success however many of them cannot be opened.
    let mut out: Vec<u8> = Vec::new();
    if operands.is_empty() {
        process(&support::read_stdin(), "<stdin>", &options, &mut out);
    } else {
        for operand in &operands {
            match support::read_file(operand) {
                Ok(data) => process(&data, operand, &options, &mut out),
                Err(()) => support::warn(format!("unvis: {operand}\n").as_bytes()),
            }
        }
    }
    support::emit(&out);
    support::terminate(0);
}

/// The C's `process()` (`unvis.c:108-140`), one operand at a time. `offset` counts
/// input bytes the way C's `getc` calls do — a byte that the decoder asks to see
/// again is not counted twice, because `UNVIS_VALIDPUSH` jumps back into the
/// `switch` without reading another byte (`:118-124`). A byte that cannot be
/// decoded is reported with its offset and the machine restarts in the ground
/// state; the run continues, since `warnx` is not `errx` (`:126-128`). At end of
/// input the machine is flushed once and only a finished byte is written — an
/// incomplete escape is dropped without a word (`:137-139`).
fn process(data: &[u8], name: &str, options: &DecodeOptions, out: &mut Vec<u8>) {
    let mut state = DecodeState::default();
    let mut offset = 0usize;
    for &byte in data {
        offset += 1;
        let mut current = Some(byte);
        while let Some(pending) = current.take() {
            match unvis_byte(pending, &mut state, options) {
                DecodeOutcome::Byte(value) => out.push(value),
                DecodeOutcome::ByteAndRetry(value) => {
                    out.push(value);
                    current = Some(pending);
                }
                DecodeOutcome::Nothing => {}
                DecodeOutcome::Invalid => {
                    support::warn(
                        format!("unvis: {name}: offset: {offset}: can't decode\n").as_bytes(),
                    );
                    state = DecodeState::default();
                }
            }
        }
    }
    if let DecodeOutcome::Byte(value) = unvis_end(&mut state) {
        out.push(value);
    }
}

/// Fold `argv` into flags and operands the way `getopt(argc, argv, "eHhm")` does
/// (`unvis.c:57-74`): letters cluster inside one token (`-eh`), `--` ends option
/// scanning, and a lone `-` is an operand. The mode combination C refuses by name
/// is checked after the whole loop and before any file is opened (`:80-89`), and
/// `-e` is not part of that test.
fn parse_unvis_args(argv: &[String]) -> Result<(DecodeOptions, Vec<&str>), String> {
    let mut options = DecodeOptions::default();
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
        for &letter in &bytes[1..] {
            match letter {
                b'e' => options.no_escape = true,
                b'h' => options.http1808 = true,
                b'm' => options.mime = true,
                b'H' => options.http1866 = true,
                _ => return Err(String::from(USAGE)),
            }
        }
        index += 1;
    }
    // `unvis.c:80-89` switches on the three style bits together and lists exactly
    // the combinations that include MIME plus at least one HTTP style.
    if options.mime && (options.http1808 || options.http1866) {
        return Err(String::from("unvis: Can't mix -m with -h and/or -H\n"));
    }
    Ok((options, operands))
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
