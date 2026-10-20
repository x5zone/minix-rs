//! End-to-end face for the `fold` binary, checked against
//! `minix3/usr.bin/fold/fold.c`（选项环 `:76-101`、操作数循环 `:103-110`、
//! `usage()` `:115-121`）与它驱动的 libc `getopt`
//!（`minix3/lib/libc/stdlib/getopt.c:101-104`、`:132-136`）。
//!
//! C 的选项串是 `"0123456789bsw:"`：只有 `w` 带值，`0-9` 是遗留宽度形的**合法字母**，
//! 选项串不以 `:` 开头所以 `opterr` 生效。于是三条出口的**行数不同**，这是本枚门要钉的：
//! ① getopt 报的两支（集外字母、`-w` 缺值）= libc 一行 + usage 一行；
//! ② `atoi` 给出非正宽度 = 命令自己的 `errx(1, "illegal width value")`，**只有那一行**；
//! ③ `usage()` 的程序名跟着 `argv[0]`（`getprogname()`），退码 1。
//!
//! 这扇门前不存在：`fold` 的读缝直接拿裸 `open`/`read`，宿主上读不到任何输入，
//! 所以以前只能靠库测（同批次三百零六给 `nl` 收的那处）。

mod face_support;

use face_support::write_stdin;
use std::ffi::OsString;
use std::fs;
use std::os::unix::ffi::OsStringExt;
use std::process::{Command, Stdio};

const FOLD: &str = env!("CARGO_BIN_EXE_fold");
const USAGE: &str = "usage: fold [-bs] [-w width] [file ...]\n";

fn run(args: &[&str], input: &[u8]) -> (String, i32, String) {
    let mut child = Command::new(FOLD)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the fold binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// 集外字母：libc 那一行在前，`usage()` 在后，退 1（`fold.c:99-101` 的 `default`）。
#[test]
fn an_unknown_letter_prints_the_libc_line_then_the_usage() {
    let (out, status, err) = run(&["-Z"], b"abc");
    assert_eq!(out, "", "报文只走 stderr");
    assert_eq!(status, 1, "stderr 是 {err:?}");
    assert_eq!(err, format!("fold: unknown option -- Z\n{USAGE}"));
}

/// 与 `run` 同一条命令缝，但**逐字节**交回：`from_utf8_lossy` 把一个裸 `c3` 渲染成三字节
/// U+FFFD，文本形分不清「一个字节」与「两个字节」这两种错法（`rev`/`paste`/`jot` 那几支同此）。
fn run_bytes(args: &[&str], input: &[u8]) -> (Vec<u8>, i32, Vec<u8>) {
    let mut child = Command::new(FOLD)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the fold binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        output.stdout,
        output.status.code().unwrap_or(-1),
        output.stderr,
    )
}

/// 集外字母那一行的字母位是**一个裸字节**：参照件 `tmp/fold-405/fold_ref`（`fold.c` 真源 +
/// Minix 的 `getopt.c` + 程序名垫片，本轮新配）现量 `-é` 与 `-bé` 都是
/// `fold: unknown option -- ` + `c3` + 换行再跟 usage、退 1（`fold.c:76` 的选项串是
/// `"0123456789bsw:"`，`b` 是合法字母且不带值，所以聚簇继续吃到 `é` 的第一字节；`-Zé` 停在 `Z`，
/// `fold.c:99-101` 的 `default` 直接 `usage()`，报文不累加）。
/// ASCII 的 `-Z` 本文件已有主人（`an_unknown_letter_prints_the_libc_line_then_the_usage`），
/// 按冗余判据不重复承载。
#[test]
fn the_libc_line_of_a_multibyte_flag_folds_to_one_byte() {
    let want: Vec<u8> = [
        b"fold: unknown option -- ".as_slice(),
        b"\xc3\n",
        USAGE.as_bytes(),
    ]
    .concat();
    for form in ["-\u{e9}", "-b\u{e9}"] {
        let (out, status, err) = run_bytes(&[form], b"abc");
        assert_eq!(out, Vec::new(), "{form} 的报文只走标准错误");
        assert_eq!(status, 1, "{form} 的 stderr 是 {err:?}");
        assert_eq!(err, want, "{form} 的字母该是一个裸字节 c3");
    }

    let (_out, status, err) = run_bytes(&["-Z\u{e9}"], b"abc");
    assert_eq!(status, 1);
    assert_eq!(
        err,
        [b"fold: unknown option -- Z\n".as_slice(), USAGE.as_bytes(),].concat(),
        "环停在第一个坏字母上，`é` 根本不该被读到"
    );
}

/// 参数缝按字节交回之后，聚簇里那一枚**不是合法 UTF-8 前缀**的字母位也只剩一个字节。
/// 上一枚门用的是合法序列 `c3 a9`，lossy 不改它，所以旧缝也能过；这一枚用的是裸 `ff`：
/// 旧的 `String::from_utf8_lossy` 缝把它渲染成三字节 U+FFFD，环取到的第一字节就成了
/// `ef`，比参照件多一位。`-b` 是合法字母且不带值（选项串 `"0123456789bsw:"`），环继续
/// 吃到下一位，所以这一形走的是聚簇腿，与跨壳表那行的单字母腿不是同一条缝。
#[test]
fn a_raw_nonutf8_letter_after_a_cluster_flag_is_one_byte() {
    let want: Vec<u8> = [
        b"fold: unknown option -- ".as_slice(),
        b"\xff\n",
        USAGE.as_bytes(),
    ]
    .concat();
    let (out, status, err) = run_raw(&[b"-b\xff".to_vec()], b"abc");
    assert_eq!(out, Vec::new(), "报文只走 stderr");
    assert_eq!(status, 1, "stderr 是 {err:?}");
    assert_eq!(err, want, "字母该是原字节 ff，不是 lossy 的第一字节 ef");
}

