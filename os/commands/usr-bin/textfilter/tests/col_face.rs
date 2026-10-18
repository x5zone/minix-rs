//! End-to-end face for the `col` binary, checked against
//! `minix3/usr.bin/col/col.c`（选项环 `:133-158`、`-l` 的 `atoi` 判定 `:145-150`、
//! 操作数一票否决 `:157-158`、`usage()` `:520-526`）与它驱动的 libc `getopt`
//!（`minix3/lib/libc/stdlib/getopt.c:74-92`、`:95-104`、`:132-136`）。
//!
//! `col.c:133` 的 optstring 是 `"bfhl:px"`：五个无值字母加一个带值的 `l`，而且
//! **一个操作数都不接受**（`optind != argc` 直接 usage）。参照件
//! `tmp/col-313/col_ref`（`col.c` 真源 + Minix 的 `getopt.c` + 程序名垫片，宿主现链）
//! 逐形量出的形状是：
//! ① 集外字母 = libc 一行 + usage 一行，退 1（`-Z`、聚簇 `-Zb` 报 `Z`、`--x` 报 `-`）；
//! ② `-l` 缺值 = libc 的 `option requires an argument -- l` + usage 两行（旧写法错走了
//!    `bad -l argument` 那一支，句子完全不同）；
//! ③ `atoi(optarg) <= 0` 是 `col: bad -l argument <原文>.` **单独一行**、不接 usage，
//!    程序名写死 `col`、句子带句点；`atoi` 不管尾巴上的杂字符，所以 `-l 3x` 是合法的 3；
//! ④ `--` 结束选项环而自身不是操作数；裸 `-`、任何文件参数都走 usage；
//! ⑤ 聚簇逐字母生效（`-bfpx`、`-xb`、`-hx` 都合法，后写的覆盖先写的）。
//!
//! 这扇门前不存在：`col` 的读缝直接拿裸 `minix_sys::read`，宿主上读不到任何输入
//!（同族第七例），所以那台行装配状态机的输出从来没有被进程级测过。

mod face_support;

use face_support::write_stdin;
use std::process::{Command, Stdio};

const COL: &str = env!("CARGO_BIN_EXE_col");
const USAGE: &str = "usage: col [-bfpx] [-l nline]\n";

fn run(args: &[&str], input: &[u8]) -> (String, i32, String) {
    let mut child = Command::new(COL)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the col binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// 同上但交回**原始字节**：字母位要钉的是「一个字节」，`String::from_utf8_lossy`
/// 把 `c3` 与 `c3 83` 都渲染成替换字符，文本夹具区分不了这两种错法。
fn run_bytes(args: &[&str], input: &[u8]) -> (Vec<u8>, i32, Vec<u8>) {
    let mut child = Command::new(COL)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the col binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        output.stdout,
        output.status.code().unwrap_or(-1),
        output.stderr,
    )
}

/// 集外字母：libc 那一行在前、`usage()` 在后，退 1（`col.c:154-156`）。旧写法少了 libc 那一行。
#[test]
fn an_unknown_letter_prints_the_libc_line_then_the_usage() {
    let (out, status, err) = run(&["-Z"], b"plain\n");
    assert_eq!(out, "");
    assert_eq!(status, 1, "stderr 是 {err:?}");
    assert_eq!(err, format!("col: unknown option -- Z\n{USAGE}"));
}

/// 聚簇只报第一个字符（参照件：`-Zb` → `unknown option -- Z`）。
#[test]
fn a_cluster_reports_only_its_first_letter() {
    let (out, status, err) = run(&["-Zb"], b"plain\n");
    assert_eq!((out.as_str(), status), ("", 1));
    assert_eq!(err, format!("col: unknown option -- Z\n{USAGE}"));
}

/// `--x` 报的字母是第二个连字符：optstring 里没有 `-`（参照件：`unknown option -- -`）。
#[test]
fn a_dash_followed_by_a_word_is_an_unknown_option_named_by_the_dash() {
    let (_, status, err) = run(&["--x"], b"plain\n");
    assert_eq!(status, 1);
    assert_eq!(err, format!("col: unknown option -- -\n{USAGE}"));
}

