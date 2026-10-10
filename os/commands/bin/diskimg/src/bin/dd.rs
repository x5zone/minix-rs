//! Minix-RS `dd` — the execution half over `minix_diskimg::dd`.
//!
//! Ground truth: `minix3/bin/dd/dd.c`. The operand language and the copy
//! plan are the library (`parse_operands`/`CopyPlan`); the block pump is the
//! library `pump` over an [`Io`] seam. This program binds that seam to real
//! file descriptors (`open`/`read`/`write`/`close`), defaults input to
//! standard input and output to standard output, truncates the output unless
//! `conv=notrunc`, and prints the POSIX `records in/out` tally to standard
//! error (C sends the summary to stderr; the copied bytes go to stdout).
//!
//! Wired here: the `bs=` (ibs==obs) unconverted path with `count`/`skip`/
//! `seek`/`conv=sync,noerror,notrunc`. Declared residual (16-stage-drivers
//! §2, E5 command面): the `ibs != obs` blocking machine, the `conv=`
//! case/`swab`/`block`/`unblock`/`cbs` byte transforms, tape `files=`, and
//! `iflag`/`oflag` beyond `append`/`creat` defaults — the pump returns an
//! error on the asymmetric-block case and this program reports it rather
//! than emitting a mis-shaped copy.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use minix_diskimg::dd::{Io, pump, CopyPlan, parse_operands};
use minix_diskimg::ImageError;
use minix_sys::{close, open, Fd};

const O_WRONLY: i32 = 0x1;
const O_CREAT: i32 = 0x200;
const O_TRUNC: i32 = 0x400;

/// An [`Io`] bound to a real descriptor.
struct FileIo {
    fd: Fd,
}

impl Io for FileIo {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, ImageError> {
        minix_sys::read(self.fd, buf).map_err(|_| ImageError::InvalidArgument)
    }

    fn write(&mut self, buf: &[u8]) -> Result<(), ImageError> {
        let mut off = 0usize;
        while off < buf.len() {
            match minix_sys::write(self.fd, &buf[off..]) {
                Ok(0) => return Err(ImageError::InvalidArgument),
                Ok(n) => off += n,
                Err(_) => return Err(ImageError::InvalidArgument),
            }
        }
        Ok(())
    }
}

fn fail(message: &str) -> ! {
    support::warn(message.as_bytes());
    support::warn(b"\n");
    support::terminate(1)
}

/// The whole program body; both `main` forms call it and it never returns.
fn run() -> ! {
    let argv: Vec<String> = support::args();
    let words: Vec<&str> = argv[1..].iter().map(String::as_str).collect();

    let plan: CopyPlan<'_> = match parse_operands(&words) {
        Ok(plan) => plan,
        Err(_) => fail("dd: invalid operand"),
    };

    let in_fd: Fd = match plan.input {
        Some(path) => open(path, 0, 0).unwrap_or_else(|_| fail("dd: cannot open input")),
        None => support::STDIN,
    };
    let out_fd: Fd = match plan.output {
        Some(path) => {
            // C `dd` truncates the output unless `conv=notrunc`.
            let mut flags = O_WRONLY | O_CREAT;
            if !plan.conversions.no_trunc() {
                flags |= O_TRUNC;
            }
            open(path, flags, 0o666).unwrap_or_else(|_| fail("dd: cannot open output"))
        }
        None => support::STDOUT,
    };

    // The pump needs a scratch of one input block; `bs` can be large by the
    // user's own request. An asymmetric `ibs != obs` request is rejected by
    // the pump (declared residual), surfaced as a transfer failure below.
    let bs = plan.input_block as usize;
    if bs == 0 {
        fail("dd: zero block size");
    }
    let mut scratch: Vec<u8> = vec![0u8; bs];

    let mut source = FileIo { fd: in_fd };
    let mut sink = FileIo { fd: out_fd };
    let stats = match pump(&plan, &mut scratch, &mut source, &mut sink) {
        Ok(stats) => stats,
        Err(_) => {
            if plan.input.is_some() {
                let _ = close(in_fd);
            }
            if plan.output.is_some() {
                let _ = close(out_fd);
            }
            fail("dd: transfer failed");
        }
    };

    if plan.input.is_some() {
        let _ = close(in_fd);
    }
    if plan.output.is_some() {
        let _ = close(out_fd);
    }

    // POSIX summary to standard error, full+partial block tallies.
    let mut note = format!(
        "{}+{} records in\n{}+{} records out\n",
        stats.full_in, stats.part_in, stats.full_out, stats.part_out
    );
    if stats.read_errors > 0 {
        note.push_str(&format!("{} records skipped because of errors\n", stats.read_errors));
    }
    support::warn(note.as_bytes());
    support::terminate(0);
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
