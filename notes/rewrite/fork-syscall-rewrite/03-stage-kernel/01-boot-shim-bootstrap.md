# 01-boot-shim-bootstrap: Boot-shim 引导准备与控制权交接

> **分类**: Kernel 硬件发现
> **源码**: `minix3/minix/kernel/arch/i386/pre_init.c`(243行), `pg_utils.c`(317行)
> **说明**: 从固件（UEFI/OpenSBI）加载 boot-shim，获取内存映射、加载内核 ELF 与 boot 模块、构造 KernelInfo、退出固件服务，最后调用 `arch_boot()` 将控制权交给内核——boot-shim 的引导准备全链路
>
> **Minix3 C 对应**: GRUB 加载内核 → `pre_init()` 解析 multiboot 数据 → 建立恒等映射 → 开启分页。本文档覆盖 Rust 重写后等效路径的引导准备阶段。
>
> **路径约定**：本文档引用的所有 `minix3/...` 路径均**相对于仓库根目录**（即 `minix3/` 是仓库根的子目录）。例如 `minix3/minix/kernel/main.c` 在仓库中实际位于 `minix3/minix/kernel/main.c`。`minix3/etc/boot.cfg` 同理。

---

## 1. 概述

### 1.1 这是内核的第一段代码

内核是操作系统中最先运行的程序——但内核并不是从零开始的。在内核之前，系统经历了两个阶段：

1. **BIOS（固件）**：x86 CPU 上电后从实模式开始执行 BIOS 固件，完成硬件自检（POST），然后按照启动顺序查找可引导设备。BIOS 代码烧录在主板 ROM 中，不属于 Minix3 源码。
2. **GRUB（引导加载器）**：BIOS 加载磁盘上的 GRUB 第一阶段，GRUB 读取自己的配置文件（Minix3 提供 `etc/boot.cfg`，见 §2.0.3），将内核二进制和启动模块加载到物理内存，切换到保护模式，然后跳转到内核入口点。GRUB 是独立的 GNU 项目，**不属于 Minix3 源码**——Minix3 只提供 GRUB 配置文件和内核侧的 Multiboot 协议头（见 §2.0.1）。

内核被 GRUB 加载到物理内存的某个位置，此时还没有分页、没有进程——一切都需要内核自己建立。

`pre_init()` 和 `pg_utils.c` 是内核启动路径的最前端。它们的职责可以浓缩为一句话：

> **从 GRUB 手中接过系统状态，建立初始页表和分页机制，然后把内核启动信息（kinfo_t）交给 kmain()。**

整个过程被执行一次，之后这些代码的大部分就不再需要了（bootstrap 数据在 kmain 之后被释放）。

### 1.2 完整启动链路

> **本文档的语义边界**：以下链路从 GRUB 将控制权交给内核开始。minix-rs 中 boot-shim 的等效流程（UEFI/OpenSBI → boot-shim → 内核），详见 §3.1-§3.2。
>
> 更底层的内容（MBR → GRUB 第二阶段 → 读 `boot.cfg` → 解析 Multiboot Header）是引导加载器的职责，不属于内核源码。对于读过"自己写 OS"的读者：GRUB 的 `load_mods` / `multiboot` 命令等价于书中手写 `int 13h` + `cli` 跳转。详见附录 A。

#### 图 1：Minix3 C 启动链路（BIOS → GRUB → 内核）

```
x86 CPU 上电（实模式）
  │
  ├── BIOS POST + 查找可引导设备
  │
  ├── GRUB 第一/二阶段加载
  │     ├── 读取 /boot.cfg 配置（Minix3 提供）
  │     ├── 加载内核二进制到物理内存
  │     ├── 加载启动模块（PM/VM/VFS/RS 等）到物理内存
  │     ├── 切换到保护模式（CR0.PE=1）
  │     └── EAX = 0x2BADB002, EBX = multiboot_info_t 地址
  │         跳转到内核入口
  │
  └── 内核（Minix3 源码从这里开始）
        │
        ├── head.S: MINIX 标签 → multiboot_init
        │     ├── 设置临时栈（load_stack, 4KB）
        │     ├── push %ebx（multiboot_info_t 地址）
        │     ├── push %eax（魔数 0x2BADB002）
        │     └── call pre_init(magic, ebx)
        │
        ├── pre_init()（pre_init.c:217）← 内核第一个 C 函数
        │     ├── 验证魔数
        │     ├── get_parameters() → 解析内存布局
        │     ├── 建立恒等映射 + 内核高地址映射
        │     ├── 开启分页
        │     └── return &kinfo
        │
        └── head.S: call kmain(&kinfo)
              └── kmain()（main.c:115）→ 初始化进程表 → cstart() → switch_to_user()
```

**关键交接点（C 版）**：GRUB → 内核的交接通过 **Multiboot 协议**完成。内核在二进制文件头部嵌入一个 Multiboot Header（魔数 `0x1BADB002`，见 §2.0.1），GRUB 识别这个头部后就知道如何加载内核。启动完成后，GRUB 通过寄存器传递 `EAX=0x2BADB002`（确认是 Multiboot 启动）和 `EBX=multiboot_info_t 物理地址`（包含内存布局、模块列表等信息），这就是 `pre_init()` 接收到的两个参数。

> minix-rs 的 64 位多架构启动链路（UEFI/OpenSBI → boot-shim → 内核）详见 §3.1-§3.2。Rust 版与 C 版的核心差异在于：GRUB 代劳了"加载"，UEFI 只提供"接口"——加载内核和模块的工作从 GRUB 转移到了 boot-shim。详见 §3.1 中的完整对比。

### 1.3 启动前系统状态

当 GRUB 跳转到内核时，系统处于以下状态：

| 维度 | 状态 |
|------|------|
| **CPU 模式** | 保护模式（Protected Mode），但还没有分页 |
| **寄存器** | EAX = MULTIBOOT_INFO_MAGIC（0x2BADB002），EBX = multiboot_info_t 结构体地址 |
| **物理内存** | GRUB 分配好的区域，但内核只知道地址，不知道大小和布局 |
| **页表** | 不存在——CR0.PG = 0 |
| **堆** | 不存在——没有 malloc，没有全局分配器 |
| **中断** | 不存在——IDT 未建立 |

> **历史包袱 vs 真正的 OS 知识**：表格里的"保护模式"是 x86 特有的术语，但它背后的语义——**内核运行在高特权级、用户态运行在低特权级**——是所有架构共享的（ARM 叫 EL1/EL0）。那些"自己写 OS"书里大段讲的实模式、A20 地址线、分段机制，是 8086/80286 时代的硬件兼容性 hack：实模式是 16 位遗留，A20 是 80286 地址回绕 bug 的补丁，分段是 x86 独有的寻址方式——ARM 一样也没有，照样跑操作系统。GRUB/UEFI 已经把这些全部处理完了，内核的第一行代码就已经跑在保护模式（或 ARM 的 SVC 模式）里。**值得理解的是"为什么需要特权级隔离"，不必深究的是"GDTR 怎么加载、A20 怎么开启"。**

### 1.4 自举的四个步骤

```
pre_init(magic, ebx)
  │
  ├── 步骤 1: get_parameters(ebx, &kinfo)
  │   ├── 复制 multiboot_info_t 到自家的 kinfo.mbi
  │   ├── 解析启动命令行（multiboot param_buf）
  │   ├── 构建物理内存 map：遍历 GRUB mmap → add_memmap()
  │   └── 切割内核 + boot 模块占用 → cut_memmap()
  │
  ├── 步骤 2: 建立恒等映射
  │   ├── pg_clear() → 页目录清零
  │   ├── pg_identity(&kinfo) → 4MB 大页恒等映射（VA = PA）
  │   └── pg_mapkernel() → 映射内核到高虚拟地址
  │
  ├── 步骤 3: 开启分页
  │   ├── pg_load() → 写 CR3 = 页目录物理地址
  │   └── vm_enable_paging() → CR0.PG=1
  │
  └── 步骤 4: 返回 &kinfo → kmain()
```

> **为什么必须先建恒等映射再开分页？** 开分页（CR0.PG=1）后，CPU 立刻开始用虚拟地址取指令。恒等映射保证虚拟地址 = 物理地址，所以开分页前后地址值不变，正在执行的代码无感过渡。如果没有恒等映射就开分页，CPU 用虚拟地址去查页表——查不到——立刻页错误，而此时内核连页错误处理程序都还没装，系统直接崩溃。

### 1.5 x86-32 与 ARM32 启动路径对比

§1.4 的四个步骤是**架构无关的语义**——无论 x86 还是 ARM，内核自举都必须完成相同的事。Minix3 同时支持 x86-32 和 ARM32（earm），两者的启动代码共享相同的函数名和调用流程，语义一致，只是指令集不同（x86 汇编 vs ARM 汇编）。

#### 1.5.1 汇编入口对比

| | x86-32 (`kernel/arch/i386/head.S`) | ARM32 (`kernel/arch/earm/head.S`) |
|---|---|---|
| **入口** | GRUB 跳转到 `MINIX` 标签，该处指令为 `jmp multiboot_init` | U-Boot 跳转到 `MINIX` 标签，该处指令为 `b multiboot_init` |
| **设栈** | `mov $load_stack_start, %esp` | `ldr sp, =load_stack_start` |
| **清帧指针** | `mov $0, %ebp` | `mov fp, #0` |
| **调用 pre_init** | `push %ebx; push %eax; call pre_init` | `bl _C_LABEL(pre_init)` |
| **参数传递** | 栈传参：EAX=魔数, EBX=mbi 地址 | 寄存器传参：r0=argc, r1=argv（C 调用约定） |
| **调用 kmain** | `push %eax; call kmain` | `ldr r2, =kmain; bx r2`（r0 仍持有 kinfo 指针） |
| **临时栈** | `.data` 段 4096 字节 | `.data` 段 4096 字节 |

**共同语义**：设栈 → 调 `pre_init()` → 调 `kmain()`。差异仅在于指令集和参数传递方式。

#### 1.5.2 引导加载器对比

| | x86-32 | ARM32 |
|---|---|---|
| **引导加载器** | GRUB（外部，不属于 Minix3） | U-Boot（外部，不属于 Minix3） |
| **协议** | Multiboot（内核嵌入 Header，GRUB 识别） | ATAGS / 设备树（U-Boot 传递板级信息） |
| **内核"名片"** | Multiboot Header（魔数 `0x1BADB002`） | 无——U-Boot 直接加载 ELF 入口点 |
| **参数传递** | GRUB → EAX/EBX 寄存器 | U-Boot → argc/argv（C 调用约定 r0/r1） |
| **内存信息来源** | GRUB 填充 `multiboot_info_t`（mmap、mods） | ARM `pre_init()` 自己构造 `multiboot_info_t`（硬编码地址范围） |
| **配置文件** | `etc/boot.cfg`（Minix3 提供） | U-Boot 环境变量（板级配置，Minix3 不提供） |

**关键差异**：x86 的 `multiboot_info_t` 由 GRUB 填充后传给内核；ARM 没有等价的外部引导协议，所以 ARM 的 `pre_init()` 在 `setup_mbi()` 中**自己硬编码**构造了一个 `multiboot_info_t`（模块地址 `0x82000000` 起，内存 `0x80000000` 起 256MB），让后续代码以为"GRUB 传了数据过来"。这是一个适配层（shim）。

#### 1.5.3 分页机制对比

两者都调用同名函数 `pg_clear()` → `pg_identity()` → `pg_mapkernel()` → `pg_load()` → `vm_enable_paging()`，语义完全一致，只是指令集不同：

| 步骤 | x86-32 | ARM32 |
|------|--------|-------|
| **页表粒度** | 4MB 大页（PSE，CR4.PSE=1） | 1MB Section（ARM L1 页表项） |
| **pg_identity()** | 遍历 4MB 区间，设置 PDE 的 PS 位 + 物理地址 | 遍历 1MB Section，设置 Section 描述符 + 域 + 缓存属性 |
| **pg_mapkernel()** | 将内核物理地址映射到高虚拟地址（PDE 条目） | 将内核物理地址映射到高虚拟地址（Section 条目） |
| **pg_load()** | `write_cr3(页目录物理地址)` | `write_ttbr0(页目录物理地址)` |
| **vm_enable_paging()** | `CR0.PG = 1`（开启分页） | `SCTLR.M = 1`（开启 MMU）+ 使能 I/D Cache + 分支预测 |
| **额外步骤** | 无 | ARM 需要先 `dcache_clean()`（清缓存），设置 DACR（域访问控制），设置 TTBRC（页表基址控制） |

**共同语义**：清零页目录 → 建立恒等映射 → 映射内核到高地址 → 加载页表基址 → 开启分页/MMU。

#### 1.5.4 架构无关的语义总结

剥离硬件细节后，内核自举必须完成的事只有四件：

```
1. 接收引导参数    → 从引导加载器获取内存布局、模块列表、启动参数
2. 建立地址映射    → 恒等映射（让当前代码继续跑）+ 内核高地址映射（为后续运行准备）
3. 开启分页/MMU    → 从物理地址世界切换到虚拟地址世界
4. 传递启动信息    → 将 kinfo_t 交给 kmain()，完成自举
```

这四件事的**顺序不可变**（必须先映射再开分页，否则开分页后地址空间不对代码就飞了），但**实现方式完全由架构决定**。关于 64 位重写后的差异，详见 §3。

### 1.6 两个 C 文件的职责划分

| 文件 | 职责 | 核心函数 |
|------|------|---------|
| `pre_init.c` | 解析 multiboot 数据、构建内存 map、编排启动流程 | `pre_init()`, `get_parameters()`, `overlaps()`, `mb_set_param()` |
| `pg_utils.c` | 页目录/页表操作、分页控制、物理页分配 | `pg_identity()`, `pg_mapkernel()`, `vm_enable_paging()`, `pg_load()`, `pg_alloc_page()`, `pg_map()` |

---

## 2. C 源码分析

### 2.0 内核入口：GRUB 向内核传递启动信息

