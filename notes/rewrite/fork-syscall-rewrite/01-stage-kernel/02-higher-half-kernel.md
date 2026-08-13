# 02-higher-half-kernel: 链接、加载与高地址跳转

> **分类**: 全局基建
> **源码**: `minix3/minix/kernel/arch/i386/head.S`, `minix3/minix/kernel/arch/i386/kernel.lds`, `os/kernel/src/lib.rs`
> **说明**: 链接脚本布局、内核 ELF 加载（ELF 解析、段拷贝、BSS 清零）、arch_boot_impl 页表映射、HigherHalf 切栈跳转——从 boot-shim 加载内核 ELF 到 kmain 的完整路径
> **前置**: [01-boot-shim-bootstrap.md](01-boot-shim-bootstrap.md) — boot-shim 已完成引导准备（获取内存映射、加载内核 ELF、构造 KernelInfo、ExitBootServices），调用 arch_boot

---

## 1. 概述

### 1.1 问题定义

01 文档结束时，`arch_boot_impl()` 已建立恒等映射和内核高地址映射，分页已启用。但 `arch_boot_impl()` 本身是在低地址被调用的——boot-shim 调用它时还没有页表。此时系统处于一个**中间态**：页表已就绪，但 CPU 的 RIP/PC 和 RSP/SP 仍在低地址。

这个中间态不能直接进入 `kmain()`。如果直接 `call kmain`，`kmain` 的符号地址虽然是高地址，但**返回地址**仍被压入低地址栈。后续中断/异常需要栈操作时，若恒等映射已移除，就会 page fault。

01 文档的 `arch_boot()` 通过 `HigherHalf::jump_to_kmain()` 解决这个中间态。这个跳转需要完成两个关键操作：

1. **切换栈指针**到高地址
2. **跳转指令指针**到高地址

这两步必须**成对完成**——只跳不切，栈仍在低地址，恒等映射移除后栈操作崩溃；只切不跳，下一条指令仍在低地址，同样崩溃。

`jump_to_kmain()` 是一个"有去无回"的跳转（`-> !`），确保内核从 `kmain` 开始完全运行在高地址空间。本文档详细展开这个跳转的实现机制。

### 1.2 为什么只有内核需要高地址

在 Minix3 的微内核架构中，只有内核运行在**高地址**（`0xF0400000` on x86-32, `0xFFFF_8000_0000_0000` on x86-64）。其他所有进程（VM、PM、VFS 等）的地址空间由 VM 管理，各自拥有独立的低地址空间（0~3GB 用户区）。

内核必须在高地址的原因：

1. **最大化用户地址空间**：内核占据高地址，用户进程可以使用完整的低地址空间
2. **隔离保护**：高地址的内核代码/数据对用户态不可见（supervisor-only 页）
3. **直接映射窗口**：高地址空间中可以放置 Direct Map（`va = pa + BASE`），让内核能访问所有物理内存

### 1.3 物理代码从未移动

一个关键认知：**内核代码在物理内存中的位置从未改变**。从 GRUB/boot-shim 将内核 ELF 加载到物理内存的那一刻起，代码就一直驻留在 `kern_phys_base`。

"跳到高地址"不是把代码搬走，而是让 CPU 通过**不同的映射窗口**访问同一块物理内存：

```
低地址窗口（恒等映射）:  VA 0x0020_0000 → PA 0x0020_0000 → [kernel code]
高地址窗口（内核映射）:  VA 0xFFFF_8000_0020_0000 → PA 0x0020_0000 → [kernel code]
```

两个虚拟地址映射到**同一块物理内存**。HigherHalf 跳转做的就是让 CPU 从低地址窗口切换到高地址窗口。

### 1.4 三件事必须配合

> **缩写说明**：
> - **VMA** = Virtual Memory Address（虚拟内存地址），链接器为符号解析的地址（运行时地址）
> - **LMA** = Load Memory Address（加载内存地址），bootloader 将代码加载到物理内存的地址

higher-half kernel 需要三件事协同工作：

| 组件 | 职责 | 失败后果 |
|------|------|---------|
| **链接脚本** (link.ld) | 设置 VMA=高地址，LMA=低物理 | 内核符号地址错误，调用任何函数都会跳到错误位置 |
| **ELF 加载器** (boot-shim) | 按 p_paddr（LMA）加载段到物理内存 | 内核代码不在预期的物理地址 |
| **HigherHalf trait** | 切栈+跳转到高地址的 kmain（内联汇编实现） | RIP/RSP 在低地址，恒等映射移除后崩溃 |

三者缺一不可。链接脚本决定"内核认为自己在哪里"，ELF 加载器决定"内核实际放在物理内存的哪里"，HigherHalf trait 决定"CPU 从哪个地址窗口执行内核"。

---

## 2. C 源码分析

### 2.0 GRUB 加载配置

Minix3 C 中，GRUB 通过 `boot.cfg` 配置直接加载内核和模块。GRUB 不依赖 boot-shim，而是作为外部工具完成所有磁盘 I/O：

**源码**：`minix3/etc/boot.cfg.default`

```
clear=1
timeout=5
default=2
menu=Start MINIX 3:load_mods /boot/minix_default/mod*;multiboot /boot/minix_default/kernel rootdevname=$rootdevname $args
menu=Start latest MINIX 3:load_mods /boot/minix_latest/mod*;multiboot /boot/minix_latest/kernel rootdevname=$rootdevname $args
menu=Start latest MINIX 3 in single user mode:load_mods /boot/minix_latest/mod*;multiboot /boot/minix_latest/kernel rootdevname=$rootdevname bootopts=-s $args
menu=Edit menu option:edit
menu=Drop to boot prompt:prompt
```

| 指令 | 作用 |
|------|------|
| `multiboot /boot/.../kernel` | GRUB 扫描内核前 8KB 的 Multiboot Header（`0x1BADB002`），按 ELF 的 `p_paddr`（LMA）将段加载到物理内存 |
| `load_mods /boot/.../mod*` | GRUB 将模块二进制读入物理内存，填充 `multiboot_info_t.mi_mods_count/addr` |
| `rootdevname=... $args` | 启动参数字符串，GRUB 填入 `multiboot_info_t.mi_cmdline` |

> **C 版 vs Rust 版**：C 版中 GRUB 是外部工具，内核只消费最终数据结构（`multiboot_info_t`）；Rust 版中 boot-shim 是内核项目的一部分，需要自行实现所有加载逻辑（ELF 解析、段拷贝、BSS 清零等），详见 §4.2。

### 2.1 kernel.lds：VMA、LMA 与 AT() 机制

Minix3 x86-32 的链接脚本 (`kernel/arch/i386/kernel.lds`)：

```c
_kern_phys_base = 0x00400000;   // LMA: 内核在物理内存的起始地址 (4MB)
_kern_vir_base  = 0xF0400000;   // VMA: 内核认为自己在虚拟地址空间的位置
_kern_offset    = (_kern_vir_base - _kern_phys_base);  // 偏移 = 0xF0000000
```

关键机制是 `AT()` 指令：

```ld
.text : AT(ADDR(.text) - _kern_offset) { *(.text*) }
```

- `.text` 的 VMA = `_kern_vir_base + offset`（高地址）——链接器用这个地址解析符号
- `AT(ADDR(.text) - _kern_offset)` 设置 LMA = VMA - offset = 物理地址
- GRUB 按 LMA（p_paddr）加载段到物理内存

**效果**：内核的所有符号（函数地址、全局变量地址）都解析为高地址，但实际代码被 GRUB 放在低物理地址。分页启用后，CPU 通过高地址映射访问这些代码。

> **演进说明**：Minix3 的链接脚本还包含 `unpaged_text/data/bss` 段组（kernel.lds:15-20）和 `usermapped/usermapped_glo` 段组（kernel.lds:24-28）。后者是 Minix3 用户态段共享机制（USMAPPED 宏），在 64-bit 重写中已废弃；前者需要解释——
>
> **unpaged 段的存在原因**：不是所有内核代码都能跳高地址。分页**启用前**必须执行的代码（`pre_init`/`pg_identity`/`pg_mapkernel`/`vm_enable_paging` 等，`unpaged_*.o` 对象组，含入口 `__k_unpaged_MINIX`，见 `arch/i386/Makefile.inc`）必须留在低地址（恒等映射）——此时页表尚未建立，高地址映射还不存在，任何高地址取指都会立即 page fault。C 用链接脚本的 `.unpaged_text/data/bss` 段（kernel.lds:15-20，VMA=LMA=物理低地址，无 AT()）承载它们，**内核入口点本身就在 unpaged 段**。minix-rs 不需要 unpaged 段：这些职责全部位于 boot-shim（独立二进制，天然在低物理地址执行，见 01 文档 §4.2），内核 ELF 从 `kmain` 起直接位于高地址——分页开启后的跳转由 `HigherHalf::jump_to_kmain` 完成（§3.4）。

ARM32 的链接脚本 (`kernel/arch/earm/kernel.lds`) 使用完全相同的模式：

```c
_kern_phys_base = 0x80200000;   // ARM 物理基址
_kern_vir_base  = 0xF0400000;   // 同样的高地址
_kern_offset    = (_kern_vir_base - _kern_phys_base);
```

### 2.2 head.S：从 pre_init 到 kmain 的三行关键代码

x86-32 的 `head.S` (`kernel/arch/i386/head.S:78-87`)（L78-82 注释 + L83-87 实际 asm 四行）：

```asm
        /* pre_init 返回后，分页已启用，但 RIP/RSP 仍在低地址 */
        mov     $k_initial_stktop, %esp  // (1) 切栈到高地址
        push    $0                       // (2) 栈终止标记
        push    %eax                     // (3) 压入 kinfo 指针
        call    _C_LABEL(kmain)          // (4) 跳转到高地址的 kmain
```

为什么 `call kmain` 能跳到高地址？因为链接器将 `kmain` 的地址解析为 `_kern_vir_base + offset`（高地址）。这条 `call` 指令的立即数就是高地址——链接时决定的，运行时不需要计算。

ARM32 的 `head.S` (`kernel/arch/earm/head.S:41-47`)：

```asm
        ldr     sp, =k_initial_stktop    // 切栈到高地址
        mov     r1, #0
        push    {r1}                     // 栈终止标记
        ldr     r2, =_C_LABEL(kmain)     // 加载 kmain 的高地址
        bx      r2                       // 跳转到高地址
```

ARM 版本与 x86 版本的语义完全相同：切栈 + 跳转。区别仅在于 ARM 用 `bx r2` 间接跳转（因为 `bl` 的立即数范围不够），而 x86 用 `call` 直接跳转。

### 2.3 切栈和跳高地址必须成对

