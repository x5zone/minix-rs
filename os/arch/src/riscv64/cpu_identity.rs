//! RISC-V CPU identity probe — machine-level identity CSRs.
//!
//! C has no riscv port in Minix3; the identity source on this ISA is the
//! SBI-readable machine CSR set (`mvendorid`/`marchid`/`mimpid`). Reading
//! M-mode CSRs from S-mode traps; OpenSBI provides them via
//! `sbi_get_mvendorid()`/`sbi_get_marchid()`/`sbi_get_mimpid()` (the
//! fid-based base extension). QEMU virt reports mvendorid = 0 (no JEDEC
//! vendor), so `Unimplemented` returns degrade to zero fields rather
//! than failing boot.

use crate::arch::cpu_identity::{CpuIdentity, CpuIdentityArch, RiscvIdentity};

/// SBI base extension FIDs (sbiret.value carries the CSR value).
const SBI_EXT_BASE: usize = 0x10;
const SBI_BASE_GET_MVENDORID: usize = 5;
const SBI_BASE_GET_MARCHID: usize = 6;
const SBI_BASE_GET_MIMPID: usize = 7;

/// One SBI base-extension call. Returns the value, or 0 when the call is
/// not implemented (SBI returns `NOT_SUPPORTED` with value left to the
/// implementation; QEMU virt returns 0 — both degrade to zero fields).
fn sbi_get(fid: usize) -> u64 {
    let value: u64;
    let error: i64;
    unsafe {
        core::arch::asm!(
            "ecall",
            inlateout("a7") SBI_EXT_BASE => _,
            inlateout("a6") fid => _,
            lateout("a0") error,
            lateout("a1") value,
            options(nostack)
        );
    }
    if error != 0 {
        return 0;
    }
    value
}

/// RISC-V implementor of the identity probe.
pub struct Riscv64CpuIdentity;

impl CpuIdentityArch for Riscv64CpuIdentity {
    fn identify_current_cpu() -> CpuIdentity {
        CpuIdentity::Riscv(RiscvIdentity {
            mvendorid: sbi_get(SBI_BASE_GET_MVENDORID) as u32,
            marchid: sbi_get(SBI_BASE_GET_MARCHID),
            mimpid: sbi_get(SBI_BASE_GET_MIMPID),
        })
    }
}
