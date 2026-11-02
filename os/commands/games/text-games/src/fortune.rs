//! Fortune quips over the `strfile` two-file model.
//!
//! Ground truth: `minix3/games/fortune/fortune/fortune.c` and the on-disk
//! format of `minix3/games/fortune/strfile/strfile.h`. The command reads a
//! text file plus a `.dat` index beside it (`is_fortfile` builds the `.dat`
//! name, `fortune.c:751-799`): the index holds a 24-byte `STRFILE` header
//! (`strfile.h:31-45`; five big-endian `u32` fields, then four spare bytes
//! whose first byte is the delimiter) followed by `numstr + 1` big-endian
//! 8-byte seek points (`strfile.c:451-461` writes them, `fortune.c:979-982`
//! reads one back with `BE64TOH`). A quip is read by seeking the text file to
//! its point and taking lines until the delimiter line — a line whose first
//! byte is the delimiter and whose second byte is a newline
//! (`STR_ENDSTRING`, `strfile.h:29-30`; the read loop is `display`,
//! `fortune.c:265-276`) — rotating each line first when `STR_ROTATED` is set
//! (`rot13`, `fortune.c:247-258`).
//!
//! Validation goes past what C checks: C trusts the index once the header
//! read succeeds and dies of a division by zero when it records zero quips
//! (`fortune.c:956/:1007`, the adjudicated divergence behind "no fortunes in
//! database"). This reader rejects a short header, a version other than 1, a
//! seek-point table that does not fit the image, and any seek point beyond
//! the text image, all as the one "invalid database" verdict.

use alloc::vec::Vec;

/// The one structural failure of the `.dat` reader. C's own shape reports
/// `errx(1, "Database `%s' corrupted", ...)` only for a short header read
/// (`fortune.c:1115-1117`) and trusts everything else; the adjudication picks
/// one readable sentence for every structural rejection instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidDatabase;

impl core::fmt::Display for InvalidDatabase {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("invalid fortune database")
    }
}

/// The `STR_ROTATED` flag (`strfile.h:38`): the text is rot-13'd.
pub const STR_ROTATED: u32 = 0x4;

/// A validated `.dat` index over a text image.
pub struct FortuneIndex<'a> {
    text: &'a [u8],
    /// `numstr + 1` seek points, each within the text image.
    offsets: Vec<u64>,
    delim: u8,
    rotated: bool,
}

impl<'a> FortuneIndex<'a> {
    /// Parse the `.dat` image against the text image it indexes.
    ///
    /// `dat` must carry the 24-byte header (`strfile.h:31-45`) plus
    /// `numstr + 1` big-endian 8-byte seek points (`strfile.c:451-461`);
    /// every point must land inside `text`, or the index is invalid.
    pub fn parse(dat: &'a [u8], text: &'a [u8]) -> Result<Self, InvalidDatabase> {
        if dat.len() < 24 {
            // C: the same short-read rejection at `fortune.c:1115-1117`.
            return Err(InvalidDatabase);
        }
        let be32 = |i: usize| u32::from_be_bytes([dat[i], dat[i + 1], dat[i + 2], dat[i + 3]]);
        if be32(0) != 1 {
            // VERSION 1 (`strfile.h:31`); C reads it without the byte swap
            // (`fortune.c:1120` keeps the conversion commented out) but every
            // tool since writes it swapped, so a non-1 version is corruption.
            return Err(InvalidDatabase);
        }
        let numstr = be32(4) as usize;
        let flags = be32(16);
        let delim = dat[20]; // `str_delim`, `stuff[0]` (`strfile.h:44-45`)
        if (dat.len() - 24) / 8 < numstr + 1 {
            // The seek-point table must be complete: `strfile` writes exactly
            // `numstr + 1` points (`strfile.c:250-253`).
            return Err(InvalidDatabase);
        }
        let mut offsets = Vec::with_capacity(numstr + 1);
        for i in 0..=numstr {
            let base = 24 + i * 8;
            let point = u64::from_be_bytes(dat[base..base + 8].try_into().expect("8 bytes"));
            if point > text.len() as u64 {
                return Err(InvalidDatabase);
            }
            offsets.push(point);
        }
        Ok(FortuneIndex {
            text,
            offsets,
            delim,
            rotated: flags & STR_ROTATED != 0,
        })
    }

    /// Number of stored quips (`str_numstr`).
    pub fn len(&self) -> usize {
        self.offsets.len() - 1
    }

    /// True when the index records no quip (C dies here; see the module head).
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Whether the text is rot-13'd (`STR_ROTATED`).
    pub fn rotated(&self) -> bool {
        self.rotated
    }

    /// Quip `index` as raw text bytes: from its seek point to the delimiter
    /// line, or to end of file when no delimiter line follows (C reads line
    /// by line with `fgets` and stops at `STR_ENDSTRING`, `fortune.c:268-275`,
    /// so the delimiter line itself is never part of the output).
    pub fn quip_bytes(&self, index: usize) -> Option<&'a [u8]> {
        // The table holds `numstr + 1` points: `numstr` quip starts plus one
        // terminal point. Past the last quip there is nothing to read.
        if index + 1 >= self.offsets.len() {
            return None;
        }
        let start = self.offsets[index] as usize;
        let mut line_start = start;
        loop {
            if line_start >= self.text.len() {
                return Some(&self.text[start..]);
            }
            let line_is_delim = self.text[line_start] == self.delim
                && line_start + 1 < self.text.len()
                && self.text[line_start + 1] == b'\n';
            if line_is_delim {
                return Some(&self.text[start..line_start]);
            }
            let newline = self.text[line_start..]
                .iter()
                .position(|&byte| byte == b'\n')?;
            line_start += newline + 1;
        }
    }
}