/// 同 `run_bytes`，但参数交回**原始字节**：`&str` 夹具写不出裸 `ff`，
/// 写成 `\u{fffd}` 会被编码成 `ef bf bd`，测的就不是这一位了。
fn run_raw(args: &[Vec<u8>], input: &[u8]) -> (Vec<u8>, i32, Vec<u8>) {
    let mut child = Command::new(FOLD)
        .args(args.iter().map(|word| OsString::from_vec(word.clone())))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the fold binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        output.stdout,
        output.status.code().unwrap_or(-1),
        output.stderr,
    )
}

/// `-w` 后面没有值是 libc 的另一句；与「值不是正数」那句**不同形**，也不同行数。
#[test]
fn a_missing_width_value_prints_the_libc_line_then_the_usage() {
    let (out, status, err) = run(&["-w"], b"abc");
    assert_eq!(out, "");
    assert_eq!(status, 1, "stderr 是 {err:?}");
    assert_eq!(
        err,
        format!("fold: option requires an argument -- w\n{USAGE}")
    );
}

/// `atoi` 吃出的非正宽度走命令自己的 `errx`：只有那一行，**不接 usage**，退 1
/// （`fold.c:84-86`）。两种写法（字母与数字）都得同一句。
#[test]
fn a_nonpositive_width_is_one_line_without_the_usage() {
    // 只钉带值形那两支。遗留的 `-0` 在 C 里**不报错**：digit case 把 width 设成 0 后
    // 直接过（`fold.c:91-98` 之后只有 `if (width == -1)` 的兜底），本壳按正整数校验拒收，
    // 那一形已作为声明差登记，不在这里当 C 的形状签字。
    for args in [vec!["-w", "0"], vec!["-w", "-3"]] {
        let (out, status, err) = run(&args, b"abc");
        assert_eq!(out, "", "{args:?}  printed {out:?}");
        assert_eq!(status, 1, "{args:?} 的 stderr 是 {err:?}");
        assert_eq!(err, "fold: illegal width value\n", "{args:?}");
    }
}

/// 宽度的三种给法都算：默认 80、`-w N`、遗留的 `-N` 与聚簇里的数字（`-b9`）。
/// 聚簇数字那形在 C 里取 `argv[optind - 1]`，结果跟着 getopt 内部位置走，
/// 没参照件可取证，本壳按「从该位置的数字串作宽度」处理并登记为声明差（见 18 篇台账）；
/// 这里钉的是它**不被当集外字母**的那一半。
#[test]
fn the_width_can_be_given_three_ways() {
    let (out, _, _) = run(&[], b"abcdefghij");
    assert_eq!(out, "abcdefghij\n", "默认宽 80 不折");
    let (out, _, err) = run(&["-9"], b"abcdefghij");
    assert_eq!((out.as_str(), err.as_str()), ("abcdefghi\nj\n", ""));
    let (out, _, err) = run(&["-w", "4"], b"abcdefghij");
    assert_eq!((out.as_str(), err.as_str()), ("abcd\nefgh\nij\n", ""));
    let (out, _, err) = run(&["-b9"], b"abcdefghij");
    assert_eq!(
        (out.as_str(), err.as_str()),
        ("abcdefghi\nj\n", ""),
        "聚簇里的数字不该被当成集外字母"
    );
}

/// 操作数按 C 的 `for (; *argv; ++argv) freopen(...) … fold(width)`：每个文件**重启**
/// 列状态，所以两个短文件不会拼成一条宽行。
#[test]
fn each_operand_restarts_the_column_state() {
    let dir = std::env::temp_dir().join(format!("fold-face-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("scratch dir");
    let one = dir.join("one");
    let two = dir.join("two");
    fs::write(&one, b"abc").expect("write one");
    fs::write(&two, b"de").expect("write two");
    let (out, status, err) = run(
        &["-w", "4", one.to_str().unwrap(), two.to_str().unwrap()],
        b"",
    );
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(out, "abc\nde\n", "两个文件各自从第一列起：{out:?}");
    let _ = fs::remove_dir_all(dir);
}

/// 读不到的操作数是 C 的 `err(1, "%s", *argv)`：程序名、操作数、`strerror` 句子
/// 三段齐备后退出（`fold.c:107-109`）。旧写法只打前两段。
#[test]
fn an_unopenable_operand_carries_the_number_sentence() {
    let missing = format!("/tmp/fold-face-definitely-absent-{}", std::process::id());
    let (out, status, err) = run(&[missing.as_str()], b"abc");
    assert_eq!(out, "", "报文只走 stderr");
    assert_eq!(status, 1, "stderr 是 {err:?}");
    let head = format!("fold: {missing}: ");
    assert!(
        err.starts_with(&head),
        "该以 `fold: <操作数>: ` 开头，实际 {err:?}"
    );
    let sentence = err[head.len()..].trim_end();
    assert!(
        !sentence.is_empty() && sentence.chars().any(|c| c.is_lowercase()),
        "缀的应是一句 strerror 文本而不是代号：{sentence:?}"
    );
}