| 情况 | 结果 | 原因 |
|------|------|------|
| 只切栈，不跳 | 栈操作正常，但下一条指令在低地址 | RIP 仍在低地址，恒等映射移除后崩溃 |
| 只跳，不切 | RIP 在高地址，但 RSP 在低地址 | 栈操作（push/call/ret）访问低地址，恒等映射移除后崩溃 |
| 先跳后切 | 不可行 | 跳转后下一条指令在高地址，但还没切栈，push 操作可能覆盖高地址映射区域 |
| **先切后跳** | **正确** | 栈先就位，然后跳转，跳转后的第一条指令在高地址执行，栈也在高地址 |

Minix3 C 的做法是**先切栈后跳转**，这是唯一正确的顺序。

### 2.4 k_initial_stktop 的来源

在 Minix3 C 中，`k_initial_stktop` 是链接脚本中定义的符号，指向内核 BSS 段末尾的栈区域：

```c
// kernel.lds 末尾
.bss ALIGN(4096) : AT(ADDR(.bss) - _kern_offset) { *(.bss* COMMON)
    _kern_size = . - _kern_vir_base;
}
_end = .;
```

`k_initial_stktop` 指向 `_end` 附近的区域，由内核启动代码在 BSS 中预留。因为 `_end` 的 VMA 是高地址，所以 `k_initial_stktop` 自然也是高地址。

**minix-rs 的替代方案**：不再使用链接脚本符号。栈顶地址作为 `stack_top: VirBytes` 参数传入 `HigherHalf::jump_to_kmain`，其值来自 `KernelInfo.kern_stack_top`。`kern_stack_top` 由 boot-shim 在构造 `KernelInfo` 时计算（例如 `kern_virt_base + kern_size + stack_size`，其中 `kern_size: u64` 避免 32 位截断——见附录 A.5），然后在 `arch_boot()` 中通过 `info.kern_stack_top` 传入：

```rust
// os/kernel/src/lib.rs
let info = arch_boot_impl::<PagingImpl>(kernel_info, root_page);
unsafe { X86_64HigherHalf::jump_to_kmain(info, info.kern_stack_top) }
```

这种设计避免了链接脚本与 Rust 代码的耦合，使 mock 测试可以直接传入任意栈顶值，也更符合 Rust 的类型安全原则。

---

## 3. Rust 设计决策

### 3.1 决策：不保留 AT() 机制

**Minix3 C 的 AT() 机制**让 GRUB 按 p_paddr（LMA）加载内核到低物理地址，同时符号解析为高地址（VMA）。这要求 bootloader 理解 ELF 的 p_paddr 字段。

**minix-rs 不保留 AT()**，原因：

1. **boot-shim 全盘控制加载路径**：boot-shim 自己解析 ELF、复制段到物理内存。它不依赖 GRUB 的 ELF 加载器，因此不需要 AT() 来告诉 bootloader "请把段放在这个物理地址"
2. **UEFI 不走 GRUB 路径**：UEFI 启动时，boot-shim 通过 `SimpleFileSystem` 读取 kernel.elf，自己调用 `load_segments_into_phys_memory()`。AT() 对 UEFI 没有意义
3. **OpenSBI/U-Boot 路径同理**：U-Boot 的 `fatload` 只是把文件内容原样拷贝到指定地址，不解析 ELF 的 p_paddr

**替代方案**：boot-shim 的 `compute_kernel_layout()` 从 ELF 的 PT_LOAD 段提取 `paddr`（物理地址）和 `vaddr`（虚拟地址），然后按 `paddr` 放置段。链接脚本中仍然设置 VMA=高地址，但不需要 AT()——boot-shim 自己处理 LMA。

> **PT_LOAD**：ELF 程序头表（Program Header）中的一种段类型，表示"加载到内存的段"。每个 PT_LOAD 段包含 `p_vaddr`（虚拟地址）、`p_paddr`（物理地址）、`p_filesz`（文件中大小）、`p_memsz`（内存中大小）等字段——boot-shim 按 `p_paddr` 将段数据拷贝到物理内存，并将 `[p_filesz, p_memsz)` 清零（BSS）。详见 §4.2。

### 3.2 决策：独立 kernel ELF

**为什么不能把内核编译为 rlib**：

rlib（Rust 静态库）是中间产物，不能直接执行。它没有入口点、没有段布局、没有链接脚本控制。higher-half kernel 需要精确控制 VMA 和 LMA，这只能通过独立 ELF + 自定义链接脚本实现。

**minix-rs 的内核构建方式**：

```
kernel.elf = link(kernel.rlib + link.ld + crt0.o)
```

- `kernel.rlib`：内核的 Rust 代码编译为静态库
- `link.ld`：自定义链接脚本，设置 VMA=高地址
- 最终链接为独立 ELF，boot-shim 加载此 ELF

> **编译期 code model 约束**：x86-64 默认 small code model 要求所有符号位于 ±2GB RIP-relative 可寻址范围内。链接到高地址后，内核镜像内部符号相互距离 < 2GB 时仍安全（镜像当前远小于 2GB）；若未来镜像膨胀或跨越大地址窗口，需显式设置 `-C code-model=large`（类似 Linux 的 `-mcmodel=kernel`、Fusion OS 的 `-mcmodel=large`），通过 `.cargo/config.toml` 的 rustflags 配置。aarch64/riscv64 的链接器模型同样有各自的高地址寻址约束。

> **实现状态**：三架构的 `link.ld`（`os/kernel/src/arch/{x86_64,aarch64,riscv64}/link.ld`）已就绪，但构建系统尚未接入——`os/kernel/Cargo.toml` 仍只声明 `[lib]`，缺少 `build.rs` 将 `kernel.rlib` + `link.ld` 链接为独立 ELF binary。当前测试路径（hello-boot 等）通过 `minix-kernel = { workspace = true }` 以 rlib 方式依赖内核 crate，这是测试场景的合理简化；生产路径的独立 ELF 构建是后续工作。01 文档 §5.2 已标注此约束（"正式的 boot-shim 需要将 kernel 改为独立 ELF binary"）。

### 3.3 决策：HigherHalf trait + 内联汇编

**不用独立 assembly 文件**（trampoline.S）的原因：

Minix3 C 内核使用独立汇编文件 `head.S` 实现切栈+跳转，因为 C 语言无法精确控制寄存器和栈指针。但在 Rust 中：

1. **内联汇编足够精准**：`core::arch::asm!` 提供精确的寄存器控制（`in`, `out`, `clobber`），`options(noreturn)` 保证编译器不插入栈帧 —— 消除 C 中需要独立汇编文件的原因
2. **trait 抽象跨架构**：三种架构（x86-64、aarch64、riscv64）的跳转语义相同但指令不同，通过 `HigherHalf` trait 在各自模块中实现，无需维护三个 `.S` 文件
3. **类型安全**：trait 签名 `unsafe fn jump_to_kmain(kinfo: &KernelInfo, stack_top: VirBytes) -> !` 由编译器强制检查，比汇编宏中的裸寄存器约定更安全。`stack_top` 作为参数传入（来自 `KernelInfo.kern_stack_top`），避免了链接脚本符号耦合，也使 mock 测试可以直接传入任意栈顶值

### 3.4 决策：HigherHalf trait 抽象

三种架构的跳转语义相同（切栈+跳转到高地址），但具体指令不同。我们定义 `HigherHalf` trait 来抽象这一机制：

```rust
/// Abstraction for the higher-half kernel transition.
///
/// After paging is enabled, the CPU still executes at low addresses
/// (identity-mapped). This trait provides the architecture-specific
/// mechanism to switch the stack pointer and jump to the kernel's
/// high virtual address entry point (kmain).
pub trait HigherHalf {
    /// Perform the higher-half transition: switch stack to high address
    /// and jump to kmain.
    ///
    /// # Arguments
    ///
    /// - `kinfo`: Pointer to KernelInfo, passed as the first argument to kmain
    /// - `stack_top`: Virtual address of the kernel stack top (high address)
    ///
    /// # Safety
    ///
    /// Caller must ensure:
    /// - Paging is enabled with both identity and kernel high mappings
    /// - `kinfo` pointer is valid and accessible at high address
    /// - `stack_top` is a valid, 16-byte-aligned virtual address in the
    ///   kernel's high-half mapping
    /// - This is called exactly once, from the boot CPU
    unsafe fn jump_to_kmain(kinfo: &KernelInfo, stack_top: VirBytes) -> !;
}
```

**为什么需要 trait 而不是 `#[cfg(target_arch)]`**：

1. **架构隔离**：每种架构的实现完全不同（x86-64 用 `mov rsp + call`，aarch64 用 `mov sp + br`，riscv64 用 `mv sp + jalr`），trait 将差异封装在实现中
2. **kernel/lib.rs 统一调用**：`arch_boot_impl` 返回后调用 `HigherHalf::jump_to_kmain()`，无需 `#[cfg]`
3. **可测试性**：mock 实现可以验证调用时序而不执行真实指令

**跨系统解法对比**：trampoline 跳转是 higher-half kernel 的**硬性要求**——分页开启后恒等映射一旦移除，仍指向低地址的 RIP/RSP 立即失效（取指与栈访问都会崩溃）。同一问题在不同系统的解法各异：

| 系统 | 方式 | 关键特征 |
|------|------|---------|
| Minix3 C | `AT()` 链接脚本 + head.S 汇编 trampoline | 同一段代码按 LMA 物理 4MB 加载、按高 VMA 执行；`AT()` 是 GNU ld 特有语法，依赖 GRUB 理解 ELF 的 p_paddr（见 §2.1，minix-rs 的取舍见 §3.1） |
| Redox | kernel ELF 链接到高地址 + bootloader 解析 ELF 段 | 更"诚实"：kernel ELF 的 VMA 就是真实高地址，bootloader 负责拷段到低物理地址、建页表、trampoline 跳高地址 |
| Linux | `lretq` far return | 64 位特有：一条指令同时切 CS 和 RIP 到高地址；压缩内核解压后已在高地址，无需 AT() |
| seL4 | elfloader 加载 kernel ELF + kernel 自己准备栈 | elfloader 把 PT_LOAD 段拷到物理内存、建页表、跳 kernel entry；栈由 kernel entry 函数自己准备 |

minix-rs 的路径（Redox 模式 + 内联汇编）：boot-shim 解析 PT_LOAD 段拷到物理内存（§3.2），跳转用 Rust 内联汇编而非独立汇编文件（§3.3），跨架构统一抽象由 `HigherHalf` trait 提供（§3.4）。不用 `AT()`（lld 不支持 GNU ld 特有语法 + UEFI/OpenSBI 路径无 GRUB 替你处理 LMA vs VMA，见 §3.1）；不用 `lretq`（x86 特有，ARM64/RISC-V 无对应语义）。

