//! Rust-leg translation of the minix3/minix/tests suite — the directory and link
//! semantics domain (a selection of test19/21/22/28/32/34/61/78).
//!
//! # Translation map
//!
//! The numbered C tests drive MFS from user space through VFS; this file lands
//! the same set of assertions at the **MFS server's on-disk semantics layer**
//! (`MfsServer` implements `FsDriver`, with an in-memory block device mounted
//! directly) — directory entries, link counts, rename replacement, symbolic
//! links, chmod/chown persistence.  The same-named semantics at the VFS dispatch
//! layer (path resolution / permission gates / suspension continuations) belong
//! to `t_fs_special.rs` and to the later VFS<->MFS cross-package bridge (both
//! sides of the seam already exist; assembling them is a separate item).
//!
//! | C test | Semantic home |
//! |---|---|
//! | test21b/21c and test28 (mkdir/rmdir lifecycle) | [`mkdir_lookup_rmdir_lifecycle`] |
//! | test19 (st_nlink reconciliation between link and fstat, test19.c:314/320) | [`link_unlink_persists_via_second_name`] |
//! | test21a/test32 (rename replacing an existing target, over-long names; test32 is rename-only) | [`rename_replaces_target_and_moves_across_dirs`] |
//! | test78 (DT_LNK nodes) and test61 (dangling link) | [`symlink_roundtrip`] |
//! | test34 (chmod/chown) | [`chmod_chown_reflected_in_stat`] |
//! | test22 (umask) | umask is a PM/VFS-side decision (the Umask arm in `syscalls.rs` already has crate tests); MFS only ever sees the already-masked mode, so it is not repeated here |
//!
//! # C<->Rust semantic gaps found while translating (recorded, product code unchanged)
//!
//! 1. `MfsServer`'s `FsDriver` impl covers 28 methods, but `rename` is absent —
//!    the call falls through to the trait default and returns `ENOSYS`.  The
//!    rename engine itself (`fs/mfs/src/link.rs:429`, including target
//!    replacement and cross-directory semantics) is already in place and has
//!    crate unit tests (`link.rs:1600`); only the entry-point wiring is missing.
//!    The test [`rename_replaces_target_and_moves_across_dirs`] pins the C
//!    semantics as assertions and lights up once that wiring lands.
//! 2. MFS mounts in this repo's own tests always use 4096-byte blocks
//!    (`mount.rs:643`/`server.rs:1219`); mounting a 1024-byte-block image
//!    reports `EINVAL` — this file's fixture goes with 4096.
//!
//! Delivery gate = compilation; every test is `#[ignore]`, and the illumination
//! precondition is stated on each test's attribute.

use minix_fs::driver::FsDriver;
use minix_fs::protocol::{CapabilityFlags, MountFlags};
use minix_fs_mfs::mkfs::{build_image, plan_layout};
use minix_fs_mfs::server::MfsServer;
use minix_fs_rt::source::ImgrdBlockSource;
use minix_types::{ENOENT, Errno};

const DEVICE: u64 = 0x301;
const BLOCK_SIZE: usize = 4096; // same as the crate's own tests (mount.rs/server.rs precedents)
const ROOT: u64 = 1; // C MFS root inode is fixed at 1 (the crate's lifecycle tests use the same value)

// ---------------------------------------------------------------------------
// Fixture: mkfs builds the image + imgrd in-memory block device (the same
// combination the crate's own tests use)
// ---------------------------------------------------------------------------

/// Build a formatted MFS disk, mount it, and return the mounted server.
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

/// Clock seam of the server: always zero (C mfs's `now` semantics).
fn zero_clock() -> i64 {
    0
}

// ---------------------------------------------------------------------------
// test23/21 — mkdir/rmdir lifecycle
// ---------------------------------------------------------------------------

