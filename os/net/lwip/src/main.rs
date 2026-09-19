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
    /// 调用方身份缝（RAW 建户的根门；生产 fail-closed，测试注 canned）。
    identity: Box<dyn minix_net_lwip::sockops::IdentitySource>,
    /// 挂起留言条账本（接收/连接/受纳三类现场，ready-scan 的对象）。
    pending: minix_net_lwip::sockops::PendingTables,
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
            identity: Box::new(minix_net_lwip::sockops::FailClosedIdentity),
            pending: minix_net_lwip::sockops::PendingTables::default(),
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
                self.pending
                    .recvs
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
            let completed = minix_net_lwip::sockops::ready_scan(
                stack.as_mut(),
                self.copy.as_mut(),
                table,
                &mut self.pending,
            );
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
                Some(req) => {
                    let is_root =
                        self.identity.is_root(minix_types::Endpoint(req.user_endpt));
                    match sockops::open_socket(stack.as_mut(), table, &req, is_root) {
                        Ok(id) => sockops::socket_reply(req.req_id, id),
                        Err(e) => sockops::socket_reply(req.req_id, -e.abs()),
                    }
                }
                None => sockops::simple_reply(0, -(minix_types::EINVAL)),
            };
            return Some(reply);
        }
        // 家族操作八条过墙（bind/connect/getsockname/send/receive/
        // listen/accept/getpeername）；按线上套接字的类内部分派到
        // UDP 路或 TCP 路。SetSockOpt/GetSockOpt（选项批）形状独立，
        // 走自己的选项路。
        use minix_sockdriver::sdev::SdevRequest;
        if matches!(
            msg.m_type,
            x if x == SdevRequest::Bind as i32
                || x == SdevRequest::Connect as i32
                || x == SdevRequest::GetSockName as i32
                || x == SdevRequest::Send as i32
                || x == SdevRequest::Receive as i32
                || x == SdevRequest::Listen as i32
                || x == SdevRequest::Accept as i32
                || x == SdevRequest::GetPeerName as i32
        ) {
            return sockops::translate(
                stack.as_mut(),
                self.copy.as_mut(),
                table,
                &mut self.pending,
                msg.m_source,
                msg,
            );
        }
        if matches!(
            msg.m_type,
            x if x == SdevRequest::SetSockOpt as i32
                || x == SdevRequest::GetSockOpt as i32
        ) {
            return sockops::sockopt_road(
                stack.as_mut(),
                self.copy.as_mut(),
                msg.m_source,
                msg,
            );
        }
        if msg.m_type == SdevRequest::Ioctl as i32 {
            return sockops::ioctl_road(
                stack.as_mut(),
                self.copy.as_mut(),
                msg.m_source,
                msg,
            );
        }
        if msg.m_type == SdevRequest::Close as i32 {
            return sockops::close_socket(stack.as_mut(), table, msg);
        }
        if msg.m_type == SdevRequest::Shutdown as i32 {
            return sockops::shutdown_socket(stack.as_mut(), msg);
        }
        if msg.m_type == SdevRequest::Select as i32 {
            return sockops::select_road(
                stack.as_mut(),
                table,
                &mut self.pending,
                msg.m_source,
                msg,
            );
        }
        if msg.m_type == SdevRequest::Cancel as i32 {
            // C：cancel 无回复——沉默让位给原请求的回复。
            sockops::cancel_road(table, &mut self.pending, msg);
            return None;
        }
        if msg.m_type == SdevRequest::SocketPair as i32 {
            // 成对建户随 rtsock/uds 语义批——先按未接线回答。
            // SAFETY(test): req_id 在首格。
            let req_id = unsafe { i32::from_le_bytes(msg.m_u.raw[0..4].try_into().unwrap()) };
            return Some(sockops::simple_reply(req_id, -(minix_types::ENOSYS)));
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
        // 未接线的请求族（SocketPair 随成对语义批）：通用形状 +
        // ENOSYS（诚实拒绝，不装成功）。已接线的 Connect/Listen 不再
        // 用作反例——它们走翻译与拷贝缝/栈错误，各有真实答案；
        // SetSockOpt/GetSockOpt 随选项批接线，走自己的选项路。
        let mut other = minix_types::Message::default();
        other.m_type = minix_sockdriver::sdev::SdevRequest::SocketPair as i32;
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
        assert_eq!(handler.pending.recvs.len(), 1, "留言条在账");

        // 5) 时钟轮：帧绕回 + ready-scan 续答（epoch 前拉制造时间前进）。
        for _ in 0..10 {
            handler.epoch = handler.epoch - std::time::Duration::from_millis(200);
            handler.notify_clock(&mut table, &minix_types::Message::default());
        }
        assert!(
            handler.pending.recvs.is_empty(),
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

    /// TCP 对端装配助手：服务端建户+绑+听，客户端建户+连接，受理——
    /// 时钟轮驱动握手与双续答，返回 `(服务端号, 客户端号, 受纳号)`。
    fn setup_tcp_pair(handler: &mut ProductionHandler, table: &mut minix_netdriver::socktable::SockTable) -> (i32, i32, i32) {
        use minix_net_lwip::server::NetHandler as _;
        fn road(
            handler: &mut ProductionHandler,
            table: &mut minix_netdriver::socktable::SockTable,
            msg: minix_types::Message,
        ) -> Option<minix_types::Message> {
            handler.socket_device(table, &msg)
        }
        let server_ep = minix_types::Endpoint::from_generation_slot(1, 0);
        let client_ep = minix_types::Endpoint::from_generation_slot(1, 1);
        let mut sa = [0u8; 16];
        sa[0] = 16;
        sa[1] = 2;
        sa[2..4].copy_from_slice(&7777u16.to_be_bytes());
        sa[4..8].copy_from_slice(&[127, 0, 0, 1]);
        let open_tcp = |req_id: i32, source: minix_types::Endpoint| {
            let mut m = minix_types::Message::default();
            m.m_type = minix_sockdriver::sdev::SdevRequest::Socket as i32;
            m.m_source = source;
            // SAFETY(test): { req_id@0; domain@4; type@8 }。
            unsafe {
                m.m_u.raw[0..4].copy_from_slice(&req_id.to_le_bytes());
                m.m_u.raw[4..8].copy_from_slice(&2i32.to_le_bytes());
                m.m_u.raw[8..12].copy_from_slice(&1i32.to_le_bytes());
            }
            m
        };
        let reply = road(handler, table, open_tcp(1, server_ep)).expect("服务端建户");
        let server_id = i32::from_le_bytes(unsafe { &reply.m_u.raw }[4..8].try_into().unwrap());
        let mut bind = minix_types::Message::default();
        bind.m_type = minix_sockdriver::sdev::SdevRequest::Bind as i32;
        bind.m_source = server_ep;
        // SAFETY(test): addr 形状。
        unsafe {
            bind.m_u.raw[0..4].copy_from_slice(&2i32.to_le_bytes());
            bind.m_u.raw[4..8].copy_from_slice(&server_id.to_le_bytes());
            bind.m_u.raw[8..12].copy_from_slice(&1i32.to_le_bytes());
            bind.m_u.raw[12..16].copy_from_slice(&16i32.to_le_bytes());
        }
        road(handler, table, bind);
        let mut listen = minix_types::Message::default();
        listen.m_type = minix_sockdriver::sdev::SdevRequest::Listen as i32;
        listen.m_source = server_ep;
        // SAFETY(test): simple 形状。
        unsafe {
            listen.m_u.raw[0..4].copy_from_slice(&3i32.to_le_bytes());
            listen.m_u.raw[4..8].copy_from_slice(&server_id.to_le_bytes());
            listen.m_u.raw[8..12].copy_from_slice(&1i32.to_le_bytes());
        }
        road(handler, table, listen);
        let reply = road(handler, table, open_tcp(4, client_ep)).expect("客户端建户");
        let client_id = i32::from_le_bytes(unsafe { &reply.m_u.raw }[4..8].try_into().unwrap());
        let mut connect = minix_types::Message::default();
        connect.m_type = minix_sockdriver::sdev::SdevRequest::Connect as i32;
        connect.m_source = client_ep;
        // SAFETY(test): addr 形状。
        unsafe {
            connect.m_u.raw[0..4].copy_from_slice(&5i32.to_le_bytes());
            connect.m_u.raw[4..8].copy_from_slice(&client_id.to_le_bytes());
            connect.m_u.raw[8..12].copy_from_slice(&2i32.to_le_bytes());
            connect.m_u.raw[12..16].copy_from_slice(&16i32.to_le_bytes());
        }
        assert!(road(handler, table, connect).is_none(), "阻塞连接挂起");
        let mut accept = minix_types::Message::default();
        accept.m_type = minix_sockdriver::sdev::SdevRequest::Accept as i32;
        accept.m_source = server_ep;
        // SAFETY(test): addr 形状（grant 3 = 对端地址出向缓冲）。
        unsafe {
            accept.m_u.raw[0..4].copy_from_slice(&6i32.to_le_bytes());
            accept.m_u.raw[4..8].copy_from_slice(&server_id.to_le_bytes());
            accept.m_u.raw[8..12].copy_from_slice(&3i32.to_le_bytes());
            accept.m_u.raw[12..16].copy_from_slice(&16i32.to_le_bytes());
            accept.m_u.raw[16..20].copy_from_slice(&server_ep.0.to_le_bytes());
        }
        assert!(road(handler, table, accept).is_none(), "受理挂起");
        for _ in 0..12 {
            handler.epoch = handler.epoch - std::time::Duration::from_millis(200);
            handler.notify_clock(table, &minix_types::Message::default());
        }
        let replies = handler.take_wake_replies();
        assert_eq!(replies.len(), 2, "连接与受纳各一条续答");
        let accept_reply_msg = replies
            .iter()
            .find(|(to, m)| {
                *to == server_ep
                    && m.m_type == minix_sockdriver::sdev::SdevReply::AcceptReply as i32
            })
            .expect("受纳续答");
        let accepted_id = i32::from_le_bytes(
            unsafe { &accept_reply_msg.1.m_u.raw }[4..8].try_into().unwrap(),
        );
        (server_id, client_id, accepted_id)
    }

    /// TCP 控制面端到端（回环设备、全程过路）：服务端建户+绑+听，
    /// 客户端建户+连接（挂起），服务端受理（挂起）——时钟轮驱动握手
    /// 与 ready-scan，连接续答 0、受纳续答新套接字号加对端地址。
    #[test]
    fn test_tcp_connect_accept_loopback_through_road() {
        use minix_net_lwip::server::NetHandler as _;
        let mut handler = ProductionHandler::new(&[]);
        let mut loop_stack =
            minix_net_lwip::stack::SmoltcpStack::<minix_net_lwip::stack::LoopDevice>::with_device(
                0x5EED,
                minix_net_lwip::stack::LoopDevice::new(),
                0,
            );
        loop_stack.add_address_v4([127, 0, 0, 1], 8, 0);
        handler.stack = Some(Box::new(loop_stack));
        let mut canned = minix_net_lwip::sockops::CannedCopyTransport::default();
        let mut sa = [0u8; 16];
        sa[0] = 16;
        sa[1] = 2; // AF_INET
        sa[2..4].copy_from_slice(&7777u16.to_be_bytes());
        sa[4..8].copy_from_slice(&[127, 0, 0, 1]);
        canned.from.push((1, sa.to_vec()));
        canned.from.push((2, sa.to_vec()));
        handler.copy = Box::new(canned);

        let mut table = minix_netdriver::socktable::SockTable::new();
        fn road(
            handler: &mut ProductionHandler,
            table: &mut minix_netdriver::socktable::SockTable,
            msg: minix_types::Message,
        ) -> Option<minix_types::Message> {
            handler.socket_device(table, &msg)
        }
        let server_ep = minix_types::Endpoint::from_generation_slot(1, 0);
        let client_ep = minix_types::Endpoint::from_generation_slot(1, 1);

        // 建户工具：PF_INET + SOCK_STREAM（req_id 区分两端）。
        let open_tcp = |req_id: i32| {
            let mut m = minix_types::Message::default();
            m.m_type = minix_sockdriver::sdev::SdevRequest::Socket as i32;
            m.m_source = minix_types::Endpoint::from_generation_slot(1, 0);
            // SAFETY(test): { req_id@0; domain@4; type@8 }。
            unsafe {
                m.m_u.raw[0..4].copy_from_slice(&req_id.to_le_bytes());
                m.m_u.raw[4..8].copy_from_slice(&2i32.to_le_bytes());
                m.m_u.raw[8..12].copy_from_slice(&1i32.to_le_bytes());
            }
            m
        };

        // 服务端：建户 → 绑 → 听。
        let reply = road(&mut handler, &mut table, open_tcp(1))
            .expect("服务端建户有回复");
        let server_id = i32::from_le_bytes(
            unsafe { &reply.m_u.raw }[4..8].try_into().unwrap(),
        );
        assert!(server_id >= 0);
        let mut bind = minix_types::Message::default();
        bind.m_type = minix_sockdriver::sdev::SdevRequest::Bind as i32;
        bind.m_source = server_ep;
        // SAFETY(test): addr 形状 { req_id@0; sock_id@4; grant@8; len@12 }。
        unsafe {
            bind.m_u.raw[0..4].copy_from_slice(&2i32.to_le_bytes());
            bind.m_u.raw[4..8].copy_from_slice(&server_id.to_le_bytes());
            bind.m_u.raw[8..12].copy_from_slice(&1i32.to_le_bytes());
            bind.m_u.raw[12..16].copy_from_slice(&16i32.to_le_bytes());
        }
        let reply = road(&mut handler, &mut table, bind).expect("绑定有回复");
        assert_eq!(
            i32::from_le_bytes(unsafe { &reply.m_u.raw }[4..8].try_into().unwrap()),
            0,
            "绑定成功"
        );
        let mut listen = minix_types::Message::default();
        listen.m_type = minix_sockdriver::sdev::SdevRequest::Listen as i32;
        listen.m_source = server_ep;
        // SAFETY(test): simple 形状 { req_id@0; sock_id@4; param@8 }。
        unsafe {
            listen.m_u.raw[0..4].copy_from_slice(&3i32.to_le_bytes());
            listen.m_u.raw[4..8].copy_from_slice(&server_id.to_le_bytes());
            listen.m_u.raw[8..12].copy_from_slice(&1i32.to_le_bytes());
        }
        let reply = road(&mut handler, &mut table, listen).expect("监听有回复");
        assert_eq!(
            i32::from_le_bytes(unsafe { &reply.m_u.raw }[4..8].try_into().unwrap()),
            0
        );

        // 客户端：建户 → 连接（阻塞语义 → 挂起）。
        let mut open2 = open_tcp(4);
        open2.m_source = client_ep;
        let reply = road(&mut handler, &mut table, open2).expect("客户端建户有回复");
        let client_id = i32::from_le_bytes(
            unsafe { &reply.m_u.raw }[4..8].try_into().unwrap(),
        );
        assert!(client_id >= 0);
        let mut connect = minix_types::Message::default();
        connect.m_type = minix_sockdriver::sdev::SdevRequest::Connect as i32;
        connect.m_source = client_ep;
        // SAFETY(test): addr 形状。
        unsafe {
            connect.m_u.raw[0..4].copy_from_slice(&5i32.to_le_bytes());
            connect.m_u.raw[4..8].copy_from_slice(&client_id.to_le_bytes());
            connect.m_u.raw[8..12].copy_from_slice(&2i32.to_le_bytes());
            connect.m_u.raw[12..16].copy_from_slice(&16i32.to_le_bytes());
        }
        assert!(
            road(&mut handler, &mut table, connect).is_none(),
            "阻塞 connect 挂起：不回复"
        );
        assert_eq!(handler.pending.connects.len(), 1, "连接留言条在账");

        // 服务端受理：无连接到挂起；有客户端 SYN 在途即等 ready-scan。
        let mut accept = minix_types::Message::default();
        accept.m_type = minix_sockdriver::sdev::SdevRequest::Accept as i32;
        accept.m_source = server_ep;
        // SAFETY(test): addr 形状（grant 3 = 对端地址出向缓冲）。
        unsafe {
            accept.m_u.raw[0..4].copy_from_slice(&6i32.to_le_bytes());
            accept.m_u.raw[4..8].copy_from_slice(&server_id.to_le_bytes());
            accept.m_u.raw[8..12].copy_from_slice(&3i32.to_le_bytes());
            accept.m_u.raw[12..16].copy_from_slice(&16i32.to_le_bytes());
            accept.m_u.raw[16..20].copy_from_slice(&server_ep.0.to_le_bytes());
        }
        assert!(
            road(&mut handler, &mut table, accept).is_none(),
            "受理挂起：等握手完成"
        );
        assert_eq!(handler.pending.accepts.len(), 1);

        // 时钟轮：握手推进 + ready-scan 双续答。
        for _ in 0..12 {
            handler.epoch = handler.epoch - std::time::Duration::from_millis(200);
            handler.notify_clock(&mut table, &minix_types::Message::default());
        }
        assert!(handler.pending.connects.is_empty(), "连接留言条已消化");
        assert!(handler.pending.accepts.is_empty(), "受纳留言条已消化");
        let replies = handler.take_wake_replies();
        assert_eq!(replies.len(), 2, "连接与受纳各一条续答");
        // 连接续答：回客户端，通用形状状态 0。
        let connect_reply = replies
            .iter()
            .find(|(to, m)| *to == client_ep && m.m_type == minix_sockdriver::sdev::SdevReply::Reply as i32)
            .expect("连接续答回客户端");
        assert_eq!(
            i32::from_le_bytes(unsafe { &connect_reply.1.m_u.raw }[4..8].try_into().unwrap()),
            0,
            "建连完成"
        );
        // 受纳续答：回服务端，AcceptReply 带新套接字号与状态 0。
        let accept_reply_msg = replies
            .iter()
            .find(|(to, m)| {
                *to == server_ep
                    && m.m_type == minix_sockdriver::sdev::SdevReply::AcceptReply as i32
            })
            .expect("受纳续答回服务端");
        // SAFETY(test): { req_id@0; sock_id@4; status@8; len@12 }。
        let raw = unsafe { &accept_reply_msg.1.m_u.raw };
        assert_eq!(i32::from_le_bytes(raw[0..4].try_into().unwrap()), 6, "req_id 原样");
        let new_sock_id = i32::from_le_bytes(raw[4..8].try_into().unwrap());
        assert!(new_sock_id >= 0, "受纳给新套接字号");
        assert_eq!(i32::from_le_bytes(raw[8..12].try_into().unwrap()), 0);
        // 新套接字在服务表在册（TCP 类）。
        let id = minix_netdriver::sockid::SockId::from_raw(new_sock_id).unwrap();
        assert_eq!(id.class_base(), minix_netdriver::sockid::SOCKID_TCP);
        assert!(table.contains(id), "受纳套接字已登记");
        // 对端地址已拷进服务端的地址缓冲（canned 写入账 grant 3，
        // canned.written 可断言——本测试以受纳回复的 len=16 为准）。
    }

    /// 请求面收尾：RAW 建户的根门（身份缝）、RT/LNK 两域的服务侧
    /// 建户、CLOSE 的全类闭环（关栈内套接字加摘服务表，槽位复用）。
    #[test]
    fn test_request_surface_completion() {
        use minix_net_lwip::server::NetHandler as _;
        let mut handler = ProductionHandler::new(&[]);
        // 启动链七步：步 2 构造栈本体（建户/关闭都要过栈）。
        for _ in 0..7 {
            handler.startup_step();
        }
        let mut table = minix_netdriver::socktable::SockTable::new();

        // RAW：非根 EACCES（fail-closed 身份缝），根放行得 RAW 类号。
        let raw_open = |handler: &mut ProductionHandler, table: &mut minix_netdriver::socktable::SockTable| {
            let mut m = minix_types::Message::default();
            m.m_type = minix_sockdriver::sdev::SdevRequest::Socket as i32;
            // SAFETY(test): { req_id@0; domain@4; type@8 }。
            unsafe {
                m.m_u.raw[0..4].copy_from_slice(&11i32.to_le_bytes());
                m.m_u.raw[4..8].copy_from_slice(&2i32.to_le_bytes());
                m.m_u.raw[8..12].copy_from_slice(&3i32.to_le_bytes());
            }
            handler.socket_device(table, &m).expect("建户有回复")
        };
        handler.identity = Box::new(minix_net_lwip::sockops::CannedIdentity(false));
        let reply = raw_open(&mut handler, &mut table);
        assert_eq!(
            i32::from_le_bytes(unsafe { &reply.m_u.raw }[4..8].try_into().unwrap()),
            -(minix_types::EACCES),
            "非根 RAW 拒绝"
        );
        handler.identity = Box::new(minix_net_lwip::sockops::CannedIdentity(true));
        let reply = raw_open(&mut handler, &mut table);
        let raw_id = i32::from_le_bytes(unsafe { &reply.m_u.raw }[4..8].try_into().unwrap());
        assert!(raw_id >= 0);
        assert_eq!(
            minix_netdriver::sockid::SockId::from_raw(raw_id)
                .unwrap()
                .class_base(),
            minix_netdriver::sockid::SOCKID_RAW,
            "根 RAW 放行得 RAW 类号"
        );

        // RT/LNK：服务侧建户（不进栈），类号各归其位。
        for (req_id, pf, base) in [
            (12i32, 34i32, minix_netdriver::sockid::SOCKID_RT),
            (13, 18, minix_netdriver::sockid::SOCKID_LNK),
        ] {
            let mut m = minix_types::Message::default();
            m.m_type = minix_sockdriver::sdev::SdevRequest::Socket as i32;
            // SAFETY(test): { req_id@0; domain@4; type@8 }。
            unsafe {
                m.m_u.raw[0..4].copy_from_slice(&req_id.to_le_bytes());
                m.m_u.raw[4..8].copy_from_slice(&pf.to_le_bytes());
                m.m_u.raw[8..12].copy_from_slice(&2i32.to_le_bytes());
            }
            let reply = handler.socket_device(&mut table, &m).expect("服务侧建户有回复");
            let id = i32::from_le_bytes(unsafe { &reply.m_u.raw }[4..8].try_into().unwrap());
            assert!(
                id >= 0,
                "PF_ROUTE/PF_LINK 服务侧建户放行（rtsock/lnksock 的消息面随后批）"
            );
            assert_eq!(
                minix_netdriver::sockid::SockId::from_raw(id)
                    .unwrap()
                    .class_base(),
                base
            );
        }

        // CLOSE：TCP 建户后关——表摘除、栈槽复用（重开得同一下标）。
        let mut open = minix_types::Message::default();
        open.m_type = minix_sockdriver::sdev::SdevRequest::Socket as i32;
        // SAFETY(test): { req_id@0; domain@4; type@8 }。
        unsafe {
            open.m_u.raw[0..4].copy_from_slice(&14i32.to_le_bytes());
            open.m_u.raw[4..8].copy_from_slice(&2i32.to_le_bytes());
            open.m_u.raw[8..12].copy_from_slice(&1i32.to_le_bytes());
        }
        let reply = handler.socket_device(&mut table, &open).expect("TCP 建户有回复");
        let tcp_id = i32::from_le_bytes(unsafe { &reply.m_u.raw }[4..8].try_into().unwrap());
        assert!(tcp_id >= 0);
        let mut close = minix_types::Message::default();
        close.m_type = minix_sockdriver::sdev::SdevRequest::Close as i32;
        // SAFETY(test): simple 形状 { req_id@0; sock_id@4 }。
        unsafe {
            close.m_u.raw[0..4].copy_from_slice(&15i32.to_le_bytes());
            close.m_u.raw[4..8].copy_from_slice(&tcp_id.to_le_bytes());
        }
        let reply = handler.socket_device(&mut table, &close).expect("关闭有回复");
        assert_eq!(
            i32::from_le_bytes(unsafe { &reply.m_u.raw }[4..8].try_into().unwrap()),
            0
        );
        assert!(
            !table.contains(minix_netdriver::sockid::SockId::from_raw(tcp_id).unwrap()),
            "关闭后服务表摘除"
        );
        let reply = handler.socket_device(&mut table, &open).expect("重开有回复");
        let reopened = i32::from_le_bytes(unsafe { &reply.m_u.raw }[4..8].try_into().unwrap());
        assert_eq!(reopened, tcp_id, "槽位回收复用");
    }

    /// TCP 流式数据面端到端：受纳后的连接上，客户端发（数据入栈即
    /// 回字节数）→ 服务端收（此刻无数据挂起）→ 时钟轮推进（数据过
    /// 线、ready-scan 唤醒）→ 续答收到字节数且数据入 canned 写入账。
    #[test]
    fn test_tcp_stream_send_recv_loopback_through_road() {
        use minix_net_lwip::server::NetHandler as _;
        let mut handler = ProductionHandler::new(&[]);
        let mut loop_stack =
            minix_net_lwip::stack::SmoltcpStack::<minix_net_lwip::stack::LoopDevice>::with_device(
                0x5EED,
                minix_net_lwip::stack::LoopDevice::new(),
                0,
            );
        loop_stack.add_address_v4([127, 0, 0, 1], 8, 0);
        handler.stack = Some(Box::new(loop_stack));
        let mut canned = minix_net_lwip::sockops::CannedCopyTransport::default();
        let mut sa = [0u8; 16];
        sa[0] = 16;
        sa[1] = 2;
        sa[2..4].copy_from_slice(&7777u16.to_be_bytes());
        sa[4..8].copy_from_slice(&[127, 0, 0, 1]);
        canned.from.push((1, sa.to_vec()));
        canned.from.push((2, sa.to_vec()));
        // grant 5 = 客户端要发的数据；grant 6 = 服务端收数据的出向缓冲。
        canned.from.push((5, vec![1, 2, 3, 4]));
        handler.copy = Box::new(canned);

        let mut table = minix_netdriver::socktable::SockTable::new();
        let (server_id, client_id, accepted_id) =
            setup_tcp_pair(&mut handler, &mut table);
        assert!(accepted_id >= 0);

        // 客户端 send：数据 grant 5、4 字节（无 addr，流语义）。
        let mut send = minix_types::Message::default();
        send.m_type = minix_sockdriver::sdev::SdevRequest::Send as i32;
        send.m_source = minix_types::Endpoint::from_generation_slot(1, 1);
        // SAFETY(test): sendrecv 域序（data@8/16，flags@44）。
        unsafe {
            send.m_u.raw[0..4].copy_from_slice(&7i32.to_le_bytes());
            send.m_u.raw[4..8].copy_from_slice(&client_id.to_le_bytes());
            send.m_u.raw[8..12].copy_from_slice(&5i32.to_le_bytes());
            send.m_u.raw[16..24].copy_from_slice(&4usize.to_le_bytes());
        }
        let reply = handler.socket_device(&mut table, &send).expect("发送有回复");
        assert_eq!(
            i32::from_le_bytes(unsafe { &reply.m_u.raw }[4..8].try_into().unwrap()),
            4,
            "发送即回实际入栈字节数"
        );

        // 服务端在受纳套接字上收：此刻数据尚未过线 → 挂起。
        let mut recv = minix_types::Message::default();
        recv.m_type = minix_sockdriver::sdev::SdevRequest::Receive as i32;
        recv.m_source = minix_types::Endpoint::from_generation_slot(1, 0);
        // SAFETY(test): sendrecv 域序（data@8/16）。
        unsafe {
            recv.m_u.raw[0..4].copy_from_slice(&8i32.to_le_bytes());
            recv.m_u.raw[4..8].copy_from_slice(&accepted_id.to_le_bytes());
            recv.m_u.raw[8..12].copy_from_slice(&6i32.to_le_bytes());
            recv.m_u.raw[16..24].copy_from_slice(&64usize.to_le_bytes());
        }
        assert!(
            handler.socket_device(&mut table, &recv).is_none(),
            "空收挂起：不回复"
        );
        assert_eq!(handler.pending.tcp_recvs.len(), 1, "TCP 接收留言条在账");

        // 时钟轮：数据过线 + ready-scan 唤醒续答。
        for _ in 0..10 {
            handler.epoch = handler.epoch - std::time::Duration::from_millis(200);
            handler.notify_clock(&mut table, &minix_types::Message::default());
        }
        assert!(handler.pending.tcp_recvs.is_empty(), "留言条已消化");
        let replies = handler.take_wake_replies();
        assert_eq!(replies.len(), 1, "挂起的 receive 得到续答");
        assert_eq!(
            replies[0].1.m_type,
            minix_sockdriver::sdev::SdevReply::ReceiveReply as i32
        );
        // SAFETY(test): { req_id@0; status@4 }。
        let raw = unsafe { &replies[0].1.m_u.raw };
        assert_eq!(i32::from_le_bytes(raw[0..4].try_into().unwrap()), 8, "req_id 原样");
        assert_eq!(i32::from_le_bytes(raw[4..8].try_into().unwrap()), 4, "收到 4 字节");
    }

    /// 选项路过墙 e2e：建户 TCP → SetSockOpt(SO_KEEPALIVE) 回 0 →
    /// GetSockOpt(SO_KEEPALIVE) 回长度 4 且值拷进 canned 写入账。
    #[test]
    fn test_sockopt_road_roundtrip_through_socket_device() {
        use minix_net_lwip::server::NetHandler as _;
        let mut handler = ProductionHandler::new(&[]);
        for _ in 0..7 {
            handler.startup_step();
        }
        let mut table = minix_netdriver::socktable::SockTable::new();
        let mut canned = minix_net_lwip::sockops::CannedCopyTransport::default();
        canned.from.push((1, 1i32.to_le_bytes().to_vec()));
        handler.copy = Box::new(canned);

        // 建户 TCP。
        let mut open = minix_types::Message::default();
        open.m_type = minix_sockdriver::sdev::SdevRequest::Socket as i32;
        // SAFETY(test): { req_id@0; domain@4; type@8 }。
        unsafe {
            open.m_u.raw[0..4].copy_from_slice(&1i32.to_le_bytes());
            open.m_u.raw[4..8].copy_from_slice(&2i32.to_le_bytes());
            open.m_u.raw[8..12].copy_from_slice(&1i32.to_le_bytes());
        }
        let reply = handler.socket_device(&mut table, &open).expect("建户");
        let sock_id = i32::from_le_bytes(unsafe { &reply.m_u.raw }[4..8].try_into().unwrap());

        // SetSockOpt(SOL_SOCKET, SO_KEEPALIVE, 1)：成功回 0。
        let mut set = minix_types::Message::default();
        set.m_type = minix_sockdriver::sdev::SdevRequest::SetSockOpt as i32;
        // SAFETY(test): { req_id@0; sock_id@4; level@8; name@12; grant@16; len@20 }。
        unsafe {
            let raw = &mut set.m_u.raw;
            raw[0..4].copy_from_slice(&2i32.to_le_bytes());
            raw[4..8].copy_from_slice(&sock_id.to_le_bytes());
            raw[8..12].copy_from_slice(&0xffffi32.to_le_bytes());
            raw[12..16].copy_from_slice(&0x0008i32.to_le_bytes());
            raw[16..20].copy_from_slice(&1i32.to_le_bytes());
            raw[20..24].copy_from_slice(&4i32.to_le_bytes());
        }
        let reply = handler.socket_device(&mut table, &set).expect("设置有回复");
        assert_eq!(
            i32::from_le_bytes(unsafe { &reply.m_u.raw }[4..8].try_into().unwrap()),
            0,
            "保活使能成功"
        );

        // GetSockOpt：回拷出长度 4，值 1 在 canned 写入账。
        let mut get = minix_types::Message::default();
        get.m_type = minix_sockdriver::sdev::SdevRequest::GetSockOpt as i32;
        // SAFETY(test): getset 域序（grant 2 = 出向缓冲）。
        unsafe {
            let raw = &mut get.m_u.raw;
            raw[0..4].copy_from_slice(&3i32.to_le_bytes());
            raw[4..8].copy_from_slice(&sock_id.to_le_bytes());
            raw[8..12].copy_from_slice(&0xffffi32.to_le_bytes());
            raw[12..16].copy_from_slice(&0x0008i32.to_le_bytes());
            raw[16..20].copy_from_slice(&2i32.to_le_bytes());
            raw[20..24].copy_from_slice(&4i32.to_le_bytes());
        }
        let reply = handler.socket_device(&mut table, &get).expect("查询有回复");
        assert_eq!(
            i32::from_le_bytes(unsafe { &reply.m_u.raw }[4..8].try_into().unwrap()),
            4,
            "回复成功码 = 拷出长度（值面由 sockops 单元测试钉住）"
        );
    }

    /// ioctl 路分派 e2e：FIONBIO 置位回 0（旗标真实落墙）、未知命令
    /// 回 ENOTTY——分派线与请求形状都对路。
    #[test]
    fn test_ioctl_road_dispatched_through_socket_device() {
        use minix_net_lwip::server::NetHandler as _;
        let mut handler = ProductionHandler::new(&[]);
        for _ in 0..7 {
            handler.startup_step();
        }
        let mut table = minix_netdriver::socktable::SockTable::new();
        let mut canned = minix_net_lwip::sockops::CannedCopyTransport::default();
        canned.from.push((1, 1i32.to_le_bytes().to_vec()));
        handler.copy = Box::new(canned);

        // 建户 TCP。
        let mut open = minix_types::Message::default();
        open.m_type = minix_sockdriver::sdev::SdevRequest::Socket as i32;
        // SAFETY(test): { req_id@0; domain@4; type@8 }。
        unsafe {
            open.m_u.raw[0..4].copy_from_slice(&1i32.to_le_bytes());
            open.m_u.raw[4..8].copy_from_slice(&2i32.to_le_bytes());
            open.m_u.raw[8..12].copy_from_slice(&1i32.to_le_bytes());
        }
        let reply = handler.socket_device(&mut table, &open).expect("建户");
        let sock_id = i32::from_le_bytes(unsafe { &reply.m_u.raw }[4..8].try_into().unwrap());

        // FIONBIO（0x8004667E，_IOW('f',126,int)）置位：回 0。
        let mut ioctl = minix_types::Message::default();
        ioctl.m_type = minix_sockdriver::sdev::SdevRequest::Ioctl as i32;
        // SAFETY(test): { req_id@0; sock_id@4; request@8(8B); grant@16 }。
        unsafe {
            let raw = &mut ioctl.m_u.raw;
            raw[0..4].copy_from_slice(&2i32.to_le_bytes());
            raw[4..8].copy_from_slice(&sock_id.to_le_bytes());
            raw[8..16].copy_from_slice(&0x8004_667Eu64.to_le_bytes());
            raw[16..20].copy_from_slice(&1i32.to_le_bytes());
        }
        let reply = handler.socket_device(&mut table, &ioctl).expect("ioctl 有回复");
        assert_eq!(
            i32::from_le_bytes(unsafe { &reply.m_u.raw }[4..8].try_into().unwrap()),
            0,
            "FIONBIO 置位成功"
        );

        // 同一套接字上的未知命令：ENOTTY。
        let mut bogus = ioctl;
        // SAFETY(test): req_id 与 request 域。
        unsafe {
            let raw = &mut bogus.m_u.raw;
            raw[0..4].copy_from_slice(&3i32.to_le_bytes());
            raw[8..16].copy_from_slice(&0x2000_7466u64.to_le_bytes());
        }
        let reply = handler.socket_device(&mut table, &bogus).expect("有回复");
        assert_eq!(
            i32::from_le_bytes(unsafe { &reply.m_u.raw }[4..8].try_into().unwrap()),
            -(minix_types::ENOTTY)
        );
    }

    /// select 双型回复 e2e：UDP 套接字 select(读) 无数据 → 挂起 →
    /// 数据过线后 ready-scan 回一型（就绪位）；cancel 摘账后沉默。
    #[test]
    fn test_select_and_cancel_lifecycle_through_road() {
        use minix_net_lwip::server::NetHandler as _;
        let mut handler = ProductionHandler::new(&[]);
        let mut loop_stack =
            minix_net_lwip::stack::SmoltcpStack::<minix_net_lwip::stack::LoopDevice>::with_device(
                0x5EED,
                minix_net_lwip::stack::LoopDevice::new(),
                0,
            );
        loop_stack.add_address_v4([127, 0, 0, 1], 8, 0);
        handler.stack = Some(Box::new(loop_stack));
        let mut canned = minix_net_lwip::sockops::CannedCopyTransport::default();
        let mut sa = [0u8; 16];
        sa[0] = 16;
        sa[1] = 2;
        sa[2..4].copy_from_slice(&7777u16.to_be_bytes());
        sa[4..8].copy_from_slice(&[127, 0, 0, 1]);
        canned.from.push((1, sa.to_vec()));
        canned.from.push((2, vec![5, 5]));
        handler.copy = Box::new(canned);

        let mut table = minix_netdriver::socktable::SockTable::new();
        let vfs = minix_types::Endpoint::from_generation_slot(1, 0);

        // 建户 UDP + 绑 7777。
        let mut open = minix_types::Message::default();
        open.m_type = minix_sockdriver::sdev::SdevRequest::Socket as i32;
        // SAFETY(test): { req_id@0; domain@4; type@8 }。
        unsafe {
            open.m_u.raw[0..4].copy_from_slice(&1i32.to_le_bytes());
            open.m_u.raw[4..8].copy_from_slice(&2i32.to_le_bytes());
            open.m_u.raw[8..12].copy_from_slice(&2i32.to_le_bytes());
        }
        let reply = handler.socket_device(&mut table, &open).expect("建户");
        let sock_id = i32::from_le_bytes(unsafe { &reply.m_u.raw }[4..8].try_into().unwrap());
        let mut bind = minix_types::Message::default();
        bind.m_type = minix_sockdriver::sdev::SdevRequest::Bind as i32;
        // SAFETY(test): addr 形状。
        unsafe {
            bind.m_u.raw[0..4].copy_from_slice(&2i32.to_le_bytes());
            bind.m_u.raw[4..8].copy_from_slice(&sock_id.to_le_bytes());
            bind.m_u.raw[8..12].copy_from_slice(&1i32.to_le_bytes());
            bind.m_u.raw[12..16].copy_from_slice(&16i32.to_le_bytes());
        }
        let _ = handler.socket_device(&mut table, &bind);

        // select(读)：无数据 → 挂起（不回复）。
        let mut select = minix_types::Message::default();
        select.m_type = minix_sockdriver::sdev::SdevRequest::Select as i32;
        select.m_source = vfs;
        // SAFETY(test): { sock_id@0; ops@4 }（SDEV_OP_RD=0x01）。
        unsafe {
            select.m_u.raw[0..4].copy_from_slice(&sock_id.to_le_bytes());
            select.m_u.raw[4..8].copy_from_slice(&1i32.to_le_bytes());
        }
        assert!(
            handler.socket_device(&mut table, &select).is_none(),
            "未就绪 select 挂起"
        );
        assert_eq!(handler.pending.selects.len(), 1);

        // cancel：摘账并沉默（C sdev.c:952 的 req_id=who_e 语义）。
        let mut cancel = minix_types::Message::default();
        cancel.m_type = minix_sockdriver::sdev::SdevRequest::Cancel as i32;
        // SAFETY(test): simple 形状 { req_id@0=who_e; sock_id@4 }。
        unsafe {
            cancel.m_u.raw[0..4].copy_from_slice(&vfs.0.to_le_bytes());
            cancel.m_u.raw[4..8].copy_from_slice(&sock_id.to_le_bytes());
        }
        assert!(
            handler.socket_device(&mut table, &cancel).is_none(),
            "cancel 无回复（sdev.c:31-32）"
        );
        assert!(handler.pending.selects.is_empty(), "cancel 摘除 select 等待");

        // 重新 select → sendto 喂数据 → 时钟轮 → 一型回复（就绪位 RD）。
        assert!(
            handler.socket_device(&mut table, &select).is_none(),
            "重挂 select"
        );
        let mut send = minix_types::Message::default();
        send.m_type = minix_sockdriver::sdev::SdevRequest::Send as i32;
        // SAFETY(test): sendrecv 域序（data@8/16、addr@32/36/40）。
        unsafe {
            send.m_u.raw[0..4].copy_from_slice(&9i32.to_le_bytes());
            send.m_u.raw[4..8].copy_from_slice(&sock_id.to_le_bytes());
            send.m_u.raw[8..12].copy_from_slice(&2i32.to_le_bytes());
            send.m_u.raw[16..24].copy_from_slice(&2usize.to_le_bytes());
            send.m_u.raw[32..36].copy_from_slice(&1i32.to_le_bytes());
            send.m_u.raw[36..40].copy_from_slice(&16i32.to_le_bytes());
            send.m_u.raw[40..44].copy_from_slice(&vfs.0.to_le_bytes());
        }
        let reply = handler.socket_device(&mut table, &send).expect("发送有回复");
        assert_eq!(
            i32::from_le_bytes(unsafe { &reply.m_u.raw }[4..8].try_into().unwrap()),
            2
        );
        for _ in 0..10 {
            handler.epoch = handler.epoch - std::time::Duration::from_millis(200);
            handler.notify_clock(&mut table, &minix_types::Message::default());
        }
        let replies = handler.take_wake_replies();
        assert_eq!(replies.len(), 1, "select 一型续答");
        assert_eq!(replies[0].0, vfs);
        assert_eq!(
            replies[0].1.m_type,
            minix_sockdriver::sdev::SdevReply::SelectReply1 as i32
        );
        // SAFETY(test): { sock_id@0; status@4 }——就绪位含读。
        let raw = unsafe { &replies[0].1.m_u.raw };
        assert_eq!(i32::from_le_bytes(raw[0..4].try_into().unwrap()), sock_id);
        assert_eq!(
            i32::from_le_bytes(raw[4..8].try_into().unwrap()) & 1,
            1,
            "就绪位含 SDEV_OP_RD"
        );
    }
}
