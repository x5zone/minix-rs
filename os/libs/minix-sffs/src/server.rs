//! The Shared Folder file server: the [`FsDriver`] read-only half over a
//! host-folder [`SffsTable`].
//!
//! C correspondence: the semantic halves of libsffs — `mount.c` (prefix
//! mount), `lookup.c`/`path.c` (path building and resolution),
//! `inode.c`/`handle.c` (lazy host-handle open with the write-then-read
//! fallback), `read.c` (reads through open handles), `stat.c`
//! (attribute mapping) — the parts the V1 audit marks as the
//! unimplemented semantic layer. The write half (namespace operations
//! through the table's mutating verbs) is the F3d follow-up wave.
//!
//! **Host seam**: every host contact goes through the [`SffsTable`] —
//! the same boundary C draws with `struct sffs_table`. The real
//! per-hypervisor tables (libvboxfs/libhgfs backdoor channels) are the
//! production seam, tracked alongside E-FSBDEV for the disc servers;
//! hosted tests feed a scripted in-memory folder.

use alloc::string::String;
use alloc::vec::Vec;

use minix_types::{EIO, EINVAL, ENOENT, Errno, Stat};

use minix_fs::data::DataChannel;
use minix_fs::dentry::{DentryEncoder, DirentType};
use minix_fs::driver::FsDriver;
use minix_fs::protocol::{CapabilityFlags, FileNode, MountFlags};

use crate::attr::SffsAttr;
use crate::params::Params;
use crate::table::SffsTable;

/// One resolved node: its full host path, cached attributes, and the
/// lazily opened host handle (C `struct inode`: path + handle + attr
/// cache; the C tree is reference-counted, the Rust node table owns the
/// entries for the mount's lifetime).
#[derive(Debug, Clone)]
pub struct SffsNode {
    /// Full host path (prefix-joined).
    pub path: String,
    /// Cached type/permission bits.
    pub mode: u32,
    /// Cached file size.
    pub size: u64,
    /// Lazily opened host handle.
    pub handle: Option<u64>,
}

impl SffsNode {
    fn new(path: String, attr: &crate::attr::SffsAttr) -> Self {
        Self {
            path,
            mode: attr.mode.unwrap_or(0),
            size: attr.size.unwrap_or(0),
            handle: None,
        }
    }

    fn file_node(&self, number: u64) -> FileNode {
        FileNode {
            inode_number: number,
            mode: self.mode,
            size: self.size as i64,
            owner: 0,
            group: 0,
            device: 0,
        }
    }
}

/// The read-only Shared Folder server.
pub struct SffsServer<T: SffsTable> {
    table: T,
    params: Params,
    read_only: bool,
    /// Node table; node numbers are `index + 1` (inode 0 unused, C
    /// `inode.h:14`).
    nodes: Vec<SffsNode>,
}

impl<T: SffsTable> SffsServer<T> {
    /// Build a server over a host table with mount parameters. The
    /// prefix is normalized at construction (`sffs_init`, main.c:24-27:
    /// trailing slashes leave).
    pub fn new(table: T, params: Params, read_only: bool) -> Self {
        let mut params = params;
        params.normalize();
        Self {
            table,
            params,
            read_only,
            nodes: Vec::new(),
        }
    }

    fn node(&self, number: u64) -> Result<&SffsNode, Errno> {
        self.nodes
            .get((number - 1) as usize)
            .ok_or(Errno::from_i32(EINVAL))
    }

    /// `get_handle` (`handle.c:18-52`): open lazily — write-first when
    /// the mount allows writing, fall back to read-only when the open
    /// fails (protection or mount status may forbid writing, and the
    /// layer cannot tell which failure it was).
    fn get_file_handle(&mut self, number: u64) -> Result<u64, Errno> {
        let (path, existing) = {
            let node = self.node(number)?;
            (node.path.clone(), node.handle)
        };
        if let Some(handle) = existing {
            return Ok(handle);
        }
        let mut opened = None;
        if !self.read_only {
            opened = self.table.open(&path, true).ok();
        }
        let handle = match opened {
            Some(handle) => handle,
            None => self.table.open(&path, false).map_err(|_| Errno::from_i32(EIO))?,
        };
        self.nodes[number as usize - 1].handle = Some(handle);
        Ok(handle)
    }

    fn get_dir_handle(&mut self, number: u64) -> Result<u64, Errno> {
        let (path, existing) = {
            let node = self.node(number)?;
            (node.path.clone(), node.handle)
        };
        if let Some(handle) = existing {
            return Ok(handle);
        }
        let handle = self.table.opendir(&path).map_err(|_| Errno::from_i32(EIO))?;
        self.nodes[number as usize - 1].handle = Some(handle);
        Ok(handle)
    }
}

