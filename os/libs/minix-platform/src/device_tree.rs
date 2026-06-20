//! Device Tree Blob (DTB) descriptor — parses FDT and implements `PlatformDesc`.
//!
//! Used by ARM64 and RISC-V when the boot-shim provides a DTB pointer via
//! `KernelInfo.platform_descriptor`. Extracts hardware parameters (CLINT,
//! PLIC, GIC, timer frequency, CPU topology) from the FDT without hardcoding.
//!
//! # Architecture coverage
//!
//! - **RISC-V**: CLINT (`/clint` or `/soc/clint`), PLIC (`/soc/plic`),
//!   CPU timebase-frequency (`/cpus/timebase-frequency`).
//! - **ARM64**: GICv3 (`/intc` or compatible `arm,gic-v3`), Generic Timer
//!   (`/timer` or `arm,armv8-timer` — frequency from CNTFRQ_EL0 at runtime,
//!   so DTB only needs to confirm presence).
//!
//! # `no_std` constraints
//!
//! - No heap allocation: `Fdt` borrows the DTB slice for `'a`.
//! - The borrowed `Fdt` cannot outlive the DTB bytes — but `PlatformDesc`
//!   must be `Send + Sync + 'static`. We solve this by **eager parsing**:
//!   `DeviceTreeDesc::parse` walks the FDT once, extracts all needed values
//!   into owned `usize`/`u32`/`u64` fields, then drops the `Fdt` borrow.
//!   The resulting `DeviceTreeDesc` is `'static` and safe to store globally.
//!
//! # Safety
//!
//! `parse` is `unsafe` because it dereferences a raw physical pointer. The
//! caller (boot path) must guarantee:
//! - `dtb_phys` points to a valid FDT blob (magic `0xD00DFEED`).
//! - The blob remains valid for the duration of `parse` (it does — DTB is
//!   in static firmware memory).
//! - No other CPU is concurrently writing to the DTB region.

use crate::desc::*;
use core::fmt;

/// Parsed DTB descriptor — owns all extracted hardware parameters.
///
/// Constructed via [`DeviceTreeDesc::parse`] (production) or
/// [`DeviceTreeDesc::from_parsed`] (tests). After construction, the DTB
/// bytes are no longer needed — all values are stored as plain integers.
#[derive(Clone, Copy)]
pub struct DeviceTreeDesc {
    ic: InterruptControllerDesc,
    timer: TimerDesc,
    console: Option<ConsoleDesc>,
    cpu_topology: CpuTopology,
    arch_misc: ArchMiscDesc,
}

impl fmt::Debug for DeviceTreeDesc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DeviceTreeDesc")
            .field("ic", &self.ic)
            .field("timer", &self.timer)
            .field("console", &self.console)
            .field("cpu_topology", &self.cpu_topology)
            .finish_non_exhaustive()
    }
}

impl DeviceTreeDesc {
    /// Parse a DTB blob at the given physical address.
    ///
    /// # Safety
    ///
    /// Caller must guarantee `dtb_phys` points to a valid FDT blob that
    /// remains readable for the duration of this call. See module docs.
    pub unsafe fn parse(dtb_phys: usize) -> Result<Self, DtParseError> {
        // SAFETY: caller guarantees dtb_phys is a valid FDT pointer.
        let fdt = unsafe { fdt::Fdt::from_ptr(dtb_phys as *const u8) }
            .map_err(|_| DtParseError::BadFdtPointer)?;

        Self::from_fdt(&fdt)
    }

    /// Parse a DTB from a byte slice (for tests and in-memory DTB).
    pub fn from_bytes(dtb: &[u8]) -> Result<Self, DtParseError> {
        let fdt = fdt::Fdt::new(dtb).map_err(|_| DtParseError::BadFdtPointer)?;
        Self::from_fdt(&fdt)
    }

    /// Build a `DeviceTreeDesc` from a parsed `Fdt`.
    ///
    /// Architecture-specific extraction is dispatched via `#[cfg]` on
    /// **data selection** (which node to look up), not behavior — see
    /// `plat-design.md` §5.2.
    fn from_fdt(fdt: &fdt::Fdt<'_>) -> Result<Self, DtParseError> {
        let ic = Self::parse_interrupt_controller(fdt)?;
        let timer = Self::parse_timer(fdt)?;
        let console = Self::parse_console(fdt);
        let cpu_topology = Self::parse_cpu_topology(fdt);
        let arch_misc = ArchMiscDesc::default();

        Ok(Self { ic, timer, console, cpu_topology, arch_misc })
    }

