//! net server (uds) entry: the socket-device event loop.
//!
//! Production wiring: kernel traps via `minix-sys`, SEF interception via
//! `minix-sef`, road bookkeeping via `minix-netdriver`. The per-road
//! business work lives in `ProductionHandler` — `socket_device` is the
//! landing spot for the uds request implementations (the 21/22 doc tracks).

fn main() {
    // C `sef_startup`'s birth conversation (`sef.c:125-141`): block on RS
    // until the RS_INIT request arrives, run the fresh hook, and report
    // over the sendrec reply (`sef_init.c:458-466`). The fresh hook here is
    // the type dispatch only (fresh/restart → OK, LU → ENOSYS, the same
    // shape as PM's birth arm) — the socket-device work is loop business.
    let mut sef_ipc = SefAdapter;
    let birth = minix_sef::sef_await_init(
        &mut sef_ipc,
        &mut |req, addr, len| {
            let r = minix_sys::syscall::sys_statectl(
                &minix_sys::syscall::DirectKernelCallTransport,
                req,
                addr,
                len,
            );
            if r < 0 { Err(-r) } else { Ok(r) }
        },
        &mut |t| match t {
            0 | 2 => 0, // SEF_INIT_FRESH / SEF_INIT_RESTART — sef.h:93-95
            _ => minix_types::ENOSYS,
        },
    );
    // C panics when the birth conversation fails ("unable to ipc_receive
    // from RS", sef.c:132) — the same fail-fast here.
    if let Err(e) = birth {
        panic!("uds: unable to complete the SEF birth conversation: {e}");
    }

    let mut ipc = minix_net_uds::server::KernelIpc {
        transport: minix_sys::ipc::DirectTrapTransport,
    };
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
        ProductionHandler {
            running: true,
            sockets_in_use: 0,
        }
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

    fn notify_clock(
        &mut self,
        _table: &mut minix_netdriver::socktable::SockTable,
        _tick: &minix_types::Message,
    ) {
    }

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

/// `minix-sef`'s receive-side adapter over the trap transport (the same
/// shape as fs-rt's `SeIpcAdapter`).
struct SefAdapter;

impl minix_sef::SefIpc for SefAdapter {
    fn receive(
        &mut self,
        src: minix_types::Endpoint,
        msg: &mut minix_types::Message,
    ) -> Result<i32, i32> {
        use minix_sys::ipc::IpcTransport;
        minix_sys::ipc::DirectTrapTransport
            .receive(src, msg)
            .map(|status| status.0 as i32)
            .map_err(|t| t.0)
    }

    fn notify(&mut self, dest: minix_types::Endpoint) -> Result<(), i32> {
        use minix_sys::ipc::IpcTransport;
        minix_sys::ipc::DirectTrapTransport
            .notify(dest)
            .map_err(|t| t.0)
    }

    fn sendrec(
        &mut self,
        dest: minix_types::Endpoint,
        msg: &mut minix_types::Message,
    ) -> Result<i32, i32> {
        use minix_sys::ipc::IpcTransport;
        minix_sys::ipc::DirectTrapTransport
            .sendrec(dest, msg)
            .map(|_| 0)
            .map_err(|t| t.0)
    }
}
