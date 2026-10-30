//! Pin test for the aarch64 `restore_to_user` return leg (modeled on
//! riscv64_return_leg_pin).
//!
//! Why a separate copy on the host: the `#[cfg(test)]` items living under
//! `os/arch/src/arm64/` are gated by a whole-module
//! `#[cfg(target_arch = "aarch64")]`, so `cargo test` on the host (x86_64)
//! never compiles them — "all green" is an illusion (reachability
//! discipline). This file reads the source text with `include_str!` and
//! reconciles shapes, so it runs natively on the host.
//!
//! The three things pinned here (each one corresponds to a real incident
//! shape):
//!
//! 1. **Masking before anything else**: the first instruction of this leg
//!    must be `msr daifset, #0xf`. §1.113: SP_EL1 is then re-anchored to the
//!    stack base (TPIDR_EL1); if any of the D/A/I/F asynchronous exceptions
//!    arrives at that moment, its frame lands on top of the frame/register
//!    file pointers this leg still has to read — the observed symptom is a
//!    silent hang after the first return. The riscv sibling test's header
//!    calls this out.
//! 2. **SP_EL1 rebase before the system-register triple write**:
//!    `mrs x16, tpidr_el1` + `mov sp, x16` must precede `msr elr_el1` — the
//!    rebase uses x16 (system-register scratch), and the triple write also
//!    uses x16, so reversing the order writes the stack base into ELR.
//! 3. **Interruptible user context**: before SPSR is loaded there must be a
//!    `bic x16, x16, #<mask>`, and the mask constant must equal 0x1E0
//!    (A(5)|I(6)|F(7)|D(9)) — miss one bit and user mode runs with that class
//!    of exception masked (the aarch64 version of entering user mode with
//!    IF=0).
//!
//! Tail discipline: `eret` is the last instruction; the self-referential
//! reload of x2 (`ldr x2, [x2, 1*8]`) is the last write to x2 before eret.
//! Fail-closed: an out-of-table mnemonic fails on the spot.

/// Extract the instruction lines of `core::arch::asm!( … )` up to the first
/// `in("…")` operand, de-indented and with comments stripped, returned in
/// order (mnemonic + operands joined with single spaces).
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

/// Mnemonics allowed on this leg (fail-closed: an out-of-table mnemonic
/// cannot be pinned by this test and fails on the spot).
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
    // An assertion that used to live behind the arch gate (unreachable on the
    // host), moved to the host: A(5)|I(6)|F(7)|D(9).
    const SRC: &str = include_str!("../src/arm64/trap_return.rs");
    assert!(
        SRC.contains("const SPSR_MASK_BITS: u64 = 0x1E0;"),
        "SPSR_MASK_BITS 必须恒等于 0x1E0（A|I|F|D 全集）——改值即改可中断性契约"
    );
}
