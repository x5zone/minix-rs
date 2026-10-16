//! End-to-end face for the `colrm` binary, checked against
//! `minix3/usr.bin/colrm/colrm.c`（选项环 `:67-74`、操作数段 `:76-95`、
//! 列走与读写 `:97-121`、`check()` `:123-130`、`usage()` `:132-137`）与它驱动的
//! libc `getopt`（`minix3/lib/libc/stdlib/getopt.c:74-78`、`:81-92`、`:95-104`）。
//!
//! `colrm.c:67` 的 optstring 是**空串**，与批次三百零九的 `rev` 逐字同构，所以那一环的
//! 三形照搬；但 `colrm` 的操作数不是文件名而是**列号**，于是同一个裸 `-` 在两条命令里
//! 落到完全不同的句子。参照件 `tmp/colrm-310/colrm_ref`（`colrm.c` 真源 + Minix 的
//! `getopt.c` + 程序名垫片，宿主现链）逐形量出的形状是：
//! ① 任何 `-` 开头的 token（除 `--` 与裸 `-`）= libc 一行 + usage 一行，退 1；
//! ② `--` 结束选项环且自身不算操作数，裸 `-` 则是操作数、被当列号拒收；
//! ③ 不重排：操作数之后的 `-Z` 也是列号（`colrm: illegal column -- -Z`）；
//! ④ 两支列号都坏时报**后写**的那支（C 先算 `argv[1]` 再 FALLTHROUGH）；
//! ⑤ 列号走 `strtol`：前导空白、`+` 号、溢出饱和都收，尾随字符与 0 拒收，
//!    负数经 `u_long` 回绕成极大值后被收（`colrm -- -5` 退 0 且整行透传）；
//! ⑥ `err(1, ...)` 那行三段形缀 `strerror`，`errx` 那行两段形不缀。
//!
//! 这扇门前不存在：`colrm` 的读缝直接拿裸 `minix_sys::read`，宿主上读不到任何输入，
//! 所以它的输出以前只能靠库测（同批次三百零六的 `nl`、三百零八的 `fold`、三百零九的 `rev`）。

mod face_support;

use face_support::write_stdin;
use std::process::{Command, Stdio};

const COLRM: &str = env!("CARGO_BIN_EXE_colrm");
const USAGE: &str = "usage: colrm [start [stop]]\n";
/// 参照件用的输入（`tmp/colrm-310/in.txt`），金样逐形取自它的读数。
const INPUT: &[u8] = b"abcdefg\nhi\n";

