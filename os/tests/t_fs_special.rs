//! minix3/minix/tests Rust 腿翻译 —— 特殊文件与目录边界域(test58/78 选集)。
//!
//! # 翻译映射
//!
//! 与 `t_fs_dir_ops.rs` 同层(MFS 服务器真盘语义);VFS 侧路径解析语义
//! (test58 的"当前目录被删后调用失败"主体)挂 VFS↔MFS 跨包桥条目。
//!
//! | C 测试 | 语义归宿 |
//! |---|---|
//! | test78(chr/blk 节点经 mknod 建立并被 readdir 枚举) | [`mknod_device_file_records_type_and_rdev`](rdev 数值断言取 `mfs stadir.c:65` 的 st_rdev 语义,编号套件无数值断言) |
//! | test78(DT_LNK 项经 lstat 取 S_IFLNK 位) | [`lstat_on_symlink_reports_link_not_target`](链接大小 = 目标串字节数为 POSIX 语义) |
//! | test58(被删目录内名字消解) | [`removed_directory_names_stop_resolving`] |
//! | (test61 的 mknod EEXIST 面、test73 为 VM 缓存黑盒测试,与 umask 无关,均不在本文件) | — |
//!
//! 交付门 = 编译;全部测试 `#[ignore]`,点亮前提见各测试属性。

use minix_fs::driver::FsDriver;
use minix_fs::protocol::{CapabilityFlags, MountFlags};
use minix_fs_mfs::mkfs::{build_image, plan_layout};
use minix_fs_mfs::server::MfsServer;
use minix_fs_rt::source::ImgrdBlockSource;
use minix_types::{ENOENT, Errno};

const DEVICE: u64 = 0x301;
const BLOCK_SIZE: usize = 4096; // 与 crate 自测一致(mount.rs/server.rs 判例)
const ROOT: u64 = 1;

// ---------------------------------------------------------------------------
// 夹具(与 t_fs_dir_ops.rs 同型)
// ---------------------------------------------------------------------------

fn mounted_server() -> MfsServer<ImgrdBlockSource> {
    let plan = plan_layout(64, Some(64), BLOCK_SIZE as u64).expect("布局合法");
    let image = build_image(&plan, 1_000).expect("mkfs 成功");
    let source = ImgrdBlockSource::new(image, BLOCK_SIZE).expect("imgrd 源合法");
    let mut server = MfsServer::with_pool(source, 8, zero_clock);
    let mut capabilities = CapabilityFlags::EMPTY;
    let root = server
        .mount(DEVICE, MountFlags::EMPTY, &mut capabilities)
        .expect("挂载成功");
    assert_eq!(root.inode_number, ROOT, "根 inode 固定 1");
    server
}

fn zero_clock() -> i64 {
    0
}

// ---------------------------------------------------------------------------
// test78 —— mknod 设备文件
// ---------------------------------------------------------------------------

/// C test78:六类节点(含 mknod 建 chr/blk/FIFO)建立并被枚举;rdev 数值随
/// stat 读回是 `mfs stadir.c:65` 的 st_rdev 语义(编号套件无数值断言)。
#[test]
#[ignore = "点亮前提:VFS↔MFS 跨包桥点亮后由真路径驱动复核"]
fn mknod_device_file_records_type_and_rdev() {
    let mut server = mounted_server();

    // C test78 的 mknod(chr/blk)段;这里钉字符设备语义。
    server
        .make_node(ROOT, "tty0", 0o020600, 0, 0, 0x2c01)
        .expect("mknod 合法");
    let (node, _) = server.lookup_child(ROOT, "tty0").expect("设备节点可解析");
    assert_eq!(node.mode & 0o170000, 0o020000, "类型位为字符设备");

    let mut stat = minix_types::Stat::zeroed();
    server
        .stat(node.inode_number, &mut stat)
        .expect("stat 可读");
    assert_eq!(stat.mode & 0o170000, 0o020000, "stat 类型位一致");
    assert_eq!(
        stat.special, 0x2c01,
        "stat 设备号一致(mfs stadir.c:65 的 st_rdev 语义)"
    );

    // 块设备同型(C test78.c:81 的 DT_BLK 段)。
    server
        .make_node(ROOT, "disk0", 0o060600, 0, 0, 0x30100)
        .expect("块设备 mknod 合法");
    let (blk, _) = server.lookup_child(ROOT, "disk0").expect("可解析");
    assert_eq!(blk.mode & 0o170000, 0o060000, "类型位为块设备");
}

// ---------------------------------------------------------------------------
// test78 —— lstat 读链接本体
// ---------------------------------------------------------------------------

/// C test78(CR 宏对 DT_LNK 项 lstat 并要求 S_IFLNK 位):lstat 对符号
/// 链接返回链接自身,不解析目标;链接大小 = 目标串字节数是 POSIX 语义
/// (编号套件无大小断言)。FS 层 stat 即 lstat 语义(VFS 才做跟随)。
#[test]
#[ignore = "点亮前提:VFS↔MFS 跨包桥点亮后由真路径驱动复核"]
fn lstat_on_symlink_reports_link_not_target() {
    let mut server = mounted_server();

    // 目标本体不存在也允许建链接(C test43 的悬空链接语义)。
    server
        .symbolic_link(ROOT, "s", 0, 0, b"/no/such")
        .expect("悬空链接可建");
    let (node, _) = server.lookup_child(ROOT, "s").expect("链接名可解析");

    let mut stat = minix_types::Stat::zeroed();
    server
        .stat(node.inode_number, &mut stat)
        .expect("stat 可读");
    assert_eq!(stat.mode & 0o170000, 0o120000, "stat 读到链接本体类型");
    assert_eq!(stat.size, 8, "链接大小 = 目标串字节数(/no/such)");
    assert_eq!(stat.nlinks, 1, "链接本体计数 1");
}

// ---------------------------------------------------------------------------
// test58 —— 被删目录的名字消解
// ---------------------------------------------------------------------------

/// C test58 的 FS 半:目录被删后,其中的名字不可再消解,目录 inode 上
/// 的查名返回 ENOENT("cwd 被删后调用失败"的完整语义需 VFS 路径解析层,
/// 挂跨包桥条目)。
#[test]
#[ignore = "点亮前提:VFS↔MFS 跨包桥点亮后由真路径驱动复核"]
fn removed_directory_names_stop_resolving() {
    let mut server = mounted_server();

    server.make_dir(ROOT, "d", 0o040755, 0, 0).expect("建目录");
    let (d, _) = server.lookup_child(ROOT, "d").expect("可解析");
    server
        .create(d.inode_number, "f", 0o100644, 0, 0)
        .expect("目录内建文件");
    server.unlink(d.inode_number, "f").expect("清空");
    server.remove_dir(ROOT, "d").expect("删目录");

    // 目录名消失;原 inode 编号上的查名也失败(C test58:旧 cwd 下
    // 一切名字解析失败)。
    assert!(server.lookup_child(ROOT, "d").is_err());
    let gone = server
        .lookup_child(d.inode_number, "f")
        .expect_err("被删目录内名字必须失败");
    assert_eq!(gone, Errno::from_i32(ENOENT));
}
