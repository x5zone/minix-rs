//! `fsck.mfs` thin shell: runs a read-only consistency check over an image
//! file and reports the result.
//!
//! C counterpart: `main` (1616-1668) in
//! `minix3/minix/commands/fsck.mfs/fsck.c` plus the assembly order of
//! `chkdev`. The decision half lives in `minix_fs_mfs::fsck` (superblock
//! validation, walk, bitmap comparison and counter reconciliation all live in
//! the format library); this program does four things only — flag parsing,
//! reading the image, printing the report, and the exit code.
//!
//! Flag face (what the read-only decision half supports): `-l` walk list, `-s`
//! superblock listing, `-i N...` inode watch list, `-z N...` zone watch list,
//! `-f` no-op (same as C). The repair flags (`-r`/`-y`/`-a`/`-p`/`-d`) and `-c`
//! (explicit inode list, C `lsi`'s prompt face) belong to the repair face,
//! which is not built — they are refused honestly rather than faked as
//! success.
//!
//! Exit codes follow C `sbin/fsck/exitvalues.h`: check completed 0
//! (inconsistencies do not change the exit code, C `main` always returns 0;
//! non-interactive consumers read the report text), usage 1, check fatal 8
//! (message plus a trailing `fatal`). Multiple images are checked one by one
//! and the watch lists reset after each image (the same as C clearing the
//! lists after `chkdev`).


#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use alloc::format;

#[path = "../bin_support.rs"]
mod support;
use minix_fs_mfs::fsck::{self, Fsck, FsckOptions};

fn usage(program: &str) -> ! {
    support::warn(format!("usage: {program} [-lfs] [-i ino]... [-z zone]... image...\n").as_bytes());
    support::terminate(fsck::EXIT_USAGE);
}

fn run() -> ! {
    let args: Vec<String> = support::args();
    let program = args.first().map(String::as_str).unwrap_or("fsck.mfs");
    let mut listing = false;
    let mut want_super = false;
    let mut watch_inodes: Vec<u64> = Vec::new();
    let mut watch_zones: Vec<u64> = Vec::new();
    let mut images: Vec<String> = Vec::new();
    let mut bad_flag = false;

    // C's flag shape: a single dash and one letter (arg[2] == 0); -i/-z
    // swallow the run of numeric items after them and hand back to the outer
    // loop at the first non-number (same shape as getlist).
    let mut index = 1;
    while index < args.len() {
        let arg = &args[index];
        let bytes = arg.as_bytes();
        if bytes.len() == 2 && bytes[0] == b'-' && bytes[1] != b'-' {
            match bytes[1] {
                b'l' => listing = true,
                b'f' => {}
                b's' => want_super = true,
                b'i' | b'z' => {
                    let list =
                        if bytes[1] == b'i' { &mut watch_inodes } else { &mut watch_zones };
                    index += 1;
                    while index < args.len() {
                        match args[index].parse::<u64>() {
                            Ok(number) => {
                                list.push(number);
                                index += 1;
                            }
                            Err(_) => break,
                        }
                    }
                    continue;
                }
                b'r' | b'y' | b'a' | b'p' | b'd' | b'c' => {
                    support::warn(
                        format!(
                            "{program}: {arg} needs the repair face (deferred); \
                         only the read-only decision half is built\n"
                        )
                        .as_bytes(),
                    );
                    support::terminate(fsck::EXIT_USAGE);
                }
                _ => {
                    support::warn(format!("{program}: unknown flag '{arg}'\n").as_bytes());
                    bad_flag = true;
                }
            }
        } else {
            images.push(arg.clone());
        }
        index += 1;
    }
    if images.is_empty() || bad_flag {
        usage(program);
    }

    for image_path in &images {
        // A failed C devopen goes to fatal (fsck.c:384-391).
        // File reading goes through `support::read_file`'s dual form (host
        // std::fs, target minix_sys open/read/close); the failure reason is
        // flattened to a plain existence check at the seam — C's strerror
        // detail is not modelled, as on the hosted io::Error path.
        let image = match support::read_file(image_path) {
            Ok(bytes) => bytes,
            Err(_) => {
                support::emit(format!("fsck.mfs: cannot read {image_path}\n").as_bytes());
                support::emit(b"couldn't open device to fsck\nfatal\n");
                support::terminate(fsck::EXIT_CHECK_FAILED);
            }
        };
        let mut checker = match Fsck::new(&image) {
            Ok(checker) => checker,
            Err(fatal) => {
                support::emit(format!("{}\nfatal\n", fatal.message).as_bytes());
                support::terminate(fsck::EXIT_CHECK_FAILED);
            }
        };
        if want_super {
            // C's lsuper prints right after the superblock is read
            // (fsck.c:577); here it prints after the static validation, with
            // the same field face.
            support::emit(format!("{}", checker.list_super()).as_bytes());
        }
        let options = FsckOptions {
            listing,
            watch_inodes: watch_inodes.clone(),
            watch_zones: watch_zones.clone(),
        };
        match checker.run(&options) {
            Ok(_summary) => {
                for message in checker.messages() {
                    support::emit(format!("{message}\n").as_bytes());
                }
            }
            Err(fatal) => {
                for message in checker.messages() {
                    support::emit(format!("{message}\n").as_bytes());
                }
                support::emit(format!("{}\nfatal\n", fatal.message).as_bytes());
                support::terminate(fsck::EXIT_CHECK_FAILED);
            }
        }
        // Multiple images: the watch lists reset after each image
        // (fsck.c:1660-1662).
        watch_inodes.clear();
        watch_zones.clear();
    }
    support::terminate(0)
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
