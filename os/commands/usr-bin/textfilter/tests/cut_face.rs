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

mod face_support;

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
    face_support::write_stdin(&mut child, input.as_bytes());
    let output = child.wait_with_output().expect("the run should finish");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// Same seam as `run`, but the two streams stay **bytes**: `from_utf8_lossy` turns a
/// bare `c3` into the three-byte replacement character, so a text form cannot tell
/// "one byte" from "two bytes" (same reason `rev`/`paste`/`jot` carry a `run_bytes`).
fn run_bytes(args: &[&str], input: &str) -> (Vec<u8>, i32, Vec<u8>) {
    let mut child = Command::new(CUT)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the cut binary should be runnable");
    face_support::write_stdin(&mut child, input.as_bytes());
    let output = child.wait_with_output().expect("the run should finish");
    (
        output.stdout,
        output.status.code().unwrap_or(-1),
        output.stderr,
    )
}

/// argv 形态的**字节形**夹具：一枚裸 `ff` 写不成 `&str`，所以参数经 `OsStr::from_bytes`
/// 交下去，比的是子进程实际收到的那串字节（文本形夹具里 `from_utf8_lossy` 会把裸 `ff`
/// 与 `ef bf bd` 弄成同一个串，那正是被测的缝）。
fn run_argv(args: &[&[u8]], input: &[u8]) -> (Vec<u8>, i32, Vec<u8>) {
    use std::os::unix::ffi::OsStrExt as _;
    let mut child = Command::new(CUT)
        .args(args.iter().map(|a| std::ffi::OsStr::from_bytes(a)))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the cut binary should be runnable");
    face_support::write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        output.stdout,
        output.status.code().unwrap_or(-1),
        output.stderr,
    )
}

/// 命令行参数那一半也按字节了（批次四百六十一）：`-d` 的值与 `column` 的 `-s` 同族——它不是
/// 回显位，而是被 `cut.c:101-102` 的一句 `dchar = *optarg` 拿去切**输出内容**的首字节。所以
/// 文本缝折一次之后分隔符变成 `ef`，同一份输入上「该切的切不开、不该切的反倒被切」。参照件
/// `tmp/cut-406/cut`（软链名跑的，`getprogname()` 算出 `cut` 与本壳一致）现量：`-d<ff> -f1`
/// 对 `a<ff>b<TAB>z` 交回 `a`、值附形 `-d<ff>` 同形；`-d<ff>` 对只含 U+FFFD 三字节的行原样交回。
/// 第三段钉字母位：`-\xff` 交回那一枚裸字节本身，再跟那三行写死的 usage、退 1。
#[test]
fn a_raw_byte_delimiter_splits_the_field_and_a_raw_byte_letter_is_named() {
    let (out, code, err) = run_argv(&[b"-d\xff", b"-f", b"1"], b"a\xffb\tz\n");
    assert_eq!(
        (out.as_slice(), code, err.as_slice()),
        (&b"a\n"[..], 0, &b""[..]),
        "分隔符该是 argv 里那一枚 ff"
    );

    let (out, code, err) = run_argv(&[b"-d", b"\xff", b"-f", b"2"], b"a\xffb\tz\n");
    assert_eq!(
        (out.as_slice(), code, err.as_slice()),
        (&b"b\tz\n"[..], 0, &b""[..]),
        "第二列拿到的是 ff 之后的那一段（制表符仍在列里）"
    );

    let (out, code, err) = run_argv(&[b"-d", b"\xff", b"-f", b"1"], b"a\xef\xbf\xbdb\n");
    assert_eq!(
        (out.as_slice(), code, err.as_slice()),
        (&b"a\xef\xbf\xbdb\n"[..], 0, &b""[..]),
        "U+FFFD 的三枚字节不是分隔符，整行照搬"
    );

    let (out, code, err) = run_argv(&[b"-\xff", b"-f", b"1"], b"a\n");
    assert_eq!(out, Vec::new(), "集外字母不打数据");
    assert_eq!(code, 1);
    assert_eq!(
        err,
        [
            b"cut: unknown option -- \xff\n".as_slice(),
            USAGE.as_bytes()
        ]
        .concat(),
        "字母位该是 argv 里那一枚原字节"
    );
}

