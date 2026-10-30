//! End-to-end face for the `sort` binary: real process, real pipes and real files,
//! checked against `minix3/usr.bin/sort/sort.c` (its option loop, the operand
//! handling at `:266-290` and the output leg at `:320-379`).
//!
//! The output leg is the interesting part for a test: it does not just write a
//! file, it decides *how* — a fresh name is opened directly, an existing regular
//! file is replaced through a temporary whose name is never left behind, and an
//! unwritable one is refused before a single byte changes. All of that is
//! observable from outside the process, which is what these goldens assert.

mod face_support;

use std::fs;
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
    face_support::write_stdin(&mut child, input);
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

/// This process's **effective** uid (the second column of the `Uid:` line in
/// `/proc/self/status`). Returns `None` when unsure, letting the caller take the
/// ordinary-user branch.
///
/// This probe is not optional: the repository's authoritative compilation gate
/// runs inside docker `minix-ci:1.94`, and that image executes as root by
/// default (`docker run --rm minix-ci:1.94 id` → `uid=0(root)`). Root ignores
/// permission bits, so the "read-only target" gate below sees **two equally
/// valid** behaviors in the container and on the host.
fn effective_uid() -> Option<u32> {
    let status = fs::read_to_string("/proc/self/status").ok()?;
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("Uid:") {
            return rest.split_whitespace().nth(1).and_then(|v| v.parse().ok());
        }
    }
    None
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
    // The `access(outpath, W_OK)` (`sort.c:336-337`) branch is
    // `err(2, "%s", outpath)` in C: program name, path, and `strerror` sentence
    // all three present (the old spelling printed only up to the path), exiting
    // 2. The old content must still be there.
    //
    // **This expectation only holds for non-root**: a process with
    // `CAP_DAC_OVERRIDE` (root has it by default) ignores permission bits in
    // `access(2)`/`open` and is constrained only by file-system-level limits such
    // as read-only mounts. So the same C command run as root writes the result
    // and exits 0. The authoritative container gate runs as root, so both
    // branches pin their own shape—rather than skipping one of them.
    let target = written("read-only", b"keep me\n");
    fs::set_permissions(&target, fs::Permissions::from_mode(0o444)).expect("chmod");
    let input = written("read-only-input", b"b\na\n");
    let (out, status, err) = run(&["-o", &text(&target), &text(&input)], b"");
    assert_eq!(out, Vec::<u8>::new());
    if effective_uid() == Some(0) {
        // root: `access` admits → the write succeeds and the target content
        // becomes the sorted result.
        assert_eq!(status, 0, "root 下 access 放行，stderr 是 {err:?}");
        assert_eq!(err, "", "root 那一支不该发任何句子");
        assert_eq!(fs::read(&target).expect("written"), b"a\nb\n");
    } else {
        assert_eq!(status, 2, "stderr was {err:?}");
        assert_eq!(
            err,
            format!("sort: {}: Permission denied\n", text(&target)),
            "那句得把号对应的句子带上"
        );
        assert_eq!(fs::read(&target).expect("untouched"), b"keep me\n");
    }
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
    // Verbatim rather than `contains`: this one used to assert only
    // `contains("disorder")`, so it stayed green even when the shell invented its
    // own sentence `sort: disorder: `. Now tightened to the exact bytes.
    assert_eq!(
        err, "sort: found disorder: a\n",
        "那句得逐字对位 `msort.c:403`"
    );
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
    // Two lines: libc's sentence first (`getopt.c:132-136`, the letter
    // unquoted), the whole usage block after. The old shell printed only an
    // invented `sort: bad option`.
    assert_eq!(
        err,
        format!("sort: option requires an argument -- o\n{USAGE}")
    );
}

/// The one line of C `usage()` (`sort.c:405-410`): two `fprintf`s joined into
/// **one line**, the second starting with thirteen spaces (counted from
/// `sort.o`'s `.rodata`). Pinned verbatim here, spacing included.
const USAGE: &str = "usage: sort [-bcdfHilmnrSsu] [-k field1[,field2]] [-o output] [-R char] [-T dir]             [-t char] [file ...]\n";

