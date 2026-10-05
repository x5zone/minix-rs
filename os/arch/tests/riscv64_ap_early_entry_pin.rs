//! riscv64 AP 早期桩的形状钉住测试（P-A64RV-01 第一增量 §续-398）。
//!
//! 桩的承重次序（义务清单 §3.1 的汇编体）：fence 首读栅栏先于记录读取、
//! satp 装载（Sv39 模式位）先于 sfence、栈与汇聚点跳转在 MMU on 之后。
//! 次序漂移＝AP 在错误地址翻译下取指（静默挂死或三术异常）。

fn stub_instructions() -> Vec<String> {
    let src = include_str!("../src/riscv64/ap_early_entry.rs");
    let begin = src
        .find("ap_early_entry_start:")
        .expect("桩入口标号找不到（改名？本审计须同步）");
    // 右界＝raw string 收尾（r#"…"# 后的 ",\n);）；行内 # 注释先剥再判空。
    let end = src[begin..]
        .find("\"#,\n);")
        .map(|n| begin + n)
        .expect("桩 asm 串结束找不到（raw string 收尾漂移？）");
    src[begin..end]
        .lines()
        .filter_map(|line| {
            // raw string 形态：行即指令（无引号包裹）；行内 # 注释剥离。
            let ins = line.split('#').next()?.trim();
            if ins.is_empty()
                || ins.starts_with('.')
                || ins.starts_with("ap_early_entry_start:")
            {
                None
            } else {
                Some(ins.split_whitespace().collect::<Vec<_>>().join(" "))
            }
        })
        .collect()
}

#[test]
fn stub_order_fences_before_first_record_read() {
    let ins = stub_instructions();
    let fence = ins
        .iter()
        .position(|i| i == "fence rw, rw")
        .expect("消费侧栅栏找不到");
    let first_ld = ins
        .iter()
        .position(|i| i.starts_with("ld "))
        .expect("记录字段首读找不到");
    assert!(
        fence < first_ld,
        "fence 必须先于记录首读（§3.9：BSP 写→SBI 交接→AP 读的 happens-before）"
    );
    assert_eq!(
        ins[fence + 1], "la t2, AP_BOOTSTRAP_RECORD",
        "栅栏后第一拍＝PC 相对取记录位址（MMU off 下的物理可达性）"
    );
}

#[test]
fn stub_satp_uses_sv39_mode_and_fences_after() {
    let ins = stub_instructions();
    let ld_root = ins
        .iter()
        .position(|i| i == "ld t3, 16(t2)")
        .expect("page_table_root_pa 装载找不到（偏移 16 = ApBootstrap 契约）");
    let csrw = ins
        .iter()
        .position(|i| i == "csrw satp, t3")
        .expect("satp 写入找不到");
    assert!(ld_root < csrw, "根页必须先于 satp 装载");
    // Sv39 模式位（bit63）与 PPN 提取（>>12）必须都在写 satp 之前。
    let ppn = ins
        .iter()
        .position(|i| i == "srli t3, t3, 12")
        .expect("PPN 提取找不到");
    let mode = ins
        .iter()
        .position(|i| i.starts_with("li t6, 0x8000000000000000"))
        .expect("SATP_MODE_SV39 立值找不到");
    assert!(ppn < csrw && mode < csrw);
    assert_eq!(
        ins[csrw + 1], "sfence.vma",
        "satp 写后必须 sfence.vma（新根的取指/翻译一致性）"
    );
}

#[test]
fn stub_stack_and_jump_prepared_before_mmu_on() {
    let ins = stub_instructions();
    // 栈顶与汇聚点链接位址在 MMU on 之前从记录读入寄存器（MMU on 后记录
    // 仍可达，但读进寄存器消除了切换窗口内的记录依赖）。
    let ld_stack = ins
        .iter()
        .position(|i| i == "ld t4, 24(t2)")
        .expect("kernel_stack_top_va 装载找不到");
    let csrw = ins
        .iter()
        .position(|i| i == "csrw satp, t3")
        .expect("satp 写入找不到");
    assert!(ld_stack < csrw);
    assert_eq!(ins.last().map(String::as_str), Some("jr t5"), "桩以 jr 收尾");
}

#[test]
fn convergence_arrival_marker_is_declared() {
    let src = include_str!("../src/riscv64/ap_early_entry.rs");
    assert!(
        src.contains("pub static AP_ARRIVED:"),
        "汇聚点到达标记消失——P-A64RV-01 门判据（次级核到达可观测）失去观测面"
    );
    assert!(
        src.contains("pub unsafe extern \"C\" fn ap_early_entry(bootstrap_pa: usize) -> !"),
        "Rust 汇聚点签名漂移（a0=bootstrap_pa 契约）"
    );
}
