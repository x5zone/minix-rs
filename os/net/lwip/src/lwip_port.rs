//! Third-party stack port surface: build subset, key options, hooks, patches.
//!
//! C correspondence: `minix3/minix/lib/liblwip/` (build subset under
//! `dist/src`, glue under `lib/`, four patches under `patches/`). No packet
//! code lives here. This module owns the portion that can be stated as
//! numbers and names: which build subset is compiled, which option values
//! the service relies on, which four hooks the service provides, and which
//! four patches are applied.
//!
//! [ARCH N-1]: ruled 2026-09-17 (decision record: 24-liblwip-port.md §1.5).
//! The replacement is a Rust stack of the smoltcp family behind a
//! service-owned wall, with a semantic shim carrying the MINIX3 socket
//! behavior; the FFI path stays reachable because every service module
//! depends only on the [`Stack`] and [`StackHooks`] traits below, never on
//! a concrete stack. The glue constants below remain the behavior contract
//! the shim must honor: single-threaded, no pool allocator, receive window
//! 16384, send buffer 11 × 1460, multicast ceilings.

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

// ---------------------------------------------------------------------------
// 栈墙：服务唯一依赖的第三方栈边界（[ARCH N-1]，裁决记录见 24 篇 §1.5）
//
// 边界的所有权方向是这条墙的全部意义：
// - 路由与网关策略属于服务（C 里 lwip_hook_ip4_route 被调用即 panic，
//   lwiphooks.h:15——含义是栈根本没有路由权，route.c 的表是唯一真相）。
//   所以墙只向栈要"下一跳/网关"这种解析服务，不交出选路决策。
// - 时间源属于服务（C sys_now = getticks × 1000 / sys_hz，毫秒），墙上的
//   时刻一律是毫秒数。
// - 设备属于服务：帧从 NDEV 进来交给栈，栈要发帧时交还服务，缓冲模型由
//   缓冲轮次（N1-P1-5）细化后替换这里的借用切片签名。
// ---------------------------------------------------------------------------

/// 线上形状的 IP 地址。最终归属随 N1-P1-1/N-8 的 wire 类型裁决定位。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackIpAddr {
    /// 版本 4 地址，网络序字节。
    V4([u8; 4]),
    /// 版本 6 地址，网络序字节。
    V6([u8; 16]),
}

/// 栈支持的 socket 家族，与服务侧 tcpsock/udpsock/rawsock/icmp 面一一对应。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackFamily {
    /// 传输控制协议（第 08 篇）。
    Tcp,
    /// 用户数据报协议（第 09 篇）。
    Udp,
    /// 原始 IP 报文（第 10 篇）。
    Raw,
    /// 互联网控制报文协议（第 07 篇的 icmp 面）。
    Icmp,
}

/// 栈内一个 socket 的句柄：家族加栈内索引。服务从不持有栈内部内存，
/// 句柄指向的对象随关闭消失，之后使用它的调用由栈以错误回答。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StackSocket {
    family: StackFamily,
    index: u16,
}

impl StackSocket {
    /// 用家族与栈内索引构造句柄。
    pub const fn new(family: StackFamily, index: u16) -> Self {
        StackSocket { family, index }
    }

    /// 句柄的家族。
    pub const fn family(&self) -> StackFamily {
        self.family
    }

    /// 句柄在栈内的索引。
    pub const fn index(&self) -> u16 {
        self.index
    }
}

/// TCP 初始序列号的输入四元组（`lwip_hook_tcp_isn` 的 Rust 重述，
/// lwiphooks.h:8-11）。契约只有一条：不可预测（RFC 6528）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConnectionQuad {
    /// 本端地址。
    pub local_address: StackIpAddr,
    /// 本端端口。
    pub local_port: u16,
    /// 对端地址。
    pub remote_address: StackIpAddr,
    /// 对端端口。
    pub remote_port: u16,
}

