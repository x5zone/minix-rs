//! End-to-end face for the `uniq` binary: real process, real standard input and
//! files, checked against expectations derived from `minix3/usr.bin/uniq/uniq.c`
//! (NetBSD) — not from the host's GNU `uniq`, which differs at two documented
//! sites (its `-c` prefix is `%7d`, and it rejects the old-style skip forms this
//! shell folds the way C's `obsolete()` does). Where the two agree, a second
//! test compares the outputs directly, so a regression in the shared part cannot
//! hide behind a stale golden value.

mod face_support;

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};

const UNIQ: &str = env!("CARGO_BIN_EXE_uniq");

/// Runs the binary with the given words and standard input, returning
/// `(stdout, exit status, stderr)`.
fn run(args: &[&str], input: &str) -> (String, i32, String) {
    run_with(UNIQ, args, input)
}

/// The same run for any program name, so the differential leg can drive the host's
/// `uniq` through the identical pipe and comparison.
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

/// A path inside a private temporary directory, so the file-operand cases cannot
/// collide with another test or a previous run.
fn scratch(name: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!("uniq-face-{}-{}", std::process::id(), name));
    path
}

#[test]
fn plain_collapse_is_one_copy_per_run() {
    let (out, status, err) = run(&[], "a\na\nb\n");
    assert_eq!(out, "a\nb\n");
    assert_eq!(status, 0);
    assert_eq!(err, "");
}

#[test]
fn counted_prefix_is_four_columns_wide() {
    // `show()` prints `"%4d %s"`, so the count occupies four columns before the
    // single separating blank. GNU `uniq -c` uses seven columns; that difference
    // is why this file pins the C shape rather than the host's.
    assert_eq!(run(&["-c"], "a\na\nb\n").0, "   2 a\n   1 b\n");
    assert_eq!(run(&["-c"], "x\n".repeat(12).as_str()).0, "  12 x\n");
}

#[test]
fn duplicates_and_uniques_flags_are_independent_bits() {
    // C keeps `cflag`/`dflag`/`uflag` as three separate ints and composes them in
    // one guard: `if ((dflag && repeats == 0) || (uflag && repeats > 0)) return;`
    assert_eq!(run(&["-d"], "a\na\nb\n").0, "a\n");
    assert_eq!(run(&["-u"], "a\na\nb\n").0, "b\n");
    assert_eq!(run(&["-c", "-d"], "a\na\nb\n").0, "   2 a\n");
    assert_eq!(run(&["-c", "-u"], "a\na\nb\n").0, "   1 b\n");
    // `-d -u` suppresses every run, and nothing is an exit status of zero here —
    // `uniq.c` returns 0 whatever it printed.
    let (out, status, err) = run(&["-d", "-u"], "a\na\nb\n");
    assert_eq!(out, "");
    assert_eq!(status, 0);
    assert_eq!(err, "");
}

#[test]
fn skipped_key_compares_whole_line_prints() {
    // `skip()` walks `numfields` blank-delimited fields, then `numchars` bytes;
    // the printed line is always the whole line (`show(ofp, prevline)`).
    let three = "foo SAME\nbar SAME\nbaz OTHER\n";
    assert_eq!(run(&["-f", "1"], three).0, "foo SAME\nbaz OTHER\n");
    assert_eq!(run(&["-s", "4"], three).0, "foo SAME\nbaz OTHER\n");
    // `-f 2` walks past the second field, which is the last one on every line, so
    // each key is the bare newline: all three lines share it and one run is
    // reported, as the first full line (`show(ofp, prevline)`).
    assert_eq!(run(&["-f", "2"], three).0, "foo SAME\n");
}

#[test]
fn obsolete_forms_fold_into_the_skip_options() {
    // `obsolete()` rewrites `-<digit>…` to `-f<digit>…` and `+<digit>…` to
    // `-s<digit>…`; GNU `uniq` does not take these forms.
    let three = "foo SAME\nbar SAME\nbaz OTHER\n";
    assert_eq!(run(&["-1"], three).0, run(&["-f", "1"], three).0);
    assert_eq!(run(&["+4"], three).0, run(&["-s", "4"], three).0);
}

#[test]
fn only_the_operand_count_takes_a_bare_usage() {
    // 第三个操作数走的是 `uniq.c:124` 的 `default: usage()`：选项环早已跑完，
    // 所以前面**没有** libc 那一行。
    const USAGE: &str = "usage: uniq [-cdu] [-f fields] [-s chars] [input_file [output_file]]\n";
    for args in [&["a", "b", "c"][..], &["-c", "a", "b", "c"][..]] {
        let (out, status, err) = run(args, "a\n");
        assert_eq!(out, "", "args {args:?} must print nothing on stdout");
        assert_eq!(status, 1, "args {args:?} must exit 1");
        assert_eq!(err, USAGE, "args {args:?}");
    }
}

