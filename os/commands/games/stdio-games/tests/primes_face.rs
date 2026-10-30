//! End-to-end face for the `primes` binary: real process, real pipes, checked
//! against `minix3/games/primes/primes.c` (the operand handling `:129-172`,
//! `read_num_buf` `:176-205`, `usage` `:339-343`).
//!
//! C splits the failures by family, and the split is the content: the `err(3)`
//! line (`primes.c:138`/`:145`/`:160`, `err(1, "%s", argv[i])`) is reached
//! **only when `errno` is actually set**, and in this command the only thing that sets
//! it is a range error — `strtoumax` over/underflow (`_strtol.h:157`/`:172`, giving
//! `Result too large or too small`). Everything else leaves `errno` alone and lands on
//! `errx(1, "%s: illegal numeric format.")` (`:140`, note the period), *including* a
//! word with no convertible digits: NetBSD/Minix's `_strtol.h:184-186` stores the
//! original `nptr` in `endptr` when nothing was consumed and does not touch `errno`
//! (the `EINVAL` at `:69` guards an invalid `base` argument, which `strtoumax(…, 0)`
//! never hits), so `*p != '\0'` fires. Reading that line too loosely suggests
//! `primes: abc: Invalid argument`, but a host-built C reference (`primes_ref`
//! from these very sources) says otherwise.
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
//! *permutation* (an option letter after an operand) is tracked separately.
//!
//! The `-d`/`-h` legs are wired now, and their golden values were not written by
//! hand: they were read off a **reference process** built from the very C sources
//! (`cc -O2 -D'__RCSID(x)=' -D'__COPYRIGHT(x)='
//!  -D'__dead=__attribute__((__noreturn__))' -include stdlib.h
//!  minix3/games/primes/{primes,pattern,pr_tbl,spsp}.c -lm -o /tmp/primes_ref`),
//! then diffed against this binary over 21 argument shapes (`SAME=21 DIFF=0`).
//! That diff is also what surfaced the exclusive-`stop` bug fixed here: C says
//! `>= start and < stop` in its own header (`primes.c:49`), and the reference
//! prints `5 7 11` for `primes 5 13`, while treating both ends as inclusive
//! prints `13` too. A test that justifies its output as "both ends included"
//! can still match by accident — 20 is not prime — while the rationale is
//! wrong. An assertion can be green and still teach the opposite of C.

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

/// C's zero-operand branch reads the start value from standard input
/// (`read_num_buf`, `primes.c:176-205`), so stdin has to be feedable. This path does
/// not inherit `run_at`'s `Stdio::null()` — that is the only reason this leg could not
/// be tested before.
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

/// The four endings of the zero-operand branch each map to one C branch. Note that two
/// of them pass the **whole `buf` (newline included)** as the argument, so one C line
/// breaks into two physical lines — not a formatting mistake but the literal shape of
/// `err(1, "%s", buf)` / `errx(1, "%s: illegal numeric format.", buf)`.
#[test]
fn the_stdin_legs_split_into_c_s_four_outcomes() {
    // (1) Only a newline/blank line: C `continue`s and reads another line; feeding
    // "blank line + real value" proves the skip really happens. The value is 999983
    // because this tree caps at 1e6 (C's default upper bound is near 2^32, a declared
    // difference): the reference gives `999983 1000003 1000033…` on this shape, while
    // ours stops at the cap and that one prime is all the interval holds (measured).
    let (out, status, err) = run_stdin(&[], "\n  \n999983\n");
    assert_eq!(status, 0);
    assert_eq!(err, "", "跳过空白行不该出声");
    assert_eq!(out, "999983\n", "起始值取自第三行");

    // (2) Empty input (EOF, no `ferror`): C `exit(0)`, silent.
    let (out, status, err) = run_stdin(&[], "");
    assert_eq!((status, out, err), (0, String::new(), String::new()));

    // (3) Not a single digit converts: this repo's source of truth `_strtol.h:69` sets
    //   `EINVAL` here, so it goes to `err(1, "%s", buf)`.
    //   (Host glibc's `strtoumax` does not set that errno, so the reference lands
    //   elsewhere than the source of truth; the source of truth wins.)
    let (_, status, err) = run_stdin(&[], "abc\n");
    assert_eq!(status, 1);
    assert_eq!(err, "primes: abc\n: illegal numeric format.\n");

    // (4) A converted prefix followed by a non-newline: `errx`, suffixing the whole
    // buf, with a period at the end of the sentence.
    let (_, status, err) = run_stdin(&[], "10x\n");
    assert_eq!(status, 1);
    assert_eq!(err, "primes: 10x\n: illegal numeric format.\n");

    // The negative branch refuses before reading (`:193-194`); leading blanks do not
    // affect it.
    let (_, status, err) = run_stdin(&[], " -5\n");
    assert_eq!(status, 1);
    assert_eq!(err, "primes: negative numbers aren't permitted.\n");

    // Overflow branch: C sets `ERANGE` and goes to the `err` family; the sentence comes
    // from this tree's Minix3 table.
    let (_, status, err) = run_stdin(&[], "999999999999999999999999\n");
    assert_eq!(status, 1);
    assert_eq!(
        err,
        "primes: 999999999999999999999999\n: Result too large or too small\n"
    );
}

