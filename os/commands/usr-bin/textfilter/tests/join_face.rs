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

/// 与 `run` 同一条命令缝，但两路输出都**逐字节**交回：`from_utf8_lossy` 把一个裸 `c3`
/// 渲染成三字节 U+FFFD，文本形分不清「一个字节」与「两个字节」这两种错法。
fn run_bytes(args: &[&str]) -> (Vec<u8>, i32, Vec<u8>) {
    let output = Command::new(env!("CARGO_BIN_EXE_join"))
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("the join binary should be runnable");
    (
        output.stdout,
        output.status.code().unwrap_or(-1),
        output.stderr,
    )
}

/// 集外字母那一行的字母位是**一个裸字节**：参照件 `tmp/join-407/join_ref`（本轮新配：
/// `minix3/usr.bin/join/join.c` + Minix 的 `getopt.c` + 程序名垫片，经 `join` 软链起，
/// 报文逐字节直比）现量 `-\u{e9}` 是 `join: unknown option -- ` + `c3` + 换行，再跟那两行
/// usage、退 1；`-Z\u{e9}` 停在 `Z`（`join.c:226-228` 的 `case '?'` 直落 `usage()`，报文不累加）。
/// join 的选项串 `"\01a:e:j:1:2:o:t:v:"` 里**每个字母都带值**，所以没有「合法字母后接多字节」
/// 这一形可钉；反面形是 `-a\u{e9}`——`é` 被当 `-a` 的值收走，答的是命令自己那句
/// `-a option field number not 1 or 2`，换渲染不许碰它。ASCII 的集外字母与缺值两形本文件
/// 已有主人，按冗余判据不重复承载。**那句自答的措辞也得照 C 逐字钉**：`-a` 这一支 C 写的是
/// `-a option **file** number not 1 or 2`（`join.c:171`，`-v` 同形 `:217`），而 `-1`/`-2`/`-j`
/// 三句写的是 **field**（`:141`、`:152`、`:186`）——同一支命令里两种措辞，本笔最初按邻居的
/// `field` 代入了 `file` 那一支，门红之后照 `join.c` 的原句改回。
#[test]
fn the_unknown_letter_of_a_multibyte_flag_is_written_as_one_byte() {
    let want: Vec<u8> = [
        b"join: unknown option -- ".as_slice(),
        b"\xc3\n",
        USAGE_FIRST.as_bytes(),
        USAGE_SECOND.as_bytes(),
    ]
    .concat();
    let (out, status, err) = run_bytes(&["-\u{e9}"]);
    assert_eq!(out, Vec::new(), "报文只走标准错误");
    assert_eq!(status, 1, "stderr 是 {err:?}");
    assert_eq!(err, want, "字母该是一个裸字节 c3");

    let (_out, status, err) = run_bytes(&["-Z\u{e9}"]);
    assert_eq!(status, 1);
    assert_eq!(
        err,
        [
            b"join: unknown option -- Z\n".as_slice(),
            USAGE_FIRST.as_bytes(),
            USAGE_SECOND.as_bytes(),
        ]
        .concat(),
        "环停在第一个坏字母上，`é` 根本不该被读到"
    );

    let (_out, status, err) = run_bytes(&["-a\u{e9}"]);
    assert_eq!(status, 1);
    assert_eq!(
        err,
        [
            b"join: -a option file number not 1 or 2\n".as_slice(),
            USAGE_FIRST.as_bytes(),
            USAGE_SECOND.as_bytes(),
        ]
        .concat(),
        "`-a` 带值，`\u{e9}` 整个被当值收走，答的是命令自己那句"
    );
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
            vec!["-a3", "f1", "f2"],
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

    let (_, _, err) = run(&["-a1x", "f1", "f2"]);
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

/// 金样来自 BSD 参照件现测：`join_ref`（`minix3/usr.bin/join/join.c` + 垫片，垫片须带
/// `fgetln`/`getprogname` 的**原型**，否则二者被隐式声明成返回 `int`、指针截断后段错误）。
/// 钉的是 `push_tok` 的槽位顺序：C 的 `outfield()` 先占分隔符槽、之后才决定内容
///（`join.c:496-497`；"字段存在但为空串"才 `return` 且槽位留着，`:503-504`）。旧写法先判空
/// 整个 `return`，于是 `-t,` 下 `a`、``、`c` 三字段被写成 `a,c`，而 C 是 `a,,c`。
// 缺席连接字段要占一个分隔符槽（金样来自参照件 `join_ref` 现测，批次二百九十八）。
// C 的 `outfield()` 先 `needsep++` 占槽再决定内容（`join.c:496-497`），因此 `-1 2`/`-2 2`
// 对上只有一列的行时，连接字段那个槽是空的但仍在；配 `-e` 就往那个槽里打替换串。
#[test]
fn missing_join_field_keeps_its_separator_slot() {
    let dir = std::env::temp_dir().join(format!("join-slot-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("scratch dir");
    let file = |name: &str, body: &[u8]| -> String {
        let path = dir.join(name);
        fs::write(&path, body).expect("scratch file");
        path.to_str().expect("utf-8 path").to_owned()
    };
    let s1 = file("short1", b"q\nr\n");
    let s2 = file("short2", b"q\nr\n");
    let (out, _, err) = run(&["-t,", "-1", "2", "-2", "2", &s1, &s2]);
    assert_eq!(
        out, ",q,q\n,q,r\n,r,q\n,r,r\n",
        "缺席的连接字段占一个空槽，stderr 是 {err:?}"
    );
    let (out, _, _) = run(&["-t,", "-1", "2", "-2", "2", "-e", "-", &s1, &s2]);
    assert_eq!(out, "-,q,q\n-,q,r\n-,r,q\n-,r,r\n", "`-e` 打的就是那个槽");
    let _ = fs::remove_dir_all(&dir);
}

// 以下六枚钉的是「字段切分」这一层与 C 同形（批次二百九十九）。
// C 的 `slurp` 用 `strsep(&bp, tabchar)`（`join.c:378`）切：**一个分隔符字节就是一刀**，
// 而 `spans`（默认 1、`-t` 清成 0，`:106`/`:200`）决定空段是不是字段。本层旧写法
// 把「跳空段」无条件套用，于是 `-t,` 下 `a,,c` 只剩两个字段：字段号、匹配集、`-o`、
// `-v` 四处一起错位。金样全部从参照件 `join_ref`（`minix3/usr.bin/join/join.c` +
// `fgetln`/`getprogname` 垫片链成）**逐形现测抄回**，命令与读数记在
// `rewrite-notes/18-stage-commands/todo.md` 批次二百九十九那条里。
// **按形分枚**：批次二百九十八的教训是「三形一测 + 整体 `#[ignore]`」会让负控
// 「注入成功却全绿」，所以一形一枚（只有 `-v 1`/`-v 2` 同一腿的两形合枚，已在测注里写明）、
// 各自一个临时目录。

/// 每枚测一个独占目录：本文件的测跑在同一进程的不同线程上，共用目录时谁先
/// `remove_dir_all` 就会把另一枚的输入抽走。
fn scratch(tag: &str, files: &[(&str, &[u8])]) -> (std::path::PathBuf, Vec<String>) {
    let dir = std::env::temp_dir().join(format!("join-split-{}-{}", std::process::id(), tag));
    fs::create_dir_all(&dir).expect("scratch dir");
    let mut paths = Vec::new();
    for (name, body) in files {
        let path = dir.join(name);
        fs::write(&path, body).expect("scratch file");
        paths.push(path.to_str().expect("utf-8 path").to_owned());
    }
    (dir, paths)
}

/// 两档只有一列的行，`-t,` 默认连接字段是第一个字段（`a`），两侧相等。
#[test]
fn dash_t_keeps_the_empty_field_between_the_two_words() {
    let (dir, paths) = scratch("default", &[("l", b"a,,c\n"), ("r", b"a,,c\n")]);
    let (out, status, err) = run(&["-t,", &paths[0], &paths[1]]);
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(out, "a,,c,,c\n", "空段是字段，槽位也留着");
    let _ = fs::remove_dir_all(dir);
}

/// `-o` 拿字段号说事：C 的 `1.2` 就是那个空段，本层旧写法把 `1.2` 读成了 `c`、
/// 把 `1.3` 读成越界（落 `-e` 的空串），两个槽的内容全错位。
#[test]
fn dash_t_numbered_output_fields_count_the_empty_field() {
    let (dir, paths) = scratch("olist", &[("l", b"a,,c\nb,,d\n"), ("r", b"a,,c\ne,,f\n")]);
    let (out, status, err) = run(&["-t,", "-o", "1.1,1.2,1.3,2.1", &paths[0], &paths[1]]);
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(out, "a,,c,a\n", "1.2 是空段、1.3 才是 `c`");
    let _ = fs::remove_dir_all(dir);
}

/// `-v` 那一路走 C 的 `outoneline`：它把行里**每个**字段都送进 `outfield`，空字段
/// 占槽不打字，所以报的是 `b,,d` 而不是 `b,d`。同一事实的两个侧写（`-v 1` 与 `-v 2`）
/// 合在一枚里，因为它们走的是同一条腿。
#[test]
fn dash_v_reports_the_unpaired_line_with_its_empty_field() {
    let (dir, paths) = scratch("dashv", &[("l", b"a,,c\nb,,d\n"), ("r", b"a,,c\ne,,f\n")]);
    let (out, status, err) = run(&["-t,", "-v", "1", &paths[0], &paths[1]]);
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(out, "b,,d\n");
    let (out, _, _) = run(&["-t,", "-v", "2", &paths[0], &paths[1]]);
    assert_eq!(out, "e,,f\n", "另一侧同形");
    let _ = fs::remove_dir_all(dir);
}

/// `-j 2` 把两侧的连接字段都指到那个空段：C 里两侧键相等，两行乘两行＝**四行交叉**；
/// 旧切法下两侧字段 2 是 `c`/`f`，匹配集本身就跑偏了（只给一行 `c,a,a`）。
/// 同一形的 `-1 2`（只改第一侧）两边读数都是空输出——那是**巧合同读数**，
/// 不能当门，所以下一枚用 `-1 2 -2 2`。
#[test]
fn dash_j_on_the_empty_field_matches_both_groups_and_cross_multiplies() {
    let (dir, paths) = scratch("dashj", &[("l", b"a,,c\nb,,d\n"), ("r", b"a,,c\ne,,f\n")]);
    let (out, status, err) = run(&["-t,", "-j", "2", &paths[0], &paths[1]]);
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(
        out, ",a,c,a,c\n,a,c,e,f\n,b,d,a,c\n,b,d,e,f\n",
        "两侧字段 2 都是空串→相等并连接"
    );
    let _ = fs::remove_dir_all(dir);
}

/// 上一枚的同形但分开写，目的是拿它能区分旧实现（旧切法只给一行 `c,a,a`）。
#[test]
fn per_file_join_fields_on_the_empty_field_also_cross_multiply() {
    let (dir, paths) = scratch("dash12", &[("l", b"a,,c\nb,,d\n"), ("r", b"a,,c\ne,,f\n")]);
    let (out, status, err) = run(&["-t,", "-1", "2", "-2", "2", &paths[0], &paths[1]]);
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(out, ",a,c,a,c\n,a,c,e,f\n,b,d,a,c\n,b,d,e,f\n");
    let _ = fs::remove_dir_all(dir);
}

/// `-a1 -a2` 同时出两侧不成对的行，三行金样都是从参照件抄下来的。
#[test]
fn dash_a_prints_every_field_of_the_unpaired_lines_including_empties() {
    let (dir, paths) = scratch("dasha", &[("l", b"a,,c\nb,,d\n"), ("r", b"a,,c\ne,,f\n")]);
    let (out, status, err) = run(&["-t,", "-a1", "-a2", &paths[0], &paths[1]]);
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(out, "a,,c,,c\nb,,d\ne,,f\n");
    let _ = fs::remove_dir_all(dir);
}

/// 行尾那个分隔符在 C 里还多给一个空字段（`strsep` 到串尾会再返一次）：
/// `a,` 是两字段，`-a1` 报 `a,`、`-o 1.1,1.2` 也报 `a,`。
/// 哪一枚能区分旧写法：`-a1` 那形能（旧壳给 `a`）；`-o 1.1,1.2` 那形**不能**——旧壳把字段 2 读成
/// 越界、落 `-e` 的空串，输出串凑巧一样。两者都钉住这一形，但区分职责只在前一句。
#[test]
fn a_trailing_delimiter_leaves_a_trailing_empty_field() {
    let (dir, paths) = scratch("trailing", &[("l", b"a,\n"), ("r", b"b\n")]);
    let (out, status, err) = run(&["-t,", "-a1", &paths[0], &paths[1]]);
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(out, "a,\n", "尾部空字段占最后一个槽");
    let (out, _, _) = run(&["-t,", "-o", "1.1,1.2", &paths[0], &paths[0]]);
    assert_eq!(out, "a,\n", "`-o` 指到那个空字段也是同形");
    let _ = fs::remove_dir_all(dir);
}

/// `-t ' '` 只把空格当分隔符（C 的 `tabchar` 就是那一字节），tab 不再是分隔符；
/// 旧写法把「分隔符是空格」当成「默认集」，连跳空段一起套上，两形都错。
#[test]
fn dash_t_space_keeps_empties_and_leaves_tabs_alone() {
    let (dir, paths) = scratch("space", &[("l", b"a  b\n"), ("r", b"a  b\n")]);
    let (out, status, err) = run(&["-t ", "-j", "2", &paths[0], &paths[1]]);
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(out, " a b a b\n", "两个空格中间那个空段就是连接字段");
    let _ = fs::remove_dir_all(dir);

    let (dir, paths) = scratch("tab", &[("l", b"a\tb\n"), ("r", b"a\tb\n")]);
    let (out, _, err) = run(&["-t ", "-j", "2", &paths[0], &paths[1]]);
    assert_eq!(
        out, " a\tb a\tb\n",
        "制表符仍是内容：一整串是一个字段，{err:?}"
    );
    let _ = fs::remove_dir_all(dir);
}

// —— 以下五枚钉的是比较层与 C 同形（批次三百零一）。C 的 `cmp`（`minix3/usr.bin/join/join.c:405-414`）
// 有**三种**答案：先查「字段号超出这一行的字段数」，两边都不越界才 `strcmp`。本层旧写法用
// `key()` 把「越界」与「该字段是空串」都折成空串，于是既会误判配对，也会让同一档内部的切组
//（C `slurp` 的 `:398`）跑偏。金样逐形从参照件 `tmp/family-diag-ref/join_ref` 现测抄回，
// 改前的读数记在 `tmp/join-cmp-301/before.log`；仍是一形一枚。

/// 「根本没有这一列」的行不等于「这一列是空串」的行：`a,` 有两字段（第二个是空串）、`a` 只有一字段，
/// 两侧都按字段 2 连接时 C 判为不匹配且不输出，本壳旧写法给 `,a,a`。
#[test]
fn an_overfield_join_key_is_not_equal_to_an_empty_one() {
    let (dir, paths) = scratch("overfield-eq", &[("l", b"a,\n"), ("r", b"a\n")]);
    let (out, status, err) = run(&["-t,", "-1", "2", "-2", "2", &paths[0], &paths[1]]);
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(out, "", "越界的那侧不当成空串去匹配");
    let _ = fs::remove_dir_all(dir);
}

/// 第一侧没有连接字段时，C 判它**大**（`:409-410` 的 `return 1`）→ 第二侧先走：金样先打 `x 9` 后打 `solo`。
/// 旧写法把缺字段折成空串，`"" < "9"` 判成第一侧小，两行顺序整个反过来。
#[test]
fn a_line_without_the_join_field_yields_the_lead_to_the_other_side() {
    let (dir, paths) = scratch("overfield-f1", &[("l", b"solo\n"), ("r", b"x 9\n")]);
    let (out, status, err) = run(&["-1", "2", "-2", "2", "-a1", "-a2", &paths[0], &paths[1]]);
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(out, "x 9\nsolo\n", "有字段那一侧先出");
    let _ = fs::remove_dir_all(dir);
}

/// 同一支的另一向：越界发生在第二侧时 C `return -1`（`:411-412`）→ 第一侧先走。
/// 两枚合看才能定序——旧写法对这两个方向给出**同一串**，正是它没有分辨力的证据。
#[test]
fn the_side_that_has_the_field_leads_when_the_other_lacks_it() {
    let (dir, paths) = scratch("overfield-f2", &[("l", b"x 9\n"), ("r", b"solo\n")]);
    let (out, status, err) = run(&["-1", "2", "-2", "2", "-a1", "-a2", &paths[0], &paths[1]]);
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(out, "x 9\nsolo\n");
    let _ = fs::remove_dir_all(dir);
}

/// 切组也走同一把尺：C 的 `slurp` 拿新行与上一行比，`cmp != 0` 就收尾这一组（`:398`）。
/// 于是 `a,,x`/`b`/`c,,y` 三行是**三个组**（`b` 没有字段 2），只第一行与另一侧的 `a,,x` 成对；
/// 旧写法把三行并成一组，多拼出两行根本不存在的配对。
#[test]
fn grouping_stops_at_a_line_that_lacks_the_join_field() {
    let (dir, paths) = scratch("grouping", &[("l", b"a,,x\nb\nc,,y\n"), ("r", b"a,,x\n")]);
    let (out, status, err) = run(&[
        "-t,", "-1", "2", "-2", "2", "-a1", "-a2", &paths[0], &paths[1],
    ]);
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(out, ",a,x,a,x\nb\nc,,y\n", "缺列那行自成一组");
    let _ = fs::remove_dir_all(dir);
}

/// 这一形改前改后同读数（都等于 C），入门作回归护栏：切组改动不能把「同键但字段数不同」的
/// 常规分档弄坏——`p 1`/`q`/`r 1` 里 `q` 没有字段 2，自成一档单独报出。
#[test]
fn the_mixed_field_counts_case_still_matches_the_reference() {
    let (dir, paths) = scratch("mixed-counts", &[("l", b"p 1\nq\nr 1\n"), ("r", b"p 1\n")]);
    let (out, status, err) = run(&["-1", "2", "-2", "2", "-a1", "-a2", &paths[0], &paths[1]]);
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(out, "1 p p\nq\nr 1\n");
    let _ = fs::remove_dir_all(dir);
}

// —— 以下四枚钉 `-o 0` 一族的拒收（批次三百零二）。C 的 `fieldarg` 只看 token 首字符是不是
// `1`/`2`（`minix3/usr.bin/join/join.c:526-527`），所以「`0` 表示连接字段自己」那支在 BSD/GNU 上
// 存在、在本树真源上不存在。四形（分开写、列首、列尾、附值形）的参照件读数都是
// `join: malformed -o option field` 退 1、且不接 usage（`errx` 那族的形状）。

/// 分开写的 `-o 0`：C 落 `errx` 那一族——只有一行句子、后面不接 usage、退 1。
#[test]
fn a_lone_zero_output_field_is_refused_without_a_usage_block() {
    let (out, status, err) = run(&["-o", "0", "f1", "f2"]);
    assert_eq!(out, "", "报文只走 stderr");
    assert_eq!(status, 1, "C 的 `errx(1, …)` 退 1");
    assert_eq!(err, "join: malformed -o option field\n");
}

/// `0` 在列表开头也一样被拒（C 是逐项扫的，第一项就撞前缀守卫）。
#[test]
fn a_zero_first_in_the_output_list_is_refused() {
    let (out, status, err) = run(&["-o", "0,1.1", "f1", "f2"]);
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(err, "join: malformed -o option field\n");
}

/// `0` 在列表尾部同样被拒：C 的循环不因前面已收下合法项就放过它。
#[test]
fn a_zero_last_in_the_output_list_is_refused() {
    let (out, status, err) = run(&["-o", "1.1,0", "f1", "f2"]);
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(err, "join: malformed -o option field\n");
}

/// 附值形 `-o0` 走同一支：`obsolete()` 的 legacy 改写只针对恰好是裸 `-o` 的 token，
/// 附值形直接把 `0` 交给 `fieldarg`，所以拒收的句子与分开写的那形完全相同。
#[test]
fn the_attached_dash_o_zero_is_refused_too() {
    let (out, status, err) = run(&["-o0", "f1", "f2"]);
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(err, "join: malformed -o option field\n");
}

/// 字节形 argv 的命令缝：`&str` 递不出裸 `ff`——`OsStr` 从 `str` 过来时那串已经是合法
/// UTF-8，所以这层夹具是必需的而不是省事。两路输出**逐字节**交回：`from_utf8_lossy` 会把
/// 一枚 `c3` 渲染成三字节 U+FFFD，文本形分不清「一个字节」与「三个字节」这两种错法。
fn run_argv(args: &[&[u8]]) -> (Vec<u8>, i32, Vec<u8>) {
    use std::os::unix::ffi::OsStrExt as _;
    let output = Command::new(env!("CARGO_BIN_EXE_join"))
        .args(args.iter().map(|word| std::ffi::OsStr::from_bytes(word)))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("the join binary should be runnable");
    (
        output.stdout,
        output.status.code().unwrap_or(-1),
        output.stderr,
    )
}

/// 按**原字节名**落一档，并把它的折叠孪生件（`from_utf8_lossy` 之后的那个名字）同时放上：
/// 开档与 `err(1, "%s", name)` 两条口要借 `&str`（在册 `ARGV-BYTE-PATH-IN-LIBS`），孪生件
/// 让「这档开不开得了」与「名字位回显成什么」这两件事能被分开量。交回的是原字节全路径。
fn write_raw(dir: &Path, name: &[u8], body: &[u8]) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt as _;
    let mut full = dir.to_str().expect("ascii dir").as_bytes().to_vec();
    full.push(b'/');
    full.extend_from_slice(name);
    fs::write(std::ffi::OsStr::from_bytes(&full), body).expect("write raw-named file");
    let folded = String::from_utf8_lossy(name).into_owned();
    fs::write(dir.join(&folded), body).expect("write the folded twin");
    full
}

/// 参数缝在 `join` 上改的是**判据与输出**，不只是报文，这是本族第一支这样的壳：
/// ①`-t` 的值按 `join.c:202` 的 `strlen(optarg) != 1` 判——文本缝上枚裸 `ff` 先被折成三字节，
///   于是「单字节分隔符」这一形从**接受**翻成**拒绝**（改前现量 `join: illegal tab character
///   specification` 退 1；参照件同形退 0 并真拿去切行，三侧读数 `tmp/join-463/before-after-smoke.log`）；
/// ②`-e` 的值由 `outfield`（`join.c:497-508`）原样打进空缺字段，折过的串会把一枚 `ff` 打成
///   `ef bf bd`（三侧读数 `tmp/join-463/before-after-smoke-2.log`）。
/// 反面形 `-t<é>`（`c3 a9`，合法 UTF-8 的两个字节）**三侧同拒**，钉的是「查的是字节数，不是
/// 能不能解码」——这一形也是「字节缝不等于什么都放宽」的证据。
#[test]
fn a_raw_byte_delimiter_is_accepted_and_a_raw_byte_replacement_is_printed() {
    // ①一枚裸 `ff` 当分隔符：接受、切行、并且输出分隔符也是那一枚字节（`*tabchar`）。
    let (dir, paths) = scratch("rawdelim", &[("l", b"a\xff1\nb\xff2\n"), ("r", b"a\xffx\n")]);
    let (out, status, err) = run_argv(&[
        b"-t\xff",
        paths[0].as_bytes(),
        paths[1].as_bytes(),
    ]);
    assert_eq!(status, 0, "单字节分隔符不该被拒：stderr 是 {err:?}");
    assert_eq!(
        out, b"a\xff1\xffx\n",
        "两侧按 `ff` 切出字段，展出的分隔符也是 `ff`"
    );
    assert_eq!(err, Vec::new(), "成功路一声不出");

    // ②反面形：`é` 是两个字节（合法 UTF-8），C 的判据同样拒——缝换了也不放宽这条。
    let (_, status, err) = run_argv(&[b"-t\xc3\xa9", paths[0].as_bytes(), paths[1].as_bytes()]);
    assert_eq!(status, 1);
    assert_eq!(
        err,
        [
            b"join: illegal tab character specification\n".as_slice(),
            USAGE_FIRST.as_bytes(),
            USAGE_SECOND.as_bytes(),
        ]
        .concat(),
        "`strlen(optarg) != 1` 查的是字节数"
    );

    // ③`-e` 的值直接是输出内容：空缺字段打进的是 argv 原字节。
    let (dir2, paths2) = scratch("rawempty", &[("l", b"a 1\nb 2\n"), ("r", b"a x\n")]);
    let (out, status, err) = run_argv(&[
        b"-a1",
        b"-o",
        b"1.1,2.2",
        b"-e\xffx",
        paths2[0].as_bytes(),
        paths2[1].as_bytes(),
    ]);
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(
        out,
        [b"a x\n".as_slice(), b"b \xffx\n".as_slice()].concat(),
        "`b` 那一行第二个字段来自 `-e`，必须是裸 `ff` 而不是 U+FFFD 的三字节"
    );

    // ④操数名字位只钉 happy path：开档与 `err` 那两句都在册的 `&str` 口上，所以盘上同放
    //    折叠孪生件，把「档位能不能打开」从这条读数的失败因里排除掉。
    let (dir3, paths3) = scratch("rawname", &[("r", b"a x\n")]);
    let raw = write_raw(&dir3, b"l\xffr", b"a 1\n");
    let (out, status, err) = run_argv(&[raw.as_slice(), paths3[0].as_bytes()]);
    assert_eq!((out.as_slice(), status), (b"a 1 x\n".as_slice(), 0), "stderr 是 {err:?}");
    let _ = fs::remove_dir_all(dir);
    let _ = fs::remove_dir_all(dir2);
    let _ = fs::remove_dir_all(dir3);
}

/// 三支把值拼进句子的腿，值位都是 argv 的原字节：`illegal field number -- %s`（`join.c:145`、
/// `:156`、`:190`）、`illegal file number -- %s`（`:176`、`:222`）与 `obsolete()` 那句
/// `illegal option -- %s`（`:592`，带的是整个 token 含前导短线）。前两支是 `warnx` 紧跟
/// `usage()`，所以各三行；第三支是 `errx`，**只有一行**——行数差与值差在同一枚门里各钉一段。
/// 字母位那一枚由 `bin_support_face` 的跨壳表钉，本枚不重复承载。
#[test]
fn the_three_value_echoes_carry_the_argv_bytes() {
    let (out, status, err) = run_argv(&[b"-1", b"2\xff", b"f1", b"f2"]);
    assert_eq!(out, Vec::new(), "报文只走标准错误");
    assert_eq!(status, 1);
    assert_eq!(
        err,
        [
            b"join: illegal field number -- 2\xff\n".as_slice(),
            USAGE_FIRST.as_bytes(),
            USAGE_SECOND.as_bytes(),
        ]
        .concat(),
        "值位是一枚 `ff`，不是折叠出的三字节"
    );

    let (_, status, err) = run_argv(&[b"-a1\xff", b"f1", b"f2"]);
    assert_eq!(status, 1);
    assert_eq!(
        err,
        [
            b"join: illegal file number -- 1\xff\n".as_slice(),
            USAGE_FIRST.as_bytes(),
            USAGE_SECOND.as_bytes(),
        ]
        .concat(),
        "文件号那支先由 `switch` 认下 1，再在尾字符上失手"
    );

    let (_, status, err) = run_argv(&[b"-j3\xff", b"f1", b"f2"]);
    assert_eq!(status, 1);
    assert_eq!(
        err,
        b"join: illegal option -- -j3\xff\n",
        "`obsolete()` 的畸形形带整个 token，且 `errx` 不接 usage"
    );
}
