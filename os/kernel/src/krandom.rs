//! Kernel randomness subsystem — entropy collection from interrupt timing.
//!
//! Provides the `krandom` global and the `get_randomness` function called
//! from the IRQ dispatch path. The collected entropy is exported to user
//! space via `SYS_GETINFO` (`GET_RANDOMNESS` / `GET_RANDOMNESS_BIN`).
//!
//! # Minix3 C Source Mapping
//!
//! - `struct k_randomness` — include/minix/type.h:187-194
//! - `krandom` global — kernel/glo.h
//! - `get_randomness()` — kernel/arch/*/arch_system.c (stub on most archs)
//! - `GET_RANDOMNESS` — kernel/system/do_getinfo.c:148-160
//! - `GET_RANDOMNESS_BIN` — kernel/system/do_getinfo.c:161-178
//!
//! # Design Decisions
//!
//! - **D1**: `#[repr(C)]` structs mirror the C ABI exactly (field order,
//!   sizes, alignment). This is required because user-space tools (the
//!   `random` driver) interpret the raw bytes via the C struct layout.
//! - **D2**: The global `KRANDOM` uses `SyncUnsafeCell` + `get()` —
//!   same pattern as `PROC_TABLE` / `PRIV_TABLE` / `IRQ_MANAGER`.
//!   Access is BKL-protected (single-writer from IRQ path, single-reader
//!   from syscall path).
//! - **D3**: `get_randomness` is a no-op stub, matching C's i386/earm
//!   implementation. The actual entropy gathering is done in user space
//!   by the `random` driver (drivers/system/random/).
//! - **D4**: `RANDOM_SOURCES = 16`, `RANDOM_ELEMENTS = 64` — matching
//!   `include/minix/type.h:182-183`.

use core::sync::atomic::{AtomicBool, Ordering};

// ── Constants ──

/// Number of randomness sources (IRQ vectors that contribute entropy).
/// C: `RANDOM_SOURCES` — include/minix/type.h:182
pub const RANDOM_SOURCES: usize = 16;

/// Number of entropy elements per source bin.
/// C: `RANDOM_ELEMENTS` — include/minix/type.h:183
pub const RANDOM_ELEMENTS: usize = 64;

// ── C-compatible structures ──

/// One randomness bin — collects entropy from a single source (IRQ vector).
///
/// C: `struct k_randomness_bin` — include/minix/type.h:189-193
///
/// # Layout
///
/// ```text
/// offset  field    type     size
/// 0       r_next   i32      4
/// 4       r_size   i32      4
/// 8       r_buf    [u16;64] 128
/// total                      136 bytes
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct KRandomnessBin {
    /// Next index to write in `r_buf`. C: `r_next`
    pub r_next: i32,
    /// Number of valid elements in `r_buf`. C: `r_size`
    /// When `< RANDOM_ELEMENTS`, the bin is not yet full.
    pub r_size: i32,
    /// Buffer for random info. C: `r_buf[RANDOM_ELEMENTS]`
    /// `rand_t = unsigned short` in C (include/minix/type.h:185).
    pub r_buf: [u16; RANDOM_ELEMENTS],
}

impl KRandomnessBin {
    /// Create a zero-initialized bin.
    pub const fn new() -> Self {
        Self {
            r_next: 0,
            r_size: 0,
            r_buf: [0; RANDOM_ELEMENTS],
        }
    }

    /// Reset the bin to zero (invalidate random data).
    ///
    /// C: `krandom.bin[i].r_size = 0; krandom.bin[i].r_next = 0;`
    /// — do_getinfo.c:154-155
    pub fn wipe(&mut self) {
        self.r_next = 0;
        self.r_size = 0;
        // C does NOT zero r_buf (just invalidates by setting r_size=0).
        // We match C behavior: r_buf contents remain but are invalidated.
    }
}

impl Default for KRandomnessBin {
    fn default() -> Self {
        Self::new()
    }
}

