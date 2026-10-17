//! End-to-end face for the `tr` binary, checked against
//! `minix3/usr.bin/tr/tr.c`（选项环 `:68`、`case '?': default: usage();` `:79-81`、
//! 个数判 `:86-97`、`-ds` 要 string2 `:104-106`、`-d` 不许 string2 `:123-125`、
//! 单集合挤压 `:139-140`、翻译路径要两个集合 `:156-157`、`empty string2` `:187-188`、
//! `usage()` `:276-282`）与它驱动的 libc `getopt`（`minix3/lib/libc/stdlib/getopt.c:74-78`、
//! `:80-85`、`:86-93`、`:94-95`、`:98-106`）。
//!
//! 下面每一条期望值都是从参照件 `tmp/tr-322/tr_ref` 现量出来的，逐形对账表在
//! `tmp/tr-322/ab322.log`（SAME=62 / DIFF=3，三形全属已登记的 `TR-SET-GRAMMAR-SENTENCES`）。
//! 量出来的形状有六条：
//! ① optstring `"cds"` 三个字母**都不带值**，libc 的「option requires an argument」在本支不可达；
//!    而 `:79-81` 把 `?` 与 `default` 直接送进 `usage()`，环当场退出 ⇒ 集外字母**只有一行**
//!    libc（`-ZQ` 只报 `Z`、`-Q -Z a` 只报 `Q`），与 `seq`/`jot` 的逐字母累加相反。
//! ② `--s a` 报的字母是 `-`：`getopt.c:79` 取的是短横后的第一个字节，`:80-85` 只把**恰好**
//!    两横认作结束标记，第三字节非空就往下走，`:98` 在 `"cds"` 里找不到 `-` ⇒ `unknown option -- -`。
//! ③ 裸 `-` 不是选项也不是操作数游标的一部分：`:86-93` 让 `-` 留在操作数里，
//!    于是 `tr - a` 是把 `-` 换成 `a`，`tr - -` 原样输出。
//! ④ usage 那四行的程序名写死在串里（`:278-281`），libc 那一行的名字却跟着 `argv[0]`
//!    （`getopt.c:103` 的 `getprogname()`）——同一支命令里两行来历不同，下面
//!    `the_libc_line_follows_argv_zero_but_the_block_is_hardwired` 就是钉这一条。
//! ⑤ `empty string2` 只在翻译路径上出现且**不接** usage（`:187-188`），
//!    `-d ''` 与 `-ds a ''` 都不触发它。
//! ⑥ 本支没有读写操作数档的腿（数据只从标准输入来），所以「薄壳裸 `read` 宿主读不到输入」
//!    那一家族停在 `paste` 的十一处。
//!
//! 集合语法的六句报文在本壳都已接上（`str.c` 的 `genclass()`、`genseq()`、`genequiv()`），
//! 期望值取自参照件现量：`tr_ref '[x*3]' y` 与 `tr_ref '[ab*2]' y` 都是
//! `Sequences only valid in string2`、`tr_ref abc '[ab*2]'` 是 `Misplaced sequence asterisk`、
//! `tr_ref abc '[x*1a]'` 是 `illegal sequence count`、`tr_ref [:bogus:] y` 是
//! `unknown class bogus`、`tr_ref '[=ab=]' y` 是 `Misplaced equivalence equals sign`、
//! `tr_ref '[=a=b=]' y` 是 `Misplaced equivalence right bracket`，全部退 1 且
//! **不跟** usage 块。同一轮里 `bracket()`（`str.c:176-210`）的判据也一并照抄：`[x]`、
//! `[:alpha`、`[x`、`[x]y*` 这些「没成形」的式子在参照件里是**字面量**且退 0。
//! 还差一枚的是 `tr abc '[=c=]'`：两边都退 0、集合都是单枚 `c`，但 string2 用尽之后
//! 参照件把后续位置填成 `\0`（现量 `c\0\0\n`）而我方重复末字节（`ccc\n`）——那是
//! `tr.c` 的 OOBCH 腿而不是 `bracket()` 的判据，已登记为 `TR-EQUIV-OOBCH-SET2`。

