//! riscv64 trap 腿的 asm 形状契约（NK4C §续-370）。
//!
//! 为什么这是一个**宿主集成测试**而不是 `trap_stub.rs` 里的 `#[test]`：
//! `minix-arch` 的 `pub mod riscv64` 带 `#[cfg(target_arch = "riscv64")]`，
//! 宿主 `cargo test` 根本不编译那个模块——写在里面的测试永远不会跑（=假绿）。
//! 本文件走 `include_str!` 读同一份源码文本做静态对账：不链接、不启动，
//! 只在宿主测试里检查 asm 的**指令顺序**。
//!
//! 检查的六条形状规则（都直接源自本案踩过的坑）：
//! 0. **词表 fail-closed**：腿里只允许已知助记符——表外指令无法证明它不写
//!    首操作数，直接算违规（否则将来新增一条指令就静默绕过下面几条）。
//! 1. **帧覆盖**：槽 1..=33 每槽恰好被 `sd` 一次——漏写一槽=返回时把一个
//!    从未保存的寄存器留给被打断的上下文；重复写一槽=某个被打断的寄存器
//!    没有家。
//! 2. **保存侧暂存纪律**：帧要携带的寄存器必须先落进**自己的槽**，随后才允许
//!    被本腿当作暂存使用。顺序反了就等于异步地把被打断上下文的一个寄存器
//!    换成腿内临时值——不报异常、不留日志（§续-370 的内核腿正是这种形状：
//!    `t0` 先被 `addi t0, sp, 34*8` 挪用去求被打断的 `sp`，之后才
//!    `sd t0, 5*8(sp)`，于是每一次监管态定时器中断都把内核的 `t0` 换成栈
//!    指针地址）。
//! 3. **恢复侧生命期**：从自己的槽回载之后，到 `sret` 之间不得再被当作暂存。
//!    §续-371 的评审就是在本条上拓出第二个同形缺陷：用户腿在 `ld t0, 5*8(sp)`
//!    之后又用 t0 写 `stvec`，返回用户态时 t0 等于自己的栈指针——而本案被
//!    击中的正是用户进程。
//! 4. **回载覆盖**：存进帧的每个寄存器都必须从自己的槽送回。
//! 5. **关键 CSR 写的存在与次序**：两条腿都必须先写帧里的 `sstatus`（硬件
//!    在陷入时已清 SIE，所以那一写就是尾声的屏蔽闩）、再装 `stvec`，且装在
//!    任何 GPR 回载之前。把向量写挪到闩之前会重新打开「S 态带 SIE=1 重入
//!    只适用于 U-origin 的腿」的窗口（§续-371 评审回执 P2-2/P2-4），而形状
//!    判定若不钉次序就拦不住这种「整理」。
//!
//! 正控制测试（`checker_fires_on_the_regression_it_exists_for` 与
//! `negative_controls_make_each_rule_fire`）逐条给规则配已知坏样本，
//! 保证解析器不是「什么都没匹配所以全绿」。

/// 帧槽 ↔ 架构寄存器名。`gpr[i]` 就是 `x_i`；`sp`(x2) 是唯一从不由
/// `sd <自己的名字>` 写入的槽（两腿都经暂存寄存器把它送进槽 2，属设计）。
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

/// 本腿允许出现的助记符白名单。**fail-closed**：表外的助记符＝无法证明它
/// 不写首操作数，直接计违规——逼后来的人把它加进表并归类，而不是让新指令
/// 静默躲过形状判定（§续-371 评审回执 P2：原表不全会悄悄盲）。
const KNOWN_MNEMONICS: &[&str] = &[
    "addi", "sd", "ld", "csrr", "csrw", "csrrw", "mv", "la", "call", "beqz", "jr", "srli", "andi",
    "sret", "j", "b", "jal", "jalr", "ret", "nop",
];

/// 首操作数被该指令写入的助记符表。
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

/// `sd <reg>, <slot>*8(sp)` → (reg, slot)。
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

