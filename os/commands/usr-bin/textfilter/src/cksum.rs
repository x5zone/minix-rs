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
//! twice (`sum2.c:48-65`). The MD5/SHA family of the C (`-a` algorithms)
//! needs crypto primitives and is declared unsupported.
//!
//! The table builds as a `const` from the polynomial — 256 entries with
//! no runtime cost, no transcription risk.

use alloc::vec::Vec;

/// One entry per byte value, MSB-first CRC-32 over the polynomial
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

/// One classified line of a `cksum -c` check file. C's `do_check`
/// (`cksum.c:283-460`) reads a line, strips the newline, and branches on
/// whether it carries the crypto-hash grammar (`… (file) = digest`) or the
/// sum/cksum grammar (`<checksum> [length] name`).
#[derive(Debug, PartialEq, Eq)]
pub enum CheckEntry<'a> {
    /// sum/cksum line: C takes the checksum from the first token (`buf` up to
    /// the first space) and the file name from after the **last** space
    /// (`strrchr(buf, ' ') + 1`); the printed length column is not compared —
    /// C checks only `val == strtoul(cksum)` (`cksum.c:443`). Names containing
    /// spaces therefore resolve to their trailing word, matching C.
    Checksum { crc: u32, name: &'a str },
    /// The line contains `(` — the `name (file) = digest` hash grammar. It
    /// needs a crypto primitive we do not carry, so it cannot be verified.
    Hash,
    /// No space to split on — C's "bogus format: … Skipping…" case.
    Malformed,
}

/// Parse the leading base-10 digits of `token`, mirroring C `strtoul(…, 10)`:
/// digits up to the first non-digit, `0` when it starts with a non-digit.
fn leading_u32(token: &str) -> u32 {
    let digits: &str = {
        let end = token
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(token.len());
        &token[..end]
    };
    digits.parse::<u32>().unwrap_or(0)
}

/// Classify one check-file `line` (the trailing newline already removed).
pub fn parse_check_line(line: &str) -> CheckEntry<'_> {
    if line.contains('(') {
        return CheckEntry::Hash;
    }
    let Some(first_space) = line.find(' ') else {
        return CheckEntry::Malformed;
    };
    let crc = leading_u32(&line[..first_space]);
    let name = &line[line.rfind(' ').unwrap() + 1..];
    CheckEntry::Checksum { crc, name }
}

/// Everything `cksum` was asked to do, folded out of argv by the C `main`
/// getopt loop (`cksum.c:193-279`, optstring `"a:cno:pqs:twx"`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CksumRequest<'a> {
    /// Which sum to compute (`-a`/`-o` fold this; default `Crc`).
    pub kind: SumKind,
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
    /// The file operands (or `-`/stdin marker), in argv order.
    pub operands: Vec<&'a str>,
}

/// Why argv could not be folded into a request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CksumError {
    /// `-o` (or `-a`) carried a value with no supported sum behind it.
    InvalidAlgorithm,
    /// `-a <algo>` named a crypto hash (md5/sha…) — needs primitives we lack.
    CryptoUnsupported,
    /// `-p`/`-s`/`-t`/`-x` call `requirehash` (`cksum.c:149`), which the
    /// CRC/old-sum path never satisfies.
    RequiresHash(char),
    /// A flag outside C's optstring; C routes it to `usage()`.
    UnknownOption(char),
}

