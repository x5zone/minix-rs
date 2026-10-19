//! End-to-end face for the `unexpand` binary, checked against
//! `minix3/usr.bin/unexpand/unexpand.c`（选项环 `:85-118`、档位回显 `:120-121`、
//! 操作数循环 `:122-136`、`tabify` `:141-215`、`usage()` `:69-73`）与它驱动的 libc
//! `getopt`（`minix3/lib/libc/stdlib/getopt.c:74-78`、`:81-92`、`:95-104`、`:132-136`）。
//!
//! `unexpand.c:86` 的 optstring 是 `"at:"`：一个无值字母加一个带值字母，两支互斥。
//! 参照件 `tmp/unexpand-312/unexpand_ref`（`unexpand.c` 真源 + Minix 的 `getopt.c` +
//! 程序名与 `fgetln`/`erealloc` 垫片，宿主现链）逐形量出的形状是：
//! ① 集外字母 = libc 一行 + usage 一行，退 1（`-Z`、`-Za` 报 `Z`、`--x` 报 `-`）；
//! ② `-t` 缺值 = libc 的 `option requires an argument -- t` + usage 一行，退 1；
//! ③ 档位表在选项环之后被**打到标准错误**（`unexpand.c:120-121` 那圈 `fprintf(stderr,
//!    "%lu %zu\n", i, tabstops[i])`）——真源里留着的一行调试输出，但它就在外部行为里；
//! ④ 档位的数字是 `strtoul(..., 0)`：`010` 是 8、`0x10` 是 16；坏档位有两支不同的句子
//!    （`Invalid tabstop`与 `Bad tabstop spec …, must be greater than the previous …`）；
//! ⑤ `-a` 与 `-t` 冲突只印 usage 一行（**首字母大写**、程序名跟着 `getprogname()`、
//!    带 `[file ...]`），退 1；
//! ⑥ 不重排：非选项 token 一到选项环就结束，所以 `unexpand -t 4 8` 的 `8` 是**第二支档名**
//!    而不是档位表的续写（参照件：先 `0 4`，再 `Cannot open `8'`）；
//! ⑦ 打不开的档名是 `err(EXIT_FAILURE, "Cannot open `%s'")` 三段形；裸 `-` 不是 stdin 的
//!    别名，它被送去 `freopen` 而报错；
//! ⑧ 末行没有换行时**不补**换行（C 的 `putchar` 只写输入里有的字节）。
//!
//! 这扇门前不存在：`unexpand` 的读缝直接拿裸 `open`/`read`，宿主上读不到任何输入
//! （同族第六例），所以它的输出以前只能靠库测。

mod face_support;

use face_support::write_stdin;
use std::fs;
use std::process::{Command, Stdio};

const UNEXPAND: &str = env!("CARGO_BIN_EXE_unexpand");
/// C `unexpand.c:70-71` 的那一行：首字母大写、程序名跟着 `getprogname()`、带 `[file ...]`。
const USAGE: &str = "Usage: unexpand [-a] [-t tabstop] [file ...]\n";
/// 参照件用的输入（`tmp/unexpand-312/lead.txt` 同料）：八空格一行、四空格一行。
const INPUT: &[u8] = b"        x\n    y\n";

fn run(args: &[&str], input: &[u8]) -> (String, i32, String) {
    let mut child = Command::new(UNEXPAND)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the unexpand binary should be runnable");
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
    let mut child = Command::new(UNEXPAND)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the unexpand binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        output.stdout,
        output.status.code().unwrap_or(-1),
        output.stderr,
    )
}

