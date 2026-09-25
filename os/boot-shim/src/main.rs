//! UEFI boot shim — bridges UEFI firmware to the bare-metal kernel.
//!
//! This is the UEFI binary entry point. It calls `UefiBootShim::prepare_boot()`
//! (which implements the `BootShim` trait from `minix-types`) to collect
//! firmware information, then hands control to the kernel's `arch_boot`.
//!
//! After ExitBootServices, UEFI runtime services may still be available
//! (via GetVariable/SetVariable), but boot services (AllocatePages, etc.)
//! are gone. The kernel itself is UEFI-free — it only receives KernelInfo.
//!
//! Corresponding Minix3 C:
//! - pre_init() — pre_init.c:217 (multiboot → kinfo)
//! - get_parameters() — pre_init.c:94 (parse GRUB data)
//! - pg_alloc_page() — pg_utils.c:138 (allocate root page table)

#![no_std]
#![no_main]

use core::panic::PanicInfo;
use uefi::prelude::*;
use minix_kernel::boot_alloc;
use minix_boot::BootShim;
use boot_shim::{raw_serial_line, UefiBootShim};

extern crate alloc;

// Global allocator for the UEFI boot-shim binary.
// We don't use uefi's "global_allocator" Cargo feature because it would
// conflict with the test-only allocator in lib.rs. Instead, we set it up
// manually here — this is exactly what the feature does internally.
#[global_allocator]
static ALLOCATOR: uefi::allocator::Allocator = uefi::allocator::Allocator;

#[entry]
fn main() -> Status {
    // 1. UEFI boot preparation via BootShim trait
    //    Internally: GetMemoryMap → AllocatePages → load kernel ELF from ESP
    //    → load boot modules → build KernelInfo → ExitBootServices
    // 1024 pages (4 MiB): the identity mapping, the kernel high mapping,
    // DM coverage establishment (kernel + VM windows), the NK4-A
    // boot-identity eviction (ELF segment pages + the 64 MiB VM heap
    // window at 4 KiB granularity ≈ one PT page per 2 MiB slice ≈ 33
    // pages) and the handoff page's fresh chain all draw lower-level
    // page-table pages from this bump pool via boot_pt_alloc. A
    // fragmentary firmware memmap forces 4 KiB-granularity DM leaves —
    // one PT page per 2 MiB region per window — which the former 16/64
    // pages cannot cover (the kernel's fallback bump is 1 MiB = 256
    // pages, FALLBACK_BUMP_LEN — this pool stays in the same order).
    // NK4-C 1.55 B30: 128 pages sufficed for boot alone, but B26 fix-B
    // also draws the kerninfo-injection chain (1–3 pages) from this
    // pool at EVERY new-root bind, and the pool has no free path — the
    // real machine died `AllocationFailed` after ~40 binds (bn9m).
    // 1024 = boot needs (~100) + per-bind chain (≤3) × bind ceiling
    // (exec 重建根每子进程一次;数百量级) × safety; runway is not
    // free though: vm_handoff deducts the WHOLE region from the VM free
    // list whether used or not (vm_handoff.rs 模块 doc), so this costs
    // every boot 4 MiB of VM-selectable memory. Before the structural
    // fix lands (reclaimable page-table pages / moving the kerninfo
    // mapping responsibility to VM per kerninfo.rs doc — WORKLOG 1.55
    // follow-up) this pool only postpones the deterministic panic.
    let result = UefiBootShim::prepare_boot(1024);

    // --- Below this point, UEFI boot services are gone ---

    // 2. Register boot-stage page table allocator
    boot_alloc::init_boot_pt_alloc(result.bump_base, result.bump_end);
    minix_arch::pt_alloc::register(boot_alloc::boot_pt_alloc);
    boot_shim::raw_serial("boot-shim: [raw] pt_alloc registered, entering kernel arch_boot\r\n");

    // 3. Hand control to the kernel's arch-specific boot.
    //    Establishes page tables, enables paging, enters kmain.
    //    NEVER RETURNS.
    //
    //    NK4-A 首亮诊断：EBS 后、arch_boot 前的最后一块 shim 侧路标。
    //    arch_boot 若先于内核第一条串口输出就卡死，本行是最后一根
    //    可见路标（ConOut 已实证落在 QEMU 串口）。
    uefi::println!("boot-shim: handing control to kernel arch_boot");
    minix_kernel::arch_boot(&result.kernel_info, result.root_page);

    // This line should never be reached.
    unreachable!()
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    // NK4-A 首亮诊断：本 bin 的原实现是 `loop {}`——静默挂死。内核代码
    // 进程内链在本 shim 里（main() 直调 arch_boot/kmain），所以 Phase A
    // 的任何 panic 都走这里，串口上什么都看不到，"真 hang"与"panic 被
    // 吞"无法区分。改为：①栈缓冲格式化 panic 位置+消息（无堆依赖，EBS
    // 前后都可用）经 raw_serial 打出；②触发内核诊断钩（arch_boot 入口
    // 已注册，带 CPU 号 + backtrace）；③驻留。
    struct StackWriter<'a> {
        buf: &'a mut [u8; 256],
        len: usize,
    }
    impl core::fmt::Write for StackWriter<'_> {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            for &b in s.as_bytes() {
                if self.len < self.buf.len() {
                    self.buf[self.len] = b;
                    self.len += 1;
                } // 超出 256B 的尾部丢弃即可：诊断信息截断好于静默。
            }
            Ok(())
        }
    }
    let mut buf = [0u8; 256];
    let mut writer = StackWriter { buf: &mut buf, len: 0 };
    // PanicInfo 的 Display 输出 "panicked at file:line:\nmessage"。
    let _ = core::fmt::Write::write_fmt(&mut writer, format_args!("{}", info));
    // 先取出长度结束对 buf 的可变借用，再按字节切读。
    let written = writer.len;
    let message = core::str::from_utf8(&buf[..written])
        .unwrap_or("boot-shim panic: (non-utf8 message)");
    raw_serial_line("boot-shim panic: ");
    raw_serial_line(message);
    // 钩子与 panic 路径同址空间：arch_boot 入口注册过则补打内核格式。
    if !minix_types::run_panic_diagnostic_hook(message) {
        raw_serial_line("boot-shim panic: diagnostic hook not registered");
    }
    loop {
        core::hint::spin_loop();
    }
}
