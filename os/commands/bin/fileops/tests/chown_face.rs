//! End-to-end face for the `chown` binary's option-gate shapes, checked
//! against `minix3/sbin/chown/chown.c`.
//!
//! The owner-change syscalls are the none-target faces, so the host observes
//! the gate decisions and the failure sentences:
//! ① `-R`/`-H`/`-L`/`-P`/`-h` pass the option gate (the old "recursive/link
//!    handling not wired" refusal is gone);
//! ② `-R -L -h` answers C's contradiction sentence (`chown.c:150-155`);
//! ③ a refused named owner keeps C's invalid-name verdict (batch 494).

use std::process::{Command, Stdio};

fn chown_bin() -> &'static str {
    env!("CARGO_BIN_EXE_chown")
}

fn run(args: &[&str]) -> (String, i32, String) {
    let out = Command::new(chown_bin())
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("spawn chown");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        out.status.code().expect("exit code"),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn the_recursion_and_link_letters_pass_the_gate() {
    // On the host the walk's syscalls are the none-target legs, so the run
    // ends in per-path failure sentences — never the old refusal.
    for letter in ["-R", "-H", "-L", "-P", "-h", "-R -h"] {
        let args: Vec<&str> = letter
            .split(' ')
            .chain(["0", "/tmp/chown-face-absent"])
            .collect();
        let (_, status, err) = run(&args);
        assert_eq!(status, 1, "{letter}: {err}");
        assert!(!err.contains("not wired"), "{letter} 不该再报未接线：{err}");
    }
}

#[test]
fn logical_and_h_together_are_refused_with_c_sentence() {
    let (out, status, err) = run(&["-R", "-L", "-h", "0", "/tmp/x"]);
    assert_eq!(out.as_str(), "");
    assert_eq!(status, 1);
    assert_eq!(
        err,
        "chown: the -L and -h options may not be specified together.\n"
    );
}

#[test]
fn a_named_owner_keeps_the_invalid_name_verdict() {
    let (_, status, err) = run(&["root", "/tmp/chown-face-absent"]);
    assert_eq!(status, 1);
    assert_eq!(err, "chown: root: invalid user name\n");
}
