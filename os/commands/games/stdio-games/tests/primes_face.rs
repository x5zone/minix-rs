//! End-to-end face for the `primes` binary: real process, real pipes, checked
//! against `minix3/games/primes/primes.c` (the operand handling `:129-172`,
//! `read_num_buf` `:176-205`, `usage` `:339-343`).
//!
//! Before this batch the shell had **no** diagnostic at all: every bad argument
//! reached `terminate(1)` in silence, so a typo looked identical to an empty
//! interval. C splits those failures by family, and the split is the content: the
//! `err(3)` line (`primes.c:138`/`:145`/`:160`, `err(1, "%s", argv[i])`) is reached
//! **only when `errno` is actually set**, and in this command the only thing that sets
//! it is a range error — `strtoumax` over/underflow (`_strtol.h:157`/`:172`, giving
//! `Result too large or too small`). Everything else leaves `errno` alone and lands on
//! `errx(1, "%s: illegal numeric format.")` (`:140`, note the period), *including* a
//! word with no convertible digits: NetBSD/Minix's `_strtol.h:184-186` stores the
//! original `nptr` in `endptr` when nothing was consumed and does not touch `errno`
//! (the `EINVAL` at `:69` guards an invalid `base` argument, which `strtoumax(…, 0)`
//! never hits), so `*p != '\0'` fires. Earlier batches of this door asserted
//! `primes: abc: Invalid argument` on the strength of a misreading of that line;
//! a host-built C reference (`primes_ref` from these very sources) says otherwise.
//!
//! The two families are still worth separating — just along the real seam: overflow
//! (`ERANGE`, sentence appended) versus any malformed word (`errx`, no sentence).
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

