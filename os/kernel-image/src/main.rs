//! 生产内核镜像 bin（NS8-A / NK4-B P3 M3.2 多架构 / P4 M4.2 三架构）：把
//! minix-kernel 链接成 boot-shim 契约位的内核 ELF。boot-shim 从
//! `/EFI/minix/kernel.elf`（`os/boot-shim/src/loader.rs`
//! `KERNEL_PATH`）读它做两件事：`compute_kernel_layout` 取布局填
//! `KernelInfo`（kern_virt_base / kern_phys_base / kern_size），
//! `load_segments_into_phys_memory` 按 PT_LOAD 的物理地址拷段体、清 BSS。
//! 布局契约由同目录的架构脚本定（高 VMA + 低物理 LMA，段间偏移一致）：
//! x86_64 用 `x86_64.ld`，aarch64 用 `aarch64.ld`，riscv64 用 `riscv64.ld`，
//! 由 `build.rs` 按 `--target` 选一份。Rust 面架构无关，只有入口指令序列
//! 与早期控制台按 `target_arch` 分道。
//!
//! 取件方按架构不同：x86_64 / aarch64 走 UEFI 装机面（xtask image →
//! boot-shim 读契约位）；riscv64 不是 UEFI 盘形，今天只由
//! `os/kernel-image/check-layout.sh riscv64` 产出并校验静态布局，装机面
//! 属 M4.3（`os/xtask/src/image.rs:195-203` 对该架构仍 honest bail）。
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

// aarch64 内核禁 NEON/FP 门（SD-23）：lazy-FPU 模型的「内核不用 FPU」前提
// 由构建旗标保证（xtask/src/image.rs 对本包的 aarch64 构建注入
// `-C target-feature=-neon,-fp-armv8`）。绕过 xtask 的直接 cargo 构建会
// 静默产出会踩用户 Q 寄存器的内核——在此编译期拒收。minix-arch 的显式
// FPSIMD 原语（fpu.rs）经 per-function target_feature 保留，不触发本门；
// boot-shim 的 UEFI 载体构建不编译本包（且载体上下文无用户进程，无踩踏
// 面），宿主测试与其它架构不受影响。
#[cfg(all(target_arch = "aarch64", target_feature = "neon"))]
compile_error!(
    "aarch64 kernel image must be built with -C target-feature=-neon,-fp-armv8 \
     (the lazy-FPU model requires a kernel that never touches Q0-Q31; \
     see SD-23 and xtask/src/image.rs)"
);

// riscv64 同族门（续-133）：sstatus.FS=Off 门控无 per-mode 分裂——内核
// 任何 FP 执行（含预编译 core 残差）都会自陷阱，必须真无 FP。
#[cfg(all(target_arch = "riscv64", target_feature = "f"))]
compile_error!(
    "riscv64 kernel image must be built with -C target-feature=-f,-d \
     (the lazy-FPU model requires a kernel that never touches f0-f31; \
     see SD-23 and xtask/src/image.rs)"
);

extern crate alloc;

use core::arch::asm;
// riscv64 腿的锚点触达住在 `bootface`（续-88 甲案），本入口不用 black_box。
#[cfg(not(target_arch = "riscv64"))]
use core::hint::black_box;
use core::panic::PanicInfo;
use core::sync::atomic::{AtomicUsize, Ordering};

use alloc::alloc::{GlobalAlloc, Layout};

use minix_boot::KernelInfo;
use minix_types::PhysBytes;

