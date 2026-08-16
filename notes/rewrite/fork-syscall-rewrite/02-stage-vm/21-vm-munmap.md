# 21-vm-munmap: VM_MUNMAP / MAP_PHYS / UNMAP_PHYS / SHM_UNMAP —— 映射拆除与物理映射

> **分类**: 阶段 7 — IPC 服务（进程生命周期）
> **源码**: `minix3/minix/servers/vm/mmap.c`（573 行：`map_perm_check` :284-307 / `do_map_phys` :310-363 / `munmap_vm_lin` :488-510 / `do_munmap` :512-573）+ `region.c`（`map_subfree` :527-565 / `map_free` :568-585 / `map_free_proc` :589-612 / `map_lookup` :616-641 / `map_unmap_region` :1065-1147 / `split_region` :1150-1220 / `map_unmap_range` :1222-1294）+ `mem_directphys.c`（全 79 行：`mem_type_directphys` :28-35 / `phys_pt_flags` :37-43 / `phys_unreference` :45-48 / `phys_pagefault` :50-61 / `phys_writable` :63-67 / `phys_setphys` :69-72 / `phys_copy` :74-78）+ `pb.c`（`pb_unreferenced` :96-134）+ `mem_file.c`（`mappedfile_delete` :280-287）+ `fdref.c`（`fdref_deref` :116-154）+ `main.c`（CALLMAP :538-540/:564）+ `minix3/minix/include/minix/ipc.h`（`mess_lc_vm_shm_unmap` :936-940 / `mess_lsys_vm_map_phys` :1504-1510 / `mess_lsys_vm_unmap_phys` :1522-1526）+ `minix3/minix/include/minix/com.h`（`VM_MUNMAP` :649 / `VMUM_ADDR`/`VMUM_LEN` :650-651 / `VM_MAP_PHYS` :677 / `VM_UNMAP_PHYS` :679 / `VM_SHM_UNMAP` :718）+ `minix3/minix/lib/libc/sys/mmap.c`（`munmap` :76-85）
> **Rust 模块**: `os/servers/vm/src/munmap.rs`（`MunmapError` :27-38 / `MunmapOutcome` :46-51 / `roundup_page` :98-100 / `handle_munmap` :102-158 / `munmap_vm_lin` :160-171 / `unmap_range` :173-297 / tests :300-818）+ `os/servers/vm/src/map_phys.rs`（`MapPhysError` :32-40 / `handle_map_phys` :48-101 / `map_perm_check` :104-121 / tests :123-180）+ `os/servers/vm/src/region/mod.rs`（`free_region_pages` :23-81）+ `os/servers/vm/src/region/vir_region.rs`（`split` :279-350 / `free_range` :352-368）+ `os/servers/vm/src/memtype.rs`（`DirectPhysical` :339-418）+ `os/servers/vm/src/ipc/dispatcher.rs`（`dispatch_munmap` :142-162 / `dispatch_unmap_phys` :164-184 / `dispatch_shm_unmap` :186-204 / `dispatch_map_phys` :433-445 / 主循环分支 :1038/:1043/:1157 / 错误映射 :1243-1253）+ `os/libs/minix-types/src/ipc/vm.rs`（`VmMunmapIn::decode_message` :791-812 / `VmUnmapPhysIn::decode_message` :309-331 / `VmShmUnmapIn::decode_message` :333-351）+ `os/libs/minix-types/src/ipc/message.rs`（`MessLsysVmUnmapPhys` :1744-1762 / `MessLcVmShmUnmap` :1773-1791）
> **前置**: `notes/rewrite/fork-syscall-rewrite/02-stage-vm/13-region-mapping.md`（区域结构 + `map_unmap_*` 的底层区域操作）+ `20-vm-mmap.md`（mmap 建立——munmap 的镜像）+ `15-ipc-dispatch.md`（主循环分发与 SUSPEND）+ `12-memtype.md`（directphys 回调族）
> **说明**: 本文档管 **映射拆除服务**——VM 如何响应 `VM_MUNMAP`/`VM_UNMAP_PHYS`/`VM_SHM_UNMAP`（`do_munmap` 统一处理）与物理映射 `VM_MAP_PHYS`（`do_map_phys`）：入口解析（target/长度）→ 范围拆除（`map_unmap_range` 四情形 + `map_unmap_region` 三情形）→ 物理页引用解除（`pb_unreferenced`）→ 数据结构回收（`map_free`/fdref）。**不覆盖**：mmap 建立（20）、exit 全区域释放 `map_free_proc`（22）、fdref 表与 VFS 对话（23）、页缓存交互（24）、`do_get_phys`/`do_get_refcount`（26）。

---

## 1. 概念：munmap = 拆除映射 + 释放资源

### 1.0 章节引言

20 回答了"映射从哪来"（mmap 三路分配 + 四种内存来源绑定）；本文档回答它的镜像问题——**"映射怎么安全地拆掉，资源怎么还回去"**。

拆除一段映射是地址空间生命周期中最需要小心的一步：虚拟区间可能只覆盖区域的**一部分**（要分裂）、物理页可能被**多个进程共享**（引用计数）、设备内存**不属于 VM**（不能释放）。munmap 的核心不是"删一个条目"，而是四件事的原子组合：

```
① 定位：找出 [addr, addr+len) 覆盖的所有区域（map_unmap_range）
② 分裂/收缩：区域可能只被部分覆盖 → split_region / 三情形收缩
③ 引用解除：逐页 pb_unreferenced → refcount--，归零才释放物理页
④ 回收：区域结构 map_free +（file 区域）fdref 递减
```

它在整个 02-stage-vm 中的位置：

```
13（区域结构）→ 15（分发）→ 19（brk）→ 20（mmap 建立）→ ★21（munmap 拆除 + map_phys）
→ 22（exit 全释放）→ 23（fdref/VFS）→ 24（页缓存）→ 26（queries）
```

### 1.1 四个入口的统一

Minix3 有四个相关的消息类型，C 由两个函数承接：

| 消息 | 值（com.h） | 发送者 | 语义 | C handler |
|------|-----------|--------|------|-----------|
| `VM_MAP_PHYS` | `VM_RQ_BASE+15`（:677） | 驱动 | 把物理地址区间映射进进程地址空间（设备寄存器/DMA） | `do_map_phys`（mmap.c:310-363） |
| `VM_UNMAP_PHYS` | `VM_RQ_BASE+16`（:679） | 驱动 | 取消上面的映射（按区域整体） | `do_munmap` |
| `VM_MUNMAP` | `VM_RQ_BASE+17`（:649） | 用户进程 | 标准 munmap()（按指定范围） | `do_munmap` |
| `VM_SHM_UNMAP` | `VM_RQ_BASE+34`（:718） | 用户进程 | 取消共享内存映射（按区域整体） | `do_munmap` |

`do_munmap` 是三个解除入口的**多路复用器**（main.c CALLMAP :538/:540/:564 全部指向它）。`proto.h:80` 声明的 `do_unmap_phys` 是**死声明**——没有定义，实际 handler 是 `do_munmap`（grep 实证：proto.h 仅此一处，无 `do_unmap_phys(` 定义）。

三个解除入口的关键差异在 **target 与长度来源**：

| 入口 | target 来源 | 长度来源 |
|------|------------|---------|
| `VM_MUNMAP` | `m_source`（调用者） | 消息 `len`，`roundup` 到页 |
| `VM_UNMAP_PHYS` | 消息 `ep`（替他人取消） | **区域全长**（`len = vr->length`） |
| `VM_SHM_UNMAP` | 消息 `forwhom` | **区域全长** |