/// 取一条腿的指令序列：从入口标号到它的 `sret`。指令文本嵌在 Rust
/// `global_asm!` 的字符串字面量里，因此只取「以引号开始」的行并截到下一个
/// 引号；伪指令（`.align`/`.globl`/`.size`）与字面量之间的 Rust 注释自然被
/// 过滤掉。
///
/// 锚点故意定在**带引号的标号行**（`"riscv64_kernel_trap_vector:"`）而不是裸
/// 标号名：否则将来有人在文档注释里写出同一个带冒号的名字，用户腿切片就会
/// 从文件更早处开始并停在内核腿的 `sret`，五条规则全绿而真正的用户腿从未被
/// 检查过（§续-371 评审回执 P2-5：要让锚点具有判别力，不能靠巧合）。
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
                // 数字标号（`8:`/`9:`/`1:`）不是指令，不参与判定
                || (ins.ends_with(':') && !ins.contains(' '))
            {
                None
            } else {
                Some(ins.split_whitespace().collect::<Vec<_>>().join(" "))
            }
        })
        .collect()
}

/// 形状规则的判定；返回违规清单（空=清白）。独立成函数是为了能被
/// 「已知坏样本」正向对照。六条规则都只读指令序列，不碰硬件。
fn leg_shape_violations(leg: &[String]) -> Vec<String> {
    let mut bad = Vec::new();
    // 规则 0（fail-closed 词表）：未知助记符一律当可疑。
    for ins in leg {
        if let Some(mn) = mnemonic(ins)
            && !KNOWN_MNEMONICS.contains(&mn)
        {
            bad.push(format!("未知助记符 {mn}（不能证明它不写首操作数）：{ins}"));
        }
    }
    // 规则 1：帧覆盖
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
        // 规则 2（保存侧暂存纪律）：帧要携带的寄存器先落进自己的槽，
        // 之后才允许被本腿当作暂存使用。
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
        // 规则 3（恢复侧生命期）：从自己的槽回载之后，到 `sret` 之间不得
        // 再被当作暂存——那一笔就是被打断上下文的值（§续-371：用户腿在
        // `ld t0, 5*8(sp)` 之后又用 t0 写 stvec，返回用户态时 t0=用户 sp）。
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
            // 规则 4（回载覆盖）：保存过的寄存器必须有人把它送回去。
            bad.push(format!("{name} 没有从自己槽（{slot}）回载就 sret"));
        }
    }
    // 规则 5（关键 CSR 写的存在与次序，§续-371 评审回执 P2-2/P2-4）：
    // 两条腿都靠「先写帧里的 sstatus（硬件在陷入时已清 SIE）、再装 stvec」
    // 把尾声变成屏蔽窗口。用户腿会写 stvec 两次（进处理体前交还内核腿、
    // 出去前换回用户腿），内核腿只写一次，所以这里钉的是**最后一次**的位置：
    // 它必须在闩之后、任何 GPR 回载之前（否则它用的暂存寄存器已属于被打断
    // 上下文，且 S 态带 SIE=1 重入只适用于 U-origin 腿的窗口被打开）。
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
    // 用户腿专属：进处理体之前必须先把向量交还内核腿（处理体内的故障不得
    // 再走一次 sscratch 交换，否则会在同一个内核栈顶再叠一帧）。
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
        // 解析器自证：一条腿至少几十条指令；只抓到几行=匹配失效，测试会假绿。
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
    // 正控制（防「测试代码自己就是错的」）：把内核腿恢复成 §续-370 修复前
    // 的顺序（先挪用 t0 再存它），检查器必须开火。
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
    // 第三条自证：槽覆盖规则真的在数满 33 槽——若某个 `sd` 行因格式变动
    // 没被解析，规则 1 会立刻报「存了 0 次」，但这里正向确认一次。
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

/// 把一条指令从腿里摸出来（找不到就是解析器失效，测试直接崩）。
fn find(leg: &[String], want: &str, why: &str) -> usize {
    leg.iter()
        .position(|i| i == want)
        .unwrap_or_else(|| panic!("腿里找不到「{want}」（{why}）——解析器失效，对照样本无效"))
}