/// C's `rot13` (`fortune.c:247-258`): only ASCII letters move, everything
/// else passes through unchanged.
pub fn rot13_bytes(bytes: &[u8]) -> Vec<u8> {
    bytes
        .iter()
        .map(|&byte| match byte {
            b'A'..=b'Z' => b'A' + (byte - b'A' + 13) % 26,
            b'a'..=b'z' => b'a' + (byte - b'a' + 13) % 26,
            other => other,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The golden `.dat` is produced by the C tool itself:
    /// `strfile` built from `minix3/games/fortune/strfile/strfile.c` over
    /// `tests/fixtures/golden.txt` ("first quip\nsecond quip\n%\nthird one\n%\n").
    const GOLDEN_DAT: &[u8] = include_bytes!("../tests/fixtures/golden.txt.dat");
    const GOLDEN_TEXT: &[u8] = include_bytes!("../tests/fixtures/golden.txt");
    const ROT13_DAT: &[u8] = include_bytes!("../tests/fixtures/rot13.txt.dat");
    const EMPTY_DAT: &[u8] = include_bytes!("../tests/fixtures/empty.txt.dat");

    #[test]
    fn golden_index_counts_two_quips_and_reads_both() {
        let index = FortuneIndex::parse(GOLDEN_DAT, GOLDEN_TEXT).unwrap();
        assert_eq!(index.len(), 2);
        assert_eq!(index.quip_bytes(0).unwrap(), b"first quip\nsecond quip\n");
        assert_eq!(index.quip_bytes(1).unwrap(), b"third one\n");
        assert_eq!(index.quip_bytes(2), None, "past the end");
        assert!(!index.rotated());
        assert!(!index.is_empty());
    }

    #[test]
    fn the_delimiter_line_is_never_output_and_midline_delim_is_not_a_boundary() {
        // "a%b\n" — the '%' sits mid-line, so C's line-level `STR_ENDSTRING`
        // never fires and the whole text is one quip.
        let text = b"weird%case\nkeep going\n%\nlast\n%\n";
        let dat = build_dat(2, b'%', 0, &[0, 24, 31], text.len());
        let index = FortuneIndex::parse(&dat, text).unwrap();
        assert_eq!(
            index.quip_bytes(0).unwrap(),
            &b"weird%case\nkeep going\n"[..]
        );
        assert_eq!(index.quip_bytes(1).unwrap(), &b"last\n"[..]);
    }

    #[test]
    fn the_rotated_flag_is_reported_and_rot13_moves_only_letters() {
        let index = FortuneIndex::parse(ROT13_DAT, GOLDEN_TEXT).unwrap();
        assert!(index.rotated());
        assert_eq!(
            rot13_bytes(b"Abc xyz!"),
            b"Nop klm!".as_slice(),
            "C's rot13 (`fortune.c:247-258`) leaves non-letters alone"
        );
    }

    #[test]
    fn an_index_recording_zero_quips_parses_and_reports_empty() {
        // C divides by this count and dies (`fortune.c:956`); the reader
        // returns a valid empty index and the shell turns it into the
        // adjudicated "no fortunes in database" sentence.
        let index = FortuneIndex::parse(EMPTY_DAT, &[]).unwrap();
        assert!(index.is_empty());
        assert_eq!(index.quip_bytes(0), None);
    }

    #[test]
    fn structural_rejections_are_one_verdict() {
        let text: &[u8] = GOLDEN_TEXT;
        // Short header (C's corrupted-table leg, `fortune.c:1115-1117`).
        assert!(matches!(
            FortuneIndex::parse(&GOLDEN_DAT[..23], text),
            Err(InvalidDatabase)
        ));
        // Wrong version.
        let mut bad_version = GOLDEN_DAT.to_vec();
        bad_version[3] = 9;
        assert!(matches!(
            FortuneIndex::parse(&bad_version, text),
            Err(InvalidDatabase)
        ));
        // Seek-point table truncated.
        assert!(matches!(
            FortuneIndex::parse(&GOLDEN_DAT[..40], text),
            Err(InvalidDatabase)
        ));
        // A seek point beyond the text image.
        let mut out_of_range = GOLDEN_DAT.to_vec();
        out_of_range[31] = 0x7f; // offset[0] becomes huge
        assert!(matches!(
            FortuneIndex::parse(&out_of_range, text),
            Err(InvalidDatabase)
        ));
    }

    /// Build a minimal well-formed `.dat` image for hand-made texts.
    fn build_dat(
        numstr: usize,
        delim: u8,
        flags: u32,
        points: &[usize],
        _text_len: usize,
    ) -> Vec<u8> {
        let mut dat = Vec::new();
        dat.extend_from_slice(&1u32.to_be_bytes());
        dat.extend_from_slice(&(numstr as u32).to_be_bytes());
        dat.extend_from_slice(&0u32.to_be_bytes());
        dat.extend_from_slice(&0u32.to_be_bytes());
        dat.extend_from_slice(&flags.to_be_bytes());
        dat.extend_from_slice(&[delim, 0, 0, 0]);
        for point in points {
            dat.extend_from_slice(&(*point as u64).to_be_bytes());
        }
        dat
    }
}
