//! End-to-end face for the `join` binary: real process, real pipes, checked against
//! `minix3/usr.bin/join/join.c` (the operand handling `:236-253`, the mutually
//! exclusive check `:235`, `usage` `:630-637`).
//!
//! `join` sits on the `err(3)`/`warnx(3)` side of the family boundary this crate keeps
//! having to relearn: the file leg is `err(1, "%s", *argv)` (`join.c:244`, `:249`), so
//! the line is `join: <operand>: <strerror(errno)>` — program name AND the number's
//! sentence. The shell printed `join: cannot open file 1`: the operand was gone, the
//! number was gone, and the words were invented. `usage()` is two lines in C with
//! `getprogname()` in the first one (`:632-635`); the shell had one line whose option
//! list did not match C either.
//!
//! One leg was missing outright: C checks `F1->fp == stdin && F2->fp == stdin` and
//! `errx(1, "only one input file may be stdin")` (`:250-251`). Without it the two
//! operands both read the same stream. Checked here before any bytes are compared,
//! because the whole point is that nothing is read.
//!
//! Unlike the file-ops shells, a hosted `join` can open real files, so the success path
//! is exercised too — but only as "stdout carries the join, stderr stays silent", which
//! is what the diagnostic legs owe.

use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;
use std::process::{Command, Stdio};

const USAGE_FIRST: &str = "usage: join [-a fileno | -v fileno] [-e string] [-j fileno field]\n";
const USAGE_SECOND: &str = "            [-o list] [-t char] [-1 field] [-2 field] file1 file2\n";

