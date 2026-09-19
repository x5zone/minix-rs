//! `PtyfsDriver` — the `FsDriver` adapter over the ptyfs decision layer.
//!
//! C correspondence: the eight entries of `ptyfs_table`
//! (`minix3/minix/fs/ptyfs/ptyfs.c:411-421`) — mount, lookup, getdents,
//! stat, chown, chmod, statvfs, other. Every entry delegates to the pure
//! decision functions in [`crate`] and keeps no state beyond the node
//! table and the root record, exactly like the C globals.
//!
//! Deliberate gap (tracked, not faked): `ptyfs_other` (ptyfs.c:299-372)
//! handles the PTY service's control messages that create and delete
//! slave nodes. The driver trait's `other` hook currently receives only
//! an `is_notification` flag — the message body (which request, which
//! index, which metadata) never reaches the server, so this adapter
//! cannot serve the control face yet. Until the runtime passes the
//! envelope through, the node table stays empty and only the root
//! directory answers; mount, lookup, listing, status, and the ownership
//! and mode changes are all real.

use alloc::vec;

use minix_fs::dentry::{DentryEncoder, DirentType};
use minix_fs::driver::FsDriver;
use minix_fs::protocol::{CapabilityFlags, FileNode, MountFlags};
use minix_types::{EINVAL, Errno, Stat, StatVfs};

use crate::{
    change_mode, change_owner, file_stat, filesystem_stat, lookup,
    table::{NodeTable, StoredNode},
    ROOT_NUMBER,
};

/// Longest name the file system reports. C: `ptyfs_statvfs` writes
/// `NAME_MAX` (`minix3/sys/sys/syslimits.h:57`, value 511).
pub const NAME_MAX_REPORTED: usize = 511;

/// Size of the staging buffer for directory entries.
/// C: `GETDENTS_BUF` (`ptyfs.c:15`, value one thousand twenty-four).
const GETDENTS_BUFFER: usize = 1024;

/// The ptyfs server: node table plus the root record.
pub struct PtyfsDriver {
    /// Slave-node table (`node.c`'s fixed array behind a bitmap).
    pub table: NodeTable,
    /// Root directory record (`root_data`, ptyfs.c:17-22).
    pub root: StoredNode,
}

impl PtyfsDriver {
    /// Fresh server: an empty slave table and the C root defaults
    /// (`root_data`, ptyfs.c:17-22 — directory 0755, owner and group
    /// zero, no device).
    pub fn new() -> Self {
        Self {
            table: NodeTable::new(),
            root: StoredNode {
                device: 0,
                mode: 0o040755,
                uid: 0,
                gid: 0,
                created: 0,
            },
        }
    }

    /// Node details for one number as a `FileNode` (sizes are always
    /// zero: device nodes and the root directory carry no bytes).
    fn file_node(&self, number: u64) -> Result<FileNode, minix_types::Errno> {
        let (details, _links, _time) =
            file_stat(&self.table, &self.root, number).map_err(|e| e.to_errno())?;
        Ok(FileNode {
            inode_number: details.number,
            mode: details.mode,
            size: 0,
            owner: details.uid as u32,
            group: details.gid as u32,
            device: details.device,
        })
    }
}

impl Default for PtyfsDriver {
    fn default() -> Self {
        Self::new()
    }
}

/// Directory-entry type from a mode (`IFTODT`, ptyfs.c:186).
fn dirent_type_of(mode: u32) -> DirentType {
    match (mode & 0o170000) >> 12 {
        0o001 => DirentType::Fifo,
        0o002 => DirentType::Character,
        0o004 => DirentType::Directory,
        0o006 => DirentType::Block,
        0o010 => DirentType::Regular,
        0o012 => DirentType::Symlink,
        0o014 => DirentType::Socket,
        _ => DirentType::Unknown,
    }
}

/// Bridge the trait's copy callback into the dentry encoder's backend
/// (`fsdriver_data`'s copy-out direction; the listing never reads back).
struct OutBackend<'a> {
    out: &'a mut dyn FnMut(&[u8]),
}

impl minix_fs::data::DataBackend for OutBackend<'_> {
    fn copy_from(&self, _offset: usize, _out: &mut [u8]) -> Result<(), minix_types::Errno> {
        Ok(())
    }
    fn copy_to(&mut self, _offset: usize, data: &[u8]) -> Result<(), minix_types::Errno> {
        (self.out)(data);
        Ok(())
    }
    fn fill_zero(&mut self, _offset: usize, _length: usize) -> Result<(), minix_types::Errno> {
        Ok(())
    }
}

impl FsDriver for PtyfsDriver {
    /// C `ptyfs_mount` (ptyfs.c:27-50): refuse the root role, report the
    /// root directory, no capability flags.
    fn mount(
        &mut self,
        _device: u64,
        flags: MountFlags,
        capabilities: &mut CapabilityFlags,
    ) -> Result<FileNode, minix_types::Errno> {
        if flags.is_root() {
            return Err(Errno::from_i32(EINVAL));
        }
        *capabilities = CapabilityFlags(0);
        self.file_node(ROOT_NUMBER)
    }

