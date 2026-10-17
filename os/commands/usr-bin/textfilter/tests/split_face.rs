//! End-to-end face for the `split` binary, checked against
//! `minix3/usr.bin/split/split.c`（选项环 `:79-130`、操作数段 `:133-150`、
//! `usage()` `:355-361`）与它驱动的 libc `getopt`
//!（`minix3/lib/libc/stdlib/getopt.c:74-78`、`:81-92`、`:95-104`、`:132-136`）。
//!
//! `split.c:80` 的 optstring 是 `"0123456789b:l:a:n:"`：`0-9` 六个**合法字母**（遗留的
//! `-<数字>` 形）加四个带值项，所以它的出口比只吃空 optstring 的 `rev`/`colrm` 多一整层。
//! 参照件 `tmp/split-311/split_ref`（`split.c` 真源 + Minix 的 `getopt.c` + 程序名垫片，
//! 宿主现链）逐形量出的形状是：
//! ① 集外字母 = libc 一行 + usage 一行，退 1（`-Z`、`-Zb` 报 `Z`、`--x` 报 `-`）；
//! ② 带值项缺值 = libc 的 `option requires an argument -- <字母>` + usage 一行，退 1；
//! ③ **不重排**：非选项 token 一到选项环就结束，所以 `split <file> -Z` 里的 `-Z` 是
//!    **输出前缀**（参照件现量：生成 `-Zaa`，退 0），而不是一个坏选项；
//! ④ 多于两个操作数只印 usage；`usage()` 那一行的程序名跟着 `getprogname()`，退码是 1；
//! ⑤ 遗留形取的是整段 token 去掉 `-`（`split.c:86-90` 的 `argv[optind] + 1`），
//!    所以 `-12x` 与 `-3b5` 都报 `split: <原文>: illegal line count.`；
//! ⑥ 打不开的输入是 `err(1, "%s")` 三段形、一行、不接 usage。
//!
//! 这扇门以前不存在：`split` 的诊断面少 libc 那两行、usage 文本是本壳自拟的短式而退码写成 2、
//! 操作数之后的选项还会被重排回选项读。

mod face_support;

use face_support::write_stdin;
use std::fs;
use std::process::{Command, Stdio};

const SPLIT: &str = env!("CARGO_BIN_EXE_split");
/// C `split.c:357-359` 的那一行（程序名跟着 `getprogname()`，本壳与参照件都叫 split）。
const USAGE: &str = "usage: split [-b byte_count] [-l line_count] [-n chunk_count] [-a suffix_length] [file [prefix]]\n";
/// 参照件用的输入：12 行、每行 8 字节。
const INPUT: &[u8] = b"line001\nline002\nline003\nline004\nline005\nline006\nline007\nline008\nline009\nline010\nline011\nline012\n";

