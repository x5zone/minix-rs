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
    // `-s` 放行之后自己那一行就印完了，并且 `nohashstdin` 让标准输入那一腿整个跳过
    //（参照件 s-10 形：`-a sha1 -p -s abc` 只有一行串摘要，既不转抄也不摘要标准输入）。
    let (out, s_s, e_s) = run(&["-a", "sha1", "-s", "abc"], b"xyzzy");
    assert!(
        !e_s.contains("requires `-a algorithm'"),
        "选中之后 -s 同理：stderr 是 {e_s:?}"
    );
    assert_eq!(
        (out.as_str(), s_s, e_s.as_str()),
        (
            "SHA1 (\"abc\") = a9993e364706816aba3e25717850c26c9cd0d89d\n",
            0,
            ""
        ),
        "`-s` 印的是串摘要那一行，标准输入的 `xyzzy` 不再被摘要"
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
        assert_eq!(
            (status, err.as_str()),
            (0, ""),
            "-a {word} 的 stderr 是 {err:?}"
        );
        assert_eq!(out, format!("{hashname} ({name}) = {digest}\n"), "缺省形");
        let (out, status, _) = run(&["-a", word, "-n", &name], b"");
        assert_eq!(status, 0);
        assert_eq!(out, format!("{digest} {name}\n"), "-n 形");
        let (out, _, _) = run(&["-a", word, "-q", &name], b"");
        assert_eq!(out, format!("{digest}\n"), "-q 形");
    }

    let _ = fs::remove_file(&file);
}

