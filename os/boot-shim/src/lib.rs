//! Boot shim — bridges firmware (UEFI/OpenSBI/...) to the bare-metal kernel.
//!
//! This crate implements the `BootShim` trait (defined in `minix-types`)
//! for different firmware types. The kernel only depends on the trait,
//! not on any concrete implementation.
//!
//! Feature gates in `Cargo.toml` select which implementation gets compiled;
//! code uses the trait uniformly without `#[cfg]` conditionals.
//!
//! Available implementations:
//! - `uefi` feature (default): `UefiBootShim` — uses `uefi` crate
//! - `opensbi` feature: `OpenSbiBootShim` — hardcoded QEMU virt memory map

#![cfg_attr(not(test), no_std)]
#![cfg_attr(test, allow(internal_features))]

extern crate alloc;

#[cfg(test)]
extern crate std;

// Provide a global allocator for the test binary.
// The kernel and its dependencies use `alloc` but are `#![no_std]`;
// without this, the test linker would have no allocator for them.
#[cfg(test)]
#[global_allocator]
static TEST_ALLOCATOR: std::alloc::System = std::alloc::System;

// Re-export the trait and result type from minix-boot.
pub use minix_boot::{BootShim, BootPrepareResult};

// Re-export the ELF parser from the shared minix-elf crate.
// Both boot-shim and kernel need ELF parsing, so it lives in os/libs/minix-elf.
pub use minix_elf;

// ── Shared, firmware-agnostic loading logic ──
//
// The kernel/module loading pipeline is identical for every firmware
// (parse ELF → copy segments → place modules → build KernelInfo). The
// only thing that differs is how raw file bytes are obtained, captured by
// the `FileLoader` trait. Both `uefi_helpers` and `opensbi_helpers`
// implement this trait and reuse the same shared loader functions.
pub mod loader;

// ── Firmware-specific modules ──
// Each module provides a struct that implements BootShim.

#[cfg(feature = "uefi")]
pub mod uefi_helpers;

#[cfg(feature = "opensbi")]
pub mod opensbi_helpers;

// ── Convenience re-export of the active implementation ──
// This allows callers to write `boot_shim::prepare_boot(...)` without
// knowing which firmware is active. The Cargo.toml feature selection
// determines which impl is compiled.

#[cfg(feature = "uefi")]
pub use uefi_helpers::{find_platform_sources, UefiBootShim};

#[cfg(all(feature = "opensbi", not(feature = "uefi")))]
pub use opensbi_helpers::OpenSbiBootShim;

/// 裸写 QEMU 串口（COM1 数据口 0x3F8；OVMF 已完成 16550 初始化）。
///
/// 用途：EBS 之后 uefi::println! 的日志路径不再可靠（见
/// uefi_helpers::exit_boot_services 的注释），裸端口写不依赖任何
/// UEFI 状态——NK4-A 首亮诊断的 post-EBS 专用通道。
#[cfg(target_arch = "x86_64")]
pub fn raw_serial(text: &str) {
    for byte in text.bytes() {
        // SAFETY: COM1 数据口是 QEMU/PC 平台的固定 I/O 资源；boot-shim
        // 是唯一所有者（固件交棒后无并发写者），out 指令无副作用泄漏。
        unsafe {
            core::arch::asm!("out dx, al", in("dx") 0x3f8u16, in("al") byte, options(nomem, nostack));
        }
    }
}

/// 非 x86_64 目标：无 COM1 语义，退化为空实现（诊断打印缺失可接受——
/// 该诊断只服务于 x86_64 载体）。
#[cfg(not(target_arch = "x86_64"))]
pub fn raw_serial(_text: &str) {}
