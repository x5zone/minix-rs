# 22-vm-queries: VM 查询接口

> **分类**: 模块库
> **源码**: `minix3/minix/servers/vm/utility.c`, `minix3/minix/servers/vm/mmap.c`, `minix3/minix/servers/vm/region.c`
> **说明**: VM 提供的 4 个只读查询服务：内存统计、物理地址查询、引用计数查询、资源使用查询。

---

## 1. 概述

### 1.1 为什么需要查询接口

VM 是内存管理的唯一权威——只有 VM 知道虚拟地址到物理地址的映射、物理页的引用计数、全局内存使用统计。外部组件（PM、调试工具、top 命令）需要查询这些信息，但不能直接访问 VM 的内部数据结构。

4 个查询服务提供了不同粒度的信息：

| 服务 | 请求码 | 查询内容 | 典型调用者 |
|------|--------|---------|-----------|
| VM_INFO | 0xC28 | 内存统计/使用/区域信息 | top、ps、调试工具 |
| VM_GETPHYS | 0xC23 | 虚拟地址对应的物理地址 | 驱动程序、调试 |
| VM_GETREF | 0xC24 | 物理页引用计数 | 调试、内存分析 |
| VM_GETRUSAGE | 0xC2F | 进程资源使用（maxrss/pagefaults） | PM（getrusage 系统调用） |

### 1.2 共同特征

这 4 个服务都是**只读、无副作用**的：
- 不修改 VM 状态
- 不分配/释放内存
- 不触发页错误处理
- 返回值通过消息字段或 `sys_datacopy` 传递

### 1.3 行为规则

1. **VM_INFO**：3 种查询模式（STATS/USAGE/REGION），通过 `what` 字段区分。REGION 模式需要 `sys_datacopy` 将结果复制到调用者地址空间。
2. **VM_GETPHYS**：查找虚拟地址所在区域，返回区域的 `regionid`（物理基地址）。地址必须精确匹配区域起始地址。
3. **VM_GETREF**：查找虚拟地址所在区域，返回区域的 `refcount`。地址必须精确匹配区域起始地址。
4. **VM_GETRUSAGE**：仅 PM 调用时返回真实数据；非 PM 调用直接返回 OK（向后兼容）。修改调用者提供的 `rusage` 结构体中的 `ru_maxrss`/`ru_minflt`/`ru_majflt` 字段。

---

## 2. C 源码分析

### 2.1 IPC 接口定义

```c
// com.h:729 — VM_INFO
#define VM_INFO  (VM_RQ_BASE+40)
// 消息字段：m_lsys_vm_info.what / .ep / .ptr / .count / .next

// VMIW 常量（com.h:731-733）
#define VMIW_STATS   1   // 全局内存统计
#define VMIW_USAGE   2   // 进程内存使用
#define VMIW_REGION  3   // 进程区域列表

// com.h:720 — VM_GETPHYS
#define VM_GETPHYS  (VM_RQ_BASE+35)
// 消息字段：m_lc_vm_getphys.endpt / .addr / .ret_addr

// com.h:722 — VM_GETREF
#define VM_GETREF   (VM_RQ_BASE+36)
// 消息字段：m_lsys_vm_getref.endpt / .addr / .retc

// com.h:764 — VM_GETRUSAGE
#define VM_GETRUSAGE  (VM_RQ_BASE+47)
// 消息字段：m_lsys_vm_rusage.endpt / .addr / .children
```

### 2.2 do_info — 内存信息查询

> 源码位置：`utility.c:100-184`

