//! The ext2 read-only server: stored-superblock decode, inode reads,
//! block mapping over the image, and the [`FsDriver`] read-only half.
//!
//! C correspondence: the semantic halves of `super.c` (stored-superblock
//! read + mount gates), `inode.c` (`get_inode` — group math plus table
//! read), `rw_block`/`rw_chunk` (block walk through
//! [`crate::mapping::decompose`]), `link.c`/`misc.c` directory lookup,
//! and `getdents`/`stat`. The allocation policy half (balloc/ialloc,
//! placement.rs) and the write/link namespace operations are F3c — the
//! write-side trait methods keep their `ENOSYS` defaults here (C mounts
//! this shape read-only first, too).
//!
//! **Disc seam**: the server holds the whole image in memory (read-only
//! half of the device state for the hosted wave); the production
//! block-driver channel is the tracked E-FSBDEV seam. Full-disc-in-memory
//! is exact for a read-only mount: every read the driver issues resolves
//! to image bytes.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use minix_fs::data::DataChannel;
use minix_fs::dentry::{DentryEncoder, DirentType};
use alloc::vec;

use minix_fs::driver::FsDriver;
use minix_fs::protocol::{CapabilityFlags, FileNode, MountFlags};
use minix_types::{EIO, EINVAL, ENOENT, Errno, Stat};

use crate::dir;
use crate::inode::{self, DiskInode};
use crate::mapping;
use crate::superblock::{
    self, GroupDescriptor, ROOT_INODE_NUMBER, SUPER_OFFSET, SUPER_STORED_BYTES,
};

/// Superblock's stored size — the boot pad occupies block 0's first half.
const STORED_AT: usize = SUPER_OFFSET;

/// The read-only ext2 server over an in-memory image.
#[derive(Debug)]
pub struct Ext2Server {
    /// The whole filesystem image.
    image: Vec<u8>,
    /// Validated geometry.
    geometry: superblock::Geometry,
    /// Group-level constants needed for inode reads.
    inodes_per_group: u32,
    first_data_block: u32,
    /// Visited inodes: number → decoded record.
    cache: BTreeMap<u32, DiskInode>,
}

impl Ext2Server {
    /// Validate the image and build the server (C `mount` half of
    /// `super.c`: stored read → magic/feature gates → geometry).
    pub fn from_image(image: Vec<u8>) -> Result<Self, Errno> {
        if image.len() < STORED_AT + SUPER_STORED_BYTES {
            return Err(Errno::from_i32(EINVAL));
        }
        let raw = superblock::from_stored(&image[STORED_AT..STORED_AT + SUPER_STORED_BYTES]);
        let geometry = superblock::validate(&raw, 4096, true)
            .map_err(|_| Errno::from_i32(EINVAL))?;
        Ok(Self {
            image,
            geometry,
            inodes_per_group: raw.inodes_per_group,
            first_data_block: raw.first_data_block,
            cache: BTreeMap::new(),
        })
    }

    /// One filesystem block's bytes.
    fn block(&self, number: u64) -> Result<&[u8], Errno> {
        let size = self.geometry.block_size as usize;
        self.image
            .get(number as usize * size..number as usize * size + size)
            .ok_or(Errno::from_i32(EIO))
    }

    /// `get_inode` (`inode.c`): group math + group-descriptor table read +
    /// inode-table slice decode, memoized.
    fn read_inode(&mut self, number: u32) -> Result<DiskInode, Errno> {
        if let Some(hit) = self.cache.get(&number) {
            return Ok(*hit);
        }
        if number == 0 {
            return Err(Errno::from_i32(ENOENT));
        }
        let group = (number - 1) / self.inodes_per_group;
        let index = (number - 1) % self.inodes_per_group;
        // The descriptor table starts right after the superblock's block
        // (`first_data_block + 1`); one 32-byte descriptor per group.
        // 描述符表紧随超块块（`first_data_block + 1`），每组 32 字节；
        // 组内 inode 表块号在描述符 +8。
        let table_block = (self.first_data_block + 1) as u64;
        let descriptor_block = self.block(table_block + group as u64)?;
        let Some(descriptor) = GroupDescriptor::decode(descriptor_block, 0) else {
            return Err(Errno::from_i32(EIO));
        };
        let table_block_number = descriptor.inode_table as u64;
        let inode_bytes_at =
            table_block_number * self.geometry.block_size as u64
                + index as u64 * self.geometry.inode_size as u64;
        let window = self
            .image
            .get(
                inode_bytes_at as usize
                    ..inode_bytes_at as usize + inode::RECORD_BYTES,
            )
            .ok_or(Errno::from_i32(EIO))?;
        let inode = inode::decode(window).map_err(|_| Errno::from_i32(EIO))?;
        self.cache.insert(number, inode);
        Ok(*self.cache.get(&number).unwrap())
    }

