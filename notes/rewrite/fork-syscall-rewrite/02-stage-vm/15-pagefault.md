# 15-pagefault: 页错误处理

> **分类**: VM私有  
> **源码**: `minix3/minix/servers/vm/pagefaults.c`  
> **说明**: 处理 CPU 产生的页错误，包括 CoW 触发、按需加载等

---

## 1. 概述

页错误（Page Fault）是 CPU 在访问内存时检测到异常情况而触发的中断。在 Minix3 中，VM 服务器负责处理所有用户进程的页错误，实现按需分页、CoW 等高级内存管理功能。

### 1.1 页错误类型

Minix3 定义了两种基本页错误类型（基于 x86-32 架构）：

| 错误类型 | 宏定义 | 触发条件 | 典型场景 |
|---------|--------|---------|---------|
| **不存在错误 (NP)** | `PFERR_NOPAGE(err)` | 页表项 Present 位为 0 | 首次访问、按需加载 |
| **保护错误 (WP)** | `PFERR_PROT(err)` | 页表项 Present 位为 1 但权限不足 | CoW 写保护触发 |

```c
// minix3/minix/servers/vm/arch/i386/pagetable.h:36
#define PFERR_NOPAGE(e)	(!((e) & I386_VM_PFE_P))  // 页不存在
#define PFERR_PROT(e)	(((e) & I386_VM_PFE_P))    // 保护错误
#define PFERR_WRITE(e)	((e) & I386_VM_PFE_W)      // 写操作触发
#define PFERR_READ(e)	(!((e) & I386_VM_PFE_W))    // 读操作触发
```

> **x86-32 vs x86-64 差异**: 上述宏定义位于 `arch/i386/pagetable.h`，是 x86-32 特有的。x86-64 的页错误码增加了 bit 2（保留位违规，`PFE_RSVD`）和 bit 4（取指违规，`PFE_FETCH`/`PFERR_EXECUTE`），支持 NX 位（No-Execute）检测。minix-rs 在 x86-64 上需要扩展 `PageFaultType` 和 `AccessType` 以支持执行权限错误。

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
// minix3/minix/servers/vm/pagefaults.c:33
struct pf_state {
    endpoint_t ep;      // 触发页错误的进程端点
    vir_bytes vaddr;    // 触发错误的虚拟地址
    u32_t err;          // CPU 错误码
};