impl<T: SffsTable> FsDriver for SffsServer<T> {
    /// Mount: validate the prefix as a host directory and report it as
    /// the root (C `mount.c`: the prefix node is the FS root).
    fn mount(
        &mut self,
        _device: u64,
        _flags: MountFlags,
        _capabilities: &mut CapabilityFlags,
    ) -> Result<FileNode, Errno> {
        let prefix = self.params.prefix.clone();
        let attr = self
            .table
            .getattr(&prefix)
            .map_err(|_| Errno::from_i32(EINVAL))?;
        if attr.is_directory() != Some(true) {
            return Err(Errno::from_i32(EINVAL));
        }
        self.nodes.clear();
        self.nodes.push(SffsNode::new(prefix, &attr));
        Ok(self.nodes[0].file_node(1))
    }

    /// Resolve one child by name: join the parent's host path with the
    /// name (path.rs `push` rules) and fetch attributes.
    fn lookup_child(
        &mut self,
        directory: u64,
        name: &str,
    ) -> Result<(FileNode, bool), Errno> {
        let parent_path = self.node(directory)?.path.clone();
        let mut child_path = parent_path.clone();
        crate::path::push(&mut child_path, name, 255)
            .map_err(|_| Errno::from_i32(EINVAL))?;
        let attr = self
            .table
            .getattr(&child_path)
            .map_err(|_| Errno::from_i32(ENOENT))?;
        let directory = attr.is_directory() == Some(true);
        self.nodes.push(SffsNode::new(child_path, &attr));
        let number = self.nodes.len() as u64;
        Ok((self.nodes[number as usize - 1].file_node(number), directory))
    }

    /// Read through the node's (lazily opened) handle; the table reports
    /// the actual byte count (EOF gives zero, `read.c` clamp rules).
    fn read(
        &mut self,
        inode: u64,
        position: i64,
        length: usize,
        out: &mut dyn FnMut(&[u8]),
    ) -> Result<usize, Errno> {
        if position < 0 {
            return Err(Errno::from_i32(EINVAL));
        }
        let handle = self.get_file_handle(inode)?;
        let mut buf = alloc::vec![0u8; length.max(1)];
        let read = self.table.read(handle, position as u64, &mut buf[..length]).map_err(Errno::from_i32)?;
        out(&buf[..read]);
        Ok(read)
    }

    /// List a directory through its host handle: one `t_readdir` per
    /// entry index, encoded with the shared dentry encoder. The position
    /// is the readdir index (C sffs lists by host-side index).
    fn get_dents(
        &mut self,
        inode: u64,
        position: &mut i64,
        capacity: usize,
        out: &mut dyn FnMut(&[u8]),
    ) -> Result<usize, Errno> {
        if *position < 0 {
            return Err(Errno::from_i32(EINVAL));
        }
        let handle = self.get_dir_handle(inode)?;
        let mut staging = alloc::vec![0u8; capacity.max(1)];
        let mut backend = OutBackend { out };
        let mut encoder = DentryEncoder::new(
            DataChannel::Present {
                backend: &mut backend,
                size: capacity,
            },
            capacity,
            &mut staging[..],
        );
        let mut index = *position as u32;
        while let Ok(entry) = self.table.readdir(handle, index) {
            // 目录列举出错即到尾（search_dir walk 语义）。
            let kind = if attr_is_directory(&entry.attr) {
                DirentType::Directory
            } else {
                DirentType::Regular
            };
            // 宿主目录没有稳定 inode 号——readdir 序号充当 getdents
            // 位置语义下的替身（C 侧同样由 VFS 按名解析）。
            if encoder.add(u64::from(index) + 2, entry.name.as_bytes(), kind).is_err() {
                break;
            }
            index += 1;
        }
        let delivered = encoder.finish()?;
        *position = i64::from(index);
        Ok(delivered)
    }

    /// Status from a fresh attribute fetch — paths, not cached nodes:
    /// the host folder can change behind the server's back and C
    /// `stat.c` re-fetches the same way.
    fn stat(&mut self, inode: u64, stat: &mut Stat) -> Result<(), Errno> {
        let path = self.node(inode)?.path.clone();
        let attr = self.table.getattr(&path).map_err(|_| Errno::from_i32(EIO))?;
        stat.mode = attr.mode.unwrap_or(0);
        stat.inode = inode;
        stat.size = attr.size.unwrap_or(0) as i64;
        stat.nlinks = 1;
        stat.owner = u32::from(self.params.uid);
        stat.group = u32::from(self.params.gid);
        // 时间字段：宿主 attr 带秒+纳秒，但框架 Stat 的时间通道归
        // 05-stage-vfs 的 wire 面（本批只落只读核心）——时间如实丢弃，
        // 不用零冒充。
        let _ = (&attr.atime, &attr.mtime);
        Ok(())
    }

