//! End-to-end face for the `head` binary: real process, real pipes and real
//! files, checked against expectations derived from `minix3/usr.bin/head/head.c`
//! (NetBSD).
//!
//! Two limits on the differential leg against the host's GNU `head`: the
//! `-n 0`/`-c 0` cases are pinned to C alone, since `head.c:90-103` rejects a
//! non-positive count where GNU accepts zero. 末行的换行状态不再是限制：行腿把「输入里
//! 有没有那枚换行」记在行上（`minix_textfilter::window::Line`），与 C 的逐字符搬运同形，
//! 参照件 `tmp/head-415/head_ref` 的无尾换行读数已进门。

mod face_support;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const HEAD: &str = env!("CARGO_BIN_EXE_head");

fn run(args: &[&str], input: &str) -> (String, i32, String) {
    run_with(HEAD, args, input)
}

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

/// argv 形态的**字节形**夹具：一枚裸 `ff` 写不成 `&str`，所以参数经 `OsStr::from_bytes`
/// 交下去，比的是子进程实际收到的那串字节（文本形夹具里 `from_utf8_lossy` 会把裸 `ff`
/// 与 `ef bf bd` 弄成同一个串，那正是被测的缝）。
fn run_argv(args: &[&[u8]], input: &[u8]) -> (Vec<u8>, i32, Vec<u8>) {
    use std::os::unix::ffi::OsStrExt as _;
    let mut child = Command::new(HEAD)
        .args(args.iter().map(|w| std::ffi::OsStr::from_bytes(w)))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the head binary should be runnable");
    face_support::write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        output.stdout,
        output.status.code().unwrap_or(-1),
        output.stderr,
    )
}

/// 按**原字节名**落一份档，交回它的路径字节；同一份内容也以它的 `from_utf8_lossy` 折叠名
/// 落一份（`fn raw_twin` 的注释里写了为什么两份都要）。
fn write_raw(dir: &Path, name: &[u8], body: &[u8]) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt as _;
    let path = dir.join(std::ffi::OsStr::from_bytes(name));
    fs::write(&path, body).expect("按原字节名落盘");
    path.as_os_str().as_bytes().to_vec()
}

