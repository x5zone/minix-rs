//! ARM64 (aarch64) architecture-specific initialization
//!
//! Implements `ArchInit` for ARM64, performing PMU cycle counter
//! enablement and board-specific initialization.
//!
//! # Instance-based design (see 04-platform-discovery.md §3.4)
//!
//! `new(desc)` stores the PMU cycle counter enable flag from
//! `ArchMiscDesc` into an instance field.
//!
//! C: arch_init() — earm/arch_system.c:101-132

use minix_platform::ArchMiscDesc;

use crate::arch_init::ArchInit;

/// ARM64 architecture-specific initialization.
///
/// Performs hardware-specific setup after protection structures and
/// interrupt controller are initialized:
///
/// 1. Enable PMU cycle counter for user mode access
/// 2. Board-specific initialization (bsp_init)
///
/// C: arch_init() — earm/arch_system.c:101-132
pub struct AArch64ArchInit {
    /// Whether to enable the PMU cycle counter for user mode access.
    pmu_cycle_counter: bool,
}

impl ArchInit for AArch64ArchInit {
    fn new(desc: &ArchMiscDesc) -> Self {
        Self {
            pmu_cycle_counter: desc.pmu_cycle_counter,
        }
    }

    fn init(&mut self) {
        // C: arch_init() — earm/arch_system.c:101-132

        // 0. CPACR_EL1.FPEN = 0b11（§续-406：直核链等价化）。续-130 裁决
        // 的「复位 0b00 窗口内内核 -neon 无 EL1 FP 使用」前提被直核链
        // 打破——QEMU reset 后 FPEN=00，预编译 core 的 NEON memset 残差
        // （SD-23 build-std 待办）在 kmain 后首次 4 字节 memset 即 undef
        // （smpd5 实证 elr=memset+0xe0、EC=0x7）；UEFI 链同代码从未暴露
        // 是因 AAVMF 固件留下 FPEN=0b11。本写与其等价（零新语义：用户态
        // FP 同样不 trap，懒门接线仍按续-130 待未来），UEFI 链幂等。
        unsafe {
            let mut cpacr: u64;
            core::arch::asm!("mrs {}, cpacr_el1", out(reg) cpacr);
            cpacr |= 0x0030_0000; // FPEN = 0b11
            core::arch::asm!("msr cpacr_el1, {}", in(reg) cpacr);
        }

        // 1. Enable PMU cycle counter for user mode access
        // C: PMU_PMCR_E + PMU_PMCNTENSET_C + PMU_PMUSERENR_EN
        if self.pmu_cycle_counter {
            unsafe {
                // Enable PMU counter hardware and reset counters
                // PMCR: E (enable) + C (reset event counters) + P (reset cycle counter)
                let pmcr: u64 = 0x7; // E + C + P bits
                core::arch::asm!("msr pmcr_el0, {}", in(reg) pmcr);

                // Enable cycle counter (PMCCNTR_EL0)
                // PMCNTENSET: C bit (bit 31) enables cycle counter
                core::arch::asm!("msr pmcntenset_el0, {}", in(reg) 0x8000_0000u64);

                // Allow EL0 (user mode) access to cycle counter
                // PMUSERENR: EN bit (bit 0)
                core::arch::asm!("msr pmuserenr_el0, {}", in(reg) 0x1u64);
            }
        }

        // 2. Board-specific initialization
        // C: bsp_init()
        // Platform-specific setup (e.g., GIC base address) is now provided
        // via PlatformDesc → Gicv3Desc, consumed by
        // AArch64InterruptController::new(desc).
    }
}
