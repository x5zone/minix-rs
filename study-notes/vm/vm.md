# VM（虚拟内存服务器）

> **学习目标**: 理解用户态虚拟内存管理的实现，地址空间如何通过 IPC 管理。
> 
> **核心问题**: 页错误如何处理？mmap 如何实现？

---
## 当前进度
main.c 待完成。

## 模块总结

### VM 服务核心职责

VM（Virtual Memory）是 Minix3 微内核架构中的核心用户态服务，负责：

| 职责 | 说明 | 关键文件 |
|------|------|----------|
| **地址空间管理** | 进程虚拟地址空间的创建、销毁、切换 | vmproc.h, region.c |
| **页错误处理** | 按需分页、写时复制 | pagefaults.c |
| **内存映射** | mmap/munmap 实现 | mmap.c, mem_*.c |
| **共享内存** | 进程间内存共享 | mem_shared.c |
| **堆管理** | sbrk/brk 实现 | break.c |
| **物理内存** | 页框分配、物理页面管理 | pb.c, pagetable.c |

### 核心设计原则

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        VM 设计原则                                           │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   1. 用户态实现                                                             │
│      - VM 运行在用户态，通过 IPC 与内核通信                                 │
│      - 页错误由内核捕获，转发给 VM 处理                                     │
│                                                                             │
│   2. 区域抽象                                                               │
│      - 地址空间划分为多个区域                         │
│      - 每个区域有独立的内存类型和访问权限                                   │
│                                                                             │
│   3. 按需分页                                                               │
│      - 物理页面延迟分配                                                     │
│      - 页错误时才真正分配物理内存                                           │
│                                                                             │
│   4. 内存类型多态                                                           │
│      - 匿名内存、文件映射、共享内存、直接物理                               │
│      - 统一的内存类型接口，不同的实现                                       │
│                                                                             │
│   5. AVL 树索引                                                             │
│      - 区域按地址组织在 AVL 树中                                            │
│      - O(log n) 查找效率                                                    │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 关键数据结构

| 结构 | 文件 | 说明 |
|------|------|------|
| `struct vmproc` | vmproc.h | VM 进程控制块 |
| `struct vm_region` | region.h | 虚拟内存区域 |
| `struct phys_block` | pb.c | 物理页框块 |
| `struct mem_type` | memtype.h | 内存类型操作表 |

### 核心流程

| 流程 | 文件 | 说明 |
|------|------|------|
| 页错误处理 | pagefaults.c | 内核通知 → 查找区域 → 分配物理页 → 更新页表 |
| mmap | mmap.c | 参数检查 → 创建区域 → 设置内存类型 |
| fork | fork.c | 复制区域 → 设置写时复制 → 复制页表 |
| exit | exit.c | 释放区域 → 释放物理页 → 清理引用 |

---

## 文件列表

### 第一组：核心架构与基础设施

> **学习重点**: VM 服务器的整体架构、进程结构、主循环
> 
> **理论关联**: 微内核服务进程模型、消息驱动架构

| 序号 | 文件 | 状态 | 说明 | 讲解笔记 |
|------|------|------|------|----------|
| 1 | `kernel/vm.h` | ⏳ 待读 | 内核 VM 接口（内核侧） | - |
| 2 | `include/minix/vm.h` | ⏳ 待读 | 用户态 VM 接口（用户侧） | - |
| 3 | `servers/vm/vm.h` | ⏳ 待读 | VM 服务器主头文件 | - |
| 4 | `servers/vm/vmproc.h` | ⏳ 待读 | VM 进程结构 | - |
| 5 | `servers/vm/proto.h` | ⏳ 待读 | VM 函数原型 | - |
| 6 | `servers/vm/glo.h` | ⏳ 待读 | VM 全局变量 | - |
| 7 | `servers/vm/main.c` | ⏳ 待读 | VM 主循环、消息分发 | - |

**核心知识点**：
- VM 与内核的接口定义
- `vmproc` 结构：地址空间描述符
- 主循环：接收页错误请求，分发处理

---

### 第二组：地址空间与区域管理

> **学习重点**: 虚拟内存区域的数据结构和操作
> 
> **理论关联**: 虚拟内存抽象、区域管理

