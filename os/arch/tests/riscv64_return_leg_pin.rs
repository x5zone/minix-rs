//! riscv64 `restore_to_user` 返回腿的钉住测试（NK4C §续-373）。
//!
//! 为什么单开一个文件而不是复用 `riscv64_trap_leg_shape.rs` 的规则：那条腿的
//! 汇编形状与陷入腿不同——寄存器文件指针住在 **t6** 里做自引用回载，步长用
//! `GP_*` 常量而不是「槽号=寄存器号」，`{sepc_off}` 一类占位符也混在操作数里。
//! 硬套陷入腿的槽位规则会一片假红，所以这里只钉这条腿真正承重的三件事：
//!
//! 1. **屏蔽先于换腿**：本腿必须先清掉 `sstatus.SIE`，再写 `stvec`。否则
//!    S 态带着开着的 SIE 走进只适用于 U-origin 的用户腿，一拍时钟就会把
//!    34 槽帧写进 `[内核栈顶-272, 内核栈顶)`——那正是调度器活帧占的地盘
//!    （aarch64 同族实现第一句 `msr daifset, #0xf` 就是为这个，且注释里记着
//!    它当年观测到的症状「首次返回后静默挂死」）。
//! 2. **换腿先于回载**：`stvec` 写必须在任何用户寄存器回载之前——那一写要
//!    一个暂存寄存器，回载之后每个寄存器都属于被打断的用户上下文。
//! 3. **回载之后不再挪用**：`t0`/`t6` 从上下文取回用户值之后到 `sret` 之间
//!    不得再被写（`t6` 的自引用回载必须是最后一次写它）。
//!
//! 位定义也在这里钉：`SIE` 是 bit 1（现行使能），`SPIE` 是 bit 5（由 `sret`
//! 消费）；把屏蔽写成清 bit 5 会同时留下窗口并让返回后的上下文根本没有中断
//! （§续-108/109 的老形状）。

/// 抽出 `core::arch::asm!( … )` 里的指令行（按顺序），去掉缩进与 Rust 注释。
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
    // fail-closed：表外助记符当场失败（静默当作不写寄存器 = 生命期钉住有盲区）。
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

/// 本腿允许出现的助记符（fail-closed：表外就当场失败，而不是当作「不写寄存器」
/// 静默放过——§续-373 评审回执第二条：同族的形状测试已经论证过这条纪律，两个
/// 文件不能一个严一个松）。
const KNOWN_MNEMONICS: &[&str] = &[
    "li", "mv", "la", "add", "ld", "csrw", "csrc", "csrs", "bnez", "j", "sret",
];

/// 这些寄存器在本腿里只当暂存用（它们的用户值在后面才回载），所以它们出现在
/// 回载哨兵里不算回载。
const SCRATCH_REGS: &[&str] = &["t0", "t1", "a4"];

/// 这条指令是否写了寄存器 `name`（首操作数形式）。占位符 `{..}` 不影响判定，
/// 因为被写的都是寄存器名。
fn writes(ins: &str, name: &str) -> bool {
    const DEST_FIRST: &[&str] = &[
        "li", "mv", "la", "ld", "csrr", "csrrw", "add", "addi", "sub", "and", "andi", "or", "ori",
        "srli", "slli", "csrc", "csrs",
    ];
    // csrc/csrs 的 CSR 名不是寄存器，首操作数按 ',' 切开后仍是寄存器名。
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

/// 首个「真回载」：写一个不属于暂存集的寄存器的 `ld`。拿 `ld sp,` 当唯一哨兵
/// 不够——若有人把顺序改成先回载 a1，哨兵就会晚报（§续-373 评审回执第三条）。
fn first_real_reload(leg: &[String]) -> Option<usize> {
    leg.iter().position(|i| {
        if !i.starts_with("ld ") {
            return false;
        }
        let reg = i[3..].split(',').next().unwrap_or("").trim();
        !SCRATCH_REGS.contains(&reg)
    })
}

/// 本仓 asm 写 CSR 位段的固定套式：`li <r>, {标志}` 紧接 `csrc sstatus, <r>`。
/// 按这个套式找，而不拿寄存器名定位——寄存器名会被重写，而「先装掩码再清」
/// 这个形式不会变。
fn sstatus_clear(leg: &[String], symbol: &str) -> Option<usize> {
    let li = pos(leg, |i| i.starts_with("li ") && i.contains(symbol))?;
    let reg = leg[li].split_whitespace().nth(1)?.split(',').next()?;
    let want = format!("csrc sstatus, {reg}");
    pos(leg, |i| i == want)
}

const SRC: &str = include_str!("../src/riscv64/trap_return.rs");

#[test]
fn sstatus_bits_match_the_privileged_spec() {
    // 位定义从源码文本里读出来对账（源码在 cfg(riscv64) 模块里，宿主单编译
    // 跑不到它的 #[test]，所以在这里读文本）。
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
    // 拿 {spp} 那对装/清当反例：它清的是 SPP（特权级位），不能被当成屏蔽。
    let spp_clear = sstatus_clear(&leg, "{spp}").expect("本腿必须清 SPP（返回 U 态）");
    assert_ne!(mask, spp_clear, "屏蔽与清 SPP 应当是两笔不同的写");
    let anchor =
        pos(&leg, |i| i.starts_with("csrw sscratch,")).expect("本腿必须重锚 sscratch 为内核栈顶");
    // 钉的是**最后一次** stvec 写：若将来有人在本腿尾部再加一次换腿，早先那一次
    // 就不再决定「返回后谁接下一个陷入」（与形状测试规则 5 同一纪律）。
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

    // 回载之后不得再挪用：t0（x5）与 t6（x31，寄存器文件指针，自引用收尾）。
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
    // 正控制（防「解析器什么都没匹配所以全绿」）：删掉屏蔽、或把屏蔽挪到换腿
    // 之后，主测试的判据必须失效。
    let mut leg = asm_instructions(SRC);
    let mask = sstatus_clear(&leg, "{sie}").expect("屏蔽写必须在位");
    let vector = rpos(&leg, |i| i.starts_with("csrw stvec,")).expect("换腿写在位");
    assert!(
        mask < vector,
        "现次序应为「先屏蔽后换腿」，实测 {mask} vs {vector}"
    );

    // ①删掉屏蔽那一写：判据应当再也找不到屏蔽（而不是认错到清 SPP 那笔）。
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

    // ②把屏蔽挪到换腿之后：次序判据必须翻脸。
    leg.swap(mask, vector);
    let mask_after = sstatus_clear(&leg, "{sie}").expect("换位后屏蔽写仍在，只是位置变了");
    let vector_after = rpos(&leg, |i| i.starts_with("csrw stvec,")).expect("换位后换腿写仍在");
    assert!(
        vector_after < mask_after,
        "换位实验失效（应当变成「换腿先于屏蔽」）：{vector_after} vs {mask_after}"
    );
}