// 早期控制台按架构取板级实现（三者同名 `write_str`，因此 Rust 面无需
// 分道）：x86_64 = COM1 串口，aarch64 = PL011，riscv64 = QEMU virt 的
// 16550 UART（`os/plat/src/riscv64/early_console.rs`）。
#[cfg(target_arch = "aarch64")]
use minix_plat::arm64::early_console;
#[cfg(target_arch = "riscv64")]
use minix_plat::riscv64::early_console;
#[cfg(target_arch = "x86_64")]
use minix_plat::x86_64::early_console;

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
// 三个架构各一份指令序列，语义逐条对应：取栈顶 → 清帧指针 → 调 Rust
// 面 → 停机驻留。aarch64 的 `adrp` + `add :lo12:` 取符号写法抄仓内既有
// 载体（`os/qemu-tests/test-kernels/kernel/bootstrap/test-rt-birth-aarch64/src/main.rs:250-251`）；
// riscv64 用 `la`（链接器松弛为 PC 相对的 `auipc` + `addi`），这样低半/
// 高半两个执行视图下都能取到栈顶符号（NK4-B P4 M4.2 决策四）。

#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(
    r#"
    .section .text.boot, "ax"
    .globl _start
    _start:
        // SD-7 / P-X86-03（2026-10-06）：防御性 .bss 清零，镜像 riscv64
        // 甲案（UEFI boot-shim 的 loader 已按 memsz-filesz 清零；本循环
        // 是 SD-1 多入口形态下的入口自持——未来直载入口不依赖装载者）。
        // RIP 相对取运行位址：分页开启（高半映射）与恒等映射两种交接
        // 视图下都落在正确的物理页。rdi 游标/rcx 界限在入口处无活值。
        lea rdi, [rip + __bss_start]
        lea rcx, [rip + __bss_end]
    1:  cmp rdi, rcx
        jae 2f
        mov qword ptr [rdi], 0
        add rdi, 8
        jmp 1b
    2:
        lea rsp, [rip + kernel_boot_stack_top]
        xor ebp, ebp
        call {rust_image_main}
    3:  cli
        hlt
        jmp 3b
    "#,
    rust_image_main = sym rust_image_main,
);

#[cfg(target_arch = "aarch64")]
core::arch::global_asm!(
    r#"
    .section .text.boot, "ax"
    .globl _start
    _start:
        // SD-7 / P-X86-03（2026-10-06）：防御性 .bss 清零，镜像 riscv64
        // 甲案（同 x86_64 注释：loader 已清是现行 UEFI 路径事实，入口
        // 自持为 SD-1 多入口形态）。adrp 取运行页位址（MMU 交接态下
        // 无论恒等或高半映射都落正确物理页）。x9 游标/x10 界限无活值。
        adrp x9, __bss_start
        add x9, x9, :lo12:__bss_start
        adrp x10, __bss_end
        add x10, x10, :lo12:__bss_end
    1:  cmp x9, x10
        b.hs 2f
        str xzr, [x9], #8
        b 1b
    2:
        adrp x9, kernel_boot_stack_top
        add x9, x9, :lo12:kernel_boot_stack_top
        mov sp, x9
        mov x29, xzr
        bl {rust_image_main}
    3:  wfi
        b 3b
    "#,
    rust_image_main = sym rust_image_main,
);

#[cfg(target_arch = "riscv64")]
core::arch::global_asm!(
    r#"
    .section .text.boot, "ax"
    .globl _start
    _start:
        # NK4-C 续-88 甲案：OpenSBI 递入的是裸 RAM（NOBITS 不写），而镜像
        # 的 bump 分配器游标/平台冻结全局就住在 .bss（NK4B M4.4 事实五），
        # 非零垃圾让首次分配返回乱址——先清 .bss 再立栈（K10 载体先例的
        # 形式，bgeu 无符号界判定 + 8 字节步进）。清零循环全走 `la` 松弛后
        # 的 PC 相对引用：执行视图 = 物理基址（medany，M4.3 实测事实），
        # 栈顶符号同样按运行位址解析，清零只会扫自己下方的镜像内区域。
        la t0, __bss_start
        la t1, __bss_end
    1:  bgeu t0, t1, 2f
        sd zero, 0(t0)
        addi t0, t0, 8
        j 1b
    2:
        la sp, kernel_boot_stack_top
        li ra, 0
        li fp, 0
        # 甲案交接：OpenSBI 递 a0 = boot hartid、a1 = DTB 物理指针；U-Boot
        # 腿另约定 a2 = BootFileTable 物理指针（本 boot 面同时读 DTB
        # /chosen/opensbi,boot-file-table，a2 不合法时靠 DTB 通道；两者均
        # 过 magic 把关才解引用）。`call` 是 PC 相对，不改动 a0–a2，三个
        # 入参自然落位 Rust 面（rt-birth 载体 :426 同形先例）。
        call {rust_image_main}
    3:  wfi
        j 3b
    "#,
    rust_image_main = sym rust_image_main,
);

