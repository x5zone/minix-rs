//! I-22 丁案 A：`MemoryMapSource` —— 三引导链内存图的单一语义出口。
//!
//! 语义契约（公理 3 的正面用法）：每条引导链向 OS 语义层暴露**同一**形状
//! —— conventional（PMM 可直分）与 reserved（运行期经 Direct Map 会访问
//! 但不得重分配）两张 `'static` 表。三链各自的固件形状（UEFI
//! `GetMemoryMap`、DTB `/memory`、未来的 Multiboot）留在各自 impl 内，
//! 不再泄漏进消费方（此前 opensbi 腿把「硬编码的 QEMU virt DRAM 单区」
//! 直接递给内核，即本 trait 要收口的泄漏）。
//!
//! 引导形态层**不**在本 trait 范围内合并（公理 3：`BootShim` 独立可执行体
//! 与 `bootface` 自举面对应不同机器状态；本层只统一下沉 memmap 源）。
//!
//! 实现落位：`DtbMemoryMap` 住本库（DTB 解析对所有裸机引导面同构，且
//! `fdt` 依赖已在本库清单内）；`UefiMemoryMap` 住 boot-shim（UEFI
//! `GetMemoryMap` 需要 `uefi` crate，不得反向进本库，孤儿规则下由
//! boot-shim 本地类型实现外来 trait）；`MultibootMemoryMap` 为 BIOS 腿
//! 占位，形态出现时补真实现（丁案 F：不为漂亮提前改契约）。

use crate::kernel_info::MemoryRegion;
use minix_types::PhysBytes;
use core::sync::atomic::{AtomicUsize, Ordering};

/// DTB 内存图容量。与 kernel-image bootface 旧 `MEMMAP_MAX = 8` 对称
///（容量随迁移走，消费方语义不变）。
const DTB_MEMMAP_MAX: usize = 8;

// `static mut`：bootface MEMMAP_STORAGE 同款形态——单 hart boot 面
// 写一次后只读；读侧从不出 &mut（addr_of! 基址 + from_raw_parts）。
static mut DTB_FREE: [MemoryRegion; DTB_MEMMAP_MAX] =
    [MemoryRegion { base: PhysBytes(0), len: 0 }; DTB_MEMMAP_MAX];
static mut DTB_RESERVED: [MemoryRegion; DTB_MEMMAP_MAX] =
    [MemoryRegion { base: PhysBytes(0), len: 0 }; DTB_MEMMAP_MAX];
static DTB_FREE_N: AtomicUsize = AtomicUsize::new(0);
static DTB_RESERVED_N: AtomicUsize = AtomicUsize::new(0);

/// 把 `[start, start+len)` 中落在固件常驻区（`[0, fw_end)`，PA 从 0 计的
/// 下界语义）的部分削进 reserved，其余归还 conventional。返回
/// `(free_start, free_len)`；`None` = 整段都是固件区（free 无剩余）。
///
/// 纯函数——bootface 腿削界算术的编译期护栏（本模块底部）钉住单字符
/// 变异（语义迁移自原 bootface `clip_firmware`，`fw_end` 参数化后
/// riscv 腿传 `KERN_PHYS_BASE`、opensbi boot-shim 腿传 0＝不削）。
const fn clip_reserved(
    start: u64,
    len: u64,
    fw_end: u64,
) -> Option<(u64, u64)> {
    if start >= fw_end {
        return Some((start, len));
    }
    let drop = fw_end - start;
    if drop >= len {
        return None; // 整段在固件区下（含恰等 len==drop）
    }
    Some((fw_end, len - drop))
}

// 削界算术的编译期护栏（语义迁移自原 bootface `clip_firmware` 断言组，
// `fw_end` 显式传 0x8020_0000＝riscv 腿 `KERN_PHYS_BASE` 的真值）：
//   • 纯固件段 `[DRAM, FW_END)` → None
const _FW_PURE: () = assert!(clip_reserved(0x8000_0000, 0x20_0000, 0x8020_0000).is_none());
//   • 完全在镜像之上的段 → 原样保留
const _FW_ABOVE: () = assert!(
    matches!(clip_reserved(0x8020_0000, 0x1000, 0x8020_0000), Some((0x8020_0000, 0x1000)))
);
//   • 跨界段 → 削掉固件区、保留其余
const _FW_CROSS: () = assert!(
    matches!(
        clip_reserved(0x8000_0000, 0x20_1000, 0x8020_0000),
        Some((0x8020_0000, 0x1000))
    )
);

