//! QEMU `virt` machine hardcoded fallback descriptor.
//!
//! Returns the fixed hardware layout of QEMU's `virt` machine for each
//! architecture. This keeps existing QEMU tests passing without requiring
//! a DTB/ACPI parser.
//!
//! # `#[cfg(target_arch)]` usage note
//!
//! The `#[cfg]` attributes here select **data** (which constant to return),
//! not **behavior**. This is permitted: each branch only returns a constant
//! descriptor value. Real hardware paths use `DeviceTreeDesc`/`AcpiDesc`
//! which contain no `#[cfg]`. See `plat-design.md` §5.2.

use crate::desc::*;

/// QEMU `virt` machine descriptor — hardcoded fallback.
///
/// Used when `KernelInfo.platform_descriptor` is `None` (boot-shim did not
/// provide a DTB/RSDP pointer). In dev builds this is a warn-and-fallback;
/// in release builds it panics (see `global::init_from_kinfo`).
#[derive(Debug)]
pub struct QemuVirtDesc;

impl PlatformDesc for QemuVirtDesc {
    fn interrupt_controller(&self) -> InterruptControllerDesc {
        #[cfg(target_arch = "riscv64")]
        {
            InterruptControllerDesc::Plic {
                plic_base: 0x0C00_0000,
                nr_irqs: 64,
                context: 1,
            }
        }
        #[cfg(target_arch = "aarch64")]
        {
            InterruptControllerDesc::Gicv3 {
                gicd_base: 0x0800_0000,
                gicr_base: 0x080A_0000,
                gicr_stride: 0x2_0000,
                nr_irqs: 64,
            }
        }
        #[cfg(target_arch = "x86_64")]
        {
            InterruptControllerDesc::Apic {
                lapic_base: 0xFEE0_0000,
                ioapic_base: 0xFEC0_0000,
                nr_irqs: 64,
            }
        }
    }

    fn timer(&self) -> TimerDesc {
        #[cfg(target_arch = "riscv64")]
        {
            TimerDesc::Clint {
                mtime_addr: 0x200_BFF8,
                mtimecmp_base: 0x200_4000,
                mtimecmp_stride: 8,
                freq: 10_000_000,
            }
        }
        #[cfg(target_arch = "aarch64")]
        {
            TimerDesc::ArmGenericTimer
        }
        #[cfg(target_arch = "x86_64")]
        {
            TimerDesc::Pit {
                pit_base_freq: 1_193_182,
                lapic_base: 0xFEE0_0000,
            }
        }
    }

    fn early_console(&self) -> Option<ConsoleDesc> {
        #[cfg(target_arch = "x86_64")]
        {
            Some(ConsoleDesc::IsaSerial { port_base: 0x3F8 })
        }
        #[cfg(target_arch = "aarch64")]
        {
            Some(ConsoleDesc::MmioSerial { mmio_base: 0x0900_0000 })
        }
        #[cfg(target_arch = "riscv64")]
        {
            Some(ConsoleDesc::SbiConsole)
        }
    }

    fn cpu_topology(&self) -> CpuTopology {
        const NR_CPUS: u32 = 4;
        let mut cpus = [CpuInfo::default(); MAX_CPUS];

        #[cfg(target_arch = "riscv64")]
        {
            const MTIMECMP_BASE: usize = 0x200_4000;
            const MTIMECMP_STRIDE: usize = 8;
            for i in 0..NR_CPUS as usize {
                cpus[i] = CpuInfo {
                    hw_id: i as u64,
                    gicr_base: None,
                    mtimecmp_addr: Some(MTIMECMP_BASE + i * MTIMECMP_STRIDE),
                };
            }
        }

        #[cfg(target_arch = "aarch64")]
        {
            const GICR_BASE: usize = 0x080A_0000;
            const GICR_STRIDE: usize = 0x2_0000;
            for i in 0..NR_CPUS as usize {
                cpus[i] = CpuInfo {
                    hw_id: i as u64,
                    gicr_base: Some(GICR_BASE + i * GICR_STRIDE),
                    mtimecmp_addr: None,
                };
            }
        }

        #[cfg(target_arch = "x86_64")]
        {
            // QEMU q35 with `-smp 4` assigns APIC IDs 0..3 by default.
            for i in 0..NR_CPUS as usize {
                cpus[i] = CpuInfo {
                    hw_id: i as u64,
                    gicr_base: None,
                    mtimecmp_addr: None,
                };
            }
        }

        CpuTopology { nr_cpus: NR_CPUS, bsp_id: 0, cpus }
    }

