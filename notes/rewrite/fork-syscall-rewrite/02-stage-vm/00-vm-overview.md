# 00-vm-overview: VM 整体架构概览

> **分类**: VM整体层级
> **源码**: `minix3/minix/servers/vm/`（24 个 .c 文件，198 个 C 函数）
> **说明**: VM 是什么、怎么工作、和谁协作——一份面向新读者的入口文档

---

## 1. VM 是什么

### 1.1 先从一个类比开始

VM（Virtual Memory server）是一个**用户态服务进程**。如果你写过 Web Server，VM 的结构会让你感到熟悉：

```
Web Server                          VM Server
─────────                          ────────
监听 HTTP 请求                    监听 IPC 消息（VM_FORK, VM_BRK, VM_MMAP...）
路由到 handler                    按消息类型分派到 do_xxx()
操作数据库                        操作页表、物理内存、虚拟区域
返回 JSON 响应                    返回 errno + 消息体
单线程事件循环                    单线程事件循环
```

和 Web Server 一样，VM 的核心业务逻辑就是**处理数据、响应请求**。只不过 VM 处理的是**物理页、页表、虚拟地址区域**而不是 JSON 和数据库。

VM 不直接操作硬件。它看不到真正的物理内存——kernel 通过 IPC 传给它一组 `{ base, size }` 的描述数据，VM 在这些数据之上做决策：谁该得到多少内存、什么时候回收、页面之间怎么共享。

### 1.2 VM 在 Minix3 微内核中的位置

Minix3 是一个微内核操作系统。传统单体内核中的内存管理子系统被拆成了**两个独立进程**：

```
┌──────────────────────────────────────────────────────┐
│                  用户进程层                            │
│           (应用程序、Shell 等)                         │
├──────────────────────────────────────────────────────┤
│                  系统服务层（用户态）                   │
│   ┌──────┐ ┌──────┐ ┌──────┐ ┌──────┐               │
│   │  PM  │ │  VM  │ │ VFS  │ │  RS  │ ...           │
│   └──┬───┘ └──┬───┘ └──┬───┘ └──┬───┘               │
├──────┼────────┼────────┼────────┼───────────────────┤
│      └────────┴────────┴────────┘                    │
│                   微内核层（内核态）                    │
│     (进程调度、中断处理、页错误捕获、IPC 机制)          │
└──────────────────────────────────────────────────────┘
```

VM 的核心职责：
- 管理每个进程的虚拟地址空间
- 分配和回收物理内存
- 处理页错误（Page Fault）
- 实现写时复制（Copy-on-Write）
- 提供内存映射（mmap）服务

### 1.3 策略 vs 机制：VM 的本质定位

> **核心原则**: Kernel 提供**机制**，VM 提供**策略**。

VM 是一个策略引擎。Kernel 告诉它"世界是什么样的"，它决定"怎么分配"，然后请求 Kernel 执行实际的硬件操作。

```
策略层（VM Server — 用户态）
├── 接收 kernel 的物理内存描述数据
├── 决定分配策略（给谁、给多少、何时回收）
├── 决定地址空间布局（虚拟区域、CoW、页错误处理）
└── 请求 kernel 执行特权操作

机制层（Kernel — 内核态）
├── 解析 UEFI/BIOS 内存 map，发现物理内存
├── 建立初始页表（恒等映射 + 内核映射）
├── 执行页表修改、TLB 刷新
└── 通过 IPC 将内存信息传递给 VM
```

**VM 能做什么**:
- 管理物理页的分配策略（位图 / free list / buddy）
- 管理进程的虚拟地址空间布局
- 决定 CoW / 按需分配等策略
- 通过 IPC 请求 kernel 执行特权操作

**VM 不能做什么**:
- 直接操作页表硬件 — CR3/TLB 等是 x86-64 特定概念，Rust 的 `Paging` trait 抽象了这些硬件细节。上层代码通过 `trait` 方法（`map()`, `unmap()`, `query()`）操作页表，不感知底层寄存器编码。各架构在独立的 `arch` crate 中实现 trait
- 直接访问物理内存地址 — 通过 `vm_phys_to_virt()` / Direct Map 间接访问
- 探测物理内存 — 这是 UEFI/e820 的职责
- 处理中断/异常 — 由 kernel 转发

