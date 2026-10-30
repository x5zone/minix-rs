//! The argv seam face for the whole crate: every shell gets its arguments through
//! `bin_support::args()`, and the host leg and the real-machine leg must have the
//! same shape there.
//!
//! A live measurement established the ground truth: the host leg once used
//! `std::env::args()` (which panics and exits 134 on a non-UTF-8 argument),
//! while the real-machine leg used `from_utf8_lossy` — so `random -\xff` crashed
//! on the host, whereas the reference build printed `unknown option -- \xff`
//! (C passes argv as bytes and never validates it). The fix copied the same seam
//! from textfilter's `bin_support.rs`: `args_os()` keeps the original bytes and
//! each leg applies lossy conversion itself.
//!
//! This gate pins the **contract of the seam**, not each shell's message: feed
//! every shell a bare `-\xff` (an `OsStr` byte, not routed through UTF-8), then
//! feed its lossy-equivalent string; the three outputs of the two runs (exit
//! code, standard output, standard error) must be byte-identical — that is,
//! "a raw-byte argument behaves like its lossy equivalent". The truth of each
//! shell's own message is carried by its own face test (the byte-shape gate in
//! `random_face`, the usage shape in `wtf_face`, the three-part `Cannot open`
//! shape in `fortune_face`); this test does not restate literals, only that the
//! two runs agree and that **no line panics**.

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

/// Feed standard input: only `BrokenPipe` is tolerable (a child that exits
/// before reading its input is the normal outcome for several shapes; see the
/// same criterion in `random_face.rs::feed_stdin`. Any other write error still
/// fails on the spot).
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

/// A raw-byte argument must behave **byte-identically** to its lossy
/// equivalent, and none of the three shells may die by signal (`code()`
/// returning None means it crashed, as the host leg once did with the
/// `env::args()` unwrap panic — it exited 134, which is exactly why this gate
/// exists).
#[test]
fn a_raw_nonutf8_argument_behaves_like_its_lossy_equivalent() {
    let raw = OsStr::from_bytes(b"-\xff");
    let lossy = OsStr::new("-\u{fffd}");
    // The three shells consume this token in different places: random/wtf in
    // the option loop, fortune as a filename — but the seam contract
    // "raw bytes == lossy equivalent" must hold in all three positions.
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
        // The flip side: an invalid byte must not appear verbatim in the
        // message (after lossy conversion it is U+FFFD's three bytes)
        assert!(
            !raw_out.contains(&b'\xff') && !raw_err.contains(&b'\xff'),
            "{name} 的报文里出现了未经 lossy 的原字节"
        );
    }
}
