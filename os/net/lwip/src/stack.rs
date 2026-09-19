//! `stack` — 语义垫片的栈半：smoltcp 一族对 [`crate::lwip_port::Stack`]
//! 墙的实现。
//!
//! 选型裁决（17-stage-net todo N1-P1-3，`[ARCH N-1]`）：协议栈本体走
//! smoltcp 一族，Minix 的套接字语义（可挂起请求、三张 grant 的数据面、
//! sdev 回复形状）由自研垫片承担；服务代码只认 [`crate::lwip_port::Stack`]
//! 特征，不认任何具体栈。本模块就是 smoltcp 侧的那块垫片，映射边界的
//! 完整论证在 `25-smoltcp-shim.md`。
//!
//! C 对应物两处：`lwip_init()`（`lwip.c:208-210`，栈库初始化——这里是
//! 构造接口与套接字集合，种子经 `Config::random_seed` 交给栈做随机面，
//! 与 C 把种子喂 `srand48` 同位）与主定时器（`init_timer(&lwip_timer)`
//! 加 `recheck_timer = TRUE`，`lwip.c:255-259`；主循环的
//! `check_lwip_timer`/`expire_timers` 在 `lwip.c:315`/`:328`）。smoltcp
//! 把"到点的定时器随一遍推进而服务"合成进一次 `Interface::poll`，所以
//! 墙上 [`Stack::poll`] 的返回值（下次交付时刻）就是 C
//! `sys_check_timeouts` 的语义位，布防后"进循环先查一遍"由调用方第一趟
//! 就调 [`Stack::poll`] 兑现。

use alloc::vec;
use alloc::vec::Vec;

use managed::ManagedSlice;
use smoltcp::iface::{Config, Interface, SocketSet};
use smoltcp::phy::{
    ChecksumCapabilities, Device, DeviceCapabilities, Medium, RxToken, TxToken,
};
use smoltcp::socket::raw;
use smoltcp::socket::tcp;
use smoltcp::time::Instant;
use smoltcp::wire::{
    HardwareAddress, IpAddress, IpCidr, IpEndpoint, IpListenEndpoint, IpVersion,
};

use crate::lwip_port::{PollWhen, Readiness, Stack, StackFamily, StackSocket};
use crate::lwip_port::{TCP_SEND_BUFFER, TCP_WINDOW};
use crate::util;

/// 垫片本体：接口、中继设备、套接字集合、槽位表四件套。
///
/// 槽位表把服务侧句柄的 `index` 半边翻译成栈内句柄：下标稳定（关闭只
/// 清槽、不搬移后续槽位，服务手里的句柄永远指同一个套接字），家族随槽
/// 记录——家族对不上的句柄按"已关闭"回答（墙的契约：缺省就绪位，不炸）。
pub struct SmoltcpStack<D: Device = TrunkDevice> {
    iface: Interface,
    device: D,
    sockets: SocketSet<'static>,
    slots: Vec<Option<Slot>>,
    /// 临时端口分配器（connect 未预绑时的本地端口，49152 起）。
    ephemeral: u16,
}

/// 一个在役槽位：栈内句柄、开户家族、UDP 默认对端（connect 语义）、
/// TCP 预绑端点与监听旗标，外加选项面的记录半（收发缓冲容量按
/// lwipopts 契约的缺省档起账，SO_SNDBUF/SO_RCVBUF 选项在界内改记；
/// SO_BROADCAST 与非阻塞旗标各占一位。物理缓冲尺寸不随选项改——
/// smoltcp 的缓冲构造后定容，登记差异见 `25-smoltcp-shim.md` §3）。
#[derive(Debug, Clone, Copy)]
struct Slot {
    handle: smoltcp::iface::SocketHandle,
    family: StackFamily,
    udp_peer: Option<crate::lwip_port::StackEndpoint>,
    tcp_local: Option<crate::lwip_port::StackEndpoint>,
    tcp_listener: bool,
    sndbuf: usize,
    rcvbuf: usize,
    broadcast: bool,
    nonblock: bool,
}

impl<D: Device + 'static> SmoltcpStack<D> {
    /// 栈本体构造（C `lwip_init`，`lwip.c:208-210`）：接口按三层介质
    /// （`HardwareAddress::Ip`——以太帧介质等 16-stage 网卡接缝时再扩），
    /// `seed` 交给栈做随机面（TCP 初始序号等，与 C 的 `srand48` 同位）。
    /// 设备即垫片的墙：缺省 [`TrunkDevice`]，测试换 [`LoopDevice`]。
    pub fn new(seed: u64, now_millis: i64) -> Self
    where
        D: Default,
    {
        Self::with_device(seed, D::default(), now_millis)
    }

    /// 指定设备构造。
    pub fn with_device(seed: u64, device: D, now_millis: i64) -> Self {
        let mut config = Config::new(HardwareAddress::Ip);
        config.random_seed = seed;
        let mut device = device;
        let iface = Interface::new(config, &mut device, Instant::from_millis(now_millis));
        SmoltcpStack {
            iface,
            device,
            sockets: SocketSet::new(vec![]),
            slots: Vec::new(),
            ephemeral: 49152,
        }
    }

    /// 按服务侧句柄取槽：下标越界、空槽、家族对不上都算"已关闭"。
    fn slot(&self, socket: StackSocket) -> Option<Slot> {
        let idx = socket.index() as usize;
        let slot = self.slots.get(idx).copied().flatten()?;
        (slot.family == socket.family()).then_some(slot)
    }

    /// 取槽位的可变引用；越界、空槽、家族对不上为 `None`。
    fn slot_mut(&mut self, socket: StackSocket) -> Option<&mut Slot> {
        let idx = socket.index() as usize;
        match self.slots.get_mut(idx) {
            Some(Some(slot)) if slot.family == socket.family() => Some(slot),
            _ => None,
        }
    }

    /// 取 UDP 套接字的栈内可变引用；句柄失效或家族不符为 `None`。
    fn udp_mut(
        &mut self,
        socket: StackSocket,
    ) -> Option<&mut smoltcp::socket::udp::Socket<'static>> {
        self.slot(socket)
            .filter(|slot| slot.family == StackFamily::Udp)
            .map(|slot| self.sockets.get_mut(slot.handle))
    }

    /// 取 TCP 套接字的栈内可变引用；句柄失效或家族不符为 `None`。
    fn tcp_mut(
        &mut self,
        socket: StackSocket,
    ) -> Option<&mut smoltcp::socket::tcp::Socket<'static>> {
        self.slot(socket)
            .filter(|slot| slot.family == StackFamily::Tcp)
            .map(|slot| self.sockets.get_mut(slot.handle))
    }

    /// 换掉槽位上的栈内套接字（TCP 受连接：监听者的句柄转给受纳
    /// 套接字，监听者换新对象）。
    fn replace_handle(&mut self, socket: StackSocket, handle: smoltcp::iface::SocketHandle) {
        let idx = socket.index() as usize;
        if let Some(Some(slot)) = self.slots.get_mut(idx) {
            slot.handle = handle;
        }
    }

    /// 新开一个 TCP 槽位指向既有句柄（受纳套接字的登记）。选项记录半
    /// 从监听者槽位继承（C `ipsock_clone` 把 sndbuf/rcvbuf 抄给克隆体，
    /// `ipsock.c:157-158`；旗标位同源 `sockevent_clone`）。
    fn push_slot_for(
        &mut self,
        family: StackFamily,
        handle: smoltcp::iface::SocketHandle,
        inherit: Option<Slot>,
    ) -> Option<StackSocket> {
        let mut slot = Slot {
            handle,
            family,
            udp_peer: None,
            tcp_local: None,
            tcp_listener: false,
            sndbuf: crate::lwip_port::TCP_SNDBUF_DEF,
            rcvbuf: crate::lwip_port::TCP_RCVBUF_DEF,
            broadcast: false,
            nonblock: false,
        };
        if let Some(from) = inherit {
            slot.sndbuf = from.sndbuf;
            slot.rcvbuf = from.rcvbuf;
            slot.broadcast = from.broadcast;
            slot.nonblock = from.nonblock;
        }
        let slot = Some(slot);
        let index = match self.slots.iter().position(|s| s.is_none()) {
            Some(free) => {
                self.slots[free] = slot;
                free
            }
            None => {
                self.slots.push(slot);
                self.slots.len() - 1
            }
        };
        Some(StackSocket::new(family, index as u16))
    }

    /// 取一个临时本地端口。
    fn next_ephemeral(&mut self) -> u16 {
        let port = self.ephemeral;
        self.ephemeral = self.ephemeral.wrapping_add(1).max(49152);
        port
    }
}

