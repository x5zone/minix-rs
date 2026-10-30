//! End-to-end face for the `head` binary: real process, real pipes and real
//! files, checked against expectations derived from `minix3/usr.bin/head/head.c`
//! (NetBSD).
//!
//! Two limits on the differential leg against the host's GNU `head`: the
//! `-n 0`/`-c 0` cases are pinned to C alone, since `head.c:90-103` rejects a
//! non-positive count where GNU accepts zero. The newline state of the final
//! line is no longer a limit: the line leg records "did the input carry that
//! newline" on the line itself (`minix_textfilter::window::Line`), the same
//! shape as C's character-by-character copy, and the no-trailing-newline
//! reading of the reference `tmp/head-415/head_ref` has passed the gate.

mod face_support;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const HEAD: &str = env!("CARGO_BIN_EXE_head");

fn run(args: &[&str], input: &str) -> (String, i32, String) {
    run_with(HEAD, args, input)
}

fn run_with(program: &str, args: &[&str], input: &str) -> (String, i32, String) {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|_| panic!("{program} should be runnable"));
    face_support::write_stdin(&mut child, input.as_bytes());
    let output = child.wait_with_output().expect("the run should finish");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// The **byte-shaped** fixture for argv forms: a bare `ff` cannot be written as
/// a `&str`, so the arguments go down through `OsStr::from_bytes` and the
/// comparison is against the byte string the child actually received (in the
/// text-shaped fixture `from_utf8_lossy` turns a bare `ff` and `ef bf bd` into
/// the same string, which is exactly the seam under test).
fn run_argv(args: &[&[u8]], input: &[u8]) -> (Vec<u8>, i32, Vec<u8>) {
    use std::os::unix::ffi::OsStrExt as _;
    let mut child = Command::new(HEAD)
        .args(args.iter().map(|w| std::ffi::OsStr::from_bytes(w)))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the head binary should be runnable");
    face_support::write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        output.stdout,
        output.status.code().unwrap_or(-1),
        output.stderr,
    )
}

/// Writes a file under its **raw byte name** and returns its path bytes; the
/// same content is also written under its `from_utf8_lossy` folded name (both
/// copies are needed so the open leg succeeds whichever name the reader
/// resolves).
fn write_raw(dir: &Path, name: &[u8], body: &[u8]) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt as _;
    let path = dir.join(std::ffi::OsStr::from_bytes(name));
    fs::write(&path, body).expect("按原字节名落盘");
    path.as_os_str().as_bytes().to_vec()
}

