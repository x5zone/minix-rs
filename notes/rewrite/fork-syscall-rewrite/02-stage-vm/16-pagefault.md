# 16-pagefault: 页错误处理

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

Minix3 中栈区域的增长由 `do_brk()` 系统调用处理（见 [18-vm-brk.md](18-vm-brk.md)），而非在页错误处理中自动扩展。页错误处理中不包含栈自动增长逻辑——如果访问的地址不在任何已映射区域内，直接发送 SIGSEGV。

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

读者应感受到：**页错误处理中"写入页表项"是 direct map 统一性的关键验证**——如果这个最复杂的操作都能被 `vm_phys_to_virt()` 一步解决，那么 direct map 的统一性就是经得起考验的。这与 17-vm-fork.md §2.8.5 的 fork 页表创建简化是同一个模式——`createpde` 的消失不是"去掉了临时映射步骤"，而是"VM 不再需要内核作为物理页访问的中介"。

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

**安全的页错误处理入口**

```rust
/// 页错误处理入口（对应 Minix3 handle_pagefault）
pub fn handle_pagefault(
    vmp: &mut VmProc,
    frames: &mut PageFrames,
    info: PageFaultInfo,
) -> Result<PageFaultResult, PageFaultError> {
    // 1. 查找虚拟区域（AVL 树，O(log n)）
    let region = vmp.lookup_region_mut(info.vaddr)
        .ok_or(PageFaultError::InvalidAddress(info.vaddr))?;

    // 2. 权限检查：写操作需要 VR_WRITABLE
    if info.is_write() && !region.flags.contains(VrFlags::WRITABLE) {
        return Ok(PageFaultResult::PermissionDenied);
    }

    // 3. 计算区域内偏移
    let offset = VirBytes(info.vaddr.align_down(PAGE_SIZE) - region.vaddr);

    // 4. 获取 PageSlot（Vec 索引，O(1)，替代 physblock_get）
    let page_idx = (offset.get() / PAGE_SIZE) as usize;
    if page_idx >= region.physblocks.len() {
        return Ok(PageFaultResult::InvalidAddress);
    }

    // 5. 如果 PageSlot 不存在，创建懒映射
    if region.physblocks[page_idx].is_none() {
        region.map_lazy(offset);
    }

    // 6. 调用 MemType 的 ev_pagefault
    let slot = region.physblocks[page_idx].unwrap();
    let memtype = slot.memtype.unwrap_or(region.def_memtype.unwrap());

    if !info.is_write() || !memtype.writable(frames, &slot) {
        let action = memtype.ev_pagefault(vmp, region, frames, offset, info.is_write())?;
        match action {
            PageFaultAction::Done => {}
            PageFaultAction::Suspend(_) => return Ok(PageFaultResult::Suspend),
            PageFaultAction::AllocateNewPage { zero } => {
                let pfn = frames.alloc_phys_page()?;
                if zero {
                    let va = vm_phys_to_virt(frames.pfn_to_phys(pfn));
                    unsafe { core::ptr::write_bytes(va.as_mut_ptr(), 0, PAGE_SIZE); }
                }
                region.map_page(frames, offset, pfn, memtype);
            }
            PageFaultAction::CopyOnWrite { src_pfn } => {
                let new_pfn = frames.alloc_phys_page()?;
                let src_va = vm_phys_to_virt(frames.pfn_to_phys(src_pfn));
                let dst_va = vm_phys_to_virt(frames.pfn_to_phys(new_pfn));
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        src_va.as_ptr(), dst_va.as_mut_ptr(), PAGE_SIZE,
                    );
                }
                frames.get_mut(src_pfn).unwrap().refcount -= 1;
                region.physblocks[page_idx] = Some(PageSlot {
                    pfn: new_pfn,
                    offset,
                    memtype: Some(ANON_MEMTYPE),
                });
                frames.get_mut(new_pfn).unwrap().refcount = 1;
            }
        }
    }

    // 7. 更新页表
    let slot = region.physblocks[page_idx].unwrap();
    let phys = frames.pfn_to_phys(slot.pfn);
    let writable = slot.memtype.unwrap().writable(frames, &slot);
    let pt_flags = PTF_PRESENT | PTF_USER | if writable { PTF_WRITE } else { PTF_READ };
    pt_writemap(vmp, region.vaddr + offset.get(), phys, PAGE_SIZE, pt_flags)?;

    Ok(PageFaultResult::Ok)
}
```

**关键安全改进**

