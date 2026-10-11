//! Minix-RS `printroot` — the doing half over `minix_sysinfo::rootdev` and
//! `minix_sysinfo::fsversion`.
//!
//! Ground truth: `minix3/minix/commands/printroot/printroot.c`. It stats the
//! root directory, walks `/dev/` and prints the first block device whose
//! `st_rdev` equals the root's `st_dev`; when none matches it reports
//! `/dev/unknown` with exit status 1. Unless `-r` is given it also prints
//! `" / "` followed by the file system version and `" rw"` (via `fsversion`).
//!
//! Reads only (no filesystem writes), so it is not the writable-root `E5`
//! gate: the real run just needs the mounted root and its `/dev` entries. The
//! discovery matching rides the tested [`rootdev::discover_root`]; the version
//! magic classification rides [`fsversion`], leaving this shell to stat the
//! device directory and open the matched device.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use minix_sys::{close, getdents, lseek, open, read, stat, Stat};
use minix_sysinfo::fsversion::{
    self, classify_super, is_iso9660_descriptor, version_tag, FsVersion, SUPER_BLOCK_BYTES,
};
use minix_sysinfo::rootdev::{DeviceEntry, SliceDeviceDir, DEVICE_DIRECTORY, UNKNOWN_DEVICE};

/// `sys/stat.h` file-type mask and block-device bits.
const S_IFMT: u32 = 0o170_000;
const S_IFBLK: u32 = 0o060_000;
/// `unistd.h` `SEEK_SET`.
const SEEK_SET: i32 = 0;
/// `syslimits.h` `NAME_MAX` (C `namebuf[sizeof DEV_PATH + NAME_MAX]`).
const NAME_MAX: usize = 511;
/// Root directory operand (`printroot.c` `ROOT`).
const ROOT: &str = "/";

/// `printroot.c done()`: print the name; with `-r` just the name plus a
/// newline; otherwise `" / "`, the file system tag, and `" rw"`. Exits with
/// the discovery status (0 found, 1 unknown). Never returns.
fn done(name: &str, status: i32, rflag: bool) -> ! {
    support::emit(name.as_bytes());
    if rflag {
        support::emit(b"\n");
        support::terminate(status);
    }
    support::emit(b" / ");
    let version = probe_fsversion(name);
    support::emit(format!("{} rw\n", version_tag(version)).as_bytes());
    support::terminate(status)
}

/// The device special open + super-block read that `fsversion.c` does; the
/// magic classification itself lives in [`fsversion`]. A failure to open or to
/// read a full super block is the C `-1` return (no match), rendered `None`.
fn probe_fsversion(path: &str) -> Option<FsVersion> {
    let fd = open(path, 0, 0).ok()?;
    let mut superblk = vec![0u8; SUPER_BLOCK_BYTES];

    if lseek(fd, SUPER_BLOCK_BYTES as i64, SEEK_SET).is_err() {
        close_fd(fd);
        return None;
    }
    match read(fd, &mut superblk) {
        Ok(n) if n == SUPER_BLOCK_BYTES => {}
        _ => {
            // C: "cannot read super block" is an error return (-1), not the
            // ISO fallback: the ISO probe only runs after a full super read.
            close_fd(fd);
            return None;
        }
    }
    if let Some(version) = classify_super(&superblk) {
        close_fd(fd);
        return Some(version);
    }

    // No MFS/ext2 magic: read the ISO 9660 primary volume descriptor region.
    if lseek(fd, fsversion::ISO9660_VOLUME_OFFSET, SEEK_SET).is_ok()
        && read(fd, &mut superblk).ok() == Some(SUPER_BLOCK_BYTES)
        && is_iso9660_descriptor(&superblk)
    {
        close_fd(fd);
        return Some(FsVersion::Iso9660);
    }
    close_fd(fd);
    None
}

