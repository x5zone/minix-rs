//! End-to-end face for the `colrm` binary, checked against
//! `minix3/usr.bin/colrm/colrm.c`（选项环 `:67-74`、操作数段 `:76-95`、
//! 列走与读写 `:97-121`、`check()` `:123-130`、`usage()` `:132-137`）与它驱动的
//! libc `getopt`（`minix3/lib/libc/stdlib/getopt.c:74-78`、`:81-92`、`:95-104`）。
//!
//! `colrm.c:67` 的 optstring 是**空串**，与批次三百零九的 `rev` 逐字同构，所以那一环的
//! 三形照搬；但 `colrm` 的操作数不是文件名而是**列号**，于是同一个裸 `-` 在两条命令里
//! 落到完全不同的句子。参照件 `tmp/colrm-310/colrm_ref`（`colrm.c` 真源 + Minix 的
//! `getopt.c` + 程序名垫片，宿主现链）逐形量出的形状是：
//! ① 任何 `-` 开头的 token（除 `--` 与裸 `-`）= libc 一行 + usage 一行，退 1；
//! ② `--` 结束选项环且自身不算操作数，裸 `-` 则是操作数、被当列号拒收；
//! ③ 不重排：操作数之后的 `-Z` 也是列号（`colrm: illegal column -- -Z`）；
//! ④ 两支列号都坏时报**后写**的那支（C 先算 `argv[1]` 再 FALLTHROUGH）；
//! ⑤ 列号走 `strtol`：前导空白、`+` 号、溢出饱和都收，尾随字符与 0 拒收，
//!    负数经 `u_long` 回绕成极大值后被收（`colrm -- -5` 退 0 且整行透传）；
//! ⑥ `err(1, ...)` 那行三段形缀 `strerror`，`errx` 那行两段形不缀。
//!
//! 这扇门前不存在：`colrm` 的读缝直接拿裸 `minix_sys::read`，宿主上读不到任何输入，
//! 所以它的输出以前只能靠库测（同批次三百零六的 `nl`、三百零八的 `fold`、三百零九的 `rev`）。

mod face_support;

use face_support::write_stdin;
use std::process::{Command, Stdio};

const COLRM: &str = env!("CARGO_BIN_EXE_colrm");
const USAGE: &str = "usage: colrm [start [stop]]\n";
/// 参照件用的输入（`tmp/colrm-310/in.txt`），金样逐形取自它的读数。
const INPUT: &[u8] = b"abcdefg\nhi\n";

fn run(args: &[&str], input: &[u8]) -> (String, i32, String) {
    let mut child = Command::new(COLRM)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the colrm binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// 同上但交回**原始字节**：字母位要钉的是「一个字节」，`String::from_utf8_lossy`
/// 把 `c3` 与 `c3 83` 都渲染成替换字符，文本夹具区分不了这两种错法。
fn run_bytes(args: &[&str], input: &[u8]) -> (Vec<u8>, i32, Vec<u8>) {
    let mut child = Command::new(COLRM)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the colrm binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        output.stdout,
        output.status.code().unwrap_or(-1),
        output.stderr,
    )
}

/// 集外字母：libc 那一行在前、`usage()` 在后，退 1（`colrm.c:67-72` 的 `case '?': default`）。
/// 参照件读数 `colrm: unknown option -- Z` + usage。旧写法少了 libc 那一行。
#[test]
fn an_unknown_letter_prints_the_libc_line_then_the_usage() {
    let (out, status, err) = run(&["-Z"], INPUT);
    assert_eq!(out, "", "报文只走 stderr");
    assert_eq!(status, 1, "stderr 是 {err:?}");
    assert_eq!(err, format!("colrm: unknown option -- Z\n{USAGE}"));
}

