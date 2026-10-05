//! 架构门内 `#[test]` 的清单冻结审计（P-ALL-01 方案乙：宿主可跑的逐架构
//! 源码静态审计）。
//!
//! `os/arch/src/{arm64,riscv64}` 与 `os/plat/src/{arm64,riscv64}` 的模块被
//! `#[cfg(target_arch = …)]` 整模块门住，宿主（x86_64）`cargo test` 不编译
//! 它们——门内 86 枚 `#[test]` 永不运行却显示全绿（本项目两次踩过的可达性
//! 纪律坑）。本审计把「不可见的未知集合」变成「冻结、漂移即红的已知集合」：
//!
//! - 门内测试**新增/删除/改名**（相对本文件冻结的清单）⇒ 本审计红，强制
//!   评审者同步审视该测试的宿主可达化路径（include_str! 形状钉住、常量
//!   迁移、或登记豁免理由）。
//! - 批量滚除门内测试也必须过这里——删测试不再是无声操作。
//!
//! 深度分级（哪些文件另有行为/形状级宿主审计）：`trap_return`/`trap_stub`
//! 双架构有专项钉住文件（riscv64_return_leg_pin / riscv64_trap_leg_shape /
//! arm64_return_leg_pin / arm64_trap_leg_shape），paging 双架构的编码位在
//! `paging_encoding_pins.rs`。其余文件以本清单审计为界。

/// 冻结清单（生成方式：`re.findall(r'#\[test\]\s*\n\s*fn (\w+)', src)`，
/// 按文件出现顺序）。更新规则：门内测试集合变动时**必须**同步本表并在
/// 提交说明里给出宿主可达化去向或豁免理由。
const FROZEN: &[(&str, &[&str])] = &[
    (
        "arch/src/arm64/boot.rs",
        &[
            "kernel_task_uses_init_task_psr",
            "user_process_uses_init_psr_and_fpen_user",
            "all_user_kinds_get_fpen_user",
            "default_ctx_is_zeroed",
            "inherit_fpu_state_propagates_fpu_enable_el0",
        ],
    ),
    (
        "arch/src/arm64/exception.rs",
        &[
            "exception_frame_size_is_documented",
            "is_user_mode_el0",
            "set_instruction_pointer_writes_elr_el1",
            "set_return_value_writes_x0",
        ],
    ),
    (
        "arch/src/arm64/fpu.rs",
        &[
            "test_fpu_state_size",
            "test_fpu_state_alignment",
            "test_fpu_state_default_is_zeroed",
        ],
    ),
    (
        "arch/src/arm64/paging.rs",
        &[
            "test_page_leaf_descriptor_type_bits",
            "test_flags_to_pte_kernel_read_write",
            "test_flags_to_pte_kernel_read_write_exec",
            "test_flags_to_pte_user_accessible",
            "test_flags_to_pte_read_only",
            "test_pte_to_flags_roundtrip_user_rw",
            "test_pte_to_flags_roundtrip_kernel_exec",
            "test_pte_to_flags_inverted_xn",
            "test_pte_to_flags_inverted_ap2",
            "test_l1_index_high_half",
            "test_addr_mask_preserves_physical",
            "split_huge_leaf_entry_arithmetic_is_base_plus_index_times_page",
            "grant_user_walk_clears_not_sets_aptable_polarity_is_negative",
        ],
    ),
    (
        "arch/src/arm64/protection.rs",
        &["privilege_level_roundtrip", "protection_has_cpu_count"],
    ),
    (
        "arch/src/arm64/signal.rs",
        &[
            "test_sigcontext_size_nonzero",
            "test_build_sigcontext_fills_from_ctx",
            "test_setup_handler_entry_arm64",
            "test_restore_sigcontext_arm64",
            "test_check_magic_arm64",
            "test_roundtrip_arm64",
        ],
    ),
    (
        "arch/src/arm64/trap_entry.rs",
        &["aarch64_trap_entry_arch_impl_satisfies_trait_bound"],
    ),
    (
        "arch/src/arm64/trap_return.rs",
        &["spsr_mask_bits_cover_aifd", "gp_regs_layout_matches_asm_strides"],
    ),
    (
        "arch/src/arm64/trap_stub.rs",
        &[
            "test_frame_layout_frozen",
            "test_spsr_m_reads_interrupted_el",
            "test_svc_entry_is_lower_el_sync_slot",
        ],
    ),
    (
        "arch/src/riscv64/boot.rs",
        &[
            "kernel_task_uses_init_task_sstatus",
            "user_process_uses_init_sstatus",
            "default_ctx_is_zeroed",
            "inherit_fpu_state_copies_sstatus",
        ],
    ),
    (
        "arch/src/riscv64/exception.rs",
        &[
            "exception_frame_size_is_documented",
            "is_user_mode_uses_spp_bit",
            "set_instruction_pointer_writes_sepc",
            "set_return_value_writes_a0",
        ],
    ),
    (
        "arch/src/riscv64/fpu.rs",
        &[
            "test_fpu_state_size",
            "test_fpu_state_alignment",
            "test_fpu_state_default_is_zeroed",
        ],
    ),
    (
        "arch/src/riscv64/paging.rs",
        &[
            "test_paddr_to_pte_roundtrip",
            "test_flags_to_pte_kernel_read_write_exec",
            "test_flags_to_pte_no_wx_combination",
            "test_pte_is_leaf",
            "test_pte_to_flags_roundtrip_user_rw",
            "test_pte_to_flags_roundtrip_kernel_exec",
            "test_pte_to_flags_roundtrip_read_only",
            "test_l0_index_4kb_page",
            "test_l0_index_within_range",
            "test_l2_index_dram_base",
            "test_l1_index_2mb_offset",
            "test_paddr_to_pte_dram_base",
        ],
    ),
    (
        "arch/src/riscv64/protection.rs",
        &["privilege_level_roundtrip", "protection_has_cpu_count"],
    ),
    (
        "arch/src/riscv64/signal.rs",
        &[
            "test_sigcontext_size_nonzero",
            "test_build_sigcontext_fills_from_ctx",
            "test_setup_handler_entry_riscv64",
            "test_restore_sigcontext_riscv64",
            "test_check_magic_riscv64",
            "test_roundtrip_riscv64",
        ],
    ),
    (
        "arch/src/riscv64/trap_entry.rs",
        &["riscv64_trap_entry_arch_impl_satisfies_trait_bound"],
    ),
    (
        "arch/src/riscv64/trap_return.rs",
        &["sstatus_bits_are_spp_and_spie", "gp_regs_layout_matches_asm_strides"],
    ),
    (
        "arch/src/riscv64/trap_stub.rs",
        &[
            "test_frame_layout_frozen",
            "test_spp_reads_interrupted_mode",
            "test_frame_slot_map_matches_gp_constants",
            "test_save_frame_to_context_persists_all_fields",
            "test_sync_status_register_pulls_a1_lane",
            "test_park_decision_values",
        ],
    ),
    (
        "plat/src/arm64/interrupt.rs",
        &[
            "test_new_from_gicv3_descriptor",
            "test_timer_irq_is_the_ns_physical_timer_ppi",
            "test_profile_clock_irq_is_unreachable_pseudo",
        ],
    ),
    (
        "plat/src/riscv64/interrupt.rs",
        &[
            "test_new_from_plic_descriptor",
            "test_new_clamps_nr_irqs_to_max",
            "test_timer_irq_is_the_reserved_pseudo_vector",
            "test_profile_clock_irq_is_unreachable_pseudo",
        ],
    ),
];