#[test]
fn negative_controls_make_each_rule_fire() {
    // 逐条给规则配一个已知坏样本（§续-371 评审回执 P2：正控制只盖住了
    // 保存侧倒序一种，其余规则一旦写错就会静默假绿）。
    let kernel = "riscv64_kernel_trap_vector:";
    let user = "riscv64_user_trap_vector:";

    // 规则 1a——漏一个 `sd`：槽 19 没人存。
    let mut leg = leg_instructions(SRC, kernel);
    let at = find(&leg, "sd s3, 19*8(sp)", "内核腿必须把 s3 存进槽 19");
    leg.remove(at);
    let bad = leg_shape_violations(&leg);
    assert!(
        bad.iter().any(|v| v.contains("槽 19 被存了 0 次")),
        "删掉 s3 的存写后规则 1 未开火：{bad:?}"
    );

    // 规则 1b——重复一个 `sd`：槽 19 被存两次（等于另一个寄存器没了家）。
    let mut leg = leg_instructions(SRC, kernel);
    let at = find(&leg, "sd s3, 19*8(sp)", "内核腿必须把 s3 存进槽 19");
    leg.insert(at, leg[at].clone());
    let bad = leg_shape_violations(&leg);
    assert!(
        bad.iter().any(|v| v.contains("槽 19 被存了 2 次")),
        "重复 s3 的存写后规则 1 未开火：{bad:?}"
    );

    // 规则 2——保存侧倒序（§续-370 那个缺陷）。
    let mut leg = leg_instructions(SRC, kernel);
    let a = find(&leg, "addi t0, sp, 34*8", "内核腿要拿 t0 回算被打断的 sp");
    let b = find(&leg, "sd t0, 5*8(sp)", "内核腿必须把 t0 存进自己的槽 5");
    leg.swap(a, b);
    let bad = leg_shape_violations(&leg);
    assert!(
        bad.iter().any(|v| v.contains("t0 在第")),
        "把 t0 换回旧顺序后规则 2 未开火：{bad:?}"
    );

    // 规则 3——恢复侧挪用（§续-371 那个缺陷）：在 t0 回载之后再拿它写 stvec。
    let mut leg = leg_instructions(SRC, user);
    let reload = find(&leg, "ld t0, 5*8(sp)", "用户腿必须把 t0 从槽 5 回载");
    leg.insert(reload + 1, "la t0, riscv64_user_trap_vector".to_string());
    leg.insert(reload + 2, "csrw stvec, t0".to_string());
    let bad = leg_shape_violations(&leg);
    assert!(
        bad.iter().any(|v| v.contains("t0 在自身回载")),
        "把 stvec 写放回 t0 回载之后（§续-371 的旧形状）规则 3 未开火：{bad:?}"
    );

    // 规则 4——保存了却没人送回：删掉 s7 的回载。
    let mut leg = leg_instructions(SRC, kernel);
    let at = find(&leg, "ld s7, 23*8(sp)", "内核腿必须把 s7 从槽 23 送回");
    leg.remove(at);
    let bad = leg_shape_violations(&leg);
    assert!(
        bad.iter().any(|v| v.contains("s7 没有从自己槽")),
        "删掉 s7 回载后规则 4 未开火：{bad:?}"
    );

    // 规则 5a——删掉用户腿最后一次 stvec 写（出去前换回用户腿那一笔）：
    // 剩下的只有入口处交还内核腿那一笔，它排在闩之前，规则 5 必须开火。
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

    // 规则 5b——把**出口那一笔** stvec 写挪到 sstatus 写之前（「把 CSR 集中到
    // 一段」式整理就仍会长这样）：屏蔽闩未先落，S 态重入窗口被重新打开。
    // 这里取的是最后一次（入口那笔本来就该在闩之前）。
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

    // 规则 0——词表 fail-closed：塞一条表外指令，必须报「未知助记符」而不是静默通过。
    let mut leg = leg_instructions(SRC, kernel);
    let at = find(&leg, "sd ra, 1*8(sp)", "内核腿必须把 ra 存进槽 1");
    leg.insert(at + 1, "gorgeous t9, 4(t0)".to_string());
    let bad = leg_shape_violations(&leg);
    assert!(
        bad.iter().any(|v| v.contains("未知助记符 gorgeous")),
        "表外助记符未被拦下（词表不是 fail-closed）：{bad:?}"
    );
}
