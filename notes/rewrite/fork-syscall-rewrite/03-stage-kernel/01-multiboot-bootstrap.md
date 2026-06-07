# 01-multiboot-bootstrap: 从 GRUB 到分页开启

> **分类**: Kernel 硬件发现
> **源码**: `minix3/minix/kernel/arch/i386/pre_init.c`(243行), `pg_utils.c`(317行)
> **说明**: 内核从 GRUB 拿到 multiboot 数据，解析物理内存布局，建立恒等映射，开启分页

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

> **本文档的语义边界**：以下链路从"GRUB/UEFI 已经将内核二进制加载到物理内存"开始。引导加载器如何将内核从磁盘读到内存（MBR → GRUB 第二阶段 → 读 `boot.cfg` → 解析 Multiboot Header、或 UEFI DXE 驱动 → 加载 `.efi`）是引导加载器的职责，不属于内核源码。对于写过"自己写 OS"的读者：GRUB 的 `load_mods` / `multiboot` 命令等价于书中手写 `int 13h` + `cli` 跳转；UEFI 的 `LoadImage()` 等价于 GRUB 的 Multiboot 加载。但本文档从"内核拿到控制权后"开始讲解，不展开引导加载器内部机制。详见附录 A 或各架构的 UEFI/OpenSBI 引导文档。

从上电到内核第一个 C 函数，完整调用链如下：

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
              └── kmain()（main.c:103）→ 初始化进程表 → cstart() → switch_to_user()
```

**关键交接点**：GRUB → 内核的交接通过 **Multiboot 协议**完成。内核在二进制文件头部嵌入一个 Multiboot Header（魔数 `0x1BADB002`，见 §2.0.1），GRUB 识别这个头部后就知道如何加载内核。启动完成后，GRUB 通过寄存器传递 `EAX=0x2BADB002`（确认是 Multiboot 启动）和 `EBX=multiboot_info_t 物理地址`（包含内存布局、模块列表等信息），这就是 `pre_init()` 接收到的两个参数。

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

这四件事的**顺序不可变**（必须先映射再开分页，否则开分页后地址空间不对代码就飞了），但**实现方式完全由架构决定**。Rust 重写时，引导参数的获取从 Multiboot 协议变为 UEFI（见 §1.7.1），分页操作被抽象为 `Paging` trait（见 §3.3），但四件事的语义不变。

### 1.6 两个 C 文件的职责划分

| 文件 | 职责 | 核心函数 |
|------|------|---------|
| `pre_init.c` | 解析 multiboot 数据、构建内存 map、编排启动流程 | `pre_init()`, `get_parameters()`, `overlaps()`, `mb_set_param()` |
| `pg_utils.c` | 页目录/页表操作、分页控制、物理页分配 | `pg_identity()`, `pg_mapkernel()`, `vm_enable_paging()`, `pg_load()`, `pg_alloc_page()`, `pg_map()` |

### 1.7 与 Rust 64 位版本的差异

minix-rs 是 64 位重写，从引导协议到页表结构都发生了根本变化。这里给出概览，详细设计见 §3。

#### 1.7.1 引导协议：Multiboot → UEFI

| 方面 | Minix3 C（x86-32） | minix-rs（x86-64 / aarch64 / riscv64） |
|------|-------------------|---------------------------------------|
| **引导协议** | Multiboot（1995 年规范，x86-32 专用） | UEFI（跨架构统一） |
| **引导加载器** | GRUB（第三方，Minix3 不含源码） | UEFI 固件（QEMU 用 OVMF，真机用板载 UEFI） |
| **内核"名片"** | Multiboot Header（魔数 `0x1BADB002`） | UEFI PE/COFF 头（标准可执行格式） |
| **参数传递** | GRUB → EAX/EBX 寄存器 | UEFI → `ImageHandle` + `SystemTable` |
| **内存发现** | 手动遍历 `multiboot_memory_map_t` | `GetMemoryMap()` 直接返回 |
| **模块加载** | GRUB `load_mods` + `multiboot_info_t.mi_mods` | UEFI `ImageHandle` protocol 标准接口 |
| **配置文件** | `etc/boot.cfg`（Minix3 提供） | UEFI Boot Manager / LoadOption |
| **boot-shim** | 无（`head.S` + `pre_init()` 在内核内部） | 独立 crate，`BootShim` trait 分离 UEFI/OpenSBI |

**为什么不用 GRUB**：Multiboot 是 1995 年的规范，只考虑了 x86-32。minix-rs 需要支持 x86-64、aarch64、riscv64 三种架构，UEFI 是唯一跨架构一致的引导协议。GRUB 虽然也支持 Multiboot2（64 位扩展），但各架构的 GRUB 实现差异大，不如 UEFI 统一。详见 §3.1。

#### 1.7.2 页表结构：2 级 → 4 级

| 方面 | Minix3 C（x86-32） | minix-rs（x86-64） |
|------|-------------------|-------------------|
| 页表层级 | 2 级（PD + PT） | 4 级（PML4 + PDPT + PD + PT） |
| 页目录条目 | `pagedir[1024]` 固定数组 | PML4/PDPT/PD 都是 512 条目的动态分配 |
| 大页大小 | 4MB（`I386_BIG_PAGE_SIZE`） | 2MB 或 1GB（`PSE` 或 `PSE-1GB`） |
| 地址空间 | 4GB（32 位，实际截断到 `LIMIT 0xFFFFF000`） | 48 位虚拟地址，物理可达 52 位 |
| PGE/PAE | 可选 | 必需（x86-64 要求 PAE） |
| 页表分配 | 编译时固定分配 6 个页表（`pagetables[6][1024]`，用完 panic） | 运行时按需分配（从 memmap 空闲区域 bump 分配物理页帧，通过 UEFI 恒等映射写入） |

#### 1.7.3 启动流程对比

```
Minix3 C (x86-32):                    minix-rs (x86-64):
BIOS → GRUB → head.S → pre_init()    固件(UEFI/OpenSBI) → boot-shim crate
         │         │                       │
         │         ├─ 验证魔数 0x2BADB002   ├─ GetMemoryMap()
         │         ├─ get_parameters()      ├─ AllocatePages()
         │         ├─ pg_identity()         ├─ 构建 KernelInfo
         │         ├─ pg_mapkernel()        ├─ 建立初始页表
         │         ├─ pg_load()             ├─ ExitBootServices()
         │         └─ vm_enable_paging()    └─ 跳转到 kernel
         │                                     │
         └─ kmain(&kinfo)                     └─ arch_boot(kernel_info, paging) → kmain()

