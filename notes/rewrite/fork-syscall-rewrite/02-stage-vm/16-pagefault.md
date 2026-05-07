# 16-pagefault: 页错误处理

> **分类**: VM私有  
> **源码**: `minix3/minix/servers/vm/pagefaults.c`  
> **说明**: 处理 CPU 产生的页错误，包括 CoW 触发、按需加载等

---

## 1. 概述

页错误（Page Fault）是 CPU 在访问内存时检测到异常情况而触发的中断。在 Minix3 中，VM 服务器负责处理所有用户进程的页错误，实现按需分页、CoW 等高级内存管理功能。

### 1.1 页错误类型

Minix3 定义了两种基本页错误类型（基于 x86 架构）：

| 错误类型 | 宏定义 | 触发条件 | 典型场景 |
|---------|--------|---------|---------|
| **不存在错误 (NP)** | `PFERR_NOPAGE(err)` | 页表项 Present 位为 0 | 首次访问、栈扩展、按需加载 |
| **保护错误 (WP)** | `PFERR_PROT(err)` | 页表项 Present 位为 1 但权限不足 | CoW 写保护触发 |

```c
// minix3/minix/servers/vm/arch/i386/pagetable.h
#define PFERR_NOPAGE(e) (!((e) & I386_VM_PFE_P))  // 页不存在
#define PFERR_PROT(e)   (((e) & I386_VM_PFE_P))   // 保护错误
#define PFERR_WRITE(e)  ((e) & I386_VM_PFE_W)     // 写操作触发
#define PFERR_READ(e)   (!((e) & I386_VM_PFE_W))  // 读操作触发
```

### 1.2 处理流程概览

```
┌─────────────────────────────────────────────────────────────────┐
│                     页错误处理流程                               │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  CPU 触发页错误异常                                              │
│         │                                                       │
│         ▼                                                       │
│  ┌──────────────┐                                               │
│  │   内核捕获    │  保存错误信息到消息                            │
│  │  发送消息给VM │  VPF_ADDR, VPF_FLAGS                          │
│  └──────────────┘                                               │
│         │                                                       │
│         ▼                                                       │
│  ┌──────────────┐                                               │
│  │ do_pagefaults│  入口函数，解析消息                            │
│  └──────────────┘                                               │
│         │                                                       │
│         ▼                                                       │
│  ┌──────────────┐                                               │
│  │handle_pagefault│ 主处理函数                                  │
│  └──────────────┘                                               │
│         │                                                       │
│         ├─────► map_lookup() ──► 查找虚拟区域                   │
│         │              │                                        │
│         │              └── 未找到 ──► SIGSEGV                   │
│         │                                                       │
│         ├─────► 权限检查 ──► 写只读区域 ──► SIGSEGV             │
│         │                                                       │
│         ▼                                                       │
│  ┌──────────────┐                                               │
│  │    map_pf    │  处理具体页面                                  │
│  └──────────────┘                                               │
│         │                                                       │
│         ├─────► physblock_get() ──► 获取/创建物理块             │
│         │                                                       │
│         ├─────► memtype->ev_pagefault() ──► 调用内存类型处理    │
│         │              │                                        │
│         │              ├── mem_anon: CoW 或分配新页              │
│         │              ├── mem_mappedfile: 从文件加载            │
│         │              └── mem_shared: 共享内存处理              │
│         │                                                       │
│         ▼                                                       │
│  ┌──────────────┐                                               │
│  │map_ph_writept│  更新页表                                     │
│  └──────────────┘                                               │
│         │                                                       │
│         ▼                                                       │
│  sys_vmctl(VMCTL_CLEAR_PAGEFAULT) ──► 恢复进程执行              │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### 1.3 与 Minix3 源码的对应关系

| 功能 | Minix3 源文件 | 函数/结构 |
|------|--------------|----------|
| 页错误入口 | `pagefaults.c` | `do_pagefaults()`, `handle_pagefault()` |
| 区域查找 | `region.c` | `map_lookup()` |
| 页面处理 | `region.c` | `map_pf()` |
| 内存类型处理 | `mem_anon.c` | `anon_pagefault()` |
| 物理块管理 | `pb.c` | `pb_new()`, `pb_reference()` |
| 页表操作 | `pagetable.c` | `pt_writemap()` |
| 错误码定义 | `arch/i386/pagetable.h` | `PFERR_*` 宏 |

### 1.4 关键数据结构

```c
// 页错误处理状态
struct pf_state {
    endpoint_t ep;      // 触发页错误的进程端点
    vir_bytes vaddr;    // 触发错误的虚拟地址
    u32_t err;          // CPU 错误码
};

// 内存请求状态（用于内核请求的内存操作）
struct hm_state {
    endpoint_t caller;      // 调用者（KERNEL 或进程）
    endpoint_t requestor;   // 请求发起者
    int transid;            // VFS 事务 ID
    struct vmproc *vmp;     // 目标地址空间
    vir_bytes mem, len;     // 内存范围
    int wrflag;             // 写标志
    int valid;              // 有效性检查
    int vfs_avail;          // 是否可以调用 VFS
};
```

---

## 2. C 源码分析

### 2.1 页错误类型

#### 2.1.1 写保护错误 (WP)

写保护错误（Write Protection Fault）是 CoW 机制的核心触发点。当进程尝试写入一个被标记为只读的页面时，CPU 触发此错误。

**触发条件**

```c
// 错误码判断
PFERR_PROT(err) == true   // 页表项 Present 位为 1
PFERR_WRITE(err) == true  // 是写操作触发
```

**典型场景：CoW 写时复制**

```
┌─────────────────────────────────────────────────────────────────┐
│                    写保护错误触发 CoW                            │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  fork() 后父子进程共享页面：                                      │
│                                                                 │
│  父进程页表: PTE_R | PTE_PRESENT (只读)                          │
│  子进程页表: PTE_R | PTE_PRESENT (只读)                          │
│  物理页:     refcount = 2                                       │
│                                                                 │
│  ─────────────────────────────────────────────────────────────  │
│                                                                 │
│  父进程尝试写入: *ptr = 42                                       │
│         │                                                       │
│         ▼                                                       │
│  CPU 检测到写只读页 → 触发页错误 (WP)                             │
│         │                                                       │
│         ▼                                                       │
│  VM 处理:                                                        │
│    1. map_lookup() 找到区域                                     │
│    2. map_pf() 调用 anon_pagefault()                            │
│    3. 检查 refcount > 1 && write → 执行 CoW                     │
│    4. mem_cow() 复制页面                                        │
│    5. 更新页表为可写                                             │
│         │                                                       │
│         ▼                                                       │
│  写保护错误处理后：                                               │
│                                                                 │
│  父进程页表: PTE_W | PTE_PRESENT (可写) ← 新物理页               │
│  子进程页表: PTE_R | PTE_PRESENT (只读) ← 原物理页               │
│  原物理页:  refcount = 1 (子进程)                                │
│  新物理页:  refcount = 1 (父进程)                                │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

**Minix3 源码分析**

```c
// minix3/minix/servers/vm/mem_anon.c - anon_pagefault()
static int anon_pagefault(struct vmproc *vmp, struct vir_region *region,
    struct phys_region *ph, int write, vfs_callback_t cb, void *state,
    int len, int *io)
{
    phys_bytes new_page, new_page_cl;
    u32_t allocflags;

    allocflags = vrallocflags(region->flags);
    assert(ph->ph->refcount > 0);

    // 分配新页面
    if((new_page_cl = alloc_mem(1, allocflags)) == NO_MEM) {
        printf("anon_pagefault: out of memory\n");
        return ENOMEM;
    }
    new_page = CLICK2ABS(new_page_cl);

    // 情况1: 物理块尚未分配（首次访问）
    if(ph->ph->phys == MAP_NONE) {
        ph->ph->phys = new_page;
        return OK;
    }

    // 情况2: 不需要 CoW（只有一个引用或只是读操作）
    if(ph->ph->refcount < 2 || !write) {
        return OK;  // 内存已就绪
    }

    // 情况3: 执行 CoW
    assert(region->flags & VR_WRITABLE);
    return mem_cow(region, ph, new_page_cl, new_page);
}
```

**判断是否需要 CoW**

```c
// minix3/minix/servers/vm/mem_anon.c - anon_writable()
static int anon_writable(struct phys_region *pr)
{
    assert(pr->ph->refcount > 0);
    
    // 物理页未分配，不可写
    if(pr->ph->phys == MAP_NONE)
        return 0;
    
    // 有 remaps（fork 后共享），可写（但会触发 CoW）
    if(pr->parent->remaps > 0)
        return 1;
    
    // 只有当引用计数为 1 时才真正可写
    return pr->ph->refcount == 1;
}
```

**关键点**

| 条件 | 行为 |
|------|------|
| `phys == MAP_NONE` | 首次访问，分配新页并清零 |
| `refcount == 1` | 单一引用，无需 CoW |
| `refcount > 1 && write` | 多引用写操作，执行 CoW |
| `refcount > 1 && !write` | 多引用读操作，直接返回 |

#### 2.1.2 不存在错误 (NP)

不存在错误（Not Present Fault）发生在访问的页面不在物理内存中时。这是按需分页（Demand Paging）和栈扩展的基础。

**触发条件**

```c
// 错误码判断
PFERR_NOPAGE(err) == true  // 页表项 Present 位为 0
```

**典型场景**

| 场景 | 说明 | 处理方式 |
|------|------|---------|
| **首次访问** | 进程首次访问已分配但未映射的内存 | 分配新页并清零 |
| **栈扩展** | 访问栈区域超出当前已映射范围 | 自动扩展栈 |
| **按需加载** | 文件映射区域首次访问 | 从文件加载内容 |
| **交换换入** | 访问被换出到磁盘的页面 | 从交换区加载 |

**首次访问流程**

```
┌─────────────────────────────────────────────────────────────────┐
│                    不存在错误 - 首次访问                          │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  malloc() 分配内存后首次访问:                                     │
│                                                                 │
│  虚拟区域: vaddr=0x10000, length=4096, flags=VR_ANON|VR_WRITABLE │
│  页表项:   Present=0 (未映射)                                    │
│  phys_block: phys=MAP_NONE, refcount=1                          │
│                                                                 │
│  ─────────────────────────────────────────────────────────────  │
│                                                                 │
│  进程访问: *ptr = 42                                             │
│         │                                                       │
│         ▼                                                       │
│  CPU 检测到页不存在 → 触发页错误 (NP)                             │
│         │                                                       │
│         ▼                                                       │
│  VM 处理:                                                        │
│    1. map_lookup() 找到区域 ✓                                   │
│    2. 权限检查: VR_WRITABLE ✓                                   │
│    3. map_pf() → physblock_get() → 创建 phys_region             │
│    4. anon_pagefault():                                         │
│       - phys == MAP_NONE → 分配新页                             │
│       - 设置 ph->ph->phys = new_page                            │
│    5. map_ph_writept() 更新页表                                 │
│         │                                                       │
│         ▼                                                       │
│  处理后状态:                                                     │
│                                                                 │
│  页表项:   Present=1, Writable=1                                │
│  phys_block: phys=0xABC000, refcount=1                          │
│  页面内容: 已清零                                                │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

**栈扩展**

Minix3 中栈区域可以自动增长。当访问栈区域未映射部分时，VM 自动扩展：

```c
// 栈区域特征
region->flags |= VR_GROWSDOWN;  // 向下增长

// 栈扩展检查（简化逻辑）
if (addr < region->vaddr && 
    addr >= vmp->vm_stack_low) {
    // 扩展栈区域
    region->vaddr = addr & PAGE_MASK;
    region->length += old_vaddr - region->vaddr;
}
```

**按需加载（文件映射）**

```c
// minix3/minix/servers/vm/mem_mappedfile.c
static int mappedfile_pagefault(...) {
    // 从文件读取内容到页面
    r = req_readwrite(...);
    
    // 设置 major page fault 统计
    *io = 1;
    
    return OK;
}
```

**Minix3 源码分析**

```c
// minix3/minix/servers/vm/region.c - map_pf()
int map_pf(struct vmproc *vmp,
    struct vir_region *region,
    vir_bytes offset,
    int write,
    vfs_callback_t pf_callback,
    void *state,
    int len,
    int *io)
{
    struct phys_region *ph;
    int r = OK;

    offset -= offset % VM_PAGE_SIZE;
    assert(offset < region->length);

    // 获取或创建 phys_region
    if(!(ph = physblock_get(region, offset))) {
        struct phys_block *pb;

        // 创建新的物理块
        if(!(pb = pb_new(MAP_NONE))) {
            printf("map_pf: pb_new failed\n");
            return ENOMEM;
        }

        // 引用物理块
        if(!(ph = pb_reference(pb, offset, region, region->def_memtype))) {
            printf("map_pf: pb_reference failed\n");
            pb_free(pb);
            return ENOMEM;
        }
    }

    // 调用内存类型的 pagefault 处理器
    if(!write || !ph->memtype->writable(ph)) {
        if((r = ph->memtype->ev_pagefault(vmp, region, ph, write,
            pf_callback, state, len, io)) == SUSPEND) {
            return SUSPEND;
        }
        // ...
    }

    // 更新页表
    if((r = map_ph_writept(vmp, region, ph)) != OK) {
        printf("map_pf: writept failed\n");
        return r;
    }

    return r;
}
```

**缺页统计**

```c
// minix3/minix/servers/vm/pagefaults.c
if (io)
    vmp->vm_major_page_fault++;  // 需要 I/O（从磁盘加载）
else
    vmp->vm_minor_page_fault++;  // 不需要 I/O（CoW、首次访问等）
```

### 2.2 页错误处理入口

#### 2.2.1 handle_pagefault - 主处理函数

`handle_pagefault()` 是页错误处理的核心函数，负责解析错误信息、查找区域、执行处理。

**函数原型**

```c
// minix3/minix/servers/vm/pagefaults.c
static void handle_pagefault(endpoint_t ep, vir_bytes addr, 
                             u32_t err, int retry);
```

**参数说明**

| 参数 | 类型 | 说明 |
|------|------|------|
| `ep` | `endpoint_t` | 触发页错误的进程端点 |
| `addr` | `vir_bytes` | 触发错误的虚拟地址 |
| `err` | `u32_t` | CPU 提供的错误码 |
| `retry` | `int` | 是否是重试（异步回调后） |

**处理流程**

```c
static void handle_pagefault(endpoint_t ep, vir_bytes addr, u32_t err, int retry)
{
    struct vmproc *vmp;
    int s, result;
    struct vir_region *region;
    vir_bytes offset;
    int p, wr = PFERR_WRITE(err);  // 是否是写操作
    int io = 0;

    // 1. 验证进程端点
    if(vm_isokendpt(ep, &p) != OK)
        panic("handle_pagefault: endpoint wrong: %d", ep);

    vmp = &vmproc[p];
    assert(vmp->vm_flags & VMF_INUSE);

    // 2. 查找虚拟区域
    if(!(region = map_lookup(vmp, addr, NULL))) {
        // 区域不存在，发送 SIGSEGV
        if(PFERR_PROT(err)) {
            printf("VM: pagefault: SIGSEGV %d protected addr 0x%lx; %s\n",
                ep, addr, pf_errstr(err));
        } else {
            assert(PFERR_NOPAGE(err));
            printf("VM: pagefault: SIGSEGV %d bad addr 0x%lx; %s\n",
                    ep, addr, pf_errstr(err));
            sys_diagctl_stacktrace(ep);  // 打印调用栈
        }
        sys_kill(vmp->vm_endpoint, SIGSEGV);
        sys_vmctl(ep, VMCTL_CLEAR_PAGEFAULT, 0);
        return;
    }

    // 3. 权限检查：写操作需要可写区域
    if(!(region->flags & VR_WRITABLE) && wr) {
        printf("VM: pagefault: SIGSEGV %d ro map 0x%lx %s\n",
                ep, addr, pf_errstr(err));
        sys_kill(vmp->vm_endpoint, SIGSEGV);
        sys_vmctl(ep, VMCTL_CLEAR_PAGEFAULT, 0);
        return;
    }

    // 4. 计算区域内的偏移
    assert(addr >= region->vaddr);
    offset = addr - region->vaddr;

    // 5. 调用 map_pf 处理页面
    if(retry) {
        // 重试路径（异步操作完成后）
        result = map_pf(vmp, region, offset, wr, NULL, NULL, 0, &io);
        assert(result != SUSPEND);
    } else {
        // 首次处理
        struct pf_state state;
        state.ep = ep;
        state.vaddr = addr;
        state.err = err;
        result = map_pf(vmp, region, offset, wr, pf_cont,
            &state, sizeof(state), &io);
    }

    // 6. 更新缺页统计
    if (io)
        vmp->vm_major_page_fault++;
    else
        vmp->vm_minor_page_fault++;

    // 7. 处理 SUSPEND（等待异步操作）
    if(result == SUSPEND) {
        return;  // 等待回调
    }

    // 8. 处理失败
    if(result != OK) {
        printf("VM: pagefault: SIGSEGV %d pagefault not handled\n", ep);
        sys_kill(ep, SIGSEGV);
        sys_vmctl(ep, VMCTL_CLEAR_PAGEFAULT, 0);
        return;
    }

    // 9. 清除页错误状态，恢复进程执行
    pt_clearmapcache();
    sys_vmctl(ep, VMCTL_CLEAR_PAGEFAULT, 0);
}
```

**错误码解析函数**

```c
char *pf_errstr(u32_t err)
{
    static char buf[100];

    snprintf(buf, sizeof(buf), "err 0x%lx ", (long)err);
    if(PFERR_NOPAGE(err)) strcat(buf, "nopage ");
    if(PFERR_PROT(err))   strcat(buf, "protection ");
    if(PFERR_WRITE(err))  strcat(buf, "write");
    if(PFERR_READ(err))   strcat(buf, "read");

    return buf;
}
```

**异步回调机制**

```c
// 异步操作完成后的回调
static void pf_cont(struct vmproc *vmp, message *m, void *arg, void *statearg)
{
    struct pf_state *state = statearg;
    int p;
    
    // 验证进程仍然有效
    if(vm_isokendpt(state->ep, &p) != OK) return;
    
    // 重试页错误处理
    handle_pagefault(state->ep, state->vaddr, state->err, 1);
}
```

**流程图**

```
┌─────────────────────────────────────────────────────────────────┐
│                  handle_pagefault 处理流程                       │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  输入: ep, addr, err, retry                                     │
│         │                                                       │
│         ▼                                                       │
│  ┌──────────────────┐                                           │
│  │ vm_isokendpt()   │──► 失败 ──► panic                         │
│  └──────────────────┘                                           │
│         │ OK                                                    │
│         ▼                                                       │
│  ┌──────────────────┐                                           │
│  │ map_lookup()     │──► 未找到 ──► SIGSEGV                     │
│  └──────────────────┘                                           │
│         │ 找到                                                  │
│         ▼                                                       │
│  ┌──────────────────┐                                           │
│  │ 权限检查         │──► 写只读区域 ──► SIGSEGV                  │
│  └──────────────────┘                                           │
│         │ OK                                                    │
│         ▼                                                       │
│  ┌──────────────────┐                                           │
│  │ map_pf()         │──► SUSPEND ──► 等待回调                   │
│  └──────────────────┘                                           │
│         │ OK/错误                                               │
│         ▼                                                       │
│  ┌──────────────────┐                                           │
│  │ 更新缺页统计     │                                           │
│  └──────────────────┘                                           │
│         │                                                       │
│         ▼                                                       │
│  ┌──────────────────┐                                           │
│  │ 错误? ──► SIGSEGV│                                           │
│  └──────────────────┘                                           │
│         │ OK                                                    │
│         ▼                                                       │
│  sys_vmctl(VMCTL_CLEAR_PAGEFAULT) ──► 恢复进程                  │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

