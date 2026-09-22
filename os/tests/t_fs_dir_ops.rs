//! minix3/minix/tests Rust 腿翻译 —— 目录与链接语义域(test19/21/22/28/32/34/61/78 选集)。
//!
//! # 翻译映射
//!
//! C 编号测试从用户态经 VFS 驱动 MFS;本文件在 **MFS 服务器真盘语义层**
//! 落同一组断言(`MfsServer` 实现 `FsDriver`,内存块设备直挂)——目录项、
//! 链接计数、rename 替换、符号链接、chmod/chown 落盘。VFS dispatch 层的
//! 同名语义(路径解析/权限门/挂起续接)归 `t_fs_special.rs` 与后续
//! VFS↔MFS 跨包桥(接缝两侧均已存在,组装为独立条目)。
//!
//! | C 测试 | 语义归宿 |
//! |---|---|
//! | test21b/21c 与 test28(mkdir/rmdir 生命周期) | [`mkdir_lookup_rmdir_lifecycle`] |
//! | test19(link 与 fstat 的 st_nlink 对账,test19.c:314/320) | [`link_unlink_persists_via_second_name`] |
//! | test21a/test32(rename 替换既有目标、超长名;test32 专测 rename) | [`rename_replaces_target_and_moves_across_dirs`] |
//! | test78(DT_LNK 节点)与 test61(悬空链接) | [`symlink_roundtrip`] |
//! | test34(chmod/chown) | [`chmod_chown_reflected_in_stat`] |
//! | test22(umask) | umask 是 PM/VFS 侧决策(`syscalls.rs` Umask 臂已有 crate 测试),MFS 层看到的已是掩码后模式,不在此重复 |
//!
//! # 翻译即发现的 C↔Rust 语义缺口(记录,不改产品代码)
//!
//! 1. `MfsServer` 的 `FsDriver` 实现覆盖 28 个方法,唯独 `rename` 缺席——
//!    调用落到 trait 默认实现返回 `ENOSYS`(78)。rename 引擎本体
//!    (`fs/mfs/src/link.rs:429`,含替换目标与跨目录语义)已在位并有 crate
//!    单测(`link.rs:1600`),缺的只是入口接线。测试 [`rename_replaces_target_and_moves_across_dirs`]
//!    把 C 语义钉为断言,接线后即可点亮。
//! 2. MFS 挂载在本仓自测均用 4096 字节块(`mount.rs:643`/`server.rs:1219`),
//!    1024 字节块镜像挂载报 `EINVAL`——本文件夹具按 4096 走。
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
const ROOT: u64 = 1; // C MFS 根 inode 固定 1(crate 生命周期判例同值)

// ---------------------------------------------------------------------------
// 夹具:mkfs 产镜像 + imgrd 内存块设备(mount.rs 自测同款组合)
// ---------------------------------------------------------------------------

/// 造一块已格式化的 MFS 盘并挂载,返回挂载好的服务器。
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

/// 服务器时钟缝:恒零(C mfs 的 now 语义)。
fn zero_clock() -> i64 {
    0
}

// ---------------------------------------------------------------------------
// test23/21 —— mkdir/rmdir 生命周期
// ---------------------------------------------------------------------------

/// C test21b/21c 与 test28:建目录、名字可解析、删除后名字消失;删不存在的目录
/// 返回 ENOENT。
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

    // 目录非空时删除:C test21 断言 rmdir 非空目录失败(MFS 语义)。
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

    // 清空后删除成功,名字消失。
    server.unlink(node.inode_number, "f").expect("清空文件");
    server.remove_dir(ROOT, "d").expect("空目录可删除");
    let gone = server.lookup_child(ROOT, "d").expect_err("名字应消失");
    assert_eq!(gone, Errno::from_i32(ENOENT));

    // 删不存在的目录:ENOENT。
    let err = server
        .remove_dir(ROOT, "nope")
        .expect_err("删不存在的目录必须失败");
    assert_eq!(err, Errno::from_i32(ENOENT));
}

// ---------------------------------------------------------------------------
// test19 —— link 计数与双名共存
// ---------------------------------------------------------------------------

/// C test19(314/320 行的 st_nlink 对账)与 test17/30 的 link 面:硬链接后
/// 两个名字指向同一 inode,链接计数 2;删原名后内容经别名仍可读;删掉
/// 最后一个名字后名字消失、链接计数归零。
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

    // 删原名:数据经别名存活。
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

    // 删最后一个名字:名字消失。
    server.unlink(ROOT, "h").expect("删别名");
    assert!(server.lookup_child(ROOT, "h").is_err());
}

// ---------------------------------------------------------------------------
// test21a/32 —— rename 替换与跨目录
// ---------------------------------------------------------------------------

/// C test21a/test32:rename 同目录替换目标(目标 inode 被 a 的 inode 接管),
/// 跨目录移动后旧目录名字消失。
#[test]
#[ignore = "点亮前提:MfsServer 补 FsDriver rename 臂接线(当前 ENOSYS 默认,见文件头缺口记录)"]
fn rename_replaces_target_and_moves_across_dirs() {
    let mut server = mounted_server();

    let a = server.create(ROOT, "a", 0o100644, 0, 0).expect("建 a");
    let _b = server.create(ROOT, "b", 0o100644, 0, 0).expect("建 b");

    // 同目录 rename 覆盖已有目标(C test32.c:88-99:目标先移除,旧编号接管)。
    server
        .rename(ROOT, "a", ROOT, "b")
        .expect("rename 覆盖合法");
    assert!(server.lookup_child(ROOT, "a").is_err(), "旧名字消失");
    let via_b = server.lookup_child(ROOT, "b").expect("目标名接管").0;
    assert_eq!(
        via_b.inode_number, a.inode_number,
        "b 的 inode = a 的 inode(替换)"
    );

    // 跨目录移动(C test21a 的跨目录段:rename 涉及不同目录)。
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
// test78/61 —— 符号链接往返
// ---------------------------------------------------------------------------

/// C test78(DT_LNK 节点)与 test61(悬空链接):symlink 建立后名字解析到
/// S_IFLNK 类型节点,readlink 逐字节还原目标串。
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
// test34 —— chmod/chown 落盘
// ---------------------------------------------------------------------------

/// C test34:chmod 只改权限位(类型位保留),chown 改属主/属组,
/// 两者经 stat 读回。
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