UNMAP_PHYS/SHM_UNMAP 无视消息长度、按 `addr` 所在区域**整体**拆除——因为物理映射和共享映射的建立粒度就是区域；只有标准 munmap 允许任意子范围。

### 1.2 拆除映射的四步（概念模型）

**第一步：定位（map_unmap_range）**。`[unmap_start, unmap_limit)` 可能与 0..n 个区域重叠：

```
[区域A][区域B][  区域C  ][区域D]
        |←── unmap 范围 ──→|
```

C 从 `unmap_start` 的 `AVL_LESS_EQUAL` 开始迭代，直到 `vr->vaddr >= unmap_limit`。无重叠 → 静默成功（POSIX munmap 对未映射地址是 no-op）。

**第二步：分裂/收缩**。单个区域被部分覆盖时有四种情形（§2.5 展开）：

```
情形1 整体包含  [AAAA]→(空)      remove + free
情形2 中间掏空  [AAA|BBB|CCC]→[AAA][CCC]   split 两次，free 中间
情形3 头部切割  [AAAAAA]→[BBB]  split 一次，free 头部
情形4 尾部切割  [AAAAAA]→[AAA]  split 一次，free 尾部
```

**第三步：引用解除**。逐页 `pb_unreferenced`：`phys_block.refcount--`；**归零**才调用 memtype 的 `ev_unreference` 释放物理页。共享页（CoW/remap）refcount>1 时只减不释放。

**第四步：回收**。`map_free`：`ev_delete` 回调（file 区域在此 `fdref_deref`，最后一个引用消失 → 异步通知 VFS 关 fd）→ 释放 physblocks 数组 → 释放 VirRegion 本身。

### 1.3 三种收缩 vs 分裂（为什么需要 ev_lowshrink）

`map_unmap_region` 的"三情形"（§2.5）里，**头部切割**（offset==0）最特殊：区域要 `vaddr += len`，所有剩余 `phys_region.offset` 要同步前移，physblocks 数组要 memmove。这要求 memtype 提供 `ev_lowshrink` 回调（如 mappedfile 的 `offset += len`）。**中间掏空**则走 `split_region`：要求 `ev_split` 回调（如 mappedfile 的 fdref 引用计数 ×2）。**尾部切割**最便宜：`length -= len` 即可。

> 概念要点：**低端收缩 ≠ 分裂**。低端收缩改变区域起点（所有内部偏移都要平移）；分裂只切开 physblocks 数组。两者需要不同的 memtype 回调支持——缺 `ev_lowshrink`/`ev_split` 的 memtype（如 `mem_type_directphys`）在对应操作时返回 EINVAL（region.c:1096/:1164）。

### 1.4 物理页引用计数语义

拆除映射时物理页的命运由 `refcount` 决定，与内存来源无关（统一走 `pb_unreferenced`）：

| 场景 | refcount | munmap 后 | 物理页去向 |
|------|----------|-----------|-----------|
| 独占匿名页 | 1 | 1→0 | `ev_unreference` → 归还分配器 |
| CoW 共享页（fork 后） | 2 | 2→1 | 保留（另一进程仍映射） |
| 共享内存（remap） | 2 | 2→1 | 保留（另一区域仍映射） |
| directphys（设备内存） | 1 | 1→0 | `phys_unreference` 空操作——设备内存**不归 VM 管理** |

**Rust PFN 模型**：`PageFrames.refcount` 由 `PageSlot` 映射/解除维护（`map_page` +1 / `unmap_page` -1）；归零且非 `IN_CACHE` 时 `free_region_pages` 调用 `ev_unreference` + `PfnAllocator::free_pfn`（分配职责集中到分配器，与 C 的 `free_mem` 对应——语义等价、结构不同，见 §3.4）。

### 1.5 对照：Redox 与 Linux

- **Linux**（mm/mmap.c）：`do_munmap` → `__do_munmap` → `unmap_region` → `unmap_vmas`（PTE 清除 + `page_remove_rmap` 引用递减）+ `free_pgtables`（页表页回收）+ `tlb_finish_mmu`（**TLB 批量失效**）。VMA 分裂 `split_vma`（含 `vma->vm_ops->open` 回调，与 memtype `ev_split` 同构）。**最佳实践**：TLB 失效是"收集-批量执行"两段式，避免逐页 IPI；minix-rs 目前逐页 `pt.unmap`，TLB 维护在 arch 页表层（08 范围），未做批量失效——诚实标注为 backlog。
- **Redox**：`AddrSpace::unmap_region`（kernel/scheme/mm.rs）——从 grant BTreeMap 移除区域、按范围分裂、`physmap` 逐页 unmap。与 minix-rs 的"BTreeMap 区域表驱动 + 分裂"同构；Redox 的 unmap 返回被释放的物理页列表由 scheme 层回收，与 `free_region_pages` 的 `pending` 返回（pfn, memtype）列表设计一致。
- **对照要点**：三家的 munmap 都抽象为**区间分裂 + PTE 清除 + 引用计数 + 结构回收**四步。Minix3 的独特之处：① munmap 在**用户态 VM 服务器**（无直接硬件 TLB 操作，靠 `pt_writemap` 更新软件页表）；② 三个解除入口复用 `do_munmap`（内核/驱动与用户进程同路径）；③ 设备内存（directphys）通过 memtype 回调**免释放**——Linux 的 `VM_IO`/`pgprot_noncached` 区域同样不参与页回收，概念等价。

### 1.6 小结

munmap = 定位（map_unmap_range 四情形）→ 分裂/收缩（split_region / 三情形 + ev_lowshrink/ev_split 回调）→ 引用解除（pb_unreferenced → refcount 归零才释放）→ 回收（map_free + fdref）。map_phys 是它的镜像：建立 VR_DIRECT 区域、`phys_setphys` 记录物理基址、缺页时直接填物理地址、拆除时 ev_unreference 空操作。C 的 do_munmap 有一个**未初始化变量 UB**（§2.2），Rust 采信消息字段并标注。

---

## 2. C 源码分析

### 2.1 调用路径与消息格式

```
用户 munmap(addr, len)
  → libc munmap（libc/sys/mmap.c:76-85：m.VMUM_ADDR=addr; m.VMUM_LEN=len → _syscall(VM_PROC_NR, VM_MUNMAP)）
  → kernel 转发（m_source = 调用者）
  → VM 主循环（vm_server.rs dispatch_on_msg 优先级 4 → acl_check(VM_MUNMAP) → dispatcher.rs:1038）
  → do_munmap(msg)              mmap.c:512-573
```

**消息结构**（三个解除入口 + 物理映射入口的 32 位 C 线格式）：

| 消息 | C 结构（ipc.h） | 字段 |
|------|----------------|------|
| `VM_MUNMAP` | 复用 `mess_mmap`（com.h:650-651 `VMUM_ADDR`=`m_mmap.addr`、`VMUM_LEN`=`m_mmap.len`） | addr u32@8、len u32@12（§20 表 2.1） |
| `VM_UNMAP_PHYS` | `mess_lsys_vm_unmap_phys`（ipc.h:1522-1526） | `ep` i32@0、`vaddr` u32@4 |
| `VM_SHM_UNMAP` | `mess_lc_vm_shm_unmap`（ipc.h:936-940） | `forwhom` i32@0、`addr` u32@4 |
| `VM_MAP_PHYS` | `mess_lsys_vm_map_phys`（ipc.h:1504-1510） | `ep` i32@0、`phaddr` u32@4、`len` u32@8、`reply` u32@12 |

**target 解析规则**（mmap.c:518-525）：`VM_UNMAP_PHYS` 取 `ep`、`VM_SHM_UNMAP` 取 `forwhom`、其余默认 `SELF` → `m_source`。即标准 munmap 的目标**永远是调用者自己**；后两个允许"替他人"取消（驱动替客户端取消物理映射、进程取消它替别人建立的共享映射）。