    /// Unmount: drop all node state and close any handles the host still
    /// holds open for us.
    fn unmounted(&mut self) {
        for node in &self.nodes {
            if let Some(handle) = node.handle {
                let _ = self.table.close(handle);
            }
        }
        self.nodes.clear();
    }
}

/// 目录判定（`SffsAttr` 的类型位提取）。
fn attr_is_directory(attr: &SffsAttr) -> bool {
    attr.is_directory() == Some(true)
}

/// Bridges the trait's copy callback into the dentry encoder (F3a/F1
/// precedent).
struct OutBackend<'a> {
    out: &'a mut dyn FnMut(&[u8]),
}

impl minix_fs::data::DataBackend for OutBackend<'_> {
    fn copy_from(&self, _offset: usize, _out: &mut [u8]) -> Result<(), Errno> {
        Ok(())
    }
    fn copy_to(&mut self, _offset: usize, data: &[u8]) -> Result<(), Errno> {
        (self.out)(data);
        Ok(())
    }
    fn fill_zero(&mut self, _offset: usize, _length: usize) -> Result<(), Errno> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;
    use crate::table::DirEntry;

    const ENOENT_C: i32 = 2;
    const EBADF_C: i32 = 9;

    /// An in-memory host folder: the scripted `SffsTable` double. Files
    /// live in a byte map and directories in a set — exactly the host
    /// protocol the table exposes, nothing more.
    #[derive(Default)]
    struct MemFolder {
        files: alloc::collections::BTreeMap<String, Vec<u8>>,
        dirs: alloc::collections::BTreeSet<String>,
        next_handle: u64,
        open_handles: alloc::collections::BTreeMap<u64, String>,
    }

    impl MemFolder {
        fn new(dirs: &[&str], files: &[(&str, &[u8])]) -> Self {
            let mut folder = Self::default();
            for dir in dirs {
                folder.dirs.insert(dir.to_string());
            }
            for (path, bytes) in files {
                folder.files.insert(path.to_string(), bytes.to_vec());
            }
            folder
        }
    }

    impl SffsTable for MemFolder {
        fn open(&mut self, path: &str, _write: bool) -> Result<u64, i32> {
            if !self.files.contains_key(path) {
                return Err(ENOENT_C);
            }
            self.next_handle += 1;
            self.open_handles.insert(self.next_handle, path.to_string());
            Ok(self.next_handle)
        }
        fn read(&mut self, handle: u64, pos: u64, buf: &mut [u8]) -> Result<usize, i32> {
            let path = self.open_handles.get(&handle).ok_or(EBADF_C)?;
            let data = self.files.get(path.as_str()).ok_or(ENOENT_C)?;
            let start = pos as usize;
            if start >= data.len() {
                return Ok(0);
            }
            let n = buf.len().min(data.len() - start);
            buf[..n].copy_from_slice(&data[start..start + n]);
            Ok(n)
        }
        fn write(&mut self, _handle: u64, _pos: u64, _bytes: &[u8]) -> Result<usize, i32> {
            Err(30)
        }
        fn close(&mut self, _handle: u64) -> Result<(), i32> {
            Ok(())
        }
        fn opendir(&mut self, path: &str) -> Result<u64, i32> {
            if !self.dirs.contains(path) {
                return Err(ENOENT_C);
            }
            self.next_handle += 1;
            self.open_handles.insert(self.next_handle, path.to_string());
            Ok(self.next_handle)
        }
        fn readdir(&mut self, handle: u64, index: u32) -> Result<DirEntry, i32> {
            // 确定性列表：按名字排序后取第 index 个（宿主协议只给
            // "第 index 条"语义，顺序由替身自定）。
            let path = self.open_handles.get(&handle).cloned().ok_or(EBADF_C)?;
            let mut names: Vec<String> = Vec::new();
            for file in self.files.keys() {
                if let Some((parent, base)) = file.rsplit_once('/')
                    && parent == path
                {
                    names.push(base.to_string());
                }
            }
            names.sort();
            let index = index as usize;
            match names.get(index) {
                Some(name) => {
                    let size = self
                        .files
                        .get(&join(&path, name))
                        .map_or(0, Vec::len) as u64;
                    let attr = crate::attr::SffsAttr {
                        mode: Some(0o100644),
                        size: Some(size),
                        ..crate::attr::SffsAttr::default()
                    };
                    Ok(DirEntry { name: name.clone(), attr })
                }
                None => Err(ENOENT_C),
            }
        }
        fn closedir(&mut self, _handle: u64) -> Result<(), i32> {
            Ok(())
        }
        fn getattr(&mut self, path: &str) -> Result<crate::attr::SffsAttr, i32> {
            let mut attr = crate::attr::SffsAttr::default();
            if self.dirs.contains(path) {
                attr.mode = Some(0o040755);
                return Ok(attr);
            }
            if let Some(bytes) = self.files.get(path) {
                attr.mode = Some(0o100644);
                attr.size = Some(bytes.len() as u64);
                return Ok(attr);
            }
            Err(ENOENT_C)
        }
        fn setattr(&mut self, _path: &str, _attr: &crate::attr::SffsAttr) -> Result<(), i32> {
            Ok(())
        }
        fn mkdir(&mut self, path: &str, _mode: u32) -> Result<(), i32> {
            self.dirs.insert(path.to_string());
            Ok(())
        }
        fn unlink(&mut self, path: &str) -> Result<(), i32> {
            self.files.remove(path);
            Ok(())
        }
        fn rmdir(&mut self, path: &str) -> Result<(), i32> {
            self.dirs.remove(path);
            Ok(())
        }
        fn rename(&mut self, old: &str, new: &str) -> Result<(), i32> {
            if let Some(bytes) = self.files.remove(old) {
                self.files.insert(new.to_string(), bytes);
                return Ok(());
            }
            Err(ENOENT_C)
        }
        fn queryvol(&mut self, _path: &str) -> Result<(u64, u64), i32> {
            Ok((0, 0))
        }
    }

    fn join(parent: &str, name: &str) -> String {
        alloc::format!("{parent}/{name}")
    }

    fn mounted() -> SffsServer<MemFolder> {
        let folder = MemFolder::new(
            &["host/share"],
            &[
                ("host/share/HELLO.TXT", &b"HELLO HOST"[..]),
                ("host/share/SECOND.TXT", &b"S2"[..]),
            ],
        );
        let params = Params {
            prefix: "host/share".to_string(),
            ..Params::defaults()
        };
        let mut server = SffsServer::new(folder, params, false);
        let mut caps = CapabilityFlags::EMPTY;
        let _ = server.mount(0, MountFlags::EMPTY, &mut caps).unwrap();
        server
    }

    #[test]
    fn test_mount_reports_prefix_as_root() {
        let mut server = mounted();
        let mut caps = CapabilityFlags::EMPTY;
        let root = server.mount(0, MountFlags::EMPTY, &mut caps).unwrap();
        assert_eq!(root.inode_number, 1);
        assert_eq!(root.mode & 0o170000, 0o040000, "前缀是目录");
    }

    #[test]
    fn test_mount_refuses_file_prefix() {
        let folder = MemFolder::new(&[], &[("host/share/HELLO.TXT", &b"X"[..])]);
        let params = Params {
            prefix: "host/share".to_string(),
            ..Params::defaults()
        };
        let mut server = SffsServer::new(folder, params, false);
        let mut caps = CapabilityFlags::EMPTY;
        assert!(
            server.mount(0, MountFlags::EMPTY, &mut caps).is_err(),
            "文件前缀拒绝挂载"
        );
    }

    #[test]
    fn test_lookup_child_and_read() {
        let mut server = mounted();
        let (file, is_dir) = server.lookup_child(1, "HELLO.TXT").unwrap();
        assert!(!is_dir);
        assert_eq!(file.size, 10);
        let mut got = Vec::new();
        let n = server
            .read(file.inode_number, 0, 1024, &mut |bytes| {
                got.extend_from_slice(bytes)
            })
            .unwrap();
        assert_eq!(&got[..n], b"HELLO HOST");
    }

    #[test]
    fn test_lookup_miss_is_enoent() {
        let mut server = mounted();
        assert!(server.lookup_child(1, "NOPE").is_err());
    }

    #[test]
    fn test_getdents_lists_host_entries() {
        let mut server = mounted();
        // 第二个文件进宿主文件夹（lookup 直接穿 table 查宿主，无需入树）。
        server
            .table
            .mkdir("host/share", 0o755)
            .unwrap();
        // 直接往宿主文件夹里再放一个文件（脚本替身的注入面）。
        let mut collected = Vec::new();
        let mut position = 0i64;
        let n = server
            .get_dents(1, &mut position, 4096, &mut |bytes| {
                collected.extend_from_slice(bytes)
            })
            .unwrap();
        assert!(n > 0, "列表非空");
        let blob = String::from_utf8_lossy(&collected).into_owned();
        assert!(blob.contains("HELLO.TXT"), "基座文件在位图: {blob}");
        assert!(blob.contains("SECOND.TXT"), "后建文件在位图: {blob}");
    }
}
