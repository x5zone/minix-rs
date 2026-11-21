//! Shape-pinning test for the aarch64 architecture-init wiring (host-runnable
//! audit).
//!
//! `os/arch/src/arm64/arch_init.rs` is gated off wholesale by
//! `#[cfg(target_arch = "aarch64")]`, so the host (x86_64) `cargo test` does
//! not compile it — in-module unit tests would never run even if written (a
//! reachability-discipline trap this project has hit twice). This file uses
//! `include_str!` to pin the two load-bearing shapes host-side: the FPEN
//! equivalence sequence and the absence of the retired PMU-enable block.
//!
//! D04 removed the PMU branch: no producer ever set
//! `ArchMiscDesc.pmu_cycle_counter` true, so a gate that could never open was
//! dead code. Keeping the negative pin stops a future edit from silently
//! reintroducing unconditional user-readable counters.

fn src() -> &'static str {
    include_str!("../src/arm64/arch_init.rs")
}

#[test]
fn init_keeps_fpen_equivalence_sequence() {
    let s = src();
    assert!(
        s.contains("cpacr_el1"),
        "FPEN 等价序列消失（§续-406：直核链 undef 回归）"
    );
    assert!(s.contains("0x0030_0000"), "FPEN=0b11 立值消失");
}

#[test]
fn init_does_not_enable_pmu_without_a_producer() {
    let s = src();
    assert!(
        !s.contains("self.pmu_cycle_counter"),
        "PMU 门控字段已被 D04 删除：不要在 arch_init 里重新引入无生产者的开关"
    );
    for msr in ["msr pmcr_el0", "msr pmcntenset_el0", "msr pmuserenr_el0"] {
        assert!(
            !s.contains(msr),
            "{msr} 出现：PMU 使能块已随 D04 删除，不能无条件重开"
        );
    }
}