/// 集外字母那一行的字母位是**一个裸字节**。期望逐形取自参照件 `tmp/unexpand-312/unexpand_ref`
/// （本轮按批次四百零七那条规矩先打 `-Z` 验过归属：它交回 `unknown option -- Z`，是 Minix 的
/// `getopt.c` 而不是 glibc 那句带引号的 `invalid option -- 'Z'`）：`-é` 与 `-aé` 都是
/// `unexpand: unknown option -- ` + 单个 `303` + 换行再跟那一行 usage、退 **1**；`-Zé` 停在
/// `Z`（环不越过第一个坏字母）；反面的 `-té` 里 `é` 是**值**而不是字母，走命令自己的
/// `unexpand: Invalid tabstop `303 251'` **单独一行、不接 usage**（`unexpand.c:101-104`）。
/// ASCII 那几形（`-Z`、`-Za`、`--x`、`-t` 缺值）本文件已有主人，不重复承载。
#[test]
fn the_libc_line_of_a_multibyte_flag_is_one_byte_for_unexpand() {
    let want: Vec<u8> = [
        b"unexpand: unknown option -- ".as_slice(),
        b"\xc3\n",
        USAGE.as_bytes(),
    ]
    .concat();
    for form in ["-\u{e9}", "-a\u{e9}"] {
        let (out, status, err) = run_bytes(&[form], INPUT);
        assert_eq!(out, Vec::new(), "{form} 的报文只走标准错误");
        assert_eq!(status, 1, "{form} 的 stderr 是 {err:?}");
        assert_eq!(err, want, "{form} 的字母该是一个裸字节 c3");
    }

    let (_out, status, err) = run_bytes(&["-Z\u{e9}"], INPUT);
    assert_eq!(status, 1);
    assert_eq!(
        err,
        [
            b"unexpand: unknown option -- Z\n".as_slice(),
            USAGE.as_bytes(),
        ]
        .concat(),
        "环停在第一个坏字母上，第二个字节根本不该被读到"
    );

    let (_out, status, err) = run_bytes(&["-t\u{e9}"], INPUT);
    assert_eq!(status, 1);
    assert_eq!(
        err,
        b"unexpand: Invalid tabstop `\xc3\xa9'\n".to_vec(),
        "带值字母把两字节整个当值收走，句子是命令自己的那一句、不接 usage"
    );
}

/// 集外字母：libc 那一行在前、`usage()` 在后，退 1（`unexpand.c:115-117` 的 `default:`）。
#[test]
fn an_unknown_letter_prints_the_libc_line_then_the_usage() {
    let (out, status, err) = run(&["-Z"], INPUT);
    assert_eq!(out, "");
    assert_eq!(status, 1, "stderr 是 {err:?}");
    assert_eq!(err, format!("unexpand: unknown option -- Z\n{USAGE}"));
}

/// 聚簇只报第一个字符（参照件：`-Za` → `unknown option -- Z`）。
#[test]
fn a_cluster_reports_only_its_first_letter() {
    let (out, status, err) = run(&["-Za"], INPUT);
    assert_eq!((out.as_str(), status), ("", 1));
    assert_eq!(err, format!("unexpand: unknown option -- Z\n{USAGE}"));
}

/// `--x` 报的字母是第二个连字符：optstring 里没有 `-`（参照件：`unknown option -- -`）。
#[test]
fn a_dash_followed_by_a_word_is_an_unknown_option_named_by_the_dash() {
    let (_, status, err) = run(&["--x"], INPUT);
    assert_eq!(status, 1);
    assert_eq!(err, format!("unexpand: unknown option -- -\n{USAGE}"));
}

/// `-t` 后面没有那一段值是 libc 的**另一句**，不是 usage 自己那一行。
#[test]
fn a_missing_tabstop_value_prints_the_other_libc_line() {
    let (out, status, err) = run(&["-t"], INPUT);
    assert_eq!((out.as_str(), status), ("", 1));
    assert_eq!(
        err,
        format!("unexpand: option requires an argument -- t\n{USAGE}")
    );
}

