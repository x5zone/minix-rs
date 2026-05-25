# 16-vm-fork: VM_FORK 服务

> **分类**: VM私有  
> **源码**: `minix3/minix/servers/vm/fork.c`, `region.c`  
> **说明**: VM 对外提供的 fork 服务，处理进程创建和内存复制

---

## 1. 概述

VM 层 fork 是 fork 系统调用的核心部分，负责复制父进程的地址空间到子进程。Minix3 采用 Copy-on-Write (CoW) 技术，实现高效的内存共享。

### 1.1 fork 在 VM 层的职责

| 职责 | 说明 |
|------|------|
| **地址空间复制** | 复制父进程的所有虚拟区域到子进程 |
| **页表创建** | 为子进程创建新的页表 |
| **CoW 设置** | 将共享页面标记为只读，设置引用计数 |
| **进程结构初始化** | 初始化子进程的 VM 数据结构 |

### 1.2 fork 处理流程

1. PM 调用 `do_fork()`，向 VM 发送 VM_FORK 消息
2. VM 验证参数（`vm_isokendpt()`、slot 范围检查）
3. 初始化子进程结构：`*vmc = *vmp`（浅拷贝），恢复子进程特有字段
4. 创建子进程页表：`pt_new(&vmc->vm_pt)`
5. 复制地址空间：`map_proc_copy(vmc, vmp)` → `map_proc_copy_range()` 遍历每个区域调用 `map_copy_region()`，`map_proc_copy_range()` 内部在所有区域复制完成后调用 `map_writept(src)` 和 `map_writept(dst)` 更新父子进程页表
6. 设置进程标志和 ACL：`vmc->vm_flags &= VMF_INUSE`、`acl_fork(vmc)`
7. 通知内核：`sys_fork()`、`pt_bind()`、`handle_memory_once()`
8. 返回子进程 endpoint 给 PM

> **关键点**：`map_writept()` 在 `map_proc_copy_range()` 内部被调用（region.c:995-996），对父子进程都更新页表。共享页通过 `pr_writable()` → `anon_writable()` 检测 `refcount > 1` 自动标记为只读，实现 CoW。

### 1.3 与 Minix3 源码的对应关系

| 功能 | Minix3 源文件 | 函数 |
|------|--------------|------|
| fork 入口 | `fork.c` | `do_fork()` |
| 地址空间复制 | `region.c` | `map_proc_copy()` |
| 区域范围复制 | `region.c` | `map_proc_copy_range()` |
| 复制单个区域 | `region.c` | `map_copy_region()` |
| 引用物理块 | `pb.c` | `pb_reference()` |
| 更新页表 | `region.c` | `map_writept()` |
| 创建页表 | `pagetable.c` | `pt_new()` |
| 绑定页表 | `pagetable.c` | `pt_bind()` |
| ACL 继承 | `acl.c` | `acl_fork()` |

### 1.4 关键数据结构

> 以下为 Minix3 x86-32 源码中的实际定义，详细字段说明参见各专题文档：[01-vmproc-struct](01-vmproc-struct.md)、[06-pagetable-struct](06-pagetable-struct.md)、[10-phys-pagestate](10-phys-pagestate.md)、[11-region-mapping](11-region-mapping.md)。

```c
// VM 进程结构 (vmproc.h:14)
struct vmproc {
    int vm_flags;                   // 标志 (VMF_INUSE 等)
    endpoint_t vm_endpoint;         // 进程端点
    pt_t vm_pt;                     // 页表数据
    struct boot_image *vm_boot;     // 启动时进程信息
    region_avl vm_regions_avl;      // 虚拟区域 AVL 树
    vir_bytes vm_region_top;        // 最高已插入虚拟地址
    int vm_acl;                     // ACL 索引
    int vm_slot;                    // 进程槽位号
#if VMSTATS
    int vm_bytecopies;
#endif
    vir_bytes vm_total;             // 已分配内存总量
    vir_bytes vm_total_max;         // 历史最大内存
    u64_t vm_minor_page_fault;      // minor 页错误计数
    u64_t vm_major_page_fault;      // major 页错误计数
};

// 虚拟区域 (region.h:37)
typedef struct vir_region {
    vir_bytes vaddr;                // 起始虚拟地址
    vir_bytes length;               // 长度
    struct phys_region **physblocks;// 物理区域数组
    u16_t flags;                    // 权限标志 (VR_WRITABLE 等)
    struct vmproc *parent;          // 所属进程
    mem_type_t *def_memtype;        // 默认内存类型
    int remaps;                     // remap 计数
    int id;                         // 唯一 ID
    union {
        phys_bytes phys;            // VR_DIRECT
        struct { endpoint_t ep; vir_bytes vaddr; int id; } shared;
        struct phys_block *pb_cache;
        struct { int inited; struct fdref *fdref; u64_t offset; u16_t clearend; } file;
    } param;
    struct vir_region *lower, *higher; // AVL 树节点
    int factor;                     // AVL 平衡因子
} region_t;

// 物理区域 (phys_region.h:8)
typedef struct phys_region {
    struct phys_block *ph;          // 物理块指针
    struct vir_region *parent;      // 所属虚拟区域
    vir_bytes offset;               // 区域内偏移
#if SANITYCHECKS
    int written;                    // 是否已写入页表
#endif
    mem_type_t *memtype;            // 内存类型
    struct phys_region *next_ph_list;// 同一 phys_block 的链表
} phys_region_t;

// 物理块 (region.h:23)
struct phys_block {
#if SANITYCHECKS
    u32_t seencount;
#endif
    phys_bytes phys;                // 物理地址
    struct phys_region *firstregion;// 第一个引用
    u8_t refcount;                  // 引用计数
    u8_t flags;                     // 标志 (PBF_INCACHE)
};

// 页表结构 (pt.h:11) — x86-32 两级页表
typedef struct {
    u32_t *pt_dir;                  // 页目录虚拟地址
    u32_t pt_dir_phys;              // 页目录物理地址
    u32_t *pt_pt[ARCH_VM_DIR_ENTRIES]; // 页表指针数组
    u32_t pt_virtop;                // 虚拟地址分配起点
} pt_t;
```

> **32 位 vs 64 位差异**：以上 `pt_t` 为 x86-32 两级页表设计（`ARCH_VM_DIR_ENTRIES=1024`）。
> minix-rs 使用 x86-64 四级页表（PML4+PDPT+PD+PT），页表项大小从 32 位变为 64 位，
> `pt_pt` 数组无法预分配所有页表，需改为动态分配。详见 [06-pagetable-struct](06-pagetable-struct.md)。

### 1.5 CoW 机制

fork 时不立即复制物理内存，而是：

1. **共享物理页面**：父子进程指向相同的物理块
2. **增加引用计数**：`phys_block.refcount++`（通过 `pb_reference()` → `pb_link()`）
3. **标记只读**：`map_writept()` → `map_ph_writept()` → `pr_writable()` 检测到共享页（`refcount > 1`），页表项不含 `PTF_WRITE`
4. **延迟复制**：写入时触发页错误，执行真正的复制

CoW 的完整机制（触发条件、页错误处理流程、`anon_writable()` 逻辑等）详见 [14-cow-mechanism](14-cow-mechanism.md)。

### 1.6 fork 与其他组件的关系

| 组件 | 文件 | 职责 |
|------|------|------|
| PM | `pm/forkexit.c` | 协调 fork 流程，向 VM 发送 VM_FORK 消息 |
| VM | `vm/fork.c` | 处理 VM_FORK，复制地址空间 |
| region.c | `vm/region.c` | 区域复制（`map_proc_copy`、`map_copy_region`）、页表更新（`map_writept`） |
| pagetable.c | `vm/pagetable.c` | 页表创建（`pt_new`）、绑定（`pt_bind`） |
| pb.c | `vm/pb.c` | 物理块引用（`pb_reference`、`pb_link`） |
| acl.c | `vm/acl.c` | ACL 继承（`acl_fork`） |
| Kernel | `kernel/` | `sys_fork()` 创建内核进程、`sys_set_pagetable()` 设置页表 |

---

## 2. C 源码分析

### 2.1 IPC 接口

PM (Process Manager) 发起 fork 请求。当用户进程调用 `fork()` 系统调用时，PM 负责协调整个 fork 流程，包括向 VM 发送 VM_FORK 消息请求复制地址空间。

**PM 调用流程**

```c
// minix3/minix/servers/pm/forkexit.c
int do_fork(void)
{
    // 1. 分配子进程 slot
    next_child = get_free_proc_slot();
    
    // 2. 初始化子进程的 PM 结构
    // ...
    
    // 3. 调用 VM 复制地址空间
    if((s=vm_fork(rmp->mp_endpoint, next_child, &child_ep)) != OK) {
        // VM fork 失败，回滚
        return s;
    }
    
    // 4. VM fork 成功后不能失败
    // 因为 VM 已经调用了 sys_fork()
    
    // 5. 返回子进程信息
    return OK;
}
```

### 2.2 消息类型

**消息定义**