/// 最后两族走同一张形状表：`MD2` 与 `RMD160`。到这一枚门为止，C 的八枚算法都能从命令行
/// 产出真摘要，`cksum -a <八枚任一> <档>` 三形各有一处端到端依据。
#[test]
fn md2_and_rmd160_print_c_shapes_for_a_file_operand() {
    let file = scratch("digest-tail");
    fs::write(&file, "a\na\nb\n").unwrap();
    let name = file.to_str().unwrap().to_owned();

    for (word, hashname, digest) in [
        ("md2", "MD2", "f3fd5f9e3538568d40ab75d46a6a3d01"),
        (
            "rmd160",
            "RMD160",
            "d51324ce5bd89642721486d3c81f0bda4a94f489",
        ),
    ] {
        let (out, status, err) = run(&["-a", word, &name], b"");
        assert_eq!(
            (status, err.as_str()),
            (0, ""),
            "-a {word} 的 stderr 是 {err:?}"
        );
        assert_eq!(out, format!("{hashname} ({name}) = {digest}\n"), "缺省形");
        let (out, _, _) = run(&["-a", word, "-n", &name], b"");
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

/// `-s` 那一行的形：`NAME ("<串>") = <摘要>`，串**原样回显**（带空格、空串都不加任何处理），
/// 而且它不受 `-n`/`-q`/`-p` 影响——C 的 `MD5String` 只有一句写死的 `printf`
///（`minix3/usr.bin/cksum/md5.c:65-66`），`print_flags` 只在 `hash_digest_file`
///（`cksum.c:506-524`）那条腿里被读。期望值取自本仓 C 原件编出的参照件
///（`tmp/cksum-hash-state/readings.log` 的 s-1…s-14 各形，配方 `bash tmp/cksum-hash-state/shapes.sh`）。
#[test]
fn dash_s_prints_the_string_digest_line_verbatim() {
    for (value, expected) in [
        ("zzz", "f3abb86bd34cf4d52698f14c0da1dc60"),
        ("hello world", "5eb63bbbe01eeed093cb22bb8f5acdc3"),
        ("", "d41d8cd98f00b204e9800998ecf8427e"),
    ] {
        let (out, status, err) = run(&["-a", "md5", "-s", value], b"");
        assert_eq!(
            (out.as_str(), status, err.as_str()),
            (format!("MD5 (\"{value}\") = {expected}\n").as_str(), 0, ""),
            "-s {value:?}"
        );
    }

    // 取值的两条路都要走到同一形：附值（`-sabc`）与聚簇带值（`-sa`，参照件 s-5/s-6 两形）。
    for (flag, value, digest) in [
        ("-sabc", "abc", "900150983cd24fb0d6963f7d28e17f72"),
        ("-sa", "a", "0cc175b9c0f1b6a831c399e269772661"),
    ] {
        let (out, status, err) = run(&["-a", "md5", flag], b"");
        assert_eq!(
            (out.as_str(), status, err.as_str()),
            (format!("MD5 (\"{value}\") = {digest}\n").as_str(), 0, ""),
            "{flag} 的取值路径"
        );
    }

    // `-n`/`-q`/`-p` 都不改这一行（s-10/s-11/s-12 三形同值），换算法就换名字与摘要（s-13/s-14）。
    for prefix in [vec!["-n"], vec!["-q"], vec!["-p"]] {
        let mut argv = vec!["-a", "md5"];
        argv.extend(prefix.iter().copied());
        argv.extend(["-s", "abc"]);
        let (out, status, err) = run(&argv, b"");
        assert_eq!(
            (out.as_str(), status, err.as_str()),
            ("MD5 (\"abc\") = 900150983cd24fb0d6963f7d28e17f72\n", 0, ""),
            "{argv:?} 里那三个旗标都不该改动串摘要那一行"
        );
    }

    let (out, _, _) = run(&["-a", "sha512", "-s", "abc"], b"");
    assert_eq!(
        out,
        "SHA512 (\"abc\") = ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f\n",
        "算法名是大写那一份，摘要跟着算法换长度"
    );
    let (out, _, _) = run(&["-a", "rmd160", "-s", "abc"], b"");
    assert_eq!(
        out,
        "RMD160 (\"abc\") = 8eb208f7e05d987a9b044a8e98c6b087f15a0bfc\n"
    );
}

/// `-x` 打的八枚串是 C 的 `MD5TestSuite` 写死的那一份（`md5.c:118-133`），八枚算法共用
/// 同一串文字（`md2.c`/`md4.c`/`rmd160.c`/`sha1.c`/`sha2*.c` 都只 `#include "md5.c"`），
/// 所以这里是**整块九行**逐字对参照件（x-1 形）。最后一枚在 C 里是反斜杠续行拼出来的
/// 80 字符（`:143-144`），抄错长度就立刻在这一行分叉。
#[test]
fn dash_x_prints_the_eight_reference_strings() {
    let expected = concat!(
        "MD5 test suite:\n",
        "MD5 (\"\") = d41d8cd98f00b204e9800998ecf8427e\n",
        "MD5 (\"a\") = 0cc175b9c0f1b6a831c399e269772661\n",
        "MD5 (\"abc\") = 900150983cd24fb0d6963f7d28e17f72\n",
        "MD5 (\"message digest\") = f96b697d7cb7938d525a2f31aaf161d0\n",
        "MD5 (\"abcdefghijklmnopqrstuvwxyz\") = c3fcd3d76192e4007dfb496cca67e13b\n",
        "MD5 (\"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq\") = 8215ef0796a20bcaaae116d3876c664a\n",
        "MD5 (\"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789\") = d174ab98d277d9f5a5611c2c9f419d9f\n",
        "MD5 (\"12345678901234567890123456789012345678901234567890123456789012345678901234567890\") = 57edf4a22be3c955ac49da2e2107b67a\n",
    );
    let (out, status, err) = run(&["-a", "md5", "-x"], b"");
    assert_eq!((status, err.as_str()), (0, ""), "stderr 是 {err:?}");
    assert_eq!(out, expected, "九行整块对参照件 x-1");

    // 出现两次就印两套（x-5 形： getopt 环每命中一次调一次 `testsuitefunc`）。
    let (out, _, _) = run(&["-a", "md5", "-x", "-x"], b"");
    assert_eq!(out, format!("{expected}{expected}"), "重复出现不合并");

    // 同一串文字配 SHA512 的 128 位十六进制（x-2 形里取头尾两枚，长度不对就当场分叉）。
    let (out, _, _) = run(&["-a", "sha512", "-x"], b"");
    assert_eq!(out.lines().count(), 9, "八枚串加一行标题");
    assert_eq!(
        out.lines().next().unwrap(),
        "SHA512 test suite:",
        "标题跟着算法名走"
    );
    assert_eq!(
        out.lines().nth(7).unwrap(),
        "SHA512 (\"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789\") = 1e07be23c26a86ea37ea810c8ec7809352515a970e9253c26f536cfc7a9996c45c8370583e0a78fa4a90041d71a4ceab7423f19c71b9d5a3e01249f0bebd5894",
        "第 0 行是标题，八枚串占 1..=8；这一枚是第七串"
    );
    assert_eq!(
        out.lines().nth(8).unwrap(),
        "SHA512 (\"12345678901234567890123456789012345678901234567890123456789012345678901234567890\") = 72ec1ef1124a45b047e8b7c75a932195135bb61de24ec0d1914042246e0aec3a2354e093d76f3048b456764346900cb130d2a4fd5dd16abb5e30bcb850dee843",
        "最后一枚是 C 那条反斜杠续行拼出的 80 字符，抄短就分叉"
    );
}

/// `-t` 的四行里前两行是写死的：`%s time trial.  Digesting %d %d-byte blocks ...` 加
/// 紧随的 `" done\n"`（`md5.c:81-83`、`:101`），所以第一行末尾是 `... done`，中间**两个**
/// 空格在 `trial.` 之后。第二行的摘要由那 1000 × 1000 字节决定（t-1 形）；
/// 后两行是墙钟，只能钉住形——`Time = <n> seconds` 与 `Speed = <n> bytes/second`。
#[test]
fn dash_t_prints_four_lines_with_the_wall_clock_pair() {
    let (out, status, err) = run(&["-a", "md5", "-t"], b"");
    assert_eq!((status, err.as_str()), (0, ""), "stderr 是 {err:?}");
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(
        &lines[..2],
        [
            "MD5 time trial.  Digesting 1000 1000-byte blocks ... done",
            "Digest = f217fb0b8599c956eaeb81611e7a8758",
        ],
        "前两行逐字对参照件 t-1（`trial.` 之后两个空格，行尾是 `... done`）"
    );
    // 后两行是墙钟，钉不得值；钉的是 C 那句算术：`Speed = 1000*1000 / (delta != 0 ? delta : 1)`
    //（`md5.c:109-111`）。一秒内跑完时读数正是参照件那一形 `Time = 0` 与 `Speed = 1000000`。
    let number = |line: &str, prefix: &str, suffix: &str| -> i64 {
        line.strip_prefix(prefix)
            .and_then(|rest| rest.strip_suffix(suffix))
            .unwrap_or_else(|| panic!("那一行的形是 {prefix}<n>{suffix}：{line:?}"))
            .parse()
            .expect("数字该是十进制整数")
    };
    let elapsed = number(lines[2], "Time = ", " seconds");
    let speed = number(lines[3], "Speed = ", " bytes/second");
    assert_eq!(
        speed,
        1_000_000 / elapsed.max(1),
        "除数按 C 取 `delta != 0 ? delta : 1`"
    );
    if elapsed == 0 {
        // 一秒内跑完（本机与容器都是这一形，参照件 t-1 亦此形）时除数必须取 1：
        // 这一枚分支不钉住的话，「把除数写死成 1」这种注入在本机就看不出来。
        assert_eq!(speed, 1_000_000, "Time = 0 那一支的 Speed 读数");
    }

    // 换算法只换名字与摘要，四行的骨架不动。
    let (out, _, _) = run(&["-a", "sha1", "-t"], b"");
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 4, "四行：{lines:?}");
    assert_eq!(
        lines[0],
        "SHA1 time trial.  Digesting 1000 1000-byte blocks ... done"
    );
    assert!(lines[1].starts_with("Digest = "), "{}", lines[1]);
    assert!(lines[2].starts_with("Time = ") && lines[2].ends_with(" seconds"));
    assert!(lines[3].starts_with("Speed = ") && lines[3].ends_with(" bytes/second"));
}

/// 三条自印腿的**次序**：它们在 C 的 getopt 环里就跑完了，所以在操作数之前、也在
/// 报错那一条诊断之前。参照件 s-7/x-6/t-3 三形都是「腿的行在前，档名报错在后」，
/// e-1/e-2/e-3 三形都是「stdout 已有那一行，stderr 是诊断加 usage，退码 1」。
#[test]
fn the_self_printing_legs_come_before_operands_and_before_errors() {
    // 同现时按 argv 次序：`-x -s abc` 是套件九行在前、串摘要一行在后（s-9 形）。
    let (out, _, _) = run(&["-a", "md5", "-x", "-s", "abc"], b"");
    assert!(out.starts_with("MD5 test suite:\n"), "{out:?}");
    assert!(
        out.ends_with("MD5 (\"abc\") = 900150983cd24fb0d6963f7d28e17f72\n"),
        "套件之后才是 -s 那一行"
    );

    // 有操作数时腿在前、档在后；档读不到照旧是那句 `warn`（s-7 形）。
    let missing = scratch("legs-missing");
    let (out, status, err) = run(&["-a", "md5", "-s", "abc", missing.to_str().unwrap()], b"");
    assert_eq!(
        out, "MD5 (\"abc\") = 900150983cd24fb0d6963f7d28e17f72\n",
        "腿的行已经在 stdout 上"
    );
    assert_eq!(status, 1, "读不到的操作数把整趟压成败");
    assert!(
        err.starts_with(&format!("cksum: {}", missing.to_str().unwrap())),
        "stderr 是 {err:?}"
    );

    // 报错追不回来：三形的 stdout 都有那一行，退码都是 1。
    for (tail, diagnostic) in [
        (vec!["-Z"], format!("cksum: unknown option -- Z\n{USAGE}")),
        (
            vec!["-o", "1"],
            format!("cksum: MD5 mutually exclusive with sum\n{USAGE}"),
        ),
        (
            vec!["-a", "sha1"],
            format!("cksum: illegal use of -a option\n{USAGE}"),
        ),
    ] {
        let mut argv = vec!["-a", "md5", "-s", "abc"];
        argv.extend(tail.iter().copied());
        let (out, status, err) = run(&argv, b"");
        assert_eq!(
            out, "MD5 (\"abc\") = 900150983cd24fb0d6963f7d28e17f72\n",
            "{argv:?} 报错之前那一行已经印出去了"
        );
        assert_eq!((status, err.as_str()), (1, diagnostic.as_str()), "{argv:?}");
    }

    // 反向对照：`-s` 自己没过 `requirehash` 时什么都没有（e-4 形：stdout 空）。
    let (out, status, err) = run(&["-s", "abc"], b"");
    assert_eq!(out, "", "没选中算法就不该有串摘要行");
    assert_eq!(
        (status, err.as_str()),
        (
            1,
            format!("cksum: -s flag requires `-a algorithm'\n{USAGE}").as_str()
        )
    );
}

/// 占位句已经全部退场：八枚算法本体、`-s`/`-t`/`-x` 三条自印腿与 hash 在里的 `-c` 校验腿
/// 都按参照件读数接成真相，所以 `cksum: -a crypto algorithms are not modelled yet`（退 2）
/// 这一句在本壳里已经不存在——这枚门钉的是「没有任何一形再落回那句占位」。
/// 判据出处：`cksum.c` 的三条自印腿 `:255-275`、`do_check` 的摘要腿 `:424-426` 与 `:454`。
#[test]
fn no_hash_shape_lands_on_the_retired_placeholder() {
    let file = scratch("placeholder-none");
    fs::write(&file, "hello\n").unwrap();
    let name = file.to_str().unwrap().to_owned();
    let placeholder = "cksum: -a crypto algorithms are not modelled yet\n";

    for args in [
        vec!["-a", "md5", "-c", name.as_str()],
        vec!["-a", "md5", "-t"],
        vec!["-a", "md5", "-x"],
        vec!["-a", "md5", "-s", "abc"],
        vec!["-a", "sha512", "-c", name.as_str()],
        vec!["-a", "rmd160", "-x"],
    ] {
        let (_, status, err) = run(&args, b"");
        assert_ne!(status, 2, "{args:?} 不该再退 2：stderr 是 {err:?}");
        assert_ne!(err, placeholder, "{args:?} 不该再落占位句");
    }

    let _ = fs::remove_file(&file);
}

/// `cksum -c` 遇上 `NAME (档) = 摘要` 这一形：C 用 `hash->filefunc` 把档重算一遍再逐字比
///（`cksum.c:424-426`），**全对就一声不出**（参照件 c-1），而且这一形不需要 `-a`——
/// 算法名是从行首前缀认出来的（`:343-352`，参照件 c-7 形同值）。摘要期望值取自参照件
/// f-4 形（同一份内容 `hello\n`）。
#[test]
fn a_matching_digest_line_says_nothing_and_exits_zero() {
    let file = scratch("check-good");
    fs::write(&file, "hello\n").unwrap();
    let name = file.to_str().unwrap().to_owned();
    let sums = scratch("check-good-sums");
    fs::write(
        &sums,
        format!("MD5 ({name}) = b1946ac92492d2347c6235b4d2611184\n"),
    )
    .unwrap();
    let sums_name = sums.to_str().unwrap().to_owned();

    for args in [
        vec!["-a", "md5", "-c", sums_name.as_str()],
        vec!["-c", sums_name.as_str()],
    ] {
        let (out, status, err) = run(&args, b"");
        assert_eq!(
            (out.as_str(), status, err.as_str()),
            ("", 0, ""),
            "{args:?} 全对的档该是无声退 0"
        );
    }

    // 换算法就换摘要：同一份字节配 SHA256 的 64 位十六进制（参照件 f-4 同族读数）。
    let sha = scratch("check-good-sha");
    fs::write(
        &sha,
        format!(
            "SHA256 ({name}) = 5891b5b522d5df086d0ff0b110fbd9d21bb4fc7163af34d08286a2e846f6be03\n"
        ),
    )
    .unwrap();
    let (out, status, err) = run(&["-c", sha.to_str().unwrap()], b"");
    assert_eq!(
        (out.as_str(), status, err.as_str()),
        ("", 0, ""),
        "不带 `-a` 也从行首前缀认出了 SHA256：stdout {out:?} stderr {err:?}"
    );

    let _ = fs::remove_file(&file);
    let _ = fs::remove_file(&sums);
    let _ = fs::remove_file(&sha);
}

/// 摘要不对的那一行：C 打 `(MD5) <档>: FAILED`（`:454` 的 `printf("(%s) ", hash->hashname)`
/// 再接 `:455` 那句），整趟退 1（`badcnt > 0 ⇒ rval = 1`，`:463-464`）。前缀里的名字跟着
/// **行首那一枚**，不跟着 `-a`：参照件 g-11 形是 `-a md5` 配 `SHA1 (...)` 一行，报的是 `(SHA1) `。
#[test]
fn a_mismatched_digest_line_fails_with_the_line_own_hash_name() {
    let file = scratch("check-bad");
    fs::write(&file, "hello\n").unwrap();
    let name = file.to_str().unwrap().to_owned();
    let sums = scratch("check-bad-sums");
    fs::write(
        &sums,
        format!("MD5 ({name}) = 00000000000000000000000000000000\n"),
    )
    .unwrap();
    let sums_name = sums.to_str().unwrap().to_owned();

    let (out, status, err) = run(&["-a", "md5", "-c", sums_name.as_str()], b"");
    assert_eq!(out, format!("(MD5) {name}: FAILED\n"), "stderr 是 {err:?}");
    assert_eq!((status, err.as_str()), (1, ""), "摘要不对要退 1");

    // 行首名字压过 `-a`：这一行按 SHA1 重算，所以报的也是 `(SHA1) `（参照件 g-11/g-12 两形）。
    let other = scratch("check-other-sums");
    fs::write(
        &other,
        format!("SHA1 ({name}) = b1946ac92492d2347c6235b4d2611184\n"),
    )
    .unwrap();
    let (out, status, _) = run(&["-a", "md5", "-c", other.to_str().unwrap()], b"");
    assert_eq!(out, format!("(SHA1) {name}: FAILED\n"));
    assert_eq!(status, 1);

    let _ = fs::remove_file(&file);
    let _ = fs::remove_file(&sums);
    let _ = fs::remove_file(&other);
}

/// 档读不到时的两条路**分叉**：`hash` 在里那一支走 `access(filename, R_OK)`（`:424`），
/// 失败只把这一行判红、一句 warn 都没有；crc 那一支才 warn（`:432`，且只在 `-w` 下）。
/// 参照件 c-5 与 h-13 两形（后者带 `-w`）实测都是只有 `(MD5) <档>: FAILED` 一行。
#[test]
fn an_unreadable_operand_in_a_digest_line_fails_without_warning() {
    let missing = scratch("check-missing-target");
    let sums = scratch("check-missing-sums");
    fs::write(
        &sums,
        format!(
            "MD5 ({}) = 900150983cd24fb0d6963f7d28e17f72\n",
            missing.to_str().unwrap()
        ),
    )
    .unwrap();
    let sums_name = sums.to_str().unwrap().to_owned();

    for flag in [None, Some("-w")] {
        let mut args = vec!["-a", "md5", "-c"];
        if let Some(w) = flag {
            args.push(w);
        }
        args.push(sums_name.as_str());
        let (out, status, err) = run(&args, b"");
        assert_eq!(
            out,
            format!("(MD5) {}: FAILED\n", missing.to_str().unwrap()),
            "{args:?} 的 stdout"
        );
        assert_eq!(
            (status, err.as_str()),
            (1, ""),
            "{args:?}：hash 那一支读不到档也不 warn（crc 那一支才 warn）"
        );
    }

    let _ = fs::remove_file(&sums);
}

/// 认不出的算法名：C 找完 `hashes[]` 没命中就 `rval = 1` 并 `continue`（`:356-366`），
/// **不打 FAILED 行**；那句 `unknown hash: <整行>` 只在 `-w` 下打（参照件 c-3 与 c-4 两形：
/// 前者 stdout/stderr 全空，后者 stderr 收到整行原文）。
#[test]
fn an_unknown_hash_prefix_fails_the_run_and_only_names_the_line_under_w() {
    let sums = scratch("check-unknown-sums");
    fs::write(&sums, "FOO (/nope) = 900150983cd24fb0d6963f7d28e17f72\n").unwrap();
    let name = sums.to_str().unwrap().to_owned();

    let (out, status, err) = run(&["-a", "md5", "-c", name.as_str()], b"");
    assert_eq!(
        (out.as_str(), status, err.as_str()),
        ("", 1, ""),
        "不带 -w 时这一行只把整趟压成败"
    );
    let (out, status, err) = run(&["-a", "md5", "-w", "-c", name.as_str()], b"");
    assert_eq!(out, "", "不该有 FAILED 行");
    assert_eq!(
        (status, err.as_str()),
        (
            1,
            "cksum: unknown hash: FOO (/nope) = 900150983cd24fb0d6963f7d28e17f72\n"
        ),
        "句子带的是整行原文"
    );

    // 大小写敏感：`md5` 这种小写行首同样认不出（参照件 g-10 形），与 `-a` 那一腿正好相反。
    let lower = scratch("check-lower-sums");
    fs::write(&lower, "md5 (/nope) = d41d8cd98f00b204e9800998ecf8427e\n").unwrap();
    let (_, status, err) = run(&["-w", "-c", lower.to_str().unwrap()], b"");
    assert_eq!(
        (status, err.as_str()),
        (
            1,
            "cksum: unknown hash: md5 (/nope) = d41d8cd98f00b204e9800998ecf8427e\n"
        )
    );

    let _ = fs::remove_file(&sums);
    let _ = fs::remove_file(&lower);
}

/// `hash` 状态**跨行留存**：C 在认出一行摘要时把 `hash` 指过去（`hash = nhash`，`:363`），
/// 于是同一档里后面那些没有 `(` 的普通行也走摘要那一支（`:373-389`），第一个词整串被拿去和
/// 重算的摘要比——参照件 c-6/h-1/h-4 三形实测：普通行一律判红并带 `(MD5) ` 前缀，
/// 而且 h-4 **没给 `-a`**（算法是第一行自己认的）。
#[test]
fn a_digest_line_switches_the_rest_of_the_file_to_digest_compare() {
    let good = scratch("mixed-good");
    fs::write(&good, "hello\n").unwrap();
    let good_name = good.to_str().unwrap().to_owned();
    let sums = scratch("mixed-sums");
    fs::write(
        &sums,
        format!("MD5 ({good_name}) = b1946ac92492d2347c6235b4d2611184\n12345 {good_name}\n"),
    )
    .unwrap();
    let sums_name = sums.to_str().unwrap().to_owned();

    for args in [
        vec!["-a", "md5", "-c", sums_name.as_str()],
        vec!["-c", sums_name.as_str()],
    ] {
        let (out, status, _) = run(&args, b"");
        assert_eq!(
            out,
            format!("(MD5) {good_name}: FAILED\n"),
            "{args:?}：只有第二行判红"
        );
        assert_eq!(status, 1, "{args:?}");
    }

    // 反过来的次序（普通行在前）也一样：那时 `hash` 还没被认定，走的是 crc 那一支，
    // 报的是不带前缀的 `<档>: FAILED`（参照件 c-8 形）。
    let reversed = scratch("mixed-reversed");
    fs::write(&reversed, format!("12345 {good_name}\n")).unwrap();
    let (out, status, _) = run(&["-c", reversed.to_str().unwrap()], b"");
    assert_eq!(out, format!("{good_name}: FAILED\n"), "不带前缀");
    assert_eq!(status, 1);

    let _ = fs::remove_file(&good);
    let _ = fs::remove_file(&sums);
    let _ = fs::remove_file(&reversed);
}

/// 两句格式诊断按状态分叉，`-w` 下的句子不同：没有 `(` 又没有空格时，
/// `hash` 未选中报 `bogus format: <行>. Skipping...`（`:395-401`，参照件 h-9 形），
/// 已选中报 `no filename in <行>? Skipping...`（`:377-383`）。有 `(` 缺 `)` 用的还是前一句
///（`:322-328`）。三形都记一次失败。
#[test]
fn the_two_format_sentences_follow_the_hash_state() {
    let no_space = scratch("sentence-nospace");
    fs::write(&no_space, "12345\n").unwrap();
    let (out, status, err) = run(&["-w", "-c", no_space.to_str().unwrap()], b"");
    assert_eq!(
        (out.as_str(), status, err.as_str()),
        ("", 1, "cksum: bogus format: 12345. Skipping...\n"),
        "hash 未选中那支的句子"
    );

    // 同一行，但让 `hash` 先被前面一行认定 → 报的是另一句。
    let good = scratch("sentence-state");
    fs::write(&good, "hello\n").unwrap();
    let good_name = good.to_str().unwrap().to_owned();
    let switched = scratch("sentence-switched");
    fs::write(
        &switched,
        format!("MD5 ({good_name}) = b1946ac92492d2347c6235b4d2611184\n12345\n"),
    )
    .unwrap();
    let (out, status, err) = run(&["-w", "-c", switched.to_str().unwrap()], b"");
    assert_eq!(out, "", "这一行不产 FAILED 行");
    assert_eq!(
        (status, err.as_str()),
        (1, "cksum: no filename in 12345? Skipping...\n"),
        "hash 已选中那支的另一句"
    );

    // 有 `(` 缺 `)`：还是 `bogus format` 那一句（`:322-328`）。
    let unclosed = scratch("sentence-unclosed");
    fs::write(&unclosed, "MD5 (/nope = abc\n").unwrap();
    let (_, status, err) = run(&["-w", "-c", unclosed.to_str().unwrap()], b"");
    assert_eq!(
        (status, err.as_str()),
        (1, "cksum: bogus format: MD5 (/nope = abc. Skipping...\n")
    );

    let _ = fs::remove_file(&no_space);
    let _ = fs::remove_file(&good);
    let _ = fs::remove_file(&switched);
    let _ = fs::remove_file(&unclosed);
}