// 内存请求状态（用于内核请求的内存操作）
// minix3/minix/servers/vm/pagefaults.c:39
struct hm_state {
    endpoint_t caller;      // 调用者（KERNEL 或进程）
    endpoint_t requestor;   // 请求发起者
    int transid;            // VFS 事务 ID
    struct vmproc *vmp;     // 目标地址空间
    vir_bytes mem, len;     // 内存范围
    int wrflag;             // 写标志
    int valid;              // 有效性检查（VALID = 0xc0ff1）
    int vfs_avail;          // 是否可以调用 VFS
#define VALID	0xc0ff1
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
// minix3/minix/servers/vm/mem_anon.c:64 - anon_pagefault()
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
// minix3/minix/servers/vm/mem_anon.c:105 - anon_writable()
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

Minix3 中栈区域的增长由 `do_brk()` 系统调用处理（见 [17-vm-brk.md](17-vm-brk.md)），而非在页错误处理中自动扩展。页错误处理中不包含栈自动增长逻辑——如果访问的地址不在任何已映射区域内，直接发送 SIGSEGV。

> **注意**: Minix3 没有 `VR_GROWSDOWN` 或 `VR_GROWSUP` 标志。栈和数据段的增长由 `do_brk()` 管理，不是通过页错误触发的自动增长机制。

**按需加载（文件映射）**

```c
// minix3/minix/servers/vm/mem_file.c:84
static int mappedfile_pagefault(struct vmproc *vmp, struct vir_region *region,
    struct phys_region *ph, int write, vfs_callback_t cb,
    void *state, int statelen, int *io)
{
    u32_t allocflags;
    int procfd = region->param.file.fdref->fd;

    allocflags = vrallocflags(region->flags);
    assert(ph->ph->refcount > 0);
    assert(region->param.file.inited);

    // 情况1: 物理块未分配 → 从文件加载或使用缓存
    if(ph->ph->phys == MAP_NONE) {
        struct cached_page *cp;
        u64_t referenced_offset =
            region->param.file.offset + ph->offset;

        // 先查找 VM 页面缓存
        // ... (缓存查找逻辑)

        // 缓存未命中且无回调 → 返回错误
        if(!cb) return EFAULT;

        // 异步请求 VFS 从文件加载
        if(vfs_request(VMVFSREQ_FDIO, procfd, vmp, referenced_offset,
            VM_PAGE_SIZE, cb, NULL, state, statelen) != OK) {
            printf("VM: mappedfile_pagefault: vfs_request failed\n");
            return ENOMEM;
        }
        *io = 1;
        return SUSPEND;
    }

    // 情况2: 读操作，页面已存在
    if(!write) return OK;

    // 情况3: 写操作 → 执行 CoW
    return cow_block(vmp, region, ph, 0);
}
```

> **注意**: `mappedfile_writable()` 始终返回 0，即文件映射内存从不直接可写，写操作总是触发 CoW。

**Minix3 源码分析**

```c
// minix3/minix/servers/vm/region.c:664
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
// minix3/minix/servers/vm/pagefaults.c:135
if (io)
    vmp->vm_major_page_fault++;  // 需要 I/O（从磁盘加载）
else
    vmp->vm_minor_page_fault++;  // 不需要 I/O（CoW、首次访问等）
```

### 2.2 页错误处理入口

#### 2.2.0 do_pagefaults - 消息入口

`do_pagefaults()` 是 VM 消息循环调用的入口函数，从内核消息中提取页错误信息。

```c
// minix3/minix/servers/vm/pagefaults.c:240
void do_pagefaults(message *m)
{
    handle_pagefault(m->m_source, m->VPF_ADDR, m->VPF_FLAGS, 0);
}
```

消息字段 `VPF_ADDR` 和 `VPF_FLAGS` 由内核在捕获页错误异常时填充。

#### 2.2.1 handle_pagefault - 主处理函数

`handle_pagefault()` 是页错误处理的核心函数，负责解析错误信息、查找区域、执行处理。

**函数原型**

```c
// minix3/minix/servers/vm/pagefaults.c:76
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
// minix3/minix/servers/vm/pagefaults.c:254
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

**handle_memory_start 使用场景**:

1. **内核请求内存访问 (VMPTYPE_CHECK)**: `sys_vircopy`/`sys_physcopy` 跨进程内存复制，内核需要访问用户空间内存
2. **VFS 请求内存操作**: 文件读写需要访问用户缓冲区，异步操作需要事务 ID 跟踪
3. **进程间内存共享检查**: 共享内存区域验证和权限检查

**内核内存请求处理**

```c
// minix3/minix/servers/vm/pagefaults.c:294 - do_memory()
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

进程地址空间布局（从低到高）：代码段 → 数据段 → BSS 段 → 堆区域（向上增长）→ ... → 栈区域（向下增长）→ 内核空间（用户态不可访问）。

非法地址示例：
- NULL (0x0)
- 超出进程地址空间范围
- 未映射的间隙区域
- 内核空间地址（用户态）

**map_lookup 实现**

```c
// minix3/minix/servers/vm/region.c:616
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

进程的虚拟区域按起始地址组织成 AVL 树，`region_search` 使用 `AVL_LESS_EQUAL` 搜索策略：返回 `vaddr <= addr` 的最大区域，再验证 `addr` 是否在该区域内。

示例：查找 `addr=0x15000`
1. 比较 `0x15000 < 0x400000` → 向左子树
2. 比较 `0x15000 >= 0x10000` 且 `< 0x20000` → 找到区域 `[0x10000, 0x20000)`

**region_search 实现**

Minix3 的 `region_search` 由通用 AVL 库（CAVL）通过宏生成，不是手写实现。其函数签名通过宏展开为：

```c
// 由 cavl_if.h + regionavl_defs.h 宏展开生成
// 函数名: region_search (AVL_UNIQUE(id) = region_ ## id)
region_t *region_search(region_avl *tree, vir_bytes k, avl_search_type st);
```

CAVL 库定义了搜索类型枚举：

```c
// minix3/minix/servers/vm/cavl_if.h:24
typedef enum {
    AVL_EQUAL = 1,
    AVL_LESS = 2,
    AVL_GREATER = 4,
    AVL_LESS_EQUAL = AVL_EQUAL | AVL_LESS,    // = 3
    AVL_GREATER_EQUAL = AVL_EQUAL | AVL_GREATER  // = 5
} avl_search_type;
```

AVL 树的节点比较基于 `vaddr` 字段：

```c
// minix3/minix/servers/vm/regionavl_defs.h
#define AVL_COMPARE_KEY_NODE(k, h) AVL_COMPARE_KEY_KEY((k), (h)->vaddr)
#define AVL_COMPARE_NODE_NODE(h1, h2) AVL_COMPARE_KEY_KEY((h1)->vaddr, (h2)->vaddr)
```

`map_lookup` 使用 `AVL_LESS_EQUAL` 搜索：先找到 `vaddr <= addr` 的最大区域，再验证 `addr` 是否确实在该区域内。

**查找标志**

| 标志 | 说明 |
|------|------|
| `AVL_EQUAL` | 精确匹配 |
| `AVL_LESS_EQUAL` | 返回 vaddr <= addr 的最大区域 |
| `AVL_GREATER_EQUAL` | 返回 vaddr >= addr 的最小区域 |

**map_lookup 完整实现**

```c
// minix3/minix/servers/vm/region.c:616
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
// minix3/minix/servers/vm/region.c:60
struct phys_region *physblock_get(struct vir_region *region, vir_bytes offset)
{
    int i;
    struct phys_region *foundregion;
    assert(!(offset % VM_PAGE_SIZE));
    assert(offset < region->length);
    i = offset/VM_PAGE_SIZE;
    if((foundregion = region->physblocks[i]))
        assert(foundregion->offset == offset);
    return foundregion;
}
```

> **注意**: Minix3 的 `physblock_get` 使用数组索引（`region->physblocks[i]`）而非链表遍历，时间复杂度为 O(1)。`physblocks` 是一个按页偏移索引的指针数组，每个槽位指向对应的 `phys_region` 或 NULL。

**查找流程**

1. `region_search` AVL 树查找 → `AVL_LESS_EQUAL` 找到 `vaddr <= addr` 的候选区域
   - 未找到 → 返回 NULL
2. 验证 `addr >= r->vaddr && addr < r->vaddr + r->length`
   - 不在区域内 → 返回 NULL
3. 计算偏移 `offset = addr - r->vaddr`
4. `physblock_get(r, offset)` 获取物理区域（可选）
5. 返回 `struct vir_region *`

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
// minix3/minix/servers/vm/pagefaults.c:82
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
// minix3/minix/servers/vm/region.h:69
#define VR_WRITABLE      0x001   // 可写
#define VR_PHYS64K       0x004   // 物理内存必须 64K 对齐
#define VR_LOWER16MB     0x008   // 物理内存限制在低 16MB
#define VR_LOWER1MB      0x010   // 物理内存限制在低 1MB
#define VR_SHARED        0x040   // 共享内存
#define VR_UNINITIALIZED 0x080   // 分配后不清零
#define VR_ANON          0x100   // 匿名内存（需清零和分配）
#define VR_DIRECT        0x200   // 直接映射（不由 VM 管理）
#define VR_PREALLOC_MAP  0x400   // 预分配映射
```

> **注意**: Minix3 没有 `VR_READABLE`、`VR_EXECUTABLE`、`VR_GROWSDOWN`、`VR_GROWSUP` 标志。可读是默认的（不可写即只读），执行权限不由 VM 区域标志管理。栈增长由内核和 VM 的其他机制处理，不通过区域标志。

**权限检查矩阵**

| 区域权限 | 读操作 | 写操作 | 执行操作 |
|---------|--------|--------|---------|
| VR_WRITABLE | ✓ | ✓ | ✗ (SIGSEGV) |
| 无 VR_WRITABLE | ✓ | ✗ (SIGSEGV) | ✗ (SIGSEGV) |

> **注意**: Minix3 没有 `VR_READABLE` 或 `VR_EXECUTABLE` 标志。可读是默认行为，执行权限不在 VM 层面检查。

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

1. `PFERR_WRITE(err)` 判断是否写操作
   - 读操作 → 可读是默认行为，继续处理
   - 写操作 → 继续检查
2. `region->flags & VR_WRITABLE` 检查区域是否可写
   - 否 → SIGSEGV（写只读区域）
   - 是 → 继续检查
3. 检查 CoW 状态：`phys_block.refcount`
   - `refcount == 1` → 直接写入
   - `refcount > 1` → 触发 CoW

**执行权限检查**

Minix3 的 VM 不在页错误处理中检查执行权限。x86-32 的页错误码不包含执行权限位（该位在 x86-64 中才引入，即 bit 4 `PFERR_FETCH`）。执行权限违规由内核的段保护机制处理，不经过 VM 的页错误处理路径。

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
// minix3/minix/servers/vm/pagefaults.c:120
// 计算区域内的偏移
offset = addr - region->vaddr;

// 调用 map_pf 处理
result = map_pf(vmp, region, offset, wr, pf_cont, &state, sizeof(state), &io);
```

**map_pf 处理逻辑**

```c
// minix3/minix/servers/vm/region.c:664
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

1. `physblock_get()` 获取物理区域
   - 未找到 → `pb_new(MAP_NONE)` + `pb_reference()` 创建新物理块
   - 找到 → 继续检查
2. 检查是否需要处理：`!write || !writable(pr)` → 跳过处理
3. `memtype->ev_pagefault()` 调用内存类型处理器
   - SUSPEND → 等待异步操作
   - 错误 → `pb_unreferenced`，返回错误
   - OK → 继续
4. `map_ph_writept()` 更新页表
5. 返回 OK

**不同内存类型的处理**

| 内存类型 | 处理函数 | 行为 |
|---------|---------|------|
| 匿名内存 | `anon_pagefault()` | CoW 或分配新页 |
| 文件映射 | `mappedfile_pagefault()` | 从文件加载 |
| 共享内存 | `shared_pagefault()` | 从源区域获取物理块并链接 |
| 直接物理 | `phys_pagefault()` | 直接计算物理地址 |

**CoW 处理详解**

```c
// minix3/minix/servers/vm/mem_anon.c:64
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
// minix3/minix/servers/vm/pb.c:136
int mem_cow(struct vir_region *region,
    struct phys_region *ph, phys_bytes new_page_cl, phys_bytes new_page)
{
    struct phys_block *pb;

    if(new_page == MAP_NONE) {
        u32_t allocflags;
        allocflags = vrallocflags(region->flags);

        if((new_page_cl = alloc_mem(1, allocflags)) == NO_MEM)
            return ENOMEM;

        new_page = CLICK2ABS(new_page_cl);
    }

    assert(ph->ph->phys != MAP_NONE);

    // 1. 复制原页面内容到新页面（使用内核系统调用）
    if(sys_abscopy(ph->ph->phys, new_page, VM_PAGE_SIZE) != OK) {
        panic("VM: abscopy failed\n");
        return EFAULT;
    }

    // 2. 创建新物理块
    if(!(pb = pb_new(new_page))) {
        free_mem(new_page_cl, 1);
        return ENOMEM;
    }

    // 3. 解除原物理块引用，链接新物理块
    pb_unreferenced(region, ph, 0);
    pb_link(ph, pb, ph->offset, region);

    // 4. CoW 后内存类型变为匿名
    ph->memtype = &mem_type_anon;

    return OK;
}
```

> **关键差异**: Minix3 的 `mem_cow` 不是简单的 `memcpy` + `refcount--`。它使用 `sys_abscopy`（内核系统调用）复制页面，通过 `pb_unreferenced`/`pb_link` 管理物理块引用关系，并将内存类型切换为匿名内存。

> **方案四补充**：`sys_abscopy` 在方案四下被 `vm_phys_to_virt() + copy_nonoverlapping()` 替代。页错误处理是 direct map 统一性的"压力测试"——它同时涉及 CoW 复制、页表写入、物理页分配，是所有机制的交汇点。在方案四中，这三个操作都通过 `vm_phys_to_virt()` 统一完成：
> - CoW 复制：`copy_nonoverlapping(vm_phys_to_virt(old), vm_phys_to_virt(new), PAGE_SIZE)`
> - 页表写入：`*(vm_phys_to_virt(pt_phys) + offset) = pte_value`
> - 物理页分配：`alloc_phys() → vm_phys_to_virt()` 获取 VA
>
> 三个操作共享同一个 `vm_phys_to_virt()` 入口——这是 direct map 统一性的集中体现。

**页表更新**

```c
// minix3/minix/servers/vm/region.c:257
int map_ph_writept(struct vmproc *vmp, struct vir_region *vr,
    struct phys_region *pr)
{
    int flags = PTF_PRESENT | PTF_USER;
    struct phys_block *pb = pr->ph;

    assert(vr);
    assert(pr);
    assert(pb);
    assert(!(vr->vaddr % VM_PAGE_SIZE));
    assert(!(pr->offset % VM_PAGE_SIZE));
    assert(pb->refcount > 0);

    // 通过 pr_writable() 判断是否可写
    if(pr_writable(vr, pr))
        flags |= PTF_WRITE;
    else
        flags |= PTF_READ;

    // 内存类型附加标志
    if(vr->def_memtype->pt_flags)
        flags |= vr->def_memtype->pt_flags(vr);

    // 更新页表映射
    if(pt_writemap(vmp, &vmp->vm_pt, vr->vaddr + pr->offset,
            pb->phys, VM_PAGE_SIZE, flags,
#if SANITYCHECKS
            !pr->written ? 0 :
#endif
            WMF_OVERWRITE) != OK) {
        printf("VM: map_writept: pt_writemap failed\n");
        return ENOMEM;
    }

    return OK;
}
```

其中 `pr_writable()` 是辅助函数：

```c
// minix3/minix/servers/vm/region.c:130
static int pr_writable(struct vir_region *vr, struct phys_region *pr)
{
    assert(pr->memtype->writable);
    return ((vr->flags & VR_WRITABLE) && pr->memtype->writable(pr));
}
```

#### 方案四视角：页表写入的终极简化

> **方案四标注**：Direct Map 方案下，`pt_writemap()` 内部的页表项写入通过 `vm_phys_to_virt()` 直接操作，无需 `createpde` 临时映射窗口。

页错误处理是 direct map 统一性的"压力测试"——它同时涉及 CoW 复制、页表写入、物理页分配，是所有机制的交汇点。其中**页表写入**是最关键的操作：

**Minix3 的页表写入**需要 `createpde` 临时映射窗口——VM 无法直接访问页表页（它们是物理页），必须请求内核在 VM 的地址空间中临时映射一个物理页，写入页表项后再释放：

```
1. createpde(pt_phys) → 在 VM 地址空间临时映射页表页
2. 通过临时映射写入页表项
3. 释放临时映射
```

**方案四的页表写入**只需一步：

```
1. vm_phys_to_virt(pt_phys) → 直接获取页表页 VA → 写入页表项
```

**三种方案的复杂度递减**：

| 方案 | 页表写入方式 | 复杂度来源 |
|------|------------|-----------|
| Minix3（createpde） | 建临时映射 → 写入 → 释放临时映射 | VM 无法直接访问物理页 |
| 方案三（PtRegion） | 从 PtRegion 分配 VA → 写入 | 页表页需要特殊 VA 管理 |
| 方案四（Direct Map） | `vm_phys_to_virt()` → 直接写入 | 物理页天然有 VA |

读者应感受到：**页错误处理中"写入页表项"是 direct map 统一性的关键验证**——如果这个最复杂的操作都能被 `vm_phys_to_virt()` 一步解决，那么 direct map 的统一性就是经得起考验的。这与 16-vm-fork.md §2.8.5 的 fork 页表创建简化是同一个模式——`createpde` 的消失不是"去掉了临时映射步骤"，而是"VM 不再需要内核作为物理页访问的中介"。

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
// minix3/minix/servers/vm/pagefaults.c:92-151

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

1. VM 检测到非法访问 → 打印错误信息（可选 `sys_diagctl_stacktrace(ep)` 打印调用栈）
2. `sys_kill(vmp->vm_endpoint, SIGSEGV)` → 发送段错误信号
3. `sys_vmctl(ep, VMCTL_CLEAR_PAGEFAULT, 0)` → 清除页错误状态
4. 内核接收 SIGSEGV → 检查进程信号处理器
   - 有处理器 → 执行用户定义处理器
   - 无处理器 → 终止进程，生成 core dump

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
// minix3/minix/servers/vm/mem_anon.c:64
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

// minix3/minix/servers/vm/region.c:664
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

1. 页错误处理中分配内存失败 → 返回 ENOMEM
2. `handle_pagefault` 检查 `result != OK` → 打印 "pagefault not handled"
3. `sys_kill(ep, SIGSEGV)` → 发送段错误信号
4. 进程被终止

**Minix3 的 OOM 策略**

Minix3 采用简单策略：内存不足时终止触发页错误的进程。

```c
// minix3/minix/servers/vm/pagefaults.c:144
if(result != OK) {
    printf("VM: pagefault: SIGSEGV %d pagefault not handled\n", ep);
    sys_kill(ep, SIGSEGV);
    sys_vmctl(ep, VMCTL_CLEAR_PAGEFAULT, 0);
    return;
}
```

**可能的改进策略**

1. **页面回收 (Page Reclamation)**: 释放最近最少使用的页面，丢弃干净的文件映射页
2. **交换 (Swapping)**: 将页面换出到磁盘释放物理内存，需要时再换入
3. **OOM Killer (Linux 风格)**: 选择一个进程终止，优先选择内存占用大的进程，保护关键系统进程
4. **内存压缩 (Memory Compaction)**: 整理内存碎片，合并空闲页面

**内存压力检测**

Minix3 没有独立的内存压力检测函数。内存不足时 `alloc_mem()` 返回 `NO_MEM`，由上层处理。

**预留内存**

Minix3 通过 `SPAREPAGES` 机制为关键操作预留内存，而非通过单独的函数。参见 [05-vm-allocpage.md](05-vm-allocpage.md)。

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

Minix3 在页错误处理失败时直接 `printf` 输出错误信息，没有独立的 OOM 日志函数。

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

> **注意**: 以下设计描述基于实际代码 `os/servers/vm/src/cow_exec_pf.rs`。页错误处理分为两层：`MemType::ev_pagefault`（返回 `PagefaultResult`）和 `handle_pagefault`（转换为 `PagefaultAction` 并执行动作）。

**PagefaultResult** (memtype 层，定义在 `os/servers/vm/src/memtype.rs`)

```rust
/// memtype 的 ev_pagefault 返回结果
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PagefaultResult {
    Handled,         // 页面已就绪
    NeedNewPage,     // 需要分配新物理页
    NeedCow,         // 需要执行 CoW
    AccessViolation, // 访问违规
}
```

**PagefaultAction** (调用者层，定义在 `os/servers/vm/src/cow_exec_pf.rs`)

```rust
/// handle_pagefault 的返回值
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PagefaultAction {
    Handled,
    MappedNewPage,
    CowResolved,
    AccessViolation,
}
```

> **注意**: 当前实现暂无 `Suspend` 变体——异步 I/O 回调机制（对应 Minix3 的 `pf_cont` / `handle_memory_continue`）尚未实现。这是 P1 级别的实现缺口。

**安全的页错误处理入口**（实际设计）

```rust
/// 页错误处理入口（对应 Minix3 handle_pagefault）
/// 实际代码: os/servers/vm/src/cow_exec_pf.rs
pub(crate) fn handle_pagefault(
    proc: &ActiveProc<'_>,       // 当前进程
    region: &mut VirRegion,      // 虚拟区域
    frames: &mut PageFrames,     // 全局物理页帧表
    alloc: &mut dyn PfnAllocator, // PFN 分配器
    fault_addr: VirBytes,        // 故障地址
    write: bool,                 // 是否写操作
) -> Result<PagefaultAction, CowError> {
    let offset = VirBytes(fault_addr.0 - region.vaddr.0);
    let memtype = region.def_memtype.ok_or(CowError::NoMemType)?;

    // 1. 调用 MemType 的 ev_pagefault
    let result = memtype.ev_pagefault(proc, region, frames, offset, write)?;

    // 2. 根据 PagefaultResult 执行动作
    match result {
        PagefaultResult::Handled => Ok(PagefaultAction::Handled),
        PagefaultResult::NeedNewPage => {
            alloc_and_map(region, frames, alloc, offset, memtype)?;
            Ok(PagefaultAction::MappedNewPage)
        }
        PagefaultResult::NeedCow => {
            cow_resolve(region, frames, alloc, offset)?;
            Ok(PagefaultAction::CowResolved)
        }
        PagefaultResult::AccessViolation => {
            Ok(PagefaultAction::AccessViolation)
        }
    }
}

/// 分配新页并映射到区域
pub(crate) fn alloc_and_map(
    region: &mut VirRegion,
    frames: &mut PageFrames,
    alloc: &mut dyn PfnAllocator,
    offset: VirBytes,
    memtype: &'static dyn MemType,
) -> Result<u32, CowError> {
    let pfn = alloc.alloc_pfn().map_err(|_| CowError::NoMemory)?;
    region.map_page(frames, offset, pfn, memtype);
    Ok(pfn)
}

/// CoW 解析入口
pub(crate) fn cow_resolve(
    region: &mut VirRegion,
    frames: &mut PageFrames,
    alloc: &mut dyn PfnAllocator,
    offset: VirBytes,
) -> Result<u32, CowError> {
    cow_resolve_core(region, frames, alloc, offset).map_err(Into::into)
}
```

**关键安全改进**

| C 代码问题 | Rust 安全封装 |
|-----------|-------------|
| `vmproc[p]` 可能越界 | `ActiveProc` 通过生命周期保证有效性 |
| `region` 可能为 NULL | `VirRegion` 是引用类型，编译器保证非空 |
| `ph->ph->phys` 可能是 `MAP_NONE` | `PageSlot.is_mapped()` + `PageFrames.get(pfn)` 返回 Option |
| `memcpy((void*)phys, ...)` 物理地址直接操作 | `vm_phys_to_virt()` + `copy_nonoverlapping()` 有类型保证 |
| `refcount` 溢出无检测 | `u16` + debug 构建下 saturating_add 断言 |
| 物理页分配失败直接 panic | `PfnAllocator::alloc_pfn()` 返回 Result，显式错误处理 |

**错误类型**

```rust
/// 页错误处理错误
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CowError {
    NoMemory,        // 物理页分配失败 (PfnAllocError)
    NoMemType,       // 区域未设置 memtype
    PageNotMapped,   // 页面未映射
    MemType(MemTypeError), // memtype 层错误
}
```

### 3.2 与 memtype 的集成

页错误处理通过 MemoryType trait 与不同内存类型实现解耦，实现多态处理。

**MemType Trait 扩展**

> **注意**: 以下 trait 签名来自实际代码 `os/servers/vm/src/memtype.rs`，§3.1 和 §4.1 中的伪代码现已替换为实际 API 的具体描述。

```rust
/// 内存类型 trait - 页错误处理相关方法（详见 12-memtype.md）
/// 实际代码: os/servers/vm/src/memtype.rs
pub trait MemType: Send + Sync {
    /// 内存类型名称
    fn name(&self) -> &'static str;

    /// 处理页错误
    fn ev_pagefault(
        &self,
        _proc: &ActiveProc<'_>,     // 当前进程
        _region: &mut VirRegion,    // 虚拟区域
        _frames: &mut PageFrames,   // 全局物理页帧表
        _offset: VirBytes,          // 区域内偏移
        _write: bool,               // 是否写操作
    ) -> Result<PagefaultResult, MemTypeError>;

    /// 检查物理页是否可写（对应 Minix3 的 mem_type.writable(pr)）
    fn writable(
        &self,
        _frames: &PageFrames,
        _slot: PageSlot,            // PageSlot 是 Copy 类型，非引用
        _region: &VirRegion,        // remaps>0 时返回 true
    ) -> bool;
}
```

**PagefaultResult** (实际代码: `os/servers/vm/src/memtype.rs`)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PagefaultResult {
    Handled,        // 页面已就绪，无需额外处理
    NeedNewPage,    // 需要分配新物理页（首次访问、按需加载）
    NeedCow,        // 需要执行 CoW（refcount>1 && 写操作）
    AccessViolation, // 访问违规（写只读区域）
}
```

**PagefaultAction** (实际代码: `os/servers/vm/src/cow_exec_pf.rs`)

```rust
/// handle_pagefault 的返回值，由调用者用于执行后续动作（如 pt_writemap）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PagefaultAction {
    Handled,         // 页面已就绪
    MappedNewPage,   // 已分配并映射新页
    CowResolved,     // CoW 已解决
    AccessViolation, // 访问违规
}
```

**关键**: `ev_pagefault` 返回 `PagefaultResult`（memtype 层的语义），`handle_pagefault` 将其转换为 `PagefaultAction`（调用者层的执行结果）。两阶段解耦便于 `handle_pagefault` 统一处理 `alloc_and_map` / `cow_resolve` 等动作。

**匿名内存实现**（实际代码: `os/servers/vm/src/memtype.rs`）

```rust
/// 匿名内存类型
pub struct AnonymousMemory;

