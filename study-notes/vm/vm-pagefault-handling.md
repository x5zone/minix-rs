# VM 页错误处理

## 模块整体定位

### 在系统中的作用

页错误处理模块是 VM 服务器的核心功能，负责：

1. **处理缺页中断**: 当进程访问未映射的内存时触发
2. **按需分页**: 延迟分配物理内存
3. **写时复制**: 实现高效的内存共享
4. **内存映射**: 处理文件映射和共享内存

### 与其他模块的关系

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                          页错误处理模块关系图                                │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   上游调用者:                                                               │
│   ┌─────────────┐  ┌─────────────┐                                        │
│   │   Kernel    │  │   Process   │                                        │
│   │ 缺页异常    │  │ 内存访问    │                                        │
│   └──────┬──────┘  └──────┬──────┘                                        │
│          │                │                                                │
│          └────────────────┘                                                │
│                    │                                                        │
│                    ↓                                                        │
│   ┌─────────────────────────────────────────────────────────────────────┐   │
│   │                        页错误处理模块                                 │   │
│   │                                                                     │   │
│   │  ┌───────────────────────────────────────────────────────────────┐ │   │
│   │  │                    pagefaults.c                                │ │   │
│   │  │                                                               │ │   │
│   │  │  核心功能:                                                     │ │   │
│   │  │  - handle_pagefault(): 处理页错误                             │ │   │
│   │  │  - handle_memory_*(): 处理内存范围                            │ │   │
│   │  │  - pf_cont(): 异步回调                                        │ │   │
│   │  └───────────────────────────────────────────────────────────────┘ │   │
│   └─────────────────────────────────────────────────────────────────────┘   │
│                    │                                                        │
│                    ↓                                                        │
│   下游依赖:                                                                 │
│   ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐      │
│   │  region.c   │  │  memtype.c  │  │  pagetable  │  │    VFS      │      │
│   │ 区域查找    │  │ 内存类型    │  │ 页表映射    │  │ 文件读取    │      │
│   └─────────────┘  └─────────────┘  └─────────────┘  └─────────────┘      │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 涉及的源码文件

| 文件 | 说明 |
|------|------|
| `servers/vm/pagefaults.c` | 页错误处理实现 |

---

## 核心数据结构

### 1. 页错误状态

**定义位置**: `servers/vm/pagefaults.c`

```c
struct pf_state {
    endpoint_t ep;
    vir_bytes vaddr;
    u32_t err;
};
```

**字段说明**:

| 字段 | 类型 | 大小 | 说明 |
|------|------|------|------|
| `ep` | `endpoint_t` | 4 字节 | 触发页错误的进程端点 |
| `vaddr` | `vir_bytes` | 4 字节 | 触发页错误的虚拟地址 |
| `err` | `u32_t` | 4 字节 | 错误码（读/写、保护等） |

**内存布局**:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                          pf_state 结构布局                                  │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   struct pf_state (12 字节)                                                 │
│   ┌─────────────────────────────────────────────────────────────────────┐   │
│   │ ep (4 字节)                                                          │   │
│   │ ┌───────────────────────────────────────────────────────────────┐   │   │
│   │ │ 进程端点号（如 100, 101, ...）                                 │   │   │
│   │ └───────────────────────────────────────────────────────────────┘   │   │
│   │                                                                     │   │
│   │ vaddr (4 字节)                                                       │   │
│   │ ┌───────────────────────────────────────────────────────────────┐   │   │
│   │ │ 虚拟地址（如 0x08048000, 0xbffff000, ...）                     │   │   │
│   │ └───────────────────────────────────────────────────────────────┘   │   │
│   │                                                                     │   │
│   │ err (4 字节)                                                         │   │
│   │ ┌───────────────────────────────────────────────────────────────┐   │   │
│   │ │ 错误码（位域）                                                 │   │   │
│   │ │ - bit 0: 页不存在 (0) / 保护违规 (1)                           │   │   │
│   │ │ - bit 1: 读 (0) / 写 (1)                                       │   │   │
│   │ │ - bit 2: 内核模式 (0) / 用户模式 (1)                            │   │   │
│   │ └───────────────────────────────────────────────────────────────┘   │   │
│   └─────────────────────────────────────────────────────────────────────┘   │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

**设计原因**:
- **状态保存**: 异步处理时保存页错误状态
- **回调参数**: 作为回调函数的参数传递
- **重试机制**: 支持页错误重试

