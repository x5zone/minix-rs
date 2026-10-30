//! Host-shape pin test for the aarch64 scheduler SGI path.
//!
//! `os/arch/src/arm64/smp.rs` and `os/plat/src/arm64/interrupt.rs` are gated
//! behind `#[cfg(target_arch = "aarch64")]`, so the host never compiles them.
//! This file pins the load-bearing wire shape with `include_str!`:
//!
//! 1. the GICv3 send path selects `ICC_SGI1R_EL1` when the distributor has
//!    affinity routing enabled (`GICD_CTLR.ARE_NS`) and keeps the legacy
//!    `GICD_SGIR` write only as the ARE-off fallback;
//! 2. `ICC_SGI1R_EL1` is written with the architectural field positions
//!    (TargetList, Aff1/2/3, INTID) and the scheduler SGI number;
//! 3. the interrupt controller keeps the per-CPU redistributor stride and
//!    opens `GICR_ISENABLER0` for the scheduler SGI (without the stride the
//!    enable aliases every CPU onto CPU 0's redistributor).
//!
//! The real-hardware judgment is a serial log from `test-smp-aps-aarch64`
//! once the APs enter the kernel scheduler; this pin only freezes the code
//! shape.

fn smp_src() -> &'static str {
    include_str!("../src/arm64/smp.rs")
}

fn gic_src() -> &'static str {
    include_str!("../../plat/src/arm64/interrupt.rs")
}

fn arch_init_src() -> &'static str {
    include_str!("../src/arm64/arch_init.rs")
}

fn ap_entry_src() -> &'static str {
    include_str!("../src/arm64/ap_early_entry.rs")
}

fn arch_lib_src() -> &'static str {
    include_str!("../src/lib.rs")
}

#[test]
fn are_ns_selects_icc_sgi1r_else_legacy_sgir() {
    let s = smp_src();
    assert!(
        s.contains("GICD_CTLR_ARE_NS") && s.contains("ICC_SGI1R_EL1"),
        "aarch64 send path is missing the ARE_NS -> ICC_SGI1R_EL1 branch"
    );
    assert!(
        s.contains("GICD_SGIR"),
        "the ARE-off GICD_SGIR fallback disappeared"
    );
}

#[test]
fn sgi1r_fields_match_the_architecture_layout() {
    let s = smp_src();
    for needle in [
        "SGI1R_TARGET_LIST_SHIFT",
        "SGI1R_AFFINITY_1_SHIFT",
        "SGI1R_INTID_SHIFT",
        "SGI1R_AFFINITY_2_SHIFT",
        "SGI1R_AFFINITY_3_SHIFT",
        r#"mrs {}, mpidr_el1"#,
        r#"msr icc_sgi1r_el1"#,
    ] {
        assert!(s.contains(needle), "ICC_SGI1R_EL1 shape missing: {needle}");
    }
    assert!(
        s.contains("pub const SCHED_SGI_INTID"),
        "SCHED_SGI_INTID must stay public: the receive path matches it"
    );
}

#[test]
fn distributor_base_is_seeded_and_never_used_unset() {
    let smp = smp_src();
    assert!(
        smp.contains("set_gicd_base")
            && smp.contains("assert!")
            && smp.contains("before set_gicd_base"),
        "send path must fail at the wiring point when the GICD base is unset"
    );
    let init = arch_init_src();
    assert!(
        init.contains("set_gicd_base") && init.contains("Gicv3Desc"),
        "arch_init must seed the SGI send path from the Gicv3Desc"
    );
}

#[test]
fn ap_entry_hands_off_to_the_kernel_tail() {
    let s = ap_entry_src();
    assert!(
        s.contains("register_ap_tail") && s.contains("AP_TAIL") && s.contains("ApTailFn"),
        "aarch64 AP convergence lost the kernel-tail handoff gate"
    );
    assert!(
        s.contains("kernel_stack_top_va") && s.contains("AP_ARRIVED"),
        "AP convergence must snapshot the record (stack top included) before publishing arrival"
    );
}

#[test]
fn aarch64_cpu_id_map_is_the_identity_anchor() {
    let smp = smp_src();
    assert!(
        smp.contains("set_cpu_id")
            && smp.contains("cpu_id_readback")
            && smp.contains("mpidr_el1"),
        "aarch64 lost the MPIDR -> logical identity map"
    );
    assert!(
        arch_lib_src().contains("arm64::smp::cpu_id_readback"),
        "ap_cpu_id_readback no longer reads the aarch64 identity map"
    );
}

#[test]
fn redistributor_enable_uses_the_per_cpu_stride() {
    let s = gic_src();
    assert!(
        s.contains("gicr_stride") && s.contains("gicr_base_for"),
        "controller lost the per-CPU redistributor mapping"
    );
    assert!(
        s.contains("GICR_ISENABLER0") && s.contains("fn enable_sgi"),
        "controller lost the SGI enable (GICR_ISENABLER0) path"
    );
}
