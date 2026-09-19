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
use smoltcp::wire::{HardwareAddress, IpVersion};

use crate::lwip_port::{PollWhen, Readiness, Stack, StackFamily, StackSocket};
use crate::lwip_port::{TCP_SEND_BUFFER, TCP_WINDOW};
use crate::util;

/// 垫片本体：接口、中继设备、套接字集合、槽位表四件套。
///
/// 槽位表把服务侧句柄的 `index` 半边翻译成栈内句柄：下标稳定（关闭只
/// 清槽、不搬移后续槽位，服务手里的句柄永远指同一个套接字），家族随槽
/// 记录——家族对不上的句柄按"已关闭"回答（墙的契约：缺省就绪位，不炸）。
pub struct SmoltcpStack {
    iface: Interface,
    device: TrunkDevice,
    sockets: SocketSet<'static>,
    slots: Vec<Option<Slot>>,
}

/// 一个在役槽位：栈内句柄加开户家族。
#[derive(Debug, Clone, Copy)]
struct Slot {
    handle: smoltcp::iface::SocketHandle,
    family: StackFamily,
}

impl SmoltcpStack {
    /// 栈本体构造（C `lwip_init`，`lwip.c:208-210`）：接口按三层介质
    /// （`HardwareAddress::Ip`——以太帧介质等 16-stage 网卡接缝时再扩），
    /// `seed` 交给栈做随机面（TCP 初始序号等，与 C 的 `srand48` 同位）。
    pub fn new(seed: u64, now_millis: i64) -> Self {
        let mut config = Config::new(HardwareAddress::Ip);
        config.random_seed = seed;
        let mut device = TrunkDevice;
        let iface = Interface::new(config, &mut device, Instant::from_millis(now_millis));
        SmoltcpStack {
            iface,
            device,
            sockets: SocketSet::new(vec![]),
            slots: Vec::new(),
        }
    }

    /// 按服务侧句柄取槽：下标越界、空槽、家族对不上都算"已关闭"。
    fn slot(&self, socket: StackSocket) -> Option<Slot> {
        let idx = socket.index() as usize;
        let slot = self.slots.get(idx).copied().flatten()?;
        (slot.family == socket.family()).then_some(slot)
    }
}

impl Stack for SmoltcpStack {
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
                Readiness { readable: s.can_recv(), writable: s.can_send() }
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
    fn open(&mut self, family: StackFamily) -> Result<StackSocket, i32> {
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
                // 版本与协议留白（通配）：rawsock 模块在自己的批次里按
                // 建户域收窄（第 10 篇）。
                let rx = raw::PacketBuffer::new(
                    vec![raw::PacketMetadata::EMPTY; 8],
                    vec![0u8; TCP_WINDOW],
                );
                let tx = raw::PacketBuffer::new(
                    vec![raw::PacketMetadata::EMPTY; 8],
                    vec![0u8; TCP_SEND_BUFFER],
                );
                self.sockets.add(raw::Socket::new(Some(IpVersion::Ipv4), None, rx, tx))
            }
            StackFamily::Icmp => return Err(util::ERR_GENERIC),
        };
        let slot = Some(Slot { handle, family });
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_open_close_roundtrip_reuses_slot() {
        let mut stack = SmoltcpStack::new(1, 0);
        let first = stack.open(StackFamily::Tcp).expect("TCP 建户");
        assert_eq!(first.index(), 0, "首户占 0 号槽");
        stack.close(first).expect("关闭");
        let second = stack.open(StackFamily::Tcp).expect("再开");
        assert_eq!(second.index(), first.index(), "槽位回收复用");
    }

    #[test]
    fn test_open_families_get_distinct_slots() {
        let mut stack = SmoltcpStack::new(1, 0);
        let t = stack.open(StackFamily::Tcp).unwrap();
        let u = stack.open(StackFamily::Udp).unwrap();
        let r = stack.open(StackFamily::Raw).unwrap();
        assert_ne!(t.index(), u.index());
        assert_ne!(u.index(), r.index());
        // ICMP 面未接：诚实回答通用错误，不发假句柄。
        assert_eq!(
            stack.open(StackFamily::Icmp).unwrap_err(),
            util::ERR_GENERIC,
            "ICMP 面随第 07 篇批次接线"
        );
    }

    #[test]
    fn test_readiness_after_close_is_default() {
        let mut stack = SmoltcpStack::new(1, 0);
        let ghost = StackSocket::new(StackFamily::Tcp, 9);
        assert_eq!(
            stack.readiness(ghost),
            Readiness::default(),
            "失效句柄回缺省就绪位（墙的契约）"
        );
        // 用 UDP 验证在役位：无连接状态、包环有余量即可发（TCP 的
        // `can_send` 依赖连接建立后的 MSS 协商，新套接字两个位都是假，
        // 与缺省值不可区分，锁不出这个断言）。
        let socket = stack.open(StackFamily::Udp).unwrap();
        assert!(!stack.readiness(socket).readable);
        assert!(stack.readiness(socket).writable);
        stack.close(socket).unwrap();
        assert_eq!(stack.readiness(socket), Readiness::default());
    }

    #[test]
    fn test_close_guards_family_and_double_close() {
        let mut stack = SmoltcpStack::new(1, 0);
        let tcp = stack.open(StackFamily::Tcp).unwrap();
        // 家族对不上的句柄按"已关闭"回答（不动栈内套接字）。
        let wrong_family = StackSocket::new(StackFamily::Udp, tcp.index());
        assert!(stack.close(wrong_family).is_err());
        stack.close(tcp).expect("首关成功");
        assert!(stack.close(tcp).is_err(), "双关按栈错误回答");
    }

    #[test]
    fn test_fresh_stack_polls_never() {
        let mut stack = SmoltcpStack::new(0x1234_5678, 0);
        // 空栈无定时器无待办：推进后睡到下一个消息到来。
        assert_eq!(stack.poll(0), PollWhen::Never);
        assert_eq!(stack.poll(10_000), PollWhen::Never);
    }
}