| C 代码问题 | Rust 安全封装 |
|-----------|-------------|
| `vmproc[p]` 可能越界 | `VmProcTable::get_by_endpoint()` 返回 Option |
| `region` 可能为 NULL | `lookup_region_mut()` 返回 Option，必须显式处理 |
| `ph->ph->phys` 可能是 `MAP_NONE` | `PageSlot.is_mapped()` + `PageFrames.get(pfn)` 返回 Option |
| `memcpy((void*)phys, ...)` 物理地址直接操作 | `vm_phys_to_virt()` + `copy_nonoverlapping()` 有类型保证 |
| `refcount` 溢出无检测 | `u16` + debug 构建下 saturating_add 断言 |

### 3.2 与 memtype 的集成

页错误处理通过 MemoryType trait 与不同内存类型实现解耦，实现多态处理。

**MemType Trait 扩展**

```rust
/// 内存类型 trait - 页错误处理相关方法（详见 12-memtype.md）
pub trait MemType: Send + Sync {
    /// 内存类型名称
    fn name(&self) -> &'static str;

    /// 处理页错误
    fn ev_pagefault(
        &self,
        vmp: &mut VmProc,
        region: &mut VirRegion,
        frames: &mut PageFrames,
        offset: VirBytes,
        write: bool,
    ) -> Result<PageFaultAction, PageFaultError>;

    /// 检查是否可写
    fn writable(
        &self,
        frames: &PageFrames,
        slot: &PageSlot,
    ) -> bool;

    /// 是否支持 CoW
    fn supports_cow(&self) -> bool {
        false
    }
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
pub struct AnonMemType;

impl MemType for AnonMemType {
    fn name(&self) -> &'static str {
        "anonymous memory"
    }

    fn ev_pagefault(
        &self,
        vmp: &mut VmProc,
        region: &mut VirRegion,
        frames: &mut PageFrames,
        offset: VirBytes,
        write: bool,
    ) -> Result<PageFaultAction, PageFaultError> {
        let page_idx = (offset.get() / PAGE_SIZE) as usize;
        let slot = region.physblocks[page_idx];

        // 情况1: 物理页未分配
        if slot.is_none() {
            return Ok(PageFaultAction::AllocateNewPage { zero: true });
        }

        let slot = slot.unwrap();
        let state = frames.get(slot.pfn).unwrap();

        // 情况2: 不需要 CoW
        if state.refcount < 2 || !write {
            return Ok(PageFaultAction::Done);
        }

        // 情况3: 执行 CoW
        let src_phys = frames.pfn_to_phys(slot.pfn);
        Ok(PageFaultAction::CopyOnWrite { src_pfn: slot.pfn })
    }

    fn writable(
        &self,
        frames: &PageFrames,
        slot: &PageSlot,
    ) -> bool {
        if !slot.is_mapped() {
            return false;
        }
        frames.get(slot.pfn).unwrap().refcount == 1
    }

    fn supports_cow(&self) -> bool {
        true
    }
}

/// 全局匿名内存实例
pub static ANON_MEMTYPE: AnonMemType = AnonMemType;
```

**文件映射内存实现**

```rust
/// 文件映射内存类型
pub struct MappedFileMemType;

impl MemType for MappedFileMemType {
    fn name(&self) -> &'static str {
        "mapped file"
    }

    fn ev_pagefault(
        &self,
        vmp: &mut VmProc,
        region: &mut VirRegion,
        frames: &mut PageFrames,
        offset: VirBytes,
        write: bool,
    ) -> Result<PageFaultAction, PageFaultError> {
        let page_idx = (offset.get() / PAGE_SIZE) as usize;
        let slot = region.physblocks[page_idx];

        // 情况1: 物理页未分配 → 从文件加载
        if slot.is_none() {
            return Ok(PageFaultAction::Suspend(
                AsyncCallback::new(PageFaultInfo {
                    endpoint: vmp.endpoint,
                    vaddr: region.vaddr + offset.get(),
                    fault_type: PageFaultType::NotPresent,
                    access_type: if write { AccessType::Write } else { AccessType::Read },
                    error_code: 0,
                }),
            ));
        }

        let slot = slot.unwrap();

        // 情况2: 读操作，页面已存在
        if !write {
            return Ok(PageFaultAction::Done);
        }

        // 情况3: 写操作 → 执行 CoW（文件映射写时复制为匿名页）
        Ok(PageFaultAction::CopyOnWrite { src_pfn: slot.pfn })
    }

    fn writable(
        &self,
        _frames: &PageFrames,
        _slot: &PageSlot,
    ) -> bool {
        false
    }

    fn supports_cow(&self) -> bool {
        true
    }
}

pub static MAPPED_FILE_MEMTYPE: MappedFileMemType = MappedFileMemType;
```