/// Fold argv into a [`CksumRequest`]. Recognises every C getopt letter so an
/// option is never mistaken for a file operand (the previous parser pushed
/// unrecognised `-n`/`-q`/`-p`/`-s`/`-t`/`-x` straight into the file list).
/// Options are matched as whole argv entries (`-o 1`/`-a crc` keep their value
/// in the next entry), matching how C is invoked and the prior bin read argv.
pub fn parse_cksum_args<'a>(args: &[&'a str]) -> Result<CksumRequest<'a>, CksumError> {
    let mut request = CksumRequest {
        kind: SumKind::Crc,
        do_check: false,
        check_warn: false,
        print_normal: false,
        print_quiet: false,
        operands: Vec::new(),
    };
    let mut index = 0;
    while index < args.len() {
        let arg = args[index];
        // A lone `-`, an empty string, or anything not starting with `-` is a
        // file operand (C getopt leaves a bare `-` to `argv[optind..]`).
        if arg.len() <= 1 || !arg.starts_with('-') {
            request.operands.push(arg);
            index += 1;
            continue;
        }
        let flag = arg.as_bytes()[1] as char;
        match arg {
            "-o" => {
                index += 1;
                let value = *args.get(index).ok_or(CksumError::InvalidAlgorithm)?;
                request.kind = match value {
                    "1" => SumKind::Old1,
                    "2" => SumKind::Old2,
                    _ => return Err(CksumError::InvalidAlgorithm),
                };
            }
            "-a" => {
                index += 1;
                let value = *args.get(index).ok_or(CksumError::InvalidAlgorithm)?;
                // C's `-a` accepts the non-crypto `crc`/`old1`/`old2` (strcasecmp);
                // the MD5/SHA family needs primitives we do not carry.
                if value.eq_ignore_ascii_case("crc") {
                    request.kind = SumKind::Crc;
                } else if value.eq_ignore_ascii_case("old1") {
                    request.kind = SumKind::Old1;
                } else if value.eq_ignore_ascii_case("old2") {
                    request.kind = SumKind::Old2;
                } else {
                    return Err(CksumError::CryptoUnsupported);
                }
            }
            "-c" => request.do_check = true,
            "-w" => request.check_warn = true,
            "-n" => request.print_normal = true,
            "-q" => request.print_quiet = true,
            "-p" | "-s" | "-t" | "-x" => return Err(CksumError::RequiresHash(flag)),
            _ => return Err(CksumError::UnknownOption(flag)),
        }
        index += 1;
    }
    Ok(request)
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_parse_check_line_sum_and_cksum_format() {
        // "<crc> <length> <name>" — length is present but not compared by C.
        assert_eq!(
            parse_check_line("1219131554 3 abc"),
            CheckEntry::Checksum {
                crc: 1219131554,
                name: "abc"
            }
        );
        // Two tokens: first is the crc, the rest-after-last-space is the name.
        assert_eq!(
            parse_check_line("4294967295 a.txt"),
            CheckEntry::Checksum {
                crc: 4294967295,
                name: "a.txt"
            }
        );
    }

    #[test]
    fn test_parse_check_line_name_is_after_the_last_space() {
        // C uses strrchr(' ')+1, so a name with spaces resolves to its trailing
        // word — a documented C limitation, not a bug we diverge on.
        assert_eq!(
            parse_check_line("123 456 my file.txt"),
            CheckEntry::Checksum {
                crc: 123,
                name: "file.txt"
            }
        );
    }

    #[test]
    fn test_parse_check_line_classifies_hash_and_malformed() {
        // The crypto hash grammar carries a '(' — unverifiable without primitives.
        assert_eq!(parse_check_line("md5 (f) = d41d8cd98f00"), CheckEntry::Hash);
        // A single token with no space is "bogus format".
        assert_eq!(parse_check_line("nospace"), CheckEntry::Malformed);
    }

    #[test]
    fn test_parse_check_line_leading_digits_only() {
        // strtoul stops at the first non-digit; a non-numeric start yields 0.
        assert_eq!(
            parse_check_line("1234abc 9 f"),
            CheckEntry::Checksum {
                crc: 1234,
                name: "f"
            }
        );
        assert_eq!(
            parse_check_line("abc 9 f"),
            CheckEntry::Checksum { crc: 0, name: "f" }
        );
    }

    #[test]
    fn test_parse_cksum_args_keeps_options_out_of_operands() {
        // `-n`/`-q` are C print modes (no-op on the crc path) and must be
        // recognised, not pushed as file operands — the previous parser leaked
        // them into the file list.
        let req = parse_cksum_args(&["-n", "-q", "f"]).unwrap();
        assert!(req.print_normal);
        assert!(req.print_quiet);
        assert_eq!(req.operands, ["f"]);
    }

    #[test]
    fn test_parse_cksum_args_requires_hash_flags() {
        // `-p/-s/-t/-x` call requirehash; they error instead of leaking as files.
        assert_eq!(
            parse_cksum_args(&["-p"]),
            Err(CksumError::RequiresHash('p'))
        );
        assert_eq!(
            parse_cksum_args(&["-x", "y"]),
            Err(CksumError::RequiresHash('x'))
        );
    }

    #[test]
    fn test_parse_cksum_args_a_and_o_algorithms() {
        assert_eq!(parse_cksum_args(&["-a", "crc"]).unwrap().kind, SumKind::Crc);
        assert_eq!(
            parse_cksum_args(&["-a", "OLD1"]).unwrap().kind,
            SumKind::Old1
        );
        assert_eq!(
            parse_cksum_args(&["-a", "old2"]).unwrap().kind,
            SumKind::Old2
        );
        assert_eq!(
            parse_cksum_args(&["-a", "md5"]),
            Err(CksumError::CryptoUnsupported)
        );
        assert_eq!(parse_cksum_args(&["-o", "2"]).unwrap().kind, SumKind::Old2);
        assert_eq!(
            parse_cksum_args(&["-o", "9"]),
            Err(CksumError::InvalidAlgorithm)
        );
        assert_eq!(parse_cksum_args(&["-o"]), Err(CksumError::InvalidAlgorithm));
    }

    #[test]
    fn test_parse_cksum_args_check_warn_and_edge_forms() {
        let req = parse_cksum_args(&["-w", "-c", "ck.txt"]).unwrap();
        assert!(req.do_check);
        assert!(req.check_warn);
        assert_eq!(req.operands, ["ck.txt"]);
        assert_eq!(
            parse_cksum_args(&["-Z"]),
            Err(CksumError::UnknownOption('Z'))
        );
        // A bare `-` is a file/stdin operand, not an option letter.
        assert_eq!(parse_cksum_args(&["-"]).unwrap().operands, ["-"]);
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