/// The letter position of the libc line is **one raw byte**: the reference
/// `tmp/cut-406/cut_ref` prints `cut: unknown option -- ` + `c3` + newline for both
/// `-\u{e9}` and `-s\u{e9}` (`s` is one of the two *valueless* letters of C's `"b:c:d:f:sn"` —
/// `b`/`c`/`d`/`f` all take a value — so the cluster keeps scanning into the first byte of
/// `\u{e9}`), then the three tabbed usage lines, exit 1. `-Z\u{e9}` stops at `Z`: `cut.c` sends
/// `'?'` straight to `usage()`, so reports never accumulate. The reverse-side form
/// `-c\u{e9}` is here for the opposite reason: `c` *takes* a value, so `\u{e9}` becomes the list
/// and C answers with its own list complaint — the render switch must not touch that leg.
/// ASCII forms are already pinned by `usage_goes_to_stderr_as_three_tabbed_lines` and the
/// unknown-letter gates in this file, so this gate carries the byte-width fact, the
/// cluster-stop shape, and the untouched value path.
#[test]
fn the_libc_line_of_a_multibyte_flag_is_one_byte_before_the_three_line_usage() {
    let want: Vec<u8> = [
        b"cut: unknown option -- ".as_slice(),
        b"\xc3\n",
        USAGE.as_bytes(),
    ]
    .concat();
    for form in ["-\u{e9}", "-s\u{e9}"] {
        let (out, status, err) = run_bytes(&[form], "abc\n");
        assert_eq!(out, Vec::new(), "{form} 的报文只走标准错误");
        assert_eq!(status, 1, "{form} 的 stderr 是 {err:?}");
        assert_eq!(err, want, "{form} 的字母该是一个裸字节 c3");
    }

    let (_out, status, err) = run_bytes(&["-Z\u{e9}"], "abc\n");
    assert_eq!(status, 1);
    assert_eq!(
        err,
        [b"cut: unknown option -- Z\n".as_slice(), USAGE.as_bytes(),].concat(),
        "环停在第一个坏字母上，第二个字节根本不该被读到"
    );

    let (_out, status, err) = run_bytes(&["-c\u{e9}"], "abc\n");
    assert_eq!(status, 1);
    assert_eq!(
        err,
        b"cut: [-bcf] list: illegal list value\n".to_vec(),
        "`c` 带值，`\u{e9}` 整个被当列表收走 ⇒ 那是命令自己的一句 errx，不接 usage"
    );
}

fn scratch(name: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!("cut-face-{}-{}", std::process::id(), name));
    path
}