```c
// minix3/minix/include/minix/com.h
#define VM_RQ_BASE          0xC00
#define VM_FORK             (VM_RQ_BASE+1)   // fork 请求消息类型

// 消息字段 (使用 m1 消息格式)
#define VMF_ENDPOINT        m1_i1   // 父进程 endpoint
#define VMF_SLOTNO          m1_i2   // 子进程 slot 号
#define VMF_CHILD_ENDPOINT  m1_i3   // 返回的子进程 endpoint
```

**m1 消息格式**

Minix3 的 IPC 消息是一个 64 字节的 `union message`，内含多种子格式（`mess_1`~`mess_8`）。`VM_FORK` 使用 `mess_1`（简称 m1）格式，该格式提供 3 个 `int` 输入字段（`m1_i1`/`m1_i2`/`m1_i3`）和 3 个 `int` 输出字段。PM 将请求参数写入输入字段，VM 处理完毕后将结果写入输出字段，**请求和响应复用同一个 `message` 结构**——没有独立的 `ForkRequest`/`ForkResponse` 类型。

这种设计的问题：字段语义完全依赖 `#define` 宏名约定，编译器无法检查字段是否被正确使用（例如写入 `m1_i3` 时无法区分是请求还是响应），也无法防止 VM 意外覆盖请求字段。Rust 的三层分离架构（§3.1）正是为了解决这些问题。

### 2.3 请求参数

#### 2.3.1 VMF_ENDPOINT

父进程 endpoint，用于标识要 fork 的源进程。VM 通过此 endpoint 查找父进程的 VM 结构。

> `vmproc` 结构的字段定义和 endpoint 管理机制见 [01-vmproc-struct](01-vmproc-struct.md)。

**验证逻辑**

```c
// minix3/minix/servers/vm/fork.c
if(vm_isokendpt(msg->VMF_ENDPOINT, &proc) != OK) {
    printf("VM: bogus endpoint VM_FORK %d\n", msg->VMF_ENDPOINT);
    return EINVAL;
}
vmp = &vmproc[proc];  // 父进程
```

**vm_isokendpt 函数**

**vm_isokendpt 伪代码**

```
vm_isokendpt(endpoint, &procn):
    procn = _ENDPOINT_P(endpoint)         // 提取 slot
    若 slot 越界 → EINVAL
    若 endpoint 与 vmproc[slot] 不匹配 → EDEADEPT（过期）
    若进程未标记 VMF_INUSE → EDEADEPT
    → OK
```

> **注意**：endpoint 不匹配或进程未使用时返回 `EDEADEPT`（而非 `EINVAL`），表示端点已过期。

#### 2.3.2 VMF_SLOTNO

子进程槽位号（由 PM 分配）。PM 在进程表中为子进程预留了一个 slot，VM 使用此 slot 初始化子进程的 VM 结构。

> 进程表 `vmproc[]` 数组的结构和 slot 管理见 [02-vmproc-table](02-vmproc-table.md)。

**验证逻辑**

```c
childproc = msg->VMF_SLOTNO;
if(childproc < 0 || childproc >= NR_PROCS) {
    printf("VM: bogus slotno VM_FORK %d\n", msg->VMF_SLOTNO);
    return EINVAL;
}
vmc = &vmproc[childproc];  // 子进程
```

### 2.4 返回结果

#### 2.4.1 VMF_CHILD_ENDPOINT

子进程 endpoint（由内核生成）。fork 成功后，VM 返回子进程的 endpoint 给 PM。

```c
msg->VMF_CHILD_ENDPOINT = vmc->vm_endpoint;
```

#### 2.4.2 错误码

| 错误码 | 说明 | 触发条件 |
|--------|------|---------|
| `EINVAL` | 参数无效 | 无效的父进程 endpoint 或子进程 slot |
| `ENOMEM` | 内存不足 | 无法分配页表或复制地址空间 |

**错误处理流程**

| 阶段 | 失败条件 | 返回值 | 处理 |
|------|---------|--------|------|
| 参数验证 | `vm_isokendpt()` 失败或 slot 越界 | `EINVAL` | 直接返回，无副作用 |
| 页表创建 | `pt_new()` 失败 | `ENOMEM` | 直接返回，子进程结构已被浅拷贝修改，但无新资源需释放（pt_new 未成功分配） |
| 地址空间复制 | `map_proc_copy()` 失败 | `ENOMEM` | `pt_free(&vmc->vm_pt)` 释放页表后返回 |
| 内核通知 | `sys_fork()` 失败 | panic | 不可恢复：内核已创建子进程，VM 必须成功 |

**两层回滚链路**

Minix3 的 fork 错误回滚分两层：

1. **区域复制层**（`map_proc_copy_range` 内部）：某个 `map_copy_region` 失败时，先调用 `map_free_proc(dst)` 释放已复制的所有区域，再返回 `ENOMEM`：

```
map_proc_copy_range() 失败
  → map_free_proc(dst)              // 释放 dst 所有已复制区域
    → map_free(region)              // 逐区域释放
      → map_subfree(region, 0, len) // 逐页释放
        → pb_unreferenced(region, pr, 1)
          → pb->refcount--          // 递减物理页引用计数
          → if refcount == 0:
              ev_unreference(pr)    // 通知 memtype 释放物理页
```

2. **do_fork 层**：`map_proc_copy` 返回失败后，`do_fork` 调用 `pt_free(&vmc->vm_pt)` 释放已创建的页表。`map_free_proc` 在 `map_proc_copy_range` 内部已调用（区域已清理），`do_fork` 只需释放页表。

> Rust 实现的对应回滚策略见 §3.4。`pb_unreferenced` 的详细语义见 [14-cow-mechanism](14-cow-mechanism.md) §2.2.2。

### 2.5 do_fork - 主处理函数

fork 的完整流程包括 7 个阶段，各阶段在 §2.6-2.9 逐阶段展开。

**Minix3 源码**：`minix3/minix/servers/vm/fork.c:32`，`do_fork(message *msg)`。

**伪代码**

```c
int do_fork(message *msg) {
    // ========== 阶段 1: 参数验证 ==========
    if(vm_isokendpt(msg->VMF_ENDPOINT, &proc) != OK)
        return EINVAL;                                    // 父进程 endpoint 无效
    childproc = msg->VMF_SLOTNO;
    if(childproc < 0 || childproc >= NR_PROCS)
        return EINVAL;                                    // 子进程 slot 越界
    vmp = &vmproc[proc]; vmc = &vmproc[childproc];

    // ========== 阶段 2: 初始化子进程结构 ==========
    origpt = vmc->vm_pt;                                  // 保存原始页表
    *vmc = *vmp;                                          // 浅拷贝父进程结构
    vmc->vm_slot = childproc;                             // 恢复子进程特有字段
    region_init(&vmc->vm_regions_avl);                    // 初始化空区域树
    vmc->vm_endpoint = NONE;                              // 暂时无效
    vmc->vm_pt = origpt;                                  // 恢复原始页表

    // ========== 阶段 3: 创建页表 ==========
    if(pt_new(&vmc->vm_pt) != OK)                         // 详见 [07-pagetable-ops](07-pagetable-ops.md) §2.1
        return ENOMEM;

    // ========== 阶段 4: 复制地址空间 ==========
    if(map_proc_copy(vmc, vmp) != OK) {                   // 详见 §2.8
        pt_free(&vmc->vm_pt);                             // 回滚：释放页表
        return ENOMEM;                                    // 区域已在 map_proc_copy_range 内回滚（§2.4）
    }

    // ========== 阶段 5: 设置进程标志和 ACL ==========
    vmc->vm_flags &= VMF_INUSE;                           // 只继承 VMF_INUSE
    acl_fork(vmc);                                        // 详见 §2.9.2

    // ========== 阶段 6: 通知内核 ==========
    if(sys_fork(vmp->vm_endpoint, childproc,              // 详见 §2.7.3
            &vmc->vm_endpoint, PFF_VMINHIBIT, &msgaddr) != OK)
        panic("do_fork can't sys_fork");                  // 不可回滚：内核已创建子进程
    if(pt_bind(&vmc->vm_pt, vmc) != OK)                   // 详见 [07-pagetable-ops](07-pagetable-ops.md) §2.3
        panic("fork can't pt_bind");                      // 不可回滚
    handle_memory_once(vmc, msgaddr, sizeof(message), 1); // 处理 fork 消息页面
    handle_memory_once(vmp, msgaddr, sizeof(message), 1);

    // ========== 阶段 7: 返回结果 ==========
    msg->VMF_CHILD_ENDPOINT = vmc->vm_endpoint;
    return OK;
}
```

> **C 源码注释矛盾**：`handle_memory_once` 的 C 源码注释称"return value needn't be checked"（暗示是可选优化），但实际代码在失败时 panic，说明其语义是必要的——fork 消息页面必须正确映射，否则子进程无法接收 fork 返回消息。注释低估了该函数的重要性。

**处理流程图**