/// 服务提供给栈的策略钩子。
///
/// C 对应 lwiphooks.h 的序列号钩子与两个网关钩子（lwiphooks.h:24-27 与
/// :40-43，服务从自己的表回答解析）。两个路由覆盖钩子**不在墙内**：它们在
/// C 里的契约就是"被调用即 panic"（lwiphooks.h:15 与 :31），真正的
/// 含义是路由决策属于服务，不是栈可以回调的策略点，所以墙上没有这个口子。
pub trait StackHooks {
    /// 一个新 TCP 连接的初始序列号。smoltcp 一族的栈在内部生成序列号、
    /// 不消费此钩子；钩子保留在墙上是为持有随机源的服务实现（记录于
    /// 24 篇 §1.5 偏差表）。
    fn initial_sequence(&mut self, quad: ConnectionQuad) -> u32;

    /// 版本 4 的网关/下一跳解析（地址解析前的最后一步）；`None` 表示
    /// 目的地即本地链路。`netif_index` 是服务侧接口表（第 14 篇）的行号。
    fn gateway_v4(&mut self, netif_index: u16, destination: [u8; 4]) -> Option<[u8; 4]>;

    /// 版本 6 的网关/下一跳解析（邻居发现消费）。
    fn gateway_v6(&mut self, netif_index: u16, destination: [u8; 16]) -> Option<[u8; 16]>;
}

/// 栈报给服务的一个 socket 就绪位。服务把它翻译成 SEV_SEND/SEV_RECV
/// 事件位（第 02 篇）；错误就绪不走这里，由操作返回的错误表达。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Readiness {
    /// 可读（有数据或连接事件待处理）。
    pub readable: bool,
    /// 可写（发送缓冲有余量）。
    pub writable: bool,
}

/// 一个 UDP 端点：地址加端口。`addr` 为 `None` 表示通配（绑定时未定
/// 地址、发送时取套接字默认对端语义外的裸端口面）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StackEndpoint {
    /// 地址；`None` = 通配。
    pub addr: Option<StackIpAddr>,
    /// 端口（主机序）。
    pub port: u16,
}

impl StackEndpoint {
    /// 指定地址与端口的端点。
    pub const fn new(addr: Option<StackIpAddr>, port: u16) -> Self {
        StackEndpoint { addr, port }
    }
}

/// 主循环向栈询问下一次交付时刻，对应 C 里 `sys_check_timeouts` 的语义
/// 位置。`At` 携带的毫秒时刻与 C `sys_now` 同一时间基。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PollWhen {
    /// 该毫秒时刻前无需 pump。
    At(u64),
    /// 无定时器、无重传待办，睡到下一个消息到来。
    Never,
}

/// 栈墙主 trait。服务代码（startup 的分发路、各 sock 模块）只依赖这里
/// 声明的面，不依赖任何具体栈；骨架期先立 pump、就绪与最小生命周期，
/// 逐家族操作与数据面细节随缓冲轮次（N1-P1-5）落地。错误一律返回
/// `util.rs` 的 `STACK_*` 线上值——它们是 N-14 双射表的栈侧半边，不在此
/// 重复定义。
pub trait Stack {
    /// 推进栈：处理到期定时器与排队工作。就绪位在 pump 之后经
    /// [`Stack::readiness`] 逐句柄查询（poll-then-scan，Redox smolnetd
    /// 的分发形状）。
    fn poll(&mut self, now_millis: u64) -> PollWhen;

    /// 查询一个 socket 的当前就绪位；句柄已关闭时返回缺省值（无事件）。
    fn readiness(&self, socket: StackSocket) -> Readiness;

    /// 打开一个指定家族的 socket，返回栈侧句柄。
    fn open(&mut self, family: StackFamily) -> Result<StackSocket, i32>;

    /// 关闭一个 socket；此后它的句柄失效。
    fn close(&mut self, socket: StackSocket) -> Result<(), i32>;

    /// 交付一个入站帧（NDEV 接收路的终点）。帧模型在缓冲轮次细化。
    fn receive_frame(&mut self, frame: &[u8]) -> Result<(), i32>;