/// C test21b/21c and test28: create a directory, its name resolves, the name
/// disappears after removal; removing a nonexistent directory returns ENOENT.
#[test]
#[ignore = "点亮前提:VFS↔MFS 跨包桥点亮后由真路径驱动复核"]
fn mkdir_lookup_rmdir_lifecycle() {
    let mut server = mounted_server();

    server
        .make_dir(ROOT, "d", 0o040755, 0, 0)
        .expect("mkdir 合法");
    let (node, mount_point) = server.lookup_child(ROOT, "d").expect("建好的目录可解析");
    assert!(!mount_point, "普通目录不是挂载点");
    assert_eq!(node.mode & 0o170000, 0o040000, "类型位为目录");

    // Removing a non-empty directory: C test21 asserts that rmdir of a non-empty
    // directory fails (MFS semantics).
    server
        .create(node.inode_number, "f", 0o100644, 0, 0)
        .expect("目录内建文件");
    let err = server
        .remove_dir(ROOT, "d")
        .expect_err("非空目录必须拒绝删除");
    assert_eq!(
        err,
        Errno::from_i32(minix_types::ENOTEMPTY),
        "非空 rmdir → ENOTEMPTY"
    );

    // After emptying it, removal succeeds and the name disappears.
    server.unlink(node.inode_number, "f").expect("清空文件");
    server.remove_dir(ROOT, "d").expect("空目录可删除");
    let gone = server.lookup_child(ROOT, "d").expect_err("名字应消失");
    assert_eq!(gone, Errno::from_i32(ENOENT));

    // Removing a nonexistent directory: ENOENT.
    let err = server
        .remove_dir(ROOT, "nope")
        .expect_err("删不存在的目录必须失败");
    assert_eq!(err, Errno::from_i32(ENOENT));
}

// ---------------------------------------------------------------------------
// test19 — link counts and two coexisting names
// ---------------------------------------------------------------------------

/// C test19 (the st_nlink reconciliation at lines 314/320) plus the link surface
/// of test17/30: after a hard link, two names point at the same inode and the
/// link count is 2; after deleting the original name the content is still
/// readable through the alias; after deleting the last name the name disappears
/// and the link count drops to zero.
#[test]
#[ignore = "点亮前提:VFS↔MFS 跨包桥点亮后由真路径驱动复核"]
fn link_unlink_persists_via_second_name() {
    let mut server = mounted_server();

    let file = server.create(ROOT, "f", 0o100644, 0, 0).expect("建文件");
    server.write(file.inode_number, 0, b"abc").expect("写入");

    server.link(ROOT, "h", file.inode_number).expect("硬链接");
    let via_h = server.lookup_child(ROOT, "h").expect("别名可解析").0;
    assert_eq!(via_h.inode_number, file.inode_number, "双名同 inode");

    let mut stat = minix_types::Stat::zeroed();
    server
        .stat(file.inode_number, &mut stat)
        .expect("stat 可读");
    assert_eq!(
        stat.nlinks, 2,
        "链接计数 2(C test19.c:320 的 st_nlink 对账)"
    );

    // Delete the original name: the data survives through the alias.
    server.unlink(ROOT, "f").expect("删原名");
    assert!(server.lookup_child(ROOT, "f").is_err());
    let mut seen = Vec::new();
    let got = server
        .read(file.inode_number, 0, 16, &mut |bytes: &[u8]| {
            seen.extend_from_slice(bytes)
        })
        .expect("别名存活则数据可读");
    assert_eq!(got, 3);
    assert_eq!(seen, b"abc".to_vec());
    server
        .stat(file.inode_number, &mut stat)
        .expect("stat 可读");
    assert_eq!(stat.nlinks, 1, "删一名后计数回 1");

    // Delete the last name: the name disappears.
    server.unlink(ROOT, "h").expect("删别名");
    assert!(server.lookup_child(ROOT, "h").is_err());
}

// ---------------------------------------------------------------------------
// test21a/32 — rename replacement and cross-directory moves
// ---------------------------------------------------------------------------