#[cfg(target_arch = "riscv64")]
mod bootface;

/// Rust 面入口（x86_64 形态；aarch64/riscv64 各有接线版见下）：横幅 +
/// 触达锚点 + 驻留。
///
/// 交接协议未接线（见模块文档"交付边界"），到达这里说明镜像已被
/// 执行——这是 NK1 跳转链路联调时的第一观测点，横幅就是串口证据。
#[cfg(all(not(target_arch = "aarch64"), not(target_arch = "riscv64")))]
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

/// §1.115 方案 A aarch64 镜像入口：boot-shim 建表 + enable 后绝对跳进本镜像
/// 高半 `_start`，`x0`＝[`BootHandoff`] blob 物理指针（identity 可达）。_start
/// 的 `bl` 不改动 `x0`，故它作为首参传入。
///
/// 本镜像链接在高半（`aarch64.ld` 的 `KERN_VIRT_BASE`），故此处调用
/// `minix_kernel::arch_boot_resume_high_half` 时，其内部的 `sym kmain`、异常
/// 向量、`.bss` 全局全部解析为真高半地址——执行流全程走永不切换的 TTBR1，
/// 免疫 `switch_address_space`（这正是 §1.114 内联 PE 低半执行模型的反面）。
///
/// 交接契约：
/// 1. 先把 boot-shim 留下的 bump 游标 `resume` 到本镜像自己的 `BOOT_ALLOC`
///    副本，令后续 DM 建页避开 boot-shim 已占的页表页/blob 页。
/// 2. `arch_boot_resume_high_half` 只做 *additive* 阶段（store KernelInfo →
///    DM 覆盖 → `jump_to_kmain`），绝不重建 bootstrap 根——根已被 boot-shim
///    建好且此刻正激活，`new_from_page` 清零会当场摧毁自身取指映射。
#[cfg(target_arch = "aarch64")]
fn rust_image_main(handoff: *const minix_boot::BootHandoff) -> ! {
    let _kernel_entry: usize = black_box(KERNEL_ENTRY_ANCHOR as usize);
    // 走 §1.114 的 TTBR1 高半 MMIO 别名（本镜像自己的 HIGH_MMIO_LIVE 全局），
    // 证明跨镜像高半控制台可用；resume 路径内部会再次置位（幂等）。
    early_console::use_high_mmio_window();
    if handoff.is_null() {
        early_console::write_str("### minix-rs kernel-image: handoff blob NULL — aborting\n");
        return halt();
    }
    // SAFETY: boot-shim wrote a valid BootHandoff to a bump-allocated identity
    // page and passed its physical address in x0; identity (0..4GB) is live in
    // the shared bootstrap root's TTBR0 until the first root switch, which has
    // not happened yet. The blob is Copy/POD.
    let h = unsafe { &*handoff };
    // Resume the boot bump allocator from the boot-shim's post-build cursor so
    // the additive DM phase never reuses a page already in service. This must
    // precede the resume call so its Step 0 sees the region already registered
    // and skips the fallback `init` (which would reset the cursor).
    minix_kernel::boot_alloc::resume_boot_pt_alloc(h.bump_base, h.bump_next, h.bump_end);
    early_console::write_str(
        "### minix-rs kernel-image: resuming high-half boot (additive §1.115 A)\n",
    );
    // Hand the boot KernelInfo (slices point into stable identity-mapped
    // physical memory) + the already-live bootstrap root back to the kernel
    // crate, which now runs the remaining phases entirely at high virtual
    // addresses — without rebuilding (which would clobber) the active root.
    minix_kernel::arch_boot_resume_high_half(&h.kernel_info, PhysBytes(h.root_page))
}