```c
int do_info(message *m)
{
    struct vm_stats_info vsi;
    struct vm_usage_info vui;
    static struct vm_region_info vri[MAX_VRI_COUNT];
    struct vmproc *vmp;
    vir_bytes addr, size, next, ptr;
    int r, pr, dummy, count, free_pages, largest_contig;

    if (vm_isokendpt(m->m_source, &pr) != OK)
        return EINVAL;
    vmp = &vmproc[pr];

    ptr = (vir_bytes) m->m_lsys_vm_info.ptr;

    switch(m->m_lsys_vm_info.what) {
    case VMIW_STATS:
        vsi.vsi_pagesize = VM_PAGE_SIZE;
        vsi.vsi_total = total_pages;
        memstats(&dummy, &free_pages, &largest_contig);
        vsi.vsi_free = free_pages;
        vsi.vsi_largest = largest_contig;
        get_stats_info(&vsi);
        addr = (vir_bytes) &vsi;
        size = sizeof(vsi);
        break;

    case VMIW_USAGE:
        if(m->m_lsys_vm_info.ep < 0)
            get_usage_info_kernel(&vui);
        else if (vm_isokendpt(m->m_lsys_vm_info.ep, &pr) != OK)
            return EINVAL;
        else get_usage_info(&vmproc[pr], &vui);
        addr = (vir_bytes) &vui;
        size = sizeof(vui);
        break;

    case VMIW_REGION:
        if(m->m_lsys_vm_info.ep == SELF)
            m->m_lsys_vm_info.ep = m->m_source;
        if (vm_isokendpt(m->m_lsys_vm_info.ep, &pr) != OK)
            return EINVAL;
        count = MIN(m->m_lsys_vm_info.count, MAX_VRI_COUNT);
        next = m->m_lsys_vm_info.next;
        count = get_region_info(&vmproc[pr], vri, count, &next);
        m->m_lsys_vm_info.count = count;
        m->m_lsys_vm_info.next = next;
        addr = (vir_bytes) vri;
        size = sizeof(vri[0]) * count;
        break;

    default:
        return EINVAL;
    }

    if (size == 0) return OK;

    // 确保复制目标地址已映射（防止死锁）
    r = handle_memory_once(vmp, ptr, size, 1 /*wrflag*/);
    if (r != OK) return r;

    // 执行数据复制
    return sys_datacopy(SELF, addr,
        (vir_bytes) vmp->vm_endpoint, ptr, size);
}
```

**关键行为**：
1. 验证调用者 endpoint
2. 根据 `what` 选择查询模式
3. STATS：返回页大小、总页数、空闲页数、最大连续块
4. USAGE：返回指定进程（或内核）的内存使用
5. REGION：返回指定进程的区域列表（分页查询，通过 `next` 游标）
6. 复制前先 `handle_memory_once` 确保目标地址已映射（防止 `sys_datacopy` 触发页错误导致死锁）

### 2.3 do_get_phys — 物理地址查询

> 源码位置：`mmap.c:438-456`

```c
int do_get_phys(message *m)
{
    int r, n;
    struct vmproc *vmp;
    endpoint_t target;
    phys_bytes ret;
    vir_bytes addr;

    target = m->m_lc_vm_getphys.endpt;
    addr = (vir_bytes) m->m_lc_vm_getphys.addr;

    if ((r = vm_isokendpt(target, &n)) != OK)
        return EINVAL;

    vmp = &vmproc[n];
    r = map_get_phys(vmp, addr, &ret);
    m->m_lc_vm_getphys.ret_addr = (void *) ret;
    return r;
}
```

底层 `map_get_phys`（region.c:1323）：

```c
int map_get_phys(struct vmproc *vmp, vir_bytes addr, phys_bytes *r)
{
    struct vir_region *vr;

    if (!(vr = map_lookup(vmp, addr, NULL)) ||
        (vr->vaddr != addr))    // 必须精确匹配起始地址
        return EINVAL;

    if (!vr->def_memtype->regionid)   // memtype 必须支持 regionid
        return EINVAL;

    if(r)
        *r = vr->def_memtype->regionid(vr);

    return OK;
}
```

**关键约束**：
- 地址必须精确匹配区域起始地址（`vr->vaddr != addr → EINVAL`）
- 区域的 memtype 必须实现 `regionid` 回调

### 2.4 do_get_refcount — 引用计数查询

> 源码位置：`mmap.c:463-483`

