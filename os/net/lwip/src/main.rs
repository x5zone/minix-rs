//! net server (lwip) entry: startup chain, then the four-road event loop.
//!
//! Production wiring: kernel traps via `minix-sys`, SEF interception via
//! `minix-sef`, road bookkeeping via `minix-netdriver`. The per-road
//! business work lives in `ProductionHandler` — each method is the landing
//! spot for the matching implementation round (stack attachment, MIB
//! tree, filter device; see 17-stage-net/todo.md).

fn main() {
    // rc 脚本的挂载参数（`up lwip -dev /dev/bpf …`）：`-dev` 后跟一个
    // 过滤器设备路径，可重复。路径的消费面是过滤器设备的挂载（其后的
    // 批次）；本壳先把它们收下来交给处理器保管。
    let mut dev_paths: Vec<String> = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "-dev" {
            if let Some(path) = args.next() {
                dev_paths.push(path);
            }
        }
    }

    let mut ipc = minix_net_lwip::server::KernelIpc { transport: minix_sys::ipc::DirectTrapTransport };
    let mut handler = ProductionHandler::new(&dev_paths);
    let mut table = minix_netdriver::socktable::SockTable::new();
    let mut startup = minix_net_lwip::startup::Startup::new();
    // 终止错误（传输持续损坏或启动链失败）在此只有退出一条路；C 侧同位
    // 是 panic（lwip.c:325）——非可观察行为差异，日志归运行时诊断。
    let _ = minix_net_lwip::server::run(&mut ipc, &mut handler, &mut table, &mut startup);
}

/// Per-road work for the running service. 循环接线在本轮（N1-P1-4）完成；
/// 每条路的实现体随各自轮次落地（缓冲与栈 attachment 见 N1-P1-5，
/// RS 生命周期握手已在批一落位为 `wait_for_init` 门）。
struct ProductionHandler {
    /// `-dev` 收下的过滤器设备路径（挂载消费随过滤器批）。
    dev_paths: Vec<String>,
    /// 十三步链第一步的随机种子（时钟混进程号，`lwip.c` 的播种面）。
    seed: u64,
    /// 启动链的阶段序号（`Startup` 每阶段回调一次 [`Self::startup_step`]，
    /// 七阶段覆盖 C 的十三步：`lwip.c:203-263`）。
    stage: usize,
    /// 栈本体（步 2 构造，`lwip.c:208-210`；步 13 之后定时器即算布防——
    /// smoltcp 的定时器服务合成在 `poll` 里，见 `25-smoltcp-shim.md` §3）。
    stack: Option<Box<dyn minix_net_lwip::lwip_port::Stack>>,
    /// 单调时钟的零点：墙上的一切时刻都是毫秒数（C `sys_now` 的
    /// 时间基），宿主侧由这里换算。
    epoch: std::time::Instant,
    /// 拷贝缝（用户 grant 与栈缓冲之间的搬运动词）。
    copy: Box<dyn minix_net_lwip::sockops::CopyTransport>,
    /// 接收留言条（挂起后完成所需的现场，ready-scan 的对象）。
    pending_recvs: Vec<minix_net_lwip::sockops::PendingRecv>,
    /// 挂起续答的待发回执（C 的 `reply` 在事件处理任意点发出；本模型
    /// 收拢进队列，循环尾经 `take_wake_replies` 统一非阻塞发出）。
    wake_replies: Vec<(minix_types::Endpoint, minix_types::Message)>,
    /// 组播成员注册表（步 4 `mcast_init`，`mcast.c:55-66`）。
    mcast: Option<minix_net_lwip::mcast::McastRegistry>,
    /// 路由表（步 9 `route_init`，`route.c:248`）。
    routes: Option<minix_net_lwip::route::RouteTable>,
}

impl ProductionHandler {
    fn new(dev_paths: &[String]) -> Self {
        ProductionHandler {
            dev_paths: dev_paths.to_vec(),
            seed: 0,
            stage: 0,
            stack: None,
            epoch: std::time::Instant::now(),
            copy: Box::new(minix_net_lwip::sockops::SysCopyTransport),
            pending_recvs: Vec::new(),
            wake_replies: Vec::new(),
            mcast: None,
            routes: None,
        }
    }

    /// 当前时刻的毫秒数（墙的时间形状，`lwip_port.rs` 的墙注记）。
    fn now_millis(&self) -> u64 {
        self.epoch.elapsed().as_millis() as u64
    }
}

