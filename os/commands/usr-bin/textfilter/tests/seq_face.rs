//! End-to-end face for the `seq` binary, checked against
//! `minix3/usr.bin/seq/seq.c`（选项环 `:101-125`、`errflg` 出口 `:132-136`、
//! 操作数与两判 `:139-159`、`-f` 的两道判 `:161-167`、`numeric()` `:195-232`、
//! `valid_format()` `:231-287`、`unescape()` `:293-376`、`e_atof()` `:383-400`）
//! 与它驱动的 libc `getopt`（`minix3/lib/libc/stdlib/getopt.c:74-78`、`:81-85`、
//! `:87-92`、`:95-104`、`:112-141`）。
//!
//! `seq.c:101` 的环写作 `while ((optind < argc) && !numeric(argv[optind]) &&
//! (c = getopt(argc, argv, "f:hs:t:w")) != -1)`，参照件 `tmp/seq-320/seq_ref`
//! （`seq.c` 真源 + Minix 的 `getopt.c` + 程序名垫片，宿主现链）逐形量出的形状是：
//! ① 三家句子两个退码——`errflg` 与命令自己的 `errx` 退 **1**，操作数过不了
//!    `strtod` 的那一族退 **2**（`seq 3x` 是 `seq: invalid floating point argument: 3x`，
//!    `seq 1e310` 是三段形 `seq: 1e310: <strerror(ERANGE)>`，末段按 Minix 的
//!    `sys_errlist` 是 `Result too large or too small`（本机 glibc 那张表写作
//!    `Numerical result out of range`，与批次三百一十一记过的同源差一致）；
//! ② usage 只有**一行**且程序名跟着 `getprogname()`（`seq.c:133-136`）；
//! ③ 环**不在第一个坏字母处停**：`seq -+- 3` 先两行 libc（`+` 与 `-`）再一行 usage；
//! ④ `h` 在 optstring 里 ⇒ libc 不为它印任何东西，而 C 的 `case 'h'` 后面没有 `break`，
//!    直接落 `default: errflg++` ⇒ `seq -h` 与 `seq -hw 3` 都**只有 usage 一行**；
//! ⑤ 环前那道 `!numeric(argv[optind])` 只挡以短横开头的操作数（`seq -- -5` 走得通），
//!    而 `-5e310` 因「`e` 后必须紧跟符号才算数字」被判非数字，进环后被拆成**五个**集外
//!    字母各一行；不以 `-` 打头的 token 由 libc 自己收环且不推进 `optind`
//!    （`getopt.c:74-78`），所以 `seq 1e2` 一句不印、直接当操作数打出 1..100，
//!    空 token 在 C 里反倒算数字（`seq ""` 打 `1\n0\n`）；
//! ⑥ 操作数解析次序是 `last`→`first`→`incr`，所以环不重排留下的 `-w` 先被点名
//!    （`seq 3 -w` 与 `seq -w 3 -s ,` 都退 2）；
//! ⑦ 零步长那句的名字随 `first < last` 变，且 `first == last` 归 `decrement` 一支
//!    （`seq 2 0 2` → `zero decrement`）；
//! ⑧ `-f` 的接受集反直觉：只有 `A a E e F G g` 算转换，故 `%d` 与 `%d\n` **都非法**，
//!    而 `abc`、`%%` 合法（零个转换），`%g%g` 因两个转换非法；句子是
//!    ``seq: invalid format string: `%q'``（反引号加单引号，退 1，不接 usage）；
//! ⑨ `-w` 分两级：一次补 `0`（`seq -w 1 10` 打 `01`…`10`），两次起补空格且右对齐
//!    （`seq -w -w -1 1` 打 `-1`/` 0`/` 1`）；`-s`/`-t` 的值过 `unescape`（`\t`、`\101`→`A`、
//!    认不出的 `\q` 连反斜杠一起留着）。
//!
//! **本支没有读写缝缺陷**（既不读标准输入也不开档，同族那十处实例到 `units` 为止）。
//! 浮点渲染与 `-f` 生效后的输出仍是一处已裁定的空白：参照件 `seq -f 'abc' 3` 打三行
//! `abc`，本壳交回一行自认的说明并退 1（登记 `SEQ-FORMAT-RENDER`），所以下面两枚
//! 关于被接受格式的断言钉的是**本壳的形状**，差在哪已写在注释里。

