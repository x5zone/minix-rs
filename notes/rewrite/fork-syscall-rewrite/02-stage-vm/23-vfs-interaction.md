# 23-vfs-interaction: VM 与 VFS 的异步对话

> **分类**: VM服务
> **源码**: `minix3/minix/servers/vm/vfs.c`, `fdref.c`, `mem_file.c`, `mmap.c`
> **说明**: VM 如何与 VFS 异步通信实现文件映射、页换入、fd 引用计数
> **代码版本**: 基于当前 `os/servers/vm/src/` 代码（2025-05-25 review 后同步）

---

## 1. 概述

### 1.1 为什么 VM 需要和 VFS 对话

VM 管理内存，VFS 管理文件。当两者交汇时——**文件映射（mmap）**——VM 必须向 VFS 请求文件数据：

| 场景 | VM 的需求 | VFS 的角色 |
|------|---------|-----------|
| 文件 mmap | 需要文件的 fd 信息（dev, ino, 大小） | VFS 查找 fd → 返回元数据 |
| 文件缺页 | 需要从文件读取一页数据 | VFS 执行磁盘 I/O → 返回数据 |
| 关闭 fd | 最后一个引用消失时关闭 fd | VFS 关闭文件描述符 |
| exec 加载 | 需要读取可执行文件的段 | VFS 读取 ELF 段到内存 |

**核心挑战**：VM 和 VFS 是两个独立的服务器进程，通信通过 IPC 完成。但 VFS 的磁盘 I/O 是**异步**的——VM 发出请求后不能阻塞等待，必须**挂起当前缺页处理**，等 VFS 回复后再继续。

### 1.2 异步通信模型

```
用户进程 A → 缺页 → VM
                    │
                    ├── 查找区域 → 文件映射
                    ├── 检查缓存 → 未命中
                    ├── vfs_request(VMVFSREQ_FDIO, ...) → 发给 VFS
                    │   返回 SUSPEND（不回复用户进程）
                    │
                    ▼  VFS 处理中... VM 可以处理其他请求
                    │
                    ▼  VFS 回复 VM_VFS_REPLY
                    │
                    ├── do_vfs_reply() → 调用回调
                    ├── 回调：mappedfile_pagefault_cont()
                    │   ├── 将 VFS 返回的数据映射到物理页
                    │   ├── 更新页表
                    │   └── 回复用户进程 A（缺页已处理）
```

**关键设计**：VM 使用**请求队列 + 回调**模型，而不是同步等待。这允许 VM 在等待 VFS 时处理其他进程的请求。

### 1.3 与其他文档的关系

| 文档 | 关系 |
|------|------|
| 19-cow-exec-pagefault | exec 缺页也需要 VFS 交互 |
| 19-vm-munmap | munmap 文件映射区域时调用 fdref_deref → vfs_request(FDCLOSE) |
| 10-phys-pagestate | PhysBlock 引用计数与 `pb_unreferenced()` 完整源码分析（§2.5），文件映射中 ev_unreference 的特殊处理 |
| 12-vir-region | VrParam::File 存储文件映射参数 |

---

## 2. C 源码分析

### 2.1 vfs_request — 异步请求框架

```c
/* vfs.c */
struct vfs_request_node {
    message         reqmsg;      /* 发给 VFS 的消息 */
    char            reqstate[70];/* 保存的缺页处理状态 */
    void           *opaque;      /* 回调参数 */
    endpoint_t      who;         /* 请求进程 */
    int             req_id;      /* 请求 ID */
    vfs_callback_t  callback;    /* 回调函数 */
    struct vfs_request_node *next;
};

static struct vfs_request_node *first_queued, *active;

#define ID_MAX LONG_MAX   /* req_id 上限 */
```

**请求队列结构**：

```
first_queued → [req3] → [req2] → NULL
active → [req1] (已发送给 VFS，等待回复)
```

```c
int vfs_request(int reqno, int fd, struct vmproc *vmp,
    u64_t offset, u32_t len,
    vfs_callback_t reply_callback, void *cbarg,
    void *state, int statelen)
{
    static int reqid = 0;
    struct vfs_request_node *reqnode;

    reqid++;

    /* 1. 分配请求节点 */
    if(!SLABALLOC(reqnode)) {
        printf("vfs_request: no memory for request node\n");
        return ENOMEM;
    }

    /* 2. 填充消息 */
    message *m = &reqnode->reqmsg;
    memset(m, 0, sizeof(*m));
    m->m_type = VFS_VMCALL;
    m->VFS_VMCALL_REQ = reqno;        /* 请求类型 */
    m->VFS_VMCALL_FD = fd;            /* 文件描述符 */
    m->VFS_VMCALL_REQID = reqid;      /* 请求 ID */
    m->VFS_VMCALL_ENDPOINT = vmp->vm_endpoint;
    m->VFS_VMCALL_OFFSET = offset;    /* 文件偏移 */
    m->VFS_VMCALL_LENGTH = len;       /* 读取长度 */

    /* 3. 保存回调和状态 */
    reqnode->who = vmp->vm_endpoint;
    reqnode->req_id = reqid;
    reqnode->callback = reply_callback;
    reqnode->opaque = cbarg;
    if(state) memcpy(reqnode->reqstate, state, statelen);

    /* 4. 加入队列头部 */
    reqnode->next = first_queued;
    first_queued = reqnode;

    /* 5. 如果没有活跃请求，立即发送 */
    if(!active) activate();

    return OK;
}
```

**activate() — 发送请求**：

```c
static void activate(void)
{
    assert(!active);
    assert(first_queued);

    active = first_queued;
    first_queued = first_queued->next;

    /* 异步发送，不等待回复 */
    if(asynsend3(VFS_PROC_NR, &active->reqmsg, AMF_NOREPLY) != OK)
        panic("VM: asynsend to VFS failed");
}
```

**关键设计**：
- **串行化**：同一时间只有一个活跃请求（`active`），VFS 不需要处理并发
- **队列**：多个请求排队等待（`first_queued` 链表）
- **异步**：`asynsend3` 不阻塞，VM 继续处理其他消息
- **回调**：VFS 回复时调用注册的 `callback` 函数

### 2.2 do_vfs_reply — 处理 VFS 回复

```c
int do_vfs_reply(message *m)
{
    struct vfs_request_node *orignode = active;
    vfs_callback_t req_callback;
    void *cbarg;
    int n;
    struct vmproc *vmp;

    assert(active);
    assert(active->req_id == m->VMV_REQID);

    /* 进程可能已退出 */
    if(vm_isokendpt(m->VMV_ENDPOINT, &n) != OK)
        vmp = NULL;
    else vmp = &vmproc[n];

    /* 取出回调 */
    req_callback = active->callback;
    cbarg = active->opaque;
    active = NULL;

    /* 调用回调 */
    if(req_callback)
        req_callback(vmp, m, cbarg, orignode->reqstate);

    SLABFREE(orignode);

    /* 发送下一个排队的请求 */
    if(first_queued && !active)
        activate();

    return SUSPEND;  /* 不回复 VFS 的回复 */
}
```

**SUSPEND 的含义**：`do_vfs_reply` 返回 SUSPEND，告诉 VM 主循环**不要回复 VFS**——VFS 的回复不需要再回复。

### 2.3 VFS 请求类型

| 请求类型 | 用途 | 回调 |
|---------|------|------|
| `VMVFSREQ_FDLOOKUP` | mmap 文件时查找 fd 元数据 | `mmap_file_cont()` |
| `VMVFSREQ_FDIO` | 文件缺页时读取一页数据 | 缺页回调 |
| `VMVFSREQ_FDCLOSE` | 关闭不再需要的 fd | NULL（不需要回调） |

### 2.4 fdref — 文件描述符引用计数

**问题**：多个 VirRegion 可能引用同一个文件（通过 split 或 fork），但每个进程只有有限的 fd。如何知道何时可以安全关闭 fd？

**解决方案**：`fdref` 结构跟踪每个文件映射区域的 fd 引用：