/// 命令行参数那一半也按字节了（批次四百六十二）。`head` 在本族里独有一处：**`-c`/`-n` 的
/// 值被 C 回显**——`head.c:96-97`、`:102-103` 的 `errx(1, "illegal byte count -- %s", optarg)`
/// 把 optarg 整串打进句子（`cut` 那两句 errx 不带值、`column`/`comm` 的回显位在 `err_line`
/// 的 `&str` 口上，所以本支是唯一一条「值既进解析又进报文」的腿）。文本缝折一次之后
/// `ff` 变 `ef bf bd` 三枚，报文就与参照件差两字节。参照件 `tmp/head-415/head` 现量：
/// `-c<ff>` → `head: illegal byte count -- <ff>`；`-c 5<ff>` → 同句带尾巴那枚；
/// obsolete 形 `-5<ff>` 重写之后报的是 `5<ff>`（`head.c:188-192` 拼的是 `-n` + 原串去掉短横
/// 的全部字节，非 UTF-8 那枚跟着走）。第四段钉字母位、第五段钉**横幅**：横幅是本壳自己
/// `printf` 的（`head.c:130-131`），不经 `&str`，所以档名必须逐字节回显；盘上同时放原字节名
/// 与其折叠孪生件（内容相同），把在册的开档腿（`ARGV-BYTE-PATH-IN-LIBS`）从读数里排除掉——
/// 只放原名时本壳必然 `ENOENT` 而参照件读得到，那读数量的是开档腿不是这条缝。
#[test]
fn an_illegal_count_value_is_echoed_as_the_argv_bytes_and_so_is_the_banner() {
    use std::os::unix::ffi::OsStrExt as _;
    let usage_line = "usage: head [-n lines] [file ...]\n";
    let cases: [(&[&[u8]], &[u8]); 3] = [
        (&[&b"-c\xff"[..]], b"head: illegal byte count -- \xff\n"),
        (
            &[b"-c".as_slice(), &b"5\xff"[..]],
            b"head: illegal byte count -- 5\xff\n",
        ),
        (&[&b"-n\xff"[..]], b"head: illegal line count -- \xff\n"),
    ];
    for (args, want) in cases {
        let (out, code, err) = run_argv(args, b"alpha\nbeta\n");
        assert_eq!(
            (out.as_slice(), code),
            (&b""[..], 1),
            "{args:?} 的报错不落标准输出：{err:?}"
        );
        assert_eq!(err, want, "{args:?} 的报文该带 argv 原字节");
    }

    // obsolete 形：`-5<ff>` 先被改写成 `-n5<ff>`，值里那枚裸字节仍跟着走。
    let dir = scratch("obsolete");
    let plain = dir.join("f1.txt");
    fs::write(&plain, b"one\ntwo\n").expect("写 f1");
    let (out, code, err) = run_argv(&[&b"-5\xff"[..], plain.as_os_str().as_bytes()], b"");
    assert_eq!((out.as_slice(), code), (&b""[..], 1));
    assert_eq!(err, b"head: illegal line count -- 5\xff\n".as_slice());

    // 字母位：`-<ff>` 交回那一枚裸字节，libc 行在前、usage 在后。
    let (out, code, err) = run_argv(&[&b"-\xff"[..]], b"alpha\n");
    assert_eq!(out, Vec::new(), "集外字母不打数据");
    assert_eq!(code, 1);
    assert_eq!(
        err,
        [
            b"head: unknown option -- \xff\n".as_slice(),
            usage_line.as_bytes(),
        ]
        .concat(),
    );

    // 横幅：`-v` 之下单操数也出横幅，档名按原字节回显；开档走的是折叠孪生件。
    let raw = write_raw(&dir, b"name\xff", b"one\ntwo\n");
    fs::write(dir.join("name\u{fffd}"), b"one\ntwo\n").expect("写折叠孪生件");
    let (out, code, err) = run_argv(&[b"-v".as_slice(), &raw], b"");
    // 横幅里打的是 argv 那一整串（C 打的是 `*argv`，即调用者给的**路径**而非末段名），
    // 所以期望按「==> 」+ 路径原字节 + 「 <==\n」+ 内容拼。
    let want = [b"==> ".as_slice(), raw.as_slice(), b" <==\none\ntwo\n"].concat();
    assert_eq!(
        (out.as_slice(), code, err.as_slice()),
        (want.as_slice(), 0, &b""[..]),
        "横幅里的档名该是 argv 里那串原字节"
    );
}

fn scratch(tag: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!("head-face-{}-{tag}", std::process::id()));
    fs::create_dir_all(&path).expect("scratch dir");
    path
}

fn written(tag: &str, bytes: &[u8]) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!("head-face-{}-{tag}", std::process::id()));
    fs::write(&path, bytes).expect("writing the scratch file");
    path
}

fn text(path: &Path) -> String {
    path.to_str().unwrap().to_owned()
}

/// Twelve newline-terminated lines, so the ten-line default is visible.
fn twelve() -> String {
    (1..=12).map(|number| format!("{number}\n")).collect()
}

fn first_six() -> String {
    (1..=6).map(|number| format!("{number}\n")).collect()
}

#[test]
fn default_is_the_first_ten_lines() {
    let (out, status, err) = run(&[], &twelve());
    assert_eq!(out, first_six() + "7\n8\n9\n10\n");
    assert_eq!(status, 0, "stderr was {err:?}");
    assert_eq!(err, "");
}

#[test]
fn counts_arrive_by_option_or_obsolete_form() {
    // `obsolete()` rewrites a leading `-<digit>…` into `-n<digit>…` (`head.c`).
    assert_eq!(run(&["-n", "2"], &twelve()).0, "1\n2\n");
    assert_eq!(run(&["-3"], &twelve()).0, "1\n2\n3\n");
    // Byte mode copies raw bytes, newline included: four bytes of `1\n2\n3\n…`
    // are `1`, newline, `2`, newline.
    assert_eq!(run(&["-c", "4"], &twelve()).0, "1\n2\n");
}

