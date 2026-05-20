# 19-cow-exec-pagefault: CoW 执行与页错误完整流程

> **分类**: VM服务
> **源码**: `minix3/minix/servers/vm/pb.c`, `region.c`, `pagefaults.c`, `pagetable.c`
> **说明**: 将 14-cow-mechanism 和 15-pagefault 的设计落地为可运行的实现——兑现 fork 留下的 CoW 悬念

---

## 1. 概述

### 1.1 本文档的定位

14-cow-mechanism 描述了 CoW 的**机制设计**：引用计数、页表只读、mem_cow 流程。15-pagefault 描述了页错误的**处理框架**：错误类型、状态机、MemoryType 集成。两篇文档都有 Ch4（实现详解），但那些代码是**设计级伪代码**——展示了"应该怎么写"，但没有和现有代码库整合。

本文档是**实现文档**：把 15/16 的设计落地为可编译、可运行的 Rust 代码，解决以下问题：

| 问题 | 15/16 的状态 | 20 的目标 |
|------|-------------|----------|
| mem_cow 执行 | 设计级伪代码 | 完整实现，和 PhysBlock/PhysRegion 集成 |
| 页表写入 | `write_page_table_mappings()` 是占位 | 完整实现，正确处理 CoW 只读/可写 |
| 页错误入口 | 无 IPC 对接 | `do_pagefaults()` 接收内核消息 |
| 页错误处理 | `on_pagefault()` 返回 `NeedCow` 但不执行 | 完整处理链：检测→分配→复制→更新页表 |

### 1.2 从 fork 的悬念说起

16-vm-fork 以 fork 完成结束，但留下一个关键悬念：

```
fork 完成
  ├── 父子进程共享物理页，refcount = 2
  ├── 页表标记为只读
  └── ??? 写入时会发生什么 ???
```

本文档回答这个问题。完整链路：

```
用户写入 CoW 页面
  │
  ▼
CPU 触发页保护异常 (#PF, P=1, W/R=1)
  │
  ▼
内核捕获异常，发送 VM_PAGEFAULT 消息给 VM
  │
  ▼
VM do_pagefaults() 解析消息
  │
  ▼
handle_pagefault() 查找区域 + 权限检查
  │
  ▼
map_pf() → on_pagefault() → 返回 NeedCow
  │
  ▼
execute_cow() 分配新页 → 复制数据 → 更新引用 → 更新页表
  │
  ▼
进程恢复执行，写入成功
```

### 1.3 与 15/16 的关系

```
14-cow-mechanism (设计)          15-pagefault (设计)
       │                              │
       │  mem_cow 设计                │  do_pagefaults 设计
       │  pb_reference/unref 设计     │  PageFaultState 设计
       │  页表只读设计                │  MemoryType 集成设计
       │                              │
       └──────────┬───────────────────┘
                  │
                  ▼
         19-cow-exec-pagefault (实现)
                  │
         ┌────────┼────────┐
         │        │        │
    mem_cow   pt_writemap  do_pagefaults
    完整实现   完整实现      完整实现
```

**阅读前提**：本文档假设读者已读过 15 和 16，不再重复基本概念。重点放在"设计如何落地"和"现有代码如何改造"。

---

## 2. C 源码分析：完整执行链路

> 本章补全 15/16 未详细分析的执行链路——从内核到 VM 的消息传递、页错误批量处理、以及页表写入的完整逻辑。

### 2.1 内核到 VM 的页错误消息

Minix3 中，内核捕获页错误后通过 IPC 通知 VM：

```c
/* minix3/minix/kernel/system/do_vmctl.c */
int do_vmctl(struct thread *who, message *m_ptr)
{
    switch (m_ptr->m_lsys_krn_vmctl.ctrl) {
    case VMCTL_CLEAR_PAGEFAULT:
        /* VM 处理完页错误后，清除进程的页错误挂起状态 */
        who->p_misc_flags &= ~MF_VM_PFAULT;
        break;
    case VMCTL_GET_PAGEFAULT:
        /* 内核将当前挂起的页错误信息拷贝给 VM */
        m_ptr->m_vm_vfs_getpagefault.endpt = who->p_endpoint;
        m_ptr->m_vm_vfs_getpagefault.vaddr = who->p_reg.sp;
        m_ptr->m_vm_vfs_getpagefault.flags = who->p_vm_pfault_err;
        break;
    }
}
```

