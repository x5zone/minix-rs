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
//! 参数缝半逐支把这一位翻正——本表里 `rev`、`paste`、`seq`、`tr`、`fold`、`col` 六支已走
//! `bin_support::args_bytes`，字母位交回原字节 `ff`（与参照件同形），只余 `units` 一行仍是
//! 折叠形 `ef`：那一支的参数缝与单位表键共用同一串字节，得先裁决表键换字节还是在查询边界
//! 折一次 lossy。字母位之外还有一条**操作数当文件路径**的腿：路径进了 `os/libs` 里的
//! `&str` 口就非折一次不可（在册 `ARGV-BYTE-PATH-IN-LIBS`）。因此下面的断言既钉住
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
const UNITS: &str = env!("CARGO_BIN_EXE_units");

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

/// 七支薄壳在非法字节上都不该再走信号路径，且第一句仍是那条 libc 报文。
/// 字母位那一列就是这条缝的读数计：走 `bin_support::args_bytes` 的壳交回**原字节** `ff`，
/// 仍走文本缝的壳交回折叠出的 U+FFFD 的第一字节 `ef`。
#[test]
fn no_command_aborts_on_a_nonutf8_option_letter() {
    let flag = [b"-".as_slice(), &[FF][..]].concat();
    let cases: [(&str, &str, i32, &[u8]); 7] = [
        // tr（批次四百四十四）、seq（四百四十三）、paste（四百四十一）、rev（四百四十）、
        // fold（四百四十六）、col（四百四十八）都走 `bin_support::args_bytes`，字母位是原字节 `ff`。
        // seq 那一支还多钉一层：它的环是累加型，一枚裸字节只该报一行
        // （`the_accumulating_ring_names_the_raw_byte_once`）。
        ("tr", TR, 1, b"tr: unknown option -- \xff\n"),
        ("seq", SEQ, 1, b"seq: unknown option -- \xff\n"),
        ("paste", PASTE, 1, b"paste: unknown option -- \xff\n"),
        ("fold", FOLD, 1, b"fold: unknown option -- \xff\n"),
        ("rev", REV, 1, b"rev: unknown option -- \xff\n"),
        ("col", COL, 1, b"col: unknown option -- \xff\n"),
        // units 是本表唯一仍走文本缝的一支（批次三百九十九只换了渲染半：按 `char` 渲染
        // 会是 `c3 af` 两字节，字节形后是 `ef` 一字节），C 那一侧是原字节 `ff`。
        ("units", UNITS, 3, b"units: unknown option -- \xef\n"),
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
