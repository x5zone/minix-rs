//! Packet-buffer pool: slice sizes, statistics, exhaustion mapping.
//!
//! C correspondence: the buffer size (`MEMPOOL_BUFSIZE 512`,
//! `lwipopts.h:49`), the pool statistics (`mempool_cur_buffers`,
//! `mempool.c:493-497`, and `mempool_max_buffers`,
//! `mempool.c:505-516`), the chain tools (`pchain_end` and
//! `pchain_size`, `pchain.c:108-129`), and the exhaustion mapping
//! (pool empty becomes no-buffers at the socket layer,
//! `udpsock.c:496-497`, `rawsock.c:716-717`).
//!
//! Pool storage stays in the service binary; this module owns the
//! sizing half (slice size, chain split, pressure line) and the pool
//! itself: slab-grown 512-byte slice storage with explicit handles.

use alloc::vec::Vec;

/// Bytes in one pool slice (`MEMPOOL_BUFSIZE`).
pub const SLICE_SIZE: u64 = 512;

/// Pool statistics: current and maximum buffer counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PoolStats {
    /// Buffers currently checked out.
    pub current: u32,
    /// Most buffers ever checked out at once.
    pub peak: u32,
}

impl PoolStats {
    /// Whether the pool is under pressure (three quarters of the
    /// send-buffer rule, cf. `TCP_MAX_SENDBUFS`, `tcpsock.c:101`).
    pub fn under_pressure(&self, limit: u32) -> bool {
        self.current * 4 >= limit * 3
    }
}

/// Split a request of `total` bytes into slice lengths (512-byte
/// chaining, `lwipopts.h:18-28`).
pub fn split_slices(total: u64) -> alloc::vec::Vec<u64> {
    let mut out = alloc::vec::Vec::new();
    let mut remaining = total;
    while remaining > 0 {
        let step = remaining.min(SLICE_SIZE);
        out.push(step);
        remaining -= step;
    }
    out
}

/// The slice size in the `usize` shape the pool arena needs; identical to
/// [`SLICE_SIZE`], which keeps the `u64` view the byte accounting uses.
pub const SLICE_BYTES: usize = 512;

/// Buffers per freshly added slab (`MEMPOOL_LARGE_COUNT`, `mempool.c:116`).
pub const SLICES_PER_SLAB: u32 = 512;

/// Slab-count ceiling of the pool (`MEMPOOL_DEFAULT_MAX_SLABS`, 64,
/// `mempool.c:238` — the comment there prices it at about 17 MB, and
/// 64 × 512 slices × 512 bytes = 16 777 216 bytes agrees).
pub const DEFAULT_MAX_SLABS: u32 = 64;

/// A handle to one pooled slice. Handles stay valid until handed back to
/// [`Pool::free`]; a handle is an index into the pool's slice array, not a
/// pointer, so growing the pool never invalidates a handle that is still out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct SliceHandle(pub u32);

/// The service-side packet pool: fixed 512-byte slices in slab-grown
/// storage.
///
/// C correspondence: the custom pool that `PBUF_POOL_SIZE 0` demands
/// (`lwipopts.h:80`; the pool owns allocation instead of the pbuf pool),
/// the slab growth up to [`DEFAULT_MAX_SLABS`] (`mempool.c:223-238`), and
/// the exhaustion shape — an empty pool surfaces as no-buffers at the
/// socket layer (`udpsock.c:496-497`, `rawsock.c:716-717`), which here is
/// [`Pool::alloc`] returning `None`.
///
/// The C pool has two buffer sizes (large and quarter-large,
/// `mempool.c:116-123`); the split exists to pack a pbuf header next to
/// each 512-byte data area. With the stack wall in place (24 篇 §1.5) the
/// server side keeps only data slices — the header-and-alignment packing
/// belongs to whichever stack sits behind the wall — so one size remains,
/// and the small slab kind is gone with it.
pub struct Pool {
    slices: Vec<[u8; SLICE_BYTES]>,
    free: Vec<SliceHandle>,
    slabs: u32,
    max_slabs: u32,
    stats: PoolStats,
}