/// Close a device fd, ignoring errors (the C `close(fd)` return is unused on
/// these paths).
fn close_fd(fd: i32) {
    // close after fsversion probe; fd no longer used.
    let _ = close(fd);
}

/// Walk `/dev/` collecting `(leaf name, st_rdev, is_block)` for every entry
/// whose `stat` succeeds, mirroring C's `opendir` + `readdir` + `stat` loop.
/// Returns `None` if `/dev/` cannot be opened.
fn collect_dev_entries() -> Option<Vec<(String, u64, bool)>> {
    let fd = open(DEVICE_DIRECTORY, 0, 0).ok()?;
    let mut entries: Vec<(String, u64, bool)> = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        let transferred = match getdents(fd, &mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(_) => {
                close_fd(fd);
                return None;
            }
        };
        let mut cursor = 0;
        while cursor < transferred {
            let record = &buf[cursor..transferred];
            if record.len() < 13 {
                break;
            }
            let reclen = u16::from_ne_bytes([record[8], record[9]]) as usize;
            if reclen == 0 || cursor + reclen > transferred {
                break;
            }
            let namlen = u16::from_ne_bytes([record[10], record[11]]) as usize;
            if 13 + namlen > record.len() {
                break;
            }
            let leaf = String::from_utf8_lossy(&record[13..13 + namlen]).into_owned();
            cursor += reclen;

            let path = format!("{DEVICE_DIRECTORY}{leaf}");
            let mut st: Stat = unsafe { core::mem::zeroed() };
            if stat(&path, &mut st).is_err() {
                continue;
            }
            let is_block = st.st_mode & S_IFMT == S_IFBLK;
            entries.push((leaf, st.st_rdev, is_block));
        }
    }
    close_fd(fd);
    Some(entries)
}

/// The whole program body; both `main` forms call it and it never returns.
fn run() -> ! {
    let argv: Vec<String> = support::args();
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();
    // C: `rflag = (argc > 1 && strcmp(argv[1], "-r") == 0)` — only argv[1].
    let rflag = args.len() > 1 && args[1] == "-r";

    // C: `if (stat(ROOT, &rootstat) == 0 && (dp = opendir(DEV_PATH)) != NULL)`.
    let mut rootstat: Stat = unsafe { core::mem::zeroed() };
    if stat(ROOT, &mut rootstat).is_err() {
        done(UNKNOWN_DEVICE, 1, rflag);
    }
    let collected = match collect_dev_entries() {
        Some(entries) => entries,
        None => done(UNKNOWN_DEVICE, 1, rflag),
    };

    // Feed the tested discovery half: only block devices can hold a root.
    let dir_entries: Vec<DeviceEntry> = collected
        .iter()
        .map(|(leaf, dev, is_block)| DeviceEntry {
            name: leaf.as_str(),
            device: *dev,
            is_block: *is_block,
        })
        .collect();
    let dir = SliceDeviceDir::new(&dir_entries);
    let mut out = vec![0u8; DEVICE_DIRECTORY.len() + NAME_MAX + 1];
    match rootdev_discover(&dir, rootstat.st_dev, &mut out) {
        Some(discovery) => {
            let name = String::from(discovery.name);
            let status = if discovery.found { 0 } else { 1 };
            done(&name, status, rflag);
        }
        // Only reachable if the composed name overflowed the buffer; C has no
        // such failure mode (fixed-size namebuf), so report the fallback.
        None => done(UNKNOWN_DEVICE, 1, rflag),
    }
}

/// Thin wrapper over [`rootdev::discover_root`] (via `minix_sysinfo::rootdev`)
/// so the `Result` is folded into an `Option` at one site.
fn rootdev_discover<'a>(
    dir: &SliceDeviceDir<'a>,
    root_device: u64,
    out: &'a mut [u8],
) -> Option<minix_sysinfo::rootdev::Discovery<'a>> {
    minix_sysinfo::rootdev::discover_root(dir, root_device, out).ok()
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
