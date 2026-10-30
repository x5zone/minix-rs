//! End-to-end face for the `cksum` binary: both the option diagnostics and the
//! `-a` digest legs, checked against `minix3/usr.bin/cksum/cksum.c` (option loop
//! `:193-278`, digest legs `:474-491`, `hash_digest_file` `:506-524`,
//! `requirehash` `:521-525`, `usage()` `:535-550`) and the libc `getopt` it
//! drives (`minix3/lib/libc/stdlib/getopt.c:101-104`, `:132-136`).
//!
//! The three shapes are independent; the old implementation collapsed them into
//! one:
//! (1) what getopt itself reports (out-of-set letter, value-taking option with a
//!     missing value) is "one libc line + the nine-line usage";
//! (2) the command's own `warnx` (`illegal argument to -a|-o option`, `<flag> flag
//!     requires `-a algorithm'`) is also "one line + the same usage block", but
//!     with **a different sentence**;
//! (3) the program name inside the usage block is a **literal** in C
//!     (`cksum`/`sum`/`md2`...), not taken from `argv[0]`, while the line in (1)
//!     follows `getprogname()`.
//!
//! The digest family has four other message shapes (three operand shapes plus
//! standard input); their expected values are not computed for their own sake:
//! they come from the reference-binary readings noted at the end of this page,
//! and the reference binary is built from this repo's C originals.

mod face_support;

use face_support::write_stdin;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};

const CKSUM: &str = env!("CARGO_BIN_EXE_cksum");

/// The digest legs really open files and read bytes, so they need scratch files;
/// `cksum`'s messages carry the operand text verbatim, so the path must be
/// process-exclusive (two concurrent test processes on one machine must not
/// collide on a name).
fn scratch(name: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!("cksum-face-{}-{name}", std::process::id()));
    path
}

/// The nine verbatim lines of C `usage()` (`cksum.c:535-549`).
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

/// An out-of-set letter: libc's `unknown option` (**not** `illegal option`,
/// which is `pr`'s `egetopt` wording), followed by the whole usage block and
/// exit 1 (C's `usage()` calls `exit(1)`; the old implementation exited 2).
#[test]
fn an_unknown_letter_prints_the_libc_line_then_the_nine_line_usage() {
    let (out, status, err) = run(&["-Z"], b"a");
    assert_eq!(out, "", "报文只走 stderr");
    assert_eq!(status, 1, "C 的 usage() 退 1，stderr 是 {err:?}");
    assert_eq!(err, format!("cksum: unknown option -- Z\n{USAGE}"));
}

/// Same command seam as `run`, but stderr comes back **byte for byte**:
/// `from_utf8_lossy` renders a bare `c3` as the three-byte U+FFFD, so the text
/// form cannot distinguish "one byte" from "two bytes" mistakes.
fn run_bytes(args: &[&str], input: &[u8]) -> (Vec<u8>, i32, Vec<u8>) {
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
        output.stdout,
        output.status.code().unwrap_or(-1),
        output.stderr,
    )
}