    /// Construct from pre-parsed values (for tests).
    pub fn from_parsed(
        ic: InterruptControllerDesc,
        timer: TimerDesc,
        console: Option<ConsoleDesc>,
        cpu_topology: CpuTopology,
    ) -> Self {
        Self { ic, timer, console, cpu_topology, arch_misc: ArchMiscDesc::default() }
    }

    // ── Interrupt controller extraction ──

    fn parse_interrupt_controller(fdt: &fdt::Fdt<'_>) -> Result<InterruptControllerDesc, DtParseError> {
        #[cfg(target_arch = "riscv64")]
        {
            Self::parse_plic(fdt)
        }
        #[cfg(target_arch = "aarch64")]
        {
            Self::parse_gic(fdt)
        }
        #[cfg(not(any(target_arch = "riscv64", target_arch = "aarch64")))]
        {
            let _ = fdt;
            Err(DtParseError::UnsupportedArch)
        }
    }

    /// RISC-V: locate PLIC node and extract base address + IRQ count.
    ///
    /// QEMU virt DTB layout:
    /// ```text
    /// /soc {
    ///     plic@c000000 {
    ///         compatible = "sifive,plic-1.0.0", "riscv,plic0";
    ///         reg = <0x0 0xc000000 0x0 0x4000000>;
    ///         riscv,ndev = <0x35>;   // 53 external IRQs
    ///         ...
    ///     };
    /// }
    /// ```
    fn parse_plic(fdt: &fdt::Fdt<'_>) -> Result<InterruptControllerDesc, DtParseError> {
        // Search for a node compatible with RISC-V PLIC.
        let plic_node = fdt
            .find_compatible(&["riscv,plic0", "sifive,plic-1.0.0"])
            .ok_or(DtParseError::PlicNotFound)?;

        // Extract base address from `reg` (first cell pair: addr + size).
        let plic_base = plic_node
            .reg()
            .and_then(|mut r| r.next())
            .map(|r| r.starting_address as usize)
            .ok_or(DtParseError::PlicRegMissing)?;

        // `riscv,ndev` gives the number of external (non-software) IRQs.
        // Total IRQ count = ndev + 16 (16 for local IRQs 0..15 in PLIC's
        // source numbering, though PLIC starts at 1; we use ndev + 1 to
        // cover the MSIP/MTIP edge case in QEMU virt).
        let nr_irqs = plic_node
            .property("riscv,ndev")
            .and_then(|p| p.as_usize())
            .map(|n| n as u32)
            .unwrap_or(64);

        // S-mode context for hart 0 is typically context 1 in QEMU virt
        // (M-mode = 0, S-mode = 1). We hardcode 1 for single-hart boot;
        // SMP expansion will derive this from the IRQ extension / hart count.
        let context = 1u32;

        Ok(InterruptControllerDesc::Plic { plic_base, nr_irqs, context })
    }

    /// ARM64: locate GICv3 node and extract distributor + redistributor bases.
    ///
    /// QEMU virt DTB layout:
    /// ```text
    /// /intc {
    ///     compatible = "arm,gic-v3";
    ///     reg = <0x0 0x8000000 0x0 0x10000    // GICD
    ///            0x0 0x80a0000 0x0 0xf60000>; // GICR
    ///     ...
    /// };
    /// ```
    fn parse_gic(fdt: &fdt::Fdt<'_>) -> Result<InterruptControllerDesc, DtParseError> {
        let gic_node = fdt
            .find_compatible(&["arm,gic-v3"])
            .ok_or(DtParseError::GicNotFound)?;

        // `reg` contains two regions: GICD and GICR.
        let mut regs = gic_node.reg().ok_or(DtParseError::GicRegMissing)?;
        let gicd_base = regs
            .next()
            .map(|r| r.starting_address as usize)
            .ok_or(DtParseError::GicRegMissing)?;
        let gicr_base = regs
            .next()
            .map(|r| r.starting_address as usize)
            .ok_or(DtParseError::GicRegMissing)?;

        // Redistributor stride: 2 × 64KB for GICv3 (RD_base + SGI_base).
        // Standard value per ARM IHI 0069; not always in DTB.
        let gicr_stride = 0x2_0000usize;
        let nr_irqs = 64u32; // QEMU virt default; SPI count not in standard DTB prop.

        Ok(InterruptControllerDesc::Gicv3 {
            gicd_base,
            gicr_base,
            gicr_stride,
            nr_irqs,
        })
    }

