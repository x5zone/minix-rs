//! Multicast membership policy: global and per-socket limits, join rules.
//!
//! C correspondence: `minix3/minix/net/lwip/mcast.c` (283 lines) with the
//! size definitions in `minix3/minix/lib/liblwip/lib/lwipopts.h:208-213`
//! and `:535-541`. Membership storage, interface lookups, and the underlying
//! group management protocol exchanges stay in the service binary. This
//! module owns the portion that can be decided from numbers alone: how many
//! memberships exist in total, how many one socket may hold, and which join
//! failures are reported without touching the network.
//!
//! The design mirrors how the reference system separates concerns. The
//! service tracks memberships independently instead of linking directly into
//! the underlying stack structures, because the stack never intended its
//! internal membership lists for external use. Multiple sockets may join the
//! same group, so there is deliberately no one-to-one relationship between
//! service membership records and stack membership records.

/// Largest memberships for version 4 groups (`NR_IPV4_MCAST_GROUP`, 64,
/// `lwipopts.h:211`).
pub const MAX_VERSION4_GROUPS: usize = 64;

/// Largest memberships for version 6 groups (`NR_IPV6_MCAST_GROUP`, 64,
/// `lwipopts.h:538`).
pub const MAX_VERSION6_GROUPS: usize = 64;

/// Total membership records (`mcast_array`, `mcast.c:47`, sized as the sum
/// of both families).
pub const TOTAL_MEMBERSHIPS: usize = MAX_VERSION4_GROUPS + MAX_VERSION6_GROUPS;

/// Largest groups one socket may join (`MAX_GROUPS_PER_SOCKET`, 8,
/// `mcast.c:41`, chosen so one socket cannot consume half of one family).
pub const MAX_GROUPS_PER_SOCKET: usize = 8;

/// Join failure reasons that can be decided without touching the network
/// (`mcast_join`, `mcast.c:96-150`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinRejection {
    /// The candidate is already joined on the same interface
    /// (`mcast.c:138-139`, reported as already-exists).
    AlreadyJoined,
    /// The socket already holds the per-socket maximum
    /// (`mcast.c:143-144`, reported as no-buffer-space).
    SocketFull,
    /// No free membership record remains globally
    /// (`mcast.c:147-148`, reported as no-buffer-space).
    TableFull,
}

/// Decide the early join outcome from counts alone.
///
/// Inputs mirror the checks at `mcast.c:133-148`: whether the same interface
/// and group pair is already present, how many groups the socket already
/// holds, and how many free records remain globally. Address validity,
/// interface capability, and routing selection stay in the service binary
/// because they need the interface table and the protocol stack. Returning
/// `None` means the early checks pass and the caller may proceed to the
/// stack join step.
pub fn early_join_check(
    already_joined: bool,
    socket_groups: usize,
    free_records: usize,
) -> Option<JoinRejection> {
    if already_joined {
        return Some(JoinRejection::AlreadyJoined);
    }
    if socket_groups >= MAX_GROUPS_PER_SOCKET {
        return Some(JoinRejection::SocketFull);
    }
    if free_records == 0 {
        return Some(JoinRejection::TableFull);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_limits_match_options_and_source() {
        assert_eq!(MAX_VERSION4_GROUPS, 64);
        assert_eq!(MAX_VERSION6_GROUPS, 64);
        assert_eq!(TOTAL_MEMBERSHIPS, 128);
        assert_eq!(MAX_GROUPS_PER_SOCKET, 8);
    }

    #[test]
    fn test_duplicate_join_is_rejected_first() {
        assert_eq!(
            early_join_check(true, 8, 0),
            Some(JoinRejection::AlreadyJoined)
        );
    }

    #[test]
    fn test_socket_limit_is_checked_before_global_table() {
        assert_eq!(
            early_join_check(false, 8, 100),
            Some(JoinRejection::SocketFull)
        );
        assert_eq!(
            early_join_check(false, 7, 100),
            None
        );
    }

    #[test]
    fn test_global_exhaustion_is_reported_when_socket_has_room() {
        assert_eq!(
            early_join_check(false, 0, 0),
            Some(JoinRejection::TableFull)
        );
        assert_eq!(early_join_check(false, 0, 1), None);
    }
}