    /// C `ptyfs_lookup` (ptyfs.c:107-147) via the decision function.
    fn lookup_child(
        &mut self,
        directory: u64,
        name: &str,
    ) -> Result<(FileNode, bool), minix_types::Errno> {
        let details =
            lookup(&self.table, &self.root, directory, name.as_bytes()).map_err(|e| e.to_errno())?;
        let node = self.file_node(details.number)?;
        Ok((node, false))
    }

    /// C `ptyfs_getdents` (ptyfs.c:152-199): walk positions in lockstep
    /// with the C loop — dot is position zero, dot-dot is one, slave
    /// index `i` is `i + 2`; unallocated indexes are skipped; the
    /// encoder's full-buffer signal stops the walk with the position
    /// already advanced past the entry that did not fit (C increments
    /// before its check too).
    fn get_dents(
        &mut self,
        inode: u64,
        position: &mut i64,
        capacity: usize,
        out: &mut dyn FnMut(&[u8]),
    ) -> Result<usize, minix_types::Errno> {
        if inode != ROOT_NUMBER {
            return Err(Errno::from_i32(EINVAL));
        }
        if *position < 0 {
            return Err(Errno::from_i32(EINVAL));
        }
        let mut pos = *position as u64;
        let mut staging = vec![0u8; GETDENTS_BUFFER];
        let mut backend = OutBackend { out };
        let mut encoder = DentryEncoder::new(
            minix_fs::data::DataChannel::Present {
                backend: &mut backend,
                size: capacity,
            },
            capacity,
            &mut staging[..],
        );
        let mut delivered = 0usize;
        loop {
            let current = pos;
            pos += 1;
            let add_result = if current < 2 {
                let name = if current == 0 { &b"."[..] } else { &b".."[..] };
                encoder.add(ROOT_NUMBER, name, DirentType::Directory)
            } else {
                let index = current - 2;
                if index >= self.table.upper_bound() {
                    break; // EOF (C ptyfs.c:180-181)
                }
                let Some(stored) = self.table.get(index as u32) else {
                    continue; // index not in use (C ptyfs.c:184-186)
                };
                let Ok(rendered) = crate::names::render(index as u32) else {
                    continue; // could not generate name (C ptyfs.c:187-189)
                };
                let number = crate::SLAVE_BASE_NUMBER + index;
                encoder.add(number, rendered.as_bytes(), dirent_type_of(stored.mode))
            };
            match add_result {
                Ok(0) => break, // result buffer full (C ptyfs.c:193-195)
                Ok(bytes) => delivered += bytes,
                Err(_) => return Err(Errno::from_i32(EINVAL)),
            }
        }
        // C `fsdriver_dentry_finish`（dentry.c:85-99）：把 staging 里的
        // 尾段刷给调用者，返回发出的总字节数。
        delivered += encoder.finish().map_err(|e| e)?;
        *position = pos as i64;
        Ok(delivered)
    }

    /// C `ptyfs_stat` (ptyfs.c:263-280): mode, owner, group, link count
    /// (two for directories, one otherwise), device number, and the
    /// creation time in all three (four) time fields.
    fn stat(&mut self, inode: u64, stat: &mut Stat) -> Result<(), minix_types::Errno> {
        let (details, links, time) =
            file_stat(&self.table, &self.root, inode).map_err(|e| e.to_errno())?;

        *stat = Stat {
            device: 0,
            inode: details.number,
            mode: details.mode,
            nlinks: links as u32,
            owner: details.uid as u32,
            group: details.gid as u32,
            special: details.device,
            size: 0,
            accessed: time,
            modified: time,
            changed: time,
            block_size: 0,
            blocks: 0,
        };
        Ok(())
    }

    /// C `ptyfs_chown` (ptyfs.c:225-239) via the decision function.
    fn change_owner(
        &mut self,
        inode: u64,
        owner: u32,
        group: u32,
    ) -> Result<u32, minix_types::Errno> {
        change_owner(
            &mut self.table,
            &mut self.root,
            inode,
            owner as u16,
            group as u16,
        )
        .map_err(|e| e.to_errno())
    }

    /// C `ptyfs_chmod` (ptyfs.c:246-257) via the decision function.
    fn change_mode(&mut self, inode: u64, mode: u32) -> Result<u32, minix_types::Errno> {
        change_mode(&mut self.table, &mut self.root, inode, mode).map_err(|e| e.to_errno())
    }