/// The same command seam as `run`, but handing standard error back **byte for
/// byte** too: `from_utf8_lossy` renders a bare `c3` as the three-byte U+FFFD,
/// and the textual form cannot tell "one byte" from "two bytes".
fn run_streams(args: &[&str], input: &[u8]) -> (Vec<u8>, i32, Vec<u8>) {
    let mut child = Command::new(SORT)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the sort binary should be runnable");
    face_support::write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        output.stdout,
        output.status.code().unwrap_or(-1),
        output.stderr,
    )
}

/// The letter slot of the out-of-set line is **one raw byte**. This branch has
/// two sources of absolute readings: at the mechanism level, a Minix `getopt.c`
/// probe compiled for this work (`tmp/sort-411/probe.c`, run with the option
/// string from `sort.c:148`, returns `optopt=303`, i.e. `c3`, and it reports
/// **each of é's two bytes once**—the real `sort.c:250-252`
/// `case '?': default: usage(NULL);` exits on the first, so the message is only
/// one line); and fifteen reference builds of the same family
/// (`tmp/rev-309/rev_ref`, `tmp/paste-321/paste_ref`, `tmp/cmp-410/cmp_ref`,
/// etc.) return a single `c3` for `-é` consistently. `l` is a legal, value-less
/// letter in the option string, so the cluster keeps consuming and eats the
/// first byte; `-Zé` stops at `Z`. The ASCII shape already has its own gate
/// (`an_unknown_letter_prints_the_libc_line_then_the_usage`), so this test does
/// not carry it twice.
#[test]
fn the_libc_line_of_a_multibyte_flag_is_one_byte_for_sort() {
    let want: Vec<u8> = [
        b"sort: unknown option -- ".as_slice(),
        b"\xc3\n",
        USAGE.as_bytes(),
    ]
    .concat();
    for form in ["-\u{e9}", "-l\u{e9}"] {
        let (out, status, err) = run_streams(&[form], b"a\n");
        assert_eq!(out, Vec::new(), "{form} 的报文只走标准错误");
        assert_eq!(status, 2, "{form} 的 stderr 是 {err:?}");
        assert_eq!(err, want, "{form} 的字母该是一个裸字节 c3");
    }

    let (_out, status, err) = run_streams(&["-Z\u{e9}"], b"a\n");
    assert_eq!(status, 2);
    assert_eq!(
        err,
        [b"sort: unknown option -- Z\n".as_slice(), USAGE.as_bytes(),].concat(),
        "环停在第一个坏字母上，第二个字节根本不该被读到"
    );
}

#[test]
fn an_unknown_letter_prints_the_libc_line_then_the_usage() {
    let (out, status, err) = run(&["-Z"], b"a\n");
    assert_eq!(out, Vec::<u8>::new());
    assert_eq!(status, 2);
    assert_eq!(err, format!("sort: unknown option -- Z\n{USAGE}"));
}

