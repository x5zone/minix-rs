//! SD-10 候选 3 改形仪器：页表写「记录-静点走查」采样器（riscv64）。
//!
//! # 谱系（为什么是这形状）
//!
//! `2026-10-19` 晚第一试形（leaf 写后 inline walk-back 四钩：map/remap/
//! unmap/update_flags 写毕当即走表复读）上线即 riscv ATF boot leg 挂死
//! （sig manager 致命 SIGSEGV）——观测装置在被测写窗内重入走表（walk_read
//! 与写窗交互），扰动被测系统活形（案卷 4.2 教训一真机现形）。已全量
//! 还原。改形把「走查」从写窗内挪到**静点**（timer tick 臂，单 hart +
//! BKL 下页表写者必然不在飞），写点只留一条有界记录（环推入）；排水侧
//! 以重入守卫挡 tick 嵌套进 kcall 的二次重入。
//!
//! # 覆盖面（v1 边界，诚实声明）
//!
//! - **KernelDm 叶写（走查级）**：内核侧四个叶写方法（map/remap/unmap/
//!   update_flags）写毕记录 {root, vaddr, want, op}，静点排水时从 root
//!   重走并逐类比对。内核侧手工映射/启动装载走这条。
//! - **网关 PteWrite（槽级）**：VM 经 SYS_VMCTL PteWrite 的内核代写
//!   （KDM 直写+全局 sfence，`syscall.rs` 臂）写毕记录 {槽物理址, 值}，
//!   静点排水时槽复读。wire 只带槽址与值（m1p1/m1p2），vaddr/root 不在
//!   射程（扩 wire = 动 `[ARCH: riscv-vmddm]` 契约，v1 不做）。
//! - **不覆盖**：未注册 VmDm 态的启动期直写（契约论证=not-present→
//!   present，无陈旧项可陈）；`split_huge` 的子表叶复制（启动期一次性）；
//!   PteZero 整页零填（新表无翻译）。SMP 撤钳（C2b）后本仪器须按每 hart
//!   槽位再战（同 tf_guard 前置③）。
//!
//! # 扰动预算（对 4.2 教训一的正面回答）
//!
//! 记录侧**无条件自武装**（tf_guard LIVE 同款先例：N=7 轮实证无害量级），
//! 每次叶写 3–4 条 Relaxed 写 + 环绕计数，无 disarm 分支——有界性由环
//! 容量与排水预算承担，不靠开关。排水侧每 tick 预算内走查 ≤8 条（每条
//! 3 次槽读）+ 槽复读 ≤32 次；打印全 boot ≤1 行活性 + ≤8 行 HIT（封顶
//! 同 tf_guard 房规，串口写是 ATF 瓶颈故零周期打印）。
//!
//! 纯逻辑层（环/分类/预算批取）不依赖 Direct Map 与 CSR，宿主可测
//! （`riscv64_walk` 同款非门控先例）；走查/打印接线在
//! `riscv64::paging::ptwb_drain_budgeted`（门内）。

use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// KernelDm 叶写环容量（条）。
pub const RING_LEAF: usize = 64;
/// 网关写槽级环容量（条）。
pub const RING_SLOT: usize = 128;
/// 每 tick 走查级预算（条）。
pub const DRAIN_WALK_BUDGET: usize = 8;
/// 每 tick 槽级预算（条）。
pub const DRAIN_SLOT_BUDGET: usize = 32;
/// HIT 打印封顶（房规 8，超出只计数）。
pub const PRINT_CAP: usize = 8;

/// 叶写操作标签（记录用；HIT 行打印为静态串）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LeafOp {
    /// `Paging::map`
    Map,
    /// `Paging::remap`
    Remap,
    /// `Paging::unmap`（want = 0）
    Unmap,
    /// `Paging::update_flags`
    UpdateFlags,
}

impl LeafOp {
    /// HIT 行的静态名字（避免运行期格式化）。
    pub fn name(self) -> &'static str {
        match self {
            LeafOp::Map => "map",
            LeafOp::Remap => "remap",
            LeafOp::Unmap => "unmap",
            LeafOp::UpdateFlags => "update_flags",
        }
    }
}