fn run(args: &[&str], input: &[u8]) -> (String, i32, String) {
    let mut child = Command::new(COLRM)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the colrm binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// 集外字母：libc 那一行在前、`usage()` 在后，退 1（`colrm.c:67-72` 的 `case '?': default`）。
/// 参照件读数 `colrm: unknown option -- Z` + usage。旧写法少了 libc 那一行。
#[test]
fn an_unknown_letter_prints_the_libc_line_then_the_usage() {
    let (out, status, err) = run(&["-Z"], INPUT);
    assert_eq!(out, "", "报文只走 stderr");
    assert_eq!(status, 1, "stderr 是 {err:?}");
    assert_eq!(err, format!("colrm: unknown option -- Z\n{USAGE}"));
}

/// 聚簇只报**第一个**字符：调用者拿到 `'?'` 就 `usage()` 退出，不回到 getopt 读第二个
/// （参照件：`-bZ` → `unknown option -- b`）。
#[test]
fn a_cluster_reports_only_its_first_letter() {
    let (out, status, err) = run(&["-bZ"], INPUT);
    assert_eq!((out.as_str(), status), ("", 1));
    assert_eq!(err, format!("colrm: unknown option -- b\n{USAGE}"));
}

/// `--x` 报的字母是第二个连字符：optstring 里没有 `-`，它也是集外字符
/// （参照件：`unknown option -- -`）。纯 `--` 走的是另一支。
#[test]
fn a_dash_followed_by_a_word_is_an_unknown_option_named_by_the_dash() {
    let (out, status, err) = run(&["--x"], INPUT);
    assert_eq!((out.as_str(), status), ("", 1));
    assert_eq!(err, format!("colrm: unknown option -- -\n{USAGE}"));
}

/// `--` 自身被吃掉、后面的都算操作数（`getopt.c:81-85` 先 `++optind`）：
/// 单给 `--` 仍是整行透传，`-- 2 4` 则真的删 2..4 列。
#[test]
fn the_end_of_options_marker_is_not_an_operand() {
    let (out, status, err) = run(&["--"], INPUT);
    assert_eq!(
        (out.as_str(), err.as_str(), status),
        ("abcdefg\nhi\n", "", 0)
    );
    let (out, status, err) = run(&["--", "2", "4"], INPUT);
    assert_eq!(
        (out.as_str(), err.as_str(), status),
        ("aefg\nh\n", "", 0),
        "`--` 后面的列号该生效"
    );
}

/// 裸 `-` 是操作数而不是选项：optstring 里没有 `-`，getopt 结束选项环时**不推进**
/// `optind`（`getopt.c:87-92`），于是它落进 `argv += optind` 那一段，被当列号拒收
/// （参照件：`colrm: illegal column -- -`，一行、没有 usage、退 1）。
/// 同一个 token 在 `rev` 那里走的是 `fopen` 失败支，句子完全不同。
#[test]
fn a_solitary_dash_is_an_operand_rejected_as_a_column() {
    let (out, status, err) = run(&["-"], INPUT);
    assert_eq!(out, "", "`-` 不是「读标准输入」的别名");
    assert_eq!(status, 1, "stderr 是 {err:?}");
    assert_eq!(err, "colrm: illegal column -- -\n");
    assert!(!err.contains("usage:"), "列号的 errx 不接 usage：{err:?}");
}

/// 不重排：非选项 token 一到，选项环就结束（`getopt.c:74-78`），所以 `colrm 2 -Z` 里的
/// `-Z` 是个**列号**（参照件：`illegal column -- -Z`，而不是 `unknown option`）。
#[test]
fn an_option_looking_token_after_an_operand_is_a_column() {
    let (out, status, err) = run(&["2", "-Z"], INPUT);
    assert_eq!(out, "");
    assert_eq!(status, 1, "stderr 是 {err:?}");
    assert_eq!(err, "colrm: illegal column -- -Z\n");
}

/// 两支都坏时报**后写**的那支：C 的 `case 2:` 先对 `argv[1]` 做 `strtol` 并检查，
/// `/* FALLTHROUGH */` 才轮到 `argv[0]`（`colrm.c:78-87`）。参照件：`abc def` → `def`。
#[test]
fn the_stop_column_is_checked_before_the_start_column() {
    let (out, status, err) = run(&["abc", "def"], INPUT);
    assert_eq!((out.as_str(), status), ("", 1));
    assert_eq!(err, "colrm: illegal column -- def\n");
    // 只有 start 坏时，报的才是 start。
    let (_, status, err) = run(&["abc", "2"], INPUT);
    assert_eq!(
        (err.as_str(), status),
        ("colrm: illegal column -- abc\n", 1)
    );
}

/// 列号的接受面是 `strtol` 而不是「一串数字」：前导空白、显式 `+`、前导零都收；
/// 零与尾巴有字符的都拒（C 测 `*p`）。参照件逐形量过。
#[test]
fn column_numbers_take_the_strtol_forms() {
    for accepted in ["3", " 3", "+3", "03"] {
        let (out, status, err) = run(&[accepted], INPUT);
        assert_eq!(
            (out.as_str(), err.as_str(), status),
            ("ab\nhi\n", "", 0),
            "`{accepted}` 该被当作列号 3"
        );
    }
    for rejected in ["0", "+0", "3x", "0x10", "1e2", "3 "] {
        let (out, status, err) = run(&[rejected], INPUT);
        assert_eq!(out, "", "报文只走 stderr");
        assert_eq!(status, 1, "`{rejected}` 该被拒收：stderr 是 {err:?}");
        assert_eq!(
            err,
            format!("colrm: illegal column -- {rejected}\n"),
            "`{rejected}` 的句子该带操作数原文"
        );
    }
}

/// 溢出与负数都**不被拒**：`strtol` 溢出饱和到 `long` 的端点，而 `colrm.c:63` 把它存进
/// `u_long`，于是 C 的 `<= 0` 只挡得住 0，负数回绕成极大值。两者都比任何真实列号大，
/// 整行透传、退 0（参照件：`colrm 99999999999999999999999` 与 `colrm -- -5` 全透传）。
#[test]
fn an_overflowing_or_negative_column_passes_everything_through() {
    for huge in ["99999999999999999999999", "100"] {
        let (out, status, err) = run(&[huge], INPUT);
        assert_eq!(
            (out.as_str(), err.as_str(), status),
            ("abcdefg\nhi\n", "", 0),
            "列号 `{huge}` 该被接受并透传"
        );
    }
    let (out, status, err) = run(&["--", "-5"], INPUT);
    assert_eq!(
        (out.as_str(), err.as_str(), status),
        ("abcdefg\nhi\n", "", 0),
        "负列号经 `u_long` 回绕成极大值，C 不拒它"
    );
}

/// 多于两个操作数只印 usage（`colrm.c:90-91` 的 `default: usage()`），没有 libc 那一行，
/// 也没有列号句子。
#[test]
fn too_many_operands_print_only_the_usage() {
    let (out, status, err) = run(&["2", "4", "6"], INPUT);
    assert_eq!(out, "");
    assert_eq!(status, 1, "stderr 是 {err:?}");
    assert_eq!(err, USAGE);
}

/// 起始列大于结束列那一支是 `err(1, ...)` 而不是 `errx`：三段形里缀 `strerror(errno)`，
/// 而此刻 `errno` 是 0，句子内容由各平台的表定（参照件在本机印 `Success`），
/// 所以本壳的门只钉形状：程序名 + 句子 + 一段非空后缀、一行、不接 usage、退 1。
#[test]
fn start_after_stop_prints_the_err_line_without_the_usage() {
    let (out, status, err) = run(&["4", "2"], INPUT);
    assert_eq!(out, "");
    assert_eq!(status, 1, "stderr 是 {err:?}");
    let head = "colrm: illegal start and stop columns: ";
    assert!(err.starts_with(head), "该以 `{head}` 开头，实际 {err:?}");
    let tail = err[head.len()..].trim_end();
    assert!(
        !tail.is_empty() && !tail.contains('\n'),
        "缀的应是一句 strerror 文本：{err:?}"
    );
    assert!(!err.contains("usage:"), "这一支不接 usage：{err:?}");
}

/// 输出腿（本闸门存在的另一半）：以前 `colrm` 用裸 `minix_sys::read`，宿主上**读不到输入**，
/// 所以列走从来没有被进程级测过；而且旧写法把输入过一遍 UTF-8 校验、非文本字节整段丢掉。
/// 金样逐形取自参照件读数（`tmp/colrm-310/golden.log`）。
#[test]
fn the_column_walk_reaches_the_output_end_to_end() {
    // 只给起始列：从该列起到行末全删（stop 留 0）。
    assert_eq!(run(&["3"], INPUT).0, "ab\nhi\n");
    // 区间：列 2..4 删掉。
    assert_eq!(run(&["2", "4"], INPUT).0, "aefg\nh\n");
    assert_eq!(run(&["1", "1"], INPUT).0, "bcdefg\ni\n");
    // 制表符把列进到 8 的倍数：删 2..8 时第一个制表符落点 8 被删，
    // 第二个落点 16 在区间外而留下。
    let (out, status, _) = run(&["2", "8"], b"a\tb\tc\n");
    assert_eq!((out.as_str(), status), ("ab\tc\n", 0));
    let (out, status, _) = run(&["2", "9"], b"a\tb\tc\n");
    assert_eq!((out.as_str(), status), ("a\tc\n", 0));
    // 退格把列拉回，被拉回后落点仍在区间外的字节留下。
    let (out, status, _) = run(&["2"], b"ab\x08\x08X\n");
    assert_eq!((out.as_str(), status), ("a\x08\x08X\n", 0));
    // 末行没有换行也照删，且不补换行（C 一路 putchar，从不自己加）。
    let (out, status, _) = run(&["2", "3"], b"abcdef");
    assert_eq!((out.as_str(), status), ("adef", 0));
    // 空输入产出空。
    let (out, status, err) = run(&["2"], b"");
    assert_eq!((out.as_str(), err.as_str(), status), ("", "", 0));
    // 非 UTF-8 字节不被整段丢弃：C 按字节走列，本壳也必须按字节（旧写法先做 UTF-8
    // 校验、校验不过就把整个输入当空）。这里 `2 3` 删掉的正是那两个不可打印字节。
    let (out, status, _) = run(&["2", "3"], b"a\xff\xbccdef\n");
    assert_eq!((out.as_str(), status), ("acdef\n", 0));
}
