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

// ── High-half device MMIO window (§1.114: cross-root-persistent MMIO) ──────
//
// aarch64 has no port I/O: the PL011 serial console and the GICv3
// distributor/redistributor are MMIO-only. During boot those devices are
// reached through the identity mapping (`VA == PA`, low half). That identity
// mapping lives in the TTBR0 half of the bootstrap page table, which
// `switch_address_space` replaces with a per-process root — and the switch
// issues `tlbi alle1is`, flushing the cached identity entry. Any kernel MMIO
// access through the low-half identity VA after the first switch therefore
// faults (NK4-C §1.114: the boot dies on the very first `set_active_root`).
//
// Fix: map the device MMIO range once into the kernel high half (TTBR1)
// of the bootstrap root. TTBR1 is pinned to the bootstrap root and never
// switched per-process, so a high-half alias of the window stays reachable
// for the entire kernel lifetime. The constants below define that window;
// every runtime device base must be routed through [`mmio_translate`].

/// Start of the physical MMIO range the aarch64 kernel touches at runtime:
/// GICD (`0x0800_0000`) through the PL011 UART (`0x0900_0000`) on QEMU virt.
pub const MMIO_PA_BASE: u64 = 0x0800_0000;
/// Length of the MMIO window (32 MiB — covers GICD, every GICR redistributor
/// frame for the supported CPUs, and the PL011 UART with headroom).
pub const MMIO_WINDOW_LEN: u64 = 0x0200_0000;
/// Virtual base the window is mapped to in the kernel (TTBR1) half. Placed
/// far above the kernel image (`0xFFFF_8000_0000_0000` = L0 slot 256) and the
/// kernel direct map (`0xFFFF_8080_0000_0000` = L0 slot 257) so it never
/// shares an L0 slot with either; still inside the TTBR1 range (≥ `0xFFFF_0000_0000_0000`).
pub const MMIO_VA_BASE: u64 = 0xFFFF_C000_0000_0000;

/// Translate a physical MMIO address inside the window to its kernel
/// high-half (TTBR1) virtual alias.
///
/// Panics (const-eval / runtime) if `pa` falls outside the mapped window —
/// that would be a device the kernel touches at runtime without a
/// cross-root mapping, i.e. the §1.114 bug class reintroduced.
pub const fn mmio_translate(pa: u64) -> u64 {
    assert!(
        pa >= MMIO_PA_BASE && pa - MMIO_PA_BASE < MMIO_WINDOW_LEN,
        "mmio_translate: address outside the mapped MMIO window"
    );
    MMIO_VA_BASE + (pa - MMIO_PA_BASE)
}

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

// ── Unit-test + boot-banner support (B-X: arch-dispatched, kernel stays cfg-free) ──

/// Hardcoded unit-test controller descriptor (QEMU virt GICv3 defaults).
pub fn unit_test_irq_desc() -> Gicv3Desc {
    Gicv3Desc {
        gicd_base: 0x0800_0000,
        gicr_base: 0x080A_0000,
        gicr_stride: 0x1_0000,
        nr_irqs: 16,
    }
}

/// Boot-banner labels (kmain_verify output, qemu_test builds).
pub const ARCH_NAME: &str = "aarch64";
pub const SP_LABEL: &str = "  SP (at entry):  ";
pub const PC_LABEL: &str = "  PC (at entry):  ";
pub const FP_LABEL: &str = "  FP (at entry):  ";
/// Per-arch debug banner printed at kmain_verify entry.
pub const REACHED_BANNER: &str = "";