impl Pool {
    /// A pool with the default slab ceiling.
    pub fn new() -> Pool {
        Pool::with_slab_limit(DEFAULT_MAX_SLABS)
    }

    /// A pool with an explicit slab ceiling (smaller limits are for
    /// tests and constrained deployments).
    pub fn with_slab_limit(max_slabs: u32) -> Pool {
        Pool {
            slices: Vec::new(),
            free: Vec::new(),
            slabs: 0,
            max_slabs,
            stats: PoolStats { current: 0, peak: 0 },
        }
    }

    /// Take one slice, growing by a slab when the free list is empty and
    /// the ceiling allows. `None` means exhausted — the no-buffers shape.
    pub fn alloc(&mut self) -> Option<SliceHandle> {
        let handle = match self.free.pop() {
            Some(handle) => handle,
            None => {
                if self.slabs >= self.max_slabs {
                    return None;
                }
                let base = self.slices.len() as u32;
                for offset in 0..SLICES_PER_SLAB {
                    let index = base + offset;
                    self.slices.push([0u8; SLICE_BYTES]);
                    self.free.push(SliceHandle(index));
                }
                self.slabs += 1;
                self.free.pop()?
            }
        };
        self.stats.current += 1;
        if self.stats.current > self.stats.peak {
            self.stats.peak = self.stats.current;
        }
        Some(handle)
    }

    /// Hand a slice back.
    pub fn free(&mut self, handle: SliceHandle) {
        assert!((handle.0 as usize) < self.slices.len(), "游离切片句柄");
        self.free.push(handle);
        self.stats.current -= 1;
    }

    /// Read one slice's bytes.
    pub fn slice(&self, handle: SliceHandle) -> &[u8; SLICE_BYTES] {
        &self.slices[handle.0 as usize]
    }

    /// Write one slice's bytes.
    pub fn slice_mut(&mut self, handle: SliceHandle) -> &mut [u8; SLICE_BYTES] {
        &mut self.slices[handle.0 as usize]
    }

    /// Pool statistics (current and peak checkout counts).
    pub const fn stats(&self) -> PoolStats {
        self.stats
    }

    /// Slices the pool can hold at its current slab count.
    pub const fn capacity(&self) -> u32 {
        self.slabs * SLICES_PER_SLAB
    }
}

impl Default for Pool {
    fn default() -> Self {
        Pool::new()
    }
}

/// One in-flight packet: a chain of pooled slices plus the used length of
/// each (`pchain_size`-shaped accounting, `pchain.c:108-129`).
#[derive(Debug, Default)]
pub struct Frame {
    parts: Vec<(SliceHandle, usize)>,
}

impl Frame {
    /// An empty frame.
    pub fn new() -> Frame {
        Frame { parts: Vec::new() }
    }

    /// Append a slice carrying `len` used bytes.
    pub fn push(&mut self, handle: SliceHandle, len: usize) {
        debug_assert!(len <= SLICE_BYTES, "切片越界长度");
        self.parts.push((handle, len));
    }

    /// Used byte count across the chain.
    pub fn len(&self) -> usize {
        self.parts.iter().map(|(_, len)| *len).sum()
    }