/// C 的零操数支从标准输入读起始值（`read_num_buf`，`primes.c:176-205`），所以要能喂 stdin。
/// 读出的那一路不继承 `run_at` 的 `Stdio::null()`——那是这条腿唯一测不到的原因。
fn run_stdin(args: &[&str], input: &str) -> (String, i32, String) {
    use std::io::Write as _;
    let mut child = Command::new(Path::new(env!("CARGO_BIN_EXE_primes")))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the primes binary should be runnable");
    child
        .stdin
        .take()
        .expect("stdin pipe")
        .write_all(input.as_bytes())
        .expect("feed stdin");
    let output = child.wait_with_output().expect("collect the output");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// 零操数支的四种收尾各归 C 的一支。注意两支报文的实参是**整块 `buf`（含换行）**，
/// 所以 C 的一行会断成两物理行——这不是排版失误，而是 `err(1, "%s", buf)` /
/// `errx(1, "%s: illegal numeric format.", buf)` 的字面形状。
#[test]
fn the_stdin_legs_split_into_c_s_four_outcomes() {
    // ①只有一个换行/空白行：C `continue` 再读一行；这里喂「空白行 + 真值」证明它确实跳过了。
    // 值取 999983 是因为本树上限 1e6（C 的默认上界近 2^32，声明差）：参照件在这一形上
    // 给 `999983 1000003 1000033…`，我方只到上限为止，区间里也只有这一个素数（实测）。
    let (out, status, err) = run_stdin(&[], "\n  \n999983\n");
    assert_eq!(status, 0);
    assert_eq!(err, "", "跳过空白行不该出声");
    assert_eq!(out, "999983\n", "起始值取自第三行");

    // ②空输入（EOF，无 `ferror`）：C `exit(0)`，一声不出。
    let (out, status, err) = run_stdin(&[], "");
    assert_eq!((status, out, err), (0, String::new(), String::new()));

    // ③一个数字也转不出：本仓真源 `_strtol.h:69` 在此置 `EINVAL`，走 `err(1, "%s", buf)`。
    //   （宿主 glibc 的 `strtoumax` 不置这个号，参照件因此与真源不同落点，以真源为准。）
    let (_, status, err) = run_stdin(&[], "abc\n");
    assert_eq!(status, 1);
    assert_eq!(err, "primes: abc\n: illegal numeric format.\n");

    // ④转出了前缀而后可非换行：`errx`，缀整块 buf、句末带句点。
    let (_, status, err) = run_stdin(&[], "10x\n");
    assert_eq!(status, 1);
    assert_eq!(err, "primes: 10x\n: illegal numeric format.\n");

    // 负号支在读之前就拒（`:193-194`），前导空白不影响它。
    let (_, status, err) = run_stdin(&[], " -5\n");
    assert_eq!(status, 1);
    assert_eq!(err, "primes: negative numbers aren't permitted.\n");

    // 溢出支：C 置 `ERANGE` 后走 `err` 族，句子取本树那张 Minix3 表。
    let (_, status, err) = run_stdin(&[], "999999999999999999999999\n");
    assert_eq!(status, 1);
    assert_eq!(
        err,
        "primes: 999999999999999999999999\n: Result too large or too small\n"
    );
}

/// `strtoumax(…, 0)`：`0x`/`0X` 走十六进制、前导 `0` 走八进制、前导空白与 `+` 都算合法。
/// 操数支与 stdin 支共用同一个转换，所以两边都得认。
#[test]
fn bounds_are_converted_in_base_zero_like_c() {
    let (out, status, err) = run(&["0x10", "0x20"]);
    assert_eq!(status, 0, "stderr: {err}");
    assert_eq!(out, "17\n19\n23\n29\n31\n", "16<=p<32");

    let (out, _, _) = run(&["010", "014"]);
    assert_eq!(out, "11\n", "八进制 010=8、014=12，中间只有 11");

    // 两个操数也走同一个转换：`0x10..0x20` 即 16<=p<32。
    let (out, status, _) = run(&["0x10", "0x20"]);
    assert_eq!(status, 0);
    assert_eq!(out, "17\n19\n23\n29\n31\n");

    // stdin 那条腿与 `-d` 可以叠加（C 先 getopt 再取操数/stdin），并且**存表支点在这条腿上
    // 同样生效**：起始值 999979 在存表（尾值 65537）之外，所以第一个素数不带差列、往后的照带。
    // 参照件同读数（`printf 999979 | primes_ref -d` → `999979` / `999983 (4)` / `1000003 (20)`…，
    // 我方到 1e6 上限为止，那是已声明的上界差异）。
    let (out, status, _) = run_stdin(&["-d"], "999979\n");
    assert_eq!(status, 0);
    assert_eq!(
        out, "999979\n999983 (4)\n",
        "首个不带列、次个带：与表内起点那一形不同"
    );
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
fn a_word_with_no_digits_is_the_errx_line_too() {
    // 「一个位也没转出」并**不**缀句子：真源 `_strtol.h:184-186` 把 `endptr` 留在最初
    // 的 `nptr` 且不动 errno（`:69` 的 `EINVAL` 是 base 参数非法的分支，base 恒 0 到不了），
    // 于是 `primes.c:139` 的 `*p != '\0'` 成立，走上面那同一句 `errx`。
    // 本文件此前把它当成 `err` 族（并据此命名），是按 `_strtol.h` 误读；参照件
    // （宿主编译的 `primes_ref`）与真源在此一致，故改名并改断言。
    let (out, status, err) = run(&["abc"]);
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(err, "primes: abc: illegal numeric format.\n");
}

#[test]
fn only_a_range_error_appends_the_strerror_sentence() {
    // 操数路上唯一真会置 errno 的是越范围（`_strtol.h:157`），它才走
    // `err(1, "%s", argv[i])` 那一族；句子取本树那张 Minix3 表（表外另有分支）。
    let (out, status, err) = run(&["999999999999999999999999", "5"]);
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(err, "primes: 999999999999999999999999: Result too large or too small\n");
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
    assert_eq!(err, format!("{name}: abc: illegal numeric format.\n"));

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
