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
//! - the block source starts fail-closed (`EIO`) until the packaging
//!   supplies the boot image (the tracked E-IMGPKG item): `BOOT_IMGRD`
//!   below is the supply point, and `minix_fs_rt::source::BootBlockSource`
//!   switches to the imgrd RAM disk
//!   (`minix_fs_rt::source::ImgrdBlockSource`) the moment it is filled;
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

/// The packaged boot image, empty until image assembly lands (E-IMGPKG).
/// C carries the same bytes as the memory driver's linked-in
/// `_binary_imgrd_mfs_*` blob (`drivers/storage/memory/local.h:5-9`); the
/// Rust packaging fills this slot instead.
#[cfg(not(test))]
static BOOT_IMGRD: &[u8] = &[];

/// Smallest MFS block (`minix3/minix/fs/mfs/const.h`'s block era) — the
/// pre-mount block size the mount path validates against the superblock.
#[cfg(not(test))]
const BOOT_BLOCK_SIZE: usize = 512;

fn main() {
    // In test builds, skip the binary entirely (the test harness drives the
    // library directly).
    #[cfg(not(test))]
    {
        let driver = minix_fs_mfs::server::MfsServer::new(
            minix_fs_rt::source::BootBlockSource::from_boot_image(BOOT_IMGRD, BOOT_BLOCK_SIZE),
        );
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
