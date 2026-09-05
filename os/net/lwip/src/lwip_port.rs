//! Third-party stack port surface: build subset, key options, hooks, patches.
//!
//! C correspondence: `minix3/minix/lib/liblwip/` (build subset under
//! `dist/src`, glue under `lib/`, four patches under `patches/`). No packet
//! code lives here. This module owns the portion that can be stated as
//! numbers and names: which build subset is compiled, which option values
//! the service relies on, which four hooks the service provides, and which
//! four patches are applied.
//!
//! [ARCH N-1]: the rewrite replaces the third-party stack with a Rust
//! stack (candidate: smoltcp, a purpose-built minimal stack, or a staged
//! foreign-function layer replaced piece by piece). The constants below map
//! the glue options the replacement must honor so behavior stays compatible
//! while the implementation changes.

/// Compiled build subset (68 `.c` files / 58232 lines across the core,
/// Internet Protocol version 4, version 6, and network interface groups).
pub const BUILD_C_FILES: usize = 68;

/// Single-threaded stack, no operating-system layer (`NO_SYS`, 1,
/// `lwipopts.h:14`).
pub const OPTION_NO_SYS: u8 = 1;

/// Custom pool replaces the pool allocator (`PBUF_POOL_SIZE`, 0,
/// `lwipopts.h:80`).
pub const OPTION_POOL_SIZE: usize = 0;

/// Largest segment size (`TCP_MSS`, 1460, `lwipopts.h:259`).
pub const TCP_MAX_SEGMENT: usize = 1460;

/// Receive window (`TCP_WND`, 16384, `lwipopts.h:267`).
pub const TCP_WINDOW: usize = 16384;

/// Send buffer (`TCP_SND_BUF`, 11 times the segment size, `lwipopts.h:282`).
pub const TCP_SEND_BUFFER: usize = 11 * TCP_MAX_SEGMENT;

/// Hooks the service provides to the stack (`lwiphooks.h`: sequence-number
/// generation, two route overrides, two gateway lookups).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackHook {
    /// Initial sequence number generation.
    TcpSequenceNumber,
    /// Version 4 route override.
    RouteVersion4,
    /// Version 6 route override.
    RouteVersion6,
    /// Gateway lookup (address resolution and neighbor discovery share one
    /// policy entry here; the service fans out to both).
    Gateway,
}

/// All four hooks.
pub const ALL_HOOKS: [StackHook; 4] = [
    StackHook::TcpSequenceNumber,
    StackHook::RouteVersion4,
    StackHook::RouteVersion6,
    StackHook::Gateway,
];

/// Applied patches, in directory order.
pub const PATCHES: [&str; 4] = [
    "0001-MINIX-3-only-mark-various-functions-as-weak",
    "0002-MINIX-3-only-control-IP-forwarding-at-run-time",
    "0003-MINIX-3-only-ignore-IPv6-Router-Advertisements",
    "0004-MINIX-3-only-avoid-large-contiguous-allocations",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_options_match_glue_header() {
        assert_eq!(BUILD_C_FILES, 68);
        assert_eq!(OPTION_NO_SYS, 1);
        assert_eq!(OPTION_POOL_SIZE, 0);
        assert_eq!(TCP_MAX_SEGMENT, 1460);
        assert_eq!(TCP_WINDOW, 16384);
        assert_eq!(TCP_SEND_BUFFER, 11 * 1460);
    }

    #[test]
    fn test_hooks_cover_glue_header() {
        assert_eq!(ALL_HOOKS.len(), 4);
    }

    #[test]
    fn test_patches_cover_patch_directory() {
        assert_eq!(PATCHES.len(), 4);
    }
}