注意：GRUB 跳转时分页未开启（CR0.PG=0），内核需要自己建页表并开分页；
UEFI 跳转时分页已开启（x86-64 长模式硬件要求分页），CR3 中是 UEFI 建的恒等映射，
boot-shim 在此映射下运行，构建新页表后切换 CR3。
```

**核心变化**：C 版本的 `head.S` + `pre_init()` 做的事（解析引导参数、建页表、开分页），在 Rust 版本中被 `boot-shim` crate 完成。`boot-shim` 是独立 crate，通过 `BootShim` trait 分离 UEFI 和 OpenSBI 实现——kernel 只依赖 trait，不依赖任何固件类型，换引导协议只需换 boot-shim 的 feature，kernel 一行不改。

---

## 2. C 源码分析

### 2.0 内核入口：从 GRUB 到 pre_init()

在分析 `pre_init.c` 和 `pg_utils.c` 之前，必须理解内核是如何被 GRUB 调用的。这涉及三个组件：内核侧的 Multiboot Header、汇编入口 `head.S`、以及 Minix3 提供的 GRUB 配置文件。

#### 2.0.1 Multiboot Header — 内核告诉 GRUB "我是 Multiboot 内核"

**源码**：`minix3/minix/kernel/arch/i386/head.S:47-67`

Multiboot 协议要求内核二进制文件的前 8192 字节内必须包含一个 **Multiboot Header**，否则 GRUB 不认识这个内核。这个头部是内核主动嵌入的"名片"：

```asm
/* head.S:47-67 — Multiboot Header */
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

**源码**：`minix3/minix/kernel/arch/i386/head.S:38-82`

GRUB 跳转到内核后，最先执行的是 `head.S` 中的 `MINIX` 标签。这个汇编文件做了三件事：跳转到 `multiboot_init`、设置栈、调用 `pre_init()`。

