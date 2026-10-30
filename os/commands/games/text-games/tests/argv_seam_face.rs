//! The argv seam face for the whole crate: every shell gets its arguments through
//! `bin_support::args_bytes()`，宿主腿与真机腿在那里交回同一串原始字节。
//!
//! 真值出处是批次四百二十二的一枚现量：宿主腿曾用 `std::env::args()`（对非 UTF-8
//! 参数 unwrap 即 panic 退 134），参照件 `tmp/random-421/random` 打的却是
//! `unknown option -- \xff`（C 把 argv 当字节传，从不校验）。批次四百二十二先把两条腿
//! 统一到「保原字节、各自 lossy」，这一枚门钉的契约是「裸字节 == lossy 等价」；
//! **族终笔（批次四百七十三）把契约升级为「裸字节 == 原字节」**：壳侧换 `args_bytes()`，
//! 有损折叠不再是缝的一部分，下面那枚门的期望也随之从「两路相等」改为逐字节等于
//! C 的答案（参照件 `tmp/random-421/random` 的读数）。
//!
//! 各壳报文本身的真值由各自的面门承载（`random_face` 的字节形门、`wtf_face` 的
//! usage 形、`fortune_face` 的 `Cannot open` 三段形）；这一枚只钉 argv 缝上
//! **跨壳一致**的那三形，外加「没有一行 panic」——宿主腿曾经的那枚
//! `env::args()` unwrap panic 退 134，正是这扇门立起来的原因。

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

/// 裸字节参数的表现必须**逐字节等于 C 的答案**（参照件 `tmp/random-421/random`
/// 的读数；`\xff` 经 lossy 折成 `ef bf bd` 的那条腿已随族终笔拆除），且三支壳
/// 哪一支都不得以信号退出（`code()` 回 None 就是崩了，比如宿主腿曾经的那枚
/// `env::args()` unwrap panic——它退 134，正是本枚门立起来的原因）。
#[test]
fn a_raw_nonutf8_argument_reaches_the_report_as_itself() {
    let raw = OsStr::from_bytes(b"-\xff");
    // random：`-\xff` 走选项环，libc 那一行的字母位是**那枚字节**（参照件读数
    // `unknown option -- \xff`），usage 行写死 `random`。
    let (code, out, err) = run("random", &[raw], b"aa\nbb\n");
    assert_ne!(code, 134, "random 以信号退出：{err:?}");
    assert_eq!(
        err,
        b"random: unknown option -- \xff\nusage: random [-er] [denominator]\n".to_vec()
    );
    assert_eq!(out, Vec::<u8>::new());
    // wtf：`-\xff` 在旗标位（`-` 开头且长于一）只落 usage，而 wtf 的 usage 走
    // **标准输出**、退 1——裸字节与折过的串在这里同判（判据是「长于一」），
    // 报文里不含任何字节位。
    let (code, out, err) = run("wtf", &[raw], b"");
    assert_eq!(code, 1);
    assert_eq!(out, b"usage: wtf [-o] [-f dbfile] [is] term ...\n".to_vec());
    assert_eq!(err, Vec::<u8>::new());
    // 反面：查询词位置上的裸字节必须**原样**进报文——`wtf -- -\xff` 把那枚字节当
    // 查询词（`--` 之后旗标环停了），那句「I don't know what …」里的名字就是那枚字节，
    // 折过的 `ef bf bd` 不再出现。查询要走到，先得有一档可开：缺省词典不在镜像里
    // （look 那支同款残差），用 `-f` 递一份临时库。
    let mut db = std::env::temp_dir();
    db.push(format!("argv-seam-db-{}", std::process::id()));
    std::fs::write(&db, b"Y2K: the year two thousand problem\n").expect("writing the scratch db");
    let db_os = OsStr::from_bytes(db.as_os_str().as_bytes());
    let (code, _out, err) = run(
        "wtf",
        &[OsStr::new("-f"), db_os, OsStr::new("--"), raw],
        b"",
    );
    assert_eq!(code, 1);
    assert_eq!(
        err,
        // 那枚 token 整个是查询词：`-` 也在里面，原样出现。
        b"wtf: I don't know what `-\xff' means!\n".to_vec()
    );
    let _ = std::fs::remove_file(&db);
    // fortune：`-\xff` 当档名开不了，`Cannot open` 三段形的档名位是原字节。
    let (code, out, err) = run("fortune", &[raw], b"");
    assert_eq!(code, 1);
    assert_eq!(out, Vec::<u8>::new());
    assert_eq!(
        err,
        b"fortune: Cannot open `-\xff': No such file or directory\n".to_vec()
    );
}
