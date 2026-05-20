# 16-vm-fork: VM_FORK 服务

> **分类**: VM服务  
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
5. 复制地址空间：`map_proc_copy(vmc, vmp)` → `map_proc_copy_range()` 遍历每个区域调用 `map_copy_region()`，最后调用 `map_writept(src)` 和 `map_writept(dst)` 更新父子进程页表
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

> 以下为 Minix3 x86-32 源码中的实际定义，详细字段说明参见各专题文档。

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

### 2.3 请求参数

#### 2.3.1 VMF_ENDPOINT

父进程 endpoint，用于标识要 fork 的源进程。VM 通过此 endpoint 查找父进程的 VM 结构。

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

```c
// 验证 endpoint 是否有效 (utility.c:84)
int vm_isokendpt(endpoint_t endpoint, int *procn)
{
    *procn = _ENDPOINT_P(endpoint);
    if(*procn < 0 || *procn >= NR_PROCS)
        return EINVAL;
    if(*procn >= 0 && endpoint != vmproc[*procn].vm_endpoint)
        return EDEADEPT;
    if(*procn >= 0 && !(vmproc[*procn].vm_flags & VMF_INUSE))
        return EDEADEPT;
    return OK;
}
```

> **注意**：endpoint 不匹配或进程未使用时返回 `EDEADEPT`（而非 `EINVAL`），表示端点已过期。

#### 2.3.2 VMF_SLOTNO

子进程槽位号（由 PM 分配）。PM 在进程表中为子进程预留了一个 slot，VM 使用此 slot 初始化子进程的 VM 结构。

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
| 页表创建 | `pt_new()` 失败 | `ENOMEM` | 直接返回，子进程结构未修改 |
| 地址空间复制 | `map_proc_copy()` 失败 | `ENOMEM` | `pt_free(&vmc->vm_pt)` 释放页表后返回 |
| 内核通知 | `sys_fork()` 失败 | panic | 不可恢复：内核已创建子进程，VM 必须成功 |

### 2.5 do_fork - 主处理函数

fork 的完整流程包括：验证参数、初始化子进程结构、复制地址空间、设置 CoW。

**函数原型**

```c
// minix3/minix/servers/vm/fork.c:32
int do_fork(message *msg);
```

**Minix3 源码实现**

```c
int do_fork(message *msg)
{
    int r, proc, childproc;
    struct vmproc *vmp, *vmc;
    pt_t origpt;
    vir_bytes msgaddr;

    SANITYCHECK(SCL_FUNCTIONS);

    // ========== 阶段 1: 参数验证 ==========
    
    // 1.1 验证父进程 endpoint
    if(vm_isokendpt(msg->VMF_ENDPOINT, &proc) != OK) {
        printf("VM: bogus endpoint VM_FORK %d\n", msg->VMF_ENDPOINT);
        SANITYCHECK(SCL_FUNCTIONS);
        return EINVAL;
    }

    // 1.2 验证子进程 slot
    childproc = msg->VMF_SLOTNO;
    if(childproc < 0 || childproc >= NR_PROCS) {
        printf("VM: bogus slotno VM_FORK %d\n", msg->VMF_SLOTNO);
        SANITYCHECK(SCL_FUNCTIONS);
        return EINVAL;
    }

    vmp = &vmproc[proc];      // 父进程
    vmc = &vmproc[childproc]; // 子进程
    assert(vmc->vm_slot == childproc);

    // ========== 阶段 2: 初始化子进程结构 ==========
    
    // 2.1 保存原始页表指针
    origpt = vmc->vm_pt;
    
    // 2.2 复制父进程结构（浅拷贝）
    *vmc = *vmp;
    
    // 2.3 恢复子进程特有字段
    vmc->vm_slot = childproc;
    region_init(&vmc->vm_regions_avl);  // 初始化空的区域树
    vmc->vm_endpoint = NONE;            // 暂时无效
    vmc->vm_pt = origpt;                // 恢复原始页表

#if VMSTATS
    vmc->vm_bytecopies = 0;
#endif

    // ========== 阶段 3: 创建页表 ==========
    
    if(pt_new(&vmc->vm_pt) != OK) {
        return ENOMEM;
    }

    SANITYCHECK(SCL_DETAIL);

    // ========== 阶段 4: 复制地址空间 ==========
    
    if(map_proc_copy(vmc, vmp) != OK) {
        printf("VM: fork: map_proc_copy failed\n");
        pt_free(&vmc->vm_pt);
        return ENOMEM;
    }

    // ========== 阶段 5: 设置进程标志和 ACL ==========
    
    // 只继承 VMF_INUSE 标志
    vmc->vm_flags &= VMF_INUSE;
    
    // 处理 ACL 继承
    acl_fork(vmc);

    // ========== 阶段 6: 通知内核 ==========
    
    // 6.1 调用 sys_fork 创建内核进程
    if((r = sys_fork(vmp->vm_endpoint, childproc,
            &vmc->vm_endpoint, PFF_VMINHIBIT, &msgaddr)) != OK) {
        panic("do_fork can't sys_fork: %d", r);
    }

    // 6.2 绑定页表到进程
    if((r = pt_bind(&vmc->vm_pt, vmc)) != OK)
        panic("fork can't pt_bind: %d", r);

    // 6.3 处理 fork 消息页面的内存访问
    {
        vir_bytes vir;
        vir = msgaddr;
        if (handle_memory_once(vmc, vir, sizeof(message), 1) != OK)
            panic("do_fork: handle_memory for child failed\n");
        vir = msgaddr;
        if (handle_memory_once(vmp, vir, sizeof(message), 1) != OK)
            panic("do_fork: handle_memory for parent failed\n");
    }

    // ========== 阶段 7: 返回结果 ==========
    
    msg->VMF_CHILD_ENDPOINT = vmc->vm_endpoint;

    SANITYCHECK(SCL_FUNCTIONS);
    return OK;
}
```

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