/// `-l` 后面没有那一段是 libc 的缺值句 + usage，**不是** `bad -l argument`：
/// 两支句子不同、行数也不同（旧写法把这一支并进了 `bad -l`，少了 usage 那一行）。
#[test]
fn a_missing_line_count_prints_the_libc_line_not_the_bad_argument() {
    let (out, status, err) = run(&["-l"], b"plain\n");
    assert_eq!((out.as_str(), status), ("", 1));
    assert_eq!(
        err,
        format!("col: option requires an argument -- l\n{USAGE}"),
        "缺值那支是两句、程序名跟着 `getprogname()`"
    );
    // 聚簇里的 `-l` 落在串尾同样走这一支（`-bl`）。
    let (_, status, err) = run(&["-bl"], b"plain\n");
    assert_eq!(status, 1);
    assert_eq!(
        err,
        format!("col: option requires an argument -- l\n{USAGE}")
    );
}

/// `atoi` 判非正那一支只有一行：程序名写死 `col`、带操作数原文、句尾一个句点，
/// 而且**不接 usage**（`col.c:145-150` 是裸 `fprintf` 加 `exit`）。
#[test]
fn a_rejected_line_count_quotes_the_operand_and_skips_the_usage() {
    for (word, reported) in [("0", "0"), ("-2", "-2"), ("abc", "abc"), ("", "")] {
        let args = if word.is_empty() {
            vec!["-l".to_string(), "".to_string()]
        } else {
            vec!["-l".to_string(), word.to_string()]
        };
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let (out, status, err) = run(&refs, b"plain\n");
        assert_eq!((out.as_str(), status), ("", 1), "`-l {word}` 该被拒");
        assert_eq!(
            err,
            format!("col: bad -l argument {reported}.\n"),
            "句子该带操作数原文与句点，且不接 usage"
        );
    }
}

/// `atoi` 不看尾巴上的杂字符，所以 `-l 3x` 是合法的 3；值也可以附在本串里（`-l3`）。
/// 旧写法用 Rust 的 `parse::<i32>()`，把 `3x` 判成非法、也不认附值形。
#[test]
fn the_line_count_accepts_attached_and_junk_tailed_forms() {
    for args in [&["-l3"][..], &["-l", "3"][..], &["-l", "3x"][..]] {
        let (out, status, err) = run(args, b"plain\n");
        assert_eq!(
            (out.as_str(), err.as_str(), status),
            ("plain\n", "", 0),
            "args 是 {args:?}"
        );
    }
}

/// 五个无值字母逐字母生效，后写的覆盖先写的：`-bfpx` 里那个 `x` 关掉空格压缩，
/// `-b` 让退格与叠字符只留最后一个。
#[test]
fn valueless_letters_apply_letter_by_letter_within_a_cluster() {
    // 缺省：退格原样透出（参照件 `a^Hb`）。
    let (out, status, err) = run(&[], b"a\x08b\nline2\n");
    assert_eq!(
        (out.as_str(), err.as_str(), status),
        ("a\x08b\nline2\n", "", 0)
    );
    // `-b`：不出退格，叠字只留最后一个。
    let (out, status, _) = run(&["-b"], b"a\x08b\nline2\n");
    assert_eq!((out.as_str(), status), ("b\nline2\n", 0));
    // 缺省把行首空格压成制表符，`-x` 改成摊平的空格。
    let (out, _, _) = run(&[], b"x\ty\n");
    assert_eq!(out, "x\ty\n");
    let (out, status, _) = run(&["-x"], b"x\ty\n");
    assert_eq!(
        (out.as_str(), status),
        ("x       y\n", 0),
        "`-x` 该摊成空格"
    );
    // 聚簇：`-xb` 两支都算。
    let (out, status, _) = run(&["-xb"], b"a\x08b\nx\ty\n");
    assert_eq!((out.as_str(), status), ("b\nx       y\n", 0));
}

/// 操作数一票否决：`col` 只读标准输入，裸 `-`、文件参数、`--` 之后还有东西都是 usage 一行；
/// 而纯 `--` 合法（它自身不算操作数）。
#[test]
fn any_operand_is_rejected_but_the_marker_alone_is_not() {
    let (out, status, err) = run(&["--"], b"plain\n");
    assert_eq!((out.as_str(), err.as_str(), status), ("plain\n", "", 0));

    for args in [&["-"][..], &["somefile"][..], &["--", "-b"][..]] {
        let (out, status, err) = run(args, b"plain\n");
        assert_eq!((out.as_str(), status), ("", 1), "args 是 {args:?}");
        assert_eq!(err, USAGE, "`{args:?}` 只该印 usage 一行");
    }
}

