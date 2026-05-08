# 00-vm-overview: VM 整体架构概览

> **分类**: VM整体层级  
> **说明**: 汇总 VM 模块的全局概念、设计原则和跨组件约定

---

## 1. VM 的双重身份

### 1.1 As Server
// TODO: VM 作为内存管理服务器，处理 IPC 请求

### 1.2 As Library
// TODO: VM 提供的库功能，可被其他组件使用

### 1.3 策略 vs 机制：VM 的本质定位

> **核心原则**: Kernel 提供**机制**，VM 提供**策略**。

VM server 是**独立的用户态进程**，它不直接操作硬件，也不直接访问物理内存。它的角色可以用一个比喻来理解：

> VM 像一个"内存策略引擎"——kernel 告诉它"世界是什么样的"（有哪些物理内存可用），它决定"怎么分配这些内存"（给谁、给多少、何时回收），然后请求 kernel 执行实际的硬件操作。

```
┌──────────────────────────────────────────────────────────────┐
│                    策略 vs 机制分层                            │
├──────────────────────────────────────────────────────────────┤
│                                                              │
│  Kernel（机制层）                                             │
│  ├── 解析 UEFI/BIOS 内存 map，发现物理内存                    │
│  ├── 建立初始页表（恒等映射 + 内核映射）                       │
│  ├── 执行页表修改（CR3 / TLB 刷新）                           │
│  ├── 执行物理内存映射（vm_map_phys）                          │
│  └── 通过 IPC 将内存信息传递给 VM                             │
│          │                                                   │
│          │  sys_getkinfo() → kinfo.memmap[]                  │
│          │  sys_vm_map_phys() → 实际映射                      │
│          ▼                                                   │
│  VM Server（策略层）                                          │
│  ├── 接收 kernel 提供的物理内存描述数据                        │
│  ├── 决定物理页分配策略（alloc_mem / free_mem）                │
│  ├── 决定进程地址空间布局（vir_region / CoW）                  │
│  ├── 决定页错误处理策略（按需分配 / swap）                     │
│  └── 请求 kernel 执行特权操作                                 │
│          │                                                   │
│          │  VM_FORK / VM_BRK / VM_MAP                        │
│          ▼                                                   │
│  用户进程                                                     │
│  └── 通过系统调用请求内存服务                                  │
│                                                              │
└──────────────────────────────────────────────────────────────┘
```

**VM 能做什么**:
- ✅ 管理物理页的分配策略（位图 / free list）
- ✅ 管理进程的虚拟地址空间布局
- ✅ 决定 CoW / 按需分配等策略
- ✅ 通过 IPC 请求 kernel 执行特权操作

**VM 不能做什么**:
- ❌ 直接操作页表硬件（CR3 / TLB）
- ❌ 直接访问物理内存地址
- ❌ 探测物理内存（UEFI / e820）
- ❌ 处理中断 / 异常（由 kernel 转发）

**关键洞察**: VM 看到的"物理内存"只是 kernel 传递给它的**数据描述**（`PhysRegion { base, size }`），而非物理内存本身。VM 管理的是这些描述数据的策略，实际的硬件操作全部由 kernel 执行。

### 1.4 架构位置

VM 在 Minix3 微内核架构中的位置：

```
用户进程 ←→ 内核 ←→ VM (管理 vmproc) ←→ PM (管理进程生命周期)
                ↓
            页表、物理内存、区域管理
```

**协作关系**:
- **PM (Process Manager)**: 负责逻辑进程状态（pid、信号、调度等）
- **VM (Virtual Memory)**: 负责内存相关状态（页表、虚拟区域、物理内存等）
- **内核**: 捕获页错误，转发给 VM 处理；提供底层内存管理原语

**数据流**:
1. 用户进程发起内存相关系统调用（如 fork、brk、mmap）
2. 内核捕获并转发给对应服务（PM 或 VM）
3. PM 处理逻辑状态，必要时调用 VM 处理内存状态
4. VM 更新页表、区域等数据结构
5. 返回结果给用户进程

---

## 2. VM 在 Minix3 中的职责

### 2.1 微内核架构中的角色

Minix3 采用微内核架构，将传统单体内核的功能拆分为多个用户态服务：

```
┌─────────────────────────────────────────┐
│              用户进程层                  │
│         (应用程序、Shell 等)              │
├─────────────────────────────────────────┤
│              系统服务层                  │
│  ┌─────┐ ┌─────┐ ┌─────┐ ┌─────┐       │
│  │ PM  │ │ VM  │ │ VFS │ │ RS  │ ...   │
│  └──┬──┘ └──┬──┘ └──┬──┘ └──┬──┘       │
├─────┼───────┼───────┼───────┼──────────┤
│     └───────┴───────┴───────┘          │
│              微内核层                    │
│    (进程调度、中断处理、IPC 机制)         │
└─────────────────────────────────────────┘
```

**VM 的核心职责**:
- 管理进程的虚拟地址空间
- 分配和回收物理内存
- 处理页错误（Page Fault）
- 实现写时复制（Copy-on-Write）
- 提供内存映射（mmap）服务

### 2.2 与其他服务的关系

#### 2.2.1 与 PM 的协作
- **进程创建**: PM 决定创建进程，VM 复制内存空间
- **进程退出**: PM 通知 VM 清理内存资源
- **进程查找**: PM 和 VM 使用相同的 slot 号标识进程

