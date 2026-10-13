//! RISC-V instruction-cache synchronization (SD-20).
//!
//! `FENCE.I` synchronizes the *local hart's* instruction stream with the
//! stores that same hart has already made to instruction memory — which is
//! exactly the loader's situation: the ELF bytes are written by whichever
//! hart runs the loader (the BSP), then the image is fetched. The
//! instruction ignores its surrounding memory range: it is defined as a
//! whole-hart operation over "prior stores by this hart", so the `range`
//! parameter of the shared contract is unused here while remaining
//! meaningful for the by-VA architectures.
//!
//! # SMP binding (recorded, not implemented here)
//!
//! `FENCE.I` does NOT reach other harts. A hart that did not perform the
//! stores needs an SBI remote fence (`remote_fence_i` of the SBI RFENCE
//! extension) before it may fetch the new code. Today only the BSP executes
//! loaded images (`clamp_cpu_to_bsp` keeps every process on CPU 0), so the
//! local fence is complete for the current execution model; the remote arm
//! is **a precondition of removing the main-core clamp** and lands with the
//! SMP unclamp batch, where the kernel's hart topology can supply the
//! hart-mask the arch layer does not own by itself. Same binding shape as
//! the IPC delivery-barrier (SD-13 bound to the unclamp milestone).
//!
//! C: none — Minix3 has no RISC-V port; the boot-leg scatter in
//! `os/kernel/src/arch/riscv64/higher_half.rs` already contains the only
//! other `fence.i` in the tree. This contract is the `[ARCH: cache-sync]`
//! addition frozen by the design register SD-20.

use crate::arch::cache_sync::CacheSyncArch;
use core::arch::asm;
use core::ops::Range;

/// RISC-V 64-bit implementor: `fence.i`.
pub struct Riscv64CacheSyncArch;

impl CacheSyncArch for Riscv64CacheSyncArch {
    fn sync_icache_range(_range: Range<u64>) {
        // SAFETY: `FENCE.I` is a fence-class instruction — it orders the
        // local hart's prior stores against its subsequent instruction
        // fetches and touches no memory, registers or CSRs. Executing it
        // with paging off (early boot) is also legal.
        unsafe { asm!("fence.i") };
    }
}
