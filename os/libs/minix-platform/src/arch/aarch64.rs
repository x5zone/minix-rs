//! ARM64 (aarch64) platform descriptor brand-name structs and QEMU virt fallback.
//!
//! All ARM64-specific hardware descriptor types live here. The upper layer
//! (`minix-boot`) sees only the `InterruptControllerDesc` / `TimerDesc` /
//! `ConsoleDesc` traits; the concrete `Gicv3Desc` / `ArmGenericTimerDesc` /
//! `MmioSerialDesc` types are visible only to ARM64 consumer code.

use minix_boot::{
    ArchMiscDesc, ConsoleDesc, CpuInfo, CpuTopology, InterruptControllerDesc, MAX_CPUS,
    PlatformDesc, PlatformSource, TimerDesc,
};

// ── Sub-descriptor structs (brand names visible only in this module) ──

/// ARM64 GICv3 descriptor (distributor + redistributor).
#[derive(Debug, Clone, Copy)]
pub struct Gicv3Desc {
    /// GICD (Distributor) MMIO base address.
    pub gicd_base: usize,
    /// GICR (Redistributor) base address for hart 0.
    pub gicr_base: usize,
    /// Redistributor stride (per-CPU spacing; SMP-ready).
    pub gicr_stride: usize,
    /// Number of IRQ sources supported.
    pub nr_irqs: u32,
}

impl InterruptControllerDesc for Gicv3Desc {
    fn nr_irqs(&self) -> u32 {
        self.nr_irqs
    }
    fn as_any(&self) -> &dyn core::any::Any {
        self
    }
}

/// ARM64 Generic Timer descriptor.
///
/// Frequency is read from `CNTFRQ_EL0` at runtime; the descriptor carries no
/// static frequency value. `frequency()` returns 0 to signal "read from
/// hardware register at runtime".
#[derive(Debug, Clone, Copy)]
pub struct ArmGenericTimerDesc;

impl TimerDesc for ArmGenericTimerDesc {
    fn frequency(&self) -> u64 {
        // 0 signals "read from CNTFRQ_EL0 at runtime".
        0
    }
    fn as_any(&self) -> &dyn core::any::Any {
        self
    }
}

/// ARM64 MMIO UART (PL011) descriptor.
#[derive(Debug, Clone, Copy)]
pub struct MmioSerialDesc {
    /// UART MMIO base address.
    pub mmio_base: usize,
}

impl ConsoleDesc for MmioSerialDesc {
    fn as_any(&self) -> &dyn core::any::Any {
        self
    }
}

// ── QEMU `virt` fallback descriptor (aarch64) ──

/// QEMU `virt` machine descriptor — aarch64 hardcoded fallback.
///
/// Stores concrete sub-descriptors directly (zero indirection on the hot path).
#[derive(Debug)]
pub struct QemuVirtDesc {
    ic: Gicv3Desc,
    timer: ArmGenericTimerDesc,
    console: MmioSerialDesc,
    cpu_topology: CpuTopology,
    arch_misc: ArchMiscDesc,
}

impl QemuVirtDesc {
    /// Construct the hardcoded QEMU `virt` aarch64 descriptor.
    pub const fn new() -> Self {
        Self {
            ic: Gicv3Desc {
                gicd_base: 0x0800_0000,
                gicr_base: 0x080A_0000,
                gicr_stride: 0x2_0000,
                nr_irqs: 64,
            },
            timer: ArmGenericTimerDesc,
            console: MmioSerialDesc { mmio_base: 0x0900_0000 },
            cpu_topology: CpuTopology {
                nr_cpus: 4,
                bsp_id: 0,
                cpus: [CpuInfo::zero(); MAX_CPUS],
            },
            arch_misc: ArchMiscDesc {
                acpi_tables: None,
                pmu_cycle_counter: false,
            },
        }
    }
}

impl Default for QemuVirtDesc {
    fn default() -> Self {
        // `cpus` needs runtime initialization for per-CPU gicr_base fields.
        let mut d = Self::new();
        const GICR_BASE: usize = 0x080A_0000;
        const GICR_STRIDE: usize = 0x2_0000;
        for i in 0..d.cpu_topology.nr_cpus as usize {
            d.cpu_topology.cpus[i] = CpuInfo {
                hw_id: i as u64,
                gicr_base: Some(GICR_BASE + i * GICR_STRIDE),
                mtimecmp_addr: None,
            };
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
    fn early_console(&self) -> Option<&dyn ConsoleDesc> {
        Some(&self.console)
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
    fn test_qemu_virt_aarch64_interrupt_controller() {
        let d = QemuVirtDesc::default();
        let ic = d.interrupt_controller();
        let any = ic.as_any();
        let gicv3 = any.downcast_ref::<Gicv3Desc>().expect("expected Gicv3Desc");
        assert_eq!(gicv3.gicd_base, 0x0800_0000);
        assert_eq!(gicv3.gicr_base, 0x080A_0000);
    }

    #[test]
    fn test_qemu_virt_aarch64_cpu_topology() {
        let d = QemuVirtDesc::default();
        let t = d.cpu_topology();
        assert_eq!(t.nr_cpus, 4);
        assert_eq!(t.cpus[0].gicr_base, Some(0x080A_0000));
        assert_eq!(t.cpus[1].gicr_base, Some(0x080C_0000));
        assert_eq!(t.cpus[3].gicr_base, Some(0x0810_0000));
    }
}