#### 2.2.2 handle_memory_start - 内存操作开始

`handle_memory_start()` 用于处理内核或其他进程请求的内存操作，如 `memcpy` 跨进程内存访问。

**函数原型**

```c
// minix3/minix/servers/vm/pagefaults.c
int handle_memory_start(struct vmproc *vmp, vir_bytes mem, vir_bytes len,
    int wrflag, endpoint_t caller, endpoint_t requestor, int transid,
    int vfs_avail);
```

**参数说明**

| 参数 | 类型 | 说明 |
|------|------|------|
| `vmp` | `struct vmproc *` | 目标进程的 VM 进程结构 |
| `mem` | `vir_bytes` | 起始虚拟地址 |
| `len` | `vir_bytes` | 内存区域长度 |
| `wrflag` | `int` | 是否需要写权限 |
| `caller` | `endpoint_t` | 调用者端点（KERNEL 或进程） |
| `requestor` | `endpoint_t` | 请求发起者 |
| `transid` | `int` | VFS 事务 ID |
| `vfs_avail` | `int` | 是否可以调用 VFS |

**处理流程**

```c
int handle_memory_start(struct vmproc *vmp, vir_bytes mem, vir_bytes len,
    int wrflag, endpoint_t caller, endpoint_t requestor, int transid,
    int vfs_avail)
{
    int r;
    struct hm_state state;
    vir_bytes o;

    // 1. 页对齐地址和长度
    if((o = mem % PAGE_SIZE)) {
        mem -= o;
        len += o;
    }
    len = roundup(len, PAGE_SIZE);

    // 2. 初始化状态结构
    state.vmp = vmp;
    state.mem = mem;
    state.len = len;
    state.wrflag = wrflag;
    state.requestor = requestor;
    state.caller = caller;
    state.transid = transid;
    state.valid = VALID;
    state.vfs_avail = vfs_avail;

    // 3. 执行内存处理步骤
    r = handle_memory_step(&state, FALSE /*retry*/);

    // 4. 处理结果
    if(r == SUSPEND) {
        assert(caller != NONE);
        assert(vfs_avail);
    } else {
        handle_memory_final(&state, r);
    }

    return r;
}
```

**同步版本**

```c
// 同步处理，不等待异步操作
int handle_memory_once(struct vmproc *vmp, vir_bytes mem, vir_bytes len,
    int wrflag)
{
    int r;
    r = handle_memory_start(vmp, mem, len, wrflag, NONE, NONE, 0, 0);
    assert(r != SUSPEND);
    return r;
}
```

**使用场景**

```
┌─────────────────────────────────────────────────────────────────┐
│               handle_memory_start 使用场景                       │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  1. 内核请求内存访问 (VMPTYPE_CHECK)                              │
│     - sys_vircopy/sys_physcopy 跨进程内存复制                    │
│     - 内核需要访问用户空间内存                                    │
│                                                                 │
│  2. VFS 请求内存操作                                             │
│     - 文件读写需要访问用户缓冲区                                  │
│     - 异步操作，需要事务 ID 跟踪                                  │
│                                                                 │
│  3. 进程间内存共享检查                                           │
│     - 共享内存区域验证                                           │
│     - 权限检查                                                   │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

**内核内存请求处理**

```c
// minix3/minix/servers/vm/pagefaults.c - do_memory()
void do_memory(void)
{
    endpoint_t who, who_s, requestor;
    vir_bytes mem, mem_s;
    vir_bytes len;
    int wrflag;

    while(1) {
        int p, r = OK;
        struct vmproc *vmp;

        // 从内核获取内存请求
        r = sys_vmctl_get_memreq(&who, &mem, &len, &wrflag, &who_s,
            &mem_s, &requestor);

        switch(r) {
        case VMPTYPE_CHECK:
        {
            int transid = 0;
            int vfs_avail;

            if(vm_isokendpt(who, &p) != OK)
                panic("do_memory: bad endpoint: %d", who);
            vmp = &vmproc[p];

            // 检查 VFS 是否被阻塞
            if(requestor == VFS_PROC_NR) vfs_avail = 0;
            else vfs_avail = 1;

            // 处理内存请求
            handle_memory_start(vmp, mem, len, wrflag,
                KERNEL, requestor, transid, vfs_avail);
            break;
        }

        default:
            return;
        }
    }
}
```

**异步回调处理**

```c
static void handle_memory_continue(struct vmproc *vmp, message *m,
    void *arg, void *statearg)
{
    int r;
    struct hm_state *state = statearg;
    
    assert(state);
    assert(state->caller != NONE);
    assert(state->valid == VALID);

    // VFS 请求失败
    if(m->VMV_RESULT != OK) {
        printf("VM: handle_memory_continue: vfs request failed\n");
        handle_memory_final(state, m->VMV_RESULT);
        return;
    }

    // 重试处理步骤
    r = handle_memory_step(state, TRUE /*retry*/);

    if(r == SUSPEND) {
        return;  // 继续等待
    }

    handle_memory_final(state, r);
}
```

**最终处理**

```c
static void handle_memory_final(struct hm_state *state, int result)
{
    int r, flag;

    assert(state);
    assert(state->valid == VALID);

    if(state->caller == KERNEL) {
        // 回复内核
        if((r=sys_vmctl(state->requestor, VMCTL_MEMREQ_REPLY, result)) != OK)
            panic("handle_memory_continue: sys_vmctl failed: %d", r);
    } else if(state->caller != NONE) {
        // 发送回复消息给进程
        message msg;
        memset(&msg, 0, sizeof(msg));
        msg.m_type = result;

        if(IS_VFS_FS_TRANSID(state->transid)) {
            assert(state->caller == VFS_PROC_NR);
            msg.m_type = TRNS_ADD_ID(msg.m_type, state->transid);
            flag = AMF_NOREPLY;
        } else
            flag = 0;

        if(asynsend3(state->caller, &msg, flag) != OK) {
            panic("handle_memory_final: asynsend3 failed");
        }

        // 清除状态
        memset(state, 0, sizeof(*state));
    }
}
```

### 2.3 处理流程

#### 2.3.1 验证地址

地址验证是页错误处理的第一步，确保访问的地址在合法范围内。

**验证步骤**

```c
// 1. 验证进程端点
if(vm_isokendpt(ep, &p) != OK)
    panic("handle_pagefault: endpoint wrong: %d", ep);

// 2. 验证进程状态
vmp = &vmproc[p];
assert(vmp->vm_flags & VMF_INUSE);

// 3. 查找虚拟区域
if(!(region = map_lookup(vmp, addr, NULL))) {
    // 地址不在任何已分配区域内
    // 发送 SIGSEGV
}
```

**地址合法性检查**

```
┌─────────────────────────────────────────────────────────────────┐
│                     地址合法性检查                                │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  进程地址空间布局:                                               │
│                                                                 │
│  高地址 ┌────────────────┐                                      │
│         │    内核空间     │  ← 用户态不可访问                     │
│         ├────────────────┤                                      │
│         │    栈区域       │  ← 可向下增长                        │
│         │       ↓        │                                      │
│         │       ...       │                                      │
│         │       ↑        │                                      │
│         │    堆区域       │  ← 可向上增长                        │
│         ├────────────────┤                                      │
│         │    BSS 段       │                                      │
│         ├────────────────┤                                      │
│         │    数据段       │                                      │
│         ├────────────────┤                                      │
│         │    代码段       │                                      │
│  低地址 └────────────────┘                                      │
│                                                                 │
│  非法地址示例:                                                   │
│  - NULL (0x0)                                                   │
│  - 超出进程地址空间范围                                          │
│  - 未映射的间隙区域                                              │
│  - 内核空间地址（用户态）                                        │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

**map_lookup 实现**

```c
// minix3/minix/servers/vm/region.c
struct vir_region *map_lookup(struct vmproc *vmp,
    vir_bytes offset, struct phys_region **physr)
{
    struct vir_region *r;

    // 使用 AVL 树快速查找
    if((r = region_search(&vmp->vm_regions_avl, offset, AVL_LESS_EQUAL))) {
        vir_bytes ph;
        // 检查地址是否在区域内
        if(offset >= r->vaddr && offset < r->vaddr + r->length) {
            ph = offset - r->vaddr;
            if(physr) {
                *physr = physblock_get(r, ph);
                if(*physr) assert((*physr)->offset == ph);
            }
            return r;
        }
    }

    return NULL;  // 未找到
}
```

**错误处理**

```c
// 地址非法时的处理
if(!(region = map_lookup(vmp, addr, NULL))) {
    if(PFERR_PROT(err)) {
        // 保护错误但区域不存在（异常情况）
        printf("VM: pagefault: SIGSEGV %d protected addr 0x%lx; %s\n",
            ep, addr, pf_errstr(err));
    } else {
        // 页不存在且区域不存在
        assert(PFERR_NOPAGE(err));
        printf("VM: pagefault: SIGSEGV %d bad addr 0x%lx; %s\n",
                ep, addr, pf_errstr(err));
        sys_diagctl_stacktrace(ep);  // 打印调用栈帮助调试
    }
    // 发送段错误信号
    sys_kill(vmp->vm_endpoint, SIGSEGV);
    // 清除页错误状态
    sys_vmctl(ep, VMCTL_CLEAR_PAGEFAULT, 0);
    return;
}
```

**常见非法访问原因**

| 原因 | 说明 | 典型地址 |
|------|------|---------|
| 空指针解引用 | `*NULL = 0` | 0x0 |
| 野指针 | 使用未初始化的指针 | 随机值 |
| 越界访问 | 数组越界、缓冲区溢出 | 栈/堆附近 |
| 释放后使用 | 访问已 free 的内存 | 堆区域 |
| 栈溢出 | 递归太深或局部变量太大 | 栈边界附近 |

#### 2.3.2 查找区域

区域查找使用 AVL 树实现 O(log n) 的时间复杂度，是页错误处理的关键步骤。

**AVL 树查找原理**

```
┌─────────────────────────────────────────────────────────────────┐
│                    AVL 树区域查找                                 │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  进程的虚拟区域按起始地址组织成 AVL 树:                            │
│                                                                 │
│                    [0x400000, 0x410000]                         │
│                         /          \                            │
│        [0x10000, 0x20000]          [0x600000, 0x610000]        │
│              /     \                                            │
│   [0x0, 0x1000]  [0x20000, 0x30000]                            │
│                                                                 │
│  查找 addr=0x15000:                                             │
│  1. 比较 0x15000 < 0x400000 → 向左                              │
│  2. 比较 0x15000 >= 0x10000 且 < 0x20000 → 找到!                │
│                                                                 │
│  region_search(vmp->vm_regions_avl, addr, AVL_LESS_EQUAL)       │
│  返回: vaddr <= addr 的最大区域                                  │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

**region_search 实现**

```c
// minix3/minix/servers/vm/region.c - region_search()
struct vir_region *region_search(struct avl_head *head, vir_bytes addr,
    int avlflags)
{
    struct avl_node *node;
    struct vir_region *r, *best = NULL;

    node = head->root;
    while(node) {
        r = avl_data(node, struct vir_region, avl_node);
        
        if(addr < r->vaddr) {
            // 地址在当前区域之前，向左子树查找
            node = node->left;
        } else if(addr >= r->vaddr + r->length) {
            // 地址在当前区域之后，向右子树查找
            if(avlflags & AVL_LESS_EQUAL)
                best = r;  // 记录候选
            node = node->right;
        } else {
            // 地址在当前区域内
            return r;
        }
    }

    // 返回最近的候选区域
    if(avlflags & AVL_LESS_EQUAL)
        return best;
    return NULL;
}
```

**查找标志**

| 标志 | 说明 |
|------|------|
| `AVL_EQUAL` | 精确匹配 |
| `AVL_LESS_EQUAL` | 返回 vaddr <= addr 的最大区域 |
| `AVL_GREATER_EQUAL` | 返回 vaddr >= addr 的最小区域 |

**map_lookup 完整实现**

```c
// minix3/minix/servers/vm/region.c
struct vir_region *map_lookup(struct vmproc *vmp,
    vir_bytes offset, struct phys_region **physr)
{
    struct vir_region *r;

    SANITYCHECK(SCL_FUNCTIONS);

#if SANITYCHECKS
    if(!region_search_root(&vmp->vm_regions_avl))
        panic("process has no regions: %d", vmp->vm_endpoint);
#endif

    // 使用 AVL_LESS_EQUAL 查找
    if((r = region_search(&vmp->vm_regions_avl, offset, AVL_LESS_EQUAL))) {
        vir_bytes ph;
        // 二次验证：确保地址确实在区域内
        if(offset >= r->vaddr && offset < r->vaddr + r->length) {
            ph = offset - r->vaddr;
            if(physr) {
                // 获取物理区域
                *physr = physblock_get(r, ph);
                if(*physr) assert((*physr)->offset == ph);
            }
            return r;
        }
    }

    SANITYCHECK(SCL_FUNCTIONS);

    return NULL;
}
```

**physblock_get 实现**

```c
// 获取虚拟区域内特定偏移处的物理区域
struct phys_region *physblock_get(struct vir_region *region, vir_bytes offset)
{
    struct phys_region *ph;

    // 遍历物理区域链表
    for(ph = region->phys; ph; ph = ph->next) {
        if(ph->offset == offset)
            return ph;
        if(ph->offset > offset)
            break;  // 物理区域按偏移排序
    }

    return NULL;  // 该偏移处没有物理区域
}
```

**查找流程图**

```
┌─────────────────────────────────────────────────────────────────┐
│                    区域查找流程                                   │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  输入: vmp, addr                                                │
│         │                                                       │
│         ▼                                                       │
│  ┌──────────────────────┐                                       │
│  │ region_search AVL树  │                                       │
│  │ AVL_LESS_EQUAL       │                                       │
│  └──────────────────────┘                                       │
│         │                                                       │
│         ├─────► 未找到 ──► 返回 NULL                            │
│         │                                                       │
│         ▼ 找到候选区域 r                                        │
│  ┌──────────────────────┐                                       │
│  │ 验证: addr >= r->vaddr &&                                    │
│  │       addr < r->vaddr + r->length                            │
│  └──────────────────────┘                                       │
│         │                                                       │
│         ├─────► 不在区域内 ──► 返回 NULL                        │
│         │                                                       │
│         ▼ 在区域内                                              │
│  ┌──────────────────────┐                                       │
│  │ 计算偏移: offset = addr - r->vaddr                           │
│  └──────────────────────┘                                       │
│         │                                                       │
│         ▼                                                       │
│  ┌──────────────────────┐                                       │
│  │ physblock_get(r, offset)                                     │
│  │ 获取物理区域（可选）                                          │
│  └──────────────────────┘                                       │
│         │                                                       │
│         ▼                                                       │
│  返回: struct vir_region *                                      │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

**性能分析**

| 操作 | 时间复杂度 | 说明 |
|------|-----------|------|
| AVL 树查找 | O(log n) | n 为区域数量 |
| 物理区域查找 | O(m) | m 为区域内的物理块数量 |
| 总体 | O(log n + m) | 通常 m 很小 |

**优化策略**

1. **缓存最近访问区域**：进程通常有局部性
2. **物理区域使用更高效结构**：大区域可考虑红黑树
3. **批量处理**：连续页错误可合并处理

#### 2.3.3 检查权限

权限检查确保进程对访问的内存区域有足够的访问权限。

**权限检查代码**

```c
// minix3/minix/servers/vm/pagefaults.c
int wr = PFERR_WRITE(err);  // 是否是写操作

// 检查区域是否可写
if(!(region->flags & VR_WRITABLE) && wr) {
    printf("VM: pagefault: SIGSEGV %d ro map 0x%lx %s\n",
            ep, addr, pf_errstr(err));
    sys_kill(vmp->vm_endpoint, SIGSEGV);
    sys_vmctl(ep, VMCTL_CLEAR_PAGEFAULT, 0);
    return;
}
```

**区域权限标志**

```c
// minix3/minix/servers/vm/region.h
#define VR_NONE        0x0000   // 无权限
#define VR_READABLE    0x0001   // 可读
#define VR_WRITABLE    0x0002   // 可写
#define VR_EXECUTABLE  0x0004   // 可执行
#define VR_ANON        0x0010   // 匿名内存
#define VR_GROWSDOWN   0x0020   // 向下增长（栈）
#define VR_GROWSUP     0x0040   // 向上增长（堆）
```

