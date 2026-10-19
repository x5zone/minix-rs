//! The argv seam face for the whole crate: every shell gets its arguments through
//! `bin_support::args()`，宿主腿与真机腿在那里必须同形。
//!
//! 真值出处是批次四百二十二的一枚现量：宿主腿曾用 `std::env::args()`（对非 UTF-8
//! 参数 unwrap 即 panic 退 134），而真机腿是 `from_utf8_lossy`——于是
//! `random -\xff` 在宿主上直接崩，参照件 `tmp/random-421/random` 打的却是
//! `unknown option -- \xff`（C 把 argv 当字节传，从不校验）。修法照 textfilter
//! `bin_support.rs` 的同一条缝：`args_os()` 保原字节、两条腿各自 lossy。
//!
//! 这一枚门钉的是**缝的契约**而不是各壳的报文：给每支壳喂一枚裸 `-\xff`
//! （`OsStr` 字节，不经 UTF-8），再喂它的 lossy 等价串，两路的三样（退码、标准
//! 输出、标准错误）必须逐字节相同——即「裸字节参数的表现 == lossy 等价的表现」。
//! 各壳报文本身的真值由各自的面门承载（`random_face` 的字节形门、`wtf_face` 的
//! usage 形、`fortune_face` 的 `Cannot open` 三段形），这里不重复断言字面，
//! 只断言两路相等且**没有一行 panic**。

use std::ffi::OsStr;
use std::io::Write as _;
use std::os::unix::ffi::OsStrExt as _;
use std::process::{Command, Stdio};

fn bin(name: &str) -> &'static str {
    match name {
        "random" => env!("CARGO_BIN_EXE_random"),
        "wtf" => env!("CARGO_BIN_EXE_wtf"),
        "fortune" => env!("CARGO_BIN_EXE_fortune"),
        _ => unreachable!(),
    }
}

fn run(name: &str, args: &[&OsStr], input: &[u8]) -> (i32, Vec<u8>, Vec<u8>) {
    let mut child = Command::new(bin(name))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn");
    feed_stdin(&mut child, input);
    let out = child.wait_with_output().expect("wait");
    (
        out.status.code().expect("exit code"),
        out.stdout,
        out.stderr,
    )
}

/// 喂标准输入：只有 `BrokenPipe` 可以容忍（子进程在读输入之前就退是若干形的正常
/// 结果，见 `random_face.rs::feed_stdin` 同一条判据；其余写错仍然当场失败）。
fn feed_stdin(child: &mut std::process::Child, input: &[u8]) {
    let mut stdin = child.stdin.take().expect("stdin pipe");
    if let Err(err) = stdin.write_all(input) {
        assert_eq!(
            err.kind(),
            std::io::ErrorKind::BrokenPipe,
            "写标准输入只该因为「子进程已退出」而失败，实际是 {err}"
        );
    }
}

/// 裸字节参数的表现必须与它的 lossy 等价**逐字节相同**，且三支壳哪一支都不得
/// 以信号退出（`code()` 回 None 就是崩了，比如宿主腿曾经的那枚 `env::args()`
/// unwrap panic——它退 134，正是本枚门立起来的原因）。
#[test]
fn a_raw_nonutf8_argument_behaves_like_its_lossy_equivalent() {
    let raw = OsStr::from_bytes(b"-\xff");
    let lossy = OsStr::new("-\u{fffd}");
    // 三支壳吃这一枚 token 的位置不同：random/wtf 走选项环，fortune 当档名——
    // 但「裸字节 == lossy 等价」这条缝契约对三种位置都得成立。
    let rows: [(&str, &[u8]); 3] = [("random", b"aa\nbb\n"), ("wtf", b""), ("fortune", b"")];
    for (name, input) in rows {
        let (raw_code, raw_out, raw_err) = run(name, &[raw], input);
        let (lossy_code, lossy_out, lossy_err) = run(name, &[lossy], input);
        assert!(
            raw_code != 134 && lossy_code != 134,
            "{name} 以信号退出（134 = panic 的 abort），宿主腿的 argv 缝在校验参数：{raw_err:?}"
        );
        assert_eq!(
            (raw_code, raw_out.clone(), raw_err.clone()),
            (lossy_code, lossy_out, lossy_err),
            "{name} 的裸字节参数与 lossy 等价表现不同——两条 argv 腿不同形"
        );
        // 反面：非法字节不许原样出现在报文里（lossy 之后它是 U+FFFD 的三个字节）
        assert!(
            !raw_out.contains(&b'\xff') && !raw_err.contains(&b'\xff'),
            "{name} 的报文里出现了未经 lossy 的原字节"
        );
    }
}
