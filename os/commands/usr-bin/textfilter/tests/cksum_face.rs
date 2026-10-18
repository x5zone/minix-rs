//! End-to-end face for the `cksum` binary: both the option diagnostics and the
//! `-a` digest legs, checked against `minix3/usr.bin/cksum/cksum.c`（选项环
//! `:193-278`、摘要腿 `:474-491`、`hash_digest_file` `:506-524`、`requirehash`
//! `:521-525`、`usage()` `:535-550`）与它驱动的 libc `getopt`
//!（`minix3/lib/libc/stdlib/getopt.c:101-104`、`:132-136`）。
//!
//! 三种形状各不隶属，旧写法把它们压成一种：
//! ① getopt 自己报的（集外字母、带值选项缺值）是「libc 一行 + 九行 usage」；
//! ② 命令自己的 `warnx`（`illegal argument to -a|-o option`、`<旗标> flag requires
//!    `-a algorithm'`）也是「一行 + 同一块 usage」，但**句子不同**；
//! ③ usage 块里的程序名在 C 里是**字面量**（`cksum`/`sum`/`md2`…），不跟着 `argv[0]` 走，
//!    而①那一行跟着 `getprogname()`。
//!
//! 摘要那一族的报文形另有四种（三种操作数形状加标准输入），它们的期望值不是算给
//! 自己看的：出处是本页末尾注明的参照件读数，参照件由本仓的 C 原件编出。

mod face_support;

use face_support::write_stdin;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};

const CKSUM: &str = env!("CARGO_BIN_EXE_cksum");

/// 摘要那几条腿要真开档、真读字节，所以得有临时档；`cksum` 的报文里带的就是操作数
/// 原文，故路径必须是进程独占的（同一台机器上并发跑两个测试进程不能撞名）。
fn scratch(name: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!("cksum-face-{}-{name}", std::process::id()));
    path
}

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
    // 反过来先 `-o` 再 `-a` 就不冲突：这时状态还是空的，C 也照样走下去——而 C 走下去之后
    // 只看 `hash`（`cksum.c:478-483` 的 `if (hash != NULL)` 在 `cfncn` 之前），`-o 1` 被
    // 整个忽略，印的是标准输入的摘要。批次三百八十四把摘要本体接上之后，这一形第一次
    // 有了真读数。
    let (out, status, err) = run(&["-o", "1", "-a", "md5"], b"");
    assert_eq!(
        (out.as_str(), status, err.as_str()),
        ("d41d8cd98f00b204e9800998ecf8427e\n", 0, ""),
        "`-o 1` 之后选中摘要就该压过 `-o`"
    );
}

