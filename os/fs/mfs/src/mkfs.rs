//! `mkfs.mfs` 的决定半：空 V3 文件系统的布局计算与镜像构建。
//!
//! C 对应物：`minix3/minix/usr.sbin/mkfs.mfs/mkfs.c`——布局算式照
//! `super()`（mkfs.c:602-716），缺省 inode 数照 `main` 的 KB 阶梯
//! （mkfs.c:344-357），根目录照 `rootdir`（mkfs.c:718-731：一个数据区、
//! `.` 与 `..` 两条目、链接数二）。原型的文件树填充（`eat_dir`/
//! `eat_file`）不在本模块——那是"从原型播种"的后续批次，这里只产出
//! 根目录为空的合法文件系统。
//!
//! # 布局（每块 4096 字节、`zone_shift = 0` 的缺省面）
//!
//! ```text
//! 块 0        引导块（全零）
//! 块 1        超级块（31 字节有效负载，其余零）
//! 块 2..      inode 位图（imap_blocks 块）
//! ..          区块位图（zmap_blocks 块）
//! ..          inode 表（inode_table_blocks 块，每块 bs/64 个）
//! first_data_zone 起为数据区
//! ```
//!
//! 位图按 Minix 惯例低位在前：0 号位恒置位（inode 0 与区块 0 "已分配
//! 但不用"），根 inode（1 号）与根数据区各占 1 号位。

use crate::inode::{DiskInode, DIRECTORY_ENTRY_SIZE, NAME_CAPACITY, TOTAL_ZONES, TYPE_DIRECTORY};
use crate::superblock::{DiskSuperblock, FLAG_CLEAN, MAGIC_V3, START_BLOCK};
use alloc::vec::Vec;
use alloc::vec;

/// 缺省块大小（C `DEFAULT_BLOCK_SIZE`，mfs.h:9 的 4096）。
pub const DEFAULT_BLOCK_SIZE: u64 = 4096;
/// 最小可接受块数（C `if (blocks < 5)`，mkfs.c:356）。
pub const MIN_BLOCKS: u64 = 5;
/// 每块的位数（`FS_BITS_PER_BLOCK`）。
fn bits_per_block(block_size: u64) -> u64 {
    block_size * 8
}

/// 每块的磁盘 inode 数（`INODES_PER_BLOCK(b) = b / 64`，const.h:39）。
fn inodes_per_block(block_size: u64) -> u64 {
    block_size / crate::superblock::INODE_DISK_SIZE as u64
}

/// 每块的间接项数（`INDIRECTS(b) = b / 4`，const.h:38）。
fn indirects_per_zone(block_size: u64) -> u64 {
    block_size / 4
}

/// 位图占块数（C `bitmapsize`，mkfs.c：向上取整到整块）。
fn bitmap_size(nr_bits: u64, block_size: u64) -> u64 {
    let per = bits_per_block(block_size);
    let blocks = nr_bits / per;
    if blocks * per < nr_bits {
        blocks + 1
    } else {
        blocks
    }
}

/// 布局计算的全部结果——镜像构建与 `mfs` 挂载校验共用同一份。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MkfsPlan {
    /// 块大小（字节）。
    pub block_size: u64,
    /// 总块数。
    pub blocks: u64,
    /// 总区块数（`zone_shift = 0` 时等于块数）。
    pub zones: u64,
    /// 可用 inode 数（已向上取整到 inode 块倍数）。
    pub inodes: u64,
    /// inode 位图块数。
    pub inode_map_blocks: u64,
    /// 区块位图块数。
    pub zone_map_blocks: u64,
    /// inode 表块数。
    pub inode_table_blocks: u64,
    /// 首个数据区块号。
    pub first_data_zone: u64,
    /// 最大文件尺寸。
    pub max_size: u64,
}

/// 布局计算失败的原因（错误消息对齐 C 的 `errx` 文案）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MkfsError {
    /// 块数不足（C "Block count too small"，mkfs.c:356）。
    TooSmall,
    /// inode 数不足（C "Inode count too small"，mkfs.c:358）。
    TooFewInodes,
    /// 位图把数据区挤没了（C "bit maps too large"，mkfs.c:626）。
    MapsTooLarge,
    /// 镜像缓冲不够容纳整幅文件系统。
    BufferTooSmall,
}

