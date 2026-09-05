//! Cave adventure vocabulary and movement.
//!
//! Ground truth: `minix3/games/adventure/hdr.h` (at most `HTSIZE 512`
//! vocabulary words near line 78, object descriptions near line 101, initial
//! object placement near line 116) and `vocab.c` (object movement near line
//! 72: objects at or below one hundred track rooms, above track fixed spots).
//! The great map and prose stay with data files; this module owns word
//! lookup, carrying, and exits.

use crate::TextGameError;

/// Largest vocabulary size accepted.
pub const MAX_WORDS: usize = 512;

/// Word kinds understood by the parser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WordKind {
    /// Movement word (goes somewhere).
    Motion,
    /// Object word (picks up something).
    Object,
    /// Action word (does something).
    Action,
}

/// One vocabulary entry (word plus kind plus numeric meaning).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VocabEntry<'a> {
    /// The word itself (lowercase).
    pub word: &'a str,
    /// Word kind.
    pub kind: WordKind,
    /// Numeric meaning (motion number, object number, action number).
    pub code: u32,
}

/// Vocabulary table behind the parser.
pub trait VocabTable<'a> {
    /// Entry for `word` (case insensitive), or `None` when unknown.
    fn lookup(&self, word: &str) -> Option<VocabEntry<'a>>;
    /// Number of stored words.
    fn len(&self) -> usize;
    /// True when no word is stored.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Table backed by a borrowed slice.
pub struct SliceVocab<'a> {
    words: &'a [VocabEntry<'a>],
}

impl<'a> SliceVocab<'a> {
    /// Build a table over borrowed entries.
    pub fn new(words: &'a [VocabEntry<'a>]) -> Self {
        SliceVocab { words }
    }
}

impl<'a> VocabTable<'a> for SliceVocab<'a> {
    fn lookup(&self, word: &str) -> Option<VocabEntry<'a>> {
        self.words.iter().find(|entry| entry.word.eq_ignore_ascii_case(word)).copied()
    }

    fn len(&self) -> usize {
        self.words.len()
    }
}

/// Empty vocabulary (every word unknown).
pub struct EmptyVocab;

impl<'a> VocabTable<'a> for EmptyVocab {
    fn lookup(&self, _word: &str) -> Option<VocabEntry<'a>> {
        None
    }

    fn len(&self) -> usize {
        0
    }
}

/// Look up a word, reporting the Unix miss number when unknown.
pub fn parse_word<'a, T: VocabTable<'a>>(table: &T, word: &str) -> Result<VocabEntry<'a>, TextGameError> {
    if word.is_empty() {
        return Err(TextGameError::InvalidArgument);
    }
    table.lookup(word).ok_or(TextGameError::NotFound)
}

/// One map exit (motion word code plus destination room).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Exit {
    /// Motion code that uses this exit.
    pub motion: u32,
    /// Destination room number.
    pub room: u32,
}

/// Follow `motion` from the exits of the current room.
pub fn follow_exit(exits: &[Exit], motion: u32) -> Result<u32, TextGameError> {
    exits
        .iter()
        .find(|exit| exit.motion == motion)
        .map(|exit| exit.room)
        .ok_or(TextGameError::NotFound)
}

/// Move an object: values at or below one hundred track rooms, above track
/// fixed spots (mirrors the movement split in the original source).
pub fn move_object(object: u32, room: u32) -> (u32, u32) {
    if object <= 100 {
        (room, 0)
    } else {
        (0, room)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_words() -> [VocabEntry<'static>; 3] {
        [
            VocabEntry { word: "north", kind: WordKind::Motion, code: 1 },
            VocabEntry { word: "lamp", kind: WordKind::Object, code: 2 },
            VocabEntry { word: "take", kind: WordKind::Action, code: 3 },
        ]
    }

    #[test]
    fn test_lookup_case_insensitive() {
        let words = sample_words();
        let table = SliceVocab::new(&words);
        assert_eq!(table.len(), 3);
        assert_eq!(parse_word(&table, "NORTH").unwrap().code, 1);
        assert_eq!(parse_word(&table, "lamp").unwrap().kind, WordKind::Object);
        assert_eq!(parse_word(&table, "xyz"), Err(TextGameError::NotFound));
        assert_eq!(parse_word(&table, ""), Err(TextGameError::InvalidArgument));
    }

    #[test]
    fn test_empty_vocab_misses() {
        let table = EmptyVocab;
        assert_eq!(table.len(), 0);
        assert_eq!(parse_word(&table, "north"), Err(TextGameError::NotFound));
    }

    #[test]
    fn test_exits_followed() {
        let exits = [Exit { motion: 1, room: 8 }, Exit { motion: 2, room: 3 }];
        assert_eq!(follow_exit(&exits, 2), Ok(3));
        assert_eq!(follow_exit(&exits, 9), Err(TextGameError::NotFound));
    }

    #[test]
    fn test_object_movement_split() {
        assert_eq!(move_object(50, 8), (8, 0));
        assert_eq!(move_object(150, 8), (0, 8));
    }
}
