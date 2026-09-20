//! `mkfs.mfs` 的决定半：空 V3 文件系统的布局计算、镜像构建与原型播种。
//!
//! C 对应物：`minix3/minix/usr.sbin/mkfs.mfs/mkfs.c`——布局算式照
//! `super()`（mkfs.c:602-716），缺省 inode 数照 `main` 的 KB 阶梯
//! （mkfs.c:344-357），根目录照 `rootdir`（mkfs.c:718-731：一个数据区、
//! `.` 与 `..` 两条目、链接数二）。原型的文件树填充照 `eat_dir`/
//! `eat_file`（mkfs.c:765-870）与目录、inode、分配三个助手组
//! （mkfs.c:873-1165），宿主文件内容经 [`ProtoHost`] 缝注入——决定半
//! 不开文件，薄壳供给读取器。
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
//!
//! # 原型文法（C `get_line`/`mode_con`，mkfs.c:1222-1256 与 :1195-1215）
//!
//! ```text
//! <引导块行，读到即弃>
//! <块数> <inode 数>
//! <类型><置位符><置位符><三位八进制> <uid> <gid>     ← 根目录
//! <名字> <模式> <uid> <gid> [参数…]                  ← 逐目录入，$ 终止//! ```
//!
//! 模式串是六字符（C `mkproto` mkproto.c:303-315 以 `%c%c%c%03o` 生成）：
//! 类型字母（`d`/`b`/`c`/`s`/`l`/`-`）、set-uid 位符（`u` 或 `-`）、
//! set-gid 位符（`g` 或 `-`）、三位八进制权限。条目行按类型字母分派
//! （`eat_dir` mkfs.c:793 起）：目录递归、块/字符设备带
//! `major minor [size-in-blocks]`、符号链接带目标、其余按普通文件从
//! 宿主读内容。名字超 60 字节截断（C `dir_try_enter` 的 strncpy 同形）。

use crate::inode::{DiskInode, DIRECTORY_ENTRY_SIZE, NAME_CAPACITY, TOTAL_ZONES, TYPE_DIRECTORY};
use crate::superblock::{DiskSuperblock, DIRECT_ZONE_COUNT, FLAG_CLEAN, MAGIC_V3, START_BLOCK};
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
    /// 块参数小于原型头声明的规模（C mkfs.c:305-308 的 errx）。
    ProtoTooLarge {
        /// 调用方给的块数（C 的 `%d`）。
        given: u64,
        /// 原型头声明的块数（C 的 `%ld`）。
        needed: u64,
    },
    /// inode 用尽（C alloc_inode 的 "File system does not have enough
    /// inodes"，mkfs.c:1118-1120）。
    NotEnoughInodes {
        /// 文件系统持有的 inode 总数（C 消息里的 `%llu`）。
        have: u64,
    },
    /// 数据区用尽（C alloc_zone 的 "File system not big enough for all
    /// the files"，mkfs.c:1157-1158）。
    NotEnoughZones,
    /// 原型行超长/超词/缺词/数字坏——C 对应面是 `get_line` 的
    /// "Line too long"（mkfs.c:1236-1237）加 atoi 的静默垃圾，这里一律
    /// 拒绝而不是复刻垃圾。
    ProtoBadLine,
    /// 原型模式串不是七字符形（C `mode_con` 对短串会读到分隔符之外的
    /// 字节——那是潜在越界读，Rust 侧按文法拒绝）。
    ProtoBadMode,
    /// 原型在 `$` 之前结束（C get_line 的 "Unexpected end-of-file"，
    /// mkfs.c:1239-1240）。
    ProtoUnexpectedEnd,
    /// 符号链接目标不短于一块（C enter_symlink 的 "symlink too long"，
    /// mkfs.c:736-738）。
    SymlinkTooLong,
    /// 目录超过一级间接还放不下新条目（C enter_dir 的 pexit，
    /// mkfs.c:947-949）。
    DirectoryTooLarge,
    /// 文件超过二级间接（C add_zone 的 pexit，mkfs.c:1052-1053）。
    FileTooLarge,
    /// 链接计数溢出（C incr_link 的 "Too many links to a directory"，
    /// mkfs.c:1071-1072）。
    TooManyLinks,
    /// inode 尺寸溢出（C incr_size 的 "File has become too big to be
    /// handled by MFS"，mkfs.c:1096-1098）。
    SizeTooLarge,
    /// 宿主文件读不出来（C eat_dir 的 "Can't open %s"；路径由薄壳侧
    /// 记录——本枚举不携带数据）。
    HostFile,
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
    raw.div_ceil(per_block) * per_block
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
    let inodes = inodes.div_ceil(per_block) * per_block;
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
    build_image_rooted(plan, now, (TYPE_DIRECTORY | 0o777) as u16, 0, 0)
}

