//! Minix-RS true — successful no-op.
//!
//! Ground truth: `minix3/usr.bin/true/true.c` (NetBSD): operands are
//! ignored, the exit status is 0. POSIX likewise specifies that `true`
//! shall do nothing successfully.

fn main() {
    std::process::exit(0);
}