/// The letter slot of that out-of-set-letter line is **one bare byte**. The
/// expected form comes from the reference binary
/// `tmp/cksum-hash-state/cksum_ref` (started through the same-named symlink
/// `tmp/cksum-414/cksum`, so the program-name segment matches this wrapper and
/// the whole stderr compares directly; probing `-Z` first established
/// provenance: it returns `unknown option -- Z`, i.e. Minix's `getopt.c`, not
/// glibc's quoted `invalid option -- 'Z'`): `-é` and `-né` both print
/// `cksum: unknown option -- ` + a single `c3` + newline followed by the
/// **nine-line** usage (`cksum.c:535-549`) and exit **1**; `-Zé` stops at `Z`;
/// conversely, in `-aé` the `é` is a **value**, not a letter, so it takes the
/// command's own `illegal argument to -a option` plus usage (the letter slot is
/// the hard-coded `a`, so both render the same). The ASCII forms (`-Z`, `-q`,
/// `--x`, the missing-value sentences) already have owners in this file and are
/// not duplicated here.
#[test]
fn the_libc_line_of_a_multibyte_flag_is_one_byte_for_cksum() {
    let want: Vec<u8> = [
        b"cksum: unknown option -- ".as_slice(),
        b"\xc3\n",
        USAGE.as_bytes(),
    ]
    .concat();
    for form in ["-\u{e9}", "-n\u{e9}"] {
        let (out, status, err) = run_bytes(&[form], b"a");
        assert_eq!(out, Vec::new(), "{form} 的报文只走标准错误");
        assert_eq!(status, 1, "{form} 的 stderr 是 {err:?}");
        assert_eq!(err, want, "{form} 的字母该是一个裸字节 c3");
    }

    let (_out, status, err) = run_bytes(&["-Z\u{e9}"], b"a");
    assert_eq!(status, 1);
    assert_eq!(
        err,
        [b"cksum: unknown option -- Z\n".as_slice(), USAGE.as_bytes(),].concat(),
        "环停在第一个坏字母上，第二个字节根本不该被读到"
    );

    let (_out, status, err) = run_bytes(&["-a\u{e9}"], b"a");
    assert_eq!(status, 1);
    assert_eq!(
        err,
        [
            b"cksum: illegal argument to -a option\n".as_slice(),
            USAGE.as_bytes(),
        ]
        .concat(),
        "带值字母把两字节整个当值收走，句子是命令自己的那一句"
    );
}

/// A value-taking option with a missing value is libc's other sentence, also
/// followed by usage; every value-taking letter must be tried (`-a`, `-o`, `-s`).
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

/// An illegal value comes from the command's own `warnx`, and the sentence names
/// the letter; the difference from (1) is the sentence, not the line count.
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

/// C's `requirehash`: `<flag> flag requires `-a algorithm'` followed by usage.
/// The old implementation invented `requires a hash algorithm`, skipped the
/// usage block, and exited 2.
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

/// The getopt shapes themselves must be externally verifiable: clustering
/// (`-nq`) and inline values (`-o2`) are both legal calls, and the old
/// implementation matched letters against the whole token and rejected them as
/// out-of-set letters.
#[test]
fn clustered_and_inline_letters_are_accepted() {
    let (out, status, err) = run(&["-nq"], b"abc");
    assert_eq!((status, err.as_str()), (0, ""), "stderr 是 {err:?}");
    // The concrete numbers belong to another face (the `src/cksum.rs` library
    // tests pin the crc values); here only "accepted and produces output" is
    // required.
    assert!(!out.is_empty(), "被接受就该有校验和输出");
    let (_, status, err) = run(&["-o2"], b"abc");
    assert_eq!((status, err.as_str()), (0, ""), "stderr 是 {err:?}");
    // A token starting with `-` after `--` is an operand, not a flag (here it is
    // an unreadable file name).
    let (_, status, _) = run(&["--", "-o"], b"a");
    assert_eq!(status, 1, "`--` 后该把 -o 当操作数");
}

/// State-driven diagnostic one: `-a` appears again after a hash has already been
/// selected. C's `case 'a'` first line is exactly this (`cksum.c:195-199`): a
/// two-segment `warnx` (no strerror suffix) followed by the same nine-line usage
/// block, exit 1. This wrapper could not reach the shape before — `-a md5` was
/// stopped during parsing by an invented placeholder sentence, so the second `-a`
/// never got a chance to be seen. Once "selected" became tracked state, this
/// diagnostic follows C's ordering.
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
    // Whether the second name is valid does not matter: this diagnostic checks
    // state first, so `crc` is stopped by it too.
    let (_, _, err2) = run(&["-a", "md5", "-a", "crc"], b"");
    assert_eq!(err2, err, "报的是「-a 用错了」而不是名字问题");
}

