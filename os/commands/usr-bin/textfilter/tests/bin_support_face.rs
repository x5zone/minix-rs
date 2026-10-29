//! 参数通道的门：argv 里出现**非法 UTF-8 字节**时，宿主这条腿必须和目标那条腿走同一步。
//!
//! 缝在 `os/commands/usr-bin/textfilter/src/bin_support.rs:args`——它是一组 `cfg` 双生面：
//! 目标侧把 `minix_rt::crt0::args()` 交来的原始字节做 lossy 渲染，宿主侧原先用
//! `std::env::args()`，而后者遇到非法字节当场 panic（`SIGABRT`）。C 把 argv 当原始字节、
//! 从不校验（`minix3/lib/libc/stdlib/getopt.c:79` 取的是字节），所以「一碰非法字节就死」
//! 既不是 C 的形状，也不是目标侧的形状。
//!
//! 下面钉的每一条报文都取自现量：`tmp/bin-support-329/measure.log`（改前：六支全 `rc=-6`）
//! 与 `tmp/bin-support-329/measure_after.log`（改后），参照件那一侧读数是
//! `tmp/tr-322/tr_ref`、`tmp/seq-320/seq_ref`、`tmp/paste-321/paste_ref`。
//!
//! **与 C 仍差在哪，这几枚门不掩盖**：C 打的是原字节 `ff` 一行
//! （`tr_ref: unknown option -- \xff`），而文本缝先把命令行 lossy 成 U+FFFD，各支再按自己的
//! 取法渲染：按字节取到的是 `ef`（U+FFFD 的第一字节），按 `char` 取再渲染的是 `c3 af` 两字节。
//! 参数缝半逐支把这一位翻正——本表十九行（`rev`、`paste`、`seq`、`tr`、`fold`、`col`、
//! `uniq`、`units`、`cksum`、`cmp`、`column`、`comm`、`cut`、`head`、`join`、`jot`、`nl`、`pr`、`sort`，末十行随批次四百五十八、
//! 四百五十九、四百六十、四百六十一、四百六十二、四百六十三、四百六十四、四百六十五、四百六十六、
//! 四百六十七落地）**至此全是原字节** `ff`，与参照件同形；表外那些仍走文本缝的壳交回的仍是 `ef`。
//! 字母位之外还有一条**操作数当文件路径**的腿：路径进了 `os/libs` 里的 `&str` 口就非折
//! 一次不可（在册 `ARGV-BYTE-PATH-IN-LIBS`）。因此下面的断言既钉住
//! 「不再 abort、两条腿一致」，也钉住**当前那一串字节**：等这些腿落地，这几枚会当场变红，
//! 由改的人有意识地翻正，而不是让门静默跟着漂。

use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt as _;
use std::process::{Command, Stdio};

const TR: &str = env!("CARGO_BIN_EXE_tr");
const SEQ: &str = env!("CARGO_BIN_EXE_seq");
const PASTE: &str = env!("CARGO_BIN_EXE_paste");
const FOLD: &str = env!("CARGO_BIN_EXE_fold");
const REV: &str = env!("CARGO_BIN_EXE_rev");
const COL: &str = env!("CARGO_BIN_EXE_col");
const UNIQ: &str = env!("CARGO_BIN_EXE_uniq");
const UNITS: &str = env!("CARGO_BIN_EXE_units");
const CKSUM: &str = env!("CARGO_BIN_EXE_cksum");
const CMP: &str = env!("CARGO_BIN_EXE_cmp");
const COLUMN: &str = env!("CARGO_BIN_EXE_column");
const COMM: &str = env!("CARGO_BIN_EXE_comm");
const CUT: &str = env!("CARGO_BIN_EXE_cut");
const HEAD: &str = env!("CARGO_BIN_EXE_head");
const JOIN: &str = env!("CARGO_BIN_EXE_join");
const JOT: &str = env!("CARGO_BIN_EXE_jot");
const NL: &str = env!("CARGO_BIN_EXE_nl");
const PR: &str = env!("CARGO_BIN_EXE_pr");
const SORT: &str = env!("CARGO_BIN_EXE_sort");

/// 一个裸 `0xFF`：既不是任何 UTF-8 序列的前缀，也不可能出现在合法 argv 里。
const FF: u8 = 0xFF;

/// 宿主侧起真子进程，argv 允许是原始字节。非法字节进不了 `&str`，所以这里不借道
/// 任何按 `str` 收参数的公共夹具；标准输入照夹具的约定喂进去，写不进（命令不读 stdin
/// 时父进程拿到 `BrokenPipe`）不当成被测代码的缺陷。
fn run(exe: &str, args: &[&[u8]], input: &[u8]) -> (i32, Vec<u8>, Vec<u8>) {
    let mut child = Command::new(exe)
        .args(args.iter().map(|a| OsStr::from_bytes(a)))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("起子进程");
    {
        use std::io::Write as _;
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(input);
            // 句柄在这里关闭，子进程随即看到 EOF
        }
    }
    let output = child.wait_with_output().expect("等子进程");
    let code = output
        .status
        .code()
        .expect("退出码该在——被信号杀掉就是双生面又分叉了");
    (code, output.stdout, output.stderr)
}

