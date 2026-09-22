//! minix3/minix/tests Rust 腿翻译 —— 特殊文件与目录边界域(test43/58/61/78)。
//!
//! # 翻译映射
//!
//! 与 `t_fs_dir_ops.rs` 同层(MFS 服务器真盘语义);VFS 侧路径解析语义
//! (test58 的"当前目录被删后调用失败"主体)挂 VFS↔MFS 跨包桥条目。
//!
//! | C 测试 | 语义归宿 |
//! |---|---|
//! | test61/78(mknod 设备文件) | [`mknod_device_file_records_type_and_rdev`] |
//! | test43/78(lstat 读链接本体而非目标) | [`lstat_on_symlink_reports_link_not_target`] |
//! | test58(被删目录内名字消解) | [`removed_directory_names_stop_resolving`] |
//! | test73(umask 与 root) | umask 归 PM/VFS 决策层(crate 已测),不在此重复 |
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
// test61/78 —— mknod 设备文件
// ---------------------------------------------------------------------------

/// C test61/78:mknod 建字符设备文件后,名字解析到 S_IFCHR 节点,
/// 设备号随 stat 读回(C test61 断言 st_rdev)。
#[test]
#[ignore = "点亮前提:VFS↔MFS 跨包桥点亮后由真路径驱动复核"]
fn mknod_device_file_records_type_and_rdev() {
    let mut server = mounted_server();

    // C test61:mkfifo/mknod 组合;这里钉字符设备语义。
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
    assert_eq!(stat.special, 0x2c01, "stat 设备号一致(C test61 的 st_rdev)");

    // 块设备同型(C test61 的第二段)。
    server
        .make_node(ROOT, "disk0", 0o060600, 0, 0, 0x30100)
        .expect("块设备 mknod 合法");
    let (blk, _) = server.lookup_child(ROOT, "disk0").expect("可解析");
    assert_eq!(blk.mode & 0o170000, 0o060000, "类型位为块设备");
}

// ---------------------------------------------------------------------------
// test43/78 —— lstat 读链接本体
// ---------------------------------------------------------------------------

/// C test43:lstat 对符号链接返回链接自身(类型 S_IFLNK、大小 = 目标串
/// 字节数),不解析目标。FS 层 stat 即 lstat 语义(VFS 才做跟随)。
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
