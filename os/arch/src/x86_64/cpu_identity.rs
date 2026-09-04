//! x86-64 CPU identity probe — CPUID leaves 0 and 1.
//!
//! C: `cpu_identify()` — arch/i386/arch_system.c:212-243. Leaf 0's vendor
//! string decides `X86Vendor`; leaf 1's EAX signature decomposes into
//! family/model/stepping (see [`decode_signature`]).
//!
//! # Modern-hardware scope
//!
//! minix-rs only supports modern x86-64 CPUs (QEMU's modern CPU models are
//! the supported platform), so two C-era relics are deliberately dropped:
//! the `max_leaf == 0` guard for 486-era CPUs (leaf 1 architecturally
//! exists on every long-mode CPU) and C's base-model-conditioned
//! extended-model merge (a MINIX3 BUG — see [`decode_signature`]).

use crate::arch::cpu_identity::{CpuIdentity, CpuIdentityArch, X86Identity, X86Vendor};

/// Leaf-0 EBX/ECX/EDX signatures identifying vendors
/// (C: INTEL_CPUID_GEN_* / AMD_CPUID_GEN_* in include/minix/x86.h,
/// the ASCII "GenuineIntel" / "AuthenticAMD" fragments).
const INTEL_EBX: u32 = 0x756E_6547; // "uneG"
const INTEL_ECX: u32 = 0x6C65_746E; // "letn"
const INTEL_EDX: u32 = 0x4965_6E69; // "Ieni"
const AMD_EBX: u32 = 0x6874_7541; // "htuA"
const AMD_ECX: u32 = 0x444D_4163; // "DMAc"
const AMD_EDX: u32 = 0x6974_6E65; // "itne"

/// Reads CPUID leaf `leaf` into (eax, ebx, ecx, edx).
fn cpuid(leaf: u32) -> (u32, u32, u32, u32) {
    let eax: u32;
    let ebx: u32;
    let ecx: u32;
    let edx: u32;
    unsafe {
        core::arch::asm!(
            // rbx is reserved by LLVM on x86-64 and cannot be a direct asm
            // operand — save/restore it around `cpuid` and copy its value
            // into a free register (same pattern as the `raw-cpuid` crate).
            "push rbx",
            "cpuid",
            "mov {0:e}, ebx",
            "pop rbx",
            out(reg) ebx,
            inout("rax") leaf => eax,
            inout("ecx") 0u32 => ecx,
            out("edx") edx,
        );
    }
    (eax, ebx, ecx, edx)
}

/// Decode the CPUID leaf-1 EAX signature into `(family, model, stepping)`.
///
/// Per SDM (CPUID leaf 1, EAX): extended family [27:20] joins when the base
/// family is 0xF; extended model [19:16] joins when the *family* is 0xF or
/// 0x6.
///
/// MINIX3 BUG: C (arch_system.c:239) conditions the extended-model merge on
/// the *base model* (`model == 0xf || model == 0x6`), truncating the model
/// for every family-6 CPU whose base model is neither 0xF nor 0x6 — i.e.
/// all modern Intel cores from Penryn on (Nehalem 0x106E0 reports 0xE
/// instead of 0x1E; Skylake 0x406E9 reports 0xE instead of 0x4E). The
/// condition is harmless on C's era hardware (family-0xF Netburst sets the
/// extended-model bits only with base model 6) but breaks exactly the
/// modern CPUs minix-rs targets. // Rust fix: condition on family.
fn decode_signature(eax: u32) -> (u8, u8, u8) {
    let mut family = ((eax >> 8) & 0xF) as u8;
    if family == 0xF {
        family = family.saturating_add(((eax >> 20) & 0xFF) as u8);
    }
    let mut model = ((eax >> 4) & 0xF) as u8;
    if family == 0xF || family == 0x6 {
        model += (((eax >> 16) & 0xF) << 4) as u8;
    }
    (family, model, (eax & 0xF) as u8)
}

/// x86-64 implementor of the identity probe.
pub struct X86_64CpuIdentity;

impl CpuIdentityArch for X86_64CpuIdentity {
    fn identify_current_cpu() -> CpuIdentity {
        // Leaf 0: vendor string in EBX:EDX:ECX (max leaf in EAX is unused —
        // leaf 1 exists on every x86-64 CPU, no 486-era guard needed).
        let (_, ebx, ecx, edx) = cpuid(0);
        let vendor = match (ebx, ecx, edx) {
            (INTEL_EBX, INTEL_ECX, INTEL_EDX) => X86Vendor::Intel,
            (AMD_EBX, AMD_ECX, AMD_EDX) => X86Vendor::Amd,
            _ => X86Vendor::Unknown,
        };

        // Leaf 1: EAX = version information (family/model/stepping),
        // ECX/EDX = feature flags. C: arch_system.c:232-243.
        let (eax, _, fcx, fdx) = cpuid(1);
        let (family, model, stepping) = decode_signature(eax);

        CpuIdentity::X86(X86Identity {
            vendor,
            family,
            model,
            stepping,
            feature_ecx: fcx,
            feature_edx: fdx,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::decode_signature;

    /// Family-6 extended model must merge (SDM): Skylake 0x406E9 → model
    /// 0x4E. C's base-model condition would truncate it to 0xE — the
    /// regression this decode fixes.
    #[test]
    fn test_decode_signature_family6_extended_model() {
        assert_eq!(decode_signature(0x0004_06E9), (6, 0x4E, 9));
    }

    /// Nehalem 0x106E0 → model 0x1E (C reports 0xE).
    #[test]
    fn test_decode_signature_nehalem() {
        assert_eq!(decode_signature(0x0001_06E0), (6, 0x1E, 0));
    }

    /// Family-0xF path unchanged: AMD Zen 2 (0x0030F11) merges ext model;
    /// Netburst-era signatures without ext-model bits decode as before.
    #[test]
    fn test_decode_signature_family_f() {
        assert_eq!(decode_signature(0x0030_F11), (0xF, 0x31, 1));
        assert_eq!(decode_signature(0x0000_0F34), (0xF, 3, 4));
    }

    /// Extended family merge (0xF + ext): AMD Family 10h Barcelona
    /// (0x00100F22) decodes to family 0x10, model 2. (No x86-64 CPU sets
    /// ext family ≠ 0 today, decode kept for SDM completeness.)
    #[test]
    fn test_decode_signature_extended_family() {
        assert_eq!(decode_signature(0x0010_0F22), (0x10, 2, 2));
    }
}