```asm
/* head.S:40-41 — 内核入口点，GRUB 跳转到这里 */
.global MINIX
MINIX:
    jmp multiboot_init          /* 直接跳转到 multiboot_init */

/* head.S:47-67 — Multiboot Header（见 §2.0.1）*/
...

/* head.S:69-82 — multiboot_init: 真正的初始化入口 */
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
4. `push $0; popf`：清除 EFLAGS 寄存器。关闭中断标志（IF=0），清除嵌套任务标志（NT=0），确保内核从干净的状态开始。
5. `push $0`：空占位，使栈 8 字节对齐（x86 System V ABI 要求 `call` 前栈 16 字节对齐，`push %eax` + `push %ebx` + `push $0` = 3×4=12 字节偏移，加上 `call` 的返回地址 4 字节 = 16 字节对齐）。
6. `push %ebx; push %eax; call pre_init`：将 GRUB 传递的两个参数压栈，调用 `pre_init()`。C 调用约定下，参数从右到左压栈，所以 `%ebx`（第 2 参数）先压，`%eax`（第 1 参数）后压。
7. `mov $k_initial_stktop, %esp`：`pre_init()` 返回后，分页已开启，内核已映射到高虚拟地址。此时切换到高地址栈 `k_initial_stktop`，因为低地址的 `load_stack` 可能不再可访问（取决于恒等映射是否保留）。
8. `push %eax; call kmain`：将 `pre_init()` 返回的 `&kinfo` 传给 `kmain()`。

**为什么需要汇编跳板**：C 函数需要栈才能运行（局部变量、参数、返回地址都在栈上），但 GRUB 跳转时没有设置栈。`head.S` 的核心职责就是"为 C 代码准备栈，然后跳转"。

#### 2.0.3 Minix3 的 GRUB 配置文件

**源码**：`minix3/etc/boot.cfg.default`

Minix3 不包含 GRUB 源码，但提供 GRUB 配置文件 `boot.cfg`，告诉 GRUB 如何加载内核：

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

**逐行解释**：

| 行 | 含义 |
|------|------|
| `clear=1` | 启动菜单前清屏 |
| `timeout=5` | 5 秒后自动选择默认项 |
| `default=2` | 默认选择第 2 项（Start latest MINIX 3） |
| `load_mods /boot/minix_default/mod*` | 加载所有启动模块（PM、VM、VFS、RS 等二进制），这些就是 `multiboot_info_t.mi_mods_addr` 指向的模块列表 |
| `multiboot /boot/minix_default/kernel` | 用 Multiboot 协议加载内核二进制——GRUB 会扫描内核前 8KB 找到 `0x1BADB002` 魔数（见 §2.0.1） |
| `rootdevname=$rootdevname` | 传递启动参数给内核，内核在 `get_parameters()` 中解析 |
| `bootopts=-s` | 单用户模式标志 |

**配置文件与源码的对应关系**：

| boot.cfg 指令 | Multiboot 协议 | 内核源码对应 |
|--------------|---------------|-------------|
| `multiboot /boot/.../kernel` | GRUB 扫描 `0x1BADB002` 头部 | `head.S:50` `MULTIBOOT_HEADER_MAGIC` |
| `load_mods /boot/.../mod*` | GRUB 填充 `multiboot_info_t.mi_mods_count/addr` | `pre_init.c` `get_parameters()` 拷贝 `module_list` |
| `rootdevname=...` | GRUB 填充 `multiboot_info_t.mi_cmdline` | `pre_init.c` `mb_set_param()` 解析到 `kinfo.param_buf` |

### 2.1 相关定义

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
| `freepde_start` | `pg_mapkernel()` 返回 | 内核映射后第一个空闲 PDE——用户空间从此开始 |

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

由 `head.S` 中的 `multiboot_init` 调用（`call pre_init`，head.S:78），是内核执行的第一个 C 函数。验证魔术值 → 解析 multiboot → 恒等映射 → 内核映射 → 开分页 → 返回 `&kinfo`。

#### 2.3.2 `get_parameters()` — 解析 GRUB 数据（pre_init.c:94）

步骤：拷贝 mbi → 初始化 kinfo 字段 → 解析命令行 → 构建物理内存 map (`add_memmap`×N) → 拷贝模块列表 → 检查重叠 (`overlaps`) → 切除占用 (`cut_memmap`)

#### 2.3.3 `add_memmap()` — 添加可用内存区域（pg_utils.c:86）

硬截断到 4GB（`LIMIT 0xFFFFF000`）→ 4KB 对齐 → 填入 `cbi->memmap[]` → 更新 `mem_high_phys`。

#### 2.3.4 `cut_memmap()` — 切除已占用区域（pg_utils.c:32）

将 `[start, end)` 从 memmap 中切除，余量通过 `add_memmap()` 写回。

#### 2.3.4b `alloc_lowest()` — 分配最低物理页（pg_utils.c:65）

遍历 memmap 找到满足 `len` 大小的最低基址区域，调用 `cut_memmap()` 切除并返回。用于 `pre_init` 中分配内核栈等需要低地址的物理内存。与 `pg_alloc_page()`（从尾部取）互补——一个取最低，一个取最高。

#### 2.3.5 `pg_identity()` — 恒等映射（pg_utils.c:162）

1024 × 4MB 大页恒等映射。每个 PDE 设置 `PRESENT | BIGPAGE | USER | WRITE` flag。超出 `mem_high_phys` 的 PDE 额外加 `PWT | PCD`（禁用缓存）。

> **USER flag 的含义**：x86-32 的 `pg_identity()` 设置 `I386_VM_USER`，意味着恒等映射允许用户态（Ring 3）访问。这是 Minix3 的设计选择——内核启动后，用户进程的代码和数据还在低地址，需要通过恒等映射访问。Rust 版本（64位 UEFI）不设 USER flag，因为 64位内核的恒等映射是 supervisor-only：x86-64 的恒等映射仅在 boot 阶段短暂使用，用户进程不依赖低地址映射；RISC-V Sv39 的 U=1 页在 S-mode 下需要 sstatus.SUM=1 才能访问，设 USER 反而增加复杂度。

#### 2.3.6 `pg_mapkernel()` — 内核高地址映射（pg_utils.c:186）

4MB 大页映射内核到高虚拟地址。返回第一个空闲 PDE 号。

#### 2.3.7 `vm_enable_paging()` — 开启分页（pg_utils.c:204）

执行：清 PG+PGE → 开 PSE → 开 PG → 开 WP → 开 PGE。

#### 2.3.8 `pg_load()` / `pg_alloc_page()` / `pg_map()`

`pg_load()` 写 CR3；`pg_alloc_page()` 从 memmap 尾部取 4KB；`pg_map(phys, vaddr, vaddr_end, cbi)` 4KB 粒度映射，将 `[vaddr, vaddr_end)` 映射到从 `phys` 开始的物理地址。若 `phys == PG_ALLOCATEME`，则每个虚拟页自动分配物理页（调用 `pg_alloc_page()`）。要求 `vaddr < kern_vir_start`（仅映射低地址区）。

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

**为什么不用 malloc**：pre_init 阶段所有数据结构编译时静态分配（`kinfo` BSS、`pagedir[1024]` 静态数组、`pagetables[6][1024]` 池）。

**为什么先恒等映射再映射内核**：开分页后 CPU 立即使用页表。不开恒等映射，正在执行的代码会页错误。

**为什么 4GB 截断**：32 位 x86 只能寻址 4GB。`LIMIT 0xFFFFF000` 是物理限制。64 位 Rust 必须删除。

**为什么物理内存从尾部取**：减少大块连续区域前端的碎片化。

**boot module 内存的生命周期**：`pre_init()` 阶段的 `cut_memmap()` 只是临时切掉 boot module 占用的物理内存，防止后续分配器误用。这些内存不是永久占用——内核在 `protect.c` 中解析每个 module 的 ELF、把代码段/数据段复制到进程地址空间后，通过 `add_memmap()` 归还，`mod_start = mod_end = 0` 标记已回收（`protect.c:450-451`）。同样，bootstrap 代码段在 `kmain()` 后也会被回收（`main.c:301`）。详见 `02-page-table-kernel.md`。

---

## 3. Rust 设计决策

> 本章解释从 Ch1&2 的 C 源码到 Rust 设计的每一个关键选择。每个决策给出：为什么选这条路径、替代方案有哪些、为什么否决替代方案。

### 3.1 UEFI 替代 Multiboot（架构演进）

**C 源码依据**：§2.3.1 `pre_init(magic, ebx)` — Multiboot 是由 GRUB 传递的 32 位引导协议。

**决策**：minix-rs 使用 UEFI 作为引导协议，替代 Minix3 的 Multiboot+GRUB。

**理由**：见 §1.7.1 对比表。核心原因：Multiboot 是 1995 年的规范，只考虑了 x86-32。三种 64 位架构中 UEFI 是唯一跨架构一致的引导协议。

**替代方案 & 否决理由**：

| 方案 | 否决原因 |
|------|---------|
| 保留 Multiboot + 64 位扩展 | ARM/RISC-V 无等效协议 |
| 各架构独立引导（x86 Multiboot + ARM PSCI + RISC-V SBI） | 三种引导路径 = 过度复杂 |
| Linux 风格（UEFI stub 内嵌 kernel） | 调试不便分离更新 |

### 3.2 boot-shim = 独立 crate，BootShim trait 分离固件实现

**C 源码依据**：§2.4 `pre_init()` → `kmain()` — C 版本 bootstrap 在 kmain 后被释放。

**决策**：引导逻辑放在独立 crate `boot-shim`，通过 `BootShim` trait（定义在 `minix-types`）分离 UEFI 和 OpenSBI 实现。kernel 只依赖 trait，不依赖任何固件类型。feature gate 仅在 `Cargo.toml` 中选择编译哪个实现，代码中不散落 `#[cfg]` 条件编译。

```
boot-shim crate (feature = "uefi")     boot-shim crate (feature = "opensbi")
     │                                       │
     │ UefiBootShim::prepare_boot()          │ OpenSbiBootShim::prepare_boot()
     │   UEFI GetMemoryMap()                 │   硬编码 QEMU virt 内存映射
     │   UEFI ExitBootServices()             │   bump 分配器
     │   → BootPrepareResult                 │   → BootPrepareResult
     │                                       │
     └─→ arch_boot(result.kernel_info,
               result.root_page) → kmain()
```

**移植性**：换非 UEFI 板 → 在 `boot-shim` 中实现新的 `BootShim` impl，Cargo.toml 启用对应 feature。kernel 一行不改。

**为什么 riscv64 用 OpenSBI 而非 UEFI**：Rust 没有 `riscv64-unknown-uefi` 编译目标，无法编译 UEFI 应用。riscv64 通过 OpenSBI 固件直接启动裸金属内核，`OpenSbiBootShim` 提供硬编码的 QEMU virt 内存映射和 bump 分配器，与 `UefiBootShim` 输出相同的 `BootPrepareResult`。

**替代方案 & 否决理由**：kernel 内部模块 → kernel 永远依赖 uefi crate；纯 feature gate 无 trait → 调用者直接依赖具体函数，无法 mock 测试，类型不安全；条件编译 `#[cfg(uefi)]` 散落代码中 → 加新引导方式必须到处加 `#[cfg]`，违反解耦原则。

