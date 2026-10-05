//! Sv39 软件走表的纯逻辑层（NK4C §续-349 污染取证产物）。
//!
//! `walk_read_with` 把「按物理地址读一个 PTE」抽成闭包，走表逻辑本身与
//! Direct Map 窗口、硬件指令完全解耦——宿主测试可用合成页表内存直接驱
//! 动（真机形态由 `riscv64::paging::walk_read` 提供读取闭包）。
//!
//! # 与硬件语义对齐（§续-349 修复）
//!
//! 1. **规范性检查**：Sv39 要求 VA[63:39] 全等于 VA[38]。硬件在走表
//!    *开始之前* 就拒绝非规范地址（QEMU `target/riscv/cpu_helper.c`
//!    masked_msbs 门）；此前软件走表不查规范性，会顺着共享根表的内核
//!    半区（root[256..512]）给非规范 VA「翻译」出错误 PA——内核代写
//!    （finw 回执/跨空间拷贝）拿到它即随机腐蚀内存。现在非规范 VA 一律
//!    `NotPresent`（与硬件 fault 对齐，fail-closed）。
//! 2. **错粒度巨叶（misaligned huge page）**：2MB/1GB 叶 PTE 的低位
//!    PPN 必须为 0，非零=保留形态（硬件必 fault）。此前软件走表直接 OR
//!    折叠偏移给出垃圾 PA；现在按硬件语义返回 `NotPresent`。
//! 3. **L0 指针形态**：L0 表项 V=1 而 R=W=X=0 在末级是保留形态（硬件
//!    必 fault）；`walk_translate`（riscv64::paging）裁决该形态为 None。
//!
//! 单一真源：位移/掩码常量与 `pte_is_leaf`/`pte_to_paddr` 由本模块导出，
//! `riscv64::paging` 委托使用（不再各持一份）。

/// Sv39 页表级位移。
pub const L2_SHIFT: u32 = 30;
pub const L1_SHIFT: u32 = 21;
pub const L0_SHIFT: u32 = 12;

/// PPN 字段掩码（pte[53:10]；bits 63:54 为保留/PBMT，不进 PA）。
pub const PTE_PPN_MASK: u64 = 0x003F_FFFF_FFFF_FC00;

/// V（Valid）位。
pub const PTE_V: u64 = 1 << 0;
/// R|W|X 位掩码（任一置位=叶；全零=表指针）。
pub const PTE_RWX_MASK: u64 = 0b1110;

/// Sv39 规范性检查：VA[63:39] 必须全等于 VA[38]。
/// 用户半区（VA[38]=0）：高位 39 位全 0；内核半区（VA[38]=1）：全 1。
#[inline]
pub fn is_canonical_sv39(vaddr: u64) -> bool {
    let msbs = vaddr & 0xFFFF_FF80_0000_0000;
    msbs == 0 || msbs == 0xFFFF_FF80_0000_0000
}

/// 叶判定：任一 R/W/X 置位（V 位由调用方先行检查）。
#[inline]
pub fn pte_is_leaf(pte: u64) -> bool {
    pte & PTE_RWX_MASK != 0
}

/// 物理地址 → PTE（PPN 字段 = paddr[53:12]<<... 即 (paddr>>2)&MASK；
/// 保留高位与页内偏移不进 PTE）。
#[inline]
pub fn paddr_to_pte(paddr: u64) -> u64 {
    (paddr >> 2) & PTE_PPN_MASK
}

/// PTE → 物理地址（PPN<<12；保留高位不进 PA）。
#[inline]
pub fn pte_to_paddr(pte: u64) -> u64 {
    ((pte & PTE_PPN_MASK) >> 10) << 12
}

/// 三级索引提取（真机 walk_alloc 与纯走表共用）。
#[inline]
pub fn l2_index(vaddr: u64) -> usize {
    ((vaddr >> L2_SHIFT) & 0x1FF) as usize
}
#[inline]
pub fn l1_index(vaddr: u64) -> usize {
    ((vaddr >> L1_SHIFT) & 0x1FF) as usize
}
#[inline]
pub fn l0_index(vaddr: u64) -> usize {
    ((vaddr >> L0_SHIFT) & 0x1FF) as usize
}