/// 参照件读数 `tmp/cut-428/ref-s.log`：`-s` 之下「没有分隔符的行」整行消失（不是
/// 一枚空行），而「有分隔符但选不出字段」的行仍留下一枚换行——两形只在换行上差一字节，
/// 却正好是「这一行到底算不算被写过」。
#[test]
fn a_suppressed_line_leaves_nothing_behind() {
    let (out, status, err) = run(&["-f", "3", "-s"], "x\na\tb\tc\n");
    assert_eq!((out.as_str(), status, err.as_str()), ("c\n", 0, ""));
    let (out, status, err) = run(&["-f", "3"], "x\na\tb\tc\n");
    assert_eq!(
        (out.as_str(), status, err.as_str()),
        ("x\nc\n", 0, ""),
        "不带 `-s` 时那一行原样照抄，连自己的换行一起"
    );
    let (out, status, err) = run(&["-f", "9", "-s"], "a\tb\tc\n");
    assert_eq!(
        (out.as_str(), status, err.as_str()),
        ("\n", 0, ""),
        "有分隔符而第 9 档选空：写了这一行，只是没写出字段"
    );
    let (out, status, err) = run(&["-f", "1", "-s"], "nosep");
    assert_eq!(
        (out.as_str(), status, err.as_str()),
        ("", 0, ""),
        "末行不带换行也一样不写"
    );
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
    // 这一只拿两侧一致的形做金样。C 的 `get_list` 曾经比本树宽（`cut.c:171-174`
    // 的注释写着「This parser is less restrictive than the Draft 9 POSIX spec」），
    // 批次二百八十六已经把列表语法对齐：`strtok(list, ", \t")`（`:175`）折叠空项
    // 并把空格与制表符也当分隔符，`5-2`（`:209-210` 不查大小关系）在 C 选中空
    // 集合而不是报错，`-f ''` 连一次循环体都不进。这三形的读数另有四枚门钉住，
    // 不在本枚里。
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
fn a_reversed_range_selects_nothing_rather_than_erroring() {
    // C 不查区间大小关系（`cut.c:209-210` 只是 `start++ <= stop` 地置位），所以
    // `5-2` 一个位置也不置，而 `x_cut.c:69-87` 那个外层环路每行仍以一个换行结尾。
    // 旧写法把它当成「illegal list value」拒掉。
    let (out, status, err) = run(&["-b", "5-2"], "abcde\nxyz\n");
    assert_eq!(out, "\n\n", "倒序区间选不中任何东西，但行还在");
    assert_eq!(status, 0, "stderr was {err:?}");
}

#[test]
fn separators_include_blank_space_and_empty_items() {
    // `cut.c:175` 是 `strtok(list, ", \t")`：三个分隔符、空项折叠。所以五种写法
    // 选到的是同一组位置（旧写法只认逗号，且把空项当错）。
    for list in ["1,3", "1 3", "1\t3", "1,,3", " 1 , 3 "] {
        let (out, status, err) = run(&["-b", list], "abcde\n");
        assert_eq!(out, "ac\n", "列表 {list:?} 的 stderr 是 {err:?}");
        assert_eq!(status, 0, "列表 {list:?}");
    }
}

#[test]
fn an_empty_list_is_not_an_error_and_prints_a_blank_line_per_input() {
    // `get_list("")` 里 `strtok` 直接交 NULL，循环体一次也跑不到：不报错、也不置
    // 任何位置（`cut.c:175`）；`x_cut.c:86` 又总给每行补一个换行。旧写法把空串
    // 当成「illegal list value」。
    let (out, status, err) = run(&["-b", ""], "abc\nde\n");
    assert_eq!(out, "\n\n");
    assert_eq!(status, 0, "stderr was {err:?}");
}

#[test]
fn more_than_sixteen_items_is_accepted() {
    // C 的 positions 数组按需长大（`cut.c:163-164`、`:199-206`，每块
    // `_POSIX2_LINE_MAX`），本树之前封顶 16 项。20 项该照切。
    let list = (1..=20)
        .map(|n| n.to_string())
        .collect::<Vec<String>>()
        .join(",");
    let line = "abcdefghijklmnopqrst\n";
    let (out, status, err) = run(&["-b", &list], line);
    assert_eq!(out, line, "stderr 是 {err:?}");
    assert_eq!(status, 0);
}

#[test]
fn a_trailing_dash_opens_the_line_end_but_a_dangling_one_is_illegal() {
    // C 在 `*p == '-'` 上写了两个判据（`cut.c:186`、`:189`），不能压平成一件事。
    // `1-2-` 是接受的：第二个判据跨过尾部那个 `-` 正好走到行尾，autostop 那一支
    // 把 maxval 之后的内容原样打出（`x_cut.c:80-82`），效果就是从 1 打到行尾。
    // `5--` 却不接受：`isdigit(p[1])` 不成立（`:187`），跨过索引 1 的 `-` 后还停在
    // 索引 2 的 `-` 上 → 残留那句（`:195-196`）而不是零那句。
    let (out, status, err) = run(&["-b", "1-2-"], "abcde\n");
    assert_eq!(out, "abcde\n", "stderr 是 {err:?}");
    assert_eq!(status, 0);
    let (out, status, err) = run(&["-b", "5--"], "abcde\n");
    assert_eq!(out, "");
    assert_eq!(status, 1, "stderr 是 {err:?}");
    assert_eq!(err, "cut: [-bcf] list: illegal list value\n");
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

/// `cut` 是这一族里 C **确实**归一化补换行的那支：每处理完一行都写一枚换行，
/// 无尾换行的输入也被补上。参照件 `tmp/cut-406/cut_ref` 对 `alpha\nbeta\ngamma`
/// 现量 `alp\nbet\ngam\n`。这一枚钉的是「归一化」本身，免得后来把 `head`/`tail`/
/// `nl`/`uniq` 那条「不造换行」的判据顺手统一到 `cut` 上，反而与真源不符。
#[test]
fn cut_normalizes_every_line_with_a_newline_even_the_last() {
    let (out, code, err) = run(&["-c", "1-3"], "alpha\nbeta\ngamma");
    assert_eq!(
        (out.as_str(), code, err.as_str()),
        ("alp\nbet\ngam\n", 0, "")
    );
}

/// 换行当分隔符时的**壳侧**读数：`-d $'\n'` 的 `-f 2` 每行交回一枚换行、字段内容为空，
/// 加上 `-s` 也一样（C 先给末行补上换行再扫，`cut.c:235-241`）；`-f 1` 则是通常的那一形。
/// 对照参照件 `tmp/cut-406/cut_ref` 同形。
#[test]
fn a_newline_delimiter_keeps_every_line_in_the_field_leg() {
    let (out, code, _) = run(&["-d", "\n", "-f", "2"], "a\nb c\n");
    assert_eq!(out, "\n\n");
    assert_eq!(code, 0);
    let (suppressed, code, _) = run(&["-d", "\n", "-f", "2", "-s"], "a\nb c\n");
    assert_eq!(suppressed, "\n\n", "加 `-s` 也照打每行一枚换行");
    assert_eq!(code, 0);
    let (first, code, _) = run(&["-d", "\n", "-f", "1"], "a\nb c\n");
    assert_eq!(first, "a\nb c\n");
    assert_eq!(code, 0);
}
