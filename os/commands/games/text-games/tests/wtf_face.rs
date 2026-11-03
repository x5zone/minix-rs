//! End-to-end face for the `wtf` binary, checked against
//! `minix3/games/wtf/wtf` (note the ground truth is a POSIX shell script, not C:
//! the `file` reading and the criterion differences it brings are recorded in
//! `rewrite-notes/18-stage-commands/todo.md`).
//!
//! This set of gates was added later: `wtf` previously had **no end-to-end gate
//! at all**, so two defects that made it completely ineffective against the real
//! database went unnoticed — ① the open leg hard-coded `minix_sys::open`, so a file
//! that really exists on the host still reported `cannot open`; ② record splitting
//! tokenized on colons, while of the 557 lines in the real
//! `minix3/share/misc/acronyms` only 1 contains a colon (the `$NetBSD:` tag line)
//! and entries have the tab form `10Q\tthank you`. The in-crate gate that reads the
//! real database (`the_shipped_acronym_database_is_readable_and_resolves` in
//! `acronym.rs`) and the first process-level gate below each pin one layer.
//!
//! The write leg is **not pinned here**: `wtf` itself has no write-failure
//! handling logic; the message and the exit code come from the shell that
//! interprets it (on the host, dash measures as "one `echo: I/O error` line plus
//! exit 0"), which is not an interface this shell can align with; that account
//! lives in the conclusion of `TEXTGAMES-EMIT-ERRLEG`.

use std::path::PathBuf;
use std::process::{Command, Stdio};

fn wtf_bin() -> &'static str {
    env!("CARGO_BIN_EXE_wtf")
}

fn fixture(tag: &str, bytes: &[u8]) -> PathBuf {
    let path = std::env::temp_dir().join(format!("minix-wtf-face-{tag}.db"));
    std::fs::write(&path, bytes).expect("write wtf fixture");
    path
}

