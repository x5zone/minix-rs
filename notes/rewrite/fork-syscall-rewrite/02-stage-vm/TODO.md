# 02-stage-vm TODO 汇总

> 收集 01~26 文档和 Rust 代码中的所有 TODO，按优先级和模块归类。
> 形式：`[文档]：简述`。原文档中的 TODO 不作改动。

---

## P0：必须立即修复

### ✅ 已修复：共享内存缺页处理（SharedMemory::ev_pagefault）

**问题**：`SharedMemory::ev_pagefault` 返回 `NeedNewPage`（分配全新物理页），违反共享语义。共享内存缺页时应该链接到**源进程的同一物理页**，而非分配新页。

**C 源码行为**（mem_shared.c:122 `shared_pagefault`）：
1. `getsrc()` → 通过 `VrParam::Shared { ep, vaddr, id }` 找到源进程和源区域
2. 若目标页未映射 → `map_pf()` 确保源进程有该页 → `pb_link()` 链接到同一 `phys_block`
3. 若目标页已映射 → 直接返回 OK（跨进程共享同一物理页）

**修复**：
- `SharedMemory::ev_pagefault` 检查 slot 是否已映射。已映射 → `Handled`；未映射 → `Err(NotSupported)`（fail-closed）
- `dispatch_pagefault` 中的 `find_region_by_addr` → `regions().find()`
- 调用链返回 `AccessViolation` 替代无声的错误行为

**为什么这么修**：
- `ev_pagefault` 签名不含 `VmProcTable`，无法在 trait 方法内查源进程
- `dispatch_pagefault` 有 `VmProcTable`，但当前无跨进程 PFN 共享机制
- fail-closed（返回错误）比 fail-open（分配错误的新页）安全
- 真实实现需：提取 `VrParam::Shared` → 查源进程/区域 → 获取源 PFN → 映射到当前 slot

**影响**：共享内存在缺页时返回 AccessViolation，不再无声地分配独立私有的新页。

**涉及的文件**：
- `os/servers/vm/src/memtype.rs` — SharedMemory::ev_pagefault 改为 fail-closed
- `os/servers/vm/src/vm_server.rs` — dispatch_pagefault 修正 region lookup

---

## P1：核心功能缺失（文档已覆盖、Rust 未实现）

### IPC 消息处理（24-vm-ipc-dispatch.md）

| TODO | 说明 | 状态 |
|------|------|------|
| `VM_MAPCACHEPAGE` | 缓存页映射 IPC handler | 已 stub（返回 NotImplemented） |
| `VM_SETCACHEPAGE` | 注册匿名内存页为缓存块 | 已 stub |
| `VM_FORGETCACHEPAGE` | 使设备偏移范围的缓存页失效 | 已 stub（dispatcher 中已实现 dispatch_forgetcache）|
| `VM_CLEARCACHE` | 清除设备的所有缓存页 | 已 stub（dispatcher 中已实现 dispatch_clearcache）|
| `VM_REMAP` / `VM_REMAP_RO` | 共享内存映射（do_remap） | 需 remap 模块 |
| `VM_PROCCTL` | 进程控制（VFS transid） | 需 VFS 事务机制 |
| `VM_ADDDMA` / `VM_DELDMA` / `VM_GETDMA` | DMA 区域管理 | 需 DMA 模块 |

> **注意**：`dispatch_forgetcache` 和 `dispatch_clearcache` 在 dispatcher.rs 中已有实现。CALLMAP 中缺失的是 RS 相关 handler 的 decode helper。

### 页表操作（07-pagetable-ops.md）

| TODO | 说明 | 状态 |
|------|------|------|
| MockPaging → 真架构实现 | 当前仅 `MockPaging`（单元测试），缺少 `X86_64Paging` 硬件实现 | 需 arch crate 就绪 |
| `map_kernel()` | 每个用户进程页表中映射内核地址空间 | 需 arch crate 就绪 |
| Direct Map 扩展 | 物理内存 > 1GB 时需要扩展 Direct Map 区域 | 需 arch crate 就绪 |

### MemType 回调（12-memtype.md）

| TODO | 说明 | 状态 |
|------|------|------|
| `ContiguousAnonymous` 完整实现 | 连续物理页分配的 ev_pagefault/stub | 已 stub（返回 NeedNewPage），正确 |
| `CacheMemory::ev_pagefault` | 缓存索引查找和 pb_link | 已 stub（返回 NeedNewPage），正确 |
| `SharedMemory::ev_pagefault` 跨进程链接 | 源进程物理页 PFN 共享 | ⛔ 需要 `VmProcTable` 传递进 trait 签名，设计变更 |
| `MappedFile` 参数复制 | VrParam::File 补全后的 ev_copy/ev_split | 已 stub（ev_copy 复制 param），正确 |

> **SharedMemory 说明**：完整实现需要改变 `MemType::ev_pagefault` trait 签名以携带 `VmProcTable`，或用 `PagefaultResult::NeedSharedPage` 变体让调用方处理。当前 fail-closed（返回错误）。

### 主循环与初始化（26-vm-init-main.md）

