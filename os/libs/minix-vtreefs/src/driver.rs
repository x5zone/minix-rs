//! Framework wiring for virtual trees: a server value implementing the
//! driver trait over a [`Tree`] plus hooks.
//!
//! C correspondence: the `fs_*` callback family of
//! `minix3/minix/lib/libvtreefs/vtreefs.c` — the functions `table.c` wires
//! into the file-server callback table. The tree owns storage and the
//! hooks own refresh; this module is the seam that turns both into a
//! dispatchable file server. Servers construct a [`TreeServer`] with their
//! hook object and hand it to the framework's task loop; every method here
//! forwards to the tree or the hooks, never to a kernel interface.

use minix_fs::data::{DataChannel, MemoryBackend};
use minix_fs::dentry::{DentryEncoder, DirentType};
use minix_fs::driver::FsDriver;
use minix_fs::protocol::{CapabilityFlags, FileNode, MountFlags};

use minix_types::Errno;

use crate::tree::{DirEntry, FsHooks, NodeId, NodeStat, Tree, TreeError};

fn node_error(error: TreeError) -> Errno {
    error.to_errno()
}

/// A dispatchable virtual-tree server: tree storage plus server hooks.
pub struct TreeServer<H: FsHooks> {
    tree: Tree,
    hooks: H,
}

impl<H: FsHooks> TreeServer<H> {
    /// Build a server over a pre-built tree.
    pub fn new(tree: Tree, hooks: H) -> Self {
        Self { tree, hooks }
    }

    /// Shared tree access (the server's own bookkeeping reads it).
    pub fn tree(&self) -> &Tree {
        &self.tree
    }

    /// Exclusive tree access.
    pub fn tree_mut(&mut self) -> &mut Tree {
        &mut self.tree
    }

    /// The hook object (refresh callbacks live here).
    pub fn hooks_mut(&mut self) -> &mut H {
        &mut self.hooks
    }

    /// Report one node's identity as the framework sees it.
    fn describe(&self, node: NodeId) -> Result<FileNode, Errno> {
        let number = self.tree.number(node).map_err(node_error)?;
        let stat = self.tree.stat_of(node).map_err(node_error)?;
        Ok(FileNode::new(
            number,
            stat.mode as u32,
            stat.size,
            stat.uid as u32,
            stat.gid as u32,
            0,
        ))
    }
}

impl<H: FsHooks> FsDriver for TreeServer<H> {
    fn mount(
        &mut self,
        _device: u64,
        _flags: MountFlags,
        capabilities: &mut CapabilityFlags,
    ) -> Result<FileNode, Errno> {
        // Mount initialises the tree through the hook, then reports the
        // root (the libvtreefs init path). Virtual trees have no blocks;
        // the sixty-four-bit size flag is the only capability.
        self.hooks_mut().init_hook();
        *capabilities = CapabilityFlags(CapabilityFlags::SIZE_64BIT.0);
        self.describe(Tree::root())
    }

    fn unmounted(&mut self) {
        self.hooks_mut().cleanup_hook();
    }

    fn put_node(&mut self, inode: u64, count: u32) -> Result<(), Errno> {
        self.tree.release_batch(inode, count).map_err(node_error)
    }

    fn lookup_child(&mut self, directory: u64, name: &str) -> Result<(FileNode, bool), Errno> {
        // C: `fs_lookup` (`vtreefs.c`) resolves the name through the path
        // walk (`path.c`), which bumps the node's reference count
        // (`inode.c`); ".." and "." are answered by the same walk. The
        // `lookup_hook` runs first for live directories.
        let parent = self.tree.find(directory).map_err(node_error)?;
        let node = self
            .tree
            .lookup(&mut self.hooks, parent, name.as_bytes())
            .map_err(node_error)?;
        // Virtual trees never carry mount points below the root.
        Ok((self.describe(node)?, false))
    }

    fn is_mount_point(&mut self, _inode: u64) -> Result<(), Errno> {
        // Virtual trees never carry mount points below the root: the C
        // tree answers `fs_mountpt` from the framework's mount state.
        Err(Errno::from_i32(minix_types::ENOSYS))
    }

