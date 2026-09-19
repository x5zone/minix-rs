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

use alloc::vec::Vec;

use minix_netdriver::sockid::{self, SockId};
use minix_netdriver::socktable::{Continuation, SockTable};
use minix_sockdriver::sdev::{SdevReply, SdevRequest};
use minix_sockdriver::sockevent::SocketEvent;
use minix_types::{Endpoint, Message};

use crate::lwip_port::{Stack, StackFamily, StackSocket};
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

/// 错误码的线上归一：`minix_types::E*` 常量是正号（crate 惯例），
/// `util::ERR_*` 与栈特征返回的错误是负号线上形状——两种来源在路上
/// 汇合时统一折成"负号或零"，回复载荷不再二次翻转。
fn wire(code: i32) -> i32 {
    if code > 0 {
        -code
    } else {
        code
    }
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

/// 调用方身份的缝（C `util_is_root`——服务持有 fproc 副本查 uid；本
/// 模型的进程表面随系统进程批接线，生产实现先 fail-closed 恒非根，
/// RAW 建户回 EACCES；测试注 canned 实现）。
pub trait IdentitySource {
    /// 该端点是否根身份（RAW 建户的门）。
    fn is_root(&self, user: Endpoint) -> bool;
}

/// 生产实现：fail-closed 恒非根（C `util_is_root` 的进程表面随系统
/// 进程批接线，接线前 RAW 建户一律 EACCES——不假装放行）。
#[derive(Debug, Default, Clone, Copy)]
pub struct FailClosedIdentity;

impl IdentitySource for FailClosedIdentity {
    fn is_root(&self, _user: Endpoint) -> bool {
        false
    }
}

/// canned 替身：按构造参数回答（服务二进制测试用它放行/拒绝 RAW）。
#[derive(Debug, Clone, Copy)]
pub struct CannedIdentity(pub bool);

impl IdentitySource for CannedIdentity {
    fn is_root(&self, _user: Endpoint) -> bool {
        self.0
    }
}

/// 类基到家族（线上套接字号反查栈家族用）。
fn family_of_class(class: sockid::SockClass) -> Option<StackFamily> {
    match class {
        sockid::SockClass::Tcp => Some(StackFamily::Tcp),
        sockid::SockClass::Udp => Some(StackFamily::Udp),
        sockid::SockClass::Raw => Some(StackFamily::Raw),
        sockid::SockClass::Rt | sockid::SockClass::Lnk => None,
    }
}

/// 家族到类基（建户时把栈内下标直接当作命名空间下标——两类下标同源
/// 同长（栈 16 位、命名空间 20 位），线上号反查即闭式映射，免维护
/// 第二张映射表）。
fn class_base_of(family: StackFamily) -> Option<i32> {
    match family {
        StackFamily::Tcp => Some(sockid::SOCKID_TCP),
        StackFamily::Udp => Some(sockid::SOCKID_UDP),
        StackFamily::Raw => Some(sockid::SOCKID_RAW),
        StackFamily::Icmp => None,
    }
}

/// 线上套接字号反查栈内句柄（家族随类、下标即栈内下标）。
pub fn stack_socket_of(raw_id: i32) -> Option<StackSocket> {
    let id = SockId::from_raw(raw_id)?;
    let family = family_of_class(id.class()?)?;
    Some(StackSocket::new(family, u16::try_from(id.index()).unwrap_or(u16::MAX)))
}

// ---------------------------------------------------------------------------
// 地址翻译：Minix `sockaddr_in`（16 字节，`netinet/in.h:239-245`——
// sin_len u8@0、sin_family u8@1、sin_port 网络序 u16@2、sin_addr@4、
// sin_zero[8]@8）与墙端点的双向翻译。
// ---------------------------------------------------------------------------

/// `sockaddr_in` 的字节数（16）。
pub const INET_ADDR_LEN: usize = 16;

/// 解码 16 字节 `sockaddr_in`；族字节非 `AF_INET`（2）拒绝。
pub fn decode_inet_endpoint(bytes: &[u8]) -> Option<crate::lwip_port::StackEndpoint> {
    if bytes.len() < INET_ADDR_LEN || bytes[1] != domain::INET as u8 {
        return None;
    }
    let port = u16::from_be_bytes([bytes[2], bytes[3]]);
    Some(crate::lwip_port::StackEndpoint {
        addr: Some(crate::lwip_port::StackIpAddr::V4([
            bytes[4], bytes[5], bytes[6], bytes[7],
        ])),
        port,
    })
}

/// 编码 16 字节 `sockaddr_in`（sin_len = 16，端口转网络序）。
pub fn encode_inet_endpoint(endpoint: crate::lwip_port::StackEndpoint) -> [u8; INET_ADDR_LEN] {
    let mut out = [0u8; INET_ADDR_LEN];
    out[0] = INET_ADDR_LEN as u8;
    out[1] = domain::INET as u8;
    out[2..4].copy_from_slice(&endpoint.port.to_be_bytes());
    if let Some(crate::lwip_port::StackIpAddr::V4(octets)) = endpoint.addr {
        out[4..8].copy_from_slice(&octets);
    }
    out
}

// ---------------------------------------------------------------------------
// 拷贝缝：凡跨空间取数处必有缝。生产实现委托 minix-sys 的 safecopy
// 动词（宿主下不可达，如实报错）；测试注 canned 实现。
// ---------------------------------------------------------------------------

/// 内核拷贝动词的缝（C `sys_safecopyfrom`/`sys_safecopyto`）。
pub trait CopyTransport {
    /// 经 grant 把授权方内存拷进 `buf`。
    fn safecopy_from(
        &mut self,
        granter: i32,
        grant: i32,
        offset: u64,
        buf: &mut [u8],
    ) -> Result<(), i32>;

    /// 经 grant 把 `buf` 拷给授权方。
    fn safecopy_to(
        &mut self,
        granter: i32,
        grant: i32,
        offset: u64,
        buf: &[u8],
    ) -> Result<(), i32>;
}

/// canned 替身：grant 号到字节的映射表，`safecopy_to` 的写入入账供
/// 断言（服务二进制测试用它喂地址/数据，不用真内核）。
#[derive(Debug, Default)]
pub struct CannedCopyTransport {
    /// grant 号 → 预置内容（`safecopy_from` 的来源）。
    pub from: Vec<(i32, Vec<u8>)>,
    /// `safecopy_to` 的写入账（grant 号与字节）。
    pub written: Vec<(i32, Vec<u8>)>,
}

impl CopyTransport for CannedCopyTransport {
    fn safecopy_from(
        &mut self,
        _granter: i32,
        grant: i32,
        offset: u64,
        buf: &mut [u8],
    ) -> Result<(), i32> {
        let start = offset as usize;
        let Some((_, src)) = self.from.iter().find(|(id, _)| *id == grant) else {
            return Err(minix_types::EFAULT);
        };
        let end = (start + buf.len()).min(src.len());
        if start > end {
            return Err(minix_types::EINVAL);
        }
        buf.copy_from_slice(&src[start..end]);
        Ok(())
    }

    fn safecopy_to(
        &mut self,
        _granter: i32,
        grant: i32,
        offset: u64,
        buf: &[u8],
    ) -> Result<(), i32> {
        self.written.push((grant, buf.to_vec()));
        let _ = offset;
        Ok(())
    }
}

/// 生产实现：minix-sys 的内核调用（宿主构建下 trap 不可达，调用方按
/// 错误折算——与其他通电面的宿主行为一致）。
#[derive(Debug, Default, Clone, Copy)]
pub struct SysCopyTransport;

impl CopyTransport for SysCopyTransport {
    fn safecopy_from(
        &mut self,
        granter: i32,
        grant: i32,
        offset: u64,
        buf: &mut [u8],
    ) -> Result<(), i32> {
        minix_sys::syscall::sys_safecopyfrom(
            &minix_sys::syscall::DirectKernelCallTransport,
            granter,
            grant,
            offset,
            buf.as_mut_ptr() as u64,
            buf.len() as u64,
        )
    }

    fn safecopy_to(
        &mut self,
        granter: i32,
        grant: i32,
        offset: u64,
        buf: &[u8],
    ) -> Result<(), i32> {
        minix_sys::syscall::sys_safecopyto(
            &minix_sys::syscall::DirectKernelCallTransport,
            granter,
            grant,
            offset,
            buf.as_ptr() as u64,
            buf.len() as u64,
        )
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
    req: &SocketRequest,
    is_root: bool,
) -> Result<i32, i32> {
    // RT/LNK 两域不进栈：服务侧表登记（C 的 rtsock/lnksocket 同为表
    // 对象），消息面（第 20/11 篇）随后批接线。
    if req.domain == domain::ROUTE {
        return open_service_socket(table, sockid::SockClass::Rt);
    }
    if req.domain == domain::LINK {
        return open_service_socket(table, sockid::SockClass::Lnk);
    }
    let family = match req.domain {
        domain::INET | domain::INET6 => internet_family(req.sock_type, is_root)?,
        _ => return Err(minix_types::EAFNOSUPPORT),
    };
    // RAW 建户的协议号校验照 C（rawsock.c:312-313——负数与超 u8 拒绝）
    // 后随协议值入栈（过滤面）。TCP/UDP 不消费协议号。
    let raw_protocol = match family {
        StackFamily::Raw => {
            if req.protocol < 0 || req.protocol > u8::MAX as i32 {
                return Err(minix_types::EPROTONOSUPPORT);
            }
            Some(req.protocol as u8)
        }
        _ => None,
    };
    let stack_socket = stack.open(family, raw_protocol)?;

    let id = class_base_of(family)
        .and_then(sockid::SockClass::from_base)
        .and_then(|c| SockId::from_class(c, stack_socket.index() as u32));
    let Some(id) = id else {
        let _ = stack.close(stack_socket);
        return Err(minix_types::EINVAL);
    };
    if table.add(id).is_err() {
        let _ = stack.close(stack_socket);
        return Err(minix_types::EAGAIN);
    }
    Ok(id.raw())
}

/// 服务侧建户（rtsock/lnksock 两域：不进栈，纯服务表对象——C 的
/// `rtsock_socket`/`lnksock_socket` 同为表登记）。类内下标线性扫描
/// 空闲号（两域套接字数少，扫描面可忽略）。
pub fn open_service_socket(
    table: &mut SockTable,
    class: sockid::SockClass,
) -> Result<i32, i32> {
    for index in 0..=sockid::INDEX_MASK {
        let Some(id) = SockId::from_class(class, index) else {
            continue;
        };
        if !table.contains(id) && table.add(id).is_ok() {
            return Ok(id.raw());
        }
    }
    Err(minix_types::ENOSPC)
}

// ---------------------------------------------------------------------------
// UDP 家族操作翻译（第 09 篇的 sdev 面）：BIND/CONNECT/GETSOCKNAME/
// SEND/RECEIVE 五条。数据面经拷贝缝在用户 grant 与栈缓冲间搬运。
// ---------------------------------------------------------------------------

/// `SDEV_BIND`/`SDEV_CONNECT`/`SDEV_GETSOCKNAME` 的载荷：
/// `mess_vfs_lsockdriver_addr { req_id@0; sock_id@4; grant@8; len@12;
/// user_endpt@16; sflags@20 }`（ipc.h:2258-2269）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AddrRequest {
    pub req_id: i32,
    pub sock_id: i32,
    pub grant: i32,
    pub len: u32,
    pub user_endpt: i32,
    pub sflags: u32,
}

/// 解码 `mess_vfs_lsockdriver_addr` 载荷。
pub fn decode_addr(msg: &Message) -> Option<AddrRequest> {
    // SAFETY: 载荷按上述域序写在消息负载区。
    let raw = unsafe { &msg.m_u.raw };
    let word = |at: usize| i32::from_le_bytes(raw[at..at + 4].try_into().unwrap());
    Some(AddrRequest {
        req_id: word(0),
        sock_id: word(4),
        grant: word(8),
        len: word(12) as u32,
        user_endpt: word(16),
        sflags: word(20) as u32,
    })
}

/// `SDEV_SEND`/`SDEV_RECV` 的载荷：`mess_vfs_lsockdriver_sendrecv
/// { req_id@0; sock_id@4; data_grant@8; [pad@12]; data_len@16(8B);
/// ctl_grant@24; ctl_len@28; addr_grant@32; addr_len@36; user_endpt@40;
/// flags@44 }`（ipc.h:2305-2317，LP64 总长 56 与断言宏一致；
/// `data_len` 是 size_t 占 8 字节，`ctl_len`/`addr_len` 是 unsigned
/// int 占 4 字节——域距与 VFS 侧编码逐格一致）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SendRecvRequest {
    pub req_id: i32,
    pub sock_id: i32,
    pub data_grant: i32,
    pub data_len: usize,
    pub ctl_grant: i32,
    pub ctl_len: usize,
    pub addr_grant: i32,
    pub addr_len: usize,
    pub user_endpt: i32,
    pub flags: u32,
}