/// 非 ASCII 的集外字母打的是**一个裸字节**：参照件 `tmp/colrm-310/colrm` 现量
/// `colrm -é` → `colrm: unknown option -- ` + `c3` + 换行 + usage，退 1。
/// 取法（短横后的第一个字节，`getopt.c:79`）与渲染（字节而不是 UTF-8 的两个字节）
/// 两半缺一都还是分叉：只改取法打 `c3 83`，只改渲染打 `c3 a9`。
#[test]
fn the_option_line_of_a_nonascii_letter_carries_one_raw_byte() {
    let want: Vec<u8> = [
        b"colrm: unknown option -- ".as_slice(),
        b"\xc3\n",
        USAGE.as_bytes(),
    ]
    .concat();
    let (out, status, err) = run_bytes(&["-\u{e9}"], INPUT);
    assert_eq!(out, Vec::new(), "报文只走 stderr");
    assert_eq!(status, 1, "stderr 是 {err:?}");
    assert_eq!(err, want, "字母该是一个裸字节 c3");

    // 聚簇里非 ASCII 排第二位：报错仍落在第一个字节上，且仍只有一行 libc 报文
    // （`colrm.c:68-72` 的 `default: usage()` 拿到 '?' 就退，不会再回 getopt）。
    let (out, status, err) = run_bytes(&["-x\u{e9}"], INPUT);
    assert_eq!(out, Vec::new());
    assert_eq!(status, 1);
    assert_eq!(
        err,
        b"colrm: unknown option -- x\nusage: colrm [start [stop]]\n".to_vec()
    );

    // ASCII 那一形两个入口一字不差。
    let (_out, status, err) = run_bytes(&["-Z"], INPUT);
    assert_eq!(status, 1);
    assert_eq!(
        err,
        b"colrm: unknown option -- Z\nusage: colrm [start [stop]]\n".to_vec()
    );
}

/// 聚簇只报**第一个**字符：调用者拿到 `'?'` 就 `usage()` 退出，不回到 getopt 读第二个
/// （参照件：`-bZ` → `unknown option -- b`）。
#[test]
fn a_cluster_reports_only_its_first_letter() {
    let (out, status, err) = run(&["-bZ"], INPUT);
    assert_eq!((out.as_str(), status), ("", 1));
    assert_eq!(err, format!("colrm: unknown option -- b\n{USAGE}"));
}

/// `--x` 报的字母是第二个连字符：optstring 里没有 `-`，它也是集外字符
/// （参照件：`unknown option -- -`）。纯 `--` 走的是另一支。
#[test]
fn a_dash_followed_by_a_word_is_an_unknown_option_named_by_the_dash() {
    let (out, status, err) = run(&["--x"], INPUT);
    assert_eq!((out.as_str(), status), ("", 1));
    assert_eq!(err, format!("colrm: unknown option -- -\n{USAGE}"));
}

/// `--` 自身被吃掉、后面的都算操作数（`getopt.c:81-85` 先 `++optind`）：
/// 单给 `--` 仍是整行透传，`-- 2 4` 则真的删 2..4 列。
#[test]
fn the_end_of_options_marker_is_not_an_operand() {
    let (out, status, err) = run(&["--"], INPUT);
    assert_eq!(
        (out.as_str(), err.as_str(), status),
        ("abcdefg\nhi\n", "", 0)
    );
    let (out, status, err) = run(&["--", "2", "4"], INPUT);
    assert_eq!(
        (out.as_str(), err.as_str(), status),
        ("aefg\nh\n", "", 0),
        "`--` 后面的列号该生效"
    );
}

/// 裸 `-` 是操作数而不是选项：optstring 里没有 `-`，getopt 结束选项环时**不推进**
/// `optind`（`getopt.c:87-92`），于是它落进 `argv += optind` 那一段，被当列号拒收
/// （参照件：`colrm: illegal column -- -`，一行、没有 usage、退 1）。
/// 同一个 token 在 `rev` 那里走的是 `fopen` 失败支，句子完全不同。
#[test]
fn a_solitary_dash_is_an_operand_rejected_as_a_column() {
    let (out, status, err) = run(&["-"], INPUT);
    assert_eq!(out, "", "`-` 不是「读标准输入」的别名");
    assert_eq!(status, 1, "stderr 是 {err:?}");
    assert_eq!(err, "colrm: illegal column -- -\n");
    assert!(!err.contains("usage:"), "列号的 errx 不接 usage：{err:?}");
}

