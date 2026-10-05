//! riscv64 trap 腿的 asm 形状契约（NK4C §续-370）。
//!
//! 为什么这是一个**宿主集成测试**而不是 `trap_stub.rs` 里的 `#[test]`：
//! `minix-arch` 的 `pub mod riscv64` 带 `#[cfg(target_arch = "riscv64")]`，
//! 宿主 `cargo test` 根本不编译那个模块——写在里面的测试永远不会跑（=假绿）。
//! 本文件走 `include_str!` 读同一份源码文本做静态对账：不链接、不启动，
//! 只在宿主测试里检查 asm 的**指令顺序**。
//!
//! 检查的两条规则（都直接源自本案踩过的坑）：
//! 1. **帧覆盖**：槽 1..=33 每槽恰好被 `sd` 一次——漏写一槽=返回时把一个
//!    从未保存的寄存器留给被打断的上下文；重复写一槽=某个被打断的寄存器
//!    没有家。
//! 2. **scratch 纪律**：帧要携带的寄存器必须先落进**自己的槽**，随后才允许
//!    被本腿当作暂存使用。顺序反了就等于异步地把被打断上下文的一个寄存器
//!    换成腿内临时值——不报异常、不留日志（§续-370 的内核腿正是这种形状：
//!    `t0` 先被 `addi t0, sp, 34*8` 挪用去求被打断的 `sp`，之后才
//!    `sd t0, 5*8(sp)`，于是每一次监管态定时器中断都把内核的 `t0` 换成栈
//!    指针地址）。
//!
//! 正控制测试（`checker_fires_on_the_regression_it_exists_for`）保证解析器
//! 不是「什么都没匹配所以全绿」。

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

/// 首操作数被该指令写入的助记符表（本腿实际用到的都在里面；表宽一点无妨，
/// 只会让规则 2 更早开火）。
const DEST_FIRST: &[&str] = &[
    "addi", "li", "mv", "la", "ld", "lw", "lh", "lb", "csrr", "csrrw", "add", "sub", "and", "andi",
    "or", "ori", "srli", "slli", "srai", "neg", "not", "seqz", "sltz", "mul",
];

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
fn leg_instructions(src: &str, entry: &str) -> Vec<String> {
    let at = src
        .find(entry)
        .unwrap_or_else(|| panic!("腿入口 {entry} 在 trap_stub 源里找不到"));
    let rest = &src[at..];
    let end = rest
        .find("sret\"")
        .unwrap_or_else(|| panic!("腿 {entry} 没有 sret 收尾"));
    rest[..end]
        .lines()
        .filter_map(|line| {
            let body = line.trim().strip_prefix('"')?.split('"').next()?;
            let ins = body.trim();
            if ins.is_empty() || ins.starts_with('.') || ins.starts_with('#') {
                None
            } else {
                Some(ins.split_whitespace().collect::<Vec<_>>().join(" "))
            }
        })
        .collect()
}

/// 两条形状规则的判定；返回违规清单（空=清白）。独立成函数是为了能被
/// 「已知坏样本」正向对照。
fn leg_shape_violations(leg: &[String]) -> Vec<String> {
    let mut bad = Vec::new();
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
    // 规则 2：scratch 纪律
    for &(name, slot) in ABI_SLOT {
        let own_store = format!("sd {name}, {slot}*8(sp)");
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