/// 档位表在选项环之后、读输入之前打到 stderr（`unexpand.c:120-121`）。
/// 这一形容易被当成「真源里的噪声」而省掉，但参照件就是把它印出来，退码还是 0。
#[test]
fn the_stop_list_is_echoed_to_stderr_before_any_output() {
    let (out, status, err) = run(&["-t", "4"], INPUT);
    // 档位在 4 时，行首四个空格也跨一档，所以第二行同样折成制表符（参照件同形）。
    assert_eq!(
        (out.as_str(), status),
        ("\tx\n\ty\n", 0),
        "stderr 是 {err:?}"
    );
    assert_eq!(err, "0 4\n", "C 那一行是「下标 空格 档位」，不带程序名");

    let (out, status, err) = run(&["-t", "4,8"], INPUT);
    assert_eq!((out.as_str(), status), ("\t\tx\n\ty\n", 0));
    assert_eq!(err, "0 4\n1 8\n");

    // 一支档位都没有时那圈一次都不进，所以 stderr 是空的（参照件：`-t \"\"`）。
    let (out, status, err) = run(&["-t", ""], INPUT);
    assert_eq!(
        (out.as_str(), status),
        ("\tx\n    y\n", 0),
        "空表等于默认档"
    );
    assert_eq!(err, "");
}

/// 两支坏档位的句子逐字不同，也不能与 usage 混成一句。
#[test]
fn the_two_stop_sentences_are_distinct_and_carry_the_offending_piece() {
    let (out, status, err) = run(&["-t", "x"], INPUT);
    assert_eq!((out.as_str(), status), ("", 1));
    assert_eq!(err, "unexpand: Invalid tabstop `x'\n");

    let (_, status, err) = run(&["-t", "3x"], INPUT);
    assert_eq!(status, 1);
    assert_eq!(err, "unexpand: Invalid tabstop `3x'\n");

    let (_, status, err) = run(&["-t", "8,4"], INPUT);
    assert_eq!(status, 1);
    assert_eq!(
        err, "unexpand: Bad tabstop spec `4', must be greater than the previous `8'\n",
        "递减那支报的是后写的档位与前一支的值"
    );
    assert!(!err.contains("Usage"), "这两句都不接 usage：{err:?}");
}

/// 档位的数字是 base 0：`010` 是八进制的 8、`0x10` 是十六进制的 16（参照件的回显为证）。
#[test]
fn the_stop_numbers_are_base_zero() {
    let (out, status, err) = run(&["-t", "010"], INPUT);
    assert_eq!(
        (out.as_str(), status),
        ("\tx\n    y\n", 0),
        "stderr 是 {err:?}"
    );
    assert_eq!(err, "0 8\n", "前导零走八进制");

    let (out, status, err) = run(&["-t", "0x10"], INPUT);
    assert_eq!((out.as_str(), status), ("        x\n    y\n", 0));
    assert_eq!(
        err, "0 16\n",
        "0x 走十六进制，16 列比这行还宽，所以什么都不折"
    );
}

/// `-a` 与 `-t` 互斥：两支冲突都只印 usage 一行（没有 libc 那一行），退 1。
#[test]
fn the_a_and_t_conflict_prints_only_the_usage() {
    for args in [
        &["-a", "-t", "4"][..],
        &["-t", "4", "-a"][..],
        &["-at4"][..],
    ] {
        let (out, status, err) = run(args, INPUT);
        assert_eq!((out.as_str(), status), ("", 1), "args 是 {args:?}");
        assert_eq!(err, USAGE, "冲突那一支只印 usage：{args:?}");
    }
}

