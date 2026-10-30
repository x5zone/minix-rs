//! `mkfs.mfs` thin shell: build a third-version Minix file system on an image
//! file — an empty volume, or one seeded from a prototype file.
//!
//! C counterpart: `main` in
//! `minix3/minix/usr.sbin/mkfs.mfs/mkfs.c`. The deciding half lives in
//! `minix_fs_mfs::mkfs` (layout arithmetic, image building, root directory
//! assembly, prototype seeding are all in the format library); this program
//! does four things only — parse arguments, read the prototype, feed host
//! files, write the image.
//!
//! Argument shape (host face): `mkfs.mfs <image> <blocks> [inodes] [block-size]
//! [-p proto]`. When `-p` is given the C `simple == 0` branch is taken: the
//! inode count on the prototype header's second line wins over the positional
//! argument (C `main` overwrites it directly), and a header block count
//! exceeding the positional argument is refused (C mkfs.c:305-308). Plain
//! files in an entry are read from the host, content and mtime, through
//! `StdHost`. C's getopt face (`-b`/`-i`/`-l`/`-z`/`-T`) and real
//! block-device writes are aligned together once the block-device path lands.


#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use alloc::format;

#[path = "../bin_support.rs"]
mod support;
use minix_fs_mfs::mkfs::{self, MkfsError, ProtoHost};
use core::cell::RefCell;

fn usage(program: &str) -> ! {
    support::warn(format!("usage: {program} <image> <blocks> [inodes] [block-size] [-p proto]\n").as_bytes());
    support::terminate(2);
}

fn fail(message: &str) -> ! {
    support::warn(format!("mkfs.mfs: {message}\n").as_bytes());
    support::terminate(1);
}

/// The production half of the host file reader: whole-file read, mtime taken
/// from metadata; the failing path stays in `last_path` so the error report
/// can name it (C's "Can't open %s").
struct StdHost {
    last_path: RefCell<String>,
}

impl ProtoHost for StdHost {
    fn read_file(&mut self, path: &str, out: &mut [u8]) -> Result<mkfs::HostFile, MkfsError> {
        *self.last_path.borrow_mut() = path.to_string();
        let bytes = support::read_file(path).map_err(|_| MkfsError::HostFile)?;
        if bytes.len() > out.len() {
            return Err(MkfsError::BufferTooSmall);
        }
        out[..bytes.len()].copy_from_slice(&bytes);
        // mtime is the current instant (the prototype file's metadata
        // modification timestamp); the target side's clock face is SYS_TIMES
        // synthesized seconds (`support::epoch_micros`), and the difference
        // from the hosted metadata query is recorded in the bin_support
        // header.
        let mtime = (support::epoch_micros().unwrap_or(0) / 1_000_000) as i32;
        Ok(mkfs::HostFile { len: bytes.len(), mtime })
    }
}

fn explain(error: MkfsError, host: &StdHost) -> ! {
    match error {
        MkfsError::TooSmall => fail("Block count too small"),
        MkfsError::TooFewInodes => fail("Inode count too small"),
        MkfsError::MapsTooLarge => fail("bit maps too large"),
        MkfsError::BufferTooSmall => fail("out of memory"),
        MkfsError::ProtoTooLarge { given, needed } => fail(&format!(
            "number of blocks given as parameter({given}) is too small for given proto file({needed})."
        )),
        MkfsError::NotEnoughInodes { have } => fail(&format!(
            "File system does not have enough inodes (only {have})"
        )),
        MkfsError::NotEnoughZones => fail("File system not big enough for all the files"),
        MkfsError::ProtoBadLine => fail("malformed prototype line"),
        MkfsError::ProtoBadMode => fail("bad mode in prototype line"),
        MkfsError::ProtoUnexpectedEnd => fail("Unexpected end-of-file"),
        MkfsError::SymlinkTooLong => fail("symlink too long"),
        MkfsError::DirectoryTooLarge => fail("directory beyond single indirect blocks"),
        MkfsError::FileTooLarge => fail("File has grown beyond double indirect"),
        MkfsError::TooManyLinks => fail("Too many links to a directory"),
        MkfsError::SizeTooLarge => fail("File has become too big to be handled by MFS"),
        MkfsError::HostFile => fail(&format!("Can't open {}", host.last_path.borrow())),
    }
}

fn run() -> ! {
    let args: Vec<String> = support::args();
    // -p <proto> may appear anywhere; everything else is taken positionally.
    let mut proto_path: Option<String> = None;
    let mut positional: Vec<String> = Vec::new();
    let mut at = 1;
    while at < args.len() {
        if args[at] == "-p" {
            let Some(path) = args.get(at + 1) else { usage(args[0].as_str()) };
            proto_path = Some(path.clone());
            at += 2;
        } else {
            positional.push(args[at].clone());
            at += 1;
        }
    }
    if positional.len() < 2 || positional.len() > 4 {
        usage(args.first().map(String::as_str).unwrap_or("mkfs.mfs"));
    }
    let image_path = positional[0].clone();
    let blocks: u64 = positional[1]
        .parse()
        .unwrap_or_else(|_| fail("blocks is not a number"));
    let inodes_arg: Option<u64> = positional
        .get(2)
        .map(|s| s.parse().unwrap_or_else(|_| fail("inodes is not a number")));
    let block_size: u64 = positional
        .get(3)
        .map(|s| s.parse().unwrap_or_else(|_| fail("block-size is not a number")))
        .unwrap_or(mkfs::DEFAULT_BLOCK_SIZE);

    // With a prototype: the header's inode count wins (zero = use the default
    // ladder); a header block count above the argument is refused.
    let mut inodes = inodes_arg;
    let mut host = StdHost { last_path: RefCell::new(String::new()) };
    let proto_text = proto_path.map(|path| {
        support::read_file(&path)
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
            .unwrap_or_else(|_| fail(&format!("Can't open {path}")))
    });
    if let Some(text) = &proto_text {
        let header = mkfs::proto_header(text)
            .unwrap_or_else(|error| explain(error, &host));
        if header.blocks > blocks {
            explain(
                MkfsError::ProtoTooLarge { given: blocks, needed: header.blocks },
                &host,
            );
        }
        inodes = if header.inodes == 0 { None } else { Some(header.inodes) };
    }

    let plan = match mkfs::plan_layout(blocks, inodes, block_size) {
        Ok(plan) => plan,
        Err(error) => explain(error, &host),
    };
    let now = (support::epoch_micros().unwrap_or(0) / 1_000_000) as i32;
    let image = match &proto_text {
        Some(text) => mkfs::build_image_seeded(&plan, text, &mut host, now),
        None => mkfs::build_image(&plan, now),
    }
    .unwrap_or_else(|error| explain(error, &host));
    if support::write_file(&image_path, &image).is_err() {
        // The target-side failure reason is flattened at the seam (minix_sys
        // Errno does not cross it), and the host io::Error {e} detail goes
        // with it — failure visibility is unchanged.
        fail(&format!("cannot write {image_path}"));
    }
    support::emit(
        format!(
            "{}: {} blocks, {} inodes, block size {}\n",
            image_path, plan.blocks, plan.inodes, plan.block_size
        )
        .as_bytes(),
    );
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
