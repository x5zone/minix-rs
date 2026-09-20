//! `fsck.mfs` 的决定半：只读的文件系统一致性检测与报告。
//!
//! C 对应物：`minix3/minix/commands/fsck.mfs/fsck.c`（1675 行）。本模块
//! 只做**检测与报告**：超块校验（`rw_super` fsck.c:561-599、`chksuper`
//! fsck.c:602-655）、位图对照（`chkmap`/`chkword` fsck.c:786-841——按
//! 走查构造的"期望图"与盘上"实际图"逐位对比）、以及超级块列表
//! （`lsuper` fsck.c:528-559 的只读半）。修复面（`repair`/`automatic`
//! 的交互式修复）与 preen 并行**不在本模块**——`yes()` 问句与
//! `devwrite` 全部不出现，维持登记挂后批。
//!
//! # 检查序（C `chkdev` fsck.c:1538-1614 的只读投影）
//!
//! ```text
//! 读超块 → 基础 sanity（magic/计数/几何）→ chksuper 深检
//! → 走查文件树构造期望图（后续批次）→ chkmap(zone) → chkcount
//! → chkmap(inode) → chkilist → printtotal
//! ```
//!
//! # 立场与偏差（均已注记）
//!
//! - 只认第三版（`SUPER_V3`）：与服务器侧 `parse_superblock` 同一立场。
//!   C 对 V2 的支持带宿主编译补丁（`block_size = 8192`，fsck.c:579-581），
//!   V2 盘面本仓库不产出也不消费。
//! - C `rw_super` 的 `s_firstdatazone <= 4` 检查（fsck.c:592）落在
//!   in-memory 字段上——`read` 只填磁盘面 31 字节，该字段读到的是
//!   超块块里的杂散字节。本模块把同一判据落在磁盘的 small 字段上
//!   （`first_data_zone_small`），语义等价于"首数据区不得落在元数据区"。
//! - 宿主镜像整幅驻内存（`mkfs::build_image` 的对偶）：C 以一块缓存
//!   流式读设备，镜像文件没有随机读问题；流式接缝（trait 抽象）留待
//!   真机块设备面（E-FSBDEV）需要时再立。

use crate::superblock::{
    Bitmap, DiskSuperblock, FLAG_CLEAN, FLAG_MANDATORY_MASK, MAGIC_V1, MAGIC_V2, MAGIC_V3,
    START_BLOCK, SUPER_BLOCK_OFFSET,
};
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

/// 最大文件位置（C `MAX_FILE_POS`，minix/const.h:124：`0x7FFFFFFF`）。
const MAX_FILE_POS: i64 = 0x7FFF_FFFF;

/// 位图报告的截断阈值（C `MAXPRINT`，fsck.c:67）：每 `MAXPRINT` 个
/// 差异停一次逐条列出，之后只报总数。
pub const MAXPRINT: u64 = 80;

/// 用法错误退出码（C `FSCK_EXIT_USAGE`，sbin/fsck/exitvalues.h）。
pub const EXIT_USAGE: i32 = 1;
/// 检查失败退出码（C `FSCK_EXIT_CHECK_FAILED`，同上；`fatal()` 的出口）。
pub const EXIT_CHECK_FAILED: i32 = 8;

/// 每块的位数（`FS_BITS_PER_BLOCK(b) = b * 8`，const.h:44 经
/// `FS_BITMAP_CHUNKS` 折算：b/4 个 32 位字 × 32 位）。
fn bits_per_block(block_size: u64) -> u64 {
    block_size * 8
}

/// 位图占块数（C `bitmapsize`，libminlib/fslib.c：向上取整到整块）。
/// 与 `mkfs` 的同名算式同源——那边是构建半、这边是校验半。
fn bitmap_size(nr_bits: u64, block_size: u64) -> u64 {
    let per = bits_per_block(block_size);
    let blocks = nr_bits / per;
    if blocks * per < nr_bits {
        blocks + 1
    } else {
        blocks
    }
}

/// 每块 inode 数（`INODES_PER_BLOCK(b) = b / 64`，const.h:39）。
fn inodes_per_block(block_size: u64) -> u64 {
    block_size / crate::superblock::INODE_DISK_SIZE as u64
}

