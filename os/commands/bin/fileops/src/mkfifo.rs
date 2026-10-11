//! `mkfifo` — create named pipes: the pure mode layer.
//!
//! Ground truth: `minix3/usr.bin/mkfifo/mkfifo.c`. The whole decision is which
//! permission bits the new pipe gets: `mkfifo.c:69` starts from `0666 &
//! ~umask(0)`, and `-m` replaces that with `getmode(setmode(optarg), 0666)`
//! (`mkfifo.c:74-81`). The `mkfifo(2)` leg lives in the thin program
//! `src/bin/mkfifo.rs`.
//!
//! Two facts about the `-m` branch are easy to get wrong and are pinned by the
//! tests. First, the base is the *assumed* `a=rw`, i.e. `0666`, not the
//! umask-modified default — the comment at `mkfifo.c:77-80` says so and
//! `getmode` is handed `0666` literally, so `-m 600` is `0600` whatever the
//! caller's mask is and the mask is subtracted only once, on the default path.
//! Second, an octal `-m` value replaces the twelve low bits of that same `0666`
//! (`setmode.c`'s `isdigit` branch), so it is a wholesale setting rather than an
//! addition.

use crate::FileOpError;
use crate::mode::apply_mode;
use alloc::string::String;
use alloc::vec::Vec;

/// `mkfifo.c:69` and `:81`: the mode the spec is measured against. POSIX puts it
/// in words: "the + and - operators are interpreted relative to an assumed
/// initial mode of `a=rw`".
pub const ASSUMED_BASE: u32 = 0o666;

/// Why the command line is not a `mkfifo` invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MkfifoArgError {
    /// Usage and exit 1: an unknown option, `-m` without its value, or no pipe
    /// name at all (`mkfifo.c:84-86`, `:90-91`, `:103-107`).
    Usage,
    /// `-m` whose text `setmode` refused. C reports it with `err(1, "Cannot set
    /// file mode \`%s'")` (`mkfifo.c:74-76`) and stops before creating anything.
    BadMode(String),
}

/// The resolved command line: one mode for every name, in the order given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MkfifoPlan {
    /// The permission bits to create each pipe with, already reduced as C does.
    pub mode: u32,
    pub names: Vec<String>,
}

/// `mkfifo.c:69`: the default mode is `0666` minus the caller's creation mask.
/// `myumask` is the value the program read by setting the mask to zero, which is
/// why the subtraction happens here and not in the file system: Minix3's file
/// servers do not apply the mask to `mknod`/`mkfifo` at all (no `umask` reference
/// anywhere in `minix3/sys/fs/mfs/`), only to `open(O_CREAT)`.
pub fn default_mode(myumask: u32) -> u32 {
    ASSUMED_BASE & !myumask
}

/// `-m SPEC` (`mkfifo.c:74-81`): `getmode` over the assumed `a=rw` base. Octal
/// and symbolic spellings both go, through the same engine `chmod` uses.
pub fn mode_from_spec(spec: &str) -> Result<u32, FileOpError> {
    apply_mode(ASSUMED_BASE, spec).map(u32::from)
}

