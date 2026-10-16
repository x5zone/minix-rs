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

use std::process::{Command, Stdio};
use face_support::write_stdin;

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
    assert_eq!(err, format!("cksum: illegal argument to -o option\n{USAGE}"));
    let (_, status, err) = run(&["-a", "nosuch"], b"a");
    assert_eq!(status, 1);
    assert_eq!(err, format!("cksum: illegal argument to -a option\n{USAGE}"));
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
