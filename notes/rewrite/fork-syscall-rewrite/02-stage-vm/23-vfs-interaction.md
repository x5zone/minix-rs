# 23-vfs-interaction: VM 与 VFS 的异步对话

> **分类**: VM服务
> **源码**: `minix3/minix/servers/vm/vfs.c`, `fdref.c`, `mem_file.c`, `mmap.c`
> **说明**: VM 如何与 VFS 异步通信实现文件映射、页换入、fd 引用计数

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
| 22-vm-munmap | munmap 文件映射区域时调用 fdref_deref → vfs_request(FDCLOSE) |
| 14-phys-region | PhysBlock 引用计数在文件映射中的特殊处理 |
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
    SLABALLOC(reqnode);

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
1. mmap 文件 → fdref_new(owner, ino, dev, fd) → 创建 fdref (refcount=0)
2. 绑定到区域 → fdref_ref(fdref, region) → refcount++
3. split 区域 → fdref_ref(fdref, r1) + fdref_ref(fdref, r2) → refcount++ ×2
4. munmap 区域 → fdref_deref(region) → refcount--
5. refcount == 0 → vfs_request(FDCLOSE) → 关闭 fd + 释放 fdref
```

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
    .ev_unreference = mappedfile_unreference,   /* 释放物理页 */
    .ev_pagefault = mappedfile_pagefault,       /* 异步读取文件页 */
    .ev_sanitycheck = mappedfile_sanitycheck,
    .ev_copy = mappedfile_copy,                 /* fork 时复制 */
    .writable = mappedfile_writable,            /* 永远返回 0 */
    .ev_split = mappedfile_split,               /* split 时调整 offset */
    .ev_lowshrink = mappedfile_lowshrink,       /* 头部取消时调整 offset */
    .ev_delete = mappedfile_delete,             /* 释放 fdref */
    .pt_flags = mappedfile_pt_flags,
};
```

