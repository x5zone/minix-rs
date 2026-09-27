//! ARM64 (aarch64) GICv3 interrupt controller implementation
//!
//! Implements `InterruptController` for ARM64 using GICv3 (Generic Interrupt
//! Controller version 3).
//!
//! C: Minix3 ARM (32-bit) uses the OMAP INTC (`omap_intr.c`); the aarch64
//! target uses GICv3 — architectural evolution `[ARCH: K-1]`
//! (05-clock-interrupt-init.md §3.8).
//!
//! # Instance-based design (see 04-platform-discovery.md §3.4)
//!
//! Hardware base addresses (GICD, GICR) are stored in instance fields,
//! populated by `new(desc)` via `Any` downcast to `Gicv3Desc`. This
//! replaces the previous `new()` + `set_base()` two-step pattern.

use minix_platform::arch::aarch64::Gicv3Desc;

use crate::interrupt::{InterruptRouter, PerCpuInterruptUnit, IrqVector, NR_IRQ_VECTORS};

/// The boot clock source's IRQ vector: the EL1 non-secure physical timer
/// (CNTP — the bank that `CNTP_CTL_EL0` controls at EL1) is PPI INTID 30.
///
/// GIC PPI assignment: INTID 30 is CNTPNSIRQ (non-secure physical timer);
/// 29 is the *secure* EL1 timer (CNTPSIRQ) and 27 the virtual timer
/// (CNTVIRQ). QEMU virt assigns the non-secure timer device-tree PPI 14,
/// which lands on INTID 14 + 16 (PPI base) = 30 — matching the GIC
/// recommendation. Delivery is gated twice: the PPI enable bit lives in
/// `GICR_ISENABLER0` bit 30 (controller-side gate, unmasked by the
/// first-handler rule in `IrqManager`), and the timer module's own
/// `CNTP_CTL_EL0.Enable` (module-local gate, opened by
/// `TimerIrqGate::enable_timer_irq`).
pub const TIMER_IRQ: IrqVector = IrqVector::new(30);

/// The statistical-profiling clock's dispatch identity: pseudo-vector 0.
///
/// There is no dedicated profiling clock on aarch64 — the arch
/// implementation returns `ProfileClockError::Unsupported` (the eventual
/// source would be the PMU, which is not integrated), so no hook is ever
/// registered on this identity and no delivery path can produce it. It
/// exists so the kernel-side profile wiring compiles per-architecture the
/// same way `TIMER_IRQ` does.
pub const PROFILE_CLOCK_IRQ: IrqVector = IrqVector::new(0);

/// GICD_CTLR: Distributor Control Register.
const GICD_CTLR: usize = 0x0000;
/// GICD_CTLR.EnableGrp1NS bit.
const GICD_CTLR_ENABLE_GRP1NS: u32 = 0x2;

/// GICD_ISENABLER<n>: Interrupt Set-Enable Register.
const GICD_ISENABLER: usize = 0x0100;

/// GICD_ICENABLER<n>: Interrupt Clear-Enable Register.
const GICD_ICENABLER: usize = 0x0180;

/// GICD_IGROUPR<n>: Interrupt Group Register.
const GICD_IGROUPR: usize = 0x0080;

/// Byte offset of the redistributor's SGI/PPI frame (ARM IHI 0069 §5.3.9:
/// the RD frame 0x0000-0xFFFF carries CTLR/WAKER/…, the SGI/PPI frame at
/// 0x10000-0x1FFFF re-uses the GICD-style register offsets for INTIDs
/// 0-31). Every GICR_IGROUPR0/ISENABLER0/ICENABLER0 access below goes
/// through this offset — without it the writes land in reserved RD-frame
/// slots, which QEMU implements as write-ignored: the enable is silently
/// dropped and the line stays dead (live, first timer-irq carrier run).
const GICR_SGI_FRAME: usize = 0x1_0000;

/// GICR_IGROUPR0: Redistributor group register for SGI/PPI (INTID 0-31).
const GICR_IGROUPR0: usize = GICR_SGI_FRAME + 0x0080;

/// GICR_ISENABLER0: Redistributor Interrupt Set-Enable Register (SGI+PPI, INTID 0-31).
const GICR_ISENABLER0: usize = GICR_SGI_FRAME + 0x0100;

