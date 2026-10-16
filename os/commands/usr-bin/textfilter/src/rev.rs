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
/// C 的 optstring 是**空串**（`rev.c:70`），于是这一支有两个别的命令没有的性质：
/// ① `strchr("", <字母>)` 永远是 `NULL`，所以**任何** `-` 后面的第一个字符都是集外字母，
/// 由 libc 先印一行（`minix3/lib/libc/stdlib/getopt.c:101-104`）再把 `'?'` 交给
/// `rev.c:72-74` 的 `case '?': default: usage()`；② optstring 里也没有 `-`，所以裸 `-`
/// 在 `getopt.c:87-92` 走的是「结束选项环但**不推进** `optind`」那一支，这个 token
/// 自己落进操作数里（参照件现量：`rev: -: No such file or directory`）。
///
/// 这一支不判「带值选项缺值」：空 optstring 里没有任何选项要值，`getopt.c:112-141`
/// 那整段到不了，所以本函数只可能交回 [`GetoptReport::UnknownOption`]。
///
/// 只看**第一个** token：C 那个 `while` 圈定不了第二次（`rev.c:70-75` 里 getopt 要么返 -1
/// 结束选项环，要么返 `'?'` 而立刻 `usage()` 退出），所以不写循环而写单次判定；写循环
/// 反而会把「报完第一个字母就退」说成「扫完整串」。
pub fn parse_rev_args<'a>(argv: &'a [&'a str]) -> Result<Vec<&'a str>, GetoptReport> {
    let Some(first) = argv.first() else {
        return Ok(Vec::new());
    };
    // 不以 `-` 开头的 token 让 getopt 当场返回 -1（`getopt.c:74-78`），**不重排**：
    // 它和它后面的东西整个交回当操作数（量过：`rev <file> -Z` 里 `-Z` 是文件名）。
    let Some(flag) = first.strip_prefix('-') else {
        return Ok(argv.to_vec());
    };
    // `--`：`getopt.c:81-85` 先 `++optind` 再结束，所以它自身不是操作数。
    if flag == "-" {
        return Ok(argv[1..].to_vec());
    }
    // 裸 `-`（`flag` 为空）：结束选项环而不推进 `optind`，于是它是操作数。
    let Some(letter) = flag.chars().next() else {
        return Ok(argv.to_vec());
    };
    // 集外字母：`getopt.c:95-104`。递出去的是 `-` 后面的**第一个字符**，聚簇与
    // `--x` 都在报第一个字符后由 `usage()` 退出（量过：`-bZ` 只报 `b`、`--x` 报 `-`）。
    Err(GetoptReport::UnknownOption(letter))
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