### 2.2 do_munmap 分步（mmap.c:512-573）

```c
int do_munmap(message *m)
{
    int r, n;
    struct vmproc *vmp;
    struct vir_region *vr;
    vir_bytes addr, len;
    endpoint_t target = SELF;

    /* 1. 确定目标进程 */
    if(m->m_type == VM_UNMAP_PHYS)
        target = m->m_lsys_vm_unmap_phys.ep;
    else if(m->m_type == VM_SHM_UNMAP)
        target = m->m_lc_vm_shm_unmap.forwhom;
    if(target == SELF)
        target = m->m_source;
    if((r=vm_isokendpt(target, &n)) != OK)
        panic("do_mmap: message from strange source: %d", m->m_source);   // :529-531
    vmp = &vmproc[n];

    /* 2. VM 自身取消映射的特殊路径 */
    if(m->m_source == VM_PROC_NR) {                                        // :535
        if(!region_search_root(&vmp->vm_regions_avl)) {                    // :540
            munmap_vm_lin(addr, m->VMUM_LEN);                              // :541  ⚠️ addr 未初始化（UB）
        }
        else if((vr = map_lookup(vmp, addr, NULL))) {                      // :543  ⚠️ 同上
            if(map_unmap_region(vmp, vr, 0, m->VMUM_LEN) != OK) {
                printf("VM: self map_unmap_region failed\n");
            }
        }
        return SUSPEND;                                                    // :548 不回复
    }

    /* 3. 获取地址与长度 */
    if(m->m_type == VM_UNMAP_PHYS)
        addr = (vir_bytes) m->m_lsys_vm_unmap_phys.vaddr;                  // :551
    else if(m->m_type == VM_SHM_UNMAP)
        addr = (vir_bytes) m->m_lc_vm_shm_unmap.addr;                      // :554
    else addr = (vir_bytes) m->VMUM_ADDR;                                  // :555

    if(addr % VM_PAGE_SIZE) return EFAULT;                                 // :557-558

    if(m->m_type == VM_UNMAP_PHYS || m->m_type == VM_SHM_UNMAP) {          // :560
        if(!(vr = map_lookup(vmp, addr, NULL))) {                          // :562
            printf("VM: unmap: address 0x%lx not found in %d\n", addr, target);
            sys_diagctl_stacktrace(target);
            return EFAULT;                                                 // :566
        }
        len = vr->length;                                                  // :568 区域全长
    } else len = roundup(m->VMUM_LEN, VM_PAGE_SIZE);                       // :569 向上圆整

    return map_unmap_range(vmp, addr, len);                                // :571
}
```

**关键设计**：

1. **未知端点 panic**（:529-531）：C 对 `vm_isokendpt` 失败直接 panic——消息来源不可信时服务器崩溃。Rust 改为**软失败**（ProcessNotFound → EINVAL），可恢复性收紧（§3.6 #4）。
2. **VM 自身分支的 C UB**（:535-548）：`addr` 在第 3 步才被赋值，但此分支在**赋值之前**读取 `addr`（:541/:543）——未初始化读。这是 Minix3 的真实缺陷（该分支实际无调用者：libc munmap 的 m_source 是用户进程，VM 不会向自己发 VM_MUNMAP）。minix-rs 采信消息字段并实现语义正确版本（§3.2 D2），在差异清单诚实标注。**另一个差异**：C 此分支**吞掉所有错误**（`munmap_vm_lin` 返回值不检查、`map_unmap_region` 失败仅打印，恒 `return SUSPEND` 不回复）；Rust 用 `?` 传播错误（`MunmapOutcome::Suspended` 只在成功路径返回）——该分支当前不可达（backlog B3），差异仅存在于理论路径，行为契约以 SUSPEND 语义为准。
3. **UNMAP_PHYS/SHM_UNMAP 按区域整体**：`map_lookup` 失败 → 打印 + 栈回溯 + EFAULT；成功 → `len = vr->length`（无视消息 len，消息里也没有 len 字段）。
4. **VM_MUNMAP len 向上圆整**（:569）：`roundup` 宏——非页对齐长度被接受。POSIX 同样要求 munmap 长度圆整到页边界。

### 2.3 munmap_vm_lin（mmap.c:488-510）

```c
int munmap_vm_lin(vir_bytes addr, size_t len)
{
    if(addr % VM_PAGE_SIZE) return EFAULT;       // :491
    if(len % VM_PAGE_SIZE) return EFAULT;        // :496
    if(pt_writemap(NULL, &vmproc[VM_PROC_NR].vm_pt, addr, MAP_NONE, len, 0,
        WMF_OVERWRITE | WMF_FREE) != OK) {       // :500-501
        printf("munmap_vm_lin: pt_writemap failed\n");
        return EFAULT;
    }
    return OK;
}
```

VM 自身地址空间的**页表直清**路径：`WMF_FREE` 释放页表页。与 `map_unmap_range` 的本质区别——**不操作 `vm_regions_avl`**（VM 自身的地址空间可以不在区域管理里，如 Live Update 后的新 VM 实例）。Rust 等价：`vm_self_unmappages`（arch 页表直清，§3.2）。

### 2.4 map_unmap_range（region.c:1222-1294）

```c
int map_unmap_range(struct vmproc *vmp, vir_bytes unmap_start, vir_bytes length)
{
    vir_bytes o = unmap_start % VM_PAGE_SIZE, unmap_limit;

    unmap_start -= o;                  /* 向下页对齐 */
    length += o;
    length = roundup(length, VM_PAGE_SIZE);   /* 向上圆整 */
    unmap_limit = length + unmap_start;

    if(length < VM_PAGE_SIZE) return EINVAL;          // :1233 len=0 → EINVAL
    if(unmap_limit <= unmap_start) return EINVAL;     // :1234 溢出 → EINVAL

    /* 找第一个 ≤ unmap_start 的区域；无则找 > 的；再无 → OK（静默） */
    region_start_iter(&vmp->vm_regions_avl, &v_iter, unmap_start, AVL_LESS_EQUAL);
    if(!(vr = region_get_iter(&v_iter))) {
        region_start_iter(..., AVL_GREATER);
        if(!(vr = region_get_iter(&v_iter))) return OK;      // :1241-1243
    }

    for(; vr && vr->vaddr < unmap_limit; vr = nextvr) {
        this_unmap_start = MAX(unmap_start, vr->vaddr);
        this_unmap_limit = MIN(unmap_limit, vr->vaddr + vr->length);
        if(this_unmap_start >= this_unmap_limit) continue;

        if(this_unmap_start > vr->vaddr && this_unmap_limit < thislimit) {
            /* 中间掏空：先 split（:1263-1274） */
            split_region(vmp, vr, &vr1, &vr2, split_len);
            vr = vr1; thislimit = vr->vaddr + vr->length;
        }
        r = map_unmap_region(vmp, vr, this_unmap_start - vr->vaddr,
            this_unmap_limit - this_unmap_start);           // :1282
        if(r != OK) return r;
    }
    return OK;
}
```

**核心逻辑**：遍历重叠区域，对每个区域计算交叠子区间，再交给 `map_unmap_region`。中间掏空时需要先 `split_region` 把"交叠部分"变成独立区域（split 后取 vr1，交叠部分在 [vr1->vaddr, split_len) 内，从 offset=0 开始 unmap）。

### 2.5 map_unmap_region 三情形（region.c:1065-1147）