### 3.3 Paging trait 统一 —— 启动和运行时用同一个 trait

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
    const PTE_HUGE_FLAGS: u64;

    fn map_huge(&mut self, vaddr: VirBytes, paddr: PhysBytes,
                size: usize, flags: PageFlags) -> Result<(), PageTableError>;
    fn supports_huge_page(size: usize) -> bool { ... }
    fn supports_1gb_page() -> bool { ... }
}
```

**为什么 `HUGE_PAGE_SIZE` 放在 `HugePages` 而非 `Paging`**：
- `HugePages` 是 `Paging` 的超集 trait（`HugePages: Paging`），boot 阶段用 `P: HugePages` bound 即可同时访问两者
- 所有目标架构（x86-64/ARM64/RISC-V）都实现了 `HugePages`，boot 阶段大页是必须的（C 源码 `pg_identity()` 和 `pg_mapkernel()` 都用 4MB 大页）
- `map_huge()`、`supports_1gb_page()`、`PTE_HUGE_FLAGS` 等高级操作保留在扩展 trait，保持 `Paging` 核心职责清晰
- 详见 02-stage-vm/06-pagetable-struct.md §3.4 和 §5.3.1 的 trait 职责划分

**页表分配替代 C 的静态池**：Minix3 C 使用 `pagetables[6][1024]` 静态池 + `pg_alloc_page()` 从 memmap 尾部分配。minix-rs 的 `Paging::map()` 和 `HugePages::map_huge()` 内部自行管理页表页的分配——从 `KernelInfo.memmap` 描述的空闲区域中取物理页（简单 bump 分配），通过 UEFI 留下的恒等映射写入（切换 CR3 前仍使用 UEFI 页表，详见 §4.6 `arch_boot_impl` 执行顺序）。不需要 C 的静态池，因为 64 位地址空间需要更多页表页，静态池的固定大小不再适用。

**为什么这是好的**：
- ≥3 个行为不同的实现（x86-64 PML4、aarch64 TTBR1、riscv64 Sv39）→ 多态必要性满足
- 调用者 `arch_boot<P: HugePages>()` 使用 trait bound → trait bound 使用满足
- 描述机制（映射页面、开启 MMU）而非硬件细节（CR3/TTBR0/satp）→ 机制描述满足

**否决的替代方案**：

| 方案 | 否决原因 |
|------|---------|
| `PageTableBoot` trait | 与 `Paging::map/query` 重复，同一硬件两个 trait 说不通 |
| `HUGE_PAGE_SIZE` 移入 `Paging` | 违反 02-stage-vm 的 trait 职责划分（大页是可选能力，非核心 Paging） |

### 3.4 KernelInfo 字段精简

**C 源码依据**：§2.2.1 `kinfo_t` — 26 字段。

**决策**：Rust 只保留 7 字段（`memmap`、`kern_virt_base`、`kern_phys_base`、`kern_size`、`free_upper_idx`、`user_sp`、`boot_modules`）。

| 删除的 C 字段 | 理由 |
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
| `bootstrap_start` / `bootstrap_len` | UEFI 不区分 |
| `boot_procs[]` | 进程管理子系统集成测试构造 |
| `nr_procs` / `nr_tasks` | `boot_modules.len()` 或运行时计算 |
| `release[]` / `version[]` | 编译时 `env!()` 宏 |
| `vm_allocated_bytes` | 运行时从 memmap 计算 |
| `kernel_allocated_bytes` / `kernel_allocated_bytes_dynamic` | 根据 memmap 动态计算 |

**否决的替代方案**：

| 方案 | 否决原因 |
|------|---------|
| 保留全部字段（1:1 翻译 `kinfo_t`） | UEFI 引导路径不产生 `mbi`/`param_buf` 等字段，强行保留需要 `Option<>` 包装，增加无意义复杂度 |
| 保留 `mem_high_phys` 单独字段 | 可从 `memmap.last().end` 推导，冗余字段违反单一数据源原则 |

`free_upper_idx` 统一起名（x86-64 = PML4 索引，aarch64 = TTBR1 L0 索引，riscv64 = VPN[2] 上界）。

**`overlaps()` 的 Rust 等价**：C 源码 `overlaps()`（pre_init.c:77）检查 boot 模块是否与内核镜像重叠。UEFI 引导路径中，`boot-shim` 的 `uefi_helpers` 通过 `GetMemoryMap()` 获取的内存描述已由 UEFI 固件保证不重叠，因此不需要 Rust 等价函数。OpenSBI 路径中，`opensbi_helpers` 使用硬编码的内存映射，bump 分配器起始偏移 4MB 避开内核镜像区域，同样不需要重叠检测。如果未来支持非 UEFI 引导（如 coreboot），需在对应 boot-shim feature module 中实现重叠检测。

### 3.5 4GB 截断删除（架构演进）

**C 源码依据**：§2.3.3 `add_memmap()` — `LIMIT 0xFFFFF000`。

**决策**：Rust 删除 `LIMIT`。64 位有 48 位虚拟 + 52 位物理地址，不需要截断。

### 3.6 集中定义，接口维度分散

`Paging` trait 定义在 `os/arch/src/paging.rs`，`HugePages` trait 定义在 `os/arch/src/paging_ext.rs`（集中定义），各架构实现在 `arch/x86_64/`、`arch/aarch64/`、`arch/riscv64/` 集中。kernel 只依赖 trait，不引用具体类型。

"分散"体现在接口维度而非物理位置：`Paging` 是基础能力（map/unmap/query），`HugePages: Paging` 是大页扩展（map_huge/HUGE_PAGE_SIZE）。消费者按需绑定——boot 阶段用 `P: HugePages`，运行时只用 `P: Paging`。Paging 是横切关注点（vm、kernel、boot 都依赖），没有单一"最近消费者"，因此集中定义更自然。

### 3.7 三层 crate 依赖

```
boot-shim   → minix-types, minix-arch, minix-kernel  # boot-shim 的 uefi feature 还依赖外部库 `uefi = "0.33"`（提供 UEFI 类型和 BootServices）
kernel      → minix-types, minix-arch                # kernel 不依赖 UEFI 外部库，也不依赖 boot-shim
minix-arch  → minix-types                             # minix-arch 定义 Paging trait，无 UEFI 依赖
```

kernel 不依赖 boot-shim，但 boot-shim 依赖 kernel——因为 `arch_boot_impl()` 定义在 kernel 中，boot-shim 调用它完成页表映射和分页开启。KernelInfo 等共享类型下沉到 `minix-types`，两个 crate 通过 `minix-types` 共享数据结构。

---

## 4. 实现详解

> 每个结构的引导语解释核心思路，代码注释标注 C 源码对应。
> 实际代码路径见各节标注。当前代码可通过 `cargo test -p minix-kernel --test boot_integration` 验证。

以下小节按启动顺序排列，读者可按序跟踪完整的代码路径：

```
固件加载 boot-shim (UEFI: .efi / OpenSBI: 裸金属)
  │
  ├── §4.5 boot-shim crate: 获取内存布局、分配页表根页、构造 KernelInfo
  │     ├── uefi_helpers (feature = "uefi"): UEFI GetMemoryMap + ExitBootServices
  │     └── opensbi_helpers (feature = "opensbi"): 硬编码 QEMU virt 内存映射 + bump 分配器
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