**为什么必须切栈——地址问题而非大小问题**：Minix3 C 中切栈有双重意义——旧栈只有 4KB（`load_stack_start`，head.S 临时栈），新栈是 BSS 大栈。minix-rs 的 UEFI 路径没有"小栈"问题（UEFI 栈通常 128KB+，boot-shim 调用 `arch_boot_impl` 期间可一直用），但**切栈仍必须做**：UEFI 栈在低地址（恒等映射区），`jump_to_kmain` 后的高地址内核不用它——恒等映射一旦移除（VM 启动后），低地址栈立即失效。`kern_stack_top` 的意义不是"更大的栈"，而是"位于高地址映射区的栈"：head.S 的 `mov $k_initial_stktop, %esp` 切的是栈的**地址**，不只是大小。

### 3.5 架构差异对照

> **注**：本表为高层概念性对照，具体汇编指令见 §4.3 三架构差异总结表。两表必须保持一致（Step 0.5.5 跨章节一致性约束）。

| 方面 | x86-64 | aarch64 | riscv64 |
|------|--------|---------|---------|
| 栈切换 | `mov rsp, {stktop}` | `mov sp, {stktop}` | `mv sp, {stktop}` |
| 跳转方式 | `call {kmain}` (直接) | `ldr x1, ={kmain}` + `br x1` (间接) | `la t0, {kmain}` + `jalr x0, t0, 0` (间接) |
| 参数传递 | `rdi` (System V ABI) | `x0` (AAPCS64) | `a0` (RISC-V ABI) |
| 栈对齐 | 16 字节（`and rsp, -16` 单指令） | 16 字节（**4 指令**：SP 不能作 AND 目的，需 X2 中转） | 16 字节（`li t0, -16` + `and sp, sp, t0` 两指令） |
| 帧指针清零 | `xor rbp, rbp` (FP) | `mov x29, #0` (FP) | `li s0, 0` (s0-fp) |
| 跳转前屏障 | 无（Intel SDM 隐含） | `isb` | `fence.i` |
| 恒等映射范围 | 4GB (2MB huge pages) | 4GB (1GB block entries) | 4GB (1GB superpages) |
| 高地址基址 | `0xFFFF_8000_0000_0000` | `0xFFFF_8000_0000_0000` | `0xFFFF_FFC0_0000_0000` |
| 页表启用 | CR0.PG + CR3 | SCTLR.M + TTBR1 | satp.MODE + satp.PPN |

---

## 4. 实现详解

### 4.1 链接脚本 (link.ld)

> 设计决策：§3.1（不保留 AT()）、§3.2（独立 kernel ELF）

内核链接脚本需要满足：

1. VMA 从 `KERN_VIRT_BASE` 开始——所有符号解析为高地址
2. LMA 从 `KERN_PHYS_BASE` 开始——boot-shim 按此地址加载段
3. 导出 `kern_virt_base`、`kern_phys_base`、`kern_size` 符号（用 `PROVIDE`）——**但 boot-shim 实际上不读这些符号**：它从 ELF 的 PT_LOAD 段读取 `paddr`/`vaddr`/`memsz` 来构造 `KernelInfo`。`PROVIDE` 的符号是给 **kernel 自己** 用的（如测试代码用 `extern "C" { static kern_virt_base: u64; }` 直接读链接值），主要是**链接期排错**和**单测断言**。boot-shim 的 ELF 加载路径不依赖 `PROVIDE`。

**x86-64 链接脚本** (`os/kernel/src/arch/x86_64/link.ld`)：

```ld
OUTPUT_ARCH(i386:x86-64)

/* 内核虚拟基址：高半核起始地址 */
KERN_VIRT_BASE = 0xFFFF800000000000;
/* 内核物理基址：boot-shim 将内核 ELF 加载到此物理地址 */
KERN_PHYS_BASE = 0x00200000;    /* 2MB，典型物理加载地址 */

SECTIONS
{
    /* 链接器从虚拟基址开始分配 VMA */
    . = KERN_VIRT_BASE;

    /* 代码段 */
    .text : {
        *(.text .text.*)
    }

    /* 只读数据段 */
    .rodata : {
        *(.rodata .rodata.*)
    }

    /* 可读写数据段 */
    .data : {
        *(.data .data.*)
    }

    /* BSS 段（未初始化全局变量） */
    .bss : {
        __bss_start = .;
        *(.bss .bss.*)
        *(COMMON)
        __bss_end = .;
    }

    /* 导出符号供 KernelInfo 使用 */
    PROVIDE(kern_virt_base = KERN_VIRT_BASE);   /* 内核虚拟基址 */
    PROVIDE(kern_phys_base = KERN_PHYS_BASE);   /* 内核物理基址 */
    PROVIDE(kern_size = . - KERN_VIRT_BASE);    /* 内核镜像大小 */

    /* 初始内核栈（4 页 = 16KB）
       栈向低地址增长。链接脚本导出 k_initial_stktop 符号
       供 boot-shim 计算 KernelInfo.kern_stack_top 时参考，
       Rust 代码不再直接引用此符号（通过参数传入栈顶） */
    . = ALIGN(4096);
    . += 0x4000;
    PROVIDE(k_initial_stktop = .);

    /* 内核镜像结束地址 */
    _end = .;

    /* 丢弃不需要的段，减小镜像体积 */
    /DISCARD/ : {
        *(.eh_frame)      /* 异常处理帧（no_std 不需要） */
        *(.comment)       /* 编译器注释 */
    }
}
```

**关键设计**：没有 AT()。boot-shim 的 `load_segments_into_phys_memory()` 按 ELF 的 `p_paddr` 加载段。链接脚本只设置 VMA，LMA 由 boot-shim 在加载时计算。

**aarch64 链接脚本** (`os/kernel/src/arch/aarch64/link.ld`)：

与 x86-64 基本相同，仅修改：

```ld
OUTPUT_ARCH(aarch64)
KERN_VIRT_BASE = 0xFFFF800000000000;
KERN_PHYS_BASE = 0x40200000;    /* QEMU virt RAM start + 2MB (2MB-aligned for huge pages) */
```

**riscv64 链接脚本** (`os/kernel/src/arch/riscv64/link.ld`)：

```ld
OUTPUT_ARCH(riscv)
KERN_VIRT_BASE = 0xFFFFFFC000000000;  /* Sv39 canonical high, VPN[2]=256 */
KERN_PHYS_BASE = 0x80200000;         /* QEMU virt DRAM start */
```

riscv64 使用不同的高地址基址，因为 Sv39 的规范地址空间划分与 x86-64 不同：

- x86-64: canonical high = `0xFFFF_8000_0000_0000` ~ `0xFFFF_FFFF_FFFF_FFFF`
- riscv64 Sv39: canonical high = `0xFFFF_FFC0_0000_0000` ~ `0xFFFF_FFFF_FFFF_FFFF` (VPN[2] = 256~511)

> **注意**：`0xFFFF_C000_0000_0000` 和 `0xFFFF_FC00_0000_0000` 都不是有效的 Sv39 规范高地址！它们的 VPN[2] = 0，与低地址 identity mapping 冲突。Sv39 使用 39 位虚拟地址，规范高地址要求 bits[63:39] 全为 1，因此高半核的起始地址为 `0xFFFF_FFC0_0000_0000`（VPN[2] = 256）。

### 4.2 ELF 加载实现

> 01 文档聚焦 boot-shim 的引导准备流程（内存映射、KernelInfo 构造、ExitBootServices）；本节展开 ELF 解析与段拷贝的实现细节。
>
> boot-shim 的核心职责之一是加载内核 ELF。Minix3 C 中这一步由 GRUB 的 `multiboot` 命令代劳；minix-rs 中 boot-shim 需要自己解析 ELF、按 `p_paddr` 复制 PT_LOAD 段到物理内存、清零 BSS。

#### 4.2.1 FileLoader trait — 两端共享的抽象

UEFI 路径（x86_64/aarch64）和 OpenSBI 路径（riscv64）的 ELF 加载主流程完全共享，唯一差异是"如何把文件字节读到内存"。这一差异由 `FileLoader` trait 封装：

```rust
// os/boot-shim/src/loader.rs
pub trait FileLoader {
    fn read(&self, path: &str) -> Option<Vec<u8>>;
    fn read_required(&self, path: &str) -> Vec<u8> { /* default impl */ }
}

// 共享的内核加载逻辑——两端用同一份代码
pub fn load_kernel_with_loader<L: FileLoader>(loader: &L)
    -> Result<KernelLoadResult, minix_elf::ElfError> { /* ... */ }

pub fn load_boot_modules_with_loader<L: FileLoader>(
    loader: &L,
    alloc_pages: PageAllocator,
) -> &'static [BootModule] { /* ... */ }
```

UEFI 侧用 `SimpleFileSystem` 实现 trait；OpenSBI 侧用 `BootFileTable` 实现 trait——加载主流程完全共享：

```
┌─────────────────────┐         ┌─────────────────────────┐
│ UefiFileLoader      │         │ UbootFileLoader<'a>     │
│  (SimpleFileSystem) │         │  (&BootFileTable)       │
│ uefi_helpers.rs     │         │ opensbi_helpers.rs     │
└─────────┬───────────┘         └────────────┬────────────┘
          │  impl FileLoader                 │  impl FileLoader
          └────────────┬─────────────────────┘
                       ↓
        loader::load_kernel_with_loader()
        loader::load_boot_modules_with_loader()
                       ↓
                 (共享：minix_elf 解析 + 段拷贝 + BootModule 构造)
```

#### 4.2.2 为什么需要自己的 ELF 解析器

boot-shim 必须从 ESP 分区读取 kernel ELF 并将其 PT_LOAD 段搬运到正确的物理地址；kernel 启动后也需要解析 boot module（VM/PM 等）的 ELF 来加载用户态服务。这两个场景在 Minix3 C 中由 GRUB 代劳（GRUB 的 `multiboot` 命令解析 ELF、按 LMA 加载段），但在 minix-rs 中需要自己完成。

为什么不使用现有的 Rust ELF 库？

| 库 | 不适用的原因 |
|---|---|
| `xmas-elf` | 错误类型返回 `&str`，需要 `alloc` 或格式化支持 |
| `object` | 拉入 `std` 依赖，不兼容 `#![no_std]` |
| `elf` | 功能过多，minix-elf 只需要 PT_LOAD 段提取 |

minix-elf 的 ELF 需求极其单一：**读取 PT_LOAD 段，搬运到 p_paddr 指定的物理地址**。不需要符号解析、不需要重定位、不需要 section header。

**为什么是共享 crate 而非 boot-shim 内部模块？** 因为 boot-shim 和 kernel 都需要 ELF 解析：boot-shim 用它加载 kernel ELF，kernel 用它解析 boot module（VM/PM 等）的 ELF。放在 `os/libs/minix-elf` 下，两个 crate 都可依赖，避免代码重复。

#### 4.2.3 零分配设计

