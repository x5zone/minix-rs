//! In-memory file server: a complete small [`FsDriver`] implementation.
//!
//! This module is the framework's worked example. It keeps a tiny tree of
//! directories, regular files, and symbolic links in memory and serves every
//! namespace, data, and metadata operation through the trait; only raw
//! device block operations stay unimplemented (there is no device). New
//! server authors can read it as the reference for wiring a real store to
//! the trait, and the request path tests use it as the second,
//! behavior-different implementation next to
//! [`NullDriver`](crate::driver::NullDriver).
//!
//! Not covered on purpose: permission enforcement beyond storing owner,
//! group, and mode bits (the lookup helper already checks directory search
//! rights); hard-link reference counts (names simply point at inodes);
//! persistence (everything vanishes with the value).

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use minix_types::{EEXIST, EINVAL, ENOENT, ENOSYS, ENOTDIR, ENOTEMPTY, Errno};

use crate::data::MemoryBackend;
use crate::dentry::{DentryEncoder, DirentType};
use crate::driver::FsDriver;
use crate::protocol::{CapabilityFlags, FileNode, MountFlags};

/// Inode number of the root directory.
pub const ROOT_INODE: u64 = 1;

/// First inode number handed out to created files.
const FIRST_FREE_INODE: u64 = 2;

/// Layout of the status buffer filled by `stat`: inode (eight bytes),
/// mode (four bytes), size (eight bytes), all little-endian. This layout is
/// local to the memory server and only used by its own tests; production
/// servers define the caller-agreed layout of their own replies.
pub const STAT_BUFFER_SIZE: usize = 20;

/// One stored node.
#[derive(Debug, Clone)]
enum Node {
    /// Directory with its children by name.
    Directory {
        /// Child name to inode number.
        children: BTreeMap<Vec<u8>, u64>,
        mode: u32,
        owner: u32,
        group: u32,
    },
    /// Regular file with its bytes.
    File {
        bytes: Vec<u8>,
        mode: u32,
        owner: u32,
        group: u32,
    },
    /// Symbolic link with its target bytes.
    Symlink {
        target: Vec<u8>,
        owner: u32,
        group: u32,
    },
}

/// In-memory file server.
#[derive(Debug, Clone)]
pub struct MemFileServer {
    nodes: BTreeMap<u64, Node>,
    parents: BTreeMap<u64, u64>,
    next_inode: u64,
    device: Option<u64>,
}

impl MemFileServer {
    /// Empty server with only the root directory.
    pub fn new() -> Self {
        let mut nodes = BTreeMap::new();
        nodes.insert(
            ROOT_INODE,
            Node::Directory {
                children: BTreeMap::new(),
                mode: 0o040755,
                owner: 0,
                group: 0,
            },
        );
        let mut parents = BTreeMap::new();
        parents.insert(ROOT_INODE, ROOT_INODE);
        Self {
            nodes,
            parents,
            next_inode: FIRST_FREE_INODE,
            device: None,
        }
    }

    /// Describe a stored node as a [`FileNode`].
    fn describe(&self, inode: u64) -> Result<FileNode, Errno> {
        let node = self.nodes.get(&inode).ok_or(Errno::from_i32(ENOENT))?;
        let (mode, size, owner, group) = match node {
            Node::Directory {
                mode, owner, group, ..
            } => (*mode, 0, *owner, *group),
            Node::File {
                bytes,
                mode,
                owner,
                group,
            } => (*mode, bytes.len() as i64, *owner, *group),
            Node::Symlink {
                target,
                owner,
                group,
            } => (0o120777, target.len() as i64, *owner, *group),
        };
        Ok(FileNode::new(inode, mode, size, owner, group, 0))
    }

    /// Insert a node under a directory, failing when the name is taken.
    fn insert_child(&mut self, directory: u64, name: &str, node: Node) -> Result<u64, Errno> {
        let parent = self
            .nodes
            .get_mut(&directory)
            .ok_or(Errno::from_i32(ENOENT))?;
        let Node::Directory { children, .. } = parent else {
            return Err(Errno::from_i32(ENOTDIR));
        };
        if children.contains_key(name.as_bytes()) {
            return Err(Errno::from_i32(EEXIST));
        }
        let inode = self.next_inode;
        self.next_inode += 1;
        children.insert(name.as_bytes().to_vec(), inode);
        self.nodes.insert(inode, node);
        self.parents.insert(inode, directory);
        Ok(inode)
    }

