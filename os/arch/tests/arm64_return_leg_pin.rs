//! aarch64 `restore_to_user` 返回腿的钉住测试（P-A64-02，仿 riscv64_return_leg_pin）。
//!
//! 为什么要在宿主单开一份：住在 `os/arch/src/arm64/` 里的 `#[cfg(test)]` 被
//! `#[cfg(target_arch = "aarch64")]` 整模块门住，宿主（x86_64）的 `cargo test`
//! 根本不编译它们——「全绿」是假象（P-ALL-01 可达性纪律）。本文件用
//! `include_str!` 读源码文本做形状对账，宿主原生可跑。
//!
//! 钉住的三件事（每一条都对应一次真实事故形状）：
//!
//! 1. **屏蔽先于一切**：本腿第一条指令必须是 `msr daifset, #0xf`。§1.113：
//!    SP_EL1 随后被重锚到栈基（TPIDR_EL1），此刻若 D/A/I/F 任一异步异常打进
//!    来，它的帧会压在本腿还要读的 frame/寄存器文件指针地盘上——观测症状是
//!    「首次返回后静默挂死」。riscv 同族测试头注点名过这条。
//! 2. **SP_EL1 重锚先于系统寄存器三联写**：`mrs x16, tpidr_el1` + `mov sp, x16`
//!    必须在 `msr elr_el1` 之前——重锚用的是 x16（系统寄存器暂存），三联写也
//!    用 x16，次序颠倒会把栈基址写进 ELR。
//! 3. **可中断用户上下文**：SPSR 装载前必须有 `bic x16, x16, #<mask>`，且
//!    掩码常量 = 0x1E0（A(5)|I(6)|F(7)|D(9)）——漏哪一位，用户态就带着那类
//!    异常被屏蔽运行（aarch64 版的 IF=0 进用户态）。
//!
//! 尾部纪律：`eret` 是最后一条指令；x2 的自引用回载（`ldr x2, [x2, 1*8]`）
//! 是 eret 前对 x2 的最后一次写。fail-closed：出现表外助记符当场失败。

/// 抽出 `core::arch::asm!( … )` 到第一个 `in("…")` 操作数之间的指令行，
/// 去缩进、去注释，按顺序返回（助记符 + 操作数压成单空格分隔）。
fn asm_instructions(src: &str, anchor: &str) -> Vec<String> {
    let at = src.find(anchor).expect("asm 锚点找不到（函数改名/格式漂移？）");
    let tail = &src[at..];
    let end = tail
        .find("in(\"")
        .expect("asm 块的输入操作数找不到（格式漂移？）");
    tail[..end]
        .lines()
        .filter_map(|line| {
            let body = line.trim().strip_prefix('"')?.split('"').next()?;
            let ins = body.trim();
            if ins.is_empty() || ins.starts_with("//") {
                None
            } else {
                Some(ins.split_whitespace().collect::<Vec<_>>().join(" "))
            }
        })
        .collect()
}

/// 本腿允许的助记符（fail-closed：表外助记符 = 钉不住，当场失败）。
const KNOWN_MNEMONICS: &[&str] = &["msr", "mrs", "mov", "ldr", "add", "bic", "eret"];

fn return_leg_instructions() -> Vec<String> {
    const SRC: &str = include_str!("../src/arm64/trap_return.rs");
    let ins = asm_instructions(SRC, "core::arch::asm!(");
    if let Some(bad) = ins.iter().find(|i| {
        i.split_whitespace()
            .next()
            .map(|mn| !KNOWN_MNEMONICS.contains(&mn))
            .unwrap_or(false)
    }) {
        panic!("返回腿出现表外助记符，钉住测试无法担保其效果：{bad}");
    }
    ins
}

#[test]
fn return_leg_first_instruction_masks_daif() {
    let ins = return_leg_instructions();
    assert_eq!(
        ins[0], "msr daifset, #0xf",
        "返回腿第一条必须是全 DAIF 屏蔽（§1.113：重锚 SP_EL1 前打进来的异步异常会压坏待 eret 的状态）"
    );
}

#[test]
fn return_leg_sp_el1_rebase_precedes_sysreg_triple() {
    let ins = return_leg_instructions();
    let rebase = ins
        .iter()
        .position(|i| i == "mrs x16, tpidr_el1")
        .expect("SP_EL1 重锚读 TPIDR_EL1 的指令找不到");
    let first_sysreg = ins
        .iter()
        .position(|i| i.starts_with("msr elr_el1"))
        .expect("ELR_EL1 写入找不到");
    assert!(
        rebase < first_sysreg,
        "SP_EL1 重锚（行 {rebase}）必须先于系统寄存器三联写（首条在行 {first_sysreg}）——x16 既是栈基暂存又是三联写载体，次序颠倒会把栈基写进 ELR"
    );
    assert_eq!(
        ins[rebase + 1], "mov sp, x16",
        "TPIDR_EL1 读出后必须立刻 mov sp, x16（重锚两拍一体）"
    );
}

#[test]
fn return_leg_spsr_bic_sits_between_load_and_store() {
    let ins = return_leg_instructions();
    let ldr = ins
        .iter()
        .position(|i| i.contains("[x0, {spsr_off}]"))
        .expect("SPSR 装载找不到");
    let bic = ins
        .iter()
        .position(|i| i.starts_with("bic x16, x16, #"))
        .expect("SPSR 可中断位清除（bic）找不到");
    let msr = ins
        .iter()
        .position(|i| i.starts_with("msr spsr_el1"))
        .expect("SPSR 写入找不到");
    assert!(
        ldr < bic && bic < msr,
        "bic 必须夹在 SPSR 装载（行 {ldr}）与写入（行 {msr}）之间，实测 bic 在行 {bic}"
    );
    assert!(
        ins[bic].starts_with("bic x16, x16, #"),
        "清除位必须走 x16 占位对账 SPSR_MASK_BITS（0x1E0 = A|I|F|D），实测 {}",
        ins[bic]
    );
}

#[test]
fn return_leg_eret_is_last_and_x2_self_load_is_last_write() {
    let ins = return_leg_instructions();
    assert_eq!(
        ins.last().map(String::as_str),
        Some("eret"),
        "eret 必须是返回腿最后一条指令"
    );
    let x2_self = ins
        .iter()
        .rposition(|i| i == "ldr x2, [x2, 1*8]")
        .expect("x2 自引用回载找不到");
    let eret = ins.len() - 1;
    assert_eq!(
        eret - x2_self,
        1,
        "x2 自引用回载与 eret 之间不得再有任何指令（指针的最后一次使用）"
    );
}

#[test]
fn spsr_mask_bits_constant_covers_aifd() {
    // 原本住在架构门内（宿主跑不到）的断言，搬到宿主：A(5)|I(6)|F(7)|D(9)。
    const SRC: &str = include_str!("../src/arm64/trap_return.rs");
    assert!(
        SRC.contains("const SPSR_MASK_BITS: u64 = 0x1E0;"),
        "SPSR_MASK_BITS 必须恒等于 0x1E0（A|I|F|D 全集）——改值即改可中断性契约"
    );
}
