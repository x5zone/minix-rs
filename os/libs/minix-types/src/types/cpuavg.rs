//! `cpuavg` — 每进程 CPU 利用率的衰减均值（C `minix3/minix/lib/libsys/cpuavg.c` 全文）。
//!
//! **为什么在 `minix-types`**：C 的文件头注释把理由写得很直白——这套算术
//! **内核与 MIB 服务共用**（`cpuavg.c:16-24`）。内核在"某进程累计消耗了
//! 约一个时钟滴答的周期"时记账（不是时钟中断里，`cpuavg.c:44-51`），MIB
//! 则在取到内核进程表快照后按需把均值**追平到当前时刻**（`ps(1)` 那类
//! 消费者看到的才是新鲜值）。两侧必须是同一份实现，否则同一次采样在两个
//! 消费者眼里数值不同。线上形 [`CpuAvgSnap`]（C `struct cpuavg`，
//! `kernel/type.h:80-85`）住 [`crate::types::proc_info`]，本模块只放算术。
//!
//! **算法**（NetBSD 同源）：每秒一次
//! `avg = ccpu * avg + (1 - ccpu) * (run / hz)`，其中 `ccpu = e**(-1/20)`
//! 使均值 60 秒后只剩 5%。实现上的两个取舍：
//! - **懒更新**：只有进程在某个滴答上被记到才推进统计，因此任何时刻的均值
//!   都可能是"旧"的——这正是 MIB 侧要 `update` 追平的原因。
//! - **延迟一秒**：上一秒的 run 先存在 `ca_last`，本秒才并入均值，于是
//!   可以顺带产出"最近一秒"的短期活动估计（`estcpu`）。
//!
//! 定点：`FSHIFT = 11`、`FSCALE = 2048`（C `sys/sys/param.h:435-436`）；
//! 衰减用两级查表（`ccpu_low`/`ccpu_high`）把每次更新限制在两次乘法内
//! （`cpuavg.c:112-117`）。
//!
//! 与 C 的唯一差别是**宽度**：C 的 `clock_t` 是 32 位，本移植按仓库惯例
//! 把时刻与 `hz` 拓宽到 `u64`（与 `ProcInfoStruct` 的时间字段同一裁决），
//! 算术次序与截断逐行照抄。

use crate::types::proc_info::CpuAvgSnap;

/// `FSHIFT` — 定点小数位数（C `sys/sys/param.h:435`）。
pub const FSHIFT: u32 = 11;

/// `FSCALE` — `1 << FSHIFT`（C `sys/sys/param.h:436`）。
pub const FSCALE: u32 = 1 << FSHIFT;

/// `CCPUTAB_SHIFT`（C `cpuavg.c:74`）：高位表每格代表 2^3 = 8 秒。
pub const CCPUTAB_SHIFT: u32 = 3;

/// `CCPUTAB_MASK`（C `cpuavg.c:75`）。
pub const CCPUTAB_MASK: u32 = (1 << CCPUTAB_SHIFT) - 1;

/// `ccpu_low`（C `cpuavg.c:80-84`）：`e**(-n/20) * FSCALE`，n = 1..7。
///
/// 值按 C 的 `(uint32_t)(n * FSCALE)` **截断**（非四舍五入）——本表的每个
/// 数字都由 `cpuavg_ccpu_tables_match_c` 测试钉住，重算方式与 C 的
/// 编译期双精度乘法一致。
pub const CCPU_LOW: [u32; 7] = [1948, 1853, 1762, 1676, 1594, 1517, 1443];

/// `ccpu_high`（C `cpuavg.c:88-96`）：`e**(-8n/20) * FSCALE`，n = 1..19。
///
/// 表长即"衰减到 0"的边界：`decay` 的提前返回判据是
/// `secs > 19 << 3 = 152`（C `cpuavg.c:130-131`）。
pub const CCPU_HIGH: [u32; 19] = [
    1372, 920, 616, 413, 277, 185, 124, 83, 55, 37, 25, 16, 11, 7, 5, 3, 2, 1, 1,
];

/// `ccpu`（C `cpuavg.c:85`）：`ccpu_low[0]`，即 `e**(-1/20) * FSCALE`。
pub const CCPU: u32 = CCPU_LOW[0];