**权限检查矩阵**

```
┌─────────────────────────────────────────────────────────────────┐
│                     权限检查矩阵                                 │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  区域权限    │  读操作    │  写操作    │  执行操作               │
│  ───────────┼───────────┼───────────┼────────────              │
│  VR_READABLE │    ✓      │    ✗      │    ✗                    │
│  VR_WRITABLE │    ✓      │    ✓      │    ✗                    │
│  VR_EXECUTABLE│   ✓      │    ✗      │    ✓                    │
│  全部权限    │    ✓      │    ✓      │    ✓                    │
│                                                                 │
│  ✗ = 触发 SIGSEGV                                              │
│  ✓ = 允许访问                                                   │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

**CoW 特殊处理**

对于 CoW 页面，权限检查有特殊逻辑：

```c
// 区域标记为可写，但页面可能是只读的（等待 CoW）
if(region->flags & VR_WRITABLE) {
    // 区域本身可写
    // 但如果 phys_block.refcount > 1，页面被标记为只读
    // 写操作会触发 CoW
}
```

**权限检查流程**

```
┌─────────────────────────────────────────────────────────────────┐
│                     权限检查流程                                 │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  输入: region, err (错误码)                                      │
│         │                                                       │
│         ▼                                                       │
│  ┌──────────────────────┐                                       │
│  │ PFERR_WRITE(err)?    │                                       │
│  │ 判断是否写操作        │                                       │
│  └──────────────────────┘                                       │
│         │                                                       │
│         ├─────► 读操作 ──► 检查 VR_READABLE                     │
│         │                                                       │
│         ▼ 写操作                                                │
│  ┌──────────────────────┐                                       │
│  │ region->flags &      │                                       │
│  │ VR_WRITABLE?         │                                       │
│  └──────────────────────┘                                       │
│         │                                                       │
│         ├─────► 否 ──► SIGSEGV (写只读区域)                     │
│         │                                                       │
│         ▼ 是                                                    │
│  ┌──────────────────────┐                                       │
│  │ 检查 CoW 状态         │                                       │
│  │ phys_block.refcount? │                                       │
│  └──────────────────────┘                                       │
│         │                                                       │
│         ├─────► refcount == 1 ──► 直接写入                      │
│         │                                                       │
│         ├─────► refcount > 1 ──► 触发 CoW                       │
│         │                                                       │
│         ▼                                                       │
│  允许访问                                                       │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

**执行权限检查**

对于代码段，需要检查执行权限：

```c
// 执行权限检查（通常在页错误处理之外）
if(!(region->flags & VR_EXECUTABLE) && is_instruction_fetch(addr)) {
    // 尝试执行不可执行区域
    sys_kill(vmp->vm_endpoint, SIGSEGV);
}
```

**共享内存权限**

```c
// 共享内存权限由创建时的标志决定
// shm_open() 或 mmap() 时指定
int prot = PROT_READ | PROT_WRITE;  // 用户请求的权限

// VM 检查
if((prot & PROT_WRITE) && !(region->flags & VR_WRITABLE)) {
    return EACCES;  // 权限被拒绝
}
```

**权限继承**

fork() 时子进程继承父进程的内存权限：

```c
// fork 时复制区域
new_region->flags = old_region->flags;  // 继承权限标志

// 但如果区域可写，需要设置 CoW
if(new_region->flags & VR_WRITABLE) {
    // 页面暂时标记为只读，等待 CoW
}
```

#### 2.3.4 执行处理

执行处理是页错误的核心，根据不同情况执行 CoW 或分配新页。

**处理入口**

```c
// minix3/minix/servers/vm/pagefaults.c
// 计算区域内的偏移
offset = addr - region->vaddr;

// 调用 map_pf 处理
result = map_pf(vmp, region, offset, wr, pf_cont, &state, sizeof(state), &io);
```

**map_pf 处理逻辑**

```c
// minix3/minix/servers/vm/region.c
int map_pf(struct vmproc *vmp,
    struct vir_region *region,
    vir_bytes offset,
    int write,
    vfs_callback_t pf_callback,
    void *state,
    int len,
    int *io)
{
    struct phys_region *ph;
    int r = OK;

    offset -= offset % VM_PAGE_SIZE;
    assert(offset < region->length);

    // 1. 获取或创建物理区域
    if(!(ph = physblock_get(region, offset))) {
        struct phys_block *pb;

        // 创建新的物理块
        if(!(pb = pb_new(MAP_NONE))) {
            return ENOMEM;
        }

        // 引用物理块
        if(!(ph = pb_reference(pb, offset, region, region->def_memtype))) {
            pb_free(pb);
            return ENOMEM;
        }
    }

    // 2. 检查是否需要处理
    if(!write || !ph->memtype->writable(ph)) {
        // 调用内存类型的 pagefault 处理器
        if((r = ph->memtype->ev_pagefault(vmp, region, ph, write,
            pf_callback, state, len, io)) == SUSPEND) {
            return SUSPEND;
        }

        if(r != OK) {
            pb_unreferenced(region, ph, 1);
            return r;
        }
    }

    // 3. 更新页表
    if((r = map_ph_writept(vmp, region, ph)) != OK) {
        return r;
    }

    return r;
}
```

**处理决策流程**

```
┌─────────────────────────────────────────────────────────────────┐
│                    执行处理决策流程                               │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  输入: region, offset, write                                    │
│         │                                                       │
│         ▼                                                       │
│  ┌──────────────────────┐                                       │
│  │ physblock_get()      │                                       │
│  │ 获取物理区域          │                                       │
│  └──────────────────────┘                                       │
│         │                                                       │
│         ├─────► 未找到 ──► 创建新物理块                         │
│         │                   │                                   │
│         │                   ▼                                   │
│         │              pb_new(MAP_NONE)                         │
│         │              pb_reference()                           │
│         │                                                       │
│         ▼ 找到物理区域 ph                                       │
│  ┌──────────────────────┐                                       │
│  │ 检查是否需要处理      │                                       │
│  │ !write || !writable? │                                       │
│  └──────────────────────┘                                       │
│         │                                                       │
│         ├─────► 否 ──► 跳过处理                                 │
│         │                                                       │
│         ▼ 是                                                    │
│  ┌──────────────────────┐                                       │
│  │ memtype->ev_pagefault│                                       │
│  │ 调用内存类型处理器    │                                       │
│  └──────────────────────┘                                       │
│         │                                                       │
│         ├─────► SUSPEND ──► 等待异步操作                        │
│         │                                                       │
│         ├─────► 错误 ──► pb_unreferenced, 返回错误              │
│         │                                                       │
│         ▼ OK                                                    │
│  ┌──────────────────────┐                                       │
│  │ map_ph_writept()     │                                       │
│  │ 更新页表              │                                       │
│  └──────────────────────┘                                       │
│         │                                                       │
│         ▼                                                       │
│  返回 OK                                                        │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

**不同内存类型的处理**

| 内存类型 | 处理函数 | 行为 |
|---------|---------|------|
| 匿名内存 | `anon_pagefault()` | CoW 或分配新页 |
| 文件映射 | `mappedfile_pagefault()` | 从文件加载 |
| 共享内存 | `shared_pagefault()` | 映射共享页 |
| 直接物理 | `direct_pagefault()` | 直接映射 |

**CoW 处理详解**

```c
// minix3/minix/servers/vm/mem_anon.c
static int anon_pagefault(struct vmproc *vmp, struct vir_region *region,
    struct phys_region *ph, int write, ...)
{
    phys_bytes new_page, new_page_cl;

    // 分配新页面
    if((new_page_cl = alloc_mem(1, allocflags)) == NO_MEM) {
        return ENOMEM;
    }
    new_page = CLICK2ABS(new_page_cl);

    // 情况1: 物理块未分配
    if(ph->ph->phys == MAP_NONE) {
        ph->ph->phys = new_page;
        return OK;
    }

    // 情况2: 不需要 CoW
    if(ph->ph->refcount < 2 || !write) {
        return OK;
    }

    // 情况3: 执行 CoW
    return mem_cow(region, ph, new_page_cl, new_page);
}
```

**mem_cow 实现**

```c
// minix3/minix/servers/vm/mem_anon.c
int mem_cow(struct vir_region *region, struct phys_region *ph,
    phys_clicks new_page_cl, phys_bytes new_page)
{
    // 1. 复制原页面内容到新页面
    memcpy((void *)new_page, (void *)ph->ph->phys, VM_PAGE_SIZE);

    // 2. 减少原页面引用计数
    ph->ph->refcount--;

    // 3. 设置新物理块
    ph->ph->phys = new_page;

    // 4. 更新页表为可写
    // (由 map_ph_writept 完成)

    return OK;
}
```

**页表更新**

```c
// minix3/minix/servers/vm/region.c
int map_ph_writept(struct vmproc *vmp, struct vir_region *region,
    struct phys_region *ph)
{
    int flags = 0;

    // 设置页表权限
    if(region->flags & VR_WRITABLE && ph->memtype->writable(ph))
        flags |= PTF_WRITE;
    if(!(region->flags & VR_KERNEL))
        flags |= PTF_USER;

    // 更新页表项
    return pt_writemap(&vmp->vm_pt, region->vaddr + ph->offset,
        ph->ph->phys, VM_PAGE_SIZE, flags, PTF_PRESENT);
}
```

### 2.4 错误处理

#### 2.4.1 非法访问

当检测到非法内存访问时，VM 向进程发送 SIGSEGV 信号终止进程。

**非法访问类型**

| 类型 | 条件 | 错误信息 |
|------|------|---------|
| 地址无效 | `map_lookup()` 返回 NULL | "bad addr" |
| 写只读区域 | `!(region->flags & VR_WRITABLE) && write` | "ro map" |
| 保护错误 | `PFERR_PROT(err)` 且区域不存在 | "protected addr" |
| 处理失败 | `map_pf()` 返回非 OK | "pagefault not handled" |

**错误处理代码**

```c
// minix3/minix/servers/vm/pagefaults.c

// 情况1: 地址不在任何区域内
if(!(region = map_lookup(vmp, addr, NULL))) {
    if(PFERR_PROT(err)) {
        printf("VM: pagefault: SIGSEGV %d protected addr 0x%lx; %s\n",
            ep, addr, pf_errstr(err));
    } else {
        assert(PFERR_NOPAGE(err));
        printf("VM: pagefault: SIGSEGV %d bad addr 0x%lx; %s\n",
                ep, addr, pf_errstr(err));
        sys_diagctl_stacktrace(ep);  // 打印调用栈帮助调试
    }
    sys_kill(vmp->vm_endpoint, SIGSEGV);
    sys_vmctl(ep, VMCTL_CLEAR_PAGEFAULT, 0);
    return;
}

// 情况2: 写只读区域
if(!(region->flags & VR_WRITABLE) && wr) {
    printf("VM: pagefault: SIGSEGV %d ro map 0x%lx %s\n",
            ep, addr, pf_errstr(err));
    sys_kill(vmp->vm_endpoint, SIGSEGV);
    sys_vmctl(ep, VMCTL_CLEAR_PAGEFAULT, 0);
    return;
}

// 情况3: 页错误处理失败
if(result != OK) {
    printf("VM: pagefault: SIGSEGV %d pagefault not handled\n", ep);
    sys_kill(ep, SIGSEGV);
    sys_vmctl(ep, VMCTL_CLEAR_PAGEFAULT, 0);
    return;
}
```

**SIGSEGV 信号处理流程**

```
┌─────────────────────────────────────────────────────────────────┐
│                   SIGSEGV 处理流程                               │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  VM 检测到非法访问                                               │
│         │                                                       │
│         ▼                                                       │
│  ┌──────────────────┐                                           │
│  │ 打印错误信息      │  printf("VM: pagefault: SIGSEGV ...")    │
│  │ 可选: 打印调用栈  │  sys_diagctl_stacktrace(ep)              │
│  └──────────────────┘                                           │
│         │                                                       │
│         ▼                                                       │
│  ┌──────────────────┐                                           │
│  │ sys_kill(ep, SIGSEGV)                                        │
│  │ 发送段错误信号     │                                          │
│  └──────────────────┘                                           │
│         │                                                       │
│         ▼                                                       │
│  ┌──────────────────┐                                           │
│  │ sys_vmctl(VMCTL_CLEAR_PAGEFAULT)                             │
│  │ 清除页错误状态     │                                          │
│  └──────────────────┘                                           │
│         │                                                       │
│         ▼                                                       │
│  内核接收 SIGSEGV                                                │
│         │                                                       │
│         ▼                                                       │
│  ┌──────────────────┐                                           │
│  │ 检查进程信号处理器 │                                          │
│  └──────────────────┘                                           │
│         │                                                       │
│         ├─────► 有处理器 ──► 执行用户定义处理器                  │
│         │                                                       │
│         ├─────► 无处理器 ──► 终止进程，生成 core dump            │
│         │                                                       │
│         ▼                                                       │
│  进程终止                                                       │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

**调试信息输出**

```c
// 错误码字符串化
char *pf_errstr(u32_t err)
{
    static char buf[100];

    snprintf(buf, sizeof(buf), "err 0x%lx ", (long)err);
    if(PFERR_NOPAGE(err)) strcat(buf, "nopage ");
    if(PFERR_PROT(err))   strcat(buf, "protection ");
    if(PFERR_WRITE(err))  strcat(buf, "write");
    if(PFERR_READ(err))   strcat(buf, "read");

    return buf;
}

// 示例输出:
// VM: pagefault: SIGSEGV 1234 bad addr 0x0; err 0x4 nopage read
// VM: pagefault: SIGSEGV 1234 ro map 0x400000 err 0x6 protection write
```

**常见非法访问示例**

```c
// 1. 空指针解引用
int *ptr = NULL;
*ptr = 42;  // SIGSEGV: bad addr 0x0

// 2. 写只读内存
const char *str = "hello";
str[0] = 'H';  // SIGSEGV: ro map (写只读代码段)

// 3. 越界访问
int arr[10];
arr[1000000] = 42;  // SIGSEGV: bad addr (超出映射区域)

// 4. 栈溢出
void recursive() {
    char buf[1024];
    recursive();  // SIGSEGV: bad addr (栈溢出)
}
```

**sys_kill 实现**

```c
// VM 调用内核发送信号
int sys_kill(endpoint_t ep, int signo)
{
    message m;

    memset(&m, 0, sizeof(m));
    m.m_source = VM_PROC_NR;
    m.m_type = SYS_KILL;
    m.KS_ENDPOINT = ep;
    m.KS_SIGNO = signo;

    return _taskcall(SYSTASK, &m);
}
```

**VMCTL_CLEAR_PAGEFAULT**

```c
// 清除页错误状态，允许进程继续执行（或被信号终止）
int sys_vmctl(endpoint_t ep, int request, int value)
{
    message m;

    memset(&m, 0, sizeof(m));
    m.m_source = VM_PROC_NR;
    m.m_type = request;
    m.VMCTL_ENDPT = ep;
    m.VMCTL_VALUE = value;

    return _taskcall(SYSTASK, &m);
}
```

#### 2.4.2 内存不足

当物理内存耗尽时，页错误处理会失败，VM 需要妥善处理 OOM（Out of Memory）情况。

**内存不足场景**

| 场景 | 函数 | 返回值 |
|------|------|--------|
| 分配新页面 | `alloc_mem()` | `NO_MEM` |
| 创建物理块 | `pb_new()` | `NULL` |
| 引用物理块 | `pb_reference()` | `NULL` |
| CoW 复制 | `mem_cow()` | `ENOMEM` |

**错误处理代码**

```c
// minix3/minix/servers/vm/mem_anon.c
static int anon_pagefault(struct vmproc *vmp, struct vir_region *region,
    struct phys_region *ph, int write, ...)
{
    phys_bytes new_page, new_page_cl;

    // 尝试分配新页面
    if((new_page_cl = alloc_mem(1, allocflags)) == NO_MEM) {
        printf("anon_pagefault: out of memory\n");
        return ENOMEM;  // 返回错误，由上层处理
    }
    // ...
}

// minix3/minix/servers/vm/region.c
int map_pf(struct vmproc *vmp, ...)
{
    // 创建新的物理块
    if(!(pb = pb_new(MAP_NONE))) {
        printf("map_pf: pb_new failed\n");
        return ENOMEM;
    }

    // 引用物理块
    if(!(ph = pb_reference(pb, offset, region, region->def_memtype))) {
        printf("map_pf: pb_reference failed\n");
        pb_free(pb);
        return ENOMEM;
    }
    // ...
}
```

**OOM 处理流程**

```
┌─────────────────────────────────────────────────────────────────┐
│                     OOM 处理流程                                 │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  页错误处理中分配内存失败                                         │
│         │                                                       │
│         ▼                                                       │
│  ┌──────────────────┐                                           │
│  │ 返回 ENOMEM       │                                          │
│  └──────────────────┘                                           │
│         │                                                       │
│         ▼                                                       │
│  ┌──────────────────┐                                           │
│  │ handle_pagefault │                                           │
│  │ 检查 result != OK │                                          │
│  └──────────────────┘                                           │
│         │                                                       │
│         ▼                                                       │
│  ┌──────────────────┐                                           │
│  │ 打印错误信息      │  printf("pagefault not handled")         │
│  └──────────────────┘                                           │
│         │                                                       │
│         ▼                                                       │
│  ┌──────────────────┐                                           │
│  │ 发送 SIGSEGV     │  sys_kill(ep, SIGSEGV)                    │
│  └──────────────────┘                                           │
│         │                                                       │
│         ▼                                                       │
│  进程被终止                                                     │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

**Minix3 的 OOM 策略**

Minix3 采用简单策略：内存不足时终止触发页错误的进程。

```c
// minix3/minix/servers/vm/pagefaults.c
if(result != OK) {
    printf("VM: pagefault: SIGSEGV %d pagefault not handled\n", ep);
    sys_kill(ep, SIGSEGV);
    sys_vmctl(ep, VMCTL_CLEAR_PAGEFAULT, 0);
    return;
}
```

**可能的改进策略**

```
┌─────────────────────────────────────────────────────────────────┐
│                    OOM 改进策略                                  │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  1. 页面回收 (Page Reclamation)                                 │
│     - 释放最近最少使用的页面                                     │
│     - 丢弃干净的文件映射页                                       │
│     - 压缩内存中的数据                                           │
│                                                                 │
│  2. 交换 (Swapping)                                             │
│     - 将页面换出到磁盘                                           │
│     - 释放物理内存                                               │
│     - 需要时再换入                                               │
│                                                                 │
│  3. OOM Killer (Linux 风格)                                     │
│     - 选择一个进程终止                                           │
│     - 优先选择内存占用大的进程                                   │
│     - 保护关键系统进程                                           │
│                                                                 │
│  4. 内存压缩 (Memory Compaction)                                │
│     - 整理内存碎片                                               │
│     - 合并空闲页面                                               │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