/// 巨叶对齐检查：L2（1GB）叶要求 PTE[27:10]（=PPN[17:0]=paddr[29:12]）
/// 为 0；L1（2MB）叶要求 PTE[18:10]（=PPN[8:0]=paddr[20:12]）为 0。
/// 非零=保留形态（硬件必 fault）。注意掩码宽度是 **PPN 位宽**（9/18 位）：
/// PPN[9]=paddr[21]（2MB 页）/PPN[17]=paddr[30]（1GB 页）是合法非零位——
/// 掩码多含一位就会把 0x80400000 这类合法 2MB 帧误判为错粒度
/// （§续-350 委托回归真因：掩码写成了 10/19 位）。
#[inline]
fn huge_leaf_misaligned(pte: u64, level2: bool) -> bool {
    let low_mask: u64 = if level2 { (1 << 18) - 1 } else { (1 << 9) - 1 };
    ((pte & PTE_PPN_MASK) >> 10) & low_mask != 0
}

/// 走表结果。`Leaf` 的 PTE 原样返回（V 位/叶形态/旗标由上层按需裁决）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalkResult {
    /// L0 叶 PTE 的物理地址与原始 PTE 值（V/叶形态由上层裁决）。
    Leaf(u64, u64),
    /// L2 1GB 叶：物理地址已按 Sv39 规则折叠 VA 低位，附原始 PTE。
    Huge1G(u64, u64),
    /// L1 2MB 叶：物理地址已折叠 VA 低位，附原始 PTE。
    Huge2M(u64, u64),
    /// 某级不命中（含非规范 VA——硬件在该地址上必然 fault，软件对齐）。
    NotPresent,
}

/// 纯逻辑 Sv39 三级走表。
///
/// * `read_pte` —— 按**物理地址**读一个 u64 PTE（真机=Direct Map 窗口
///   读取；宿主测试=合成页表内存闭包）。
/// * `root_paddr` —— 根表物理地址。
/// * `vaddr` —— 待翻译 VA（非规范 → `NotPresent`，对齐硬件 fault）。
pub fn walk_read_with<F>(read_pte: &mut F, root_paddr: u64, vaddr: u64) -> WalkResult
where
    F: FnMut(u64) -> u64,
{
    // §续-356 A/B：加固默认关（旧语义），walk-hardening 特性开=fail-closed。
    if cfg!(feature = "walk-hardening") && !is_canonical_sv39(vaddr) {
        return WalkResult::NotPresent;
    }

    let i2 = ((vaddr >> L2_SHIFT) & 0x1FF) as usize;
    let l2e = read_pte(root_paddr + (i2 as u64) * 8);
    if l2e & PTE_V == 0 {
        return WalkResult::NotPresent;
    }
    if pte_is_leaf(l2e) {
        if cfg!(feature = "walk-hardening") && huge_leaf_misaligned(l2e, true) {
            return WalkResult::NotPresent; // 保留形态：硬件必 fault，软件对齐
        }
        return WalkResult::Huge1G(pte_to_paddr(l2e) | (vaddr & 0x3FFF_FFFF), l2e);
    }
    let l1_base = pte_to_paddr(l2e);

    let i1 = ((vaddr >> L1_SHIFT) & 0x1FF) as usize;
    let l1e = read_pte(l1_base + (i1 as u64) * 8);
    if l1e & PTE_V == 0 {
        return WalkResult::NotPresent;
    }
    if pte_is_leaf(l1e) {
        if cfg!(feature = "walk-hardening") && huge_leaf_misaligned(l1e, false) {
            return WalkResult::NotPresent;
        }
        return WalkResult::Huge2M(pte_to_paddr(l1e) | (vaddr & 0x1F_FFFF), l1e);
    }
    let l0_base = pte_to_paddr(l1e);

    let i0 = ((vaddr >> L0_SHIFT) & 0x1FF) as usize;
    let leaf_paddr = l0_base + (i0 as u64) * 8;
    let pte = read_pte(leaf_paddr);
    WalkResult::Leaf(leaf_paddr, pte)
}

#[cfg(all(test, feature = "runtime-window"))]
mod tests {
    use super::*;

    // ── 合成页表区：一块泄漏内存当「物理内存」，phys = REGION_PHYS + 偏移 ──
    const REGION_PHYS: u64 = 0x9000_0000;
    const PAGE_ENTRIES: usize = 512;

    struct Synth {
        region: &'static mut [u64],
    }