    /// Resolve one file block number through the pointer maze
    /// (`rw_block`'s map half + `mapping::decompose`). A hole (zero
    /// pointer) reports `None`: the caller substitutes zeros.
    fn physical_of(
        &self,
        inode: &DiskInode,
        file_block: u64,
    ) -> Result<Option<u64>, Errno> {
        let addresses = (self.geometry.block_size / 4) as u64;
        let path =
            mapping::decompose(file_block, addresses).map_err(|_| Errno::from_i32(EIO))?;
        let read_slot = |block: u64, index: u64| -> Result<Option<u64>, Errno> {
            if block == 0 {
                return Ok(None);
            }
            let data = self.block(block)?;
            let at = index as usize * 4;
            let value = u32::from_le_bytes(data[at..at + 4].try_into().unwrap());
            Ok(if value == 0 { None } else { Some(value as u64) })
        };
        match path {
            mapping::BlockPath::Direct { slot } => {
                let value = inode.blocks[slot];
                Ok(if value == 0 { None } else { Some(value as u64) })
            }
            mapping::BlockPath::Single { slot, index } => {
                if inode.blocks[slot] == 0 {
                    return Ok(None);
                }
                read_slot(inode.blocks[slot] as u64, index)
            }
            mapping::BlockPath::Double { slot, outer, inner } => {
                if inode.blocks[slot] == 0 {
                    return Ok(None);
                }
                let Some(middle) = read_slot(inode.blocks[slot] as u64, outer)? else {
                    return Ok(None);
                };
                read_slot(middle, inner)
            }
            mapping::BlockPath::Triple { slot, outer, middle, inner } => {
                if inode.blocks[slot] == 0 {
                    return Ok(None);
                }
                let Some(double) = read_slot(inode.blocks[slot] as u64, outer)? else {
                    return Ok(None);
                };
                let Some(single) = read_slot(double, middle)? else {
                    return Ok(None);
                };
                read_slot(single, inner)
            }
        }
    }

    /// One file block's bytes, holes as zeros.
    fn file_block_bytes(
        &self,
        inode: &DiskInode,
        file_block: u64,
        feed: &mut dyn FnMut(&[u8]),
    ) -> Result<(), Errno> {
        let size = self.geometry.block_size as usize;
        match self.physical_of(inode, file_block)? {
            Some(physical) => {
                let bytes = self.block(physical)?;
                feed(bytes);
            }
            None => feed(&vec![0u8; size]),
        }
        Ok(())
    }

    fn file_node(number: u32, inode: &DiskInode) -> FileNode {
        FileNode {
            inode_number: number as u64,
            mode: inode.mode as u32,
            size: inode.size as i64,
            owner: inode.uid as u32,
            group: inode.gid as u32,
            device: 0,
        }
    }

    fn is_directory(inode: &DiskInode) -> bool {
        inode.mode as u32 & 0o170000 == 0o040000
    }

    /// Walk every data block of a directory, decoding its entries.
    /// `visit` receives each entry and returns `false` to stop. Deleted
    /// slots (`number == 0`) skip by their record length like C.
    fn walk_directory<F>(
        &mut self,
        directory: u32,
        mut visit: F,
    ) -> Result<(), Errno>
    where
        F: FnMut(dir::Entry) -> bool,
    {
        let inode = self.read_inode(directory)?;
        // 块数按 inode 尺寸向上取整；**块内**按 rec_len 步进（目录项不
        // 跨块，最后一条的 rec_len 吸收块内余量——遍历按块界走，不按
        // 尺寸逐字节截断）。
        let blocks = u64::from(inode.size).div_ceil(u64::from(self.geometry.block_size)) as usize;
        let mut stop = false;
        for file_block in 0..blocks {
            if stop {
                break;
            }
            let mut chunk = Vec::new();
            self.file_block_bytes(&inode, file_block as u64, &mut |bytes| {
                chunk.extend_from_slice(bytes);
            })?;
            let mut offset = 0usize;
            while offset + dir::HEADER_BYTES <= chunk.len() {
                let window = &chunk[offset..];
                let (entry, step) = match dir::decode(window) {
                    Ok(pair) => pair,
                    Err(_) => break,
                };
                offset += step;
                if entry.number != 0 && !visit(entry) {
                    stop = true;
                    break;
                }
            }
        }
        Ok(())
    }
}