mod face_support;

use face_support::write_stdin;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};

const TR: &str = env!("CARGO_BIN_EXE_tr");
/// `tr.c:276-282`：整块四行，后三行七个空格对齐，程序名写死为 `tr`。
const USAGE: &str = concat!(
    "usage: tr [-cs] string1 string2\n",
    "       tr [-c] -d string1\n",
    "       tr [-c] -s string1\n",
    "       tr [-c] -ds string1 string2\n"
);

fn run(args: &[&str], input: &[u8]) -> (String, i32, String) {
    let mut child = Command::new(TR)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the tr binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn usage_block_is_four_lines_on_stderr() {
    // 参照件对旗标自身无操作数的每一形都只退这块：rc=1、标准输出空、stderr 恰是四行
    for args in [
        &[] as &[&str],
        &["-"],
        &["a"],
        &["-c"],
        &["-d"],
        &["-s"],
        &["-ds"],
        &["-cd"],
        &["-cs"],
        &["-dc"],
    ] {
        let (out, code, err) = run(args, b"abc\n");
        assert_eq!(code, 1, "{args:?}");
        assert_eq!(out, "", "{args:?} 诊断不许走标准输出");
        assert_eq!(err, USAGE, "{args:?}");
    }
}

#[test]
fn operand_count_and_combination_all_say_usage_alone() {
    // 个数判 `:86-97` 与两处组合判 `:104-106`、`:123-125` 全在环**之外**，
    // 所以这些形只有 usage，没有 libc 那一行
    for args in [
        &["a", "b", "c"][..],      // 三个操作数
        &["a", "b", "c", "d"][..], // 四个
        &["--", "a"][..],          // `--` 吃掉后只剩一个
        &["a", "-", "b"][..],      // 环在 `a` 就收，`-` 是第二个操作数 ⇒ 三个
        &["a", "b", "-c"][..],     // 不重排：`-c` 也是操作数
        &["-d", "a", "b"][..],     // `-d` 多给一个（`:123-125`）
        &["-dc", "a", "b"][..],    // 同上，聚簇顺序无关
        &["-cd", "a", "b"][..],
        &["-ds", "a"][..],      // `-ds` 少给一个（`:104-106`）
        &["-s", "-d", "a"][..], // 跨 token 的 -s 与 -d 合成 `-ds`，仍少一个
        &["-c", "a"][..],       // 无 -d/-s 时一个操作数不够
    ] {
        let (out, code, err) = run(args, b"aabbcc\n");
        assert_eq!(code, 1, "{args:?}");
        assert_eq!(out, "", "{args:?}");
        assert_eq!(err, USAGE, "{args:?}");
    }
}

#[test]
fn off_letter_gets_one_libc_line_then_the_block() {
    // `:79-81` 在环里就退出 ⇒ 报文不累加：`-ZQ` 只报 Z，`-Q -Z a` 只报 Q
    for (args, letter) in [
        (vec!["-Z"], "Z"),
        (vec!["-h"], "h"),
        (vec!["-0"], "0"),
        (vec!["-a"], "a"),
        (vec!["-ZQ"], "Z"),
        (vec!["-sZ", "a"], "Z"),
        (vec!["-Q", "-Z", "a"], "Q"),
    ] {
        let (out, code, err) = run(&args, b"abc\n");
        assert_eq!((code, out.as_str()), (1, ""), "{args:?}");
        assert_eq!(
            err,
            format!("tr: unknown option -- {letter}\n{USAGE}"),
            "{args:?} 一行 libc 必须在整块 usage 之前"
        );
    }
    // `--s a` 报的字母是短横本身：第三字节 `s` 让它不被认作结束标记
    let (out, code, err) = run(&["--s", "a"], b"aabbcc\n");
    assert_eq!((code, out.as_str()), (1, ""));
    assert_eq!(err, format!("tr: unknown option -- -\n{USAGE}"));
}

#[test]
fn the_libc_line_follows_argv_zero_but_the_block_is_hardwired() {
    // `getopt.c:103` 用 `getprogname()`，`tr.c:278-281` 把 `tr` 写死在串里。
    // 参照件现量：argv[0]="trsweep" 报 `trsweep:`，argv[0]="/some/dir/tr" 报过 basename 的 `tr:`，
    // 两种情况下那四行都一字不变
    for argv0 in ["trsweep", "/some/dir/tr"] {
        let mut child = Command::new(TR)
            .arg0(argv0)
            .arg("-Z")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("the tr binary should be runnable");
        write_stdin(&mut child, b"abc\n");
        let output = child.wait_with_output().expect("the run should finish");
        assert_eq!(output.status.code(), Some(1), "{argv0:?}");
        assert_eq!(output.stdout, b"", "{argv0:?}");
        let err = String::from_utf8_lossy(&output.stderr).into_owned();
        let name = argv0.rsplit('/').next().unwrap_or(argv0);
        assert_eq!(
            err,
            format!("{name}: unknown option -- Z\n{USAGE}"),
            "{argv0:?} 实际 stderr={err:?}"
        );
    }
}

#[test]
fn flag_cluster_is_per_letter_and_order_free() {
    // 同一组旗标不论聚簇还是分词、不论顺序，落点一致
    for args in [
        vec!["-c", "-s", "a", "b"],
        vec!["-sc", "a", "b"],
        vec!["-cs", "a", "b"],
    ] {
        let (out, code, err) = run(&args, b"aabbcc\n");
        assert_eq!((code, err), (0, String::new()), "{args:?}");
        // 挤压集取的是 string2，取补后的 string1 把余下全映射成 `b`，
        // 于是换行也被挤掉：参照件给出 `aab`，末尾没有换行
        assert_eq!(out, "aab", "{args:?}");
    }
    // `-s -d a b` 与 `-ds a b` 同形；`-ds ab b` 删掉 a、b 再按 string2 挤压
    for (args, want) in [
        (vec!["-s", "-d", "a", "b"], "bcc\n"),
        (vec!["-ds", "a", "b"], "bcc\n"),
        (vec!["-ds", "ab", "b"], "cc\n"),
    ] {
        let (out, code, err) = run(&args, b"aabbcc\n");
        assert_eq!((code, err), (0, String::new()), "{args:?}");
        assert_eq!(out, want, "{args:?}");
    }
}

#[test]
fn delete_squeeze_and_complement_shapes_are_the_measured_ones() {
    for (args, input, want) in [
        (vec!["-d", "a"], &b"aabbcc\n"[..], "bbcc\n"),
        (vec!["-d", "ab"], &b"aabbcc\n"[..], "cc\n"),
        (vec!["-cd", "a"], &b"aabbcc\n"[..], "aa"),
        (vec!["-s", "a"], &b"aabbcc\n"[..], "abbcc\n"),
        (vec!["-s", "ab"], &b"aabbcc\n"[..], "abcc\n"),
        (vec!["-s", "a", "b"], &b"aabbcc\n"[..], "bcc\n"),
        (vec!["-cs", "a"], &b"aabbcc\n"[..], "aabc\n"),
        (vec!["-c", "a", "b"], &b"aabbcc\n"[..], "aabbbbb"),
    ] {
        let (out, code, err) = run(&args, input);
        assert_eq!((code, err), (0, String::new()), "{args:?}");
        assert_eq!(out, want, "{args:?}");
    }
}

#[test]
fn empty_string2_sentence_has_no_usage_block() {
    // `:187-188` 的 `errx`：一行、退 1、**不接** usage
    let (out, code, err) = run(&["a", ""], b"abc\n");
    assert_eq!((code, out), (1, String::new()));
    assert_eq!(err, "tr: empty string2\n");
    // 同一条判据不在删除与挤压路径上：空集合都合法
    for (args, want) in [
        (vec!["-d", ""], "aabbcc\n"),
        (vec!["-ds", "a", ""], "bbcc\n"),
    ] {
        let (out, code, err) = run(&args, b"aabbcc\n");
        assert_eq!((code, err), (0, String::new()), "{args:?}");
        assert_eq!(out, want, "{args:?}");
    }
}

#[test]
fn the_ring_does_not_reorder_operands() {
    // 裸 `-` 留在操作数里：`tr - a` 把 `-` 换成 `a`，`tr - -` 原样
    let (out, code, err) = run(&["-", "a"], b"a-b\n");
    assert_eq!((code, err), (0, String::new()));
    assert_eq!(out, "aab\n");
    let (out, code, err) = run(&["-", "-"], b"a-b\n");
    assert_eq!((code, err), (0, String::new()));
    assert_eq!(out, "a-b\n");
    // `--` 吃掉自身：`tr -- a b` 与 `tr a b` 同形
    let (out, code, err) = run(&["--", "a", "b"], b"aabbcc\n");
    assert_eq!((code, err), (0, String::new()));
    assert_eq!(out, "bbbbcc\n");
}

#[test]
fn translation_path_is_unchanged_by_the_shell_rewrite() {
    for (args, input, want) in [
        (vec!["a", "b"], &b"aabbcc\n"[..], "bbbbcc\n"),
        (vec!["ab", "xy"], &b"aabbcc\n"[..], "xxyycc\n"),
        (vec!["a-z", "A-Z"], &b"abc\n"[..], "ABC\n"),
        (vec!["[:lower:]", "[:upper:]"], &b"abc\n"[..], "ABC\n"),
        (vec!["a", "[b*3]"], &b"abc\n"[..], "bbc\n"),
        (vec!["a-z", "z"], &b"abc\n"[..], "zzz\n"),
        (vec!["-d", "[:alpha:]"], &b"aabbcc12\n"[..], "12\n"),
        (vec!["a", "b"], &b"a-b\n"[..], "b-b\n"),
    ] {
        let (out, code, err) = run(&args, input);
        assert_eq!((code, err), (0, String::new()), "{args:?}");
        assert_eq!(out, want, "{args:?}");
    }
    // 空输入是成功
    let (out, code, err) = run(&["a", "b"], b"");
    assert_eq!((out, code, err), (String::new(), 0, String::new()));
}

#[test]
fn an_empty_string1_is_the_identity_map() {
    // `tr.c:178-181` 的恒等预填 + `:214-215` 的空集合立刻 break ⇒ 谁也没被映射过；
    // 参照件对这四形都是退 0 且原样照抄（开工前的读数在 `tmp/tr-323/measure_before.log`）
    for args in [
        vec!["", "x"],
        vec!["", "b"],
        vec!["", "A-Z"],
        vec!["-s", "", "x"],
    ] {
        let (out, code, err) = run(&args, b"aabbcc\n");
        assert_eq!((code, err), (0, String::new()), "{args:?}");
        assert_eq!(out, "aabbcc\n", "{args:?}");
    }
    // 取补路径不受这条影响：空的 string1 取补是**全体字节**，都映射到 string2 的末字节，
    // 于是七个输入字节全变 `x`，连结尾换行也被换掉
    let (out, code, err) = run(&["-c", "", "x"], b"aabbcc\n");
    assert_eq!((code, err), (0, String::new()));
    assert_eq!(out, "xxxxxxx");
    // 而空 string2 仍是那一句 `errx`（`:187-188`），恒等支不会把它吞掉
    let (out, code, err) = run(&["", ""], b"aabbcc\n");
    assert_eq!((code, out), (1, String::new()));
    assert_eq!(err, "tr: empty string2\n");
    // 三个操作数里带一个空串，判的还是个数（`:86-97`），恒等支管不着
    let (_, code, err) = run(&["", "x", "extra"], b"aabbcc\n");
    assert_eq!(code, 1);
    assert_eq!(err, USAGE);
}

#[test]
fn the_four_set_grammar_sentences_reach_stderr_verbatim() {
    // 参照件现量：一句、退 1、**不跟** usage 块（`str.c` 每处都是当场 errx(1, ...)）
    for (set1, set2, want) in [
        ("[x*3]", "y", "tr: Sequences only valid in string2\n"),
        ("[ab*2]", "y", "tr: Sequences only valid in string2\n"),
        ("abc", "[ab*2]", "tr: Misplaced sequence asterisk\n"),
        ("abc", "[x*1a]", "tr: illegal sequence count\n"),
        ("[:bogus:]", "y", "tr: unknown class bogus\n"),
    ] {
        let (out, code, err) = run(&[set1, set2], b"abc\n");
        assert_eq!(
            (code, out.as_str(), err.as_str()),
            (1, "", want),
            "[{set1}] [{set2}]"
        );
    }
}

#[test]
fn the_two_equivalence_sentences_reach_stderr_verbatim() {
    // 参照件现量（`tmp/tr-341/measure341.log`）：这两句各一句、退 1、**不跟** usage，
    // 且两串位同判——`bracket()` 认形式、`genequiv()` 核内部，两处都不分串位。
    // 本支之前本壳把 `[=ab=]` 悄悄收成单枚成员 `a` 并退 0：假报「能干」比不报更贵。
    for (set, want) in [
        ("[=ab=]", "tr: Misplaced equivalence equals sign\n"),
        ("[=abc=]", "tr: Misplaced equivalence equals sign\n"),
        ("[==]", "tr: Misplaced equivalence equals sign\n"),
        (r"[=\=]", "tr: Misplaced equivalence equals sign\n"),
        ("[=a=b=]", "tr: Misplaced equivalence right bracket\n"),
    ] {
        for (a, b) in [(set, "y"), ("y", set)] {
            let (out, code, err) = run(&[a, b], b"abc\n");
            assert_eq!(
                (code, out.as_str(), err.as_str()),
                (1, "", want),
                "[{a}] [{b}]"
            );
        }
    }
    // 成形的那一枚不退化：`[=c=]` 仍是**单枚**成员 `c`、退 0（参照件现量 `abx`，
    // 因为被换掉的是输入里的 `c` 而不是 `a`）
    let (out, code, err) = run(&["[=c=]", "x"], b"abc\n");
    assert_eq!((code, out.as_str(), err.as_str()), (0, "abx\n", ""));
}

#[test]
fn an_unshaped_bracket_is_a_literal_not_a_complaint() {
    // `bracket()` 返回假就 FALLTHROUGH 成字面 `[`（`str.c:124-132`），参照件这几形全退 0
    let (out, code, err) = run(&["[x]", "y"], b"abc\n");
    assert_eq!((code, out.as_str(), err.as_str()), (0, "abc\n", ""));
    let (out, code, err) = run(&["abc", "[x]"], b"abc\n");
    assert_eq!((code, out.as_str(), err.as_str()), (0, "[x]\n", ""));
    // `[:alpha` 没成形，于是 `[`、`:`、`a`、`l`、`p`、`h`、`a` 全是字面量：集合里含 `a`，
    // `a` 被换成 `y`——参照件同形现量 `ybc\n`（见 tmp/tr-335/ab335.log）
    for (set, want) in [("[x", "abc\n"), ("[x]y*", "abc\n"), ("[:alpha", "ybc\n")] {
        let (out, code, err) = run(&[set, "y"], b"abc\n");
        assert_eq!((code, out.as_str(), err.as_str()), (0, want, ""), "{set:?}");
    }
}

#[test]
fn an_open_or_zero_repeat_replaces_every_member() {
    // `[x*]` 与 `[x*0]` 都是 INFINITE，参照件与空集合无关（空 string2 另有那句）
    for set in ["[x*]", "[x*0]"] {
        let (out, code, err) = run(&["abc", set], b"abc\n");
        assert_eq!(
            (code, out.as_str(), err.as_str()),
            (0, "xxx\n", ""),
            "{set:?}"
        );
    }
}

#[test]
fn a_reversed_range_is_three_literals_and_exits_zero() {
    // 参照件 `tr_ref z-a y` 退 0：区间退回后 z、-、a 各自入集，`a` 被换成 `y`
    let (out, code, err) = run(&["z-a", "y"], b"abc\n");
    assert_eq!((code, out.as_str(), err.as_str()), (0, "ybc\n", ""));
}