/// 校验通过后确立的文件系统几何——走查与位图对照的公共输入。
///
/// 字段对应 C `struct super_block` 的磁盘面加 `chksuper` 的两处重算
/// （`s_firstdatazone`、期望 `max_size`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SuperInfo {
    /// 块大小（字节；V3 取自超块字段，fsck.c:584）。
    pub block_size: u64,
    /// 总区块数（`s_zones`）。
    pub zones: u64,
    /// 可用 inode 数（`s_ninodes`）。
    pub inodes: u64,
    /// inode 位图块数（`s_imap_blocks`）。
    pub inode_map_blocks: u64,
    /// 区块位图块数（`s_zmap_blocks`）。
    pub zone_map_blocks: u64,
    /// inode 表块数（`N_ILIST`，fsck.c:107 的向上取整算式）。
    pub inode_table_blocks: u64,
    /// 首数据区块号：`chksuper` 重算值与盘上 small 值核对后的采用值
    /// （fsck.c:629-641；盘上为 0 时采用重算值，不一致时告警并从盘）。
    pub first_data_zone: u64,
    /// 每区块的块数对数（`s_log_zone_size`）。
    pub log_zone_size: u32,
    /// 校验后的最大文件尺寸（盘上值或 C :642-646 的封顶算式）。
    pub max_size: i64,
    /// 清洁旗标（`MFSFLAG_CLEAN`）：preen 的跳过判据，本模块仅携带。
    pub clean: bool,
}

impl SuperInfo {
    /// inode 表的首块号（`BLK_ILIST = 2 + imap + zmap`，fsck.c:114）。
    pub fn inode_table_block(&self) -> u64 {
        START_BLOCK + self.inode_map_blocks + self.zone_map_blocks
    }

    /// 每区块的字节数（`ZONE_SIZE = block_size << log_zone_size`，
    /// fsck.c:116 的 `ztob` 面）。
    pub fn zone_bytes(&self) -> u64 {
        self.block_size << self.log_zone_size
    }
}

/// 致命错误：检查无法继续（C `fatal()`，fsck.c:241-246——打印消息、
/// "fatal" 尾注、退出 [`EXIT_CHECK_FAILED`]）。
///
/// 多行时按 C 的输出序：前置明细行在前、`fatal()` 的短消息收尾。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fatal {
    /// 致命原因（可含前置明细的换行）。
    pub message: String,
}

/// 对照的位图种类（C `chkmap` 的 `type` 参数加两个调用点
/// fsck.c:1586/:1588）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapKind {
    /// inode 位图：块起点 [`START_BLOCK`]，报告号即 inode 号。
    Inode,
    /// 区块位图：块起点 `2 + imap_blocks`，报告号 = 位号 + 首数据区 - 1
    /// （与 `markzone` 的位编号 `zno - FIRST + 1` 互逆，fsck.c:1223）。
    Zone,
}

impl MapKind {
    fn label(self) -> &'static str {
        match self {
            MapKind::Inode => "inode",
            MapKind::Zone => "zone",
        }
    }

    fn base_block(self, sup: &SuperInfo) -> u64 {
        match self {
            MapKind::Inode => START_BLOCK,
            MapKind::Zone => START_BLOCK + sup.inode_map_blocks,
        }
    }

    fn blocks(self, sup: &SuperInfo) -> u64 {
        match self {
            MapKind::Inode => sup.inode_map_blocks,
            MapKind::Zone => sup.zone_map_blocks,
        }
    }

    /// 位号到报告号的平移（`chkmap` 的 `bit` 实参，fsck.c:1586/:1588）。
    fn first_reported(self, sup: &SuperInfo) -> u64 {
        match self {
            MapKind::Inode => 0,
            MapKind::Zone => sup.first_data_zone - 1,
        }
    }
}

/// `fsck.mfs` 检查器：持有一幅镜像与校验后的几何，逐相位产出消息。
///
/// 消息（进度行、警告、不一致报告）按 C 的输出流序收进 [`Fsck::messages`]；
/// 检查器不改镜像——修复面未在本模块。
pub struct Fsck<'a> {
    image: &'a [u8],
    sup: SuperInfo,
    messages: Vec<String>,
}