#### 2.2.2 与 VFS 的协作
- **文件映射**: VFS 提供文件信息，VM 建立内存映射
- **页缓存**: VM 管理文件数据的内存缓存

#### 2.2.3 与内核的协作
- **页错误**: 内核捕获页错误，转发给 VM
- **系统调用**: 内核将内存相关系统调用路由到 VM
- **特权操作**: VM 通过系统调用请求内核执行特权操作

### 2.3 启动阶段物理内存初始化

> 本节描述从硬件启动到 VM 开始管理物理内存的完整链路。这是理解"VM 的物理内存从哪来"的关键背景。

**与单体内核的区别**: 在 Linux/xv6 等单体内核中，物理内存的发现（UEFI/e820）和管理（buddy allocator）在同一个执行主体中完成。在 Minix3 微内核中，这条链路被拆分到 kernel 和 VM 两个不同的执行主体中，中间通过 IPC 传递信息。

#### 2.3.1 完整初始化链路

```
┌─────────────────────────────────────────────────────────────────────┐
│                    物理内存初始化的四个阶段                           │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  阶段 1: Kernel 获取内存 map（pre_init）                             │
│  ─────────────────────────────────────                               │
│  Bootloader (GRUB)                                                  │
│    │  multiboot_info_t（ebx 寄存器）                                 │
│    │  ├── MULTIBOOT_INFO_HAS_MMAP → 完整内存 map                    │
│    │  └── mi_mem_lower / mi_mem_upper → 基本内存大小                 │
│    ▼                                                                │
│  pre_init() → get_parameters()                                      │
│    │  遍历 multiboot_memory_map_t                                   │
│    │  只保留 MULTIBOOT_MEMORY_AVAILABLE 的区域                       │
│    │  add_memmap(cbi, base, length)                                 │
│    │  cut_memmap() 扣除 kernel 自身 + boot modules 占用              │
│    ▼                                                                │
│  kinfo.memmap[]  ←  可用物理内存区域列表                             │
│                                                                     │
│  阶段 2: Kernel 建立恒等映射（pre_init）                             │
│  ─────────────────────────────────────                               │
│  pg_identity(&kinfo)    ← 4MB 大页恒等映射：VA = PA                  │
│  pg_mapkernel()         ← 映射内核到高地址                           │
│  pg_load() + vm_enable_paging()  ← 开启分页                         │
│                                                                     │
│  阶段 3: VM 通过 IPC 获取内存 map（vm/main.c）                       │
│  ─────────────────────────────────────                               │
│  sys_getkinfo(&kernel_boot_info)  ← IPC 请求 kernel 拷贝 kinfo      │
│  get_mem_chunks(mem_chunks)        ← 转换为 click 单位               │
│                                                                     │
│  阶段 4: VM 初始化物理内存管理器（vm/alloc.c）                        │
│  ─────────────────────────────────────                               │
│  mem_init(mem_chunks)              ← 初始化空闲页位图                 │
│  → 详见 [04-physical-memory.md](04-physical-memory.md)              │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

#### 2.3.2 各阶段源码位置

| 阶段 | 代码位置 | 关键函数 |
|------|---------|---------|
| 1. 解析 Multiboot | [pre_init.c](minix3/minix/kernel/arch/i386/pre_init.c) | `get_parameters()`, `add_memmap()`, `cut_memmap()` |
| 2. 恒等映射 | [pg_utils.c](minix3/minix/kernel/arch/i386/pg_utils.c) | `pg_identity()`, `pg_mapkernel()`, `vm_enable_paging()` |
| 3. VM 获取内存信息 | [main.c](minix3/minix/servers/vm/main.c), [utility.c](minix3/minix/servers/vm/utility.c) | `sys_getkinfo()`, `get_mem_chunks()` |
| 4. VM 初始化分配器 | [alloc.c](minix3/minix/servers/vm/alloc.c) | `mem_init()` |

#### 2.3.3 数据流转

```
Multiboot (GRUB)
    │
    │  multiboot_info_t
    ▼
kernel: kinfo.memmap[NR_MEMS]          ← 阶段 1 产出
    │    (multiboot_memory_map_t[])
    │    已扣除: kernel, boot modules, BIOS 保留区
    │
    │  sys_getkinfo() IPC
    ▼
VM: kernel_boot_info.memmap[]          ← 阶段 3 产出
    │
    │  get_mem_chunks() 转换
    ▼
VM: mem_chunks[NR_MEMS]               ← click 单位的内存块
    │    (struct memory { base, size })
    │
    │  mem_init() 初始化
    ▼
VM: free_pages_bitmap[]               ← 阶段 4 产出
       位图：1 bit = 1 page (4KB)