/// 输出腿（本闸门存在的另一半）：以前 `col` 用裸 `minix_sys::read`，宿主上读不到输入，
/// 那台行装配状态机从来没有被进程级测过。金样逐形取自参照件读数（`golden.log`）。
#[test]
fn the_reverse_feed_filter_reaches_the_output_end_to_end() {
    // 反向换行（回车 + 退格回拉）被吸收，两行都留下。
    let (out, status, err) = run(&[], b"l1\n\x08\x08l0\n");
    assert_eq!((out.as_str(), err.as_str(), status), ("l1\nl0\n", "", 0));
    // 空行原样出。
    let (out, status, _) = run(&[], b"\n\n\n");
    assert_eq!((out.as_str(), status), ("\n\n\n", 0));
    // 未知转义序列被吃掉（参照件：`a\033Zb` 出 `ab`）。
    let (out, status, _) = run(&[], b"a\x1bZb\n");
    assert_eq!((out.as_str(), status), ("ab\n", 0));
    let (out, status, _) = run(&["-p"], b"a\x1bZb\n");
    assert_eq!((out.as_str(), status), ("ab\n", 0));
    // 末行没有换行时这一行不出（C 攒到换行才flush；参照件同形）。
    let (out, status, err) = run(&[], b"abc");
    assert_eq!((out.as_str(), err.as_str(), status), ("", "", 0));
    // 空输入产出空。
    let (out, status, _) = run(&[], b"");
    assert_eq!((out.as_str(), status), ("", 0));
}

/// 参照件 `tmp/col-313/col_ref` 现量：`col -bé` 打的是
/// `col: unknown option -- ` + **裸字节 `0xC3`** + 换行，退 1，再跟整块 usage。
/// 本层曾按字符切，于是打出了完整的 `é`（`c3 a9`）——句子看着对，字节不对。
/// 门里比的是 `char::from(0xC3)` 那个码位 U+00C3（打成 `c3 83`）；
/// 按字符取时会得到整个 `é`（U+00E9，打成 `c3 a9`）。**末段仍是本族共有的一处不同**：C 打的是裸字节
/// `c3` 一个，本树走 `char` 的 `Display` 就成了两字节——那一条登记在
/// `*-NONASCII-OPTLETTER` 一族里等共享缝那轮一起收，本枚门只钉「取的是第一个字节」
/// 这一判（同 `column` 已有的处置）。
#[test]
fn nonascii_option_letter_is_reported_as_one_byte() {
    // 参照件 `tmp/col-313/col` 现量三形都打**裸字节 `c3`** 一个字节 + usage、退 1：
    // `-bé`（`b` 合法，继续吃到 `é` 的第一字节）、`-é`、`-xé`。
    // 这一枚曾钉的是 lossy 后的码位 U+00C3——那对应的是「按字节取但仍按 `char` 渲染」
    // 那一版（打 `c3 83` 两字节）；换成字节形之后期望改成逐字节，
    // 因为 lossy 会把裸 `c3` 与 `c3 83` 都读成替换字符，文本夹具已无力分辨。
    let want: Vec<u8> = [
        b"col: unknown option -- ".as_slice(),
        b"\xc3\n",
        USAGE.as_bytes(),
    ]
    .concat();
    for form in ["-b\u{e9}", "-\u{e9}", "-x\u{e9}"] {
        let (out, code, err) = run_bytes(&[form], b"a\n");
        assert_eq!(out, Vec::new(), "{form} 的报文只走 stderr");
        assert_eq!(code, 1, "{form} 的 stderr 是 {err:?}");
        assert_eq!(err, want, "{form} 的字母该是一个裸字节 c3");
    }

    // ASCII 那一形两个入口一字不差：报的就是 `Z` 本体
    let (_out, code, err) = run_bytes(&["-Z"], b"a\n");
    assert_eq!(code, 1);
    assert_eq!(
        err,
        b"col: unknown option -- Z\nusage: col [-bfpx] [-l nline]\n".to_vec()
    );
}

/// `-l` 的缺值那一形也走字节形：`l` 是 ASCII，两个入口同串（参照件现量
/// `col: option requires an argument -- l` + usage、退 1）。这枚门防的是
/// 「换渲染时把缺值那句的措辞一起漂走」。
#[test]
fn the_missing_value_line_of_l_is_ascii_and_unchanged() {
    let (out, code, err) = run_bytes(&["-l"], b"a\n");
    assert_eq!(out, Vec::new());
    assert_eq!(code, 1);
    assert_eq!(
        err,
        b"col: option requires an argument -- l\nusage: col [-bfpx] [-l nline]\n".to_vec()
    );
}
