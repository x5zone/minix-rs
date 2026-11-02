//! End-to-end face for the `fortune` binary, checked against
//! `minix3/games/fortune/fortune/fortune.c` and the `strfile` on-disk format
//! (`minix3/games/fortune/strfile/strfile.h`).
//!
//! The `.dat` fixtures under `fixtures/` are produced by the C tool itself:
//! `strfile`, built from `minix3/games/fortune/strfile/strfile.c`, run over
//! small texts (`golden.txt`, `rot13.txt` with `-x`, `single.txt`,
//! `empty.txt` — the last one prints "There were 0 strings"). Each face
//! copies the text and its `.dat` into a temporary directory so the operand
//! names a real two-file pair.
//!
//! The shapes and their readings:
//! ① Two-quip index — the draw is the clock's seconds modulo the count, so
//!    either quip may surface; the face pins the output to that set.
//! ② Single-quip index — deterministic: that quip, byte for byte, exit 0.
//! ③ Missing `.dat` — `is_fortfile`'s readability check
//!    (`fortune.c:792-795`) fails inside `add_file`, so the verdict is
//!    `warnx("`X' not a fortune file or directory")` (`:549-551`), exit 1.
//! ④ Missing text file — `err(1, "Cannot open `%s'", path)` (`:525`), the
//!    full three-part line with the errno sentence, exit 1.
//! ⑤ Illegal suffix (`X.dat` as the operand, `fortune.c:756-762`) — the same
//!    warnx shape as ③, decided on the name alone.
//! ⑥ Zero-quip index — C divides by zero (`fortune.c:956`); the adjudicated
//!    verdict is `fortune: no fortunes in database`, exit 1.
//! ⑦ Corrupt index (short header, bad version, truncated table, out-of-range
//!    point) — the adjudicated verdict `fortune: invalid fortune database`,
//!    exit 1 (C's own check covers only the short read, `:1115-1117`).
//! ⑧ `STR_ROTATED` index — the quip is printed rot-13'd (`fortune.c:270-271`).
//! ⑨ Standard output stops taking bytes — C's output leg does not check for
//!    failure (`fputs` + `(void) fflush`, `:272-274`), so the shape matches ②
//!    with an empty stderr and exit 0. The host injects it with `/dev/full`.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn fortune_bin() -> &'static str {
    env!("CARGO_BIN_EXE_fortune")
}