/// `mkfifo.c:71-91`: BSD `getopt` with `"m:"` — one option, one flag letter, and
/// it takes a value. The first non-option ends the scan and becomes a name, and
/// `--` ends it as well. A second `-m` simply wins, since C assigns the same
/// `mode` variable twice (`mkfifo.c:81`).
pub fn parse_args(args: &[&str], myumask: u32) -> Result<MkfifoPlan, MkfifoArgError> {
    let mut mode = default_mode(myumask);
    let mut i = 0;
    while i < args.len() {
        let a = args[i];
        if a == "--" {
            i += 1;
            break;
        }
        let bytes = a.as_bytes();
        if bytes.len() < 2 || bytes[0] != b'-' {
            break;
        }
        if !bytes[1..].iter().all(|b| *b == b'm') {
            return Err(MkfifoArgError::Usage);
        }
        // Every `m` in the cluster wants its own value, and C's getopt reads it
        // from the following argv entry only (`-m600` would be parsed as the
        // option string "600"), so a glued value is refused rather than guessed.
        for _ in &bytes[1..] {
            let spec = match args.get(i + 1) {
                Some(v) => *v,
                None => return Err(MkfifoArgError::Usage),
            };
            mode = match mode_from_spec(spec) {
                Ok(m) => m,
                Err(_) => return Err(MkfifoArgError::BadMode(String::from(spec))),
            };
            i += 1;
        }
        i += 1;
    }

    let names: Vec<String> = args[i..].iter().map(|s| String::from(*s)).collect();
    if names.is_empty() {
        // `mkfifo.c:90-91`: `argv[0] == NULL` is usage, even if `-m` was valid.
        return Err(MkfifoArgError::Usage);
    }
    Ok(MkfifoPlan { mode, names })
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn default_mode_subtracts_the_creation_mask() {
        // mkfifo.c:69: `0666 & ~umask(0)`.
        assert_eq!(default_mode(0o000), 0o666);
        assert_eq!(default_mode(0o022), 0o644);
        assert_eq!(default_mode(0o077), 0o600);
        assert_eq!(default_mode(0o777), 0o000);
    }

    #[test]
    fn the_default_applies_when_no_m_flag_is_given() {
        assert_eq!(
            parse_args(&["p"], 0o022),
            Ok(MkfifoPlan {
                mode: 0o644,
                names: vec![String::from("p")],
            })
        );
    }

    #[test]
    fn m_flag_octal_replaces_the_assumed_base_wholesale() {
        // mkfifo.c:81 hands `getmode` the literal 0666, so the creation mask is
        // not subtracted a second time on this path.
        assert_eq!(parse_args(&["-m", "600", "p"], 0o022).unwrap().mode, 0o600);
        assert_eq!(
            parse_args(&["-m", "777", "p"], 0o777).unwrap().mode,
            0o777,
            "even a mask that would kill the bits does not reach -m"
        );
    }

    #[test]
    fn m_flag_symbolic_is_relative_to_a_rw() {
        assert_eq!(parse_args(&["-m", "a+x", "p"], 0o022).unwrap().mode, 0o777);
        // `u=r` clears the user class (setuid bit included) and sets only read.
        assert_eq!(parse_args(&["-m", "u=r", "p"], 0o022).unwrap().mode, 0o466);
        assert_eq!(parse_args(&["-m", "g-w", "p"], 0o022).unwrap().mode, 0o646);
        assert_eq!(
            parse_args(&["-m", "o+r", "-m", "u+w", "p"], 0o022)
                .unwrap()
                .mode,
            0o666,
            "the second -m starts again from the assumed base (mkfifo.c:81 assigns 0666), \
             so the first clause is not carried over"
        );
    }

    #[test]
    fn x_adds_nothing_to_an_a_rw_base() {
        // The `X` arm (setmode.c:148-150) tests the mode it is handed, and
        // mkfifo hands it 0666: no directory bit, no execute bit anywhere.
        assert_eq!(parse_args(&["-m", "a+X", "p"], 0o000).unwrap().mode, 0o666);
    }

    #[test]
    fn several_names_keep_their_order_and_one_mode() {
        let p = parse_args(&["-m", "640", "a", "b", "c"], 0o022).unwrap();
        assert_eq!(p.mode, 0o640);
        assert_eq!(
            p.names,
            [String::from("a"), String::from("b"), String::from("c")]
        );
    }

    #[test]
    fn scanning_stops_at_the_first_name_and_dash_dash_ends_it() {
        // BSD getopt does not permute: `-b` after a name is a name, not an
        // option, and it is refused as an option only if it comes first.
        let p = parse_args(&["a", "-b"], 0o022).unwrap();
        assert_eq!(p.names, [String::from("a"), String::from("-b")]);
        let p = parse_args(&["-m", "600", "--", "-p"], 0o022).unwrap();
        assert_eq!(p.names, [String::from("-p")]);
    }

    #[test]
    fn usage_for_unknown_letters_missing_values_and_no_name() {
        assert_eq!(parse_args(&["-z", "p"], 0o022), Err(MkfifoArgError::Usage));
        assert_eq!(parse_args(&["-m"], 0o022), Err(MkfifoArgError::Usage));
        assert_eq!(
            parse_args(&["-m", "600"], 0o022),
            Err(MkfifoArgError::Usage)
        );
        assert_eq!(
            parse_args(&["-m600", "p"], 0o022),
            Err(MkfifoArgError::Usage),
            "a glued value is not the shape C's getopt reads here"
        );
    }

    #[test]
    fn a_bad_mode_is_reported_with_the_text_that_failed() {
        assert_eq!(
            parse_args(&["-m", "zz", "p"], 0o022),
            Err(MkfifoArgError::BadMode(String::from("zz")))
        );
        assert_eq!(
            parse_args(&["-m", "u+", "p"], 0o022),
            Err(MkfifoArgError::BadMode(String::from("u+")))
        );
    }
}
