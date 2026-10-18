//! End-to-end face for the `rev` binary, checked against
//! `minix3/usr.bin/rev/rev.c`（选项环 `:70-75`、`argv += optind` `:77-78`、
//! 操作数循环与 `warn` `:83-106`、`usage()` `:110-115`）与它驱动的 libc `getopt`
//!（`minix3/lib/libc/stdlib/getopt.c:74-78`、`:81-92`、`:95-104`）。
//!
//! C 的 optstring 是**空串**，这一条容易被读成「没有选项环」，于是本壳以前把 `-Z`
//! 当文件操作数。参照件 `tmp/rev-309/rev_ref`（`rev.c` 真源 + Minix 的 `getopt.c` +
//! `fgetwln`/`setprogname` 垫片，宿主现链）逐形量出的形状是：
//! ① 任何 `-` 开头的 token（除 `--` 与裸 `-`）= libc 一行 + usage 一行，退 1；
//! ② `--` 结束选项环且自身不算操作数，裸 `-` 则是操作数；
//! ③ 不重排：非选项 token 之后的 `-Z` 归操作数；
//! ④ 打不开的操作数是 `warn(3)` 三段形、**非致命**（继续做后面的文件，退码 1）。
//!
//! 这扇门前不存在：`rev` 的读缝直接拿裸 `open`/`read`，宿主上读不到任何输入，
//! 所以它的输出以前只能靠库测（同批次三百零六的 `nl`、三百零八的 `fold`）。

mod face_support;

use face_support::write_stdin;
use std::fs;
use std::process::{Command, Stdio};

const REV: &str = env!("CARGO_BIN_EXE_rev");
const USAGE: &str = "usage: rev [file ...]\n";

fn run(args: &[&str], input: &[u8]) -> (String, i32, String) {
    let mut child = Command::new(REV)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the rev binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// 同上，但三条通道交回**原始字节**。
///
/// 报文里要钉住一个裸字节时只能用这一条：`String::from_utf8_lossy` 会把 `c3` 和
/// `c3 83` 都渲染成替换字符 U+FFFD，上面那个夹具区分不了这两种错法。
fn run_bytes(args: &[&str], input: &[u8]) -> (Vec<u8>, i32, Vec<u8>) {
    let mut child = Command::new(REV)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the rev binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        output.stdout,
        output.status.code().unwrap_or(-1),
        output.stderr,
    )
}

/// 集外字母：libc 那一行在前、`usage()` 在后，退 1（`rev.c:72-74` 的 `case '?': default`）。
/// 参照件读数 `rev: unknown option -- Z` + `usage: rev [file ...]`。
#[test]
fn an_unknown_letter_prints_the_libc_line_then_the_usage() {
    let (out, status, err) = run(&["-Z"], b"abc");
    assert_eq!(out, "", "报文只走 stderr");
    assert_eq!(status, 1, "stderr 是 {err:?}");
    assert_eq!(err, format!("rev: unknown option -- Z\n{USAGE}"));
}

/// 聚簇只报**第一个**字符：调用者拿到 `'?'` 就直接 `usage()` 退出，不会回到
/// getopt 去读第二个字母（参照件：`-bZ` → `unknown option -- b`）。
#[test]
fn a_cluster_reports_only_its_first_letter() {
    let (out, status, err) = run(&["-bZ"], b"abc");
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(err, format!("rev: unknown option -- b\n{USAGE}"));
}

/// 非 ASCII 的集外字母打的是**一个裸字节**：参照件 `tmp/rev-309/rev_ref` 现量
/// `rev -é` → `rev: unknown option -- ` + `c3` + 换行，再一行 usage，退 1。
/// 本壳在这一形上错过两次：按字符取时报 `c3 a9`（整个 `é`），按字节取但仍用 `char`
/// 渲染时报 `c3 83`（U+00C3 的 UTF-8）——两种都比 C 多一个字节，而 lossy 的 `String`
/// 夹具把这两种都读成替换字符，所以这一枚逐字节比 stderr。
/// 只有一行 libc 报文也是断言的一部分：`rev.c:72-74` 拿到 `'?'` 就退出，
/// 不会像 `getopt` 的累加型调用者那样把 `a9` 再报一次。
#[test]
fn an_unknown_nonascii_letter_prints_one_raw_byte() {
    let want: Vec<u8> = [
        b"rev: unknown option -- ".as_slice(),
        b"\xc3\n",
        USAGE.as_bytes(),
    ]
    .concat();
    let (out, status, err) = run_bytes(&["-\u{e9}"], b"abc");
    assert_eq!(out, Vec::new(), "报文只走 stderr");
    assert_eq!(status, 1, "stderr 是 {err:?}");
    assert_eq!(err, want, "字母该是一个裸字节 c3");

    // 聚簇里非 ASCII 排在第二位：报错仍落在第一个字节上，且仍是**一行**。
    let (out, status, err) = run_bytes(&["-x\u{e9}"], b"abc");
    assert_eq!(out, Vec::new());
    assert_eq!(status, 1);
    assert_eq!(
        err,
        b"rev: unknown option -- x\nusage: rev [file ...]\n".to_vec()
    );
}

