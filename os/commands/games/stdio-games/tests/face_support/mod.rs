//! Shared fixture for the end-to-end gates: feed input to an already-started real
//! child process, and explain why writing standard input must not panic.
//!
//! Every `*_face.rs` gate follows the same three steps: start a real process, write
//! its standard input, and collect three channels. The second step has an easily
//! overlooked precondition: **the command may never read standard input at all**.
//! Most C diagnostic branches come before any read (`usage()` in
//! `minix3/usr.bin/pr/pr.c`, the out-of-set-letter branch in
//! `minix3/usr.bin/vis/vis.c`, and the negative-number branch in
//! `minix3/games/primes/primes.c` are all shaped that way); once the child exits,
//! the read end of the pipe closes and the parent's `write_all` gets `BrokenPipe`.
//!
//! Whether this **happens depends on which of parent and child is scheduled first**:
//! when the input is small enough to fit a 64K pipe buffer, the parent usually
//! finishes writing before the child exits, so the same case goes red or green at
//! random. Measured shape: `pr_face` in a single-target triple run went 1 red, 2
//! green, and `vis_face` was the only red one in a same-container multi-gate run
//! (the evidence is recorded in `rewrite-notes/18-stage-commands/todo.md`).
//! Treating it as a panic mistakes the gate's own race for a defect in the code
//! under test.
//!
//! So only `BrokenPipe` is tolerated here; any other write error asserts immediately
//! — otherwise a gate would silently lose its input and pass falsely.

use std::io::ErrorKind;
use std::io::Write as _;
use std::process::Child;

/// Write `input` into the child's standard input and close it (closing is required:
/// commands generally read to end-of-file before finishing).
///
/// In cases where the child exits before reading, this write gets `BrokenPipe`; that
/// is a normal outcome of the shape under test, not a gate failure. The verdicts
/// still come from the three channels stdout, stderr, and exit code, which the
/// caller's `wait_with_output` collects regardless of whether the write completed.
pub fn write_stdin(child: &mut Child, input: &[u8]) {
    let mut stdin = child.stdin.take().expect("stdin piped");
    if let Err(err) = stdin.write_all(input) {
        assert_eq!(
            err.kind(),
            ErrorKind::BrokenPipe,
            "写标准输入只该因为「子进程已退出」而失败，实际是 {err}"
        );
    }
}