### 1.4 执行模型：单线程事件循环

VM 是一个**单线程**进程。主循环（`run()`）独占 `&mut self`，每次处理一条 IPC 消息。这意味着：

- `Rc` 替代 `Arc`（无跨线程共享）
- `RefCell` 替代 `Mutex`（无并发访问）
- `!Send` / `!Sync` 是合理的（数据不跨线程）
- `AssumeSyncCell` 在单线程下安全

这条假设贯穿整个 VM 代码库。如果未来需要多线程，需要重构状态管理。

### 1.5 协作关系

| 服务 | 协作方式 | 典型场景 |
|------|---------|---------|
| **PM** (Process Manager) | PM 分配 slot，VM 复制/清理内存 | fork、exit |
| **VFS** (Virtual File System) | VFS 提供文件信息，VM 建立内存映射 | mmap 文件、缺页 I/O |
| **RS** (Reincarnation Server) | RS 管理服务生命周期，VM 注册 ACL | 启动握手、Live Update（暂不实现） |
| **Kernel** | 捕获页错误→转发 VM；VM 请求 kernel 执行特权操作 | pagefault、sys_vmctl |

---

## 2. 文档导航：01~26 的叙事逻辑

VM 涉及的概念极多——物理内存、页表、虚拟区域、CoW、页错误、mmap...如果平铺直叙地逐个介绍，读者容易迷失在概念海洋中。

因此，01~19 采用**单一主线叙事**：跟随 `fork` 系统调用的执行路径，按需引入每个概念。fork 是最复杂的内存操作之一，它几乎触及 VM 的所有核心组件。

### 2.1 01~19：fork 主线

```
fork 请求到达
  │
  ├── 需要进程表 → 01 vmproc 结构 / 02 vmproc 表
  ├── 需要权限检查 → 03 ACL
  ├── 需要分配物理页 → 04 物理内存 / 05 页分配
  ├── 需要页表 → 06 页表结构 / 07 页表操作
  ├── 需要内核堆 → 08 slab 分配器 / 09 VM 重定位
  ├── 需要物理块/区域 → 10 PhysBlock / 11 MemType / 12 VirRegion / 13 AVL
  ├── 需要写时复制 → 14 CoW 机制 / 15 页错误处理
  ├── 执行 fork → 16 VM_FORK
  ├── 需要堆扩展 → 17 VM_BRK
  └── 需要内存映射 → 18 VM_MMAP / 19 VM_MUNMAP
```

| 编号 | 文档 | 主线角色 |
|------|------|---------|
| 01 | [vmproc-struct](01-vmproc-struct.md) | fork 的目标：进程在 VM 中的表示 |
| 02 | [vmproc-table](02-vmproc-table.md) | fork 需要查找/分配进程槽 |
| 03 | [acl](03-acl.md) | fork 需要权限检查 |
| 04 | [physical-memory](04-physical-memory.md) | fork 需要分配物理页 |
| 05 | [vm-allocpage](05-vm-allocpage.md) | fork 需要页分配器 |
| 06 | [pagetable-struct](06-pagetable-struct.md) | fork 需要创建子进程页表 |
| 07 | [pagetable-ops](07-pagetable-ops.md) | fork 需要操作页表 |
| 08 | [slab-allocator](08-slab-allocator.md) | VM 内部使用的堆分配器 |
| 09 | [vm-relocation](09-vm-relocation.md) | fork 前需完成元数据搬迁 |
| 10 | [phys-pagestate](10-phys-pagestate.md) | fork 的 CoW 需要物理块引用计数 |
| 11 | [region-mapping](11-region-mapping.md) | fork 需要复制虚拟区域和物理映射 |
| 12 | [memtype](12-memtype.md) | fork 需要知道内存类型语义 |
| 13 | [region-avl](13-region-avl.md) | fork 的区域查找需要 AVL 树 |
| 14 | [cow-mechanism](14-cow-mechanism.md) | fork 设置 CoW 保护 |
| 15 | [pagefault](15-pagefault.md) | CoW 页面写入时触发页错误 |
| 16 | [vm-fork](16-vm-fork.md) | fork 的完整执行流程 |
| 17 | [vm-brk](17-vm-brk.md) | fork 后子进程可能扩展堆 |
| 18 | [vm-mmap](18-vm-mmap.md) | fork 后可能 mmap |
| 19 | [vm-munmap](19-vm-munmap.md) | 取消映射 |