/// `cpuavg_decay`（C `cpuavg.c:118-145`）：把均值按 `secs` 秒衰减。
///
/// 两级查表：先按"每格 8 秒"的高位表走 `secs >> 3` 格，再按低位表走余数。
/// `secs` 超出高位表覆盖范围时直接归零（表尾之后的乘积在 FSCALE 单位下
/// 已经是 0）。
pub fn decay(avg: u32, secs: u32) -> u32 {
    if secs > (CCPU_HIGH.len() as u32) << CCPUTAB_SHIFT {
        return 0;
    }

    let mut avg = avg;
    let mut secs = secs;

    if secs > CCPUTAB_MASK {
        let slot = (secs >> CCPUTAB_SHIFT) - 1;
        avg = (CCPU_HIGH[slot as usize] * avg) >> FSHIFT; // decay #3
        secs &= CCPUTAB_MASK;
    }

    if secs > 0 {
        avg = (CCPU_LOW[secs as usize - 1] * avg) >> FSHIFT; // decay #4
    }

    avg
}

/// `cpuavg_update`（C `cpuavg.c:153-214`）：把统计时间轴向前推进到 `now`。
///
/// 最多四步（C 注释里的 decay #1..#4），因此是 O(1)：
/// 1. 不足一秒 → 什么都不做（懒更新）；
/// 2. 先把上一秒的 `ca_last` 并入均值（decay #1），`run` 挪进 `last`，
///    时间轴前移一秒；
/// 3. 若还剩至少一秒，说明挪进来的 `last` 也过期了，再并一次（decay #2）；
/// 4. 剩下的整秒交给 [`decay`] 批量衰减（decay #3/#4），时间轴按整秒对齐
///    前移。
///
/// 前置：`hz > 0`（C 直接除，本移植同样不做零除保护）。
pub fn update(ca: &mut CpuAvgSnap, now: u64, hz: u64) {
    debug_assert!(hz > 0, "hz 为 0 时 C 会除零");

    // C 的 `delta = now - ca_base` 是有符号 `clock_t` 差：`now < ca_base` 时
    // 得到负值，下面的 `delta < hz` 直接早退。用 saturating_sub 得到同样的
    // 早退（0 < hz）。
    let mut delta = now.saturating_sub(ca.ca_base);

    if delta < hz {
        return;
    }

    // decay #1：并入上一秒的 run 比例，时间轴前移一个虚拟秒。
    ca.ca_avg = (CCPU * ca.ca_avg) >> FSHIFT;
    ca.ca_avg += ((FSCALE - CCPU) * (ca.ca_last as u64 / hz) as u32) >> FSHIFT;

    ca.ca_last = ca.ca_run;
    ca.ca_run = 0;

    ca.ca_base += hz;
    delta -= hz;

    if delta < hz {
        return;
    }

    // decay #2：`last` 也过期了（`run` 此刻已经是 0）。
    ca.ca_avg = (CCPU * ca.ca_avg) >> FSHIFT;
    ca.ca_avg += ((FSCALE - CCPU) * (ca.ca_last as u64 / hz) as u32) >> FSHIFT;

    ca.ca_last = 0;

    ca.ca_base += hz;
    delta -= hz;

    if delta < hz {
        return;
    }

    // 余下的整秒批量衰减，并按整秒对齐前移时间轴。
    let secs = (delta / hz) as u32;
    ca.ca_avg = decay(ca.ca_avg, secs);
    ca.ca_base += secs as u64 * hz;
}

/// `cpuavg_increment`（C `cpuavg.c:224-239`）：时钟滴答记到这个进程头上。
///
/// 首次记账只把 `ca_base` 对齐到 `now`（`ca_base == 0` 是"未初始化"判据）；
/// 之后先 [`update`] 追平，再给本秒的 run 计数加一个 `FSCALE`
/// （= 一个整滴答）。
pub fn increment(ca: &mut CpuAvgSnap, now: u64, hz: u64) {
    if ca.ca_base == 0 {
        ca.ca_base = now;
    } else {
        update(ca, now, hz);
    }

    ca.ca_run += FSCALE;
}

/// [`getstats`] 的三元结果（C 的三个出参）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuAvgStats {
    /// 衰减均值本身（FSCALE 单位，100% ≈ `FSCALE`）。
    pub avg: u32,
    /// 本秒到目前为止的运行滴答数（`l_cpticks`）。
    pub cpticks: u32,
    /// 最近一秒的 CPU 占用百分比（0..100，`l_pctcpu` 的输入）。
    pub estcpu: u32,
}

/// `cpuavg_getstats`（C `cpuavg.c:250-276`）：取三个统计量，**不改**原结构。
///
/// C 的用法是"拷一份再算"（`ca = *ca_orig`），所以 MIB 侧读到的是新鲜值
/// 而内核表里的原值不动。这里同样只读借用。
pub fn getstats(ca: &CpuAvgSnap, now: u64, hz: u64) -> CpuAvgStats {
    debug_assert!(hz > 0, "hz 为 0 时 C 会除零");

    let mut ca = *ca;
    update(&mut ca, now, hz);

    // 把上一秒并入均值——`getstats` 看到的是"刚刚结束的那一秒"。
    ca.ca_avg = (CCPU * ca.ca_avg) >> FSHIFT;
    ca.ca_avg += ((FSCALE - CCPU) * (ca.ca_last as u64 / hz) as u32) >> FSHIFT;

    CpuAvgStats {
        avg: ca.ca_avg,
        cpticks: ca.ca_run >> FSHIFT,
        // C 的 estcpu 就是"最近一秒的占用百分比"（`cpuavg.c:266-272`）。
        estcpu: ((ca.ca_last as u64 / hz * 100) as u32) >> FSHIFT,
    }
}