> 设计决策：§3.4, §3.5
> 实际文件：`os/libs/minix-types/src/kernel_info.rs+48`

```rust
// minix-types crate 中定义 — boot-shim 和 kernel 共享
pub struct KernelInfo {
    pub memmap: &'static [MemoryRegion],       // 空闲物理内存区域
    pub kern_virt_base: VirBytes,               // 内核虚拟基址
    pub kern_phys_base: PhysBytes,              // 内核物理基址
    pub kern_size: usize,                       // 内核总大小（字节）
    pub free_upper_idx: usize,                  // 页表根级第一个空闲索引（PML4 / TTBR1 L0 / VPN[2]）
    pub user_sp: VirBytes,                      // 用户栈顶地址
    pub boot_modules: &'static [BootModule],    // 启动模块列表（PM/VM/VFS 等）
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
```

### 4.2 Paging trait 扩展 + HugePages trait

> 设计决策：§3.3
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
            // 注意：C 的 vm_enable_paging() 还设置 CR4.PSE（bit 4）以支持 2MB 大页。
            // x86-64 长模式下 1GB 大页不需要 PSE（PDPT.PS 位直接表示 1GB 页），
            // 但如果未来需要 2MB 大页支持，需在此添加 CR4.PSE 设置。
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
    const PTE_HUGE_FLAGS: u64 = 1 << 7;             // PS bit

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

`map_huge` 在 walk 页表时，如果中间级（PML4→PDPT→PD）的页表页不存在，需要分配物理页来放新的页表。这个物理页从哪来？Boot 和 VM 的策略不同：

| 阶段 | 分配策略 | VA↔PA |
|------|---------|-------|
| Boot | Bump（恒等映射区域） | VA = PA |
| VM（未来） | 全局 `VmPageAllocator` | VA = DM_BASE + PA |

**`pt_alloc` 只做注册和转发**，不包含任何分配器实现。它是一个薄层（~60 行）：

```rust
// arch/src/pt_alloc.rs（精简后）
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

理由：boot-shim 只负责"拿根页面、调 arch_boot"，不该知道中间页表页的存在（职责分离）；`arch_boot_impl` 兜底保障鲁棒性——即使调用方忘了注册，内核也能自动初始化。

**代码位置**：`boot_pt_alloc` 定义在 `kernel/src/boot_alloc.rs`，测试内核和 boot-shim 都通过 `use minix_kernel::boot_alloc` 直接引用生产代码：

```rust
// kernel/src/boot_alloc.rs（生产代码）
pub fn init_boot_pt_alloc(base: u64, end: u64) { ... }
fn boot_pt_alloc() -> Result<(PhysBytes, VirBytes), PageTableError> { ... }
```

VM 阶段时，`pt_alloc::register(vm_pt_alloc)` 替换分配器，同时 boot bump 范围从 memmap 切除，防止 VM 的 PhysAlloc 误用。bump 阶段分配的页表页（identity map + kernel map）永久有效、不会泄漏。

### 4.4 引导协议抽象 — BootShim trait

`BootShim` trait 定义在 `minix-types` 中，是引导层与内核层之间的接口。kernel 只认识这个 trait，不认识 UEFI/OpenSBI 等具体实现。`boot-shim` crate 提供 `UefiBootShim` 和 `OpenSbiBootShim` 两个实现，通过 Cargo.toml 的 feature gate 选择编译哪个——代码中不散落 `#[cfg]` 条件编译。

```rust
// os/libs/minix-types/src/kernel_info.rs
pub trait BootShim {
    /// 完成固件相关的引导准备，返回内核所需的全部数据。
    /// 调用后固件服务（如 UEFI BootServices）已不可用。
    fn prepare_boot(
        kern_virt_base: u64,
        kern_phys_base: u64,
        kern_size: usize,
        bump_pages: usize,
    ) -> BootPrepareResult;
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
#![no_std]

#[cfg(feature = "uefi")]
extern crate alloc;

// Re-export trait + result type — 调用者通过 trait 统一接口
pub use minix_types::{BootShim, BootPrepareResult};

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
     │   构建 KernelInfo                       │
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

- **`uefi` feature**（默认启用）：`UefiBootShim` — 使用 UEFI BootServices 获取内存映射、分配页表根页、退出引导服务
- **`opensbi` feature**：`OpenSbiBootShim` — 使用硬编码的 QEMU virt 内存映射和 bump 分配器，无需 UEFI 依赖

两种实现都实现 `BootShim` trait，输出相同的 `BootPrepareResult`，kernel 不感知底层固件类型。

#### 4.5.1 UEFI 路径 (uefi_helpers)

**UEFI 入口**：`boot-shim` 的 `main.rs` 是 UEFI 固件加载 `.efi` 后调用的入口函数。它通过 `BootShim` trait 调用 `UefiBootShim::prepare_boot()` 完成引导准备，然后将控制权交给内核。`main.rs` 本身不包含任何 UEFI 逻辑——所有固件细节封装在 `UefiBootShim` 的 trait 实现中。

```rust
// os/boot-shim/src/main.rs
use minix_types::BootShim;
use boot_shim::UefiBootShim;