#[test]
fn a_bad_key_gets_its_own_sentence_without_the_usage() {
    // The five `errx` arms of `setfield`/`setcolumn` **do not append usage**
    // (`init.c:160-216`), the exact opposite of the two above; only pinning the
    // exit status together with "is there a usage block" tells them apart. The
    // five sentences have a single source in `sort.rs:KeySpecError::sentence`,
    // and which shapes use which sentence was measured shape by shape against
    // the reference build (`tmp/family-diag-ref/sort_ref`, linked from the real
    // `minix3/usr.bin/sort/` sources): the old spelling squeezed the five into
    // one `illegal field descriptor`, so four of them signed with someone else's
    // sentence.
    for (spec, sentence) in [
        ("x", "field numbers must be positive"),
        ("0", "field numbers must be positive"),
        ("1,-2", "field numbers must be positive"),
        ("1.0", "illegal offset"),
        ("1,0.5", "cannot indent end of line"),
        ("1 2", "illegal field descriptor"),
        ("1n2", "illegal field descriptor"),
        ("2.5,1.2", "fields out of order"),
    ] {
        let (out, status, err) = run(&["-k", spec], b"a\n");
        assert_eq!(out, Vec::<u8>::new(), "键形 {spec:?} 不该打任何东西");
        assert_eq!(status, 2, "键形 {spec:?} 的 stderr 是 {err:?}");
        assert_eq!(
            err,
            format!("sort: {sentence}\n"),
            "键形 {spec:?} 该走那一句，而且不接 usage"
        );
    }
    // The reverse direction must be pinned too: the shapes C accepts must not
    // report any sentence. `-k 2,1` does not report `fields out of order` (with
    // no `.C` on the end column the indent is 0, so the `tcol.indent != 0` test
    // at `:214` does not hold); `-k 1,2.0` does not report `illegal offset` (the
    // decrement happens only on the start column, `:166-167`). The old ledger
    // recorded `-k 2,1` as "fields out of order in C"; this pass overturned that
    // with the reference build.
    for spec in ["2,1", "1,2.0", "1,2.", "1,0", "1b", "+1", "01"] {
        let (out, status, err) = run(&["-k", spec], b"a\n");
        assert_eq!(status, 0, "键形 {spec:?} 的 stderr 是 {err:?}");
        assert_eq!(err, "", "键形 {spec:?} 不该报句子");
        assert_eq!(out, b"a\n", "键形 {spec:?}");
    }
}

#[test]
fn the_length_flag_sorts_by_span_length() {
    // `-l` sorts by the "span byte count" after skipping leading blanks (C
    // `fields.c`'s `length()`), not by content. The four readings below were
    // measured on the same input with the reference build
    // `tmp/family-diag-ref/sort_ref`: empty line (0), `b 2` (1), `cc 3` (2),
    // `aaa 1` (3). Neither lexicographic nor numeric order yields this sequence.
    let input = b"aaa 1\nb 2\ncc 3\n\n";
    let (out, status, err) = run(&["-l"], input);
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(out, b"\nb 2\ncc 3\naaa 1\n");
    // The local `-k 1l` and the global `-l` give the same order; `-l` with `-r`
    // is its reverse.
    assert_eq!(run(&["-k", "1l"], input).0, out);
    assert_eq!(run(&["-l", "-r"], input).0, b"aaa 1\ncc 3\nb 2\n\n");
    // When `l` and `n` are both given, length wins (`enterfield` checks `L`
    // before `N`): on the reference build, `-k 1ln` and `-k 1nl` read exactly the
    // same as `-k 1l`.
    assert_eq!(run(&["-k", "1ln"], input).0, out);
    assert_eq!(run(&["-k", "1nl"], input).0, out);
    // Padding does not count toward length: under `-t:`, the first field of the
    // first line `  aa:1` is `  aa`, which is 2 after skipping the two blanks, so
    // it sorts after `b:2`, whose length is 1.
    let delimited = b"  aa:1\nb:2\nccc:3\n";
    let (out, status, err) = run(&["-t:", "-k", "1l"], delimited);
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(
        out, b"b:2\n  aa:1\nccc:3\n",
        "参照件给的序就是 b:2、  aa:1、ccc:3"
    );
    // The reverse case: without `-l` the same input sorts lexicographically, and
    // the padded `  aa:1` comes first instead.
    assert_eq!(run(&["-t:", "-k", "1"], delimited).0, delimited);
}