impl MemType for AnonymousMemory {
    fn name(&self) -> &'static str {
        "anonymous memory"
    }

    fn writable(&self, frames: &PageFrames, slot: PageSlot, region: &VirRegion) -> bool {
        if !slot.is_mapped() {
            return false;
        }
        if region.remaps > 0 {
            return true;
        }
        frames.get(slot.pfn)
            .map(|s| s.refcount == 1)
            .unwrap_or(false)
    }

    fn ev_pagefault(
        &self,
        _proc: &ActiveProc<'_>,
        region: &mut VirRegion,
        frames: &mut PageFrames,
        offset: VirBytes,
        write: bool,
    ) -> Result<PagefaultResult, MemTypeError> {
        let slot = region.get_slot(offset);

        // 情况1: PageSlot 不存在或物理页未分配
        match slot {
            None => return Ok(PagefaultResult::NeedNewPage),
            Some(s) if !s.is_mapped() => return Ok(PagefaultResult::NeedNewPage),
            _ => {}
        }

        let slot = slot.unwrap();
        let refcount = frames.get(slot.pfn)
            .map(|s| s.refcount)
            .unwrap_or(0);

        // 情况2: 不需要 CoW
        if refcount < 2 || !write {
            return Ok(PagefaultResult::Handled);
        }

        // 情况3: 区域不可写 → 访问违规
        if !region.is_writable() {
            return Ok(PagefaultResult::AccessViolation);
        }

        // 情况4: refcount>=2 && 写操作 → 需要 CoW
        Ok(PagefaultResult::NeedCow)
    }

    fn region_id(&self, region: &VirRegion) -> u32 {
        region.id as u32
    }

    fn ref_count(&self, region: &VirRegion) -> i32 {
        1 + region.remaps
    }
}
```

> **与 Minix3 的对应**: 上述实现对应 `mem_anon.c:64` 的 `anon_pagefault()`。关键差异是引入了 `region.is_writable()` 检查（对应 `VR_WRITABLE` 标志）和 `region.remaps > 0` 的快速可写路径。

---

**文件映射内存实现**（实际代码: `os/servers/vm/src/memtype.rs`）

```rust
pub struct MappedFile;

