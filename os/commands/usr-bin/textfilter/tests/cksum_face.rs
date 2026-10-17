//! End-to-end face for the `cksum` binary's option diagnostics, checked against
//! `minix3/usr.bin/cksum/cksum.c`（选项环 `:193-278`、`requirehash` `:521-525`、
//! `usage()` `:535-550`）与它驱动的 libc `getopt`
//!（`minix3/lib/libc/stdlib/getopt.c:101-104`、`:132-136`）。
//!
//! 三种形状各不隶属，旧写法把它们压成一种：
//! ① getopt 自己报的（集外字母、带值选项缺值）是「libc 一行 + 九行 usage」；
//! ② 命令自己的 `warnx`（`illegal argument to -a|-o option`、`<旗标> flag requires
//!    `-a algorithm'`）也是「一行 + 同一块 usage」，但**句子不同**；
//! ③ usage 块里的程序名在 C 里是**字面量**（`cksum`/`sum`/`md2`…），不跟着 `argv[0]` 走，
//!    而①那一行跟着 `getprogname()`。

mod face_support;

use face_support::write_stdin;
use std::process::{Command, Stdio};

const CKSUM: &str = env!("CARGO_BIN_EXE_cksum");

/// C `usage()` 的九行原文（`cksum.c:535-549`）。
const USAGE: &str = concat!(
    "usage: cksum [-n] [-a algorithm [-ptx] [-s string]] [-o 1|2]\n",
    "             [file ... | -c [-w] [sumfile]]\n",
    "       sum [-n] [-a algorithm [-ptx] [-s string]] [-o 1|2]\n",
    "           [file ... | -c [-w] [sumfile]]\n",
    "       md2 [-nptx] [-s string] [file ... | -c [-w] [sumfile]]\n",
    "       md4 [-nptx] [-s string] [file ... | -c [-w] [sumfile]]\n",
    "       md5 [-nptx] [-s string] [file ... | -c [-w] [sumfile]]\n",
    "       rmd160 [-nptx] [-s string] [file ... | -c [-w] [sumfile]]\n",
    "       sha1 [-nptx] [-s string] [file ... | -c [-w] [sumfile]]\n",
);

