//! End-to-end face for the `jot` binary, checked against
//! `minix3/usr.bin/jot/jot.c`（选项环 `:149-177`、操作数计数的 `switch (argc)`
//! `:184-226`、`usage()` `:313-320`）与它驱动的 libc `getopt`
//!（`minix3/lib/libc/stdlib/getopt.c:74-78`、`:81-92`、`:95-104`、`:118-141`）。
//!
//! `jot.c:150` 的 optstring 是 `"b:cnp:rs:w:"`：三个不带值的字母（`c`、`n`、`r`）
//! 加四个带值的（`b`、`p`、`s`、`w`），所以可达的出口有**四类**，句式各有主人：
//! ① 集外字母 = libc 一行 + usage 一行，退 1（聚簇只报第一个坏字母，`--x` 报的是 `-`）；
//! ② 带值字母缺值 = libc 的 `option requires an argument -- <字母>` + usage 两行，
//!    四个字母同形（旧写法里 `-p` 缺值错走了 `Bad precision value` 那一支，句子和行数都不同）；
//! ③ `-p` 的值过不了 `strtol(optarg, &ep, 0)` 全串检查或为负 = `jot: Bad precision value`
//!    **单独一行**、不接 usage；
//! ④ 环正常结束后才轮到操作数：一个都没有（含光写 `--`）是 usage 且**没有** libc 行，
//!    五个以上是 `jot: Too many arguments.  What do you mean by <第五个>?` 单独一行。
//!
//! 另两处只有端到端才看得见：`-b` 与 `-w` 共用一个词槽（`:172-176` 的 FALLTHROUGH），
//! 以及环**不重排**——`jot 3 -Z` 里 `-Z` 是 begin 那个操作数，`sscanf` 不吃它时就取
//! 它最后一个字符的代码点当值（`:203-204`），所以打的是 90/91/92。
//!
//! 本支没有读写缝缺陷：`jot` 既不读标准输入也不开任何档，值全由命令行推出来。
//! 也不测 `prec` 保持 `-1` 的那一族（`jot 3`、`-n 3`、`3 - 5`、`-s , 3`）——`jot.c:349-356`
//! 在那一族拼出的说明符本身是坏的，被打印成什么由各家 `printf` 决定，本机不可判定，
//! 已登记为 `JOT-PREC-MINUS-ONE-FORMAT`。`-r` 的随机流另有一档（`07` §4.4）。

use std::process::{Command, Stdio};

const JOT: &str = env!("CARGO_BIN_EXE_jot");
const USAGE: &str = "usage: jot [-cnr] [-b word] [-p precision] [-s string] [-w word] [reps [begin [end [step | seed]]]]\n";

