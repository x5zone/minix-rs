//! End-to-end face for the `nl` binary: real process, real pipes, checked against
//! `minix3/usr.bin/nl/nl.c` (the option loop `:149-233`, the operand guard
//! `:239-249`, `parse_numbering` `:340-373` and `usage()` `:400-405`) plus the
//! libc `getopt` it drives (`minix3/lib/libc/stdlib/getopt.c:101-104`, `:132-136`).
//!
//! 两条出口形状要分清，这是本枚门的全部要点：**getopt 自己**报的错（集外字母、带值选项
//! 缺值）是「libc 那一行 + 命令的 usage 两行」；**命令自己**的 `errx` 只有一行且不接
//! usage。旧写法把两者都压成一行自创话，也没有 getopt 的逐字母扫描（`-ba`、`-pb t` 都会
//! 掉进集外字母那支）。

mod face_support;

use face_support::write_stdin;
use std::process::{Command, Stdio};

const NL: &str = env!("CARGO_BIN_EXE_nl");

/// C `usage()` 的两行原文：第一行末尾是 `\n\t`，第二行以制表符缩进。
const USAGE: &str = concat!(
    "Usage: nl [-p] [-b type] [-d delim] [-f type] [-h type] [-i incr] [-l num]\n",
    "\t[-n format] [-s sep] [-v startnum] [-w width] [file]\n"
);

fn run(args: &[&str], input: &[u8]) -> (String, i32, String) {
    let mut child = Command::new(NL)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the nl binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// 与 `run` 同一条命令缝，但标准错误**逐字节**交回：`from_utf8_lossy` 把一个裸 `c3`
/// 渲染成三字节 U+FFFD，文本形分不清「一个字节」与「两个字节」这两种错法。
fn run_bytes(args: &[&str], input: &[u8]) -> (Vec<u8>, i32, Vec<u8>) {
    let mut child = Command::new(NL)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the nl binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        output.stdout,
        output.status.code().unwrap_or(-1),
        output.stderr,
    )
}

/// 集外字母那一行的字母位是**一个裸字节**。期望逐形取自本轮新配的参照件
/// `tmp/nl-417/nl_ref`（`minix3/usr.bin/nl/nl.c` + Minix 的 `getopt.c` + 垫片，经同名软链
/// `nl` 起 ⇒ 程序名段与本壳一致；本族此前没有 Minix 血脉的 nl 参照件，批次三百零六那批
/// 读数是直接回 C 源推的）：`-é` 与 `-pé` 都是 `nl: unknown option -- ` + 单个 `c3` +
/// 换行再跟那**两行** usage、退 1；`-Zé` 停在 `Z`；反面的 `-dé` 里 `é` 是**值**（选项串
/// `"pb:d:f:h:i:l:n:s:v:w:"` 里只有 `p` 不带值），当正则用，退 0 且一句不报。
/// 这一支的环整个住在薄壳里（`src/bin/nl.rs:136` 的 `bytes[pos] as char`），没有可单测的
/// 库函数 ⇒ 取法半与渲染半都由这一枚面门承载（与批次四百零八的 `uniq` 同形，那轮的
/// 结论是「环不在库里就不硬造库门」）。
#[test]
fn the_libc_line_of_a_multibyte_flag_is_one_byte_for_nl() {
    let want: Vec<u8> = [
        b"nl: unknown option -- ".as_slice(),
        b"\xc3\n",
        USAGE.as_bytes(),
    ]
    .concat();
    for form in ["-\u{e9}", "-p\u{e9}"] {
        let (out, status, err) = run_bytes(&[form], b"a\n");
        assert_eq!(out, Vec::new(), "{form} 的报文只走标准错误");
        assert_eq!(status, 1, "{form} 的 stderr 是 {err:?}");
        assert_eq!(err, want, "{form} 的字母该是一个裸字节 c3");
    }

    let (_out, status, err) = run_bytes(&["-Z\u{e9}"], b"a\n");
    assert_eq!(status, 1);
    assert_eq!(
        err,
        [b"nl: unknown option -- Z\n".as_slice(), USAGE.as_bytes(),].concat(),
        "环停在第一个坏字母上，第二个字节根本不该被读到"
    );

    let (out, status, err) = run_bytes(&["-d\u{e9}"], b"a\n");
    assert_eq!(status, 0, "带值字母把两字节整个当值收走，退码是成功的");
    assert_eq!(err, Vec::new(), "那一形不报案：{err:?}");
    assert_eq!(
        out,
        b"     1\ta\n".to_vec(),
        "分隔符正则用不上就不改行（参照件同形）"
    );
}

/// 集外字母：libc 先印 `nl: unknown option -- <字母>`（`getopt.c:101-104`），随后
/// `case '?': default: usage()`（`nl.c:230-233`）——三行，退 1。
#[test]
fn an_unknown_letter_prints_the_libc_line_then_the_usage() {
    let (out, status, err) = run(&["-Z"], b"a\n");
    assert_eq!(out, "", "报文只走 stderr");
    assert_eq!(status, 1, "stderr 是 {err:?}");
    assert_eq!(err, format!("nl: unknown option -- Z\n{USAGE}"));
}

/// 带值选项缺值：libc 的另一句（`getopt.c:132-136`），同样接 usage。旧写法把这一支当成
/// 「值非法」，用的是被拒值自己的句子，等于把两支护成了同一支。
#[test]
fn a_value_taking_letter_without_its_value_prints_the_libc_line() {
    for (letter, args) in [('b', &["-b"][..]), ('w', &["-w"][..]), ('n', &["-n"][..])] {
        let (out, status, err) = run(args, b"a\n");
        assert_eq!(out, "");
        assert_eq!(status, 1, "{args:?} 的 stderr 是 {err:?}");
        assert_eq!(
            err,
            format!("nl: option requires an argument -- {letter}\n{USAGE}"),
            "{args:?}"
        );
    }
}

