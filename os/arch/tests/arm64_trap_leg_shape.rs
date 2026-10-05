//! aarch64 陷入腿（trap_stub.rs）的形状钉住测试（P-A64-02，仿 riscv64_trap_leg_shape）。
//!
//! 宿主（x86_64）不编译 arm64 模块，门内 `#[test]` 永不运行（P-ALL-01）——
//! 本文件用 `include_str!` 读源码文本，把两条宏腿的承重次序钉在宿主可跑处。
//!
//! 钉住的四件事：
//!
//! 1. **EL0 腿的口袋先于取 sp**：`stp x9, x10, [sp, #-16]!` 必须是宏体第一条
//!    指令——x9/x10 在读 SP_EL0 之前要有地方放；次序颠倒会把用户 x9/x10 踩掉。
//! 2. **EL0 sp 先于开帧**：`mrs x9, sp_el0` 必须在 `sub sp, sp, #(34*8)` 之前
//!    ——开帧后 sp 就不再是那个 sp。
//! 3. **停车腿的双段退栈**：park 分支（`cbz x0, 9f` 之外）必须同时退
//!    `34*8` 帧与 `16` 口袋再跳 `aarch64_resched_entry`——漏掉任何一段都会让
//!    SP_EL1 逐次棘轮下坠进 .bss（§1.113 EL1h 单栈模型）。
//! 4. **SP_EL0 交换回与口袋回载在 eret 前**：恢复尾必须 `ldr x9, [sp, #248]`
//!    + `msr sp_el0, x9`（用户 sp 回用户侧寄存器）、最后 `ldp x9, x10, [sp], #16`
//!    收口袋，`eret` 收尾。
//!
//! 向量表分组也在这里钉：Group1=el1_*（内核源）、Group2=el0_*（用户源）、
//! Group0/Group3=exc_bad_mode——分组漂移即中断源走错腿。

fn macro_body(src: &str, name: &str) -> Vec<String> {
    let open = format!(".macro {name}");
    let at = src
        .find(&open)
        .unwrap_or_else(|| panic!("宏 {name} 找不到（改名/删除？——本测试必须同步重审）"));
    let tail = &src[at..];
    let end = tail
        .find(".endm")
        .unwrap_or_else(|| panic!("宏 {name} 的 .endm 找不到"));
    tail[..end]
        .lines()
        .filter_map(|line| {
            let body = line.trim().strip_prefix('"')?.split('"').next()?;
            let ins = body.trim();
            if ins.is_empty() || ins.starts_with("//") || ins.starts_with(".macro") {
                None
            } else {
                Some(ins.split_whitespace().collect::<Vec<_>>().join(" "))
            }
        })
        .collect()
}

fn strip_params(ins: &[String]) -> Vec<String> {
    ins.to_vec()
}

#[test]
fn el0body_pocket_precedes_sp_el0_read() {
    let src = include_str!("../src/arm64/trap_stub.rs");
    let body = strip_params(&macro_body(src, "EL0BODY handler class"));
    assert_eq!(
        body[0], "stp x9, x10, [sp, #-16]!",
        "EL0 腿第一条必须是 x9/x10 口袋——读 SP_EL0 之前它们要有地方放"
    );
    assert_eq!(
        body[1], "mrs x9, sp_el0",
        "第二条必须是读 SP_EL0（用户 sp 先于开帧捕获）"
    );
    assert_eq!(
        body[2], "sub sp, sp, #(34*8)",
        "第三条才是开帧——次序早于取 sp 会把用户 sp 记成内核栈值"
    );
}

#[test]
fn el0body_park_branch_unwinds_both_segments() {
    let src = include_str!("../src/arm64/trap_stub.rs");
    let body = strip_params(&macro_body(src, "EL0BODY handler class"));
    let park = body
        .iter()
        .position(|i| i == "cbz x0, 9f")
        .expect("停车判读（cbz x0, 9f）找不到");
    assert_eq!(body[park + 1], "add sp, sp, #(34*8)", "停车退栈第一段＝帧");
    assert_eq!(body[park + 2], "add sp, sp, #16", "停车退栈第二段＝口袋");
    assert_eq!(
        body[park + 3], "b aarch64_resched_entry",
        "退栈后必须直跳重调度入口（EL1h 单栈模型：漏一段 = SP_EL1 棘轮进 .bss）"
    );
}