/// 选中之后到占位之间的那段路：`-p`/`-s` 这类要先过 `requirehash` 的旗标，在 C 里
/// 一旦 `hash` 已选中就**放行**（`cksum.c:521-525` 的条件不成立），所以它们不该再报
/// `-p flag requires \`-a algorithm'`；本壳以前无条件报，是因为状态没建模。
/// 放行之后两条腿分岔：`-p` 只动 `pflag`（`cksum.c:250`），标准输入那一腿照旧要跑，
/// 于是批次三百八十四接上摘要本体之后它印的是真摘要；`-s` 还置 `nohashstdin`
///（`:258`）并且自己那一句要靠 `stringfunc`，那条腿仍在占位句上（`CKSUM-HASH-STATE` 的
/// 下一片）。
#[test]
fn flags_requiring_a_hash_pass_once_a_hash_is_selected() {
    let (out, s_p, e_p) = run(&["-a", "sha1", "-p"], b"");
    assert!(
        !e_p.contains("requires `-a algorithm'"),
        "选中之后 -p 不该再要求 -a：stderr 是 {e_p:?}"
    );
    assert_eq!(
        (out.as_str(), s_p, e_p.as_str()),
        ("da39a3ee5e6b4b0d3255bfef95601890afd80709\n", 0, ""),
        "`-p` 放行之后走的是标准输入那一腿"
    );
    let (_, s_s, e_s) = run(&["-a", "sha1", "-s", "abc"], b"");
    assert!(
        !e_s.contains("requires `-a algorithm'"),
        "选中之后 -s 同理：stderr 是 {e_s:?}"
    );
    assert_eq!(
        (s_s, e_s.as_str()),
        (2, "cksum: -a crypto algorithms are not modelled yet\n"),
        "`-s` 的值被吃掉之后仍走到占位句"
    );
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

/// 三族各自端到端：文件操作数走 C 的三种报文形（缺省 `NAME (档) = 摘要`、`-n` 把摘要
/// 提到前面并接空格与档名、`-q` 只剩摘要），`-q` 与 `-n` 同时给时按
/// `cksum.c:515-520` 的次序 ** quiet 赢**。摘要的期望值出自本仓 C 原件编出的参照件
/// （配方见 `rewrite-notes/18-stage-commands/todo.md` 批次三百八十四），内容是同一份
/// `a\na\nb\n`，三族各一行，一枚算法对一次形状表。
#[test]
fn md5_sha1_sha256_print_c_shapes_for_a_file_operand() {
    let file = scratch("digest-body");
    fs::write(&file, "a\na\nb\n").unwrap();
    let name = file.to_str().unwrap().to_owned();

    for (word, hashname, digest) in [
        ("md5", "MD5", "1ffe2b0635d3964e20f149a90572adfb"),
        ("sha1", "SHA1", "69a938899ace490903de60f8c65079bff7d00854"),
        (
            "sha256",
            "SHA256",
            "82fc121e516876d99b5667deecf01602f9cbeb62ea0797b1efa782e5c9826a6d",
        ),
    ] {
        let (out, status, err) = run(&["-a", word, &name], b"");
        assert_eq!(
            (status, err.as_str()),
            (0, ""),
            "-a {word} 的 stderr 是 {err:?}"
        );
        assert_eq!(out, format!("{hashname} ({name}) = {digest}\n"), "缺省形");
        let (out, status, _) = run(&["-a", word, "-n", &name], b"");
        assert_eq!(status, 0);
        assert_eq!(out, format!("{digest} {name}\n"), "-n 形");
        let (out, status, _) = run(&["-a", word, "-q", &name], b"");
        assert_eq!(status, 0);
        assert_eq!(out, format!("{digest}\n"), "-q 形");
        let (out, _, _) = run(&["-a", word, "-nq", &name], b"");
        assert_eq!(out, format!("{digest}\n"), "-q 与 -n 同给时 quiet 赢");
    }

    let _ = fs::remove_file(&file);
}

/// 第二片新接的三族走同一张形状表：`MD4`、`SHA384`、`SHA512`。摘要值同样出自参照件，
/// 内容与上一枚门用的是同一份 `a\na\nb\n`，所以两枚门横向还能互校（同一份输入、六串
/// 摘要、两套形状）。
#[test]
fn md4_sha384_sha512_print_c_shapes_for_a_file_operand() {
    let file = scratch("digest-wide");
    fs::write(&file, "a\na\nb\n").unwrap();
    let name = file.to_str().unwrap().to_owned();

    for (word, hashname, digest) in [
        ("md4", "MD4", "26dadbd6300e9593a7f8e7d6610de0c5"),
        (
            "sha384",
            "SHA384",
            "86e588c45b29eb502b79da83ba48fb82bc3592935a82ec29b201e7bbfa6739af4f0ea4a7dd1a3c2b0a49f2eb42f2515c",
        ),
        (
            "sha512",
            "SHA512",
            "1b856a944be0f8196449e8250b6525ec1c52aa5e08803a31cb165388846e2197c49b0a261b8f5ee9b3d7a6369c9f034ec843e93275521889ff335adfeb917dad",
        ),
    ] {
        let (out, status, err) = run(&["-a", word, &name], b"");
        assert_eq!((status, err.as_str()), (0, ""), "-a {word} 的 stderr 是 {err:?}");
        assert_eq!(out, format!("{hashname} ({name}) = {digest}\n"), "缺省形");
        let (out, status, _) = run(&["-a", word, "-n", &name], b"");
        assert_eq!(status, 0);
        assert_eq!(out, format!("{digest} {name}\n"), "-n 形");
        let (out, _, _) = run(&["-a", word, "-q", &name], b"");
        assert_eq!(out, format!("{digest}\n"), "-q 形");
    }

    let _ = fs::remove_file(&file);
}

/// 读不到的操作数：C 的 `filefunc` 交回 `NULL` 时不印摘要行，`main` 报
/// `warn("%s", fn)` 并把 `rval` 置 1（`cksum.c:477-483`）。好档仍然各得一行，且**按
/// 操作数顺序**——把坏档夹在两枚好档中间，两行摘要的次序不能跟着坏档走。
#[test]
fn an_unreadable_operand_prints_no_digest_line_and_makes_the_run_fail() {
    let first = scratch("first");
    let second = scratch("second");
    fs::write(&first, "a\na\nb\n").unwrap();
    fs::write(&second, "xyz\n").unwrap();
    let first_name = first.to_str().unwrap().to_owned();
    let second_name = second.to_str().unwrap().to_owned();
    let missing = scratch("absent").to_str().unwrap().to_owned();

    let (out, status, err) = run(
        &["-a", "md5", "-q", &first_name, &missing, &second_name],
        b"",
    );
    assert_eq!(
        out, "1ffe2b0635d3964e20f149a90572adfb\nb6273b589df2dfdbd8fe35b1011e3183\n",
        "坏档占一行位置但不产出行，好档按 argv 次序"
    );
    assert_eq!(err, format!("cksum: {missing}\n"), "报的是那枚操作数本身");
    assert_eq!(status, 1, "任一操作数读不到就整体退 1");

    for path in [&first, &second] {
        let _ = fs::remove_file(path);
    }
}

/// `-p` 的转抄在摘要**之前**，而且转抄的是原样字节（C 的 `MD5Filter` 边读边 `fwrite`，
/// 末尾才 `printf("%s\n", …End(…))`，`minix3/usr.bin/cksum/md5.c` 的 `MD5Filter`）。
/// 输入不带换行时，那一行摘要就接在转抄的最后一个字节后面——这枚门钉的就是这个不分
/// 行、不补换行的形状。
#[test]
fn the_passthrough_copies_the_input_verbatim_before_the_digest() {
    let (out, status, err) = run(&["-a", "md5", "-p"], b"xyzzy");
    assert_eq!(
        out,
        format!("xyzzy{}\n", "1271ed5ef305aadabc605b1609e24c52"),
        "转抄加摘要，中间不插换行"
    );
    assert_eq!((status, err.as_str()), (0, ""));

    // 不给 `-p` 就没有转抄，只有那一行摘要（同一份输入）。
    let (out, _, _) = run(&["-a", "md5"], b"xyzzy");
    assert_eq!(out, "1271ed5ef305aadabc605b1609e24c52\n");
}

/// 占位句的剩余射程要说得出名目：本体未接的两枚算法（这里试 `md2`）、`-s`/`-t`/`-x`
/// 三条腿（试 `-t`）、以及 hash 在里的 `-c` 校验腿（C 用它重算比对，`cksum.c:421-441`）。
/// 三形都是同一句、同一个退码 2；已接的三枚不给这些旗标时**不得**再走这一句。
#[test]
fn the_placeholder_now_covers_only_the_three_remaining_legs() {
    let file = scratch("placeholder");
    fs::write(&file, "a\na\nb\n").unwrap();
    let name = file.to_str().unwrap().to_owned();

    for args in [
        vec!["-a", "md2", name.as_str()],
        vec!["-a", "md5", "-t"],
        vec!["-a", "md5", "-c", name.as_str()],
    ] {
        let (out, status, err) = run(&args, b"");
        assert_eq!(out, "", "{args:?} 不该产出");
        assert_eq!(
            (status, err.as_str()),
            (2, "cksum: -a crypto algorithms are not modelled yet\n"),
            "{args:?} 的 stderr 是 {err:?}"
        );
    }

    let _ = fs::remove_file(&file);
}
