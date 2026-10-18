//! End-to-end face for the `units` binary, checked against
//! `minix3/usr.bin/units/units.c`（选项环 `:817-846`、操作数判 `:849-851`、
//! `readunits()` 的开档支 `:142-181`、`usage()` `:793-807`、交互提示环
//! `:876-893`）与它驱动的 libc `getopt`（`minix3/lib/libc/stdlib/getopt.c:74-78`、
//! `:81-85`、`:87-92`、`:95-104`、`:112-141`）。
//!
//! `units.c:817` 的 optstring 是 `"lLvqf:"`：`l`/`L`/`q`/`v` 不带值，只有 `f` 带值。
//! 参照件 `tmp/units-319/units_ref`（`units.c` 真源 + Minix 的 `getopt.c` + 程序名与
//! `strlcpy`/`strlcat` 垫片，宿主现链）逐形量出的形状是：
//! ① 集外字母 = libc 一行 + **九行** usage，退 3；聚簇 `-qZ` 报的是 `Z`；
//! ② 缺值只有 `-f` 一形，句子与退码同 ①；
//! ③ `-v` 的两行版本在**标准错误**上，且它在环里就退出——`-vZ` 没有集外字母那一行，
//!    `-Zv` 没有版本两行；
//! ④ 操作数个数（非 0/2/3）与「`-l` 还带操作数」都**只有 usage 块**，没有 libc 那一行；
//! ⑤ 这个判据排在 `readunits` **之前**，所以 `-f <坏档> m` 报 usage 而不是开档失败；
//! ⑥ 两支开档失败分属两个句子：`-f` 是 `err` 的三段形（带 `strerror`），默认档
//!    `/usr/share/misc/units.lib` 是 `errx` 的两段形（不带），都退 1；
//! ⑦ `--` 被吃掉、裸 `-` 是操作数，而且 `-f` 取下一个 token 时不看它的脸色
//!    （`-f - m sec` 的档名就是 `-`）；
//! ⑧ 环不重排：`units -f <档> m sec -l` 的 `-l` 是第三个操作数，于是它是 want 侧的
//!    单位名，报文 `units: Unknown unit '-l'`；
//! ⑨ 交互面 `do { 提示; fgets; } while (不认)`：一行不认就**重问同一格**（参照件对
//!    `bogus\nm\nsec\n` 打的是 `You have: You have: You want: `），`fgets(buf, 80, …)`
//!    把 100 字符的长行切成 79+21 两段，各报一条 `Unknown unit`。
//!
//! 这扇门前不存在：`units` 的交互面拿裸 `minix_sys::read` 读标准输入，宿主上一个字也
//! 读不到（同族第九处之后的第十处），而 `usage()` 只有两行、`-v` 打到标准输出上、
//! 默认档名写成 `/usr/lib/units`——四处都由本门第一次钉住。
//!
//! 决定半（换算数值、`-l`/`-L` 的展开形、坏单位时的**半成品答案块**）不在此门射程内，
//! 已在 `rewrite-notes/18-stage-commands/todo.md` 批次三百一十九那条登记。

mod face_support;

use face_support::write_stdin;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};

const UNITS: &str = env!("CARGO_BIN_EXE_units");
/// `units.c:795-806` 的两个 `fprintf` 合起来的九行；`units` 是写死的字面量。
const USAGE: &str = concat!(
    "\nunits [-Llqv] [-f filename] [[count] from-unit to-unit]\n",
    "\n    -f specify units file\n",
    "    -L list units in standardized base units\n",
    "    -l list units\n",
    "    -q suppress prompting (quiet)\n",
    "    -v print version number\n",
);
/// `units.c:836-839` 的两行版本，同在标准错误上。
const VERSION: &str = concat!(
    "\n  units version 1.0  Copyright (c) 1993 by Adrian Mariano\n",
    "                    This program may be freely distributed\n",
);
/// 参照件与门共用的那份小词典（`tmp/units-319/tiny.lib` 的同一内容）。
const TINY: &str = concat!(
    "/ tiny test database for batch 319 A/B\n",
    "/ primitive units\n",
    "m\t!a!\n",
    "sec\t!c!\n",
    "/ derived\n",
    "km\t1000*m\n",
    "cm\t.01*m\n",
    "/ composite\n",
    "min\t60*sec\n",
);

