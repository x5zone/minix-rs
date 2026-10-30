//! Shared fixture for the end-to-end gates: feeds input into a spawned real child
//! process, and explains why writing standard input cannot panic.
//!
//! Every `*_face.rs` gate is the same three steps: start a real process, write
//! standard input, collect the three channels. The second step has an easily
//! overlooked precondition: **the command may never read standard input at all**.
//! Most C diagnostic branches are placed before any read (`usage()` in
//! `minix3/usr.bin/pr/pr.c`, the out-of-set letter branch in
//! `minix3/usr.bin/vis/vis.c`, the negative-number branch in
//! `minix3/games/primes/primes.c` all have this shape); as soon as the child
//! exits, the read end of the pipe closes, and the parent's `write_all` then gets
//! `BrokenPipe`.
//!
//! Whether that **happens depends on which side is scheduled first**: when the
//! input is small enough to fit in a 64K pipe buffer, the parent usually finishes
//! writing before the child exits, so the same test case is sometimes red and
//! sometimes green. The observed shapes are `pr_face` single-target triple runs
//! with 1 red and 2 green, and `vis_face` red only in the run where several gates
//! were chained in the same container (evidence in
//! `rewrite-notes/18-stage-commands/todo.md`). Treating it as a panic would mean
//! mistaking the gate's own race for a defect in the code under test.
//!
//! So only `BrokenPipe` is allowed through here; any other write error fails the
//! assertion on the spot—otherwise the gate would silently lose its input and
//! pass falsely.

use std::io::ErrorKind;
use std::io::Write as _;
use std::process::Child;

/// Writes `input` into the child's standard input and closes it (closing is
/// required: commands generally read to end of file before finishing).
///
/// In the test cases where the child exits before reading, this write gets
/// `BrokenPipe`; that is the normal outcome of the shape under test, not a gate
/// failure. The verdict still comes from the three channels stdout, stderr, and
/// exit code, which the caller collects with `wait_with_output`, independent of
/// whether the write completed.
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
