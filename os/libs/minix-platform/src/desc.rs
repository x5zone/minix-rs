//! Platform descriptor trait and enum sub-descriptors.
//!
//! Defines the unified abstraction that all hardware discovery sources
//! (Device Tree, ACPI, QEMU fallback) implement. Upper-layer hardware
//! traits (`ClockArch`, `InterruptController`, `ArchInit`) consume these
//! descriptors via `new(desc) -> Self` constructors.

/// Maximum number of CPUs supported by the platform descriptor.
///
/// Used to size the fixed `CpuTopology.cpus` array (`no_std` — no heap).
/// Can be adjusted via feature flags if a target needs more.
pub const MAX_CPUS: usize = 64;

/// Platform hardware description — the unified abstraction.
///
/// One `PlatformDesc` instance answers: "what hardware am I running on?".
/// Upper layers (`ClockArch` / `InterruptController` / `ArchInit`) read
/// only this abstraction — they never touch raw FDT/ACPI bytes.
///
/// # Implementors
///
/// - [`crate::QemuVirtDesc`]: hardcoded QEMU `virt` values (fallback / tests).
/// - `DeviceTreeDesc` (Phase 3): parsed from FDT (ARM64 / RISC-V).
/// - `AcpiDesc` (Phase 4): parsed from ACPI RSDP (x86-64).
///
/// # Concurrency
///
/// All implementors must be `Send + Sync`: the descriptor is written once
/// at T2.5 (BKL held, single-threaded) and read by all CPUs thereafter.
pub trait PlatformDesc: Send + Sync + core::fmt::Debug {
    /// Interrupt controller descriptor (LAPIC+IOAPIC / GICv3 / PLIC).
    fn interrupt_controller(&self) -> InterruptControllerDesc;
    /// Timer descriptor (PIT / ARM Generic Timer / CLINT mtime).
    fn timer(&self) -> TimerDesc;
    /// Early console descriptor (optional — used before proper console init).
    fn early_console(&self) -> Option<ConsoleDesc>;
    /// CPU topology (core count, per-CPU info — SMP-ready, single-core for now).
    fn cpu_topology(&self) -> CpuTopology;
    /// Architecture miscellany (ACPI table pointer, PMU enable, ...).
    fn arch_misc(&self) -> ArchMiscDesc;
    /// Source identifier (for debugging / logging).
    fn source(&self) -> PlatformSource;
}

/// Where the platform description came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformSource {
    /// Parsed from a Flattened Device Tree blob.
    DeviceTree,
    /// Parsed from ACPI tables (RSDP → XSDT → MADT).
    Acpi,
    /// Hardcoded QEMU `virt` fallback.
    QemuVirt,
}

/// Interrupt controller descriptor.
///
/// Enum (not trait) because the set of interrupt controller types is
/// finite and known at compile time. Pattern matching gives compile-time
/// exhaustiveness; value type gives `Copy`/`Clone` for easy passing.
#[derive(Debug, Clone, Copy)]
pub enum InterruptControllerDesc {
    /// x86-64 LAPIC + IOAPIC.
    Apic {
        lapic_base: usize,
        ioapic_base: usize,
        nr_irqs: u32,
    },
    /// ARM64 GICv3.
    Gicv3 {
        gicd_base: usize,
        gicr_base: usize,
        /// Redistributor stride (per-CPU spacing; SMP-ready, single-core ignores).
        gicr_stride: usize,
        nr_irqs: u32,
    },
    /// RISC-V PLIC.
    Plic {
        plic_base: usize,
        nr_irqs: u32,
        /// S-mode context ID (hart 0 → context 1 in QEMU virt).
        context: u32,
    },
}

/// Timer descriptor.
#[derive(Debug, Clone, Copy)]
pub enum TimerDesc {
    /// x86-64 8254 PIT (boot) + LAPIC Timer (runtime).
    Pit {
        pit_base_freq: u32,
        lapic_base: usize,
    },
    /// ARM64 Generic Timer (frequency read from CNTFRQ_EL0 at runtime).
    ArmGenericTimer,
    /// RISC-V CLINT mtime.
    Clint {
        mtime_addr: usize,
        mtimecmp_base: usize,
        /// per-hart mtimecmp stride (SMP-ready; single-core uses base only).
        mtimecmp_stride: usize,
        freq: u64,
    },
}

/// Early console descriptor.
#[derive(Debug, Clone, Copy)]
pub enum ConsoleDesc {
    /// x86-64 COM1 etc. ISA serial (port I/O).
    IsaSerial { port_base: u16 },
    /// MMIO UART (ARM PL011 etc.).
    MmioSerial { mmio_base: usize },
    /// RISC-V SBI console (no MMIO — uses ecall).
    SbiConsole,
}

/// CPU topology (SMP-ready; `QemuVirtDesc` pre-populates 4 cores for test).
#[derive(Debug, Clone, Copy)]
pub struct CpuTopology {
    /// Total CPU/hart count.
    pub nr_cpus: u32,
    /// Hardware ID of the BSP (boot CPU).
    pub bsp_id: u32,
    /// Per-CPU info. Only `cpus[0..nr_cpus]` is valid.
    pub cpus: [CpuInfo; MAX_CPUS],
}