/// 同 [`build_image`]，根 inode 的模式与属主由调用方给定——原型文件的
/// 头第三行就是根目录的模式行（C `main` mkfs.c:313-318）。
fn build_image_rooted(
    plan: &MkfsPlan,
    now: i32,
    root_mode: u16,
    root_uid: u16,
    root_gid: u16,
) -> Result<Vec<u8>, MkfsError> {
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

    // 根 inode：1 号，链接数二，尺寸两条目，直接区指首个数据区
    // （C `alloc_inode` 加 `rootdir` 的 `add_zone`/`incr_link`×2）。
    let inode_table_block = START_BLOCK + plan.inode_map_blocks + plan.zone_map_blocks;
    let root = DiskInode {
        mode: root_mode,
        nlinks: 2,
        owner: root_uid,
        group: root_gid,
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

// ======================================================================
// 原型播种（C 的 proto-file processing assist group + eat_dir/eat_file）
// ======================================================================

/// 宿主文件读取缝（C `eat_dir` 里 `open(token[4])` 的等价面）：决定半
/// 不碰文件系统，普通文件的内容由薄壳注入。`HostFile.mtime` 落进
/// inode 的修改时刻（C `file_time`，mkfs.c:860——`-d` 强制 current_time
/// 的分支由薄壳自行折算）。
pub trait ProtoHost {
    /// 读一个宿主文件的全部内容进 `out`，返回字节数与修改时刻。内容
    /// 超过缓冲按 [`MkfsError::BufferTooSmall`] 拒绝（文件放不进目标
    /// 文件系统，早失败比写一半强）。实现方自行记录失败路径——
    /// [`MkfsError::HostFile`] 不携带数据。
    fn read_file(&mut self, path: &str, out: &mut [u8]) -> Result<HostFile, MkfsError>;
}

/// 一次宿主读取的结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostFile {
    /// 内容字节数。
    pub len: usize,
    /// 修改时刻（inode 的 `mtime` 来源，C `file_time`）。
    pub mtime: i32,
}

/// 原型文件头（前两行有内容的产物）：引导块行读到即弃，第二行给块数
/// 与 inode 数（C `main` mkfs.c:294-310）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProtoHeader {
    /// 原型声明的块数（第二行第一词；0 表示由调用方定夺）。
    pub blocks: u64,
    /// 原型声明的 inode 数（0 = 用缺省阶梯）。
    pub inodes: u64,
}

/// 从原型文本解头（C `main` mkfs.c:294-310 的前两行）。调用方用头的
/// inode 数（零换缺省阶梯）算 [`plan_layout`]，再把同一份文本交给
/// [`build_image_seeded`]。
pub fn proto_header(proto: &str) -> Result<ProtoHeader, MkfsError> {
    let mut lines = proto.lines();
    // 第一行是引导块装载器名，读到即弃（C "skip boot block info"）。
    lines.next().ok_or(MkfsError::ProtoUnexpectedEnd)?;
    let counts = lines.next().ok_or(MkfsError::ProtoUnexpectedEnd)?;
    let tokens = tokenize(counts)?;
    if tokens.len() < 2 {
        return Err(MkfsError::ProtoBadLine);
    }
    Ok(ProtoHeader {
        blocks: parse_number(tokens[0])?,
        inodes: parse_number(tokens[1])?,
    })
}

/// 把一行按空格与制表符切词（C `get_line` 的 tokenize 半，mkfs.c:1244
/// 往后：空白变分隔符，词数上限十）。空行没有词，返回空表由调用方
/// 分派；超过十词拒绝——C 在这里越界写 `parse[10]`，属于潜在内存
/// 事故，Rust 侧按文法错误拒绝。
fn tokenize(line: &str) -> Result<Vec<&str>, MkfsError> {
    if line.len() > 300 {
        return Err(MkfsError::ProtoBadLine);
    }
    let mut tokens: Vec<&str> = Vec::new();
    for word in line.split([' ', '\t']) {
        if word.is_empty() {
            continue;
        }
        if tokens.len() == 10 {
            return Err(MkfsError::ProtoBadLine);
        }
        tokens.push(word);
    }
    Ok(tokens)
}

/// 十进制数解析（C `atoi`/`strtol` 的严格面：C 对垃圾输入静默回零，
/// 原型是构图输入，坏数直接拒绝比复刻垃圾诚实）。
fn parse_number(token: &str) -> Result<u64, MkfsError> {
    token.parse::<u64>().map_err(|_| MkfsError::ProtoBadLine)
}

/// 六字符模式串转权限位（C `mode_con`，mkfs.c:1195-1215；C `mkproto`
/// mkproto.c:303-315 以 `%c%c%c%03o` 生成）。首字符定文件类型（`l` 与
/// `s` 同映射 `S_IFLNK`，C 注记的 ls 兼容面），二三字符分别是
/// set-uid/set-gid 位符，末三位是八进制权限。
pub fn mode_con(text: &str) -> Result<u16, MkfsError> {
    let bytes = text.as_bytes();
    if bytes.len() < 6 {
        return Err(MkfsError::ProtoBadMode);
    }
    let mut mode: u16 = match bytes[0] {
        b'd' => 0o040000,
        b'b' => 0o060000,
        b'c' => 0o020000,
        b's' | b'l' => 0o120000,
        b'-' => 0o100000,
        _ => return Err(MkfsError::ProtoBadMode),
    };
    if bytes[1] == b'u' {
        mode |= 0o4000;
    }
    if bytes[2] == b'g' {
        mode |= 0o2000;
    }
    let mut perm = 0u16;
    for &digit in &bytes[3..6] {
        if !digit.is_ascii_digit() || digit > b'7' {
            return Err(MkfsError::ProtoBadMode);
        }
        perm = perm * 8 + (digit - b'0') as u16;
    }
    Ok(mode | perm)
}