/// 走查分类：表态与写时意图的分歧形状。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HitClass {
    /// 写时意图 = 无效（unmap），表里 V=1——「幽灵复活」。
    Revived,
    /// 写时意图 = 有效（map/remap/update_flags），表里 V=0 或走表不命中。
    Gone,
    /// 两边都有效但 PTE 值不同（PPN 或旗标位漂移）。
    Changed,
    /// 槽级复读：网关写下的值在静点已不是写入值。
    SlotDrift,
}

impl HitClass {
    /// HIT 行的静态名字。
    pub fn name(self) -> &'static str {
        match self {
            HitClass::Revived => "REVIVED",
            HitClass::Gone => "GONE",
            HitClass::Changed => "CHANGED",
            HitClass::SlotDrift => "SLOT-DRIFT",
        }
    }
}

/// Sv39 有效位（与 `Sv39PteFlags::V` 同值；纯层不依赖门内 bitflags）。
pub const PTE_V: u64 = 1 << 0;

// ── 走查级环：{root, vaddr, want, op} × RING_LEAF ──
static LEAF_ROOT: [AtomicUsize; RING_LEAF] = [const { AtomicUsize::new(0) }; RING_LEAF];
static LEAF_VADDR: [AtomicUsize; RING_LEAF] = [const { AtomicUsize::new(0) }; RING_LEAF];
static LEAF_WANT: [AtomicUsize; RING_LEAF] = [const { AtomicUsize::new(0) }; RING_LEAF];
static LEAF_OP: [AtomicUsize; RING_LEAF] = [const { AtomicUsize::new(0) }; RING_LEAF];

// ── 槽级环：{槽物理址, 写入值} × RING_SLOT ──
static SLOT_PA: [AtomicUsize; RING_SLOT] = [const { AtomicUsize::new(0) }; RING_SLOT];
static SLOT_VAL: [AtomicUsize; RING_SLOT] = [const { AtomicUsize::new(0) }; RING_SLOT];

// head = 下一条写入槽位；tail = 最旧未排水样本槽位；COUNT = 在环样本数
//（0..=容量，满环写入选最旧覆盖——「抽查」语义：排水预算追不上时保最新
// 样本）；WRAPS = 环绕次数。计数式环避开口/满歧义（head==tail 既形）。
static LEAF_HEAD: AtomicUsize = AtomicUsize::new(0);
static LEAF_TAIL: AtomicUsize = AtomicUsize::new(0);
static LEAF_COUNT: AtomicUsize = AtomicUsize::new(0);
static LEAF_WRAPS: AtomicUsize = AtomicUsize::new(0);
static SLOT_HEAD: AtomicUsize = AtomicUsize::new(0);
static SLOT_TAIL: AtomicUsize = AtomicUsize::new(0);
static SLOT_COUNT: AtomicUsize = AtomicUsize::new(0);
static SLOT_WRAPS: AtomicUsize = AtomicUsize::new(0);

// ── 读数计数（结账行/负控用）与排水重入守卫 ──
static VERIFIED_LEAF: AtomicUsize = AtomicUsize::new(0);
static VERIFIED_SLOT: AtomicUsize = AtomicUsize::new(0);
static DRAINS: AtomicUsize = AtomicUsize::new(0);
static HITS: AtomicUsize = AtomicUsize::new(0);
static DRAINING: AtomicBool = AtomicBool::new(false);
static LIVE: AtomicBool = AtomicBool::new(false);
static PRINTED: AtomicUsize = AtomicUsize::new(0);

/// 活性行一次性凭证：首个有核验的排水打印一行执行记录（swap 保证只发
/// 一次；打印本体在门内排水 fn，串口写不进纯层）。
pub fn first_live() -> bool {
    !LIVE.swap(true, Ordering::AcqRel)
}

/// HIT 打印全局许可（v3：封顶是**全 boot** ≤8 行，不是每排水 8 行——
/// v1/v2 按次重置让串口在 HIT 面前刷屏，串口写是 ATF 瓶颈，观测装置
/// 自扰动正是第一试形负结果的教训形状）。取到许可才打。
pub fn take_print_permit() -> bool {
    PRINTED.fetch_add(1, Ordering::AcqRel) < PRINT_CAP
}