### 2.2 20~26：补全阶段

fork 主线完成后，VM 的主体框架已经建立。剩余内容是在已有骨架上填充：

| 编号 | 文档 | 覆盖内容 |
|------|------|---------|
| 20 | [vm-exit](20-vm-exit.md) | 进程退出：清理页表、释放内存 |
| 21 | [vm-rs-services](21-vm-rs-services.md) | RS 服务：SET_PRIV、PREPARE、UPDATE、MEMCTL |
| 22 | [vm-queries](22-vm-queries.md) | 查询服务：GETPHYS、GETREF、INFO、GETRUSAGE |
| 23 | [vfs-interaction](23-vfs-interaction.md) | VM 与 VFS 的异步对话：文件映射、fd 引用计数 |
| 24 | [vm-ipc-dispatch](24-vm-ipc-dispatch.md) | IPC 消息分发：MessageDispatcher |
| 25 | [page-cache](25-page-cache.md) | 页缓存：缓存块、内存类型变体 |
| 26 | [vm-init-main](26-vm-init-main.md) | 所有零件怎么组装启动：初始化、主循环、SEF |

---

## 3. 初始化：VM 怎么启动

### 3.1 "鸡生蛋"问题

VM 是系统的内存分配器。但它自己也需要内存来运行。这就是 VM 启动的核心矛盾：

```
VM 需要内存 → 但内存管理是 VM 的职责 → VM 怎么管理自己的内存？
```

答案：**分阶段初始化**。先用内核提供的静态内存，再逐步建立自己的内存管理系统。

### 3.2 初始化四阶段

```
阶段 1: 堆不可用
├── sys_getkinfo() → 获取内核启动信息
├── memset(vmproc, 0) → 进程表清零
├── acl_init() → ACL 初始化
├── mem_init() → 物理内存分配器
└── 只能用: 静态变量、栈、BSS

阶段 2: 堆不可用
├── init_proc(VM) → VM 自身进程槽
├── pt_init() → 建立页表 + 保留页池
└── 只能用: 静态变量、栈、BSS

        ─── pt_init() 是分界线 ───

阶段 3: 堆可用
├── __minix_init() → IPC 向量初始化
├── exec_bootproc() → 为启动进程建立地址空间
├── CALLMAP 注册 + SEF 启动
└── 可以用: Box / Vec / GlobalAlloc

阶段 4: 主循环
├── 接收 IPC 消息
├── 分派到 do_xxx() handler
└── 回复 errno
```

### 3.3 各组件的堆依赖

| 组件 | 文档 | 需要堆？ | 说明 |
|------|------|---------|------|
| vmproc 结构体 | 01, 02 | 否 | 编译时静态数组 |
| ACL | 03 | 否 | 静态位图 |
| 物理内存分配器 | 04 | **禁止** | bitmap 必须静态分配 |
| 页表结构 | 06 | 保留页 | 使用 `vm_allocpage()` |
| 页表操作 | 07 | 保留页 | 使用保留页池 |
| Slab 分配器 | 08 | 可用 | 通过 `#[global_allocator]` |
| 虚拟区域 | 11 | 可用 | fork/mmap 时 |
| IPC 处理 | 24, 26 | 可用 | 主循环中 |

---

## 4. 设计原则

### 4.1 地址稳定性

`VmProc` 对象一旦创建，其内存地址必须保持不变。页表、区域迭代器等可能持有指向进程的引用，如果地址变化会导致悬空指针。

**实现**：使用编译时静态数组 `[AssumeSyncCell<VmProc>; VM_PROC_COUNT]`，禁止 move 语义，删除只标记不收缩。