/// 缺省 inode 数的 KB 阶梯（C `main` mkfs.c:344-357：每 2 KB 一个，
/// 容量越大越稀，最后向上取整到 inode 块倍数）。
pub fn default_inode_count(blocks: u64, block_size: u64) -> u64 {
    let kb = blocks * block_size / 1024;
    let per_kb = if kb >= 1_000_000_000 {
        12
    } else if kb >= 100_000_000 {
        10
    } else if kb >= 10_000_000 {
        8
    } else if kb >= 1_000_000 {
        6
    } else if kb >= 100_000 {
        4
    } else {
        2
    };
    let raw = kb / per_kb;
    let per_block = inodes_per_block(block_size);
    (raw + per_block - 1) / per_block * per_block
}

/// 布局计算（C `super` 的算术半，mkfs.c:602-642；`zone_shift = 0`）。
pub fn plan_layout(
    blocks: u64,
    inodes: Option<u64>,
    block_size: u64,
) -> Result<MkfsPlan, MkfsError> {
    if blocks < MIN_BLOCKS {
        return Err(MkfsError::TooSmall);
    }
    let per_block = inodes_per_block(block_size);
    let inodes = match inodes {
        Some(n) => n,
        None => default_inode_count(blocks, block_size),
    };
    if inodes < 1 {
        return Err(MkfsError::TooFewInodes);
    }
    let inodes = (inodes + per_block - 1) / per_block * per_block;
    let zones = blocks;
    let inode_map_blocks = bitmap_size(1 + inodes, block_size);
    let zone_map_blocks = bitmap_size(zones, block_size);
    let inode_table_blocks = inodes / per_block;
    let first_data_zone = crate::superblock::START_BLOCK + inode_map_blocks + zone_map_blocks
        + inode_table_blocks;
    if first_data_zone >= zones {
        return Err(MkfsError::MapsTooLarge);
    }
    // 最大文件尺寸：`NR_DZONES + ind + ind²` 个区块，封顶 `INT32_MAX`
    // （C mkfs.c:646-655 的 `MAX_MAX_SIZE` 面）。
    let ind = indirects_per_zone(block_size);
    let zo = crate::superblock::DIRECT_ZONE_COUNT as u64
        + ind
        + ind.checked_mul(ind).ok_or(MkfsError::MapsTooLarge)?;
    let max_size = if (i32::MAX as u64) / block_size < zo {
        i32::MAX as u64
    } else {
        zo * block_size
    };
    Ok(MkfsPlan {
        block_size,
        blocks,
        zones,
        inodes,
        inode_map_blocks,
        zone_map_blocks,
        inode_table_blocks,
        first_data_zone,
        max_size,
    })
}

/// 在镜像的某一位图块里置一个位（`insert_bit`：Minix 位图低位在前）。
fn set_bit(image: &mut [u8], block_size: u64, block: u64, bit: u64) {
    let base = (block * block_size) as usize;
    let index = (bit / 8) as usize;
    let offset = (bit % 8) as u8;
    image[base + index] |= 1 << offset;
}

/// 一条目录项：4 字节 inode 号加 60 字节的名字缓冲（NUL 补齐）。
fn dir_entry(name: &[u8], inode: u64) -> [u8; DIRECTORY_ENTRY_SIZE] {
    let mut out = [0u8; DIRECTORY_ENTRY_SIZE];
    out[0..4].copy_from_slice(&(inode as u32).to_le_bytes());
    let n = name.len().min(NAME_CAPACITY);
    out[4..4 + n].copy_from_slice(&name[..n]);
    out
}