mod face_support;

use face_support::write_stdin;
use std::process::{Command, Stdio};

const SEQ: &str = env!("CARGO_BIN_EXE_seq");
/// `seq.c:133-136` 的那一行，程序名跟着 `getprogname()`。
const USAGE: &str = "usage: seq [-w] [-f format] [-s string] [-t string] [first [incr]] last\n";

fn run(args: &[&str]) -> (String, i32, String) {
    let mut child = Command::new(SEQ)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the seq binary should be runnable");
    write_stdin(&mut child, b"");
    let output = child.wait_with_output().expect("the run should finish");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// 与 `run` 同一条命令缝，但标准错误**逐字节**交回：`from_utf8_lossy` 把一个裸 `c3`
/// 渲染成三字节 U+FFFD，文本形分不清「一个字节」与「两个字节」这两种错法。
fn run_bytes(args: &[&str]) -> (Vec<u8>, i32, Vec<u8>) {
    let mut child = Command::new(SEQ)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the seq binary should be runnable");
    write_stdin(&mut child, b"");
    let output = child.wait_with_output().expect("the run should finish");
    (
        output.stdout,
        output.status.code().unwrap_or(-1),
        output.stderr,
    )
}

/// 集外字母的每一行字母位都是**一个裸字节**，而且 `seq` 的环是累加形 ⇒ 一个多字节字母
/// 交回**几行**。期望逐形取自参照件 `tmp/seq-320/seq_ref`（本轮经同名软链 `seq` 起，
/// 归属先前已验：打 `-Z` 交回 `unknown option -- Z`，是 Minix 的 `getopt.c`）：
/// `-é`（`c3 a9`）是 `seq: unknown option -- ` + `303` + 换行、再来一行同样的句子带
/// `251`、再跟那一行 usage，退 1；`-Zé` 是三行（`Z`、`303`、`251`）；`-aé` 也是三行
/// （`a` 不在选项串里，环照旧往后扫）。这与批次四百零三给 `jot` 纠正的那处家族错判同形——
/// `seq` 是本族唯一累加的环，别把「停在第一个坏字母」的结论套过来。ASCII 那几形
/// （`-Z`、`-+-`、`-ABC`、`-wZ`）本文件已有主人，不重复承载。
#[test]
fn the_libc_lines_of_a_multibyte_flag_are_one_byte_each_for_seq() {
    let usage = USAGE.as_bytes();
    let cases: [(&str, &[&[u8]]); 3] = [
        ("-\u{e9}", &[b"\xc3", b"\xa9"]),
        ("-Z\u{e9}", &[b"Z", b"\xc3", b"\xa9"]),
        ("-a\u{e9}", &[b"a", b"\xc3", b"\xa9"]),
    ];
    for (form, letters) in cases {
        let mut want: Vec<u8> = Vec::new();
        for letter in letters {
            want.extend_from_slice(b"seq: unknown option -- ");
            want.extend_from_slice(letter);
            want.push(b'\n');
        }
        want.extend_from_slice(usage);
        let (out, code, err) = run_bytes(&[form]);
        assert_eq!(out, Vec::new(), "{form} 的报文只走标准错误");
        assert_eq!(code, 1, "{form} 的 stderr 是 {err:?}");
        assert_eq!(err, want, "{form} 的每一行字母位都该是一个裸字节");
    }
}

#[test]
fn off_letter_gets_one_libc_line_and_the_single_usage_line() {
    let (out, code, err) = run(&["-Z"]);
    assert_eq!(code, 1, "errflg 一族退 1（`seq.c:136`），参照件同值");
    assert_eq!(err, format!("seq: unknown option -- Z\n{USAGE}"));
    assert_eq!(out, "");
}

#[test]
fn the_ring_keeps_going_and_accumulates_the_libc_lines() {
    // 旧写法在第一个坏字母处就退出，只能打出一行 libc；参照件 `-+- 3` 是两行。
    let (out, code, err) = run(&["-+-", "3"]);
    assert_eq!(code, 1);
    assert_eq!(out, "", "errflg 置起来后仍要先过操作数判据，不打任何序列");
    assert_eq!(
        err,
        format!("seq: unknown option -- +\nseq: unknown option -- -\n{USAGE}")
    );
    // 聚簇里合法字母先被吃掉，报的是坏的那个
    let (_, code, err) = run(&["-wZ"]);
    assert_eq!(code, 1);
    assert_eq!(err, format!("seq: unknown option -- Z\n{USAGE}"));
    let (_, code, err) = run(&["-Zw"]);
    assert_eq!(code, 1);
    assert_eq!(err, format!("seq: unknown option -- Z\n{USAGE}"));
    // 三个坏字母 ⇒ 三行（`tmp/seq-320/measure4.log`），usage 仍只有一行
    let (_, code, err) = run(&["-ABC", "3"]);
    assert_eq!(code, 1);
    assert_eq!(
        err,
        format!(
            "seq: unknown option -- A\nseq: unknown option -- B\nseq: unknown option -- C\n{USAGE}"
        )
    );
    // 坏字母之后带值字母缺值：两句都是 libc 的，次序按扫描走（`-Zf` 量出同形）
    let (_, code, err) = run(&["-Zf"]);
    assert_eq!(code, 1);
    assert_eq!(
        err,
        format!("seq: unknown option -- Z\nseq: option requires an argument -- f\n{USAGE}")
    );
}

#[test]
fn missing_value_is_reachable_for_f_s_and_t_only() {
    for letter in ['f', 's', 't'] {
        let flag = format!("-{letter}");
        let (out, code, err) = run(&[flag.as_str()]);
        assert_eq!(code, 1, "{flag}");
        assert_eq!(out, "", "{flag}");
        assert_eq!(
            err,
            format!("seq: option requires an argument -- {letter}\n{USAGE}"),
            "{flag}"
        );
    }
    // 聚簇里 `w` 先被吃掉，缺值报 `f`
    let (_, code, err) = run(&["-wf"]);
    assert_eq!(code, 1);
    assert_eq!(
        err,
        format!("seq: option requires an argument -- f\n{USAGE}")
    );
    // `w`/`h` 不带值 ⇒ 这一支对它们到不了：`-w 3` 正常出序列
    let (out, code, err) = run(&["-w", "3"]);
    assert_eq!((code, err), (0, String::new()));
    assert_eq!(out, "1\n2\n3\n");
}

#[test]
fn help_letter_is_in_the_set_and_prints_no_libc_line() {
    for args in [&["-h"][..], &["-h", "3"][..], &["-hw", "3"][..]] {
        let (out, code, err) = run(args);
        assert_eq!(code, 1, "{args:?}");
        assert_eq!(out, "", "{args:?}");
        assert_eq!(err, USAGE, "{args:?} 该只有 usage 一行，libc 那句到不了");
    }
}

#[test]
fn operand_count_face_is_usage_alone() {
    for args in [&[] as &[&str], &["-w"][..], &["1", "2", "3", "4"][..]] {
        let (out, code, err) = run(args);
        assert_eq!(code, 1, "{args:?}");
        assert_eq!(out, "", "{args:?}");
        assert_eq!(err, USAGE, "{args:?}");
    }
}

#[test]
fn every_byte_of_a_nonascii_cluster_is_reported() {
    // `-é` 是两个字节（`c3 a9`），参照件对**每个字节**各打一行（`%c` 打裸字节，
    // `tmp/seq-320/measure3.log` 里是 `\303` 与 `\251`）。批次四百二十把本壳也换到那一形：
    // 之前按 `char` 渲染成 `Ã`/`©` 两个可打印字符（各两字节），那句登记为
    // `SEQ-NONASCII-OPTLETTER`，**本轮闭合**，所以这一枚门的期望**有意识翻正**成
    // 逐字节的 `c3`/`a9`（翻正前后的差别正是这一族的全部要点：文本形里 `c3` 会被
    // `from_utf8_lossy` 变成替换字符，断不出字节，故这里改用 `run_bytes`）。
    let (out, code, err) = run_bytes(&["-\u{e9}"]);
    assert_eq!(out, Vec::new());
    assert_eq!(code, 1);
    assert_eq!(
        err,
        [
            b"seq: unknown option -- \xc3\n".as_slice(),
            b"seq: unknown option -- \xa9\n".as_slice(),
            USAGE.as_bytes(),
        ]
        .concat()
    );
}

#[test]
fn a_token_without_a_leading_dash_ends_the_ring_without_any_line() {
    // 环前那道 `!numeric(...)` 只在以短横开头的 token 上起作用：`1e2` 被护栏判为
    // 「不是数字」（`e` 后无符号），但 libc 一看首字符不是 `-` 就收环且不推进 `optind`
    // （`getopt.c:74-78`），于是它就是操作数——参照件 `seq 1e2` 打满 1..100。
    let (out, code, err) = run(&["1e2"]);
    assert_eq!((code, err), (0, String::new()));
    let expected: String = (1..=100).map(|n| format!("{n}\n")).collect();
    assert_eq!(out, expected, "参照件 `seq 1e2` 打满 1..100");
    // 空 token 在 C 里也是「数字」（`while (*s)` 一步不走而判据是 `*s == '\0'`）：
    // `seq ""` 是 last=0 ⇒ 打 1 与 0，参照件字节量为 `1\n0\n`
    let (out, code, err) = run(&[""]);
    assert_eq!((code, err), (0, String::new()));
    assert_eq!(out, "1\n0\n");
    let (out, code, err) = run(&["", "3"]);
    assert_eq!((code, err), (0, String::new()));
    assert_eq!(out, "0\n1\n2\n3\n");
}

#[test]
fn the_numeric_guard_lets_negative_operands_through() {
    // 护栏认不出 `-5e310`（`e` 后无符号），于是它进环被拆成集外字母；参照件同形且
    // **五个字母各一行**（`tmp/seq-320/measure4.log`）
    let (_, code, err) = run(&["-5e310"]);
    assert_eq!(code, 1);
    assert_eq!(
        err,
        format!(
            "seq: unknown option -- 5\nseq: unknown option -- e\nseq: unknown option -- 3\n\
             seq: unknown option -- 1\nseq: unknown option -- 0\n{USAGE}"
        )
    );
    // `-5` 是数字 ⇒ 直接是操作数：C 打 1 0 -1 -2 -3 -4 -5
    let (out, code, err) = run(&["-5"]);
    assert_eq!((code, err), (0, String::new()));
    assert_eq!(out, "1\n0\n-1\n-2\n-3\n-4\n-5\n");
    // `--` 被吃掉，`-- -Z` 只剩一个坏操作数
    let (_, code, err) = run(&["--", "-Z"]);
    assert_eq!(code, 2);
    assert_eq!(err, "seq: invalid floating point argument: -Z\n");
}

#[test]
fn operand_errors_take_the_last_operand_first_and_exit_two() {
    // 环不重排：`seq 3 -w` 的两个 token 都是操作数，而 C 先算 `last` ⇒ 点名 `-w`。
    for (args, word) in [
        (&["3", "-w"][..], "-w"),
        (&["-w", "3", "-s", ","][..], ","),
        (&["3x"][..], "3x"),
    ] {
        let (out, code, err) = run(args);
        assert_eq!(code, 2, "{args:?} 走 e_atof 那一族");
        assert_eq!(out, "", "{args:?}");
        assert_eq!(
            err,
            format!("seq: invalid floating point argument: {word}\n"),
            "{args:?}"
        );
    }
    // ERANGE 那一支是 `err(2, "%s", num)` 的三段形，带 strerror：上溢与「下溢到 0」
    // 同一条出口（参照件 `1e310`、`1e-40000` 各退 2，`measure3.log`）
    for word in ["1e310", "1e-40000"] {
        let (out, code, err) = run(&[word]);
        assert_eq!(code, 2, "{word}");
        assert_eq!(out, "", "{word}");
        assert_eq!(
            err,
            // 末段取自 Minix 的 `sys_errlist`（`compat_errlist.c` 的 34 号是
            // `Result too large or too small`）；本机链的参照件走 glibc 那张表，
            // 打 `Numerical result out of range` —— 与批次三百一十一记过的同源差一致。
            format!("seq: {word}: Result too large or too small\n"),
            "{word}"
        );
    }
}

#[test]
fn zero_stride_sentence_follows_the_direction() {
    // `(first < last) ? "in" : "de"`：相等也算 de 支（参照件 `seq 2 0 2` 同）
    for (args, name) in [
        (&["1", "0", "3"][..], "increment"),
        (&["5", "0", "1"][..], "decrement"),
        (&["2", "0", "2"][..], "decrement"),
    ] {
        let (out, code, err) = run(args);
        assert_eq!(code, 1, "{args:?}");
        assert_eq!(out, "", "{args:?}");
        assert_eq!(err, format!("seq: zero {name}\n"), "{args:?}");
    }
    // 反方向两判排在零步长之后
    let (_, code, err) = run(&["1", "-1", "5"]);
    assert_eq!(
        (code, err),
        (1, "seq: needs positive increment\n".to_string())
    );
    let (_, code, err) = run(&["5", "1", "1"]);
    assert_eq!(
        (code, err),
        (1, "seq: needs negative decrement\n".to_string())
    );
}

#[test]
fn format_validation_rejects_integer_conversions_with_the_named_sentence() {
    // 参照件现量：`%d`、`%d\n`、`%d%d`、`%g%g`、`%q` 全部非法，句子带原文且**不接 usage**
    for fmt in ["%d", "%d\n", "%d%d", "%g%g", "%q", "%"] {
        let (out, code, err) = run(&["-f", fmt, "3"]);
        assert_eq!(code, 1, "{fmt:?}");
        assert_eq!(out, "", "{fmt:?}");
        assert_eq!(
            err,
            format!("seq: invalid format string: `{fmt}'\n"),
            "{fmt:?}"
        );
    }
}

#[test]
fn accepted_formats_reach_the_float_render_gap_the_engine_declares() {
    // 参照件对 `%g`/`abc`/`%%` 是照常出数（`abc` 打三行字面量），本壳只走整数：
    // 这一处差登记为 SEQ-FORMAT-RENDER，断言钉的是本壳自认的形状。
    let (out, code, err) = run(&["-f", "%g", "3"]);
    assert_eq!(code, 1);
    assert_eq!(out, "");
    assert_eq!(err, "seq: -f formats wait for the float renderer\n");
    let (_, code, _) = run(&["-f", "abc", "3"]);
    assert_eq!(code, 1, "零转换的串在 C 里合法，走到同一处空白");
}

#[test]
fn equalize_has_two_levels_and_the_second_switches_to_spaces() {
    let (out, code, err) = run(&["-w", "1", "10"]);
    assert_eq!((code, err), (0, String::new()));
    assert_eq!(out, "01\n02\n03\n04\n05\n06\n07\n08\n09\n10\n");
    // 空格那一级是 `% 2g`：非负数**也占一个符号位**，所以 `10` 打成三列的 `" 10"`
    // （超宽不截），而不是右对齐到两列。`tmp/seq-320/measure6.log` 逐字节量出。
    let (out, code, err) = run(&["-w", "-w", "1", "10"]);
    assert_eq!((code, err), (0, String::new()));
    assert_eq!(out, " 1\n 2\n 3\n 4\n 5\n 6\n 7\n 8\n 9\n 10\n");
    let (out, code, err) = run(&["-w", "-w", "8", "10"]);
    assert_eq!((code, err), (0, String::new()));
    assert_eq!(out, " 8\n 9\n 10\n");
    // 宽度取自**实际被打印的**首尾两值：`seq -w 1 5 10` 只打 1 与 6 ⇒ 宽度 1
    let (out, code, err) = run(&["-w", "1", "5", "10"]);
    assert_eq!((code, err), (0, String::new()));
    assert_eq!(out, "1\n6\n");
    let (out, code, err) = run(&["-w", "-w", "1", "5", "10"]);
    assert_eq!((code, err), (0, String::new()));
    assert_eq!(out, " 1\n 6\n");
    // 降序也一样：`seq -w 10 -3 1` 打 10 7 4 1，宽度取自 first=10 ⇒ 零填到两列
    let (out, code, err) = run(&["-w", "10", "-3", "1"]);
    assert_eq!((code, err), (0, String::new()));
    assert_eq!(out, "10\n07\n04\n01\n");
    let (out, code, err) = run(&["-w", "-w", "10", "-3", "1"]);
    assert_eq!((code, err), (0, String::new()));
    assert_eq!(out, " 10\n 7\n 4\n 1\n");
    // 跨零的区间里两级各自的符号位：`0` 在 `%02g` 下是 `00`，在 `% 2g` 下是 `" 0"`
    let (out, code, err) = run(&["-w", "-5", "5"]);
    assert_eq!((code, err), (0, String::new()));
    assert_eq!(out, "-5\n-4\n-3\n-2\n-1\n00\n01\n02\n03\n04\n05\n");
    let (out, code, err) = run(&["-w", "-w", "-5", "5"]);
    assert_eq!((code, err), (0, String::new()));
    assert_eq!(out, "-5\n-4\n-3\n-2\n-1\n 0\n 1\n 2\n 3\n 4\n 5\n");
    // 三位宽的空格级：`-9` 得 `" -9"`（两列）而 `0` 得 `"  0"`（三列）——
    // 宽度取自 `-10`，符号位与对齐填充是两件事
    let (out, code, err) = run(&["-w", "-w", "-10", "10"]);
    assert_eq!((code, err), (0, String::new()));
    assert_eq!(
        out,
        "-10\n -9\n -8\n -7\n -6\n -5\n -4\n -3\n -2\n -1\n  0\n  1\n  2\n  3\n  4\n  5\n  6\n  7\n  8\n  9\n 10\n"
    );
    // 第三级仍是空格，且符号占一格
    let (out, code, err) = run(&["-w", "-w", "-w", "-1", "1"]);
    assert_eq!((code, err), (0, String::new()));
    assert_eq!(out, "-1\n 0\n 1\n");
}

#[test]
fn separator_and_terminator_values_pass_through_unescape() {
    let (out, code, err) = run(&["-s", "\\t", "1", "3"]);
    assert_eq!((code, err), (0, String::new()));
    assert_eq!(
        out, "1\t2\t3\t",
        "C 的 `sep = unescape(optarg)`，每个数之后都放分隔符"
    );
    let (out, code, err) = run(&["-t", "\\a", "3"]);
    assert_eq!((code, err), (0, String::new()));
    assert_eq!(out, "1\n2\n3\n\u{7}", "terminator 只加在最后一个之后");
    // 认不出的转义把反斜杠原样留着；八进制 `\101` 就是 `A`
    let (out, _, _) = run(&["-s", "\\q", "3"]);
    assert_eq!(out, "1\\q2\\q3\\q");
    let (out, _, _) = run(&["-s", "\\101", "3"]);
    assert_eq!(out, "1A2A3A");
}

#[test]
fn plain_sequences_keep_the_direction_following_default_stride() {
    for (args, want) in [
        (vec!["3"], "1\n2\n3\n"),
        (vec!["1", "3"], "1\n2\n3\n"),
        (vec!["1", "2", "6"], "1\n3\n5\n"),
        (vec!["3", "1"], "3\n2\n1\n"),
        (vec!["0"], "1\n0\n"),
    ] {
        let (out, code, err) = run(&args);
        assert_eq!(
            (code, err.clone()),
            (0, String::new()),
            "{args:?} → stderr={err}"
        );
        assert_eq!(out, want, "{args:?}");
    }
}