```c
int do_get_refcount(message *m)
{
    int r, n;
    struct vmproc *vmp;
    endpoint_t target;
    u8_t cnt;
    vir_bytes addr;

    target = m->m_lsys_vm_getref.endpt;
    addr = (vir_bytes) m->m_lsys_vm_getref.addr;

    if ((r = vm_isokendpt(target, &n)) != OK)
        return EINVAL;

    vmp = &vmproc[n];
    r = map_get_ref(vmp, addr, &cnt);
    m->m_lsys_vm_getref.retc = cnt;
    return r;
}
```

底层 `map_get_ref`（region.c:1343）：

```c
int map_get_ref(struct vmproc *vmp, vir_bytes addr, u8_t *cnt)
{
    struct vir_region *vr;

    if (!(vr = map_lookup(vmp, addr, NULL)) ||
        (vr->vaddr != addr) || !vr->def_memtype->refcount)
        return EINVAL;

    if (cnt)
        *cnt = vr->def_memtype->refcount(vr);

    return OK;
}
```

**与 `map_get_phys` 的区别**：额外检查 `refcount` 回调是否存在。

### 2.5 do_getrusage — 资源使用查询

> 源码位置：`utility.c:426-470`

```c
int do_getrusage(message *m)
{
    int res, slot;
    struct vmproc *vmp;
    struct rusage r_usage;

    // 非 PM 调用：向后兼容，直接返回 OK
    if (m->m_source != PM_PROC_NR)
        return OK;

    if ((res = vm_isokendpt(m->m_lsys_vm_rusage.endpt, &slot)) != OK)
        return ESRCH;

    vmp = &vmproc[slot];

    // 从 PM 地址空间复制 rusage 结构
    if ((res = sys_datacopy(m->m_source, m->m_lsys_vm_rusage.addr,
        SELF, (vir_bytes) &r_usage, sizeof(r_usage))) < 0)
        return res;

    if (!m->m_lsys_vm_rusage.children) {
        r_usage.ru_maxrss = vmp->vm_total_max / 1024L;  // KB
        r_usage.ru_minflt = vmp->vm_minor_page_fault;
        r_usage.ru_majflt = vmp->vm_major_page_fault;
    }
    // children=true 时：TODO 未实现

    // 复制回 PM
    return sys_datacopy(SELF, (vir_bytes) &r_usage,
        m->m_source, m->m_lsys_vm_rusage.addr, sizeof(r_usage));
}
```

**关键行为**：
1. 非 PM 调用直接返回 OK（不修改 rusage）
2. PM 调用时：从 PM 地址空间复制 `rusage`，修改 3 个字段，复制回去
3. `children=true` 路径未实现（C 源码注释 XXX TODO）
4. 错误码使用 ESRCH（而非 EINVAL），与 VM_INFO/GETPHYS/GETREF 不同

### 2.6 返回结构体

#### vm_stats_info（VMIW_STATS）

| 字段 | 类型 | 含义 |
|------|------|------|
| vsi_pagesize | int | 页大小 |
| vsi_total | int | 总页数 |
| vsi_free | int | 空闲页数 |
| vsi_largest | int | 最大连续空闲块 |

#### vm_usage_info（VMIW_USAGE）

| 字段 | 类型 | 含义 |
|------|------|------|
| vui_total | vir_bytes | 总使用内存 |
| vui_shared | vir_bytes | 共享内存 |
| vui_text | vir_bytes | 代码段 |
| vui_data | vir_bytes | 数据段 |
| vui_stack | vir_bytes | 栈 |

#### vm_region_info（VMIW_REGION）

每个区域一条记录，包含起始地址、长度、权限标志等。

### 2.7 C 源码覆盖完整性

**语义范围**：VM 的 4 个只读查询服务