impl FsDriver for Ext2Server {
    /// Mount: the image scan happened at construction; report the root.
    fn mount(
        &mut self,
        _device: u64,
        _flags: MountFlags,
        _capabilities: &mut CapabilityFlags,
    ) -> Result<FileNode, Errno> {
        let root = self.read_inode(ROOT_INODE_NUMBER)?;
        Ok(Self::file_node(ROOT_INODE_NUMBER, &root))
    }

    /// Resolve one child by exact name (`link.c` search half).
    fn lookup_child(
        &mut self,
        directory: u64,
        name: &str,
    ) -> Result<(FileNode, bool), Errno> {
        let mut found: Option<u32> = None;
        self.walk_directory(directory as u32, |entry| {
            if entry.name == name.as_bytes() {
                found = Some(entry.number);
                false
            } else {
                true
            }
        })?;
        let number = found.ok_or(Errno::from_i32(ENOENT))?;
        let child = self.read_inode(number)?;
        let directory = Self::is_directory(&child);
        Ok((Self::file_node(number, &child), directory))
    }

    /// Read file bytes block-by-block through the pointer maze, clamped
    /// to the inode size; holes read as zeros.
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
        let inode = self.read_inode(inode as u32)?;
        let size = inode.size as i64;
        let start = position.min(size);
        let end = size.min((position + length as i64).min(i64::MAX - size));
        if start >= end {
            return Ok(0);
        }
        let begin = start as u64;
        let count = (end - start) as usize;
        let block_size = self.geometry.block_size as u64;
        let mut fed = 0usize;
        let mut file_block = begin / block_size;
        let mut offset_in_block = (begin % block_size) as usize;
        while fed < count {
            let chunk = (block_size as usize - offset_in_block).min(count - fed);
            match self.physical_of(&inode, file_block)? {
                Some(physical) => {
                    let bytes = self.block(physical)?;
                    let from = offset_in_block;
                    out(&bytes[from..from + chunk]);
                }
                None => out(&vec![0u8; chunk]),
            }
            fed += chunk;
            offset_in_block = 0;
            file_block += 1;
        }
        Ok(count)
    }

    /// List a directory's entries through the shared dentry encoder; the
    /// position is the byte offset inside the directory data.
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
        let number = inode as u32;
        let inode = self.read_inode(number)?;
        let size = inode.size as usize;
        let block_size = self.geometry.block_size as usize;
        let mut cursor = *position as usize;
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
        let mut done = false;
        // 逐块遍历：inode 尺寸只定块数，**块内**按 rec_len 步进（最后
        // 一条的 rec_len 吸收块内余量，尺寸不逐字节截断遍历）。
        let blocks = u64::from(inode.size).div_ceil(block_size as u64) as usize;
        for block_index in 0..blocks {
            if done {
                break;
            }
            let block_number = inode.blocks[block_index] as u64;
            let block = self.block(block_number)?;
            let mut within = if block_index == 0 {
                cursor.min(block_size)
            } else {
                0
            };
            while within + dir::HEADER_BYTES <= block_size {
                let (entry, step) = match dir::decode(&block[within..block_size]) {
                    Ok(pair) => pair,
                    Err(_) => break,
                };
                if entry.number != 0 {
                    let kind = match entry.file_type {
                        dir::TYPE_DIRECTORY => DirentType::Directory,
                        dir::TYPE_REGULAR => DirentType::Regular,
                        dir::TYPE_SYMLINK => DirentType::Symlink,
                        _ => DirentType::Unknown,
                    };
                    if encoder
                        .add(entry.number as u64, &entry.name, kind)
                        .is_err()
                    {
                        done = true;
                        break;
                    }
                }
                within += step;
            }
            cursor = block_index * block_size + within;
        }
        let delivered = encoder.finish()?;
        if !done {
            *position = size as i64;
        } else {
            *position = cursor as i64;
        }
        Ok(delivered)
    }

    /// Status from the decoded inode.
    fn stat(&mut self, inode: u64, stat: &mut Stat) -> Result<(), Errno> {
        let record = self.read_inode(inode as u32)?;
        stat.mode = record.mode as u32;
        stat.inode = inode;
        stat.size = record.size as i64;
        stat.nlinks = record.links as u32;
        stat.owner = record.uid as u32;
        stat.group = record.gid as u32;
        Ok(())
    }
}