> **本节定位**：本节聚焦 GRUB 如何识别内核、跳转入口，以及最关键的是——**通过哪些寄存器和数据结构向内核传递启动信息**（内存布局、模块列表、启动参数）。这些传递机制是理解 `pre_init()` 和 `get_parameters()` 的前提。关于"GRUB/UEFI 如何加载内核二进制到内存"（磁盘 I/O、ELF 解析、模块拷贝等）属于链接与加载范畴，详见 [02-higher-half-kernel.md §2.0](02-higher-half-kernel.md#20-grub-加载配置) 及该文档 §4.2。
>
> **C 版 vs Rust 版**：C 版通过 Multiboot 协议完成上述交接（魔数识别 + `multiboot_info_t` 结构体）；Rust 版通过 UEFI/OpenSBI 固件协议加载 boot-shim，再由 boot-shim 构建 `KernelInfo` 并调用 `arch_boot()`。虽然机制不同，但传递的信息类型完全一致——内存映射、内核位置、模块列表。

#### 2.0.1 Multiboot Header — 内核告诉 GRUB "我是 Multiboot 内核"

**源码**：`minix3/minix/kernel/arch/i386/head.S:47-66`

Multiboot 协议要求内核二进制文件的前 8192 字节内必须包含一个 **Multiboot Header**，否则 GRUB 不认识这个内核。这个头部是内核主动嵌入的"名片"：

```asm
/* head.S:47-66 — Multiboot Header */
.balign 8

#define MULTIBOOT_FLAGS (MULTIBOOT_HEADER_WANT_MEMORY | MULTIBOOT_HEADER_MODS_ALIGNED)

multiboot_magic:
    .long MULTIBOOT_HEADER_MAGIC       /* 0x1BADB002 — 告诉 GRUB "我是 Multiboot 内核" */
multiboot_flags:
    .long MULTIBOOT_FLAGS              /* 请求内存信息 + 模块对齐 */
multiboot_checksum:
    .long -(MULTIBOOT_HEADER_MAGIC + MULTIBOOT_FLAGS)  /* 校验和：三者之和必须为 0 */
    .long 0                            /* 头部地址（未使用 HAS_ADDR） */
    .long 0                            /* 加载地址 */
    .long 0                            /* 加载结束地址 */
    .long 0                            /* BSS 结束地址 */
    .long 0                            /* 入口地址 */
/* Video mode — 请求 EGA 文本模式 80×25 */
multiboot_mode_type:
    .long MULTIBOOT_VIDEO_MODE_EGA     /* 1 = EGA 文本模式 */
multiboot_width:
    .long MULTIBOOT_CONSOLE_COLS       /* 80 */
multiboot_height:
    .long MULTIBOOT_CONSOLE_LINES      /* 25 */
multiboot_depth:
    .long 0
```

**三个关键字段的作用**：

| 字段 | 值 | 作用 |
|------|------|------|
| `MULTIBOOT_HEADER_MAGIC` | `0x1BADB002` | GRUB 扫描内核前 8KB，找到这个魔数就识别为 Multiboot 内核 |
| `MULTIBOOT_FLAGS` | `WANT_MEMORY \| MODS_ALIGNED` | 告诉 GRUB："启动后给我内存信息"和"模块请 4KB 对齐" |
| `checksum` | `-(magic + flags)` | GRUB 验证 `magic + flags + checksum == 0`，确认头部完整 |

**两个魔数的区别**（新手常混淆）：

| 魔数 | 值 | 方向 | 含义 |
|------|------|------|------|
| `MULTIBOOT_HEADER_MAGIC` | `0x1BADB002` | 内核 → GRUB | 内核二进制头部嵌入，GRUB 读取识别 |
| `MULTIBOOT_INFO_MAGIC` | `0x2BADB002` | GRUB → 内核 | GRUB 启动后放入 EAX，内核验证确认是 Multiboot 启动 |

定义于 `minix3/sys/arch/i386/include/multiboot.h:40,86`。

#### 2.0.2 head.S — 汇编入口到 pre_init() 的跳板

**源码**：`minix3/minix/kernel/arch/i386/head.S:36-82`

GRUB 跳转到内核后，最先执行的是 `head.S` 中的 `MINIX` 标签。这个汇编文件做了三件事：跳转到 `multiboot_init`、设置栈、调用 `pre_init()`。

```asm
/* head.S:36-39 — 内核入口点，GRUB 跳转到这里 */
.global MINIX
MINIX:
/* this is the entry point for the MINIX kernel */
    jmp multiboot_init          /* 直接跳转到 multiboot_init */

/* head.S:47-66 — Multiboot Header（见 §2.0.1）*/
...

/* head.S:68-82 — multiboot_init: 真正的初始化入口 */
multiboot_init:
    mov  $load_stack_start, %esp   /* 设置临时栈（4KB 静态分配，head.S:94-96） */
    mov  $0, %ebp                  /* 清除帧指针（便于调试器回溯终止） */
    push $0                        /* 清除 EFLAGS（关闭中断、清除嵌套任务标志） */
    popf
    push $0                        /* 对齐栈（16 字节对齐，x86 ABI 要求） */

    push %ebx                      /* 第 2 个参数：multiboot_info_t 地址 */
    push %eax                      /* 第 1 个参数：魔数 0x2BADB002 */
    call pre_init                  /* 调用内核第一个 C 函数 */

    /* pre_init 返回后，分页已开启，内核已映射到高地址 */
    mov  $k_initial_stktop, %esp   /* 切换到高地址栈 */
    push $0                        /* 栈终止标记 */
    push %eax                      /* pre_init 返回的 &kinfo */
    call kmain                     /* 调用 kmain(&kinfo) */

hang:
    jmp hang                       /* 不应到达 */
```

**逐行解释**：

1. `jmp multiboot_init`：GRUB 跳转到 `MINIX` 标签后，立即跳过中间的 Multiboot Header 数据，跳到代码区。
2. `mov $load_stack_start, %esp`：设置临时栈。`load_stack` 是 head.S 末尾 `.data` 段中静态分配的 4096 字节（`head.S:94-96`）。在 `pre_init()` 返回前，内核使用这个栈。
3. `mov $0, %ebp`：清零帧指针。栈回溯（内核 panic 打印调用栈、QEMU+GDB 远程调试等）通过 `%ebp` 链逐帧回溯，`%ebp=0` 表示栈底，回溯到此终止。
4. `push $0; popf`：清除 EFLAGS 寄存器。关闭中断标志（IF=0），清除嵌套任务标志（NT=0），确保内核从干净的状态开始。`popf` 恢复了 `push $0` 占用的栈空间，因此这对操作对栈深度净影响为零。
5. `push $0`：空占位，使栈 16 字节对齐（x86 System V ABI 要求 `call` 前栈 16 字节对齐，`push %eax` + `push %ebx` + `push $0` = 3×4=12 字节偏移，加上 `call` 的返回地址 4 字节 = 16 字节对齐）。
6. `push %ebx; push %eax; call pre_init`：将 GRUB 传递的两个参数压栈，调用 `pre_init()`。C 调用约定下，参数从右到左压栈，所以 `%ebx`（第 2 参数）先压，`%eax`（第 1 参数）后压。
7. `mov $k_initial_stktop, %esp`：`pre_init()` 返回后，分页已开启，内核已映射到高虚拟地址。此时切换到高地址栈 `k_initial_stktop`，因为低地址的 `load_stack` 可能不再可访问（取决于恒等映射是否保留）。
8. `push %eax; call kmain`：将 `pre_init()` 返回的 `&kinfo` 传给 `kmain()`。

**为什么需要汇编跳板**：C 函数需要栈才能运行（局部变量、参数、返回地址都在栈上），但 GRUB 跳转时没有设置栈。`head.S` 的核心职责就是"为 C 代码准备栈，然后跳转"。

#### 2.0.3 GRUB 启动配置与模块加载

Minix3 通过 `boot.cfg` 配置 GRUB 的启动行为：`multiboot /boot/.../kernel` 加载内核（GRUB 扫描 Multiboot Header 魔数），`load_mods /boot/.../mod*` 加载启动模块（PM、VM、VFS、RS 等），`rootdevname=...` 和 `bootopts=-s` 作为启动参数通过 `multiboot_info_t.mi_cmdline` 传递给内核。

> **语义边界**：`boot.cfg` 描述的是"GRUB 如何从磁盘加载内核和模块到内存"——属于**链接与加载**范畴。C 版中这些操作由 GRUB 代劳，Rust 版中由 boot-shim 完成（ELF 解析、段拷贝、BSS 清零等）——UEFI 路径直接读 ESP 分区，OpenSBI 路径通过 U-Boot 预加载 + BootFileTable 间接读取，但两者的 ELF 加载主流程完全相同。详见 [02-higher-half-kernel.md §4.2](02-higher-half-kernel.md#42-elf-加载实现)。

**配置与源码的对应关系**（信息传递视角）：

| boot.cfg 指令 | 传递的信息 | 内核源码消费位置 |
|--------------|----------|----------------|
| `multiboot /boot/.../kernel` | 内核二进制被加载到某物理地址 | `head.S` 直接跳转到入口点 |
| `load_mods /boot/.../mod*` | 模块列表 → `multiboot_info_t.mi_mods_count/addr` | `pre_init.c` `get_parameters()` 拷贝到 `kinfo.module_list[]` |
| `rootdevname=...` | 参数字符串 → `multiboot_info_t.mi_cmdline` | `pre_init.c` `mb_set_param()` 解析到 `kinfo.param_buf` |

### 2.1 相关定义

> **过渡说明**：§2.0 描述了 GRUB 与内核的交互流程（Multiboot Header 识别、head.S 跳板、启动配置）。以下常量是内核 C 源码中实际识别和使用的数据定义，与 §2.0 描述的 GRUB 传递信息直接对应——例如 `MULTIBOOT_INFO_MAGIC` 是 head.S 验证的魔数，`MULTIBOOT_INFO_HAS_MMAP` 是 `multiboot_info_t` 标志位，对应 `get_parameters()` 解析的内存映射来源。

#### 2.1.1 Multiboot 常量

```
MULTIBOOT_INFO_MAGIC      = 0x2BADB002
MULTIBOOT_INFO_HAS_MMAP    ← mbi.mi_flags 位：有完整内存 map
MULTIBOOT_INFO_HAS_CMDLINE ← mbi.mi_flags 位：有启动命令行
MULTIBOOT_INFO_HAS_MEMORY  ← mbi.mi_flags 位：有基本内存大小
MULTIBOOT_INFO_HAS_MODS    ← mbi.mi_flags 位：有启动模块列表
MULTIBOOT_MEMORY_AVAILABLE  = 1
MULTIBOOT_MAX_MODS          ← 最大模块数
MAXMEMMAP                   ← 最大内存区域数
MULTIBOOT_VIDEO_BUFFER      ← 视频帧缓冲区地址
MULTIBOOT_PARAM_BUF_SIZE    ← 启动参数字符串缓冲区大小
```

#### 2.1.2 页表相关常量（minix/include/arch/i386/include/vm.h）

| 常量 | 值 | 说明 |
|------|------|------|
| `I386_PAGE_SIZE` | 4096 | 页大小（4KB） |
| `I386_BIG_PAGE_SIZE` | 4096 × 1024 = 4MB | 大页大小 |
| `I386_VM_DIR_ENTRIES` | 1024 | 页目录条目数 |
| `I386_VM_PRESENT` | 0x001 | PTE/PDE 存在位 |
| `I386_VM_WRITE` | 0x002 | 可写 |
| `I386_VM_USER` | 0x004 | 用户可访问 |
| `I386_VM_PWT` | 0x008 | Write-Through 缓存 |
| `I386_VM_PCD` | 0x010 | Cache Disable |
| `I386_VM_ACCESSED` | 0x020 | 已访问（CPU 硬件自动设置，boot 阶段不使用） |
| `I386_VM_DIRTY` | 0x040 | 已写入（CPU 硬件自动设置，boot 阶段不使用） |
| `I386_VM_BIGPAGE` | 0x080 | 4MB 大页 |
| `I386_VM_ADDR_MASK` | 0xFFFFF000 | 物理地址掩码（4KB 对齐） |
| `I386_VM_ADDR_MASK_4MB` | 0xFFC00000 | 物理地址掩码（4MB 对齐） |

#### 2.1.3 CR0/CR4 控制位

| 常量 | 位 | 说明 |
|------|------|------|
| `I386_CR0_PE` | bit 0 | 保护模式（GRUB 已设置） |
| `I386_CR0_WP` | bit 16 | 写保护（内核态也不能写只读页） |
| `I386_CR0_PG` | bit 31 | 分页开关 |
| `I386_CR4_PSE` | bit 4 | 大页支持（4MB/2MB） |
| `I386_CR4_PGE` | bit 7 | Global Page 标志 |

#### 2.1.4 地址宏

```
I386_VM_PDE(v) = v >> 22          ← 虚拟地址 → 页目录索引（高 10 位）
I386_VM_PTE(v) = (v >> 12) & 0x3FF ← 虚拟地址 → 页表索引（中 10 位）
I386_VM_PFA(e) = e & 0xFFFFF000    ← PDE/PTE → 物理地址（低 12 位清零）
```

#### 2.1.5 pg_utils.c 内部常量

```
PG_PAGETABLES = 6                 ← 静态页表池：6 个 4KB 页表（共 24KB）
PG_ALLOCATEME                     ← pg_map() 的占位值——调用 pg_alloc_page()
LIMIT         = 0xFFFFF000        ← 物理内存截断到 4GB（32 位限制）
```

### 2.2 核心数据结构

#### 2.2.1 `kinfo_t` — 内核启动信息

```c
typedef struct kinfo {
    /* Straight multiboot-provided info */
    multiboot_info_t        mbi;
    multiboot_module_t      module_list[MULTIBOOT_MAX_MODS];
    multiboot_memory_map_t  memmap[MAXMEMMAP]; /* free mem list */
    phys_bytes              mem_high_phys;
    int                     mmap_size;

    /* Multiboot-derived */
    int                     mods_with_kernel; /* no. of mods incl kernel */
    int                     kern_mod; /* which one is kernel */

    /* Minix stuff, started at bootstrap phase */
    int                     freepde_start;  /* lowest pde unused kernel pde */
    char                    param_buf[MULTIBOOT_PARAM_BUF_SIZE];

    /* Minix stuff */
    struct kmessages        *kmessages;
    int do_serial_debug;    /* system serial output */
    int serial_debug_baud;  /* serial baud rate */
    int minix_panicing;     /* are we panicing? */
    vir_bytes               user_sp; /* where does kernel want stack set */
    vir_bytes               user_end; /* upper proc limit */
    vir_bytes               vir_kern_start; /* kernel addrspace starts */
    vir_bytes               bootstrap_start, bootstrap_len;
    struct boot_image       boot_procs[NR_BOOT_PROCS];
    int nr_procs;           /* number of user processes */
    int nr_tasks;           /* number of kernel tasks */
    char release[6];        /* kernel release number */
    char version[6];        /* kernel version number */
    int vm_allocated_bytes; /* allocated by kernel to load vm */
    int kernel_allocated_bytes;		/* used by kernel */
    int kernel_allocated_bytes_dynamic;	/* used by kernel (runtime) */
} kinfo_t;
```

**关键字段说明**：

| 字段 | 来源 | 用途 |
|------|------|------|
| `mmap_size` / `memmap[]` | `get_parameters()` 解析 | 空闲物理内存区域列表——内核和 VM 的分配器都依赖它 |
| `mem_high_phys` | `add_memmap()` 更新 | 可用的最大物理地址 |
| `module_list[]` | GRUB → 拷贝 | 启动进程二进制信息（PM/VM/VFS/RS 等） |
| `bootstrap_start/len` | 链接器符号 | bootstrap 代码的范围，kmain 之后可以释放 |
| `freepde_start` | `pg_mapkernel()` 返回 | 内核映射后第一个空闲 PDE——跨地址空间访问时 `createpde()` 的临时映射槽位从此分配（详见 [06](07-cross-space-init.md)） |

#### 2.2.2 `multiboot_memory_map_t`

描述一段物理内存区域：起始地址、长度、是否可用。GRUB 遍历 BIOS 提供的内存映射，填充多条此结构，内核据此知道哪些物理内存可以分配。

```c
struct multiboot_mmap {
    u32_t   mm_size;
    u64_t   mm_base_addr;
    u64_t   mm_length;
    u32_t   mm_type;        // MULTIBOOT_MEMORY_AVAILABLE(=1)
};
```

#### 2.2.3 `multiboot_module_t`

描述一个启动模块（PM、VM、VFS 等用户态服务器二进制）：它在物理内存中的起止地址和命令行参数。GRUB 通过 `load_mods` 加载这些模块后填充此结构。

```c
struct multiboot_module {
    u32_t   mmo_start;
    u32_t   mmo_end;
    u32_t   mmo_string;     // 命令行字符串物理地址
    u32_t   mmo_reserved;
};
```

#### 2.2.4 `pagedir[1024]` — 静态页目录

```c
static u32_t pagedir[1024]  __aligned(4096);
```

x86-32 二级页表的一级表。每个条目（PDE）覆盖 4MB 地址空间，可以直接映射 4MB 大页（`pg_identity()`、`pg_mapkernel()` 的做法），也可以指向一个二级页表做 4KB 精细映射（`pg_map()` 的做法）。`pg_load()` 通过 `vir2phys(pagedir)` 获取物理地址写入 CR3。

#### 2.2.5 `pagetables[6][1024]` — 静态页表池

```c
#define PG_PAGETABLES 6
static u32_t pagetables[PG_PAGETABLES][1024] __aligned(4096);
```

x86-32 二级页表的二级表池。`pg_identity()` 和 `pg_mapkernel()` 用 4MB 大页，不需要页表；`pg_map()` 做 4KB 精细映射时，从池中取一个页表挂到 `pagedir` 的某个 PDE 下。6 个是编译时写死的上限，用完 panic。

### 2.3 关键函数分析

#### 2.3.1 `pre_init()` — 内核入口点（pre_init.c:217）

```c
kinfo_t *pre_init(u32_t magic, u32_t ebx)
{
    assert(magic == MULTIBOOT_INFO_MAGIC);
    get_parameters(ebx, &kinfo);
    pg_clear();
    pg_identity(&kinfo);
    kinfo.freepde_start = pg_mapkernel();
    pg_load();
    vm_enable_paging();
    return &kinfo;
}
```

由 `head.S` 中的 `multiboot_init` 调用（`call pre_init`，head.S:77），是内核执行的第一个 C 函数。验证魔术值 → 解析 multiboot → 恒等映射 → 内核映射 → 开分页 → 返回 `&kinfo`。

#### 2.3.2 `get_parameters()` — 解析 GRUB 数据（pre_init.c:94）

**角色**：`pre_init()` 的"情报收集官"。它将 GRUB 传递的原始 multiboot 数据（`multiboot_info_t`）转化为内核可用的结构化信息（`kinfo_t`）。内核后续的所有内存分配、模块加载、页表建立都依赖它的输出——如果它漏掉一块内存或算错模块位置，后续 `pg_identity()` 和 `pg_mapkernel()` 就会映射到错误的物理页。

步骤：拷贝 mbi → 初始化 kinfo 字段 → 解析命令行 → 构建物理内存 map (`add_memmap`×N) → 拷贝模块列表 → 检查重叠 (`overlaps`) → 切除占用 (`cut_memmap`)

#### 2.3.3 `add_memmap()` — 添加可用内存区域（pg_utils.c:86）

**角色**：物理内存账本的"记账员"。`get_parameters()` 调用它把 GRUB 报告的可用内存区域登记到 `kinfo.memmap[]` 中。这个数组是内核自举阶段唯一的物理内存信息来源——`pg_alloc_page()`（从尾部分配物理页）和 `alloc_lowest()`（从头部分配）都依赖它。

硬截断到 4GB（`LIMIT 0xFFFFF000`）→ 4KB 对齐 → 填入 `cbi->memmap[]` → 更新 `mem_high_phys`。

#### 2.3.4 `cut_memmap()` — 切除已占用区域（pg_utils.c:32）

**角色**：内存账本的"红笔修正"。当内核镜像或 boot 模块被加载到某段物理内存后，这段内存就不能再被分配了。`cut_memmap()` 将已占用区域 `[start, end)` 从可用内存列表中切除，保证后续的 `pg_alloc_page()` 不会分配到已被代码或数据占用的物理页——否则页表会覆盖内核自身，导致启动崩溃。

将 `[start, end)` 从 memmap 中切除，余量通过 `add_memmap()` 写回。

#### 2.3.4b `alloc_lowest()` — 分配最低物理页（pg_utils.c:65）

遍历 memmap 找到满足 `len` 大小的最低基址区域，调用 `cut_memmap()` 切除并返回。用于 `pre_init` 中分配内核栈等需要低地址的物理内存。与 `pg_alloc_page()`（从尾部取）互补——一个取最低，一个取最高。

#### 2.3.5 `pg_identity()` — 恒等映射（pg_utils.c:162）

1024 × 4MB 大页恒等映射。每个 PDE 设置 `PRESENT | BIGPAGE | USER | WRITE` flag。超出 `mem_high_phys` 的 PDE 额外加 `PWT | PCD`（禁用缓存）。

> **PWT|PCD 辨析**：超出 `mem_high_phys` 的物理地址是 MMIO 设备寄存器或空洞（非真 DRAM），必须禁缓存（写穿透、不进 cache），否则设备读写的写后读语义与一致性会被缓存破坏而失灵。C 源码注释（`pg_utils.c:167-169`）明示："We map memory that does not correspond to physical memory as non-cacheable."

> **USER flag 辨析**：`pg_identity()` 设 `I386_VM_USER`，但 boot 阶段内核运行在 Ring 0，U/S 位对访问无影响——本质上是"无所谓"的标志。有意义的 contrast 在 `pg_mapkernel()`（§2.3.6）：它**不设** `I386_VM_USER`，刻意将内核地址标记为 supervisor-only。`pg_info()` 将页目录写入 `vm->p_seg.p_cr3`，但 VM 的 `pt_init()`（`vm/pagetable.c:1284`）**显式跳过所有 BIGPAGE 条目**（"boot identity mapping (don't want)"），自己新建页目录。恒等映射从 VM 启动那一刻起就被丢弃，不会被任何 Ring 3 进程使用。
> 
> Rust 版本（64 位 UEFI）不设 USER flag，因为 boot-shim 页表仅在内核 boot 阶段使用，不会传递给任何用户态进程。

#### 2.3.6 `pg_mapkernel()` — 内核高地址映射（pg_utils.c:186）

4MB 大页映射内核到高虚拟地址。返回第一个空闲 PDE 号。

#### 2.3.7 `vm_enable_paging()` — 开启分页（pg_utils.c:204）

执行：清 PG+PGE → 开 PSE → 开 PG → 开 WP → 开 PGE。

#### 2.3.8 `pg_load()` / `pg_alloc_page()` / `pg_map()`

`pg_load()` 写 CR3；`pg_alloc_page()` 从 memmap 尾部取 4KB；`pg_map(phys, vaddr, vaddr_end, cbi)` 4KB 粒度映射，将 `[vaddr, vaddr_end)` 映射到从 `phys` 开始的物理地址。若 `phys == PG_ALLOCATEME`，则每个虚拟页自动分配物理页（调用 `pg_alloc_page()`）。要求 `vaddr < kern_vir_start`（仅映射低地址区）。

> **未单独分析的辅助函数**：`pg_rounddown(addr)`（pg_utils.c:259）将地址向下对齐到页边界，一行实现；`pg_info(proc, cbi)`（pg_utils.c:295）将页目录物理地址写入进程的 `p_cr3` 字段，供 VM 阶段使用；`print_memmap(cbi)`（pg_utils.c:17）调试打印函数。三者均非 boot 核心逻辑，Rust 版本中 `pg_rounddown` 由 `PhysBytes`/`VirBytes` 的对齐方法替代，`pg_info` 由 `Paging` trait 内部管理，`print_memmap` 由 `log::debug!` 替代。

### 2.4 调用关系

```
head.S: multiboot_init (assembly)
  └── pre_init(magic, ebx)
        ├── get_parameters(ebx, &kinfo)
        │     ├── memcpy(mbi, ebx)
        │     ├── add_memmap() × N → cut_memmap() × mod_count
        │     └── overlaps() × mod_count
        ├── pg_clear()
        ├── pg_identity(&kinfo)     ← 4MB 大页恒等映射
        ├── pg_mapkernel()          ← 内核高地址映射
        ├── pg_load() → write_cr3()
        └── vm_enable_paging()      ← CR0.PG=1
        → return &kinfo
head.S: call kmain(&kinfo)
  └── kmain(cbi)
        ├── cstart()               ← 内核初始化（prot_init, init_clock 等）
        ├── proc_init()            ← 进程表初始化
        └── switch_to_user()
kmain(cbi)
  └── pg_map() / pg_alloc_page()   ← kmain 阶段继续使用
```

### 2.5 设计要点

**预分配的静态内存**：pre_init 阶段所有数据结构编译时静态分配（`kinfo` BSS、`pagedir[1024]` 静态数组、`pagetables[6][1024]` 池）。

**为什么先恒等映射再映射内核**：开分页后 CPU 立即使用页表。不开恒等映射，正在执行的代码会页错误。

**为什么 4GB 截断**：32 位 x86 只能寻址 4GB。`LIMIT 0xFFFFF000` 是物理限制。64 位 Rust 必须删除。

**为什么物理内存从尾部取**：减少大块连续区域前端的碎片化。

**boot module 内存的生命周期**：`pre_init()` 阶段的 `cut_memmap()` 只是临时切掉 boot module 占用的物理内存，防止后续分配器误用。这些内存不是永久占用——内核在 `protect.c` 中解析每个 module 的 ELF、把代码段/数据段复制到进程地址空间后，通过 `add_memmap()` 归还，`mod_start = mod_end = 0` 标记已回收（`protect.c:450-451`）。同样，bootstrap 代码段在 `kmain()` 后也会被回收（`main.c:301`）。<!-- TODO: 补充回收逻辑的详细分析文档引用 -->

---

## 3. Rust 设计决策

> 本章解释从 Ch1&2 的 C 源码到 Rust 设计的每一个关键选择。每个决策给出：为什么选这条路径、替代方案有哪些、为什么否决替代方案。

### 3.1 UEFI 替代 Multiboot（架构演进）

**C 源码依据**：§2.3.1 `pre_init(magic, ebx)` — Multiboot 是由 GRUB 传递的 32 位引导协议。

**决策**：minix-rs 使用 UEFI 作为引导协议，替代 Minix3 的 Multiboot+GRUB。

**理由**：

| | Minix3 C（x86-32） | minix-rs（x86-64 / aarch64 / riscv64） |
|------|-------------------|---------------------------------------|
| **引导协议** | Multiboot（1995 年规范，x86-32 专用） | UEFI（跨架构统一） |
| **引导加载器** | GRUB（第三方，Minix3 不含源码） | UEFI 固件（QEMU 用 OVMF，真机用板载 UEFI） |
| **内核"名片"** | Multiboot Header（魔数 `0x1BADB002`） | UEFI PE/COFF 头（标准可执行格式） |
| **参数传递** | GRUB → EAX/EBX 寄存器 | UEFI → `ImageHandle` + `SystemTable` |
| **内存发现** | 手动遍历 `multiboot_memory_map_t` | `GetMemoryMap()` 直接返回 |
| **模块加载** | GRUB `load_mods` + `multiboot_info_t.mi_mods` | boot-shim 通过 `SimpleFileSystemProtocol` 从 ESP 分区读取（见下方约束） |
| **配置文件** | `etc/boot.cfg`（Minix3 提供） | UEFI Boot Manager / LoadOption |
| **boot-shim** | 无（`head.S` + `pre_init()` 在内核内部） | 独立 crate，`BootShim` trait 分离 UEFI/OpenSBI |

上表的核心差异可以概括为一句话：GRUB 代劳了"加载"，而 UEFI 只提供"接口"——加载内核和模块的工作从 GRUB 转移到了 boot-shim。这个变化不是偶然：64 位多架构内核需要统一的引导抽象，而 UEFI 恰好提供了跨平台的 API 标准。

**为什么不用 GRUB**：Multiboot 是 1995 年的规范，只考虑了 x86-32。minix-rs 需要支持 x86-64、aarch64、riscv64 三种架构，UEFI 是唯一跨架构一致的引导协议。GRUB 虽然也支持 Multiboot2（64 位扩展），但各架构的 GRUB 实现差异大，不如 UEFI 统一。

> **关键约束**：`ExitBootServices()` 之后，UEFI 的所有服务（文件系统、内存分配、协议查询）全部失效。因此 boot-shim 必须在调用 `ExitBootServices()` **之前**完成所有磁盘 I/O——读取 kernel ELF 和所有 boot 模块（VM/PM/VFS/RS 等）。这是所有主流 OS bootloader 的共同做法。

**替代方案 & 否决理由**：

| 方案 | 否决原因 |
|------|---------|
| 保留 Multiboot + 64 位扩展 | ARM/RISC-V 无等效协议 |
| 各架构独立引导（x86 Multiboot + ARM PSCI + RISC-V SBI） | 三种引导路径 = 过度复杂 |
| Linux 风格（UEFI stub 内嵌 kernel） | boot-shim 与内核耦合，不利于独立调试和迭代 |

### 3.2 boot-shim = 独立 crate，BootShim trait 分离固件实现

**C 源码依据**：§2.4 `pre_init()` → `kmain()` — C 版本 bootstrap 在 kmain 后被释放。

**决策**：引导逻辑放在独立 crate `boot-shim`，通过 `BootShim` trait（定义在 `minix-boot` crate 的 `boot_shim.rs`）分离 UEFI 和 OpenSBI 实现。kernel 只依赖 trait，不依赖任何固件类型。feature gate 仅在 `Cargo.toml` 中选择编译哪个实现，代码中不散落 `#[cfg]` 条件编译。

```
boot-shim crate (feature = "uefi")     boot-shim crate (feature = "opensbi")
     │                                       │
     │ UefiBootShim::prepare_boot()          │ OpenSbiBootShim::prepare_boot()
     │   UEFI GetMemoryMap()                 │   硬编码 QEMU virt 内存映射
     │   UEFI AllocatePages()                │   bump 分配器
     │   从 ESP 加载 kernel + boot 模块      │   从 BootFileTable 加载 kernel + boot 模块
     │   定位平台描述符（ACPI RSDP / DTB）   │   从 a1 寄存器获取 DTB 物理指针
     │   UEFI ExitBootServices()             │
     │   → BootPrepareResult                 │   → BootPrepareResult
     │                                       │
     └─→ arch_boot(result.kernel_info,
               result.root_page) → kmain()
```

**移植性**：换非 UEFI 板 → 在 `boot-shim` 中实现新的 `BootShim` impl，Cargo.toml 启用对应 feature。kernel 一行不改。

**为什么 riscv64 用 OpenSBI 而非 UEFI**：Rust 没有 `riscv64-unknown-uefi` 编译目标，无法编译 UEFI 应用。riscv64 通过 OpenSBI 固件直接启动裸金属内核，`OpenSbiBootShim` 提供硬编码的 QEMU virt 内存映射和 bump 分配器，与 `UefiBootShim` 输出相同的 `BootPrepareResult`。未来，一旦 Rust 生态完整支持 RISC-V UEFI，我们只需在 boot-shim crate 中新增一个 UefiBootShim 实现，并更新 Cargo.toml 中的 feature。kernel 源代码无需任何修改，即可无缝切换到 UEFI 启动。

**替代方案 & 否决理由**：kernel 内部模块 → kernel 永远依赖 uefi crate；纯 feature gate 无 trait → 调用者直接依赖具体函数，无法 mock 测试，新增引导方式时编译器不强制检查接口一致性；条件编译 `#[cfg(uefi)]` 散落代码中 → 加新引导方式必须到处加 `#[cfg]`，违反解耦原则。

#### C vs Rust 启动流程对照

```
Minix3 C (x86-32):                    minix-rs (x86-64):
BIOS → GRUB → head.S → pre_init()    固件(UEFI/OpenSBI) → boot-shim crate
         │         │                       │
         │         ├─ 验证魔数 0x2BADB002   ├─ GetMemoryMap()
         │         ├─ get_parameters()      ├─ AllocatePages()
         │         ├─ pg_identity()         ├─ 从 ESP 分区加载 kernel ELF + boot 模块
         │         ├─ pg_mapkernel()        ├─ 构建 KernelInfo（含 boot_modules 列表）
         │         │                       └─ ExitBootServices()
         │         ├─ pg_load()             │
         │         └─ vm_enable_paging()    └─ arch_boot(kernel_info, paging) → 建立新页表
         │                                     │     └─ paging.enable() → 切换 CR3
         │                                     └─ HigherHalf::jump_to_kmain() → kmain()
         └─ kmain(&kinfo)

注意：GRUB 跳转时分页未开启（CR0.PG=0），内核需要自己建页表并开分页；
UEFI 跳转时分页已开启（x86-64 长模式硬件要求分页），CR3 中是 UEFI 建的恒等映射，
boot-shim 在此映射下运行；arch_boot_impl() 构建新页表后，paging.enable() 切换 CR3。
```

**核心变化**：C 版本的 `head.S` + `pre_init()` 做的事（解析引导参数、建页表、开分页），在 Rust 版本中被 `boot-shim` crate 完成。`boot-shim` 是独立 crate，通过 `BootShim` trait 分离 UEFI 和 OpenSBI 实现——kernel 只依赖 trait，不依赖任何固件类型，换引导协议只需换 boot-shim 的 feature，kernel 一行不改。

### 3.3 页表结构：2 级 → 4 级（架构演进）

**C 源码依据**：§2.3.5 `pg_identity()`、§2.3.6 `pg_mapkernel()` — 32 位 x86 使用 2 级页表（PD+PT），4MB 大页映射。

**决策**：minix-rs 使用 4 级页表（x86-64 PML4+PDPT+PD+PT），页表分配从编译时固定改为运行时按需分配。

|  | Minix3 C（x86-32） | minix-rs（x86-64） |
|------|-------------------|-------------------|
| 页表层级 | 2 级（PD + PT） | 4 级（PML4 + PDPT + PD + PT） |
| 页目录条目 | `pagedir[1024]` 固定数组 | PML4/PDPT/PD 都是 512 条目的动态分配 |
| 大页大小 | 4MB（`I386_BIG_PAGE_SIZE`） | 2MB 或 1GB（`PSE` 或 `PSE-1GB`） |
| 地址空间 | 4GB（32 位，实际截断到 `LIMIT 0xFFFFF000`） | 48 位虚拟地址，物理可达 52 位 |
| PGE/PAE | 可选 | 必需（x86-64 要求 PAE） |
| 页表分配 | 编译时固定分配 6 个页表（`pagetables[6][1024]`，用完 panic） | 运行时按需分配（从 memmap 空闲区域 bump 分配物理页帧，通过 UEFI 恒等映射写入） |

> **bump 分配器**：一种最简单的线性内存分配器——维护一个指针（`next`），每次分配时向前推进所需大小，不回收、不合并。适合 boot 阶段这种"只分配不释放"的场景。详见 §4.3.1。

### 3.4 Paging trait 统一 —— 启动和运行时用同一个 trait

**C 源码依据**：§2.3.5 `pg_identity()`、§2.3.6 `pg_mapkernel()`、§2.3.7 `vm_enable_paging()` — C 版本无统一抽象。

**决策**：boot 阶段的页表操作使用 `Paging` + `HugePages` trait，不定义单独的 `PageTableBoot` trait。大页参数（`HUGE_PAGE_SIZE`）从 `HugePages: Paging` 扩展 trait 获取，而非放在 `Paging` 本身。

```rust
// C 的行为                    // Paging + HugePages trait 方法
pg_identity()                  → paging.map_huge(0, 0, HUGE_SIZE, W | X) × N
pg_mapkernel()                 → paging.map_huge(kern_virt, kern_phys, HUGE_SIZE, W | X)
pg_load() + vm_enable_paging()→ paging.enable()
alloc_pagetable()              → 不在 trait 中（boot-shim 从 UEFI 分配 root page）
```

**boot 阶段用到的 trait 方法**（以下仅列出 boot 阶段使用的方法，完整定义见 02-stage-vm/06-pagetable-struct.md §3.4，包括 destroy、remap、update_flags、root_paddr、switch、flush_tlb、flush_tlb_addr、map_range、unmap_range 等运行时方法）：

```rust
// Paging trait — 完整定义见 02-stage-vm/06-pagetable-struct.md §3.4
pub trait Paging {
    const PAGE_SIZE: usize;
    fn new() -> Result<Self, PageTableError> where Self: Sized;
    fn map(&mut self, vaddr, paddr, flags) -> Result<(), PageTableError>;
    fn unmap(&mut self, vaddr) -> Result<PhysBytes, PageTableError>;
    fn query(&self, vaddr) -> Option<(PhysBytes, PageFlags)>;

    /// Create from a known physical page (during boot, no global allocator).
    /// Unlike `new()`, this takes a pre-allocated page from the caller.
    /// C 对应: pagedir[1024] + pg_clear() — pg_utils.c 静态页目录
    fn new_from_page(root_page: PhysBytes) -> Self;

    /// Load root table and enable MMU.
    /// C 对应: pg_load() + vm_enable_paging() — pg_utils.c:204,247
    /// # Safety
    /// Caller must have set up identity mapping covering current RIP.
    unsafe fn enable(&self) -> PhysBytes;
}

// HugePages trait — 完整定义见 02-stage-vm/06-pagetable-struct.md §3.4
pub trait HugePages: Paging {
    const HUGE_PAGE_SIZES: &'static [usize];
    const HUGE_PAGE_SIZE: u64;           // Direct Map 首选大页大小
    const HUGE_PAGE_SHIFT: u32;
    const FALLBACK_HUGE_PAGE_SIZE: u64;  // 首选不可用时的回退
    const PTE_HUGE_IDENTIFIER_BIT: u64;

    fn map_huge(&mut self, vaddr: VirBytes, paddr: PhysBytes,
                size: usize, flags: PageFlags) -> Result<(), PageTableError>;
    fn supports_huge_page(size: usize) -> bool { ... }
    fn supports_1gb_page() -> bool { ... }
}
```

**为什么 `HUGE_PAGE_SIZE` 放在 `HugePages` 而非 `Paging`**：
- `HugePages` 是 `Paging` 的超集 trait（`HugePages: Paging`），boot 阶段用 `P: HugePages` bound 即可同时访问两者
- 所有目标架构（x86-64/ARM64/RISC-V）都实现了 `HugePages`，boot 阶段大页是必须的（C 源码 `pg_identity()` 和 `pg_mapkernel()` 都用 4MB 大页）
- `map_huge()`、`supports_1gb_page()`、`PTE_HUGE_IDENTIFIER_BIT` 等高级操作保留在扩展 trait，保持 `Paging` 核心职责清晰
- 详见 02-stage-vm/06-pagetable-struct.md §3.4 和 §5.3.1 的 trait 职责划分

**页表分配替代 C 的静态池**：Minix3 C 使用 `pagetables[6][1024]` 静态池 + `pg_alloc_page()` 从 memmap 尾部分配。minix-rs 的 `Paging::map()` 和 `HugePages::map_huge()` 内部自行管理页表页的分配——从 `KernelInfo.memmap` 描述的空闲区域中取物理页（简单 bump 分配），通过 UEFI 留下的恒等映射写入（切换 CR3 前仍使用 UEFI 页表，详见 §4.6 `arch_boot_impl` 执行顺序）。不需要 C 的静态池，因为 64 位地址空间需要更多页表页，静态池的固定大小不再适用。

**为什么采用 trait 而不是具体类型**：
- 三种架构的页表实现完全不同（x86-64 PML4、aarch64 TTBR1、riscv64 Sv39），适合用多态统一
- `arch_boot<P: HugePages>()` 只依赖 trait bound，不耦合任何具体类型——测试时传 mock 实现即可验证启动逻辑
- trait 描述的是"做什么"（映射页面、开启 MMU），隐藏了"怎么做"（CR3/TTBR0/satp），kernel 上层代码不需要关心硬件细节

**否决的替代方案**：

| 方案 | 否决原因 |
|------|---------|
| `PageTableBoot` trait | 与 `Paging::map/query` 重复，同一硬件两个 trait 说不通 |
| `HUGE_PAGE_SIZE` 移入 `Paging` | 违反 02-stage-vm 的 trait 职责划分（大页是可选能力，非核心 Paging） |

### 3.5 KernelInfo 字段精简

**C 源码依据**：§2.2.1 `kinfo_t` — 26 字段。

**决策**：Rust 保留 12 字段（`memmap`、`kern_virt_base`、`kern_phys_base`、`kern_size`、`free_upper_idx`、`user_sp`、`kern_stack_top`、`syscall_entry`、`boot_modules`、`bootstrap_start`、`bootstrap_len`、`platform_descriptor`）。

| 删除/重命名的 C 字段 | 理由 |
|--------------|------|
| `mbi` | UEFI 不需要 raw multiboot |
| `module_list[]` | → `boot_modules: &'static [BootModule]` |
| `memmap[]` | → `memmap: &'static [MemoryRegion]` |
| `mem_high_phys` | 可从 `memmap.last().end` 推导 |
| `mmap_size` | `&[MemoryRegion]` 自带长度 |
| `mods_with_kernel` | UEFI 不区分 |
| `kern_mod` | UEFI 不区分 |
| `freepde_start` | Paging trait 内部管理 |
| `param_buf[]` | UEFI LoadOption |
| `kmessages` | 独立 log 子系统 |
| `do_serial_debug` / `serial_debug_baud` | 独立 debug 模块 |
| `minix_panicing` | 独立 panic handler |
| `user_end` | 运行时从 memmap 计算 |
| `vir_kern_start` | → `kern_virt_base` |
| `boot_procs[]` | 进程管理子系统集成测试构造 |
| `nr_procs` / `nr_tasks` | `boot_modules.len()` 或运行时计算 |
| `release[]` / `version[]` | 尚未实现，计划通过 Cargo.toml 的 `version` 字段 + `env!("CARGO_PKG_VERSION")` 或独立版本模块提供 |
| `vm_allocated_bytes` | 运行时从 memmap 计算 |
| `kernel_allocated_bytes` / `kernel_allocated_bytes_dynamic` | 根据 memmap 动态计算 |

**保留的 C 字段（名称微调）**：

| C 字段 | Rust 字段 | 理由 |
|--------|----------|------|
| `bootstrap_start` | `bootstrap_start: PhysBytes` | boot-shim 仍需向内核报告自身物理范围，以便后续回收；UEFI/OpenSBI 路径目前用 `PhysBytes(0)` 占位（见 `uefi_helpers.rs:132`、`opensbi_helpers.rs:229`），但字段保留 |
| `bootstrap_len` | `bootstrap_len: u64` | 与 `bootstrap_start` 配对，目前用 `kern_phys_base.0` 作为上界近似，后续需精确化 |

**否决的替代方案**：

| 方案 | 否决原因 |
|------|---------|
| 保留全部字段（1:1 翻译 `kinfo_t`） | UEFI 引导路径不产生 `mbi`/`param_buf` 等字段，强行保留需要 `Option<>` 包装，增加无意义复杂度 |
| 保留 `mem_high_phys` 单独字段 | 可从 `memmap.last().end` 推导，冗余字段违反单一数据源原则 |

`free_upper_idx` 统一起名（x86-64 = PML4 索引，aarch64 = TTBR1 L0 索引，riscv64 = VPN[2] 上界）。类型为 `Option<usize>`：`None` 表示 boot-shim 尚未计算（C 中由 `pg_mapkernel()` 返回值设置），`Some(idx)` 表示已计算。

**新增字段**（C 版 `kinfo_t` 中无直接对应，但信息来源可追溯）：

| 新增字段 | C 版信息来源 | 理由 |
|---------|------------|------|
| `kern_stack_top` | 链接器符号 `k_initial_stktop`（head.S:80 `mov $k_initial_stktop, %esp`），C 版通过 `tss_init(0, &k_boot_stktop)` 在 `protect.c:338` 使用 | C 版通过全局链接器符号隐式传递，Rust 版将其显式纳入 `KernelInfo`，使 boot-shim → kernel 的数据传递完全通过结构体完成，不依赖链接器符号 |
| `syscall_entry` | `protect.c:189-205` 中 LSTAR MSR 配置的入口地址 | C 版在 `prot_init()` 中硬编码计算，Rust 版将其作为元信息显式传入，便于 `arch_boot_impl` 统一配置 |
| `platform_descriptor` | C 版 `kinfo_t` 无直接对应；信息来自 UEFI Configuration Table（ACPI RSDP/DTB）或 OpenSBI a1 寄存器 | 平台发现需要原始固件描述符指针；通过 `KernelInfo.platform_descriptor: Option<PlatformDescriptorPtr>`（Dtb/Rsdp）从 boot-shim 传递到 `minix-platform::init_from_kinfo`，见 [04-platform-discovery.md](04-platform-discovery.md) §4.4 与 §4.6 |

**`overlaps()` 的 Rust 等价**：C 源码 `overlaps()`（pre_init.c:77）检查 boot 模块是否与内核镜像重叠。UEFI 引导路径中，`boot-shim` 的 `uefi_helpers` 通过 `GetMemoryMap()` 获取的内存描述已由 UEFI 固件保证不重叠，因此不需要 Rust 等价函数。OpenSBI 路径中，U-Boot 通过 `fatload` 将 kernel/modules 预加载到指定地址，bump 分配器从 `DRAM + 32MB` 起步避开已加载区域，同样不需要重叠检测。如果未来支持非 UEFI 引导（如 coreboot），需在对应 boot-shim feature module 中实现重叠检测。

### 3.6 4GB 截断删除（架构演进）

**C 源码依据**：§2.3.3 `add_memmap()` — `LIMIT 0xFFFFF000`。

**决策**：Rust 删除 `LIMIT`。64 位有 48 位虚拟 + 52 位物理地址，不需要截断。

### 3.7 集中定义，接口维度分散

`Paging` trait 定义在 `os/arch/src/paging.rs`，`HugePages` trait 定义在 `os/arch/src/paging_ext.rs`（集中定义），各架构实现在 `arch/x86_64/`、`arch/aarch64/`、`arch/riscv64/` 集中。kernel 只依赖 trait，不引用具体类型。

"分散"体现在接口维度而非物理位置：`Paging` 是基础能力（map/unmap/query），`HugePages: Paging` 是大页扩展（map_huge/HUGE_PAGE_SIZE）。消费者按需绑定——boot 阶段用 `P: HugePages`，运行时只用 `P: Paging`。Paging 是横切关注点（vm、kernel、boot 都依赖），没有单一"最近消费者"，因此集中定义更自然。

### 3.8 三层 crate 依赖

```
boot-shim   → minix-boot, minix-arch, minix-kernel  # boot-shim 的 uefi feature 还依赖外部库 `uefi = "0.33"`（提供 UEFI 类型和 BootServices）
minix-boot  → minix-types                             # minix-boot 定义 KernelInfo、BootShim trait 等共享类型
kernel      → minix-boot, minix-arch                  # kernel 不依赖 UEFI 外部库，也不依赖 boot-shim
minix-arch  → minix-types                             # minix-arch 定义 Paging trait，无 UEFI 依赖
```

kernel 不依赖 boot-shim，但 boot-shim 依赖 kernel——因为 `arch_boot_impl()` 定义在 kernel 中，boot-shim 调用它完成页表映射和分页开启。KernelInfo、BootShim trait 等共享类型定义在 `minix-boot` crate 中（`minix-boot` 依赖 `minix-types` 提供的基础类型如 `PhysBytes`/`VirBytes`），boot-shim 和 kernel 通过 `minix-boot` 共享数据结构。

#### minix-rs 启动链路总览（UEFI/OpenSBI → boot-shim → 内核）

```
固件加载 boot-shim (UEFI: .efi / OpenSBI: 裸金属)
  │
  ├── UEFI/OpenSBI 固件初始化
  │     ├── POST + 初始化 DXE 驱动（UEFI）/ 硬件初始化（OpenSBI）
  │     ├── 查找启动项
  │     └── 加载 boot-shim 到内存
  │
  ├── boot-shim crate 执行
  │     ├── GetMemoryMap() / 硬编码 memmap → 获取物理内存布局
  │     ├── AllocatePages() / bump 分配 → 分配根页表页
  │     ├── 加载 kernel ELF（解析 ELF → 按 p_paddr 拷贝段 → BSS 清零）
  │     ├── 加载 boot 模块（VM/PM/VFS/RS 等）
  │     ├── 构建 KernelInfo
  │     └── ExitBootServices()（UEFI）/ 直接调用（OpenSBI）
  │         调用 arch_boot()
  │
  └── 内核（minix-rs 源码从这里开始）
        │
        ├── arch_boot()
        │     ├── arch_boot_impl() → 建立恒等映射 + 内核高地址映射
        │     ├── Paging::enable() → 切换页表基址
        │     └── return &KernelInfo
        │
        └── HigherHalf::jump_to_kmain() → kmain()
              └── kmain() → 初始化进程表 → ...
```

**关键交接点（Rust 版）**：固件 → boot-shim 的交接通过 **标准可执行格式**完成（UEFI 用 PE/COFF，OpenSBI 用裸金属二进制）。boot-shim → 内核的交接通过 **`arch_boot()` 函数调用**完成——`KernelInfo` 和根页表物理地址作为参数传递，这是 boot-shim 与内核之间唯一的契约。

> **核心差异**：C 版本中 GRUB 替内核完成了所有磁盘 I/O——`multiboot` 命令加载内核，`load_mods` 命令将模块读入物理内存，内核拿到的 `multiboot_info_t` 已包含模块地址列表。Rust 版本中，UEFI 只加载 boot-shim 自身，kernel ELF 和 VM/PM 等模块必须由 boot-shim 通过文件系统协议从 ESP 分区显式读取。C 版本的"从磁盘搬到内存"由 GRUB 代劳，Rust 版本由 boot-shim 自己完成。

---

## 4. 实现详解

> 每个结构的引导语解释核心思路，代码注释标注 C 源码对应。
> 实际代码路径见各节标注。当前代码可通过 `cargo test -p minix-kernel --test boot_integration` 验证。

以下按**模块职责**组织：先介绍 boot-shim 与 kernel 之间的共享数据结构（§4.1）和页表接口（§4.2~4.3），再展开 boot-shim 的具体实现（§4.5），最后是 kernel 入口（§4.6）和 C/Rust 对照（§4.7）。

若你想**按启动时序**阅读，建议顺序为：§4.5（boot-shim 准备）→ §4.1（KernelInfo 构造）→ §4.2/4.3（页表接口）→ §4.6（arch_boot 入口）→ §4.7（对照表）。

```
固件加载 boot-shim (UEFI: .efi / OpenSBI: 裸金属)
  │
  ├── §4.5 boot-shim crate: 获取内存布局、分配页表根页、构造 KernelInfo
  │     ├── uefi_helpers (feature = "uefi"): UEFI GetMemoryMap + 从 ESP 加载 kernel + boot 模块 + ExitBootServices
  │     └── opensbi_helpers (feature = "opensbi"): U-Boot 通过 fatload 预加载文件到 RAM，传递 BootFileTable（a0 寄存器）。boot-shim 以 UbootFileLoader 读取，走与 UEFI 路径相同的 ELF 加载流程（parse + 段拷贝 + BSS 清零）。硬编码 QEMU virt 内存映射 + bump 分配器，无需 UEFI 依赖。
  │
  ├── ↑ boot-shim 的核心职责：从固件/磁盘加载 kernel + boot 模块、构造 KernelInfo、退出固件服务
  │
  ├── §4.1 KernelInfo: 引导层与内核之间的共享数据结构
  │
  ├── §4.2 Paging trait + HugePages: 页表管理接口
  │     └── §4.3 x86-64 架构实现（arm64/riscv64 位置见节内指引）
  │           └── §4.3.1 pt_alloc: 中间页表页分配
  │
  ├── §4.6 kernel 入口: arch_boot → kmain
  │
  └── §4.7 与 Minix3 C 的函数对照
```

### 4.1 KernelInfo — C 的 kinfo_t 对应

> 设计决策：§3.5, §3.6
> 实际文件：`os/libs/minix-boot/src/kernel_info.rs`

```rust
// minix-boot crate 中定义 — boot-shim 和 kernel 共享
pub struct KernelInfo {
    pub memmap: &'static [MemoryRegion],       // 空闲物理内存区域
    pub kern_virt_base: VirBytes,               // 内核虚拟基址
    pub kern_phys_base: PhysBytes,              // 内核物理基址
    pub kern_size: u64,                         // 内核总大小（字节）。u64 与地址类型（PhysBytes/VirBytes）保持一致，避免 kern_virt_base + kern_size 等运算时类型转换
    pub free_upper_idx: Option<usize>,          // 页表根级第一个空闲索引。None = boot-shim 尚未计算（C 由 pg_mapkernel() 返回值设置）
    pub user_sp: VirBytes,                      // 用户栈顶地址
    pub kern_stack_top: VirBytes,               // 内核初始栈顶（虚拟地址），HigherHalf 切栈用
    pub syscall_entry: VirBytes,                // 系统调用处理程序入口虚拟地址（= kern_virt_base + offset）。内核元信息，所有架构一致记录；仅 x86-64 用它配置 LSTAR MSR，aarch64/riscv64 编译时确定入口，此字段仅作参考
    pub boot_modules: &'static [BootModule],    // 启动模块列表（PM/VM/VFS 等）
    pub bootstrap_start: PhysBytes,             // boot-shim 自身物理起始地址（当前 UEFI/OpenSBI 路径用 PhysBytes(0) 占位）
    pub bootstrap_len: u64,                     // boot-shim 自身物理长度（当前用 kern_phys_base.0 作为上界近似）
    pub platform_descriptor: Option<PlatformDescriptorPtr>, // 平台描述符原始指针（ACPI RSDP / DTB），None 时内核回退到 QemuVirtDesc
}

pub struct MemoryRegion {
    pub base: PhysBytes,    // 物理起始地址
    pub len: usize,         // 长度（字节）
}

pub struct BootModule {
    pub name: &'static str, // 模块名（如 "pm", "vm"）
    pub start: PhysBytes,   // 物理起始地址
    pub len: usize,         // 长度（字节）
}

pub enum PlatformDescriptorPtr {
    Dtb(PhysBytes),  // 扁平设备树（ARM64 / RISC-V）物理地址
    Rsdp(PhysBytes), // ACPI RSDP（x86-64）物理地址
}
```

> **TODO（review 04-platform-discovery.md 时一并处理）**：当前 `PlatformDescriptorPtr { Dtb, Rsdp }` 把固件描述符的两类来源以 `pub enum` 形式直接暴露在 `KernelInfo` 公共 API 表面，与"上层代码完全屏蔽设备差异"的设计哲学有违和（详见本文档 §4.1 上方讨论）。
>
> 拟修改方向：拆分为 `platform_descriptor: PhysBytes`（裸指针）+ `platform_source: PlatformSource`（私有 enum，仅 `init_from_kinfo` 内部 match），或仅把 `PlatformSource` 收为 boot-shim crate 私有类型，使上层 `platform_desc()` 之后完全看不到 DTB/ACPI 来源。
>
> 影响范围：`os/boot-shim/src/kernel_info.rs:78-118`、`os/libs/minix-platform/src/global.rs::init_from_kinfo`、`04-platform-discovery.md` §4.4。当前由 `PlatformDescriptorPtr` 引用的所有调用点（本文档第 794、1285 行）均需同步更新。
> 触发条件：本 review 通过 `minix-platform::init_from_kinfo` 完成首次全链路验证后、进入 §4.4 重构前。

> **TODO（review 01 本文档 §4.5+ 时一并处理）**：`os/boot-shim/src/opensbi_helpers.rs:229` 留有真 TODO —— `PhysBytes(0), // TODO: determine boot-shim physical start from OpenSBI`。该字段用于 `kinfo.bootstrap_start/len` 告知 kernel "boot-shim 占用的内存请回收"，若写 0 会让 kernel 把整段 RAM 误标记为已用 / 未用。需要从 OpenSBI 的 ELF program headers（`_start` 到 `_edata`）导出符号，或在 entry trampoline 把 `a0`/`sp` 推断得到。这**是真实未完成实现**，不是 mock。
>
> 同节还有第 1203-1204 行注释（"riscv64 没有 UEFI 入口——它的入口是裸金属的 `_start` 汇编"）措辞过简，未区分 UEFI 不可达（`riscv64-unknown-uefi` target 不存在）/ 生产可用（OpenSBI + U-Boot fatload）/ 测试场景（QEMU `-kernel`）三个维度，触发本节重构时一并改写。
> 触发条件：boot-shim 在 OpenSBI+U-Boot 真实启动链验证后（当前仅在 QEMU `-kernel` 测试场景验证）。

> **TODO（review 02-higher-half-kernel.md §5.1 时一并处理）**：当前 02 §5.1 三架构 5 行测试对照表（hello-boot / test-memmap / test-paging-enable / test-kernel-map / test-higher-half）与本文档 §5 表存在跨文档重复——其中 hello-boot / test-memmap / test-paging-enable / test-kernel-map 本属 boot-shim 后端验证，**应只在本文档 §5 表出现**；02 §5.1 应只保留 test-higher-half 及专属的高半核一致性测试。
>
> 影响范围：[02-higher-half-kernel.md §5.1](02-higher-half-kernel.md#L868-L876) 表格行 1-4（删除并提示见本文档）、行 5（保留 + 增补 Sv39 canonical 内容）。02 §5.1 表下方"运行方式"段需同步迁移到本文档 §5。
> 触发条件：02 文档 review 时一并处理。本任务已经将本文档 §5 表对应行移除（避免冗余）。

> **TODO（补 QEMU + OpenSBI + U-Boot 真实启动链集成测试）**：[§5 架构差异要点表](01-boot-shim-bootstrap.md#L1573-L1580) `kern_virt_base` 行暴露 riscv64 在 QEMU `-kernel` 测试场景下未完成 ELF 装载 + 高半核切换的临时妥协。本文档目标读者希望测试尽可能模拟生产环境，应当补一个真实生产链路的集成测试。
>
> **目标**：在 `os/qemu-tests/` 下新增 `test-sbi-uboot`（暂名），验证完整 OpenSBI + U-Boot + boot-shim + kernel 链路：
>
> 1. QEMU `-machine virt` + `-bios u-boot.bin` + `-drive file=uboot.pflash`
> 2. U-Boot 通过 `boot.cmd` 走 `fatload` 把 `kernel.elf` 与各模块预加载到 `0x8020_0000` + 偏移地址，并构造 `BootFileTable` 写入 `a0`
> 3. `OpenSbiBootShim::prepare_boot()` 调用全链路：U-Boot `fatload` → `BootFileTable` 解析 → ELF 装载（`load_kernel_with_loader`） → `build_kernel_info` → `arch_boot_impl`
> 4. 验证 `kern_virt_base == 0xFFFF_FFC0_0000_0000`（Sv39 canonical high）切栈成功，最终 SP 在高地址、kernel 通过高地址执行
>
> **当前覆盖缺口**：`opensbi_helpers.rs::OpenSbiBootShim::prepare_boot`（[L193-L244](os/boot-shim/src/opensbi_helpers.rs#L193-L244)）的 `build_memmap`、`alloc_root_page`、`alloc_bump_region`、`build_kernel_info`、`load_boot_modules_with_loader` 等子例程在生产路径上**完全没有测试覆盖**。
>
> **阻塞依赖**：
> - [os/boot-shim/src/opensbi_helpers.rs:229](os/boot-shim/src/opensbi_helpers.rs#L229) — `PhysBytes(0)` TODO 必须先解决（否则 kernel 内存回收有 bug）
> - [02-higher-half-kernel.md §4.2](02-higher-half-kernel.md#42-elf-加载实现) — riscv64 ELF 高地址链接脚本需就绪（当前仅 x86-64/aarch64 配置完整）
>
> **实施要素**：
> - 构建 `u-boot.bin`（含 `CONFIG_EFI_LOADER=y` 或纯 extlinux 配置）与 `boot.cmd`（含 `fatload` + `minix_elf` 命令序列）
> - 构建包含 `kernel.elf` + 各模块的 FAT 镜像（`mkfs.vfat` + `mcopy`）
> - 在 `os/qemu-tests/run_all.sh` 中追加 `test-sbi-uboot` 入口
>
> **验收**：CI 中 `test-sbi-uboot` 通过等价于"boot-shim 全链路在 QEMU virt 上真实启动验证 OK"，并将本文档 §5 表 `kern_virt_base（当前测试场景）` 列升级为 `与生产路径一致`，删除"妥协"说明。
> 触发条件：本 review 通过后、未实现前该 TODO 不被覆盖。


### 4.2 Paging trait 扩展 + HugePages trait

> 设计决策：§3.4
> 实际文件：`os/arch/src/paging.rs`：包含 `fn new_from_page(root_page: PhysBytes) -> Self`、`unsafe fn enable(&self) -> PhysBytes`
> 实际文件：`os/arch/src/paging_ext.rs`：包含 `HugePages` trait（`HUGE_PAGE_SIZE`、`map_huge()` 等）

`Paging` trait 提供 `new_from_page`（用外部传入的物理页创建根页表）和 `enable`（加载页表基址到 MMU 并开启分页）。`HugePages: Paging` 扩展 trait 提供大页映射能力，boot 阶段通过 `P: HugePages` bound 同时访问两者。各架构汇编差异见 trait 方法文档注释（x86-64 `mov cr3`、aarch64 `msr TTBR1_EL1`、riscv64 `csrw satp`）。

### 4.3 x86-64 架构实现

ARM64 和 RISC-V 实现位于 `os/arch/src/arm64/paging.rs` 和 `os/arch/src/riscv64/paging.rs`，结构与 x86-64 一致，差异仅在 MMU 控制指令（`msr` / `csrw`）和页表层级。

```rust
impl Paging for X86_64Paging {
    const PAGE_SIZE: usize = 4096;

    fn new_from_page(root_page: PhysBytes) -> Self {
        // 清零根页表（PML4）的 512 个条目，记录物理地址
        // new() 从内核分配器取页，new_from_page 用 UEFI/调用者传入的页
        let ptr = unsafe { phys_to_ptr(root_page.0) };
        unsafe { core::ptr::write_bytes(ptr, 0, 512) };
        Self { root_paddr: root_page.0 }
    }

    unsafe fn enable(&self) -> PhysBytes {
        unsafe {
            // 切换 CR3 到内核自己的页表
            // UEFI 已运行在长模式（CR0.PG=1），但用的是 UEFI 自己的页表
            asm!("mov cr3, {}", in(reg) self.root_paddr);
            // 确保 WP（CR0 bit 16）已设置，使内核不能写只读页（W^X）
            // UEFI 固件不保证 WP 已开启
            let mut cr0: u64;
            asm!("mov {}, cr0", out(reg) cr0);
            cr0 |= 1 << 31;   // PG = 1 开启分页（UEFI 已开，此处防御性重设）
            cr0 |= 1 << 16;   // WP = 1 写保护
            asm!("mov cr0, {}", in(reg) cr0);
            // x86-64 长模式下，2MB 大页（PD.PS=1）和 1GB 大页（PDPT.PS=1）
            // 均由页表项自身的 PS 位控制，不需要 CR4.PSE（PSE 是 32 位保护模式
            // 下启用 4MB 页的历史特性，64 位长模式已废弃此依赖）。
        }
        PhysBytes(self.root_paddr)
    }
    // map/unmap/query/new — 运行时方法
}

impl HugePages for X86_64Paging {
    const HUGE_PAGE_SIZES: &'static [usize] = &[1 << 30, 1 << 21]; // 1GB, 2MB
    const HUGE_PAGE_SIZE: u64 = 1 << 30;           // 首选 1GB
    const HUGE_PAGE_SHIFT: u32 = 30;
    const FALLBACK_HUGE_PAGE_SIZE: u64 = 1 << 21;  // 回退 2MB
    const PTE_HUGE_IDENTIFIER_BIT: u64 = 1 << 7;    // PS bit

    fn map_huge(&mut self, vaddr: VirBytes, paddr: PhysBytes,
                size: usize, flags: PageFlags) -> Result<(), PageTableError> {
        // 1GB 大页：PML4 → PDPT（直接写 1GB 块描述符，跳过后两级）
        // 2MB 大页：PML4 → PDPT → PD（直接写 2MB 块描述符，跳过末级）
        // 中间页表按需分配（调用 arch::pt_alloc::alloc_pt_page()）
        // 最后写入 PTE，包含物理地址和权限位（见 flags_to_pte）
    }
}
```

三个架构的 `map_huge` 都通过 `arch::pt_alloc::alloc_pt_page()` 分配中间页表页。页表页分配器的设计见 §4.3.1。

### 4.3.1 页表页分配器 — pt_alloc

> 实际文件：`os/arch/src/pt_alloc.rs`
> 设计决策：§3.2（页表分配器与 VM 解耦）

`map_huge` 在 walk 页表时，如果中间级（PML4→PDPT→PD）的页表页不存在，需要分配物理页来放新的页表。这个物理页从哪来？Boot 和 VM 的策略不同：

| 阶段 | 分配策略 | VA↔PA |
|------|---------|-------|
| Boot | Bump（恒等映射区域） | VA = PA |
| VM | 全局 `VmPageAllocator` | VA = DM_BASE + PA |

**`pt_alloc` 只做注册和转发**，不包含任何分配器实现。它是一个薄层（~60 行）：

```rust
// arch/src/pt_alloc.rs
type PtAllocFn = fn() -> Result<(PhysBytes, VirBytes), PageTableError>;
static mut PT_ALLOC: PtAllocFn = uninit_alloc;

pub fn register(alloc_fn: fn() -> ...) { PT_ALLOC = alloc_fn; }
pub fn alloc_pt_page() -> Result<...> { PT_ALLOC() }
```

函数指针的签名是纯 `fn()`——不携带 Boot 或 VM 的冗余信息。分配器实现由调用者提供：

```rust
// hello-boot（调用者）
static mut BOOT_PT_NEXT: u64 = 0;
static mut BOOT_PT_END: u64 = 0;

fn boot_pt_alloc() -> Result<(PhysBytes, VirBytes), PageTableError> {
    unsafe {
        if BOOT_PT_NEXT >= BOOT_PT_END { return Err(...); }
        let pa = BOOT_PT_NEXT;
        BOOT_PT_NEXT += 0x1000;
        core::ptr::write_bytes(pa as *mut u64, 0, 512);
        Ok((PhysBytes(pa), VirBytes(pa)))
    }
}

// 使用前注册
unsafe { BOOT_PT_NEXT = base; BOOT_PT_END = end; }
pt_alloc::register(boot_pt_alloc);

// Paging 内部调用（三架构统一）
let (phys, _virt) = pt_alloc::alloc_pt_page()?;
```

**生命周期**：分配器注册随时间演进，不同阶段使用不同实现：

```
时间 →    boot-shim/main.rs              kernel::arch_boot_impl     VM init
          ─────────────────────          ─────────────────────     ──────────
PT_ALLOC  boot_pt_alloc                  boot_pt_alloc             vm_pt_alloc
          ↑ register()                   ↑ is_registered()=true    ↑ register()
            (调用方显式注册)               → 跳过重复注册
```

**两种注册路径**及其设计理由：

1. **调用方显式注册**（当前 `boot-shim/main.rs` + 所有测试内核）：`boot_alloc::init_boot_pt_alloc(base, end)` + `pt_alloc::register(boot_pt_alloc)`
2. **`arch_boot_impl` 自动注册**（兜底）：如果 `is_registered()` 返回 false，从 memmap 第一个区域取 1MB 作为 bump 区域

理由：boot-shim 只负责"从 ESP 加载 kernel + boot 模块、拿根页面、调 arch_boot"，页表的具体操作（包括中间页表页分配）由 kernel 完成（职责分离）；`arch_boot_impl` 兜底保障鲁棒性——即使调用方忘了注册，内核也能自动初始化。

**代码位置**：`boot_pt_alloc` 定义在 `kernel/src/boot_alloc.rs`，测试内核和 boot-shim 都通过 `use minix_kernel::boot_alloc` 直接引用生产代码：

```rust
// kernel/src/boot_alloc.rs（生产代码）
pub fn init_boot_pt_alloc(base: u64, end: u64) { ... }
fn boot_pt_alloc() -> Result<(PhysBytes, VirBytes), PageTableError> { ... }
```

VM 阶段时，`pt_alloc::register(vm_pt_alloc)` 替换分配器，同时 boot bump 范围从 memmap 切除，防止 VM 的 PhysAlloc 误用。bump 阶段分配的页表页（identity map + kernel map）永久有效、不会泄漏。

### 4.4 引导协议抽象 — BootShim trait

`BootShim` trait 定义在 `minix-boot` crate 的 `boot_shim.rs` 中，是引导层与内核层之间的接口。kernel 只认识这个 trait，不认识 UEFI/OpenSBI 等具体实现。`boot-shim` crate 提供 `UefiBootShim` 和 `OpenSbiBootShim` 两个实现，通过 Cargo.toml 的 feature gate 选择编译哪个——代码中不散落 `#[cfg]` 条件编译。

```rust
// os/libs/minix-boot/src/boot_shim.rs
pub trait BootShim {
    /// 完成固件相关的引导准备，返回内核所需的全部数据。
    /// 调用后固件服务（如 UEFI BootServices）已不可用。
    ///
    /// kernel 的物理/虚拟基址和大小由实现内部确定
    /// （UEFI 路径通过解析 kernel ELF 的 PT_LOAD 段得出，
    ///  OpenSBI 路径通过 QEMU -kernel 的已知加载地址得出）。
    /// 调用者只需指定 bump_pages（boot-stage 页表分配所需页数）。
    fn prepare_boot(bump_pages: usize) -> BootPrepareResult;
}

pub struct BootPrepareResult {
    pub kernel_info: KernelInfo,
    pub root_page: PhysBytes,
    pub bump_base: u64,
    pub bump_end: u64,
}
```

```rust
// os/boot-shim/src/lib.rs — re-export 和 feature gate 选择
#![cfg_attr(not(test), no_std)]

extern crate alloc;

// Re-export trait + result type — 调用者通过 trait 统一接口
pub use minix_boot::{BootShim, BootPrepareResult};

// Re-export shared ELF parser — boot-shim 和 kernel 都需要
pub use minix_elf;

#[cfg(feature = "uefi")]
pub mod uefi_helpers;

#[cfg(feature = "opensbi")]
pub mod opensbi_helpers;

// 按启用的 feature 导出具体实现
#[cfg(feature = "uefi")]
pub use uefi_helpers::UefiBootShim;

#[cfg(all(feature = "opensbi", not(feature = "uefi")))]
pub use opensbi_helpers::OpenSbiBootShim;
```

**为什么用 trait 而非纯 feature gate**：

1. **类型安全**：`UefiBootShim` 和 `OpenSbiBootShim` 是不同的类型，编译器保证你不会在 riscv64 上调用 UEFI 接口
2. **封装性**：固件实现细节（UEFI BootServices、OpenSBI 硬编码 memmap）封装在各自的 impl 中，kernel 完全不感知
3. **可测试性**：测试中可以 mock 一个 `MockBootShim` 实现 trait，不需要真实固件
4. **feature gate 仅在 Cargo.toml**：代码中的 `#[cfg]` 只出现在 `lib.rs` 的模块导出处（3 行），业务逻辑零条件编译

**调用关系**：

```
boot-shim crate (uefi feature)          kernel crate
     │                                        │
     │ UefiBootShim::prepare_boot()           │
     │   UEFI GetMemoryMap()                  │
     │   UEFI AllocatePages()                 │
     │   从 ESP 加载 kernel ELF + boot 模块   │
     │   构建 KernelInfo（含 boot_modules）    │
     │   UEFI ExitBootServices()              │
     │ arch_boot(result.kernel_info, ──────→│ arch_boot_impl::<X86_64Paging>(...)
     │          result.root_page)             │   kmain()
     │  （不再返回）                           │

boot-shim crate (opensbi feature)       kernel crate
     │                                        │
     │ OpenSbiBootShim::prepare_boot()        │
     │   硬编码 QEMU virt 内存映射             │
     │   bump 分配器分配页表页                 │
     │   构建 KernelInfo                       │
     │ arch_boot(result.kernel_info, ──────→│ arch_boot_impl::<Riscv64Paging>(...)
     │          result.root_page)             │   kmain()
     │  （不再返回）                           │
```

**解耦效果**：换引导协议只需在 Cargo.toml 中启用不同 feature（如 `opensbi`），kernel 代码零修改。调用者通过 `BootShim` trait 统一接口，不直接依赖任何固件类型。详见 §3.2。

### 4.5 boot-shim crate

上一节定义了 `BootShim` trait 接口，本节展示 `UefiBootShim` 和 `OpenSbiBootShim` 两个具体实现如何工作。

`boot-shim` 通过 `BootShim` trait 提供两种固件实现：

- **`uefi` feature**（默认启用）：`UefiBootShim` — 使用 UEFI BootServices 获取内存映射、分配页表根页、从 ESP 分区加载 kernel ELF 和 boot 模块、退出引导服务
- **`opensbi` feature**：`OpenSbiBootShim` — 使用硬编码的 QEMU virt 内存映射和 bump 分配器，无需 UEFI 依赖

两种实现都实现 `BootShim` trait，输出相同的 `BootPrepareResult`，kernel 不感知底层固件类型。

#### 4.5.1 UEFI 路径 (uefi_helpers)

> 设计决策：§3.1（跨架构引导抽象）

**UEFI 入口**：`boot-shim` 的 `main.rs` 是 UEFI 固件加载 `.efi` 后调用的入口函数。它通过 `BootShim` trait 调用 `UefiBootShim::prepare_boot()` 完成引导准备，然后将控制权交给内核。`main.rs` 本身不包含任何 UEFI 逻辑——所有固件细节封装在 `UefiBootShim` 的 trait 实现中。

```rust
// os/boot-shim/src/main.rs
use minix_types::BootShim;
use boot_shim::UefiBootShim;

#[entry]
fn main() -> Status {
    // 1. UEFI boot preparation via BootShim trait
    //    Internally: GetMemoryMap → AllocatePages → 从 ESP 加载 kernel ELF
    //    → 加载 boot 模块 → build KernelInfo → ExitBootServices
    //    kernel 的物理/虚拟基址和大小由 load_kernel_elf() 内部解析 ELF 得到，
    //    调用者只需指定 bump_pages（boot-stage 页表分配所需页数）。
    let result = UefiBootShim::prepare_boot(8);

    // ── 以下代码运行在裸金属环境，UEFI BootServices 已不可用 ──

    // 2. 注册 boot-stage 页表页分配器
    boot_alloc::init_boot_pt_alloc(result.bump_base, result.bump_end);
    minix_arch::pt_alloc::register(boot_alloc::boot_pt_alloc);

    // 3. 交控制权给内核的 arch_boot——建立页表、开启分页、进入 kmain
    //    此函数永不返回（-> !）
    minix_kernel::arch_boot(&result.kernel_info, result.root_page);

    unreachable!()
}
```

> **注意**：`main.rs` 只存在于 UEFI 目标（x86_64/aarch64）。riscv64 没有 UEFI 入口——它的入口是裸金属的 `_start` 汇编，直接跳入 `rust_main()`。在真实启动链中（OpenSBI → U-Boot → boot-shim），`rust_main` 会调用 `OpenSbiBootShim::prepare_boot()` 通过 `BootFileTable` 加载 kernel.elf；但在 QEMU `-kernel` 直接加载的测试场景中，没有 U-Boot 和文件系统，因此测试内核直接构造 `BootPrepareResult`，跳过 `BootShim` trait，仅验证 `arch_boot_impl` 路径。

**`uefi_helpers` 模块**将 UEFI 特有步骤拆分为可复用的独立函数，供 `UefiBootShim` trait 实现和测试内核共同使用：

```rust
// os/boot-shim/src/uefi_helpers.rs

/// 将 UEFI 内存映射转为 MemoryRegion 切片（仅含 CONVENTIONAL 内存）
/// 使用 Box::leak 获取 'static 生命周期——引导阶段只运行一次，可接受
pub fn build_memmap() -> &'static [MemoryRegion] { /* ... */ }

/// 分配一页物理内存作为根页表
/// 对应 Minix3 C 的 pagedir[1024]
pub fn alloc_root_page() -> PhysBytes { /* ... */ }

/// 分配 bump 区域供 boot_pt_alloc 使用
pub fn alloc_bump_region(num_pages: usize) -> (u64, u64) { /* ... */ }

/// 构造 KernelInfo
pub fn build_kernel_info(
    memmap, kern_virt_base, kern_phys_base, kern_size, boot_modules
) -> KernelInfo { /* ... */ }

/// 退出 UEFI BootServices——此后 AllocatePages 等不可用
pub fn exit_boot_services() { /* ... */ }

/// 从 ESP 分区加载 kernel ELF（实现详见 02-higher-half-kernel.md §4.2）
pub fn load_kernel_elf() -> Result<KernelLoadResult, ElfError> { /* ... */ }

/// 加载 boot 模块（实现详见 02-higher-half-kernel.md §4.2）
pub fn load_boot_modules() -> &'static [BootModule] { /* ... */ }

/// UEFI 引导的 BootShim 实现
pub struct UefiBootShim;

impl BootShim for UefiBootShim {
    fn prepare_boot(bump_pages: usize) -> BootPrepareResult {
        let memmap = build_memmap();
        let root_page = alloc_root_page();
        let (bump_base, bump_end) = alloc_bump_region(bump_pages);

        // 从 ESP 加载 kernel ELF——必须在 ExitBootServices 之前
        let kern = load_kernel_elf()
            .expect("Failed to load kernel ELF from ESP");

        // 加载 boot 模块（VM/PM/VFS/RS 等）
        let boot_modules = load_boot_modules();

        let kernel_info = build_kernel_info(
            memmap, kern.kern_virt_base, kern.kern_phys_base,
            kern.kern_size, boot_modules,
        );

        exit_boot_services();

        BootPrepareResult { kernel_info, root_page, bump_base, bump_end }
    }
}
```

> **设计决策**：
>
> 1. **`prepare_boot` 的封装边界**：`prepare_boot` 只暴露 `bump_pages` 一个参数。内核的物理/虚拟布局是 ELF 文件的固有属性，应由 `load_kernel_elf()` 内部解析 PT_LOAD 段自动得出，而非由调用者猜测或手动传入。ELF 解析与段拷贝的实现详见 [02-higher-half-kernel.md](02-higher-half-kernel.md) §4.2。
>
> 2. **函数粒度选择**：拆分为独立函数而非只提供 `UefiBootShim::prepare_boot` 一站式接口，是因为测试内核可能只需要其中部分功能（如 `build_memmap` 但不需要 `exit_boot_services`）。

#### 4.5.2 OpenSBI 路径 (opensbi_helpers)

> 设计意图：与 UEFI 路径在「ELF 解析 + 段拷贝 + 模块加载 + KernelInfo 构造」上**100% 共享代码**；唯一差异是「如何把文件字节读到内存」。这一差异由 `FileLoader` trait 封装。

**RISC-V 上为什么不能直接复用 UEFI 路径？**

Rust 工具链对 UEFI 目标只覆盖 `aarch64-unknown-uefi` / `i686-unknown-uefi` / `x86_64-unknown-uefi` 三个 target，没有 `riscv64-unknown-uefi`。根因不是社区"懒得加"，而是 LLVM 无法为 RISC-V 生成 UEFI 所要求的 PE/COFF 二进制。即使 U-Boot 实现了 UEFI 子集（`CONFIG_EFI_LOADER`），我们也无法生成可以被它装载的 `.efi` 文件。

因此 OpenSBI 路径无法直接复用 `uefi` crate；但**ELF 解析、段拷贝、KernelInfo 构造的代码是固件无关的**，可以完整共享。

**启动链**：

```
OpenSBI (M-mode)     ← SBI 运行时服务：console、timer、IPI（无文件系统）
    ↓ jr a1
U-Boot (S-mode)      ← 真正的 RISC-V "UEFI 等价物"：提供 FAT 驱动、fatload、go
    ↓ fatload + go
boot-shim (S-mode)   ← THIS（解析 ELF、加载模块、构造 KernelInfo）
    ↓ minix_elf
kernel
```

U-Boot 在 RISC-V 嵌入式生态中扮演 UEFI 的角色：它读取 ESP 风格的 FAT 分区，把文件加载到指定物理地址（参见 [U-Boot UEFI 文档](https://docs.u-boot.org/en/latest/develop/uefi/uefi.html) 与 [Alpine riscv64 启动指南](https://wiki.alpinelinux.org/wiki/Riscv64)）。我们让 U-Boot 替 boot-shim 完成"读文件"这一步——用 `fatload` 把 `kernel.elf` 和所有模块预加载到 RAM，并构建一个小小的 `BootFileTable`（路径 → 物理地址映射）传给 boot-shim。

**平台描述符来源**：UEFI 路径通过 `uefi_helpers::find_platform_descriptor()` 扫描 UEFI Configuration Table 获取 ACPI RSDP（x86-64）或 DTB（aarch64），必须在 `ExitBootServices()` 之前调用。OpenSBI 路径通过 `opensbi_helpers::dtb_ptr()` 读取入口汇编保存的 `a1` 寄存器值获取 DTB 物理地址；两者都封装为 `PlatformDescriptorPtr`（见 [04-platform-discovery.md](04-platform-discovery.md) §4.4）后写入 `KernelInfo.platform_descriptor`，供 `minix-platform::init_from_kinfo()`（见 §4.6 启动时序）解析。

**boot.cmd 脚本骨架**（QEMU virt + virtio-blk + FAT）：

```sh
# 把每个文件加载到独立物理地址
fatload virtio 0:1 0x88000000 /EFI/minix/kernel.elf
fatload virtio 0:1 0x89000000 /EFI/minix/modules/vm
fatload virtio 0:1 0x89200000 /EFI/minix/modules/pm
# ... 其余 module 同样

# 构造 BootFileTable 到 0x8a000000 后，跳入 boot-shim
fatload virtio 0:1 0x80100000 /EFI/minix/boot-shim.elf
go      0x80100000 0x8a000000   # a0 = BootFileTable phys addr
```

（`BootFileTable` 可由一个小工具在制盘时生成为独立文件并 `fatload`；
本节关注 Rust 侧的协议，不展开 boot.cmd 细节。）

> **路径格式约定**：`loader::KERNEL_PATH` 和 `loader::MODULES_DIR` 使用正斜杠（`/EFI/minix/kernel.elf`）作为规范格式——与 U-Boot `fatload` 脚本一致。`UefiFileLoader::read` 在传递给 UEFI `SimpleFileSystem` 之前自动将 `/` 转换为 `\`，因为 UEFI 规范要求反斜杠分隔符。`UbootFileLoader::read` 直接用正斜杠匹配 `BootFileTable` 中的路径，无需转换。

UEFI 侧和 OpenSBI 侧的 ELF 加载主流程完全共享同一套代码——通过 `FileLoader` trait 抽象文件读取差异，`load_kernel_with_loader()` 和 `load_boot_modules_with_loader()` 实现统一的加载逻辑。详见 [02-higher-half-kernel.md](02-higher-half-kernel.md) §4.2。

**`BootFileTable` —— U-Boot 与 boot-shim 之间的协议**：

```rust
// os/boot-shim/src/opensbi_helpers.rs
pub const BOOT_FILE_TABLE_MAGIC: u64 = 0x3154_4f4f_4258_4e4d; // "MNXBOOT1"

#[repr(C)]
pub struct BootFileEntry {
    pub path: [u8; BOOT_FILE_PATH_MAX],  // NUL 结尾 UTF-8
    pub phys_addr: u64,                   // fatload 落地的物理地址
    pub len: u64,                         // 文件字节数
}

#[repr(C)]
pub struct BootFileTable {
    pub magic: u64,                                              // 必须等于 MAGIC
    pub entry_count: u32,
    pub _pad: u32,
    pub entries: [BootFileEntry; BOOT_FILE_TABLE_MAX_ENTRIES],
}
```

`#[repr(C)]` 保证 U-Boot 脚本（或预生成工具）可以按字段偏移直接填值。`magic` 是防御性检查：boot 脚本配错时 `a0` 指向无效内存，我们能立即 panic 报错而不是继续解释垃圾数据。

**UbootFileLoader 实现**：

```rust
pub struct UbootFileLoader<'a> { table: &'a BootFileTable }

impl<'a> FileLoader for UbootFileLoader<'a> {
    fn read(&self, path: &str) -> Option<Vec<u8>> {
        let entry = self.table.find(path)?;
        // SAFETY: U-Boot fatload 已把 entry.len 字节放在 entry.phys_addr，
        // 该区域在引导阶段恒等映射，无并发修改。
        let slice = unsafe {
            slice::from_raw_parts(entry.phys_addr as *const u8, entry.len as usize)
        };
        Some(slice.to_vec())
    }
}
```

**`OpenSbiBootShim::prepare_boot` — 与 UEFI 版结构同构**：

```rust
impl BootShim for OpenSbiBootShim {
    fn prepare_boot(bump_pages: usize) -> BootPrepareResult {
        let table = boot_file_table()
            .expect("boot-shim: U-Boot did not pass a BootFileTable in a0");
        let file_loader = UbootFileLoader::new(table);

        let memmap = build_memmap();             // 硬编码 QEMU virt DRAM
        let root_page = alloc_root_page();        // bump_alloc
        let (bump_base, bump_end) = alloc_bump_region(bump_pages);

        // ↓↓↓ 与 UEFI 路径调用的是同一对函数 ↓↓↓
        let kern = loader::load_kernel_with_loader(&file_loader)
            .expect("boot-shim: failed to parse kernel ELF");
        let boot_modules =
            loader::load_boot_modules_with_loader(&file_loader, alloc_module_pages);

        let kernel_info = build_kernel_info(
            memmap,
            kern.kern_virt_base, kern.kern_phys_base, kern.kern_size,
            boot_modules,
        );

        // 无 exit_boot_services()：U-Boot 已经 go 走，无回头路。
        BootPrepareResult { kernel_info, root_page, bump_base, bump_end }
    }
}
```

**入口汇编需要保存 `a0`**——U-Boot 把 `BootFileTable` 物理地址放在 `a0`，rust_main 之前必须把它存到 `BOOT_FILE_TABLE_PTR` 静态变量：

```rust
core::arch::global_asm!(
    ".section .text.init",
    ".global _start",
    "_start:",
    "    la sp, __stack_top",
    "    call rust_save_a0",      // unsafe { install_boot_file_table(a0) }
    "    call rust_main",
    "1:  wfi",
    "    j 1b",
);

#[no_mangle]
pub extern "C" fn rust_save_a0(a0: u64) {
    unsafe { boot_shim::opensbi_helpers::install_boot_file_table(a0); }
}
```

**与 UEFI 路径的差异点（仅四处，皆为固件本质决定）**：

| 维度 | UEFI 路径 | OpenSBI 路径 |
|------|----------|--------------|
| 文件读取 | `SimpleFileSystem::read()` | `BootFileTable` 物理地址查表 |
| 内存映射来源 | `boot::memory_map()` 动态发现 | 硬编码 QEMU virt DRAM 布局 |
| 模块页分配 | `boot::allocate_pages(LOADER_DATA)` | bump 分配器（DRAM 高 32 MB） |
| Exit 固件服务 | `boot::exit_boot_services()` | 无（U-Boot 已 `go`） |

**完全共享的代码（两端 100% 同源）**：
- ELF 加载主流程（`load_kernel_with_loader` / `load_boot_modules_with_loader`）——详见 [02-higher-half-kernel.md](02-higher-half-kernel.md) §4.2
- `KernelInfo` 字段集
- `BootModule` 描述符布局

**测试覆盖**：`opensbi_helpers::tests` 验证 `BootFileTable` 的查表、magic 校验、entry_count 边界、`UbootFileLoader` 实际从内存读字节。ELF 加载逻辑的测试详见 02 文档 §4.2。

> 实际部署的 U-Boot 编译/分区/boot.cmd 细节超出本章范围（属于"如何运维"而非"内核如何启动"）。读者只需理解 Rust 侧的协议：U-Boot 负责把文件准备到 RAM 并填好 `BootFileTable`，boot-shim 负责剩下的一切。


### 4.6 kernel 入口 — arch_boot 与 arch_boot_impl

boot-shim 调用 `arch_boot()` 进入内核。`arch_boot()` 做两件事：

1. **`arch_boot_impl::<P>(kernel_info, root_page)`** — 建立页表脚手架（恒等映射 + 内核高地址映射 + 启用分页）
2. **`HigherHalf::jump_to_kmain(info, info.kern_stack_top)`** — 切栈到高地址，跳转到 `kmain()`

```rust
// os/kernel/src/lib.rs
#[cfg(target_arch = "x86_64")]
pub fn arch_boot(kernel_info: &KernelInfo, root_page: PhysBytes) -> ! {
    let info = arch_boot_impl::<X86_64Paging>(kernel_info, root_page);
    // SAFETY: arch_boot_impl just enabled paging with both identity
    // and kernel high mappings. info is valid and accessible at high address.
    // kern_stack_top is a valid high virtual address from KernelInfo.
    unsafe { X86_64HigherHalf::jump_to_kmain(info, info.kern_stack_top) }
}
```

`arch_boot_impl` 的三步操作（Step 1 恒等映射 → Step 2 内核高地址映射 → Step 3 启用分页）和 `HigherHalf::jump_to_kmain` 的切栈跳转机制，详见 [02-higher-half-kernel.md](02-higher-half-kernel.md) §4.4~§4.5。

> **Step 3 的实际含义**：在 Minix3 C 版本中，`vm_enable_paging()` 是真正"开分页"（CR0.PG 从 0 变 1）。在 UEFI 路径下，MMU 一直是开的，所以 `paging.enable()` 的实际操作是**切换页表基址**（x86-64 写 CR3，ARM64 写 TTBR0，RISC-V 写 satp），从 UEFI 的恒等映射切换到内核自己的映射。

### 4.7 与 Minix3 C 的函数对照

| C 函数 (Ch2) | Rust 实现 | 位置 |
|-------------|----------|------|
| `pre_init(magic, ebx)` | 不需要 — UEFI 替代 | — |
| `get_parameters(ebx)` | `prepare_boot()` GetMemoryMap / 硬编码 memmap | `boot-shim/src/uefi_helpers.rs` 或 `opensbi_helpers.rs` |
| GRUB `multiboot` + `load_mods` | `load_boot_modules()` 从 ESP 分区读取 | `boot-shim/src/uefi_helpers.rs` |
| `pg_clear()` | `Paging::new_from_page(root_page)` | `arch/x86_64/paging.rs` |
| `pg_identity(&kinfo)` | `arch_boot_impl` Step 1 (`map_huge`) | `kernel/src/lib.rs` |
| `pg_mapkernel()` | `arch_boot_impl` Step 2 (`map_huge`) | `kernel/src/lib.rs` |
| `pg_load()`+`vm_enable_paging()` | `Paging::enable()` | `arch/x86_64/paging.rs` |
| `alloc_pagetable()` | `boot_pt_alloc()`（中间页表页池） | `arch/src/pt_alloc.rs` |
| `pg_map()` | `Paging::map()`（4KB 粒度映射） | `arch/src/paging.rs` |

---

## 5. 测试要点

Boot 阶段是整个系统最脆弱的环节——页表配置错误直接导致三重故障（x86-64）或异常死锁（ARM64/RISC-V），没有任何调试输出机会。测试的核心目标是：**保证从 UEFI 交控制权到内核 `kmain()` 的每一步都不出错，且每一步的语义正确**。

具体来说，测试要验证三层保证：

1. **控制权转移正确**：boot-shim 能正确获取内存映射、从 ESP 加载 kernel ELF 与 boot 模块、构造 KernelInfo、调用 arch_boot——任何一步失败都意味着内核根本跑不起来
2. **页表脚手架正确**：恒等映射和高半核映射覆盖了正确的地址范围，切换页表后内核能通过高地址访问自身代码/数据——映射遗漏或权限错误会导致切换后立即崩溃
3. **三架构行为一致**：同一套 `arch_boot_impl<P>` 泛型代码在三个架构上产生相同的语义——架构实现差异（PML4 四级 vs Sv39 三级）不应导致上层行为分歧

### 5.1 单元测试（mock，`#[cfg(test)]`）

- `MemoryRegion` 的重叠检测、对齐、转换为 PFN 范围 — 保证 KernelInfo 传给内核的内存描述不含重叠/未对齐区域，否则后续 `map_huge` 会映射到错误的物理页
- `KernelInfo` 构造器从 mock UEFI mmap 构建 — 保证 boot-shim 到 kernel 的数据传递正确，KernelInfo 是两者之间唯一的契约，构造错误意味着内核基于错误的信息做所有决策
- `Paging` mock 实现的 `new_from_page()` + `enable()` 不 panic — 保证 Paging trait 的接口契约可被实现，泛型代码 `arch_boot_impl<P>` 能正确调用，这是上层代码能工作的前提

**实际测试数量（2026-06-11 验证）**:

| Crate | 测试数 | 覆盖范围 |
|-------|--------|----------|
| `minix-elf` | 23 | `ElfError` 变体、`ProgramHeader64` 解析、PT_LOAD 迭代、边界检查、zero-filesz 段、segment flags 等 |
| `boot-shim` (loader) | 12 | `first_overlapping_pair` (5) + `load_kernel_with_loader` (3) + `MockLoader` (2) + `load_boot_modules` (1) + panic path (1) |
| `boot-shim` (opensbi_helpers) | 8 | `BootFileTable` 校验/查找、`entry_path_eq`、U-Boot file loader、`build_kernel_info` (riscv64 user_sp)、`bump_alloc` (正向+负向) |
| `boot-shim` (uefi_helpers) | 1 | `build_kernel_info_fields` |
| `boot-shim` (总计) | 20 | 上三项之和 |
| `minix-boot` | 0 | 仅有类型定义，无运行时逻辑可测 |
| **doc 01 路径总计** | **43** | — |

**UEFI 路径测试不足**: 20 个 boot-shim 测试中仅 1 个（5%）覆盖 UEFI 路径。其余 19 个测试都是 OpenSBI + loader + U-Boot table 路径。这是因为 UEFI 协议调用是单根（efi_main → BootServices），难以在 host 上 mock，需要 QEMU 集成测试覆盖。

### 5.2 集成测试（qemu-tests/）

Mock 测不了的硬件启动路径，用 QEMU 集成测试覆盖。每个测试内核编译为独立二进制，QEMU 启动后串口输出 `### TEST_RESULT: PASS <name> ###`， `run_all.sh` 捕获输出判定结果。

测试内核按架构分目录：

```
os/qemu-tests/
├── run_all.sh               # CI 入口：构建三个架构 + 执行 + 统计 PASS/FAIL
├── run_qemu.sh              # 单测试运行器：接收架构 + 二进制路径，配置 QEMU 参数
└── test-kernels/
    └── kernel/
        └── bootstrap/
            ├── hello-boot/              # x86_64: uefi_helpers → arch_boot_impl<X86_64Paging> → PASS
            │   └── src/main.rs
            ├── hello-boot-aarch64/      # aarch64: uefi_helpers → arch_boot_impl<AArch64Paging> → PASS
            │   └── src/main.rs
            ├── hello-boot-riscv64/      # riscv64: 直接构造 BootPrepareResult → arch_boot_impl<Riscv64Paging> → PASS
            │   └── src/main.rs
            ├── test-memmap/             # x86_64: build_memmap → 断言 memmap 非空 + 有低地址 CONVENTIONAL
            │   └── src/main.rs
            ├── test-paging-enable/      # x86_64: uefi_helpers → arch_boot_impl → 串口输出 PASS
            │   └── src/main.rs
            └── test-kernel-map/         # x86_64: arch_boot_impl(高半核) → 读高/低地址 sentinel → 断言一致
                └── src/main.rs
```

所有测试内核复用正式代码，不复制模板：

- **UEFI 入口逻辑**：`boot_shim::uefi_helpers`（`prepare_boot`, `build_memmap`, `exit_boot_services` 等）
- **OpenSBI 入口逻辑**：`boot_shim::opensbi_helpers`（`prepare_boot`, `build_memmap`, `bump_alloc` 等）
- **页表设置**：`minix_kernel::arch_boot_impl::<P>()`（恒等映射 + 高半核映射 + enable）
- **串口输出**：`minix_arch::{x86_64,arm64,riscv64}::early_console`（`write_str`, `write_hex` 等）
- **页分配器**：`minix_kernel::boot_alloc` + `minix_arch::pt_alloc`

测试内核仅实现测试逻辑（断言、输出），所有共享逻辑通过 `use` 引用正式 crate。

三个架构的启动路径不同，`run_qemu.sh` 各自适配：

| 架构 | QEMU 命令 | 固件 | 入口 | 串口方式 |
|------|-----------|------|------|---------|
| x86_64 | `qemu-system-x86_64` | OVMF (UEFI) | `efi_main()` | COM1 (x86 I/O port) |
| aarch64 | `qemu-system-aarch64 -machine virt -cpu cortex-a72` | AAVMF (UEFI) | `efi_main()` | PL011 UART (MMIO `0x0900_0000`) |
| riscv64 | `qemu-system-riscv64 -machine virt` | OpenSBI (`-bios default`) | `_start` asm → `rust_main()` | SBI ecall `CONSOLE_PUTCHAR` |

`run_qemu.sh` 自动探测固件路径（适配不同发行版），无 UEFI 固件时 riscv64 回退到 `-bios default -kernel`。

所有架构的 hello-boot 均已集成对应 Paging trait 实现（`X86_64Paging`/`AArch64Paging`/`Riscv64Paging`），走真实的 `arch_boot_impl::<P> → enable` 启动路径。x86_64/aarch64 通过 `UefiBootShim::prepare_boot` 完成引导准备；riscv64 在 QEMU `-kernel` 测试场景中直接构造 `BootPrepareResult`（因为没有 U-Boot/BootFileTable），仅验证 `arch_boot_impl` 路径。

> **关于 hello-boot 的链接方式**：hello-boot 通过 `minix-kernel = { workspace = true }` 以 rlib 方式依赖内核 crate——这是测试代码的合理简化，因为 hello-boot 只需验证"boot-shim → arch_boot → 串口输出"链路能跑通，不需要 higher-half kernel。正式的 boot-shim 需要将 kernel 改为独立 ELF binary，通过 ELF 加载器解析并加载 kernel 的 PT_LOAD 段到物理内存，才能实现 trampoline 跳转到高地址。详见 02-boot-bridge.md。

测试用例（每个测试均覆盖 x86_64 / aarch64 / riscv64 三架构）：

| 测试 | 覆盖范围 | 验证目标 |
|------|---------|---------|
| **hello-boot** | x86_64/aarch64: UEFI → `UefiBootShim::prepare_boot` → `arch_boot_impl<P>` → 串口输出；riscv64: OpenSBI 直接构造 `BootPrepareResult` → `arch_boot_impl<Riscv64Paging>` → 串口输出 | 全链路可达：boot-shim 能正确构造 KernelInfo 并调用 arch_boot，Paging 能完成 identity map + enable 不崩溃。riscv64 跳过 `OpenSbiBootShim::prepare_boot`（QEMU `-kernel` 无 U-Boot/BootFileTable） |
| **test-memmap** | x86_64/aarch64: `uefi_helpers::build_memmap` → 断言 memmap 非空 + 有 CONVENTIONAL 区域；riscv64: 硬编码 QEMU virt DRAM 布局 → 断言覆盖 DRAM 区域 | KernelInfo.memmap 反映真实物理内存布局。若为空或遗漏内核区域，后续恒等映射会跳过内核自身，切换页表后立即崩溃 |
| **test-paging-enable** | x86_64/aarch64: UEFI helpers → `arch_boot_impl` → 串口输出 PASS；riscv64: 直接构造 `BootPrepareResult` → `arch_boot_impl` → PASS | `paging.enable()` 后 CPU 仍能继续执行——最基本的安全断言。x86_64 切换 CR3，aarch64 使能 MMU，riscv64 写入 satp + sfence.vma |
| **test-kernel-map** | x86_64/aarch64: `arch_boot_impl`(高半核) → 读写 sentinel 断言高低地址值一致；riscv64: `arch_boot_impl`(identity) → 读写 sentinel 断言一致 | 高半核映射语义正确。riscv64 因 `kern_virt_base == kern_phys_base`，仅验证 identity 映射 |

> **三架构 QEMU 测试结果**：上述 4 类测试 × 3 架构 = 12/12 全部通过。`test-higher-half`（高半核切换后 SP/FP 寄存器状态验证）由 [02-higher-half-kernel.md §5.1](02-higher-half-kernel.md#51-qemu-集成测试三架构-1515-通过) 覆盖，合计 15/15 通过（含 riscv64 Sv39 canonical 高地址断言）。

**运行方式**：`cd os/qemu-tests && bash run_all.sh`

**架构差异要点**：

| 差异 | x86_64 | aarch64 | riscv64 |
|------|--------|---------|---------|
| QEMU DRAM 起点（OS 可用 RAM 起） | 0x0（实模式 1MB 以下被 BIOS/ROM 占，OS 从 1MB+ 起；长模式不限制） | 0x4000_0000 | 0x8000_0000 |
| kern_phys_base | 0x200_000 | 0x4020_0000 | 0x8000_0000 |
| kern_virt_base（生产路径） | 0xFFFF_8000_0000_0000 | 0xFFFF_8000_0000_0000 | 0xFFFF_FFC0_0000_0000（Sv39 canonical high） |
| kern_virt_base（当前测试场景） | 0xFFFF_8000_0000_0000 | 0xFFFF_8000_0000_0000 | 0x8000_0000（QEMU `-kernel` 跳过 ELF 装载与高半核切换的妥协） |
| 引导方式 | UEFI (OVMF) | UEFI (QEMU_EFI) | OpenSBI + U-Boot（生产）/ OpenSBI 直连（测试） |
| 调用约定 | Windows x64 ABI | AAPCS64 | RISC-V calling convention |

> **架构范围**：本表 `kern_virt_base` 行**生产路径**列反映三架构统一的"高半核"安排；**当前测试场景**列暴露了 riscv64 在 QEMU `-kernel` 直接加载下未完成 ELF 装载 + 高半核切换的临时妥协（见本文档 §4.5+ TODO 第 2 条与本文档末 TODO）。
>
> **RISC-V 物理地址整体从 0 起**：表格中 `0x8000_0000` 是 **DRAM（OS 可用 RAM）的起点**，不是物理地址空间起点。RISC-V 物理地址空间整体 `0x0 ~ 2^XLEN-1`（XLEN=64 即 16EB），低 2GB 区间分配给 M-mode 固件 / CLINT / PLIC / UART 等 MMIO 设备，OS 可用的 RAM 从 `0x8000_0000` 起。这与 x86-64 实模式下 `0~1MB` 被 BIOS/ROM 占满、OS 从 1MB+ 开始用，是完全类似的思路。所有主流 RISC-V SoC（包括 SiFive HiFive Unleashed、StarFive VisionFive 2）都遵循"`0x8000_0000` 起是 DRAM" 这一约定。QEMU virt 文档列出完整 MMIO 布局：[virt 机器 MMIO 区](https://www.qemu.org/docs/master/system/riscv/virt.html)（MROM/`0x0`-`0x1000`、CLINT/`0x200_0000`、PLIC/`0x0C00_0000`、NS16550A UART/`0x1000_0000`、DRAM/`0x8000_0000`）。

---

## 6. 参见

- [00-kernel-overview.md](00-kernel-overview.md) — Kernel 整体架构概览
- [02-higher-half-kernel.md](02-higher-half-kernel.md) — 链接脚本、ELF 加载、arch_boot_impl 页表映射、HigherHalf 切栈跳转
- `os/arch/src/paging.rs` — Paging trait 定义 + PageFlags
- `os/arch/src/x86_64/paging.rs` — x86-64 Paging 实现

## 附录 A：UEFI 极简指南

本附录只覆盖文档中出现的 UEFI 概念，不追求完整。完整规范见 [UEFI Spec](https://uefi.org/specifications)。

### A.1 UEFI 是什么

UEFI（Unified Extensible Firmware Interface）是 BIOS 的替代者——它是开机后第一个运行的固件程序，负责硬件初始化和加载操作系统。与 BIOS 的关键区别：

| | BIOS | UEFI |
|---|---|---|
| **CPU 模式** | 16 位实模式启动 | 32/64 位保护模式启动 |
| **可执行格式** | 纯二进制（512 字节 MBR） | PE32+ 格式（`.efi` 文件） |
| **编程接口** | 16 位中断调用（`int 10h` 等） | C 风格函数表（`SystemTable`） |
| **架构支持** | x86 专用 | x86-64、aarch64、riscv64 |
| **内存发现** | `int 15h/E820`（BIOS 中断） | `GetMemoryMap()` 函数调用 |

### A.2 UEFI 启动流程

```
UEFI 固件开机
  │
  ├── POST（硬件自检）
  ├── 初始化 DXE 驱动（磁盘、网卡、显示等）
  ├── 查找启动项（Boot Manager → BootOrder 变量）
  │
  └── 从磁盘加载 .efi 到内存 ← 引导加载器核心职责
        UEFI 读取 FAT 分区中的 `boot.efi`，解析 PE32+ 头部，
        将代码段/数据段装入物理内存，完成后调用入口函数
        （对标"自己写 OS"中的 int 13h 读扇区）。
        以下链路从 "UEFI 已加载 .efi 到内存" 开始：
        │
        ├── 传入两个参数：ImageHandle + SystemTable
        └── .efi 在 UEFI Boot Services 环境中运行
              │
              ├── 调用 Boot Services 函数（GetMemoryMap、AllocatePages 等）
              ├── 从 ESP 分区加载 kernel ELF + boot 模块（VM/PM 等）
              ├── 调用 ExitBootServices() ← 交还硬件控制权
              └── 进入纯裸机模式，UEFI 不再干预
```

### A.3 核心概念速查

| 概念 | 含义 | 文档中出现位置 |
|------|------|-------------|
| **`.efi` 文件** | UEFI 可执行文件格式（PE32+），相当于 Linux 的 ELF | §3.8、§5.2 |
| **`ImageHandle`** | UEFI 传给 `.efi` 入口函数的第一个参数，代表"你自己这个程序" | §3.1、§4.4 |
| **`SystemTable<Boot>`** | UEFI 传给 `.efi` 入口函数的第二个参数，包含所有 Boot Services 函数指针 | §4.4 |
| **`BootServices`** | UEFI 提供的服务函数集（内存分配、协议查询等），`ExitBootServices()` 后不可用 | §3.2 |
| **`GetMemoryMap()`** | BootServices 函数，返回物理内存布局（哪些可用、哪些被占用） | §3.1、§3.6 |
| **`AllocatePages()`** | BootServices 函数，分配物理内存页（1 页 = 4KB） | §3.2、§4.4 |
| **`ExitBootServices()`** | BootServices 函数，告知 UEFI "我不再需要你了"，UEFI 释放所有资源，此后内核独占硬件 | §3.2 |
| **OVMF** | Open Virtual Machine Firmware——QEMU 用的 UEFI 固件实现，让 QEMU 能跑 UEFI 启动 | §3.1、§3.8 |

### A.4 UEFI vs Multiboot：角色对照

如果你熟悉 Multiboot/GRUB，这个对照表帮你快速理解 UEFI 的等价物：

| Multiboot/GRUB | UEFI | 说明 |
|----------------|------|------|
| GRUB 读取 `boot.cfg` | UEFI Boot Manager 读取 `BootOrder` 变量 | 决定加载谁 |
| `multiboot /boot/kernel` | 加载 `boot.efi` | 加载可执行文件 |
| Multiboot Header（`0x1BADB002`） | PE32+ 头（标准可执行格式） | 固件如何识别可执行文件 |
| EAX/EBX 寄存器传参 | `ImageHandle` + `SystemTable` 参数传参 | 如何传递启动信息 |
| `multiboot_info_t`（GRUB 填充） | `GetMemoryMap()` 返回值（UEFI 填充） | 内存布局信息 |
| `load_mods` 加载模块 | boot-shim 通过 `SimpleFileSystemProtocol` 从 ESP 分区读取 | 加载额外二进制 |
| 无（内核直接跑在裸机上） | `ExitBootServices()` 后进入裸机 | 交接硬件控制权 |

### A.5 QEMU + UEFI 快速上手

```bash
# 1. 安装 OVMF（QEMU 的 UEFI 固件）
# Debian/Ubuntu:
sudo apt install ovmf
# Arch:
sudo pacman -S ovmf

# 2. 用 UEFI 启动 QEMU
qemu-system-x86_64 \
  -bios /usr/share/OVMF/OVMF_CODE.fd \   # UEFI 固件（替代 BIOS）
  -drive format=raw,file=disk.img         # 包含 EFI 系统分区的磁盘

# 3. 调试：QEMU + GDB（与 Multiboot 调试方式相同）
qemu-system-x86_64 -bios /usr/share/OVMF/OVMF_CODE.fd -s -S
# 另一个终端：
gdb boot.efi
(gdb) target remote :1234
```

---


---

*分类: Kernel 硬件发现*