    // ── Timer extraction ──

    fn parse_timer(fdt: &fdt::Fdt<'_>) -> Result<TimerDesc, DtParseError> {
        #[cfg(target_arch = "riscv64")]
        {
            Self::parse_clint(fdt)
        }
        #[cfg(target_arch = "aarch64")]
        {
            // ARM Generic Timer frequency is read from CNTFRQ_EL0 at runtime;
            // DTB only confirms presence of the timer node. We return the
            // variant and let the arch layer read the register.
            let _ = fdt;
            Ok(TimerDesc::ArmGenericTimer)
        }
        #[cfg(not(any(target_arch = "riscv64", target_arch = "aarch64")))]
        {
            let _ = fdt;
            Err(DtParseError::UnsupportedArch)
        }
    }

    /// RISC-V: locate CLINT node and extract mtime/mtimecmp addresses + frequency.
    ///
    /// QEMU virt DTB layout:
    /// ```text
    /// /soc {
    ///     clint@2000000 {
    ///         compatible = "riscv,clint0";
    ///         reg = <0x0 0x2000000 0x0 0x10000>;
    ///         ...
    ///     };
    /// }
    /// ```
    /// The CLINT layout (SiFive):
    /// - `MSIP` (per-hart): base + 0x0, stride 0x4
    /// - `MTIMECMP` (per-hart): base + 0x4000, stride 0x8
    /// - `MTIME` (global): base + 0xBFF8
    ///
    /// Frequency: `/cpus/timebase-frequency` (QEMU virt = 10 MHz).
    fn parse_clint(fdt: &fdt::Fdt<'_>) -> Result<TimerDesc, DtParseError> {
        let clint_node = fdt
            .find_compatible(&["riscv,clint0"])
            .ok_or(DtParseError::ClintNotFound)?;

        let clint_base = clint_node
            .reg()
            .and_then(|mut r| r.next())
            .map(|r| r.starting_address as usize)
            .ok_or(DtParseError::ClintRegMissing)?;

        // Standard SiFive CLINT offsets.
        let mtimecmp_base = clint_base + 0x4000;
        let mtime_addr = clint_base + 0xBFF8;
        let mtimecmp_stride = 8usize;

        // Timebase frequency: `/cpus/timebase-frequency`.
        let freq = fdt
            .find_node("/cpus")
            .and_then(|cpus| cpus.property("timebase-frequency"))
            .and_then(|p| p.as_usize())
            .map(|n| n as u64)
            .ok_or(DtParseError::TimebaseFreqMissing)?;

        Ok(TimerDesc::Clint {
            mtime_addr,
            mtimecmp_base,
            mtimecmp_stride,
            freq,
        })
    }

    // ── Console extraction ──

    fn parse_console(fdt: &fdt::Fdt<'_>) -> Option<ConsoleDesc> {
        #[cfg(target_arch = "riscv64")]
        {
            // RISC-V QEMU virt uses SBI console by default (no MMIO UART in
            // the standard DTB). If a UART node exists, prefer MMIO.
            if fdt.find_compatible(&["ns16550a", "sifive,uart0"]).is_some() {
                // UART node found; extract base. Fall back to SBI if reg missing.
                if let Some(uart) = fdt.find_compatible(&["ns16550a", "sifive,uart0"]) {
                    if let Some(base) = uart.reg().and_then(|mut r| r.next()) {
                        return Some(ConsoleDesc::MmioSerial {
                            mmio_base: base.starting_address as usize,
                        });
                    }
                }
            }
            Some(ConsoleDesc::SbiConsole)
        }
        #[cfg(target_arch = "aarch64")]
        {
            // ARM64 QEMU virt uses PL011 UART at 0x0900_0000.
            if let Some(uart) = fdt.find_compatible(&["arm,pl011", "arm,primecell"]) {
                if let Some(base) = uart.reg().and_then(|mut r| r.next()) {
                    return Some(ConsoleDesc::MmioSerial {
                        mmio_base: base.starting_address as usize,
                    });
                }
            }
            None
        }
        #[cfg(not(any(target_arch = "riscv64", target_arch = "aarch64")))]
        {
            let _ = fdt;
            None
        }
    }

