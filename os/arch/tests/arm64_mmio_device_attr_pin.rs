//! aarch64 设备内存属性（SD-42）的宿主形状钉住测试。
//!
//! 两个被门住的面各钉一半：
//!
//! 1. `os/arch/src/arm64/paging.rs`——设备属性必须在描述符里真实成立：
//!    `PageFlags::DEVICE` → `AttrIndx=1`（bits[4:2]）→ MAIR_EL1 Attr1 =
//!    0x00 = Device-nGnRnE；`pte_to_flags` 必须做逆映射（query 读回不能把
//!    设备页读成普通页）。MAIR 写入值必须仍是 0xFF（Attr0 = Normal WBWA）
//!    ——把 Attr1 写成别的非零值会让设备映射静默错档（例如 Normal
//!    Non-cacheable，仍缺强序语义）。
//! 2. `os/kernel/src/lib.rs` 的两处映射点——恒等映射块内的设备窗口必须按
//!    2 MiB 分片用 DEVICE、高点别名（Step 2b）必须带 DEVICE；两处属性一致
//!    是 ARM ARM 的要求（同一物理页两套属性 = UNPREDICTABLE）。
//!
//! 真机判读边界：QEMU 对 Normal/Device 的差异不发声，门禁只证「没写坏」；
//! 本钉补的是「属性真的进了描述符与映射点」这一静态面（SD-42 原文：
//! "gates cannot clear it"）。

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
    // 文档面：Attr1 = Device-nGnRnE 的判据必须留在文件里（0x00 是架构定义值，
    // 属「看似多余」的知识点，删掉它后人会把 Attr1 当成不可用）。
    assert!(
        s.contains("Attr1 = 0x00") && s.contains("Device-nGnRnE"),
        "MAIR Attr1 = Device-nGnRnE 的布局说明丢失"
    );
}

#[test]
fn kernel_maps_device_window_addr_window_as_device() {
    let c = strip_comments(include_str!("../../kernel/src/lib.rs"));
    // 恒等映射分片：DEV_BLOCK（2 MiB）与设备窗口常量必须同框出现。
    assert!(
        c.contains("const DEV_BLOCK: u64 = 1 << 21;"),
        "恒等映射的设备分片粒度常量丢失（2 MiB 分片是实现设备窗口属性的前提）"
    );
    assert!(
        c.contains("MMIO_PA_BASE") && c.contains("MMIO_WINDOW_LEN"),
        "恒等映射不再引用设备窗口边界——设备属性无从生效"
    );
    assert!(
        c.contains("PageFlags::kernel_read_write() | PageFlags::DEVICE"),
        "设备窗口的两处映射点未使用 DEVICE 属性（SD-42 的核心修法）"
    );
    // 高点别名（Step 2b）与恒等别名的一致性论证必须留在文件里
    // （论证住在注释里，这里查原始文本而非剥注释后的骨架）。
    let raw = include_str!("../../kernel/src/lib.rs");
    assert!(
        raw.contains("UNPREDICTABLE"),
        "两别名属性必须一致的论证丢失（同一物理页两套属性 = UNPREDICTABLE）"
    );
    // Step 2b 的高点别名必须带 DEVICE（与恒等别名同属性）。
    let two_b = strip_comments(raw);
    assert!(
        two_b
            .matches("PageFlags::kernel_read_write() | PageFlags::DEVICE")
            .count()
            >= 2,
        "恒等块与 Step 2b 两处映射点并非都带 DEVICE（两处必须同属性）"
    );
}
