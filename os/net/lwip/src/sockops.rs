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
    let family = match req.domain {
        domain::INET | domain::INET6 => internet_family(req.sock_type, is_root)?,
        domain::ROUTE | domain::LINK => return Err(minix_types::ENOSYS),
        _ => return Err(minix_types::EAFNOSUPPORT),
    };
    let stack_socket = stack.open(family)?;
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

/// 一张接收"留言条"：挂起后完成所需的全部现场（C 的 `w_drv_sendrec`
/// 原始消息在此拆成字段——续答时要按它拷数据、写对端地址、回原请求）。
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

/// UDP 路的翻译入口：`msg` 已是 UDP 类套接字上的某条请求。返回
/// `Some(回复)` 立即答；`None` = 已挂起（留言条入 `pending`，VFS 侧的
/// 调用进程在睡，别回；唤醒后由 [`ready_scan`] 续答）。
pub fn translate_udp(
    stack: &mut dyn Stack,
    copy: &mut dyn CopyTransport,
    table: &mut SockTable,
    pending: &mut Vec<PendingRecv>,
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
            Err(e) => simple_reply(req.req_id, -e),
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
            Err(code) => Some(simple_reply(req.req_id, -code.abs())),
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
    pending: &mut Vec<PendingRecv>,
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
            Err(e) => Some(simple_reply(req.req_id, -e)),
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
                pending.push(PendingRecv {
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

/// 就绪扫描（C `sockevent_process` 的"事件唤醒续答"半）：每趟推进后
/// 扫一遍接收留言条——栈报可读的即续答完成（拷数据与对端地址给用户、
/// 回 `SDEV_RECV_REPLY`），返回 `(调用方, 回复)` 对由循环尾发出；暂不
/// 就绪的留言条原样留在 `pending`。超时路径（`poll_timers` 的
/// `TimedOut`）由调用方在同一处处理并从 `pending` 摘条。
pub fn ready_scan(
    stack: &mut dyn Stack,
    copy: &mut dyn CopyTransport,
    pending: &mut Vec<PendingRecv>,
) -> Vec<(Endpoint, Message)> {
    let mut replies = Vec::new();
    let mut i = 0;
    while i < pending.len() {
        let note = pending[i];
        let readable = stack_socket_of(note.sock_id)
            .map(|s| stack.readiness(s))
            .map(|r| r.readable)
            .unwrap_or(false);
        if !readable {
            i += 1;
            continue;
        }
        let Some(stack_socket) = stack_socket_of(note.sock_id) else {
            pending.remove(i);
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
                pending.remove(i);
            }
            Err(_) => {
                // 就绪位翻了但收不动（对端关闭一类）：按连接复位回。
                replies.push((
                    note.caller,
                    recv_reply(note.req_id, -(minix_types::ECONNRESET), 0, 0),
                ));
                pending.remove(i);
            }
        }
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
        assert_eq!(
            open_socket(&mut stack, &mut table, &route, false).unwrap_err(),
            minix_types::ENOSYS,
            "rtsock 随后批接线，先诚实回答未接线"
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