```c
int map_unmap_region(struct vmproc *vmp, struct vir_region *r,
    vir_bytes offset, vir_bytes len)
{
    if(offset+len > r->length || (len % VM_PAGE_SIZE)) return EINVAL;   // :1076
    regionstart = r->vaddr + offset;

    map_subfree(r, offset, len);          /* ① 先解除物理页引用 */

    if(r->length == len) {                /* 情形1：整体消失 */
        region_remove(&vmp->vm_regions_avl, r->vaddr);
        map_free(r);                       // :1088-1090
    } else if(offset == 0) {              /* 情形2：低端收缩 */
        if(!r->def_memtype->ev_lowshrink) return EINVAL;   // :1096
        if(r->def_memtype->ev_lowshrink(r, len) != OK) return EINVAL;
        region_remove(&vmp->vm_regions_avl, r->vaddr);
        r->vaddr += len;                   // vaddr 前移
        for(voffset = len; ...) pr->offset -= len;   /* 物理区域 offset 平移 */
        memmove(r->physblocks, r->physblocks + freeslots, ...);
        r->length -= len;
        region_insert(&vmp->vm_regions_avl, r);
    } else if(offset + len == r->length) { /* 情形3：高端收缩 */
        r->length -= len;                  // :1134 最便宜
    }

    if(pt_writemap(vmp, &vmp->vm_pt, regionstart, MAP_NONE, len, 0,
        WMF_OVERWRITE) != OK) return ENOMEM;   // :1139-1142
    return OK;
}
```

三种情形的复杂度差异（为什么低端收缩最贵）：

| 情形 | 触发 | AVL 操作 | physblocks | vaddr | 回调 |
|------|------|---------|-----------|-------|------|
| 整体 | len==length | remove + free | 全部释放 | — | ev_delete |
| 低端 | offset==0 | remove + 改 + insert | memmove 前移 | += len | **ev_lowshrink 必须** |
| 高端 | offset+len==length | 无 | 截断（length 减短） | — | 无 |

**为什么低端收缩需要 ev_lowshrink**：vaddr 前移后，memtype 内部状态（如 file 区域的 `offset += len`、shared 区域的源地址）必须同步；且这是"删掉区域前 len 字节"，与分裂语义不同。

### 2.6 split_region（region.c:1150-1220）

```c
static int split_region(struct vmproc *vmp, struct vir_region *vr,
    struct vir_region **vr1, struct vir_region **vr2, vir_bytes split_len)
{
    if(!vr->def_memtype->ev_split) {        // :1164 无回调 → EINVAL
        printf("VM: split region not implemented for %s\n", vr->def_memtype->name);
        return EINVAL;
    }
    r1 = region_new(vmp, vr->vaddr, split_len, vr->flags, vr->def_memtype);
    r2 = region_new(vmp, vr->vaddr+split_len, rem_len, vr->flags, vr->def_memtype);
    /* 逐个 phys_region：pb_reference（refcount++）到 r1/r2 */
    for(voffset...) pb_reference(ph->ph, voffset, r1, ph->memtype);
    for(voffset...) pb_reference(ph->ph, split_len + voffset, r2, ph->memtype);
    vr->def_memtype->ev_split(vmp, vr, r1, r2);   // 回调：file → fdref_ref ×2
    region_remove(...vr...); map_free(vr);
    region_insert(r1); region_insert(r2);
}
```

**关键**：split 通过 `pb_reference` **增加**引用计数（一份物理页现在被两个区域引用）——不是搬运，是复制引用。`ev_split` 回调处理 memtype 专属状态（mappedfile 的 fdref `fdref_ref` ×2、offset 分配，mem_file.c:262-276）。

### 2.7 map_subfree / map_free（region.c:527-585）

```c
static int map_subfree(struct vir_region *region, vir_bytes start, vir_bytes len)
{
    for(voffset = start; voffset < end; voffset += VM_PAGE_SIZE) {
        if(!(pr = physblock_get(region, voffset))) continue;
        pb_unreferenced(region, pr, 1);   /* refcount--，归零 → ev_unreference + free_mem */
        SLABFREE(pr);                     /* 释放 phys_region 结构 */
    }
}

int map_free(struct vir_region *region)
{
    map_subfree(region, 0, region->length);   // 全部页解除引用
    if(region->def_memtype->ev_delete)        // file → fdref_deref
        region->def_memtype->ev_delete(region);
    free(region->physblocks);
    SLABFREE(region);
}
```

`pb_unreferenced`（pb.c:96-134）：refcount--；从 `firstregion` 链表摘除；**refcount==0** → `ev_unreference`（memtype 决定是否释放物理页）+ `SLABFREE(pb)`。`map_free_proc`（region.c:589-612）是"全部区域"的循环版本（22 范围）。

### 2.8 do_map_phys 与 mem_type_directphys（mmap.c:310-363 + mem_directphys.c）

```c
int do_map_phys(message *m)
{
    target = m->m_lsys_vm_map_phys.ep;
    len = m->m_lsys_vm_map_phys.len;
    if (len <= 0) return EINVAL;                          // :323
    if(target == SELF) target = m->m_source;              // :325-326
    if((r=vm_isokendpt(target, &n)) != OK) return EINVAL; // :328

    startaddr = (vir_bytes)m->m_lsys_vm_map_phys.phaddr;
    if(map_perm_check(m->m_source, target, startaddr, len) != OK) {
        printf("VM: unauthorized mapping of 0x%lx by %d for %d\n", ...);
        return EPERM;                                     // :336-340
    }
    offset = startaddr % VM_PAGE_SIZE;
    len += offset;  startaddr -= offset;                  // 向下对齐 phys
    if(len % VM_PAGE_SIZE) len += VM_PAGE_SIZE - (len % VM_PAGE_SIZE);  // 向上圆整
    if(!(vr = map_page_region(vmp, VM_MMAPBASE, VM_MMAPTOP, len,
        VR_DIRECT | VR_WRITABLE, 0, &mem_type_directphys))) return ENOMEM;
    phys_setphys(vr, startaddr);                          // :356 VR 参数记录物理基址
    m->m_lsys_vm_map_phys.reply = (void *)(vr->vaddr + offset);  // :358 带偏移回复
    return OK;
}
```

`map_perm_check`（mmap.c:284-307）：TTY/MEM 豁免（TTY 可为任何人 TIOCMAPMEM，MEM 仅自身）；其余调用 `sys_privquery_mem(target, physaddr, len)` 由内核裁决（PCI 授权）。**Rust 中内核 syscall 未实现 → fail-closed 拒绝**（§3.5）。

`mem_type_directphys`（mem_directphys.c）——"映射但不管理"：

| 回调 | C 实现 | 语义 |
|------|--------|------|
| `ev_unreference` | `phys_unreference` :45-48 空操作 | 设备内存不归 VM 分配器管理 |
| `ev_pagefault` | `phys_pagefault` :50-61 直接填 `region->param.phys + ph->offset` | 缺页不分配，物理地址即答案 |
| `ev_copy` | `phys_copy` :74-78 复制 param.phys | fork 复制 VR_DIRECT |
| `writable` | `phys_writable` :63-67 `phys != MAP_NONE` | 已映射即可写 |
| `ev_split` | **无**（NULL） | split 区域 → EINVAL（region.c:1164） |
| `ev_lowshrink` | **无**（NULL） | 低端收缩 → EINVAL（region.c:1096） |
| `phys_setphys` | :69-72 `vr->param.phys = phys` | 记录物理基址（do_map_phys 调用） |

### 2.9 C 小结：符号全景