/// Kernel randomness collection structure.
///
/// C: `struct k_randomness` — include/minix/type.h:187-194
///
/// # Layout
///
/// ```text
/// offset  field           type                     size
/// 0       random_elements  i32                      4
/// 4       random_sources   i32                      4
/// 8       bin             [KRandomnessBin; 16]     2176
/// total                                             2184 bytes
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct KRandomness {
    /// Capacity of each bin's buffer. C: `random_elements`
    pub random_elements: i32,
    /// Number of source bins. C: `random_sources`
    pub random_sources: i32,
    /// Per-source entropy bins. C: `bin[RANDOM_SOURCES]`
    pub bin: [KRandomnessBin; RANDOM_SOURCES],
}

impl KRandomness {
    /// Create a zero-initialized krandom, with capacity fields set.
    ///
    /// C: `krandom.random_sources = RANDOM_SOURCES;`
    ///    `krandom.random_elements = RANDOM_ELEMENTS;` — main.c:48
    pub const fn new() -> Self {
        Self {
            random_elements: RANDOM_ELEMENTS as i32,
            random_sources: RANDOM_SOURCES as i32,
            bin: [KRandomnessBin::new(); RANDOM_SOURCES],
        }
    }

    /// Wipe all bins (invalidate all random data).
    ///
    /// C: do_getinfo.c:153-156 — called after `GET_RANDOMNESS` copy.
    pub fn wipe_all(&mut self) {
        for bin in &mut self.bin {
            bin.wipe();
        }
    }

    /// Wipe a single bin by index.
    ///
    /// C: do_getinfo.c:219-221 — called after `GET_RANDOMNESS_BIN` copy.
    pub fn wipe_bin(&mut self, bin: usize) {
        if bin < RANDOM_SOURCES {
            self.bin[bin].wipe();
        }
    }
}

impl Default for KRandomness {
    fn default() -> Self {
        Self::new()
    }
}

// ── Global state ──

/// Global kernel randomness state.
///
/// C: `krandom` — kernel/glo.h
///
/// # Safety
///
/// Access is BKL-protected. The IRQ path (single-writer via
/// `get_randomness`) and the syscall path (single-reader via
/// `dispatch_getinfo`) never run concurrently under BKL.
static KRANDOM: crate::SyncUnsafeCell<KRandomness> = crate::SyncUnsafeCell::new(KRandomness::new());

/// Track whether `KRANDOM` has been initialized (fields set to non-zero).
/// C: main.c:48 sets `krandom.random_sources = RANDOM_SOURCES`.
/// In Rust, `KRANDOM` is `const fn new()` so it's initialized at link time.
static KRANDOM_INIT: AtomicBool = AtomicBool::new(false);

/// Mark the krandom global as initialized.
///
/// C: `krandom.random_sources = RANDOM_SOURCES;` — main.c:48
/// In Rust, the static is already initialized via `const fn`, so this
/// just sets the init flag for `try_krandom()` callers.
///
/// # Safety
///
/// Caller must be in single-threaded boot context (before BKL exists).
pub fn init() {
    KRANDOM_INIT.store(true, Ordering::Release);
}

/// Get a mutable reference to the global krandom.
///
/// Returns `None` if `init()` has not been called yet (e.g. in unit
/// tests that skip boot).
///
/// # Safety
///
/// Caller must hold the BKL (or be in single-threaded boot before BKL exists).
pub unsafe fn try_krandom() -> Option<&'static mut KRandomness> {
    if !KRANDOM_INIT.load(Ordering::Acquire) {
        return None;
    }
    // SAFETY: caller guarantees BKL (or single-threaded boot).
    // `SyncUnsafeCell::get()` returns a raw pointer, avoiding `static_mut_refs`.
    Some(unsafe { &mut *KRANDOM.get() })
}

/// Get a mutable reference to the global krandom, panicking if not initialized.
///
/// # Safety
///
/// Caller must hold the BKL (or be in single-threaded boot before BKL exists).
pub unsafe fn krandom() -> &'static mut KRandomness {
    unsafe { try_krandom() }.expect("KRANDOM not initialized — krandom::init() must run first")
}

