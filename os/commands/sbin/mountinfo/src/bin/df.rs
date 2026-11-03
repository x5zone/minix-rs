//! Minix-RS `df` — the doing half over `minix_mountinfo::df`.
//!
//! Ground truth: `minix3/bin/df/df.c`. With no operands `df` asks the VFS for
//! every mounted file system (`getmntinfo` → `getvfsstat`, which the tree
//! serves with `VFS_GETVFSSTAT`) and prints one row per record; with file
//! operands it stats each (`statvfs`) and prints the filesystem behind it.
//! The block arithmetic and the `-P` row layout live in the library
//! (`crate::df`); this program only drives the two VFS calls and emits rows.
//!
//! Wired: the POSIX `-P`/`-k` view (1024-block units) and the human-readable
//! `-h` view (C's default layout with `Size` figures, `df.c:491-509`); the
//! last size flag wins like C's own assignments. Declared subset residual:
//! the 512-block legacy aligned layout and the `-t`/`-i`/`-g`/`-G`/`-a`/`-l`
//! filter/format faces are not modelled here — requesting them reports
//! "option not wired" and exits 1, rather than emitting a wrong-looking
//! table. `-P -h` refuses by name: C's `-P` arm asserts `hflag == 0`
//! (`df.c:455`), an abort this shell turns into a sentence.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::string::String;
use alloc::vec::Vec;
use minix_mountinfo::df::{P_HEADER, format_h_row, format_p_row, h_header};
use minix_sys::{StatvfsBuf, statvfs_off};

/// Matches the VFS mount table width (`NR_MNTS`).
const MAX_MOUNTS: usize = 16;

fn fail(message: &str) -> ! {
    // The hosted stderr twin — the raw `write` leg exists only for the
    // target face, and the face tests need the sentence.
    let mut line = String::from(message);
    line.push('\n');
    support::warn(line.as_bytes());
    support::terminate(1)
}

/// Which figures the report prints (`df.c`'s `hflag`; the last size flag
/// wins).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SizeMode {
    Blocks,
    Human,
}

/// The whole program body; both `main` forms call it and it never returns.
fn run() -> ! {
    let argv: Vec<String> = support::args();
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();

    let mut operands: Vec<&str> = Vec::new();
    let mut mode = SizeMode::Blocks;
    let mut posix = false;
    for arg in &args[1..] {
        if arg.starts_with('-') && arg.len() > 1 && *arg != "-" {
            for c in arg[1..].chars() {
                match c {
                    // `-k` selects 1024-block figures; `-h` switches the
                    // same layout to humanised ones; the last of C's size
                    // flags wins (`df.c:95-129` assignments). `-P` requires
                    // block figures.
                    'k' => mode = SizeMode::Blocks,
                    'h' => mode = SizeMode::Human,
                    'P' => {
                        if mode == SizeMode::Human {
                            fail("df: -P and -h cannot be combined");
                        }
                        posix = true;
                    }
                    _ => fail("df: option not wired (POSIX -Pk view only)"),
                }
            }
        } else {
            operands.push(*arg);
        }
    }

    // C checks `-P -h` only through an assert in the `-P` arm (`df.c:455`,
    // reached as `hflag == 0`); this shell refuses the pair by name instead
    // of aborting.
    if posix && mode == SizeMode::Human {
        fail("df: -P and -h cannot be combined");
    }
    let human = mode == SizeMode::Human;

    // Collect the records first: the `-h` header and rows need the widest
    // mount-from name across the whole table (`df.c:204-210`).
    let mut records: Vec<(String, String, StatvfsBuf)> = Vec::new();
    if operands.is_empty() {
        let mut mounts = [StatvfsBuf::new(); MAX_MOUNTS];
        match minix_sys::getvfsstat(&mut mounts) {
            Ok(count) => {
                for buf in &mounts[..count.min(MAX_MOUNTS)] {
                    records.push((
                        buf.get_name(statvfs_off::MNTFROMNAME),
                        buf.get_name(statvfs_off::MNTONNAME),
                        *buf,
                    ));
                }
            }
            Err(_) => fail("df: retrieving mounted file system information failed"),
        }
    } else {
        for path in operands {
            let mut buf = StatvfsBuf::new();
            match minix_sys::statvfs(path, &mut buf) {
                Ok(()) => records.push((
                    buf.get_name(statvfs_off::MNTFROMNAME),
                    buf.get_name(statvfs_off::MNTONNAME),
                    buf,
                )),
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
    let maxwidth = records
        .iter()
        .map(|(from, _, _)| from.len())
        .max()
        .unwrap_or(0);

    let mut out = [0u8; 512];
    if human {
        support::emit(h_header(maxwidth).as_bytes());
    } else {
        support::emit(P_HEADER.as_bytes());
    }
    for (from, to, buf) in &records {
        let used = if human {
            format_h_row(
                from,
                to,
                maxwidth,
                buf.get_u64(statvfs_off::BLOCKS),
                buf.get_u64(statvfs_off::BFREE),
                buf.get_u64(statvfs_off::BRESVD),
                buf.get_u64(statvfs_off::FRSIZE),
                &mut out,
            )
        } else {
            format_p_row(
                from,
                to,
                buf.get_u64(statvfs_off::BLOCKS),
                buf.get_u64(statvfs_off::BFREE),
                buf.get_u64(statvfs_off::BRESVD),
                buf.get_u64(statvfs_off::FRSIZE),
                &mut out,
            )
        };
        if let Ok(used) = used {
            support::emit(&out[..used]);
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