| 符号 | 位置 | 职责 |
|------|------|------|
| `do_munmap` | mmap.c:512-573 | 三入口统一编排（target/len/VM-self） |
| `munmap_vm_lin` | mmap.c:488-510 | VM 自身页表直清（WMF_FREE） |
| `map_unmap_range` | region.c:1222-1294 | 范围遍历 + 四情形分派 |
| `map_unmap_region` | region.c:1065-1147 | 单区域三情形（整体/低端/高端） |
| `split_region` | region.c:1150-1220 | 中间掏空的分裂（ev_split + pb_reference） |
| `map_subfree` | region.c:527-565 | 逐页 pb_unreferenced |
| `map_free` / `map_free_proc` | region.c:568-585 / 589-612 | 区域/全进程回收 |
| `pb_unreferenced` | pb.c:96-134 | refcount-- → ev_unreference |
| `do_map_phys` | mmap.c:310-363 | 物理映射建立（VR_DIRECT） |
| `phys_setphys` | mem_directphys.c:69-72 | VR param 记录物理基址 |
| `do_unmap_phys` | proto.h:80 | **死声明**（无定义，handler 是 do_munmap） |

---

## 3. Rust 设计决策

### 3.1 D1：wire format 修复（21-P1-1，20-P1-1 同族）

**问题**：`VmMunmapIn::decode(m1)` 从 `MessageM1` 解码（m1i1@0=endpoint、m1p1@16=addr、m1p2@24=length），但 C 的 VM_MUNMAP 线格式是 `mess_mmap`（addr u32@8、len u32@12）——错位读到了 `offset` 低位、`prot/flags`、`fd/forwhom`。VM_SHM_UNMAP 的 m1 解码同理（addr 应 @4，实际读 @16）。20-P1-1 已修 VM_MMAP/VM_VFS_MMAP/VM_MAP_PHYS/VM_REMAP 四个消息，**VM_MUNMAP/UNMAP_PHYS/SHM_UNMAP 是同一家族的漏网之鱼**。

**修复**（message.rs + vm.rs + dispatcher.rs 三处一致）：

1. `message.rs` 新增 `MessLsysVmUnmapPhys`（ep@0/vaddr@4，:1744-1762）与 `MessLcVmShmUnmap`（forwhom@0/addr@4，:1773-1791）两个 overlay，与 `m_lsys_vm_map_phys` 同风格。
2. `VmMunmapIn::decode_message`（vm.rs:791-812）：`endpoint = m_source`（C target 解析 :518-525）+ `addr/len` 从 `m_mmap` overlay 读。**废弃 M1 decode**（`DecodeFromM1 for VmMunmapIn` 移除）。
3. `VmUnmapPhysIn::decode_message`（vm.rs:309-331）/ `VmShmUnmapIn::decode_message`（vm.rs:333-351）：专用 overlay。
4. dispatcher 三个分支改用 `decode_message(msg)`（:1038/:1043/:1157）。

### 3.2 D2：handle_munmap 编排（munmap.rs:102-158）

承载 C `do_munmap`（mmap.c:512-573）：

```
handle_munmap(request)
  ├─ addr 页对齐校验 → BadAddress（C :557-558 EFAULT）
  ├─ vm_isokendpt → ProcessNotFound（C :529-531 panic → 软失败）
  ├─ endpoint == VM（VM 自身分支，C :535-548）：
  │    ├─ 区域树空 → munmap_vm_lin(addr, len)（C :540-541）
  │    ├─ find(addr) 命中 → unmap_range(addr, len)（C :543-544）
  │    └─ 返回 MunmapOutcome::Suspended（C :548 SUSPEND，不回复）
  ├─ length 解析：
  │    ├─ lookup_region_length（UNMAP_PHYS/SHM_UNMAP）→ find(addr) 区域全长；未命中 → NotMapped（C :560-566 EFAULT）
  │    └─ length==0 → InvalidLength（C map_unmap_range :1233 EINVAL）
  │        length>0 → roundup_page（C :569 roundup，非对齐长度接受）
  └─ unmap_range(addr, length) → Ok(MunmapOutcome::Replied)
```

**设计决策**：

- **VM 自身分支采信消息字段**：C 在 `addr` 赋值前读取（mmap.c:541-542 UB）。minix-rs 用 `request.addr`（消息字段），语义正确版本；`MunmapOutcome::Suspended` 让 dispatcher 返回 `VmReply::Suspend`（与 mmap 的 `MmapResult::Suspended` 同构）。
- **len roundup**：C 接受非页对齐 len（向上圆整，:569）。Rust 原实现返回 `InvalidLength`——**外部行为偏差**（用户 `munmap(ptr, 0x100)` 在 Minix3 成功）。本轮改为 `roundup_page`（§4.5），与 POSIX 一致。
- **错误映射**（dispatcher.rs:1243-1253）：ProcessNotFound → `InvalidProcess`（EINVAL）；BadAddress/NotMapped → `InvalidAddress`（EFAULT）；**InvalidLength → `InvalidParam`（EINVAL）**（C map_unmap_range length<页 / 溢出、map_unmap_region len 非对齐 均 EINVAL——review 修复，原错误映射 EFAULT）；MemTypeNotSupported → `InvalidParam`（EINVAL）；InternalError → `InternalError`（EIO）。

### 3.3 D3：unmap_range 四情形（munmap.rs:173-297）

承载 C `map_unmap_range` + `map_unmap_region`（region.c:1222-1294/:1065-1147）。先收集重叠区域 vaddr（避免迭代中修改集合），再逐个处理：

| Rust 分支 | C 对应 | 操作 |
|-----------|--------|------|
| `unmap_start<=vaddr && unmap_end>=end` | map_unmap_region 情形1 | remove + `free_region_pages`（整体） |
| `unmap_start>vaddr && unmap_end<end` | split_region + 情形1 | `split(head_len)` + `split(length)` → free 中间 → insert 左右 |
| `unmap_start<=vaddr && unmap_end<end` | map_unmap_region 情形2（低端） | `split(cut_len)` → free 头 → insert 尾 |
| `unmap_start>vaddr && unmap_end>=end` | map_unmap_region 情形3（高端） | `split(head_len)` → free 尾 → insert 头 |

**设计差异**：C 的低端收缩走 `ev_lowshrink`（vaddr 前移 + physblocks memmove），Rust 统一用 `VirRegion::split` 表达"保留剩余部分"——split 已处理 fdref 引用计数分裂（vir_region.rs:279-350，`VrParam::File` 分支 ref_entry ×1，**review 修复：×2 在头/尾切割时泄漏 1 个 fdref**）与 slot 搬运，**结构上消除了 ev_lowshrink 专用路径**（mappedfile 的 `offset += len` 语义由 split 的 offset 分配覆盖：右半区 `offset += split_len`）。这是结构简化（Rewrite 允许：外部行为等价），但 memtype 的 `ev_low_shrink`/`ev_split` trait 方法仍保留（default `NotSupported`），供仍需要它的类型使用。**review 补充能力门控**：中间掏空要求 `supports_split`、头部切割要求 `supports_low_shrink`（C region.c:1164/:1096 的 EINVAL 限制；directphys/shared 缺回调 → EINVAL，直接映射区域不允许分裂——也避免分裂后 `VrParam::Direct` 物理基址错位）。**文档 §3.6 #7/#15 诚实标注**。

无重叠 → 静默 `Ok(Replied)`（C :1238-1243）。每区域 `active.sub_total(freed_len)` 维护 `vm_total` 记账。溢出防护：`unmap_end <= unmap_start` → InvalidLength（C :1234 EINVAL）。

### 3.4 D4：物理页释放链与 fdref（free_region_pages）

`free_region_pages`（region/mod.rs:23-81）是统一释放入口（munmap/brk shrink/exit 共用）：

