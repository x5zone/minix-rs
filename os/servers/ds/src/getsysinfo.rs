//! DS getsysinfo: lend the whole store image to a reader.
//!
//! Mirrors `do_getsysinfo` (`minix3/minix/servers/ds/store.c:653-678`).
//! 11-ds-getsysinfo.md.
//!
//! The module owns the verdict and nothing else: which `what` is
//! acceptable, and how many bytes move. The copy itself
//! (`sys_datacopy`, 02) stays out — the caller moves
//! [`image_bytes()`] through its own transport.
//!
//! Why this exists at all: the Information Server renders `dmp_ds`
//! (`servers/is/dmp_ds.c`) by reading DS memory *as* `struct
//! data_store` — no parsing, no version check. The image layout is
//! therefore a cross-server contract (A-10), and the scan order that
//! fills it (first-fit ascending, 04) is load-bearing: reorder the
//! allocator and the reader misreads in silence.
//!
//! Single-threaded event loop: pure functions, no shared state.

use core::mem::size_of;

use minix_types::EINVAL;

use crate::store::{DataEntry, NR_DS_KEYS};

/// The only accepted query. C: `SI_DATA_STORE` — sysinfo.h:13.
pub const SI_DATA_STORE: i32 = 5;

/// Why a getsysinfo is refused (`do_getsysinfo` error paths).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GetsysinfoReject {
    /// Unknown query. C: `EINVAL` — store.c:659-665 (`default` arm).
    BadWhat,
    /// Size mismatch. C: `EINVAL` — store.c:668-669.
    ///
    /// Exact match, both directions: a short buffer would truncate the
    /// image, a long one would leak past it. C accepts neither.
    BadSize,
}

impl GetsysinfoReject {
    /// The Minix3 errno each refusal carries (no invented codes).
    pub const fn errno(self) -> i32 {
        match self {
            Self::BadWhat | Self::BadSize => EINVAL,
        }
    }
}

/// How many bytes the image holds (`sizeof(ds_store)`, store.c:671).
///
/// The product, not a magic number: grow the entry or the table and
/// the image follows. Callers compare the request `size` against this
/// before copying — the exact-match rule at :668.
pub const fn image_bytes() -> usize {
    size_of::<DataEntry>() * NR_DS_KEYS
}

/// Decide a getsysinfo (`do_getsysinfo` verdict, store.c:659-672).
///
/// Two gates, in C order: the query must be `SI_DATA_STORE`, and the
/// offered size must equal [`image_bytes()`] exactly. Success answers
/// the image length for the caller to copy.
pub fn plan_getsysinfo(what: i32, size: usize) -> Result<usize, GetsysinfoReject> {
    if what != SI_DATA_STORE {
        return Err(GetsysinfoReject::BadWhat);
    }
    if size != image_bytes() {
        return Err(GetsysinfoReject::BadSize);
    }
    Ok(image_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_image_is_entry_times_table() {
        // The image is the table, whole: 192-byte entries × 128 seats.
        assert_eq!(image_bytes(), size_of::<DataEntry>() * NR_DS_KEYS);
        assert_eq!(image_bytes(), 192 * 128);
    }

    #[test]
    fn test_exact_size_passes() {
        assert_eq!(plan_getsysinfo(SI_DATA_STORE, image_bytes()), Ok(image_bytes()));
    }

    #[test]
    fn test_wrong_what_refuses() {
        assert_eq!(
            plan_getsysinfo(0, image_bytes()),
            Err(GetsysinfoReject::BadWhat)
        );
    }

    #[test]
    fn test_short_and_long_sizes_refuse() {
        // Both directions refuse: no truncation, no over-read (:668).
        assert_eq!(
            plan_getsysinfo(SI_DATA_STORE, image_bytes() - 1),
            Err(GetsysinfoReject::BadSize)
        );
        assert_eq!(
            plan_getsysinfo(SI_DATA_STORE, image_bytes() + 1),
            Err(GetsysinfoReject::BadSize)
        );
        assert_eq!(
            plan_getsysinfo(SI_DATA_STORE, 0),
            Err(GetsysinfoReject::BadSize)
        );
    }

    #[test]
    fn test_errno_mapping() {
        assert_eq!(GetsysinfoReject::BadWhat.errno(), EINVAL);
        assert_eq!(GetsysinfoReject::BadSize.errno(), EINVAL);
    }
}