/// 排水重入守卫：tick 嵌套进 kcall 时第二重排水直接退（失败试形的
/// 教训正是重入；swap 保证嵌套者拿不到）。返回 true = 本重是外层。
pub fn drain_guard_enter() -> bool {
    !DRAINING.swap(true, Ordering::AcqRel)
}

/// 外层排水结束放行。
pub fn drain_guard_exit() {
    DRAINING.store(false, Ordering::Release);
}

/// KernelDm 叶写记录（BKL 临界区单写者，原子只为 SMP 撤钳前卫生）。
/// 同键（root,vaddr）在环即**就地更新期望值**（v2：后续合法写刷新期望，
/// 不堆新样本）——去重后静点仍漂移 = 「无记录写者改了表」，正是污染
/// 签名；满环覆盖最旧，tail 前移保持排水侧只读未覆盖样本。
pub fn record_leaf(root: u64, vaddr: u64, want: u64, op: LeafOp) {
    // 同键去重：只在在环（tail..tail+count）里扫。
    let count = LEAF_COUNT.load(Ordering::Acquire);
    let tail = LEAF_TAIL.load(Ordering::Acquire);
    for k in 0..count {
        let j = (tail + k) % RING_LEAF;
        if LEAF_ROOT[j].load(Ordering::Relaxed) == root as usize
            && LEAF_VADDR[j].load(Ordering::Relaxed) == vaddr as usize
        {
            LEAF_WANT[j].store(want as usize, Ordering::Relaxed);
            LEAF_OP[j].store(op as usize, Ordering::Relaxed);
            return;
        }
    }
    let i = LEAF_HEAD.load(Ordering::Acquire);
    LEAF_ROOT[i].store(root as usize, Ordering::Relaxed);
    LEAF_VADDR[i].store(vaddr as usize, Ordering::Relaxed);
    LEAF_WANT[i].store(want as usize, Ordering::Relaxed);
    LEAF_OP[i].store(op as usize, Ordering::Relaxed);
    let head_after = (i + 1) % RING_LEAF;
    LEAF_HEAD.store(head_after, Ordering::Release);
    if head_after == 0 {
        LEAF_WRAPS.fetch_add(1, Ordering::Relaxed);
    }
    // 满环：覆盖最旧（tail 同步前移），count 封顶在容量。
    let count = LEAF_COUNT.load(Ordering::Acquire);
    if count < RING_LEAF {
        LEAF_COUNT.store(count + 1, Ordering::Release);
    } else {
        LEAF_TAIL.store((LEAF_TAIL.load(Ordering::Acquire) + 1) % RING_LEAF, Ordering::Release);
    }
}

/// 网关 PteWrite 槽级记录（同键=槽物理址，去重语义同 `record_leaf` v2）。
pub fn record_slot(slot_pa: u64, val: u64) {
    let count = SLOT_COUNT.load(Ordering::Acquire);
    let tail = SLOT_TAIL.load(Ordering::Acquire);
    for k in 0..count {
        let j = (tail + k) % RING_SLOT;
        if SLOT_PA[j].load(Ordering::Relaxed) == slot_pa as usize {
            SLOT_VAL[j].store(val as usize, Ordering::Relaxed);
            return;
        }
    }
    let i = SLOT_HEAD.load(Ordering::Acquire);
    SLOT_PA[i].store(slot_pa as usize, Ordering::Relaxed);
    SLOT_VAL[i].store(val as usize, Ordering::Relaxed);
    let head_after = (i + 1) % RING_SLOT;
    SLOT_HEAD.store(head_after, Ordering::Release);
    if head_after == 0 {
        SLOT_WRAPS.fetch_add(1, Ordering::Relaxed);
    }
    let count = SLOT_COUNT.load(Ordering::Acquire);
    if count < RING_SLOT {
        SLOT_COUNT.store(count + 1, Ordering::Release);
    } else {
        SLOT_TAIL.store((SLOT_TAIL.load(Ordering::Acquire) + 1) % RING_SLOT, Ordering::Release);
    }
}