**关键点**：内核不是主动推送页错误，而是 VM 通过 `VMCTL_GET_PAGEFAULT` 拉取。VM 在主循环中轮询内核获取页错误信息。

### 2.2 do_pagefaults — 页错误批量处理

```c
/* minix3/minix/servers/vm/pagefaults.c:127 */
void do_pagefaults(message *msg)
{
    int result, io;
    endpoint_t ep;
    vir_bytes v;
    struct vmproc *vmp;
    u32_t flags;

    ep = msg->m_source;
    v = msg->VPF_ADDR;
    flags = msg->VPF_FLAGS;

    if(vm_isokendpt(ep, &vmp->vm_slot) != OK) {
        printf("do_pagefaults: bad endpoint %d\n", ep);
        return;
    }

    vmp = &vmproc[vmp->vm_slot];

    /* 处理单个页错误 */
    result = handle_pagefault(vmp, v, flags, &io);

    if (result == SUSPEND) {
        /* 需要异步 I/O（文件映射），进程挂起 */
        return;
    }

    if (result != OK) {
        /* 页错误处理失败，发送 SIGSEGV */
        sys_kill(vmp->vm_endpoint, SIGSEGV);
    }

    /* 清除页错误状态，允许进程继续执行 */
    sys_vmctl(VMCTL_CLEAR_PAGEFAULT, vmp->vm_endpoint);
}
```

**批量处理**：Minix3 的 `do_memory()` 函数在主循环中批量获取并处理多个页错误请求，避免每次只处理一个导致 IPC 开销过大：

```c
/* minix3/minix/servers/vm/pagefaults.c:191 */
void do_memory(void)
{
    struct vmproc *vmp;
    message msg;
    int result, io;

    while (1) {
        /* 从内核获取下一个挂起的页错误 */
        int r = sys_vmctl(VMCTL_GET_PAGEFAULT, &msg);
        if (r != OK) break;

        /* 处理页错误 */
        result = handle_pagefault(vmp, msg.VPF_ADDR, msg.VPF_FLAGS, &io);

        if (result != OK && result != SUSPEND) {
            sys_kill(vmp->vm_endpoint, SIGSEGV);
        }

        if (result != SUSPEND) {
            sys_vmctl(VMCTL_CLEAR_PAGEFAULT, vmp->vm_endpoint);
        }
    }
}
```

### 2.3 handle_pagefault — 页错误核心处理

```c
/* minix3/minix/servers/vm/pagefaults.c:57 */
int handle_pagefault(struct vmproc *vmp, vir_bytes v, u32_t flags, int *io)
{
    struct vir_region *region;
    vir_bytes offset;
    int r, write;

    write = PFERR_WRITE(flags);

    /* 1. 查找虚拟区域 */
    if(!(region = map_lookup(vmp, v, NULL)))
        return EFAULT;

    /* 2. 权限检查 */
    if(write && !(region->flags & VR_WRITABLE))
        return EFAULT;

    /* 3. 计算页内偏移 */
    offset = v - region->vaddr;
    offset -= offset % VM_PAGE_SIZE;

    /* 4. 处理页面 */
    if((r = map_pf(vmp, region, offset, write, NULL, NULL, 0, io)) != OK) {
        return r;
    }

    /* 5. 统计 */
    if(io && *io)
        vmp->vm_major_page_fault++;
    else
        vmp->vm_minor_page_fault++;

    return OK;
}
```

### 2.4 map_pf — 页面处理核心

```c
/* minix3/minix/servers/vm/region.c:664 */
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

    /* 获取或创建 phys_region */
    if(!(ph = physblock_get(region, offset))) {
        struct phys_block *pb;
        if(!(pb = pb_new(MAP_NONE))) return ENOMEM;
        if(!(ph = pb_reference(pb, offset, region, region->def_memtype))) {
            pb_free(pb);
            return ENOMEM;
        }
    }

    /* 调用内存类型的 pagefault 处理器 */
    if(!write || !ph->memtype->writable(ph)) {
        if((r = ph->memtype->ev_pagefault(vmp, region, ph, write,
            pf_callback, state, len, io)) == SUSPEND) {
            return SUSPEND;
        }
    }

    /* 更新页表 */
    if((r = map_ph_writept(vmp, region, ph)) != OK) return r;

    return r;
}
```