/// 解码 `mess_vfs_lsockdriver_sendrecv` 载荷。
pub fn decode_sendrecv(msg: &Message) -> Option<SendRecvRequest> {
    // SAFETY: 载荷按上述域序写在消息负载区；只有 data_len 是 8 字节。
    let raw = unsafe { &msg.m_u.raw };
    let word = |at: usize| i32::from_le_bytes(raw[at..at + 4].try_into().unwrap());
    Some(SendRecvRequest {
        req_id: word(0),
        sock_id: word(4),
        data_grant: word(8),
        data_len: usize::from_le_bytes(raw[16..24].try_into().unwrap()),
        ctl_grant: word(24),
        ctl_len: word(28) as usize,
        addr_grant: word(32),
        addr_len: word(36) as usize,
        user_endpt: word(40),
        flags: word(44) as u32,
    })
}

/// 组装 `SDEV_RECV_REPLY`：`mess_lsockdriver_vfs_recv_reply { req_id@0;
/// status@4; ctl_len@8; addr_len@12; flags@16 }`。
pub fn recv_reply(req_id: i32, status: i32, ctl_len: u32, addr_len: u32) -> Message {
    let mut m = Message {
        m_type: SdevReply::ReceiveReply as i32,
        ..Message::default()
    };
    // SAFETY: 回复载荷按上述域序写在消息负载区。
    unsafe {
        let raw = &mut m.m_u.raw;
        raw[0..4].copy_from_slice(&req_id.to_le_bytes());
        raw[4..8].copy_from_slice(&status.to_le_bytes());
        raw[8..12].copy_from_slice(&ctl_len.to_le_bytes());
        raw[12..16].copy_from_slice(&addr_len.to_le_bytes());
    }
    m
}

