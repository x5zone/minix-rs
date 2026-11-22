//! ARM64 (aarch64) architecture-specific initialization
//!
//! Implements `ArchInit` for ARM64: CPACR_EL1.FPEN and the GIC
//! distributor seed. There is no PMU-enable branch: every producer leaves
//! `ArchMiscDesc.pmu_cycle_counter` false.
//!
//! C: arch_init() — earm/arch_system.c:101-132

use minix_platform::ArchMiscDesc;

use crate::arch_init::ArchInit;

/// ARM64 architecture-specific initialization.
///
/// Performs hardware-specific setup after protection structures and
/// interrupt controller are initialized:
///
/// 1. CPACR_EL1.FPEN
/// 2. Board-specific initialization (bsp_init)
///
/// C: arch_init() — earm/arch_system.c:101-132
pub struct AArch64ArchInit;

impl ArchInit for AArch64ArchInit {
    fn new(_desc: &ArchMiscDesc) -> Self {
        Self
    }

    fn init(&mut self) {
        // C: arch_init() — earm/arch_system.c:101-132

        // 0. CPACR_EL1.FPEN = 0b11, matching the direct-kernel chain. The
        // earlier decision assumed the kernel makes no EL1 FP use inside the
        // reset window where FPEN is 0b00; the direct-kernel chain breaks
        // that premise. After QEMU reset FPEN=00, and the NEON memset in the
        // precompiled core (build-std, still an open item) takes an undef on
        // the first 4-byte memset after kmain (observed on run smpd5:
        // elr=memset+0xe0, EC=0x7). The same code on the UEFI chain never
        // exposed this because AAVMF firmware leaves FPEN=0b11. This write is
        // equivalent (no new semantics: user-mode FP likewise does not trap;
        // wiring the lazy gate is still future work), and idempotent on the
        // UEFI chain.
        unsafe {
            let mut cpacr: u64;
            core::arch::asm!("mrs {}, cpacr_el1", out(reg) cpacr);
            cpacr |= 0x0030_0000; // FPEN = 0b11
            core::arch::asm!("msr cpacr_el1, {}", in(reg) cpacr);
        }

        // 1. Board-specific initialization
        // C: bsp_init()
        //
        // The SGI send path is a zero-sized trait impl and cannot hold a
        // reference to the interrupt-controller instance, so it keeps the
        // distributor base in a static. Seed that static from the same
        // `Gicv3Desc` the controller consumes; without this call the first
        // scheduler IPI reads/writes a null-derived MMIO address (live on
        // the aarch64 full image: faults at 0xF00 in the GICD_SGIR path).
        // The trait-object methods below resolve through the `dyn` types
        // returned by the globals; no trait import is needed.
        let controller = minix_platform::platform_desc().interrupt_controller();
        if let Some(gicv3) = controller
            .as_any()
            .downcast_ref::<minix_platform::arch::aarch64::Gicv3Desc>()
        {
            // SAFETY: boot is single-threaded here and no IPI sender exists
            // before `smp_init`.
            unsafe { crate::arm64::smp::set_gicd_base(gicv3.gicd_base) };
        }
    }
}
