//! `sockops` — 套接字路的服务半：sdev 请求翻译、建户、挂起续答。
//!
//! C 对应物三处：`alloc_socket`（`lwip.c:151-190`，域与类型分派到各套
//! 接字模块，RAW 要根身份）、sockdriver 框架的"立即回答或挂起记账"
//! （可挂起表 `sockdriver.c:8-26`，`Continuation::new` 的 Rust 同位）、
//! 以及 sockevent 的唤醒回执（`SEV_*` 事件位到期/就绪后按请求的回复
//! 形状续答）。映射边界见 `25-smoltcp-shim.md` §2/§3。
//!
//! 本模块当前翻译的请求：`SDEV_SOCKET`（建户——立即回答，C 同样不挂
//! 起）。其余请求族（读写/连接/收发数据面）随各自家族批次逐条过墙；
//! 对它们暂按"未接线"（ENOSYS）以通用回复形状回答——VFS 侧的各续接
//! 体对不认识的回复号一律折 EIO，不会把通用形状误读成数据。

use minix_netdriver::sockid::{self, SockId};
use minix_netdriver::socktable::SockTable;
use minix_sockdriver::sdev::SdevReply;
use minix_types::Message;

use crate::lwip_port::{Stack, StackFamily};
use crate::startup::raw_allowed;

/// 域常量（Minix3 `sys/sys/socket.h:179-331` 的 `PF_*` 值：套接字路按
/// 域分派，`alloc_socket` 的 `lwip.c:156-188` 同一张表）。
pub mod domain {
    /// `PF_INET`（2，`sys/sys/socket.h:179`）。
    pub const INET: i32 = 2;
    /// `PF_LINK`（18，`sys/sys/socket.h:196` 的 `AF_LINK`）。
    pub const LINK: i32 = 18;
    /// `PF_INET6`（24，`sys/sys/socket.h:206` 的 `AF_INET6`）。
    pub const INET6: i32 = 24;
    /// `PF_ROUTE`（34，`sys/sys/socket.h:222` 的 `AF_ROUTE`）。
    pub const ROUTE: i32 = 34;
}

/// 类型常量（`sys/socket.h` 的 `SOCK_*`：流 1、数据报 2、原始 3）。
pub mod sock_type {
    /// `SOCK_STREAM`（1）。
    pub const STREAM: i32 = 1;
    /// `SOCK_DGRAM`（2）。
    pub const DGRAM: i32 = 2;
    /// `SOCK_RAW`（3）。
    pub const RAW: i32 = 3;
}

/// 一条 `SDEV_SOCKET` 请求的解码结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SocketRequest {
    /// 请求方带回的标识（VFS 侧放的是调用方端点，回复原样带回）。
    pub req_id: i32,
    /// 通讯域（[`domain`] 常量）。
    pub domain: i32,
    /// 套接字类型（[`sock_type`] 常量）。
    pub sock_type: i32,
    /// 协议号（0 = 缺省）。
    pub protocol: i32,
    /// 发起套接字调用的用户进程端点（RAW 的根身份检查用它）。
    pub user_endpt: i32,
}

/// 解码 `SDEV_SOCKET` 载荷：`mess_vfs_lsockdriver_socket { req_id@0;
/// domain@4; type@8; protocol@12; user_endpt@16 }`（ipc.h:2329-2337）。
pub fn decode_socket(msg: &Message) -> Option<SocketRequest> {
    // SAFETY: 该请求的载荷按上述域序写在消息负载区（无专属 union 成员）。
    let raw = unsafe { &msg.m_u.raw };
    let word = |at: usize| i32::from_le_bytes(raw[at..at + 4].try_into().unwrap());
    Some(SocketRequest {
        req_id: word(0),
        domain: word(4),
        sock_type: word(8),
        protocol: word(12),
        user_endpt: word(16),
    })
}

