//! End-to-end face for the `head` binary: real process, real pipes and real
//! files, checked against expectations derived from `minix3/usr.bin/head/head.c`
//! (NetBSD).
//!
//! Two deliberate limits on the differential leg against the host's GNU `head`:
//! inputs are newline-terminated, because this shell rebuilds every printed line
//! with its newline (a declared simplification shared with `tail`) while C and
//! GNU copy bytes; and the `-n 0`/`-c 0` cases are pinned to C alone, since
//! `head.c:90-103` rejects a non-positive count where GNU accepts zero.

use std::fs;
use std::io::Write as _;
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
    child
        .stdin
        .as_mut()
        .expect("stdin piped")
        .write_all(input.as_bytes())
        .expect("writing standard input");
    drop(child.stdin.take());
    let output = child.wait_with_output().expect("the run should finish");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
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

#[test]
fn unknown_option_prints_the_libc_line_then_the_usage_line() {
    // C 是**两行**：`getopt` 先印 `"<prog>: unknown option -- <字母>"`
    // （`minix3/lib/libc/stdlib/getopt.c:101-104`，字母不带引号），再由
    // `head.c:115-117` 的 `case '?'` 走 usage。usage 那一行逐字对位
    // `head.c:199-203`，而且 C 的清单里本来就没有 `-c`/`-q`/`-v`。
    const USAGE: &str = "usage: head [-n lines] [file ...]\n";
    let (out, status, err) = run(&["-x"], "a\n");
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(err, format!("head: unknown option -- x\n{USAGE}"));
    // 缺值是同一形状的另一句（`getopt.c:132-136`）。
    let (out, status, err) = run(&["-n"], "a\n");
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(
        err,
        format!("head: option requires an argument -- n\n{USAGE}")
    );
    // 集外字母不靠位置：`obsolete()` 只折前导那一串，所以文件名之后的 `-5`
    // 走到 getopt 那里就是字母 `5`。
    let good = written("letter-after-operand", b"a1\n");
    let good_text = text(good.as_path());
    let (out, status, err) = run(&[&good_text, "-5"], "");
    assert_eq!(out, "", "不该把 -5 当成操作数去读");
    assert_eq!(status, 1);
    assert_eq!(err, format!("head: unknown option -- 5\n{USAGE}"));
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
    // `warn("%s", *argv)`（`head.c:124-126`）的三个成分：程序名、操作数、
    // `strerror` 句子。旧写法只印前两个。
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