**关键流程**：
1. `physblock_get()` 查找已有的 PhysRegion，不存在则创建（延迟分配）
2. `memtype->ev_pagefault()` 调用具体内存类型的页错误处理（匿名内存→CoW/分配，文件映射→从文件加载）
3. `map_ph_writept()` 更新页表映射

### 2.5 map_ph_writept — 单个 PhysRegion 的页表写入

```c
/* minix3/minix/servers/vm/region.c:847 */
int map_ph_writept(struct vmproc *vmp, struct vir_region *region,
    struct phys_region *ph)
{
    int flags;
    assert(ph->ph);
    assert(ph->ph->phys != MAP_NONE);

    /* 计算页表标志 */
    flags = PTF_PRESENT | PTF_USER;
    if(pr_writable(region, ph))
        flags |= PTF_WRITE;

    /* 写入页表 */
    pt_writemap(vmp, region->vaddr + ph->offset,
        ph->ph->phys, VM_PAGE_SIZE, flags, WMF_OVERWRITE);

    return OK;
}

/* 判断 PhysRegion 是否可写 */
int pr_writable(struct vir_region *region, struct phys_region *pr)
{
    if(!(region->flags & VR_WRITABLE))
        return 0;
    return pr->memtype->writable(pr);
}
```

**pr_writable 的两层判断**：
1. **区域层**：`VR_WRITABLE` 标志——区域本身是否允许写入
2. **内存类型层**：`memtype->writable(pr)`——物理页是否可写（匿名内存：refcount==1 才可写）

两层都必须通过，页表才设置 `PTF_WRITE`。这就是 CoW 的核心：fork 后 refcount>1，`anon_writable()` 返回 0，页表不设 `PTF_WRITE`，写入触发保护异常。

### 2.6 pt_writemap — 页表映射写入

```c
/* minix3/minix/servers/vm/pagetable.c:685 */
int pt_writemap(struct vmproc *vmp, vir_bytes vaddr, phys_bytes paddr,
    size_t len, int flags, int writemapflags)
{
    int r;
    assert(!(vaddr % VM_PAGE_SIZE));
    assert(!(paddr % VM_PAGE_SIZE));
    assert(!(len % VM_PAGE_SIZE));

    while(len > 0) {
        r = pt_map(vmp, vaddr, paddr, flags, writemapflags);
        if(r != OK) return r;
        vaddr += VM_PAGE_SIZE;
        paddr += VM_PAGE_SIZE;
        len -= VM_PAGE_SIZE;
    }

    return OK;
}
```

**pt_map 内部**：分配中间页表页（PDPT/PD/PT），写入 PTE。如果 `writemapflags & WMF_OVERWRITE`，已存在的映射会被覆盖。

### 2.7 完整执行链路总结

```
用户写入 0x400000 (CoW 页面)
  │
  ▼
CPU: #PF, error_code=0x07 (P=1, W/R=1, U/S=1)
  │
  ▼
Kernel: 保存异常信息，标记进程 MF_VM_PFAULT
  │
  ▼
VM 主循环: sys_vmctl(VMCTL_GET_PAGEFAULT) → 获取 {ep, vaddr=0x400000, flags=0x07}
  │
  ▼
do_pagefaults(): 解析 endpoint → vmproc
  │
  ▼
handle_pagefault(vmp, 0x400000, 0x07):
  ├── map_lookup(vmp, 0x400000) → 找到 VirRegion
  ├── write=1, VR_WRITABLE=1 → 权限 OK
  └── map_pf(vmp, region, offset, write=1):
       ├── physblock_get(region, offset) → 找到 PhysRegion
       ├── !writable(pr) → 调用 anon_pagefault()
       │    ├── refcount=2, write=1 → NeedCow
       │    └── mem_cow(region, ph):
       │         ├── alloc_mem(1) → new_page
       │         ├── sys_abscopy(old_phys, new_page, 4096) → 复制数据
       │         ├── pb_unreferenced(region, ph, rm=0) → old_pb.refcount: 2→1
       │         ├── pb_link(ph, new_pb, offset, region) → new_pb.refcount=1
       │         └── ph->memtype = &mem_type_anon
       └── map_ph_writept(vmp, region, ph):
            ├── pr_writable(region, ph) → 1 (refcount=1, VR_WRITABLE)
            ├── flags = PTF_PRESENT | PTF_USER | PTF_WRITE
            └── pt_writemap(vmp, 0x400000, new_phys, 4096, flags, WMF_OVERWRITE)
  │
  ▼
sys_vmctl(VMCTL_CLEAR_PAGEFAULT) → 进程恢复执行
  │
  ▼
用户写入 0x400000 成功（私有页，可写）
```

