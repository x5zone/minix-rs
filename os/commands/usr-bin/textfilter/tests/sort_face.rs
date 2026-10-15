//! End-to-end face for the `sort` binary: real process, real pipes and real files,
//! checked against `minix3/usr.bin/sort/sort.c` (its option loop, the operand
//! handling at `:266-290` and the output leg at `:320-379`).
//!
//! The output leg is the interesting part for a test: it does not just write a
//! file, it decides *how* — a fresh name is opened directly, an existing regular
//! file is replaced through a temporary whose name is never left behind, and an
//! unwritable one is refused before a single byte changes. All of that is
//! observable from outside the process, which is what these goldens assert.

use std::fs;
use std::io::Write as _;
use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const SORT: &str = env!("CARGO_BIN_EXE_sort");

fn run(args: &[&str], input: &[u8]) -> (Vec<u8>, i32, String) {
    let mut child = Command::new(SORT)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the sort binary should be runnable");
    child
        .stdin
        .as_mut()
        .expect("stdin piped")
        .write_all(input)
        .expect("writing standard input");
    drop(child.stdin.take());
    let output = child.wait_with_output().expect("the run should finish");
    (
        output.stdout,
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

fn written(tag: &str, bytes: &[u8]) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!("sort-face-{}-{tag}", std::process::id()));
    fs::write(&path, bytes).expect("writing the scratch file");
    path
}

fn text(path: &Path) -> String {
    path.to_str().unwrap().to_owned()
}

fn clean(path: &Path) {
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o644));
    let _ = fs::remove_file(path);
}

#[test]
fn without_the_flag_the_result_goes_to_standard_output() {
    let (out, status, err) = run(&[], b"c\na\nb\n");
    assert_eq!(out, b"a\nb\nc\n", "stderr was {err:?}");
    assert_eq!(status, 0);
    assert_eq!(err, "");
}

#[test]
fn a_name_that_does_not_exist_yet_is_written_directly() {
    // `lstat` fails, so C skips the temporary entirely and opens the name with "w"
    // (`sort.c:324-326`, `:352-355`).
    let target = written("fresh-target", b"placeholder");
    // Removing it is what puts the name in the state this branch cares about.
    fs::remove_file(&target).expect("the name must not exist");
    let input = written("fresh-input", b"c\na\nb\n");
    let (out, status, err) = run(&["-o", &text(&target), &text(&input)], b"");
    assert_eq!(out, Vec::<u8>::new(), "stdout must stay empty: {out:?}");
    assert_eq!(status, 0, "stderr was {err:?}");
    assert_eq!(fs::read(&target).expect("the output exists"), b"a\nb\nc\n");
    clean(&target);
    clean(&input);
}

#[test]
fn an_existing_file_is_replaced_through_a_temporary_and_keeps_its_mode() {
    // The branch that needs the temporary: the old file's mode is copied onto it
    // before the names swap (`sort.c:366`), and the temporary is unlinked after the
    // link, so nothing is left beside the target (`:372-379`).
    let target = written("existing-target", b"stale line\n");
    fs::set_permissions(&target, fs::Permissions::from_mode(0o640)).expect("chmod");
    // The inode is what tells the two branches apart from outside: the temporary
    // leaves the name on a fresh inode, while opening the existing name with "w"
    // would reuse the old one after truncating it.
    let old_inode = fs::metadata(&target).expect("metadata").ino();
    let input = written("existing-input", b"b\na\n");
    let directory = target.parent().expect("a parent directory");
    let before: Vec<String> = fs::read_dir(directory)
        .expect("readable temp dir")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(&format!("sort-face-{}-", std::process::id())))
        .collect();
    let (out, status, err) = run(&["-o", &text(&target), &text(&input)], b"");
    assert_eq!(out, Vec::<u8>::new());
    assert_eq!(status, 0, "stderr was {err:?}");
    assert_eq!(fs::read(&target).expect("the target"), b"a\nb\n");
    assert_eq!(
        fs::metadata(&target)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o7777,
        0o640,
        "the old file's permission bits must survive the replacement"
    );
    let written_meta = fs::metadata(&target).expect("metadata");
    assert_ne!(
        written_meta.ino(),
        old_inode,
        "an existing file must be replaced, not truncated"
    );
    let after: Vec<String> = fs::read_dir(directory)
        .expect("readable temp dir")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(&format!("sort-face-{}-", std::process::id())))
        .collect();
    let mut extra: Vec<&String> = after.iter().filter(|name| !before.contains(name)).collect();
    extra.sort();
    assert_eq!(
        extra,
        Vec::<&String>::new(),
        "a temporary name was left behind"
    );
    clean(&target);
    clean(&input);
}

#[test]
fn sorting_a_file_over_itself_works() {
    // Same name for input and output is the case the temporary exists for: the
    // operand is read first, and only then does the name move
    // (`sort.c:320-379` after the input loop at `:266-290`).
    let one = written("in-place", b"c\na\nb\n");
    let (out, status, err) = run(&["-o", &text(&one), &text(&one)], b"");
    assert_eq!(out, Vec::<u8>::new());
    assert_eq!(status, 0, "stderr was {err:?}");
    assert_eq!(fs::read(&one).expect("still readable"), b"a\nb\nc\n");
    clean(&one);
}

