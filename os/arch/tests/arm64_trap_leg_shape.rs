//! Shape-pinning test for the aarch64 trap leg (trap_stub.rs), modelled on
//! riscv64_trap_leg_shape.
//!
//! The host (x86_64) does not compile the arm64 module, so an in-gate `#[test]`
//! there would never run -- this file reads the source text with `include_str!`
//! and pins the load-bearing ordering of both macro legs somewhere the host can
//! run.
//!
//! The four pinned facts:
//!
//! 1. **The EL0 leg's pocket precedes the sp fetch**: `stp x9, x10, [sp, #-16]!`
//!    must be the first instruction in the macro body -- x9/x10 need somewhere
//!    to live before SP_EL0 is read; reversing the order clobbers the user's
//!    x9/x10.
//! 2. **EL0 sp before opening the frame**: `mrs x9, sp_el0` must precede
//!    `sub sp, sp, #(34*8)` -- once the frame is open, sp is no longer that sp.
//! 3. **The park leg's two-segment unwind**: the park branch (the instructions
//!    after `cbz x0, 9f`) must unwind both the `34*8` frame and the `16` pocket
//!    before jumping to `aarch64_resched_entry` -- missing either segment lets
//!    SP_EL1 ratchet downward into .bss (the EL1h single-stack model).
//! 4. **The SP_EL0 exchange-back and the pocket reload precede eret**: the
//!    restore tail must do `ldr x9, [sp, #248]` + `msr sp_el0, x9` (return the
//!    user sp to the user-side register) and finally `ldp x9, x10, [sp], #16` to
//!    close the pocket, with `eret` last.
//!
//! The vector-table grouping is pinned here as well: Group1=el1_* (kernel
//! origin), Group2=el0_* (user origin), Group0/Group3=exc_bad_mode -- a grouping
//! drift means an interrupt source takes the wrong leg.

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
    // Vector-table section: from the first VEC line to just before the DIAGBODY macro definition.
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
    // The 34x8 frame layout (gpr 0-30, sp 31, elr 32, spsr 33) is expressed by
    // the 34*8 literals throughout the leg plus slot offset 264 (=33*8); this
    // test pins that no other frame size appears in the macro bodies.
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