impl MemType for MappedFile {
    fn name(&self) -> &'static str {
        "mapped file"
    }

    fn writable(&self, _frames: &PageFrames, _slot: PageSlot, _region: &VirRegion) -> bool {
        false
    }

    fn ev_pagefault(
        &self,
        _proc: &ActiveProc<'_>,
        _region: &mut VirRegion,
        _frames: &mut PageFrames,
        offset: VirBytes,
        write: bool,
    ) -> Result<PagefaultResult, MemTypeError> {
        // TODO(P0): Minix3's mappedfile_pagefault implements CoW for file-mapped pages
        // (cow_block path): when refcount > 1 and write=true, it calls cow_block()
        // to create a private anonymous copy. Currently we only return NeedNewPage
        // for unmapped pages, missing the CoW case for shared file-mapped pages.
        //
        // Minix3 的 mappedfile_pagefault 有两种情况:
        //   phys == MAP_NONE → 从 VM cache 查找或异步请求 VFS 加载 → SUSPEND
        //   phys != MAP_NONE && write → cow_block() → 写时复制为匿名页
        //
        // 当前实现仅返回 NeedNewPage（覆盖首次访问），
        // 缺失: VFS 异步加载 和 CoW cow_block 路径。
        Ok(PagefaultResult::NeedNewPage)
    }
}
```

> **与 Minix3 的对应**: `mappedfile_writable()` 始终返回 0（`mem_file.c:150`），即文件映射内存从不直接可写，写操作总是触发 CoW。CoW 后 memtype 切换为 `MEM_TYPE_ANON`，与 Minix3 的 `ph->memtype = &mem_type_anon` 语义一致。

---

**共享内存实现**（实际代码: `os/servers/vm/src/memtype.rs`）

```rust
pub struct SharedMemory;

impl MemType for SharedMemory {
    fn name(&self) -> &'static str {
        "shared memory"
    }

    fn writable(&self, _frames: &PageFrames, slot: PageSlot, _region: &VirRegion) -> bool {
        slot.is_mapped()
    }

    fn ev_pagefault(
        &self,
        _proc: &ActiveProc<'_>,
        _region: &mut VirRegion,
        _frames: &mut PageFrames,
        _offset: VirBytes,
        _write: bool,
    ) -> Result<PagefaultResult, MemTypeError> {
        // TODO(P0): Minix3's shared_pagefault maps the shared segment's physical page
        // into the faulting process's address space. Shared memory never triggers
        // CoW — writes go to the shared page directly. Current default (Handled)
        // is incorrect for unmapped shared pages; should map the shared page on
        // first access (similar to DirectPhysical's ev_pagefault).
        //
        // Minix3 的 mem_shared.c shared_pagefault():
        //   phys == MAP_NONE → 从共享段获取物理页并映射 → return OK
        //   phys != MAP_NONE → 页面已映射 → return OK
        //
        // 当前实现返回 Handled（默认），对于未映射的共享页是错误行为。
        Ok(PagefaultResult::Handled)
    }
}
```

> **与 Minix3 的对应**: 共享内存不支持 CoW（`mem_shared.c`）。写操作直接修改共享物理页，所有共享进程可见。当前实现为占位（stub），待实现。
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

**Rust 错误类型设计**（实际代码: `os/servers/vm/src/cow_exec_pf.rs`, `os/servers/vm/src/memtype.rs`）

```rust
/// handle_pagefault 的错误类型（实际代码: cow_exec_pf.rs）
#[derive(Debug)]
pub(crate) enum CowError {
    NoMemory,       // PFN 分配失败（对应 Minix3 ENOMEM）
    PageNotMapped,  // 页面未映射
    NoMemType,      // 区域无 memtype 关联
}

/// cow_resolve_core 的错误类型（实际代码: cow_exec_pf.rs）
#[derive(Debug)]
pub(crate) enum CowCoreError {
    NoMemory,
    PageNotMapped,
}

/// memtype 操作的错误类型（实际代码: memtype.rs）
#[derive(Debug)]
pub(crate) enum MemTypeError {
    // 当前为空枚举，各 memtype 实现通过 PagefaultResult 表达不同的处理结果
}
```

> **设计要点**: Minix3 使用整型错误码（`ENOMEM`, `EFAULT`, `EACCES`, `SUSPEND`），Rust 通过两层机制替代：
> 1. **错误类型枚举** (`CowError`, `CowCoreError`): 对真正的失败（如内存不足）使用 `Result::Err`
> 2. **结果枚举** (`PagefaultResult`, `PagefaultAction`): 对非错误的处理路径（如 NeedCow, Handled）使用 `Result::Ok` 内的枚举变体，避免将正常控制流当作错误传播

**与 Minix3 错误码的对应**

| Minix3 错误码 | Rust 处理方式 | 说明 |
|--------------|-------------|------|
| `ENOMEM` (alloc_mem 失败) | `CowError::NoMemory` | PFN 分配失败，通过 `?` 向上传播 |
| `EFAULT` (map_lookup 返回 NULL) | 调用者通过 `ActiveProc` 查找 region，返回 `Option` | 不存在的区域直接 SIGSEGV |
| `EACCES` (写只读区域) | `PagefaultResult::AccessViolation` | 权限违规作为处理结果，非错误 |
| `SUSPEND` (mappedfile_pagefault) | **未实现** | 异步 I/O 回调机制（对应 Minix3 `pf_cont`）尚未实现，见 §4.1 说明 |

**关键简化**: 方案 A 通过 PFN index model 消除了 `PhysBlock` 的独立分配/引用操作——`PhysBlock` 的分配失败（`pb_new` → `ENOMEM`）、引用失败（`pb_reference` → `ENOMEM`）在方案 A 中合并为 PFN 分配器的 `alloc_pfn()` → `CowError::NoMemory`一条路径。

**错误传播示例**（实际代码: `cow_exec_pf.rs`）

```rust
pub(crate) fn handle_pagefault(
    proc: &ActiveProc<'_>,
    region: &mut VirRegion,
    frames: &mut PageFrames,
    alloc: &mut dyn PfnAllocator,
    fault_addr: VirBytes,
    write: bool,
) -> Result<PagefaultAction, CowError> {
    let offset = VirBytes(fault_addr.0 - region.vaddr.0);

    let memtype = region.def_memtype
        .ok_or(CowError::NoMemType)?;

    let result = memtype.ev_pagefault(proc, region, frames, offset, write)?;

    match result {
        PagefaultResult::Handled => Ok(PagefaultAction::Handled),
        PagefaultResult::NeedNewPage => {
            alloc_and_map(region, frames, alloc, offset, memtype)?;
            Ok(PagefaultAction::MappedNewPage)
        }
        PagefaultResult::NeedCow => {
            cow_resolve(region, frames, alloc, offset)?;
            Ok(PagefaultAction::CowResolved)
        }
        PagefaultResult::AccessViolation => {
            Ok(PagefaultAction::AccessViolation)
        }
    }
}
```

**错误恢复策略**

| PagefaultAction | 调用者动作 | 对应 Minix3 行为 |
|----------------|-----------|----------------|
| `Handled` / `MappedNewPage` / `CowResolved` | 更新页表（`pt_writemap`），恢复进程执行 | `map_ph_writept` + 返回 OK |
| `AccessViolation` | 发送 SIGSEGV | `sys_kill(ep, SIGSEGV)` |
| `Err(CowError::NoMemory)` | 发送 SIGSEGV（Minix3 无 OOM killer） | `sys_kill(ep, SIGSEGV)` |

---

## 4. 实现详解

### 4.1 页错误入口

> **注意**: 以下描述基于实际代码 `os/servers/vm/src/cow_exec_pf.rs`。文档之前描述的 `PageFaultHandler` 和 `update_pt` 是早期设计，实际代码采用更简洁的函数式设计。

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
```

**页错误处理入口**（实际代码: `os/servers/vm/src/cow_exec_pf.rs`）

