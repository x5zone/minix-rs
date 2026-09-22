//! 裸机内核镜像的链接脚本接线（NS8-A / NK4-B P3 M3.2）。
//!
//! 按 `TARGET` 选脚本：`x86_64-unknown-none` 用 `x86_64.ld`，
//! `aarch64-unknown-none` 用 `aarch64.ld`。两者同形（高 VMA + 低物理
//! LMA、段间 `vaddr - paddr` 一致），只差基址常量与入口指令序列，
//! 契约写在各自脚本头部。宿主目标不追加任何链接参数——本包在宿主本无
//! 可构建目标（bin 被 fw-none-image required-features 门隔离，X-2/NK5）。

use std::env;

fn main() {
    let target = env::var("TARGET").unwrap_or_default();
    let manifest = env::var("CARGO_MANIFEST_DIR").unwrap_or_default();
    // 一个 bin、多架构：靠 --target 选链接脚本（riscv64 在 P4 M4.2 同形扩展）。
    let script = match target.as_str() {
        "x86_64-unknown-none" => Some("x86_64.ld"),
        "aarch64-unknown-none" => Some("aarch64.ld"),
        _ => None,
    };
    if let Some(script) = script {
        println!("cargo:rustc-link-arg-bins=-T{manifest}/{script}");
    }
    println!("cargo:rerun-if-changed=build.rs");
    // 两份脚本都无条件声明：改任一份都要重跑本 build.rs（选脚本的逻辑本身
    // 也在它们的影响范围内）。
    println!("cargo:rerun-if-changed=x86_64.ld");
    println!("cargo:rerun-if-changed=aarch64.ld");
}