#[test]
fn unknown_option_and_missing_value_take_two_lines() {
    // Minix3 的 `uniq` 没有 `-i` 这个字母（选项串是 `"-cdf:s:u"`）。两行：
    // `getopt.c:101-104` 的那一句（字母不带引号，与本机 glibc 不同形）加 usage。
    const USAGE: &str = "usage: uniq [-cdu] [-f fields] [-s chars] [input_file [output_file]]\n";
    let (out, status, err) = run(&["-i"], "a\n");
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(err, format!("uniq: unknown option -- i\n{USAGE}"));
    // 缺值同理（`getopt.c:132-136`）：`-f` 与 `-s` 各自的字母。
    for letter in ['f', 's'] {
        let word = format!("-{letter}");
        let (out, status, err) = run(&[&word], "a\n");
        assert_eq!(out, "", "{word} 不该打任何东西");
        assert_eq!(status, 1);
        assert_eq!(
            err,
            format!("uniq: option requires an argument -- {letter}\n{USAGE}"),
            "`-{letter}` 缺值那一支"
        );
    }
    // 不带值的旗标 `-c` 本身就是合法选项，不该走 usage（与上面两支的分界）。
    assert_eq!(
        run(&["-c"], "a\na\n"),
        (String::from("   2 a\n"), 0, String::new())
    );
}

#[test]
fn the_two_skip_values_get_their_own_sentences_with_the_bad_value_echoed() {
    // C 这里是 `errx(1, "illegal field skip value: %s", optarg)` 与它的 `-s` 同胞
    //（`uniq.c:90-98`）：自己的句子、带坏值、**没有 usage**。
    for (args, message) in [
        (&["-f", "x"][..], "uniq: illegal field skip value: x\n"),
        (&["-s", "x"][..], "uniq: illegal character skip value: x\n"),
        (&["-f-1"][..], "uniq: illegal field skip value: -1\n"),
        (
            &["-s", "5x"][..],
            "uniq: illegal character skip value: 5x\n",
        ),
    ] {
        let (out, status, err) = run(args, "a\n");
        assert_eq!(out, "", "{args:?} 不该打任何东西");
        assert_eq!(status, 1, "{args:?} 的 stderr 是 {err:?}");
        assert_eq!(err, message, "{args:?}");
    }
    // `-f -1` 不同：那个 `-1` 会先被 `obsolete()` 折叠成 `-f1`，所以带回去的是
    // `-f1`（宿主参照件同读数）。
    let (out, status, err) = run(&["-f", "-1"], "a\n");
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(err, "uniq: illegal field skip value: -f1\n");
}

#[test]
fn an_empty_or_padded_skip_value_is_a_count_not_an_error() {
    // C 的那道检查只有两条：转出一个负数，或 `*p` 还有残留（`uniq.c:91`、`:97`）。
    // `strtol` 一个数字都没转出来时值为 0 且 `*p` 停在数字起点，所以 `-f ''` 是
    // 合法的零（跳 0 个字段，等价于不给 `-f`），而前导空白会被跳掉。
    let three = "foo SAME\nbar SAME\nbaz OTHER\n";
    let plain = run(&["-f", "0"], three).0;
    assert_eq!(plain, run(&["-f", ""], three).0, "空值就是零");
    assert_eq!(plain, run(&["-f", " 0"], three).0, "前导空白跳掉");
    assert_eq!(run(&["-f+0"], three).0, plain, "附值形可带一个正号");
    assert_ne!(plain, run(&["-f", "1"], three).0, "零与一真的不一样");
    // 而分开写的 `+0` 不走到 `strtol`：C 的 `obsolete()` 先把它折成 `-s0`，于是
    // `-f` 拿到的值是 `-s0`，转不出数字就报 errx（宿主参照件同读数：
    // `uniq_ref: illegal field skip value: -s0`）。
    let (out, status, err) = run(&["-f", "+0"], three);
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(err, "uniq: illegal field skip value: -s0\n");
}

#[test]
fn file_operand_and_output_file_match_stdout_only_run() {
    let input = scratch("in");
    let output = scratch("out");
    fs::write(&input, "a\na\nb\n").unwrap();
    fs::write(&output, "stale content longer than the report\n").unwrap();
    let input_text = input.to_str().unwrap().to_owned();
    let output_text = output.to_str().unwrap().to_owned();

    let (out, status, err) = run(&[&input_text, &output_text], "");
    assert_eq!(out, "", "the report belongs in the output file");
    assert_eq!(status, 0, "stderr was {err:?}");
    assert_eq!(fs::read_to_string(&output).unwrap(), "a\nb\n");

    for path in [&input, &output] {
        let _ = fs::remove_file(path);
    }
}

#[test]
fn same_file_as_input_and_output_reports_nothing() {
    // C opens the input, then truncates the output, and only then reads — so the
    // stream it compares is already empty and the run prints nothing (exit 0
    // because `main` returns 0). Reading the input first would echo the old bytes
    // back, which is the mistake this case keeps out.
    let file = scratch("same");
    fs::write(&file, "a\na\nb\n").unwrap();
    let name = file.to_str().unwrap().to_owned();
    let (out, status, err) = run(&[&name, &name], "");
    assert_eq!(out, "");
    assert_eq!(status, 0, "stderr was {err:?}");
    assert_eq!(fs::read(&file).unwrap(), b"");
    let _ = fs::remove_file(&file);
}