**内存压力检测**

```c
// 检查可用内存
int check_memory_pressure(void)
{
    phys_clicks free = free_pages();
    phys_clicks total = total_pages();
    
    // 计算空闲比例
    int free_percent = (free * 100) / total;
    
    if(free_percent < 5) {
        return MEMORY_CRITICAL;
    } else if(free_percent < 15) {
        return MEMORY_LOW;
    } else if(free_percent < 30) {
        return MEMORY_MODERATE;
    }
    
    return MEMORY_NORMAL;
}
```

**预留内存**

```c
// 为关键操作预留内存
#define RESERVED_PAGES  16  // 预留页面数

int alloc_mem_safe(phys_clicks clicks)
{
    // 检查预留内存
    if(free_pages() - clicks < RESERVED_PAGES) {
        return NO_MEM;  // 保护预留内存
    }
    
    return alloc_mem(clicks, 0);
}
```

**错误传播**

```c
// 错误从底层向上传播
anon_pagefault() → ENOMEM
       ↓
map_pf() → ENOMEM
       ↓
handle_pagefault() → result != OK
       ↓
sys_kill(ep, SIGSEGV)
```

**日志记录**

```c
// 记录 OOM 事件
void log_oom_event(endpoint_t ep, vir_bytes addr)
{
    struct vmproc *vmp;
    int p;
    
    if(vm_isokendpt(ep, &p) == OK) {
        vmp = &vmproc[p];
        printf("VM: OOM: process %d (%s) at addr 0x%lx\n",
            ep, vmp->vm_proc_name, addr);
        printf("VM: free memory: %lu KB\n", 
            (unsigned long)free_pages() * (VM_PAGE_SIZE / 1024));
    }
}
```

---

## 3. Rust 设计决策

### 3.1 安全封装

Rust 的类型系统为内核态页错误处理提供了安全保障，避免 C 代码中常见的内存安全问题。

**C 代码的安全问题**

```c
// C 代码中的潜在问题
void handle_pagefault(endpoint_t ep, vir_bytes addr, u32_t err) {
    struct vmproc *vmp = &vmproc[p];  // 可能越界
    struct vir_region *region = map_lookup(vmp, addr, NULL);
    
    // region 可能为 NULL，但后续代码可能忘记检查
    offset = addr - region->vaddr;  // 如果 region == NULL，崩溃
    
    // 物理地址可能被错误计算
    phys_bytes phys = ph->ph->phys;  // 可能是 MAP_NONE
    memcpy((void *)phys, ...);  // 如果 phys 无效，崩溃
}
```

**Rust 安全封装设计**

```rust
/// 页错误类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageFaultType {
    /// 页不存在 (Not Present)
    NotPresent,
    /// 保护错误 (Protection Fault)
    Protection,
}

/// 页错误访问类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessType {
    Read,
    Write,
    Execute,
}

/// 页错误信息
#[derive(Debug)]
pub struct PageFaultInfo {
    /// 触发错误的进程端点
    pub endpoint: Endpoint,
    /// 触发错误的虚拟地址
    pub vaddr: VirtAddr,
    /// 错误类型
    pub fault_type: PageFaultType,
    /// 访问类型
    pub access_type: AccessType,
    /// CPU 错误码
    pub error_code: u32,
}

impl PageFaultInfo {
    /// 从 CPU 错误码解析页错误信息
    pub fn from_error_code(endpoint: Endpoint, vaddr: VirtAddr, error_code: u32) -> Self {
        let fault_type = if error_code & PFE_PRESENT != 0 {
            PageFaultType::Protection
        } else {
            PageFaultType::NotPresent
        };
        
        let access_type = if error_code & PFE_WRITE != 0 {
            AccessType::Write
        } else if error_code & PFE_EXECUTE != 0 {
            AccessType::Execute
        } else {
            AccessType::Read
        };
        
        Self {
            endpoint,
            vaddr,
            fault_type,
            access_type,
            error_code,
        }
    }
    
    /// 是否是写操作
    pub fn is_write(&self) -> bool {
        self.access_type == AccessType::Write
    }
}

/// 页错误处理结果
#[derive(Debug)]
pub enum PageFaultResult {
    /// 处理成功
    Ok,
    /// 需要等待异步操作
    Suspend,
    /// 地址无效，发送 SIGSEGV
    InvalidAddress,
    /// 权限错误，发送 SIGSEGV
    PermissionDenied,
    /// 内存不足
    OutOfMemory,
}
```

**安全的状态机设计**

```rust
/// 页错误处理状态机
pub enum PageFaultState {
    /// 初始状态
    Initial,
    /// 验证进程
    ValidatingProcess {
        info: PageFaultInfo,
    },
    /// 查找区域
    LookingUpRegion {
        info: PageFaultInfo,
        vmp: VmProcRef,
    },
    /// 检查权限
    CheckingPermission {
        info: PageFaultInfo,
        vmp: VmProcRef,
        region: VirRegionRef,
    },
    /// 处理页面
    HandlingPage {
        info: PageFaultInfo,
        vmp: VmProcRef,
        region: VirRegionRef,
        offset: usize,
    },
    /// 等待异步操作
    WaitingAsync {
        info: PageFaultInfo,
        callback: AsyncCallback,
    },
    /// 完成
    Completed(PageFaultResult),
}

impl PageFaultState {
    /// 执行下一步处理
    pub fn step(self) -> Result<Self, PageFaultError> {
        match self {
            Self::Initial => panic!("Invalid state"),
            
            Self::ValidatingProcess { info } => {
                let vmp = VmProcTable::get_by_endpoint(info.endpoint)?;
                Ok(Self::LookingUpRegion { info, vmp })
            }
            
            Self::LookingUpRegion { info, vmp } => {
                let region = vmp.lookup_region(info.vaddr)
                    .ok_or(PageFaultError::InvalidAddress)?;
                Ok(Self::CheckingPermission { info, vmp, region })
            }
            
            Self::CheckingPermission { info, vmp, region } => {
                if info.is_write() && !region.is_writable() {
                    return Err(PageFaultError::PermissionDenied);
                }
                let offset = info.vaddr - region.vaddr();
                Ok(Self::HandlingPage { info, vmp, region, offset })
            }
            
            Self::HandlingPage { info, vmp, region, offset } => {
                match region.handle_pagefault(&vmp, offset, info.is_write())? {
                    HandleResult::Ok => Ok(Self::Completed(PageFaultResult::Ok)),
                    HandleResult::Suspend(callback) => {
                        Ok(Self::WaitingAsync { info, callback })
                    }
                }
            }
            
            Self::WaitingAsync { info, callback } => {
                if callback.is_ready() {
                    Ok(Self::Completed(PageFaultResult::Ok))
                } else {
                    Ok(Self::WaitingAsync { info, callback })
                }
            }
            
            Self::Completed(_) => Ok(self),
        }
    }
}
```

**安全的物理内存访问**

```rust
/// 物理页引用
pub struct PhysPage {
    addr: PhysAddr,
    size: usize,
}

impl PhysPage {
    /// 安全读取页面内容
    pub fn read(&self, offset: usize, buf: &mut [u8]) -> Result<(), MemoryError> {
        if offset + buf.len() > self.size {
            return Err(MemoryError::OutOfBounds);
        }
        
        // 安全：边界已检查，物理地址有效
        unsafe {
            let src = (self.addr.as_usize() + offset) as *const u8;
            core::ptr::copy_nonoverlapping(src, buf.as_mut_ptr(), buf.len());
        }
        
        Ok(())
    }
    
    /// 安全写入页面内容
    pub fn write(&self, offset: usize, data: &[u8]) -> Result<(), MemoryError> {
        if offset + data.len() > self.size {
            return Err(MemoryError::OutOfBounds);
        }
        
        // 安全：边界已检查，物理地址有效
        unsafe {
            let dst = (self.addr.as_usize() + offset) as *mut u8;
            core::ptr::copy_nonoverlapping(data.as_ptr(), dst, data.len());
        }
        
        Ok(())
    }
}
```

**错误处理**

```rust
/// 页错误处理错误
#[derive(Debug)]
pub enum PageFaultError {
    /// 进程不存在
    ProcessNotFound(Endpoint),
    /// 地址无效
    InvalidAddress,
    /// 权限被拒绝
    PermissionDenied,
    /// 内存不足
    OutOfMemory,
    /// 内部错误
    InternalError(i32),
}

impl From<PageFaultError> for PageFaultResult {
    fn from(err: PageFaultError) -> Self {
        match err {
            PageFaultError::ProcessNotFound(_) |
            PageFaultError::InvalidAddress => PageFaultResult::InvalidAddress,
            PageFaultError::PermissionDenied => PageFaultResult::PermissionDenied,
            PageFaultError::OutOfMemory => PageFaultResult::OutOfMemory,
            PageFaultError::InternalError(_) => PageFaultResult::InvalidAddress,
        }
    }
}
```

### 3.2 与 memtype 的集成

页错误处理通过 MemoryType trait 与不同内存类型实现解耦，实现多态处理。

**MemoryType Trait 扩展**

```rust
/// 内存类型 trait - 页错误处理相关方法
pub trait MemoryType: Send + Sync {
    /// 内存类型名称
    fn name(&self) -> &'static str;
    
    /// 处理页错误
    fn pagefault(
        &self,
        vmp: &mut VmProc,
        region: &mut VirRegion,
        phys_region: &mut PhysRegion,
        write: bool,
    ) -> Result<PageFaultAction, PageFaultError>;
    
    /// 检查是否可写
    fn writable(&self, phys_region: &PhysRegion) -> bool;
    
    /// 是否支持 CoW
    fn supports_cow(&self) -> bool {
        false
    }
    
    /// CoW 目标类型
    fn cow_target_type(&self) -> &'static dyn MemoryType;
}

/// 页错误处理动作
#[derive(Debug)]
pub enum PageFaultAction {
    /// 处理完成
    Done,
    /// 需要等待异步操作
    Suspend(AsyncCallback),
    /// 需要分配新页
    AllocateNewPage {
        /// 是否需要清零
        zero: bool,
    },
    /// 需要执行 CoW
    CopyOnWrite {
        /// 源物理地址
        src_phys: PhysAddr,
    },
}
```

**匿名内存实现**

```rust
/// 匿名内存类型
pub struct AnonymousMemory;

impl MemoryType for AnonymousMemory {
    fn name(&self) -> &'static str {
        "anonymous memory"
    }
    
    fn pagefault(
        &self,
        vmp: &mut VmProc,
        region: &mut VirRegion,
        phys_region: &mut PhysRegion,
        write: bool,
    ) -> Result<PageFaultAction, PageFaultError> {
        let phys_block = phys_region.phys_block();
        
        // 情况1: 物理块未分配
        if phys_block.phys().is_none() {
            return Ok(PageFaultAction::AllocateNewPage { zero: true });
        }
        
        // 情况2: 不需要 CoW
        if phys_block.refcount() < 2 || !write {
            return Ok(PageFaultAction::Done);
        }
        
        // 情况3: 执行 CoW
        let src_phys = phys_block.phys().unwrap();
        Ok(PageFaultAction::CopyOnWrite { src_phys })
    }
    
    fn writable(&self, phys_region: &PhysRegion) -> bool {
        let phys_block = phys_region.phys_block();
        
        // 物理页未分配，不可写
        if phys_block.phys().is_none() {
            return false;
        }
        
        // 有 remaps，可写（但会触发 CoW）
        if phys_region.parent().remaps() > 0 {
            return true;
        }
        
        // 只有当引用计数为 1 时才真正可写
        phys_block.refcount() == 1
    }
    
    fn supports_cow(&self) -> bool {
        true
    }
    
    fn cow_target_type(&self) -> &'static dyn MemoryType {
        &ANONYMOUS_MEMORY
    }
}

/// 全局匿名内存实例
pub static ANONYMOUS_MEMORY: AnonymousMemory = AnonymousMemory;
```

**文件映射内存实现**

```rust
/// 文件映射内存类型
pub struct MappedFileMemory {
    /// 文件系统接口（mock）
    fs: &'static dyn FileSystemOps,
}

impl MemoryType for MappedFileMemory {
    fn name(&self) -> &'static str {
        "mapped file"
    }
    
    fn pagefault(
        &self,
        vmp: &mut VmProc,
        region: &mut VirRegion,
        phys_region: &mut PhysRegion,
        write: bool,
    ) -> Result<PageFaultAction, PageFaultError> {
        let phys_block = phys_region.phys_block();
        
        // 物理块未分配，需要从文件加载
        if phys_block.phys().is_none() {
            let file_info = region.file_info()
                .ok_or(PageFaultError::InternalError(ENODEV))?;
            
            // 创建异步回调
            let callback = self.fs.read_async(
                file_info.inode,
                phys_region.offset(),
                phys_block,
            );
            
            return Ok(PageFaultAction::Suspend(callback));
        }
        
        // 如果是写操作且不是私有映射，需要 CoW
        if write && !region.is_private_mapping() {
            let src_phys = phys_block.phys().unwrap();
            return Ok(PageFaultAction::CopyOnWrite { src_phys });
        }
        
        Ok(PageFaultAction::Done)
    }
    
    fn writable(&self, phys_region: &PhysRegion) -> bool {
        // 文件映射的可写性取决于映射类型
        phys_region.parent().is_writable()
    }
    
    fn supports_cow(&self) -> bool {
        true
    }
    
    fn cow_target_type(&self) -> &'static dyn MemoryType {
        &ANONYMOUS_MEMORY  // CoW 后变为匿名内存
    }
}
```

**共享内存实现**

```rust
/// 共享内存类型
pub struct SharedMemory;

impl MemoryType for SharedMemory {
    fn name(&self) -> &'static str {
        "shared memory"
    }
    
    fn pagefault(
        &self,
        vmp: &mut VmProc,
        region: &mut VirRegion,
        phys_region: &mut PhysRegion,
        write: bool,
    ) -> Result<PageFaultAction, PageFaultError> {
        let phys_block = phys_region.phys_block();
        
        // 共享内存不支持 CoW
        if phys_block.phys().is_none() {
            return Ok(PageFaultAction::AllocateNewPage { zero: true });
        }
        
        Ok(PageFaultAction::Done)
    }
    
    fn writable(&self, phys_region: &PhysRegion) -> bool {
        // 共享内存总是可写的（如果映射时指定了写权限）
        phys_region.parent().is_writable()
    }
    
    fn supports_cow(&self) -> bool {
        false  // 共享内存不支持 CoW
    }
    
    fn cow_target_type(&self) -> &'static dyn MemoryType {
        panic!("Shared memory does not support CoW")
    }
}
```

**页错误处理集成**

```rust
impl VirRegion {
    /// 处理页错误
    pub fn handle_pagefault(
        &mut self,
        vmp: &mut VmProc,
        offset: usize,
        write: bool,
    ) -> Result<HandleResult, PageFaultError> {
        // 获取或创建物理区域
        let phys_region = self.get_or_create_phys_region(offset)?;
        
        // 检查是否需要处理
        if write && phys_region.memtype().writable(&phys_region) {
            return Ok(HandleResult::Ok);
        }
        
        // 调用内存类型的 pagefault 处理器
        let action = phys_region.memtype().pagefault(
            vmp,
            self,
            &mut phys_region,
            write,
        )?;
        
        // 处理动作
        match action {
            PageFaultAction::Done => {
                self.update_pagetable(vmp, &phys_region, write)?;
                Ok(HandleResult::Ok)
            }
            
            PageFaultAction::Suspend(callback) => {
                Ok(HandleResult::Suspend(callback))
            }
            
            PageFaultAction::AllocateNewPage { zero } => {
                let new_page = vmp.alloc_page(zero)?;
                phys_region.phys_block().set_phys(new_page);
                self.update_pagetable(vmp, &phys_region, write)?;
                Ok(HandleResult::Ok)
            }
            
            PageFaultAction::CopyOnWrite { src_phys } => {
                let new_page = vmp.alloc_page(false)?;
                // 复制内容
                new_page.copy_from(&src_phys)?;
                // 更新引用计数
                phys_region.phys_block().dec_refcount();
                phys_region.phys_block().set_phys(new_page);
                self.update_pagetable(vmp, &phys_region, write)?;
                Ok(HandleResult::Ok)
            }
        }
    }
}
```

### 3.3 错误传播

Rust 的 Result 类型提供了清晰的错误传播机制，比 C 的错误码更安全。

**C 的错误传播问题**

```c
// C 代码中错误容易被忽略
int result = map_pf(vmp, region, offset, wr, ...);
// 如果忘记检查 result，程序继续执行可能导致崩溃

// 错误信息丢失
if(result != OK) {
    printf("error: %d\n", result);  // 只有数字，缺乏上下文
}
```

**Rust 错误类型设计**

