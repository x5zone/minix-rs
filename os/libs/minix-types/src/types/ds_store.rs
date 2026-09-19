//! DS 数据仓表快照 —— `SI_DATA_STORE` 的 wire 权威（`[ARCH: A-4]` 单一权威）。
//!
//! C ground truth: `minix3/minix/servers/ds/store.h:16-29`（`ds_store[NR_DS_KEYS]`）。
//! 快照只带**标量面**：C 的 `union dsi_u { u32 u32; struct dsi_mem mem; }`
//! 里，宽臂的 `data` 是 DS 地址空间的指针（跨 wire 无意义），故快照以单个
//! 标量槽取代整个联合——U32/LABEL 取 `u.u32`，STR/MEM 取 `u.mem.length`
//! （C 的 dump 对 MEM 打 `length`；STR 打 `data` 指针的**内容**，那是
//! Minix3 自身的 bug，见 `08-stage-is/09-is-dump-ds.md` §3.4）。
//!
//! 生产者（`os/servers/ds/src/server.rs` 的 `render_image`）按本结构逐槽
//! 渲染 `SI_DATA_STORE` 应答镜像，消费方（IS `dump_ds`）按名读取；尺寸门
//! 是精确匹配（`store.c:668`），两侧都从 `size_of::<DsEntrySnap>()` 推。

/// Number of store slots. C: `NR_DS_KEYS (2*NR_SYS_PROCS)` — store.h:12
/// (`NR_SYS_PROCS 64` — sys_config.h:9).
pub const NR_DS_KEYS: usize = 128;
/// Max key/owner length. C: `DS_MAX_KEYLEN 80` — ds.h:29.
pub const DS_MAX_KEYLEN: usize = 80;
/// In-use bit. C: `DSF_IN_USE 0x001` — ds.h:12（`DsFlags::IN_USE` 是同一
/// 位在标志集里的名字；消费方（IS dump）按标量字读，故此处给裸值）。
pub const DSF_IN_USE: u32 = 0x001;
// `DSF_MASK_TYPE` / `DS_MAX_KEYLEN` 的权威在 `types/com.rs`（单一位置），
// 本模块不重复定义（本模块只加 `DsEntrySnap` 与它自有的槽数/在役位）。

/// Data-store entry snapshot (scalar face only).
///
/// C: `struct data_store` — `store.h:16-29`（子集：字符串本体与指针不进
/// 快照，见模块文档）。
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct DsEntrySnap {
    /// C: `flags` (store.h:17).
    pub flags: i32,
    /// C: `key[DS_MAX_KEYLEN]` (store.h:18).
    pub key: [u8; DS_MAX_KEYLEN],
    /// C: `owner[DS_MAX_KEYLEN]` (store.h:19).
    pub owner: [u8; DS_MAX_KEYLEN],
    /// 标量载荷：`u.u32`（U32/LABEL）或 `u.mem.length`（STR/MEM）。
    pub scalar: u32,
}

impl Default for DsEntrySnap {
    // Manual: `[u8; 80]` has no `Default` — all-zero bytes are the C BSS
    // initialiser (vacant seats render all-zero, `server.rs` render_image).
    fn default() -> Self {
        Self { flags: 0, key: [0u8; DS_MAX_KEYLEN], owner: [0u8; DS_MAX_KEYLEN], scalar: 0 }
    }
}

#[cfg(test)]
mod ds_entry_snap_tests {
    use super::*;
    use core::mem::offset_of;

    /// 布局见证：flags(4) + key(80) + owner(80) + scalar(4) = 168。
    #[test]
    fn test_ds_entry_snap_layout() {
        assert_eq!(size_of::<DsEntrySnap>(), 168);
        assert_eq!(offset_of!(DsEntrySnap, flags), 0);
        assert_eq!(offset_of!(DsEntrySnap, key), 4);
        assert_eq!(offset_of!(DsEntrySnap, owner), 84);
        assert_eq!(offset_of!(DsEntrySnap, scalar), 164);
    }

    /// 镜像宽度 = 槽数 × 行宽（生产者与消费者的同一算式）。
    #[test]
    fn test_image_bytes_follows_row_width() {
        assert_eq!(NR_DS_KEYS, 128);
        assert_eq!(size_of::<DsEntrySnap>() * NR_DS_KEYS, 21_504);
    }

    /// 在役位与类型位（ds.h:12/17-20 的 wire 值；掩码常量权威在
    /// `types/com.rs`，这里只钉本模块自有的在役位）。
    #[test]
    fn test_dsf_constants() {
        assert_eq!(DSF_IN_USE, 0x001);
        // 类型位在掩码 0xFF0 内的位置（U32 0x010 / LABEL 0x100）。
        assert_eq!(0x010 & 0xFF0, 0x010);
        assert_eq!(0x100 & 0xFF0, 0x100);
    }
}
