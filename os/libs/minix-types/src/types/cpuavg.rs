//! `cpuavg` — per-process CPU utilization as a decayed average (all of
//! C `minix3/minix/lib/libsys/cpuavg.c`).
//!
//! **Why it lives in `minix-types`**: C's file header states the reason
//! plainly — this arithmetic is **shared by the kernel and the MIB service**
//! (`cpuavg.c:16-24`). The kernel accounts when "a process has accumulated
//! about one clock tick of cycles" (not in the clock interrupt,
//! `cpuavg.c:44-51`), while MIB, after taking a kernel process-table
//! snapshot, catches the average up to the current instant on demand (only
//! then do consumers like `ps(1)` see a fresh value). Both sides must run the
//! same implementation, otherwise one sample would read differently to the
//! two consumers. The wire form [`CpuAvgSnap`] (C `struct cpuavg`,
//! `kernel/type.h:80-85`) lives in [`crate::types::proc_info`]; this module
//! holds only the arithmetic.
//!
//! **Algorithm** (same lineage as NetBSD): once per second,
//! `avg = ccpu * avg + (1 - ccpu) * (run / hz)`, where `ccpu = e**(-1/20)`
//! leaves only 5% of the average after 60 seconds. Two implementation
//! choices:
//! - **Lazy update**: statistics advance only when the process is accounted
//!   on some tick, so the average at any instant may be "stale" — exactly
//!   why the MIB side calls `update` to catch up.
//! - **One second of delay**: the previous second's run is stored in
//!   `ca_last` first and folded into the average one second later, which
//!   also yields a short-term activity estimate for "the last second"
//!   (`estcpu`).
//!
//! Fixed point: `FSHIFT = 11`, `FSCALE = 2048` (C `sys/sys/param.h:435-436`);
//! the decay uses two lookup tables (`ccpu_low`/`ccpu_high`) to keep each
//! update within two multiplications (`cpuavg.c:112-117`).
//!
//! The only difference from C is **width**: C's `clock_t` is 32 bits, while
//! this port widens the instant and `hz` to `u64` following repository
//! convention (the same ruling as the time fields of `ProcInfoStruct`); the
//! arithmetic order and truncation are copied line by line.

use crate::types::proc_info::CpuAvgSnap;

/// `FSHIFT` — number of fractional bits in the fixed point
/// (C `sys/sys/param.h:435`).
pub const FSHIFT: u32 = 11;

/// `FSCALE` — `1 << FSHIFT` (C `sys/sys/param.h:436`).
pub const FSCALE: u32 = 1 << FSHIFT;

/// `CCPUTAB_SHIFT` (C `cpuavg.c:74`): each high-table entry stands for
/// 2^3 = 8 seconds.
pub const CCPUTAB_SHIFT: u32 = 3;

/// `CCPUTAB_MASK` (C `cpuavg.c:75`).
pub const CCPUTAB_MASK: u32 = (1 << CCPUTAB_SHIFT) - 1;

/// `ccpu_low` (C `cpuavg.c:80-84`): `e**(-n/20) * FSCALE`, n = 1..7.
///
/// Values are **truncated** from C's `(uint32_t)(n * FSCALE)` (not rounded) —
/// every number in this table is pinned by the
/// `cpuavg_ccpu_tables_match_c` test, recomputed the same way as C's
/// compile-time double-precision multiplication.
pub const CCPU_LOW: [u32; 7] = [1948, 1853, 1762, 1676, 1594, 1517, 1443];

/// `ccpu_high` (C `cpuavg.c:88-96`): `e**(-8n/20) * FSCALE`, n = 1..19.
///
/// The table length is the "decayed to zero" boundary: `decay`'s early-return
/// test is `secs > 19 << 3 = 152` (C `cpuavg.c:130-131`).
pub const CCPU_HIGH: [u32; 19] = [
    1372, 920, 616, 413, 277, 185, 124, 83, 55, 37, 25, 16, 11, 7, 5, 3, 2, 1, 1,
];

/// `ccpu` (C `cpuavg.c:85`): `ccpu_low[0]`, i.e. `e**(-1/20) * FSCALE`.
pub const CCPU: u32 = CCPU_LOW[0];