/// GICR_ICENABLER0: Redistributor Interrupt Clear-Enable Register (SGI+PPI, INTID 0-31).
const GICR_ICENABLER0: usize = GICR_SGI_FRAME + 0x0180;

/// GICR_WAKER: Redistributor Wake Register.
const GICR_WAKER: usize = 0x0014;
/// GICR_WAKER.ChildrenAsleep bit (read-only).
const GICR_WAKER_CHILDREN_ASLEEP: u32 = 0x4;

/// ARM64 GICv3 interrupt controller.
///
/// # Fields
///
/// - `gicd_base`: GIC Distributor MMIO base, from `Gicv3Desc`.
/// - `gicr_base`: GIC Redistributor MMIO base, from `Gicv3Desc`.
/// - `nr_irqs`: number of IRQ vectors (from descriptor, clamped to `NR_IRQ_VECTORS`).
pub struct AArch64InterruptController {
    /// GIC Distributor MMIO base address.
    gicd_base: usize,
    /// GIC Redistributor MMIO base address.
    gicr_base: usize,
    /// Number of IRQ vectors supported.
    nr_irqs: usize,
}

impl AArch64InterruptController {
    unsafe fn gicd_read32(&self, offset: usize) -> u32 {
        core::ptr::read_volatile((self.gicd_base + offset) as *const u32)
    }

    unsafe fn gicd_write32(&self, offset: usize, value: u32) {
        core::ptr::write_volatile((self.gicd_base + offset) as *mut u32, value);
    }

    unsafe fn gicr_read32(&self, offset: usize) -> u32 {
        core::ptr::read_volatile((self.gicr_base + offset) as *const u32)
    }

    unsafe fn gicr_write32(&self, offset: usize, value: u32) {
        core::ptr::write_volatile((self.gicr_base + offset) as *mut u32, value);
    }

    fn init_distributor(&mut self) {
        unsafe {
            for irq in (32..self.nr_irqs).step_by(32) {
                let reg = (irq / 32) as usize;
                self.gicd_write32(GICD_IGROUPR + reg * 4, 0xFFFF_FFFF);
            }
            for irq in (32..self.nr_irqs).step_by(32) {
                let reg = (irq / 32) as usize;
                self.gicd_write32(GICD_ICENABLER + reg * 4, 0xFFFF_FFFF);
            }
            // Enable Group 1 (NS) interrupts by setting only EnableGrp1NS.
            // A whole-register write would clear firmware-configured bits
            // such as ARE_NS/ARE_S (affinity routing, bits 31:30), which is
            // destructive on real hardware — read-modify-write instead.
            let ctlr = self.gicd_read32(GICD_CTLR) | GICD_CTLR_ENABLE_GRP1NS;
            self.gicd_write32(GICD_CTLR, ctlr);
        }
    }

    fn init_redistributor(&mut self) {
        unsafe {
            self.gicr_write32(GICR_WAKER, 0);
            while self.gicr_read32(GICR_WAKER) & GICR_WAKER_CHILDREN_ASLEEP != 0 {
                core::hint::spin_loop();
            }
            // Disable the banked SGI/PPI block (INTID 0-31) as part of
            // init: firmware may leave PPIs (e.g. the timer, INTID 30)
            // enabled. Each line is re-enabled per-consumer — the timer
            // via `IrqManager::register_hook`'s unmask, SGIs at IPI
            // bring-up (S-7/S-10). This is the redistributor half of the
            // "init masks everything" invariant (D-61; the SPI half is
            // GICD_ICENABLER in `init_distributor`).
            self.gicr_write32(GICR_ICENABLER0, 0xFFFF_FFFF);
            // Group all SGIs/PPIs into Group 1 (non-secure) to match the
            // distributor's IGROUPR handling of the SPIs above: reset
            // leaves GICR_IGROUPR0 = 0, which puts every PPI in Group 0 —
            // delivered as FIQ, not IRQ (live, first timer-irq carrier
            // run: the enabled timer PPI stayed silent because the IRQ
            // leg was listening and the FIQ mask stayed set). Group 1
            // delivery under ICC_IGRPEN1_EL1 is the routing this
            // controller's claim/complete half expects.
            self.gicr_write32(GICR_IGROUPR0, 0xFFFF_FFFF);
        }
    }