/// NK4-C 续-88 甲案 riscv64 镜像入口：kernel-image 自当引导体（无跨镜像
/// 交接、无 BootHandoff）。入口 asm 已清 `.bss`（甲案共同前置，NK4B M4.4
/// 事实五），a0/a1/a2 依次落为三个入参；真活在 `bootface`：a1 DTB 解
/// memmap → BootFileTable（a2/chosen 双通道，magic 闸）→ 12 模块装载 →
/// `minix_kernel::arch_boot` 过 validate 真门槛。
#[cfg(target_arch = "riscv64")]
fn rust_image_main(boot_hart: u64, dtb_phys: u64, boot_file_table_pa: u64) -> ! {
    bootface::bootface(boot_hart, dtb_phys, boot_file_table_pa)
}

fn halt() -> ! {
    loop {
        // 中断全关的停机驻留。不改内存、不碰栈。
        // - x86_64：`cli` + `hlt`；
        // - aarch64：`wfi`（本镜像未开 WFI 陷入控制，且入口态由未来的
        //   交接协议定，属 NK1/OQ-N6 裁决范围；此处只需保证驻留）；
        // - riscv64：同名 `wfi`（S-mode 可用；是否被 `mstatus.TW` 陷入
        //   取决于 OpenSBI 配置，同属入口态未接线那条边界）。
        #[cfg(target_arch = "x86_64")]
        unsafe {
            asm!("cli", "hlt", options(nomem, nostack))
        }
        #[cfg(target_arch = "aarch64")]
        unsafe {
            asm!("wfi", options(nomem, nostack))
        }
        #[cfg(target_arch = "riscv64")]
        unsafe {
            asm!("wfi", options(nomem, nostack))
        }
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    // 镜像期 panic 面走不进格式化 machinery（无堆依赖）：用定长栈缓冲
    // 把 panic 消息 + 位置写进早期控制台再驻留（§1.115 调试需求：仅一行
    // "PANIC" 无助于定位，必须带 payload）。
    early_console::write_str("### minix-rs kernel image: PANIC — ");
    struct StackWriter<'a> {
        buf: &'a mut [u8],
        pos: usize,
    }
    impl core::fmt::Write for StackWriter<'_> {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            for &b in s.as_bytes() {
                if self.pos >= self.buf.len() {
                    break;
                }
                self.buf[self.pos] = b;
                self.pos += 1;
            }
            Ok(())
        }
    }
    let mut buf = [0u8; 200];
    let mut w = StackWriter { buf: &mut buf, pos: 0 };
    // `write_fmt` on `PanicInfo` renders the message; append the location.
    let _ = core::fmt::write(&mut w, format_args!("{info}"));
    let n = w.pos;
    // The 200-byte cap can truncate mid-`char`, so validate rather than
    // assume ASCII: a non-UTF-8 tail falls back to a shorter valid prefix
    // (`utf8::floor`-style loop) instead of UB via `from_utf8_unchecked`.
    let mut end = n;
    while end > 0 && core::str::from_utf8(&buf[..end]).is_err() {
        end -= 1;
    }
    let text = core::str::from_utf8(&buf[..end]).unwrap_or("");
    early_console::write_str(text);
    early_console::write_str(" — halting\n");
    halt()
}

// ── 镜像期最小 bump 分配器 ──────────────────────────────────────
//
// 内核启动图经锚点拉入链接后，alloc 面符号（__rust_alloc 等）随之
// 进入链接需求，bin 必须提供 global_allocator。这里只满足"镜像
// 链接面"：静态区域一次性 bump、不回收（dealloc 按契约安全空转）。
// 内核运行面的真实堆初始化（C 对位：kmalloc/mempool 初始化半）属
// NK1 入口接线波次的设计范围，不在此代决。