    /// Whether nothing was ever appended.
    pub fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }

    /// The chain, in order.
    pub fn parts(&self) -> &[(SliceHandle, usize)] {
        &self.parts
    }

    /// Hand every slice back to the pool and empty the frame.
    pub fn release(self, pool: &mut Pool) {
        for (handle, _) in self.parts {
            pool.free(handle);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn test_slice_size_matches_options() {
        assert_eq!(SLICE_SIZE, 512);
    }

    #[test]
    fn test_split_chains_at_512() {
        assert_eq!(split_slices(1200), alloc::vec![512, 512, 176]);
        assert_eq!(split_slices(0), alloc::vec![]);
    }

    #[test]
    fn test_pressure_at_three_quarters() {
        let stats = PoolStats { current: 75, peak: 80 };
        assert!(stats.under_pressure(100));
        assert!(!stats.under_pressure(200));
    }

    #[test]
    fn test_pool_grows_by_slab_up_to_the_ceiling() {
        let mut pool = Pool::with_slab_limit(2);
        assert_eq!(pool.capacity(), 0, "空池容量为零");

        let first = pool.alloc().expect("首分配触发一块 slab");
        assert_eq!(pool.capacity(), SLICES_PER_SLAB);
        assert_eq!(pool.stats().current, 1);

        let mut handles = vec![first];
        for _ in 1..SLICES_PER_SLAB {
            handles.push(pool.alloc().expect("本 slab 配额内"));
        }
        assert_eq!(pool.capacity(), SLICES_PER_SLAB, "配额未破格不增长");
        let crossing = pool.alloc().expect("越界分配触发第二块 slab");
        assert_eq!(pool.capacity(), 2 * SLICES_PER_SLAB);
        handles.push(crossing);

        // 补到全池四分之三线：512 + 256 = 768 片（1024 的四分之三）。
        for _ in 0..(SLICES_PER_SLAB / 2) {
            handles.push(pool.alloc().expect("第二块 slab 配额内"));
        }
        assert!(pool.stats().under_pressure(SLICES_PER_SLAB * 2), "四分之三线上");
        for handle in handles {
            pool.free(handle);
        }
        assert_eq!(pool.stats().current, 0);
    }

    #[test]
    fn test_exhaustion_reports_none_the_nobuffers_shape() {
        let mut pool = Pool::with_slab_limit(1);
        let mut handles = Vec::new();
        for _ in 0..SLICES_PER_SLAB {
            handles.push(pool.alloc().expect("一块 slab 恰好发满"));
        }
        assert!(pool.alloc().is_none(), "到顶即无缓冲——ENOBUFS 的池侧形状");
        pool.free(handles[handles.len() - 1]);
        assert!(pool.alloc().is_some(), "归还一片即可再分配");
    }

    #[test]
    fn test_slice_bytes_round_trip_through_handles() {
        let mut pool = Pool::new();
        let a = pool.alloc().expect("a");
        pool.slice_mut(a).fill(0xA5);
        assert_eq!(pool.slice(a)[0], 0xA5);
        assert_eq!(pool.slice(a)[SLICE_BYTES - 1], 0xA5);
        pool.free(a);
        let b = pool.alloc().expect("b 复用刚归还的槽");
        assert_eq!(b, a, "空闲栈 LIFO，先归还先复用");
    }

    #[test]
    fn test_frame_accounts_and_releases_the_chain() {
        let mut pool = Pool::new();
        let plan = split_slices(1200);
        assert_eq!(plan, alloc::vec![512, 512, 176], "512 字节切链不变");

        let mut frame = Frame::new();
        for len in &plan {
            let handle = pool.alloc().expect("逐段分配");
            frame.push(handle, *len as usize);
        }
        assert_eq!(frame.len(), 1200);
        assert!(!frame.is_empty());
        assert_eq!(pool.stats().current, 3);

        frame.release(&mut pool);
        assert_eq!(pool.stats().current, 0, "整链归还");
    }

    #[test]
    #[should_panic(expected = "游离切片句柄")]
    fn test_free_rejects_a_stray_handle() {
        let mut pool = Pool::with_slab_limit(1);
        // 从未分配过的越界句柄必须被拒绝：C 侧对非法释放无条件崩溃，
        // 这里用不带条件编译的断言守住同一条不变量。
        pool.free(SliceHandle(SLICES_PER_SLAB + 1));
    }

    #[test]
    fn test_default_ceiling_matches_the_seventeen_mb_note() {
        assert_eq!(DEFAULT_MAX_SLABS, 64);
        assert_eq!(SLICES_PER_SLAB, 512);
        let total = DEFAULT_MAX_SLABS as u64 * SLICES_PER_SLAB as u64 * SLICE_SIZE;
        assert_eq!(total, 16_777_216, "64 slab × 512 片 × 512 字节 ≈ 17MB（mempool.c:238）");
    }
}