```

**关键点**: VM 从未直接接触 UEFI/BIOS 或物理硬件。它看到的"物理内存"只是 kernel 通过 IPC 传递的**数据描述**——一组 `{ base, size }` 结构体。VM 在此基础上构建自己的管理策略。

### 2.4 VM 初始化顺序与堆可用时机

> **核心问题**: VM 是系统的内存分配器，它自己什么时候可以使用堆？

#### 2.4.1 完整初始化时序

```
┌─────────────────────────────────────────────────────────────────────┐
│                    VM 初始化时序与堆可用性                            │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  阶段 0: 内核启动 VM 进程                                            │
│  ─────────────────────────                                          │
│  ├── 创建初始页表（映射代码段、数据段、栈）                           │
│  ├── BSS 段清零（包括 static_sparepages[STATIC_SPAREPAGES 页]）     │
│  └── 堆状态: ❌ 不可用（只能使用栈和静态变量）                        │
│                                                                     │
│  阶段 1: init_vm() 开始                                             │
│  ─────────────────────────                                          │
│  ├── sys_getkinfo(&kernel_boot_info)  ← IPC 获取内核信息            │
│  ├── get_mem_chunks(mem_chunks)       ← 转换为 click 单位           │
│  ├── memset(vmproc, 0)                ← 清零进程表                  │
│  │     → 对应文档: 01-vmproc-struct, 02-vmproc-table               │
│  ├── acl_init()                       ← ACL 初始化                  │
│  │     → 对应文档: 03-acl                                           │
│  └── 堆状态: ❌ 不可用                                               │
│                                                                     │
│  阶段 2: mem_init(mem_chunks)                                       │
│  ─────────────────────────                                          │
│  ├── 初始化 free_pages_bitmap[]       ← 物理内存分配器              │
│  │     → 对应文档: 04-physical-memory                               │
│  ├── alloc_mem() 可用                 ← 可以分配物理页              │
│  └── 堆状态: ❌ 仍不可用（_brk 尚未就绪）                            │
│                                                                     │
│  阶段 3: pt_init()                                                  │
│  ─────────────────────────                                          │
│  ├── 初始化 VM 自己的页表                                            │
│  │     → 对应文档: 06-pagetable-struct, 07-pagetable-ops            │
│  ├── 创建保留页池（spare_pagequeue）                                │
│  │     → 对应文档: 05-vm-allocpage                                  │
│  ├── _brk() 可用                      ← VM 可以扩展自己的堆         │
│  └── 堆状态: ✅ 可用！malloc/calloc 可以使用                         │
│                                                                     │
│  阶段 4: init_vm() 返回后                                           │
│  ─────────────────────────                                          │
│  ├── sef_local_startup()              ← SEF 框架初始化              │
│  ├── 主循环开始                        ← 处理 IPC 请求               │
│  │     → 对应文档: 12-vir-region ~ 19-vm-map                        │
│  └── 堆状态: ✅ 完全可用                                             │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

#### 2.4.2 关键边界：pt_init()

```
                    pt_init() 是分界线
                           │
        ┌──────────────────┼──────────────────┐
        │                  │                  │
        ▼                  ▼                  ▼
   堆不可用            堆开始可用          堆完全可用
        │                  │                  │
   只能用:            可以用:            可以用:
   - 静态变量         - _brk()           - malloc()
   - 栈变量           - alloc_mem()      - calloc()
   - BSS 段           - 保留页池         - free()
                                          - Vec, Box 等
```

#### 2.4.3 各组件的堆依赖

| 组件 | 初始化时机 | 堆依赖 | 说明 |
|------|-----------|--------|------|
| vmproc 结构体 | `memset(vmproc, 0)` | ❌ 无 | 静态数组 |
| vmproc 表 | `memset(vmproc, 0)` | ❌ 无 | 静态数组 |
| ACL | `acl_init()` | ❌ 无 | 静态数据 |
| 物理内存分配器 | `mem_init()` | ❌ **禁止** | 必须静态分配 |
| 页表结构 | `pt_init()` | ⚠️ 保留页 | 使用保留页池 |
| 保留页池 | `pt_init()` | ❌ 无 | BSS 段静态内存 |
| 虚拟区域 | 运行时 | ✅ 可用 | fork/mmap 时 |
| Slab 分配器 | 运行时 | ✅ 可用 | 使用 malloc |
| IPC 处理 | 运行时 | ✅ 可用 | 处理请求时 |

#### 2.4.4 Rust 实现约束

> 以下约束基于 §2.4.3 的堆依赖分析，指导 Rust 实现中各阶段可用的类型。

**阶段 1-2（堆不可用）**：不能使用 `Vec`, `Box`, `String` 等 heap 类型，必须使用静态数组或栈分配，`BitmapAllocator` 的 bitmap 必须静态分配。

**阶段 3（堆开始可用）**：可以使用 `_brk()` 扩展堆，可以使用保留页池分配关键结构，页表操作需要使用保留页池。

**阶段 4（堆完全可用）**：可以自由使用 `Vec`, `Box` 等，可以使用 `malloc`/`free`，IPC 处理、区域管理等可以使用堆。

> **详见**: [05-vm-allocpage.md](05-vm-allocpage.md) §2.4 - VM 堆初始化与保留页池的完整分析。

---

## 3. 文档导航：主线叙事与补全路径

### 3.1 叙事策略：以 fork 系统调用为主线

VM Server 涉及的概念极多——物理内存、页表、虚拟区域、CoW、页错误、mmap……如果平铺直叙地逐个介绍，读者容易迷失在概念海洋中。

因此，01-19 采用**单一主线叙事**：跟随 `fork` 系统调用的执行路径，按需引入每个概念。fork 是最复杂的内存操作之一，它几乎触及 VM 的所有核心组件：

