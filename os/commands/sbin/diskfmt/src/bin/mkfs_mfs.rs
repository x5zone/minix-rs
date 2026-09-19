//! `mkfs.mfs` 薄壳：在镜像文件上构建一个空的第三版 Minix 文件系统。
//!
//! C 对应物：`minix3/minix/usr.sbin/mkfs.mfs/mkfs.c` 的 `main`。决定半
//! 在 `minix_fs_mfs::mkfs`（布局算式、镜像构建、根目录装配都在格式库
//! 里）；本程序只做三件事——解析参数、构建镜像、落盘。
//!
//! 参数形状（宿主面）：`mkfs.mfs <image> <blocks> [inodes] [block-size]`。
//! C 的 getopt 面（`-b`/`-i`/`-l`/`-z`/`-T`）与原型文件填充（`eat_dir`）
//! 未在本壳呈现——原型播种属后续批次，参数面随真机块设备接缝
//! （E-FSBDEV）一并对位。

use minix_fs_mfs::mkfs;

fn usage(program: &str) -> ! {
    eprintln!("usage: {program} <image> <blocks> [inodes] [block-size]");
    std::process::exit(2);
}

fn fail(message: &str) -> ! {
    eprintln!("mkfs.mfs: {message}");
    std::process::exit(1);
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 || args.len() > 5 {
        usage(args.first().map(String::as_str).unwrap_or("mkfs.mfs"));
    }
    let image_path = args[1].clone();
    let blocks: u64 = args[2].parse().unwrap_or_else(|_| fail("blocks is not a number"));
    let inodes: Option<u64> = args
        .get(3)
        .map(|s| {
            s.parse()
                .unwrap_or_else(|_| fail("inodes is not a number"))
        });
    let block_size: u64 = args
        .get(4)
        .map(|s| {
            s.parse()
                .unwrap_or_else(|_| fail("block-size is not a number"))
        })
        .unwrap_or(mkfs::DEFAULT_BLOCK_SIZE);

    let plan = match mkfs::plan_layout(blocks, inodes, block_size) {
        Ok(plan) => plan,
        Err(mkfs::MkfsError::TooSmall) => fail("Block count too small"),
        Err(mkfs::MkfsError::TooFewInodes) => fail("Inode count too small"),
        Err(mkfs::MkfsError::MapsTooLarge) => fail("bit maps too large"),
        Err(mkfs::MkfsError::BufferTooSmall) => fail("out of memory"),
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i32)
        .unwrap_or(0);
    let image = match mkfs::build_image(&plan, now) {
        Ok(image) => image,
        Err(_) => fail("out of memory"),
    };
    if let Err(e) = std::fs::write(&image_path, &image) {
        fail(&format!("cannot write {}: {e}", image_path));
    }
    println!(
        "{}: {} blocks, {} inodes, block size {}",
        image_path, plan.blocks, plan.inodes, plan.block_size
    );
}
