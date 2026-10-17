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
//! 分母那一枚读的是 C 的 `double`（`strtod` 的十进制腿）：`1e3`、`1e-3`、`.5`、`-2`、
//! `inf`、`nan` 都认；`18e308`（溢出）与 `1e-400`（下溢成 0）都立 `errno == ERANGE`
//! 走 ③。还敞着的是十六进制浮点 `0x1p3` 与 `nan(0x…)` 载荷两形，以及 `emit` 写缝的退码。

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
    // ② 的射程由 C 的两判据划定：`e5`（一个字符都没吃）、`1e`（指数位空着、`ep` 停在
    // `e`）、`infi`（`inity` 比到第一格就退回）、`3 `（尾巴不吃）、`0e99999`（全零不立
    // ERANGE，落回「不是有效分母」而不是 ③）
    for word in ["e5", "1e", "infi", "3 ", "0e99999"] {
        let (out, code, err) = run(&[word], INPUT);
        assert_eq!(
            (out.as_str(), code, err.as_str()),
            ("", 1, "random: denominator is not valid.\n"),
            "{word:?}"
        );
    }
}

#[test]
fn an_overflowed_operand_uses_the_three_part_err_shape() {
    // `err(1, "%s", *argv)`（`:96-97`）比 ② 多一个位：操作数坐在中间，尾巴是号对应的句子。
    // 句子出自 `minix-types` 的 `SYS_ERRLIST`（`os/libs/minix-types/src/types/errno.rs:426`
    // 的第 34 项 `Result too large or too small`）；宿主上的参照件走 glibc 那张表
    // （`Numerical result out of range`），那是承载差异，`errno.rs:383` 已把这一对写下
    // 溢出（`strtod.c` 的 `ovfl`）与非零数字串下溢成 0（`undfl`）汇到同一个 `range_err`，
    // 两形都报 ③。`4294967296` 不在这一列：它是 `double` 里精确的 2^32，参照件现量退 0
    // （`tmp/random-343/ab343-ref.log`）——这条门过去钉的是实现的 u32 溢出，不是参照件
    for word in ["18e308", "1e-400"] {
        let (out, code, err) = run(&[word], INPUT);
        let want = format!("random: {word}: Result too large or too small\n");
        assert_eq!(
            (out.as_str(), code, err.as_str()),
            ("", 1, want.as_str()),
            "{word:?}"
        );
        // 只有一行：三段形之后不跟 usage 块，也不追加第二句
        assert_eq!(
            err.lines().filter(|line| !line.is_empty()).count(),
            1,
            "三段形应当只有一行：{err:?}"
        );
    }
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
    // 溢出那一枚换成 `18e308`：`4294967296` 在 `double` 里是精确的 2^32，认下了不报错
    let (_, code, err) = run_with_argv0("renamed-random", &["18e308"], INPUT);
    assert_eq!(
        (code, err.as_str()),
        (1, "renamed-random: 18e308: Result too large or too small\n")
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
#[test]
fn an_unknown_letter_gets_the_libc_line_then_the_usage_line() {
    // 两句的顺序与前缀各不相同：第一行走 `getprogname()`，第二行的 `random` 写死在串里。
    // 字样取 Minix 自己的 `minix3/lib/libc/stdlib/getopt.c:101-104`（`unknown option -- %c`，
    // 字母不带引号）。参照件这一形是宿主 glibc 的 `invalid option -- 'Z'`，不承载真值
    for args in [&["-Z"][..], &["-eZ"][..], &["-Ze"][..]] {
        let (out, code, err) = run(args, INPUT);
        assert_eq!(
            (out.as_str(), code, err.as_str()),
            (
                "",
                1,
                "random: unknown option -- Z\nusage: random [-er] [denominator]\n"
            ),
            "{args:?}"
        );
    }
}

#[test]
fn dash_dash_ends_the_ring_and_the_rest_are_operands() {
    // `-- 1`：环在 `--` 处结束，后面那枚才是分母 ⇒ 每行必中，输出与输入逐字节相同
    let (out, code, err) = run(&["--", "1"], INPUT);
    assert_eq!((out.as_str(), code, err.as_str()), (INPUT_TEXT, 0, ""));
    // 光写 `--` 等于零枚操作数：分母取 2，照常过滤；这一形**允许**空输出（掷不中不是错误），
    // 所以只钉退码与标准错误，不去钉那些随机量
    let (_, code, err) = run(&["--"], INPUT);
    assert_eq!((code, err.as_str()), (0, ""));
    // 裸 `-` 不是结束标记也不是旗标：它就是那枚被否掉的操作数（三百四十三的门里也有这一形）
    let (_, code, err) = run(&["-e", "-"], INPUT);
    assert_eq!(
        (code, err.as_str()),
        (1, "random: denominator is not valid.\n")
    );
}

#[test]
fn the_e_flag_returns_a_draw_and_reads_nothing() {
    // `random.c:109-111`：这一支在 `getchar()` 环之前就 return，所以标准输出必然为空、
    // 标准错误必然为空，退码落在 `(denom * random()) / RANDOM_MAX` 的闭区间 [0, denom] 里
    let (out, code, err) = run(&["-e", "1"], INPUT);
    assert_eq!((out.as_str(), code, err.as_str()), ("", 0, ""));
    for _ in 0..12 {
        let (out, code, err) = run(&["-e", "4"], INPUT);
        assert!(
            (out.is_empty()) && err.is_empty(),
            "out={out:?} err={err:?}"
        );
        assert!(
            (0..=4).contains(&code),
            "`-e 4` 打出退码 {code}，越出闭区间 [0, 4]"
        );
    }
    // `-r` 只是关缓冲：写缝本来就逐行落盘，因此它与不加 `-r` 同形（六行全中）
    let (out, code, err) = run(&["-r", "1"], INPUT);
    assert_eq!((out.as_str(), code, err.as_str()), (INPUT_TEXT, 0, ""));
}

#[test]
fn a_scientific_denominator_is_accepted_as_a_double() {
    // `1e3` 走 C 的指数分支（`strtod.c` 的 `if (c == 'e' || c == 'E')`）：认下之后照常
    // 过滤。1/1000 的掷中率下六行输入可能一行都不中，所以钉的是「不退 1、不报错」这一对
    let (out, code, err) = run(&["1e3"], INPUT);
    assert_eq!((code, err.as_str()), (0, ""), "stdout={out:?}");
    assert_subset_of_input(&out, INPUT_TEXT, "分母 1e3");
}

#[test]
fn a_small_fraction_denominator_prints_every_line() {
    // `1e-3`：`(int)(1e-3 * random() / RANDOM_MAX)` 商最大约 4.7e-13，向零截恒为 0，
    // 于是每一行都掷中——输出与输入逐字节相同。分母小于 1 在 C 里是一条合法的「全通过」
    let text = many_lines(60);
    let (out, code, err) = run(&["1e-3"], text.as_bytes());
    assert_eq!((code, err.as_str()), (0, ""), "stdout={out:?}");
    assert_eq!(out, text);
}

#[test]
fn a_negative_denominator_is_accepted_and_prints_every_line() {
    // 负数不是「非法分母」：C 从没说过 `denom` 必须为正，只说它不能为 0、尾巴必须吃干净。
    // `-1e-3` 的商落进 [-4.7e-13, 0]，向零截恒为 0 → 每行都掷中
    let text = many_lines(60);
    let (out, code, err) = run(&["--", "-1e-3"], text.as_bytes());
    assert_eq!((code, err.as_str()), (0, ""), "stdout={out:?}");
    assert_eq!(out, text);
}

#[test]
fn a_big_integer_denominator_is_not_out_of_range() {
    // `2^32` 在 `double` 里精确表示，`strtod` 不立 ERANGE：本支过去按 u32 面判成溢出，
    // 于是把参照件退 0 的一形报成了三段形
    let (out, code, err) = run(&["4294967296"], INPUT);
    assert_eq!((code, err.as_str()), (0, ""), "stdout={out:?}");
    assert_subset_of_input(&out, INPUT_TEXT, "分母 2^32");
}

#[test]
fn infinity_and_nan_are_accepted_and_never_select_a_line() {
    // `inf`/`nan` 靠 `INFNAN_CHECK`（`gdtoaimp.h:191` 与 `:643-645` 都开着）认下。
    // C 的判据是「截成整数再与 0 比」：非有限值→整数在 C 是未定义行为，参照件所在的
    // x86-64 给 `0x80000000`，不为 0 → 一行都不掷中，但退码仍是 0、不报错
    let text = many_lines(40);
    for word in ["inf", "nan"] {
        let (out, code, err) = run(&[word], text.as_bytes());
        assert_eq!((out.as_str(), code, err.as_str()), ("", 0, ""), "{word:?}");
    }
}

#[test]
fn leading_whitespace_is_skipped_and_trailing_is_not() {
    // C 的开头 `switch (*s)` 吃掉空白，尾巴上的不吃：`" 3"` 认下、`"3 "` 因 `*ep != '\0'`
    // 落到 ②。两形只差一个空格，走的是两条出口
    let (out, code, err) = run(&[" 3"], INPUT);
    assert_eq!((code, err.as_str()), (0, ""), "stdout={out:?}");
    assert_subset_of_input(&out, INPUT_TEXT, "前导空白");
    let (out, code, err) = run(&["3 "], INPUT);
    assert_eq!(
        (out.as_str(), code, err.as_str()),
        ("", 1, "random: denominator is not valid.\n")
    );
}
