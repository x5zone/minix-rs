//! Packet capture policy: buffer bounds, filter limits, version check.
//!
//! C correspondence: `minix3/minix/net/lwip/bpfdev.c` (1365 lines) with the
//! filter interpreter in `minix3/minix/net/lwip/bpf_filter.c` (561 lines,
//! ported from the reference system). Device storage, packet buffering, and
//! program checking stay in the service binary. This module owns the portion
//! that can be decided from numbers alone: which buffer sizes pass, how many
//! filter instructions are allowed, and which filter version is accepted.
//!
//! Capture devices assume a single observing process per device and do not
//! support concurrent calls. That assumption lives in the service binary;
//! the numeric policy here is independent of it.

/// Smallest capture buffer (`BPF_BUF_MIN`, `bpfdev.c:44`: word-aligned
/// header size).
pub const BUFFER_MIN: usize = 32;

/// Default capture buffer (`BPF_BUF_DEF`, `bpfdev.c:45`).
pub const BUFFER_DEFAULT: usize = 32768;

/// Largest capture buffer (`BPF_BUF_MAX`, `bpfdev.c:46`).
pub const BUFFER_MAX: usize = 262144;

/// Largest filter program (`BPF_MAXINSNS`, 512, `bpf.h:67`, enforced at
/// `bpfdev.c:711` and `bpf_filter.c:418`).
pub const MAX_INSTRUCTIONS: usize = 512;

/// Accepted filter major version (`BPF_MAJOR_VERSION`, 1, `bpf.h:114`,
/// checked at `bpfdev.c:36`).
pub const VERSION_MAJOR: u32 = 1;

/// Accepted filter minor version (`BPF_MINOR_VERSION`, 1, `bpf.h:115`).
pub const VERSION_MINOR: u32 = 1;

/// Clamp a requested buffer size into the allowed interval
/// (`bpfdev_ioctl`, `bpfdev.c:799-805`: below the minimum becomes the
/// minimum, above the maximum becomes the maximum, then word-aligned).
pub fn clamp_buffer_size(requested: usize) -> usize {
    const WORD: usize = 4;
    let bounded = requested.clamp(BUFFER_MIN, BUFFER_MAX);
    bounded.div_ceil(WORD) * WORD
}

/// Whether a filter program length passes (non-empty, up to the maximum).
pub fn instruction_count_allowed(count: usize) -> bool {
    count > 0 && count <= MAX_INSTRUCTIONS
}

/// Whether a filter version is accepted.
pub fn version_allowed(major: u32, minor: u32) -> bool {
    major == VERSION_MAJOR && minor == VERSION_MINOR
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffer_bounds_match_device_source() {
        assert_eq!(BUFFER_DEFAULT, 32768);
        assert_eq!(BUFFER_MAX, 262144);
        assert_eq!(clamp_buffer_size(0), BUFFER_MIN);
        assert_eq!(clamp_buffer_size(1_000_000), BUFFER_MAX);
        assert_eq!(clamp_buffer_size(32768), 32768);
    }

    #[test]
    fn test_buffer_clamp_aligns_to_words() {
        assert_eq!(clamp_buffer_size(33) % 4, 0);
        assert_eq!(clamp_buffer_size(34) % 4, 0);
    }

    #[test]
    fn test_instruction_limit_matches_header() {
        assert_eq!(MAX_INSTRUCTIONS, 512);
        assert!(!instruction_count_allowed(0));
        assert!(instruction_count_allowed(512));
        assert!(!instruction_count_allowed(513));
    }

    #[test]
    fn test_version_matches_header() {
        assert!(version_allowed(1, 1));
        assert!(!version_allowed(2, 1));
        assert!(!version_allowed(1, 2));
    }
}