/// `cpuavg_decay` (C `cpuavg.c:118-145`): decay the average by `secs`
/// seconds.
///
/// Two-level lookup: first the high table at "8 seconds per entry" for
/// `secs >> 3` entries, then the low table for the remainder. When `secs`
/// exceeds the high table's reach the result is zero directly (past the table
/// end the product is already zero in FSCALE units).
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

/// `cpuavg_update` (C `cpuavg.c:153-214`): advance the statistics timeline
/// to `now`.
///
/// At most four steps (decay #1..#4 in C's comments), hence O(1):
/// 1. Less than one second → do nothing (lazy update);
/// 2. Fold the previous second's `ca_last` into the average (decay #1), move
///    `run` into `last`, and advance the timeline one second;
/// 3. If at least one second remains, the `last` just moved in is stale too,
///    so fold once more (decay #2);
/// 4. Hand the remaining whole seconds to [`decay`] for batch decay (decay
///    #3/#4), moving the timeline forward by whole seconds.
///
/// Precondition: `hz > 0` (C divides directly; this port likewise has no
/// divide-by-zero guard).
pub fn update(ca: &mut CpuAvgSnap, now: u64, hz: u64) {
    debug_assert!(hz > 0, "hz 为 0 时 C 会除零");

    // C's `delta = now - ca_base` is a signed `clock_t` difference: when
    // `now < ca_base` it goes negative and the `delta < hz` below returns
    // early. saturating_sub gives the same early return (0 < hz).
    let mut delta = now.saturating_sub(ca.ca_base);

    if delta < hz {
        return;
    }

    // decay #1: fold in the previous second's run ratio, advancing the
    // timeline by one virtual second.
    ca.ca_avg = (CCPU * ca.ca_avg) >> FSHIFT;
    ca.ca_avg += ((FSCALE - CCPU) * (ca.ca_last as u64 / hz) as u32) >> FSHIFT;

    ca.ca_last = ca.ca_run;
    ca.ca_run = 0;

    ca.ca_base += hz;
    delta -= hz;

    if delta < hz {
        return;
    }

    // decay #2: `last` is stale too (`run` is already 0 at this point).
    ca.ca_avg = (CCPU * ca.ca_avg) >> FSHIFT;
    ca.ca_avg += ((FSCALE - CCPU) * (ca.ca_last as u64 / hz) as u32) >> FSHIFT;

    ca.ca_last = 0;

    ca.ca_base += hz;
    delta -= hz;

    if delta < hz {
        return;
    }

    // Decay the remaining whole seconds in batch, and move the timeline
    // forward aligned to whole seconds.
    let secs = (delta / hz) as u32;
    ca.ca_avg = decay(ca.ca_avg, secs);
    ca.ca_base += secs as u64 * hz;
}

/// `cpuavg_increment` (C `cpuavg.c:224-239`): account one clock tick to this
/// process.
///
/// The first accounting only aligns `ca_base` to `now` (`ca_base == 0` is the
/// "uninitialized" test); afterwards it first calls [`update`] to catch up,
/// then adds one `FSCALE` (= one whole tick) to this second's run counter.
pub fn increment(ca: &mut CpuAvgSnap, now: u64, hz: u64) {
    if ca.ca_base == 0 {
        ca.ca_base = now;
    } else {
        update(ca, now, hz);
    }

    ca.ca_run += FSCALE;
}

/// The three-value result of [`getstats`] (C's three out params).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuAvgStats {
    /// The decayed average itself (FSCALE units, 100% ≈ `FSCALE`).
    pub avg: u32,
    /// Run ticks so far this second (`l_cpticks`).
    pub cpticks: u32,
    /// CPU utilization percentage of the last second (0..100, the input of
    /// `l_pctcpu`).
    pub estcpu: u32,
}

/// `cpuavg_getstats` (C `cpuavg.c:250-276`): take the three statistics
/// without **modifying** the original structure.
///
/// C's usage is "copy then compute" (`ca = *ca_orig`), so the MIB side reads a
/// fresh value while the original in the kernel table stays put. This port
/// likewise borrows read-only.
pub fn getstats(ca: &CpuAvgSnap, now: u64, hz: u64) -> CpuAvgStats {
    debug_assert!(hz > 0, "hz 为 0 时 C 会除零");

    let mut ca = *ca;
    update(&mut ca, now, hz);

    // Fold the previous second into the average — `getstats` reports "the
    // second that just ended".
    ca.ca_avg = (CCPU * ca.ca_avg) >> FSHIFT;
    ca.ca_avg += ((FSCALE - CCPU) * (ca.ca_last as u64 / hz) as u32) >> FSHIFT;

    CpuAvgStats {
        avg: ca.ca_avg,
        cpticks: ca.ca_run >> FSHIFT,
        // C's estcpu is exactly "the utilization percentage of the last
        // second" (`cpuavg.c:266-272`).
        estcpu: ((ca.ca_last as u64 / hz * 100) as u32) >> FSHIFT,
    }
}