```c
/* fdref.h */
struct fdref {
    int         fd;        /* 文件描述符 */
    int         refcount;  /* 引用计数 */
    dev_t       dev;       /* 设备号 */
    ino_t       ino;       /* inode 号 */
    struct fdref *next;    /* 链表 */
    int         counting;  /* 调试用 */
};
```

**fdref 的生命周期**：

```
1. mmap 文件 → fdref_new(owner, ino, dev, fd) → 创建 fdref (refcount=0)，插入全局链表 fdrefs 头部
2. 绑定到区域 → fdref_ref(fdref, region) → refcount++
3. split 区域 → fdref_ref(fdref, r1) + fdref_ref(fdref, r2) → refcount++ ×2
4. munmap 区域 → fdref_deref(region) → refcount--
5. refcount == 0 → 从全局链表 fdrefs 移除（遍历链表找到前驱，修改前驱的 next 指针）+ SLABFREE 释放内存 + vfs_request(FDCLOSE) 异步关闭 fd
```

**fdref_deref 链表移除步骤**（`fdref.c:116-155`）：
1. `ref = region->param.file.fdref`，`region->param.file.fdref = NULL`
2. `ref->refcount--`，如果 `refcount > 0` 直接返回
3. 如果 `fdrefs == ref`（头节点），`fdrefs = ref->next`
4. 否则遍历链表 `for(r = fdrefs; r->next != ref; r = r->next)` 找到前驱，`r->next = ref->next`
5. `SLABFREE(ref)` 释放内存
6. `vfs_request(VMVFSREQ_FDCLOSE, fd, ...)` 异步关闭 fd

**fdref_dedup_or_new — 去重**：

```c
struct fdref *fdref_dedup_or_new(struct vmproc *owner,
    ino_t ino, dev_t dev, int fd, int mayclose)
{
    struct fdref *fr;

    for(fr = fdrefs; fr; fr = fr->next) {
        if(ino == fr->ino && dev == fr->dev) {
            if(fd == fr->fd) {
                return fr;   /* 完全匹配，复用 */
            }
            if(!mayclose) continue;
            /* 同文件不同 fd，关闭新 fd，复用旧 fdref */
            vfs_request(VMVFSREQ_FDCLOSE, fd, owner, ...);
            return fr;
        }
    }

    return fdref_new(owner, ino, dev, fd);
}
```

**去重的意义**：如果进程多次 mmap 同一个文件，不需要为每次 mmap 打开新 fd。复用已有的 fdref 节省 fd 资源。

### 2.5 mem_type_mappedfile — 文件映射的 memtype

```c
struct mem_type mem_type_mappedfile = {
    .name = "file-mapped memory",
    .ev_new = NULL,                             /* 无特殊初始化 */
    .ev_delete = mappedfile_delete,             /* 释放 fdref */
    .ev_reference = NULL,                       /* 不支持共享引用（文件页独立管理） */
    .ev_unreference = mappedfile_unreference,   /* 释放物理页 */
    .ev_pagefault = mappedfile_pagefault,       /* 异步读取文件页 */
    .ev_resize = NULL,                          /* 不支持 resize */
    .ev_split = mappedfile_split,               /* split 时调整 offset */
    .writable = mappedfile_writable,            /* 永远返回 0 */
    .ev_sanitycheck = mappedfile_sanitycheck,
    .ev_copy = mappedfile_copy,                 /* fork 时复制 */
    .ev_lowshrink = mappedfile_lowshrink,       /* 头部取消时调整 offset */
    .regionid = NULL,                           /* 无特殊 region ID */
    .refcount = NULL,                           /* 无特殊引用计数 */
    .pt_flags = mappedfile_pt_flags,            /* ARM: ARM_VM_PTE_CACHED; 其他: 0 */
};
```

**关键特性**：
- `writable` 永远返回 0 → 文件映射页初始只读，写入时触发 CoW
- `ev_pagefault` 可能返回 SUSPEND → 异步读取文件数据
- `ev_delete` 调用 `fdref_deref` → 最后一个引用消失时关闭 fd
- `pt_flags` 在 ARM 架构返回 `ARM_VM_PTE_CACHED`（缓存属性），其他架构返回 0
- `ev_reference` 为 NULL → 文件映射页不支持共享引用，每个物理页独立管理
- `ev_resize` 为 NULL → 文件映射区域不支持动态 resize

### 2.6 mappedfile_pagefault — 文件缺页处理

```c
static int mappedfile_pagefault(struct vmproc *vmp,
    struct vir_region *region, struct phys_region *ph,
    int write, vfs_callback_t cb, void *state, int statelen, int *io)
{
    int procfd = region->param.file.fdref->fd;

    /* 情况 1: 全新页（phys == MAP_NONE） */
    if(ph->ph->phys == MAP_NONE) {
        struct cached_page *cp;
        u64_t referenced_offset = region->param.file.offset + ph->offset;

        /* 1a. 先查 VM 页缓存 */
        /* 设备文件（如 frame buffer）没有 inode 号，走 bydev 查找；
         * 普通文件走 byino 查找 */
        if(region->param.file.fdref->ino == VMC_NO_INODE) {
            cp = find_cached_page_bydev(
                region->param.file.fdref->dev,
                referenced_offset, VMC_NO_INODE, 0, 1);
        } else {
            cp = find_cached_page_byino(
                region->param.file.fdref->dev,
                region->param.file.fdref->ino,
                referenced_offset, 1);
        }

        if(cp && (!cb || !(cp->flags & VMSF_ONCE))) {
            /* 缓存命中！直接使用缓存的物理页 */
            /* VMSF_ONCE 页的特殊处理：
             * - 如果有回调(cb!=NULL)且页标记为一次性使用(VMSF_ONCE)，
             *   仍走 VFS 读取路径，让文件系统更新页内容
             * - 无回调时直接使用缓存（无法异步，只能用缓存）
             * - 映射后一次性页从缓存移除（rmcache），不做长期缓存 */
            pb_unreferenced(region, ph, 0);
            pb_link(ph, cp->page, ph->offset, region);

            /* 尾部页需要 CoW（清零 clearend） */
            if(roundup(ph->offset + region->param.file.clearend,
                VM_PAGE_SIZE) >= region->length) {
                cow_block(vmp, region, ph, region->param.file.clearend);
            } else if(write) {
                cow_block(vmp, region, ph, 0);
            }

            /* 一次性使用页映射后立即从缓存移除 */
            if(result == OK && (cp->flags & VMSF_ONCE))
                rmcache(cp);

            return OK;
        }

        /* 1b. 缓存未命中，需要从 VFS 读取 */
        if(!cb) return EFAULT;  /* 无回调，无法异步 */

        vfs_request(VMVFSREQ_FDIO, procfd, vmp,
            referenced_offset, VM_PAGE_SIZE, cb, NULL, state, statelen);
        *io = 1;
        return SUSPEND;  /* 挂起，等 VFS 回复 */
    }

    /* 情况 2: 已有物理页，写入触发 CoW */
    if(!write) return OK;
    return cow_block(vmp, region, ph, 0);
}
```

**文件缺页的完整流程**：

```
用户写入文件映射页 → #PF
  │
  ▼
mappedfile_pagefault():
  ├── phys == MAP_NONE?
  │    ├── 查缓存 → 命中 → pb_link + 可能 CoW → OK
  │    └── 未命中 → vfs_request(FDIO) → SUSPEND
  │         │
  │         ▼  VFS 读取文件数据
  │         │
  │         ▼  VM_VFS_REPLY 到达
  │         │
  │         do_vfs_reply() → 回调:
  │           ├── 分配物理页
  │           ├── 复制 VFS 返回的数据
  │           ├── 更新页表
  │           └── 回复用户进程
  │
  └── phys != MAP_NONE + write?
       └── cow_block() → CoW 复制 → 切换为匿名内存
```

### 2.7 cow_block — 文件页写入时 CoW