#[entry]
fn main() -> Status {
    // 1. UEFI boot preparation via BootShim trait
    //    Internally: GetMemoryMap → AllocatePages → build KernelInfo → ExitBootServices
    let result = UefiBootShim::prepare_boot(0, 0, 0, 8);

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

> **注意**：`main.rs` 只存在于 UEFI 目标（x86_64/aarch64）。riscv64 没有 UEFI 入口——它的入口是裸金属的 `_start` 汇编，直接调用 `OpenSbiBootShim::prepare_boot()`。两种入口最终都通过 `BootShim` trait 统一接口。

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
pub fn build_kernel_info(memmap, kern_virt_base, kern_phys_base, kern_size) -> KernelInfo { /* ... */ }

/// 退出 UEFI BootServices——此后 AllocatePages 等不可用
pub fn exit_boot_services() { /* ... */ }

/// UEFI 引导的 BootShim 实现
pub struct UefiBootShim;

impl BootShim for UefiBootShim {
    fn prepare_boot(
        kern_virt_base: u64,
        kern_phys_base: u64,
        kern_size: usize,
        bump_pages: usize,
    ) -> BootPrepareResult {
        let memmap = build_memmap();
        let root_page = alloc_root_page();
        let (bump_base, bump_end) = alloc_bump_region(bump_pages);
        let kernel_info = build_kernel_info(memmap, kern_virt_base, kern_phys_base, kern_size);

        exit_boot_services();

        BootPrepareResult { kernel_info, root_page, bump_base, bump_end }
    }
}
```

> **设计决策**：`uefi_helpers` 的函数粒度选择——拆分为 5 个独立函数而非只提供 `UefiBootShim::prepare_boot` 一站式接口，是因为测试内核可能只需要其中部分功能（如 `build_memmap` 但不需要 `exit_boot_services`）。`UefiBootShim::prepare_boot` 是便捷封装，内部调用顺序与 `main.rs` 一致。

#### 4.5.2 OpenSBI 路径 (opensbi_helpers)

**OpenSBI 入口**：riscv64 没有 `riscv64-unknown-uefi` Rust 目标，QEMU virt 使用 OpenSBI 作为固件层。启动流程：

```
QEMU -bios default (OpenSBI) → 加载内核 ELF 到 0x8020_0000 → 跳转 _start
```

内核入口需要一段汇编设置栈指针，然后调用 Rust 的 `rust_main`：

```rust
// 测试内核入口（hello-boot-riscv64/src/main.rs）
core::arch::global_asm!(
    ".section .text.init",
    ".global _start",
    "_start:",
    "    la sp, __stack_top",   // 设置栈指针（BSS 段静态数组）
    "    call rust_main",       // 进入 Rust 代码
    "1:",
    "    wfi",                  // 死循环等待中断
    "    j 1b",
);
```

`rust_main` 通过 `BootShim` trait 调用 `OpenSbiBootShim::prepare_boot` 完成引导准备：

```rust
use minix_types::BootShim;
use boot_shim::OpenSbiBootShim;

#[no_mangle]
pub extern "C" fn rust_main() -> ! {
    // 1. OpenSBI 引导准备 via BootShim trait
    let result = OpenSbiBootShim::prepare_boot(0, 0, 0, 8);

    // 2. 注册 boot-stage 页表页分配器
    boot_alloc::init_boot_pt_alloc(result.bump_base, result.bump_end);
    pt_alloc::register(boot_alloc::boot_pt_alloc);

    // 3. 调用内核的通用引导逻辑（恒等映射 + 内核映射 + 开分页）
    minix_kernel::arch_boot(&result.kernel_info, result.root_page);
}
```

**`opensbi_helpers` 模块**提供与 `uefi_helpers` 相同的接口，但实现完全不同：

```rust
// os/boot-shim/src/opensbi_helpers.rs
const DRAM_BASE: u64 = 0x8000_0000;
const DEFAULT_RAM_SIZE: u64 = 0x800_0000; // 128 MB (QEMU virt default)

/// Bump allocator — skips first 4MB (OpenSBI + kernel image + BSS + stack).
fn bump_alloc(num_pages: usize) -> u64 {
    unsafe {
        if BUMP_PTR == 0 {
            // 跳过前 4MB：OpenSBI (1MB at 0x8000_0000) + 内核镜像 + BSS + 栈
            BUMP_PTR = DRAM_BASE + 0x40_0000;
        }
        let addr = BUMP_PTR;
        BUMP_PTR += (num_pages as u64) * 4096;
        addr
    }
}

/// Hardcoded memory map for QEMU virt — 不需要运行时发现
pub fn build_memmap() -> &'static [MemoryRegion] {
    const REGION: MemoryRegion = MemoryRegion {
        base: PhysBytes(DRAM_BASE),
        len: DEFAULT_RAM_SIZE as usize,
    };
    &[REGION]  // const 静态数组，无需堆分配
}

/// OpenSBI 引导的 BootShim 实现
pub struct OpenSbiBootShim;

impl BootShim for OpenSbiBootShim {
    fn prepare_boot(
        kern_virt_base: u64,
        kern_phys_base: u64,
        kern_size: usize,
        bump_pages: usize,
    ) -> BootPrepareResult {
        let memmap = build_memmap();
        let root_page = alloc_root_page();
        let (bump_base, bump_end) = alloc_bump_region(bump_pages);
        let kernel_info = build_kernel_info(memmap, kern_virt_base, kern_phys_base, kern_size);

        // 无 exit_boot_services() — OpenSBI 不提供 BootServices
        BootPrepareResult { kernel_info, root_page, bump_base, bump_end }
    }
}
```

**与 UEFI 路径的关键差异**：
- **无 `exit_boot_services()`** — OpenSBI 不提供 BootServices，内核已在 S-mode 运行，无需"退出"
- **内存映射硬编码** — QEMU virt 平台的 DRAM 布局固定（0x8000_0000 起 128MB），不需要运行时发现
- **bump 分配器起始偏移 4MB** — 避开 OpenSBI (1MB at 0x8000_0000) + 内核镜像 + BSS + 栈
- **`build_memmap` 使用 const 静态数组** — 不需要 `Box::leak`，因为内存布局在编译期已知


### 4.6 kernel 入口 — lib.rs

```rust
// os/kernel/src/lib.rs
#[cfg(target_arch = "x86_64")]
pub fn arch_boot(kernel_info: &KernelInfo, root_page: PhysBytes) -> ! {
    use minix_arch::x86_64::paging::X86_64Paging;
    let info = arch_boot_impl::<X86_64Paging>(kernel_info, root_page);
    kmain(info)
}
// 同模式 aarch64 → Aarch64Paging, riscv64 → Riscv64Paging