/// `SDEV_LISTEN`/`SDEV_SHUTDOWN` 的载荷：`mess_vfs_lsockdriver_simple
/// { req_id@0; sock_id@4; param@8 }`（ipc.h:2319-2327）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SimpleRequest {
    pub req_id: i32,
    pub sock_id: i32,
    pub param: i32,
}

/// 解码 `mess_vfs_lsockdriver_simple` 载荷。
pub fn decode_simple(msg: &Message) -> Option<SimpleRequest> {
    // SAFETY: 载荷按上述域序写在消息负载区。
    let raw = unsafe { &msg.m_u.raw };
    let word = |at: usize| i32::from_le_bytes(raw[at..at + 4].try_into().unwrap());
    Some(SimpleRequest {
        req_id: word(0),
        sock_id: word(4),
        param: word(8),
    })
}

/// 组装 `SDEV_ACCEPT_REPLY`：`mess_lsockdriver_vfs_accept_reply
/// { req_id@0; sock_id@4; status@8; len@12 }`——新套接字号随回复带回。
pub fn accept_reply(req_id: i32, sock_id: i32, status: i32, len: u32) -> Message {
    let mut m = Message {
        m_type: SdevReply::AcceptReply as i32,
        ..Message::default()
    };
    // SAFETY: 回复载荷按上述域序写在消息负载区。
    unsafe {
        let raw = &mut m.m_u.raw;
        raw[0..4].copy_from_slice(&req_id.to_le_bytes());
        raw[4..8].copy_from_slice(&sock_id.to_le_bytes());
        raw[8..12].copy_from_slice(&status.to_le_bytes());
        raw[12..16].copy_from_slice(&len.to_le_bytes());
    }
    m
}

/// 一张接收"留言条"：挂起后完成所需的全部现场（C 的 `w_drv_sendrec`
/// 原始消息在此拆成字段——续答时要按它拷数据、写对端地址、回原请求）。
/// TCP connect 的留言条：等待建连完成后回 0。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingConnect {
    pub sock_id: i32,
    pub caller: Endpoint,
    pub req_id: i32,
}

/// TCP 发送的留言条：缓冲满时挂起，可写后重新拷数据再试（数据在
/// 用户内存里，每次尝试都要重拷）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingTcpSend {
    pub sock_id: i32,
    pub caller: Endpoint,
    pub req_id: i32,
    pub data_grant: i32,
    pub data_len: usize,
    pub user_endpt: i32,
}

/// TCP 接收的留言条：无数据时挂起，可读后续答（EOF 回 0，即 C 的
/// read 返回 0）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingTcpRecv {
    pub sock_id: i32,
    pub caller: Endpoint,
    pub req_id: i32,
    pub data_grant: i32,
    pub data_len: usize,
    pub user_endpt: i32,
}

/// TCP accept 的留言条：等待受纳完成后回新套接字号与对端地址。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingAccept {
    pub listener_id: i32,
    pub caller: Endpoint,
    pub req_id: i32,
    pub addr_grant: i32,
    pub addr_len: usize,
    pub user_endpt: i32,
}

/// 挂起留言条的账本（服务侧"没办完写留言条"的全部现场；唤醒扫描
/// 与超时摘条都在这本账上做）。
#[derive(Debug, Default)]
pub struct PendingTables {
    /// UDP 接收挂起（带对端地址出向面）。
    pub recvs: Vec<PendingRecv>,
    /// TCP 连接挂起。
    pub connects: Vec<PendingConnect>,
    /// TCP 受纳挂起。
    pub accepts: Vec<PendingAccept>,
    /// TCP 发送挂起（缓冲满）。
    pub tcp_sends: Vec<PendingTcpSend>,
    /// TCP 接收挂起（无数据）。
    pub tcp_recvs: Vec<PendingTcpRecv>,
    /// select 等待（一层触发：回复后由 VFS 自行重挂）。
    pub selects: Vec<PendingSelect>,
}

/// 一条 select 等待：套接字号、VFS 端点（select1 回复的目的地）与
/// 关心的就绪位（`SDEV_OP_*`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingSelect {
    pub sock_id: i32,
    pub vfs: Endpoint,
    pub ops: u8,
}