/// 不重排：非选项 token 一到选项环就结束。`unexpand -t 4 8` 的 `8` 是**第二支档名**，
/// 参照件先在 stderr 回显 `0 4`，再去开那支不存在的档而报三段形。
#[test]
fn a_non_option_token_ends_the_ring() {
    let missing = std::env::temp_dir().join(format!("ue312-eight-{}", std::process::id()));
    let (out, status, err) = run(&["-t", "4", missing.to_str().unwrap()], INPUT);
    assert_eq!(out, "");
    assert_eq!(status, 1, "stderr 是 {err:?}");
    assert!(
        err.starts_with("0 4\nunexpand: Cannot open `"),
        "档位回显之后该是第二支档名的三段形：{err:?}"
    );

    let dir = scratch();
    let file = dir.join("in");
    fs::write(&file, INPUT).expect("write operand");
    // `unexpand <file> -a`：`-a` 被当第二支档名，参照件报 `Cannot open `-a'`。
    let (_, status, err) = run(&[file.to_str().unwrap(), "-a"], b"");
    assert_eq!(status, 1);
    assert!(
        err.starts_with("unexpand: -a: ") || err.contains("Cannot open `-a'"),
        "操作数之后的 `-a` 该被当档名处理：{err:?}"
    );
    let _ = fs::remove_dir_all(dir);
}

/// 打不开的档名是 `err(EXIT_FAILURE, "Cannot open `%s'")` 三段形：程序名、带反引号的
/// 档名、`strerror` 句子，一行、不接 usage、退 1。裸 `-` 也走这里——`unexpand` 不给
/// 它开 stdin 的特例（C 直接拿去 `freopen`）。
#[test]
fn an_unopenable_operand_prints_the_err_shape_in_three_parts() {
    let missing = std::env::temp_dir().join(format!("ue312-absent-{}", std::process::id()));
    let (out, status, err) = run(&[missing.to_str().unwrap()], INPUT);
    assert_eq!(out, "");
    assert_eq!(status, 1, "stderr 是 {err:?}");
    let head = format!("unexpand: Cannot open `{}': ", missing.to_str().unwrap());
    assert!(err.starts_with(&head), "该以 `{head}` 开头，实际 {err:?}");
    let tail = err[head.len()..].trim_end();
    assert!(
        !tail.is_empty() && !tail.contains('\n'),
        "缀的应是一句 strerror：{err:?}"
    );
    assert!(!err.contains("Usage"), "这一支不接 usage：{err:?}");

    let (_, status, err) = run(&["-"], INPUT);
    assert_eq!(status, 1, "stderr 是 {err:?}");
    assert_eq!(
        err, "unexpand: Cannot open `-': No such file or directory\n",
        "裸 `-` 不是 stdin 的别名"
    );
}

/// 输出腿（本闸门存在的另一半）：以前 `unexpand` 用裸 `open`/`read`，宿主上读不到输入，
/// 所以转换从来没有被进程级测过。金样逐形取自参照件读数（`tmp/unexpand-312/golden.log`）。
#[test]
fn the_tabify_reaches_the_output_end_to_end() {
    // 默认八列档：八个空格折一个制表符，四个空格留原样。
    let (out, status, err) = run(&[], INPUT);
    assert_eq!(
        (out.as_str(), err.as_str(), status),
        ("\tx\n    y\n", "", 0)
    );
    // `--` 结束选项环且自身不是操作数。
    let (out, status, _) = run(&["--"], INPUT);
    assert_eq!((out.as_str(), status), ("\tx\n    y\n", 0));
    // 无 `-a` 时行中的空白不动；`-a` 才折行中的空白段。
    let (out, status, _) = run(&["-a"], b"   a   b   c\n");
    assert_eq!(
        (out.as_str(), status),
        ("   a   b   c\n", 0),
        "八个以内不跨档"
    );
    let (out, _, _) = run(&["-a"], b"ab        c\n");
    assert_eq!(out, "ab\t  c\n");
    // 末行没有换行也不补（参照件：输入 `   ab` 出去仍是无换行的一行）。
    let (out, status, err) = run(&[], b"   ab");
    assert_eq!((out.as_str(), err.as_str(), status), ("   ab", "", 0));
    // 多支档名依次处理并重置行游标（参照件：两档合一输出）。
    let dir = scratch();
    let first = dir.join("a");
    let second = dir.join("b");
    fs::write(&first, b"aaa\n").expect("write first");
    fs::write(&second, INPUT).expect("write second");
    let (out, status, err) = run(&[first.to_str().unwrap(), second.to_str().unwrap()], b"");
    assert_eq!(
        (out.as_str(), err.as_str(), status),
        ("aaa\n\tx\n    y\n", "", 0),
        "两支档名该依次转换"
    );
    let _ = fs::remove_dir_all(dir);
}

