//! Two-architecture page-table encoding bit pins (host-visible face of the 25
//! in-module paging tests for both architectures).
//!
//! The in-module paging tests are pure logic assertions (flag encoding, index
//! extraction); the reason they are unreachable on the host is the whole-module
//! architecture gate. This file pins the **load-bearing MMU hardware contracts**
//! among them as source text: a wrong bit position is invisible to a software
//! walk but fatal to the MMU (the arm64 descriptor type bits are the precedent
//! of a live-found bug; see the comment on the in-module
//! test_page_leaf_descriptor_type_bits). The pinning method = whole-line or
//! constant text matching (project discipline: prevent "an accidental substring
//! fooling the count").

// ── riscv64 (Sv39) ───────────────────────────────────────────────

#[test]
fn riscv64_pte_flag_bits_are_hardware_frozen() {
    let src = include_str!("../src/riscv64/paging.rs");
    // Sv39 PTE bit layout (Privileged Spec 4.3c): V/R/W/X/U/G/A/D = 0..7.
    for (lit, bit) in [
        ("const V = 1 << 0;", 0),
        ("const R = 1 << 1;", 1),
        ("const W = 1 << 2;", 2),
        ("const X = 1 << 3;", 3),
        ("const U = 1 << 4;", 4),
        ("const G = 1 << 5;", 5),
        ("const A = 1 << 6;", 6),
        ("const D = 1 << 7;", 7),
    ] {
        assert!(
            src.contains(lit),
            "Sv39 位定义漂移：找不到 `{lit}`（{bit} 号位）——PTE 编码是 MMU 硬件契约"
        );
    }
}

#[test]
fn riscv64_ppn_mask_and_shifts_are_frozen() {
    let walk = include_str!("../src/riscv64_walk.rs");
    // PPN payload bits [53:10] (Sv39, PA ≤ 56 bits); paddr→pte shifts right
    // by 2, pte→paddr shifts right by 10 then left by 12. The three-level
    // shifts are 30/21/12.
    assert!(
        walk.contains("pub const PTE_PPN_MASK: u64 = 0x003F_FFFF_FFFF_FC00;"),
        "PTE_PPN_MASK 漂移（Sv39 PPN 位 [53:10] 契约）"
    );
    assert!(
        walk.contains("(paddr >> 2) & PTE_PPN_MASK"),
        "paddr→pte 的 PPN 装载形状漂移"
    );
    assert!(
        walk.contains("((pte & PTE_PPN_MASK) >> 10) << 12"),
        "pte→paddr 的 PPN 卸载形状漂移"
    );
    assert!(walk.contains("pub const L2_SHIFT: u32 = 30;"));
    assert!(walk.contains("pub const L1_SHIFT: u32 = 21;"));
    assert!(walk.contains("pub const L0_SHIFT: u32 = 12;"));
}

#[test]
fn riscv64_nonleaf_is_rwx_zero_w_requires_r() {
    let src = include_str!("../src/riscv64/paging.rs");
    // The non-leaf decision and the W⊆R constraint are what the
    // free_child_tables/expand family discriminates on.
    assert!(
        src.contains("R=W=X=0 indicates a non-leaf (table pointer) entry"),
        "非叶判定的规格注释漂移"
    );
    assert!(
        src.contains("W=1 requires R=1 (RISC-V Privileged Spec §4.3.1)"),
        "W⊆R 约束注释漂移"
    );
}

// ── aarch64 (AArch64 VMSA) ──────────────────────────────────────

#[test]
fn arm64_pte_flag_bits_are_hardware_frozen() {
    let src = include_str!("../src/arm64/paging.rs");
    for lit in [
        "const VALID = 1 << 0;",
        "const TABLE = 1 << 1;",
        "const AP1   = 1 << 6;",
        "const AP2   = 1 << 7;",
        "const AF    = 1 << 10;",
        "const NG    = 1 << 11;",
        "const PXN   = 1 << 53;",
        "const XN    = 1 << 54;",
    ] {
        assert!(
            src.contains(lit),
            "AArch64 描述符位定义漂移：找不到 `{lit}`——PTE 编码是 MMU 硬件契约"
        );
    }
}

#[test]
fn arm64_leaf_vs_block_descriptor_type_bits_are_frozen() {
    let src = include_str!("../src/arm64/paging.rs");
    // 4 KiB page leaf = 0b11 (VALID|PAGE), block descriptor = 0b01: invisible
    // to a software walk, fatal to the MMU (the live-found bug behind the
    // in-module test_page_leaf_descriptor_type_bits).
    assert!(
        src.contains("const PAGE  = 1 << 1;"),
        "PAGE 位（叶描述符类型位的 L3 语义）漂移"
    );
    assert!(
        src.contains("flags_to_pte_page"),
        "页叶构造函数（0b11 腿）找不到——描述符类型位契约的承载者"
    );
}

#[test]
fn arm64_addr_mask_and_shifts_are_frozen() {
    let src = include_str!("../src/arm64/paging.rs");
    assert!(
        src.contains("const ADDR_MASK: u64 = 0x000F_FFFF_FFFF_F000;"),
        "ADDR_MASK（位 [47:12]，4 KiB 粒度）漂移"
    );
    for lit in [
        "const L0_SHIFT: u32 = 39;",
        "const L1_SHIFT: u32 = 30;",
        "const L2_SHIFT: u32 = 21;",
    ] {
        assert!(src.contains(lit), "索引移位漂移：{lit}");
    }
}

#[test]
fn arm64_aptable_polarity_and_intermediate_reclaim_are_frozen() {
    let src = include_str!("../src/arm64/paging.rs");
    // APTABLE polarity for user-walk grants (negative semantics: clear the bit
    // rather than set it).
    assert!(
        src.contains("const APTABLE: u64 = 1 << 60;"),
        "APTABLE 位（L0 表项的 APTable 字段高位）漂移"
    );
    // Intermediate page-table reclamation: the four-level cutoff is level>=3,
    // the discriminator bits are VALID+TABLE, and root pages are returned.
    // Pinning the text makes "whoever touched the reclamation contract" visible
    // on the host.
    assert!(
        src.contains("unsafe fn free_child_tables(&self, table_paddr: u64, level: u8)"),
        "free_child_tables（中间页表回收）被改名/删除——回收契约变动必须过 P-A64-01 评审"
    );
    assert!(
        src.contains("if level >= 3 {"),
        "回收深度收口必须是 level>=3（L0→L1→L2→L3；L3 描述符是数据页）"
    );
    assert!(
        src.contains("crate::pt_alloc::free_pt_page(minix_types::PhysBytes(child))"),
        "中间帧必须归还分配器（只解链不归还＝双重释放隐患，见 P-A64-01 红线）"
    );
}