impl<D: Device + 'static> Stack for SmoltcpStack<D> {
    /// 推进栈：收发包、服务到期定时器，再问栈下次何时需要推进。
    /// smoltcp 的一次 `poll` 就是 C `expire_timers` 的同位动作；返回的
    /// 时刻与 C `sys_now` 同一毫秒时间基。
    fn poll(&mut self, now_millis: u64) -> PollWhen {
        let now = Instant::from_millis(now_millis as i64);
        self.iface.poll(now, &mut self.device, &mut self.sockets);
        match self.iface.poll_delay(now, &self.sockets) {
            // `Duration::total_millis` 本就返回 `u64`，直接相加。
            Some(d) => PollWhen::At(now_millis + d.total_millis()),
            None => PollWhen::Never,
        }
    }

    /// 一个套接字的当前就绪位。TCP 按收发缓冲余量，UDP 与 RAW 按包环
    /// 余量；句柄失效按墙的契约回缺省值。
    fn readiness(&self, socket: StackSocket) -> Readiness {
        let Some(slot) = self.slot(socket) else {
            return Readiness::default();
        };
        match slot.family {
            StackFamily::Tcp => {
                let s = self.sockets.get::<tcp::Socket>(slot.handle);
                if slot.tcp_listener {
                    // 监听者：状态离开 Listen 即有连接挂账（可受理）。
                    Readiness {
                        readable: s.state() != smoltcp::socket::tcp::State::Listen,
                        writable: false,
                    }
                } else {
                    Readiness { readable: s.can_recv(), writable: s.can_send() }
                }
            }
            StackFamily::Udp => {
                let s = self.sockets.get::<smoltcp::socket::udp::Socket>(slot.handle);
                Readiness { readable: s.can_recv(), writable: s.can_send() }
            }
            StackFamily::Raw => {
                let s = self.sockets.get::<raw::Socket>(slot.handle);
                Readiness { readable: s.can_recv(), writable: s.can_send() }
            }
            StackFamily::Icmp => Readiness::default(),
        }
    }

    /// 打开一个指定家族的 socket。TCP 的收发缓冲按 lwipopts 契约定容
    /// （接收窗 [`TCP_WINDOW`]、发送缓冲 [`TCP_SEND_BUFFER`]，
    /// `lwipopts.h:267`/`:282`）；UDP 与 RAW 的包环按同一份契约的量级
    /// 取整块；ICMP 面随第 07 篇的批次接线（C 侧 ICMP 走 RAW 协议口，
    /// 本特性集未编入 smoltcp 的独立 ICMP 套接字），先以通用错误回答。
    fn open(&mut self, family: StackFamily, protocol: Option<u8>) -> Result<StackSocket, i32> {
        let handle = match family {
            StackFamily::Tcp => {
                // SocketBuffer 是托管切片（环形缓冲的构造参数），自有
                // 缓冲经 `ManagedSlice::from` 交栈。
                let rx = ManagedSlice::from(vec![0u8; TCP_WINDOW]);
                let tx = ManagedSlice::from(vec![0u8; TCP_SEND_BUFFER]);
                self.sockets.add(tcp::Socket::new(rx, tx))
            }
            StackFamily::Udp => {
                let rx = smoltcp::socket::udp::PacketBuffer::new(
                    vec![smoltcp::socket::udp::PacketMetadata::EMPTY; 8],
                    vec![0u8; TCP_WINDOW],
                );
                let tx = smoltcp::socket::udp::PacketBuffer::new(
                    vec![smoltcp::socket::udp::PacketMetadata::EMPTY; 8],
                    vec![0u8; TCP_SEND_BUFFER],
                );
                self.sockets.add(smoltcp::socket::udp::Socket::new(rx, tx))
            }
            StackFamily::Raw => {
                // 协议在建户时收下（Minix 第三参，rawsock.c:314 的
                // `raw_new_ip_type` 同位）；None = 通配（收面过滤全放）。
                let rx = raw::PacketBuffer::new(
                    vec![raw::PacketMetadata::EMPTY; 8],
                    vec![0u8; TCP_WINDOW],
                );
                let tx = raw::PacketBuffer::new(
                    vec![raw::PacketMetadata::EMPTY; 8],
                    vec![0u8; TCP_SEND_BUFFER],
                );
                let ip_protocol = protocol.map(smoltcp::wire::IpProtocol::from);
                self.sockets
                    .add(raw::Socket::new(Some(IpVersion::Ipv4), ip_protocol, rx, tx))
            }
            StackFamily::Icmp => return Err(util::ERR_GENERIC),
        };
        // 收发容量的缺省档随家族取（tcpsock.c:87/:90、udpsock.c:30/:33）。
        let (sndbuf, rcvbuf) = match family {
            StackFamily::Tcp => (
                crate::lwip_port::TCP_SNDBUF_DEF,
                crate::lwip_port::TCP_RCVBUF_DEF,
            ),
            _ => (
                crate::lwip_port::UDP_SNDBUF_DEF,
                crate::lwip_port::UDP_RCVBUF_DEF,
            ),
        };
        let slot = Some(Slot {
            handle,
            family,
            udp_peer: None,
            tcp_local: None,
            tcp_listener: false,
            sndbuf,
            rcvbuf,
            broadcast: false,
            nonblock: false,
        });
        let index = match self.slots.iter().position(|s| s.is_none()) {
            Some(free) => {
                self.slots[free] = slot;
                free
            }
            None => {
                self.slots.push(slot);
                self.slots.len() - 1
            }
        };
        Ok(StackSocket::new(family, index as u16))
    }

    /// 关闭一个 socket：清槽、撤栈内套接字。此后它的句柄失效（槽位
    /// 已空，之后再按已关闭句柄回答）。
    fn close(&mut self, socket: StackSocket) -> Result<(), i32> {
        let idx = socket.index() as usize;
        let slot = self.slot(socket).ok_or(util::ERR_GENERIC)?;
        self.slots[idx] = None;
        self.sockets.remove(slot.handle);
        Ok(())
    }

    // -- UDP 半（第 09 篇的栈面）--

    fn bind_udp(
        &mut self,
        socket: StackSocket,
        local: Option<crate::lwip_port::StackEndpoint>,
    ) -> Result<(), i32> {
        let listen = listen_endpoint(local);
        self.udp_mut(socket)
            .ok_or(util::ERR_GENERIC)?
            .bind(listen)
            .map_err(|_| util::ERR_ADDRESS_IN_USE)
    }

    fn connect_udp(
        &mut self,
        socket: StackSocket,
        remote: crate::lwip_port::StackEndpoint,
    ) -> Result<(), i32> {
        // smoltcp 的 UDP 没有连接态：默认对端记在服务侧槽位上，send
        // 不带地址时取它（逐包元数据模型，`send_slice` 的 meta 参数）。
        if remote.addr.is_none() {
            return Err(util::ERR_INVALID);
        }
        let idx = socket.index() as usize;
        match self.slots.get_mut(idx).and_then(|s| s.as_mut()) {
            Some(slot) if slot.family == StackFamily::Udp => {
                slot.udp_peer = Some(remote);
                Ok(())
            }
            _ => Err(util::ERR_GENERIC),
        }
    }

    fn send_udp(
        &mut self,
        socket: StackSocket,
        data: &[u8],
        remote: Option<crate::lwip_port::StackEndpoint>,
    ) -> Result<usize, i32> {
        let endpoint = match remote {
            Some(remote) => {
                let Some(addr) = remote.addr else {
                    return Err(util::ERR_INVALID);
                };
                IpEndpoint { addr: to_ip_address(addr), port: remote.port }
            }
            None => {
                // 无地址即 send 语义：取 connect 定下的默认对端。
                let slot = self.slot(socket).ok_or(util::ERR_GENERIC)?;
                let peer = slot.udp_peer.ok_or(util::ERR_INVALID)?;
                let peer_addr = peer.addr.expect("connect 时已校验地址在场");
                IpEndpoint { addr: to_ip_address(peer_addr), port: peer.port }
            }
        };
        let bound = self
            .udp_mut(socket)
            .map(|s| s.endpoint())
            .ok_or(util::ERR_GENERIC)?;
        if bound.port == 0 {
            return Err(util::ERR_INVALID);
        }
        self.udp_mut(socket)
            .ok_or(util::ERR_GENERIC)?
            .send_slice(data, endpoint)
            .map(|_| data.len())
            .map_err(|_| util::ERR_NO_BUFFERS)
    }

    fn recv_udp(
        &mut self,
        socket: StackSocket,
        data: &mut [u8],
    ) -> Result<(usize, crate::lwip_port::StackEndpoint), i32> {
        self.udp_mut(socket)
            .ok_or(util::ERR_GENERIC)?
            .recv_slice(data)
            .map(|(n, meta)| {
                (
                    n,
                    crate::lwip_port::StackEndpoint {
                        addr: Some(from_ip_address(meta.endpoint.addr)),
                        port: meta.endpoint.port,
                    },
                )
            })
            .map_err(|_| util::ERR_WOULD_BLOCK)
    }

    fn local_endpoint_udp(
        &self,
        socket: StackSocket,
    ) -> Result<crate::lwip_port::StackEndpoint, i32> {
        let slot = self.slot(socket).ok_or(util::ERR_GENERIC)?;
        if slot.family != StackFamily::Udp {
            return Err(util::ERR_GENERIC);
        }
        let listen = self
            .sockets
            .get::<smoltcp::socket::udp::Socket>(slot.handle)
            .endpoint();
        Ok(crate::lwip_port::StackEndpoint {
            addr: listen.addr.map(from_ip_address),
            port: listen.port,
        })
    }

    // -- TCP 半（第 08 篇的栈面）--

    fn bind_tcp(
        &mut self,
        socket: StackSocket,
        local: Option<crate::lwip_port::StackEndpoint>,
    ) -> Result<(), i32> {
        let idx = socket.index() as usize;
        match self.slots.get_mut(idx).and_then(|s| s.as_mut()) {
            Some(slot) if slot.family == StackFamily::Tcp => {
                slot.tcp_local = local;
                Ok(())
            }
            _ => Err(util::ERR_GENERIC),
        }
    }

    fn listen_tcp(&mut self, socket: StackSocket, backlog: u32) -> Result<(), i32> {
        // backlog 收下记账（smoltcp 单监听者一次挂一个连接；多并发
        // 监听随接口批次评估）。
        let _ = backlog;
        let local = self.slot(socket).and_then(|s| s.tcp_local);
        let listen = listen_endpoint(local);
        self.tcp_mut(socket)
            .ok_or(util::ERR_GENERIC)?
            .listen(listen)
            .map_err(|e| match e {
                smoltcp::socket::tcp::ListenError::Unaddressable => util::ERR_INVALID,
                _ => util::ERR_ADDRESS_IN_USE,
            })?;
        let idx = socket.index() as usize;
        if let Some(Some(slot)) = self.slots.get_mut(idx) {
            slot.tcp_listener = true;
        }
        Ok(())
    }

    fn connect_tcp(
        &mut self,
        socket: StackSocket,
        remote: crate::lwip_port::StackEndpoint,
    ) -> Result<(), i32> {
        let Some(addr) = remote.addr else {
            return Err(util::ERR_INVALID);
        };
        let remote = IpEndpoint { addr: to_ip_address(addr), port: remote.port };
        let local_port = match self.slot(socket).and_then(|s| s.tcp_local) {
            Some(local) => local.port,
            None => self.next_ephemeral(),
        };
        // 字段级分离借用：先读槽位（拷走句柄），再同时可变借 iface
        // 上下文与套接字集合。
        let handle = self
            .slot(socket)
            .filter(|slot| slot.family == StackFamily::Tcp)
            .map(|slot| slot.handle)
            .ok_or(util::ERR_GENERIC)?;
        let listen = IpListenEndpoint { addr: None, port: local_port };
        let cx = self.iface.context();
        self.sockets
            .get_mut::<smoltcp::socket::tcp::Socket>(handle)
            .connect(cx, remote, listen)
            .map_err(|_| util::ERR_NO_BUFFERS)
    }

    fn accept_tcp(
        &mut self,
        socket: StackSocket,
    ) -> Result<(StackSocket, crate::lwip_port::StackEndpoint), i32> {
        use smoltcp::socket::tcp::State;
        let slot = self.slot(socket).ok_or(util::ERR_GENERIC)?;
        if slot.family != StackFamily::Tcp || !slot.tcp_listener {
            return Err(util::ERR_GENERIC);
        }
        let state = self.sockets.get::<smoltcp::socket::tcp::Socket>(slot.handle).state();
        if state != State::Established {
            // 只认建连完成的连接；未完成由服务层挂起等待。
            return Err(util::ERR_WOULD_BLOCK);
        }
        let peer = self
            .sockets
            .get::<smoltcp::socket::tcp::Socket>(slot.handle)
            .remote_endpoint()
            .ok_or(util::ERR_GENERIC)?;
        let listen_endpoint = self
            .sockets
            .get::<smoltcp::socket::tcp::Socket>(slot.handle)
            .listen_endpoint();
        // 先挂新监听者（占新句柄），再摘建连的旧对象重新登记——顺序
        // 反了会让集合复用刚释放的槽位号，两个句柄撞号。
        let fresh = {
            let rx = ManagedSlice::from(vec![0u8; TCP_WINDOW]);
            let tx = ManagedSlice::from(vec![0u8; TCP_SEND_BUFFER]);
            smoltcp::socket::tcp::Socket::new(rx, tx)
        };
        let fresh_handle = self.sockets.add(fresh);
        self.replace_handle(socket, fresh_handle);
        let fresh_stack = StackSocket::new(StackFamily::Tcp, socket.index());
        self.tcp_mut(fresh_stack)
            .expect("刚换上的监听者")
            .listen(listen_endpoint)
            .map_err(|_| util::ERR_ADDRESS_IN_USE)?;
        let established = self.sockets.remove(slot.handle);
        let accepted_handle = self.sockets.add(established);
        // 受纳槽位继承监听者的选项记录半（C ipsock_clone 同位）。
        let accepted = self
            .push_slot_for(StackFamily::Tcp, accepted_handle, Some(slot))
            .ok_or(util::ERR_GENERIC)?;
        Ok((
            accepted,
            crate::lwip_port::StackEndpoint {
                addr: Some(from_ip_address(peer.addr)),
                port: peer.port,
            },
        ))
    }

    fn send_tcp(&mut self, socket: StackSocket, data: &[u8]) -> Result<usize, i32> {
        self.tcp_mut(socket)
            .ok_or(util::ERR_GENERIC)?
            .send_slice(data)
            .map_err(|_| util::ERR_NOT_CONNECTED)
    }

    fn recv_tcp(
        &mut self,
        socket: StackSocket,
        data: &mut [u8],
    ) -> Result<(usize, bool), i32> {
        use smoltcp::socket::tcp::RecvError;
        match self.tcp_mut(socket).ok_or(util::ERR_GENERIC)?.recv_slice(data) {
            // 语义三分：有数据；EOF（对端关且取尽）；连接未开。
            Ok(0) => Err(util::ERR_WOULD_BLOCK),
            Ok(n) => Ok((n, false)),
            Err(RecvError::Finished) => Ok((0, true)),
            Err(_) => Err(util::ERR_NOT_CONNECTED),
        }
    }

    fn shutdown_tcp(&mut self, socket: StackSocket, how: i32) -> Result<(), i32> {
        // 写向与双向：体面 FIN（随推进发出，套接字留驻到收尾）。
        // 读向单独关闭在 smoltcp 无对应动词——按能力如实回答。
        const SHUT_RD: i32 = 0;
        const SHUT_WR: i32 = 1;
        const SHUT_RDWR: i32 = 2;
        match how {
            SHUT_WR | SHUT_RDWR => {
                self.tcp_mut(socket)
                    .ok_or(util::ERR_GENERIC)?
                    .close();
                Ok(())
            }
            SHUT_RD => Err(util::ERR_INVALID),
            _ => Err(util::ERR_INVALID),
        }
    }

    // -- 选项半（SDEV_SETSOCKOPT/SDEV_GETSOCKOPT 的栈面）--

    fn sockopt_tcp(
        &mut self,
        socket: StackSocket,
        name: i32,
        value: Option<i32>,
    ) -> Result<Option<i32>, i32> {
        match name {
            // 保活：使能按 C 的 SOF_KEEPALIVE 语义（setsockmask，
            // tcpsock.c:2096-2100）——smoltcp 的间隔型 API 取 C pcb 的
            // 缺省空闲值（TCP_KEEPIDLE_DEFAULT，tcp_priv.h:139）。
            crate::lwip_port::SO_KEEPALIVE => {
                let s = self.tcp_mut(socket).ok_or(util::ERR_GENERIC)?;
                match value {
                    Some(v) => {
                        s.set_keep_alive(
                            (v != 0).then(|| {
                                smoltcp::time::Duration::from_millis(
                                    crate::lwip_port::TCP_KEEPALIVE_IDLE_MS,
                                )
                            }),
                        );
                        Ok(None)
                    }
                    None => Ok(Some(i32::from(s.keep_alive().is_some()))),
                }
            }
            // 收发容量：界内改记（C ipsock_setsockopt:482-510 的界检查
            // 加记账半）；物理缓冲维持 lwipopts 契约尺寸，登记差异。
            crate::lwip_port::SO_SNDBUF => match value {
                Some(v) => {
                    if v <= 0
                        || (v as usize) < crate::lwip_port::TCP_SNDBUF_MIN
                        || (v as usize) > crate::lwip_port::TCP_SNDBUF_MAX
                    {
                        return Err(util::ERR_INVALID);
                    }
                    self.slot_mut(socket).ok_or(util::ERR_GENERIC)?.sndbuf = v as usize;
                    Ok(None)
                }
                None => {
                    let sndbuf = self.slot(socket).ok_or(util::ERR_GENERIC)?.sndbuf;
                    Ok(Some(sndbuf as i32))
                }
            },
            crate::lwip_port::SO_RCVBUF => match value {
                Some(v) => {
                    if v <= 0
                        || (v as usize) < crate::lwip_port::TCP_RCVBUF_MIN
                        || (v as usize) > crate::lwip_port::TCP_RCVBUF_MAX
                    {
                        return Err(util::ERR_INVALID);
                    }
                    self.slot_mut(socket).ok_or(util::ERR_GENERIC)?.rcvbuf = v as usize;
                    Ok(None)
                }
                None => {
                    let rcvbuf = self.slot(socket).ok_or(util::ERR_GENERIC)?.rcvbuf;
                    Ok(Some(rcvbuf as i32))
                }
            },
            _ => Err(util::STACK_BAD_ARGUMENT),
        }
    }

    fn sockopt_udp(
        &mut self,
        socket: StackSocket,
        name: i32,
        value: Option<i32>,
    ) -> Result<Option<i32>, i32> {
        match name {
            // 广播放行：C 在 setsockmask 置 SOF_BROADCAST（udpsock.c:553-
            // 557）；smoltcp 的广播收发是栈内行为（无对位 API），旗标记
            // 在服务侧槽位上——查询语义保留，发送面的差异已登记。
            crate::lwip_port::SO_BROADCAST => match value {
                Some(v) => {
                    self.slot_mut(socket).ok_or(util::ERR_GENERIC)?.broadcast = v != 0;
                    Ok(None)
                }
                None => {
                    let broadcast = self.slot(socket).ok_or(util::ERR_GENERIC)?.broadcast;
                    Ok(Some(i32::from(broadcast)))
                }
            },
            // 容量面与 TCP 同形，界取 UDP 三档。
            crate::lwip_port::SO_SNDBUF => match value {
                Some(v) => {
                    if v <= 0
                        || (v as usize) < crate::lwip_port::UDP_SNDBUF_MIN
                        || (v as usize) > crate::lwip_port::UDP_SNDBUF_MAX
                    {
                        return Err(util::ERR_INVALID);
                    }
                    self.slot_mut(socket).ok_or(util::ERR_GENERIC)?.sndbuf = v as usize;
                    Ok(None)
                }
                None => {
                    let sndbuf = self.slot(socket).ok_or(util::ERR_GENERIC)?.sndbuf;
                    Ok(Some(sndbuf as i32))
                }
            },
            crate::lwip_port::SO_RCVBUF => match value {
                Some(v) => {
                    if v <= 0
                        || (v as usize) < crate::lwip_port::UDP_RCVBUF_MIN
                        || (v as usize) > crate::lwip_port::UDP_RCVBUF_MAX
                    {
                        return Err(util::ERR_INVALID);
                    }
                    self.slot_mut(socket).ok_or(util::ERR_GENERIC)?.rcvbuf = v as usize;
                    Ok(None)
                }
                None => {
                    let rcvbuf = self.slot(socket).ok_or(util::ERR_GENERIC)?.rcvbuf;
                    Ok(Some(rcvbuf as i32))
                }
            },
            _ => Err(util::STACK_BAD_ARGUMENT),
        }
    }

    // -- RAW 半（第 10 篇的栈面；全报文语义 = HDRINCL）--

    fn send_raw(&mut self, socket: StackSocket, data: &[u8]) -> Result<usize, i32> {
        let slot = self
            .slot(socket)
            .filter(|slot| slot.family == StackFamily::Raw)
            .ok_or(util::ERR_GENERIC)?;
        self.sockets
            .get_mut::<raw::Socket>(slot.handle)
            .send_slice(data)
            .map(|_| data.len())
            .map_err(|_| util::ERR_NO_BUFFERS)
    }

    fn recv_raw(&mut self, socket: StackSocket, data: &mut [u8]) -> Result<usize, i32> {
        let slot = self
            .slot(socket)
            .filter(|slot| slot.family == StackFamily::Raw)
            .ok_or(util::ERR_GENERIC)?;
        self.sockets
            .get_mut::<raw::Socket>(slot.handle)
            .recv_slice(data)
            .map_err(|_| util::ERR_WOULD_BLOCK)
    }

    fn remote_endpoint_tcp(
        &self,
        socket: StackSocket,
    ) -> Result<crate::lwip_port::StackEndpoint, i32> {
        let slot = self.slot(socket).ok_or(util::ERR_GENERIC)?;
        if slot.family != StackFamily::Tcp {
            return Err(util::ERR_GENERIC);
        }
        let remote = self
            .sockets
            .get::<smoltcp::socket::tcp::Socket>(slot.handle)
            .remote_endpoint()
            .ok_or(util::ERR_GENERIC)?;
        Ok(crate::lwip_port::StackEndpoint {
            addr: Some(from_ip_address(remote.addr)),
            port: remote.port,
        })
    }

    /// 交付一个入站帧：中继设备没有真网卡可交，按"网络未接"回答。
    /// 真数据路径随 16-stage 网卡接缝落地（N1-P1-5 的帧模型一并细化）。
    fn receive_frame(&mut self, _frame: &[u8]) -> Result<(), i32> {
        Err(util::ERR_NETWORK_DOWN)
    }

    /// 从栈取一个出站帧：中继设备的发送半恒空，恒 `None`（暂无待发帧
    /// ——没有网卡，栈发不出去任何东西）。
    fn transmit_frame(&mut self, _frame: &mut [u8]) -> Option<usize> {
        None
    }
}

