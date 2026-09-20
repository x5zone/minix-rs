//! `fsck.mfs` 的决定半：只读的文件系统一致性检测与报告。
//!
//! C 对应物：`minix3/minix/commands/fsck.mfs/fsck.c`（1675 行）。本模块
//! 只做**检测与报告**：超块校验（`rw_super` fsck.c:561-599、`chksuper`
//! fsck.c:602-655）、文件树走查（`chktree`/`descendtree`/`chkinode`/
//! `chkmode`/`chkfile`/`chkzones`/`markzone` 与目录项检查族
//! fsck.c:1021-1501）、位图对照（`chkmap`/`chkword` fsck.c:786-841——按
//! 走查构造的"期望图"与盘上"实际图"逐位对比）、链接计数对账
//! （`getcount`/`chkcount`/`counterror` fsck.c:866-908）、空闲 inode
//! 清零检查（`chkilist` fsck.c:843-864）与总数报告（`printtotal`
//! fsck.c:1503-1531）。`-l` 列表（`list` fsck.c:936-968）与 `-i`/`-z`
//! 观察单（`fillbitmap` fsck.c:739-757）随走查提供。修复面
//! （`repair`/`automatic` 的交互式修复）与 preen 并行**不在本模块**——
//! `yes()` 问句与 `devwrite` 全部不出现，维持登记挂后批。
//!
//! # 检查序（C `chkdev` fsck.c:1538-1614 的只读投影）
//!
//! ```text
//! 读超块 → 基础 sanity（magic/计数/几何）→ chksuper 深检
//! → chktree 走查构造期望图与链接计数 → chkmap(zone) → chkcount
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
//! - 修复问句（`yes`）在只读模式恒"否"（fsck.c:263-266 本就如此）：
//!   所有 remove/repair/adjust 分支不出现，检查照走、报告照发、盘面
//!   零改动。
//! - 宿主镜像整幅驻内存（`mkfs::build_image` 的对偶）：C 以一块缓存
//!   流式读设备，镜像文件没有随机读问题；流式接缝（trait 抽象）留待
//!   真机块设备面（E-FSBDEV）需要时再立。

use crate::inode::DiskInode;
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

/// 一次读入的间接项数（C `CINDIR`，fsck.c:68）。
const CINDIR: usize = 128;
/// 区块级别数：`NLEVEL = NR_ZONE_NUMS - NR_DZONE_NUM + 1 = 4`
/// （fsck.c:117；直连/单间/双间/盘上第 10 槽的"更间"位）。
const ZONE_LEVELS: usize = 4;
/// FIFO 类型位（C `I_NAMED_PIPE`，minix/const.h:111）。
const TYPE_NAMED_PIPE: u32 = 0o010000;
/// 符号链接目标长度上限（C `PATH_MAX`，sys/syslimits.h:64 的 1024；
/// `chksymlinkzone` fsck.c:1175 的自洽判据）。
const PATH_MAX_LIMIT: i32 = 1024;
/// 链接计数上限（C `SHRT_MAX`；inode.rs 的 `LINK_CEILING` 同值）。
const SHRT_MAX: u32 = crate::inode::LINK_CEILING;
/// 目录走查的 `.` 在位旗标（C `DOT`，fsck.c:141）。
const DOT: u8 = 1;
/// 目录走查的 `..` 在位旗标（C `DOTDOT`，fsck.c:142）。
const DOTDOT: u8 = 2;

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

/// 走查选项（C 的 `-l`/`-i`/`-z` 旗标面；修复类旗标不在只读决定半）。
#[derive(Debug, Clone, Default)]
pub struct FsckOptions {
    /// `-l`：走查途中逐 inode 列表（C `listing`）。
    pub listing: bool,
    /// `-i`：观察的 inode 号（C `ilist`；命中只提示，不判错）。
    pub watch_inodes: Vec<u64>,
    /// `-z`：观察的区块号（C `zlist`；命中只提示，不判错）。
    pub watch_zones: Vec<u64>,
}

/// 一次完整检查的统计面（C `printtotal` 的数字来源）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FsckSummary {
    /// 普通文件数（`nregular`）。
    pub nregular: u64,
    /// 目录数（`ndirectory`）。
    pub ndirectory: u64,
    /// 块设备数（`nblkspec`）。
    pub nblkspec: u64,
    /// 字符设备数（`ncharspec`）。
    pub ncharspec: u64,
    /// 坏模式 inode 数（`nbadinode`）。
    pub nbadinode: u64,
    /// FIFO 数（`npipe`）。
    pub npipe: u64,
    /// Unix 套接字数（`nsock`）。
    pub nsock: u64,
    /// 符号链接数（`nsyml`）。
    pub nsyml: u64,
    /// 空闲 inode 数（`nfreeinode`，走查起点为总 inode 数）。
    pub nfreeinode: i64,
    /// 空闲区块数（`nfreezone`，走查起点为数据区总数）。
    pub nfreezone: i64,
    /// 按级别标到的区块数（`ztype[0..4]`：直连/单间/双间/更间）。
    pub ztype: [u64; ZONE_LEVELS],
    /// 发现的不一致总数（Rust 侧汇总；C 无单一计数器，只逐条报告）。
    pub errors: u64,
}

/// 走查的路径栈帧（C `struct stack`，fsck.c:133-137）。
struct Frame {
    /// 渲染后的名字（`printname` 语义：NUL 截断、不可打印转 `?`）。
    name: String,
    /// 本帧指向的 inode 号。
    ino: u64,
    /// `.`/`..` 在位旗标（`st_presence`）。
    presence: u8,
}