| 序号 | 文件 | 状态 | 说明 | 讲解笔记 |
|------|------|------|------|----------|
| 8 | `servers/vm/region.h` | ⏳ 待读 | 内存区域定义 | - |
| 9 | `servers/vm/region.c` | ⏳ 待读 | 区域管理实现 | - |
| 10 | `servers/vm/regionavl.h` | ⏳ 待读 | 区域 AVL 树头文件 | - |
| 11 | `servers/vm/regionavl.c` | ⏳ 待读 | 区域 AVL 树实现 | - |
| 12 | `servers/vm/regionavl_defs.h` | ⏳ 待读 | 区域 AVL 树定义 | - |
| 13 | `servers/vm/unavl.h` | ⏳ 待读 | AVL 树辅助 | - |
| 14 | `servers/vm/cavl_if.h` | ⏳ 待读 | 通用 AVL 接口 | - |
| 15 | `servers/vm/cavl_impl.h` | ⏳ 待读 | 通用 AVL 实现 | - |
| 16 | `servers/vm/phys_region.h` | ⏳ 待读 | 物理区域定义 | - |

**核心知识点**：
- 区域 结构：地址范围、权限、内存类型
- AVL 树：按地址快速查找区域
- 区域操作：分配、释放、分裂、合并

---

### 第三组：页表与物理内存

> **学习重点**: 页表管理、物理页框分配
> 
> **理论关联**: 分页机制、物理内存管理

| 序号 | 文件 | 状态 | 说明 | 讲解笔记 |
|------|------|------|------|----------|
| 17 | `servers/vm/pt.h` | ⏳ 待读 | 页表管理头文件 | - |
| 18 | `servers/vm/pagetable.c` | ⏳ 待读 | 页表管理实现 | - |
| 19 | `servers/vm/pb.c` | ⏳ 待读 | 页框管理 | - |

**核心知识点**：
- 页表结构：多级页表、页表项
- 物理页框分配：位图管理、分配策略
- 页表更新：映射、取消映射、权限修改

---

### 第四组：内存分配器

> **学习重点**: VM 内部使用的内存分配器
> 
> **理论关联**: 内存分配算法、slab 分配器

| 序号 | 文件 | 状态 | 说明 | 讲解笔记 |
|------|------|------|------|----------|
| 20 | `servers/vm/alloc.c` | ⏳ 待读 | 内存分配入口 | - |
| 21 | `servers/vm/slaballoc.c` | ⏳ 待读 | Slab 分配器 | - |
| 22 | `servers/vm/mem_cache.c` | ⏳ 待读 | 内存缓存 | - |
| 23 | `servers/vm/cache.h` | ⏳ 待读 | 缓存头文件 | - |
| 24 | `servers/vm/cache.c` | ⏳ 待读 | 缓存实现 | - |

**核心知识点**：
- Slab 分配器：高效的小对象分配
- 内存缓存：减少分配开销
- 与内核分配器的协作

---

### 第五组：页错误处理

> **学习重点**: 按需分页的核心逻辑
> 
> **理论关联**: 按需分页、写时复制

| 序号 | 文件 | 状态 | 说明 | 讲解笔记 |
|------|------|------|------|----------|
| 25 | `servers/vm/pagefaults.c` | ⏳ 待读 | 页错误处理 | - |

**核心知识点**：
- 页错误类型：缺页、权限错误、写时复制
- 处理流程：查找区域 → 分配物理页 → 更新页表
- 写时复制：fork 后的延迟复制

---

### 第六组：进程生命周期支持

> **学习重点**: fork/exit 时的地址空间操作
> 
> **理论关联**: 进程创建与终止、地址空间继承

| 序号 | 文件 | 状态 | 说明 | 讲解笔记 |
|------|------|------|------|----------|
| 26 | `servers/vm/fork.c` | ⏳ 待读 | fork 地址空间复制 | - |
| 27 | `servers/vm/exit.c` | ⏳ 待读 | exit 地址空间清理 | - |
| 28 | `servers/vm/break.c` | ⏳ 待读 | heap 管理 | - |

**核心知识点**：
- fork：复制区域、设置写时复制
- exit：释放所有区域和物理页
- sbrk：调整堆区域大小

---

### 第七组：内存映射与内存类型

> **学习重点**: mmap 实现和各种内存类型
> 
> **理论关联**: 内存映射、虚拟内存类型

| 序号 | 文件 | 状态 | 说明 | 讲解笔记 |
|------|------|------|------|----------|
| 29 | `servers/vm/mmap.c` | ✅ 已读 | mmap/munmap 实现 | [vm-memory-mapping.md](vm-memory-mapping.md) |
| 30 | `servers/vm/memtype.h` | ✅ 已读 | 内存类型定义 | [vm-memory-mapping.md](vm-memory-mapping.md) |
| 31 | `servers/vm/mem_anon.c` | ✅ 已读 | 匿名内存 | [vm-memory-mapping.md](vm-memory-mapping.md) |
| 32 | `servers/vm/mem_file.c` | ✅ 已读 | 文件映射 | [vm-memory-mapping.md](vm-memory-mapping.md) |
| 33 | `servers/vm/mem_shared.c` | ✅ 已读 | 共享内存 | [vm-memory-mapping.md](vm-memory-mapping.md) |
| 34 | `servers/vm/mem_directphys.c` | ✅ 已读 | 直接物理内存 | [vm-memory-mapping.md](vm-memory-mapping.md) |
| 35 | `servers/vm/mem_anon_contig.c` | ⏳ 待读 | 连续匿名内存 | - |