/// `cpuavg_getccpu`（C `cpuavg.c:281-286`）：衰减常数本身（FSCALE 单位）。
pub const fn getccpu() -> u32 {
    CCPU
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 常量表与 C 逐值钉死（C 的 `F(n) = (uint32_t)(n * FSCALE)`，编译期
    /// 双精度乘后**截断**）。任一格漂移都会让所有进程的 `l_pctcpu` 系统性
    /// 偏移，而单测里看不出来——所以在这里钉死。
    #[test]
    fn cpuavg_ccpu_tables_match_c() {
        assert_eq!(FSHIFT, 11);
        assert_eq!(FSCALE, 2048);
        assert_eq!(CCPUTAB_SHIFT, 3);
        assert_eq!(CCPUTAB_MASK, 7);
        // e**(-n/20)*2048 截断，n=1..7（C cpuavg.c:80-84）。
        assert_eq!(CCPU_LOW, [1948, 1853, 1762, 1676, 1594, 1517, 1443]);
        // e**(-8n/20)*2048 截断，n=1..19（C cpuavg.c:88-96）。
        assert_eq!(
            CCPU_HIGH,
            [1372, 920, 616, 413, 277, 185, 124, 83, 55, 37, 25, 16, 11, 7, 5, 3, 2, 1, 1]
        );
        assert_eq!(CCPU, 1948);
        assert_eq!(getccpu(), CCPU);
    }

    /// `decay` 的四个分支：超界归零、高位表段、低位表段、零秒恒等。
    #[test]
    fn decay_branches() {
        // 超界：`secs > 19*8 = 152` → 直接 0（C cpuavg.c:130-131）。
        assert_eq!(decay(FSCALE, 153), 0);
        assert_eq!(decay(FSCALE, 1000), 0);
        // 零秒：不动。
        assert_eq!(decay(FSCALE, 0), FSCALE);
        // 低位表：1 秒衰减 = ccpu_low[0]。
        assert_eq!(decay(FSCALE, 1), (CCPU_LOW[0] * FSCALE) >> FSHIFT);
        // 高位表 + 余数：9 秒 = 8 秒格 ×1 + 低位 ×1。
        let expect = (CCPU_LOW[0] * ((CCPU_HIGH[0] * FSCALE) >> FSHIFT)) >> FSHIFT;
        assert_eq!(decay(FSCALE, 9), expect);
        // 8 秒整：只走高位表。
        assert_eq!(decay(FSCALE, 8), (CCPU_HIGH[0] * FSCALE) >> FSHIFT);
    }

    /// `increment` 的首次调用只对齐时间轴；后续调用按秒推进并把本秒的
    /// run 计数加满一个 FSCALE。
    #[test]
    fn increment_first_call_aligns_then_accumulates() {
        let hz = 100u64;
        let mut ca = CpuAvgSnap::default();
        // ca_base == 0 → 只对齐，不 decay（C cpuavg.c:227-228）。
        increment(&mut ca, 500, hz);
        assert_eq!(ca.ca_base, 500);
        assert_eq!(ca.ca_run, FSCALE);
        assert_eq!(ca.ca_avg, 0, "首次不产生均值");
        // 同一秒内再来一次：run 继续加，均值仍不动（delta < hz）。
        increment(&mut ca, 550, hz);
        assert_eq!(ca.ca_run, 2 * FSCALE);
        assert_eq!(ca.ca_avg, 0);
        // 跨过**两秒**（delta == 2*hz）：C 会走 decay #1 **和** #2 ——
        // 第一步后 delta == hz 不满足 `delta < hz`，于是继续第二步
        // （cpuavg.c:183-198 的既有行为，照抄）。
        increment(&mut ca, 700, hz);
        assert_eq!(ca.ca_base, 700, "两秒各推进一步");
        assert_eq!(ca.ca_last, 0, "第二步把 last 也并掉并清零");
        assert_eq!(ca.ca_run, FSCALE, "本秒重新起算一个滴答");
        // 两步各并入一次"2 个滴答 / 100 秒"：100 * 40 >> 11 = 1，两次都是 1。
        assert_eq!(ca.ca_avg, 1);
    }

    /// `update` 的四步：一秒、两秒、多秒批量，以及"时间倒流"（C 的负
    /// delta）早退。
    #[test]
    fn update_steps_and_backwards_time() {
        let hz = 100u64;
        let mut ca = CpuAvgSnap {
            ca_base: 1000,
            ca_run: FSCALE,
            ca_last: FSCALE,
            ca_avg: FSCALE,
            _padding: 0,
        };
        // 不足一秒：什么都不动。
        let before = ca;
        update(&mut ca, 1050, hz);
        assert_eq!(ca, before);
        // 时间倒流（now < ca_base）：C 的负 delta 早退，同样不动。
        update(&mut ca, 900, hz);
        assert_eq!(ca, before);
        // 恰好一秒：decay #1，run→last、last 归零。
        update(&mut ca, 1100, hz);
        assert_eq!(ca.ca_base, 1100);
        assert_eq!(ca.ca_last, FSCALE);
        assert_eq!(ca.ca_run, 0);
        assert_ne!(ca.ca_avg, FSCALE, "均值已衰减并并入上一秒");
        // 十秒：走 decay #1/#2 + 批量衰减。
        let mut ca = CpuAvgSnap {
            ca_base: 1000,
            ca_run: 0,
            ca_last: 0,
            ca_avg: FSCALE,
            _padding: 0,
        };
        update(&mut ca, 1000 + 10 * hz, hz);
        assert_eq!(ca.ca_base, 1000 + 10 * hz, "时间轴对齐到整秒");
        // 本场景 ca_last 为 0，故 #1/#2 两步只有衰减项、没有并入项；
        // 余下 8 秒走批量 decay（高位表一格）。逐项写出来而不是抄捷径，
        // 免得把"两步各乘一次 ccpu"误写成"一步"。
        let step = |avg: u32| (CCPU * avg) >> FSHIFT;
        assert_eq!(ca.ca_avg, decay(step(step(FSCALE)), 8));
    }

    /// `getstats`：只读、追平到 now，三个量按 C 的公式出；且**不改原结构**
    /// （C 的 `ca = *ca_orig` 拷贝语义）。
    #[test]
    fn getstats_is_pure_and_matches_c_formulas() {
        let hz = 100u64;
        let orig = CpuAvgSnap {
            ca_base: 1000,
            ca_run: 50 * FSCALE, // 本秒 50 个滴答
            ca_last: 25 * FSCALE, // 上一秒 25 个滴答
            ca_avg: FSCALE,
            _padding: 0,
        };
        let stats = getstats(&orig, 1100, hz);
        // 原结构一个字节不动。
        assert_eq!(orig.ca_base, 1000);
        assert_eq!(orig.ca_run, 50 * FSCALE);
        // cpticks = ca_run >> FSHIFT（C cpuavg.c:264）——注意 update 把
        // run 挪进了 last 并清零，故这里是"更新后"的 run。
        assert_eq!(stats.cpticks, 0);
        // estcpu 用的是 update **之后**的 `ca_last`——update 把本秒的 run
        // （50 个滴答）挪进了 last，所以这里反映的是"刚结束的那一秒"，
        // 不是入参里的旧 last（C cpuavg.c:264-273 的同一语义）。
        let expect_est = ((50 * FSCALE as u64 / hz * 100) as u32) >> FSHIFT;
        assert_eq!(stats.estcpu, expect_est);
        assert_eq!(expect_est, 50, "50 滴答 / 100 hz = 50%");
        assert!(stats.estcpu <= 100, "百分比不越界");
        // avg 走 C 的两步（update 的 #1 + getstats 的并入）。
        let mut ca = orig;
        update(&mut ca, 1100, hz);
        ca.ca_avg = (CCPU * ca.ca_avg) >> FSHIFT;
        ca.ca_avg += ((FSCALE - CCPU) * (ca.ca_last as u64 / hz) as u32) >> FSHIFT;
        assert_eq!(stats.avg, ca.ca_avg);
    }

    /// 稳态：进程每秒满负荷时均值收敛到 FSCALE 附近（不越界、不爆）。
    #[test]
    fn steady_state_stays_within_fscale() {
        let hz = 100u64;
        let mut ca = CpuAvgSnap::default();
        let mut now = 1000u64;
        for _ in 0..200 {
            // 一秒内 hz 个滴答，全部记到这个进程。
            for _ in 0..hz {
                increment(&mut ca, now, hz);
                now += 1;
            }
        }
        let stats = getstats(&ca, now, hz);
        assert!(stats.avg <= FSCALE, "均值不越 FSCALE: {}", stats.avg);
        assert!(stats.avg > FSCALE / 2, "满负荷应收敛到高位: {}", stats.avg);
    }
}