**关键特性**：
- `writable` 永远返回 0 → 文件映射页初始只读，写入时触发 CoW
- `ev_pagefault` 可能返回 SUSPEND → 异步读取文件数据
- `ev_delete` 调用 `fdref_deref` → 最后一个引用消失时关闭 fd

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
        cp = find_cached_page_byino(..., referenced_offset, ...);

        if(cp && (!cb || !(cp->flags & VMSF_ONCE))) {
            /* 缓存命中！直接使用缓存的物理页 */
            pb_unreferenced(region, ph, 0);
            pb_link(ph, cp->page, ph->offset, region);

            /* 尾部页需要 CoW（清零 clearend） */
            if(roundup(ph->offset + region->param.file.clearend,
                VM_PAGE_SIZE) >= region->length) {
                cow_block(vmp, region, ph, region->param.file.clearend);
            } else if(write) {
                cow_block(vmp, region, ph, 0);
            }
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
        /* 匿名映射：直接创建匿名区域 */
        vr = mmap_region(vmp, addr, flags, len,
            VR_WRITABLE | VR_ANON, &mem_type_anon, execpriv);
    } else {
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

---

## 3. Rust 设计决策

### 3.1 现有代码状态

| 组件 | 现有状态 | VFS 交互需要 |
|------|---------|------------|
| `MemType` trait | ✅ 已有 `ev_pagefault` 签名 | 需要支持异步返回 |
| `AnonymousMemory` | ✅ 已实现 | 不需要 VFS |
| `DirectPhysical` | ✅ 已实现 | 不需要 VFS |
| `SharedMemory` | ✅ 已实现 | 不需要 VFS |
| `MappedFile` memtype | ❌ 不存在 | 需要完整实现 |
| `FdRef` 结构 | ❌ 不存在 | 需要实现引用计数 |
| `vfs_request()` | ❌ 不存在 | 需要实现异步请求框架 |
| `do_vfs_reply()` | ❌ 不存在 | 需要实现回复处理 |
| `VrParam::File` | ✅ 已有占位 | 需要补全字段 |
| 页缓存 | ❌ 不存在 | 需要实现 |

### 3.2 设计原则

**原则 1：异步回调模型**

与 Minix3 一致，使用请求队列 + 回调模型。Rust 中可以用 `FnOnce` 闭包替代函数指针：

```rust
type VfsCallback = Box<dyn FnOnce(Option<&mut ActiveProc<'_>>, &VfsReply)>;
```

**原则 2：FdRef 使用 Arc**

Minix3 用手动引用计数（`fdref_ref` / `fdref_deref`）。Rust 中可以用 `Arc<FdRefInner>` 自动管理：

```rust
struct FdRefInner {
    fd: i32,
    dev: u64,
    ino: u64,
}

struct FdRef(Arc<FdRefInner>);
```

当最后一个 `Arc` 被 drop 时，自动发送 `FDCLOSE` 请求（通过 `Drop` trait）。

**原则 3：MappedFile memtype**

新增 `MappedFile` memtype，实现文件映射的所有回调：

```rust
struct MappedFile;

impl MemType for MappedFile {
    fn ev_pagefault(&self, ...) -> Result<PagefaultResult, MemTypeError> {
        // 查缓存 → 未命中 → 返回 NeedVfsIo
    }
    fn ev_split(&self, ...) { /* 调整 offset, 增加 fdref */ }
    fn ev_low_shrink(&self, ...) { /* offset += len */ }
    fn ev_delete(&self, ...) { /* fdref_deref */ }
}
```

**原则 4：PagefaultResult 扩展**

现有 `PagefaultResult` 需要新增变体：

```rust
enum PagefaultResult {
    Handled,
    NeedNewPage,
    NeedCow,
    NeedVfsIo,      // 新增：需要 VFS I/O
    AccessViolation,
}
```

### 3.3 与 Minix3 的关键差异

| 方面 | Minix3 | minix-rs |
|------|--------|----------|
| 回调 | 函数指针 + void* | `Box<dyn FnOnce>` 闭包 |
| fdref 引用计数 | 手动 refcount++ | `Arc<FdRefInner>` 自动管理 |
| 请求状态保存 | `char reqstate[70]` memcpy | 闭包捕获状态 |
| 请求队列 | 链表 + SLABALLOC | `VecDeque<VfsRequest>` |
| SUSPEND 返回 | 整数常量 | `Result<PagefaultResult, _>` 枚举 |
| 页缓存 | `cached_page` 链表 | `HashMap<CacheKey, PhysBlock>` |
| cow_block 后 memtype 切换 | `ph->memtype = &mem_type_anon` | 需要设计安全的切换机制 |

---

## 4. Rust 实现详解

### 4.1 VfsRequest — 异步请求框架

```rust
pub(crate) struct VfsRequest {
    msg: VfsCallMessage,
    callback: Option<Box<dyn FnOnce(Option<VmProcRef>, &VfsReplyMessage)>>,
    req_id: u32,
    who: Endpoint,
}

pub(crate) struct VfsRequestQueue {
    queued: VecDeque<VfsRequest>,
    active: Option<VfsRequest>,
    next_id: u32,
}

impl VfsRequestQueue {
    pub(crate) fn new() -> Self {
        Self {
            queued: VecDeque::new(),
            active: None,
            next_id: 1,
        }
    }

    /// 发送异步请求给 VFS
    pub(crate) fn request(
        &mut self,
        reqno: VfsRequestType,
        fd: i32,
        endpoint: Endpoint,
        offset: u64,
        len: u32,
        callback: Option<Box<dyn FnOnce(Option<VmProcRef>, &VfsReplyMessage)>>,
    ) -> Result<(), VfsError> {
        let req_id = self.next_id;
        self.next_id += 1;

        let msg = VfsCallMessage {
            msg_type: VFS_VMCALL,
            req: reqno as u32,
            fd,
            req_id,
            endpoint,
            offset,
            len,
        };

        let req = VfsRequest {
            msg,
            callback,
            req_id,
            who: endpoint,
        };

        self.queued.push_back(req);

        if self.active.is_none() {
            self.activate();
        }

        Ok(())
    }

    fn activate(&mut self) {
        if let Some(req) = self.queued.pop_front() {
            self.active = Some(req);
            // 异步发送消息给 VFS
            // ipc_asynsend(VFS_PROC_NR, &self.active.as_ref().unwrap().msg);
        }
    }

    /// 处理 VFS 回复
    pub(crate) fn handle_reply(
        &mut self,
        reply: &VfsReplyMessage,
        table: &VmProcTable,
    ) {
        let req = self.active.take()
            .expect("VFS reply without active request");

        assert_eq!(req.req_id, reply.req_id);

        let vmp = table.vm_isokendpt(req.who)
            .ok()
            .and_then(|slot| table.get_active(slot));

        if let Some(callback) = req.callback {
            callback(vmp, reply);
        }

        if !self.queued.is_empty() {
            self.activate();
        }
    }
}
```

### 4.2 FdRef — 文件描述符引用计数

```rust
pub(crate) struct FdRefInner {
    pub fd: i32,
    pub dev: u64,
    pub ino: u64,
}

pub(crate) struct FdRef {
    inner: Arc<FdRefInner>,
}

impl FdRef {
    pub(crate) fn new(fd: i32, dev: u64, ino: u64) -> Self {
        Self {
            inner: Arc::new(FdRefInner { fd, dev, ino }),
        }
    }

    pub(crate) fn clone_ref(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }

    pub(crate) fn fd(&self) -> i32 {
        self.inner.fd
    }

    pub(crate) fn dev(&self) -> u64 {
        self.inner.dev
    }

    pub(crate) fn ino(&self) -> u64 {
        self.inner.ino
    }

    pub(crate) fn refcount(&self) -> usize {
        Arc::strong_count(&self.inner)
    }
}

impl Drop for FdRefInner {
    fn drop(&mut self) {
        // 最后一个引用消失，异步关闭 fd
        // vfs_request(VMVFSREQ_FDCLOSE, self.fd, ...)
    }
}
```

**Arc 的优势**：当最后一个 `FdRef` 被 drop 时，`FdRefInner::drop()` 自动触发，无需手动检查 refcount。这比 Minix3 的手动 `fdref_deref` 更安全。

### 4.3 VrParam::File — 补全文件参数

```rust
pub(crate) enum VrParam {
    Direct { phys: PhysBytes },
    Shared { ep: i32, vaddr: VirBytes, id: i32 },
    PbCache { pb: Option<NonNull<PhysBlock>> },
    File {
        inited: bool,
        offset: u64,        /* 文件偏移（页对齐） */
        clearend: u16,      /* 尾部清零字节数 */
        fdref: Option<FdRef>, /* 文件描述符引用 */
    },
}
```

### 4.4 MappedFile — 文件映射 memtype

```rust
pub(crate) struct MappedFile;

impl MappedFile {
    pub(crate) const fn new() -> Self { Self }
}

impl MemType for MappedFile {
    fn name(&self) -> &'static str {
        "file-mapped memory"
    }

    fn writable(&self, _pr: &crate::region::PhysRegion) -> bool {
        false  // 文件映射页初始只读，写入触发 CoW
    }

    fn ev_unreference(&self, pr: &mut crate::region::PhysRegion) -> Result<bool, MemTypeError> {
        let refcount = pr.get_refcount().unwrap_or(0);
        if refcount == 0 && pr.get_phys_addr().unwrap_or(PhysBlock::MAP_NONE) != PhysBlock::MAP_NONE {
            Ok(true)  // 释放物理页
        } else {
            Ok(false)
        }
    }

    fn ev_pagefault(
        &self,
        _proc: &ActiveProc<'_>,
        region: &mut crate::region::VirRegion,
        pr: &mut crate::region::PhysRegion,
        write: bool,
    ) -> Result<PagefaultResult, MemTypeError> {
        if pr.get_phys_addr().unwrap_or(PhysBlock::MAP_NONE) == PhysBlock::MAP_NONE {
            // 全新页：需要 VFS I/O
            return Ok(PagefaultResult::NeedVfsIo);
        }

        if !write {
            return Ok(PagefaultResult::Handled);
        }

        // 写入触发 CoW → 切换为匿名内存
        Ok(PagefaultResult::NeedCow)
    }

    fn ev_split(
        &self,
        _proc: &mut ActiveProc<'_>,
        original: &crate::region::VirRegion,
        left: &mut crate::region::VirRegion,
        right: &mut crate::region::VirRegion,
    ) {
        if let VrParam::File { inited, offset, clearend, fdref } = &original.param {
            if !inited { return; }

            // 两个子区域都引用同一个 fdref
            let fdref_clone_l = fdref.as_ref().map(|f| f.clone_ref());
            let fdref_clone_r = fdref.as_ref().map(|f| f.clone_ref());

            left.param = VrParam::File {
                inited: true,
                offset: *offset,
                clearend: 0,  // 左半部分没有 clearend
                fdref: fdref_clone_l,
            };

            right.param = VrParam::File {
                inited: true,
                offset: offset + left.length.get(),  // 右半部分 offset 前移
                clearend: *clearend,
                fdref: fdref_clone_r,
            };
        }
    }

    fn ev_low_shrink(
        &self,
        region: &mut crate::region::VirRegion,
        len: VirBytes,
    ) -> Result<(), MemTypeError> {
        if let VrParam::File { offset, .. } = &mut region.param {
            *offset += len.get();
        }
        Ok(())
    }

    fn ev_delete(&self, region: &mut crate::region::VirRegion) {
        // 将 fdref 设为 None，触发 Arc 的 drop
        if let VrParam::File { fdref, inited, .. } = &mut region.param {
            *fdref = None;
            *inited = false;
        }
    }

    fn ev_copy(
        &self,
        src: &crate::region::VirRegion,
        dst: &mut crate::region::VirRegion,
    ) -> Result<(), MemTypeError> {
        if let VrParam::File { inited, offset, clearend, fdref } = &src.param {
            if !inited { return Ok(()); }
            dst.param = VrParam::File {
                inited: true,
                offset: *offset,
                clearend: *clearend,
                fdref: fdref.as_ref().map(|f| f.clone_ref()),
            };
        }
        Ok(())
    }
}
```

### 4.5 文件缺页的异步处理流程

```rust
/// 处理文件映射缺页（VFS I/O 完成后）
fn handle_file_pagefault_completion(
    vmp: &mut ActiveProc<'_>,
    region_vaddr: VirBytes,
    page_offset: VirBytes,
    vfs_data: &[u8],
    write: bool,
    page_alloc: &mut VmPageAllocator,
) -> Result<(), VmError> {
    let region = vmp.regions_mut().find_mut(region_vaddr)
        .ok_or(VmError::NotFound)?;

    // 1. 分配物理页
    let new_phys = page_alloc.alloc_phys(1)
        .ok_or(VmError::NoMemory)?;

    // 2. 复制 VFS 返回的数据到物理页
    // sys_vircopy(SELF, vfs_data, new_phys, PAGE_SIZE);

    // 3. 创建 PhysRegion 并链接
    let pr = PhysRegion::new_linked(new_phys, page_offset);
    region.set_phys_region(page_offset, pr);

    // 4. 如果是写入，执行 CoW → 切换为匿名内存
    if write {
        // cow_block: 复制到新页 + 切换 memtype
    }

    // 5. 更新页表
    // write_pt_single(vmp, region, pr);

    Ok(())
}
```

### 4.6 完整文件映射流程（Rust）

```
mmap(fd=3, offset=0x1000, len=0x4000):
  │
  ▼
do_mmap():
  ├── fd != -1 → 文件映射
  ├── vfs_request(FDLOOKUP, fd=3) → SUSPEND
  │
  ▼  VFS 回复: { fd=3, dev=0x800, ino=42, size=0x10000 }
  │
  handle_mmap_vfs_reply():
  ├── mmap_region(vmp, addr, len, VR_WRITABLE, MappedFile)
  ├── mappedfile_setfile(vmp, region, fd=3, offset=0x1000, dev, ino)
  │   ├── fdref_dedup_or_new() → FdRef(Arc{fd=3, dev, ino})
  │   ├── fdref.clone_ref() → refcount=2 (region + fdref)
  │   └── prefill: 查缓存，命中则直接映射
  └── 回复用户进程: retaddr = region.vaddr

用户写入 0x2000 → #PF:
  │
  ▼
mappedfile_pagefault():
  ├── phys == MAP_NONE → NeedVfsIo
  │
  ▼  缺页处理代码:
  ├── vfs_request(FDIO, fd=3, offset=0x2000, len=PAGE_SIZE) → SUSPEND
  │
  ▼  VFS 回复: { data=[...], result=OK }
  │
  handle_file_pagefault_completion():
  ├── alloc_phys(1) → new_phys
  ├── 复制 VFS 数据到 new_phys
  ├── 创建 PhysRegion → 链接到 region
  ├── write == true → cow_block → 切换为匿名内存
  ├── 更新页表
  └── 回复用户进程

munmap(文件映射区域):
  │
  ▼
unmap_region():
  ├── free_range() → 释放物理页
  ├── MappedFile::on_delete() → fdref = None
  │   └── Arc::drop → refcount--
  │       └── refcount == 0 → FdRefInner::drop() → vfs_request(FDCLOSE, fd=3)
  └── 更新页表
```

---

## 5. 异步通信的深层设计

### 5.1 为什么必须串行化

Minix3 的 VFS 请求是**严格串行**的：同一时间只有一个活跃请求。原因：

1. **VFS 的限制**：VFS 可能无法处理并发的 VM 请求
2. **状态一致性**：回调函数依赖请求发出时的状态，并发请求可能导致状态混乱
3. **简化设计**：串行化消除了竞态条件

### 5.2 请求队列的时序保证

```
时间线:
  t1: VM 发出 FDLOOKUP 请求 (active)
  t2: VM 收到其他缺页 → 需要 FDIO → 排队 (queued)
  t3: VM 收到其他缺页 → 需要 FDIO → 排队 (queued)
  t4: VFS 回复 FDLOOKUP → 回调处理 → active = null → 发送下一个 FDIO
  t5: VFS 回复 FDIO → 回调处理 → 发送下一个 FDIO
  t6: VFS 回复 FDIO → 回调处理 → 队列空
```

**保证**：请求按发出顺序处理（FIFO），回调在对应回复到达时调用。

### 5.3 进程退出与未完成的 VFS 请求

如果进程在 VFS 请求未完成时退出：

```c
/* do_vfs_reply 中 */
if(vm_isokendpt(m->VMV_ENDPOINT, &n) != OK)
    vmp = NULL;  /* 进程已退出 */

/* 回调中检查 vmp */
if(req_callback) req_callback(vmp, m, cbarg, ...);
```

回调函数必须处理 `vmp == NULL` 的情况——进程已退出，不需要更新页表，但可能需要释放已分配的资源。

### 5.4 cow_block 的 memtype 切换

```c
/* Minix3 */
ph->memtype = &mem_type_anon;  /* 直接切换！ */
```

在 Rust 中，PhysRegion 的 memtype 切换需要更安全的设计。一种方案：

```rust
impl PhysRegion {
    /// CoW 后切换为匿名内存
    fn switch_to_anonymous(&mut self) {
        self.memtype = Some(&MEM_TYPE_ANON as &'static dyn MemType);
    }
}
```

这要求 `PhysRegion.memtype` 是 `Option<&'static dyn MemType>`，而不是编译时确定的类型。当前 Rust 代码中 PhysRegion 没有 memtype 字段，需要添加。

---

## 6. 实现清单

### 6.1 需要修改的现有代码

| 文件 | 修改内容 | 优先级 |
|------|---------|--------|
| `memtype.rs` | 新增 `MappedFile` memtype | 🔴 P0 |
| `memtype.rs` | `PagefaultResult` 新增 `NeedVfsIo` | 🔴 P0 |
| `region/vir_region.rs` | `VrParam::File` 补全 fdref 字段 | 🔴 P0 |
| `region/phys_region.rs` | PhysRegion 新增 memtype 字段 | 🟡 P1 |

### 6.2 需要新增的代码

| 文件 | 新增内容 | 优先级 |
|------|---------|--------|
| `vfs.rs` (新) | `VfsRequestQueue`, `VfsRequest`, `VfsRequestType` | 🔴 P0 |
| `vfs.rs` | `request()`, `handle_reply()` | 🔴 P0 |
| `fdref.rs` (新) | `FdRef`, `FdRefInner`, `Arc` 管理 | 🔴 P0 |
| `mmap.rs` (新) | `do_mmap()`, `mmap_file()`, `mmap_file_cont()` | 🔴 P0 |
| `pagecache.rs` (新) | 页缓存 `HashMap<CacheKey, PhysBlock>` | 🟡 P1 |

### 6.3 测试计划

| 测试 | 描述 |
|------|------|
| `test_vfs_request_queue` | 请求排队和串行发送 |
| `test_vfs_reply_callback` | 回复到达时正确调用回调 |
| `test_fdref_refcount` | Arc 引用计数正确 |
| `test_fdref_deref_close` | 最后引用消失时发送 FDCLOSE |
| `test_fdref_dedup` | 同文件复用 fdref |
| `test_mappedfile_pagefault_miss` | 缓存未命中 → NeedVfsIo |
| `test_mappedfile_pagefault_hit` | 缓存命中 → 直接映射 |
| `test_mappedfile_cow` | 写入触发 CoW → 切换匿名 |
| `test_mappedfile_split` | split 后两个区域引用同一 fdref |
| `test_mappedfile_delete` | 删除区域 → fdref 释放 |
| `test_process_exit_with_pending_vfs` | 进程退出时有未完成的 VFS 请求 |

---

## 7. VM-VFS 交互的完整图景

### 7.1 三种 VFS 交互场景

```
场景 1: mmap 文件
  用户 → VM_MMAP(fd=3) → VM → vfs_request(FDLOOKUP) → VFS
  VFS → VM_VFS_REPLY(dev, ino, size) → VM → mmap_file_cont() → 创建区域

场景 2: 文件缺页
  用户 → #PF → VM → mappedfile_pagefault → NeedVfsIo
  VM → vfs_request(FDIO) → VFS → 读取文件 → VM_VFS_REPLY(data) → VM
  VM → handle_completion() → 映射物理页 → 回复用户

场景 3: 关闭 fd
  用户 → munmap → VM → fdref_deref → refcount==0
  VM → vfs_request(FDCLOSE) → VFS → 关闭 fd → VM_VFS_REPLY → 无回调
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
| VFS 请求 | `VMVFSREQ_FDIO` | `VMVFSREQ_FDIO` |
| 缺页触发 | 用户访问文件映射页 | 用户访问代码/数据段 |
| memtype | `MappedFile` | `AnonymousMemory`（加载后） |
| CoW 行为 | 写入时 CoW → 切换匿名 | 加载时直接写入匿名页 |

exec 的文件加载可以复用 `vfs_request` 框架，但缺页回调不同——exec 加载的数据直接写入匿名页，不需要 MappedFile 的 CoW 逻辑。
