//! Fortune quips.
//!
//! Ground truth: `minix3/games/fortune/fortune/fortune.c` (quip selection
//! through a seek table: first seek near line 267, table reads near line 980)
//! and the data files in `datfiles/`. Large prose ships as program resources;
//! this module owns the quip database shape and uniform picking over it.

use crate::TextGameError;

/// Quip database behind picking.
pub trait QuipDb<'a> {
    /// Quip at `index`, or `None` when past the end.
    fn quip(&self, index: usize) -> Option<&'a str>;
    /// Number of stored quips.
    fn len(&self) -> usize;
    /// True when no quip is stored.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Database backed by a borrowed slice.
pub struct SliceQuips<'a> {
    quips: &'a [&'a str],
}

impl<'a> SliceQuips<'a> {
    /// Build a database over borrowed quips.
    pub fn new(quips: &'a [&'a str]) -> Self {
        SliceQuips { quips }
    }
}

impl<'a> QuipDb<'a> for SliceQuips<'a> {
    fn quip(&self, index: usize) -> Option<&'a str> {
        self.quips.get(index).copied()
    }

    fn len(&self) -> usize {
        self.quips.len()
    }
}

/// Empty database (every pick misses).
pub struct EmptyQuips;

impl<'a> QuipDb<'a> for EmptyQuips {
    fn quip(&self, _index: usize) -> Option<&'a str> {
        None
    }

    fn len(&self) -> usize {
        0
    }
}

/// Pick the quip at `draw` modulo the database size (uniform when `draw` is
/// uniform). Empty databases report not found.
pub fn pick_quip<'a, D: QuipDb<'a>>(db: &D, draw: u32) -> Result<&'a str, TextGameError> {
    if db.is_empty() {
        return Err(TextGameError::NotFound);
    }
    let index = (draw as usize) % db.len();
    db.quip(index).ok_or(TextGameError::NotFound)
}

/// Split `%` separated quip text into quips (the on disk separator shape).
///
/// Empty segments are skipped (leading, trailing, and doubled separators).
pub fn split_quips<'a>(text: &'a str, out: &mut [&'a str]) -> Result<usize, TextGameError> {
    let mut count = 0;
    for segment in text.split("\n%\n") {
        let trimmed = segment.trim();
        if trimmed.is_empty() {
            continue;
        }
        if count >= out.len() {
            return Err(TextGameError::OutOfRange);
        }
        out[count] = trimmed;
        count += 1;
    }
    if count == 0 {
        return Err(TextGameError::NotFound);
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pick_wraps() {
        static QUIPS: &[&str] = &["first", "second", "third"];
        let db = SliceQuips::new(QUIPS);
        assert_eq!(pick_quip(&db, 0).unwrap(), "first");
        assert_eq!(pick_quip(&db, 3).unwrap(), "first");
        assert_eq!(pick_quip(&db, 4).unwrap(), "second");
    }

    #[test]
    fn test_empty_db_misses() {
        let db = EmptyQuips;
        assert!(db.is_empty());
        assert_eq!(pick_quip(&db, 0), Err(TextGameError::NotFound));
    }

    #[test]
    fn test_separator_splits() {
        let text = "first quip\n%\nsecond quip\n%\n";
        let mut out = [""; 4];
        let count = split_quips(text, &mut out).unwrap();
        assert_eq!(count, 2);
        assert_eq!(out[0], "first quip");
        assert_eq!(out[1], "second quip");
    }

    #[test]
    fn test_blank_text_reports_not_found() {
        let mut out = [""; 4];
        assert_eq!(split_quips("", &mut out), Err(TextGameError::NotFound));
        assert_eq!(split_quips("\n%\n", &mut out), Err(TextGameError::NotFound));
    }
}