/// The command-line half is byte-based too. `head` is unique in this family:
/// **the `-c`/`-n` value is echoed back by C** — `head.c:96-97`, `:102-103`'s
/// `errx(1, "illegal byte count -- %s", optarg)` puts the whole optarg into the
/// sentence (`cut`'s two errx lines carry no value, and the echo slot of
/// `column`/`comm` sits on `err_line`'s `&str` mouth, so this is the only leg
/// where the value enters both the parse and the diagnostic). Once the text
/// seam folds, `ff` becomes the three bytes `ef bf bd` and the diagnostic
/// differs from the reference by two bytes. Measured against `tmp/head-415/head`:
/// `-c<ff>` → `head: illegal byte count -- <ff>`; `-c 5<ff>` → the same
/// sentence with the trailing byte; the obsolete form `-5<ff>` after rewriting
/// reports `5<ff>` (`head.c:188-192` builds `-n` plus all bytes of the original
/// string minus the dash, so the non-UTF-8 byte rides along). One section pins
/// the option-letter slot, another pins the **banner**: the banner is printed
/// by this shell itself (`head.c:130-131`), not through `&str`, so the file
/// name must be echoed byte for byte; the disk holds both the raw byte name and
/// its folded twin (same content), which removes the registered open-path leg
/// (`ARGV-BYTE-PATH-IN-LIBS`) from the reading — with only the raw name, this
/// shell would necessarily hit `ENOENT` while the reference can read it, so the
/// reading would measure the open leg rather than this seam.
#[test]
fn an_illegal_count_value_is_echoed_as_the_argv_bytes_and_so_is_the_banner() {
    use std::os::unix::ffi::OsStrExt as _;
    let usage_line = "usage: head [-n lines] [file ...]\n";
    let cases: [(&[&[u8]], &[u8]); 3] = [
        (&[&b"-c\xff"[..]], b"head: illegal byte count -- \xff\n"),
        (
            &[b"-c".as_slice(), &b"5\xff"[..]],
            b"head: illegal byte count -- 5\xff\n",
        ),
        (&[&b"-n\xff"[..]], b"head: illegal line count -- \xff\n"),
    ];
    for (args, want) in cases {
        let (out, code, err) = run_argv(args, b"alpha\nbeta\n");
        assert_eq!(
            (out.as_slice(), code),
            (&b""[..], 1),
            "{args:?} 的报错不落标准输出：{err:?}"
        );
        assert_eq!(err, want, "{args:?} 的报文该带 argv 原字节");
    }

    // The obsolete form: `-5<ff>` is first rewritten to `-n5<ff>`, and the bare
    // byte in the value still rides along.
    let dir = scratch("obsolete");
    let plain = dir.join("f1.txt");
    fs::write(&plain, b"one\ntwo\n").expect("写 f1");
    let (out, code, err) = run_argv(&[&b"-5\xff"[..], plain.as_os_str().as_bytes()], b"");
    assert_eq!((out.as_slice(), code), (&b""[..], 1));
    assert_eq!(err, b"head: illegal line count -- 5\xff\n".as_slice());

    // Option-letter slot: `-<ff>` hands back that bare byte, with the libc line
    // first and usage after.
    let (out, code, err) = run_argv(&[&b"-\xff"[..]], b"alpha\n");
    assert_eq!(out, Vec::new(), "集外字母不打数据");
    assert_eq!(code, 1);
    assert_eq!(
        err,
        [
            b"head: unknown option -- \xff\n".as_slice(),
            usage_line.as_bytes(),
        ]
        .concat(),
    );

    // Banner: under `-v` a single operand also gets a banner, the file name is
    // echoed as raw bytes, and the open goes through the folded twin.
    let raw = write_raw(&dir, b"name\xff", b"one\ntwo\n");
    fs::write(dir.join("name\u{fffd}"), b"one\ntwo\n").expect("写折叠孪生件");
    let (out, code, err) = run_argv(&[b"-v".as_slice(), &raw], b"");
    // The banner prints the whole argv string (C prints `*argv`, i.e. the
    // **path** the caller gave rather than the last component), so the
    // expectation is built as "==> " + raw path bytes + " <==\n" + content.
    let want = [b"==> ".as_slice(), raw.as_slice(), b" <==\none\ntwo\n"].concat();
    assert_eq!(
        (out.as_slice(), code, err.as_slice()),
        (want.as_slice(), 0, &b""[..]),
        "横幅里的档名该是 argv 里那串原字节"
    );
}

fn scratch(tag: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!("head-face-{}-{tag}", std::process::id()));
    fs::create_dir_all(&path).expect("scratch dir");
    path
}

fn written(tag: &str, bytes: &[u8]) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!("head-face-{}-{tag}", std::process::id()));
    fs::write(&path, bytes).expect("writing the scratch file");
    path
}

fn text(path: &Path) -> String {
    path.to_str().unwrap().to_owned()
}

/// Twelve newline-terminated lines, so the ten-line default is visible.
fn twelve() -> String {
    (1..=12).map(|number| format!("{number}\n")).collect()
}

fn first_six() -> String {
    (1..=6).map(|number| format!("{number}\n")).collect()
}

#[test]
fn default_is_the_first_ten_lines() {
    let (out, status, err) = run(&[], &twelve());
    assert_eq!(out, first_six() + "7\n8\n9\n10\n");
    assert_eq!(status, 0, "stderr was {err:?}");
    assert_eq!(err, "");
}

#[test]
fn counts_arrive_by_option_or_obsolete_form() {
    // `obsolete()` rewrites a leading `-<digit>…` into `-n<digit>…` (`head.c`).
    assert_eq!(run(&["-n", "2"], &twelve()).0, "1\n2\n");
    assert_eq!(run(&["-3"], &twelve()).0, "1\n2\n3\n");
    // Byte mode copies raw bytes, newline included: four bytes of `1\n2\n3\n…`
    // are `1`, newline, `2`, newline.
    assert_eq!(run(&["-c", "4"], &twelve()).0, "1\n2\n");
}