```
┌─────────────────────────────────────────────────────────────────┐
│                    do_fork 处理流程                              │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   输入: message *msg (VMF_ENDPOINT, VMF_SLOTNO)                 │
│         │                                                       │
│         ▼                                                       │
│   ┌──────────────────────────────────────────────────────────┐  │
│   │ 阶段 1: 参数验证                                          │  │
│   │                                                          │  │
│   │   vm_isokendpt() ──► 失败 ──► return EINVAL              │  │
│   │         │                                                │  │
│   │         ▼                                                │  │
│   │   slotno 范围检查 ──► 失败 ──► return EINVAL              │  │
│   └──────────────────────────────────────────────────────────┘  │
│         │ OK                                                    │
│         ▼                                                       │
│   ┌──────────────────────────────────────────────────────────┐  │
│   │ 阶段 2: 初始化子进程结构                                   │  │
│   │                                                          │  │
│   │   origpt = vmc->vm_pt     // 保存原始页表                 │  │
│   │   *vmc = *vmp             // 浅拷贝父进程结构             │  │
│   │   vmc->vm_slot = childproc                               │  │
│   │   region_init(&vmc->vm_regions_avl)                      │  │
│   │   vmc->vm_endpoint = NONE                                │  │
│   │   vmc->vm_pt = origpt                                    │  │
│   └──────────────────────────────────────────────────────────┘  │
│         │                                                       │
│         ▼                                                       │
│   ┌──────────────────────────────────────────────────────────┐  │
│   │ 阶段 3: 创建页表                                          │  │
│   │                                                          │  │
│   │   pt_new(&vmc->vm_pt) ──► 失败 ──► return ENOMEM          │  │
│   └──────────────────────────────────────────────────────────┘  │
│         │ OK                                                    │
│         ▼                                                       │
│   ┌──────────────────────────────────────────────────────────┐  │
│   │ 阶段 4: 复制地址空间                                       │  │
│   │                                                          │  │
│   │   map_proc_copy(vmc, vmp)                                │  │
│   │         │                                                │  │
│   │         ▼                                                │  │
│   │   失败? ──► pt_free(&vmc->vm_pt) ──► return ENOMEM       │  │
│   └──────────────────────────────────────────────────────────┘  │
│         │ OK                                                    │
│         ▼                                                       │
│   ┌──────────────────────────────────────────────────────────┐  │
│   │ 阶段 5: 设置进程标志和 ACL                                 │  │
│   │                                                          │  │
│   │   vmc->vm_flags &= VMF_INUSE                             │  │
│   │   acl_fork(vmc)                                          │  │
│   └──────────────────────────────────────────────────────────┘  │
│         │                                                       │
│         ▼                                                       │
│   ┌──────────────────────────────────────────────────────────┐  │
│   │ 阶段 6: 通知内核                                          │  │
│   │                                                          │  │
│   │   sys_fork() ──► 失败 ──► panic (不可恢复)               │  │
│   │         │                                                │  │
│   │         ▼                                                │  │
│   │   pt_bind(&vmc->vm_pt, vmc)                              │  │
│   │         │                                                │  │
│   │         ▼                                                │  │
│   │   handle_memory_once() x2  // 处理消息页面               │  │
│   └──────────────────────────────────────────────────────────┘  │
│         │                                                       │
│         ▼                                                       │
│   ┌──────────────────────────────────────────────────────────┐  │
│   │ 阶段 7: 返回结果                                          │  │
│   │                                                          │  │
│   │   msg->VMF_CHILD_ENDPOINT = vmc->vm_endpoint             │  │
│   │   return OK                                              │  │
│   └──────────────────────────────────────────────────────────┘  │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### 2.6 验证阶段

#### 2.6.1 验证父进程

使用 `vm_isokendpt()` 检查父进程 endpoint 是否有效，确保进程存在且在使用中。

**验证要点**

`vm_isokendpt()` (utility.c:84) 执行三步验证：

1. **slot 范围检查**：`*procn = _ENDPOINT_P(endpoint)`，若 `*procn < 0 || *procn >= NR_PROCS` 返回 `EINVAL`
2. **endpoint 一致性**：若 `endpoint != vmproc[*procn].vm_endpoint` 返回 `EDEADEPT`（防止使用过期 endpoint）
3. **进程状态**：若 `!(vmproc[*procn].vm_flags & VMF_INUSE)` 返回 `EDEADEPT`（进程不在使用中）

#### 2.6.2 验证子进程槽位

检查 PM 提供的子进程 slot 是否在有效范围内，且未被其他进程占用。

**验证逻辑**

```c
childproc = msg->VMF_SLOTNO;

// 1. 范围检查
if(childproc < 0 || childproc >= NR_PROCS) {
    printf("VM: bogus slotno VM_FORK %d\n", msg->VMF_SLOTNO);
    return EINVAL;
}

// 2. 获取子进程结构
vmc = &vmproc[childproc];

// 3. 防御性断言（非严格验证，Minix3 依赖 PM 保证 slot 一致性）
assert(vmc->vm_slot == childproc);
```

### 2.7 初始化阶段

#### 2.7.1 分配子进程结构

使用 PM 提供的 slot 初始化子进程的 vmproc 结构，设置基本标志。

> `vmproc` 结构的完整字段定义见 [01-vmproc-struct](01-vmproc-struct.md)。`region_init` 的 AVL 树初始化见 [13-region-avl](13-region-avl.md)。

**初始化步骤**

1. **保存原始页表**：`origpt = vmc->vm_pt`（子进程 slot 原有的页表）
2. **浅拷贝父进程结构**：`*vmc = *vmp`（此时 vmc 暂时与 vmp 共享区域树指针、页表指针、标志等）
3. **恢复子进程特有字段**：
   - `vmc->vm_slot = childproc`（恢复自己的 slot）
   - `region_init(&vmc->vm_regions_avl)`（初始化空区域树）
   - `vmc->vm_endpoint = NONE`（暂时无效，等待内核分配）
   - `vmc->vm_pt = origpt`（恢复原始页表，等待 `pt_new()`）

初始化完成后子进程状态：`vm_endpoint=NONE`，`vm_regions_avl=空`，`vm_pt=原始页表`，`vm_flags` 从父进程复制（后续 `&= VMF_INUSE`），`vm_slot=childproc`。

> **设计观察 — 冗余调用**：`do_fork()` 在此步骤中调用了 `region_init(&vmc->vm_regions_avl)`（fork.c:60），而后续 `map_proc_copy()` 内部也调用了 `region_init(&dst->vm_regions_avl)`（region.c:935）。这是 Minix3 的防御性编程——`region_init` 将 AVL 树初始化为空树，是幂等操作，重复调用安全但冗余。

#### 2.7.2 创建页表

调用 `pt_new()` 为子进程创建新的页表结构。

**pt_new 伪代码**

```c
int pt_new(pt_t *pt) {
    if(!pt->pt_dir)
        pt->pt_dir = vm_allocpages(&pt->pt_dir_phys, VMP_PAGEDIR, ...);  // 分配页目录
    if(!pt->pt_dir) return ENOMEM;
    assert(pt->pt_dir_phys % ARCH_PAGEDIR_SIZE == 0);                     // 对齐验证
    for(i = 0; i < ARCH_VM_DIR_ENTRIES; i++) {
        pt->pt_dir[i] = 0;   // 无效条目 (PRESENT=0)
        pt->pt_pt[i] = NULL;
    }
    pt->pt_virtop = 0;                                                    // 虚拟地址起点
    return pt_mapkernel(pt);                                               // 映射内核空间
}
```

> `pt_new` 的完整 C 源码和 `vm_allocpages` 分配细节见 [07-pagetable-ops](07-pagetable-ops.md) §2.1。页表结构（pt_t 字段、x86 两级页表、地址转换）见 [06-pagetable-struct](06-pagetable-struct.md)。

> **32 位 vs 64 位差异**：x86-32 使用两级页表（1024 个 PDE，每个 PDE 指向 1024 项页表，寻址 4GB）。
> x86-64 使用四级页表（PML4 → PDPT → PD → PT），虚拟地址 48 位，PTE 从 32 位扩展为 64 位。
> `pt_pt[]` 数组在 64 位下无法预分配所有页表，需改为动态分配。

#### 2.7.3 调用 sys_fork

内核生成子进程 endpoint，完成进程表中的注册。

**VM 侧调用**

```c
if(sys_fork(vmp->vm_endpoint, childproc,
        &vmc->vm_endpoint, PFF_VMINHIBIT, &msgaddr) != OK)
    panic("do_fork can't sys_fork");  // 不可回滚
