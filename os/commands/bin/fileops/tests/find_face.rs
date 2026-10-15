//! End-to-end face for the `find` binary: real process, real pipes, checked against
//! `minix3/usr.bin/find/{main.c,option.c,function.c,operator.c}`.
//!
//! `find`'s parse-side reports are all `errx(1, …)` lines: program name from
//! `getprogname()`, C's phrase, and **no** `strerror` sentence (C never consulted
//! `errno` there). Until this batch the shell carried its own invented wording — e.g.
//! ``find: unknown option `-nope'`` where C says ``find: -nope: unknown option``
//! (`option.c:131`), and an invented `find: (\` unmatched` where C says
//! `find: (: missing closing ')'` (`operator.c:96-98`). Every line here is therefore
//! checked byte-for-byte against the C source, including the punctuation C chose.
//!
//! These legs need no filesystem, so all of them are reachable on a host build —
//! while the walk warnings (`find.c:230`'s `warnx` with `strerror`) used to be the
//! exception: the probe trait answered `Option` and dropped the number. Since batch 274
//! the probes hand it over, and this door pins the *shape* of that line (program name,
//! path, `strerror` sentence) — but only the shape: a hosted `find` cannot read a real
//! directory, because `getdents` goes over the VFS IPC (`minix-sys`'s `getdents_via`)
//! and there is no VFS server here, so the number the host produces is `EIO`. Which
//! sentence a genuine `EACCES`/`ENOENT` produces is pinned at the unit level
//! (`find.rs`'s walk tests, synthetic probe) and on the machine leg.

use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;
use std::process::{Command, Stdio};