#[test]
fn counts_above_the_window_are_honoured_not_refused() {
    // C `head()` just counts down, so a request larger than the file prints the
    // file; the window holds only what the input offered.
    assert_eq!(run(&["-n", "40"], &twelve()).0, twelve());
    assert_eq!(run(&["-n", "1000000"], &twelve()).0, twelve());
}

#[test]
fn non_positive_counts_reject_with_c_wording() {
    // `head.c:90-103`: `strtoimax` then `*ep || count <= 0` →
    // `errx(1, "illegal line count -- %s")` (or "byte"). Note `tail` is not the
    // same: its `ARG` macro accepts a zero offset and prints nothing.
    for (args, message) in [
        (vec!["-n", "0"], "head: illegal line count -- 0"),
        (vec!["-n", "-3"], "head: illegal line count -- -3"),
        (vec!["-c", "0"], "head: illegal byte count -- 0"),
        (vec!["-n", "x"], "head: illegal line count -- x"),
    ] {
        let (out, status, err) = run(args.as_slice(), "a\n");
        assert_eq!(out, "", "args {args:?} printed {out:?}");
        assert_eq!(status, 1, "args {args:?} stderr was {err:?}");
        assert!(err.contains(message), "stderr was {err:?}");
    }
}

/// 与 `run` 同一条命令缝，但标准错误**逐字节**交回：`from_utf8_lossy` 把一个裸 `c3`
/// 渲染成三字节 U+FFFD，文本形分不清「一个字节」与「两个字节」这两种错法。
fn run_bytes(args: &[&str], input: &str) -> (Vec<u8>, i32, Vec<u8>) {
    let mut child = Command::new(HEAD)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the head binary should be runnable");
    face_support::write_stdin(&mut child, input.as_bytes());
    let output = child.wait_with_output().expect("the run should finish");
    (
        output.stdout,
        output.status.code().unwrap_or(-1),
        output.stderr,
    )
}

/// 集外字母那一行的字母位是**一个裸字节**。期望逐形取自本轮新配的参照件
/// `tmp/head-415/head_ref`（`minix3/usr.bin/head/head.c` + Minix 的 `getopt.c` + 垫片，
/// 经同名软链 `head` 起 ⇒ 程序名段与本壳一致、整条 stderr 直比）：`-é` 与 `-qé` 都是
/// `head: unknown option -- ` + 单个 `c3` + 换行再跟那一行 usage（`head.c:199-203`）、
/// 退 **1**；`-Zé` 停在 `Z`；反面的 `-cé` 里 `é` 是**值**而不是字母，走命令自己的
/// `head: illegal byte count -- é`（`head.c:92-102`）**单独一行、不接 usage**。
/// 现成的 `tmp/family-diag-ref/head_ref` 不能用：它打 `-Z` 交回 glibc 那句带引号的
/// `invalid option -- 'Z'`，usage 里的程序名还是 `?`（批次四百零七那条验归属规矩第三次兑现）。
/// ASCII 那几形（`-x`、`-5`、`-n` 缺值）本文件已有主人，不重复承载。
#[test]
fn the_libc_line_of_a_multibyte_flag_is_one_byte_for_head() {
    const USAGE: &[u8] = b"usage: head [-n lines] [file ...]\n";
    let want: Vec<u8> = [b"head: unknown option -- ".as_slice(), b"\xc3\n", USAGE].concat();
    for form in ["-\u{e9}", "-q\u{e9}"] {
        let (out, status, err) = run_bytes(&[form], "a\n");
        assert_eq!(out, Vec::new(), "{form} 的报文只走标准错误");
        assert_eq!(status, 1, "{form} 的 stderr 是 {err:?}");
        assert_eq!(err, want, "{form} 的字母该是一个裸字节 c3");
    }

    let (_out, status, err) = run_bytes(&["-Z\u{e9}"], "a\n");
    assert_eq!(status, 1);
    assert_eq!(
        err,
        [b"head: unknown option -- Z\n".as_slice(), USAGE].concat(),
        "环停在第一个坏字母上，第二个字节根本不该被读到"
    );

    let (_out, status, err) = run_bytes(&["-c\u{e9}"], "a\n");
    assert_eq!(status, 1);
    assert_eq!(
        err,
        [
            b"head: illegal byte count -- ".as_slice(),
            "\u{e9}".as_bytes(),
            b"\n".as_slice(),
        ]
        .concat(),
        "带值字母把两字节整个当值收走，句子是命令自己的那一句、不接 usage"
    );
}

