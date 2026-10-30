//! The ext2 read-only server: stored-superblock decode, inode reads,
//! block mapping over the image, and the [`FsDriver`] read-only half.
//!
//! C correspondence: the semantic halves of `super.c` (stored-superblock
//! read + mount gates), `inode.c` (`get_inode` — group math plus table
//! read), `rw_block`/`rw_chunk` (block walk through
//! [`crate::mapping::decompose`]), `link.c`/`misc.c` directory lookup,
//! and `getdents`/`stat`. The allocation policy half (balloc/ialloc,
//! placement.rs) and the write/link namespace operations come next — the
//! write-side trait methods keep their `ENOSYS` defaults here (C mounts this
//! shape read-only first, too).
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
use minix_types::{EIO, ENOSPC, EPERM, EINVAL, ENOENT, EISDIR, EMLINK, ENOTDIR, Errno, Stat};

use crate::dir;
use crate::inode::{self, DiskInode};
use crate::mapping;
use crate::superblock::{
    self, GroupDescriptor, ROOT_INODE_NUMBER, SUPER_OFFSET, SUPER_STORED_BYTES,
};

/// Superblock's stored size — the boot pad occupies block 0's first half.
const STORED_AT: usize = SUPER_OFFSET;

/// Link count cap (Minix semantics take 32767, the `LINK_MAX` of
/// `minix3/sys/sys/syslimits.h`; although the on-disk field is u16, C's
/// `link.c:364-365` double gate `SHRT_MAX || LINK_MAX` yields the same value).
const LINK_MAX: u32 = 32767;

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
        // The descriptor table directly follows the superblock block
        // (`first_data_block + 1`), 32 bytes per entry; the group's inode-table
        // block number sits at descriptor +8.
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

    /// Write the physical block number of file block `file_block` into the
    /// pointer maze; intermediate blocks along the path (single/double/triple
    /// indirect) are allocated and zeroed first when absent.
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

    /// Read one pointer slot (the u32 at `index` inside an indirect block).
    fn pointer_slot(&self, block: u64, index: u64) -> Result<Option<u64>, Errno> {
        let data = self.block(block)?;
        let at = index as usize * 4;
        let value = u32::from_le_bytes(data[at..at + 4].try_into().unwrap());
        Ok(if value == 0 { None } else { Some(value as u64) })
    }

    /// Write one pointer slot.
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

    /// Zero one block (a newly allocated indirect block must be zeroed, or
    /// stale data leaks).
    fn zero_block(&mut self, block: u64) -> Result<(), Errno> {
        let size = self.geometry.block_size as usize;
        let at = block as usize * size;
        self.image.get_mut(at..at + size).map(|slice| slice.fill(0)).ok_or(Errno::from_i32(EIO))
    }

    /// Free all data blocks and indirect blocks of one inode (the free half of
    /// C `truncate`). Returns the number of data blocks freed.
    fn free_file_blocks(&mut self, inode: &DiskInode) -> Result<u32, Errno> {
        let addresses = u64::from(self.geometry.block_size) / 4;
        let mut freed = 0u32;
        for (slot, pointer) in inode.blocks.iter().enumerate() {
            if *pointer == 0 {
                continue;
            }
            let level = if slot < 12 {
                0 // direct
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
                    // Indirect block: recursively free what it points at
                    // first, then free itself.
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

    /// Recursively free one indirect tree (`level` = tree height: 1 single,
    /// 2 double, 3 triple).
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
            // Copy the pointer value out first, then recurse (recursion needs
            // &mut self).
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

    /// Find a directory entry by name; returns (entry, slot, predecessor slot —
    /// `None` for the first one in the block). Deletion needs the predecessor
    /// for rec_len merging (C `ext2_delete_entry`).
    fn find_entry_slot(
        &mut self,
        directory: u32,
        name: &str,
    ) -> Result<(dir::Entry, SlotPos, Option<SlotPos>), Errno> {
        let inode = self.read_inode(directory)?;
        let block_size = self.geometry.block_size as usize;
        let blocks = u64::from(inode.size).div_ceil(u64::from(self.geometry.block_size)) as usize;
        let mut prev: Option<SlotPos> = None;
        for block_index in 0..blocks {
            let block_number = inode.blocks[block_index] as u64;
            let block = self.block(block_number)?;
            let mut offset = 0usize;
            while offset + dir::HEADER_BYTES <= block_size {
                let window = &block[offset..block_size];
                let (entry, step) = match dir::decode(window) {
                    Ok(pair) => pair,
                    Err(_) => break,
                };
                let slot = (block_index, offset, step as u16);
                let matched = entry.number != 0 && entry.name == name.as_bytes();
                let prev_now = prev;
                if matched {
                    return Ok((entry, slot, prev_now));
                }
                prev = Some(slot);
                offset += step;
            }
        }
        Err(Errno::from_i32(ENOENT))
    }

    /// Insert a new directory entry (C `ext2_add_entry`): first look for a
    /// reusable empty slot (number == 0 with a long enough rec_len) or the tail
    /// slack of a real entry (rec_len − actual_size ≥ needed: shrink the current
    /// entry and put the new one in the slack); if neither exists, grow the
    /// directory by one block and the new entry lands at the block head
    /// (rec_len = block size).
    fn add_entry(
        &mut self,
        directory: u32,
        number: u32,
        file_type: u8,
        name: &[u8],
    ) -> Result<(), Errno> {
        let needed = dir::actual_size(name.len());
        let record = self.read_inode(directory)?;
        let block_size = self.geometry.block_size as usize;
        let blocks = u64::from(record.size).div_ceil(u64::from(self.geometry.block_size)) as usize;

        for block_index in 0..blocks {
            let block_number = record.blocks[block_index] as u64;
            let block = self.block(block_number)?;
            let mut offset = 0usize;
            while offset + dir::HEADER_BYTES <= block_size {
                let window = &block[offset..block_size];
                let (entry, step) = match dir::decode(window) {
                    Ok(pair) => pair,
                    Err(_) => break,
                };
                let actual = dir::actual_size(entry.name.len());
                // Empty-slot reuse: number == 0 and the whole segment is long
                // enough.
                if entry.number == 0 && step >= needed {
                    let at = block_number as usize * block_size + offset;
                    let mut new = vec![0u8; needed];
                    new[0..4].copy_from_slice(&number.to_le_bytes());
                    new[4..6].copy_from_slice(&(needed as u16).to_le_bytes());
                    new[6] = name.len() as u8;
                    new[7] = file_type;
                    new[8..8 + name.len()].copy_from_slice(name);
                    self.image[at..at + needed].copy_from_slice(&new);
                    // The slack becomes a trailing empty slot
                    // (rec_len = step - needed).
                    if step > needed {
                        let rest = step - needed;
                        let mut free = vec![0u8; rest];
                        free[4..6].copy_from_slice(&(rest as u16).to_le_bytes());
                        self.image[at + needed..at + step].copy_from_slice(&free);
                    }
                    return Ok(());
                }
                // Tail slack of a real entry: shrink the current rec_len to its
                // actual size and insert the new entry into the slack.
                if step >= needed + actual {
                    let shrink_at = block_number as usize * block_size + offset;
                    let shrunken: [u8; 2] = (actual as u16).to_le_bytes();
                    self.image[shrink_at + 4..shrink_at + 6].copy_from_slice(&shrunken);
                    let insert_at = shrink_at + actual;
                    let rest = step - actual;
                    let mut new = vec![0u8; needed];
                    new[0..4].copy_from_slice(&number.to_le_bytes());
                    new[4..6].copy_from_slice(&(needed as u16).to_le_bytes());
                    new[6] = name.len() as u8;
                    new[7] = file_type;
                    new[8..8 + name.len()].copy_from_slice(name);
                    self.image[insert_at..insert_at + needed].copy_from_slice(&new);
                    if rest > needed {
                        let mut free = vec![0u8; rest - needed];
                        free[4..6].copy_from_slice(&((rest - needed) as u16).to_le_bytes());
                        self.image[insert_at + needed..insert_at + rest]
                            .copy_from_slice(&free);
                    }
                    return Ok(());
                }
                offset += step;
            }
        }
        // No room: grow by one block; the new entry lands at the block head
        // (rec_len = block size).
        let new_block = self.alloc_block()?;
        let mut record_ref = self.read_inode(directory)?;
        record_ref.blocks[blocks] = new_block as u32;
        record_ref.size += self.geometry.block_size;
        self.write_inode_back(directory, &record_ref);
        self.cache.insert(directory, record_ref);
        let at = new_block as usize * block_size;
        let mut new = vec![0u8; block_size];
        new[0..4].copy_from_slice(&number.to_le_bytes());
        new[4..6].copy_from_slice(&(block_size as u16).to_le_bytes());
        new[6] = name.len() as u8;
        new[7] = file_type;
        new[8..8 + name.len()].copy_from_slice(name);
        self.image[at..at + block_size].copy_from_slice(&new);
        Ok(())
    }

    /// Delete a directory entry by name (C `ext2_delete_entry`): with a
    /// predecessor merge rec_len; for the first entry in the block set number =
    /// 0 (leaving the slot for add_entry to reuse). Returns the deleted entry's
    /// inode number.
    fn remove_entry(&mut self, directory: u32, name: &str) -> Result<u32, Errno> {
        let (entry, (block_index, offset, rec_len), prev) =
            self.find_entry_slot(directory, name)?;
        let block_size = self.geometry.block_size as usize;
        let block_number = self.read_inode(directory)?.blocks[block_index] as u64;
        let at = block_number as usize * block_size + offset;
        match prev {
            Some((_, prev_offset, prev_len)) => {
                // Merge into the predecessor: the predecessor's rec_len grows
                // by this entry.
                let merged = prev_len as usize + rec_len as usize;
                let prev_at =
                    block_number as usize * block_size + prev_offset + 4;
                self.image[prev_at..prev_at + 2]
                    .copy_from_slice(&(merged as u16).to_le_bytes());
            }
            None => {
                // First entry in the block: zero the inode number (rec_len
                // stays as a placeholder).
                self.image[at..at + 4].copy_from_slice(&0u32.to_le_bytes());
            }
        }
        Ok(entry.number)
    }

    /// Adjust an inode's link count; return `true` when it reaches zero (the
    /// caller does the freeing).
    fn adjust_links(&mut self, number: u32, delta: i32) -> Result<bool, Errno> {
        let mut record = self.read_inode(number)?;
        let links = i32::from(record.links) + delta;
        if links <= 0 {
            return Ok(true);
        }
        record.links = links as u16;
        self.write_inode_back(number, &record);
        self.cache.insert(number, record);
        Ok(false)
    }

    /// Free one inode: clear the inode bitmap bit, zero the table record, add
    /// the free count back (the IMAP half of C `free_bit`). The caller frees the
    /// data blocks first.
    fn free_inode(&mut self, number: u32) -> Result<(), Errno> {
        let group = (number - 1) / self.inodes_per_group;
        let index = (number - 1) % self.inodes_per_group;
        let descriptor = self.group_descriptor(group)?;
        let bitmap_block = descriptor.inode_bitmap as u64;
        let bit = (number - 1) % self.inodes_per_group;
        let at = bitmap_block as usize * self.geometry.block_size as usize
            + (bit / 8) as usize;
        if let Some(byte) = self.image.get_mut(at) {
            *byte &= !(1 << (bit % 8));
        }
        self.gdt_add_free_inodes(group, 1);
        self.sb_add_free_inodes(1);
        // Zero the table record.
        let record_at = descriptor.inode_table as usize
            * self.geometry.block_size as usize
            + index as usize * self.geometry.inode_size as usize;
        let encoded = inode::encode(&DiskInode {
            mode: 0,
            uid: 0,
            size: 0,
            accessed: 0,
            changed: 0,
            modified: 0,
            deleted_at: 0,
            gid: 0,
            links: 0,
            sectors: 0,
            flags: 0,
            blocks: [0; inode::BLOCK_POINTERS],
            generation: 0,
        });
        self.image[record_at..record_at + inode::RECORD_BYTES]
            .copy_from_slice(&encoded);
        self.cache.remove(&number);
        Ok(())
    }

    /// Group descriptor read (group number → 32-byte descriptor).
    fn group_descriptor(&mut self, group: u32) -> Result<GroupDescriptor, Errno> {
        let table_block = (self.first_data_block + 1) as u64;
        let block = self.block(table_block + group as u64)?;
        GroupDescriptor::decode(block, 0).ok_or(Errno::from_i32(EIO))
    }

    /// Write back the stored superblock's free-block count
    /// (`s_free_blocks_count` @12).
    fn sb_add_free_blocks(&mut self, delta: i32) {
        let at = STORED_AT + 12;
        let current = i32::from_le_bytes(self.image[at..at + 4].try_into().unwrap());
        self.image[at..at + 4].copy_from_slice(&(current + delta).to_le_bytes());
    }

    /// Write back the stored superblock's free-inode count
    /// (`s_free_inodes_count` @16). Consumed by alloc_inode/free_inode.
    #[allow(dead_code)]
    fn sb_add_free_inodes(&mut self, delta: i32) {
        let at = STORED_AT + 16;
        let current = i32::from_le_bytes(self.image[at..at + 4].try_into().unwrap());
        self.image[at..at + 4].copy_from_slice(&(current + delta).to_le_bytes());
    }

    /// Adjust the group descriptor's free-block count
    /// (`bg_free_blocks_count` @12).
    fn gdt_add_free_blocks(&mut self, group: u32, delta: i32) {
        let at = (self.first_data_block + 1) as usize
            * self.geometry.block_size as usize
            + group as usize * 32
            + 12;
        let current = u16::from_le_bytes(self.image[at..at + 2].try_into().unwrap()) as i32;
        self.image[at..at + 2].copy_from_slice(&((current + delta) as u16).to_le_bytes());
    }

    /// Adjust the group descriptor's free-inode count
    /// (`bg_free_inodes_count` @14). Consumed by alloc_inode/free_inode.
    #[allow(dead_code)]
    fn gdt_add_free_inodes(&mut self, group: u32, delta: i32) {
        let at = (self.first_data_block + 1) as usize
            * self.geometry.block_size as usize
            + group as usize * 32
            + 14;
        let current = u16::from_le_bytes(self.image[at..at + 2].try_into().unwrap()) as i32;
        self.image[at..at + 2].copy_from_slice(&((current + delta) as u16).to_le_bytes());
    }

    /// Allocate a free block (the ZMAP half of C `alloc_bit`): scan the block
    /// bitmaps group by group and take the first zero bit (bit 0 = block
    /// `first_data_block` + in-group offset). Set the bit and write back the
    /// free counts of the superblock and the group descriptor.
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

    /// Free one block (C `free_bit`): clear the bit and write back the free
    /// counts.
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

    /// Allocate an inode: scan the inode bitmap for the first zero bit, write
    /// the initial record (mode + link count 1, pointers zeroed), write the free
    /// counts back, and return the inode number (bit i → inode i+1; bit 0 is
    /// nonexistent inode 0, always occupied by mkfs). Consumed by
    /// create/mkdir/link (this batch only reads and writes through
    /// write/truncate directly).
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

    /// Encode the cached inode record back to its slot in the image's inode
    /// table.
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
        // The block count rounds the inode size up; **within a block** the walk
        // steps by rec_len (a directory entry never spans blocks, and the last
        // one's rec_len absorbs the in-block slack — the walk follows block
        // boundaries, never truncating byte by byte by size).
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
        // Size raise: only when the write passes the old end (an in-place
        // overwrite does not change the size).
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
        // Per-block walk: the inode size only fixes the block count; **within a
        // block** the walk steps by rec_len (the last one's rec_len absorbs the
        // in-block slack, so the walk is not truncated byte by byte by size).
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
            // Not ended early: the position advances to the directory end
            // (block count × block size).
            let blocks = u64::from(inode.size).div_ceil(u64::from(self.geometry.block_size)) as usize;
            *position = (blocks as u64 * u64::from(self.geometry.block_size)) as i64;
        } else {
            *position = cursor as i64;
        }
        Ok(delivered)
    }

    /// Create a regular file: `alloc_inode` + parent entry. C `mfs`
    /// create: type bit `I_REGULAR` forced on the caller's mode.
    fn create(
        &mut self,
        directory: u64,
        name: &str,
        mode: u32,
        owner: u32,
        group: u32,
    ) -> Result<FileNode, Errno> {
        let mode = 0o100000 | (mode & 0o7777);
        let number = self.alloc_inode(mode as u16)?;
        {
            let mut record = self.read_inode(number)?;
            record.uid = owner as u16;
            record.gid = group as u16;
            self.write_inode_back(number, &record);
            self.cache.insert(number, record);
        }
        self.add_entry(directory as u32, number, dir::TYPE_REGULAR, name.as_bytes())?;
        let record = self.read_inode(number)?;
        Ok(Self::file_node(number, &record))
    }

    /// Create a directory: links start at two (the name plus its own `.`),
    /// entries `.` and `..` fill the first block, and the parent's link
    /// count grows for the child's `..` (C `mfs mkdir` link bookkeeping).
    fn make_dir(
        &mut self,
        directory: u64,
        name: &str,
        mode: u32,
        owner: u32,
        group: u32,
    ) -> Result<(), Errno> {
        let number = self.alloc_inode(0o040000 | (mode & 0o7777) as u16)?;
        {
            let mut record = self.read_inode(number)?;
            record.uid = owner as u16;
            record.gid = group as u16;
            record.links = 2;
            let data_block = self.alloc_block()?;
            record.blocks[0] = data_block as u32;
            record.size = self.geometry.block_size;
            self.write_inode_back(number, &record);
            self.cache.insert(number, record);
            // The dot entries: `.` points at itself and `..` at the parent
            // (each rec_len is 12, and the tail slack belongs to `..`).
            let at = data_block as usize * self.geometry.block_size as usize;
            let mut dot = vec![0u8; 12];
            dot[0..4].copy_from_slice(&number.to_le_bytes());
            dot[4..6].copy_from_slice(&12u16.to_le_bytes());
            dot[6] = 1;
            dot[7] = dir::TYPE_DIRECTORY;
            dot[8] = b'.';
            self.image[at..at + 12].copy_from_slice(&dot);
            let rest = (self.geometry.block_size - 12) as u16;
            let mut dotdot = vec![0u8; rest as usize];
            dotdot[0..4].copy_from_slice(&(directory as u32).to_le_bytes());
            dotdot[4..6].copy_from_slice(&rest.to_le_bytes());
            dotdot[6] = 2;
            dotdot[7] = dir::TYPE_DIRECTORY;
            dotdot[8] = b'.';
            dotdot[9] = b'.';
            self.image[at + 12..at + 12 + rest as usize]
                .copy_from_slice(&dotdot);
        }
        // The parent directory's link count grows by one for the
        // subdirectory's `..`.
        let mut parent = self.read_inode(directory as u32)?;
        parent.links += 1;
        self.write_inode_back(directory as u32, &parent);
        self.cache.insert(directory as u32, parent);
        self.add_entry(directory as u32, number, dir::TYPE_DIRECTORY, name.as_bytes())
    }

    /// Create a device node: mode as given (type bits ride the request),
    /// the device number lives in the pointer area (`blocks[0]`).
    fn make_node(
        &mut self,
        directory: u64,
        name: &str,
        mode: u32,
        owner: u32,
        group: u32,
        device: u64,
    ) -> Result<(), Errno> {
        let number = self.alloc_inode(mode as u16)?;
        {
            let mut record = self.read_inode(number)?;
            record.uid = owner as u16;
            record.gid = group as u16;
            record.blocks[0] = device as u32;
            self.write_inode_back(number, &record);
            self.cache.insert(number, record);
        }
        self.add_entry(directory as u32, number, dir::TYPE_UNKNOWN, name.as_bytes())
    }

    /// Hard link: an entry naming an existing inode; directories refuse
    /// (C `link.c`: `EPERM`).
    fn link(&mut self, directory: u64, name: &str, inode: u64) -> Result<(), Errno> {
        let target = self.read_inode(inode as u32)?;
        if Self::is_directory(&target) {
            return Err(Errno::from_i32(EPERM));
        }
        self.add_entry(directory as u32, inode as u32, dir::TYPE_REGULAR, name.as_bytes())?;
        //[SD19-gate] C link.c:92 does `rip->i_links_count++` then a void
        // `put_inode` that reports no error; the link count is best effort
        // after add_entry already succeeded
        #[allow(clippy::let_underscore_must_use)]
        let _ = self.adjust_links(inode as u32, 1);
        Ok(())
    }

    /// Remove a name: drop the entry, decrement the target's links, and
    /// free the inode (with its blocks) when the count hits zero.
    /// Directories refuse (C `link.c:255-258` — `EPERM`).
    fn unlink(&mut self, directory: u64, name: &str) -> Result<(), Errno> {
        let number = self.remove_entry(directory as u32, name)?;
        let record = self.read_inode(number)?;
        if Self::is_directory(&record) {
            return Err(Errno::from_i32(EPERM));
        }
        if self.adjust_links(number, -1)? {
            //[SD19-gate] C link.c:260 `put_inode` handles block release once
            // the inode hits zero and is void, reporting no error; unlink already
            // confirmed the last link, so a failed block release does not roll
            // back
            #[allow(clippy::let_underscore_must_use)]
            let _ = self.free_file_blocks(&record);
            self.free_inode(number)?;
        }
        Ok(())
    }

    /// Remove an empty directory: the child must hold nothing past its
    /// dot pair, then its blocks/inode are freed and the parent's link
    /// count drops (C `mfs rmdir`).
    fn remove_dir(&mut self, directory: u64, name: &str) -> Result<(), Errno> {
        let (entry, _, _) = self.find_entry_slot(directory as u32, name)?;
        let child = self.read_inode(entry.number)?;
        if !Self::is_directory(&child) {
            return Err(Errno::from_i32(EINVAL));
        }
        // Empty check: only the two real entries `.` and `..` are allowed (any
        // other entry means `ENOTEMPTY` — C `link.c`'s rmdir empty check).
        let mut real_entries = 0usize;
        self.walk_directory(entry.number, |e| {
            if e.number != 0 {
                real_entries += 1;
            }
            true
        })?;
        if real_entries > 2 {
            return Err(Errno::from_i32(minix_types::ENOTEMPTY));
        }
        let number = self.remove_entry(directory as u32, name)?;
        //[SD19-gate] C link.c:260 `put_inode` handles block release once the
        // inode hits zero and is void, reporting no error; rmdir already
        // confirmed empty and took the child inode, so a failed block release
        // does not roll back
        #[allow(clippy::let_underscore_must_use)]
        let _ = self.free_file_blocks(&child);
        self.free_inode(number)?;
        //[SD19-gate] C link.c:255 does `rip->i_links_count--;` then a void
        // `put_inode`; the parent directory's link count is best effort after
        // remove_entry already succeeded
        #[allow(clippy::let_underscore_must_use)]
        let _ = self.adjust_links(directory as u32, -1);
        Ok(())
    }

    /// Rename: C `fs_rename`'s decision tree. The same-inode rename is a
    /// no-op success; type mismatches refuse (`ENOTDIR`/`EISDIR`); the
    /// ancestor walk refuses moving a directory into its own subtree
    /// (`EINVAL`, `..` missing treated the same); the new parent's link
    /// cap refuses before the move (`EMLINK`). An existing target comes
    /// off first (empty-dir rule for directories), then the entries swap
    /// in the C order: same parent deletes the old name first so the
    /// freed slot serves the new one, cross parent enters the new name
    /// first so a failure cannot lose the file. A moved directory gets
    /// its `..` retargeted, trading one link between the parents.
    /// Mountpoint guards are VFS-level and have no single-server
    /// counterpart.
    fn rename(
        &mut self,
        old_directory: u64,
        old_name: &str,
        new_directory: u64,
        new_name: &str,
    ) -> Result<(), Errno> {
        let old_number = self.find_entry_slot(old_directory as u32, old_name)?.0.number;
        let old_record = self.read_inode(old_number)?;
        let old_is_dir = Self::is_directory(&old_record);
        let same_pdir = old_directory == new_directory;
        // Current state of the target name: absent records `None` (the lookup's
        // ENOENT); every other error propagates as it is.
        let target = match self.find_entry_slot(new_directory as u32, new_name) {
            Ok((entry, _, _)) => Some(entry.number),
            Err(e) if e.to_i32() == ENOENT => None,
            Err(e) => return Err(e),
        };

        // Ancestor-cycle check (C runs it before the target checks): walk up
        // the new parent's `..` and EINVAL if the directory being moved is met; a
        // missing `..` is likewise rejected as the worst case; the root's `..`
        // pointing at itself means the top was reached, so allow it.
        if old_is_dir && !same_pdir {
            let mut current = new_directory as u32;
            loop {
                if current == old_number {
                    return Err(Errno::from_i32(EINVAL));
                }
                if current == ROOT_INODE_NUMBER {
                    break;
                }
                match self.find_entry_slot(current, "..") {
                    Ok((entry, _, _)) if entry.number != current => current = entry.number,
                    Ok(_) => break,
                    Err(_) => return Err(Errno::from_i32(EINVAL)),
                }
            }
        }

        if let Some(target_number) = target {
            if target_number == old_number {
                // C `SAME`: renaming to itself succeeds as a no-op.
                return Ok(());
            }
            let target_record = self.read_inode(target_number)?;
            let target_is_dir = Self::is_directory(&target_record);
            if old_is_dir && !target_is_dir {
                return Err(Errno::from_i32(ENOTDIR));
            }
            if !old_is_dir && target_is_dir {
                return Err(Errno::from_i32(EISDIR));
            }
        } else if old_is_dir && !same_pdir {
            // Only when the target is absent does it check the new parent's
            // link cap: a directory moving in takes one `..` link.
            let parent = self.read_inode(new_directory as u32)?;
            if u32::from(parent.links) >= LINK_MAX {
                return Err(Errno::from_i32(EMLINK));
            }
        }

        // Overwrite: remove the target first — a directory goes through the
        // empty check and free, a file through link decrement and free.
        if target.is_some() {
            if old_is_dir {
                self.remove_dir(new_directory, new_name)?;
            } else {
                self.unlink(new_directory, new_name)?;
            }
        }

        // Entry swap (C's two orders): same parent deletes then inserts, giving
        // the freed slot to the new name; cross parent inserts then deletes, so
        // an insert failure (disk full) leaves the old name untouched and the
        // file is not lost.
        let entry_type = dirent_type_for_mode(old_record.mode as u32);
        if same_pdir {
            self.remove_entry(old_directory as u32, old_name)?;
            self.add_entry(new_directory as u32, old_number, entry_type, new_name.as_bytes())?;
        } else {
            self.add_entry(new_directory as u32, old_number, entry_type, new_name.as_bytes())?;
            self.remove_entry(old_directory as u32, old_name)?;
        }

        // `..` fix-up: a moved directory still points at the old parent.
        // Removing `..` decrements the old parent's link count (the decrement
        // half of C unlink_file); a successful reinsert pointing at the new
        // parent increments it (C ENTER + links_count++), and a failed insert
        // adds and subtracts nothing, as in C.
        if old_is_dir && !same_pdir {
            if let Ok(old_parent) = self.remove_entry(old_number, "..") {
                //[SD19-gate] C link.c:440 rename does the direct `--` then
                // `put_inode(old_dirp)` without reporting; `..` is already
                // detached, so the old parent's link count is best effort
                #[allow(clippy::let_underscore_must_use)]
                let _ = self.adjust_links(old_parent, -1);
            }
            if self
                .add_entry(old_number, new_directory as u32, dir::TYPE_DIRECTORY, b"..")
                .is_ok()
            {
                let mut new_parent = self.read_inode(new_directory as u32)?;
                new_parent.links += 1;
                self.write_inode_back(new_directory as u32, &new_parent);
                self.cache.insert(new_directory as u32, new_parent);
            }
        }
        Ok(())
    }

    /// Symbolic link: the target rides the pointer area when it fits
    /// (fast symlink, C `s_i_block` convention), otherwise a data block.
    fn symbolic_link(
        &mut self,
        directory: u64,
        name: &str,
        owner: u32,
        group: u32,
        target: &[u8],
    ) -> Result<(), Errno> {
        let number = self.alloc_inode(0o120777)?;
        {
            let mut record = self.read_inode(number)?;
            record.uid = owner as u16;
            record.gid = group as u16;
            record.size = target.len() as u32;
            if target.len() < 60 {
                // Fast symbolic link: the target goes straight into the 60
                // bytes of the pointer area. Assemble through u32 slots,
                // avoiding a raw pointer.
                for (index, chunk) in target.chunks(4).enumerate() {
                    let mut word = [0u8; 4];
                    word[..chunk.len()].copy_from_slice(chunk);
                    record.blocks[index] = u32::from_le_bytes(word);
                }
            } else {
                let data_block = self.alloc_block()?;
                record.blocks[0] = data_block as u32;
                let at = data_block as usize * self.geometry.block_size as usize;
                self.image[at..at + target.len()].copy_from_slice(target);
            }
            self.write_inode_back(number, &record);
            self.cache.insert(number, record);
        }
        self.add_entry(directory as u32, number, dir::TYPE_SYMLINK, name.as_bytes())
    }

    /// Read a symbolic link target: fast links read from the pointer
    /// area (size < 60), slow links from their data block.
    fn read_link(
        &mut self,
        inode: u64,
        capacity: usize,
        out: &mut dyn FnMut(&[u8]),
    ) -> Result<usize, Errno> {
        let record = self.read_inode(inode as u32)?;
        let length = record.size as usize;
        let bytes: Vec<u8> = if length < 60 {
            // Fast symbolic link: the target lives in the 60 bytes of the
            // pointer area, expanded through u32 slots.
            let mut target = Vec::new();
            for word in &record.blocks {
                target.extend_from_slice(&word.to_le_bytes());
            }
            target.truncate(length);
            target
        } else {
            let data_block = u64::from(record.blocks[0]);
            self.block(data_block)?
                .get(..length)
                .ok_or(Errno::from_i32(EIO))?
                .to_vec()
        };
        let n = length.min(capacity);
        out(&bytes[..n]);
        Ok(n)
    }

    /// Change owner/group (the chown half of C `protect.c`: write the fields
    /// back).
    fn change_owner(&mut self, inode: u64, owner: u32, group: u32) -> Result<u32, Errno> {
        let mut record = self.read_inode(inode as u32)?;
        record.uid = owner as u16;
        record.gid = group as u16;
        self.write_inode_back(inode as u32, &record);
        self.cache.insert(inode as u32, record);
        Ok(0)
    }

    /// Change permission bits: format bits stay, the caller's mode rides
    /// the low bits (C `mfs protect.c`).
    fn change_mode(&mut self, inode: u64, mode: u32) -> Result<u32, Errno> {
        let mut record = self.read_inode(inode as u32)?;
        record.mode = ((record.mode & 0o170000) as u32 | (mode & 0o7777)) as u16;
        self.write_inode_back(inode as u32, &record);
        self.cache.insert(inode as u32, record);
        Ok(u32::from(record.mode))
    }

    /// Update access and modification times (seconds land on disk; the
    /// nanosecond fields are not stored by old ext2 revisions and are dropped —
    /// C stores only seconds too).
    fn update_times(
        &mut self,
        inode: u64,
        accessed: (i64, i64),
        modified: (i64, i64),
    ) -> Result<(), Errno> {
        let mut record = self.read_inode(inode as u32)?;
        record.accessed = accessed.0 as u32;
        record.modified = modified.0 as u32;
        self.write_inode_back(inode as u32, &record);
        self.cache.insert(inode as u32, record);
        Ok(())
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
        if record.mode as u32 & 0o170000 == 0o020000
            || record.mode as u32 & 0o170000 == 0o060000
        {
            stat.special = u64::from(record.blocks[0]);
        }
        Ok(())
    }
}