/// `--x` 报的字母是第二个连字符：optstring 里没有 `-`，所以它也是集外字符
/// （参照件：`unknown option -- -`）。而纯 `--` 走的是另一支——结束选项环，不报错。
#[test]
fn a_dash_followed_by_a_word_is_an_unknown_option_named_by_the_dash() {
    let (out, status, err) = run(&["--x"], b"abc");
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(err, format!("rev: unknown option -- -\n{USAGE}"));
}

/// `--` 自身被吃掉、后面的都算操作数（`getopt.c:81-85` 先 `++optind`）：
/// 单给 `--` 仍是读标准输入，`-- <file>` 则改读那个文件。
#[test]
fn the_end_of_options_marker_is_not_an_operand() {
    let (out, status, err) = run(&["--"], b"abc\ndefg\n");
    assert_eq!((out.as_str(), err.as_str(), status), ("cba\ngfed\n", "", 0));
    let dir = scratch();
    let file = dir.join("in");
    fs::write(&file, b"xy\nz\n").expect("write operand");
    let (out, status, err) = run(&["--", file.to_str().unwrap()], b"");
    assert_eq!(
        (out.as_str(), err.as_str(), status),
        ("yx\nz\n", "", 0),
        "`--` 后面的具名文件该被读"
    );
    let _ = fs::remove_dir_all(dir);
}

/// 裸 `-` 是操作数而不是选项：optstring 里没有 `-`，getopt 结束选项环时**不推进**
/// `optind`（`getopt.c:87-92`），于是这个 token 落进 `argv += optind` 的那一段，
/// 走到 C 的 `fopen("-")` → `warn` 支（参照件：`rev: -: No such file or directory`，
/// **一行、没有 usage**、退 1）。
#[test]
fn a_solitary_dash_is_an_operand_and_warns_in_the_err3_shape() {
    let (out, status, err) = run(&["-"], b"abc");
    assert_eq!(out, "", "`-` 不是「读标准输入」的别名");
    assert_eq!(status, 1, "stderr 是 {err:?}");
    let head = "rev: -: ";
    assert!(
        err.starts_with(head),
        "该以 `rev: -: ` 开头（程序名 + 操作数），实际 {err:?}"
    );
    let line = err[head.len()..].trim_end();
    assert!(
        !line.is_empty() && !line.contains('\n'),
        "缀的应是一句 strerror 文本且只有这一行：{err:?}"
    );
    assert!(!err.contains("usage:"), "操作数的 warn 不接 usage：{err:?}");
}

/// 不重排：非选项 token 一到，选项环就结束（`getopt.c:74-78`），所以
/// `rev <file> -Z` 里的 `-Z` 是个**文件名**（参照件给它 warn 而照常处理前一个文件）。
#[test]
fn an_option_looking_token_after_an_operand_is_an_operand() {
    let dir = scratch();
    let file = dir.join("in");
    fs::write(&file, b"abc\n").expect("write operand");
    let (out, status, err) = run(&[file.to_str().unwrap(), "-Z"], b"");
    assert_eq!(out, "cba\n", "具名文件该被处理：{out:?}");
    assert_eq!(status, 1, "stderr 是 {err:?}");
    assert!(
        err.starts_with("rev: -Z: ") && !err.contains("usage:"),
        "`-Z` 该按打不开的操作数报 warn 三段形，而不是选项错：{err:?}"
    );
    let _ = fs::remove_dir_all(dir);
}