#[test]
fn unknown_option_prints_the_libc_line_then_the_usage_line() {
    // C 是**两行**：`getopt` 先印 `"<prog>: unknown option -- <字母>"`
    // （`minix3/lib/libc/stdlib/getopt.c:101-104`，字母不带引号），再由
    // `head.c:115-117` 的 `case '?'` 走 usage。usage 那一行逐字对位
    // `head.c:199-203`，而且 C 的清单里本来就没有 `-c`/`-q`/`-v`。
    const USAGE: &str = "usage: head [-n lines] [file ...]\n";
    let (out, status, err) = run(&["-x"], "a\n");
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(err, format!("head: unknown option -- x\n{USAGE}"));
    // 缺值是同一形状的另一句（`getopt.c:132-136`）。
    let (out, status, err) = run(&["-n"], "a\n");
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert_eq!(
        err,
        format!("head: option requires an argument -- n\n{USAGE}")
    );
}

#[test]
fn an_operand_ends_option_scanning_like_c() {
    // C 的 `getopt` **不重排**：`minix3/lib/libc/stdlib/getopt.c:71-74` 在第一个
    // 非选项 token 上直接交回 -1 且不推进 `optind`，`head.c:118-119` 的
    // `argv += optind` 于是把那个 token 与其后的**全部**当操作数。参照件八形现量
    // （`python3 tmp/head-424/matrix.py`，读数 `tmp/head-424/matrix-ref.log`）：
    // `head a.txt -5` 打 `==> a.txt <==` 加十行内容，再 `head: -5: No such file or
    // directory` 退 1——`unknown option` 那一句根本不会出现。改前本壳报的是那句
    // 加 usage，且**一行内容都不打**。
    let good = written("operand-first", b"a1\na2\n");
    let good_text = text(good.as_path());
    let (out, status, err) = run(&[&good_text, "-5"], "");
    assert_eq!(out, format!("==> {good_text} <==\na1\na2\n"));
    assert_eq!(status, 1);
    assert_eq!(err, "head: -5: No such file or directory\n");

    // 真旗标也一样落不进选项面：`-q` 在此是一个叫 `-q` 的档名，横幅照打（操作数
    // 两个），静默旗标不生效。
    let (out, status, err) = run(&[&good_text, "-q"], "");
    assert_eq!(out, format!("==> {good_text} <==\na1\na2\n"));
    assert_eq!(status, 1);
    assert_eq!(err, "head: -q: No such file or directory\n");

    // 选项在前仍照常生效，只是环撞上操作数就再也回不来：`-n 1` 取了一行，
    // 其后的 `-5` 只是又一个档名。
    let (out, status, err) = run(&["-n", "1", &good_text, "-5"], "");
    assert_eq!(out, format!("==> {good_text} <==\na1\n"));
    assert_eq!(status, 1);
    assert_eq!(err, "head: -5: No such file or directory\n");

    // 操作数之后的 `--` 不是「选项结束」而是一个档名（C 的环没走到它）；单个 `-`
    // 也在同一处收 -1（`getopt.c:96-104`：`head` 的选项串里没有 `-`），于是它与
    // 后面的都是操作数。两形参照件读数同形。
    let (out, status, err) = run(&[&good_text, "--", "-5"], "");
    assert_eq!(
        out,
        format!("==> {good_text} <==\na1\na2\n"),
        "-- 之后的 -5 不该被当成旗标"
    );
    assert_eq!(status, 1);
    assert_eq!(
        err,
        "head: --: No such file or directory\nhead: -5: No such file or directory\n"
    );
    let (out, status, err) = run(&[&good_text, "-"], "");
    assert_eq!(out, format!("==> {good_text} <==\na1\na2\n"));
    assert_eq!(status, 1);
    assert_eq!(err, "head: -: No such file or directory\n");
    let _ = fs::remove_file(&good);
}

