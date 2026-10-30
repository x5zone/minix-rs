//! Host-shape pin test for the riscv64 AP scheduler-entry path.
//!
//! The riscv AP entry, the stack-based identity, the SSIE/STIE gates and the
//! zero-root PTE-walk guard are all gated out of the host build. This file
//! freezes their load-bearing shape with `include_str!`:
//!
//! 1. `ap_early_entry` installs a kernel-tail gate and snapshots the record
//!    (stack top included) before publishing `AP_ARRIVED`;
//! 2. the kernel identity is stack-based (`cpu_id_for_stack_pointer`) with
//!    the BSP slot fallback (`bsp_logical`) — riscv has no readable S-mode
//!    hart id, `tp` is restored on user return and `sscratch` carries the
//!    user sp;
//! 3. the AP tail opens `sie.SSIE`/`sie.STIE` and marks `ap_irq_ready`;
//! 4. a zero page-table root returns `None` from the walker instead of
//!    reading physical 0 through the Direct Map (the live AP fault).

fn ap_entry_src() -> &'static str {
    include_str!("../src/riscv64/ap_early_entry.rs")
}

fn paging_src() -> &'static str {
    include_str!("../src/riscv64/paging.rs")
}

fn arch_smp_src() -> &'static str {
    include_str!("../src/riscv64/smp.rs")
}

fn kernel_smp_src() -> &'static str {
    include_str!("../../kernel/src/smp.rs")
}

fn trap_src() -> &'static str {
    include_str!("../../kernel/src/trap_dispatch.rs")
}

fn sched_src() -> &'static str {
    include_str!("../../kernel/src/sched.rs")
}

fn kernel_lib_src() -> &'static str {
    include_str!("../../kernel/src/lib.rs")
}

#[test]
fn ap_entry_hands_off_to_the_kernel_tail() {
    let s = ap_entry_src();
    assert!(
        s.contains("register_ap_tail") && s.contains("AP_TAIL") && s.contains("ApTailFn"),
        "riscv AP convergence lost the kernel-tail handoff gate"
    );
    assert!(
        s.contains("kernel_stack_top_va") && s.contains("AP_ARRIVED"),
        "AP convergence must snapshot the record (stack top included) before publishing arrival"
    );
}

#[test]
fn identity_is_stack_based_with_the_bsp_slot_fallback() {
    let smp = kernel_smp_src();
    assert!(
        smp.contains("cpu_id_for_stack_pointer") && smp.contains("bsp_logical"),
        "riscv identity lost the stack-based lookup or the BSP fallback"
    );
    let lib = kernel_lib_src();
    assert!(
        lib.contains("cpu_id_for_stack_pointer") && lib.contains("bsp_logical"),
        "current_cpu_id no longer uses the riscv stack-based identity"
    );
}

#[test]
fn ap_tail_opens_ssie_and_stie_before_marking_ready() {
    let arch = arch_smp_src();
    assert!(
        arch.contains("enable_ap_interrupts") && arch.contains("(1u64 << 1) | (1u64 << 5)"),
        "riscv lost the SSIE/STIE gate opener"
    );
    let kernel = kernel_smp_src();
    assert!(
        kernel.contains("enable_ap_interrupts") && kernel.contains("mark_ap_irq_ready"),
        "riscv AP tail no longer opens the gates / latches the reading"
    );
}

#[test]
fn riscv_swi_arm_applies_scheduler_flags() {
    let s = trap_src();
    let start = s.find("fn riscv64_swi_arm").expect("riscv SWI arm missing");
    let end = s[start..]
        .find("riscv64 EI arm")
        .map(|i| start + i)
        .unwrap_or(s.len());
    let arm = &s[start..end];
    assert!(
        arm.contains("sched_handler_full") && arm.contains("preempt_current_on_ipi"),
        "riscv SWI arm must apply scheduler flags and preempt the current process"
    );
    assert!(
        arm.contains("bkl_unlock"),
        "riscv SWI arm must release the BKL it acquired"
    );
    assert!(
        !arm.contains("swi-ack"),
        "the diagnostic swi-ack print should be gone"
    );
}

#[test]
fn riscv_keeps_the_narrow_firmware_gate_clamp() {
    let s = sched_src();
    assert!(
        s.contains(r#"#[cfg(target_arch = "riscv64")]"#)
            && s.contains("params.cpu = params.cpu.map(|_| bsp)"),
        "riscv's narrow BSP clamp disappeared; the default firmware cannot \
         deliver a cross-hart SSIP, so AP-homed processes would starve"
    );
    assert!(
        s.contains("test-smp-ipi-riscv64")
            && s.contains("aclint=on")
            && s.contains("platform")
            && s.contains("gate"),
        "the narrow clamp lost its firmware-gate rationale"
    );
}

#[test]
fn zero_root_walk_is_not_mapped() {
    let s = paging_src();
    assert!(
        s.contains("if root_paddr.0 == 0") && s.contains("return None;"),
        "riscv PTE walk lost the zero-root guard (it must not read physical 0)"
    );
}