---

## 3. Rust 设计决策

### 3.1 现有代码状态

| 组件 | 现有状态 | 缺失 |
|------|---------|------|
| `PhysRegion::needs_cow()` | ✅ 已实现 | — |
| `PhysRegion::is_writable()` | ✅ 已实现 | — |
| `AnonymousMemory::on_pagefault()` | ✅ 返回 `NeedCow`/`NeedNewPage` | 不执行实际 CoW |
| `VmProc::write_page_table_mappings()` | 🟡 占位实现 | 不处理单个 PhysRegion 更新 |
| `VirRegion::prepare_cow()` | 🟡 占位（只计算 flags） | 不实际调用 pt.map() |
| `do_pagefaults()` | ❌ 不存在 | 无 IPC 对接 |
| `mem_cow()` | ❌ 不存在 | 无 CoW 执行 |
| `map_ph_writept()` | ❌ 不存在 | 无单页页表更新 |
| `handle_pagefault()` | ❌ 不存在 | 无页错误处理链 |

### 3.2 设计原则

**原则 1：复用 15/16 的设计，不重新发明**

15/16 的设计是正确的，20 只需要把它们落地。具体来说：
- `PagefaultResult` 枚举（`Handled`/`NeedNewPage`/`NeedCow`/`AccessViolation`）保留
- `MemType::on_pagefault()` 接口保留，只补充调用后的执行逻辑
- `PageFaultInfo`/`PageFaultType`/`AccessType` 类型保留

**原则 2：Direct Map 简化物理页操作**

15/16 已经分析了 Direct Map 的简化效果。20 的实现直接使用 `vm_phys_to_virt()` + `copy_nonoverlapping()`，不使用 `sys_abscopy`。

**原则 3：单页页表更新 vs 全量写入**

Minix3 有两个层次的页表写入：
- `map_writept(vmp)` — 遍历所有区域，全量写入（fork 后使用）
- `map_ph_writept(vmp, vr, pr)` — 单个 PhysRegion 更新（CoW 后使用）

Rust 代码已有 `write_page_table_mappings()`（对应 `map_writept`），需要新增 `write_pt_single()`（对应 `map_ph_writept`）。

**原则 4：页错误处理不使用状态机**

15-pagefault 设计了 `PageFaultState` 状态机，但 VM 是单线程事件循环，状态机增加了不必要的复杂度。20 使用简单的函数调用链：

```rust
fn do_pagefaults(msg) → handle_pagefault(vmp, vaddr, write) → map_pf(vmp, region, offset, write)
```

### 3.3 mem_cow 实现设计

```rust
/// 执行 Copy-on-Write
///
/// 对应 Minix3 的 mem_cow()。
/// 前置条件：pr.needs_cow() == true && region.is_writable()
fn execute_cow(
    region: &mut VirRegion,
    pr: &mut PhysRegion,
    page_alloc: &mut VmPageAllocator,
) -> Result<(), CowError> {
    let old_phys = pr.get_phys_addr()
        .ok_or(CowError::NoPhysicalAddress)?;

    // 1. 分配新物理页
    let new_phys = page_alloc.alloc_phys(1, PageAllocFlags::CLEAR)
        .map_err(|_| CowError::OutOfMemory)?;

    // 2. 复制页面内容（Direct Map）
    unsafe {
        let src = vm_phys_to_virt(old_phys) as *const u8;
        let dst = vm_phys_to_virt(new_phys) as *mut u8;
        core::ptr::copy_nonoverlapping(src, dst, PAGE_SIZE);
    }

    // 3. 取消旧 PhysBlock 引用
    pr.unlink_from_block();

    // 4. 创建新 PhysBlock 并链接
    let new_pb = PhysBlock::new(new_phys);
    pr.link_to_block(new_pb);

    // 5. 设置内存类型为匿名
    pr.set_memtype(MEM_TYPE_ANON);

    Ok(())
)
}
```

**与 Minix3 的关键差异**：