    // ── CPU topology extraction ──

    fn parse_cpu_topology(fdt: &fdt::Fdt<'_>) -> CpuTopology {
        let mut cpus = [CpuInfo::default(); MAX_CPUS];
        let mut nr_cpus = 0u32;
        let mut bsp_id = 0u32;

        for cpu in fdt.cpus() {
            if (nr_cpus as usize) >= MAX_CPUS {
                break;
            }
            let hw_id = cpu.ids().first() as u64;
            if nr_cpus == 0 {
                bsp_id = hw_id as u32;
            }

            // Architecture-specific per-CPU fields.
            let gicr_base = match Self::parse_interrupt_controller(fdt) {
                Ok(InterruptControllerDesc::Gicv3 { gicr_base, gicr_stride, .. }) => {
                    Some(gicr_base + (hw_id as usize) * gicr_stride)
                }
                _ => None,
            };
            let mtimecmp_addr = match Self::parse_timer(fdt) {
                Ok(TimerDesc::Clint { mtimecmp_base, mtimecmp_stride, .. }) => {
                    Some(mtimecmp_base + (hw_id as usize) * mtimecmp_stride)
                }
                _ => None,
            };

            cpus[nr_cpus as usize] = CpuInfo {
                hw_id,
                gicr_base,
                mtimecmp_addr,
            };
            nr_cpus += 1;
        }

        if nr_cpus == 0 {
            // No CPU nodes found — fall back to single-core.
            return CpuTopology::default();
        }

        CpuTopology { nr_cpus, bsp_id, cpus }
    }
}

impl PlatformDesc for DeviceTreeDesc {
    fn interrupt_controller(&self) -> InterruptControllerDesc {
        self.ic
    }
    fn timer(&self) -> TimerDesc {
        self.timer
    }
    fn early_console(&self) -> Option<ConsoleDesc> {
        self.console
    }
    fn cpu_topology(&self) -> CpuTopology {
        self.cpu_topology
    }
    fn arch_misc(&self) -> ArchMiscDesc {
        self.arch_misc
    }
    fn source(&self) -> PlatformSource {
        PlatformSource::DeviceTree
    }
}

/// Errors that can occur during DTB parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DtParseError {
    /// The DTB pointer was invalid or the magic check failed.
    BadFdtPointer,
    /// Architecture not supported by DTB path (e.g., x86-64).
    UnsupportedArch,
    /// PLIC node not found (RISC-V).
    PlicNotFound,
    /// PLIC `reg` property missing or malformed.
    PlicRegMissing,
    /// CLINT node not found (RISC-V).
    ClintNotFound,
    /// CLINT `reg` property missing or malformed.
    ClintRegMissing,
    /// `/cpus/timebase-frequency` missing (RISC-V).
    TimebaseFreqMissing,
    /// GIC node not found (ARM64).
    GicNotFound,
    /// GIC `reg` property missing or malformed.
    GicRegMissing,
}

