//! End-to-end face for the `tail` binary: real process, real pipes and real
//! files, checked against expectations derived from `minix3/usr.bin/tail/`
//! (`tail.c`, `forward.c`, `reverse.c`).
//!
//! The host's GNU `tail` is used only where the two specifications agree. It is
//! deliberately not compared on: `-b` (GNU has no block mode), `-r` (GNU has no
//! reverse), and the line shapes on input whose last line is unterminated — this
//! shell rebuilds each printed line with its newline (a declared simplification
//! shared with the reverse path), while C and GNU copy bytes.
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

use std::fs;
use std::io::{BufRead as _, BufReader, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

const TAIL: &str = env!("CARGO_BIN_EXE_tail");
/// C 的一拍就是 `sleep(1)`（`forward.c:253`），这里只作参照：正向等待按它的倍数取，
/// 静默窗口也必须长于它（不然「本该有一拍什么也不打」那一支根本抓不住 stray round）。
const ROUND: Duration = Duration::from_secs(1);
/// 等一行到来时的**耐心上限**，不是判据。断言的是「来的是哪一行」而不是「它第几秒到」，
/// 所以把这里放宽不会让任何断言变弱；反过来，拿它当判据就会把「机器此刻被别的线程占满、
/// 子进程十几秒才拿到一次 CPU」误报成跟随环路的缺陷——本仓的整包并跑就报过一次这种假红
/// （单跑同一目标与 `--no-fail-fast` 重跑均绿，而这一枚的等待本身就是唯一依赖调度快慢的量）。
/// 代价只有真失败时多等几秒，而失败路径本来就要打 stderr 与通道错。取 20 拍，
/// 所以下面那个数是从 `ROUND` 乘出来的，不是拍的个数写成魔法数。
const PATIENCE: Duration = Duration::from_secs(ROUND.as_secs() * 20);
/// 「必须什么都不打」的窗口：必须是 `ROUND` 的倍数且大于一拍，否则一整个周期都没跑完就
/// 下结论。这个方向上**缩短它就是削弱判据**，所以它不参与上面的耐心调整（仍是 2.5 拍）。
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
    child
        .stdin
        .as_mut()
        .expect("stdin piped")
        .write_all(input.as_bytes())
        .expect("writing standard input");
    drop(child.stdin.take());
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

/// 把一个仍在运行的 child 的 stdout 接到一条通道上：读线程一行一行送，
/// 于是「没有新行」是超时报错，而不是把整个测试挂死。
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

/// 启动一个跟随中的 `tail`，stdout 走行通道、stderr 落进文件（跟随环路不退出，
/// `output()` 那只会一直等）。
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

/// 下一行，超过耐心上限即失败（附上的读数含 stderr，便于定位）。这里等的是「会不会来」，
/// 不是「多久来」，所以用 [`PATIENCE`] 而不是拍数的倍数。
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

/// 在 `wait` 这段时间里必须一行也不来（C 的静默支：睡过但什么都不打）。
fn expect_quiet(lines: &Receiver<String>, wait: Duration) {
    match lines.recv_timeout(wait) {
        Err(_) => {}
        Ok(line) => panic!("expected silence, got {line:?}"),
    }
}

/// 等够一拍以上的跨度，用来确认进程还活着（而不是默默退了）。
fn outlives_ticks(child: &mut Child, ticks: Duration) {
    let deadline = Instant::now() + ticks;
    while Instant::now() < deadline {
        if matches!(child.try_wait(), Ok(Some(_))) {
            panic!("the follow loop exited; C keeps sleeping here");
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// 追加一行（刷新到档），供跟随环路下一拍读到。
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

#[test]
fn unknown_letters_and_bad_counts_are_refused_not_leaked() {
    // `-x` 是 C 的 `case '?'` → `usage()`（`tail.c:128-130`），但前面还有一行是
    // `getopt` 自己印的：`"<prog>: unknown option -- <字母>"`
    // （`minix3/lib/libc/stdlib/getopt.c:101-104`，字母不带引号）。本仓的
    // 宿主参照件比不了这一行——它跑的是 glibc 的 `getopt`，那句写的是
    // `invalid option -- 'x'`；这一句的真值只能来自 C 源本身。
    // 而 usage 行本身是大写 `Usage:`、带程序名、选项里本来就有 `-f | -F`
    // （`tail.c:299-305`），此前本壳那行小写且只列 `-r` 的写法是自创的。
    const USAGE: &str = "Usage: tail [-f | -F | -r] [-b # | -c # | -n #] [file ...]\n";
    let (out, status, err) = run(&["-x"], "a\n");
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(err, format!("tail: unknown option -- x\n{USAGE}"));
    // 带值选项缺值：同样两行，那句是 `option requires an argument -- n`
    // （`getopt.c:132-136`）。
    let (out, status, err) = run(&["-n"], "a\n");
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(
        err,
        format!("tail: option requires an argument -- n\n{USAGE}")
    );
    // 只有 usage 一行的两支：数字选项重复（`ARG` 首句）与 `-r` 配跟随。
    for args in [&["-n", "3", "-n", "5"][..], &["-r", "-f", "a"][..]] {
        let (out, status, err) = run(args, "a\n");
        assert_eq!(out, "", "args {args:?} printed {out:?}");
        assert_eq!(status, 1, "args {args:?} stderr was {err:?}");
        assert_eq!(err, USAGE, "args {args:?}");
    }
    // 计数非法走的是 `xerrx`，那一句不缀 `errno` 句子，也没有 usage 行。
    let (out, status, err) = run(&["-n", "x"], "a\n");
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(err, "tail: illegal offset -- x\n");
}

#[test]
fn follow_with_two_operands_is_refused_before_anything_is_read() {
    // `tail.c:135-137` 的单文件门排在打开任何操作数之前，而且是 `xerrx`（
    // `misc.c:78-89` 走 `vwarnx`），所以那一句后面没有 `errno` 句子。stdout
    // 必须是空的：连第一个文件也没开。
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
    // `-F` 走同一句（`fflag` 是 1 还是 2 都过这道门）。
    assert_eq!(
        run(&["-F", one.to_str().unwrap(), two.to_str().unwrap(),], "").2,
        "tail: -f and -F options only appropriate for a single file\n"
    );
    // 门排在 `-r` 的用法检查之前（`:135` 对 `:143`）。
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
    // `tail.c:143-145`：`if (rflag) { if (fflag) usage(); }`。两个方例都是退 1、
    // 什么都不打。
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
    // `tail.c:197-201` 把管道上的 `fflag` 清零（注释写着 POSIX.2 requires this）。
    // 不接这一支的话，本例会永久等下去——这也是本仓把“跑不起来”翻译成
    // “挂住”而非“报错”的那类坑。
    for flag in ["-f", "-F"] {
        let (out, status, err) = run(&[flag], "p1\np2\n");
        assert_eq!(out, "p1\np2\n", "{flag} 在管道上该打完就退");
        assert_eq!(status, 0, "{flag} 的 stderr 是 {err:?}");
    }
}

#[test]
fn follow_prints_what_arrives_after_the_first_round() {
    // `-f` 那一拍：C 先抄到 EOF、再睡（`forward.c:191-202`、`:253`），所以初始内容
    // 先出、新行在下一拍出。
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
    // `forward.c:255` 的 `fflag == 2` 只让 `-F` 去查身份。这一条是拿它跟上一条
    // （`-f` 仍读旧 inode）一起区分两形：同一个剧本里只给 `-f`，那么同名出现的
    // 替身档必须一声不出——不然就等于把 `-f` 做成了 `-F`。
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
    // `forward.c:257-266`：`-F` 比到身份不同就 `freopen`，而 freopen 把读位置带回
    // 开头，所以替身档是**整份**打出来（不是接着旧位置只打尾巴）。注意这与 GNU
    // `tail -F` 也不完全同：GNU 会试着重放旧尾。
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
    // 重开后身份换了（`*sbp = statbuf`），同一身份上的追加仍然看得见。
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
    // 这一条把实现选择钉住：跟随拿的是**已打开的句柄**，不是每拍重开路径。
    // `-f` 不比身份（`forward.c:255` 的 `fflag == 2`），所以改名后 C 仍从同一个
    // inode 读：往新路径追加看不见，往旧 inode（改名后的名字）追加看得见。
    let file = scratch("follow-inode");
    let moved = scratch("follow-inode-moved");
    let err_file = scratch("follow-inode-err");
    fs::write(&file, "x1\nx2\n").unwrap();
    let (mut child, lines) = spawn_follow(&["-f", file.to_str().unwrap()], &err_file);
    assert_eq!(next_line(&lines, &mut child, &err_file), "x1\n");
    assert_eq!(next_line(&lines, &mut child, &err_file), "x2\n");
    fs::rename(&file, &moved).unwrap();
    // 名字现在不存在了：若实现是“每拍按路径重读”，这里会报错或静默丢掉；
    // C 和本壳都照旧从手上的句柄读。
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
    // 重定位到末尾只存在于被 `#if !defined(__minix)` 排除的 kqueue 支
    // （`forward.c:238-243`），所以 Minix 上的真源不回退位置：截空后重新写满，
    // 长回旧位置之前的部分一声不出，越过去那一刻打的是**半行**。
    let file = scratch("follow-shrink");
    let err_file = scratch("follow-shrink-err");
    fs::write(&file, "abcdef\nghijkl\n").unwrap();
    let (mut child, lines) = spawn_follow(&["-f", file.to_str().unwrap()], &err_file);
    assert_eq!(next_line(&lines, &mut child, &err_file), "abcdef\n");
    assert_eq!(next_line(&lines, &mut child, &err_file), "ghijkl\n");
    // 位置现在在 14。截空后写 21 字节：前 14 字节已“被读过”，只有 14 之后可见。
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
    // C 的 `stat(fname, &statbuf) != -1`（`forward.c:255-256`）把“名字认不出来”整支
    // 挡住：不报错、不退出、也不打东西，下一拍继续睡。
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
    // `ierr()` 是 `xerr(0, "%s", fname)` → `vwarn`（`misc.c:53-57`、`:65-76`），
    // 那句里三个成分缺一不可：程序名、操作数、`strerror(errno)` 的句子。
    // 本壳之前只打前两个（把 C 的 `warn(3)` 当成 `warnx(3)` 用）。
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