/// 原型播种构建器：镜像加两个分配游标（C 的 `next_inode`/`next_zone`
/// 静态量），方法逐一对位 C 的助手函数组。
struct Seeder<'a> {
    image: &'a mut Vec<u8>,
    plan: MkfsPlan,
    /// inode 表首块（C 的 `inode_offset` = `BLK_ILIST`）。
    table_block: u64,
    /// 区块位图首块。
    zmap_block: u64,
    next_inode: u64,
    next_zone: u64,
    now: i32,
}

impl<'a> Seeder<'a> {
    fn new(image: &'a mut Vec<u8>, plan: &MkfsPlan, now: i32) -> Self {
        let table_block = START_BLOCK + plan.inode_map_blocks + plan.zone_map_blocks;
        let zmap_block = START_BLOCK + plan.inode_map_blocks;
        Seeder {
            image,
            plan: *plan,
            table_block,
            zmap_block,
            next_inode: 2, // 根 inode 已在底像里
            next_zone: plan.first_data_zone + 1, // 根数据区已占
            now,
        }
    }

    fn bs(&self) -> u64 {
        self.plan.block_size
    }

    /// inode `n` 在镜像里的 64 字节记录位置。
    fn inode_at(&self, n: u64) -> usize {
        let per_block = self.bs() / crate::superblock::INODE_DISK_SIZE as u64;
        let block = self.table_block + (n - 1) / per_block;
        ((block * self.bs()) + ((n - 1) % per_block) * crate::superblock::INODE_DISK_SIZE as u64)
            as usize
    }

    fn read_inode(&self, n: u64) -> Result<DiskInode, MkfsError> {
        let at = self.inode_at(n);
        DiskInode::from_bytes(&self.image[at..at + crate::superblock::INODE_DISK_SIZE])
            .map_err(|_| MkfsError::BufferTooSmall)
    }

    fn write_inode(&mut self, inode: &DiskInode, n: u64) {
        let at = self.inode_at(n);
        let bytes = inode.to_bytes();
        self.image[at..at + crate::superblock::INODE_DISK_SIZE].copy_from_slice(&bytes);
    }

    fn set_imap_bit(&mut self, bit: u64) {
        let bs = self.bs();
        set_bit(self.image, bs, START_BLOCK, bit);
    }

    /// 区块号到位图位（C `zoff = s_firstdatazone - 1` 的记账：首个数据
    /// 区占 1 号位）。
    fn set_zmap_bit(&mut self, zone: u64) {
        let bs = self.bs();
        set_bit(self.image, bs, self.zmap_block, zone + 1 - self.plan.first_data_zone);
    }

    /// `alloc_inode`（C mkfs.c:1112-1145）：取号、写模式属主、置位图。
    fn alloc_inode(&mut self, mode: u16, uid: u16, gid: u16) -> Result<u64, MkfsError> {
        let num = self.next_inode;
        if num > self.plan.inodes {
            return Err(MkfsError::NotEnoughInodes { have: self.plan.inodes });
        }
        self.next_inode += 1;
        let inode = DiskInode {
            mode,
            nlinks: 0,
            owner: uid,
            group: gid,
            size: 0,
            accessed: 0,
            modified: 0,
            changed: 0,
            zones: [0; TOTAL_ZONES],
        };
        self.write_inode(&inode, num);
        self.set_imap_bit(num);
        Ok(num)
    }

    /// `alloc_zone`（C mkfs.c:1147-1165）：取号、清零整区、置位图。
    fn alloc_zone(&mut self) -> Result<u64, MkfsError> {
        let zone = self.next_zone;
        if zone >= self.plan.blocks {
            return Err(MkfsError::NotEnoughZones);
        }
        self.next_zone += 1;
        let bs = self.bs() as usize;
        let at = (zone * self.bs()) as usize;
        self.image[at..at + bs].fill(0);
        self.set_zmap_bit(zone);
        Ok(zone)
    }

    /// `incr_link`（C mkfs.c:1057-1080）：链接计数加一，溢出即错。
    fn incr_link(&mut self, n: u64) -> Result<(), MkfsError> {
        let mut inode = self.read_inode(n)?;
        inode.nlinks = inode.nlinks.checked_add(1).ok_or(MkfsError::TooManyLinks)?;
        self.write_inode(&inode, n);
        Ok(())
    }