fn run_at(program: &Path, args: &[&str]) -> (String, i32, String) {
    let output = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("the find binary should be runnable");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

fn run(args: &[&str]) -> (String, i32, String) {
    run_at(Path::new(env!("CARGO_BIN_EXE_find")), args)
}

/// One `errx`-shaped report: exactly the line C prints, exit 1, nothing on stdout.
fn assert_errx_line(args: &[&str], expected: &str) {
    let (out, status, err) = run(args);
    assert_eq!(out, "", "`{args:?}` 的报错路不写标准输出");
    assert_eq!(status, 1, "`errx(1, …)` 是退 1：`{args:?}`");
    assert_eq!(err, format!("{expected}\n"), "报文逐字对 C");
}

#[test]
fn the_usage_line_carries_the_program_name_and_a_capital_usage() {
    // `main.c:159-163`: `fprintf(stderr, "Usage: %s [-H | -L | -P] [-dEhsXx] [-f
    // file] file [file ...] [expression]\n", getprogname())`. Capital `U`, name
    // inside the line, and it fires when no starting path is given (`:147-149`).
    assert_errx_line(
        &[],
        "Usage: find [-H | -L | -P] [-dEhsXx] [-f file] file [file ...] [expression]",
    );
    assert_errx_line(
        &["-print"],
        "Usage: find [-H | -L | -P] [-dEhsXx] [-f file] file [file ...] [expression]",
    );
}

#[test]
fn an_unknown_option_puts_the_name_first_with_no_backquotes() {
    // `option.c:131`: `errx(1, "%s: unknown option", *argv)`.
    assert_errx_line(&[".", "-nope"], "find: -nope: unknown option");
}

#[test]
fn a_missing_option_argument_names_the_option() {
    // `option.c:134`: `errx(1, "%s: requires additional arguments", *--argv)`.
    assert_errx_line(
        &[".", "-name"],
        "find: -name: requires additional arguments",
    );
}

#[test]
fn the_exec_terminator_line_quotes_both_terminators() {
    // `function.c:647-651`: the scan gives up on `!*ap` with
    // `errx(1, "%s: no terminating \";\" or \"+\"", isok ? "-ok" : "-exec")` — the
    // `-ok` spelling is C's too, because the `+` here does not follow `{}`.
    assert_errx_line(
        &[".", "-exec", "echo"],
        "find: -exec: no terminating \";\" or \"+\"",
    );
    assert_errx_line(
        &[".", "-ok", "echo", "+"],
        "find: -ok: no terminating \";\" or \"+\"",
    );
}

#[test]
fn plus_after_braces_is_permitted_for_exec_and_refused_for_ok() {
    // `function.c:666-669`: `errx(1, "-ok: terminating \"+\" not permitted.")` —
    // note the trailing period C writes, and that `-exec … {} +` is legal.
    assert_errx_line(
        &[".", "-ok", "echo", "{}", "+"],
        "find: -ok: terminating \"+\" not permitted.",
    );
    // C accepts `-exec … {} +` (it sets `F_PLUSSET` and goes on), so the run must not
    // produce a parse-side refusal for it. The host walk itself fails here, so the
    // exit status is the traversal's answer, not this leg's: check the line instead.
    let (_, _, err) = run(&[".", "-exec", "echo", "{}", "+"]);
    assert!(
        !err.contains("no terminating") && !err.contains("not permitted"),
        "`-exec … {{}} +` 是被收下的形状，不该出报文：`{err}`"
    );
}

#[test]
fn the_paren_lines_are_c_phrases_not_invented_ones() {
    // Three separate C lines, three separate shapes:
    // `operator.c:96-98` `err(1, "(: missing closing ')'")`, `operator.c:106-108`
    // `errx(1, "(): empty inner expression")`, `operator.c:149`
    // `errx(1, "): no beginning '('")`. The first is `err` in C — its sentence would
    // be whatever `errno` the last call left, so the shell stops at the phrase and
    // says so in its header rather than inventing a number.
    assert_errx_line(&[".", "(", "-print"], "find: (: missing closing ')'");
    assert_errx_line(&[".", "(", ")"], "find: (): empty inner expression");
    assert_errx_line(&[".", "-print", ")"], "find: ): no beginning '('");
}

#[test]
fn a_bang_without_an_operand_uses_c_phrase() {
    // `operator.c:197-199`: `errx(1, "!: no following expression")`.
    assert_errx_line(&[".", "!"], "find: !: no following expression");
}

#[test]
fn every_line_follows_argv_zero_like_getprogname_does() {
    // The prefix is C's `getprogname()` (seeded by `crt0-common.c:156-165`), so a
    // copy under another name moves all of them — including the usage line, which
    // embeds the name inside it (`Usage: %s …`) rather than prefixing it.
    let copied = std::env::temp_dir().join(format!("find-face-copy-{}", std::process::id()));
    fs::copy(Path::new(env!("CARGO_BIN_EXE_find")), &copied).expect("copy the binary");
    fs::set_permissions(&copied, fs::Permissions::from_mode(0o755)).expect("make it run");
    let name = copied.file_name().unwrap().to_str().unwrap().to_string();

    let (_, _, err) = run_at(&copied, &[]);
    assert_eq!(
        err,
        format!("Usage: {name} [-H | -L | -P] [-dEhsXx] [-f file] file [file ...] [expression]\n")
    );

    let (_, _, err) = run_at(&copied, &[".", "-nope"]);
    assert_eq!(err, format!("{name}: -nope: unknown option\n"));

    let _ = fs::remove_file(&copied);
}

/// 宿主能钉的是**行形**：`warnx` 族先打 `getprogname() + ": "`（`vwarnx.c`），再是
/// `<路径>: <strerror 句子>`。走表告警此前漏了程名前缀（同壳的解析错 `errx` 行都带了），
/// 本批补上。至于号本身：宿主的 `getdents` 走 VFS IPC（`minix-sys::getdents_via`），
/// 这里没有 VFS 服务，所以真跑一次只能拿到 `EIO` —— 这一形在宿主上恰恰证明
/// 「报的是号对应的句子，不是编造的短语」，而 `EACCES` 那类真号在单元测与机器腿各自钉。
#[test]
fn a_walk_warning_line_carries_the_program_name_path_and_sentence() {
    let dir = std::env::temp_dir().join(format!("find-face-warn-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create the walk root");

    let (out, status, err) = run(&[dir.to_str().unwrap()]);
    let _ = fs::remove_dir_all(&dir);

    assert_eq!(
        err,
        format!("find: {}: Input/output error\n", dir.display()),
        "走表告警必须是 `find: <路径>: <strerror 句子>`；stdout 为 {out:?}"
    );
    assert_eq!(
        status, 1,
        "读不下目次 = C 的 `FTS_DNR`，`rval` 置 1（`find.c:232`）"
    );
}

/// C 的 `c_not` 与 `c_or` 各管两句，四支各有自己的报文（`operator.c:198-201`、`:253-259`）。
/// 旧实现把 `!` 的两支拢成一句、`-o` 的两支干脆不报（三种坏形状一路走到求值），所以这里钉的是
/// 「哪一句」而不是「有没有报错」——退码由 `assert_errx_line` 统一按 `errx(1, …)` 断言。
#[test]
fn the_two_operator_pairs_refuse_with_their_own_sentences() {
    assert_errx_line(&[".", "!"], "find: !: no following expression");
    assert_errx_line(
        &[".", "!", "-o", "-print"],
        "find: !: nothing between ! and -o",
    );
    assert_errx_line(&[".", "-o", "-print"], "find: -o: no expression before -o");
    assert_errx_line(&[".", "-print", "-o"], "find: -o: no expression after -o");
    // C 的 `c_not` 先把连续 `!` 消合并（`while (node->type == N_NOT)`）再看类型，
    // 所以 `! ! -or` 落的是同一句「之间没东西」。
    assert_errx_line(
        &[".", "!", "!", "-or", "-print"],
        "find: !: nothing between ! and -o",
    );
    // C 的先查左后查右：只写一个 `-o` 时不能报成「after」那一句。
    assert_errx_line(&[".", "-o"], "find: -o: no expression before -o");
}
