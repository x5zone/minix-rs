//! Host-side static audit of the deep-migration wrap-up batch: four groups of
//! small files (13 in-gate tests) covering fpu / protection / trap_entry /
//! plat interrupt.
//!
//! fpu: `AArch64FpuState` is 528B and 16-aligned (`FPSIMD_SIZE`, the arm64
//! FPSIMD context frame size — Q0-Q31 are never saved or restored yet, and
//! the frame size is the foundation for wiring that save path up later).
//! protection: the privilege-level ↔ `Privilege` mapping on both
//! architectures (EL1/S_MODE→Kernel, EL0/U_MODE→User).
//! trap_entry: existence of the `TrapEntryArch` trait bound on both
//! architectures (the original in-gate smoke tests have been trimmed to
//! bound assertions, pinned the same way on the host side).
//! plat interrupt: arm64 `TIMER_IRQ`=INTID 30 (CNTPNSIRQ, PPI 14+16 in the
//! QEMU virt device tree), the statistics-clock pseudo-vector 0 on both
//! architectures, and riscv64 PLIC `nr_irqs` clamped to `NR_IRQ_VECTORS`.

// ── fpu (arm64) ──

#[test]
fn arm64_fpu_state_geometry_is_frozen() {
    let src = include_str!("../src/arm64/fpu.rs");
    assert!(
        src.contains("const FPSIMD_SIZE: usize = 528;"),
        "FPSIMD_SIZE 漂移（528B＝arm64 FPSIMD 现场帧：31×16B Q 寄存器 + FPCR/FPSR + 头部）"
    );
    // In-gate assertions: size == FPSIMD_SIZE, alignment 16 (a Q-register
    // requirement), and all-zero default (fpsr/fpcr/regs all zero). Source
    // pins for those three points:
    assert!(src.contains("core::mem::size_of::<AArch64FpuState>(), FPSIMD_SIZE"));
    assert!(src.contains("core::mem::align_of::<AArch64FpuState>(), 16"));
}

// ── protection (privilege mapping on both architectures) ──

#[test]
fn protection_privilege_mapping_is_frozen() {
    let a64 = include_str!("../src/arm64/protection.rs");
    let rv = include_str!("../src/riscv64/protection.rs");
    // arm64: EL1→Kernel, EL0→User; riscv64: S_MODE→Kernel, U_MODE→User.
    for (src, kernel, user) in [
        (a64, "AArch64PrivilegeLevel::EL1", "AArch64PrivilegeLevel::EL0"),
        (rv, "Riscv64PrivilegeLevel::S_MODE", "Riscv64PrivilegeLevel::U_MODE"),
    ] {
        assert!(
            src.contains(&format!("to_privilege({kernel})")),
            "特权级→Privilege 映射漂移：{kernel} 腿"
        );
        assert!(
            src.contains(&format!("to_privilege({user})")),
            "特权级→Privilege 映射漂移：{user} 腿"
        );
        assert!(src.contains("Privilege::Kernel") && src.contains("Privilege::User"));
    }
}

// ── trap_entry (trait bound on both architectures) ──

#[test]
fn trap_entry_trait_bounds_are_frozen() {
    let a64 = include_str!("../src/arm64/trap_entry.rs");
    let rv = include_str!("../src/riscv64/trap_entry.rs");
    assert!(
        a64.contains("fn aarch64_trap_entry_arch_impl_satisfies_trait_bound"),
        "arm64 TrapEntryArch bound 断言消失"
    );
    assert!(
        rv.contains("fn riscv64_trap_entry_arch_impl_satisfies_trait_bound"),
        "riscv64 TrapEntryArch bound 断言消失"
    );
}

// ── plat interrupt (clock IRQ dispatch identity) ──

#[test]
fn arm64_timer_irq_is_ns_physical_timer_intid_30() {
    let src = include_str!("../../plat/src/arm64/interrupt.rs");
    // CNTPNSIRQ = INTID 30 (QEMU virt device tree PPI 14 + PPI base 16); 29
    // is the secure-state EL1 timer and 27 the virtual timer — one wrong
    // bit and the clock never arrives.
    assert!(
        src.contains("pub const TIMER_IRQ: IrqVector = IrqVector::new(30);"),
        "arm64 TIMER_IRQ 漂移（INTID 30 = CNTPNSIRQ）"
    );
    assert!(
        src.contains("pub const PROFILE_CLOCK_IRQ: IrqVector = IrqVector::new(0);"),
        "arm64 统计时钟 pseudo-vector 0 漂移（保留向量：核间不可达＝无控制器线）"
    );
}

#[test]
fn riscv64_timer_irq_is_reserved_pseudo_vector() {
    let src = include_str!("../../plat/src/riscv64/interrupt.rs");
    // The RISC-V S-mode timer is a CPU-local interrupt (sip.STIP) and does
    // not go through the PLIC — its dispatch identity is reserved
    // pseudo-vector 0 (an address that claim() cannot read).
    assert!(
        src.contains("pub const TIMER_IRQ: IrqVector = IrqVector::new(0);"),
        "riscv64 TIMER_IRQ 漂移（保留伪向量 0）"
    );
    assert!(
        src.contains(".min(NR_IRQ_VECTORS)"),
        "PLIC nr_irqs 钳位到 NR_IRQ_VECTORS 的形状漂移（越界 IRQ 号会踩向量表）"
    );
}