    fn arch_misc(&self) -> ArchMiscDesc {
        ArchMiscDesc::default()
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
        let d = QemuVirtDesc;
        assert_eq!(d.source(), PlatformSource::QemuVirt);
    }

    #[test]
    fn test_qemu_virt_cpu_topology_four_cores() {
        let d = QemuVirtDesc;
        let t = d.cpu_topology();
        assert_eq!(t.nr_cpus, 4, "QEMU virt test config uses 4 CPUs (-smp 4)");
        assert_eq!(t.bsp_id, 0);
        for i in 0..t.nr_cpus as usize {
            assert_eq!(t.cpus[i].hw_id, i as u64);
        }

        #[cfg(target_arch = "riscv64")]
        {
            assert_eq!(t.cpus[0].mtimecmp_addr, Some(0x200_4000));
            assert_eq!(t.cpus[1].mtimecmp_addr, Some(0x200_4008));
            assert_eq!(t.cpus[3].mtimecmp_addr, Some(0x200_4018));
        }

        #[cfg(target_arch = "aarch64")]
        {
            assert_eq!(t.cpus[0].gicr_base, Some(0x080A_0000));
            assert_eq!(t.cpus[1].gicr_base, Some(0x080C_0000));
            assert_eq!(t.cpus[3].gicr_base, Some(0x0810_0000));
        }
    }

    #[cfg(target_arch = "riscv64")]
    #[test]
    fn test_qemu_virt_riscv64_interrupt_controller() {
        let d = QemuVirtDesc;
        match d.interrupt_controller() {
            InterruptControllerDesc::Plic { plic_base, nr_irqs, context } => {
                assert_eq!(plic_base, 0x0C00_0000);
                assert_eq!(nr_irqs, 64);
                assert_eq!(context, 1);
            }
            _ => panic!("expected Plic on riscv64"),
        }
    }

    #[cfg(target_arch = "riscv64")]
    #[test]
    fn test_qemu_virt_riscv64_timer() {
        let d = QemuVirtDesc;
        match d.timer() {
            TimerDesc::Clint { mtime_addr, mtimecmp_base, freq, .. } => {
                assert_eq!(mtime_addr, 0x200_BFF8);
                assert_eq!(mtimecmp_base, 0x200_4000);
                assert_eq!(freq, 10_000_000);
            }
            _ => panic!("expected Clint on riscv64"),
        }
    }

    #[cfg(target_arch = "aarch64")]
    #[test]
    fn test_qemu_virt_aarch64_interrupt_controller() {
        let d = QemuVirtDesc;
        match d.interrupt_controller() {
            InterruptControllerDesc::Gicv3 { gicd_base, gicr_base, .. } => {
                assert_eq!(gicd_base, 0x0800_0000);
                assert_eq!(gicr_base, 0x080A_0000);
            }
            _ => panic!("expected Gicv3 on aarch64"),
        }
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn test_qemu_virt_x86_64_interrupt_controller() {
        let d = QemuVirtDesc;
        match d.interrupt_controller() {
            InterruptControllerDesc::Apic { lapic_base, ioapic_base, .. } => {
                assert_eq!(lapic_base, 0xFEE0_0000);
                assert_eq!(ioapic_base, 0xFEC0_0000);
            }
            _ => panic!("expected Apic on x86_64"),
        }
    }
}