```
fork 请求到达
  │
  ├── 需要进程表 → 01 vmproc 结构 / 02 vmproc 表
  ├── 需要权限检查 → 03 ACL
  ├── 需要分配物理页 → 04 物理内存 / 05 页分配
  ├── 需要页表 → 06 页表结构 / 07 页表操作
  ├── 需要内核堆 → 08 slab 分配器
  ├── 需要搬迁元数据 → 09 VM 重定位
  ├── 需要物理块 → 10 PhysBlock / 11 MemType / 14 PhysRegion
  ├── 需要虚拟区域 → 12 VirRegion / 13 AVL 树
  ├── 需要写时复制 → 15 CoW 机制
  ├── 需要页错误 → 16 页错误处理
  ├── 执行 fork → 17 VM Fork
  ├── 需要堆扩展 → 18 brk
  └── 需要内存映射 → 19 mmap
```

**核心思路**：每个文档只在 fork 需要它时才出现，读者始终知道"为什么现在要学这个"。

### 3.2 01-19：fork 主线（已完成）

fork 主线构建了 VM 的**完整骨架**——所有核心数据结构和抽象层都已建立。以下是各文档在主线中的角色：

| 编号 | 文档 | 主线角色 | 引入的核心抽象 |
|------|------|---------|--------------|
| 01 | vmproc-struct | fork 的目标：进程在 VM 中的表示 | `VmProc` 结构体 |
| 02 | vmproc-table | fork 需要查找/分配进程槽 | 进程表、slot/endpoint |
| 03 | acl | fork 需要权限检查 | ACL 位图 |
| 04 | physical-memory | fork 需要分配物理页 | `PhysAllocator` trait |
| 05 | vm-allocpage | fork 需要页分配器 | `VmPageAllocator`、`ReservedRegion` |
| 06 | pagetable-struct | fork 需要创建子进程页表 | `Paging` trait、`DirectMapArch` |
| 07 | pagetable-ops | fork 需要操作页表 | `map`/`unmap`/`remap`/`query` |
| 08 | slab-allocator | fork 使用的堆分配器 | Slab 分配器 |
| 09 | vm-relocation | fork 前需完成元数据搬迁 | 重定位接口 |
| 10 | phys-block | fork 的 CoW 需要物理块引用计数 | `PhysBlock`、引用计数 |
| 11 | memtype | fork 需要知道内存类型语义 | `MemType` trait |
| 12 | vir-region | fork 需要复制虚拟区域 | `VirRegion`、AVL 树 |
| 13 | region-avl | fork 的区域查找需要 AVL | AVL 平衡树操作 |
| 14 | phys-region | fork 需要链接物理区域到物理块 | `PhysRegion` |
| 15 | cow-mechanism | fork 设置 CoW 保护 | CoW 标记、页表只读 |
| 16 | pagefault | CoW 页面写入时触发页错误 | 页错误解析 |
| 17 | vm-fork | fork 的完整执行流程 | `do_fork()` |
| 18 | vm-brk | fork 后子进程可能扩展堆 | `do_brk()` |
| 19 | vm-map | fork 后可能 mmap | `do_mmap()` |

### 3.3 20-27：补全阶段（fork 主线完成后）

fork 主线完成后，VM 的主体框架已经建立。剩余内容是**在已有骨架上填充操作**——不需要再找新的系统调用做主线，直接按功能补全即可。

补全阶段的组织原则是**教学性优先**：每个文档回答读者此刻最想知道的问题，顺着好奇心走。

| 编号 | 文档 | 读者心中的问题 | 覆盖的 Minix3 模块 | 文档结构 |
|------|------|---------------|-------------------|---------|
| 20 | cow-exec-pagefault | "写入 CoW 页面时到底怎么复制？" | mem_cow, pt_writemap, map_ph_writept, do_pagefaults | 完整 Ch1-4 |
| 21 | vm-exit | "fork 的反面——进程怎么退出？" | exit.c, pt_free, map_free_proc | 完整 Ch1-4 |
| 22 | vm-brk-complete | "brk 完整逻辑是什么？" | real_brk, map_region_extend_upto_v | 完整 Ch1-4 |
| 23 | vm-munmap | "怎么取消映射？" | do_munmap, map_unmap_region/range, do_map_phys | 完整 Ch1-4 |
| 24 | vfs-interaction | "VM 怎么和 VFS 对话？" | vfs_request/reply, fdref, mem_file 补全 | 完整 Ch1-4 |
| 25 | client-alloc-lib | "其他服务器怎么用 VM 分配内存？" | minix_alloc crate 设计 | Ch1-3，Ch4=TODO |
| 26 | cache-memtypes | "缓存、共享内存、连续内存呢？" | cache.c, mem_cache/shared/contig | 完整 Ch1-4 |
| 27 | vm-init-main | "所有零件怎么组装启动？" | init_vm, pt_init, SEF, 主循环, 工具函数 | 完整 Ch1-4 |

**补全阶段的叙事线**：

```
兑现承诺          生命周期闭合       补全接口           走出 VM              变体扩展         回到起点        收尾
   │                 │                │                  │                   │              │             │
   ▼                 ▼                ▼                  ▼                   ▼              ▼             ▼
20 CoW执行       21 进程退出      22 brk补全        24 VFS交互          26 缓存+       27 初始化+     27(续)
+ 页错误                         23 munmap         + fdref              内存类型       主循环
                                                  25 客户端内存库
   │                 │                │                  │                   │              │
   └─────────────────┴────────────────┴──────────────────┴───────────────────┴──────────────┘
                                        VM Server 完成
```

> **遗留逻辑完整清单**: [minix3_missed.md](minix3_missed.md) 对照 Minix3 源码列出了所有尚未实现的函数，20-27 的内容即来源于此。