/// 墙端点到栈监听端点的翻译（`None` 地址 = 通配）。
fn listen_endpoint(
    endpoint: Option<crate::lwip_port::StackEndpoint>,
) -> IpListenEndpoint {
    let (addr, port) = match endpoint {
        Some(e) => (e.addr.map(to_ip_address), e.port),
        None => (None, 0),
    };
    IpListenEndpoint { addr, port }
}

/// 墙地址到栈地址。
fn to_ip_address(addr: crate::lwip_port::StackIpAddr) -> IpAddress {
    match addr {
        crate::lwip_port::StackIpAddr::V4(octets) => {
            IpAddress::Ipv4(smoltcp::wire::Ipv4Address::new(
                octets[0], octets[1], octets[2], octets[3],
            ))
        }
        crate::lwip_port::StackIpAddr::V6(bytes) => {
            IpAddress::Ipv6(core::net::Ipv6Addr::from(bytes))
        }
    }
}

/// 栈地址到墙地址。
fn from_ip_address(addr: IpAddress) -> crate::lwip_port::StackIpAddr {
    match addr {
        IpAddress::Ipv4(v4) => crate::lwip_port::StackIpAddr::V4(v4.octets()),
        IpAddress::Ipv6(v6) => crate::lwip_port::StackIpAddr::V6(v6.octets()),
    }
}

