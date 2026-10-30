//! Rust-leg translation of minix3/minix/tests — special files and directory boundary domain (a selection from test58/78).
//!
//! # Translation map
//!
//! Same layer as `t_fs_dir_ops.rs` (MFS server real-disk semantics); the VFS-side
//! path-resolution semantics (the main body of test58's "calls fail after the
//! current directory is deleted") are registered as VFS↔MFS cross-package bridge
//! entries.
//!
//! | C test | Semantic home |
//! |---|---|
//! | test78 (chr/blk nodes created by mknod and enumerated by readdir) | [`mknod_device_file_records_type_and_rdev`](the numeric rdev assertion takes the st_rdev semantics of `mfs stadir.c:65`; the numbered suite has no numeric assertion) |
//! | test78 (the S_IFLNK bit read via lstat on a DT_LNK entry) | [`lstat_on_symlink_reports_link_not_target`](link size = target string byte count is POSIX semantics) |
//! | test58 (name resolution inside a deleted directory) | [`removed_directory_names_stop_resolving`] |
//! | (test61's mknod EEXIST face and test73, a VM cache black-box test, are unrelated to umask and are not in this file) | — |
//!
//! The delivery gate is compilation; every test is `#[ignore]`d, and the precondition for lighting each one up is in its attribute.

use minix_fs::driver::FsDriver;
use minix_fs::protocol::{CapabilityFlags, MountFlags};
use minix_fs_mfs::mkfs::{build_image, plan_layout};
use minix_fs_mfs::server::MfsServer;
use minix_fs_rt::source::ImgrdBlockSource;
use minix_types::{ENOENT, Errno};

const DEVICE: u64 = 0x301;
const BLOCK_SIZE: usize = 4096; // matches the crate's own tests (the mount.rs/server.rs precedent)
const ROOT: u64 = 1;

// ---------------------------------------------------------------------------
// Fixtures (same shape as t_fs_dir_ops.rs)
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
// test78 — mknod device files
// ---------------------------------------------------------------------------

/// C test78: six node kinds (including chr/blk/FIFO created by mknod) are
/// created and enumerated; the rdev value read back through stat is the st_rdev
/// semantics of `mfs stadir.c:65` (the numbered suite has no numeric assertion).
#[test]
#[ignore = "点亮前提:VFS↔MFS 跨包桥点亮后由真路径驱动复核"]
fn mknod_device_file_records_type_and_rdev() {
    let mut server = mounted_server();

    // The mknod (chr/blk) part of C test78; character device semantics are pinned here.
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

    // Same shape for block devices (the DT_BLK part of C test78.c:81).
    server
        .make_node(ROOT, "disk0", 0o060600, 0, 0, 0x30100)
        .expect("块设备 mknod 合法");
    let (blk, _) = server.lookup_child(ROOT, "disk0").expect("可解析");
    assert_eq!(blk.mode & 0o170000, 0o060000, "类型位为块设备");
}

// ---------------------------------------------------------------------------
// test78 — lstat reads the link itself
// ---------------------------------------------------------------------------

/// C test78 (the CR macro lstats a DT_LNK entry and requires the S_IFLNK bit):
/// lstat on a symlink returns the link itself and does not resolve the target;
/// link size = target string byte count is POSIX semantics (the numbered suite
/// asserts no size). At the FS layer stat is lstat semantics (only VFS follows).
#[test]
#[ignore = "点亮前提:VFS↔MFS 跨包桥点亮后由真路径驱动复核"]
fn lstat_on_symlink_reports_link_not_target() {
    let mut server = mounted_server();

    // A link may be created even when the target itself does not exist (C test43's dangling-link semantics).
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
// test58 — name resolution in a deleted directory
// ---------------------------------------------------------------------------

/// The FS half of C test58: once a directory is deleted its names no longer
/// resolve, and a name lookup on the directory inode returns ENOENT (the full
/// "calls fail after cwd is deleted" semantics needs the VFS path-resolution
/// layer and is registered as a cross-package bridge entry).
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

    // The directory name is gone; name lookup by the old inode number fails too
    // (C test58: under the old cwd every name resolution fails).
    assert!(server.lookup_child(ROOT, "d").is_err());
    let gone = server
        .lookup_child(d.inode_number, "f")
        .expect_err("被删目录内名字必须失败");
    assert_eq!(gone, Errno::from_i32(ENOENT));
}
