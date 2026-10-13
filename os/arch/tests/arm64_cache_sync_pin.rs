//! aarch64 指令缓存同步（SD-20）的宿主形状钉住测试。
//!
//! `os/arch/src/arm64/cache_sync.rs` 被 `#[cfg(target_arch = "aarch64")]`
//! 整模块门住（`os/arch/src/lib.rs`），宿主（x86_64）`cargo test` 不编译它——
//! in-module 单测写了也永不运行（同 `arm64_arch_init_pin.rs` 族的可达性纪律）。
//! 本文件用 `include_str!` 在宿主侧钉住承重形状：
//!
//! 1. ARMv8 序列五步齐备且次序正确——`DC CVAU`（清数据到 PoU）→ `DSB ISH`
//!    → `IC IVAU`（失效指令到 PoU）→ `DSB ISH` → `ISB`。任何一步丢失或
//!    次序颠倒（例如省掉 `DC CVAU` 只留 `IC IVAU`）会把「取陈旧指令」变成
//!    「取到半新半旧」，在真机上更难复现（`TODO-3ARCH-PARITY-20261006.md`
//!    P-A64RV-02 风险段的原话）；
//! 2. 行长来自 `CTR_EL0` 且带 16 字节下限——下限是防「解码偏小导致漏清
//!    某一行」的保守值，删掉下限等于把安全裕量交给运气；
//! 3. 契约体不得凭空长出 `tlbi`/其他无关维护指令（本钉只认序列本体）。
//!
//! 真机判读边界（`STRUCTURAL-DEBT-REGISTER-20261008.md` SD-20 原文）：QEMU
//! 不建模这一类缓存失配，所以任何门禁跑绿都不能证明本项修好——门只能证明
//! 「没把它写坏」；本钉补的是「它在代码里真实存在且次序正确」这一静态面。

/// 原始文件全文（含注释与文档）。
fn src() -> &'static str {
    include_str!("../src/arm64/cache_sync.rs")
}

/// 剥去所有 `//` 注释（整行 `//!`/`//` 与行内尾注释），再把连续空白折叠为
/// 单个空格——留下的是「可执行代码骨架」，用于判定某段是否真的写成了指令，
/// 而不是只在注释里被提及。
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
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn armv8_sequence_present_in_order() {
    let c = code();
    let dc_cvau = c.find("dc cvau, {}").expect("缺 DC CVAU（清数据到 PoU）");
    let dsb_first = c
        .find("dsb ish")
        .expect("缺第一步 DSB ISH（把清过的数据对指令侧发布）");
    let ic_ivau = c.find("ic ivau, {}").expect("缺 IC IVAU（失效指令到 PoU）");
    // 第二个 DSB 必须出现在 IC IVAU 之后：找 ic_ivau 之后的那个 dsb。
    let dsb_second_rel = c[ic_ivau..]
        .find("dsb ish")
        .expect("缺 IC IVAU 之后的 DSB ISH（完成失效）");
    let isb = c.find("isb").expect("缺收尾 ISB（流水线同步）");
    assert!(
        dc_cvau < dsb_first,
        "次序错：DC CVAU 必须在第一个 DSB ISH 之前"
    );
    assert!(
        dsb_first < ic_ivau,
        "次序错：DSB ISH 必须在 IC IVAU 之前（先发布再失效）"
    );
    assert!(
        ic_ivau + dsb_second_rel < isb,
        "次序错：收尾 ISB 必须在两个 DSB/IC IVAU 全完成之后"
    );
}

#[test]
fn line_sizes_come_from_ctr_el0_with_floor() {
    let c = code();
    assert!(
        c.contains("mrs {}, ctr_el0"),
        "行长未从 CTR_EL0 读取（写死行长会在别的实现上漏清）"
    );
    assert!(
        c.contains("const MIN_LINE_BYTES: u64 = 16;"),
        "16 字节保守下限缺失——解码偏小时会静默漏清尾部行"
    );
    assert!(
        c.contains("const CTR_EL0_DMINLINE_SHIFT: u32 = 16;"),
        "DminLine 位域位置漂移（CTR_EL0 bits[19:16]）"
    );
    assert!(
        c.contains("const CTR_EL0_IMINLINE_SHIFT: u32 = 0;"),
        "IminLine 位域位置漂移（CTR_EL0 bits[3:0]）"
    );
}

#[test]
fn no_unrelated_maintenance_instructions_in_body() {
    let c = code();
    for forbidden in ["tlbi", "dc ivac", "dc civac", "ic iallu", "at s1e1r"] {
        assert!(
            !c.contains(forbidden),
            "契约体里出现了序列之外的维护指令 `{forbidden}`——本契约只做 \
             data→instruction 同步这一件事"
        );
    }
}

#[test]
fn empty_range_is_short_circuited() {
    let c = code();
    assert!(
        c.contains("if range.start >= range.end { return; }"),
        "空区间短路被移除：空区间按契约是 no-op，缺了它会把空区间也走一遍序列"
    );
}