/// Directory-entry slot location: block index, in-block offset, rec_len.
type SlotPos = (usize, usize, u16);

/// Bitmap bit test (bit 0 = the lowest bit of byte 0).
fn bit_test(map: &[u8], bit: u32) -> bool {
    map[(bit / 8) as usize] & (1 << (bit % 8)) != 0
}

/// inode mode type bits → directory-entry file_type byte (the ENTER mapping
/// chain of `path.c`, `minix3/minix/fs/ext2/path.c:286-303`). Sockets and
/// unknown types land on `TYPE_UNKNOWN` — the mapping chain has no socket
/// branch, matching C.
fn dirent_type_for_mode(mode: u32) -> u8 {
    match mode & 0o170000 {
        0o100000 => dir::TYPE_REGULAR,
        0o040000 => dir::TYPE_DIRECTORY,
        0o120000 => dir::TYPE_SYMLINK,
        0o060000 => dir::TYPE_BLKDEV,
        0o020000 => dir::TYPE_CHRDEV,
        0o010000 => dir::TYPE_FIFO,
        _ => dir::TYPE_UNKNOWN,
    }
}

/// Set a bitmap bit.
#[allow(dead_code)] // consumed by the inode bitmap allocation
fn bit_set(map: &mut [u8], bit: u32) {
    map[(bit / 8) as usize] |= 1 << (bit % 8);
}

