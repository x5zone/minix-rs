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

/// 与 `run` 同一条命令缝，但**逐字节**交回：`from_utf8_lossy` 把一个裸 `c3` 渲染成三字节
/// U+FFFD，文本形分不清「一个字节」与「两个字节」这两种错法（`rev`/`paste` 那几支同此）。
fn run_bytes(args: &[&str]) -> (Vec<u8>, i32, Vec<u8>) {
    let output = Command::new(JOT)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("the jot binary should be runnable");
    (
        output.stdout,
        output.status.code().unwrap_or(-1),
        output.stderr,
    )
}

/// 集外字母那一行的字母位是**一个裸字节**：参照件 `tmp/jot-317/jot_ref` 现量 `-é` 与 `-cé`
/// 都是 `jot: unknown option -- ` + `c3` + 换行，再跟 usage、退 1（`c` 是 `"b:cnp:rs:w:"` 里
/// 合法且不带值的字母，聚簇继续吃到 `é` 的第一字节；`-Zé` 停在 `Z`，`jot.c:151-152` 拿到
/// `'?'` 就 `usage()`，第二个字母轮不到）。`-bé 3` 是这一族的反面：`b` 带值，`é` 整个被当值
/// 收走，退 0 把那枚词按 `reps` 重复三行——换渲染不许碰取值那半。ASCII 的 `-Z` 与聚簇只报第一枚两形本文件
/// 各已有主人（`an_unknown_letter_prints_the_libc_line_then_the_usage`、
/// `a_cluster_reports_only_its_first_bad_letter`），这里不重复承载。
#[test]
fn the_libc_line_of_a_multibyte_flag_is_a_single_raw_byte() {
    let multibyte: Vec<u8> = [
        b"jot: unknown option -- ".as_slice(),
        b"\xc3\n",
        USAGE.as_bytes(),
    ]
    .concat();
    for form in ["-\u{e9}", "-c\u{e9}"] {
        let (out, code, err) = run_bytes(&[form, "3"]);
        assert_eq!(out, Vec::new(), "{form} 的报文只走 stderr");
        assert_eq!(code, 1, "{form} 的 stderr 是 {err:?}");
        assert_eq!(err, multibyte, "{form} 的字母该是一个裸字节 c3");
    }

    let (_, code, err) = run_bytes(&["-Z\u{e9}", "3"]);
    assert_eq!(code, 1);
    assert_eq!(
        err,
        [b"jot: unknown option -- Z\n".as_slice(), USAGE.as_bytes(),].concat(),
        "环停在第一个坏字母上，`é` 根本不该被读到"
    );

    let (out, code, err) = run_bytes(&["-b\u{e9}", "3"]);
    // 参照件 `jot_ref -bé 3` 现量三行 `c3 a9 0a`（`-b` 把那枚词重复 reps 次），退 0 且不写 stderr。
    assert_eq!(
        (out.as_slice(), code),
        ("\u{e9}\n\u{e9}\n\u{e9}\n".as_bytes(), 0),
        "{err:?}"
    );
    assert_eq!(err, Vec::new(), "`-b` 的值不是字母位，多字节字符照原样出去");
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

#[test]
fn four_operands_with_a_zero_step_stay_bounded() {
    // 参照件 `tmp/jot-317/jot_ref` 现量：`jot 3 1 100 0` 是三行 `1`，退 0；
    // 派生 switch 曾把它误派到 `case ENDER | STEP` 那一支而 reps 归零，于是永不终止
    for args in [&["3", "1", "100", "0"][..], &["3", "1", "1", "0"][..]] {
        let (out, code, err) = run(args);
        assert_eq!((code, err.as_str()), (0, ""), "{args:?}");
        assert_eq!(out, "1\n1\n1\n", "{args:?}");
    }
    // 步长非零时取「给定的 reps」与「算出的个数」里较小的一个：`jot 3 1 100 5` 是三行而不是二十行
    let (out, code, err) = run(&["3", "1", "100", "5"]);
    assert_eq!((code, err), (0, String::new()));
    assert_eq!(out, "1\n6\n11\n");
    // 算出的更小就用算出的（`jot 2 1 100 5` 两行；`jot 20 1 100 5` 二十行，此处只钉前两行）
    let (out, code, _) = run(&["2", "1", "100", "5"]);
    assert_eq!((code, out.as_str()), (0, "1\n6\n"));
    // reps 缺省而 begin/ender/step 三个都给：C 用 `case ENDER | STEP | BEGIN` 的同一段体算 reps
    // （`(10 - 1 + 2) / 2` = 5），本层少写这一个标签时它会落进 `default` 而按默认 100 行走
    let (out, code, _) = run(&["-", "1", "10", "2"]);
    assert_eq!((code, out.as_str()), (0, "1\n3\n5\n7\n9\n"));
    // 六个给定 reps 但 ender 已在步内 reached 的形：C 仍按 reps 与算出值取小，这里打六行 `1`
    let (out, code, _) = run(&["6", "1", "10", "0"]);
    assert_eq!((code, out.as_str()), (0, "1\n1\n1\n1\n1\n1\n"));
}

#[test]
fn a_negative_or_fractional_seed_is_accepted_like_c() {
    // `jot.c:118` 是 `srandom((unsigned long) step)`：负数与小数都是**合法**种子。
    // 参照件 `jot -r 3 1 10 -5` 退 0 打三个整数（本层曾用自创的 `v >= 0.0` 报
    // `Bad step value`）。随机流的**数值**是另一档分歧（本层的 LCG 不是 libc 的
    // `random()`，见 07 §4.4），所以这里只钉退码、行数、行数与「全是数字」这三件。
    for arg in ["-5", "-2147483648", "-0.5", "0", "5"] {
        let (out, code, err) = run(&["-r", "3", "1", "10", arg]);
        assert_eq!(code, 0, "种子 {arg} 的 stderr={err:?}");
        assert_eq!(err, "", "种子 {arg} 不该有诊断");
        let lines: Vec<&str> = out.trim_end_matches('\n').split('\n').collect();
        assert_eq!(lines.len(), 3, "种子 {arg} 实际输出 {out:?}");
        assert!(
            lines.iter().all(|l| l.bytes().all(|b| b.is_ascii_digit())),
            "种子 {arg} 输出不全是整数：{out:?}"
        );
    }
    // 截断取整的可观察推论：C 的 `(unsigned long)(-0.5)` 就是 0，
    // 所以 `-0.5` 与 `0` 两串数必须逐字节相同（参照件现量同为 `9\n4\n8\n`）
    let (a, _, _) = run(&["-r", "3", "1", "10", "-0.5"]);
    let (b, _, _) = run(&["-r", "3", "1", "10", "0"]);
    assert_eq!(a, b, "小数种子要先截断，不能当成坏值");
    // `strtod` 的全串检查没动：尾巴带杂字符仍然报同一句、退 1
    let (out, code, err) = run(&["-r", "3", "1", "10", "5x"]);
    assert_eq!((code, out.as_str()), (1, ""));
    assert_eq!(err, "jot: Bad step value:  5x\n");
}

/// 登记里的四形（`jot 3`、`-n 3`、`3 - 5`、`-s , 3`）都是 `prec` 从未被写过那一族。
/// 这一形在进程级能钉的是形状：每行不带小数点（精度 0 的浮点，或取整后的整数），
/// 也不带 `%`（宿主 glibc 把 `%.-1f` 当坏说明符打回字面量 `%.0-1f`，那是参照件所链
/// libc 的产物，不是 Minix 的真值，所以本门刻意不拿参照件的这一形当判据）。
#[test]
fn the_unwritten_precision_family_prints_no_decimal_point_and_no_literal() {
    for args in [
        &["3"][..],
        &["-n", "3"][..],
        &["3", "-", "5"][..],
        &["-s", ",", "3"][..],
    ] {
        let (out, status, err) = run(args);
        assert_eq!(status, 0, "{args:?} 的退码：stderr 是 {err:?}");
        assert_eq!(err, "", "{args:?} 不该有诊断");
        assert!(!out.contains('.'), "{args:?} 的输出不该出现小数点：{out:?}");
        assert!(
            !out.contains('%'),
            "{args:?} 的输出不该是打回的说明符：{out:?}"
        );
        assert!(!out.is_empty(), "{args:?} 要有输出");
    }
}

/// 字节形 argv 的命令缝：`&str` 递不出裸 `ff`（`OsStr` 从 `str` 过来时那串已是合法 UTF-8），
/// 而 `jot` 的三支带值字母（`-b`/`-s`/`-w`）的值**就是输出内容**，所以这一层夹具是必需的。
/// 两路输出逐字节交回：文本形分不清「一枚字节」与「折叠出的三字节」这两种错法。
fn run_argv(args: &[&[u8]]) -> (Vec<u8>, i32, Vec<u8>) {
    use std::os::unix::ffi::OsStrExt as _;
    let output = Command::new(JOT)
        .args(args.iter().map(|word| std::ffi::OsStr::from_bytes(word)))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("the jot binary should be runnable");
    (
        output.stdout,
        output.status.code().unwrap_or(-1),
        output.stderr,
    )
}

/// 参数缝在 `jot` 上直接改到**打出去的内容**：`-b`/`-w` 的词由 `putdata`（`jot.c:300-311`）
/// 原样送 stdout，`-s` 的串由 `:310` 的 `fputs(sepstring, stdout)` 打在条目之间。文本缝之下
/// 一枚裸 `ff` 先被折成 `ef bf bd`，于是每一行都多两枚字节。反面形是 `%c`：C 走 `putchar`
/// 交回**一枚**字节（参照件对 `-p 0 -w '%c' 1 200 1` 现量 `c8`），而 `v as u8 as char` 经
/// UTF-8 会变成 `c3 88` 两枚——这一支的 `char` 通道在字节形下退出。
#[test]
fn a_raw_byte_in_the_word_or_the_separator_is_printed_as_that_byte() {
    // ①`-b` 的词：两行都是 `ff 77 64`。
    let (out, status, err) = run_argv(&[b"-b\xffwd", b"2"]);
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(err, Vec::new(), "成功路一声不出");
    assert_eq!(out, vec![0xff, b'w', b'd', b'\n', 0xff, b'w', b'd', b'\n']);

    // ②`-s` 的串：条目之间夹的是 `ff 78` 两枚，不是五枚。
    let (out, status, err) = run_argv(&[b"-s\xffx", b"3", b"1", b"5"]);
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(
        out,
        vec![b'1', 0xff, b'x', b'3', 0xff, b'x', b'5', b'\n'],
        "分隔符必须按 argv 的原字节打出去"
    );

    // ③`-w` 的词里，百分号之前的字面段照样进输出。
    let (out, status, err) = run_argv(&[b"-wa\xff%d", b"2"]);
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(out, vec![b'a', 0xff, b'1', b'\n', b'a', 0xff, b'2', b'\n']);

    // ④前缀里的 `%%` 展开成一枚（参照件对 `-p 2 -w '%%x%d' 3 1 5` 现量 `%x1|%x3|%x5`）。
    let (out, status, err) = run(&["-p", "2", "-w%%x%d", "3", "1", "5"]);
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(out, "%x1\n%x3\n%x5\n", "两枚百分号进、一枚百分号出");

    // ⑤`%c` 只交回一枚字节：这是「按字节」在输出端的另一个可观察面。
    let (out, status, err) = run_argv(&[b"-p", b"0", b"-w%c", b"1", b"200", b"1"]);
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(
        out,
        vec![0xc8, b'\n'],
        "`putchar` 那一枚就是 `c8`，不是 `c3 88`"
    );
}

/// 三支把值拼进句子的腿，值位都是 argv 原字节：`unknown or invalid format \`%s'`
/// （`jot.c:385`、`:396`——**这一支此前在本壳根本没有值**，见本轮台账）、
/// `Bad reps value:  %s`（`:215`）、`Too many arguments.  What do you mean by %s?`（`:225`）。
/// 三支都是 `errx`：一行、不接 usage、退 1。另一族是可观察的**判据**差：空 reps 在 C 是合法的
/// 0（`strtoul` 把 `ep` 留在起点），配上 begin/ender 落 `Infinite sequences cannot be bounded`
/// （`:272-275`）而不是那句 Bad reps；空 begin 在 C 是 `argv[i][-1]` 的越界读，宿主参照件现量
/// `jot 3 '' 5` 交回 `1 3 5`（那一格留缺省），而本壳此前是**当场 panic**。
#[test]
fn the_operand_and_the_word_echoed_into_a_sentence_are_the_argv_bytes() {
    // ①说明符校验失败：反引号之间是**整个 format 词**，含那枚裸字节。
    let (out, status, err) = run_argv(&[b"-w%\xff", b"2"]);
    assert_eq!(out, Vec::new(), "报文只走标准错误");
    assert_eq!(status, 1);
    assert_eq!(
        err,
        [
            b"jot: unknown or invalid format `%".as_slice(),
            &[0xff],
            b"'\n".as_slice()
        ]
        .concat(),
        "这一句在 C 带值；不带值就少了一整个可观察面"
    );

    // ②坏 reps：`-- ` 之后是原字节。
    let (out, status, err) = run_argv(&[b"x\xff", b"2"]);
    assert_eq!(out, Vec::new());
    assert_eq!(status, 1);
    assert_eq!(
        err,
        [
            b"jot: Bad reps value:  x".as_slice(),
            &[0xff],
            b"\n".as_slice()
        ]
        .concat(),
        "两个空格是 C 的原句，值位是原字节"
    );

    // ③第五个操作数：报的是第五个，前四个一个都不解读。
    let (out, status, err) = run_argv(&[b"1", b"2", b"3", b"4", b"5\xff"]);
    assert_eq!(out, Vec::new());
    assert_eq!(status, 1);
    assert_eq!(
        err,
        [
            b"jot: Too many arguments.  What do you mean by 5".as_slice(),
            &[0xff],
            b"?\n".as_slice(),
        ]
        .concat()
    );

    // ④空 reps 是合法的 0：无界序列配上 begin/ender 落那句 bounded，而不是 Bad reps。
    let (out, status, err) = run_argv(&[b"", b"1", b"2"]);
    assert_eq!(out, Vec::new(), "无界又不可能界定，一格都不该打");
    assert_eq!(status, 1);
    assert_eq!(err, b"jot: Infinite sequences cannot be bounded\n");

    // ⑤高字节操作数经 `char` 落地所以为负：参照件对 `jot 3 <ff> 5` 现量 `-1 2 5`
    //    （begin = -1，步长 (5-(-1))/2 = 3），按无符号读会打成 `189 97 5` 或 `1 97 189`。
    let (out, status, err) = run_argv(&[b"3", &[0xff][..], b"5"]);
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(out, b"-1\n2\n5\n".to_vec(), "最后一枚字节要经 signed char");

    // ⑥空 begin：那一格保持缺省（参照件现量 `1 3 5`），关键是**不再 panic**。
    let (out, status, err) = run_argv(&[b"3", b"", b"5"]);
    assert_eq!(status, 0, "退码该是 0：stderr 是 {err:?}");
    assert_eq!(
        out,
        b"1\n3\n5\n".to_vec(),
        "begin 留缺省 1，步长由 (5-1)/(3-1) 推出来"
    );
}
