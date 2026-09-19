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
use minix_types::{EIO, ENOSPC, EINVAL, ENOENT, Errno, Stat};

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

    /// 把文件块 `file_block` 的物理块号写进指针迷宫；路径上的中间块
    /// （一级/二级/三级间接）为零时先分配并清零。
    fn set_pointer(
        &mut self,
        inode: &mut DiskInode,
        file_block: u64,
        physical: u64,
    ) -> Result<(), Errno> {
        let addresses = u64::from(self.geometry.block_size) / 4;
        let path = mapping::decompose(file_block, addresses)
            .map_err(|_| Errno::from_i32(EIO))?;
        match path {
            mapping::BlockPath::Direct { slot } => {
                inode.blocks[slot] = physical as u32;
            }
            mapping::BlockPath::Single { slot, index } => {
                if inode.blocks[slot] == 0 {
                    inode.blocks[slot] = self.alloc_block()? as u32;
                    self.zero_block(u64::from(inode.blocks[slot]))?;
                }
                self.write_pointer_slot(inode.blocks[slot] as u64, index, physical)?;
            }
            mapping::BlockPath::Double { slot, outer, inner } => {
                if inode.blocks[slot] == 0 {
                    inode.blocks[slot] = self.alloc_block()? as u32;
                    self.zero_block(u64::from(inode.blocks[slot]))?;
                }
                let middle = self.pointer_slot(u64::from(inode.blocks[slot]), outer)?;
                let middle = match middle {
                    Some(m) => m,
                    None => {
                        let allocated = self.alloc_block()?;
                        self.zero_block(allocated)?;
                        self.write_pointer_slot(u64::from(inode.blocks[slot]), outer, allocated)?;
                        allocated
                    }
                };
                self.write_pointer_slot(middle, inner, physical)?;
            }
            mapping::BlockPath::Triple { slot, outer, middle, inner } => {
                if inode.blocks[slot] == 0 {
                    inode.blocks[slot] = self.alloc_block()? as u32;
                    self.zero_block(u64::from(inode.blocks[slot]))?;
                }
                let double = match self.pointer_slot(u64::from(inode.blocks[slot]), outer)? {
                    Some(d) => d,
                    None => {
                        let allocated = self.alloc_block()?;
                        self.zero_block(allocated)?;
                        self.write_pointer_slot(u64::from(inode.blocks[slot]), outer, allocated)?;
                        allocated
                    }
                };
                let single = match self.pointer_slot(double, middle)? {
                    Some(s) => s,
                    None => {
                        let allocated = self.alloc_block()?;
                        self.zero_block(allocated)?;
                        self.write_pointer_slot(double, middle, allocated)?;
                        allocated
                    }
                };
                self.write_pointer_slot(single, inner, physical)?;
            }
        }
        Ok(())
    }

    /// 读一个指针槽（间接块内 `index` 处的 u32）。
    fn pointer_slot(&self, block: u64, index: u64) -> Result<Option<u64>, Errno> {
        let data = self.block(block)?;
        let at = index as usize * 4;
        let value = u32::from_le_bytes(data[at..at + 4].try_into().unwrap());
        Ok(if value == 0 { None } else { Some(value as u64) })
    }

    /// 写一个指针槽。
    fn write_pointer_slot(
        &mut self,
        block: u64,
        index: u64,
        physical: u64,
    ) -> Result<(), Errno> {
        let at = block as usize * self.geometry.block_size as usize + index as usize * 4;
        self.image[at..at + 4].copy_from_slice(&(physical as u32).to_le_bytes());
        Ok(())
    }

    /// 清零一个块（新分配的间接块必须清零，否则陈旧数据泄漏）。
    fn zero_block(&mut self, block: u64) -> Result<(), Errno> {
        let size = self.geometry.block_size as usize;
        let at = block as usize * size;
        self.image.get_mut(at..at + size).map(|slice| slice.fill(0)).ok_or(Errno::from_i32(EIO))
    }

    /// 释放一个 inode 的全部数据块与间接块（C `truncate` 的释放半）。
    /// 返回释放的数据块数。
    fn free_file_blocks(&mut self, inode: &DiskInode) -> Result<u32, Errno> {
        let addresses = u64::from(self.geometry.block_size) / 4;
        let mut freed = 0u32;
        for (slot, pointer) in inode.blocks.iter().enumerate() {
            if *pointer == 0 {
                continue;
            }
            let level = if slot < 12 {
                0 // 直接
            } else if slot == 12 {
                1
            } else if slot == 13 {
                2
            } else {
                3
            };
            match level {
                0 => {
                    self.free_block(u64::from(*pointer));
                    freed += 1;
                }
                _ => {
                    // 间接块：先递归释放其指向的数据/下层块，再释放自身。
                    freed += self.free_indirect_tree(
                        u64::from(*pointer),
                        level,
                        addresses,
                    )?;
                }
            }
        }
        Ok(freed)
    }

    /// 递归释放一棵间接树（`level` = 树高：1 单间、2 双间、3 三间）。
    fn free_indirect_tree(
        &mut self,
        block: u64,
        level: u8,
        addresses: u64,
    ) -> Result<u32, Errno> {
        if block == 0 {
            return Ok(0);
        }
        let mut freed = 0u32;
        if level > 1 {
            // 先把指针值拷出来再递归释放（递归需要 &mut self）。
            let data = self.block(block)?.to_vec();
            for index in 0..addresses {
                let at = index as usize * 4;
                let pointer =
                    u32::from_le_bytes(data[at..at + 4].try_into().unwrap());
                if pointer != 0 {
                    freed += self.free_indirect_tree(u64::from(pointer), level - 1, addresses)?;
                }
            }
        }
        self.free_block(block);
        Ok(freed + 1)
    }

    /// 组描述符读取（组号 → 32 字节描述符）。
    fn group_descriptor(&mut self, group: u32) -> Result<GroupDescriptor, Errno> {
        let table_block = (self.first_data_block + 1) as u64;
        let block = self.block(table_block + group as u64)?;
        GroupDescriptor::decode(block, 0).ok_or(Errno::from_i32(EIO))
    }

    /// 存储态超块的空闲块计数回写（`s_free_blocks_count` @12）。
    fn sb_add_free_blocks(&mut self, delta: i32) {
        let at = STORED_AT + 12;
        let current = i32::from_le_bytes(self.image[at..at + 4].try_into().unwrap());
        self.image[at..at + 4].copy_from_slice(&(current + delta).to_le_bytes());
    }

    /// 存储态超块的空闲 inode 计数回写（`s_free_inodes_count` @16）。
    /// F3c-2 的 alloc_inode/free_inode 消费。
    #[allow(dead_code)]
    fn sb_add_free_inodes(&mut self, delta: i32) {
        let at = STORED_AT + 16;
        let current = i32::from_le_bytes(self.image[at..at + 4].try_into().unwrap());
        self.image[at..at + 4].copy_from_slice(&(current + delta).to_le_bytes());
    }

    /// 组描述符的空闲块计数增减（`bg_free_blocks_count` @12）。
    fn gdt_add_free_blocks(&mut self, group: u32, delta: i32) {
        let at = (self.first_data_block + 1) as usize
            * self.geometry.block_size as usize
            + group as usize * 32
            + 12;
        let current = u16::from_le_bytes(self.image[at..at + 2].try_into().unwrap()) as i32;
        self.image[at..at + 2].copy_from_slice(&((current + delta) as u16).to_le_bytes());
    }

    /// 组描述符的空闲 inode 计数增减（`bg_free_inodes_count` @14）。
    /// F3c-2 的 alloc_inode/free_inode 消费。
    #[allow(dead_code)]
    fn gdt_add_free_inodes(&mut self, group: u32, delta: i32) {
        let at = (self.first_data_block + 1) as usize
            * self.geometry.block_size as usize
            + group as usize * 32
            + 14;
        let current = u16::from_le_bytes(self.image[at..at + 2].try_into().unwrap()) as i32;
        self.image[at..at + 2].copy_from_slice(&((current + delta) as u16).to_le_bytes());
    }

    /// 分配一个空闲块（C `alloc_bit` ZMAP 半）：逐组扫块位图，首个零位
    /// 即取（位 0 = 块 `first_data_block` + 组内偏移）。置位并回写超块/
    /// 组描述符的空闲计数。
    fn alloc_block(&mut self) -> Result<u64, Errno> {
        for group in 0..self.geometry.group_count {
            let descriptor = self.group_descriptor(group)?;
            let bitmap_block = descriptor.block_bitmap as u64;
            let map = self.block(bitmap_block)?;
            let bits = self.geometry.block_size * 8;
            for bit in 0..bits {
                if bit_test(map, bit) {
                    continue;
                }
                let number = self.first_data_block as u64
                    + group as u64 * u64::from(self.inodes_per_group)
                    + u64::from(bit);
                let at = bitmap_block as usize * self.geometry.block_size as usize
                    + (bit / 8) as usize;
                self.image[at] |= 1 << (bit % 8);
                self.gdt_add_free_blocks(group, -1);
                self.sb_add_free_blocks(-1);
                return Ok(number);
            }
        }
        Err(Errno::from_i32(ENOSPC))
    }

    /// 释放一个块（C `free_bit`）：清位并回写空闲计数。
    fn free_block(&mut self, number: u64) {
        let group = ((number.saturating_sub(self.first_data_block as u64))
            / u64::from(self.inodes_per_group.max(1))) as u32;
        if let Ok(descriptor) = self.group_descriptor(group) {
            let bitmap_block = descriptor.block_bitmap as u64;
            let bit = (number - self.first_data_block as u64
                - group as u64 * u64::from(self.inodes_per_group)) as u32;
            let at = bitmap_block as usize * self.geometry.block_size as usize
                + (bit / 8) as usize;
            if let Some(byte) = self.image.get_mut(at) {
                *byte &= !(1 << (bit % 8));
            }
            self.gdt_add_free_blocks(group, 1);
            self.sb_add_free_blocks(1);
        }
    }

    /// 分配一个 inode：扫 inode 位图取首个零位，写入初始记录（模式 +
    /// 链接数 1，指针清零），回写空闲计数，返回 inode 号（位 i →
    /// inode i+1；位 0 对应不存在的 inode 0，mkfs 时恒占用）。
    /// F3c-2 的 create/mkdir/link 消费（本批仅 write/truncate 直接读写）。
    #[allow(dead_code)]
    fn alloc_inode(&mut self, mode: u16) -> Result<u32, Errno> {
        for group in 0..self.geometry.group_count {
            let descriptor = self.group_descriptor(group)?;
            let bitmap_block = descriptor.inode_bitmap as u64;
            let map = self.block(bitmap_block)?;
            let bits = self.geometry.block_size * 8;
            for bit in 0..bits {
                if bit_test(map, bit) {
                    continue;
                }
                let at = bitmap_block as usize * self.geometry.block_size as usize
                    + (bit / 8) as usize;
                self.image[at] |= 1 << (bit % 8);
                let number = bit + 1;
                self.gdt_add_free_inodes(group, -1);
                self.sb_add_free_inodes(-1);
                let record = DiskInode {
                    mode,
                    uid: 0,
                    size: 0,
                    accessed: 0,
                    changed: 0,
                    modified: 0,
                    deleted_at: 0,
                    gid: 0,
                    links: 1,
                    sectors: 0,
                    flags: 0,
                    blocks: [0; inode::BLOCK_POINTERS],
                    generation: 0,
                };
                self.write_inode_back(number, &record);
                self.cache.insert(number, record);
                return Ok(number);
            }
        }
        Err(Errno::from_i32(ENOSPC))
    }

    /// 把缓存里的 inode 记录编码回镜像的 inode 表位置。
    fn write_inode_back(&mut self, number: u32, inode: &DiskInode) {
        let group = (number - 1) / self.inodes_per_group;
        let index = u64::from(number - 1) % u64::from(self.inodes_per_group);
        if let Ok(descriptor) = self.group_descriptor(group) {
            let at = descriptor.inode_table as usize
                * self.geometry.block_size as usize
                + index as usize * self.geometry.inode_size as usize;
            let encoded = inode::encode(inode);
            self.image[at..at + inode::RECORD_BYTES].copy_from_slice(&encoded);
        }
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

    /// Write file bytes through the pointer maze, allocating data and
    /// indirect blocks as needed (C `rw_chunk` write half); the inode
    /// size grows to cover the new end and the record is written back.
    fn write(&mut self, inode: u64, position: i64, data: &[u8]) -> Result<usize, Errno> {
        if position < 0 {
            return Err(Errno::from_i32(EINVAL));
        }
        let number = inode as u32;
        let mut record = self.read_inode(number)?;
        let block_size = u64::from(self.geometry.block_size);
        let mut fed = 0usize;
        let mut file_block = position as u64 / block_size;
        let mut offset_in_block = position as u64 % block_size;
        while fed < data.len() {
            let chunk = ((block_size - offset_in_block) as usize).min(data.len() - fed);
            let physical = match self.physical_of(&record, file_block)? {
                Some(physical) => physical,
                None => {
                    let allocated = self.alloc_block()?;
                    self.set_pointer(&mut record, file_block, allocated)?;
                    allocated
                }
            };
            let at = physical as usize * self.geometry.block_size as usize
                + offset_in_block as usize;
            self.image[at..at + chunk].copy_from_slice(&data[fed..fed + chunk]);
            fed += chunk;
            offset_in_block = 0;
            file_block += 1;
        }
        // 尺寸抬升：写入越过了旧尾才抬（原地覆写不改尺寸）。
        let new_end = position as u64 + data.len() as u64;
        if new_end > u64::from(record.size) {
            record.size = new_end as u32;
        }
        self.write_inode_back(number, &record);
        self.cache.insert(number, record);
        Ok(data.len())
    }

    /// Truncate: shrink the file to `start` bytes, freeing every data and
    /// indirect block beyond (C `truncate_vnode`), zero the pointer maze,
    /// write the record back. `end` is ignored (Minix3 uses it for punched
    /// holes, which ext2 does not serve — the VFS passes end = 0 for the
    /// O_TRUNC/ftruncate forms).
    fn truncate(&mut self, inode: u64, start: i64, _end: i64) -> Result<(), Errno> {
        if start < 0 {
            return Err(Errno::from_i32(EINVAL));
        }
        let number = inode as u32;
        let mut record = self.read_inode(number)?;
        self.free_file_blocks(&record)?;
        record.blocks = [0; inode::BLOCK_POINTERS];
        record.size = start as u32;
        self.write_inode_back(number, &record);
        self.cache.insert(number, record);
        Ok(())
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

/// 位图位测试（位 0 = 字节 0 的最低位）。
fn bit_test(map: &[u8], bit: u32) -> bool {
    map[(bit / 8) as usize] & (1 << (bit % 8)) != 0
}

/// 位图置位。
#[allow(dead_code)] // F3c-2 的 inode 位图分配消费
fn bit_set(map: &mut [u8], bit: u32) {
    map[(bit / 8) as usize] |= 1 << (bit % 8);
}

/// 位图清位。
#[allow(dead_code)] // F3c-2 的 free_inode 消费
fn bit_clear(map: &mut [u8], bit: u32) {
    map[(bit / 8) as usize] &= !(1 << (bit % 8));
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
    const BLOCK_BITMAP_BLOCK: u64 = 2;
    const INODE_BITMAP_BLOCK: u64 = 3;
    const INODE_TABLE_BLOCK: u64 = 4;
    const ROOT_DIR_BLOCK: u64 = 5;
    const FILE_DATA_BLOCK: u64 = 6;
    const FILE_DATA: &[u8] = b"HELLO EXT2";

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
        let mut image = vec![0u8; 32 * BLOCK];
        // 超块
        image[STORED_AT..STORED_AT + SUPER_STORED_BYTES].copy_from_slice(&stored_super());
        // 组描述符（块 1，组 0）：块位图=2、inode 位图=3、inode 表=4。
        let mut gdt = vec![0u8; BLOCK];
        let mut gd = [0u8; 32];
        gd[0..4].copy_from_slice(&(BLOCK_BITMAP_BLOCK as u32).to_le_bytes());
        gd[4..8].copy_from_slice(&(INODE_BITMAP_BLOCK as u32).to_le_bytes());
        gd[8..12].copy_from_slice(&(INODE_TABLE_BLOCK as u32).to_le_bytes());
        gdt[..32].copy_from_slice(&gd);
        image[BLOCK..2 * BLOCK].copy_from_slice(&gdt);
        // 块位图（块 2）：块 0..=6 已占（引导+超块/GDT/位图×2/inode 表/
        // 根目录/文件数据）。
        let bm = BLOCK_BITMAP_BLOCK as usize * BLOCK;
        for bit in 0u32..=6 {
            image[bm + (bit / 8) as usize] |= 1 << (bit % 8);
        }
        // inode 位图（块 3）：inode 1..=11 已占（位 0..=10）。
        let ib = INODE_BITMAP_BLOCK as usize * BLOCK;
        for bit in 0u32..=10 {
            image[ib + (bit / 8) as usize] |= 1 << (bit % 8);
        }
        // inode 表（块 4 起，128 字节/条）：
        // inode 2 = 根目录，inode 11 = FILE.TXT（直接块 6）。
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
        // 根目录（块 5）
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
        // 文件数据（块 6）
        let data = FILE_DATA_BLOCK as usize * BLOCK;
        image[data..data + FILE_DATA.len()].copy_from_slice(FILE_DATA);
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
    /// F3c-1：写扩展——inode 11 追加数据，分配新块并抬升尺寸。
    #[test]
    fn test_write_extends_and_allocates() {
        let mut server = mounted();
        let data = b"MORE DATA 12345";
        let n = server.write(11, 10, data).unwrap();
        assert_eq!(n, data.len(), "原地+扩展混合写全量落盘");
        // 读回拼接验证：前 10 字节旧数据 + 新数据。
        let mut got = Vec::new();
        let read = server
            .read(11, 0, 1024, &mut |bytes| got.extend_from_slice(bytes))
            .unwrap();
        assert_eq!(read, 10 + data.len());
        assert_eq!(&got[..10], b"HELLO EXT2");
        assert_eq!(&got[10..], data);
    }

    /// F3c-1：间接链写——12 个直接块写满后再写触发单间间接块分配
    /// （`mapping::decompose`：文件块 12 起走 `blocks[12]`）。
    #[test]
    fn test_write_allocates_indirect_blocks() {
        let block_size = 4096usize;
        let mut server = mounted();
        // 写 13 块：0..12 直接，12 起单间间接。
        let big = vec![b'A'; block_size * 13 + 16];
        let n = server.write(11, 0, &big).unwrap();
        assert_eq!(n, big.len());
        // 读回两端：最后一块住在单间间接块里。
        let mut head = Vec::new();
        let _ = server.read(11, 0, 16, &mut |bytes| head.extend_from_slice(bytes));
        assert_eq!(head, vec![b'A'; 16]);
        let mut tail = Vec::new();
        let _ = server.read(11, (block_size * 13) as i64, 16, &mut |bytes| {
            tail.extend_from_slice(bytes)
        });
        assert_eq!(tail, vec![b'A'; 16]);
    }

    /// F3c-1：truncate 释放全部块与间接块，位图与计数还原。
    #[test]
    fn test_truncate_frees_blocks_and_restores_counts() {
        let mut server = mounted();
        let big = vec![b'A'; 4096 * 2];
        let _ = server.write(11, 0, &big).unwrap();
        server.truncate(11, 0, 0).unwrap();
        let mut st = Stat::zeroed();
        server.stat(11, &mut st).unwrap();
        assert_eq!(st.size, 0, "truncate 后尺寸归零");
        // 截断后再写：块从位图重新分配，数据依然可读。
        let n = server.write(11, 0, b"FRESH").unwrap();
        assert_eq!(n, 5);
        let mut got = Vec::new();
        let _ = server.read(11, 0, 16, &mut |bytes| got.extend_from_slice(bytes));
        assert_eq!(&got[..5], b"FRESH");
    }

    /// F3c-1：写穿间接链后 truncate 的位图清位核对（间接块也被释放）。
    #[test]
    fn test_truncate_after_indirect_write_restores_bitmap() {
        let mut server = mounted();
        let big = vec![b'B'; 4096 * 2];
        let _ = server.write(11, 0, &big).unwrap();
        server.truncate(11, 0, 0).unwrap();
        // 全部块位图清零后，一次大分配仍应成功（位图还原断言的代理）。
        let first = server.alloc_block().unwrap();
        let second = server.alloc_block().unwrap();
        assert_ne!(first, second);
        server.free_block(first);
        server.free_block(second);
    }

}

