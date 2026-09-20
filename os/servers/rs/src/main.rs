//! Minix-RS Reincarnation Server — entry point.
//!
//! C: `main()` — `minix3/minix/servers/rs/main.c:38-131`.
//! See `notes/rewrite/fork-syscall-rewrite/03-stage-rs/01-rs-boot-init.md`.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

// In test builds, use the system allocator (the crate is no_std in
// production; the test harness allocates before main() runs).
#[cfg(test)]
#[global_allocator]
static GLOBAL: std::alloc::System = std::alloc::System;


// freestanding 侧持有 minix-rt 的链接引用：bin 代码本身不经 rt 的任何
// 符号，未引用的 rlib 会被整体丢弃，`_start`/`#[panic_handler]`/
// `#[global_allocator]` 三件随之消失（panic-halt 模式的 `extern crate`
// 保留语义）。出生链 `_start` 在 main 前已自动完成 rt 初始化
//（`lib.rs:86` init 契约：no_std 模式 `_start` 自动调用、幂等），这里
// 无须也不应再调。
#[cfg(all(not(test), target_os = "none"))]
extern crate minix_rt;

// 入口按目标拆双形：none 侧满足 crt0 Consumer contract（名字+Rust
// ABI+`-> i32`，os/libs/minix-rt/src/crt0.rs:40）；宿主/测试侧保持
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
        use minix_rs::{RsServer, SefInitType, boot::BootTables};

        // C: sef_local_startup() — main.c:51. The SEF callback set is the
        // `SefCallbacks` trait implemented by `RsServer` itself (N5); there
        // is no separate registration value to construct.

        // C: sys_getimage(image) — main.c:196. The kernel's GET_IMAGE
        // answer is the boot truth (do_getinfo.c:86-90); a failure stops
        // the boot (C panic, main.c:196-198) — half-true tables must not
        // reach the boot invariants below (ARCH A-13 resolved: the tables
        // now come from the kernel, not the placeholder).
        let tables = BootTables::acquire_from(&minix_sys::syscall::DirectKernelCallTransport)
            .unwrap_or_else(|e| panic!("RS boot: sys_getimage failed: errno {e}"));

        // C main.c:171 — env_parse("rs_verbose", "d", 0, &rs_verbose, 0, 1)
        // over the monitor parameters (GET_MONPARAMS → S42 批一真值链)。
        // verbose 带来的 printf 增量在 C 遍布 rs_verbose 门控点；Rust 侧
        // 的消费面随 19/E-11 接线补齐，此处先取真值并经诊断缝确认解析。
        let mut monparams = [0u8; 1024]; // MULTIBOOT_PARAM_BUF_SIZE — param.h:28
        let monparams_ok =
            minix_sys::syscall::sys_getmonparams(&minix_sys::syscall::DirectKernelCallTransport, &mut monparams)
                .is_ok();
        let rs_verbose = monparams_ok && minix_rs::boot::parse_rs_verbose(&monparams);
        if rs_verbose {
            use minix_sys::syscall::{sys_diagctl, DirectKernelCallTransport};
            let line = b"RS: running in verbose mode\n";
            let _ = sys_diagctl(&DirectKernelCallTransport, 1, line.as_ptr() as u64, line.len() as i32);
        }

        let mut server = RsServer::new(tables);

        // C: sef_startup() → sef_cb_init_fresh() — main.c:151,158-494.
        // KernelApi 生产实现 = TrapKernelApi（trap 直连内核，S42 批一
        // 接线；构造序 RsServer::new 默认装配）。C treats boot failure
        // as fatal (main.c:226 `panic(...)`); do not enter the main loop on
        // an incomplete boot — an RS that never finished booting cannot
        // manage services. The SEF face carries a bare errno; the typed
        // cause (E-6) distinguishes "kernel face not wired" (`ENOSYS`) from
        // "boot invariant violation" (`EINVAL`, e.g. a corrupt boot table).
        if let Err(e) = server.init(SefInitType::Fresh) {
            panic!(
                "RS boot failed: {e:?} (boot diagnostic: {:?})",
                server.boot_diagnostic()
            );
        }

        // C: main loop — main.c:50-131 (skeleton; details in 06). C 的
        // while(TRUE) 永不返回；循环退出 = 传输/boot 面故障，属致命——
        // 与 boot 失败同款 panic（X-6：此前 `let _ =` 把 Err 静默吞掉，
        // 进程以 0 退出假成功）。
        if let Err(e) = server.run() {
            panic!("RS main loop terminated: {e:?}");
        }
    }
}
