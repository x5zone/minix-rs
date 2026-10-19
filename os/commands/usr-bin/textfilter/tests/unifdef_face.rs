//! End-to-end face for the `unifdef` binary, checked against
//! `minix3/usr.bin/unifdef/unifdef.c`（选项环 `:273-323`、符号判定 `addsym`
//! `:965-991`、操数与开档 `:327-341`、`usage()` `:373-379`）与它驱动的 libc `getopt`
//!（`minix3/lib/libc/stdlib/getopt.c:74-78`、`:81-92`、`:95-104`、`:118-141`）。
//!
//! `unifdef.c:273` 的 optstring 是 `"i:D:U:I:o:cdeklst"`：`D`/`U`/`i`/`I`/`o` 五个带值、
//! `c`/`d`/`e`/`k`/`l`/`s`/`t` 七个不带值，而且大小写算不同字母（`-uD` 的 `u` 集外、
//! `-tE` 的 `E` 集外）。出口有三类，且**本支的 usage 退的是 2**（`exit(2)`，不是同族常见的 1）：
//! ① 集外字母与缺值各由 libc 先印一行，再 `default: usage()` 接一行；
//! ② `-i` 的后随字母不是 `D`/`U`、`-D`/`-U` 的符号名不合 `addsym` 的形——两处 C 都**只打
//!    usage 一行**，没有任何自己的句子（本壳改前给这两处自拟了 `-i must be followed by D or U`
//!    与 `bad symbol spec …`，多出一行）；
//! ③ 环结束后先查「一个符号都没有且没给 `-s`」= `warnx` 一句 + usage 两行，这一支排在
//!    操数判定之前，所以 `unifdef -- -z` 报的是它而不是「只能一个档」。
//!
//! 另三处只有端到端才看得见：`-D ''` 与 `-U ''` 是**合法**的空符号名（`skipsym` 一个字符
//! 也不吃就返回原点，于是 `*val == '\0'`），整档原样透传退 0；操数只许一个且环**不重排**
//! （`unifdef -DA in.c -c` 的 `-c` 是第二个操数，退 2 而不是被当旗标吃掉）；开不了的操数档名
//! 是 `err(2, "can't open %s", …)` 三段形。`-i` 那一支先吃一个字符再看剩下那段，所以
//! `-iDA` 的符号是 `A`（与 `-D A` 逐字节同效），不是 `DFOO` 那样的整串。
//!
//! `-s` 打出的控制符号清单与 `-d` 的调试行都不在这扇门的断言里：前者由 `findsym` 在查表的
//! 同时 `printf`（`unifdef.c:930-931`）、后者是 `debug()` 往标准错误上刷，两支都属决定半，
//! 已登记为 `UNIFDEF-SYMLIST-OUTPUT` 与 `UNIFDEF-D-DEBUG-OUTPUT`。

mod face_support;

use face_support::write_stdin;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};

const UNIFDEF: &str = env!("CARGO_BIN_EXE_unifdef");
const USAGE: &str = "usage: unifdef [-cdeklst] [-o output] [-Dsym[=val]] [-Usym] [-iDsym[=val]] [-iUsym] ... [file]\n";
const SOURCE: &[u8] = b"#ifdef A\nint a;\n#endif\n#ifdef B\nint b;\n#endif\n";
/// `-DA` 把 A 那一组解开：`#ifdef A` 与它的 `#endif` 消失，`int a;` 留下，B 那一组原样。
const RESOLVED_A: &str = "int a;\n#ifdef B\nint b;\n#endif\n";

fn run(args: &[&str], input: &[u8]) -> (String, i32, String) {
    let mut child = Command::new(UNIFDEF)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the unifdef binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("unifdef318-{tag}-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("the scratch directory should be creatable");
    dir
}

/// 与 `run` 同一条命令缝，但**逐字节**交回：`from_utf8_lossy` 把一个裸 `c3` 渲染成三字节
/// U+FFFD，文本形分不清「一个字节」与「两个字节」这两种错法（`rev`/`paste`/`jot` 那几支同此）。
fn run_bytes(args: &[&str], input: &[u8]) -> (Vec<u8>, i32, Vec<u8>) {
    let mut child = Command::new(UNIFDEF)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the unifdef binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        output.stdout,
        output.status.code().unwrap_or(-1),
        output.stderr,
    )
}