/// State-driven diagnostic two: `-o` appears after a hash has been selected. C
/// prints `warnx("%s mutually exclusive with sum", hash->hashname)`
/// (`cksum.c:228-234`), where the name is the **uppercase** entry in the
/// `hashes[]` table (`"MD5"` at `:126`); the case used on the command line does
/// not affect it.
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
    // In the opposite order, `-o` before `-a`, there is no conflict: the state
    // is still empty and C proceeds as well — and once it proceeds, only `hash`
    // is consulted (`if (hash != NULL)` at `cksum.c:478-483` precedes `cfncn`),
    // so `-o 1` is ignored entirely and the standard-input digest is printed.
    // Once the digest body was wired in, this shape got real readings for the
    // first time.
    let (out, status, err) = run(&["-o", "1", "-a", "md5"], b"");
    assert_eq!(
        (out.as_str(), status, err.as_str()),
        ("d41d8cd98f00b204e9800998ecf8427e\n", 0, ""),
        "`-o 1` 之后选中摘要就该压过 `-o`"
    );
}

/// The stretch between selection and the placeholder: flags like `-p`/`-s` that
/// must first pass `requirehash` are **let through** in C once `hash` is
/// selected (the condition at `cksum.c:521-525` does not hold), so they must not
/// report `-p flag requires \`-a algorithm'` anymore; this wrapper used to report
/// it unconditionally because the state was not modeled. After the let-through
/// the two legs diverge: `-p` only touches `pflag` (`cksum.c:250`) and the
/// standard-input leg still runs, so once the digest body was wired in it prints
/// a real digest; `-s` also sets `nohashstdin` (`:258`) and its own line depends
/// on `stringfunc`, which is still on the placeholder sentence.
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
    // Once `-s` is let through, its own line is printed and `nohashstdin` makes
    // the standard-input leg skip entirely (reference form s-10: `-a sha1 -p -s
    // abc` prints only the string digest line, neither copying nor digesting
    // standard input).
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
    // Without a selection it is still the same sentence (control: proves the
    // let-through is decided by state).
    let (_, s0, e0) = run(&["-p"], b"");
    assert_eq!(
        (s0, e0.as_str()),
        (
            1,
            format!("cksum: -p flag requires `-a algorithm'\n{USAGE}").as_str()
        )
    );
}

/// Each of the three families end to end: a file operand follows C's three
/// message shapes (default `NAME (file) = digest`, `-n` putting the digest first
/// followed by a space and the file name, `-q` leaving only the digest), and
/// when `-q` and `-n` are both given, quiet wins per the order at
/// `cksum.c:515-520`. The expected digests come from the reference binary built
/// from this repo's C originals; the content is the same `a\na\nb\n` in every
/// case, one line per family, one algorithm against one shape table.
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

/// The next three newly wired families follow the same shape table: `MD4`,
/// `SHA384`, `SHA512`. The digest values likewise come from the reference
/// binary, over the same `a\na\nb\n`, so the two gates can cross-check each
/// other (same input, six digests, two shape tables).
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

/// The last two families follow the same shape table: `MD2` and `RMD160`. With
/// this gate, all eight C algorithms produce real digests from the command line,
/// and each of the three shapes for `cksum -a <any of the eight> <file>` has
/// end-to-end evidence.
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

/// An unreadable operand: when C's `filefunc` returns `NULL` no digest line is
/// printed; `main` reports `warn("%s", fn)` and sets `rval` to 1
/// (`cksum.c:477-483`). Good files still each get a line, **in operand order** —
/// with the bad file between two good ones, the order of the two digest lines
/// must not follow the bad file.
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
    assert_eq!(
        err,
        format!("cksum: {missing}: No such file or directory\n"),
        "C 那句是 `warn(\"%s\", fn)`（`cksum.c:479`）：程序名、操作数、`strerror` 句子三段齐全"
    );
    assert_eq!(status, 1, "任一操作数读不到就整体退 1");

    for path in [&first, &second] {
        let _ = fs::remove_file(path);
    }
}

