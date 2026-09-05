//! Card fishing: ranks, books, and asks.
//!
//! Ground truth: `minix3/games/fish/fish.c` (thirteen `RANKS` near line 61,
//! the full deck near line 64, per rank hands near lines 77 to 78, book
//! counting near line 83, the ask prompt near line 160). A book is four cards
//! of one rank; the game ends when all thirteen books complete. Dealing and
//! prompting stay with the execution layer; this module owns ranks, books,
//! and ask choices.

use crate::TextGameError;

/// Ranks in one suit (ace through king).
pub const RANKS: usize = 13;

/// Cards that complete one book.
pub const BOOK_SIZE: u32 = 4;

/// Parse a rank word (`ace`, `2` through `10`, `jack`, `queen`, `king`).
pub fn parse_rank(word: &str) -> Result<usize, TextGameError> {
    if word.is_empty() {
        return Err(TextGameError::InvalidArgument);
    }
    let lower = word.as_bytes();
    let mut folded = [0u8; 8];
    if lower.len() > folded.len() {
        return Err(TextGameError::InvalidArgument);
    }
    for (index, byte) in lower.iter().enumerate() {
        folded[index] = byte.to_ascii_lowercase();
    }
    match &folded[..lower.len()] {
        b"ace" => Ok(0),
        b"jack" => Ok(10),
        b"queen" => Ok(11),
        b"king" => Ok(12),
        digits => {
            let mut value: u32 = 0;
            for byte in digits {
                if !byte.is_ascii_digit() {
                    return Err(TextGameError::InvalidArgument);
                }
                value = value * 10 + (byte - b'0') as u32;
            }
            if !(2..=10).contains(&value) {
                return Err(TextGameError::InvalidArgument);
            }
            Ok((value - 1) as usize)
        }
    }
}

/// Count completed books in one rank hand (each rank holds zero through four
/// cards; fours become books).
pub fn count_books(hand: &[u32]) -> Result<u32, TextGameError> {
    if hand.len() != RANKS {
        return Err(TextGameError::InvalidArgument);
    }
    let mut books = 0;
    for count in hand {
        if *count > BOOK_SIZE {
            return Err(TextGameError::InvalidArgument);
        }
        if *count == BOOK_SIZE {
            books += 1;
        }
    }
    Ok(books)
}

/// Choose a rank to ask for: the lowest rank held but not yet asked about.
///
/// `held` counts cards per rank, `asked` marks ranks already asked. Reports
/// not found when every held rank was asked (time to draw).
pub fn choose_ask(held: &[u32], asked: &[bool]) -> Result<usize, TextGameError> {
    if held.len() != RANKS || asked.len() != RANKS {
        return Err(TextGameError::InvalidArgument);
    }
    held.iter()
        .zip(asked.iter())
        .enumerate()
        .find(|(_, (count, was_asked))| **count > 0 && !**was_asked)
        .map(|(rank, _)| rank)
        .ok_or(TextGameError::NotFound)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ranks_parse() {
        assert_eq!(parse_rank("ace"), Ok(0));
        assert_eq!(parse_rank("King"), Ok(12));
        assert_eq!(parse_rank("7"), Ok(6));
        assert_eq!(parse_rank("10"), Ok(9));
        assert_eq!(parse_rank("1"), Err(TextGameError::InvalidArgument));
        assert_eq!(parse_rank("joker"), Err(TextGameError::InvalidArgument));
        assert_eq!(parse_rank(""), Err(TextGameError::InvalidArgument));
    }

    #[test]
    fn test_books_counted() {
        let mut hand = [0u32; RANKS];
        hand[0] = 4;
        hand[5] = 4;
        hand[7] = 2;
        assert_eq!(count_books(&hand).unwrap(), 2);
        let mut bad = [0u32; RANKS];
        bad[0] = 5;
        assert_eq!(count_books(&bad), Err(TextGameError::InvalidArgument));
        assert_eq!(count_books(&[0u32; 3]), Err(TextGameError::InvalidArgument));
    }

    #[test]
    fn test_ask_prefers_lowest_unasked() {
        let mut held = [0u32; RANKS];
        held[5] = 2;
        held[2] = 1;
        let mut asked = [false; RANKS];
        asked[2] = true;
        assert_eq!(choose_ask(&held, &asked).unwrap(), 5);
        asked[5] = true;
        assert_eq!(choose_ask(&held, &asked), Err(TextGameError::NotFound));
    }
}
