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

/// Which CPU a record lives on (`schedproc.h:32`, the `cpu` field).
///
/// A `u32` newtype, the same shape the kernel uses for the same idea
/// (`os/kernel/src/proc.rs` `CpuId`) — two crates, one vocabulary. The
/// constructor checks nothing on purpose: a legal `CpuId` is below the
/// topology's count, and that guarantee is `pick`'s job (the only
/// producer); the type's value is at the signatures, where a bare
/// `u32` forced every reader to re-ask "is this a CPU, a count, or a
/// queue?". [ARCH S-4] kin: bare integers that carry meaning become
/// types (the load counter's sentinel went the same way, `CpuLoad`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuId(pub u32);

/// One CPU's load: present with a count, or gone (`schedule.c:37-46`).
///
/// C keeps `unsigned cpu_proc[]` with a `-1` sentinel (`CPU_DEAD`, `37`)
/// for the gone — a bare `unsigned` that asks "how dead is dead?".
/// `None` answers in the type: the slot holds a count or it holds
/// nothing. [ARCH S-4] (plan.md).
pub type CpuLoad = Option<u32>;

/// Choose a CPU (`pick_cpu`, `schedule.c:48-78`).
///
/// Three rules in C order, first match wins:
/// 1. One CPU in the world → it (`54-57`; the non-SMP `cpu = 0` is the
///    same rule with `bsp_id == 0`).
/// 2. A system child → the BSP (`60-64`; systems stay where they were
///    born, 05's parentage test decides).
/// 3. Anyone else → the least-loaded *available* non-BSP CPU within
///    `processors_count`, ties to the lowest index (the strict `>` at `68`
///    keeps the first minimum); with no better seat, the BSP (`65-76`,
///    fallback included).
///
/// `loads` may be wider than the machine (the server keeps a `MAX_CPUS`
/// ledger); the walk is clamped to `topo.processors_count` so seats past
/// the real CPU count — nonexistent, not idle — can never win. `is_system`
/// arrives decided (05's predicate, not re-derived). The BSP index is
/// clamped into the real seat range — an out-of-range `bsp_id` cannot happen
/// from a real `sys_getmachine`, and clamping keeps the function total
/// instead of panicking on a broken caller. An empty view yields CPU 0 by
/// the same clamp.
pub fn pick(is_system: bool, topo: &MachineTopology, loads: &[CpuLoad]) -> CpuId {
    // The real seat count on this machine — C's loop bound `c <
    // machine.processors_count` (`schedule.c:67`). The caller's `loads` view
    // may be wider than the machine: the server keeps a `MAX_CPUS`-length
    // ledger regardless of topology, so seats past `processors_count` are
    // nonexistent, not merely idle. Walking the full array would treat a
    // phantom seat as an empty one, brand it as the choice, and hand the
    // kernel an out-of-range `cpu` — which `validate_cpu_param` rejects
    // `EINVAL` (`v >= ncpus`). Clamping the walk to the topology is what
    // C's bounded `for` does and what the retry ring's contract needs: when
    // every real non-BSP seat is dead, fall back to the BSP, never invent a
    // CPU beyond the machine.
    let ncpus = (topo.processors_count as usize).min(loads.len());
    // The only seat, clamped into the real view (see above).
    let bsp = topo.bsp_id.min(ncpus.saturating_sub(1) as u32);
    if topo.processors_count <= 1 {
        return CpuId(bsp);
    }
    if is_system {
        return CpuId(bsp);
    }
    let mut best = bsp;
    let mut best_load = u32::MAX;
    for (index, load) in loads.iter().enumerate().take(ncpus) {
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
    CpuId(best)
}

/// Brand a CPU dead (`schedule.c:229`, the retry ring's mark).
///
/// C writes `CPU_DEAD` (`-1`); the brand reads `None` here. The ring
/// itself (06 D6) decides *when*; this verb only performs the mark.
pub fn mark_dead(loads: &mut [CpuLoad], cpu: CpuId) {
    if let Some(slot) = loads.get_mut(cpu.0 as usize) {
        *slot = None;
    }
}

/// Book one unit of load (`schedule.c:77`, the choice's price).
///
/// Saturating: on legal paths (every booking released exactly once, 07)
/// this equals C's `++` exactly; on a double booking it holds at the
/// ceiling instead of wrapping the books into nonsense. Same answers
/// where it matters, no corruption where it doesn't.
pub fn add_load(loads: &mut [CpuLoad], cpu: CpuId) {
    if let Some(Some(count)) = loads.get_mut(cpu.0 as usize) {
        *count = count.saturating_add(1);
    }
}

/// Release one unit of load (`schedule.c:130`, the release's refund).
///
/// Saturating, mirroring [`add_load`]: legal paths match C's `--`
/// exactly; a double release holds at zero instead of wrapping to
/// `UINT_MAX` — which C would then read as "dead, never pick" (the
/// load comparison, §D2). The wrap is C's accident, not its contract.
pub fn release_load(loads: &mut [CpuLoad], cpu: CpuId) {
    if let Some(Some(count)) = loads.get_mut(cpu.0 as usize) {
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
        assert_eq!(pick(false, &one, &[Some(9)]), CpuId(0));
        assert_eq!(pick(true, &one, &[Some(9)]), CpuId(0));
        assert_eq!(pick(false, &one, &[None]), CpuId(0));
    }

    #[test]
    fn test_system_to_bsp() {
        // System children stay on the BSP (`60-64`), even with idle
        // seats elsewhere — birthplace over balance.
        let four = topo(4, 0);
        let loads = [Some(9), Some(0), Some(0), Some(0)];
        assert_eq!(pick(true, &four, &loads), CpuId(0));
        // A non-zero BSP is honored too.
        let shifted = topo(4, 2);
        assert_eq!(pick(true, &shifted, &loads), CpuId(2));
    }

    #[test]
    fn test_least_loaded() {
        // Users take the least-loaded available non-BSP seat (`65-76`).
        let four = topo(4, 0);
        assert_eq!(
            pick(false, &four, &[Some(5), Some(3), Some(1), Some(2)]),
            CpuId(2)
        );
        // Ties keep the lowest index (the strict `>` at `68`).
        assert_eq!(
            pick(false, &four, &[Some(5), Some(1), Some(1), Some(2)]),
            CpuId(1)
        );
        // The BSP never displaces, even when lightest.
        assert_eq!(
            pick(false, &four, &[Some(0), Some(5), Some(5), Some(5)]),
            CpuId(1)
        );
    }

    #[test]
    fn test_dead_skipped() {
        // Gone seats never win (`68-69` skip); with no better seat the
        // BSP holds — even dead, exactly as C falls back (`65`), leaving
        // the liveness verdict to the kernel's ring (06 D6).
        let four = topo(4, 0);
        assert_eq!(
            pick(false, &four, &[Some(5), None, Some(1), Some(2)]),
            CpuId(2)
        );
        assert_eq!(pick(false, &four, &[None, None, None, None]), CpuId(0));
    }

    #[test]
    fn test_phantom_seats_beyond_topology() {
        // The regression (§1.118): the server keeps a `MAX_CPUS`-length ledger,
        // so `loads` is wider than a real 4-CPU machine. Once the retry ring
        // brands the (present but not-ready) non-BSP seats dead, the phantom
        // seats 4..`loads.len` are still `Some(0)` — an unbounded walk picks
        // one, the kernel rejects `cpu >= ncpus` with `EINVAL`, and a fork
        // never starts. The walk must clamp to `processors_count` and fall
        // back to the BSP instead (C's bounded `for`, schedule.c:67).
        let four = topo(4, 0);
        let mut loads = [Some(0); 32];
        // Ring has tried and branded the real non-BSP seats 1..3 dead.
        for cpu in [CpuId(1), CpuId(2), CpuId(3)] {
            mark_dead(&mut loads, cpu);
        }
        // Phantom seats 4.. remain Some(0); a bounded pick ignores them.
        assert_eq!(pick(false, &four, &loads), CpuId(0));
        // Even with a lighter phantom seat (4.. are Some(0), lighter than the
        // real cpu 2), the bounded walk ignores it and takes the best *real*
        // non-BSP seat. An unbounded walk would grab cpu 4 (a phantom) and the
        // kernel would reject it.
        let mut mixed = [Some(0); 32];
        mixed[0] = Some(5); // BSP, never displaces
        mixed[1] = None; // dead
        mixed[2] = Some(3); // best real available non-BSP
        mixed[3] = Some(9); // real but heavier
        assert_eq!(pick(false, &four, &mixed), CpuId(2));
    }

    #[test]
    fn test_books_balance() {
        // Booking and release mirror C's `++`/`--` on legal paths
        // (`77` / `130`); off the path they saturate instead of
        // wrapping the books into nonsense.
        let mut loads = [Some(1), Some(0)];
        add_load(&mut loads, CpuId(1));
        assert_eq!(loads, [Some(1), Some(1)]);
        release_load(&mut loads, CpuId(0));
        assert_eq!(loads, [Some(0), Some(1)]);
        // A double release holds at zero (C would wrap to UINT_MAX —
        // which its own comparison would then read as dead).
        release_load(&mut loads, CpuId(0));
        assert_eq!(loads[0], Some(0));
        // The retry ring's brand (`229`): gone, and skipped after.
        mark_dead(&mut loads, CpuId(1));
        assert_eq!(loads[1], None);
        let four = topo(2, 0);
        assert_eq!(pick(false, &four, &loads), CpuId(0));
        // Out-of-range seats are ignored, never panicking.
        mark_dead(&mut loads, CpuId(99));
        add_load(&mut loads, CpuId(99));
        release_load(&mut loads, CpuId(99));
        assert_eq!(loads, [Some(0), None]);
    }
}