| 步骤 | Minix3 | minix-rs (Direct Map) |
|------|--------|----------------------|
| 复制数据 | `sys_abscopy()` 内核系统调用 | `vm_phys_to_virt()` + `copy_nonoverlapping()` |
| 分配标志 | `vrallocflags()` 转换 | `PageAllocFlags::CLEAR`（分配时清零，CoW 场景实际不需要清零因为会覆盖） |
| PhysBlock 管理 | `pb_unreferenced()` + `pb_link()` | `unlink_from_block()` + `link_to_block()` |
| memtype 设置 | `ph->memtype = &mem_type_anon` | `pr.set_memtype(MEM_TYPE_ANON)` |

**PageAllocFlags::CLEAR 的处理**：CoW 场景下新页会被旧页内容完全覆盖，不需要清零。但 Minix3 的 `alloc_mem()` 在 `vrallocflags()` 包含 `VR_UNINITIALIZED` 时不清零。Rust 实现可以用 `PageAllocFlags::empty()` 跳过清零：

```rust
let new_phys = page_alloc.alloc_phys(1, PageAllocFlags::empty())
    .map_err(|_| CowError::OutOfMemory)?;
```

### 3.4 map_ph_writept 实现设计

```rust
/// 更新单个 PhysRegion 的页表映射
///
/// 对应 Minix3 的 map_ph_writept()。
/// CoW 执行后必须调用此函数更新页表，否则进程仍然看到旧映射。
fn write_pt_single(
    vmp: &mut ActiveProc<'_>,
    region: &VirRegion,
    pr: &PhysRegion,
) -> Result<(), PageTableError> {
    let phys = pr.get_phys_addr()
        .ok_or(PageTableError::InvalidAddress)?;
    let vaddr = VirBytes(region.vaddr().0 + pr.offset());

    // 判断可写性：区域可写 && 物理页可写（refcount==1）
    let writable = region.is_writable() && pr.is_writable();
    let flags = if writable {
        PageFlags::read_write()
    } else {
        PageFlags::read_only()
    };

    // 使用 remap 原子替换映射
    vmp.page_table_mut().remap(vaddr, phys, flags)?;

    // 刷新 TLB
    unsafe { vmp.page_table_mut().flush_tlb_addr(vaddr); }

    Ok(())
}
```

**为什么用 `remap` 而不是 `unmap` + `map`**：CoW 后虚拟地址已经有映射（指向旧物理页），需要原子替换为新物理页。`remap` 避免了"无映射窗口"——如果先 `unmap` 再 `map`，中间时刻该地址无映射，如果此时发生中断或 TLB 未刷新，可能导致问题。

### 3.5 do_pagefaults 实现设计

```rust
/// 处理页错误 IPC 消息
///
/// 对应 Minix3 的 do_pagefaults()。
/// 从内核获取页错误信息，处理后清除页错误状态。
pub fn do_pagefaults(
    msg: &PageFaultMessage,
    table: &mut VmProcTable,
    page_alloc: &mut VmPageAllocator,
) -> PageFaultResult {
    let ep = msg.endpoint;
    let vaddr = VirBytes(msg.vaddr);
    let write = msg.is_write();

    // 1. 查找进程
    let slot = match vm_isokendpt(ep) {
        Some(s) => s,
        None => return PageFaultResult::InvalidProcess,
    };

    // 2. 处理页错误
    let result = {
        let mut vmp = table.get_active(slot)
            .ok_or(PageFaultResult::InvalidProcess)?;

        handle_pagefault(&mut vmp, vaddr, write, page_alloc)
    };

    match result {
        Ok(()) => {
            // 清除页错误状态
            kernel::vmctl_clear_pagefault(ep);
            PageFaultResult::Ok
        }
        Err(PageFaultError::Suspend) => PageFaultResult::Suspend,
        Err(e) => {
            // 发送 SIGSEGV
            kernel::sys_kill(ep, Signal::SIGSEGV);
            kernel::vmctl_clear_pagefault(ep);
            PageFaultResult::Segfault
        }
    }
}
```

### 3.6 handle_pagefault 实现设计