impl<'a> Fsck<'a> {
    /// 读超块并做全部静态校验（C `rw_super` fsck.c:561-599 +
    /// `chkdev` 的块大小下限 :1549-1550 + `chksuper` fsck.c:602-655）。
    ///
    /// 致命项直接拒检；非致命偏差（位图块数与算式不符、首数据区与
    /// 重算值不符、最大尺寸与封顶算式不符）记为警告消息后继续。
    pub fn new(image: &'a [u8]) -> Result<Self, Fatal> {
        let at = SUPER_BLOCK_OFFSET;
        let disk = DiskSuperblock::from_bytes(&image[at..at + DiskSuperblock::STORED_BYTES])
            .map_err(|_| Fatal {
                message: String::from("couldn't read super block"),
            })?;
        let (sup, warnings) = Self::validate(&disk, image.len())?;
        let mut messages: Vec<String> = Vec::new();
        messages.extend(warnings);
        Ok(Self {
            image,
            sup,
            messages,
        })
    }

    /// C `rw_super` 的 sanity 组加 `chksuper` 的深检组，按 C 的判定序。
    /// 非致命偏差作为警告随几何一起返回。
    fn validate(disk: &DiskSuperblock, image_len: usize) -> Result<(SuperInfo, Vec<String>), Fatal> {
        // 魔数：V1 直接拒（fsck.c:578）；V2 的支持带宿主编译补丁，
        // 本仓库立场同服务器侧——只认 V3（模块文档§立场与偏差）。
        match disk.magic {
            MAGIC_V1 => {
                return Err(Fatal {
                    message: String::from("Cannot handle V1 file systems"),
                })
            }
            MAGIC_V2 => {
                return Err(Fatal {
                    message: String::from("Cannot handle V2 file systems"),
                })
            }
            MAGIC_V3 => {}
            _ => {
                return Err(Fatal {
                    message: String::from("bad magic number in super block"),
                })
            }
        }
        let block_size = disk.block_size as u64;
        let mut warnings: Vec<String> = Vec::new();
        // rw_super 的计数与几何 sanity（fsck.c:588-598）。
        if disk.inode_count == 0 {
            return Err(Fatal {
                message: String::from("no inodes"),
            });
        }
        if disk.zones == 0 {
            return Err(Fatal {
                message: String::from("no zones"),
            });
        }
        if disk.inode_map_blocks <= 0 {
            return Err(Fatal {
                message: String::from("no imap"),
            });
        }
        if disk.zone_map_blocks <= 0 {
            return Err(Fatal {
                message: String::from("no zmap"),
            });
        }
        // C :592 的判据落在 in-memory 字段上（读到的是超块块杂散字节，
        // 见模块文档§立场与偏差）；这里落在磁盘 small 字段上。
        if disk.first_data_zone_small != 0 && disk.first_data_zone_small <= 4 {
            return Err(Fatal {
                message: String::from("first data zone too small"),
            });
        }
        if disk.log_zone_size < 0 {
            return Err(Fatal {
                message: String::from("zone size < block size"),
            });
        }
        let mut max_size = disk.max_size as i64;
        if max_size <= 0 {
            warnings.push(format!(
                "warning: invalid max file size {}",
                disk.max_size
            ));
            max_size = i64::MAX;
        }
        // chkdev 的块大小下限（fsck.c:1549-1550）。
        if block_size < SUPER_BLOCK_OFFSET as u64 {
            return Err(Fatal {
                message: String::from("funny block size"),
            });
        }
        let log_zone_size = disk.log_zone_size as u32;
        // 每区块块数的对数：先上限后告警，再谈移位（fsck.c:625-628；
        // block_nr 为 64 位，上限 64）。
        if log_zone_size >= 64 {
            return Err(Fatal {
                message: String::from("log_zone_size too large"),
            });
        }
        if log_zone_size > 8 {
            warnings.push(format!(
                "warning: large log_zone_size ({})",
                log_zone_size
            ));
        }
        let scale = 1u64 << log_zone_size;
        let inode_map_blocks = disk.inode_map_blocks as u64;
        let zone_map_blocks = disk.zone_map_blocks as u64;
        let inodes = disk.inode_count as u64;
        let zones = disk.zones as u64;
        let inode_table_blocks = inodes.div_ceil(inodes_per_block(block_size));
        // chksuper：位图块数与算式对账（fsck.c:607-624）。
        let need_imap = bitmap_size(inodes + 1, block_size);
        if inode_map_blocks < need_imap {
            return Err(Fatal {
                // C 原文把 blocks 拼成 "bocks"（fsck.c:611），按原样保留。
                message: format!(
                    "need {} bocks for inode bitmap; only have {}\ntoo few imap blocks",
                    need_imap, inode_map_blocks
                ),
            });
        }
        if inode_map_blocks != need_imap {
            warnings.push(format!(
                "warning: expected {} imap_blocks instead of {}",
                need_imap, inode_map_blocks
            ));
        }
        let need_zmap = bitmap_size(zones, block_size);
        if zone_map_blocks < need_zmap {
            return Err(Fatal {
                message: String::from("too few zmap blocks"),
            });
        }
        if zone_map_blocks != need_zmap {
            warnings.push(format!(
                "warning: expected {} zmap_blocks instead of {}",
                need_zmap, zone_map_blocks
            ));
        }
        // 首数据区：重算值与盘上 small 值对账（fsck.c:629-641）。
        let computed_first =
            (Self::blk_ilist(inode_map_blocks, zone_map_blocks) + inode_table_blocks + scale - 1)
                >> log_zone_size;
        let first_data_zone = if disk.first_data_zone_small != 0 {
            let old = disk.first_data_zone_small as u64;
            if old >= zones {
                return Err(Fatal {
                    message: String::from("first data zone too large"),
                });
            }
            if old < computed_first {
                return Err(Fatal {
                    message: String::from("first data zone too small"),
                });
            }
            if old != computed_first {
                warnings.push(format!(
                    "warning: expected first data zone to be {} instead of {}",
                    computed_first, old
                ));
            }
            old
        } else {
            computed_first
        };
        // 最大文件尺寸封顶算式（fsck.c:642-646）：`MAX_FILE_POS` 起步，
        // 盖不住 `MAX_ZONES` 个区块时按区块容量折算。
        let max_zones = {
            let ind = block_size / 4;
            7 + ind + ind * ind
        };
        let mut expected_max = MAX_FILE_POS as i128;
        if ((expected_max - 1) >> log_zone_size) / block_size as i128 >= max_zones as i128 {
            expected_max = ((max_zones as i128) * (block_size as i128)) << log_zone_size;
        }
        if expected_max <= 0 {
            expected_max = i64::MAX as i128;
        }
        if max_size as i128 != expected_max {
            warnings.push(format!(
                "warning: expected max size to be {} instead of {}",
                expected_max, max_size
            ));
        }
        // 强制特性位：新格式的高位一律拒检（fsck.c:652-654）。
        if disk.flags & FLAG_MANDATORY_MASK != 0 {
            return Err(Fatal {
                message: String::from("unsupported feature bits - newer fsck needed"),
            });
        }
        // 宿主镜像的实在性检查（C 的对偶是设备读失败；镜像短于几何
        // 意味着后续每一块读都是零填充噪声，直接拒检更诚实）。
        let wanted_bytes = ((zones as u128) << log_zone_size) * (block_size as u128);
        if (image_len as u128) < wanted_bytes {
            return Err(Fatal {
                message: String::from("image shorter than the file system geometry"),
            });
        }
        Ok((
            SuperInfo {
                block_size,
                zones,
                inodes,
                inode_map_blocks,
                zone_map_blocks,
                inode_table_blocks,
                first_data_zone,
                log_zone_size,
                max_size,
                clean: disk.flags & FLAG_CLEAN != 0,
            },
            warnings,
        ))
    }