    fn read(
        &mut self,
        inode: u64,
        position: i64,
        length: usize,
        out: &mut dyn FnMut(&[u8]),
    ) -> Result<usize, Errno> {
        if position < 0 {
            return Err(Errno::from_i32(minix_types::EINVAL));
        }
        let mut buffer = alloc::vec![0u8; length];
        let moved = self
            .tree
            .read(
                &mut self.hooks,
                inode,
                &mut buffer,
                position as u64,
                length,
            )
            .map_err(node_error)?;
        if moved > 0 {
            out(&buffer[..moved]);
        }
        Ok(moved)
    }

    fn write(&mut self, inode: u64, position: i64, data: &[u8]) -> Result<usize, Errno> {
        if position < 0 {
            return Err(Errno::from_i32(minix_types::EINVAL));
        }
        let node = self.tree.find(inode).map_err(node_error)?;
        self.hooks_mut()
            .write_hook(node, data, position as u64)
            .map_err(|hook| hook.0)
    }

    fn get_dents(
        &mut self,
        inode: u64,
        position: &mut i64,
        capacity: usize,
        out: &mut dyn FnMut(&[u8]),
    ) -> Result<usize, Errno> {
        if *position < 0 {
            return Err(Errno::from_i32(minix_types::EINVAL));
        }
        // Listing stages through the directory-entry encoder exactly like
        // the framework's data path: the tree yields typed entries and the
        // encoder packs them for the virtual file system. The staging
        // buffer keeps C's `GETDENTS_BUFSIZ` floor (`file.c:11,21-23`); a
        // smaller one could not hold a full `NAME_MAX` record.
        let mut caller = alloc::vec![0u8; capacity.max(1)];
        let mut staging = alloc::vec![0u8; crate::LISTING_BUFFER_MINIMUM];
        let dir_node = self.tree.find(inode).map_err(node_error)?;
        let mut resume = *position as u64;
        let mut failure = None;
        let total = {
            let mut backend = MemoryBackend {
                storage: &mut caller,
                fail_with: None,
            };
            let mut encoder = DentryEncoder::new(
                DataChannel::Present {
                    backend: &mut backend,
                    size: capacity,
                },
                capacity,
                &mut staging,
            );
            self.tree
                .list(
                    &mut self.hooks,
                    dir_node,
                    &mut resume,
                    capacity,
                    &mut |entry: DirEntry| {
                        // C `fs_getdents` (`file.c:285-292`) returns an
                        // error, stops on zero (the entry did not fit), or
                        // continues on a positive record length.
                        match encoder.add(
                            entry.number,
                            &entry.name,
                            DirentType::from_raw(entry.file_type),
                        ) {
                            Ok(0) => false,
                            Ok(_) => true,
                            Err(error) => {
                                failure = Some(error.to_errno());
                                false
                            }
                        }
                    },
                )
                .map_err(node_error)?;
            if let Some(error) = failure {
                return Err(error);
            }
            encoder.finish()?
        };
        *position = resume as i64;
        out(&caller[..total]);
        Ok(total)
    }

    fn truncate(&mut self, inode: u64, start: i64, end: i64) -> Result<(), Errno> {
        if start < 0 || end < 0 {
            return Err(Errno::from_i32(minix_types::EINVAL));
        }
        let node = self.tree.find(inode).map_err(node_error)?;
        // `end == 0` asks for a whole-file truncate down to `start`
        // (`link.c:439-441`); range punching has no virtual-tree surface.
        if end == 0 {
            self.hooks_mut()
                .trunc_hook(node, start as u64)
                .map_err(|hook| hook.0)
        } else {
            Err(Errno::from_i32(minix_types::ENOSYS))
        }
    }

    fn change_owner(&mut self, inode: u64, owner: u32, group: u32) -> Result<u32, Errno> {
        let node = self.tree.find(inode).map_err(node_error)?;
        let mut stat = self.tree.stat_of(node).map_err(node_error)?;
        stat.uid = owner as u16;
        stat.gid = group as u16;
        self.hooks_mut().chstat_hook(node, &stat).map_err(|h| h.0)?;
        Ok(self.tree.stat_of(node).map_err(node_error)?.mode as u32)
    }

    fn change_mode(&mut self, inode: u64, mode: u32) -> Result<u32, Errno> {
        let node = self.tree.find(inode).map_err(node_error)?;
        let mut stat = self.tree.stat_of(node).map_err(node_error)?;
        stat.mode = (stat.mode & !0o7777) | (mode & 0o7777) as u16;
        self.hooks_mut().chstat_hook(node, &stat).map_err(|h| h.0)?;
        Ok(self.tree.stat_of(node).map_err(node_error)?.mode as u32)
    }