    /// C `ptyfs_statvfs` (ptyfs.c:287-296): names never truncate. The
    /// `StatVfs` value carries no flag field, so only `f_namemax` lands
    /// (the flag face is a tracked wire gap).
    fn stat_vfs(&mut self, vfs: &mut StatVfs) -> Result<(), minix_types::Errno> {
        let (_never_truncates, name_max) = filesystem_stat(NAME_MAX_REPORTED);
        vfs.name_max = name_max as u64;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 装配级全链：挂载 → 写入一个从节点（直接操纵表，模拟控制消息
    /// 到达后的效果）→ lookup → getdents 列出 → stat → chown/chmod。
    /// `ptyfs_table` 八入口里不依赖控制消息传递面的六个。
    #[test]
    fn test_driver_mount_lookup_list_stat_chmod() {
        let mut driver = PtyfsDriver::new();

        // 挂载：非根角色，回根节点（目录 0755），无能力位。
        let mut caps = minix_fs::protocol::CapabilityFlags(0);
        let root = driver
            .mount(0, minix_fs::protocol::MountFlags::EMPTY, &mut caps)
            .expect("挂载成功");
        assert_eq!(root.inode_number, ROOT_NUMBER);
        assert_eq!(root.mode, 0o040755);
        assert_eq!(caps.0, 0, "RES_NOFLAGS");
        // 根角色拒绝（C ptyfs.c:35-38）。
        let mut caps = minix_fs::protocol::CapabilityFlags(0);
        assert_eq!(
            driver
                .mount(0, minix_fs::protocol::MountFlags::IS_ROOT, &mut caps)
                .unwrap_err(),
            Errno::from_i32(EINVAL)
        );

        // 控制消息替身：直接向表写一个从节点（index 3 → 名字 "3"）。
        let _ = driver.table.set(
            3,
            crate::table::StoredNode { device: 9, mode: 0o020620, uid: 100, gid: 5, created: 77 },
        );

        // lookup：名字 "3" → 从节点详情。
        let (node, is_mount_point) = driver
            .lookup_child(ROOT_NUMBER, "3")
            .expect("已分配的从节点查得到");
        assert!(!is_mount_point);
        assert_eq!(node.inode_number, crate::SLAVE_BASE_NUMBER + 3);
        assert_eq!(node.device, 9);
        assert_eq!(node.mode, 0o020620);

        // getdents：一趟装下 dot、dot-dot 与从节点。回调的 chunk 是
        // **成段**字节（编码器可一次刷多条），按 reclen 逐条拆开。
        let mut position = 0i64;
        let collected: core::cell::RefCell<alloc::vec::Vec<u8>> =
            core::cell::RefCell::new(alloc::vec::Vec::new());
        let bytes = driver
            .get_dents(ROOT_NUMBER, &mut position, 4096, &mut |chunk| {
                collected.borrow_mut().extend_from_slice(chunk);
            })
            .expect("列出成功");
        let mut rows: alloc::vec::Vec<(u64, alloc::vec::Vec<u8>)> = alloc::vec::Vec::new();
        {
            let raw = collected.borrow();
            let mut at = 0usize;
            while at < raw.len() {
                let reclen = u16::from_le_bytes(raw[at + 8..at + 10].try_into().unwrap()) as usize;
                rows.push(decode_one(&raw[at..at + reclen]));
                at += reclen;
            }
        }
        assert!(bytes > 0);
        assert_eq!(rows.len(), 3, "dot + dot-dot + 从节点 3");
        assert_eq!(rows[0].0, ROOT_NUMBER);
        assert_eq!(rows[2].0, crate::SLAVE_BASE_NUMBER + 3);
        assert_eq!(rows[2].1, b"3".to_vec());

        // stat：从节点 nlink 为 1、三时间域为创建时间、rdev 为设备号。
        let mut st = Stat::zeroed();
        driver
            .stat(crate::SLAVE_BASE_NUMBER + 3, &mut st)
            .expect("stat 成功");
        assert_eq!(st.mode, 0o020620);
        assert_eq!(st.nlinks, 1);
        assert_eq!(st.special, 9);
        assert_eq!(st.accessed, 77);

        // chown + chmod：落表。
        let mode = driver
            .change_owner(crate::SLAVE_BASE_NUMBER + 3, 1000, 1000)
            .unwrap();
        assert_eq!(mode & 0o777, 0o620);
        let mode = driver
            .change_mode(crate::SLAVE_BASE_NUMBER + 3, 0o020600)
            .unwrap();
        assert_eq!(mode & 0o777, 0o600);

        // statvfs：名字不截断、namemax 上报。
        let mut vfs = StatVfs::zeroed();
        driver.stat_vfs(&mut vfs).expect("statvfs 成功");
        assert_eq!(vfs.name_max, NAME_MAX_REPORTED as u64);
    }

    /// 从一条目录项字节里取 inode 号与名字。编码格式：
    /// inode@0(8 字节)、reclen@8(2)、namelen@10(2)、type@12(1)、
    /// name@13 起（dentry.rs:198-207 的布局）。
    fn decode_one(chunk: &[u8]) -> (u64, alloc::vec::Vec<u8>) {
        let inode = u64::from_le_bytes(chunk[0..8].try_into().unwrap());
        let name_len = u16::from_le_bytes(chunk[10..12].try_into().unwrap()) as usize;
        (inode, chunk[13..13 + name_len].to_vec())
    }
}