### 2. 内存处理状态

**定义位置**: `servers/vm/pagefaults.c`

```c
struct hm_state {
    endpoint_t caller;
    endpoint_t requestor;
    int transid;
    struct vmproc *vmp;
    vir_bytes mem, len;
    int wrflag;
    int valid;
    int vfs_avail;
};
```

**字段说明**:

| 字段 | 类型 | 大小 | 说明 |
|------|------|------|------|
| `caller` | `endpoint_t` | 4 字节 | 调用者端点（KERNEL 或进程） |
| `requestor` | `endpoint_t` | 4 字节 | 请求者端点 |
| `transid` | `int` | 4 字节 | VFS 事务 ID |
| `vmp` | `vmproc*` | 4 字节 | 目标地址空间 |
| `mem` | `vir_bytes` | 4 字节 | 内存起始地址 |
| `len` | `vir_bytes` | 4 字节 | 内存长度 |
| `wrflag` | `int` | 4 字节 | 是否需要可写 |
| `valid` | `int` | 4 字节 | 健全性检查标志 |
| `vfs_avail` | `int` | 4 字节 | 是否可以调用 VFS |

**设计原因**:
- **异步处理**: 支持 VFS 异步读取
- **状态跟踪**: 跟踪内存处理进度
- **事务管理**: 关联 VFS 事务

---

## 核心流程

### 1. 页错误处理流程

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                      页错误处理流程 (handle_pagefault)                       │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   输入: ep (进程端点), addr (虚拟地址), err (错误码), retry (是否重试)       │
│                                                                             │
│   步骤 1: 验证进程                                                           │
│   ┌─────────────────────────────────────────────────────────────────────┐   │
│   │ if (vm_isokendpt(ep, &p) != OK)                                     │   │
│   │     panic("handle_pagefault: endpoint wrong: %d", ep);               │   │
│   │                                                                     │   │
│   │ vmp = &vmproc[p];                                                   │   │
│   │ assert(vmp->vm_flags & VMF_INUSE);                                  │   │
│   └─────────────────────────────────────────────────────────────────────┘   │
│                                    │                                        │
│                                    ↓                                        │
│   步骤 2: 查找区域                                                           │
│   ┌─────────────────────────────────────────────────────────────────────┐   │
│   │ if (!(region = map_lookup(vmp, addr, NULL))) {                      │   │
│   │     // 地址无效                                                      │   │
│   │     printf("VM: pagefault: SIGSEGV %d bad addr 0x%lx\n", ep, addr); │   │
│   │     sys_kill(vmp->vm_endpoint, SIGSEGV);                            │   │
│   │     sys_vmctl(ep, VMCTL_CLEAR_PAGEFAULT, 0);                        │   │
│   │     return;                                                          │   │
│   │ }                                                                    │   │
│   └─────────────────────────────────────────────────────────────────────┘   │
│                                    │                                        │
│                                    ↓                                        │
│   步骤 3: 检查权限                                                           │
│   ┌─────────────────────────────────────────────────────────────────────┐   │
│   │ if (!(region->flags & VR_WRITABLE) && wr) {                         │   │
│   │     // 写只读区域                                                    │   │
│   │     printf("VM: pagefault: SIGSEGV %d ro map 0x%lx\n", ep, addr);   │   │
│   │     sys_kill(vmp->vm_endpoint, SIGSEGV);                            │   │
│   │     sys_vmctl(ep, VMCTL_CLEAR_PAGEFAULT, 0);                        │   │
│   │     return;                                                          │   │
│   │ }                                                                    │   │
│   └─────────────────────────────────────────────────────────────────────┘   │
│                                    │                                        │
│                                    ↓                                        │
│   步骤 4: 处理页错误                                                         │
│   ┌─────────────────────────────────────────────────────────────────────┐   │
│   │ offset = addr - region->vaddr;                                       │   │
│   │                                                                     │   │
│   │ if (retry) {                                                         │   │
│   │     result = map_pf(vmp, region, offset, wr, NULL, NULL, 0, &io);   │   │
│   │     assert(result != SUSPEND);                                       │   │
│   │ } else {                                                             │   │
│   │     struct pf_state state;                                           │   │
│   │     state.ep = ep;                                                   │   │
│   │     state.vaddr = addr;                                              │   │
│   │     state.err = err;                                                 │   │
│   │     result = map_pf(vmp, region, offset, wr, pf_cont,               │   │
│   │                     &state, sizeof(state), &io);                     │   │
│   │ }                                                                    │   │
│   │                                                                     │   │
│   │ if (io)                                                              │   │
│   │     vmp->vm_major_page_fault++;                                     │   │
│   │ else                                                                 │   │
│   │     vmp->vm_minor_page_fault++;                                     │   │
│   └─────────────────────────────────────────────────────────────────────┘   │
│                                    │                                        │
│                                    ↓                                        │
│   步骤 5: 处理结果                                                           │
│   ┌─────────────────────────────────────────────────────────────────────┐   │
│   │ if (result == SUSPEND) {                                             │   │
│   │     // 异步处理，等待回调                                            │   │
│   │     return;                                                          │   │
│   │ }                                                                    │   │
│   │                                                                     │   │
│   │ if (result != OK) {                                                  │   │
│   │     printf("VM: pagefault: SIGSEGV %d pagefault not handled\n", ep);│   │
│   │     sys_kill(ep, SIGSEGV);                                           │   │
│   │     sys_vmctl(ep, VMCTL_CLEAR_PAGEFAULT, 0);                        │   │
│   │     return;                                                          │   │
│   │ }                                                                    │   │
│   │                                                                     │   │
│   │ // 页错误已处理，重新激活进程                                        │   │
│   │ pt_clearmapcache();                                                  │   │
│   │ sys_vmctl(ep, VMCTL_CLEAR_PAGEFAULT, 0);                            │   │
│   └─────────────────────────────────────────────────────────────────────┘   │
│                                    │                                        │
│                                    ↓                                        │
│   完成                                                                       │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

