//! 生产内核镜像 bin（NS8-A / NK4-B P3 M3.2 多架构）：把 minix-kernel 链接成
//! boot-shim 契约位的内核 ELF。boot-shim 从
//! `/EFI/minix/kernel.elf`（`os/boot-shim/src/loader.rs`
//! `KERNEL_PATH`）读它做两件事：`compute_kernel_layout` 取布局填
//! `KernelInfo`（kern_virt_base / kern_phys_base / kern_size），
//! `load_segments_into_phys_memory` 按 PT_LOAD 的物理地址拷段体、清 BSS。
//! 布局契约由同目录的架构脚本定（高 VMA + 低物理 LMA，段间偏移一致）：
//! x86_64 用 `x86_64.ld`，aarch64 用 `aarch64.ld`，由 `build.rs` 按
//! `--target` 选一份。Rust 面架构无关，只有入口指令序列与早期控制台
//! 按 `target_arch` 分道。
//!
//! ## 交付边界（诚实登记）
//!
//! `_start` 目前立栈后驻留（halt）——boot-shim → 内核镜像的跳转交接
//! 协议（KernelInfo 怎么递、入口态怎么约定）是 NK1（C-29，OQ-N6 设计
//! 轮）的裁决范围，这里不代决。当前 UEFI 产线仍是 boot-shim 内联内核、
//! 进程内调 `arch_boot`（`os/boot-shim/src/main.rs:56`）；本镜像使其
//! 契约位从"无产出者"变"按布局契约产出"，NK4-A 装机链即可走通。
//!
//! ## 链接保留（防裁剪）
//!
//! 镜像的存在意义是"含内核本体"。`KERNEL_ENTRY_ANCHOR` 持有
//! `minix_kernel::arch_boot` 的 fn 指针：链接脚本 `KEEP` 该节 +
//! `rust_image_main` 运行路径触达，双保险防止 `--gc-sections`/
//! 归档成员粒度裁剪把整个内核启动图裁出镜像。
//!
//! C 对位：Minix3 C 内核镜像由 kernel Makefile + 链接脚本产出
//! （`minix3/minix/kernel/`），boot 模块（bootx64.efi）装载它——
//! 本 bin 是同一形态的 Rust 侧重表达（Rewrite not Translate）。

#![no_std]
#![no_main]

extern crate alloc;

use core::arch::asm;
use core::hint::black_box;
use core::panic::PanicInfo;
use core::sync::atomic::{AtomicUsize, Ordering};

use alloc::alloc::{GlobalAlloc, Layout};

use minix_boot::KernelInfo;
use minix_types::PhysBytes;

// 早期控制台按架构取板级实现（两者同名 `write_str`，因此 Rust 面无需
// 分道）：x86_64 = COM1 串口，aarch64 = PL011。
#[cfg(target_arch = "x86_64")]
use minix_plat::x86_64::early_console;
#[cfg(target_arch = "aarch64")]
use minix_plat::arm64::early_console;

// ── 内核启动图锚点 ──────────────────────────────────────────────
//
// 类型即契约：`arch_boot(&KernelInfo, PhysBytes) -> !` 是 boot-shim
// 进程内产线（`os/boot-shim/src/main.rs:56`）与未来跳转产线共用的
// 内核启动面。锚点被链接脚本 `KEEP(*(.rodata.kernel_anchor))` 钉住。

#[used]
#[unsafe(link_section = ".rodata.kernel_anchor")]
static KERNEL_ENTRY_ANCHOR: fn(&KernelInfo, PhysBytes) -> ! = minix_kernel::arch_boot;

// ── 入口（global_asm：先立栈，再进 Rust 面）──────────────────────
//
// 栈区由链接脚本在 .bss 里预留（kernel_boot_stack_bottom/top，
// 64 KiB）——与 Linux 内核 init_stack 同形。`.text.boot` 段置于
// 镜像首个可执行页（链接脚本 KEEP + 首位）。
//
// `kernel_boot_stack_top` 是链接脚本符号：在此处之外它无 Rust 声明，
// 交给链接器解析（符号不存在时链接期即失败，符合 fail-fast）。
//
// 两个架构各一份指令序列，语义逐条对应：取栈顶 → 清帧指针 → 调 Rust
// 面 → 停机驻留。aarch64 的 `adrp` + `add :lo12:` 取符号写法抄仓内既有
// 载体（`os/qemu-tests/test-kernels/kernel/bootstrap/test-rt-birth-aarch64/src/main.rs:250-251`）。