minix-elf 必须在 `#![no_std]` 环境下工作：boot-shim 运行在 UEFI 环境中（唯一可用的分配器是 `AllocatePages`，页粒度 4KB+），kernel 启动早期也没有堆。因此 ELF 解析器采用**迭代器模式**而非返回 `Vec`：

```rust
// 零分配：调用者逐段处理，无需收集到 Vec
pub fn segment_iter(image: &[u8]) -> Result<SegmentIter<'_>, ElfError> { ... }

// 使用方式
for seg in minix_elf::segment_iter(&image)? {
    // 1. 将 image[seg.offset..seg.offset+seg.filesz] 复制到 seg.paddr
    // 2. 将 [seg.paddr+seg.filesz, seg.paddr+seg.memsz) 清零（BSS）
}
```

`SegmentIter` 持有 `&[u8]` 引用和解析后的 `Elf64Ehdr`，每次 `next()` 调用解析一个 program header，跳过非 PT_LOAD 段，返回 `LoadSegment`。整个迭代过程零堆分配。

#### 4.2.4 核心数据结构

解析器定义三个结构体，分别对应 ELF 规范中的三个层次：

```rust
// ELF64 文件头（Ehdr）——只保留 minix-elf 需要的字段
pub struct Elf64Ehdr {
    pub e_type: u16,       // 必须是 ET_EXEC(2)
    pub e_machine: u16,    // 架构：EM_X86_64=62, EM_AARCH64=183, EM_RISCV=243
    pub e_entry: u64,      // 入口点虚拟地址
    pub e_phoff: u64,      // program header 表的文件偏移
    pub e_phentsize: u16,  // 每个 Phdr 的大小
    pub e_phnum: u16,      // Phdr 数量
}

// ELF64 程序头（Phdr）——只保留 minix-elf 需要的字段
pub struct Elf64Phdr {
    pub p_type: u32,    // 段类型（PT_LOAD=1 表示可加载段）
    pub p_flags: u32,   // 段权限（PF_R|PF_W|PF_X）
    pub p_offset: u64,  // 段数据在文件中的偏移
    pub p_vaddr: u64,   // 段的虚拟地址
    pub p_paddr: u64,   // 段的物理地址（调用者使用此字段）
    pub p_filesz: u64,  // 文件中的字节数
    pub p_memsz: u64,   // 内存中的字节数（memsz > filesz 时，多出部分为 BSS）
    pub p_align: u64,   // 对齐要求
}

// 可加载段——minix-elf 的最终输出
pub struct LoadSegment {
    pub paddr: u64,    // 目标物理地址
    pub vaddr: u64,    // 段的虚拟地址（内核页表建立 kern_virt_base → kern_phys_base 映射所需）
    pub offset: u64,   // 文件偏移
    pub filesz: u64,   // 需要从文件复制的字节数
    pub memsz: u64,    // 内存总大小（filesz + BSS）
    pub align: u64,    // 对齐
    pub flags: u32,    // 段权限（PF_R=4, PF_W=2, PF_X=1，用于设置页表项属性）
}
```

**为什么不直接用 `#[repr(C)]` 结构体 + `transmute`？** 因为 ELF 结构体有对齐和填充问题（C 编译器可能插入 padding），直接 transmute 字节切片不安全。解析器逐字段用 `from_le_bytes()` 读取，虽然代码稍长，但保证正确且无 UB。

#### 4.2.5 解析流程

```
image: &[u8]（整个 ELF 文件的字节）
    │
    ├─ parse_ehdr() ── 验证 magic(\x7fELF)、class(64-bit)、endianness(LE)、type(EXEC)
    │                   提取 e_phoff, e_phnum, e_phentsize, e_entry
    │
    └─ SegmentIter::new() ── 验证 e_phentsize >= 56、program header 表在 image 范围内
         │
         └─ next() 循环 ── 对每个 Phdr：
              ├─ p_type != PT_LOAD → 跳过
              ├─ p_offset + p_filesz > image.len() → 跳过（段数据越界）
              └─ 返回 LoadSegment { paddr, vaddr, offset, filesz, memsz, align, flags }
```

#### 4.2.6 错误处理

解析器使用 `ElfError` 枚举而非 `errno` 整数，每个变体对应一种具体的解析失败：

| 错误 | 含义 |
|------|------|
| `TooShort` | image 长度 < 64 字节（放不下 Ehdr） |
| `BadMagic` | 前 4 字节不是 `\x7fELF` |
| `Not64Bit` | EI_CLASS != ELFCLASS64 |
| `NotLittleEndian` | EI_DATA != ELFDATA2LSB |
| `NotExecutable` | e_type != ET_EXEC |
| `PhdrOutOfBounds` | program header 表超出 image 范围 |
| `PhdrEntryTooSmall` | e_phentsize < 56（ELF64 Phdr 最小大小） |
| `NoLoadSegments` | 未找到 PT_LOAD 段（仅 `load_segments()` 使用，迭代器不报此错） |

#### 4.2.7 与 Minix3 C 的对应关系

| Minix3 C | minix-rs | 说明 |
|----------|----------|------|
| GRUB `multiboot` 命令解析 ELF | `minix_elf::segment_iter()` | C 版由 GRUB 代劳（见 01 文档 §1.2），Rust 版自行解析 |
| GRUB 按 LMA 加载段到物理内存 | `LoadSegment.paddr` | C 版用 AT() 链接器技巧设 LMA，Rust 版直接读 p_paddr |
| `pre_init()` 遍历 `multiboot_info_t.mods_addr` | boot-shim 遍历 ESP 分区中的模块文件 | C 版模块由 GRUB `load_mods` 加载，Rust 版由 boot-shim 读取 |
| `protect.c` 解析 boot module 的 ELF | 同样使用 `minix_elf::segment_iter()` | 内核启动后解析 VM/PM 等模块的 ELF，复用同一解析器 |

#### 4.2.8 代码位置与测试

```
os/libs/minix-elf/src/lib.rs       — ELF 解析器实现（独立共享 crate，23 个测试）
os/boot-shim/src/uefi_helpers.rs   — compute_kernel_layout() + load_segments_into_buffer()（1 个测试）
os/boot-shim/src/lib.rs            — pub use minix_elf; re-export
```

**测试策略**：由于 UEFI 函数依赖 UEFI 运行时，无法在标准 `cargo test` 中直接测试。因此从 `load_kernel_elf` 中提取了两个纯逻辑函数：

- `compute_kernel_layout(elf_data)` — 解析 ELF 计算 kern_phys_base / kern_virt_base / kern_size / entry_point
- `load_segments_into_buffer(elf_data, buf, buf_base_paddr)` — 将 PT_LOAD 段拷贝到缓冲区并清零 BSS

这两个函数不依赖 UEFI 运行时，可以在标准测试环境中验证 ELF 解析、段拷贝、BSS 清零的正确性。`load_kernel_elf` 在生产代码中调用 `compute_kernel_layout` 获取布局，然后执行物理内存写入。

`loader::tests` 用 `MockLoader` 验证 `load_kernel_with_loader` / `load_boot_modules_with_loader` 主流程。共 26 个单元测试（`loader.rs` 12 个共享主流程 + `opensbi_helpers.rs` 13 个 OpenSBI 路径 + `uefi_helpers.rs` 1 个布局计算），在 `cargo test --features test-all` 下全部通过。**UEFI 路径测试覆盖不足（仅 1 个）** 是已知 gap——`load_kernel_elf` 涉及 UEFI `AllocatePages` / `SimpleFileSystem` 调用，无法在标准 `cargo test` 中覆盖，需要 QEMU 集成测试。

### 4.3 HigherHalf trait 与三架构实现

> 设计决策：§3.4（HigherHalf trait）

`HigherHalf` trait 抽象了分页启用后从低地址切换到高地址的跳转。trait 的优势：`arch_boot()` 可以统一调用 `HigherHalf::jump_to_kmain()`，无需关心具体架构的寄存器和指令差异。三个架构通过内联汇编实现切栈+跳转，替代了 C 中由独立汇编 trampoline 完成的职责。

**trait 定义** (`os/kernel/src/boot/higher_half.rs`)：

```rust
//! 高半核内核切换抽象。
//!
//! 分页启用后，CPU 仍在低地址执行（恒等映射）。
//! 本 trait 提供架构相关的机制，切换栈指针并跳转到
//! 内核的高虚拟地址入口点（kmain）。
//!
//! 对应 Minix3 head.S:78-87 (x86) / head.S:41-47 (ARM)。

use minix_boot::KernelInfo;

/// 高半核内核切换的抽象。
///
/// 每个架构实现此 trait，提供从恒等映射执行切换到
/// 内核高虚拟地址空间的底层机制。
///
/// # Safety 契约
///
/// `jump_to_kmain` 是 unsafe 的，因为：
/// - 必须在分页已启用且恒等映射+内核高映射都已建立后调用
/// - 启动期间只能调用一次
/// - 调用者必须保证 `kinfo` 在高地址可访问
pub trait HigherHalf {
    /// 执行高半核切换：切栈到高地址，然后跳转到 kmain。
    ///
    /// # 参数
    ///
    /// - `kinfo`: 指向 KernelInfo 的指针，作为 kmain 的第一个参数传入
    /// - `stack_top`: 内核栈顶的虚拟地址（高地址）
    ///
    /// # Safety
    ///
    /// 调用者必须保证：
    /// - 分页已启用，恒等映射和内核高映射都已建立
    /// - `kinfo` 指针在高地址有效且可访问
    /// - `stack_top` 是内核高半核映射中有效的、16 字节对齐的虚拟地址
    /// - 只从 boot CPU 调用一次
    unsafe fn jump_to_kmain(kinfo: &KernelInfo, stack_top: VirBytes) -> !;
}
```

三个架构的实现遵循同一模式：内联汇编执行切栈+跳转，通过 `options(noreturn)` 告诉编译器永不返回。下面以 x86-64 为完整示例，aarch64 / riscv64 仅列出与 x86-64 的差异点。

**x86-64 实现** (`os/kernel/src/arch/x86_64/higher_half.rs`)：

