//! End-to-end face for the `paste` binary, checked against
//! `minix3/usr.bin/paste/paste.c`（选项环 `:70-83`、操作数下沉 `:85-92`、
//! `parallel()` `:95-155`、`sequential()` `:161-195`、`tr()` `:199-226`、`usage()` `:229-233`）
//! 与它驱动的 libc `getopt`（`minix3/lib/libc/stdlib/getopt.c:74-78`、`:81-85`、`:95-104`、`:112-141`）。
//!
//! `paste.c:70` 的 optstring 是 `"d:s"`：只有 `d` 带值。参照件 `tmp/paste-321/paste_ref`
//! （`paste.c` 真源 + Minix 的 `getopt.c` + 程序名垫片 + 一个 BSD `fgetln` 垫片，宿主现链）
//! 逐形量出的形状是：
//! ① 环在**第一个**坏字母处退出（`case '?': default: usage();` 就坐在 `switch` 里），
//!    所以 `paste -sZ` 与 `paste -ZQ` 都只有一行 libc 再加一行 usage，退 1——这与同族
//!    `seq`（BADCH 之后环继续扫、报文累加）正相反；`jot` 与本支同形，参照件 `-ZQ` 只报 `Z`；
//! ② 错误的**先后**就是扫描的先后：`paste -d '' -Z` 报「没有分隔符」（`tr()` 在 `case 'd'`
//!    里就地跑），`paste -Z -d ''` 报集外字母；
//! ③ usage 那行的程序名写死在串里（`paste.c:231`），libc 那行跟着 `getprogname()`；
//! ④ 操作数**没有个数判**：`paste` 不带操作数不读标准输入、不打任何东西、退 0；
//! ⑤ 环不重排：`paste a.txt -s` 的 `-s` 是第二个档名 ⇒ 并行的 `err(1, "%s", …)` 致命；
//! ⑥ 开不了的档名在两种模式下走两条出口：并行**致命退 1**，顺序 `warn` + 继续且整体**退 0**；
//! ⑦ `-d` 的表按字节计数，`\n`/`\t`/`\0` 三个转义，认不出的转义**丢掉反斜杠**（与 `seq` 相反），
//!    值为 0 的那项占一位但什么都不打；并行的游标每行从**表尾那个 NUL** 起，
//!    某列的流见底时后面的列会为它**多补一个分隔符**（`paste -d '+:' a b a` 的第三行是 `3+:3`）。
//!
//! **第十一处「薄壳裸 `open`/`read` 宿主读不到输入」**：本支原先拿裸 `minix_sys::open` 读
//! 操作数档，宿主上任何档名都开不了 ⇒ 并行一律退 1、顺序一律 warn，操作数形状从来没被
//! 进程级测过。现改走 `bin_support` 的双生面（`read_file`/`read_stdin`）。

mod face_support;

use face_support::write_stdin;
use std::ffi::OsString;
use std::fs;
use std::os::unix::ffi::OsStringExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const PASTE: &str = env!("CARGO_BIN_EXE_paste");
/// `paste.c:231` 的那一行：程序名写死在串里，不给插值。
const USAGE: &str = "paste: [-s] [-d delimiters] file ...\n";

fn run(args: &[&str], input: &[u8]) -> (String, i32, String) {
    let mut child = Command::new(PASTE)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the paste binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// 与 `run` 同一条命令缝，但**逐字节**交回：文本形经 `from_utf8_lossy` 会把一个裸
/// `c3` 变成三字节 U+FFFD，于是「一个字节」与「两个字节」在 `String` 里看不出差别。
/// 批次四百零一之前本文件没有这类需求——所有期望都是 ASCII 报文。
fn run_bytes(args: &[&str], input: &[u8]) -> (Vec<u8>, i32, Vec<u8>) {
    let mut child = Command::new(PASTE)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the paste binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        output.stdout,
        output.status.code().unwrap_or(-1),
        output.stderr,
    )
}

/// 同一条命令缝，但 argv **逐字节**交回：`OsString::from_vec` 不经 UTF-8 校验，
/// 所以一枚裸 `ff` 能原样到达 `paste` 的 `argv`——这是 `bin_support::args_bytes` 那条腿
/// 唯一的可观察面（`run`/`run_bytes` 的 `&str` 形只能交出合法字节）。
fn run_raw(args: &[Vec<u8>], input: &[u8]) -> (Vec<u8>, i32, Vec<u8>) {
    let mut child = Command::new(PASTE)
        .args(args.iter().map(|raw| OsString::from_vec(raw.clone())))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the paste binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        output.stdout,
        output.status.code().unwrap_or(-1),
        output.stderr,
    )
}

