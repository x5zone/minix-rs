//! Checksums for `cksum`: the POSIX CRC and the two historic sums.
//!
//! Ground truth: `minix3/usr.bin/cksum/`. The default algorithm is the
//! CRC of crc.c: a 256-entry MSB-first table over the polynomial
//! `0x04c11db7` (`crctab`, crc.c:53-108; entries verified reproducible
//! from the polynomial), fed byte-wise MSB-first
//! (`COMPUTE`, crc.c:123: `crc = crc << 8 ^ crctab[crc >> 24 ^ byte]`),
//! then the length little-endian low byte first
//! (crc.c:131-133), and the final value is the bitwise complement
//! (crc.c:135). Cross-checked byte-for-byte against the system
//! `cksum`: empty → `4294967295 0`, `"a"` → `1220704766 1`, `"abc"`
//! → `1219131554 3`, `"hello world"` → `1135714720 11`.
//!
//! `-o 1` selects the historic 16-bit rotating sum (`sum1.c:44-56`:
//! rotate right, add, mask to 16 bits); `-o 2` the additive sum folded
//! twice (`sum2.c:48-65`). C's eight `-a` digest algorithms (the `hashes[]`
//! table at `cksum.c:120-144`) all have their bodies in `src/digest.rs`, so
//! this module only **selects which one** and **formats** the output; C's
//! remaining `-s`/`-t`/`-x` legs and the `-c` check leg inside hash are not
//! wired yet, and the thin shell speaks for those two legs.
//!
//! The table builds as a `const` from the polynomial — 256 entries with
//! no runtime cost, no transcription risk.

use crate::digest::Algo;
use alloc::vec::Vec;

/// One entry per byte value, MSB-first CRC32 over the polynomial
/// 0x04c11db7 (the C `crctab`).
const CRC_TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut index = 0usize;
    while index < 256 {
        let mut entry = (index as u32) << 24;
        let mut bit = 0;
        while bit < 8 {
            entry = if entry & 0x8000_0000 != 0 {
                (entry << 1) ^ 0x04c11db7
            } else {
                entry << 1
            };
            bit += 1;
        }
        table[index] = entry;
        index += 1;
    }
    table
};

/// The default `cksum` algorithm over the whole input: returns
/// `(crc, length)` for the `%u %lld` output line.
pub fn cksum_crc(data: &[u8]) -> (u32, u64) {
    let mut crc: u32 = 0;
    let mut length: u64 = 0;
    for &byte in data {
        length += 1;
        crc = (crc << 8) ^ CRC_TABLE[(crc >> 24) as usize ^ (byte as usize)];
    }
    // Include the length of the input, low byte first (crc.c:131-133).
    let mut remaining = length;
    while remaining != 0 {
        crc = (crc << 8) ^ CRC_TABLE[(crc >> 24) as usize ^ (remaining & 0xFF) as usize];
        remaining >>= 8;
    }
    (!crc, length)
}

/// `-o 1`: the historic 16-bit rotating sum (`csum1`, sum1.c:44-56).
pub fn csum1(data: &[u8]) -> (u32, u64) {
    let mut crc: u32 = 0;
    let mut length: u64 = 0;
    for &byte in data {
        length += 1;
        if crc & 1 != 0 {
            crc |= 0x10000;
        }
        crc = ((crc >> 1) + byte as u32) & 0xFFFF;
    }
    (crc, length)
}

/// `-o 2`: the additive sum folded twice into 16 bits
/// (`csum2`, sum2.c:48-66).
pub fn csum2(data: &[u8]) -> (u32, u64) {
    let mut crc: u32 = 0;
    let mut length: u64 = 0;
    for &byte in data {
        length += 1;
        crc = crc.wrapping_add(byte as u32);
    }
    crc = (crc & 0xFFFF) + (crc >> 16);
    crc = (crc & 0xFFFF) + (crc >> 16);
    (crc, length)
}

/// Which length the print step shows. The three algorithms share the value-and-
/// length output line but disagree on the length's unit (`print.c`: `pcrc`
/// prints the raw byte count, `psum1` rounds up to 1024-byte blocks, `psum2`
/// to 512-byte blocks).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SumKind {
    /// `cksum` (default CRC): print the byte count verbatim.
    Crc,
    /// `-o 1` / `sum`: print 1024-byte blocks, rounded up.
    Old1,
    /// `-o 2`: print 512-byte blocks, rounded up.
    Old2,
}

/// The second column `print.c` emits for `kind` given the input `bytes`.
pub fn sum_display_length(kind: SumKind, bytes: u64) -> u64 {
    match kind {
        SumKind::Crc => bytes,
        // C `(len + 1023) / 1024` and `(len + 511) / 512`, i.e. round up.
        SumKind::Old1 => bytes.div_ceil(1024),
        SumKind::Old2 => bytes.div_ceil(512),
    }
}

