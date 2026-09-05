//! Entropy pools: thirty-two pools, round-robin feeds, reseed schedule.
//!
//! C correspondence: the pool state (`deriv`, `pool_ind`, `pool_ctx`,
//! `samples`) and `add_sample` in `minix3/minix/drivers/system/random/
//! random.c:18-31,130-179`, the pool count `NR_POOLS 32` and derivative
//! depth `N_DERIV 16` (`random.c:18-19`), the reseed threshold
//! `MIN_SAMPLES 256` (`random.c:20-22`), and the reseed walk in `reseed`
//! (`random.c:206-236`).
//!
//! Hashing stays behind the [`PoolHash`] trait: production wires the
//! system hash, tests use the folding hasher below. Pool topology (which
//! sample lands where, which pools join a reseed) is pure and fully
//! testable.

/// Number of entropy pools.
///
/// C: `NR_POOLS 32` (`random.c:19`).
pub const POOL_COUNT: usize = 32;

/// Depth of the per-source derivative chain.
///
/// C: `N_DERIV 16` (`random.c:18`).
pub const DERIVATIVE_DEPTH: usize = 16;

/// Samples needed in pool zero before a reseed runs.
///
/// C: `MIN_SAMPLES 256` (`random.c:20-22`).
pub const MIN_SAMPLES: u32 = 256;

/// Sources feeding the pools: sixteen kernel sources plus the internal
/// timing source.
///
/// C: `RANDOM_SOURCES 16` (`type.h:182`) plus `RANDOM_SOURCES_INTERNAL 1`
/// with `TOTAL_SOURCES` their sum (`random.h:8-10`); the timing source is
/// index zero (`RND_TIMING`, `random.h:8`).
pub const KERNEL_SOURCES: usize = 16;
/// Internal sources (timing only).
pub const INTERNAL_SOURCES: usize = 1;
/// All sources.
pub const TOTAL_SOURCES: usize = KERNEL_SOURCES + INTERNAL_SOURCES;
/// Timing source index.
pub const TIMING_SOURCE: usize = 0;

/// Minimum derivative magnitude for an accepted sample.
///
/// C: `if (min < 2) return;` (`random.c:156-163`): flat samples carry no
/// entropy and are dropped before touching any pool.
pub const MIN_DERIVATIVE: u64 = 2;

/// Hash behavior for one pool: absorb bytes, snapshot and reset.
///
/// C: the `SHA256_CTX pool_ctx[NR_POOLS]` uses (`random.c:26`): update on
/// feed, final plus re-init on reseed. The digest width is fixed at
/// thirty-two bytes (the system hash width); test hashers match it.
pub trait PoolHash {
    /// Absorb bytes into the running state.
    fn absorb(&mut self, bytes: &[u8]);
    /// Snapshot the digest and restart empty (final plus re-init).
    fn snapshot_reset(&mut self, out: &mut [u8; 32]);
}

/// Folding test hasher: rotates and exclusive-ors input into thirty-two
/// bytes. Not cryptographic; only proves the pool plumbing.
#[derive(Debug, Clone)]
pub struct FoldHash {
    state: [u8; 32],
    position: usize,
}

impl FoldHash {
    /// Fresh hasher (zero state).
    pub const fn new() -> FoldHash {
        FoldHash {
            state: [0; 32],
            position: 0,
        }
    }
}

impl Default for FoldHash {
    fn default() -> Self {
        FoldHash::new()
    }
}

impl PoolHash for FoldHash {
    fn absorb(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.state[self.position] ^= byte.wrapping_add(1);
            self.position = (self.position + 1) % 32;
        }
    }

    fn snapshot_reset(&mut self, out: &mut [u8; 32]) {
        *out = self.state;
        self.state = [0; 32];
        self.position = 0;
    }
}

