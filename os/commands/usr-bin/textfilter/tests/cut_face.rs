//! End-to-end face for the `cut` binary: real process, real pipes and real
//! files, checked against `minix3/usr.bin/cut/cut.c`.
//!
//! This file exists because the diagnostics are the part `cut` got most wrong,
//! and the shapes involved are only observable through a real process: which
//! stream a message lands on, how many lines `usage()` really is, and which
//! leg prints the libc `getopt` report before its own. The C is explicit at
//! every one of those points: `usage()` (`cut.c:290-296`) writes **three**
//! tab-separated lines to standard error with the program name spelled literally
//! (no `%s`), `getopt` prints its own line before returning `'?'`
//! (`minix3/lib/libc/stdlib/getopt.c:101-104` for an unknown letter,
//! `:132-136` for a missing value), the two list complaints are `errx` with no
//! value substituted back (`cut.c:196`, `:198`), and an unopenable operand is
//! `warn("%s", *argv)` — program name, operand, and the `strerror` sentence
//! (`cut.c:138-142`).

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};

const CUT: &str = env!("CARGO_BIN_EXE_cut");

/// C `cut.c:290-296` verbatim: a literal tab after `usage:`, two more lines each
/// starting with a tab, no program-name placeholder.
const USAGE: &str = "usage:\tcut -b list [-n] [file ...]\n\
                     \tcut -c list [file ...]\n\
                     \tcut -f list [-d string] [-s] [file ...]\n";

/// Runs `cut` with the given words and standard input, keeping the two output
/// streams apart — that separation is the point of several cases below.
fn run(args: &[&str], input: &str) -> (String, i32, String) {
    let mut child = Command::new(CUT)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the cut binary should be runnable");
    {
        use std::io::Write as _;
        let stdin = child.stdin.as_mut().expect("stdin piped");
        stdin
            .write_all(input.as_bytes())
            .expect("writing standard input");
    }
    let output = child.wait_with_output().expect("the run should finish");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

fn scratch(name: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!("cut-face-{}-{}", std::process::id(), name));
    path
}

#[test]
fn usage_goes_to_stderr_as_three_tabbed_lines() {
    // C writes it with `fprintf(stderr, ...)`, and the string is three lines.
    // The old shell printed one invented line — and sent it to **stdout**, so a
    // pipeline swallowed the diagnostic as if it were data.
    let (out, status, err) = run(&["-s"], "");
    assert_eq!(out, "", "usage 不该出现在标准输出上");
    assert_eq!(status, 1);
    assert_eq!(err, USAGE);
}

#[test]
fn unknown_letter_prints_the_libc_line_then_the_usage() {
    // Two lines, in this order: `getopt.c:101-104` then `usage()`. The letter is
    // NOT quoted — glibc writes `invalid option -- 'x'`, which is a different
    // program's text and cannot be the baseline here.
    let (out, status, err) = run(&["-x"], "a\n");
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(err, format!("cut: unknown option -- x\n{USAGE}"));
}

#[test]
fn a_value_taking_option_without_its_value_names_the_letter() {
    // `getopt.c:132-136`: the optstring does not start with `:`, so a missing
    // value is still reported by getopt and then falls into the same `?` arm.
    for letter in ['b', 'c', 'd', 'f'] {
        let (out, status, err) = run(&[&format!("-{letter}")], "");
        assert_eq!(out, "");
        assert_eq!(status, 1, "缺值应该退 1");
        assert_eq!(
            err,
            format!("cut: option requires an argument -- {letter}\n{USAGE}"),
            "-{letter} 缺值那一支"
        );
    }
}

#[test]
fn a_list_without_a_selector_is_the_bare_usage() {
    // `-s` and `-d` only mean something with `-f` (`cut.c:124`:
    // `else if ((!cflag && !bflag) || dflag || sflag) usage()`), and no list at
    // all is the first clause of that same arm. None of these print a getopt
    // line first — the loop already succeeded.
    for args in [
        vec!["-s"],
        vec!["-d,"],
        vec!["-c", "1", "-d,", "file"],
        vec!["-c", "1", "-s", "file"],
        vec!["-b", "1", "-f", "1"],
    ] {
        let words: Vec<&str> = args.iter().copied().collect();
        let (out, status, err) = run(&words, "");
        assert_eq!(out, "", "{words:?} 不该打任何内容");
        assert_eq!(status, 1, "{words:?} 的 stderr 是 {err:?}");
        assert_eq!(err, USAGE, "{words:?} 只该有 usage 三行");
    }
}

