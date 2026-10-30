//! End-to-end face for the `join` binary: real process, real pipes, checked against
//! `minix3/usr.bin/join/join.c` (the operand handling `:236-253`, the mutually
//! exclusive check `:235`, `usage` `:630-637`).
//!
//! `join` sits on the `err(3)`/`warnx(3)` side of the family boundary this crate keeps
//! having to relearn: the file leg is `err(1, "%s", *argv)` (`join.c:244`, `:249`), so
//! the line is `join: <operand>: <strerror(errno)>` — program name AND the number's
//! sentence. The shell printed `join: cannot open file 1`: the operand was gone, the
//! number was gone, and the words were invented. `usage()` is two lines in C with
//! `getprogname()` in the first one (`:632-635`); the shell had one line whose option
//! list did not match C either.
//!
//! One leg was missing outright: C checks `F1->fp == stdin && F2->fp == stdin` and
//! `errx(1, "only one input file may be stdin")` (`:250-251`). Without it the two
//! operands both read the same stream. Checked here before any bytes are compared,
//! because the whole point is that nothing is read.
//!
//! Unlike the file-ops shells, a hosted `join` can open real files, so the success path
//! is exercised too — but only as "stdout carries the join, stderr stays silent", which
//! is what the diagnostic legs owe.

use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;
use std::process::{Command, Stdio};

const USAGE_FIRST: &str = "usage: join [-a fileno | -v fileno] [-e string] [-j fileno field]\n";
const USAGE_SECOND: &str = "            [-o list] [-t char] [-1 field] [-2 field] file1 file2\n";

