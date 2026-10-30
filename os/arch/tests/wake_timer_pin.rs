//! Host-shape pin test for the idle-wake timer restart (C
//! `context_stop_idle`'s `restart_local_timer` half).
//!
//! The three clock implementors and the trap dispatch arms are gated out of
//! the host build. This file freezes the load-bearing shape:
//!
//! 1. every architecture implements `ClockArch::restart_local_timer` with its
//!    own one-shot re-arm (x86 LVT unmask + ICR, arm64 CNTP re-arm + enable,
//!    riscv STIE + SBI re-arm);
//! 2. the kernel's `restart_local_timer` guards the BSP out (it keeps the
//!    PIT/auto-reloading source) and feeds the calibrated ICR;
//! 3. the interrupt entries call `context_stop_idle_current`, which clears
//!    `cpu_is_idle` and runs the restart.

fn x86_clock() -> &'static str {
    include_str!("../src/x86_64/clock.rs")
}

fn arm_clock() -> &'static str {
    include_str!("../src/arm64/clock.rs")
}

fn riscv_clock() -> &'static str {
    include_str!("../src/riscv64/clock.rs")
}

fn trap_src() -> &'static str {
    include_str!("../../kernel/src/trap_dispatch.rs")
}

fn kernel_lib_src() -> &'static str {
    include_str!("../../kernel/src/lib.rs")
}

fn kernel_clock_src() -> &'static str {
    include_str!("../../kernel/src/clock.rs")
}

#[test]
fn every_arch_implements_the_restart_half() {
    assert!(
        x86_clock().contains("fn restart_local_timer")
            && x86_clock().contains("LAPIC_TIMER_ICR_OFF"),
        "x86 restart must unmask LVT and reload the ICR"
    );
    assert!(
        arm_clock().contains("fn restart_local_timer")
            && arm_clock().contains("cntp_ctl_el0"),
        "arm64 restart must re-arm CNTP and re-enable the module gate"
    );
    assert!(
        riscv_clock().contains("fn restart_local_timer")
            && riscv_clock().contains("csrs sie"),
        "riscv restart must reopen STIE and reprogram the SBI deadline"
    );
}

#[test]
fn kernel_guards_the_bsp_and_uses_the_calibrated_icr() {
    let lib = kernel_lib_src();
    assert!(
        lib.contains("fn restart_local_timer")
            && lib.contains("bsp_cpu_id()")
            && lib.contains("local_tick_icr()"),
        "kernel restart must guard the BSP and feed the calibrated ICR"
    );
    assert!(
        kernel_clock_src().contains("fn local_tick_icr"),
        "the calibrated ICR accessor disappeared"
    );
}

#[test]
fn interrupt_entries_run_context_stop_idle() {
    let s = trap_src();
    let calls = s.matches("context_stop_idle_current()").count();
    assert!(
        calls >= 4,
        "expected the wake/timer entries to call context_stop_idle_current \
         (found {calls})"
    );
    assert!(
        kernel_lib_src().contains("fn context_stop_idle_current")
            && kernel_lib_src().contains("clear_idle_flag"),
        "context_stop_idle_current lost the clear/restart pairing"
    );
}