/// 不重排：非选项 token 一到，选项环就结束（`getopt.c:74-78`），所以 `colrm 2 -Z` 里的
/// `-Z` 是个**列号**（参照件：`illegal column -- -Z`，而不是 `unknown option`）。
#[test]
fn an_option_looking_token_after_an_operand_is_a_column() {
    let (out, status, err) = run(&["2", "-Z"], INPUT);
    assert_eq!(out, "");
    assert_eq!(status, 1, "stderr 是 {err:?}");
    assert_eq!(err, "colrm: illegal column -- -Z\n");
}

/// 两支都坏时报**后写**的那支：C 的 `case 2:` 先对 `argv[1]` 做 `strtol` 并检查，
/// `/* FALLTHROUGH */` 才轮到 `argv[0]`（`colrm.c:78-87`）。参照件：`abc def` → `def`。
#[test]
fn the_stop_column_is_checked_before_the_start_column() {
    let (out, status, err) = run(&["abc", "def"], INPUT);
    assert_eq!((out.as_str(), status), ("", 1));
    assert_eq!(err, "colrm: illegal column -- def\n");
    // 只有 start 坏时，报的才是 start。
    let (_, status, err) = run(&["abc", "2"], INPUT);
    assert_eq!(
        (err.as_str(), status),
        ("colrm: illegal column -- abc\n", 1)
    );
}

/// 列号的接受面是 `strtol` 而不是「一串数字」：前导空白、显式 `+`、前导零都收；
/// 零与尾巴有字符的都拒（C 测 `*p`）。参照件逐形量过。
#[test]
fn column_numbers_take_the_strtol_forms() {
    for accepted in ["3", " 3", "+3", "03"] {
        let (out, status, err) = run(&[accepted], INPUT);
        assert_eq!(
            (out.as_str(), err.as_str(), status),
            ("ab\nhi\n", "", 0),
            "`{accepted}` 该被当作列号 3"
        );
    }
    for rejected in ["0", "+0", "3x", "0x10", "1e2", "3 "] {
        let (out, status, err) = run(&[rejected], INPUT);
        assert_eq!(out, "", "报文只走 stderr");
        assert_eq!(status, 1, "`{rejected}` 该被拒收：stderr 是 {err:?}");
        assert_eq!(
            err,
            format!("colrm: illegal column -- {rejected}\n"),
            "`{rejected}` 的句子该带操作数原文"
        );
    }
}

/// 溢出与负数都**不被拒**：`strtol` 溢出饱和到 `long` 的端点，而 `colrm.c:63` 把它存进
/// `u_long`，于是 C 的 `<= 0` 只挡得住 0，负数回绕成极大值。两者都比任何真实列号大，
/// 整行透传、退 0（参照件：`colrm 99999999999999999999999` 与 `colrm -- -5` 全透传）。
#[test]
fn an_overflowing_or_negative_column_passes_everything_through() {
    for huge in ["99999999999999999999999", "100"] {
        let (out, status, err) = run(&[huge], INPUT);
        assert_eq!(
            (out.as_str(), err.as_str(), status),
            ("abcdefg\nhi\n", "", 0),
            "列号 `{huge}` 该被接受并透传"
        );
    }
    let (out, status, err) = run(&["--", "-5"], INPUT);
    assert_eq!(
        (out.as_str(), err.as_str(), status),
        ("abcdefg\nhi\n", "", 0),
        "负列号经 `u_long` 回绕成极大值，C 不拒它"
    );
}

/// 多于两个操作数只印 usage（`colrm.c:90-91` 的 `default: usage()`），没有 libc 那一行，
/// 也没有列号句子。
#[test]
fn too_many_operands_print_only_the_usage() {
    let (out, status, err) = run(&["2", "4", "6"], INPUT);
    assert_eq!(out, "");
    assert_eq!(status, 1, "stderr 是 {err:?}");
    assert_eq!(err, USAGE);
}