    /// Remove a child name, returning its inode number.
    fn remove_child(&mut self, directory: u64, name: &str) -> Result<u64, Errno> {
        let parent = self
            .nodes
            .get_mut(&directory)
            .ok_or(Errno::from_i32(ENOENT))?;
        let Node::Directory { children, .. } = parent else {
            return Err(Errno::from_i32(ENOTDIR));
        };
        children
            .remove(name.as_bytes())
            .ok_or(Errno::from_i32(ENOENT))
    }

    /// File type tag for directory listings.
    fn entry_type(&self, inode: u64) -> DirentType {
        match self.nodes.get(&inode) {
            Some(Node::Directory { .. }) => DirentType::Directory,
            Some(Node::Symlink { .. }) => DirentType::Symlink,
            Some(Node::File { .. }) => DirentType::Other,
            None => DirentType::Unknown,
        }
    }
}

impl Default for MemFileServer {
    fn default() -> Self {
        Self::new()
    }
}

impl FsDriver for MemFileServer {
    fn mount(
        &mut self,
        device: u64,
        _flags: MountFlags,
        capabilities: &mut CapabilityFlags,
    ) -> Result<FileNode, Errno> {
        *capabilities = CapabilityFlags::EMPTY;
        self.device = Some(device);
        self.describe(ROOT_INODE)
    }

    fn unmounted(&mut self) {
        self.device = None;
    }

    fn is_mount_point(&mut self, inode: u64) -> Result<(), Errno> {
        // The memory server never hosts mounts: report "not a mount point".
        // Callers treat anything but success as a plain directory.
        if self.nodes.contains_key(&inode) {
            Err(Errno::from_i32(EINVAL))
        } else {
            Err(Errno::from_i32(ENOENT))
        }
    }

    fn new_node(
        &mut self,
        mode: u32,
        owner: u32,
        group: u32,
        _device: u64,
    ) -> Result<FileNode, Errno> {
        let inode = self.next_inode;
        self.next_inode += 1;
        self.nodes.insert(
            inode,
            Node::File {
                bytes: Vec::new(),
                mode,
                owner,
                group,
            },
        );
        self.describe(inode)
    }