/// `cpuavg_getccpu` (C `cpuavg.c:281-286`): the decay constant itself
/// (FSCALE units).
pub const fn getccpu() -> u32 {
    CCPU
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Constant tables pinned value by value against C (C's
    /// `F(n) = (uint32_t)(n * FSCALE)`, a compile-time double multiplication
    /// followed by **truncation**). Any drift in one entry shifts every
    /// process's `l_pctcpu` systematically yet stays invisible to unit tests —
    /// hence the pinning here.
    #[test]
    fn cpuavg_ccpu_tables_match_c() {
        assert_eq!(FSHIFT, 11);
        assert_eq!(FSCALE, 2048);
        assert_eq!(CCPUTAB_SHIFT, 3);
        assert_eq!(CCPUTAB_MASK, 7);
        // e**(-n/20)*2048 truncated, n=1..7 (C cpuavg.c:80-84).
        assert_eq!(CCPU_LOW, [1948, 1853, 1762, 1676, 1594, 1517, 1443]);
        // e**(-8n/20)*2048 truncated, n=1..19 (C cpuavg.c:88-96).
        assert_eq!(
            CCPU_HIGH,
            [1372, 920, 616, 413, 277, 185, 124, 83, 55, 37, 25, 16, 11, 7, 5, 3, 2, 1, 1]
        );
        assert_eq!(CCPU, 1948);
        assert_eq!(getccpu(), CCPU);
    }

    /// The four `decay` branches: out of range → zero, high-table segment,
    /// low-table segment, zero-seconds identity.
    #[test]
    fn decay_branches() {
        // Out of range: `secs > 19*8 = 152` → 0 directly (C cpuavg.c:130-131).
        assert_eq!(decay(FSCALE, 153), 0);
        assert_eq!(decay(FSCALE, 1000), 0);
        // Zero seconds: unchanged.
        assert_eq!(decay(FSCALE, 0), FSCALE);
        // Low table: one second of decay = ccpu_low[0].
        assert_eq!(decay(FSCALE, 1), (CCPU_LOW[0] * FSCALE) >> FSHIFT);
        // High table plus remainder: 9 seconds = one 8-second entry + one
        // low entry.
        let expect = (CCPU_LOW[0] * ((CCPU_HIGH[0] * FSCALE) >> FSHIFT)) >> FSHIFT;
        assert_eq!(decay(FSCALE, 9), expect);
        // Exactly 8 seconds: only the high table.
        assert_eq!(decay(FSCALE, 8), (CCPU_HIGH[0] * FSCALE) >> FSHIFT);
    }

    /// The first `increment` call only aligns the timeline; later calls
    /// advance by seconds and add one full FSCALE to this second's run
    /// counter.
    #[test]
    fn increment_first_call_aligns_then_accumulates() {
        let hz = 100u64;
        let mut ca = CpuAvgSnap::default();
        // ca_base == 0 → align only, no decay (C cpuavg.c:227-228).
        increment(&mut ca, 500, hz);
        assert_eq!(ca.ca_base, 500);
        assert_eq!(ca.ca_run, FSCALE);
        assert_eq!(ca.ca_avg, 0, "首次不产生均值");
        // Another call within the same second: run keeps growing, the
        // average still does not move (delta < hz).
        increment(&mut ca, 550, hz);
        assert_eq!(ca.ca_run, 2 * FSCALE);
        assert_eq!(ca.ca_avg, 0);
        // Crossing **two seconds** (delta == 2*hz): C runs decay #1 **and**
        // #2 — after the first step delta == hz does not satisfy
        // `delta < hz`, so the second step follows (existing behavior of
        // cpuavg.c:183-198, copied as is).
        increment(&mut ca, 700, hz);
        assert_eq!(ca.ca_base, 700, "两秒各推进一步");
        assert_eq!(ca.ca_last, 0, "第二步把 last 也并掉并清零");
        assert_eq!(ca.ca_run, FSCALE, "本秒重新起算一个滴答");
        // Each of the two steps folds in "2 ticks / 100 seconds":
        // 100 * 40 >> 11 = 1, both times.
        assert_eq!(ca.ca_avg, 1);
    }

    /// The four steps of `update`: one second, two seconds, multi-second
    /// batch, and the "time going backwards" (C's negative delta) early
    /// return.
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
        // Less than a second: nothing moves.
        let before = ca;
        update(&mut ca, 1050, hz);
        assert_eq!(ca, before);
        // Time going backwards (now < ca_base): C's negative delta
        // early-returns, equally unchanged.
        update(&mut ca, 900, hz);
        assert_eq!(ca, before);
        // Exactly one second: decay #1, run→last, last cleared.
        update(&mut ca, 1100, hz);
        assert_eq!(ca.ca_base, 1100);
        assert_eq!(ca.ca_last, FSCALE);
        assert_eq!(ca.ca_run, 0);
        assert_ne!(ca.ca_avg, FSCALE, "均值已衰减并并入上一秒");
        // Ten seconds: decay #1/#2 plus batch decay.
        let mut ca = CpuAvgSnap {
            ca_base: 1000,
            ca_run: 0,
            ca_last: 0,
            ca_avg: FSCALE,
            _padding: 0,
        };
        update(&mut ca, 1000 + 10 * hz, hz);
        assert_eq!(ca.ca_base, 1000 + 10 * hz, "时间轴对齐到整秒");
        // In this scenario ca_last is 0, so steps #1/#2 contribute only the
        // decay term and no folded-in term; the remaining 8 seconds go
        // through batch decay (one high-table entry). Spelled out term by
        // term instead of taking a shortcut, so that "two steps each multiply
        // by ccpu" does not get written as "one step".
        let step = |avg: u32| (CCPU * avg) >> FSHIFT;
        assert_eq!(ca.ca_avg, decay(step(step(FSCALE)), 8));
    }

    /// `getstats`: read-only, catches up to now, produces the three values by
    /// C's formulas; and does **not modify** the original structure (C's
    /// `ca = *ca_orig` copy semantics).
    #[test]
    fn getstats_is_pure_and_matches_c_formulas() {
        let hz = 100u64;
        let orig = CpuAvgSnap {
            ca_base: 1000,
            ca_run: 50 * FSCALE, // 50 ticks this second
            ca_last: 25 * FSCALE, // 25 ticks in the previous second
            ca_avg: FSCALE,
            _padding: 0,
        };
        let stats = getstats(&orig, 1100, hz);
        // Not a byte of the original structure moves.
        assert_eq!(orig.ca_base, 1000);
        assert_eq!(orig.ca_run, 50 * FSCALE);
        // cpticks = ca_run >> FSHIFT (C cpuavg.c:264) — note that update
        // moved run into last and cleared it, so this is the run *after* the
        // update.
        assert_eq!(stats.cpticks, 0);
        // estcpu uses `ca_last` **after** update — update moved this
        // second's run (50 ticks) into last, so this reflects "the second that
        // just ended", not the old last in the input (the same semantics as
        // C cpuavg.c:264-273).
        let expect_est = ((50 * FSCALE as u64 / hz * 100) as u32) >> FSHIFT;
        assert_eq!(stats.estcpu, expect_est);
        assert_eq!(expect_est, 50, "50 滴答 / 100 hz = 50%");
        assert!(stats.estcpu <= 100, "百分比不越界");
        // avg goes through C's two steps (update's #1 plus getstats'
        // fold-in).
        let mut ca = orig;
        update(&mut ca, 1100, hz);
        ca.ca_avg = (CCPU * ca.ca_avg) >> FSHIFT;
        ca.ca_avg += ((FSCALE - CCPU) * (ca.ca_last as u64 / hz) as u32) >> FSHIFT;
        assert_eq!(stats.avg, ca.ca_avg);
    }

    /// Steady state: with the process saturated every second the average
    /// converges near FSCALE (no overshoot, no blow-up).
    #[test]
    fn steady_state_stays_within_fscale() {
        let hz = 100u64;
        let mut ca = CpuAvgSnap::default();
        let mut now = 1000u64;
        for _ in 0..200 {
            // hz ticks within one second, all accounted to this process.
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