#[test]
fn el0body_restore_tail_exchanges_sp_el0_before_eret() {
    let src = include_str!("../src/arm64/trap_stub.rs");
    let body = strip_params(&macro_body(src, "EL0BODY handler class"));
    let sp0_ldr = body
        .iter()
        .position(|i| i == "ldr x9, [sp, #248]")
        .expect("槽 31（EL0 sp）回载找不到");
    assert_eq!(
        body[sp0_ldr + 1], "msr sp_el0, x9",
        "槽 31 必须写回 SP_EL0（交换回）"
    );
    let pocket = body
        .iter()
        .position(|i| i == "ldp x9, x10, [sp], #16")
        .expect("口袋回载找不到");
    assert_eq!(
        body.last().map(String::as_str),
        Some("eret"),
        "eret 必须收尾"
    );
    assert_eq!(pocket, body.len() - 2, "口袋回载必须是 eret 前最后一条");
}

#[test]
fn el1body_frames_below_interrupted_kernel_sp() {
    let src = include_str!("../src/arm64/trap_stub.rs");
    let body = strip_params(&macro_body(src, "EL1BODY handler class"));
    assert_eq!(
        body[0], "sub sp, sp, #(34*8)",
        "EL1 腿第一帧开在被中断的内核 sp 之下（无栈切换）"
    );
    let rec = body
        .iter()
        .position(|i| i == "add x9, x9, #(34*8)")
        .expect("被中断内核 sp 恢复（frame_base + frame_size）找不到");
    assert!(
        rec < body
            .iter()
            .position(|i| i.starts_with("bl "))
            .expect("bl handler 找不到"),
        "被中断 sp 的恢复必须在进入 C 处理器之前（handler 之后 sp 归属已变）"
    );
    assert_eq!(
        body[body.len() - 2], "add sp, sp, #(34*8)",
        "EL1 恢复尾＝退帧"
    );
    assert_eq!(body.last().map(String::as_str), Some("eret"));
}

#[test]
fn vector_table_groups_are_frozen() {
    let src = include_str!("../src/arm64/trap_stub.rs");
    // 向量表段：从第一行 VEC 到 DIAGBODY 宏定义之前。
    let begin = src.find("\"VEC ").expect("向量表找不到");
    let end = src.find(".macro EL1BODY").expect("EL1BODY 宏找不到（向量表段的右界）");
    let vecs: Vec<String> = src[begin..end]
        .lines()
        .filter_map(|l| {
            let b = l.trim().strip_prefix('"')?.split('"').next()?;
            (!b.is_empty()).then(|| b.to_string())
        })
        .collect();
    assert_eq!(vecs.len(), 16, "AArch64 异常向量表恒 16 槽");
    let expect: [&str; 16] = [
        "VEC exc_bad_mode",
        "VEC exc_bad_mode",
        "VEC exc_bad_mode",
        "VEC exc_bad_mode",
        "VEC el1_sync",
        "VEC el1_irq",
        "VEC el1_fiq",
        "VEC el1_serror",
        "VEC el0_sync",
        "VEC el0_irq",
        "VEC el0_fiq",
        "VEC el0_serror",
        "VEC exc_bad_mode",
        "VEC exc_bad_mode",
        "VEC exc_bad_mode",
        "VEC exc_bad_mode",
    ];
    for (got, want) in vecs.iter().zip(expect.iter()) {
        assert_eq!(got, want, "向量表分组漂移（Slot 顺序或处理腿变了——重审腿代码后同步本表）");
    }
}

#[test]
fn frame_size_is_34_slots_in_both_legs() {
    // 帧布局 34×8（gpr 0–30、sp 31、elr 32、spsr 33）由腿内全部 34*8 字面量
    // 与槽偏移 264（=33*8）共同表达；本测试钉住「宏体内不再出现其它帧大写」。
    let src = include_str!("../src/arm64/trap_stub.rs");
    for name in ["EL0BODY handler class", "EL1BODY handler class"] {
        let body = macro_body(src, name).join("\n");
        assert!(
            body.contains("#(34*8)"),
            "{name} 必须按 34 槽开帧（build↔run 契约，见模块头注）"
        );
        assert!(
            !body.contains("#(33*8)") && !body.contains("#(35*8)"),
            "{name} 出现了非 34 槽的帧尺寸字面量——要么是 bug，要么帧契约变了需同步"
        );
    }
}