```rust
/// VM page fault handler entry point.
///
/// Dispatches to the region's `MemType::ev_pagefault`, then acts on the
/// returned `PagefaultResult`: allocate a new page, resolve CoW, or report
/// an access violation.
pub(crate) fn handle_pagefault(
    proc: &ActiveProc<'_>,
    region: &mut VirRegion,
    frames: &mut PageFrames,
    alloc: &mut dyn PfnAllocator,
    fault_addr: VirBytes,
    write: bool,
) -> Result<PagefaultAction, CowError> {
    let offset = VirBytes(fault_addr.0 - region.vaddr.0);

    let memtype = region.def_memtype
        .ok_or(CowError::NoMemType)?;

    let result = memtype.ev_pagefault(proc, region, frames, offset, write)?;

    match result {
        PagefaultResult::Handled => Ok(PagefaultAction::Handled),
        PagefaultResult::NeedNewPage => {
            alloc_and_map(region, frames, alloc, offset, memtype)?;
            Ok(PagefaultAction::MappedNewPage)
        }
        PagefaultResult::NeedCow => {
            cow_resolve(region, frames, alloc, offset)?;
            Ok(PagefaultAction::CowResolved)
        }
        PagefaultResult::AccessViolation => {
            Ok(PagefaultAction::AccessViolation)
        }
    }
}
```

**关键设计决策**: `handle_pagefault` 不包含页表更新（`pt_writemap`）。页表写入由调用者负责，因为 `handle_pagefault` 接收 `&ActiveProc`（不可变借用），而页表写入需要 `&mut VmProc`（可变借用）。调用者根据 `PagefaultAction` 变体决定是否执行页表更新：

| PagefaultAction | 调用者动作 |
|----------------|-----------|
| `Handled` | 页面已就绪，可能不需要页表更新（memtype 内部已处理）|
| `MappedNewPage` | 执行 `pt_writemap` 将新 PFN 写入页表 |
| `CowResolved` | 执行 `pt_writemap` 将新 PFN 写入页表 |
| `AccessViolation` | 发送 SIGSEGV |

> **与 Minix3 的差异**: Minix3 的 `handle_pagefault` 内部隐式调用 `map_ph_writept`（通过 `map_pf` → 末尾的 `map_ph_writept`）。方案 A 将此步骤推迟到调用者，实现了页表操作的延迟和借用模型的清晰化。

**alloc_and_map 辅助函数**

```rust
pub(crate) fn alloc_and_map(
    region: &mut VirRegion,
    frames: &mut PageFrames,
    alloc: &mut dyn PfnAllocator,
    offset: VirBytes,
    memtype: &'static dyn MemType,
) -> Result<u32, CowError> {
    let pfn = alloc.alloc_pfn()
        .map_err(|_| CowError::NoMemory)?;
    region.map_page(frames, offset, pfn, memtype);
    Ok(pfn)
}
```

**错误类型**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CowError {
    NoMemory,
    NoMemType,
    PageNotMapped,
    MemType(MemTypeError),
}
```

**与 Minix3 的关键差异**

| Minix3 | 方案 A | 说明 |
|--------|-------|------|
| `physblock_get(region, offset)` | `region.get_slot(offset)` | O(1) Vec 索引替代链表遍历 |
| `pb_new(MAP_NONE)` + `pb_reference()` | `region.map_page(frames, offset, pfn, mt)` | 直接映射，无需分配 PhysBlock |
| `ph->memtype->ev_pagefault(vmp, region, ph, ...)` | `memtype.ev_pagefault(proc, region, frames, offset, write)` | PageSlot + PageFrames 替代 PhysRegion |
| `pb_unreferenced(region, ph, 0)` + `pb_link(ph, pb, ...)` | `region.unmap_page` → `ev_unreference` + `free_pfn` | 两阶段释放，无侵入式链表 |
| `sys_abscopy(old, new, PAGE_SIZE)` | `copy_nonoverlapping(vm_phys_to_virt(old), vm_phys_to_virt(new), PAGE_SIZE)` | Direct Map 替代内核系统调用 |
| `map_ph_writept(vmp, vr, pr)` | 调用者负责 `pt_writemap` | 页表更新推迟到调用者 |

> **异步回调**: Minix3 的 `pf_cont` / `handle_memory_continue` 异步回调机制在方案 A 中尚未实现。当前 `PagefaultAction` 不包含 `Suspend` 变体。这是 P1 级别的实现缺口（见 [19-cow-exec-pagefault.md](19-cow-exec-pagefault.md)）。

**消息循环集成**（调用者示例）

```rust
// VM 服务器主循环中处理页错误：
match msg.m_type {
    VM_PAGEFAULT => {
        let ep = msg.m_source;
        if let Some(vmp) = self.proc_table.get_mut(ep) {
            let region = vmp.lookup_region_mut(msg.vpf_addr);
            let action = handle_pagefault(
                vmp.as_active(), region, &mut self.frames,
                &mut self.buddy, msg.vpf_addr,
                (msg.vpf_flags & PFE_WRITE) != 0,
            );
            match action {
                Ok(PagefaultAction::MappedNewPage | PagefaultAction::CowResolved) => {
                    pt_writemap(vmp, ...)?;
                }
                Ok(PagefaultAction::AccessViolation) => {
                    sys_kill(ep, SIGSEGV);
                }
                _ => {}
            }
            kernel::vmctl_clear_pagefault(ep);
        }
    }
    _ => {}
}

### 4.2 地址解析

地址解析将虚拟地址转换为对应的虚拟区域和 PageSlot。

**地址解析结构**

```rust
/// 地址解析结果
#[derive(Debug)]
pub struct AddressResolution {
    /// 虚拟区域引用
    pub region: &'static VirRegion,
    /// 区域内偏移（页对齐）
    pub offset: VirBytes,
    /// 物理页槽（如果已映射）
    pub slot: Option<PageSlot>,
}

impl VmProc {
    /// 解析虚拟地址（对应 Minix3 map_lookup + physblock_get）
    pub fn resolve_address(&self, vaddr: VirtAddr) -> Option<AddressResolution> {
        // 1. 查找虚拟区域（AVL 树，O(log n)）
        let region = self.lookup_region(vaddr)?;

        // 2. 计算区域内偏移
        let offset = VirBytes(vaddr.align_down(PAGE_SIZE) - region.vaddr);

        // 3. 查找 PageSlot（Vec 索引，O(1)）
        let page_idx = (offset.get() / PAGE_SIZE) as usize;
        let slot = region.physblocks.get(page_idx).copied().flatten();

        Some(AddressResolution {
            region,
            offset,
            slot,
        })
    }
}
```

**AVL 树查找实现**

```rust
impl RegionAvlTree {
    /// 查找包含指定地址的区域（对应 Minix3 region_search + map_lookup 验证）
    pub fn find(&self, vaddr: VirtAddr) -> Option<&VirRegion> {
        // 使用 AVL_LESS_EQUAL 查找
        let candidate = self.search(vaddr, SearchType::LessEqual)?;

        // 验证地址确实在区域内
        if vaddr >= candidate.vaddr && vaddr < candidate.vaddr + candidate.length {
            Some(candidate)
        } else {
            None
        }
    }
}
```

**PageSlot 查找**

```rust
impl VirRegion {
    /// 获取指定偏移处的 PageSlot（对应 Minix3 physblock_get）
    ///
    /// Minix3 使用 region->physblocks[i] 指针数组索引，方案 A 使用 Vec<Option<PageSlot>> 索引。
    /// 两者都是 O(1) 操作，但方案 A 的 PageSlot 是 Copy 类型，无需堆分配。
    pub fn get_slot(&self, offset: VirBytes) -> Option<PageSlot> {
        let page_idx = (offset.get() / PAGE_SIZE) as usize;
        self.physblocks.get(page_idx).copied().flatten()
    }

    /// 获取或创建懒映射 PageSlot（对应 Minix3 physblock_get + pb_new + pb_reference）
    pub fn get_or_create_slot(&mut self, offset: VirBytes) -> Option<PageSlot> {
        let page_idx = (offset.get() / PAGE_SIZE) as usize;
        if self.physblocks[page_idx].is_none() {
            self.map_lazy(offset);
        }
        self.physblocks[page_idx]
    }
}
```

**地址解析流程**

1. AVL 树查找 `SearchType::LessEqual` → 找到 `vaddr <= addr` 的候选区域
   - 未找到 → 返回 None
2. 验证 `vaddr >= region.vaddr && vaddr < region.vaddr + region.length`
   - 不在区域内 → 返回 None
3. 计算偏移 `offset = vaddr - region.vaddr`
4. `Vec` 索引查找 PageSlot → `region.physblocks[page_idx]`
   - 找到 → 返回 `AddressResolution { region, offset, slot: Some(..) }`
   - 未找到 → 返回 `AddressResolution { region, offset, slot: None }`

**与 Minix3 的对比**

| Minix3 | 方案 A | 说明 |
|--------|-------|------|
| `region->physblocks[i]` (指针数组) | `region.physblocks[page_idx]` (Vec) | O(1) 索引，但 PageSlot 是 Copy 类型 |
| `physblock_get()` 返回 `phys_region*` | `get_slot()` 返回 `Option<PageSlot>` | 无裸指针，Option 强制空检查 |
| `pb_new(MAP_NONE)` + `pb_reference()` | `map_lazy(offset)` | 懒映射无需堆分配 |

**地址范围检查**

```rust
impl VmProc {
    /// 检查地址范围是否有效（对应 Minix3 handle_memory_start）
    pub fn check_address_range(
        &self,
        start: VirtAddr,
        len: usize,
        write: bool,
    ) -> Result<(), PageFaultError> {
        let mut current = start;
        let end = start + len;

        while current < end {
            let region = self.lookup_region(current)
                .ok_or(PageFaultError::InvalidAddress(current))?;

            if write && !region.flags.contains(VrFlags::WRITABLE) {
                return Err(PageFaultError::PermissionDenied {
                    addr: current,
                    access: AccessType::Write,
                });
            }

            current = region.vaddr + region.length;
        }

        Ok(())
    }
}
```

### 4.3 CoW 处理路径

CoW 处理路径负责处理写保护错误，实现写时复制。

**CoW 处理流程（方案 A）**

**cow_resolve_core — 核心 CoW 解析**（实际代码：`os/servers/vm/src/cow_exec_pf.rs`）

