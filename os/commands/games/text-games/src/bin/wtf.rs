//! Minix-RS wtf — the doing half over `minix_text_games::acronym`.
//!
//! Ground truth: `minix3/games/wtf/wtf` (`-f` names a database, `-o` accepts
//! only whole words, a leading `is` is removed while later `is` tokens are
//! still looked up; a database line is `term: expansion`). The decision half
//! (line parsing, case-insensitive first-match lookup) lives in the library;
//! this shell assembles arguments, reads the database, prints
//! `term: expansion` word by word, and exits 1 if any word is unknown.
//!
//! The default database is `/usr/share/misc/acronyms` (NetBSD wtf's default
//! path).

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

#[path = "../bin_support.rs"]
mod support;
use minix_text_games::acronym::matching_expansions;

const DEFAULT_DB: &str = "/usr/share/misc/acronyms";

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();
    let mut db_files: Vec<&str> = Vec::new();
    let mut at = 1;
    // The source of truth `minix3/games/wtf/wtf:16-27` runs `getopts f:o`, and
    // any unrecognised flag gets `usage` and exit 1 (measured: `sh
    // minix3/games/wtf/wtf -Z nasa` → first the shell's own `Illegal option
    // -Z`, then the program's usage, exit 1). The earlier shell swallowed `-Z`
    // as a query word. That `Illegal option …` line is printed by the shell,
    // not by wtf — the same conclusion as for the `wtf` write leg: the verdict
    // does not belong to this program, so this shell only reuses its own
    // usage line.
    //
    // Where the ring stops is decided by `getopts`: it stops at the first
    // non-option token, **without reordering**, and the remaining run goes
    // back to the positional parameters through the script's `:30`
    // `shift "$(expr "$OPTIND" - 1)"`.
    while at < args.len() {
        match args[at] {
            "-f" => {
                at += 1;
                match args.get(at).copied() {
                    Some(path) => {
                        // Script `:21` is `acronyms="$OPTARG $acronyms"` — every
                        // `-f` is **prepended to the list**, so `-f w1 -f w2`
                        // searches w2 first. Measured: with `LOL` in both
                        // files the output is `LOL: second file` newline
                        // `first file`.
                        db_files.insert(0, path);
                        at += 1;
                    }
                    // Measured: `sh … -f` → one shell line
                    // `No arg for -f option` + the program's usage, exit 1.
                    // Neither matches this shell's old invented
                    // `wtf: -f needs a database path`, so the shell now falls
                    // back to usage.
                    None => usage(),
                }
            }
            "-o" => {
                // The whole-word-only flag: the decision half's lookup is
                // already whole-word, so its semantics are implied. Actually
                // picking the `*-o` family of database files would need glob
                // expansion, which needs directory enumeration this crate does
                // not have yet.
                at += 1;
            }
            "--" => {
                at += 1;
                break;
            }
            leading if leading.starts_with('-') && leading.len() > 1 => usage(),
            _ => break,
        }
    }
    // The script does exactly one thing with `is` (`:32-34`): `if [ "$1" =
    // "is" ]; then shift; fi` — it removes **the leading** one, once, and
    // compares the literal (`IS` does not count; it is a query word). Every
    // token after that is a query word, **including a later `is`**. Measured
    // for the three shapes (`sh minix3/games/wtf/wtf -f
    // minix3/share/misc/acronyms …`):
    // `y2k is` → one "I don't know what …" each for `y2k` and `is`;
    // `is is y2k` → `is` first, then `y2k` (only the leading one is removed);
    // `IS lol` → one not-found line for `IS`, then still prints
    // `LOL: laughing out loud`, exit 1.
    // The old form skipped `is` at any position inside the query loop and made
    // it the case-insensitive `is_skipped_word`, so all three shapes diverged
    // from the reference build (exit codes too).
    let mut terms: Vec<&str> = args[at.min(args.len())..].to_vec();
    if terms.first().copied() == Some("is") {
        terms.remove(0);
    }
    if terms.is_empty() {
        usage();
    }

    // Script `:42-56` builds the file list: when `-f` was given its value is
    // used (`ACRONYMDB` is **not consulted at all** in that case; measured:
    // `ACRONYMDB=<database with that word> sh … -f /etc/hostname lol` → not
    // found); otherwise, when `ACRONYMDB` is non-empty its whole string is the
    // list (`acronyms="$ACRONYMDB"`, and then `for f in $acronyms` splits on
    // **whitespace** — measured: `ACRONYMDB="w1  w2 "` with runs and trailing
    // spaces still finds the entry inside w2); only when both are empty does it
    // glob `/usr/share/misc/acronyms*` — that branch needs directory
    // enumeration, which this crate cannot reach today (`dents` lives in
    // `minix-fileops`), so it falls back to the single default name; that shape
    // is a recorded gap.
    let env_list = if db_files.is_empty() {
        env_value("ACRONYMDB").unwrap_or_default()
    } else {
        String::new()
    };
    let mut files: Vec<&str> = db_files;
    if files.is_empty() {
        files.extend(env_list.split_ascii_whitespace());
    }
    if files.is_empty() {
        files.push(DEFAULT_DB);
    }

    // Every file in the list must be checked **first**: script `:64-69` is a
    // separate front loop testing `[ ! -f $f ]` per file, and the first file
    // that cannot be opened prints `cannot open acronym database file
    // \`<file>'` and **exits 1 on the spot** — later good files are not
    // searched and no match line is printed (measured: `-f <file with LOL> -f
    // <missing> lol` produces only that line and exits 1; the reverse order
    // behaves the same). Only after the front loop does it read every file into
    // one line pool (`:76`'s `fgrep -h $acronyms` searches all files as a
    // single stream), in list order.
    // Opening and reading files goes through `support::slurp_operand`'s twin
    // face (real-hardware leg `minix_sys::{open, read, close}`, host leg
    // `std::fs`). The earlier version wrote `minix_sys::open` directly, so
    // **any database file that really exists on the host reported `cannot
    // open` and exited 1** — the same defect that was fixed for `fortune`.
    let mut lines: Vec<&str> = Vec::new();
    let mut pool: Vec<String> = Vec::new();
    for path in &files {
        let image = match support::slurp_operand(path) {
            Ok(image) => image,
            Err(support::OperandFailure::Open(_)) => {
                // The script's literal line is `wtf: cannot open acronym
                // database file \`<path>'` (`:67`; the measurement agrees).
                support::warn(
                    format!("wtf: cannot open acronym database file `{path}'\n").as_bytes(),
                );
                support::terminate(1);
            }
        };
        pool.push(String::from_utf8_lossy(&image).into_owned());
    }
    for text in &pool {
        lines.extend(text.lines());
    }

    let mut status = 0;
    for term in &terms {
        // Script `:75` upper-cases the query word first (`tr '[a-z]' '[A-Z]'`)
        // and uses that for the output; measured: `ACRONYMDB=<database> sh …
        // nasa` → `NASA: National Aeronautics and Space Administration`. The
        // earlier shell printed the query word as given, which lost that
        // ground-truth detail of "which form gets printed".
        let shown = term.to_ascii_uppercase();
        let expansions = matching_expansions(&lines, term);
        if !expansions.is_empty() {
            // When one word matches **several lines** (a duplicate entry in one
            // file, or the word in two files), `:77`'s `sed` keeps every
            // matching line and `:80`'s `echo "$target: $ans"` prefixes
            // `TARGET: ` to the **first line** only, leaving later lines bare.
            // Measured: two `LOL` lines in one file → `LOL: first match`
            // newline `second match`.
            let joined = expansions.join("\n");
            support::emit(format!("{shown}: {joined}\n").as_bytes());
            continue;
        }
        {
            // The script's own line uses the **operand as given**, not the
            // upper-cased target: `:105` says `echo "$PROGNAME: I don't know
            // what \`$i' means!"` (`$i` is the shifted argument; `$target` is
            // only used in the matching branch). Measured: `… zzz` → ``I don't
            // know what `zzz' means!``; this shell once printed `ZZZ` and has
            // been changed back. The sentence itself goes to stderr, and at the
            // end `rv=1` (`:106-107`).
            support::warn(format!("wtf: I don't know what `{term}' means!\n").as_bytes());
            status = 1;
        }
    }
    support::terminate(status)
}

/// The value of `NAME` in the environment: follow C's `getenv`, take the
/// **first** entry with the `NAME=` prefix and strip the prefix.
/// Same method as `os/commands/bin/fileops/src/bin/du.rs:env_value` — the host
/// and real-hardware twin faces each live in their own `bin_support`, so this
/// scan is kept once per copy too (hoisting it into shared code would cross
/// crate boundaries, which is an `os/libs/` ownership decision and out of
/// scope here).
fn env_value(name: &str) -> Option<String> {
    let prefix = format!("{name}=");
    support::envs().iter().find_map(|entry| {
        let text = core::str::from_utf8(entry).ok()?;
        text.strip_prefix(prefix.as_str()).map(String::from)
    })
}

/// The script's `usage()` (`minix3/games/wtf/wtf:11-14`) is an `echo` to
/// **standard output** followed by exit 1; measured per stream: stdout gets
/// `usage: wtf [-o] [-f dbfile] [is] term ...` and stderr is empty. The earlier
/// shell sent it to stderr and the text had no `[-o]`.
fn usage() -> ! {
    support::emit(b"usage: wtf [-o] [-f dbfile] [is] term ...\n");
    support::terminate(1);
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