/// 排水侧取一批走查级样本（最旧先出，至多 `out.len()` 条）。返回实取
/// 条数，样本写进 `out`（每条 {root, vaddr, want, op}）。
pub fn take_leaf_batch(out: &mut [[u64; 4]]) -> usize {
    let mut n = 0;
    while n < out.len() {
        let count = LEAF_COUNT.load(Ordering::Acquire);
        if count == 0 {
            break;
        }
        let tail = LEAF_TAIL.load(Ordering::Acquire);
        out[n] = [
            LEAF_ROOT[tail].load(Ordering::Relaxed) as u64,
            LEAF_VADDR[tail].load(Ordering::Relaxed) as u64,
            LEAF_WANT[tail].load(Ordering::Relaxed) as u64,
            LEAF_OP[tail].load(Ordering::Relaxed) as u64,
        ];
        LEAF_TAIL.store((tail + 1) % RING_LEAF, Ordering::Release);
        LEAF_COUNT.store(count - 1, Ordering::Release);
        n += 1;
    }
    n
}

/// 排水侧取一批槽级样本（最旧先出，至多 `out.len()` 条）。返回实取
/// 条数，样本写进 `out`（每条 {槽物理址, 写入值}）。
pub fn take_slot_batch(out: &mut [[u64; 2]]) -> usize {
    let mut n = 0;
    while n < out.len() {
        let count = SLOT_COUNT.load(Ordering::Acquire);
        if count == 0 {
            break;
        }
        let tail = SLOT_TAIL.load(Ordering::Acquire);
        out[n] = [
            SLOT_PA[tail].load(Ordering::Relaxed) as u64,
            SLOT_VAL[tail].load(Ordering::Relaxed) as u64,
        ];
        SLOT_TAIL.store((tail + 1) % RING_SLOT, Ordering::Release);
        SLOT_COUNT.store(count - 1, Ordering::Release);
        n += 1;
    }
    n
}

/// 叶级分类（纯函数）：写时意图 vs 静点表态。
pub fn classify_leaf(want: u64, got: u64) -> Option<HitClass> {
    if want & PTE_V == 0 {
        if got & PTE_V != 0 {
            Some(HitClass::Revived)
        } else {
            None
        }
    } else if got & PTE_V == 0 {
        Some(HitClass::Gone)
    } else if want != got {
        Some(HitClass::Changed)
    } else {
        None
    }
}

/// 槽级分类（纯函数）：静点复读值 ≠ 写入值即漂移。
pub fn classify_slot(want: u64, got: u64) -> Option<HitClass> {
    if want == got {
        None
    } else {
        Some(HitClass::SlotDrift)
    }
}

/// 排水一次记一笔（活性/结账读数）。
pub fn note_drain(verified_leaf: usize, verified_slot: usize, hits: usize) {
    DRAINS.fetch_add(1, Ordering::Relaxed);
    VERIFIED_LEAF.fetch_add(verified_leaf, Ordering::Relaxed);
    VERIFIED_SLOT.fetch_add(verified_slot, Ordering::Relaxed);
    HITS.fetch_add(hits, Ordering::Relaxed);
}

/// 活性/结账读数（结账行四件套用）：（排水次数, 走查级核验数, 槽级
/// 核验数, HIT 数, 环绕次数和）。
pub fn readings() -> (usize, usize, usize, usize, usize) {
    (
        DRAINS.load(Ordering::Relaxed),
        VERIFIED_LEAF.load(Ordering::Relaxed),
        VERIFIED_SLOT.load(Ordering::Relaxed),
        HITS.load(Ordering::Relaxed),
        LEAF_WRAPS.load(Ordering::Relaxed) + SLOT_WRAPS.load(Ordering::Relaxed),
    )
}

#[cfg(test)]
mod ptwb_tests {
    use super::*;

