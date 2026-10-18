//! End-to-end face for the `column` binary, checked against
//! `minix3/usr.bin/column/column.c`（选项环 `:93-110`、操作数段 `:112-127`、
//! `input()` 的残行判 `:270-297`、`usage()` `:302-307`）与它驱动的 libc `getopt`
//!（`minix3/lib/libc/stdlib/getopt.c:74-78`、`:81-92`、`:95-104`、`:132-136`）。
//!
//! `column.c:93` 的 optstring 是 `"c:s:tx"`：两个带值项加两个旗标，操数是**文件清单**
//! （开得了就读，开不了就 `warn(3)` 继续）。参照件 `tmp/column-314/column_ref`
//!（`column.c` 真源 + Minix 的 `getopt.c` + 程序名/`fgetln` 垫片，宿主现链）逐形量出的
//! 形状是：
//! ① 集外字母 = libc 一行 + usage 一行，退 1（`-Z`、聚簇 `-tZ` 报 `Z`、`--x` 报 `-`）；
//! ② `-c`/`-s` 缺值 = libc 的 `option requires an argument -- <字母>` + usage，退 1
//!    （旧写法把这一支并进自拟的 usage，少了 libc 那一行）；
//! ③ usage 那一行是**大写 `Usage`、程序名跟着 `getprogname()`、末尾带 `[file ...]`**，
//!    旧写法写成了小写自拟短式；
//! ④ `-c` 的值是 `atoi`：`0`、`abc`、`-5` 都不是错，宽度塌成「每条一行」，退 0；
//! ⑤ 不重排：第一个非选项 token 一到选项环就结束，它后面的 `-t` 是**档名**（参照件现量：
//!    `column: Cannot open `-t': No such file or directory`），不是被悄悄吃掉的旗标；
//! ⑥ 打不开的操数是三段形 `warn(3)` 且**不致命**：本条跳过、退出码置 1、继续下一个档；
//! ⑦ 裸 `-` 与 `--` 之后的 token 都是操数，而 `--` 自身不是（`getopt.c:81-85` 先推进）；
//! ⑧ 末行没有换行符时 C 判它「长行」：那句 `warnx(3)` 只有两段（不缀 strerror），
//!    这一行不进条目池，退出码置 1——整条是空白的末行仍按空行处理，一句警告都没有；
//! ⑨ 条目是**整行**：C 存的是行首的 `buf`，不是跳过前导空白后的那个指针，
//!    所以前导空白既进池也占布局宽度。
//!
//! ⑧⑨ 都是这扇新门第一次真正跑通布局时抓出来的：旧件的 `c_columnate`/`r_columnate`
//! 把条目、制表符、换行当三段各自返回，壳侧又逐段补一个换行，于是纵向/横向填满两条
//! 布局从来没打出过一行像样的输出——以前没有进程级门，没有任何测试看得见这一点。
//!
//! 这扇门前不存在：`column` 的四类出口只有自拟的一行 usage，残行那一句连影子都没有。

mod face_support;

use face_support::write_stdin;
use std::fs;
use std::process::{Command, Stdio};

const COLUMN: &str = env!("CARGO_BIN_EXE_column");
/// `column.c:304-305` 的那一行：大写 `Usage`、程序名跟着 `getprogname()`。
const USAGE: &str = "Usage: column [-tx] [-c columns] [-s sep] [file ...]\n";
/// 参照件在无 `-c` 时用的默认宽度 80 下的布局（五条 5 字节条目一行放得下）。
/// 逐字节比三条通道的夹具：`col`/`rev`/`colrm`/`column` 这一族要钉的都是
/// 「字母位是一个裸字节」，而 `String::from_utf8_lossy` 会把裸 `c3` 与 `c3 83`
/// 都渲染成替换字符，文本夹具分不开这两种错法。
fn run_bytes(args: &[&str], input: &[u8]) -> (Vec<u8>, i32, Vec<u8>) {
    let mut child = Command::new(COLUMN)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the column binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        output.stdout,
        output.status.code().unwrap_or(-1),
        output.stderr,
    )
}

