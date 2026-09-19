//! Minix file server entry point.
//!
//! C: `main` + the SEF callbacks — `minix3/minix/fs/mfs/main.c:29-78`
//! (`env_setargs` + `sef_local_startup` + `fsdriver_task`). The common
//! runtime shape (SEF receive loop, RS birth handshake, request decode,
//! replies, grant data plane) lives in `minix-fs-rt`; this entry only
//! contributes the server value and the two hooks.
//!
//! Two deliberate stand-ins, both honest failures rather than pretend
//! successes:
//! - the block source is `PendingBlockSource` — every block touch answers
//!   `EIO` until the block-driver seam (the tracked E-FSBDEV item) wires a
//!   real device channel;
//! - the signal hook ignores notifications for now — the C handler
//!   terminates on `SIGTERM` (`main.c:70-78`), but process signals reach
//!   it through the signal-manager pull whose kernel wrappers are not
//!   wired yet; the decision point stays in one hook, ready for that
//!   channel.

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
        let driver =
            minix_fs_mfs::server::MfsServer::new(minix_fs_rt::source::PendingBlockSource);
        // Birth: the C fresh-install sequence (vmcache flag, inode table
        // zeroing, buffer pool — main.c:47-65) is carried by the server
        // value's construction, so the callback only confirms readiness.
        let hooks = minix_fs_rt::transport::ServerHooks {
            on_signal: Box::new(|_pending| false),
            init: Box::new(|_| Ok(())),
        };
        match minix_fs_rt::serve(driver, hooks) {
            Ok(()) => {}
            // C panics on the startup path (sef_startup); the boot failure
            // here is the birth report RS refused, same fatal outcome.
            Err(e) => panic!("mfs: startup failed: {e}"),
        }
    }
}