```

**参数说明**

| 参数 | 说明 |
|------|------|
| `vmp->vm_endpoint` | 父进程 endpoint |
| `childproc` | 子进程 slot 号 |
| `&vmc->vm_endpoint` | 输出：子进程 endpoint |
| `PFF_VMINHIBIT` | fork 标志：阻止调度直到 VM 释放 |
| `&msgaddr` | 输出：fork 消息地址 |

**关键点**

1. **不可逆性**：`sys_fork()` 成功后内核已创建子进程，VM 必须成功，否则系统状态不一致。因此 `sys_fork()` 失败时 panic，而不是返回错误
2. **PFF_VMINHIBIT 标志**：源码定义为"Don't schedule until release by VM"（`com.h:360`）。内核在 `sys_fork()` 中检测到此标志后设置 `RTS_VMINHIBIT`，阻止子进程被调度，直到 VM 完成内存初始化后调用 `handle_memory_once()` 清除此标志。这是 Minix3 微内核架构中 VM 与内核的同步机制：PM 先请求 VM 准备内存，VM 完成后通知内核放行子进程
3. **fork 消息地址**：内核返回 fork 消息的地址，VM 需要处理这个消息页面的内存访问（`handle_memory_once`）

**内核侧消息字段**

VM 调用 `sys_fork()` 时，消息在 VM 侧使用 `VMF_ENDPOINT`/`VMF_SLOTNO` 字段名，经 IPC 传递到内核后，内核侧使用不同的字段名访问：

| 方向 | 字段名 | 含义 |
|------|--------|------|
| VM → 内核 | `m_lsys_krn_sys_fork.endpt` | 父进程 endpoint |
| VM → 内核 | `m_lsys_krn_sys_fork.slot` | 子进程 slot |
| VM → 内核 | `m_lsys_krn_sys_fork.flags` | fork 标志（含 PFF_VMINHIBIT） |
| 内核 → VM | `m_krn_lsys_sys_fork.endpt` | 子进程 endpoint（内核生成） |
| 内核 → VM | `m_krn_lsys_sys_fork.msgaddr` | fork 消息页面地址 |

> 详见 `minix3/minix/kernel/system/do_fork.c:5-9`。

**内核 `do_fork()` 处理**（`kernel/system/do_fork.c:26`）

> **注意**：以下伪代码省略了平台特定细节（如 i386 的 FPU/SSE 上下文复制、ARM 的 TTB 设置等），仅保留与 fork 语义直接相关的核心逻辑。

```c
int do_fork(endpoint_t endpt, int slot, int flags, ...) {
    // 1. 验证父进程
    isokendpt(endpt, &p_proc);            // 父进程 endpoint 有效性
    isemptyp(rpc);                        // 子进程 slot 为空

    // 2. 复制整个 proc 结构体
    *rpc = *rpp;                          // 浅拷贝父进程 struct proc
    rpc->p_nr = slot;                     // 恢复子进程编号
    rpc->p_endpoint = _ENDPOINT(++gen, p_nr);  // 递增代数生成新 endpoint
    rpc->p_reg.retreg = 0;                // 子进程 fork 返回 0

    // 3. 设置子进程调度状态
    RTS_SET(rpc, RTS_NO_QUANTUM);         // 无时间片，不可调度
    if(flags & PFF_VMINHIBIT)
        RTS_SET(rpc, RTS_VMINHIBIT);      // VM 阻塞，等待内存初始化
    RTS_UNSET(rpc, RTS_SIGNALED | RTS_SIG_PENDING | RTS_P_STOP);  // 不继承信号

    // 4. 特权处理
    if(rpp->p_priv->s_flags & SYS_PROC) {
        rpc->p_priv = privp(USER_PRIV);   // 系统进程子进程降级为 USER_PRIV
        RTS_SET(rpc, RTS_NO_PRIV);
    }

    // 5. 返回值
    reply->endpt = rpc->p_endpoint;       // 子进程 endpoint
    reply->msgaddr = rpp->p_delivermsg_vir; // 消息页面地址
}
```

> **endpoint 生成方式**：内核使用 `_ENDPOINT(++gen, p_nr)` 递增代数生成新 endpoint，而非 `_ENDPOINT(0, child_slot)`。这保证了 endpoint 的唯一性——即使 slot 被复用，代数递增使得旧 endpoint 不可能匹配新进程。

### 2.8 内存复制阶段

#### 2.8.1 map_proc_copy

遍历父进程的所有虚拟区域，为每个区域创建副本。

> 区域映射的完整机制（`vir_region` 结构、区域查找/插入/删除）见 [11-region-mapping](11-region-mapping.md)。

**map_proc_copy 伪代码**

```c
int map_proc_copy(dst, src) {
    region_init(&dst->vm_regions_avl);                    // 初始化空区域树
    return map_proc_copy_range(dst, src, NULL, NULL);     // 复制全部区域（§2.8.2）
}
```

#### 2.8.2 map_proc_copy_range

复制指定范围内的虚拟区域。

**map_proc_copy_range 伪代码**

```c
int map_proc_copy_range(dst, src, start_vr, end_vr) {
    if(!start_vr) start_vr = region_search_least(src);    // 默认：最小区域
    if(!end_vr)   end_vr   = region_search_greatest(src);  // 默认：最大区域

    for each vr from start_vr to end_vr (AVL 迭代) {
        newvr = map_copy_region(dst, vr);                  // 复制单个区域（§2.8.3）
        if(!newvr) {
            map_free_proc(dst);                            // 回滚：释放已复制区域（§2.4）
            return ENOMEM;
        }
        region_insert(&dst->vm_regions_avl, newvr);        // 插入目标区域树
    }

    map_writept(src);                                      // 更新父进程页表（CoW 只读）
    map_writept(dst);                                      // 更新子进程页表（CoW 只读）
    return OK;
}
```

> `region_search_least`/`region_insert` 等 AVL 操作见 [13-region-avl](13-region-avl.md)。`map_writept` 的 CoW 只读设置见 §2.8.4。

**处理流程图**

```
确定复制范围 (least → greatest)
        │
        ▼
┌─► map_copy_region(dst, vr)
│       │
│       ├─ 失败 → map_free_proc(dst) → return ENOMEM
│       │
│       ▼
│   region_insert(dst, newvr)
│       │
│       ▼
│   还有下一个区域? ── 是 ──► 下一个 vr
│       │
│       否
│       ▼
└── map_writept(src) + map_writept(dst) → return OK
```

#### 2.8.3 map_copy_region

复制单个虚拟区域，包括创建新的 vir_region 结构和共享物理块。

> `phys_block`/`phys_region` 的结构和 `pb_reference`/`pb_link` 的完整语义见 [10-phys-pagestate](10-phys-pagestate.md)。

> **C 源码注释矛盾**：`map_copy_region` 的注释（region.c:804-810）声称"it doesn't increase the refcount in the phys_block; the caller has to do this once it's linked"，但实际代码中 `pb_reference()` → `pb_link()` 已经递增了 refcount（`pb_link` 内部调用 `pb->refcount++`）。注释描述的是设计意图（延迟递增以保持 sanity check 工作），但实际实现并未遵循。

**map_copy_region 伪代码**

```c
struct vir_region *map_copy_region(vmp, vr) {
    newvr = region_new(vr->parent, vr->vaddr, vr->length, vr->flags, vr->def_memtype);
    if(!newvr) return NULL;
    newvr->parent = vmp;                                  // 设置为子进程

    if(vr->def_memtype->ev_copy &&
       vr->def_memtype->ev_copy(vr, newvr) != OK) {      // memtype 复制回调
        map_free(newvr);
        return NULL;
    }

