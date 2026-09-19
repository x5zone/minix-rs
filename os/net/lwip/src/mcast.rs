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
// 注册表（C `mcast_array` 加空闲链，`mcast.c:43-66` 的服务半）：成员记录
// 定长总量（两族各 64），插队走 `early_join_check` 的三道早检。
// ---------------------------------------------------------------------------

/// 一条在役成员：套接字号、接口行号、组地址。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Member {
    /// 服务侧套接字号（`SockId::raw`，关户清账的键）。
    pub socket: u32,
    /// 服务侧接口表行号（第 14 篇）。
    pub ifdev: u16,
    /// 组地址。
    pub group: crate::lwip_port::StackIpAddr,
}

/// 组播成员注册表：定长记录槽，空闲槽复用（C 的空闲链在定长数组上的
/// 等价形状——插入取首个空槽，退出即清槽）。
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
    /// `mcast_init`（`mcast.c:55-66`）：全部槽位清空。
    pub fn new() -> Self {
        McastRegistry { records: alloc::vec![None; TOTAL_MEMBERSHIPS] }
    }

    /// 空闲记录数（`early_join_check` 的第三道输入）。
    pub fn free_records(&self) -> usize {
        self.records.iter().filter(|s| s.is_none()).count()
    }

    /// 入组：先过三道早检（重复、单套接字上限、全局余量），通过即占
    /// 一个空槽。网络侧的组管理协议交换随接口批次接线。
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

    /// 退组：找到即清槽，返回是否确实在册。
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

    /// 关户清账：套接字关闭时它的全部成员记录一并释放，返回释放条数
    /// （C `mcast_head` 的清理半）。
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
        // 别的套接字可共享同一组（成员记录按套接字独立，无一对一约束）。
        assert!(reg.join(2, 0, v4([224, 0, 0, 1])).is_ok());
    }

    #[test]
    fn test_global_exhaustion_and_close_cleanup() {
        let mut reg = McastRegistry::new();
        // 两个套接字各持八组轮流填，填满 128 条。
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
        // 关户清账：1 号套接字的全部记录释放后，全局又有余量。
        let freed = reg.leave_socket(1);
        assert!(freed > 0);
        assert!(reg.join(99, 0, v4([224, 0, 0, 200])).is_ok());
    }
}