#[test]
fn list_complaints_carry_no_offending_value() {
    // `cut.c:196` and `:198` are `errx(1, "[-bcf] list: …")` with no `%s`：坏文本不会被
    // 拼回句子里。零那一形走后一句，其他读不完的走前一句。
    //
    // 这一只拿两侧一致的形做金样。C 的 `get_list` 比本树的 `parse_list` 宽（`cut.c:171-174`
    // 的注释写着「This parser is less restrictive than the Draft 9 POSIX spec」），三形
    // 归不到同一句：`strtok(list, ", \t")`（`:176`）会折叠空项并把空格与制表符也当分隔符
    //（`1,,2` 在 C 就是 `{1,2}`），`5-2`（`:193-195` 不查大小关系）在 C 选中空集合而不是
    // 报错，而 `-f ''` 连一次循环体都不进（`strtok` 直接交 NULL）。这三形归入下一条
    // 待办（列表语法），本轮不拿它们的读数当 C 的基线。
    for value in ["0", "0-5", "5-0"] {
        let (out, status, err) = run(&["-f", value], "a\n");
        assert_eq!(out, "", "列表 {value:?} 不该打任何内容");
        assert_eq!(status, 1, "列表 {value:?} 的 stderr 是 {err:?}");
        assert_eq!(
            err, "cut: [-bcf] list: values may not include zero\n",
            "列表 {value:?}"
        );
    }
    for value in ["a", "1-2-3", "1-x"] {
        let (out, status, err) = run(&["-f", value], "a\n");
        assert_eq!(out, "");
        assert_eq!(status, 1, "列表 {value:?} 的 stderr 是 {err:?}");
        assert_eq!(
            err, "cut: [-bcf] list: illegal list value\n",
            "列表 {value:?}"
        );
    }
}

#[test]
fn a_bad_list_outranks_a_later_unknown_letter() {
    // C validates the list *inside* the case arm (`get_list(optarg)`,
    // `cut.c:105`), so the list error happens while the loop is still reading
    // options — before `-x` is ever seen. Swapping the two checks would print
    // the getopt line instead.
    let (out, status, err) = run(&["-f", "oops", "-x"], "a\n");
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(err, "cut: [-bcf] list: illegal list value\n");
}

#[test]
fn a_later_unknown_letter_outranks_the_conflict() {
    // 与上一条相反的方向：两类列表并存不是环里的错，而是环外的四道 post-check
    // 之一（`cut.c:120-128`）。所以 `-x` 排在并存之后时，C 报的是 `-x`（带 libc
    // 那一行），而不是先报冲突。旧写法在选项环里就把冲突报掉了，这个形会反。
    let (out, status, err) = run(&["-b", "1", "-f", "2", "-x"], "a\n");
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(err, format!("cut: unknown option -- x\n{USAGE}"));
}

#[test]
fn a_multi_byte_delimiter_is_truncated_not_rejected() {
    // `cut.c:101-102` is one statement: `dchar = *optarg`. Nothing checks the
    // length, so `-d::` cuts on the first `:` — the old shell called this a
    // dedicated error instead.
    let (out, status, err) = run(&["-d::", "-f", "2"], "a:b:c\n");
    assert_eq!(out, "b\n");
    assert_eq!(status, 0, "stderr was {err:?}");
    // An empty value takes the NUL byte itself, which is still not an error.
    let (out, status, _) = run(&["-d", "", "-f", "1"], "a:b:c\n");
    assert_eq!(status, 0, "空分隔符不该报错，输出是 {out:?}");
}

#[test]
fn an_unopenable_operand_prints_program_operand_and_errno_sentence() {
    // `warn("%s", *argv)` (`cut.c:140`) is the three-part line; the run keeps
    // going with the remaining operands and only the exit status says 1.
    let good = scratch("partial-good");
    let missing = scratch("partial-missing");
    fs::write(&good, "x:y\n").unwrap();
    let good_text = good.to_str().unwrap().to_owned();
    let missing_text = missing.to_str().unwrap().to_owned();
    let (out, status, err) = run(&["-d:", "-f", "2", &missing_text, &good_text], "");
    assert_eq!(out, "y\n", "打不开的文件不该影响后面那个");
    assert_eq!(status, 1);
    assert_eq!(
        err,
        format!("cut: {missing_text}: No such file or directory\n")
    );
    let _ = fs::remove_file(&good);
}
