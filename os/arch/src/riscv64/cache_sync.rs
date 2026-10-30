//! RISC-V instruction-cache synchronization.
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
//! # SMP binding (implemented)
//!
//! `FENCE.I` does NOT reach other harts. A hart that did not perform the
//! stores needs its own fence after observing them; the portable SBI route
//! is the RFENCE extension's `remote_fence_i`, which sends an inter-processor
//! fence request to a set of harts. This implementation issues the local
//! fence and then the SBI broadcast with `hart_mask_base = -1` ("all
//! available harts", SBI §12) — no kernel hart-mask table is needed, and
//! harts that have not been started are ignored by firmware.
//!
//! The RFENCE extension is a platform prerequisite, not an optional
//! optimization: if the ecall returns non-success, the new code may be
//! fetched stale on another hart, so the call fails fast instead of
//! silently continuing. The trap-entry IPI path uses the same ecall shape
//! (EID in `a7`, FID in `a6`, arguments in `a0`/`a1`).
//!
//! C: none — Minix3 has no RISC-V port; the boot-leg scatter in
//! `os/kernel/src/arch/riscv64/higher_half.rs` contains the only other
//! `fence.i` in the tree. This contract is the `[ARCH: cache-sync]`
//! addition described in `arch::cache_sync`'s module docs.

use crate::arch::cache_sync::CacheSyncArch;
use core::arch::asm;
use core::ops::Range;

/// SBI RFENCE extension ID (EID 0x5246_4E43, ASCII "RFNC").
///
/// RISC-V SBI specification §12 (RFENCE extension).
const SBI_RFENCE_EID: u64 = 0x5246_4E43;

/// SBI RFENCE `remote_fence_i` function ID (FID 0).
const SBI_RFENCE_REMOTE_FENCE_I_FID: u64 = 0;

/// `hart_mask_base = -1` addresses every available hart; `hart_mask` is
/// ignored in that form (SBI specification §12).
const SBI_HART_MASK_ALL: u64 = u64::MAX;

/// RISC-V 64-bit implementor: local `fence.i` + SBI remote-fence broadcast.
pub struct Riscv64CacheSyncArch;

impl CacheSyncArch for Riscv64CacheSyncArch {
    fn sync_icache_range(_range: Range<u64>) {
        // SAFETY: `FENCE.I` is a fence-class instruction — it orders the
        // local hart's prior stores against its subsequent instruction
        // fetches and touches no memory, registers or CSRs. Executing it
        // with paging off (early boot) is also legal.
        unsafe { asm!("fence.i") };

        // Remote half: ask firmware to execute FENCE.I on every available
        // hart. `ecall` traps to SBI; all argument registers are
        // initialized. The call may synchronize with other harts, so `nomem`
        // is not set; `nostack` holds because no stack is accessed.
        let ret: u64;
        // SAFETY: see above. The return code is the SBI error status.
        unsafe {
            asm!(
                "ecall",
                inout("a0") 0u64 => ret,
                in("a1") SBI_HART_MASK_ALL,
                in("a6") SBI_RFENCE_REMOTE_FENCE_I_FID,
                in("a7") SBI_RFENCE_EID,
                options(nostack),
            );
        }
        // The RFENCE extension is required for cross-hart instruction
        // visibility; a nonzero status means the remote fence did not run.
        assert_eq!(
            ret, 0,
            "SBI remote_fence_i failed: the RFENCE extension is required for \
             cross-hart instruction-cache visibility"
        );
    }
}
