//! Minix-RS VFS Entry Point

// 按目标分形态（init 先例，`os/commands/sbin/init/src/main.rs:27-35`）：
// 宿主与测试构建保持 std 形态；`target_os = "none"` 侧是 freestanding
// 真身（boot 装机模块）。
#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

// freestanding 侧持有 minix-rt 的链接引用：bin 代码不经 rt 的任何符号，
// 未引用的 rlib 会被整体丢弃，`_start`/`#[panic_handler]`/
// `#[global_allocator]` 三件随之消失（panic-halt 模式）。出生链 `_start`
// 已在 main 前自动完成 rt 初始化（lib.rs:86 契约，幂等），此处不再调。
#[cfg(all(not(test), target_os = "none"))]
extern crate minix_rt;

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
    use minix_vfs::main_loop;

    // run() 发散（main_loop.rs:7310 `-> !`），尾表达式即 real_main
    // 的函数尾。
    main_loop::run()
}