/// 三引导链内存图的单一语义出口（模块头有全量论证）。
pub trait MemoryMapSource {
    /// 返回 `(conventional, reserved)` 两张 `'static` 表。
    fn collect(&self) -> (&'static [MemoryRegion], &'static [MemoryRegion]);
}

/// DTB 腿：解析 `/memory` 与 `/memory@<unit>`（unit-address 变体）的
/// `reg`，经 [`clip_reserved`] 削掉固件常驻区后产出两表。查找走
/// `all_nodes` 全树遍历而非 `find_node`（上游 fdt 0.1.5 的 `find_node`
/// 对部分固件重建 blob 会漏节点——SD-38 实证，消费方 bootface 两侧
/// 统一绕行形）。
///
/// `firmware_reserve_end`：固件常驻区上界（PA）。riscv bootface 腿传
/// `KERN_PHYS_BASE`（OpenSBI 常驻 `[DRAM_BASE, KERN_PHYS_BASE)` 且
/// VM 位图元数据会 memset 整段——续-91 实钉，削除是功能必需）；
/// opensbi boot-shim 腿传 0＝不削（该腿内核侧 `cut_memmap` 自切）。
pub struct DtbMemoryMap<'a> {
    fdt: &'a fdt::Fdt<'a>,
    firmware_reserve_end: u64,
}

impl<'a> DtbMemoryMap<'a> {
    pub fn new(fdt: &'a fdt::Fdt<'a>, firmware_reserve_end: u64) -> Self {
        Self {
            fdt,
            firmware_reserve_end,
        }
    }
}

impl MemoryMapSource for DtbMemoryMap<'_> {
    fn collect(&self) -> (&'static [MemoryRegion], &'static [MemoryRegion]) {
        // 单 hart boot 面写一次后只读；计数器归零重入（同面二次 collect）
        // 也只是覆盖同一批格。
        DTB_FREE_N.store(0, Ordering::Relaxed);
        DTB_RESERVED_N.store(0, Ordering::Relaxed);
        let mut free_n = 0usize;
        let mut reserved_n = 0usize;
        for node in self.fdt.all_nodes() {
                // `/memory` 或 `/memory@80000000`（unit-address 变体）。
                if node.name.split('@').next() != Some("memory") {
                    continue;
                }
                let Some(regs) = node.reg() else { continue };
                for region in regs {
                    let start = region.starting_address as u64;
                    let len = match region.size {
                        Some(s) if s != 0 => s,
                        _ => continue,
                    };
                    if let Some((free_start, free_len)) =
                        clip_reserved(start, len as u64, self.firmware_reserve_end)
                    {
                        if free_n < DTB_MEMMAP_MAX {
                            // SAFETY: free_n < 数组长度；boot 单 hart。
                            unsafe {
                                DTB_FREE[free_n] =
                                    MemoryRegion { base: PhysBytes(free_start), len: free_len as usize };
                            }
                            free_n += 1;
                        }
                    }
                    if start < self.firmware_reserve_end && reserved_n < DTB_MEMMAP_MAX {
                        let r_len = (self.firmware_reserve_end - start).min(len as u64) as usize;
                        // SAFETY: reserved_n < 数组长度；boot 单 hart。
                        unsafe {
                            DTB_RESERVED[reserved_n] =
                                MemoryRegion { base: PhysBytes(start), len: r_len };
                        }
                        reserved_n += 1;
                    }
                }
            }
        DTB_FREE_N.store(free_n, Ordering::Release);
        DTB_RESERVED_N.store(reserved_n, Ordering::Release);
        // SAFETY: 两静态数组长度 DTB_MEMMAP_MAX；n ≤ DTB_MEMMAP_MAX（上面
        // 入队前钳位）；boot 单 hart 写完才读。
        unsafe {
            (
                core::slice::from_raw_parts(
                    core::ptr::addr_of!(DTB_FREE) as *const MemoryRegion,
                    free_n,
                ),
                core::slice::from_raw_parts(
                    core::ptr::addr_of!(DTB_RESERVED) as *const MemoryRegion,
                    reserved_n,
                ),
            )
        }
    }
}