/// 十九支薄壳在非法字节上都不该再走信号路径，且第一句仍是那条 getopt 报文。
/// 字母位那一列就是这条缝的读数计：走 `bin_support::args_bytes` 的壳交回**原字节** `ff`，
/// 仍走文本缝的壳交回折叠出的 U+FFFD 的第一字节 `ef`。
#[test]
fn no_command_aborts_on_a_nonutf8_option_letter() {
    let flag = [b"-".as_slice(), &[FF][..]].concat();
    let cases: [(&str, &str, i32, &[u8]); 19] = [
        // tr（批次四百四十四）、seq（四百四十三）、paste（四百四十一）、rev（四百四十）、
        // fold（四百四十六）、col（四百四十八）、uniq（四百五十二）都走 `bin_support::args_bytes`，
        // 字母位是原字节 `ff`。
        // seq 那一支还多钉一层：它的环是累加型，一枚裸字节只该报一行
        // （`the_accumulating_ring_names_the_raw_byte_once`）。
        ("tr", TR, 1, b"tr: unknown option -- \xff\n"),
        ("seq", SEQ, 1, b"seq: unknown option -- \xff\n"),
        ("paste", PASTE, 1, b"paste: unknown option -- \xff\n"),
        ("fold", FOLD, 1, b"fold: unknown option -- \xff\n"),
        ("rev", REV, 1, b"rev: unknown option -- \xff\n"),
        ("col", COL, 1, b"col: unknown option -- \xff\n"),
        // units 的 `ef` 残留随批次四百五十五的 argv 字节缝翻正：`from_utf8_lossy` 在
        // 参数缝上把一枚裸 `ff` 折成 U+FFFD，字母位因此看着像 `ef`；换 `args_bytes`
        // 之后环拿到的就是那一枚 `ff`，与 C 同形。
        ("uniq", UNIQ, 1, b"uniq: unknown option -- \xff\n"),
        ("units", UNITS, 3, b"units: unknown option -- \xff\n"),
        // cksum 随批次四百五十七落地：C 的 `usage()` 是 `exit(1)`（`cksum.c:549`），
        // 与 `units` 的 `myexit(3)` 不同，所以这一行的退码仍是 1。
        ("cksum", CKSUM, 1, b"cksum: unknown option -- \xff\n"),
        // cmp 随批次四百五十八落地：C 的 `usage()` 退 `ERR_EXIT` = **2**（`cmp.c:157`），
        // 与 cksum 的 `exit(1)` 不同，所以这一行的退码钉的是 2 而不是抄上一行。
        ("cmp", CMP, 2, b"cmp: unknown option -- \xff\n"),
        // column 随批次四百五十九落地：它的 `usage()` 退 1（`column.c:302-307` 的 `exit(1)`）。
        ("column", COLUMN, 1, b"column: unknown option -- \xff\n"),
        // comm 随批次四百六十落地：`comm.c:195-199` 的 `usage()` 也是退 1，但它那一行的
        // 程序名**写死在串里**（没有 `getprogname()`），而本行钉的是 libc 那一行的字母位——
        // 两行的来历不同，见 `comm_face` 的 `comm_reports_the_raw_option_byte_before_the_hardcoded_usage_line`。
        ("comm", COMM, 1, b"comm: unknown option -- \xff\n"),
        // cut 随批次四百六十一落地：它的 usage 是三行、退 1（`cut.c:290-296`），
        // 而且那三行里根本没有程序名占位——本行钉的只有 libc 那一行的字母位。
        ("cut", CUT, 1, b"cut: unknown option -- \xff\n"),
        // head 随批次四百六十二落地：它的 usage 只有一行且带程序名（`head.c:199-203` 把
        // `getprogname()` 当 `%s` 递进去，与 `cut`/`comm` 那两行写死命令名的不同），退 1。
        ("head", HEAD, 1, b"head: unknown option -- \xff\n"),
        // join 随批次四百六十三落地：它的 usage 是**两行**、退 1（`join.c:630-637`），
        // 而字母位的取法（`src/join.rs` 的 `char::from(b[1])`）与渲染（`line_bytes`）
        // 早在批次四百零七就按字节了——本行翻正的是 argv 那一头。
        ("join", JOIN, 1, b"join: unknown option -- \xff\n"),
        // jot 随批次四百六十四落地：它的 `usage()` 退 1（`jot.c:314-319`，句尾是 `exit(1)`），
        // 而这一支的取法（`src/jot.rs` 的 `char::from(bytes[pos])`）与渲染（`line_bytes`）
        // 早已按字节——本行翻正的是 argv 那一头，也就是把 `support::args()` 换成 `args_bytes()`。
        ("jot", JOT, 1, b"jot: unknown option -- \xff\n"),
        // nl 随批次四百六十五落地：它的 `usage()` 是**两行**（第一行行尾 `\n\t`）、退 1
        //（`nl.c:400-405`）；这一支的选项环整个住在薄壳里，所以字母位与七句回显都由
        // `nl_face` 钉，本行只钉「不再 abort + 字母位是原字节」这一层。
        ("nl", NL, 1, b"nl: unknown option -- \xff\n"),
        // pr 随批次四百六十六落地，而它这一行**与本族其它行不同词**：`pr` 不吃 libc 的
        // `getopt`，链的是自带的 `minix3/usr.bin/pr/egetopt.c`——`:148` 打的是
        // `illegal option -- %c`（libc 在 `getopt.c:103` 打 `unknown option`）。
        // 同一枚裸字节在两个解析器下的句子就是不一样，拿「同族规律」抹平会把措辞写错；
        // 本行钉的正是这一支自己的那一行（字母位仍是一个字节 `ff`），usage 那三行与
        // 十枚空格由 `pr_face` 的 `the_four_invalid_value_echoes_and_the_operand_message_carry_the_argv_bytes`
        // 与 `letters_outside_the_c_optstring_print_the_egetopt_line_then_the_usage` 承载。
        ("pr", PR, 1, b"pr: illegal option -- \xff\n"),
        // sort 随批次四百六十七落地，措辞又回到 libc 那一支：`sort.c:156` 吃的就是
        // `getopt(argc, argv, SORT_OPTS)`（`getopt.c:101-104` 的 `unknown option`），
        // 与上一行 `pr` 自带 `egetopt` 的 `illegal option` 互为对照——同一张表里两种措辞
        // 并存，正是「拿同族规律抹平会把句子写错」的实物证据。它的 usage 是**一行**、
        // 行内有一段十三空格的间距（`sort.c:399-412`），退 **2** 而不是 1。
        ("sort", SORT, 2, b"sort: unknown option -- \xff\n"),
    ];
    for (name, exe, want_code, want_first) in cases {
        let (code, _out, err) = run(exe, &[&flag], b"aabbcc\n");
        assert_eq!(code, want_code, "{name} 在非法字节上的退码");
        assert!(
            err.starts_with(want_first),
            "{name} 的 libc 那句该是 {want_first:?}，实际 {err:?}"
        );
    }
}

