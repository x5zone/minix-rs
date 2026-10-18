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
//! （`tr_ref: unknown option -- \xff`），本层先 lossy 成 U+FFFD 再按各支自己的取法渲染，
//! 于是按字节取、按 `char` 渲染的那几支得到 `c3 af`（U+00EF 的 UTF-8 两字节），已换字节
//! 渲染的 `rev` 得到单字节 `ef`，累加型的 `seq` 更把替换字符的三个字节当成三个字母连报三行。
//! 还差的是**两个字节**：lossy 之前的那个原字节在 argv 缝上就没有了
//! （`bin_support::args` 两条腿都渲染成 `String`），加上 `GETOPT-REPORT-BYTE-LETTER`
//! 里其余各支壳的渲染还没换——因此下面的断言既钉住「不再 abort、两条腿一致」，
//! 也钉住**当前那一串字节**：等 argv 字节化的那笔落地，这几枚会当场变红，
//! 由改的人有意识地翻正，而不是让门静默跟着漂。

use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt as _;
use std::process::{Command, Stdio};

const TR: &str = env!("CARGO_BIN_EXE_tr");
const SEQ: &str = env!("CARGO_BIN_EXE_seq");
const PASTE: &str = env!("CARGO_BIN_EXE_paste");
const FOLD: &str = env!("CARGO_BIN_EXE_fold");
const REV: &str = env!("CARGO_BIN_EXE_rev");
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

/// 六支薄壳在非法字节上都不该再走信号路径，且第一句仍是那条 libc 报文。
#[test]
fn no_command_aborts_on_a_nonutf8_option_letter() {
    let flag = [b"-".as_slice(), &[FF][..]].concat();
    let cases: [(&str, &str, i32, &[u8]); 6] = [
        ("tr", TR, 1, b"tr: unknown option -- \xc3\xaf\n"),
        ("seq", SEQ, 1, b"seq: unknown option -- \xc3\xaf\n"),
        ("paste", PASTE, 1, b"paste: unknown option -- \xc3\xaf\n"),
        ("fold", FOLD, 1, b"fold: unknown option -- \xc3\xaf\n"),
        ("rev", REV, 1, b"rev: unknown option -- \xef\n"),
        ("units", UNITS, 3, b"units: unknown option -- \xc3\xaf\n"),
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
/// 本层同形。这一条形下 `args()` 是否 lossy 都过不了 panic 那一关，所以它是这条缝的正面证据。
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

/// 累加型选项环把 lossy 后的**三个字节当三个字母**连报三行（`seq.c` 那族是环内累加、
/// 出环后统一 usage），而参照件 `seq -\xff5` 只有一行 `\xff` 加一行 `5`。
/// 钉住「三行」是把本层 lossy 这一步的可见后果留在门上，不是把它藏起来。
#[test]
fn the_lossy_render_expands_one_bad_byte_into_three_option_letters() {
    let flag = [b"-".as_slice(), &[FF][..]].concat();
    let (code, _out, err) = run(SEQ, &[&flag], b"aabbcc\n");
    assert_eq!(code, 1);
    assert_eq!(
        err.windows(b"seq: unknown option -- ".len())
            .filter(|w| *w == &b"seq: unknown option -- "[..])
            .count(),
        3,
        "实际 {err:?}"
    );
}