```c
static int cow_block(struct vmproc *vmp, struct vir_region *region,
    struct phys_region *ph, u16_t clearend)
{
    /* 1. CoW 复制：分配新物理页，复制数据 */
    mem_cow(region, ph, MAP_NONE, MAP_NONE);

    /* 2. 切换为匿名内存！ */
    ph->memtype = &mem_type_anon;

    /* 3. 如果是尾部页，清零 clearend 部分 */
    if(clearend) {
        phys_bytes phaddr = ph->ph->phys + (VM_PAGE_SIZE - clearend);
        sys_memset(NONE, 0, phaddr, clearend);
    }

    return OK;
}
```

**关键洞察**：文件映射页写入时，CoW 后**切换为匿名内存**（`ph->memtype = &mem_type_anon`）。这意味着：
- 写入后的页不再与文件关联
- 后续缺页由 `anon_pagefault` 处理，不再需要 VFS
- 这实现了 POSIX 的 MAP_PRIVATE 语义：写入不修改文件

### 2.8 mappedfile_setfile — 初始化文件映射

```c
int mappedfile_setfile(struct vmproc *owner,
    struct vir_region *region, int fd, u64_t offset,
    dev_t dev, ino_t ino, u16_t clearend, int prefill, int mayclosefd)
{
    struct fdref *newref;

    /* 1. 创建或复用 fdref */
    newref = fdref_dedup_or_new(owner, ino, dev, fd, mayclosefd);
    fdref_ref(newref, region);   /* refcount++ */

    /* 2. 设置文件参数 */
    region->param.file.offset = offset;
    region->param.file.clearend = clearend;
    region->param.file.inited = 1;

    /* 3. 预填充：从缓存加载已有页 */
    if(!prefill) return OK;

    for(vaddr = 0; vaddr < region->length; vaddr += VM_PAGE_SIZE) {
        u64_t referenced_offset = offset + vaddr;

        /* 尾部页不预填充 */
        if(roundup(vaddr + clearend, VM_PAGE_SIZE) >= region->length)
            break;

        /* 查缓存 */
        cp = find_cached_page_byino(dev, ino, referenced_offset, 1);
        if(!cp || (cp->flags & VMSF_ONCE)) continue;

        /* 缓存命中：直接引用 */
        pr = pb_reference(cp->page, vaddr, region, &mem_type_mappedfile);
        map_ph_writept(region->parent, region, pr);
    }

    return OK;
}
```

**sanitycheck 函数**：`mappedfile_sanitycheck` 和 `fdref_sanitycheck` 是调试辅助函数，仅在启用 sanitycheck 时运行。前者验证物理页的使用计数一致性，后者遍历全局 fdref 链表检查：同一 fd 不应出现两次、同一 dev+ino 不应重复、每个 fdref 的 refcount 应与实际引用它的区域数一致。

### 2.9 mappedfile_split / lowshrink / delete

```c
/* split: 两个子区域都引用同一个 fdref */
static void mappedfile_split(struct vmproc *vmp,
    struct vir_region *vr, struct vir_region *r1, struct vir_region *r2)
{
    r1->param.file = vr->param.file;
    r2->param.file = vr->param.file;

    fdref_ref(vr->param.file.fdref, r1);   /* refcount++ */
    fdref_ref(vr->param.file.fdref, r2);   /* refcount++ */

    r1->param.file.clearend = 0;
    r2->param.file.offset += r1->length;   /* 右半部分 offset 前移 */
}

/* lowshrink: 头部取消时 offset 前移 */
static int mappedfile_lowshrink(struct vir_region *vr, vir_bytes len)
{
    vr->param.file.offset += len;
    return OK;
}

/* delete: 释放 fdref 引用 */
static void mappedfile_delete(struct vir_region *region)
{
    fdref_deref(region);   /* refcount--, 可能关闭 fd */
    region->param.file.inited = 0;
}
```

### 2.10 do_mmap — mmap 系统调用处理

```c
int do_mmap(message *m)
{
    vir_bytes addr = m->m_mmap.addr;
    size_t len = m->m_mmap.len;

    if(m->m_mmap.fd == -1 || (m->m_mmap.flags & MAP_ANON)) {
        /* 匿名映射：fd != -1 且 MAP_ANON 是非法组合 */
        if(m->m_mmap.fd != -1) return EINVAL;

        /* 连续物理内存需要预分配 */
        if((m->m_mmap.flags & (MAP_CONTIG|MAP_PREALLOC)) == MAP_CONTIG)
            return EINVAL;

        mt = (m->m_mmap.flags & MAP_CONTIG)
            ? &mem_type_anon_contig : &mem_type_anon;
        vr = mmap_region(vmp, addr, flags, len,
            VR_WRITABLE | VR_ANON, mt, execpriv);
    } else {
        /* 文件映射可能被禁用 */
        if(!enable_filemap) return ENXIO;

        /* 不支持可写的 MAP_SHARED 文件映射 */
        if((m->m_mmap.flags & MAP_SHARED)
            && (m->m_mmap.prot & PROT_WRITE))
            return ENXIO;

        /* 文件映射：先向 VFS 查询 fd 信息 */
        vfs_request(VMVFSREQ_FDLOOKUP, m->m_mmap.fd, vmp, 0, 0,
            mmap_file_cont, NULL, m, sizeof(*m));
        return SUSPEND;  /* 异步等待 VFS 回复 */
    }

    m->m_mmap.retaddr = (void *) vr->vaddr;
    return OK;
}
```

**mmap_file_cont — VFS 回复后的回调**：

```c
static void mmap_file_cont(struct vmproc *vmp, message *replymsg,
    void *cbarg, void *origmsg_v)
{
    message *origmsg = origmsg_v;

    if(replymsg->VMV_RESULT != OK) {
        result = replymsg->VMV_RESULT;
    } else {
        /* VFS 返回了 fd 的 dev, ino, size 信息 */
        result = mmap_file(vmp, replymsg->VMV_FD,
            origmsg->m_mmap.offset, origmsg->m_mmap.flags,
            replymsg->VMV_INO, replymsg->VMV_DEV,
            replymsg->VMV_SIZE_PAGES * PAGE_SIZE,
            origmsg->m_mmap.addr, origmsg->m_mmap.len,
            &v, 0, writable, 1);
    }

    /* 回复用户进程 */
    mmap_reply.m_type = result;
    mmap_reply.m_mmap.retaddr = (void *) v;
    ipc_send(vmp->vm_endpoint, &mmap_reply);
}
```

### 2.10.1 do_vfs_mmap — VFS 主动映射（同步路径）

除了用户进程通过 `do_mmap` 发起的异步文件映射，VFS 自身也可以主动请求 VM 为进程创建文件映射。这是一个**同步**路径——VFS 已拥有文件元数据，无需再向自己查询：

```c
int do_vfs_mmap(message *m)
{
    if(!enable_filemap) return ENXIO;

    /* VFS 直接提供 fd, offset, dev, ino, vaddr, len */
    return mmap_file(vmp, m->m_vm_vfs_mmap.fd,
        m->m_vm_vfs_mmap.offset,
        MAP_PRIVATE | MAP_FIXED,    /* 强制 MAP_PRIVATE */
        m->m_vm_vfs_mmap.ino, m->m_vm_vfs_mmap.dev,
        (u64_t) LONG_MAX * VM_PAGE_SIZE,
        m->m_vm_vfs_mmap.vaddr, m->m_vm_vfs_mmap.len, &v,
        clearend, flags, 0);        /* mayclosefd=0 */
}
```

**与 do_mmap 的对比**：

| 方面 | do_mmap（用户发起） | do_vfs_mmap（VFS 发起） |
|------|-------------------|----------------------|
| 触发者 | 用户进程 mmap() 系统调用 | VFS 内部请求 |
| VFS 查询 | 需要 FDLOOKUP 异步查询 | 不需要（VFS 已有元数据） |
| 映射标志 | 用户指定 | 强制 MAP_PRIVATE \| MAP_FIXED |
| mayclosefd | 1（可关闭多余 fd） | 0（VFS 管理的 fd 不自动关闭） |
| 返回方式 | SUSPEND → 回调后回复 | 同步返回 |

---