/// `-p`'s passthrough happens **before** the digest, and it copies the original
/// bytes (C's `MD5Filter` reads and `fwrite`s as it goes, only calling
/// `printf("%s\n", ...End(...))` at the end; `MD5Filter` in
/// `minix3/usr.bin/cksum/md5.c`). When the input has no trailing newline, the
/// digest line follows the last copied byte directly — this gate pins exactly
/// that shape: no line break inserted, no newline appended.
#[test]
fn the_passthrough_copies_the_input_verbatim_before_the_digest() {
    let (out, status, err) = run(&["-a", "md5", "-p"], b"xyzzy");
    assert_eq!(
        out,
        format!("xyzzy{}\n", "1271ed5ef305aadabc605b1609e24c52"),
        "转抄加摘要，中间不插换行"
    );
    assert_eq!((status, err.as_str()), (0, ""));

    // Without `-p` there is no passthrough, only the digest line (same input).
    let (out, _, _) = run(&["-a", "md5"], b"xyzzy");
    assert_eq!(out, "1271ed5ef305aadabc605b1609e24c52\n");
}

/// The shape of the `-s` line: `NAME ("<string>") = <digest>`, with the string
/// **echoed verbatim** (spaces or the empty string get no special treatment),
/// and it is unaffected by `-n`/`-q`/`-p` — C's `MD5String` has a single
/// hard-coded `printf` (`minix3/usr.bin/cksum/md5.c:65-66`), and `print_flags`
/// is only read on the `hash_digest_file` leg (`cksum.c:506-524`). The expected
/// values come from the reference binary built from this repo's C originals
/// (forms s-1...s-14 in `tmp/cksum-hash-state/readings.log`, recipe
/// `bash tmp/cksum-hash-state/shapes.sh`).
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

    // Both ways of taking the value must reach the same shape: attached value
    // (`-sabc`) and clustered-with-value (`-sa`, reference forms s-5/s-6).
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

    // `-n`/`-q`/`-p` do not change this line (forms s-10/s-11/s-12 have the same
    // value); changing the algorithm changes the name and the digest (s-13/s-14).
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

/// The eight strings `-x` prints are the ones hard-coded in C's `MD5TestSuite`
/// (`md5.c:118-133`), shared by all eight algorithms (`md2.c`/`md4.c`/`rmd160.c`/
/// `sha1.c`/`sha2*.c` all just `#include "md5.c"`), so here the **whole nine-line
/// block** compares verbatim against the reference (form x-1). The last string is
/// 80 characters assembled with backslash continuations in C (`:143-144`); a
/// wrong length diverges on that line immediately.
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

    // Given twice, it prints two sets (form x-5: the getopt loop calls
    // `testsuitefunc` once per hit).
    let (out, _, _) = run(&["-a", "md5", "-x", "-x"], b"");
    assert_eq!(out, format!("{expected}{expected}"), "重复出现不合并");

    // The same strings with SHA512's 128 hex digits (form x-2 spot-checks the
    // first and last; a wrong length diverges on the spot).
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

/// The first two of `-t`'s four lines are hard-coded: `%s time trial.  Digesting
/// %d %d-byte blocks ...` followed by `" done\n"` (`md5.c:81-83`, `:101`), so the
/// first line ends with `... done`, with **two** spaces after `trial.`. The
/// second line's digest is determined by those 1000 x 1000 bytes (form t-1); the
/// last two lines are wall-clock readings, so only their shape can be pinned —
/// `Time = <n> seconds` and `Speed = <n> bytes/second`.
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
    // The last two lines are wall-clock readings, so their values cannot be
    // pinned; what is pinned is C's arithmetic:
    // `Speed = 1000*1000 / (delta != 0 ? delta : 1)` (`md5.c:109-111`). When the
    // run finishes within a second the readings are exactly the reference's
    // `Time = 0` and `Speed = 1000000`.
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
        // When the run finishes within a second (the case on this machine and in
        // containers, and the reference form t-1 too), the divisor must be 1: if
        // this branch were not pinned, an injection that hard-codes the divisor
        // to 1 would be invisible here.
        assert_eq!(speed, 1_000_000, "Time = 0 那一支的 Speed 读数");
    }

    // Changing the algorithm changes only the name and the digest; the four-line
    // skeleton stays.
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

