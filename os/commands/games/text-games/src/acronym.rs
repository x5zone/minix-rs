//! Acronym lookup.
//!
//! Ground truth: `minix3/games/wtf/wtf` (flags `-o` for the offensive file
//! and `-f` for extra database files). The word `is` is not this module's
//! business: the script drops one leading `is` among the operands (`:32-34`)
//! and queries everything else, including any later `is`. That is argument
//! handling, so it lives with the shell. Database lines follow
//! `term: expansion` shape. File reading stays with the execution layer;
//! this module owns line parsing and term lookup.

use crate::TextGameError;

/// Parse one database line (`term: expansion`, both sides non empty).
pub fn parse_acronym_line(line: &str) -> Result<(&str, &str), TextGameError> {
    // The field separator is **whitespace**, not a colon. In the real database
    // `minix3/share/misc/acronyms`, only 1 of 557 lines contains a colon (the
    // `$NetBSD:` identity line); entries look like `10Q\tthank you`. The script
    // side also splits on whitespace: `sed -ne "^$target[[:space:]]*||p"`
    // (`minix3/games/wtf/wtf:76-77`).
    // The old form here used `split_once(':')`, so **not a single line of the
    // real database could be parsed**, and `wtf` always reported "no such word"
    // for its own bundled data file (the criterion is recorded in
    // `rewrite-notes/18-stage-commands/24-text-games.md`). Splitting at the
    // first whitespace guarantees `term` contains no whitespace, so no extra
    // whitespace check is needed.
    let (term, expansion) = line
        .split_once(char::is_whitespace)
        .ok_or(TextGameError::InvalidArgument)?;
    let term = term.trim();
    let expansion = expansion.trim();
    if term.is_empty() || expansion.is_empty() {
        return Err(TextGameError::InvalidArgument);
    }
    Ok((term, expansion))
}

/// Every expansion stored under `term`, in the order the lines were given.
///
/// C's lookup is a pipeline rather than a single table query: `:75` upper-cases
/// the operand as a whole (`tr '[a-z]' '[A-Z]'`), `:76`'s
/// `fgrep -h "$target" $acronyms` searches **all database files as one stream**
/// for every line containing that string, and `:77`'s
/// `sed -ne "\|^$target[[:space:]]*||"` keeps only lines whose start is exactly
/// `$target` followed by whitespace, stripping that prefix. So one word can hit
/// **several lines** (a duplicate entry in one file, or the word in several
/// files); all of them come back, in list order and line order within a file.
///
/// The case rule is **one-way**: the query side is flattened by upper-casing,
/// the entry side is not, so database entries that begin with a lowercase letter
/// can never be found — measured (database lines `abc<TAB>lower term entry`):
/// querying `abc` and querying `ABC` both find nothing, while `DEF<TAB>upper`
/// is found by `def`. "Equal to `term` case-insensitively" combined with "`name`
/// itself is already all uppercase" is equivalent to "upper-case the query word
/// first, then compare character by character", and needs no temporary
/// `String`.
#[must_use]
/// The glob branch of the default database list (C `wtf:43-55`): with
/// neither `-f` nor `ACRONYMDB`, the shell expands
/// `/usr/share/misc/acronyms*` over `/usr/share/misc` — names starting with
/// the pattern stem, byte-sorted (the script runs in the C locale); a name
/// ending `-o` joins only when `-o` selected the offensive batch; and an
/// unmatched glob passes through as the **literal pattern** (the shell's
/// own behavior), so the caller's database pre-pass then fails naming the
/// star. A match list that the `-o` filter empties does **not** fall back:
/// the script's next check answers `acronym database not found!` for it
/// (`wtf:56-58`).
pub fn default_database_list(
    entries: &[&str],
    offensive: bool,
) -> alloc::vec::Vec<alloc::string::String> {
    const STEM: &str = "acronyms";
    let mut names: alloc::vec::Vec<&str> = entries
        .iter()
        .copied()
        .filter(|name| name.starts_with(STEM))
        .collect();
    names.sort_unstable();
    if names.is_empty() {
        return alloc::vec![alloc::string::String::from("/usr/share/misc/acronyms*")];
    }
    let mut list = alloc::vec::Vec::new();
    for name in names {
        if name.ends_with("-o") && !offensive {
            continue;
        }
        list.push(alloc::format!("/usr/share/misc/{name}"));
    }
    list
}