/// Bridges the trait's copy callback into the dentry encoder (F3a
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
    use super::superblock::RawSuperblock;
    use alloc::string::String;

    const BLOCK: usize = 4096;
    const INODE_TABLE_BLOCK: u64 = 4;
    const ROOT_DIR_BLOCK: u64 = 6;
    const FILE_DATA_BLOCK: u64 = 7;

    /// 盘上 1024 字节超块（旧 revision、单组、16 inode/组）。
    fn stored_super() -> [u8; SUPER_STORED_BYTES] {
        let raw = RawSuperblock {
            magic: superblock::MAGIC,
            log_block_size: 2,
            revision: superblock::REVISION_OLD,
            inode_count: 16,
            block_count: 16,
            reserved_count: 0,
            free_blocks: 0,
            free_inodes: 0,
            first_data_block: 0,
            blocks_per_group: 8192,
            inodes_per_group: 16,
            inode_size: 128,
            first_inode: 11,
            feature_compat: 0,
            feature_incompat: superblock::INCOMPAT_FILETYPE,
            feature_ro_compat: 0,
            state: superblock::STATE_CLEAN,
        };
        // 存储 encode 走同一偏移表（from_stored 的对偶）。
        let mut stored = [0u8; SUPER_STORED_BYTES];
        fn put32(stored: &mut [u8], at: usize, v: u32) {
            stored[at..at + 4].copy_from_slice(&v.to_le_bytes());
        }
        fn put16(stored: &mut [u8], at: usize, v: u16) {
            stored[at..at + 2].copy_from_slice(&v.to_le_bytes());
        }
        put32(&mut stored, 0, raw.inode_count);
        put32(&mut stored, 4, raw.block_count);
        put32(&mut stored, 8, raw.reserved_count);
        put32(&mut stored, 12, raw.free_blocks);
        put32(&mut stored, 16, raw.free_inodes);
        put32(&mut stored, 20, raw.first_data_block);
        put32(&mut stored, 24, raw.log_block_size);
        put32(&mut stored, 32, raw.blocks_per_group);
        put32(&mut stored, 40, raw.inodes_per_group);
        put16(&mut stored, 56, raw.magic);
        put16(&mut stored, 58, raw.state);
        put32(&mut stored, 76, raw.revision);
        put32(&mut stored, 84, raw.first_inode);
        put16(&mut stored, 88, raw.inode_size);
        put32(&mut stored, 92, raw.feature_compat);
        put32(&mut stored, 96, raw.feature_incompat);
        put32(&mut stored, 100, raw.feature_ro_compat);
        stored
    }

    /// 一条 ext2 目录项（dir::decode 可解，rec_len 含尾部 slack）。
    fn entry(number: u32, file_type: u8, name: &[u8], rec_len: u16) -> Vec<u8> {
        let mut e = vec![0u8; rec_len as usize];
        e[0..4].copy_from_slice(&number.to_le_bytes());
        e[4..6].copy_from_slice(&rec_len.to_le_bytes());
        e[6] = name.len() as u8;
        e[7] = file_type;
        e[8..8 + name.len()].copy_from_slice(name);
        e
    }

    /// 一个 DiskInode 的存储态（inode::encode 的输入）。
    fn inode_record(mode: u16, size: u32, blocks: [u32; 15]) -> [u8; 128] {
        let inode = DiskInode {
            mode,
            uid: 0,
            size,
            accessed: 0,
            changed: 0,
            modified: 0,
            deleted_at: 0,
            gid: 0,
            links: 1,
            sectors: 0,
            flags: 0,
            blocks,
            generation: 0,
        };
        inode::encode(&inode)
    }

    /// 构建最小 ext2 镜像：块 0 超块、块 1 组描述符、块 4~5 inode 表、
    /// 块 6 根目录、块 7 文件数据。
    fn build_image() -> Vec<u8> {
        let mut image = vec![0u8; 16 * BLOCK];
        // 超块
        image[STORED_AT..STORED_AT + SUPER_STORED_BYTES].copy_from_slice(&stored_super());
        // 组描述符（块 1，组 0）：inode 表在块 4
        let mut gdt = vec![0u8; BLOCK];
        let mut gd = [0u8; 32];
        gd[0..4].copy_from_slice(&3u32.to_le_bytes()); // block bitmap（占位）
        gd[4..8].copy_from_slice(&4u32.to_le_bytes()); // inode bitmap（占位）
        gd[8..12].copy_from_slice(&(INODE_TABLE_BLOCK as u32).to_le_bytes());
        gdt[..32].copy_from_slice(&gd);
        image[BLOCK..2 * BLOCK].copy_from_slice(&gdt);
        // inode 表（块 4 起，128 字节/条）：
        // inode 2 = 根目录，inode 11 = FILE.TXT（直接块 7）
        let mut inodes = vec![0u8; 2 * BLOCK];
        let root = inode_record(0o040755, 3 * 8, {
            let mut b = [0u32; 15];
            b[0] = ROOT_DIR_BLOCK as u32;
            b
        });
        inodes[128..256].copy_from_slice(&root);
        let file = inode_record(0o100644, 10, {
            let mut b = [0u32; 15];
            b[0] = FILE_DATA_BLOCK as u32;
            b
        });
        let at = (11 - 1) * 128;
        inodes[at..at + 128].copy_from_slice(&file);
        image[INODE_TABLE_BLOCK as usize * BLOCK..INODE_TABLE_BLOCK as usize * BLOCK + inodes.len()]
            .copy_from_slice(&inodes);
        // 根目录（块 6）
        let mut dir_data = vec![0u8; BLOCK];
        let mut at = 0usize;
        for e in [
            entry(2, 2, b".", 12),
            entry(2, 2, b"..", 12),
            entry(11, 1, b"FILE.TXT", BLOCK as u16 - 24),
        ] {
            dir_data[at..at + e.len()].copy_from_slice(&e);
            at += e.len();
        }
        image[ROOT_DIR_BLOCK as usize * BLOCK..ROOT_DIR_BLOCK as usize * BLOCK + BLOCK]
            .copy_from_slice(&dir_data);
        // 文件数据（块 7）
        image[FILE_DATA_BLOCK as usize * BLOCK..FILE_DATA_BLOCK as usize * BLOCK + 10]
            .copy_from_slice(b"HELLO EXT2");
        image
    }

    fn mounted() -> Ext2Server {
        let mut server = Ext2Server::from_image(build_image()).unwrap();
        let mut caps = CapabilityFlags::EMPTY;
        server.mount(0, MountFlags::EMPTY, &mut caps).unwrap();
        server
    }

    #[test]
    fn test_mount_rejects_bad_magic() {
        let mut image = build_image();
        image[STORED_AT + 56] = 0;
        image[STORED_AT + 57] = 0;
        assert_eq!(
            Ext2Server::from_image(image).unwrap_err().to_i32(),
            minix_types::EINVAL
        );
    }

    #[test]
    fn test_mount_reports_root() {
        let mut server = Ext2Server::from_image(build_image()).unwrap();
        let mut caps = CapabilityFlags::EMPTY;
        let root = server
            .mount(0, MountFlags::EMPTY, &mut caps)
            .unwrap();
        assert_eq!(root.inode_number, ROOT_INODE_NUMBER as u64);
        assert_eq!(root.mode & 0o170000, 0o040000, "根是目录");
        assert_eq!(root.size, 3 * 8, "根尺寸 = 三条目录项");
    }

    #[test]
    fn test_lookup_child_finds_file() {
        let mut server = mounted();
        let (file, is_dir) = server.lookup_child(ROOT_INODE_NUMBER as u64, "FILE.TXT").unwrap();
        assert!(!is_dir);
        assert_eq!(file.inode_number, 11);
        assert_eq!(file.size, 10);
        assert_eq!(file.mode & 0o170000, 0o100000);
        assert!(server.lookup_child(ROOT_INODE_NUMBER as u64, "NOPE").is_err());
    }

    #[test]
    fn test_read_walks_file_blocks() {
        let mut server = mounted();
        let mut got = Vec::new();
        let n = server
            .read(11, 0, 1024, &mut |bytes| got.extend_from_slice(bytes))
            .unwrap();
        assert_eq!(n, 10);
        assert_eq!(got, b"HELLO EXT2");
        // EOF 之后读到零。
        let n = server.read(11, 100, 8, &mut |_| {}).unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn test_getdents_lists_entries() {
        let mut server = mounted();
        let mut collected = Vec::new();
        let mut position = 0i64;
        let n = server
            .get_dents(ROOT_INODE_NUMBER as u64, &mut position, 4096, &mut |bytes| {
                collected.extend_from_slice(bytes);
            })
            .unwrap();
        assert!(n > 0);
        let blob = String::from_utf8_lossy(&collected).into_owned();
        assert!(blob.contains("FILE.TXT"), "位图含文件名: {blob}");
        assert!(position > 0, "位置推进到目录尾");
    }

    #[test]
    fn test_stat_reports_inode_fields() {
        let mut server = mounted();
        let mut st = Stat::zeroed();
        assert!(server.stat(11, &mut st).is_ok());
        assert_eq!(st.mode, 0o100644);
        assert_eq!(st.size, 10);
        assert_eq!(st.nlinks, 1);
    }
}

