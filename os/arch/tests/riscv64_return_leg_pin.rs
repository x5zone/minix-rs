//! Pinning test for the riscv64 `restore_to_user` return leg.
//!
//! Why a separate file instead of reusing the rules in
//! `riscv64_trap_leg_shape.rs`: this leg's assembly shape differs from the trap
//! leg's -- the register-file pointer lives in **t6** and reloads itself by
//! self-reference, the stride uses `GP_*` constants instead of
//! "slot number = register number", and placeholders such as `{sepc_off}` are
//! interleaved with the operands. Applying the trap leg's slot rules verbatim
//! would light up a field of false failures, so this file pins only the three
//! things this leg genuinely bears:
//!
//! 1. **Mask before switching legs**: this leg must clear `sstatus.SIE` before
//!    writing `stvec`. Otherwise S-mode enters a user leg that is valid only for
//!    U-origin with SIE still open, and one clock tick writes the 34-slot frame
//!    into `[kernel stack top - 272, kernel stack top)` -- exactly where the
//!    scheduler's live frames live (the aarch64 sibling implementation opens
//!    with `msr daifset, #0xf` for this very reason, and its comments record the
//!    symptom observed back then: "silent hang after the first return").
//! 2. **Switch the vector before reloading**: the `stvec` write must precede
//!    any user-register reload -- that write needs a scratch register, and after
//!    the reload every register belongs to the interrupted user context.
//! 3. **No repurposing after the reload**: between taking back the user values
//!    of `t0`/`t6` and `sret`, neither may be written again (t6's self-reference
//!    reload must be the last write to it).
//!
//! The bit definitions are pinned here too: `SIE` is bit 1 (current enable),
//! `SPIE` is bit 5 (consumed by `sret`); writing the mask as a clear of bit 5
//! would both leave a window open and leave the post-return context with
//! interrupts disabled entirely (an older shape that was tried before).

/// Extracts the instruction lines out of the `core::arch::asm!( … )` block in
/// order, dropping indentation and Rust comments.
fn asm_instructions(src: &str) -> Vec<String> {
    let at = src
        .find("core::arch::asm!(")
        .expect("restore_to_user 的 asm 块找不到");
    let tail = &src[at..];
    let end = tail
        .find("in(\"a0\")")
        .expect("asm 块的输入操作数找不到（格式漂移？）");
    let out: Vec<String> = tail[..end]
        .lines()
        .filter_map(|line| {
            let body = line.trim().strip_prefix('"')?.split('"').next()?;
            let ins = body.trim();
            if ins.is_empty() || (ins.ends_with(':') && !ins.contains(' ')) {
                None
            } else {
                Some(ins.split_whitespace().collect::<Vec<_>>().join(" "))
            }
        })
        .collect();
    // fail-closed: a mnemonic outside the table fails immediately (silently
    // treating it as "does not write a register" would leave a blind spot in
    // the lifetime pinning).
    if let Some(bad) = out.iter().find(|i| {
        i.split_whitespace()
            .next()
            .map(|mn| !KNOWN_MNEMONICS.contains(&mn))
            .unwrap_or(false)
    }) {
        panic!("本腿出现表外助记符，钉住测试无法证明它不写首操作数：{bad}");
    }
    out
}

/// Mnemonics allowed on this leg (fail-closed: anything outside the table fails
/// on the spot rather than being silently treated as "does not write a
/// register" -- the shape test for the sibling leg already argued this
/// discipline, and the two files must not differ in strictness).
const KNOWN_MNEMONICS: &[&str] = &[
    "li", "mv", "la", "add", "ld", "csrw", "csrc", "csrs", "bnez", "j", "sret",
];

/// These registers serve only as scratch on this leg (their user values are
/// reloaded later), so their appearance in the reload sentinel does not count
/// as a reload.
const SCRATCH_REGS: &[&str] = &["t0", "t1", "a4"];