## 3. Rust 设计决策

> 本章解释"为什么这样设计"。每个决策都追溯至 Ch1&2 的 C 源码分析，并对比多种可行方案。

### 3.1 异步回调：函数指针 + 枚举状态

**问题**（源自 §2.1 `vfs_request_node`）：Minix3 用函数指针 `vfs_callback_t` + `void *opaque` + `char reqstate[70]` 保存回调上下文。Rust 如何安全地表达这个模型？

**方案对比**：

| 方案 | 回调表达 | 状态保存 | 堆分配 | no_std | 可调试性 |
|------|---------|---------|--------|--------|---------|
| A: `Box<dyn FnOnce>` | 闭包捕获 | 闭包内部 | 每次请求 | 需 alloc | 差（不透明） |
| B: 函数指针 + 枚举状态 | `fn(&mut VmServer, &VfsReply, &State)` | `VfsRequestState` 枚举 | 零 | 不需要 | 好（可打印） |
| C: async/await | `async fn` | 编译器生成状态机 | 取决于执行器 | 需 alloc | 中等 |

**选择方案 B**，理由：

1. **VFS 回调种类固定**（§2.3 只有 3 种：FDLOOKUP/FDIO/FDCLOSE），枚举完全覆盖，不需要闭包的灵活性
2. **零堆分配**：`VfsRequestState` 是枚举，大小编译时已知，直接内嵌在 `VfsRequest` 中，无需 `Box`
3. **可调试**：枚举可以 `Debug` 打印，闭包不透明
4. **no_std 友好**：不依赖 `alloc::boxed::Box`（虽然 VM 可用 alloc，但减少分配是好事）
5. **与 Minix3 对应清晰**：`char reqstate[70]` → `VfsRequestState`，`vfs_callback_t` → `fn(...)`

方案 A 的问题：`Box<dyn FnOnce>` 每次请求堆分配一个闭包对象，闭包捕获状态大小不确定，且不透明无法调试。方案 C 的问题：需要 async 运行时，与 VM 的单线程事件循环模型不匹配。

**C 源码依据**：§2.1 的 `vfs_request_node` 结构中 `callback` 是函数指针，`reqstate` 是固定大小缓冲区。方案 B 是这个设计的类型安全重写。

### 3.2 FdRef：显式引用计数 + FdRefTable

**问题**（源自 §2.4 `fdref`）：Minix3 用全局链表 `fdrefs` + 手动 `refcount` 管理文件描述符引用。Rust 如何表达？

**方案对比**：

| 方案 | 引用管理 | Drop 行为 | 访问 VfsQueue | 单线程安全 | 与 Minix3 对齐 |
|------|---------|----------|-------------|-----------|--------------|
| A: `Arc<FdRefInner>` + Drop | 自动 | 最后引用消失时触发 Drop | ❌ Drop 无法访问 VfsQueue | ✅ | 部分 |
| B: `Rc<FdRefInner>` + Drop | 自动（无原子开销） | 同上 | ❌ 同上 | ✅ | 部分 |
| C: 显式 refcount + FdRefTable | 手动 | 无隐式 Drop | ✅ deref 返回 PendingFdClose | ✅ | 完全 |

**选择方案 C**，理由：

1. **关键限制**：`fdref_deref` 在 `refcount==0` 时需要发送 `vfs_request(FDCLOSE)`（§2.4），但 Rust 的 `Drop::drop` 只接收 `&mut self`，无法访问 `VfsRequestQueue`。Arc/Rc 的 Drop 都无法解决这个问题
2. **Minix3 的 fdref 本身就是全局表 + 手动引用计数**：这不是 C 语言限制，而是设计需要——fd 的关闭必须通过 VFS 异步请求，不能在 Drop 中隐式触发
3. **FdRefTable 模式已有先例**：与 `VmProcTable` 的设计模式一致——全局表 + 索引访问 + 显式生命周期管理
4. **`VrParam::File` 中存 `fdref_id: Option<u32>`**（而非 `FdRef` 本身）：与 Minix3 的 `param.file.fdref` 指针语义对应——指向全局表中的条目

**C 源码依据**：§2.4 的 `fdref_deref()` 在 `refcount==0` 时执行"从全局链表移除 + SLABFREE + vfs_request(FDCLOSE)"。这个"条件触发异步关闭"逻辑用 Drop 无法安全实现。

### 3.3 VfsRequestQueue：串行激活模型

**问题**（源自 §2.1 `first_queued` + `active`）：Minix3 的 VFS 请求是严格串行的——同一时间只有一个 active 请求。为什么？Rust 如何表达？

**串行化的原因**（不是 C 语言限制，是 VFS 协议约束）：

1. **VFS 的限制**：VFS 可能无法处理并发的 VM 请求
2. **状态一致性**：回调函数依赖请求发出时的状态，并发请求可能导致状态混乱
3. **简化设计**：串行化消除了竞态条件

**设计**：`active: Option<VfsRequest>` + `queued: VecDeque<VfsRequest>`

- `request()` 将请求加入 `queued`，如果 `active == None` 则调用 `activate()`
- `activate()` 从 `queued` 取出第一个请求设为 `active`，通过 IPC 发送给 VFS
- `handle_reply()` 取出 `active`，返回回调函数+状态供调用方执行，然后 `activate()` 下一个

**与当前代码的差异**：`VfsRequestQueue` 现在有 `active: Option<VfsRequest>` + `queued: VecDeque<VfsRequest>`，`handle_reply` 通过 `active.req_id` 匹配，符合串行语义。

**C 源码依据**：§2.1 的 `static struct vfs_request_node *first_queued, *active;` 和 `do_vfs_reply` 中 `orignode = active` 的匹配逻辑。

### 3.4 PagefaultResult 扩展：NeedVfsIo + PagefaultAction::Suspended

**问题**（源自 §2.6 `mappedfile_pagefault` 返回 SUSPEND）：Minix3 的 `SUSPEND` 表示"缺页挂起，等 VFS 回复后再处理"。Rust 如何表达？

**设计**：

```rust
pub(crate) enum PagefaultResult {
    Handled,
    NeedNewPage,
    NeedCow,
    NeedVfsIo,         // 对应 Minix3 的 SUSPEND
    AccessViolation,
}

pub(crate) enum PagefaultAction {
    Handled,
    MappedNewPage,
    CowResolved,
    Suspended,         // 新增：缺页挂起，不回复用户进程
    AccessViolation,
}
```

**`NeedVfsIo` vs `Suspended` 的区别**：

- `NeedVfsIo`：`MemType::ev_pagefault` 的返回值，告诉缺页处理框架"这个缺页需要 VFS I/O"
- `Suspended`：`PagefaultAction` 的变体，告诉 VM 主循环"不要回复用户进程，等 VFS 回复后再处理"

缺页处理框架将 `NeedVfsIo` 转换为 `Suspended`，同时构造 `VfsRequest` 加入队列。

**C 源码依据**：§2.6 的 `mappedfile_pagefault` 返回 `SUSPEND`，§2.1 的 `vfs_request` 调用后返回 `SUSPEND`。

### 3.5 VrParam::File 的 fdref_id 设计

**问题**（源自 §2.4 `param.file.fdref`）：Minix3 的 `vir_region.param.file.fdref` 是指向 `fdref` 结构的指针。Rust 中如何表达？

**设计**：`fdref_id: Option<u32>`——FdRefTable 中的索引

```rust
pub(crate) enum VrParam {
    Direct { phys: PhysBytes },
    Shared { ep: i32, vaddr: VirBytes, id: i32 },
    PbCache { pfn: u32 },
    File {
        inited: bool,
        fdref_id: Option<u32>,   // FdRefTable 中的索引
        offset: u64,
        clearend: u16,
    },
}
```

**为什么不用 `Rc<FdRef>` 或 `Arc<FdRef>`**：

