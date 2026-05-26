# 18-vm-mmap: VM_MMAP 服务

> **分类**: VM服务  
> **源码**: `minix3/minix/servers/vm/mmap.c`  
> **说明**: VM 对外提供的内存映射服务，支持文件映射和匿名映射。munmap/map_phys 详见 [19-vm-munmap.md](19-vm-munmap.md)

---

## 1. 概述

VM_MMAP 服务提供内存映射功能，实现 POSIX mmap 系统调用。munmap 和 map_phys 由 [19-vm-munmap](19-vm-munmap.md) 覆盖。

**服务类型**

| 消息类型 | 值 | 说明 | 对应系统调用 |
|---------|-----|------|-------------|
| `VM_MMAP` | `VM_RQ_BASE+10` | 内存映射 | mmap() |

> **注意**: `VM_MUNMAP`、`VM_MAP_PHYS`、`VM_UNMAP_PHYS`、`VM_SHM_UNMAP` 详见 [19-vm-munmap.md](19-vm-munmap.md)。

**映射类型**

| 映射类型 | 标志 | 特点 | 典型用途 |
|---------|------|------|---------|
| 匿名映射 | `MAP_ANONYMOUS` | 不关联文件，内存初始化为 0，私有映射写时复制 | malloc 大块分配 |
| 文件映射 | `MAP_FILE` | 关联文件描述符，支持共享/私有映射，按需从文件加载 | 加载可执行文件、共享库 |

**Minix3 扩展标志**（mman.h:108-124，不属于 POSIX，但 do_mmap/mmap_region 支持）

| 标志 | 值 | 说明 | 典型用途 |
|------|-----|------|---------|
| `MAP_ALIGNMENT_64KB` | `0x01000000` | 要求 64KB 对齐（`MAP_ALIGNED(16)`） | DMA 对齐需求 |
| `MAP_UNINITIALIZED` | `0x040000` | 不清零已分配页面 | 性能优化（已知内容将被覆盖） |
| `MAP_PREALLOC` | `0x080000` | 映射时预分配所有物理页 | 实时/ DPC 场景 |
| `MAP_CONTIG` | `0x100000` | 分配物理连续的页面 | DMA 缓冲区 |
| `MAP_LOWER16M` | `0x200000` | 物理地址在 16MB 以下 | ISA DMA |
| `MAP_LOWER1M` | `0x400000` | 物理地址在 1MB 以下 | 传统设备 |
| `MAP_THIRDPARTY` | `0x800000` | 代表其他进程操作（`forwhom != SELF`） | PM/RS 代理映射 |

> **注**：`MAP_ALIGNMENT_64KB` 在 `mmap_region()`（mmap.c:45）中映射为 `VR_PHYS64K`。`MAP_ALIGNED(n)` 系列宏（mman.h:105-112）还定义了 16MB/4GB/1TB/256TB/64PB 对齐，但 `mmap_region` 仅处理 `MAP_ALIGNMENT_64KB`。

**与 Minix3 的对应关系**

```c
/* minix3/minix/include/minix/com.h */
#define VM_RQ_BASE      0xC00
#define VM_MMAP         (VM_RQ_BASE+10)   // mmap 请求
```

**Minix3 消息处理**

```c
/* minix3/minix/servers/vm/main.c */
static struct callmap vm_callmap[] = {
    // ...
    CALLMAP(VM_MMAP, do_mmap),       // mmap 处理
    // ...
};
```

**地址空间布局**

> **Minix3 (32位)**: mmap 区域位于 `VM_MMAPBASE` 到 `VM_MMAPTOP` 之间。在 `_MINIX_MAGIC` 构建下，`VM_MMAPTOP = VM_STACKTOP - DEFAULT_STACK_LIMIT`，`VM_MMAPBASE = VM_MMAPTOP / 2`；否则 `VM_MMAPTOP = VM_DATATOP`，`VM_MMAPBASE = VM_PAGE_SIZE`。这些值在运行时确定，不是固定常量。
>
> **minix-rs (64位)**: 64 位地址空间远大于 32 位，mmap 区域范围需要重新规划，利用更大的地址空间。

进程地址空间从低到高依次为：text/data/bss → heap (brk) → mmap 区域 → stack。mmap 区域是本服务管理的核心范围。

---

## 2. C 源码分析

### 2.1 IPC 接口说明

#### 2.1.1 调用者

VM_MMAP 的调用者包括：

**1. 用户进程（直接调用）**

```c
/* minix3/minix/lib/libc/sys/mmap.c */

void *mmap(void *addr, size_t len, int prot, int flags,
    int fd, off_t offset)
{
    return minix_mmap_for(SELF, addr, len, prot, flags, fd, offset);
}
```

用户进程通过 libc 的 mmap() 调用 minix_mmap_for(SELF, ...)，后者向 VM 发送 VM_MMAP 请求。若 forwhom != SELF，则自动附加 MAP_THIRDPARTY 标志。

**2. VFS（文件映射）**

```c
/* VFS 在处理文件映射时调用 */
int minix_vfs_mmap(endpoint_t who, off_t offset, size_t len,
    dev_t dev, ino_t ino, int fd, u32_t vaddr, u16_t clearend,
    u16_t flags);
```

文件映射需要 VFS 和 VM 协作：
1. 用户调用 mmap() → VM 收到 VM_MMAP
2. VM 向 VFS 发送 VMVFSREQ_FDLOOKUP 请求
3. VFS 返回文件信息（fd, dev, ino, size_pages）
4. VM 回调 mmap_file_cont 完成映射，解除进程阻塞

**3. RS（系统服务）**

```c
/* minix3/minix/servers/vm/mmap.c */

/* RS and VFS can do slightly more special mmap() things */
if(m->m_source == VFS_PROC_NR || m->m_source == RS_PROC_NR)
    execpriv = 1;
```

RS 和 VFS 拥有特权，可以执行特殊映射操作（如 MAP_UNINITIALIZED）。

**调用流程**

- **匿名映射**: 用户进程 → VM_MMAP → VM → 直接创建匿名区域 → 返回映射地址
- **文件映射**: 用户进程 → VM_MMAP → VM → VMVFSREQ_FDLOOKUP → VFS → 返回文件信息 → VM 创建文件区域 → 返回映射地址

#### 2.1.2 请求参数

**消息结构**

```c
/* minix3/minix/include/minix/ipc.h */

typedef struct {
    off_t offset;        // 文件偏移
    void *addr;          // 请求的映射地址（提示）
    size_t len;          // 映射长度
    int prot;            // 保护标志
    int flags;           // 映射标志
    int fd;              // 文件描述符（-1 表示匿名映射）
    endpoint_t forwhom;  // 目标进程
    void *retaddr;       // 返回的映射地址
    u32_t padding[5];
} mess_mmap;
```

**参数说明**

| 参数 | 类型 | 说明 |
|------|------|------|
| `addr` | `void*` | 请求的映射地址，NULL 表示由系统选择 |
| `len` | `size_t` | 映射长度（字节），会被页对齐 |
| `prot` | `int` | 保护标志 |
| `flags` | `int` | 映射标志 |
| `fd` | `int` | 文件描述符，-1 表示匿名映射 |
| `offset` | `off_t` | 文件偏移，必须是页大小的倍数 |

