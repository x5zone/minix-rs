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

        // SD-11 U-5 迁移（第九台，首个文件服务器）：MFS 是缓存大头（块缓存与
        // inode 表，src/ 内 Vec/Box 工作集），也是 x86 2048 页档的运行时理由
        // 所在。把全局堆供给从焊死固定池切到 VM_BRK 增长堆。安装点在 server
        // 构造（fresh 体已建表与缓存，仍在固定池）之后、`serve` 主循环之前——
        // 此刻 VM（boot 序第 2 位，早于 MFS 第 11 位）主循环已在收请求，
        // `install_vm_supplier` 内的 VM_BRK 直连有应答；EINVAL fail-fast；与
        // DS(T3)/SCHED(T3b)/MIB/VFS/PM/PFS/TTY/memory 同法，整块
        // #[cfg(target_os="none")] 不影响宿主测试。
        #[cfg(target_os = "none")]
        minix_rt::install_vm_supplier(minix_rt::image_end())
            .expect("mfs: install_vm_supplier(镜像尾) 不得失败");

        // SD-11 真机取证探针：安装后立即从 VM 腿取一页（8 字节 slab 请求触发
        // add_slab → supply_page → VM_BRK），失败即 fail-fast；成功打一行串口
        // 标记作 VM_BRK 增长腿端到端贯通的 L1 证据（与 VFS 同款）。
        #[cfg(target_os = "none")]
        {
            let probe = minix_rt::alloc(8);
            assert!(!probe.is_null(), "mfs: vm-brk probe 分配失败");
            minix_rt::free(probe);
            use minix_sys::syscall::{DirectKernelCallTransport, sys_diagctl_write};
            #[allow(clippy::let_underscore_must_use)] // [SD19-gate] 报告通道自坏时 park 兜底（panic.c:66）
            let _ = sys_diagctl_write(
                &DirectKernelCallTransport,
                "nk4a: mfs-vmbrk probe ok pages=1 via=VM_BRK\n",
            );
        }

        // C registers a signal handler for this server (`mfs/main.c:38`);
        // its body (`main.c:70-78`) ignores everything but SIGTERM and, on
        // SIGTERM, syncs the file system before asking the framework to stop
        // (`fsdriver_terminate`, `fsdriver.c:68-74`). The decision function
        // lives in the library so C's rule is testable outside this binary.
        // Birth: the C fresh-install sequence (vmcache flag, inode table
        // zeroing, buffer pool — main.c:47-65) is carried by the server
        // value's construction, so the callback only confirms readiness.
        let hooks = minix_fs_rt::transport::ServerHooks {
            on_signal: Box::new(minix_fs_mfs::startup::decide_on_signal),
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