```rust
/// 页错误处理错误
#[derive(Debug, thiserror::Error)]
pub enum PageFaultError {
    #[error("process not found: {0}")]
    ProcessNotFound(Endpoint),
    
    #[error("invalid address: {0:#x}")]
    InvalidAddress(VirtAddr),
    
    #[error("permission denied for {access:?} at {addr:#x}")]
    PermissionDenied {
        addr: VirtAddr,
        access: AccessType,
    },
    
    #[error("out of memory while handling page fault for process {process}")]
    OutOfMemory {
        process: Endpoint,
    },
    
    #[error("region lookup failed: {0}")]
    RegionLookup(#[from] RegionError),
    
    #[error("physical block error: {0}")]
    PhysBlock(#[from] PhysBlockError),
    
    #[error("page table error: {0}")]
    PageTable(#[from] PageTableError),
    
    #[error("async operation failed: {0}")]
    AsyncError(String),
    
    #[error("internal error: {0}")]
    Internal(i32),
}

/// 区域错误
#[derive(Debug, thiserror::Error)]
pub enum RegionError {
    #[error("region not found at address {0:#x}")]
    NotFound(VirtAddr),
    
    #[error("region split failed: {reason}")]
    SplitFailed { reason: String },
    
    #[error("region merge failed: {reason}")]
    MergeFailed { reason: String },
}

/// 物理块错误
#[derive(Debug, thiserror::Error)]
pub enum PhysBlockError {
    #[error("failed to allocate physical block")]
    AllocationFailed,
    
    #[error("failed to reference physical block")]
    ReferenceFailed,
    
    #[error("physical block has no physical address")]
    NoPhysicalAddress,
    
    #[error("invalid reference count: {0}")]
    InvalidRefcount(u32),
}

/// 页表错误
#[derive(Debug, thiserror::Error)]
pub enum PageTableError {
    #[error("failed to map page at {vaddr:#x} -> {paddr:#x}")]
    MapFailed {
        vaddr: VirtAddr,
        paddr: PhysAddr,
    },
    
    #[error("failed to update page table entry at {0:#x}")]
    UpdateFailed(VirtAddr),
    
    #[error("TLB flush failed for address {0:#x}")]
    TlbFlushFailed(VirtAddr),
}
```

**错误传播示例**

```rust
/// 页错误处理入口
pub fn handle_pagefault(
    info: PageFaultInfo,
) -> Result<PageFaultResult, PageFaultError> {
    // 使用 ? 运算符自动传播错误
    
    // 1. 验证进程
    let vmp = VmProcTable::get_by_endpoint(info.endpoint)
        .map_err(|_| PageFaultError::ProcessNotFound(info.endpoint))?;
    
    // 2. 查找区域
    let region = vmp.lookup_region(info.vaddr)
        .ok_or(PageFaultError::InvalidAddress(info.vaddr))?;
    
    // 3. 检查权限
    if info.is_write() && !region.is_writable() {
        return Err(PageFaultError::PermissionDenied {
            addr: info.vaddr,
            access: info.access_type,
        });
    }
    
    // 4. 处理页面
    let offset = info.vaddr - region.vaddr();
    match region.handle_pagefault(&vmp, offset, info.is_write()) {
        Ok(HandleResult::Ok) => Ok(PageFaultResult::Ok),
        Ok(HandleResult::Suspend(cb)) => Ok(PageFaultResult::Suspend),
        Err(e) => Err(e),
    }
}
```

**错误上下文增强**

```rust
/// 带上下文的错误处理
impl PageFaultHandler {
    pub fn handle(&mut self, info: PageFaultInfo) -> Result<(), PageFaultError> {
        self.handle_inner(info).map_err(|e| {
            // 添加上下文信息
            log::error!(
                "Page fault handling failed for process {} at {:#x}: {}",
                info.endpoint,
                info.vaddr,
                e
            );
            
            // 记录详细状态
            if let Ok(vmp) = VmProcTable::get_by_endpoint(info.endpoint) {
                log::debug!("Process state: {:?}", vmp.state());
                log::debug!("Memory usage: {} KB", vmp.memory_usage() / 1024);
            }
            
            e
        })
    }
}
```

**错误恢复策略**

```rust
impl PageFaultResult {
    /// 执行错误恢复
    pub fn recover(self, info: &PageFaultInfo) -> Result<(), ()> {
        match self {
            Self::Ok => Ok(()),
            
            Self::Suspend => {
                // 等待异步操作完成
                Ok(())
            }
            
            Self::InvalidAddress | Self::PermissionDenied => {
                // 发送 SIGSEGV
                send_signal(info.endpoint, Signal::SIGSEGV);
                Err(())
            }
            
            Self::OutOfMemory => {
                // 尝试释放内存
                if try_free_memory() {
                    // 重试页错误处理
                    log::info!("Memory freed, retrying page fault");
                    Err(())  // 让调用者重试
                } else {
                    // 无法释放内存，终止进程
                    send_signal(info.endpoint, Signal::SIGKILL);
                    Err(())
                }
            }
        }
    }
}
```

**错误链追踪**

```rust
/// 错误链
#[derive(Debug)]
pub struct ErrorChain {
    errors: Vec<(String, PageFaultError)>,
}

impl ErrorChain {
    pub fn new() -> Self {
        Self { errors: Vec::new() }
    }
    
    pub fn push(&mut self, context: &str, error: PageFaultError) {
        self.errors.push((context.to_string(), error));
    }
    
    pub fn last(&self) -> Option<&PageFaultError> {
        self.errors.last().map(|(_, e)| e)
    }
    
    pub fn format_chain(&self) -> String {
        self.errors
            .iter()
            .rev()
            .map(|(ctx, e)| format!("{}: {}", ctx, e))
            .collect::<Vec<_>>()
            .join(" -> ")
    }
}

/// 使用示例
fn handle_with_context(info: PageFaultInfo) -> Result<(), ErrorChain> {
    let mut chain = ErrorChain::new();
    
    match VmProcTable::get_by_endpoint(info.endpoint) {
        Ok(vmp) => {
            match vmp.lookup_region(info.vaddr) {
                Some(region) => {
                    // 继续处理...
                }
                None => {
                    chain.push("region lookup", PageFaultError::InvalidAddress(info.vaddr));
                }
            }
        }
        Err(e) => {
            chain.push("process lookup", PageFaultError::ProcessNotFound(info.endpoint));
        }
    }
    
    Err(chain)
}
```

**与 C 错误码的互操作**

```rust
impl From<PageFaultError> for i32 {
    fn from(err: PageFaultError) -> Self {
        match err {
            PageFaultError::ProcessNotFound(_) => ESRCH,
            PageFaultError::InvalidAddress(_) => EFAULT,
            PageFaultError::PermissionDenied { .. } => EACCES,
            PageFaultError::OutOfMemory { .. } => ENOMEM,
            PageFaultError::RegionLookup(e) => e.into(),
            PageFaultError::PhysBlock(e) => e.into(),
            PageFaultError::PageTable(e) => e.into(),
            PageFaultError::AsyncError(_) => EIO,
            PageFaultError::Internal(code) => code,
        }
    }
}

impl From<RegionError> for i32 {
    fn from(err: RegionError) -> Self {
        match err {
            RegionError::NotFound(_) => EFAULT,
            RegionError::SplitFailed { .. } => ENOMEM,
            RegionError::MergeFailed { .. } => ENOMEM,
        }
    }
}
```

---

## 4. 实现详解

### 4.1 页错误入口

页错误入口负责从内核接收页错误信息并启动处理流程。

**内核到 VM 的消息格式**

```rust
/// 页错误消息（从内核接收）
#[repr(C)]
pub struct PageFaultMessage {
    /// 消息类型
    pub m_type: i32,
    /// 触发页错误的进程端点
    pub m_source: Endpoint,
    /// 触发错误的虚拟地址
    pub vpf_addr: VirtAddr,
    /// CPU 错误码
    pub vpf_flags: u32,
}

/// VM 消息类型
pub const VM_PAGEFAULT: i32 = 1;
```

**页错误处理入口**

```rust
/// 页错误处理器
pub struct PageFaultHandler {
    /// 进程表引用
    proc_table: &'static VmProcTable,
    /// 物理内存分配器
    allocator: &'static dyn PhysMemAlloc,
}

impl PageFaultHandler {
    /// 创建新的页错误处理器
    pub fn new(
        proc_table: &'static VmProcTable,
        allocator: &'static dyn PhysMemAlloc,
    ) -> Self {
        Self { proc_table, allocator }
    }
    
    /// 处理页错误消息
    pub fn handle_message(&mut self, msg: PageFaultMessage) {
        let info = PageFaultInfo::from_error_code(
            msg.m_source,
            msg.vpf_addr,
            msg.vpf_flags,
        );
        
        match self.handle(info) {
            Ok(PageFaultResult::Ok) => {
                // 清除页错误状态
                self.clear_pagefault(msg.m_source);
            }
            Ok(PageFaultResult::Suspend) => {
                // 等待异步操作完成
            }
            Err(e) => {
                log::error!("Page fault error: {}", e);
                // 发送 SIGSEGV
                self.send_signal(msg.m_source, Signal::SIGSEGV);
                self.clear_pagefault(msg.m_source);
            }
        }
    }
    
    /// 处理页错误
    fn handle(&mut self, info: PageFaultInfo) -> Result<PageFaultResult, PageFaultError> {
        // 1. 验证进程
        let vmp = self.proc_table
            .get_by_endpoint(info.endpoint)
            .ok_or(PageFaultError::ProcessNotFound(info.endpoint))?;
        
        // 2. 查找区域
        let region = vmp.lookup_region(info.vaddr)
            .ok_or(PageFaultError::InvalidAddress(info.vaddr))?;
        
        // 3. 检查权限
        if info.is_write() && !region.is_writable() {
            return Err(PageFaultError::PermissionDenied {
                addr: info.vaddr,
                access: info.access_type,
            });
        }
        
        // 4. 计算偏移
        let offset = info.vaddr.align_down(PAGE_SIZE) - region.vaddr();
        
        // 5. 处理页面
        let result = self.handle_page(vmp, region, offset, info.is_write())?;
        
        // 6. 更新缺页统计
        if result.is_io() {
            vmp.increment_major_fault();
        } else {
            vmp.increment_minor_fault();
        }
        
        Ok(result.into())
    }
    
    /// 清除页错误状态
    fn clear_pagefault(&self, endpoint: Endpoint) {
        // 调用内核接口清除页错误
        kernel::vmctl_clear_pagefault(endpoint);
    }
    
    /// 发送信号
    fn send_signal(&self, endpoint: Endpoint, signal: Signal) {
        kernel::sys_kill(endpoint, signal);
    }
}
```

**内核接口（mock）**

```rust
/// 内核接口模块
pub mod kernel {
    use super::*;
    
    /// VM 控制操作
    pub fn vmctl_clear_pagefault(endpoint: Endpoint) {
        // 实际实现会调用内核系统调用
        log::debug!("Clearing pagefault for {}", endpoint);
    }
    
    /// 发送信号
    pub fn sys_kill(endpoint: Endpoint, signal: Signal) {
        log::debug!("Sending signal {:?} to {}", signal, endpoint);
    }
    
    /// 获取内存请求
    pub fn vmctl_get_memreq() -> Option<MemoryRequest> {
        // 从内核获取内存请求
        None
    }
}

/// 内存请求
#[derive(Debug)]
pub struct MemoryRequest {
    pub endpoint: Endpoint,
    pub addr: VirtAddr,
    pub len: usize,
    pub write: bool,
    pub requestor: Endpoint,
}
```

**消息循环**

```rust
impl VmServer {
    /// 主消息循环
    pub fn run(&mut self) {
        loop {
            // 接收消息
            let msg = self.receive_message();
            
            match msg.m_type {
                VM_PAGEFAULT => {
                    let pf_msg = PageFaultMessage {
                        m_type: msg.m_type,
                        m_source: msg.m_source,
                        vpf_addr: msg.vpf_addr,
                        vpf_flags: msg.vpf_flags,
                    };
                    self.pagefault_handler.handle_message(pf_msg);
                }
                
                VM_MEMORY_REQ => {
                    self.handle_memory_request();
                }
                
                _ => {
                    log::warn!("Unknown message type: {}", msg.m_type);
                }
            }
        }
    }
    
    /// 处理内存请求
    fn handle_memory_request(&mut self) {
        while let Some(req) = kernel::vmctl_get_memreq() {
            match self.handle_memory(req) {
                Ok(result) => {
                    kernel::vmctl_memreq_reply(req.requestor, result);
                }
                Err(e) => {
                    log::error!("Memory request failed: {}", e);
                    kernel::vmctl_memreq_reply(req.requestor, EFAULT);
                }
            }
        }
    }
}
```

**异步处理支持**

```rust
/// 异步页错误处理
impl PageFaultHandler {
    /// 处理需要异步操作的页错误
    pub fn handle_async(&mut self, callback: AsyncCallback) {
        // 检查回调是否完成
        if callback.is_ready() {
            // 重试页错误处理
            if let Some(info) = callback.pagefault_info() {
                match self.handle(info) {
                    Ok(PageFaultResult::Ok) => {
                        self.clear_pagefault(info.endpoint);
                    }
                    Err(e) => {
                        log::error!("Async page fault error: {}", e);
                        self.send_signal(info.endpoint, Signal::SIGSEGV);
                        self.clear_pagefault(info.endpoint);
                    }
                    _ => {}
                }
            }
        }
    }
}

/// 异步回调
#[derive(Debug)]
pub struct AsyncCallback {
    /// 页错误信息
    pagefault_info: Option<PageFaultInfo>,
    /// 是否完成
    ready: bool,
}

impl AsyncCallback {
    pub fn new(info: PageFaultInfo) -> Self {
        Self {
            pagefault_info: Some(info),
            ready: false,
        }
    }
    
    pub fn is_ready(&self) -> bool {
        self.ready
    }
    
    pub fn pagefault_info(&self) -> Option<PageFaultInfo> {
        self.pagefault_info.clone()
    }
    
    pub fn complete(&mut self) {
        self.ready = true;
    }
}
```

### 4.2 地址解析

地址解析将虚拟地址转换为对应的虚拟区域和物理区域。

**地址解析结构**

```rust
/// 地址解析结果
#[derive(Debug)]
pub struct AddressResolution {
    /// 虚拟区域引用
    pub region: VirRegionRef,
    /// 区域内偏移（页对齐）
    pub offset: usize,
    /// 物理区域（如果存在）
    pub phys_region: Option<PhysRegionRef>,
}

impl VmProc {
    /// 解析虚拟地址
    pub fn resolve_address(&self, vaddr: VirtAddr) -> Option<AddressResolution> {
        // 1. 查找虚拟区域
        let region = self.lookup_region(vaddr)?;
        
        // 2. 计算区域内偏移
        let offset = vaddr.align_down(PAGE_SIZE) - region.vaddr();
        
        // 3. 查找物理区域
        let phys_region = region.get_phys_region(offset);
        
        Some(AddressResolution {
            region,
            offset,
            phys_region,
        })
    }
    
    /// 查找虚拟区域
    pub fn lookup_region(&self, vaddr: VirtAddr) -> Option<VirRegionRef> {
        self.regions.find(vaddr)
    }
}
```

**AVL 树查找实现**

```rust
impl RegionAvlTree {
    /// 查找包含指定地址的区域
    pub fn find(&self, vaddr: VirtAddr) -> Option<VirRegionRef> {
        // 使用 AVL_LESS_EQUAL 查找
        let candidate = self.search(vaddr, SearchType::LessEqual)?;
        
        // 验证地址确实在区域内
        if vaddr >= candidate.vaddr() && vaddr < candidate.vaddr() + candidate.length() {
            Some(candidate)
        } else {
            None
        }
    }
    
    /// 搜索区域
    fn search(&self, vaddr: VirtAddr, search_type: SearchType) -> Option<VirRegionRef> {
        let mut node = self.root.as_ref()?;
        let mut best: Option<VirRegionRef> = None;
        
        while let Some(current) = node {
            let region = current.data();
            
            if vaddr < region.vaddr() {
                // 地址在当前区域之前
                node = current.left.as_ref();
            } else if vaddr >= region.vaddr() + region.length() {
                // 地址在当前区域之后
                if search_type == SearchType::LessEqual {
                    best = Some(region.clone());
                }
                node = current.right.as_ref();
            } else {
                // 地址在当前区域内
                return Some(region.clone());
            }
        }
        
        best
    }
}

/// 搜索类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchType {
    /// 精确匹配
    Equal,
    /// 小于等于
    LessEqual,
    /// 大于等于
    GreaterEqual,
}
```

**物理区域查找**

```rust
impl VirRegion {
    /// 获取指定偏移处的物理区域
    pub fn get_phys_region(&self, offset: usize) -> Option<PhysRegionRef> {
        // 页对齐偏移
        let page_offset = offset & !(PAGE_SIZE - 1);
        
        // 遍历物理区域链表
        for pr in self.phys_regions.iter() {
            if pr.offset() == page_offset {
                return Some(pr.clone());
            }
            if pr.offset() > page_offset {
                break;  // 物理区域按偏移排序
            }
        }
        
        None
    }
    
    /// 获取或创建物理区域
    pub fn get_or_create_phys_region(
        &mut self,
        offset: usize,
    ) -> Result<PhysRegionRef, PageFaultError> {
        let page_offset = offset & !(PAGE_SIZE - 1);
        
        // 尝试获取现有区域
        if let Some(pr) = self.get_phys_region(offset) {
            return Ok(pr);
        }
        
        // 创建新的物理区域
        self.create_phys_region(page_offset)
    }
    
    /// 创建新的物理区域
    fn create_phys_region(
        &mut self,
        offset: usize,
    ) -> Result<PhysRegionRef, PageFaultError> {
        // 创建物理块
        let phys_block = PhysBlock::new(None)?;
        
        // 创建物理区域
        let phys_region = PhysRegion::new(
            offset,
            phys_block,
            self.default_memtype(),
        );
        
        // 插入到链表（按偏移排序）
        self.phys_regions.insert_sorted(phys_region.clone());
        
        Ok(phys_region)
    }
}
```

**地址解析流程**

