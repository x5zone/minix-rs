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
///
/// x86 语义保留原样：逐字节直写、不等发送侧腾空（需要等待的路径见
/// [`raw_serial_line`]）。
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

/// aarch64：EBS 后只能直接摸 PL011。寄存器布局与发送节流都住在
/// `minix-plat` 的 early_console 里（一处真相），本函数不重写一遍。
/// 本架构上「等腾空」与「不等」没有区别（两条通道共用同一个带节流的
/// 发射器），所以这里直接逐字节走 [`emit_byte`]。
#[cfg(target_arch = "aarch64")]
pub fn raw_serial(text: &str) {
    for byte in text.bytes() {
        emit_byte(byte);
    }
}

/// 其余目标：无已证实的裸机串口通道，退化为空实现。
#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
pub fn raw_serial(_text: &str) {}

/// 逐字节裸写串口（带发送侧有界等待 + `\n` 自动补 `\r`）。
///
/// 与 [`raw_serial`] 的区别：本函数不要求调用方自带 `\r\n`、且等传输
/// 就绪（原实现快速连写会撞满 16550 发送缓冲丢字），并且只吃 `&str` 的
/// 字节流——panic 路径上无分配、无 UTF-8 重切分风险。panic handler 用它。
///
/// 住在库里而不是 bin 里（NK4-B P3 M3.3）：架构分道只需要一个落点，
/// 否则 `raw_serial` 与 `raw_serial_line` 各自要写一遍 cfg。x86 腿的
/// 指令与轮询上限逐字未变。
pub fn raw_serial_line(text: &str) {
    for byte in text.bytes() {
        let mut emit = [byte, 0];
        let n = if byte == b'\n' {
            emit[0] = b'\r';
            emit[1] = b'\n';
            2
        } else {
            1
        };
        for &b in &emit[..n] {
            emit_byte(b);
        }
    }
}

/// [`raw_serial_line`] 的单字节发射器：先等发送侧腾空（有界）再写。
#[cfg(target_arch = "x86_64")]
fn emit_byte(b: u8) {
    // SAFETY: COM1（0x3F8 基址）是 QEMU/PC 平台固定 I/O 资源；
    // boot-shim 交棒后是唯一所有者。LSR（0x3FD = 基址+5）bit5＝THRE
    //（发送保持寄存器空）：置位才写 THR，保证每个字节都进得了发送
    // 寄存器。
    // NK4-A 取证：轮询有界（~10 万次即放弃，字节照写）——LSR
    // 异常时丢个别字节，绝不把 panic 消息整个吞掉。
    // SD-5 / P-X86-02 勘误（2026-10-06）：旧实现读的是 0x3F9＝IER
    //（中断使能寄存器，仅 bit0-3 有效，bit5 恒 0）——每字节空转满
    // 上限再照写。端口勘误后 THRE 真正被等到（早期串口提速数量级）。
    unsafe {
        let mut spins: u32 = 0;
        loop {
            let lsr: u8;
            core::arch::asm!("in al, dx", out("al") lsr, in("dx") 0x3fdu16, options(nomem, nostack));
            if lsr & 0x20 != 0 || spins >= 100_000 {
                break;
            }
            spins += 1;
        }
        core::arch::asm!("out dx, al", in("dx") 0x3f8u16, in("al") b, options(nomem, nostack));
    }
}

/// aarch64：发射器 = `minix-plat` 的早期控制台写入（自带「有界等
/// TXFF」，上限与位定义都在那里，本文件不重复一份寄存器语义）。
#[cfg(target_arch = "aarch64")]
fn emit_byte(b: u8) {
    use minix_plat::EarlyConsole;
    <minix_plat::CurrentEarlyConsole as EarlyConsole>::write_byte(b);
}

/// 其余目标：空实现（与 [`raw_serial`] 同调）。
#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
fn emit_byte(_b: u8) {}