    for(p = 0; p < phys_slot(vr->length); p++) {
        ph = physblock_get(vr, p*VM_PAGE_SIZE);
        if(!ph) continue;                                 // 跳过未分配槽位
        newph = pb_reference(ph->ph, ph->offset, newvr, vr->def_memtype);  // 共享物理块（refcount++）
        if(!newph) { map_free(newvr); return NULL; }
        if(ph->memtype->ev_reference)
            ph->memtype->ev_reference(ph, newph);         // 引用回调（返回值被忽略 — C 源码 bug）
    }
    return newvr;
}
```

> `pb_reference`/`pb_link` 的完整语义见 [10-phys-pagestate](10-phys-pagestate.md)。`ev_copy`/`ev_reference` 回调机制见 [12-memtype](12-memtype.md)。

> **C 源码 bug**：`map_copy_region()` 中 `ev_reference` 的返回值被忽略（region.c:841-842 `if(ph->memtype->ev_reference) ph->memtype->ev_reference(ph, newph);`），如果回调返回错误，物理块引用状态可能不一致。Rust `fork_region()` 修复了此问题：`ev_reference` 失败时回滚已递增的 refcount（遍历 `refcounted_pfns` 向量逐个递减）。详见 [14-cow-mechanism](14-cow-mechanism.md) §2.5。

> **Rust 实现差异**：`fork_region()` 在 `ev_reference` 失败时会自动回滚已递增的 refcount（遍历 `refcounted_pfns` 向量逐个递减），而 Minix3 的 `map_copy_region()` 在失败时调用 `map_free(newvr)` 释放整个新区域（`map_free` 内部会调用 `pb_unreferenced` 递减 refcount）。

#### 2.8.4 CoW 设置

共享 phys_block 后，将页表项标记为只读，触发写时复制。

**map_writept 伪代码**

```c
int map_writept(struct vmproc *vmp) {
    for each vir_region vr in vmp->vm_regions_avl {
        for each phys_region ph in vr (page granularity) {
            map_ph_writept(vmp, vr, ph);  // 写入单个页表项
        }
    }
}
```

**map_ph_writept 伪代码** — 页表标志计算

```c
int map_ph_writept(vmp, vr, pr) {
    flags = PTF_PRESENT | PTF_USER;
    if(pr_writable(vr, pr))
        flags |= PTF_WRITE;       // 私有可写页
    else
        flags |= PTF_READ;        // 共享页只读（CoW）或本身只读
    flags |= vr->def_memtype->pt_flags(vr);  // memtype 附加标志
    pt_writemap(vmp, &vmp->vm_pt, vr->vaddr + pr->offset,
                pr->ph->phys, VM_PAGE_SIZE, flags, WMF_OVERWRITE);
}
```

**pr_writable → anon_writable 判断链**

CoW 只读机制的核心判断链：`pr_writable(vr, pr)` → `memtype->writable(pr)` → `anon_writable(pr)`。对于匿名内存，`anon_writable()` 在 `refcount > 1` 时返回 0（不可写），fork 后共享页因此被标记为只读。写入时触发页错误，CoW 处理分配新物理页后 `refcount` 降为 1，页表恢复可写。

fork 语境下的页表标志：

| 条件 | 页表标志 | 说明 |
|------|---------|------|
| `refcount == 1` 且 `VR_WRITABLE` | `PTF_PRESENT | PTF_USER | PTF_WRITE` | 私有可写页 |
| `refcount > 1` 且 `VR_WRITABLE` | `PTF_PRESENT | PTF_USER | PTF_READ` | 共享页只读（CoW） |
| 非 `VR_WRITABLE` | `PTF_PRESENT | PTF_USER | PTF_READ` | 本身只读页 |

> `anon_writable()` 的完整判断逻辑（`MAP_NONE`/`remaps`/`refcount` 三条件）和 CoW 页错误处理流程详见 [14-cow-mechanism](14-cow-mechanism.md) §2.4.1。`map_writept`/`map_ph_writept` 的完整 C 源码见 `region.c:906` 和 `region.c:257`。

### 2.9 完成阶段

#### 2.9.1 设置子进程状态

设置 vm_flags 为 VMF_INUSE，设置 vm_endpoint 为子进程端点。

```c
// 只继承 VMF_INUSE 标志
vmc->vm_flags &= VMF_INUSE;
```

#### 2.9.2 ACL 继承

调用 `acl_fork()` 处理子进程的 ACL 条目。

> ACL 机制的完整设计（ACL 类型、权限检查流程）见 [03-acl](03-acl.md)。

**acl_fork 伪代码**

```c
void acl_fork(vmp) {    // vmp 是子进程（vmc），已通过 *vmc = *vmp 继承了父进程的 vm_acl
    if(vmp->vm_acl != USER_ACL)
        vmp->vm_acl = NO_ACL;   // 系统进程 ACL 不继承，降级为 NO_ACL
}
```

**ACL 继承规则**

| 父进程 ACL | 子进程 ACL | 说明 |
|-----------|-----------|------|
| `NO_ACL` (-1) | `NO_ACL` | 无限制，直接继承 |
| `USER_ACL` (0) | `USER_ACL` | 用户进程共享 ACL，直接继承 |
| 系统进程 ACL | `NO_ACL` | 系统进程 ACL 是进程特定的，不应继承，由 RS 重新设置 |

> **注意**：`acl_fork()` 中的 `vmp` 此时指向子进程（因为 `*vmc = *vmp` 后 `vmc` 的 `vm_acl` 继承了父进程值），所以 `vmp->vm_acl != USER_ACL` 检查的是子进程继承来的 ACL 值。

---

## 3. Rust 设计决策

> **阅读提示**：
> - §3.1 IPC 消息类型：`VmForkIn`/`VmForkOut` 的传输层定义与编解码（`os/libs/minix-types/src/ipc/vm.rs`）
> - §3.2 Direct Map 对 fork 的简化：CoW 和页表创建如何受益于 Direct Map
> - §3.3 错误处理：`ForkError` 6 变体与 Minix3 错误路径的对应
> - §3.4 `do_fork` 编排：验证→初始化→页表→区域复制→CoW→sys_fork→pt_bind 的完整流程
> - §3.5 进程表管理：`AssumeSyncCell` + typestate 视图如何支持 fork 的跨 slot 访问

### 3.1 IPC 消息类型设计：传输层 + 语义层 + 编解码

Minix3 的 IPC 消息是一个 64 字节的大 union（`struct message`），没有独立的 per-message 类型。`VM_FORK` 消息使用的是 `mess_1` 子格式（`m_m1`），通过 `#define` 宏将字段语义映射到偏移：

```c
// minix3/minix/include/minix/com.h:633-635
#define VMF_ENDPOINT        m1_i1   // 父进程 endpoint
#define VMF_SLOTNO          m1_i2   // 子进程 slot
#define VMF_CHILD_ENDPOINT  m1_i3   // 返回值: 子进程 endpoint
```

请求和响应**复用同一个 `message`** —— PM 把 `VMF_ENDPOINT` 和 `VMF_SLOTNO` 写入 `m1_i1`/`m1_i2`，VM 处理完毕后把 `VMF_CHILD_ENDPOINT` 写入 `m1_i3`。**没有独立的 `ForkRequest` 结构体**。

Rust 设计采用**三层分离**架构：

```
┌──────────────────────────────────────┐
│         语义层 (Semantic)             │  ← handler 操作的类型，栈上 Copy
│  VmForkIn / VmForkOut                │
├──────────────────────────────────────┤
│         编解码层 (Codec)              │  ← #[inline(always)]，编译后等同 C 宏
│  DecodeFromM1 / EncodeToM1           │
├──────────────────────────────────────┤
│         传输层 (Transport)            │  ← #[repr(C)]，与 C message 二进制兼容
│  Message / MessageM1 / MessageUnion  │
└──────────────────────────────────────┘
```

**语义层类型定义**（`minix-types/src/ipc/vm.rs`）：

```rust
/// PM → VM: fork 请求
/// 对应 Minix3: VMF_ENDPOINT(m1_i1), VMF_SLOTNO(m1_i2)
pub struct VmForkIn {
    pub parent_endpoint: Endpoint,  // 父进程 endpoint（PM 传入）
    pub child_slot: UserSlot,       // 子进程 slot 号（PM 分配）
}

/// VM → PM: fork 响应
/// 对应 Minix3: VMF_CHILD_ENDPOINT(m1_i3)
pub struct VmForkOut {
    pub child_endpoint: Endpoint,   // 子进程 endpoint（内核生成）
}
```

**编解码实现**：

```rust
// 从 m1 消息解码 fork 请求（PM → VM 方向）
impl DecodeFromM1 for VmForkIn {
    #[inline(always)]
    fn decode(m1: &MessageM1) -> Self {
        Self {
            parent_endpoint: Endpoint(m1.m1i1),  // m1_i1 → 父进程 endpoint
            child_slot: UserSlot(m1.m1i2 as usize), // m1_i2 → 子进程 slot
        }
    }
}

// 将 fork 响应编码到 m1 消息（VM → PM 方向）
impl EncodeToM1 for VmForkOut {
    #[inline(always)]
    fn encode(&self, m1: &mut MessageM1) {
        m1.m1i3 = self.child_endpoint.0; // 子进程 endpoint → m1_i3
    }
}
```

**设计要点**：

1. **请求/响应分离**：`VmForkIn`（PM→VM 输入）和 `VmForkOut`（VM→PM 输出）是两个独立类型，分别对应 `mess_1` 的不同字段。不像旧设计把 `child_endpoint`（响应字段）混入请求类型。
2. **命名遵守链路**：`In` = VM 接收，`Out` = VM 发出。同一链路的其他方向消息（如 VM→Kernel 的 `SYS_FORK`）属于独立类型，不在 VM 的 IPC 类型中。
3. **编解码零开销**：`#[inline(always)]` + `Copy` + 栈分配，编译后与 C 宏生成相同的 `mov` 指令，无额外 CPU 或内存开销。
4. **传输层独立于语义层**：`Message`/`MessageM1` 是 `#[repr(C)]` flat struct，与 C 的 `message` union 二进制兼容。修改语义层不影响传输格式。

**VMF_INHIBIT 标志不属于 VM_FORK**：

C 源码中 `PFF_VMINHIBIT` 标志是 VM→**Kernel** `sys_fork()` 的参数（`fork.c:91`），不是 PM→VM `VM_FORK` 的参数。该标志应出现在 `SysForkIn` 类型中（VM→Kernel 链路），不在 `VmForkIn` 中。

> **TODO**: 内核侧 `SysForkIn`/`SysForkOut` 类型尚未实现，需在 VM→Kernel IPC 链路设计中补充。

### 3.2 Direct Map 对 fork 实现的简化

Direct Map 从根本上消除了"VM 无法直接通过物理地址访问内存"的限制，fork 实现的两个环节因此被简化。

**CoW 设置的简化**

CoW 设置中，`pt_writemap()` 需要写入目标进程的页表项。在 Minix3 中，这些页表页通过 `vm_mappages()` 映射到 VM 自身地址空间后才能操作。Direct Map 方案下，`vm_phys_to_virt()` 让 VM 直接访问任意物理页，无需 `vm_mappages` 的"找洞→映射"流程。