### 4.2 Fail-Stop 语义

VM 是核心服务（`SF_CORE_SRV`）。VM 崩溃 = 系统必须重启。因此一旦检测到内部不一致，VM 直接 `_exit(1)` ——不执行任何清理操作。如果状态已经损坏，执行清理可能加剧损坏。

**Rust 注意**：普通的 `panic!` 会触发栈展开和 `Drop`，这在 VM 的上下文中是危险的——Drop 可能释放错误的资源、写入损坏的数据。

### 4.3 可见性：VM crate 不对外暴露内部类型

VM 是独立用户空间进程，没有外部 crate 消费者。VM 通过 IPC 消息（定义在 `minix-types` crate）与 PM、VFS、RS 通信，不需要共享库。

| 层级 | 可见性 |
|------|--------|
| 子模块声明 | `pub(crate) mod` |
| 类型重导出 | `pub(crate) use` |
| 内部类型/方法 | `pub(crate)` 或 `pub(super)` |
| 对外接口 | `pub`（仅 `VmServer::new()`, `init()`, `run()` 三个入口） |

### 4.4 TOCTOU 防御：Endpoint 验证

PM 和 VM 是独立的地址空间。当 PM 发送 VM_FORK 携带一个 endpoint 时，在消息到达之前那个进程可能已经崩溃退出了。VM 必须用 `vm_isokendpt()` 验证 endpoint 有效性：

- **slot 越界检查**：`ENDPOINT_P(endpoint) < 0 || >= NR_PROCS` → `EINVAL`
- **generation 不匹配**：slot 被重用后 generation 递增，旧 endpoint 自然失效 → `EDEADEPT`
- **进程已退出**：`!(vm_flags & VMF_INUSE)` → `EDEADEPT`

这是分布式系统中的防御性编程——VM 不信任 PM 提供的任何 endpoint。

---

## 5. 错误处理

### 5.1 错误码

VM 使用 Minix3 标准 errno，不自行创造错误码：

| VmError | C errno | 典型场景 |
|---------|---------|---------|
| `InvalidEndpoint` | `ESRCH` | mmap 第三方映射失败、getrusage 进程不存在 |
| `InvalidProcess` | `EINVAL` | fork/brk/exit 的 vm_isokendpt 失败 |
| `OutOfMemory` | `ENOMEM` | 物理页分配不足 |
| `PermissionDenied` | `EPERM` | ACL 拒绝、创建 VM 实例过多 |
| `AccessViolation` | `EACCES` | 写只读页面 |
| `NotImplemented` | `ENOSYS` | 尚未实现的调用号 |

### 5.2 panic 策略

可恢复的错误返回 `Result`，不可恢复的错误直接 panic。panic 时**不执行栈展开**——栈展开会触发 Drop，而 Drop 在已损坏的状态上可能造成二次破坏。

---

## 6. 全局状态

VM 维护以下跨组件共享的全局变量：

| Rust 变量 | C 对应 | 类型 | 说明 |
|------|------|------|------|
| `TOTAL_PAGES` | `total_pages` | `AssumeSyncCell<usize>` | VM 管理的总物理页数 |
| `VM_INSTANCE_COUNT` | `num_vm_instances` | `AssumeSyncCell<u32>` | VM 进程实例数（最多 2） |
| `BOOT_INFO` | `kernel_boot_info.boot_procs[]` | `AssumeSyncCell<[BootImage; N]>` | 启动镜像数组 |
| `VM_PROC_TABLE` | `vmproc[]` (BSS) | `[AssumeSyncCell<VmProc>; N]` | 进程表（编译时静态数组） |

`VmProcTable` 是编译时静态数组，通过 `VmProcTable::get_global()` 全局访问。它不在 `VmServer` 结构体中——全局静态让 `MessageDispatcher::dispatch_xxx()` 可以直接访问进程表，无需从 VmServer 参数链层层传递。

---

## 7. 与其他文档的关系

### 7.1 按堆依赖分类