/// Whether this instruction writes register `name` (destination-first form).
/// Placeholders `{..}` do not affect the decision, because everything written is
/// a register name.
fn writes(ins: &str, name: &str) -> bool {
    const DEST_FIRST: &[&str] = &[
        "li", "mv", "la", "ld", "csrr", "csrrw", "add", "addi", "sub", "and", "andi", "or", "ori",
        "srli", "slli", "csrc", "csrs",
    ];
    // The CSR name in csrc/csrs is not a register; after splitting the first
    // operand on ',' the result is still a register name.
    let (mn, rest) = match ins.split_once(char::is_whitespace) {
        Some(pair) => pair,
        None => return false,
    };
    if !DEST_FIRST.contains(&mn) {
        return false;
    }
    rest.split(',').next().map(|r| r.trim()) == Some(name)
}

fn pos(leg: &[String], pred: impl Fn(&str) -> bool) -> Option<usize> {
    leg.iter().position(|i| pred(i))
}

fn rpos(leg: &[String], pred: impl Fn(&str) -> bool) -> Option<usize> {
    leg.iter().rposition(|i| pred(i))
}

/// The first "real reload": an `ld` that writes a register outside the scratch
/// set. Using `ld sp,` as the only sentinel is not enough -- if someone reorders
/// the leg to reload a1 first, the sentinel reports late.
fn first_real_reload(leg: &[String]) -> Option<usize> {
    leg.iter().position(|i| {
        if !i.starts_with("ld ") {
            return false;
        }
        let reg = i[3..].split(',').next().unwrap_or("").trim();
        !SCRATCH_REGS.contains(&reg)
    })
}

/// The fixed idiom this repo uses for clearing CSR bit fields in asm:
/// `li <r>, {flag}` immediately followed by `csrc sstatus, <r>`. Match on that
/// idiom rather than on a register name -- register names get rewritten, but the
/// "load the mask then clear" form does not change.
fn sstatus_clear(leg: &[String], symbol: &str) -> Option<usize> {
    let li = pos(leg, |i| i.starts_with("li ") && i.contains(symbol))?;
    let reg = leg[li].split_whitespace().nth(1)?.split(',').next()?;
    let want = format!("csrc sstatus, {reg}");
    pos(leg, |i| i == want)
}

const SRC: &str = include_str!("../src/riscv64/trap_return.rs");

#[test]
fn sstatus_bits_match_the_privileged_spec() {
    // The bit definitions are read out of the source text and reconciled (the
    // source sits in a cfg(riscv64) module, so a host-only build cannot run its
    // #[test]; hence reading the text here).
    for (needle, want) in [
        ("SSTATUS_SPP", 1u64 << 8),
        ("SSTATUS_SPIE", 1u64 << 5),
        ("SSTATUS_SIE", 1u64 << 1),
    ] {
        let line = SRC
            .lines()
            .find(|l| l.contains(&format!("const {needle}")))
            .unwrap_or_else(|| panic!("{needle} 的定义找不到"));
        let rhs = line.split('=').nth(1).expect("赋值右侧");
        let shift: u32 = rhs
            .split("<<")
            .nth(1)
            .expect("位常量应写成 1 << n")
            .trim()
            .trim_end_matches(';')
            .trim()
            .parse()
            .unwrap_or_else(|_| panic!("{needle} 的移位量解析失败：{line}"));
        assert_eq!(1u64 << shift, want, "{needle} 应当是 {line}");
    }
}