| 符号 | 类型 | 源码位置 | 在语义范围内? | 文档覆盖? |
|------|------|---------|-------------|-----------|
| do_info | 函数 | utility.c:100 | 已覆盖 | §2.2 |
| do_get_phys | 函数 | mmap.c:438 | 已覆盖 | §2.3 |
| do_get_refcount | 函数 | mmap.c:463 | 已覆盖 | §2.4 |
| do_getrusage | 函数 | utility.c:426 | 已覆盖 | §2.5 |
| map_get_phys | 函数 | region.c:1323 | 已覆盖 | §2.3 |
| map_get_ref | 函数 | region.c:1343 | 已覆盖 | §2.4 |
| get_stats_info | 函数 | utility.c | 已覆盖 | §2.2 |
| get_usage_info | 函数 | utility.c | 已覆盖 | §2.2 |
| get_region_info | 函数 | utility.c | 已覆盖 | §2.2 |
| handle_memory_once | 函数 | utility.c | 已覆盖 | §2.2 |
| VMIW_STATS | 宏 | com.h:731 | 已覆盖 | §2.1 |
| VMIW_USAGE | 宏 | com.h:732 | 已覆盖 | §2.1 |
| VMIW_REGION | 宏 | com.h:733 | 已覆盖 | §2.1 |
| VM_INFO | 宏 | com.h:729 | 已覆盖 | §2.1 |
| VM_GETPHYS | 宏 | com.h:720 | 已覆盖 | §2.1 |
| VM_GETREF | 宏 | com.h:722 | 已覆盖 | §2.1 |
| VM_GETRUSAGE | 宏 | com.h:764 | 已覆盖 | §2.1 |

**覆盖统计**：总符号 17 / 已覆盖 17 / 覆盖率 100%

---

## 3. Rust 设计决策

### 3.1 模块组织

4 个查询服务共享 `query.rs` 文件：

```
os/servers/vm/src/
├── query.rs       # handle_info/get_phys/get_refcount/getrusage
```

### 3.2 查询模式用 enum 而非裸整数

```rust
pub(crate) enum InfoQuery {
    Stats,
    Usage { target: Endpoint },
    Region { target: Endpoint, count: usize, next: usize },
}
```

### 3.3 返回值用结构体而非修改消息

C 源码通过修改消息字段返回结果。Rust 使用明确的返回结构体：

```rust
pub(crate) struct StatsInfo {
    pub page_size: u64,
    pub total_pages: u32,
    pub free_pages: u32,
    pub largest_contiguous: u32,
}

pub(crate) struct UsageInfo {
    pub total: VirBytes,
    pub shared: VirBytes,
    pub text: VirBytes,
    pub data: VirBytes,
    pub stack: VirBytes,
}
```

### 3.4 getrusage 的 PM-only 语义

C 源码中非 PM 调用直接返回 OK（不修改 rusage）。Rust 中用 `Option` 表达：

```rust
pub(crate) enum GetrusageResult {
    Ok,                    // 非 PM 调用，不修改数据
    Data(ResourceUsage),   // PM 调用，返回修改后的数据
}
```

### 3.5 sys_datacopy 的替代

C 源码中 `do_info` 和 `do_getrusage` 使用 `sys_datacopy` 将数据复制到调用者地址空间。Rust 中当前没有等价机制——查询结果通过 IPC 消息返回，不需要跨地址空间复制。

**具体方案**：

1. **Stats / Usage 模式**：返回结构体（`StatsInfo` / `UsageInfo`）直接编码到 IPC 响应消息字段中，无需 `sys_datacopy`。结构体大小远小于 IPC 消息上限，完全可行。

2. **Region 模式**：C 使用 `sys_datacopy` 将 `vm_region_info[]` 数组复制到调用者缓冲区。Rust 替代方案：
   - 固定大小数组 `[RegionInfo; 8]`（对应 C 的 `MAX_VRI_COUNT`），直接编码到 IPC 响应消息。
   - 分页机制：`next` 字段作为游标，调用者多次请求以获取所有区域。
   - 限制：单次最多返回 8 个区域条目。对于区域数 >8 的进程，需要多次 IPC 调用。