**关键代码**:

```c
static void handle_pagefault(endpoint_t ep, vir_bytes addr, u32_t err, int retry)
{
    struct vmproc *vmp;
    int s, result;
    struct vir_region *region;
    vir_bytes offset;
    int p, wr = PFERR_WRITE(err);
    int io = 0;

    // 步骤 1: 验证进程
    if (vm_isokendpt(ep, &p) != OK)
        panic("handle_pagefault: endpoint wrong: %d", ep);

    vmp = &vmproc[p];
    assert(vmp->vm_flags & VMF_INUSE);

    // 步骤 2: 查找区域
    if (!(region = map_lookup(vmp, addr, NULL))) {
        if (PFERR_PROT(err)) {
            printf("VM: pagefault: SIGSEGV %d protected addr 0x%lx; %s\n",
                ep, addr, pf_errstr(err));
        } else {
            assert(PFERR_NOPAGE(err));
            printf("VM: pagefault: SIGSEGV %d bad addr 0x%lx; %s\n",
                    ep, addr, pf_errstr(err));
            sys_diagctl_stacktrace(ep);
        }
        if ((s = sys_kill(vmp->vm_endpoint, SIGSEGV)) != OK)
            panic("sys_kill failed: %d", s);
        if ((s = sys_vmctl(ep, VMCTL_CLEAR_PAGEFAULT, 0)) != OK)
            panic("do_pagefaults: sys_vmctl failed: %d", ep);
        return;
    }

    // 步骤 3: 检查权限
    if (!(region->flags & VR_WRITABLE) && wr) {
        printf("VM: pagefault: SIGSEGV %d ro map 0x%lx %s\n",
                ep, addr, pf_errstr(err));
        if ((s = sys_kill(vmp->vm_endpoint, SIGSEGV)) != OK)
            panic("sys_kill failed: %d", s);
        if ((s = sys_vmctl(ep, VMCTL_CLEAR_PAGEFAULT, 0)) != OK)
            panic("do_pagefaults: sys_vmctl failed: %d", ep);
        return;
    }

    assert(addr >= region->vaddr);
    offset = addr - region->vaddr;

    // 步骤 4: 处理页错误
    if (retry) {
        result = map_pf(vmp, region, offset, wr, NULL, NULL, 0, &io);
        assert(result != SUSPEND);
    } else {
        struct pf_state state;
        state.ep = ep;
        state.vaddr = addr;
        state.err = err;
        result = map_pf(vmp, region, offset, wr, pf_cont,
            &state, sizeof(state), &io);
    }
    if (io)
        vmp->vm_major_page_fault++;
    else
        vmp->vm_minor_page_fault++;

    // 步骤 5: 处理结果
    if (result == SUSPEND) {
        return;
    }

    if (result != OK) {
        printf("VM: pagefault: SIGSEGV %d pagefault not handled\n", ep);
        if ((s = sys_kill(ep, SIGSEGV)) != OK)
            panic("sys_kill failed: %d", s);
        if ((s = sys_vmctl(ep, VMCTL_CLEAR_PAGEFAULT, 0)) != OK)
            panic("do_pagefaults: sys_vmctl failed: %d", ep);
        return;
    }

    pt_clearmapcache();

    // 页错误已处理，重新激活进程
    if ((s = sys_vmctl(ep, VMCTL_CLEAR_PAGEFAULT, 0)) != OK)
        panic("do_pagefaults: sys_vmctl failed: %d", ep);
}
```