#[test]
fn counts_above_the_window_are_honoured_not_refused() {
    // C `head()` just counts down, so a request larger than the file prints the
    // file; the window holds only what the input offered.
    assert_eq!(run(&["-n", "40"], &twelve()).0, twelve());
    assert_eq!(run(&["-n", "1000000"], &twelve()).0, twelve());
}

#[test]
fn non_positive_counts_reject_with_c_wording() {
    // `head.c:90-103`: `strtoimax` then `*ep || count <= 0` →
    // `errx(1, "illegal line count -- %s")` (or "byte"). Note `tail` is not the
    // same: its `ARG` macro accepts a zero offset and prints nothing.
    for (args, message) in [
        (vec!["-n", "0"], "head: illegal line count -- 0"),
        (vec!["-n", "-3"], "head: illegal line count -- -3"),
        (vec!["-c", "0"], "head: illegal byte count -- 0"),
        (vec!["-n", "x"], "head: illegal line count -- x"),
    ] {
        let (out, status, err) = run(args.as_slice(), "a\n");
        assert_eq!(out, "", "args {args:?} printed {out:?}");
        assert_eq!(status, 1, "args {args:?} stderr was {err:?}");
        assert!(err.contains(message), "stderr was {err:?}");
    }
}

/// The same command seam as `run`, but standard error comes back **byte for
/// byte**: `from_utf8_lossy` renders a bare `c3` as the three bytes of U+FFFD,
/// and the text form cannot tell "one byte" apart from "two bytes" as failure
/// modes.
fn run_bytes(args: &[&str], input: &str) -> (Vec<u8>, i32, Vec<u8>) {
    let mut child = Command::new(HEAD)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the head binary should be runnable");
    face_support::write_stdin(&mut child, input.as_bytes());
    let output = child.wait_with_output().expect("the run should finish");
    (
        output.stdout,
        output.status.code().unwrap_or(-1),
        output.stderr,
    )
}

/// The option-letter slot on the out-of-set-letter line is **one bare byte**.
/// The expectations are taken form by form from the freshly built reference
/// `tmp/head-415/head_ref` (`minix3/usr.bin/head/head.c` + Minix's `getopt.c` +
/// the shim, started through a same-named `head` symlink so the program-name
/// segment matches this shell and the whole stderr is compared directly): `-é`
/// and `-qé` both give `head: unknown option -- ` + a single `c3` + newline
/// followed by the usage line (`head.c:199-203`), exiting **1**; `-Zé` stops at
/// `Z`; conversely, in `-cé` the `é` is a **value**, not an option letter, and
/// goes through the command's own `head: illegal byte count -- é`
/// (`head.c:92-102`) **on a line of its own, with no usage**. The existing
/// `tmp/family-diag-ref/head_ref` cannot be used: it prints `-Z` returning
/// glibc's quoted `invalid option -- 'Z'`, and the program name in the usage
/// line is still `?` (the third time this ownership rule has been honoured).
/// The ASCII forms (`-x`, `-5`, `-n` missing its value) already have their owner
/// in this file and are not duplicated.
#[test]
fn the_libc_line_of_a_multibyte_flag_is_one_byte_for_head() {
    const USAGE: &[u8] = b"usage: head [-n lines] [file ...]\n";
    let want: Vec<u8> = [b"head: unknown option -- ".as_slice(), b"\xc3\n", USAGE].concat();
    for form in ["-\u{e9}", "-q\u{e9}"] {
        let (out, status, err) = run_bytes(&[form], "a\n");
        assert_eq!(out, Vec::new(), "{form} 的报文只走标准错误");
        assert_eq!(status, 1, "{form} 的 stderr 是 {err:?}");
        assert_eq!(err, want, "{form} 的字母该是一个裸字节 c3");
    }

    let (_out, status, err) = run_bytes(&["-Z\u{e9}"], "a\n");
    assert_eq!(status, 1);
    assert_eq!(
        err,
        [b"head: unknown option -- Z\n".as_slice(), USAGE].concat(),
        "环停在第一个坏字母上，第二个字节根本不该被读到"
    );

    let (_out, status, err) = run_bytes(&["-c\u{e9}"], "a\n");
    assert_eq!(status, 1);
    assert_eq!(
        err,
        [
            b"head: illegal byte count -- ".as_slice(),
            "\u{e9}".as_bytes(),
            b"\n".as_slice(),
        ]
        .concat(),
        "带值字母把两字节整个当值收走，句子是命令自己的那一句、不接 usage"
    );
}

