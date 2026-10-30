//! AArch64 instruction-cache synchronization.
//!
//! Implements the ARMv8 by-VA maintenance sequence of the ARM ARM,
//! section "Cache maintenance for instruction coherency":
//!
//! 1. clean the D-cache to the point of unification (PoU) for each cache
//!    line covering the range — `DC CVAU`;
//! 2. `DSB ISH` — make those cleans visible to the instruction side;
//! 3. invalidate the I-cache to PoU for each line — `IC IVAU`;
//! 4. `DSB ISH` and `ISB` — complete the invalidation and synchronize the
//!    instruction stream before the caller fetches the new code.
//!
//! Both loops use the cache line sizes from `CTR_EL0` (`DminLine` and
//! `IminLine`, encoded as log2 of words); a floor of 16 bytes keeps a
//! mis-decoded register from under-synchronizing (over-invalidation is
//! harmless, under-invalidation is the bug this file exists to prevent).
//!
//! Scope of the broadcast: `IC IVAU` invalidates to PoU for the inner
//! shareable domain, and `DC CVAU` + `DSB ISH` publish the cleaned data to
//! that domain, so the sequence issued on one PE covers every other PE in
//! the same inner shareable domain — no cross-CPU IPI is required for
//! secondary cores to observe newly written code.
//!
//! C: none — Minix3 has no aarch64 kernel port and no cache-maintenance
//! call in its loader path; this is the `[ARCH: cache-sync]` addition
//! recorded in `arch::cache_sync`'s module docs.

use crate::arch::cache_sync::CacheSyncArch;
use core::arch::asm;
use core::ops::Range;

/// AArch64 implementor: `dc cvau` + `dsb ish` + `ic ivau` + `dsb ish` +
/// `isb` over the range.
pub struct AArch64CacheSyncArch;

/// `CTR_EL0.DminLine` field position (bits [19:16]).
const CTR_EL0_DMINLINE_SHIFT: u32 = 16;
/// `CTR_EL0.IminLine` field position (bits [3:0]).
const CTR_EL0_IMINLINE_SHIFT: u32 = 0;
/// Architectural minimum line size (and the fallback when the decoded
/// value would be smaller than any real implementation's line).
const MIN_LINE_BYTES: u64 = 16;

/// Decode a `CTR_EL0` line-size field: the field holds log2 of words.
fn line_bytes(ctr: u64, shift: u32) -> u64 {
    let log2_words = ((ctr >> shift) & 0xF) as u32;
    let bytes = 4u64 << log2_words;
    if bytes < MIN_LINE_BYTES {
        MIN_LINE_BYTES
    } else {
        bytes
    }
}

impl CacheSyncArch for AArch64CacheSyncArch {
    fn sync_icache_range(range: Range<u64>) {
        if range.start >= range.end {
            return;
        }
        // `CTR_EL0` is always readable at EL1 (it advertises cache line
        // sizes; some implementations also allow EL0 reads).
        let ctr: u64;
        // SAFETY: reading CTR_EL0 at EL1 is unconditionally allowed.
        unsafe { asm!("mrs {}, ctr_el0", out(reg) ctr) };
        let dline = line_bytes(ctr, CTR_EL0_DMINLINE_SHIFT);
        let iline = line_bytes(ctr, CTR_EL0_IMINLINE_SHIFT);

        // 1. Clean D to PoU, first line-aligned address at or below start.
        let mut addr = range.start & !(dline - 1);
        while addr < range.end {
            // SAFETY: `DC CVAU` on a mapped address is a data-cache clean;
            // the caller guarantees the range is mapped in the active root
            // (trait contract). No register state is clobbered.
            unsafe { asm!("dc cvau, {}", in(reg) addr) };
            addr += dline;
        }
        // 2. Publish the cleaned lines to the instruction side.
        // SAFETY: `DSB ISH` orders the preceding cleans within the inner
        // shareable domain; no memory is dereferenced.
        unsafe { asm!("dsb ish") };

        // 3. Invalidate I to PoU, first line-aligned address at or below
        //    start (the I line size may differ from the D line size).
        let mut addr = range.start & !(iline - 1);
        while addr < range.end {
            // SAFETY: `IC IVAU` on a mapped address is an I-cache
            // invalidate-to-PoU; the caller guarantees the range is mapped
            // in the active root (trait contract). No register clobber.
            unsafe { asm!("ic ivau, {}", in(reg) addr) };
            addr += iline;
        }
        // 4. Complete the invalidations and synchronize the instruction
        //    stream with the context.
        // SAFETY: `DSB ISH` completes the invalidations; `ISB` flushes the
        // pipeline so instructions already prefetched from the old contents
        // cannot execute. No memory is dereferenced.
        unsafe {
            asm!("dsb ish");
            asm!("isb");
        }
    }
}