```rust
use crate::boot::higher_half::HigherHalf;
use minix_boot::KernelInfo;

/// x86-64 高半核切换实现。
///
/// `arch_boot_impl()` 返回 &KernelInfo，通过 RDI 接收它（System V ABI）。
///
/// 执行流：
///   boot-shim → arch_boot() → arch_boot_impl() → [返回]
///     → HigherHalf::jump_to_kmain() → kmain()
pub struct X86_64HigherHalf;

impl HigherHalf for X86_64HigherHalf {
    unsafe fn jump_to_kmain(kinfo: &KernelInfo, stack_top: VirBytes) -> ! {
        // SAFETY: 调用者保证分页已启用且两套映射都已建立。
        // stack_top 来自 KernelInfo.kern_stack_top，是高地址虚拟地址。
        unsafe {
            core::arch::asm!(
                // (1) 加载高地址栈顶到 RSP
                "mov rsp, {stktop}",
                // (2) 栈对齐到 16 字节
                "and rsp, -16",
                // (3) 清零帧指针，压入 NULL 返回地址
                "xor rbp, rbp",
                "push 0",
                // (4) 调用 kmain（RDI 已由 in("rdi") 约束传入 &KernelInfo）
                "call {kmain}",
                stktop = in(reg) stack_top.0,     // 运行时传入的栈顶虚拟地址
                kmain = sym kmain,                // kmain 符号地址
                in("rdi") kinfo,                  // System V ABI：第 1 参数通过 RDI
                options(noreturn)                 // 永不返回，编译器不生成返回路径
            );
        }
    }
}
```

**aarch64 差异点**（完整实现见 `os/kernel/src/arch/aarch64/higher_half.rs`）：
- 加载符号地址：`ldr x1, ={kmain}`（伪指令，由汇编器放入 literal pool，再通过 `br x1` 间接跳转）。`bl` 的立即数范围不足以覆盖 kmain 符号。
- 栈对齐需要 4 条指令：`mov x1, #-16` / `mov x2, sp` / `and x2, x2, x1` / `mov sp, x2`（aarch64 禁止 SP 作为 AND 目的寄存器，必须用临时寄存器 x2 中转）。
- 参数寄存器：`in("x0")`（AAPCS64）。

**riscv64 差异点**（完整实现见 `os/kernel/src/arch/riscv64/higher_half.rs`）：
- 加载符号地址：`la t0, {kmain}`（伪指令，展开为 `auipc + addi`，计算 PC-relative 地址）。
- 跳转用 `jalr x0, t0, 0`，`rd=x0` 显式丢弃返回地址到 x0（避免 x1/ra 被覆盖，与 x86 的 `push 0` 等效）。
- 参数寄存器：`in("a0")`（RISC-V ABI）。
- 帧指针寄存器：s0（不是 x29 / rbp）。
- **SBI ecall 安全约束**：a0 持有 kinfo 指针（通过 `in("a0")` 约束传入），必须在到达 kmain 前保持不变。asm 块内禁止使用 a0/a7/a6——SBI ecall（通过 a7 选 SBI 扩展号、a0 传参/返参）会 clobber a0，破坏 kinfo。当前 asm 块用 t0 作为加载 kmain 地址的临时寄存器，避免触碰 a0；进入 kmain 后 a0 才被消费。

**三架构差异总结**：

| 差异维度 | x86-64 | aarch64 | riscv64 |
|---------|--------|---------|---------|
| 切栈指令 | `mov rsp, {stktop}` | `mov sp, {stktop}` | `mv sp, {stktop}` |
| 栈对齐 | `and rsp, -16` | `mov x1, #-16` + `and sp, sp, x1` | `li t0, -16` + `and sp, sp, t0` |
| 清零帧指针 | `xor rbp, rbp` | `mov x29, #0` | `li s0, 0` |
| 跳转指令 | `call {kmain}` | `ldr x1, ={kmain}` + `br x1` | `la t0, {kmain}` + `jalr x0, t0, 0` |
| 参数寄存器 | `in("rdi")` | `in("x0")` | `in("a0")` |
| 核心差异 | `call` 立即数足够大 | `bl` 范围不足 → 寄存器间接 | `jalr rd=x0` 显式丢弃返回地址 |

三者的**共同模式**不变：加载高地址栈顶 → 栈对齐 → 清零帧指针 → 跳转到 kmain。差异仅在于各架构的汇编指令和调用约定，这正是 `HigherHalf` trait 存在的意义——将架构差异封装在实现中，上层代码统一调用。

### 4.4 boot-shim 到 kmain 的完整跳转

> 设计决策：§3.1（不保留 AT()）、§3.2（独立 kernel ELF）

boot-shim 的 `main.rs` 完成以下步骤后，将控制权交给内核：

1. `load_kernel_with_loader()` — 解析内核 ELF，按 `p_paddr` 复制段到物理内存
2. `build KernelInfo` — 收集内存映射、固件信息、模块列表
3. `ExitBootServices()` — 退出 UEFI/OpenSBI 固件服务
4. `arch_boot(&kinfo, root_page)` — **进入内核**

**`arch_boot_impl` 的完整实现** (`os/kernel/src/lib.rs`)：

```rust
pub fn arch_boot_impl<P: HugePages>(kernel_info: &KernelInfo, root_page: PhysBytes) -> &KernelInfo {
    // Step 0: 验证 KernelInfo 约束，注册 boot_pt_alloc 分配器，选择 huge page size
    let kern_huge = boot_validate_and_prepare::<P>(kernel_info);

    let mut paging = P::new_from_page(root_page);

    // Step 1: 恒等映射 — VA=PA 前 4GB (C: pg_identity — pg_utils.c:162)
    let id_flags = PageFlags::kernel_read_write() | PageFlags::EXECUTABLE;
    let mut addr: u64 = 0;
    while addr < 0x1_0000_0000 {
        let _ = paging.map_huge(VirBytes(addr), PhysBytes(addr),
                        kern_huge as usize, id_flags);
        addr += kern_huge;
    }

    // Step 2: 内核高半核映射 (C: pg_mapkernel — pg_utils.c:186)
    let kern_flags = PageFlags::kernel_read_write() | PageFlags::EXECUTABLE;
    // supervisor-only mapping 注释:
    //   `kernel_read_write()` **不含** `USER_ACCESSIBLE`，所以三架构 PTE 都设了
    //   supervisor-only 标记：x86-64 U/S=0；aarch64 AP1=0 (EL1 only)；riscv64 U=0。
    //   这意味着 RISC-V 上即使 sstatus.SUM=0，内核仍可正常访问用户内存 —
    //   因为 S-mode 永远读 supervisor-only mapping，不需要 SUM。
    //   **总结**: 我们用 supervisor-only mapping，不设 USER_ACCESSIBLE，sstatus.SUM
    //   是否为 0 都不影响当前阶段。SUM 只在"内核要读 U=1 页面"时才有意义。
    let kern_virt = kernel_info.kern_virt_base.0;
    let kern_phys = kernel_info.kern_phys_base.0;
    // 当 kern_virt == kern_phys 时，Step 2 与 Step 1 完全重叠，
    // 跳过以避免覆盖 L2 条目（详见附录 A）
    if kern_virt != kern_phys {
        let mut offset = 0u64;
        while offset < kernel_info.kern_size {
            paging.map_huge(
                VirBytes(kern_virt + offset),
                PhysBytes(kern_phys + offset),
                kern_huge as usize, kern_flags).expect("kernel map failed");
            offset += kern_huge;
        }
    }

    // Step 3: 开分页 (C: pg_load + vm_enable_paging)
    unsafe { paging.enable() };

    kernel_info
}
```

> **Step 0→3 的顺序不可调换**：先注册分配器（Step 0），否则 `map_huge` 分配中间页表页会 panic；先建映射（Step 1+2），否则切换页表后内核找不到自己；最后切换页表（Step 3），切换后脚手架生效。