fn scratch(tag: &str) -> PathBuf {
    let mut dir = std::env::temp_dir();
    dir.push(format!("paste321-{}-{}", tag, std::process::id()));
    fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

/// 三个操作数档：三行、两行、空档（参照件用的同一套内容）。
fn operands(tag: &str) -> (String, String, String) {
    let dir = scratch(tag);
    let a = dir.join("a.txt");
    let b = dir.join("b.txt");
    let c = dir.join("c.txt");
    fs::write(&a, b"1\n2\n3\n").expect("write a.txt");
    fs::write(&b, b"a\nb\n").expect("write b.txt");
    fs::write(&c, b"").expect("write c.txt");
    (path(&a), path(&b), path(&c))
}

fn path(file: &Path) -> String {
    file.to_str().expect("utf-8 temp path").to_string()
}

#[test]
fn off_letter_gets_one_libc_line_and_the_hardcoded_usage() {
    for letter in ['Z', 'h', '0'] {
        let flag = format!("-{letter}");
        let (out, code, err) = run(&[flag.as_str()], b"");
        assert_eq!(code, 1, "{flag} 退 1（`usage()` 自己 `exit(1)`）");
        assert_eq!(out, "", "{flag}");
        assert_eq!(
            err,
            format!("paste: unknown option -- {letter}\n{USAGE}"),
            "{flag}"
        );
    }
}

/// 集外字母那一行的字母位是**一个裸字节**：参照件 `tmp/paste-321/paste_ref` 现量
/// `paste -é` 与 `paste -sé` 都是 `paste: unknown option -- ` + `c3` + 换行，再跟那行
/// 写死的 usage、退 1（机制在 `getopt.c:79` 的 `optopt = *place++` 与 `:103` 的 `%c`）。
/// `s` 是 `paste.c:70` 的 optstring `"d:s"` 里的合法字母，所以聚簇会继续吃到 `é` 的第一字节；
/// `-Zé` 反而停在 `Z` 上——`paste.c:79-81` 的 `default: usage()` 坐在环里，第二个字母轮不到，
/// 这一形也是「多字节旗标根本进不到字母位」的正面证据。ASCII 的 `-Z` 与缺值的 `-d` 两形
/// 本文件各已有主人（`off_letter_gets_one_libc_line_and_the_hardcoded_usage`、
/// `missing_value_for_d_is_a_libc_line_then_usage`），这里不重复承载同一件事。
#[test]
fn the_libc_line_of_a_multibyte_flag_ends_at_its_first_byte() {
    let multibyte: Vec<u8> = [
        b"paste: unknown option -- ".as_slice(),
        b"\xc3\n",
        USAGE.as_bytes(),
    ]
    .concat();
    for form in ["-\u{e9}", "-s\u{e9}"] {
        let (out, code, err) = run_bytes(&[form], b"");
        assert_eq!(out, Vec::new(), "{form} 的报文只走 stderr");
        assert_eq!(code, 1, "{form} 的 stderr 是 {err:?}");
        assert_eq!(err, multibyte, "{form} 的字母该是一个裸字节 c3");
    }

    let (_, code, err) = run_bytes(&["-Z\u{e9}"], b"");
    assert_eq!(code, 1);
    assert_eq!(
        err,
        [b"paste: unknown option -- Z\n".as_slice(), USAGE.as_bytes(),].concat(),
        "环停在第一个坏字母上，`é` 根本不该被读到"
    );
}

#[test]
fn the_ring_stops_at_the_first_bad_letter() {
    // 与 `seq` 相反的形制：`usage()` 在环里被直接调用，报文**不累加**
    let (_, code, err) = run(&["-ZQ"], b"");
    assert_eq!(code, 1);
    assert_eq!(err, format!("paste: unknown option -- Z\n{USAGE}"));
    let (_, code, err) = run(&["-sZ", "a.txt"], b"");
    assert_eq!(code, 1);
    assert_eq!(
        err,
        format!("paste: unknown option -- Z\n{USAGE}"),
        "s 先被吃掉"
    );
}

#[test]
fn missing_value_for_d_is_a_libc_line_then_usage() {
    let (out, code, err) = run(&["-d"], b"");
    assert_eq!((code, out), (1, String::new()));
    assert_eq!(
        err,
        format!("paste: option requires an argument -- d\n{USAGE}")
    );
    // 聚簇里也一样：`s` 先被吃掉，缺值报的是 `d`
    let (_, code, err) = run(&["-sd"], b"");
    assert_eq!(code, 1);
    assert_eq!(
        err,
        format!("paste: option requires an argument -- d\n{USAGE}")
    );
}

#[test]
fn error_precedence_follows_the_scan() {
    // `-d ''` 在环内就撞上 `tr()` ⇒ 「没有分隔符」先报，后面的集外字母轮不到
    let (out, code, err) = run(&["-d", "", "-Z"], b"");
    assert_eq!((code, out), (1, String::new()));
    assert_eq!(
        err, "paste: no delimiters specified.\n",
        "句子自带句号且不带 strerror"
    );
    // 反过来坏字母在前时轮不到 `-d`
    let (_, code, err) = run(&["-Z", "-d", ""], b"");
    assert_eq!(code, 1);
    assert_eq!(err, format!("paste: unknown option -- Z\n{USAGE}"));
    // 紧贴的 `-d` 值也算（空值同样退 1）
    let (_, code, err) = run(&["-d", "\\q"], b"");
    assert_eq!((code, err), (0, String::new()), "零个操作数 ⇒ 不打任何东西");
}

#[test]
fn no_operand_reads_nothing_and_exits_zero() {
    // C 对零个操作数既不读标准输入也不报错（参照件对 stdin `L\n` 打空、退 0）
    let (out, code, err) = run(&[], b"L\n");
    assert_eq!((out, code, err), (String::new(), 0, String::new()));
    let (out, code, err) = run(&["--"], b"L\n");
    assert_eq!((out, code, err), (String::new(), 0, String::new()));
    let (out, code, err) = run(&["-s"], b"L\n");
    assert_eq!((out, code, err), (String::new(), 0, String::new()));
}

#[test]
fn operands_are_not_reordered() {
    let (a, _, _) = operands("reorder");
    // 环在第一个非选项 token 处收环且不推进 ⇒ `-s` 是**第二个档名**，并行的开档判致命
    let (out, code, err) = run(&[a.as_str(), "-s"], b"");
    assert_eq!((code, out), (1, String::new()));
    assert_eq!(err, "paste: -s: No such file or directory\n");
    // `--` 之后的 `-s` 同样是档名
    let (out, code, err) = run(&["--", "-s"], b"");
    assert_eq!((code, out), (1, String::new()));
    assert_eq!(err, "paste: -s: No such file or directory\n");
}

#[test]
fn unopenable_operand_is_fatal_in_parallel_and_a_warning_in_serial() {
    let (_, b, _) = operands("openface");
    // 并行：`err(1, "%s", argv[cnt])` 三段形、致命、退 1
    let (out, code, err) = run(&["nope.txt", b.as_str()], b"");
    assert_eq!(
        (code, out),
        (1, String::new()),
        "第一个开不了就退出，后面的档名不读"
    );
    assert_eq!(err, "paste: nope.txt: No such file or directory\n");
    // 顺序：`warn("%s", p)` 三段形、跳过并继续，整体退 0
    let (out, code, err) = run(&["-s", "nope.txt", b.as_str()], b"");
    assert_eq!(code, 0);
    assert_eq!(err, "paste: nope.txt: No such file or directory\n");
    assert_eq!(out, "a\tb\n", "坏档名跳过，好档名照旧并成一行");
    // 顺序模式里坏档名夹在中间也不改变后面的输出
    let (a, _, _) = operands("openface");
    let (out, code, err) = run(&["-s", b.as_str(), "nope.txt", a.as_str()], b"");
    assert_eq!(code, 0);
    assert_eq!(err, "paste: nope.txt: No such file or directory\n");
    assert_eq!(out, "a\tb\n1\t2\t3\n");
}

#[test]
fn dash_operand_names_stdin_and_is_shared_per_open() {
    // 两列都是 `-` ⇒ 同一个流、同一个游标：每行两列各取一行（参照件同形）
    let (out, code, err) = run(&["-", "-"], b"1\n2\n3\n");
    assert_eq!((code, err), (0, String::new()));
    assert_eq!(out, "1\t2\n3\t\n");
    // 单列 `-` 就是一路透传
    let (out, code, err) = run(&["-"], b"p\nq\n");
    assert_eq!((code, err), (0, String::new()));
    assert_eq!(out, "p\nq\n");
    // 顺序模式下第二个 `-` 已经没有行可取
    let (out, code, err) = run(&["-s", "-", "-"], b"1\n2\n");
    assert_eq!((code, err), (0, String::new()));
    assert_eq!(out, "1\t2\n");
}

#[test]
fn delimiter_escapes_and_nul_slots() {
    let (a, b, _) = operands("escapes");
    // `\q`：认不出的转义**丢掉反斜杠**（与 seq 的 unescape 相反）
    let (out, code, err) = run(&["-d", "\\q", a.as_str(), b.as_str()], b"");
    assert_eq!((code, err), (0, String::new()));
    assert_eq!(out, "1qa\n2qb\n3q\n");
    let (out, _, _) = run(&["-d", "\\n", a.as_str(), b.as_str()], b"");
    assert_eq!(out, "1\na\n2\nb\n3\n\n");
    // `\0`：占一位而什么都不打
    let (out, _, _) = run(&["-d", "\\0", a.as_str(), b.as_str()], b"");
    assert_eq!(out, "1a\n2b\n3\n");
    // `a\0b` 三位：两列之间用的是第 0 位 'a'
    let (out, _, _) = run(&["-d", "a\\0b", a.as_str(), b.as_str()], b"");
    assert_eq!(out, "1aa\n2ab\n3a\n");
    // 值紧贴本 token（`-d,`）与 `-d` 取下一 token 同效
    let (out, _, _) = run(&["-d,", a.as_str(), b.as_str()], b"");
    assert_eq!(out, "1,a\n2,b\n3,\n");
    // `-ds` 的 `s` 是**值**而不是旗标：两列并行、分隔符是 `s`
    let (out, code, err) = run(&["-ds", a.as_str(), b.as_str()], b"");
    assert_eq!((code, err), (0, String::new()));
    assert_eq!(out, "1sa\n2sb\n3s\n");
}

#[test]
fn parallel_catches_up_for_a_finished_stream() {
    let (a, b, _) = operands("catchup");
    // 第三列又是 a.txt（第二次 open ⇒ 独立游标）；b 只有两行，第三行为它补一个 `+`
    let (out, code, err) = run(&["-d", "+:", a.as_str(), b.as_str(), a.as_str()], b"");
    assert_eq!((code, err), (0, String::new()));
    assert_eq!(out, "1+a:1\n2+b:2\n3+:3\n");
    // 空档名也是一列：每行都给它补一个分隔符
    let (_, _, c) = operands("catchup");
    let (out, _, _) = run(&[c.as_str(), a.as_str()], b"");
    assert_eq!(out, "\t1\n\t2\n\t3\n");
}

/// 操作数档与标准输入里的非 UTF-8 字节**逐字节**走完两条合并腿：C 的行腿
/// （`fgetln` `paste.c:121`、`fgets` `:171`）从不解码。期望逐形取自参照件
/// `tmp/paste-321/paste_ref` 的现量读数（`tmp/paste-comm-434/ab.log`），
/// 旧本壳在 `from_utf8(...).unwrap_or("")` 那一行上把整个档抹成空。
#[test]
fn an_operand_line_that_is_not_utf8_keeps_every_byte() {
    let dir = scratch("nonutf8");
    let raw = dir.join("raw.txt");
    let ok = dir.join("ok.txt");
    fs::write(&raw, b"a\xffb\nc\n").expect("write raw.txt");
    fs::write(&ok, b"x\ny\n").expect("write ok.txt");
    let raw = path(&raw);
    let ok = path(&ok);

    // 同一个档开两次：两列各自读到那一枚 ff
    let (out, status, err) = run_bytes(&[raw.as_str(), raw.as_str()], b"");
    assert_eq!((status, err), (0, Vec::new()));
    assert_eq!(out, b"a\xffb\ta\xffb\nc\tc\n");

    // 坏档与好档并排：坏档的行不许把好档的行拽空
    let (out, _, _) = run_bytes(&[ok.as_str(), raw.as_str()], b"");
    assert_eq!(out, b"x\ta\xffb\ny\tc\n");

    // 顺序模式同样按字节
    let (out, _, _) = run_bytes(&["-s", raw.as_str()], b"");
    assert_eq!(out, b"a\xffb\tc\n");

    // 标准输入这一腿也一样：末行没有换行照样是一行
    let (out, _, _) = run_bytes(&["-", raw.as_str()], b"p\n\xffq\n");
    assert_eq!(out, b"p\ta\xffb\n\xffq\tc\n");
}

/// 集外字母那一行的字母位是 argv 里的**那一枚裸字节** `ff`：`bin_support::args_bytes`
/// 把 argv 原样交回，`src/paste.rs` 取的是那一字节，`line_bytes` 把它一枚一枚写出去。
/// 参照件 `tmp/paste-321/paste_ref` 现量同形（`tmp/argv-441/measure-ref.log`），
/// 改前的本壳交回的是折叠后的第一字节 `ef`（`tmp/argv-441/before-probe.log`）。
#[test]
fn a_raw_ff_option_letter_is_reported_as_that_byte() {
    let flag: Vec<u8> = [b"-".as_slice(), b"\xff"].concat();
    let (out, code, err) = run_raw(&[flag], b"");
    assert_eq!((out, code), (Vec::new(), 1));
    assert_eq!(
        err,
        [
            b"paste: unknown option -- ".as_slice(),
            b"\xff",
            b"\n".as_slice(),
            USAGE.as_bytes()
        ]
        .concat()
    );
}

/// `-d` 的**值**里那枚非 UTF-8 字节是分隔符表的**一位**：改前经 lossy 折叠成
/// `ef bf bd` 三位，于是 `paste -d\xff a.txt b.txt` 打的是 `1\xefa`（`ef` 用在第一行，
/// `bf`、`bd` 轮到后面两行）；参照件现量是每行一枚 `ff`（`tmp/argv-441/measure-ref.log`）。
#[test]
fn a_raw_nonutf8_delimiter_byte_is_one_slot() {
    let (a, b, _) = operands("delimbyte");
    let attached: Vec<u8> = [b"-d".as_slice(), b"\xff"].concat();
    let (out, code, err) = run_raw(
        &[attached, a.as_bytes().to_vec(), b.as_bytes().to_vec()],
        b"",
    );
    assert_eq!((code, err), (0, Vec::new()));
    assert_eq!(out, b"1\xffa\n2\xffb\n3\xff\n");

    // 顺序模式同一枚表：三行之间两枚 `ff`（一位表按位循环）
    let attached: Vec<u8> = [b"-d".as_slice(), b"\xff"].concat();
    let (out, code, err) = run_raw(
        &[
            b"-s".to_vec(),
            attached,
            a.as_bytes().to_vec(),
            b.as_bytes().to_vec(),
        ],
        b"",
    );
    assert_eq!((code, err), (0, Vec::new()));
    assert_eq!(out, b"1\xff2\xff3\na\xffb\n");

    // 值取自下一个 token 时同样按字节：`ff` 与 `+` 是两位。两列时每行的第二列都落在
    // 表的首位（并行的游标每行从表尾那个 NUL 起，`paste.c:112`），所以第二行也是 `ff`；
    // 第三列才用到 `+`——两形皆参照件现量（`tmp/argv-441/measure-three.log`）。
    let value: Vec<u8> = [b"\xff".as_slice(), b"+"].concat();
    let (out, code, err) = run_raw(
        &[
            b"-d".to_vec(),
            value.clone(),
            a.as_bytes().to_vec(),
            b.as_bytes().to_vec(),
        ],
        b"",
    );
    assert_eq!((code, err), (0, Vec::new()));
    assert_eq!(out, b"1\xffa\n2\xffb\n3\xff\n");
    let (out, _, _) = run_raw(
        &[
            b"-d".to_vec(),
            value,
            a.as_bytes().to_vec(),
            b.as_bytes().to_vec(),
            a.as_bytes().to_vec(),
        ],
        b"",
    );
    assert_eq!(out, b"1\xffa+1\n2\xffb+2\n3\xff+3\n");
}

#[test]
fn serial_mode_joins_each_operand_onto_one_line() {
    let (a, b, _) = operands("serial");
    let (out, code, err) = run(&["-s", a.as_str()], b"");
    assert_eq!((code, err), (0, String::new()));
    assert_eq!(out, "1\t2\t3\n");
    let (out, _, _) = run(&["-s", a.as_str(), b.as_str()], b"");
    assert_eq!(out, "1\t2\t3\na\tb\n", "每个操作数一行，顺序输出");
    let (out, _, _) = run(&["-s", "-d", "+:", a.as_str()], b"");
    assert_eq!(out, "1+2:3\n", "顺序模式的游标从表头起，按位循环");
}