#[test]
fn numeric_sort_normalizes_signs_and_zeros_the_way_c_does() {
    // C's `number()` eats leading zeros, only steps over a leading plus, and
    // backs up trailing fraction zeros together with the terminator, so these
    // shapes are **equal-valued**—equal value means tied keys, and tied keys
    // keep input order. The readings were measured pair by pair with the
    // reference build `sort_ref -n` (both input directions come out in input
    // order). The old spelling recognized only a minus sign and did not
    // normalize trailing fraction zeros, so `+7` looked like "no number was
    // converted" and fell to zero, splitting the `007`/`7`/`+7` lines into two
    // keys—at that time the whole-line fallback made it look right.
    for input in [
        b"007\n7\n".to_vec(),
        b"7\n007\n".to_vec(),
        b"+7\n7\n".to_vec(),
        b"1.5\n1.50\n".to_vec(),
        b"1.50\n1.5\n".to_vec(),
        b".5\n0.5\n".to_vec(),
        b"+0\n-0\n".to_vec(),
        b"12.0\n12\n".to_vec(),
        b"  12\n12\n".to_vec(),
    ] {
        let (out, status, err) = run(&["-n"], &input);
        assert_eq!(status, 0, "stderr 是 {err:?}");
        assert_eq!(out, input, "{input:?} 两行同值，必须保持输入序");
    }
    // Normalization must not lose magnitude: after dropping zeros it still
    // compares by digit count and digit by digit.
    let (out, _, err) = run(&["-n"], b"7\n6\n");
    assert_eq!(out, b"6\n7\n", "stderr 是 {err:?}");
    let (out, _, err) = run(&["-n"], b"0007\n0006\n");
    assert_eq!(
        out, b"0006\n0007\n",
        "前导零归一后仍要比出量级，stderr 是 {err:?}"
    );
    // C does not recognize exponents: the key of `1e3` is 1 (it stops at `e`),
    // so it sorts before `1000`.
    let (out, _, err) = run(&["-n"], b"1000\n1e3\n");
    assert_eq!(out, b"1e3\n1000\n", "参照件同读数，stderr 是 {err:?}");
    // A shape with no digit at all counts as zero (C's comment says so
    // explicitly: "\"0\", \"0.00\", \"000\" or \"fubar\" but sorts as 0").
    let (out, _, err) = run(&["-n"], b"abc\n0\n");
    assert_eq!(
        out, b"abc\n0\n",
        "两者都是零键，保持输入序，stderr 是 {err:?}"
    );
}

#[test]
fn equal_keys_keep_input_order_and_capital_s_switches_to_the_record() {
    // The tie rule (measured shape by shape against the reference build
    // `sort_ref`). All four input lines have equal keys (the second column), so
    // the four spellings give four readings, each matching only its own branch:
    let input = b"b 1\na 1\nc 1\nd 1\n";
    // Default: C does not re-sort by the whole record (`fields.c:168` trims
    // `keylen` to the key alone when `posix_sort` is zero), so tied keys keep
    // input order—the old spelling fell back to the whole line here and printed
    // a b c d.
    assert_eq!(run(&["-k", "2,2"], input).0, input.to_vec());
    // `-S` is what "sort by key, then by the whole record" means
    // (`sort.c:202-212`).
    assert_eq!(run(&["-S", "-k", "2,2"], input).0, b"a 1\nb 1\nc 1\nd 1\n");
    // A global `-r` reverses the stable order along with everything else (the
    // reference build's `-r -k 2,2` gives d c a b).
    assert_eq!(run(&["-r", "-k", "2,2"], input).0, b"d 1\nc 1\na 1\nb 1\n");
    // `-u` clears `-S` **after** the option loop (`:256-258`), so both shapes
    // keep only the first line; the old spelling always broke ties by the whole
    // line and kept all four—the opposite of `sort -u`.
    assert_eq!(run(&["-u", "-k", "2,2"], input).0, b"b 1\n");
    assert_eq!(run(&["-S", "-u", "-k", "2,2"], input).0, b"b 1\n");
    assert_eq!(run(&["-u", "-S", "-k", "2,2"], input).0, b"b 1\n");
    // `-c` uses the same measuring stick: by default the four lines are not out
    // of order (exit 0, nothing printed), while with `-S` enabled `a 1` after
    // `b 1` is disorder (the reference build does
    // `errx(1, "found disorder: %s", …)`, `msort.c:403`, exit 1). The wording is
    // now pinned verbatim against the reference build, down to "which
    // program-name prefix + which word"; see the dedicated gate below.
    let (out, status, err) = run(&["-c", "-k", "2,2"], input);
    assert_eq!(out, Vec::<u8>::new());
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(err, "");
    let (out, status, err) = run(&["-c", "-S", "-k", "2,2"], input);
    assert_eq!(out, Vec::<u8>::new(), "失序检查不该往标准输出打东西");
    assert_eq!(status, 1, "stderr 是 {err:?}");
    assert_eq!(err, "sort: found disorder: a 1\n");
}