    /// 从栈取一个出站帧到服务提供的缓冲，返回写入长度；`None` 表示
    /// 暂无待发帧。
    fn transmit_frame(&mut self, frame: &mut [u8]) -> Option<usize>;

    // -- UDP 半（第 09 篇的栈面；家族操作按批上墙）--

    /// 绑定本地端点；`None` = 通配地址加临时端口语义由实现定（栈侧
    /// 分配）。非 UDP 句柄或重复绑定按栈错误回答。
    fn bind_udp(&mut self, socket: StackSocket, local: Option<StackEndpoint>) -> Result<(), i32>;

    /// 设默认对端（connect 的 UDP 语义）：此后 [`Self::send_udp`] 不带
    /// 地址即发往对端。
    fn connect_udp(&mut self, socket: StackSocket, remote: StackEndpoint) -> Result<(), i32>;

    /// 发送一段数据；带 `remote` 即 sendto 语义，`None` 走 connect 定
    /// 下的默认对端。返回实际入栈字节数（栈缓冲满按 EWOULDBLOCK 类
    /// 错误回答，不部分发送）。
    fn send_udp(
        &mut self,
        socket: StackSocket,
        data: &[u8],
        remote: Option<StackEndpoint>,
    ) -> Result<usize, i32>;

    /// 接收一段数据，返回 `(字节数, 发送方端点)`；无到包按 EWOULDBLOCK
    /// 类错误回答（调用方决定挂起还是立即返回）。
    fn recv_udp(&mut self, socket: StackSocket, data: &mut [u8])
        -> Result<(usize, StackEndpoint), i32>;

    /// 查询本地端点（getsockname 的栈半）；未绑定时返回通配端点。
    fn local_endpoint_udp(&self, socket: StackSocket) -> Result<StackEndpoint, i32>;
}

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

}

// The hook list mirrors `lwiphooks.h` and the patch list mirrors the patch
// directory — both hand-maintained, so the counts freeze at compile time
// instead of burning a runtime test on a fact the compiler already knows
// (N1-P2-5).
#[cfg(test)]
const _: () = {
    assert!(ALL_HOOKS.len() == 4);
    assert!(PATCHES.len() == 4);
};

#[cfg(test)]
mod wall_tests {
    use super::*;
    use alloc::vec::Vec;

    /// 服务持有的固定序列号源：钩子的最小真实现。
    struct FixedSource(u32);

    impl StackHooks for FixedSource {
        fn initial_sequence(&mut self, _quad: ConnectionQuad) -> u32 {
            self.0
        }

        fn gateway_v4(&mut self, _netif_index: u16, destination: [u8; 4]) -> Option<[u8; 4]> {
            (!destination.iter().all(|octet| *octet == 0)).then_some([192, 0, 2, 1])
        }

        fn gateway_v6(
            &mut self,
            _netif_index: u16,
            _destination: [u8; 16],
        ) -> Option<[u8; 16]> {
            None
        }
    }

    /// 形状替身：证明墙上的面足够服务走出"开、泵、查就绪、关"一圈。
    struct ShapeStack {
        opened: Vec<StackSocket>,
        next_index: u16,
        readable: Option<u16>,
    }

    impl ShapeStack {
        fn new() -> Self {
            ShapeStack { opened: Vec::new(), next_index: 0, readable: None }
        }

        fn mark_readable(&mut self, index: u16) {
            self.readable = Some(index);
        }
    }

    impl Stack for ShapeStack {
        fn bind_udp(
            &mut self,
            _socket: StackSocket,
            _local: Option<StackEndpoint>,
        ) -> Result<(), i32> {
            Ok(())
        }

        fn connect_udp(
            &mut self,
            _socket: StackSocket,
            _remote: StackEndpoint,
        ) -> Result<(), i32> {
            Ok(())
        }

        fn send_udp(
            &mut self,
            _socket: StackSocket,
            data: &[u8],
            _remote: Option<StackEndpoint>,
        ) -> Result<usize, i32> {
            Ok(data.len())
        }