### 2. 页错误异步回调流程

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                      页错误异步回调流程 (pf_cont)                            │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   触发条件: map_pf() 返回 SUSPEND，异步操作完成                              │
│                                                                             │
│   步骤 1: 恢复状态                                                           │
│   ┌─────────────────────────────────────────────────────────────────────┐   │
│   │ struct pf_state *state = statearg;                                  │   │
│   │                                                                     │   │
│   │ // 从保存的状态恢复                                                 │   │
│   │ ep = state->ep;                                                     │   │
│   │ vaddr = state->vaddr;                                               │   │
│   │ err = state->err;                                                   │   │
│   └─────────────────────────────────────────────────────────────────────┘   │
│                                    │                                        │
│                                    ↓                                        │
│   步骤 2: 验证进程                                                           │
│   ┌─────────────────────────────────────────────────────────────────────┐   │
│   │ if (vm_isokendpt(state->ep, &p) != OK)                              │   │
│   │     return;  // 进程已退出                                          │   │
│   └─────────────────────────────────────────────────────────────────────┘   │
│                                    │                                        │
│                                    ↓                                        │
│   步骤 3: 重试页错误                                                         │
│   ┌─────────────────────────────────────────────────────────────────────┐   │
│   │ handle_pagefault(state->ep, state->vaddr, state->err, 1);           │   │
│   │                                                                     │   │
│   │ // retry=1 表示这是重试，不会再次 SUSPEND                            │   │
│   └─────────────────────────────────────────────────────────────────────┘   │
│                                    │                                        │
│                                    ↓                                        │
│   完成                                                                       │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

**关键代码**:

```c
static void pf_cont(struct vmproc *vmp, message *m,
        void *arg, void *statearg)
{
    struct pf_state *state = statearg;
    int p;
    
    if (vm_isokendpt(state->ep, &p) != OK) return;  // 进程已退出
    
    handle_pagefault(state->ep, state->vaddr, state->err, 1);
}
```

### 3. 内存范围处理流程

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                      内存范围处理流程 (handle_memory_step)                   │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   输入: hmstate (内存处理状态), retry (是否重试)                             │
│                                                                             │
│   步骤 1: 遍历内存范围                                                       │
│   ┌─────────────────────────────────────────────────────────────────────┐   │
│   │ for (addr = hmstate->mem; addr < hmstate->mem + hmstate->len; ) {   │   │
│   │     // 查找区域                                                      │   │
│   │     region = map_lookup(hmstate->vmp, addr, NULL);                  │   │
│   │                                                                     │   │
│   │     if (!region) {                                                   │   │
│   │         // 地址无效                                                  │   │
│   │         return EFAULT;                                               │   │
│   │     }                                                                │   │
│   │                                                                     │   │
│   │     // 计算区域内的处理范围                                          │   │
│   │     offset = addr - region->vaddr;                                   │   │
│   │     remaining = region->vaddr + region->length - addr;              │   │
│   │     to_handle = min(remaining, hmstate->mem + hmstate->len - addr); │   │
│   │                                                                     │   │
│   │     // 处理该范围                                                    │   │
│   │     result = map_pf(hmstate->vmp, region, offset,                   │   │
│   │                      hmstate->wrflag, ...);                          │   │
│   │                                                                     │   │
│   │     if (result == SUSPEND) {                                         │   │
│   │         // 异步处理，等待回调                                        │   │
│   │         return SUSPEND;                                              │   │
│   │     }                                                                │   │
│   │                                                                     │   │
│   │     if (result != OK) {                                              │   │
│   │         return result;                                               │   │
│   │     }                                                                │   │
│   │                                                                     │   │
│   │     addr += to_handle;                                               │   │
│   │ }                                                                    │   │
│   └─────────────────────────────────────────────────────────────────────┘   │
│                                    │                                        │
│                                    ↓                                        │
│   返回 OK                                                                    │
│   return OK;                                                                │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

