//! End-to-end face for the `chmod` binary's option-gate shapes, checked
//! against `minix3/bin/chmod/chmod.c`.
//!
//! The mode syscalls are the none-target faces, so the host observes the
//! gate decisions:
//! ① `-H`/`-L`/`-P`/`-h` pass the option gate (the old "not wired" refusal
//!    is gone), and `-R` still walks;
//! ② `-R -h` answers C's contradiction sentence (`chmod.c:145-150`);
//! ③ an unknown letter keeps the usage line.

use std::process::{Command, Stdio};

fn chmod_bin() -> &'static str {
    env!("CARGO_BIN_EXE_chmod")
}

fn run(args: &[&str]) -> (String, i32, String) {
    let out = Command::new(chmod_bin())
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("spawn chmod");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        out.status.code().expect("exit code"),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn the_traversal_and_link_letters_pass_the_gate() {
    for letter in ["-H", "-L", "-P", "-h", "-RH", "-RL", "-RP"] {
        let args: Vec<&str> = [letter, "755", "/tmp/chmod-face-absent"].to_vec();
        let (_, status, err) = run(&args);
        assert_eq!(status, 1, "{letter}: {err}");
        assert!(!err.contains("not wired"), "{letter} 不该再报未接线：{err}");
    }
}

#[test]
fn recursive_and_h_together_are_refused_with_c_sentence() {
    let (out, status, err) = run(&["-R", "-h", "755", "/tmp/x"]);
    assert_eq!(out.as_str(), "");
    assert_eq!(status, 1);
    assert_eq!(
        err,
        "chmod: the -R and -h options may not be specified together.\n"
    );
}

#[test]
fn an_unknown_letter_keeps_the_usage_line() {
    let (_, status, err) = run(&["-Z", "755", "f"]);
    assert_eq!(status, 1);
    assert_eq!(
        err,
        "usage: chmod [-R [-H | -L | -P]] [-fhv] mode file ...\n"
    );
}