#[test]
fn the_two_check_reports_use_c_sentences_and_one_line_each() {
    // The two `-c` sentences are verbatim C's `msort.c:order()` (the reference
    // build `sort_ref` reads the same): disorder is `found disorder: <the later
    // one>` (`:403`), and an equal key under `-u` is `found non-uniqueness: <the
    // later one>` (`:405-407`). Both exit 1, both append no usage, and both
    // print **one line** only (C writes the record's trailing separator as `\0`
    // before printing, so the closing newline is `errx`'s own).
    let (out, status, err) = run(&["-c"], b"b\na\n");
    assert_eq!(out, Vec::<u8>::new());
    assert_eq!(status, 1, "stderr 是 {err:?}");
    assert_eq!(err, "sort: found disorder: a\n");
    // Neither sentence is a usage error: the whole stderr is that one sentence,
    // and not a single word of the usage block may follow.
    assert_eq!(err.matches('\n').count(), 1, "stderr 是 {err:?}");
    let (out, status, err) = run(&["-c", "-u"], b"a\na\nb\n");
    assert_eq!(out, Vec::<u8>::new());
    assert_eq!(status, 1, "stderr 是 {err:?}");
    assert_eq!(err, "sort: found non-uniqueness: a\n");
    // Order: C checks disorder before duplication within the same comparison, so
    // `b b a` reports the pair it hits first (the duplicate `b`), not the later
    // disorder—the reference build reads the same.
    let (_, status, err) = run(&["-c", "-u"], b"b\nb\na\n");
    assert_eq!(
        (status, err.as_str()),
        (1, "sort: found non-uniqueness: b\n")
    );
    // The reverse case: without `-u`, the same input must not emit the
    // "non-uniqueness" sentence (exit 0).
    let (out, status, err) = run(&["-c"], b"a\na\nb\n");
    assert_eq!(out, Vec::<u8>::new());
    assert_eq!(
        (status, err.as_str()),
        (0, ""),
        "相等键在无 -u 时既不失序也不报错"
    );
}

#[test]
fn a_second_delimiter_option_takes_the_message_plus_the_usage_block() {
    // `usage("multiple field delimiters")` (`sort.c:214-215`): the `%s: %s`
    // sentence first, then the whole usage line. A multibyte delimiter is not an
    // error by itself (C takes only the first byte).
    let (out, status, err) = run(&["-t,", "-t;"], b"a\n");
    assert_eq!(out, Vec::<u8>::new());
    assert_eq!(status, 2);
    assert_eq!(err, format!("sort: multiple field delimiters\n{USAGE}"));
    let (out, status, err) = run(&["-t::", "-k", "1"], b"a:a\n");
    assert_eq!(out, b"a:a\n", "多字节分隔符取首字节，不该报错：{err:?}");
    assert_eq!(status, 0);
}

// —— The following six gates pin `-R` (record delimiter) and its mutual
// exclusion with `-t`. The goldens were copied shape by shape from current
// measurements of the reference build `tmp/family-diag-ref/sort_ref` (the real
// `minix3/usr.bin/sort/` sources + shims): `-R ,` splits the input into records
// at commas, and the last segment keeps its own newline; the output is "one
// delimiter per record", so it **ends with a delimiter** (`putrec` at
// `files.c:194`/`:215`).

/// Record splitting and tail shape: `b,a,d,c\n` splits into `b`/`a`/`d`/`c\n`,
/// and after sorting each record gets a comma appended.
#[test]
fn dash_r_splits_records_on_the_given_byte_and_ends_the_output_with_it() {
    let (out, status, err) = run(&["-R", ","], b"b,a,d,c\n");
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(out, b"a,b,c\n,d,", "最后一条记录带着它内部的换行");
}