    impl Synth {
        fn new(pages: usize) -> Self {
            Self { region: Box::leak(vec![0u64; PAGE_ENTRIES * pages].into_boxed_slice()) }
        }
        fn page_phys(&self, slot: usize) -> u64 {
            REGION_PHYS + (slot * PAGE_ENTRIES * 8) as u64
        }
        fn entry_mut(&mut self, phys: u64) -> &mut u64 {
            let off = (phys - REGION_PHYS) as usize / 8;
            &mut self.region[off]
        }
        /// 按物理地址读 PTE 的闭包（真机 read_pte_dm 的宿主替身）。
        /// 区外物理地址返回 0（对齐真实内存语义：非独占地址空间）。
        fn reader(&self) -> impl FnMut(u64) -> u64 + '_ {
            let region = &*self.region;
            move |phys: u64| {
                let off = (phys.wrapping_sub(REGION_PHYS)) as usize;
                if off % 8 == 0 && off / 8 < region.len() {
                    region[off / 8]
                } else {
                    0
                }
            }
        }
        fn walker(&self, root: u64, va: u64) -> WalkResult {
            let mut rd = self.reader();
            walk_read_with(&mut rd, root, va)
        }
    }

    // ── 参考实现（按 Sv39 规范独立写就，不 import 被测代码）──
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum RefWalk {
        NotPresent,
        Leaf { pa: u64 },
    }

    fn ref_walk(region: &[u64], root_phys: u64, vaddr: u64) -> RefWalk {
        let rd = |pa: u64| -> u64 {
            let off = (pa.wrapping_sub(REGION_PHYS)) as usize;
            if pa >= REGION_PHYS && off / 8 < region.len() {
                region[off / 8]
            } else {
                0
            }
        };
        if cfg!(feature = "walk-hardening") && !is_canonical_sv39(vaddr) {
            return RefWalk::NotPresent;
        }
        let idx = |phys: u64, level_shift: u32| -> usize {
            (((phys.wrapping_sub(REGION_PHYS)) / 8) as usize)
                + (((vaddr >> level_shift) & 0x1FF) as usize)
        };
        let l2e = rd(root_phys + (((vaddr >> L2_SHIFT) & 0x1FF) as u64) * 8);
        if l2e & PTE_V == 0 {
            return RefWalk::NotPresent;
        }
        if l2e & PTE_RWX_MASK != 0 {
            if cfg!(feature = "walk-hardening")
                && (l2e & PTE_PPN_MASK) >> 10 & ((1 << 18) - 1) != 0
            {
                return RefWalk::NotPresent; // 错粒度 1GB 叶
            }
            return RefWalk::Leaf { pa: pte_to_paddr(l2e) | (vaddr & 0x3FFF_FFFF) };
        }
        let l1_base = pte_to_paddr(l2e);
        let l1e = rd(l1_base + (((vaddr >> L1_SHIFT) & 0x1FF) as u64) * 8);
        if l1e & PTE_V == 0 {
            return RefWalk::NotPresent;
        }
        if l1e & PTE_RWX_MASK != 0 {
            if (l1e & PTE_PPN_MASK) >> 10 & ((1 << 9) - 1) != 0 {
                return RefWalk::NotPresent; // 错粒度 2MB 叶
            }
            return RefWalk::Leaf { pa: pte_to_paddr(l1e) | (vaddr & 0x1F_FFFF) };
        }
        let l0_base = pte_to_paddr(l1e);
        let pte = rd(l0_base + (((vaddr >> L0_SHIFT) & 0x1FF) as u64) * 8);
        if pte & PTE_V == 0 || pte & PTE_RWX_MASK == 0 {
            // V=0 不命中；V=1 而 RWX=0 是末级保留形态（硬件必 fault）。
            return RefWalk::NotPresent;
        }
        RefWalk::Leaf { pa: pte_to_paddr(pte) | (vaddr & 0xFFF) }
    }

    /// 被测走表折叠成参考形态（Leaf 的 V/叶形态裁决 = walk_translate 语义）。
    fn walk_under(s: &Synth, root: u64, va: u64) -> RefWalk {
        match s.walker(root, va) {
            WalkResult::Leaf(_, pte) => {
                if pte & PTE_V == 0 || pte & PTE_RWX_MASK == 0 {
                    RefWalk::NotPresent
                } else {
                    RefWalk::Leaf { pa: pte_to_paddr(pte) | (va & 0xFFF) }
                }
            }
            WalkResult::Huge1G(pa, _) | WalkResult::Huge2M(pa, _) => RefWalk::Leaf { pa },
            WalkResult::NotPresent => RefWalk::NotPresent,
        }
    }

    const V: u64 = PTE_V;
    const R: u64 = 1 << 1;
    const W: u64 = 1 << 2;
    const X: u64 = 1 << 3;

    #[test]
    fn happy_4kb_path_and_offset_folding() {
        let mut s = Synth::new(4);
        let (root, l1, l0) = (s.page_phys(0), s.page_phys(1), s.page_phys(2));
        let frame: u64 = 0x8123_4000;
        *s.entry_mut(root + 8 * 3) = paddr_to_pte(l1) | V; // i2=3
        *s.entry_mut(l1 + 8 * 7) = paddr_to_pte(l0) | V; // i1=7
        *s.entry_mut(l0 + 8 * 9) = paddr_to_pte(frame) | V | R | W; // i0=9
        let va = (3 << 30) | (7 << 21) | (9 << 12) | 0xABC;
        match s.walker(root, va) {
            WalkResult::Leaf(_, pte) => assert_eq!(pte_to_paddr(pte) | (va & 0xFFF), frame | 0xABC),
            other => panic!("期望 Leaf，得 {other:?}"),
        }
        assert_eq!(walk_under(&s, root, va), ref_walk(&s.region, root, va));
    }

    #[test]
    fn v0_with_rwx_set_is_not_present_at_every_level() {
        // 腐蚀/保留形态：V=0 但 R|W|X 置位——规范：V=0 一律不命中，
        // 绝不许按叶处理、也绝不许下降。
        let mut s = Synth::new(4);
        let (root, l1, l0) = (s.page_phys(0), s.page_phys(1), s.page_phys(2));
        let ghost = R | W | X; // V=0, RWX 全 1
        *s.entry_mut(root + 8 * 2) = paddr_to_pte(l1) | V;
        *s.entry_mut(l1 + 8 * 6) = paddr_to_pte(l0) | V;
        *s.entry_mut(root + 8 * 1) = paddr_to_pte(l1) | ghost; // L2 ghost
        *s.entry_mut(l1 + 8 * 5) = paddr_to_pte(l0) | ghost; // L1 ghost
        *s.entry_mut(l0 + 8 * 5) = 0x8ABC_D000 | R | W;
        assert_eq!(walk_under(&s, root, (1 << 30)), RefWalk::NotPresent); // L2 ghost
        assert_eq!(
            walk_under(&s, root, (2 << 30) | (5 << 21)),
            RefWalk::NotPresent
        ); // L1 ghost
        assert_eq!(
            walk_under(&s, root, (2 << 30) | (6 << 21) | (5 << 12)),
            ref_walk(&s.region, root, (2 << 30) | (6 << 21) | (5 << 12))
        ); // 真表仍在
    }

    #[test]
    fn w_without_r_is_leaf_shape_at_l0() {
        // 保留编码 W=1,R=0：硬件在「访问」时 fault，但形态上是叶——
        // 走表（翻译）阶段返回叶 PTE，与硬件走表阶段对齐。
        let mut s = Synth::new(4);
        let (root, l1, l0) = (s.page_phys(0), s.page_phys(1), s.page_phys(2));
        *s.entry_mut(root + 8 * 0) = paddr_to_pte(l1) | V;
        *s.entry_mut(l1 + 8 * 0) = paddr_to_pte(l0) | V;
        *s.entry_mut(l0 + 8 * 1) = paddr_to_pte(0x8000_1000) | V | W;
        match s.walker(root, 0x1000) {
            WalkResult::Leaf(_, pte) => assert!(pte & W != 0 && pte & R == 0),
            other => panic!("W-only 叶应返回 Leaf，得 {other:?}"),
        }
    }

    #[cfg(feature = "walk-hardening")]
    #[test]
    fn misaligned_huge_leaf_is_not_present() {
        // 错粒度巨叶：2MB 叶 PPN[9:0]≠0 / 1GB 叶 PPN[18:0]≠0 = 保留形态，
        // 硬件必 fault——此前软件直接 OR 折叠出垃圾 PA（fail-open），现对齐。
        let mut s = Synth::new(4);
        let (root, l1) = (s.page_phys(0), s.page_phys(1));
        *s.entry_mut(root + 8 * 4) = paddr_to_pte(0x8000_1000) | V | R | W; // 1GB 错粒度（PPN[0]=1）
        assert_eq!(walk_under(&s, root, 4 << 30), RefWalk::NotPresent);
        *s.entry_mut(root + 8 * 4) = paddr_to_pte(l1) | V;
        *s.entry_mut(l1 + 8 * 4) = paddr_to_pte(0x8040_1000) | V | R | W; // 2MB 错粒度（PPN[0]=1）
        assert_eq!(walk_under(&s, root, (4 << 30) | (4 << 21)), RefWalk::NotPresent);
        // 对照：对齐 2MB 巨叶正常折叠。
        *s.entry_mut(l1 + 8 * 4) = paddr_to_pte(0x8040_0000) | V | R | W;
        match walk_under(&s, root, (4 << 30) | (4 << 21) | 0x1234) {
            RefWalk::Leaf { pa } => assert_eq!(pa, 0x8040_0000 | 0x1234),
            other => panic!("对齐 2MB 巨叶应折叠成功，得 {other:?}"),
        }
    }

    #[cfg(feature = "walk-hardening")]
    #[test]
    fn non_canonical_va_is_not_present() {
        // §续-349 头号形状：共享根表的内核半区在 root[256..512]——非规范
        // VA（历史栈顶 0x7fff_ffff_f000、run8 的 0x7ffff48b124e6）硬件必
        // fault；此前软件会顺 root[511] 「翻译」成功并给内核代写喂错 PA。
        let mut s = Synth::new(4);
        let (root, l1) = (s.page_phys(0), s.page_phys(1));
        *s.entry_mut(root + 8 * 511) = paddr_to_pte(l1) | V; // 内核半区真在用的表项
        *s.entry_mut(l1 + 8 * 511) = paddr_to_pte(0x8700_0000) | V | R;
        // 非规范 VA（bits 63:39 = 0 ≠ bit38 = 1）：一律 NotPresent。
        assert_eq!(walk_under(&s, root, 0x7fff_ffff_f000), RefWalk::NotPresent);
        assert_eq!(walk_under(&s, root, 0x7fff_f48b_1000), RefWalk::NotPresent);
        // 对照：内核半区规范 VA（高位全 1）仍可走（i2=i1=511，2MB 巨叶）。
        // 对照：内核半区规范 VA（全 1 页，i2=i1=511）仍可走。
        let kva = 0xFFFF_FFFF_FFFF_F000;
        match s.walker(root, kva) {
            WalkResult::Huge2M(pa, _) => assert_eq!(pa, 0x8700_0000 | (kva & 0x1F_FFFF)),
            other => panic!("内核半区规范 VA 应可走，得 {other:?}"),
        }
    }

    #[test]
    fn l0_pointer_shape_is_not_present_semantics() {
        // 末级 V=1 而 RWX=0 = 保留形态：walk_read 原样上报 Leaf，由
        // walk_translate 裁决为 None（本测试锁 walk_under 的对齐语义）。
        let mut s = Synth::new(4);
        let (root, l1, l0) = (s.page_phys(0), s.page_phys(1), s.page_phys(2));
        *s.entry_mut(root + 8 * 0) = paddr_to_pte(l1) | V;
        *s.entry_mut(l1 + 8 * 0) = paddr_to_pte(l0) | V;
        *s.entry_mut(l0 + 8 * 2) = paddr_to_pte(s.page_phys(3)) | V; // V=1, RWX=0
        assert_eq!(walk_under(&s, root, 0x2000), RefWalk::NotPresent);
    }

    #[test]
    fn boundary_indices() {
        // 索引边界：i2/i1/i0 全 0 与全 511 的角。
        let mut s = Synth::new(8);
        let (root, l1, l0, l1k, l0k) =
            (s.page_phys(0), s.page_phys(1), s.page_phys(2), s.page_phys(3), s.page_phys(4));
        *s.entry_mut(root + 0) = paddr_to_pte(l1) | V;
        *s.entry_mut(root + 8 * 511) = paddr_to_pte(l1k) | V; // 内核半区角（规范 VA 才可达）
        *s.entry_mut(l1 + 0) = paddr_to_pte(l0) | V;
        *s.entry_mut(l1k + 8 * 511) = paddr_to_pte(l0k) | V;
        *s.entry_mut(l0 + 0) = paddr_to_pte(0x8100_0000) | V | R;
        *s.entry_mut(l0k + 8 * 511) = paddr_to_pte(0x8200_0000) | V | R;
        // 用户半区零角：i2=i1=i0=0。
        match walk_under(&s, root, 0x0) {
            RefWalk::Leaf { pa } => assert_eq!(pa & !0xFFF, 0x8100_0000),
            other => panic!("零角应命中，得 {other:?}"),
        }
        // 内核半区满角：全 1 页（i2=i1=i0=511）。
        let kva = 0xFFFF_FFFF_FFFF_F000;
        match walk_under(&s, root, kva) {
            RefWalk::Leaf { pa } => assert_eq!(pa & !0xFFF, 0x8200_0000),
            other => panic!("内核满角应命中，得 {other:?}"),
        }
    }

    #[test]
    fn aligned_huge_leaf_with_high_ppn_bits_is_accepted() {
        // §续-350 委托回归真因回归测试：2MB 叶在 paddr 0x80400000
        // （PPN[8]=paddr[21]=1，合法）必须被接受——掩码多含 PPN[9] 会把
        // 它误判为错粒度（真机=kernel 高半区 2MB 映射全灭→kerninfo panic）。
        // 1GB 叶同理：paddr 0xC0000000（PPN[17]=paddr[30]=1）合法。
        let mut s = Synth::new(4);
        let (root, l1) = (s.page_phys(0), s.page_phys(1));
        *s.entry_mut(root + 8 * 2) = paddr_to_pte(l1) | V; // i2=2
        *s.entry_mut(l1 + 8 * 2) = paddr_to_pte(0x8040_0000) | V | R | W; // 2MB 叶，bit21=1
        let va = (2 << 30) | (2 << 21) | 0x4567;
        match walk_under(&s, root, va) {
            RefWalk::Leaf { pa } => assert_eq!(pa, 0x8040_0000 | (va & 0x1F_FFFF)),
            other => panic!("合法 2MB 叶（PPN[8]=1）应被接受，得 {other:?}"),
        }
        // 1GB 叶：paddr 0xC000_0000（PPN[17]=paddr[30]=1，合法）。
        *s.entry_mut(root + 8 * 3) = paddr_to_pte(0xC000_0000) | V | R;
        let va3 = (3 << 30) | 0x89AB;
        match walk_under(&s, root, va3) {
            RefWalk::Leaf { pa } => assert_eq!(pa, 0xC000_0000 | (va3 & 0x3FFF_FFFF)),
            other => panic!("合法 1GB 叶（PPN[17]=1）应被接受，得 {other:?}"),
        }
        // 边界内侧拒绝（错粒度）仅加固态成立；关态=旧语义（接受并折叠）。
        *s.entry_mut(l1 + 8 * 2) = paddr_to_pte(0x8040_1000) | V | R | W;
        if cfg!(feature = "walk-hardening") {
            assert_eq!(walk_under(&s, root, va), RefWalk::NotPresent);
        }
        // 1GB 叶 paddr[29:12]≠0（paddr 0xC000_1000）。
        *s.entry_mut(root + 8 * 3) = paddr_to_pte(0xC000_1000) | V | R;
        if cfg!(feature = "walk-hardening") {
            assert_eq!(walk_under(&s, root, va3), RefWalk::NotPresent);
        }
    }

    #[test]
    fn fuzz_against_reference() {
        // 随机 PTE 场 × 随机 VA 扫描：被测走表与独立参考实现全等。
        let mut s = Synth::new(16);
        let mut seed: u64 = 0x2026_1005;
        let mut rng = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let root = s.page_phys(0);
        for slot in 1..16 {
            let base = s.page_phys(slot);
            for idx in 0..PAGE_ENTRIES {
                *s.entry_mut(base + (idx as u64) * 8) = rng();
            }
        }
        *s.entry_mut(root + 8 * 2) = s.page_phys(3) | V;
        *s.entry_mut(root + 8 * 5) = rng(); // L2 随机形态（叶/表/无效都可能）
        for _ in 0..4000 {
            let va = rng() & 0x0000_3FFF_FFFF_FFFF; // 用户半区规范域
            assert_eq!(
                walk_under(&s, root, va),
                ref_walk(&s.region, root, va),
                "va={va:#x} 走表与参考实现不一致"
            );
        }
        for _ in 0..1000 {
            let va = 0xFFFF_FFC0_0000_0000 | (rng() & 0x0000_3FFF_FFFF_FFFF); // 内核半区
            assert_eq!(
                walk_under(&s, root, va),
                ref_walk(&s.region, root, va),
                "kva={va:#x} 走表与参考实现不一致"
            );
        }
    }
}