    /// `BLK_ILIST = 2 + imap + zmap`（fsck.c:114；这里只差 inode 表）。
    fn blk_ilist(inode_map_blocks: u64, zone_map_blocks: u64) -> u64 {
        START_BLOCK + inode_map_blocks + zone_map_blocks
    }

    /// 校验后的几何。
    pub fn super_info(&self) -> &SuperInfo {
        &self.sup
    }

    /// 至此累计的消息（进度行、警告、不一致报告），按流序。
    pub fn messages(&self) -> &[String] {
        &self.messages
    }

    /// 超级块列表（C `lsuper` fsck.c:528-559 的只读半：修复问句与
    /// 回写不在本模块，只列字段）。
    pub fn list_super(&self) -> String {
        let at = SUPER_BLOCK_OFFSET;
        let disk = DiskSuperblock::from_bytes(&self.image[at..at + DiskSuperblock::STORED_BYTES]);
        let disk = disk.expect("superblock validated at construction");
        format!(
            "ninodes       = {}\nnzones        = {}\nimap_blocks   = {}\nzmap_blocks   = {}\nfirstdatazone = {}\nlog_zone_size = {}\nmaxsize       = {}\nblock size    = {}\nflags         = {}\n",
            disk.inode_count,
            disk.zones,
            disk.inode_map_blocks,
            disk.zone_map_blocks,
            disk.first_data_zone_small,
            disk.log_zone_size,
            disk.max_size,
            disk.block_size,
            if self.sup.clean { "CLEAN" } else { "DIRTY" },
        )
    }