impl Default for CpuTopology {
    fn default() -> Self {
        Self {
            nr_cpus: 1,
            bsp_id: 0,
            cpus: [CpuInfo::default(); MAX_CPUS],
        }
    }
}

/// Per-CPU descriptor entry.
#[derive(Debug, Clone, Copy, Default)]
pub struct CpuInfo {
    /// Architecture-specific hardware ID:
    /// - x86-64: APIC ID
    /// - ARM64: MPIDR_EL1.Aff0
    /// - RISC-V: hart ID
    pub hw_id: u64,
    /// ARM64 GICv3: this CPU's Redistributor base
    /// (`gicr_base + hw_id * gicr_stride`).
    pub gicr_base: Option<usize>,
    /// RISC-V CLINT: this hart's mtimecmp address
    /// (`mtimecmp_base + hart_id * mtimecmp_stride`).
    pub mtimecmp_addr: Option<usize>,
}

/// Architecture miscellany consumed by `ArchInit`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ArchMiscDesc {
    /// x86-64: ACPI tables physical address (if not via RSDP path).
    pub acpi_tables: Option<usize>,
    /// ARM64: enable PMU cycle counter.
    pub pmu_cycle_counter: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cpu_topology_default_single_core() {
        let t = CpuTopology::default();
        assert_eq!(t.nr_cpus, 1);
        assert_eq!(t.bsp_id, 0);
        assert_eq!(t.cpus.len(), MAX_CPUS);
    }

    #[test]
    fn test_cpu_info_default() {
        let c = CpuInfo::default();
        assert_eq!(c.hw_id, 0);
        assert_eq!(c.gicr_base, None);
        assert_eq!(c.mtimecmp_addr, None);
    }

    #[test]
    fn test_platform_source_variants() {
        assert_ne!(PlatformSource::DeviceTree, PlatformSource::Acpi);
        assert_ne!(PlatformSource::Acpi, PlatformSource::QemuVirt);
        assert_ne!(PlatformSource::DeviceTree, PlatformSource::QemuVirt);
    }

    #[test]
    fn test_interrupt_controller_desc_variants() {
        let apic = InterruptControllerDesc::Apic {
            lapic_base: 0xFEE0_0000,
            ioapic_base: 0xFEC0_0000,
            nr_irqs: 64,
        };
        let gicv3 = InterruptControllerDesc::Gicv3 {
            gicd_base: 0x0800_0000,
            gicr_base: 0x080A_0000,
            gicr_stride: 0x2_0000,
            nr_irqs: 64,
        };
        let plic = InterruptControllerDesc::Plic {
            plic_base: 0x0C00_0000,
            nr_irqs: 64,
            context: 1,
        };
        // Verify pattern matching works
        match apic {
            InterruptControllerDesc::Apic { lapic_base, .. } => {
                assert_eq!(lapic_base, 0xFEE0_0000);
            }
            _ => panic!("expected Apic"),
        }
        match gicv3 {
            InterruptControllerDesc::Gicv3 { gicd_base, .. } => {
                assert_eq!(gicd_base, 0x0800_0000);
            }
            _ => panic!("expected Gicv3"),
        }
        match plic {
            InterruptControllerDesc::Plic { plic_base, .. } => {
                assert_eq!(plic_base, 0x0C00_0000);
            }
            _ => panic!("expected Plic"),
        }
    }

    #[test]
    fn test_timer_desc_variants() {
        let pit = TimerDesc::Pit {
            pit_base_freq: 1_193_182,
            lapic_base: 0xFEE0_0000,
        };
        let clint = TimerDesc::Clint {
            mtime_addr: 0x200_BFF8,
            mtimecmp_base: 0x200_4000,
            mtimecmp_stride: 8,
            freq: 10_000_000,
        };
        match pit {
            TimerDesc::Pit { pit_base_freq, .. } => assert_eq!(pit_base_freq, 1_193_182),
            _ => panic!("expected Pit"),
        }
        match clint {
            TimerDesc::Clint { mtime_addr, .. } => assert_eq!(mtime_addr, 0x200_BFF8),
            _ => panic!("expected Clint"),
        }
        // ArmGenericTimer carries no data
        assert!(matches!(TimerDesc::ArmGenericTimer, TimerDesc::ArmGenericTimer));
    }

    #[test]
    fn test_console_desc_variants() {
        let isa = ConsoleDesc::IsaSerial { port_base: 0x3F8 };
        let mmio = ConsoleDesc::MmioSerial { mmio_base: 0x0900_0000 };
        let sbi = ConsoleDesc::SbiConsole;
        match isa {
            ConsoleDesc::IsaSerial { port_base } => assert_eq!(port_base, 0x3F8),
            _ => panic!("expected IsaSerial"),
        }
        match mmio {
            ConsoleDesc::MmioSerial { mmio_base } => assert_eq!(mmio_base, 0x0900_0000),
            _ => panic!("expected MmioSerial"),
        }
        assert!(matches!(sbi, ConsoleDesc::SbiConsole));
    }
}