impl minix_net_lwip::server::NetHandler for ProductionHandler {
    fn startup_step(&mut self) -> bool {
        // 十三步链的逐步落点（`lwip.c:203-263`；每步的"已实现/等什么"
        // 落点表见 03-lwip-main-init.md §2.2）：七阶段覆盖十三步，已
        // 实现的步做真，未到批次的步按"顺序推进、旗标留白"放行——链
        // 的形状与推进次序不变。
        self.stage += 1;
        match self.stage {
            // C 步 1 与步 2 同阶段（`StartupStage::LibraryReady`：种子与
            // 栈库就绪）。步 1 播种（时钟混进程号，`lwip.c:203-206`）；
            // 步 2 栈本体构造（`lwip_init`，`lwip.c:208-210`）——种子交给
            // 栈做随机面（TCP 初始序号，与 C 的 srand48 同位）。步 3/4
            // （事件库与三小工）随后批。
            1 => {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default();
                let pid = minix_sys::pm::getpid_via(&minix_sys::ipc::DirectTrapTransport)
                    .unwrap_or(0) as u32;
                self.seed = now
                    .as_secs()
                    .wrapping_add(now.subsec_micros() as u64)
                    .wrapping_add(pid as u64)
                    .wrapping_mul(0x9E37_79B9_7F4A_7C15);
                // 墙上的栈按 `Stack` 特征持有——换栈实现只动这一行。
                let wall: Box<dyn minix_net_lwip::lwip_port::Stack> = Box::new(
                    minix_net_lwip::stack::SmoltcpStack::<minix_net_lwip::stack::TrunkDevice>::new(
                        self.seed,
                        self.now_millis() as i64,
                    ),
                );
                self.stack = Some(wall);
            }
            // C 步 13 独占第七阶段（`StartupStage::Running`：定时器布防
            // 加置运行标记，`lwip.c:255-263`）。smoltcp 侧布防即"进主
            // 循环先推一遍"，这里先做一次首查兑现 C 注释的语义（后续
            // 主循环每趟的 `check_lwip_timer` 位随主循环批接线）。
            7 => {
                let now = self.now_millis();
                if let Some(stack) = self.stack.as_mut() {
                    stack.poll(now);
                }
            }
            // C 步 3 与步 4 同阶段（`StartupStage::FrameworkReady`：事件
            // 库与三小工就绪）。事件库的实体机制住 `SockTable`（主函数
            // 构造后传入循环），内存池库内已建；这里补组播成员注册表
            // （`mcast_init`，`mcast.c:55-66`）。初始序号小工是栈内同位
            // （种子已随步 2 交栈，见 `24-liblwip-port.md` §1.5 偏差表）。
            2 => {
                self.mcast = Some(minix_net_lwip::mcast::McastRegistry::new());
            }
            // C 步 8~10 同阶段（`StartupStage::LowSocketsReady`）：路由
            // 表在 `route_init`（`route.c:248`）清表建账；rtsock/lnksock
            // 两域的建户随后批接线。
            5 => {
                self.routes = Some(minix_net_lwip::route::RouteTable::new());
            }
            // C 步 6/7（网卡）等 16-stage 数据路径；其余步维持留白。
            _ => {}
        }
        true
    }

    fn keep_running(&mut self) -> bool {
        true
    }

    fn notify_clock(
        &mut self,
        table: &mut minix_netdriver::socktable::SockTable,
        _tick: &minix_types::Message,
    ) {
        // 时钟响铃查主定时器（C `expire_timers`，`lwip.c:328`）：smoltcp
        // 把"到点服务定时器"合成进推进，垫片的 `poll` 一次做完
        // （`25-smoltcp-shim.md` §4）。
        let now = self.now_millis();
        if let Some(stack) = self.stack.as_mut() {
            stack.poll(now);
        }
        // 带截止时刻的挂起请求到期：按通用回复形状回超时错误（C
        // `sockevent` 的定时器半；`Resume`/`RetestSelect`/`Alarm` 三种
        // 唤醒的产出方随家族操作批接线，本批尚无生产者）。
        let timed_out = table.poll_timers(now);
        for action in timed_out {
            if let minix_netdriver::socktable::WakeAction::TimedOut { id, continuation } = action
            {
                // 摘掉同现场的留言条（超时的接收不再续答数据）。
                self.pending_recvs
                    .retain(|note| !(note.sock_id == id.raw() && note.caller == continuation.caller()));
                let reply = minix_net_lwip::sockops::simple_reply(
                    0,
                    -(minix_types::ETIMEDOUT),
                );
                self.wake_replies.push((continuation.caller(), reply));
            }
        }
        // 就绪扫描：栈报可读的接收留言条在此续答（Resume 唤醒的生产半）。
        if let Some(stack) = self.stack.as_mut() {
            let completed =
                minix_net_lwip::sockops::ready_scan(stack.as_mut(), self.copy.as_mut(), &mut self.pending_recvs);
            self.wake_replies.extend(completed);
        }
    }