fn run_at(program: &Path, args: &[&str]) -> (String, i32, String) {
    let output = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("the join binary should be runnable");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

fn run(args: &[&str]) -> (String, i32, String) {
    run_at(Path::new(env!("CARGO_BIN_EXE_join")), args)
}

/// Same seam as `run`, but both output streams come back **byte for byte**:
/// `from_utf8_lossy` renders a raw `c3` as the three-byte U+FFFD, so the text
/// form cannot distinguish the "one byte" and "two bytes" failure modes.
fn run_bytes(args: &[&str]) -> (Vec<u8>, i32, Vec<u8>) {
    let output = Command::new(env!("CARGO_BIN_EXE_join"))
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("the join binary should be runnable");
    (
        output.stdout,
        output.status.code().unwrap_or(-1),
        output.stderr,
    )
}

/// The letter slot of the out-of-set-letter line is **one raw byte**: the
/// reference binary `tmp/join-407/join_ref` (freshly assembled here:
/// `minix3/usr.bin/join/join.c` + Minix's `getopt.c` + a program-name shim,
/// started through the `join` symlink, messages compared byte for byte)
/// measures `-\u{e9}` as `join: unknown option -- ` + `c3` + newline, then the
/// two usage lines, exit 1; `-Z\u{e9}` stops at `Z` (`join.c:226-228`'s
/// `case '?'` falls straight to `usage()`, the message does not accumulate).
/// Every letter in join's optstring `"\01a:e:j:1:2:o:t:v:"` takes a value, so
/// there is no "valid letter followed by a multibyte" shape to pin; the
/// opposite shape is `-a\u{e9}` — `é` is taken as `-a`'s value, and the answer
/// is the command's own `-a option field number not 1 or 2`, which a rendering
/// change must not touch. The ASCII out-of-set letter and missing-value shapes
/// already have owners in this file and are not repeated under the redundancy
/// rule. **That self-answered sentence's wording must also be pinned verbatim
/// against C**: the `-a` branch in C says `-a option **file** number not 1 or
/// 2` (`join.c:171`, `-v` likewise at `:217`), while the `-1`/`-2`/`-j` three
/// say **field** (`:141`, `:152`, `:186`) — two wordings inside one command;
/// this check first substituted the neighbouring `field` into the `file`
/// branch, and once the gate went red it was changed back to `join.c`'s
/// original sentence.
#[test]
fn the_unknown_letter_of_a_multibyte_flag_is_written_as_one_byte() {
    let want: Vec<u8> = [
        b"join: unknown option -- ".as_slice(),
        b"\xc3\n",
        USAGE_FIRST.as_bytes(),
        USAGE_SECOND.as_bytes(),
    ]
    .concat();
    let (out, status, err) = run_bytes(&["-\u{e9}"]);
    assert_eq!(out, Vec::new(), "报文只走标准错误");
    assert_eq!(status, 1, "stderr 是 {err:?}");
    assert_eq!(err, want, "字母该是一个裸字节 c3");

    let (_out, status, err) = run_bytes(&["-Z\u{e9}"]);
    assert_eq!(status, 1);
    assert_eq!(
        err,
        [
            b"join: unknown option -- Z\n".as_slice(),
            USAGE_FIRST.as_bytes(),
            USAGE_SECOND.as_bytes(),
        ]
        .concat(),
        "环停在第一个坏字母上，`é` 根本不该被读到"
    );

    let (_out, status, err) = run_bytes(&["-a\u{e9}"]);
    assert_eq!(status, 1);
    assert_eq!(
        err,
        [
            b"join: -a option file number not 1 or 2\n".as_slice(),
            USAGE_FIRST.as_bytes(),
            USAGE_SECOND.as_bytes(),
        ]
        .concat(),
        "`-a` 带值，`\u{e9}` 整个被当值收走，答的是命令自己那句"
    );
}

/// C's `err(3)` line: `<prog>: <operand>: <sentence>`.
fn assert_err_line(err: &str, prog: &str, operand: &str) {
    let head = format!("{prog}: {operand}: ");
    assert!(err.starts_with(&head), "`{err}` should start with `{head}`");
    let sentence = &err[head.len()..err.len() - 1];
    assert!(
        !sentence.is_empty() && sentence.chars().any(|c| c.is_lowercase()),
        "缀的应是一句 strerror 文本而不是代号：`{sentence}`"
    );
}

/// Two throwaway files with a shared key column; returns their paths.
fn operand_files() -> (std::path::PathBuf, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("join-face-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("temp dir");
    let one = dir.join("one");
    let two = dir.join("two");
    fs::write(&one, "a 1\nb 2\n").expect("write one");
    fs::write(&two, "a x\nb y\n").expect("write two");
    (one, two)
}

#[test]
fn a_successful_join_says_nothing_on_stderr() {
    // The diagnostic legs owe silence on the success path: C prints only the joined
    // lines (`join.c:253+`), and `stderr` stays empty.
    let (one, two) = operand_files();
    let (out, status, err) = run(&[one.to_str().unwrap(), two.to_str().unwrap()]);
    assert_eq!(status, 0, "两档都能开：`{err}`");
    assert_eq!(err, "", "成功路一声不出");
    assert!(
        out.contains('a') && out.contains('b'),
        "该有连接结果：`{out}`"
    );
    let _ = fs::remove_dir_all(one.parent().unwrap());
}

#[test]
fn an_unopenable_operand_reports_the_operand_and_the_number() {
    let (out, status, err) = run(&["/tmp/join-face-absent", "/tmp/join-face-other"]);
    assert_eq!(out, "", "C 的失败路不写标准输出");
    assert_eq!(status, 1, "`err(1, …)` 是退 1");
    assert_err_line(&err, "join", "/tmp/join-face-absent");
    assert!(
        !err.contains("cannot open"),
        "「cannot open」是本树自创的短语，C 打的是操数名：`{err}`"
    );
}

#[test]
fn two_stdin_operands_are_refused_before_anything_is_read() {
    // `join.c:250-251`: both `FILE*` end up as `stdin`, which C refuses outright —
    // `errx` shaped (program name, phrase, no sentence), and no usage line follows.
    let (out, status, err) = run(&["-", "-"]);
    assert_eq!(out, "", "这一支护住在读之前，不该有任何输出");
    assert_eq!(status, 1);
    assert_eq!(err, "join: only one input file may be stdin\n");
}

#[test]
fn the_mutually_exclusive_pair_is_errx_without_a_usage_line() {
    // `join.c:235`: `errx(1, "-a and -v options mutually exclusive")` — one line, no
    // usage. The other option errors in C are `warnx` + `usage()`; that wording needs
    // the option letter and value, which the deciding half's error type drops, and is
    // registered as the next slice rather than invented here.
    let (_, status, err) = run(&["-a", "1", "-v", "1", "/tmp/join-face-a", "/tmp/join-face-b"]);
    assert_eq!(status, 1);
    assert_eq!(err, "join: -a and -v options mutually exclusive\n");
}

#[test]
fn usage_is_two_lines_with_the_name_only_in_the_first() {
    // `join.c:630-637`: `fprintf(stderr, "usage: %s ...\n            ...\n",
    // getprogname())` — the second line is indented twelve spaces and carries no name.
    let expected = format!("{USAGE_FIRST}{USAGE_SECOND}");
    let (out, status, err) = run(&[]);
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(err, expected, "两行 usage 逐字对 C");

    let (_, status, err) = run(&["/tmp/join-face-only"]);
    assert_eq!(status, 1, "一个操数也是 usage（`argc != 2`）");
    assert_eq!(err, expected);
}

/// The out-of-set letter and the value-bearing option's missing value both
/// print one line of libc's own message before the two usage lines above
/// (`minix3/lib/libc/stdlib/getopt.c:101-104` and `:132-136`). The previous
/// test pinned **bare usage** (a third operand and no operand), whose line
/// count differs — covering it with a generic "usage error" drops that line
/// (which is exactly what this shell did before).
#[test]
fn getopt_owned_legs_print_their_line_before_the_two_usage_lines() {
    let usage = format!("{USAGE_FIRST}{USAGE_SECOND}");
    let (out, status, err) = run(&["-z", "f1", "f2"]);
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(err, format!("join: unknown option -- z\n{usage}"));
    // The missing-value sentence carries that letter (the letter is not
    // quoted, unlike local glibc).
    let (_, status, err) = run(&["f1", "f2", "-t"]);
    assert_eq!(status, 1);
    assert_eq!(
        err,
        format!("join: option requires an argument -- t\n{usage}")
    );
}

#[test]
fn the_report_line_follows_argv_zero_like_getprogname_does() {
    // The program name is `argv[0]`'s last segment: a copy under another name moves the
    // `err(3)` line's head and the usage line's name, while the operand path stays as
    // given. That is what pins the name is not baked in as a literal.
    let dir = std::env::temp_dir().join(format!("join-face-copy-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("temp dir");
    let copied = dir.join("renamed-join");
    fs::copy(Path::new(env!("CARGO_BIN_EXE_join")), &copied).expect("copy the binary");
    fs::set_permissions(&copied, fs::Permissions::from_mode(0o755)).expect("make it run");

    let (_, _, err) = run_at(&copied, &["/tmp/join-face-3", "/tmp/join-face-4"]);
    assert_err_line(&err, "renamed-join", "/tmp/join-face-3");

    let (_, _, err) = run_at(&copied, &[]);
    assert_eq!(
        err,
        format!(
            "usage: renamed-join [-a fileno | -v fileno] [-e string] [-j fileno field]\n{USAGE_SECOND}"
        )
    );

    let _ = fs::remove_dir_all(&dir);
}

/// `warnx` + `usage()` together are **three lines**: the first carries the
/// program name and that branch's own option letter, the last two are usage.
/// The old shell wrote the first line as a generic sentence without the
/// letter, effectively collapsing `join.c`'s eight branches into one.
#[test]
fn a_warnx_leg_prints_its_own_letter_then_the_two_usage_lines() {
    for (args, first) in [
        (
            vec!["-1", "0", "f1", "f2"],
            "join: -1 option field number less than 1\n",
        ),
        (
            vec!["-2", "0", "f1", "f2"],
            "join: -2 option field number less than 1\n",
        ),
        (
            vec!["-j", "0", "f1", "f2"],
            "join: -j option field number less than 1\n",
        ),
        (
            vec!["-a3", "f1", "f2"],
            "join: -a option file number not 1 or 2\n",
        ),
        (
            vec!["-v", "3", "f1", "f2"],
            "join: -v option file number not 1 or 2\n",
        ),
        (
            vec!["-t", "ab", "f1", "f2"],
            "join: illegal tab character specification\n",
        ),
    ] {
        let (out, status, err) = run(&args);
        assert_eq!(out, "", "报文只走 stderr");
        assert_eq!(status, 1, "`{args:?}` 应退 1");
        assert_eq!(
            err,
            format!("{first}{USAGE_FIRST}{USAGE_SECOND}"),
            "`{args:?}` 的三行形与 C 不符"
        );
    }
}

/// The `illegal … -- <value>` branches **do not print the option letter**;
/// they append the whole `optarg` after a pair of dashes
/// (`join.c:145`/`:176`). This also pins `strtol`'s judgment order: the 1 in
/// `1x` is already accepted by the switch, so `-a 1x` falls to the illegal
/// sentence; a value that converts no digit at all, `f1`, hits the `< 1`
/// guard first, so `-j f1` falls to less-than-1 — the old implementation split
/// on "is it all digits" and got both branches wrong. A reconciliation gate
/// against the BSD reference shape by shape (`tmp/family-diag-ref/join_ref`,
/// linked from the real `minix3/usr.bin/join/join.c` plus the
/// `fgetln`/`getprogname` shims). Every expected sentence is a constant
/// **copied back from measurement on the reference** (this file runs our own
/// binary; the reconciliation commands and readings are recorded under the
/// corresponding entry in `rewrite-notes/18-stage-commands/todo.md`: the same
/// shapes were run on both sides and the only remaining difference is the
/// program-name segment), not derived from the C code. It covers shapes the
/// existing gates did not pin: `-2`, `-j`'s tail-character branch, `-o`'s four
/// shapes, `-t`'s attached-value shape. **Usable boundary (must be stated, or
/// this gate will be mistaken for a full-shape comparison)**: that reference
/// links the host glibc's `fgetln` shim, and **reading a file segfaults**, so
/// it can only measure diagnostic paths that exit before opening a file; its
/// `usage()` also segfaults under the same shim (after a `-1 0`-style warnx
/// the usage block is unreachable), so what is compared here is the
/// **first-line sentence**; the two usage lines are pinned separately by the
/// gates above against the C source text.
#[test]
fn the_option_error_first_lines_match_the_bsd_reference() {
    let subject = std::path::Path::new(env!("CARGO_BIN_EXE_join"));
    let (first, second) = operand_files();
    for (args, line) in [
        (vec!["-2", "0"], "join: -2 option field number less than 1"),
        (vec!["-1", "1x"], "join: illegal field number -- 1x"),
        (vec!["-j", "5x"], "join: illegal field number -- 5x"),
        (vec!["-o", "0.1"], "join: malformed -o option field"),
        (vec!["-o", "3.1"], "join: malformed -o option field"),
        (vec!["-o", "1.1x"], "join: malformed -o option field"),
        (vec!["-o", "1.0"], "join: field numbers are 1 based"),
        (
            vec!["-t", "xy"],
            "join: illegal tab character specification",
        ),
    ] {
        let mut words = args.clone();
        words.push(first.to_str().unwrap());
        words.push(second.to_str().unwrap());
        let (_, status, err) = run_at(subject, &words);
        let got = err.lines().next().unwrap_or_default().to_owned();
        assert_eq!(
            got, line,
            "`{args:?}` 的首行该与 C 同形，完整 stderr 是 {err:?}"
        );
        // These branches are all `warnx`+`usage()` or `errx`, and both exit 1
        // (`join.c:usage()`'s `exit(1)` and `errx(1, ...)` share the value).
        assert_eq!(status, 1, "`{args:?}` 的退码该是 1，stderr 是 {err:?}");
    }
    let _ = std::fs::remove_file(&first);
    let _ = std::fs::remove_file(&second);
}

#[test]
fn the_illegal_legs_quote_the_value_without_the_option_letter() {
    let (_, status, err) = run(&["-1", "2x", "f1", "f2"]);
    assert_eq!(status, 1);
    assert_eq!(
        err,
        format!("join: illegal field number -- 2x\n{USAGE_FIRST}{USAGE_SECOND}")
    );

    let (_, _, err) = run(&["-a1x", "f1", "f2"]);
    assert_eq!(
        err,
        format!("join: illegal file number -- 1x\n{USAGE_FIRST}{USAGE_SECOND}")
    );

    let (_, _, err) = run(&["-j", "f1", "f2"]);
    assert_eq!(
        err,
        format!("join: -j option field number less than 1\n{USAGE_FIRST}{USAGE_SECOND}"),
        "`strtol` 转不出数字时值为 0，C 先查 `< 1`"
    );
}

/// The two `-o` `fieldarg` branches and the `-a`+`-v` branch are all `errx`:
/// **one line only**, with no usage appended (`join.c:235`, `:527-532`). The
/// two `-o` failures are two different sentences in C and must not be lumped
/// into one.
#[test]
fn the_errx_legs_print_one_line_and_skip_the_usage_block() {
    let (_, status, err) = run(&["-o", "1.0", "f1", "f2"]);
    assert_eq!(status, 1);
    assert_eq!(err, "join: field numbers are 1 based\n");

    let (_, status, err) = run(&["-o", "3.1", "f1", "f2"]);
    assert_eq!(status, 1);
    assert_eq!(err, "join: malformed -o option field\n");

    let (_, status, err) = run(&["-a1", "-v2", "f1", "f2"]);
    assert_eq!(status, 1);
    assert_eq!(err, "join: -a and -v options mutually exclusive\n");
}

/// The golden samples come from live measurement on the BSD reference:
/// `join_ref` (`minix3/usr.bin/join/join.c` + shims, which must carry the
/// **prototypes** of `fgetln`/`getprogname`, or both are implicitly declared
/// as returning `int` and a truncated pointer segfaults). This pins
/// `push_tok`'s slot order: C's `outfield()` claims the separator slot first
/// and only then decides the content (`join.c:496-497`; only "the field exists
/// but is an empty string" returns with the slot kept, `:503-504`). The old
/// form tested emptiness first and returned, so under `-t,` the three fields
/// `a`, ``, `c` were written as `a,c`, while C writes `a,,c`.
// An absent join field must occupy a separator slot (the golden sample comes
// from live measurement on the reference `join_ref`).
// C's `outfield()` first does `needsep++` to claim the slot and only then
// decides the content (`join.c:496-497`), so when `-1 2`/`-2 2` meet a
// single-column line the join-field slot is empty but still there; with `-e`
// the replacement string is printed into that slot.
#[test]
fn missing_join_field_keeps_its_separator_slot() {
    let dir = std::env::temp_dir().join(format!("join-slot-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("scratch dir");
    let file = |name: &str, body: &[u8]| -> String {
        let path = dir.join(name);
        fs::write(&path, body).expect("scratch file");
        path.to_str().expect("utf-8 path").to_owned()
    };
    let s1 = file("short1", b"q\nr\n");
    let s2 = file("short2", b"q\nr\n");
    let (out, _, err) = run(&["-t,", "-1", "2", "-2", "2", &s1, &s2]);
    assert_eq!(
        out, ",q,q\n,q,r\n,r,q\n,r,r\n",
        "缺席的连接字段占一个空槽，stderr 是 {err:?}"
    );
    let (out, _, _) = run(&["-t,", "-1", "2", "-2", "2", "-e", "-", &s1, &s2]);
    assert_eq!(out, "-,q,q\n-,q,r\n-,r,q\n-,r,r\n", "`-e` 打的就是那个槽");
    let _ = fs::remove_dir_all(&dir);
}

// The six tests below pin the "field splitting" layer to C's shape.
// C's `slurp` splits with `strsep(&bp, tabchar)` (`join.c:378`): **one
// separator byte is one cut**, and `spans` (default 1, cleared to 0 by `-t`,
// `:106`/`:200`) decides whether empty segments become fields. The old form
// in this layer applied "skip empty segments" unconditionally, so under
// `-t,` `a,,c` yielded only two fields: field numbers, the match set, `-o`,
// and `-v` all went wrong together. Every golden sample was **copied back
// from live measurement** on the reference `join_ref`
// (`minix3/usr.bin/join/join.c` + `fgetln`/`getprogname` shims) shape by
// shape; commands and readings are recorded in
// `rewrite-notes/18-stage-commands/todo.md`.
// **One test per shape**: an earlier lesson was that "three shapes in one
// test + a blanket `#[ignore]`" lets a negative control "pass green while the
// fault is injected", so each shape gets its own test (only `-v 1`/`-v 2`,
// two shapes on the same leg, share one, as noted in the test comment), each
// with its own temporary directory.

/// One dedicated directory per test: tests in this file run on different
/// threads of the same process, and a shared directory lets whichever test
/// calls `remove_dir_all` first pull the input out from under another.
fn scratch(tag: &str, files: &[(&str, &[u8])]) -> (std::path::PathBuf, Vec<String>) {
    let dir = std::env::temp_dir().join(format!("join-split-{}-{}", std::process::id(), tag));
    fs::create_dir_all(&dir).expect("scratch dir");
    let mut paths = Vec::new();
    for (name, body) in files {
        let path = dir.join(name);
        fs::write(&path, body).expect("scratch file");
        paths.push(path.to_str().expect("utf-8 path").to_owned());
    }
    (dir, paths)
}

/// Both files have single-column lines; with `-t,` the default join field is
/// the first field (`a`), equal on both sides.
#[test]
fn dash_t_keeps_the_empty_field_between_the_two_words() {
    let (dir, paths) = scratch("default", &[("l", b"a,,c\n"), ("r", b"a,,c\n")]);
    let (out, status, err) = run(&["-t,", &paths[0], &paths[1]]);
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(out, "a,,c,,c\n", "空段是字段，槽位也留着");
    let _ = fs::remove_dir_all(dir);
}

/// `-o` works in terms of field numbers: in C, `1.2` is that empty segment,
/// while the old form in this layer read `1.2` as `c` and `1.3` as
/// out-of-range (falling to `-e`'s empty string), misplacing both slots'
/// content.
#[test]
fn dash_t_numbered_output_fields_count_the_empty_field() {
    let (dir, paths) = scratch("olist", &[("l", b"a,,c\nb,,d\n"), ("r", b"a,,c\ne,,f\n")]);
    let (out, status, err) = run(&["-t,", "-o", "1.1,1.2,1.3,2.1", &paths[0], &paths[1]]);
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(out, "a,,c,a\n", "1.2 是空段、1.3 才是 `c`");
    let _ = fs::remove_dir_all(dir);
}

/// The `-v` path goes through C's `outoneline`: it sends **every** field of
/// the line to `outfield`, and an empty field occupies its slot without
/// printing, so it reports `b,,d` rather than `b,d`. Two profiles of the same
/// fact (`-v 1` and `-v 2`) are combined into one test because they use the
/// same leg.
#[test]
fn dash_v_reports_the_unpaired_line_with_its_empty_field() {
    let (dir, paths) = scratch("dashv", &[("l", b"a,,c\nb,,d\n"), ("r", b"a,,c\ne,,f\n")]);
    let (out, status, err) = run(&["-t,", "-v", "1", &paths[0], &paths[1]]);
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(out, "b,,d\n");
    let (out, _, _) = run(&["-t,", "-v", "2", &paths[0], &paths[1]]);
    assert_eq!(out, "e,,f\n", "另一侧同形");
    let _ = fs::remove_dir_all(dir);
}

/// `-j 2` points both sides' join field at that empty segment: in C the two
/// keys are equal, so two lines times two lines give **four crossed lines**;
/// under the old splitting the two sides' field 2 was `c`/`f`, so the match
/// set itself drifted (yielding only the line `c,a,a`). The same shape as
/// `-1 2` (changing only the first side) reads as empty output on both sides —
/// a **coincidentally identical reading**, so it cannot serve as a gate; the
/// next test therefore uses `-1 2 -2 2`.
#[test]
fn dash_j_on_the_empty_field_matches_both_groups_and_cross_multiplies() {
    let (dir, paths) = scratch("dashj", &[("l", b"a,,c\nb,,d\n"), ("r", b"a,,c\ne,,f\n")]);
    let (out, status, err) = run(&["-t,", "-j", "2", &paths[0], &paths[1]]);
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(
        out, ",a,c,a,c\n,a,c,e,f\n,b,d,a,c\n,b,d,e,f\n",
        "两侧字段 2 都是空串→相等并连接"
    );
    let _ = fs::remove_dir_all(dir);
}

/// Same shape as the previous test but written separately, so that it can
/// distinguish the old implementation (the old splitting gave only one line,
/// `c,a,a`).
#[test]
fn per_file_join_fields_on_the_empty_field_also_cross_multiply() {
    let (dir, paths) = scratch("dash12", &[("l", b"a,,c\nb,,d\n"), ("r", b"a,,c\ne,,f\n")]);
    let (out, status, err) = run(&["-t,", "-1", "2", "-2", "2", &paths[0], &paths[1]]);
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(out, ",a,c,a,c\n,a,c,e,f\n,b,d,a,c\n,b,d,e,f\n");
    let _ = fs::remove_dir_all(dir);
}

/// `-a1 -a2` print the unpaired lines from both sides; all three golden lines
/// were copied from the reference.
#[test]
fn dash_a_prints_every_field_of_the_unpaired_lines_including_empties() {
    let (dir, paths) = scratch("dasha", &[("l", b"a,,c\nb,,d\n"), ("r", b"a,,c\ne,,f\n")]);
    let (out, status, err) = run(&["-t,", "-a1", "-a2", &paths[0], &paths[1]]);
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(out, "a,,c,,c\nb,,d\ne,,f\n");
    let _ = fs::remove_dir_all(dir);
}

/// A trailing delimiter yields one extra empty field in C (`strsep` returns
/// once more at the end of the string): `a,` is two fields, `-a1` reports
/// `a,`, and `-o 1.1,1.2` reports `a,` as well.
/// Which test distinguishes the old form: `-a1` does (the old shell gave `a`);
/// `-o 1.1,1.2` does **not** — the old shell read field 2 as out-of-range and
/// fell to `-e`'s empty string, giving a coincidentally identical output
/// string. Both pin this shape, but only the first carries the distinguishing
/// duty.
#[test]
fn a_trailing_delimiter_leaves_a_trailing_empty_field() {
    let (dir, paths) = scratch("trailing", &[("l", b"a,\n"), ("r", b"b\n")]);
    let (out, status, err) = run(&["-t,", "-a1", &paths[0], &paths[1]]);
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(out, "a,\n", "尾部空字段占最后一个槽");
    let (out, _, _) = run(&["-t,", "-o", "1.1,1.2", &paths[0], &paths[0]]);
    assert_eq!(out, "a,\n", "`-o` 指到那个空字段也是同形");
    let _ = fs::remove_dir_all(dir);
}

/// `-t ' '` treats only the space as a delimiter (C's `tabchar` is that one
/// byte), and tab is no longer a delimiter; the old form treated "the
/// delimiter is a space" as "the default set" and applied the empty-segment
/// skip too, getting both shapes wrong.
#[test]
fn dash_t_space_keeps_empties_and_leaves_tabs_alone() {
    let (dir, paths) = scratch("space", &[("l", b"a  b\n"), ("r", b"a  b\n")]);
    let (out, status, err) = run(&["-t ", "-j", "2", &paths[0], &paths[1]]);
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(out, " a b a b\n", "两个空格中间那个空段就是连接字段");
    let _ = fs::remove_dir_all(dir);

    let (dir, paths) = scratch("tab", &[("l", b"a\tb\n"), ("r", b"a\tb\n")]);
    let (out, _, err) = run(&["-t ", "-j", "2", &paths[0], &paths[1]]);
    assert_eq!(
        out, " a\tb a\tb\n",
        "制表符仍是内容：一整串是一个字段，{err:?}"
    );
    let _ = fs::remove_dir_all(dir);
}

// —— The five tests below pin the comparison layer to C's shape. C's `cmp`
// (`minix3/usr.bin/join/join.c:405-414`) has **three** answers: it first
// checks "the field number exceeds this line's field count", and only when
// neither side is out of range does it `strcmp`. The old form in this layer
// used `key()` to fold both "out of range" and "the field is the empty
// string" into the empty string, so it misjudged pairing and also threw off
// the grouping inside one file (C `slurp`'s `:398`). Golden samples were
// copied shape by shape from live measurement on the reference
// `tmp/family-diag-ref/join_ref`; the pre-change readings are recorded in
// `tmp/join-cmp-301/before.log`; still one test per shape.

/// A line with "no such column at all" is not the same as a line where "this
/// column is the empty string": `a,` has two fields (the second empty) and
/// `a` has only one, so when both sides join on field 2 C judges them unequal
/// and prints nothing, while the old shell printed `,a,a`.
#[test]
fn an_overfield_join_key_is_not_equal_to_an_empty_one() {
    let (dir, paths) = scratch("overfield-eq", &[("l", b"a,\n"), ("r", b"a\n")]);
    let (out, status, err) = run(&["-t,", "-1", "2", "-2", "2", &paths[0], &paths[1]]);
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(out, "", "越界的那侧不当成空串去匹配");
    let _ = fs::remove_dir_all(dir);
}

/// When the first side lacks the join field, C judges it **greater**
/// (`:409-410`'s `return 1`) → the second side goes first: the golden sample
/// prints `x 9` then `solo`. The old form folded the missing field into the
/// empty string, judged `"" < "9"` as the first side being smaller, and
/// reversed the two lines entirely.
#[test]
fn a_line_without_the_join_field_yields_the_lead_to_the_other_side() {
    let (dir, paths) = scratch("overfield-f1", &[("l", b"solo\n"), ("r", b"x 9\n")]);
    let (out, status, err) = run(&["-1", "2", "-2", "2", "-a1", "-a2", &paths[0], &paths[1]]);
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(out, "x 9\nsolo\n", "有字段那一侧先出");
    let _ = fs::remove_dir_all(dir);
}

/// The other direction of the same branch: when the out-of-range side is the
/// second, C does `return -1` (`:411-412`) → the first side goes first. Only
/// the two tests together pin the ordering — the old form produced the **same
/// output** for both directions, which is precisely the evidence that it
/// could not tell them apart.
#[test]
fn the_side_that_has_the_field_leads_when_the_other_lacks_it() {
    let (dir, paths) = scratch("overfield-f2", &[("l", b"x 9\n"), ("r", b"solo\n")]);
    let (out, status, err) = run(&["-1", "2", "-2", "2", "-a1", "-a2", &paths[0], &paths[1]]);
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(out, "x 9\nsolo\n");
    let _ = fs::remove_dir_all(dir);
}

/// Grouping uses the same ruler: C's `slurp` compares a new line with the
/// previous one and closes the group when `cmp != 0` (`:398`). So the three
/// lines `a,,x`/`b`/`c,,y` are **three groups** (`b` lacks field 2), and only
/// the first pairs with the other side's `a,,x`; the old form merged the
/// three into one group and fabricated two pairings that do not exist.
#[test]
fn grouping_stops_at_a_line_that_lacks_the_join_field() {
    let (dir, paths) = scratch("grouping", &[("l", b"a,,x\nb\nc,,y\n"), ("r", b"a,,x\n")]);
    let (out, status, err) = run(&[
        "-t,", "-1", "2", "-2", "2", "-a1", "-a2", &paths[0], &paths[1],
    ]);
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(out, ",a,x,a,x\nb\nc,,y\n", "缺列那行自成一组");
    let _ = fs::remove_dir_all(dir);
}

/// This shape reads the same before and after the change (both equal to C),
/// so it enters as a regression guard: the grouping change must not break the
/// ordinary bucketing of "same key but different field counts" — in
/// `p 1`/`q`/`r 1`, `q` lacks field 2 and is reported on its own.
#[test]
fn the_mixed_field_counts_case_still_matches_the_reference() {
    let (dir, paths) = scratch("mixed-counts", &[("l", b"p 1\nq\nr 1\n"), ("r", b"p 1\n")]);
    let (out, status, err) = run(&["-1", "2", "-2", "2", "-a1", "-a2", &paths[0], &paths[1]]);
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(out, "1 p p\nq\nr 1\n");
    let _ = fs::remove_dir_all(dir);
}

// —— The four tests below pin the rejection family for `-o 0`. C's
// `fieldarg` only looks at whether the token's first character is `1`/`2`
// (`minix3/usr.bin/join/join.c:526-527`), so the branch where "`0` means the
// join field itself" exists in BSD/GNU but not in this tree's ground truth.
// The reference readings for all four shapes (separate, first in the list,
// last in the list, attached value) are `join: malformed -o option field`,
// exit 1, with no usage appended (the `errx` family's shape).

/// A separate `-o 0`: C falls into the `errx` family — one sentence only, no
/// usage after it, exit 1.
#[test]
fn a_lone_zero_output_field_is_refused_without_a_usage_block() {
    let (out, status, err) = run(&["-o", "0", "f1", "f2"]);
    assert_eq!(out, "", "报文只走 stderr");
    assert_eq!(status, 1, "C 的 `errx(1, …)` 退 1");
    assert_eq!(err, "join: malformed -o option field\n");
}

/// `0` at the start of the list is refused as well (C scans item by item; the
/// first item already hits the prefix guard).
#[test]
fn a_zero_first_in_the_output_list_is_refused() {
    let (out, status, err) = run(&["-o", "0,1.1", "f1", "f2"]);
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(err, "join: malformed -o option field\n");
}

/// `0` at the end of the list is refused too: C's loop does not let it
/// through just because valid items were already accepted.
#[test]
fn a_zero_last_in_the_output_list_is_refused() {
    let (out, status, err) = run(&["-o", "1.1,0", "f1", "f2"]);
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(err, "join: malformed -o option field\n");
}

/// The attached form `-o0` takes the same branch: `obsolete()`'s legacy
/// rewrite only applies to a token that is exactly a bare `-o`; the attached
/// form hands `0` straight to `fieldarg`, so the refusal sentence is identical
/// to the separate form.
#[test]
fn the_attached_dash_o_zero_is_refused_too() {
    let (out, status, err) = run(&["-o0", "f1", "f2"]);
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(err, "join: malformed -o option field\n");
}

/// The command seam for byte-form argv: `&str` cannot pass a raw `ff` — when
/// an `OsStr` comes from a `str` the string is already valid UTF-8, so this
/// fixture layer is necessary rather than convenient. Both output streams
/// come back **byte for byte**: `from_utf8_lossy` renders one `c3` as the
/// three-byte U+FFFD, and the text form cannot distinguish the "one byte" and
/// "three bytes" failure modes.
fn run_argv(args: &[&[u8]]) -> (Vec<u8>, i32, Vec<u8>) {
    use std::os::unix::ffi::OsStrExt as _;
    let output = Command::new(env!("CARGO_BIN_EXE_join"))
        .args(args.iter().map(|word| std::ffi::OsStr::from_bytes(word)))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("the join binary should be runnable");
    (
        output.stdout,
        output.status.code().unwrap_or(-1),
        output.stderr,
    )
}

/// Write one file under its **raw byte name**, and put its folded twin (the
/// name after `from_utf8_lossy`) on disk too: the open and the
/// `err(1, "%s", name)` entries need `&str` (registered under
/// `ARGV-BYTE-PATH-IN-LIBS`), and the twin lets "can this file be opened" and
/// "what the name slot echoes" be measured separately. Returns the raw-byte
/// full path.
fn write_raw(dir: &Path, name: &[u8], body: &[u8]) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt as _;
    let mut full = dir.to_str().expect("ascii dir").as_bytes().to_vec();
    full.push(b'/');
    full.extend_from_slice(name);
    fs::write(std::ffi::OsStr::from_bytes(&full), body).expect("write raw-named file");
    let folded = String::from_utf8_lossy(name).into_owned();
    fs::write(dir.join(&folded), body).expect("write the folded twin");
    full
}

/// On `join` the argument seam changes **judgment and output**, not just
/// messages — the first shell in this family of that kind:
/// (1) the `-t` value is judged by `strlen(optarg) != 1` (`join.c:202`) — on
///     the text seam a single raw `ff` is first folded into three bytes, so
///     the "one-byte delimiter" shape flips from **accepted** to **rejected**
///     (measured before the change: `join: illegal tab character
///     specification`, exit 1; the reference accepts it, exits 0, and really
///     uses it to split lines; three-way readings in
///     `tmp/join-463/before-after-smoke.log`);
/// (2) the `-e` value is printed verbatim into the vacant field by `outfield`
///     (`join.c:497-508`), and a folded string prints one `ff` as `ef bf bd`
///     (three-way readings in `tmp/join-463/before-after-smoke-2.log`).
/// The opposite shape `-t<é>` (`c3 a9`, two bytes of valid UTF-8) is
/// **rejected by all three sides**, pinning "what is checked is the byte
/// count, not decodability" — this shape is also the evidence that "a byte
/// seam does not mean everything is relaxed".
#[test]
fn a_raw_byte_delimiter_is_accepted_and_a_raw_byte_replacement_is_printed() {
    // (1) One raw `ff` as delimiter: accepted, splits lines, and the output
    //     delimiter is that same byte (`*tabchar`).
    let (dir, paths) = scratch("rawdelim", &[("l", b"a\xff1\nb\xff2\n"), ("r", b"a\xffx\n")]);
    let (out, status, err) = run_argv(&[
        b"-t\xff",
        paths[0].as_bytes(),
        paths[1].as_bytes(),
    ]);
    assert_eq!(status, 0, "单字节分隔符不该被拒：stderr 是 {err:?}");
    assert_eq!(
        out, b"a\xff1\xffx\n",
        "两侧按 `ff` 切出字段，展出的分隔符也是 `ff`"
    );
    assert_eq!(err, Vec::new(), "成功路一声不出");

    // (2) The opposite shape: `é` is two bytes (valid UTF-8), and C's test
    //     rejects it too — changing the seam does not relax this.
    let (_, status, err) = run_argv(&[b"-t\xc3\xa9", paths[0].as_bytes(), paths[1].as_bytes()]);
    assert_eq!(status, 1);
    assert_eq!(
        err,
        [
            b"join: illegal tab character specification\n".as_slice(),
            USAGE_FIRST.as_bytes(),
            USAGE_SECOND.as_bytes(),
        ]
        .concat(),
        "`strlen(optarg) != 1` 查的是字节数"
    );

    // (3) The `-e` value is output content: the vacant field receives the raw
    //     argv bytes.
    let (dir2, paths2) = scratch("rawempty", &[("l", b"a 1\nb 2\n"), ("r", b"a x\n")]);
    let (out, status, err) = run_argv(&[
        b"-a1",
        b"-o",
        b"1.1,2.2",
        b"-e\xffx",
        paths2[0].as_bytes(),
        paths2[1].as_bytes(),
    ]);
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(
        out,
        [b"a x\n".as_slice(), b"b \xffx\n".as_slice()].concat(),
        "`b` 那一行第二个字段来自 `-e`，必须是裸 `ff` 而不是 U+FFFD 的三字节"
    );

    // (4) The operand name slot: only the happy path is pinned — opening and
    //     the `err` sentences both sit on registered `&str` entries, so the
    //     folded twin is also placed on disk, ruling out "can the file be
    //     opened" as a cause of this reading failing.
    let (dir3, paths3) = scratch("rawname", &[("r", b"a x\n")]);
    let raw = write_raw(&dir3, b"l\xffr", b"a 1\n");
    let (out, status, err) = run_argv(&[raw.as_slice(), paths3[0].as_bytes()]);
    assert_eq!((out.as_slice(), status), (b"a 1 x\n".as_slice(), 0), "stderr 是 {err:?}");
    let _ = fs::remove_dir_all(dir);
    let _ = fs::remove_dir_all(dir2);
    let _ = fs::remove_dir_all(dir3);
}

/// The three branches that splice a value into the sentence all use the raw
/// argv bytes for the value slot: `illegal field number -- %s`
/// (`join.c:145`, `:156`, `:190`), `illegal file number -- %s` (`:176`,
/// `:222`), and `obsolete()`'s `illegal option -- %s` (`:592`, carrying the
/// whole token including the leading dash). The first two are `warnx`
/// followed by `usage()`, so three lines each; the third is `errx`, **one line
/// only** — line-count differences and value differences are each pinned in a
/// section of this one gate. The letter-slot test is pinned by
/// `bin_support_face`'s cross-shell table, and is not repeated here.
#[test]
fn the_three_value_echoes_carry_the_argv_bytes() {
    let (out, status, err) = run_argv(&[b"-1", b"2\xff", b"f1", b"f2"]);
    assert_eq!(out, Vec::new(), "报文只走标准错误");
    assert_eq!(status, 1);
    assert_eq!(
        err,
        [
            b"join: illegal field number -- 2\xff\n".as_slice(),
            USAGE_FIRST.as_bytes(),
            USAGE_SECOND.as_bytes(),
        ]
        .concat(),
        "值位是一枚 `ff`，不是折叠出的三字节"
    );

    let (_, status, err) = run_argv(&[b"-a1\xff", b"f1", b"f2"]);
    assert_eq!(status, 1);
    assert_eq!(
        err,
        [
            b"join: illegal file number -- 1\xff\n".as_slice(),
            USAGE_FIRST.as_bytes(),
            USAGE_SECOND.as_bytes(),
        ]
        .concat(),
        "文件号那支先由 `switch` 认下 1，再在尾字符上失手"
    );

    let (_, status, err) = run_argv(&[b"-j3\xff", b"f1", b"f2"]);
    assert_eq!(status, 1);
    assert_eq!(
        err,
        b"join: illegal option -- -j3\xff\n",
        "`obsolete()` 的畸形形带整个 token，且 `errx` 不接 usage"
    );
}
