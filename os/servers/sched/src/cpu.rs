//! SCHED CPU choice: who goes where, and who keeps the books.
//!
//! Mirrors `pick_cpu()` + the load table (`minix3/minix/servers/sched/
//! schedule.c:37-78`) and the machine shape (`minix3/minix/include/minix/
//! type.h:122-125`).
//! 10-pick-cpu-smp.md.
//!
//! The module owns the role split and nothing else: the choice rule and
//! the bookkeeping verbs. The books themselves (per-CPU load counts)
//! stay caller-side — the caller holds the table (04 D2's precedent:
//! doors judge, holders act), this module only reads a view and names
//! the mutations.
//!
//! Single-threaded event loop: pure functions, no shared state.

/// The machine shape behind every choice (`type.h:122-125`).
///
/// Two numbers the kernel reports once at startup (`sys_getmachine`,
/// 01): how many CPUs exist and which one bootstrapped. Every choice
/// below reads them — the topology is a parameter, never a global, so
/// single-CPU and multi-CPU fall out of one rule (S-5: the `CONFIG_SMP`
/// compile gate becomes a runtime count).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MachineTopology {
    /// How many CPUs exist. C: `machine.processors_count` — type.h:123.
    pub processors_count: u32,
    /// Which CPU bootstrapped. C: `machine.bsp_id` — type.h:124.
    pub bsp_id: u32,
}

/// One CPU's load: present with a count, or gone (`schedule.c:37-46`).
///
/// C keeps `unsigned cpu_proc[]` with a `-1` sentinel (`CPU_DEAD`, `37`)
/// for the gone — a bare `unsigned` that asks "how dead is dead?".
/// `None` answers in the type: the slot holds a count or it holds
/// nothing. [ARCH S-4] (plan.md).
pub type CpuLoad = Option<u32>;

/// Whether a CPU may take work (`schedule.c:39`).
///
/// C's macro reads `(cpu_proc[c] >= 0)` — over `unsigned`, unconditionally
/// true: the `continue` it guards never fires, and the dead are really
/// filtered by the load comparison below (a `UINT_MAX` load never wins
/// `cpu_load > cpu_proc[c]`). The Rust form says what C means: present
/// counts work, gone slots don't. Same choices on every input, without
/// the tautology.
pub const fn is_available(load: CpuLoad) -> bool {
    load.is_some()
}

/// Choose a CPU (`pick_cpu`, `schedule.c:48-78`).
///
/// Three rules in C order, first match wins:
/// 1. One CPU in the world → it (`54-57`; the non-SMP `cpu = 0` is the
///    same rule with `bsp_id == 0`).
/// 2. A system child → the BSP (`60-64`; systems stay where they were
///    born, 05's parentage test decides).
/// 3. Anyone else → the least-loaded *available* non-BSP CPU, ties to
///    the lowest index (the strict `>` at `68` keeps the first minimum);
///    with no better seat, the BSP (`65-76`, fallback included).
///
/// `loads` is a view over the caller's table (`len` entries for `len`
/// CPUs); `is_system` arrives decided (05's predicate, not re-derived).
/// The BSP index is clamped into the view — an out-of-range `bsp_id`
/// cannot happen from a real `sys_getmachine`, and clamping keeps the
/// function total instead of panicking on a broken caller. An empty view
/// yields CPU 0 by the same clamp (documented, unreachable in practice:
/// callers always pass the real table).
pub fn pick(is_system: bool, topo: &MachineTopology, loads: &[CpuLoad]) -> u32 {
    // The only seat, clamped into the view (see above).
    let bsp = topo.bsp_id.min(loads.len().saturating_sub(1) as u32);
    if topo.processors_count <= 1 {
        return bsp;
    }
    if is_system {
        return bsp;
    }
    let mut best = bsp;
    let mut best_load = u32::MAX;
    for (index, load) in loads.iter().enumerate() {
        let cpu = index as u32;
        // Dead seats never win (`68-69`); the BSP never displaces
        // (`c != bsp_id` at `68`); strictly-lighter displaces, ties
        // keep the earlier (lower) index.
        if let Some(count) = load
            && cpu != bsp
            && *count < best_load
        {
            best_load = *count;
            best = cpu;
        }
    }
    best
}

/// Brand a CPU dead (`schedule.c:229`, the retry ring's mark).
///
/// C writes `CPU_DEAD` (`-1`); the brand reads `None` here. The ring
/// itself (06 D6) decides *when*; this verb only performs the mark.
pub fn mark_dead(loads: &mut [CpuLoad], cpu: usize) {
    if let Some(slot) = loads.get_mut(cpu) {
        *slot = None;
    }
}