CoW 的策略逻辑（`refcount > 1` → 只读 → 写入触发页错误 → 分配新页）完全不变——简化的是"如何写入页表项"的机制，而非 CoW 策略本身。

**页表创建的简化**

`pt_new()` 为子进程创建新页表，两方案步骤数相同，但每步复杂度不同：

| 步骤 | Minix3 | Direct Map |
|------|--------|------------|
| 分配物理页 | `vm_allocpages()`：`alloc_mem()` + `vm_mappages()` 映射到 VM 地址空间 | `bitmap.alloc_mem(1)`：分配即可，物理页天然有 VA |
| 清零页目录 | 通过 `vm_mappages` 映射的虚拟地址操作 | `vm_phys_to_virt(dir_phys)` 直接操作 |
| 建立内核映射 | `pt_mapkernel()` | `pt_mapkernel()`（含 kernel direct map） |
| 复制用户空间映射 | 遍历父进程区域复制 | 遍历父进程区域复制 |

Minix3 的 `vm_allocpages` 流程：`alloc_mem()` 分配物理页 → `findhole()` 在 VM 地址空间找空洞 → `pt_writemap()` 建立映射 → `sys_vmctl(VMCTL_FLUSHTLB)` 刷新 TLB。Direct Map 跳过了后三步。

> **`createpde` 澄清**：`createpde` 是 **kernel** 的函数（`kernel/arch/i386/memory.c`），用于 kernel 的 `virtual_copy`/`vm_memset` 在进程地址空间间拷贝数据时临时映射 4MB 窗口，与 VM 创建页表无关。VM 自身使用 `vm_mappages()` 管理物理页映射。

**双视图模型在 fork 中的协作**：VM 通过 VM direct map（`vm_phys_to_virt()`）操作物理页（清零、复制），通过 `pt_mapkernel()` 确保新页表包含 kernel direct map。两者在 fork 中协作——VM 用自己的视图操作数据，用内核的视图确保新进程的页表包含内核映射。

### 3.3 错误处理

使用 `Result<T, ForkError>` 处理各阶段失败。

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ForkError {
    InvalidEndpoint,       // 父进程 endpoint 无效
    InvalidSlot,           // 父进程 slot 非活跃状态
    SlotInUse,             // 子进程 slot 已被占用
    NoMemory,              // 物理内存不足（页表/区域复制）
    PageNotMapped,         // 页表项缺失
    MemType(MemTypeError), // MemType 回调失败
}
```

| 变体 | 对应 Minix3 错误 | 触发场景 |
|------|-----------------|---------|
| `InvalidEndpoint` | `vm_isokendpt()` → `EINVAL`/`EDEADEPT` | 父进程 endpoint 不存在、已过期或进程未使用 |
| `InvalidSlot` | `vm_isokendpt()` 通过但 `get_active` 返回 None | 父进程不在活跃状态（未设置 `IN_USE` 标志，或处于 `EXITING` 退出过程中） |
| `SlotInUse` | Minix3 未显式检查（依赖 PM 保证 slot 空闲） | 子进程 slot 已被占用（`IN_USE` 已设置），Rust 通过 `get_empty()` 显式验证 |
| `NoMemory` | `pt_new` → `ENOMEM`、`pb_new` → `ENOMEM`、`alloc_mem` → `NO_MEM` | 页表分配或物理页分配失败 |
| `PageNotMapped` | 无直接对应（Minix3 不区分此情况） | 页表项缺失 |
| `MemType(MemTypeError)` | `ev_copy`/`ev_reference` 返回非 OK | MemType 回调（如 `ev_reference`）失败 |

与 Minix3 的关键差异：Minix3 的 `pb_new` → `ENOMEM`、`pb_reference` → `ENOMEM`、`alloc_mem` → `NO_MEM` 三条物理内存分配失败路径，在 Rust 中统一为 `NoMemory` 一条——因为 `PageSlot` 是 `Copy` 类型无需堆分配，`PageFrames` 全局数组无需创建 `PhysBlock` 对象。新增的 `InvalidEndpoint`/`InvalidSlot`/`SlotInUse` 三个变体对应 Minix3 中 `vm_isokendpt()` 和 slot 检查的验证路径，Minix3 统一返回 `EINVAL`，Rust 拆分为更精确的错误类型。

### 3.4 事务性

fork 是多阶段操作（验证→页表创建→区域复制→内核通知），任何阶段失败都需回滚已完成的副作用。

#### Minix3 的回滚机制

Minix3 的 `do_fork()`（`fork.c:32`）采用线性错误路径，每个阶段失败后执行对应的清理：

| 阶段 | Minix3 代码 | 失败条件 | 回滚动作 |
|------|------------|---------|---------|
| 验证 | `vm_isokendpt()` / slot 范围检查 | endpoint 无效或 slot 越界 | `return EINVAL`（无副作用） |
| 页表创建 | `pt_new(&vmc->vm_pt)` | 内存不足 | `return ENOMEM`（子进程结构未修改） |
| 区域复制 | `map_proc_copy(vmc, vmp)` | `map_copy_region` 返回 NULL | `pt_free(&vmc->vm_pt)` + `map_free_proc(dst)` |
| 内核通知 | `sys_fork(...)` | 内核返回错误 | `panic`（不可回滚：内核已创建子进程） |
| 页表绑定 | `pt_bind(...)` | 绑定失败 | `panic`（不可回滚） |

区域复制阶段的回滚链路：

```
map_proc_copy_range() 失败
  → map_free_proc(dst)              // 释放 dst 所有已复制区域
    → map_free(region)              // 逐区域释放
      → map_subfree(region, 0, len) // 逐页释放
        → pb_unreferenced(region, pr, 1)
          → pb->refcount--          // 递减物理页引用计数
          → if refcount == 0:
              ev_unreference(pr)    // 通知 memtype 释放物理页
  → pt_free(&vmc->vm_pt)           // 释放已创建的页表