impl<D: Device + 'static> SmoltcpStack<D> {
    /// 给接口挂一个版本 4 地址（ifconf 的栈半；接口批次会用墙外的一层
    /// 包住它，这里先供测试与缺省配置步使用）。
    pub fn add_address_v4(&mut self, octets: [u8; 4], prefix: u8, now_millis: i64) {
        let cidr = IpCidr::new(
            IpAddress::Ipv4(smoltcp::wire::Ipv4Address::new(
                octets[0], octets[1], octets[2], octets[3],
            )),
            prefix,
        );
        let _ = now_millis;
        self.iface.update_ip_addrs(|addrs| {
            if addrs.push(cidr).is_err() {
                // 地址表满：首版只挂一个地址，替换首条。
                if let Some(first) = addrs.iter_mut().next() {
                    *first = cidr;
                }
            }
        });
    }
}

/// 16-stage 网卡数据路径的接缝（垫片的墙）：收发恒空的中继设备。
///
/// 接口在它上面一切就绪（构造、地址、路由表、套接字集合、定时推进），
/// 只是没有包进也没有包出——每趟推进空转。真驱动落地时换这个类型的
/// 收发两个半；若裁决回退 FFI（N1-P1-3 的墙后面），换的也是这一个点。
#[derive(Debug, Default, Clone, Copy)]
pub struct TrunkDevice;