#[test]
fn unopenable_input_leaves_no_output_file() {
    // `case 2:` opens the input first; `file()` then dies through
    // `err(1, "%s", name)` before the output is created.
    let missing = scratch("missing");
    let output = scratch("not-created");
    let _ = fs::remove_file(&output);
    let missing_text = missing.to_str().unwrap().to_owned();
    let output_text = output.to_str().unwrap().to_owned();
    let (out, status, err) = run(&[&missing_text, &output_text], "");
    assert_eq!(out, "");
    assert_eq!(status, 1);
    // `err(1, "%s", name)` 的三个成分（程序名、操作数、`strerror` 句子），
    // 而且它是致命的（退 1）。
    assert_eq!(
        err,
        format!("uniq: {missing_text}: No such file or directory\n")
    );
    assert!(
        !output.exists(),
        "the output file must not have been created"
    );
}

#[test]
fn agrees_with_the_host_uniq_where_the_two_specs_coincide() {
    // Only the forms whose NetBSD and GNU semantics match: plain collapsing, the
    // `-d`/`-u` selections and the field/character skip. The `-c` prefix and the
    // obsolete forms are excluded on purpose — GNU prints `%7d` there and rejects
    // `+N`, so comparing them would measure the difference between the two
    // programs rather than a regression in this one.
    let inputs = [
        ("a\na\nb\n", vec![vec![], vec!["-d"], vec!["-u"]]),
        (
            "foo SAME\nbar SAME\nbaz OTHER\n",
            vec![vec!["-f", "1"], vec!["-s", "4"]],
        ),
    ];
    let system = if Command::new("uniq").arg("--version").output().is_ok() {
        "uniq"
    } else {
        eprintln!("no host uniq to compare against; skipping the differential leg");
        return;
    };
    for (input, option_sets) in inputs {
        for options in option_sets {
            let ours = run(&options, input);
            let theirs = run_with(system, &options, input);
            assert_eq!(
                ours.0, theirs.0,
                "options {options:?} on {input:?}: ours {:?}, host {:?}",
                ours.0, theirs.0
            );
            assert_eq!(ours.1, theirs.1, "options {options:?} on {input:?}");
        }
    }
}

/// 把标准输出接到一枚已打开的落点上跑一腿（`/dev/full` 用得上：写它必回 `ENOSPC`）。
fn run_into_sink(args: &[&str], input: &str, sink_path: &str) -> (i32, String) {
    let sink = fs::OpenOptions::new()
        .write(true)
        .open(sink_path)
        .expect("/dev/full carries a failing write");
    let mut child = Command::new(UNIQ)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::from(sink))
        .stderr(Stdio::piped())
        .spawn()
        .expect("the uniq binary should be runnable");
    face_support::write_stdin(&mut child, input.as_bytes());
    let output = child.wait_with_output().expect("the run should finish");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// 写腿第一形（标准输出写不进）：C 的输出腿从不查失手——`file(argv[1], "w")` 只在**开档**
/// 失败时 `err(1, "%s", name)`（`minix3/usr.bin/uniq/uniq.c:216-221`），此后 `fprintf`
/// 的返回值一律不看，全文件没有一处 `ferror`/`fflush`。参照件同形实测一字不报、**退 0**。
/// 改前本壳经 `support::emit` 静默退 1：一句话不说是对的，退码不对。
#[test]
fn a_stdout_that_stops_taking_bytes_stays_silent_and_exits_zero() {
    let (status, err) = run_into_sink(&[], "a\na\nb\n", "/dev/full");
    assert_eq!(
        (status, err.as_str()),
        (0, ""),
        "C 查不到写失手，本壳也不该因此改退码或添句子"
    );
}

/// 写腿第二形（输出操作数写不进）：宿主上 `File::create("/dev/full")` 这一层是成功的
/// （字符设备的截断是空操作），失败只落在写与收尾——正是 C 不看的那两处。改前本壳印一句
/// 只有两段的 `uniq: <路径>` 并退 1：句子不是 C 的句子（C 的两段形只出现在 `errx`），
/// 退码也不是 C 的退码。
#[test]
fn an_output_operand_that_cannot_be_written_stays_silent_and_exits_zero() {
    let input = scratch("sink-in");
    fs::write(&input, "a\na\nb\n").unwrap();
    let name = input.to_str().unwrap().to_owned();
    let (status, err) = run_into_sink(&[&name, "/dev/full"], "", "/dev/full");
    assert_eq!(
        (status, err.as_str()),
        (0, ""),
        "写失手在 C 那侧不可观察，本壳两形都要一致地静默退 0"
    );
    let _ = fs::remove_file(&input);
}