fn run(args: &[&str]) -> (String, i32, String) {
    let out = Command::new(wtf_bin())
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn wtf")
        .wait_with_output()
        .expect("wait wtf");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        out.status.code().expect("exit code"),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// The hit shape: the database opens, the entry resolves, "term: definition" is
/// printed, exit 0, stderr empty.
/// This gate also guards defects ① and ② — a regression in either turns it red.
#[test]
fn a_real_database_resolves_a_term_and_exits_zero() {
    let db = fixture(
        "hit",
        b"NASA\tNational Aeronautics and Space Administration\n",
    );
    let (out, code, err) = run(&["-f", &db.to_string_lossy(), "nasa"]);
    assert_eq!(
        (out.as_str(), code, err.as_str()),
        (
            "NASA: National Aeronautics and Space Administration\n",
            0,
            ""
        ),
        "脚本 `:75` 先把查询词转大写再输出，打印形态跟着大写化的 target 走"
    );
    let _ = std::fs::remove_file(&db);
}

/// Case-insensitive lookup is this shell's established rule (the script uppercases
/// the query term before comparing, which is indistinguishable in this shape).
#[test]
fn the_lookup_ignores_case() {
    let db = fixture("case", b"Y2K\tYear 2000 problem\n");
    for (term, want) in [("y2k", "Y2K"), ("Y2K", "Y2K")] {
        let (out, code, _) = run(&["-f", &db.to_string_lossy(), term]);
        assert_eq!(
            (out.as_str(), code),
            (
                format!("{}: Year 2000 problem\n", term.to_ascii_uppercase()).as_str(),
                0
            ),
            "词条 {want}"
        );
    }
    let _ = std::fs::remove_file(&db);
}

/// An unopenable database: the precheck (C `wtf:64-69`) answers the
/// script's own sentence `wtf: cannot open acronym database file
/// \`<path>'`, exit 1 — the two layers are aligned since batch 393.
/// C's precheck is `[ ! -f $f ]` per file (`wtf:64-69`) — a directory
/// and a device node fail it exactly like a missing path, and a real
/// regular file passes through to the lookup.
#[test]
fn the_database_precheck_demands_a_regular_file() {
    let db = format!("/tmp/minix-wtf-face-db-{}", std::process::id());
    std::fs::write(&db, b"LOL laughing out loud\n").expect("write the fixture");
    for path in ["/tmp", "/dev/null"] {
        let (out, code, err) = run_env(&["-f", path, "lol"], &[]);
        assert_eq!(out, "", "{path}");
        assert_eq!(code, 1, "{path}");
        assert_eq!(
            err,
            format!("wtf: cannot open acronym database file `{path}'\n"),
            "{path}"
        );
    }
    let (out, code, err) = run_env(&["-f", db.as_str(), "lol"], &[]);
    assert_eq!(out, "LOL: laughing out loud\n");
    // A hit leaves the run's status at zero (`wtf:107`'s `exit $rv`).
    assert_eq!((code, err.as_str()), (0, ""));
    let _ = std::fs::remove_file(&db);
}

#[test]
fn a_missing_database_names_the_path_and_exits_one() {
    let db = std::env::temp_dir().join("minix-wtf-face-definitely-not-here.db");
    let _ = std::fs::remove_file(&db);
    let (out, code, err) = run(&["-f", &db.to_string_lossy(), "nasa"]);
    assert_eq!(out, "", "失败时不该往标准输出写东西");
    assert_eq!(
        err,
        format!(
            "wtf: cannot open acronym database file `{}'\n",
            db.display()
        ),
        "脚本 `:67` 的原句：反引号与单引号包住档名"
    );
    assert_eq!(code, 1);
}

/// Unknown term: exit 1 with the sentence on stderr (same shape as the script: the
/// sentence goes to `1>&2` and `rv=1`).
#[test]
fn an_unknown_term_exits_one_with_the_sentence_on_stderr() {
    let db = fixture(
        "miss",
        b"NASA\tNational Aeronautics and Space Administration\n",
    );
    let (out, code, err) = run(&["-f", &db.to_string_lossy(), "zzznotaword"]);
    assert_eq!(out, "");
    assert_eq!(
        err, "wtf: I don't know what `zzznotaword' means!\n",
        "脚本 `:105` 印操作数原样（大写化只用在命中那支），走 stderr 且 rv=1"
    );
    assert_eq!(code, 1);
    let _ = std::fs::remove_file(&db);
}

/// No operand: the usage sentence goes to **standard output** and the exit is 1
/// (the script's `usage()` is an `echo` to stdout).
#[test]
fn no_operand_prints_usage_and_exits_one() {
    let (out, code, err) = run(&["-f", "/dev/null"]);
    // The script's usage() echoes to **standard output** and then exits 1
    // (`:11-14`; measured: stderr empty).
    assert_eq!(
        out, "usage: wtf [-o] [-f dbfile] [is] term ...\n",
        "usage 走 stdout"
    );
    assert_eq!(err, "", "stderr 该是空的");
    assert_eq!(code, 1, "usage 那支退 1");
}

/// The script does exactly one thing with `is` (`minix3/games/wtf/wtf:32-34`):
/// `if [ "$1" = "is" ]; then shift; fi` — it removes the **leading** one, once,
/// comparing a literal. Every token after that is a query term, including any `is`
/// that appears later. All four shapes' expectations are measured from
/// `sh minix3/games/wtf/wtf -f minix3/share/misc/acronyms <shape>`:
/// `is lol` → one hit line, exit 0; `lol is` → one hit line plus one miss for `is`,
/// exit 1; `is is lol` → only the first is removed, the second is still looked up
/// (one miss plus one hit line); `-- is lol` → the `is` after the option loop is
/// removed as well. The old spelling skipped `is` at **any position** inside the
/// query loop, so the last three shapes each lost a message line and the exit code
/// differed too.
#[test]
fn only_the_leading_is_word_is_dropped() {
    let db = fixture("isword", b"LOL\tlaughing out loud\n");
    let db = db.to_string_lossy().into_owned();

    let (out, code, err) = run(&["-f", &db, "is", "lol"]);
    assert_eq!(out, "LOL: laughing out loud\n");
    assert_eq!(err, "");
    assert_eq!(code, 0, "开头那枚 is 被移走，剩下的照查");

    let (out, code, err) = run(&["-f", &db, "lol", "is"]);
    assert_eq!(
        out, "LOL: laughing out loud\n",
        "尾部的 is 不影响前面命中那行"
    );
    assert_eq!(err, "wtf: I don't know what `is' means!\n");
    assert_eq!(code, 1, "尾部 is 是个查询词，查无就把退码置 1");

    let (out, code, err) = run(&["-f", &db, "is", "is", "lol"]);
    assert_eq!(out, "LOL: laughing out loud\n");
    assert_eq!(
        err, "wtf: I don't know what `is' means!\n",
        "只移走开头那一枚"
    );
    assert_eq!(code, 1);

    let (out, code, err) = run(&["-f", &db, "--", "is", "lol"]);
    assert_eq!(out, "LOL: laughing out loud\n");
    assert_eq!(err, "");
    assert_eq!(code, 0, "`--` 之后的 is 仍是开头那个操作数");
}

/// Removing that `is` compares a **literal** (`[ "$1" = "is" ]`), not a
/// case-insensitive comparison: `IS` is an ordinary query term. Measured: `IS lol`
/// → one ``I don't know what `IS' means!`` sentence plus one
/// `LOL: laughing out loud` line, exit 1. The old spelling's database-side
/// criterion `is_skipped_word` was case-insensitive, so this shape lost a message
/// line and the exit code differed.
#[test]
fn the_dropped_is_word_is_matched_case_sensitively() {
    let db = fixture("iscase", b"LOL\tlaughing out loud\n");
    let (out, code, err) = run(&["-f", &db.to_string_lossy(), "IS", "lol"]);
    assert_eq!(out, "LOL: laughing out loud\n");
    assert_eq!(err, "wtf: I don't know what `IS' means!\n");
    assert_eq!(code, 1);
    let _ = std::fs::remove_file(&db);
}

/// A lone `is`: after removal there is no operand, so `usage()` runs (the
/// `[ -z "$1" ]` at script `:36-38`). Measured: usage goes to **stdout**, exit 1,
/// stderr empty.
#[test]
fn a_lone_is_word_leaves_no_operand_and_prints_usage() {
    let db = fixture("islone", b"LOL\tlaughing out loud\n");
    let (out, code, err) = run(&["-f", &db.to_string_lossy(), "is"]);
    assert_eq!(out, "usage: wtf [-o] [-f dbfile] [is] term ...\n");
    assert_eq!(err, "");
    assert_eq!(code, 1);
    let _ = std::fs::remove_file(&db);
}

/// On the entry side the case is compared **literally**: C first uppercases the
/// operand (the `tr '[a-z]' '[A-Z]'` at script `:75`), then compares that
/// uppercased form against the database line's start (the `fgrep -h` plus
/// `sed "\|^$target[[:space:]]*||"` at `:76-77`). Hence entries whose database line
/// starts with a lowercase letter are never reachable — measured
/// (`ACRONYMDB=<db> sh minix3/games/wtf/wtf <shape>`, with database lines
/// `abc<TAB>lower term entry` and `DEF<TAB>upper`): querying `abc` and querying
/// `ABC` both fall into "I don't know what …" with exit 1, while querying `def`
/// hits and prints `DEF: upper`. The old spelling compared entries in the database
/// file with `eq_ignore_ascii_case`, so the first two shapes would wrongly report
/// a hit.
#[test]
fn a_lowercase_database_term_is_not_reachable_end_to_end() {
    let db = fixture("termcase", b"abc\tlower term entry\nDEF\tupper\n");
    let db = db.to_string_lossy().into_owned();

    for query in ["abc", "ABC"] {
        let (out, code, err) = run(&["-f", &db, query]);
        assert_eq!(out, "", "查 {query} 不该有命中行");
        assert_eq!(err, format!("wtf: I don't know what `{query}' means!\n"));
        assert_eq!(code, 1);
    }

    let (out, code, err) = run(&["-f", &db, "def"]);
    assert_eq!(out, "DEF: upper\n", "查询侧的大写化照旧生效");
    assert_eq!(err, "");
    assert_eq!(code, 0);
}

/// As above but with environment control: half of `wtf`'s database list comes from
/// `ACRONYMDB` (script `:47-53`). The parent's value must not leak in, so every
/// shape writes it explicitly to the wanted value or to the empty string.
fn run_env(args: &[&str], vars: &[(&str, &str)]) -> (String, i32, String) {
    let mut command = Command::new(wtf_bin());
    command
        .args(args)
        .env("ACRONYMDB", "")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, value) in vars {
        command.env(key, value);
    }
    let out = command
        .spawn()
        .expect("spawn wtf")
        .wait_with_output()
        .expect("wait wtf");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        out.status.code().expect("exit code"),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// The database is a **list**, and each `-f` is prepended to the **front** (the
/// `acronyms="$OPTARG $acronyms"` at script `:21`): `-f alpha -f beta` searches
/// beta first. Measured with `LOL` in both databases
/// (`sh minix3/games/wtf/wtf -f alpha -f beta lol`) → `LOL: from beta`, newline,
/// `from alpha` — **only the first line carries the `LOL: ` prefix**, because
/// `:80` is `echo "$target: $ans"` and `$ans` is a multi-line string.
#[test]
fn the_database_list_is_prepended_and_its_extra_answers_print_bare() {
    let alpha = fixture("list-alpha", b"LOL\tfrom alpha\nAAA\tonly alpha\n");
    let beta = fixture("list-beta", b"LOL\tfrom beta\nBBB\tonly beta\n");
    let (out, code, err) = run(&[
        "-f",
        &alpha.to_string_lossy(),
        "-f",
        &beta.to_string_lossy(),
        "lol",
    ]);
    assert_eq!(
        out, "LOL: from beta\nfrom alpha\n",
        "beta 先、alpha 后，第二行裸出"
    );
    assert_eq!(err, "");
    assert_eq!(code, 0);

    // Terms present in only one database are still found (the list is "all files as
    // one stream", not just the first file)
    let (out, code, _err) = run(&[
        "-f",
        &alpha.to_string_lossy(),
        "-f",
        &beta.to_string_lossy(),
        "aaa",
    ]);
    assert_eq!(out, "AAA: only alpha\n");
    assert_eq!(code, 0);
    let (out, code, _err) = run(&[
        "-f",
        &alpha.to_string_lossy(),
        "-f",
        &beta.to_string_lossy(),
        "bbb",
    ]);
    assert_eq!(out, "BBB: only beta\n");
    assert_eq!(code, 0);
}

/// Duplicate entries in one database also produce multi-line answers (the
/// `fgrep -h` at script `:76` does not stop at the first one): measured with
/// `-f <db with two LOL lines> lol` → `LOL: first match`, newline, `second match`.
#[test]
fn a_duplicated_entry_prints_every_line() {
    let db = fixture("dup", b"LOL\tfirst match\nLOL\tsecond match\n");
    let (out, code, err) = run(&["-f", &db.to_string_lossy(), "lol"]);
    assert_eq!(out, "LOL: first match\nsecond match\n");
    assert_eq!(err, "");
    assert_eq!(code, 0);
}

/// Each file in the list goes through the existence check **first** (script
/// `:64-69` is a separate loop); if any file cannot be opened, the message is
/// reported for **that** file and the process exits 1 on the spot — **later good
/// files are neither checked nor searched for hits**. Measured:
/// `-f <db with LOL> -f <missing> lol` and the reverse order both yield only that
/// one sentence.
#[test]
fn an_unreadable_entry_in_the_list_stops_before_any_lookup() {
    let good = fixture("precheck", b"LOL\tfrom good\n");
    let missing = "/tmp/minix-wtf-face-definitely-absent.db";
    let (out, code, err) = run(&["-f", &good.to_string_lossy(), "-f", missing, "lol"]);
    assert_eq!(out, "", "前置检查没过就不该有任何命中行");
    assert_eq!(
        err,
        format!("wtf: cannot open acronym database file `{missing}'\n"),
        "报的是那一档自己"
    );
    assert_eq!(code, 1);

    // Same shape reversed: a bad file first behaves the same, and a good file first
    // does not make it exit 0
    let (out, code, err) = run(&["-f", missing, "-f", &good.to_string_lossy(), "lol"]);
    assert_eq!(out, "");
    assert_eq!(
        err,
        format!("wtf: cannot open acronym database file `{missing}'\n")
    );
    assert_eq!(code, 1);
}

/// The semantics of `ACRONYMDB` (script `:47-53`): it has a place only when **not
/// a single `-f` was given**, and the whole string is split on whitespace into a
/// list (after `acronyms="$ACRONYMDB"`, `for f in $acronyms`), so runs of spaces
/// and trailing spaces do not matter. Measured:
/// `ACRONYMDB="alpha  beta " sh … bbb` → hits the beta file. Once any `-f` was
/// given, `ACRONYMDB` is **ignored wholesale**:
/// `ACRONYMDB=<db with LOL> sh … -f /etc/hostname lol` falls into the miss path.
#[test]
fn the_environment_names_the_list_only_when_no_flag_was_given() {
    let alpha = fixture("env-alpha", b"LOL\tfrom alpha\nAAA\tonly alpha\n");
    let beta = fixture("env-beta", b"LOL\tfrom beta\nBBB\tonly beta\n");
    let both = format!("{}  {} ", alpha.display(), beta.display());

    let (out, code, err) = run_env(&["bbb"], &[("ACRONYMDB", &both)]);
    assert_eq!(out, "BBB: only beta\n", "整串断词后第二档也在列表里");
    assert_eq!(err, "");
    assert_eq!(code, 0);

    // One `-f` displaces the environment string (querying `aaa` would hit the alpha
    // file from the environment; here the lookup must miss)
    let (out, code, err) = run_env(
        &["-f", &beta.to_string_lossy(), "aaa"],
        &[("ACRONYMDB", &both)],
    );
    assert_eq!(out, "");
    assert_eq!(err, "wtf: I don't know what `aaa' means!\n");
    assert_eq!(code, 1);

    // The empty string counts as absent (`[ -z "$ACRONYMDB" ]`); the glob
    // branch applies (`wtf:43-55`). On a host build the directory read
    // fails — the shell's unmatched-glob shape — so the literal pattern
    // passes through and the pre-pass fails naming the star. This is the
    // one-character divergence the batch-395 run measured, now closed.
    let (_out, code, err) = run_env(&["lol"], &[("ACRONYMDB", "")]);
    assert_eq!(code, 1, "本机没有缺省库档时该报开不了");
    assert_eq!(
        err, "wtf: cannot open acronym database file `/usr/share/misc/acronyms*'\n",
        "字面通配式带星号（C 的 unmatched-glob 形），实际 {err:?}"
    );
}