#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(
    r#"
    .section .text.boot, "ax"
    .globl _start
    _start:
        lea rsp, [rip + kernel_boot_stack_top]
        xor ebp, ebp
        call {rust_image_main}
    2:  cli
        hlt
        jmp 2b
    "#,
    rust_image_main = sym rust_image_main,
);

#[cfg(target_arch = "aarch64")]
core::arch::global_asm!(
    r#"
    .section .text.boot, "ax"
    .globl _start
    _start:
        adrp x9, kernel_boot_stack_top
        add x9, x9, :lo12:kernel_boot_stack_top
        mov sp, x9
        mov x29, xzr
        bl {rust_image_main}
    2:  wfi
        b 2b
    "#,
    rust_image_main = sym rust_image_main,
);

/// Rust 面入口：横幅 + 触达锚点 + 驻留。
///
/// 交接协议未接线（见模块文档"交付边界"），到达这里说明镜像已被
/// 执行——这是 NK1 跳转链路联调时的第一观测点，横幅就是串口证据。
fn rust_image_main() -> ! {
    // 触达锚点：运行路径上引用一次，保证锚点静态与其指向的内核
    // 启动图在归档成员粒度上也被拉入链接（KEEP 是链接期第二道保险）。
    let _kernel_entry: usize = black_box(KERNEL_ENTRY_ANCHOR as usize);

    early_console::write_str(
        "### minix-rs kernel image: entry reached (boot-shim handoff not \
         wired yet — NK1/OQ-N6); halting\n",
    );
    halt()
}

fn halt() -> ! {
    loop {
        // 中断全关的停机驻留。不改内存、不碰栈。
        // - x86_64：`cli` + `hlt`；
        // - aarch64：`wfi`（本镜像未开 WFI 陷入控制，且入口态由未来的
        //   交接协议定，属 NK1/OQ-N6 裁决范围；此处只需保证驻留）。
        #[cfg(target_arch = "x86_64")]
        unsafe {
            asm!("cli", "hlt", options(nomem, nostack))
        }
        #[cfg(target_arch = "aarch64")]
        unsafe {
            asm!("wfi", options(nomem, nostack))
        }
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    // 镜像期 panic 面走不进格式化 machinery（无堆依赖）：一行横幅
    // 即证据，然后与入口同款驻留。
    early_console::write_str("### minix-rs kernel image: PANIC — halting\n");
    halt()
}

// ── 镜像期最小 bump 分配器 ──────────────────────────────────────
//
// 内核启动图经锚点拉入链接后，alloc 面符号（__rust_alloc 等）随之
// 进入链接需求，bin 必须提供 global_allocator。这里只满足"镜像
// 链接面"：静态区域一次性 bump、不回收（dealloc 按契约安全空转）。
// 内核运行面的真实堆初始化（C 对位：kmalloc/mempool 初始化半）属
// NK1 入口接线波次的设计范围，不在此代决。

/// 128 KiB：只兜链接面与最小启动期的量级，非运行面配额承诺。
const IMAGE_HEAP_LEN: usize = 128 * 1024;

/// 零初始化静态数组 → 落 `.bss`（加载/拷贝阶段按 PT_LOAD memsz 清零）。
static IMAGE_HEAP: [u8; IMAGE_HEAP_LEN] = [0; IMAGE_HEAP_LEN];

struct ImageBump {
    cursor: AtomicUsize,
}

impl ImageBump {
    const fn new() -> Self {
        Self {
            cursor: AtomicUsize::new(0),
        }
    }
}

unsafe impl GlobalAlloc for ImageBump {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: 无前置并发契约的镜像期单线程阶段（引导 CPU 独占），
        // 原子游标仅为满足 GlobalAlloc 的 &self 契约；边界全部显式检查。
        let old = self
            .cursor
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |cur| {
                let aligned = cur.checked_add(layout.align() - 1)? & !(layout.align() - 1);
                let end = aligned.checked_add(layout.size())?;
                (end <= IMAGE_HEAP_LEN).then_some(end)
            });
        match old {
            Ok(new_end) => unsafe { IMAGE_HEAP.as_ptr().add(new_end - layout.size()) as *mut u8 },
            Err(_) => core::ptr::null_mut(),
        }
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        // 镜像期 bump 不回收：一次性阶段无配对释放需求；真回收面随
        // NK1 波次的堆设计落地（届时本分配器整体退役）。
    }
}

#[global_allocator]
static IMAGE_ALLOCATOR: ImageBump = ImageBump::new();