    /// 位图对照（C `chkmap` fsck.c:813-841 的只读半）：`expected` 是
    /// 按走查构造的期望图，盘上图逐位对账。
    ///
    /// 契约：`expected` 的位宽必须覆盖盘上图的全部整块
    /// （`blocks * block_size * 8` 位）——C 的构造图按整块分配，比较
    /// 遍历全部块，位图块尾部（inode 数以外的填充位）也参与对账。
    /// 返回差异数（`nerr`）。
    pub fn check_bitmap(&mut self, kind: MapKind, expected: &Bitmap) -> u64 {
        self.messages
            .push(format!("Checking {} map.", kind.label()));
        let blocks = kind.blocks(&self.sup);
        let total_bits = blocks * bits_per_block(self.sup.block_size);
        debug_assert_eq!(
            expected.bit_count(),
            total_bits,
            "expected map must cover the full on-disk map blocks"
        );
        let byte_base = (kind.base_block(&self.sup) * self.sup.block_size) as usize;
        let mut nerr: u64 = 0;
        let mut report = true;
        for bit in 0..total_bits {
            let on_disk = self.disk_bit(byte_base, bit);
            let wanted = expected.test(bit);
            if on_disk == wanted {
                continue;
            }
            nerr += 1;
            // C `chkword`：每第 MAXPRINT 个差异走静音支（该条不列），
            // 之后只计数不列条；只读模式下列举停止后补 "etc."。
            if nerr.is_multiple_of(MAXPRINT) {
                if report {
                    report = false;
                }
            } else if report {
                let number = kind.first_reported(&self.sup) + bit;
                match (on_disk, wanted) {
                    (true, false) => self.messages.push(format!(
                        "{} {} is missing",
                        kind.label(),
                        number
                    )),
                    (false, true) => self.messages.push(format!(
                        "{} {} is not free",
                        kind.label(),
                        number
                    )),
                    _ => unreachable!("bits differ by construction"),
                }
            }
        }
        if !report {
            self.messages.push(String::from("etc."));
        }
        if nerr > MAXPRINT || nerr > 10 {
            self.messages.push(format!("{} errors found.", nerr));
        }
        nerr
    }