> **与 Minix3 的对应**: `mappedfile_writable()` 始终返回 0，即文件映射内存从不直接可写，写操作总是触发 CoW。CoW 后 memtype 切换为 `ANON_MEMTYPE`，与 Minix3 的 `ph->memtype = &mem_type_anon` 语义一致。

**共享内存实现**

```rust
/// 共享内存类型
pub struct SharedMemType;

impl MemType for SharedMemType {
    fn name(&self) -> &'static str {
        "shared memory"
    }

    fn ev_pagefault(
        &self,
        _vmp: &mut VmProc,
        region: &mut VirRegion,
        frames: &mut PageFrames,
        offset: VirBytes,
        _write: bool,
    ) -> Result<PageFaultAction, PageFaultError> {
        let page_idx = (offset.get() / PAGE_SIZE) as usize;
        let slot = region.physblocks[page_idx];

        // 共享内存不支持 CoW，物理页未分配时分配新页
        if slot.is_none() {
            return Ok(PageFaultAction::AllocateNewPage { zero: false });
        }

        Ok(PageFaultAction::Done)
    }

    fn writable(
        &self,
        _frames: &PageFrames,
        slot: &PageSlot,
    ) -> bool {
        slot.is_mapped()
    }

    fn supports_cow(&self) -> bool {
        false
    }
}

pub static SHARED_MEMTYPE: SharedMemType = SharedMemType;
```

**页错误处理集成（VirRegion 视角）**

```rust
impl VirRegion {
    /// 处理页错误（对应 Minix3 map_pf）
    pub fn handle_pagefault(
        &mut self,
        vmp: &mut VmProc,
        frames: &mut PageFrames,
        offset: VirBytes,
        write: bool,
    ) -> Result<PageFaultAction, PageFaultError> {
        let page_idx = (offset.get() / PAGE_SIZE) as usize;

        // 如果 PageSlot 不存在，创建懒映射（对应 physblock_get + pb_new）
        if self.physblocks[page_idx].is_none() {
            self.map_lazy(offset);
        }

        let slot = self.physblocks[page_idx].unwrap();
        let memtype = slot.memtype.unwrap_or(self.def_memtype.unwrap());

        // 检查是否需要处理（对应 !write || !writable(pr)）
        if write && memtype.writable(frames, &slot) {
            return Ok(PageFaultAction::Done);
        }

        // 调用 MemType 的 ev_pagefault
        memtype.ev_pagefault(vmp, self, frames, offset, write)
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
#[derive(Debug)]
pub enum PageFaultError {
    /// 地址无效
    InvalidAddress(VirtAddr),
    /// 权限被拒绝
    PermissionDenied {
        addr: VirtAddr,
        access: AccessType,
    },
    /// 内存不足（alloc_phys_page 失败）
    OutOfMemory,
    /// 页表操作失败
    PageTable(PageTableError),
    /// 异步操作失败
    AsyncError(&'static str),
}

/// 页表错误
#[derive(Debug)]
pub enum PageTableError {
    /// 映射失败
    MapFailed { vaddr: VirtAddr, pfn: u32 },
    /// 更新失败
    UpdateFailed(VirtAddr),
}
```

**与 Minix3 错误码的对应**

| Minix3 错误码 | Rust 错误类型 | 说明 |
|--------------|-------------|------|
| `ENOMEM` (map_pf 中 pb_new 失败) | `PageFaultError::OutOfMemory` | alloc_phys_page 失败 |
| `ENOMEM` (anon_pagefault 中 alloc_mem 失败) | `PageFaultError::OutOfMemory` | CoW 分配新页失败 |
| `EFAULT` (map_lookup 返回 NULL) | `PageFaultError::InvalidAddress` | 地址不在任何区域内 |
| `EACCES` (写只读区域) | `PageFaultError::PermissionDenied` | VR_WRITABLE 未设置 |
| `SUSPEND` (mappedfile_pagefault 等待 VFS) | `Ok(PageFaultAction::Suspend(..))` | 非错误，异步等待 |

**关键简化**: 方案 A 消除了 `PhysBlockError` 和 `RegionError`——`PhysBlock` 的分配/引用失败合并为 `PageFaultError::OutOfMemory`（`PageFrames::alloc_phys_page` 返回 `Result`），`PhysRegion` 的查找失败合并为 `PageFaultError::InvalidAddress`（`Vec` 索引越界）。Minix3 中 `pb_new` → `ENOMEM`、`pb_reference` → `ENOMEM` 两条错误路径在方案 A 中合并为 `alloc_phys_page` → `OutOfMemory` 一条。