    fn init_cpu_interface(&mut self) {
        unsafe {
            let mut sre: u64;
            core::arch::asm!("mrs {}, icc_sre_el1", out(reg) sre);
            sre |= 0x7;
            core::arch::asm!("msr icc_sre_el1, {}", in(reg) sre);
            core::arch::asm!("msr icc_pmr_el1, {}", in(reg) 0xFFu64);
            core::arch::asm!("msr icc_igrpen1_el1, {}", in(reg) 0x1u64);
        }
    }
}

impl InterruptRouter for AArch64InterruptController {
    fn new(desc: &dyn minix_platform::InterruptControllerDesc) -> Self {
        let gicv3 = desc.as_any()
            .downcast_ref::<Gicv3Desc>()
            .expect("AArch64InterruptController::new: expected Gicv3Desc");
        // §1.114 review(SF1)：先判零再翻译——descriptor 缺 GICD/GICR base
        // （物理 `0`，ACPI/UEFI 表解析失败后结构体零初始化的最常见故障形态）
        // 时给精准诊断，避免被 `mmio_translate` 的越界 const-assert 抢先 panic
        // 成泛化的「address outside the mapped MMIO window」。
        assert!(
            gicv3.gicd_base != 0,
            "AArch64InterruptController::new: descriptor did not provide GICD base"
        );
        assert!(
            gicv3.gicr_base != 0,
            "AArch64InterruptController::new: descriptor did not provide GICR base"
        );
        // 路由到内核高半（TTBR1）MMIO 别名。descriptor 里的
        // `gicd_base`/`gicr_base` 是物理地址；本控制器在 `enable()` 之后才
        // 构造（bsp_finish_booting），高半窗口已装入 bootstrap 根的 TTBR1，
        // 因而 mask/unmask/init 的 MMIO 访问跨切根持久。若仍存物理地址，
        // 首次 `switch_address_space` + `tlbi alle1is` 后就会因 TTBR0 身份
        // 映射被切走而同步异常。
        Self {
            gicd_base: minix_platform::arch::aarch64::mmio_translate(gicv3.gicd_base as u64)
                as usize,
            gicr_base: minix_platform::arch::aarch64::mmio_translate(gicv3.gicr_base as u64)
                as usize,
            nr_irqs: (gicv3.nr_irqs as usize).min(NR_IRQ_VECTORS),
        }
    }

    fn init(&mut self) {
        // 零值/窗内合法性已在 `new()` 前置判定；此处仅 debug 层兜底。
        debug_assert!(self.gicd_base != 0, "AArch64InterruptController: gicd_base is zero");
        debug_assert!(self.gicr_base != 0, "AArch64InterruptController: gicr_base is zero");
        self.init_distributor();
        self.init_redistributor();
        self.init_cpu_interface();
    }

    fn mask(&mut self, irq: IrqVector) {
        let irq_num = irq.get() as usize;
        let bit = 1u32 << (irq_num % 32);
        if irq_num >= 32 {
            // SPI: use GICD_ICENABLER<n>
            let reg = (irq_num / 32) as usize;
            unsafe {
                self.gicd_write32(GICD_ICENABLER + reg * 4, bit);
            }
        } else {
            // SGI (0-15) / PPI (16-31): use GICR_ICENABLER0
            // GICv3 spec: GICR_ICENABLER0 bit N controls INTID N for this CPU.
            // SGI masking is technically supported but rarely used — the
            // register layout is the same as for PPI.
            unsafe {
                self.gicr_write32(GICR_ICENABLER0, bit);
            }
        }
    }

    fn unmask(&mut self, irq: IrqVector) {
        let irq_num = irq.get() as usize;
        let bit = 1u32 << (irq_num % 32);
        if irq_num >= 32 {
            // SPI: use GICD_ISENABLER<n>
            let reg = (irq_num / 32) as usize;
            unsafe {
                self.gicd_write32(GICD_ISENABLER + reg * 4, bit);
            }
        } else {
            // SGI (0-15) / PPI (16-31): use GICR_ISENABLER0
            // GICv3 spec: GICR_ISENABLER0 bit N controls INTID N for this CPU.
            // SGI unmasking is technically supported but rarely used — the
            // register layout is the same as for PPI.
            unsafe {
                self.gicr_write32(GICR_ISENABLER0, bit);
            }
        }
    }


