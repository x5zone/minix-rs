//! Shape pin test for the riscv64 AP early stub (the first increment of the
//! AP early-entry work).
//!
//! The load-bearing order of the stub's assembly: the fence (first-read
//! barrier) before the record read, the satp load (Sv39 mode bits) before
//! the sfence, and the stack plus convergence-point jump after the MMU is
//! on. Any order drift means the AP fetches instructions under a wrong
//! address translation (a silent hang or a triple fault).

fn stub_instructions() -> Vec<String> {
    let src = include_str!("../src/riscv64/ap_early_entry.rs");
    let begin = src
        .find("ap_early_entry_start:")
        .expect("桩入口标号找不到（改名？本审计须同步）");
    // Right bound = the raw-string closing (the ",\n); after r#"…"#); inline
    // # comments are stripped before the emptiness test.
    let end = src[begin..]
        .find("\"#,\n);")
        .map(|n| begin + n)
        .expect("桩 asm 串结束找不到（raw string 收尾漂移？）");
    src[begin..end]
        .lines()
        .filter_map(|line| {
            // Raw-string form: a line is an instruction (no surrounding
            // quotes); inline # comments are stripped.
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
    // The Sv39 mode bits (bit63) and the PPN extraction (>>12) must both
    // happen before satp is written.
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
    // The stack top and convergence-point link address are read from the
    // record into registers before the MMU is on (the record stays reachable
    // after MMU on, but loading it into registers removes the record
    // dependency inside the switch window).
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
