//! Minix-RS input server — entry point.
//!
//! C: `main()` — `minix3/minix/servers/input/input.c:696-704`.
//! See `rewrite-notes/12-stage-input/01-input-init-main.md`.
//!
//! The binary does three things, in order: register the fresh-boot callback
//! ([`minix_input::init::startup_registration`]), run the four-step init
//! plan ([`minix_input::init::init_plan`]), then enter the framework main
//! loop and never return. The loop's *decisions* are all in place
//! ([`minix_input::dispatcher`]: one arrival in, state changes and effects
//! out); the loop itself lands with the message transport, which performs
//! those effects (edge todo E-INWIRE). Until then the binary waits rather
//! than pretend to serve.

// In test builds, use the system allocator (the crate is no_std in
// production; the test harness allocates before main() runs).
#[cfg(test)]
#[global_allocator]
static GLOBAL: std::alloc::System = std::alloc::System;

fn main() {
    // In test builds, skip the binary entirely (the test harness drives the
    // library directly).
    #[cfg(not(test))]
    {
        // C: input_startup() then chardriver_task(&input_tab) — input.c:699-701.
        // The registration names the fresh-boot callback (init.rs), the plan
        // lists the four init steps (init.rs), and the loop classifies,
        // gates, and replies through the framework (framework.rs), with the
        // per-arrival decisions in dispatcher.rs.
        //
        // E-INWIRE: the transport is landed (minix-sys direct verbs); the
        // loop classifies, dispatches, and performs through serve.rs. The
        // SEF switch point (receive → sef_receive_status) is documented at
        // KernelTransport::receive.
        //
        // 出生自证:C sef_startup 先 sys_whoami 填 sef_self_endpoint
        //(sef.c:76-87,失败即 panic :81)。问询走无源
        // DirectKernelCallTransport(应答按调用者进程上下文取内核槽,
        // 与 m_source 无关;tty NL6-A 同款);旧注"A-6/RS assignment
        // pending"失实——端点由内核引导过程授予,不经 RS。
        let self_ep =
            minix_sys::syscall::sys_whoami(&minix_sys::syscall::DirectKernelCallTransport)
                .unwrap_or_else(|r| panic!("input: sys_whoami failed: {r}"))
                .endpoint;
        let mut transport = minix_input::serve::KernelTransport::default();
        let mut server = minix_input::dispatcher::Server::fresh();
        minix_input::serve::serve(&mut transport, self_ep, &mut server);
    }
}
