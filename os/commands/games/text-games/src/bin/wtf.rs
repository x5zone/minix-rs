//! Minix-RS wtf — the doing half over `minix_text_games::acronym`.
//!
//! Ground truth: `minix3/games/wtf/wtf` (a `-f` names a database, `-o` accepts whole
//! words only, the leading `is` is removed while later `is` occurrences are still looked
//! up; a database line is `term: expansion`). The deciding half (line parsing,
//! case-insensitive first-match lookup) lives in the library; this shell assembles the
//! arguments, reads the databases, prints `term: expansion` per word, and exits 1 as
//! soon as any word is unknown.
//!
//! The database defaults to `/usr/share/misc/acronyms` (NetBSD wtf's default path).

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

#[path = "../bin_support.rs"]
mod support;
use minix_fileops::dents;
use minix_text_games::acronym::matching_expansions;

/// The C glob's directory read (`wtf:43` expands over `/usr/share/misc`):
/// the entries the shared dents decoder can name. `None` is the host shape
/// — the open stub fails, which is the shell's unmatched-glob case.
fn read_dir_names(path: &str) -> Option<Vec<String>> {
    const O_RDONLY: i32 = 0x0000_0000;
    const WALK_BUF: usize = 4096;
    let fd = minix_sys::open(path, O_RDONLY, 0).ok()?;
    let mut names = Vec::new();
    let mut buf = [0u8; WALK_BUF];
    loop {
        let transferred = match minix_sys::getdents(fd, &mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(_) => {
                let _ = minix_sys::close(fd);
                return None;
            }
        };
        let mut cursor = 0usize;
        while cursor < transferred {
            let entry = match dents::next_record(&buf, cursor) {
                Ok(Some((entry, next))) => {
                    cursor = next;
                    entry
                }
                _ => break,
            };
            let name = alloc::string::String::from_utf8_lossy(entry.name).into_owned();
            if entry.ino == 0 || !dents::is_descendable(&name) {
                continue;
            }
            names.push(name);
        }
    }
    let _ = minix_sys::close(fd);
    Some(names)
}