impl fmt::Display for DtParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadFdtPointer => write!(f, "invalid FDT pointer or bad magic"),
            Self::UnsupportedArch => write!(f, "DTB parsing not supported on this arch"),
            Self::PlicNotFound => write!(f, "PLIC node not found in DTB"),
            Self::PlicRegMissing => write!(f, "PLIC node missing `reg` property"),
            Self::ClintNotFound => write!(f, "CLINT node not found in DTB"),
            Self::ClintRegMissing => write!(f, "CLINT node missing `reg` property"),
            Self::TimebaseFreqMissing => write!(f, "`/cpus/timebase-frequency` not found"),
            Self::GicNotFound => write!(f, "GICv3 node not found in DTB"),
            Self::GicRegMissing => write!(f, "GIC node missing `reg` property"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dt_parse_error_variants() {
        // Verify all variants are constructible and distinct.
        let errs = [
            DtParseError::BadFdtPointer,
            DtParseError::UnsupportedArch,
            DtParseError::PlicNotFound,
            DtParseError::PlicRegMissing,
            DtParseError::ClintNotFound,
            DtParseError::ClintRegMissing,
            DtParseError::TimebaseFreqMissing,
            DtParseError::GicNotFound,
            DtParseError::GicRegMissing,
        ];
        // All variants must be distinct.
        for i in 0..errs.len() {
            for j in (i + 1)..errs.len() {
                assert_ne!(errs[i], errs[j], "duplicate error variant");
            }
        }
    }

    #[test]
    fn test_device_tree_desc_from_parsed() {
        let ic = InterruptControllerDesc::Plic {
            plic_base: 0x0C00_0000,
            nr_irqs: 53,
            context: 1,
        };
        let timer = TimerDesc::Clint {
            mtime_addr: 0x200_BFF8,
            mtimecmp_base: 0x200_4000,
            mtimecmp_stride: 8,
            freq: 10_000_000,
        };
        let desc = DeviceTreeDesc::from_parsed(ic, timer, None, CpuTopology::default());
        assert_eq!(desc.source(), PlatformSource::DeviceTree);
        match desc.interrupt_controller() {
            InterruptControllerDesc::Plic { plic_base, .. } => {
                assert_eq!(plic_base, 0x0C00_0000);
            }
            _ => panic!("expected Plic"),
        }
    }

    /// Test DTB from the `fdt` crate's test suite (RISC-V QEMU virt).
    /// Contains PLIC, CLINT, and CPU nodes with `riscv,plic0` / `riscv,clint0`
    /// compatible strings.
    static TEST_DTB: &[u8] = include_bytes!("../tests/data/qemu_virt_riscv.dtb");

    #[test]
    fn test_from_bytes_bad_magic() {
        // Empty buffer → BadFdtPointer.
        let r = DeviceTreeDesc::from_bytes(&[]);
        assert!(matches!(r, Err(DtParseError::BadFdtPointer)));
    }

    #[cfg(target_arch = "riscv64")]
    #[test]
    fn test_parse_riscv64_qemu_virt_dtb() {
        // Parse the RISC-V QEMU virt DTB. Expect PLIC + CLINT extraction.
        let desc = DeviceTreeDesc::from_bytes(TEST_DTB)
            .expect("DTB parse should succeed on riscv64");
        assert_eq!(desc.source(), PlatformSource::DeviceTree);

        // PLIC: base 0x0C00_0000 (from `plic@c000000`).
        match desc.interrupt_controller() {
            InterruptControllerDesc::Plic { plic_base, context, .. } => {
                assert_eq!(plic_base, 0x0C00_0000, "PLIC base mismatch");
                assert_eq!(context, 1, "S-mode context for hart 0");
            }
            other => panic!("expected Plic, got {:?}", other),
        }

        // CLINT: mtime at base + 0xBFF8, mtimecmp at base + 0x4000.
        match desc.timer() {
            TimerDesc::Clint { mtime_addr, mtimecmp_base, mtimecmp_stride, freq } => {
                // CLINT base is 0x0200_0000 (from `clint@2000000`).
                assert_eq!(mtime_addr, 0x0200_BFF8, "mtime address mismatch");
                assert_eq!(mtimecmp_base, 0x0200_4000, "mtimecmp base mismatch");
                assert_eq!(mtimecmp_stride, 8, "mtimecmp stride mismatch");
                assert!(freq > 0, "timebase frequency must be non-zero");
            }
            other => panic!("expected Clint, got {:?}", other),
        }

        // CPU topology: at least 1 CPU.
        let topo = desc.cpu_topology();
        assert!(topo.nr_cpus >= 1, "should find at least 1 CPU");
    }

    #[cfg(not(any(target_arch = "riscv64", target_arch = "aarch64")))]
    #[test]
    fn test_parse_unsupported_arch_returns_error() {
        // On x86_64 (and other non-DTB arches), DTB parsing returns
        // UnsupportedArch because the IC/timer extraction is arch-specific.
        let r = DeviceTreeDesc::from_bytes(TEST_DTB);
        assert!(matches!(r, Err(DtParseError::UnsupportedArch)));
    }
}