    fn notify_dev_mgr(&mut self) {}

    fn management(&mut self, _msg: &minix_types::Message) -> Option<minix_types::Message> {
        None
    }

    fn socket_device(
        &mut self,
        table: &mut minix_netdriver::socktable::SockTable,
        msg: &minix_types::Message,
    ) -> Option<minix_types::Message> {
        // 套接字路的服务半：当前翻译 `SDEV_SOCKET`（建户，C 不挂起、
        // 立即回答）；其余请求族随各自家族批次逐条过墙，先按未接线
        // （ENOSYS）以通用回复形状回答——VFS 侧各续接体对不认识的
        // 回复号折 EIO，不会把通用形状误读成数据。
        use minix_net_lwip::sockops;
        let Some(stack) = self.stack.as_mut() else {
            return Some(sockops::simple_reply(0, -(minix_types::ENOSYS)));
        };
        if msg.m_type == minix_sockdriver::sdev::SdevRequest::Socket as i32 {
            let reply = match sockops::decode_socket(msg) {
                Some(req) => match sockops::open_socket(stack.as_mut(), table, &req, false)
                {
                    Ok(id) => sockops::socket_reply(req.req_id, id),
                    Err(e) => sockops::socket_reply(req.req_id, -e),
                },
                None => sockops::simple_reply(0, -(minix_types::EINVAL)),
            };
            return Some(reply);
        }
        // UDP 家族五条（bind/connect/getsockname/send/receive）过墙：
        use minix_sockdriver::sdev::SdevRequest;
        if matches!(
            msg.m_type,
            x if x == SdevRequest::Bind as i32
                || x == SdevRequest::Connect as i32
                || x == SdevRequest::GetSockName as i32
                || x == SdevRequest::Send as i32
                || x == SdevRequest::Receive as i32
        ) {
            return sockops::translate_udp(
                stack.as_mut(),
                self.copy.as_mut(),
                table,
                &mut self.pending_recvs,
                msg.m_source,
                msg,
            );
        }
        // 其余 sdev 请求：通用形状 + ENOSYS（req_id 在两形状里都在首格）。
        // SAFETY: { req_id@0 } 两族回复共用首格。
        let req_id = unsafe { i32::from_le_bytes(msg.m_u.raw[0..4].try_into().unwrap()) };
        Some(sockops::simple_reply(req_id, -(minix_types::ENOSYS)))
    }

    fn bpf_device(&mut self, _msg: &minix_types::Message) -> Option<minix_types::Message> {
        None
    }

    fn net_device_reply(&mut self, _msg: &minix_types::Message) -> Option<minix_types::Message> {
        None
    }

    fn unexpected(&mut self, _msg: &minix_types::Message, _is_notify: bool) {}