```

关键点：`map_free_proc` 在 `map_proc_copy_range` 内部调用，而非 `do_fork` 中调用。`do_fork` 只负责 `pt_free`。这是因为 `map_proc_copy` 返回时，失败路径的区域已经被 `map_free_proc` 清理，但页表仍需 `do_fork` 释放。

#### Rust 实现

Rust 不需要 `ForkState` 状态机。Minix3 本身也没有状态机——它用的是线性错误路径。Rust 用 `Result` + 显式清理函数实现同样的语义：

**两层回滚**：

1. **`fork_regions` 层**（对应 Minix3 的 `map_proc_copy_range`）：失败时调用 `free_forked_regions` 递减已复制区域的 refcount + 调用 `ev_unreference`
2. **`do_fork` 层**（对应 Minix3 的 `do_fork`）：`fork_regions` 失败时释放页表

`fork_regions` 的回滚策略：逐区域调用 `fork_region`，任一失败则对已成功的区域调用 `free_forked_regions` 回滚，再向上传播错误。这对应 Minix3 的 `map_free_proc(dst)`——区别是 Minix3 遍历侵入式链表 `phys_block.firstregion` 逐个 `pb_unlink`，Rust 遍历 `region.physblocks` 递减 `PageFrames.states[pfn].refcount`。

`do_fork` 的编排步骤与 Minix3 的 `do_fork` 一一对应：

| 步骤 | Minix3 | Rust | 失败处理 |
|------|--------|------|---------|
| 验证父进程 | `vm_isokendpt()` | `table.vm_isokendpt()` → `get_active()` | `InvalidEndpoint`/`InvalidSlot` |
| 验证子进程 slot | slot 范围检查 + `assert` | `table.get_empty()` | `SlotInUse` |
| 初始化子进程 | `*vmc = *vmp` + 恢复字段 | `activate_relaxed()` → `init_from_fork()` + `copy_acl_from()` | 不会失败 |
| 创建页表 | `pt_new()` | `init_page_table()` | `NoMemory` |
| 复制区域 | `map_proc_copy()` | `fork_regions()` | 内部回滚 + `free_page_table()` |
| CoW + 页表写入 | `map_writept()` ×2 | `setup_cow_for_all_regions()` + `write_page_table_mappings()` | 不会失败 |
| 内核通知 | `sys_fork()` | `sys_fork()`（stub） | panic（不可回滚） |
| 页表绑定 | `pt_bind()` | `bind_page_table()` | panic（不可回滚） |
| 消息页面处理 | `handle_memory_once()` ×2 | TODO（stub） | panic（不可回滚，注释矛盾已识别，见 §2.5） |

关键设计差异：Minix3 用 `*vmc = *vmp` 整结构体浅拷贝再逐字段恢复，Rust 用 `init_from_fork()` 逐字段拷贝——避免拷贝后"恢复"的脆弱模式，typestate 视图保证子进程只能通过 `EmptySlot` → `ActiveProc` 转换进入活跃状态。

### 3.5 进程表管理

fork 对进程表的核心需求：通过 endpoint 定位父进程（只读），通过 slot 初始化子进程（写入）。两者操作不同 slot，且父进程只读、子进程只写。

`VmProcTable` 的 `AssumeSyncCell<VmProc>` 静态数组 + typestate 视图设计天然满足这一需求：

- **slot 粒度隔离**：每个 slot 的 `UnsafeCell` 独立，`get_active(parent_slot)` 和 `get_empty(child_slot)` 返回的视图各自持有不同 slot 的 `&mut VmProc`，Rust 借用检查器看到的是两个 `&table` 共享借用，完全合法
- **不变量显式检查**：`do_fork` 中 `assert_ne!(parent_slot, child_slot)` 将"父子 slot 不同"这一安全性前提从隐式依赖（PM 语义）提升为运行时显式断言，防止同一 slot 产生两个 `&mut VmProc` 别名导致 UB
- **typestate 保证状态安全**：父进程通过 `ActiveProc` 视图读取 regions/page_table，子进程通过 `EmptySlot` → `ActiveProc` 视图逐步初始化，状态转换由类型系统强制，不可能在未初始化的 slot 上执行 `ActiveProc` 操作
- **无需跨 slot 联合方法**：父子操作是顺序的（先从 parent 读，再写到 child），各自在独立 view 上完成，不需要 `get_two_active` 之类的聚合接口

---

## 4. 实现详解

### 4.1 消息处理入口

dispatcher 从 IPC 消息解码出 `VmForkIn`，调用 `do_fork` 编排 fork 流程。

> **注意**: `sys_fork` 当前为 stub（假设成功），待内核 IPC 实现后替换。

```rust
// dispatcher 收到 VM_FORK 后的调用路径
fn dispatch_fork(table: &VmProcTable, _page_alloc: &mut VmPageAllocator, frames: &mut PageFrames, request: VmForkIn) -> VmReply {
    match fork::do_fork(table, frames, request.parent_endpoint, request.child_slot) {
        Ok(child_endpoint) => VmReply::Fork(VmForkOut { child_endpoint }),
        Err(ForkError::InvalidEndpoint) => VmReply::Error(VmError::InvalidEndpoint),
        Err(ForkError::InvalidSlot) | Err(ForkError::SlotInUse) => VmReply::Error(VmError::SlotInUse),
        Err(ForkError::NoMemory) => VmReply::Error(VmError::OutOfMemory),
        // ... 其他错误映射
    }
}
```

dispatcher 只做消息解码和错误映射，核心逻辑在 `fork::do_fork`（§4.2）。

### 4.2 do_fork 编排

`do_fork` 是 fork 的核心编排函数，对应 Minix3 的 `do_fork()`。通过 `VmProcTable` 的 typestate 视图获取父子进程的独立访问。

```rust
pub(crate) fn do_fork(
    table: &VmProcTable,
    frames: &mut PageFrames,
    parent_endpoint: Endpoint,   // PM 传入的父进程 endpoint
    child_slot: UserSlot,        // PM 分配的子进程 slot
) -> Result<Endpoint, ForkError> {
    // 阶段1: 验证父进程 — endpoint → slot → ActiveProc 视图
    let parent_slot = table.vm_isokendpt(parent_endpoint)
        .map_err(|_| ForkError::InvalidEndpoint)?;
    let parent = table.get_active(parent_slot)
        .ok_or(ForkError::InvalidSlot)?;

    // 安全性不变量：父子 slot 必须不同，否则同一 slot 产生两个 &mut VmProc 别名
    assert_ne!(parent_slot, child_slot);

    // 阶段1: 验证子进程 slot — EmptySlot 视图
    let empty = table.get_empty(child_slot)
        .ok_or(ForkError::SlotInUse)?;

    // 阶段2: 初始化子进程（EmptySlot → ActiveProc 状态转换）
    let mut child = empty.activate_relaxed(Endpoint::NONE);
    child.init_from_fork(Endpoint::NONE, parent.total(), parent.total_max(), parent.region_top());
    child.copy_acl_from(&parent);

    // 阶段3: 创建子进程页表
    child.init_page_table().map_err(|_| ForkError::NoMemory)?;
    child.init_regions();

    // 阶段4: 复制地址空间（详见 §4.4）
    let parent_regions: Vec<&VirRegion> = parent.regions().iter().collect();
    let dst_regions = match fork_regions(&parent_regions, frames) {
        Ok(r) => r,
        Err(e) => {
            unsafe { child.free_page_table(); }  // 回滚：释放页表（对应 Minix3 pt_free）
            return Err(e);
        }
    };
    for region in dst_regions {
        child.regions_mut().insert(*region);
    }

    // 阶段5: CoW 设置 + 页表写入
    unsafe { child.setup_cow_for_all_regions(frames); }
    unsafe { child.write_page_table_mappings(frames); }

    // 阶段6: 通知内核（详见 §4.5）
    let child_endpoint = sys_fork(parent.endpoint(), child.slot());
    child.set_endpoint(child_endpoint);

    // 阶段7: 绑定页表（详见 §4.6）— 不可回滚，失败时 panic
    child.bind_page_table()
        .expect("pt_bind failed after sys_fork — irrecoverable");

    // TODO: handle_memory_once — 通知内核 fork 消息页面的内存映射

    Ok(child_endpoint)
}
```

**关键设计点**：

- `parent` 和 `child` 是不同 slot 的 typestate 视图，`AssumeSyncCell` 保证它们可以同时存在（§3.5）
- `empty.activate_relaxed(Endpoint::NONE)` 完成 `EmptySlot` → `ActiveProc` 的状态转换，之后 `child` 只能调用 `ActiveProc` 的方法
- `init_from_fork()` 逐字段拷贝（而非 Minix3 的 `*vmc = *vmp` 整结构体拷贝后恢复），避免"拷贝后恢复"的脆弱模式

### 4.3 进程表操作

fork 使用的三个 `VmProcTable` 方法，对应 Minix3 的验证和访问逻辑：

| 方法 | 对应 Minix3 | 返回类型 | 说明 |
|------|------------|---------|------|
| `vm_isokendpt(endpoint)` | `vm_isokendpt()` | `Result<UserSlot, EndptError>` | endpoint → slot 映射，验证有效性 |
| `get_active(slot)` | `&vmproc[proc]` + flags 检查 | `Option<ActiveProc>` | 获取活跃进程视图（`IN_USE` + 非 `EXITING`） |
| `get_empty(slot)` | `&vmproc[childproc]` + 范围检查 | `Option<EmptySlot>` | 获取空槽位视图（非 `IN_USE`）。**注意**：Minix3 的 `do_fork` 不检查子进程 slot 是否空闲（依赖 PM 保证 slot 正确），Rust 通过 `get_empty()` 显式验证 `IN_USE` 标志 |

Minix3 直接通过 `&vmproc[proc]` 获取裸指针，无状态检查。Rust 通过 typestate 视图在编译时保证：不可能对空 slot 执行 `ActiveProc` 操作，不可能对活跃 slot 执行 `EmptySlot` 操作。

### 4.4 区域遍历与复制

遍历父进程的区域列表，复制每个虚拟区域到子进程。

**fork_region** — 复制单个虚拟区域

```rust
// 复制单个虚拟区域（对应 Minix3 map_copy_region）
pub(crate) fn fork_region(
    src: &VirRegion,          // 源区域（父进程）
    frames: &mut PageFrames,  // 全局物理页帧管理器
) -> Result<Box<VirRegion>, ForkError>  // 返回新区域或错误
```

对应 Minix3 的 `map_copy_region()`。核心步骤：

1. **创建新区域**：`VirRegion::new()` 复制 `vaddr`、`length`、`flags`、`memtype` 等字段（含 `parent_slot`，此时仍指向父进程 slot，后续由 `region_insert` 更新）
2. **遍历 physblocks**：对源区域每个已分配的 `PageSlot`，递增 `frames.get_mut(slot.pfn).refcount` 并复制 `PageSlot` 到新区域（Copy 语义）
3. **ev_reference 回调**：对每个已共享的物理页调用 `memtype.ev_reference()(frames, slot)`，失败时回滚已递增的 refcount（遍历 `refcounted_pfns` 向量逐个递减）
4. **设置不可写**：`dst.set_writable(false)` 标记 CoW

关键差异：
- 使用 `PageSlot` Copy 语义替代 `pb_reference()` + `pb_link()`
- `ev_reference` 失败时自动回滚已递增的 refcount
- 复制后设置 `dst.set_writable(false)` 实现 CoW

**fork_regions** — 复制所有虚拟区域

```rust
// 复制所有虚拟区域（对应 Minix3 map_proc_copy）
pub(crate) fn fork_regions(
    src_regions: &[&VirRegion],  // 父进程区域引用列表
    frames: &mut PageFrames,     // 全局物理页帧管理器
) -> Result<Vec<Box<VirRegion>>, ForkError>  // 返回新区域列表或错误
```

对应 Minix3 的 `map_proc_copy()`。遍历源区域列表，对每个区域调用 `fork_region()`。失败时自动调用 `free_forked_regions` 回滚。

**cow_copy_page** — CoW 单页复制

> **注意**：`cow_copy_page` 是页错误处理时调用的辅助函数，**不在 fork 主流程中**。fork 时仅设置 CoW（共享页 + 只读），实际页面复制在写入触发页错误后由 `cow_resolve_core` 完成。

```rust
// CoW 单页复制（对应 Minix3 mem_cow）
pub(crate) fn cow_copy_page(
    region: &mut VirRegion,       // 目标区域（CoW 触发页所在区域）
    frames: &mut PageFrames,      // 全局物理页帧管理器
    alloc: &mut dyn PfnAllocator, // 物理页分配器
    offset: VirBytes,             // 页内偏移量
) -> Result<(), ForkError>        // 成功或错误
```

对应 Minix3 的 `mem_cow()`。是 `cow_resolve_core()` 的薄封装，将 `CowCoreError` 映射为 `ForkError`。

**与 Minix3 的关键差异**

| Minix3 | Rust 实现 | 说明 |
|--------|-------|------|
| `pb_reference(ph->ph, ph->offset, newvr, vr->def_memtype)` | `frames.get_mut(slot.pfn).refcount += 1` + `new_region.physblocks[idx] = Some(slot)` | 直接递增 refcount + 复制 PageSlot，无需创建 PhysRegion/PhysBlock 对象 |
| `ph->memtype->ev_reference(ph, newph)` | `memtype.ev_reference()(frames, slot)` | MemType 回调签名变更：`PhysRegion` → `PageSlot + PageFrames` |
| `phys_region.phys_block().refcount()` | `frames.get(slot.pfn).refcount` | refcount 从 PhysBlock 移至 PageFrames 全局数组 |
| `phys_region.phys_block().phys()` | `frames.pfn_to_phys(slot.pfn)` | 物理地址通过 PFN 间接获取 |
| `pr_writable(vr, pr)` | `slot.memtype.unwrap().writable(frames, slot)` | 可写判断统一到 MemType trait |

> Minix3 的 `map_copy_region` 需要 `pb_reference()` → `pb_link()` 创建新的 `phys_region` 并链入 `phys_block.firstregion` 侵入式链表。Rust 实现只需递增 `PageFrames.states[pfn].refcount` 并复制 `PageSlot`（Copy 类型），省去了堆分配和链表操作。

### 4.5 内核通知

`sys_fork` 通知内核创建子进程的调度实体。实现采用 `#[cfg(test)]`/`#[cfg(not(test))]` 分版本：测试中返回一个有效的假 endpoint，生产代码中使用 `todo!()` 显式标记未实现。