#[test]
fn return_leg_masks_interrupts_before_switching_the_vector() {
    let leg = asm_instructions(SRC);
    assert!(leg.len() > 40, "只解析出 {} 条指令，解析器失效", leg.len());

    let mask = sstatus_clear(&leg, "{sie}")
        .expect("本腿必须先清 sstatus.SIE（与 aarch64 的 msr daifset 对齐）");
    // The {spp} load/clear pair serves as the counter-example: it clears SPP
    // (the privilege-level bit) and must not be mistaken for the mask.
    let spp_clear = sstatus_clear(&leg, "{spp}").expect("本腿必须清 SPP（返回 U 态）");
    assert_ne!(mask, spp_clear, "屏蔽与清 SPP 应当是两笔不同的写");
    let anchor =
        pos(&leg, |i| i.starts_with("csrw sscratch,")).expect("本腿必须重锚 sscratch 为内核栈顶");
    // What is pinned is the **last** stvec write: if someone later adds another
    // vector switch at the tail of this leg, the earlier one no longer decides
    // "who receives the next trap after the return" (the same discipline as rule
    // 5 of the shape test).
    let vector = rpos(&leg, |i| i.starts_with("csrw stvec,")).expect("本腿必须把 stvec 指向用户腿");
    let first_reload = first_real_reload(&leg).expect("首个真回载找不到");
    let sret = pos(&leg, |i| i == "sret").expect("本腿必须以 sret 收尾");

    assert!(
        mask < vector,
        "SIE 屏蔽（第 {mask} 条）必须早于 stvec 换腿（第 {vector} 条）：否则这段窗口里 \
         S 态带着开着的中断走进只适用于 U-origin 的用户腿，时钟会把 34 槽帧写进调度器活栈"
    );
    assert!(
        anchor < vector,
        "sscratch 重锚（第 {anchor} 条）应早于换腿（第 {vector} 条）：换腿之后 sscratch \
         的语义就被人依赖了"
    );
    assert!(
        vector < first_reload,
        "stvec 换腿（第 {vector} 条）必须早于首个用户寄存器回载（第 {first_reload} 条）：\
         那一写要占用一个暂存寄存器"
    );
    assert_eq!(sret, leg.len() - 1, "sret 必须是本腿最后一条指令");

    // No repurposing after the reload: t0 (x5) and t6 (x31, the register-file
    // pointer, which closes with a self-reference).
    let t0_back = pos(&leg, |i| i.starts_with("ld t0, 3*8")).expect("t0 的用户值回载");
    if let Some(bad) = (t0_back + 1..sret).find(|&k| writes(&leg[k], "t0")) {
        panic!(
            "t0 在回载（第 {t0_back} 条）之后第 {bad} 条又被写：{}/",
            leg[bad]
        );
    }
    let t6_back = pos(&leg, |i| i.starts_with("ld t6, 28*8")).expect("t6 的自引用回载");
    if let Some(bad) = (t6_back + 1..sret).find(|&k| writes(&leg[k], "t6")) {
        panic!(
            "t6 在自引用回载（第 {t6_back} 条）之后第 {bad} 条又被写：{}/",
            leg[bad]
        );
    }
    assert!(
        t6_back < sret && t0_back < sret,
        "两个寄存器都必须在 sret 之前取回用户值"
    );
}

#[test]
fn controls_fire_when_the_mask_is_missing_or_misplaced() {
    // Positive control (guards against "the parser matched nothing, so
    // everything is green"): delete the mask, or move it after the vector
    // switch, and the main test's predicate must stop holding.
    let mut leg = asm_instructions(SRC);
    let mask = sstatus_clear(&leg, "{sie}").expect("屏蔽写必须在位");
    let vector = rpos(&leg, |i| i.starts_with("csrw stvec,")).expect("换腿写在位");
    assert!(
        mask < vector,
        "现次序应为「先屏蔽后换腿」，实测 {mask} vs {vector}"
    );

    // (1) Delete the mask write: the predicate must no longer find a mask (and
    // must not mis-identify the SPP clear as one).
    let mut dropped = leg.clone();
    dropped.remove(mask);
    assert!(
        sstatus_clear(&dropped, "{sie}").is_none(),
        "删掉屏蔽后仍能匹配到屏蔽写，说明判据认错了对象"
    );
    assert!(
        sstatus_clear(&dropped, "{spp}").is_some(),
        "清 SPP 那笔应当还在（这一步只证判据分得清屏蔽与清特权位两事）"
    );

    // (2) Move the mask after the vector switch: the ordering predicate must
    // flip.
    leg.swap(mask, vector);
    let mask_after = sstatus_clear(&leg, "{sie}").expect("换位后屏蔽写仍在，只是位置变了");
    let vector_after = rpos(&leg, |i| i.starts_with("csrw stvec,")).expect("换位后换腿写仍在");
    assert!(
        vector_after < mask_after,
        "换位实验失效（应当变成「换腿先于屏蔽」）：{vector_after} vs {mask_after}"
    );
}