    fn take_wake_replies(&mut self) -> Vec<(minix_types::Endpoint, minix_types::Message)> {
        core::mem::take(&mut self.wake_replies)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // 调特征方法需要特征在作用域内。
    use minix_net_lwip::server::NetHandler as _;

    /// 启动链七步走满：步 1 播种、步 2 构造栈本体、步 13 首查——
    /// 链形与推进次序不变（落点表见 03-lwip-main-init.md §2.2）。
    #[test]
    fn test_startup_chain_builds_stack_and_polls() {
        let mut handler = ProductionHandler::new(&[]);
        for _ in 0..7 {
            assert!(handler.startup_step(), "启动链七步都应成功");
        }
        assert_ne!(handler.seed, 0, "步 1 已播种");
        assert!(handler.stack.is_some(), "步 2 已构造栈本体");
        // 时钟响铃路：布防后的推进不炸（栈内无套接字无定时器，空转）。
        let mut table = minix_netdriver::socktable::SockTable::new();
        handler.notify_clock(&mut table, &minix_types::Message::default());
    }

    /// 建户路的端到端：SDEV_SOCKET 到达 → 栈内开户 + 命名空间分配 →
    /// SocketReply 带回套接字号；未接线的请求族按通用形状回 ENOSYS。
    #[test]
    fn test_socket_road_opens_and_answers_enosys_for_unwired() {
        use minix_net_lwip::server::NetHandler as _;
        let mut handler = ProductionHandler::new(&[]);
        for _ in 0..7 {
            handler.startup_step();
        }
        let mut table = minix_netdriver::socktable::SockTable::new();
        let mut msg = minix_types::Message::default();
        msg.m_type = minix_sockdriver::sdev::SdevRequest::Socket as i32;
        // SAFETY(test): { req_id@0; domain@4; type@8 }（PF_INET=2，
        // SOCK_STREAM=1，sys/sys/socket.h:179）。
        unsafe {
            let raw = &mut msg.m_u.raw;
            raw[0..4].copy_from_slice(&77i32.to_le_bytes());
            raw[4..8].copy_from_slice(&2i32.to_le_bytes());
            raw[8..12].copy_from_slice(&1i32.to_le_bytes());
        }
        let reply = handler.socket_device(&mut table, &msg).expect("建户有回复");
        assert_eq!(
            reply.m_type,
            minix_sockdriver::sdev::SdevReply::SocketReply as i32
        );
        // SAFETY(test): { req_id@0; sock_id@4 }。
        let raw = unsafe { &reply.m_u.raw };
        assert_eq!(i32::from_le_bytes(raw[0..4].try_into().unwrap()), 77);
        let sock_id = i32::from_le_bytes(raw[4..8].try_into().unwrap());
        assert!(sock_id >= 0, "建户成功给非负套接字号");
        let id = minix_netdriver::sockid::SockId::from_raw(sock_id).unwrap();
        assert_eq!(id.class_base(), minix_netdriver::sockid::SOCKID_TCP, "流类型归 TCP 类");
        assert!(table.contains(id), "服务表已登记");
        // 未接线的请求族（Listen 随家族批）：通用形状 + ENOSYS（诚实
        // 拒绝，不装成功）。已接线的 Connect 不再用作反例——它走翻译
        // 与拷贝缝，宿主下如实报拷贝失败。
        let mut other = minix_types::Message::default();
        other.m_type = minix_sockdriver::sdev::SdevRequest::Listen as i32;
        // SAFETY(test): req_id 在两形状里都在首格。
        unsafe {
            other.m_u.raw[0..4].copy_from_slice(&99i32.to_le_bytes());
        }
        let reply2 = handler.socket_device(&mut table, &other).expect("有回复");
        assert_eq!(
            reply2.m_type,
            minix_sockdriver::sdev::SdevReply::Reply as i32
        );
        // SAFETY(test): 状态在第二格。
        let raw2 = unsafe { &reply2.m_u.raw };
        assert_eq!(
            i32::from_le_bytes(raw2[4..8].try_into().unwrap()),
            -(minix_types::ENOSYS)
        );
    }

    /// UDP 数据面端到端（回环设备）：建户 → 绑定 → sendto → receive
    /// 挂起 → 时钟轮推进（帧绕回、ready-scan 唤醒）→ 续答
    /// `SDEV_RECV_REPLY`，数据与对端地址都拷进 canned 的写入账。
    #[test]
    fn test_udp_sendto_recvfrom_loopback_through_road() {
        use minix_net_lwip::server::NetHandler as _;
        let mut handler = ProductionHandler::new(&[]);
        // 换回环栈（挂接口地址）再装箱：墙类型不变，测试看得到全链。
        let mut loop_stack =
            minix_net_lwip::stack::SmoltcpStack::<minix_net_lwip::stack::LoopDevice>::with_device(
                0x5EED,
                minix_net_lwip::stack::LoopDevice::new(),
                0,
            );
        loop_stack.add_address_v4([127, 0, 0, 1], 8, 0);
        handler.stack = Some(Box::new(loop_stack));
        // canned 拷贝：grant 1 = 绑定地址（127.0.0.1:7777），grant 2 =
        // 发送数据；grant 3/4 是接收的地址/数据出向缓冲。
        let mut canned = minix_net_lwip::sockops::CannedCopyTransport::default();
        let mut sa = [0u8; 16];
        sa[0] = 16;
        sa[1] = 2; // AF_INET
        sa[2..4].copy_from_slice(&7777u16.to_be_bytes());
        sa[4..8].copy_from_slice(&[127, 0, 0, 1]);
        canned.from.push((1, sa.to_vec()));
        canned.from.push((2, vec![9, 9, 9, 9]));
        handler.copy = Box::new(canned);

        let mut table = minix_netdriver::socktable::SockTable::new();
        fn road(
            handler: &mut ProductionHandler,
            table: &mut minix_netdriver::socktable::SockTable,
            msg: minix_types::Message,
        ) -> Option<minix_types::Message> {
            handler.socket_device(table, &msg)
        }

        // 1) 建户 UDP（PF_INET=2，SOCK_DGRAM=2）。
        let mut open = minix_types::Message::default();
        open.m_type = minix_sockdriver::sdev::SdevRequest::Socket as i32;
        // SAFETY(test): { req_id@0; domain@4; type@8 }。
        unsafe {
            open.m_u.raw[0..4].copy_from_slice(&5i32.to_le_bytes());
            open.m_u.raw[4..8].copy_from_slice(&2i32.to_le_bytes());
            open.m_u.raw[8..12].copy_from_slice(&2i32.to_le_bytes());
        }
        let reply = road(&mut handler, &mut table, open).expect("建户有回复");
        // SAFETY(test): sock_id@4。
        let sock_id =
            i32::from_le_bytes(unsafe { &reply.m_u.raw }[4..8].try_into().unwrap());
        assert!(sock_id >= 0);

        // 2) 绑定 127.0.0.1:7777。
        let mut bind = minix_types::Message::default();
        bind.m_type = minix_sockdriver::sdev::SdevRequest::Bind as i32;
        // SAFETY(test): { req_id@0; sock_id@4; grant@8; len@12 }。
        unsafe {
            bind.m_u.raw[0..4].copy_from_slice(&6i32.to_le_bytes());
            bind.m_u.raw[4..8].copy_from_slice(&sock_id.to_le_bytes());
            bind.m_u.raw[8..12].copy_from_slice(&1i32.to_le_bytes());
            bind.m_u.raw[12..16].copy_from_slice(&16i32.to_le_bytes());
        }
        let reply = road(&mut handler, &mut table, bind).expect("绑定有回复");
        assert_eq!(reply.m_type, minix_sockdriver::sdev::SdevReply::Reply as i32);
        // SAFETY(test): 状态在第二格。
        assert_eq!(
            i32::from_le_bytes(unsafe { &reply.m_u.raw }[4..8].try_into().unwrap()),
            0,
            "绑定成功"
        );

        // 3) sendto 自身 7777：数据 grant 2、地址 grant 1。
        let mut send = minix_types::Message::default();
        send.m_type = minix_sockdriver::sdev::SdevRequest::Send as i32;
        // SAFETY(test): sendrecv 域序（ipc.h:2305-2317）。
        unsafe {
            send.m_u.raw[0..4].copy_from_slice(&7i32.to_le_bytes());
            send.m_u.raw[4..8].copy_from_slice(&sock_id.to_le_bytes());
            send.m_u.raw[8..12].copy_from_slice(&2i32.to_le_bytes());
            send.m_u.raw[16..24].copy_from_slice(&4usize.to_le_bytes());
            send.m_u.raw[32..36].copy_from_slice(&1i32.to_le_bytes());
            send.m_u.raw[36..40].copy_from_slice(&16i32.to_le_bytes());
            send.m_u.raw[40..44].copy_from_slice(&100i32.to_le_bytes());
        }
        let reply = road(&mut handler, &mut table, send).expect("发送有回复");
        // SAFETY(test): 状态在第二格（发送字节数）。
        assert_eq!(
            i32::from_le_bytes(unsafe { &reply.m_u.raw }[4..8].try_into().unwrap()),
            4
        );

        // 4) recvfrom：无包 → 挂起（无回复、留言条入账）。
        let mut recv = minix_types::Message::default();
        recv.m_type = minix_sockdriver::sdev::SdevRequest::Receive as i32;
        // SAFETY(test): sendrecv 域序；flags@60 = 0（阻塞语义）。
        unsafe {
            recv.m_u.raw[0..4].copy_from_slice(&8i32.to_le_bytes());
            recv.m_u.raw[4..8].copy_from_slice(&sock_id.to_le_bytes());
            recv.m_u.raw[8..12].copy_from_slice(&4i32.to_le_bytes());
            recv.m_u.raw[16..24].copy_from_slice(&64usize.to_le_bytes());
            recv.m_u.raw[32..36].copy_from_slice(&3i32.to_le_bytes());
            recv.m_u.raw[36..40].copy_from_slice(&16i32.to_le_bytes());
            recv.m_u.raw[40..44].copy_from_slice(&100i32.to_le_bytes());
        }
        let caller = minix_types::Endpoint::from_generation_slot(1, 0);
        let mut recv_msg = recv;
        recv_msg.m_source = caller;
        assert!(
            road(&mut handler, &mut table, recv_msg).is_none(),
            "空收挂起：不回复"
        );
        assert_eq!(handler.pending_recvs.len(), 1, "留言条在账");

        // 5) 时钟轮：帧绕回 + ready-scan 续答（epoch 前拉制造时间前进）。
        for _ in 0..10 {
            handler.epoch = handler.epoch - std::time::Duration::from_millis(200);
            handler.notify_clock(&mut table, &minix_types::Message::default());
        }
        assert!(
            handler.pending_recvs.is_empty(),
            "留言条已被 ready-scan 消化"
        );
        let replies = handler.take_wake_replies();
        assert_eq!(replies.len(), 1, "挂起的 receive 得到续答");
        assert_eq!(replies[0].0, caller);
        assert_eq!(
            replies[0].1.m_type,
            minix_sockdriver::sdev::SdevReply::ReceiveReply as i32
        );
        // SAFETY(test): { req_id@0; status@4; ctl_len@8; addr_len@12 }。
        let raw = unsafe { &replies[0].1.m_u.raw };
        assert_eq!(
            i32::from_le_bytes(raw[0..4].try_into().unwrap()),
            8,
            "req_id 原样带回"
        );
        assert_eq!(i32::from_le_bytes(raw[4..8].try_into().unwrap()), 4, "收到 4 字节");
        assert_eq!(u32::from_le_bytes(raw[12..16].try_into().unwrap()), 16, "地址 16 字节");
    }

    /// 挂起续答通道：带截止时刻的挂起请求到期 → 循环尾的待发回执里
    /// 出现超时回复（C `sockevent` 定时器半；回复形状取通用形状，
    /// req_id 槽不用——VFS 侧只读状态格）。
    #[test]
    fn test_suspended_call_times_out_into_wake_reply() {
        use minix_net_lwip::server::NetHandler as _;
        let mut handler = ProductionHandler::new(&[]);
        for _ in 0..7 {
            handler.startup_step();
        }
        let mut table = minix_netdriver::socktable::SockTable::new();
        let id = minix_netdriver::sockid::SockId::from_class(
            minix_netdriver::sockid::SockClass::Tcp,
            1,
        )
        .unwrap();
        table.add(id).unwrap();
        let caller = minix_types::Endpoint::from_generation_slot(1, 0);
        let continuation = minix_netdriver::socktable::Continuation::new(
            minix_sockdriver::sdev::SdevRequest::Connect,
            caller,
            minix_sockdriver::sockevent::SocketEvent::Connect.bits(),
            Some(0),
        )
        .expect("Connect 在可挂起表上");
        table.suspend(id, continuation).unwrap();
        // 时钟响铃过点：定时器到账 → 续答入队（截断时刻 0，任何时刻
        // 都已过期）。
        handler.notify_clock(&mut table, &minix_types::Message::default());
        let replies = handler.take_wake_replies();
        assert_eq!(replies.len(), 1, "超时挂起产生一条续答回执");
        assert_eq!(replies[0].0, caller);
        assert_eq!(
            replies[0].1.m_type,
            minix_sockdriver::sdev::SdevReply::Reply as i32
        );
        // SAFETY(test): 状态在第二格。
        let raw = unsafe { &replies[0].1.m_u.raw };
        assert_eq!(
            i32::from_le_bytes(raw[4..8].try_into().unwrap()),
            -minix_types::ETIMEDOUT
        );
    }
}
