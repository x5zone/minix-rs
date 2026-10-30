//! Instruction-shape contract for the riscv64 trap legs.
//!
//! Why this is a **host integration test** instead of a `#[test]` in
//! `trap_stub.rs`: `minix-arch`'s `pub mod riscv64` is gated by
//! `#[cfg(target_arch = "riscv64")]`, so a host `cargo test` never compiles that
//! module — tests written there would never run (= false green). This file uses
//! `include_str!` to read the same source text and reconcile it statically: no
//! linking, no boot, only the asm's **instruction order** is checked from a host
//! test.
//!
//! The six shape rules checked here (all derived directly from defects this code
//! has already hit):
//! 0. **Fail-closed mnemonic list**: only known mnemonics are allowed in a leg —
//!    an instruction outside the list cannot be proven not to write its first
//!    operand, so it counts as a violation (otherwise a newly added instruction
//!    would silently bypass the rules below).
//! 1. **Frame coverage**: each slot 1..=33 is stored exactly once by `sd` — a
//!    missing slot means the return path leaves a register that was never saved
//!    to the interrupted context; a duplicated slot means some interrupted
//!    register has no home.
//! 2. **Save-side scratch discipline**: a register the frame must carry has to
//!    land in **its own slot** first, and only afterwards may the leg use it as
//!    scratch. Reversing the order asynchronously replaces a register of the
//!    interrupted context with a leg-local temporary — no exception, no log (the
//!    kernel leg had exactly this shape: `t0` was first repurposed by
//!    `addi t0, sp, 34*8` to compute the interrupted `sp`, and only later stored
//!    with `sd t0, 5*8(sp)`, so every supervisor timer interrupt replaced the
//!    kernel's `t0` with a stack pointer address).
//! 3. **Restore-side lifetime**: once reloaded from its own slot, a register must
//!    not be used as scratch again up to `sret`. The review of this rule
//!    uncovered a second defect of the same shape: the user leg used t0 to write
//!    `stvec` after `ld t0, 5*8(sp)`, so on return to user mode t0 held its own
//!    stack pointer — and here the victim was the user process itself.
//! 4. **Reload coverage**: every register stored into the frame must be restored
//!    from its own slot.
//! 5. **Existence and order of the critical CSR writes**: both legs must write
//!    the frame's `sstatus` first (hardware has already cleared SIE on trap entry,
//!    so that write is the epilogue's masking latch) and install `stvec` after
//!    it, before any GPR reload. Moving the vector write ahead of the latch
//!    reopens the window where S-mode re-entry with SIE=1 is only safe for a
//!    U-origin leg, and a shape check that does not pin the order cannot stop
//!    such a "tidy-up".
//!
//! Positive-control tests (`checker_fires_on_the_regression_it_exists_for` and
//! `negative_controls_make_each_rule_fire`) pair each rule with a known-bad
//! sample, guaranteeing the parser is not merely "green because nothing
//! matched".

/// Frame slot ↔ architectural register name. `gpr[i]` is `x_i`; `sp`(x2) is the
/// only slot never written by `sd <its own name>` (both legs route it through a
/// scratch register into slot 2, by design).
const ABI_SLOT: &[(&str, usize)] = &[
    ("ra", 1),
    ("gp", 3),
    ("tp", 4),
    ("t0", 5),
    ("t1", 6),
    ("t2", 7),
    ("s0", 8),
    ("s1", 9),
    ("a0", 10),
    ("a1", 11),
    ("a2", 12),
    ("a3", 13),
    ("a4", 14),
    ("a5", 15),
    ("a6", 16),
    ("a7", 17),
    ("s2", 18),
    ("s3", 19),
    ("s4", 20),
    ("s5", 21),
    ("s6", 22),
    ("s7", 23),
    ("s8", 24),
    ("s9", 25),
    ("s10", 26),
    ("s11", 27),
    ("t3", 28),
    ("t4", 29),
    ("t5", 30),
    ("t6", 31),
];

/// Whitelist of mnemonics allowed in these legs. **Fail-closed**: a mnemonic
/// outside the list cannot be proven not to write its first operand, so it is
/// counted as a violation immediately — whoever adds such an instruction has to
/// put it in the list and classify it, rather than letting the new instruction
/// silently slip past the shape check (the original incomplete list went quietly
/// blind).
const KNOWN_MNEMONICS: &[&str] = &[
    "addi", "sd", "ld", "csrr", "csrw", "csrrw", "mv", "la", "call", "beqz", "jr", "srli", "andi",
    "sret", "j", "b", "jal", "jalr", "ret", "nop",
];

