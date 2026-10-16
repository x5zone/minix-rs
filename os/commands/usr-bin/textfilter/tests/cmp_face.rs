//! End-to-end face for the `cmp` binary, checked against
//! `minix3/usr.bin/cmp/cmp.c`（选项环 `:74-86`、`-l`/`-s` 互斥 `:87-90`、
//! 操作数个数 `:92-93`、开档支 `:95-119`、skip 解析 `:121-134`、`usage()` `:158-164`）
//! 与它驱动的 libc `getopt`（`minix3/lib/libc/stdlib/getopt.c:74-92`、`:95-104`）。
//!
//! `cmp.c:74` 的 optstring 是 `"ls"`：两个字母**都不带值**，所以 libc 的
//! `option requires an argument` 那一支在本支根本到不了（可达出口由 optstring 定，
//! 不按同族规律补）。参照件 `tmp/cmp-316/cmp_ref`（`cmp.c` + `misc.c` + `regular.c` +
//! `special.c` 真源 + Minix 的 `getopt.c` + 程序名垫片，宿主现链）逐形量出的形状是：
//! ① 集外字母 = libc 一行 + usage 一行，退 **2**（`-q`、聚簇 `-lq` 报 `q`、`--x` 报 `-`）；
//! ② `-l` 与 `-s` 同时给（含聚簇 `-ls`、`-sl`）= `cmp: only one of -l and -s may be specified`
//!    **单独一行、不接 usage**，退 2（`errx` 两支形）；
//! ③ 操作数少于两个或多于四个 = **只有 usage 一行**，退 2；
//! ④ 开不了的档名 = `warn("%s", name)` 三段形、退 2，而且 **`-s` 之下整句不打**
//!    （`cmp.c:101-104`、`:113-117` 的 `if (!sflag)`）；
//! ⑤ 环不重排：`cmp 档1 -l 档2` 是**三个操作数**，那个 `-l` 是第二个档名
//!    （参照件现量 `cmp: -l: No such file or directory`），旧写法把它当旗标吃掉后再比两个档；
//! ⑥ 裸 `-` 是标准输入的别名，而**两个操作数都能写 `-` 一次**——两边都写时
//!    `errx(ERR_EXIT, "standard input may only be specified once")`；
//! ⑦ 报文里出现的是**操作数原文**（`cmp.c:96` 把 `file1` 改写成 `"stdin"` 只影响后面的
//!    报表，不影响这一行），退出码三档：0 相同、1 有差异、2 出错（`ERR_EXIT`）。
//!
//! skip 那一段（`strtoq(…, 0)`）也是量出来的：`5x` 吃 5（尾巴不管）、`010` 是八进制的 8、
//! `0x2` 是 2、`-1` 合法（越界后落 `EOF on <file>` 那一支，且**不带** char/line 后缀），
//! 而 `x`、空串、溢出各落 usage。

mod face_support;

use face_support::write_stdin;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const CMP: &str = env!("CARGO_BIN_EXE_cmp");
/// `cmp.c:161-162` 那一行：程序名写死在串里。
const USAGE: &str = "usage: cmp [-l | -s] file1 file2 [skip1 [skip2]]\n";

/// 与参照件同一套档名与内容（放在临时目录里，命令以该目录为工作目录跑，
/// 这样报文里的档名就是可逐字比对的短名）。
/// 报表里的档名标签：`-` 那一侧写成 `stdin`（`cmp.c:96`、`:111` 把 `file1`/`file2`
/// 改写成 `"stdin"`），另一侧仍是操作数原文。
/// 再一件是**次序**：C 先开两个档、后解析 skip（`cmp.c:95-119` 在 `:121-134` 之前），
/// 所以「档开不了 + skip 又是坏值」时打的是开档那一句，`usage:` 根本不会出现。
#[test]
fn the_stdin_label_wins_in_the_report_and_the_open_failure_wins_over_a_bad_skip() {
    let dir = workspace("label");
    fs::write(dir.join("b2.txt"), b"abd\n").expect("write b2");
    let (out, status, err) = run(&dir, &["-", "b2.txt"], b"abc\n");
    assert_eq!(
        (out.as_str(), status, err.as_str()),
        ("stdin b2.txt differ: char 3, line 1\n", 1, "")
    );

    let (out, status, err) = run(&dir, &["nope.txt", "same2.txt", "x"], b"");
    assert_eq!((out.as_str(), status), ("", 2));
    assert_eq!(err, "cmp: nope.txt: No such file or directory\n");
    assert!(
        !err.contains("usage:"),
        "开档失败该压过坏 skip：stderr 是 {err:?}"
    );
}