> **`kern_virt != kern_phys` 守卫（防御性编程）**：当 `kern_virt_base == kern_phys_base` 时，Step 2 的映射目标与 Step 1 完全重叠，**跳过 Step 2 是正确的**——identity mapping 已覆盖相同范围且权限相同，跳过不丢映射。当 `kern_virt_base` 与 `kern_phys_base` 不同时（生产场景），Step 2 仍照常执行。
>
> 守卫覆盖两类历史 bug：①identity-only 测试场景（hello-boot / test-paging-enable / test-kernel-map 三架构共用）；②riscv64 Sv39 canonical 地址错误（test-higher-half-riscv64 最初用 `0xFFFF_FC00_0000_0000` 导致 L2 槽位冲突）。两类场景均通过守卫得到合理默认。详见 [附录 A](02-higher-half-kernel.md#附录-ariscv64-identity-mapping-与-kernel-mapping-的-l2-条目覆盖)。

> **与 C 源码的差异**：C 的 `pg_mapkernel()` 只设 `PRESENT | BIGPAGE | WRITE`，无 GLOBAL 位。Rust 代码中 `PageFlags::kernel_read_write()` 含 GLOBAL，boot 阶段无实际作用（无进程切换，CR3 不变）。GLOBAL 位的真正价值在 VM 的 Direct Map 中——每次进程切换重写 CR3 时避免内核映射 TLB miss。

**`arch_boot()` 的实际代码** (`os/kernel/src/lib.rs:80-132`，三架构版本 + mock 测试入口)：

```rust
// x86-64
#[cfg(all(not(feature = "mock"), target_arch = "x86_64"))]
pub fn arch_boot(kernel_info: &KernelInfo, root_page: PhysBytes) -> ! {
    use minix_arch::x86_64::paging::X86_64Paging;
    use crate::x86_64::higher_half::X86_64HigherHalf;
    use crate::boot::HigherHalf;
    let info = arch_boot_impl::<X86_64Paging>(kernel_info, root_page);
    // SAFETY: arch_boot_impl 刚启用分页，两套映射都已建立，
    //         info 在高地址可访问，且只从 boot CPU 调用一次。
    unsafe { X86_64HigherHalf::jump_to_kmain(info, info.kern_stack_top) }
}

// aarch64 / riscv64 / mock：重复同样的 3 行结构，仅类型别名不同
// （`AArch64Paging` + `AArch64HigherHalf`、`Riscv64Paging` + `Riscv64HigherHalf`、
//  `MockPaging` + `mock_kmain_ok()`）
```

**两步骤为何缺一不可**：

- **Step 1（`arch_boot_impl` 返回时）**：
    - 恒等映射已建立（0~4GB，VA=PA）
    - 内核高地址映射已建立（KERN_VIRT_BASE → 物理内存）
    - CR3/satp/TTBR1 已加载，分页已启用
    - **但 RSP 仍在低地址**（boot-shim 的栈）

- **Step 2（`HigherHalf::jump_to_kmain`）**：
    - 必须切栈到高地址，再调用 kmain
    - 如果直接 `call kmain`，返回地址会被压入低地址栈
    - 后续中断/异常需要栈操作时，若恒等映射已移除就会 crash

这两步的语义边界在 §4.6 前文 "Step 0→3 的顺序不可调换" 注释块已详述：`arch_boot_impl` 是"启用分页 + 建映射"，`HigherHalf::jump_to_kmain` 是"切栈到高地址 + 转移控制权"。前者是分页机制，后者是栈控制权交接——必须分两步执行，不能合并。

**为什么有 4 份 `#[cfg]` 重复**：当前 `os/arch/src/arch/arch_boot.rs:60` 的 `ArchBoot` trait 仅覆盖 timer handler 注册，**未包含** `Paging` 与 `HigherHalf` 类型。`arch_boot` 函数的统一形式本可以是：

```rust
pub fn arch_boot(kernel_info: &KernelInfo, root_page: PhysBytes) -> ! {
    let info = arch_boot_impl::<CurrentArchPaging>(kernel_info, root_page);
    unsafe { CurrentArchHigherHalf::jump_to_kmain(info, info.kern_stack_top) }
}
```

通过关联类型 `type Paging; type HigherHalf;` 在 `ArchBoot`（或独立的 `ArchBootFlow`）trait 下编译期选择。但当前 4 份 `#[cfg]` 重复反映 trait 设计不完整，与 `CurrentArchBoot`、`CurrentArchInit` 类型别名的成熟模式不一致。

> **当前状态与演进方向**：`ArchBoot` trait（`os/arch/src/arch/arch_boot.rs:60`）目前仅覆盖 timer handler 注册，未包含 `Paging` 与 `HigherHalf` 关联类型。后续重构可参考 [01-boot-shim-bootstrap.md §4.4 BootShim trait](01-boot-shim-bootstrap.md#44-引导协议抽象--bootshim-trait) 的关联类型模式，将 `Paging` + `HigherHalf` 接入 trait 体系，消除 4 份 `#[cfg]` 重复。当前 §4.4 代码块保留 4 份展示，反映真实工程现状；trait 重构后将本节改写为单份 `arch_boot` 实现。

> **三架构 `arch_boot()` 差异**：aarch64/riscv64 版本的 `arch_boot()` 与 x86-64 几乎完全相同，**唯一差异是 Paging 类型别名**：
> - x86-64: `minix_arch::x86_64::paging::X86_64Paging` (PML4, 4 级)
> - aarch64: `minix_arch::arm64::paging::AArch64Paging` (TTBR1, 4 级 L0→L3)
> - riscv64: `minix_arch::riscv64::paging::Riscv64Paging` (Sv39, 3 级)
>
> 其余步骤（`arch_boot_impl::<P>` + `HigherHalf::jump_to_kmain`）完全相同——这是 `P: HugePages` 泛型设计的目标。详见 [os/kernel/src/lib.rs:80-132](os/kernel/src/lib.rs)。

**为什么必须 `jump_to_kmain` 而不是直接 `kmain(info)`？**

| 操作 | 直接 `kmain(info)` | `jump_to_kmain(info, info.kern_stack_top)` |
|------|-------------------|------------------------------------------|
| 指令地址 | `kmain` 符号 = 高地址 ✓ | `kmain` 符号 = 高地址 ✓ |
| 栈地址 | RSP = 低地址（boot-shim 栈）✗ | RSP = `info.kern_stack_top`（高地址）✓ |
| 返回地址 | 压入低地址栈 ✗ | 压入高地址栈 ✓ |
| 中断安全 | 恒等映射移除后中断会 crash | 栈在高地址，始终安全 ✓ |

直接调用 `kmain` 的问题不是**指令地址**（编译器会自动用高地址），而是**栈地址**。`arch_boot()` 的栈是 boot-shim 分配的，位于低物理地址。`jump_to_kmain()` 的职责就是：**在调用 `kmain` 之前，把 RSP 切换到内核自己的高地址栈**。

**执行流程**：

```
boot-shim (低地址执行)
  │
  ├── ExitBootServices()
  │
  └── arch_boot(&kinfo, root_page)  ← RSP = boot-shim 栈（低地址）
        │
        ├── arch_boot_impl()         ← 仍在低地址执行
        │     ├── 清零页表
        │     ├── 恒等映射 0~4GB
        │     ├── 内核高地址映射
        │     ├── 加载 CR3（启用分页）
        │     └── 返回 &KernelInfo
        │
        └── X86_64HigherHalf::jump_to_kmain(info, info.kern_stack_top)
              │
              ├── mov rsp, stack_top                 ← 切栈到高地址（参数传入）
              ├── and rsp, -16                       ← 栈对齐
              ├── xor rbp, rbp                       ← 清零帧指针
              ├── push 0                             ← NULL 返回地址
              └── call kmain                         ← 跳转到高地址
                                                    ← RIP = 高地址, RSP = 高地址
```

**关键点**：`jump_to_kmain()` 是一个 `-> !` 函数（永不返回）。它不只是"调用 kmain"，而是**接管 CPU 的执行上下文**——切栈、对齐、清零帧指针、然后跳转。从 `kmain` 开始，内核完全运行在高地址空间。

### 4.5 完整启动流程图

```
boot-shim (UEFI/OpenSBI，低地址执行)
  │
  ├── load_kernel_with_loader()  ← 按 p_paddr 复制段到物理内存
  ├── load_boot_modules()        ← 加载 VM/PM/VFS 等模块
  ├── build KernelInfo           ← 收集内存映射、固件信息
  ├── ExitBootServices()         ← 退出固件服务
  │
  └── arch_boot(&kinfo, root_page)  ← 进入内核（低地址）
        │
        ├── arch_boot_impl::<X86_64Paging>()
        │     ├── 清零根页表
        │     ├── 恒等映射 0~4GB
        │     ├── 内核高地址映射
        │     ├── 加载 CR3（启用分页）
        │     └── 返回 &KernelInfo
        │
        └── X86_64HigherHalf::jump_to_kmain(info, info.kern_stack_top)  ← 切栈 + 跳转
              │
              ├── RSP → info.kern_stack_top（高地址，参数传入）
              ├── 栈对齐、清零帧指针
              └── call kmain  ← 高地址执行
                    │
                    ├── cstart()     ← 详见 03-kmain-cstart.md
                    ├── proc_init()  ← 详见 06-proc-init-boot-proc.md
                    └── switch_to_user()  ← 详见 07-kmain-entry-protection.md
```

> **为什么这一切能工作？** 链接器为 `kmain` 分配 VMA（高地址），boot-shim 按 p_paddr 将代码放在 LMA（低物理地址），页表建立两者之间的映射。CPU 通过高地址窗口访问同一块物理内存——"逻辑 vs 物理"的分离是所有现代 OS 的基石，而高半核内核正是这种分离最极致的体现。详见 02-stage-vm/06-pagetable-struct.md §1.1。

---

## 5. 测试要点

### 5.1 QEMU 集成测试：test-higher-half（三架构 3/3 通过）

高半核切换是整个启动链中最脆弱的环节——栈指针、PC、帧指针任何一个错误都会导致 CPU 立即异常。QEMU 集成测试通过在真实硬件模型上运行测试内核，验证每一步的正确性。

> **测试归属**：`hello-boot` / `test-memmap` / `test-paging-enable` / `test-kernel-map` 四类测试属于 boot-shim 后端验证，详见 [01-boot-shim-bootstrap.md §5.2](01-boot-shim-bootstrap.md#52-集成测试qemu-tests)。本节仅覆盖 02 专属的 `test-higher-half` 测试。合计三架构 15/15 通过（01 的 12/12 + 02 的 3/3）。

**test-higher-half 测试内核**验证高半核切换后的寄存器状态：

```
arch_boot_impl → HigherHalf::jump_to_kmain → kmain (naked) → kmain_verify
```

`kmain` 使用 `#[naked]` 属性避免函数序言修改栈指针，直接捕获入口时的 SP/PC/FP 寄存器值。`kmain` 与 `kmain_verify` 位于内核 `os/kernel/src/lib.rs:519-643`（`#[cfg(feature = "qemu_test")]`）——测试内核 `main.rs` 仅调用 `arch_boot`，跳转后的验证由内核侧完成。`kmain_verify` 断言：

1. **SP >= kern_virt_base**：栈指针在高地址空间
2. **SP 16 字节对齐**：满足 ABI 要求
3. **FP == 0**：帧指针被清零（栈回溯终止标记）

**三架构 QEMU 测试结果**：

| 测试 | x86_64 | aarch64 | riscv64 |
|------|--------|---------|---------|
| test-higher-half | PASS | PASS | PASS |

> **riscv64 test-higher-half 说明**：riscv64 的 test-higher-half 使用 `kern_virt_base = 0xFFFF_FFC0_0000_0000`（Sv39 canonical high, VPN[2]=256），与 x86_64/aarch64 一样验证 SP 在高地址。PC 仍在低地址（QEMU `-kernel` 加载到 0x8020_0000，identity mapping 保持可访问），这与 x86_64/aarch64 的测试行为一致——三架构的 test-higher-half 都只验证 SP 和 FP，不验证 PC 跳转到高地址。真正的 PC 高地址跳转需要 boot-shim 加载 ELF 到高 VMA 后才能验证。

**运行方式**：`cd os/qemu-tests && bash run_all.sh`

**test-higher-half 三架构实现差异**：

| 差异 | x86_64 | aarch64 | riscv64 |
|------|--------|---------|---------|
| kmain 入口捕获 | `mov rdx, rsp; lea r8, [rip]; mov r9, rbp` | `mov x1, sp; adr x2, 0; mov x3, fp` | `mv a1, sp; auipc a2, 0; mv a3, fp` |
| 调用约定 | Windows x64 ABI (RCX/RDX/R8/R9) | AAPCS64 (x0-x3) | RISC-V (a0-a3) |
| kern_virt_base | 0xFFFF_8000_0000_0000 | 0xFFFF_8000_0000_0000 | 0xFFFF_FFC0_0000_0000 |
| SP 断言 | SP >= 0xFFFF_8000_0000_0000 | SP >= 0xFFFF_8000_0000_0000 | SP >= 0xFFFF_FFC0_0000_0000 |

### 5.2 QEMU + GDB 手动验证

最直接的验证方式是在 QEMU 中运行内核，用 GDB 检查 RIP/PC 是否在高地址：

```bash
# x86-64
qemu-system-x86_64 -kernel kernel.elf -s -S
gdb kernel.elf
(gdb) break kmain
(gdb) continue
(gdb) print $rip    # 应显示 0xFFFF_8000_xxx_xxx

# aarch64
qemu-system-aarch64 -machine virt -kernel kernel.elf -s -S
gdb kernel.elf
(gdb) break kmain
(gdb) continue
(gdb) print $pc     # 应显示 0xFFFF_8000_xxx_xxx

# riscv64
qemu-system-riscv64 -machine virt -kernel kernel.elf -s -S
gdb kernel.elf
(gdb) break kmain
(gdb) continue
(gdb) print $pc     # 应显示 0xFFFF_FFC0_xxx_xxx
```

### 5.3 单元测试（`cargo test -p minix-kernel --lib`，610 个通过，截至 2026-08-14）

| 测试 | 验证内容 |
|------|---------|
| `test_higher_half_trait_is_implementable` | HigherHalf trait 可被 mock 类型实现（类型系统检查） |
| `test_arch_boot_impl_enables_paging` | arch_boot_impl 调用 MockPaging 完成映射+enable 后返回正确 KernelInfo |
| `test_kernel_info_x86_64_alignment` | x86_64 kern_phys_base 2MB 对齐、kern_virt_base 1GB 对齐 |
| `test_kernel_info_aarch64_phys_in_ram` | aarch64 kern_phys_base 在 QEMU virt RAM 范围内 |
| `test_kernel_info_riscv64_identity` | riscv64 kern_virt_base == kern_phys_base（Sv39 identity mapping） |
| `test_arch_boot_impl_aarch64_params` | arch_boot_impl 使用 aarch64 KernelInfo 参数正常完成 |
| `test_arch_boot_impl_riscv64_params` | arch_boot_impl 使用 riscv64 KernelInfo 参数正常完成 |
| `test_boot_flow_identity_and_kernel_map` | boot 流程同时建立 identity mapping + kernel high mapping |
| `test_boot_empty_memmap` | 空 memmap 时 boot 流程的边界行为 |
| `test_linker_script_x86_64_constraints` | x86_64 link.ld 满足 arch_boot_impl 约束（VMA/LMA/对齐） |
| `test_linker_script_aarch64_constraints` | aarch64 link.ld 满足 arch_boot_impl 约束 |
| `test_linker_script_riscv64_constraints` | riscv64 link.ld 满足 arch_boot_impl 约束（Sv39 canonical high） |
| `test_arch_boot_rejects_misaligned_phys` | arch_boot 拒绝非页对齐的 kern_phys_base |
| `test_arch_boot_rejects_zero_kern_size` | arch_boot 拒绝 kern_size = 0 |
| `test_arch_boot_rejects_misaligned_stack_top` | arch_boot 拒绝非 16 字节对齐的 kern_stack_top |

**架构特定 PTE 转换测试**：

| 测试 | 验证内容 |
|------|---------|
| aarch64: `test_flags_to_pte_kernel_read_write` | AP2=0 (writable), AP1=0 (EL1 only), nG=0 (global), XN=1 (non-exec) |
| aarch64: `test_flags_to_pte_kernel_read_write_exec` | AP2=0, XN=0 (executable) |
| aarch64: `test_flags_to_pte_user_accessible` | AP1=1 (EL0+EL1) |
| aarch64: `test_flags_to_pte_read_only` | AP2=1 (read-only) |
| riscv64: `test_paddr_to_pte_roundtrip` | paddr → PTE → paddr 往返转换正确 |
| riscv64: `test_flags_to_pte_kernel_read_write_exec` | V=1, R=1, W=1, X=1, A=1, D=1, G=1 |
| riscv64: `test_flags_to_pte_no_wx_combination` | W=1 时 R=1（Sv39 禁止 W=1,R=0） |
| riscv64: `test_l2_index_dram_base` | DRAM_BASE (0x8000_0000) 的 L2 index = 2 |
| riscv64: `test_paddr_to_pte_dram_base` | paddr_to_pte(0x8000_0000) = 0x2000_0000 |

### 5.4 集成测试（boot_integration.rs）

`cargo test -p minix-kernel --test boot_integration` 运行完整的 boot 模拟：

1. 创建 MockPaging 页表
2. Identity mapping（模拟 `pg_identity`）
3. Kernel high-address mapping（模拟 `pg_mapkernel`）
4. Enable paging（模拟 `pg_load + vm_enable_paging`）
5. 打印诊断信息

---

## 6. 过渡：从 HigherHalf 切换到 kmain

02 文档结束时，CPU 已在高地址执行 `kmain()`。此时系统状态：

| 状态 | 值 |
|------|-----|
| RIP/PC | 内核高地址（`kmain` 入口） |
| RSP/SP | 内核启动栈顶（Rust: `info.kern_stack_top`；Minix3 C 对应 `k_boot_stktop` 链接符号） |
| 分页 | 已启用，恒等映射 + 高地址映射并存 |
| 保护结构 | **未初始化** — 使用 GRUB/UEFI 留下的描述符表 |
| 中断 | **未初始化** — 任何异常都会 triple fault |

`kmain()` 的第一个任务是调用 `cstart()`，而 `cstart()` 的第一个调用是 `prot_init()`——建立内核自己的保护结构（GDT/IDT/TSS 或 VBAR_EL1/stvec）。这是 03 文档的内容。

**→ 下一篇**: [03-kmain-cstart.md](03-kmain-cstart.md) — kmain 入口与保护模式初始化

---

## 附录 A：riscv64 Identity Mapping 与 Kernel Mapping 的 L2 条目覆盖

> **分类**: Bug 分析
> **影响架构**: riscv64（Sv39 分页）
> **根因**: `arch_boot_impl` Step 2 对 `kern_virt_base == kern_phys_base` 场景缺乏保护
> **修复**: 当 `kern_virt == kern_phys` 时跳过 Step 2

### A.1 Bug 现象

riscv64 的三个 QEMU 测试（hello-boot、test-paging-enable、test-kernel-map）在启动后无限循环打印启动消息，最终 PANIC。test-higher-half-riscv64 则在 `arch_boot_impl` 后崩溃重启。

**串口输出**（hello-boot-riscv64 为例）：

```
### Booting Minix-RS hello-boot (riscv64, Paging trait)...
### Booting Minix-RS hello-boot (riscv64, Paging trait)...
### Booting Minix-RS hello-boot (riscv64, Paging trait)...
...（无限重复）
### PANIC ###
```

### A.2 根因分析

#### A.2.1 Sv39 地址空间结构

RISC-V 64 位使用 Sv39 分页方案，虚拟地址为 39 位，结构如下：

```
 63        39 38   30 29   21 20   12 11        0
┌───────────┬───────┬───────┬───────┬───────────┐
│  扩展位   │ VPN[2]│ VPN[1]│ VPN[0]│ 页内偏移  │
│ (必须全同) │ 9位   │ 9位   │ 9位   │ 12位      │
└───────────┴───────┴───────┴───────┴───────────┘
```

**关键规则**：bits[63:39] 必须全为 0 或全为 1（规范地址要求）。

- **低地址区域**：`0x0000_0000_0000_0000` ~ `0x0000_003F_FFFF_FFFF`，VPN[2] = 0~255
- **高地址区域**：`0xFFFF_FFC0_0000_0000` ~ `0xFFFF_FFFF_FFFF_FFFF`，VPN[2] = 256~511

> 简言之：Sv39 把 512GB 的虚拟地址空间切成两半——低 256GB（VPN[2]=0~255）和高 256GB（VPN[2]=256~511）。这个看似对称的分割是理解后续 bug 的关键：内核映射如果选错了"半区"，就会和恒等映射踩到同一个 L2 槽位。

Sv39 的页表是三级结构：

```
L2（根页表，512 项）→ L1（中间页表，512 项）→ L0（叶页表，512 项）
```

L2 的每一项覆盖 1GB 地址空间（2^30 字节）。L2 索引由 VPN[2] 决定：

```
l2_index(vaddr) = (vaddr >> 30) & 0x1FF
```

#### A.2.2 arch_boot_impl 的三步操作

`arch_boot_impl` 在启用分页前执行三步：

| 步骤 | 操作 | riscv64 实现 |
|------|------|-------------|
| Step 1 | 恒等映射（VA = PA） | L2[0]~L2[3] 写入 1GB superpage 条目，覆盖 0~4GB |
| Step 2 | 内核高地址映射 | L2[256] 写入指向 L1 页表的指针，L1 中映射 2MB huge page |
| Step 3 | 启用分页 | 写入 satp 寄存器 |

#### A.2.3 Bug 的触发条件

Bug 发生在 **Step 2 的目标地址与 Step 1 的恒等映射重叠** 时。具体有两种触发场景：

**场景 A：kern_virt_base == kern_phys_base（identity-only 测试）**

hello-boot、test-paging-enable、test-kernel-map 这三个测试使用：

```rust
kern_virt_base: VirBytes(0x8000_0000),  // DRAM_BASE
kern_phys_base: PhysBytes(0x8000_0000), // DRAM_BASE
```

执行流程：

1. **Step 1**：恒等映射，`map_huge(VA=0x0, PA=0x0, size=1GB)` × 4
   - `l2_index(0x8000_0000) = (0x8000_0000 >> 30) & 0x1FF = 2`
   - L2[2] = 1GB superpage 条目（`0x8000_0000 | PTE_R | PTE_W | PTE_X | PTE_V`）

2. **Step 2**：内核映射，`kern_virt = kern_phys = 0x8000_0000`
   - `kern_virt != kern_phys` 为 **false**，但原代码没有这个守卫
   - `map_huge(VA=0x8000_0000, PA=0x8000_0000, size=2MB)`
   - `l2_index(0x8000_0000) = 2` — **与 Step 1 使用相同的 L2 槽位！**
   - L2[2] 原来是 1GB superpage，现在被覆盖为 L1 页表指针
   - L1 页表是新分配的空页，只有 L1[0] 有映射
   - **L2[2] 的 1GB 覆盖范围被破坏**：原来 0x8000_0000~0xBFFF_FFFF 全部可访问，现在只有 0x8000_0000~0x801F_FFFF 可访问

3. **Step 3**：启用分页，CPU 切换到新页表
   - PC 在 0x8020_xxxx（OpenSBI 加载内核到此地址）
   - L2[2] 已被覆盖为 L1 页表指针
   - L1[0] 映射 0x8000_0000~0x801F_FFFF（2MB）
   - L1[1] 映射 0x8020_0000~0x803F_FFFF（2MB）
   - 但如果代码访问 0x8040_0000 之后的地址 → **页错误**
   - 更严重的情况：L1 页表分配的物理页可能被覆盖，导致整个映射崩溃

**场景 B：kern_virt_base 使用错误的 Sv39 地址**

test-higher-half-riscv64 最初使用：

```rust
kern_virt_base: VirBytes(0xFFFF_FC00_0000_0000)
```

这个地址看起来像是 Sv39 高地址，但实际计算 VPN[2]：

```
l2_index(0xFFFF_FC00_0000_0000) = (0xFFFF_FC00_0000_0000 >> 30) & 0x1FF
```

关键：`0xFFFF_FC00_0000_0000 >> 30 = 0x3FFFF_F000`，`& 0x1FF = 0`。

**VPN[2] = 0！** 这与低地址的 identity mapping 使用相同的 L2 槽位。

执行流程：

1. **Step 1**：恒等映射，L2[0]~L2[3] 写入 1GB superpage
2. **Step 2**：内核映射，`map_huge(VA=0xFFFF_FC00_0000_0000, PA=0x8000_0000, size=2MB)`
   - `l2_index(0xFFFF_FC00_0000_0000) = 0`
   - **L2[0] 被覆盖！** 原来 L2[0] 映射 0x0~0x3FFF_FFFF（1GB superpage）
   - 现在 L2[0] 变成 L1 页表指针，只映射 0xFFFF_FC00_0000_0000 附近的 2MB
3. **Step 3**：启用分页
   - 低地址 0x0~0x3FFF_FFFF 的映射丢失
   - CPU 在低地址执行，但低地址映射已被破坏 → **立即崩溃**

> 两个场景本质相同：Step 2 覆盖了 Step 1 的 leaf 条目，只是触发条件不同——场景 A 是"同地址不同粒度"（1GB superpage vs 2MB huge page），场景 B 是"不同地址同槽位"（规范地址错误导致 VPN[2] 冲突）。

#### A.2.4 为什么 0xFFFF_FFC0_0000_0000 是正确的

```
l2_index(0xFFFF_FFC0_0000_0000) = (0xFFFF_FFC0_0000_0000 >> 30) & 0x1FF
```

`0xFFFF_FFC0_0000_0000 >> 30 = 0x3FFFFFF0`，`& 0x1FF = 0x100 = 256`。

**VPN[2] = 256**，与 identity mapping 的 L2[0]~L2[3] 不冲突。

#### A.2.5 地址计算详解

理解这个 bug 的关键在于 Sv39 的规范地址要求和 VPN[2] 计算：

| 地址 | 二进制 [63:38] | VPN[2] | 规范? | 区域 |
|------|---------------|--------|-------|------|
| `0x0000_0000_0000_0000` | 全0 | 0 | 是 | 低地址 |
| `0x0000_003F_FFFF_FFFF` | 全0 | 255 | 是 | 低地址末尾 |
| `0xFFFF_C000_0000_0000` | **不全1** | **0** | **否** | **非法** |
| `0xFFFF_FC00_0000_0000` | **不全1** | **0** | **否** | **非法** |
| `0xFFFF_FFC0_0000_0000` | 全1 | **256** | 是 | 高地址起始 |
| `0xFFFF_FFFF_FFFF_FFFF` | 全1 | 511 | 是 | 高地址末尾 |

> 上表揭示了 bug 的深层原因：直觉上的"高地址"（如 `0xFFFF_FC00_0000_0000`）在 Sv39 中其实是非法的——它的 VPN[2]=0，落在低地址区。只有 bits[63:39] 全为 1 的地址才真正属于高地址区。这个细节被 x86-64 的经验误导了：x86-64 的 canonical high 从 `0xFFFF_8000_0000_0000` 开始，而 Sv39 要高得多（`0xFFFF_FFC0_0000_0000`）。

**为什么 `0xFFFF_FC00_0000_0000` 不是规范高地址？**

Sv39 规定：bits[63:39] 必须全为 0（低地址）或全为 1（高地址）。

```
0xFFFF_FC00_0000_0000 = 1111...11 1111 1100 0000 ... 0000
                                    ^^^^
                                    bit 38 = 0, bit 39 = 0, bit 40 = 1
                                    → bits[63:39] 不全为 1
```

而：

```
0xFFFF_FFC0_0000_0000 = 1111...11 1111 1111 1100 0000 ... 0000
                                    ^^^^
                                    bit 38 = 1, bit 39 = 1, bit 40 = 1
                                    → bits[63:39] 全为 1 ✓
```

### A.3 修复方案

#### A.3.1 修复 1：跳过冗余的 kernel mapping

当 `kern_virt_base == kern_phys_base` 时，Step 2 的映射与 Step 1 完全重叠，跳过即可：

```rust
let kern_virt = kernel_info.kern_virt_base.0;
let kern_phys = kernel_info.kern_phys_base.0;
if kern_virt != kern_phys {
    // Step 2: kernel high-address mapping
    // ...
}
```

**为什么安全**：identity mapping 已经覆盖了 `kern_phys_base ~ kern_phys_base + kern_size` 的范围，且权限（RWX）与 kernel mapping 相同。跳过 Step 2 不会丢失任何映射。

#### A.3.2 修复 2：map_huge 增加 leaf 条目检测

即使有了 `kern_virt != kern_phys` 守卫，`map_huge` 本身仍可能在其他场景下覆盖已有的 leaf 条目。为此在 riscv64 的 `map_huge` 中增加了 leaf 检测：

```rust
let l1 = if e2 & Sv39PteFlags::V.bits() != 0 {
    // L2 entry is valid — check whether it's a leaf or a table pointer.
    if pte_is_leaf(e2) {
        // L2 entry is already a 1GB leaf (R/W/X set). Demoting it to
        // a table pointer would silently corrupt the existing 1GB mapping.
        return Err(PageTableError::AlreadyMapped);
    }
    pte_to_paddr(e2) as *mut u64
} else { /* allocate new L1 */ };
```

同样的 leaf 检测也添加到了 x86_64 和 aarch64 的 `map_huge` 实现中。

#### A.3.3 修复 3：使用正确的 Sv39 规范高地址

链接脚本和测试中的 `KERN_VIRT_BASE` 修正：

```
错误：0xFFFFFC0000000000  (VPN[2] = 0，与 identity mapping 冲突)
正确：0xFFFFFFC000000000  (VPN[2] = 256，Sv39 canonical high)
```

### A.4 教训总结

#### A.4.1 Sv39 地址空间不是简单的高位取反

x86-64 的 canonical high 从 `0xFFFF_8000_0000_0000` 开始，bit 48 以上全为 1。直觉上可能认为 Sv39 也类似，从某个"高位取反"的位置开始。但 Sv39 只有 39 位有效地址，规范要求 bits[63:39] 全同，因此高地址从 `0xFFFF_FFC0_0000_0000` 开始——比 x86-64 的 `0xFFFF_8000_0000_0000` 高得多。

#### A.4.2 页表索引计算必须验证

不能凭直觉判断"这个地址的 VPN[2] 应该是多少"。必须实际计算：

```python
# Python 验证
def vpn2(addr):
    return (addr >> 30) & 0x1FF

assert vpn2(0xFFFF_FC00_0000_0000) == 0    # 错误地址！
assert vpn2(0xFFFF_FFC0_0000_0000) == 256  # 正确地址
```

#### A.4.3 Identity mapping 测试是必要的边界条件

hello-boot 等测试使用 `kern_virt_base == kern_phys_base`，这是"不使用高半核"的场景。arch_boot_impl 必须正确处理这种边界条件，否则 Step 2 的映射会覆盖 Step 1 的结果。

#### A.4.4 调试方法

在 map_huge 中添加调试输出，打印 `vaddr`、`paddr`、`i2`（L2 索引）、`e2`（L2 原有条目），可以快速定位 L2 条目被覆盖的问题：

```
map_huge: vaddr=0xffffffc000000000 paddr=0x80000000 size=0x200000 i2=0x100 e2=0x0
  L1 allocated at paddr=0x82001000
```

当看到 `i2` 值与 identity mapping 的 L2 索引重叠时，就能定位问题。

### A.5 影响范围

| 文件 | 修改内容 |
|------|---------|
| `os/kernel/src/arch/riscv64/link.ld` | `KERN_VIRT_BASE` 从 `0xFFFFFC0000000000` 改为 `0xFFFFFFC000000000` |
| `os/kernel/src/lib.rs` | Step 2 添加 `if kern_virt != kern_phys` 守卫；提取 `boot_validate_and_prepare` 消除重复逻辑；移除 riscv64 临时调试输出 |
| `os/arch/src/riscv64/paging.rs` | PTE 位常量改为 `Sv39PteFlags` bitflags；`map_huge` 增加 leaf 条目检测（`pte_is_leaf`）；`flags_to_pte` 添加 R=1 约束注释；runtime 方法（`map`/`unmap`/`query`/`remap`/`update_flags`/`new`/`destroy`）通过 Direct Map + 3-level Sv39 walk 实现 |
| `os/arch/src/x86_64/paging.rs` | PTE 位常量改为 `X64PteFlags` bitflags；`map_huge` 增加 1GB leaf 检测；runtime 方法通过 Direct Map + 4-level walk 实现 |
| `os/arch/src/arm64/paging.rs` | PTE 位常量改为 `Arm64PteFlags` bitflags；`map_huge` 增加 1GB block 检测；runtime 方法通过 Direct Map + 4-level walk 实现；新增 `pte_to_flags` 逆转换 |
| `os/kernel/src/boot_alloc.rs` | `static mut` → `AtomicU64`（避免 Rust 2024 UB）；后封装为 `BootAlloc` 结构体（含 `next`/`end` 两个 `AtomicU64`）支持 per-test 实例（避免 `cargo test` 多线程并行时全局 `BOOT_PT_NEXT` 互染）；生产路径仍用全局 `static BOOT_ALLOC: BootAlloc` |
| `os/arch/src/arch/pt_alloc.rs` | `static mut` → `UnsafeCell` + `AtomicBool`（避免 Rust 2024 UB） |
| `os/libs/minix-boot/src/kernel_info.rs` | `kern_size: usize` → `kern_size: u64`（避免 32 位截断） |
| `os/qemu-tests/test-kernels/kernel/bootstrap/test-higher-half-riscv64/src/main.rs` | `kern_virt_base` 修正为 `0xFFFF_FFC0_0000_0000` |
| `os/kernel/src/lib.rs` (单元测试) | `test_linker_script_riscv64_constraints` 地址修正 |

### A.6 参见

- 本文档 §4.1 — 链接脚本与 Sv39 地址规范
- [01-boot-shim-bootstrap.md](01-boot-shim-bootstrap.md) §4.6 — arch_boot_impl 三步操作
- RISC-V Privileged Specification §4.3 — Sv39 Page Table

---

## 7. 参见

- [01-boot-shim-bootstrap.md](01-boot-shim-bootstrap.md) — boot-shim 引导准备、KernelInfo 构造、BootShim trait
- [03-kmain-cstart.md](03-kmain-cstart.md) — kmain 入口后的保护模式初始化
- [00-kernel-overview.md](00-kernel-overview.md) — 内核整体架构概览
- [99-global-concepts.md](99-global-concepts.md) — 全局常量和类型定义
- `os/arch/src/arch/paging.rs` — Paging trait 定义
- `os/arch/src/arch/direct_map.rs` — Direct Map 地址布局
- `os/arch/src/arch/pte_walk_arch.rs` — `PteWalkArch` trait：跨架构只读 PTE walk（VA→PA），三架构实现（x86_64 4-level / aarch64 4-level / riscv64 Sv39 3-level），通过 `CurrentPteWalk` 类型别名 trait 分发，供 `vm::lookup_in_table` 与跨空间拷贝使用（详见 [18-syscall-copy.md](18-syscall-copy.md) §4.7/§4.8）
- `os/kernel/src/lib.rs` — arch_boot 入口
- `os/boot-shim/src/loader.rs` — ELF 加载逻辑