```rust
/// Core CoW resolution: allocate a new physical page, copy content from the
/// shared page, unmap the old slot and map the new one as `MEM_TYPE_ANON`.
///
/// If `refcount <= 1` the page is already private and no copy is needed.
pub(crate) fn cow_resolve_core(
    region: &mut VirRegion,
    frames: &mut PageFrames,
    alloc: &mut dyn PfnAllocator,
    offset: VirBytes,
) -> Result<u32, CowCoreError> {
    // 1. 获取当前 PageSlot
    let slot = region.get_slot(offset)
        .ok_or(CowCoreError::PageNotMapped)?;

    if !slot.is_mapped() {
        return Err(CowCoreError::PageNotMapped);
    }

    let old_pfn = slot.pfn;
    let refcount = frames.get(old_pfn)
        .map(|s| s.refcount)
        .unwrap_or(0);

    // 2. 快速路径: refcount <= 1，页面已是私有，无需 CoW
    if refcount <= 1 {
        return Ok(old_pfn);
    }

    // 3. 分配新物理页（通过 PfnAllocator，对应 Minix3 alloc_mem）
    let new_pfn = alloc.alloc_pfn()
        .map_err(|_| CowCoreError::NoMemory)?;

    // 4. 复制页面内容（Direct Map + copy_nonoverlapping，替代 sys_abscopy）
    copy_page_content(frames, old_pfn, new_pfn);

    // 5. unmap 旧页面，递减 refcount
    let pending = region.unmap_page(frames, offset);

    // 6. map 新页面为匿名内存（对应 Minix3 ph->memtype = &mem_type_anon）
    region.map_page(frames, offset, new_pfn, &MEM_TYPE_ANON);

    // 7. 如果旧页 refcount 降为 0 且非缓存页，释放物理页
    //    （对应 Minix3 pb_unreferenced 后的释放逻辑）
    if let Some((pfn, mt)) = pending {
        mt.ev_unreference(frames, pfn);
        alloc.free_pfn(pfn);
    }

    // 8. Debug 构建：验证 CoW 后一致性
    #[cfg(debug_assertions)]
    verify_cow_consistency(frames, old_pfn, new_pfn, region, offset);

    Ok(new_pfn)
}
```

**copy_page_content — 页面复制**（对应 Minix3 `sys_abscopy(ph->ph->phys, new_page, VM_PAGE_SIZE)`）

```rust
#[cfg(not(test))]
fn copy_page_content(frames: &PageFrames, src_pfn: u32, dst_pfn: u32) {
    // SAFETY: pfn_to_phys 返回页对齐的物理地址（PAGE_SIZE 的倍数）。
    // AlignedPhysBytes::new_unchecked 要求参数页对齐，
    // 由 PageFrames 不变式保证所有 PFN 映射到页对齐的物理地址。
    let src_phys = AlignedPhysBytes::new_unchecked(frames.pfn_to_phys(src_pfn).0);
    let dst_phys = AlignedPhysBytes::new_unchecked(frames.pfn_to_phys(dst_pfn).0);
    let src_ptr = vm_phys_to_virt(src_phys).0 as *const u8;
    let dst_ptr = vm_phys_to_virt(dst_phys).0 as *mut u8;
    // SAFETY: src_ptr 和 dst_ptr 指向不同的物理页（调用者保证 src_pfn != dst_pfn），
    // 因此内存区域不重叠。两个页面均通过 direct map 区域映射且可访问。
    unsafe {
        core::ptr::copy_nonoverlapping(src_ptr, dst_ptr, PAGE_SIZE as usize);
    }
}
```

> **与 19-cow-exec-pagefault.md §5.2 “CoW 复制 → Direct Map 方案” 对应**：`copy_page_content` 的 Direct Map 实现是方案四的典型应用——Minix3 需要 `createpde` 临时映射窗口调用 `sys_abscopy`，方案四只需 `vm_phys_to_virt()` + `copy_nonoverlapping`。

**verify_cow_consistency — Debug 一致性验证**

```rust
/// Debug-only CoW consistency verification.
///
/// After CoW resolution, asserts that refcounts and slot mappings are correct:
/// - old_pfn: refcount should be decremented (was shared, now private to other owner)
/// - new_pfn: refcount should be 1 (newly allocated, owned by this region)
/// - region's slot at `offset` should point to new_pfn
#[cfg(debug_assertions)]
fn verify_cow_consistency(
    frames: &PageFrames,
    old_pfn: u32,
    new_pfn: u32,
    region: &VirRegion,
    offset: VirBytes,
) {
    if let Some(old_state) = frames.get(old_pfn) {
        assert!(
            old_state.refcount >= 1,
            "old_pfn {} refcount should be >= 1 after CoW, got {}",
            old_pfn, old_state.refcount
        );
    }

    if let Some(new_state) = frames.get(new_pfn) {
        assert_eq!(
            new_state.refcount, 1,
            "new_pfn {} refcount should be 1 after CoW, got {}",
            new_pfn, new_state.refcount
        );
    }

    if let Some(slot) = region.get_slot(offset) {
        assert!(
            slot.is_mapped(),
            "slot at offset {:?} should be mapped after CoW",
            offset
        );
        assert_eq!(
            slot.pfn, new_pfn,
            "slot at offset {:?} should point to new_pfn {}, got {}",
            offset, new_pfn, slot.pfn
        );
    }
}
```

**CowCoreError — CoW 核心错误类型**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CowCoreError {
    NoMemory,         // PfnAllocator::alloc_pfn 失败
    PageNotMapped,    // 页面未映射（get_slot 返回 None 或 slot.is_mapped() == false）
}
```

**cow_resolve_region — 批量 CoW 解析**

```rust
/// Resolve CoW for all pages in a region that need it.
///
/// Iterates over every page slot; if `needs_cow` is true, performs
/// `cow_resolve` on that page. Returns the number of pages resolved.
pub(crate) fn cow_resolve_region(
    region: &mut VirRegion,
    frames: &mut PageFrames,
    alloc: &mut dyn PfnAllocator,
) -> Result<usize, CowError> {
    let num_pages = region.physblocks.len();
    let mut resolved = 0;

    for i in 0..num_pages {
        let offset = VirBytes((i as u64) * PAGE_SIZE);
        if region.needs_cow(frames, offset) {
            cow_resolve(region, frames, alloc, offset)?;
            resolved += 1;
        }
    }

    Ok(resolved)
}
```

**与 Minix3 mem_cow 的完整对应**

| Minix3 步骤 | 方案 A 步骤 | 说明 |
|------------|-----------|------|
| `alloc_mem(1, allocflags)` | `alloc.alloc_pfn()` | 通过 PfnAllocator trait 分配 PFN |
| `sys_abscopy(ph->ph->phys, new_page, PAGE_SIZE)` | `copy_page_content(frames, old_pfn, new_pfn)` | Direct Map + copy_nonoverlapping |
| `pb_new(new_page)` | 不需要 | PageFrames 全局数组，无需创建 PhysBlock |
| `pb_unreferenced(region, ph, 0)` | `region.unmap_page(frames, offset)` | 递减 refcount 并返回待释放信息 |
| `pb_link(ph, pb, ph->offset, region)` | `region.map_page(frames, offset, new_pfn, &MEM_TYPE_ANON)` | 直接映射，memtype 切换为匿名 |
| `ph->memtype = &mem_type_anon` | `&MEM_TYPE_ANON` 作为 map_page 参数 | CoW 后切换为匿名内存 |
| `pb_unreferenced` 后 refcount=0 的释放 | `ev_unreference` + `alloc.free_pfn` | 两阶段释放：通知 memtype → 释放 PFN |
| — | `verify_cow_consistency` | 方案 A 新增：debug 构建下的完整性断言 |
| — | `cow_resolve_region` 批量 CoW | 方案 A 新增：遍历整个区域执行 CoW |

**关键简化**: Minix3 的 `mem_cow` 需要 5 步（分配 → 复制 → 创建 PhysBlock → 解除旧引用 → 链接新引用），方案 A 的核心路径只需 3 步（分配 → 复制 → unmap+map），因为 `PageFrames` 全局数组无需创建/销毁 `PhysBlock` 对象。此外 `refcount <= 1` 快速路径避免了不必要的分配和复制。

> **Direct Map 统一性**: `vm_phys_to_virt()` 使物理页直接可操作。Minix3 中 `sys_abscopy` 是内核系统调用（VM 无法直接访问物理页）；方案 A 中 `vm_phys_to_virt()` 将物理地址转换为虚拟地址，复制变成一行 `copy_nonoverlapping`。这与 [14-cow-mechanism.md](14-cow-mechanism.md) 的 `mem_cow()` 简化是同一个范式转变。

### 4.4 按需加载路径

按需加载路径处理首次访问未映射页面的情况。在方案 A 中，按需加载通过 `MemType::ev_pagefault` 返回 `NeedNewPage`，由 `handle_pagefault` 统一调用 `alloc_and_map` 完成。

**`alloc_and_map` — 分配物理页并映射**（实际代码: `os/servers/vm/src/cow_exec_pf.rs`）

```rust
/// Allocate a new physical page via PfnAllocator, then map it into the region.
/// Called when memtype's ev_pagefault returns NeedNewPage.
pub(crate) fn alloc_and_map(
    region: &mut VirRegion,
    frames: &mut PageFrames,
    alloc: &mut dyn PfnAllocator,
    offset: VirBytes,
    memtype: &'static dyn MemType,
) -> Result<u32, CowError> {
    let pfn = alloc.alloc_pfn()
        .map_err(|_| CowError::NoMemory)?;

    region.map_page(frames, offset, pfn, memtype);

    Ok(pfn)
}
```

**按需加载触发路径**

```
handle_pagefault()
  → memtype.ev_pagefault(...)
    → 返回 NeedNewPage
      → alloc_and_map(region, frames, alloc, offset, memtype)
        → alloc.alloc_pfn()      // 分配物理页
        → region.map_page(...)   // 更新 PageSlot + refcount
      → 返回 MappedNewPage
    → 调用者执行 pt_writemap    // 写入页表