```
free_region_pages(region, pt, frames, page_alloc)
  ① 逐页 pt.unmap(vaddr)（非 test 构建；C pt_writemap MAP_NONE WMF_OVERWRITE :1139-1142）
  ② mt.ev_delete(region)（file → FdRefTable::deref_entry → PendingFdClose，VFS IPC DEFERRED）
  ③ region.free_range(frames, 0, length) → 逐页 unmap_page：
       refcount--；归零且非 IN_CACHE → 返回 (pfn, mt)
  ④ 每 (pfn, mt)：mt.ev_unreference(frames, pfn) + page_alloc.free_pfn(pfn)
```

**与 C 的对应**：`map_subfree`（region.c:527-565）→ ③；`ev_unreference`（pb.c:96-134 内）→ ④ 前半；`free_mem` → `PfnAllocator::free_pfn`（**分配职责集中到分配器**，PFN 索引模型的结构差异——11/05 范围已确立）。`ev_delete`（mem_file.c:280-287 `mappedfile_delete` → `fdref_deref`）→ ②：file 区域最后一个引用消失 → VFS_FDCLOSE 异步通知（fdref.c:150-154）；Rust `PendingFdClose` 本地暂存，真实发送依赖 IPC transport（23 范围，backlog B2）。

**refcount 语义验证**（§5.1 测试）：独占页 1→0 释放；CoW 共享页 2→1 保留（`test_munmap_cow_shared_page_kept`）。

**fdref 引用平衡**（review 修复，21-P1-5）：不变量是"每个存活区域持 1 个 fdref"。`VirRegion::split` 把 1 个区域变成 2 个——净增 1。原实现 `ref_entry ×2` 净增 2：头/尾切割（立即释放一半）后剩 1 个区域却持 2 个引用，fdref 永不归零 → `VFS_FDCLOSE` 永不发送（fd 泄漏）。已改为 `ref_entry ×1` + `test_munmap_file_head_cut_fdref_balanced` 回归（§5.1）。

### 3.5 D5：map_phys 常量复用与权限 fail-closed（map_phys.rs:48-121）

`handle_map_phys` 承载 C `do_map_phys`（mmap.c:310-363）：

- **常量复用**：C 用运行时 `VM_MMAPBASE/VM_MMAPTOP`（32 位计算）；Rust 固定 64 位 `MMAP_BASE/MMAP_TOP`（mmap.rs:203-204，[ARCH: A-6]）。原实现硬编码字面量 `0x0000_0001_0000_0000`——与 mmap.rs 重复（const 权威位置问题，review-doc-skill §2.4g）。本轮改为 `crate::mmap::MMAP_BASE/MMAP_TOP` 引用（§4.6）。
- **权限**：`map_perm_check`（map_phys.rs:104-121）——TTY/MEM 豁免（C mmap.c:292-297）；其余 `sys_privquery_mem` 内核 syscall 未实现 → **fail-closed 拒绝**（PermissionDenied → EPERM，backlog B1）。与 20 的 B4 同源。
- **对齐与回复**：`offset = phys % PAGE_SIZE`；`startaddr` 向下对齐；`aligned_len` 向上圆整；`find_slot` 分配；`VR_DIRECT|VR_WRITABLE + MEM_TYPE_DIRECT + VrParam::Direct{phys: startaddr}`（C `phys_setphys` 的 Rust 表达）；回复 `vaddr + offset`（C :358）。
- **错误映射**（dispatcher.rs 对应 From impl）：InvalidLength → `InvalidParam`（EINVAL，C :323）；ProcessNotFound → `InvalidProcess`（EINVAL，C :328）；OutOfMemory → `OutOfMemory`（ENOMEM，C :351-354）；PermissionDenied → `PermissionDenied`（EPERM，C :336-340）。

### 3.6 差异清单（C ↔ Rust，诚实标注）

| # | C 行为 | Rust 行为 | 判定 |
|---|--------|----------|------|
| 1 | VM_MUNMAP 消息 `mess_mmap` overlay（addr@8/len@12）+ target=m_source | 原 M1 stub decode 错位 → 本轮 `decode_message` 修复 | **修复（21-P1-1）** |
| 2 | VM_UNMAP_PHYS/SHM_UNMAP 消息 `ep/vaddr@4`、`forwhom/addr@4` | 原 M1 decode addr@16 错位 → 本轮专用 overlay | **修复（21-P1-1）** |
| 3 | VM_UNMAP_PHYS 已 CALLMAP（main.c:540） | 原 `dispatch_by_number` 无分支 → NotImplemented | **修复（21-P1-2，本轮接线）** |
| 4 | do_munmap 未知端点 panic（mmap.c:529-531） | 软失败 ProcessNotFound → EINVAL | 收紧（可恢复） |
| 5 | VM_MUNMAP len 非页对齐 roundup 接受（:569） | 原 EFAULT 拒绝 → 本轮 roundup_page | **修复（21-P1-3）** |
| 6 | VM 自身分支读取未初始化 `addr`（mmap.c:541-542 UB） | 采信消息字段 + SUSPEND 语义 | 修复（标注 C UB） |
| 7 | 低端收缩 ev_lowshrink（region.c:1092-1119） | 统一 `VirRegion::split` 表达（fdref offset 分配覆盖） | 结构简化（外部等价；review 补能力门控 #15） |
| 8 | map_perm_check 内核 sys_privquery_mem | fail-closed 拒绝（TTY/MEM 豁免） | 缺口（backlog B1） |
| 9 | map_phys MMAPBASE/MMAPTOP 运行时计算 | 复用 mmap.rs 常量（[ARCH: A-6]） | 等价（本轮修重复） |
| 10 | munmap_vm_lin WMF_FREE 页表页释放 | `vm_self_unmappages`（arch 页表直清） | 等价（arch 抽象） |
| 11 | 物理页释放经 `free_mem` | `PfnAllocator::free_pfn` | 等价（分配器抽象） |
| 12 | fdref 归零 → VFS_FDCLOSE 直发 | `PendingFdClose` 本地暂存（transport 未接线） | 缺口（backlog B2，23 范围） |
| 13 | len=0/溢出/len 非对齐 → EINVAL（region.c:1233/:1234/:1076） | 原 InvalidLength → EFAULT；review 改为 `InvalidParam`（EINVAL） | **修复（21-P1-4）** |
| 14 | split fdref 引用净增 1（C mappedfile_split +2 / map_free −1） | 原 `ref_entry ×2` 净增 2 → 头/尾切割 fdref 泄漏；改 ×1 | **修复（21-P1-5）** |
| 15 | 中间掏空/头部切割需 memtype 回调（region.c:1164/:1096 EINVAL） | 原统一 split 绕过门控（directphys/shared/cache 可分裂）；review 补 `supports_split`/`supports_low_shrink` 门控 | **修复（21-P0-1）** |

---

## 4. 实现详解

### 4.1 消息路径（dispatcher.rs:1038-1162 → munmap.rs）

```
VM_MUNMAP      → dispatch_by_number :1038 → VmMunmapIn::decode_message → dispatch_munmap :142
VM_UNMAP_PHYS  → dispatch_by_number :1043 → VmUnmapPhysIn::decode_message → dispatch_unmap_phys :164
VM_SHM_UNMAP   → dispatch_by_number :1157 → VmShmUnmapIn::decode_message → dispatch_shm_unmap :186
VM_MAP_PHYS    → dispatch_by_number → VmMapPhysIn::decode_message → dispatch_map_phys :433
```

三个 dispatch 函数把解码后的请求构造成 `MunmapRequest`，调 `handle_munmap`，按 `MunmapOutcome` 决定回复：