/// `fsck.mfs` 检查器：持有一幅镜像与校验后的几何，逐相位产出消息。
///
/// 消息（进度行、警告、不一致报告）按 C 的输出流序收进 [`Fsck::messages`]；
/// 检查器不改镜像——修复面未在本模块。走查状态（期望图、链接计数、
/// 路径栈）由 [`Fsck::run`] 初始化。
pub struct Fsck<'a> {
    image: &'a [u8],
    sup: SuperInfo,
    messages: Vec<String>,
    /// 走查构造的 inode 期望图（`imap`）。
    imap: Bitmap,
    /// 走查构造的区块期望图（`zmap`）。
    zmap: Bitmap,
    /// 已访问目录表（`dirmap`：目录不得有第二链接）。
    dirmap: Bitmap,
    /// 观察单 inode 图（`spec_imap`）。
    spec_imap: Bitmap,
    /// 观察单区块图（`spec_zmap`）。
    spec_zmap: Bitmap,
    /// 链接计数台账（`count[]`：每找到一个链接 +1，访问到 inode 减去
    /// 盘上链接数，收尾应全零）。
    counts: Vec<i64>,
    /// 路径栈（C 的 `ftop` 链）。
    path: Vec<Frame>,
    /// 类型与空闲计数（`printtotal` 的数字面）。
    stats: FsckSummary,
    /// `-l` 列表开关。
    listing: bool,
    /// 列表头是否已印（C `firstlist`）。
    firstlist: bool,
    /// 计数错误表头是否已印（C `firstcnterr`）。
    firstcnterr: bool,
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
            // 走查状态在 run() 里按几何重建；位 0 恒置（allocbitmap
            // 的 `*bitmap |= 1`，fsck.c:707）先立在这里。
            imap: Bitmap::new(1),
            zmap: Bitmap::new(1),
            dirmap: Bitmap::new(1),
            spec_imap: Bitmap::new(1),
            spec_zmap: Bitmap::new(1),
            counts: Vec::new(),
            path: Vec::new(),
            stats: FsckSummary::default(),
            listing: false,
            firstlist: true,
            firstcnterr: true,
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
        compare_map(self.image, &self.sup, kind, expected, &mut self.messages)
    }

    /// 从镜像读一字节（越界补零：C `devread` 读失败时零填充续走，
    /// fsck.c:429-432；几何校验后正常到不了边界）。
    fn read_bytes(&self, at: u64, buf: &mut [u8]) {
        let at = at as usize;
        for (index, out) in buf.iter_mut().enumerate() {
            *out = self.image.get(at + index).copied().unwrap_or(0);
        }
    }

    /// 读一个 inode 记录（`inoblock`/`inooff` 的算式，fsck.c:657-665）。
    fn read_inode(&self, ino: u64) -> DiskInode {
        let isz = crate::superblock::INODE_DISK_SIZE as u64;
        let offset = (ino - 1) * isz;
        let block = self.sup.inode_table_block() + offset / self.sup.block_size;
        let at = block * self.sup.block_size + offset % self.sup.block_size;
        let mut buf = [0u8; crate::superblock::INODE_DISK_SIZE];
        self.read_bytes(at, &mut buf);
        // 64 字节缓冲不可能短读；Corrupt 分支按全零盘面处理。
        DiskInode::from_bytes(&buf).unwrap_or(DiskInode {
            mode: 0,
            nlinks: 0,
            owner: 0,
            group: 0,
            size: 0,
            accessed: 0,
            modified: 0,
            changed: 0,
            zones: [0; crate::inode::TOTAL_ZONES],
        })
    }

    /// 区块号到字节地址（`ztob(z) = z << log_zone_size` 后乘块大小，
    /// fsck.c:99-100）。
    fn zone_byte_at(&self, zno: u64) -> u64 {
        (zno << self.sup.log_zone_size) * self.sup.block_size
    }

    /// 文件空洞的跨度（C `jump`，fsck.c:1262-1271：`ZONE_SIZE` 起，
    /// 每级乘每区块的间接项数）。
    fn jump(&self, level: u32) -> i64 {
        let mut power = self.sup.zone_bytes() as i64;
        for _ in 0..level {
            power *= (self.sup.block_size / 4) as i64;
        }
        power
    }

    /// 当前路径（C `printpath(0)`：根帧单独成 `/`，其余逐级 `/` 连接；
    /// `printrec` fsck.c:354-361 的渲染）。
    fn path_none(&self) -> String {
        if self.path.len() <= 1 {
            return String::from("/");
        }
        let mut out = String::new();
        for frame in &self.path[1..] {
            out.push('/');
            out.push_str(&frame.name);
        }
        out
    }

    /// 当前路径带开口括号（`printpath(1)`：`{路径} (ino = N, `）。
    fn path_open(&self) -> String {
        format!(
            "{} (ino = {}, ",
            self.path_none(),
            self.path.last().map_or(0, |f| f.ino)
        )
    }

    /// 当前路径带闭括号（`printpath(2)`：`{路径} (ino = N)`）。
    fn path_full(&self) -> String {
        format!(
            "{} (ino = {})",
            self.path_none(),
            self.path.last().map_or(0, |f| f.ino)
        )
    }

    /// 区块报告的公共尾（C `errzone`，fsck.c:1196-1213）。
    fn errzone(&mut self, mess: &str, zno: u64, level: u32, pos: i64) {
        let kind = match level {
            0 => "DATA",
            1 => "SINGLE INDIRECT",
            2 => "DOUBLE INDIRECT",
            _ => "VERY INDIRECT",
        };
        self.messages.push(format!(
            "{} zone in {}zno = {}, type = {}, pos = {})",
            mess,
            self.path_open(),
            zno,
            kind,
            pos
        ));
    }

    /// 走查状态的建立（C `initvars` + `getbitmaps` + `fillbitmap` +
    /// `getcount`，fsck.c:226-238/:766-784/:739-757/:866-870）。
    fn init_walk(&mut self, options: &FsckOptions) {
        let bs = self.sup.block_size;
        let imap_bits = self.sup.inode_map_blocks * bits_per_block(bs);
        let zmap_bits = self.sup.zone_map_blocks * bits_per_block(bs);
        self.imap = Bitmap::new(imap_bits);
        self.dirmap = Bitmap::new(imap_bits);
        self.spec_imap = Bitmap::new(imap_bits);
        self.zmap = Bitmap::new(zmap_bits);
        self.spec_zmap = Bitmap::new(zmap_bits);
        self.counts = alloc::vec![0; (self.sup.inodes + 1) as usize];
        self.path = Vec::new();
        self.stats = FsckSummary {
            nfreeinode: self.sup.inodes as i64,
            nfreezone: (self.sup.zones - self.sup.first_data_zone) as i64,
            ..FsckSummary::default()
        };
        self.listing = options.listing;
        self.firstlist = true;
        self.firstcnterr = true;
        // 观察单：越界忽略并提示（fillbitmap 的 range 分支，fsck.c:749-754）。
        for &ino in &options.watch_inodes {
            if ino < 1 || ino > self.sup.inodes {
                self.messages
                    .push(format!("inode number {} out of range (ignored)", ino));
            } else {
                self.spec_imap.set(ino);
            }
        }
        for &zno in &options.watch_zones {
            if zno < self.sup.first_data_zone || zno >= self.sup.zones {
                self.messages
                    .push(format!("zone number {} out of range (ignored)", zno));
            } else {
                self.spec_zmap.set(zno - self.sup.first_data_zone + 1);
            }
        }
    }

    /// 完整检查（C `chkdev` 只读投影的走查半，fsck.c:1556-1591 的序）：
    /// 走查 → 区块图对账 → 计数对账 → inode 图对账 → 空闲表清零检查
    /// → 总数。返回 [`FsckSummary`]；致命项（根非目录、符号链接超长）
    /// 走 `Err`。
    pub fn run(&mut self, options: &FsckOptions) -> Result<FsckSummary, Fatal> {
        self.init_walk(options);
        // chktree（fsck.c:1490-1501）：伪目录项进根。
        let root_ok = self.descendtree(crate::superblock::ROOT_INODE_NUMBER, &[])?;
        if !root_ok {
            return Err(Fatal {
                message: String::from("bad root inode"),
            });
        }
        // C 的序：zone 图 → 计数 → inode 图 → 空闲表 → 总数。
        let zerr = compare_map(
            self.image,
            &self.sup,
            MapKind::Zone,
            &self.zmap,
            &mut self.messages,
        );
        self.stats.errors += zerr;
        self.chkcount();
        let ierr = compare_map(
            self.image,
            &self.sup,
            MapKind::Inode,
            &self.imap,
            &mut self.messages,
        );
        self.stats.errors += ierr;
        self.chkilist();
        self.printtotal();
        Ok(self.stats)
    }

    /// 链接计数对账（C `chkcount`，fsck.c:901-908）。
    fn chkcount(&mut self) {
        for ino in 1..=self.sup.inodes {
            if self.counts[ino as usize] != 0 {
                self.counterror(ino);
            }
        }
    }

    /// 单个 inode 的计数差报告（C `counterror`，fsck.c:873-892 的只读
    /// 半：表头一次、回加盘上链接数、三列 `ino nlink 计数`）。
    fn counterror(&mut self, ino: u64) {
        if self.firstcnterr {
            self.messages
                .push(String::from("INODE NLINK COUNT"));
            self.firstcnterr = false;
        }
        let inode = self.read_inode(ino);
        self.counts[ino as usize] += inode.nlinks as i64;
        self.messages.push(format!(
            "{:5} {:5} {:5}",
            ino,
            inode.nlinks,
            self.counts[ino as usize]
        ));
        self.stats.errors += 1;
    }

    /// 空闲 inode 清零检查（C `chkilist`，fsck.c:843-864 的只读半）。
    fn chkilist(&mut self) {
        self.messages.push(String::from("Checking inode list."));
        for ino in 1..=self.sup.inodes {
            if !self.imap.test(ino) {
                let inode = self.read_inode(ino);
                if inode.mode != crate::inode::NOT_ALLOCATED {
                    self.messages
                        .push(format!("mode inode {} not cleared", ino));
                    self.stats.errors += 1;
                }
            }
        }
    }

    /// 总数报告（C `printtotal`，fsck.c:1503-1531 的非 preen 面）。
    fn printtotal(&mut self) {
        self.messages.push(format!(
            "blocksize = {:5}        zonesize  = {:5}",
            self.sup.block_size,
            self.sup.zone_bytes()
        ));
        let s = &self.stats;
        let plural = |n: u64| if n == 1 { "" } else { "s" };
        self.messages.push(format!("{:8}    Regular file{}", s.nregular, plural(s.nregular)));
        self.messages.push(format!(
            "{:8}    Director{}",
            s.ndirectory,
            if s.ndirectory == 1 { "y" } else { "ies" }
        ));
        self.messages
            .push(format!("{:8}    Block special file{}", s.nblkspec, plural(s.nblkspec)));
        self.messages.push(format!(
            "{:8}    Character special file{}",
            s.ncharspec,
            plural(s.ncharspec)
        ));
        if s.nbadinode != 0 {
            self.messages
                .push(format!("{:8}    Bad inode{}", s.nbadinode, plural(s.nbadinode)));
        }
        self.messages.push(format!(
            "{:8}    Free inode{}",
            s.nfreeinode,
            plural(s.nfreeinode as u64)
        ));
        self.messages
            .push(format!("{:8}    Named pipe{}", s.npipe, plural(s.npipe)));
        self.messages
            .push(format!("{:8}    Unix socket{}", s.nsock, plural(s.nsock)));
        self.messages
            .push(format!("{:8}    Symbolic link{}", s.nsyml, plural(s.nsyml)));
        self.messages.push(format!(
            "{:8}    Free zone{}",
            s.nfreezone,
            plural(s.nfreezone as u64)
        ));
    }

    /// 走查入口（C `descendtree`，fsck.c:1454-1488 的只读半）：压栈、
    /// 未访问则读 inode 检查、弹栈。`raw_name` 是目录项名字缓冲（可空）。
    fn descendtree(&mut self, ino: u64, raw_name: &[u8]) -> Result<bool, Fatal> {
        self.path.push(Frame {
            name: render_name(raw_name),
            ino,
            presence: 0,
        });
        if self.spec_imap.test(ino) {
            let path = self.path_none();
            self.messages.push(format!("found inode {}: {}", ino, path));
        }
        let visited = self.imap.test(ino);
        if !visited || self.listing {
            let inode = self.read_inode(ino);
            if self.listing {
                self.list_inode(ino, &inode);
            }
            if !visited && !self.chkinode(ino, &inode)? {
                // 只读：remove 问句恒否，标注后继续。
                self.spec_imap.set(ino);
            }
        }
        self.path.pop();
        Ok(true)
    }

    /// inode 检查（C `chkinode`，fsck.c:1427-1451）。
    fn chkinode(&mut self, ino: u64, inode: &DiskInode) -> Result<bool, Fatal> {
        if ino == crate::superblock::ROOT_INODE_NUMBER
            && inode.mode as u32 & crate::inode::TYPE_MASK != crate::inode::TYPE_DIRECTORY
        {
            return Err(Fatal {
                message: format!(
                    "root inode is not a directory (ino = {}, mode = {:o})",
                    ino, inode.mode
                ),
            });
        }
        if inode.nlinks == 0 {
            let path = self.path_full();
            self.messages.push(format!("link count zero of {}", path));
            self.stats.errors += 1;
            return Ok(false);
        }
        self.stats.nfreeinode -= 1;
        self.imap.set(ino);
        if inode.nlinks as u32 > SHRT_MAX {
            let path = self.path_open();
            self.messages.push(format!(
                "link count too big in {}cnt = {})",
                path, inode.nlinks
            ));
            self.counts[ino as usize] -= SHRT_MAX as i64;
            self.spec_imap.set(ino);
            self.stats.errors += 1;
        } else {
            self.counts[ino as usize] -= inode.nlinks as i64;
        }
        self.chkmode(ino, inode)
    }

    /// 按类型分派（C `chkmode`，fsck.c:1391-1424）。
    fn chkmode(&mut self, ino: u64, inode: &DiskInode) -> Result<bool, Fatal> {
        match inode.mode as u32 & crate::inode::TYPE_MASK {
            crate::inode::TYPE_REGULAR => {
                self.stats.nregular += 1;
                self.chkfile(ino, inode)
            }
            crate::inode::TYPE_DIRECTORY => {
                self.stats.ndirectory += 1;
                self.chkdirectory(ino, inode)
            }
            crate::inode::TYPE_BLOCK => {
                self.stats.nblkspec += 1;
                self.chkspecial(ino, inode)
            }
            crate::inode::TYPE_CHARACTER => {
                self.stats.ncharspec += 1;
                self.chkspecial(ino, inode)
            }
            TYPE_NAMED_PIPE => {
                self.stats.npipe += 1;
                self.chkfile(ino, inode)
            }
            crate::inode::TYPE_SOCKET => {
                self.stats.nsock += 1;
                self.chkfile(ino, inode)
            }
            crate::inode::TYPE_SYMLINK => {
                self.stats.nsyml += 1;
                self.chklink(ino, inode)
            }
            _ => {
                self.stats.nbadinode += 1;
                let path = self.path_open();
                self.messages
                    .push(format!("bad mode of {}mode = {:o})", path, inode.mode));
                self.stats.errors += 1;
                Ok(false)
            }
        }
    }

    /// 文件的区块走查（C `chkfile`，fsck.c:1313-1322）：七个直连槽
    /// level 0，其余三槽 level 1..4。
    fn chkfile(&mut self, ino: u64, inode: &DiskInode) -> Result<bool, Fatal> {
        let mut pos: i64 = 0;
        let direct = inode.zones[..crate::inode::DIRECT_ZONES].to_vec();
        let mut ok = self.chkzones(ino, inode, &mut pos, &direct, 0)?;
        for slot in crate::inode::DIRECT_ZONES..crate::inode::TOTAL_ZONES {
            let level = (slot - crate::inode::DIRECT_ZONES + 1) as u32;
            let single = [inode.zones[slot]];
            ok &= self.chkzones(ino, inode, &mut pos, &single, level)?;
        }
        Ok(ok)
    }

    /// 区块列表走查（C `chkzones`，fsck.c:1292-1310）：洞跳过、标记
    /// 失败跳过并记错、成功则进内容检查。
    fn chkzones(
        &mut self,
        ino: u64,
        inode: &DiskInode,
        pos: &mut i64,
        zlist: &[u32],
        level: u32,
    ) -> Result<bool, Fatal> {
        let mut ok = true;
        for &zno in zlist {
            if zno == 0 {
                *pos += self.jump(level);
            } else if !self.markzone(zno, level, *pos) {
                *pos += self.jump(level);
                ok = false;
            } else if !self.zonechk(ino, inode, pos, zno, level)? {
                ok = false;
            }
        }
        Ok(ok)
    }

    /// 区块标记（C `markzone`，fsck.c:1218-1239）：范围、重复、观察单
    /// 命中三态，命中记入期望图并扣空闲计数。
    fn markzone(&mut self, zno: u32, level: u32, pos: i64) -> bool {
        let zno = zno as u64;
        self.stats.ztype[level as usize] += 1;
        if zno < self.sup.first_data_zone || zno >= self.sup.zones {
            self.errzone("out-of-range", zno, level, pos);
            self.stats.errors += 1;
            return false;
        }
        let bit = zno - self.sup.first_data_zone + 1;
        if self.zmap.test(bit) {
            self.spec_zmap.set(bit);
            self.errzone("duplicate", zno, level, pos);
            self.stats.errors += 1;
            return false;
        }
        self.stats.nfreezone -= 1;
        if self.spec_zmap.test(bit) {
            // 观察单命中：提示而非错误（C 只报不计）。
            self.errzone("found", zno, level, pos);
        }
        self.zmap.set(bit);
        true
    }

    /// 单区块内容分派（C `zonechk`，fsck.c:1276-1289）。
    fn zonechk(
        &mut self,
        ino: u64,
        inode: &DiskInode,
        pos: &mut i64,
        zno: u32,
        level: u32,
    ) -> Result<bool, Fatal> {
        if level == 0 {
            let kind = inode.mode as u32 & crate::inode::TYPE_MASK;
            if kind == crate::inode::TYPE_DIRECTORY && !self.chkdirzone(ino, inode, *pos, zno)? {
                return Ok(false);
            }
            if kind == crate::inode::TYPE_SYMLINK
                && !self.chksymlinkzone(ino, inode, *pos, zno)?
            {
                return Ok(false);
            }
            *pos += self.sup.zone_bytes() as i64;
            Ok(true)
        } else {
            self.chkindzone(ino, inode, pos, zno, level)
        }
    }

    /// 间接区块走查（C `chkindzone`，fsck.c:1244-1257）：按 `CINDIR`
    /// 分块读，位置越过文件尺寸即止。
    fn chkindzone(
        &mut self,
        ino: u64,
        inode: &DiskInode,
        pos: &mut i64,
        zno: u32,
        level: u32,
    ) -> Result<bool, Fatal> {
        let ind = (self.sup.block_size / 4) as usize;
        let mut chunks = ind / CINDIR;
        let mut offset = 0usize;
        loop {
            let mut buf = alloc::vec![0u8; CINDIR * 4];
            self.read_bytes(self.zone_byte_at(zno as u64) + offset as u64, &mut buf);
            let entries: Vec<u32> = buf
                .chunks_exact(4)
                .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                .collect();
            if !self.chkzones(ino, inode, pos, &entries, level - 1)? {
                return Ok(false);
            }
            offset += CINDIR * 4;
            chunks -= 1;
            if chunks == 0 || *pos >= inode.size as i64 {
                return Ok(true);
            }
        }
    }

    /// 目录区块走查（C `chkdirzone`，fsck.c:1132-1166 的只读半）：
    /// 逐项分派 [`Fsck::chkentry`]，尾尺寸大于 inode 尺寸报"未更新"。
    fn chkdirzone(
        &mut self,
        ino: u64,
        inode: &DiskInode,
        pos: i64,
        zno: u32,
    ) -> Result<bool, Fatal> {
        let per_block = self.sup.block_size / crate::inode::DIRECTORY_ENTRY_SIZE as u64;
        let total = (1u64 << self.sup.log_zone_size) * per_block;
        let zone_at = self.zone_byte_at(zno as u64);
        let mut entry_pos = pos;
        let mut size: i64 = 0;
        for index in 0..total {
            let mut buf = [0u8; crate::inode::DIRECTORY_ENTRY_SIZE];
            self.read_bytes(
                zone_at + index * crate::inode::DIRECTORY_ENTRY_SIZE as u64,
                &mut buf,
            );
            let inum = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
            if inum != crate::inode::NO_ENTRY as u32
                && !self.chkentry(ino, entry_pos, inum as u64, &buf[4..])?
            {
                // C 的 dirty 面只在修复时有意义；只读径直继续。
            }
            entry_pos += crate::inode::DIRECTORY_ENTRY_SIZE as i64;
            if inum != crate::inode::NO_ENTRY as u32 {
                size = entry_pos;
            }
        }
        if size > inode.size as i64 {
            let path = self.path_full();
            self.messages
                .push(format!("size not updated of directory {}", path));
            self.stats.errors += 1;
        }
        Ok(true)
    }

    /// 符号链接目标检查（C `chksymlinkzone`，fsck.c:1169-1194）：尺寸
    /// 越过 PATH_MAX 是检查器自洽破坏（fatal）；NUL 位置与尺寸不符报
    /// 坏尺寸。
    fn chksymlinkzone(
        &mut self,
        _ino: u64,
        inode: &DiskInode,
        _pos: i64,
        zno: u32,
    ) -> Result<bool, Fatal> {
        if inode.size > PATH_MAX_LIMIT {
            return Err(Fatal {
                message: String::from("chksymlinkzone: fsck program inconsistency"),
            });
        }
        let mut buf = alloc::vec![0u8; inode.size.max(0) as usize];
        self.read_bytes(self.zone_byte_at(zno as u64), &mut buf);
        let len = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        if len as i64 != inode.size as i64 {
            let path = self.path_full();
            self.messages.push(format!(
                "bad size in symbolic link ({} instead of {}) {}",
                inode.size, len, path
            ));
            self.stats.errors += 1;
        }
        Ok(true)
    }

    /// 目录内容与 `.`/`..` 检查（C `chkdirectory`，fsck.c:1325-1342）。
    fn chkdirectory(&mut self, ino: u64, inode: &DiskInode) -> Result<bool, Fatal> {
        self.dirmap.set(ino);
        let mut ok = self.chkfile(ino, inode)?;
        let presence = self.path.last().map_or(0, |f| f.presence);
        if presence & DOT == 0 {
            let path = self.path_full();
            self.messages.push(format!(". missing in {}", path));
            self.stats.errors += 1;
            ok = false;
        }
        if presence & DOTDOT == 0 {
            let path = self.path_full();
            self.messages.push(format!(".. missing in {}", path));
            self.stats.errors += 1;
            ok = false;
        }
        Ok(ok)
    }

    /// 符号链接整体检查（C `chklink`，fsck.c:1347-1361）：空链接与超
    /// 块尺寸都报。
    fn chklink(&mut self, ino: u64, inode: &DiskInode) -> Result<bool, Fatal> {
        let mut ok = self.chkfile(ino, inode)?;
        if inode.size <= 0 || inode.size > self.sup.block_size as i32 {
            let path = self.path_full();
            if inode.size == 0 {
                self.messages.push(format!("empty symbolic link {}", path));
            } else {
                self.messages.push(format!(
                    "symbolic link too large (size {}) {}",
                    inode.size, path
                ));
            }
            self.stats.errors += 1;
            ok = false;
        }
        Ok(ok)
    }

    /// 设备文件检查（C `chkspecial`，fsck.c:1366-1388）：0 号设备非
    /// 法，其余槽位必须清零。
    fn chkspecial(&mut self, _ino: u64, inode: &DiskInode) -> Result<bool, Fatal> {
        let mut ok = true;
        let path = self.path_full();
        if inode.zones[0] == 0 {
            self.messages.push(format!(
                "illegal device number {} for special file {}",
                inode.zones[0], path
            ));
            self.stats.errors += 1;
            ok = false;
        }
        for &zno in &inode.zones[1..] {
            if zno != 0 {
                self.messages.push(format!(
                    "nonzero zone number {} for special file {}",
                    zno, path
                ));
                self.stats.errors += 1;
                ok = false;
            }
        }
        Ok(ok)
    }

    /// 目录项检查（C `chkentry`，fsck.c:1084-1127 的只读半）：inode 号
    /// 范围、计数上限、`.`/`..` 对账、名字合法性、目录第二链接、递归。
    fn chkentry(
        &mut self,
        dir_ino: u64,
        pos: i64,
        inum: u64,
        raw_name: &[u8],
    ) -> Result<bool, Fatal> {
        if inum < crate::superblock::ROOT_INODE_NUMBER || inum > self.sup.inodes {
            let path = self.path_open();
            self.messages.push(format!(
                "bad inode found in directory {}ino found = {}, name = '{}')",
                path,
                inum,
                render_name(raw_name)
            ));
            self.stats.errors += 1;
            return Ok(true);
        }
        if self.counts[inum as usize] == SHRT_MAX as i64 {
            let path = self.path_none();
            self.messages.push(format!(
                "too many links to ino {}\ndiscovered at entry '{}' in directory {}\n",
                inum,
                render_name(raw_name),
                path
            ));
            self.stats.errors += 1;
            // 只读：Remove 问句恒否，计数照走。
        }
        self.counts[inum as usize] += 1;
        let name = cstr_bytes(raw_name);
        if name == b"." {
            if let Some(frame) = self.path.last_mut() {
                frame.presence |= DOT;
            }
            return Ok(self.chkdots(dir_ino, pos, inum, raw_name, dir_ino));
        }
        if name == b".." {
            if let Some(frame) = self.path.last_mut() {
                frame.presence |= DOTDOT;
            }
            // 父目录号：根的 `..` 指根，其余取栈下一帧（fsck.c:1114-1115）。
            let expected = if dir_ino == crate::superblock::ROOT_INODE_NUMBER {
                dir_ino
            } else {
                self.path
                    .get(self.path.len() - 2)
                    .map_or(dir_ino, |f| f.ino)
            };
            return Ok(self.chkdots(dir_ino, pos, inum, raw_name, expected));
        }
        if !self.chkname(dir_ino, raw_name) {
            return Ok(false);
        }
        if self.dirmap.test(inum) {
            let path = self.path_open();
            self.messages.push(format!(
                "link to directory discovered in {}name = '{}', dir ino = {})",
                path,
                render_name(raw_name),
                inum
            ));
            self.stats.errors += 1;
            return Ok(true);
        }
        self.descendtree(inum, raw_name)
    }

    /// `.`/`..` 指向与位置对账（C `chkdots`，fsck.c:1022-1053 的只读
    /// 半）：指向错为错误，位置错为告警；三处观察单标注照发。
    fn chkdots(
        &mut self,
        dir_ino: u64,
        pos: i64,
        inum: u64,
        raw_name: &[u8],
        expected: u64,
    ) -> bool {
        let rendered = render_name(raw_name);
        let second = raw_name.get(1).copied().unwrap_or(0);
        let expected_pos = if second != 0 {
            crate::inode::DIRECTORY_ENTRY_SIZE as i64
        } else {
            0
        };
        if inum != expected {
            let path = self.path_open();
            self.messages.push(format!(
                "bad {} in {}{} is linked to {} instead of {})",
                rendered, path, rendered, inum, expected
            ));
            self.spec_imap.set(dir_ino);
            self.spec_imap.set(inum);
            self.spec_imap.set(expected);
            self.stats.errors += 1;
        } else if pos != expected_pos {
            let path = self.path_open();
            self.messages.push(format!(
                "warning: {} has offset {} in {}{} is linked to {})",
                rendered, pos, path, rendered, inum
            ));
            self.spec_imap.set(dir_ino);
            self.spec_imap.set(inum);
            self.spec_imap.set(expected);
            self.stats.errors += 1;
        }
        true
    }

    /// 名字合法性（C `chkname`，fsck.c:1056-1079 的只读半）：空名与
    /// 含 `/` 都报并标注目录。
    fn chkname(&mut self, dir_ino: u64, raw_name: &[u8]) -> bool {
        if raw_name.first().copied().unwrap_or(0) == 0 {
            let path = self.path_none();
            self.messages.push(format!("null name found in {}", path));
            self.spec_imap.set(dir_ino);
            self.stats.errors += 1;
            return true;
        }
        for &byte in cstr_bytes(raw_name) {
            if byte == b'/' {
                let path = self.path_open();
                self.messages.push(format!(
                    "found a '/' in entry of directory {}entry = '{}')",
                    path,
                    render_name(raw_name)
                ));
                self.spec_imap.set(dir_ino);
                self.stats.errors += 1;
                break;
            }
        }
        true
    }

    /// inode 列表行（C `list`，fsck.c:936-968）：类型字符、九位权限、
    /// 链接数、尺寸或设备号、路径。
    fn list_inode(&mut self, ino: u64, inode: &DiskInode) {
        if self.firstlist {
            self.messages
                .push(String::from(" inode permission link   size name"));
            self.firstlist = false;
        }
        let kind = inode.mode as u32 & crate::inode::TYPE_MASK;
        let type_char = match kind {
            crate::inode::TYPE_REGULAR => '-',
            crate::inode::TYPE_DIRECTORY => 'd',
            crate::inode::TYPE_CHARACTER => 'c',
            crate::inode::TYPE_BLOCK => 'b',
            TYPE_NAMED_PIPE => 'p',
            crate::inode::TYPE_SOCKET => 's',
            crate::inode::TYPE_SYMLINK => 'l',
            _ => '?',
        };
        // printperm：九位权限，特殊位叠字（s/s/t）。
        let mut perms = String::new();
        for (shift, special, overlay) in [
            (6u32, 0o4000u32, 's'),
            (3, 0o2000, 's'),
            (0, 0o1000, 't'),
        ] {
            let bit = |mask: u32| inode.mode as u32 >> shift & mask != 0;
            perms.push(if bit(4) { 'r' } else { '-' });
            perms.push(if bit(2) { 'w' } else { '-' });
            perms.push(if inode.mode as u32 & special != 0 {
                overlay
            } else if bit(1) {
                'x'
            } else {
                '-'
            });
        }
        let body = match kind {
            crate::inode::TYPE_CHARACTER | crate::inode::TYPE_BLOCK => format!(
                "  {:2x},{:2x} ",
                inode.zones[0] >> 8,
                inode.zones[0] & 0xff
            ),
            _ => format!("{:7} ", inode.size),
        };
        let path = self.path_none();
        self.messages.push(format!(
            "{:6} {}{} {:3} {}{}",
            ino, type_char, perms, inode.nlinks, body, path
        ));
    }
}