#[test]
fn banners_follow_the_q_and_v_rules() {
    let one = written("banner-a", b"a1\na2\n");
    let two = written("banner-b", b"b1\nb2\n");
    let (one_text, two_text) = (text(one.as_path()), text(two.as_path()));

    // More than one operand and no `-q`: a banner per file, blank line between.
    let (out, _, _) = run(&[&one_text, &two_text], "");
    assert_eq!(
        out,
        format!("==> {one_text} <==\na1\na2\n\n==> {two_text} <==\nb1\nb2\n")
    );
    // `-q` suppresses them, `-v` forces one even for a single file.
    assert_eq!(run(&["-q", &one_text, &two_text], "").0, "a1\na2\nb1\nb2\n");
    assert_eq!(
        run(&["-v", &one_text], "").0,
        format!("==> {one_text} <==\na1\na2\n")
    );

    for path in [&one, &two] {
        let _ = fs::remove_file(path);
    }
}

#[test]
fn an_unopenable_operand_warns_without_a_banner_and_sets_the_status() {
    // `fopen` failure is `warn(); eval = 1; continue;`, and the banner test runs
    // only after a successful open. A bare `-` is a literal name for `head`,
    // because every operand goes through `fopen` (`head.c:121`).
    let good = written("partial-a", b"g1\n");
    let good_text = text(&good);
    let missing = format!("head-face-{}-missing", std::process::id());
    let (out, status, err) = run(&[&missing, &good_text], "");
    assert_eq!(status, 1);
    // `warn("%s", *argv)`（`head.c:124-126`）的三个成分：程序名、操作数、
    // `strerror` 句子。旧写法只印前两个。
    assert_eq!(err, format!("head: {missing}: No such file or directory\n"));
    assert_eq!(out, format!("==> {good_text} <==\ng1\n"));

    let (out, status, err) = run(&["-"], "");
    assert_eq!(out, "");
    assert_eq!(status, 1);
    assert!(err.contains("head: -"), "stderr was {err:?}");

    let _ = fs::remove_file(&good);
}

#[test]
fn agrees_with_the_host_head_on_the_shared_forms() {
    let cases: Vec<(Vec<&str>, String)> = vec![
        (vec![], twelve()),
        (vec!["-n", "3"], twelve()),
        (vec!["-n", "40"], twelve()),
        (vec!["-c", "5"], twelve()),
    ];
    let system = if Command::new("head").arg("--version").output().is_ok() {
        "head"
    } else {
        eprintln!("no host head to compare against; skipping the differential leg");
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

/// 无尾换行的末行原样交回，本壳不给它造一枚换行。C 的行形是逐字符搬运
/// （`minix3/usr.bin/head/head.c:169-172` 的 `getc`/`putchar`），参照件
/// `tmp/head-415/head_ref` 对 `alpha\nbeta\ngamma` 现量交回的正是这串、末尾没有换行；
/// 中间行仍各带一枚换行。登记项 `HEAD-FINAL-LINE-ADDS-NEWLINE` 由此闭合。
#[test]
fn an_unterminated_final_line_reaches_stdout_without_a_newline() {
    let (out, code, err) = run(&[], "alpha\nbeta\ngamma");
    assert_eq!(
        (out.as_str(), code, err.as_str()),
        ("alpha\nbeta\ngamma", 0, "")
    );
    // 计数截到中间行时，那一行本来带着换行，所以照样有。
    let (two, _, _) = run(&["-n", "2"], "alpha\nbeta\ngamma");
    assert_eq!(two, "alpha\nbeta\n");
}