| 阶段 | 文档 | 堆状态 |
|------|------|--------|
| 阶段 1-2 | 01, 02, 03, 04 | 堆不可用——只能静态分配 |
| 阶段 3 | 05, 06, 07 | 保留页池可用 |
| 阶段 4 | 08~26 | 堆完全可用 |

### 7.2 由 VM 暴露的全局类型

| 类型 | 定义位置 | 使用 |
|------|---------|------|
| `BootImage` | `minix-types` crate | VM 和 PM 共享的启动进程信息 |
| `VmForkIn` / `VmForkOut` 等 IPC 类型 | `minix-types` crate | VM 与 PM/VFS/RS 的 IPC 协议 |
| `VmProcTable` | `vmproc/table.rs` | 全局静态进程表 |
| `VmServer` | `vm_server.rs` | 主循环入口 |

---

## 8. Minix3 未迁移的函数

> 以下 Minix3 C 函数当前既无独立文档覆盖、也无 Rust 实现。

### 8.1 Debug/Sanity — ARCH 不需要

这些函数仅在 `SANITYCHECKS` 编译时生效。Rust 的类型系统、`debug_assert!`、`#[cfg(test)]` 模块可以提供等价或更强的保护。

| 函数 | C 源文件 | 说明 |
|------|---------|------|
| `cache_sanitycheck_internal` | cache.c:87 | 缓存一致性检查 |
| `fdref_sanitycheck` | fdref.c:37 | fd 引用计数一致性检查 |
| `map_printmap` | region.c:98 | 打印整个区域映射表 |
| `map_printregion` | region.c:40 | 打印单个区域 |
| `mem_sanitycheck` | alloc.c:338 | 物理内存分配器一致性检查 |
| `printregionstats` | region.c:1510 | 区域统计打印 |
| `pt_sanitycheck` | pagetable.c:130 | 页表结构一致性检查 |
| `ptestr` | pagetable.c:587 | 页表条目格式化 |
| `slabstats` | slaballoc.c:504 | Slab 分配器统计打印 |
| `usedpages_add_f` | alloc.c:509 | 已用页调试计数器 |
| `usedpages_reset` | alloc.c:501 | 已用页计数器重置 |
| `rmhash_f` (×2) | cache.c:152-153 | hash 表内部辅助函数 |

> **ARCH 理由**：这些函数不产生外部可观察行为。在 C 中作为手动自检工具存在；在 Rust 中，unused 警告、`debug_assert!`、`#[cfg(test)]` 模块和更强的类型安全提供等价保护。

### 8.2 Live Update — 暂不实现

| 函数 | C 源文件 | 说明 |
|------|---------|------|
| `sef_cb_init_vm_multi_lu` | main.c:592 | 多组件 Live Update 回调 |

> **ARCH 理由**：Live Update 需要进程槽交换、状态序列化、IPC 过滤三个子系统协同工作。Rust 版本暂不实现。`rs.rs` 中 `handle_rs_prepare()` / `handle_rs_update()` 已预留 RS 协议入口。详见 [26-vm-init-main.md](26-vm-init-main.md) §9.4。

### 8.3 非 Debug 函数 — 待归属文档

| 函数 | C 源文件 | 行为 | 应归属 |
|------|---------|------|--------|
| `is_stack_region` | region.c:1385 | 判断 vir_region 是否为栈区域。C 源码注释明确指出"we do not actually have this information"，仅用于统计目的 | [22-vm-queries.md](22-vm-queries.md) |
| `get_usage_info_vm` | region.c:1366 | 获取 VM 自身内存使用量，`get_usage_info()` 的内部辅助函数 | [22-vm-queries.md](22-vm-queries.md) |
| `physregions` | region.c:1546 | 遍历 vir_region 所有 phys_region，统计已映射物理页数 | [11-region-mapping.md](11-region-mapping.md) |

---

## 9. 参见

- [01-vmproc-struct.md](01-vmproc-struct.md) — 进程结构体（入口点）
- [26-vm-init-main.md](26-vm-init-main.md) — 所有零件怎么组装启动（终结点）
- [系统核心概念](../../concepts/README.md) — Endpoint、IPC 等全局概念

---

*分类: VM整体层级*