/// 组装 `SDEV_SOCKET_REPLY`：`mess_lsockdriver_vfs_socket_reply
/// { req_id@0; sock_id@4; sock_id2@8 }`——`sock_id` 为负即错误（VFS 侧
/// 按此判读），非成对请求 `sock_id2` 恒 -1。
pub fn socket_reply(req_id: i32, sock_id: i32) -> Message {
    let mut m = Message {
        m_type: SdevReply::SocketReply as i32,
        ..Message::default()
    };
    // SAFETY: 回复载荷按上述域序写在消息负载区。
    unsafe {
        let raw = &mut m.m_u.raw;
        raw[0..4].copy_from_slice(&req_id.to_le_bytes());
        raw[4..8].copy_from_slice(&sock_id.to_le_bytes());
        raw[8..12].copy_from_slice(&(-1i32).to_le_bytes());
    }
    m
}

/// 组装通用回复（`mess_lsockdriver_vfs_reply { req_id; status }`——
/// **状态在第二格**）：建户之外的请求族暂以 ENOSYS 走这条形状诚实
/// 回答；VFS 侧各续接体对不认识的回复号折 EIO，不会误读成数据。
pub fn simple_reply(req_id: i32, status: i32) -> Message {
    let mut m = Message {
        m_type: SdevReply::Reply as i32,
        ..Message::default()
    };
    // SAFETY: 回复载荷按上述域序写在消息负载区。
    unsafe {
        let raw = &mut m.m_u.raw;
        raw[0..4].copy_from_slice(&req_id.to_le_bytes());
        raw[4..8].copy_from_slice(&status.to_le_bytes());
    }
    m
}

/// 域与类型到栈家族的分派（`alloc_socket` 的因特网半，`lwip.c:156-172`）。
/// 流→TCP、数据报→UDP、原始→RAW（根身份门在调用方）；其余类型
/// EPROTOTYPE（`lwip.c:170`）。
pub fn internet_family(sock_type: i32, is_root: bool) -> Result<StackFamily, i32> {
    match sock_type {
        sock_type::STREAM => Ok(StackFamily::Tcp),
        sock_type::DGRAM => Ok(StackFamily::Udp),
        sock_type::RAW => {
            if raw_allowed(is_root) {
                Ok(StackFamily::Raw)
            } else {
                Err(minix_types::EACCES)
            }
        }
        _ => Err(minix_types::EPROTOTYPE),
    }
}

/// 每类套接字的下一个栈内下标（C 各套接字模块各自持有计数；这里收拢
/// 成按类分配，扫描服务表跳过在用号防串号）。
#[derive(Debug)]
pub struct IdAllocator {
    next: [u32; 5],
}

impl Default for IdAllocator {
    fn default() -> Self {
        Self::new()
    }
}

impl IdAllocator {
    /// 全零起。
    pub fn new() -> Self {
        IdAllocator { next: [0; 5] }
    }

    /// 按家族取类基（TCP/UDP/RAW 之外的家族不在栈内建户，返回 `None`）。
    fn class_base(family: StackFamily) -> Option<i32> {
        let class = match family {
            StackFamily::Tcp => sockid::SOCKID_TCP,
            StackFamily::Udp => sockid::SOCKID_UDP,
            StackFamily::Raw => sockid::SOCKID_RAW,
            StackFamily::Icmp => return None,
        };
        Some(class)
    }

    /// 分配一个未占用的 `SockId`：从该类计数器起线性扫描服务表，越过
    /// 在用号（`sockid` 命名空间 20 位下标，`lwip.h:58-62` 的五类基值）。
    pub fn allocate(&mut self, table: &SockTable, family: StackFamily) -> Option<SockId> {
        let base = Self::class_base(family)?;
        let class = sockid::SockClass::from_base(base)?;
        let mut idx = self.next[slots(class)] % (sockid::INDEX_MASK);
        for _ in 0..=sockid::INDEX_MASK {
            let candidate = SockId::from_class(class, idx)?;
            if !table.contains(candidate) {
                self.next[slots(class)] = (idx + 1) % (sockid::INDEX_MASK);
                return Some(candidate);
            }
            idx = (idx + 1) % (sockid::INDEX_MASK);
        }
        None
    }
}

