//! x86-64 platform descriptor brand-name structs and QEMU virt fallback.
//!
//! All x86-64-specific hardware descriptor types live here. The upper layer
//! (`minix-boot`) sees only the `InterruptControllerDesc` / `TimerDesc` /
//! `ConsoleDesc` traits; the concrete `ApicDesc` / `PitDesc` / `IsaSerialDesc`
//! types are visible only to x86-64 consumer code (which downcasts via `Any`).

use minix_boot::{
    ArchMiscDesc, ConsoleDesc, CpuInfo, CpuTopology, InterruptControllerDesc, MAX_CPUS,
    PlatformDesc, PlatformSource, TimerDesc,
};

// ── Sub-descriptor structs (brand names visible only in this module) ──

/// x86-64 LAPIC + IOAPIC descriptor.
///
/// Fields are public so that x86-64 consumer code
/// (`X86_64InterruptController::new`) can downcast via `Any` and read them.
#[derive(Debug, Clone, Copy)]
pub struct ApicDesc {
    /// LAPIC MMIO base address (default `0xFEE0_0000`).
    pub lapic_base: usize,
    /// IOAPIC MMIO base address (default `0xFEC0_0000`).
    pub ioapic_base: usize,
    /// Number of IRQ sources supported.
    pub nr_irqs: u32,
}

impl InterruptControllerDesc for ApicDesc {
    fn nr_irqs(&self) -> u32 {
        self.nr_irqs
    }
    fn as_any(&self) -> &dyn core::any::Any {
        self
    }
}

/// x86-64 8254 PIT + LAPIC Timer descriptor.
#[derive(Debug, Clone, Copy)]
pub struct PitDesc {
    /// 8254 PIT base frequency (Hz) — fixed hardware constant 1_193_182.
    pub pit_base_freq: u32,
    /// LAPIC MMIO base (for LAPIC Timer setup after APIC init).
    pub lapic_base: usize,
}

impl TimerDesc for PitDesc {
    fn frequency(&self) -> u64 {
        self.pit_base_freq as u64
    }
    fn as_any(&self) -> &dyn core::any::Any {
        self
    }
}

/// x86-64 ISA serial port (COM1) descriptor.
#[derive(Debug, Clone, Copy)]
pub struct IsaSerialDesc {
    /// I/O port base (default `0x3F8` for COM1).
    pub port_base: u16,
}

impl ConsoleDesc for IsaSerialDesc {
    fn as_any(&self) -> &dyn core::any::Any {
        self
    }
}

// ── QEMU `virt` fallback descriptor (x86-64) ──

/// QEMU `virt` machine descriptor — x86-64 hardcoded fallback.
///
/// Used when `KernelInfo.platform_sources` is empty (boot-shim did not
/// provide an RSDP pointer). In dev builds this is a warn-and-fallback;
/// in release builds it panics (see `global::init_from_kinfo`).
///
/// Stores concrete sub-descriptors directly (zero indirection on the hot path).
#[derive(Debug)]
pub struct QemuVirtDesc {
    ic: ApicDesc,
    timer: PitDesc,
    console: IsaSerialDesc,
    cpu_topology: CpuTopology,
    arch_misc: ArchMiscDesc,
}

impl QemuVirtDesc {
    /// Construct the hardcoded QEMU `virt` x86-64 descriptor.
    pub const fn new() -> Self {
        Self {
            ic: ApicDesc {
                lapic_base: 0xFEE0_0000,
                ioapic_base: 0xFEC0_0000,
                nr_irqs: 64,
            },
            timer: PitDesc {
                pit_base_freq: 1_193_182,
                lapic_base: 0xFEE0_0000,
            },
            console: IsaSerialDesc { port_base: 0x3F8 },
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
        // `cpus` needs runtime initialization for per-CPU hw_id fields.
        let mut d = Self::new();
        // QEMU q35 with `-smp 4` assigns APIC IDs 0..3 by default.
        for i in 0..d.cpu_topology.nr_cpus as usize {
            d.cpu_topology.cpus[i] = CpuInfo {
                hw_id: i as u64,
                gicr_base: None,
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
    fn test_qemu_virt_cpu_topology_four_cores() {
        let d = QemuVirtDesc::default();
        let t = d.cpu_topology();
        assert_eq!(t.nr_cpus, 4, "QEMU virt test config uses 4 CPUs (-smp 4)");
        assert_eq!(t.bsp_id, 0);
        for i in 0..t.nr_cpus as usize {
            assert_eq!(t.cpus[i].hw_id, i as u64);
        }
    }

    #[test]
    fn test_qemu_virt_x86_64_interrupt_controller() {
        let d = QemuVirtDesc::default();
        // Downcast via Any to verify the concrete type.
        let ic = d.interrupt_controller();
        let any = ic.as_any();
        let apic = any.downcast_ref::<ApicDesc>().expect("expected ApicDesc");
        assert_eq!(apic.lapic_base, 0xFEE0_0000);
        assert_eq!(apic.ioapic_base, 0xFEC0_0000);
        assert_eq!(apic.nr_irqs, 64);
    }

    #[test]
    fn test_qemu_virt_x86_64_timer() {
        let d = QemuVirtDesc::default();
        let timer = d.timer();
        let any = timer.as_any();
        let pit = any.downcast_ref::<PitDesc>().expect("expected PitDesc");
        assert_eq!(pit.pit_base_freq, 1_193_182);
        assert_eq!(pit.lapic_base, 0xFEE0_0000);
    }
}