    fn mask_all(&mut self) {
        for irq in (32..self.nr_irqs).step_by(32) {
            let reg = (irq / 32) as usize;
            unsafe {
                self.gicd_write32(GICD_ICENABLER + reg * 4, 0xFFFF_FFFF);
            }
        }
        // SGI (0-15) + PPI (16-31) live in the banked GICR_ICENABLER0 —
        // GICD writes never reach them. Disabling them here (D-61) is what
        // makes "init leaves every line masked" hold: PPIs such as the
        // timer (INTID 30) otherwise keep their firmware-enabled state and
        // can fire with no handler registered. The timer PPI is unmasked
        // deliberately later, by `IrqManager::register_hook` under
        // `minix_plat::TIMER_IRQ` (bsp_finish_booting Step 6).
        unsafe {
            self.gicr_write32(GICR_ICENABLER0, 0xFFFF_FFFF);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_from_gicv3_descriptor() {
        let desc = Gicv3Desc {
            gicd_base: 0x0800_0000,
            gicr_base: 0x080A_0000,
            gicr_stride: 0x2_0000,
            nr_irqs: 64,
        };
        let ic = AArch64InterruptController::new(&desc);
        // §1.114：new() 将物理基址路由到高半（TTBR1）MMIO 别名，跨切根持久。
        // 钉字面量常量（非 `mmio_translate` 自身表达式）：若 `MMIO_VA_BASE`
        // 被误改到与内核镜像/DM 冲突的 L0 slot，本测试应能捕获。
        assert_eq!(ic.gicd_base, 0xFFFF_C000_0000_0000_usize);
        assert_eq!(ic.gicr_base, (0xFFFF_C000_0000_0000 + 0x000A_0000) as usize);
    }

    // 注：本文件原 3 个测试中 2 个是常量自指/平凡表达式，删除 (Pattern #38)：
    // - `test_gic_register_offsets`：6 个 GICD/GICR 偏移常量与字面量 self-reference。
    //   真值由 arm ARM / GICv3 spec 保证，编译期 constant eval 已确认正确。
    //   运行时正确性（GIC 寄存器读写）由 QEMU 集成测试验证。
    // - `test_ppi_bit_calculation`：测的是 `1u32 << (N % 32)` 这条 Rust 表达式
    //   语义，不是项目代码。无价值。
    //
    // 保留 `test_new_from_gicv3_descriptor`（验证 new() 正确填充字段）。

    #[test]
    fn test_timer_irq_is_the_ns_physical_timer_ppi() {
        // D-59: the boot clock source (CNTP at EL1, non-secure) is PPI
        // INTID 30. This pin carries the GIC PPI assignment fact — if the
        // constant is ever changed, the change must cite the timer bank
        // actually used by `ArmGenericTimerDesc` / `CNTP_CTL_EL0`, not
        // just a different number. It also documents that 30 < 32, so the
        // controller-side gate goes through GICR (PPI path), not GICD.
        assert_eq!(TIMER_IRQ.get(), 30);
        assert!(TIMER_IRQ.get() < 32);
    }

    #[test]
    fn test_profile_clock_irq_is_unreachable_pseudo() {
        // init_profile_clock on aarch64 reports Unsupported (no PMU yet),
        // so no hook is ever registered on this identity and the trap
        // entry's line check can never match a delivered interrupt.
        assert_eq!(PROFILE_CLOCK_IRQ.get(), 0);
    }
}

impl PerCpuInterruptUnit for AArch64InterruptController {
    fn claim(&mut self) -> Option<u32> {
        let iar: u64;
        // SAFETY: GIC system-register read on the interrupting CPU; the
        // read itself acknowledges and yields the INTID.
        unsafe {
            core::arch::asm!("mrs {}, icc_iar1_el1", out(reg) iar);
        }
        let intid = (iar as u32) & 0x00FF_FFFF;
        // GIC spurious INTID (1023): nothing claimable — handling and
        // completion are both skipped (K8 pairing).
        (intid != 1023).then_some(intid)
    }

    fn complete(&mut self, claimed: Option<u32>) {
        let Some(intid) = claimed else {
            return; // spurious claim: nothing to complete
        };
        // SAFETY: write back the claimed INTID — the completion register
        // expects exactly the identity the claim read returned (D-61).
        unsafe {
            core::arch::asm!("msr icc_eoir1_el1, {}", in(reg) intid as u64);
        }
    }
}