/// `parse_numbering` 的句子带**段名**（`numbering_properties[]` 的 name 依次是
/// footer / body / header，`nl.c:73-75`），且把整个被拒的值打出来（`:366-369`）。
/// 只有一行，不接 usage。
#[test]
fn numbering_type_errors_name_their_own_section() {
    for (flag, name) in [('b', "body"), ('f', "footer"), ('h', "header")] {
        let (out, status, err) = run(&[&format!("-{flag}"), "q"], b"a\n");
        assert_eq!(out, "");
        assert_eq!(status, 1);
        assert_eq!(
            err,
            format!("nl: illegal {name} line numbering type -- q\n"),
            "-{flag} 的句子该带自己的段名"
        );
    }
}

/// getopt 的逐字母扫描：值可以附在本 token（`-ba`），字母可以聚簇（`-pb t`）。
/// 旧写法按整 token 匹配，这两形都会掉进集外字母那支。
#[test]
fn inline_and_clustered_letters_parse_like_getopt() {
    // 本枚门只管诊断面：编号输出的形状由 `src/nl.rs` 的库测负责，这里不拿二进制输出当金样。
    let (out, status, err) = run(&["-ba", "-w", "4"], b"x\n");
    assert_eq!((status, err.as_str()), (0, ""), "附值形该被接受");
    assert!(!out.is_empty(), "被接受就该有编号输出");
    let (out, status, err) = run(&["-pb", "t"], b"x\n");
    assert_eq!((status, err.as_str()), (0, ""), "聚簇形该被接受");
    assert!(!out.is_empty(), "被接受就该有编号输出");
}

/// `-w` 的三支各不同句：转不出数字是 `invalid width value -- <值>`，转出来但 ≤ 0 是
/// `width argument must be > 0 -- <值>`（`nl.c:220-228`，值也被打出来）。
#[test]
fn width_rejects_unparseable_and_nonpositive_with_different_sentences() {
    let (_, status, err) = run(&["-w", "x"], b"a\n");
    assert_eq!(
        (status, err.as_str()),
        (1, "nl: invalid width value -- x\n")
    );
    let (_, status, err) = run(&["-w", "0"], b"a\n");
    assert_eq!(
        (status, err.as_str()),
        (1, "nl: width argument must be > 0 -- 0\n")
    );
}

/// `-d` 收下前两个字符，第三个才拒（`nl.c:157-169`）；空值不动默认（两个
/// `if (optarg[n] != '\0')` 都不成立），所以 `-d ''` 是合法无操作。
#[test]
fn delim_takes_two_characters_and_refuses_a_third() {
    let (_, status, err) = run(&["-d", "abc"], b"a\n");
    assert_eq!(
        (status, err.as_str()),
        (1, "nl: invalid delim argument -- abc\n")
    );
    let (out, status, err) = run(&["-d", ""], b"x\n");
    assert_eq!((status, err.as_str()), (0, ""), "空值该是无操作");
    assert!(!out.is_empty(), "被接受就该有编号输出");
}

/// 其余数值支各报自己的句子，且都把被拒的值原样打出去（`nl.c:176-228`）。
#[test]
fn the_number_arguments_quote_the_rejected_value() {
    for (args, sentence) in [
        (vec!["-i", "2x"], "invalid incr argument -- 2x"),
        (vec!["-l", "2x"], "invalid num argument -- 2x"),
        (vec!["-v", "1x"], "invalid startnum value -- 1x"),
        (vec!["-n", "xy"], "illegal format -- xy"),
    ] {
        let (_, status, err) = run(&args, b"a\n");
        assert_eq!(status, 1, "{args:?} 的 stderr 是 {err:?}");
        assert_eq!(err, format!("nl: {sentence}\n"), "{args:?}");
    }
}

/// C 这里**没有**非零检查（只查 `*ep`），所以 `-i 0` 合法、退 0；旧写法自创过一句
/// 「increment must be non-zero」，那是把不存在的守卫当真。
#[test]
fn a_zero_increment_is_accepted() {
    let (_, status, err) = run(&["-i", "0"], b"a\n");
    assert_eq!((status, err.as_str()), (0, ""), "stderr 是 {err:?}");
}

/// 两个以上操作数是 `usage()` 那一支（`nl.c:246-248`），**不带** libc 那一行——它与
/// 集外字母的区别正是这枚门要钉的：同样是 usage，行数不同。
#[test]
fn more_than_one_operand_is_plain_usage_without_the_libc_line() {
    let (_, status, err) = run(&["f1", "f2"], b"a\n");
    assert_eq!(status, 1);
    assert_eq!(err, USAGE);
}

/// 读不到的操作数走 `err(3)`：程序名、**带反引号的操作数**、`strerror` 的句子三件齐备
/// （`nl.c:244-246` 的 `err(EXIT_FAILURE, "Cannot open `%s'", argv[0])`），退 1。
#[test]
fn an_unreadable_operand_names_the_operand_and_the_number_sentence() {
    let (out, status, err) = run(&["/tmp/nl-face-definitely-absent"], b"a\n");
    assert_eq!(out, "");
    assert_eq!(status, 1, "stderr 是 {err:?}");
    assert!(
        err.starts_with("nl: Cannot open `/tmp/nl-face-definitely-absent': "),
        "该是 err(3) 的三段形，实际 {err:?}"
    );
    assert!(
        !err.contains("cannot open input"),
        "自创短语顶替了 strerror 句子：{err:?}"
    );
}
