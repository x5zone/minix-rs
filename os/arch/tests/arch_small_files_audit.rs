//! P-ALL-01 深度迁移收尾批：fpu / protection / trap_entry / plat interrupt
//! 四组小文件（13 枚门内测试）的宿主静态审计。
//!
//! fpu：`AArch64FpuState` 528B/16 对齐（`FPSIMD_SIZE`，arm64 FPSIMD 现场帧
//! 尺寸——`SD-23` 记 Q0-Q31 从不存恢复，帧尺寸是未来接通保存路径的地基）。
//! protection：双架构特权级 ↔ `Privilege` 映射（EL1/S_MODE→Kernel、
//! EL0/U_MODE→User）。
//! trap_entry：双架构 `TrapEntryArch` trait bound 存在性（门内原烟雾测试
//! 已精简为 bound 断言，宿主侧同形钉住）。
//! plat interrupt：arm64 `TIMER_IRQ`=INTID 30（CNTPNSIRQ，QEMU virt 设备树
//! PPI 14+16）、双架构统计时钟 pseudo-vector 0、riscv64 PLIC `nr_irqs`
//! 钳位到 `NR_IRQ_VECTORS`。

// ── fpu（arm64）──

#[test]
fn arm64_fpu_state_geometry_is_frozen() {
    let src = include_str!("../src/arm64/fpu.rs");
    assert!(
        src.contains("const FPSIMD_SIZE: usize = 528;"),
        "FPSIMD_SIZE 漂移（528B＝arm64 FPSIMD 现场帧：31×16B Q 寄存器 + FPCR/FPSR + 头部）"
    );
    // 门内断言：尺寸==FPSIMD_SIZE、对齐 16（Q 寄存器要求）、默认全零
    //（fpsr/fpcr/reg 皆零）。三点的源码锚：
    assert!(src.contains("core::mem::size_of::<AArch64FpuState>(), FPSIMD_SIZE"));
    assert!(src.contains("core::mem::align_of::<AArch64FpuState>(), 16"));
}

// ── protection（双架构特权映射）──

#[test]
fn protection_privilege_mapping_is_frozen() {
    let a64 = include_str!("../src/arm64/protection.rs");
    let rv = include_str!("../src/riscv64/protection.rs");
    // arm64：EL1→Kernel、EL0→User；riscv64：S_MODE→Kernel、U_MODE→User。
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

// ── trap_entry（双架构 trait bound）──

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

// ── plat interrupt（时钟 IRQ 派发身份）──

#[test]
fn arm64_timer_irq_is_ns_physical_timer_intid_30() {
    let src = include_str!("../../plat/src/arm64/interrupt.rs");
    // CNTPNSIRQ＝INTID 30（QEMU virt 设备树 PPI 14 + PPI 基 16）；29 是
    // 安全态 EL1 定时器、27 是虚拟定时器——错一位即时钟永不到达。
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
    // riscv 的 S 态定时器是 CPU 本地中断（sip.STIP），不经 PLIC——派发
    // 身份＝保留伪向量 0（§续-376：claim() 读不到的地址）。
    assert!(
        src.contains("pub const TIMER_IRQ: IrqVector = IrqVector::new(0);"),
        "riscv64 TIMER_IRQ 漂移（保留伪向量 0）"
    );
    assert!(
        src.contains(".min(NR_IRQ_VECTORS)"),
        "PLIC nr_irqs 钳位到 NR_IRQ_VECTORS 的形状漂移（越界 IRQ 号会踩向量表）"
    );
}