```rust
/// 页错误核心处理
///
/// 对应 Minix3 的 handle_pagefault()。
fn handle_pagefault(
    vmp: &mut ActiveProc<'_>,
    vaddr: VirBytes,
    write: bool,
    page_alloc: &mut VmPageAllocator,
) -> Result<(), PageFaultError> {
    // 1. 查找虚拟区域
    let region_idx = vmp.regions().find_index(vaddr)
        .ok_or(PageFaultError::InvalidAddress)?;

    // 2. 权限检查
    {
        let region = &vmp.regions()[region_idx];
        if write && !region.is_writable() {
            return Err(PageFaultError::PermissionDenied);
        }
    }

    // 3. 计算页偏移
    let region = &mut vmp.regions_mut()[region_idx];
    let offset = (vaddr.0 - region.vaddr().0) & !(PAGE_SIZE - 1);
    let page_idx = (offset / PAGE_SIZE) as usize;

    // 4. 获取或创建 PhysRegion
    let needs_new = region.physblocks[page_idx].is_none();
    if needs_new {
        // 延迟分配：创建新的 PhysBlock + PhysRegion
        let new_pb = PhysBlock::new(PhysBlock::MAP_NONE);
        let new_pr = PhysRegion::new_linked(new_pb, offset);
        region.physblocks[page_idx] = Some(new_pr);
    }

    let pr = region.physblocks[page_idx].as_mut()
        .ok_or(PageFaultError::InternalError)?;

    // 5. 调用内存类型的页错误处理
    let result = if let Some(memtype) = pr.memtype {
        memtype.ev_pagefault(&vmp.as_active(), region, pr, write)?
    } else {
        // 无 memtype，默认行为
        if pr.needs_cow() && write {
            PagefaultResult::NeedCow
        } else if !pr.has_phys_block() {
            PagefaultResult::NeedNewPage
        } else {
            PagefaultResult::Handled
        }
    };

    // 6. 根据结果执行操作
    match result {
        PagefaultResult::Handled => {}
        PagefaultResult::NeedNewPage => {
            let new_phys = page_alloc.alloc_phys(1, PageAllocFlags::CLEAR)
                .map_err(|_| PageFaultError::OutOfMemory)?;
            pr.set_phys_addr(new_phys);
        }
        PagefaultResult::NeedCow => {
            execute_cow(region, pr, page_alloc)
                .map_err(|_| PageFaultError::OutOfMemory)?;
        }
        PagefaultResult::AccessViolation => {
            return Err(PageFaultError::PermissionDenied);
        }
    }

    // 7. 更新页表
    write_pt_single(vmp, region, pr)?;

    // 8. 统计
    vmp.inc_minor_fault();

    Ok(())
}
```

**关键设计决策**：

1. **on_pagefault 只返回决策，不执行操作**：`AnonymousMemory::on_pagefault()` 返回 `NeedCow`，实际的 CoW 执行由 `handle_pagefault()` 完成。这保持了 MemType trait 的简洁性——它只做判断，不做资源分配。

2. **write_pt_single 在最后统一调用**：无论是 NeedNewPage 还是 NeedCow，最后都需要更新页表。统一在最后调用避免重复。

3. **延迟分配在 handle_pagefault 中处理**：Minix3 在 `map_pf()` 中通过 `physblock_get()` + `pb_new()` + `pb_reference()` 处理。Rust 版本简化为直接创建 PhysRegion。