// 3. 验证 slot 一致性
assert(vmc->vm_slot == childproc);
```

### 2.7 初始化阶段

#### 2.7.1 分配子进程结构

使用 PM 提供的 slot 初始化子进程的 vmproc 结构，设置基本标志。

**初始化步骤**

1. **保存原始页表**：`origpt = vmc->vm_pt`（子进程 slot 原有的页表）
2. **浅拷贝父进程结构**：`*vmc = *vmp`（此时 vmc 暂时与 vmp 共享区域树指针、页表指针、标志等）
3. **恢复子进程特有字段**：
   - `vmc->vm_slot = childproc`（恢复自己的 slot）
   - `region_init(&vmc->vm_regions_avl)`（初始化空区域树）
   - `vmc->vm_endpoint = NONE`（暂时无效，等待内核分配）
   - `vmc->vm_pt = origpt`（恢复原始页表，等待 `pt_new()`）

初始化完成后子进程状态：`vm_endpoint=NONE`，`vm_regions_avl=空`，`vm_pt=原始页表`，`vm_flags` 从父进程复制（后续 `&= VMF_INUSE`），`vm_slot=childproc`。

#### 2.7.2 创建页表

调用 `pt_new()` 为子进程创建新的页表结构。

**pt_new 函数**

```c
// minix3/minix/servers/vm/pagetable.c:990
int pt_new(pt_t *pt)
{
    int i, r;

    // 1. 分配页目录（如果尚未分配）
    if(!pt->pt_dir &&
        !(pt->pt_dir = vm_allocpages((phys_bytes *)&pt->pt_dir_phys,
            VMP_PAGEDIR, ARCH_PAGEDIR_SIZE/VM_PAGE_SIZE))) {
        return ENOMEM;
    }

    // 2. 验证页目录物理地址对齐
    assert(!((u32_t)pt->pt_dir_phys % ARCH_PAGEDIR_SIZE));

    // 3. 初始化页目录项
    for(i = 0; i < ARCH_VM_DIR_ENTRIES; i++) {
        pt->pt_dir[i] = 0;  // 无效条目 (PRESENT 位 = 0)
        pt->pt_pt[i] = NULL;
    }

    // 4. 初始化虚拟地址分配起点
    pt->pt_virtop = 0;

    // 5. 映射内核空间
    if((r = pt_mapkernel(pt)) != OK)
        return r;

    return OK;
}
```

**页表结构**

```
┌─────────────────────────────────────────────────────────────────┐
│                    页表结构                                      │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   pt_t 结构:                                                    │
│   ┌─────────────────────────────────────────────────────┐      │
│   │ pt_dir:        页目录虚拟地址                        │      │
│   │ pt_dir_phys:   页目录物理地址                        │      │
│   │ pt_pt[]:       页表指针数组                          │      │
│   │ pt_virtop:     虚拟地址分配起点                       │      │
│   └─────────────────────────────────────────────────────┘      │
│                                                                 │
│   x86 两级页表结构:                                              │
│   ┌─────────────────────────────────────────────────────┐      │
│   │                                                     │      │
│   │   页目录 (Page Directory)                           │      │
│   │   ┌─────────────────────────────────────┐           │      │
│   │   │ PDE[0] ──► 页表 0                    │           │      │
│   │   │ PDE[1] ──► 页表 1                    │           │      │
│   │   │ ...                                 │           │      │
│   │   │ PDE[1023] ──► 内核页表               │           │      │
│   │   └─────────────────────────────────────┘           │      │
│   │         │                                           │      │
│   │         ▼                                           │      │
│   │   页表 (Page Table)                                 │      │
│   │   ┌─────────────────────────────────────┐           │      │
│   │   │ PTE[0] ──► 物理页 0                  │           │      │
│   │   │ PTE[1] ──► 物理页 1                  │           │      │
│   │   │ ...                                 │           │      │
│   │   │ PTE[1023] ──► 物理页 1023            │           │      │
│   │   └─────────────────────────────────────┘           │      │
│   │                                                     │      │
│   └─────────────────────────────────────────────────────┘      │
│                                                                 │
│   地址转换:                                                     │
│   虚拟地址: [31:22] 页目录索引 | [21:12] 页表索引 | [11:0] 偏移  │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

> **32 位 vs 64 位差异**：以上为 x86-32 两级页表（1024 个 PDE，每个 PDE 指向 1024 项页表，寻址 4GB）。
> x86-64 使用四级页表（PML4 → PDPT → PD → PT），虚拟地址 48 位，PTE 从 32 位扩展为 64 位。
> `pt_pt[]` 数组在 64 位下无法预分配所有页表，需改为动态分配。

#### 2.7.3 调用 sys_fork

内核生成子进程 endpoint，完成进程表中的注册。

**sys_fork 调用**

```c
// 调用内核的 sys_fork
if((r = sys_fork(vmp->vm_endpoint, childproc,
        &vmc->vm_endpoint, PFF_VMINHIBIT, &msgaddr)) != OK) {
    panic("do_fork can't sys_fork: %d", r);
}
```

**参数说明**

| 参数 | 说明 |
|------|------|
| `vmp->vm_endpoint` | 父进程 endpoint |
| `childproc` | 子进程 slot 号 |
| `&vmc->vm_endpoint` | 输出：子进程 endpoint |
| `PFF_VMINHIBIT` | fork 标志：VM 禁止标志 |
| `&msgaddr` | 输出：fork 消息地址 |

**关键点**