/// 在自己的临时目录里跑一次（`split` 往当前目录写分片文件），返回退码、stderr、
/// 以及目录里生成的文件名（排序后）。
fn run(args: &[&str], input: &[u8]) -> (i32, String, Vec<String>, String) {
    let dir = scratch();
    let mut child = Command::new(SPLIT)
        .args(args)
        .current_dir(&dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the split binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    let mut files: Vec<String> = fs::read_dir(&dir)
        .expect("the scratch dir should be readable")
        .filter_map(|entry| {
            entry
                .ok()
                .map(|e| e.file_name().to_string_lossy().into_owned())
        })
        .collect();
    files.sort();
    let first = files
        .first()
        .map(|name| {
            String::from_utf8_lossy(
                &fs::read(dir.join(name)).expect("the chunk file should be readable"),
            )
            .into_owned()
        })
        .unwrap_or_default();
    let err = String::from_utf8_lossy(&output.stderr).into_owned();
    let status = output.status.code().unwrap_or(-1);
    fs::remove_dir_all(&dir).expect("clean the scratch dir");
    (status, err, files, first)
}

/// 每次调用换一个目录：cargo 用多个线程并行跑测试，同一个进程内按 pid 命名的临时目录
/// 会互相看见对方写的分片文件（本门的断言正是「目录里有哪些文件」）。
static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

fn scratch() -> std::path::PathBuf {
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("split-face-{}-{}", std::process::id(), seq));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

/// 集外字母：libc 那一行在前、`usage()` 在后，退 1（`split.c:128-129` 的 `default:`）。
/// 旧写法少了 libc 那一行。
#[test]
fn an_unknown_letter_prints_the_libc_line_then_the_usage() {
    let (status, err, files, _) = run(&["-Z"], INPUT);
    assert_eq!(status, 1, "stderr 是 {err:?}");
    assert!(files.is_empty(), "报错了就不该写分片文件：{files:?}");
    assert_eq!(err, format!("split: unknown option -- Z\n{USAGE}"));
}

/// 聚簇只报**第一个**字符（参照件：`-Zb` → `unknown option -- Z`）。
#[test]
fn a_cluster_reports_only_its_first_letter() {
    let (status, err, files, _) = run(&["-Zb"], INPUT);
    assert_eq!(status, 1);
    assert!(files.is_empty());
    assert_eq!(err, format!("split: unknown option -- Z\n{USAGE}"));
}

/// `--x` 报的字母是第二个连字符：optstring 里没有 `-`，它也是集外字符
/// （参照件：`unknown option -- -`）。纯 `--` 走的是另一支。
#[test]
fn a_dash_followed_by_a_word_is_an_unknown_option_named_by_the_dash() {
    let (status, err, _, _) = run(&["--x"], INPUT);
    assert_eq!(status, 1);
    assert_eq!(err, format!("split: unknown option -- -\n{USAGE}"));
}

/// 带值项缺值是 libc 的**另一句**（`getopt.c:132-136`），随后才接 usage：
/// 四个带值字母逐个量（参照件读数同一形）。旧写法四支都直接只印 usage。
#[test]
fn a_value_taking_letter_without_its_value_prints_the_libc_line() {
    for letter in ['l', 'b', 'a', 'n'] {
        let flag = format!("-{letter}");
        let (status, err, files, _) = run(&[flag.as_str()], INPUT);
        assert_eq!(status, 1, "`-{letter}` 该退 1");
        assert!(files.is_empty(), "`-{letter}` 缺值不该写文件：{files:?}");
        assert_eq!(
            err,
            format!("split: option requires an argument -- {letter}\n{USAGE}"),
            "`-{letter}` 缺值该是 libc 那一行加 usage"
        );
    }
}

/// `usage()` 那一行是 C 的四选项式样、程序名跟着 `getprogname()`、退码 **1**
/// （`split.c:355-361`）。旧写法两样都不合：句子是本壳自拟的短式 `[-l lines]`，
/// 退码写成 2。这一形用 `-l 3 -l 4`（C 的 `if (numlines != 0) usage()`）走。
#[test]
fn the_usage_line_names_all_four_value_options_and_exits_one() {
    let (status, err, files, _) = run(&["-l", "3", "-l", "4"], INPUT);
    assert_eq!(status, 1, "C 的 usage 是 exit(1)，旧写法写的是 2");
    assert!(files.is_empty());
    assert_eq!(err, USAGE, "usage 行该逐字等于 C 的那一行");
}

/// **不重排**（本门的另一半判据）：非选项 token 一到，`getopt.c:74-78` 当场结束选项环，
/// 后面的 token 整个落进操作数段——第一个是输入档、第二个是**输出前缀**。
/// 参照件现量：`split <file> -Z` 生成 `-Zaa` 并退 0，那个 `-Z` 从来没被当选项看。
#[test]
fn an_option_looking_token_after_an_operand_is_the_output_prefix() {
    let dir = scratch();
    let file = dir.join("in");
    fs::write(&file, INPUT).expect("write operand");
    let mut child = Command::new(SPLIT)
        .arg(file.to_str().unwrap())
        .arg("-Z")
        .current_dir(&dir)
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the split binary should be runnable");
    write_stdin(&mut child, b"");
    let output = child.wait_with_output().expect("the run should finish");
    let err = String::from_utf8_lossy(&output.stderr).into_owned();
    let files: Vec<String> = fs::read_dir(&dir)
        .expect("read scratch")
        .filter_map(|entry| {
            entry
                .ok()
                .map(|e| e.file_name().to_string_lossy().into_owned())
        })
        .filter(|name| name != "in")
        .collect();
    assert_eq!(output.status.code().unwrap_or(-1), 0, "stderr 是 {err:?}");
    assert_eq!(files, vec!["-Zaa".to_string()], "`-Z` 该被当输出前缀");
    let _ = fs::remove_dir_all(dir);
}

/// 多于两个操作数只印 usage（`split.c:148-149` 的 `if (*argv != NULL) usage()`），
/// 没有 libc 那一行。
#[test]
fn three_operands_print_only_the_usage() {
    let dir = scratch();
    let file = dir.join("in");
    fs::write(&file, INPUT).expect("write operand");
    let mut child = Command::new(SPLIT)
        .arg(file.to_str().unwrap())
        .arg("pp")
        .arg("qq")
        .current_dir(&dir)
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the split binary should be runnable");
    write_stdin(&mut child, b"");
    let output = child.wait_with_output().expect("the run should finish");
    let err = String::from_utf8_lossy(&output.stderr).into_owned();
    assert_eq!(output.status.code().unwrap_or(-1), 1);
    assert_eq!(err, USAGE);
    let _ = fs::remove_dir_all(dir);
}

/// 打不开的输入是 `err(1, "%s", *argv)`（`split.c:137-138`）：程序名、操作数原文、
/// `strerror` 句子三段齐备、**一行**、不接 usage、退 1。旧写法少了第三段。
#[test]
fn an_unopenable_input_prints_the_err_shape_in_three_parts() {
    let missing = std::env::temp_dir().join(format!("split311-absent-{}", std::process::id()));
    let (status, err, files, _) = run(&[missing.to_str().unwrap()], INPUT);
    assert_eq!(status, 1);
    assert!(files.is_empty(), "输入打不开就不该写分片文件：{files:?}");
    let head = format!("split: {}: ", missing.to_str().unwrap());
    assert!(
        err.starts_with(&head),
        "该以 `split: <操作数>: ` 开头，实际 {err:?}"
    );
    let tail = err[head.len()..].trim_end();
    assert!(
        !tail.is_empty() && !tail.contains('\n'),
        "缀的应是一句 strerror 文本且只有这一行：{err:?}"
    );
    assert!(!err.contains("usage:"), "输入腿的 err 不接 usage：{err:?}");
}

/// 裸 `-` 是第一操作数、意思是标准输入（`split.c:136` 为它开了特例），前缀取默认 `x`；
/// `--` 自身不算操作数（`getopt.c:81-85` 先 `++optind`）。参照件：`-- -Z` 去开名叫
/// `-Z` 的档而报 `split: -Z: No such file or directory`。
#[test]
fn the_solitary_dash_reads_stdin_and_the_marker_is_not_an_operand() {
    let (status, err, files, content) = run(&["-"], INPUT);
    assert_eq!((status, err.as_str()), (0, ""));
    assert_eq!(files, vec!["xaa".to_string()], "裸 `-` 该读标准输入");
    assert_eq!(content, String::from_utf8_lossy(INPUT));

    let (status, err, files, _) = run(&["--"], INPUT);
    assert_eq!((status, err.as_str()), (0, ""), "`--` 自身不该被当操作数");
    assert_eq!(files, vec!["xaa".to_string()]);

    let missing = std::env::temp_dir().join(format!("split311-dashZ-{}", std::process::id()));
    let (status, err, files, _) = run(&["--", missing.to_str().unwrap()], INPUT);
    assert_eq!(status, 1);
    assert!(files.is_empty());
    assert!(
        err.starts_with(&format!("split: {}: ", missing.to_str().unwrap())),
        "`--` 之后的 token 该按输入档处理：{err:?}"
    );
}

/// 遗留形 `-<数字>` 取的是整段 token 去掉那个 `-`（C 从 `argv[optind] + 1` 起算）：
/// `-3` 把 12 行切成四片，`-12x` 与 `-3b5` 都把原文报成 illegal line count，
/// `-0` 同理（`split.c:86-96`）。旧写法只认「全数字尾巴」，杂字符尾巴会被当集外字母。
#[test]
fn the_obsolete_digit_form_takes_the_whole_token_as_the_count() {
    let (status, err, files, content) = run(&["-3"], INPUT);
    assert_eq!((status, err.as_str()), (0, ""));
    assert_eq!(
        files,
        vec![
            "xaa".to_string(),
            "xab".to_string(),
            "xac".to_string(),
            "xad".to_string()
        ],
        "12 行按 3 行切该是四片"
    );
    assert_eq!(content, "line001\nline002\nline003\n");

    for (word, reported) in [("-0", "0"), ("-12x", "12x"), ("-3b5", "3b5")] {
        let (status, err, files, _) = run(&[word], INPUT);
        assert_eq!(status, 1, "`{word}` 该被拒：stderr 是 {err:?}");
        assert!(files.is_empty());
        assert_eq!(
            err,
            format!("split: {reported}: illegal line count.\n"),
            "`{word}` 该把整段原文报出去"
        );
    }
}

/// 值可以贴在字母后面也可以另起一段，两支都真生效（`-l3` 与 `-l 3` 同为三行一片；
/// `-a 3` 把后缀宽度改成三支，生成 `xaaa`）。
#[test]
fn option_values_take_the_attached_and_the_separate_form() {
    for args in [&["-l3"][..], &["-l", "3"][..]] {
        let (status, err, files, _) = run(args, INPUT);
        assert_eq!((status, err.as_str()), (0, ""), "args 是 {args:?}");
        assert_eq!(files.len(), 4, "三行一片该切四片：{files:?}");
    }
    let (status, err, files, _) = run(&["-a", "3"], INPUT);
    assert_eq!((status, err.as_str()), (0, ""));
    assert_eq!(
        files,
        vec!["xaaa".to_string()],
        "`-a 3` 该把后缀宽度改成三支"
    );
}

/// 输出腿：默认一千行一片，所以十二行只有一个 `xaa` 且内容与输入逐字节同。
#[test]
fn the_split_reaches_the_chunk_files_end_to_end() {
    let (status, err, files, content) = run(&[], INPUT);
    assert_eq!((status, err.as_str()), (0, ""), "stderr 是 {err:?}");
    assert_eq!(files, vec!["xaa".to_string()]);
    assert_eq!(
        content,
        String::from_utf8_lossy(INPUT),
        "一片装得下全部输入"
    );

    let (status, _, files, _) = run(&["-l", "5"], INPUT);
    assert_eq!(status, 0);
    assert_eq!(
        files,
        vec!["xaa".to_string(), "xab".to_string(), "xac".to_string()],
        "十二行按五行切是三片（5 + 5 + 2）"
    );

    // 第二操作数才是输出前缀：`split <file> pp` 生成 `ppaa`..（参照件同形，A/B 已逐字对过）。
    // 注意 `split -l 3 pp` 里的 `pp` 是**第一**操作数，意思是「读名叫 pp 的档」而打不开，
    // 参照件在同一形上给同一句话，所以这里必须给输入档名在前。
    let input = std::env::temp_dir().join(format!("split311-in-{}", std::process::id()));
    fs::write(&input, INPUT).expect("write the input file");
    let (status, err, files, _) = run(&["-l", "3", input.to_str().unwrap(), "pp"], b"");
    assert_eq!((status, err.as_str()), (0, ""), "stderr 是 {err:?}");
    assert_eq!(
        files,
        vec![
            "ppaa".to_string(),
            "ppab".to_string(),
            "ppac".to_string(),
            "ppad".to_string()
        ],
        "第二操作数该当输出前缀"
    );
    let _ = fs::remove_file(&input);
}

/// 行模式的**原始字节**读数。上面那扇 `run` 把首个分片转成了 lossy 字符串，
/// 非法字节会被折成 U+FFFD，正好把这几枚要钉的东西遮掉，所以这条腿直接交回
/// 每个分片的字节内容（按文件名排序）。
fn run_chunk_bytes(args: &[&str], input: &[u8]) -> (i32, Vec<(String, Vec<u8>)>) {
    let dir = scratch();
    let mut child = Command::new(SPLIT)
        .args(args)
        .current_dir(&dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the split binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    let mut chunks: Vec<(String, Vec<u8>)> = fs::read_dir(&dir)
        .expect("the scratch dir should be readable")
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let data = fs::read(entry.path()).expect("the chunk file should be readable");
            Some((name, data))
        })
        .collect();
    chunks.sort();
    fs::remove_dir_all(&dir).expect("clean the scratch dir");
    (output.status.code().unwrap_or(-1), chunks)
}