/// The **order** of the three self-printing legs: they finish inside C's getopt
/// loop, so they come before the operands and also before the error diagnostic.
/// Reference forms s-7/x-6/t-3 are all "leg lines first, file-name error after",
/// and e-1/e-2/e-3 are all "stdout already has that line, stderr has the
/// diagnostic plus usage, exit code 1".
#[test]
fn the_self_printing_legs_come_before_operands_and_before_errors() {
    // When both appear, argv order applies: `-x -s abc` prints the nine-line
    // suite first and the string-digest line after (form s-9).
    let (out, _, _) = run(&["-a", "md5", "-x", "-s", "abc"], b"");
    assert!(out.starts_with("MD5 test suite:\n"), "{out:?}");
    assert!(
        out.ends_with("MD5 (\"abc\") = 900150983cd24fb0d6963f7d28e17f72\n"),
        "套件之后才是 -s 那一行"
    );

    // With an operand, the legs come first and the file after; an unreadable
    // file still gets that `warn` line (form s-7).
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

    // The error cannot undo it: in all three shapes stdout has that line and the
    // exit code is 1.
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

    // Reverse control: when `-s` itself fails `requirehash`, nothing is printed
    // (form e-4: stdout empty).
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

/// The placeholder sentence is entirely gone: the eight algorithm bodies, the
/// three self-printing legs `-s`/`-t`/`-x`, and the `-c` check leg with a hash all
/// produce real readings from the reference binary, so
/// `cksum: -a crypto algorithms are not modelled yet` (exit 2) no longer exists
/// in this wrapper — this gate pins that "no shape falls back to that
/// placeholder". Source of the criteria: the three self-printing legs in
/// `cksum.c` `:255-275`, and `do_check`'s digest legs `:424-426` and `:454`.
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

/// `cksum -c` meeting the `NAME (file) = digest` shape: C recomputes the file
/// with `hash->filefunc` and compares character by character
/// (`cksum.c:424-426`); **a full match prints nothing** (reference form c-1),
/// and this shape needs no `-a` — the algorithm name is recognized from the line
/// prefix (`:343-352`, same values as reference form c-7). The expected digest
/// comes from reference form f-4 (same content `hello\n`).
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

    // Changing the algorithm changes the digest: the same bytes with SHA256's 64
    // hex digits (same family of readings as reference f-4).
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

/// A line with a wrong digest: C prints `(MD5) <file>: FAILED` (`printf("(%s) ",
/// hash->hashname)` at `:454`, then the sentence at `:455`) and the whole run
/// exits 1 (`badcnt > 0 => rval = 1`, `:463-464`). The name in the prefix follows
/// **the one at the line start**, not `-a`: reference form g-11 combines `-a md5`
/// with a `SHA1 (...)` line and reports `(SHA1) `.
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

    // The line-start name wins over `-a`: this line is recomputed as SHA1, so it
    // reports `(SHA1) ` too (reference forms g-11/g-12).
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

/// The two paths **diverge** when the file cannot be read: the branch with a hash
/// goes through `access(filename, R_OK)` (`:424`) and on failure only marks that
/// line bad, with no warn at all; only the crc branch warns (`:432`, and only
/// under `-w`). Reference forms c-5 and h-13 (the latter with `-w`) both show
/// only the single `(MD5) <file>: FAILED` line.
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

/// An unrecognized algorithm name: after scanning `hashes[]` without a hit, C
/// sets `rval = 1` and `continue`s (`:356-366`), printing **no FAILED line**; the
/// `unknown hash: <whole line>` sentence is printed only under `-w` (reference
/// forms c-3 and c-4: the former has both stdout and stderr empty, the latter
/// puts the whole line on stderr).
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

    // Case-sensitive: a lowercase line start like `md5` is equally unrecognized
    // (reference form g-10), the exact opposite of the `-a` leg.
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

/// The `hash` state **persists across lines**: when C recognizes a digest line it
/// points `hash` at it (`hash = nhash`, `:363`), so later ordinary lines without
/// a `(` in the same file also take the digest branch (`:373-389`), and the first
/// word is compared as a whole against the recomputed digest — reference forms
/// c-6/h-1/h-4 all show ordinary lines marked bad with the `(MD5) ` prefix, and
/// h-4 gives **no `-a`** at all (the algorithm was recognized from the first
/// line).
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

    // The reverse order (ordinary line first) behaves the same way: `hash` is not
    // yet established, so the crc branch applies and reports the unprefixed
    // `<file>: FAILED` (reference form c-8).
    let reversed = scratch("mixed-reversed");
    fs::write(&reversed, format!("12345 {good_name}\n")).unwrap();
    let (out, status, _) = run(&["-c", reversed.to_str().unwrap()], b"");
    assert_eq!(out, format!("{good_name}: FAILED\n"), "不带前缀");
    assert_eq!(status, 1);

    let _ = fs::remove_file(&good);
    let _ = fs::remove_file(&sums);
    let _ = fs::remove_file(&reversed);
}

