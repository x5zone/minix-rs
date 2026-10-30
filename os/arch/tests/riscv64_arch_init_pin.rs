//! Shape-pinning test for the riscv64 architecture-init wiring (the
//! host-runnable audit variant).
//!
//! The whole `os/arch/src/riscv64/arch_init.rs` module is gated behind
//! `#[cfg(target_arch = "riscv64")]` (`os/arch/src/lib.rs`), so a host (x86_64)
//! `cargo test` does not compile it — in-module unit tests would be written and
//! never run (a reachability discipline this project has tripped over twice;
//! see the same family in `arm64_arch_init_pin.rs`). This file uses
//! `include_str!` to pin the load-bearing shape of riscv64 `Riscv64ArchInit` on
//! the host side:
//!
//! 1. Construction contract — `new` is a fieldless empty constructor (riscv
//!    has no architecture misc parameters) that returns a unit struct;
//! 2. The `init` body must stay empty — both historical writes have been
//!    removed, and their reappearance means a silent boot regression:
//!    - the PMP CSRs (`pmpcfg*`/`pmpaddr*`) are M-mode-only, and writing them
//!      from S mode traps as an illegal instruction (observed while running the
//!      first timer-interrupt carrier: scause=2, stval=`csrw pmpaddr0`); that
//!      allow-all intent is satisfied by the OpenSBI firmware Domain0
//!      delegating S/U;
//!    - `sie = STIE | SSIE` (0x22) was removed — opening the S-mode timer gate
//!      early arms interrupt sources that have no handler yet; STIE is now
//!      claimed by `TimerIrqGate::enable_timer_irq`, and SSIE belongs to the SMP
//!      IPI bring-up.
//!
//! C counterpart: none (Minix3 has no RISC-V port) — an architectural
//! evolution `[ARCH: K-2]` (05-clock-interrupt-init.md §3.8), so what this pin
//! guards is "the removed writes do not come back" and "the construction
//! contract shape", not consistency with C behavior.

/// The whole original file (comments and docs included).
fn src() -> &'static str {
    include_str!("../src/riscv64/arch_init.rs")
}

/// Strip every `//` comment (whole-line `//!`/`//` and trailing ones), then
/// collapse runs of whitespace into single spaces — what remains is the
/// "executable code skeleton", used to decide whether something is really
/// written as an instruction rather than merely mentioned in a comment.
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
    // Collapse runs of whitespace so multiline constructs like `{ Self }` and
    // `{ }` become comparable.
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
    // Main gate: the init body must be empty once comments are stripped — any
    // reappearing instruction trips this check.
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
    // The reason for the removal must stay in the comments for future editors
    // to see (the provenance of a load-bearing fact is not lost).
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
    // The current owner of STIE / SSIE must be named in the comments, or the
    // next round will take it for a gap and add it back.
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
