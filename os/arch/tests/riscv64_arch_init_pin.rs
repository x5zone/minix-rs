//! riscv64 架构初始化接线的形状钉住测试（P-ALL-01 方案乙：宿主可跑审计）。
//!
//! `os/arch/src/riscv64/arch_init.rs` 被 `#[cfg(target_arch = "riscv64")]`
//! 整模块门住（`os/arch/src/lib.rs`），宿主（x86_64）`cargo test` 不编译它——
//! in-module 单测写了也永不运行（本项目两次踩过的可达性纪律坑，见
//! `arm64_arch_init_pin.rs` 同族）。本文件用 `include_str!` 在宿主侧钉住
//! riscv64 `Riscv64ArchInit` 的承重形状：
//!
//! 1. 实例化契约——`new` 是无字段空构造（riscv 无架构杂项参数），返回单元结构；
//! 2. `init` 体必须保持空——两处历史写入都已移除，重新出现＝启动静默回退：
//!    - PMP CSR（`pmpcfg*`/`pmpaddr*`）是 M-mode-only，S-mode 写入触发非法指令
//!      陷入（首枚定时器中断载体实跑 scause=2、stval=`csrw pmpaddr0` 的现场），
//!      该 allow-all 意图由 OpenSBI 固件 Domain0 委托 S/U 满足；
//!    - `sie = STIE | SSIE`（0x22）由 D-59 移除——提前打开 S-mode 定时器门会把
//!      尚无处理器的中断源武装起来；STIE 现由 `TimerIrqGate::enable_timer_irq`
//!      认领、SSIE 属 SMP IPI 拉起（smp_todo S-7/S-10）。
//!
//! C 对位：无 Minix3 等价（Minix3 无 RISC-V 端口）——架构演进 `[ARCH: K-2]`
//! （05-clock-interrupt-init.md §3.8），故本钉守护的是「移除的写入不再回来」
//! 与「实例化契约形状」，不是对 C 行为的一致性。

/// 原始文件全文（含注释与文档）。
fn src() -> &'static str {
    include_str!("../src/riscv64/arch_init.rs")
}

/// 剥去所有 `//` 注释（整行 `//!`/`//` 与行内尾注释），再把连续空白折叠为
/// 单个空格——留下的是「可执行代码骨架」，用于判定某段是否真的写成了指令，
/// 而不是只在注释里被提及。
fn code() -> String {
    let mut out = String::new();
    for line in src().lines() {
        let cut = match line.find("//") {
            Some(i) => &line[..i],
            None => line,
        };
        out.push_str(cut);
        out.push(' ');
    }
    // 折叠连续空白，让 `{ Self }`、`{ }` 这类跨行的结构可比对。
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn new_is_noop_unit_constructor() {
    let c = code();
    assert!(
        c.contains("pub struct Riscv64ArchInit;"),
        "Riscv64ArchInit 不再是单元结构（实例化契约漂移）"
    );
    assert!(
        c.contains("impl ArchInit for Riscv64ArchInit"),
        "Riscv64ArchInit 不再实现 ArchInit trait"
    );
    assert!(
        c.contains("fn new(_desc: &ArchMiscDesc) -> Self { Self }"),
        "new 未把描述符丢弃并直接返回 Self（riscv 无架构杂项参数，应空构造）"
    );
}

#[test]
fn init_body_stays_empty_of_csr_writes() {
    // 主闸：init 体在剥注释后必须为空——任何重新出现的指令都从这里报警。
    let c = code();
    assert!(
        c.contains("fn init(&mut self) { }"),
        "init 体不再为空（PMP 或 sie 写入被重新塞回＝启动静默回退）"
    );
}

#[test]
fn init_stays_free_of_m_mode_pmp_writes() {
    let c = code();
    for hazard in ["csrw pmp", "pmpaddr0", "pmpcfg"] {
        assert!(
            !c.contains(hazard),
            "init 出现 M-mode-only 的 PMP 写入 {hazard:?}（S-mode 写入触发非法指令陷入，实跑 scause=2）"
        );
    }
    // 移除的理由必须留在注释里，供未来编辑者看见（承重事实的出处不丢）。
    assert!(
        src().contains("M-mode-only"),
        "PMP 写入被移除的理由（M-mode-only）说明消失"
    );
}

#[test]
fn init_stays_free_of_premature_sie_enable() {
    let c = code();
    for hazard in ["csrw sie", "0x22"] {
        assert!(
            !c.contains(hazard),
            "init 出现提前打开 S-mode 中断门的写入 {hazard:?}（D-59 移除：武装尚无处理器的中断源）"
        );
    }
    // STIE / SSIE 的现归属要在注释里留名，否则下一轮会当成缺口补回来。
    let s = src();
    assert!(
        s.contains("enable_timer_irq"),
        "STIE 的认领方（TimerIrqGate::enable_timer_irq）说明消失"
    );
    assert!(
        s.contains("S-7/S-10") || s.contains("SMP IPI"),
        "SSIE 的认领方（SMP IPI 拉起）说明消失"
    );
}
