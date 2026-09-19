//! net server (uds) entry: the socket-device event loop.
//!
//! Production wiring: kernel traps via `minix-sys`, SEF interception via
//! `minix-sef`, road bookkeeping via `minix-netdriver`. The per-road
//! business work lives in `ProductionHandler` — `socket_device` is the
//! landing spot for the uds request implementations (21/22 篇轨道).

fn main() {
    let mut ipc = minix_net_uds::server::KernelIpc { transport: minix_sys::ipc::DirectTrapTransport };
    let mut handler = ProductionHandler::new();
    let mut table = minix_netdriver::socktable::SockTable::new();
    // 传输持续损坏时循环带错误退出；C 侧同位是 panic——日志归运行时诊断。
    let _ = minix_net_uds::server::run(&mut ipc, &mut handler, &mut table);
}

/// Per-road work for the running service. `running` 由终止信号清零
/// （uds_signal 的 C 形状），`sockets_in_use` 随对象表增减。
struct ProductionHandler {
    running: bool,
    sockets_in_use: usize,
}

impl ProductionHandler {
    fn new() -> Self {
        ProductionHandler { running: true, sockets_in_use: 0 }
    }
}

impl minix_net_uds::server::UdsHandler for ProductionHandler {
    fn keep_running(&mut self) -> bool {
        minix_net_uds::core::loop_keeps_running(self.running, self.sockets_in_use)
    }

    fn notify_clock(&mut self, _table: &mut minix_netdriver::socktable::SockTable, _tick: &minix_types::Message) {}

    fn socket_device(
        &mut self,
        _table: &mut minix_netdriver::socktable::SockTable,
        _msg: &minix_types::Message,
    ) -> Option<minix_types::Message> {
        // uds 请求实现体随 21/22 篇轨道落地；循环与记账已就位。
        None
    }

    fn on_terminate(&mut self) {
        self.running = false;
    }

    fn unexpected(&mut self, _msg: &minix_types::Message, _is_notify: bool) {}
}
