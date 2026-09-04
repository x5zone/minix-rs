//! AArch64 CPU identity probe — MIDR_EL1 decode.
//!
//! C: `cpu_identify()` — arch/earm/arch_system.c:85-100 (32-bit `mrc p15`
//! read of MIDR). On AArch64 the same register is read from EL1 via
//! `mrs MIDR_EL1`; the field decode (implementer/variant/arch/part/revision)
//! is identical to the 32-bit MIDR layout.
//!
//! C hardcodes `freq = 660` MHz for the earm platform; minix-rs keeps
//! frequency out of the identity record — it belongs to the clock
//! subsystem (`clock.hz()`), not to CPU identification.

use crate::arch::cpu_identity::{ArmIdentity, CpuIdentity, CpuIdentityArch};

/// AArch64 implementor of the identity probe.
pub struct AArch64CpuIdentity;

impl CpuIdentityArch for AArch64CpuIdentity {
    fn identify_current_cpu() -> CpuIdentity {
        let midr: u64;
        unsafe {
            core::arch::asm!(
                "mrs {}, midr_el1",
                out(reg) midr,
                options(nomem, nostack, preserves_flags)
            );
        }
        CpuIdentity::Arm(ArmIdentity {
            implementer: (midr >> 24) as u8,
            variant: ((midr >> 20) & 0xF) as u8,
            arch: ((midr >> 16) & 0xF) as u8,
            part: ((midr >> 4) & 0xFFF) as u16,
            revision: (midr & 0xF) as u8,
        })
    }
}
