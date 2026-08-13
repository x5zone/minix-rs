//! Device Tree Blob (DTB) descriptor — parses FDT and implements `PlatformDesc`.
//!
//! Used by ARM64 and RISC-V when the boot-shim provides a DTB pointer via
//! `KernelInfo.platform_sources`. Extracts hardware parameters (CLINT,
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
//!   into owned `usize`/`u32`/`u64` fields (stored as concrete arch structs),
//!   then drops the `Fdt` borrow. The resulting `DeviceTreeDesc` is `'static`
//!   and safe to store globally.
//!
//! # Safety
//!
//! `parse` is `unsafe` because it dereferences a raw physical pointer. The
//! caller (boot path) must guarantee:
//! - `dtb_phys` points to a valid FDT blob (magic `0xD00DFEED`).
//! - The blob remains valid for the duration of `parse` (it does — DTB is
//!   in static firmware memory).
//! - No other CPU is concurrently writing to the DTB region.
//! - Early boot identity mapping covers the physical address: during boot
//!   the kernel enables paging with an identity mapping over the low 4 GB
//!   (C `pg_identity()` semantics, `os/kernel/src/lib.rs:27-28`), and the
//!   DTB lives in static firmware memory far below that bound — so the raw
//!   physical address can be dereferenced as a virtual address (VA == PA).
//!   If a future path hands a DTB above the identity-mapped range, `parse`
//!   must be preceded by a temporary mapping.

use crate::desc::*;
use core::fmt;

// ── Arch-specific sub-descriptor type aliases ──
//
// `DeviceTreeDesc` is only meaningfully used on `riscv64` and `aarch64`.
// On other arches the struct still compiles (with placeholder field types)
// but `parse` returns `UnsupportedArch` early. The trait impl is cfg-gated
// to arches that actually support DTB.

#[cfg(target_arch = "riscv64")]
use crate::arch::riscv64::{ClintDesc, PlicDesc, Riscv64ConsoleDesc};
#[cfg(target_arch = "aarch64")]
use crate::arch::aarch64::{ArmGenericTimerDesc, Gicv3Desc, MmioSerialDesc};

/// Parsed DTB descriptor — owns all extracted hardware parameters.
///
/// Constructed via [`DeviceTreeDesc::parse`] (production) or
/// [`DeviceTreeDesc::from_parsed`] (tests). After construction, the DTB
/// bytes are no longer needed — all values are stored as concrete arch
/// structs implementing the sub-descriptor traits.
#[derive(Clone, Copy)]
pub struct DeviceTreeDesc {
    /// Interrupt controller descriptor (arch-specific concrete type).
    #[cfg(target_arch = "riscv64")]
    ic: PlicDesc,
    #[cfg(target_arch = "aarch64")]
    ic: Gicv3Desc,
    /// Timer descriptor (arch-specific concrete type).
    #[cfg(target_arch = "riscv64")]
    timer: ClintDesc,
    #[cfg(target_arch = "aarch64")]
    timer: ArmGenericTimerDesc,
    /// Early console descriptor (optional).
    #[cfg(target_arch = "riscv64")]
    console: Option<Riscv64ConsoleDesc>,
    #[cfg(target_arch = "aarch64")]
    console: Option<MmioSerialDesc>,
    /// CPU topology.
    cpu_topology: CpuTopology,
    /// Architecture miscellany.
    arch_misc: ArchMiscDesc,
}

#[cfg(any(target_arch = "riscv64", target_arch = "aarch64"))]
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

