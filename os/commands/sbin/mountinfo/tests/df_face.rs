//! End-to-end face for the `df` binary's host-safe shapes, checked against
//! `minix3/bin/df/df.c`.
//!
//! The mount-table legs are the none-target syscall faces, so the host sees
//! the failure sentences; the option parsing runs before any of that and is
//! pinned directly:
//! ① `-P -h` is refused by name (C reaches an `assert(hflag == 0)` in the
//!    `-P` arm, `df.c:455` — this shell turns the abort into a sentence);
//! ② C's two option-conflict sentences (`:140-145`) come out prefixed with
//!    the program name, in C's order;
//! ③ `-t`'s argument handling: a missing argument, a repeated option and an
//!    empty list each answer their own line (`:130-134`, `:250-251`);
//! ④ every letter in the usage string passes the option gate and reaches the
//!    mount-table fetch, which fails on the host with C's sentence;
//! ⑤ an unknown letter answers the usage line (`:512-519`), not a refusal.

use std::process::{Command, Stdio};

fn df_bin() -> &'static str {
    env!("CARGO_BIN_EXE_df")
}

fn run(args: &[&str]) -> (String, i32, String) {
    let out = Command::new(df_bin())
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("spawn df");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        out.status.code().expect("exit code"),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn mount_fetch_failure() -> String {
    String::from("df: retrieving mounted file system information failed\n")
}

fn usage_of(name: &str) -> String {
    format!("Usage: {name} [-agln] [-Ghkm|-ihkm|-Pk] [-t type] [file | file_system ...]\n")
}

#[test]
fn p_and_h_cannot_be_combined() {
    let (out, status, err) = run(&["-P", "-h"]);
    assert_eq!(out.as_str(), "");
    assert_eq!(status, 1);
    assert_eq!(err, "df: -P and -h cannot be combined\n");
}

#[test]
fn h_passes_the_option_gate_to_the_mount_table() {
    let (out, status, err) = run(&["-h"]);
    assert_eq!(out.as_str(), "");
    assert_eq!(status, 1);
    assert_eq!(err, mount_fetch_failure());
}

/// `df.c:140-145`: the two `errx` sentences, program name included, with the
/// `-G` clash checked first.
#[test]
fn the_option_conflicts_answer_c_sentences() {
    for args in [["-G", "-P"].as_slice(), ["-G", "-i"].as_slice()] {
        let (out, status, err) = run(args);
        assert_eq!(out.as_str(), "");
        assert_eq!(status, 1);
        assert_eq!(err, "df: only one of -G and -P or -i may be specified\n");
    }
    let (out, status, err) = run(&["-P", "-i"]);
    assert_eq!(out.as_str(), "");
    assert_eq!(status, 1);
    assert_eq!(err, "df: only one of -P and -i may be specified\n");
}

/// `-t` takes an argument: attached (`-tmfs`), separate (`-t mfs`), only
/// once, and never empty (`df.c:130-134`, `:250-251`).
#[test]
fn the_type_option_answers_its_three_failures() {
    let (_, status, err) = run(&["-t"]);
    assert_eq!(status, 1);
    assert_eq!(
        err,
        format!("df: option requires an argument -- t\n{}", usage_of("df"))
    );

    let (_, status, err) = run(&["-t", "mfs", "-t", "ext2"]);
    assert_eq!(status, 1);
    assert_eq!(err, "df: only one -t option may be specified.\n");

    let (_, status, err) = run(&["-t", ""]);
    assert_eq!(status, 1);
    assert_eq!(err, "df: empty type list\n");

    // Both accepted spellings pass the gate and reach the mount fetch.
    for args in [["-t", "mfs"].as_slice(), ["-tmfs"].as_slice()] {
        let (_, status, err) = run(args);
        assert_eq!(status, 1);
        assert_eq!(err, mount_fetch_failure());
    }
}

/// Every usage letter is accepted now; the reachable proof is that each run
/// reaches the mount-table fetch instead of printing usage or a refusal.
#[test]
fn every_usage_letter_reaches_the_mount_table() {
    for letter in ["-a", "-g", "-G", "-h", "-i", "-k", "-l", "-m", "-n", "-P"] {
        let (out, status, err) = run(&[letter]);
        assert_eq!(out.as_str(), "", "{letter}");
        assert_eq!(status, 1, "{letter}");
        assert_eq!(err, mount_fetch_failure(), "{letter}");
    }
}

#[test]
fn an_unknown_letter_answers_the_usage_line() {
    let (out, status, err) = run(&["-Z"]);
    assert_eq!(out.as_str(), "");
    assert_eq!(status, 1);
    assert_eq!(err, usage_of("df"));
}

/// An operand that cannot be resolved falls into `getmntpt` (`df.c:168-171`);
/// on a host build the mount table itself is unavailable, which is C's fatal
/// `Can't get mount information` leg.
#[test]
fn an_operand_without_a_mount_table_is_fatal() {
    let (out, status, err) = run(&["/tmp/df-face-operand"]);
    assert_eq!(out.as_str(), "");
    assert_eq!(status, 1);
    assert_eq!(err, "df: Can't get mount information\n");
}