/// 集外字母那一行的字母位是**一个裸字节**：参照件 `tmp/unifdef-318/unifdef_ref` 现量
/// `-é` 与 `-cé` 都是 `unifdef: unknown option -- ` + `c3` + 换行再跟 usage、退 2
/// （`c` 是 `"i:D:U:I:o:cdeklst"` 里合法且不带值的字母，聚簇继续吃到 `é` 的第一字节；
/// `-Zé` 停在 `Z`，`unifdef.c:322-323` 拿到 `'?'` 就 `usage()`）。ASCII 的 `-z` 与「聚簇只报
/// 第一枚」两形本文件各已有主人（`an_unknown_letter_prints_the_libc_line_then_the_usage`、
/// `the_case_of_a_letter_is_part_of_the_option_set`），按冗余判据不重复承载。
#[test]
fn a_multibyte_flag_letter_reaches_the_libc_line_as_one_byte() {
    let want: Vec<u8> = [
        b"unifdef: unknown option -- ".as_slice(),
        b"\xc3\n",
        USAGE.as_bytes(),
    ]
    .concat();
    for form in ["-\u{e9}", "-c\u{e9}"] {
        let (out, status, err) = run_bytes(&[form], SOURCE);
        assert_eq!(out, Vec::new(), "{form} 的报文只走 stderr");
        assert_eq!(status, 2, "{form} 的 stderr 是 {err:?}");
        assert_eq!(err, want, "{form} 的字母该是一个裸字节 c3");
    }

    let (_, status, err) = run_bytes(&["-Z\u{e9}"], SOURCE);
    assert_eq!(status, 2);
    assert_eq!(
        err,
        [
            b"unifdef: unknown option -- Z\n".as_slice(),
            USAGE.as_bytes(),
        ]
        .concat(),
        "环停在第一个坏字母上，`é` 根本不该被读到"
    );
}

/// 集外字母：libc 那一行在前、usage 在后，退 **2**（`unifdef.c:322-323` 加 `:378`）。
#[test]
fn an_unknown_letter_prints_the_libc_line_then_the_usage() {
    let (out, status, err) = run(&["-z"], SOURCE);
    assert_eq!(out, "");
    assert_eq!(status, 2, "stderr 是 {err:?}");
    assert_eq!(err, format!("unifdef: unknown option -- z\n{USAGE}"));
}

/// 聚簇只报第一个坏字母；而字母的大小写是集合的一部分：`u` 与 `E` 都不在串里。
#[test]
fn the_case_of_a_letter_is_part_of_the_option_set() {
    for (args, letter) in [
        ("-cz", "z"),
        ("-ZD", "Z"),
        ("--x", "-"),
        ("-tE", "E"),
        ("-uD", "u"),
    ] {
        let (out, status, err) = run(&[args], SOURCE);
        assert_eq!(
            (out.as_str(), status),
            ("", 2),
            "`{args}` 的 stderr 是 {err:?}"
        );
        assert_eq!(err, format!("unifdef: unknown option -- {letter}\n{USAGE}"));
    }
}

/// 五个带值字母缺值同形（含聚簇落在串尾的 `-cD`、`-lI`）：libc 那句 + usage，退 2。
#[test]
fn each_value_letter_without_its_value_prints_the_libc_line() {
    for letter in ['D', 'U', 'i', 'I', 'o'] {
        let flag = format!("-{letter}");
        let (out, status, err) = run(&[&flag], SOURCE);
        assert_eq!(
            (out.as_str(), status),
            ("", 2),
            "{flag} 的 stderr 是 {err:?}"
        );
        assert_eq!(
            err,
            format!("unifdef: option requires an argument -- {letter}\n{USAGE}"),
            "缺值这一支是两句，程序名跟着 `getprogname()`"
        );
    }
    for (cluster, letter) in [("-cD", "D"), ("-lI", "I")] {
        let (_, status, err) = run(&[cluster], SOURCE);
        assert_eq!(status, 2);
        assert_eq!(
            err,
            format!("unifdef: option requires an argument -- {letter}\n{USAGE}")
        );
    }
}

