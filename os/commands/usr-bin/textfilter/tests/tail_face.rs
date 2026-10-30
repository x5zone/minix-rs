//! End-to-end face for the `tail` binary: real process, real pipes and real
//! files, checked against expectations derived from `minix3/usr.bin/tail/`
//! (`tail.c`, `forward.c`, `reverse.c`).
//!
//! The host's GNU `tail` is used only where the two specifications agree. It is
//! deliberately not compared on: `-b` (GNU has no block mode) and `-r` (GNU has no
//! reverse). Line shapes are copied, not rebuilt: a line carries whether its
//! input had a newline, so an unterminated last line stays unterminated on the
//! way out, exactly as C's `WR`/`reverse.c` range copy does.
//!
//! The follow legs are pinned against the loop the C actually runs on Minix, not
//! against GNU: `forward.c`'s kqueue arm is excluded by `#if !defined(__minix)`
//! (`:207-246`), so a round is sleep-then-read-from-position (`:191-200`, `:253`)
//! and only `-F` on a named operand compares identities and reopens (`:255-266`).
//! That is what makes three of the shapes below counter-intuitive if GNU were the
//! yardstick: `-f` keeps reading the file it opened (so a rename does not move it
//! off that inode), `-F` prints the replacement from its start, and a file that
//! shrinks does not rewind the position — the bytes printed after a shrink begin
//! mid-line.

mod face_support;

use std::fs;
use std::io::{BufRead as _, BufReader, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

const TAIL: &str = env!("CARGO_BIN_EXE_tail");
/// C's tick is exactly `sleep(1)` (`forward.c:253`); this is only a reference:
/// forward waits are taken as multiples of it, and the quiet window must also be
/// longer than it (otherwise the "a tick that should print nothing" arm cannot
/// catch a stray round).
const ROUND: Duration = Duration::from_secs(1);
/// **Patience ceiling** while waiting for a line to arrive, not a decision
/// criterion. The assertions check *which* line arrives, not *at which second*,
/// so loosening this weakens no assertion; conversely, using it as a criterion
/// would misreport "the machine is busy with other threads right now and the
/// child gets CPU only every dozen seconds" as a defect in the follow loop —
/// a whole-package parallel run in this repo produced exactly that kind of false
/// red (the same target alone and a `--no-fail-fast` rerun were both green, and
/// this wait is the only quantity that depends on scheduling speed). The only
/// cost is a few extra seconds on a real failure, and the failure path prints
/// stderr and the channel error anyway. It is 20 ticks, so the number below is
/// multiplied out of `ROUND`, not a magic count of ticks.
const PATIENCE: Duration = Duration::from_secs(ROUND.as_secs() * 20);
/// The "must print nothing" window: it must be a multiple of `ROUND` and larger
/// than one tick, otherwise the conclusion is drawn before a whole period has
/// run. In this direction **shortening it weakens the criterion**, so it does
/// not take part in the patience adjustment above (still 2.5 ticks).
const QUIET: Duration = Duration::from_millis((ROUND.as_millis() * 5 / 2) as u64);

/// Runs `tail` with the given words and standard input.
fn run(args: &[&str], input: &str) -> (String, i32, String) {
    run_with(TAIL, args, input)
}

/// The same run for any program, so the differential leg drives the host `tail`
/// through the identical pipe and comparison.
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

fn scratch(name: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!("tail-face-{}-{}", std::process::id(), name));
    path
}

/// Attach the stdout of a still-running child to a channel: a reader thread
/// sends one line at a time, so "no new line" is a timeout error rather than a
/// hung test.
fn line_channel(pipe: std::process::ChildStdout) -> Receiver<String> {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut reader = BufReader::new(pipe);
        loop {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    if sender.send(line).is_err() {
                        break;
                    }
                }
            }
        }
    });
    receiver
}

/// Start a following `tail` with stdout on a line channel and stderr into a file
/// (the follow loop never exits, so `output()` would wait forever).
fn spawn_follow(args: &[&str], stderr_to: &Path) -> (Child, Receiver<String>) {
    let err_file = fs::File::create(stderr_to).expect("stderr file");
    let mut child = Command::new(TAIL)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::from(err_file))
        .spawn()
        .expect("the tail binary should be runnable");
    let lines = line_channel(child.stdout.take().expect("stdout piped"));
    (child, lines)
}

/// The next line; past the patience ceiling this fails (the attached reading
/// includes stderr for diagnosis). What is awaited here is "will it come", not
/// "how soon it comes", hence [`PATIENCE`] rather than a multiple of ticks.
fn next_line(lines: &Receiver<String>, child: &mut Child, stderr_to: &Path) -> String {
    match lines.recv_timeout(PATIENCE) {
        Ok(line) => line,
        Err(error) => {
            let text = fs::read_to_string(stderr_to).unwrap_or_default();
            let _ = child.kill();
            panic!("no line within {PATIENCE:?} (stderr was {text:?}, channel {error:?})");
        }
    }
}

