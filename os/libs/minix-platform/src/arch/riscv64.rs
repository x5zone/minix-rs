//! RISC-V 64-bit platform descriptor brand-name structs and QEMU virt fallback.
//!
//! All RISC-V-specific hardware descriptor types live here. The upper layer
//! (`minix-boot`) sees only the `InterruptControllerDesc` / `TimerDesc` /
//! `ConsoleDesc` traits; the concrete `PlicDesc` / `ClintDesc` /
//! `Riscv64ConsoleDesc` types are visible only to RISC-V consumer code.

use core::fmt;

use minix_boot::{
    ArchMiscDesc, ConsoleDesc, CpuInfo, CpuTopology, InterruptControllerDesc, MAX_CPUS,
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

/// RISC-V console descriptor.
///
/// RISC-V QEMU virt may use either SBI ecall console (default, no MMIO) or
/// an MMIO UART (ns16550a / SiFive UART). This enum holds the runtime-detected
/// console kind. Brand names (`Sbi`, `Mmio`) are confined to this arch module.
#[derive(Debug, Clone, Copy)]
pub enum Riscv64ConsoleKind {
    /// SBI ecall console (no MMIO — uses SBI console putchar/getchar calls).
    Sbi,
    /// MMIO UART (ns16550a or SiFive UART).
    Mmio { mmio_base: usize },
}

/// RISC-V console descriptor — wraps the runtime-detected console kind.
#[derive(Debug, Clone, Copy)]
pub struct Riscv64ConsoleDesc {
    kind: Riscv64ConsoleKind,
}

impl Riscv64ConsoleDesc {
    /// Construct an SBI console descriptor.
    pub const fn sbi() -> Self {
        Self {
            kind: Riscv64ConsoleKind::Sbi,
        }
    }
    /// Construct an MMIO UART console descriptor.
    pub const fn mmio(mmio_base: usize) -> Self {
        Self {
            kind: Riscv64ConsoleKind::Mmio { mmio_base },
        }
    }
    /// Access the console kind (for arch-layer downcast consumers).
    pub fn kind(&self) -> Riscv64ConsoleKind {
        self.kind
    }
}

impl ConsoleDesc for Riscv64ConsoleDesc {
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
    console: Riscv64ConsoleDesc,
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
            console: Riscv64ConsoleDesc::sbi(),
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
        // `cpus` needs runtime initialization for per-CPU mtimecmp_addr fields.
        let mut d = Self::new();
        const MTIMECMP_BASE: usize = 0x200_4000;
        const MTIMECMP_STRIDE: usize = 8;
        for i in 0..d.cpu_topology.nr_cpus as usize {
            d.cpu_topology.cpus[i] = CpuInfo {
                hw_id: i as u64,
                gicr_base: None,
                mtimecmp_addr: Some(MTIMECMP_BASE + i * MTIMECMP_STRIDE),
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

// Suppress unused warning for `fmt` import when no fmt impl is needed.
#[allow(unused_imports)]
use fmt as _;

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
        assert_eq!(t.cpus[0].mtimecmp_addr, Some(0x200_4000));
        assert_eq!(t.cpus[1].mtimecmp_addr, Some(0x200_4008));
        assert_eq!(t.cpus[3].mtimecmp_addr, Some(0x200_4018));
    }
}