/// `-i` 的后随字符不是 `D`/`U` 时 C 只 `usage()`：既没有 libc 行，也没有自拟句子。
/// 空值（`-i` 配一个空 token）同形，因为 `*optarg` 当场就是 `\0`。
#[test]
fn a_rejected_ignore_form_is_the_usage_line_alone() {
    for args in [
        vec!["-iX"],
        vec!["-i", ""],
        vec!["-id"],
        vec!["-i", "-x"],
        vec!["-i", "-"],
    ] {
        let (out, status, err) = run(&args, SOURCE);
        assert_eq!(
            (out.as_str(), status),
            ("", 2),
            "`{args:?}` 的 stderr 是 {err:?}"
        );
        assert_eq!(err, USAGE, "只有 usage 一行");
    }
}

/// `addsym` 的形判定（`unifdef.c:978-990`）：`-D` 允许 `sym`、`sym=`、`sym=val`，
/// `-U` 只允许 `sym`；名字之后既不是结尾也不是 `=` 就是坏形，而坏形只打 usage 一行。
#[test]
fn a_rejected_symbol_form_is_the_usage_line_alone() {
    for args in [
        vec!["-UFOO=1"],
        vec!["-DFOO."],
        vec!["-D."],
        vec!["-U", "FOO."],
        vec!["-D", "FOO BAR"],
    ] {
        let (out, status, err) = run(&args, SOURCE);
        assert_eq!(
            (out.as_str(), status),
            ("", 2),
            "`{args:?}` 的 stderr 是 {err:?}"
        );
        assert_eq!(err, USAGE);
    }
}

/// 空符号名是合法的：`skipsym("")` 原地返回，`*val == '\0'` 于是走「值为空」那一支。
/// 这一枚盯的是旧写法把空名当坏形拒——C 收，而且整档原样透传退 0。
#[test]
fn an_empty_symbol_name_is_accepted() {
    for flag in ["-D", "-U"] {
        let (out, status, err) = run(&[flag, ""], SOURCE);
        assert_eq!(
            (out.as_str(), status),
            ("#ifdef A\nint a;\n#endif\n#ifdef B\nint b;\n#endif\n", 0),
            "`{flag}` 配空符号名的 stderr 是 {err:?}"
        );
        assert_eq!(err, "");
    }
}

/// 一个符号都没有：`warnx` 那句 + usage，退 2；而这一支排在操数判定**之前**，
/// 所以 `-- -z`（两个 token 都在环后）报的还是它。给 `-s` 就不需要符号（清单模式）。
#[test]
fn a_missing_symbol_warns_then_prints_the_usage() {
    for args in [
        vec![],
        vec!["-c"],
        vec!["-l", "-t"],
        vec!["--"],
        vec!["--", "-z"],
    ] {
        let (out, status, err) = run(&args, SOURCE);
        assert_eq!(
            (out.as_str(), status),
            ("", 2),
            "`{args:?}` 的 stderr 是 {err:?}"
        );
        assert_eq!(
            err,
            format!("unifdef: must -D or -U at least one symbol\n{USAGE}")
        );
    }
    // `-s` 单独给是合法的：这一枚只断言「不走那句告警」，清单内容归决定半的登记项。
    let (_, status, err) = run(&["-s"], SOURCE);
    assert!(!err.contains("must -D or -U"), "`-s` 不该撞那句: {err:?}");
    assert_eq!(status, 0);
}

/// 操数只许一个，且环**不重排**：第一个非选项 token 之后的 `-c` 是第二个操数
/// （参照件：`unifdef -DA in.c -c` → `can only do one file`），句子是 `errx` 的单独一行。
#[test]
fn only_one_operand_is_taken_and_the_ring_does_not_rearrange() {
    let dir = scratch("operands");
    let file = dir.join("in.c");
    fs::write(&file, SOURCE).expect("write in.c");
    let path = file.to_str().unwrap();
    for args in [
        vec!["-DA", path, "extra"],
        vec!["-DA", path, "-c"],
        vec!["-DA", path, path],
    ] {
        let (out, status, err) = run(&args, SOURCE);
        assert_eq!(
            (out.as_str(), status),
            ("", 2),
            "`{args:?}` 的 stderr 是 {err:?}"
        );
        assert_eq!(
            err, "unifdef: can only do one file\n",
            "只有这一行，不接 usage"
        );
    }
}