/// Mnemonics that write their first operand.
const DEST_FIRST: &[&str] = &[
    "addi", "li", "mv", "la", "ld", "lw", "lh", "lb", "csrr", "csrrw", "add", "sub", "and", "andi",
    "or", "ori", "srli", "slli", "srai", "neg", "not", "seqz", "sltz", "mul", "jal", "jalr",
];

fn mnemonic(ins: &str) -> Option<&str> {
    ins.split_whitespace().next()
}

fn written_reg(ins: &str) -> Option<&str> {
    let (mn, rest) = ins.split_once(char::is_whitespace)?;
    if !DEST_FIRST.contains(&mn) {
        return None;
    }
    rest.split(',').next().map(|r| r.trim())
}

/// `sd <reg>, <slot>*8(sp)` → (reg, slot).
fn stored_slot(ins: &str) -> Option<(&str, usize)> {
    let (mn, rest) = ins.split_once(char::is_whitespace)?;
    if mn != "sd" {
        return None;
    }
    let mut it = rest.split(',');
    let reg = it.next()?.trim();
    let addr = it.next()?.trim();
    if !addr.contains("(sp)") {
        return None;
    }
    let n: usize = addr.split('*').next()?.trim().parse().ok()?;
    Some((reg, n))
}

/// Extract one leg's instruction sequence: from the entry label up to its
/// `sret`. The instruction text lives inside Rust string literals passed to
/// `global_asm!`, so only lines that start with a quote are taken, cut at the
/// next quote; pseudo-ops (`.align`/`.globl`/`.size`) and Rust comments between
/// literals are filtered out naturally.
///
/// The anchor is deliberately the **quoted label line**
/// (`"riscv64_kernel_trap_vector:"`) rather than a bare label name: otherwise
/// someone writing the same colon-suffixed name in a doc comment would make the
/// user-leg slice start earlier in the file and end at the kernel leg's `sret`,
/// leaving all five rules green while the real user leg is never checked (an
/// anchor must be discriminating by construction, not by coincidence).
fn leg_instructions(src: &str, entry: &str) -> Vec<String> {
    let anchor = format!("\"{entry}\"");
    let at = src
        .find(&anchor)
        .unwrap_or_else(|| panic!("腿入口 {anchor} 在 trap_stub 源里找不到（标号行格式漂移？）"));
    let rest = &src[at..];
    let end = rest
        .find("sret\"")
        .unwrap_or_else(|| panic!("腿 {entry} 没有 sret 收尾"));
    rest[..end]
        .lines()
        .filter_map(|line| {
            let body = line.trim().strip_prefix('"')?.split('"').next()?;
            let ins = body.trim();
            if ins.is_empty()
                || ins.starts_with('.')
                || ins.starts_with('#')
                // numeric labels (`8:`/`9:`/`1:`) are not instructions and take
                // no part in the check
                || (ins.ends_with(':') && !ins.contains(' '))
            {
                None
            } else {
                Some(ins.split_whitespace().collect::<Vec<_>>().join(" "))
            }
        })
        .collect()
}