/// The two format diagnostics diverge on state, and the sentences differ under
/// `-w`: with neither `(` nor a space, an unselected `hash` reports
/// `bogus format: <line>. Skipping...` (`:395-401`, reference form h-9) while a
/// selected one reports `no filename in <line>? Skipping...` (`:377-383`). A `(`
/// without a matching `)` still uses the former sentence (`:322-328`). All three
/// shapes record one failure.
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

    // Same line, but let `hash` be established by an earlier line -> the other
    // sentence is reported.
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

    // A `(` without a matching `)`: still the `bogus format` sentence (`:322-328`).
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

/// The three file legs compare verbatim against the reference (sections
/// f-1/f-3/c-9 in `tmp/cksum-hash-state/readings.log`): an unreadable operand is
/// `<program>: <operand>: <strerror>`, the directory shape is
/// `.: Is a directory`, and an unopenable checksum file is `<program>: Cannot
/// read <path>: <strerror>` with exit 1. All three go through the `err(3)`/
/// `warn(3)` seam (`bin_support::warn_line` -> `minix_sys::err_line`), not
/// through `warn(3)` used as `warnx(3)`.
#[test]
fn the_three_file_legs_carry_the_strerror_segment() {
    let missing = scratch("fileleg-missing").to_str().unwrap().to_owned();
    let here = ".";

    // f-1: `-a md5 <unreadable file>` produces no digest line and exits 1.
    let (out, status, err) = run(&["-a", "md5", &missing], b"");
    assert_eq!(out, "", "读不到的那档不产行");
    assert_eq!(
        (status, err.as_str()),
        (
            1,
            format!("cksum: {missing}: No such file or directory\n").as_str()
        )
    );

    // f-3: a directory as the operand — the sentence comes from the `strerror`
    // table, not invented "Is a directory" wording.
    let (_, status, err) = run(&["-a", "md5", here], b"");
    assert_eq!((status, err.as_str()), (1, "cksum: .: Is a directory\n"));

    // c-9: an unopenable checksum file itself is `err(1, "Cannot read %s", ...)`
    // — the three-segment sentence plus exit 1.
    let (out, status, err) = run(&["-a", "md5", "-c", &missing], b"");
    assert_eq!(out, "");
    assert_eq!(
        (status, err.as_str()),
        (
            1,
            format!("cksum: Cannot read {missing}: No such file or directory\n").as_str()
        )
    );

    // A checksum-file check without `-a` uses the same sentence (`err(1, ...)`
    // sits outside the option loop and is algorithm-independent).
    let (_, status, err) = run(&["-c", &missing], b"");
    assert_eq!(
        (status, err.as_str()),
        (
            1,
            format!("cksum: Cannot read {missing}: No such file or directory\n").as_str()
        )
    );
}