```
┌─────────────────────────────────────────────────────────────────┐
│                    地址解析流程                                   │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  输入: vmp, vaddr                                               │
│         │                                                       │
│         ▼                                                       │
│  ┌──────────────────────┐                                       │
│  │ AVL 树查找           │                                       │
│  │ SearchType::LessEqual│                                       │
│  └──────────────────────┘                                       │
│         │                                                       │
│         ├─────► 未找到 ──► 返回 None                            │
│         │                                                       │
│         ▼ 找到候选区域                                          │
│  ┌──────────────────────┐                                       │
│  │ 验证: vaddr >= region.vaddr &&                               │
│  │       vaddr < region.vaddr + region.length                   │
│  └──────────────────────┘                                       │
│         │                                                       │
│         ├─────► 不在区域内 ──► 返回 None                        │
│         │                                                       │
│         ▼ 在区域内                                              │
│  ┌──────────────────────┐                                       │
│  │ 计算偏移:            │                                       │
│  │ offset = vaddr - region.vaddr                                │
│  │ page_offset = offset & ~PAGE_MASK                            │
│  └──────────────────────┘                                       │
│         │                                                       │
│         ▼                                                       │
│  ┌──────────────────────┐                                       │
│  │ 查找物理区域         │                                       │
│  │ 遍历 phys_regions    │                                       │
│  └──────────────────────┘                                       │
│         │                                                       │
│         ├─────► 找到 ──► 返回 AddressResolution                 │
│         │                                                       │
│         ├─────► 未找到 ──► phys_region = None                   │
│         │                                                       │
│         ▼                                                       │
│  返回 AddressResolution { region, offset, phys_region: None }  │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

**地址范围检查**

```rust
impl VmProc {
    /// 检查地址范围是否有效
    pub fn check_address_range(
        &self,
        start: VirtAddr,
        len: usize,
        write: bool,
    ) -> Result<(), PageFaultError> {
        let mut current = start;
        let end = start + len;
        
        while current < end {
            // 查找区域
            let region = self.lookup_region(current)
                .ok_or(PageFaultError::InvalidAddress(current))?;
            
            // 检查权限
            if write && !region.is_writable() {
                return Err(PageFaultError::PermissionDenied {
                    addr: current,
                    access: AccessType::Write,
                });
            }
            
            // 移动到下一个区域
            current = region.vaddr() + region.length();
        }
        
        Ok(())
    }
}
```

**地址缓存优化**

```rust
/// 地址解析缓存
pub struct AddressCache {
    /// 最近访问的区域
    recent_region: Option<VirRegionRef>,
    /// 缓存命中率统计
    hits: u64,
    misses: u64,
}

impl AddressCache {
    pub fn new() -> Self {
        Self {
            recent_region: None,
            hits: 0,
            misses: 0,
        }
    }
    
    /// 尝试从缓存获取
    pub fn try_get(&mut self, vaddr: VirtAddr) -> Option<VirRegionRef> {
        if let Some(ref region) = self.recent_region {
            if vaddr >= region.vaddr() && vaddr < region.vaddr() + region.length() {
                self.hits += 1;
                return Some(region.clone());
            }
        }
        self.misses += 1;
        None
    }
    
    /// 更新缓存
    pub fn update(&mut self, region: VirRegionRef) {
        self.recent_region = Some(region);
    }
    
    /// 获取命中率
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }
}

impl VmProc {
    /// 带缓存的地址解析
    pub fn resolve_address_cached(
        &self,
        vaddr: VirtAddr,
        cache: &mut AddressCache,
    ) -> Option<AddressResolution> {
        // 尝试缓存
        if let Some(region) = cache.try_get(vaddr) {
            let offset = vaddr.align_down(PAGE_SIZE) - region.vaddr();
            let phys_region = region.get_phys_region(offset);
            return Some(AddressResolution {
                region,
                offset,
                phys_region,
            });
        }
        
        // 正常查找
        let result = self.resolve_address(vaddr);
        if let Some(ref resolution) = result {
            cache.update(resolution.region.clone());
        }
        
        result
    }
}
```

### 4.3 CoW 处理路径

CoW 处理路径负责处理写保护错误，实现写时复制。

**CoW 处理器**

```rust
/// CoW 处理器
pub struct CowHandler {
    /// 物理内存分配器
    allocator: &'static dyn PhysMemAlloc,
}

impl CowHandler {
    /// 创建新的 CoW 处理器
    pub fn new(allocator: &'static dyn PhysMemAlloc) -> Self {
        Self { allocator }
    }
    
    /// 处理 CoW
    pub fn handle(
        &self,
        region: &mut VirRegion,
        phys_region: &mut PhysRegion,
    ) -> Result<(), PageFaultError> {
        let phys_block = phys_region.phys_block();
        
        // 检查是否需要 CoW
        if phys_block.refcount() <= 1 {
            // 只有一个引用，无需 CoW
            return Ok(());
        }
        
        // 执行 CoW
        self.do_cow(region, phys_region)
    }
    
    /// 执行 CoW 复制
    fn do_cow(
        &self,
        region: &VirRegion,
        phys_region: &mut PhysRegion,
    ) -> Result<(), PageFaultError> {
        let old_phys = phys_region.phys_block().phys()
            .ok_or(PageFaultError::PhysBlock(PhysBlockError::NoPhysicalAddress))?;
        
        // 分配新页面
        let new_page = self.allocator.alloc(1, AllocFlags::empty())
            .map_err(|_| PageFaultError::OutOfMemory {
                process: region.owner(),
            })?;
        
        // 复制内容
        new_page.copy_from(&old_phys)?;
        
        // 减少原页面引用计数
        phys_region.phys_block().dec_refcount();
        
        // 设置新物理地址
        phys_region.phys_block().set_phys(new_page);
        
        log::debug!(
            "CoW: copied page from {:#x} to {:#x}",
            old_phys,
            new_page
        );
        
        Ok(())
    }
}
```

**CoW 处理流程**

```
┌─────────────────────────────────────────────────────────────────┐
│                    CoW 处理流程                                  │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  输入: region, phys_region, write=true                          │
│         │                                                       │
│         ▼                                                       │
│  ┌──────────────────────┐                                       │
│  │ 检查 refcount        │                                       │
│  └──────────────────────┘                                       │
│         │                                                       │
│         ├─────► refcount == 1 ──► 无需 CoW，返回 Ok             │
│         │                                                       │
│         ▼ refcount > 1                                          │
│  ┌──────────────────────┐                                       │
│  │ 获取原物理地址       │                                       │
│  │ old_phys = ph->phys  │                                       │
│  └──────────────────────┘                                       │
│         │                                                       │
│         ▼                                                       │
│  ┌──────────────────────┐                                       │
│  │ 分配新页面           │                                       │
│  │ alloc_mem(1)         │                                       │
│  └──────────────────────┘                                       │
│         │                                                       │
│         ├─────► 失败 ──► 返回 OutOfMemory                       │
│         │                                                       │
│         ▼ 成功                                                  │
│  ┌──────────────────────┐                                       │
│  │ 复制页面内容         │                                       │
│  │ memcpy(new, old)     │                                       │
│  └──────────────────────┘                                       │
│         │                                                       │
│         ▼                                                       │
│  ┌──────────────────────┐                                       │
│  │ 减少原页面引用计数   │                                       │
│  │ refcount--           │                                       │
│  └──────────────────────┘                                       │
│         │                                                       │
│         ▼                                                       │
│  ┌──────────────────────┐                                       │
│  │ 设置新物理地址       │                                       │
│  │ ph->phys = new_page  │                                       │
│  └──────────────────────┘                                       │
│         │                                                       │
│         ▼                                                       │
│  返回 Ok                                                        │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

**页面复制实现**

```rust
impl PhysPage {
    /// 从另一个物理页复制内容
    pub fn copy_from(&self, src: &PhysAddr) -> Result<(), MemoryError> {
        // 安全：两个物理地址都有效，大小相同
        unsafe {
            let dst_ptr = self.addr.as_usize() as *mut u8;
            let src_ptr = src.as_usize() as *const u8;
            core::ptr::copy_nonoverlapping(src_ptr, dst_ptr, PAGE_SIZE);
        }
        Ok(())
    }
    
    /// 清零页面
    pub fn zero(&self) -> Result<(), MemoryError> {
        unsafe {
            let ptr = self.addr.as_usize() as *mut u8;
            core::ptr::write_bytes(ptr, 0, PAGE_SIZE);
        }
        Ok(())
    }
}
```

**引用计数管理**

```rust
impl PhysBlock {
    /// 增加引用计数
    pub fn inc_refcount(&self) {
        self.refcount.fetch_add(1, Ordering::SeqCst);
    }
    
    /// 减少引用计数
    pub fn dec_refcount(&self) -> u32 {
        let old = self.refcount.fetch_sub(1, Ordering::SeqCst);
        if old == 1 {
            // 引用计数降为 0，释放物理内存
            self.free();
        }
        old - 1
    }
    
    /// 获取引用计数
    pub fn refcount(&self) -> u32 {
        self.refcount.load(Ordering::SeqCst)
    }
    
    /// 释放物理内存
    fn free(&self) {
        if let Some(phys) = self.phys {
            // 通知分配器释放
            PHYS_ALLOCATOR.free(phys);
            self.phys = None;
        }
    }
}
```

**CoW 统计**

```rust
/// CoW 统计信息
#[derive(Debug, Default)]
pub struct CowStats {
    /// CoW 触发次数
    pub cow_count: u64,
    /// 跳过 CoW 次数（refcount == 1）
    pub skip_count: u64,
    /// CoW 失败次数
    pub fail_count: u64,
    /// 复制的总字节数
    pub bytes_copied: u64,
}

impl CowStats {
    /// 记录 CoW 事件
    pub fn record_cow(&mut self) {
        self.cow_count += 1;
        self.bytes_copied += PAGE_SIZE as u64;
    }
    
    /// 记录跳过事件
    pub fn record_skip(&mut self) {
        self.skip_count += 1;
    }
    
    /// 记录失败事件
    pub fn record_fail(&mut self) {
        self.fail_count += 1;
    }
}
```

**与页表更新集成**

```rust
impl VirRegion {
    /// 更新页表（CoW 后）
    pub fn update_pagetable_after_cow(
        &self,
        vmp: &VmProc,
        phys_region: &PhysRegion,
    ) -> Result<(), PageTableError> {
        let vaddr = self.vaddr() + phys_region.offset();
        let phys = phys_region.phys_block().phys()
            .ok_or(PageTableError::UpdateFailed(vaddr))?;
        
        // 更新页表项为可写
        let flags = PageTableFlags::PRESENT | PageTableFlags::USER | PageTableFlags::WRITABLE;
        
        vmp.page_table().map(vaddr, phys, flags)?;
        
        // 刷新 TLB
        vmp.page_table().flush_tlb(vaddr);
        
        Ok(())
    }
}
```

### 4.4 按需加载路径

按需加载路径处理首次访问未映射页面的情况，分配新页并清零。

**按需加载处理器**

```rust
/// 按需加载处理器
pub struct DemandLoadHandler {
    /// 物理内存分配器
    allocator: &'static dyn PhysMemAlloc,
}

impl DemandLoadHandler {
    /// 创建新的按需加载处理器
    pub fn new(allocator: &'static dyn PhysMemAlloc) -> Self {
        Self { allocator }
    }
    
    /// 处理按需加载
    pub fn handle(
        &self,
        region: &VirRegion,
        phys_region: &mut PhysRegion,
    ) -> Result<(), PageFaultError> {
        let phys_block = phys_region.phys_block();
        
        // 检查是否已分配物理页
        if phys_block.phys().is_some() {
            return Ok(());  // 已分配，无需处理
        }
        
        // 分配新页面
        let new_page = self.allocator.alloc(1, AllocFlags::CLEAR)
            .map_err(|_| PageFaultError::OutOfMemory {
                process: region.owner(),
            })?;
        
        // 设置物理地址
        phys_block.set_phys(new_page);
        
        log::debug!(
            "Demand load: allocated page at {:#x} for region {:#x}+{:#x}",
            new_page,
            region.vaddr(),
            phys_region.offset()
        );
        
        Ok(())
    }
}
```

**分配标志**

```rust
/// 内存分配标志
bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct AllocFlags: u32 {
        /// 清零分配的内存
        const CLEAR = 0x01;
        /// 对齐到 64KB 边界
        const ALIGN64K = 0x02;
        /// 在低 16MB 范围内
        const LOWER16MB = 0x04;
        /// 在低 1MB 范围内
        const LOWER1MB = 0x08;
    }
}

impl VirRegion {
    /// 获取分配标志
    pub fn alloc_flags(&self) -> AllocFlags {
        let mut flags = AllocFlags::empty();
        
        if !self.flags().contains(RegionFlags::UNINITIALIZED) {
            flags |= AllocFlags::CLEAR;
        }
        if self.flags().contains(RegionFlags::PHYS64K) {
            flags |= AllocFlags::ALIGN64K;
        }
        if self.flags().contains(RegionFlags::LOWER16MB) {
            flags |= AllocFlags::LOWER16MB;
        }
        if self.flags().contains(RegionFlags::LOWER1MB) {
            flags |= AllocFlags::LOWER1MB;
        }
        
        flags
    }
}
```

**按需加载流程**

```
┌─────────────────────────────────────────────────────────────────┐
│                    按需加载流程                                  │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  输入: region, phys_region                                      │
│         │                                                       │
│         ▼                                                       │
│  ┌──────────────────────┐                                       │
│  │ 检查 phys 是否已分配 │                                       │
│  └──────────────────────┘                                       │
│         │                                                       │
│         ├─────► 已分配 ──► 返回 Ok                              │
│         │                                                       │
│         ▼ 未分配                                                │
│  ┌──────────────────────┐                                       │
│  │ 获取分配标志         │                                       │
│  │ alloc_flags()        │                                       │
│  └──────────────────────┘                                       │
│         │                                                       │
│         ▼                                                       │
│  ┌──────────────────────┐                                       │
│  │ 分配物理页面         │                                       │
│  │ alloc_mem(1, flags)  │                                       │
│  └──────────────────────┘                                       │
│         │                                                       │
│         ├─────► 失败 ──► 返回 OutOfMemory                       │
│         │                                                       │
│         ▼ 成功                                                  │
│  ┌──────────────────────┐                                       │
│  │ 设置物理地址         │                                       │
│  │ phys_block.phys = new│                                       │
│  └──────────────────────┘                                       │
│         │                                                       │
│         ▼                                                       │
│  返回 Ok                                                        │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

**文件映射按需加载**

```rust
/// 文件映射按需加载处理器
pub struct FileDemandLoader {
    /// 文件系统接口
    fs: &'static dyn FileSystemOps,
}

impl FileDemandLoader {
    /// 处理文件映射按需加载
    pub fn handle(
        &self,
        region: &VirRegion,
        phys_region: &mut PhysRegion,
    ) -> Result<LoadResult, PageFaultError> {
        let phys_block = phys_region.phys_block();
        
        // 检查是否已加载
        if phys_block.phys().is_some() {
            return Ok(LoadResult::Done);
        }
        
        // 获取文件信息
        let file_info = region.file_info()
            .ok_or(PageFaultError::Internal(ENODEV))?;
        
        // 分配物理页
        let new_page = PHYS_ALLOCATOR.alloc(1, AllocFlags::empty())
            .map_err(|_| PageFaultError::OutOfMemory {
                process: region.owner(),
            })?;
        
        // 从文件读取内容
        let offset = phys_region.offset();
        let result = self.fs.read_sync(
            file_info.inode,
            file_info.offset + offset,
            new_page,
            PAGE_SIZE,
        );
        
        match result {
            Ok(bytes_read) => {
                // 如果读取不足，清零剩余部分
                if bytes_read < PAGE_SIZE {
                    new_page.zero_range(bytes_read, PAGE_SIZE - bytes_read)?;
                }
                
                phys_block.set_phys(new_page);
                Ok(LoadResult::Done)
            }
            Err(e) => {
                // 读取失败，释放页面
                PHYS_ALLOCATOR.free(new_page);
                Err(PageFaultError::AsyncError(format!("File read failed: {}", e)))
            }
        }
    }
}

/// 加载结果
#[derive(Debug)]
pub enum LoadResult {
    /// 加载完成
    Done,
    /// 需要异步等待
    Pending(AsyncLoadHandle),
}

/// 异步加载句柄
#[derive(Debug)]
pub struct AsyncLoadHandle {
    /// 请求 ID
    request_id: u64,
    /// 是否完成
    completed: bool,
}
```

**统计信息**

```rust
/// 按需加载统计
#[derive(Debug, Default)]
pub struct DemandLoadStats {
    /// 分配的页面数
    pub pages_allocated: u64,
    /// 清零的页面数
    pub pages_zeroed: u64,
    /// 从文件加载的页面数
    pub pages_loaded: u64,
    /// 加载失败次数
    pub load_failures: u64,
    /// 总分配字节数
    pub bytes_allocated: u64,
}

impl DemandLoadStats {
    /// 记录分配
    pub fn record_alloc(&mut self, zeroed: bool) {
        self.pages_allocated += 1;
        self.bytes_allocated += PAGE_SIZE as u64;
        if zeroed {
            self.pages_zeroed += 1;
        }
    }
    
    /// 记录文件加载
    pub fn record_file_load(&mut self) {
        self.pages_loaded += 1;
    }
    
    /// 记录失败
    pub fn record_failure(&mut self) {
        self.load_failures += 1;
    }
}
```

### 4.5 栈扩展

栈扩展处理栈区域的自动增长，当访问超出当前栈边界时自动扩展。

**栈区域特征**

```rust
/// 栈区域标志
bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct StackFlags: u32 {
        /// 向下增长
        const GROWSDOWN = 0x01;
        /// 最大大小限制
        const LIMITED = 0x02;
    }
}

/// 栈区域
pub struct StackRegion {
    /// 基础区域
    base: VirRegion,
    /// 栈标志
    stack_flags: StackFlags,
    /// 栈底地址（最低可扩展到的地址）
    stack_low: VirtAddr,
    /// 最大栈大小
    max_size: usize,
}