1. **不可逆性**：`sys_fork()` 成功后内核已创建子进程，VM 必须成功，否则系统状态不一致。因此 `sys_fork()` 失败时 panic，而不是返回错误
2. **PFF_VMINHIBIT 标志**：表示子进程的内存管理由 VM 负责，内核不会自动复制父进程的内存映射。这是 Minix3 微内核架构的特点
3. **fork 消息地址**：内核返回 fork 消息的地址，VM 需要处理这个消息页面的内存访问（`handle_memory_once`）

### 2.8 内存复制阶段

#### 2.8.1 map_proc_copy

遍历父进程的所有虚拟区域，为每个区域创建副本。

**函数原型**

```c
// minix3/minix/servers/vm/region.c:933
int map_proc_copy(struct vmproc *dst, struct vmproc *src);
```

**Minix3 源码实现**

```c
int map_proc_copy(struct vmproc *dst, struct vmproc *src)
{
    // 初始化目标进程的区域树
    region_init(&dst->vm_regions_avl);

    // 调用范围复制函数
    return map_proc_copy_range(dst, src, NULL, NULL);
}
```

#### 2.8.2 map_proc_copy_range

复制指定范围内的虚拟区域。

**函数原型**

```c
int map_proc_copy_range(struct vmproc *dst, struct vmproc *src,
    struct vir_region *start_src_vr, struct vir_region *end_src_vr);
```

**Minix3 源码实现**

```c
int map_proc_copy_range(struct vmproc *dst, struct vmproc *src,
    struct vir_region *start_src_vr, struct vir_region *end_src_vr)
{
    struct vir_region *vr;
    region_iter v_iter;

    // 1. 确定复制范围
    if(!start_src_vr)
        start_src_vr = region_search_least(&src->vm_regions_avl);
    if(!end_src_vr)
        end_src_vr = region_search_greatest(&src->vm_regions_avl);

    assert(start_src_vr && end_src_vr);
    assert(start_src_vr->parent == src);

    // 2. 初始化迭代器
    region_start_iter(&src->vm_regions_avl, &v_iter,
        start_src_vr->vaddr, AVL_EQUAL);
    assert(region_get_iter(&v_iter) == start_src_vr);

    SANITYCHECK(SCL_FUNCTIONS);

    // 3. 遍历并复制每个区域
    while((vr = region_get_iter(&v_iter))) {
        struct vir_region *newvr;
        
        // 3.1 复制单个区域
        if(!(newvr = map_copy_region(dst, vr))) {
            map_free_proc(dst);
            return ENOMEM;
        }
        
        // 3.2 插入到目标进程的区域树
        region_insert(&dst->vm_regions_avl, newvr);
        assert(vr->length == newvr->length);

#if SANITYCHECKS
        // 验证物理块共享正确
        {
            vir_bytes vaddr;
            struct phys_region *orig_ph, *new_ph;
            assert(vr->physblocks != newvr->physblocks);
            for(vaddr = 0; vaddr < vr->length; vaddr += VM_PAGE_SIZE) {
                orig_ph = physblock_get(vr, vaddr);
                new_ph = physblock_get(newvr, vaddr);
                if(!orig_ph) { assert(!new_ph); continue; }
                assert(new_ph);
                assert(orig_ph != new_ph);        // 不同的 phys_region
                assert(orig_ph->ph == new_ph->ph); // 相同的 phys_block
            }
        }
#endif

        // 3.3 检查是否到达结束区域
        if(vr == end_src_vr) {
            break;
        }
        region_incr_iter(&v_iter);
    }

    // 4. 更新页表
    map_writept(src);
    map_writept(dst);

    SANITYCHECK(SCL_FUNCTIONS);
    return OK;
}
```

**处理流程图**

```
┌─────────────────────────────────────────────────────────────────┐
│                    map_proc_copy_range 流程                      │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   输入: dst, src, start_vr, end_vr                              │
│         │                                                       │
│         ▼                                                       │
│   ┌──────────────────┐                                          │
│   │ 确定复制范围     │                                          │
│   │ start_vr = least │                                          │
│   │ end_vr = greatest│                                          │
│   └──────────────────┘                                          │
│         │                                                       │
│         ▼                                                       │
│   ┌──────────────────┐                                          │
│   │ 初始化迭代器     │                                          │
│   └──────────────────┘                                          │
│         │                                                       │
│         ▼                                                       │
│   ┌──────────────────────────────────────────────────────────┐  │
│   │ 循环: 遍历每个区域                                        │  │
│   │                                                          │  │
│   │   vr = region_get_iter(&v_iter)                          │  │
│   │         │                                                │  │
│   │         ▼                                                │  │
│   │   ┌──────────────────┐                                   │  │
│   │   │ map_copy_region()│                                   │  │
│   │   └──────────────────┘                                   │  │
│   │         │                                                │  │
│   │         ├─► 失败 ──► map_free_proc(dst) ──► return ENOMEM│  │
│   │         │                                                │  │
│   │         ▼                                                │  │
│   │   region_insert(&dst->vm_regions_avl, newvr)             │  │
│   │         │                                                │  │
│   │         ▼                                                │  │
│   │   vr == end_vr? ──► 是 ──► break                         │  │
│   │         │                                                │  │
│   │         否                                               │  │
│   │         ▼                                                │  │
│   │   region_incr_iter(&v_iter)                              │  │
│   │                                                          │  │
│   └──────────────────────────────────────────────────────────┘  │
│         │                                                       │
│         ▼                                                       │
│   ┌──────────────────┐                                          │
│   │ map_writept(src) │  更新父进程页表（设置只读）              │
│   └──────────────────┘                                          │
│         │                                                       │
│         ▼                                                       │
│   ┌──────────────────┐                                          │
│   │ map_writept(dst) │  更新子进程页表（设置只读）              │
│   └──────────────────┘                                          │
│         │                                                       │
│         ▼                                                       │
│   return OK                                                     │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

#### 2.8.3 map_copy_region

复制单个虚拟区域，包括创建新的 vir_region 结构和共享物理块。

**函数原型**

```c
struct vir_region *map_copy_region(struct vmproc *vmp, struct vir_region *vr);
```

**Minix3 源码实现**

```c
struct vir_region *map_copy_region(struct vmproc *vmp, struct vir_region *vr)
{
    struct vir_region *newvr;
    struct phys_region *ph;
    int r;
    vir_bytes p;