/// [`TrunkDevice`] 的接收令牌：`receive` 恒 `None`，令牌永不发出去。
#[derive(Debug)]
pub struct TrunkRxToken;

/// [`TrunkDevice`] 的发送令牌：`transmit` 恒 `None`，令牌永不发出去。
#[derive(Debug)]
pub struct TrunkTxToken;

impl RxToken for TrunkRxToken {
    fn consume<R, F>(self, _f: F) -> R
    where
        F: FnOnce(&[u8]) -> R,
    {
        unreachable!("TrunkDevice 的 receive 恒 None，接收令牌不会发出去")
    }
}

impl TxToken for TrunkTxToken {
    fn consume<R, F>(self, _len: usize, _f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        unreachable!("TrunkDevice 的 transmit 恒 None，发送令牌不会发出去")
    }
}

impl Device for TrunkDevice {
    type RxToken<'a> = TrunkRxToken;
    type TxToken<'a> = TrunkTxToken;

    fn receive(&mut self, _timestamp: Instant) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        None
    }

    fn transmit(&mut self, _timestamp: Instant) -> Option<Self::TxToken<'_>> {
        None
    }

    fn capabilities(&self) -> DeviceCapabilities {
        // 非穷尽结构体：crate 外先取缺省再改字段（smoltcp 自己的
        // `phy/mod.rs:54` 同款写法）。
        let mut caps = DeviceCapabilities::default();
        caps.medium = Medium::Ip;
        caps.max_transmission_unit = 1500;
        caps.max_burst_size = None;
        caps.checksum = ChecksumCapabilities::default();
        caps
    }
}

