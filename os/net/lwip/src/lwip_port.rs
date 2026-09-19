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

// SO_SNDBUF/SO_RCVBUF 选项的容量契约（C 侧各套接字模块顶部的三档常量；
// 垫片按同档收下、记录，物理缓冲仍按上面两个 lwipopts 常量定容——
// 登记差异见 `25-smoltcp-shim.md` §3）。
/// TCP 发送缓冲下界（`TCP_SNDBUF_MIN`，1，`tcpsock.c:86`）。
pub const TCP_SNDBUF_MIN: usize = 1;
/// TCP 发送缓冲缺省（`TCP_SNDBUF_DEF`，32768，`tcpsock.c:87`）。
pub const TCP_SNDBUF_DEF: usize = 32768;
/// TCP 发送缓冲上界（`TCP_SNDBUF_MAX`，131072，`tcpsock.c:88`）。
pub const TCP_SNDBUF_MAX: usize = 131072;
/// TCP 接收缓冲下界（`TCP_RCVBUF_MIN`，即 `TCP_WND`，`tcpsock.c:89`）。
pub const TCP_RCVBUF_MIN: usize = TCP_WINDOW;
/// TCP 接收缓冲缺省（`TCP_RCVBUF_DEF`，`MAX(TCP_WND, 32768)`；接收窗
/// 16384 小于 32768，取值即 32768，`tcpsock.c:90`）。
pub const TCP_RCVBUF_DEF: usize = 32768;
/// TCP 接收缓冲上界（`TCP_RCVBUF_MAX`，`MAX(TCP_WND, 131072)`；同上
/// 取值即 131072，`tcpsock.c:91`）。
pub const TCP_RCVBUF_MAX: usize = 131072;
/// UDP 发送缓冲下界（`UDP_SNDBUF_MIN`，1，`udpsock.c:29`）。
pub const UDP_SNDBUF_MIN: usize = 1;
/// UDP 发送缓冲缺省（`UDP_SNDBUF_DEF`，8192，`udpsock.c:30`）。
pub const UDP_SNDBUF_DEF: usize = 8192;
/// UDP 发送缓冲上界（`UDP_SNDBUF_MAX`，`UDP_MAX_PAYLOAD`，`udpsock.c:28`/`:31`）。
pub const UDP_SNDBUF_MAX: usize = u16::MAX as usize;
/// UDP 接收缓冲下界（`UDP_RCVBUF_MIN`，即 `MEMPOOL_BUFSIZE`，
/// `lwipopts.h:49` + `udpsock.c:32`）。
pub const UDP_RCVBUF_MIN: usize = 512;
/// UDP 接收缓冲缺省（`UDP_RCVBUF_DEF`，32768，`udpsock.c:33`）。
pub const UDP_RCVBUF_DEF: usize = 32768;
/// UDP 接收缓冲上界（`UDP_RCVBUF_MAX`，65536，`udpsock.c:34`）。
pub const UDP_RCVBUF_MAX: usize = 65536;

/// TCP 保活使能时的空闲间隔（C 使能 `SOF_KEEPALIVE` 后 pcb 的缺省
/// 空闲值 `TCP_KEEPIDLE_DEFAULT`，7200000 毫秒，
/// `lwip/priv/tcp_priv.h:138-139`）。
pub const TCP_KEEPALIVE_IDLE_MS: u64 = 7_200_000;