fn run() -> ! {
    let argv: Vec<Vec<u8>> = support::args_bytes();
    let args: Vec<&[u8]> = argv.iter().map(Vec::as_slice).collect();
    let mut db_files: Vec<&[u8]> = Vec::new();
    let mut offensive = false;
    let mut at = 1;
    // The ground truth `minix3/games/wtf/wtf:16-27` runs `getopts f:o`, and an
    // unrecognized flag always ends in `usage` and exit 1 (`sh minix3/games/wtf/wtf -Z
    // nasa` prints the shell's own `Illegal option -Z` first, then the program's usage,
    // and exits 1).
    // The `Illegal option ...` sentence is printed by the shell, not by wtf, so that
    // judgment does not belong to this program and this shell reuses only wtf's own
    // usage line.
    //
    // Where the loop stops is set by `getopts`: it stops at the first non-option token,
    // without permuting, and the whole remaining run goes back as positional parameters
    // through the script's `:30` `shift "$(expr "$OPTIND" - 1)"`.
    while at < args.len() {
        match args[at] {
            b"-f" => {
                at += 1;
                match args.get(at).copied() {
                    Some(path) => {
                        // The script's `:21` is `acronyms="$OPTARG $acronyms"` — every
                        // `-f` is inserted at the front of the list, so `-f w1 -f w2`
                        // searches w2 first. With `LOL` in both databases the output is
                        // `LOL: second file`, newline, `first file`.
                        db_files.insert(0, path);
                        at += 1;
                    }
                    // `sh ... -f` prints the shell's `No arg for -f option` plus the
                    // program's usage and exits 1; an invented
                    // `wtf: -f needs a database path` would be neither.
                    None => usage(),
                }
            }
            b"-o" => {
                // The flag has two C faces: the lookup is already a
                // whole-word match (the deciding half's anchored compare),
                // and it selects the `*-o` batch of the default database
                // glob (`wtf:45-52`).
                offensive = true;
                at += 1;
            }
            b"--" => {
                at += 1;
                break;
            }
            leading if leading.first() == Some(&b'-') && leading.len() > 1 => usage(),
            _ => break,
        }
    }
    // The script does exactly one thing with `is` (`:32-34`):
    // `if [ "$1" = "is" ]; then shift; fi` — it removes the leading one, once, and
    // compares the literal (`IS` does not count, it is a query term).
    // After that every token is a query term, including an `is` that appears later.
    // Run against `sh minix3/games/wtf/wtf -f minix3/share/misc/acronyms ...`:
    // `y2k is` -> one "I don't know what ..." line for `y2k` and one for `is`;
    // `is is y2k` -> `is` then `y2k` (only the leading one is removed);
    // `IS lol` -> one not-found line for `IS`, then `LOL: laughing out loud`, exit 1.
    // An earlier version skipped `is` at any position in the query loop and made it a
    // case-insensitive `is_skipped_word`, which diverged from C on all three shapes
    // (the exit codes differed too).
    let mut terms: Vec<&[u8]> = args[at.min(args.len())..].to_vec();
    if terms.first().copied() == Some(b"is".as_slice()) {
        terms.remove(0);
    }
    if terms.is_empty() {
        usage();
    }

    // The script's `:42-56` builds the file list: when `-f` was given, the `-f` files
    // win (`ACRONYMDB` is then ignored entirely — `ACRONYMDB=<database containing the
    // word> sh ... -f /etc/hostname lol` finds nothing); otherwise a non-empty
    // `ACRONYMDB` becomes the whole list (`acronyms="$ACRONYMDB"`, then
    // `for f in $acronyms` splits on whitespace, so `ACRONYMDB="w1  w2 "` with its run of
    // spaces and trailing space still reaches the entry in w2); only when both are empty
    // does it glob `/usr/share/misc/acronyms*` — that branch needs directory enumeration,
    // which this crate does not have (`dents` lives in `minix-fileops`), so it falls back
    // to the single default name, a declared limitation.
    let env_list = if db_files.is_empty() {
        env_value("ACRONYMDB").unwrap_or_default()
    } else {
        String::new()
    };
    let mut files: Vec<Vec<u8>> = db_files.iter().map(|f| f.to_vec()).collect();
    if files.is_empty() {
        // `ACRONYMDB` is an environment string, not argv, so it stays `&str`; the open
        // call folds it like every other name.
        files.extend(
            env_list
                .split_ascii_whitespace()
                .map(|f| f.as_bytes().to_vec()),
        );
    }
    if files.is_empty() {
        // C `wtf:43-55`: the shell globs `/usr/share/misc/acronyms*` over
        // `/usr/share/misc`. The directory read rides the shared dents
        // decoder (`minix-fileops`); on a host build the open stub fails,
        // which is exactly the shell's unmatched-glob shape — the literal
        // pattern passes through and the pre-pass fails naming the star.
        let entries = read_dir_names("/usr/share/misc").unwrap_or_default();
        let borrowed: Vec<&str> = entries.iter().map(String::as_str).collect();
        files.extend(
            minix_text_games::acronym::default_database_list(&borrowed, offensive)
                .iter()
                .map(|path| path.as_bytes().to_vec()),
        );
    }
    if files.is_empty() {
        // C `wtf:56-58`: the glob matched, but the `-o` filter emptied the
        // list.
        support::warn(b"wtf: acronym database not found!\n");
        support::terminate(1);
    }

    // C `wtf:64-69`: a per-file precheck before any reading — the first
    // path that is not a regular file prints `cannot open acronym database
    // file \`<file>'` and exits 1; later good files are not searched.
    for path in &files {
        let shown = String::from_utf8_lossy(path).into_owned();
        if !support::is_regular_file(&shown) {
            let mut line: Vec<u8> = b"wtf: cannot open acronym database file `".to_vec();
            line.extend_from_slice(path);
            line.extend_from_slice(b"'\n");
            support::warn(&line);
            support::terminate(1);
        }
    }

    // Every database in the list is checked first: the script's `:64-69` is a separate
    // pre-pass that tests `[ ! -f $f ]` per file, and the first file that cannot be
    // opened prints `cannot open acronym database file \`<file>'` and exits 1 on the
    // spot — later good files are not searched and no matching line is printed
    // (`-f <database with LOL> -f <nonexistent> lol` prints only that line and exits 1;
    // the reverse order does the same).
    // Only after the pre-pass does it read every file into one shared line pool (`:76`'s
    // `fgrep -h $acronyms` searches all files as one stream), in list order.
    // Opening and reading go through `support::slurp_operand`'s twins (the target leg
    // uses `minix_sys::{open, read, close}`, the hosted leg `std::fs`). Hard-coding
    // `minix_sys::open` here, as an earlier version did, reports `cannot open` and exits
    // 1 for any database that really exists on the host — the same defect once fixed in
    // `fortune`.
    let mut lines: Vec<&str> = Vec::new();
    let mut pool: Vec<String> = Vec::new();
    for path in &files {
        // The open call takes `&str` (registered `ARGV-BYTE-PATH-IN-LIBS`), so the fold
        // happens only at this point; what the message reads out is the original bytes.
        let shown = String::from_utf8_lossy(path).into_owned();
        // The precheck above owns the `cannot open` sentence (C `:65-69`).
        // Past it, C's reader runs `fgrep … 2>/dev/null` (`:78`) — a read
        // failure after the precheck is a silent miss, never an error.
        let image = match support::slurp_operand(&shown) {
            Ok(image) => image,
            Err(_) => continue,
        };
        pool.push(String::from_utf8_lossy(&image).into_owned());
    }
    for text in &pool {
        lines.extend(text.lines());
    }

    let mut status = 0;
    for term in &terms {
        // The script's `:75` uppercases the query term first (`tr '[a-z]' '[A-Z]'` —
        // bytewise, so non-ASCII bytes pass through unchanged) and prints that form:
        // `ACRONYDB=<database> sh ... nasa` prints `NASA: National Aeronautics and Space
        // Administration`. Printing the query term as given, as an earlier version did,
        // dropped the "which form gets printed" truth.
        // `shown` follows the raw argv bytes; the lookup call takes `&str` (the line pool
        // is already folded content — the source says so, though it is unmeasured here
        // because this shell has no wtf reference build, so a mismatch between raw bytes
        // in the pool and the query term is not observable), and the fold happens only
        // at the moment of the call — the ASCII test gives the same verdict either way.
        let shown: Vec<u8> = term.iter().map(|b| b.to_ascii_uppercase()).collect();
        let term_text = String::from_utf8_lossy(term).into_owned();
        let expansions = matching_expansions(&lines, &term_text);
        if !expansions.is_empty() {
            // When one word matches several lines (a duplicate entry in one database, or
            // the word present in two), `:77`'s `sed` keeps every matching line and
            // `:80`'s `echo "$target: $ans"` prefixes only the first line with `TARGET: `;
            // later lines come out bare. Two `LOL` lines in one database print
            // `LOL: first match`, newline, `second match`.
            let joined = expansions.join("\n");
            let mut out: Vec<u8> = shown;
            out.extend_from_slice(b": ");
            out.extend_from_slice(joined.as_bytes());
            out.push(b'\n');
            support::emit(&out);
            continue;
        }
        {
            // The script's sentence uses the operand as given, not the uppercased
            // target: `:105` writes
            // `echo "$PROGNAME: I don't know what \`$i' means!"` (`$i` is the shifted
            // argument; `$target` is used only on the match branch). `... zzz` prints
            // ``I don't know what `zzz' means!``; printing `ZZZ`, as this shell once
            // did, is wrong. The sentence goes to stderr and the run ends with `rv=1`
            // (`:106-107`).
            let mut line: Vec<u8> = b"wtf: I don't know what `".to_vec();
            line.extend_from_slice(term);
            line.extend_from_slice(b"' means!\n");
            support::warn(&line);
            status = 1;
        }
    }
    support::terminate(status)
}

/// The value of `NAME` in the environment: like C's `getenv`, take the first string
/// carrying the `NAME=` prefix and strip the prefix.
/// The same approach as `os/commands/bin/fileops/src/bin/du.rs:env_value` — the two
/// hosted/target twins each live in their own `bin_support`, so this scan is kept in
/// each as well (moving it to a shared piece would cross crates, which is an `os/libs/`
/// ownership decision and out of scope here).
fn env_value(name: &str) -> Option<String> {
    let prefix = format!("{name}=");
    support::envs().iter().find_map(|entry| {
        let text = core::str::from_utf8(entry).ok()?;
        text.strip_prefix(prefix.as_str()).map(String::from)
    })
}

/// The script's `usage()` (`minix3/games/wtf/wtf:11-14`) is an `echo` to standard
/// output followed by exit 1: stdout receives
/// `usage: wtf [-o] [-f dbfile] [is] term ...` and stderr stays empty. An earlier
/// version sent it to stderr and left `[-o]` out of the text.
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
