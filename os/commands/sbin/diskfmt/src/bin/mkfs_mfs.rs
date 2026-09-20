//! `mkfs.mfs` 薄壳：在镜像文件上构建第三版 Minix 文件系统——空卷或按
//! 原型文件播种。
//!
//! C 对应物：`minix3/minix/usr.sbin/mkfs.mfs/mkfs.c` 的 `main`。决定半
//! 在 `minix_fs_mfs::mkfs`（布局算式、镜像构建、根目录装配、原型播种
//! 都在格式库里）；本程序只做四件事——解析参数、读原型、喂宿主文件、
//! 落盘。
//!
//! 参数形状（宿主面）：`mkfs.mfs <image> <blocks> [inodes] [block-size]
//! [-p proto]`。`-p` 给出时按 C 的 `simple == 0` 支路走：原型头第二行
//! 的 inode 数优先于位置参数（C `main` 直接覆盖），头声明的块数超过
//! 位置参数即拒（C mkfs.c:305-308）。条目里的普通文件经 `StdHost`
//! 从宿主读内容与修改时刻。C 的 getopt 面（`-b`/`-i`/`-l`/`-z`/`-T`）
//! 与真机块设备写入随 E-FSBDEV 一并对位。

use minix_fs_mfs::mkfs::{self, MkfsError, ProtoHost};
use std::cell::RefCell;

fn usage(program: &str) -> ! {
    eprintln!("usage: {program} <image> <blocks> [inodes] [block-size] [-p proto]");
    std::process::exit(2);
}

fn fail(message: &str) -> ! {
    eprintln!("mkfs.mfs: {message}");
    std::process::exit(1);
}

/// 宿主文件读取器的生产半：整文件读入，修改时刻取自元数据；失败的
/// 路径留在 `last_path` 里，错误报告时点名（C 的 "Can't open %s"）。
struct StdHost {
    last_path: RefCell<String>,
}

impl ProtoHost for StdHost {
    fn read_file(&mut self, path: &str, out: &mut [u8]) -> Result<mkfs::HostFile, MkfsError> {
        *self.last_path.borrow_mut() = path.to_string();
        let bytes = std::fs::read(path).map_err(|_| MkfsError::HostFile)?;
        if bytes.len() > out.len() {
            return Err(MkfsError::BufferTooSmall);
        }
        out[..bytes.len()].copy_from_slice(&bytes);
        let mtime = std::fs::metadata(path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i32)
            .unwrap_or(0);
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

fn main() {
    let args: Vec<String> = std::env::args().collect();
    // -p <proto> 可出现在任意位置；其余按位置收。
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

    // 原型给出时：头的 inode 数优先（零 = 用缺省阶梯），头块数超出参数即拒。
    let mut inodes = inodes_arg;
    let mut host = StdHost { last_path: RefCell::new(String::new()) };
    let proto_text = proto_path.map(|path| {
        std::fs::read_to_string(&path)
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
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i32)
        .unwrap_or(0);
    let image = match &proto_text {
        Some(text) => mkfs::build_image_seeded(&plan, text, &mut host, now),
        None => mkfs::build_image(&plan, now),
    }
    .unwrap_or_else(|error| explain(error, &host));
    if let Err(e) = std::fs::write(&image_path, &image) {
        fail(&format!("cannot write {}: {e}", image_path));
    }
    println!(
        "{}: {} blocks, {} inodes, block size {}",
        image_path, plan.blocks, plan.inodes, plan.block_size
    );
}