3. **getrusage 模式**：C 使用 `sys_datacopy` 将 `vm_rusage` 复制到 PM 提供的缓冲区。Rust 替代方案：将 `RusageInfo` 编码到 IPC 响应消息字段中（字段数与 C 结构体一致），PM 直接从消息中提取。

**与 C 的差异**：C 的 `sys_datacopy` 可以复制任意大小的数据块到调用者地址空间；Rust 的 IPC 消息方案受限于消息大小（约 64 字节有效载荷），因此 Region 模式需要分页。这不会影响功能正确性，因为 Minix3 的 `MAX_VRI_COUNT` 也是 8。

### 3.6 功能依赖与架构分层

| 服务 | 依赖模块 | 架构说明 |
|------|---------|---------|
| VM_GETPHYS | region 模块 (map_get_phys) | 驱动程序需要查询物理地址，直接读取 VirRegion 的 VrParam |
| VM_GETREF | region 模块 + PageFrames | 调试和内存分析，需要 PFN 引用计数查询 |
| VM_INFO (STATS) | VmPageAllocator (memstats) | top/ps 等工具需要，从物理分配器获取空闲页统计 |
| VM_INFO (USAGE) | ActiveProc (total) | 进程级内存使用查询，从 vmproc 读取已分配总量 |
| VM_INFO (REGION) | ActiveProc (regions) | 调试用途，遍历区域列表并序列化为固定大小数组 |
| VM_GETRUSAGE | ActiveProc (fault counters) | PM 的 getrusage 系统调用依赖，读取页错误计数 |

---

## 4. 实现详解

### 4.1 模块结构

```
os/servers/vm/src/query.rs
├── InfoQuery enum
├── StatsInfo / UsageInfo / RegionInfo structs
├── ResourceUsage struct
├── QueryError enum
├── handle_info()
├── handle_get_phys()
├── handle_get_refcount()
├── handle_getrusage()
└── tests
```

### 4.2 handle_get_phys

> 对应 C 源码 `do_get_phys()` (mmap.c:438) + `map_get_phys()` (region.c:1323)

```rust
pub(crate) fn handle_get_phys(
    table: &VmProcTable,
    target: Endpoint,
    addr: VirBytes,
) -> Result<PhysBytes, QueryError> {
    let slot = table.vm_isokendpt(target)
        .map_err(|_| QueryError::ProcessNotFound)?;

    let active = table.get_active(slot)
        .ok_or(QueryError::ProcessNotFound)?;

    let vr = active.regions().find(addr)
        .ok_or(QueryError::NotMapped)?;

    // C: vr->vaddr != addr → EINVAL
    if vr.vaddr() != addr {
        return Err(QueryError::NotMapped);
    }

    // C: vr->def_memtype->regionid(vr)
    vr.phys_base_addr()
        .ok_or(QueryError::NotSupported)
}
```

### 4.3 handle_get_refcount

> 对应 C 源码 `do_get_refcount()` (mmap.c:463) + `map_get_ref()` (region.c:1343)

```rust
pub(crate) fn handle_get_refcount(
    table: &VmProcTable,
    target: Endpoint,
    addr: VirBytes,
) -> Result<u8, QueryError> {
    let slot = table.vm_isokendpt(target)
        .map_err(|_| QueryError::ProcessNotFound)?;

    let active = table.get_active(slot)
        .ok_or(QueryError::ProcessNotFound)?;

    let vr = active.regions().find(addr)
        .ok_or(QueryError::NotMapped)?;

    if vr.vaddr() != addr {
        return Err(QueryError::NotMapped);
    }

    vr.refcount()
        .ok_or(QueryError::NotSupported)
}
```

### 4.4 handle_info

> 对应 C 源码 `do_info()` (utility.c:100)