**保护标志 (prot)**

```c
/* minix3/sys/sys/mman.h */

#define PROT_NONE   0x00    // 无权限
#define PROT_READ   0x01    // 可读
#define PROT_WRITE  0x02    // 可写
#define PROT_EXEC   0x04    // 可执行
```

**映射标志 (flags)**

```c
/* 共享类型（必须指定其一） */
#define MAP_SHARED    0x0001    // 共享映射
#define MAP_PRIVATE   0x0002    // 私有映射（写时复制）

/* 其他标志 */
#define MAP_FIXED     0x0010    // 必须使用指定地址
#define MAP_ANONYMOUS 0x1000    // 匿名映射（不关联文件）
#define MAP_ANON      MAP_ANONYMOUS  // 别名（mman.h:98，源码 mmap.c:232 使用此名）

/* Minix 特有标志 */
#define MAP_UNINITIALIZED 0x040000  // 不清零内存（特权）
#define MAP_PREALLOC      0x080000  // 预分配物理内存
#define MAP_CONTIG        0x100000  // 连续物理内存
#define MAP_LOWER16M      0x200000  // 物理地址低于 16MB
#define MAP_LOWER1M       0x400000  // 物理地址低于 1MB
#define MAP_THIRDPARTY    0x800000  // 代表其他进程映射
```

**标志组合示例**

```c
/* 匿名私有映射（malloc 大块） */
void *mem = mmap(NULL, size, PROT_READ|PROT_WRITE,
                 MAP_PRIVATE|MAP_ANONYMOUS, -1, 0);

/* 文件共享映射 */
void *data = mmap(NULL, size, PROT_READ|PROT_WRITE,
                  MAP_SHARED, fd, 0);

/* 固定地址映射 */
void *fixed = mmap((void*)0x400000, size, PROT_READ,
                   MAP_FIXED|MAP_PRIVATE, fd, 0);
```

#### 2.1.3 返回结果

**成功返回**

```c
/* 成功时返回映射地址 */
m->m_mmap.retaddr = (void *) vr->vaddr;
return OK;
```

返回值通过 `m_mmap.retaddr` 字段传递，是映射区域的起始地址。

**错误返回**

```c
/* minix3/minix/servers/vm/mmap.c */

int do_mmap(message *m)
{
    // ...

    /* "SUSv3 specifies that mmap() should fail if length is 0" */
    if(len <= 0) {
        return EINVAL;
    }

    // ...

    if(!(vr = mmap_region(...))) {
        return ENOMEM;
    }

    // ...
}
```

**错误码**

| 错误码 | 说明 |
|--------|------|
| `EINVAL` | 参数无效（len=0、flags 无效、offset 未对齐） |
| `ENOMEM` | 内存不足或地址空间不足 |
| `EPERM` | 权限不足（如 MAP_THIRDPARTY 无特权） |
| `ESRCH` | 目标进程不存在（MAP_THIRDPARTY） |
| `ENXIO` | 文件映射被禁用或 VFS 请求失败 |
| `EFAULT` | 地址无效（MAP_FIXED 且地址不可用） |

**返回值处理**

```c
/* minix3/minix/lib/libc/sys/mmap.c */

void *mmap(void *addr, size_t len, int prot, int flags,
    int fd, off_t offset)
{
    // ...
    r = _syscall(VM_PROC_NR, VM_MMAP, &m);

    if(r != OK) {
        return MAP_FAILED;  // 错误时返回 MAP_FAILED
    }

    return m.m_mmap.retaddr;  // 成功返回映射地址
}
```

**MAP_FAILED**

```c
/* minix3/sys/sys/mman.h */
#define MAP_FAILED  ((void *) -1)   // mmap 失败时的返回值
```

### 2.2 do_mmap - 主处理函数

do_mmap 是 mmap 系统调用的核心处理函数。

**函数原型**

```c
/* minix3/minix/servers/vm/mmap.c */

int do_mmap(message *m)
```

**处理流程**

```c
int do_mmap(message *m)
{
    int r, n;
    struct vmproc *vmp;
    vir_bytes addr = (vir_bytes) m->m_mmap.addr;
    struct vir_region *vr = NULL;
    int execpriv = 0;
    size_t len = (vir_bytes) m->m_mmap.len;

    /* 1. 检查特权 */
    if(m->m_source == VFS_PROC_NR || m->m_source == RS_PROC_NR)
        execpriv = 1;

    /* 2. 确定目标进程 */
    if(m->m_mmap.flags & MAP_THIRDPARTY) {
        if(!execpriv) return EPERM;
        if((r=vm_isokendpt(m->m_mmap.forwhom, &n)) != OK)
            return ESRCH;
    } else {
        if((r=vm_isokendpt(m->m_source, &n)) != OK) {
            panic("do_mmap: message from strange source: %d",
                m->m_source);
        }
    }
    vmp = &vmproc[n];

    /* 3. 验证长度 */
    if(len <= 0) {
        return EINVAL;
    }

    /* 4. 根据映射类型处理 */
    if(m->m_mmap.fd == -1 || (m->m_mmap.flags & MAP_ANON)) {
        /* 匿名映射 */
        mem_type_t *mt = NULL;

        if(m->m_mmap.fd != -1) {
            return EINVAL;
        }

        if((m->m_mmap.flags & (MAP_CONTIG|MAP_PREALLOC)) == MAP_CONTIG) {
            return EINVAL;
        }

        if(m->m_mmap.flags & MAP_CONTIG) {
            mt = &mem_type_anon_contig;
        } else {
            mt = &mem_type_anon;
        }

        if(!(vr = mmap_region(vmp, addr, m->m_mmap.flags, len,
            VR_WRITABLE | VR_ANON, mt, execpriv))) {
            return ENOMEM;
        }
    } else {
        /* 文件映射 - 需要 VFS 协作 */
        if(!enable_filemap) return ENXIO;

        if((m->m_mmap.flags & MAP_SHARED) && 
           (m->m_mmap.prot & PROT_WRITE)) {
            return ENXIO;
        }

        if(vfs_request(VMVFSREQ_FDLOOKUP, m->m_mmap.fd, vmp, 0, 0,
            mmap_file_cont, NULL, m, sizeof(*m)) != OK) {
            return ENXIO;
        }

        return SUSPEND;  // 等待 VFS 回复
    }

    /* 5. 返回映射地址 */
    m->m_mmap.retaddr = (void *) vr->vaddr;

    return OK;
}
```

**流程概述**

1. 检查特权：VFS/RS 拥有 execpriv
2. 确定目标进程：MAP_THIRDPARTY 时从 forwhom 获取，否则从 m_source 获取
3. 验证长度：len <= 0 返回 EINVAL
4. 根据映射类型处理：
   - 匿名映射（fd == -1 或 MAP_ANON）：选择 mem_type_anon 或 mem_type_anon_contig，调用 mmap_region 创建区域
   - 文件映射：检查 enable_filemap，拒绝可写 MAP_SHARED，通过 vfs_request 异步请求 VFS，返回 SUSPEND
5. 返回映射地址：m->m_mmap.retaddr = vr->vaddr