fn run(args: &[&str], input: &[u8]) -> (String, i32, String) {
    let mut child = Command::new(UNITS)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the units binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// 把小词典落到临时文件，返回它的路径（参照件跑的是同一份内容）。
fn database(tag: &str) -> (PathBuf, String) {
    let dir = scratch(tag);
    let path = dir.join("tiny.lib");
    fs::write(&path, TINY).expect("write tiny.lib");
    let name = path.to_str().expect("utf-8 scratch path").to_string();
    (dir, name)
}

fn scratch(tag: &str) -> PathBuf {
    static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("units-face-{}-{tag}-{n}", std::process::id()));
    fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

#[test]
fn off_letter_gets_libc_line_then_the_nine_line_usage_block() {
    let (out, code, err) = run(&["-Z"], b"");
    assert_eq!(code, 3, "参照件 `-Z` 退 3，实际 stdout={out:?}");
    assert_eq!(err, format!("units: unknown option -- Z\n{USAGE}"));
    assert_eq!(out, "");
}

#[test]
fn cluster_reports_the_off_letter_after_the_valid_ones() {
    // `-qZ` 与 `-Zq` 的报文都是 `Z`：前者 `q` 先被置上再撞上集外字母，后者一上来就是
    // `Z`。两支都在 `usage()` 里退出，所以差别在外部行为上看不见——但字母不能取第一个。
    for args in [&["-qZ"][..], &["-Zq"][..]] {
        let (out, code, err) = run(args, b"");
        assert_eq!(code, 3, "{args:?} 该退 3，实际 stdout={out:?}");
        assert_eq!(
            err,
            format!("units: unknown option -- Z\n{USAGE}"),
            "{args:?}"
        );
        assert_eq!(out, "");
    }
}

#[test]
fn missing_value_is_reachable_only_through_f() {
    let (out, code, err) = run(&["-f"], b"");
    assert_eq!(code, 3);
    assert_eq!(
        err,
        format!("units: option requires an argument -- f\n{USAGE}")
    );
    assert_eq!(out, "");
    // `l`/`q`/`v` 都不带值，所以「后面紧跟的 token 被当值吃掉」这一形在本支不存在：
    // `-l m sec` 走的是「`-l` 还带操作数」那一支，`-q` 干脆一路走到读默认档。
    let (_, code, err) = run(&["-q"], b"");
    assert_eq!(code, 1, "-q 之后没选项了，该去读词典");
    assert_eq!(
        err,
        "units: Can't find units file '/usr/share/misc/units.lib'\n"
    );
}

#[test]
fn attached_value_is_never_reported_missing() {
    // `-fZ` 的值紧贴着，libc 不会报缺值，于是这一形落到开档失败那三支里的一段。
    let (out, code, err) = run(&["-fZ"], b"");
    assert_eq!(code, 1);
    assert_eq!(
        err,
        "units: Unable to open units file 'Z': No such file or directory\n"
    );
    assert_eq!(out, "");
}

#[test]
fn version_lines_go_to_stderr_and_win_over_later_letters() {
    // 旧写法把这两行打到了标准输出上（`units.c:836` 是 `fprintf(stderr, …)`）。
    let (out, code, err) = run(&["-v"], b"");
    assert_eq!(code, 3);
    assert_eq!(out, "", "版本两行在标准错误上，标准输出该是空的");
    assert_eq!(err, format!("{VERSION}{USAGE}"));
    // `-v` 在环里就退出：同一 token 后面的 `Z` 永远扫不到。
    let (_, code, err) = run(&["-vZ"], b"");
    assert_eq!(code, 3);
    assert_eq!(
        err,
        format!("{VERSION}{USAGE}"),
        "-v 之后不该再有集外字母那一行"
    );
    // 反过来 `-Zv` 只有集外字母那一行加 usage，版本两行打不出来。
    let (_, code, err) = run(&["-Zv"], b"");
    assert_eq!(code, 3);
    assert_eq!(err, format!("units: unknown option -- Z\n{USAGE}"));
    // `-f <档> -v` 也一样：`-v` 排在操作数判据之前，所以坏值都读过了还是先看版本。
    let (_dir, db) = database("version-after-f");
    let (_, code, err) = run(&["-f", &db, "-v"], b"");
    assert_eq!(code, 3);
    assert_eq!(err, format!("{VERSION}{USAGE}"));
}

#[test]
fn the_operand_count_check_runs_before_the_database_read() {
    let (_dir, db) = database("count-before-read");
    // 一个操作数：C 在 `units.c:849-851` 就 usage，档名连开都不开。
    let (out, code, err) = run(&["-f", &db, "m"], b"");
    assert_eq!(code, 3);
    assert_eq!(err, USAGE, "旧写法把这形拖到读库之后，坏档时会先印开档失败");
    assert_eq!(out, "");
    // 四个操作数同一支。
    let (_, code, err) = run(&["-f", &db, "m", "sec", "km", "cm"], b"");
    assert_eq!(code, 3);
    assert_eq!(err, USAGE);
    // 坏档 + 合法的操作数个数：这才轮到开档失败那一句。
    let (_, code, err) = run(&["-f", "/definitely-not-here", "m", "sec"], b"");
    assert_eq!(code, 1);
    assert_eq!(
        err,
        "units: Unable to open units file '/definitely-not-here': No such file or directory\n"
    );
}

#[test]
fn default_database_failure_is_the_errx_two_part_shape() {
    // 无 `-f` 时 C 找的是 `_PATH_UNITSLIB`（`minix3/usr.bin/units/pathnames.h`），
    // 找不到走 `errx`：句子带档名但**不带** `strerror`，与 `-f` 那支的三段形相对。
    for args in [&["--"][..], &["-q"][..], &["m", "-l", "sec"][..]] {
        let (out, code, err) = run(args, b"");
        assert_eq!(code, 1, "{args:?}");
        assert_eq!(
            err, "units: Can't find units file '/usr/share/misc/units.lib'\n",
            "{args:?}"
        );
        assert_eq!(out, "", "{args:?} 那句在标准错误上");
    }
}

#[test]
fn double_dash_is_consumed_and_bare_dash_is_an_operand() {
    // `--` 在 `getopt.c:81-85` 结束环且自身被推进掉，于是 `-- -Z` 只剩一个操作数：
    // 走「个数不对」那一支，只有 usage 块，没有 libc 的集外字母那一行。
    let (_, code, err) = run(&["--", "-Z"], b"");
    assert_eq!(code, 3);
    assert_eq!(err, USAGE);
    // 裸 `-` 结束环却不推进（`:87-92`），所以它是操作数；`-f` 取下一个 token 时
    // 不看脸色，`-f - m sec` 的档名就是 `-`。
    let (_, code, err) = run(&["-f", "-", "m", "sec"], b"");
    assert_eq!(code, 1);
    assert_eq!(
        err,
        "units: Unable to open units file '-': No such file or directory\n"
    );
}

#[test]
fn list_mode_rejects_operands_and_lists_the_tiny_database() {
    let (_dir, db) = database("list");
    let (_, code, err) = run(&["-l", "-f", &db, "m", "sec"], b"");
    assert_eq!(code, 3);
    assert_eq!(
        err, USAGE,
        "`list && argc != 0` 与个数不对同走 usage 一支，没有 libc 行"
    );
    // 无操作数时才进列表；这一段是决定半与外部形状的重叠处，参照件现量的正是这份小词典。
    let (out, code, err) = run(&["-l", "-f", &db], b"");
    assert_eq!(code, 0, "列表里有坏定义才退 1，这份词典没有");
    assert_eq!(err, "");
    assert_eq!(
        out,
        "/ Primitive units\nm\t!a!\nsec\t!c!\n/ Prefixes\n/ Other units\n\
         km\t1000*m\ncm\t.01*m\nmin\t60*sec\n"
    );
}

#[test]
fn ring_does_not_rearrange_the_tail() {
    // 环在第一个非选项 token 就结束（`getopt.c:74-78`），所以写在操作数之后的选项
    // **不是选项**。这一形有四个操作数（`m`、`sec`、`-l`、档名），落「个数不对」那一支：
    // 只有 usage 块、退 3。若谁去重排参数，`-l` 会被当选项吃掉而只剩两个操作数，退码变 0。
    let (_dir, db) = database("rearrange");
    let (out, code, err) = run(&["m", "sec", "-l", &db], b"");
    assert_eq!(
        code, 3,
        "旧写法会把 `-l` 与 `-f` 捞回环内，于是这形变成合法的两个操作数"
    );
    assert_eq!(err, USAGE);
    assert_eq!(out, "");
    // 三个操作数时那一路把尾巴当单位名：`units -f <档> m sec -l` 的 `-l` 是 want 侧的
    // 名字，报文是 C 的 `Unknown unit`，而**不是**进列表（旧写法在此退 1 且不报任何句子）。
    let (out, code, err) = run(&["-f", &db, "m", "sec", "-l"], b"");
    assert_eq!(code, 0, "C 忽略换算失败照样退 0");
    assert!(
        !out.contains("/ Primitive units"),
        "这一形不该进列表模式，stdout={out:?}"
    );
    assert!(
        err.starts_with("units: Unknown unit '") && err.ends_with("'\n"),
        "参照件报的是 `units: Unknown unit '-l'`；本壳的名字段少了前导短横，\
         那一处失真登记为 UNITS-LEADING-DASH-NAME"
    );
}

#[test]
fn unknown_operand_name_gets_the_c_warning_and_exit_zero() {
    // 参照件：`units -f <档> bogus m` 打 `units: Unknown unit 'bogus'`（标准错误、
    // 两段形）再照样出一个答案块，退 0。答案块那一半是决定半里的「半成品展示」，
    // 登记为 UNITS-PARTIAL-UNIT-ON-ERROR，本门只钉句子与退码。
    let (_dir, db) = database("unknown-name");
    let (_, code, err) = run(&["-f", &db, "bogus", "m"], b"");
    assert_eq!(code, 0);
    assert_eq!(err, "units: Unknown unit 'bogus'\n");
    // 两边都陌生时句子按 have→want 的顺序各打一行（参照件 `tmp/units-319/measure4.log`
    // 那一形：`Unknown unit 'bogus'` 然后 `Unknown unit 'nope'`）。
    let (_, code, err) = run(&["-f", &db, "bogus", "nope"], b"");
    assert_eq!(code, 0);
    assert_eq!(
        err, "units: Unknown unit 'bogus'\nunits: Unknown unit 'nope'\n",
        "两句的顺序照 C 的先 have 后 want"
    );
}

#[test]
fn interactive_pair_reads_stdin_and_prompts_in_order() {
    // 这一枚是同族第十处读缝缺陷的门：旧写法拿裸 `minix_sys::read` 读标准输入，
    // 宿主上一个字节也读不到，于是交互面只剩「打提示 → 立刻 EOF」。
    let (_dir, db) = database("interactive");
    let (out, code, err) = run(&["-f", &db], b"km\ncm\n");
    assert_eq!(code, 0);
    assert_eq!(err, "");
    assert_eq!(
        out,
        "5 units, 0 prefixes\n\nYou have: You want: \t* 100000\n\t/ 1e-05\nYou have: \n"
    );
}

#[test]
fn unknown_have_line_reprompts_the_same_slot() {
    // C 的 `do { 提示; fgets; } while (addunit || completereduce)`：坏行重问**同一格**。
    // 旧写法把两次读当成 have/want 一对，于是 `bogus` 吃掉的是 want 那一格。
    let (_dir, db) = database("reprompt");
    let (out, code, err) = run(&["-f", &db], b"bogus\nm\nsec\n");
    assert_eq!(code, 0);
    assert_eq!(err, "units: Unknown unit 'bogus'\n");
    assert!(
        out.starts_with("5 units, 0 prefixes\n\nYou have: You have: You want: "),
        "坏的一行该重问 have，实际 stdout={out:?}"
    );
    assert!(
        out.ends_with("You have: \n"),
        "成对之后还要再问一次 have，实际 {out:?}"
    );
}

#[test]
fn eof_mid_pair_prints_a_newline_and_exits_zero() {
    let (_dir, db) = database("eof-mid");
    // have 读到了、want 那格撞上 EOF：C 在**非 `-q`** 时补一个换行再退 0，答案块不打。
    let (out, code, err) = run(&["-f", &db], b"m\n");
    assert_eq!(code, 0);
    assert_eq!(err, "");
    assert_eq!(out, "5 units, 0 prefixes\n\nYou have: You want: \n");
    // 一上来就 EOF：只有 have 那一次提示加换行。
    let (out, code, _) = run(&["-f", &db], b"");
    assert_eq!(code, 0);
    assert_eq!(out, "5 units, 0 prefixes\n\nYou have: \n");
}

#[test]
fn quiet_suppresses_prompts_and_the_counts_line() {
    let (_dir, db) = database("quiet");
    // `-q` 之下提示与 counts 都不打，EOF 那个换行也不打——三通道全空、退 0。
    let (out, code, err) = run(&["-q", "-f", &db], b"");
    assert_eq!(code, 0);
    assert_eq!(out, "", "-q 下提示、counts 与 EOF 换行都不打");
    assert_eq!(err, "");
    let (out, code, err) = run(&["-q", "-f", &db], b"km\ncm\n");
    assert_eq!(code, 0);
    assert_eq!(err, "");
    assert_eq!(out, "\t* 100000\n\t/ 1e-05\n");
}

#[test]
fn long_input_line_truncates_at_seventy_nine_bytes() {
    // C 用 `fgets(havestr, 80, stdin)`：一行最多 79 字节，剩下的连同换行归下一次读。
    // 参照件对 100 个 `x` 报的是 79 与 21 两段，然后才把 `km`/`cm` 当第二、三次读。
    let (_dir, db) = database("long-line");
    let input = format!("{}\nkm\ncm\n", "x".repeat(100));
    let (out, code, err) = run(&["-f", &db], input.as_bytes());
    assert_eq!(code, 0);
    assert_eq!(
        err,
        format!(
            "units: Unknown unit '{}'\nunits: Unknown unit '{}'\n",
            "x".repeat(79),
            "x".repeat(21)
        )
    );
    assert!(
        out.starts_with("5 units, 0 prefixes\n\nYou have: You have: You have: You want: "),
        "长行切两段各重问一次 have，实际 stdout={out:?}"
    );
}

#[test]
fn blank_line_is_a_valid_dimensionless_pair() {
    // 空行不是「跳过」：C 把它交给 `addunit`，得到无量纲的 1，于是答案是 `* 1` `/ 1`。
    let (_dir, db) = database("blank");
    let (out, code, err) = run(&["-f", &db], b"\n\n");
    assert_eq!(code, 0);
    assert_eq!(err, "");
    assert_eq!(
        out,
        "5 units, 0 prefixes\n\nYou have: You want: \t* 1\n\t/ 1\nYou have: \n"
    );
}

#[test]
fn colon_in_the_optstring_is_a_marker_not_a_selectable_letter() {
    // 参照件现量（`tmp/units-319/measure5.log`）：`units -:` 与 `units -l:` 都打
    // `units: unknown option -- :` 再接 usage，退 3；`units -f:` 的档名就是那个冒号。
    for args in [&["-:"][..], &["-l:"][..]] {
        let (_, code, err) = run(args, b"");
        assert_eq!(code, 3, "{args:?}");
        assert_eq!(
            err,
            format!("units: unknown option -- :\n{USAGE}"),
            "{args:?}"
        );
    }
    let (_, code, err) = run(&["-f:"], b"");
    assert_eq!(code, 1);
    assert_eq!(
        err,
        "units: Unable to open units file ':': No such file or directory\n"
    );
}

#[test]
fn listexpand_flag_expands_definitions_in_the_listing() {
    // `-L` 是第二个字母而不是 `-l` 的大小写同义：它除进列表外还置 `listexpand`，
    // 于是定义行被折算成十进制因子（参照件 `km\t1000 m`），`-l` 则原样透传 `km\t1000*m`。
    let (_dir, db) = database("listexpand");
    let (out, code, err) = run(&["-L", "-f", &db], b"");
    assert_eq!(code, 0);
    assert_eq!(err, "");
    assert_eq!(
        out,
        "/ Primitive units\nm\t!a!\nsec\t!c!\n/ Prefixes\n/ Other units\n\
km\t1000 m\ncm\t0.01 m\nmin\t60 sec\n"
    );
}

#[test]
fn off_letter_outside_the_charset_is_reported_by_its_first_byte() {
    // 参照件对 `-é` 报的是**首字节**（libc 的 `%c` 打那一个字节）：
    // `tmp/units-319/units -é` 与 `-qé` 都打 `units: unknown option -- ` + `c3` + 换行
    // + 那份九行 usage，退 3。本壳的游标本来就按字节走（所以不会有 `column -té`
    // 那种越出字符边界的 panic），曾经差的只剩渲染：按 `char` 打的是 Latin-1 码点
    // U+00C3，即 `c3 83` 两个字节——那一处失真登记为 `UNITS-NONASCII-OPTLETTER`，
    // 随批次三百九十九换 `GetoptReport::line_bytes` 闭合，断言因此改成逐字节。
    // 文本夹具在这里已经不够用：`String::from_utf8_lossy` 会把裸 `c3` 与 `c3 83`
    // 都渲染成替换字符，所以比的是 stderr 的原始字节。
    let want: Vec<u8> = [
        b"units: unknown option -- ".as_slice(),
        b"\xc3\n",
        USAGE.as_bytes(),
    ]
    .concat();
    for form in ["-\u{e9}", "-q\u{e9}"] {
        let out = Command::new(UNITS)
            .args([form])
            .stdin(Stdio::null())
            .stderr(Stdio::piped())
            .output()
            .expect("units 该能起");
        assert_eq!(out.stdout, Vec::new(), "{form} 的报文只走 stderr");
        assert_eq!(out.status.code(), Some(3), "{form} 走 usage 那支退 3");
        assert_eq!(out.stderr, want, "{form} 的字母该是一个裸字节 c3");
    }

    // ASCII 那一形两个入口一字不差
    let (_, code, err) = run(&["-Z"], b"");
    assert_eq!(code, 3);
    assert_eq!(err, format!("units: unknown option -- Z\n{USAGE}"));
}