/// Shape-rule checking; returns the violation list (empty = clean). It is a
/// separate function so the "known-bad sample" controls can drive it directly.
/// All six rules read only the instruction sequence and never touch hardware.
fn leg_shape_violations(leg: &[String]) -> Vec<String> {
    let mut bad = Vec::new();
    // Rule 0 (fail-closed list): any unknown mnemonic is treated as suspect.
    for ins in leg {
        if let Some(mn) = mnemonic(ins)
            && !KNOWN_MNEMONICS.contains(&mn)
        {
            bad.push(format!("未知助记符 {mn}（不能证明它不写首操作数）：{ins}"));
        }
    }
    // Rule 1: frame coverage
    let mut counts = [0u8; 34];
    for ins in leg {
        if let Some((_, slot)) = stored_slot(ins) {
            if slot < 34 {
                counts[slot] += 1;
            }
        }
    }
    for slot in 1..=33 {
        if counts[slot] != 1 {
            bad.push(format!("槽 {slot} 被存了 {} 次", counts[slot]));
        }
    }
    for &(name, slot) in ABI_SLOT {
        let own_store = format!("sd {name}, {slot}*8(sp)");
        let own_load = format!("ld {name}, {slot}*8(sp)");
        // Rule 2 (save-side scratch discipline): a register the frame must carry
        // lands in its own slot first, and only then may the leg use it as
        // scratch.
        let save_at = leg.iter().position(|i| i.as_str() == own_store);
        let use_at = leg
            .iter()
            .position(|i| i.as_str() != own_store && written_reg(i) == Some(name));
        match (save_at, use_at) {
            (_, None) => {}
            (None, Some(_)) => bad.push(format!("{name} 从未保存就被挪用")),
            (Some(s), Some(u)) if u < s => {
                bad.push(format!("{name} 在第 {u} 条被挪用，早于第 {s} 条的保存"))
            }
            (Some(_), Some(_)) => {}
        }
        // Rule 3 (restore-side lifetime): after reloading from its own slot, a
        // register must not be used as scratch again up to `sret` — its value
        // is the interrupted context's (the user leg wrote stvec through t0
        // after `ld t0, 5*8(sp)`, returning to user mode with t0 = user sp).
        let reload_at = leg.iter().position(|i| i.as_str() == own_load);
        if let Some(r) = reload_at {
            let after = leg[r + 1..]
                .iter()
                .position(|i| written_reg(i) == Some(name));
            if let Some(w) = after {
                bad.push(format!(
                    "{name} 在自身回载（第 {r} 条）之后第 {} 条又被当暂存用",
                    r + 1 + w
                ));
            }
        } else {
            // Rule 4 (reload coverage): a saved register must be put back.
            bad.push(format!("{name} 没有从自己槽（{slot}）回载就 sret"));
        }
    }
    // Rule 5 (existence and order of the critical CSR writes): both legs make
    // the epilogue a masked window by writing the frame's sstatus first (hardware
    // cleared SIE on trap entry) and installing stvec afterwards. The user leg
    // writes stvec twice (handing back to the kernel leg before entering the
    // handler, and switching back to the user leg before leaving), the kernel leg
    // only once, so what is pinned here is the position of the **last** write: it
    // must come after the latch and before any GPR reload (otherwise the scratch
    // register it uses already belongs to the interrupted context, and the window
    // where S-mode re-entry with SIE=1 is only safe for a U-origin leg is
    // reopened).
    let stvec_writes: Vec<usize> = leg
        .iter()
        .enumerate()
        .filter(|(_, i)| i.starts_with("csrw stvec"))
        .map(|(k, _)| k)
        .collect();
    if stvec_writes.is_empty() {
        bad.push("腿从未写 stvec：返回后向量与被返回特权级不再一致".to_string());
    }
    for (needle, why) in [
        ("csrw sstatus, t0", "屏蔽闩（帧里的 SIE 已清）"),
        ("csrw sepc, t0", "重执行位置"),
    ] {
        if !leg.iter().any(|i| i.as_str() == needle) {
            bad.push(format!("缺少关键写 {needle}（{why}）"));
        }
    }
    let sstatus_at = leg.iter().position(|i| i.as_str() == "csrw sstatus, t0");
    let first_reload = ABI_SLOT
        .iter()
        .filter_map(|&(name, slot)| {
            let own_load = format!("ld {name}, {slot}*8(sp)");
            leg.iter().position(|i| i.as_str() == own_load)
        })
        .min();
    if let Some(&v) = stvec_writes.last() {
        if let Some(s) = sstatus_at
            && v < s
        {
            bad.push(format!(
                "最后一次 stvec 写（第 {v} 条）排在 sstatus 写（第 {s} 条）之前：屏蔽闩未先落，S 态可重入异权腿"
            ));
        }
        if let Some(r) = first_reload
            && v > r
        {
            bad.push(format!(
                "最后一次 stvec 写（第 {v} 条）落在首个 GPR 回载（第 {r} 条）之后：它用的暂存寄存器已是被打断上下文的值"
            ));
        }
    }
    // User-leg only: the vector must be handed back to the kernel leg before
    // entering the handler (a fault inside the handler must not go through the
    // sscratch swap again, or it stacks a second frame on the same kernel stack
    // top).
    if leg.iter().any(|i| i.starts_with("csrrw sp, sscratch")) {
        let back = leg
            .iter()
            .position(|i| i.as_str() == "la t0, riscv64_kernel_trap_vector");
        let enter = leg
            .iter()
            .position(|i| i.as_str() == "call riscv64_user_trap_dispatch");
        match (back, enter) {
            (Some(b), Some(e)) if b < e => {}
            _ => bad.push("用户腿没在进处理体之前把 stvec 交还内核腿".to_string()),
        }
    }
    bad
}

