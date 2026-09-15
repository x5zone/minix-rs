//! Minix-RS input server — entry point.
//!
//! C: `main()` — `minix3/minix/servers/input/input.c:696-704`.
//! See `notes/rewrite/fork-syscall-rewrite/12-stage-input/01-input-init-main.md`.
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
        // Spinning is intentional until the transport lands: a server that
        // cannot receive yet must not pretend otherwise.
        #[allow(clippy::empty_loop)]
        loop {}
    }
}