**错误传播示例**

```rust
pub fn handle_pagefault(
    vmp: &mut VmProc,
    frames: &mut PageFrames,
    info: PageFaultInfo,
) -> Result<PageFaultResult, PageFaultError> {
    // 1. 查找区域 → InvalidAddress
    let region = vmp.lookup_region_mut(info.vaddr)
        .ok_or(PageFaultError::InvalidAddress(info.vaddr))?;

    // 2. 权限检查 → PermissionDenied
    if info.is_write() && !region.flags.contains(VrFlags::WRITABLE) {
        return Err(PageFaultError::PermissionDenied {
            addr: info.vaddr,
            access: info.access_type,
        });
    }

    // 3. MemType 处理 → ? 自动传播
    let offset = VirBytes(info.vaddr.align_down(PAGE_SIZE) - region.vaddr);
    let action = region.handle_pagefault(vmp, frames, offset, info.is_write())?;

    // 4. 执行动作 → OutOfMemory
    match action {
        PageFaultAction::AllocateNewPage { zero } => {
            let pfn = frames.alloc_phys_page()
                .map_err(|_| PageFaultError::OutOfMemory)?;
            // ...
        }
        PageFaultAction::CopyOnWrite { src_pfn } => {
            let new_pfn = frames.alloc_phys_page()
                .map_err(|_| PageFaultError::OutOfMemory)?;
            // ...
        }
        _ => {}
    }

    Ok(PageFaultResult::Ok)
}
```

**错误恢复策略**

| 错误类型 | 恢复策略 | 对应 Minix3 行为 |
|---------|---------|----------------|
| `InvalidAddress` | 发送 SIGSEGV | `sys_kill(ep, SIGSEGV)` |
| `PermissionDenied` | 发送 SIGSEGV | `sys_kill(ep, SIGSEGV)` |
| `OutOfMemory` | 发送 SIGSEGV（Minix3 无 OOM killer） | `sys_kill(ep, SIGSEGV)` |
| `PageTableError` | 发送 SIGSEGV | `sys_kill(ep, SIGSEGV)` |
| `AsyncError` | 发送 SIGSEGV | `sys_kill(ep, SIGSEGV)` |

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
    /// 全局物理页状态表
    frames: &'static mut PageFrames,
}

impl PageFaultHandler {
    /// 创建新的页错误处理器
    pub fn new(frames: &'static mut PageFrames) -> Self {
        Self { frames }
    }

    /// 处理页错误消息（对应 Minix3 do_pagefaults + handle_pagefault）
    pub fn handle_message(
        &mut self,
        vmp: &mut VmProc,
        msg: PageFaultMessage,
    ) {
        let info = PageFaultInfo::from_error_code(
            msg.m_source,
            msg.vpf_addr,
            msg.vpf_flags,
        );

        match self.handle(vmp, info) {
            Ok(PageFaultResult::Ok) => {
                kernel::vmctl_clear_pagefault(msg.m_source);
            }
            Ok(PageFaultResult::Suspend) => {}
            Err(e) => {
                kernel::sys_kill(msg.m_source, Signal::SIGSEGV);
                kernel::vmctl_clear_pagefault(msg.m_source);
            }
        }
    }

    /// 处理页错误
    fn handle(
        &mut self,
        vmp: &mut VmProc,
        info: PageFaultInfo,
    ) -> Result<PageFaultResult, PageFaultError> {
        // 1. 查找虚拟区域
        let region = vmp.lookup_region_mut(info.vaddr)
            .ok_or(PageFaultError::InvalidAddress(info.vaddr))?;

        // 2. 权限检查
        if info.is_write() && !region.flags.contains(VrFlags::WRITABLE) {
            return Err(PageFaultError::PermissionDenied {
                addr: info.vaddr,
                access: info.access_type,
            });
        }

        // 3. 计算偏移
        let offset = VirBytes(info.vaddr.align_down(PAGE_SIZE) - region.vaddr);

        // 4. 处理页面
        let result = self.handle_page(vmp, region, offset, info.is_write())?;

        // 5. 更新缺页统计
        match &result {
            PageFaultResult::Ok => vmp.increment_minor_fault(),
            PageFaultResult::Suspend => vmp.increment_major_fault(),
            _ => {}
        }

        Ok(result)
    }