/// A trailing delimiter does not leave an empty record (C's `getrec` returns 0
/// when it reads zero bytes before EOF).
#[test]
fn a_trailing_record_delimiter_does_not_make_an_extra_record() {
    let (out, status, err) = run(&["-R", ","], b"b,a,d,");
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(out, b"a,b,d,");
    let (out, status, _) = run(&["-R", ","], b"");
    assert_eq!(
        (out.as_slice(), status),
        (b"".as_ref(), 0),
        "空输入一条记录也没有"
    );
}

/// A single-byte value is a **literal byte**: `-R 0` splits on the character
/// `0`, not a zero byte; the `\NNN` form goes octal.
#[test]
fn dash_r_single_byte_is_literal_while_the_backslash_form_is_octal() {
    let (out, status, err) = run(&["-R", "0"], b"a0b0c\n");
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(out, b"a0b0c\n0");
    let (out, status, err) = run(&["-R", "\\011"], b"b,a,d,c\n");
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(
        out, b"b,a,d,c\n\t",
        "整串只有一个制表符分隔的记录，输出以制表收尾"
    );
}

/// `-u` and `-c` both operate on the same records, so de-duplication also works
/// on split records.
#[test]
fn dash_u_dedups_records_not_lines() {
    let (out, status, err) = run(&["-R", ",", "-u"], b"a,a,b\n");
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(out, b"a,b\n,");
}

/// Two `errx` sentences (`sort.c:217-218`, `:233-238`): **only that one line**,
/// no usage appended, exit 2. The `invalid` arm carries the `optarg` C holds at
/// that moment—the backslash prefix has already been skipped.
#[test]
fn the_clash_and_invalid_legs_print_one_line_and_skip_the_usage_block() {
    let (out, status, err) = run(&["-R", ",", "-t", ","], b"a\n");
    assert_eq!(out.as_slice(), b"");
    assert_eq!(status, 2);
    assert_eq!(err, "sort: record/field delimiter clash\n");
    // No reverse check: only the `-t` arm reads the mask. The reference build
    // gives `a\n,b,` for this shape: the newline is just an ordinary byte here,
    // so it **stays in the record content**, and the output ends with the record
    // delimiter. The first version wrote the expectation as `a,b,\n` from the
    // intuition of "commas separate two lines", and both this gate and the
    // reference build rejected it.
    let (out, status, err) = run(&["-t", ",", "-R", ","], b"b,a\n");
    assert_eq!(
        (out.as_slice(), status),
        (b"a\n,b,".as_ref(), 0),
        "stderr 是 {err:?}"
    );
    let (_, status, err) = run(&["-R", "300"], b"a\n");
    assert_eq!(status, 2);
    assert_eq!(err, "sort: invalid record delimiter 300\n");
    let (_, status, err) = run(&["-R", "\\q"], b"a\n");
    assert_eq!(status, 2);
    assert_eq!(err, "sort: invalid record delimiter q\n");
    let (_, status, err) = run(&["-t", "\n"], b"a\n");
    assert_eq!(status, 2, "默认记录分隔符就是换行");
    assert_eq!(err, "sort: record/field delimiter clash\n");
}

/// `multiple record delimiters` goes through `usage()`, so it is **two lines**
/// (one sentence with the program name + the usage block). The reference build
/// crashes when it reaches this point on the host (its own `usage()` depends on
/// a shim that segfaults on the host, the same trap as `join_ref`), so this
/// gate's shape is pinned from the C source text: the first `fprintf` at
/// `sort.c:399-412` has no newline, and the second starts with thirteen spaces.
#[test]
fn a_second_stuck_dash_r_lands_on_the_usage_leg() {
    let (out, status, err) = run(&["-R", ",", "-R", "x"], b"a\n");
    assert_eq!(out.as_slice(), b"", "报文只走 stderr");
    assert_eq!(status, 2, "C 的 usage() 是 exit(2)");
    assert!(
        err.starts_with("sort: multiple record delimiters\nusage: sort "),
        "实际 stderr 是 {err:?}"
    );
    assert!(
        err.contains("[-R char]"),
        "usage 块里要带得上 `-R` 那一项：{err:?}"
    );
}