    fn update_times(
        &mut self,
        inode: u64,
        accessed: (i64, i64),
        modified: (i64, i64),
    ) -> Result<(), Errno> {
        let node = self.tree.find(inode).map_err(node_error)?;
        let stat = self.tree.stat_of(node).map_err(node_error)?;
        let _ = (accessed, modified);
        self.hooks_mut().chstat_hook(node, &stat).map_err(|h| h.0)
    }

    fn stat(&mut self, inode: u64, out: &mut minix_types::Stat) -> Result<(), Errno> {
        let (status, accessed, modified, changed) = self
            .tree
            .file_stat(&mut self.hooks, inode, 0)
            .map_err(node_error)?;
        *out = minix_types::Stat {
            device: 0,
            inode,
            mode: status.mode as u32,
            nlinks: status.nlinks as u32,
            owner: status.owner as u32,
            group: status.group as u32,
            special: 0,
            size: status.size,
            accessed,
            modified,
            changed,
            block_size: 0,
            blocks: 0,
        };
        Ok(())
    }

    fn stat_vfs(&mut self, out: &mut minix_types::StatVfs) -> Result<(), Errno> {
        // Virtual trees have no blocks: the counts name nodes and names
        // (the C `fs_statvfs` of libvtreefs reports capacity, zero free).
        *out = minix_types::StatVfs {
            blocks: 0,
            blocks_free: 0,
            blocks_available: 0,
            block_size: 0,
            fragment_size: 0,
            io_size: 0,
            files: self.tree.capacity() as u64,
            files_free: 0,
            files_available: 0,
            name_max: crate::NAME_MAX as u64,
        };
        Ok(())
    }

    fn symbolic_link(
        &mut self,
        directory: u64,
        name: &str,
        owner: u32,
        group: u32,
        target: &[u8],
    ) -> Result<(), Errno> {
        let dir_node = self.tree.find(directory).map_err(node_error)?;
        let stat = NodeStat {
            mode: 0o120777,
            uid: owner as u16,
            gid: group as u16,
            size: target.len() as i64,
            device: 0,
        };
        let node = self
            .tree
            .add(dir_node, name.as_bytes(), -1, stat, -1, 0)
            .map_err(node_error)?;
        self.hooks_mut()
            .slink_hook(node, name.as_bytes(), stat, target)
            .map_err(|h| h.0)
    }

    fn read_link(
        &mut self,
        inode: u64,
        capacity: usize,
        out: &mut dyn FnMut(&[u8]),
    ) -> Result<usize, Errno> {
        let node = self.tree.find(inode).map_err(node_error)?;
        let mut buffer = alloc::vec![0u8; capacity.min(512)];
        let moved = self
            .hooks_mut()
            .rdlink_hook(node, &mut buffer)
            .map_err(|h| h.0)?;
        out(&buffer[..moved]);
        Ok(moved)
    }

    fn unlink(&mut self, directory: u64, name: &str) -> Result<(), Errno> {
        let dir_node = self.tree.find(directory).map_err(node_error)?;
        let node = self
            .tree
            .lookup(&mut self.hooks, dir_node, name.as_bytes())
            .map_err(node_error)?;
        self.hooks_mut().unlink_hook(node).map_err(|h| h.0)?;
        self.tree.delete(node).map_err(node_error)
    }