/// BIOS/Multiboot 腿占位：本仓尚无该引导形态，空表实现保 trait 完整；
/// 形态出现时补真实现（丁案 F：不为漂亮提前改契约）。
pub struct MultibootMemoryMap;

impl MemoryMapSource for MultibootMemoryMap {
    fn collect(&self) -> (&'static [MemoryRegion], &'static [MemoryRegion]) {
        (&[], &[])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::{format, vec::Vec};

    /// 按规范形手搓最小 FDT blob：header(40) + memrsv(16) + 结构块 +
    /// strings。根节点带 `#address-cells=2`/`#size-cells=2`，`/memory`
    /// 子节点（unit-address 变体可选）挂 128 位 `reg`。
    fn build_fdt(memory_regs: &[(u64, u64)], unit_address: bool) -> Vec<u8> {
        let mut strings: Vec<u8> = Vec::new();
        strings.extend_from_slice(b"#address-cells\0");
        strings.extend_from_slice(b"#size-cells\0");
        let nameoff_cells = 0usize;
        let nameoff_size = 15usize; // "#address-cells\0" 恰 15 字节
        strings.extend_from_slice(b"reg\0");
        let nameoff_reg = 27usize; // 15 + 12（"#size-cells\0" 恰 12 字节）
        let mut struct_blk: Vec<u8> = Vec::new();
        let push_u32 = |out: &mut Vec<u8>, v: u32| out.extend_from_slice(&v.to_be_bytes());
        let push_prop = |out: &mut Vec<u8>, nameoff: u32, val: &[u8]| {
            push_u32(out, 3); // FDT_PROP
            push_u32(out, val.len() as u32);
            push_u32(out, nameoff as u32);
            out.extend_from_slice(val);
            while out.len() % 4 != 0 { out.push(0); }
        };
        push_u32(&mut struct_blk, 1); // FDT_BEGIN_NODE（根，名空）
        struct_blk.push(0);
        while struct_blk.len() % 4 != 0 { struct_blk.push(0); }
        push_prop(&mut struct_blk, nameoff_cells as u32, &2u32.to_be_bytes());
        push_prop(&mut struct_blk, nameoff_size as u32, &2u32.to_be_bytes());
        for (base, size) in memory_regs {
            push_u32(&mut struct_blk, 1); // FDT_BEGIN_NODE
            let name: &str = if unit_address {
                &format!("memory@{base:x}\u{0}")
            } else {
                "memory\u{0}"
            };
            struct_blk.extend_from_slice(name.as_bytes());
            while struct_blk.len() % 4 != 0 { struct_blk.push(0); }
            // 根已声明 cells 2/2，reg 按其语义全编码（128 位：
            // [hi(base)=0, base, hi(size)=0, size]）。真机教训：1/1 短编码
            // 在 2/2 语义下被截断（size=None）→ 区域被静默跳过。
            let mut reg: Vec<u8> = Vec::new();
            reg.extend_from_slice(&0u32.to_be_bytes());
            reg.extend_from_slice(&(*base as u32).to_be_bytes());
            reg.extend_from_slice(&0u32.to_be_bytes());
            reg.extend_from_slice(&(*size as u32).to_be_bytes());
            push_prop(&mut struct_blk, nameoff_reg as u32, &reg);
            push_u32(&mut struct_blk, 2); // FDT_END_NODE
        }
        push_u32(&mut struct_blk, 2); // FDT_END_NODE（根）
        push_u32(&mut struct_blk, 9); // FDT_END

        let struct_off = 40u32 + 16u32;
        let strings_off = struct_off + struct_blk.len() as u32;
        let mut out: Vec<u8> = Vec::new();
        let push = |out: &mut Vec<u8>, v: u32| out.extend_from_slice(&v.to_be_bytes());
        push(&mut out, 0xd00d_feed);          // magic
        push(&mut out, 0);                    // totalsize（回填）
        push(&mut out, struct_off);
        push(&mut out, strings_off);
        push(&mut out, 40);                   // off_memrsv
        push(&mut out, 17);                   // version
        push(&mut out, 16);                   // last_comp_version
        push(&mut out, 0);                    // boot_cpuid
        push(&mut out, strings.len() as u32); // size_strings
        push(&mut out, struct_blk.len() as u32); // size_struct
        out.extend_from_slice(&[0u8; 16]);    // memrsv
        out.extend_from_slice(&struct_blk);
        out.extend_from_slice(&strings);
        while out.len() % 4 != 0 { out.push(0); }
        let total = out.len() as u32;
        out[4..8].copy_from_slice(&total.to_be_bytes());
        out
    }

    #[test]
    fn dtb_single_memory_region_fw_end_zero() {
        let blob = build_fdt(&[(0x8000_0000, 0x4000_0000)], false);
        let fdt = fdt::Fdt::new(&blob).expect("规范形 stub 必须可解析");
        let (conv, res) = DtbMemoryMap::new(&fdt, 0).collect();
        assert_eq!(conv.len(), 1);
        assert_eq!(conv[0].base.0, 0x8000_0000);
        assert_eq!(conv[0].len, 0x4000_0000 as usize);
        assert!(res.is_empty(), "fw_end=0 不产生 reserved");
    }

    #[test]
    fn dtb_multi_region_and_unit_address_variant() {
        let blob = build_fdt(&[(0x8000_0000, 0x4000_0000), (0x8800_0000, 0x1000_0000)], true);
        let fdt = fdt::Fdt::new(&blob).expect("stub 必须可解析");
        let (conv, _res) = DtbMemoryMap::new(&fdt, 0).collect();
        assert_eq!(conv.len(), 2, "两段 /memory → 2 条（含 unit-address 变体）");
        assert_eq!(conv[0].base.0, 0x8000_0000);
        assert_eq!(conv[1].base.0, 0x8800_0000);
    }

    #[test]
    fn dtb_fw_clip_moves_tail_to_reserved() {
        // /memory = [0x8000_0000, +64MiB)，fw_end = 0x8020_0000（OpenSBI 2MiB）
        // → free = [0x8020_0000, +62MiB)，reserved = [0x8000_0000, 2MiB)
        let blob = build_fdt(&[(0x8000_0000, 0x400_0000)], false);
        let fdt = fdt::Fdt::new(&blob).expect("stub 必须可解析");
        let (conv, res) = DtbMemoryMap::new(&fdt, 0x8020_0000).collect();
        assert_eq!(conv.len(), 1);
        assert_eq!(conv[0].base.0, 0x8020_0000, "削界后 free 起点抬到 fw_end");
        assert_eq!(conv[0].len, (0x400_0000 - 0x20_0000) as usize);
        assert_eq!(res.len(), 1, "削掉的固件段进 reserved");
        assert_eq!(res[0].base.0, 0x8000_0000);
        assert_eq!(res[0].len, 0x20_0000 as usize);
    }

    #[test]
    fn dtb_fully_reserved_region_drops_from_free() {
        // /memory = [0x8000_0000, +1MiB) 全在 fw_end=0x8020_0000 之下
        // → free 空、reserved 全段（bootface 侧消费方见空表走诚实停机）。
        let blob = build_fdt(&[(0x8000_0000, 0x10_0000)], false);
        let fdt = fdt::Fdt::new(&blob).expect("stub 必须可解析");
        let (conv, res) = DtbMemoryMap::new(&fdt, 0x8020_0000).collect();
        assert!(conv.is_empty(), "整段固件区 → free 空（诚实停机由消费方判）");
        assert_eq!(res.len(), 1);
    }

    #[test]
    fn dtb_no_memory_node_yields_empty() {
        let blob = build_fdt(&[], false);
        let fdt = fdt::Fdt::new(&blob).expect("stub 必须可解析");
        let (conv, res) = DtbMemoryMap::new(&fdt, 0).collect();
        assert!(conv.is_empty(), "无 /memory → 空（诚实停机由消费方判）");
        assert!(res.is_empty());
    }
}