fn run(args: &[&str], input: &[u8]) -> (String, i32, String) {
    let mut child = Command::new(CKSUM)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the cksum binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// 集外字母：libc 的 `unknown option`（**不是** `illegal option`，那是 `pr` 的 `egetopt` 措辞），
/// 后接整块 usage，退 1（C 的 `usage()` 是 `exit(1)`，旧写法退 2）。
#[test]
fn an_unknown_letter_prints_the_libc_line_then_the_nine_line_usage() {
    let (out, status, err) = run(&["-Z"], b"a");
    assert_eq!(out, "", "报文只走 stderr");
    assert_eq!(status, 1, "C 的 usage() 退 1，stderr 是 {err:?}");
    assert_eq!(err, format!("cksum: unknown option -- Z\n{USAGE}"));
}

/// 带值选项缺值是 libc 的另一句，同样接 usage；两个字母都要试（`-a` 与 `-o`）。
#[test]
fn a_value_taking_letter_without_its_value_prints_the_libc_line() {
    for letter in ['a', 'o', 's'] {
        let (out, status, err) = run(&[&format!("-{letter}")], b"a");
        assert_eq!(out, "");
        assert_eq!(status, 1, "-{letter} 的 stderr 是 {err:?}");
        assert_eq!(
            err,
            format!("cksum: option requires an argument -- {letter}\n{USAGE}"),
            "-{letter} 缺值"
        );
    }
}

/// 值不合法是命令自己的 `warnx`，句子跟着字母走；与①的区别在句子，不在行数。
#[test]
fn an_illegal_option_value_names_its_own_letter_in_warnx() {
    let (_, status, err) = run(&["-o", "9"], b"a");
    assert_eq!(status, 1);
    assert_eq!(
        err,
        format!("cksum: illegal argument to -o option\n{USAGE}")
    );
    let (_, status, err) = run(&["-a", "nosuch"], b"a");
    assert_eq!(status, 1);
    assert_eq!(
        err,
        format!("cksum: illegal argument to -a option\n{USAGE}")
    );
}

/// C 的 `requirehash`：`<旗标> flag requires `-a algorithm'` 再接 usage。
/// 旧写法自创成 `requires a hash algorithm` 且不接 usage、还退 2。
#[test]
fn the_hash_only_flags_report_requirehash_then_usage() {
    for flag in ['p', 't', 'x'] {
        let (_, status, err) = run(&[&format!("-{flag}")], b"a");
        assert_eq!(status, 1, "-{flag} 的 stderr 是 {err:?}");
        assert_eq!(
            err,
            format!("cksum: -{flag} flag requires `-a algorithm'\n{USAGE}"),
            "-{flag} 该走 requirehash"
        );
    }
}

/// getopt 的形状本身要能从外面验：聚簇（`-nq`）与附值（`-o2`）都是合法调用，
/// 旧写法按整个 token 匹配字母，会把它们当集外字母拒掉。
#[test]
fn clustered_and_inline_letters_are_accepted() {
    let (out, status, err) = run(&["-nq"], b"abc");
    assert_eq!((status, err.as_str()), (0, ""), "stderr 是 {err:?}");
    // 输出的具体数字属另一面（`src/cksum.rs` 的库测已钉 crc 值），这里只要求「被接受且有产出」。
    assert!(!out.is_empty(), "被接受就该有校验和输出");
    let (_, status, err) = run(&["-o2"], b"abc");
    assert_eq!((status, err.as_str()), (0, ""), "stderr 是 {err:?}");
    // `--` 之后的负向词是操作数，不再当旗标（这里它是个读不到的档名）。
    let (_, status, _) = run(&["--", "-o"], b"a");
    assert_eq!(status, 1, "`--` 后该把 -o 当操作数");
}

/// 状态驱动的第一条诊断：`-a` 在已选中 hash 之后再次出现。C 的 `case 'a'` 第一行就是
/// 这条（`cksum.c:195-199`），`warnx` 两段形（不缀 strerror）再接同一块九行 usage，退 1。
/// 本壳以前到不了这一形——`-a md5` 在解析阶段就被自创的占位句挡住，于是第二枚 `-a`
/// 根本没机会被看到；批次三百三十三把「选中」记成状态之后，这条诊断的先后次序就照 C 走了。
#[test]
fn a_second_dash_a_after_a_selection_reports_illegal_use() {
    let (out, status, err) = run(&["-a", "md5", "-a", "sha1"], b"");
    assert_eq!(out, "", "诊断形不产出行");
    assert_eq!(
        err,
        format!("cksum: illegal use of -a option\n{USAGE}"),
        "该是 warnx 那一行接 usage"
    );
    assert_eq!(status, 1);
    // 第二枚的名字好不好都在所不问：这条诊断先看状态，`crc` 同样被它挡住。
    let (_, _, err2) = run(&["-a", "md5", "-a", "crc"], b"");
    assert_eq!(err2, err, "报的是「-a 用错了」而不是名字问题");
}

/// 状态驱动的第二条诊断：`-o` 在已选中 hash 之后出现。C 打的是
/// `warnx("%s mutually exclusive with sum", hash->hashname)`（`cksum.c:228-234`），
/// 句子里的名字是 `hashes[]` 表里**大写**那一份（`:126` 的 `"MD5"`），大小写入口不影响它。
#[test]
fn dash_o_after_a_selection_names_the_hash_and_exits_one() {
    for word in ["md5", "MD5", "Md5"] {
        let (out, status, err) = run(&["-a", word, "-o", "1"], b"");
        assert_eq!(out, "");
        assert_eq!(
            (status, err.as_str()),
            (
                1,
                format!("cksum: MD5 mutually exclusive with sum\n{USAGE}").as_str()
            ),
            "`-a {word}` 之后 `-o 1` 该报大写的 MD5"
        );
    }
    // 反过来先 `-o` 再 `-a` 就不冲突：这时状态还是空的，C 也照样走下去
    // （本壳走到占位那一句，见下一枚门）。
    let (_, status, err) = run(&["-o", "1", "-a", "md5"], b"");
    assert_eq!(
        (status, err.as_str()),
        (2, "cksum: -a crypto algorithms are not modelled yet\n"),
        "先 -o 再 -a 不报互斥"
    );
}

/// 选中之后到占位之间的那段路：`-p`/`-s` 这类要先过 `requirehash` 的旗标，在 C 里
/// 一旦 `hash` 已选中就**放行**（`cksum.c:521-525` 的条件不成立），所以它们不该再报
/// `-p flag requires \`-a algorithm'`；本壳以前无条件报，是因为状态没建模。
/// 放行之后它们会一路走到占位那一句（原语尚未接，见 `CKSUM-HASH-STATE` 第二片）。
#[test]
fn flags_requiring_a_hash_pass_once_a_hash_is_selected() {
    let (_, s_p, e_p) = run(&["-a", "sha1", "-p"], b"");
    assert!(
        !e_p.contains("requires `-a algorithm'"),
        "选中之后 -p 不该再要求 -a：stderr 是 {e_p:?}"
    );
    assert_eq!(
        (s_p, e_p.as_str()),
        (2, "cksum: -a crypto algorithms are not modelled yet\n")
    );
    let (_, s_s, e_s) = run(&["-a", "sha1", "-s", "abc"], b"");
    assert!(
        !e_s.contains("requires `-a algorithm'"),
        "选中之后 -s 同理：stderr 是 {e_s:?}"
    );
    assert_eq!(s_s, 2, "`-s` 的值被吃掉之后一路走到占位句");
    // 没选中时仍是批次三百零七钉下的那一句（对照，证明放行是由状态决定的）。
    let (_, s0, e0) = run(&["-p"], b"");
    assert_eq!(
        (s0, e0.as_str()),
        (
            1,
            format!("cksum: -p flag requires `-a algorithm'\n{USAGE}").as_str()
        )
    );
}
