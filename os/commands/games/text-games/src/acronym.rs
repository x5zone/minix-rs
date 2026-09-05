//! Acronym lookup.
//!
//! Ground truth: `minix3/games/wtf/wtf` (flags `-o` for the offensive file
//! and `-f` for extra database files, the word `is` skipped between the
//! command name and the terms). Database lines follow `term: expansion`
//! shape. File reading stays with the execution layer; this module owns line
//! parsing and term lookup.

use crate::TextGameError;

/// Words skipped between the command name and the terms.
pub const SKIPPED_WORDS: &[&str] = &["is"];

/// True when `word` is skipped (case insensitive).
pub fn is_skipped_word(word: &str) -> bool {
    SKIPPED_WORDS
        .iter()
        .any(|known| known.eq_ignore_ascii_case(word))
}

/// Parse one database line (`term: expansion`, both sides non empty).
pub fn parse_acronym_line(line: &str) -> Result<(&str, &str), TextGameError> {
    let (term, expansion) = line.split_once(':').ok_or(TextGameError::InvalidArgument)?;
    let term = term.trim();
    let expansion = expansion.trim();
    if term.is_empty() || expansion.is_empty() {
        return Err(TextGameError::InvalidArgument);
    }
    if term.contains(char::is_whitespace) {
        return Err(TextGameError::InvalidArgument);
    }
    Ok((term, expansion))
}

/// Look up `term` in `lines` (case insensitive, first match wins).
pub fn lookup_acronym<'a>(lines: &[&'a str], term: &str) -> Result<&'a str, TextGameError> {
    if term.is_empty() {
        return Err(TextGameError::InvalidArgument);
    }
    for line in lines {
        if let Ok((name, expansion)) = parse_acronym_line(line)
            && name.eq_ignore_ascii_case(term)
        {
            return Ok(expansion);
        }
    }
    Err(TextGameError::NotFound)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lines_parse() {
        assert_eq!(
            parse_acronym_line("LOL: laughing out loud"),
            Ok(("LOL", "laughing out loud"))
        );
        assert_eq!(parse_acronym_line("NOEXP:"), Err(TextGameError::InvalidArgument));
        assert_eq!(parse_acronym_line("no colon"), Err(TextGameError::InvalidArgument));
        assert_eq!(
            parse_acronym_line("two words: bad"),
            Err(TextGameError::InvalidArgument)
        );
    }

    #[test]
    fn test_skipped_words() {
        assert!(is_skipped_word("is"));
        assert!(is_skipped_word("IS"));
        assert!(!is_skipped_word("lol"));
    }

    #[test]
    fn test_lookup_finds_first() {
        let lines = ["LOL: laughing out loud", "BRB: be right back"];
        assert_eq!(lookup_acronym(&lines, "lol").unwrap(), "laughing out loud");
        assert_eq!(lookup_acronym(&lines, "zzz"), Err(TextGameError::NotFound));
        assert_eq!(lookup_acronym(&lines, ""), Err(TextGameError::InvalidArgument));
    }
}
