//! Host-side shape pins for aarch64 instruction-cache synchronization.
//!
//! `os/arch/src/arm64/cache_sync.rs` is gated out as a whole module by
//! `#[cfg(target_arch = "aarch64")]` (`os/arch/src/lib.rs`), so a host
//! (x86_64) `cargo test` never compiles it — in-module unit tests would never
//! run (the same reachability discipline as the `arm64_arch_init_pin.rs`
//! family). This file uses `include_str!` to pin the load-bearing shapes on
//! the host side:
//!
//! 1. The five-step ARMv8 sequence is complete and correctly ordered —
//!    `DC CVAU` (clean data to PoU) → `DSB ISH` → `IC IVAU` (invalidate
//!    instructions to PoU) → `DSB ISH` → `ISB`. Losing any step or reversing
//!    the order (for example dropping `DC CVAU` and keeping only `IC IVAU`)
//!    turns "fetch stale instructions" into "fetch half-new half-old", which
//!    is even harder to reproduce on real hardware (the original wording of
//!    the architecture-parity risk note);
//! 2. Line sizes come from `CTR_EL0` with a 16-byte floor — the floor is a
//!    conservative value that prevents "decoding too small, so some line is
//!    left uncleaned"; removing the floor hands the safety margin to luck;
//! 3. The contract body must not grow `tlbi` or other unrelated maintenance
//!    instructions out of nowhere (this pin only recognizes the sequence
//!    itself).
//!
//! Real-hardware reading boundary (the original wording of the structural
//! debt register): QEMU does not model this class of cache mismatch, so a
//! green gate run cannot prove the item fixed — the gate can only prove "it
//! was not broken"; this pin adds the static side, "it really exists in the
//! code and in the right order".

/// Full source text (comments and docs included).
fn src() -> &'static str {
    include_str!("../src/arm64/cache_sync.rs")
}

/// Strip all `//` comments (whole-line `//!`/`//` and trailing comments), then
/// collapse consecutive whitespace into a single space — what remains is the
/// "executable code skeleton", used to decide whether a piece really exists as
/// an instruction rather than only being mentioned in a comment.
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
    // The second DSB must appear after IC IVAU: find the dsb following ic_ivau.
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