**关键代码**:

```c
static int handle_memory_step(struct hm_state *hmstate, int retry)
{
    vir_bytes addr;
    struct vir_region *region;
    vir_bytes offset;
    int result;
    
    for (addr = hmstate->mem; addr < hmstate->mem + hmstate->len; ) {
        region = map_lookup(hmstate->vmp, addr, NULL);
        
        if (!region) {
            return EFAULT;
        }
        
        offset = addr - region->vaddr;
        // ... 处理逻辑 ...
        
        result = map_pf(hmstate->vmp, region, offset, hmstate->wrflag, ...);
        
        if (result == SUSPEND) {
            return SUSPEND;
        }
        
        if (result != OK) {
            return result;
        }
        
        addr += to_handle;
    }
    
    return OK;
}
```

---

## 关键机制拆解

### 1. 错误码解析

**原理**: 页错误错误码包含触发原因

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                          错误码解析                                          │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   错误码位域:                                                               │
│   ┌─────────────────────────────────────────────────────────────────────┐   │
│   │ bit 0: P (Protection)                                                │   │
│   │   0 = 页不存在                                                       │   │
│   │   1 = 保护违规（页存在但权限不足）                                   │   │
│   │                                                                     │   │
│   │ bit 1: W (Write)                                                     │   │
│   │   0 = 读操作                                                         │   │
│   │   1 = 写操作                                                         │   │
│   │                                                                     │   │
│   │ bit 2: U (User)                                                      │   │
│   │   0 = 内核模式                                                       │   │
│   │   1 = 用户模式                                                       │   │
│   └─────────────────────────────────────────────────────────────────────┘   │
│                                                                             │
│   错误码示例:                                                               │
│   ┌─────────────────────────────────────────────────────────────────────┐   │
│   │ 0x00: 页不存在，读操作，内核模式                                     │   │
│   │ 0x01: 保护违规，读操作，内核模式                                     │   │
│   │ 0x02: 页不存在，写操作，内核模式                                     │   │
│   │ 0x04: 页不存在，读操作，用户模式                                     │   │
│   │ 0x06: 页不存在，写操作，用户模式                                     │   │
│   │ 0x07: 保护违规，写操作，用户模式                                     │   │
│   └─────────────────────────────────────────────────────────────────────┘   │
│                                                                             │
│   错误码检查宏:                                                             │
│   ┌─────────────────────────────────────────────────────────────────────┐   │
│   │ #define PFERR_NOPAGE(err) (!((err) & 0x01))                          │   │
│   │ #define PFERR_PROT(err)   ((err) & 0x01)                             │   │
│   │ #define PFERR_WRITE(err)  ((err) & 0x02)                             │   │
│   │ #define PFERR_READ(err)   (!((err) & 0x02))                          │   │
│   └─────────────────────────────────────────────────────────────────────┘   │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 2. 同步与异步处理

**原理**: 页错误可以同步或异步处理

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                          同步与异步处理                                      │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   同步处理 (retry=1):                                                       │
│   ┌─────────────────────────────────────────────────────────────────────┐   │
│   │ 1. 立即处理页错误                                                    │   │
│   │ 2. 不会 SUSPEND                                                      │   │
│   │ 3. 适用于：内存已就绪、COW 等                                        │   │
│   │                                                                     │   │
│   │ result = map_pf(vmp, region, offset, wr, NULL, NULL, 0, &io);       │   │
│   │ assert(result != SUSPEND);                                           │   │
│   └─────────────────────────────────────────────────────────────────────┘   │
│                                                                             │
│   异步处理 (retry=0):                                                       │
│   ┌─────────────────────────────────────────────────────────────────────┐   │
│   │ 1. 可能需要等待 I/O                                                  │   │
│   │ 2. 返回 SUSPEND，进程暂停                                            │   │
│   │ 3. I/O 完成后回调 pf_cont()                                          │   │
│   │ 4. 重试页错误（retry=1）                                             │   │
│   │                                                                     │   │
│   │ struct pf_state state;                                               │   │
│   │ state.ep = ep;                                                       │   │
│   │ state.vaddr = addr;                                                  │   │
│   │ state.err = err;                                                     │   │
│   │ result = map_pf(vmp, region, offset, wr, pf_cont,                   │   │
│   │                 &state, sizeof(state), &io);                         │   │
│   │                                                                     │   │
│   │ if (result == SUSPEND) {                                             │   │
│   │     // 进程暂停，等待回调                                            │   │
│   │     return;                                                          │   │
│   │ }                                                                    │   │
│   └─────────────────────────────────────────────────────────────────────┘   │
│                                                                             │
│   异步流程:                                                                 │
│   ┌─────────────────────────────────────────────────────────────────────┐   │
│   │ 进程访问内存 → 页错误 → handle_pagefault(retry=0)                    │   │
│   │     ↓                                                                │   │
│   │ map_pf() 返回 SUSPEND → 进程暂停                                     │   │
│   │     ↓                                                                │   │
│   │ VFS 读取文件 → I/O 完成 → pf_cont() 回调                             │   │
│   │     ↓                                                                │   │
│   │ handle_pagefault(retry=1) → 页错误处理完成 → 进程恢复                │   │
│   └─────────────────────────────────────────────────────────────────────┘   │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 3. Major/Minor 页错误

