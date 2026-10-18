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
    // 字段分隔是**空白**，不是冒号。真库 `minix3/share/misc/acronyms` 的 557 行里只有
    // 1 行含冒号（那是 `$NetBSD:` 标识行），条目形如 `10Q\tthank you`；脚本那侧的取法也是
    // 按空白断词：`sed -ne "^$target[[:space:]]*||p"`（`minix3/games/wtf/wtf:76-77`）。
    // 改前这里按 `split_once(':')`，于是**真库没有一行解得开**，`wtf` 对自带的数据档
    // 永远报「查无此词」（判据见 18 篇批次三百七十六）。按第一个空白断开即保证
    // `term` 不含空白，不再需要额外的空白检查。
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
        // 真库的形是「词 + 制表符 + 释义」（`minix3/share/misc/acronyms`）。
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
        // 冒号不是分界符：脚本按空白断词，所以 `LOL: x` 的词是 `LOL:` 而不是 `LOL`。
        assert_eq!(
            parse_acronym_line("LOL: laughing"),
            Ok(("LOL:", "laughing"))
        );
    }

    /// 这一枚直接对真库读数：`wtf` 的默认档就是它，解不开就等于命令失效。
    /// 改前两处缺陷（宿主开不了档、按冒号断词）都能被它当场抓住。
    #[test]
    fn the_shipped_acronym_database_is_readable_and_resolves() {
        let db = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../../minix3/share/misc/acronyms"
        );
        let text = std::fs::read_to_string(db).expect("the shipped acronyms database");
        let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
        assert!(lines.len() > 400, "真库应有数百条目，实测 {}", lines.len());
        assert_eq!(
            lookup_acronym(&lines, "10Q").ok(),
            Some("thank you"),
            "`10Q' 该在库里解得出"
        );
        assert_eq!(lookup_acronym(&lines, "10q").ok(), Some("thank you"));
        assert_eq!(
            lookup_acronym(&lines, "definitely-not-an-acronym").err(),
            Some(TextGameError::NotFound)
        );
    }

    #[test]
    fn test_lookup_finds_first() {
        let lines = ["LOL\tlaughing out loud", "BRB\tbe right back"];
        assert_eq!(lookup_acronym(&lines, "lol").unwrap(), "laughing out loud");
        assert_eq!(lookup_acronym(&lines, "zzz"), Err(TextGameError::NotFound));
        assert_eq!(
            lookup_acronym(&lines, ""),
            Err(TextGameError::InvalidArgument)
        );
    }
}
