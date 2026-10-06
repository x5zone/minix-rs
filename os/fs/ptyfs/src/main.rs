//! Unix98 pseudoterminal slave-node filesystem (`/dev/pts`) binary.
//!
//! C: `main` + the SEF callbacks — `minix3/minix/fs/ptyfs/ptyfs.c:422-434`
//! (`env_setargs` + `sef_local_startup` + `fsdriver_task(&ptyfs_table)`).
//! The common runtime shape (SEF receive loop, RS birth handshake, request
//! decode, replies, grant data plane) lives in `minix-fs-rt`; this entry
//! only contributes the server value and the two hooks.
//!
//! Deliberate stand-in, honest rather than fake: the signal hook ignores
//! notifications for now. The C handler's termination path
//! (`ptyfs_signal`, ptyfs.c:417-420) needs the signal-manager pull channel
//! whose kernel wrappers are not wired yet; the decision point stays in
//! one hook. The PTY service's control messages (slave-node create and
//! delete, `ptyfs_other`) need the runtime to pass non-request envelopes
//! through — tracked with the driver's control-face gap.

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
        let server = minix_fs_ptyfs::init();
        // Birth: the C fresh-install callback (`ptyfs_init`,
        // ptyfs.c:422-424) only confirms readiness; the driver value
        // already carries the empty slave table and the root record.
        let hooks = minix_fs_rt::transport::ServerHooks {
            // C `ptyfs_signal` (`ptyfs.c:392-397`, registered `ptyfs.c:407`):
            // SIGTERM terminates without a sync, anything else is ignored.
            on_signal: Box::new(minix_fs_ptyfs::signal_action),
            init: Box::new(|_| Ok(())),
        };
        match minix_fs_rt::serve(server, hooks) {
            Ok(()) => {}
            // C panics on the startup path (sef_startup); the boot failure
            // here is the birth report RS refused, same fatal outcome.
            Err(e) => panic!("ptyfs: startup failed: {e}"),
        }
    }
}