/// 1 MiB：兜链接面与最小启动期的量级，非运行面配额承诺。NK4-C 续-88
/// 甲案把 128 KiB 提到 1 MiB：`.bss` 已由入口 asm 清零（游标可信），而
/// 任何走堆的 boot 腿（含 panic 诊断旁路）都需要模块量级的宽裕；模块
/// 拷贝本身走物理 bump 暂存页、不消耗堆（见 `bootface`）。注意堆宽会
/// 移动镜像跨距：`check-layout.sh` L9 用 readelf 实测 ELF 跨距（末段
/// VMA+memsz − 首段 VMA）与 `bootface.rs` 的 `KERN_SIZE` 字面量对账
/// （堆宽变→跨距变→L9 当场红）；不在 `.ld` 开符号节（多一个 PT_LOAD
/// 会把跨距推成非 2MiB 整数倍，反破 L5/L7，续-88 实试过已回退）。
const IMAGE_HEAP_LEN: usize = 1024 * 1024;

/// 镜像堆本体。CodeReview NIT-8 实锤：`static IMAGE_HEAP = [0; N]` 只经
/// `as_ptr()` 取址，rustc 把它当只读常量落进 `.rodata`——而 bump 分配器
/// 往里写！改为 `static mut`（无安全引用、只经 `&raw` 地址算术使用），
/// 零初始化对象落 `.bss`，与「可写堆」语义一致，也消除页保护生效后
/// 写只读段的潜伏坏形（三架构同益：x86/aarch64 今天走 boot-shim 产线
/// 未暴露，属潜伏债）。
static mut IMAGE_HEAP: [u8; IMAGE_HEAP_LEN] = [0; IMAGE_HEAP_LEN];

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
            // SAFETY: `IMAGE_HEAP` 是独占的镜像期堆区，仅本分配器经地址
            // 算术写入（无并发引用）；偏移已由上面的 `end <= IMAGE_HEAP_LEN`
            // 显式限界。
            Ok(new_end) => unsafe {
                (&raw const IMAGE_HEAP).cast::<u8>().add(new_end - layout.size()) as *mut u8
            },
            Err(_) => {
                // NK4-C 1.5c 取证（task1-close 裁决删除）：内核 bump arena
                // 耗尽——锁定 OOM 是否落在内核 ImageBump（非增长 128KiB）。
                // 无分配路径：定长标签 + 游标/请求字节数手动 hex。
                let cur = self.cursor.load(Ordering::Relaxed);
                nk4c_oom_hex(b"nk4c: OOM-KERN cur=", cur, layout.size());
                core::ptr::null_mut()
            }
        }
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        // 镜像期 bump 不回收：一次性阶段无配对释放需求；真回收面随
        // NK1 波次的堆设计落地（届时本分配器整体退役）。
    }
}

/// NK4-C 1.5c 取证（task1-close 裁决删除）：无分配的 hex 行打印——
/// `tag` + `cur`（8 位） + ` req=` + `req`（8 位） + 换行。
fn nk4c_oom_hex(tag: &[u8], cur: usize, req: usize) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut buf = [0u8; 64];
    let mut n = 0;
    for &b in tag {
        buf[n] = b;
        n += 1;
    }
    let put = |buf: &mut [u8], n: &mut usize, v: usize| {
        for i in (0..8).rev() {
            buf[*n] = HEX[(v >> (i * 4)) & 0xf];
            *n += 1;
        }
    };
    put(&mut buf, &mut n, cur);
    buf[n] = b' ';
    n += 1;
    buf[n] = b'r';
    buf[n + 1] = b'e';
    buf[n + 2] = b'q';
    buf[n + 3] = b'=';
    n += 4;
    put(&mut buf, &mut n, req);
    buf[n] = b'\n';
    n += 1;
    if let Ok(s) = core::str::from_utf8(&buf[..n]) {
        early_console::write_str(s);
    }
}

#[global_allocator]
static IMAGE_ALLOCATOR: ImageBump = ImageBump::new();