/// 行模式以前把整档过一次 UTF-8 校验，整档不可解码就折成空串：于是一个分片都不产、
/// 还退 0（静默丢数据，P0 级）。C 的行模式数的是读缓冲里的换行（`split.c:252` 的
/// `read` 与 `:262-267` 的换行扫描），从不校验编码。期望取自参照件
/// `tmp/split-311/split_ref -l 1 tmp/split-330/in.bin x` 的 `od -An -c` 读数
/// （`tmp/split-330/a/`：三片，非法字节原样留在片里）。
#[test]
fn line_mode_keeps_undecodable_bytes() {
    let (status, chunks) = run_chunk_bytes(&["-l", "1"], b"aa\xffbb\ncc\xfe\ndd\n");
    assert_eq!(status, 0);
    assert_eq!(
        chunks,
        vec![
            ("xaa".to_string(), b"aa\xffbb\n".to_vec()),
            ("xab".to_string(), b"cc\xfe\n".to_vec()),
            ("xac".to_string(), b"dd\n".to_vec()),
        ],
        "不可解码的字节必须留在它原来那一片里"
    );
}

/// 末行没有换行就不补：C 只有见到 `\n` 才把 `Ce - Cs + 1` 个字节整行搬走，缓冲末尾
/// 剩下的那段按原样写出（`split.c:276-280`）。旧写法切成不带终止符的行、写出时逐行
/// 补 `\n`，往返一次就把「末行有没有换行」丢了。参照件同形给 `xaa`=`a \n`、
/// `xab`=`b`（`tmp/split-330/r/`，`split -l 1 nonl.bin`）。
#[test]
fn line_mode_does_not_invent_a_final_newline() {
    let (status, chunks) = run_chunk_bytes(&["-l", "1"], b"a\nb");
    assert_eq!(status, 0);
    assert_eq!(
        chunks,
        vec![
            ("xaa".to_string(), b"a\n".to_vec()),
            ("xab".to_string(), b"b".to_vec()),
        ],
        "不带换行的末行就不能带出换行"
    );
}

