//! End-to-end face for the `primes` binary: real process, real pipes, checked
//! against `minix3/games/primes/primes.c` (the operand handling `:129-172`,
//! `read_num_buf` `:176-205`, `usage` `:339-343`).
//!
//! Before this batch the shell had **no** diagnostic at all: every bad argument
//! reached `terminate(1)` in silence, so a typo looked identical to an empty
//! interval. C splits those failures by family, and the split is the content:
//! a word `strtoumax` cannot convert at all sets `errno`, so C prints
//! `primes: <word>: Invalid argument` (`primes.c:138`/`:145`/`:160` via
//! `err(1, "%s", argv[i])`), while digits followed by junk leave `errno` alone and
//! print `primes: <word>: illegal numeric format.` (`:140`, note the period) —
//! the first is the `err(3)` family, the second `errx(3)`.
//!
//! `usage()` is the other family detail worth pinning: C writes it as a literal
//! (`:341`), so renaming the binary moves the `err`/`errx` lines and leaves the
//! usage line alone — the opposite of `rm`/`cp`/`mv`.
//!
//! Two of C's faces are declared out of reach instead of faked: the zero-operand
//! path that reads a start value from standard input (`read_num_buf`), which this
//! tree replaces with the 2..cap interval, and the base-0 spellings. Option
//! *permutation* (an option letter after an operand) is the separate `7g` item.
//!
//! The `-d`/`-h` legs are wired now, and their golden values were not written by
//! hand: they were read off a **reference process** built from the very C sources
//! (`cc -O2 -D'__RCSID(x)=' -D'__COPYRIGHT(x)='
//!  -D'__dead=__attribute__((__noreturn__))' -include stdlib.h
//!  minix3/games/primes/{primes,pattern,pr_tbl,spsp}.c -lm -o /tmp/primes_ref`),
//! then diffed against this binary over 21 argument shapes (`SAME=21 DIFF=0`).
//! That diff is also what surfaced the exclusive-`stop` bug this batch fixes:
//! C says `>= start and < stop` in its own header (`primes.c:49`), and the
//! reference prints `5 7 11` for `primes 5 13` while the old shell printed `13`
//! too. The first test below used to justify its output as "both ends included"
//! — the output happened to match because 20 is not prime, while the rationale
//! was wrong. An assertion can be green and still teach the opposite of C.

use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;
use std::process::{Command, Stdio};

const USAGE: &str = "usage: primes [-dh] [start [stop]]\n";