```

> **与 Minix3 的对应**: Minix3 的 `map_pf` 中 `phys == MAP_NONE` 分支由各 `mem_type.ev_pagefault` 处理。方案 A 将"分配+映射"分离为 `alloc_and_map`，使 `handle_pagefault` 统一处理所有内存类型的分配，避免每个 memtype 实现重复分配逻辑。

**匿名内存按需加载**

匿名内存（`AnonymousMemory`）的 `ev_pagefault` 在 slot 为空时返回 `NeedNewPage`（§3.2）。`alloc_and_map` 分配物理页并映射后，页面内容在 Direct Map 下已隐式清零（物理分配器保证），无需显式 `write_bytes`。

**文件映射按需加载**

文件映射（`MappedFile`）的按需加载在方案 A 中尚未完全实现。Minix3 的 `mappedfile_pagefault` 在 `phys == MAP_NONE` 时通过 `vfs_request(VMVFSREQ_FDIO, ...)` 发起异步 I/O 从磁盘读取文件内容，返回 `SUSPEND`。当前 `MappedFile::ev_pagefault` 简化地返回 `NeedNewPage` 处理 slot 为空的情况，异步 VFS 回调机制（对应 Minix3 的 `pf_cont` / `handle_memory_continue`）留待后续实现。

### 4.5 栈扩展

> **设计草图**: Minix3 不支持栈自动扩展（栈大小由 `do_brk` 静态分配），以下为 minix-rs 未来可能引入的改进特性设计草图，尚未在实际代码中实现。

栈扩展处理栈区域的自动增长，当访问超出当前栈边界时自动扩展。

> **重要**: Minix3 不支持栈自动扩展。访问不在已映射区域内的地址会直接触发 SIGSEGV。栈和数据段的增长由 `do_brk()` 系统调用管理（见 [17-vm-brk.md](17-vm-brk.md)）。minix-rs 可考虑实现自动栈扩展作为改进。

**栈区域特征**

```rust
bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct StackFlags: u32 {
        const GROWSDOWN = 0x01;
        const LIMITED = 0x02;
    }
}

pub struct StackRegion {
    base: VirRegion,
    stack_flags: StackFlags,
    stack_low: VirtAddr,
    max_size: usize,
}

impl StackRegion {
    pub fn new(
        vaddr: VirtAddr,
        initial_size: usize,
        max_size: usize,
    ) -> Result<Self, PageFaultError> {
        let base = VirRegion::new(
            vaddr - initial_size,
            initial_size,
            VrFlags::WRITABLE | VrFlags::ANON,
        ).map_err(|_| PageFaultError::OutOfMemory)?;

        Ok(Self {
            base,
            stack_flags: StackFlags::GROWSDOWN | StackFlags::LIMITED,
            stack_low: vaddr - max_size,
            max_size,
        })
    }

    pub fn can_grow(&self, addr: VirtAddr) -> bool {
        addr >= self.stack_low && addr < self.base.vaddr
    }

    pub fn grow(&mut self, addr: VirtAddr) -> Result<(), PageFaultError> {
        if !self.can_grow(addr) {
            return Err(PageFaultError::InvalidAddress(addr));
        }

        let new_vaddr = addr.align_down(PAGE_SIZE);
        let new_size = self.base.vaddr + self.base.length - new_vaddr;

        if new_size > self.max_size {
            return Err(PageFaultError::InvalidAddress(addr));
        }

        let growth = self.base.vaddr - new_vaddr;
        self.base.vaddr = new_vaddr;
        self.base.length = new_size;

        Ok(())
    }
}
```

**栈扩展处理器**

```rust
pub struct StackGrowHandler {
    max_stack_size: usize,
}

impl StackGrowHandler {
    pub fn new(max_stack_size: usize) -> Self {
        Self { max_stack_size }
    }

    pub fn handle(
        &self,
        vmp: &mut VmProc,
        addr: VirtAddr,
    ) -> Result<(), PageFaultError> {
        let stack = vmp.stack_region_mut()
            .ok_or(PageFaultError::InvalidAddress(addr))?;

        if !stack.can_grow(addr) {
            return Err(PageFaultError::InvalidAddress(addr));
        }

        stack.grow(addr)
    }
}
```

**栈扩展流程**

如果实现自动栈扩展，流程为：
1. 页错误访问栈区域外地址 → `lookup_region()` 查找栈区域
2. 未找到 → 检查地址是否在可扩展范围（`addr >= stack_low`）
   - 否 → SIGSEGV
   - 是 → 扩展栈区域，分配新页面，处理页错误
3. 找到区域 → 正常页错误处理

**栈保护页**

```rust
pub struct StackGuard {
    guard_size: usize,
    guard_addr: Option<VirtAddr>,
}

impl StackGuard {
    pub fn new(guard_size: usize) -> Self {
        Self { guard_size, guard_addr: None }
    }

    pub fn is_guard(&self, addr: VirtAddr) -> bool {
        if let Some(guard) = self.guard_addr {
            addr >= guard && addr < guard + self.guard_size
        } else {
            false
        }
    }

    pub fn update_guard(&mut self, new_stack_bottom: VirtAddr) {
        self.guard_addr = Some(new_stack_bottom - self.guard_size);
    }
}

impl StackRegion {
    pub fn grow_with_guard(
        &mut self,
        addr: VirtAddr,
        guard: &mut StackGuard,
    ) -> Result<(), PageFaultError> {
        if guard.is_guard(addr) {
            return Err(PageFaultError::InvalidAddress(addr));
        }
        self.grow(addr)?;
        guard.update_guard(self.base.vaddr);
        Ok(())
    }
}
```

**栈限制配置**

```rust
#[derive(Debug, Clone)]
pub struct StackLimits {
    pub default_size: usize,
    pub max_size: usize,
    pub guard_size: usize,
}

impl Default for StackLimits {
    fn default() -> Self {
        Self {
            default_size: 128 * 1024,
            max_size: 8 * 1024 * 1024,
            guard_size: PAGE_SIZE,
        }
    }
}
```

---

## 5. 性能优化

> **设计草图**: 以下性能优化方案为设计草图，尚未在实际代码中实现。当前 minix-rs VM 服务器为单线程事件驱动模型（和 Minix3 一致），快速路径优化和锁粒度设计留待性能分析和多线程引入后实施。

### 5.1 快速路径

快速路径优化常见情况的页错误处理，减少处理延迟。

**快速路径识别**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FastPath {
    Cow,
    FirstAccess,
    NoOp,
    SlowPath,
}

impl PageFaultHandler {
    pub fn detect_fast_path(
        &self,
        frames: &PageFrames,
        slot: Option<PageSlot>,
        write: bool,
    ) -> FastPath {
        match slot {
            None => FastPath::FirstAccess,
            Some(s) => {
                if !s.is_mapped() {
                    return FastPath::FirstAccess;
                }

                let state = frames.get(s.pfn).unwrap();
                if write && state.refcount > 1 {
                    return FastPath::Cow;
                }

                FastPath::NoOp
            }
        }
    }
}
```

> **与 Minix3 的对应**: Minix3 的 `map_pf` 中 `physblock_get` 返回 NULL 对应 `PageSlot::None`（首次访问），`ph->ph->phys == MAP_NONE` 对应 `!s.is_mapped()`，`ph->ph->refcount > 1` 对应 `state.refcount > 1`。方案 A 的 `Vec<Option<PageSlot>>` 索引是 O(1)，比 Minix3 的指针数组索引更紧凑（PageSlot 是 Copy 类型，4+8+8=20 字节 vs 指针 8 字节，但省去了 PhysBlock 堆分配开销）。

**快速路径实现**

```rust
impl PageFaultHandler {
    #[inline]
    pub fn handle_fast(
        &mut self,
        vmp: &mut VmProc,
        region: &mut VirRegion,
        offset: VirBytes,
        write: bool,
    ) -> Result<PageFaultResult, PageFaultError> {
        let page_idx = (offset.get() / PAGE_SIZE) as usize;
        let slot = region.physblocks.get(page_idx).copied().flatten();

        match self.detect_fast_path(self.frames, slot, write) {
            FastPath::FirstAccess => self.fast_alloc(region, offset),
            FastPath::Cow => self.fast_cow(region, offset),
            FastPath::NoOp => Ok(PageFaultResult::Ok),
            FastPath::SlowPath => self.handle_slow(vmp, region, offset, write),
        }
    }

    #[inline]
    fn fast_alloc(
        &mut self,
        region: &mut VirRegion,
        offset: VirBytes,
    ) -> Result<PageFaultResult, PageFaultError> {
        let pfn = self.frames.alloc_phys_page()
            .map_err(|_| PageFaultError::OutOfMemory)?;

        let va = vm_phys_to_virt(self.frames.pfn_to_phys(pfn));
        unsafe { core::ptr::write_bytes(va.as_mut_ptr(), 0, PAGE_SIZE); }

        let memtype = region.def_memtype.unwrap();
        region.map_page(self.frames, offset, pfn, memtype);

        Ok(PageFaultResult::Ok)
    }

    #[inline]
    fn fast_cow(
        &mut self,
        region: &mut VirRegion,
        offset: VirBytes,
    ) -> Result<PageFaultResult, PageFaultError> {
        let page_idx = (offset.get() / PAGE_SIZE) as usize;
        let src_pfn = region.physblocks[page_idx].unwrap().pfn;

        let new_pfn = self.frames.alloc_phys_page()
            .map_err(|_| PageFaultError::OutOfMemory)?;

        let src_va = vm_phys_to_virt(self.frames.pfn_to_phys(src_pfn));
        let dst_va = vm_phys_to_virt(self.frames.pfn_to_phys(new_pfn));
        unsafe {
            core::ptr::copy_nonoverlapping(src_va.as_ptr(), dst_va.as_mut_ptr(), PAGE_SIZE);
        }

        self.frames.get_mut(src_pfn).unwrap().refcount -= 1;
        region.physblocks[page_idx] = Some(PageSlot {
            pfn: new_pfn,
            offset,
            memtype: Some(ANON_MEMTYPE),
        });
        self.frames.get_mut(new_pfn).unwrap().refcount = 1;

        Ok(PageFaultResult::Ok)
    }
}
```

**快速路径统计**

