//! VFS 设备映射表快照 —— `SI_DMAP_TAB` 的 wire 权威
//! （`[ARCH: A-4]` 单一权威）。
//!
//! C ground truth: `minix3/minix/servers/vfs/dmap.h:16-28`（`struct dmap
//! dmap[NR_DEVICES]`，`NR_DEVICES 135` — dmap.h:82）。快照只带**使用字段
//! 子集**（C 声明序，`#[repr(C)]`）——与 `FProcSnap`/`MProcSnap`/
//! `DsEntrySnap`/`RprocpubSnap` 同一裁定：C 的 `dmap_sel_filp`（filp 指针）、
//! `dmap_lock`（互斥量）、`dmap_servicing`（线程 id）都是 VFS 进程内部状态，
//! 跨 wire 无意义；IS 的 `dmap_dmp` 只打标签、major 与驱动端点三列。
//!
//! 生产者（`os/servers/vfs/src/misc.rs` 的 `do_getsysinfo` DMAP 臂）按本
//! 结构逐行序列化整表；消费方（IS `dump_vfs` 的 `dmap_dmp`）按名读取。
//! 空槽的 `dmap_driver` 是 `NONE`（`init_dmap` 把每行填 `NONE`——
//! dmap.c:235-247；不是 0），消费者按它过滤。

/// C `LABEL_MAX 16` — vfs/const.h:34（标签列宽）。
pub const DMAP_LABEL_LEN: usize = 16;
/// C `NR_DEVICES 135` — dmap.h:82（表行数；`SI_DMAP_TAB` 的长度契约）。
pub const NR_DEVICES: usize = 135;

/// 设备映射表行快照（`struct dmap` 的使用字段子集）。
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DmapSnap {
    /// C: `dmap_driver` (dmap.h:17；`NONE` = 空槽，不是 0)。
    pub dmap_driver: i32,
    /// C: `dmap_label[LABEL_MAX]` (dmap.h:18)。
    pub dmap_label: [u8; DMAP_LABEL_LEN],
}

impl Default for DmapSnap {
    // Manual: `[u8; 16]` has no `Default`; all-zero bytes are the C BSS shape.
    fn default() -> Self {
        Self { dmap_driver: 0, dmap_label: [0u8; DMAP_LABEL_LEN] }
    }
}

#[cfg(test)]
mod dmap_snap_layout_tests {
    use super::*;
    use core::mem::offset_of;

    /// 布局见证：驱动端点 4 字节 + 标签 16 字节 = 20。
    #[test]
    fn test_dmap_snap_layout() {
        assert_eq!(size_of::<DmapSnap>(), 20);
        assert_eq!(offset_of!(DmapSnap, dmap_driver), 0);
        assert_eq!(offset_of!(DmapSnap, dmap_label), 4);
    }

    /// 整表宽度 = 行数 × 行宽（生产者与消费者的同一算式）。
    #[test]
    fn test_table_width() {
        assert_eq!(NR_DEVICES, 135);
        assert_eq!(size_of::<DmapSnap>() * NR_DEVICES, 2_700);
    }
}
