//! x86_64 instruction-cache synchronization — no operation.
//!
//! # Why a no-op is correct here
//!
//! x86 has coherent instruction and data caches: a data write is visible to
//! subsequent instruction fetches of the same physical address without any
//! cache-maintenance instruction (there is none in the ISA). The only
//! architectural obligation after writing code is that the *executing*
//! processor reaches a serializing boundary before fetching the new bytes.
//!
//! The loader path this contract serves ends in the kernel and the new
//! process is first entered via `iretq` (the return-to-user path), which is
//! a serializing instruction; the CPU cannot fetch stale pre-write bytes
//! across it. The contract is therefore satisfied with an empty body.
//!
//! C: none — C has the same "no instruction exists" property; this file
//! exists so the loader calls one contract on all three architectures
//! (`[ARCH: cache-sync]`, SD-20).

use crate::arch::cache_sync::CacheSyncArch;
use core::ops::Range;

/// x86_64 implementor: coherent I/D caches, nothing to do.
pub struct X86_64CacheSyncArch;

impl CacheSyncArch for X86_64CacheSyncArch {
    fn sync_icache_range(_range: Range<u64>) {
        // No instruction-cache maintenance instruction exists on x86: the
        // write-through-to-fetch visibility is guaranteed by the coherent
        // I/D cache design plus the serializing `iretq` on the way back to
        // user mode (see module docs).
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The no-op must be callable with any range, including empty and
    /// unaligned ones — it never touches memory, so no range is invalid.
    #[test]
    fn noop_accepts_any_range() {
        X86_64CacheSyncArch::sync_icache_range(0..0);
        X86_64CacheSyncArch::sync_icache_range(0x1000..0x2000);
        X86_64CacheSyncArch::sync_icache_range(0x1234_5678_9abc_deff..0x1234_5678_9abc_df00);
    }
}