/// A command seam with byte-form argv: `&str` cannot pass a bare `ff`. This
/// shell's `run_streams` takes `&[&str]`, so the two shapes "a bare byte as a
/// letter" and "a bare byte as a value" could never be pinned end to end—this
/// pass adds them.
fn run_argv(args: &[&[u8]], input: &[u8]) -> (Vec<u8>, i32, Vec<u8>) {
    use std::os::unix::ffi::OsStrExt as _;
    let mut child = Command::new(SORT)
        .args(args.iter().map(|word| std::ffi::OsStr::from_bytes(word)))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the sort binary should be runnable");
    face_support::write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        output.stdout,
        output.status.code().unwrap_or(-1),
        output.stderr,
    )
}

/// Everything `sort`'s argument seam changed is **the value itself**: `-t`
/// takes only the first byte (`d_mask[(u_char)*optarg]` at `sort.c:219`), `-R`
/// takes the first byte as the record delimiter (`:226`) and then looks at the
/// rest to decide the numeric form or an error (`:228-238`), the error sentence
/// carrying `optarg`'s original bytes; and the `-t` arm immediately checks
/// `d_mask[byte] & REC_D_F` (`:217-218`). Below the text seam, a bare `ff` first
/// folds to `ef bf bd`, and all four places go wrong together—the worst being a
/// **false clash**: `-R\xef -t\xff` are two different bytes in C (the reference
/// build exits 0), but after one fold both sides become `ef` and the
/// `record/field delimiter clash` sentence is triggered out of thin air.
/// Byte-for-byte readings match the reference build; see `tmp/sort-467/ab.log`.
#[test]
fn the_delimiter_and_record_delimiter_values_are_argv_bytes() {
    // ① The same raw byte on `-R`'s **character path** gives C -1 (a signed
    //    `char` handed to `int REC_D`), which neither collides with `-t`'s mask
    //    slot nor ever matches an input byte ⇒ accepted, and the tail still adds
    //    that `ff` (`putchar(-1)` writes `0xff`). The reference build has the
    //    same shape; see `tmp/sort-467/ab.log`.
    let (out, status, err) = run_argv(&[b"-R\xff", b"-t\xff"], b"x\n");
    assert_eq!(status, 0, "字符路径的 ff 不算冲突：stderr 是 {err:?}");
    assert_eq!(out, b"x\n\xff");
    // ① The same 255 on the **numeric path** (`-R\377` through `strtol` yields
    //    the positive 255) lands in `d_mask[255]`, colliding with `-t`'s raw byte
    //    ⇒ the clash sentence (one line, no usage, exit 2).
    let (out, status, err) = run_argv(&[b"-R\\377", b"-t\xff"], b"x\n");
    assert_eq!(out, Vec::new(), "冲突发生在任何输出之前");
    assert_eq!(status, 2, "stderr 是 {err:?}");
    assert_eq!(err, b"sort: record/field delimiter clash\n");
    // ① Control shape: the same ASCII byte still clashes (`-R,` and `-t,`).
    let (_out, status, err) = run_argv(&[b"-R,", b"-t,"], b"x\n");
    assert_eq!(status, 2, "stderr 是 {err:?}");
    assert_eq!(err, b"sort: record/field delimiter clash\n");

    // ② Discriminating shape: two different bytes must not clash—one fold would
    //    make them equal, so this shape pins "neither loosened nor tightened".
    let (out, status, err) = run_argv(&[b"-R\xef", b"-t\xff"], b"x\n");
    assert_eq!(status, 0, "两枚不同的字节各自成立：stderr 是 {err:?}");
    assert_eq!(out, b"x\n\xef", "记录按 ef 切，收尾补的也是那一枚 ef");

    // ③ The echo slot of a numeric-form failure: both bytes of `q\xff` are argv
    //    originals (only the backslash-prefix arm skips one first).
    let (out, status, err) = run_argv(&[b"-Rq\xff"], b"");
    assert_eq!(out, Vec::new());
    assert_eq!(status, 2, "stderr 是 {err:?}");
    assert_eq!(err, b"sort: invalid record delimiter q\xff\n");

    // ④ A record delimiter that never matches: the whole input is **one**
    //    record, and the output still ends with that byte (`putrec`,
    //    `files.c:194`/`:215`). The shape whose content carries a raw byte
    //    (`-R<ff>` with `a\xffb`) is handed back as `a\xef\xbf\xbdb\n\xff` by
    //    this shell while the reference build returns `a\xffb\n\xff`—the
    //    difference is that **record content is a `String`** (`split_records`
    //    goes through `from_utf8_lossy`); that is a different scope, logged and
    //    counted as a known divergence in the diff.
    let (out, status, err) = run_argv(&[b"-R\xff", b"-"], b"a\nb\n");
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(out, b"a\nb\n\xff", "不切分，只在末尾补那一枚分隔符");

    // ⑤ The letter slot is a raw byte that **cannot be passed through &str**:
    //    the libc line reports itself, followed by the `usage()` line
    //    (`sort.c:250-252` + `getopt.c:101-104`), exit 2.
    let (_out, status, err) = run_argv(&[b"-\xff"], b"a\n");
    assert_eq!(status, 2);
    assert_eq!(
        err.split(|&byte| byte == b'\n').next().unwrap(),
        b"sort: unknown option -- \xff"
    );
}