fn scratch() -> std::path::PathBuf {
    static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("unexpand-face-{}-{}", std::process::id(), seq));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

/// 参照件 `tmp/unexpand-312/unexpand_ref` 现量（矩阵 `tmp/unexpand-425/ref-matrix.log`）：
/// 档位为 `0` 不是一支「坏档位」，C 的 `strtoul` 收下它，`limit` 与两处 `tabstops[n] - 1`
/// 都是 `size_t` 回绕，于是整行原样交回、退 0，而档位表照 `unexpand.c:124-125` 打到
/// 标准错误。本壳以前在这里崩掉（`src/unexpand.rs` 里三处无符号减法在回绕前先触 trap）。
#[test]
fn a_zero_tabstop_reaches_the_line_unchanged() {
    let (out, status, err) = run(&["-t", "0"], b"a\tb\tc\n");
    assert_eq!(
        (out.as_str(), status, err.as_str()),
        ("a\tb\tc\n", 0, "0 0\n")
    );
    let (out, status, err) = run(&["-t", "0,8"], b"    abc\n");
    assert_eq!(
        (out.as_str(), status, err.as_str()),
        ("    abc\n", 0, "0 0\n1 8\n")
    );
    // 无尾换行的输入也照 C 交回（`fgetln` 只给输入里有的字节）。
    let (out, status, err) = run_bytes(&["-t", "0"], b"x\t y");
    assert_eq!(
        (out, status, err),
        (b"x\t y"[..].to_vec(), 0, b"0 0\n".to_vec())
    );
}

/// 参照件 `tmp/unexpand-312/unexpand_ref` 现量（读数 `tmp/unexpand-426/ref-shapes.log`
/// 与 `tmp/unexpand-426/eol.log`）：制表符够不着下一档时它被原样交回；末行只剩空白且
/// 输入没有换行时参照件交出**零字节**（C 的循环走到缓冲区末尾直接 `return`）。
/// 带换行的那一半（`b"    \n"` 对 `\t\n`）属已登记的
/// `UNEXPAND-BLANK-LINE-END-FLUSH-NEEDS-TERMINATOR`，本轮不钉。
#[test]
fn a_tab_past_the_last_stop_and_a_blank_final_line_reach_stdout() {
    let (out, status, err) = run(&["-t", "8"], b"  \tx\n");
    assert_eq!(
        (out.as_str(), status, err.as_str()),
        ("  \tx\n", 0, "0 8\n")
    );
    let (out, status, err) = run(&["-t", "8"], b"        \ty\n");
    assert_eq!(
        (out.as_str(), status, err.as_str()),
        ("\t\ty\n", 0, "0 8\n")
    );
    let (out, status, err) = run(&["-t", "4,8"], b"    \ty\n");
    assert_eq!(
        (out.as_str(), status, err.as_str()),
        ("\t\ty\n", 0, "0 4\n1 8\n")
    );
    let (out, status, err) = run_bytes(&["-t", "8"], b"        ");
    assert_eq!((out, status, err), (Vec::new(), 0, b"0 8\n".to_vec()));
    let (out, status, err) = run_bytes(&[], b"   ");
    assert_eq!((out, status, err), (Vec::new(), 0, Vec::new()));
    let (out, status, err) = run_bytes(&["-t", "8"], b"\t");
    assert_eq!((out, status, err), (b"\t".to_vec(), 0, b"0 8\n".to_vec()));
}
