//! End-to-end face for the `random` binary's operand step, checked against
//! `minix3/games/random/random.c`（个数判 `:89-104`、`err(1, "%s", *argv)` `:96-97`、
//! `errx(1, "denominator is not valid.")` `:98-99`、`usage()` `:143-147`）。
//!
//! 期望值全部从参照件 `tmp/random-342/random_ref` 现量，逐形读数在
//! `tmp/random-343/ab343-ref.log`。量出来的形状有四条：
//! ① 零枚操作数**不是**缺参数：C 的 `case 0` 把分母定成 2 然后照常过滤，退 0。
//! ② 一枚被否的操作数报 `random: denominator is not valid.`——前缀跟着 `argv[0]`，
//!    句子逐字带句尾的点，**不含操作数**（`errx` 没有 `%s` 位）。
//! ③ 撞上数值范围那一枚报三段形 `random: <操作数>: <句子>`（`err`），与 ② 是两句话。
//! ④ `usage()` 那一行不缀程序名：`random` 是写死在串里的字面量（`:144`），改名调用它
//!    也不动，而 ② 与 ③ 的前缀会跟着动——`a_renamed_invocation_moves_the_err_prefixes_only`
//!    钉的就是这一对差异。
//!
//! 射程止于操作数那一步。参照件认 `1e3`、`1e-3`、`inf`、`nan` 这些 `strtod` 形
//! （现量退 0），本壳按十进制整数面把它们判进 ② —— 那是「分母的取值域」一件事，
//! 登记在同一条目的第三片；`-e`/`-r` 与「每行独立掷、中者整行写出」的过滤体是第二片，
//! 本文件不提前给它们下结论。

use std::io::Write as _;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};

fn random_bin() -> &'static str {
    env!("CARGO_BIN_EXE_random")
}

