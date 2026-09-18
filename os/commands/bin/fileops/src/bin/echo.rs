//! Minix-RS echo — the doing half over `minix_fileops::echo`.
//!
//! Ground truth: `minix3/bin/echo/echo.c` (NetBSD 1.19). The deciding half
//! (leading `-n`, single-space separation, trailing newline) lives in the
//! library; this program gathers argv, writes each piece to standard
//! output through `minix_sys::write`, and exits 0, or 1 when a write
//! failed (echo.c:77-78's `ferror` check). The output channel follows the
//! stage contract (99-global-concepts.md §1): no stdio library, no message
//! construction.
//!
//! Two seams carry the hosted-versus-target split, and both swap in one
//! sweep when no_std program images land:
//!
//! - argv gathering uses `std::env::args` here; the target build reads the
//!   birth-chain descriptor through `minix-rt`.
//! - `terminate` below exits through the host runtime, because
//!   `minix_sys::exit` deliberately spins when no process manager answers
//!   (the C `_exit` last resort, `minix3/minix/lib/libc/sys/_exit.c`), which
//!   would hang every hosted run; the target build swaps it for
//!   `minix_sys::exit`. Writes always go through `minix_sys::write`, so a
//!   hosted run without a kernel must fail with exit 1 as soon as the
//!   transport reports its explicit error (that error short-circuits to a
//!   typed `Err` — edge E-SYSCALL-SIGN).

use minix_fileops::echo::echo_emit;
use minix_sys::{write, Fd};

/// Standard output, POSIX `STDOUT_FILENO`.
const STDOUT: Fd = 1;

/// Terminates the process with an exit status.
fn terminate(code: i32) -> ! {
    std::process::exit(code)
}

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();
    let delivered = echo_emit(&args, |piece: &[u8]| write(STDOUT, piece).is_ok());
    if delivered {
        terminate(0);
    } else {
        terminate(1);
    }
}