1. `VrParam` 需要 `Clone`（split 时复制），`Rc`/`Arc` 的 clone 是浅拷贝（共享引用），语义正确
2. 但 `Rc`/`Arc` 的 Drop 会自动减少引用计数，而我们需要在 `refcount==0` 时显式发送 `FDCLOSE`（§3.2 已分析）
3. 用 `fdref_id` 间接引用，`FdRefTable` 集中管理所有 fdref，`fdref_ref(id)` / `fdref_deref(id)` 显式操作

**C 源码依据**：§2.4 的 `region->param.file.fdref` 是指针，指向全局链表中的 `fdref` 节点。`fdref_id` 是指针的 Rust 安全替代。

### 3.6 页缓存：CacheKey 枚举 + BTreeMap

**问题**（源自 §2.6 `find_cached_page_byino` / `find_cached_page_bydev`）：Minix3 有两种缓存查找路径——按 inode（普通文件）和按 dev（设备文件）。Rust 如何统一表达？

**设计**：

```rust
#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum CacheKey {
    ByInode { dev: u64, ino: u64, offset: u64 },
    ByDevice { dev: u64, offset: u64 },
}
```

用 `BTreeMap<CacheKey, PageCacheEntry>` 而非 `HashMap`——`BTreeMap` 在 `alloc` 中直接可用，不需要 `hashbrown` crate。

**C 源码依据**：§2.6 的 `VMC_NO_INODE` 分支——当 `ino == VMC_NO_INODE` 时走 `find_cached_page_bydev`，否则走 `find_cached_page_byino`。`CacheKey` 枚举将这个条件分支编码到类型系统中。

### 3.7 cow_block 后的 memtype 切换——已有机制

**问题**（源自 §2.6 `cow_block`）：Minix3 在写入文件映射页时执行 `ph->memtype = &mem_type_anon` 切换为匿名内存。Rust 是否需要额外设计？

**答案：不需要**。当前代码已通过 `map_page(new_pfn, &MEM_TYPE_ANON)` 实现了 memtype 切换：

```rust
// cow_exec_pf.rs: cow_resolve_core()
let pending = region.unmap_page(frames, offset);
region.map_page(frames, offset, new_pfn, &MEM_TYPE_ANON);
```

`map_page` 的第四个参数是新页的 memtype，传入 `&MEM_TYPE_ANON` 即完成类型切换。这比 Minix3 的 `ph->memtype = &mem_type_anon` 更优雅——不需要在 PhysRegion 上加 memtype 字段，因为 `PageSlot` 已经有了 per-page memtype。

**C 源码依据**：§2.6 的 `cow_block()` → `mem_cow()` → `ph->memtype = &mem_type_anon`。Rust 版本通过 `map_page` 的 memtype 参数实现等价语义。

### 3.8 测试策略：IPC mock 模式

**核心洞察**：VM 与 VFS 的交互是通过 IPC 完成的。这意味着 VFS 依赖不影响 VM 的开发——只需要 mock IPC 层。

**为什么之前的 TODO 是思维误区**：

很多人以为"VFS 未就绪就不能开发文件映射功能"。实际上，IPC 边界天然提供了隔离——VM 只需要发送消息和接收回复，不需要知道 VFS 内部如何工作。单元测试中：

1. **不需要真正的 VFS 进程**：mock 一个 `IpcSender` trait，记录发送的消息
2. **不需要多线程**：VM 是单线程事件循环，测试中手动构造 `VfsReply` 调用 `handle_reply`
3. **不需要集成测试**：那是未来的事，验证 VM 和 VFS 的端到端交互

**IpcSender trait 设计**：

```rust
pub(crate) trait IpcSender {
    fn async_send(&self, dest: Endpoint, msg: &VfsCallMessage) -> Result<(), IpcError>;
}
```

生产实现对接真正的 Minix3 IPC（`ipc_asynsend`），测试实现 `MockIpcSender` 记录消息。`VfsRequestQueue` 依赖 `IpcSender` 而非直接调用 IPC 系统调用，使得整个 VFS 交互逻辑可以在单元测试中验证。

**单元测试模式**：

```
1. 创建 VmServer，注入 MockIpcSender
2. 触发缺页 → MappedFile::ev_pagefault 返回 NeedVfsIo
3. 缺页框架构造 VfsRequest(FdIo) → VfsRequestQueue.request()
4. MockIpcSender 记录发送的消息 → 验证消息内容
5. 手动构造 VfsReply(result=OK, data=...) → handle_reply()
6. 验证回调逻辑：物理页分配、页表更新、用户进程回复
```

**集成测试**（未来）：多进程 IPC，启动真正的 VFS 服务器。不在当前范围。

---

## 4. Rust 实现详解

> 本章聚焦"怎么做"。每个实现都对应 §3 的设计决策，代码与设计严格一致。

### 4.1 VfsRequestType — 对齐 Minix3 命名

> 设计决策：§3.1（函数指针 + 枚举状态）

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VfsRequestType {
    FdLookup,   // VMVFSREQ_FDLOOKUP: mmap 时查询 fd 元数据
    FdIo,       // VMVFSREQ_FDIO: 文件缺页时读取一页数据
    FdClose,    // VMVFSREQ_FDCLOSE: 关闭不再需要的 fd
}
```

**与当前代码的差异**：`VfsRequestType` 已改为 `FdLookup/FdIo/FdClose`，与 Minix3 的三种请求类型对齐。

### 4.2 VfsRequestState — 回调状态枚举

> 设计决策：§3.1（函数指针 + 枚举状态替代闭包）

```rust
pub(crate) enum VfsRequestState {
    FdLookup {
        orig_msg: VmMmapIn,
    },
    FdIo {
        region_vaddr: VirBytes,
        page_offset: VirBytes,
        write: bool,
        caller_endpoint: Endpoint,
    },
    FdClose {
        fd: i32,
    },
}
```

**与 Minix3 的对应**：`char reqstate[70]` + `void *opaque` → `VfsRequestState` 枚举。每种请求类型的状态大小编译时已知，无需 `memcpy` 固定大小缓冲区。

### 4.3 VfsCallback — 函数指针类型

> 设计决策：§3.1

```rust
pub(crate) type VfsCallbackFn = fn(
    server: &mut VmServer,
    reply: &VfsReply,
    state: &VfsRequestState,
) -> Result<(), VfsQueueError>;
```

**为什么不用 `Box<dyn FnOnce>`**：

1. 函数指针零堆分配，`VfsRequestState` 枚举直接内嵌
2. 回调种类固定（3 种），枚举完全覆盖
3. 函数指针可以 `Debug` 打印，闭包不透明
4. 与 Minix3 的 `vfs_callback_t` 函数指针对应

### 4.4 VfsRequestQueue — 串行激活模型

> 设计决策：§3.3（串行激活模型）

```rust
pub(crate) struct VfsRequest {
    pub request_type: VfsRequestType,
    pub req_id: u32,
    pub caller_endpoint: Endpoint,
    pub fd: i32,
    pub offset: u64,
    pub length: u32,
    pub callback: Option<VfsCallbackFn>,
    pub state: Option<VfsRequestState>,
}

pub(crate) struct VfsRequestQueue {
    queued: VecDeque<VfsRequest>,
    active: Option<VfsRequest>,
    next_id: u32,
    sender: &'static dyn IpcSender,
}

impl VfsRequestQueue {
    pub(crate) fn new(sender: &'static dyn IpcSender) -> Self {
        Self {
            queued: VecDeque::new(),
            active: None,
            next_id: 1,
            sender,
        }
    }

    pub(crate) fn request(&mut self, mut req: VfsRequest) -> Result<(), VfsQueueError> {
        req.req_id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        self.queued.push_back(req);
        if self.active.is_none() {
            self.activate();
        }
        Ok(())
    }

    fn activate(&mut self) {
        if let Some(req) = self.queued.pop_front() {
            self.active = Some(req);
            // self.sender.async_send(Endpoint::VFS, &req.to_message());
        }
    }