/// When the file named by a line in the checksum file cannot be read, the two
/// paths differ in sentence count: the crc branch prints
/// `<program>: <file>: <strerror>` under `-w` (`warn("%s", filename)` at
/// `cksum.c:432`) and nothing without `-w`; the digest branch never prints this
/// sentence at all (`access` short-circuits, `:424`). Contrast reference forms
/// h-13 and f-1.
#[test]
fn the_crc_check_leg_warns_with_three_segments_only_under_w() {
    let missing = scratch("crcwarn-missing").to_str().unwrap().to_owned();
    let sums = scratch("crcwarn-sums");
    fs::write(&sums, format!("3827948814 6 {missing}\n")).unwrap();
    let sums_name = sums.to_str().unwrap().to_owned();

    let (out, status, err) = run(&["-c", sums_name.as_str()], b"");
    assert_eq!(
        out,
        format!("{missing}: FAILED\n"),
        "不带 -w 时只有 FAILED 行"
    );
    assert_eq!((status, err.as_str()), (1, ""), "一句不报");

    let (out, status, err) = run(&["-w", "-c", sums_name.as_str()], b"");
    assert_eq!(out, format!("{missing}: FAILED\n"));
    assert_eq!(
        (status, err.as_str()),
        (
            1,
            format!("cksum: {missing}: No such file or directory\n").as_str()
        ),
        "-w 下那一句也是三段形"
    );

    let _ = fs::remove_file(&sums);
}

/// The crc leg (an operand without `-a`) now goes through the twin face too, so
/// its sentence can be pinned: C prints `warn("%s", fn)` at `cksum.c:491-495` —
/// program name, operand, `strerror`, three segments. When calling
/// `minix_sys::open` directly, the host could only pin the placeholder
/// transport's `Input/output error` (carrier, not behavior); after the seam
/// change, reference and implementation return the same sentence (A/B: zero
/// mismatches across 232 shapes).
#[test]
fn the_crc_operand_leg_prints_the_same_three_segments_as_the_hash_leg() {
    let missing = scratch("crcshape-missing").to_str().unwrap().to_owned();
    let (out, status, err) = run(&[&missing], b"");
    assert_eq!((out.as_str(), status), ("", 1), "读不到就没有校验和行");
    assert_eq!(
        err,
        format!("cksum: {missing}: No such file or directory\n"),
        "不带 -a 也是三段形"
    );

    // The directory shape (reference f-3): the sentence comes from the carrier's
    // strerror table, not invented wording.
    let (_, status, err) = run(&["."], b"");
    assert_eq!((status, err.as_str()), (1, "cksum: .: Is a directory\n"));

    // Good and bad files mixed: the bad file takes a position but produces no
    // line, while good files come out in argv order (a corollary of reference
    // f-2).
    let good = scratch("crcshape-good");
    fs::write(&good, "abc\n").unwrap();
    let good_name = good.to_str().unwrap().to_owned();
    let (out, status, err) = run(&[&missing, &good_name], b"");
    assert_eq!(
        err,
        format!("cksum: {missing}: No such file or directory\n")
    );
    assert_eq!(
        out,
        format!("1112837078 4 {good_name}\n"),
        "abc 加换行的 POSIX CRC 与字节数（读数取自参照件：`cksum_ref <abc+换行>` 交回 `1112837078 4`）"
    );
    assert_eq!(status, 1);

    let _ = fs::remove_file(&good);
}