pub fn arch_boot_impl<P: HugePages>(kernel_info: &KernelInfo, root_page: PhysBytes) -> &KernelInfo {
    // Step 0: 注册 boot_pt_alloc 分配器（如未注册）
    if !pt_alloc::is_registered() {
        boot_alloc::init_boot_pt_alloc(base, boot_alloc_end);
        pt_alloc::register(boot_alloc::boot_pt_alloc);
    }

    let mut paging = P::new_from_page(root_page);
    let huge_size = P::HUGE_PAGE_SIZE as usize;

    // Step 1: 恒等映射 — VA=PA 前 4GB (C: pg_identity — pg_utils.c:162)
    // 使用 kernel_read_write() | EXECUTABLE：supervisor-only + 可执行
    // （RISC-V Sv39 中 U=1 的页面不能被 supervisor 执行，除非 sstatus.SUM=1）
    //
    // 与 C 的差异：C 的 pg_identity() 对超出 mem_high_phys 的区域设置 PWT|PCD（禁用缓存），
    // Rust 版本不区分。原因：64位 UEFI 环境下，固件已正确设置 MTRR（Memory Type Range
    // Register），硬件层面的缓存属性由 MTRR 控制，软件页表属性中的 PWT|PCD 只是建议；
    // 且恒等映射在分页切换后立即被内核高地址映射替代，短暂存在期间不需要精确控制缓存。
    let id_flags = PageFlags::kernel_read_write() | PageFlags::EXECUTABLE;
    let mut addr: u64 = 0;
    while addr < 0x1_0000_0000 {
        let _ = paging.map_huge(VirBytes(addr), PhysBytes(addr),
                        huge_size, id_flags);
        addr += huge_size as u64;
    }

    // Step 2: 内核高半核映射 (C: pg_mapkernel — pg_utils.c:186)
    let kern_flags = PageFlags::kernel_read_write() | PageFlags::EXECUTABLE;
    let mut offset = 0u64;
    while offset < kernel_info.kern_size as u64 {
        paging.map_huge(
            VirBytes(kernel_info.kern_virt_base.0 + offset),
            PhysBytes(kernel_info.kern_phys_base.0 + offset),
            huge_size, kern_flags).expect("kernel map failed");
        offset += huge_size as u64;
    }

    // Step 3: 开分页 (C: pg_load + vm_enable_paging)
    unsafe { paging.enable() };

    // 注意：C 的 pg_mapkernel() 返回第一个空闲 PDE 号存入 kinfo.freepde_start，
    // Rust 版本未更新 kernel_info.free_upper_idx（始终为 0）。
    // 原因：64位内核使用动态页表分配（Paging::map 按需分配中间页表页），
    // 不再需要静态的"空闲 PDE 起始号"来划分用户/内核地址空间边界。
    // 如果未来需要此值，可在 Step 2 后计算并设置。

    kernel_info
}
```

> **Step 3 的实际含义**：在 Minix3 C 版本中，`vm_enable_paging()` 是真正"开分页"（CR0.PG 从 0 变 1）。在 UEFI 路径下，MMU 一直是开的（x86-64 长模式硬件要求分页，ARM64/RISC-V UEFI 规范也要求 MMU 开启），所以 `paging.enable()` 的实际操作是**切换页表基址**（x86-64 写 CR3，ARM64 写 TTBR0，RISC-V 写 satp），从 UEFI 的恒等映射切换到内核自己的映射。保留"开分页"的语义是为了兼容非 UEFI 引导路径（如 coreboot），此时 `enable()` 才真正开启 MMU。

> `#[cfg(target_arch)]` 在入口中是**编译时选择编译单元**，不是在运行时根据硬件特性选择代码路径。上层代码只依赖 trait。
>
> `P: HugePages` bound 替代了原来的 `P: Paging`。因为 `HugePages: Paging`，boot 阶段自动获得 `Paging` 的所有方法（`new_from_page`、`enable`、`map` 等），同时可直接访问 `P::HUGE_PAGE_SIZE` 和 `P::map_huge()`。

> **与 C 源码的差异**：C 的 `pg_mapkernel()` 只设 `PRESENT | BIGPAGE | WRITE`，无 GLOBAL 位。Rust 代码中 `PageFlags::kernel_read_write()` 含 GLOBAL，boot 阶段无实际作用（无进程切换，CR3 不变）。GLOBAL 位的真正价值在 VM 的 Direct Map 中——每次进程切换重写 CR3 时避免内核映射 TLB miss。

### 4.7 启动执行链路

从固件交控制权到内核 `kmain()`，经过三个阶段、两次控制权转移：

```
固件 (UEFI / OpenSBI)
  │
  ▼
boot-shim (§4.5)                    ← 第 1 次控制权转移：固件 → boot-shim
  │  prepare_boot():
  │    1. 获取内存映射 (UEFI: GetMemoryMap / OpenSBI: 硬编码)
  │    2. 分配根页表页 (root_page)
  │    3. 分配 bump 区域
  │    4. 构造 KernelInfo
  │    5. [UEFI] ExitBootServices
  │    6. 调用 arch_boot(kernel_info, root_page)
  ▼
kernel arch_boot (§4.6)             ← 第 2 次控制权转移：boot-shim → kernel
  │  arch_boot_impl::<Paging>():
  │    Step 0: 注册 boot_pt_alloc 分配器
  │    Step 1: 恒等映射 (VA=PA)
  │    Step 2: 内核高半核映射
  │    Step 3: 切换页表 (enable → 写 CR3/TTBR1/satp)
  ▼
kmain()                             ← 内核正式运行，此后由 IPC 驱动
```

**关键设计点**：

- **boot-shim 不碰页表**：它只准备数据（KernelInfo + root_page + bump_region），页表操作全部由 kernel 完成。这样 boot-shim 不依赖 arch crate 的 Paging 实现，保持最小职责
- **arch_boot 是编译期分派**：`#[cfg(target_arch)]` 选择具体 Paging 类型，喂给泛型 `arch_boot_impl<P>`。所有架构共享同一套 boot 流程
- **Step 0→3 的顺序不可调换**：先注册分配器（Step 0），否则 `map_huge` 分配中间页表页会 panic；先建映射（Step 1+2），否则切换页表后内核找不到自己；最后切换页表（Step 3），切换后脚手架生效

### 4.8 与 Minix3 C 的函数对照

| C 函数 (Ch2) | Rust 实现 | 位置 |
|-------------|----------|------|
| `pre_init(magic, ebx)` | 不需要 — UEFI 替代 | — |
| `get_parameters(ebx)` | `prepare_boot()` GetMemoryMap / 硬编码 memmap | `boot-shim/src/uefi_helpers.rs` 或 `opensbi_helpers.rs` |
| `add_memmap()` | 不需要 — UEFI 直接返回 | — |
| `cut_memmap()` | 不需要 — UEFI 已扣减 | — |
| `overlaps()` | 不需要 — UEFI 固件保证不重叠 | — |
| `alloc_lowest()` | 不需要 — UEFI 分配器替代 | — |
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

