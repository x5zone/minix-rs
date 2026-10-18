//! End-to-end face for the `fortune` binary, checked against
//! `minix3/games/fortune/fortune/fortune.c`。
//!
//! 这一套门的前提是**宿主能把操作数那枚档开起来**：壳里的档名访问走
//! `bin_support::slurp_operand` 的双生面（宿主腿 `std::fs`，真机腿
//! `minix_sys::{open, read, close}`）。在此之前 `fortune` 在宿主连档都开不开，
//! 任何端到端门都写不出来（判据见 `rewrite-notes/18-stage-commands/todo.md`
//! 批次三百六十与三百六十一）。
//!
//! 四条形与读数：
//! ① 单行库——抽取下标对库长取模恒为 0，掷到什么数都命中同一行，原样写出去、退 0。
//! ② 开不了的档——报本壳那一句 `fortune: <操作数>: cannot open` 退 1。
//! ③ 空库——`pick_quip` 无从掷，报 `fortune: <操作数>: empty database` 退 1。
//! ④ 标准输出收下不了字节——C 的输出腿不查失手：`fputs(line, stdout)`（`:272`）之后是
//!    `(void) fflush(stdout)`（`:274`），返回值丢掉，全文件没有 `ferror(stdout)`；退码
//!    只由 `:202` 与 `:377` 那两条 `exit` 决定。⇒ 写失手**不新增一句话、也不改退码**，
//!    与 ① 同形：退 0、stderr 空。宿主用 `/dev/full` 注入（写它必回「设备没有空间」）。

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn fortune_bin() -> &'static str {
    env!("CARGO_BIN_EXE_fortune")
}

/// 在临时目录里造一枚内容给定的库，路径带 `tag` 以免几枚门并行时互相覆盖。
fn fixture(tag: &str, bytes: &[u8]) -> PathBuf {
    let path = std::env::temp_dir().join(format!("minix-fortune-face-{tag}.db"));
    std::fs::write(&path, bytes).expect("write fortune fixture");
    path
}

/// 跑一腿：标准输出交给调用者决定（管道或 `/dev/full`），三路都收回。
fn run(db: &Path, stdout: Stdio) -> (String, i32, String) {
    let child = Command::new(fortune_bin())
        .arg(db)
        .stdout(stdout)
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn fortune");
    let out = child.wait_with_output().expect("wait fortune");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        out.status.code().expect("exit code"),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn a_one_line_database_prints_that_line_and_exits_zero() {
    let db = fixture("plain", b"only-quip\n");
    let (out, code, err) = run(&db, Stdio::piped());
    assert_eq!((out.as_str(), code, err.as_str()), ("only-quip\n", 0, ""));
    let _ = std::fs::remove_file(&db);
}

#[test]
fn an_unopenable_operand_reports_the_c_err_line_and_exits_one() {
    // C 是 `err(1, "Cannot open `%s'", fp->path)`（`minix3/games/fortune/fortune/fortune.c:1056`）：
    // 三段形，中间那位是句子连着用反引号与单引号包住的档名，尾巴是号对应的句子，退码 1。
    // 期望读数取自现跑（同一条命令换档名即可复现，见批次三百六十二）
    let db = std::env::temp_dir().join("minix-fortune-face-does-not-exist.db");
    let _ = std::fs::remove_file(&db);
    let (out, code, err) = run(&db, Stdio::piped());
    assert_eq!(
        (out.as_str(), code, err.as_str()),
        (
            "",
            1,
            &format!(
                "fortune: Cannot open `{db}': No such file or directory\n",
                db = db.display()
            )[..]
        ),
        "stderr={err}"
    );
}

#[test]
fn an_empty_database_is_blamed_by_name_and_exits_one() {
    let db = fixture("empty", b"");
    let (out, code, err) = run(&db, Stdio::piped());
    assert_eq!(
        (out.as_str(), code, err.as_str()),
        (
            "",
            1,
            &format!("fortune: {}: empty database\n", db.display())[..]
        ),
        "stderr={err}"
    );
    let _ = std::fs::remove_file(&db);
}

#[test]
fn a_stdout_that_stops_taking_bytes_adds_no_word_and_no_status() {
    // 同一枚库在 `a_one_line_database_prints_that_line_and_exits_zero` 里确实写出了一行
    // 且退 0，所以这里的 0 不是「根本没掷中」骗来的
    let db = fixture("full", b"only-quip\n");
    let file = std::fs::File::create("/dev/full").expect("open /dev/full");
    let (out, code, err) = run(&db, Stdio::from(file));
    assert_eq!(err, "", "标准输出收下不了字节时不该有任何一句话：{err}");
    assert_eq!(
        (out.as_str(), code),
        ("", 0),
        "C 的输出腿不查失手（`fortune.c:272-274`），退码不该因写不进去而变"
    );
    let _ = std::fs::remove_file(&db);
}

/// 开得了、读不断的操作数（目录：`open` 成功、`read` 回「是一个目录」）在 C 里就是
/// 装载到零条格言——`fgets` 回 NULL 与 EOF 同形，且从不查 `ferror`（`fortune.c:268`、`:293`）。
/// 本壳以前给它单独一句 `read error` 并退 1，那句与那个退码都是自造的。改完之后它必须走
/// 与空库**同一条路**：同一句报文（换掉操作数）、同一个退码。这条断言不猜 C 的绝对措辞，
/// 只钉「读失手不成其一形」——两形的报文与退码必须一致。
#[test]
fn an_operand_that_opens_but_cannot_be_read_is_an_empty_database() {
    let dir = std::env::temp_dir().join("minix-fortune-face-dir-as-db.db");
    std::fs::create_dir_all(&dir).expect("make a directory operand");
    let (dir_out, dir_code, dir_err) = run(&dir, Stdio::piped());
    let empty = fixture("dirfold", b"");
    let (_empty_out, empty_code, empty_err) = run(&empty, Stdio::piped());
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_file(&empty);
    assert_eq!(dir_out, "");
    assert_eq!(dir_code, empty_code, "读不断与空库的退码必须同形");
    assert_eq!(
        dir_err.replace(dir.display().to_string().as_str(), "<操作数>"),
        empty_err.replace(empty.display().to_string().as_str(), "<操作数>"),
        "读不断与空库的报文必须同形：目录得 {dir_err:?}，空库得 {empty_err:?}"
    );
}