/// 与清单同口径的现场提取器（字面正则，工具可复算）。
fn gated_test_names(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut parts = src.split("#[test]").peekable();
    parts.next(); // 首段不含任何 #[test] 后内容
    while let Some(seg) = parts.next() {
        // #[test] 后的下一个 `fn NAME`
        if let Some(rest) = seg
            .lines()
            .map(str::trim)
            .skip_while(|l| l.starts_with("//") || l.is_empty())
            .find(|l| l.starts_with("fn "))
            .map(|l| l.to_string())
        {
            let name = rest
                .trim_start_matches("fn ")
                .split(|c: char| !(c.is_alphanumeric() || c == '_'))
                .next()
                .unwrap_or("")
                .to_string();
            out.push(name);
        }
        let _ = parts.peek();
    }
    out
}

fn read_src(rel: &str) -> String {
    // 本文件在 os/arch/tests/；清单键相对 os/。
    let path = format!("../{rel}");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("读 {path} 失败：{e}（清单键漂移？）"))
}

#[test]
fn gated_test_inventory_matches_frozen_manifest() {
    let mut failures = Vec::new();
    for (rel, frozen) in FROZEN {
        let live = gated_test_names(&read_src(rel));
        let frozen: Vec<String> = frozen.iter().map(|s| s.to_string()).collect();
        if live != frozen {
            let added: Vec<_> = live
                .iter()
                .filter(|n| !frozen.contains(n))
                .cloned()
                .collect();
            let removed: Vec<_> = frozen
                .iter()
                .filter(|n| !live.contains(n))
                .cloned()
                .collect();
            failures.push(format!(
                "{rel}: 新增 {added:?}，删除 {removed:?}——门内测试集合变动必须给出宿主可达化去向或豁免理由（P-ALL-01）"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "架构门内测试清单漂移：\n{}",
        failures.join("\n")
    );
}

#[test]
fn frozen_manifest_total_is_known() {
    // 总量哨兵：86 枚（2026-10-06 冻结）。有人门内加测试而忘改清单时，
    // 上一条按文件报差异；有人整文件重命名导致读取失败时上一条 panic；
    // 本条兜底「清单键数量」本身。
    let files: usize = FROZEN.len();
    let tests: usize = FROZEN.iter().map(|(_, v)| v.len()).sum();
    assert_eq!(files, 20, "受审计文件数");
    assert_eq!(tests, 86, "受审计门内测试总数（2026-10-06 冻结值）");
}