1. **控制权转移正确**：boot-shim 能正确获取内存映射、定位内核、构造 KernelInfo、调用 arch_boot——任何一步失败都意味着内核根本跑不起来
2. **页表脚手架正确**：恒等映射和高半核映射覆盖了正确的地址范围，切换页表后内核能通过高地址访问自身代码/数据——映射遗漏或权限错误会导致切换后立即崩溃
3. **三架构行为一致**：同一套 `arch_boot_impl<P>` 泛型代码在三个架构上产生相同的语义——架构实现差异（PML4 四级 vs Sv39 三级）不应导致上层行为分歧

### 5.1 单元测试（mock，`#[cfg(test)]`）

- `MemoryRegion` 的重叠检测、对齐、转换为 PFN 范围 — 保证 KernelInfo 传给内核的内存描述不含重叠/未对齐区域，否则后续 `map_huge` 会映射到错误的物理页
- `KernelInfo` 构造器从 mock UEFI mmap 构建 — 保证 boot-shim 到 kernel 的数据传递正确，KernelInfo 是两者之间唯一的契约，构造错误意味着内核基于错误的信息做所有决策
- `Paging` mock 实现的 `new_from_page()` + `enable()` 不 panic — 保证 Paging trait 的接口契约可被实现，泛型代码 `arch_boot_impl<P>` 能正确调用，这是上层代码能工作的前提

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
            ├── hello-boot-riscv64/      # riscv64: opensbi_helpers → arch_boot_impl<Riscv64Paging> → PASS
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

所有架构的 hello-boot 均已集成对应 Paging trait 实现（`X86_64Paging`/`AArch64Paging`/`Riscv64Paging`），走真实的 `boot_shim::prepare_boot → arch_boot_impl::<P> → enable` 启动路径。riscv64 使用 `boot-shim` 的 `opensbi` feature，通过 OpenSBI 固件直接启动裸金属内核，无需 UEFI 依赖。

测试用例：

| 测试 | 覆盖范围 | 验证目标 |
|------|---------|---------|
| **hello-boot** (x86_64) | UEFI 启动 → UefiBootShim::prepare_boot → arch_boot_impl\<X86_64Paging\> → 串口输出 | 全链路可达：boot-shim 能正确构造 KernelInfo 并调用 arch_boot，X86_64Paging 能完成 identity map + enable 不崩溃 |
| **hello-boot** (aarch64) | UEFI 启动 → UefiBootShim::prepare_boot → arch_boot_impl\<AArch64Paging\> → 串口输出 | 同上，验证 AArch64Paging 在真实硬件模型上行为一致 |
| **hello-boot** (riscv64) | OpenSBI 启动 → OpenSbiBootShim::prepare_boot → arch_boot_impl\<Riscv64Paging\> → 串口输出 | 同上，验证 Riscv64Paging 在 OpenSBI 路径下行为一致 |
| **test-memmap** | uefi_helpers::build_memmap → 断言 memmap 非空 + 有低地址 CONVENTIONAL | KernelInfo.memmap 反映了真实的物理内存布局——如果 memmap 为空或遗漏内核所在区域，后续恒等映射会跳过内核自身，切换页表后立即崩溃 |
| **test-paging-enable** | uefi_helpers → arch_boot_impl → 串口输出 PASS | `paging.enable()` 切换页表基址后 CPU 仍能继续执行——这是最基本的安全断言，enable 后串口能输出说明内核代码段映射正确 |
| **test-kernel-map** | arch_boot_impl(高半核) → 读高/低地址 sentinel → 断言值一致 | 高半核映射语义正确——内核通过高地址（如 `0xFFFF800000000000`）访问自身数据，值与低地址恒等映射一致，证明 Step 2 的 `kern_virt_base → kern_phys_base` 映射无偏移错误 |

---

## 6. 参见

- [00-kernel-overview.md](00-kernel-overview.md) — Kernel 整体架构概览
- [02-page-table-kernel.md](02-page-table-kernel.md) — 内核页表操作（memory.c 核心）
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
              ├── 调用 ExitBootServices() ← 交还硬件控制权
              └── 进入纯裸机模式，UEFI 不再干预
```

### A.3 核心概念速查

| 概念 | 含义 | 文档中出现位置 |
|------|------|-------------|
| **`.efi` 文件** | UEFI 可执行文件格式（PE32+），相当于 Linux 的 ELF | §3.7、§5.2 |
| **`ImageHandle`** | UEFI 传给 `.efi` 入口函数的第一个参数，代表"你自己这个程序" | §1.7.1、§4.4 |
| **`SystemTable<Boot>`** | UEFI 传给 `.efi` 入口函数的第二个参数，包含所有 Boot Services 函数指针 | §4.4 |
| **`BootServices`** | UEFI 提供的服务函数集（内存分配、协议查询等），`ExitBootServices()` 后不可用 | §3.2 |
| **`GetMemoryMap()`** | BootServices 函数，返回物理内存布局（哪些可用、哪些被占用） | §1.7.1、§3.1、§3.5 |
| **`AllocatePages()`** | BootServices 函数，分配物理内存页（1 页 = 4KB） | §1.7.3、§4.4 |
| **`ExitBootServices()`** | BootServices 函数，告知 UEFI "我不再需要你了"，UEFI 释放所有资源，此后内核独占硬件 | §3.2、§1.7.3 |
| **OVMF** | Open Virtual Machine Firmware——QEMU 用的 UEFI 固件实现，让 QEMU 能跑 UEFI 启动 | §1.7.1、§3.7 |

### A.4 UEFI vs Multiboot：角色对照

如果你熟悉 Multiboot/GRUB，这个对照表帮你快速理解 UEFI 的等价物：

| Multiboot/GRUB | UEFI | 说明 |
|----------------|------|------|
| GRUB 读取 `boot.cfg` | UEFI Boot Manager 读取 `BootOrder` 变量 | 决定加载谁 |
| `multiboot /boot/kernel` | 加载 `boot.efi` | 加载可执行文件 |
| Multiboot Header（`0x1BADB002`） | PE32+ 头（标准可执行格式） | 固件如何识别可执行文件 |
| EAX/EBX 寄存器传参 | `ImageHandle` + `SystemTable` 参数传参 | 如何传递启动信息 |
| `multiboot_info_t`（GRUB 填充） | `GetMemoryMap()` 返回值（UEFI 填充） | 内存布局信息 |
| `load_mods` 加载模块 | `ImageHandle` protocol / LoadImage | 加载额外二进制 |
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

*分类: Kernel 硬件发现*