    pub(crate) fn handle_reply(
        &mut self,
        reply: VfsReply,
    ) -> Result<Option<(VfsCallbackFn, VfsReply, VfsRequestState)>, VfsQueueError> {
        let req = self.active.take()
            .ok_or(VfsQueueError::NoActiveRequest)?;

        if req.req_id != reply.req_id {
            self.active = Some(req);
            return Err(VfsQueueError::UnexpectedReply);
        }

        let result = match (req.callback, req.state) {
            (Some(cb), Some(state)) => Some((cb, reply, state)),
            _ => None,
        };

        if !self.queued.is_empty() {
            self.activate();
        }

        Ok(result)
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.active.is_none() && self.queued.is_empty()
    }

    pub(crate) fn has_active(&self) -> bool {
        self.active.is_some()
    }
}
```

**与当前代码的关键差异**：

| 方面 | 旧代码 | 当前代码（已对齐设计） | Minix3 依据 |
|------|---------|--------|-----------|
| 队列结构 | `pending: VecDeque` | `active + queued` | §2.1 `first_queued + active` |
| 回复匹配 | `remove_by_caller(endpoint)` | `active.req_id == reply.req_id` | §2.2 `orignode = active` |
| 回调类型 | `fn(&mut VfsReply, &mut VmServer)` | `fn(&mut VmServer, &VfsReply, &VfsRequestState)` | §2.1 `callback + reqstate` |
| 请求类型 | `ReadPage/WritePage/SyncPage/FdLookup` | `FdLookup/FdIo/FdClose` | §2.3 三种请求 |

### 4.5 FdRefTable + FdRefEntry — 显式引用计数

> 设计决策：§3.2（显式 refcount + FdRefTable）

```rust
pub(crate) struct FdRefEntry {
    pub fd: i32,
    pub dev: u64,
    pub ino: u64,
    pub may_close: bool,
    pub refcount: u32,
}

pub(crate) struct PendingFdClose {
    pub fd: i32,
    pub dev: u64,
    pub ino: u64,
}

struct FdRefTableInner {
    entries: alloc::collections::BTreeMap<u32, FdRefEntry>,
    next_id: u32,
}

pub(crate) struct FdRefTable {
    inner: core::cell::UnsafeCell<FdRefTableInner>,
}

// SAFETY: single-threaded event loop model; no concurrent access.
unsafe impl Sync for FdRefTable {}

impl FdRefTable {
    const fn new_const() -> Self {
        Self {
            inner: core::cell::UnsafeCell::new(FdRefTableInner {
                entries: BTreeMap::new(),
                next_id: 1,
            }),
        }
    }

    pub(crate) fn get_global() -> &'static FdRefTable {
        static FDREF_TABLE: FdRefTable = FdRefTable::new_const();
        &FDREF_TABLE
    }

    fn inner(&self) -> &mut FdRefTableInner {
        unsafe { &mut *self.inner.get() }
    }

    /// 创建新的 fdref 条目（对应 fdref_new）
    pub(crate) fn create(
        &self,
        fd: i32, dev: u64, ino: u64, may_close: bool,
    ) -> u32 {
        let inner = self.inner();
        let id = inner.next_id;
        inner.next_id += 1;
        inner.entries.insert(id, FdRefEntry {
            fd, dev, ino, may_close, refcount: 0,
        });
        id
    }

    /// 增加引用计数（对应 fdref_ref）
    pub(crate) fn ref_entry(&self, id: u32) {
        if let Some(entry) = self.inner().entries.get_mut(&id) {
            entry.refcount += 1;
        }
    }

    /// 减少引用计数，如果 refcount==0 返回 PendingFdClose（对应 fdref_deref）
    pub(crate) fn deref_entry(&self, id: u32) -> Option<PendingFdClose> {
        let inner = self.inner();
        let entry = inner.entries.get_mut(&id)?;
        entry.refcount = entry.refcount.saturating_sub(1);
        if entry.refcount == 0 {
            let entry = inner.entries.remove(&id)?;
            if entry.may_close {
                Some(PendingFdClose {
                    fd: entry.fd, dev: entry.dev, ino: entry.ino,
                })
            } else {
                None
            }
        } else {
            None
        }
    }

    /// 去重查找（对应 fdref_dedup_or_new）
    pub(crate) fn find_by_dev_ino(&self, dev: u64, ino: u64) -> Option<u32> {
        self.inner().entries.iter()
            .find(|(_, e)| e.dev == dev && e.ino == ino)
            .map(|(id, _)| *id)
    }

    pub(crate) fn get(&self, id: u32) -> Option<&FdRefEntry> {
        unsafe { (*self.inner.get()).entries.get(&id) }
    }
}
```

**FdRefTable 持有位置**：全局静态 `FDREF_TABLE`，通过 `get_global()` 访问。与 `VmProcTable` 的设计模式一致——`UnsafeCell` + 静态常量 + 单线程事件循环安全保障。

**`may_close` 的语义**：§2.10.1 中 `do_vfs_mmap` 的 `mayclosefd=0`——VFS 管理的 fd 不自动关闭。`do_mmap` 的 `mayclosefd=1`——VM 管理的 fd 在最后一个引用消失时关闭。

### 4.6 PagefaultResult + PagefaultAction 扩展

> 设计决策：§3.4

```rust
// memtype.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PagefaultResult {
    Handled,
    NeedNewPage,
    NeedCow,
    NeedVfsIo,          // 新增：需要 VFS I/O（对应 SUSPEND）
    AccessViolation,
}

// cow_exec_pf.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PagefaultAction {
    Handled,
    MappedNewPage,
    CowResolved,
    Suspended,          // 新增：缺页挂起，不回复用户进程
    AccessViolation,
}
```

**缺页框架中的处理**：

```rust
// cow_exec_pf.rs: resolve_pagefault_core()
match result {
    PagefaultResult::NeedVfsIo => {
        // 1. 从 VrParam::File 获取 fdref_id → fd, dev, ino
        // 2. 构造 VfsRequestState::FdIo { region_vaddr, page_offset, write, caller }
        // 3. vfs_queue.request(VfsRequest { type: FdIo, callback: Some(mappedfile_pf_cont), ... })
        Ok(PagefaultAction::Suspended)
    }
    // ... 其他变体不变
}
```

### 4.7 MappedFile memtype 完整实现

> 设计决策：§3.2（fdref_id）、§3.4（NeedVfsIo）、§3.7（cow_block 已有机制）

```rust
pub(crate) struct MappedFile;

impl MemType for MappedFile {
    fn name(&self) -> &'static str { "file-mapped memory" }

    fn writable(&self, _frames: &PageFrames, _slot: PageSlot, _region: &VirRegion) -> bool {
        false  // mappedfile_writable: 文件映射页初始只读，写入触发 CoW
    }

    fn ev_pagefault(
        &self,
        _proc_endpoint: Endpoint,
        region: &mut VirRegion,
        _frames: &mut PageFrames,
        offset: VirBytes,
        write: bool,
    ) -> Result<PagefaultResult, MemTypeError> {
        if let VrParam::File { inited, fdref_id, .. } = &region.param {
            if !inited { return Ok(PagefaultResult::NeedNewPage); }

            // 缓存查找逻辑由缺页框架在 NeedVfsIo 返回后处理
            // 这里简化：未映射的页都需要 VFS I/O
            let page_idx = offset.get() / PAGE_SIZE;
            if page_idx < region.physblocks.len() {
                if let Some(slot) = &region.physblocks[page_idx] {
                    if slot.is_mapped() {
                        if !write {
                            return Ok(PagefaultResult::Handled);
                        }
                        // 写入已映射页 → CoW（§3.7: cow_block 已有机制）
                        return Ok(PagefaultResult::NeedCow);
                    }
                }
            }
        }
        // 页未映射或 fdref 无效 → 需要 VFS I/O
        Ok(PagefaultResult::NeedVfsIo)
    }

