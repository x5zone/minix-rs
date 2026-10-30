//! net server (uds) entry: the socket-device event loop.
//!
//! Production wiring: kernel traps via `minix-sys`, SEF interception via
//! `minix-sef`, road bookkeeping via `minix-netdriver`. The per-road
//! business work lives in `ProductionHandler` — `socket_device` is the
//! landing spot for the uds request implementations (the 21/22 doc tracks).

fn main() {
    let mut ipc = minix_net_uds::server::KernelIpc { transport: minix_sys::ipc::DirectTrapTransport };
    let mut handler = ProductionHandler::new();
    let mut table = minix_netdriver::socktable::SockTable::new();
    // The loop exits with an error when the transport stays broken; the C
    // counterpart panics at the same point — logging belongs to the runtime
    // diagnostics.
    let _ = minix_net_uds::server::run(&mut ipc, &mut handler, &mut table);
}

/// Per-road work for the running service. `running` is cleared by the
/// termination signal (the C shape of uds_signal); `sockets_in_use` follows
/// the object table.
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

    fn draining(&self) -> bool {
        // C `uds_in_use > 0` (uds.c:1358) — the escape hatch cancels only
        // while nobody is using the device.
        self.sockets_in_use > 0
    }

    fn notify_clock(&mut self, _table: &mut minix_netdriver::socktable::SockTable, _tick: &minix_types::Message) {}

    fn socket_device(
        &mut self,
        _table: &mut minix_netdriver::socktable::SockTable,
        _msg: &minix_types::Message,
    ) -> Option<minix_types::Message> {
        // The uds request implementations land with the 21/22 doc tracks; the
        // loop and the bookkeeping are already in place.
        None
    }

    fn on_terminate(&mut self) {
        self.running = false;
    }

    fn unexpected(&mut self, _msg: &minix_types::Message, _is_notify: bool) {}
}