    fn make_node(
        &mut self,
        directory: u64,
        name: &str,
        mode: u32,
        owner: u32,
        group: u32,
        device: u64,
    ) -> Result<(), Errno> {
        let dir_node = self.tree.find(directory).map_err(node_error)?;
        let stat = NodeStat {
            mode: mode as u16,
            uid: owner as u16,
            gid: group as u16,
            size: 0,
            device,
        };
        self.hooks_mut()
            .mknod_hook(dir_node, name.as_bytes(), stat)
            .map_err(|h| h.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::{HookError, Tree};

    struct NoHooks;

    impl FsHooks for NoHooks {}

    fn server() -> TreeServer<NoHooks> {
        let root = NodeStat {
            mode: 0o040755,
            uid: 0,
            gid: 0,
            size: 0,
            device: 0,
        };
        TreeServer::new(Tree::new(8, root, 0, 0).unwrap(), NoHooks)
    }

    fn flags(read_only: bool) -> MountFlags {
        if read_only {
            MountFlags::READ_ONLY
        } else {
            MountFlags::EMPTY
        }
    }

    #[test]
    fn test_mount_reports_root_with_size_flag() {
        let mut server = server();
        let mut capabilities = CapabilityFlags::EMPTY;
        let root = server.mount(1, flags(false), &mut capabilities).unwrap();
        assert_eq!(root.inode_number, 1);
        assert_eq!(root.mode, 0o040755);
        assert!(capabilities.is_64bit());
        server.unmounted();
    }

    #[test]
    fn test_stat_fills_typed_status() {
        let mut server = server();
        let mut capabilities = CapabilityFlags::EMPTY;
        server.mount(1, flags(false), &mut capabilities).unwrap();
        let mut stat = minix_types::Stat::zeroed();
        server.stat(1, &mut stat).unwrap();
        assert_eq!(stat.mode, 0o040755);
        assert_eq!(stat.inode, 1);
    }

    /// Memory-backed hooks: writes append, reads replay.
    struct MemoryHooks {
        data: alloc::vec::Vec<u8>,
    }


    impl FsHooks for MemoryHooks {
        fn read_hook(
            &mut self,
            _node: NodeId,
            buffer: &mut [u8],
            _offset: u64,
        ) -> Result<usize, HookError> {
            let take = buffer.len().min(self.data.len());
            buffer[..take].copy_from_slice(&self.data[..take]);
            Ok(take)
        }

        fn write_hook(
            &mut self,
            _node: NodeId,
            data: &[u8],
            _offset: u64,
        ) -> Result<usize, HookError> {
            self.data.extend_from_slice(data);
            Ok(data.len())
        }
    }

    #[test]
    fn test_write_then_read_round_trip_through_hooks() {
        let root = NodeStat {
            mode: 0o040755,
            uid: 0,
            gid: 0,
            size: 0,
            device: 0,
        };
        let tree = Tree::new(8, root, 0, 0).unwrap();
        let mut server = TreeServer::new(tree, MemoryHooks { data: alloc::vec::Vec::new() });
        let mut capabilities = CapabilityFlags::EMPTY;
        server.mount(1, flags(false), &mut capabilities).unwrap();
        // 在根下种一个普通文件节点（编号二：槽零加一）。
        server
            .tree_mut()
            .add(
                Tree::root(),
                b"a",
                -1,
                NodeStat {
                    mode: 0o100644,
                    uid: 0,
                    gid: 0,
                    size: 0,
                    device: 0,
                },
                -1,
                0,
            )
            .unwrap();
        let moved = server.write(2, 0, b"abc").unwrap();
        assert_eq!(moved, 3);
        let mut seen = alloc::vec::Vec::new();
        let got = server
            .read(2, 0, 8, &mut |bytes: &[u8]| seen.extend_from_slice(bytes))
            .unwrap();
        assert_eq!(got, 3);
        assert_eq!(seen, b"abc".to_vec());
    }

    #[test]
    fn test_missing_node_reports_invalid() {
        let mut server = server();
        let mut capabilities = CapabilityFlags::EMPTY;
        server.mount(1, flags(false), &mut capabilities).unwrap();
        let error = server.read(99, 0, 4, &mut |_| {}).unwrap_err();
        assert_eq!(error.to_i32(), minix_types::EINVAL);
    }

    /// Header bytes every record carries: inode number, record length, name
    /// length, and file-type byte (`dirent.h` `_DIRENT_NAMEOFF`).
    const DIRENT_HEADER: usize = 13;

    /// Add one regular file under the root (child numbering follows the
    /// tree's slot order, which is what the listing walks).
    fn add_child(server: &mut TreeServer<NoHooks>, name: &[u8]) {
        server
            .tree_mut()
            .add(
                Tree::root(),
                name,
                -1,
                NodeStat {
                    mode: 0o100644,
                    uid: 0,
                    gid: 0,
                    size: 0,
                    device: 0,
                },
                -1,
                0,
            )
            .unwrap();
    }

    /// Decode a packed listing into the names it carries.
    fn dirent_names(bytes: &[u8]) -> alloc::vec::Vec<alloc::vec::Vec<u8>> {
        let mut names = alloc::vec::Vec::new();
        let mut offset = 0usize;
        while offset + DIRENT_HEADER <= bytes.len() {
            let record = u16::from_ne_bytes([bytes[offset + 8], bytes[offset + 9]]) as usize;
            let name_length =
                u16::from_ne_bytes([bytes[offset + 10], bytes[offset + 11]]) as usize;
            if record < DIRENT_HEADER || offset + DIRENT_HEADER + name_length > bytes.len() {
                break;
            }
            names.push(bytes[offset + DIRENT_HEADER..offset + DIRENT_HEADER + name_length].to_vec());
            offset += record;
        }
        names
    }

    #[test]
    fn test_get_dents_flushes_the_staging_tail() {
        let mut server = server();
        let mut capabilities = CapabilityFlags::EMPTY;
        server.mount(1, flags(false), &mut capabilities).unwrap();
        for name in [&b"a"[..], b"bb", b"ccc", b"dddd"] {
            add_child(&mut server, name);
        }
        let mut position = 0i64;
        let mut seen = alloc::vec::Vec::new();
        let moved = server
            .get_dents(1, &mut position, 4096, &mut |bytes| {
                seen.extend_from_slice(bytes)
            })
            .unwrap();
        assert_eq!(moved, seen.len());
        // Children come out in C's insertion order: `add_inode` inserts at
        // the head of the child list (`inode.c:236`), so the listing walks
        // them newest first.
        assert_eq!(
            dirent_names(&seen),
            alloc::vec![
                b".".to_vec(),
                b"..".to_vec(),
                b"dddd".to_vec(),
                b"ccc".to_vec(),
                b"bb".to_vec(),
                b"a".to_vec(),
            ]
        );
        // The count is the packed record length, not the caller capacity:
        // dot, dot-dot, then the four children.
        let expected: usize = [1usize, 2, 4, 3, 2, 1]
            .iter()
            .map(|length| minix_fs::dentry::record_length(*length))
            .sum();
        assert_eq!(moved, expected);
    }

    #[test]
    fn test_get_dents_stops_on_a_full_buffer_and_resumes_without_gaps() {
        let mut server = server();
        let mut capabilities = CapabilityFlags::EMPTY;
        server.mount(1, flags(false), &mut capabilities).unwrap();
        for name in [&b"a"[..], b"bb", b"ccc"] {
            add_child(&mut server, name);
        }
        let mut position = 0i64;
        let mut first = alloc::vec::Vec::new();
        let moved = server
            .get_dents(1, &mut position, 40, &mut |bytes| {
                first.extend_from_slice(bytes)
            })
            .unwrap();
        // Dot and dot-dot (sixteen bytes each) fit; the first child record
        // would need forty-eight. The walk stops on that child and leaves
        // the position there for the next call.
        assert_eq!(moved, 32);
        assert_eq!(position, 2);
        assert_eq!(
            dirent_names(&first),
            alloc::vec![b".".to_vec(), b"..".to_vec()]
        );
        let mut rest = alloc::vec::Vec::new();
        let resumed = server
            .get_dents(1, &mut position, 4096, &mut |bytes| {
                rest.extend_from_slice(bytes)
            })
            .unwrap();
        assert_eq!(resumed, rest.len());
        // The head-insertion order again: the first plain child is the
        // newest one, then the older ones.
        assert_eq!(
            dirent_names(&rest),
            alloc::vec![b"ccc".to_vec(), b"bb".to_vec(), b"a".to_vec()]
        );
    }

    #[test]
    fn test_get_dents_reports_invalid_when_no_record_fits() {
        let mut server = server();
        let mut capabilities = CapabilityFlags::EMPTY;
        server.mount(1, flags(false), &mut capabilities).unwrap();
        let mut position = 0i64;
        let mut seen = alloc::vec::Vec::new();
        let error = server
            .get_dents(1, &mut position, 8, &mut |bytes| {
                seen.extend_from_slice(bytes)
            })
            .unwrap_err();
        assert_eq!(error.to_i32(), minix_types::EINVAL);
        assert!(seen.is_empty());
        assert_eq!(position, 0);
    }

    #[test]
    fn test_get_dents_lists_a_name_at_the_name_maximum() {
        let mut server = server();
        let mut capabilities = CapabilityFlags::EMPTY;
        server.mount(1, flags(false), &mut capabilities).unwrap();
        let name = alloc::vec![b'x'; crate::NAME_MAX];
        add_child(&mut server, &name);
        let mut position = 0i64;
        let mut seen = alloc::vec::Vec::new();
        let moved = server
            .get_dents(1, &mut position, 4096, &mut |bytes| {
                seen.extend_from_slice(bytes)
            })
            .unwrap();
        assert_eq!(moved, seen.len());
        assert!(dirent_names(&seen).contains(&name));
    }
}