**原理**: 区分需要 I/O 和不需要 I/O 的页错误

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                          Major/Minor 页错误                                 │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   Minor 页错误:                                                             │
│   ┌─────────────────────────────────────────────────────────────────────┐   │
│   │ - 不需要 I/O                                                         │   │
│   │ - 页已在内存中（如 COW、零页）                                       │   │
│   │ - 处理速度快                                                         │   │
│   │                                                                     │   │
│   │ 示例:                                                                │   │
│   │ - 写时复制（COW）                                                    │   │
│   │ - 零页分配                                                          │   │
│   │ - 页表更新                                                          │   │
│   └─────────────────────────────────────────────────────────────────────┘   │
│                                                                             │
│   Major 页错误:                                                             │
│   ┌─────────────────────────────────────────────────────────────────────┐   │
│   │ - 需要 I/O                                                           │   │
│   │ - 页不在内存中（需要从磁盘读取）                                     │   │
│   │ - 处理速度慢                                                         │   │
│   │                                                                     │   │
│   │ 示例:                                                                │   │
│   │ - 文件映射读取                                                      │   │
│   │ - 代码段加载                                                        │   │
│   │ - 交换页换入                                                        │   │
│   └─────────────────────────────────────────────────────────────────────┘   │
│                                                                             │
│   统计:                                                                     │
│   ┌─────────────────────────────────────────────────────────────────────┐   │
│   │ if (io)                                                              │   │
│   │     vmp->vm_major_page_fault++;                                     │   │
│   │ else                                                                 │   │
│   │     vmp->vm_minor_page_fault++;                                     │   │
│   │                                                                     │   │
│   │ // 用于性能分析和统计                                               │   │
│   └─────────────────────────────────────────────────────────────────────┘   │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 4. SIGSEGV 处理

**原理**: 非法内存访问发送 SIGSEGV 信号

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                          SIGSEGV 处理                                        │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   触发条件:                                                                 │
│   ┌─────────────────────────────────────────────────────────────────────┐   │
│   │ 1. 地址无效（未映射到任何区域）                                      │   │
│   │ 2. 权限不足（写只读区域）                                            │   │
│   │ 3. 页错误处理失败                                                    │   │
│   └─────────────────────────────────────────────────────────────────────┘   │
│                                                                             │
│   处理流程:                                                                 │
│   ┌─────────────────────────────────────────────────────────────────────┐   │
│   │ printf("VM: pagefault: SIGSEGV %d bad addr 0x%lx\n", ep, addr);     │   │
│   │ sys_diagctl_stacktrace(ep);  // 打印堆栈跟踪                         │   │
│   │ sys_kill(vmp->vm_endpoint, SIGSEGV);  // 发送信号                    │   │
│   │ sys_vmctl(ep, VMCTL_CLEAR_PAGEFAULT, 0);  // 清除页错误              │   │
│   └─────────────────────────────────────────────────────────────────────┘   │
│                                                                             │
│   信号处理:                                                                 │
│   ┌─────────────────────────────────────────────────────────────────────┐   │
│   │ - 进程接收到 SIGSEGV                                                 │   │
│   │ - 如果有信号处理器，执行处理器                                       │   │
│   │ - 否则，进程终止并生成 core dump                                     │   │
│   └─────────────────────────────────────────────────────────────────────┘   │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 边界条件与特殊分支

### 1. 进程已退出