impl StackRegion {
    /// 创建新的栈区域
    pub fn new(
        vaddr: VirtAddr,
        initial_size: usize,
        max_size: usize,
    ) -> Result<Self, RegionError> {
        let base = VirRegion::new(
            vaddr - initial_size,
            initial_size,
            RegionFlags::WRITABLE | RegionFlags::ANON,
        )?;
        
        Ok(Self {
            base,
            stack_flags: StackFlags::GROWSDOWN | StackFlags::LIMITED,
            stack_low: vaddr - max_size,
            max_size,
        })
    }
    
    /// 检查是否可以扩展
    pub fn can_grow(&self, addr: VirtAddr) -> bool {
        // 地址必须在可扩展范围内
        addr >= self.stack_low && addr < self.base.vaddr()
    }
    
    /// 扩展栈
    pub fn grow(&mut self, addr: VirtAddr) -> Result<(), PageFaultError> {
        if !self.can_grow(addr) {
            return Err(PageFaultError::InvalidAddress(addr));
        }
        
        // 计算新的起始地址（页对齐）
        let new_vaddr = addr.align_down(PAGE_SIZE);
        let new_size = self.base.vaddr() + self.base.length() - new_vaddr;
        
        // 检查最大大小限制
        if new_size > self.max_size {
            log::warn!(
                "Stack growth exceeded limit: {} > {}",
                new_size,
                self.max_size
            );
            return Err(PageFaultError::InvalidAddress(addr));
        }
        
        // 扩展区域
        let growth = self.base.vaddr() - new_vaddr;
        self.base.set_vaddr(new_vaddr);
        self.base.set_length(new_size);
        
        log::debug!(
            "Stack grown by {} bytes to {:#x}+{:#x}",
            growth,
            new_vaddr,
            new_size
        );
        
        Ok(())
    }
}
```

**栈扩展处理器**

```rust
/// 栈扩展处理器
pub struct StackGrowHandler {
    /// 最大栈大小
    max_stack_size: usize,
}

impl StackGrowHandler {
    /// 创建新的栈扩展处理器
    pub fn new(max_stack_size: usize) -> Self {
        Self { max_stack_size }
    }
    
    /// 处理栈扩展
    pub fn handle(
        &self,
        vmp: &mut VmProc,
        addr: VirtAddr,
    ) -> Result<(), PageFaultError> {
        // 获取栈区域
        let stack = vmp.stack_region_mut()
            .ok_or(PageFaultError::InvalidAddress(addr))?;
        
        // 检查地址是否在可扩展范围内
        if !stack.can_grow(addr) {
            return Err(PageFaultError::InvalidAddress(addr));
        }
        
        // 扩展栈
        stack.grow(addr)?;
        
        Ok(())
    }
}
```

**栈扩展流程**

```
┌─────────────────────────────────────────────────────────────────┐
│                    栈扩展流程                                    │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  页错误: 访问栈区域外的地址                                      │
│         │                                                       │
│         ▼                                                       │
│  ┌──────────────────────┐                                       │
│  │ 查找栈区域           │                                       │
│  │ map_lookup()         │                                       │
│  └──────────────────────┘                                       │
│         │                                                       │
│         ├─────► 未找到 ──► 检查是否可以扩展                     │
│         │                    │                                  │
│         │                    ▼                                  │
│         │              ┌──────────────────┐                     │
│         │              │ 地址 >= stack_low?│                    │
│         │              └──────────────────┘                     │
│         │                    │                                  │
│         │                    ├─────► 否 ──► SIGSEGV             │
│         │                    │                                  │
│         │                    ▼ 是                               │
│         │              ┌──────────────────┐                     │
│         │              │ 扩展栈区域       │                     │
│         │              │ grow_stack()     │                     │
│         │              └──────────────────┘                     │
│         │                    │                                  │
│         │                    ▼                                  │
│         │              ┌──────────────────┐                     │
│         │              │ 分配新页面       │                     │
│         │              │ 处理页错误       │                     │
│         │              └──────────────────┘                     │
│         │                                                       │
│         ▼ 找到区域                                              │
│  ┌──────────────────────┐                                       │
│  │ 正常页错误处理       │                                       │
│  └──────────────────────┘                                       │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

**栈保护页**

```rust
/// 栈保护页管理
pub struct StackGuard {
    /// 保护页大小（通常 1 页）
    guard_size: usize,
    /// 保护页地址
    guard_addr: Option<VirtAddr>,
}

impl StackGuard {
    /// 创建新的栈保护
    pub fn new(guard_size: usize) -> Self {
        Self {
            guard_size,
            guard_addr: None,
        }
    }
    
    /// 设置保护页
    pub fn set_guard(&mut self, addr: VirtAddr) {
        self.guard_addr = Some(addr);
    }
    
    /// 检查地址是否在保护页内
    pub fn is_guard(&self, addr: VirtAddr) -> bool {
        if let Some(guard) = self.guard_addr {
            addr >= guard && addr < guard + self.guard_size
        } else {
            false
        }
    }
    
    /// 更新保护页位置
    pub fn update_guard(&mut self, new_stack_bottom: VirtAddr) {
        // 保护页在栈底之下
        self.guard_addr = Some(new_stack_bottom - self.guard_size);
    }
}

impl StackRegion {
    /// 带保护页的栈扩展
    pub fn grow_with_guard(
        &mut self,
        addr: VirtAddr,
        guard: &mut StackGuard,
    ) -> Result<(), PageFaultError> {
        // 检查是否访问保护页
        if guard.is_guard(addr) {
            log::warn!("Access to stack guard page at {:#x}", addr);
            return Err(PageFaultError::InvalidAddress(addr));
        }
        
        // 正常扩展
        self.grow(addr)?;
        
        // 更新保护页位置
        guard.update_guard(self.base.vaddr());
        
        Ok(())
    }
}
```

**栈限制检查**

```rust
impl VmProc {
    /// 获取栈使用情况
    pub fn stack_usage(&self) -> StackUsage {
        if let Some(stack) = self.stack_region() {
            let current_size = stack.length();
            let max_size = stack.max_size();
            
            StackUsage {
                current_size,
                max_size,
                used: current_size,  // 简化，实际应计算已映射部分
                available: max_size - current_size,
            }
        } else {
            StackUsage::default()
        }
    }
}

/// 栈使用情况
#[derive(Debug, Default)]
pub struct StackUsage {
    /// 当前栈大小
    pub current_size: usize,
    /// 最大栈大小
    pub max_size: usize,
    /// 已使用大小
    pub used: usize,
    /// 可用大小
    pub available: usize,
}

/// 栈限制配置
#[derive(Debug, Clone)]
pub struct StackLimits {
    /// 默认初始栈大小
    pub default_size: usize,
    /// 最大栈大小
    pub max_size: usize,
    /// 保护页大小
    pub guard_size: usize,
}

impl Default for StackLimits {
    fn default() -> Self {
        Self {
            default_size: 128 * 1024,      // 128 KB
            max_size: 8 * 1024 * 1024,     // 8 MB
            guard_size: PAGE_SIZE,          // 1 页
        }
    }
}
```

**栈溢出检测**

```rust
/// 栈溢出检测器
pub struct StackOverflowDetector {
    /// 栈限制
    limits: StackLimits,
    /// 溢出计数
    overflow_count: u64,
}

impl StackOverflowDetector {
    /// 检测栈溢出
    pub fn check(&mut self, vmp: &VmProc, addr: VirtAddr) -> StackCheckResult {
        let usage = vmp.stack_usage();
        
        if usage.current_size >= self.limits.max_size {
            self.overflow_count += 1;
            log::error!(
                "Stack overflow: current={}, max={}",
                usage.current_size,
                self.limits.max_size
            );
            return StackCheckResult::Overflow;
        }
        
        if usage.available < self.limits.guard_size {
            log::warn!(
                "Stack near limit: available={}",
                usage.available
            );
            return StackCheckResult::NearLimit;
        }
        
        StackCheckResult::Ok
    }
}

/// 栈检查结果
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackCheckResult {
    /// 正常
    Ok,
    /// 接近限制
    NearLimit,
    /// 溢出
    Overflow,
}
```

---

## 5. 性能优化

### 5.1 快速路径

快速路径优化常见情况的页错误处理，减少处理延迟。

**快速路径识别**

```rust
/// 快速路径条件
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FastPath {
    /// CoW 触发（refcount > 1，写操作）
    Cow,
    /// 首次访问（phys == MAP_NONE）
    FirstAccess,
    /// 无需处理（已映射且权限正确）
    NoOp,
    /// 需要慢路径
    SlowPath,
}

impl PageFaultHandler {
    /// 检测是否可以使用快速路径
    pub fn detect_fast_path(
        &self,
        region: &VirRegion,
        phys_region: Option<&PhysRegion>,
        write: bool,
    ) -> FastPath {
        match phys_region {
            None => {
                // 无物理区域，首次访问
                FastPath::FirstAccess
            }
            Some(pr) => {
                let phys_block = pr.phys_block();
                
                // 检查物理地址
                if phys_block.phys().is_none() {
                    return FastPath::FirstAccess;
                }
                
                // 检查 CoW
                if write && phys_block.refcount() > 1 {
                    return FastPath::Cow;
                }
                
                // 已正确映射
                FastPath::NoOp
            }
        }
    }
}
```

**快速路径实现**

```rust
impl PageFaultHandler {
    /// 快速路径处理
    #[inline]
    pub fn handle_fast(
        &mut self,
        vmp: &mut VmProc,
        region: &mut VirRegion,
        offset: usize,
        write: bool,
    ) -> Result<HandleResult, PageFaultError> {
        // 获取物理区域
        let phys_region = region.get_phys_region(offset);
        
        // 检测快速路径类型
        match self.detect_fast_path(region, phys_region.as_ref(), write) {
            FastPath::FirstAccess => {
                // 快速分配新页
                self.fast_alloc(region, offset)
            }
            
            FastPath::Cow => {
                // 快速 CoW
                self.fast_cow(vmp, region, offset)
            }
            
            FastPath::NoOp => {
                // 无需处理
                Ok(HandleResult::Ok)
            }
            
            FastPath::SlowPath => {
                // 回退到慢路径
                self.handle_slow(vmp, region, offset, write)
            }
        }
    }
    
    /// 快速分配
    #[inline]
    fn fast_alloc(
        &mut self,
        region: &mut VirRegion,
        offset: usize,
    ) -> Result<HandleResult, PageFaultError> {
        // 直接分配，跳过复杂检查
        let phys_region = region.get_or_create_phys_region(offset)?;
        let phys_block = phys_region.phys_block();
        
        // 分配并清零
        let page = self.allocator.alloc(1, AllocFlags::CLEAR)
            .map_err(|_| PageFaultError::OutOfMemory {
                process: region.owner(),
            })?;
        
        phys_block.set_phys(page);
        
        Ok(HandleResult::Ok)
    }
    
    /// 快速 CoW
    #[inline]
    fn fast_cow(
        &mut self,
        vmp: &mut VmProc,
        region: &mut VirRegion,
        offset: usize,
    ) -> Result<HandleResult, PageFaultError> {
        let phys_region = region.get_phys_region(offset)
            .ok_or(PageFaultError::Internal(EFAULT))?;
        
        let old_phys = phys_region.phys_block().phys()
            .ok_or(PageFaultError::Internal(EFAULT))?;
        
        // 分配新页
        let new_page = self.allocator.alloc(1, AllocFlags::empty())
            .map_err(|_| PageFaultError::OutOfMemory {
                process: region.owner(),
            })?;
        
        // 复制内容
        new_page.copy_from(&old_phys)?;
        
        // 更新引用计数和物理地址
        phys_region.phys_block().dec_refcount();
        phys_region.phys_block().set_phys(new_page);
        
        Ok(HandleResult::Ok)
    }
}
```

**快速路径统计**

```rust
/// 快速路径统计
#[derive(Debug, Default)]
pub struct FastPathStats {
    /// 快速路径命中次数
    pub fast_hits: u64,
    /// 慢路径次数
    pub slow_hits: u64,
    /// 各类型命中次数
    pub first_access: u64,
    pub cow_count: u64,
    pub noop_count: u64,
}

impl FastPathStats {
    /// 记录快速路径
    pub fn record_fast(&mut self, path: FastPath) {
        self.fast_hits += 1;
        match path {
            FastPath::FirstAccess => self.first_access += 1,
            FastPath::Cow => self.cow_count += 1,
            FastPath::NoOp => self.noop_count += 1,
            FastPath::SlowPath => self.slow_hits += 1,
        }
    }
    
    /// 获取快速路径命中率
    pub fn hit_rate(&self) -> f64 {
        let total = self.fast_hits + self.slow_hits;
        if total == 0 {
            0.0
        } else {
            self.fast_hits as f64 / total as f64
        }
    }
}
```

**内联优化**

```rust
impl VirRegion {
    /// 内联的物理区域查找
    #[inline]
    pub fn get_phys_region_inline(&self, offset: usize) -> Option<&PhysRegion> {
        let page_offset = offset & !(PAGE_SIZE - 1);
        
        // 内联遍历
        let mut current = self.phys_regions.head();
        while let Some(pr) = current {
            if pr.offset() == page_offset {
                return Some(pr);
            }
            if pr.offset() > page_offset {
                return None;
            }
            current = pr.next();
        }
        None
    }
}

impl PhysBlock {
    /// 内联的引用计数检查
    #[inline]
    pub fn needs_cow_inline(&self, write: bool) -> bool {
        write && self.refcount.load(Ordering::Relaxed) > 1
    }
    
    /// 内联的物理地址检查
    #[inline]
    pub fn has_phys_inline(&self) -> bool {
        self.phys.is_some()
    }
}
```

**批处理优化**

```rust
impl PageFaultHandler {
    /// 批量处理页错误
    pub fn handle_batch(
        &mut self,
        vmp: &mut VmProc,
        faults: &[PageFaultInfo],
    ) -> Vec<Result<PageFaultResult, PageFaultError>> {
        let mut results = Vec::with_capacity(faults.len());
        
        // 按区域分组
        let mut by_region: BTreeMap<VirtAddr, Vec<usize>> = BTreeMap::new();
        for (i, info) in faults.iter().enumerate() {
            if let Some(region) = vmp.lookup_region(info.vaddr) {
                by_region.entry(region.vaddr())
                    .or_default()
                    .push(i);
            }
        }
        
        // 批量处理每个区域
        for (region_vaddr, indices) in by_region {
            if let Some(region) = vmp.lookup_region_mut(region_vaddr) {
                for &i in &indices {
                    let info = &faults[i];
                    let offset = info.vaddr.align_down(PAGE_SIZE) - region_vaddr;
                    let result = self.handle_fast(vmp, region, offset, info.is_write());
                    results.push(result.map(|r| r.into()));
                }
            }
        }
        
        results
    }
}
```

### 5.2 锁粒度

细粒度锁减少锁竞争，提高并发性能。

**锁层次结构**

```rust
/// VM 锁层次
/// 
/// 锁获取顺序（避免死锁）:
/// 1. VmProcTable 锁（全局进程表）
/// 2. VmProc 锁（进程级）
/// 3. VirRegion 锁（区域级）
/// 4. PhysBlock 锁（物理块级）
pub struct VmLocks {
    /// 全局进程表锁
    proc_table: RwLock<()>,
    /// 进程级锁
    proc_locks: Vec<Mutex<()>>,
    /// 区域级锁池
    region_lock_pool: LockPool,
}

/// 锁池
pub struct LockPool {
    locks: Vec<Mutex<()>>,
    mask: usize,
}

impl LockPool {
    pub fn new(count: usize) -> Self {
        let count = count.next_power_of_two();
        Self {
            locks: (0..count).map(|_| Mutex::new(())).collect(),
            mask: count - 1,
        }
    }
    
    /// 根据地址获取锁
    pub fn get_lock(&self, addr: VirtAddr) -> &Mutex<()> {
        &self.locks[addr.as_usize() & self.mask]
    }
}
```

**细粒度锁定**

```rust
impl PageFaultHandler {
    /// 使用细粒度锁处理页错误
    pub fn handle_with_locking(
        &mut self,
        info: PageFaultInfo,
    ) -> Result<PageFaultResult, PageFaultError> {
        // 1. 获取进程引用（不需要锁）
        let vmp = self.proc_table.get_by_endpoint(info.endpoint)
            .ok_or(PageFaultError::ProcessNotFound(info.endpoint))?;
        
        // 2. 查找区域（使用读锁）
        let region = {
            let _read = vmp.regions_read_lock();
            vmp.lookup_region(info.vaddr)
                .ok_or(PageFaultError::InvalidAddress(info.vaddr))?
        };
        
        // 3. 检查权限（无需锁）
        if info.is_write() && !region.is_writable() {
            return Err(PageFaultError::PermissionDenied {
                addr: info.vaddr,
                access: info.access_type,
            });
        }
        
        // 4. 获取区域锁并处理
        let offset = info.vaddr.align_down(PAGE_SIZE) - region.vaddr();
        let _region_lock = region.lock();
        
        self.handle_page(&vmp, &region, offset, info.is_write())?;
        
        Ok(PageFaultResult::Ok)
    }
}
```

**读写锁优化**

```rust
impl VmProc {
    /// 区域读锁
    pub fn regions_read_lock(&self) -> RwLockReadGuard<'_, ()> {
        self.regions_lock.read().unwrap()
    }
    
    /// 区域写锁
    pub fn regions_write_lock(&self) -> RwLockWriteGuard<'_, ()> {
        self.regions_lock.write().unwrap()
    }
    
    /// 尝试获取读锁
    pub fn try_regions_read_lock(&self) -> Option<RwLockReadGuard<'_, ()>> {
        self.regions_lock.try_read().ok()
    }
}

impl VirRegion {
    /// 使用乐观锁处理页错误
    pub fn handle_optimistic(
        &self,
        vmp: &VmProc,
        offset: usize,
        write: bool,
    ) -> Result<HandleResult, PageFaultError> {
        // 乐观读取：假设不需要修改
        loop {
            // 获取版本号
            let version = self.version.load(Ordering::Acquire);
            
            // 执行操作
            let result = self.handle_inner(vmp, offset, write);
            
            // 检查版本是否变化
            if self.version.load(Ordering::Acquire) == version {
                return result;
            }
            
            // 版本变化，重试
            if result.is_err() {
                return result;
            }
        }
    }
}
```

