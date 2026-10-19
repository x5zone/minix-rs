//! End-to-end face for the `comm` binary, checked against
//! `minix3/usr.bin/comm/comm.c`（选项环 `:80-95`、操作数判 `:97-102`、
//! `file()` 的开档支 `:177-187`、`usage()` `:195-199`、`getnextln()` `:201-215`）
//! 与它驱动的 libc `getopt`（`minix3/lib/libc/stdlib/getopt.c:74-78`、`:81-92`、`:95-104`）。
//!
//! `comm.c:80` 的 optstring 是 `"123f"`：`1`/`2`/`3`/`f` 四个**无值**字母。这一条决定了
//! 本支的可达出口只有两类——参照件 `tmp/comm-315/comm_ref`（`comm.c` 真源 + Minix 的
//! `getopt.c` + 程序名垫片，宿主现链）逐形量出的形状是：
//! ① 集外字母 = libc 一行 + usage 一行，退 1（`-Z`、聚簇 `-1Z` 报 `Z`、`--x` 报 `-`）；
//! ② 操作数不是两个 = **只有 usage 一行**，退 1（少一个、多一个、`comm 档 -1 档` 那种
//!    「选项写在操作数之后」的形都是这一类——环不重排，`-1` 是第三个档名）；
//! ③ 打不开的档名 = `err(1, "%s", name)` 三段形、一行、**致命**：第一个档名开不了就退 1，
//!    第二个档名连试都不试（区别于 `column`/`rev` 的 warn+继续）；
//! ④ usage 那一行的程序名是**写死在串里**的（`comm.c:197` 没有 `getprogname()`），
//!    而 ① 那一行的前缀跟着 `getprogname()`——同一支命令里两行的来历不同；
//! ⑤ 裸 `-` 是合法操作数（`comm.c:181` 的 `!strcmp(name, "-")` → `stdin`），`--` 结束环
//!    而自身不是操作数；
//! ⑥ 末行没有换行符**照样是一条记录**（`getnextln` 只在 EOF 且一个字符都没读到时才返回
//!    `NULL`），这一条与 `column` 的「残行当长行丢掉」正相反；
//! ⑦ 制表符前缀**跟着真正打出来的列走**（`comm.c:105-110` 依次把 `tabs[]` 发给
//!    打出来的列），所以 `-1` 之后公共行拿的是 `tabs[1]` 一个制表符而不是两个——
//!    本壳旧写法按列号固定取 `tabs[column - 1]`，被这扇新门当场逮住。
//!
//! 这扇门前不存在：`comm` 的读缝直接拿裸 `minix_sys::open`/`read`，宿主上读不到任何
//! 操作数文件（同族第八例），三列合并的进程级行为从来没被测过。

mod face_support;

use face_support::write_stdin;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};

const COMM: &str = env!("CARGO_BIN_EXE_comm");
/// `comm.c:197` 那一行：程序名写死在串里。
const USAGE: &str = "usage: comm [-123f] file1 file2\n";

/// 与 `run` 同一条命令缝，但两路输出**逐字节**交回：`from_utf8_lossy` 把一个裸 `c3`
/// 渲染成三字节 U+FFFD，文本形分不清「一个字节」与「两个字节」这两种错法。
fn run_bytes(args: &[&str], input: &[u8]) -> (Vec<u8>, i32, Vec<u8>) {
    let mut child = Command::new(COMM)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the comm binary should be runnable");
    face_support::write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        output.stdout,
        output.status.code().unwrap_or(-1),
        output.stderr,
    )
}

/// 集外字母那一行的字母位是**一个裸字节**：参照件 `tmp/comm-409/comm_ref` 现量 `-é` 与
/// `-1é` 都是 `comm: unknown option -- ` + `c3` + 换行再跟那行 usage、退 1
/// （`1` 是 `"123f"` 里的合法字母，聚簇继续吃到 `é` 的第一字节；`-Zé` 停在 `Z`）。
/// 这一形要**两半一起对**才成立：取法按字节 + 报文按字节写，所以本枚与库里那枚
/// `the_unknown_letter_is_the_first_byte_not_the_first_character` 各钉一半。
#[test]
fn the_libc_line_of_a_multibyte_flag_is_one_byte_after_the_cluster() {
    let want: Vec<u8> = [
        b"comm: unknown option -- ".as_slice(),
        b"\xc3\n",
        USAGE.as_bytes(),
    ]
    .concat();
    for form in ["-\u{e9}", "-1\u{e9}"] {
        let (out, status, err) = run_bytes(&[form, "/dev/null", "/dev/null"], b"");
        assert_eq!(out, Vec::new(), "{form} 的报文只走标准错误");
        assert_eq!(status, 1, "{form} 的 stderr 是 {err:?}");
        assert_eq!(err, want, "{form} 的字母该是一个裸字节 c3");
    }

    let (_out, status, err) = run_bytes(&["-Z\u{e9}", "/dev/null", "/dev/null"], b"");
    assert_eq!(status, 1);
    assert_eq!(
        err,
        [b"comm: unknown option -- Z\n".as_slice(), USAGE.as_bytes(),].concat(),
        "环停在第一个坏字母上，`é` 根本不该被读到"
    );
}