#[test]
fn unknown_option_prints_the_libc_line_then_the_usage_line() {
    // C emits **two lines**: `getopt` first prints `"<prog>: unknown option --
    // <letter>"` (`minix3/lib/libc/stdlib/getopt.c:101-104`, the letter without
    // quotes), and then `head.c:115-117`'s `case '?'` runs usage. That usage
    // line matches `head.c:199-203` verbatim, and C's list never had
    // `-c`/`-q`/`-v` anyway.
    const USAGE: &str = "usage: head [-n lines] [file ...]\n";
    let (out, status, err) = run(&["-x"], "a\n");
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(err, format!("head: unknown option -- x\n{USAGE}"));
    // A missing value is another sentence of the same shape (`getopt.c:132-136`).
    let (out, status, err) = run(&["-n"], "a\n");
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(
        err,
        format!("head: option requires an argument -- n\n{USAGE}")
    );
}

#[test]
fn an_operand_ends_option_scanning_like_c() {
    // C's `getopt` does **not** permute: `minix3/lib/libc/stdlib/getopt.c:71-74`
    // returns -1 at the first non-option token without advancing `optind`, so
    // `head.c:118-119`'s `argv += optind` treats that token and **everything**
    // after it as operands. Measured over eight forms against the reference
    // (`python3 tmp/head-424/matrix.py`, readings in `tmp/head-424/matrix-ref.log`):
    // `head a.txt -5` prints `==> a.txt <==` plus ten lines of content, then
    // `head: -5: No such file or directory` and exits 1 — the `unknown option`
    // sentence never appears. Before the change this shell reported that
    // sentence plus usage, and printed **not a single line of content**.
    let good = written("operand-first", b"a1\na2\n");
    let good_text = text(good.as_path());
    let (out, status, err) = run(&[&good_text, "-5"], "");
    assert_eq!(out, format!("==> {good_text} <==\na1\na2\n"));
    assert_eq!(status, 1);
    assert_eq!(err, "head: -5: No such file or directory\n");

    // Real flags do not reach the option face either: `-q` here is a file named
    // `-q`, the banner prints as usual (two operands), and the quiet flag has no
    // effect.
    let (out, status, err) = run(&[&good_text, "-q"], "");
    assert_eq!(out, format!("==> {good_text} <==\na1\na2\n"));
    assert_eq!(status, 1);
    assert_eq!(err, "head: -q: No such file or directory\n");

    // Options before an operand still take effect, but once the loop hits an
    // operand it never comes back: `-n 1` takes one line, and the `-5` after it
    // is just another file name.
    let (out, status, err) = run(&["-n", "1", &good_text, "-5"], "");
    assert_eq!(out, format!("==> {good_text} <==\na1\n"));
    assert_eq!(status, 1);
    assert_eq!(err, "head: -5: No such file or directory\n");

    // A `--` after an operand is not "end of options" but a file name (C's loop
    // never reaches it); a lone `-` also takes -1 at the same place
    // (`getopt.c:96-104`: `head`'s option string has no `-`), so it and what
    // follows are operands. Both forms read the same against the reference.
    let (out, status, err) = run(&[&good_text, "--", "-5"], "");
    assert_eq!(
        out,
        format!("==> {good_text} <==\na1\na2\n"),
        "-- 之后的 -5 不该被当成旗标"
    );
    assert_eq!(status, 1);
    assert_eq!(
        err,
        "head: --: No such file or directory\nhead: -5: No such file or directory\n"
    );
    let (out, status, err) = run(&[&good_text, "-"], "");
    assert_eq!(out, format!("==> {good_text} <==\na1\na2\n"));
    assert_eq!(status, 1);
    assert_eq!(err, "head: -: No such file or directory\n");
    let _ = fs::remove_file(&good);
}