| TODO | 说明 | 状态 |
|------|------|------|
| `CriticalPool` refill | 保留页池补充（`alloc_cycle()` 等价） | 需 CriticalPool 集成到 VmPageAllocator |
| `do_memory()` | 内核 SIGKMEM 信号处理 | 需内核 IPC |
| `pt_clearmapcache()` | 清除页表映射缓存 | 需添加 PageTable 方法 |
| `exec_bootproc()` Rust 实现 | ELF 加载 + 初始堆栈设置 | 需 libexec 对应 |
| `IpcTransport` trait | IPC 基础设施抽象（`ipc_receive`/`ipc_send`） | 需独立 ipc 模块 |

### 进程管理

| TODO | 来源 | 说明 | 状态 |
|------|------|------|------|
| `handle_vm_willexit` | [20-vm-exit.md](20-vm-exit.md) L801 | 已 stub（dispatcher 中 dispatch_willexit 已实现）| ✅ 已就绪 |
| `swap_proc_slot` typestate | [21-vm-rs-services.md](21-vm-rs-services.md) L479 | 需要 typestate 扩展以支持 slot 交换 | 需设计 |
| `handle_memory_once` | [16-vm-fork.md](16-vm-fork.md) L1091 | 通知内核 fork 消息页面的内存映射 | 需内核 IPC |
| `bind_page_table` kernel IPC | [16-vm-fork.md](16-vm-fork.md) L1209 | sys_vmctl IPC | ✅ 已在 vmproc_handle.rs:285 实现 |
| `SysForkIn`/`SysForkOut` 类型 | [16-vm-fork.md](16-vm-fork.md) L882 | VM→Kernel IPC 链路类型定义 | 需内核 side |
| `sys_fork` non-test | [fork.rs](fork.rs) L204 | `todo!("sys_fork: send SYS_FORK message to kernel")` | 需内核 IPC |

### 查询服务

| TODO | 来源 | 说明 | 状态 |
|------|------|------|------|
| `getrusage` children 路径 | [22-vm-queries.md](22-vm-queries.md) L275 | 需要 VmProc 增加 `vm_parent` 字段追踪父子关系 | 需设计（C 源码自身标注 XXX TODO）|

### 物理内存

| TODO | 来源 | 说明 | 状态 |
|------|------|------|------|
| `cache_freepages()` 重试 | [04-physical-memory.md](04-physical-memory.md) L1501 | bitmap_alloc.rs 中已 stub（返回 0）。真实实现需要 `PageCache` 引用传入 `PhysAlloc` | 已 stub |

### ACL

| TODO | 来源 | 说明 | 状态 |
|------|------|------|------|
| 日志输出 | [03-acl.md](03-acl.md) L563 | `log::warn!` 不可用（no_std，`log` crate 未依赖）| 需添加 log crate 依赖或轻量日志设施 |
| NO_ACL 处理 | [03-acl.md](03-acl.md) L593 | `Uninitialized` 允许调用（与 Minix3 行为一致）| ✅ 语义正确 |

---

## P2：优化与重构

| 来源 | TODO | 说明 |
|------|------|------|
| [01-vmproc-struct.md](01-vmproc-struct.md) L609+L613 | `MaybeUninit+bool` → `InPlaceOption<T>` 自定义类型评估 | 替换非安全的初始化模式 |
| [01-vmproc-struct.md](01-vmproc-struct.md) L863 | 实现 VM 专用的 `panic!` 函数 | 支持栈展开以获取 Drop 位置 |
| [01-vmproc-struct.md](01-vmproc-struct.md) L1271 | 再次 review vmproc 模块可见性 | `VmFlags`/`VM_PROC_COUNT` 可能应降级 |
| [02-vmproc-table.md](02-vmproc-table.md) L908 | 再次 review vmproc 模块可见性 | 同上 |
| [11-region-mapping.md](11-region-mapping.md) L1598 | enum dispatch 替代 `dyn MemType` | 虚函数调用开销优化（缺页频发时） |

---

## 附录：已关闭的 TODO

以下 TODOs 在 cc-review-scan 修复中已关闭：

| 原 TODO | 关闭原因 |
|---------|---------|
| 00-vm-overview.md emoji ✅❌🚧 | 已替换为中性文字 |
| 13-region-avl.md 引用 12-vir-region.md | 已修正为 11-region-mapping.md |
| 15-pagefault.md 引用 19-cow-exec-pagefault.md | 已修正为 23-vfs-interaction.md |
| 18-vm-mmap.md 引用 25-cache-memtypes.md | 已修正为 25-page-cache.md |
| exit.rs `unsafe { exiting.reap() }` 缺 SAFETY | 已补充 |
| memtype/alloc_page/page_state `as u32` 缺注释 | 已补充截断安全性注释 |
| 00-vm-overview.md CR3/TLB 缺架构说明 | 已补充 Paging trait 说明 |
| 07-pagetable-ops.md 缺 AVL tree 引用 | 已补充 13-region-avl.md 链接 |
| 10-phys-pagestate.md 引用 25-cache-memtypes | 已修正 |
| vm_server.rs 7 处 `pub` → `pub(crate)` | 已降级 |
| minix3_missed.md | 已删除，内容归档到 00-vm-overview.md §8 |
| cc-review-scan.md | 已删除，工作文档使命完成 |
