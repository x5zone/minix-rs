//! The ACPI service binary.
//!
//! C: `main` runs the SEF startup (`acpi.c:109-136`: the gates, the
//! memory privilege, the interpreter's initialization chain, the machine
//! mode report, and the PCI bridge scan), then the request loop
//! (`acpi.c:139-167`). The loop shell and the RS birth handshake live in
//! the shared driver runtime; this binary builds the service.
//!
//! C announces no character key: ACPI is an RS service with its own two
//! requests (`ACPI_REQ_GET_IRQ`, `ACPI_REQ_MAP_BRIDGE`), so the loop is
//! entered through the announce-free serve path.
//!
//! The interpreter is the registered seam (`E-ACPICA`): with the unbound
//! interpreter in place the service reports "ACPI failed" (C's
//! `ACPI: ACPI failed with err %d`, `acpi.c:100-103`) instead of claiming
//! to have parsed tables, and then answers the two requests with "no
//! route" and "no device" respectively.
#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

// freestanding 侧持有 minix-rt 的链接引用：bin 代码本身不经 rt 的任何
// 符号，未引用的 rlib 会被整体丢弃，`_start`/`#[panic_handler]`/
// `#[global_allocator]` 三件随之消失（panic-halt 模式的 `extern crate`
// 保留语义）。出生链 `_start` 在 main 前已自动完成 rt 初始化
//（`lib.rs` init 契约：no_std 模式 `_start` 自动调用、幂等），这里无须
// 也不应再调。
#[cfg(all(not(test), target_os = "none"))]
extern crate minix_rt;

use minix_driver_rt::kernel::KernelTransport;
use minix_driver_rt::runtime::DriverRuntime;

/// The machine record's wire layout: `processors_count` at zero,
/// `bsp_id` at four, padding at eight, `apic_enabled` at twelve,
/// `acpi_rsdp` at sixteen, `board_id` at twenty-four
/// (`struct machine`, the `GET_MACHINE` record the kernel fills).
fn read_machine() -> Option<minix_driver_acpi::service::Machine> {
    let mut buffer = [0u8; 32];
    minix_sys::syscall::sys_get_machine(
        &minix_sys::syscall::DirectKernelCallTransport,
        &mut buffer,
    )
    .ok()?;
    let apic_enabled = i32::from_le_bytes(buffer[12..16].try_into().unwrap()) != 0;
    let acpi_rsdp = u64::from_le_bytes(buffer[16..24].try_into().unwrap());
    Some(minix_driver_acpi::service::Machine {
        apic_enabled,
        acpi_rsdp,
    })
}

/// Adds one memory range to this process's privileges
/// (`sys_privctl(SELF, SYS_PRIV_ADD_MEM, &mr)`, `acpi.c:19`).
///
/// The request number is `SYS_PRIV_ADD_MEM` (`com.h:347`); the argument
/// is a `struct minix_mem_range` — two `phys_bytes` fields — in this
/// process's own memory.
fn add_memory_privilege(range: minix_driver_acpi::service::MemoryRange) -> bool {
    /// `SYS_PRIV_ADD_MEM` (`com.h:347`).
    const SYS_PRIV_ADD_MEM: i32 = 6;

    /// The C argument structure (`struct minix_mem_range`: two
    /// `phys_bytes` fields).
    #[repr(C)]
    struct MinixMemRange {
        mr_base: u64,
        mr_limit: u64,
    }

    let range = MinixMemRange {
        mr_base: range.base,
        mr_limit: range.limit,
    };
    minix_sys::syscall::sys_privctl(
        &minix_sys::syscall::DirectKernelCallTransport,
        minix_types::Endpoint::SELF.0,
        SYS_PRIV_ADD_MEM,
        &range as *const MinixMemRange as u64,
    )
    .is_ok()
}

// 入口按目标拆双形：none 侧满足 crt0 Consumer contract（名字 + Rust
// ABI + `-> i32`，`os/libs/minix-rt/src/crt0.rs`）；宿主与测试侧保持
// `()`——rustc 1.94 起 Termination 不再为 i32 实现。
#[cfg(all(not(test), target_os = "none"))]
#[unsafe(no_mangle)]
extern "Rust" fn main() -> i32 {
    real_main();
    0
}

#[cfg(any(test, not(target_os = "none")))]
fn main() {
    real_main()
}

fn real_main() {
    // C `sef_startup` asks the kernel who we are (`sef.c:76-87`).
    let who = minix_sys::syscall::sys_whoami(&minix_sys::syscall::DirectKernelCallTransport)
        .unwrap_or_else(|r| panic!("acpi: sys_whoami failed: {r}"));
    let transport = KernelTransport::new(who.endpoint);
    let mut service = minix_driver_acpi::init();

    // C `sys_getmachine` supplies the two gate facts (`acpi.c:84-88`);
    // without them ACPI stays off.
    let machine = match read_machine() {
        Some(machine) => machine,
        None => return,
    };
    // C asks the kernel for memory access before touching any table
    // (`acpi.c:80-84`).
    service.init(machine, add_memory_privilege);

    let mut runtime = DriverRuntime::new(transport, "acpi");
    // `serve_quiet`: birth handshake then the loop, without publishing a
    // label C never publishes. The loop returns only when the transport
    // dies; C turns the same receive failure into a print-and-continue.
    let _birth = runtime.serve_quiet(&mut service, |_| Ok(()));
}