/// During `wait` not a single line may arrive (C's silent arm: it slept but
/// printed nothing).
fn expect_quiet(lines: &Receiver<String>, wait: Duration) {
    match lines.recv_timeout(wait) {
        Err(_) => {}
        Ok(line) => panic!("expected silence, got {line:?}"),
    }
}

/// Wait for a span of more than one tick, to confirm the process is still alive
/// (rather than having exited silently).
fn outlives_ticks(child: &mut Child, ticks: Duration) {
    let deadline = Instant::now() + ticks;
    while Instant::now() < deadline {
        if matches!(child.try_wait(), Ok(Some(_))) {
            panic!("the follow loop exited; C keeps sleeping here");
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Append a line (flushed to the file) for the follow loop to read on its next
/// tick.
fn append(path: &Path, text: &str) {
    fs::OpenOptions::new()
        .append(true)
        .open(path)
        .expect("append handle")
        .write_all(text.as_bytes())
        .expect("appending");
}

/// Twelve newline-terminated lines, so the default ten-line window is visible.
fn twelve() -> String {
    (1..=12).map(|number| format!("{number}\n")).collect()
}

#[test]
fn default_is_the_last_ten_lines() {
    // `tail.c:160-162`: no number option and no `-r` is `RLINES` with `off = 10`.
    let (out, status, err) = run(&[], &twelve());
    assert_eq!(out, "3\n4\n5\n6\n7\n8\n9\n10\n11\n12\n");
    assert_eq!(status, 0, "stderr was {err:?}");
}

#[test]
fn line_byte_and_block_counts_take_the_trailing_window() {
    assert_eq!(run(&["-n", "2"], &twelve()).0, "11\n12\n");
    // Obsolete leading-digit form is folded to `-n` by `obsolete()`.
    assert_eq!(run(&["-2"], &twelve()).0, "11\n12\n");
    assert_eq!(run(&["-c", "3"], "abcdef").0, "def");
    // `-b N` counts 512-byte blocks, so one block of a short file is all of it.
    assert_eq!(run(&["-b", "1"], "abcdef").0, "abcdef");
    assert_eq!(run(&["-c", "100"], "abcdef").0, "abcdef");
}

#[test]
fn forward_offsets_are_one_based_and_skip_nothing_at_one() {
    // `ARG`'s forward arm: `+1` leaves `off` at zero, which prints the whole file.
    assert_eq!(run(&["-n", "+1"], &twelve()).0, twelve());
    assert_eq!(run(&["-n", "+10"], &twelve()).0, "10\n11\n12\n");
    assert_eq!(run(&["-c", "+4"], "abcdef").0, "def");
    assert_eq!(run(&["-b", "+2"], "abcdef").0, "");
    // An offset past the end prints nothing, and that is a successful exit.
    let (out, status, err) = run(&["-n", "+99"], &twelve());
    assert_eq!(out, "");
    assert_eq!(status, 0, "stderr was {err:?}");
}

#[test]
fn zero_counts_select_nothing_and_exit_zero() {
    // C does not reject them: `forward.c` seeks to the end for `RLINES`/`RBYTES`
    // when `off` is zero, and a pipe is simply drained.
    for args in [&["-n", "0"][..], &["-c", "0"][..], &["-b", "0"][..]] {
        let (out, status, err) = run(args, &twelve());
        assert_eq!(out, "", "args {args:?} printed {out:?}");
        assert_eq!(status, 0, "args {args:?} stderr was {err:?}");
    }
}

#[test]
fn reverse_forms_walk_the_input_bottom_up() {
    // GNU `tail` has no `-r`, so these are pinned to C alone (`reverse.c`).
    assert_eq!(run(&["-r"], "a\nb\n").0, "b\na\n");
    assert_eq!(run(&["-r", "-n", "1"], "a\nb\n").0, "b\n");
    // `tail -r -c 3` on the four bytes `abc\n` is `r_reg` with `off = 3`: it clamps
    // the walk to three bytes and starts at the last one, so the newline is the
    // “special last char”, then `b` and `c` are walked backwards and the leftover
    // fragment `bc\n` is written by the closing `if (llen) WR(p, llen)`. The `a`
    // is outside the byte window and never appears.
    assert_eq!(run(&["-r", "-c", "3"], "abc\n").0, "bc\n");
    // Folding `+N` under `-r` yields a backward count (`tail.c:146-149`): the `+3`
    // becomes `RLINES` with `off = 2`, so two lines come back bottom-up, while
    // `+1` leaves `off` at zero and `reverse.c:79-80` returns without printing.
    assert_eq!(run(&["-r", "-n", "+1"], "a\nb\n").0, "");
    assert_eq!(run(&["-r", "-n", "+3"], "a\nb\n").0, "b\na\n");
}

/// Same command seam as `run`, but standard error comes back **byte for byte**:
/// `from_utf8_lossy` renders a bare `c3` as the three-byte U+FFFD, and the text
/// form cannot tell the "one byte" and "two bytes" failure modes apart.
fn run_bytes(args: &[&str], input: &str) -> (Vec<u8>, i32, Vec<u8>) {
    let mut child = Command::new(TAIL)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the tail binary should be runnable");
    face_support::write_stdin(&mut child, input.as_bytes());
    let output = child.wait_with_output().expect("the run should finish");
    (
        output.stdout,
        output.status.code().unwrap_or(-1),
        output.stderr,
    )
}

/// The letter slot of the unknown-option line is **one raw byte**. Each
/// expected shape was measured against the reference binary built for this
/// round, `tmp/tail-416/tail_ref` (five ground-truth files + Minix's
/// `getopt.c` + shims, `-D__minix` turning off the BSD kqueue leg, started
/// through a same-name symlink `tail` so the program-name segment matches this
/// shell): `-é` and `-ré` both give `tail: unknown option -- ` + a single `c3`
/// + newline followed by the usage line (capitalized `Usage:`, `tail.c:299-305`)
/// and exit **1**; `-Zé` stops at `Z`; conversely in `-né`, `é` is a **value** —
/// the optstring `"Fb:c:fn:r"` at `tail.c:108` has `n` take a value, so it goes
/// to `tail: illegal offset -- é` on **one line, with no usage**.
#[test]
fn the_libc_line_of_a_multibyte_flag_is_one_byte_for_tail() {
    const USAGE: &[u8] = b"Usage: tail [-f | -F | -r] [-b # | -c # | -n #] [file ...]\n";
    let want: Vec<u8> = [b"tail: unknown option -- ".as_slice(), b"\xc3\n", USAGE].concat();
    for form in ["-\u{e9}", "-r\u{e9}"] {
        let (out, status, err) = run_bytes(&[form], "a\n");
        assert_eq!(out, Vec::new(), "{form} 的报文只走标准错误");
        assert_eq!(status, 1, "{form} 的 stderr 是 {err:?}");
        assert_eq!(err, want, "{form} 的字母该是一个裸字节 c3");
    }

    let (_out, status, err) = run_bytes(&["-Z\u{e9}"], "a\n");
    assert_eq!(status, 1);
    assert_eq!(
        err,
        [b"tail: unknown option -- Z\n".as_slice(), USAGE].concat(),
        "环停在第一个坏字母上，第二个字节根本不该被读到"
    );

    let (_out, status, err) = run_bytes(&["-n\u{e9}"], "a\n");
    assert_eq!(status, 1);
    assert_eq!(
        err,
        [
            b"tail: illegal offset -- ".as_slice(),
            "\u{e9}".as_bytes(),
            b"\n".as_slice(),
        ]
        .concat(),
        "带值字母把两字节整个当值收走，句子是命令自己的那一句、不接 usage"
    );
}

/// Same leg as `run_bytes`, but **argv is handed as raw bytes**: a bare `0xff`
/// cannot enter a `&str`, so this leg cannot go through any fixture that takes
/// `str` arguments. It runs in a temporary directory holding a real file whose
/// **name itself contains a raw byte** (`a\xff.txt`) and a plain `in.txt`, so
/// the banner leg is also on the table. Returns stdout, exit code and byte-exact
/// standard error.
fn run_raw(args: &[&[u8]], input: &[u8]) -> (Vec<u8>, i32, Vec<u8>) {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt as _;
    let dir = std::env::temp_dir().join(format!(
        "tail-raw-{}-{}",
        std::process::id(),
        RAW_SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    std::fs::write(dir.join("in.txt"), b"line1\nline2\nline3\n").expect("in.txt");
    std::fs::write(dir.join(OsStr::from_bytes(b"a\xff.txt")), b"raw1\nraw2\n")
        .expect("a raw-byte file name can be created on the host");
    let mut child = Command::new(TAIL)
        .args(args.iter().map(|a| OsStr::from_bytes(a)))
        .current_dir(&dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the tail binary should be runnable");
    face_support::write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    let code = (
        output.stdout,
        output.status.code().unwrap_or(-1),
        output.stderr,
    );
    let _ = std::fs::remove_dir_all(&dir);
    code
}

static RAW_SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Byte criteria for the argument seam (each shape measured against the
/// reference binary `tmp/tail-416/tail` started through a same-name symlink,
/// three-way comparison in `tmp/tail-469/measure.log`): before the fix only 10
/// of 22 shapes matched the reference binary, after it 19. These five shapes
/// are the five classes turned around this round:
/// (1) one raw `ff` in the letter slot (`getopt.c:79`'s `optopt = *place++`
///     takes one byte at a time, `:103`'s `%c` writes it out verbatim);
/// (2) the value slot of the three `illegal offset -- %s` sentences (`tail.c:90`
///     prints `optarg` itself);
/// (3) the `obsolete()` sentence `illegal option -- %s` carries the **whole
///     token** (`tail.c:269`);
/// (4) the bytes in the token **after** the legacy form is rewritten — C
///     assembles it with `malloc` + `strcpy` (`:238-273`), so `-1<ff>2` becomes
///     `-n-1<ff>2` and the echo carries `-1<ff>2` rather than the original
///     token;
/// (5) the **file name** in diagnostic lines (`ierr()` prints the global
///     `fname`).
/// The final assertion pins the leg still on the books: a **real file** whose
/// name contains a raw byte. The reference binary opens it and prints
/// `==> a\xff.txt <==`, while this shell cannot open it (the name enters the
/// `&str` door in `os/libs`, a registered limitation), so it can only report
/// "no such file" — pinned to the current shape, and this gate turns red the
/// moment that door switches to bytes.
#[test]
fn the_offset_echo_the_token_echo_and_the_file_name_carry_the_argv_bytes() {
    const USAGE: &[u8] = b"Usage: tail [-f | -F | -r] [-b # | -c # | -n #] [file ...]\n";
    let (out, status, err) = run_raw(&[b"-\xff"], b"a\n");
    assert_eq!(out, Vec::new(), "字母位那一行不走标准输出");
    assert_eq!(status, 1);
    assert_eq!(
        err,
        [b"tail: unknown option -- \xff\n".as_slice(), USAGE].concat(),
        "字母该是那一枚字节本身，不是折过的第一字节 ef"
    );

    // In a cluster, the first high byte is the one reported.
    let (_out, status, err) = run_raw(&[b"-\xffn"], b"a\n");
    assert_eq!(status, 1);
    assert_eq!(
        err,
        [b"tail: unknown option -- \xff\n".as_slice(), USAGE].concat()
    );

    // The value slot of the three illegal offset sentences: attached value,
    // split word and `-c`, one shape each.
    for flag in [b"-n\xff".as_slice(), b"-c\xff".as_slice()] {
        let (_out, status, err) = run_raw(&[flag], b"a\n");
        assert_eq!(status, 1, "{flag:?} 的退码");
        assert_eq!(
            err,
            b"tail: illegal offset -- \xff\n".to_vec(),
            "{flag:?} 的值位该是原字节"
        );
    }
    let (_out, status, err) = run_raw(&[b"-n", b"\xff"], b"a\n");
    assert_eq!(status, 1);
    assert_eq!(
        err,
        b"tail: illegal offset -- \xff\n".to_vec(),
        "分词形同样按字节"
    );

    // The `obsolete()` sentence itself carries the whole token.
    let (_out, status, err) = run_raw(&[b"-20\xffx"], b"a\n");
    assert_eq!(status, 1);
    assert_eq!(
        err,
        b"tail: illegal option -- -20\xffx\n".to_vec(),
        "这一句的回声位是 token 而不是它的尾巴"
    );

    // The token after rewriting: `-1<ff>2` is assembled by C into `-n-1<ff>2`,
    // so the echo carries the sign and that ff.
    let (_out, status, err) = run_raw(&[b"-1\xff2"], b"a\n");
    assert_eq!(status, 1);
    assert_eq!(
        err,
        b"tail: illegal offset -- -1\xff2\n".to_vec(),
        "回声带的是改写之后的值位，其中那枚 ff 仍是 ff"
    );

    // An unopenable file name: `ierr()` reads the raw argv bytes.
    let (_out, status, err) = run_raw(&[b"no\xff/such"], b"a\n");
    assert_eq!(status, 1);
    assert_eq!(
        err,
        [
            b"tail: no\xff/such: ".as_slice(),
            b"No such file or directory\n".as_slice()
        ]
        .concat()
    );

    // The registered shape: a **real file** whose name contains a raw byte
    // cannot be opened by this shell, while the reference binary opens it and
    // prints the banner.
    let (out, status, err) = run_raw(&[b"-n", b"1", b"a\xff.txt", b"in.txt"], b"a\n");
    assert_eq!(
        err,
        [
            b"tail: a\xff.txt: ".as_slice(),
            b"No such file or directory\n".as_slice()
        ]
        .concat(),
        "报文里的档名是原字节，而开档那条口还在折"
    );
    assert_eq!(status, 1);
    assert_eq!(
        out,
        [b"==> in.txt <==\n".as_slice(), b"line3\n".as_slice()].concat(),
        "参照件这里有两段横幅（第一段带的是那枚裸字节），本壳只有 in.txt 那一段"
    );
}

#[test]
fn unknown_letters_and_bad_counts_are_refused_not_leaked() {
    // `-x` is C's `case '?'` → `usage()` (`tail.c:128-130`), but one line comes
    // before it, printed by `getopt` itself: `"<prog>: unknown option -- <letter>"`
    // (`minix3/lib/libc/stdlib/getopt.c:101-104`, the letter unquoted). The
    // repo's **old** host reference binary could not compare this line — it ran
    // glibc's `getopt`, whose sentence is `invalid option -- 'x'`. This round
    // supplied `tmp/tail-416/tail_ref` linked against Minix `getopt.c` (the
    // `tmp/tail-follow-ref/tail_ref` is glibc lineage too; after verifying
    // provenance it was judged unusable), so this sentence now has a byte-exact
    // differential leg — the letter-slot gate pins it with `run_bytes`.
    // The usage line itself is the capitalized `Usage:`, carries the program
    // name and already lists `-f | -F` (`tail.c:299-305`); the shell's earlier
    // lowercase line listing only `-r` was invented.
    const USAGE: &str = "Usage: tail [-f | -F | -r] [-b # | -c # | -n #] [file ...]\n";
    let (out, status, err) = run(&["-x"], "a\n");
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(err, format!("tail: unknown option -- x\n{USAGE}"));
    // A value option missing its value: likewise two lines, the sentence being
    // `option requires an argument -- n` (`getopt.c:132-136`).
    let (out, status, err) = run(&["-n"], "a\n");
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(
        err,
        format!("tail: option requires an argument -- n\n{USAGE}")
    );
    // The two arms with only the usage line: a repeated number option (the
    // first sentence of `ARG`) and `-r` with follow.
    for args in [&["-n", "3", "-n", "5"][..], &["-r", "-f", "a"][..]] {
        let (out, status, err) = run(args, "a\n");
        assert_eq!(out, "", "args {args:?} printed {out:?}");
        assert_eq!(status, 1, "args {args:?} stderr was {err:?}");
        assert_eq!(err, USAGE, "args {args:?}");
    }
    // An illegal count goes through `xerrx`, whose sentence appends no `errno`
    // sentence and has no usage line.
    let (out, status, err) = run(&["-n", "x"], "a\n");
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(err, "tail: illegal offset -- x\n");
}

#[test]
fn follow_with_two_operands_is_refused_before_anything_is_read() {
    // `tail.c:135-137`'s single-file gate comes before opening any operand, and
    // it is `xerrx` (`misc.c:78-89` goes through `vwarnx`), so no `errno`
    // sentence follows. stdout must be empty: not even the first file was
    // opened.
    let one = scratch("multi-a");
    let two = scratch("multi-b");
    fs::write(&one, "a1\n").unwrap();
    fs::write(&two, "b1\n").unwrap();
    let (out, status, err) = run(&["-f", one.to_str().unwrap(), two.to_str().unwrap()], "");
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(
        err,
        "tail: -f and -F options only appropriate for a single file\n"
    );
    // `-F` takes the same sentence (whether `fflag` is 1 or 2, it passes this
    // gate).
    assert_eq!(
        run(&["-F", one.to_str().unwrap(), two.to_str().unwrap(),], "").2,
        "tail: -f and -F options only appropriate for a single file\n"
    );
    // The gate comes before the `-r` usage check (`:135` vs `:143`).
    assert_eq!(
        run(
            &["-r", "-f", one.to_str().unwrap(), two.to_str().unwrap(),],
            ""
        )
        .2,
        "tail: -f and -F options only appropriate for a single file\n"
    );
    let _ = fs::remove_file(&one);
    let _ = fs::remove_file(&two);
}

#[test]
fn follow_with_reverse_is_the_usage_line() {
    // `tail.c:143-145`: `if (rflag) { if (fflag) usage(); }`. Both cases exit 1
    // and print nothing.
    let file = scratch("rev-f");
    fs::write(&file, "r1\nr2\n").unwrap();
    let path = file.to_str().unwrap().to_owned();
    for flag in ["-f", "-F"] {
        let (out, status, err) = run(&["-r", flag, &path], "");
        assert_eq!(out, "", "{flag} 与 -r 同给不该打任何内容");
        assert_eq!(status, 1, "{flag} 的 stderr 是 {err:?}");
        assert_eq!(
            err,
            "Usage: tail [-f | -F | -r] [-b # | -c # | -n #] [file ...]\n"
        );
    }
    let _ = fs::remove_file(&file);
}

#[test]
fn follow_on_a_pipe_is_dropped_and_the_stream_still_prints() {
    // `tail.c:197-201` clears `fflag` on a pipe (the comment says POSIX.2
    // requires this). Without this arm the case would wait forever — exactly the
    // pit this repo translates as "hangs" rather than "errors".
    for flag in ["-f", "-F"] {
        let (out, status, err) = run(&[flag], "p1\np2\n");
        assert_eq!(out, "p1\np2\n", "{flag} 在管道上该打完就退");
        assert_eq!(status, 0, "{flag} 的 stderr 是 {err:?}");
    }
}

#[test]
fn follow_prints_what_arrives_after_the_first_round() {
    // The `-f` tick: C copies to EOF first, then sleeps (`forward.c:191-202`,
    // `:253`), so the initial content comes out first and new lines arrive on
    // the next tick.
    let file = scratch("follow-append");
    let err_file = scratch("follow-append-err");
    fs::write(&file, "l1\nl2\nl3\n").unwrap();
    let (mut child, lines) = spawn_follow(&["-f", file.to_str().unwrap()], &err_file);
    assert_eq!(next_line(&lines, &mut child, &err_file), "l1\n");
    assert_eq!(next_line(&lines, &mut child, &err_file), "l2\n");
    assert_eq!(next_line(&lines, &mut child, &err_file), "l3\n");
    append(&file, "after1\n");
    assert_eq!(next_line(&lines, &mut child, &err_file), "after1\n");
    append(&file, "after2\nafter3\n");
    assert_eq!(next_line(&lines, &mut child, &err_file), "after2\n");
    assert_eq!(next_line(&lines, &mut child, &err_file), "after3\n");
    let _ = child.kill();
    let _ = child.wait();
    assert_eq!(
        fs::read_to_string(&err_file).unwrap(),
        "",
        "跟随环路不该报错"
    );
    let _ = fs::remove_file(&file);
    let _ = fs::remove_file(&err_file);
}

#[test]
fn follow_by_handle_ignores_a_replacement_at_the_same_name() {
    // `forward.c:255`'s `fflag == 2` lets only `-F` check identity. This case,
    // together with the one above (`-f` still reads the old inode), separates the
    // two forms: in the same scenario with only `-f`, a replacement appearing
    // under the same name must stay silent — otherwise `-f` would have been made
    // into `-F`.
    let file = scratch("follow-namerepl");
    let moved = scratch("follow-namerepl-moved");
    let err_file = scratch("follow-namerepl-err");
    fs::write(&file, "z1\n").unwrap();
    let (mut child, lines) = spawn_follow(&["-f", file.to_str().unwrap()], &err_file);
    assert_eq!(next_line(&lines, &mut child, &err_file), "z1\n");
    fs::rename(&file, &moved).unwrap();
    fs::write(&file, "replacement1\nreplacement2\n").unwrap();
    expect_quiet(&lines, QUIET);
    outlives_ticks(&mut child, QUIET);
    let _ = child.kill();
    let _ = child.wait();
    assert_eq!(fs::read_to_string(&err_file).unwrap(), "");
    let _ = fs::remove_file(&file);
    let _ = fs::remove_file(&moved);
    let _ = fs::remove_file(&err_file);
}

#[test]
fn follow_by_name_prints_the_replacement_from_its_start() {
    // `forward.c:257-266`: `-F` calls `freopen` once identities differ, and
    // freopen brings the read position back to the start, so the replacement is
    // printed **in full** (not just the tail after the old position). Note this
    // also differs from GNU `tail -F`, which tries to replay the old tail.
    let file = scratch("follow-rotate");
    let gone = scratch("follow-rotated");
    let err_file = scratch("follow-rotate-err");
    fs::write(&file, "old1\nold2\n").unwrap();
    let (mut child, lines) = spawn_follow(&["-F", file.to_str().unwrap()], &err_file);
    assert_eq!(next_line(&lines, &mut child, &err_file), "old1\n");
    assert_eq!(next_line(&lines, &mut child, &err_file), "old2\n");
    fs::rename(&file, &gone).unwrap();
    fs::write(&file, "fresh1\nfresh2\n").unwrap();
    assert_eq!(next_line(&lines, &mut child, &err_file), "fresh1\n");
    assert_eq!(next_line(&lines, &mut child, &err_file), "fresh2\n");
    // After the reopen the identity changed (`*sbp = statbuf`), and appends to
    // that same identity are still visible.
    append(&file, "more\n");
    assert_eq!(next_line(&lines, &mut child, &err_file), "more\n");
    let _ = child.kill();
    let _ = child.wait();
    assert_eq!(fs::read_to_string(&err_file).unwrap(), "");
    let _ = fs::remove_file(&file);
    let _ = fs::remove_file(&gone);
    let _ = fs::remove_file(&err_file);
}

#[test]
fn follow_by_handle_stays_with_the_file_it_opened() {
    // This pins the implementation choice: follow uses the **already-open
    // handle**, not a path reopened every tick. `-f` does not compare identity
    // (`forward.c:255`'s `fflag == 2`), so after a rename C still reads the same
    // inode: appends to the new path are invisible, appends to the old inode
    // (the renamed name) are visible.
    let file = scratch("follow-inode");
    let moved = scratch("follow-inode-moved");
    let err_file = scratch("follow-inode-err");
    fs::write(&file, "x1\nx2\n").unwrap();
    let (mut child, lines) = spawn_follow(&["-f", file.to_str().unwrap()], &err_file);
    assert_eq!(next_line(&lines, &mut child, &err_file), "x1\n");
    assert_eq!(next_line(&lines, &mut child, &err_file), "x2\n");
    fs::rename(&file, &moved).unwrap();
    // The name no longer exists: if the implementation re-read by path every
    // tick, this would error or silently drop; C and this shell both keep
    // reading from the handle in hand.
    append(&moved, "via-old-inode\n");
    assert_eq!(
        next_line(&lines, &mut child, &err_file),
        "via-old-inode\n",
        "改名后仍应读到旧 inode 的新字节"
    );
    let _ = child.kill();
    let _ = child.wait();
    assert_eq!(fs::read_to_string(&err_file).unwrap(), "");
    let _ = fs::remove_file(&moved);
    let _ = fs::remove_file(&err_file);
}

#[test]
fn follow_does_not_rewind_the_position_when_the_file_shrinks() {
    // Repositioning to the end exists only in the kqueue arm excluded by
    // `#if !defined(__minix)` (`forward.c:238-243`), so the real ground truth on
    // Minix never rewinds the position: after truncating and rewriting to full
    // length, nothing is printed until the old position is passed, and the
    // moment it is passed a **partial line** comes out.
    let file = scratch("follow-shrink");
    let err_file = scratch("follow-shrink-err");
    fs::write(&file, "abcdef\nghijkl\n").unwrap();
    let (mut child, lines) = spawn_follow(&["-f", file.to_str().unwrap()], &err_file);
    assert_eq!(next_line(&lines, &mut child, &err_file), "abcdef\n");
    assert_eq!(next_line(&lines, &mut child, &err_file), "ghijkl\n");
    // The position is now at 14. After truncation write 21 bytes: the first 14
    // are already "read", only what follows 14 is visible.
    fs::write(&file, "1234567890ABCDEFGHIJ\n").unwrap();
    assert_eq!(
        next_line(&lines, &mut child, &err_file),
        "EFGHIJ\n",
        "缩小时不回退位置，打出的是半行"
    );
    append(&file, "TAILMARK\n");
    assert_eq!(next_line(&lines, &mut child, &err_file), "TAILMARK\n");
    let _ = child.kill();
    let _ = child.wait();
    assert_eq!(fs::read_to_string(&err_file).unwrap(), "");
    let _ = fs::remove_file(&file);
    let _ = fs::remove_file(&err_file);
}

#[test]
fn follow_by_name_waits_in_silence_while_the_name_is_gone() {
    // C's `stat(fname, &statbuf) != -1` (`forward.c:255-256`) blocks the whole
    // "name cannot be identified" branch: no error, no exit, no output, and the
    // next tick keeps sleeping.
    let file = scratch("follow-gone");
    let err_file = scratch("follow-gone-err");
    fs::write(&file, "y1\n").unwrap();
    let (mut child, lines) = spawn_follow(&["-F", file.to_str().unwrap()], &err_file);
    assert_eq!(next_line(&lines, &mut child, &err_file), "y1\n");
    fs::remove_file(&file).unwrap();
    expect_quiet(&lines, QUIET);
    outlives_ticks(&mut child, QUIET);
    let _ = child.kill();
    let _ = child.wait();
    assert_eq!(
        fs::read_to_string(&err_file).unwrap(),
        "",
        "stat 失败在 C 不是报错腿"
    );
    let _ = fs::remove_file(&err_file);
}

#[test]
fn unopenable_operand_prints_the_program_the_name_and_the_errno_sentence() {
    // `ierr()` is `xerr(0, "%s", fname)` → `vwarn` (`misc.c:53-57`, `:65-76`), and
    // that sentence needs all three parts: program name, operand, and the
    // `strerror(errno)` sentence. The shell used to print only the first two
    // (using C's `warn(3)` as if it were `warnx(3)`).
    let missing = scratch("errno-missing");
    let missing_text = missing.to_str().unwrap().to_owned();
    let (out, status, err) = run(&[&missing_text], "");
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(
        err,
        format!("tail: {missing_text}: No such file or directory\n")
    );
}

#[test]
fn counts_above_the_old_window_cap_are_honoured() {
    // C `forward()` + `RLINES` walks back through the mapped file for any count,
    // and the window now keeps only what the input offered: a count above the
    // ceiling this shell used to enforce prints the whole file instead of being
    // refused, and a huge one must not cost the count in memory either.
    let (out, status, err) = run(&["-n", "40"], &twelve());
    assert_eq!(
        out,
        twelve(),
        "a count above the old 32 must not be refused"
    );
    assert_eq!(status, 0, "stderr was {err:?}");
    let (out, status, err) = run(&["-n", "1000000"], &twelve());
    assert_eq!(out, twelve());
    assert_eq!(status, 0, "stderr was {err:?}");
    // The reverse shape never went through the window and keeps working.
    assert_eq!(
        run(&["-r", "-n", "40"], &twelve()).0,
        "12\n11\n10\n9\n8\n7\n6\n5\n4\n3\n2\n1\n"
    );
}

#[test]
fn several_operands_get_a_banner_and_a_blank_line_between() {
    // `tail.c:172-177` prints `"%s==> %s <==\n"` with the separator only after the
    // first file, and this NetBSD `tail` has no `-q`/`-v` to change it.
    let one = scratch("many-a");
    let two = scratch("many-b");
    fs::write(&one, "a1\na2\n").unwrap();
    fs::write(&two, "b1\nb2\n").unwrap();
    let one_text = one.to_str().unwrap().to_owned();
    let two_text = two.to_str().unwrap().to_owned();
    let (out, status, err) = run(&[&one_text, &two_text], "");
    assert_eq!(status, 0, "stderr was {err:?}");
    assert_eq!(
        out,
        format!("==> {one_text} <==\na1\na2\n\n==> {two_text} <==\nb1\nb2\n")
    );

    // A single operand gets no banner at all.
    let (out, _, _) = run(&[&one_text], "");
    assert_eq!(out, "a1\na2\n");

    for path in [&one, &two] {
        let _ = fs::remove_file(path);
    }
}

#[test]
fn an_unopenable_operand_warns_and_the_others_still_print() {
    // `fopen` failure is `ierr(); continue;` — non-fatal, exit status becomes 1,
    // and no banner is printed for the file that could not be opened.
    let good = scratch("partial-a");
    let missing = scratch("partial-missing");
    fs::write(&good, "g1\n").unwrap();
    let good_text = good.to_str().unwrap().to_owned();
    let missing_text = missing.to_str().unwrap().to_owned();
    let (out, status, err) = run(&[&missing_text, &good_text], "");
    assert_eq!(status, 1);
    assert!(err.contains("tail:"), "stderr was {err:?}");
    assert_eq!(out, format!("==> {good_text} <==\ng1\n"));
    let _ = fs::remove_file(&good);
}

#[test]
fn agrees_with_the_host_tail_on_the_shared_forms() {
    // Only the shapes whose NetBSD and GNU semantics match: a line count, a
    // forward line offset, a byte count, and the multi-file banner. Byte counts
    // and the offsets use newline-terminated input so the shared "copy the bytes"
    // behaviour is what is compared.
    let cases: Vec<(Vec<&str>, String)> = vec![
        (vec![], twelve()),
        (vec!["-n", "3"], twelve()),
        (vec!["-n", "+9"], twelve()),
        (vec!["-c", "4"], "abcdef\n".to_string()),
    ];
    let system = if Command::new("tail").arg("--version").output().is_ok() {
        "tail"
    } else {
        eprintln!("no host tail to compare against; skipping the differential leg");
        return;
    };
    for (options, input) in cases {
        let ours = run(&options, &input);
        let theirs = run_with(system, &options, &input);
        assert_eq!(
            ours.0, theirs.0,
            "options {options:?}: ours {:?}, host {:?}",
            ours.0, theirs.0
        );
        assert_eq!(ours.1, theirs.1, "options {options:?}");
    }
}

/// The default form (`tail -n 10`) does not turn an unterminated final line
/// into a terminated one. C's `RLINES` records the line range including that
/// newline (`minix3/usr.bin/tail/reverse.c:133-144`), and the reference binary
/// `tmp/tail-416/tail_ref` measures `alpha\nbeta\ngamma` (no trailing newline).
/// The registered item was closed by this test.
#[test]
fn tail_leaves_an_unterminated_final_line_unterminated() {
    let (out, code, err) = run(&[], "alpha\nbeta\ngamma");
    assert_eq!(
        (out.as_str(), code, err.as_str()),
        ("alpha\nbeta\ngamma", 0, "")
    );
    let (two, _, _) = run(&["-n", "2"], "alpha\nbeta\ngamma");
    assert_eq!(two, "beta\ngamma");
}

/// `-r` moves the unterminated last line to the very front of the output, and
/// that line still carries no newline: termination state is recorded on the
/// line and travels with it, regardless of its position in the output. The
/// reference binary measures `gammabeta\nalpha\n`.
#[test]
fn reversed_output_carries_the_unterminated_line_to_the_front() {
    let (out, code, err) = run(&["-r"], "alpha\nbeta\ngamma");
    assert_eq!(
        (out.as_str(), code, err.as_str()),
        ("gammabeta\nalpha\n", 0, "")
    );
    let (bytes, _, _) = run(&["-r", "-c", "5"], "alpha\nbeta\ngamma");
    assert_eq!(bytes, "gamma");
}

#[test]
fn an_operand_ends_option_scanning_like_c() {
    // The same criterion as `head` (`minix3/lib/libc/stdlib/getopt.c:71-74`: at
    // the first non-option token `getopt` returns -1 without advancing
    // `optind`, so `tail.c:110`'s `argv += optind` treats that token and
    // everything after it as operands). The reference binary measures
    // `tail_ref a.txt -r`: `==> a.txt <==` plus the last ten lines in forward
    // order, then `tail: -r: No such file or directory` and exit 1 — reversal
    // does **not** happen.
    let good = scratch("operand-first");
    fs::write(&good, "g1\ng2\n").unwrap();
    let good_text = good.to_str().unwrap().to_owned();
    let (out, status, err) = run(&[&good_text, "-r"], "");
    assert_eq!(out, format!("==> {good_text} <==\ng1\ng2\n"));
    assert_eq!(status, 1);
    assert_eq!(err, "tail: -r: No such file or directory\n");

    // Flags only take effect before the operands, and every token after them is
    // an operand: `-r` prints two lines in reverse, and `-n 3` is just two
    // unopenable file names (the reference binary's `-r a.txt -n 3` has the
    // same shape).
    let (out, status, err) = run(&["-r", &good_text, "-n", "3"], "");
    assert_eq!(out, format!("==> {good_text} <==\ng2\ng1\n"));
    assert_eq!(status, 1);
    assert_eq!(
        err,
        "tail: -n: No such file or directory\ntail: 3: No such file or directory\n"
    );
    let _ = fs::remove_file(&good);
}