/// 开不了的操数档名是 `err(2, "can't open %s", filename)` 三段形：程序名、操数原文、
/// `strerror` 句子，三者缺一不可（旧写法少了第三段）。
#[test]
fn an_unopenable_operand_is_a_three_part_error() {
    let (out, status, err) = run(&["-DA", "./nope-318.c"], SOURCE);
    assert_eq!((out.as_str(), status), ("", 2));
    assert_eq!(
        err, "unifdef: can't open ./nope-318.c: No such file or directory\n",
        "三段形：程序名、操数原文、`strerror` 句子"
    );
}

/// 裸 `-` 是标准输入的别名（`unifdef.c:333` 的 `strcmp(*argv, "-") != 0`），
/// 与「根本没有操数」同效；这是本支的读缝——改前它走裸 `minix_sys::read`，宿主上读不到。
#[test]
fn a_dash_operand_reads_stdin_like_no_operand_at_all() {
    let (out, status, err) = run(&["-DA"], SOURCE);
    assert_eq!((out.as_str(), status), (RESOLVED_A, 1), "stderr 是 {err:?}");
    assert_eq!(err, "");

    let (out, status, err) = run(&["-DA", "-"], SOURCE);
    assert_eq!((out.as_str(), status), (RESOLVED_A, 1), "stderr 是 {err:?}");
    assert_eq!(err, "");
}

/// `-i` 先吃掉一个字符再取符号名（`opt = *optarg++`），所以 `-iDA` 的符号是 `A`；
/// 两个不相关的符号名（`DFOO`）都点不起任何一支，于是整档透传退 0。
#[test]
fn the_ignore_letter_is_consumed_before_the_symbol_name() {
    let (out, status, err) = run(&["-iDA"], SOURCE);
    assert_eq!((out.as_str(), status), (RESOLVED_A, 1), "stderr 是 {err:?}");
    assert_eq!(err, "", "`-iDA` 与 `-D A` 同效：符号是 A，不是 DFOO");

    let (out, status, _err) = run(&["-iDFOO"], SOURCE);
    assert_eq!(
        (out.as_str(), status),
        (std::str::from_utf8(SOURCE).unwrap(), 0)
    );

    // `-iD=1` 的符号名是那一段的**开头**（这里开头就是 `=`，一个标识符字符也没有），
    // 于是它谁也匹配不上：整档透传退 0。这一形两边逐字节同（参照件现量）。
    let (out, status, _err) = run(&["-iD=1"], SOURCE);
    assert_eq!(
        (out.as_str(), status),
        (std::str::from_utf8(SOURCE).unwrap(), 0),
        "`-iD=1` 是合法的「空名字挂个值」，不是坏形"
    );
}

/// 重复给同一个符号不报错也不叠两份：C 的 `findsym` 找到同一格覆写值/ignore。
#[test]
fn a_repeated_symbol_updates_the_same_slot() {
    let (out, status, err) = run(&["-DA", "-DA"], SOURCE);
    assert_eq!((out.as_str(), status), (RESOLVED_A, 1), "stderr 是 {err:?}");
    let (out, status, _err) = run(&["-DB", "-DA"], SOURCE);
    assert_eq!(
        (out.as_str(), status),
        ("int a;\nint b;\n", 1),
        "两个符号各管各的那一组"
    );
}

/// `-o` 把结果写进档，退码仍按「有没有改过」给；这一枚只钉住「标准输出不再出内容」
/// 与档里确有内容——写失败那一句缺 `strerror` 一段，已登记为 `UNIFDEF-O-ERRNO-LEG`。
#[test]
fn the_output_operand_takes_the_text_and_stdout_stays_empty() {
    let dir = scratch("output");
    let out_path = dir.join("out.c");
    let path = out_path.to_str().unwrap();
    let (out, status, err) = run(&["-DA", "-o", path], SOURCE);
    assert_eq!((out.as_str(), status), ("", 1), "stderr 是 {err:?}");
    assert_eq!(err, "");
    assert_eq!(
        fs::read_to_string(&out_path).expect("read out.c"),
        RESOLVED_A
    );
}