/// 非法字节落在**操作数**里时只是普通数据：参照件 `tr a\xffb c` 退 0 打 `cccccc`，
/// 本层同形。操作数被当数据收走时，文本缝与字节缝给出的都是同一串字节，所以这一条
/// 钉的是「不炸」而不是「字母位」——后者归上面那张表。
#[test]
fn a_raw_byte_inside_an_operand_is_ordinary_data() {
    let set1 = [b"a".as_slice(), &[FF][..], b"b"].concat();
    let (code, out, err) = run(TR, &[&set1, b"c"], b"aabbcc\n");
    assert_eq!(code, 0, "实际 stderr {err:?}");
    assert_eq!(out, b"cccccc\n");
    assert!(err.is_empty(), "实际 {err:?}");
}

/// `-d` 把非法字节**当值收走**时根本不是错：参照件 `paste -d\xff` 退 0 且无输出
/// （零操作数那条旧判据：既不读标准输入也不报错）。
#[test]
fn a_raw_byte_consumed_as_an_option_value_is_not_an_error() {
    let dflag = [b"-d".as_slice(), &[FF][..]].concat();
    let (code, out, err) = run(PASTE, &[&dflag], b"aabbcc\n");
    assert_eq!(code, 0, "实际 stderr {err:?}");
    assert!(out.is_empty(), "实际 {out:?}");
    assert!(err.is_empty(), "实际 {err:?}");
}

/// 累加型选项环（`seq.c:101-113` 的 `errflg` 累加、不在第一个坏字母处停）在**文本缝**上
/// 会把 lossy 出来的三个字节当三个字母连报三行；`seq` 走 `bin_support::args_bytes` 之后
/// （批次四百四十三），命令行上那一枚裸 `ff` 就是一个字母，于是报文与参照件同形：
/// **一行、字母位是原字节**，紧跟一行 usage。这一枚钉的是改完之后该长的样子，
/// 防止有人把缝改回 `String` 时让「三行」悄悄回来。
#[test]
fn the_accumulating_ring_names_the_raw_byte_once() {
    let flag = [b"-".as_slice(), &[FF][..]].concat();
    let usage = b"usage: seq [-w] [-f format] [-s string] [-t string] [first [incr]] last\n";
    let (code, _out, err) = run(SEQ, &[&flag], b"aabbcc\n");
    assert_eq!(code, 1);
    assert_eq!(
        err,
        [b"seq: unknown option -- \xff\n".as_slice(), usage].concat()
    );
}