---

## 4. VM 全局概念

### 4.1 进程标识

#### 4.1.1 vmproc 与进程的关系
// TODO: 每个进程在 VM 中有一个 vmproc 条目

#### 4.1.2 slot 的概念
// TODO: 进程表索引，与 endpoint 的关系

### 4.2 内存管理抽象

#### 4.2.1 虚拟地址空间
// TODO: 每个进程的独立地址空间

#### 4.2.2 物理内存管理
// TODO: VM 作为物理内存的分配者

#### 4.2.3 页表管理
// TODO: 两级页表结构

### 4.3 核心数据结构关系

```
// TODO: vmproc → page_table → vir_region(AVL) → phys_region → phys_block
```

### 4.4 全局状态

VM 服务维护以下全局状态：

| Minix3 C 变量 | Rust 变量 | C 类型 | Rust 类型 | 说明 |
|------|------|------|------|------|
| `total_pages` | `TOTAL_PAGES` | `EXTERN int` | `AssumeSyncCell<usize>` | VM 管理的总物理页数 |
| `num_vm_instances` | `VM_INSTANCE_COUNT` | `EXTERN int` | `AssumeSyncCell<u32>` | 当前 VM 进程实例数 |
| `kernel_boot_info.boot_procs[]` | `BOOT_INFO` | `struct boot_image[]` | `AssumeSyncCell<[BootImage; NR_BOOT_PROCS]>` | 启动镜像数组 |

> **架构演进说明**：Minix3 C 源码中 `num_vm_instances` 是普通 `int`（单线程无需原子），Rust 版本使用 `AssumeSyncCell<u32>` 而非 `AtomicU32`，因为 VM 是单线程事件循环模型，不需要原子操作。

**BootImage 类型**：

