//! aarch64 架构初始化接线的形状钉住测试（P-ALL-01 方案乙：宿主可跑审计）。
//!
//! `os/arch/src/arm64/arch_init.rs` 被 `#[cfg(target_arch = "aarch64")]`
//! 整模块门住，宿主（x86_64）`cargo test` 不编译它——in-module 单测写了
//! 也永不运行（本项目两次踩过的可达性纪律坑）。本文件用 `include_str!`
//! 在宿主侧钉住三处承重形状：构造接线（PMU 开关进实例字段）、FPEN
//! 等价值（§续-406）、PMU 使能块的门控与三 msr。任一漂移＝启动行为静默
//! 改变（FP undef 回归，或用户态周期计数器语义变）。

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