/// 集外字母那一行的字母位是**一个裸字节**：参照件 `tmp/column-314/column`
/// （`column.c` 真源 + Minix 的 `getopt.c`，经软链起，程序名与本壳一致）现量
/// `column -é` 与 `column -té` 都打 `unknown option -- ` + `c3` + 换行 + usage、退 1
/// （optstring 是 `"c:s:tx"`，`t` 合法所以聚簇继续吃到 `é` 的第一字节）。
/// ASCII 的 `-Z` 与缺值的 `-c` 两形同时钉住：换渲染不许改动它们。
#[test]
fn the_option_line_of_a_nonascii_letter_carries_one_raw_byte() {
    let want: Vec<u8> = [
        b"column: unknown option -- ".as_slice(),
        b"\xc3\n",
        USAGE.as_bytes(),
    ]
    .concat();
    for form in ["-\u{e9}", "-t\u{e9}"] {
        let (out, code, err) = run_bytes(&[form], FIVE);
        assert_eq!(out, Vec::new(), "{form} 的报文只走 stderr");
        assert_eq!(code, 1, "{form} 的 stderr 是 {err:?}");
        assert_eq!(err, want, "{form} 的字母该是一个裸字节 c3");
    }

    let (_out, code, err) = run_bytes(&["-Z"], FIVE);
    assert_eq!(code, 1);
    assert_eq!(
        err,
        b"column: unknown option -- Z\nUsage: column [-tx] [-c columns] [-s sep] [file ...]\n"
            .to_vec()
    );

    let (_out, code, err) = run_bytes(&["-c"], FIVE);
    assert_eq!(code, 1);
    assert_eq!(
        err,
        b"column: option requires an argument -- c\nUsage: column [-tx] [-c columns] [-s sep] [file ...]\n".to_vec()
    );
}

const FIVE: &[u8] = b"alpha\nbeta\ngamma\ndelta\nepsilon\n";
const FIVE_LINE: &str = "alpha\tbeta\tgamma\tdelta\tepsilon\n";

