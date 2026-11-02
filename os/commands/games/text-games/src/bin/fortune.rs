//! Minix-RS fortune — the doing half over `minix_text_games::fortune`.
//!
//! Ground truth: `minix3/games/fortune/fortune/fortune.c` — the two-file
//! model. The operand names the **text** file; the index is that path plus
//! `.dat` (`is_fortfile`, `fortune.c:788-799`). The acceptance order in
//! `add_file` is: open the text file (`err(1, "Cannot open `%s'", path)`,
//! `fortune.c:525` when that fails), then `is_fortfile` gates the name — the
//! last suffix must not be one of the "illegal" ones (`fortune.c:756-762`)
//! and the `.dat` beside it must be readable (`fortune.c:792-795`) — a
//! failure there answers `warnx("`%s' not a fortune file or directory")`
//! (`fortune.c:549-551`). The draw index is taken modulo the recorded quip
//! count (`get_pos`, `fortune.c:960-975`); the drawn quip is seeked and read
//! to the delimiter line (`display`, `fortune.c:265-276`), rotated when the
//! index says so. Two verdicts replace C's crash legs (the cross-board
//! adjudication on the empty-database and corrupt-index shapes): an index
//! that records zero quips answers "no fortunes in database" where C divides
//! by zero (`fortune.c:956/:1007`), and a structurally bad index answers
//! "invalid fortune database" where C's check covers only a short header read
//! (`fortune.c:1115-1117`).

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

#[path = "../bin_support.rs"]
mod support;
use minix_text_games::fortune::{FortuneIndex, InvalidDatabase, rot13_bytes};

const DEFAULT_DB: &str = "/usr/share/games/fortune";

/// The suffixes `is_fortfile` rejects (`fortune.c:756-762`): an operand whose
/// last suffix is one of these is not treated as a fortune file at all.
const ILLEGAL_SUFFIXES: [&str; 13] = [
    "dat", "pos", "c", "h", "p", "i", "f", "pas", "ftn", "ins.c", "ins,pas", "ins.ftn", "sml",
];

/// C's `is_fortfile` name half (`fortune.c:780-786`): a basename starting
/// with a dot, or carrying an illegal last suffix, is not a fortune file.
fn name_is_not_a_fortune_file(shown: &str) -> bool {
    let base = shown.rsplit('/').next().unwrap_or(shown);
    if base.starts_with('.') {
        return true;
    }
    if let Some(dot) = base.rfind('.') {
        let suffix = &base[dot + 1..];
        if ILLEGAL_SUFFIXES.contains(&suffix) {
            return true;
        }
    }
    false
}

/// C's `err(1, "Cannot open `%s'", path)` for the text file
/// (`fortune.c:525`): program name, the sentence with the raw path wrapped in
/// a backtick and a single quote, and the errno sentence. The path entry
/// wants `&str` (registered under `ARGV-BYTE-PATH-IN-LIBS`), so the fold
/// happens only at the call point; the message still reports the raw bytes.
fn cannot_open(path: &[u8], code: minix_sys::Errno) -> ! {
    let argv: Vec<Vec<u8>> = support::args_bytes();
    let mut line: Vec<u8> = support::prog_name_bytes(&argv).to_vec();
    line.extend_from_slice(b": Cannot open `");
    line.extend_from_slice(path);
    line.extend_from_slice(b"': ");
    line.extend_from_slice(minix_sys::Strerror(code).to_string().as_bytes());
    line.push(b'\n');
    support::warn(&line);
    support::terminate(1);
}

/// C's `warnx("`%s' not a fortune file or directory", path)`
/// (`fortune.c:549-551`): no errno sentence, exit 1 through the same
/// `form_file_list` failure that the open failure takes (`fortune.c:213`).
fn not_a_fortune_file(path: &[u8]) -> ! {
    let argv: Vec<Vec<u8>> = support::args_bytes();
    let mut line: Vec<u8> = support::prog_name_bytes(&argv).to_vec();
    line.extend_from_slice(b": `");
    line.extend_from_slice(path);
    line.extend_from_slice(b"' not a fortune file or directory\n");
    support::warn(&line);
    support::terminate(1);
}

fn run() -> ! {
    let argv: Vec<Vec<u8>> = support::args_bytes();
    let args: Vec<&[u8]> = argv.iter().map(Vec::as_slice).collect();
    let path: &[u8] = args.get(1).copied().unwrap_or(DEFAULT_DB.as_bytes());
    let shown = String::from_utf8_lossy(path).into_owned();

    // The text file first (C opens it in `add_file`, `fortune.c:525`).
    // A read that breaks partway is end of input: C loads with
    // `fgets(...) != NULL` (`fortune.c:268`) and never checks `ferror`.
    let image = match support::slurp_operand(&shown) {
        Ok(image) => image,
        Err(support::OperandFailure::Open(code)) => cannot_open(path, code),
    };
    // Then the name gate and the index beside it. In C the `.dat` readability
    // check lives inside `is_fortfile` (`fortune.c:792-795`), so an
    // unopenable index is the warnx shape, not a second "Cannot open".
    if name_is_not_a_fortune_file(&shown) {
        not_a_fortune_file(path);
    }
    let dat_shown = format!("{shown}.dat");
    let dat_image = match support::slurp_operand(&dat_shown) {
        Ok(image) => image,
        Err(support::OperandFailure::Open(_)) => not_a_fortune_file(path),
    };

    // Structural verdicts: one sentence for every bad index, one for an index
    // that records nothing. C's own paths are the corrupted-table `errx` and
    // the division by zero; the adjudicated replacements are below.
    let index = match FortuneIndex::parse(&dat_image, &image) {
        Ok(index) => index,
        Err(InvalidDatabase) => {
            support::warn(b"fortune: invalid fortune database\n");
            support::terminate(1);
        }
    };
    if index.is_empty() {
        support::warn(b"fortune: no fortunes in database\n");
        support::terminate(1);
    }

    let draw = (support::epoch_micros().unwrap_or(0) / 1_000_000) as u32;
    let picked = (draw as usize) % index.len();
    let mut bytes = index.quip_bytes(picked).unwrap_or_default().to_vec();
    if index.rotated() {
        bytes = rot13_bytes(&bytes);
    }
    // C's output leg does not check for failure: `fputs(line, stdout)`
    // (`fortune.c:272`) and `(void) fflush(stdout)` (`:274`) both drop the
    // result, and the file has no `ferror(stdout)` test, so a failed write
    // adds no line and changes no exit status (`exit(0)`, `fortune.c:377`).
    let _ = support::emit_error(&bytes);
    support::terminate(0)
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