/// 构建整幅镜像（C `main` 的写序：boot 块零、super 块、map 与 inode 区
/// 清零、两张位图的 0 号位、根 inode、根数据区的 `.` 与 `..`）。
///
/// `now` 是写入 inode 时间戳的当前时刻（C 的 `current_time`；宿主测试
/// 固定它以保断言可复现）。
pub fn build_image(plan: &MkfsPlan, now: i32) -> Result<Vec<u8>, MkfsError> {
    let total = plan
        .blocks
        .checked_mul(plan.block_size)
        .ok_or(MkfsError::BufferTooSmall)?;
    let total = total as usize;
    let mut image = vec![0u8; total];
    let bs = plan.block_size;

    // 超级块：块 1（字节偏移 1024），31 字节有效负载。
    let small_first = if plan.first_data_zone <= u16::MAX as u64 {
        plan.first_data_zone as u16
    } else {
        // 放不下就写零（C mkfs.c:632-640：用其它字段可重算）。
        0
    };
    let sup = DiskSuperblock {
        inode_count: plan.inodes as u32,
        zone_total_small: 0,
        inode_map_blocks: plan.inode_map_blocks as i16,
        zone_map_blocks: plan.zone_map_blocks as i16,
        first_data_zone_small: small_first,
        log_zone_size: 0,
        flags: FLAG_CLEAN,
        max_size: plan.max_size as i32,
        zones: plan.zones as u32,
        magic: MAGIC_V3,
        pad: 0,
        block_size: plan.block_size as u16,
        disk_version: 0,
    };
    let at = bs as usize;
    image[at..at + DiskSuperblock::STORED_BYTES].copy_from_slice(&sup.to_bytes());

    // 两张位图的 0 号位恒置位（C mkfs.c:705-708）。
    set_bit(&mut image, bs, START_BLOCK, 0);
    set_bit(&mut image, bs, START_BLOCK + plan.inode_map_blocks, 0);

    // 根 inode：1 号，目录 0777，链接数二，尺寸两条目，直接区指首个
    // 数据区（C `alloc_inode` 加 `rootdir` 的 `add_zone`/`incr_link`×2）。
    let inode_table_block = START_BLOCK + plan.inode_map_blocks + plan.zone_map_blocks;
    let root = DiskInode {
        mode: (TYPE_DIRECTORY | 0o777) as u16,
        nlinks: 2,
        owner: 0,
        group: 0,
        size: 2 * DIRECTORY_ENTRY_SIZE as i32,
        accessed: now,
        modified: now,
        changed: now,
        zones: {
            let mut z = [0u32; TOTAL_ZONES];
            z[0] = plan.first_data_zone as u32;
            z
        },
    };
    let root_at = (inode_table_block * bs) as usize;
    image[root_at..root_at + crate::superblock::INODE_DISK_SIZE]
        .copy_from_slice(&root.to_bytes());

    // 根数据区：`.` 与 `..` 都指向 1 号（C `enter_dir`×2）。
    let zone_at = (plan.first_data_zone * bs) as usize;
    image[zone_at..zone_at + DIRECTORY_ENTRY_SIZE].copy_from_slice(&dir_entry(b".", 1));
    image[zone_at + DIRECTORY_ENTRY_SIZE..zone_at + 2 * DIRECTORY_ENTRY_SIZE]
        .copy_from_slice(&dir_entry(b"..", 1));

    // 位图记账：根 inode（1 号位）与根数据区（1 号位）。
    set_bit(&mut image, bs, START_BLOCK, 1);
    set_bit(&mut image, bs, START_BLOCK + plan.inode_map_blocks, 1);

    Ok(image)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::superblock::INODE_DISK_SIZE;
    use crate::superblock::ROOT_INODE_NUMBER;

    /// 布局算术逐项对照 C `super`（mkfs.c:602-655）：64 块、64 inode、
    /// 4096 字节块 → 各图一整块、inode 表一整块、数据区从 5 起。
    #[test]
    fn test_plan_layout_matches_c_arithmetic() {
        let plan = plan_layout(64, Some(64), 4096).unwrap();
        assert_eq!(plan.zones, 64);
        assert_eq!(plan.inodes, 64);
        assert_eq!(plan.inode_map_blocks, 1, "imap = ceil(65/32768)");
        assert_eq!(plan.zone_map_blocks, 1, "zmap = ceil(64/32768)");
        assert_eq!(plan.inode_table_blocks, 1, "64 inode / 每块 64 个");
        assert_eq!(plan.first_data_zone, 5);
        // 最大文件尺寸：7 + 1024 + 1024² 个区块 × 4096 封顶 INT32_MAX。
        assert_eq!(plan.max_size, i32::MAX as u64);
        // 尾偏移成立：`1,2+3` 的算式在 8 字节域里同为 5。
        assert_eq!(crate::superblock::START_BLOCK, 2);
    }

    /// 缺省 inode 数的 KB 阶梯与取整（C mkfs.c:344-357）。
    #[test]
    fn test_default_inode_count_ladder() {
        // 64 块 × 4096 = 256 KB → 每 2 KB 一个 = 128。
        assert_eq!(default_inode_count(64, 4096), 128);
        // 阶梯切换点：100_000 KB 起每 4 KB 一个。
        let blocks_100m = 100_000 * 1024 / 4096;
        // 25_000 向上取整到 64 的倍数（C mkfs.c:355-357 同款取整）。
        assert_eq!(default_inode_count(blocks_100m, 4096), 25_024);
        // 太小直接拒。
        assert_eq!(plan_layout(4, None, 4096), Err(MkfsError::TooSmall));
        // 位图挤占数据区：5 块放不下 2 图加 inode 表加数据。
        assert_eq!(
            plan_layout(5, Some(64), 4096),
            Err(MkfsError::MapsTooLarge)
        );
    }

    /// 镜像逐位核对：超级块可解析且字段回读一致，两张位图的 0/1 号位
    /// 置位，根 inode 与两条目录项就位（C `main` 写序）。
    #[test]
    fn test_image_round_trips_through_our_own_parser() {
        let plan = plan_layout(64, Some(64), 4096).unwrap();
        let image = build_image(&plan, 1_000).unwrap();
        assert_eq!(image.len() as u64, 64 * 4096);
        // 引导块与超块块的其余部分保持零（C 的 put_block(zero)）。
        assert!(image[..1024].iter().all(|b| *b == 0));
        // 超级块：用服务器自己的解析器回读。
        let at = 4096usize;
        let sup = DiskSuperblock::from_bytes(&image[at..at + DiskSuperblock::STORED_BYTES])
            .unwrap();
        assert_eq!(sup.magic, MAGIC_V3);
        assert_eq!(sup.inode_count, 64);
        assert_eq!(sup.zones, 64);
        assert_eq!(sup.inode_map_blocks, 1);
        assert_eq!(sup.zone_map_blocks, 1);
        assert_eq!(sup.first_data_zone_small, 5);
        assert_eq!(sup.block_size, 4096);
        assert_eq!(sup.flags, FLAG_CLEAN);
        assert_eq!(sup.log_zone_size, 0);
        assert_eq!(sup.disk_version, 0);
        // 位图：imap/zmap 的 0 号位与根 inode/根数据区的 1 号位。
        let imap_at = (crate::superblock::START_BLOCK * 4096) as usize;
        assert_eq!(image[imap_at], 0b11, "imap 位 0 与 1（根 inode）");
        let zmap_at = imap_at + 4096;
        assert_eq!(image[zmap_at], 0b11, "zmap 位 0 与 1（根数据区）");
        // 根 inode：目录、链接数二、尺寸两条目、直接区指数据区。
        let table_at = (crate::superblock::START_BLOCK + 2) * 4096;
        let root = DiskInode::from_bytes(
            &image[table_at as usize..table_at as usize + INODE_DISK_SIZE],
        )
        .unwrap();
        assert_eq!(root.mode, (TYPE_DIRECTORY | 0o777) as u16);
        assert_eq!(root.nlinks, 2);
        assert_eq!(root.size, 2 * DIRECTORY_ENTRY_SIZE as i32);
        assert_eq!(root.zones[0], 5);
        assert_eq!(root.accessed, 1_000);
        // 目录项：`.` 与 `..` 都指根。
        let zone_at = (plan.first_data_zone * 4096) as usize;
        assert_eq!(u32::from_le_bytes(image[zone_at..zone_at + 4].try_into().unwrap()), 1);
        assert_eq!(&image[zone_at + 4..zone_at + 5], b".");
        let second = zone_at + DIRECTORY_ENTRY_SIZE;
        assert_eq!(
            u32::from_le_bytes(image[second..second + 4].try_into().unwrap()),
            ROOT_INODE_NUMBER as u32
        );
        assert_eq!(&image[second + 4..second + 6], b"..");
        assert!(image[second + 6..second + DIRECTORY_ENTRY_SIZE]
            .iter()
            .all(|b| *b == 0));
    }
}