/// Null test hasher: absorbs nothing, always snapshots zeros.
///
/// Behavior differs from [`FoldHash`] (which mixes input): pairs of pools
/// that must stay independent in tests use different hashers, and the
/// seeded/empty distinction stays visible.
#[derive(Debug, Default, Clone, Copy)]
pub struct NullHash;

impl PoolHash for NullHash {
    fn absorb(&mut self, _bytes: &[u8]) {}

    fn snapshot_reset(&mut self, out: &mut [u8; 32]) {
        *out = [0; 32];
    }
}

/// Derivative filter for one source: sixteen chained differences.
///
/// C: the `deriv[source][N_DERIV]` chain in `add_sample`
/// (`random.c:140-163`): each new sample pushes through the chain, the
/// minimum absolute difference decides acceptance.
#[derive(Debug, Clone)]
pub struct DerivativeFilter {
    chain: [u64; DERIVATIVE_DEPTH],
}

impl DerivativeFilter {
    /// Fresh filter (zero chain, like `random_init`).
    pub const fn new() -> DerivativeFilter {
        DerivativeFilter {
            chain: [0; DERIVATIVE_DEPTH],
        }
    }

    /// Feed one sample; true means accepted into a pool.
    pub fn feed(&mut self, sample: u64) -> bool {
        let mut value = sample;
        let mut min = u64::MAX;
        for slot in self.chain.iter_mut() {
            let previous = *slot;
            let difference = value.abs_diff(previous);
            *slot = value;
            value = difference;
            if value < min {
                min = value;
            }
        }
        min >= MIN_DERIVATIVE
    }
}

impl Default for DerivativeFilter {
    fn default() -> Self {
        DerivativeFilter::new()
    }
}

/// Which pools join the next reseed: pool zero plus pools one through the
/// first set bit of the reseed counter.
///
/// C: the walk in `reseed` (`random.c:222-229`): pool zero always joins;
/// pool `i` joins unless bit `i-1` of the (already incremented) counter
/// is set, at which point the walk stops. Returns the count of extra
/// pools (zero means pool zero alone).
pub const fn reseed_extra_pools(reseed_count: u64) -> usize {
    let mut extra = 0;
    while extra < POOL_COUNT - 1 {
        if reseed_count & (1 << extra) != 0 {
            break;
        }
        extra += 1;
    }
    extra
}

/// Pool set: per-source filters, round-robin cursors, sample count.
///
/// C: `deriv`, `pool_ind`, `samples` (`random.c:24-27`) with the feed in
/// `random_update`/`add_sample` and the threshold in `reseed`.
#[derive(Debug, Clone)]
pub struct PoolSet {
    filters: [DerivativeFilter; TOTAL_SOURCES],
    cursors: [usize; TOTAL_SOURCES],
    samples: u32,
}

impl PoolSet {
    /// Fresh set: zero chains, zero cursors, zero samples.
    ///
    /// C: `random_init` (`random.c:37-55`).
    pub fn new() -> PoolSet {
        PoolSet {
            filters: core::array::from_fn(|_| DerivativeFilter::new()),
            cursors: [0; TOTAL_SOURCES],
            samples: 0,
        }
    }

    /// Samples counted toward the next reseed.
    pub const fn samples(&self) -> u32 {
        self.samples
    }

    /// Feed raw kernel samples for one source; returns the pool each
    /// accepted sample landed in.
    ///
    /// C: `random_update` panics on a bad source (`random.c:74-75`); here
    /// a bad source is refused with false and no panic (library code must
    /// not stop the process; the service crate maps the refusal).
    pub fn feed<H: PoolHash>(&mut self, pools: &mut [H], source: usize, samples: &[u64]) -> bool {
        if source >= TOTAL_SOURCES || pools.len() != POOL_COUNT {
            return false;
        }
        for sample in samples {
            if !self.filters[source].feed(*sample) {
                continue;
            }
            let pool = self.cursors[source];
            pools[pool].absorb(&sample.to_ne_bytes());
            if pool == 0 {
                self.samples += 1;
            }
            self.cursors[source] = if pool + 1 >= POOL_COUNT { 0 } else { pool + 1 };
        }
        true
    }