fn workspace(tag: &str) -> PathBuf {
    static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("cmp-face-{}-{tag}-{n}", std::process::id()));
    fs::create_dir_all(&dir).expect("scratch dir");
    fs::write(dir.join("same1.txt"), b"abcde\n").expect("write same1");
    fs::write(dir.join("same2.txt"), b"abcde\n").expect("write same2");
    fs::write(dir.join("diff2.txt"), b"abXde\n").expect("write diff2");
    fs::write(dir.join("short3.txt"), b"abc\n").expect("write short3");
    fs::write(dir.join("empty.txt"), b"").expect("write empty");
    dir
}

fn run(dir: &Path, args: &[&str], input: &[u8]) -> (String, i32, String) {
    let mut child = Command::new(CMP)
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the cmp binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// 集外字母：libc 那一行在前、usage 在后，退 2（旧写法少了 libc 那一行）。
#[test]
fn an_unknown_letter_prints_the_libc_line_then_the_usage() {
    let dir = workspace("unknown");
    let (out, status, err) = run(&dir, &["-q", "same1.txt", "same2.txt"], b"");
    assert_eq!((out.as_str(), status), ("", 2), "stderr 是 {err:?}");
    assert_eq!(err, format!("cmp: unknown option -- q\n{USAGE}"));

    // 聚簇只报第一个集外字母（前面的合法字母不改变出口）。
    let (_, status, err) = run(&dir, &["-lq", "same1.txt", "same2.txt"], b"");
    assert_eq!(status, 2);
    assert_eq!(err, format!("cmp: unknown option -- q\n{USAGE}"));

    // `--x` 报的字母是第二个连字符：optstring 里没有 `-`。
    let (_, status, err) = run(&dir, &["--x", "same1.txt", "same2.txt"], b"");
    assert_eq!(status, 2);
    assert_eq!(err, format!("cmp: unknown option -- -\n{USAGE}"));
}

/// `-l` 与 `-s` 同时给是一句 `errx`：程序名 + 句子 + 句尾**没有**句点，
/// 而且**不接 usage**（旧写法把聚簇形判成集外字母，打的是两行的 usage）。
#[test]
fn the_two_flags_conflict_in_one_line_without_the_usage() {
    let dir = workspace("conflict");
    for word in ["-ls", "-sl"] {
        let (out, status, err) = run(&dir, &[word, "same1.txt", "same2.txt"], b"");
        assert_eq!(
            (out.as_str(), status),
            ("", 2),
            "`{word}` 的 stderr 是 {err:?}"
        );
        assert_eq!(
            err, "cmp: only one of -l and -s may be specified\n",
            "`{word}` 在 C 是逐字母聚簇，两面旗标都立起来才落这一支"
        );
    }
    // 分两支写同样冲突（C 只是再赋一次 1，所以 `-l -l` 反而合法——见下一枚）。
    let (_, status, err) = run(&dir, &["-l", "-s", "same1.txt", "same2.txt"], b"");
    assert_eq!(status, 2);
    assert_eq!(err, "cmp: only one of -l and -s may be specified\n");
}

/// 操作数少于两个或多于四个：只有 usage 一行、退 2（参照件现量三种形都同）。
#[test]
fn an_operand_count_out_of_range_is_the_usage_line_only() {
    let dir = workspace("count");
    for args in [
        vec![],
        vec!["same1.txt"],
        vec!["same1.txt", "same2.txt", "0", "0", "0"],
    ] {
        let (out, status, err) = run(&dir, &args, b"");
        assert_eq!((out.as_str(), status), ("", 2), "args 是 {args:?}");
        assert_eq!(err, USAGE);
    }
}

/// 重复给同一面旗标是合法的（C 就是再赋一次 1），`-l -l` 与 `-l` 同读数。
#[test]
fn a_repeated_flag_stays_legal_and_idempotent() {
    let dir = workspace("repeat");
    let (out, status, err) = run(&dir, &["-l", "-l", "same1.txt", "diff2.txt"], b"");
    assert_eq!((out.as_str(), status), ("     3 143 130\n", 1));
    assert_eq!(err, "");
}

/// 环不重排：`same1.txt` 一到，环就结束，那个 `-l` 是**第二个档名**（参照件现量
/// `cmp: -l: No such file or directory`、退 2）。旧写法扫完全程，把 `-l` 当旗标吃掉后
/// 比两个档，退码与句子全不同。
#[test]
fn the_option_ring_stops_at_the_first_operand() {
    let dir = workspace("reorder");
    let (out, status, err) = run(&dir, &["same1.txt", "-l", "same2.txt"], b"");
    assert_eq!((out.as_str(), status), ("", 2));
    assert_eq!(err, "cmp: -l: No such file or directory\n");

    // `--` 结束环而自身不是操作数：`-- same1 same2` 与不带 `--` 同读数。
    let (out, status, err) = run(&dir, &["--", "same1.txt", "same2.txt"], b"");
    assert_eq!((out.as_str(), status), ("", 0));
    assert_eq!(err, "");
}

/// 开不了的档名是三段形 `warn(3)`，而 `-s` 之下**整句不打**（`cmp.c:101-104` 的
/// `if (!sflag)`）——退出码仍是 2。
#[test]
fn an_unopenable_operand_warns_in_three_parts_and_silence_suppresses_it() {
    let dir = workspace("open");
    let (out, status, err) = run(&dir, &["nope.txt", "same2.txt"], b"");
    assert_eq!((out.as_str(), status), ("", 2));
    assert_eq!(err, "cmp: nope.txt: No such file or directory\n");

    let (out, status, err) = run(&dir, &["-s", "nope.txt", "same2.txt"], b"");
    assert_eq!(
        (out.as_str(), status, err.as_str()),
        ("", 2, ""),
        "`-s` 把开档失败的那一句整个压掉，只留退出码"
    );

    // 第二个档名开不了也一样，且报文里是它自己的名字。
    let (out, status, err) = run(&dir, &["same1.txt", "nope2.txt"], b"");
    assert_eq!((out.as_str(), status), ("", 2));
    assert_eq!(err, "cmp: nope2.txt: No such file or directory\n");
}

/// 裸 `-` 是标准输入的别名；两边都写 `-` 是一句 `errx`（参照件现量退 2）。
#[test]
fn a_dash_operand_reads_stdin_and_only_one_operand_may() {
    let dir = workspace("stdin");
    let (out, status, err) = run(&dir, &["-", "same1.txt"], b"abcde\n");
    assert_eq!((out.as_str(), status, err.as_str()), ("", 0, ""));
    let (out, status, err) = run(&dir, &["same1.txt", "-"], b"abcde\n");
    assert_eq!((out.as_str(), status, err.as_str()), ("", 0, ""));
    let (out, status, err) = run(&dir, &["-", "-"], b"abcde\n");
    assert_eq!((out.as_str(), status), ("", 2));
    assert_eq!(
        err, "cmp: standard input may only be specified once\n",
        "`cmp.c:106-110` 的第二支 `errx`，不接 usage"
    );
}

/// 三档退出码与两条报表：相同 0 且什么都不打；有差异 1，报表走**标准输出**、
/// 句式是 `%s %s differ: char %lld, line %lld`；`-l` 换成一格六宽的字节表。
#[test]
fn the_three_exit_codes_and_both_reports_follow_the_c() {
    let dir = workspace("report");
    let (out, status, err) = run(&dir, &["same1.txt", "same2.txt"], b"");
    assert_eq!((out.as_str(), status, err.as_str()), ("", 0, ""));
    let (out, status, err) = run(&dir, &["same1.txt", "diff2.txt"], b"");
    assert_eq!(
        (out.as_str(), status, err.as_str()),
        ("same1.txt diff2.txt differ: char 3, line 1\n", 1, "")
    );
    let (out, status, _) = run(&dir, &["-l", "same1.txt", "diff2.txt"], b"");
    assert_eq!((out.as_str(), status), ("     3 143 130\n", 1));
    let (out, status, err) = run(&dir, &["-s", "same1.txt", "diff2.txt"], b"");
    assert_eq!(
        (out.as_str(), status, err.as_str()),
        ("", 1, ""),
        "`-s` 什么都不打"
    );
    // 两边都空也退 0（`empty.txt` 与 `empty.txt`）。
    let (out, status, _) = run(&dir, &["empty.txt", "empty.txt"], b"");
    assert_eq!((out.as_str(), status), ("", 0));
}

/// 一边先见底：`regular.c:111` 报的是**短的那一边**。不带 `-l` 时先撞到的是差异
/// （`c` 之后一边是 `d` 一边是换行）所以走 `differ` 那一支；带 `-l` 才走到
/// `eofmsg`，句式是 `cmp: EOF on <短档>: char N, line M`。
#[test]
fn an_early_eof_names_the_shorter_file_with_its_own_shape() {
    let dir = workspace("eof");
    let (out, status, err) = run(&dir, &["same1.txt", "short3.txt"], b"");
    assert_eq!(
        (out.as_str(), status, err.as_str()),
        ("same1.txt short3.txt differ: char 4, line 1\n", 1, "")
    );
    let (out, status, err) = run(&dir, &["-l", "same1.txt", "short3.txt"], b"");
    assert_eq!((out.as_str(), status), ("     4 144  12\n", 1));
    assert_eq!(err, "cmp: EOF on short3.txt: char 5, line 1\n");
}

/// skip 那一段是 `strtoq(…, 0)` 的读数：`5x` 吃 5 而不管尾巴，`0x2` 是 2，
/// `010` 是八进制的 8（越过档尾 → `cmp: EOF on same1.txt`，**不带** char/line 后缀），
/// `-1` 合法并落到同一支；一个数字也没转出来的（`x`、空串）与溢出各退 2 走 usage。
#[test]
fn the_skip_operands_are_read_the_way_strtoq_reads_them() {
    let dir = workspace("skip");
    let cases = [
        ("2", "same1.txt same2.txt differ: char 1, line 1\n"),
        ("0x2", "same1.txt same2.txt differ: char 1, line 1\n"),
        ("5x", "same1.txt same2.txt differ: char 1, line 1\n"),
    ];
    for (word, expected) in cases {
        let (out, status, err) = run(&dir, &["same1.txt", "same2.txt", word], b"");
        assert_eq!(
            (out.as_str(), status, err.as_str()),
            (expected, 1, ""),
            "`{word}` 的读数不对"
        );
    }
    for word in ["010", "-1"] {
        let (out, status, err) = run(&dir, &["same1.txt", "same2.txt", word], b"");
        assert_eq!(
            (out.as_str(), status, err.as_str()),
            ("", 1, "cmp: EOF on same1.txt\n"),
            "`{word}` 越过档尾该报不带 char/line 的那一支"
        );
    }
    for word in ["x", "", "99999999999999999999999"] {
        let (out, status, err) = run(&dir, &["same1.txt", "same2.txt", word], b"");
        assert_eq!(
            (out.as_str(), status, err.as_str()),
            ("", 2, USAGE),
            "`{word}` 该落 usage 一支"
        );
    }
    // 两个 skip 都给：`2 2` 之后仍是差异。
    let (out, status, _) = run(&dir, &["same1.txt", "diff2.txt", "2", "2"], b"");
    assert_eq!(
        (out.as_str(), status),
        ("same1.txt diff2.txt differ: char 1, line 1\n", 1)
    );
}