/// 用给定的 `argv[0]` 跑一腿：标准输入喂 `input`，收回三路。文本按 lossy 收——报文本身
/// 是 ASCII，而标准输出可能带任意字节。
fn run_with_argv0(arg0: &str, args: &[&str], input: &[u8]) -> (String, i32, String) {
    let mut child = Command::new(random_bin())
        .arg0(arg0)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn random");
    child
        .stdin
        .as_mut()
        .expect("stdin pipe")
        .write_all(input)
        .expect("feed stdin");
    let out = child.wait_with_output().expect("wait random");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        out.status.code().expect("exit code"),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn run(args: &[&str], input: &[u8]) -> (String, i32, String) {
    run_with_argv0("random", args, input)
}

const INPUT: &[u8] = b"aa\nbb\ncc\ndd\nee\nff\n";
const INPUT_TEXT: &str = "aa\nbb\ncc\ndd\nee\nff\n";

/// `count` 行 `line-0`…`line-(count-1)`，用来把「确实在掷」与「一行都没掷中」分开。
fn many_lines(count: usize) -> String {
    let mut text = String::new();
    for index in 0..count {
        text.push_str(&format!("line-{index}\n"));
    }
    text
}

/// 输出必须是输入行的一个子集：过滤器整行写出去，不造新行。
fn assert_subset_of_input(stdout: &str, input: &str, shape: &str) {
    let pool: Vec<&str> = input.lines().collect();
    for line in stdout.lines() {
        assert!(pool.contains(&line), "{shape}: 输出行 {line:?} 不在输入里");
    }
}

#[test]
fn no_operand_filters_with_denominator_two_instead_of_complaining() {
    // C 的 `case 0`（`random.c:90-92`）：一枚操作数都没有时分母取 2，照常读标准输入。
    // 参照件现量退 0、吐出输入行的一个子集、不写标准错误
    // 六行输入只用来验「输出是输入行的子集」，**不验退码**：这具过滤体还有一枚
    // 「一行都没掷中就说 nothing selected 并退 1」的发明（`random.c:127-137` 没有这一形，
    // 它是过滤体那一片的账），六行下「全不中」的概率是 2 的 -6 次方，钉不住退码。
    // 退码交给下面 200 行那一形，那里「全不中」的概率是 2 的 -200 次方
    let six = run(&[], INPUT);
    assert_subset_of_input(&six.0, INPUT_TEXT, "零枚操作数");
    // 掷 200 行而一行都不中，概率是 2 的 -200 次方：非空就是「过滤器确实在跑」的证据
    let text = many_lines(200);
    let (out, code, err) = run(&[], text.as_bytes());
    assert_eq!((code, err.as_str()), (0, ""), "stdout={out:?}");
    assert!(
        !out.is_empty(),
        "零枚操作数时应当掷中若干行，实测一行都没有"
    );
    assert_subset_of_input(&out, &text, "零枚 200 行");
}

#[test]
fn one_operand_still_feeds_the_filter() {
    // 分母 1 ⇒ 每行必中（`draw` 恒为 0），于是这一形是确定的：输出与输入**逐字节相同**。
    // 上一笔这条只敢验「非空且是输入行的子集」，因为那时身体只留最后一枚中者（实测只吐
    // `ff\n`）；过滤体接成 C 的过流形状之后，这里可以钉全等
    let (out, code, err) = run(&["1"], INPUT);
    assert_eq!((out.as_str(), code, err.as_str()), (INPUT_TEXT, 0, ""));
    // 分母 6 落在「全中」与「全不中」之间：只要求退 0、不报错、输出是输入行的子集
    let (out, code, err) = run(&["6"], INPUT);
    assert_eq!((code, err.as_str()), (0, ""), "stdout={out:?}");
    assert_subset_of_input(&out, INPUT_TEXT, "分母 6");
}

#[test]
fn a_rejected_operand_says_the_errx_sentence_without_naming_itself() {
    // 参照件现量：`0`、`abc`、`-` 三形都是 `random_ref: denominator is not valid.`、退 1、
    // 标准输出空。前缀在这里是 `random`（即 `argv[0]`），句子不含操作数
    for word in ["0", "", "abc", "-", "6x"] {
        let (out, code, err) = run(&[word], INPUT);
        assert_eq!(
            (out.as_str(), code, err.as_str()),
            ("", 1, "random: denominator is not valid.\n"),
            "{word:?}"
        );
    }
    // 取值域那一腿还没接：`1e3` 在参照件里退 0（现量见 `ab343-ref.log`），本壳报 ②
    let (out, code, err) = run(&["1e3"], INPUT);
    assert_eq!(
        (out.as_str(), code, err.as_str()),
        ("", 1, "random: denominator is not valid.\n"),
        "登记在第三片的取值域分叉，此断言翻红就是那一腿落地的时刻"
    );
}

#[test]
fn an_overflowed_operand_uses_the_three_part_err_shape() {
    // `err(1, "%s", *argv)`（`:96-97`）比 ② 多一个位：操作数坐在中间，尾巴是号对应的句子。
    // 句子出自 `minix-types` 的 `SYS_ERRLIST`（`os/libs/minix-types/src/types/errno.rs:426`
    // 的第 34 项 `Result too large or too small`）；宿主上的参照件走 glibc 那张表
    // （`Numerical result out of range`），那是承载差异，`errno.rs:383` 已把这一对写下
    let (out, code, err) = run(&["4294967296"], INPUT);
    assert_eq!(
        (out.as_str(), code, err.as_str()),
        ("", 1, "random: 4294967296: Result too large or too small\n")
    );
    // 只有一行：三段形之后不跟 usage 块，也不追加第二句
    assert_eq!(
        err.lines().filter(|line| !line.is_empty()).count(),
        1,
        "三段形应当只有一行：{err:?}"
    );
}

#[test]
fn two_or_more_operands_print_the_hardwired_usage_line() {
    // 参照件现量 `usage: random [-er] [denominator]\n`、退 1、标准输出空——注意这一行
    // **没有**程序名前缀，且把 `-er` 写了出来
    for args in [&["3", "4"][..], &["3", ""][..], &["3", "4", "5"][..]] {
        let (out, code, err) = run(args, INPUT);
        assert_eq!(
            (out.as_str(), code, err.as_str()),
            ("", 1, "usage: random [-er] [denominator]\n"),
            "{args:?}"
        );
    }
    // 个数判的是个数，不是内容：一枚空串仍走 ②，绝不落到 usage
    let (_, code, err) = run(&[""], INPUT);
    assert_eq!(
        (code, err.as_str()),
        (1, "random: denominator is not valid.\n")
    );
}

#[test]
fn a_renamed_invocation_moves_the_err_prefixes_only() {
    // 同一支命令里两种前缀来历：`errx`/`err` 走 `getprogname()`（即 `argv[0]`），
    // `usage()` 把 `random` 写死在串里（`:144`）。参照件里被否那一形的确带着 `random_ref:`
    let (_, code, err) = run_with_argv0("renamed-random", &["abc"], INPUT);
    assert_eq!(
        (code, err.as_str()),
        (1, "renamed-random: denominator is not valid.\n")
    );
    let (_, code, err) = run_with_argv0("renamed-random", &["4294967296"], INPUT);
    assert_eq!(
        (code, err.as_str()),
        (
            1,
            "renamed-random: 4294967296: Result too large or too small\n"
        )
    );
    let (_, code, err) = run_with_argv0("renamed-random", &["3", "4"], INPUT);
    assert_eq!(
        (code, err.as_str()),
        (1, "usage: random [-er] [denominator]\n")
    );
}
#[test]
fn selected_lines_come_out_in_input_order() {
    // C 是一趟过流（`random.c:126-137`）：选中就 `putchar`，所以输出必然是输入行的
    // **保序子序列**。「只留最后一枚中者」在这种输入下当场被照出来
    let text = many_lines(200);
    let (out, code, err) = run(&[], text.as_bytes());
    assert_eq!((code, err.as_str()), (0, ""), "stdout={out:?}");
    let pool: Vec<&str> = text.lines().collect();
    let printed: Vec<&str> = out.lines().collect();
    assert!(!printed.is_empty(), "200 行、分母 2 却一行都没掷中");
    let mut cursor = 0usize;
    for line in printed {
        let found = pool[cursor..]
            .iter()
            .position(|candidate| *candidate == line)
            .unwrap_or_else(|| {
                panic!("输出行 {line:?} 不在游标 {cursor} 之后：顺序被打乱或造了新行")
            });
        cursor += found + 1;
    }
}

#[test]
fn an_empty_selection_is_silent_and_exits_zero() {
    // C 没有「一行都没掷中」这一说：环走完直接 `exit(0)`（`random.c:114-137`）。
    // 分母 40 亿配一行输入，掷中的概率是 2.5e-10，因此这一形是确定的空输出。
    // 薄壳曾自创一句 `random: nothing selected` 并退 1，参照件里没有这一形
    let (out, code, err) = run(&["4000000000"], b"aa\n");
    assert_eq!((out.as_str(), code, err.as_str()), ("", 0, ""));
}

#[test]
fn an_unreadable_stdin_blames_the_stdin_word_and_exits_two() {
    // `ferror(stdin)` 那一路是 `err(2, "stdin")`（`random.c:135-136`）：三段形，位上是
    // `stdin` 这个词而不是档名，退码是 2。参照件现量把目录当标准输入递进去 →
    // `random_ref: stdin: Is a directory`、退 2（`tmp/random-345/ab345-ref.log` 末行）。
    // 句子在本树取 `minix-types` 的表第 21 项（`errno.rs:413`），与 glibc 同字
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("stdin-is-a-dir");
    std::fs::create_dir_all(&dir).expect("create dir");
    let handle = std::fs::File::open(&dir).expect("open a directory read-only");
    let child = Command::new(random_bin())
        .arg("3")
        .stdin(Stdio::from(handle))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn random");
    let bytes = child.wait_with_output().expect("wait random");
    let stdout = String::from_utf8_lossy(&bytes.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&bytes.stderr).into_owned();
    assert_eq!(
        (
            stdout.as_str(),
            bytes.status.code().expect("exit code"),
            stderr.as_str()
        ),
        ("", 2, "random: stdin: Is a directory\n")
    );
}
