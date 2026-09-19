//! Process-information filesystem (`/proc`) binary.
//!
//! C: `main` + the SEF callbacks — `minix3/minix/fs/procfs/main.c`
//! (`env_setargs` + `sef_local_startup` + `fsdriver_task(&procfs_table)`).
//! The common runtime shape (SEF receive loop, RS birth handshake, request
//! decode, replies, grant data plane) lives in `minix-fs-rt`; this entry
//! contributes the server value (the static root tree and read hooks) and
//! the two hooks.
//!
//! Deliberate stand-ins, honest rather than fake: the signal hook ignores
//! notifications (the termination path needs the signal-manager pull
//! channel, not wired yet); the per-process directories are the tracked
//! framework gap (see `server`'s module notes), so the mounted tree
//! carries only the static root files; and the production kernel-query
//! source reports every input as absent until E-KERNINFO lands, so reads
//! fail with `EIO` instead of rendering invented bytes.

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
        let server = minix_fs_procfs::init(minix_fs_procfs::server::PendingProcSource);
        let hooks = minix_fs_rt::transport::ServerHooks {
            on_signal: Box::new(|_pending| false),
            init: Box::new(|_| Ok(())),
        };
        match minix_fs_rt::serve(server, hooks) {
            Ok(()) => {}
            Err(e) => panic!("procfs: startup failed: {e}"),
        }
    }
}