const SRC: &str = include_str!("../src/riscv64/trap_stub.rs");

#[test]
fn legs_save_scratch_before_using_it() {
    for entry in ["riscv64_kernel_trap_vector:", "riscv64_user_trap_vector:"] {
        let leg = leg_instructions(SRC, entry);
        // parser self-check: a leg has at least dozens of instructions; catching
        // only a few lines means matching broke and the test would go falsely
        // green.
        assert!(
            leg.len() > 60,
            "{entry}: 只解析出 {} 条指令，解析器失效",
            leg.len()
        );
        let bad = leg_shape_violations(&leg);
        assert!(bad.is_empty(), "{entry} 形状违规：{bad:?}");
    }
}

#[test]
fn checker_fires_on_the_regression_it_exists_for() {
    // Positive control (guards against "the test code is itself wrong"): restore
    // the kernel leg to the order that preceded the fix (repurpose t0 before
    // storing it); the checker must fire.
    let mut leg = leg_instructions(SRC, "riscv64_kernel_trap_vector:");
    let rebased = leg
        .iter()
        .position(|i| i.starts_with("addi t0, sp, 34*8"))
        .expect("内核腿必须用 t0 求被打断的 sp");
    let saved = leg
        .iter()
        .position(|i| i == "sd t0, 5*8(sp)")
        .expect("内核腿必须把 t0 存进槽 5");
    assert!(
        saved < rebased,
        "对照样本只在「先保存后挪用」的现顺序下才有意义（保存在 {saved}，挪用在 {rebased}）"
    );
    leg.swap(rebased, saved);
    let bad = leg_shape_violations(&leg);
    assert!(
        bad.iter().any(|v| v.contains("t0 ")),
        "将 t0 换回旧顺序后检查器仍然沉默：{bad:?}"
    );
}

#[test]
fn parser_sees_both_legs_and_their_frame_slots() {
    // Third self-check: the slot-coverage rule really does count all 33 slots —
    // if an `sd` line stops being parsed because the format changed, rule 1
    // immediately reports "stored 0 times", but confirm the positive side here.
    for entry in ["riscv64_kernel_trap_vector:", "riscv64_user_trap_vector:"] {
        let leg = leg_instructions(SRC, entry);
        let stored: Vec<usize> = leg
            .iter()
            .filter_map(|i| stored_slot(i))
            .map(|(_, slot)| slot)
            .collect();
        assert_eq!(
            stored.len(),
            33,
            "{entry}: 解析到 {} 个帧槽存写，应为 33（1..=33）",
            stored.len()
        );
    }
}

/// Fish an instruction out of a leg (not finding it means the parser broke and
/// the test panics outright).
fn find(leg: &[String], want: &str, why: &str) -> usize {
    leg.iter()
        .position(|i| i == want)
        .unwrap_or_else(|| panic!("腿里找不到「{want}」（{why}）——解析器失效，对照样本无效"))
}