### 2.3 地址选择

#### 2.3.1 固定地址

MAP_FIXED 标志要求映射必须发生在指定地址。

**处理逻辑**

```c
/* minix3/minix/servers/vm/mmap.c */

static struct vir_region *mmap_region(struct vmproc *vmp, vir_bytes addr,
    u32_t vmm_flags, size_t len, u32_t vrflags,
    mem_type_t *mt, int execpriv)
{
    // ...

    if (addr && (vmm_flags & MAP_FIXED)) {
        /* MAP_FIXED: 先解除该地址范围的现有映射 */
        int r = map_unmap_range(vmp, addr, len);
        if(r != OK) {
            printf("mmap_region: map_unmap_range failed (%d)\n", r);
            return NULL;
        }
    }

    if (addr || (vmm_flags & MAP_FIXED)) {
        /* 尝试在指定地址创建映射 */
        vr = map_page_region(vmp, addr, 0, len, vrflags, mfflags, mt);
        if(!vr && (vmm_flags & MAP_FIXED))
            return NULL;  /* MAP_FIXED 失败则返回错误 */
    }

    // ...
}
```

**MAP_FIXED 行为**

| 情况 | 行为 |
|------|------|
| 地址可用 | 在指定地址创建映射 |
| 地址被占用 | 先解除现有映射，再创建新映射 |
| 地址无效 | 返回 NULL（失败） |

**注意事项**

1. MAP_FIXED 会覆盖现有映射，可能导致数据丢失
2. 地址必须是页对齐的
3. 如果指定地址无法使用，MAP_FIXED 会失败而不是选择其他地址

#### 2.3.2 自动选择

当没有指定地址或 MAP_FIXED 失败时，系统自动选择映射地址。

**自动选择逻辑**

```c
/* minix3/minix/servers/vm/mmap.c */

if (!vr) {
    /* 没有指定地址或指定地址已被占用 */
    vr = map_page_region(vmp, VM_MMAPBASE, VM_MMAPTOP, len,
        vrflags, mfflags, mt);
}
```

**地址范围**

```c
/* minix3/minix/servers/vm/vm.h */

/* VM_MMAPBASE 和 VM_MMAPTOP 不是固定常量，取决于构建配置和运行时值 */
#ifdef _MINIX_MAGIC
#define VM_MMAPTOP    (VM_STACKTOP - DEFAULT_STACK_LIMIT)
#define VM_MMAPBASE   (VM_MMAPTOP / 2)
#else
#define VM_MMAPTOP    VM_DATATOP    /* = kernel_boot_info.user_end */
#define VM_MMAPBASE   VM_PAGE_SIZE  /* = 4096 */
#endif
```

> **32位/64位差异**: Minix3 的 mmap 区域在非 `_MINIX_MAGIC` 构建下从 `VM_PAGE_SIZE` 开始到 `VM_DATATOP`，范围由运行时确定。minix-rs 使用 64 位地址空间，mmap 区域范围需要重新规划以利用更大的地址空间。

**查找算法**

map_page_region 调用 region_find_slot 在 [minv, maxv) 范围内查找足够大的空闲区域。查找通过 AVL 树遍历实现，找到第一个能容纳请求长度的空洞即返回。

**map_page_region 实现**

```c
/* minix3/minix/servers/vm/region.c:463 */

struct vir_region *map_page_region(struct vmproc *vmp,
    vir_bytes minv, vir_bytes maxv, vir_bytes length,
    u32_t flags, int mapflags, mem_type_t *memtype)
{
    struct vir_region *newregion;
    vir_bytes startv;

    assert(!(length % VM_PAGE_SIZE));

    startv = region_find_slot(vmp, minv, maxv, length);
    if (startv == SLOT_FAIL)
        return NULL;

    if(!(newregion = region_new(vmp, startv, length, flags, memtype))) {
        printf("VM: map_page_region: allocating region failed\n");
        return NULL;
    }

    if(newregion->def_memtype->ev_new) {
        if(newregion->def_memtype->ev_new(newregion) != OK) {
            return NULL;
        }
    }

    if(mapflags & MF_PREALLOC) {
        if(map_handle_memory(vmp, newregion, 0, length, 1,
            NULL, 0, 0) != OK) {
            map_free(newregion);
            return NULL;
        }
    }

    /* ... */
}
```

### 2.4 区域创建

#### 2.4.1 创建 vir_region

根据映射类型创建不同的虚拟区域。

**匿名映射**

```c
/* minix3/minix/servers/vm/mmap.c */

if(m->m_mmap.fd == -1 || (m->m_mmap.flags & MAP_ANON)) {
    mem_type_t *mt = NULL;

    if(m->m_mmap.flags & MAP_CONTIG) {
        mt = &mem_type_anon_contig;  /* 连续物理内存 */
    } else {
        mt = &mem_type_anon;         /* 普通匿名内存 */
    }

    if(!(vr = mmap_region(vmp, addr, m->m_mmap.flags, len,
        VR_WRITABLE | VR_ANON, mt, execpriv))) {
        return ENOMEM;
    }
}
```

**文件映射**

```c
/* 文件映射需要 VFS 协作 */
if(vfs_request(VMVFSREQ_FDLOOKUP, m->m_mmap.fd, vmp, 0, 0,
    mmap_file_cont, NULL, m, sizeof(*m)) != OK) {
    return ENXIO;
}
return SUSPEND;  /* 等待 VFS 回复 */
```