```rust
match munmap::handle_munmap(table, page_alloc, frames, &req) {
    Ok(MunmapOutcome::Replied)   => VmReply::Munmap,
    Ok(MunmapOutcome::Suspended) => VmReply::Suspend,   // VM 自身分支
    Err(e)                       => VmReply::Error(e.into()),
}
```

`dispatch_unmap_phys`/`dispatch_shm_unmap` 构造 `MunmapRequest { lookup_region_length: true }`（长度由区域决定，C :560-568）；`dispatch_munmap` 用 `lookup_region_length: false`。

### 4.2 伪码总结（munmap.rs:102-297）

```
handle_munmap(req):
  addr 非页对齐 → BadAddress
  slot = vm_isokendpt(req.endpoint)?
  if req.endpoint == VM:                      # VM 自身（C :535-548）
      if 区域树空: munmap_vm_lin(addr, len)?  # 页表直清
      elif find(addr): unmap_range(addr, len)?
      return Suspended
  length = req.lookup_region_length
      ? find(addr)?.length（未命中 → NotMapped）
      : req.length==0 → InvalidLength
        else roundup_page(req.length)
  unmap_range(addr, length)?; return Replied

unmap_range(addr, length):
  unmap_end = addr + length（溢出 → InvalidLength）
  vaddrs = 所有与 [addr, unmap_end) 重叠的区域 vaddr
  空 → Ok                                        # C 静默
  for vaddr in vaddrs:
      region = remove(vaddr)
      if 整体包含:     free_region_pages(region); sub_total
      elif 中间掏空:   split×2 → free(middle) → insert(left,right)
      elif 头部切割:   split → free(head) → insert(tail)
      elif 尾部切割:   split → free(tail) → insert(head)
  Ok
```

### 4.3 21-P1-1 wire format 修复记录（本轮）

- **症状**：VM_MUNMAP/UNMAP_PHYS/SHM_UNMAP 的 Rust 解码与 C 线格式错位（与 20-P1-1 同族）。
- **根因**：三个消息沿用 `MessageM1` stub 解码，未使用专用 overlay。
- **修复**：message.rs 新增 2 个 overlay（`MessLsysVmUnmapPhys`/`MessLcVmShmUnmap`）+ union 臂；vm.rs 新增/重写 3 个 `decode_message`；dispatcher 3 个分支改用 `decode_message(msg)`。
- **回归**：minix-types 3 个新 decode 测试（§5.1）；vm_server 1 个 UNMAP_PHYS 接线测试。

### 4.4 21-P1-2 VM_UNMAP_PHYS 接线修复记录（本轮）

- **症状**：`dispatch_unmap_phys` 已存在（dispatcher.rs:164）但 `dispatch_by_number` 无 VM_UNMAP_PHYS 分支——请求落入 `_` catch-all → NotImplemented。checklist I-006 声称"已实现"与实际不符。
- **修复**：新增分支（dispatcher.rs:1043），与 C `CALLMAP(VM_UNMAP_PHYS, do_munmap)`（main.c:540）对齐。
- **回归**：`test_dispatch_vm_unmap_phys_wired`（vm_server.rs）断言不再返回 NotImplemented。

### 4.5 21-P1-3 len roundup 语义修复记录（本轮）

- **症状**：`handle_munmap` 对非页对齐 len 返回 `InvalidLength`（EFAULT）；C `roundup` 接受（mmap.c:569）。
- **修复**：`roundup_page` 向上圆整（munmap.rs:98-100）；`test_munmap_unaligned_len_rounds_up` 回归。

### 4.6 map_phys 常量复用修复记录（本轮）

- **症状**：`handle_map_phys` 硬编码 `0x0000_0001_0000_0000`/`0x0000_0200_0000_0000`，与 mmap.rs:203-204 重复（const 权威位置问题）。
- **修复**：`MMAP_BASE/MMAP_TOP` 升为 `pub(crate)`，map_phys.rs 引用。


### 4.7 review 轮修复记录（2026-08-16 回归深度 full-review）

- **21-P0-1 memtype 能力门控缺失**：Rust 的 `unmap_range` 统一用 `VirRegion::split`，未复刻 C 的 memtype 回调门控（region.c:1164 `ev_split` / :1096 `ev_lowshrink`）。directphys/shared/cache/anon_contig 的局部拆除在 C 返回 EINVAL，Rust 原实现静默成功；其中 **VR_DIRECT 分裂还会让右半区沿用同一物理基址**（缺页时映射错误设备页）。修复：`MemType` trait 新增 `supports_split`/`supports_low_shrink` 谓词（memtype.rs:90-107），`unmap_range` 在中间掏空/头部切割前检查（munmap.rs:227-232/:254-259）；新增 `MunmapError::MemTypeNotSupported` → EINVAL；回归测试 ×3（`test_munmap_{middle_hole,head_cut}_directphys_rejected` + `test_munmap_tail_cut_directphys_allowed`）。
- **21-P1-5 split fdref 引用泄漏**：`VirRegion::split` 原 `ref_entry ×2` 使 fdref 净增 2（应为 1：1 区域 → 2 区域）；头/尾切割立即释放一半后剩 1 区域持 2 引用，fdref 永不归零 → `VFS_FDCLOSE` 永不发送。修复：`ref_entry ×1`（vir_region.rs:321-333）；回归测试 `test_munmap_file_head_cut_fdref_balanced`。
- **21-P1-4 errno 映射修正**：`MunmapError::InvalidLength` 原映射 `InvalidAddress`（EFAULT），但 C 在 len=0（region.c:1233）、溢出（:1234）、len 非对齐（:1076）三处均返回 **EINVAL**。修复：`InvalidLength → VmError::InvalidParam`（dispatcher.rs:1247-1251）；`munmap_vm_lin` 非对齐 len 改用 `BadAddress`（C mmap.c:496 EFAULT）；`test_munmap_error_to_errno` 断言更新。doc §5.1 "len=0 → InvalidLength（C EINVAL）" 由此自洽。
- **行号漂移**：§5.1 测试表 15 行 + §2.2 代码块注释 + 头部范围全部按 `rg`/`nl -ba` 实证重算（munmap.rs 因 review 修复行号再偏移）；design 快照 21-design.v1.md 同步修正（§3.6 引用面）。

---

## 5. 测试要点


### 5.1 单元测试清单（grep 实证，2026-08-16）