/// The classification of one line in a check file, as C's `do_check`
/// (`cksum.c:283-465`) performs it.
///
/// Classification uses only two criteria — whether the line contains `(`, and
/// the **current `hash` state** — and the second makes two lines of the same
/// shape take different paths: once `hash` is recognized by some line
/// (`hash = nhash`, `:363`), later ordinary lines without `(` also take the
/// digest branch (`:373-389`), comparing the whole first word against a
/// recomputed digest. The reference measured all three shapes h-1/h-3/h-4
/// failing this way with the `(MD5) ` prefix, and h-4 passes **no `-a`**: the
/// first line's prefix in the file decides the algorithm by itself.
#[derive(Debug, PartialEq, Eq)]
pub enum CheckLine<'a> {
    /// Digest form `NAME (file) = digest`. The file name runs from after the
    /// first `(` (`strchr`, `:314`) to before the last `)` (`strrchr`, `:322`),
    /// so `MD5 (a (f1) = …` names the file `a (f1` (reference shape g-7), and
    /// `MD5 (f1)) = …` names it `f1)` (shape g-6). The digest starts at the
    /// **fourth character after** the last `)` (`p_cksum += 4` at `:331`, which
    /// skips exactly the four characters `) = `), so the spelling `) =x` with
    /// one space missing yields the empty string, and fewer than four characters
    /// after `)` also yields the empty string (shapes g-3/g-4 both measured as
    /// `FAILED`).
    /// `algo` is the entry the line prefix is recognized as in `hashes[]`
    /// (case-sensitive; see `Algo::by_line_prefix`); `None` is C's
    /// `unknown hash: <whole line>` (`:358`, reference shapes
    /// g-1/g-5?/g-10/h-12).
    Hash {
        algo: Option<Algo>,
        name: &'a str,
        digest: &'a str,
    },
    /// Two-column form `<checksum> <file>`. `token` is always the segment before
    /// the first space (both C branches, `:404` and `:389`); how `name` is taken
    /// **branches on the `hash` state**: when nothing is selected it is what
    /// follows the **last** space (`strrchr`, `:406`), so `123 f1 x` names the
    /// file `x` (reference shape h-6); when selected it is what follows the
    /// leading run of whitespace (`while (isspace((int)*++p_filename))`,
    /// `:386-387`), so a file name containing spaces is kept whole.
    Token { token: &'a str, name: &'a str },
    /// C's `bogus format: %s. Skipping...`: a line with `(` but no `)` (`:322-328`),
    /// or a line with neither `(` nor a space while `hash` is unselected
    /// (`:395-401`). Both produce the same sentence.
    Malformed,
    /// C's other sentence `no filename in %s? Skipping...` (`:377-383`):
    /// **reachable only through the path where `hash` is already selected** —
    /// the same shape reports the previous sentence when `hash` is unselected
    /// (the `:406-412` branch is reached only with a space but an empty file
    /// name; the reference's h-7 versus h-8/h-9 shows this). The two sentences
    /// differ, and so do the readings under `-w`.
    NoFilename,
}

/// Parse the leading base-10 digits of `token`, mirroring C `strtoul(…, 10)`:
/// digits up to the first non-digit, `0` when it starts with a non-digit
/// (`cksum.c:442`). Only the branch with `hash` unselected uses it in the check
/// leg; the selected branch compares the whole string (`strcmp` at `:424-426`).
pub fn leading_u32(token: &str) -> u32 {
    let digits: &str = {
        let end = token
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(token.len());
        &token[..end]
    };
    digits.parse::<u32>().unwrap_or(0)
}

/// Split one check-file line the way C classifies it (the caller has already
/// stripped the newline). `hash` is C's **state carried across lines**, so this
/// function must be called with the current state; a returned
/// `Hash { algo: Some(..) }` is the new state the caller adopts next (C both
/// changes the state and uses it to judge the failing line's prefix in the same
/// place, `:363` and `:454`).
pub fn classify_check_line(line: &str, hash: Option<Algo>) -> CheckLine<'_> {
    if let Some(open) = line.find('(') {
        let rest = &line[open + 1..];
        let Some(close) = rest.rfind(')') else {
            return CheckLine::Malformed;
        };
        let after = &rest[close + 1..];
        CheckLine::Hash {
            algo: Algo::by_line_prefix(line),
            name: &rest[..close],
            // C's `p_cksum += 4` advances the pointer from the closing
            // parenthesis itself to the first character of the digest: `)`,
            // ` `, `=`, ` ` — four steps (`cksum.c:331`). `after` has already
            // skipped the `)` itself, so three characters are stripped here.
            // With fewer than three (`) ` not followed by ` = `) C's pointer
            // runs to the end of the string and yields the empty string (shapes
            // g-3/g-4 measured as `FAILED`), the same rule as here.
            digest: if after.len() >= 3 { &after[3..] } else { "" },
        }
    } else if let Some(first_space) = line.find(' ') {
        let token = &line[..first_space];
        if hash.is_some() {
            // C first advances the pointer past one space, then eats the whole
            // run of whitespace (`:386-387`); the file name starts at the first
            // non-whitespace byte.
            let bytes = line.as_bytes();
            let mut index = first_space;
            while index < bytes.len() && bytes[index].is_ascii_whitespace() {
                index += 1;
            }
            CheckLine::Token {
                token,
                name: &line[index..],
            }
        } else {
            // The unselected branch uses `strrchr` (`:406`): after the last
            // space.
            let last_space = line.rfind(' ').unwrap_or(first_space);
            CheckLine::Token {
                token,
                name: &line[last_space + 1..],
            }
        }
    } else if hash.is_some() {
        CheckLine::NoFilename
    } else {
        CheckLine::Malformed
    }
}

/// Everything `cksum` was asked to do, folded out of argv by the C `main`
/// getopt loop (`cksum.c:193-279`, optstring `"a:cno:pqs:twx"`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CksumRequest<'a> {
    /// Which sum to compute (`-a`/`-o` fold this; default `Crc`).
    pub kind: SumKind,
    /// C's `const struct hash *hash` (`cksum.c:162`): the algorithm `-a` selected, or
    /// `None` while the CRC/old-sum path is in force. The name table exists in
    /// one place only, `Algo::from_hashname` in `src/digest.rs`; use
    /// `Algo::hashname()` when the uppercase name is needed.
    /// Two of C's diagnostics are driven by this state alone — `-a` a second time
    /// (`cksum.c:195-199`) and `-o` after a selection (`:228-234`) — and both
    /// `usage()` before any digest is computed, so they are reachable without the
    /// primitives themselves.
    pub hash: Option<Algo>,
    /// `-c`: verify a check file rather than print sums.
    pub do_check: bool,
    /// `-w`: warn on each bogus/unreadable check line.
    pub check_warn: bool,
    /// `-n`: `PRINT_NORMAL`. C folds it into `print_flags`, which only shapes
    /// the crypto-hash output; it is accepted here so the flag is not mistaken
    /// for a file operand, and has no effect on the CRC/old-sum path.
    pub print_normal: bool,
    /// `-q`: `PRINT_QUIET`, same hash-output-only role as `-n`.
    pub print_quiet: bool,
    /// C's `pflag` (`cksum.c:247-251`, set at `:250`): after an algorithm is
    /// selected it makes the standard-input leg **copy the bytes it reads
    /// verbatim** to standard output, then print the digest at the end
    /// (`MD5Filter` in `md5.c`:
    /// `if (pipe && (len != fwrite(buffer, 1, len, stdout))) err(1, "stdout")`).
    /// It can only be read here after an algorithm is selected, so it never
    /// reaches a diagnostic branch.
    pub pipe: bool,
    /// C's `nohashstdin` (`cksum.c:258`, `:264`, `:273`): set when any of
    /// `-s`/`-t`/`-x` appears. It means two things at once — "this leg has
    /// already printed its output" and "the final digest of standard input must
    /// not be computed" (`else if (hash && !nohashstdin)` at `cksum.c:489`).
    pub nohash_stdin: bool,
    /// The file operands (or `-`/stdin marker), in argv order. C hands `argv`
    /// to `filefunc`/`hash_digest_file` as raw bytes, so these are the bytes the
    /// shell received, not a decoded view of them.
    pub operands: Vec<&'a [u8]>,
}