impl PendingTables {
    /// 取消一位调用方在某套接字上的全部挂起现场（C `sockevent_cancel`
    /// 的服务半；SDEV_CANCEL 无回复，取消后原请求由超时/事件路径
    /// 自然沉默）。
    pub fn cancel_for(&mut self, sock_id: i32, who: Endpoint) {
        self.recvs.retain(|n| !(n.sock_id == sock_id && n.caller == who));
        self.connects
            .retain(|n| !(n.sock_id == sock_id && n.caller == who));
        self.accepts
            .retain(|n| !(n.listener_id == sock_id && n.caller == who));
        self.tcp_sends
            .retain(|n| !(n.sock_id == sock_id && n.caller == who));
        self.tcp_recvs
            .retain(|n| !(n.sock_id == sock_id && n.caller == who));
        self.selects
            .retain(|n| !(n.sock_id == sock_id && n.vfs == who));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingRecv {
    /// 线上套接字号。
    pub sock_id: i32,
    /// 被挂起的调用方（VFS 进程端点，续答回执的目的地）。
    pub caller: Endpoint,
    /// 原请求的 req_id（回复原样带回）。
    pub req_id: i32,
    /// 数据 grant（收到的数据拷给它）。
    pub data_grant: i32,
    /// 用户数据缓冲长度。
    pub data_len: usize,
    /// 对端地址 grant（sockaddr_in 写回给它）。
    pub addr_grant: i32,
    /// 用户地址缓冲长度。
    pub addr_len: usize,
    /// 授权方端点（拷贝动词的 granter）。
    pub user_endpt: i32,
}

/// 关闭如何半关（`sys/socket.h` 的 `SHUT_*` 值，与垫片的 `shutdown_tcp`
/// 同表）。
pub mod shut {
    /// `SHUT_RD`（0）。
    pub const RD: i32 = 0;
    /// `SHUT_WR`（1）。
    pub const WR: i32 = 1;
    /// `SHUT_RDWR`（2）。
    pub const RDWR: i32 = 2;
}

/// 取消（C `sdev_cancel` 的服务半，sdev.c:25 的"无回复"面）：摘掉
/// 该调用方在该套接字上的全部挂起现场——服务侧留言条与表内记账都
/// 清。**不发回复**（C：取消的回复就是原请求的回复，这里沉默让位）。
pub fn cancel_road(table: &mut SockTable, pending: &mut PendingTables, msg: &Message) {
    let Some(req) = decode_simple(msg) else {
        return;
    };
    let Some(id) = SockId::from_raw(req.sock_id) else {
        return;
    };
    let who = Endpoint(req.req_id);
    table.cancel(id, who);
    pending.cancel_for(req.sock_id, who);
}

/// select 路（C `sdev_select` 的两层回复）：就绪即回一型；未就绪登记
/// 等待（一层触发——回复后由 VFS 自行决定重挂），ready-scan 唤醒。
pub fn select_road(
    stack: &mut dyn Stack,
    table: &mut SockTable,
    pending: &mut PendingTables,
    vfs: Endpoint,
    msg: &Message,
) -> Option<Message> {
    let Some((sock_id, ops)) = decode_select(msg) else {
        return Some(select1_reply(sock_id_from(0), 0));
    };
    let Some(stack_socket) = stack_socket_of(sock_id) else {
        return Some(select1_reply(sock_id, 0));
    };
    let readiness = stack.readiness(stack_socket);
    let ready = matched_ops(readiness, ops);
    if ready != 0 {
        return Some(select1_reply(sock_id, ready));
    }
    let Some(id) = SockId::from_raw(sock_id) else {
        return Some(select1_reply(sock_id, 0));
    };
    // 同一套接字只挂一条等待（C `ss_endpt` 单槽；重复 select 顶替）。
    if let Err(e) = table.register_select(id, vfs, ops) {
        let _ = e;
        return Some(select1_reply(sock_id, 0));
    }
    pending.selects.retain(|s| s.sock_id != sock_id);
    pending.selects.push(PendingSelect { sock_id, vfs, ops });
    None
}

fn sock_id_from(v: usize) -> i32 {
    v as i32
}

/// 关闭（C `sdev_close` → 各模块 close + 表摘除）：栈类先关栈内套
/// 接字，服务侧类（rtsock/lnksock）只摘表；两类都回 0。C 的可挂起
/// 关闭（缓冲未走的体面收尾）随接口批——此处为立即关闭，登记差异。
pub fn close_socket(
    stack: &mut dyn Stack,
    table: &mut SockTable,
    msg: &Message,
) -> Option<Message> {
    let Some(req) = decode_simple(msg) else {
        return Some(simple_reply(0, -(minix_types::EINVAL)));
    };
    let Some(id) = SockId::from_raw(req.sock_id) else {
        return Some(simple_reply(req.req_id, -(minix_types::EBADF)));
    };
    if let Some(stack_socket) = stack_socket_of(req.sock_id) {
        let _ = stack.close(stack_socket);
    }
    table.close(id);
    Some(simple_reply(req.req_id, 0))
}

/// 半关（C `sdev_shutdown` → `sock_shutdown`）：栈类转
/// [`Stack::shutdown_tcp`]，服务侧类（rtsock/lnksock）无对端半关
/// 语义，成功无操作回答（与 C 的 datagram shutdown 行为一致）。
pub fn shutdown_socket(
    stack: &mut dyn Stack,
    msg: &Message,
) -> Option<Message> {
    let Some(req) = decode_simple(msg) else {
        return Some(simple_reply(0, -(minix_types::EINVAL)));
    };
    match stack_socket_of(req.sock_id) {
        Some(stack_socket) => Some(match stack.shutdown_tcp(stack_socket, req.param) {
            Ok(()) => simple_reply(req.req_id, 0),
            Err(e) => simple_reply(req.req_id, wire(e)),
        }),
        None => Some(simple_reply(req.req_id, 0)),
    }
}

/// 解码 `SDEV_SELECT` 载荷：`mess_vfs_lsockdriver_select { sock_id@0;
/// ops@4 }`（ipc.h:2295-2302；ops 是 `SDEV_OP_*` 就绪位）。
pub fn decode_select(msg: &Message) -> Option<(i32, u8)> {
    // SAFETY: 载荷按上述域序写在消息负载区。
    let raw = unsafe { &msg.m_u.raw };
    Some((
        i32::from_le_bytes(raw[0..4].try_into().unwrap()),
        i32::from_le_bytes(raw[4..8].try_into().unwrap()) as u8,
    ))
}

/// 组装 select 一型回复：`mess_lsockdriver_vfs_select_reply
/// { sock_id@0; status@4 }`——status 是就绪的 `SDEV_OP_*` 位。
pub fn select1_reply(sock_id: i32, ready: u8) -> Message {
    let mut m = Message {
        m_type: SdevReply::SelectReply1 as i32,
        ..Message::default()
    };
    // SAFETY: 回复载荷按上述域序写在消息负载区。
    unsafe {
        let raw = &mut m.m_u.raw;
        raw[0..4].copy_from_slice(&sock_id.to_le_bytes());
        raw[4..8].copy_from_slice(&(ready as i32).to_le_bytes());
    }
    m
}

/// 就绪位与关心位的交集（读/写两面；错误位随 error 字段单独批）。
fn matched_ops(readiness: crate::lwip_port::Readiness, ops: u8) -> u8 {
    use minix_sockdriver::sdev::{SDEV_OP_RD, SDEV_OP_WR};
    let mut matched = 0u8;
    if readiness.readable {
        matched |= SDEV_OP_RD;
    }
    if readiness.writable {
        matched |= SDEV_OP_WR;
    }
    matched & ops
}

/// UDP 路的翻译入口：`msg` 已是 UDP 类套接字上的某条请求。返回
/// `Some(回复)` 立即答；`None` = 已挂起（留言条入 `pending`，VFS 侧的
/// 调用进程在睡，别回；唤醒后由 [`ready_scan`] 续答）。
pub fn translate_udp(
    stack: &mut dyn Stack,
    copy: &mut dyn CopyTransport,
    table: &mut SockTable,
    pending: &mut PendingTables,
    caller: Endpoint,
    msg: &Message,
) -> Option<Message> {
    let m_type = msg.m_type;
    if m_type == SdevRequest::Bind as i32 || m_type == SdevRequest::Connect as i32 {
        let Some(req) = decode_addr(msg) else {
            return Some(simple_reply(0, -(minix_types::EINVAL)));
        };
        let Some(stack_socket) = stack_socket_of(req.sock_id) else {
            return Some(simple_reply(req.req_id, -(minix_types::EBADF)));
        };
        // 地址在用户内存里（grant），拷进来再翻译。
        let mut bytes = [0u8; INET_ADDR_LEN];
        let n = req.len.min(INET_ADDR_LEN as u32) as usize;
        if copy
            .safecopy_from(req.user_endpt, req.grant, 0, &mut bytes[..n])
            .is_err()
        {
            return Some(simple_reply(req.req_id, -(minix_types::EFAULT)));
        }
        let Some(endpoint) = decode_inet_endpoint(&bytes) else {
            return Some(simple_reply(req.req_id, -(minix_types::EAFNOSUPPORT)));
        };
        let result = if m_type == SdevRequest::Bind as i32 {
            stack.bind_udp(stack_socket, Some(endpoint))
        } else {
            stack.connect_udp(stack_socket, endpoint)
        };
        return Some(match result {
            Ok(()) => simple_reply(req.req_id, 0),
            Err(e) => simple_reply(req.req_id, wire(e)),
        });
    }
    if m_type == SdevRequest::GetSockName as i32 {
        let Some(req) = decode_addr(msg) else {
            return Some(simple_reply(0, -(minix_types::EINVAL)));
        };
        let Some(stack_socket) = stack_socket_of(req.sock_id) else {
            return Some(simple_reply(req.req_id, -(minix_types::EBADF)));
        };
        let Ok(endpoint) = stack.local_endpoint_udp(stack_socket) else {
            return Some(simple_reply(req.req_id, -(minix_types::EBADF)));
        };
        let bytes = encode_inet_endpoint(endpoint);
        let len = req.len.min(INET_ADDR_LEN as u32) as usize;
        return match copy.safecopy_to(req.user_endpt, req.grant, 0, &bytes[..len]) {
            Ok(()) => Some(simple_reply(req.req_id, len as i32)),
            Err(code) => Some(simple_reply(req.req_id, wire(code))),
        };
    }
    if m_type == SdevRequest::Send as i32 || m_type == SdevRequest::Receive as i32 {
        return translate_udp_data(stack, copy, table, pending, caller, msg);
    }
    Some(simple_reply(0, -(minix_types::ENOSYS)))
}

fn translate_udp_data(
    stack: &mut dyn Stack,
    copy: &mut dyn CopyTransport,
    table: &mut SockTable,
    pending: &mut PendingTables,
    caller: Endpoint,
    msg: &Message,
) -> Option<Message> {
    let Some(req) = decode_sendrecv(msg) else {
        return Some(simple_reply(0, -(minix_types::EINVAL)));
    };
    let Some(stack_socket) = stack_socket_of(req.sock_id) else {
        return Some(simple_reply(req.req_id, -(minix_types::EBADF)));
    };
    let sending = msg.m_type == SdevRequest::Send as i32;
    if sending {
        // 拷数据入栈缓冲再发；长度按用户的 data_len 截（上限在本批为
        // 栈缓冲尺寸）。
        let mut data = alloc::vec![0u8; req.data_len.min(crate::lwip_port::TCP_SEND_BUFFER)];
        if copy
            .safecopy_from(req.user_endpt, req.data_grant, 0, &mut data)
            .is_err()
        {
            return Some(simple_reply(req.req_id, -(minix_types::EFAULT)));
        }
        // 对端地址：addr_len 在场即 sendto 语义，否则走 connect 的
        // 默认对端。
        let remote = if req.addr_len >= INET_ADDR_LEN {
            let mut addr_bytes = [0u8; INET_ADDR_LEN];
            if copy
                .safecopy_from(req.user_endpt, req.addr_grant, 0, &mut addr_bytes)
                .is_err()
            {
                return Some(simple_reply(req.req_id, -(minix_types::EFAULT)));
            }
            decode_inet_endpoint(&addr_bytes)
        } else {
            None
        };
        match stack.send_udp(stack_socket, &data, remote) {
            Ok(n) => Some(simple_reply(req.req_id, n as i32)),
            Err(e) => Some(simple_reply(req.req_id, wire(e))),
        }
    } else {
        // 接收：先试一次（可能已有包）；空且非阻塞即回 EWOULDBLOCK；
        // 空且阻塞则挂起记账（调用方由 VFS 侧挂起，此处不回），唤醒后
        // 由 ready-scan 续答。
        let mut data = alloc::vec![0u8; req.data_len.min(crate::lwip_port::TCP_SEND_BUFFER)];
        match stack.recv_udp(stack_socket, &mut data) {
            Ok((n, peer)) => {
                // 数据与对端地址都要拷给用户（C recv 数据面的两个
                // safecopyto）；拷不动即报错，不以成功码丢数据。
                let addr_bytes = encode_inet_endpoint(peer);
                let addr_len = req.addr_len.min(INET_ADDR_LEN);
                if copy
                    .safecopy_to(req.user_endpt, req.data_grant, 0, &data[..n])
                    .is_err()
                {
                    return Some(recv_reply(req.req_id, -(minix_types::EFAULT), 0, 0));
                }
                if copy
                    .safecopy_to(req.user_endpt, req.addr_grant, 0, &addr_bytes[..addr_len])
                    .is_err()
                {
                    return Some(recv_reply(req.req_id, -(minix_types::EFAULT), 0, 0));
                }
                Some(recv_reply(req.req_id, n as i32, 0, addr_len as u32))
            }
            Err(_) => {
                if req.flags & minix_sockdriver::sdev::MSG_DONTWAIT != 0 {
                    return Some(recv_reply(req.req_id, -(minix_types::EAGAIN), 0, 0));
                }
                // 挂起：唤醒事件 = 可读或对端关闭；无超时（SO_RCVTIMEO
                // 随选项批）。留言条入 `pending`，唤醒后由 ready-scan
                // 完成接收并续答。
                if stack_socket_of(req.sock_id).is_none() {
                    return Some(recv_reply(req.req_id, -(minix_types::EBADF), 0, 0));
                }
                let Some(id) = SockId::from_raw(req.sock_id) else {
                    return Some(recv_reply(req.req_id, -(minix_types::EBADF), 0, 0));
                };
                let wake = SocketEvent::Receive.bits() | SocketEvent::Close.bits();
                let Some(continuation) =
                    Continuation::new(SdevRequest::Receive, caller, wake, None)
                else {
                    return Some(recv_reply(req.req_id, -(minix_types::EINVAL), 0, 0));
                };
                if table.suspend(id, continuation).is_err() {
                    return Some(recv_reply(req.req_id, -(minix_types::EBADF), 0, 0));
                }
                pending.recvs.push(PendingRecv {
                    sock_id: req.sock_id,
                    caller,
                    req_id: req.req_id,
                    data_grant: req.data_grant,
                    data_len: req.data_len,
                    addr_grant: req.addr_grant,
                    addr_len: req.addr_len,
                    user_endpt: req.user_endpt,
                });
                None
            }
        }
    }
}

/// 统一分派：按线上套接字的类把请求交给 UDP 路或 TCP 路。未知类
/// （route/link 两域的模块随后批）按未接线回答。
#[allow(clippy::too_many_arguments)]
pub fn translate(
    stack: &mut dyn Stack,
    copy: &mut dyn CopyTransport,
    table: &mut SockTable,
    pending: &mut PendingTables,
    caller: Endpoint,
    msg: &Message,
) -> Option<Message> {
    // sock_id 在三类载荷里都在 @4；建户（Socket/SocketPair）不带
    // sock_id，不走本分派。
    // SAFETY: 首格域序两形状共用。
    let raw = unsafe { &msg.m_u.raw };
    let sock_id = i32::from_le_bytes(raw[4..8].try_into().unwrap());
    match SockId::from_raw(sock_id)
        .and_then(|id| id.class())
        .and_then(family_of_class)
    {
        Some(StackFamily::Udp) => translate_udp(stack, copy, table, pending, caller, msg),
        Some(StackFamily::Tcp) => translate_tcp(stack, copy, pending, caller, msg),
        Some(StackFamily::Raw) => translate_raw(stack, copy, msg),
        _ => Some(simple_reply(0, -(minix_types::ENOSYS))),
    }
}

/// TCP 路的翻译入口：BIND/LISTEN/CONNECT/ACCEPT/GETPEERNAME 五条。
/// connect 与 accept 的可挂起语义在此落：非阻塞立即回
/// EINPROGRESS/EWOULDBLOCK，阻塞挂起记账，唤醒后由 [`ready_scan`]
/// 续答（connect 等建连=可写；accept 等受理=监听者可读）。
pub fn translate_tcp(
    stack: &mut dyn Stack,
    copy: &mut dyn CopyTransport,
    pending: &mut PendingTables,
    caller: Endpoint,
    msg: &Message,
) -> Option<Message> {
    let m_type = msg.m_type;
    if m_type == SdevRequest::Bind as i32 {
        let Some(req) = decode_addr(msg) else {
            return Some(simple_reply(0, -(minix_types::EINVAL)));
        };
        let Some(stack_socket) = stack_socket_of(req.sock_id) else {
            return Some(simple_reply(req.req_id, -(minix_types::EBADF)));
        };
        let mut bytes = [0u8; INET_ADDR_LEN];
        let n = (req.len as usize).min(INET_ADDR_LEN);
        if copy
            .safecopy_from(req.user_endpt, req.grant, 0, &mut bytes[..n])
            .is_err()
        {
            return Some(simple_reply(req.req_id, -(minix_types::EFAULT)));
        }
        let Some(endpoint) = decode_inet_endpoint(&bytes) else {
            return Some(simple_reply(req.req_id, -(minix_types::EAFNOSUPPORT)));
        };
        return Some(match stack.bind_tcp(stack_socket, Some(endpoint)) {
            Ok(()) => simple_reply(req.req_id, 0),
            Err(e) => simple_reply(req.req_id, wire(e)),
        });
    }
    if m_type == SdevRequest::Listen as i32 {
        let Some(req) = decode_simple(msg) else {
            return Some(simple_reply(0, -(minix_types::EINVAL)));
        };
        let Some(stack_socket) = stack_socket_of(req.sock_id) else {
            return Some(simple_reply(req.req_id, -(minix_types::EBADF)));
        };
        let backlog = req.param.clamp(0, i32::from(u16::MAX)) as u32;
        return Some(match stack.listen_tcp(stack_socket, backlog) {
            Ok(()) => simple_reply(req.req_id, 0),
            Err(e) => simple_reply(req.req_id, wire(e)),
        });
    }
    if m_type == SdevRequest::Connect as i32 {
        let Some(req) = decode_addr(msg) else {
            return Some(simple_reply(0, -(minix_types::EINVAL)));
        };
        let Some(stack_socket) = stack_socket_of(req.sock_id) else {
            return Some(simple_reply(req.req_id, -(minix_types::EBADF)));
        };
        let mut bytes = [0u8; INET_ADDR_LEN];
        let n = (req.len as usize).min(INET_ADDR_LEN);
        if copy
            .safecopy_from(req.user_endpt, req.grant, 0, &mut bytes[..n])
            .is_err()
        {
            return Some(simple_reply(req.req_id, -(minix_types::EFAULT)));
        }
        let Some(endpoint) = decode_inet_endpoint(&bytes) else {
            return Some(simple_reply(req.req_id, -(minix_types::EAFNOSUPPORT)));
        };
        return match stack.connect_tcp(stack_socket, endpoint) {
            Ok(()) => {
                if req.sflags & minix_sockdriver::sdev::SDEV_NONBLOCK != 0 {
                    // 非阻塞：开张即回"进行中"，建连完成由就绪位说话。
                    Some(simple_reply(req.req_id, -(minix_types::EINPROGRESS)))
                } else {
                    // 阻塞：挂等建连（SEV_CONNECT 唤醒；无超时，选项随
                    // 后批）。
                    pending.connects.push(PendingConnect {
                        sock_id: req.sock_id,
                        caller,
                        req_id: req.req_id,
                    });
                    None
                }
            }
            Err(e) => Some(simple_reply(req.req_id, wire(e))),
        };
    }
    if m_type == SdevRequest::Accept as i32 {
        let Some(req) = decode_addr(msg) else {
            return Some(simple_reply(0, -(minix_types::EINVAL)));
        };
        let Some(listener) = stack_socket_of(req.sock_id) else {
            return Some(simple_reply(req.req_id, -(minix_types::EBADF)));
        };
        return match stack.accept_tcp(listener) {
            Ok((accepted, peer)) => {
                let bytes = encode_inet_endpoint(peer);
                let len = (req.len as usize).min(INET_ADDR_LEN);
                if copy
                    .safecopy_to(req.user_endpt, req.grant, 0, &bytes[..len])
                    .is_err()
                {
                    return Some(accept_reply(
                        req.req_id,
                        0,
                        -(minix_types::EFAULT),
                        0,
                    ));
                }
                Some(accept_reply(req.req_id, accepted.index() as i32, 0, len as u32))
            }
            Err(_) => {
                if req.sflags & minix_sockdriver::sdev::SDEV_NONBLOCK != 0 {
                    return Some(accept_reply(
                        req.req_id,
                        0,
                        crate::util::ERR_WOULD_BLOCK,
                        0,
                    ));
                }
                pending.accepts.push(PendingAccept {
                    listener_id: req.sock_id,
                    caller,
                    req_id: req.req_id,
                    addr_grant: req.grant,
                    addr_len: req.len as usize,
                    user_endpt: req.user_endpt,
                });
                None
            }
        };
    }
    if m_type == SdevRequest::Send as i32 || m_type == SdevRequest::Receive as i32 {
        let Some(req) = decode_sendrecv(msg) else {
            return Some(simple_reply(0, -(minix_types::EINVAL)));
        };
        let Some(stack_socket) = stack_socket_of(req.sock_id) else {
            return Some(simple_reply(req.req_id, -(minix_types::EBADF)));
        };
        let nonblock = req.flags & minix_sockdriver::sdev::MSG_DONTWAIT != 0;
        let _ = req.ctl_grant;
        let _ = req.ctl_len;
        let _ = req.addr_grant;
        let _ = req.addr_len;
        if m_type == SdevRequest::Send as i32 {
            // 流式发送：入多少回多少（部分发送合法）；缓冲满且阻塞
            // 挂起，非阻塞回 EAGAIN。
            let mut data = alloc::vec![0u8; req.data_len.min(crate::lwip_port::TCP_SEND_BUFFER)];
            if copy
                .safecopy_from(req.user_endpt, req.data_grant, 0, &mut data)
                .is_err()
            {
                return Some(simple_reply(req.req_id, -(minix_types::EFAULT)));
            }
            return match stack.send_tcp(stack_socket, &data) {
                Ok(n) if n > 0 => Some(simple_reply(req.req_id, n as i32)),
                Ok(_) => {
                    if nonblock {
                        Some(simple_reply(req.req_id, -(minix_types::EAGAIN)))
                    } else {
                        pending.tcp_sends.push(PendingTcpSend {
                            sock_id: req.sock_id,
                            caller,
                            req_id: req.req_id,
                            data_grant: req.data_grant,
                            data_len: req.data_len,
                            user_endpt: req.user_endpt,
                        });
                        None
                    }
                }
                Err(e) => Some(simple_reply(req.req_id, wire(e))),
            };
        }
        // 流式接收：有数据拷给用户；EOF（对端关且取尽）回 0；空且
        // 阻塞挂起，非阻塞回 EAGAIN；连接未开按错误回答。
        let mut data = alloc::vec![0u8; req.data_len.min(crate::lwip_port::TCP_SEND_BUFFER)];
        let outcome = stack.recv_tcp(stack_socket, &mut data);
        return match outcome {
            // EOF 与真错误先行（栈把"空"折成阻塞类错误，归入挂起半）。
            Ok((0, true)) => Some(recv_reply(req.req_id, 0, 0, 0)),
            Err(e) if e != crate::util::ERR_WOULD_BLOCK => {
                Some(recv_reply(req.req_id, wire(e), 0, 0))
            }
            // 空（阻塞类错误或空读）：非阻塞回 EAGAIN，阻塞挂起记账。
            _ => {
                if nonblock {
                    return Some(recv_reply(req.req_id, -(minix_types::EAGAIN), 0, 0));
                }
                pending.tcp_recvs.push(PendingTcpRecv {
                    sock_id: req.sock_id,
                    caller,
                    req_id: req.req_id,
                    data_grant: req.data_grant,
                    data_len: req.data_len,
                    user_endpt: req.user_endpt,
                });
                None
            }
        }
    }
    if m_type == SdevRequest::GetPeerName as i32 {
        let Some(req) = decode_addr(msg) else {
            return Some(simple_reply(0, -(minix_types::EINVAL)));
        };
        let Some(stack_socket) = stack_socket_of(req.sock_id) else {
            return Some(simple_reply(req.req_id, -(minix_types::EBADF)));
        };
        return match stack.remote_endpoint_tcp(stack_socket) {
            Ok(endpoint) => {
                let bytes = encode_inet_endpoint(endpoint);
                let len = (req.len as usize).min(INET_ADDR_LEN);
                if copy
                    .safecopy_to(req.user_endpt, req.grant, 0, &bytes[..len])
                    .is_err()
                {
                    return Some(simple_reply(req.req_id, -(minix_types::EFAULT)));
                }
                Some(simple_reply(req.req_id, len as i32))
            }
            Err(e) => Some(simple_reply(req.req_id, wire(e))),
        };
    }
    Some(simple_reply(0, -(minix_types::ENOSYS)))
}

/// RAW 路的翻译入口：Send（写全报文）与 Receive（读全报文）。数据面
/// 是 grant 与栈缓冲间的直接搬运——全报文语义（含 IP 头，HDRINCL）。
pub fn translate_raw(
    stack: &mut dyn Stack,
    copy: &mut dyn CopyTransport,
    msg: &Message,
) -> Option<Message> {
    let Some(req) = decode_sendrecv(msg) else {
        return Some(simple_reply(0, -(minix_types::EINVAL)));
    };
    let Some(stack_socket) = stack_socket_of(req.sock_id) else {
        return Some(simple_reply(req.req_id, -(minix_types::EBADF)));
    };
    let sending = msg.m_type == SdevRequest::Send as i32;
    if sending {
        let mut data = alloc::vec![0u8; req.data_len.min(crate::lwip_port::TCP_SEND_BUFFER)];
        if copy
            .safecopy_from(req.user_endpt, req.data_grant, 0, &mut data)
            .is_err()
        {
            return Some(simple_reply(req.req_id, -(minix_types::EFAULT)));
        }
        return match stack.send_raw(stack_socket, &data) {
            Ok(n) => Some(simple_reply(req.req_id, n as i32)),
            Err(e) => Some(simple_reply(req.req_id, wire(e))),
        };
    }
    // 接收：拷给用户。
    let mut data = alloc::vec![0u8; req.data_len.min(crate::lwip_port::TCP_SEND_BUFFER)];
    match stack.recv_raw(stack_socket, &mut data) {
        Ok(n) => {
            if copy
                .safecopy_to(req.user_endpt, req.data_grant, 0, &data[..n])
                .is_err()
            {
                return Some(recv_reply(req.req_id, -(minix_types::EFAULT), 0, 0));
            }
            Some(recv_reply(req.req_id, n as i32, 0, 0))
        }
        Err(e) => Some(recv_reply(req.req_id, wire(e), 0, 0)),
    }
}

/// 就绪扫描（C `sockevent_process` 的"事件唤醒续答"半）：每趟推进后
/// 扫一遍接收留言条——栈报可读的即续答完成（拷数据与对端地址给用户、
/// 回 `SDEV_RECV_REPLY`），返回 `(调用方, 回复)` 对由循环尾发出；暂不
/// 就绪的留言条原样留在 `pending`。超时路径（`poll_timers` 的
/// `TimedOut`）由调用方在同一处处理并从 `pending` 摘条。
pub fn ready_scan(
    stack: &mut dyn Stack,
    copy: &mut dyn CopyTransport,
    table: &mut SockTable,
    pending: &mut PendingTables,
) -> Vec<(Endpoint, Message)> {
    let mut replies = Vec::new();
    let mut i = 0;
    while i < pending.recvs.len() {
        let note = pending.recvs[i];
        let readable = stack_socket_of(note.sock_id)
            .map(|s| stack.readiness(s))
            .map(|r| r.readable)
            .unwrap_or(false);
        if !readable {
            i += 1;
            continue;
        }
        let Some(stack_socket) = stack_socket_of(note.sock_id) else {
            pending.recvs.remove(i);
            continue;
        };
        let mut data = alloc::vec![0u8; note.data_len];
        match stack.recv_udp(stack_socket, &mut data) {
            Ok((n, peer)) => {
                // 数据与对端地址都拷给用户；任一拷不动即按错误续答
                // （成功码丢数据是"假成功"，禁）。
                let addr_bytes = encode_inet_endpoint(peer);
                let addr_len = note.addr_len.min(INET_ADDR_LEN);
                let data_ok =
                    copy.safecopy_to(note.user_endpt, note.data_grant, 0, &data[..n]).is_ok();
                let addr_ok = copy
                    .safecopy_to(note.user_endpt, note.addr_grant, 0, &addr_bytes[..addr_len])
                    .is_ok();
                let status = if data_ok && addr_ok {
                    n as i32
                } else {
                    -(minix_types::EFAULT)
                };
                replies.push((note.caller, recv_reply(note.req_id, status, 0, addr_len as u32)));
                pending.recvs.remove(i);
            }
            Err(_) => {
                // 就绪位翻了但收不动（对端关闭一类）：按连接复位回。
                replies.push((
                    note.caller,
                    recv_reply(note.req_id, -(minix_types::ECONNRESET), 0, 0),
                ));
                pending.recvs.remove(i);
            }
        }
    }
    // TCP connect：建连完成（可写）即回 0。
    let mut i = 0;
    while i < pending.connects.len() {
        let note = pending.connects[i];
        let writable = stack_socket_of(note.sock_id)
            .map(|s| stack.readiness(s))
            .map(|r| r.writable)
            .unwrap_or(false);
        if writable {
            replies.push((note.caller, simple_reply(note.req_id, 0)));
            pending.connects.remove(i);
        } else {
            i += 1;
        }
    }
    // TCP 发送：可写后重拷数据再试（部分发送即回实际入队量）。
    let mut i = 0;
    while i < pending.tcp_sends.len() {
        let note = pending.tcp_sends[i];
        let writable = stack_socket_of(note.sock_id)
            .map(|s| stack.readiness(s))
            .map(|r| r.writable)
            .unwrap_or(false);
        if !writable {
            i += 1;
            continue;
        }
        let Some(stack_socket) = stack_socket_of(note.sock_id) else {
            pending.tcp_sends.remove(i);
            continue;
        };
        let mut data = alloc::vec![0u8; note.data_len.min(crate::lwip_port::TCP_SEND_BUFFER)];
        if copy
            .safecopy_from(note.user_endpt, note.data_grant, 0, &mut data)
            .is_err()
        {
            replies.push((note.caller, simple_reply(note.req_id, -(minix_types::EFAULT))));
            pending.tcp_sends.remove(i);
            continue;
        }
        match stack.send_tcp(stack_socket, &data) {
            Ok(n) if n > 0 => {
                replies.push((note.caller, simple_reply(note.req_id, n as i32)));
                pending.tcp_sends.remove(i);
            }
            Ok(_) => i += 1,
            Err(e) => {
                replies.push((note.caller, simple_reply(note.req_id, wire(e))));
                pending.tcp_sends.remove(i);
            }
        }
    }
    // TCP 接收：可读后续答；EOF 回 0（C read 的文件尾语义）。
    let mut i = 0;
    while i < pending.tcp_recvs.len() {
        let note = pending.tcp_recvs[i];
        let readable = stack_socket_of(note.sock_id)
            .map(|s| stack.readiness(s))
            .map(|r| r.readable)
            .unwrap_or(false);
        if !readable {
            i += 1;
            continue;
        }
        let Some(stack_socket) = stack_socket_of(note.sock_id) else {
            pending.tcp_recvs.remove(i);
            continue;
        };
        let mut data = alloc::vec![0u8; note.data_len.min(crate::lwip_port::TCP_SEND_BUFFER)];
        match stack.recv_tcp(stack_socket, &mut data) {
            Ok((n, _eof)) if n > 0 => {
                if copy
                    .safecopy_to(note.user_endpt, note.data_grant, 0, &data[..n])
                    .is_err()
                {
                    replies.push((
                        note.caller,
                        recv_reply(note.req_id, -(minix_types::EFAULT), 0, 0),
                    ));
                } else {
                    replies.push((note.caller, recv_reply(note.req_id, n as i32, 0, 0)));
                }
                pending.tcp_recvs.remove(i);
            }
            Ok((0, true)) => {
                replies.push((note.caller, recv_reply(note.req_id, 0, 0, 0)));
                pending.tcp_recvs.remove(i);
            }
            _ => i += 1,
        }
    }
    // TCP accept：监听者可读即受纳，回新套接字号与对端地址。
    let mut i = 0;
    while i < pending.accepts.len() {
        let note = pending.accepts[i];
        let pending_connection = stack_socket_of(note.listener_id)
            .map(|s| stack.readiness(s))
            .map(|r| r.readable)
            .unwrap_or(false);
        if !pending_connection {
            i += 1;
            continue;
        }
        let Some(listener) = stack_socket_of(note.listener_id) else {
            pending.accepts.remove(i);
            continue;
        };
        match stack.accept_tcp(listener) {
            Ok((accepted, peer)) => {
                let bytes = encode_inet_endpoint(peer);
                let len = note.addr_len.min(INET_ADDR_LEN);
                let copy_ok = copy
                    .safecopy_to(note.user_endpt, note.addr_grant, 0, &bytes[..len])
                    .is_ok();
                let status = if copy_ok { 0 } else { -(minix_types::EFAULT) };
                // 受纳套接字登记进服务表（线上号 = TCP 类基 + 栈内
                // 下标）；登记失败按"无空位"续答。
                let registered =
                    SockId::from_class(sockid::SockClass::Tcp, accepted.index() as u32);
                match registered {
                    Some(id) if table.add(id).is_ok() => {
                        replies.push((
                            note.caller,
                            accept_reply(note.req_id, id.raw(), status, len as u32),
                        ));
                        pending.accepts.remove(i);
                    }
                    _ => {
                        let _ = stack.close(accepted);
                        replies.push((
                            note.caller,
                            accept_reply(
                                note.req_id,
                                0,
                                -(minix_types::ENOSPC),
                                0,
                            ),
                        ));
                        pending.accepts.remove(i);
                    }
                }
            }
            Err(_) => {
                // 可读但受理未完成（握手半程）：留言条留下，下轮再试。
                i += 1;
            }
        }
    }
    // select 等待：就绪位与关心位有交集即回一型并摘账（一层触发：
    // 回复后由 VFS 自行决定重挂；对端关闭按错误位归入 ERR 面）。
    let mut i = 0;
    while i < pending.selects.len() {
        let note = pending.selects[i];
        let ready = stack_socket_of(note.sock_id)
            .map(|s| stack.readiness(s))
            .map(|r| matched_ops(r, note.ops))
            .unwrap_or(0);
        if ready == 0 {
            i += 1;
            continue;
        }
        let Some(id) = SockId::from_raw(note.sock_id) else {
            pending.selects.remove(i);
            continue;
        };
        if table.take_select(id).is_some() {
            replies.push((note.vfs, select1_reply(note.sock_id, ready)));
        }
        pending.selects.remove(i);
    }
    replies
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stack::SmoltcpStack;

    /// 现成栈加空表：建户用的最小现场。
    fn fixture() -> (SmoltcpStack, SockTable) {
        (SmoltcpStack::new(1, 0), SockTable::new())
    }

    #[test]
    fn test_open_stream_gets_tcp_class_id() {
        let (mut stack, mut table) = fixture();
        let req = SocketRequest {
            req_id: 7,
            domain: domain::INET,
            sock_type: sock_type::STREAM,
            protocol: 0,
            user_endpt: 100,
        };
        let id = open_socket(&mut stack, &mut table, &req, false).unwrap();
        assert_eq!(sockid::SockId::from_raw(id).unwrap().class_base(), sockid::SOCKID_TCP);
        assert!(table.contains(sockid::SockId::from_raw(id).unwrap()));
    }

    #[test]
    fn test_open_dgram_and_raw_distinct_classes() {
        let (mut stack, mut table) = fixture();
        let dgram = SocketRequest {
            req_id: 1, domain: domain::INET, sock_type: sock_type::DGRAM,
            protocol: 0, user_endpt: 100,
        };
        let raw_req = SocketRequest {
            req_id: 2, domain: domain::INET6, sock_type: sock_type::RAW,
            protocol: 0, user_endpt: 0, // 根身份
        };
        let udp_id = open_socket(&mut stack, &mut table, &dgram, false).unwrap();
        let raw_id = open_socket(&mut stack, &mut table, &raw_req, true).unwrap();
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
        let (mut stack, mut table) = fixture();
        let raw_req = SocketRequest {
            req_id: 1, domain: domain::INET, sock_type: sock_type::RAW,
            protocol: 0, user_endpt: 100,
        };
        assert_eq!(
            open_socket(&mut stack, &mut table, &raw_req, false).unwrap_err(),
            minix_types::EACCES,
            "非根开 RAW 被拒（lwip.c:166-167）"
        );
        let bogus = SocketRequest {
            req_id: 2, domain: domain::INET, sock_type: 7,
            protocol: 0, user_endpt: 100,
        };
        assert_eq!(
            open_socket(&mut stack, &mut table, &bogus, true).unwrap_err(),
            minix_types::EPROTOTYPE,
            "未知类型按 EPROTOTYPE（lwip.c:170）"
        );
    }

    #[test]
    fn test_route_link_domains_answer_enosys_and_unknown_domain_eafnosupport() {
        let (mut stack, mut table) = fixture();
        let route = SocketRequest {
            req_id: 1, domain: domain::ROUTE, sock_type: sock_type::DGRAM,
            protocol: 0, user_endpt: 100,
        };
        // RT/LNK 服务侧建户（批八起转真）：不进栈，类号各归其位。
        let route_id = open_socket(&mut stack, &mut table, &route, false).unwrap();
        assert_eq!(
            sockid::SockId::from_raw(route_id).unwrap().class_base(),
            sockid::SOCKID_RT,
            "路由套接字是服务侧对象，不进栈"
        );
        let alien = SocketRequest {
            req_id: 2, domain: 42, sock_type: sock_type::STREAM,
            protocol: 0, user_endpt: 100,
        };
        assert_eq!(
            open_socket(&mut stack, &mut table, &alien, false).unwrap_err(),
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