    /// `incr_size`（C mkfs.c:1082-1110）：目录每收一条目长 64 字节。
    fn incr_size(&mut self, n: u64, count: i32) -> Result<(), MkfsError> {
        let mut inode = self.read_inode(n)?;
        inode.size = inode
            .size
            .checked_add(count)
            .ok_or(MkfsError::SizeTooLarge)?;
        self.write_inode(&inode, n);
        Ok(())
    }

    /// `add_zone`（C mkfs.c:957-1055）：把区块挂到 inode 的第一个空槽，
    /// 直接区满走一级间接、再满走二级间接；尺寸与时刻随挂随记。
    fn add_zone(&mut self, n: u64, zone: u64, bytes: i32, mtime: i32) -> Result<(), MkfsError> {
        let mut inode = self.read_inode(n)?;
        inode.size = inode
            .size
            .checked_add(bytes)
            .ok_or(MkfsError::SizeTooLarge)?;
        inode.modified = mtime;
        inode.accessed = self.now;
        inode.changed = self.now;
        for slot in 0..DIRECT_ZONE_COUNT {
            if inode.zones[slot] == 0 {
                inode.zones[slot] = zone as u32;
                self.write_inode(&inode, n);
                return Ok(());
            }
        }
        // 一级间接：槽空先分配间接区（alloc_zone 已清零置位）。
        if inode.zones[DIRECT_ZONE_COUNT] == 0 {
            inode.zones[DIRECT_ZONE_COUNT] = self.alloc_zone()? as u32;
        }
        let indirect = inode.zones[DIRECT_ZONE_COUNT] as u64;
        self.write_inode(&inode, n);
        if self.hang_zone(indirect, zone)? {
            return Ok(());
        }
        // 二级间接：槽空先分配二级区，再在其一级层挂。
        if inode.zones[DIRECT_ZONE_COUNT + 1] == 0 {
            let mut inode = self.read_inode(n)?;
            inode.zones[DIRECT_ZONE_COUNT + 1] = self.alloc_zone()? as u32;
            self.write_inode(&inode, n);
        }
        let double = self.read_inode(n)?.zones[DIRECT_ZONE_COUNT + 1] as u64;
        let entries = (self.bs() / 4) as usize;
        for i in 0..entries {
            let mid = self.indirect_entry(double, i)?;
            if mid == 0 {
                let fresh = self.alloc_zone()? as u32;
                self.set_indirect_entry(double, i, fresh)?;
            }
            let mid = self.indirect_entry(double, i)?;
            if self.hang_zone(mid, zone)? {
                return Ok(());
            }
        }
        Err(MkfsError::FileTooLarge)
    }