/// 起始列大于结束列那一支是 `err(1, ...)` 而不是 `errx`：三段形里缀 `strerror(errno)`，
/// 而此刻 `errno` 是 0，句子内容由各平台的表定（参照件在本机印 `Success`），
/// 所以本壳的门只钉形状：程序名 + 句子 + 一段非空后缀、一行、不接 usage、退 1。
#[test]
fn start_after_stop_prints_the_err_line_without_the_usage() {
    let (out, status, err) = run(&["4", "2"], INPUT);
    assert_eq!(out, "");
    assert_eq!(status, 1, "stderr 是 {err:?}");
    let head = "colrm: illegal start and stop columns: ";
    assert!(err.starts_with(head), "该以 `{head}` 开头，实际 {err:?}");
    let tail = err[head.len()..].trim_end();
    assert!(
        !tail.is_empty() && !tail.contains('\n'),
        "缀的应是一句 strerror 文本：{err:?}"
    );
    assert!(!err.contains("usage:"), "这一支不接 usage：{err:?}");
}

/// 输出腿（本闸门存在的另一半）：以前 `colrm` 用裸 `minix_sys::read`，宿主上**读不到输入**，
/// 所以列走从来没有被进程级测过；而且旧写法把输入过一遍 UTF-8 校验、非文本字节整段丢掉。
/// 金样逐形取自参照件读数（`tmp/colrm-310/golden.log`）。
#[test]
fn the_column_walk_reaches_the_output_end_to_end() {
    // 只给起始列：从该列起到行末全删（stop 留 0）。
    assert_eq!(run(&["3"], INPUT).0, "ab\nhi\n");
    // 区间：列 2..4 删掉。
    assert_eq!(run(&["2", "4"], INPUT).0, "aefg\nh\n");
    assert_eq!(run(&["1", "1"], INPUT).0, "bcdefg\ni\n");
    // 制表符把列进到 8 的倍数：删 2..8 时第一个制表符落点 8 被删，
    // 第二个落点 16 在区间外而留下。
    let (out, status, _) = run(&["2", "8"], b"a\tb\tc\n");
    assert_eq!((out.as_str(), status), ("ab\tc\n", 0));
    let (out, status, _) = run(&["2", "9"], b"a\tb\tc\n");
    assert_eq!((out.as_str(), status), ("a\tc\n", 0));
    // 退格把列拉回，被拉回后落点仍在区间外的字节留下。
    let (out, status, _) = run(&["2"], b"ab\x08\x08X\n");
    assert_eq!((out.as_str(), status), ("a\x08\x08X\n", 0));
    // 末行没有换行也照删，且不补换行（C 一路 putchar，从不自己加）。
    let (out, status, _) = run(&["2", "3"], b"abcdef");
    assert_eq!((out.as_str(), status), ("adef", 0));
    // 空输入产出空。
    let (out, status, err) = run(&["2"], b"");
    assert_eq!((out.as_str(), err.as_str(), status), ("", "", 0));
    // 非 UTF-8 字节不被整段丢弃：C 按字节走列，本壳也必须按字节（旧写法先做 UTF-8
    // 校验、校验不过就把整个输入当空）。这里 `2 3` 删掉的正是那两个不可打印字节。
    let (out, status, _) = run(&["2", "3"], b"a\xff\xbccdef\n");
    assert_eq!((out.as_str(), status), ("acdef\n", 0));
}