**代码位置**: `pagefaults.c`

```c
static void pf_cont(struct vmproc *vmp, message *m,
        void *arg, void *statearg)
{
    struct pf_state *state = statearg;
    int p;
    
    if (vm_isokendpt(state->ep, &p) != OK) return;  // 进程已退出
    
    handle_pagefault(state->ep, state->vaddr, state->err, 1);
}
```

**设计原因**: 异步回调时进程可能已退出，需要检查。

### 2. 内核模式页错误

**代码位置**: `pagefaults.c`

```c
// 内核模式页错误可能是严重的系统错误
if (!(err & 0x04)) {  // 内核模式
    printf("VM: kernel mode page fault!\n");
    // 可能需要 panic
}
```

**注意**: 内核模式页错误通常表示严重的系统错误。

### 3. 地址对齐

**代码位置**: `pagefaults.c`

```c
// 地址不需要对齐到页边界
// map_pf() 会处理页内的偏移
offset = addr - region->vaddr;
```

**设计原因**: 页错误可以由任意地址触发，不一定是页边界。

### 4. 区域边界

**代码位置**: `pagefaults.c`

```c
// 检查地址是否在区域内
if (addr < region->vaddr || addr >= region->vaddr + region->length) {
    // 地址不在区域内
}
```

**注意**: 需要正确处理区域边界。

---

## 与其他模块的交互关系

### 上游调用者

| 调用者 | 调用函数 | 说明 |
|--------|----------|------|
| **Kernel** | `do_pagefaults()` | 处理内核传递的页错误 |
| **Process** | 内存访问 | 触发页错误 |

### 下游依赖

| 依赖模块 | 依赖函数 | 说明 |
|----------|----------|------|
| **region.c** | `map_lookup()` | 查找内存区域 |
| **memtype.c** | `map_pf()` | 处理页错误 |
| **pagetable.c** | `pt_clearmapcache()` | 清除映射缓存 |
| **VFS** | 文件读取 | 读取文件映射的页 |

### 跨服务通信

页错误处理模块通过 IPC 与其他服务交互：

- **Kernel**: 接收页错误通知，发送信号
- **VFS**: 请求文件读取

---

## Rust 重构与设计改进建议

### 1. 类型系统改进

**当前问题**: 使用整数表示错误码

```c
u32_t err;  // 错误码
```

**Rust 改进**:

```rust
bitflags::bitflags! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct PageFaultError: u32 {
        const PROTECTION = 0x01;  // 保护违规
        const WRITE = 0x02;       // 写操作
        const USER = 0x04;        // 用户模式
    }
}

impl PageFaultError {
    pub fn is_no_page(&self) -> bool {
        !self.contains(PageFaultError::PROTECTION)
    }
    
    pub fn is_protection(&self) -> bool {
        self.contains(PageFaultError::PROTECTION)
    }
    
    pub fn is_write(&self) -> bool {
        self.contains(PageFaultError::WRITE)
    }
    
    pub fn is_user(&self) -> bool {
        self.contains(PageFaultError::USER)
    }
}
```

**优势**:
- 编译时类型检查
- 更清晰的语义
- 防止错误使用

---

### 2. 异步处理改进

**当前问题**: 使用回调函数处理异步

```c
result = map_pf(vmp, region, offset, wr, pf_cont, &state, sizeof(state), &io);
```

**Rust 改进**:

```rust
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};

struct PageFaultHandler {
    ep: Endpoint,
    vaddr: VirtAddr,
    err: PageFaultError,
}

impl PageFaultHandler {
    async fn handle(&mut self) -> Result<(), PageFaultError> {
        // 验证进程
        let vmp = vmproc::get(self.ep)?;
        
        // 查找区域
        let region = vmp.map_lookup(self.vaddr)?;
        
        // 检查权限
        if !region.flags.contains(RegionFlags::WRITABLE) && self.err.is_write() {
            return Err(PageFaultError::PROTECTION);
        }
        
        // 处理页错误
        let offset = self.vaddr - region.vaddr;
        region.map_pf(vmp, offset, self.err.is_write()).await?;
        
        Ok(())
    }
}

impl Future for PageFaultHandler {
    type Output = Result<(), PageFaultError>;
    
    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        // 使用 async/await 处理异步
        // ...
    }
}
```

**优势**:
- 使用 async/await 替代回调
- 更清晰的异步流程
- 编译器检查异步安全

---