    /// Feed caller bytes straight into pool zero as trusted bits.
    ///
    /// C: `random_putbytes` (`random.c:115-128`): counts eight samples per
    /// byte (trusted means full credit) and tries a reseed.
    pub fn feed_trusted<H: PoolHash>(&mut self, pools: &mut [H], bytes: &[u8]) -> bool {
        if pools.len() != POOL_COUNT {
            return false;
        }
        pools[0].absorb(bytes);
        self.samples = self
            .samples
            .saturating_add((bytes.len() as u32).saturating_mul(8));
        true
    }

    /// True when pool zero holds enough samples to reseed.
    pub const fn reseed_due(&self) -> bool {
        self.samples >= MIN_SAMPLES
    }

    /// Clear the sample count after a reseed.
    ///
    /// C: `samples = 0` at the end of `reseed` (`random.c:233`).
    pub fn note_reseeded(&mut self) {
        self.samples = 0;
    }
}

impl Default for PoolSet {
    fn default() -> Self {
        PoolSet::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pools() -> [FoldHash; POOL_COUNT] {
        core::array::from_fn(|_| FoldHash::new())
    }

    #[test]
    fn test_constants_match_c_sources() {
        assert_eq!(POOL_COUNT, 32);
        assert_eq!(DERIVATIVE_DEPTH, 16);
        assert_eq!(MIN_SAMPLES, 256);
        assert_eq!(TOTAL_SOURCES, 17);
        assert_eq!(TIMING_SOURCE, 0);
    }

    #[test]
    fn test_flat_samples_are_rejected_once_chain_fills() {
        let mut filter = DerivativeFilter::new();
        assert!(filter.feed(42));
        for _ in 0..40 {
            filter.feed(42);
        }
        assert!(!filter.feed(42));
    }

    #[test]
    fn test_varied_samples_are_accepted() {
        let mut filter = DerivativeFilter::new();
        let mut accepted = 0;
        let mut value = 1000u64;
        for _ in 0..40 {
            value = value
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            if filter.feed(value) {
                accepted += 1;
            }
        }
        assert!(accepted > 0);
    }

    #[test]
    fn test_feeds_round_robin_across_pools() {
        let mut set = PoolSet::new();
        let mut pool_set = pools();
        let mut value = 7u64;
        let mut samples = alloc::vec::Vec::new();
        for _ in 0..80 {
            value = value
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            samples.push(value);
        }
        assert!(set.feed(&mut pool_set, 3, &samples));
        assert!(set.cursors[3] > 0);
    }

    #[test]
    fn test_bad_source_is_refused_not_fatal() {
        let mut set = PoolSet::new();
        let mut pool_set = pools();
        assert!(!set.feed(&mut pool_set, TOTAL_SOURCES, &[1, 2, 3]));
    }

    #[test]
    fn test_trusted_bytes_credit_eight_samples_each() {
        let mut set = PoolSet::new();
        let mut pool_set = pools();
        set.feed_trusted(&mut pool_set, &[0; 32]);
        assert!(set.reseed_due());
        set.note_reseeded();
        assert!(!set.reseed_due());
    }

    #[test]
    fn test_reseed_schedule_matches_c_walk() {
        assert_eq!(reseed_extra_pools(1), 0);
        assert_eq!(reseed_extra_pools(2), 1);
        assert_eq!(reseed_extra_pools(3), 0);
        assert_eq!(reseed_extra_pools(4), 2);
    }

    #[test]
    fn test_null_hasher_ignores_input() {
        let mut hasher = NullHash;
        hasher.absorb(&[1, 2, 3]);
        let mut out = [0xFF; 32];
        hasher.snapshot_reset(&mut out);
        assert_eq!(out, [0; 32]);
    }
}