// 选项名（`sys/sys/socket.h` 的线上值；服务路按名字分派，墙按名字
// 落效果——名字清单在墙上与路上各消费一次）。
/// `SOL_SOCKET`（0xffff，`sys/socket.h:171`）。
pub const SOL_SOCKET: i32 = 0xffff;
/// `SO_KEEPALIVE`（0x0008，`sys/socket.h:124`）。
pub const SO_KEEPALIVE: i32 = 0x0008;
/// `SO_BROADCAST`（0x0020，`sys/socket.h:126`）。
pub const SO_BROADCAST: i32 = 0x0020;
/// `SO_SNDBUF`（0x1001，`sys/socket.h:139`）。
pub const SO_SNDBUF: i32 = 0x1001;
/// `SO_RCVBUF`（0x1002，`sys/socket.h:140`）。
pub const SO_RCVBUF: i32 = 0x1002;

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

    /// 打开一个指定家族的 socket，返回栈侧句柄。`protocol` 只有 RAW
    /// 家族消费（Minix 建户的第三参，`raw_new_ip_type` 的过滤协议，
    /// rawsock.c:314——u8 值域，如 ICMP=1）。
    fn open(&mut self, family: StackFamily, protocol: Option<u8>) -> Result<StackSocket, i32>;

    /// RAW 收发（第 10 篇的栈面）：全报文语义（含 IP 头，HDRINCL）。
    fn send_raw(&mut self, socket: StackSocket, data: &[u8]) -> Result<usize, i32>;

    fn recv_raw(&mut self, socket: StackSocket, data: &mut [u8]) -> Result<usize, i32>;

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

    // -- TCP 半（第 08 篇的栈面）--

    /// 预绑本地端点（bind 的 TCP 语义：栈内监听随 listen 落地）。
    fn bind_tcp(&mut self, socket: StackSocket, local: Option<StackEndpoint>) -> Result<(), i32>;

    /// 转入监听；`backlog` 的语义按 C 收下（栈对"单个监听者一次挂一个
    /// 连接"的模型只记账不承诺队列深度，见 08 篇）。
    fn listen_tcp(&mut self, socket: StackSocket, backlog: u32) -> Result<(), i32>;

    /// 发起连接（connect 的栈半）。非阻塞的调用方语义（EINPROGRESS）
    /// 由服务层折算；这里只报"能否开张"。
    fn connect_tcp(&mut self, socket: StackSocket, remote: StackEndpoint) -> Result<(), i32>;

    /// 受连接（accept 的栈半）：监听者上已有建连则把它整体转成受纳
    /// 套接字并就地换上新监听者，返回 `(受纳句柄, 对端端点)`；无连接
    /// 按阻塞类错误回答（服务层决定挂起还是立即返回）。
    fn accept_tcp(&mut self, socket: StackSocket) -> Result<(StackSocket, StackEndpoint), i32>;

    /// 查询对端端点（getpeername 的栈半）。
    fn remote_endpoint_tcp(&self, socket: StackSocket) -> Result<StackEndpoint, i32>;

    /// 流式发送（write 的栈半）：把数据入栈缓冲，返回**实际入队**
    /// 字节数——流语义允许部分发送，缓冲满时返回 0（调用方决定挂起
    /// 还是以 EAGAIN 回）；连接未建按错误回答。
    fn send_tcp(&mut self, socket: StackSocket, data: &[u8]) -> Result<usize, i32>;

    /// 流式接收（read 的栈半）：返回 `(字节数, 对端已关闭)`。空且连接
    /// 仍开按阻塞类错误回答；对端关闭且数据取尽返回 `(0, true)`——
    /// C 的 read 返回 0 即 EOF。
    fn recv_tcp(&mut self, socket: StackSocket, data: &mut [u8])
        -> Result<(usize, bool), i32>;

    /// 半关/全关（shutdown 的栈半）：写向关闭走体面的 FIN（后续推进
    /// 发出）；读向单独关闭在栈上无对应动词，由实现按能力回答。
    fn shutdown_tcp(&mut self, socket: StackSocket, how: i32) -> Result<(), i32>;

    // -- 选项半（SDEV_SETSOCKOPT/SDEV_GETSOCKOPT 的栈面）--

    /// 读写一个 TCP 套接字的选项（收拢形：`value` 带值即设置，`None`
    /// 即查询并回当前值。设计裁决记于 `25-smoltcp-shim.md` §3——逐选项
    /// 一个墙方法会让墙随着选项清单膨胀，收拢后墙上只有一个口子）。
    /// `name` 是线上的 `SO_*` 值；墙不认识的名字按栈参数错误回答。
    fn sockopt_tcp(
        &mut self,
        socket: StackSocket,
        name: i32,
        value: Option<i32>,
    ) -> Result<Option<i32>, i32>;

    /// 读写一个 UDP 套接字的选项，形状同 [`Stack::sockopt_tcp`]。
    fn sockopt_udp(
        &mut self,
        socket: StackSocket,
        name: i32,
        value: Option<i32>,
    ) -> Result<Option<i32>, i32>;

    // -- ioctl 半（SDEV_IOCTL 的栈面）--

    /// 查询待收首包的载荷长度（FIONREAD 的 UDP 半：C `pktsock_test_recv`
    /// 取队首包的 tot_len，`pktsock.c:886-899`）。空环回 `None`。
    fn pending_recv_udp(&mut self, socket: StackSocket) -> Option<usize>;

    /// 查询接收缓冲的排队字节数（FIONREAD 的 TCP 半：C
    /// `tcpsock_test_recv` 的 `tr_len` 语义，`tcpsock.c:1969`）。
    fn pending_recv_tcp(&self, socket: StackSocket) -> usize;

    /// 查询 RAW 接收环的排队字节数（FIONREAD 的 RAW 半；smoltcp 的
    /// raw 包环只报总量，与 C 的首包语义差异登记）。
    fn pending_recv_raw(&self, socket: StackSocket) -> Option<usize>;

    /// 置/清非阻塞旗标（FIONBIO 的栈半；旗标存服务侧槽位，收发路把它
    /// 折进 MSG_DONTWAIT 判定——C 里 FIONBIO 由 libc 改写为
    /// fcntl(O_NONBLOCK)，`libc/sys/ioctl.c:296`/`:330`，服务侧本模型
    /// 补此位作为同一语义的补充）。
    fn set_nonblock(&mut self, socket: StackSocket, nonblock: bool) -> Result<(), i32>;

    /// 查询非阻塞旗标。
    fn is_nonblock(&self, socket: StackSocket) -> bool;
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
        // 选项容量契约的三档（tcpsock.c:86-91、udpsock.c:29-34）。
        assert_eq!(TCP_SNDBUF_MIN, 1);
        assert_eq!(TCP_SNDBUF_DEF, 32768);
        assert_eq!(TCP_SNDBUF_MAX, 131072);
        assert_eq!(TCP_RCVBUF_MIN, 16384);
        assert_eq!(TCP_RCVBUF_DEF, 32768);
        assert_eq!(TCP_RCVBUF_MAX, 131072);
        assert_eq!(UDP_SNDBUF_MIN, 1);
        assert_eq!(UDP_SNDBUF_DEF, 8192);
        assert_eq!(UDP_SNDBUF_MAX, 65535);
        assert_eq!(UDP_RCVBUF_MIN, 512);
        assert_eq!(UDP_RCVBUF_DEF, 32768);
        assert_eq!(UDP_RCVBUF_MAX, 65536);
        // 保活使能的缺省空闲间隔（lwip/priv/tcp_priv.h:138-139）。
        assert_eq!(TCP_KEEPALIVE_IDLE_MS, 7_200_000);
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

        fn bind_tcp(
            &mut self,
            _socket: StackSocket,
            _local: Option<StackEndpoint>,
        ) -> Result<(), i32> {
            Ok(())
        }

        fn listen_tcp(&mut self, _socket: StackSocket, _backlog: u32) -> Result<(), i32> {
            Ok(())
        }

        fn connect_tcp(
            &mut self,
            _socket: StackSocket,
            _remote: StackEndpoint,
        ) -> Result<(), i32> {
            Ok(())
        }

        fn accept_tcp(&mut self, socket: StackSocket) -> Result<(StackSocket, StackEndpoint), i32> {
            let accepted = StackSocket::new(socket.family(), self.next_index);
            self.next_index += 1;
            Ok((
                accepted,
                StackEndpoint {
                    addr: Some(StackIpAddr::V4([127, 0, 0, 1])),
                    port: 5,
                },
            ))
        }

        fn remote_endpoint_tcp(&self, _socket: StackSocket) -> Result<StackEndpoint, i32> {
            Ok(StackEndpoint {
                addr: Some(StackIpAddr::V4([127, 0, 0, 1])),
                port: 5,
            })
        }

        fn send_tcp(&mut self, _socket: StackSocket, data: &[u8]) -> Result<usize, i32> {
            Ok(data.len())
        }

        fn recv_tcp(
            &mut self,
            _socket: StackSocket,
            data: &mut [u8],
        ) -> Result<(usize, bool), i32> {
            let n = data.len().min(2);
            data[..n].copy_from_slice(&[7, 7][..n]);
            Ok((n, false))
        }

        fn shutdown_tcp(&mut self, _socket: StackSocket, _how: i32) -> Result<(), i32> {
            Ok(())
        }

        fn sockopt_tcp(
            &mut self,
            _socket: StackSocket,
            _name: i32,
            _value: Option<i32>,
        ) -> Result<Option<i32>, i32> {
            Err(crate::util::STACK_BAD_ARGUMENT)
        }

        fn sockopt_udp(
            &mut self,
            _socket: StackSocket,
            _name: i32,
            _value: Option<i32>,
        ) -> Result<Option<i32>, i32> {
            Err(crate::util::STACK_BAD_ARGUMENT)
        }

        fn pending_recv_udp(&mut self, _socket: StackSocket) -> Option<usize> {
            None
        }

        fn pending_recv_tcp(&self, _socket: StackSocket) -> usize {
            0
        }

        fn pending_recv_raw(&self, _socket: StackSocket) -> Option<usize> {
            None
        }

        fn set_nonblock(&mut self, _socket: StackSocket, _nonblock: bool) -> Result<(), i32> {
            Err(crate::util::STACK_BAD_VALUE)
        }

        fn is_nonblock(&self, _socket: StackSocket) -> bool {
            false
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

        fn open(&mut self, family: StackFamily, _protocol: Option<u8>) -> Result<StackSocket, i32> {
            let socket = StackSocket::new(family, self.next_index);
            self.next_index += 1;
            self.opened.push(socket);
            Ok(socket)
        }

        fn send_raw(&mut self, _socket: StackSocket, data: &[u8]) -> Result<usize, i32> {
            Ok(data.len())
        }

        fn recv_raw(&mut self, _socket: StackSocket, data: &mut [u8]) -> Result<usize, i32> {
            let n = data.len().min(2);
            data[..n].copy_from_slice(&[7, 7][..n]);
            Ok(n)
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

        let socket = stack.open(StackFamily::Tcp, None).expect("open");
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