#[test]
fn negative_controls_make_each_rule_fire() {
    // Pair every rule with a known-bad sample (the positive control only covered
    // the reversed save-side order; any other rule written wrongly would go
    // silently green).
    let kernel = "riscv64_kernel_trap_vector:";
    let user = "riscv64_user_trap_vector:";

    // Rule 1a — drop one `sd`: slot 19 is never stored.
    let mut leg = leg_instructions(SRC, kernel);
    let at = find(&leg, "sd s3, 19*8(sp)", "内核腿必须把 s3 存进槽 19");
    leg.remove(at);
    let bad = leg_shape_violations(&leg);
    assert!(
        bad.iter().any(|v| v.contains("槽 19 被存了 0 次")),
        "删掉 s3 的存写后规则 1 未开火：{bad:?}"
    );

    // Rule 1b — duplicate one `sd`: slot 19 is stored twice (equivalent to
    // another register losing its home).
    let mut leg = leg_instructions(SRC, kernel);
    let at = find(&leg, "sd s3, 19*8(sp)", "内核腿必须把 s3 存进槽 19");
    leg.insert(at, leg[at].clone());
    let bad = leg_shape_violations(&leg);
    assert!(
        bad.iter().any(|v| v.contains("槽 19 被存了 2 次")),
        "重复 s3 的存写后规则 1 未开火：{bad:?}"
    );

    // Rule 2 — reversed save-side order (the defect this check exists for).
    let mut leg = leg_instructions(SRC, kernel);
    let a = find(&leg, "addi t0, sp, 34*8", "内核腿要拿 t0 回算被打断的 sp");
    let b = find(&leg, "sd t0, 5*8(sp)", "内核腿必须把 t0 存进自己的槽 5");
    leg.swap(a, b);
    let bad = leg_shape_violations(&leg);
    assert!(
        bad.iter().any(|v| v.contains("t0 在第")),
        "把 t0 换回旧顺序后规则 2 未开火：{bad:?}"
    );

    // Rule 3 — restore-side repurposing: write stvec through t0 after t0 was
    // reloaded.
    let mut leg = leg_instructions(SRC, user);
    let reload = find(&leg, "ld t0, 5*8(sp)", "用户腿必须把 t0 从槽 5 回载");
    leg.insert(reload + 1, "la t0, riscv64_user_trap_vector".to_string());
    leg.insert(reload + 2, "csrw stvec, t0".to_string());
    let bad = leg_shape_violations(&leg);
    assert!(
        bad.iter().any(|v| v.contains("t0 在自身回载")),
        "把 stvec 写放回 t0 回载之后（§续-371 的旧形状）规则 3 未开火：{bad:?}"
    );

    // Rule 4 — saved but never put back: drop the reload of s7.
    let mut leg = leg_instructions(SRC, kernel);
    let at = find(&leg, "ld s7, 23*8(sp)", "内核腿必须把 s7 从槽 23 送回");
    leg.remove(at);
    let bad = leg_shape_violations(&leg);
    assert!(
        bad.iter().any(|v| v.contains("s7 没有从自己槽")),
        "删掉 s7 回载后规则 4 未开火：{bad:?}"
    );

    // Rule 5a — drop the user leg's last stvec write (the one switching back to
    // the user leg before leaving): only the entry-time hand-back to the kernel
    // leg remains, and it sits ahead of the latch, so rule 5 must fire.
    let mut leg = leg_instructions(SRC, user);
    let writes: Vec<usize> = leg
        .iter()
        .enumerate()
        .filter(|(_, i)| i.starts_with("csrw stvec"))
        .map(|(k, _)| k)
        .collect();
    assert_eq!(
        writes.len(),
        2,
        "用户腿应当写两次 stvec（入口交还/出去换回），实际 {writes:?}"
    );
    leg.remove(*writes.last().unwrap());
    let bad = leg_shape_violations(&leg);
    assert!(
        bad.iter().any(|v| v.contains("排在 sstatus 写")),
        "删掉出口 stvec 写后规则 5 未开火：{bad:?}"
    );

    // Rule 5b — move the **exit** stvec write ahead of the sstatus write (a
    // "collect the CSR writes in one block" tidy-up still looks like this): the
    // masking latch did not land first, reopening the S-mode re-entry window.
    // The last write is taken here (the entry one belongs before the latch
    // anyway).
    let mut leg = leg_instructions(SRC, user);
    let v = leg
        .iter()
        .rposition(|i| i.starts_with("csrw stvec"))
        .expect("用户腿必须有出口 stvec 写");
    let s = find(&leg, "csrw sstatus, t0", "用户腿必须先落屏蔽闩");
    assert!(
        s < v,
        "现次序应当是闩先、出口向量后（闩 {s}，出口向量 {v}）"
    );
    leg.swap(s, v);
    let bad = leg_shape_violations(&leg);
    assert!(
        bad.iter().any(|v| v.contains("排在 sstatus 写")),
        "把 stvec 写换到闩之前后规则 5 未开火：{bad:?}"
    );

    // Rule 0 — fail-closed list: insert an instruction not in the table; it must
    // be reported as an unknown mnemonic rather than pass silently.
    let mut leg = leg_instructions(SRC, kernel);
    let at = find(&leg, "sd ra, 1*8(sp)", "内核腿必须把 ra 存进槽 1");
    leg.insert(at + 1, "gorgeous t9, 4(t0)".to_string());
    let bad = leg_shape_violations(&leg);
    assert!(
        bad.iter().any(|v| v.contains("未知助记符 gorgeous")),
        "表外助记符未被拦下（词表不是 fail-closed）：{bad:?}"
    );
}
