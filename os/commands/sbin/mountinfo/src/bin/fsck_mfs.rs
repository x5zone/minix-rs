//! `fsck.mfs` 薄壳：对镜像文件跑只读的一致性检查与报告。
//!
//! C 对应物：`minix3/minix/commands/fsck.mfs/fsck.c` 的 `main`
//! （1616-1668）与 `chkdev` 的装配序。决定半在 `minix_fs_mfs::fsck`
//! （超块校验、走查、位图对照、计数对账都在格式库）；本程序只做四件
//! 事——旗标解析、镜像读入、报告打印、退出码。
//!
//! 旗标面（只读决定半支持）：`-l` 走查列表、`-s` 超块列表、`-i N...`
//! inode 观察单、`-z N...` 区块观察单、`-f` 空操作（C 同）。修复类
//! 旗标（`-r`/`-y`/`-a`/`-p`/`-d`）与 `-c`（指定 inode 列表，C `lsi`
//! 的问句面）属修复批——现在诚实拒绝，不做假成功。
//!
//! 退出码照 C `sbin/fsck/exitvalues.h`：检查完成 0（发现不一致不改
//! 退出码，C `main` 恒返 0；非交互消费方读报告文本）、用法 1、检查
//! 致命 8（消息 + `fatal` 尾注）。多镜像逐个检查，观察单每镜像后
//! 重置（C `chkdev` 后清列表同款）。

use minix_fs_mfs::fsck::{self, Fsck, FsckOptions};

fn usage(program: &str) -> ! {
    eprintln!("usage: {program} [-lfs] [-i ino]... [-z zone]... image...");
    std::process::exit(fsck::EXIT_USAGE);
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let program = args.first().map(String::as_str).unwrap_or("fsck.mfs");
    let mut listing = false;
    let mut want_super = false;
    let mut watch_inodes: Vec<u64> = Vec::new();
    let mut watch_zones: Vec<u64> = Vec::new();
    let mut images: Vec<String> = Vec::new();
    let mut bad_flag = false;

    // C 的旗标形状：单破折线单字母（arg[2] == 0）；-i/-z 吃掉其后
    // 连续的数字项，遇非数字交还外层（getlist 同形）。
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
                    eprintln!(
                        "{program}: {arg} needs the repair face (deferred); \
                         only the read-only decision half is built"
                    );
                    std::process::exit(fsck::EXIT_USAGE);
                }
                _ => {
                    eprintln!("{program}: unknown flag '{arg}'");
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
        // C devopen 失败走 fatal（fsck.c:384-391）。
        let image = match std::fs::read(image_path) {
            Ok(bytes) => bytes,
            Err(error) => {
                println!("fsck.mfs: cannot read {image_path}: {error}");
                println!("couldn't open device to fsck\nfatal");
                std::process::exit(fsck::EXIT_CHECK_FAILED);
            }
        };
        let mut checker = match Fsck::new(&image) {
            Ok(checker) => checker,
            Err(fatal) => {
                println!("{}\nfatal", fatal.message);
                std::process::exit(fsck::EXIT_CHECK_FAILED);
            }
        };
        if want_super {
            // C 的 lsuper 在读超块后立即打（fsck.c:577）；这里在静态
            // 校验后打，字段面相同。
            print!("{}", checker.list_super());
        }
        let options = FsckOptions {
            listing,
            watch_inodes: watch_inodes.clone(),
            watch_zones: watch_zones.clone(),
        };
        match checker.run(&options) {
            Ok(_summary) => {
                for message in checker.messages() {
                    println!("{message}");
                }
            }
            Err(fatal) => {
                for message in checker.messages() {
                    println!("{message}");
                }
                println!("{}\nfatal", fatal.message);
                std::process::exit(fsck::EXIT_CHECK_FAILED);
            }
        }
        // 多镜像：观察单每镜像后重置（fsck.c:1660-1662）。
        watch_inodes.clear();
        watch_zones.clear();
    }
}