#[test]
fn banners_follow_the_q_and_v_rules() {
    let one = written("banner-a", b"a1\na2\n");
    let two = written("banner-b", b"b1\nb2\n");
    let (one_text, two_text) = (text(one.as_path()), text(two.as_path()));

    // More than one operand and no `-q`: a banner per file, blank line between.
    let (out, _, _) = run(&[&one_text, &two_text], "");
    assert_eq!(
        out,
        format!("==> {one_text} <==\na1\na2\n\n==> {two_text} <==\nb1\nb2\n")
    );
    // `-q` suppresses them, `-v` forces one even for a single file.
    assert_eq!(run(&["-q", &one_text, &two_text], "").0, "a1\na2\nb1\nb2\n");
    assert_eq!(
        run(&["-v", &one_text], "").0,
        format!("==> {one_text} <==\na1\na2\n")
    );

    for path in [&one, &two] {
        let _ = fs::remove_file(path);
    }
}

#[test]
fn an_unopenable_operand_warns_without_a_banner_and_sets_the_status() {
    // `fopen` failure is `warn(); eval = 1; continue;`, and the banner test runs
    // only after a successful open. A bare `-` is a literal name for `head`,
    // because every operand goes through `fopen` (`head.c:121`).
    let good = written("partial-a", b"g1\n");
    let good_text = text(&good);
    let missing = format!("head-face-{}-missing", std::process::id());
    let (out, status, err) = run(&[&missing, &good_text], "");
    assert_eq!(status, 1);
    // The three components of `warn("%s", *argv)` (`head.c:124-126`): program
    // name, operand, `strerror` sentence. The old form printed only the first
    // two.
    assert_eq!(err, format!("head: {missing}: No such file or directory\n"));
    assert_eq!(out, format!("==> {good_text} <==\ng1\n"));

    let (out, status, err) = run(&["-"], "");
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert!(err.contains("head: -"), "stderr was {err:?}");

    let _ = fs::remove_file(&good);
}

#[test]
fn agrees_with_the_host_head_on_the_shared_forms() {
    let cases: Vec<(Vec<&str>, String)> = vec![
        (vec![], twelve()),
        (vec!["-n", "3"], twelve()),
        (vec!["-n", "40"], twelve()),
        (vec!["-c", "5"], twelve()),
    ];
    let system = if Command::new("head").arg("--version").output().is_ok() {
        "head"
    } else {
        eprintln!("no host head to compare against; skipping the differential leg");
        return;
    };
    for (options, input) in cases {
        let ours = run(&options, &input);
        let theirs = run_with(system, &options, &input);
        assert_eq!(
            ours.0, theirs.0,
            "options {options:?}: ours {:?}, host {:?}",
            ours.0, theirs.0
        );
        assert_eq!(ours.1, theirs.1, "options {options:?}");
    }
}

/// An unterminated final line is handed back as is; this shell does not
/// manufacture a newline for it. C's line shape is a character-by-character copy
/// (`minix3/usr.bin/head/head.c:169-172`'s `getc`/`putchar`), and the reference
/// `tmp/head-415/head_ref` for `alpha\nbeta\ngamma` hands back exactly this
/// string with no newline at the end; intermediate lines still each carry a
/// newline. The register entry `HEAD-FINAL-LINE-ADDS-NEWLINE` is thereby
/// closed.
#[test]
fn an_unterminated_final_line_reaches_stdout_without_a_newline() {
    let (out, code, err) = run(&[], "alpha\nbeta\ngamma");
    assert_eq!(
        (out.as_str(), code, err.as_str()),
        ("alpha\nbeta\ngamma", 0, "")
    );
    // When the count cuts at an intermediate line, that line already carried a
    // newline, so it still has one.
    let (two, _, _) = run(&["-n", "2"], "alpha\nbeta\ngamma");
    assert_eq!(two, "alpha\nbeta\n");
}