    /// 处理单个页面（对应 Minix3 map_pf）
    fn handle_page(
        &mut self,
        vmp: &mut VmProc,
        region: &mut VirRegion,
        offset: VirBytes,
        write: bool,
    ) -> Result<PageFaultResult, PageFaultError> {
        let page_idx = (offset.get() / PAGE_SIZE) as usize;

        // 如果 PageSlot 不存在，创建懒映射
        if region.physblocks[page_idx].is_none() {
            region.map_lazy(offset);
        }

        let slot = region.physblocks[page_idx].unwrap();
        let memtype = slot.memtype.unwrap_or(region.def_memtype.unwrap());

        // 检查是否需要处理
        if write && memtype.writable(self.frames, &slot) {
            // 更新页表
            self.update_pt(vmp, region, page_idx)?;
            return Ok(PageFaultResult::Ok);
        }

        // 调用 MemType 的 ev_pagefault
        let action = memtype.ev_pagefault(vmp, region, self.frames, offset, write)?;

        // 执行动作
        match action {
            PageFaultAction::Done => {
                self.update_pt(vmp, region, page_idx)?;
                Ok(PageFaultResult::Ok)
            }
            PageFaultAction::Suspend(_) => Ok(PageFaultResult::Suspend),
            PageFaultAction::AllocateNewPage { zero } => {
                let pfn = self.frames.alloc_phys_page()
                    .map_err(|_| PageFaultError::OutOfMemory)?;
                if zero {
                    let va = vm_phys_to_virt(self.frames.pfn_to_phys(pfn));
                    unsafe { core::ptr::write_bytes(va.as_mut_ptr(), 0, PAGE_SIZE); }
                }
                region.map_page(self.frames, offset, pfn, memtype);
                self.update_pt(vmp, region, page_idx)?;
                Ok(PageFaultResult::Ok)
            }
            PageFaultAction::CopyOnWrite { src_pfn } => {
                let new_pfn = self.frames.alloc_phys_page()
                    .map_err(|_| PageFaultError::OutOfMemory)?;
                let src_va = vm_phys_to_virt(self.frames.pfn_to_phys(src_pfn));
                let dst_va = vm_phys_to_virt(self.frames.pfn_to_phys(new_pfn));
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        src_va.as_ptr(), dst_va.as_mut_ptr(), PAGE_SIZE,
                    );
                }
                self.frames.get_mut(src_pfn).unwrap().refcount -= 1;
                region.physblocks[page_idx] = Some(PageSlot {
                    pfn: new_pfn,
                    offset,
                    memtype: Some(ANON_MEMTYPE),
                });
                self.frames.get_mut(new_pfn).unwrap().refcount = 1;
                self.update_pt(vmp, region, page_idx)?;
                Ok(PageFaultResult::Ok)
            }
        }
    }

    /// 更新页表（对应 Minix3 map_ph_writept）
    fn update_pt(
        &self,
        vmp: &mut VmProc,
        region: &VirRegion,
        page_idx: usize,
    ) -> Result<(), PageFaultError> {
        let slot = region.physblocks[page_idx].unwrap();
        let phys = self.frames.pfn_to_phys(slot.pfn);
        let writable = slot.memtype.unwrap().writable(self.frames, &slot);
        let pt_flags = PTF_PRESENT | PTF_USER | if writable { PTF_WRITE } else { PTF_READ };
        pt_writemap(vmp, region.vaddr + slot.offset.get(), phys, PAGE_SIZE, pt_flags)
            .map_err(PageFaultError::PageTable)
    }
}
```

**与 Minix3 的关键差异**

| Minix3 | 方案 A | 说明 |
|--------|-------|------|
| `physblock_get(region, offset)` | `region.physblocks[page_idx]` | O(1) Vec 索引替代链表遍历 |
| `pb_new(MAP_NONE)` + `pb_reference()` | `region.map_lazy(offset)` | 懒映射，无需分配 PhysBlock |
| `ph->memtype->ev_pagefault(vmp, region, ph, ...)` | `memtype.ev_pagefault(vmp, region, frames, offset, ...)` | PageSlot + PageFrames 替代 PhysRegion |
| `pb_unreferenced(region, ph, 0)` + `pb_link(ph, pb, ...)` | `frames.refcount -= 1` + `region.physblocks[idx] = Some(new_slot)` | 直接操作，无侵入式链表 |
| `sys_abscopy(old, new, PAGE_SIZE)` | `copy_nonoverlapping(vm_phys_to_virt(old), vm_phys_to_virt(new), PAGE_SIZE)` | Direct Map 替代内核系统调用 |
| `map_ph_writept(vmp, vr, pr)` | `update_pt(vmp, region, page_idx)` | PageSlot + PageFrames 替代 PhysRegion |

**异步回调处理**

```rust
impl PageFaultHandler {
    /// 异步操作完成后重试页错误处理（对应 Minix3 pf_cont）
    pub fn handle_async_callback(
        &mut self,
        vmp: &mut VmProc,
        info: PageFaultInfo,
    ) {
        match self.handle(vmp, info) {
            Ok(PageFaultResult::Ok) => {
                kernel::vmctl_clear_pagefault(info.endpoint);
            }
            Ok(PageFaultResult::Suspend) => {}
            Err(_) => {
                kernel::sys_kill(info.endpoint, Signal::SIGSEGV);
                kernel::vmctl_clear_pagefault(info.endpoint);
            }
        }
    }
}
```

**消息循环**

```rust
impl VmServer {
    /// 主消息循环
    pub fn run(&mut self) {
        loop {
            let msg = self.receive_message();

            match msg.m_type {
                VM_PAGEFAULT => {
                    let pf_msg = PageFaultMessage {
                        m_type: msg.m_type,
                        m_source: msg.m_source,
                        vpf_addr: msg.vpf_addr,
                        vpf_flags: msg.vpf_flags,
                    };
                    if let Some(vmp) = self.proc_table.get_mut(msg.m_source) {
                        self.pagefault_handler.handle_message(vmp, pf_msg);
                    }
                }

                VM_MEMORY_REQ => {
                    self.handle_memory_request();
                }

                _ => {}
            }
        }
    }
}
```

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

```rust
/// CoW 处理（对应 Minix3 mem_cow）
pub fn handle_cow(
    region: &mut VirRegion,
    frames: &mut PageFrames,
    page_idx: usize,
) -> Result<(), PageFaultError> {
    let slot = region.physblocks[page_idx].unwrap();
    let src_pfn = slot.pfn;
    let src_state = frames.get(src_pfn).unwrap();

    // 检查是否需要 CoW
    if src_state.refcount <= 1 {
        return Ok(());
    }

    // 1. 分配新物理页
    let new_pfn = frames.alloc_phys_page()
        .map_err(|_| PageFaultError::OutOfMemory)?;

    // 2. 复制页面内容（Direct Map，替代 sys_abscopy）
    let src_va = vm_phys_to_virt(frames.pfn_to_phys(src_pfn));
    let dst_va = vm_phys_to_virt(frames.pfn_to_phys(new_pfn));
    unsafe {
        core::ptr::copy_nonoverlapping(src_va.as_ptr(), dst_va.as_mut_ptr(), PAGE_SIZE);
    }

    // 3. 减少原页面引用计数（替代 pb_unreferenced）
    frames.get_mut(src_pfn).unwrap().refcount -= 1;

    // 4. 更新 PageSlot 指向新页面，memtype 切换为匿名（替代 pb_link + ph->memtype = &mem_type_anon）
    region.physblocks[page_idx] = Some(PageSlot {
        pfn: new_pfn,
        offset: slot.offset,
        memtype: Some(ANON_MEMTYPE),
    });

    // 5. 设置新页面引用计数
    frames.get_mut(new_pfn).unwrap().refcount = 1;

    Ok(())
}
```

**与 Minix3 mem_cow 的对应**

| Minix3 步骤 | 方案 A 步骤 | 说明 |
|------------|-----------|------|
| `alloc_mem(1, allocflags)` | `frames.alloc_phys_page()` | 物理页分配 |
| `sys_abscopy(old, new, PAGE_SIZE)` | `copy_nonoverlapping(vm_phys_to_virt(old), vm_phys_to_virt(new), PAGE_SIZE)` | Direct Map 替代内核系统调用 |
| `pb_new(new_page)` | 不需要 | PageFrames 全局数组，无需创建 PhysBlock |
| `pb_unreferenced(region, ph, 0)` | `frames.get_mut(src_pfn).refcount -= 1` | 直接递减 refcount |
| `pb_link(ph, pb, ph->offset, region)` | `region.physblocks[idx] = Some(PageSlot { pfn: new_pfn, .. })` | Vec 索引替代侵入式链表 |
| `ph->memtype = &mem_type_anon` | `memtype: Some(ANON_MEMTYPE)` | CoW 后切换为匿名内存 |

**关键简化**: Minix3 的 `mem_cow` 需要 5 步（分配 → 复制 → 创建 PhysBlock → 解除旧引用 → 链接新引用），方案 A 只需 4 步（分配 → 复制 → 递减旧 refcount → 更新 PageSlot），因为 `PageFrames` 全局数组无需创建/销毁 `PhysBlock` 对象。

**页面复制实现**

> **Direct Map 统一性**: `vm_phys_to_virt()` 使物理页直接可操作。Minix3 中 `sys_abscopy` 是内核系统调用（VM 无法直接访问物理页）；方案 A 中 `vm_phys_to_virt()` 将物理地址转换为虚拟地址，复制变成一行 `copy_nonoverlapping`。这与 [15-cow-mechanism.md](15-cow-mechanism.md) 的 `mem_cow()` 简化是同一个范式转变。

**引用计数管理**

```rust
impl PageFrames {
    /// 分配物理页并返回 PFN
    pub fn alloc_phys_page(&mut self) -> Result<u32, ()> {
        let pfn = self.buddy_alloc.alloc(1)?;
        self.states[pfn as usize] = PageState {
            refcount: 1,
            flags: PageFlags::ALLOCATED,
        };
        Ok(pfn)
    }