fn run_at(program: &Path, args: &[&str]) -> (String, i32, String) {
    let output = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("the join binary should be runnable");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

fn run(args: &[&str]) -> (String, i32, String) {
    run_at(Path::new(env!("CARGO_BIN_EXE_join")), args)
}

/// C's `err(3)` line: `<prog>: <operand>: <sentence>`.
fn assert_err_line(err: &str, prog: &str, operand: &str) {
    let head = format!("{prog}: {operand}: ");
    assert!(err.starts_with(&head), "`{err}` should start with `{head}`");
    let sentence = &err[head.len()..err.len() - 1];
    assert!(
        !sentence.is_empty() && sentence.chars().any(|c| c.is_lowercase()),
        "缀的应是一句 strerror 文本而不是代号：`{sentence}`"
    );
}

/// Two throwaway files with a shared key column; returns their paths.
fn operand_files() -> (std::path::PathBuf, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("join-face-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("temp dir");
    let one = dir.join("one");
    let two = dir.join("two");
    fs::write(&one, "a 1\nb 2\n").expect("write one");
    fs::write(&two, "a x\nb y\n").expect("write two");
    (one, two)
}

#[test]
fn a_successful_join_says_nothing_on_stderr() {
    // The diagnostic legs owe silence on the success path: C prints only the joined
    // lines (`join.c:253+`), and `stderr` stays empty.
    let (one, two) = operand_files();
    let (out, status, err) = run(&[one.to_str().unwrap(), two.to_str().unwrap()]);
    assert_eq!(status, 0, "两档都能开：`{err}`");
    assert_eq!(err, "", "成功路一声不出");
    assert!(
        out.contains('a') && out.contains('b'),
        "该有连接结果：`{out}`"
    );
    let _ = fs::remove_dir_all(one.parent().unwrap());
}

#[test]
fn an_unopenable_operand_reports_the_operand_and_the_number() {
    let (out, status, err) = run(&["/tmp/join-face-absent", "/tmp/join-face-other"]);
    assert_eq!(out, "", "C 的失败路不写标准输出");
    assert_eq!(status, 1, "`err(1, …)` 是退 1");
    assert_err_line(&err, "join", "/tmp/join-face-absent");
    assert!(
        !err.contains("cannot open"),
        "「cannot open」是本树自创的短语，C 打的是操数名：`{err}`"
    );
}

#[test]
fn two_stdin_operands_are_refused_before_anything_is_read() {
    // `join.c:250-251`: both `FILE*` end up as `stdin`, which C refuses outright —
    // `errx` shaped (program name, phrase, no sentence), and no usage line follows.
    let (out, status, err) = run(&["-", "-"]);
    assert_eq!(out, "", "这一支护住在读之前，不该有任何输出");
    assert_eq!(status, 1);
    assert_eq!(err, "join: only one input file may be stdin\n");
}

#[test]
fn the_mutually_exclusive_pair_is_errx_without_a_usage_line() {
    // `join.c:235`: `errx(1, "-a and -v options mutually exclusive")` — one line, no
    // usage. The other option errors in C are `warnx` + `usage()`; that wording needs
    // the option letter and value, which the deciding half's error type drops, and is
    // registered as the next slice rather than invented here.
    let (_, status, err) = run(&["-a", "1", "-v", "1", "/tmp/join-face-a", "/tmp/join-face-b"]);
    assert_eq!(status, 1);
    assert_eq!(err, "join: -a and -v options mutually exclusive\n");
}

#[test]
fn usage_is_two_lines_with_the_name_only_in_the_first() {
    // `join.c:630-637`: `fprintf(stderr, "usage: %s ...\n            ...\n",
    // getprogname())` — the second line is indented twelve spaces and carries no name.
    let expected = format!("{USAGE_FIRST}{USAGE_SECOND}");
    let (out, status, err) = run(&[]);
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(err, expected, "两行 usage 逐字对 C");

    let (_, status, err) = run(&["/tmp/join-face-only"]);
    assert_eq!(status, 1, "一个操数也是 usage（`argc != 2`）");
    assert_eq!(err, expected);
}

/// 表外字母与带值选项缺值这两支，上面那两行 usage 前还有一行 libc 自己的报文
/// （`minix3/lib/libc/stdlib/getopt.c:101-104` 与 `:132-136`）。上一枚测钉的是
/// **裸 usage**（第三个操数与无操数），两者行数不同——拿一枚通用「usage 错」盖住
/// 就会把那一行弄掉（本壳之前就是这样）。
#[test]
fn getopt_owned_legs_print_their_line_before_the_two_usage_lines() {
    let usage = format!("{USAGE_FIRST}{USAGE_SECOND}");
    let (out, status, err) = run(&["-z", "f1", "f2"]);
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(err, format!("join: unknown option -- z\n{usage}"));
    // 缺值那一句带的是那个字母（字母不带引号，与本机 glibc 不同形）。
    let (_, status, err) = run(&["f1", "f2", "-t"]);
    assert_eq!(status, 1);
    assert_eq!(
        err,
        format!("join: option requires an argument -- t\n{usage}")
    );
}

#[test]
fn the_report_line_follows_argv_zero_like_getprogname_does() {
    // The program name is `argv[0]`'s last segment: a copy under another name moves the
    // `err(3)` line's head and the usage line's name, while the operand path stays as
    // given. That is what pins the name is not baked in as a literal.
    let dir = std::env::temp_dir().join(format!("join-face-copy-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("temp dir");
    let copied = dir.join("renamed-join");
    fs::copy(Path::new(env!("CARGO_BIN_EXE_join")), &copied).expect("copy the binary");
    fs::set_permissions(&copied, fs::Permissions::from_mode(0o755)).expect("make it run");

    let (_, _, err) = run_at(&copied, &["/tmp/join-face-3", "/tmp/join-face-4"]);
    assert_err_line(&err, "renamed-join", "/tmp/join-face-3");

    let (_, _, err) = run_at(&copied, &[]);
    assert_eq!(
        err,
        format!(
            "usage: renamed-join [-a fileno | -v fileno] [-e string] [-j fileno field]\n{USAGE_SECOND}"
        )
    );

    let _ = fs::remove_dir_all(&dir);
}

/// `warnx` + `usage()` 两支合起来是**三行**：第一行缀程名并带该支自己的选项字母，
/// 后两行是 usage。旧壳把第一行写成不带字母的通用话，等于把 `join.c` 的八支并成了一句。
#[test]
fn a_warnx_leg_prints_its_own_letter_then_the_two_usage_lines() {
    for (args, first) in [
        (
            vec!["-1", "0", "f1", "f2"],
            "join: -1 option field number less than 1\n",
        ),
        (
            vec!["-2", "0", "f1", "f2"],
            "join: -2 option field number less than 1\n",
        ),
        (
            vec!["-j", "0", "f1", "f2"],
            "join: -j option field number less than 1\n",
        ),
        (
            vec!["-a", "3", "f1", "f2"],
            "join: -a option file number not 1 or 2\n",
        ),
        (
            vec!["-v", "3", "f1", "f2"],
            "join: -v option file number not 1 or 2\n",
        ),
        (
            vec!["-t", "ab", "f1", "f2"],
            "join: illegal tab character specification\n",
        ),
    ] {
        let (out, status, err) = run(&args);
        assert_eq!(out, "", "报文只走 stderr");
        assert_eq!(status, 1, "`{args:?}` 应退 1");
        assert_eq!(
            err,
            format!("{first}{USAGE_FIRST}{USAGE_SECOND}"),
            "`{args:?}` 的三行形与 C 不符"
        );
    }
}

/// `illegal … -- <值>` 那两支**不写选项字母**，只把 `optarg` 整个跟在一对短线后
/// （`join.c:145`/`:176`）。同时钉住 `strtol` 的判序：`1x` 的 1 已被 switch 认下，
/// 所以 `-a 1x` 落 illegal 那句；而一个数字都转不出的 `f1` 先过 `< 1` 那道守卫，
/// 所以 `-j f1` 落 less-than-1——旧实现按「是否全数字」分流，这两支都归错了。
/// 与 BSD 参照件逐形 A/B 的一枚对账门（`tmp/family-diag-ref/join_ref`，由
/// `minix3/usr.bin/join/join.c` 真源加 `fgetln`/`getprogname` 垫片链成；批次二百九十四）。
/// 每一行的期望句子都是从参照件**实测抄回**的常量（本文件跑的是我们自己的二进制；对账
/// 命令与读数记在 `rewrite-notes/18-stage-commands/todo.md` 批次二百九十四那条里：同一批
/// 形两边各跑一遍，差异只剩程序名段），不是从 C 代码推的。覆盖的是既有门没钉到的形：
/// `-2`、`-j` 的尾字符支、`-o` 的四形、`-t` 的附值形。
/// **可用边界（必须写清，否则这枚门会被误当成全形对照）**：那份参照件链的是宿主 glibc 的
/// `fgetln` 垫片，**读档路径会段错误**，所以它只量得到「在开档之前就退出」的诊断路；
/// 而它的 `usage()` 在同一垫片下也段错误（`-1 0` 那类 warnx 之后就到不了 usage 块），
/// 因此这里比的是**首行句子**，usage 那两行由上面各枚门按 C 源码文本单独钉。
#[test]
fn the_option_error_first_lines_match_the_bsd_reference() {
    let subject = std::path::Path::new(env!("CARGO_BIN_EXE_join"));
    let (first, second) = operand_files();
    for (args, line) in [
        (vec!["-2", "0"], "join: -2 option field number less than 1"),
        (vec!["-1", "1x"], "join: illegal field number -- 1x"),
        (vec!["-j", "5x"], "join: illegal field number -- 5x"),
        (vec!["-o", "0.1"], "join: malformed -o option field"),
        (vec!["-o", "3.1"], "join: malformed -o option field"),
        (vec!["-o", "1.1x"], "join: malformed -o option field"),
        (vec!["-o", "1.0"], "join: field numbers are 1 based"),
        (
            vec!["-t", "xy"],
            "join: illegal tab character specification",
        ),
    ] {
        let mut words = args.clone();
        words.push(first.to_str().unwrap());
        words.push(second.to_str().unwrap());
        let (_, status, err) = run_at(subject, &words);
        let got = err.lines().next().unwrap_or_default().to_owned();
        assert_eq!(
            got, line,
            "`{args:?}` 的首行该与 C 同形，完整 stderr 是 {err:?}"
        );
        // 这些支全部 `warnx`+`usage()` 或 `errx`，两者都退 1（`join.c:usage()` 的
        // `exit(1)` 与 `errx(1, ...)` 同值）。
        assert_eq!(status, 1, "`{args:?}` 的退码该是 1，stderr 是 {err:?}");
    }
    let _ = std::fs::remove_file(&first);
    let _ = std::fs::remove_file(&second);
}

#[test]
fn the_illegal_legs_quote_the_value_without_the_option_letter() {
    let (_, status, err) = run(&["-1", "2x", "f1", "f2"]);
    assert_eq!(status, 1);
    assert_eq!(
        err,
        format!("join: illegal field number -- 2x\n{USAGE_FIRST}{USAGE_SECOND}")
    );

    let (_, _, err) = run(&["-a", "1x", "f1", "f2"]);
    assert_eq!(
        err,
        format!("join: illegal file number -- 1x\n{USAGE_FIRST}{USAGE_SECOND}")
    );

    let (_, _, err) = run(&["-j", "f1", "f2"]);
    assert_eq!(
        err,
        format!("join: -j option field number less than 1\n{USAGE_FIRST}{USAGE_SECOND}"),
        "`strtol` 转不出数字时值为 0，C 先查 `< 1`"
    );
}

/// `-o` 的 `fieldarg` 两支与 `-a`+`-v` 那支都是 `errx`：**只有一行**，后面不接 usage
/// （`join.c:235`、`:527-532`）。两支 `-o` 失手在 C 里是两句不同的话，不能拢成一句。
#[test]
fn the_errx_legs_print_one_line_and_skip_the_usage_block() {
    let (_, status, err) = run(&["-o", "1.0", "f1", "f2"]);
    assert_eq!(status, 1);
    assert_eq!(err, "join: field numbers are 1 based\n");

    let (_, status, err) = run(&["-o", "3.1", "f1", "f2"]);
    assert_eq!(status, 1);
    assert_eq!(err, "join: malformed -o option field\n");

    let (_, status, err) = run(&["-a1", "-v2", "f1", "f2"]);
    assert_eq!(status, 1);
    assert_eq!(err, "join: -a and -v options mutually exclusive\n");
}
