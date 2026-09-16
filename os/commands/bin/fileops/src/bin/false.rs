//! Minix-RS false — failing no-op.
//!
//! Ground truth: `minix3/usr.bin/false/false.c` (NetBSD): operands are
//! ignored, the exit status is 1. POSIX likewise specifies that `false`
//! shall do nothing unsuccessfully.

fn main() {
    std::process::exit(1);
}