**核心知识点**：
- mmap 参数：地址、长度、权限、标志、文件
- 内存类型接口：统一操作，不同实现
- 匿名内存：无后备存储
- 文件映射：以文件为后备
- 共享内存：进程间共享

---

### 第八组：服务接口与协作

> **学习重点**: VM 与其他服务的协作
> 
> **理论关联**: 微内核服务协作、IPC 通信

| 序号 | 文件 | 状态 | 说明 | 讲解笔记 |
|------|------|------|------|----------|
| 36 | `servers/vm/vfs.c` | ✅ 已读 | VFS 接口 | [vm-service-interface.md](vm-service-interface.md) |
| 37 | `servers/vm/rs.c` | ✅ 已读 | RS（重启服务）接口 | [vm-service-interface.md](vm-service-interface.md) |
| 38 | `servers/vm/acl.c` | ✅ 已读 | 访问控制 | [vm-service-interface.md](vm-service-interface.md) |
| 39 | `servers/vm/fdref.h` | ✅ 已读 | 文件描述符引用头文件 | [vm-service-interface.md](vm-service-interface.md) |
| 40 | `servers/vm/fdref.c` | ✅ 已读 | 文件描述符引用实现 | [vm-service-interface.md](vm-service-interface.md) |

**核心知识点**：
- VFS 协作：文件映射、页面换入换出
- RS 协作：服务重启时的内存管理
- fdref：跟踪文件描述符引用

---

### 第九组：工具与辅助

> **学习重点**: 辅助功能和工具函数
> 
> **理论关联**: 软件工程实践

| 序号 | 文件 | 状态 | 说明 | 讲解笔记 |
|------|------|------|------|----------|
| 41 | `servers/vm/util.h` | ✅ 已读 | 工具函数头文件 | [vm-service-interface.md](vm-service-interface.md) |
| 42 | `servers/vm/utility.c` | ✅ 已读 | 工具函数实现 | [vm-service-interface.md](vm-service-interface.md) |
| 43 | `servers/vm/memlist.h` | ✅ 已读 | 内存列表 | [vm-service-interface.md](vm-service-interface.md) |
| 44 | `servers/vm/sanitycheck.h` | ✅ 已读 | 健全性检查 | [vm-service-interface.md](vm-service-interface.md) |

**核心知识点**：
- 工具函数：地址对齐、权限检查
- 健全性检查：调试时验证数据结构

---

### 第十组：内核侧 VM 系统调用

> **学习重点**: 内核中处理 VM 相关系统调用的代码
> 
> **理论关联**: 系统调用机制、内核-用户态接口

| 序号 | 文件 | 状态 | 说明 | 讲解笔记 |
|------|------|------|------|----------|
| 45 | `kernel/system/do_vmctl.c` | ✅ 已读 | VM 控制系统调用 | [vm-service-interface.md](vm-service-interface.md) |
| 46 | `kernel/system/do_memset.c` | ✅ 已读 | 内核内存设置 | [vm-service-interface.md](vm-service-interface.md) |
| 47 | `kernel/system/do_safememset.c` | ✅ 已读 | 安全内存设置 | [vm-service-interface.md](vm-service-interface.md) |

**核心知识点**：
- vmctl：VM 控制接口
- 内核态内存操作

---

### 第十一组：用户态 VM 库函数

> **学习重点**: 用户态调用 VM 服务的库函数封装
> 
> **理论关联**: 系统调用封装、IPC 客户端