    // 环是全进程静态，测试间互相污染——每个用例先清场再跑（单线程
    // 测试语义下安全）。
    fn reset_ring() {
        LEAF_HEAD.store(0, Ordering::Relaxed);
        LEAF_TAIL.store(0, Ordering::Relaxed);
        LEAF_COUNT.store(0, Ordering::Relaxed);
        LEAF_WRAPS.store(0, Ordering::Relaxed);
        SLOT_HEAD.store(0, Ordering::Relaxed);
        SLOT_TAIL.store(0, Ordering::Relaxed);
        SLOT_COUNT.store(0, Ordering::Relaxed);
        SLOT_WRAPS.store(0, Ordering::Relaxed);
        DRAINING.store(false, Ordering::Relaxed);
        for i in 0..RING_LEAF {
            LEAF_ROOT[i].store(0, Ordering::Relaxed);
            LEAF_VADDR[i].store(0, Ordering::Relaxed);
            LEAF_WANT[i].store(0, Ordering::Relaxed);
            LEAF_OP[i].store(0, Ordering::Relaxed);
        }
        for i in 0..RING_SLOT {
            SLOT_PA[i].store(0, Ordering::Relaxed);
            SLOT_VAL[i].store(0, Ordering::Relaxed);
        }
    }

    #[test]
    fn classify_leaf_matrix() {
        // unmap 意图（want V=0）
        assert_eq!(classify_leaf(0, 0), None);
        assert_eq!(classify_leaf(0, PTE_V | 0x123), Some(HitClass::Revived));
        // map 意图（want V=1）
        let want = PTE_V | (1 << 10) | (1 << 6) | (1 << 7); // V|R|A|D 一帧
        assert_eq!(classify_leaf(want, want), None);
        assert_eq!(classify_leaf(want, 0), Some(HitClass::Gone));
        assert_eq!(classify_leaf(want, want ^ (1 << 10)), Some(HitClass::Changed));
        // 旗标位单比特漂移同为 Changed
        assert_eq!(classify_leaf(want, want | (1 << 4)), Some(HitClass::Changed));
    }

    #[test]
    fn classify_slot_matrix() {
        assert_eq!(classify_slot(0xabc, 0xabc), None);
        assert_eq!(classify_slot(0xabc, 0), Some(HitClass::SlotDrift));
    }

    #[test]
    fn leaf_ring_fifo_within_capacity() {
        reset_ring();
        for k in 0..5u64 {
            record_leaf(0x8000_0000 + k, k * 0x1000, PTE_V | k, LeafOp::Map);
        }
        let mut out = [[0u64; 4]; DRAIN_WALK_BUDGET];
        let n = take_leaf_batch(&mut out);
        assert_eq!(n, 5);
        for (k, e) in out.iter().enumerate().take(5) {
            assert_eq!(e[0], 0x8000_0000 + k as u64);
            assert_eq!(e[1], (k as u64) * 0x1000);
            assert_eq!(e[3], LeafOp::Map as u64);
        }
        // 排空后再取 = 0
        assert_eq!(take_leaf_batch(&mut out), 0);
    }

    #[test]
    fn leaf_ring_overwrites_oldest_when_full() {
        reset_ring();
        for k in 0..(RING_LEAF as u64 + 3) {
            record_leaf(k, k, PTE_V, LeafOp::Unmap);
        }
        assert!(LEAF_WRAPS.load(Ordering::Relaxed) >= 1);
        let mut out = [[0u64; 4]; RING_LEAF];
        let n = take_leaf_batch(&mut out);
        // 环满覆盖后只剩最新 RING_LEAF 条：seq 3 … RING_LEAF+2（0..2 被挤掉）
        assert_eq!(n, RING_LEAF);
        assert_eq!(out[0][0], 3);
        assert_eq!(out[RING_LEAF - 1][0], (RING_LEAF as u64 + 2));
    }

    #[test]
    fn slot_ring_batch_and_budget() {
        reset_ring();
        for k in 0..40u64 {
            record_slot(0x1000 + k * 8, 0xdead + k);
        }
        let mut out = [[0u64; 2]; DRAIN_SLOT_BUDGET];
        assert_eq!(take_slot_batch(&mut out), DRAIN_SLOT_BUDGET);
        assert_eq!(out[0][0], 0x1000);
        assert_eq!(out[0][1], 0xdead);
        // 余 8 条一次收干
        let mut out2 = [[0u64; 2]; DRAIN_SLOT_BUDGET];
        assert_eq!(take_slot_batch(&mut out2), 8);
        assert_eq!(out2[7][0], 0x1000 + 39 * 8);
        assert_eq!(take_slot_batch(&mut out2), 0);
    }