### 3.7 错误处理

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageFaultError {
    InvalidAddress,
    PermissionDenied,
    OutOfMemory,
    InternalError,
    Suspend,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CowError {
    NoPhysicalAddress,
    OutOfMemory,
    CopyFailed,
}
```

**SIGSEGV 触发条件**：

| 条件 | PageFaultError | 说明 |
|------|---------------|------|
| 地址不在任何区域 | InvalidAddress | 访问了未映射的地址 |
| 写只读区域 | PermissionDenied | 区域本身不可写（不是 CoW） |
| CoW 内存不足 | OutOfMemory | 无法分配新物理页 |

**CoW 内存不足的处理**：Minix3 在 `ENOMEM` 时直接发送 SIGSEGV 杀死进程。这是合理的——如果连一页物理内存都无法分配，进程无法继续运行。Rust 实现遵循相同策略。

---

## 4. 实现详解

### 4.1 新增模块：pagefault.rs

```rust
// os/servers/vm/src/pagefault.rs

use crate::*;
use minix_types::{VirBytes, Endpoint};
use crate::phys_mem::PageAllocFlags;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageFaultError {
    InvalidAddress,
    PermissionDenied,
    OutOfMemory,
    InternalError,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CowError {
    NoPhysicalAddress,
    OutOfMemory,
}

pub struct PageFaultMessage {
    pub endpoint: Endpoint,
    pub vaddr: u64,
    pub error_code: u32,
}

impl PageFaultMessage {
    pub fn is_write(&self) -> bool {
        (self.error_code & 0x2) != 0
    }

    pub fn is_present(&self) -> bool {
        (self.error_code & 0x1) != 0
    }

    pub fn is_user(&self) -> bool {
        (self.error_code & 0x4) != 0
    }
}

const PAGE_SIZE: u64 = 4096;

pub fn handle_pagefault(
    vmp: &mut ActiveProc<'_>,
    vaddr: VirBytes,
    write: bool,
    page_alloc: &mut VmPageAllocator,
) -> Result<(), PageFaultError> {
    let region_idx = vmp.regions().find_index(vaddr)
        .ok_or(PageFaultError::InvalidAddress)?;

    {
        let region = &vmp.regions()[region_idx];
        if write && !region.is_writable() {
            return Err(PageFaultError::PermissionDenied);
        }
    }

    let region = &mut vmp.regions_mut()[region_idx];
    let offset = (vaddr.0 - region.vaddr().0) & !(PAGE_SIZE - 1);
    let page_idx = (offset / PAGE_SIZE) as usize;

    if page_idx >= region.physblocks.len() {
        return Err(PageFaultError::InvalidAddress);
    }

    let needs_new = region.physblocks[page_idx].is_none();
    if needs_new {
        let new_pb = PhysBlock::new(PhysBlock::MAP_NONE);
        let new_pr = PhysRegion::new_linked(new_pb, offset);
        region.physblocks[page_idx] = Some(new_pr);
    }

    let pr = region.physblocks[page_idx].as_mut()
        .ok_or(PageFaultError::InternalError)?;

    let result = if let Some(memtype) = pr.memtype {
        memtype.ev_pagefault(&vmp.as_active(), region, pr, write)
            .map_err(|_| PageFaultError::InternalError)?
    } else {
        if pr.needs_cow() && write {
            PagefaultResult::NeedCow
        } else if !pr.has_phys_block() {
            PagefaultResult::NeedNewPage
        } else {
            PagefaultResult::Handled
        }
    };

    match result {
        PagefaultResult::Handled => {}
        PagefaultResult::NeedNewPage => {
            let new_phys = page_alloc.alloc_phys(1, PageAllocFlags::CLEAR)
                .map_err(|_| PageFaultError::OutOfMemory)?;
            pr.set_phys_addr(new_phys);
        }
        PagefaultResult::NeedCow => {
            execute_cow(region, pr, page_alloc)
                .map_err(|_| PageFaultError::OutOfMemory)?;
        }
        PagefaultResult::AccessViolation => {
            return Err(PageFaultError::PermissionDenied);
        }
    }

    write_pt_single(vmp, region, pr)?;

    vmp.inc_minor_fault();

    Ok(())
}

fn execute_cow(
    region: &mut VirRegion,
    pr: &mut PhysRegion,
    page_alloc: &mut VmPageAllocator,
) -> Result<(), CowError> {
    let old_phys = pr.get_phys_addr()
        .ok_or(CowError::NoPhysicalAddress)?;

    let new_phys = page_alloc.alloc_phys(1, PageAllocFlags::empty())
        .map_err(|_| CowError::OutOfMemory)?;

    unsafe {
        let src = vm_phys_to_virt(old_phys) as *const u8;
        let dst = vm_phys_to_virt(new_phys) as *mut u8;
        core::ptr::copy_nonoverlapping(src, dst, PAGE_SIZE as usize);
    }

    pr.unlink_from_block();

    let new_pb = PhysBlock::new(new_phys);
    pr.link_to_block(new_pb);

    pr.set_memtype(MEM_TYPE_ANON);

    Ok(())
}

fn write_pt_single(
    vmp: &mut ActiveProc<'_>,
    region: &VirRegion,
    pr: &PhysRegion,
) -> Result<(), PageTableError> {
    let phys = pr.get_phys_addr()
        .ok_or(PageTableError::InvalidAddress)?;
    let vaddr = VirBytes(region.vaddr().0 + pr.offset());

    let writable = region.is_writable() && pr.is_writable();
    let flags = if writable {
        PageFlags::read_write()
    } else {
        PageFlags::read_only()
    };

    vmp.page_table_mut().remap(vaddr, phys, flags)?;

    unsafe { vmp.page_table_mut().flush_tlb_addr(vaddr); }

    Ok(())
}
```

### 4.2 改造 write_page_table_mappings

现有的 `write_page_table_mappings()` 是 fork 后的全量写入，需要确保它和 `write_pt_single()` 使用相同的可写性判断逻辑：

```rust
/// 全量写入页表映射（fork 后使用）
///
/// 对应 Minix3 的 map_writept()。
pub(crate) unsafe fn write_page_table_mappings(&mut self) {
    const PAGE_SIZE: u64 = <PageTable as Paging>::PAGE_SIZE as u64;

    let mut mappings: alloc::vec::Vec<(VirBytes, PhysBytes, PageFlags)> = alloc::vec::Vec::new();

    for region in self.regions_mut().iter_mut() {
        for (i, phys_opt) in region.physblocks.iter().enumerate() {
            if let Some(phys) = phys_opt {
                if let Some(block_ptr) = phys.ph {
                    unsafe {
                        let block = &*block_ptr.as_ptr();
                        let vaddr = VirBytes(region.vaddr.0 + i as u64 * PAGE_SIZE);
                        let paddr = block.phys();

                        // 与 write_pt_single 使用相同的可写性判断
                        let writable = region.is_writable() && phys.is_writable();
                        let flags = if writable {
                            PageFlags::read_write()
                        } else {
                            PageFlags::read_only()
                        };

                        mappings.push((vaddr, paddr, flags));
                    }
                }
            }
        }
    }

    let pt = self.page_table_mut();
    for (vaddr, paddr, flags) in mappings {
        let _ = pt.remap(vaddr, paddr, flags);
    }
}
```

**改造要点**：
1. 使用 `phys.is_writable()` 而不是 `block.refcount() == 1`，保持和 `write_pt_single` 一致
2. 使用 `remap` 而不是 `map`，因为 fork 后父进程的页表已有映射，需要覆盖

### 4.3 PhysRegion 补充方法

```rust
// phys_region.rs 新增方法

impl PhysRegion {
    /// 获取区域内偏移
    pub(crate) fn offset(&self) -> u64 {
        self.offset
    }

    /// 设置物理地址
    pub(crate) fn set_phys_addr(&mut self, phys: PhysBytes) {
        if let Some(block_ptr) = self.ph {
            unsafe {
                (*block_ptr.as_ptr()).set_phys(phys);
            }
        }
    }

    /// 设置内存类型
    pub(crate) fn set_memtype(&mut self, memtype: &'static dyn MemType) {
        self.memtype = Some(memtype);
    }
}
```

### 4.4 lib.rs 注册模块

```rust
// lib.rs 新增
pub(crate) mod pagefault;
pub(crate) use pagefault::{PageFaultError, CowError, PageFaultMessage};
```

---

## 5. 测试要点

### 5.1 CoW 执行测试

| 测试场景 | 验证点 |
|---------|--------|
| fork 后写入触发 CoW | 新物理页分配成功，旧页 refcount 减 1，新页 refcount 为 1 |
| CoW 后父子进程隔离 | 父进程写入不影响子进程，反之亦然 |
| CoW 后页表可写 | PTE 包含 WRITABLE 标志 |
| CoW 后 memtype 为匿名 | `pr.memtype` 指向 `MEM_TYPE_ANON` |
| CoW 内存不足 | 返回 `CowError::OutOfMemory` |
| 多次 CoW（祖父子孙三代 fork） | refcount 正确递减，物理页正确释放 |

### 5.2 页错误处理测试

| 测试场景 | 验证点 |
|---------|--------|
| 首次访问（延迟分配） | 分配新页，页表映射正确 |
| 写保护错误（CoW） | 执行 CoW，页表更新为可写 |
| 读保护错误 | 不触发 CoW，直接返回 |
| 地址无效 | 返回 `InvalidAddress` |
| 写只读区域 | 返回 `PermissionDenied` |
| 连续页错误 | 批量处理正确 |

### 5.3 页表写入测试

| 测试场景 | 验证点 |
|---------|--------|
| fork 后全量写入 | 所有共享页标记只读，私有页标记可写 |
| CoW 后单页更新 | 只更新一个 PTE，其他不变 |
| remap 原子性 | 无"无映射窗口" |
| TLB 刷新 | 更新后 TLB 一致 |

### 5.4 集成测试

| 测试场景 | 验证点 |
|---------|--------|
| fork → 写入 → CoW 完整流程 | 端到端正确 |
| fork → exec → 旧页释放 | CoW 页面在 exec 后正确释放 |
| 多进程 fork 树 | refcount 在复杂场景下正确 |
| 物理内存压力 | OOM 时正确处理 |
