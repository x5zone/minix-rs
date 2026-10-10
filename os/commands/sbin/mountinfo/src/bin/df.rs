//! Minix-RS `df` — the doing half over `minix_mountinfo::df`.
//!
//! Ground truth: `minix3/bin/df/df.c`. With no operands `df` asks the VFS for
//! every mounted file system (`getmntinfo` → `getvfsstat`, which the tree
//! serves with `VFS_GETVFSSTAT`) and prints one row per record; with file
//! operands it stats each (`statvfs`) and prints the filesystem behind it.
//! The block arithmetic and the `-P` row layout live in the library
//! (`crate::df`); this program only drives the two VFS calls and emits rows.
//!
//! Wired: the POSIX `-P`/`-k` view (1024-block units), which the deciding
//! half renders exactly. Declared subset residual: the human-readable `-h`,
//! the 512-block legacy aligned layout, and the `-t`/`-i`/`-g`/`-G`/`-a`/
//! `-l` filter/format faces are not modelled here — requesting them reports
//! "option not wired" and exits 1, rather than emitting a wrong-looking table.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::string::String;
use alloc::vec::Vec;
use minix_mountinfo::df::{format_p_row, P_HEADER};
use minix_sys::write;
use minix_types::{StatvfsBuf, statvfs_off};

const STDERR: i32 = 2;
/// Matches the VFS mount table width (`NR_MNTS`).
const MAX_MOUNTS: usize = 16;

fn fail(message: &str) -> ! {
    let _ = write(STDERR, message.as_bytes());
    let _ = write(STDERR, b"\n");
    support::terminate(1)
}

/// Render one mount record and write it to standard output.
fn emit_row(buf: &StatvfsBuf) {
    let from = buf.get_name(statvfs_off::MNTFROMNAME);
    let to = buf.get_name(statvfs_off::MNTONNAME);
    let mut out = [0u8; 512];
    if let Ok(used) = format_p_row(
        &from,
        &to,
        buf.get_u64(statvfs_off::BLOCKS),
        buf.get_u64(statvfs_off::BFREE),
        buf.get_u64(statvfs_off::BRESVD),
        buf.get_u64(statvfs_off::FRSIZE),
        &mut out,
    ) {
        support::emit(&out[..used]);
    }
}

/// The whole program body; both `main` forms call it and it never returns.
fn run() -> ! {
    let argv: Vec<String> = support::args();
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();

    let mut operands: Vec<&str> = Vec::new();
    for arg in &args[1..] {
        if arg.starts_with('-') && arg.len() > 1 && *arg != "-" {
            for c in arg[1..].chars() {
                match c {
                    // Both select the 1024-block POSIX report the deciding
                    // half renders; other faces are declared unwired above.
                    'P' | 'k' => {}
                    _ => fail("df: option not wired (POSIX -Pk view only)"),
                }
            }
        } else {
            operands.push(*arg);
        }
    }

    support::emit(P_HEADER.as_bytes());

    if operands.is_empty() {
        let mut mounts = [StatvfsBuf::new(); MAX_MOUNTS];
        match minix_sys::getvfsstat(&mut mounts) {
            Ok(count) => {
                for buf in &mounts[..count.min(MAX_MOUNTS)] {
                    emit_row(buf);
                }
            }
            Err(_) => fail("df: retrieving mounted file system information failed"),
        }
    } else {
        for path in operands {
            let mut buf = StatvfsBuf::new();
            match minix_sys::statvfs(path, &mut buf) {
                Ok(()) => emit_row(&buf),
                Err(_) => {
                    // C `warn`s the operand and continues (`df.c:198`).
                    let mut line = String::from("df: ");
                    line.push_str(path);
                    line.push('\n');
                    support::warn(line.as_bytes());
                }
            }
        }
    }
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