// ── Entropy collection ──

/// Gather entropy from an interrupt source.
///
/// Called from the IRQ dispatch path (`generic_handler`) to record
/// timing-based entropy from the IRQ vector.
///
/// C: `get_randomness(&krandom, hook->irq)` — do_irqctl.c:154
///
/// # Implementation
///
/// The C implementation is a **no-op stub** on most architectures (i386,
/// earm — `arch_system.c:179`). The actual entropy gathering is done in
/// user space by the `random` driver, which reads the (zero) bins via
/// `GET_RANDOMNESS` and combines them with its own entropy sources.
///
/// We match the C behavior: this function is a no-op. When a real
/// implementation is needed, it would record `read_tsc()` into
/// `krandom.bin[source].r_buf[r_next]` and advance `r_next`.
///
/// # Safety
///
/// Caller must hold the BKL (IRQ dispatch path acquires BKL).
pub fn get_randomness(_source: i32) {
    // No-op stub — matches C's i386/earm implementation.
    // Real implementation would be:
    //   if let Some(kr) = unsafe { try_krandom() } {
    //       let src = (_source as usize) % RANDOM_SOURCES;
    //       let bin = &mut kr.bin[src];
    //       if (bin.r_size as usize) < RANDOM_ELEMENTS {
    //           let tsc = read_tsc();
    //           bin.r_buf[bin.r_next as usize] = (tsc & 0xFFFF) as u16;
    //           bin.r_next = (bin.r_next + 1) % RANDOM_ELEMENTS as i32;
    //           if bin.r_next == 0 { bin.r_size = RANDOM_ELEMENTS as i32; }
    //           else if bin.r_size < bin.r_next { bin.r_size = bin.r_next; }
    //       }
    //   }
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_krandomness_bin_layout() {
        // C struct k_randomness_bin has:
        //   int r_next (4) + int r_size (4) + rand_t r_buf[64] (128) = 136 bytes
        assert_eq!(
            core::mem::size_of::<KRandomnessBin>(),
            136,
            "KRandomnessBin must match C struct k_randomness_bin layout"
        );
    }

    #[test]
    fn test_krandomness_layout() {
        // C struct k_randomness has:
        //   int random_elements (4) + int random_sources (4) +
        //   struct k_randomness_bin bin[16] (16*136=2176) = 2184 bytes
        assert_eq!(
            core::mem::size_of::<KRandomness>(),
            2184,
            "KRandomness must match C struct k_randomness layout"
        );
    }

    #[test]
    fn test_krandomness_new() {
        let kr = KRandomness::new();
        assert_eq!(kr.random_elements, RANDOM_ELEMENTS as i32);
        assert_eq!(kr.random_sources, RANDOM_SOURCES as i32);
        for bin in &kr.bin {
            assert_eq!(bin.r_next, 0);
            assert_eq!(bin.r_size, 0);
        }
    }

    #[test]
    fn test_wipe_bin() {
        let mut kr = KRandomness::new();
        kr.bin[5].r_next = 10;
        kr.bin[5].r_size = 10;
        kr.wipe_bin(5);
        assert_eq!(kr.bin[5].r_next, 0);
        assert_eq!(kr.bin[5].r_size, 0);
    }

    #[test]
    fn test_wipe_all() {
        let mut kr = KRandomness::new();
        for bin in &mut kr.bin {
            bin.r_next = 5;
            bin.r_size = 5;
        }
        kr.wipe_all();
        for bin in &kr.bin {
            assert_eq!(bin.r_next, 0);
            assert_eq!(bin.r_size, 0);
        }
    }

    #[test]
    fn test_get_randomness_is_noop() {
        // get_randomness is a no-op stub matching C's i386 implementation.
        // It should not panic and should not modify any state.
        get_randomness(3);
        get_randomness(15);
    }
}