/// `strtoumax(…, 0)`: `0x`/`0X` is hexadecimal, a leading `0` is octal, and both leading
/// blanks and `+` are legal. The operand branch and the stdin branch share the same
/// conversion, so both have to accept it.
#[test]
fn bounds_are_converted_in_base_zero_like_c() {
    let (out, status, err) = run(&["0x10", "0x20"]);
    assert_eq!(status, 0, "stderr: {err}");
    assert_eq!(out, "17\n19\n23\n29\n31\n", "16<=p<32");

    let (out, _, _) = run(&["010", "014"]);
    assert_eq!(out, "11\n", "八进制 010=8、014=12，中间只有 11");

    // Both operands go through the same conversion too: `0x10..0x20` is 16<=p<32.
    let (out, status, _) = run(&["0x10", "0x20"]);
    assert_eq!(status, 0);
    assert_eq!(out, "17\n19\n23\n29\n31\n");

    // The stdin leg can be combined with `-d` (C runs getopt first, then takes
    // operands/stdin), and **the stored-table pivot applies on this leg too**: the
    // start value 999979 lies outside the stored table (tail value 65537), so the
    // first prime carries no difference column while the following ones do.
    // The reference reads the same (`printf 999979 | primes_ref -d` → `999979` /
    // `999983 (4)` / `1000003 (20)`…; ours stops at the 1e6 cap, a declared upper-bound
    // difference).
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
    // "Not a single digit converted" does **not** append a sentence: the source of truth
    // `_strtol.h:184-186` leaves `endptr` at the original `nptr` and does not touch
    // errno (`:69`'s `EINVAL` is the invalid-base-argument branch, unreachable with base
    // 0), so `primes.c:139`'s `*p != '\0'` holds and the same `errx` line as above is
    // used. Treating this as the `err` family was a misreading of `_strtol.h`; the
    // reference (host-built `primes_ref`) agrees with the source of truth here, hence
    // the rename and the corrected assertion.
    let (out, status, err) = run(&["abc"]);
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(err, "primes: abc: illegal numeric format.\n");
}