| 序号 | 文件 | 状态 | 说明 | 讲解笔记 |
|------|------|------|------|----------|
| 48 | `lib/libsys/vm_fork.c` | ✅ 已读 | VM fork 封装 | [vm-service-interface.md](vm-service-interface.md) |
| 49 | `lib/libsys/vm_exit.c` | ✅ 已读 | VM exit 封装 | [vm-service-interface.md](vm-service-interface.md) |
| 50 | `lib/libsys/vm_procctl.c` | ✅ 已读 | VM 进程控制 | [vm-service-interface.md](vm-service-interface.md) |
| 51 | `lib/libsys/vm_memctl.c` | ✅ 已读 | VM 内存控制 | [vm-service-interface.md](vm-service-interface.md) |
| 52 | `lib/libsys/vm_info.c` | ✅ 已读 | VM 信息获取 | [vm-service-interface.md](vm-service-interface.md) |
| 53 | `lib/libsys/vm_set_priv.c` | ✅ 已读 | VM 设置权限 | [vm-service-interface.md](vm-service-interface.md) |
| 54 | `lib/libsys/vm_map_phys.c` | ✅ 已读 | VM 物理映射 | [vm-service-interface.md](vm-service-interface.md) |
| 55 | `lib/libsys/vm_cache.c` | ✅ 已读 | VM 缓存操作 | [vm-service-interface.md](vm-service-interface.md) |
| 56 | `lib/libsys/vm_getrusage.c` | ✅ 已读 | VM 资源使用 | [vm-service-interface.md](vm-service-interface.md) |
| 57 | `lib/libsys/vm_prepare.c` | ✅ 已读 | VM 准备 | [vm-service-interface.md](vm-service-interface.md) |
| 58 | `lib/libsys/vm_update.c` | ✅ 已读 | VM 更新 | [vm-service-interface.md](vm-service-interface.md) |
| 59 | `lib/libsys/sys_vmctl.c` | ✅ 已读 | 系统 VM 控制 | [vm-service-interface.md](vm-service-interface.md) |
| 60 | `lib/libsys/sys_memset.c` | ✅ 已读 | 系统内存设置 | [vm-service-interface.md](vm-service-interface.md) |
| 61 | `lib/libsys/sys_safememset.c` | ✅ 已读 | 安全内存设置 | [vm-service-interface.md](vm-service-interface.md) |

**核心知识点**：
- 库函数封装：隐藏 IPC 细节
- 参数传递：消息构造
- 返回值处理：错误码转换

---

## 推荐阅读顺序

```
第一阶段：基础认知 (1-2 周)
  └─> 第一组：核心架构与基础设施
      - 理解 VM 在微内核中的角色
      - 熟悉 vmproc 结构

第二阶段：核心机制 (2-3 周)
  └─> 第二组：地址空间与区域管理
  └─> 第三组：页表与物理内存
  └─> 第五组：页错误处理

第三阶段：生命周期 (1 周)
  └─> 第六组：进程生命周期支持
      - fork/exit 与 PM 的协作

第四阶段：高级特性 (2 周)
  └─> 第七组：内存映射与内存类型
  └─> 第八组：服务接口与协作

第五阶段：辅助知识 (可选)
  └─> 第四组：内存分配器
  └─> 第九组：工具与辅助

第六阶段：接口与封装 (1 周)
  └─> 第十组：内核侧 VM 系统调用
  └─> 第十一组：用户态 VM 库函数
      - 理解 IPC 封装层
```

---

## 进度统计

| 分类 | 已读 | 待读 | 覆盖率 |
|------|------|------|--------|
| 核心架构 | 0 | 7 | 0% |
| 区域管理 | 0 | 9 | 0% |
| 页表物理内存 | 0 | 3 | 0% |
| 内存分配器 | 0 | 5 | 0% |
| 页错误处理 | 0 | 1 | 0% |
| 生命周期支持 | 0 | 3 | 0% |
| 内存映射类型 | 0 | 7 | 0% |
| 服务接口 | 0 | 5 | 0% |
| 工具辅助 | 0 | 4 | 0% |
| 内核侧系统调用 | 0 | 3 | 0% |
| 用户态库函数 | 0 | 14 | 0% |
| **总计** | **0** | **61** | **0%** |

---

## Rust 重构要点

### 类型系统改进

| 当前设计 | Rust 改进 |
|----------|-----------|
| `vaddr_t vaddr` (u32) | `struct VAddr(usize)` 强类型 |
| `phys_bytes phys` (u64) | `struct PhysAddr(usize)` 强类型 |
| `int flags` (位标志) | `struct Permissions` 位标志结构体 |

### 内存安全改进

| 当前设计 | Rust 改进 |
|----------|-----------|
| 手动管理物理页引用计数 | `Arc<PhysPage>` 自动引用计数 |
| 区域指针手动管理 | `Box<Region>` 或 arena 分配 |
| 页表操作无检查 | unsafe 块 + 安全封装 |

### 错误处理改进

| 当前设计 | Rust 改进 |
|----------|-----------|
| 返回整数错误码 | `Result<T, VmError>` |
| panic 系统错误 | 类型状态防止非法状态 |

### 异步模型改进

| 当前设计 | Rust 改进 |
|----------|-----------|
| 同步 IPC 阻塞 | `async fn handle_page_fault()` |
| 回调机制 | `Future` 组合子 |

详见各模块讲解文档的 Rust 重构章节。