/// Copy a committed fixture pair (`<name>.txt` + `<name>.txt.dat`) into a
/// temporary directory under `tag`, so the operand names a real two-file pair
/// and parallel gates do not overwrite one another. Returns the text path.
fn fixture(tag: &str, name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("minix-fortune-face-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("make the face directory");
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let text = dir.join(name);
    std::fs::copy(source.join(format!("{name}.txt")), &text).expect("copy the text fixture");
    // The index beside the operand: `<operand>.dat`, not `<name>.txt.dat` —
    // `is_fortfile` builds the index name by appending `.dat` to the operand.
    std::fs::copy(
        source.join(format!("{name}.txt.dat")),
        dir.join(format!("{name}.dat")),
    )
    .expect("copy the .dat fixture");
    text
}

/// A bare text file with no index beside it (for the ③/⑦ shapes that need a
/// hand-made or absent `.dat`).
fn bare_text(tag: &str, bytes: &[u8]) -> PathBuf {
    let path = std::env::temp_dir().join(format!("minix-fortune-face-{tag}.db"));
    std::fs::write(&path, bytes).expect("write fortune fixture");
    path
}

/// Run one leg: standard output is the caller's choice (a pipe or
/// `/dev/full`); all three channels are collected.
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

const QUIP_A: &str = "first quip\nsecond quip\n";
const QUIP_B: &str = "third one\n";

#[test]
fn the_c_strfile_fixture_prints_one_of_its_quips_and_exits_zero() {
    let db = fixture("golden", "golden");
    let (out, code, err) = run(&db, Stdio::piped());
    assert!(out == QUIP_A || out == QUIP_B, "output={out:?}");
    assert_eq!((code, err.as_str()), (0, ""), "stderr={err}");
    std::fs::remove_dir_all(db.parent().unwrap()).expect("clean the face directory");
}

#[test]
fn a_single_quip_index_prints_that_quip_byte_for_byte() {
    let db = fixture("single", "single");
    let (out, code, err) = run(&db, Stdio::piped());
    assert_eq!((out.as_str(), code, err.as_str()), ("single one\n", 0, ""));
    std::fs::remove_dir_all(db.parent().unwrap()).expect("clean the face directory");
}

#[test]
fn a_missing_index_answers_not_a_fortune_file() {
    // The text exists; the `.dat` beside it does not. C decides this inside
    // `is_fortfile` (`fortune.c:792-795`) and answers with the warnx shape.
    let db = bare_text("no-dat", b"whatever\n");
    let (out, code, err) = run(&db, Stdio::piped());
    assert_eq!(
        (out.as_str(), code, err.as_str()),
        (
            "",
            1,
            &format!(
                "fortune: `{db}' not a fortune file or directory\n",
                db = db.display()
            )[..]
        ),
        "stderr={err}"
    );
    let _ = std::fs::remove_file(&db);
}

#[test]
fn a_missing_text_file_reports_the_cannot_open_line() {
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
fn an_illegal_suffix_is_rejected_on_the_name_alone() {
    // `fortune X.dat` — "dat" is in the suffix list (`fortune.c:756-762`), so
    // C answers the warnx shape before touching anything.
    let db = bare_text("suffix.dat", b"whatever\n");
    let (out, code, err) = run(&db, Stdio::piped());
    assert_eq!(
        (out.as_str(), code, err.as_str()),
        (
            "",
            1,
            &format!(
                "fortune: `{db}' not a fortune file or directory\n",
                db = db.display()
            )[..]
        ),
        "stderr={err}"
    );
    let _ = std::fs::remove_file(&db);
}

#[test]
fn an_index_recording_zero_quips_says_no_fortunes_in_database() {
    // `empty.txt.dat` records zero strings (the C tool prints
    // "There were 0 strings" for it); C itself divides by the count and dies.
    let db = fixture("empty", "empty");
    let (out, code, err) = run(&db, Stdio::piped());
    assert_eq!(
        (out.as_str(), code, err.as_str()),
        ("", 1, "fortune: no fortunes in database\n"),
        "stderr={err}"
    );
    std::fs::remove_dir_all(db.parent().unwrap()).expect("clean the face directory");
}

#[test]
fn a_truncated_index_is_invalid_fortune_database() {
    // Shorten the golden index into its header only: the seek-point table no
    // longer fits, the reader refuses, and the shell answers the adjudicated
    // sentence.
    let text = bare_text(
        "trunc-text",
        std::fs::read(fixture("trunc-src", "golden"))
            .expect("read golden text")
            .as_slice(),
    );
    let dat_path = std::env::temp_dir().join("minix-fortune-face-trunc-text.db.dat");
    let full =
        std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/golden.txt.dat"))
            .expect("read golden dat");
    std::fs::write(&dat_path, &full[..20]).expect("write truncated dat");
    let (out, code, err) = run(&text, Stdio::piped());
    assert_eq!(
        (out.as_str(), code, err.as_str()),
        ("", 1, "fortune: invalid fortune database\n"),
        "stderr={err}"
    );
    let _ = std::fs::remove_file(&text);
    let _ = std::fs::remove_file(&dat_path);
}

#[test]
fn the_rotated_flag_prints_rot13_text() {
    // `rot13.txt.dat` was produced with `strfile -x` over the golden text: the
    // bytes on disk are plain, the flag says the OUTPUT is rotated.
    let db = fixture("rot13", "rot13");
    let (out, code, err) = run(&db, Stdio::piped());
    let rotated_a: String = QUIP_A
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' => (b'A' + (byte - b'A' + 13) % 26) as char,
            b'a'..=b'z' => (b'a' + (byte - b'a' + 13) % 26) as char,
            other => other as char,
        })
        .collect();
    let rotated_b: String = QUIP_B
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' => (b'A' + (byte - b'A' + 13) % 26) as char,
            b'a'..=b'z' => (b'a' + (byte - b'a' + 13) % 26) as char,
            other => other as char,
        })
        .collect();
    assert!(out == rotated_a || out == rotated_b, "output={out:?}");
    assert_eq!((code, err.as_str()), (0, ""), "stderr={err}");
    std::fs::remove_dir_all(db.parent().unwrap()).expect("clean the face directory");
}

#[test]
fn a_stdout_that_stops_taking_bytes_adds_no_word_and_no_status() {
    // The same database does write one quip and exit 0 in
    // `a_single_quip_index_prints_that_quip_byte_for_byte`, so the 0 here is
    // not faked by "never hit anything".
    let db = fixture("full", "single");
    let file = std::fs::File::create("/dev/full").expect("open /dev/full");
    let (out, code, err) = run(&db, Stdio::from(file));
    assert_eq!(err, "", "标准输出收不下字节时不该有任何一句话：{err}");
    assert_eq!(
        (out.as_str(), code),
        ("", 0),
        "C 的输出腿不查失手（`fortune.c:272-274`），退码不该因写不进去而变"
    );
    std::fs::remove_dir_all(db.parent().unwrap()).expect("clean the face directory");
}