fn run(args: &[&str], input: &[u8]) -> (String, i32, String) {
    let mut child = Command::new(COLUMN)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the column binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// 集外字母：libc 那一行在前、`usage()` 在后，退 1（`column.c:107-109`）。
/// 旧写法只有 usage 一行，libc 那一行整个缺失。
#[test]
fn an_unknown_letter_prints_the_libc_line_then_the_usage() {
    let (out, status, err) = run(&["-Z"], FIVE);
    assert_eq!((out.as_str(), status), ("", 1), "stderr 是 {err:?}");
    assert_eq!(err, format!("column: unknown option -- Z\n{USAGE}"));

    // 聚簇只报第一个集外字母（参照件：`-tZ` → `Z`）。
    let (_, status, err) = run(&["-tZ"], FIVE);
    assert_eq!(status, 1);
    assert_eq!(err, format!("column: unknown option -- Z\n{USAGE}"));

    // `--x` 报的字母是第二个连字符：optstring 里没有 `-`。
    let (_, status, err) = run(&["--x"], FIVE);
    assert_eq!(status, 1);
    assert_eq!(err, format!("column: unknown option -- -\n{USAGE}"));
}

/// 带值项后面没有那一段是 libc 的缺值句 + usage，**不是**本命令自拟的一句
/// （`getopt.c:132-136` 先印，`column.c:107-109` 再 usage）。
#[test]
fn a_missing_option_value_prints_the_libc_line_then_the_usage() {
    for letter in ['c', 's'] {
        let (out, status, err) = run(&[&format!("-{letter}")], FIVE);
        assert_eq!((out.as_str(), status), ("", 1));
        assert_eq!(
            err,
            format!("column: option requires an argument -- {letter}\n{USAGE}"),
            "缺值那支是两句、程序名跟着 `getprogname()`"
        );
    }
    // 聚簇里落在串尾的 `-c` 同样走这一支。
    let (_, status, err) = run(&["-tc"], FIVE);
    assert_eq!(status, 1);
    assert_eq!(
        err,
        format!("column: option requires an argument -- c\n{USAGE}")
    );
}

/// `-c` 的值是 C 的 `atoi`（`column.c:96` 无守卫）：非正或非数字都不是错，
/// 只是宽度塌到「每条一行」，退出码仍是 0。旧写法把这三种都判成 usage 退 1。
#[test]
fn an_unparsable_width_is_not_an_error() {
    let each_line = "alpha\nbeta\ngamma\ndelta\nepsilon\n";
    for word in ["0", "abc", "-5", ""] {
        let args = if word.is_empty() {
            vec!["-c", ""]
        } else {
            vec!["-c", word]
        };
        let (out, status, err) = run(&args, FIVE);
        assert_eq!(
            (out.as_str(), status),
            (each_line, 0),
            "`-c {word}` 的 stderr 是 {err:?}"
        );
        assert_eq!(err, "");
    }
    // 尾巴上的杂字符 `atoi` 不管：`-c20x` 是宽度 20（参照件同受）。
    let (out, status, _) = run(&["-c20x"], FIVE);
    assert_eq!(status, 0);
    assert_eq!(out, "alpha\tdelta\nbeta\tepsilon\ngamma\n");
}

/// 旗标逐字母生效、值可以附在本串里：`-tx` 与 `-t -c12 -s,` 都不该退化成操数。
#[test]
fn flags_cluster_and_values_attach() {
    // 宽度 12 时 `numcols = 12 / 8 = 1`，`-x` 的横向填满也就是一条一行。
    let (out, status, err) = run(&["-tx", "-c", "12"], FIVE);
    assert_eq!(
        (out.as_str(), status),
        ("alpha\nbeta\ngamma\ndelta\nepsilon\n", 0)
    );
    assert_eq!(err, "");
    // `-s` 的分离器只影响 `-t` 的字段切分：附值形与分离形同解。
    let tabbed = b"aa,bb,cc\ndddddee,ff,ggg\n";
    let (attached, _, _) = run(&["-t", "-s,"], tabbed);
    let (separate, _, _) = run(&["-t", "-s", ","], tabbed);
    assert_eq!(attached, separate);
    assert_eq!(attached, "aa       bb  cc\ndddddee  ff  ggg\n");
}

/// 不重排：第一个非选项 token 一到，选项环当场结束，它后面的 `-t` 是**档名**。
/// 参照件现量 `column: Cannot open `-t': No such file or directory`，退 1。
#[test]
fn the_option_ring_stops_at_the_first_operand() {
    let dir = scratch("reorder");
    let file = dir.join("in.txt");
    fs::write(&file, FIVE).expect("write the operand file");
    let (out, status, err) = run(&[file.to_str().unwrap(), "-t"], b"should not be read\n");
    assert_eq!(status, 1);
    assert_eq!(err, "column: Cannot open `-t': No such file or directory\n");
    // 条目池里只有那个档的五条（`-t` 没被当旗标吃掉，所以仍是纵向填满的布局）。
    assert_eq!(out, FIVE_LINE);
}

/// 打不开的操数是三段形 `warn(3)` 而**不是致命错**：跳过它继续下一个档，
/// 退出码置 1（`column.c:119-122`）。
#[test]
fn an_unopenable_operand_warns_and_the_run_continues() {
    let dir = scratch("continue");
    let file = dir.join("good.txt");
    fs::write(&file, b"alpha\nbeta\n").expect("write the operand file");
    let (out, status, err) = run(&[file.to_str().unwrap(), "no-such-file"], b"ignored\n");
    assert_eq!(status, 1);
    assert_eq!(
        err, "column: Cannot open `no-such-file': No such file or directory\n",
        "三段形：程序名、操数原文、`strerror` 句子"
    );
    assert_eq!(out, "alpha\tbeta\n");

    // 坏档在前也一样：好档的条目照样进池。
    let (out, status, _) = run(&["no-such-file", file.to_str().unwrap()], b"");
    assert_eq!((out.as_str(), status), ("alpha\tbeta\n", 1));
}

/// 裸 `-` 是档名不是标准输入的别名（`column.c:115` 直接 `fopen`）：
/// 参照件现量 `column: Cannot open `-': No such file or directory`，标准输入不被读。
#[test]
fn a_solitary_dash_is_a_file_name_not_stdin() {
    let (out, status, err) = run(&["-"], FIVE);
    assert_eq!((out.as_str(), status), ("", 1));
    assert_eq!(err, "column: Cannot open `-': No such file or directory\n");
}

/// `--` 结束选项环而自身不是操数：`column` 因此在 `!*argv` 那一判上回落读标准输入
///（参照件 `--` 现量退 0 且输出标准输入的布局）；`--` 后面的 token 才是档名。
#[test]
fn the_double_dash_ends_the_ring_without_becoming_an_operand() {
    let (out, status, err) = run(&["--"], FIVE);
    assert_eq!((out.as_str(), status), (FIVE_LINE, 0));
    assert_eq!(err, "");

    let dir = scratch("dashdash");
    let file = dir.join("named.txt");
    fs::write(&file, b"one\ttwo\n").expect("write the operand file");
    // `--` 之后的 `-c` 是字面档名（开不了 → warn），标准输入不被读。
    let (out, status, err) = run(&["--", "-c", file.to_str().unwrap()], FIVE);
    assert_eq!(status, 1);
    assert_eq!(err, "column: Cannot open `-c': No such file or directory\n");
    // 没有 `-t`，条目原样进布局：里面那个制表符不会被换成补齐的空格。
    assert_eq!(out, "one\ttwo\n");
}

/// 末行没有换行符：C 判「长行」，丢这一条、打一句两段形的 `warnx`、退出码置 1
///（`column.c:277-283`）。整条是空白的末行按空行处理，一句警告都没有。
#[test]
fn an_unterminated_last_line_warns_and_is_dropped() {
    let (out, status, err) = run(&[], b"one\ntwo");
    assert_eq!((out.as_str(), status), ("one\n", 1));
    assert_eq!(
        err, "column: line too long\n",
        "`warnx` 不缀 strerror，所以只有两段"
    );

    // 空白末行：没有条目被丢，也就没有警告。
    let (out, status, err) = run(&[], b"one\n   ");
    assert_eq!((out.as_str(), status, err.as_str()), ("one\n", 0, ""));

    // 换行结尾的输入一切正常。
    let (out, status, err) = run(&[], b"one\ntwo\n");
    assert_eq!((out.as_str(), status, err.as_str()), ("one\ttwo\n", 0, ""));
}

/// 条目池空则按 `eval` 直接返回（`column.c:125-127`）：全空白的输入加一个坏档，
/// 只有 warn 那一行、没有输出，退码 1。
#[test]
fn an_empty_pool_keeps_only_the_exit_status() {
    let (out, status, err) = run(&["no-such-file"], b"\n  \n\t\n");
    assert_eq!((out.as_str(), status), ("", 1));
    assert_eq!(
        err,
        "column: Cannot open `no-such-file': No such file or directory\n"
    );

    let (out, status, err) = run(&[], b"\n  \n");
    assert_eq!((out.as_str(), status, err.as_str()), ("", 0, ""));
}

/// 布局由宽度决定：最长条目取齐到 8 之后达到宽度就退成每条一行，
/// 否则纵向填满（`column.c:128-137`）。
#[test]
fn the_layout_follows_the_width() {
    // 宽度 8 而条目长 5（取齐到 8）：`maxlength >= termwidth` 走每条一行。
    let (out, status, err) = run(&["-c", "8"], FIVE);
    assert_eq!(status, 0);
    assert_eq!(err, "");
    assert_eq!(out, "alpha\nbeta\ngamma\ndelta\nepsilon\n");
    // `-x` 换成横向填满。
    let (out, _, _) = run(&["-x", "-c", "24"], FIVE);
    assert_eq!(out, "alpha\tbeta\tgamma\ndelta\tepsilon\n");
}

/// 条目是整行：前导空白跟着进池、也跟着进布局。参照件现量
/// `  lead` + 两个补齐制表符 + 以制表符开头的那一条 + 一个补齐制表符 + `plain`，
/// 所以连着三个制表符里有一个属于条目本身——剥掉前导空白就少算一格宽度。
#[test]
fn an_entry_keeps_its_leading_blanks() {
    let (out, status, err) = run(&[], b"  lead\n\ttablead\nplain\n");
    assert_eq!(
        (out.as_str(), status),
        ("  lead\t\t\ttablead\tplain\n", 0),
        "stderr 是 {err:?}"
    );
    assert_eq!(err, "");
}

fn scratch(tag: &str) -> std::path::PathBuf {
    static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("column-face-{}-{tag}-{n}", std::process::id()));
    fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

/// 超长残行的告警按**来源计数**：两个来源各带一条残行 ⇒ 两句，同一个来源写两遍 ⇒ 还是两句
/// （一次打开一次计数），好来源 + 残行来源 ⇒ 一句，`-` 这类开不了的来源先一句 `Cannot open`
/// 再按它的残行补一句。下面四段的句数与退码全部取自参照件 `tmp/column-314/column_ref` 的现量。
#[test]
fn each_source_with_a_long_tail_line_warns_once() {
    let dir = std::env::temp_dir().join(format!("column-face-tail-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("临时目录该能建");
    let long = "z".repeat(70);
    let one = dir.join("tail1.txt");
    let two = dir.join("tail2.txt");
    let good = dir.join("ok.txt");
    fs::write(&one, format!("a b c\n{long}")).expect("写第一档");
    fs::write(&two, format!("d e f\n{long}")).expect("写第二档");
    fs::write(&good, "a b\nc d\n").expect("写好档");
    let one = one.to_string_lossy().into_owned();
    let two = two.to_string_lossy().into_owned();
    let good = good.to_string_lossy().into_owned();

    // 两档各一条残行 ⇒ 两句，退 1
    let (_, code, err) = run(&["-c", "20", one.as_str(), two.as_str()], b"x y\n");
    assert_eq!(code, 1);
    assert_eq!(
        err.matches("column: line too long\n").count(),
        2,
        "两个来源各该报一句，实际 stderr={err:?}"
    );
    // 同一档给两遍 ⇒ 两次打开、还是两句
    let (_, _, err) = run(&["-c", "20", one.as_str(), one.as_str()], b"x y\n");
    assert_eq!(err.matches("column: line too long\n").count(), 2);
    // 好档在前、残行档在后 ⇒ 只有一句
    let (_, code, err) = run(&["-c", "20", good.as_str(), one.as_str()], b"x y\n");
    assert_eq!(code, 1);
    assert_eq!(
        err.matches("column: line too long\n").count(),
        1,
        "实际 {err:?}"
    );
    // `-` 不是标准输入的别名：C 先报开不了，再按残行档补一句
    let (_, code, err) = run(&["-c", "20", "-", one.as_str()], b"x y\n");
    assert_eq!(code, 1);
    assert_eq!(
        err.lines().next().unwrap_or(""),
        "column: Cannot open `-': No such file or directory"
    );
    assert_eq!(
        err.matches("column: line too long\n").count(),
        1,
        "实际 {err:?}"
    );
    let _ = fs::remove_dir_all(&dir);
}