fn run(args: &[&str]) -> (String, i32, String) {
    let output = Command::new(JOT)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("the jot binary should be runnable");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// 集外字母：libc 那一行在前、`usage()` 在后，退 1（`jot.c:151-152`）。旧写法少了 libc 那一行。
#[test]
fn an_unknown_letter_prints_the_libc_line_then_the_usage() {
    let (out, status, err) = run(&["-Z", "3"]);
    assert_eq!((out.as_str(), status), ("", 1));
    assert_eq!(err, format!("jot: unknown option -- Z\n{USAGE}"));
}

/// 聚簇只报第一个坏字母（参照件：`-cz` → `unknown option -- z`，`c` 已生效但不再往后扫）。
#[test]
fn a_cluster_reports_only_its_first_bad_letter() {
    for (args, letter) in [("-cz", "z"), ("-Zb", "Z"), ("-ncrZ", "Z")] {
        let (out, status, err) = run(&[args, "3"]);
        assert_eq!((out.as_str(), status), ("", 1), "stderr 是 {err:?}");
        assert_eq!(err, format!("jot: unknown option -- {letter}\n{USAGE}"));
    }
}

/// `--x` 报的字母是第二个连字符：optstring 里没有 `-`（参照件：`unknown option -- -`）。
#[test]
fn a_dash_followed_by_a_word_is_an_unknown_option_named_by_the_dash() {
    let (out, status, err) = run(&["--x", "3"]);
    assert_eq!((out.as_str(), status), ("", 1));
    assert_eq!(err, format!("jot: unknown option -- -\n{USAGE}"));
}

/// 四个带值字母缺值同形：libc 那一行 + usage，退 1。旧写法里 `-b`/`-s`/`-w` 只打 usage、
/// `-p` 打成 `Bad precision value`，两支的行数与句子都错位。
#[test]
fn a_value_letter_without_its_value_prints_the_libc_line_then_the_usage() {
    for letter in ['b', 'p', 's', 'w'] {
        let flag = format!("-{letter}");
        let (out, status, err) = run(&[&flag]);
        assert_eq!(
            (out.as_str(), status),
            ("", 1),
            "{flag} 的 stderr 是 {err:?}"
        );
        assert_eq!(
            err,
            format!("jot: option requires an argument -- {letter}\n{USAGE}"),
            "缺值这一支是两句、程序名跟着 `getprogname()`"
        );
        // 聚簇落在串尾同样走这一支——但**后面不能再有 token**：`-cb 3` 里的 `3` 会被
        // `-b` 取作词值，于是缺值判据根本不成立（参照件现量：那样打的是 usage）。
        let clustered = format!("-c{letter}");
        let (_, status, err) = run(&[&clustered]);
        assert_eq!(status, 1);
        assert_eq!(
            err,
            format!("jot: option requires an argument -- {letter}\n{USAGE}")
        );
    }
}

/// `-p` 的值坏是 `errx` 的单独一行（`jot.c:161-165`）：程序名写死在句子里、**不接 usage**、
/// 退 1。判定用的是 `strtol` 的全串检查，所以 `09`（八进吃到 `0` 就停）与 `3x` 都拒。
#[test]
fn a_rejected_precision_is_one_line_without_the_usage() {
    for word in ["abc", "-1", "3x", "09", " ", "-", "99999999999999999999"] {
        let (out, status, err) = run(&["-p", word, "3"]);
        assert_eq!(
            (out.as_str(), status),
            ("", 1),
            "`-p {word}` 的 stderr 是 {err:?}"
        );
        assert_eq!(err, "jot: Bad precision value\n");
    }
}

/// 一个操作数都没有就是 usage，而且**没有** libc 那一行。`-b X` 的 `X` 被 `-b` 吃掉，
/// 于是同样是一个操作数都没有（参照件现量：`jot -b X` 打 usage、退 1）。
#[test]
fn an_empty_operand_list_is_the_usage_without_a_libc_line() {
    for args in [
        vec![],
        vec!["--"],
        vec!["-n"],
        vec!["-c", "-r"],
        vec!["-b", "X"],
    ] {
        let (out, status, err) = run(&args);
        assert_eq!(
            (out.as_str(), status),
            ("", 1),
            "`{args:?}` 的 stderr 是 {err:?}"
        );
        assert_eq!(err, USAGE, "只有 usage 一行");
    }
}

/// 五个操作数起是一句 `errx`，报的是**第五个**（`argv[4]`），前面四个一个都不解读；
/// `arguments.` 后面是两个空格（`jot.c:223-225`）。
#[test]
fn the_fifth_operand_is_the_one_named_by_the_too_many_arguments() {
    let (out, status, err) = run(&["3", "1", "5", "2", "9"]);
    assert_eq!((out.as_str(), status), ("", 1));
    assert_eq!(err, "jot: Too many arguments.  What do you mean by 9?\n");

    let (_, status, err) = run(&["1", "2", "3", "4", "5", "6"]);
    assert_eq!(status, 1);
    assert_eq!(
        err, "jot: Too many arguments.  What do you mean by 5?\n",
        "报的是第五个操作数，不是最后一个"
    );

    // 坏的第一操作数在这一支之前轮不到：个数判定先于任何解读。
    let (_, status, err) = run(&["x", "1", "2", "3", "4"]);
    assert_eq!(status, 1);
    assert_eq!(err, "jot: Too many arguments.  What do you mean by 4?\n");
}

/// `reps` 与 `step` 的拒收句都是「两个空格 + 操作数原文 + 句尾无句点」（`jot.c:189 与 :215`）。
#[test]
fn a_rejected_operand_names_itself_after_two_spaces() {
    let (out, status, err) = run(&["x"]);
    assert_eq!((out.as_str(), status), ("", 1));
    assert_eq!(err, "jot: Bad reps value:  x\n");

    let (_, status, err) = run(&["3", "1", "2", "x"]);
    assert_eq!(status, 1);
    assert_eq!(err, "jot: Bad step value:  x\n");
}

/// 环不重排：操作数之后的 `-c` 不再被当选项，而是第四个操作数即 step，
/// `strtod` 全串检查当场拒掉（参照件：`jot 3 65 69 -c` → `Bad step value:  -c`）。
#[test]
fn the_ring_does_not_rearrange_options_after_the_first_operand() {
    let (out, status, err) = run(&["3", "65", "69", "-c"]);
    assert_eq!((out.as_str(), status), ("", 1));
    assert_eq!(err, "jot: Bad step value:  -c\n");

    // `-Z` 落进 begin 那一格：`sscanf("%lf")` 不吃它，于是取最后一个字符的代码点
    // （`jot.c:203-204`），`Z` 是 90。
    let (out, status, err) = run(&["3", "-Z"]);
    assert_eq!(
        (out.as_str(), status),
        ("90\n91\n92\n", 0),
        "stderr 是 {err:?}"
    );
    assert_eq!(err, "");
}

/// 裸 `-` 是操作数而不是 stdin 别名，且它那一格保持默认值；`--` 之后的 `-Z` 是字面操作数。
/// 三个形状都走 step/begin/ender 的默认推导，`jot.c:193-219` 的 FALLTHROUGH 逐格回看。
#[test]
fn a_bare_dash_keeps_the_default_for_its_slot() {
    // 第四个格子给 step，第三格 `-` 让 ender 留默认：reps=3、begin=65、step=2。
    let (out, status, err) = run(&["3", "65", "-", "2"]);
    assert_eq!(
        (out.as_str(), status),
        ("65\n67\n69\n", 0),
        "stderr 是 {err:?}"
    );
    assert_eq!(err, "");

    // 三、四格都留默认，于是 begin=65 配默认 step=1。
    let (out, status, _err) = run(&["3", "65", "-", "-"]);
    assert_eq!((out.as_str(), status), ("65\n66\n67\n", 0));

    let (out, status, _err) = run(&["--", "3", "65"]);
    assert_eq!((out.as_str(), status), ("65\n66\n67\n", 0));
}

/// `-b` 只点起 `boring`，值与 `-w` 写进同一个词槽：后写的覆盖先写的，
/// 而 `boring` 一旦置起不再落下（`jot.c:172-176`）。
#[test]
fn b_and_w_write_into_one_word_slot() {
    let (out, status, err) = run(&["-bX", "-wY", "3"]);
    assert_eq!(
        (out.as_str(), status),
        ("Y\nY\nY\n", 0),
        "stderr 是 {err:?}"
    );
    assert_eq!(err, "");

    let (out, status, _err) = run(&["-bX", "-c", "3"]);
    assert_eq!(
        (out.as_str(), status),
        ("X\nX\nX\n", 0),
        "`boring` 赢过 `-c`"
    );
}

/// `-p` 的接受集由 `strtol(…, 0)` 定：base 0 认 `0x`，空串转不出数字也算全串 consumed，
/// 尾巴一个杂字符都不许剩。这一枚盯的是旧写法那句 `parse::<usize>()`——
/// 它把 `0x2` 与空串两支都拒了，而参照件两支都收。
#[test]
fn the_precision_takes_base_zero_and_requires_a_full_operand() {
    let (out, status, err) = run(&["-p", "0x2", "3"]);
    assert_eq!(
        (out.as_str(), status),
        ("1.00\n2.00\n3.00\n", 0),
        "stderr 是 {err:?}"
    );
    assert_eq!(err, "");

    let (out, status, _err) = run(&["-p", "", "3"]);
    assert_eq!(
        (out.as_str(), status),
        ("1\n2\n3\n", 0),
        "空串是合法的 0，走整数形"
    );

    let (out, status, _err) = run(&["-p", "010", "3"]);
    assert_eq!(
        (out.as_str(), status),
        ("1.00000000\n2.00000000\n3.00000000\n", 0),
        "`010` 是八进 8，即 8 位小数"
    );

    let (out, status, _err) = run(&["-p", "2", "3", "1.5"]);
    assert_eq!((out.as_str(), status), ("1.50\n2.50\n3.50\n", 0));
}