    fn ev_split(
        &self,
        _proc: &mut ActiveProc<'_>,
        original: &VirRegion,
        left: &mut VirRegion,
        right: &mut VirRegion,
    ) -> Result<(), MemTypeError> {
        // mappedfile_split: 两个子区域都引用同一个 fdref
        if let VrParam::File { inited: true, fdref_id, offset, clearend } = &original.param {
            let fdref_id = *fdref_id;
            let orig_offset = *offset;
            let orig_clearend = *clearend;

            left.param = VrParam::File {
                inited: true,
                fdref_id,
                offset: orig_offset,
                clearend: 0,
            };

            right.param = VrParam::File {
                inited: true,
                fdref_id,
                offset: orig_offset + left.length.get(),
                clearend: orig_clearend,
            };

            // fdref_ref: 两个子区域各增加一次引用
            // 调用方负责 fdref_ref(fdref_id) × 2
        }
        Ok(())
    }

    fn ev_low_shrink(
        &self,
        region: &mut VirRegion,
        len: VirBytes,
    ) -> Result<(), MemTypeError> {
        // mappedfile_lowshrink: 头部取消时 offset 前移
        if let VrParam::File { offset, .. } = &mut region.param {
            *offset += len.get();
        }
        Ok(())
    }

    fn ev_delete(&self, region: &mut VirRegion) {
        // mappedfile_delete: 释放 fdref 引用
        // 调用方负责 fdref_deref(fdref_id)，可能触发 FDCLOSE
        if let VrParam::File { inited, fdref_id, .. } = &mut region.param {
            *inited = false;
            *fdref_id = None;
        }
    }

    fn ev_copy(
        &self,
        src: &VirRegion,
        dst: &mut VirRegion,
    ) -> Result<(), MemTypeError> {
        // mappedfile_copy: fork 时复制参数
        if let VrParam::File { inited: true, fdref_id, offset, clearend } = &src.param {
            dst.param = VrParam::File {
                inited: true,
                fdref_id: *fdref_id,
                offset: *offset,
                clearend: *clearend,
            };
            // 调用方负责 fdref_ref(fdref_id)
        }
        Ok(())
    }

    fn ev_sanitycheck(
        &self,
        _frames: &PageFrames,
        _slot: PageSlot,
    ) -> Result<(), MemTypeError> {
        Ok(())
    }
}
```

**注意**：`ev_split`、`ev_delete`、`ev_copy` 中的 `fdref_ref`/`fdref_deref` 操作需要访问 `FdRefTable`，但 `MemType` trait 方法签名中没有 `FdRefTable` 参数。解决方案：

1. **方案 A**：在 `VirRegion` 上增加 `fdref_ref`/`fdref_deref` 的延迟操作队列，由调用方统一处理
2. **方案 B**：修改 `MemType` trait 签名，增加 `FdRefTable` 参数
3. **方案 C**：`ev_split`/`ev_delete`/`ev_copy` 只设置 `VrParam::File` 的字段，`fdref_ref`/`fdref_deref` 由调用方（region 框架）负责

**选择方案 C**：与当前代码中 `ev_split` 的调用模式一致——调用方在 `ev_split` 前后负责资源管理，`ev_split` 只调整参数。这保持了 `MemType` trait 的简洁性。

### 4.8 VrParam::File 补全

> 设计决策：§3.5（fdref_id）

```rust
#[derive(Debug, Clone)]
pub(crate) enum VrParam {
    Direct { phys: PhysBytes },
    Shared { ep: i32, vaddr: VirBytes, id: i32 },
    PbCache { pfn: u32 },
    File {
        inited: bool,
        fdref_id: Option<u32>,   // FdRefTable 中的索引
        offset: u64,             // 文件偏移（页对齐）
        clearend: u16,           // 尾部清零字节数
    },
}
```

**与当前代码的差异**：`FileDescriptorRef` 已替换为 `fdref_id: Option<u32>`，`Clone` 只是复制索引值，真正的引用计数由 `FdRefTable` 管理。

### 4.9 页缓存 CacheKey + PageCacheEntry

> 设计决策：§3.6

```rust
#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum CacheKey {
    ByInode { dev: u64, ino: u64, offset: u64 },
    ByDevice { dev: u64, offset: u64 },
}

pub(crate) struct PageCacheEntry {
    pub pfn: u32,
    pub refcount: u32,
}

pub(crate) struct PageCache {
    entries: alloc::collections::BTreeMap<CacheKey, PageCacheEntry>,
}

impl PageCache {
    pub(crate) fn new() -> Self {
        Self { entries: BTreeMap::new() }
    }

    /// 按 inode 查找缓存页（对应 find_cached_page_byino）
    pub(crate) fn find_by_inode(
        &self, dev: u64, ino: u64, offset: u64,
    ) -> Option<&PageCacheEntry> {
        self.entries.get(&CacheKey::ByInode { dev, ino, offset })
    }

    /// 按 dev 查找缓存页（对应 find_cached_page_bydev，VMC_NO_INODE 路径）
    pub(crate) fn find_by_device(
        &self, dev: u64, offset: u64,
    ) -> Option<&PageCacheEntry> {
        self.entries.get(&CacheKey::ByDevice { dev, offset })
    }

    /// 插入缓存页
    pub(crate) fn insert(
        &mut self, key: CacheKey, pfn: u32,
    ) {
        self.entries.insert(key, PageCacheEntry { pfn, refcount: 1 });
    }

    /// 移除缓存页（VMSF_ONCE 映射后 rmcache）
    pub(crate) fn remove(&mut self, key: &CacheKey) -> Option<PageCacheEntry> {
        self.entries.remove(key)
    }
}
```

**C 源码依据**：§2.6 的 `VMC_NO_INODE` 分支——`ino == VMC_NO_INODE` 时走 `find_cached_page_bydev`，否则走 `find_cached_page_byino`。

### 4.10 IpcSender trait + MockIpcSender

> 设计决策：§3.8（IPC mock 模式）

```rust
pub(crate) trait IpcSender {
    fn async_send(&self, dest: Endpoint, msg: &VfsCallMessage) -> Result<(), IpcError>;
}

