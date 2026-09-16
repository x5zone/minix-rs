//! net server (lwip) entry: startup chain, then the four-road event loop.
//!
//! Production wiring: kernel traps via `minix-sys`, SEF interception via
//! `minix-sef`, road bookkeeping via `minix-netdriver`. The per-road
//! business work lives in `ProductionHandler` — each method is the landing
//! spot for the matching implementation round (stack attachment, MIB
//! tree, filter device; see 17-stage-net/todo.md).

fn main() {
    let mut ipc = minix_net_lwip::server::KernelIpc { transport: minix_sys::ipc::DirectTrapTransport };
    let mut handler = ProductionHandler;
    let mut table = minix_netdriver::socktable::SockTable::new();
    let mut startup = minix_net_lwip::startup::Startup::new();
    // 终止错误（传输持续损坏或启动链失败）在此只有退出一条路；C 侧同位
    // 是 panic（lwip.c:325）——非可观察行为差异，日志归运行时诊断。
    let _ = minix_net_lwip::server::run(&mut ipc, &mut handler, &mut table, &mut startup);
}

/// Per-road work for the running service. 循环接线在本轮（N1-P1-4）完成；
/// 每条路的实现体随各自轮次落地（缓冲与栈 attachment 见 N1-P1-5，
/// RS 生命周期握手挂 edge E-NETSTART）。
struct ProductionHandler;

impl minix_net_lwip::server::NetHandler for ProductionHandler {
    fn startup_step(&mut self) -> bool {
        // 每步的实际工作（库初始化、接口注册、驱动模块加载）随栈与 RS
        // 接线落地；顺序与门控已由 Startup 承担。
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