fn run_at(program: &Path, args: &[&str]) -> (String, i32, String) {
    let output = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("the primes binary should be runnable");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

fn run(args: &[&str]) -> (String, i32, String) {
    run_at(Path::new(env!("CARGO_BIN_EXE_primes")), args)
}

#[test]
fn a_bounded_run_prints_primes_and_stays_silent_on_stderr() {
    // One prime per line for `start <= p < stop` (`primes.c:49`, `primes.c:173`
    // → `primes(start, stop)`). 20 is not prime, so this shape looks the same
    // under an inclusive bound — the exclusivity itself is pinned by
    // `stop_is_an_exclusive_bound` below, which is where C and the old shell
    // actually disagreed.
    let (out, status, err) = run(&["10", "20"]);
    assert_eq!(status, 0);
    assert_eq!(err, "", "正常路一声不出");
    assert_eq!(out, "11\n13\n17\n19\n");
}

#[test]
fn digits_with_trailing_junk_is_the_errx_line() {
    // `primes.c:140`: `errx(1, "%s: illegal numeric format.", argv[0])` — digits
    // converted, `errno` untouched, so the line ends with C's own period and no
    // strerror sentence.
    let (out, status, err) = run(&["12a"]);
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(err, "primes: 12a: illegal numeric format.\n");
}

#[test]
fn a_word_that_cannot_convert_carries_the_number_sentence() {
    // `primes.c:138` with `_strtol.h:69` (`errno = EINVAL`): this one is
    // `err(1, "%s", argv[0])`, so the sentence belongs to the number.
    let (out, status, err) = run(&["abc"]);
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(err, "primes: abc: Invalid argument\n");
}

#[test]
fn an_inverted_interval_is_refused_by_phrase() {
    // `primes.c:171-172`: `errx(1, "start value must be less than stop value.")`.
    let (out, status, err) = run(&["5", "4"]);
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(err, "primes: start value must be less than stop value.\n");
}

#[test]
fn a_stop_beyond_the_cap_names_the_cap() {
    // `primes.c:149` prints C's `SPSPMAX`; this tree prints its own declared cap
    // (1,000,000 — the difference is registered in 22 篇, the shape is C's).
    let (out, status, err) = run(&["2", "2000000"]);
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(err, "primes: 2000000: stop value too large (>1000000).\n");
}

#[test]
fn an_operand_starting_with_a_hyphen_is_the_negative_phrase() {
    // `primes.c:133`: the check runs on the operands *after* option scanning, so
    // `primes 5 -3` reaches it while `primes -5` does not (an option letter that
    // `getopt` does not know answers `usage()` first — see the next test).
    let (_, status, err) = run(&["5", "-3"]);
    assert_eq!(status, 1);
    assert_eq!(err, "primes: negative numbers aren't permitted.\n");
}

#[test]
fn unknown_letters_and_extra_operands_answer_the_hardcoded_usage() {
    // `primes.c:115-116` (`default: usage()`) and `:167-168`.
    let (out, status, err) = run(&["-q"]);
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(err, USAGE);

    let (_, status, err) = run(&["1", "2", "3"]);
    assert_eq!(status, 1);
    assert_eq!(err, USAGE);

    // C 认得的字母不再被拒收：`-d`/`-h` 从本批起是真输出面（形状由下面几枚钉），
    // 所以带它们的正常跑退 0、只走 stdout，不再出一行「未接线」。
    // （旧版在这里断言的是 `primes: -d/-h not wired …`，那个行为已不存在。）
    let (out, status, err) = run(&["-d", "2", "6"]);
    assert_eq!(status, 0);
    assert_eq!(err, "");
    assert_eq!(out, "2 (2)\n3 (1)\n5 (2)\n");
}

#[test]
fn the_usage_line_ignores_argv_zero_while_the_error_lines_do_not() {
    // The two halves of C's habit in one command: `err`/`errx` print
    // `getprogname()` (`:341` is a literal), so a renamed copy moves one line and
    // not the other.
    let copied = std::env::temp_dir().join(format!("primes-face-copy-{}", std::process::id()));
    fs::copy(Path::new(env!("CARGO_BIN_EXE_primes")), &copied).expect("copy the binary");
    fs::set_permissions(&copied, fs::Permissions::from_mode(0o755)).expect("make it run");

    let name = copied.file_name().unwrap().to_str().unwrap().to_string();
    let (_, _, err) = run_at(&copied, &["abc"]);
    assert_eq!(err, format!("{name}: abc: Invalid argument\n"));

    let (_, _, err) = run_at(&copied, &["-q"]);
    assert_eq!(err, USAGE, "C 的 usage 是硬写字面量，不跟 `argv[0]`");

    let _ = fs::remove_file(&copied);
}

#[test]
fn stop_is_an_exclusive_bound() {
    // The bug this batch fixes, pinned where C and the old shell actually disagreed.
    // C: `primes.c:49` “Print primes >= start and < stop”; reference process reads
    // `primes 5 13` → `5 7 11` and `primes 2 2` → nothing at all. The old shell
    // treated `stop` as inclusive and printed `13` / `2`.
    assert_eq!(run(&["5", "13"]).0, "5\n7\n11\n");
    assert_eq!(
        run(&["2", "2"]).0,
        "",
        "空输出不是错误：C 只拦 `start > stop`"
    );
    assert_eq!(run(&["13", "13"]).0, "");
    assert_eq!(run(&["7", "8"]).0, "7\n", "上界不是素数时两形看起来一样");
    // `start > stop` 仍是那一句 `errx`（`primes.c:171-172`），与开区间不矛盾。
    let (_, status, err) = run(&["20", "10"]);
    assert_eq!(status, 1);
    assert_eq!(err, "primes: start value must be less than stop value.\n");
}

#[test]
fn difference_column_prints_c_including_its_first_prime_quirk() {
    // `-d` 的差值对位 C 的 `prev`（`primes.c:220`/`:261`）。两个细节都不是凑出来的：
    // 首项 `2 (2)` 因为 `prev` 从 0 起；`5 15` 里 `5 (2)` 的前驱 3 落在区间外，C 照样
    // 拿它作差。逐字取自参照进程的同输入读数。
    assert_eq!(run(&["-d", "5", "15"]).0, "5 (2)\n7 (2)\n11 (4)\n13 (2)\n");
    assert_eq!(run(&["-d", "2", "10"]).0, "2 (2)\n3 (1)\n5 (2)\n7 (2)\n");
    assert_eq!(run(&["-d", "1", "10"]).0, "2 (2)\n3 (1)\n5 (2)\n7 (2)\n");
    // 113 的前驱是 109 而不是「上一项奇数」，所以差是 4；127 前面是 113，差 14。
    assert_eq!(run(&["-d", "113", "130"]).0, "113 (4)\n127 (14)\n");
}

#[test]
fn hex_output_is_lowercase_and_carries_no_prefix() {
    // `printf("%" PRIx64, …)`（`primes.c:255`/`:327`）：小写、无 `0x`，所以 17 打 `11`。
    // 这一点单看输出容易错认成“少了个 0x”，所以连十进制同跑一次的对照也钉住。
    assert_eq!(run(&["-h", "5", "20"]).0, "5\n7\nb\nd\n11\n13\n");
    assert_eq!(run(&["5", "20"]).0, "5\n7\n11\n13\n17\n19\n");
}

#[test]
fn hex_flag_does_not_make_the_difference_hexadecimal() {
    // 两个旗标各管一段：`-h` 只改素数本身的进制，差值列在 C 里始终是十进制
    // `PRIu64`（`:257`/`:329`）。参照进程：`primes -dh 113 130` → `71 (4)`/`7f (14)`，
    // 那个 `14` 若跟着走十六进制就会变成 `e`。
    assert_eq!(run(&["-dh", "113", "130"]).0, "71 (4)\n7f (14)\n");
}

#[test]
fn first_prime_above_the_stored_table_has_no_difference_column() {
    // C 的筛子路守卫 `dflag && (prev || (start <= *pr_limit))`（`:328`）在走到这条路时
    // 只剩 `prev != 0`，所以从表外（存表尾值 65537，`pr_tbl.c`）开始时第一个素数不带列。
    // 两形都逐字取自参照进程：`primes -d 70000 70040` 的首行就是不带列的 `70001`，
    // 而 `primes -d 65530 65545` 从表内起，首项就带 `65537 (16)`。
    let (out, status, err) = run(&["-d", "70000", "70040"]);
    assert_eq!(status, 0);
    assert_eq!(err, "");
    assert_eq!(
        out, "70001\n70003 (2)\n70009 (6)\n70019 (10)\n70039 (20)\n",
        "第一个不带列，往后的照带"
    );
    assert_eq!(
        run(&["-d", "65530", "65545"]).0,
        "65537 (16)\n65539 (2)\n65543 (4)\n"
    );
}

#[test]
fn option_letters_cluster_repeat_and_split() {
    // `getopt("dh")` 的两个分支都是 `flag++`（`primes.c:107-112`），所以同一个旗标写几次
    // 也只是一个旗标。等价类得按字母集分开，不能拢总比：参照进程给得出三个类——
    // `{dh, hd, -d -h, -d -dh}` 同打「十六进制 + 差值」，`{d, dd}` 同打「十进制 + 差值」，
    // `{h, hh}` 同打「十六进制不带差」。（本批第一轮把 `-dd` 归进了 `-dh` 那一类，
    // 测试当场红在那里：代码是对的，错的是分类。）
    let both = run(&["-dh", "10", "20"]);
    assert_eq!(both.0, "b (4)\nd (2)\n11 (4)\n13 (2)\n");
    for shape in [
        &["-hd", "10", "20"][..],
        &["-d", "-h", "10", "20"][..],
        &["-h", "-d", "10", "20"][..],
        &["-d", "-dh", "10", "20"][..],
    ] {
        assert_eq!(both.0, run(shape).0, "{shape:?} 应与 `-dh` 同形");
    }
    let diff_only = run(&["-d", "10", "20"]);
    assert_eq!(diff_only.0, "11 (4)\n13 (2)\n17 (4)\n19 (2)\n");
    assert_eq!(
        diff_only.0,
        run(&["-dd", "10", "20"]).0,
        "`-dd` 是 `-d`，不是 `-dh`"
    );
    let hex_only = run(&["-h", "10", "20"]);
    assert_eq!(hex_only.0, "b\nd\n11\n13\n");
    assert_eq!(hex_only.0, run(&["-hh", "10", "20"]).0, "`-hh` 是 `-h`");
}