        fn recv_udp(
            &mut self,
            _socket: StackSocket,
            data: &mut [u8],
        ) -> Result<(usize, StackEndpoint), i32> {
            let n = data.len().min(3);
            data[..n].copy_from_slice(&[1, 2, 3][..n]);
            Ok((
                n,
                StackEndpoint {
                    addr: Some(StackIpAddr::V4([127, 0, 0, 1])),
                    port: 7,
                },
            ))
        }

        fn local_endpoint_udp(&self, _socket: StackSocket) -> Result<StackEndpoint, i32> {
            Ok(StackEndpoint { addr: None, port: 0 })
        }

        fn poll(&mut self, now_millis: u64) -> PollWhen {
            let _ = now_millis;
            PollWhen::Never
        }

        fn readiness(&self, socket: StackSocket) -> Readiness {
            let live =
                self.opened.iter().any(|open| *open == socket);
            let readable = live && self.readable == Some(socket.index());
            Readiness { readable, writable: live }
        }

        fn open(&mut self, family: StackFamily) -> Result<StackSocket, i32> {
            let socket = StackSocket::new(family, self.next_index);
            self.next_index += 1;
            self.opened.push(socket);
            Ok(socket)
        }

        fn close(&mut self, socket: StackSocket) -> Result<(), i32> {
            let before = self.opened.len();
            self.opened.retain(|open| *open != socket);
            if self.opened.len() == before {
                return Err(crate::util::STACK_BAD_VALUE);
            }
            Ok(())
        }

        fn receive_frame(&mut self, frame: &[u8]) -> Result<(), i32> {
            if frame.is_empty() {
                return Err(crate::util::STACK_BAD_ARGUMENT);
            }
            Ok(())
        }

        fn transmit_frame(&mut self, frame: &mut [u8]) -> Option<usize> {
            frame.get_mut(..4)?.copy_from_slice(&[0xde, 0xad, 0xbe, 0xef]);
            Some(4)
        }
    }

    fn sample_quad() -> ConnectionQuad {
        ConnectionQuad {
            local_address: StackIpAddr::V4([10, 0, 0, 1]),
            local_port: 7,
            remote_address: StackIpAddr::V4([10, 0, 0, 2]),
            remote_port: 40_000,
        }
    }

    #[test]
    fn test_wall_composes_hooks_lifecycle_and_poll() {
        let mut hooks = FixedSource(0x1234_5678);
        let mut stack = ShapeStack::new();

        assert_eq!(hooks.initial_sequence(sample_quad()), 0x1234_5678);
        assert_eq!(
            hooks.gateway_v4(0, [0, 0, 0, 0]),
            None,
            "零目的地即本地链路，无网关"
        );
        assert_eq!(hooks.gateway_v4(0, [10, 0, 0, 2]), Some([192, 0, 2, 1]));

        let socket = stack.open(StackFamily::Tcp).expect("open");
        assert_eq!(socket.family(), StackFamily::Tcp);
        assert!(stack.readiness(socket).writable, "新开即写就绪");
        assert_eq!(stack.poll(1_000), PollWhen::Never);

        stack.mark_readable(socket.index());
        assert!(stack.readiness(socket).readable);

        stack.close(socket).expect("close");
        assert_eq!(
            stack.readiness(socket),
            Readiness::default(),
            "关闭后的句柄不再报任何事件"
        );
        assert!(stack.close(socket).is_err(), "重复关闭报栈错误");
    }

    #[test]
    fn test_wall_frame_seam_moves_bytes_both_ways() {
        let mut stack = ShapeStack::new();

        assert!(stack.receive_frame(&[]).is_err(), "空帧被拒");
        assert!(stack.receive_frame(&[0x45, 0, 0, 20]).is_ok());

        let mut buffer = [0u8; 64];
        assert_eq!(stack.transmit_frame(&mut buffer), Some(4));
        assert_eq!(&buffer[..4], &[0xde, 0xad, 0xbe, 0xef]);
    }
}