```rust
#[derive(Debug, Default)]
pub struct FastPathStats {
    pub fast_hits: u64,
    pub slow_hits: u64,
    pub first_access: u64,
    pub cow_count: u64,
    pub noop_count: u64,
}

impl FastPathStats {
    pub fn record_fast(&mut self, path: FastPath) {
        self.fast_hits += 1;
        match path {
            FastPath::FirstAccess => self.first_access += 1,
            FastPath::Cow => self.cow_count += 1,
            FastPath::NoOp => self.noop_count += 1,
            FastPath::SlowPath => self.slow_hits += 1,
        }
    }

    pub fn hit_rate(&self) -> f64 {
        let total = self.fast_hits + self.slow_hits;
        if total == 0 { 0.0 } else { self.fast_hits as f64 / total as f64 }
    }
}
```

**内联优化**

```rust
impl PageFrames {
    #[inline]
    pub fn needs_cow_inline(&self, pfn: u32, write: bool) -> bool {
        write && self.states[pfn as usize].refcount > 1
    }

    #[inline]
    pub fn is_mapped_inline(&self, slot: &Option<PageSlot>) -> bool {
        slot.is_some()
    }
}
```

**批处理优化**

```rust
impl PageFaultHandler {
    pub fn handle_batch(
        &mut self,
        vmp: &mut VmProc,
        faults: &[PageFaultInfo],
    ) -> Vec<Result<PageFaultResult, PageFaultError>> {
        let mut results = Vec::with_capacity(faults.len());

        let mut by_region: BTreeMap<VirtAddr, Vec<usize>> = BTreeMap::new();
        for (i, info) in faults.iter().enumerate() {
            if let Some(region) = vmp.lookup_region(info.vaddr) {
                by_region.entry(region.vaddr).or_default().push(i);
            }
        }

        for (region_vaddr, indices) in by_region {
            if let Some(region) = vmp.lookup_region_mut(region_vaddr) {
                for &i in &indices {
                    let info = &faults[i];
                    let offset = VirBytes(info.vaddr.align_down(PAGE_SIZE) - region_vaddr);
                    let result = self.handle_fast(vmp, region, offset, info.is_write());
                    results.push(result);
                }
            }
        }

        results
    }
}
```

### 5.2 锁粒度（设计草图）

> **设计草图**: 以下锁设计是 minix-rs 多线程改进的预留设计，当前单线程模型下不存在并发问题。

细粒度锁减少锁竞争，提高并发性能。

**锁层次结构**

```rust
/// VM 锁层次
///
/// 锁获取顺序（避免死锁）:
/// 1. VmProcTable 锁（全局进程表）
/// 2. VmProc 锁（进程级）
/// 3. VirRegion 锁（区域级）
/// 4. PageFrames 锁（全局物理页状态表级）
pub struct VmLocks {
    proc_table: RwLock<()>,
    proc_locks: Vec<Mutex<()>>,
    region_lock_pool: LockPool,
}

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

    pub fn get_lock(&self, addr: VirtAddr) -> &Mutex<()> {
        &self.locks[addr.as_usize() & self.mask]
    }
}
```

**细粒度锁定**

```rust
impl PageFaultHandler {
    pub fn handle_with_locking(
        &mut self,
        info: PageFaultInfo,
    ) -> Result<PageFaultResult, PageFaultError> {
        let vmp = self.proc_table.get_by_endpoint(info.endpoint)
            .ok_or(PageFaultError::InvalidAddress(info.vaddr))?;

        let region = {
            let _read = vmp.regions_read_lock();
            vmp.lookup_region(info.vaddr)
                .ok_or(PageFaultError::InvalidAddress(info.vaddr))?
        };

        if info.is_write() && !region.flags.contains(VrFlags::WRITABLE) {
            return Err(PageFaultError::PermissionDenied {
                addr: info.vaddr,
                access: info.access_type,
            });
        }

        let offset = VirBytes(info.vaddr.align_down(PAGE_SIZE) - region.vaddr);
        let _region_lock = region.lock();

        self.handle_page(&vmp, &region, offset, info.is_write())?;

        Ok(PageFaultResult::Ok)
    }
}
```

**读写锁优化**

```rust
impl VmProc {
    pub fn regions_read_lock(&self) -> RwLockReadGuard<'_, ()> {
        self.regions_lock.read().unwrap()
    }

    pub fn regions_write_lock(&self) -> RwLockWriteGuard<'_, ()> {
        self.regions_lock.write().unwrap()
    }

    pub fn try_regions_read_lock(&self) -> Option<RwLockReadGuard<'_, ()>> {
        self.regions_lock.try_read().ok()
    }
}
```

**锁避免策略**

```rust
impl PageFaultHandler {
    pub fn handle_lockfree(
        &mut self,
        info: PageFaultInfo,
    ) -> Result<PageFaultResult, PageFaultError> {
        let vmp = self.proc_table.get_by_endpoint_atomic(info.endpoint)
            .ok_or(PageFaultError::InvalidAddress(info.vaddr))?;

        let region = vmp.lookup_region_rcu(info.vaddr)
            .ok_or(PageFaultError::InvalidAddress(info.vaddr))?;

        if info.is_write() && !region.flags.contains(VrFlags::WRITABLE) {
            return Err(PageFaultError::PermissionDenied {
                addr: info.vaddr,
                access: info.access_type,
            });
        }

        let offset = VirBytes(info.vaddr.align_down(PAGE_SIZE) - region.vaddr);
        self.handle_page_atomic(&region, offset, info.is_write())?;

        Ok(PageFaultResult::Ok)
    }

    fn handle_page_atomic(
        &mut self,
        region: &VirRegion,
        offset: VirBytes,
        write: bool,
    ) -> Result<(), PageFaultError> {
        let page_idx = (offset.get() / PAGE_SIZE) as usize;
        let slot = region.physblocks.get(page_idx).copied().flatten();

        match slot {
            None => {
                let _lock = region.lock();
                self.allocate_page(region, offset)
            }
            Some(s) => {
                let state = self.frames.get(s.pfn).unwrap();
                if !s.is_mapped() {
                    let _lock = region.lock();
                    self.allocate_page(region, offset)
                } else if write && state.refcount > 1 {
                    self.do_cow_atomic(region, offset)
                } else {
                    Ok(())
                }
            }
        }
    }
}
```

**死锁避免**

```rust
#[cfg(debug_assertions)]
pub struct LockOrderChecker {
    held_locks: RefCell<Vec<LockId>>,
}

#[cfg(debug_assertions)]
impl LockOrderChecker {
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

    pub fn record_release(&self, lock_id: LockId) {
        let mut held = self.held_locks.borrow_mut();
        if let Some(pos) = held.iter().position(|&id| id == lock_id) {
            held.remove(pos);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LockId {
    ProcTable = 1,
    Proc(usize) = 2,
    Region(usize) = 3,
    PageFrames(usize) = 4,
}
```

> **与 Minix3 的对比**: Minix3 的 `PhysBlock` 有独立的 `refcount` 原子操作（`AtomicU8`），方案 A 将 `refcount` 放在 `PageFrames.states[pfn]` 中，锁粒度从"物理块级"变为"全局物理页状态表级"。在 Minix3 单线程模型下无差异；若 minix-rs 引入多线程，可考虑对 `PageFrames` 分区加锁或使用原子 `refcount`。

---

## 6. 测试与验证

> **现有测试**: `cow_exec_pf.rs` 中已有 `test_alloc_and_map`、`test_cow_resolve`、`test_cow_resolve_no_sharing`、`test_cow_resolve_region`、`test_cow_resolve_core_refcount_one` 等单元测试，`memtype.rs` 中有 `test_anon_writable`、`test_mapped_file_copy`。以下为补充测试要点。

### 6.1 CoW 触发测试

- **CoW 触发**: 共享页面（refcount=2）写操作 → 触发 CoW，refcount 降为 1，物理地址改变
- **CoW 不触发（单引用快速路径）**: 私有页面（refcount<=1）写操作 → `cow_resolve_core` 快速路径直接返回原 PFN，物理地址不变
- **CoW 不触发（读操作）**: 共享页面读操作 → `ev_pagefault` 返回 `Handled`，refcount 不变
- **CoW 内容复制**: CoW 后新页面内容与原页面一致
- **CoW 一致性验证**: `#[cfg(debug_assertions)]` 下 `verify_cow_consistency` 校验 refcount 和新旧 PFN

### 6.2 非法访问测试

- **空指针访问**: vaddr=0 → 调用者发送 SIGSEGV
- **写只读区域**: 无 VR_WRITABLE 区域写操作 → `PagefaultResult::AccessViolation` + SIGSEGV
- **越界访问**: 超出进程地址空间 → 调用者发送 SIGSEGV
- **栈溢出检测**: 超出最大栈限制 → SIGSEGV

### 6.3 栈扩展测试

> **注意**: Minix3 不支持栈自动扩展，以下为 minix-rs 改进特性的测试要点。

- **栈正常扩展**: 访问栈底以下地址 → 栈区域增长，页面分配
- **栈多次扩展**: 连续多次触发 → 栈区域持续增长
- **保护页**: 访问保护页地址 → InvalidAddress
- **栈扩展后页面清零**: 新分配页面内容全为零

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

- [14-cow-mechanism.md](14-cow-mechanism.md) - CoW 实现（mem_cow 详解）
- [16-vm-fork.md](16-vm-fork.md) - fork 后的首次写入
- [11-region-mapping.md](11-region-mapping.md) - 区域查找与页映射（map_lookup 详解，替代原 vir_region + phys_region）
- [13-region-avl.md](13-region-avl.md) - AVL 树实现（region_search 详解）
- [10-phys-pagestate.md](10-phys-pagestate.md) - 物理页状态管理（PageState.refcount，替代原 pb_new/pb_link/pb_unreferenced）
- [12-memtype.md](12-memtype.md) - 内存类型（mem_type 及 ev_pagefault 分派）
- [05-vm-allocpage.md](05-vm-allocpage.md) - 物理内存分配（alloc_mem/SPAREPAGES）
- [17-vm-brk.md](17-vm-brk.md) - 栈和数据段增长（do_brk）

---

*分类: VM私有*
