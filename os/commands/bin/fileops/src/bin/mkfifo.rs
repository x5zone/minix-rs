//! Minix-RS `mkfifo` — the doing half over `minix_fileops::mkfifo`.
//!
//! Ground truth: `minix3/usr.bin/mkfifo/mkfifo.c`. The mode decision — default
//! `0666 & ~umask`, or `-m` over the assumed `a=rw` base — lives in
//! `minix_fileops::mkfifo`; this program supplies the `umask` and `mkfifo(2)`
//! legs and the per-name loop (`mkfifo.c:69`, `:71-91`, `:93-98`).
//!
//! Wired in full: `mkfifo [-m mode] fifoname ...`. This command has no declared
//! residual: `-m` is the only option C takes, and the mode engine it uses is the
//! same `setmode`/`getmode` port `chmod` runs on.
//!
//! Two deliberate differences, both cosmetic or invisible:
//! - C's failure report is `warn("%s", *argv)` (`mkfifo.c:95`), i.e. the name
//!   with the program name and a `strerror` suffix. That suffix is not wired here
//!   yet (the table exists in `minix_types::Errno::message`; wiring is a registered
//!   follow-up), so the line is the conventional `mkfifo: <name>`.
//! - C reads the creation mask with `umask(0)` and never puts it back
//!   (`mkfifo.c:69`). Nothing else in the program creates a file, and Minix3's
//!   file servers do not apply the mask to `mknod`/`mkfifo` at all, so the
//!   difference cannot be observed; this program restores it anyway, since
//!   leaving the mask at zero is a side effect with no purpose to justify it.
//!
//! Real-machine effect is the writable-root gate (`C-57`/E5), as for every
//! write-side command: the root image is the read-only boot-shim `imgrd`.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use minix_fileops::mkfifo::{MkfifoArgError, MkfifoPlan, parse_args};
use minix_sys::{mkfifo, umask};

fn usage() -> ! {
    // `mkfifo.c:103-107`: one line, then exit 1.
    support::warn(b"usage: mkfifo [-m mode] fifoname ...\n");
    support::terminate(1);
}

fn run() -> ! {
    let argv: Vec<String> = support::args();
    if argv.is_empty() {
        usage();
    }
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();

    // `mkfifo.c:69`: the mask is read by setting it to zero. The value it hands
    // back is what the default mode is measured against.
    let myumask = match umask(0) {
        Ok(prev) => {
            let _ = umask(prev);
            prev
        }
        // A failed `umask` leaves the program with no mask to subtract, which is
        // the same answer C would produce for a caller whose mask is already 0.
        Err(_) => 0,
    };

    let plan: MkfifoPlan = match parse_args(&args[1..], myumask) {
        Ok(p) => p,
        Err(MkfifoArgError::Usage) => usage(),
        Err(MkfifoArgError::BadMode(spec)) => {
            // `mkfifo.c:74-76`: `err(1, "Cannot set file mode \`%s'")` — quoted
            // text, program-name prefix, and it stops before creating anything.
            support::warn(format!("mkfifo: Cannot set file mode `{spec}'\n").as_bytes());
            support::terminate(1);
        }
    };

    // `mkfifo.c:93-99`: every name gets the same mode, each failure is reported
    // and counted, and the run continues to the next name.
    let mut any_failed = false;
    for name in &plan.names {
        if mkfifo(name, plan.mode).is_err() {
            support::warn(format!("mkfifo: {name}\n").as_bytes());
            any_failed = true;
        }
    }
    support::terminate(if any_failed { 1 } else { 0 });
}

#[cfg(all(not(test), target_os = "none"))]
#[unsafe(no_mangle)]
extern "Rust" fn main() -> i32 {
    run()
}

#[cfg(any(test, not(target_os = "none")))]
fn main() {
    run()
}