/// 回环设备（仅测试）：发出去的帧绕回接收半，配合接口地址即可在
/// 宿主测试里走完整的数据面（绑、发、推进、收），不依赖任何网卡。
#[derive(Debug, Default)]
pub struct LoopDevice {
    looped: alloc::vec::Vec<alloc::vec::Vec<u8>>,
}

impl LoopDevice {
    /// 空回环设备。
    pub fn new() -> Self {
        LoopDevice { looped: alloc::vec::Vec::new() }
    }
}

impl Device for LoopDevice {
    type RxToken<'a> = LoopRxToken;
    type TxToken<'a> = LoopTxToken<'a>;

    fn receive(&mut self, _timestamp: Instant) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        if self.looped.is_empty() {
            return None;
        }
        let frame = self.looped.remove(0);
        Some((LoopRxToken { frame }, LoopTxToken { queue: &mut self.looped }))
    }

    fn transmit(&mut self, _timestamp: Instant) -> Option<Self::TxToken<'_>> {
        Some(LoopTxToken { queue: &mut self.looped })
    }

    fn capabilities(&self) -> DeviceCapabilities {
        let mut caps = DeviceCapabilities::default();
        caps.medium = Medium::Ip;
        caps.max_transmission_unit = 1500;
        caps
    }
}

/// [`LoopDevice`] 的接收令牌：把绕回的帧交给栈。
#[derive(Debug)]
pub struct LoopRxToken {
    frame: alloc::vec::Vec<u8>,
}

impl RxToken for LoopRxToken {
    fn consume<R, F>(self, f: F) -> R
    where
        F: FnOnce(&[u8]) -> R,
    {
        f(&self.frame)
    }
}