#[test]
fn an_unwritable_target_is_refused_before_anything_changes() {
    // `access(outpath, W_OK)`（`sort.c:336-337`）那一支在 C 是 `err(2, "%s", outpath)`：
    // 程序名、路径、`strerror` 句子三件齐备（旧写法只印到路径为止），且退 2。
    // 旧内容必须还在。
    let target = written("read-only", b"keep me\n");
    fs::set_permissions(&target, fs::Permissions::from_mode(0o444)).expect("chmod");
    let input = written("read-only-input", b"b\na\n");
    let (out, status, err) = run(&["-o", &text(&target), &text(&input)], b"");
    assert_eq!(out, Vec::<u8>::new());
    assert_eq!(status, 2, "stderr was {err:?}");
    assert_eq!(
        err,
        format!("sort: {}: Permission denied\n", text(&target)),
        "那句得把号对应的句子带上"
    );
    assert_eq!(fs::read(&target).expect("untouched"), b"keep me\n");
    clean(&target);
    clean(&input);
}

#[test]
fn the_check_mode_never_opens_the_output() {
    // `-c` runs `order()` and exits before the output branch exists
    // (`sort.c:316-319`), so a disorder report must not create the named file and
    // must not overwrite an existing one either.
    let target = written("check-target", b"untouched\n");
    let input = written("check-input", b"b\na\n");
    let (out, status, err) = run(&["-c", "-o", &text(&target), &text(&input)], b"");
    assert_eq!(out, Vec::<u8>::new());
    assert_eq!(status, 1, "stderr was {err:?}");
    assert!(err.contains("disorder"), "stderr was {err:?}");
    assert_eq!(fs::read(&target).expect("untouched"), b"untouched\n");
    clean(&target);
    clean(&input);
}

#[test]
fn operands_concatenate_and_a_missing_one_exits_two() {
    // Each operand is opened and read in argument order; standard input is then
    // ignored, because C only falls back to it when `optind == argc`
    // (`sort.c:306-312`). An unopenable operand is C's `access` failure at status 2
    // (`sort.c:284`).
    let one = written("operand-a", b"d\nb\n");
    let two = written("operand-b", b"c\n");
    let (out, status, err) = run(&[&text(&one), &text(&two)], b"ignored stdin\n");
    assert_eq!(out, b"b\nc\nd\n", "stderr was {err:?}");
    assert_eq!(status, 0);
    let missing = format!("sort-face-{}-missing", std::process::id());
    let (out, status, err) = run(&[&missing], b"");
    assert_eq!(out, Vec::<u8>::new());
    assert_eq!(status, 2, "stderr was {err:?}");
    assert!(err.contains(&missing), "stderr was {err:?}");
    clean(&one);
    clean(&two);
}

#[test]
fn a_lone_dash_names_standard_input() {
    // C rewrites a `-` operand to `/dev/stdin` (`sort.c:270-281`); this shell reads
    // the already-open standard input, which is the same bytes.
    let (out, status, err) = run(&["-"], b"c\na\n");
    assert_eq!(out, b"a\nc\n", "stderr was {err:?}");
    assert_eq!(status, 0);
    // `-o` with no value at all is C's `?` and lands on the usage path, status 2.
    let (out, status, err) = run(&["-o"], b"a\n");
    assert_eq!(out, Vec::<u8>::new(), "stderr was {err:?}");
    assert_eq!(status, 2, "stderr was {err:?}");
    // 两行：libc 的那一句在前（`getopt.c:132-136`，字母不带引号），整个 usage 块在后。
    // 旧壳只印一句自创的 `sort: bad option`。
    assert_eq!(
        err,
        format!("sort: option requires an argument -- o\n{USAGE}")
    );
}

/// C `usage()` 的那一行（`sort.c:405-410`）：两条 `fprintf` 拼成**一行**，第二条以
/// 十三个空格开头（从 `sort.o` 的 `.rodata` 数出来的）。这里逐字钉住，间距也算。
const USAGE: &str = "usage: sort [-bcdfHilmnrSsu] [-k field1[,field2]] [-o output] [-R char] [-T dir]             [-t char] [file ...]\n";

#[test]
fn an_unknown_letter_prints_the_libc_line_then_the_usage() {
    let (out, status, err) = run(&["-Z"], b"a\n");
    assert_eq!(out, Vec::<u8>::new());
    assert_eq!(status, 2);
    assert_eq!(err, format!("sort: unknown option -- Z\n{USAGE}"));
}

#[test]
fn a_bad_key_gets_its_own_sentence_without_the_usage() {
    // `setfield`/`setcolumn` 的三支 `errx` 都**不接 usage**（`init.c:186-200`），
    // 与上面那两支正好相反；把退出码与「有没有 usage 块」一起钉住才分得开。
    let (out, status, err) = run(&["-k", "x"], b"a\n");
    assert_eq!(out, Vec::<u8>::new());
    assert_eq!(status, 2);
    assert_eq!(err, "sort: illegal field descriptor\n");
    // 开始列后的残留不是 `\0` 也不是 `,` 就是这一支（`-k 1 2` 的空格）。
    assert_eq!(
        run(&["-k", "1 2"], b"a\n").2,
        "sort: illegal field descriptor\n"
    );
}

#[test]
fn a_second_delimiter_option_takes_the_message_plus_the_usage_block() {
    // `usage("multiple field delimiters")`（`sort.c:214-215`）：先 `%s: %s` 那一句，
    // 再接整个 usage 行。而多字节本身不是错（C 只取首字节）。
    let (out, status, err) = run(&["-t,", "-t;"], b"a\n");
    assert_eq!(out, Vec::<u8>::new());
    assert_eq!(status, 2);
    assert_eq!(err, format!("sort: multiple field delimiters\n{USAGE}"));
    let (out, status, err) = run(&["-t::", "-k", "1"], b"a:a\n");
    assert_eq!(out, b"a:a\n", "多字节分隔符取首字节，不该报错：{err:?}");
    assert_eq!(status, 0);
}
