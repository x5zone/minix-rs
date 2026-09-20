//! Minix VM Server.
//!
//! Virtual memory manager service process.
//! Entry point for the VM server binary.

// In test builds, use the system allocator instead of VmAllocator.
// VmAllocator requires PAGE_ALLOC_PTR which is only set by VmServer::new(),
// but the test harness allocates memory before main() runs.
#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

// freestanding 侧持有 minix-rt 的链接引用：bin 代码本身不经 rt 的任何
// 符号，未引用的 rlib 会被整体丢弃，`_start`/`#[panic_handler]`/
// `#[global_allocator]` 三件随之消失（panic-halt 模式的 `extern crate`
// 保留语义）。出生链 `_start` 在 main 前已自动完成 rt 初始化
//（`lib.rs:86` init 契约：no_std 模式 `_start` 自动调用、幂等），这里
// 无须也不应再调。
#[cfg(all(not(test), target_os = "none"))]
extern crate minix_rt;

#[cfg(test)]
#[global_allocator]
static GLOBAL: std::alloc::System = std::alloc::System;

// 入口按目标拆双形：none 侧满足 crt0 Consumer contract（名字+Rust
// ABI+`-> i32`，os/libs/minix-rt/src/crt0.rs:40）；宿主/测试侧保持
// `()`——rustc 1.94 起 Termination 不再为 i32 实现，宿主 i32 main 即
// E0277（docker minix-ci:1.94 实测）。
#[cfg(all(not(test), target_os = "none"))]
#[unsafe(no_mangle)]
extern "Rust" fn main() -> i32 {
    real_main()
}

#[cfg(any(test, not(target_os = "none")))]
fn main() {
    real_main()
}

fn real_main() -> ! {
    // In test builds, the global allocator (VmAllocator) requires PAGE_ALLOC_PTR
    // which is only set by VmServer::new(). Since the test harness allocates
    // memory before main() runs, we skip the binary entirely in test mode.
    #[cfg(not(test))]
    {
        use minix_vm::{VmServer, read_boot_params};

        // C: main.c:79-88 is_first_time() — fresh boot gates init_vm().
        // The kernel writes a `VmBootHandoff` page (A1 root identity + A2
        // post-bootstrap classification + boot tables + kernel footprint)
        // and maps it user read-only before scheduling VM. VM consumes it
        // once here: the free list feeds the PMM, `root_paddr` feeds
        // adoption (init_vm_self_pt), and the A2 deduction record is
        // reconciled at this consumer boundary before the allocator is
        // built (07-paging_init_design §6.0).
        let params = read_boot_params();

        let mut server = VmServer::new_with_boot_params(params);

        // C: main.c:101-108 — if(is_first_time()) { init_vm(); __vm_init_fresh=1; }
        //
        // G-V12-10: a warm start (`is_first_time == false`, C hot-restart
        // semantics after RTS_BOOTINHIBIT clears) cannot be served here — C
        // survives it because its state lives in BSS globals that outlive the
        // restart, while minix-rs state lives inside the VmServer instance.
        // Refuse loudly instead of silently panicking at `run()`'s first
        // `assert!(self.initialized)` (02-stage-vm/todo.md G-V12-10).
        if params.is_first_time {
            server.init();
        } else {
            panic!(
                "VM warm restart (is_first_time = false) is not supported: \
                 minix-rs keeps no cross-restart BSS state (C main.c:101-108 \
                 semantics); see 02-stage-vm/todo.md G-V12-10"
            );
        }

        // C: sef_local_startup() — the RS_INIT handshake happens inside the
        // main loop's priority-2 dispatch (rs_handshake), so no separate
        // SEF startup step is needed (see doc 01-vm-init-main §3.4).
        server.run();
    }
    #[cfg(test)]
    unreachable!("bin entry unused in test builds")
}