/// Why argv could not be folded into a request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CksumError {
    /// A value-taking option (`a`/`o`/`s`) with nothing after it: libc
    /// `getopt`'s `option requires an argument -- <letter>`
    /// (`getopt.c:132-136`), followed by usage.
    MissingValue(char),
    /// The `-a`/`-o` value is not a recognized name: C does
    /// `warnx("illegal argument to -a|-o option")` + `usage()` (`cksum.c:219`,
    /// `:243`), **not** an invented "invalid algorithm", and does not exit 2.
    IllegalOptionValue(char),
    /// `-a` again after a selection: C's
    /// `warnx("illegal use of -a option")` followed by `usage()`
    /// (`cksum.c:195-199`).
    IllegalUseOfA,
    /// `-o` after a hash has been selected: C's
    /// `warnx("%s mutually exclusive with sum", hash->hashname)` followed by
    /// `usage()` (`cksum.c:228-234`), with the uppercase name in the sentence.
    MutuallyExclusiveWithSum(&'static str),
    /// `-p`/`-s`/`-t`/`-x` call `requirehash` (`cksum.c:149`), which the
    /// CRC/old-sum path never satisfies. C's sentence is
    /// `warnx("%s flag requires `-a algorithm'", flg)` followed by usage
    /// (`cksum.c:521-525`).
    RequiresHash(char),
    /// A flag outside C's optstring; C routes it to `usage()`.
    UnknownOption(char),
}

/// C's three legs `-s`/`-t`/`-x` that **finish printing inside the getopt loop
/// itself** (`cksum.c:255-274` call `stringfunc`/`timetrialfunc`/
/// `testsuitefunc`; all three bodies live in `minix3/usr.bin/cksum/md5.c`, shared
/// by the eight algorithms through `#include "md5.c"`). The algorithm is carried
/// because C uses the `hash` in force: `-p`/`-s`/`-t`/`-x` all pass
/// `requirehash` first, so by the time execution reaches here an algorithm is
/// always selected.
///
/// The order follows argv, and **a later option error cannot take it back**:
/// for reference shape e-1 (`-a md5 -s abc -Z`), stdout has the string-digest
/// line, stderr has libc's `unknown option`, and the exit code is 1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CksumAction<'a> {
    /// `-s <string>`: C's `MD5String` (`md5.c:60-67`), one line
    /// `NAME ("<string>") = <digest>`. The string is argv's raw bytes — C hands
    /// it straight to `printf("…\"%s\"…")` without asking about encoding.
    String(Algo, &'a [u8]),
    /// `-t`: C's `MD5TimeTrial` (`md5.c:73-112`), four lines; the last two
    /// change with the wall clock.
    TimeTrial(Algo),
    /// `-x`: C's `MD5TestSuite` (`md5.c:118-133`), `NAME test suite:` plus eight
    /// fixed-string lines.
    TestSuite(Algo),
}

/// Fold argv into a [`CksumRequest`], scanning in libc `getopt`'s shape
/// (`"a:cno:pqs:twx"` at `cksum.c:193`): a token may cluster several letters
/// (`-nq`), a value-taking letter's value may be the rest of this token (`-ao`,
/// `-sabc`) or the next word (`-a md5`), `--` ends the scan, and a bare `-` is
/// an operand. The old implementation treated the whole token as one letter, so
/// `-nq` became the unknown letter `nq`.
///
/// The three legs `-s`/`-t`/`-x` are **print-as-they-scan** in C
/// (`cksum.c:255-274` call `stringfunc`/`timetrialfunc`/`testsuitefunc`
/// directly), so their order follows argv, and a later option error cannot stop
/// the lines already printed. The collection face therefore lives here and is
/// handed to the caller instead of being folded into [`CksumRequest`]: the error
/// path has no request to hang it on.
fn name_matches_ignore_case(value: &[u8], want: &[u8]) -> bool {
    value.len() == want.len()
        && value
            .iter()
            .zip(want)
            .all(|(got, expected)| got.eq_ignore_ascii_case(expected))
}

