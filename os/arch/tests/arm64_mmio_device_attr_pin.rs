//! Host-side shape pin for aarch64 device memory attributes.
//!
//! Each of the two gated surfaces gets pinned halfway:
//!
//! 1. `os/arch/src/arm64/paging.rs` — the device attribute must really hold in
//!    the descriptor: `PageFlags::DEVICE` → `AttrIndx=1` (bits[4:2]) →
//!    MAIR_EL1 Attr1 = 0x00 = Device-nGnRnE; `pte_to_flags` must perform the
//!    reverse mapping (a query read-back must not report a device page as a
//!    normal page). The MAIR write value must still be 0xFF (Attr0 = Normal
//!    WBWA) — writing Attr1 as some other non-zero value silently mis-bins
//!    device mappings (Normal Non-cacheable, for instance, still lacks strong
//!    ordering semantics).
//! 2. The two mapping points in `os/kernel/src/lib.rs` — the device window
//!    inside the identity-mapped block must use DEVICE at 2 MiB granularity, and
//!    the high-half alias (Step 2b) must carry DEVICE; identical attributes at
//!    both places is an ARM ARM requirement (two attribute sets for one physical
//!    page = UNPREDICTABLE).
//!
//! Real-hardware reading boundary: QEMU does not report Normal/Device
//! differences, so the gate suites only prove "nothing was written wrong"; this
//! pin supplies the static surface "the attribute really reaches the descriptor
//! and the mapping points" (the standard phrasing: "gates cannot clear it").

fn strip_comments(src: &str) -> String {
    let mut out = String::new();
    for line in src.lines() {
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
fn device_flag_selects_mair_attr_index_one() {
    let c = strip_comments(include_str!("../src/arm64/paging.rs"));
    assert!(
        c.contains("const ATTR_IDX_DEVICE = 1 << 2;"),
        "AttrIndx=1 的位域常量漂移（bits[4:2]；值错会落到未定义的 MAIR 字节）"
    );
    assert!(
        c.contains(
            "if flags.contains(PageFlags::DEVICE) { pte |= Arm64PteFlags::ATTR_IDX_DEVICE; }"
        ),
        "flags_to_pte 不再为 DEVICE 置 AttrIndx——设备页会静默退回 Attr0（Normal WBWA）"
    );
    assert!(
        c.contains(
            "if hw.contains(Arm64PteFlags::ATTR_IDX_DEVICE) { flags |= PageFlags::DEVICE; }"
        ),
        "pte_to_flags 缺逆映射——query 读回会把设备页读成普通页（对账判据失真）"
    );
}

#[test]
fn mair_layout_keeps_attr0_normal_and_attr1_device() {
    let s = include_str!("../src/arm64/paging.rs");
    let c = strip_comments(s);
    assert!(
        c.contains("const MAIR_EL1_VALUE: u64 = 0xFF;"),
        "MAIR 写入值漂移——Attr0 必须保持 0xFF（Normal WBWA），低 16 位之外的\
         比特应保持 0；写字面量直接内联会丢掉这一步的名字与校验锚"
    );
    // Documentation side: the Attr1 = Device-nGnRnE criterion must stay in the
    // file (0x00 is an architecturally defined value — knowledge that looks
    // redundant; drop it and later readers take Attr1 for unusable).
    assert!(
        s.contains("Attr1 = 0x00") && s.contains("Device-nGnRnE"),
        "MAIR Attr1 = Device-nGnRnE 的布局说明丢失"
    );
}

#[test]
fn kernel_maps_device_window_addr_window_as_device() {
    let c = strip_comments(include_str!("../../kernel/src/lib.rs"));
    // Identity-mapping granule: the 2 MiB device piece and the device window
    // constants must appear together. SD-42 generalized the carve-out (the
    // riscv64 leg reuses it for PLIC/UART), so the pin also requires the two
    // decision helpers — the split must stay a *decision*, not inline text.
    assert!(
        c.contains("const DEV_PIECE: u64 = 1 << 21;"),
        "恒等映射的设备分片粒度常量丢失（2 MiB 分片是实现设备窗口属性的前提）"
    );
    assert!(
        c.contains("fn identity_block_touches_device")
            && c.contains("fn identity_piece_is_device")
            && c.contains("fn identity_device_windows"),
        "设备分片的判定被内联回循环——窗口/分片决策的可测缝丢失"
    );
    assert!(
        c.contains("MMIO_PA_BASE") && c.contains("MMIO_WINDOW_LEN"),
        "恒等映射不再引用设备窗口边界——设备属性无从生效"
    );
    assert!(
        c.contains("PageFlags::kernel_read_write() | PageFlags::DEVICE"),
        "设备窗口的两处映射点未使用 DEVICE 属性（SD-42 的核心修法）"
    );
    // The consistency argument between the high-half alias (Step 2b) and the
    // identity alias must stay in the file (the argument lives in comments, so
    // this checks the raw text rather than the comment-stripped skeleton).
    let raw = include_str!("../../kernel/src/lib.rs");
    assert!(
        raw.contains("UNPREDICTABLE"),
        "两别名属性必须一致的论证丢失（同一物理页两套属性 = UNPREDICTABLE）"
    );
    // The Step 2b high-half alias must carry DEVICE (the same attribute as the
    // identity alias).
    let two_b = strip_comments(raw);
    assert!(
        two_b
            .matches("PageFlags::kernel_read_write() | PageFlags::DEVICE")
            .count()
            >= 2,
        "恒等块与 Step 2b 两处映射点并非都带 DEVICE（两处必须同属性）"
    );
}