/// Book one unit of load (`schedule.c:77`, the choice's price).
///
/// Saturating: on legal paths (every booking released exactly once, 07)
/// this equals C's `++` exactly; on a double booking it holds at the
/// ceiling instead of wrapping the books into nonsense. Same answers
/// where it matters, no corruption where it doesn't.
pub fn add_load(loads: &mut [CpuLoad], cpu: usize) {
    if let Some(Some(count)) = loads.get_mut(cpu) {
        *count = count.saturating_add(1);
    }
}

/// Release one unit of load (`schedule.c:130`, the release's refund).
///
/// Saturating, mirroring [`add_load`]: legal paths match C's `--`
/// exactly; a double release holds at zero instead of wrapping to
/// `UINT_MAX` — which C would then read as "dead, never pick" (the
/// load comparison, §D2). The wrap is C's accident, not its contract.
pub fn release_load(loads: &mut [CpuLoad], cpu: usize) {
    if let Some(Some(count)) = loads.get_mut(cpu) {
        *count = count.saturating_sub(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn topo(count: u32, bsp: u32) -> MachineTopology {
        MachineTopology {
            processors_count: count,
            bsp_id: bsp,
        }
    }

    #[test]
    fn test_single_cpu() {
        // One CPU in the world: it, regardless of system-ness or books
        // (`54-57`; the non-SMP `cpu = 0` is this rule with bsp 0).
        let one = topo(1, 0);
        assert_eq!(pick(false, &one, &[Some(9)]), 0);
        assert_eq!(pick(true, &one, &[Some(9)]), 0);
        assert_eq!(pick(false, &one, &[None]), 0);
    }

    #[test]
    fn test_system_to_bsp() {
        // System children stay on the BSP (`60-64`), even with idle
        // seats elsewhere — birthplace over balance.
        let four = topo(4, 0);
        let loads = [Some(9), Some(0), Some(0), Some(0)];
        assert_eq!(pick(true, &four, &loads), 0);
        // A non-zero BSP is honored too.
        let shifted = topo(4, 2);
        assert_eq!(pick(true, &shifted, &loads), 2);
    }

    #[test]
    fn test_least_loaded() {
        // Users take the least-loaded available non-BSP seat (`65-76`).
        let four = topo(4, 0);
        assert_eq!(pick(false, &four, &[Some(5), Some(3), Some(1), Some(2)]), 2);
        // Ties keep the lowest index (the strict `>` at `68`).
        assert_eq!(pick(false, &four, &[Some(5), Some(1), Some(1), Some(2)]), 1);
        // The BSP never displaces, even when lightest.
        assert_eq!(pick(false, &four, &[Some(0), Some(5), Some(5), Some(5)]), 1);
    }

    #[test]
    fn test_dead_skipped() {
        // Gone seats never win (`68-69` skip); with no better seat the
        // BSP holds — even dead, exactly as C falls back (`65`), leaving
        // the liveness verdict to the kernel's ring (06 D6).
        let four = topo(4, 0);
        assert_eq!(pick(false, &four, &[Some(5), None, Some(1), Some(2)]), 2);
        assert_eq!(pick(false, &four, &[None, None, None, None]), 0);
        assert!(is_available(Some(3)));
        assert!(!is_available(None));
    }

    #[test]
    fn test_books_balance() {
        // Booking and release mirror C's `++`/`--` on legal paths
        // (`77` / `130`); off the path they saturate instead of
        // wrapping the books into nonsense.
        let mut loads = [Some(1), Some(0)];
        add_load(&mut loads, 1);
        assert_eq!(loads, [Some(1), Some(1)]);
        release_load(&mut loads, 0);
        assert_eq!(loads, [Some(0), Some(1)]);
        // A double release holds at zero (C would wrap to UINT_MAX —
        // which its own comparison would then read as dead).
        release_load(&mut loads, 0);
        assert_eq!(loads[0], Some(0));
        // The retry ring's brand (`229`): gone, and skipped after.
        mark_dead(&mut loads, 1);
        assert_eq!(loads[1], None);
        let four = topo(2, 0);
        assert_eq!(pick(false, &four, &loads), 0);
        // Out-of-range seats are ignored, never panicking.
        mark_dead(&mut loads, 99);
        add_load(&mut loads, 99);
        release_load(&mut loads, 99);
        assert_eq!(loads, [Some(0), None]);
    }
}