    /// 释放物理页（refcount 降为 0 时调用）
    pub fn free_phys_page(&mut self, pfn: u32) {
        self.buddy_alloc.free(pfn, 1);
        self.states[pfn as usize] = PageState {
            refcount: 0,
            flags: PageFlags::empty(),
        };
    }
}
```

**CoW 后页表更新**

```rust
/// CoW 后更新页表（对应 Minix3 map_ph_writept）
pub fn update_pt_after_cow(
    vmp: &mut VmProc,
    region: &VirRegion,
    frames: &PageFrames,
    page_idx: usize,
) -> Result<(), PageTableError> {
    let slot = region.physblocks[page_idx].unwrap();
    let phys = frames.pfn_to_phys(slot.pfn);
    let writable = slot.memtype.unwrap().writable(frames, &slot);
    let pt_flags = PTF_PRESENT | PTF_USER | if writable { PTF_WRITE } else { PTF_READ };
    pt_writemap(vmp, region.vaddr + slot.offset.get(), phys, PAGE_SIZE, pt_flags)
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
    /// CoW 失败次数（alloc_phys_page 失败）
    pub fail_count: u64,
}
```

### 4.4 按需加载路径

按需加载路径处理首次访问未映射页面的情况，分配新页并清零。

**按需加载处理器**

```rust
/// 按需加载处理器（对应 Minix3 map_pf 中 phys == MAP_NONE 分支）
pub struct DemandLoadHandler {
    /// 全局物理页状态表
    frames: &'static mut PageFrames,
}