/// 空行是自己的一行，内容就是那一个换行（`tmp/split-330/t/` 的 `zaa`/`zab`/`zac`
/// 三形对照）；这一形两侧本来就相同，留在这里是为了让「终止符随行走」这件事有
/// 一枚不带非法字节的对照，不至于把两件事钉在一枚门上。
#[test]
fn line_mode_makes_a_blank_line_its_own_chunk() {
    let (status, chunks) = run_chunk_bytes(&["-l", "1"], b"a\n\nb\n");
    assert_eq!(status, 0);
    assert_eq!(
        chunks,
        vec![
            ("xaa".to_string(), b"a\n".to_vec()),
            ("xab".to_string(), b"\n".to_vec()),
            ("xac".to_string(), b"b\n".to_vec()),
        ]
    );
}

/// 一片装多行时逐行各带各的终止符，末行的形状也跟着整片走：`split -l 2` 对
/// `a\nb\nc`（末行不带换行）给 `xaa`=`a\nb\n`、`xab`=`c`。
#[test]
fn line_mode_keeps_each_line_terminator_inside_a_multi_line_chunk() {
    let (status, chunks) = run_chunk_bytes(&["-l", "2"], b"a\nb\nc");
    assert_eq!(status, 0);
    assert_eq!(
        chunks,
        vec![
            ("xaa".to_string(), b"a\nb\n".to_vec()),
            ("xab".to_string(), b"c".to_vec()),
        ]
    );
}

/// 对照腿：同一份非法输入走 `-b 6`，改前改后逐字节不变（参照件
/// `split -b 6 tmp/split-330/in.bin x` 给 `xaa`=`aa 377 bb \n`、`xab`=`cc 376 \n d d`、
/// `xac`=`\n`，`tmp/split-330/d/`）。它证明这一刀只落在行模式，`-b` 两条腿本来就按字节读。
#[test]
fn byte_mode_is_untouched_by_the_line_rewrite() {
    let (status, chunks) = run_chunk_bytes(&["-b", "6"], b"aa\xffbb\ncc\xfe\ndd\n");
    assert_eq!(status, 0);
    assert_eq!(
        chunks,
        vec![
            ("xaa".to_string(), b"aa\xffbb\n".to_vec()),
            ("xab".to_string(), b"cc\xfe\ndd".to_vec()),
            ("xac".to_string(), b"\n".to_vec()),
        ]
    );
}