/// The operand's **name slot** is raw bytes here: that C arm is
/// `err(2, "%s", argv[i])` (`sort.c:285`), taking `err(3)`'s three-part form
/// (program name, operand, `strerror` sentence). This shell used to hand it to
/// `support::warn_line` → `minix_sys::err_line(&str, …)`, which folded the raw
/// byte to U+FFFD and printed
/// `sort: /tmp/nosuch-…\xef\xbf\xbd: No such file or directory`; this pass
/// assembles the three parts itself, matching the reference build byte for byte
/// (the raw-byte file-name shape in `tmp/sort-467/ab.log`).
/// The open step still has to fold first (`minix_sys::open`'s `&str` door, a
/// registered follow-up), so "an actually existing raw-byte file name" cannot
/// be opened by this shell—that shape is a known divergence and is not pinned
/// by this gate.
#[test]
fn the_operand_message_names_the_operand_bytes_and_exits_two() {
    let (out, status, err) = run_argv(&[b"/tmp/nosuch-sort-\xff"], b"a\n");
    assert_eq!(out, Vec::new(), "开不了的档名不出任何输出");
    assert_eq!(status, 2, "stderr 是 {err:?}");
    assert_eq!(
        err, b"sort: /tmp/nosuch-sort-\xff: No such file or directory\n",
        "三段齐备、名字位是原字节，且不接 usage"
    );
    // Control shape: an ASCII file name goes through the same leg, sentence
    // unchanged.
    let (out, status, err) = run_argv(&[b"/tmp/nosuch-sort-ok.txt"], b"a\n");
    assert_eq!(out, Vec::new());
    assert_eq!(status, 2);
    assert_eq!(
        err,
        b"sort: /tmp/nosuch-sort-ok.txt: No such file or directory\n"
    );
    // Two unopenable file names emit **only the first one's sentence**: C does
    // `err(2, "%s", argv[i])`, and `err` terminates the process itself—it does
    // not report each and then exit. The reference build's current measurement
    // agrees (the second name never appears).
    let (out, status, err) = run_argv(&[b"/tmp/nosuch-a\xff", b"/tmp/nosuch-b\xff"], b"a\n");
    assert_eq!(out, Vec::new());
    assert_eq!(status, 2);
    assert_eq!(
        err, b"sort: /tmp/nosuch-a\xff: No such file or directory\n",
        "第一枚即终止，第二枚不留名字"
    );
}
