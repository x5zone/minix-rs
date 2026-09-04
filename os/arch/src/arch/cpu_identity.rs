//! CPU identity probing — "which CPU am I running on?" at the ISA level.
//!
//! # Minix3 C Source Mapping
//!
//! - `arch/i386/arch_system.c:212-243` — `cpu_identify()`: CPUID fills
//!   `cpu_info[cpu]` (vendor/family/model/stepping/flags)
//! - `arch/earm/arch_system.c:85-100` — MIDR fills `cpu_info[cpu]`
//!   (implementer/variant/arch/part/revision + hardcoded freq)
//! - `glo.h` — `EXTERN struct cpu_info cpu_info[CONFIG_MAX_CPUS]`
//!
//! # Design (08-system-init-boot-finish.md §4.6, D-53)
//!
//! The three ISAs report *different shapes* of identity data (x86's
//! family/model decomposition does not exist on ARM; ARM's MIDR fields do
//! not exist on RISC-V), and C papers over this by having each
//! architecture write different-shaped data into the same byte blob that
//! user space then re-interprets per-arch. Rust makes that per-arch shape
//! a *type-level fact*: [`CpuIdentity`] is an enum with one variant per
//! ISA, each arch producing its own variant — the kernel never needs to
//! reinterpret raw fields across architectures.
//!
//! Probing happens at boot, once per CPU. This crate only supplies the
//! probe (a pure register read for the *currently executing* CPU); the
//! kernel owns the per-CPU table and indexes it by its own CPU-id
//! mechanism — mirroring C's split of `cpu_identify()` (arch code) vs
//! `cpu_info[CONFIG_MAX_CPUS]` (kernel glo.h).

/// x86-64 CPU identity (CPUID leaf 0 vendor string + leaf 1 signature).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct X86Identity {
    /// CPUID leaf-0 vendor: INTEL / AMD / UNKNOWN — C: `CPU_VENDOR_*` codes.
    pub vendor: X86Vendor,
    /// C: `family` — bits [11:8], extended by [27:20] when base == 0xF.
    pub family: u8,
    /// C: `model` — bits [7:4], extended by [19:16]<<4 when family ∈
    /// {0xF, 0x6} (SDM rule; MINIX3 BUG fix — C conditions on the base
    /// model, see x86_64/cpu_identity.rs::decode_signature).
    pub model: u8,
    /// C: `stepping` — bits [3:0].
    pub stepping: u8,
    /// C: `flags[0]` — CPUID leaf-1 ECX feature bits.
    pub feature_ecx: u32,
    /// C: `flags[1]` — CPUID leaf-1 EDX feature bits.
    pub feature_edx: u32,
}

/// x86 CPU vendor classification — C: `CPU_VENDOR_*` in include/minix/type.h.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum X86Vendor {
    /// C: CPU_VENDOR_INTEL.
    Intel,
    /// C: CPU_VENDOR_AMD.
    Amd,
    /// C: CPU_VENDOR_UNKNOWN.
    Unknown,
}

/// AArch64 CPU identity — MIDR_EL1 decode (C earm `cpu_identify`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArmIdentity {
    /// MIDR[31:24] — implementer code (C: `implementer`).
    pub implementer: u8,
    /// MIDR[23:20] — major revision (C: `variant`).
    pub variant: u8,
    /// MIDR[19:16] — architecture (C: `arch`).
    pub arch: u8,
    /// MIDR[15:4] — part number (C: `part`).
    pub part: u16,
    /// MIDR[3:0] — minor revision (C: `revision`).
    pub revision: u8,
}

/// RISC-V CPU identity — `marchid`/`mimpid`/`mvendorid` CSRs.
///
/// C has no riscv port; this variant expresses the equivalent Sv-mode
/// identity registers so the table shape is uniform across all three
/// supported architectures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RiscvIdentity {
    /// `mvendorid` — JEDEC vendor ID (0 on QEMU virt).
    pub mvendorid: u32,
    /// `marchid` — architecture ID.
    pub marchid: u64,
    /// `mimpid` — implementation ID.
    pub mimpid: u64,
}

/// Per-CPU identity as reported by the running ISA.
///
/// One variant per architecture — the same information C's per-arch
/// `struct cpu_info` layouts carried, made explicit at the type level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpuIdentity {
    /// x86-64 CPUID identity.
    X86(X86Identity),
    /// AArch64 MIDR identity.
    Arm(ArmIdentity),
    /// RISC-V CSR identity.
    Riscv(RiscvIdentity),
}

/// Per-architecture CPU identity probe.
///
/// Implementations read ISA identity registers for the *currently
/// executing* CPU and classify them into the arch's [`CpuIdentity`]
/// variant. Pure register reads: no allocation, no side effects, safe to
/// call once per CPU during boot.
pub trait CpuIdentityArch {
    /// Probe the identity of the currently executing CPU.
    ///
    /// x86: CPUID leaves 0/1 (vendor string + family/model/stepping +
    /// feature flags). aarch64: MIDR_EL1 decode. riscv64:
    /// mvendorid/marchid/mimpid CSRs.
    fn identify_current_cpu() -> CpuIdentity;
}

// ── Mock (tests / `feature = "mock"` builds) ──

/// Mock identity probe: returns a value chosen by the test via
/// [`set_mock_identity_raw`] instead of touching ISA registers, so
/// kernel-layer tests run on the x86_64 host without executing `cpuid`.
#[cfg(feature = "mock")]
pub mod mock {
    use super::{CpuIdentity, CpuIdentityArch};
    use core::sync::atomic::{AtomicU64, Ordering};

    static MOCK_RAW: AtomicU64 = AtomicU64::new(0);

    /// Set the raw identity the mock returns. Encoded as
    /// `[vendor:8][family:8][model:8][stepping:8]` packed into a u64 —
    /// enough for tests to distinguish probe inputs without the test
    /// crate depending on arch internals.
    pub fn set_mock_identity_raw(raw: u64) {
        MOCK_RAW.store(raw, Ordering::Relaxed);
    }

    pub struct MockCpuIdentity;

    impl CpuIdentityArch for MockCpuIdentity {
        fn identify_current_cpu() -> CpuIdentity {
            let raw = MOCK_RAW.load(Ordering::Relaxed);
            CpuIdentity::X86(super::X86Identity {
                vendor: match raw >> 24 & 0xFF {
                    1 => super::X86Vendor::Intel,
                    2 => super::X86Vendor::Amd,
                    _ => super::X86Vendor::Unknown,
                },
                family: (raw >> 16 & 0xFF) as u8,
                model: (raw >> 8 & 0xFF) as u8,
                stepping: (raw & 0xFF) as u8,
                feature_ecx: 0,
                feature_edx: 0,
            })
        }
    }
}