fn run(args: &[&str], input: &[u8]) -> (String, i32, String) {
    let mut child = Command::new(COMM)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the comm binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// 两个已排序的操作数档（参照件用的同一对内容），返回它们的绝对路径。
fn operands(tag: &str) -> (PathBuf, String, String) {
    let dir = scratch(tag);
    let a = dir.join("a.txt");
    let b = dir.join("b.txt");
    fs::write(&a, b"apple\nbanana\ncherry\n").expect("write a.txt");
    fs::write(&b, b"banana\ndate\nfig\n").expect("write b.txt");
    (
        dir,
        a.to_str().unwrap().to_string(),
        b.to_str().unwrap().to_string(),
    )
}

/// 集外字母：libc 那一行在前、写死的 usage 在后，退 1（旧写法少了 libc 那一行）。
#[test]
fn an_unknown_letter_prints_the_libc_line_then_the_usage() {
    let (_dir, a, b) = operands("unknown");
    let (out, status, err) = run(&["-Z", &a, &b], b"");
    assert_eq!((out.as_str(), status), ("", 1), "stderr 是 {err:?}");
    assert_eq!(err, format!("comm: unknown option -- Z\n{USAGE}"));

    // 聚簇只报第一个集外字母，前面的合法字母不改变出口。
    let (_, status, err) = run(&["-1Z", &a, &b], b"");
    assert_eq!(status, 1);
    assert_eq!(err, format!("comm: unknown option -- Z\n{USAGE}"));

    // `--x` 报的字母是第二个连字符（optstring 里没有 `-`）。
    let (_, status, err) = run(&["--x", &a, &b], b"");
    assert_eq!(status, 1);
    assert_eq!(err, format!("comm: unknown option -- -\n{USAGE}"));
}

/// 操作数不是两个 = **只有一行 usage**：libc 那一行没有，`err` 那一行也没有。
/// 「选项写在操作数之后」同样算三个操作数——环不重排。
#[test]
fn an_operand_count_other_than_two_is_the_usage_line_only() {
    let (_dir, a, b) = operands("count");
    for (args, why) in [
        (vec![], "没有操作数"),
        (vec![a.as_str()], "只有一个操作数"),
        (vec![a.as_str(), b.as_str(), "extra"], "三个操作数"),
        (
            vec![a.as_str(), "-1", b.as_str()],
            "选项写在操作数之后（不重排）",
        ),
    ] {
        let (out, status, err) = run(&args, b"apple\nbanana\n");
        assert_eq!((out.as_str(), status), ("", 1), "{why}：stderr 是 {err:?}");
        assert_eq!(err, USAGE, "{why} 只该有 usage 一行");
    }
}

/// 三列合并的缺省形制：列一无前缀、列二一个制表符、列三两个（`comm.c:56` 的 `tabs[]`）。
#[test]
fn the_default_merge_writes_three_columns_with_tab_offsets() {
    let (_dir, a, b) = operands("merge");
    let (out, status, err) = run(&[&a, &b], b"");
    assert_eq!(status, 0);
    assert_eq!(err, "");
    assert_eq!(out, "apple\n\t\tbanana\ncherry\n\tdate\n\tfig\n");
}

/// `-1`/`-2`/`-3` 各自压掉一列，**且列前缀跟着重排**：`-1` 之后原来的列二变成
/// 无前缀那一列（`comm.c:105-110` 只给「打出来的列」依次配 `tabs` 的偏移）。
#[test]
fn each_suppression_flag_drops_its_column_and_retitles_the_rest() {
    let (_dir, a, b) = operands("suppress");
    let (out, _, _) = run(&["-1", &a, &b], b"");
    assert_eq!(out, "\tbanana\ndate\nfig\n");
    let (out, _, _) = run(&["-2", &a, &b], b"");
    assert_eq!(out, "apple\n\tbanana\ncherry\n");
    let (out, _, _) = run(&["-3", &a, &b], b"");
    assert_eq!(out, "apple\ncherry\n\tdate\n\tfig\n");
    // 聚簇逐字母生效：`-12` 只剩公共列，且它拿的是 `tabs[0]`（无前缀）。
    let (out, _, _) = run(&["-12", &a, &b], b"");
    assert_eq!(out, "banana\n");
    // 三列全压掉就什么都不打，退码仍是 0。
    let (out, status, err) = run(&["-123", &a, &b], b"");
    assert_eq!((out.as_str(), status, err.as_str()), ("", 0, ""));
}

/// `-f` 把比较换成 `strcasecmp`（`comm.c:88-89`）：同一对档不折叠时没有公共行，
/// 折叠后 `Apple`/`banana` 这类配对才成立。
#[test]
fn the_fold_flag_compares_case_insensitively() {
    let dir = scratch("fold");
    let up = dir.join("up.txt");
    let b = dir.join("b.txt");
    fs::write(&up, b"Apple\nBANANA\nCherry\n").expect("write up.txt");
    fs::write(&b, b"banana\ndate\nfig\n").expect("write b.txt");
    let (out, status, err) = run(&["-f", up.to_str().unwrap(), b.to_str().unwrap()], b"");
    assert_eq!(
        (out.as_str(), status),
        ("Apple\n\t\tBANANA\nCherry\n\tdate\n\tfig\n", 0)
    );
    assert_eq!(err, "");
    // 不折叠时同一对档按字节序走：大写全排在 `banana` 之前，所以三条都在列一，
    // 然后第一边见底、第二边整段落在列二（参照件现量正是这一形）。
    let (plain, status, _) = run(&[up.to_str().unwrap(), b.to_str().unwrap()], b"");
    assert_eq!(status, 0);
    assert_eq!(plain, "Apple\nBANANA\nCherry\n\tbanana\n\tdate\n\tfig\n");
}

/// 裸 `-` 是**合法操作数**并就是标准输入（`comm.c:181`）；`--` 结束环而自身不是操作数，
/// 它后面的 `-1` 是字面档名。
#[test]
fn a_dash_names_stdin_and_the_double_dash_ends_the_ring() {
    let (_dir, a, _b) = operands("dash");
    // 第一操作数是 `-`：标准输入那一份与档里那一份并排走。第一边先见底，**剩下的
    // 是第二边的行，落在列二**（参照件现量 `\t\tapple`、`\t\tbanana`、`\tcherry`）。
    let (out, status, err) = run(&["-", &a], b"apple\nbanana\n");
    assert_eq!(
        (out.as_str(), status),
        ("\t\tapple\n\t\tbanana\n\tcherry\n", 0)
    );
    assert_eq!(err, "");

    // 第二操作数开不了 = err(1) 三段形、一行、致命（不接 usage）。
    let (out, status, err) = run(&["-", "/nonexistent-second"], b"apple\n");
    assert_eq!((out.as_str(), status), ("", 1));
    assert_eq!(
        err, "comm: /nonexistent-second: No such file or directory\n",
        "开不了的档名是 err(1) 三段形且致命"
    );

    let dir = scratch("dashdash");
    let one = dir.join("one.txt");
    let two = dir.join("two.txt");
    fs::write(&one, b"alpha\nbeta\n").expect("write one.txt");
    fs::write(&two, b"beta\ngamma\n").expect("write two.txt");
    let (out, status, err) = run(&["--", one.to_str().unwrap(), two.to_str().unwrap()], b"");
    assert_eq!((out.as_str(), status), ("alpha\n\t\tbeta\n\tgamma\n", 0));
    assert_eq!(err, "");
    // `--` 之后的 `-1` 不当旗标：两个都是字面档名，第一个就开不了 → 致命。
    let (_, status, err) = run(&["--", "-1", "-2"], b"");
    assert_eq!(status, 1);
    assert_eq!(err, "comm: -1: No such file or directory\n");
}

/// 打不开的档名是 `err(1, "%s", name)` 三段形而且**致命**：第一个档名失败时第二个
/// 连试都不试（参照件两支都只印一行、退 1）；第二个失败时第一个已成功读入。
#[test]
fn an_unopenable_operand_is_fatal_and_prints_one_three_part_line() {
    let (_dir, a, b) = operands("fatal");
    let (out, status, err) = run(&["no-such-file", &b], b"");
    assert_eq!((out.as_str(), status), ("", 1));
    assert_eq!(
        err, "comm: no-such-file: No such file or directory\n",
        "三段形：程序名、档名、`strerror` 句子，且**不接** usage"
    );

    let (out, status, err) = run(&[&a, "no-such-file"], b"");
    assert_eq!((out.as_str(), status), ("", 1));
    assert_eq!(err, "comm: no-such-file: No such file or directory\n");
}

/// 末行没有换行符**照样是一条记录**（`getnextln` 只在「EOF 且一个字符都没读到」时才
/// 返回 `NULL`）——这一条与 `column` 正相反，那边把残行当「长行」丢掉并警告。
#[test]
fn an_unterminated_last_line_is_still_a_record() {
    let dir = scratch("tail");
    let a = dir.join("a.txt");
    let nonl = dir.join("nonl.txt");
    fs::write(&a, b"apple\nbanana\ncherry\n").expect("write a.txt");
    fs::write(&nonl, b"apple\nbanana\ncherry").expect("write nonl.txt");
    let (out, status, err) = run(&[a.to_str().unwrap(), nonl.to_str().unwrap()], b"");
    assert_eq!(status, 0);
    assert_eq!(err, "");
    assert_eq!(out, "\t\tapple\n\t\tbanana\n\t\tcherry\n");
}

/// 空档是合法的第二个输入：另一边的行整个落在自己那列里，两句都空则什么都没有。
#[test]
fn an_empty_operand_drains_the_other_side_into_its_own_column() {
    let dir = scratch("empty");
    let empty = dir.join("empty.txt");
    let b = dir.join("b.txt");
    fs::write(&empty, b"").expect("write empty.txt");
    fs::write(&b, b"banana\ndate\nfig\n").expect("write b.txt");
    let (out, status, err) = run(&[empty.to_str().unwrap(), b.to_str().unwrap()], b"");
    assert_eq!((out.as_str(), status), ("\tbanana\n\tdate\n\tfig\n", 0));
    assert_eq!(err, "");
    let (out, status, err) = run(&[empty.to_str().unwrap(), empty.to_str().unwrap()], b"");
    assert_eq!((out.as_str(), status, err.as_str()), ("", 0, ""));
}

/// 两腿一起按字节：① 非 UTF-8 的行照样是记录、逐字节打回（C 的 `getnextln` 逐 `getc`
/// 读，`comm.c:204-215`）；② `-f` 的折叠是逐字节的 ctype 表，`c3 89`（É）与 `c3 a9`（é）
/// **不**相等（`strcasecmp` `minix3/common/lib/libc/string/strcasecmp.c:58-67`）。
/// 期望逐形取自参照件 `tmp/comm-409/comm_ref` 的现量读数（`tmp/paste-comm-434/ab.log`）。
#[test]
fn a_nonutf8_line_is_a_record_and_the_fold_is_per_byte() {
    let dir = scratch("nonutf8");
    let a = dir.join("a.txt");
    let b = dir.join("b.txt");
    let upper = dir.join("upper.txt");
    let lower = dir.join("lower.txt");
    fs::write(&a, b"a\xffb\ncommon\nz\n").expect("write a.txt");
    fs::write(&b, b"a\xffb\nother\n").expect("write b.txt");
    fs::write(&upper, b"\xc3\x89\nApple\n").expect("write upper.txt");
    fs::write(&lower, b"\xc3\xa9\napple\n").expect("write lower.txt");

    let (out, status, err) = run_bytes(&[a.to_str().unwrap(), b.to_str().unwrap()], b"");
    assert_eq!((status, err), (0, Vec::new()));
    assert_eq!(
        out, b"\t\ta\xffb\ncommon\n\tother\nz\n",
        "共有的那一行带 ff，两半都该留住"
    );

    // 不折叠：按字节序，`c3 89` 与 `c3 a9` 在第二位分开 ⇒ 各落自己那一列
    let (out, status, err) = run_bytes(&[upper.to_str().unwrap(), lower.to_str().unwrap()], b"");
    assert_eq!((status, err), (0, Vec::new()));
    assert_eq!(out, b"\xc3\x89\nApple\n\t\xc3\xa9\n\tapple\n");

    // `-f` 之后**形状不变**：C 的 ctype 表只折 `A`–`Z`，非 ASCII 字节原样返回，
    // 而 ASCII 那一对 `Apple`/`apple` 折完相同、照旧落列一（因为 `A` < `c3`）。
    // 旧写法经 `str::to_lowercase` 把 É/é 折成同一个字符，这两行被判成公共行落列三，
    // 现量见 `tmp/paste-comm-434/ab.log` 的「comm -f」一行。
    let (out, status, err) = run_bytes(
        &["-f", upper.to_str().unwrap(), lower.to_str().unwrap()],
        b"",
    );
    assert_eq!((status, err), (0, Vec::new()));
    assert_eq!(out, b"\xc3\x89\nApple\n\t\xc3\xa9\n\tapple\n");
}

fn scratch(tag: &str) -> PathBuf {
    static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("comm-face-{}-{tag}-{n}", std::process::id()));
    fs::create_dir_all(&dir).expect("scratch dir");
    dir
}