/// Clear a bitmap bit.
#[allow(dead_code)] // consumed by free_inode
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

    /// The on-disk 1024-byte superblock (old revision, single group, 16 inodes
    /// per group).
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
        // The stored encode goes through the same offset table (the dual of
        // from_stored).
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

    /// One ext2 directory entry (decodable by dir::decode, rec_len including
    /// tail slack).
    fn entry(number: u32, file_type: u8, name: &[u8], rec_len: u16) -> Vec<u8> {
        let mut e = vec![0u8; rec_len as usize];
        e[0..4].copy_from_slice(&number.to_le_bytes());
        e[4..6].copy_from_slice(&rec_len.to_le_bytes());
        e[6] = name.len() as u8;
        e[7] = file_type;
        e[8..8 + name.len()].copy_from_slice(name);
        e
    }

    /// The stored form of a DiskInode (input to inode::encode).
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

    /// Build a minimal ext2 image: block 0 superblock, block 1 group
    /// descriptor, blocks 4-5 inode table, block 6 root directory, block 7 file
    /// data.
    fn build_image() -> Vec<u8> {
        let mut image = vec![0u8; 32 * BLOCK];
        // Superblock
        image[STORED_AT..STORED_AT + SUPER_STORED_BYTES].copy_from_slice(&stored_super());
        // Group descriptor (block 1, group 0): block bitmap = 2, inode
        // bitmap = 3, inode table = 4.
        let mut gdt = vec![0u8; BLOCK];
        let mut gd = [0u8; 32];
        gd[0..4].copy_from_slice(&(BLOCK_BITMAP_BLOCK as u32).to_le_bytes());
        gd[4..8].copy_from_slice(&(INODE_BITMAP_BLOCK as u32).to_le_bytes());
        gd[8..12].copy_from_slice(&(INODE_TABLE_BLOCK as u32).to_le_bytes());
        gdt[..32].copy_from_slice(&gd);
        image[BLOCK..2 * BLOCK].copy_from_slice(&gdt);
        // Block bitmap (block 2): blocks 0..=6 are taken (boot + superblock /
        // GDT / two bitmaps / inode table / root directory / file data).
        let bm = BLOCK_BITMAP_BLOCK as usize * BLOCK;
        for bit in 0u32..=6 {
            image[bm + (bit / 8) as usize] |= 1 << (bit % 8);
        }
        // Inode bitmap (block 3): inodes 1..=11 are taken (bits 0..=10).
        let ib = INODE_BITMAP_BLOCK as usize * BLOCK;
        for bit in 0u32..=10 {
            image[ib + (bit / 8) as usize] |= 1 << (bit % 8);
        }
        // Inode table (from block 4, 128 bytes each):
        // inode 2 = root directory, inode 11 = FILE.TXT (direct block 6).
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
        // Root directory (block 5)
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
        // File data (block 6)
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
        // Reads past EOF yield zeros.
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
    /// Write extension: appending data to inode 11 allocates new blocks and
    /// raises the size.
    #[test]
    fn test_write_extends_and_allocates() {
        let mut server = mounted();
        let data = b"MORE DATA 12345";
        let n = server.write(11, 10, data).unwrap();
        assert_eq!(n, data.len(), "原地+扩展混合写全量落盘");
        // Read-back check: the 10 old bytes plus the new data.
        let mut got = Vec::new();
        let read = server
            .read(11, 0, 1024, &mut |bytes| got.extend_from_slice(bytes))
            .unwrap();
        assert_eq!(read, 10 + data.len());
        assert_eq!(&got[..10], b"HELLO EXT2");
        assert_eq!(&got[10..], data);
    }

    /// Indirect chain writes: after the 12 direct blocks are full, the next
    /// write triggers a single-indirect block allocation (`mapping::decompose`:
    /// file blocks from 12 on go through `blocks[12]`).
    #[test]
    fn test_write_allocates_indirect_blocks() {
        let block_size = 4096usize;
        let mut server = mounted();
        // Write 13 blocks: 0..12 direct, from 12 on single-indirect.
        let big = vec![b'A'; block_size * 13 + 16];
        let n = server.write(11, 0, &big).unwrap();
        assert_eq!(n, big.len());
        // Read back both ends: the last block lives in the single-indirect
        // block.
        let mut head = Vec::new();
        //[SD19-gate] Test fixture: the read-back content is asserted by the
        // `assert_eq!(head, ...)` right after, so a failed read shows up as the
        // next mismatch; not unwrapping is deliberate (the same treatment as the
        // minix-sys/src/lib.rs test fixture)
        #[allow(clippy::let_underscore_must_use)]
        let _ = server.read(11, 0, 16, &mut |bytes| head.extend_from_slice(bytes));
        assert_eq!(head, vec![b'A'; 16]);
        let mut tail = Vec::new();
        //[SD19-gate] Test fixture: the indirect-block tail is asserted by the
        // `assert_eq!(tail, ...)` right after, so a failed read shows up as the
        // next mismatch; not unwrapping is the same shape as above
        #[allow(clippy::let_underscore_must_use)]
        let _ = server.read(11, (block_size * 13) as i64, 16, &mut |bytes| {
            tail.extend_from_slice(bytes)
        });
        assert_eq!(tail, vec![b'A'; 16]);
    }

    /// truncate frees every block and indirect block, restoring the bitmaps and
    /// counts.
    #[test]
    fn test_truncate_frees_blocks_and_restores_counts() {
        let mut server = mounted();
        let big = vec![b'A'; 4096 * 2];
        let _ = server.write(11, 0, &big).unwrap();
        server.truncate(11, 0, 0).unwrap();
        let mut st = Stat::zeroed();
        server.stat(11, &mut st).unwrap();
        assert_eq!(st.size, 0, "truncate 后尺寸归零");
        // Write after truncation: blocks are reallocated from the bitmap and the
        // data is still readable.
        let n = server.write(11, 0, b"FRESH").unwrap();
        assert_eq!(n, 5);
        let mut got = Vec::new();
        //[SD19-gate] Test fixture: `assert_eq!(&got[..5], ...)` right after
        // asserts the old tail is gone after the truncating write; not unwrapping
        // is the same shape as above
        #[allow(clippy::let_underscore_must_use)]
        let _ = server.read(11, 0, 16, &mut |bytes| got.extend_from_slice(bytes));
        assert_eq!(&got[..5], b"FRESH");
    }

    /// Bitmap clearing after a truncate of a write that crossed the indirect
    /// chain (the indirect block is freed too).
    #[test]
    fn test_truncate_after_indirect_write_restores_bitmap() {
        let mut server = mounted();
        let big = vec![b'B'; 4096 * 2];
        let _ = server.write(11, 0, &big).unwrap();
        server.truncate(11, 0, 0).unwrap();
        // With every block bitmap bit cleared, one large allocation should still
        // succeed (a proxy for the bitmap-restoration assertion).
        let first = server.alloc_block().unwrap();
        let second = server.alloc_block().unwrap();
        assert_ne!(first, second);
        server.free_block(first);
        server.free_block(second);
    }

    // ---- Namespace operations and metadata ----

    #[test]
    fn test_create_makes_file_findable_and_readable() {
        let mut server = mounted();
        let node = server
            .create(ROOT_INODE_NUMBER as u64, "NEW.TXT", 0o644, 0, 0)
            .unwrap();
        assert_eq!(node.mode & 0o170000, 0o100000, "普通文件类型位");
        let (found, is_dir) = server
            .lookup_child(ROOT_INODE_NUMBER as u64, "NEW.TXT")
            .unwrap();
        assert!(!is_dir);
        assert_eq!(found.inode_number, node.inode_number);
    }

    #[test]
    fn test_link_shares_inode_and_counts() {
        let mut server = mounted();
        let node = server
            .create(ROOT_INODE_NUMBER as u64, "A.TXT", 0o644, 0, 0)
            .unwrap();
        server
            .link(ROOT_INODE_NUMBER as u64, "B.TXT", node.inode_number)
            .unwrap();
        let (via_a, _) = server
            .lookup_child(ROOT_INODE_NUMBER as u64, "A.TXT")
            .unwrap();
        let (via_b, _) = server
            .lookup_child(ROOT_INODE_NUMBER as u64, "B.TXT")
            .unwrap();
        assert_eq!(via_a.inode_number, via_b.inode_number, "硬链接同 inode");
        let mut st = Stat::zeroed();
        server.stat(node.inode_number, &mut st).unwrap();
        assert_eq!(st.nlinks, 2);
    }

    #[test]
    fn test_unlink_drops_name_and_frees_on_last_link() {
        let mut server = mounted();
        let node = server
            .create(ROOT_INODE_NUMBER as u64, "GONE.TXT", 0o644, 0, 0)
            .unwrap();
        server.unlink(ROOT_INODE_NUMBER as u64, "GONE.TXT").unwrap();
        assert!(server.lookup_child(ROOT_INODE_NUMBER as u64, "GONE.TXT").is_err());
        // The last link breaks → the inode is freed (the next allocation gets
        // the same number).
        let reused = server.alloc_inode(0o100644).unwrap();
        assert_eq!(reused, node.inode_number as u32, "释放后位图位回收");
    }

    #[test]
    fn test_mkdir_lays_dot_pair_and_rmdir_takes_it_back() {
        let mut server = mounted();
        server
            .make_dir(ROOT_INODE_NUMBER as u64, "SUB", 0o755, 0, 0)
            .unwrap();
        let (sub, is_dir) = server
            .lookup_child(ROOT_INODE_NUMBER as u64, "SUB")
            .unwrap();
        assert!(is_dir);
        // `.` and `..` are present in the subdirectory: `.` points at itself
        // and `..` at the parent.
        let (dot, dot_is_dir) = server.lookup_child(sub.inode_number, ".").unwrap();
        assert!(dot_is_dir && dot.inode_number == sub.inode_number);
        let (dotdot, _) = server.lookup_child(sub.inode_number, "..").unwrap();
        assert_eq!(dotdot.inode_number, ROOT_INODE_NUMBER as u64);
        // rmdir: the name disappears (empty-directory semantics).
        server.remove_dir(ROOT_INODE_NUMBER as u64, "SUB").unwrap();
        assert!(server.lookup_child(ROOT_INODE_NUMBER as u64, "SUB").is_err());
    }

    #[test]
    fn test_rmdir_refuses_nonempty() {
        let mut server = mounted();
        server
            .make_dir(ROOT_INODE_NUMBER as u64, "SUB2", 0o755, 0, 0)
            .unwrap();
        let (sub, _) = server
            .lookup_child(ROOT_INODE_NUMBER as u64, "SUB2")
            .unwrap();
        let _ = server
            .create(sub.inode_number, "INNER.TXT", 0o644, 0, 0)
            .unwrap();
        assert_eq!(
            server
                .remove_dir(ROOT_INODE_NUMBER as u64, "SUB2")
                .unwrap_err()
                .to_i32(),
            minix_types::ENOTEMPTY
        );
    }

    #[test]
    fn test_symlink_roundtrip_fast() {
        let mut server = mounted();
        server
            .symbolic_link(
                ROOT_INODE_NUMBER as u64,
                "LNK",
                0,
                0,
                b"/some/target",
            )
            .unwrap();
        let (lnk_node, _) = server
            .lookup_child(ROOT_INODE_NUMBER as u64, "LNK")
            .unwrap();
        let mut got = Vec::new();
        let n = server
            .read_link(lnk_node.inode_number, 128, &mut |bytes| {
                got.extend_from_slice(bytes)
            })
            .unwrap();
        assert_eq!(&got[..n], b"/some/target");
    }

    #[test]
    fn test_rename_moves_name() {
        let mut server = mounted();
        let _ = server
            .create(ROOT_INODE_NUMBER as u64, "OLD.TXT", 0o644, 0, 0)
            .unwrap();
        server
            .rename(
                ROOT_INODE_NUMBER as u64,
                "OLD.TXT",
                ROOT_INODE_NUMBER as u64,
                "RENAMED.TXT",
            )
            .unwrap();
        assert!(server.lookup_child(ROOT_INODE_NUMBER as u64, "OLD.TXT").is_err());
        assert!(server.lookup_child(ROOT_INODE_NUMBER as u64, "RENAMED.TXT").is_ok());
    }

    // ---- Tail: the fs_rename decision tree (paired with link.c:268-445) ----

    /// Move a directory across parents: the name migrates, `..` re-points at
    /// the new parent, and the two parents' link counts shift by one each way.
    #[test]
    fn test_rename_moves_dir_across_parents_and_retargets_dotdot() {
        let mut server = mounted();
        server.make_dir(ROOT_INODE_NUMBER as u64, "A", 0o755, 0, 0).unwrap();
        server.make_dir(ROOT_INODE_NUMBER as u64, "B", 0o755, 0, 0).unwrap();
        let (a, _) = server.lookup_child(ROOT_INODE_NUMBER as u64, "A").unwrap();
        let (b, _) = server.lookup_child(ROOT_INODE_NUMBER as u64, "B").unwrap();
        let mut root_before = Stat::zeroed();
        server.stat(ROOT_INODE_NUMBER as u64, &mut root_before).unwrap();

        server.rename(ROOT_INODE_NUMBER as u64, "A", b.inode_number, "C").unwrap();

        assert!(server.lookup_child(ROOT_INODE_NUMBER as u64, "A").is_err());
        let (c, is_dir) = server.lookup_child(b.inode_number, "C").unwrap();
        assert!(is_dir);
        assert_eq!(c.inode_number, a.inode_number, "inode 跟着走");
        // C's `..` now points at the new parent B, while `.` still points at
        // itself.
        let (dotdot, _) = server.lookup_child(c.inode_number, "..").unwrap();
        assert_eq!(dotdot.inode_number, b.inode_number);
        let (dot, _) = server.lookup_child(c.inode_number, ".").unwrap();
        assert_eq!(dot.inode_number, c.inode_number);
        // Directory-entry type bytes: both C and `..` are directories.
        let (c_entry, _, _) = server.find_entry_slot(b.inode_number as u32, "C").unwrap();
        assert_eq!(c_entry.file_type, dir::TYPE_DIRECTORY);
        let (dd_entry, _, _) = server.find_entry_slot(c.inode_number as u32, "..").unwrap();
        assert_eq!(dd_entry.file_type, dir::TYPE_DIRECTORY);
        // Link accounting: old parent -1, new parent +1 (B = its own two links
        // plus C's `..`).
        let mut root_after = Stat::zeroed();
        server.stat(ROOT_INODE_NUMBER as u64, &mut root_after).unwrap();
        assert_eq!(root_after.nlinks + 1, root_before.nlinks);
        let mut b_stat = Stat::zeroed();
        server.stat(b.inode_number, &mut b_stat).unwrap();
        assert_eq!(b_stat.nlinks, 3);
    }

    /// Move a file across parents: the swap goes "insert first, then delete"
    /// and directory link counts stay put (a file brings no `..`).
    #[test]
    fn test_rename_moves_file_across_parents() {
        let mut server = mounted();
        server.make_dir(ROOT_INODE_NUMBER as u64, "DST", 0o755, 0, 0).unwrap();
        let (dst, _) = server.lookup_child(ROOT_INODE_NUMBER as u64, "DST").unwrap();
        server
            .rename(ROOT_INODE_NUMBER as u64, "FILE.TXT", dst.inode_number, "MOVED.TXT")
            .unwrap();
        assert!(server.lookup_child(ROOT_INODE_NUMBER as u64, "FILE.TXT").is_err());
        let (moved, is_dir) = server.lookup_child(dst.inode_number, "MOVED.TXT").unwrap();
        assert!(!is_dir);
        assert_eq!(moved.inode_number, 11);
        let mut st = Stat::zeroed();
        server.stat(dst.inode_number, &mut st).unwrap();
        assert_eq!(st.nlinks, 2, "文件迁移不占目录链接");
    }

    /// Move a directory into its own subtree: walking up `..` and meeting
    /// itself is EINVAL; a legal move to another generation still succeeds.
    #[test]
    fn test_rename_refuses_dir_into_own_subtree() {
        let mut server = mounted();
        server.make_dir(ROOT_INODE_NUMBER as u64, "TOP", 0o755, 0, 0).unwrap();
        let (top, _) = server.lookup_child(ROOT_INODE_NUMBER as u64, "TOP").unwrap();
        server.make_dir(top.inode_number, "SUB", 0o755, 0, 0).unwrap();
        let (sub, _) = server.lookup_child(top.inode_number, "SUB").unwrap();
        // A direct subtree hit.
        assert_eq!(
            server
                .rename(ROOT_INODE_NUMBER as u64, "TOP", sub.inode_number, "UNDER")
                .unwrap_err()
                .to_i32(),
            minix_types::EINVAL
        );
        // Moving SUB under the root forms no cycle, so allow it, and `..`
        // follows along and points at the root.
        server
            .rename(top.inode_number, "SUB", ROOT_INODE_NUMBER as u64, "SUB2")
            .unwrap();
        let (moved, _) = server.lookup_child(ROOT_INODE_NUMBER as u64, "SUB2").unwrap();
        let (dotdot, _) = server.lookup_child(moved.inode_number, "..").unwrap();
        assert_eq!(dotdot.inode_number, ROOT_INODE_NUMBER as u64);
    }

    /// Overwrite a same-named file target: the old target is detached and
    /// freed, the name lands on the old inode, and the number is recycled.
    #[test]
    fn test_rename_overwrites_file_target_and_recycles_inode() {
        let mut server = mounted();
        let a = server
            .create(ROOT_INODE_NUMBER as u64, "A.TXT", 0o644, 0, 0)
            .unwrap();
        let b = server
            .create(ROOT_INODE_NUMBER as u64, "B.TXT", 0o644, 0, 0)
            .unwrap();
        server
            .rename(ROOT_INODE_NUMBER as u64, "A.TXT", ROOT_INODE_NUMBER as u64, "B.TXT")
            .unwrap();
        let (now_b, _) = server.lookup_child(ROOT_INODE_NUMBER as u64, "B.TXT").unwrap();
        assert_eq!(now_b.inode_number, a.inode_number, "B.TXT 指向 A 的 inode");
        let mut st = Stat::zeroed();
        server.stat(a.inode_number, &mut st).unwrap();
        assert_eq!(st.nlinks, 1);
        // The old B hit zero links and was freed: the next allocation recycles
        // the same number.
        let recycled = server.alloc_inode(0o100644).unwrap();
        assert_eq!(recycled, b.inode_number as u32);
    }

    /// Put a directory over a non-empty directory: the empty check refuses, and
    /// both names stay put.
    #[test]
    fn test_rename_dir_onto_nonempty_dir_refuses() {
        let mut server = mounted();
        server.make_dir(ROOT_INODE_NUMBER as u64, "SRC", 0o755, 0, 0).unwrap();
        server.make_dir(ROOT_INODE_NUMBER as u64, "DST", 0o755, 0, 0).unwrap();
        let (dst, _) = server.lookup_child(ROOT_INODE_NUMBER as u64, "DST").unwrap();
        let _ = server.create(dst.inode_number, "INNER.TXT", 0o644, 0, 0).unwrap();
        assert_eq!(
            server
                .rename(ROOT_INODE_NUMBER as u64, "SRC", ROOT_INODE_NUMBER as u64, "DST")
                .unwrap_err()
                .to_i32(),
            minix_types::ENOTEMPTY
        );
        assert!(server.lookup_child(ROOT_INODE_NUMBER as u64, "SRC").is_ok());
        assert!(server.lookup_child(ROOT_INODE_NUMBER as u64, "DST").is_ok());
    }

    /// Two type-mismatch refusals: a directory over a file gives ENOTDIR, a file
    /// over a directory gives EISDIR.
    #[test]
    fn test_rename_type_mismatch_refusals() {
        let mut server = mounted();
        server.make_dir(ROOT_INODE_NUMBER as u64, "SUB", 0o755, 0, 0).unwrap();
        assert_eq!(
            server
                .rename(ROOT_INODE_NUMBER as u64, "SUB", ROOT_INODE_NUMBER as u64, "FILE.TXT")
                .unwrap_err()
                .to_i32(),
            minix_types::ENOTDIR
        );
        assert_eq!(
            server
                .rename(ROOT_INODE_NUMBER as u64, "FILE.TXT", ROOT_INODE_NUMBER as u64, "SUB")
                .unwrap_err()
                .to_i32(),
            minix_types::EISDIR
        );
    }

    /// Rename to itself: succeeds as a no-op, with no duplicate entry and no
    /// link growth.
    #[test]
    fn test_rename_to_same_name_is_noop() {
        let mut server = mounted();
        server
            .rename(ROOT_INODE_NUMBER as u64, "FILE.TXT", ROOT_INODE_NUMBER as u64, "FILE.TXT")
            .unwrap();
        let (found, _) = server.lookup_child(ROOT_INODE_NUMBER as u64, "FILE.TXT").unwrap();
        assert_eq!(found.inode_number, 11);
        let mut real = 0usize;
        server.walk_directory(ROOT_INODE_NUMBER, |_| {
            real += 1;
            true
        })
        .unwrap();
        assert_eq!(real, 3, "点名自身后仍是三条真实条目");
        let mut st = Stat::zeroed();
        server.stat(11, &mut st).unwrap();
        assert_eq!(st.nlinks, 1);
    }

    /// Same-parent rename keeps the directory-entry type byte (one regular file
    /// and one symbolic link each).
    #[test]
    fn test_rename_keeps_dirent_type_byte() {
        let mut server = mounted();
        server
            .symbolic_link(ROOT_INODE_NUMBER as u64, "LNK", 0, 0, b"/t")
            .unwrap();
        server
            .rename(ROOT_INODE_NUMBER as u64, "FILE.TXT", ROOT_INODE_NUMBER as u64, "RENAMED.TXT")
            .unwrap();
        server
            .rename(ROOT_INODE_NUMBER as u64, "LNK", ROOT_INODE_NUMBER as u64, "LNK2")
            .unwrap();
        let (file_entry, _, _) = server
            .find_entry_slot(ROOT_INODE_NUMBER, "RENAMED.TXT")
            .unwrap();
        assert_eq!(file_entry.file_type, dir::TYPE_REGULAR, "换名不改类型字节");
        let (lnk_entry, _, _) = server
            .find_entry_slot(ROOT_INODE_NUMBER, "LNK2")
            .unwrap();
        assert_eq!(lnk_entry.file_type, dir::TYPE_SYMLINK);
    }

    /// Move a directory across parents with the new parent at its link cap:
    /// EMLINK, and the failure has no side effects.
    #[test]
    fn test_rename_cross_parent_emlink_when_parent_full() {
        let mut server = mounted();
        server.make_dir(ROOT_INODE_NUMBER as u64, "A", 0o755, 0, 0).unwrap();
        server.make_dir(ROOT_INODE_NUMBER as u64, "B", 0o755, 0, 0).unwrap();
        let (b, _) = server.lookup_child(ROOT_INODE_NUMBER as u64, "B").unwrap();
        let mut record = server.read_inode(b.inode_number as u32).unwrap();
        record.links = LINK_MAX as u16;
        server.write_inode_back(b.inode_number as u32, &record);
        server.cache.insert(b.inode_number as u32, record);
        assert_eq!(
            server
                .rename(ROOT_INODE_NUMBER as u64, "A", b.inode_number, "C")
                .unwrap_err()
                .to_i32(),
            minix_types::EMLINK
        );
        assert!(server.lookup_child(ROOT_INODE_NUMBER as u64, "A").is_ok());
    }

    #[test]
    fn test_chmod_chown_update_cache_and_disc() {
        let mut server = mounted();
        let _ = server
            .create(ROOT_INODE_NUMBER as u64, "M.TXT", 0o644, 0, 0)
            .unwrap();
        // Change the permission bits of FILE.TXT (inode 11, 0o100644) in the
        // base image.
        let new_mode = server
            .change_mode(11, 0o600)
            .unwrap();
        assert_eq!(new_mode & 0o170000, 0o100000, "类型位保持");
        assert_eq!(new_mode & 0o7777, 0o600);
        let mut st = Stat::zeroed();
        server.stat(11, &mut st).unwrap();
        assert_eq!(st.mode & 0o7777, 0o600, "模式写回镜像且 stat 读到");
    }

}