impl DemandLoadHandler {
    pub fn new(frames: &'static mut PageFrames) -> Self {
        Self { frames }
    }

    /// 处理按需加载
    pub fn handle(
        &mut self,
        region: &mut VirRegion,
        offset: VirBytes,
    ) -> Result<(), PageFaultError> {
        let page_idx = (offset.get() / PAGE_SIZE) as usize;

        if region.physblocks[page_idx].is_some() {
            return Ok(());
        }

        let pfn = self.frames.alloc_phys_page()
            .map_err(|_| PageFaultError::OutOfMemory)?;

        let va = vm_phys_to_virt(self.frames.pfn_to_phys(pfn));
        unsafe { core::ptr::write_bytes(va.as_mut_ptr(), 0, PAGE_SIZE); }

        let memtype = region.def_memtype.unwrap();
        region.map_page(self.frames, offset, pfn, memtype);

        Ok(())
    }
}
```

**分配标志**

```rust
/// 内存分配标志（对应 Minix3 vrallocflags）
bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct VrAllocFlags: u32 {
        const NONE = 0x00;
        const CLEAR = 0x01;
        const ALIGN64K = 0x02;
        const LOWER16MB = 0x04;
        const LOWER1MB = 0x08;
    }
}

impl VirRegion {
    pub fn alloc_flags(&self) -> VrAllocFlags {
        let mut flags = VrAllocFlags::empty();

        if !self.flags.contains(VrFlags::UNINITIALIZED) {
            flags |= VrAllocFlags::CLEAR;
        }
        if self.flags.contains(VrFlags::PHYS64K) {
            flags |= VrAllocFlags::ALIGN64K;
        }
        if self.flags.contains(VrFlags::LOWER16MB) {
            flags |= VrAllocFlags::LOWER16MB;
        }
        if self.flags.contains(VrFlags::LOWER1MB) {
            flags |= VrAllocFlags::LOWER1MB;
        }

        flags
    }
}
```

**按需加载流程**

1. 检查 `PageSlot` 是否存在 → 已存在返回 Ok
2. 不存在 → `frames.alloc_phys_page()` 分配物理页 → 失败返回 OutOfMemory
3. 清零页面内容（Direct Map + `write_bytes`）
4. `region.map_page(frames, offset, pfn, memtype)` 更新 PageSlot 并递增 refcount
5. 返回 Ok

**文件映射按需加载**

```rust
/// 文件映射按需加载处理器（对应 Minix3 mappedfile_pagefault）
pub struct FileDemandLoader;

