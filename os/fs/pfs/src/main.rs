//! Pipe file server entry point.
//!
//! C: `main` — `minix3/minix/fs/pfs/pfs.c:422-451` (`env_setargs` +
//! `sef_local_startup` + `fsdriver_task(&pfs_table)`). The common runtime
//! shape lives in `minix-fs-rt`; this entry contributes the server value
//! (in-memory table, no block device — PFS never touches a disk) and the
//! two hooks.
//!
//! Deliberate stand-in, honest rather than fake: the signal hook ignores
//! notifications for now. The C handler's termination path needs the
//! signal-manager pull channel whose kernel wrappers are not wired yet;
//! the decision point stays in one hook. PFS has no block-device face at
//! all, so the E-FSBDEV stand-in does not apply here.

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
        let server = minix_fs_pfs::init();
        // Birth: the C fresh-install sequence builds the inode table inside
        // the server value (pfs.c init half); the callback confirms ready.
        let hooks = minix_fs_rt::transport::ServerHooks {
            on_signal: Box::new(|_pending| false),
            init: Box::new(|_| Ok(())),
        };
        match minix_fs_rt::serve(server, hooks) {
            Ok(()) => {}
            Err(e) => panic!("pfs: startup failed: {e}"),
        }
    }
}