#[test]
fn only_a_range_error_appends_the_strerror_sentence() {
    // The only thing on the operand path that really sets errno is a range error
    // (`_strtol.h:157`), and only that takes the `err(1, "%s", argv[i])` family; the
    // sentence comes from this tree's Minix3 table (messages outside the table have
    // their own branch).
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
    // (1,000,000 — a declared difference; the shape is C's).
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

    // Letters C accepts are not rejected: `-d`/`-h` are real output faces (their shapes
    // are pinned by the tests below), so a normal run with them exits 0 and writes only
    // to stdout, with no "not wired" line any more.
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
    // The exclusivity disagreement, pinned where C and an inclusive reading differ.
    // C: `primes.c:49` "Print primes >= start and < stop"; the reference process reads
    // `primes 5 13` → `5 7 11` and `primes 2 2` → nothing at all. Treating `stop` as
    // inclusive prints `13` / `2`.
    assert_eq!(run(&["5", "13"]).0, "5\n7\n11\n");
    assert_eq!(
        run(&["2", "2"]).0,
        "",
        "空输出不是错误：C 只拦 `start > stop`"
    );
    assert_eq!(run(&["13", "13"]).0, "");
    assert_eq!(run(&["7", "8"]).0, "7\n", "上界不是素数时两形看起来一样");
    // `start > stop` is still that `errx` line (`primes.c:171-172`), which does not
    // conflict with the exclusive interval.
    let (_, status, err) = run(&["20", "10"]);
    assert_eq!(status, 1);
    assert_eq!(err, "primes: start value must be less than stop value.\n");
}

#[test]
fn difference_column_prints_c_including_its_first_prime_quirk() {
    // `-d`'s difference pairs against C's `prev` (`primes.c:220`/`:261`). Neither detail
    // is made up: the first item `2 (2)` because `prev` starts at 0; in `5 15`,
    // `5 (2)`'s predecessor 3 lies outside the interval and C still subtracts it.
    // Verbatim from the reference process on the same input.
    assert_eq!(run(&["-d", "5", "15"]).0, "5 (2)\n7 (2)\n11 (4)\n13 (2)\n");
    assert_eq!(run(&["-d", "2", "10"]).0, "2 (2)\n3 (1)\n5 (2)\n7 (2)\n");
    assert_eq!(run(&["-d", "1", "10"]).0, "2 (2)\n3 (1)\n5 (2)\n7 (2)\n");
    // 113's predecessor is 109, not "the previous odd number", so the difference is 4;
    // 127 follows 113, difference 14.
    assert_eq!(run(&["-d", "113", "130"]).0, "113 (4)\n127 (14)\n");
}

#[test]
fn hex_output_is_lowercase_and_carries_no_prefix() {
    // `printf("%" PRIx64, …)` (`primes.c:255`/`:327`): lowercase, no `0x`, so 17 prints
    // `11`. On output alone this is easily mistaken for "a missing 0x", so the decimal
    // run is pinned alongside as a control.
    assert_eq!(run(&["-h", "5", "20"]).0, "5\n7\nb\nd\n11\n13\n");
    assert_eq!(run(&["5", "20"]).0, "5\n7\n11\n13\n17\n19\n");
}

#[test]
fn hex_flag_does_not_make_the_difference_hexadecimal() {
    // The two flags each govern one thing: `-h` changes only the base of the primes
    // themselves, while the difference column is always decimal `PRIu64` in C
    // (`:257`/`:329`). Reference process: `primes -dh 113 130` → `71 (4)`/`7f (14)`;
    // that `14` would turn into `e` if it followed hex.
    assert_eq!(run(&["-dh", "113", "130"]).0, "71 (4)\n7f (14)\n");
}

#[test]
fn first_prime_above_the_stored_table_has_no_difference_column() {
    // C's sieve-path guard `dflag && (prev || (start <= *pr_limit))` (`:328`) reduces to
    // `prev != 0` on this route, so starting outside the table (stored tail value
    // 65537, `pr_tbl.c`) leaves the first prime without a difference column.
    // Both shapes are verbatim from the reference process: `primes -d 70000 70040`'s
    // first line is `70001` with no column, while `primes -d 65530 65545` starts inside
    // the table and its first item carries `65537 (16)`.
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
    // Both branches of `getopt("dh")` are `flag++` (`primes.c:107-112`), so writing the
    // same flag several times still yields one flag. Equivalence classes must be
    // separated by letter set, not lumped together: the reference yields three classes
    // — `{dh, hd, -d -h, -d -dh}` all print "hex + difference", `{d, dd}` print
    // "decimal + difference", `{h, hh}` print "hex without difference". (Lumping `-dd`
    // into the `-dh` class turns the test red on the spot: the code is right, the
    // classification is what is wrong.)
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