/// 位图对照核（C `chkmap` fsck.c:813-841 的逐位对账）：`expected` 与
/// 盘上图逐位比对，`chkword` 语义报告（盘置/期清 → "missing"，盘清/
/// 期置 → "not free"），MAXPRINT 截断。返回差异数。
///
/// 自由函数而非方法：`run` 里期望图与消息同为 `self` 字段，拆字段
/// 借用在此汇总。
fn compare_map(
    image: &[u8],
    sup: &SuperInfo,
    kind: MapKind,
    expected: &Bitmap,
    messages: &mut Vec<String>,
) -> u64 {
    messages.push(format!("Checking {} map.", kind.label()));
    let blocks = kind.blocks(sup);
    let total_bits = blocks * bits_per_block(sup.block_size);
    debug_assert_eq!(
        expected.bit_count(),
        total_bits,
        "expected map must cover the full on-disk map blocks"
    );
    let byte_base = (kind.base_block(sup) * sup.block_size) as usize;
    let disk_bit = |bit: u64| {
        image
            .get(byte_base + (bit / 8) as usize)
            .is_some_and(|byte| byte & (1 << (bit % 8)) != 0)
    };
    let mut nerr: u64 = 0;
    let mut report = true;
    for bit in 0..total_bits {
        let on_disk = disk_bit(bit);
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
            let number = kind.first_reported(sup) + bit;
            match (on_disk, wanted) {
                (true, false) => messages.push(format!(
                    "{} {} is missing",
                    kind.label(),
                    number
                )),
                (false, true) => messages.push(format!(
                    "{} {} is not free",
                    kind.label(),
                    number
                )),
                _ => unreachable!("bits differ by construction"),
            }
        }
    }
    if !report {
        messages.push(String::from("etc."));
    }
    if nerr > MAXPRINT || nerr > 10 {
        messages.push(format!("{} errors found.", nerr));
    }
    nerr
}