    /// 在一级间接区的第 `i` 槽找空位挂数据区；挂上返回真。
    fn hang_zone(&mut self, indirect: u64, zone: u64) -> Result<bool, MkfsError> {
        let entries = (self.bs() / 4) as usize;
        for i in 0..entries {
            if self.indirect_entry(indirect, i)? == 0 {
                self.set_indirect_entry(indirect, i, zone as u32)?;
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn indirect_entry(&self, indirect: u64, index: usize) -> Result<u64, MkfsError> {
        let at = (indirect * self.bs()) as usize + index * 4;
        let bytes: [u8; 4] = self
            .image
            .get(at..at + 4)
            .and_then(|s| s.try_into().ok())
            .ok_or(MkfsError::BufferTooSmall)?;
        Ok(u32::from_le_bytes(bytes) as u64)
    }

    fn set_indirect_entry(&mut self, indirect: u64, index: usize, zone: u32) -> Result<(), MkfsError> {
        let at = (indirect * self.bs()) as usize + index * 4;
        let end = at + 4;
        let slice = self.image.get_mut(at..end).ok_or(MkfsError::BufferTooSmall)?;
        slice.copy_from_slice(&zone.to_le_bytes());
        Ok(())
    }

    /// `dir_try_enter`（C mkfs.c:873-903）：在一个目录区里找首个空槽
    /// （`d_inum == 0`）写入条目。名字超 60 字节截断（C 的 strncpy 同
    /// 形，(verbose) 警告不进决定半）。
    fn dir_try_enter(&mut self, zone: u64, child: u64, name: &[u8]) -> Result<bool, MkfsError> {
        let bs = self.bs() as usize;
        let base = (zone * self.bs()) as usize;
        let entries = bs / DIRECTORY_ENTRY_SIZE;
        for i in 0..entries {
            let at = base + i * DIRECTORY_ENTRY_SIZE;
            let busy = u32::from_le_bytes(
                self.image[at..at + 4].try_into().map_err(|_| MkfsError::BufferTooSmall)?,
            ) != 0;
            if busy {
                continue;
            }
            let entry = dir_entry(&name[..name.len().min(NAME_CAPACITY)], child);
            self.image[at..at + DIRECTORY_ENTRY_SIZE].copy_from_slice(&entry);
            return Ok(true);
        }
        Ok(false)
    }

    /// `enter_dir`（C mkfs.c:906-949）：先扫直接区（空的当场合一），
    /// 满了走一级间接；再放不下按 C 的 pexit 拒绝。
    fn enter_dir(&mut self, parent: u64, name: &[u8], child: u64) -> Result<(), MkfsError> {
        let mut inode = self.read_inode(parent)?;
        for slot in 0..DIRECT_ZONE_COUNT {
            if inode.zones[slot] == 0 {
                inode.zones[slot] = self.alloc_zone()? as u32;
                self.write_inode(&inode, parent);
            }
            let zone = self.read_inode(parent)?.zones[slot] as u64;
            if self.dir_try_enter(zone, child, name)? {
                return Ok(());
            }
        }
        if inode.zones[DIRECT_ZONE_COUNT] == 0 {
            inode.zones[DIRECT_ZONE_COUNT] = self.alloc_zone()? as u32;
            self.write_inode(&inode, parent);
        }
        let indirect = self.read_inode(parent)?.zones[DIRECT_ZONE_COUNT] as u64;
        let entries = (self.bs() / 4) as usize;
        for i in 0..entries {
            let zone = self.indirect_entry(indirect, i)?;
            let zone = if zone == 0 {
                let fresh = self.alloc_zone()?;
                self.set_indirect_entry(indirect, i, fresh as u32)?;
                fresh
            } else {
                zone
            };
            if self.dir_try_enter(zone, child, name)? {
                return Ok(());
            }
        }
        Err(MkfsError::DirectoryTooLarge)
    }

    /// `rootdir` 的递归半加 `eat_dir`（C mkfs.c:765-830）：逐行读原型，
    /// `$` 收束本目录；目录递归、设备挂设备号、链接挂目标、普通文件
    /// 经 [`ProtoHost`] 缝读内容。
    fn eat_dir<'p, H: ProtoHost>(
        &mut self,
        parent: u64,
        lines: &mut core::str::Lines<'p>,
        host: &mut H,
        scratch: &mut [u8],
    ) -> Result<(), MkfsError> {
        loop {
            let line = lines.next().ok_or(MkfsError::ProtoUnexpectedEnd)?;
            let tokens = tokenize(line)?;
            let Some(name) = tokens.first() else {
                continue; // 空行没有词——C 在这里读空指针，按跳过处理
            };
            if *name == "$" {
                return Ok(());
            }
            let mode_text = tokens.get(1).ok_or(MkfsError::ProtoBadLine)?;
            let uid = parse_number(tokens.get(2).ok_or(MkfsError::ProtoBadLine)?)?;
            let gid = parse_number(tokens.get(3).ok_or(MkfsError::ProtoBadLine)?)?;
            let mode = mode_con(mode_text)?;
            let n = self.alloc_inode(mode, uid as u16, gid as u16)?;
            self.enter_dir(parent, name.as_bytes(), n)?;
            self.incr_size(parent, DIRECTORY_ENTRY_SIZE as i32)?;
            self.incr_link(n)?;
            match mode_text.as_bytes()[0] {
                b'd' => {
                    let zone = self.alloc_zone()?;
                    self.add_zone(n, zone, 2 * DIRECTORY_ENTRY_SIZE as i32, self.now)?;
                    self.enter_dir(n, b".", n)?;
                    self.enter_dir(n, b"..", parent)?;
                    self.incr_link(parent)?;
                    self.incr_link(n)?;
                    self.eat_dir(n, lines, host, scratch)?;
                }
                b'b' | b'c' => {
                    let major = parse_number(tokens.get(4).ok_or(MkfsError::ProtoBadLine)?)?;
                    let minor = parse_number(tokens.get(5).ok_or(MkfsError::ProtoBadLine)?)?;
                    let blocks = match tokens.get(6) {
                        Some(text) => parse_number(text)?,
                        None => 0,
                    };
                    // C `add_zone(n, makedev(maj, min), …)`：区块号槽位
                    // 放设备号（major<<8|minor），不占位图。
                    let dev = (major << 8) | (minor & 0xff);
                    self.add_zone(n, dev, (blocks as u32)
                        .checked_mul(self.bs() as u32)
                        .ok_or(MkfsError::SizeTooLarge)? as i32, self.now)?;
                }
                b's' => {
                    let target = tokens.get(4).ok_or(MkfsError::ProtoBadLine)?;
                    self.enter_symlink(n, target.as_bytes())?;
                }
                _ => {
                    let path = tokens.get(4).ok_or(MkfsError::ProtoBadLine)?;
                    let file = host.read_file(path, scratch)?;
                    self.eat_file(n, &scratch[..file.len], file.mtime)?;
                }
            }
        }
    }

    /// `enter_symlink`（C mkfs.c:734-749）：目标写进一个新区，尺寸为
    /// 目标长度（不含 NUL）。
    fn enter_symlink(&mut self, n: u64, target: &[u8]) -> Result<(), MkfsError> {
        if target.len() >= self.bs() as usize {
            return Err(MkfsError::SymlinkTooLong);
        }
        let zone = self.alloc_zone()?;
        let at = (zone * self.bs()) as usize;
        self.image[at..at + target.len()].copy_from_slice(target);
        self.add_zone(n, zone, target.len() as i32, self.now)
    }

    /// `eat_file`（C mkfs.c:851-870）：内容按块切区写入，逐区挂链；
    /// 零长文件不占区。时刻用宿主文件的修改时刻。
    fn eat_file(&mut self, n: u64, data: &[u8], mtime: i32) -> Result<(), MkfsError> {
        let bs = self.bs() as usize;
        if data.is_empty() {
            return Ok(());
        }
        for chunk in data.chunks(bs) {
            let zone = self.alloc_zone()?;
            let at = (zone * self.bs()) as usize;
            self.image[at..at + chunk.len()].copy_from_slice(chunk);
            self.add_zone(n, zone, chunk.len() as i32, mtime)?;
        }
        Ok(())
    }
}

/// 带原型播种的镜像构建（C `main` 的 `simple == 0` 支路，mkfs.c:294-424）：
/// 头两行定规模与根模式，其余行逐目录播种。`plan` 由调用方按
/// [`proto_header`] 的头算出——头声明的块数超过 `plan.blocks` 即拒
/// （C mkfs.c:304-308 的 errx）。返回镜像。
pub fn build_image_seeded<H: ProtoHost>(
    plan: &MkfsPlan,
    proto: &str,
    host: &mut H,
    now: i32,
) -> Result<Vec<u8>, MkfsError> {
    let mut lines = proto.lines();
    lines.next().ok_or(MkfsError::ProtoUnexpectedEnd)?; // 引导块行
    let counts = lines.next().ok_or(MkfsError::ProtoUnexpectedEnd)?;
    let tokens = tokenize(counts)?;
    if tokens.len() < 2 {
        return Err(MkfsError::ProtoBadLine);
    }
    let header_blocks = parse_number(tokens[0])?;
    if header_blocks > plan.blocks {
        return Err(MkfsError::ProtoTooLarge { given: plan.blocks, needed: header_blocks });
    }
    let root_line = lines.next().ok_or(MkfsError::ProtoUnexpectedEnd)?;
    let root_tokens = tokenize(root_line)?;
    if root_tokens.len() < 3 {
        return Err(MkfsError::ProtoBadLine);
    }
    let root_mode = mode_con(root_tokens[0])?;
    let root_uid = parse_number(root_tokens[1])?;
    let root_gid = parse_number(root_tokens[2])?;

    let mut image = build_image_rooted(plan, now, root_mode, root_uid as u16, root_gid as u16)?;
    let mut scratch = vec![0u8; image.len()];
    let mut seeder = Seeder::new(&mut image, plan, now);
    seeder.eat_dir(1, &mut lines, host, &mut scratch)?;
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

    // ---------------- 原型播种 ----------------

    /// 内存宿主：路径到（内容、修改时刻）的替身——`ProtoHost` 缝的
    /// 测试半，对应薄壳的 std 文件读取器。
    struct MemHost {
        files: Vec<(&'static str, &'static [u8], i32)>,
    }

    impl ProtoHost for MemHost {
        fn read_file(&mut self, path: &str, out: &mut [u8]) -> Result<HostFile, MkfsError> {
            let Some((_, bytes, mtime)) = self.files.iter().find(|(p, _, _)| *p == path) else {
                return Err(MkfsError::HostFile);
            };
            if bytes.len() > out.len() {
                return Err(MkfsError::BufferTooSmall);
            }
            out[..bytes.len()].copy_from_slice(bytes);
            Ok(HostFile { len: bytes.len(), mtime: *mtime })
        }
    }

    /// 从镜像解析 inode `n` 的直接区里第 `i` 条目录项（测试走查助手）。
    fn dir_slot(image: &[u8], plan: &MkfsPlan, inode_number: u64, slot: usize) -> (u32, Vec<u8>) {
        let bs = plan.block_size as usize;
        let per_block = bs / INODE_DISK_SIZE;
        let table = (START_BLOCK + plan.inode_map_blocks + plan.zone_map_blocks) as usize;
        let at = table * bs
            + ((inode_number - 1) as usize / per_block) * INODE_DISK_SIZE
            + ((inode_number - 1) as usize % per_block) * INODE_DISK_SIZE;
        let inode = DiskInode::from_bytes(&image[at..at + INODE_DISK_SIZE]).unwrap();
        let zone = inode.zones[slot / (bs / DIRECTORY_ENTRY_SIZE)] as usize;
        let base = zone * bs + (slot % (bs / DIRECTORY_ENTRY_SIZE)) * DIRECTORY_ENTRY_SIZE;
        let number =
            u32::from_le_bytes(image[base..base + 4].try_into().unwrap());
        let mut name = Vec::new();
        for &b in &image[base + 4..base + DIRECTORY_ENTRY_SIZE] {
            if b == 0 {
                break;
            }
            name.push(b);
        }
        (number, name)
    }

    /// 六字符模式串的转换面（C `mode_con` 加 mkproto 的生成形）。
    #[test]
    fn test_mode_con_six_character_forms() {
        assert_eq!(mode_con("d--755").unwrap(), 0o040755);
        assert_eq!(mode_con("-u-644").unwrap(), 0o100644 | 0o4000);
        assert_eq!(mode_con("--g664").unwrap(), 0o100664 | 0o2000);
        assert_eq!(mode_con("s--777").unwrap(), 0o120777);
        assert_eq!(mode_con("c--660").unwrap(), 0o020660);
        // 短串在 C 里会读到分隔符之外（潜在越界读）——按文法拒绝。
        assert_eq!(mode_con("d755"), Err(MkfsError::ProtoBadMode));
        assert_eq!(mode_con("d--888"), Err(MkfsError::ProtoBadMode));
        assert_eq!(mode_con("q--755"), Err(MkfsError::ProtoBadMode));
    }

    /// 原型播种整树：目录递归、普通文件内容、符号链接、设备文件、
    /// 链接计数与位图记账，全部经服务器自己的解析器回读核对
    /// （C `eat_dir`/`eat_file` 的语义面）。
    #[test]
    fn test_proto_seeds_tree_with_all_entry_types() {
        let proto = "boot\n\
                     64 16\n\
                     d--750 0 0\n\
                     etc d--755 2 2\n\
                     $\n\
                     motd ---644 3 3 /host/motd\n\
                     tty c--660 0 0 4 0\n\
                     fast s--777 4 4 /host/motd\n\
                     $\n";
        let mut host =
            MemHost { files: vec![("/host/motd", b"hello proto\n", 1_700)] };
        let header = proto_header(proto).unwrap();
        assert_eq!(header, ProtoHeader { blocks: 64, inodes: 16 });
        let plan = plan_layout(64, Some(16), 4096).unwrap();
        let image = build_image_seeded(&plan, proto, &mut host, 1_000).unwrap();

        // 根目录：模式行来自头第三行（d-u750）；子目录一个，链接数三。
        let bs = 4096usize;
        let per_block = bs / INODE_DISK_SIZE;
        let table = (START_BLOCK + plan.inode_map_blocks + plan.zone_map_blocks) as usize;
        let inode_at = |n: u64| {
            let per = per_block as u64;
            table * bs
                + (((n - 1) / per) * INODE_DISK_SIZE as u64) as usize
                + (((n - 1) % per) * INODE_DISK_SIZE as u64) as usize
        };
        let root = DiskInode::from_bytes(&image[inode_at(1)..inode_at(1) + INODE_DISK_SIZE])
            .unwrap();
        assert_eq!(root.mode, 0o040750);
        assert_eq!(root.nlinks, 3, "根目录含子目录 etc：`.`、`..` 加 etc 的 `..`");
        assert_eq!(root.size as usize, 6 * DIRECTORY_ENTRY_SIZE);
        // 根的头两条还是 `.`/`..`，随后是四个条目（顺序按原型行）。
        assert_eq!(dir_slot(&image, &plan, 1, 0), (1, b".".to_vec()));
        assert_eq!(dir_slot(&image, &plan, 1, 1), (1, b"..".to_vec()));
        assert_eq!(dir_slot(&image, &plan, 1, 2), (2, b"etc".to_vec()));
        assert_eq!(dir_slot(&image, &plan, 1, 3), (3, b"motd".to_vec()));
        assert_eq!(dir_slot(&image, &plan, 1, 4), (4, b"tty".to_vec()));
        assert_eq!(dir_slot(&image, &plan, 1, 5), (5, b"fast".to_vec()));

        // etc 目录：空目录（`$` 紧随），`.`/`..` 指对，链接数二。
        assert_eq!(dir_slot(&image, &plan, 2, 0), (2, b".".to_vec()));
        assert_eq!(dir_slot(&image, &plan, 2, 1), (1, b"..".to_vec()));
        let etc = DiskInode::from_bytes(&image[inode_at(2)..inode_at(2) + INODE_DISK_SIZE])
            .unwrap();
        assert_eq!(etc.mode, 0o040755);
        assert_eq!(etc.owner, 2);
        assert_eq!(etc.group, 2);
        assert_eq!(etc.nlinks, 2);
        assert_eq!(etc.size as usize, 2 * DIRECTORY_ENTRY_SIZE);

        // 普通文件：内容从宿主读出，尺寸 13，mtime 来自宿主文件。
        let motd = DiskInode::from_bytes(&image[inode_at(3)..inode_at(3) + INODE_DISK_SIZE])
            .unwrap();
        assert_eq!(motd.mode, 0o100644);
        assert_eq!(motd.size as usize, b"hello proto\n".len());
        assert_eq!(motd.modified, 1_700);
        let data_at = motd.zones[0] as usize * bs;
        assert_eq!(&image[data_at..data_at + 12], b"hello proto\n");

        // 字符设备：`major<<8|minor` 进区块号槽，尺寸零，不占数据区。
        let tty = DiskInode::from_bytes(&image[inode_at(4)..inode_at(4) + INODE_DISK_SIZE])
            .unwrap();
        assert_eq!(tty.mode, 0o020660);
        assert_eq!(tty.zones[0], (4 << 8) | 0);
        assert_eq!(tty.size, 0);

        // 符号链接：目标字符串进数据区，尺寸为目标长度。
        let fast = DiskInode::from_bytes(&image[inode_at(5)..inode_at(5) + INODE_DISK_SIZE])
            .unwrap();
        assert_eq!(fast.mode, 0o120777);
        assert_eq!(fast.size as usize, "/host/motd".len());
        let target_at = fast.zones[0] as usize * bs;
        assert_eq!(&image[target_at..target_at + 10], b"/host/motd");

        // 位图记账：五个 inode 全在 imap，四个数据区（根、etc、motd、
        // fast）在 zmap；tty 无数据区。
        let imap = &image[(START_BLOCK * 4096) as usize..];
        for ino in 1..=5u64 {
            assert_eq!(imap[(ino / 8) as usize] >> (ino % 8) & 1, 1, "imap 位 {ino}");
        }
        let zmap_at = (START_BLOCK * 4096) as usize + bs;
        let zmap = &image[zmap_at..];
        assert_eq!(zmap[0] >> 1 & 0b1111, 0b1111, "位 1..4：根、etc、motd、fast 四区");
    }

    /// 播种的错误面：块参数小于头、原型早断、宿主文件缺、坏模式行、
    /// inode 用尽。
    #[test]
    fn test_proto_errors_are_rejected() {
        let mut host = MemHost { files: vec![] };
        let plan = plan_layout(16, Some(8), 4096).unwrap();
        // 头声明 64 块，参数只给 16（C mkfs.c:305-308 的 errx）。
        let proto_big = "boot\n64 8\nd--755 0 0\n$\n";
        assert_eq!(
            build_image_seeded(&plan, proto_big, &mut host, 0),
            Err(MkfsError::ProtoTooLarge { given: 16, needed: 64 })
        );
        // 原型在 `$` 之前结束（C get_line 的 Unexpected end-of-file）。
        let plan64 = plan_layout(64, Some(16), 4096).unwrap();
        let proto_short = "boot\n64 8\nd--755 0 0\n";
        assert_eq!(
            build_image_seeded(&plan64, proto_short, &mut host, 0),
            Err(MkfsError::ProtoUnexpectedEnd)
        );
        // 宿主文件缺失（C "Can't open %s"）。
        let proto_missing = "boot\n64 8\nd--755 0 0\nmotd ---644 0 0 /nope\n$\n";
        assert_eq!(
            build_image_seeded(&plan64, proto_missing, &mut host, 0),
            Err(MkfsError::HostFile)
        );
        // 模式行坏（缺词）。
        let proto_mode = "boot\n64 8\nd--755\n$\n";
        assert_eq!(
            build_image_seeded(&plan64, proto_mode, &mut host, 0),
            Err(MkfsError::ProtoBadLine)
        );
        // inode 用尽：4096 字节块的 inode 按块取整后 64 个，装不下
        // 根加 70 条目（C alloc_inode 的 not enough inodes）。
        let mut many = alloc::string::String::from("boot\n64 16\nd--755 0 0\n");
        use core::fmt::Write as _;
        for k in 0..70 {
            let _ = write!(many, "f{k} ---644 0 0 /host/motd\n");
        }
        many.push('$');
        // 空内容文件不占区，让 inode 先耗尽。
        let mut host2 = MemHost { files: vec![("/host/motd", b"", 0)] };
        let header = proto_header(&many).unwrap();
        let plan_tight = plan_layout(64, Some(header.inodes), 4096).unwrap();
        assert_eq!(plan_tight.inodes, 64, "16 向上取整到 inode 块倍数");
        assert_eq!(
            build_image_seeded(&plan_tight, &many, &mut host2, 0),
            Err(MkfsError::NotEnoughInodes { have: 64 })
        );
    }

    /// 零长文件与空根目录：零长不占区（C eat_file 的 ct==0 支路），
    /// 空根只有 `.`/`..`。
    #[test]
    fn test_proto_empty_file_and_bare_root() {
        let mut host = MemHost { files: vec![("/empty", b"", 5)] };
        let proto = "boot\n32 8\nd-u755 0 0\nempty --u644 0 0 /empty\n$\n";
        let plan = plan_layout(32, Some(8), 4096).unwrap();
        let image = build_image_seeded(&plan, proto, &mut host, 7).unwrap();
        let bs = plan.block_size as usize;
        let per_block = bs / INODE_DISK_SIZE;
        let table = (START_BLOCK + 2) as usize;
        let at = table * bs + (2 - 1) / per_block * INODE_DISK_SIZE + (2 - 1) % per_block * INODE_DISK_SIZE;
        let file = DiskInode::from_bytes(&image[at..at + INODE_DISK_SIZE]).unwrap();
        assert_eq!(file.size, 0);
        assert_eq!(file.zones[0], 0, "零长文件不占区");
        assert_eq!(file.modified, 0, "无区可挂，mtime 不落");
        // zmap 只有根数据区一个数据位。
        let zmap_at = (START_BLOCK as usize * bs) + plan.inode_map_blocks as usize * bs;
        assert_eq!(image[zmap_at], 0b11, "位 0 恒置 + 位 1 = 根数据区");
    }
}
