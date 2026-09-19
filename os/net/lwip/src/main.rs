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
}

impl ProductionHandler {
    fn new(dev_paths: &[String]) -> Self {
        ProductionHandler { dev_paths: dev_paths.to_vec(), seed: 0 }
    }
}

impl minix_net_lwip::server::NetHandler for ProductionHandler {
    fn startup_step(&mut self) -> bool {
        // 十三步链的逐步落点（`lwip.c:203-263`；每步的"已实现/等什么"
        // 落点表见 03-lwip-main-init.md §4.1a）：播种在库内即可真实
        // 完成；栈/接口/驱动各步等 smoltcp 依赖与 16-stage 面落地后
        // 逐步替换——链的形状与推进次序不变。
        if self.seed == 0 {
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