pub fn parse_cksum_args_into<'a>(
    args: &[&'a [u8]],
    actions: &mut Vec<CksumAction<'a>>,
) -> Result<CksumRequest<'a>, CksumError> {
    let mut request = CksumRequest {
        kind: SumKind::Crc,
        hash: None,
        do_check: false,
        check_warn: false,
        print_normal: false,
        print_quiet: false,
        pipe: false,
        nohash_stdin: false,
        operands: Vec::new(),
    };
    let mut index = 0;
    let mut scanning = true;
    while index < args.len() {
        let arg = args[index];
        // A lone `-`, an empty string, or anything not starting with `-` is a
        // file operand (C getopt leaves a bare `-` to `argv[optind..]`).
        if !scanning || arg.len() <= 1 || arg[0] != b'-' {
            request.operands.push(arg);
            index += 1;
            continue;
        }
        if arg == b"--" {
            scanning = false;
            index += 1;
            continue;
        }
        let bytes = arg;
        let mut pos = 1usize;
        while pos < bytes.len() {
            let flag = bytes[pos] as char;
            // Taking the value of a value-taking letter: prefer the rest of this
            // token (`-amd5`), otherwise the next word (`-a md5`); if neither
            // exists, it is libc's missing-value line (the old implementation
            // treated it as an empty value and then printed an invented
            // sentence). The index advances only when **another** token is
            // actually taken — the current token is advanced by the loop tail.
            // Advancing in both places swallows an extra word: `-a crc file`
            // would then skip `file` and fall back to reading standard input.
            let take_value = |parser_index: &mut usize| -> Result<&'a [u8], CksumError> {
                if pos + 1 < bytes.len() {
                    return Ok(&bytes[pos + 1..]);
                }
                match args.get(*parser_index + 1) {
                    Some(word) => {
                        *parser_index += 1;
                        Ok(word)
                    }
                    None => Err(CksumError::MissingValue(flag)),
                }
            };
            match flag {
                'o' => {
                    let value = take_value(&mut index)?;
                    // C checks `hash` on the first line of `case 'o'`
                    // (`cksum.c:228-234`); the value itself has already been
                    // consumed by getopt, so the missing-value line is still
                    // printed first by libc.
                    if let Some(algo) = request.hash {
                        return Err(CksumError::MutuallyExclusiveWithSum(algo.hashname()));
                    }
                    request.kind = if value == b"1".as_slice() {
                        SumKind::Old1
                    } else if value == b"2".as_slice() {
                        SumKind::Old2
                    } else {
                        return Err(CksumError::IllegalOptionValue('o'));
                    };
                    break;
                }
                'a' => {
                    let value = take_value(&mut index)?;
                    // C's `case 'a'` first checks whether `hash` was already
                    // selected (`cksum.c:195-199`); if so it is
                    // `illegal use of -a option` — even when this second name
                    // is itself valid.
                    if request.hash.is_some() {
                        return Err(CksumError::IllegalUseOfA);
                    }
                    // Only then is the name table searched by `strcasecmp`
                    // (`:200-207`); if that misses, `old1`/`old2`/`crc` are tried
                    // (`:209-224`), and failing that it is
                    // `illegal argument to -a option`.
                    if let Some(algo) = Algo::from_hashname_bytes(value) {
                        request.hash = Some(algo);
                    } else if name_matches_ignore_case(value, b"crc") {
                        request.kind = SumKind::Crc;
                    } else if name_matches_ignore_case(value, b"old1") {
                        request.kind = SumKind::Old1;
                    } else if name_matches_ignore_case(value, b"old2") {
                        request.kind = SumKind::Old2;
                    } else {
                        return Err(CksumError::IllegalOptionValue('a'));
                    }
                    break;
                }
                's' => {
                    // `-s` passes `requirehash` first (`cksum.c:255-259` +
                    // `:521-525`): it reports that sentence only when nothing is
                    // selected, demanding `-a algorithm`. The value itself has
                    // already been taken by getopt, so the missing-value line is
                    // still printed first by libc.
                    let value = take_value(&mut index)?;
                    let Some(algo) = request.hash else {
                        return Err(CksumError::RequiresHash('s'));
                    };
                    // Once allowed, C calls `stringfunc(optarg)` **on the spot**
                    // (`cksum.c:258-259`) and sets `nohashstdin` (`:258`).
                    // `nohashstdin` does not mean "this leg is done" but "do not
                    // compute the final digest of standard input again" (`:489`).
                    actions.push(CksumAction::String(algo, value));
                    request.nohash_stdin = true;
                    // Once the rest of this token is taken as the value, the
                    // token is fully scanned (libc clears the scan pointer after
                    // returning the rest of the token as the value —
                    // `getopt.c:116-117` and `place = EMSG` at `:140`). The
                    // `-a`/`-o` arms break anyway; this arm used not to:
                    // `-sabc` would scan `a`, `b`, `c` as flags again, hit
                    // `case 'a'` and take "bc" as the algorithm name — reference
                    // shape s-5 is `MD5 ("abc") = …`, while this shell's old
                    // reading was one `illegal argument to -a option` line. A
                    // new test bit it out.
                    break;
                }
                'c' => request.do_check = true,
                'w' => request.check_warn = true,
                'n' => request.print_normal = true,
                'q' => request.print_quiet = true,
                'p' => {
                    // `-p` also passes `requirehash` but **does not** touch
                    // `nohashstdin` (`cksum.c:247-251` only set `pflag = 1`):
                    // the standard-input copy leg still runs.
                    if request.hash.is_none() {
                        return Err(CksumError::RequiresHash('p'));
                    }
                    request.pipe = true;
                }
                't' | 'x' => {
                    // These two also pass `requirehash`; once allowed, C calls
                    // `timetrialfunc`/`testsuitefunc` on the spot
                    // (`cksum.c:261-266`, `:270-275`) and sets `nohashstdin` for
                    // each: the time trial and the test suite finish printing by
                    // themselves, so the final standard-input digest is skipped.
                    let Some(algo) = request.hash else {
                        return Err(CksumError::RequiresHash(flag));
                    };
                    actions.push(if flag == 't' {
                        CksumAction::TimeTrial(algo)
                    } else {
                        CksumAction::TestSuite(algo)
                    });
                    request.nohash_stdin = true;
                }
                _ => return Err(CksumError::UnknownOption(flag)),
            }
            pos += 1;
        }
        index += 1;
    }
    Ok(request)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// These tests judge **state folding** (which letter changes which field to
    /// what) and do not care about the printing order of the three
    /// self-printing legs; the collection face has its own tests calling
    /// `parse_cksum_args_into` directly.
    fn parse_cksum_args<'a>(args: &[&'a [u8]]) -> Result<CksumRequest<'a>, CksumError> {
        parse_cksum_args_into(args, &mut Vec::new())
    }

    /// The three self-printing legs enter the collection face in **argv order**,
    /// and a co-occurrence like `-x -s abc` is neither merged nor reordered
    /// (reference shape s-9: the nine `MD5 test suite:` lines first, then
    /// `MD5 ("abc") = …`).
    #[test]
    fn the_self_printing_legs_are_collected_in_argv_order() {
        let mut actions = Vec::new();
        assert!(
            parse_cksum_args_into(
                &[
                    "-a".as_bytes(),
                    "md5".as_bytes(),
                    "-x".as_bytes(),
                    "-s".as_bytes(),
                    "abc".as_bytes()
                ],
                &mut actions
            )
            .is_ok()
        );
        assert_eq!(
            actions,
            [
                CksumAction::TestSuite(Algo::Md5),
                CksumAction::String(Algo::Md5, "abc".as_bytes())
            ],
            "`-x` 在 argv 里在前，采集就必须在先"
        );

        // Repetition repeats the collection: C's getopt loop calls once per hit
        // (reference shape x-5 printed two sets of sixteen lines).
        let mut twice = Vec::new();
        assert!(
            parse_cksum_args_into(
                &[
                    "-a".as_bytes(),
                    "sha1".as_bytes(),
                    "-x".as_bytes(),
                    "-x".as_bytes()
                ],
                &mut twice
            )
            .is_ok()
        );
        assert_eq!(
            twice,
            [
                CksumAction::TestSuite(Algo::Sha1),
                CksumAction::TestSuite(Algo::Sha1)
            ]
        );

        // Attached and clustered values both yield that string: `-sabc` and
        // `-sa` print `("abc")`/`("a")`.
        let mut inline = Vec::new();
        assert!(
            parse_cksum_args_into(
                &["-a".as_bytes(), "md5".as_bytes(), "-sabc".as_bytes()],
                &mut inline
            )
            .is_ok()
        );
        assert_eq!(inline, [CksumAction::String(Algo::Md5, "abc".as_bytes())]);

        // All three legs touch `nohashstdin`, while `-p` does not
        // (`cksum.c:247-251` only set `pflag = 1`).
        assert!(
            parse_cksum_args_into(
                &["-a".as_bytes(), "md5".as_bytes(), "-t".as_bytes()],
                &mut Vec::new()
            )
            .is_ok()
        );
        assert!(
            !parse_cksum_args(&["-a".as_bytes(), "md5".as_bytes(), "-p".as_bytes()])
                .unwrap()
                .nohash_stdin
        );
    }

    /// The error shape must still keep the lines already printed: C calls
    /// `stringfunc` inside the getopt loop, and a later `usage()` cannot take it
    /// back (reference shapes e-1/e-2/e-3 all have the `MD5 ("abc") = …` line on
    /// stdout, and the exit code is still 1). The collection face runs before the
    /// return precisely for this path.
    #[test]
    fn actions_survive_a_later_option_error() {
        for tail in [
            vec!["-Z".as_bytes()],
            vec!["-o".as_bytes(), "1".as_bytes()],
            vec!["-a".as_bytes(), "sha1".as_bytes()],
        ] {
            let mut actions = Vec::new();
            let mut argv = vec![
                "-a".as_bytes(),
                "md5".as_bytes(),
                "-s".as_bytes(),
                "abc".as_bytes(),
            ];
            argv.extend(tail.iter());
            assert!(
                parse_cksum_args_into(&argv, &mut actions).is_err(),
                "{argv:?} 该报错"
            );
            assert_eq!(
                actions,
                [CksumAction::String(Algo::Md5, "abc".as_bytes())],
                "{argv:?} 报错之前那一行已经印出去了"
            );
        }
    }

    #[test]
    fn test_crc_table_matches_the_polynomial() {
        // Spot checks against the C crctab (crc.c:53-108): entries 0, 1,
        // 2, 255.
        assert_eq!(CRC_TABLE[0], 0x0);
        assert_eq!(CRC_TABLE[1], 0x04c11db7);
        assert_eq!(CRC_TABLE[2], 0x09823b6e);
        assert_eq!(CRC_TABLE[255], 0xb1f740b4);
    }

    #[test]
    fn test_cksum_known_values_from_the_system() {
        // Cross-checked against the system cksum on this host.
        assert_eq!(cksum_crc(b""), (4294967295, 0));
        assert_eq!(cksum_crc(b"a"), (1220704766, 1));
        assert_eq!(cksum_crc(b"abc"), (1219131554, 3));
        assert_eq!(cksum_crc(b"hello world"), (1135714720, 11));
    }

    #[test]
    fn test_sum_display_length_block_rounding() {
        // CRC shows bytes; old1 rounds to 1024-blocks, old2 to 512-blocks.
        assert_eq!(sum_display_length(SumKind::Crc, 11), 11);
        assert_eq!(sum_display_length(SumKind::Old1, 0), 0);
        assert_eq!(sum_display_length(SumKind::Old1, 1), 1); // (1+1023)/1024
        assert_eq!(sum_display_length(SumKind::Old1, 1024), 1);
        assert_eq!(sum_display_length(SumKind::Old1, 1025), 2);
        assert_eq!(sum_display_length(SumKind::Old2, 511), 1);
        assert_eq!(sum_display_length(SumKind::Old2, 512), 1);
        assert_eq!(sum_display_length(SumKind::Old2, 513), 2);
    }

    #[test]
    fn test_classify_check_line_sum_and_cksum_format() {
        // "<crc> <length> <name>" — length is present but not compared by C.
        assert_eq!(
            classify_check_line("1219131554 3 abc", None),
            CheckLine::Token {
                token: "1219131554",
                name: "abc"
            }
        );
        // Two tokens: first is the crc, the rest-after-last-space is the name.
        assert_eq!(
            classify_check_line("4294967295 a.txt", None),
            CheckLine::Token {
                token: "4294967295",
                name: "a.txt"
            }
        );
    }

    /// The branch with `hash` unselected takes the file name with `strrchr`
    /// (`cksum.c:406`), so a name containing spaces is cut down to its last
    /// word; the branch with `hash` selected skips the leading whitespace run
    /// (`:386-387`) and keeps the whole string. Two judgments on the same line,
    /// which is the difference between reference shapes h-5/h-6.
    #[test]
    fn the_filename_rule_follows_the_hash_state() {
        assert_eq!(
            classify_check_line("123 456 my file.txt", None),
            CheckLine::Token {
                token: "123",
                name: "file.txt"
            },
            "未选中取最后一个空格之后"
        );
        assert_eq!(
            classify_check_line("123 456 my file.txt", Some(Algo::Md5)),
            CheckLine::Token {
                token: "123",
                name: "456 my file.txt"
            },
            "选中之后取开头那段空白之后"
        );
        // Multiple spaces only affect C's `nspaces`; `token` is still the
        // segment before the first space (`:389`).
        assert_eq!(
            classify_check_line("12345    f1", None),
            CheckLine::Token {
                token: "12345",
                name: "f1"
            }
        );
        assert_eq!(
            classify_check_line("12345    f1", Some(Algo::Sha1)),
            CheckLine::Token {
                token: "12345",
                name: "f1"
            }
        );
    }

    /// Digest form `NAME (file) = digest`: C's file name runs from after the
    /// first `(` to before the last `)`, and the digest starts at the fourth
    /// character after the last `)` (`cksum.c:314`/`:322`/`:331`). All three
    /// malformed forms were measured: one missing space, nothing after `=`, and
    /// an unrecognized line prefix.
    #[test]
    fn the_hash_grammar_splits_at_the_first_open_and_last_close() {
        assert_eq!(
            classify_check_line("MD5 (f1) = d41d8cd98f00b204e9800998ecf8427e", None),
            CheckLine::Hash {
                algo: Some(Algo::Md5),
                name: "f1",
                digest: "d41d8cd98f00b204e9800998ecf8427e"
            }
        );
        // Parentheses inside the file name: the name runs up to before the last
        // `)` (reference shapes g-6/g-7).
        assert_eq!(
            classify_check_line("MD5 (a (f1) = xyz", None),
            CheckLine::Hash {
                algo: Some(Algo::Md5),
                name: "a (f1",
                digest: "xyz"
            }
        );
        assert_eq!(
            classify_check_line("MD5 (f1)) = xyz", None),
            CheckLine::Hash {
                algo: Some(Algo::Md5),
                name: "f1)",
                digest: "xyz"
            }
        );
        // The `) =x` spelling with one space missing: C's `+= 4` is a hard
        // advance that does not look at spaces, and counting four characters
        // already passes `x` itself, so the result is still the empty string
        // (reference shape g-3 measured as `FAILED`).
        assert_eq!(
            classify_check_line("MD5 (f1) =x", None),
            CheckLine::Hash {
                algo: Some(Algo::Md5),
                name: "f1",
                digest: ""
            }
        );
        // Fewer than ` = ` after `)`: C's pointer runs to the end of the string
        // and yields the empty string (shape g-4 measured `FAILED`).
        assert_eq!(
            classify_check_line("MD5 (f1)", None),
            CheckLine::Hash {
                algo: Some(Algo::Md5),
                name: "f1",
                digest: ""
            }
        );
        // The line prefix is **case-sensitive** (`strncmp`, `:343-352`):
        // lower-case `md5` is not recognized, the exact opposite of the
        // `strcasecmp` on the `-a` leg (reference shape g-10).
        assert_eq!(
            classify_check_line("md5 (f1) = d41d8cd98f00b204e9800998ecf8427e", None),
            CheckLine::Hash {
                algo: None,
                name: "f1",
                digest: "d41d8cd98f00b204e9800998ecf8427e"
            }
        );
        // A `(` without a `)` takes `bogus format` (`:322-328`).
        assert_eq!(
            classify_check_line("MD5 (f1 = abc", None),
            CheckLine::Malformed
        );
    }

    /// The "no space" shape reports **two different sentences** on the two
    /// paths: `no filename in %s?` when `hash` is selected (`:377-383`), and
    /// `bogus format: %s.` when it is not (`:395-401`). The difference between
    /// reference h-7 and h-8/h-9 is exactly here.
    #[test]
    fn a_spaceless_line_reports_two_different_sentences_depending_on_state() {
        assert_eq!(classify_check_line("12345", None), CheckLine::Malformed);
        assert_eq!(
            classify_check_line("12345", Some(Algo::Md5)),
            CheckLine::NoFilename
        );
        assert_eq!(classify_check_line("", None), CheckLine::Malformed);
        assert_eq!(
            classify_check_line("", Some(Algo::Md5)),
            CheckLine::NoFilename
        );
    }

    #[test]
    fn leading_u32_follows_strtoul() {
        // strtoul stops at the first non-digit; a non-numeric start yields 0.
        assert_eq!(leading_u32("1234abc"), 1234);
        assert_eq!(leading_u32("abc"), 0);
        assert_eq!(leading_u32(""), 0);
        assert_eq!(leading_u32("4294967295"), 4294967295);
    }

    #[test]
    fn test_parse_cksum_args_keeps_options_out_of_operands() {
        // `-n`/`-q` are C print modes (no-op on the crc path) and must be
        // recognised, not pushed as file operands — the previous parser leaked
        // them into the file list.
        let req = parse_cksum_args(&["-n".as_bytes(), "-q".as_bytes(), "f".as_bytes()]).unwrap();
        assert!(req.print_normal);
        assert!(req.print_quiet);
        assert_eq!(req.operands, ["f".as_bytes()]);
    }

    #[test]
    fn test_parse_cksum_args_requires_hash_flags() {
        // `-p/-s/-t/-x` call requirehash; they error instead of leaking as files.
        assert_eq!(
            parse_cksum_args(&["-p".as_bytes()]),
            Err(CksumError::RequiresHash('p'))
        );
        assert_eq!(
            parse_cksum_args(&["-x".as_bytes(), "y".as_bytes()]),
            Err(CksumError::RequiresHash('x'))
        );
    }

    #[test]
    fn test_parse_cksum_args_a_and_o_algorithms() {
        assert_eq!(
            parse_cksum_args(&["-a".as_bytes(), "crc".as_bytes()])
                .unwrap()
                .kind,
            SumKind::Crc
        );
        assert_eq!(
            parse_cksum_args(&["-a".as_bytes(), "OLD1".as_bytes()])
                .unwrap()
                .kind,
            SumKind::Old1
        );
        assert_eq!(
            parse_cksum_args(&["-a".as_bytes(), "old2".as_bytes()])
                .unwrap()
                .kind,
            SumKind::Old2
        );
        assert_eq!(
            parse_cksum_args(&["-a".as_bytes(), "md5".as_bytes()])
                .unwrap()
                .hash,
            Some(Algo::Md5),
            "选中记的是 C 表里的 hashname（大写那一份，`cksum.c:126`）"
        );
        // All eight entries in the table must be recognized (`cksum.c:120-144`),
        // matching by `strcasecmp`: the name's case does not affect selection,
        // but reports use the uppercase form.
        for (word, name) in [
            ("md2", "MD2"),
            ("MD4", "MD4"),
            ("md5", "MD5"),
            ("rmd160", "RMD160"),
            ("sha1", "SHA1"),
            ("SHA256", "SHA256"),
            ("sha384", "SHA384"),
            ("sha512", "SHA512"),
        ] {
            assert_eq!(
                parse_cksum_args(&["-a".as_bytes(), word.as_bytes()])
                    .unwrap()
                    .hash
                    .map(Algo::hashname),
                Some(name),
                "`-a {word}` 该选中诊断与报文里那一个大写的 {name}"
            );
        }
        // A spelling absent from the table is not an alias: C has no
        // `ripemd160` row, so it lands on `illegal argument to -a option`
        // (after the two-level lookup at `cksum.c:201-224`).
        assert_eq!(
            parse_cksum_args(&["-a".as_bytes(), "ripemd160".as_bytes()]),
            Err(CksumError::IllegalOptionValue('a'))
        );
        // `-a` after a selection: this is the first line of C's `case 'a'`
        // (`cksum.c:195-199`); whether the second name is valid does not matter
        // — `crc` is blocked by it as well.
        assert_eq!(
            parse_cksum_args(&[
                "-a".as_bytes(),
                "md5".as_bytes(),
                "-a".as_bytes(),
                "sha1".as_bytes()
            ]),
            Err(CksumError::IllegalUseOfA)
        );
        assert_eq!(
            parse_cksum_args(&[
                "-a".as_bytes(),
                "md5".as_bytes(),
                "-a".as_bytes(),
                "crc".as_bytes()
            ]),
            Err(CksumError::IllegalUseOfA)
        );
        // `-o` after a selection: the sentence carries the uppercase name
        // (`cksum.c:228-234`).
        assert_eq!(
            parse_cksum_args(&[
                "-a".as_bytes(),
                "md5".as_bytes(),
                "-o".as_bytes(),
                "1".as_bytes()
            ]),
            Err(CksumError::MutuallyExclusiveWithSum("MD5"))
        );
        // The reverse order, `-o` then `-a`, does not conflict: `hash` is still
        // empty at that point.
        assert_eq!(
            parse_cksum_args(&[
                "-o".as_bytes(),
                "1".as_bytes(),
                "-a".as_bytes(),
                "md5".as_bytes()
            ])
            .unwrap()
            .hash,
            Some(Algo::Md5),
        );
        assert_eq!(
            parse_cksum_args(&[
                "-o".as_bytes(),
                "1".as_bytes(),
                "-a".as_bytes(),
                "md5".as_bytes()
            ])
            .unwrap()
            .kind,
            SumKind::Old1,
        );
        // After a selection `-p`/`-s` no longer report
        // `requires \`-a algorithm'` (C's requirehash lets them through); the
        // report appears only when nothing is selected, and that shape is
        // pinned here.
        assert!(parse_cksum_args(&["-a".as_bytes(), "sha1".as_bytes(), "-p".as_bytes()]).is_ok());
        assert!(
            parse_cksum_args(&[
                "-a".as_bytes(),
                "sha1".as_bytes(),
                "-s".as_bytes(),
                "abc".as_bytes()
            ])
            .is_ok()
        );
        assert_eq!(
            parse_cksum_args(&["-p".as_bytes()]),
            Err(CksumError::RequiresHash('p'))
        );
        // The missing value is still reported by libc first, even with a hash
        // selected (getopt demands the value before entering the switch).
        assert_eq!(
            parse_cksum_args(&["-a".as_bytes(), "md5".as_bytes(), "-o".as_bytes()]),
            Err(CksumError::MissingValue('o'))
        );
        // An operand **after** a value-taking option must not be swallowed. The
        // old index arithmetic advanced once at the value site and once at the
        // loop tail, so `-a crc file` skipped `file`, sending a cksum that should
        // hash a file off to read standard input — the attached form
        // `-acrc file` had the same disease. This group of assertions pins
        // "a token is advanced exactly once, by the loop tail".
        assert_eq!(
            parse_cksum_args(&["-a".as_bytes(), "crc".as_bytes(), "file".as_bytes()])
                .unwrap()
                .operands,
            vec!["file".as_bytes()]
        );
        assert_eq!(
            parse_cksum_args(&["-acrc".as_bytes(), "file".as_bytes()])
                .unwrap()
                .operands,
            vec!["file".as_bytes()]
        );
        assert_eq!(
            parse_cksum_args(&[
                "-o".as_bytes(),
                "1".as_bytes(),
                "-a".as_bytes(),
                "sha1".as_bytes(),
                "f1".as_bytes(),
                "f2".as_bytes()
            ])
            .unwrap()
            .operands,
            vec!["f1".as_bytes(), "f2".as_bytes()]
        );
        assert_eq!(
            parse_cksum_args(&[
                "-a".as_bytes(),
                "crc".as_bytes(),
                "-n".as_bytes(),
                "-".as_bytes()
            ])
            .unwrap()
            .operands,
            vec!["-".as_bytes()],
            "裸 `-` 仍是操作数，且不被前一个值吞掉"
        );
        assert_eq!(
            parse_cksum_args(&["-o".as_bytes(), "2".as_bytes()])
                .unwrap()
                .kind,
            SumKind::Old2
        );
        // `-o 9` is "illegal value" (C's `illegal argument to -o option`), while
        // `-o` at end of input with no value is a different thing (libc's
        // missing-value line).
        assert_eq!(
            parse_cksum_args(&["-o".as_bytes(), "9".as_bytes()]),
            Err(CksumError::IllegalOptionValue('o'))
        );
        assert_eq!(
            parse_cksum_args(&["-o".as_bytes()]),
            Err(CksumError::MissingValue('o'))
        );
        assert_eq!(
            parse_cksum_args(&["-a".as_bytes(), "nosuch".as_bytes()]),
            Err(CksumError::IllegalOptionValue('a'))
        );
    }

    /// An unknown letter comes back as the **first byte** after the dash, not the
    /// first character: `-\u{e9}` (`c3 a9`) is U+00C3 in the `char` domain. The
    /// loop side already takes `bytes[pos] as char` (`optopt = *place++` at
    /// `getopt.c:79` takes one byte at a time), so this test pins the taking
    /// method that is **already in place**, so that nobody changes it back to
    /// per-character later; the reference `tmp/cksum-hash-state/cksum_ref`
    /// measured `-é` reporting the single byte `303`, and the byte-level test is
    /// in `tests/cksum_face.rs`.
    #[test]
    fn the_unknown_letter_is_the_first_byte_not_the_first_character() {
        assert_eq!(
            parse_cksum_args(&["-\u{e9}".as_bytes()]),
            Err(CksumError::UnknownOption('\u{00c3}')),
            "`é` 是 c3 a9 两字节，libc 报 c3 而不是整个字符"
        );
        // Cluster: the valid `n` is consumed first, and the report is the first
        // byte after it.
        assert_eq!(
            parse_cksum_args(&["-n\u{e9}".as_bytes()]),
            Err(CksumError::UnknownOption('\u{00c3}')),
            "-n\u{e9}"
        );
        // The loop stops at the first bad letter.
        assert_eq!(
            parse_cksum_args(&["-Z\u{e9}".as_bytes()]),
            Err(CksumError::UnknownOption('Z')),
            "-Z\u{e9}"
        );
        // The ASCII shape must not be affected. Taking `z`: the option string at
        // `cksum.c:193` is `"a:cno:pqs:twx"`, which has no `z` (this side's `q`
        // is a valid letter instead; the first version of this test used it as a
        // counter-example).
        assert_eq!(
            parse_cksum_args(&["-z".as_bytes()]),
            Err(CksumError::UnknownOption('z'))
        );
        // The opposite case: a value-taking letter takes all following bytes as
        // its value, so no byte reaches the letter slot (the reference gives the
        // same sentence `cksum: illegal argument to -a option` followed by
        // usage).
        assert_eq!(
            parse_cksum_args(&["-a\u{e9}".as_bytes()]),
            Err(CksumError::IllegalOptionValue('a'))
        );
    }

    /// getopt's shape: clustering, attached values, and everything after `--`
    /// being operands. The old implementation treated the whole token as one
    /// letter, getting `-nq`, `-o2`, and `-a crc -- -f` wrong.
    #[test]
    fn test_parse_cksum_args_follows_the_getopt_shape() {
        let req = parse_cksum_args(&["-nq".as_bytes(), "f".as_bytes()]).unwrap();
        assert!(req.print_normal && req.print_quiet);
        assert_eq!(req.operands, vec!["f".as_bytes()]);
        // Attached form: `-o2` and `-o 2` are equivalent (`optarg` is the rest
        // of this token).
        assert_eq!(
            parse_cksum_args(&["-o2".as_bytes()]).unwrap().kind,
            parse_cksum_args(&["-o".as_bytes(), "2".as_bytes()])
                .unwrap()
                .kind
        );
        assert_eq!(
            parse_cksum_args(&["-acrc".as_bytes()]).unwrap().kind,
            SumKind::Crc
        );
        // A dash-leading word after `--` is an operand, not a flag.
        let req = parse_cksum_args(&["-c".as_bytes(), "--".as_bytes(), "-o".as_bytes()]).unwrap();
        assert!(req.do_check);
        assert_eq!(req.operands, vec!["-o".as_bytes()]);
        // `-s` first takes getopt's value (rest of this token or the next word),
        // then hits `requirehash`; with no value it reports the missing value
        // rather than requirehash.
        assert_eq!(
            parse_cksum_args(&["-s".as_bytes(), "x".as_bytes()]),
            Err(CksumError::RequiresHash('s'))
        );
        assert_eq!(
            parse_cksum_args(&["-s".as_bytes()]),
            Err(CksumError::MissingValue('s'))
        );
        // An unknown letter inside a cluster is reported as that letter itself.
        assert_eq!(
            parse_cksum_args(&["-ncZ".as_bytes()]),
            Err(CksumError::UnknownOption('Z'))
        );
    }

    #[test]
    fn test_parse_cksum_args_check_warn_and_edge_forms() {
        let req =
            parse_cksum_args(&["-w".as_bytes(), "-c".as_bytes(), "ck.txt".as_bytes()]).unwrap();
        assert!(req.do_check);
        assert!(req.check_warn);
        assert_eq!(req.operands, ["ck.txt".as_bytes()]);
        assert_eq!(
            parse_cksum_args(&["-Z".as_bytes()]),
            Err(CksumError::UnknownOption('Z'))
        );
        // A bare `-` is a file/stdin operand, not an option letter.
        assert_eq!(
            parse_cksum_args(&["-".as_bytes()]).unwrap().operands,
            ["-".as_bytes()]
        );
    }

    #[test]
    fn test_cksum_is_the_bitwise_complement_of_the_running_crc() {
        // The C complements at the end (crc.c:135); an empty input's
        // running CRC is zero, so the result is all ones.
        assert_eq!(cksum_crc(b"").0, u32::MAX);
    }

    #[test]
    fn test_csum1_rotating_16_bit() {
        assert_eq!(csum1(b""), (0, 0));
        assert_eq!(csum1(b"a"), (97, 1));
        assert_eq!(csum1(b"abc\n"), (8288, 4));
        assert_eq!(csum1(b"hello world\n"), (3762, 12));
    }

    #[test]
    fn test_csum2_additive_folded() {
        assert_eq!(csum2(b""), (0, 0));
        assert_eq!(csum2(b"a"), (97, 1));
        assert_eq!(csum2(b"abc\n"), (304, 4));
        assert_eq!(csum2(b"hello world\n"), (1126, 12));
    }
}