/// [`LoopDevice`] 的发送令牌：帧入绕回队列。
#[derive(Debug)]
pub struct LoopTxToken<'a> {
    queue: &'a mut alloc::vec::Vec<alloc::vec::Vec<u8>>,
}

impl<'a> TxToken for LoopTxToken<'a> {
    fn consume<R, F>(self, _len: usize, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        let mut frame = alloc::vec![0u8; _len];
        let result = f(&mut frame);
        self.queue.push(frame);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_open_close_roundtrip_reuses_slot() {
        let mut stack: SmoltcpStack = SmoltcpStack::new(1, 0);
        let first = stack.open(StackFamily::Tcp, None).expect("TCP 建户");
        assert_eq!(first.index(), 0, "首户占 0 号槽");
        stack.close(first).expect("关闭");
        let second = stack.open(StackFamily::Tcp, None).expect("再开");
        assert_eq!(second.index(), first.index(), "槽位回收复用");
    }

    #[test]
    fn test_open_families_get_distinct_slots() {
        let mut stack: SmoltcpStack = SmoltcpStack::new(1, 0);
        let t = stack.open(StackFamily::Tcp, None).unwrap();
        let u = stack.open(StackFamily::Udp, None).unwrap();
        let r = stack.open(StackFamily::Raw, None).unwrap();
        assert_ne!(t.index(), u.index());
        assert_ne!(u.index(), r.index());
        // ICMP 面未接：诚实回答通用错误，不发假句柄。
        assert_eq!(
            stack.open(StackFamily::Icmp, None).unwrap_err(),
            util::ERR_GENERIC,
            "ICMP 面随第 07 篇批次接线"
        );
    }

    #[test]
    fn test_readiness_after_close_is_default() {
        let mut stack: SmoltcpStack = SmoltcpStack::new(1, 0);
        let ghost = StackSocket::new(StackFamily::Tcp, 9);
        assert_eq!(
            stack.readiness(ghost),
            Readiness::default(),
            "失效句柄回缺省就绪位（墙的契约）"
        );
        // 用 UDP 验证在役位：无连接状态、包环有余量即可发（TCP 的
        // `can_send` 依赖连接建立后的 MSS 协商，新套接字两个位都是假，
        // 与缺省值不可区分，锁不出这个断言）。
        let socket = stack.open(StackFamily::Udp, None).unwrap();
        assert!(!stack.readiness(socket).readable);
        assert!(stack.readiness(socket).writable);
        stack.close(socket).unwrap();
        assert_eq!(stack.readiness(socket), Readiness::default());
    }

    #[test]
    fn test_close_guards_family_and_double_close() {
        let mut stack: SmoltcpStack = SmoltcpStack::new(1, 0);
        let tcp = stack.open(StackFamily::Tcp, None).unwrap();
        // 家族对不上的句柄按"已关闭"回答（不动栈内套接字）。
        let wrong_family = StackSocket::new(StackFamily::Udp, tcp.index());
        assert!(stack.close(wrong_family).is_err());
        stack.close(tcp).expect("首关成功");
        assert!(stack.close(tcp).is_err(), "双关按栈错误回答");
    }

    #[test]
    fn test_udp_datagram_loopback_roundtrip() {
        use crate::lwip_port::StackEndpoint;
        // 回环设备上的完整数据面：绑 → 发到自身 → 推进（帧绕回、
        // 栈收包入套接字缓冲）→ 收到带发送方端点的原样数据。
        let mut stack: SmoltcpStack<LoopDevice> =
            SmoltcpStack::with_device(0x5EED, LoopDevice::new(), 0);
        stack.add_address_v4([127, 0, 0, 1], 8, 0);
        let socket = stack.open(StackFamily::Udp, None).expect("UDP 建户");
        stack
            .bind_udp(
                socket,
                Some(StackEndpoint {
                    addr: Some(crate::lwip_port::StackIpAddr::V4([127, 0, 0, 1])),
                    port: 7777,
                }),
            )
            .expect("绑定 7777");
        let sent = stack
            .send_udp(
                socket,
                &[9, 9, 9, 9],
                Some(StackEndpoint {
                    addr: Some(crate::lwip_port::StackIpAddr::V4([127, 0, 0, 1])),
                    port: 7777,
                }),
            )
            .expect("发送入栈");
        assert_eq!(sent, 4);
        // 一趟推进不足以让帧绕回并被栈消费（发送与接收各需一遍），
        // 推到时间前进为止——有数据后再收。
        let mut received = None;
        for tick in 1..=8u64 {
            stack.poll(tick * 100);
            let mut buf = [0u8; 64];
            match stack.recv_udp(socket, &mut buf) {
                Ok((n, peer)) => {
                    received = Some((n, peer, buf));
                    break;
                }
                Err(_) => continue,
            }
        }
        let (n, peer, buf) = received.expect("回环报文在数趟推进内到达");
        assert_eq!(n, 4);
        assert_eq!(&buf[..4], &[9, 9, 9, 9]);
        assert_eq!(
            peer,
            StackEndpoint {
                addr: Some(crate::lwip_port::StackIpAddr::V4([127, 0, 0, 1])),
                port: 7777,
            },
            "发送方端点随报文带回"
        );
    }

    #[test]
    fn test_tcp_control_plane_loopback_handshake() {
        use crate::lwip_port::StackEndpoint;
        let v4 = |octets: [u8; 4]| Some(crate::lwip_port::StackIpAddr::V4(octets));
        let mut stack: SmoltcpStack<LoopDevice> =
            SmoltcpStack::with_device(0x5EED, LoopDevice::new(), 0);
        stack.add_address_v4([127, 0, 0, 1], 8, 0);

        // 服务端：绑 7777 → 监听。
        let server = stack.open(StackFamily::Tcp, None).expect("服务端建户");
        stack
            .bind_tcp(
                server,
                Some(StackEndpoint { addr: v4([127, 0, 0, 1]), port: 7777 }),
            )
            .expect("绑定");
        stack.listen_tcp(server, 1).expect("监听");

        // 客户端：连 127.0.0.1:7777（本地端口走临时分配）。
        let client = stack.open(StackFamily::Tcp, None).expect("客户端建户");
        stack
            .connect_tcp(
                client,
                StackEndpoint { addr: v4([127, 0, 0, 1]), port: 7777 },
            )
            .expect("connect 开张");

        // 轮询推进：握手在数趟内完成（SYN → SYN-ACK → ACK 经回环设备
        // 各绕回一趟）。可读位在 SYN 到达即亮（状态离开 Listen），此时
        // 受纳仍会回阻塞类错误——真实服务的形状就是唤醒后再试，直到
        // 建连完成。
        let mut outcome = None;
        for tick in 1..=16u64 {
            stack.poll(tick * 100);
            if !stack.readiness(server).readable {
                continue;
            }
            if let Ok(pair) = stack.accept_tcp(server) {
                outcome = Some(pair);
                break;
            }
        }
        let (accepted, peer) = outcome.expect("数趟推进内完成受纳");
        assert_eq!(
            peer,
            StackEndpoint { addr: v4([127, 0, 0, 1]), port: 49152 },
            "对端端点=客户端的临时端口"
        );
        assert_ne!(accepted.index(), server.index(), "受纳套接字是独立槽位");
        // 新监听者继续在岗：再开一个客户端可再次建连。
        assert!(
            stack.readiness(server).writable == false && !stack.readiness(server).readable,
            "换上的新监听者回到等待态"
        );
        let client2 = stack.open(StackFamily::Tcp, None).expect("第二客户端建户");
        stack
            .connect_tcp(
                client2,
                StackEndpoint { addr: v4([127, 0, 0, 1]), port: 7777 },
            )
            .expect("第二连接开张");
        let mut accepted2 = None;
        for tick in 20..=40u64 {
            stack.poll(tick * 100);
            if let Ok(pair) = stack.accept_tcp(server) {
                accepted2 = Some(pair);
                break;
            }
        }
        let (accepted2, _) = accepted2.expect("新监听者继续受理");
        assert_ne!(accepted2.index(), accepted.index());
        // 建连两端可写（established 的 can_send）。
        assert!(stack.readiness(accepted).writable, "受纳套接字可写");
        assert!(stack.readiness(client).writable, "客户端套接字可写");
    }

    #[test]
    fn test_fresh_stack_polls_never() {
        let mut stack: SmoltcpStack = SmoltcpStack::new(0x1234_5678, 0);
        // 空栈无定时器无待办：推进后睡到下一个消息到来。
        assert_eq!(stack.poll(0), PollWhen::Never);
        assert_eq!(stack.poll(10_000), PollWhen::Never);
    }

    #[test]
    fn test_tcp_sockopt_keepalive_and_buffer_bounds() {
        use crate::lwip_port::{SO_KEEPALIVE, SO_RCVBUF, SO_SNDBUF};
        let mut stack: SmoltcpStack = SmoltcpStack::new(1, 0);
        let socket = stack.open(StackFamily::Tcp, None).unwrap();

        // 保活：缺省关；使能后查询为开（间隔取 C 的缺省空闲值）。
        assert_eq!(
            stack.sockopt_tcp(socket, SO_KEEPALIVE, None).unwrap(),
            Some(0),
            "新套接字保活缺省关"
        );
        assert_eq!(stack.sockopt_tcp(socket, SO_KEEPALIVE, Some(1)), Ok(None));
        assert_eq!(
            stack.sockopt_tcp(socket, SO_KEEPALIVE, None).unwrap(),
            Some(1),
            "使能后查询为开"
        );
        assert_eq!(stack.sockopt_tcp(socket, SO_KEEPALIVE, Some(0)), Ok(None));
        assert_eq!(
            stack.sockopt_tcp(socket, SO_KEEPALIVE, None).unwrap(),
            Some(0),
            "清位后查询为关"
        );

        // 容量：缺省档（tcpsock.c:87/:90），界内改记，界外 EINVAL。
        assert_eq!(
            stack.sockopt_tcp(socket, SO_SNDBUF, None).unwrap(),
            Some(crate::lwip_port::TCP_SNDBUF_DEF as i32)
        );
        assert_eq!(stack.sockopt_tcp(socket, SO_SNDBUF, Some(65536)), Ok(None));
        assert_eq!(
            stack.sockopt_tcp(socket, SO_SNDBUF, None).unwrap(),
            Some(65536)
        );
        assert_eq!(
            stack.sockopt_tcp(socket, SO_SNDBUF, Some(0)).unwrap_err(),
            util::ERR_INVALID,
            "零容量拒（C val <= 0 同判）"
        );
        assert_eq!(
            stack.sockopt_tcp(socket, SO_SNDBUF, Some(131073)).unwrap_err(),
            util::ERR_INVALID,
            "超上界拒（tcpsock.c:88）"
        );
        assert_eq!(
            stack.sockopt_tcp(socket, SO_RCVBUF, Some(16383)).unwrap_err(),
            util::ERR_INVALID,
            "低于接收窗拒（tcpsock.c:89：接收缓冲必须不小于窗口）"
        );
        // 未支持的名字：墙按栈参数错误回答（路上折 ENOPROTOOPT）。
        assert_eq!(
            stack.sockopt_tcp(socket, 0x2000, Some(1)).unwrap_err(),
            util::STACK_BAD_ARGUMENT
        );
    }

    #[test]
    fn test_udp_sockopt_broadcast_and_capacity() {
        use crate::lwip_port::{SO_BROADCAST, SO_RCVBUF};
        let mut stack: SmoltcpStack = SmoltcpStack::new(1, 0);
        let socket = stack.open(StackFamily::Udp, None).unwrap();

        // 广播放行：缺省关，置位后查询为开。
        assert_eq!(stack.sockopt_udp(socket, SO_BROADCAST, None).unwrap(), Some(0));
        assert_eq!(stack.sockopt_udp(socket, SO_BROADCAST, Some(1)), Ok(None));
        assert_eq!(stack.sockopt_udp(socket, SO_BROADCAST, None).unwrap(), Some(1));

        // 接收容量：缺省 32768（udpsock.c:33），下界 512（MEMPOOL_BUFSIZE）。
        assert_eq!(
            stack.sockopt_udp(socket, SO_RCVBUF, None).unwrap(),
            Some(crate::lwip_port::UDP_RCVBUF_DEF as i32)
        );
        assert_eq!(
            stack.sockopt_udp(socket, SO_RCVBUF, Some(511)).unwrap_err(),
            util::ERR_INVALID
        );
        assert_eq!(stack.sockopt_udp(socket, SO_RCVBUF, Some(512)), Ok(None));
        // 选项互不串槽：广播位仍在。
        assert_eq!(stack.sockopt_udp(socket, SO_BROADCAST, None).unwrap(), Some(1));
    }
}