```rust
pub(crate) fn handle_info(
    table: &VmProcTable,
    page_alloc: &VmPageAllocator,
    query: InfoQuery,
) -> Result<InfoResult, QueryError> {
    match query {
        InfoQuery::Stats => {
            let stats = page_alloc.memstats();
            Ok(InfoResult::Stats(StatsInfo {
                page_size: 4096,
                total_pages: stats.total,
                free_pages: stats.free,
                largest_contiguous: stats.largest,
            }))
        }
        InfoQuery::Usage { target } => {
            let slot = table.vm_isokendpt(target)
                .map_err(|_| QueryError::ProcessNotFound)?;
            let active = table.get_active(slot)
                .ok_or(QueryError::ProcessNotFound)?;
            let usage = active.memory_usage();
            Ok(InfoResult::Usage(usage))
        }
        InfoQuery::Region { target, count, next } => {
            let slot = table.vm_isokendpt(target)
                .map_err(|_| QueryError::ProcessNotFound)?;
            let active = table.get_active(slot)
                .ok_or(QueryError::ProcessNotFound)?;
            let regions = active.region_info(count, next);
            Ok(InfoResult::Region(regions))
        }
    }
}
```

### 4.5 handle_getrusage

> 对应 C 源码 `do_getrusage()` (utility.c:426)

```rust
pub(crate) fn handle_getrusage(
    table: &VmProcTable,
    caller: Endpoint,
    target: Endpoint,
    children: bool,
) -> Result<GetrusageResult, QueryError> {
    // C: 非 PM 调用直接返回 OK
    if caller != Endpoint::PM {
        return Ok(GetrusageResult::Ok);
    }

    let slot = table.vm_isokendpt(target)
        .map_err(|_| QueryError::ProcessNotFound)?;

    let active = table.get_active(slot)
        .ok_or(QueryError::ProcessNotFound)?;

    if !children {
        Ok(GetrusageResult::Data(ResourceUsage {
            max_rss_kb: active.total_max() / 1024,
            minor_faults: active.minor_page_faults(),
            major_faults: active.major_page_faults(),
        }))
    } else {
        // C: XXX TODO — children 路径未实现
        Ok(GetrusageResult::Data(ResourceUsage {
            max_rss_kb: 0,
            minor_faults: 0,
            major_faults: 0,
        }))
    }
}
```

---

## 5. 测试要点

### 5.1 GETPHYS 测试

| 测试 | 验证 |
|------|------|
| 无效 endpoint | 返回 ProcessNotFound |
| 未映射地址 | 返回 NotMapped |
| 非起始地址 | 返回 NotMapped |
| 有效区域起始地址 | 返回物理地址 |

### 5.2 GETREF 测试

| 测试 | 验证 |
|------|------|
| 无效 endpoint | 返回 ProcessNotFound |
| 未映射地址 | 返回 NotMapped |
| 有效区域起始地址 | 返回引用计数 |

### 5.3 INFO 测试

| 测试 | 验证 |
|------|------|
| STATS 查询 | 返回正确的页大小和统计 |
| USAGE 无效 endpoint | 返回 ProcessNotFound |
| REGION 查询 | 返回区域列表 |

### 5.4 GETRUSAGE 测试

| 测试 | 验证 |
|------|------|
| 非 PM 调用 | 返回 Ok（不修改数据） |
| PM 调用 | 返回 ResourceUsage |
| 无效 endpoint | 返回 ProcessNotFound |

### 5.5 错误码映射测试

| 错误 | errno |
|------|-------|
| ProcessNotFound (INFO/GETPHYS/GETREF) | EINVAL |
| ProcessNotFound (GETRUSAGE) | ESRCH |
| NotMapped | EINVAL |
| NotSupported | EINVAL |

---

## 6. 参见

- [04-physical-memory.md](04-physical-memory.md) — 物理内存管理，`memstats()` 的实现
- [10-phys-pagestate.md](10-phys-pagestate.md) — 物理页引用计数
- [11-region-mapping.md](11-region-mapping.md) — 区域映射，`map_lookup()` 的实现
- [12-memtype.md](12-memtype.md) — memtype 的 `regionid`/`refcount` 回调
- [26-vm-init-main.md](26-vm-init-main.md) — VM 初始化，CALLMAP 注册