#[cfg(not(any(target_arch = "riscv64", target_arch = "aarch64")))]
impl fmt::Debug for DeviceTreeDesc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DeviceTreeDesc")
            .field("cpu_topology", &self.cpu_topology)
            .field("arch_misc", &self.arch_misc)
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
        #[cfg(any(target_arch = "riscv64", target_arch = "aarch64"))]
        {
            // SAFETY: caller guarantees dtb_phys is a valid FDT pointer.
            let fdt = unsafe { fdt::Fdt::from_ptr(dtb_phys as *const u8) }
                .map_err(|_| DtParseError::BadFdtPointer)?;
            Self::from_fdt(&fdt)
        }
        #[cfg(not(any(target_arch = "riscv64", target_arch = "aarch64")))]
        {
            let _ = dtb_phys;
            Err(DtParseError::UnsupportedArch)
        }
    }

    /// Parse a DTB from a byte slice (for tests and in-memory DTB).
    pub fn from_bytes(dtb: &[u8]) -> Result<Self, DtParseError> {
        #[cfg(any(target_arch = "riscv64", target_arch = "aarch64"))]
        {
            let fdt = fdt::Fdt::new(dtb).map_err(|_| DtParseError::BadFdtPointer)?;
            Self::from_fdt(&fdt)
        }
        #[cfg(not(any(target_arch = "riscv64", target_arch = "aarch64")))]
        {
            let _ = dtb;
            Err(DtParseError::UnsupportedArch)
        }
    }

    /// Build a `DeviceTreeDesc` from a parsed `Fdt`.
    ///
    /// Architecture-specific extraction is dispatched via `#[cfg]` on
    /// **data selection** (which node to look up), not behavior — see
    /// 04-platform-discovery.md §4.2.2.
    #[cfg(any(target_arch = "riscv64", target_arch = "aarch64"))]
    fn from_fdt(fdt: &fdt::Fdt<'_>) -> Result<Self, DtParseError> {
        let ic = Self::parse_interrupt_controller(fdt)?;
        let timer = Self::parse_timer(fdt)?;
        let console = Self::parse_console(fdt);
        let cpu_topology = Self::parse_cpu_topology(fdt);
        let arch_misc = ArchMiscDesc::default();

        Ok(Self { ic, timer, console, cpu_topology, arch_misc })
    }

    // ── Interrupt controller extraction ──

    #[cfg(target_arch = "riscv64")]
    fn parse_interrupt_controller(fdt: &fdt::Fdt<'_>) -> Result<PlicDesc, DtParseError> {
        Self::parse_plic(fdt)
    }

    #[cfg(target_arch = "aarch64")]
    fn parse_interrupt_controller(fdt: &fdt::Fdt<'_>) -> Result<Gicv3Desc, DtParseError> {
        Self::parse_gic(fdt)
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
    #[cfg(target_arch = "riscv64")]
    fn parse_plic(fdt: &fdt::Fdt<'_>) -> Result<PlicDesc, DtParseError> {
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
        let nr_irqs = plic_node
            .property("riscv,ndev")
            .and_then(|p| p.as_usize())
            .map(|n| n as u32)
            .unwrap_or(64);

        // S-mode context for hart 0 is typically context 1 in QEMU virt
        // (M-mode = 0, S-mode = 1).
        let context = 1u32;

        Ok(PlicDesc { plic_base, nr_irqs, context })
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
    #[cfg(target_arch = "aarch64")]
    fn parse_gic(fdt: &fdt::Fdt<'_>) -> Result<Gicv3Desc, DtParseError> {
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
        let gicr_stride = 0x2_0000usize;
        let nr_irqs = 64u32; // QEMU virt default; SPI count not in standard DTB prop.

        Ok(Gicv3Desc { gicd_base, gicr_base, gicr_stride, nr_irqs })
    }

    // ── Timer extraction ──

    #[cfg(target_arch = "riscv64")]
    fn parse_timer(fdt: &fdt::Fdt<'_>) -> Result<ClintDesc, DtParseError> {
        Self::parse_clint(fdt)
    }

    #[cfg(target_arch = "aarch64")]
    fn parse_timer(_fdt: &fdt::Fdt<'_>) -> Result<ArmGenericTimerDesc, DtParseError> {
        // ARM Generic Timer frequency is read from CNTFRQ_EL0 at runtime;
        // DTB only confirms presence of the timer node.
        Ok(ArmGenericTimerDesc)
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
    #[cfg(target_arch = "riscv64")]
    fn parse_clint(fdt: &fdt::Fdt<'_>) -> Result<ClintDesc, DtParseError> {
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

        Ok(ClintDesc { mtime_addr, mtimecmp_base, mtimecmp_stride, freq })
    }

    // ── Console extraction ──

    #[cfg(target_arch = "riscv64")]
    fn parse_console(fdt: &fdt::Fdt<'_>) -> Option<Riscv64ConsoleDesc> {
        // RISC-V QEMU virt uses SBI console by default (no MMIO UART in
        // the standard DTB). If a UART node exists, prefer MMIO.
        if let Some(uart) = fdt.find_compatible(&["ns16550a", "sifive,uart0"]) {
            if let Some(base) = uart.reg().and_then(|mut r| r.next()) {
                return Some(Riscv64ConsoleDesc::mmio(base.starting_address as usize));
            }
        }
        Some(Riscv64ConsoleDesc::sbi())
    }

    #[cfg(target_arch = "aarch64")]
    fn parse_console(fdt: &fdt::Fdt<'_>) -> Option<MmioSerialDesc> {
        // ARM64 QEMU virt uses PL011 UART at 0x0900_0000.
        if let Some(uart) = fdt.find_compatible(&["arm,pl011", "arm,primecell"]) {
            if let Some(base) = uart.reg().and_then(|mut r| r.next()) {
                return Some(MmioSerialDesc { mmio_base: base.starting_address as usize });
            }
        }
        None
    }

    // ── CPU topology extraction ──

    #[cfg(any(target_arch = "riscv64", target_arch = "aarch64"))]
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
            #[cfg(target_arch = "aarch64")]
            let gicr_base = match Self::parse_gic(fdt) {
                Ok(g) => Some(g.gicr_base + (hw_id as usize) * g.gicr_stride),
                Err(_) => None,
            };
            #[cfg(not(target_arch = "aarch64"))]
            let gicr_base = None;

            #[cfg(target_arch = "riscv64")]
            let mtimecmp_addr = match Self::parse_clint(fdt) {
                Ok(c) => Some(c.mtimecmp_base + (hw_id as usize) * c.mtimecmp_stride),
                Err(_) => None,
            };
            #[cfg(not(target_arch = "riscv64"))]
            let mtimecmp_addr = None;

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

// The trait impl is only compiled on arches that support DTB.
// On other arches, `DeviceTreeDesc` has no sub-descriptor fields and the
// trait impl cannot be written.
#[cfg(any(target_arch = "riscv64", target_arch = "aarch64"))]
impl PlatformDesc for DeviceTreeDesc {
    fn interrupt_controller(&self) -> &dyn InterruptControllerDesc {
        &self.ic
    }
    fn timer(&self) -> &dyn TimerDesc {
        &self.timer
    }
    fn early_console(&self) -> Option<&dyn ConsoleDesc> {
        self.console.as_ref().map(|c| c as &dyn ConsoleDesc)
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
    fn test_from_bytes_bad_magic() {
        // Empty buffer → BadFdtPointer on DTB arches, UnsupportedArch on others.
        let r = DeviceTreeDesc::from_bytes(&[]);
        #[cfg(any(target_arch = "riscv64", target_arch = "aarch64"))]
        assert!(matches!(r, Err(DtParseError::BadFdtPointer)));
        #[cfg(not(any(target_arch = "riscv64", target_arch = "aarch64")))]
        assert!(matches!(r, Err(DtParseError::UnsupportedArch)));
    }

    /// Test DTB from the `fdt` crate's test suite (RISC-V QEMU virt).
    /// Contains PLIC, CLINT, and CPU nodes with `riscv,plic0` / `riscv,clint0`
    /// compatible strings.
    static TEST_DTB: &[u8] = include_bytes!("../tests/data/qemu_virt_riscv.dtb");

    #[cfg(target_arch = "riscv64")]
    #[test]
    fn test_parse_riscv64_qemu_virt_dtb() {
        // Parse the RISC-V QEMU virt DTB. Expect PLIC + CLINT extraction.
        let desc = DeviceTreeDesc::from_bytes(TEST_DTB)
            .expect("DTB parse should succeed on riscv64");
        assert_eq!(desc.source(), PlatformSource::DeviceTree);

        // PLIC: base 0x0C00_0000 (from `plic@c000000`).
        let ic = desc.interrupt_controller();
        let plic = ic
            .as_any()
            .downcast_ref::<PlicDesc>()
            .expect("expected PlicDesc");
        assert_eq!(plic.plic_base, 0x0C00_0000, "PLIC base mismatch");
        assert_eq!(plic.context, 1, "S-mode context for hart 0");

        // CLINT: mtime at base + 0xBFF8, mtimecmp at base + 0x4000.
        let timer = desc.timer();
        let clint = timer
            .as_any()
            .downcast_ref::<ClintDesc>()
            .expect("expected ClintDesc");
        // CLINT base is 0x0200_0000 (from `clint@2000000`).
        assert_eq!(clint.mtime_addr, 0x0200_BFF8, "mtime address mismatch");
        assert_eq!(clint.mtimecmp_base, 0x0200_4000, "mtimecmp base mismatch");
        assert_eq!(clint.mtimecmp_stride, 8, "mtimecmp stride mismatch");
        assert!(clint.freq > 0, "timebase frequency must be non-zero");

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