    fn lookup_child(&mut self, directory: u64, name: &str) -> Result<(FileNode, bool), Errno> {
        if name == "." {
            return Ok((self.describe(directory)?, false));
        }
        if name == ".." {
            let parent = self.parents.get(&directory).copied().unwrap_or(ROOT_INODE);
            return Ok((self.describe(parent)?, false));
        }
        let child = {
            let parent = self.nodes.get(&directory).ok_or(Errno::from_i32(ENOENT))?;
            let Node::Directory { children, .. } = parent else {
                return Err(Errno::from_i32(ENOTDIR));
            };
            children
                .get(name.as_bytes())
                .copied()
                .ok_or(Errno::from_i32(ENOENT))?
        };
        Ok((self.describe(child)?, false))
    }

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
        let node = self.nodes.get(&inode).ok_or(Errno::from_i32(ENOENT))?;
        let Node::File { bytes, .. } = node else {
            return Err(Errno::from_i32(EINVAL));
        };
        let start = (position as usize).min(bytes.len());
        let end = start.saturating_add(length).min(bytes.len());
        out(&bytes[start..end]);
        Ok(end - start)
    }

    fn write(&mut self, inode: u64, position: i64, data: &[u8]) -> Result<usize, Errno> {
        if position < 0 {
            return Err(Errno::from_i32(EINVAL));
        }
        let node = self.nodes.get_mut(&inode).ok_or(Errno::from_i32(ENOENT))?;
        let Node::File { bytes, .. } = node else {
            return Err(Errno::from_i32(EINVAL));
        };
        let start = position as usize;
        if start > bytes.len() {
            bytes.resize(start, 0);
        }
        let end = start.saturating_add(data.len());
        if end > bytes.len() {
            bytes.resize(end, 0);
        }
        bytes[start..end].copy_from_slice(data);
        Ok(data.len())
    }

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
        let names: Vec<(Vec<u8>, u64)> = {
            let node = self.nodes.get(&inode).ok_or(Errno::from_i32(ENOENT))?;
            let Node::Directory { children, .. } = node else {
                return Err(Errno::from_i32(ENOTDIR));
            };
            children
                .iter()
                .map(|(name, child)| (name.clone(), *child))
                .collect()
        };
        // Stage through the directory entry encoder into a local buffer,
        // then deliver the staged bytes through the caller closure.
        let mut caller = alloc::vec![0u8; capacity.max(1)];
        let mut staging = alloc::vec![0u8; 256];
        let start = (*position as usize).min(names.len());
        let mut delivered = 0usize;
        {
            let mut backend = MemoryBackend {
                storage: &mut caller[..],
                fail_with: None,
            };
            let channel = crate::data::DataChannel::Present {
                backend: &mut backend,
                size: capacity,
            };
            let mut encoder = DentryEncoder::new(channel, capacity, &mut staging[..]);
            for (name, child) in names.iter().skip(start) {
                let entry_type = self.entry_type(*child);
                match encoder.add(*child, name, entry_type) {
                    Ok(0) => break,
                    Ok(_) => delivered += 1,
                    Err(error) => return Err(error.to_errno()),
                }
            }
            let bytes = encoder.finish()?;
            *position += delivered as i64;
            out(&caller[..bytes]);
            Ok(bytes)
        }
    }

    fn truncate(&mut self, inode: u64, start: i64, end: i64) -> Result<(), Errno> {
        if start < 0 || end < 0 || end < start {
            return Err(Errno::from_i32(EINVAL));
        }
        let node = self.nodes.get_mut(&inode).ok_or(Errno::from_i32(ENOENT))?;
        let Node::File { bytes, .. } = node else {
            return Err(Errno::from_i32(EINVAL));
        };
        bytes.resize(end as usize, 0);
        let _ = start;
        Ok(())
    }

    fn create(
        &mut self,
        directory: u64,
        name: &str,
        mode: u32,
        owner: u32,
        group: u32,
    ) -> Result<FileNode, Errno> {
        let inode = self.insert_child(
            directory,
            name,
            Node::File {
                bytes: Vec::new(),
                mode,
                owner,
                group,
            },
        )?;
        self.describe(inode)
    }

    fn make_dir(
        &mut self,
        directory: u64,
        name: &str,
        mode: u32,
        owner: u32,
        group: u32,
    ) -> Result<(), Errno> {
        self.insert_child(
            directory,
            name,
            Node::Directory {
                children: BTreeMap::new(),
                mode,
                owner,
                group,
            },
        )?;
        Ok(())
    }

    fn unlink(&mut self, directory: u64, name: &str) -> Result<(), Errno> {
        let child = self.remove_child(directory, name)?;
        if matches!(self.nodes.get(&child), Some(Node::Directory { .. })) {
            // Put the name back: directories go through remove_dir.
            let parent = self
                .nodes
                .get_mut(&directory)
                .ok_or(Errno::from_i32(ENOENT))?;
            if let Node::Directory { children, .. } = parent {
                children.insert(name.as_bytes().to_vec(), child);
            }
            return Err(Errno::from_i32(ENOTDIR));
        }
        Ok(())
    }

    fn remove_dir(&mut self, directory: u64, name: &str) -> Result<(), Errno> {
        let child = self.remove_child(directory, name)?;
        match self.nodes.get(&child) {
            Some(Node::Directory { children, .. }) if children.is_empty() => {
                self.nodes.remove(&child);
                self.parents.remove(&child);
                Ok(())
            }
            Some(Node::Directory { .. }) => {
                // Restore the name: refusal must not lose the directory.
                let parent = self
                    .nodes
                    .get_mut(&directory)
                    .ok_or(Errno::from_i32(ENOENT))?;
                if let Node::Directory { children, .. } = parent {
                    children.insert(name.as_bytes().to_vec(), child);
                }
                Err(Errno::from_i32(ENOTEMPTY))
            }
            Some(_) => {
                let parent = self
                    .nodes
                    .get_mut(&directory)
                    .ok_or(Errno::from_i32(ENOENT))?;
                if let Node::Directory { children, .. } = parent {
                    children.insert(name.as_bytes().to_vec(), child);
                }
                Err(Errno::from_i32(ENOTDIR))
            }
            None => Err(Errno::from_i32(ENOENT)),
        }
    }

    fn rename(
        &mut self,
        old_directory: u64,
        old_name: &str,
        new_directory: u64,
        new_name: &str,
    ) -> Result<(), Errno> {
        let child = self.remove_child(old_directory, old_name)?;
        let parent = self
            .nodes
            .get_mut(&new_directory)
            .ok_or(Errno::from_i32(ENOENT))?;
        let Node::Directory { children, .. } = parent else {
            // Restore the old name before refusing.
            let old_parent = self
                .nodes
                .get_mut(&old_directory)
                .ok_or(Errno::from_i32(ENOENT))?;
            if let Node::Directory { children, .. } = old_parent {
                children.insert(old_name.as_bytes().to_vec(), child);
            }
            return Err(Errno::from_i32(ENOTDIR));
        };
        children.remove(new_name.as_bytes());
        children.insert(new_name.as_bytes().to_vec(), child);
        self.parents.insert(child, new_directory);
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
        self.insert_child(
            directory,
            name,
            Node::Symlink {
                target: target.to_vec(),
                owner,
                group,
            },
        )?;
        Ok(())
    }

    fn read_link(
        &mut self,
        inode: u64,
        capacity: usize,
        out: &mut dyn FnMut(&[u8]),
    ) -> Result<usize, Errno> {
        let node = self.nodes.get(&inode).ok_or(Errno::from_i32(ENOENT))?;
        let Node::Symlink { target, .. } = node else {
            return Err(Errno::from_i32(EINVAL));
        };
        let take = target.len().min(capacity);
        out(&target[..take]);
        Ok(take)
    }

    fn stat(&mut self, inode: u64, out: &mut [u8]) -> Result<(), Errno> {
        if out.len() < STAT_BUFFER_SIZE {
            return Err(Errno::from_i32(EINVAL));
        }
        let node = self.describe(inode)?;
        out[..8].copy_from_slice(&node.inode_number.to_le_bytes());
        out[8..12].copy_from_slice(&node.mode.to_le_bytes());
        out[12..20].copy_from_slice(&node.size.to_le_bytes());
        Ok(())
    }

    fn change_owner(&mut self, inode: u64, owner: u32, group: u32) -> Result<u32, Errno> {
        let node = self.nodes.get_mut(&inode).ok_or(Errno::from_i32(ENOENT))?;
        let mode = match node {
            Node::Directory {
                mode,
                owner: o,
                group: g,
                ..
            }
            | Node::File {
                mode,
                owner: o,
                group: g,
                ..
            } => {
                *o = owner;
                *g = group;
                *mode
            }
            Node::Symlink {
                owner: o, group: g, ..
            } => {
                *o = owner;
                *g = group;
                0o120777
            }
        };
        Ok(mode)
    }

    fn change_mode(&mut self, inode: u64, mode: u32) -> Result<u32, Errno> {
        let node = self.nodes.get_mut(&inode).ok_or(Errno::from_i32(ENOENT))?;
        match node {
            Node::Directory { mode: m, .. } | Node::File { mode: m, .. } => {
                // Preserve the file type bits; only permission bits change.
                *m = (*m & 0o170000) | (mode & 0o7777);
                Ok(*m)
            }
            Node::Symlink { .. } => Err(Errno::from_i32(ENOSYS)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::call::{NameInput, adapt_create, adapt_read_link};

    fn server_with_file() -> (MemFileServer, u64) {
        let mut server = MemFileServer::new();
        let node = server
            .create(ROOT_INODE, "note", 0o100644, 100, 100)
            .unwrap();
        (server, node.inode_number)
    }

    #[test]
    fn test_create_write_read_roundtrip() {
        let (mut server, inode) = server_with_file();
        let written = server.write(inode, 0, b"hello").unwrap();
        assert_eq!(written, 5);
        let mut seen = Vec::new();
        let moved = server
            .read(inode, 0, 5, &mut |chunk: &[u8]| {
                seen.extend_from_slice(chunk)
            })
            .unwrap();
        assert_eq!(moved, 5);
        assert_eq!(seen, b"hello");
        // Reading past the end stops at the file size.
        let mut tail = Vec::new();
        let moved = server
            .read(inode, 3, 10, &mut |chunk: &[u8]| {
                tail.extend_from_slice(chunk)
            })
            .unwrap();
        assert_eq!(moved, 2);
        assert_eq!(tail, b"lo");
    }

    #[test]
    fn test_lookup_dot_and_dot_dot() {
        let mut server = MemFileServer::new();
        server.make_dir(ROOT_INODE, "sub", 0o040755, 0, 0).unwrap();
        let (root, _) = server.lookup_child(ROOT_INODE, ".").unwrap();
        assert_eq!(root.inode_number, ROOT_INODE);
        let (sub, _) = server.lookup_child(ROOT_INODE, "sub").unwrap();
        let (back, _) = server.lookup_child(sub.inode_number, "..").unwrap();
        assert_eq!(back.inode_number, ROOT_INODE);
        // The root is its own parent.
        let (stays, _) = server.lookup_child(ROOT_INODE, "..").unwrap();
        assert_eq!(stays.inode_number, ROOT_INODE);
    }

    #[test]
    fn test_duplicate_create_is_rejected() {
        let mut server = MemFileServer::new();
        server.create(ROOT_INODE, "dup", 0o100644, 0, 0).unwrap();
        assert_eq!(
            server
                .create(ROOT_INODE, "dup", 0o100644, 0, 0)
                .unwrap_err()
                .to_i32(),
            EEXIST
        );
    }

    #[test]
    fn test_remove_dir_refuses_nonempty() {
        let mut server = MemFileServer::new();
        server.make_dir(ROOT_INODE, "dir", 0o040755, 0, 0).unwrap();
        let (dir, _) = server.lookup_child(ROOT_INODE, "dir").unwrap();
        server
            .create(dir.inode_number, "f", 0o100644, 0, 0)
            .unwrap();
        assert_eq!(
            server.remove_dir(ROOT_INODE, "dir").unwrap_err().to_i32(),
            ENOTEMPTY
        );
        // The directory survived the refusal.
        assert!(server.lookup_child(ROOT_INODE, "dir").is_ok());
        server.unlink(dir.inode_number, "f").unwrap();
        server.remove_dir(ROOT_INODE, "dir").unwrap();
        assert!(server.lookup_child(ROOT_INODE, "dir").is_err());
    }

    #[test]
    fn test_rename_moves_names() {
        let mut server = MemFileServer::new();
        server.create(ROOT_INODE, "old", 0o100644, 0, 0).unwrap();
        server.rename(ROOT_INODE, "old", ROOT_INODE, "new").unwrap();
        assert!(server.lookup_child(ROOT_INODE, "old").is_err());
        assert!(server.lookup_child(ROOT_INODE, "new").is_ok());
    }

    #[test]
    fn test_symlink_roundtrip() {
        let mut server = MemFileServer::new();
        server
            .symbolic_link(ROOT_INODE, "link", 0, 0, b"target")
            .unwrap();
        let (node, _) = server.lookup_child(ROOT_INODE, "link").unwrap();
        let mut seen = Vec::new();
        let moved = server
            .read_link(node.inode_number, 64, &mut |chunk: &[u8]| {
                seen.extend_from_slice(chunk)
            })
            .unwrap();
        assert_eq!(moved, 6);
        assert_eq!(seen, b"target");
    }

    #[test]
    fn test_stat_layout() {
        let (mut server, inode) = server_with_file();
        let mut buf = [0u8; STAT_BUFFER_SIZE];
        server.stat(inode, &mut buf).unwrap();
        assert_eq!(&buf[..8], &inode.to_le_bytes());
        assert_eq!(&buf[8..12], &0o100644u32.to_le_bytes());
        assert_eq!(&buf[12..20], &0i64.to_le_bytes());
        let mut short = [0u8; 4];
        assert_eq!(server.stat(inode, &mut short).unwrap_err().to_i32(), EINVAL);
    }

    #[test]
    fn test_full_request_path_through_adapters() {
        // Create through the adapter, then read the link target through the
        // adapter: exercises trait plus validation together.
        let mut server = MemFileServer::new();
        let node = adapt_create(
            &mut server,
            NameInput {
                directory: ROOT_INODE,
                name: "doc",
            },
            0o100644,
            0,
            0,
        )
        .unwrap();
        server.write(node.inode_number, 0, b"bytes").unwrap();
        server
            .symbolic_link(ROOT_INODE, "alias", 0, 0, b"doc")
            .unwrap();
        let (alias, _) = server.lookup_child(ROOT_INODE, "alias").unwrap();
        let mut seen = Vec::new();
        let moved = adapt_read_link(
            &mut server,
            alias.inode_number,
            64,
            &mut |chunk: &[u8]| seen.extend_from_slice(chunk),
        )
        .unwrap();
        assert_eq!(moved, 3);
        assert_eq!(seen, b"doc");
    }
}