Minix3 C 源码定义（[type.h:148](minix3/minix/include/minix/type.h#L148)）：
```c
struct boot_image {
  int proc_nr;                     /* process number to use */
  char proc_name[PROC_NAME_LEN];   /* name in process table */
  endpoint_t endpoint;             /* endpoint number when started */
  phys_bytes start_addr;           /* Where it's in memory */
  phys_bytes len;
};
```

Rust 实现（定义在 `minix-types` crate，`types/boot.rs`）：
```rust
#[derive(Debug, Clone, Copy)]
pub struct BootImage {
    pub proc_nr: i32,
    pub proc_name: [u8; PROC_NAME_LEN],
    pub endpoint: Endpoint,
    pub start_addr: u64,
    pub len: u64,
}
```

> **架构演进说明**：C 源码中 `start_addr` 和 `len` 类型为 `phys_bytes`（32 位下为 `u32`），Rust 版本使用 `u64` 以支持 64 位物理地址空间。

`BootImage` 是跨服务共享类型，记录系统启动时加载的进程信息。VM 通过 `VmProc.vm_boot: Option<BootImage>` 引用它。

> **TODO**: `BootImage` 的完整文档应归属于 `minix-types` crate 的文档，当前暂放此处。

**VM 实例数说明**（C 源码：[glo.h:46](minix3/minix/servers/vm/glo.h#L46) `num_vm_instances`，[rs.c:230](minix3/minix/servers/vm/rs.c#L230) 限制检查）：
- 标准启动时为 1（VM 服务自身，[main.c:578](minix3/minix/servers/vm/main.c#L578)）
- RS（复活服务器）可创建新 VM 实例进行无缝重启
- 最多支持 2 个实例：1 个旧实例（可能故障）+ 1 个新实例（正在启动）
- 超过 2 个会因 VM 内部实现限制（页表、内存映射冲突）而返回 `EPERM`（[rs.c:231-233](minix3/minix/servers/vm/rs.c#L231-L233)）

---

## 4. 设计原则

### 4.1 地址稳定性

> **原则**: `vmproc` 对象一旦创建，其内存地址必须保持不变。

**原因**:
- 页表、区域等结构可能持有指向 `vmproc` 的指针
- 内核的 `pagedir_mappings[]` 数组存储进程到页表的映射
- IPC 消息处理中通过 slot 快速定位进程

**如果地址变化**:
- 页表绑定失效，MMU 无法正确转换地址
- 区域迭代器中的进程指针成为悬空指针
- 其他服务持有的进程引用失效

**实现保障**:
- 使用静态数组 `[MaybeUninit<VmProc>; NR_PROCS]`
- 禁止 move 操作（没有 `Pin`，直接禁止 move 语义）
- 删除操作只标记为未使用，不收缩数组

### 4.2 Fail-Stop 语义

> **原则**: VM 是核心服务，一旦崩溃系统必须重启，设计需保证崩溃时"干净地死掉"。

**背景**:
- VM 被标记为 `SF_CORE_SRV`（核心服务）
- VM 崩溃时 RS（重启动服务）会直接退出，系统必须重启
- 参考: [manager.c:1121-1123](minix3/minix/servers/rs/manager.c#L1121-L1123)

**为什么需要"干净地死掉"**:
- 避免崩溃过程中污染系统状态
- 防止错误的 Drop 操作释放不存在的资源
- 保证调试信息可靠

**实现策略**:
- 使用 `MaybeUninit` 避免隐式 Drop
- panic 时直接终止，不执行栈展开
- 参考: [panic.c:21-67](minix3/minix/lib/libsys/panic.c#L21-L67) 的用户态 `panic()` 实现

### 4.3 无堆分配

> **原则**: VM 是系统的内存分配器，不能使用 malloc，`vmproc` 必须使用静态分配或 Slab 分配器。

**循环依赖问题**:
```
VM 需要分配内存 → 调用 malloc → malloc 需要内存 → 调用 VM
```

**解决方案**:
- `vmproc` 使用静态数组：`[MaybeUninit<VmProc>; NR_PROCS]`
- 其他 VM 内部结构使用 Slab 分配器（VM 自己实现的）
- 物理内存分配通过 `alloc_mem()` 接口，不依赖外部分配器

**Slab 分配器**:
- VM 专用的内存分配器
- 仅用于 VM 内部，不对外提供服务
- 详见: [08-slab-allocator.md](08-slab-allocator.md)

### 4.4 引用计数管理
// TODO: 物理块的引用计数约定

### 4.5 可见性原则：VM crate 对外不暴露内部类型

> **原则**: VM 是独立用户空间进程，没有外部 crate 消费者。crate 内部最大可见性为 `pub(crate)`。

**架构依据**：
- VM 与 PM、VFS、RS 等服务进程通过 **IPC 消息**交互，不通过共享库
- IPC 消息类型（`VmRequest`/`VmResponse`）定义在 `minix-types` crate 中，外部进程使用 `minix-types` 构造消息，而非链接 VM crate
- VM crate 的所有类型均为内部实现细节，外部无需也不应访问

**当前可见性策略**：

| 层级 | 可见性 | 说明 |
|------|--------|------|
| 子模块声明 | `pub(crate) mod` | 所有子模块（`vmproc`、`phys_mem`、`region` 等） |
| 类型重导出 | `pub(crate) use` | 所有 `use` 重导出 |
| 内部类型 | `pub(crate) struct/enum/fn` | 所有 struct、enum、方法 |
| 模块内部 | `pub(super)` / 私有 | 模块树内部更严格的可见性 |

**例外**：无。当前 VM crate 没有任何需要 `pub` 导出的类型。

> **TODO**: 当更多模块就绪后，再次 review 整个 VM crate 的可见性。需确认：
> - `minix-types` 中的 `VmRequest`/`VmResponse` 是否已覆盖所有 IPC 交互需求
> - 是否有类型（如错误码、常量）需要提升到 `minix-types` 供外部使用
> - Kernel 是否需要直接使用 VM 的某些类型（当前通过 IPC 间接访问）

---

## 5. 跨组件约定

### 5.1 与 PM 的交互

#### 5.1.1 进程生命周期
// TODO: PM 管理逻辑进程，VM 管理内存

#### 5.1.2 Fork 协作
// TODO: PM 分配 slot，VM 复制内存

#### 5.1.3 Exit 协作
// TODO: PM 通知，VM 清理内存

### 5.1.4 TOCTOU 与分布式一致性

> **问题**: PM 和 VM 是两个独立的地址空间，如何处理进程生命周期的竞争条件？

**场景分析**:

```
时间线:
  T0: PM 决定为进程 A fork 子进程
  T1: PM 发送 VM_FORK 消息给 VM（携带父进程 endpoint）
  T2: 进程 A 在消息到达前崩溃退出
  T3: PM 回收 slot，分配给新进程 B
  T4: VM 收到 VM_FORK 消息，endpoint 已失效
```

**如果没有验证机制**:
- VM 使用旧的 endpoint 查找进程
- 可能错误地操作进程 B 的内存（slot 已被重用）
- 导致严重的安全问题

**解决方案 - Endpoint 验证**:

VM 使用 `vm_isokendpt()` 验证 endpoint 有效性（[utility.c:84](minix3/minix/servers/vm/utility.c#L84)）：

```c
int vm_isokendpt(endpoint_t endpoint, int *procn)
{
    *procn = _ENDPOINT_P(endpoint);
    if(*procn < 0 || *procn >= NR_PROCS)
        return EINVAL;      // slot 越界
    if(*procn >= 0 && endpoint != vmproc[*procn].vm_endpoint)
        return EDEADEPT;    // 端点已失效（slot 被重用，generation 不匹配）
    if(*procn >= 0 && !(vmproc[*procn].vm_flags & VMF_INUSE))
        return EDEADEPT;    // 进程已退出
    return OK;
}
```

**设计本质**:

- **分布式一致性**: PM 和 VM 像分布式系统中的节点，需要处理状态不一致
- **Generation 机制**: endpoint 包含 generation，slot 重用后 generation 递增，旧 endpoint 自然失效
- **防御性编程**: VM 不信任 PM 提供的 endpoint，必须验证

> 这种验证机制防的不是恶意攻击，而是**时间差（TOCTOU: Time-of-Check to Time-of-Use）**问题。

### 5.2 与 VFS 的交互

#### 5.2.1 文件映射
// TODO: mmap 文件时的协作

#### 5.2.2 页缓存
// TODO: 与文件系统缓存的关系

### 5.3 与 Kernel 的交互

#### 5.3.1 页错误处理
// TODO: 内核捕获页错误，转发给 VM

#### 5.3.2 系统调用转发
// TODO: 内核将内存相关系统调用转发给 VM

---

## 6. 命名约定

### 6.1 类型命名
// TODO: VmProc vs vmproc，Rust 与 C 的对应

### 6.2 函数命名
// TODO: do_xxx 表示 IPC 处理函数

### 6.3 常量命名
// TODO: VM_ 前缀的常量

---

## 7. 错误处理策略

### 7.1 错误码约定
// TODO: 使用 Minix3 标准错误码

### 7.2 panic 策略

VM 作为 Minix3 的核心服务，采用 **Fail-Stop** 语义：

```
Fail-Stop = 检测到错误 → 立即停止 → 不执行任何副作用
```

**核心原则**: 宁可让系统停止，也不要让系统处于不确定状态。

**Minix3 用户态 panic 实现**（[panic.c:21-67](minix3/minix/lib/libsys/panic.c#L21-L67)）：

```c
void panic(const char *fmt, ...)
{
    endpoint_t me = NONE;
    char name[20];
    /* ... sys_whoami 获取调用者信息 ... */
    if(sys_whoami(&me, name, sizeof(name), &priv_flags, &init_flags) == OK && me != NONE)
        printf("%s(%d): panic: ", name, me);
    else
        printf("(sys_whoami failed): panic: ");
    if(fmt) { va_start(args, fmt); vprintf(fmt, args); va_end(args); }
    else { printf("no message\n"); }
    printf("\n");
    util_stacktrace();
    panic_hook();
    _exit(1);       /* 直接退出！ */
    abort();        /* 备用方案 */
    suicide = (void (*)(void)) -1; suicide();  /* 更激进的自杀 */
    for(;;) { }     /* 最后手段：死循环 */
}
```

**关键特点**：
- **没有清理操作**！直接 `_exit(1)`
- **没有资源释放**！不调用任何 cleanup 函数
- **立即终止**，不执行任何后续代码

**Rust panic 的风险**：

| 特性 | Minix3 panic | Rust panic |
|------|-------------|------------|
| **触发后行为** | 打印信息 → `_exit(1)` | 栈展开 → 调用 `Drop` → 终止 |
| **资源清理** | ❌ 不清理 | ✅ 自动 Drop |
| **副作用风险** | ✅ 无（直接退出） | ⚠️ Drop 可能出错 |

**问题**：如果对象处于不一致状态，Rust 的 `Drop` 可能释放错误的资源、写入损坏的数据、导致更严重的系统状态污染。

**Minix3 的设计哲学**：一旦检测到内部不一致，系统状态可能已经损坏，**执行任何清理操作都可能加剧损坏**，所以直接 `_exit(1)`。

**VM 是核心服务（SF_CORE_SRV）**：

```c
// [manager.c:1121-1123](minix3/minix/servers/rs/manager.c#L1121-L1123)
if ((rp->r_pub->sys_flags & SF_CORE_SRV) && !shutting_down) {
    printf("core system service died: %s\n", srv_to_string(rp));
    _exit(1);  // RS 直接退出，系统崩溃
}
```

只有非核心服务才能被 RS 重启。VM 崩溃 = 系统崩溃。

**Rust 实现策略**：

```rust
pub enum VmError {
    OutOfMemory,        // 致命：VM 状态已损坏
    PageTableError,
    ProcessNotFound,    // 非致命：可以继续运行
    PermissionDenied,   // 非致命
}

impl VmError {
    pub fn is_fatal(&self) -> bool {
        matches!(self, VmError::OutOfMemory)
    }

    pub fn handle(&self) {
        log::error!("VM error: {:?}", self);
        if self.is_fatal() {
            unsafe { libc::_exit(1) };  // 干净地死掉
        }
    }
}
```

**panic 时避免污染系统状态**：

```rust
pub fn safe_operation<F, T>(op: F) -> Result<T, VmError>
where F: FnOnce() -> Result<T, VmError>,
{
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(op))
        .map_err(|_| VmError::InternalError)?
}

pub unsafe fn force_cleanup() {
    libc::_exit(1);  // 直接调用 _exit，不执行任何 Drop
}
```

> **详见**: [fail-stop.md](../../concepts/fail-stop.md) 的完整 Fail-Stop 语义与 Panic 安全分析。

### 7.3 资源清理

**MaybeUninit 的 panic 安全价值**：

`MaybeUninit` 防止未初始化内存的非法 Drop：

```rust
// 普通数组会 panic
let arr: [VmProc; 256] = unsafe { uninitialized() };  // 未定义行为！

// MaybeUninit 明确告诉编译器：这是未初始化的
let arr: [MaybeUninit<VmProc>; 256] = unsafe { uninitialized() };
// panic 时不会尝试 Drop 不存在的对象
```

| 风险 | 没有 MaybeUninit | 有 MaybeUninit |
|------|------------------|---------------|
| panic 时 Drop | 释放不存在的物理页 | 安全：未初始化内存不 Drop |
| 破坏页表 | 可能 | 避免 |
| 发送错误 IPC | 可能 | 避免 |
| 调试信息 | 不可靠 | 可靠 |

---

## 8. 性能考虑

### 8.1 缓存友好性
// TODO: 数据结构布局优化

### 8.2 锁粒度
// TODO: 并发访问控制

### 8.3 快速路径
// TODO: 常见操作的优化

---

## 9. 参见

### 9.1 文档阶段分类

> 根据 §2.4 的初始化时序，将文档按堆可用性分类。这对编程和文档阅读有重大意义。

#### 阶段 1-2：堆不可用（Heap-Free）

这些文档对应的代码在 `pt_init()` 之前执行，**不能使用堆**：

| 文档 | 初始化时机 | 堆依赖 | Rust 实现约束 |
|------|-----------|--------|---------------|
| [01-vmproc-struct.md](01-vmproc-struct.md) | `memset(vmproc, 0)` | ❌ 无 | 静态数组 `[VmProc; NR_PROCS]` |
| [02-vmproc-table.md](02-vmproc-table.md) | `memset(vmproc, 0)` | ❌ 无 | 静态数组，`AssumeSyncCell` |
| [03-acl.md](03-acl.md) | `acl_init()` | ❌ 无 | 静态数据，bitflags |
| [04-physical-memory.md](04-physical-memory.md) | `mem_init()` | ❌ **禁止** | bitmap 必须静态分配！ |

**Rust 代码检查点**：
- ❌ 不能使用 `Vec`, `Box`, `String`, `HashMap` 等
- ✅ 只能使用静态数组、栈变量、`MaybeUninit`

#### 阶段 3：堆开始可用（Heap-Bootstrapping）

这些文档对应的代码在 `pt_init()` 期间执行，**可以使用保留页池**：

| 文档 | 初始化时机 | 堆依赖 | Rust 实现约束 |
|------|-----------|--------|---------------|
| [05-vm-allocpage.md](05-vm-allocpage.md) | `pt_init()` | ❌ 无 | 保留页池与自举机制 |
| [06-pagetable-struct.md](06-pagetable-struct.md) | `pt_init()` | ⚠️ 保留页 | 使用 `vm_allocpage()` |
| [07-pagetable-ops.md](07-pagetable-ops.md) | `pt_init()` | ⚠️ 保留页 | 页表操作 |

**Rust 代码检查点**：
- ⚠️ 可以使用 `alloc_mem()` 获取物理页
- ⚠️ 可以使用保留页池（`vm_getsparepage()`）
- ❌ 仍不能直接使用 `Vec`, `Box`（`_brk` 刚就绪）

#### 阶段 4：堆完全可用（Heap-Available）

这些文档对应的代码在 `init_vm()` 返回后执行，**可以自由使用堆**：

| 文档 | 运行时机 | 堆依赖 | Rust 实现约束 |
|------|---------|--------|---------------|
| [12-vir-region.md](12-vir-region.md) | fork/mmap | ✅ 可用 | 可以使用 `Vec` |
| [08-slab-allocator.md](08-slab-allocator.md) | 运行时 | ✅ 可用 | 使用 `malloc` |
| [14-phys-region.md](14-phys-region.md) | fork/mmap | ✅ 可用 | 可以使用 `Vec` |
| [11-memtype.md](11-memtype.md) | 运行时 | ✅ 可用 | 可以使用堆 |
| [13-region-avl.md](13-region-avl.md) | fork/mmap | ✅ 可用 | 可以使用 `Box` |
| [10-phys-block.md](10-phys-block.md) | fork/mmap | ✅ 可用 | 可以使用堆 |
| [15-cow-mechanism.md](15-cow-mechanism.md) | 页错误 | ✅ 可用 | 可以使用堆 |
| [16-pagefault.md](16-pagefault.md) | 页错误 | ✅ 可用 | 可以使用堆 |
| [17-vm-fork.md](17-vm-fork.md) | IPC | ✅ 可用 | 可以使用堆 |
| [18-vm-brk.md](18-vm-brk.md) | IPC | ✅ 可用 | 可以使用堆 |
| [19-vm-map.md](19-vm-map.md) | IPC | ✅ 可用 | 可以使用堆 |

**Rust 代码检查点**：
- ✅ 可以自由使用 `Vec`, `Box`, `String`, `HashMap` 等
- ✅ 可以使用 `malloc`/`free`（通过 `#[global_allocator]`）

### 9.2 VM 私有组件
- [01-vmproc-struct.md](01-vmproc-struct.md) - 进程结构体
- [02-vmproc-table.md](02-vmproc-table.md) - 进程表管理
- [03-acl.md](03-acl.md) - 访问控制
- [08-slab-allocator.md](08-slab-allocator.md) - Slab 分配器
- [12-vir-region.md](12-vir-region.md) - 虚拟区域
- [14-phys-region.md](14-phys-region.md) - 物理区域
- [13-region-avl.md](13-region-avl.md) - AVL 树
- [10-phys-block.md](10-phys-block.md) - 物理块
- [15-cow-mechanism.md](15-cow-mechanism.md) - 写时复制
- [16-pagefault.md](16-pagefault.md) - 页错误处理

### 9.3 VM 库组件
- [04-physical-memory.md](04-physical-memory.md) - 物理内存分配
- [06-pagetable-struct.md](06-pagetable-struct.md) - 页表结构
- [07-pagetable-ops.md](07-pagetable-ops.md) - 页表操作
- [11-memtype.md](11-memtype.md) - 内存类型系统

### 9.4 VM 服务组件
- [17-vm-fork.md](17-vm-fork.md) - VM_FORK 服务
- [18-vm-brk.md](18-vm-brk.md) - VM_BRK 服务
- [19-vm-map.md](19-vm-map.md) - VM_MAP 服务

### 9.5 全局概念
- [系统核心概念](../../concepts/README.md) - 全局概念文档（Endpoint、IPC 等）
- [Endpoint 协议](../../concepts/endpoint.md) - 进程标识协议详解

---

*分类: VM整体层级 | 本文档汇总 VM 模块的全局概念和约定*