/// 打不开的操作数**非致命**（`rev.c:85-89` 是 `warn` + `++argv` + `continue`）：
/// 坏文件报一行、好文件照做、退码 1。旧写法两处都不合：句子少了 `strerror`，
/// 而且没有选项环。
#[test]
fn an_unopenable_operand_warns_and_the_run_continues() {
    let dir = scratch();
    let good = dir.join("good");
    fs::write(&good, b"abc\ndefg\n").expect("write good");
    let missing = dir.join("absent");
    let (out, status, err) = run(&[missing.to_str().unwrap(), good.to_str().unwrap()], b"");
    assert_eq!(out, "cba\ngfed\n", "后面的好文件该照做：{out:?}");
    assert_eq!(status, 1, "stderr 是 {err:?}");
    let head = format!("rev: {}: ", missing.to_str().unwrap());
    assert!(
        err.starts_with(&head),
        "该以 `rev: <操作数>: ` 开头，实际 {err:?}"
    );
    assert_eq!(
        err.lines().count(),
        1,
        "只报 warn 那一行，不接 usage：{err:?}"
    );
    let _ = fs::remove_dir_all(dir);
}

/// 输出腿（本闸门存在的另一半）：以前 `rev` 用裸 `open`/`read`，宿主上**读不到输入**，
/// 所以这三形从来没有被进程级测过。金样逐形对参照件（`tmp/rev-309/golden.log`）。
#[test]
fn the_reversal_reaches_the_output_end_to_end() {
    let (out, status, err) = run(&[], b"abc\ndefg\n");
    assert_eq!((out.as_str(), err.as_str(), status), ("cba\ngfed\n", "", 0));
    // 末行没有换行也照反，并补一个换行（`rev.c:94-99`）。
    let (out, status, _) = run(&[], b"abc");
    assert_eq!((out.as_str(), status), ("cba\n", 0));
    // 空输入不产出行。
    let (out, status, _) = run(&[], b"");
    assert_eq!((out.as_str(), status), ("", 0));
    // 字符级反转：多字节字符不被撕裂（C 走的是宽字符，同一保证）。
    let (out, _, _) = run(&[], "aé中b\n".as_bytes());
    assert_eq!(out, "b中éa\n");
}

/// 标准输入读不出内容时，C 报的是**流名**而不是操作数：`rev.c:80-81` 把 `filename`
/// 起成 `"stdin"`，`:101-104` 的 `if (ferror(fp)) warn("%s", filename)` 于是印
/// `rev: stdin: <strerror>` 并把退码置 1。宿主上最省事的读错误承载是让标准输入
/// 是一个目录的文件描述符：打开成功、读的时候拿 `EISDIR`。参照件在同一形上的读数
/// （`tmp/rev-331/measure.log`）：`rev_ref: stdin: Is a directory`、退 1、无输出。
/// 本壳以前把读失败当文件末尾，于是这一形**什么都不说还退 0**。
/// `Is a directory` 那句取自宿主 `strerror` 表（真机读库腿归 `C-57`/E5）。
#[test]
fn a_stdin_that_cannot_be_read_names_the_stream_and_exits_one() {
    let dir = scratch();
    let stream = dir.join("not-a-stream");
    fs::create_dir(&stream).expect("create the directory to hand over as stdin");
    let handle = fs::File::open(&stream).expect("a directory opens read-only here");
    let output = Command::new(REV)
        .stdin(Stdio::from(handle))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the rev binary should be runnable")
        .wait_with_output()
        .expect("the run should finish");
    let _ = fs::remove_dir_all(&dir);
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "",
        "读不到内容就不该有输出"
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "rev: stdin: Is a directory\n",
        "报的该是流名 `stdin`，一行、三段形"
    );
    assert_eq!(
        output.status.code(),
        Some(1),
        "读失败是致命的吗？不，但要把退码置起来"
    );
}

/// 同一条 `warn("%s", filename)` 的另一半：操作数顶掉了 `filename` 的初值
/// （`rev.c:91`），所以读一个打不开/读不出的操作数时报的是**那个操作数**而不是
/// `stdin`。这一形本壳已经对（走 `read_file` 的错误通道），留在这里是为了让上一枚
/// 门钉的「流名跟着流走」有对照，不至于把两件事混成一句泛化的报文。
#[test]
fn an_unreadable_operand_names_itself_rather_than_stdin() {
    let dir = scratch();
    let stream = dir.join("directory-operand");
    fs::create_dir(&stream).expect("create the directory operand");
    let (out, status, err) = run(&[stream.to_str().unwrap()], b"");
    let _ = fs::remove_dir_all(&dir);
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(
        err,
        format!("rev: {}: Is a directory\n", stream.to_str().unwrap())
    );
}

fn scratch() -> std::path::PathBuf {
    // 每枚门各拿一个目录：这些门结尾都要 `remove_dir_all`，而 cargo 是多线程跑测试的，
    // 共用同一个按进程号命名的目录就会让先跑完的那枚删掉别人正在用的档。
    static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("rev-face-{}-{}", std::process::id(), seq));
    fs::create_dir_all(&dir).expect("scratch dir");
    dir
}