```rust
// 通知内核创建子进程
// 测试版本：返回有效的假 endpoint 供 fork 测试使用
#[cfg(test)]
fn sys_fork(_parent_endpoint: Endpoint, child_slot: UserSlot) -> Endpoint {
    Endpoint::from_generation_slot(1, child_slot.get() as i32)
}

// 生产版本：标记为未实现（发送 SYS_FORK 消息给内核）
#[cfg(not(test))]
fn sys_fork(_parent_endpoint: Endpoint, _child_slot: UserSlot) -> Endpoint {
    todo!("sys_fork: send SYS_FORK message to kernel and receive child endpoint")
}
```

> 内核侧 `sys_fork` 的完整处理流程（消息字段、proc 结构体复制、调度状态设置、endpoint 生成方式）见 §2.7.3。

**不可回滚**：`sys_fork` 成功后内核已创建子进程，系统状态不可逆，因此失败时 panic。这与 Minix3 的 `panic("VM: do_fork can't sys_fork")` 语义一致。分版本的原因：测试版本需要 `sys_fork` 返回有效的 endpoint 来验证后续 `pt_bind` 和 CoW 逻辑；生产版本使用 `todo!()` 在运行时 panic 而非静默返回错误值。

### 4.6 页表绑定

`bind_page_table` 将子进程的页表物理地址注册到内核，使内核在调度子进程时切换到正确的页表。

```rust
// ActiveProc 方法，将页表绑定到子进程（stub，TODO: 实现内核 IPC）
fn bind_page_table(&self) -> Result<(), PageTableError> {
    // TODO: 调用 sys_vmctl(VMCTL_SET_PAGE_DIR, endpoint, pt_phys)
    // 通知内核将子进程的页目录物理地址写入进程结构
    Ok(())
}
```

对应 Minix3 的 `pt_bind(&vmc->vm_pt, vmc)`，最终调用 `sys_vmctl(VMCTL_SET_PAGE_DIR, endpoint, pt_phys)`。当前为 stub 实现，假设成功。

**不可回滚**：`bind_page_table` 在 `sys_fork` 成功后调用，此时内核已创建子进程，绑定失败不可恢复。`do_fork` 中使用 `.expect()` 而非 `?` 处理此错误，与 Minix3 的 `panic("fork can't pt_bind")` 语义一致。

### 4.7 错误处理与回滚

fork 的错误回滚分两层（§3.4 有设计要点，此处为实现细节）：

**第一层：`fork_regions` 内部回滚**

```rust
pub(crate) fn fork_regions(
    src_regions: &[&VirRegion],  // 父进程区域引用列表
    frames: &mut PageFrames,     // 全局物理页帧管理器
) -> Result<Vec<VirRegion>, ForkError> {
    let mut dst_regions = Vec::with_capacity(src_regions.len()); // 预分配容量
    for src in src_regions {
        match fork_region(src, frames) {
            Ok(dst) => dst_regions.push(dst), // 成功：加入结果列表
            Err(e) => {
                // 任一区域复制失败，回滚已成功的所有区域
                free_forked_regions(&mut dst_regions, frames);
                return Err(e);
            }
        }
    }
    Ok(dst_regions)
}
```

`free_forked_regions` 遍历已复制区域的 `physblocks`，递减 `PageFrames.states[pfn].refcount` 并调用 `ev_unreference`。对应 Minix3 的 `map_free_proc` → `map_free` → `pb_unreferenced` 链路，但无需侵入式链表操作。

**第二层：`do_fork` 回滚**

`fork_regions` 返回 `Err` 时，`do_fork` 调用 `child.free_page_table()` 释放已创建的子进程页表（对应 Minix3 的 `pt_free(&vmc->vm_pt)`）。`sys_fork` 成功后不存在回滚路径。

| 阶段 | 失败条件 | 处理 |
|------|---------|------|
| 参数验证 | endpoint 无效或 slot 越界 | 直接返回错误码，无副作用 |
| 页表创建 | `init_page_table()` 失败 | 返回 `NoMemory`，子进程结构未修改 |
| 区域复制 | `fork_regions()` 失败 | `free_forked_regions` 递减 refcount + `free_page_table()` 释放页表 |
| 内核通知 | `sys_fork()` 失败 | panic（不可回滚） |
| 页表绑定 | `bind_page_table()` 失败 | panic（不可回滚） |

---

## 5. 测试要点

### 5.1 测试维度

| 维度 | 测试内容 |
|------|---------|
| **参数验证** | 无效 endpoint（不存在、未使用、过期）、无效 slot（越界、负数） |
| **子进程初始化** | 浅拷贝后字段恢复（slot、endpoint、regions、pagetable）正确性 |
| **页表创建** | `pt_new()` 成功/失败路径、内核空间映射正确性 |
| **区域复制** | `map_copy_region()` PageSlot 共享（refcount 递增）、`map_proc_copy()` 遍历完整性 |
| **CoW 设置** | fork 后父子进程页表均为只读、refcount > 1 的页不可写 |
| **内核通知** | `sys_fork()` 失败时 panic（不可恢复）、`pt_bind()` 绑定正确性 |
| **错误处理** | 各阶段失败回滚：pt_new 失败无副作用、map_proc_copy 失败释放页表 |
| **ACL 继承** | USER_ACL 继承、系统 ACL 清除为 NO_ACL |

### 5.2 关键测试场景

1. **正常 fork 流程**：父进程有多个区域（code、data、stack），fork 后子进程区域数量和属性一致，PageSlot 共享且 `PageFrames.states[pfn].refcount` 正确
2. **CoW 触发**：fork 后子进程写入共享页，验证物理页分离、refcount 降为 1、页表恢复可写
3. **CoW 只读共享**：fork 后父子进程读取共享页不触发 CoW，物理页不变
4. **内存不足**：模拟 `pt_new()` 或 `map_copy_region()` 失败，验证回滚正确、父进程状态不变
5. **sys_fork 不可逆**：验证 `sys_fork()` 成功后后续步骤失败时 panic 而非返回错误
6. **handle_memory_once**：fork 消息页面的可写处理，父子进程都需调用

---

## 6. 参见

- [01-vmproc-struct](01-vmproc-struct.md) - vmproc 结构定义
- [06-pagetable-struct](06-pagetable-struct.md) - 页表结构定义
- [07-pagetable-ops](07-pagetable-ops.md) - 页表操作（pt_new、pt_bind）
- [10-phys-pagestate.md](10-phys-pagestate.md) - 物理页状态与引用计数
- [11-region-mapping.md](11-region-mapping.md) - 虚拟区域与页映射
- [13-region-avl](13-region-avl.md) - 虚拟区域 AVL 树
- [14-cow-mechanism](14-cow-mechanism.md) - CoW 机制详解（pr_writable、anon_writable、页错误处理）
- [15-pagefault](15-pagefault.md) - 页错误处理
- [03-acl](03-acl.md) - ACL 机制