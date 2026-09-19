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
}

impl ProductionHandler {
    fn new(dev_paths: &[String]) -> Self {
        ProductionHandler {
            dev_paths: dev_paths.to_vec(),
            seed: 0,
            stage: 0,
            stack: None,
            epoch: std::time::Instant::now(),
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
                let wall: Box<dyn minix_net_lwip::lwip_port::Stack> =
                    Box::new(minix_net_lwip::stack::SmoltcpStack::new(
                        self.seed,
                        self.now_millis() as i64,
                    ));
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
            // C 步 3~12 的其余各步维持留白（落点表 §2.2）。
            _ => {}
        }
        true
    }

    fn keep_running(&mut self) -> bool {
        true
    }

    fn notify_clock(
        &mut self,
        _table: &mut minix_netdriver::socktable::SockTable,
        _tick: &minix_types::Message,
    ) {
        // 时钟响铃查主定时器（C `expire_timers`，`lwip.c:328`）：smoltcp
        // 把"到点服务定时器"合成进推进，垫片的 `poll` 一次做完
        // （`25-smoltcp-shim.md` §4）。
        let now = self.now_millis();
        if let Some(stack) = self.stack.as_mut() {
            stack.poll(now);
        }
    }

    fn notify_dev_mgr(&mut self) {}

    fn management(&mut self, _msg: &minix_types::Message) -> Option<minix_types::Message> {
        None
    }

    fn socket_device(
        &mut self,
        _table: &mut minix_netdriver::socktable::SockTable,
        _msg: &minix_types::Message,
    ) -> Option<minix_types::Message> {
        None
    }

    fn bpf_device(&mut self, _msg: &minix_types::Message) -> Option<minix_types::Message> {
        None
    }

    fn net_device_reply(&mut self, _msg: &minix_types::Message) -> Option<minix_types::Message> {
        None
    }

    fn unexpected(&mut self, _msg: &minix_types::Message, _is_notify: bool) {}
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
}