/// 读写两根腿的承载件：把标准输出接到 `/dev/full`（字符设备，写它必拿 `ENOSPC`）。
/// 参照件 `tmp/colrm-310/colrm_ref` 在这一形上要越过 stdio 缓冲才报——实测阈值落在
/// (4096, 4505] 字节之间：4 KiB 输出退 0 且一字不报，4505 字节起报
/// `colrm_ref: stdout: No space left on device` 退 1。本壳没有 stdio 缓冲，
/// 尺寸无论大小都当场报（下面两枚门各钉一边）。
fn run_into_sink(args: &[&str], input: &[u8]) -> (i32, String) {
    let sink = std::fs::OpenOptions::new()
        .write(true)
        .open("/dev/full")
        .expect("/dev/full carries a failing write");
    let mut child = Command::new(COLRM)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::from(sink))
        .stderr(Stdio::piped())
        .spawn()
        .expect("the colrm binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// 读腿：把一个**目录**的 fd 交给标准输入，打开成功而读的时候拿 `EISDIR`。
/// C 的 `check(stdin)`（`colrm.c:123-130`）先看 `feof` 再看 `ferror`，于是走
/// `err(1, "stdin")`——参照件同形打 `colrm_ref: stdin: Is a directory`、退 1、无输出。
/// 改前本壳把读失败当文件末尾，一句话不说还退 0。句子取自 Minix 的 errno 表。
#[test]
fn a_stdin_that_cannot_be_read_names_the_stream_and_exits_one() {
    let dir = std::env::temp_dir().join(format!("colrm-stdin-dir-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let handle = std::fs::File::open(&dir).expect("a directory opens read-only here");
    let output = Command::new(COLRM)
        .arg("2")
        .stdin(Stdio::from(handle))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the colrm binary should be runnable")
        .wait_with_output()
        .expect("the run should finish");
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "",
        "读不到字节就不该有任何输出"
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "colrm: stdin: Is a directory\n",
        "报文该是程序名、流名 stdin、句子三段的 err 形"
    );
    assert_eq!(output.status.code(), Some(1), "读错要把退码置起来");
}

/// 写腿（越过参照件阈值的那一形，两边同形）：8 KiB 透传写到 `/dev/full`。
#[test]
fn a_stdout_that_cannot_be_written_names_the_stream_and_exits_one() {
    let payload = vec![b'y'; 8192];
    let (status, err) = run_into_sink(&["--", "-5"], &payload);
    assert_eq!(
        (status, err.as_str()),
        (1, "colrm: stdout: No space left on device\n"),
        "写错报的是流名 stdout，与读错分得开"
    );
}

/// 同一形的小输出：参照件被 glibc 的缓冲吞掉（退 0 不报），本壳无缓冲因此**报**。
/// 这一枚钉的是刻意的分叉，不是回归——把缓冲尺寸抄进命令层等于把一个 libc 的机制
/// 写进 OS 语义，而真机上 Minix stdio 的缓冲又是另一个数，抄了也是错。
#[test]
fn a_small_write_failure_is_reported_instead_of_swallowed_by_a_buffer() {
    let (status, err) = run_into_sink(&["--", "-5"], b"abcdefg\nhi\n");
    assert_eq!(
        (status, err.as_str()),
        (1, "colrm: stdout: No space left on device\n"),
        "本壳写不进去就报，不论尺寸"
    );
}

/// 两根腿同时坏：参照件实测报的是 **stdin** 而不是 stdout——C 一个字节都没读进来，
/// 环里从未调用 `putchar`，写错压根没机会被观察。本壳同样先拿到零字节输入、写出零字节
/// （空写不失败），于是报读腿。这一枚钉的是「两腿同坏时报哪条」这个**观察结果**。
///
/// 本注释以前还写着「先后规则本身在这形上不可证……宿主上造不出『有字节在飞、同时读也
/// 出错』的承载」，那半句已被批次三百七十五**否证**：pty 的 master 端在 slave 关闭后
/// 会先交出已缓冲的字节、下一口才回 `EIO`，正是一根宿主可造的真读源。四形矩阵读数与
/// 判据说见 `tools/atf-c-compat/probes/colrm_leg_order.py`。先后规则本身仍未钉住，但
/// 原因换了：不是造不出承载，而是要钉它就得在命令层复刻 libc 的缓冲阈值（Minix 的
/// `BUFSIZ`＝1024，见 `minix3/include/stdio.h:182` 与 `minix3/lib/libc/stdio/makebuf.c:170`），
/// 而「该不该复刻」是原则冲突，已挂 A 板 §6 待人裁决——裁决未落之前本枚门维持现状，
/// 不去动它。
#[test]
fn both_streams_broken_reports_the_stream_c_reaches_first() {
    let dir = std::env::temp_dir().join(format!("colrm-both-broken-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let handle = std::fs::File::open(&dir).expect("a directory opens read-only here");
    let sink = std::fs::OpenOptions::new()
        .write(true)
        .open("/dev/full")
        .expect("/dev/full carries a failing write");
    let output = Command::new(COLRM)
        .arg("2")
        .stdin(Stdio::from(handle))
        .stdout(Stdio::from(sink))
        .stderr(Stdio::piped())
        .spawn()
        .expect("the colrm binary should be runnable")
        .wait_with_output()
        .expect("the run should finish");
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "colrm: stdin: Is a directory\n",
        "两腿同坏时报文该跟着 C 先观察到的那条流"
    );
    assert_eq!(output.status.code(), Some(1));
}