**锁避免策略**

```rust
impl PageFaultHandler {
    /// 无锁快速路径
    pub fn handle_lockfree(
        &mut self,
        info: PageFaultInfo,
    ) -> Result<PageFaultResult, PageFaultError> {
        // 使用原子操作避免锁
        
        // 1. 原子查找进程
        let vmp = self.proc_table.get_by_endpoint_atomic(info.endpoint)
            .ok_or(PageFaultError::ProcessNotFound(info.endpoint))?;
        
        // 2. 无锁区域查找（RCU 风格）
        let region = vmp.lookup_region_rcu(info.vaddr)
            .ok_or(PageFaultError::InvalidAddress(info.vaddr))?;
        
        // 3. 权限检查（只读）
        if info.is_write() && !region.is_writable() {
            return Err(PageFaultError::PermissionDenied {
                addr: info.vaddr,
                access: info.access_type,
            });
        }
        
        // 4. 原子页处理
        let offset = info.vaddr.align_down(PAGE_SIZE) - region.vaddr();
        self.handle_page_atomic(&region, offset, info.is_write())?;
        
        Ok(PageFaultResult::Ok)
    }
    
    /// 原子页处理
    fn handle_page_atomic(
        &mut self,
        region: &VirRegion,
        offset: usize,
        write: bool,
    ) -> Result<(), PageFaultError> {
        // 使用 CAS 操作更新引用计数
        let phys_region = region.get_phys_region(offset);
        
        match phys_region {
            None => {
                // 需要分配，获取锁
                let _lock = region.lock();
                self.allocate_page(region, offset)
            }
            Some(pr) => {
                let phys_block = pr.phys_block();
                
                if phys_block.phys().is_none() {
                    let _lock = region.lock();
                    self.allocate_page(region, offset)
                } else if write && phys_block.refcount() > 1 {
                    // CoW：使用原子 CAS
                    self.do_cow_atomic(pr)
                } else {
                    Ok(())
                }
            }
        }
    }
}
```

**锁统计**

```rust
/// 锁统计
#[derive(Debug, Default)]
pub struct LockStats {
    /// 读锁获取次数
    pub read_locks: AtomicU64,
    /// 写锁获取次数
    pub write_locks: AtomicU64,
    /// 锁竞争次数
    pub contentions: AtomicU64,
    /// 总等待时间（纳秒）
    pub wait_time_ns: AtomicU64,
}

impl LockStats {
    /// 记录锁获取
    pub fn record_lock(&self, is_write: bool, wait_ns: u64, contended: bool) {
        if is_write {
            self.write_locks.fetch_add(1, Ordering::Relaxed);
        } else {
            self.read_locks.fetch_add(1, Ordering::Relaxed);
        }
        
        if contended {
            self.contentions.fetch_add(1, Ordering::Relaxed);
        }
        
        self.wait_time_ns.fetch_add(wait_ns, Ordering::Relaxed);
    }
    
    /// 计算竞争率
    pub fn contention_rate(&self) -> f64 {
        let total = self.read_locks.load(Ordering::Relaxed) 
            + self.write_locks.load(Ordering::Relaxed);
        if total == 0 {
            0.0
        } else {
            self.contentions.load(Ordering::Relaxed) as f64 / total as f64
        }
    }
}
```

**死锁避免**

```rust
/// 锁顺序验证
#[cfg(debug_assertions)]
pub struct LockOrderChecker {
    /// 当前持有的锁
    held_locks: RefCell<Vec<LockId>>,
}

#[cfg(debug_assertions)]
impl LockOrderChecker {
    /// 检查锁顺序
    pub fn check_acquire(&self, lock_id: LockId) {
        let held = self.held_locks.borrow();
        for &held_id in held.iter() {
            if held_id >= lock_id {
                panic!(
                    "Lock order violation: trying to acquire {:?} while holding {:?}",
                    lock_id, held_id
                );
            }
        }
        drop(held);
        self.held_locks.borrow_mut().push(lock_id);
    }
    
    /// 记录锁释放
    pub fn record_release(&self, lock_id: LockId) {
        let mut held = self.held_locks.borrow_mut();
        if let Some(pos) = held.iter().position(|&id| id == lock_id) {
            held.remove(pos);
        }
    }
}

/// 锁 ID
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LockId {
    /// 进程表锁
    ProcTable = 1,
    /// 进程锁
    Proc(usize) = 2,
    /// 区域锁
    Region(usize) = 3,
    /// 物理块锁
    PhysBlock(usize) = 4,
}
```

---

## 6. 测试与验证

### 6.1 CoW 触发测试

测试写保护错误正确触发 CoW 机制。

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    /// 测试 CoW 触发
    #[test]
    fn test_cow_triggered() {
        let mut handler = PageFaultHandler::new_mock();
        let (vmp, region) = setup_shared_region();
        
        // 设置共享页面（refcount = 2）
        let phys_region = region.get_or_create_phys_region(0).unwrap();
        phys_region.phys_block().set_phys(PhysAddr::new(0x1000));
        phys_region.phys_block().set_refcount(2);
        
        // 触发写操作页错误
        let info = PageFaultInfo {
            endpoint: vmp.endpoint(),
            vaddr: region.vaddr(),
            fault_type: PageFaultType::Protection,
            access_type: AccessType::Write,
            error_code: 0x6,
        };
        
        let result = handler.handle(info).unwrap();
        assert_eq!(result, PageFaultResult::Ok);
        
        // 验证 CoW 发生
        assert_eq!(phys_region.phys_block().refcount(), 1);
        assert_ne!(phys_region.phys_block().phys(), Some(PhysAddr::new(0x1000)));
    }
    
    /// 测试 CoW 不触发（refcount == 1）
    #[test]
    fn test_cow_not_triggered_single_ref() {
        let mut handler = PageFaultHandler::new_mock();
        let (vmp, region) = setup_private_region();
        
        let phys_region = region.get_or_create_phys_region(0).unwrap();
        phys_region.phys_block().set_phys(PhysAddr::new(0x1000));
        phys_region.phys_block().set_refcount(1);
        
        let info = PageFaultInfo {
            endpoint: vmp.endpoint(),
            vaddr: region.vaddr(),
            fault_type: PageFaultType::Protection,
            access_type: AccessType::Write,
            error_code: 0x6,
        };
        
        let result = handler.handle(info).unwrap();
        assert_eq!(result, PageFaultResult::Ok);
        
        // 物理地址不变
        assert_eq!(phys_region.phys_block().phys(), Some(PhysAddr::new(0x1000)));
    }
    
    /// 测试 CoW 不触发（读操作）
    #[test]
    fn test_cow_not_triggered_read() {
        let mut handler = PageFaultHandler::new_mock();
        let (vmp, region) = setup_shared_region();
        
        let phys_region = region.get_or_create_phys_region(0).unwrap();
        phys_region.phys_block().set_phys(PhysAddr::new(0x1000));
        phys_region.phys_block().set_refcount(2);
        
        let info = PageFaultInfo {
            endpoint: vmp.endpoint(),
            vaddr: region.vaddr(),
            fault_type: PageFaultType::Protection,
            access_type: AccessType::Read,
            error_code: 0x4,
        };
        
        let result = handler.handle(info).unwrap();
        assert_eq!(result, PageFaultResult::Ok);
        
        // 引用计数不变
        assert_eq!(phys_region.phys_block().refcount(), 2);
    }
    
    /// 测试 CoW 内容复制
    #[test]
    fn test_cow_content_copy() {
        let mut handler = PageFaultHandler::new_mock();
        let (vmp, region) = setup_shared_region();
        
        // 设置原始内容
        let old_phys = PhysAddr::new(0x1000);
        let old_page = unsafe { &mut *(old_phys.as_usize() as *mut [u8; 4096]) };
        old_page[0] = 0x42;
        old_page[100] = 0x99;
        
        let phys_region = region.get_or_create_phys_region(0).unwrap();
        phys_region.phys_block().set_phys(old_phys);
        phys_region.phys_block().set_refcount(2);
        
        // 触发 CoW
        let info = PageFaultInfo {
            endpoint: vmp.endpoint(),
            vaddr: region.vaddr(),
            fault_type: PageFaultType::Protection,
            access_type: AccessType::Write,
            error_code: 0x6,
        };
        
        handler.handle(info).unwrap();
        
        // 验证新页面内容相同
        let new_phys = phys_region.phys_block().phys().unwrap();
        let new_page = unsafe { &*(new_phys.as_usize() as *const [u8; 4096]) };
        assert_eq!(new_page[0], 0x42);
        assert_eq!(new_page[100], 0x99);
    }
}
```

### 6.2 非法访问测试

测试 SIGSEGV 正确发送给非法访问的进程。

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    /// 测试空指针访问
    #[test]
    fn test_null_pointer_access() {
        let mut handler = PageFaultHandler::new_mock();
        let vmp = create_test_process();
        
        let info = PageFaultInfo {
            endpoint: vmp.endpoint(),
            vaddr: VirtAddr::new(0x0),
            fault_type: PageFaultType::NotPresent,
            access_type: AccessType::Read,
            error_code: 0x4,
        };
        
        let result = handler.handle(info);
        assert!(matches!(result, Err(PageFaultError::InvalidAddress(_))));
        
        // 验证 SIGSEGV 被发送
        assert!(handler.signal_sent(vmp.endpoint(), Signal::SIGSEGV));
    }
    
    /// 测试写只读区域
    #[test]
    fn test_write_readonly_region() {
        let mut handler = PageFaultHandler::new_mock();
        let (vmp, region) = setup_readonly_region();
        
        let info = PageFaultInfo {
            endpoint: vmp.endpoint(),
            vaddr: region.vaddr(),
            fault_type: PageFaultType::Protection,
            access_type: AccessType::Write,
            error_code: 0x6,
        };
        
        let result = handler.handle(info);
        assert!(matches!(result, Err(PageFaultError::PermissionDenied { .. })));
        
        assert!(handler.signal_sent(vmp.endpoint(), Signal::SIGSEGV));
    }
    
    /// 测试越界访问
    #[test]
    fn test_out_of_bounds_access() {
        let mut handler = PageFaultHandler::new_mock();
        let vmp = create_test_process();
        
        // 访问超出进程地址空间的地址
        let info = PageFaultInfo {
            endpoint: vmp.endpoint(),
            vaddr: VirtAddr::new(0xFFFFFFFFF000),
            fault_type: PageFaultType::NotPresent,
            access_type: AccessType::Read,
            error_code: 0x4,
        };
        
        let result = handler.handle(info);
        assert!(matches!(result, Err(PageFaultError::InvalidAddress(_))));
    }
    
    /// 测试栈溢出检测
    #[test]
    fn test_stack_overflow_detection() {
        let mut handler = PageFaultHandler::new_mock();
        let (vmp, stack) = setup_stack_region(4096, 8192);  // 初始 4KB，最大 8KB
        
        // 尝试扩展超过最大限制
        let overflow_addr = stack.vaddr() - 16384;  // 超出最大限制
        
        let info = PageFaultInfo {
            endpoint: vmp.endpoint(),
            vaddr: overflow_addr,
            fault_type: PageFaultType::NotPresent,
            access_type: AccessType::Write,
            error_code: 0x6,
        };
        
        let result = handler.handle(info);
        assert!(matches!(result, Err(PageFaultError::InvalidAddress(_))));
    }
}
```

### 6.3 栈扩展测试

测试自动增长栈功能。

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    /// 测试栈正常扩展
    #[test]
    fn test_stack_normal_growth() {
        let mut handler = PageFaultHandler::new_mock();
        let (vmp, stack) = setup_stack_region(4096, 65536);  // 初始 4KB，最大 64KB
        
        // 访问栈底以下的地址
        let growth_addr = stack.vaddr() - 4096;
        
        let info = PageFaultInfo {
            endpoint: vmp.endpoint(),
            vaddr: growth_addr,
            fault_type: PageFaultType::NotPresent,
            access_type: AccessType::Write,
            error_code: 0x6,
        };
        
        let result = handler.handle(info).unwrap();
        assert_eq!(result, PageFaultResult::Ok);
        
        // 验证栈已扩展
        assert!(stack.contains(growth_addr));
        assert_eq!(stack.length(), 8192);
    }
    
    /// 测试栈多次扩展
    #[test]
    fn test_stack_multiple_growth() {
        let mut handler = PageFaultHandler::new_mock();
        let (vmp, stack) = setup_stack_region(4096, 65536);
        
        // 第一次扩展
        let addr1 = stack.vaddr() - 4096;
        handler.handle(PageFaultInfo {
            endpoint: vmp.endpoint(),
            vaddr: addr1,
            fault_type: PageFaultType::NotPresent,
            access_type: AccessType::Write,
            error_code: 0x6,
        }).unwrap();
        
        assert_eq!(stack.length(), 8192);
        
        // 第二次扩展
        let addr2 = stack.vaddr() - 8192;
        handler.handle(PageFaultInfo {
            endpoint: vmp.endpoint(),
            vaddr: addr2,
            fault_type: PageFaultType::NotPresent,
            access_type: AccessType::Write,
            error_code: 0x6,
        }).unwrap();
        
        assert_eq!(stack.length(), 16384);
    }
    
    /// 测试保护页
    #[test]
    fn test_stack_guard_page() {
        let mut handler = PageFaultHandler::new_mock();
        let (vmp, stack, guard) = setup_stack_with_guard(4096, 65536);
        
        // 访问保护页
        let guard_addr = guard.guard_addr().unwrap();
        
        let info = PageFaultInfo {
            endpoint: vmp.endpoint(),
            vaddr: guard_addr,
            fault_type: PageFaultType::NotPresent,
            access_type: AccessType::Write,
            error_code: 0x6,
        };
        
        let result = handler.handle(info);
        assert!(matches!(result, Err(PageFaultError::InvalidAddress(_))));
    }
    
    /// 测试栈扩展后页面分配
    #[test]
    fn test_stack_growth_page_allocation() {
        let mut handler = PageFaultHandler::new_mock();
        let (vmp, stack) = setup_stack_region(4096, 65536);
        
        let growth_addr = stack.vaddr() - 4096;
        
        let info = PageFaultInfo {
            endpoint: vmp.endpoint(),
            vaddr: growth_addr,
            fault_type: PageFaultType::NotPresent,
            access_type: AccessType::Write,
            error_code: 0x6,
        };
        
        handler.handle(info).unwrap();
        
        // 验证页面已分配并清零
        let phys_region = stack.get_phys_region(0).unwrap();
        assert!(phys_region.phys_block().phys().is_some());
        
        // 验证页面已清零
        let phys = phys_region.phys_block().phys().unwrap();
        let page = unsafe { &*(phys.as_usize() as *const [u8; 4096]) };
        assert!(page.iter().all(|&b| b == 0));
    }
}
```

### 6.4 缺页统计 (vm_minor_page_fault / vm_major_page_fault)

**统计字段**

`vmproc` 结构中有两个字段用于统计缺页中断：

| 字段 | 类型 | 说明 |
|------|------|------|
| `vm_minor_page_fault` | `u64_t` | 次缺页计数（无需磁盘 I/O） |
| `vm_major_page_fault` | `u64_t` | 主缺页计数（需要磁盘 I/O） |

**Minor Page Fault（次缺页）**

**触发条件**:
- 页表项存在但权限不足（如 CoW 写保护触发）
- 页面在内存中，但需要额外的处理

**处理流程**:
```
CPU 写入只读页 → 触发页错误 → VM 处理 CoW → 分配新页 → 恢复执行
```

**特点**:
- 无需磁盘 I/O，处理速度快
- 通常由 CoW 机制触发
- Minor fault 增加表示 CoW 正在工作

**Major Page Fault（主缺页）**

**触发条件**:
- 页面不在物理内存中
- 需要从磁盘加载（如可执行文件代码段、交换区换入）

**处理流程**:
```
CPU 访问未加载页 → 触发页错误 → VM 请求磁盘 I/O → 等待加载 → 恢复执行
```

**特点**:
- 需要磁盘 I/O，处理速度慢
- 影响程序性能
- Major fault 过多表示内存压力大

**统计时机**:

```c
// handle_pagefault() 中
if (need_disk_io) {
    vm_major_page_fault++;
    // 执行磁盘 I/O
} else {
    vm_minor_page_fault++;
    // 直接处理（如 CoW）
}
```

**性能分析价值**:

| 指标组合 | 含义 | 建议 |
|----------|------|------|
| Minor 高, Major 低 | CoW 工作正常，内存充足 | 系统健康 |
| Minor 高, Major 高 | 频繁缺页，可能有内存压力 | 考虑增加物理内存 |
| Minor 低, Major 高 | 大量磁盘 I/O，严重内存不足 | 急需内存优化或扩容 |
| 两者都低 | 工作集完全在内存中 | 理想状态 |

**fork 时的处理**:
- 子进程的 `vm_minor_page_fault` 和 `vm_major_page_fault` 都初始化为 0
- 子进程独立统计自己的缺页情况
- 父进程的统计不受影响

**使用场景**:
- **性能监控**: 通过 `/proc` 或调试接口暴露统计信息
- **系统调优**: 根据 Major fault 频率调整内存分配策略
- **问题诊断**: 高 Major fault 可能预示内存泄漏或配置不当

> **注意**: 这两个统计字段帮助理解进程的内存访问模式，对系统调优和问题诊断很有价值。

---

## 7. 参见

- [15-cow-mechanism.md](15-cow-mechanism.md) - CoW 实现
- [17-vm-fork.md](17-vm-fork.md) - fork 后的首次写入
- [12-vir-region.md](12-vir-region.md) - 区域查找

---

*分类: VM私有*
