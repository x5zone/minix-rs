//! Shape-pinning test for the aarch64 architecture-init wiring (host-runnable
//! audit).
//!
//! `os/arch/src/arm64/arch_init.rs` is gated off wholesale by
//! `#[cfg(target_arch = "aarch64")]`, so the host (x86_64) `cargo test` does
//! not compile it — in-module unit tests would never run even if written (a
//! reachability-discipline trap this project has hit twice). This file uses
//! `include_str!` to pin three load-bearing shapes host-side: the constructor
//! wiring (PMU switch stored in an instance field), the FPEN equivalence
//! sequence, and the PMU-enable block's gating plus its three msr writes. Any
//! drift silently changes boot behavior (FP undef regression, or changed
//! user-mode cycle-counter semantics).

fn src() -> &'static str {
    include_str!("../src/arm64/arch_init.rs")
}

#[test]
fn new_wires_pmu_flag_from_descriptor() {
    assert!(
        src().contains("pmu_cycle_counter: desc.pmu_cycle_counter"),
        "new 未把描述符的 PMU 开关存入实例字段（构造接线断）"
    );
}

#[test]
fn init_keeps_fpen_equivalence_sequence() {
    let s = src();
    assert!(
        s.contains("cpacr_el1"),
        "FPEN 等价序列消失（§续-406：直核链 undef 回归）"
    );
    assert!(
        s.contains("0x0030_0000"),
        "FPEN=0b11 立值消失"
    );
}

#[test]
fn init_pmu_block_stays_gated_with_three_msrs() {
    let s = src();
    assert!(
        s.contains("if self.pmu_cycle_counter"),
        "PMU 使能块门控消失（无开关常开＝用户态可读计数器语义变）"
    );
    for msr in ["msr pmcr_el0", "msr pmcntenset_el0", "msr pmuserenr_el0"] {
        assert!(s.contains(msr), "{msr} 缺失（PMU 使能三件套不全）");
    }
}