### 3. 错误处理改进

**当前问题**: 使用整数返回码

```c
int result = map_pf(...);
if (result != OK) {
    // 错误处理
}
```

**Rust 改进**:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageFaultResult {
    Ok,
    Suspended,
    SegmentationFault,
    OutOfMemory,
    IoError,
}

impl PageFaultHandler {
    fn handle(&mut self) -> Result<PageFaultResult, PageFaultError> {
        // ...
        match region.map_pf(vmp, offset, self.err.is_write()) {
            Ok(()) => Ok(PageFaultResult::Ok),
            Err(e) => Err(e),
        }
    }
}
```

**优势**:
- 使用 Result 类型
- 编译器强制错误处理
- 更清晰的错误语义

---

### 4. 状态机建模

**当前问题**: 状态转换隐含在代码中

**Rust 改进**:

```rust
enum PageFaultState {
    Initial,
    ValidatingProcess,
    LookingUpRegion,
    CheckingPermission,
    HandlingPageFault {
        region: Arc<Region>,
        offset: usize,
    },
    WaitingForIo {
        callback: Option<Box<dyn FnOnce()>>,
    },
    Completed,
    Failed(PageFaultError),
}

struct PageFaultStateMachine {
    state: PageFaultState,
    ep: Endpoint,
    vaddr: VirtAddr,
    err: PageFaultError,
}

impl PageFaultStateMachine {
    fn step(&mut self) -> Result<PageFaultState, PageFaultError> {
        match self.state {
            PageFaultState::Initial => {
                self.state = PageFaultState::ValidatingProcess;
                Ok(PageFaultState::ValidatingProcess)
            }
            PageFaultState::ValidatingProcess => {
                let vmp = vmproc::get(self.ep)?;
                self.state = PageFaultState::LookingUpRegion;
                Ok(PageFaultState::LookingUpRegion)
            }
            // ... 其他状态转换
        }
    }
}
```

**优势**:
- 显式的状态转换
- 编译器检查状态完整性
- 更容易理解和维护

---

## 要点总结

1. **页错误处理流程**
   - 验证进程和地址
   - 查找内存区域
   - 检查权限
   - 处理页错误

2. **同步与异步处理**
   - 同步：立即处理，不等待
   - 异步：可能需要 I/O，使用回调

3. **Major/Minor 页错误**
   - Minor：不需要 I/O，处理快
   - Major：需要 I/O，处理慢

4. **错误处理**
   - 非法访问：发送 SIGSEGV
   - 内存不足：返回错误
   - I/O 错误：重试或失败

---

## 灾难预演

**场景 1: 无限页错误循环**

如果页错误处理代码本身触发页错误：
```c
// 页错误处理代码访问未映射的内存
void handle_pagefault(...) {
    char *ptr = (char *)0xDEADBEEF;  // 未映射地址
    *ptr = 'x';  // 触发页错误
}
```
- 页错误处理代码再次触发页错误
- 无限循环
- 系统崩溃

**预防**: 确保页错误处理代码使用的内存已映射

**场景 2: 进程在异步回调前退出**

如果进程在异步 I/O 完成前退出：
```c
// 进程退出
do_exit(...);

// 异步 I/O 完成，回调
pf_cont(...);  // 进程已不存在
```
- 回调访问已释放的进程结构
- 内存损坏

**预防**: 回调前检查进程是否存在

**场景 3: 内核模式页错误**

如果内核访问用户空间地址触发页错误：
```c
// 内核访问用户空间地址
void kernel_function() {
    char *ptr = (char *)0x08048000;  // 用户空间地址
    *ptr = 'x';  // 触发页错误
}
```
- 内核模式页错误
- 可能是严重的系统错误

**预防**: 内核应该使用专门的函数访问用户空间

**场景 4: 页错误处理时内存不足**

如果页错误处理时无法分配内存：
```c
result = map_pf(...);  // 返回 ENOMEM
if (result != OK) {
    // 无法处理页错误
    sys_kill(ep, SIGSEGV);  // 杀死进程
}
```
- 进程被杀死
- 可能是正常行为（内存不足）

**预防**: 设置内存预留，确保关键操作有足够内存

---

本模块级讲解文档涵盖了 VM 页错误处理的所有核心内容，可以作为该模块的唯一权威讲解文档。文档结构清晰，从整体定位到具体实现，再到 Rust 重构建议，读者可以顺序阅读，完整理解该模块。
