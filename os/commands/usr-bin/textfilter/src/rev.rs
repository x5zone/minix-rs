//! Line reversal for `rev`.
//!
//! Ground truth: `minix3/usr.bin/rev/rev.c` (NetBSD), main at lines 57-95:
//! each line's characters print in reverse order with a newline, files
//! (operands) fall back to stdin, and a read or open failure sets status
//! 1. The C reverses wide characters; this slice reverses Rust `char`s —
//! the same character-level guarantee, byte-safe through UTF-8.

use crate::GetoptReport;
use alloc::string::String;
use alloc::vec::Vec;

/// Reverses every line of the collected input (lines split on `\n`, the
/// final newline dropped), returning one reversed line per input line.
pub fn reversed_lines(input: &[u8]) -> Vec<String> {
    reversed_lines_of(split_lines(input))
}

/// Reverses already-split lines.
pub fn reversed_lines_of(lines: Vec<String>) -> Vec<String> {
    lines
        .into_iter()
        .map(|line| line.chars().rev().collect())
        .collect()
}

fn split_lines(input: &[u8]) -> Vec<String> {
    if input.is_empty() {
        return Vec::new();
    }
    let text = alloc::str::from_utf8(input).unwrap_or("");
    let mut pieces: Vec<String> = text.split('\n').map(String::from).collect();
    if input.ends_with(b"\n") {
        pieces.pop();
    }
    pieces
}

/// `rev` 的选项环，交回剩下的操作数（C 的 `argv += optind`，`rev.c:77-78`）。
///
/// C 的 optstring 是**空串**（`rev.c:70`），所以这一支就是共件
/// [`crate::empty_optstring_operands`] 描述的那三形：任何 `-` 开头的 token 都算集外字母
/// 并由 libc 先印一行、`--` 自身不算操作数、裸 `-` 算操作数。`rev` 拿到操作数后把它们
/// 当文件逐个 `fopen`（`rev.c:83-106`），这一段是 `rev` 自己的事，归壳。
pub fn parse_rev_args<'a>(argv: &'a [&'a str]) -> Result<Vec<&'a str>, GetoptReport> {
    crate::empty_optstring_operands(argv)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lines_reverse_character_order() {
        assert_eq!(reversed_lines(b"abc\ndef\n"), vec!["cba", "fed"]);
    }

    #[test]
    fn test_unterminated_final_line_still_reverses() {
        assert_eq!(reversed_lines(b"abc\ndef"), vec!["cba", "fed"]);
    }

    #[test]
    fn test_multibyte_characters_stay_intact() {
        // Character-level reversal: a two-byte character does not split.
        assert_eq!(reversed_lines("éab\n".as_bytes()), vec!["baé"]);
    }

    #[test]
    fn test_empty_input_yields_no_lines() {
        assert!(reversed_lines(b"").is_empty());
    }

    /// 选项环的几形逐个钉：任何字母都集外（取第一个字符）、`--` 自身不算操作数、
    /// 裸 `-` 是操作数、非选项 token 之后的内容不再扫描（不重排）。
    #[test]
    fn the_option_ring_takes_no_letters_and_stops_at_the_first_operand() {
        // 空 optstring ⇒ 任何字母都报，报的是 `-` 后的第一个字符。
        assert_eq!(
            parse_rev_args(&["-Z"]),
            Err(GetoptReport::UnknownOption('Z'))
        );
        assert_eq!(
            parse_rev_args(&["-bZ"]),
            Err(GetoptReport::UnknownOption('b')),
            "聚簇只报第一个字符，因为调用者拿到 '?' 就 usage() 退出"
        );
        assert_eq!(
            parse_rev_args(&["--x"]),
            Err(GetoptReport::UnknownOption('-'))
        );
        // `--` 结束选项环且自身被吃掉；裸 `-` 留着当操作数。
        assert_eq!(parse_rev_args(&["--", "f"]), Ok(vec!["f"]));
        assert_eq!(parse_rev_args(&["-"]), Ok(vec!["-"]));
        assert_eq!(parse_rev_args(&["--"]), Ok(vec![]));
        // 非选项 token 一到就停：它后面的 `-Z` 不再当选项看。
        assert_eq!(parse_rev_args(&["f", "-Z"]), Ok(vec!["f", "-Z"]));
        assert_eq!(parse_rev_args(&["f", "g"]), Ok(vec!["f", "g"]));
        let nothing: [&str; 0] = [];
        assert_eq!(parse_rev_args(&nothing), Ok(vec![]));
    }
}