    // 1. 创建新的虚拟区域结构
    if(!(newvr = region_new(vr->parent, vr->vaddr, vr->length, 
            vr->flags, vr->def_memtype)))
        return NULL;

    // 2. 设置父进程为子进程
    USE(newvr, newvr->parent = vmp;);

    // 3. 调用内存类型的复制回调（如果有）
    if(vr->def_memtype->ev_copy && 
       (r = vr->def_memtype->ev_copy(vr, newvr)) != OK) {
        map_free(newvr);
        printf("VM: memtype-specific copy failed (%d)\n", r);
        return NULL;
    }

    // 4. 遍历所有物理块，共享引用
    for(p = 0; p < phys_slot(vr->length); p++) {
        struct phys_region *newph;

        // 4.1 获取源物理区域
        if(!(ph = physblock_get(vr, p*VM_PAGE_SIZE))) continue;
        
        // 4.2 创建新的物理区域，共享物理块
        newph = pb_reference(ph->ph, ph->offset, newvr, vr->def_memtype);

        if(!newph) { 
            map_free(newvr); 
            return NULL; 
        }

        // 4.3 调用内存类型的引用回调
        if(ph->memtype->ev_reference)
            ph->memtype->ev_reference(ph, newph);
    }

    return newvr;
}
```

**区域复制详解**

`map_copy_region()` 的核心操作：

1. `region_new()` 创建新 `vir_region`，复制 `vaddr`、`length`、`flags`、`def_memtype`
2. 遍历源区域的 `physblocks` 数组（`physblock_get(vr, p*VM_PAGE_SIZE)`），跳过未分配的槽位
3. 对每个已分配的 `phys_region`，调用 `pb_reference(ph->ph, ph->offset, newvr, vr->def_memtype)` 共享物理块（`refcount++`）
4. 调用 `memtype->ev_reference()` 回调（如有）

复制后父子进程的 `vir_region` 指向相同的 `phys_block`，`refcount` 从 1 增至 2。

#### 2.8.4 CoW 设置

共享 phys_block 后，将页表项标记为只读，触发写时复制。

**map_writept 函数**

```c
// minix3/minix/servers/vm/region.c:906
int map_writept(struct vmproc *vmp)
{
    struct vir_region *vr;
    struct phys_region *ph;
    int r;
    region_iter v_iter;
    region_start_iter_least(&vmp->vm_regions_avl, &v_iter);

    while((vr = region_get_iter(&v_iter))) {
        vir_bytes p;
        for(p = 0; p < vr->length; p += VM_PAGE_SIZE) {
            if(!(ph = physblock_get(vr, p))) continue;

            if((r=map_ph_writept(vmp, vr, ph)) != OK) {
                printf("VM: map_writept: failed\n");
                return r;
            }
        }
        region_incr_iter(&v_iter);
    }

    return OK;
}
```

**map_ph_writept 函数 — 实际的页表标志计算**

```c
// minix3/minix/servers/vm/region.c:257
int map_ph_writept(struct vmproc *vmp, struct vir_region *vr,
    struct phys_region *pr)
{
    int flags = PTF_PRESENT | PTF_USER;
    struct phys_block *pb = pr->ph;

    if(pr_writable(vr, pr))
        flags |= PTF_WRITE;
    else
        flags |= PTF_READ;

    if(vr->def_memtype->pt_flags)
        flags |= vr->def_memtype->pt_flags(vr);

