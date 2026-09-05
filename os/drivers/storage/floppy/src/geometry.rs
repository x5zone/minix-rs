//! Floppy geometry and retry policy: densities, error budget, recalibration.
//!
//! C correspondence: the density table `fdensity` (`floppy.c:161-177`),
//! the drive/media compatibility table (`floppy.c:194-199`), the error
//! budget `MAX_ERRORS` (`floppy.c:128`) with the halfway recalibration
//! (`floppy.c:652-657`), and the `recalibrate`/`f_reset` helpers
//! (`floppy.c:1045-1061`, `floppy.c:222-258`).
//!
//! Hardware traffic (FDC commands such as `FDC_SEEK 0x0F`,
//! `FDC_RECALIBRATE`, `floppy.c:87-1061`) stays in the service binary;
//! this module owns the pure policy half: which geometry a density
//! means, and when an error deserves a retry, a recalibration, or a
//! reset.

/// Highest-density media: 2880 blocks (`HC_SIZE`, `floppy.c:101`).
pub const HIGH_CAPACITY_BLOCKS: u32 = 2880;

/// Most sectors any supported density packs on one track
/// (`MAX_SECTORS`, `floppy.c:103`).
pub const MAX_SECTORS_PER_TRACK: u8 = 18;

/// Errors tolerated before a transfer is abandoned
/// (`MAX_ERRORS`, `floppy.c:128`).
pub const MAX_ERRORS: u8 = 6;

/// Media density: sectors per track and cylinder count.
///
/// The C table (`fdensity`, `floppy.c:161-177`) holds seven drive and
/// media combinations (`NT`, `floppy.c:134`) of eight parameters each
/// (sectors, cylinders, steps, test sector, rate, motor start, gap,
/// specify byte). Every entry below reproduces the geometry half of
/// one same-drive same-media row; timing parameters stay in the
/// service binary, and both heads are assumed (all modeled media are
/// double-sided).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Density {
    /// Human name, e.g. `"1.44M"`.
    pub name: &'static str,
    /// Sectors on each track.
    pub sectors_per_track: u8,
    /// Cylinders on each side.
    pub cylinders: u8,
    /// Sides (heads) of the diskette.
    pub heads: u8,
}

/// The four densities from `fdensity` (`floppy.c:161-177`).
pub const DENSITIES: [Density; 4] = [
    Density { name: "360K", sectors_per_track: 9, cylinders: 40, heads: 2 },
    Density { name: "720K", sectors_per_track: 9, cylinders: 80, heads: 2 },
    Density { name: "1.2M", sectors_per_track: 15, cylinders: 80, heads: 2 },
    Density { name: "1.44M", sectors_per_track: 18, cylinders: 80, heads: 2 },
];

/// Total 512-byte blocks one density holds.
pub fn density_blocks(density: Density) -> u32 {
    density.sectors_per_track as u32 * density.cylinders as u32 * density.heads as u32
}

/// What to do after one failed transfer attempt.
///
/// Mirrors the C loop (`floppy.c:652-657`): some errors never retry
/// (`err_no_retry`, `floppy.c:120`), errors pile up to `MAX_ERRORS`,
/// and halfway there the head is recalibrated before continuing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryAction {
    /// Try the same transfer again.
    Retry,
    /// Recalibrate the head, then try again.
    RecalibrateAndRetry,
    /// Stop: report the error to the caller.
    GiveUp,
}

/// Per-transfer error counter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    errors: u8,
}

impl RetryPolicy {
    /// A fresh transfer starts with zero recorded errors.
    pub fn new() -> Self {
        RetryPolicy { errors: 0 }
    }

    /// Record one failure and decide what happens next.
    ///
    /// `retryable` is false for errors the C code refuses to retry
    /// (write protection and similar, `err_no_retry`, `floppy.c:120`).
    pub fn record_failure(&mut self, retryable: bool) -> RetryAction {
        if !retryable {
            return RetryAction::GiveUp;
        }
        self.errors += 1;
        if self.errors >= MAX_ERRORS {
            RetryAction::GiveUp
        } else if self.errors == MAX_ERRORS / 2 {
            RetryAction::RecalibrateAndRetry
        } else {
            RetryAction::Retry
        }
    }

    /// How many failures have been recorded so far.
    pub fn errors(&self) -> u8 {
        self.errors
    }
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_density_table_covers_four_media() {
        assert_eq!(DENSITIES.len(), 4);
        assert_eq!(DENSITIES[3].name, "1.44M");
        assert_eq!(DENSITIES[3].sectors_per_track, MAX_SECTORS_PER_TRACK);
    }

    #[test]
    fn test_high_density_holds_2880_blocks() {
        let blocks = density_blocks(DENSITIES[3]);
        assert_eq!(blocks, HIGH_CAPACITY_BLOCKS);
    }

    #[test]
    fn test_retry_gives_up_after_six_errors() {
        let mut policy = RetryPolicy::new();
        for _ in 0..MAX_ERRORS - 1 {
            assert_ne!(policy.record_failure(true), RetryAction::GiveUp);
        }
        assert_eq!(policy.record_failure(true), RetryAction::GiveUp);
    }

    #[test]
    fn test_retry_recalibrates_halfway() {
        let mut policy = RetryPolicy::new();
        assert_eq!(policy.record_failure(true), RetryAction::Retry);
        assert_eq!(policy.record_failure(true), RetryAction::Retry);
        assert_eq!(policy.record_failure(true), RetryAction::RecalibrateAndRetry);
        assert_eq!(policy.record_failure(true), RetryAction::Retry);
    }

    #[test]
    fn test_non_retryable_error_gives_up_at_once() {
        let mut policy = RetryPolicy::new();
        assert_eq!(policy.record_failure(false), RetryAction::GiveUp);
        assert_eq!(policy.errors(), 0);
    }
}