    #[test]
    fn readings_accumulate() {
        reset_ring();
        record_leaf(1, 2, PTE_V, LeafOp::Map);
        record_slot(3, 4);
        let mut leaf_out = [[0u64; 4]; DRAIN_WALK_BUDGET];
        let mut slot_out = [[0u64; 2]; DRAIN_SLOT_BUDGET];
        let nl = take_leaf_batch(&mut leaf_out);
        let ns = take_slot_batch(&mut slot_out);
        note_drain(nl, ns, 0);
        let (d, vl, vs, h, _w) = readings();
        assert_eq!((d, vl, vs, h), (1, 1, 1, 0));
    }

    #[test]
    fn leaf_ring_dedup_updates_expectation_in_place() {
        reset_ring();
        record_leaf(0x8000_0000, 0x1000, PTE_V | 1, LeafOp::Map);
        record_leaf(0x8000_0000, 0x2000, PTE_V | 2, LeafOp::Map);
        // 同键重写：期望值就地刷新，不追加样本
        record_leaf(0x8000_0000, 0x1000, PTE_V | 9, LeafOp::UpdateFlags);
        let mut out = [[0u64; 4]; RING_LEAF];
        assert_eq!(take_leaf_batch(&mut out), 2, "去重后仍只 2 条在环");
        assert_eq!(out[0][1], 0x1000);
        assert_eq!(out[0][2], PTE_V | 9, "期望值被刷新");
        assert_eq!(out[0][3], LeafOp::UpdateFlags as u64);
        assert_eq!(out[1][1], 0x2000);
    }

    #[test]
    fn slot_ring_dedup_updates_val_in_place() {
        reset_ring();
        record_slot(0x8300, 0xaaa);
        record_slot(0x8400, 0xbbb);
        record_slot(0x8300, 0xccc);
        let mut out = [[0u64; 2]; RING_SLOT];
        assert_eq!(take_slot_batch(&mut out), 2);
        assert_eq!(out[0][0], 0x8300);
        assert_eq!(out[0][1], 0xccc, "同槽重写刷新期望值");
        assert_eq!(out[1][0], 0x8400);
    }

    #[test]
    fn dedupe_ignores_drained_entries() {
        reset_ring();
        record_slot(0x9000, 0x1);
        let mut out = [[0u64; 2]; RING_SLOT];
        take_slot_batch(&mut out); // 排干
        // 再记同键：旧条目已排水，应作为新样本入环
        record_slot(0x9000, 0x2);
        assert_eq!(take_slot_batch(&mut out), 1);
    }

    #[test]
    fn drain_guard_blocks_recursion() {
        reset_ring();
        assert!(drain_guard_enter());
        // 嵌套者拿不到守卫
        assert!(!drain_guard_enter());
        drain_guard_exit();
        // 放行后可再入
        assert!(drain_guard_enter());
        drain_guard_exit();
    }

    #[test]
    fn print_permit_is_global_boot_cap() {
        // 注意：PRINTED 是全进程静态，与真机共享此测试进程内先发计数——
        // 清零后取满 PRINT_CAP 张、第 9 张拒绝。
        let start = PRINTED.load(Ordering::Relaxed);
        let budget = if start < PRINT_CAP { PRINT_CAP - start } else { 0 };
        for _ in 0..budget {
            assert!(take_print_permit());
        }
        assert!(!take_print_permit(), "第 {} 张后必须拒绝", PRINT_CAP);
        // 旧读数还原（测试间卫生；超出部分不再回收——本测试进程内后续
        // 用例不再依赖打印许可）。
    }

    #[test]
    fn op_names_are_static() {
        assert_eq!(LeafOp::Map.name(), "map");
        assert_eq!(LeafOp::Unmap.name(), "unmap");
        assert_eq!(HitClass::Revived.name(), "REVIVED");
        assert_eq!(HitClass::SlotDrift.name(), "SLOT-DRIFT");
    }
}