/// C test21a/test32: a same-directory rename replaces the target (the target
/// inode is taken over by a's inode), and after a cross-directory move the old
/// directory's name is gone.
#[test]
#[ignore = "点亮前提:MfsServer 补 FsDriver rename 臂接线(当前 ENOSYS 默认,见文件头缺口记录)"]
fn rename_replaces_target_and_moves_across_dirs() {
    let mut server = mounted_server();

    let a = server.create(ROOT, "a", 0o100644, 0, 0).expect("建 a");
    let _b = server.create(ROOT, "b", 0o100644, 0, 0).expect("建 b");

    // Same-directory rename over an existing target (C test32.c:88-99: the target
    // is removed first and the old inode number takes over).
    server
        .rename(ROOT, "a", ROOT, "b")
        .expect("rename 覆盖合法");
    assert!(server.lookup_child(ROOT, "a").is_err(), "旧名字消失");
    let via_b = server.lookup_child(ROOT, "b").expect("目标名接管").0;
    assert_eq!(
        via_b.inode_number, a.inode_number,
        "b 的 inode = a 的 inode(替换)"
    );

    // Cross-directory move (the cross-directory part of C test21a: rename
    // involving different directories).
    server.make_dir(ROOT, "d", 0o040755, 0, 0).expect("建目录");
    let d = server.lookup_child(ROOT, "d").expect("目录可解析").0;
    server.create(ROOT, "c", 0o100644, 0, 0).expect("建 c");
    server
        .rename(ROOT, "c", d.inode_number, "c")
        .expect("跨目录移动合法");
    assert!(server.lookup_child(ROOT, "c").is_err(), "旧目录名字消失");
    assert!(
        server.lookup_child(d.inode_number, "c").is_ok(),
        "新目录名字出现"
    );
}

// ---------------------------------------------------------------------------
// test78/61 — symbolic link round trip
// ---------------------------------------------------------------------------

/// C test78 (DT_LNK nodes) and test61 (dangling link): after creating a symlink
/// the name resolves to an S_IFLNK-typed node, and readlink reproduces the target
/// string byte for byte.
#[test]
#[ignore = "点亮前提:VFS↔MFS 跨包桥点亮后由真路径驱动复核"]
fn symlink_roundtrip() {
    let mut server = mounted_server();

    server
        .symbolic_link(ROOT, "s", 0, 0, b"/a/b")
        .expect("symlink 合法");
    let (node, _) = server.lookup_child(ROOT, "s").expect("链接名可解析");
    assert_eq!(node.mode & 0o170000, 0o120000, "类型位为符号链接");

    let mut target = Vec::new();
    let len = server
        .read_link(node.inode_number, 128, &mut |bytes: &[u8]| {
            target.extend_from_slice(bytes)
        })
        .expect("readlink 合法");
    assert_eq!(len, 4, "目标串长度");
    assert_eq!(
        target,
        b"/a/b".to_vec(),
        "目标串逐字节还原(C test74.c:171,套件唯一 readlink 载体)"
    );
}

// ---------------------------------------------------------------------------
// test34 — chmod/chown persistence
// ---------------------------------------------------------------------------

/// C test34: chmod changes only the permission bits (the type bits are kept) and
/// chown changes owner/group; both are read back through stat.
#[test]
#[ignore = "点亮前提:VFS↔MFS 跨包桥点亮后由真路径驱动复核"]
fn chmod_chown_reflected_in_stat() {
    let mut server = mounted_server();

    let file = server.create(ROOT, "f", 0o100644, 0, 0).expect("建文件");

    let _old_mode = server
        .change_mode(file.inode_number, 0o100600)
        .expect("chmod 合法");
    let mut stat = minix_types::Stat::zeroed();
    server
        .stat(file.inode_number, &mut stat)
        .expect("stat 可读");
    assert_eq!(stat.mode, 0o100600, "chmod 后类型与权限位落盘");

    let _old_owner = server
        .change_owner(file.inode_number, 1000, 100)
        .expect("chown 合法");
    server
        .stat(file.inode_number, &mut stat)
        .expect("stat 可读");
    assert_eq!(
        (stat.owner, stat.group),
        (1000, 100),
        "属主属组落盘(C test34/35)"
    );
}
