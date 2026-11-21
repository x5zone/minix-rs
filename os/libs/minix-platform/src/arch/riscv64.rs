//! RISC-V 64-bit platform descriptor brand-name structs and QEMU virt fallback.
//!
//! All RISC-V-specific hardware descriptor types live here. The upper layer
//! (`minix-boot`) sees only the `InterruptControllerDesc` / `TimerDesc`
//! traits; the concrete `PlicDesc` / `ClintDesc` types are visible only to
//! RISC-V consumer code.

use core::fmt;

use minix_boot::{
    ArchMiscDesc, CpuInfo, CpuTopology, InterruptControllerDesc, MAX_CPUS,
    PlatformDesc, PlatformSource, TimerDesc,
};

// ── Sub-descriptor structs (brand names visible only in this module) ──

/// RISC-V PLIC (Platform-Level Interrupt Controller) descriptor.
#[derive(Debug, Clone, Copy)]
pub struct PlicDesc {
    /// PLIC MMIO base address.
    pub plic_base: usize,
    /// Number of IRQ sources supported.
    pub nr_irqs: u32,
    /// S-mode context ID (hart 0 → context 1 in QEMU virt).
    pub context: u32,
}

impl InterruptControllerDesc for PlicDesc {
    fn nr_irqs(&self) -> u32 {
        self.nr_irqs
    }
    fn as_any(&self) -> &dyn core::any::Any {
        self
    }
}

/// RISC-V CLINT (Core Local Interruptor) mtime descriptor.
#[derive(Debug, Clone, Copy)]
pub struct ClintDesc {
    /// CLINT `mtime` register MMIO address (global counter).
    pub mtime_addr: usize,
    /// CLINT `mtimecmp` base address for hart 0.
    pub mtimecmp_base: usize,
    /// Per-hart `mtimecmp` spacing (SMP-ready; single-core uses base only).
    pub mtimecmp_stride: usize,
    /// `mtime` counter frequency (Hz).
    pub freq: u64,
}

impl TimerDesc for ClintDesc {
    fn frequency(&self) -> u64 {
        self.freq
    }
    fn as_any(&self) -> &dyn core::any::Any {
        self
    }
}

// ── QEMU `virt` fallback descriptor (riscv64) ──

/// QEMU `virt` machine descriptor — riscv64 hardcoded fallback.
///
/// Stores concrete sub-descriptors directly (zero indirection on the hot path).
#[derive(Debug)]
pub struct QemuVirtDesc {
    ic: PlicDesc,
    timer: ClintDesc,
    cpu_topology: CpuTopology,
    arch_misc: ArchMiscDesc,
}

impl QemuVirtDesc {
    /// Construct the hardcoded QEMU `virt` riscv64 descriptor.
    pub const fn new() -> Self {
        Self {
            ic: PlicDesc {
                plic_base: 0x0C00_0000,
                nr_irqs: 64,
                context: 1,
            },
            timer: ClintDesc {
                mtime_addr: 0x200_BFF8,
                mtimecmp_base: 0x200_4000,
                mtimecmp_stride: 8,
                freq: 10_000_000,
            },
            cpu_topology: CpuTopology {
                nr_cpus: 4,
                bsp_id: 0,
                cpus: [CpuInfo::zero(); MAX_CPUS],
            },
            arch_misc: ArchMiscDesc { acpi_tables: None },
        }
    }
}

impl Default for QemuVirtDesc {
    fn default() -> Self {
        let mut d = Self::new();
        for i in 0..d.cpu_topology.nr_cpus as usize {
            d.cpu_topology.cpus[i] = CpuInfo { hw_id: i as u64 };
        }
        d
    }
}

impl PlatformDesc for QemuVirtDesc {
    fn interrupt_controller(&self) -> &dyn InterruptControllerDesc {
        &self.ic
    }
    fn timer(&self) -> &dyn TimerDesc {
        &self.timer
    }
    fn cpu_topology(&self) -> CpuTopology {
        self.cpu_topology
    }
    fn arch_misc(&self) -> ArchMiscDesc {
        self.arch_misc
    }
    fn source(&self) -> PlatformSource {
        PlatformSource::QemuVirt
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_qemu_virt_source() {
        let d = QemuVirtDesc::default();
        assert_eq!(d.source(), PlatformSource::QemuVirt);
    }

    #[test]
    fn test_qemu_virt_riscv64_interrupt_controller() {
        let d = QemuVirtDesc::default();
        let ic = d.interrupt_controller();
        let any = ic.as_any();
        let plic = any.downcast_ref::<PlicDesc>().expect("expected PlicDesc");
        assert_eq!(plic.plic_base, 0x0C00_0000);
        assert_eq!(plic.nr_irqs, 64);
        assert_eq!(plic.context, 1);
    }

    #[test]
    fn test_qemu_virt_riscv64_timer() {
        let d = QemuVirtDesc::default();
        let timer = d.timer();
        let any = timer.as_any();
        let clint = any.downcast_ref::<ClintDesc>().expect("expected ClintDesc");
        assert_eq!(clint.mtime_addr, 0x200_BFF8);
        assert_eq!(clint.mtimecmp_base, 0x200_4000);
        assert_eq!(clint.freq, 10_000_000);
    }

    #[test]
    fn test_qemu_virt_riscv64_cpu_topology() {
        let d = QemuVirtDesc::default();
        let t = d.cpu_topology();
        assert_eq!(t.nr_cpus, 4);
    }
}

// ── Unit-test + boot-banner support (B-X: arch-dispatched, kernel stays cfg-free) ──

/// Hardcoded unit-test controller descriptor (QEMU virt PLIC defaults).
pub fn unit_test_irq_desc() -> PlicDesc {
    PlicDesc {
        plic_base: 0x0C00_0000,
        nr_irqs: 16,
        context: 1,
    }
}

/// Boot-banner labels (kmain_verify output, qemu_test builds).
pub const ARCH_NAME: &str = "riscv64";
pub const SP_LABEL: &str = "  SP (at entry):  ";
pub const PC_LABEL: &str = "  PC (at entry):  ";
pub const FP_LABEL: &str = "  FP (at entry):  ";
/// Per-arch debug banner printed at kmain_verify entry (riscv64 boot
/// brings this over from the old `#[cfg(target_arch)]` line — the other
/// arches emit nothing).
pub const REACHED_BANNER: &str = "kmain_verify: reached!\n";
