//! Multicast membership policy: global and per-socket limits, join rules,
//! plus the service-side membership registry (`mcast_init` half).
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

use alloc::vec::Vec;

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


// ---------------------------------------------------------------------------
// Registry (the service half of C's `mcast_array` plus free list,
// `mcast.c:43-66`): a fixed total of membership records (64 per family),
// and a join goes through the three early checks of `early_join_check`.
// ---------------------------------------------------------------------------

/// One live membership: socket id, interface row, group address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Member {
    /// Service-side socket id (`SockId::raw`, the key for close-time cleanup).
    pub socket: u32,
    /// Service-side interface table row (doc 14).
    pub ifdev: u16,
    /// Group address.
    pub group: crate::lwip_port::StackIpAddr,
}

/// Multicast membership registry: fixed-size record slots, free slots
/// reused (the fixed-array equivalent of C's free list — insert takes the
/// first empty slot, leave clears that slot).
#[derive(Debug)]
pub struct McastRegistry {
    records: Vec<Option<Member>>,
}

impl Default for McastRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl McastRegistry {
    /// `mcast_init` (`mcast.c:55-66`): all slots cleared.
    pub fn new() -> Self {
        McastRegistry { records: alloc::vec![None; TOTAL_MEMBERSHIPS] }
    }

    /// Number of free records (the third input of `early_join_check`).
    pub fn free_records(&self) -> usize {
        self.records.iter().filter(|s| s.is_none()).count()
    }

    /// Join: pass the three early checks first (duplicate, per-socket
    /// maximum, global room), then occupy one empty slot. The network-side
    /// group management protocol exchange is wired with the interface work.
    pub fn join(
        &mut self,
        socket: u32,
        ifdev: u16,
        group: crate::lwip_port::StackIpAddr,
    ) -> Result<(), JoinRejection> {
        let already = self
            .records
            .iter()
            .flatten()
            .any(|m| m.socket == socket && m.ifdev == ifdev && m.group == group);
        let socket_groups = self
            .records
            .iter()
            .flatten()
            .filter(|m| m.socket == socket)
            .count();
        if let Some(rejection) = early_join_check(already, socket_groups, self.free_records()) {
            return Err(rejection);
        }
        let slot = self
            .records
            .iter_mut()
            .position(|s| s.is_none())
            .expect("早检通过则必有空槽");
        self.records[slot] = Some(Member { socket, ifdev, group });
        Ok(())
    }

    /// Leave: clear the slot if found, returning whether it was present.
    pub fn leave(
        &mut self,
        socket: u32,
        ifdev: u16,
        group: crate::lwip_port::StackIpAddr,
    ) -> bool {
        let pos = self.records.iter().position(|s| {
            matches!(s, Some(m) if m.socket == socket && m.ifdev == ifdev && m.group == group)
        });
        if let Some(p) = pos {
            self.records[p] = None;
            true
        } else {
            false
        }
    }

    /// Close-time cleanup: release all of a socket's membership records when
    /// it closes, returning how many were released (the cleanup half of C's
    /// `mcast_head`).
    pub fn leave_socket(&mut self, socket: u32) -> usize {
        let mut freed = 0;
        for slot in self.records.iter_mut() {
            if let Some(m) = slot
                && m.socket == socket
            {
                *slot = None;
                freed += 1;
            }
        }
        freed
    }
}

#[cfg(test)]
mod registry_tests {
    use super::*;
    use crate::lwip_port::StackIpAddr;

    fn v4(octets: [u8; 4]) -> StackIpAddr {
        StackIpAddr::V4(octets)
    }

    #[test]
    fn test_join_leave_roundtrip_and_duplicate_rejection() {
        let mut reg = McastRegistry::new();
        assert!(reg.join(1, 0, v4([224, 0, 0, 1])).is_ok());
        assert_eq!(
            reg.join(1, 0, v4([224, 0, 0, 1])),
            Err(JoinRejection::AlreadyJoined),
            "同套接字同接口同组重复入组被拒"
        );
        assert!(reg.leave(1, 0, v4([224, 0, 0, 1])));
        assert!(!reg.leave(1, 0, v4([224, 0, 0, 1])), "不在册的退组返回假");
        assert!(reg.join(1, 0, v4([224, 0, 0, 1])).is_ok(), "退组后槽位复用");
    }

    #[test]
    fn test_socket_limit_and_cross_socket_sharing() {
        let mut reg = McastRegistry::new();
        for i in 0..MAX_GROUPS_PER_SOCKET {
            assert!(reg
                .join(1, 0, v4([224, 0, 0, 1 + i as u8]))
                .is_ok());
        }
        assert_eq!(
            reg.join(1, 0, v4([224, 0, 0, 99])),
            Err(JoinRejection::SocketFull),
            "单套接字八组上限"
        );
        // Another socket may share the same group (records are per socket,
        // with no one-to-one constraint).
        assert!(reg.join(2, 0, v4([224, 0, 0, 1])).is_ok());
    }

    #[test]
    fn test_global_exhaustion_and_close_cleanup() {
        let mut reg = McastRegistry::new();
        // Fill all 128 records by rotating sockets that hold eight groups each.
        let mut joined = 0;
        'fill: for group in 1.. {
            for socket in 1..=32u32 {
                if reg
                    .join(socket, 0, v4([224, 0, 0, group as u8]))
                    .is_ok()
                {
                    joined += 1;
                    if joined == TOTAL_MEMBERSHIPS {
                        break 'fill;
                    }
                }
            }
        }
        assert_eq!(joined, TOTAL_MEMBERSHIPS);
        assert_eq!(
            reg.join(99, 0, v4([224, 0, 0, 200])),
            Err(JoinRejection::TableFull)
        );
        // Close-time cleanup: once socket 1's records are released, global
        // room exists again.
        let freed = reg.leave_socket(1);
        assert!(freed > 0);
        assert!(reg.join(99, 0, v4([224, 0, 0, 200])).is_ok());
    }
}