pub fn matching_expansions<'a>(lines: &[&'a str], term: &str) -> alloc::vec::Vec<&'a str> {
    if term.is_empty() {
        return alloc::vec::Vec::new();
    }
    lines
        .iter()
        .filter_map(|line| parse_acronym_line(line).ok())
        .filter(|(name, _)| {
            name.eq_ignore_ascii_case(term) && !name.bytes().any(|byte| byte.is_ascii_lowercase())
        })
        .map(|(_, expansion)| expansion)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lines_parse() {
        // The real database's shape is "word + tab + expansion"
        // (`minix3/share/misc/acronyms`).
        assert_eq!(
            parse_acronym_line("10Q\tthank you"),
            Ok(("10Q", "thank you"))
        );
        assert_eq!(
            parse_acronym_line("3WHS\tthree-way handshake"),
            Ok(("3WHS", "three-way handshake"))
        );
        assert_eq!(
            parse_acronym_line("NOEXP\t"),
            Err(TextGameError::InvalidArgument)
        );
        assert_eq!(
            parse_acronym_line("single-field"),
            Err(TextGameError::InvalidArgument)
        );
        // A colon is not a separator: the script splits on whitespace, so the
        // term in `LOL: x` is `LOL:`, not `LOL`.
        assert_eq!(
            parse_acronym_line("LOL: laughing"),
            Ok(("LOL:", "laughing"))
        );
    }

    /// This one reads the real database directly: it is `wtf`'s default file,
    /// so being unable to parse it means the command is broken. Both earlier
    /// defects (the host could not open the file, and term splitting on a
    /// colon) are caught by it on the spot.
    #[test]
    fn the_shipped_acronym_database_is_readable_and_resolves() {
        let db = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../../minix3/share/misc/acronyms"
        );
        let text = std::fs::read_to_string(db).expect("the shipped acronyms database");
        let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
        assert!(lines.len() > 400, "真库应有数百条目，实测 {}", lines.len());
        assert_eq!(matching_expansions(&lines, "10Q"), ["thank you"]);
        assert_eq!(matching_expansions(&lines, "10q"), ["thank you"]);
        assert!(matching_expansions(&lines, "definitely-not-an-acronym").is_empty());
    }

    /// The glob branch (`wtf:43-55`): stem match, byte sort, the `-o`
    /// batch gate, and the unmatched-glob literal.
    #[test]
    fn the_default_glob_selects_sorts_and_gates_the_batch() {
        let entries = [
            "README",
            "acronyms",
            "acronyms-o",
            "acronyms.2",
            ".acronyms-x",
        ];
        // Without `-o`: the plain files in byte order; the `-o` batch and
        // the non-matching names stay out.
        assert_eq!(
            default_database_list(&entries, false),
            ["/usr/share/misc/acronyms", "/usr/share/misc/acronyms.2"]
        );
        // With `-o`: the offensive batch joins at its sorted position.
        assert_eq!(
            default_database_list(&entries, true),
            [
                "/usr/share/misc/acronyms",
                "/usr/share/misc/acronyms-o",
                "/usr/share/misc/acronyms.2"
            ]
        );
        // No match at all: the literal pattern passes through (the shell's
        // own behavior), so the pre-pass fails naming the star.
        assert_eq!(
            default_database_list(&["other"], false),
            ["/usr/share/misc/acronyms*"]
        );
        // A match list that `-o` empties does not fall back: the script's
        // next check answers `acronym database not found!` (`wtf:56-58`),
        // which the caller owes.
        assert!(default_database_list(&["acronyms-o"], false).is_empty());
    }

    #[test]
    fn test_lookup_finds_first() {
        let lines = ["LOL\tlaughing out loud", "BRB\tbe right back"];
        assert_eq!(matching_expansions(&lines, "lol"), ["laughing out loud"]);
        assert!(matching_expansions(&lines, "zzz").is_empty());
        assert!(matching_expansions(&lines, "").is_empty());
    }

    /// When one word hits several lines, **all** of them come back, in list
    /// order and line order within a file: the script's `:76`
    /// `fgrep -h $acronyms` searches all files as one stream, and `:77`'s `sed`
    /// keeps them line by line. Measured (two `LOL` lines in one file) →
    /// `LOL: first match` newline `second match`; (one `LOL` in each of two
    /// files, `-f w1 -f w2`, where **prepending** makes w2 searched first) →
    /// `LOL: second file` newline `first file`.
    #[test]
    fn every_matching_line_comes_back_in_list_order() {
        let dup = ["LOL\tfirst match", "LOL\tsecond match"];
        assert_eq!(
            matching_expansions(&dup, "lol"),
            ["first match", "second match"]
        );
        let two_files = ["LOL\tsecond file", "LOL\tfirst file"];
        assert_eq!(
            matching_expansions(&two_files, "LOL"),
            ["second file", "first file"]
        );
    }

    /// Case is flattened only on the **query** side: C upper-cases the operand
    /// first, then compares character by character against the start of each
    /// database line. Measured (`ACRONYMDB=<db> sh minix3/games/wtf/wtf <form>`,
    /// with database lines `abc<TAB>lower term entry` and `DEF<TAB>upper`):
    /// `abc` finds nothing, `ABC` finds nothing, `def` hits `upper`. Database
    /// entries that start with a lowercase letter are therefore unreachable.
    #[test]
    fn a_database_term_written_in_lowercase_is_unreachable() {
        let lines = ["abc\tlower term entry", "DEF\tupper"];
        assert!(matching_expansions(&lines, "abc").is_empty());
        assert!(matching_expansions(&lines, "ABC").is_empty());
        assert_eq!(matching_expansions(&lines, "def"), ["upper"]);
        assert_eq!(matching_expansions(&lines, "DEF"), ["upper"]);
    }
}