    /// 盘上某位图位的取值（位图低位在前，`set_bit`/`bitset` 同一布局）。
    /// 越界按零读——几何已校验，此处只是防御。
    fn disk_bit(&self, byte_base: usize, bit: u64) -> bool {
        self.image
            .get(byte_base + (bit / 8) as usize)
            .is_some_and(|byte| byte & (1 << (bit % 8)) != 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mkfs::{build_image, plan_layout};

    /// 64 块、64 inode、4096 字节块的基准镜像：imap/zmap/inode 表各
    /// 一块，首数据区 5。
    fn base_image() -> (alloc::vec::Vec<u8>, crate::mkfs::MkfsPlan) {
        let plan = plan_layout(64, Some(64), 4096).unwrap();
        let image = build_image(&plan, 1_000).unwrap();
        (image, plan)
    }

    /// 与基准镜像配套的期望图：位 0 恒置，根 inode 与根数据区占位 1。
    fn expected_maps() -> (Bitmap, Bitmap) {
        let bits = 4096 * 8;
        let mut imap = Bitmap::new(bits);
        let mut zmap = Bitmap::new(bits);
        imap.set(1);
        zmap.set(1);
        (imap, zmap)
    }

    /// 取出致命错误（`unwrap_err` 需要 `Ok` 侧可调试，镜像不值得）。
    fn fatal_of(result: Result<Fsck, Fatal>) -> Fatal {
        match result {
            Err(fatal) => fatal,
            Ok(_) => panic!("expected a fatal error"),
        }
    }

    #[test]
    fn clean_image_passes_super_checks() {
        let (image, plan) = base_image();
        let fsck = Fsck::new(&image).unwrap();
        let sup = fsck.super_info();
        assert_eq!(sup.block_size, 4096);
        assert_eq!(sup.zones, 64);
        assert_eq!(sup.inodes, 64);
        assert_eq!(sup.inode_map_blocks, 1);
        assert_eq!(sup.zone_map_blocks, 1);
        assert_eq!(sup.inode_table_blocks, 1);
        assert_eq!(sup.first_data_zone, plan.first_data_zone);
        assert_eq!(sup.log_zone_size, 0);
        assert_eq!(sup.max_size, i32::MAX as i64);
        assert!(sup.clean);
        assert!(fsck.messages().is_empty(), "干净镜像无警告");
    }

    #[test]
    fn clean_maps_agree_bit_for_bit() {
        let (image, _plan) = base_image();
        let (imap, zmap) = expected_maps();
        let mut fsck = Fsck::new(&image).unwrap();
        assert_eq!(fsck.check_bitmap(MapKind::Zone, &zmap), 0);
        assert_eq!(fsck.check_bitmap(MapKind::Inode, &imap), 0);
        assert_eq!(
            fsck.messages(),
            ["Checking zone map.", "Checking inode map."]
        );
    }

    #[test]
    fn cleared_zone_bit_reports_not_free() {
        let (mut image, _plan) = base_image();
        // 区块位图在第 3 块；清掉位 1（根数据区）。
        let at = 3 * 4096;
        image[at] &= !0b10;
        let (_imap, zmap) = expected_maps();
        let mut fsck = Fsck::new(&image).unwrap();
        assert_eq!(fsck.check_bitmap(MapKind::Zone, &zmap), 1);
        assert!(fsck.messages().contains(&String::from("zone 5 is not free")));
    }

    #[test]
    fn stray_zone_bit_reports_missing() {
        let (mut image, _plan) = base_image();
        // 置一个空闲区块的位（位 7 → 区块 11）。
        let at = 3 * 4096;
        image[at] |= 0b1000_0000;
        let (_imap, zmap) = expected_maps();
        let mut fsck = Fsck::new(&image).unwrap();
        assert_eq!(fsck.check_bitmap(MapKind::Zone, &zmap), 1);
        assert!(fsck.messages().contains(&String::from("zone 11 is missing")));
    }

    #[test]
    fn padding_bits_beyond_geometry_are_also_checked() {
        let (mut image, _plan) = base_image();
        // 位图块尾部的填充位（位 32000，远超 64 个区块）也参与对账。
        let at = 3 * 4096 + (32000 / 8) as usize;
        image[at] |= 1 << (32000 % 8);
        let (_imap, zmap) = expected_maps();
        let mut fsck = Fsck::new(&image).unwrap();
        assert_eq!(fsck.check_bitmap(MapKind::Zone, &zmap), 1);
        // 报告号 = 位号 + 首数据区 - 1 = 32000 + 4。
        assert!(fsck
            .messages()
            .contains(&String::from("zone 32004 is missing")));
    }

    #[test]
    fn error_flood_truncates_listing_but_counts_all() {
        let (mut image, _plan) = base_image();
        // 置 100 个杂散位（位 100..200）：前 79 条逐条列出，第 80 条
        // 走静音支，之后只计数。
        for bit in 100u64..200 {
            let at = 3 * 4096 + (bit / 8) as usize;
            image[at] |= 1 << (bit % 8);
        }
        let (_imap, zmap) = expected_maps();
        let mut fsck = Fsck::new(&image).unwrap();
        assert_eq!(fsck.check_bitmap(MapKind::Zone, &zmap), 100);
        let listed = fsck
            .messages()
            .iter()
            .filter(|m| m.contains(" is missing"))
            .count();
        assert_eq!(listed, (MAXPRINT - 1) as usize, "MAXPRINT 条静音前只列 79 条");
        assert!(fsck.messages().contains(&String::from("etc.")));
        assert!(fsck
            .messages()
            .contains(&String::from("100 errors found.")));
    }

    #[test]
    fn small_error_count_has_no_summary() {
        let (mut image, _plan) = base_image();
        // 两条差异（nerr ≤ 10 且 ≤ MAXPRINT）：不报总数（fsck.c:838）。
        let at = 3 * 4096;
        image[at] |= 0b0001_0000; // 位 4 → 区块 8
        image[at] |= 0b0010_0000; // 位 5 → 区块 9
        let (_imap, zmap) = expected_maps();
        let mut fsck = Fsck::new(&image).unwrap();
        assert_eq!(fsck.check_bitmap(MapKind::Zone, &zmap), 2);
        assert!(!fsck
            .messages()
            .iter()
            .any(|m| m.contains("errors found.")));
    }

    /// 改写超块字段的助手：重建 31 字节磁盘面并写回镜像。
    fn rewrite_super(image: &mut [u8], patch: impl FnOnce(&mut DiskSuperblock)) {
        let at = SUPER_BLOCK_OFFSET;
        let mut sup =
            DiskSuperblock::from_bytes(&image[at..at + DiskSuperblock::STORED_BYTES]).unwrap();
        patch(&mut sup);
        image[at..at + DiskSuperblock::STORED_BYTES].copy_from_slice(&sup.to_bytes());
    }

    #[test]
    fn bad_magic_is_fatal() {
        let (mut image, _plan) = base_image();
        rewrite_super(&mut image, |sup| sup.magic = 0x1234);
        assert_eq!(
            fatal_of(Fsck::new(&image)),
            Fatal {
                message: String::from("bad magic number in super block")
            }
        );
    }

    #[test]
    fn v1_and_v2_magic_are_refused() {
        let (mut image, _plan) = base_image();
        rewrite_super(&mut image, |sup| sup.magic = MAGIC_V1);
        assert_eq!(
            fatal_of(Fsck::new(&image)),
            Fatal {
                message: String::from("Cannot handle V1 file systems")
            }
        );
        rewrite_super(&mut image, |sup| sup.magic = MAGIC_V2);
        assert_eq!(
            fatal_of(Fsck::new(&image)),
            Fatal {
                message: String::from("Cannot handle V2 file systems")
            }
        );
    }

    #[test]
    fn zero_counters_are_fatal() {
        for (field, message) in [
            //（字段名，rw_super 的短消息）
            ("inodes", "no inodes"),
            ("zones", "no zones"),
        ] {
            let (mut image, _plan) = base_image();
            rewrite_super(&mut image, |sup| match field {
                "inodes" => sup.inode_count = 0,
                _ => sup.zones = 0,
            });
            assert_eq!(
                fatal_of(Fsck::new(&image)),
                Fatal {
                    message: String::from(message)
                }
            );
        }
        let (mut image, _plan) = base_image();
        rewrite_super(&mut image, |sup| sup.inode_map_blocks = 0);
        assert_eq!(
            fatal_of(Fsck::new(&image)),
            Fatal {
                message: String::from("no imap")
            }
        );
        let (mut image, _plan) = base_image();
        rewrite_super(&mut image, |sup| sup.zone_map_blocks = 0);
        assert_eq!(
            fatal_of(Fsck::new(&image)),
            Fatal {
                message: String::from("no zmap")
            }
        );
    }

    #[test]
    fn too_few_imap_blocks_fatal_names_the_arithmetic() {
        let (mut image, _plan) = base_image();
        // 200_000 个 inode 需要 ceil(200_001/32768) = 7 块位图，只有 1 块。
        rewrite_super(&mut image, |sup| sup.inode_count = 200_000);
        assert_eq!(
            fatal_of(Fsck::new(&image)),
            Fatal {
                message: String::from(
                    "need 7 bocks for inode bitmap; only have 1\ntoo few imap blocks"
                )
            }
        );
    }

    #[test]
    fn surplus_imap_blocks_warn_but_pass() {
        let (mut image, _plan) = base_image();
        // imap 多一块：inode 表整体后移，首数据区重算值变 6——盘上
        // small 值同步改到 6，只剩位图块数一条警告。
        rewrite_super(&mut image, |sup| {
            sup.inode_map_blocks = 2;
            sup.first_data_zone_small = 6;
        });
        let fsck = Fsck::new(&image).unwrap();
        assert_eq!(fsck.super_info().inode_map_blocks, 2);
        assert!(fsck
            .messages()
            .contains(&String::from(
                "warning: expected 1 imap_blocks instead of 2"
            )));
    }

    #[test]
    fn first_data_zone_bounds_are_fatal() {
        let (mut image, _plan) = base_image();
        // old >= zones（fsck.c:631-632）。
        rewrite_super(&mut image, |sup| sup.first_data_zone_small = 64);
        assert_eq!(
            fatal_of(Fsck::new(&image)),
            Fatal {
                message: String::from("first data zone too large")
            }
        );
        // old <= 4（rw_super 判据，落在磁盘 small 字段上）。
        rewrite_super(&mut image, |sup| sup.first_data_zone_small = 3);
        assert_eq!(
            fatal_of(Fsck::new(&image)),
            Fatal {
                message: String::from("first data zone too small")
            }
        );
        // old < 重算值：128 inode 的布局首数据区是 6，old=5 落在 inode 表里。
        let mut plan = plan_layout(64, Some(64), 4096).unwrap();
        plan.inodes = 128;
        let mut image = build_image(&plan, 1_000).unwrap();
        rewrite_super(&mut image, |sup| sup.first_data_zone_small = 5);
        assert_eq!(
            fatal_of(Fsck::new(&image)),
            Fatal {
                message: String::from("first data zone too small")
            }
        );
    }

    #[test]
    fn first_data_zone_mismatch_warns_and_keeps_disk_value() {
        let (mut image, _plan) = base_image();
        // 布局允许的更靠后首数据区：警告并采用盘上值（fsck.c:635-640）。
        rewrite_super(&mut image, |sup| sup.first_data_zone_small = 6);
        let fsck = Fsck::new(&image).unwrap();
        assert_eq!(fsck.super_info().first_data_zone, 6);
        assert!(fsck.messages().contains(&String::from(
            "warning: expected first data zone to be 5 instead of 6"
        )));
    }

    #[test]
    fn log_zone_size_bounds() {
        let (mut image, _plan) = base_image();
        rewrite_super(&mut image, |sup| sup.log_zone_size = 64);
        assert_eq!(
            fatal_of(Fsck::new(&image)),
            Fatal {
                message: String::from("log_zone_size too large")
            }
        );
        let (mut image, _plan) = base_image();
        rewrite_super(&mut image, |sup| {
            sup.log_zone_size = -1;
        });
        assert_eq!(
            fatal_of(Fsck::new(&image)),
            Fatal {
                message: String::from("zone size < block size")
            }
        );
    }

    #[test]
    fn max_size_mismatch_warns() {
        let (mut image, _plan) = base_image();
        rewrite_super(&mut image, |sup| sup.max_size = 1_000);
        let fsck = Fsck::new(&image).unwrap();
        assert!(fsck.messages().contains(&String::from(
            "warning: expected max size to be 2147483647 instead of 1000"
        )));
    }

    #[test]
    fn mandatory_feature_bits_are_fatal() {
        let (mut image, _plan) = base_image();
        rewrite_super(&mut image, |sup| sup.flags |= 0x0100);
        assert_eq!(
            fatal_of(Fsck::new(&image)),
            Fatal {
                message: String::from("unsupported feature bits - newer fsck needed")
            }
        );
    }

    #[test]
    fn tiny_block_size_is_fatal() {
        let (mut image, _plan) = base_image();
        rewrite_super(&mut image, |sup| sup.block_size = 512);
        assert_eq!(
            fatal_of(Fsck::new(&image)),
            Fatal {
                message: String::from("funny block size")
            }
        );
    }

    #[test]
    fn short_image_is_fatal() {
        let (image, _plan) = base_image();
        let cut = &image[..4_000];
        assert_eq!(
            fatal_of(Fsck::new(cut)),
            Fatal {
                message: String::from("image shorter than the file system geometry")
            }
        );
    }

    #[test]
    fn list_super_lists_all_disk_fields() {
        let (image, plan) = base_image();
        let fsck = Fsck::new(&image).unwrap();
        let text = fsck.list_super();
        assert!(text.contains(&format!("ninodes       = {}", plan.inodes)));
        assert!(text.contains(&format!("nzones        = {}", plan.zones)));
        assert!(text.contains("firstdatazone = 5"));
        assert!(text.contains("block size    = 4096"));
        assert!(text.contains("flags         = CLEAN"));
    }
}