fn slots(class: sockid::SockClass) -> usize {
    match class {
        sockid::SockClass::Tcp => 0,
        sockid::SockClass::Udp => 1,
        sockid::SockClass::Raw => 2,
        sockid::SockClass::Rt => 3,
        sockid::SockClass::Lnk => 4,
    }
}

/// 建户（C `alloc_socket` 的因特网半 + 模块 `*_socket` 的登记半）：
/// 域门 → 类型分派 → 栈内开户 → 命名空间分配 → 服务表登记。返回线上
/// 套接字号（`SockId::raw`，回复载荷原样携带）。
///
/// `PF_ROUTE`/`PF_LINK` 两域的模块（rtsock/lnksock）随后批接线，暂按
/// 未接线（ENOSYS）回答；未知域按 C 回 EAFNOSUPPORT（`lwip.c:186`）。
pub fn open_socket(
    stack: &mut dyn Stack,
    table: &mut SockTable,
    alloc: &mut IdAllocator,
    req: &SocketRequest,
    is_root: bool,
) -> Result<i32, i32> {
    let family = match req.domain {
        domain::INET | domain::INET6 => internet_family(req.sock_type, is_root)?,
        domain::ROUTE | domain::LINK => return Err(minix_types::ENOSYS),
        _ => return Err(minix_types::EAFNOSUPPORT),
    };
    let stack_socket = stack.open(family)?;
    let Some(id) = alloc.allocate(table, family) else {
        // 命名空间耗尽：撤栈内套接字再报错，不留半开户。
        let _ = stack.close(stack_socket);
        return Err(minix_types::ENOSPC);
    };
    if table.add(id).is_err() {
        let _ = stack.close(stack_socket);
        return Err(minix_types::EAGAIN);
    }
    Ok(id.raw())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stack::SmoltcpStack;
    use alloc::vec;

    /// 现成栈加空表：建户用的最小现场。
    fn fixture() -> (SmoltcpStack, SockTable, IdAllocator) {
        (SmoltcpStack::new(1, 0), SockTable::new(), IdAllocator::new())
    }

    #[test]
    fn test_open_stream_gets_tcp_class_id() {
        let (mut stack, mut table, mut alloc) = fixture();
        let req = SocketRequest {
            req_id: 7,
            domain: domain::INET,
            sock_type: sock_type::STREAM,
            protocol: 0,
            user_endpt: 100,
        };
        let id = open_socket(&mut stack, &mut table, &mut alloc, &req, false).unwrap();
        assert_eq!(sockid::SockId::from_raw(id).unwrap().class_base(), sockid::SOCKID_TCP);
        assert!(table.contains(sockid::SockId::from_raw(id).unwrap()));
    }

    #[test]
    fn test_open_dgram_and_raw_distinct_classes() {
        let (mut stack, mut table, mut alloc) = fixture();
        let dgram = SocketRequest {
            req_id: 1, domain: domain::INET, sock_type: sock_type::DGRAM,
            protocol: 0, user_endpt: 100,
        };
        let raw_req = SocketRequest {
            req_id: 2, domain: domain::INET6, sock_type: sock_type::RAW,
            protocol: 0, user_endpt: 0, // 根身份
        };
        let udp_id = open_socket(&mut stack, &mut table, &mut alloc, &dgram, false).unwrap();
        let raw_id = open_socket(&mut stack, &mut table, &mut alloc, &raw_req, true).unwrap();
        assert_eq!(
            sockid::SockId::from_raw(udp_id).unwrap().class_base(),
            sockid::SOCKID_UDP
        );
        assert_eq!(
            sockid::SockId::from_raw(raw_id).unwrap().class_base(),
            sockid::SOCKID_RAW
        );
    }

    #[test]
    fn test_raw_needs_root_and_bad_type_rejected() {
        let (mut stack, mut table, mut alloc) = fixture();
        let raw_req = SocketRequest {
            req_id: 1, domain: domain::INET, sock_type: sock_type::RAW,
            protocol: 0, user_endpt: 100,
        };
        assert_eq!(
            open_socket(&mut stack, &mut table, &mut alloc, &raw_req, false).unwrap_err(),
            minix_types::EACCES,
            "非根开 RAW 被拒（lwip.c:166-167）"
        );
        let bogus = SocketRequest {
            req_id: 2, domain: domain::INET, sock_type: 7,
            protocol: 0, user_endpt: 100,
        };
        assert_eq!(
            open_socket(&mut stack, &mut table, &mut alloc, &bogus, true).unwrap_err(),
            minix_types::EPROTOTYPE,
            "未知类型按 EPROTOTYPE（lwip.c:170）"
        );
    }

    #[test]
    fn test_route_link_domains_answer_enosys_and_unknown_domain_eafnosupport() {
        let (mut stack, mut table, mut alloc) = fixture();
        let route = SocketRequest {
            req_id: 1, domain: domain::ROUTE, sock_type: sock_type::DGRAM,
            protocol: 0, user_endpt: 100,
        };
        assert_eq!(
            open_socket(&mut stack, &mut table, &mut alloc, &route, false).unwrap_err(),
            minix_types::ENOSYS,
            "rtsock 随后批接线，先诚实回答未接线"
        );
        let alien = SocketRequest {
            req_id: 2, domain: 42, sock_type: sock_type::STREAM,
            protocol: 0, user_endpt: 100,
        };
        assert_eq!(
            open_socket(&mut stack, &mut table, &mut alloc, &alien, false).unwrap_err(),
            minix_types::EAFNOSUPPORT
        );
    }

    #[test]
    fn test_decode_and_reply_roundtrip() {
        let mut msg = Message::default();
        // SAFETY(test): 按 mess_vfs_lsockdriver_socket 域序填。
        unsafe {
            let raw = &mut msg.m_u.raw;
            raw[0..4].copy_from_slice(&55i32.to_le_bytes());
            raw[4..8].copy_from_slice(&domain::INET.to_le_bytes());
            raw[8..12].copy_from_slice(&sock_type::STREAM.to_le_bytes());
            raw[12..16].copy_from_slice(&0i32.to_le_bytes());
            raw[16..20].copy_from_slice(&100i32.to_le_bytes());
        }
        let req = decode_socket(&msg).unwrap();
        assert_eq!(req.req_id, 55);
        assert_eq!(req.domain, domain::INET);
        assert_eq!(req.user_endpt, 100);
        // 回复形状：req_id 原样带回，sock_id2 恒 -1。
        let reply = socket_reply(55, sockid::SOCKID_TCP + 3);
        // SAFETY(test): 按 mess_lsockdriver_vfs_socket_reply 域序读。
        let raw = unsafe { &reply.m_u.raw };
        assert_eq!(reply.m_type, SdevReply::SocketReply as i32);
        assert_eq!(i32::from_le_bytes(raw[0..4].try_into().unwrap()), 55);
        assert_eq!(i32::from_le_bytes(raw[4..8].try_into().unwrap()), sockid::SOCKID_TCP + 3);
        assert_eq!(i32::from_le_bytes(raw[8..12].try_into().unwrap()), -1);
    }

    #[test]
    fn test_simple_reply_carries_status_in_second_word() {
        let reply = simple_reply(9, -(minix_types::ENOSYS));
        assert_eq!(reply.m_type, SdevReply::Reply as i32);
        // SAFETY(test): { req_id@0; status@4 }。
        let raw = unsafe { &reply.m_u.raw };
        assert_eq!(i32::from_le_bytes(raw[4..8].try_into().unwrap()), -(minix_types::ENOSYS));
    }
}
