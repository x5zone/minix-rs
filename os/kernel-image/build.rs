//! x86_64 裸机内核镜像的链接脚本接线（NS8-A）。
//!
//! 只对 `x86_64-unknown-none` 目标生效：bin 链接时追加 `-T x86_64.ld`，
//! 使 PT_LOAD 带上"高 VMA + 低物理 LMA"的 higher-half 布局（脚本头部
//! 有完整契约说明）。宿主目标不追加任何链接参数——本包在宿主本无
//! 可构建目标（bin 被 fw-x86-none required-features 门隔离，X-2/NK5）。

use std::env;

fn main() {
    let target = env::var("TARGET").unwrap_or_default();
    if target == "x86_64-unknown-none" {
        let manifest = env::var("CARGO_MANIFEST_DIR").unwrap_or_default();
        println!("cargo:rustc-link-arg-bins=-T{manifest}/x86_64.ld");
    }
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=x86_64.ld");
}