/// 目录项名字渲染（C `printname`，fsck.c:337-349：NUL 截断，不可打印
/// 字符转 `?`）。
fn render_name(raw: &[u8]) -> String {
    let mut out = String::new();
    for &byte in raw.iter().take(crate::inode::NAME_CAPACITY) {
        if byte == 0 {
            break;
        }
        out.push(if (0x20..=0x7E).contains(&byte) {
            byte as char
        } else {
            '?'
        });
    }
    out
}

/// 名字缓冲的 C 串视图（到首个 NUL 为止，无 NUL 则全缓冲）。
fn cstr_bytes(raw: &[u8]) -> &[u8] {
    let end = raw
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(raw.len());
    &raw[..end]
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

    // ---- 批二：走查、计数与目录检查 ----
    //
    // 基准布局（64 块、64 inode、4096 字节）：imap 在块 2、zmap 在块 3、
    // inode 表在块 4、首数据区 5。根 inode 是 1 号，根目录数据区是 5。

    const IMAP_BLOCK: u64 = 2;
    const ZMAP_BLOCK: u64 = 3;
    const INODE_TABLE_BLOCK: u64 = 4;
    const FIRST_ZONE: u64 = 5;

    fn set_map_bit(image: &mut [u8], map_block: u64, bit: u64) {
        let at = (map_block * 4096 + bit / 8) as usize;
        image[at] |= 1 << (bit % 8);
    }

    fn write_inode(image: &mut [u8], ino: u64, inode: &DiskInode) {
        let at = (INODE_TABLE_BLOCK * 4096 + (ino - 1) * 64) as usize;
        image[at..at + 64].copy_from_slice(&inode.to_bytes());
    }

    fn write_entry(image: &mut [u8], zone: u64, slot: usize, name: &[u8], inum: u64) {
        let at = (zone * 4096 + slot as u64 * 64) as usize;
        image[at..at + 4].copy_from_slice(&(inum as u32).to_le_bytes());
        let n = name.len().min(60);
        image[at + 4..at + 4 + n].copy_from_slice(&name[..n]);
    }

    fn clear_entry(image: &mut [u8], zone: u64, slot: usize) {
        let at = (zone * 4096 + slot as u64 * 64) as usize;
        image[at..at + 64].fill(0);
    }

    /// 目录 inode 的标准形状（`.`/`..` 两条目起）。
    fn dir_inode(nlinks: u16, size: i32, zone: u64) -> DiskInode {
        DiskInode {
            mode: (crate::inode::TYPE_DIRECTORY | 0o777) as u16,
            nlinks,
            owner: 0,
            group: 0,
            size,
            accessed: 1_000,
            modified: 1_000,
            changed: 1_000,
            zones: {
                let mut z = [0u32; crate::inode::TOTAL_ZONES];
                z[0] = zone as u32;
                z
            },
        }
    }

    /// 普通文件 inode。
    fn file_inode(nlinks: u16, size: i32, zones: &[u32]) -> DiskInode {
        let mut inode = DiskInode {
            mode: (crate::inode::TYPE_REGULAR | 0o644) as u16,
            nlinks,
            owner: 0,
            group: 0,
            size,
            accessed: 1_000,
            modified: 1_000,
            changed: 1_000,
            zones: [0u32; crate::inode::TOTAL_ZONES],
        };
        for (slot, zone) in zones.iter().enumerate() {
            inode.zones[slot] = *zone;
        }
        inode
    }

    #[test]
    fn clean_empty_fs_full_run_is_clean() {
        let (image, _plan) = base_image();
        let mut fsck = Fsck::new(&image).unwrap();
        let summary = fsck.run(&FsckOptions::default()).unwrap();
        assert_eq!(summary.errors, 0);
        assert_eq!(summary.ndirectory, 1);
        assert_eq!(summary.nfreeinode, 63);
        assert_eq!(summary.nfreezone, 58, "64 - 首数据区 5 - 根占 1");
        assert_eq!(summary.ztype, [1, 0, 0, 0]);
        // 检查序的进度行与总数表都在。
        assert!(fsck
            .messages()
            .contains(&String::from("Checking zone map.")));
        assert!(fsck
            .messages()
            .contains(&String::from("Checking inode list.")));
        assert!(fsck.messages().contains(&String::from(
            "blocksize =  4096        zonesize  =  4096"
        )));
        assert!(fsck
            .messages()
            .contains(&String::from("       0    Regular files")));
        assert!(fsck
            .messages()
            .contains(&String::from("       1    Directory")));
        assert!(fsck
            .messages()
            .contains(&String::from("      63    Free inodes")));
        assert!(fsck
            .messages()
            .contains(&String::from("      58    Free zones")));
    }

    #[test]
    fn link_count_mismatch_is_tallied() {
        let (mut image, _plan) = base_image();
        // 文件 f（inode 2）盘上链接数 2，目录里只有一个入口。
        write_inode(
            &mut image,
            1,
            &dir_inode(2, 3 * 64, FIRST_ZONE),
        );
        write_inode(&mut image, 2, &file_inode(2, 0, &[]));
        write_entry(&mut image, FIRST_ZONE, 2, b"f", 2);
        set_map_bit(&mut image, IMAP_BLOCK, 2);
        let mut fsck = Fsck::new(&image).unwrap();
        let summary = fsck.run(&FsckOptions::default()).unwrap();
        assert!(summary.errors >= 1);
        assert!(fsck
            .messages()
            .contains(&String::from("INODE NLINK COUNT")));
        // 三列：ino、盘上链接数、目录里找到的入口数。
        assert!(fsck
            .messages()
            .contains(&String::from("    2     2     1")));
        assert_eq!(summary.nregular, 1);
    }

    #[test]
    fn dangling_entry_inode_reports_bad_inode() {
        let (mut image, _plan) = base_image();
        write_inode(&mut image, 1, &dir_inode(2, 3 * 64, FIRST_ZONE));
        write_entry(&mut image, FIRST_ZONE, 2, b"x", 99);
        let mut fsck = Fsck::new(&image).unwrap();
        fsck.run(&FsckOptions::default()).unwrap();
        assert!(fsck.messages().contains(&String::from(
            "bad inode found in directory / (ino = 1, ino found = 99, name = 'x')"
        )));
    }

    #[test]
    fn missing_dotdot_reported() {
        let (mut image, _plan) = base_image();
        clear_entry(&mut image, FIRST_ZONE, 1);
        let mut fsck = Fsck::new(&image).unwrap();
        let summary = fsck.run(&FsckOptions::default()).unwrap();
        assert!(fsck
            .messages()
            .contains(&String::from(".. missing in / (ino = 1)")));
        // `.` 仍在而 `..` 缺席：根的计数差一条。
        assert!(summary.errors >= 2);
    }

    #[test]
    fn out_of_range_zone_reported() {
        let (mut image, _plan) = base_image();
        write_inode(&mut image, 1, &dir_inode(2, 3 * 64, FIRST_ZONE));
        write_inode(&mut image, 2, &file_inode(1, 4096, &[999]));
        write_entry(&mut image, FIRST_ZONE, 2, b"f", 2);
        set_map_bit(&mut image, IMAP_BLOCK, 2);
        let mut fsck = Fsck::new(&image).unwrap();
        fsck.run(&FsckOptions::default()).unwrap();
        assert!(fsck.messages().contains(&String::from(
            "out-of-range zone in /f (ino = 2, zno = 999, type = DATA, pos = 0)"
        )));
    }

    #[test]
    fn duplicate_zone_reported() {
        let (mut image, _plan) = base_image();
        // 两个文件共指根的数据区 5：第二个走查时撞重复。
        write_inode(&mut image, 1, &dir_inode(2, 4 * 64, FIRST_ZONE));
        write_inode(&mut image, 2, &file_inode(1, 100, &[5]));
        write_inode(&mut image, 3, &file_inode(1, 100, &[5]));
        write_entry(&mut image, FIRST_ZONE, 2, b"f", 2);
        write_entry(&mut image, FIRST_ZONE, 3, b"g", 3);
        set_map_bit(&mut image, IMAP_BLOCK, 2);
        set_map_bit(&mut image, IMAP_BLOCK, 3);
        let mut fsck = Fsck::new(&image).unwrap();
        fsck.run(&FsckOptions::default()).unwrap();
        assert!(fsck.messages().contains(&String::from(
            "duplicate zone in /f (ino = 2, zno = 5, type = DATA, pos = 0)"
        )));
    }

    #[test]
    fn link_to_directory_reported() {
        let (mut image, _plan) = base_image();
        // 子目录 d（inode 2）+ 第二入口 hard：目录不得有第二链接。
        write_inode(&mut image, 1, &dir_inode(3, 4 * 64, FIRST_ZONE));
        write_inode(&mut image, 2, &dir_inode(2, 2 * 64, 6));
        write_entry(&mut image, FIRST_ZONE, 2, b"d", 2);
        write_entry(&mut image, FIRST_ZONE, 3, b"hard", 2);
        write_entry(&mut image, 6, 0, b".", 2);
        write_entry(&mut image, 6, 1, b"..", 1);
        set_map_bit(&mut image, IMAP_BLOCK, 2);
        set_map_bit(&mut image, ZMAP_BLOCK, 2); // 数据区 6 → 位 2
        let mut fsck = Fsck::new(&image).unwrap();
        fsck.run(&FsckOptions::default()).unwrap();
        assert!(fsck.messages().contains(&String::from(
            "link to directory discovered in / (ino = 1, name = 'hard', dir ino = 2)"
        )));
    }

    #[test]
    fn slash_in_name_reported() {
        let (mut image, _plan) = base_image();
        write_inode(&mut image, 1, &dir_inode(2, 3 * 64, FIRST_ZONE));
        write_entry(&mut image, FIRST_ZONE, 2, b"a/b", 3);
        let mut fsck = Fsck::new(&image).unwrap();
        fsck.run(&FsckOptions::default()).unwrap();
        assert!(fsck.messages().contains(&String::from(
            "found a '/' in entry of directory / (ino = 1, entry = 'a/b')"
        )));
    }

    #[test]
    fn bad_mode_reported_and_counted() {
        let (mut image, _plan) = base_image();
        write_inode(&mut image, 1, &dir_inode(2, 3 * 64, FIRST_ZONE));
        let mut bad = file_inode(1, 0, &[]);
        bad.mode = 0o160644; // 类型位 160000 不属任何已知类型
        write_inode(&mut image, 2, &bad);
        write_entry(&mut image, FIRST_ZONE, 2, b"f", 2);
        set_map_bit(&mut image, IMAP_BLOCK, 2);
        let mut fsck = Fsck::new(&image).unwrap();
        let summary = fsck.run(&FsckOptions::default()).unwrap();
        assert!(fsck.messages().contains(&String::from(
            "bad mode of /f (ino = 2, mode = 160644)"
        )));
        assert_eq!(summary.nbadinode, 1);
    }

    #[test]
    fn symlink_bad_size_reported() {
        let (mut image, _plan) = base_image();
        write_inode(&mut image, 1, &dir_inode(2, 3 * 64, FIRST_ZONE));
        let mut link = file_inode(1, 5, &[6]);
        link.mode = (crate::inode::TYPE_SYMLINK | 0o777) as u16;
        write_inode(&mut image, 2, &link);
        write_entry(&mut image, FIRST_ZONE, 2, b"l", 2);
        set_map_bit(&mut image, IMAP_BLOCK, 2);
        set_map_bit(&mut image, ZMAP_BLOCK, 2); // 数据区 6
        // 目标内容 "hel\0lo"：NUL 在 3，尺寸却是 5。
        let at = (6 * 4096) as usize;
        image[at..at + 6].copy_from_slice(b"hel\0lo");
        let mut fsck = Fsck::new(&image).unwrap();
        fsck.run(&FsckOptions::default()).unwrap();
        assert!(fsck.messages().contains(&String::from(
            "bad size in symbolic link (5 instead of 3) /l (ino = 2)"
        )));
    }

    #[test]
    fn empty_symlink_reported() {
        let (mut image, _plan) = base_image();
        write_inode(&mut image, 1, &dir_inode(2, 3 * 64, FIRST_ZONE));
        let mut link = file_inode(1, 0, &[]);
        link.mode = (crate::inode::TYPE_SYMLINK | 0o777) as u16;
        write_inode(&mut image, 2, &link);
        write_entry(&mut image, FIRST_ZONE, 2, b"l", 2);
        set_map_bit(&mut image, IMAP_BLOCK, 2);
        let mut fsck = Fsck::new(&image).unwrap();
        let summary = fsck.run(&FsckOptions::default()).unwrap();
        assert!(fsck
            .messages()
            .contains(&String::from("empty symbolic link /l (ino = 2)")));
        assert_eq!(summary.nsyml, 1);
    }

    #[test]
    fn special_file_checks() {
        // 合法字符设备：干净，计入 ncharspec。
        let (mut image, _plan) = base_image();
        write_inode(&mut image, 1, &dir_inode(2, 3 * 64, FIRST_ZONE));
        let mut dev = file_inode(1, 0, &[]);
        dev.mode = (crate::inode::TYPE_CHARACTER | 0o660) as u16;
        dev.zones[0] = (4 << 8) | 5;
        write_inode(&mut image, 2, &dev);
        write_entry(&mut image, FIRST_ZONE, 2, b"null", 2);
        set_map_bit(&mut image, IMAP_BLOCK, 2);
        let mut fsck = Fsck::new(&image).unwrap();
        let summary = fsck
            .run(&FsckOptions {
                listing: true,
                ..FsckOptions::default()
            })
            .unwrap();
        assert_eq!(summary.errors, 0);
        assert_eq!(summary.ncharspec, 1);
        assert!(fsck.messages().contains(&String::from(
            "     2 crw-rw----   1    4, 5 /null"
        )));
        // 非法 0 号设备 + 残留区块号：两条都报。
        let (mut image, _plan) = base_image();
        write_inode(&mut image, 1, &dir_inode(2, 3 * 64, FIRST_ZONE));
        let mut dev = file_inode(1, 0, &[]);
        dev.mode = (crate::inode::TYPE_BLOCK | 0o660) as u16;
        dev.zones[1] = 7;
        write_inode(&mut image, 2, &dev);
        write_entry(&mut image, FIRST_ZONE, 2, b"hdb", 2);
        set_map_bit(&mut image, IMAP_BLOCK, 2);
        let mut fsck = Fsck::new(&image).unwrap();
        fsck.run(&FsckOptions::default()).unwrap();
        assert!(fsck.messages().contains(&String::from(
            "illegal device number 0 for special file /hdb (ino = 2)"
        )));
        assert!(fsck.messages().contains(&String::from(
            "nonzero zone number 7 for special file /hdb (ino = 2)"
        )));
    }

    #[test]
    fn listing_half_lists_root() {
        let (image, _plan) = base_image();
        let mut fsck = Fsck::new(&image).unwrap();
        fsck.run(&FsckOptions {
            listing: true,
            ..FsckOptions::default()
        })
        .unwrap();
        assert_eq!(
            fsck.messages()[0],
            " inode permission link   size name"
        );
        assert_eq!(fsck.messages()[1], "     1 drwxrwxrwx   2     128 /");
    }

    #[test]
    fn watch_lists_hint_without_erroring() {
        let (image, _plan) = base_image();
        let mut fsck = Fsck::new(&image).unwrap();
        let summary = fsck
            .run(&FsckOptions {
                watch_zones: alloc::vec![5],
                watch_inodes: alloc::vec![500],
                ..FsckOptions::default()
            })
            .unwrap();
        // 观察单命中是提示：根数据区被标到时报 "found"，不记错。
        assert!(fsck.messages().contains(&String::from(
            "found zone in / (ino = 1, zno = 5, type = DATA, pos = 0)"
        )));
        assert!(fsck
            .messages()
            .contains(&String::from("inode number 500 out of range (ignored)")));
        assert_eq!(summary.errors, 0);
    }

    #[test]
    fn size_not_updated_reported() {
        let (mut image, _plan) = base_image();
        // 根的尺寸记成一条目，实际有两条：尾尺寸越过字段值。
        write_inode(&mut image, 1, &dir_inode(2, 64, FIRST_ZONE));
        let mut fsck = Fsck::new(&image).unwrap();
        fsck.run(&FsckOptions::default()).unwrap();
        assert!(fsck.messages().contains(&String::from(
            "size not updated of directory / (ino = 1)"
        )));
    }

    #[test]
    fn indirect_zone_walk_clean_and_tampered() {
        // 干净面：单间区块 7 指数据区 6，期望图与计数都对上。
        let (mut image, _plan) = base_image();
        write_inode(&mut image, 1, &dir_inode(2, 3 * 64, FIRST_ZONE));
        write_inode(&mut image, 2, &file_inode(1, 100, &[0, 0, 0, 0, 0, 0, 0, 7]));
        write_entry(&mut image, FIRST_ZONE, 2, b"f", 2);
        set_map_bit(&mut image, IMAP_BLOCK, 2);
        set_map_bit(&mut image, ZMAP_BLOCK, 2); // 数据区 6
        set_map_bit(&mut image, ZMAP_BLOCK, 3); // 间区块 7
        let indirect_at = (7 * 4096) as usize;
        image[indirect_at..indirect_at + 4].copy_from_slice(&6u32.to_le_bytes());
        let mut fsck = Fsck::new(&image).unwrap();
        let summary = fsck.run(&FsckOptions::default()).unwrap();
        assert_eq!(summary.errors, 0);
        assert_eq!(summary.ztype, [2, 1, 0, 0], "根数据区与文件数据区两级 0，间区块一级 1");
        assert_eq!(summary.nfreezone, 56, "58 - 数据区 6 - 间区块 7");
        // 篡改面：间区块指到 999（越界走递归的 level 0 报告）。
        let (mut image, _plan) = base_image();
        write_inode(&mut image, 1, &dir_inode(2, 3 * 64, FIRST_ZONE));
        write_inode(&mut image, 2, &file_inode(1, 4096, &[0, 0, 0, 0, 0, 0, 0, 7]));
        write_entry(&mut image, FIRST_ZONE, 2, b"f", 2);
        set_map_bit(&mut image, IMAP_BLOCK, 2);
        set_map_bit(&mut image, ZMAP_BLOCK, 3);
        let indirect_at = (7 * 4096) as usize;
        image[indirect_at..indirect_at + 4].copy_from_slice(&999u32.to_le_bytes());
        let mut fsck = Fsck::new(&image).unwrap();
        fsck.run(&FsckOptions::default()).unwrap();
        assert!(fsck.messages().contains(&String::from(
            // 前七个直连槽都是洞：pos 已按每洞一个区块跳到 28672。
            "out-of-range zone in /f (ino = 2, zno = 999, type = DATA, pos = 28672)"
        )));
    }

    #[test]
    fn root_not_directory_is_fatal() {
        let (mut image, _plan) = base_image();
        let mut root = dir_inode(2, 128, FIRST_ZONE);
        root.mode = (crate::inode::TYPE_REGULAR | 0o644) as u16;
        write_inode(&mut image, 1, &root);
        let mut fsck = Fsck::new(&image).unwrap();
        let fatal = match fsck.run(&FsckOptions::default()) {
            Err(fatal) => fatal,
            Ok(_) => panic!("expected fatal"),
        };
        assert_eq!(
            fatal.message,
            "root inode is not a directory (ino = 1, mode = 100644)"
        );
    }

    #[test]
    fn orphan_allocated_inode_flagged_by_maps_and_ilist() {
        // 只有位图位、无模式无入口：图对照报 missing，空闲表不报
        // （模式为零本就是清零态）。
        let (mut image, _plan) = base_image();
        set_map_bit(&mut image, IMAP_BLOCK, 2);
        let mut fsck = Fsck::new(&image).unwrap();
        fsck.run(&FsckOptions::default()).unwrap();
        assert!(fsck
            .messages()
            .contains(&String::from("inode 2 is missing")));
        assert!(!fsck
            .messages()
            .iter()
            .any(|m| m.contains("not cleared")));
        // 再补一个非零模式：空闲表也报 not cleared。
        let (mut image, _plan) = base_image();
        set_map_bit(&mut image, IMAP_BLOCK, 2);
        write_inode(&mut image, 2, &file_inode(1, 0, &[]));
        let mut fsck = Fsck::new(&image).unwrap();
        fsck.run(&FsckOptions::default()).unwrap();
        assert!(fsck
            .messages()
            .contains(&String::from("mode inode 2 not cleared")));
    }
}