> **物理内存映射**（`do_map_phys`）使用 `VR_DIRECT | VR_WRITABLE` 和 `mem_type_directphys` 创建区域，详见 [19-vm-munmap §2.7](19-vm-munmap.md#27-do_map_phys--物理内存映射)。

#### 2.4.2 设置 mem_type

mem_type 决定了内存的分配和访问方式。

**mem_type 结构体定义**

```c
/* minix3/minix/servers/vm/memtype.h:12 */

typedef struct mem_type {
    const char *name;
    int (*ev_new)(struct vir_region *region);
    void (*ev_delete)(struct vir_region *region);
    int (*ev_reference)(struct phys_region *pr, struct phys_region *newpr);
    int (*ev_unreference)(struct phys_region *pr);
    int (*ev_pagefault)(struct vmproc *vmp, struct vir_region *region,
         struct phys_region *ph, int write, vfs_callback_t cb, void *state,
         int len, int *io);
    int (*ev_resize)(struct vmproc *vmp, struct vir_region *vr, vir_bytes len);
    void (*ev_split)(struct vmproc *vmp, struct vir_region *vr,
            struct vir_region *r1, struct vir_region *r2);
    int (*writable)(struct phys_region *pr);
    int (*ev_sanitycheck)(struct phys_region *pr, const char *file, int line);
    int (*ev_copy)(struct vir_region *vr, struct vir_region *newvr);
    int (*ev_lowshrink)(struct vir_region *vr, vir_bytes len);
    u32_t (*regionid)(struct vir_region *vr);
    int (*refcount)(struct vir_region *vr);
    int (*pt_flags)(struct vir_region *vr);
} mem_type_t;
```

**各 mem_type 实例**

```c
/* minix3/minix/servers/vm/mem_anon.c:34 */
struct mem_type mem_type_anon = {
    .name = "anonymous memory",
    .ev_unreference = anon_unreference,
    .ev_pagefault = anon_pagefault,
    .ev_resize = anon_resize,
    .ev_sanitycheck = anon_sanitycheck,
    .ev_lowshrink = anon_lowshrink,
    .ev_split = anon_split,
    .regionid = anon_regionid,
    .writable = anon_writable,
    .refcount = anon_refcount,
    .pt_flags = anon_pt_flags,
};

/* minix3/minix/servers/vm/mem_file.c:30 */
struct mem_type mem_type_mappedfile = {
    .name = "file-mapped memory",
    .ev_unreference = mappedfile_unreference,
    .ev_pagefault = mappedfile_pagefault,
    .ev_sanitycheck = mappedfile_sanitycheck,
    .ev_copy = mappedfile_copy,
    .writable = mappedfile_writable,
    .ev_split = mappedfile_split,
    .ev_lowshrink = mappedfile_lowshrink,
    .ev_delete = mappedfile_delete,
    .pt_flags = mappedfile_pt_flags,
};

/* mem_type_directphys 定义见 19-vm-munmap §2.8 */
```

**类型选择**

| 映射类型 | mem_type | 特点 |
|---------|----------|------|
| 匿名映射 | `mem_type_anon` | 按需分配，写时复制 |
| 连续匿名 | `mem_type_anon_contig` | 物理连续，DMA 用 |
| 文件映射 | `mem_type_mappedfile` | 按需从文件加载 |
| 物理映射 | `mem_type_directphys` | 直接映射物理地址 |
| 页缓存 | `mem_type_cache` | 磁盘缓存页，按需从块设备加载 |
| 共享内存 | `mem_type_shared` | 进程间共享，由 VM_REMAP 创建 |

> **注**：`mem_type_cache` 和 `mem_type_shared` 不在 mmap/munmap 的直接路径中使用。`mem_type_cache` 用于 VM 页缓存（参见 [25-page-cache.md](25-page-cache.md)），`mem_type_shared` 用于 `VM_REMAP`/`VM_REMAP_RO` 共享内存映射（参见 `do_remap`，mmap.c:366）。Minix3 共定义 6 种 mem_type（glo.h:37-42）。每种 mem_type 的字段级定义和 Rust trait 对应关系见 [12-memtype.md](12-memtype.md)。

**区域标志**

```c
/* minix3/minix/servers/vm/region.h:69 */

/* Mapping flags: */
#define VR_WRITABLE      0x001   /* Process may write here. */
#define VR_PHYS64K       0x004   /* Physical memory must be 64k aligned. */
#define VR_LOWER16MB     0x008
#define VR_LOWER1MB      0x010
#define VR_SHARED        0x040
#define VR_UNINITIALIZED 0x080   /* Do not clear after allocation */

/* Mapping type: */
#define VR_ANON          0x100   /* Memory to be cleared and allocated */
#define VR_DIRECT        0x200   /* Mapped, but not managed by VM */
#define VR_PREALLOC_MAP  0x400   /* Preallocated map. */
```

> **注**：`VR_*` 标志在 region.h 中定义为权威来源。本文档按 mmap 路径所需列出子集；完整的标志语义、组合使用、以及与 `VrFlags` bitflags 的 Rust 对应关系见 [11-region-mapping.md](11-region-mapping.md)。

### 2.5 文件映射

#### 2.5.1 VFS 交互

文件映射需要 VFS 提供文件信息。

**交互流程**

```c
/* minix3/minix/servers/vm/mmap.c */

/* VM 向 VFS 发送请求 */
if(vfs_request(VMVFSREQ_FDLOOKUP, m->m_mmap.fd, vmp, 0, 0,
    mmap_file_cont, NULL, m, sizeof(*m)) != OK) {
    return ENXIO;
}
return SUSPEND;  /* 等待 VFS 回复 */
```

**VFS 返回的信息**

```c
/* VFS 回复消息 */
replymsg->VMV_RESULT     /* 操作结果 */
replymsg->VMV_FD         /* 文件描述符 */
replymsg->VMV_INO        /* inode 号 */
replymsg->VMV_DEV        /* 设备号 */
replymsg->VMV_SIZE_PAGES /* 文件大小（页数） */
```

**交互流程**

1. VM 收到 VM_MMAP 请求（文件映射）
2. VM 调用 vfs_request(VMVFSREQ_FDLOOKUP, fd, ...) 发送异步请求给 VFS
3. VM 返回 SUSPEND，进程阻塞
4. VFS 查找文件信息后回复
5. VM 回调 mmap_file_cont 处理 VFS 回复
6. mmap_file_cont 调用 mmap_file 创建文件映射区域
7. VM 通过 ipc_send 解除进程阻塞

**VFS 回调处理**

```c
/* minix3/minix/servers/vm/mmap.c:160 */

static void mmap_file_cont(struct vmproc *vmp, message *replymsg, void *cbarg,
    void *origmsg_v)
{
    message *origmsg = (message *) origmsg_v;
    message mmap_reply;
    int result;
    int writable = 0;
    vir_bytes v = (vir_bytes) MAP_FAILED;

    if(origmsg->m_mmap.prot & PROT_WRITE)
        writable = 1;

    if(replymsg->VMV_RESULT != OK) {
        result = replymsg->VMV_RESULT;
    } else {
        /* Finish mmap */
        result = mmap_file(vmp, replymsg->VMV_FD, origmsg->m_mmap.offset,
            origmsg->m_mmap.flags, 
            replymsg->VMV_INO, replymsg->VMV_DEV,
            (u64_t) replymsg->VMV_SIZE_PAGES*PAGE_SIZE,
            (vir_bytes) origmsg->m_mmap.addr,
            origmsg->m_mmap.len, &v, 0, writable, 1);
    }

    /* Unblock requesting process. */
    memset(&mmap_reply, 0, sizeof(mmap_reply));
    mmap_reply.m_type = result;
    mmap_reply.m_mmap.retaddr = (void *) v;

    if(ipc_send(vmp->vm_endpoint, &mmap_reply) != OK)
        panic("VM: mmap_file_cont: ipc_send() failed");
}
```

#### 2.5.2 VFS 主动映射 (do_vfs_mmap)

与 `do_mmap`（用户进程发起）不同，`do_vfs_mmap` 是 VFS 主动请求 VM 创建文件映射的入口，用于 VFS 自身将文件内容映射到进程地址空间（如 `ld.so` 加载共享库）。

**消息号**

```c
/* minix3/minix/include/minix/com.h:762 */
#define VM_VFS_MMAP  (VM_RQ_BASE+46)
```

**请求消息**

```c
/* minix3/minix/include/minix/ipc.h:2367 */
typedef struct {
    off_t       offset;     /* 文件偏移 */
    dev_t       dev;        /* 设备号 */
    ino_t       ino;        /* inode 号 */
    endpoint_t  who;        /* 目标进程 */
    u32_t       vaddr;      /* 虚拟地址（MAP_FIXED） */
    u32_t       len;        /* 映射长度 */
    u32_t       flags;      /* 映射标志 */
    u32_t       fd;         /* 文件描述符 */
    u16_t       clearend;   /* 清除末端页数 */
} mess_vm_vfs_mmap;
```

**处理流程**

```c
/* minix3/minix/servers/vm/mmap.c:135 */

int do_vfs_mmap(message *m)
{
    vir_bytes v;
    struct vmproc *vmp;
    int r, n;
    u16_t clearend, flags = 0;

    if(!enable_filemap) return ENXIO;

    clearend = m->m_vm_vfs_mmap.clearend;
    flags = m->m_vm_vfs_mmap.flags;

    if((r=vm_isokendpt(m->m_vm_vfs_mmap.who, &n)) != OK)
        panic("bad ep %d from vfs", m->m_vm_vfs_mmap.who);
    vmp = &vmproc[n];

    return mmap_file(vmp, m->m_vm_vfs_mmap.fd, m->m_vm_vfs_mmap.offset,
        MAP_PRIVATE | MAP_FIXED,
        m->m_vm_vfs_mmap.ino, m->m_vm_vfs_mmap.dev,
        (u64_t) LONG_MAX * VM_PAGE_SIZE,
        m->m_vm_vfs_mmap.vaddr, m->m_vm_vfs_mmap.len, &v,
        clearend, flags, 0);
}
```

**与 do_mmap 的关键差异**

| 方面 | do_mmap | do_vfs_mmap |
|------|---------|-------------|
| 调用者 | 用户进程 | VFS |
| 消息号 | VM_MMAP (VM_RQ_BASE+10) | VM_VFS_MMAP (VM_RQ_BASE+46) |
| 文件信息获取 | 异步（vfs_request → SUSPEND → mmap_file_cont） | 同步（VFS 已提供 ino/dev/offset） |
| 映射标志 | 用户指定 | 强制 `MAP_PRIVATE \| MAP_FIXED` |
| 地址 | 由 region_find_slot 分配 | 由 VFS 指定（vaddr） |
| mayclosefd | 1（可关闭 fd） | 0（不关闭 fd） |

**设计要点**

1. VFS 已拥有文件元数据（ino/dev/offset），无需再向 VFS 查询，因此是同步调用
2. 强制 `MAP_PRIVATE | MAP_FIXED`：VFS 映射总是私有的、地址固定的
3. `enable_filemap` 开关：文件映射功能可全局禁用
4. 文件大小传 `(u64_t) LONG_MAX * VM_PAGE_SIZE`，表示不限制（VFS 已知文件大小）

#### 2.5.3 页缓存

文件映射与文件系统缓存的关系。

**按需加载**

文件映射的缺页处理由 `mappedfile_pagefault` 回调完成（`mem_type_mappedfile.ev_pagefault`）。当写入私有映射页面时，执行写时复制（COW）；当读取未加载页面时，从文件加载。具体实现参见 [12-memtype.md](12-memtype.md)。

**缓存策略**

文件映射使用按需加载：首次访问某页时触发缺页，由 `mappedfile_pagefault` 处理。私有映射（MAP_PRIVATE）的写入触发 COW，修改不影响文件；共享映射（MAP_SHARED）的修改写回文件。

**MAP_SHARED vs MAP_PRIVATE**

| 类型 | 行为 |
|------|------|
| MAP_SHARED | 修改写回文件，与其他映射共享 |
| MAP_PRIVATE | 写时复制，修改不影响文件 |

**Minix3 限制**

```c
/* minix3/minix/servers/vm/mmap.c */

/* Minix3 不支持可写的 MAP_SHARED 文件映射 */
if((m->m_mmap.flags & MAP_SHARED) && (m->m_mmap.prot & PROT_WRITE)) {
    return ENXIO;
```

### 2.6 辅助路径：do_remap / do_get_phys / do_get_refcount

mmap.c 中还包含以下辅助函数，不属于 VM_MMAP/VM_MUNMAP 服务的核心路径：

| 函数 | 位置 | 请求类型 | 功能 | 服务归属 |
|------|------|---------|------|---------|
| `do_remap` | mmap.c:366 | VM_REMAP / VM_REMAP_RO | 共享内存重映射，使用 `mem_type_shared` | VM_REMAP 服务 |
| `do_get_phys` | mmap.c:438 | VM_GET_PHYS | 查询虚拟地址对应的物理地址 | VM_GET_PHYS 服务 |
| `do_get_refcount` | mmap.c:463 | VM_GET_REFCOUNT | 查询物理页引用计数 | VM_GET_REFCOUNT 服务 |

这三个函数与 mmap/munmap 的核心逻辑无交叉依赖，将在各自服务的文档中分析。

---

## 3. Rust 设计决策

> **本章基于 Ch1&Ch2 分析，确定 Rust 实现的设计决策，不描述具体代码。**

### 3.1 模块组织：一服务一文件

**决策**：mmap 相关服务分为两个文件 `mmap.rs` 和 `map_phys.rs`，与已有 `brk.rs`、`munmap.rs`、`fork.rs` 平级。

**依据**（Ch2§2.1, [19-vm-munmap §2.7](19-vm-munmap.md)）：
- `do_mmap` 和 `do_map_phys` 是不同消息类型（VM_MMAP vs VM_MAP_PHYS），处理逻辑差异大
- mmap 需要 VFS 异步交互（§2.5），map_phys 全程同步（见 [19-vm-munmap](19-vm-munmap.md)）
- 与现有 `brk.rs`/`munmap.rs` 模式一致

**不采用**：单一 `VmHandler` 上帝对象 —— 违反现有代码的单文件约定，增加耦合。

### 3.2 IPC 层：minix-types 类型

**决策**：IPC 消息类型定义在 `minix-types/src/ipc/vm.rs`，遵循 `VmXxxIn`/`VmXxxOut` 命名。

| 类型 | 对应 C 结构 | 消息号 | 方向 |
|------|------------|--------|------|
| `VmMmapIn` | `mess_mmap` (ipc.h:1575) | VM_MMAP | PM→VM |
| `VmMmapOut` | `m_mmap.retaddr` | — | VM→PM |
| `VmMapPhysIn` | `mess_lsys_vm_map_phys` (ipc.h:1498) | VM_MAP_PHYS | PM→VM |
| `VmMapPhysOut` | `m_lsys_vm_map_phys.reply` | — | VM→PM |
| `VmVfsMmapIn` | `mess_vm_vfs_mmap` (ipc.h:2367) | VM_VFS_MMAP | VFS→VM |
| `VmReply::Mmap` | — | — | VM→调用者 |
| `VmReply::MapPhys` | — | — | VM→调用者 |
| `VmReply::VfsMmap` | — | — | VM→VFS |

**设计依据**（Ch2§2.1）：
- `VmMmapIn` 字段 1:1 对应 `mess_mmap`，不过度抽象
- `VmVfsMmapIn` 对应 `mess_vm_vfs_mmap`，同步路径无需回调
- 标志位（prot/flags）以原始 `u32` 传递，在业务层转换为 bitflags

### 3.3 标志处理：bitflags 在业务层

**决策**：IPC 层传递原始 `u32`，业务层（`mmap.rs`）转换为类型安全的 bitflags。

**依据**（Ch2§2.1）：
- Minix3 的 `MAP_SHARED`/`PROT_READ` 等定义在 `mman.h` 中（L62-124）
- IPC 消息字段是 `int prot; int flags;`，不做编码转换
- Rust 中 `MmapFlags`/`ProtFlags` 使用 `bitflags::bitflags!`，值必须与 `mman.h` 一致

```rust
// 值必须与 minix3/sys/sys/mman.h 一致
bitflags! {
    struct MmapFlags: u32 {
        const SHARED      = 0x0001;   // mman.h:71
        const PRIVATE     = 0x0002;   // mman.h:72
        const FIXED       = 0x0010;   // mman.h:85
        const ANONYMOUS   = 0x1000;   // mman.h:97
        const CONTIG      = 0x100000;  // mman.h:121
        const PREALLOC    = 0x080000;  // mman.h:120
        const UNINITIALIZED = 0x040000; // mman.h:119
        const LOWER16M    = 0x200000;  // mman.h:122
        const LOWER1M     = 0x400000;  // mman.h:123
        const THIRDPARTY  = 0x800000;  // mman.h:124
        const ALIGNMENT_64KB = 0x01000000; // mman.h: MAP_ALIGNED(16)
    }
}
```

### 3.4 错误处理：每服务独立 enum

**决策**：每个服务定义自己的错误枚举，`to_errno()` 直接映射 Minix3 errno，dispatcher 再映射到共享 `VmError`。

**依据**（Ch2§2.2, [19-vm-munmap §2.2](19-vm-munmap.md)）：
- brk 错误：ENOMEM/ESRCH（§2.2.2）
- mmap 错误：EINVAL/EPERM/ENOMEM/ENXIO（§2.2）
- munmap 错误：EFAULT vs ENOMEM vs 静默成功（见 [19-vm-munmap §2.2](19-vm-munmap.md)）
- 三种服务的错误语义不可合并，每服务独立 enum 是正确的选择

**MmapError → errno 映射**：

| MmapError 变体 | errno | Minix3 来源 |
|---------------|-------|------------|
| `ProcessNotFound` | ESRCH | `vm_isokendpt()` 失败 |
| `InvalidLength` | EINVAL | `len == 0` |
| `InvalidAddress` | EINVAL | MAP_FIXED + addr==0 |
| `InvalidFlags` | EINVAL | MAP_SHARED 和 MAP_PRIVATE 同时/都不设置 |
| `PermissionDenied` | EPERM | `map_perm_check()` 失败 |
| `OutOfMemory` | ENOMEM | `find_slot`/`map_page_region` 失败 |
| `FileMapDisabled` | ENXIO | 文件映射未启用（对应 C `enable_filemap` 检查） |

### 3.5 mem_type 选择

**决策**：根据映射类型选择对应的 `MemType` 实现。

| 映射类型 | mem_type | 实现位置 | 对应 C |
|---------|----------|---------|--------|
| 匿名映射 | `MEM_TYPE_ANON` | 已有 | `mem_type_anon` (mem_anon.c:34) |
| 连续匿名 | `MEM_TYPE_CONTIG_ANON` | 已有 | `mem_type_anon_contig` (mem_anon_contig.c:24) |
| 直接物理映射 | `MEM_TYPE_DIRECT` | 已有 | `mem_type_directphys` (mem_directphys.c:28) |
| 文件映射 | `MEM_TYPE_MAPPED_FILE` | 已有 | `mem_type_mappedfile` (mem_file.c:30) |

**依据**（Ch2§2.4）：C 代码中类型选择逻辑在 `do_mmap`（mmap.c:240-255），基于 `fd == -1`、`MAP_CONTIG` 等标志。

### 3.6 RegionMap 操作复用

**决策**：复用已有 `RegionMap` 方法，不新增专用方法。

| mmap 需求 | RegionMap 方法 | 对应 Minix3 |
|----------|---------------|-------------|
| 地址查找 | `find_slot(minv, maxv, length)` | `region_find_slot()` (region.c:399) |
| 重叠检查 | `find_overlap(start, end)` | `nextvr->vaddr < offset` (break.c) |
| 创建区域 | `insert(region)` | `map_page_region()` (region.c:463) |
| 查找区域 | `find(addr)` / `find_mut(addr)` | `map_lookup()` (region.c:616) |
| 遍历区域 | `iter()` / `iter_mut()` | AVL 树遍历 |

**依据**（Ch2§2.3, [19-vm-munmap §2.2](19-vm-munmap.md)）：Minix3 的区域查找/创建/遍历对应关系。

### 3.7 64 位地址空间

**决策**：mmap 区域使用 64 位地址空间的固定范围。

| 参数 | Minix3 (32位) | minix-rs (64位) |
|------|-------------|----------------|
| mmap_base | 运行时计算（~`VM_MMAPTOP/2`） | 固定 `0x0000_0001_0000_0000` |
| mmap_top | 运行时计算（~`VM_STACKTOP - DEFAULT_STACK_LIMIT`） | 固定 `0x0000_0200_0000_0000` |
| 地址空间 | 32位（4GB） | 48位规范用户空间（256TB） |

**依据**（Ch2§2.3 架构演进标注）：
- 64位空间远大于 32位，不需要运行时动态调整 mmap 范围
- 预留足够的地址空间供 future 扩展
- 最终实现应使 mmap_base/mmap_top 为 `VmProc` 配置字段，以支持不同策略

### 3.8 VFS 异步交互

**决策**：文件映射走 `VfsRequestQueue::FdLookup` 异步路径，匿名映射走同步路径。

**依据**（Ch2§2.5）：
- Minix3 的 `do_mmap` 对文件映射发送 `vfs_request(VMVFSREQ_FDLOOKUP,...)` 后返回 SUSPEND
- 回调 `mmap_file_cont` 等待 VFS 返回文件元数据后调用 `mmap_file`
- 匿名映射（`fd == -1`）不需要 VFS，直接 `mmap_region` 创建

```
用户 mmap(fd!=−1)
  → VM 发送 FdLookup → SUSPEND
  → VFS 返回 {fd, dev, ino, size_pages}
  → mmap_file_cont 回调
    → mmap_file: mmap_region + mappedfile_setfile
    → ipc_send 回复用户进程

用户 mmap(fd==−1)
  → handle_mmap: find_slot + insert(anon_region)
  → 直接回复
```

**VfsReply 扩展**：新增 `fd`/`dev`/`ino_nr`/`size_pages` 字段，对应 VFS 的 FDLOOKUP 回复信息。

### 3.9 VrParam::File 扩展

**决策**：`VrParam::File` 增加 `fdref: Option<FileDescriptorRef>` 字段。

**依据**（Ch2§2.5, memtype.h:12）：
- Minix3 的 `param.file.fdref` 追踪文件映射的文件描述符引用（mem_file.c）
- Rust 中对应 `FileDescriptorRef { fd, dev, ino, may_close }`
- `may_close` 在 `do_mmap` 路径为 `true`（用户进程持有 fd），在 `do_vfs_mmap` 路径为 `false`（VFS 持有 fd）

### 3.10 实现优先级

基于复杂性递增，建议实现顺序：

1. **map_phys** — 最简单，同步，无 VFS，验证 RegionMap + VR_DIRECT + mem_type_directphys
2. **mmap 匿名映射** — 同步，验证 MmapFlags/ProtFlags + find_slot + mem_type_anon
3. **munmap 增强** — 验证 split + map_unmap_range + PageFrames 引用计数
4. **mmap 文件映射** — VFS 异步 + mem_type_mappedfile + VrParam::File fdref
5. **do_vfs_mmap** — 同步路径，VFS 已提供文件元数据
6. **do_remap/do_get_phys** 等辅助路径

---

## 4. 实现详解

> **本章描述实际 Rust 代码结构。每个 § 对应一个源文件，段落标注了与 Ch2 C 源码的对应关系。**

### 4.1 模块结构

```
os/servers/vm/src/
├── mmap.rs         # handle_mmap() + handle_vfs_mmap() + MmapFlags/ProtFlags
├── map_phys.rs     # handle_map_phys() + MapPhysError
├── region/
│   ├── vir_region.rs  # VirRegion + VrFlags + VrParam::File + FileDescriptorRef
│   ├── region_map.rs  # RegionMap + find_mut_by_end()
│   └── page_state.rs  # PageFrames + PageSlot + PfnAllocator
├── ipc/
│   └── dispatcher.rs  # dispatch_mmap/dispatch_map_phys/dispatch_vfs_mmap
├── vfs_queue.rs    # VfsRequestQueue + FdLookup + VfsReply(fd/dev/ino)
├── vm_server.rs    # handle_mmap/handle_map_phys 入口
└── lib.rs          # pub(crate) mod mmap; pub(crate) mod map_phys;
```

```
os/libs/minix-types/src/ipc/
└── vm.rs           # VmMmapIn/VmMmapOut/VmMapPhysIn/VmMapPhysOut/VmVfsMmapIn
                    # VmReply::{Mmap, MapPhys, VfsMmap}
```

### 4.2 mmap.rs

**对应 C 源码**：`minix3/minix/servers/vm/mmap.c` 的 `do_mmap()` (L200) + `do_vfs_mmap()` (L135) + `mmap_file()` (L84)

#### 4.2.1 handle_mmap — 用户 mmap 入口

```
handle_mmap(table, page_alloc, frames, &VmMmapIn) → Result<MmapResponse, MmapError>
```

对应 Minix3 `do_mmap(message *m)` (mmap.c:200-278) 的完整流程：

| 步骤 | Rust | Minix3 C | 说明 |
|------|------|----------|------|
| 1. 提取标志 | `MmapFlags::from_bits_truncate()` | `m->m_mmap.flags` | 原始 u32 → bitflags |
| 2. 参数验证 | `length==0 → InvalidLength` | `len<=0 → EINVAL` | |
| 3. 标志验证 | `!flags.is_valid() → InvalidFlags` | SHARED/PRIVATE 互斥检查 | |
| 4. 目标进程 | `vm_isokendpt()` | `vm_isokendpt()` | |
| 5. 地址对齐 | `roundup(len, PAGE_SIZE)` | `len += offset; roundup(len)` | |
| 6. 地址选择 | `find_slot(mmap_base,mmap_top,len)` | `region_find_slot()` | MAP_FIXED → 直接使用; hint→尝试后 fallback |
| 7. mem_type | `MEM_TYPE_ANON` 或 `MEM_TYPE_MAPPED_FILE` | `&mem_type_anon` 或 `&mem_type_mappedfile` | |
| 8. 创建区域 | `insert(VirRegion)` | `map_page_region()` | |
| 9. 文件路径 | 返回 `FileMapDisabled`（Phase 4） | `vfs_request` + SUSPEND | |

**匿名映射路径**（同步，无需 VFS）：

```rust
fn handle_mmap(..., request: &VmMmapIn) -> Result<MmapResponse, MmapError> {
    let flags = MmapFlags::from_bits_truncate(request.flags);
    let prot = ProtFlags::from_bits_truncate(request.prot);

    // 1-2. 参数验证 + 目标进程
    if request.length.0 == 0 { return Err(InvalidLength); }
    if !flags.is_valid() { return Err(InvalidFlags); }
    let mut active = table.get_active(table.vm_isokendpt(request.forwhom)?)?
        .ok_or(ProcessNotFound)?;

    // 3. 页对齐
    let aligned_len = roundup(request.length, PAGE_SIZE);

    // 4. 地址选择：MAP_FIXED → 直接用；hint → 尝试后 fallback 全范围搜索
    let vaddr = match (flags.contains(FIXED), request.addr.0 != 0) {
        (true, _) if request.addr.0 == 0 => return Err(InvalidAddress),
        (true, _) => request.addr,
        (false, true) => find_slot(request.addr, MMAP_TOP, aligned_len)
            .unwrap_or_else(|| find_slot(MMAP_BASE, MMAP_TOP, aligned_len))
            .unwrap_or(VirBytes(0)),
        (false, false) => find_slot(MMAP_BASE, MMAP_TOP, aligned_len)
            .unwrap_or(VirBytes(0)),
    };

    // 5. mem_type
    let mt = if flags.contains(ANONYMOUS) || request.fd == -1 {
        if flags.contains(CONTIG) { &MEM_TYPE_CONTIG_ANON } else { &MEM_TYPE_ANON }
    } else {
        return Err(FileMapDisabled);
    };

    // 6. 创建区域: VirRegion::with_memtype + regions_mut().insert + add_total
    let region = VirRegion::with_memtype(vaddr, aligned_len, prot.to_vr_flags(flags), mt);
    active.regions_mut().insert(region);
    active.add_total(aligned_len);
    Ok(MmapResponse { mapped_addr: vaddr })
}
```

#### 4.2.2 handle_vfs_mmap — VFS 主动映射

```
handle_vfs_mmap(table, page_alloc, frames, &VmVfsMmapIn) → Result<MmapResponse, MmapError>
```

对应 Minix3 `do_vfs_mmap(message *m)` (mmap.c:135-158)：

| 步骤 | Rust | Minix3 C | 差异 |
|------|------|----------|------|
| 启用检查 | 无 | `!enable_filemap → ENXIO` | Rust 在 handle_mmap 层检查 |
| 目标进程 | `vm_isokendpt(who)` | `vm_isokendpt(who)` | 同 |
| 标志 | 忽略传入的 flags，强制 `VrFlags::ANON` | 强制 `MAP_PRIVATE\|MAP_FIXED` | Rust 直接使用 VFS 传入地址；VFS 映射是按需加载（缺页触发 `mappedfile_pagefault`），不设 `VR_PREALLOC_MAP` |
| 地址 | `request.vaddr` | `m->m_vm_vfs_mmap.vaddr` | 同 |
| mem_type | `MEM_TYPE_MAPPED_FILE` | `mem_type_mappedfile` | 同 |
| mayclosefd | `false` | `0` | VFS 持有 fd，VM 不关闭 |
| clearend | `request.clearend` | `m->m_vm_vfs_mmap.clearend` | 同 |

#### 4.2.3 标志转换

**MmapFlags → VrFlags**：

| MmapFlags | VrFlags | 说明 |
|-----------|---------|------|
| WRITE (prot) | `VR_WRITABLE` | 可写权限 |
| SHARED | `VR_SHARED` | 共享映射 |
| UNINITIALIZED | `VR_UNINITIALIZED` | 不清零 |
| PREALLOC | `VR_PREALLOC_MAP` | 预分配 |
| CONTIG | `VR_PHYS64K` | 物理连续 |
| LOWER16M | `VR_LOWER16MB` | 16MB 以下 |
| LOWER1M | `VR_LOWER1MB` | 1MB 以下 |

#### 4.2.4 MmapFlags 有效性验证

`MAP_SHARED` 和 `MAP_PRIVATE` 必须且只设置其一（对应 Minix3 `mman.h` L71-72）。

### 4.3 map_phys.rs

**对应 C 源码**：`minix3/minix/servers/vm/mmap.c` 的 `do_map_phys()` (L310-365)

```
handle_map_phys(table, page_alloc, frames, target, phys_addr, length) → Result<VirBytes, MapPhysError>
```

| 步骤 | Rust | Minix3 C |
|------|------|----------|
| 1. 长度检查 | `length==0 → InvalidLength` | `len<=0 → EINVAL` |
| 2. 目标进程 | `vm_isokendpt(target)` | `vm_isokendpt(target)` |
| 3. 页对齐 | `offset = phys_addr % PAGE_SIZE; startaddr -= offset; len += offset` | 同 |
| 4. 权限检查 | 由 `map_perm_check` 完成，当前仅允许 TTY/MEM 调用者（依赖内核 `sys_privquery_mem` syscall） | `map_perm_check(m_source,target,startaddr,len)!=OK → EPERM` |
| 5. 地址选择 | `find_slot(mmap_base, mmap_top, aligned_len)` | `map_page_region(VM_MMAPBASE, VM_MMAPTOP, len, ...)` |
| 6. 区域创建 | `VirRegion::with_memtype(vaddr, len, VR_DIRECT\|VR_WRITABLE, MEM_TYPE_DIRECT)` | `VR_DIRECT\|VR_WRITABLE, &mem_type_directphys` |
| 7. 物理地址 | `region.param = VrParam::Direct { phys: startaddr }` | `phys_setphys(vr, startaddr)` |
| 8. 返回值 | `vaddr + offset` | `vr->vaddr + offset` |

### 4.4 基础设施变更

#### 4.4.1 VmReply 扩展（minix-types）

```rust
pub enum VmReply {
    // ... 原有变体 ...
    Mmap(VmMmapOut),      // 新增
    MapPhys(VmMapPhysOut), // 新增
    VfsMmap(VmMmapOut),   // 新增，VFS→VM 路径返回
}
```

#### 4.4.2 dispatcher 新增分发

```rust
impl MessageDispatcher {
    fn dispatch_mmap(...)     // VmMmapIn → mmap::handle_mmap → VmReply::Mmap
    fn dispatch_vfs_mmap(...) // VmVfsMmapIn → mmap::handle_vfs_mmap → VmReply::VfsMmap
    fn dispatch_map_phys(...) // VmMapPhysIn → map_phys::handle_map_phys → VmReply::MapPhys
}
```

#### 4.4.3 VfsRequestQueue 扩展

- 新增 `VfsRequestType::FdLookup` — 对应 Minix3 `VMVFSREQ_FDLOOKUP`
- `VfsReply` 新增字段：`fd`、`dev`、`ino_nr`、`size_pages`

#### 4.4.4 VmServer 新增入口

```rust
impl VmServer {
    pub fn handle_mmap(&mut self, req: VmMmapIn) -> VmReply { ... }
    pub fn handle_map_phys(&mut self, req: VmMapPhysIn) -> VmReply { ... }
}
```

`page_frames` 从 `PageFrames::new(PhysBytes(0))` 零值占位改为 `Option<PageFrames>`，在 `init()` 阶段正式创建。

#### 4.4.5 VrParam::File 扩展

```rust
pub enum VrParam {
    File {
        inited: bool,
        fdref: Option<FileDescriptorRef>,  // ← 新增
        offset: u64,
        clearend: u16,
    },
    // ...
}

pub struct FileDescriptorRef {
    pub fd: i32,
    pub dev: u64,
    pub ino: u64,
    pub may_close: bool,
}
```

对应 Minix3 `param.file.fdref`（mem_file.c），追踪文件映射的文件描述符引用。

#### 4.4.6 RegionMap::find_mut_by_end

新增方法，替代 brk `grow_heap` 中的 `find_less + filter + get_mut` 三步骤：
```rust
fn find_mut_by_end(&mut self, end_addr: VirBytes) -> Option<&mut VirRegion>
```

### 4.5 文件映射异步路径（Phase 4 实现）

文件映射是唯一需要异步处理的 mmap 路径，流程为：

1. `handle_mmap` 检测到 `fd != -1` 时，不直接创建区域
2. 向 `VfsRequestQueue` 发送 `FdLookup` 请求，注册回调
3. 返回一个特殊状态（Dispatcher 不回复调用者）
4. VFS 回复到达时，`handle_vfs_reply` 匹配 `FdLookup` 类型
5. 提取 `reply.fd/dev/ino_nr/size_pages` 构造 `VrParam::File`
6. 调用 `mmap_file` 等价逻辑创建 `MEM_TYPE_MAPPED_FILE` 区域
7. 通过 IPC 直接回复用户进程解除阻塞

当前实现：匿名映射路径完整，文件映射返回 `FileMapDisabled` 错误。

### 4.6 与已有服务的比较

| 方面 | fork | brk | munmap | mmap | map_phys |
|------|------|-----|--------|------|----------|
| VFS 交互 | 无 | 无 | 无 | 有（文件） | 无 |
| mem_type | ANON | ANON | 多种 | ANON/MAPPED_FILE/CONTIG | DIRECT |
| 地址范围 | 复制父进程 | 堆区域 | 任意 | mmap 范围 | mmap 范围 |
| VR_DIRECT | 无 | 无 | 无 | 无 | 有 |
| PageFrames | refcount++ | 分配 | 释放+unmap | 按需分配 | 无需分配 |

---

## 5. 测试要点

> 本节基于 Ch2 错误场景和 Ch3 设计决策，列出测试覆盖要点。

### 5.1 map_phys 测试

- [x] 基本物理映射创建
- [x] 零长度拒绝
- [x] errno 映射正确性
- [ ] 权限检查（`map_perm_check`）
- [ ] 页对齐偏移
- [ ] 地址空间耗尽

### 5.2 mmap 测试

- [x] 匿名映射基本创建
- [x] 零长度拒绝
- [x] 无效标志拒绝（SHARED+PRIVATE 同时设置/都不设置）
- [ ] MAP_FIXED 固定地址
- [ ] hint 地址 fallback
- [ ] MAP_CONTIG 连续物理内存
- [ ] MAP_PREALLOC 预分配
- [ ] 文件映射异步路径（Phase 4）
- [ ] do_vfs_mmap 同步路径（Phase 5）
- [ ] errno 映射正确性

### 5.3 跨服务测试

- [ ] mmap 后 brk 不冲突（区域重叠检查）
- [ ] fork 后子进程 mmap 独立
- [ ] munmap 对 mmap 区域的部分解除
- [ ] exec 后 mmap 区域清除

