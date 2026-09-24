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
//! - the block source starts fail-closed (`EIO`) when no packaged image is
//!   available (dev/test build); when `xtask image` runs, its build.rs
//!   discovers the generated imgrd and `include_bytes!` fills `BOOT_IMGRD`
//!   with real bytes, switching `BootBlockSource` to the imgrd RAM disk
//!   (`minix_fs_rt::source::ImgrdBlockSource`). (E-IMGPKG landed B11.);
//! - the signal hook ignores notifications for now — the C handler
//!   terminates on `SIGTERM` (`main.c:70-78`), but process signals reach
//!   it through the signal-manager pull whose kernel wrappers are not
//!   wired yet; the decision point stays in one hook, ready for that
//!   channel.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

// freestanding 侧持有 minix-rt 的链接引用：bin 代码本身不经 rt 的任何
// 符号，未引用的 rlib 会被整体丢弃，`_start`/`#[panic_handler]`/
// `#[global_allocator]` 三件随之消失（panic-halt 模式的 `extern crate`
// 保留语义）。出生链 `_start` 在 main 前已自动完成 rt 初始化
//（`lib.rs:86` init 契约：no_std 模式 `_start` 自动调用、幂等），这里
// 无须也不应再调。
#[cfg(all(not(test), target_os = "none"))]
extern crate minix_rt;

// hooks 的 Box::new 裸用：none 侧由 alloc 供给（宿主/测试走 std prelude）。
#[cfg(all(not(test), target_os = "none"))]
extern crate alloc;

#[cfg(all(not(test), target_os = "none"))]
use alloc::boxed::Box;

// In test builds, use the system allocator (the crate is no_std in
// production; the test harness allocates before main() runs).
#[cfg(test)]
#[global_allocator]
static GLOBAL: std::alloc::System = std::alloc::System;

// The packaged boot image, empty until image assembly lands (E-IMGPKG).
// C carries the same bytes as the memory driver's linked-in
// `_binary_imgrd_mfs_*` blob (`drivers/storage/memory/local.h:5-9`); the
// Rust packaging fills this slot instead — `build.rs` emits
// `BOOT_IMGRD_DATA` via `include_bytes!` when a packaged imgrd is present
// at `target/image/<arch>/imgrd.img`, or falls back to `&[]` for dev/test.
include!(concat!(env!("OUT_DIR"), "/imgrd_data.rs"));
#[cfg(not(test))]
static BOOT_IMGRD: &[u8] = BOOT_IMGRD_DATA;

/// The imgrd filesystem block size (`mkfs_mfs` default, `const.h` 4 KiB
/// era). Must match the superblock's declared `s_block_size` or mount
/// fails with `BlockSizeMismatch` (`mount.rs:387`).
#[cfg(not(test))]
const BOOT_BLOCK_SIZE: usize = 4096;

// 入口按目标拆双形：none 侧满足 crt0 Consumer contract（名字+Rust
// ABI+`-> i32`，os/libs/minix-rt/src/crt0.rs:38）；宿主/测试侧保持
// `()`——rustc 1.94 起 Termination 不再为 i32 实现，宿主 i32 main 即
// E0277（docker minix-ci:1.94 实测）。
#[cfg(all(not(test), target_os = "none"))]
#[unsafe(no_mangle)]
extern "Rust" fn main() -> i32 {
    // real_main 原地消费返回 ()；0 即进程退出码（crt0 Consumer contract）。
    real_main();
    0
}

#[cfg(any(test, not(target_os = "none")))]
fn main() {
    real_main()
}

fn real_main() {
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