impl FileDemandLoader {
    /// 处理文件映射按需加载
    pub fn handle(
        frames: &mut PageFrames,
        region: &mut VirRegion,
        offset: VirBytes,
    ) -> Result<PageFaultAction, PageFaultError> {
        let page_idx = (offset.get() / PAGE_SIZE) as usize;

        if region.physblocks[page_idx].is_some() {
            return Ok(PageFaultAction::Done);
        }

        let file_info = region.param.file;
        let referenced_offset = file_info.offset + offset.get();

        Ok(PageFaultAction::Suspend(
            AsyncCallback::new(VfsRequest::Fdio {
                fd: file_info.fdref.fd,
                offset: referenced_offset,
                len: PAGE_SIZE,
            }),
        ))
    }
}
```

> **与 Minix3 的对应**: Minix3 的 `mappedfile_pagefault` 在 `phys == MAP_NONE` 时调用 `vfs_request(VMVFSREQ_FDIO, ...)` 发起异步 I/O，返回 `SUSPEND`。方案 A 将此逻辑封装在 `PageFaultAction::Suspend` 中，VFS 回调完成后由 `pf_cont` 重试页错误处理。文件内容加载到物理页后，`map_page()` 更新 PageSlot 并设置 refcount。

**统计信息**

```rust
/// 按需加载统计
#[derive(Debug, Default)]
pub struct DemandLoadStats {
    pub pages_allocated: u64,
    pub pages_zeroed: u64,
    pub pages_loaded: u64,
    pub load_failures: u64,
    pub bytes_allocated: u64,
}

impl DemandLoadStats {
    pub fn record_alloc(&mut self, zeroed: bool) {
        self.pages_allocated += 1;
        self.bytes_allocated += PAGE_SIZE as u64;
        if zeroed {
            self.pages_zeroed += 1;
        }
    }

    pub fn record_file_load(&mut self) {
        self.pages_loaded += 1;
    }

    pub fn record_failure(&mut self) {
        self.load_failures += 1;
    }
}
```

### 4.5 栈扩展

栈扩展处理栈区域的自动增长，当访问超出当前栈边界时自动扩展。

> **重要**: Minix3 不支持栈自动扩展。访问不在已映射区域内的地址会直接触发 SIGSEGV。栈和数据段的增长由 `do_brk()` 系统调用管理（见 [18-vm-brk.md](18-vm-brk.md)）。minix-rs 可考虑实现自动栈扩展作为改进。

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

### 5.2 锁粒度

细粒度锁减少锁竞争，提高并发性能。

> **注意**: Minix3 VM 是单线程事件驱动模型，不存在并发问题。以下锁设计是 minix-rs 多线程改进的预留设计。

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

### 6.1 CoW 触发测试

- **CoW 触发**: 共享页面（refcount=2）写操作 → 触发 CoW，refcount 降为 1，物理地址改变
- **CoW 不触发（单引用）**: 私有页面（refcount=1）写操作 → 不触发 CoW，物理地址不变
- **CoW 不触发（读操作）**: 共享页面读操作 → 不触发 CoW，refcount 不变
- **CoW 内容复制**: CoW 后新页面内容与原页面一致

### 6.2 非法访问测试

- **空指针访问**: vaddr=0 → InvalidAddress + SIGSEGV
- **写只读区域**: 无 VR_WRITABLE 区域写操作 → PermissionDenied + SIGSEGV
- **越界访问**: 超出进程地址空间 → InvalidAddress
- **栈溢出检测**: 超出最大栈限制 → InvalidAddress

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

- [15-cow-mechanism.md](15-cow-mechanism.md) - CoW 实现（mem_cow 详解）
- [17-vm-fork.md](17-vm-fork.md) - fork 后的首次写入
- [11-region-mapping.md](11-region-mapping.md) - 区域查找与页映射（map_lookup 详解，替代原 vir_region + phys_region）
- [13-region-avl.md](13-region-avl.md) - AVL 树实现（region_search 详解）
- [10-phys-pagestate.md](10-phys-pagestate.md) - 物理页状态管理（PageState.refcount，替代原 pb_new/pb_link/pb_unreferenced）
- [12-memtype.md](12-memtype.md) - 内存类型（mem_type 及 ev_pagefault 分派）
- [05-vm-allocpage.md](05-vm-allocpage.md) - 物理内存分配（alloc_mem/SPAREPAGES）
- [18-vm-brk.md](18-vm-brk.md) - 栈和数据段增长（do_brk）

---

*分类: VM私有*