/// Byte-shaped argv fixture: a bare `ff` cannot be written as `&str`, so
/// arguments are passed down as bytes (`OsStr::from_bytes`) and the comparison is
/// against the byte string the child process actually receives.
fn run_argv(args: &[&[u8]], input: &[u8]) -> (Vec<u8>, Vec<u8>, Option<i32>) {
    use std::os::unix::ffi::OsStrExt as _;
    let mut child = Command::new(CKSUM)
        .args(args.iter().map(|a| std::ffi::OsStr::from_bytes(a)))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the cksum binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (output.stdout, output.stderr, output.status.code())
}

/// A **byte-shaped** scratch file name: `scratch` returns a `PathBuf`, while this
/// one must carry a bare `ff` in the name.
fn scratch_bytes(suffix: &[u8]) -> Vec<u8> {
    let mut name = std::env::temp_dir()
        .as_os_str()
        .to_string_lossy()
        .into_owned()
        .into_bytes();
    name.extend_from_slice(format!("/cksum-face-{}-", std::process::id()).as_bytes());
    name.extend_from_slice(suffix);
    name
}

#[test]
fn a_raw_byte_in_the_s_string_is_echoed_as_that_byte_by_cksum() {
    // C's `MD5String` hands the string to `printf("%s (\"%s\") = ", ...)`
    // (`md5.c:60-67`), echoing it verbatim without escaping. This shape does not
    // pin the digest value (the algorithm body has its own gate); it pins the
    // bare byte in the echo slot: if the value path still crossed a
    // `from_utf8_lossy` layer, this would become the three bytes `ef bf bd`.
    let value = [b"ab".as_slice(), b"\xff", b"cd"].concat();
    let args: [&[u8]; 4] = [
        b"-a".as_slice(),
        b"md5".as_slice(),
        b"-s".as_slice(),
        value.as_slice(),
    ];
    let (out, err, code) = run_argv(&args, b"");
    assert_eq!(err, b"", "选中算法且给了串，不该有报文，实读 {err:?}");
    assert_eq!(code, Some(0));
    assert!(
        out.starts_with(b"MD5 (\"ab\xffcd\") = "),
        "串的回显位该逐字节等于 argv 里那一串，实读 {out:?}"
    );
}

#[test]
fn a_raw_byte_in_an_operand_name_is_echoed_as_that_byte_by_cksum() {
    // C's digest line is `printf("%lu %llu %s\n", v, len, fn)` (`printfile` at
    // `cksum.c:486`), and that `fn` is the byte string from argv. The argument
    // seam used to fold it into replacement characters in `support::args()` (one
    // `ff` became the three bytes `ef bf bd`), so the echo slot differed from the
    // real source by two bytes. `1112837078 4` is the POSIX CRC and byte count
    // for `abc` plus a newline, taken from the crc shape gate on this page.
    //
    // The file is written under the **folded** name only to separate the two
    // legs: the open leg still requires `&str` (a known libs gap), so this
    // wrapper folds once before opening. Written under the raw byte name, the
    // host would only produce `ENOENT`, which falls to the message leg and never
    // reaches the echo slot. The round that closes that libs gap should switch
    // this to the raw byte name; the assertions themselves stay unchanged.
    let raw = scratch_bytes(b"raw\xfflib");
    let folded = String::from_utf8_lossy(&raw).into_owned();
    fs::write(&folded, b"abc\n").expect("折叠名的临时档该落得下去");
    let (out, err, code) = run_argv(&[&raw], b"");
    assert_eq!(err, b"", "开得到就不该有报文，实读 {err:?}");
    assert_eq!(code, Some(0));
    let mut want = b"1112837078 4 ".to_vec();
    want.extend_from_slice(&raw);
    want.push(b'\n');
    assert_eq!(out, want, "摘要行的名字位该是 argv 里那串原字节");
    let _ = fs::remove_file(&folded);
}