pub(crate) struct VfsCallMessage {
    pub req_type: VfsRequestType,
    pub req_id: u32,
    pub fd: i32,
    pub endpoint: Endpoint,
    pub offset: u64,
    pub length: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IpcError {
    SendFailed,
}

// 生产实现
pub(crate) struct MinixIpcSender;

impl IpcSender for MinixIpcSender {
    fn async_send(&self, dest: Endpoint, msg: &VfsCallMessage) -> Result<(), IpcError> {
        // 对接 Minix3 的 ipc_asynsend(VFS_PROC_NR, &message)
        // 当前为 stub，等 IPC 基础设施就绪后实现
        Ok(())
    }
}

// Mock 实现（仅测试用）
#[cfg(test)]
pub(crate) struct MockIpcSender {
    pub sent: alloc::vec::Vec<VfsCallMessage>,
}

#[cfg(test)]
impl IpcSender for MockIpcSender {
    fn async_send(&self, dest: Endpoint, msg: &VfsCallMessage) -> Result<(), IpcError> {
        // 测试中不真正发送 IPC，只记录消息
        Ok(())
    }
}
```

**为什么 `IpcSender` 是 trait 而非具体类型**：VM 的 IPC 发送是硬件/平台相关的操作（调用内核 syscall），不同测试环境需要不同实现。trait 抽象使得 `VfsRequestQueue` 可以在单元测试中使用 `MockIpcSender`，在生产环境使用 `MinixIpcSender`。

**trait 设计质量评估**（§2.5）：`IpcSender` 有 ≥2 个行为不同的实现（`MinixIpcSender` 真正发送 IPC，`MockIpcSender` 记录消息），且被 `VfsRequestQueue` 用作依赖注入点。✅ 合理。

---

## 5. 异步通信的补充设计

> 本章讨论 §3/§4 未覆盖的边界情况。

### 5.1 进程退出与未完成的 VFS 请求

如果进程在 VFS 请求未完成时退出：

```c
/* do_vfs_reply 中 */
if(vm_isokendpt(m->VMV_ENDPOINT, &n) != OK)
    vmp = NULL;  /* 进程已退出 */

/* 回调中检查 vmp */
if(req_callback) req_callback(vmp, m, cbarg, ...);
```

回调函数必须处理 `vmp == NULL` 的情况——进程已退出，不需要更新页表，但可能需要释放已分配的资源。

在 Rust 中，`VfsCallbackFn` 的签名是 `fn(server: &mut VmServer, reply: &VfsReply, state: &VfsRequestState)`。回调内部通过 `server.proc_table.get(endpoint)` 检查进程是否仍存在，不存在则跳过页表更新，但仍释放物理页等资源。

### 5.2 请求队列的时序保证

```
时间线:
  t1: VM 发出 FDLOOKUP 请求 (active)
  t2: VM 收到其他缺页 → 需要 FDIO → 排队 (queued)
  t3: VM 收到其他缺页 → 需要 FDIO → 排队 (queued)
  t4: VFS 回复 FDLOOKUP → 回调处理 → active = None → activate() → 发送下一个 FDIO
  t5: VFS 回复 FDIO → 回调处理 → activate() → 发送下一个 FDIO
  t6: VFS 回复 FDIO → 回调处理 → 队列空
```

**保证**：请求按发出顺序处理（FIFO），回调在对应回复到达时调用。这是 §3.3 串行激活模型的直接结果。

---

## 6. 修改清单

> 本章列出需要修改/新增的代码文件，与 §4 的实现详解一一对应。

### 6.1 需要修改的现有代码

| 文件 | 修改内容 | 对应章节 | 优先级 | 状态 |
|------|---------|---------|--------|------|
| `memtype.rs` | `PagefaultResult` 新增 `NeedVfsIo` | §4.6 | P0 | ✅ 已实现 |
| `memtype.rs` | 新增 `MappedFile` memtype | §4.7 | P0 | ✅ 已实现 |
| `memtype.rs` | `MappedFile` 实现 `ev_delete` | §4.7 | P0 | ✅ 已实现 |
| `cow_exec_pf.rs` | `PagefaultAction` 新增 `Suspended` | §4.6 | P0 | ✅ 已实现 |
| `cow_exec_pf.rs` | 缺页框架处理 `NeedVfsIo` 分支 | §4.6 | P0 | ✅ 已实现 |
| `region/vir_region.rs` | `VrParam::File` 补全 `fdref_id` | §4.8 | P0 | ✅ 已实现 |
| `region/vir_region.rs` | `split` 内联处理 File 参数 + `fdref_ref` | §4.7 | P1 | ✅ 已实现 |
| `region/mod.rs` | `free_region_pages` 增加 `ev_delete` + `fdref_deref` | §4.7 | P0 | ✅ 已实现 |
| `vfs_queue.rs` | 重构为串行激活模型 | §4.4 | P0 | ✅ 已实现 |
| `mmap.rs` | 实现文件映射路径 + `fdref_id` 创建 | §4.7 | P1 | ✅ 已实现 |
| `fork.rs` | `fork_region` 增加 `fdref_ref` | §4.7 | P1 | ✅ 已实现 |

### 6.2 需要新增的代码

| 文件 | 新增内容 | 对应章节 | 优先级 | 状态 |
|------|---------|---------|--------|------|
| `fdref.rs` | `FdRefTable`, `FdRefEntry`, `PendingFdClose` | §4.5 | P0 | ✅ 已实现 |
| `pagecache.rs` | `PageCache`, `CacheKey`, `PageCacheEntry` | §4.9 | P1 | ✅ 已实现 |
| `ipc_sender.rs` | `IpcSender` trait, `MinixIpcSender`, `VfsCallMessage` | §4.10 | P1 | ✅ 已实现 |

### 6.3 测试计划

#### 6.3.1 单元测试（`#[cfg(test)]` 模块内）

| 测试 | 描述 | 对应章节 | 状态 |
|------|------|---------|------|
| `test_vfs_request_queue` | 请求排队和串行发送 | §4.4 | ✅ 已有 |
| `test_vfs_reply_callback` | 回复到达时正确调用回调 | §4.4 | ✅ 已有 |
| `test_fdref_refcount` | 显式引用计数正确 | §4.5 | ✅ 已有 |
| `test_fdref_deref_close` | 最后引用消失时返回 PendingFdClose | §4.5 | ✅ 已有 |
| `test_fdref_dedup` | 同文件复用 fdref | §4.5 | ✅ 已有 |
| `test_mappedfile_pagefault_miss` | 缓存未命中 → NeedVfsIo | §4.7 | 待实现 |
| `test_mappedfile_pagefault_hit` | 缓存命中 → 直接映射 | §4.7 | 待实现 |
| `test_mappedfile_cow` | 写入触发 CoW → 切换匿名 | §4.7 | 待实现 |
| `test_mappedfile_split` | split 后两个区域引用同一 fdref | §4.7 | 待实现 |
| `test_mappedfile_delete` | 删除区域 → fdref 释放 | §4.7 | 待实现 |
| `test_process_exit_with_pending_vfs` | 进程退出时有未完成的 VFS 请求 | §5.1 | 待实现 |
| `test_ipc_sender_mock` | MockIpcSender 记录消息 | §4.10 | ✅ 已有 |

#### 6.3.2 集成测试要点

- **MockIpcSender**：使用 `IpcSender` trait 的 mock 实现，拦截 VFS 请求消息，手动构造 `VfsReply` 回复
- **FdRefTable 全局状态**：测试间需重置全局 `FDREF_TABLE`，或使用独立的测试实例
- **VfsRequestQueue 串行语义**：验证 `has_active() == true` 时新请求入队、`handle_reply` 激活下一个
- **端到端流程**：mmap → pagefault → NeedVfsIo → VFS reply → 页映射 → 用户进程恢复

---

## 7. VM-VFS 交互的完整图景

> 本章是 §5 异步通信设计的端到端总览。§5 侧重边界情况（进程退出、时序保证），本章侧重正常流程的三种场景。

### 7.1 三种 VFS 交互场景

```
场景 1: mmap 文件
  用户 → VM_MMAP(fd=3) → VM → vfs_request(FdLookup) → VFS
  VFS → VM_VFS_REPLY(dev, ino, size) → VM → mmap_file_cont() → 创建区域

场景 2: 文件缺页
  用户 → #PF → VM → MappedFile::ev_pagefault → NeedVfsIo
  VM → vfs_request(FdIo) → VFS → 读取文件 → VM_VFS_REPLY(data) → VM
  VM → mappedfile_pf_cont() → 映射物理页 → 回复用户

场景 3: 关闭 fd
  用户 → munmap → VM → FdRefTable::deref_entry → refcount==0 → PendingFdClose
  VM → vfs_request(FdClose) → VFS → 关闭 fd → VM_VFS_REPLY → 无回调
```

### 7.2 memtype 与 VFS 交互的关系

| memtype | 需要 VFS? | 缺页行为 | 写入行为 |
|---------|----------|---------|---------|
| AnonymousMemory | ❌ | 分配零页 | 直接写入 |
| DirectPhysical | ❌ | 计算物理地址 | 直接写入 |
| SharedMemory | ❌ | 引用已有页 | CoW |
| MappedFile | ✅ | 查缓存/VFS读取 | CoW → 切换匿名 |

**MappedFile 是唯一需要 VFS 交互的 memtype**。其他 memtype 的缺页处理完全在 VM 内部完成。这就是 memtype 抽象的另一个好处：VFS 交互逻辑被封装在 `MappedFile` 中，不影响其他 memtype。

### 7.3 与 19-cow-exec-pagefault 的关系

exec 加载可执行文件时也需要 VFS 交互（读取 ELF 段）。但 exec 使用不同的机制：

| 方面 | mmap 文件 | exec 加载 |
|------|----------|----------|
| VFS 请求 | `FdIo` | `FdIo` |
| 缺页触发 | 用户访问文件映射页 | 用户访问代码/数据段 |
| memtype | `MappedFile` | `AnonymousMemory`（加载后） |
| CoW 行为 | 写入时 CoW → 切换匿名 | 加载时直接写入匿名页 |

exec 的文件加载可以复用 `vfs_request` 框架，但缺页回调不同——exec 加载的数据直接写入匿名页，不需要 MappedFile 的 CoW 逻辑。
