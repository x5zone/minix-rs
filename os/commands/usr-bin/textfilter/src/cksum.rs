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
    ((!crc) as u32, length)
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