    if(pt_writemap(vmp, &vmp->vm_pt, vr->vaddr + pr->offset,
            pb->phys, VM_PAGE_SIZE, flags,
#if SANITYCHECKS
            !pr->written ? 0 :
#endif
            WMF_OVERWRITE) != OK) {
        printf("VM: map_writept: pt_writemap failed\n");
        return ENOMEM;
    }

#if SANITYCHECKS
    USE(pr, pr->written = 1;);
#endif
    return OK;
}
```

**pr_writable 函数 — 可写判断的核心逻辑**

```c
// minix3/minix/servers/vm/region.c:130
static int pr_writable(struct vir_region *vr, struct phys_region *pr)
{
    assert(pr->memtype->writable);
    return ((vr->flags & VR_WRITABLE) && pr->memtype->writable(pr));
}
```

CoW 只读机制通过 `pr_writable()` → `memtype->writable()` 间接实现。对于匿名内存，`anon_writable()` (mem_anon.c:105) 在 `refcount > 1` 时返回 0，从而使页表项不含 `PTF_WRITE`。详见 [14-cow-mechanism](14-cow-mechanism.md)。

**CoW 页表标志**

fork 后的页表状态由 `map_ph_writept()` 通过 `pr_writable()` 自动设置：

| 条件 | pr_writable() | 页表标志 | 说明 |
|------|--------------|---------|------|
| `refcount == 1` 且 `VR_WRITABLE` | 1 | `PTF_PRESENT \| PTF_USER \| PTF_WRITE` | 私有可写页 |
| `refcount > 1` 且 `VR_WRITABLE` | 0 | `PTF_PRESENT \| PTF_USER \| PTF_READ` | 共享页只读（CoW） |
| 非 `VR_WRITABLE` | 0 | `PTF_PRESENT \| PTF_USER \| PTF_READ` | 本身只读页 |

fork 后，父子进程的共享页 `refcount > 1`，`anon_writable()` 返回 0，`pr_writable()` 返回 0，页表项不含 `PTF_WRITE`。写入时触发页错误，CoW 处理分配新物理页后 `refcount` 降为 1，`pr_writable()` 返回 1，页表项恢复 `PTF_WRITE`。

> **方案四标注**：CoW 设置中的 `pt_writemap()` 在方案四下通过 `vm_phys_to_virt()` 直接操作页表页，无需 `createpde` 临时映射窗口。CoW 的逻辑（`refcount > 1` → 只读 → 写入触发页错误 → 分配新页）完全不变——direct map 简化的是"如何写入页表项"这个实现细节，而不是 CoW 的策略逻辑。这是策略/机制分离的一个例证：CoW 策略（何时共享、何时复制）不变，机制（如何写入页表）被简化。

#### 2.8.5 方案四视角：fork 页表创建的终极简化

> **方案四标注**：Direct Map 方案下，fork 创建子进程页表的流程被大幅简化。

**Minix3 的 fork 页表创建**需要 `createpde` 临时映射窗口——VM 无法直接访问物理页，必须请求内核在 VM 的地址空间中临时映射一个物理页，操作完毕后再解除映射：

```
1. alloc_mem() 分配物理页
2. createpde() 建立临时映射窗口
3. 通过临时映射窗口清零页目录
4. pt_mapkernel() 建立内核映射
5. 复制父进程用户空间映射
6. 释放临时映射窗口
```

**方案四的 fork 页表创建**只需 4 步，无需临时映射窗口：

```
1. bitmap.alloc_mem(1)           → 分配新页目录物理页
2. vm_phys_to_virt(dir_phys)     → 清零（物理页天然有 VA）
3. pt_mapkernel(dir_ptr)         → 建立内核映射（含 kernel direct map）
4. 复制父进程的用户空间映射
```

**三种方案的复杂度递减**：

| 方案 | 步骤 | 复杂度来源 |
|------|------|-----------|
| Minix3（createpde） | 分配 → 建临时映射 → 清零 → 建内核映射 → 复制 → 释放临时映射 | VM 无法直接访问物理页 |
| 方案三（PtRegion） | 分配 → 从 PtRegion 分配 VA → 清零 → 建内核映射 → 复制 | 页表页需要特殊 VA 管理 |
| 方案四（Direct Map） | 分配 → `vm_phys_to_virt()` 清零 → 建内核映射 → 复制 | 物理页天然有 VA |

读者应感受到：**direct map 的"终极简化"不是"又少了一步"，而是"间接操作的根源被消除了"**——Minix3 的 `createpde` 和方案三的 PtRegion 都是为了解决"VM 无法直接访问物理页"这个问题，direct map 从根本上消除了这个问题。

**双视图模型在 fork 中的协作**：VM 通过 VM direct map（`vm_phys_to_virt()`）操作物理页（清零、复制），通过 `pt_mapkernel()` 确保新页表包含 kernel direct map。两者在 fork 中协作——VM 用自己的视图操作数据，用内核的视图确保新进程的页表包含内核映射。

### 2.9 完成阶段

#### 2.9.1 设置子进程状态

设置 vm_flags 为 VMF_INUSE，设置 vm_endpoint 为子进程端点。

```c
// 只继承 VMF_INUSE 标志
vmc->vm_flags &= VMF_INUSE;
```

#### 2.9.2 ACL 继承

调用 `acl_fork()` 处理子进程的 ACL 条目。

**acl_fork 函数**

```c
// minix3/minix/servers/vm/acl.c:110
// 参数 vmp 是子进程（vmc），已通过 *vmc = *vmp 继承了父进程的 vm_acl
void acl_fork(struct vmproc *vmp)
{
    if (vmp->vm_acl != USER_ACL)
        vmp->vm_acl = NO_ACL;
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

### 3.1 ForkRequest/ForkResponse

使用类型安全的 IPC 消息结构，包括 ForkRequest 和 ForkResponse 枚举。

```rust
use crate::vm::vmproc::{VmProc, Endpoint};
use crate::vm::region::VirRegion;

/// fork 请求
#[derive(Debug, Clone)]
pub struct ForkRequest {
    /// 父进程端点
    pub parent_endpoint: Endpoint,
    /// 子进程 slot
    pub child_slot: usize,
}

/// fork 响应
#[derive(Debug)]
pub enum ForkResponse {
    /// 成功
    Ok {
        /// 子进程端点
        child_endpoint: Endpoint,
    },
    /// 内存不足
    OutOfMemory,
    /// 参数无效
    InvalidParameter,
}

/// fork 标志
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ForkFlags(u32);

impl ForkFlags {
    pub const VM_INHIBIT: ForkFlags = ForkFlags(0x01);
    
    pub fn contains(&self, other: ForkFlags) -> bool {
        (self.0 & other.0) != 0
    }
}
```

### 3.2 错误处理

使用 Result 类型处理各阶段失败，包括验证失败、内存不足等错误。

```rust
/// fork 错误类型
#[derive(Debug)]
pub enum ForkError {
    InvalidEndpoint(i32),
    InvalidSlot(i32),
    OutOfMemory,
    PageTableError(PageTableError),
    KernelError(i32),
}

/// fork 结果类型
pub type ForkResult<T> = Result<T, ForkError>;
```

> **方案 A 简化**: Minix3 的 fork 错误路径涉及 `pb_new` → `ENOMEM`、`pb_reference` → `ENOMEM`、`alloc_mem` → `NO_MEM` 三条物理内存分配失败路径。方案 A 统一为 `PageFrames::alloc_phys_page` → `ForkError::OutOfMemory` 一条，因为 `PageSlot` 是 `Copy` 类型无需堆分配，`PageFrames` 全局数组无需创建 `PhysBlock` 对象。

### 3.3 事务性

通过检查点和回滚机制保证 fork 操作的原子性。

```rust
/// fork 操作状态
pub struct ForkState {
    /// 检查点：保存的原始状态
    checkpoint: Option<ForkCheckpoint>,
    /// 当前阶段
    phase: ForkPhase,
}

/// fork 检查点
struct ForkCheckpoint {
    /// 原始页表
    orig_pagetable: PageTable,
    /// 原始标志
    orig_flags: VmFlags,
}

/// fork 阶段
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ForkPhase {
    /// 初始状态
    Init,
    /// 参数验证完成
    Validated,
    /// 页表创建完成
    PageTableCreated,
    /// 区域复制完成
    RegionsCopied,
    /// 内核通知完成
    KernelNotified,
    /// 完成
    Completed,
}

impl ForkState {
    pub fn new() -> Self {
        Self {
            checkpoint: None,
            phase: ForkPhase::Init,
        }
    }

    pub fn checkpoint(&mut self, child: &mut VmProc) {
        self.checkpoint = Some(ForkCheckpoint {
            orig_pagetable: child.pagetable().clone(),
            orig_flags: child.flags(),
        });
        self.phase = ForkPhase::Validated;
    }
    
    pub fn rollback(&mut self, child: &mut VmProc, frames: &mut PageFrames) {
        if let Some(cp) = self.checkpoint.take() {
            if self.phase >= ForkPhase::PageTableCreated {
                child.pagetable_mut().free();
            }

            if self.phase >= ForkPhase::RegionsCopied {
                for region in child.regions_mut().iter_mut() {
                    for slot_opt in region.physblocks.iter() {
                        if let Some(slot) = slot_opt {
                            let state = frames.get_mut(slot.pfn).unwrap();
                            state.refcount -= 1;
                        }
                    }
                }
            }

            child.set_pagetable(cp.orig_pagetable);
            child.set_flags(cp.orig_flags);
            child.regions_mut().clear();
        }
        self.phase = ForkPhase::Init;
    }
    
    /// 更新阶段
    pub fn set_phase(&mut self, phase: ForkPhase) {
        self.phase = phase;
    }
}
```

### 3.4 进程表管理

```rust
use alloc::collections::BTreeMap;

pub struct ProcTable {
    procs: Vec<Mutex<VmProc>>,
    endpoint_map: BTreeMap<Endpoint, usize>,
}

impl ProcTable {
    pub fn new(nr_procs: usize) -> Self {
        let mut procs = Vec::with_capacity(nr_procs);
        for slot in 0..nr_procs {
            procs.push(Mutex::new(VmProc::new_empty(slot)));
        }

        Self {
            procs,
            endpoint_map: BTreeMap::new(),
        }
    }

    pub fn validate_endpoint(&self, endpoint: Endpoint) -> ForkResult<usize> {
        let slot = self.endpoint_map.get(&endpoint)
            .copied()
            .ok_or(ForkError::InvalidEndpoint(endpoint.as_raw()))?;

        let proc = self.procs[slot].lock();
        if !proc.is_in_use() || proc.endpoint() != endpoint {
            return Err(ForkError::InvalidEndpoint(endpoint.as_raw()));
        }

        Ok(slot)
    }

    pub fn get(&self, slot: usize) -> Option<MutexGuard<'_, VmProc>> {
        self.procs.get(slot).map(|p| p.lock())
    }
}
```

> **no_std 兼容性**: 使用 `alloc` crate 的 `Vec` 和 `BTreeMap`，而非 `std::sync::RwLock`。项目已在 09 完成自举，`VmAllocator`（基于 HeapArena）已实现 `GlobalAlloc`，`alloc` crate 可用。

---

## 4. 实现详解

### 4.1 消息处理入口

do_fork 函数接收 VM_FORK 消息，解析参数并启动 fork 流程。

```rust
use crate::ipc::Message;
use super::{ForkRequest, ForkResponse, ForkError, ForkState};

/// VM_FORK 消息处理入口
pub fn do_fork(
    proc_table: &ProcTable,
    frames: &mut PageFrames,
    msg: &Message,
) -> ForkResult<ForkResponse> {
    let request = ForkRequest {
        parent_endpoint: Endpoint::from_raw(msg.vmf_endpoint()),
        child_slot: msg.vmf_slotno() as usize,
    };
    
    let child_endpoint = fork_process(proc_table, frames, &request)?;
    
    Ok(ForkResponse::Ok { child_endpoint })
}

/// 执行 fork 操作
fn fork_process(
    proc_table: &ProcTable,
    frames: &mut PageFrames,
    request: &ForkRequest,
) -> ForkResult<Endpoint> {
    let mut state = ForkState::new();
    
    let parent_slot = proc_table.validate_endpoint(request.parent_endpoint)?;
    
    if request.child_slot >= NR_PROCS {
        return Err(ForkError::InvalidSlot(request.child_slot as i32));
    }
    
    let parent = proc_table.get(parent_slot)
        .ok_or(ForkError::InvalidSlot(parent_slot as i32))?;
    
    let mut child = proc_table.get_mut(request.child_slot)
        .ok_or(ForkError::InvalidSlot(request.child_slot as i32))?;
    
    state.checkpoint(&mut child);
    
    child.copy_from(&parent);
    child.set_slot(request.child_slot);
    child.regions_mut().clear();
    child.set_endpoint(Endpoint::NONE);
    
    let pagetable = PageTable::new()
        .map_err(ForkError::PageTableError)?;
    child.set_pagetable(pagetable);
    state.set_phase(ForkPhase::PageTableCreated);
    
    map_proc_copy(&mut child, &parent, frames)
        .map_err(|e| {
            state.rollback(&mut child, frames);
            ForkError::OutOfMemory
        })?;
    state.set_phase(ForkPhase::RegionsCopied);
    
    child.set_flags(VmFlags::IN_USE);
    acl_fork(&mut child);
    
    let child_endpoint = sys_fork(
        parent.endpoint(),
        request.child_slot,
        ForkFlags::VM_INHIBIT,
    ).map_err(|e| {
        panic!("sys_fork failed: {}", e);
    })?;
    
    child.set_endpoint(child_endpoint);
    
    child.bind_pagetable()
        .map_err(ForkError::PageTableError)?;
    
    state.set_phase(ForkPhase::Completed);
    
    Ok(child_endpoint)
}
```

### 4.2 父进程查找

通过 endpoint 在进程表中查找父进程的 VmProc 结构。

```rust
impl ProcTable {
    pub fn lookup_by_endpoint(&self, endpoint: Endpoint) -> Option<usize> {
        self.endpoint_map.get(&endpoint).copied()
    }

    pub fn validate_and_get_slot(&self, endpoint: Endpoint) -> ForkResult<usize> {
        let slot = self.lookup_by_endpoint(endpoint)
            .ok_or(ForkError::InvalidEndpoint(endpoint.as_raw()))?;

        let proc = self.get(slot)
            .ok_or(ForkError::InvalidSlot(slot as i32))?;

        if !proc.is_in_use() || proc.endpoint() != endpoint {
            return Err(ForkError::InvalidEndpoint(endpoint.as_raw()));
        }

        Ok(slot)
    }
}
```

### 4.3 子进程初始化

初始化子进程的 VmProc 结构，包括创建页表和初始化区域树。

```rust
impl VmProc {
    pub fn copy_from(&mut self, parent: &VmProc) {
        self.flags = parent.flags;
        self.stack_low = parent.stack_low;
        self.acl = parent.acl;
    }

    pub fn init_empty(&mut self, slot: usize) {
        self.slot = slot;
        self.endpoint = Endpoint::NONE;
        self.flags = VmFlags::empty();
        self.regions.clear();
        self.acl = NO_ACL;
    }
}
```

### 4.4 区域遍历与复制

遍历父进程的 AVL 树，复制每个虚拟区域到子进程。

```rust
/// 复制进程地址空间（对应 Minix3 map_proc_copy + map_proc_copy_range）
fn map_proc_copy(
    child: &mut VmProc,
    parent: &VmProc,
    frames: &mut PageFrames,
) -> Result<(), ForkError> {
    child.regions_mut().clear();

    for region in parent.regions().iter() {
        let new_region = map_copy_region(child, region, frames)?;
        child.regions_mut().insert(new_region);
    }

    map_writept(parent, frames)?;
    map_writept(child, frames)?;

    Ok(())
}

/// 复制单个虚拟区域（对应 Minix3 map_copy_region）
fn map_copy_region(
    child: &VmProc,
    parent_region: &VirRegion,
    frames: &mut PageFrames,
) -> Result<VirRegion, ForkError> {
    let mut new_region = VirRegion::new(
        parent_region.vaddr,
        parent_region.length,
        parent_region.flags,
        parent_region.def_memtype,
    );

    new_region.parent_slot = Some(child.slot.into());

    for (page_idx, slot_opt) in parent_region.physblocks.iter().enumerate() {
        if let Some(slot) = slot_opt {
            let memtype = slot.memtype.unwrap_or(parent_region.def_memtype.unwrap());

            if let Some(ev_ref) = memtype.ev_reference() {
                ev_ref(frames, slot);
            }

            frames.get_mut(slot.pfn).unwrap().refcount += 1;

            new_region.physblocks[page_idx] = Some(PageSlot {
                pfn: slot.pfn,
                offset: slot.offset,
                memtype: slot.memtype,
            });
        }
    }

    Ok(new_region)
}

/// 更新进程页表（对应 Minix3 map_writept）
fn map_writept(
    vmp: &VmProc,
    frames: &PageFrames,
) -> Result<(), PageTableError> {
    for region in vmp.regions().iter() {
        for (page_idx, slot_opt) in region.physblocks.iter().enumerate() {
            if let Some(slot) = slot_opt {
                if !slot.is_mapped() {
                    continue;
                }

                let state = frames.get(slot.pfn).unwrap();
                let writable = slot.memtype.unwrap().writable(frames, slot);
                let flags = if writable {
                    PteFlags::PRESENT | PteFlags::USER | PteFlags::WRITE
                } else {
                    PteFlags::PRESENT | PteFlags::USER
                };

                let offset = page_idx * PAGE_SIZE;
                pt_writemap(
                    vmp.pagetable(),
                    region.vaddr + offset,
                    frames.pfn_to_phys(slot.pfn),
                    PAGE_SIZE,
                    flags,
                )?;
            }
        }
    }

    Ok(())
}
```

**与 Minix3 的关键差异**

| Minix3 | 方案 A | 说明 |
|--------|-------|------|
| `pb_reference(ph->ph, ph->offset, newvr, vr->def_memtype)` | `frames.get_mut(slot.pfn).refcount += 1` + `new_region.physblocks[idx] = Some(slot)` | 直接递增 refcount + 复制 PageSlot，无需创建 PhysRegion/PhysBlock 对象 |
| `ph->memtype->ev_reference(ph, newph)` | `memtype.ev_reference()(frames, slot)` | MemType 回调签名变更：`PhysRegion` → `PageSlot + PageFrames` |
| `phys_region.phys_block().refcount()` | `frames.get(slot.pfn).refcount` | refcount 从 PhysBlock 移至 PageFrames 全局数组 |
| `phys_region.phys_block().phys()` | `frames.pfn_to_phys(slot.pfn)` | 物理地址通过 PFN 间接获取 |
| `pr_writable(vr, pr)` | `slot.memtype.unwrap().writable(frames, slot)` | 可写判断统一到 MemType trait |

> **方案 A 简化**: Minix3 的 `map_copy_region` 需要 `pb_reference()` → `pb_link()` 创建新的 `phys_region` 并链入 `phys_block.firstregion` 侵入式链表。方案 A 只需递增 `PageFrames.states[pfn].refcount` 并复制 `PageSlot`（Copy 类型），省去了堆分配和链表操作。

### 4.5 内核通知

调用 sys_fork 通知内核创建子进程，并绑定页表。

```rust
/// 通知内核创建子进程
fn notify_kernel(
    parent_endpoint: Endpoint,
    child_slot: usize,
) -> ForkResult<Endpoint> {
    let child_endpoint = sys_fork(
        parent_endpoint,
        child_slot,
        ForkFlags::VM_INHIBIT,
    ).map_err(|e| {
        panic!("sys_fork failed: {:?}", e);
    })?;

    Ok(child_endpoint)
}

pub fn sys_fork(
    parent: Endpoint,
    child_slot: usize,
    flags: ForkFlags,
) -> Result<Endpoint, KernelError> {
    let mut msg = Message::new(KERNEL, SYS_FORK);
    msg.set_parent(parent);
    msg.set_child_slot(child_slot);
    msg.set_flags(flags.bits());

    kernel_call(&mut msg)?;

    Ok(Endpoint::from_raw(msg.child_endpoint()))
}
```

**sys_fork 内核处理**

`sys_fork()` 的内核操作：

1. 分配新的 endpoint 给子进程：`endpoint = _ENDPOINT(0, child_slot)`
2. 复制父进程的内核栈
3. 设置子进程的调度状态
4. 如果设置了 `PFF_VMINHIBIT`：子进程初始状态为 VM 阻塞，等待 VM 完成初始化

返回值：`child_endpoint`（子进程 endpoint）和 `msgaddr`（fork 消息页面地址）

### 4.6 页表绑定

将创建的页表绑定到子进程。

```rust
impl VmProc {
    pub fn bind_pagetable(&mut self) -> Result<(), PageTableError> {
        pt_bind(&self.pagetable, self)
    }
}

fn pt_bind(pt: &PageTable, vmp: &VmProc) -> Result<(), PageTableError> {
    let pt_phys = pt.dir_phys();

    sys_set_pagetable(vmp.endpoint(), pt_phys)
        .map_err(PageTableError::KernelError)
}

pub fn sys_set_pagetable(
    endpoint: Endpoint,
    pt_phys: PhysAddr,
) -> Result<(), KernelError> {
    let mut msg = Message::new(KERNEL, SYS_VMCTL);
    msg.set_endpoint(endpoint);
    msg.set_request(VMCTL_SET_PAGE_DIR);
    msg.set_value(pt_phys.as_raw());

    kernel_call(&mut msg)?;
    Ok(())
}
```

### 4.7 错误处理与回滚

fork 过程中如果发生错误，需要回滚已完成的操作。

```rust
impl ForkState {
    pub fn checkpoint(&mut self, child: &mut VmProc) {
        self.checkpoint = Some(ForkCheckpoint {
            orig_pagetable: child.pagetable().clone(),
            orig_flags: child.flags(),
        });
    }

    pub fn rollback(&self, child: &mut VmProc, frames: &mut PageFrames) {
        if let Some(ref cp) = self.checkpoint {
            if self.phase >= ForkPhase::PageTableCreated {
                child.pagetable_mut().free();
            }

            if self.phase >= ForkPhase::RegionsCopied {
                for region in child.regions_mut().iter_mut() {
                    for slot_opt in region.physblocks.iter() {
                        if let Some(slot) = slot_opt {
                            let state = frames.get_mut(slot.pfn).unwrap();
                            state.refcount -= 1;
                        }
                    }
                }
            }

            child.set_pagetable(cp.orig_pagetable.clone());
            child.set_flags(cp.orig_flags);
            child.regions_mut().clear();
        }
    }

    pub fn set_phase(&mut self, phase: ForkPhase) {
        self.phase = phase;
    }
}
```

> **方案 A 回滚简化**: Minix3 的回滚需要遍历 `phys_block.firstregion` 侵入式链表，逐个 `pb_unlink()` 释放 `phys_region`。方案 A 的回滚只需遍历 `region.physblocks` 递减 `PageFrames.states[pfn].refcount`，无需链表操作和堆释放。

**错误处理流程**

| 阶段 | 失败条件 | 处理 |
|------|---------|------|
| Init → Validated | 参数验证失败 | 直接返回错误码，无副作用 |
| Validated → PageTableCreated | `pt_new()` 失败 | 返回 `ENOMEM`，子进程结构未修改 |
| PageTableCreated → RegionsCopied | `map_proc_copy()` 失败 | `state.rollback()` 释放页表和已复制区域，返回 `ENOMEM` |
| RegionsCopied → KernelNotified | `sys_fork()` 失败 | `panic!`（内核已创建子进程，状态不可逆） |

> `sys_fork()` 成功后不能失败——内核已创建子进程，不能回滚，必须继续完成。这就是 Minix3 使用 `panic` 而非返回错误的原因。

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