| 测试 | 位置 | 覆盖 |
|------|------|------|
| `test_munmap_unaligned_addr_returns_bad_address` | munmap.rs:373 | addr 非页对齐 → BadAddress（C EFAULT） |
| `test_munmap_zero_length_returns_invalid_length` | munmap.rs:392 | len=0 → InvalidLength（C EINVAL） |
| `test_munmap_no_region_silent_ok` | munmap.rs:411 | 无重叠区域 → 静默 Replied（C OK） |
| `test_munmap_unaligned_len_rounds_up` | munmap.rs:430 | 非页对齐 len 圆整后拆除（C roundup） |
| `test_munmap_whole_region` | munmap.rs:454 | 整体包含 → 区域移除 |
| `test_munmap_head_cut` | munmap.rs:474 | 头部切割 → 保留尾区域（vaddr/length 断言） |
| `test_munmap_tail_cut` | munmap.rs:502 | 尾部切割 → 保留头区域 |
| `test_munmap_middle_hole` | munmap.rs:530 | 中间掏空 → 两个区域 [0x1000,0x3000) |
| `test_munmap_cross_regions` | munmap.rs:557 | 范围跨多个区域 → 全部移除 |
| `test_munmap_unmap_phys_region_length` | munmap.rs:579 | UNMAP_PHYS/SHM_UNMAP 按区域全长（C :568） |
| `test_munmap_unmap_phys_not_mapped` | munmap.rs:601 | 区域查找失败 → NotMapped（C EFAULT） |
| `test_munmap_vm_self_suspended` | munmap.rs:620 | VM 自身分支（区域存在路径）→ unmap_range → Suspended（C :548）|
| `test_munmap_releases_physical_page` | munmap.rs:640 | 本地区域：refcount 1→0 + free_pfn |
| `test_munmap_cow_shared_page_kept` | munmap.rs:662 | CoW 共享页 2→1 保留 / 1→0 释放 |
| `test_munmap_middle_hole_directphys_rejected` | munmap.rs:696 | memtype 门控：VR_DIRECT 中间掏空 → MemTypeNotSupported（C EINVAL，region.c:1164） |
| `test_munmap_head_cut_directphys_rejected` | munmap.rs:717 | memtype 门控：VR_DIRECT 头部切割 → MemTypeNotSupported（C EINVAL，region.c:1096） |
| `test_munmap_tail_cut_directphys_allowed` | munmap.rs:738 | 尾部切割无需回调（C region.c:1132-1135），Direct 保留物理基址 |
| `test_munmap_file_head_cut_fdref_balanced` | munmap.rs:769 | 头部切割 fdref 1 区域 1 引用 → 归零触发 VFS_FDCLOSE（21-P1-5） |
| `test_munmap_error_to_errno` | munmap.rs:808 | 全错误路径 errno（EFAULT/EINVAL/EIO） |
| `test_map_phys_basic/zero_length/error_to_errno` | map_phys.rs:151-179 | 基本/零长度/errno（EPERM/ENOMEM/EINVAL） |
| `test_vm_munmap_in_decode_message` | vm.rs:1232 | 21-P1-1：m_mmap overlay + m_source |
| `test_vm_unmap_phys_in_decode_message` | vm.rs:1256 | 21-P1-1：专用 overlay（ep/vaddr@4） |
| `test_vm_shm_unmap_in_decode_message` | vm.rs:1277 | 21-P1-1：专用 overlay（forwhom/addr@4） |
| `test_dispatch_vm_unmap_phys_wired` | vm_server.rs:1323 | 21-P1-2：不再 NotImplemented |

### 5.2 覆盖维度

- **四情形**：整体 / 头部 / 尾部 / 中间掏空——各一个结构断言测试（区域数 + vaddr/length）。
- **入口差异**：MUNMAP（len 圆整）vs UNMAP_PHYS/SHM_UNMAP（区域全长 + 未映射 EFAULT）。
- **物理页语义**：独占释放 / CoW 共享保留（refcount 2→1→0）。
- **错误语义**：6 条 MunmapError + 4 条 MapPhysError errno 路径全部单测断言（InvalidLength → EINVAL 修正后，C 语义逐条对齐）。
- **wire format**：3 个 decode_message 回归 + 1 个接线测试（21-P1-1/P1-2）。
- **VM 自身分支**：区域存在路径 → unmap_range → Suspended（`test_munmap_vm_self_suspended`，确定性断言）；空区域路径（`munmap_vm_lin` → `vm_self_unmappages`）需真实 self-PT，单测环境不可达（返回 InternalError，backlog B3）。
- **memtype 门控**：VR_DIRECT 中间掏空/头部切割 → EINVAL；尾部切割允许（C 回调矩阵，§2.8）。
- **fdref 平衡**：头部切割 split 后 1 区域 1 引用，归零触发 PendingFdClose（21-P1-5）。

### 5.3 覆盖缺口与诚实标注

| # | 缺口 | 原因 | 归属 |
|---|------|------|------|
| B1 | map_perm_check 内核裁决 | `sys_privquery_mem` 未实现 → fail-closed | backlog（26/内核） |
| B2 | fdref 归零 VFS_FDCLOSE 真实发送 | KernelIpcTransport 未实现 | backlog（23） |
| B3 | VM 自身分支真实可达 | 需 VM 向自己发 VM_MUNMAP（transport） | backlog（26） |
| B4 | TLB 批量失效 | arch 页表层逐页 unmap（08 范围） | backlog（08/arch） |
| B5 | 页缓存页面拆除交互 | mem_type_cache 无 ev_delete（mem_cache.c:39-49）；页面经 cache_unreference（≡ anon ev_unreference）释放 | 24 范围 |

### 5.4 测试统计（截至 2026-08-16）

- `cargo test -p minix-vm --lib`：**390 passed / 1 failed**（写作轮基线 374/1；写作轮 +12 = 386，review 轮 +4（门控 ×3 + fdref 平衡 ×1）= 390；唯一失败 `region::vir_region::tests::test_map_lazy` 归 13 范围 pre-existing）。
- `cargo test -p minix-types --lib`：**83 passed / 0 failed**（上轮 80；写作轮 +3 个 decode_message 回归；review 轮未动 minix-types）。
- 定向：`cargo test -p minix-vm --lib munmap` → 19 passed；`cargo test -p minix-vm --lib map_phys` → 3 passed（+3 个误匹配）；`cargo test -p minix-vm --lib unmap_phys` → 3 passed。

---

## 6. 过渡

### 6.1 位置可回答性

本文档是阶段 7（IPC 服务）的"映射拆除"篇。主循环中的位置：`do_munmap`/`do_map_phys` 经 CALLMAP 分发（main.c:538-540/:564），与 mmap（20）构成"建立/拆除"闭环。

- **前置可回答**：区域怎么建（13）、怎么找（14）、消息怎么分发（15）、映射怎么建（20）、六种 memtype 是什么（12）、directphys 缺页怎么填（16）。
- **本文档回答**：映射怎么拆除（四情形 + 三情形）、物理页引用怎么解除（refcount）、设备内存为什么免释放（directphys）、物理映射怎么建立（map_phys）。
- **未回答（移交）**：进程退出全区域释放（22 `map_free_proc`）；fdref 表生命周期与 VFS 对话（23）；页缓存拆除交互（24）；`do_get_phys`/`do_get_refcount` 查询（26）。

### 6.2 下游移交（对照 plan.md §3.4）

| 移交项 | 目标文档 | 交接内容 |
|--------|---------|---------|
| 全区域释放 | 22 | `map_free_proc`（region.c:589-612）逐区域 `map_free`，本文档已铺垫 |
| fdref 归零 VFS 对话 | 23 | `PendingFdClose` 暂存 → VFS_FDCLOSE 发送（fdref.c:150-154） |
| 页缓存拆除 | 24 | cache 区域 ev_delete/ev_unreference 的缓存交互 |
| 查询 | 26 | `do_get_phys`/`do_get_refcount`（mmap.c:438-485） |
| TLB/arch | 08 | 逐页 `pt.unmap` 与批量失效的 arch 层实现 |

---

## 7. 参见

- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/13-region-mapping.md`——区域结构与 `map_unmap_*` 底层操作（四情形的落点）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/20-vm-mmap.md`——mmap 建立（munmap 的镜像，map_phys 移交确认）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/15-ipc-dispatch.md`——主循环分发与 `VmReply::Suspend` 语义
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/12-memtype.md`——directphys/shared/mappedfile 回调族（ev_unreference/ev_split/ev_lowshrink）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/11-phys-pagestate.md`——refcount 语义与物理页生命周期
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/22-vm-exit.md`——进程退出全区域释放（`map_free_proc`）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/23-vfs-interaction.md`——fdref 表与 VFS 异步对话（PendingFdClose 归属）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/24-page-cache.md`——页缓存（cache 区域拆除交互）
- `os/servers/vm/src/munmap.rs`、`os/servers/vm/src/map_phys.rs`——Rust 实现
- `minix3/minix/servers/vm/mmap.c`、`region.c`、`mem_directphys.c`——C 真相源
