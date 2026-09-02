# 26-vm-queries: 查询类服务 —— VM 作为内存权威的只读窗口

> **分类**: 阶段 8 — 运行时服务（查询：INFO / GETPHYS / GETREF / GETRUSAGE）
> **源码**: `minix3/minix/servers/vm/utility.c`（`do_info` :100-184 / `do_getrusage` :426-472）+ `minix3/minix/servers/vm/mmap.c`（`do_get_phys` :438-458 / `do_get_refcount` :463-483）+ `minix3/minix/servers/vm/region.c`（`map_get_phys` :1323-1335 / `map_get_ref` :1343-1355 / `get_usage_info_kernel` :1357-1364 / `get_usage_info_vm` :1366-1373 / `is_stack_region` :1384-1390 / `get_usage_info` :1395-1447 / `get_region_info` :1452-1505）+ `minix3/minix/servers/vm/cache.c`（`get_stats_info` :328-331）+ 接口面（`minix/include/minix/com.h`：请求码 :720/:722/:729/:764、VMIW_* :732-734；`minix/include/minix/vm.h`：`vm_stats_info` :40-46 / `vm_usage_info` :48-57 / `vm_region_info` :59-64 / `MAX_VRI_COUNT` :66；`minix/lib/libsys/vm_info.c`：libc 封装 :10-57）+ 回调面（`mem_anon.c`：`anon_regionid` :132-135 / `anon_refcount` :142-145；`mem_shared.c`：`shared_regionid` :99-108 / `shared_refcount` :207-210）
> **Rust 模块**: `os/servers/vm/src/query.rs`（`QueryError` :47 / `InfoQuery` :92 / `StatsInfo` :99 / `UsageInfo` :122 / `RegionInfo` :141 / `InfoResult` :149 / `UsageSources` :183 / `handle_get_phys` :203 / `handle_get_refcount` :241 / `handle_info` :286 / `used_page_range` :473 / `handle_getrusage` :496 / `MAX_VRI_COUNT` :26）+ `os/servers/vm/src/ipc/dispatcher.rs`（`dispatch_get_phys` :923 / `dispatch_get_refcount` :935 / `dispatch_info` :948 / `dispatch_getrusage` :997 / CALLMAP 解码 :1142-1183 / `From<QueryError>` :1353 / `query_rusage_error_to_vm_error` :1370）+ `os/servers/vm/src/vm_server.rs`（`usage_sources` :1032 / `encode_reply_data` 查询分支 :1280-1420）+ `os/servers/vm/src/memtype.rs`（`supports_region_id` :146/:316/:496 / `supports_ref_count` :151/:321/:501）+ `os/servers/vm/src/boot.rs`（`vm_allocated_bytes` :87）+ `os/libs/minix-types/src/ipc/vm.rs`（`VmRegionInfo` :615 / `VmReply` 变体 :636-706）
> **前置**: `13-region-mapping.md`（区域查找/physblocks）、`15-ipc-dispatch.md`（主循环分发）
> **说明**: 本文档管 **VM 的 4 个只读查询服务**——内存统计（INFO/STATS）、进程内存使用（INFO/USAGE，含内核与 VM 自身特判）、区域列表（INFO/REGION 分页）、物理地址/引用计数（GETPHYS/GETREF，实为 anon/shared 区域的 region id 与 remaps 计数）、资源使用（GETRUSAGE，PM-only）。**不覆盖**：区域生命周期与 physblocks 语义本体（13）、页缓存统计口径（24）、getrusage 计数器的清零时机（22）、主循环分发框架（15）、memtype 回调定义本体（12）。

---

## 1. 概念：VM 作为内存权威的只读窗口

### 1.0 章节引言

**目标读者**：已读完 13（区域模型）、15（主循环分发）、22（进程退出与 rusage 清零）的读者。本文档回答四个问题：为什么"查询"必须由 VM 提供？四个查询服务各回答什么系统问题？"物理地址查询"为什么名不副实？Rust 重写如何在不引入跨地址空间复制的前提下保持相同的查询契约？

**本章不讲什么**：区域如何在进程间复制/分裂（13）；主循环如何把消息分发到 handler（15）；`cached_pages` 的增减口径（24）；`vm_total_max`/页错误计数器的清零时机（22）；memtype 回调（`regionid`/`refcount`）的定义本体（12）——这里只讲"外部组件如何问、VM 如何答"这一侧。

### 1.1 为什么查询服务是 VM 的职责

VM 是 Minix3 中**内存状态的唯一权威**：虚拟地址→物理页的映射、物理页的引用计数、每个进程的区域清单、全局空闲页统计，全部只存在于 VM 的内部数据结构中。外部组件（MIB、procfs、VFS coredump、IPC shm、调试工具）需要这些信息，但**不能也不应直接访问 VM 的地址空间**——微内核的进程隔离要求所有信息跨越 IPC 边界。

于是 VM 提供 4 个只读、无副作用的查询服务：

| 服务 | 请求码 | 回答的系统问题 | 典型消费者 |
|------|--------|---------------|-----------|
| VM_INFO | com.h:729 | 内存有多忙？进程用了多少？它的地址空间长什么样？ | MIB（hw.c:26/:54/:59、proc.c:621）、procfs（pid.c:213）、VFS coredump（coredump.c:205）、`is` 调试 |
| VM_GETPHYS | com.h:720 | 这个虚拟地址属于哪个区域？区域的标识是什么？ | IPC shm（shm.c:118，共享内存令牌） |
| VM_GETREF | com.h:722 | 这个区域被重映射过多少次？ | 调试、内存分析 |
| VM_GETRUSAGE | com.h:764 | 这个进程的峰值内存与缺页计数？ | PM（getrusage(2) 系统调用） |

三个特征让这组服务与 16~25 的其他服务区分开：**只读**（不修改 VM 状态）、**无分配**（不触发物理页分配或区域创建）、**可重入安全**（查询期间 VM 状态不变化，无 SUSPEND/回调）。

**对照另两家 OS**：
- **Linux**：查询面 = 内核内嵌的 `/proc`/`/sys` 文件系统 + `getrusage(2)` 系统调用。内存权威在内核，查询是"读内核文件"，无 IPC 往返；`/proc/PID/smaps` 对应 INFO/REGION，`/proc/meminfo` 对应 INFO/STATS，`getrusage` 对应 GETRUSAGE。
- **Redox**：内存管理在内核（无 VM server），`syscall::sys_memory_usage` 直接读内核记账；进程地址空间查询通过内核 syscall + scheme 文件接口。Redox 没有"另一个进程替你查询"的 IPC 服务面——查询者即内核调用者。
- **Minix3 的位置**：内存权威被刻意放在**用户态 VM server** 里，所以查询必须跨 IPC——这是微内核"最小可信计算基"取舍的直接后果。Rust 重写**保留了这个外部契约**（4 个请求码、消息字段、errno 语义不变），差异只在 VM 内部如何组织数据。

### 1.2 四个服务的请求面

```
外部组件（MIB / procfs / VFS / PM / IPC shm）
  │  _taskcall(VM_PROC_NR, 请求码, &m)      ← libsys/vm_info.c 封装
  ▼
VM 主循环（15-ipc-dispatch §1.3）
  ├─ VM_INFO     → dispatch_info     → handle_info      → VmReply::InfoStats/Usage/Region
  ├─ VM_GETPHYS  → dispatch_get_phys → handle_get_phys  → VmReply::GetPhys
  ├─ VM_GETREF   → dispatch_get_refcount → handle_get_refcount → VmReply::GetRefcount
  └─ VM_GETRUSAGE→ dispatch_getrusage → handle_getrusage → VmReply::Getrusage
```

**VM_INFO 的三模式**（`what` 字段，com.h:732-734）：

| 模式 | 常量 | 回答的问题 | 数据来源 |
|------|------|-----------|---------|
| STATS | `VMIW_STATS` (1) | 物理内存全局状态 | `memstats()` + `get_stats_info()` |
| USAGE | `VMIW_USAGE` (2) | 单个进程（或内核/VM）的使用量 | `get_usage_info()` / `get_usage_info_kernel()` / `get_usage_info_vm()` |
| REGION | `VMIW_REGION` (3) | 单个进程的区域列表（分页） | `get_region_info()` |

USAGE 模式有一个 C 侧的特殊约定：**`ep < 0` 表示内核**（内核任务端点都是负数），返回内核自身占用；**`ep == VM_PROC_NR` 表示 VM 自身**。REGION 模式则允许调用者用 `ep == SELF` 指代自己（utility.c:141-143）。

### 1.3 "物理地址查询"的真相：region id 不是物理地址

`VM_GETPHYS` 的名字极具误导性。C 侧 `map_get_phys` 最终调用 memtype 的 `regionid` 回调：

- `anon_regionid` 返回 `region->id`（mem_anon.c:132-135）——**区域的唯一编号**，与物理地址无关；
- `shared_regionid` 返回**源区域**的 id（mem_shared.c:99-108）；
- 只有 anon/shared 两类区域实现了 `regionid` 回调（mem_anon.c:42、mem_shared.c:35）；direct/contig/cache/file 类区域**没有**该回调 → `map_get_phys` 返回 EINVAL。

真正的消费者是 **IPC shm 服务**：`shmget` 时 mmap 一块匿名内存，用 `vm_getphys(sef_self(), page)` 拿到该区域的 id 存为 `vm_id`（shm.c:112-118）；`shmat` 时再用 `vm_remap` 把共享区域映射到客户端。**region id 是跨进程传递的共享内存令牌**，物理地址反而没有意义（物理页可能被迁移、回收）。

这个历史命名在 Rust 重写中**原样保留**（外部行为不变）：`handle_get_phys` 返回 `MemType::region_id()`，文档与注释均明确"不是物理地址"。GETREF 同理：`anon_refcount`/`shared_refcount` 返回 `1 + vr->remaps`（mem_anon.c:142-145、mem_shared.c:207-210）——区域的**重映射计数**，不是物理页的引用计数（物理页引用计数是 11 篇的 `PageFrames` 域，查询面不暴露它）。

### 1.4 查询服务的共同纪律：复制前先钉住目标页

C 的 `do_info`/`do_getrusage` 用 `sys_datacopy` 把结果写进调用者地址空间。`sys_datacopy` 是内核复制，如果目标页尚未映射，会触发页错误 → 内核向 VM 发 notify → 而 VM 正阻塞在复制调用上 → **死锁**。C 的解法（utility.c:166-171 注释）：复制前先 `handle_memory_once(vmp, ptr, size, 1)` 把目标页强制解析（必要时分配/CoW 分裂）。

```
C 路径：handle_memory_once 钉住目标页 → sys_datacopy 复制 → 安全
Rust 路径：结果编码进 IPC 消息 → 无跨地址空间复制 → 死锁窗口不存在
```

Rust 的 IPC 消息模型里，结果以 `VmReply` 枚举返回、由传输层编码——不存在"往调用者地址空间写"这一步，因此 `handle_memory_once` 前置检查被**结构性消除**（[ARCH: 26-D1]）。代价是：**region 数组不能随消息内联**（M1 只有 3 指针 + 3 整型槽），`VmReply::InfoRegion` 的数组负载要等 `sys_datacopy` 传输接线后才能送达（见 §3.7 与 §4.8 的 transport 缺口）。

### 1.5 小结

查询服务 = **4 个只读 IPC 面**（统计/使用/区域/资源）+ **3 个隐藏语义**（REGION 的 SELF 别名、USAGE 的负端点=内核、GETPHYS 的 region id 真相）+ **1 条共同纪律**（无副作用的纯函数化查询）。Rust 侧对应：`query.rs`（4 个 handler + 类型化请求/结果）+ `dispatcher.rs`（解码、SELF 替换、errno 映射）+ `vm_server.rs`（boot 用量源 + 编码）+ `memtype.rs`（能力门控）。

---

## 2. C 源码分析

### 2.1 IPC 接口：4 个请求码与消息字段

```c
// com.h:720 / :722 / :729 / :764
#define VM_GETPHYS      (VM_RQ_BASE+35)   /* 消息: m_lc_vm_getphys (endpt, addr, ret_addr) */
#define VM_GETREF       (VM_RQ_BASE+36)   /* 消息: m_lsys_vm_getref (endpt, addr, retc) */
#define VM_INFO         (VM_RQ_BASE+40)   /* 消息: m_lsys_vm_info (what, ep, count, ptr, next) */
#define VM_GETRUSAGE    (VM_RQ_BASE+47)   /* 消息: m_lsys_vm_rusage (endpt, addr, children) */

// com.h:732-734 — VM_INFO 的 what 取值
#define VMIW_STATS  1
#define VMIW_USAGE  2
#define VMIW_REGION 3
```

libc 侧封装（`libsys/vm_info.c`）：`vm_info_stats`（:10-19）、`vm_info_usage(who, vui)`（:24-34，`who` 可为负端点查内核）、`vm_info_region(who, vri, count, &next)`（:39-57，分页游标由调用者持有并回传）。注意 `vm_info_region` 的**循环约定**在消费者侧（procfs pid.c:229、coredump.c:225）：`do { ... } while (r == MAX_VRI_COUNT)`——返回满 64 条就继续翻页。

### 2.2 do_info —— 三模式分派（utility.c:100-184）

```c
int do_info(message *m)
{
	struct vm_stats_info vsi;
	struct vm_usage_info vui;
	static struct vm_region_info vri[MAX_VRI_COUNT];   /* 64 条上限 */

	if (vm_isokendpt(m->m_source, &pr) != OK)          /* 先验证调用者 */
		return EINVAL;

	switch(m->m_lsys_vm_info.what) {
	case VMIW_STATS:
		vsi.vsi_pagesize = VM_PAGE_SIZE;
		vsi.vsi_total = total_pages;
		memstats(&dummy, &free_pages, &largest_contig);
		vsi.vsi_free = free_pages;
		vsi.vsi_largest = largest_contig;
		get_stats_info(&vsi);                          /* 补 vsi_cached */
		break;
	case VMIW_USAGE:
		if(m->m_lsys_vm_info.ep < 0)                   /* 负端点 = 内核 */
			get_usage_info_kernel(&vui);
		else if (vm_isokendpt(m->m_lsys_vm_info.ep, &pr) != OK)
			return EINVAL;
		else get_usage_info(&vmproc[pr], &vui);
		break;
	case VMIW_REGION:
		if(m->m_lsys_vm_info.ep == SELF)               /* SELF 别名 */
			m->m_lsys_vm_info.ep = m->m_source;
		if (vm_isokendpt(m->m_lsys_vm_info.ep, &pr) != OK)
			return EINVAL;
		count = MIN(m->m_lsys_vm_info.count, MAX_VRI_COUNT);
		next = m->m_lsys_vm_info.next;
		count = get_region_info(&vmproc[pr], vri, count, &next);
		m->m_lsys_vm_info.count = count;               /* 回写 count/next */
		m->m_lsys_vm_info.next = next;
		break;
	default:                                            /* 未知 what */
		return EINVAL;
	}
	...
	r = handle_memory_once(vmp, ptr, size, 1);         /* 钉住目标页防死锁 */
	...
	return sys_datacopy(SELF, addr, (vir_bytes) vmp->vm_endpoint, ptr, size);
}
```

关键行为：
1. **先验证调用者**（`m_source`），失败 EINVAL——查询服务的端点验证纪律。
2. STATS：`total_pages` 是全局总页数（alloc.c:319-331 累计）；`memstats`（alloc.c:348-366）扫位图统计空闲页数与最大连续块；`get_stats_info`（cache.c:328-331）只填 `vsi_cached = cached_pages`。
3. USAGE：**负端点 → 内核**（utility.c:131-132）；否则按端点查进程。
4. REGION：`ep == SELF` 替换为调用者（utility.c:141-143）；`count` 截断到 `MAX_VRI_COUNT`（64）；`next` 游标回写。
5. 复制前 `handle_memory_once` 防死锁（utility.c:166-180），随后 `sys_datacopy` 把 `vsi`/`vui`/`vri[]` 复制到调用者缓冲区。

### 2.3 do_get_phys + map_get_phys（mmap.c:438-458 + region.c:1323-1335）

```c
int do_get_phys(message *m)
{
	target = m->m_lc_vm_getphys.endpt;
	addr = (vir_bytes) m->m_lc_vm_getphys.addr;
	if ((r = vm_isokendpt(target, &n)) != OK)
		return EINVAL;                                  /* 端点错 EINVAL */
	vmp = &vmproc[n];
	r = map_get_phys(vmp, addr, &ret);
	m->m_lc_vm_getphys.ret_addr = (void *) ret;         /* 失败时 ret 未初始化也写回（C 瑕疵） */
	return r;
}

int map_get_phys(struct vmproc *vmp, vir_bytes addr, phys_bytes *r)
{
	if (!(vr = map_lookup(vmp, addr, NULL)) ||          /* 区域不存在 */
		(vr->vaddr != addr))                            /* 地址不是区域起点 */
		return EINVAL;
	if (!vr->def_memtype->regionid)                     /* memtype 无 regionid 回调 */
		return EINVAL;
	if(r) *r = vr->def_memtype->regionid(vr);           /* anon: region->id */
	return OK;
}
```

**错误契约**：三个失败路径（区域不存在 / 非起始地址 / 无 regionid 回调）**全部 EINVAL**。返回的是 `regionid()` 回调值——anon 返回 `region->id`，shared 返回源区域 id（§1.3）。

### 2.4 do_get_refcount + map_get_ref（mmap.c:463-483 + region.c:1343-1355）

与 GETPHYS 同构，差别在回调换成 `refcount`：

```c
int map_get_ref(struct vmproc *vmp, vir_bytes addr, u8_t *cnt)
{
	if (!(vr = map_lookup(vmp, addr, NULL)) ||
		(vr->vaddr != addr) || !vr->def_memtype->refcount)
		return EINVAL;
	if (cnt) *cnt = vr->def_memtype->refcount(vr);      /* anon: 1 + remaps */
	return OK;
}
```

返回 `u8_t`——`1 + vr->remaps` 被截断到 8 位（remaps 是 `int`）。**注意与物理页引用计数的区别**：这是区域的 remap 次数，不是 `phys_block.refcount`。

### 2.5 do_getrusage —— PM-only 的资源使用查询（utility.c:426-472）

```c
int do_getrusage(message *m)
{
	if (m->m_source != PM_PROC_NR)                      /* 非 PM 直接 OK */
		return OK;                                      /* 过时构造，向后兼容 */

	if ((res = vm_isokendpt(m->m_lsys_vm_rusage.endpt, &slot)) != OK)
		return ESRCH;                                   /* 唯一用 ESRCH 的查询 */

	/* 从 PM 地址空间复制 rusage 结构（只改 3 个字段） */
	sys_datacopy(m->m_source, m->m_lsys_vm_rusage.addr,
		SELF, &r_usage, sizeof(r_usage));

	if (!m->m_lsys_vm_rusage.children) {
		r_usage.ru_maxrss = vmp->vm_total_max / 1024L;  /* KB */
		r_usage.ru_minflt = vmp->vm_minor_page_fault;
		r_usage.ru_majflt = vmp->vm_major_page_fault;
	} else {
		/* XXX TODO: children 路径未实现（utility.c:458-467）。
		 * 假定 PM 调用前已清零字段，因此不显式清零。 */
	}
	return sys_datacopy(SELF, &r_usage, m->m_source,
		m->m_lsys_vm_rusage.addr, sizeof(r_usage));
}
```

**错误契约特殊**：端点验证失败返回 **ESRCH**（utility.c:441-442），与其余三个查询的 EINVAL 不同；非 PM 调用者直接返回 OK（不修改任何数据）。children 路径在 C 中也是 TODO——Rust 返回全零（在"PM 先清零"约定下语义等价，见 §3.9）。

### 2.6 辅助函数：用量统计与区域列表

**`get_usage_info_kernel`（region.c:1357-1364）**——内核用量：

```c
vui->vui_total = kernel_boot_info.kernel_allocated_bytes +
	kernel_boot_info.kernel_allocated_bytes_dynamic;
vui->vui_virtual = vui->vui_mvirtual = vui->vui_total;   /* 内核页全部已映射 */
```

**`get_usage_info_vm`（region.c:1366-1373）**——VM 自身用量：

```c
vui->vui_total = kernel_boot_info.vm_allocated_bytes +
	get_vm_self_pages() * VM_PAGE_SIZE;
vui->vui_virtual = vui->vui_mvirtual = vui->vui_total;
```

`get_vm_self_pages()`（pagetable.c:1500）统计 VM 自映射页数（页表页 + 自身映射），在 pagetable.c:249/:362/:390 处增删。

**`is_stack_region`（region.c:1384-1390）**——栈区启发式：

```c
return (vr->vaddr == VM_STACKTOP - DEFAULT_STACK_LIMIT &&
    vr->length == DEFAULT_STACK_LIMIT);
```

注释（region.c:1375-1382）自承是"guess work"：VM 并不负责建立栈，只能靠地址猜测；线程栈、VFS 把栈放别处等场景都不准确——但"仅供统计，无所谓"。

**`get_usage_info`（region.c:1395-1447）**——逐进程用量：

```
if (vmp->vm_endpoint == VM_PROC_NR) { get_usage_info_vm(vui); return; }   ← :1405-1408
if (vmp->vm_endpoint < 0)         { get_usage_info_kernel(vui); return; } ← :1410-1413
逐区域（AVL 中序）:
    vui_virtual += vr->length;  vui_mvirtual += vr->length;               ← :1416-1417
    逐页 (voffset 步进 VM_PAGE_SIZE):
        无 phys_block → 若 is_stack_region(vr) 则 mvirtual -= PAGE_SIZE   ← :1419-1423
        有 phys_block → total += PAGE_SIZE                                ← :1426
            refcount > 1 → common += PAGE_SIZE                            ← :1428-1430
                VR_SHARED 区域 → shared += PAGE_SIZE                      ← :1432-1433
尾部（:1444-1446）:
    vui_maxrss = vm_total_max / 1024;  vui_minflt = vm_minor_page_fault;
    vui_majflt = vm_major_page_fault;
```

**尾部三字段**是刻意为之：MIB 每次进程查询只需一次 VM 调用（region.c:1440-1443 注释）——这是 §5 里 `UsageInfo` 必须含 maxrss/minflt/majflt 的 C 侧依据。

**`get_region_info`（region.c:1452-1505）**——区域列表分页：

```
if (!max) return 0;                                    ← :1462
region_start_iter(avl, &v_iter, next, AVL_GREATER_EQUAL); ← :1464 从 next 起扫
for (count = 0; (vr = region_get_iter(&v_iter)) && count < max;
     region_incr_iter(&v_iter)) {
    next = vr->vaddr + vr->length;                     ← :1473 游标先推进（无论是否上报）
    找首/末映射 phys_region（ph1/ph2）;                  ← :1478-1483
    无 ph1/ph2 → printf("skipping empty region...") + continue;  ← :1485-1489
    vri->vri_addr = vr->vaddr + ph1->offset;           ← :1492 用段起点
    vri->vri_prot = PROT_READ;                          ← :1493
    vri->vri_length = ph2->offset + VM_PAGE_SIZE - ph1->offset;  ← :1494 用段长度
    if (vr->flags & VR_WRITABLE) vri->vri_prot |= PROT_WRITE;    ← :1497-1498
    count++;  vri++;
}
*nextp = next;                                          ← :1503
```

三个容易被忽略的语义：
1. **上报的是"用段"**——从首个映射页到末个映射页，不是区域全长（区域可能只 touch 了中间一段）。
2. **空区域（无映射页）跳过**，但**游标仍推进**——跳过不消耗 count 配额。
3. `vri_flags` 字段**从不写入**：`vri` 是 `do_info` 里的 `static` 数组（零初始化），所以线上恒为 0。这是 C 的"死字段"——消费者（procfs/coredump）只读 `vri_addr/vri_length/vri_prot`。

### 2.7 返回结构体

**`struct vm_stats_info`（vm.h:40-46）**：

| 字段 | 类型 | 含义 | 来源 |
|------|------|------|------|
| vsi_pagesize | unsigned int | 页大小 | `VM_PAGE_SIZE` |
| vsi_total | unsigned long | 总页数 | 全局 `total_pages` |
| vsi_free | unsigned long | 空闲页数 | `memstats` |
| vsi_largest | unsigned long | 最大连续空闲块 | `memstats` |
| vsi_cached | unsigned long | 文件系统缓存页数 | `get_stats_info`（cache.c） |

**`struct vm_usage_info`（vm.h:48-57）**：

| 字段 | 类型 | 含义 |
|------|------|------|
| vui_total | vir_bytes | 已映射进程内存总量 |
| vui_common | vir_bytes | 被映射多次的部分（refcount > 1） |
| vui_shared | vir_bytes | common 中非 CoW（VR_SHARED）的部分 |
| vui_virtual | vir_bytes | 虚拟地址空间总量 |
| vui_mvirtual | vir_bytes | virtual 减去未映射栈页 |
| vui_maxrss | uint64_t | 峰值常驻集（KB） |
| vui_minflt / vui_majflt | uint64_t | 次要/主要缺页计数 |

**`struct vm_region_info`（vm.h:59-64）**：

| 字段 | 类型 | 含义 | 取值 |
|------|------|------|------|
| vri_addr | vir_bytes | 区域基址 | **用段**起点（vaddr + 首映射页偏移） |
| vri_length | vir_bytes | 区域长度 | **用段**长度 |
| vri_prot | int | 保护标志 | `PROT_READ`（=1）\| `PROT_WRITE`（=2，若 VR_WRITABLE）；**永不含 PROT_EXEC** |
| vri_flags | int | 内存标志 | **恒 0**（static 零初始化，从不写入） |

`MAX_VRI_COUNT`（vm.h:66）= **64**。`PROT_*` 定义在 `sys/sys/mman.h:63-65`（READ=0x01 / WRITE=0x02 / EXEC=0x04）。

### 2.8 C 源码覆盖完整性

**语义范围**：VM 的 4 个只读查询服务 + 支撑它们的统计/枚举函数。

| 符号 | 类型 | 源码位置 | 在语义范围内? | 文档覆盖? |
|------|------|---------|-------------|-----------|
| do_info | 函数 | utility.c:100 | ✅ | §2.2 |
| do_get_phys | 函数 | mmap.c:438 | ✅ | §2.3 |
| do_get_refcount | 函数 | mmap.c:463 | ✅ | §2.4 |
| do_getrusage | 函数 | utility.c:426 | ✅ | §2.5 |
| map_get_phys | 函数 | region.c:1323 | ✅ | §2.3 |
| map_get_ref | 函数 | region.c:1343 | ✅ | §2.4 |
| get_usage_info_kernel | 函数 | region.c:1357 | ✅ | §2.6 |
| get_usage_info_vm | 函数 | region.c:1366 | ✅ | §2.6 |
| is_stack_region | 函数 | region.c:1384 | ✅ | §2.6 |
| get_usage_info | 函数 | region.c:1395 | ✅ | §2.6 |
| get_region_info | 函数 | region.c:1452 | ✅ | §2.6 |
| get_stats_info | 函数 | cache.c:328 | ✅ | §2.6 |
| anon_regionid / anon_refcount | 函数 | mem_anon.c:132/:142 | ✅ | §1.3/§2.3/§2.4（回调本体归 12 篇） |
| shared_regionid / shared_refcount | 函数 | mem_shared.c:99/:207 | ✅ | §1.3/§2.3/§2.4（回调本体归 12 篇） |
| VM_INFO / VM_GETPHYS / VM_GETREF / VM_GETRUSAGE | 宏 | com.h:729/:720/:722/:764 | ✅ | §2.1 |
| VMIW_STATS / USAGE / REGION | 宏 | com.h:732-734 | ✅ | §2.1 |
| vm_stats_info / vm_usage_info / vm_region_info | 结构 | vm.h:40-46/:48-57/:59-64 | ✅ | §2.7 |
| MAX_VRI_COUNT | 宏 | vm.h:66 | ✅ | §2.1/§2.6/§2.7 |
| memstats | 函数 | alloc.c:348-366 | ✅ | §2.2（实现本体归 05 篇） |
| handle_memory_once | 函数 | utility.c:172-178 调用 | ✅ | §1.4（机制本体归 16 篇） |

**覆盖统计**：查询服务面 100% 进入本文档语义范围；`memstats`/`handle_memory_once` 的实现本体分别归 05/16 篇，此处只引调用面。

---

## 3. Rust 设计决策

### 3.1 模块组织：一服务一文件 + 四处接线

与 18~25 篇相同的组织原则：handler 本体在 `query.rs`，解码/分发在 `dispatcher.rs`，编码在 `vm_server.rs`，能力在 `memtype.rs`。

```
os/servers/vm/src/
├── query.rs        # 4 个 handler + 类型化请求/结果/错误（本轮完整化）
├── ipc/dispatcher.rs  # 解码（M1/M2 overlay）、SELF 替换、errno 映射、dispatch_info 接线
├── vm_server.rs    # usage_sources()（boot 用量源）+ encode_reply_data 编码
├── memtype.rs      # supports_region_id/supports_ref_count 能力门控 + region_id/ref_count 实现
└── boot.rs         # BootParams.vm_allocated_bytes（VM 自身用量输入）
```

### 3.2 错误处理：QueryError 统一 + 上下文相关 errno（D2）

**C 侧事实**：4 个查询的 errno 并不统一——`do_info`/`do_get_phys`/`do_get_refcount` 全部 EINVAL；`do_getrusage` 端点错 ESRCH、非 PM 返回 OK。

**Rust 设计**：`QueryError { ProcessNotFound, NotMapped, NotSupported, InvalidQuery }`（query.rs:48），经 `From<QueryError> for VmError`（dispatcher.rs:1353）映射：

| QueryError | VmError | errno | C 依据 |
|-----------|---------|-------|--------|
| ProcessNotFound | InvalidProcess | EINVAL | utility.c:110、mmap.c:449/:474（端点验证） |
| NotMapped | InvalidParam | EINVAL | region.c:1327-1329/:1346-1348（区域不存在/非起始地址） |
| NotSupported | InvalidParam | EINVAL | region.c:1331/:1349（无 regionid/refcount 回调） |
| InvalidQuery | InvalidParam | EINVAL | utility.c:163（未知 what） |

**本轮 P0 修正**：旧实现把 `NotMapped`/`NotSupported` 映射到 `InvalidAddress`（EFAULT）——与 C 的 EINVAL 不符。getrusage 上下文（ESRCH）仍由 `query_rusage_error_to_vm_error`（dispatcher.rs:1370）与 `to_errno_for_rusage`（query.rs:62-72）在分发边界处理：同一 `QueryError` 在不同调用点映射不同 errno，`From` 无法表达（与 munmap.rs 的 `EndpointError` 模式同族，见 21 篇 §3.2）。

### 3.3 请求类型化：InfoQuery enum（D3）

C 用裸整数 `what` + switch；Rust 用 `InfoQuery { Stats, Usage { target }, Region { target, count, next } }`（query.rs:92-96）。类型化收益：

1. 非法 `what` 在 **decode 层**即拒绝（dispatcher.rs:1164，`InvalidParam`）——handler 永远只见到合法模式；
   - **wire 值对齐 C**：decode 匹配 `VMIW_STATS=1 / VMIW_USAGE=2 / VMIW_REGION=3`（com.h:732-734，libsys vm_info.c 原样发送）——本轮 P0 修正：旧 decode 用 0/1/2 与 C 差一，真实 libsys 调用（what=1 发 STATS）会被错配为 Usage。常量定义在 minix-types `VMIW_*`（ipc/vm.rs），回归测试 `test_dispatch_vm_info_what_matches_c_wire`（vm_server.rs）锁定 1/2/3 映射；
2. `next` 游标从"裸 usize 索引"改为 **`VirBytes` vaddr 游标**（本轮修正，见 D7）——类型即文档；
3. SELF 别名（C: utility.c:141-143）在 decode 层完成：`ep == Endpoint::SELF` → 替换为 `msg.m_source`，handler 无感知。

### 3.4 StatsInfo 五字段完整（D4）

**C 侧事实**：`vm_stats_info` 有 5 个字段，`get_stats_info`（cache.c:328-331）填 `vsi_cached`；MIB 的 hw.c:26/:54 读它（`vm_info_stats` 调用点）。

**Rust 设计**：`StatsInfo` 补上 `cached_pages: u64`（query.rs:99-107），数据来自 `PageCache::total_cached()`（page_cache.rs:421），经 `dispatch_info` 新参数 `cached_pages: u64` 传入（dispatcher.rs:948-955）。**旧实现丢 vsi_cached**——这是本轮 P1 级缺口：C 提供的数据在 Rust 侧被静默丢弃，MIB 读不到缓存页数。

**V10-P2-4 扩展**：`VmReply::InfoStats`（minix-types vm.rs:661-678）在 5 个 C wire 字段之外携带两个 minix-rs 可观测性计数——`dropped_messages: u64`（主循环丢弃消息数，[ARCH: A-14]）与 `pagefault_errors: u64`（页错误处理失败数，[ARCH: A-15]，V9-P1-1）。它们**不在 C 的 `struct vm_stats_info` wire 布局上**（dispatcher.rs:1041-1042 从 `server.dropped_messages()/pagefault_errors()` 取值，encode 时丢弃，见 §4.8）——进程内可观测（测试 + 未来 syslog 槽位），对外 wire 保持 C 兼容。

### 3.5 UsageInfo 八字段完整 + 内核/VM 自身特判（D5）

**C 侧事实**：`vm_usage_info` 有 8 个字段；`get_usage_info` 尾部填 maxrss/minflt/majflt（region.c:1444-1446，MIB 单调用需求）；`ep < 0` → 内核（region.c:1410-1413）；`ep == VM_PROC_NR` → VM 自身（region.c:1405-1408）。

**Rust 设计**：`UsageInfo` 补 `max_rss_kb`/`minor_faults`/`major_faults`（query.rs:122-131），来自 `ActiveProc::total_max/minor_fault/major_fault`（vmproc_handle.rs:216/:226/:231）。`handle_info` 的 Usage 分支按序特判（query.rs:306-397）：

```
target.get() < 0      → 内核用量（UsageSources.kernel_bytes）
target == Endpoint::VM → VM 自身用量（UsageSources.vm_self_bytes）
否则                  → 逐进程统计（region 遍历 + PageFrames 引用计数）
```

**boot 数据源**（`UsageSources`，query.rs:183-186 + vm_server.rs:1032-1044）：`kernel_bytes = kernel_allocated.static_bytes + dynamic_bytes`（对应 region.c:1360-1361）；`vm_self_bytes = vm_allocated_bytes + self_page_count() * PAGE_SIZE`。`vm_allocated_bytes` 是 `BootParams` 新增字段（boot.rs:87，对应 `kernel_boot_info.vm_allocated_bytes`，param.h:44）。

**ARCH 标注**：`get_vm_self_pages()`（pagetable.c:1500）在 Rust 中由 `VmPageAllocator::self_page_count()`（alloc_page.rs:102）承担——Direct Map（A-1，06 篇 §3.3）消除了 VM 自映射页的独立记账通道，VM 自身用量统一走分配器活跃计数。

### 3.6 RegionInfo 对齐"用段"语义 + vri_flags 不建模（D6）

**C 侧事实**：`get_region_info` 上报**用段**（region.c:1492-1494）；`vri_prot` = READ \| (WRITABLE ? WRITE : 0)（:1493/:1497-1498）；`vri_flags` 恒 0（死字段，§2.6）；消费者是 procfs 地图与 VFS coredump（ELF PT_LOAD 段）。

**Rust 设计**：`RegionInfo { addr, length, prot }`（query.rs:141-145）：

```rust
pub(crate) struct RegionInfo {
    pub addr: VirBytes,   // C: vri_addr = vaddr + ph1->offset（用段起点）
    pub length: VirBytes, // C: vri_length = ph2->offset + PAGE - ph1->offset
    pub prot: u16,        // C: vri_prot = PROT_READ | (WRITABLE ? PROT_WRITE : 0)
}
```

- `used_page_range`（query.rs:473-485）找首/末映射 `PageSlot`，等价 C 的 ph1/ph2 扫描（region.c:1478-1483）。
- **`vri_flags` 不建模**：C 恒 0、无消费者，建模即死代码。若未来 wire 对齐需要，编码层补零即可。
- `prot` 用 u16（PROT_READ=0x01/PROT_WRITE=0x02 只需 2 位），类型即文档。
- 空区域跳过（query.rs:433-438）——C 打 `printf`，Rust 查询路径静默（调试输出归 sanity 域）。

### 3.7 分页游标与 MAX_VRI_COUNT=64（D7）

**C 侧事实**：游标 `next` 是 **vaddr**（每次访问区域 end，region.c:1473）；续扫用 `AVL_GREATER_EQUAL(next)`（:1464）；调用者循环 `while (r == MAX_VRI_COUNT)`（procfs pid.c:229）；`count = MIN(请求, 64)`（utility.c:149）。

**Rust 设计**（query.rs:398-465）：

- `next: VirBytes` 游标（本轮从 usize 索引修正为 vaddr，语义对齐 C）；
- `MAX_VRI_COUNT = 64` 常量（query.rs:27，本轮从 8 修正——旧注释声称"Mirrors MAX_VRI_COUNT"但 8 ≠ 64，是**事实错误**）；
- handler 内固定数组 `[RegionInfo; 64]`（栈上 64×16B，无分配）；
- BTreeMap 按 vaddr 升序迭代天然等价 AVL 中序；`vr.vaddr < next` 过滤等价 `AVL_GREATER_EQUAL`。

**transport 缺口（诚实标注）**：`VmReply::InfoRegion.regions` 是**栈上 inline 数组** `[VmRegionInfo; 64]`（minix-types vm.rs:709-718）——`VmReply` 整体因此保持 `Copy`（minix-types vm.rs:634-651 注释解释 inline 比 Box 更优：变体 ~1.5 KiB 小到足以放栈，`VmRegionInfo` 自身 `Copy`，且 boxing 会引入 `extern crate alloc` 而无收益）。但 M1 编码只有 3 指针 + 3 整型槽，**无法内联数组**——`encode_reply_data` 只写 `count`/`next` 到整型槽、源侧长度到 `m1p1`（vm_server.rs:1215-1236，VMI-2 现状延续）。数组负载 DEFERRED 到 `sys_datacopy` 传输接线。**handler 正确性是硬契约**（数据算对了，编码缺一步），文档与代码注释均显式标注。`VmReply::InfoRegion { regions, .. }` 构造路径**无任何堆分配**——`regions` 字段是值类型 64×24B = 1536B，Copy 触发 64 次 24B mem-copy（SIMD 友好），远快于 `Box` 的 alloc+memcpy+refcount 路径。`#[allow(clippy::large_enum_variant)]` 标在 `DispatchAction`（vm_server.rs:719-729）以及未来其他 `Copy`-by-value 使用点，silence ~1.5 KiB "large variant" lint。

### 3.8 GETPHYS/GETREF 走 MemType 能力门控（D8）

**C 侧事实**：`map_get_phys`/`map_get_ref` 检查**回调存在性**（region.c:1331/:1349）；仅 anon/shared 注册（mem_anon.c:42/:44、mem_shared.c:35/:36）。

**Rust 设计**：`MemType` trait 新增两个能力谓词（memtype.rs:133-140）：

```rust
fn supports_region_id(&self) -> bool { false }  // C: regionid != NULL
fn supports_ref_count(&self) -> bool { false }  // C: refcount != NULL
```

`AnonymousMemory`/`SharedMemory` 覆写为 true（memtype.rs:298-303/:472-477）；已有 `region_id()`/`ref_count()` 默认返回 0（= C 的 NULL 语义），anon 返回 `region.id`/`1 + remaps`（memtype.rs:307-312），shared 返回 `param.shared.id`/`1 + remaps`（memtype.rs:485-496）。

**本轮 P0 修正**：旧 `handle_get_phys` 匹配 `VrParam::Direct { phys }` 返回物理地址——但 C 中 **direct 类 memtype 恰恰没有 regionid 回调**（应 EINVAL），而 anon 区域在 Rust 模型里 `param` 默认就是 `Direct { phys: 0 }`（VirRegion::default，vir_region.rs:51-54）——旧实现对 anon 区域返回 0、对 anon/shared 返回 NotSupported，**与 C 完全相反**。新实现对 anon 返回 `region.id`、对 shared 返回源区域 id、对无能力 memtype 返回 EINVAL，与 C 一致。

### 3.9 GETRUSAGE 的 PM-only 语义与 children 零值（D9 局部）

C 的非 PM 返回 OK、端点错 ESRCH、children 为 TODO 且假定 PM 先清零。Rust 对应：

- `!is_pm(caller)` → `GetrusageResult::Ok`（query.rs:533-535）——不产生任何回复数据；
- 端点错 → `QueryError::ProcessNotFound` → dispatcher 映射 `InvalidEndpoint`（ESRCH，dispatcher.rs:1352-1356）；
- children=true → 全零 `ResourceUsage`（query.rs:524-530）——在"PM 清零"约定下与 C 语义等价（C 只改 3 个字段，children 时不改）。

**与 C 的差异**：C 通过 `sys_datacopy` 往返修改 PM 的 rusage 结构（保留 PM 侧其余 12 个字段）；Rust 只回复 3 个字段（消息模型限制），PM 侧其余字段需自行保持——D1 的 ARCH 差异在 getrusage 上的具体表现。

### 3.10 差异清单

| # | 差异 | C 行为 | Rust 行为 | 类型 |
|---|------|--------|-----------|------|
| 1 | sys_datacopy → IPC 消息 | 结果复制进调用者地址空间 | 结果编码进 `VmReply` | ARCH（D1） |
| 2 | handle_memory_once 前置 | 复制前钉住目标页防死锁 | 结构性消除（无跨空间复制） | ARCH（D1） |
| 3 | region 数组编码 | sys_datacopy 送 64 条 | 只编码 count/next，数组 DEFERRED | transport 缺口（D7） |
| 4 | vri_flags | 恒 0（死字段） | 不建模 | 结构简化（D6） |
| 5 | is_stack_region | vaddr/length 精确启发式（region.c:1388-1389） | `end_addr() == region_top()` 近似 | 近似（C 自述 guesswork） |
| 6 | get_vm_self_pages | pagetable.c:1500 独立计数器 | `self_page_count()`（分配器记账） | ARCH（D5，A-1 衍生） |
| 7 | MAX_VRI_COUNT | 64（vm.h:66） | 64（旧 8 已修正） | 事实修正 |
| 8 | GETPHYS/GETREF errno | 全 EINVAL | InvalidParam（EINVAL）；旧 EFAULT 已修正 | 事实修正 |
| 9 | rusage 结构往返 | 15 字段结构复制 | 3 字段消息回复 | ARCH（D9） |

---

## 4. 实现详解

### 4.1 模块结构与数据流

```
调用者消息
  │  dispatch_by_number（dispatcher.rs:1021）
  ├─ VM_GETPHYS  → m1: endpt/addr → dispatch_get_phys(:923)  → handle_get_phys(query.rs:203)
  ├─ VM_GETREF   → m1: endpt/addr → dispatch_get_refcount(:935) → handle_get_refcount(query.rs:241)
  ├─ VM_INFO     → m2: what/ep/count/next → decode(:1142)     → dispatch_info(:948) → handle_info(query.rs:286)
  └─ VM_GETRUSAGE→ m2: target/children → dispatch_getrusage(:997) → handle_getrusage(query.rs:496)
  │
  ▼
VmReply → encode_reply_data（vm_server.rs:1280-1420）→ 传输层
```

`dispatch_by_number` 顶部先以 `&self` 取 `usage_sources = server.usage_sources()`（dispatcher.rs:1029，Copy 值），再 `parts_mut()` 借可变部分——规避双借用。

### 4.2 handle_get_phys（query.rs:203-226）

> 对应 C：`do_get_phys`（mmap.c:438）+ `map_get_phys`（region.c:1323）

```rust
pub(crate) fn handle_get_phys(
    table: &VmProcTable,
    target: Endpoint,
    addr: VirBytes,
) -> Result<PhysBytes, QueryError> {
    let slot = table.vm_isokendpt(target)?;              // C: EINVAL
    let active = table.get_active(slot).ok_or(QueryError::ProcessNotFound)?;
    let vr = active.regions().find(addr).ok_or(QueryError::NotMapped)?;
    if vr.vaddr != addr { return Err(QueryError::NotMapped); }   // C: region.c:1328
    let memtype = vr.def_memtype.ok_or(QueryError::NotSupported)?;
    if !memtype.supports_region_id() { return Err(QueryError::NotSupported); } // C: :1331
    Ok(PhysBytes(memtype.region_id(vr) as u64))          // C: anon→region.id
}
```

关键点：**能力门控先行**（supports_region_id），再取 `region_id()`——对应 C 的"回调存在性 → 调用"两步。`def_memtype` 为 None 的区域（理论上不存在，C 恒有 memtype）归 NotSupported。

### 4.3 handle_get_refcount（query.rs:241-264）

同构于 GETPHYS，门控换成 `supports_ref_count`，返回 `memtype.ref_count(vr) as u8`（C: `u8_t *cnt`，mmap.c:481）。remaps 语义：anon/shared 的 `1 + remaps`（memtype.rs:311-312/:493-494），与 12 篇的 memtype 回调定义一致。

### 4.4 handle_info —— Stats 分支（query.rs:296-305）

```rust
InfoQuery::Stats => {
    let stats = page_alloc.phys_alloc().memstats();      // C: memstats (alloc.c:348)
    Ok(InfoResult::Stats(StatsInfo {
        page_size: PAGE_SIZE as u64,                     // C: vsi_pagesize
        total_pages: page_alloc.total_pages() as u32,    // C: total_pages
        free_pages: stats.free_pages as u32,             // C: vsi_free
        largest_contiguous: stats.largest_free as u32,   // C: vsi_largest
        cached_pages,                                    // C: get_stats_info → vsi_cached
    }))
}
```

`cached_pages` 由 dispatcher 从 `cache.total_cached()` 取得（dispatcher.rs:1170）——数据源与 C 同源（PageCache 就是 cache.c 的 Rust 对应，24 篇）。

### 4.5 handle_info —— Usage 分支（query.rs:306-397）

三路特判 + 逐进程统计：

```rust
InfoQuery::Usage { target } => {
    if target.get() < 0 {                      // C: utility.c:131-132 内核
        return Ok(Usage{ total: kernel_bytes, virtual= mvirtual = total, 其余 0 });
    }
    if target == Endpoint::VM {                // C: region.c:1405-1408 VM 自身
        return Ok(Usage{ total: vm_self_bytes, virtual = mvirtual = total, 其余 0 });
    }
    let slot = table.vm_isokendpt(target)?;    // C: EINVAL
    ...逐区域/逐页累计（与 C region.c:1415-1437 逐行对应）...
    Ok(Usage{
        total, common, shared, virtual_total, mvirtual,
        max_rss_kb: active.total_max().0 / 1024,   // C: region.c:1444
        minor_faults: active.minor_fault(),        // C: :1445
        major_faults: active.major_fault(),        // C: :1446
    })
}
```

内核/VM 自身用量只填 total/virtual/mvirtual（C 的 `memset` 后其余为 0）。逐进程部分与 C 的差异仅在 `is_stack_region` 近似（§3.10 #5）。

### 4.6 handle_info —— Region 分支（query.rs:398-465）

```rust
if count == 0 { return 空结果; }                 // C: region.c:1462 !max → 0
let max = count.min(MAX_VRI_COUNT);              // C: utility.c:149 MIN
let mut cursor = next;
for vr in active.regions().iter() {
    if vr.vaddr < next { continue; }             // C: AVL_GREATER_EQUAL(:1464)
    if idx >= max { break; }                     // C: count < max(:1467)
    cursor = vr.end_addr();                      // C: next 先推进(:1473)
    let (Some(first), Some(last)) = used_page_range(vr) else {
        continue;                                // C: 空区域跳过(:1485-1489)
    };
    result[idx] = RegionInfo {
        addr: VirBytes(vr.vaddr.0 + first.offset().0),   // C: :1492
        length: VirBytes(last.offset().0 + PAGE_SIZE - first.offset().0), // C: :1494
        prot: if vr.flags.contains(VrFlags::WRITABLE) {
            PROT_READ | PROT_WRITE                      // C: :1497-1498
        } else { PROT_READ },
    };
    idx += 1;
}
```

**游标语义验证**（对应 §3.7 的三条）：跳过空区域时 `cursor` 已推进但不占 count；`next` 回传的是**最后访问区域**的 end（含全部跳过的）；调用者从该 vaddr 续扫。与 C 逐行对照：cursor 推进时机（:1473 vs 循环内先置）、count 上限（:1467 vs `idx >= max`）、用段计算（:1492-1494）全部一致。

### 4.7 handle_getrusage（query.rs:496-530）

```rust
pub(crate) fn handle_getrusage(
    table: &VmProcTable, caller: Endpoint, target: Endpoint, children: bool,
) -> Result<GetrusageResult, QueryError> {
    if !is_pm(caller) { return Ok(GetrusageResult::Ok); }   // C: utility.c:437-438
    let slot = table.vm_isokendpt(target)?;                 // C: ESRCH（dispatcher 边界）
    let active = table.get_active(slot).ok_or(QueryError::ProcessNotFound)?;
    if !children {
        Ok(Data(ResourceUsage {
            max_rss_kb: active.total_max().0 / 1024,        // C: :455 KB
            minor_faults: active.minor_fault(),             // C: :456
            major_faults: active.major_fault(),             // C: :457
        }))
    } else {
        Ok(Data(ResourceUsage { 0, 0, 0 }))                 // C: :458-467 TODO，PM 清零约定
    }
}
```

### 4.8 接线与编码

**decode**（dispatcher.rs:1142-1183）：`what=0/1/2` 映射三模式；REGION 模式先做 SELF 替换；`next` 读为 `VirBytes`；非法 what → `InvalidParam`（C: utility.c:163 EINVAL）。

**encode**（vm_server.rs:1280-1420）：

| 回复 | 槽位分配 | 说明 |
|------|---------|------|
| GetPhys | m1p1 = phys | `ret_addr` 语义（C: mmap.c:456） |
| GetRefcount | m1i1 = count | `retc`（C: mmap.c:481） |
| InfoStats | p1=pagesize, i1=total, i2=free, i3=largest, **p2=cached** | 5 字段全编码；`dropped_messages`/`pagefault_errors`（V10-P2-4 扩展）**无 C wire 槽位**，encode 丢弃（vm_server.rs:1299-1314） |
| InfoUsage | p1=total, p2=common, p3=shared, i1=virtual(页数), i2=mvirtual(页数), **i3=maxrss(KB)** | minflt/majflt 无槽位，DEFERRED |
| InfoRegion | p1=源长度, i1=count, i2=next | 数组负载 DEFERRED（VMI-2） |
| Getrusage | p1=maxrss, i1=minflt, i2=majflt | 3 字段 |

编码注释均按 review-patterns-skill §模式19 附 SAFETY（`as i32` 截断饱和）。

### 4.9 与相邻文档的关系

- **13（区域模型）**：`RegionMap::find`/`iter`、`physblocks`、`VrFlags`、`VrParam` 是查询的数据底座；本文档不重复区域生命周期。
- **15（分发）**：CALLMAP 解码与 ACL 检查在 15 篇；本文档只述 4 个查询分支的字段解码。
- **22（进程退出）**：`reset_rusage`/`vm_region_top` 清零是 getrusage 数据正确性的前置（22 篇 §22-P1-4）；本文档消费 `total_max`/fault 计数器。
- **24（页缓存）**：`PageCache::total_cached()` 的口径在 24 篇；本文档只读它喂 `vsi_cached`。
- **12（memtype）**：`region_id`/`ref_count`/能力谓词的**定义本体**在 12 篇 memtype.rs；本文档是它们的查询侧消费方。
- **05（物理内存）**：`memstats` 的实现本体在 05 篇 alloc 模块；本文档只引 `PhysMemStats` 结果。

---

## 5. 测试要点

### 5.1 测试覆盖矩阵（`os/servers/vm/src/query.rs` tests 模块，24 个）

| 测试 | 验证点 | C 依据 |
|------|--------|--------|
| test_query_error_errno | QueryError→VmError→to_errno 全链：4×EINVAL + getrusage ESRCH | utility.c:110/:442/:449/:474、region.c:1327-1334 |
| test_get_phys_invalid_endpoint / test_get_refcount_invalid_endpoint | 无效端点 → ProcessNotFound | mmap.c:449-450/:474-475 |
| test_get_phys_anon_region_returns_region_id | anon 区域返回 region.id（含非起始地址 → NotMapped） | mem_anon.c:132-135、region.c:1328 |
| test_get_refcount_anon_returns_1_plus_remaps | anon remaps=3 → 4 | mem_anon.c:142-145 |
| test_get_phys_unsupported_memtype | Direct 类无能力 → NotSupported（GETPHYS+GETREF） | region.c:1331/:1349 |
| test_getrusage_non_pm / test_getrusage_pm_invalid_endpoint | 非 PM → Ok；PM+无效端点 → ProcessNotFound | utility.c:437-442 |
| test_handle_info_stats_cached_pages | Stats 五字段 + cached=123 | cache.c:328-331 |
| test_dropped_messages_observable_via_info_stats（vm_server.rs:1792） | **V10-P2-4**：3 次 receive 失败 → `dropped_messages==3`，经 VMIW_STATS 的 `VmReply::InfoStats` 扩展字段可观测（`pagefault_errors==0`） | minix-rs 扩展（[ARCH: A-14]；C wire 无槽位） |
| test_handle_info_usage_kernel_target | ep=KERNEL → kernel_bytes，virtual=mvirtual=total | region.c:1357-1364 |
| test_handle_info_usage_vm_self_target | ep=VM → vm_self_bytes | region.c:1366-1373 |
| test_handle_info_usage_invalid_endpoint | 无效端点 → ProcessNotFound | utility.c:133-136 |
| test_handle_info_usage_process_accumulation | 单页 refcount=2 + VR_SHARED → total=common=shared；maxrss=total_max/1024 | region.c:1415-1446 |
| test_handle_info_region_pagination | 两页游标续扫：首页 2 条（含空区域跳过、用段计算、prot 推导）、次页 0 条 | region.c:1452-1505 |
| test_handle_info_region_count_zero | count=0 → 0 条 | region.c:1462 |
| test_memtype_capability_gates | anon/shared true；direct/contig/cache/file false | mem_anon.c:42/:44、mem_shared.c:35/:36 |
| test_shared_region_id_and_refcount | shared.region_id=param.shared.id=77、ref_count=1+remaps | mem_shared.c:99-108/:207-210 |
| test_used_page_range_empty | 无映射页 → (None, None) | region.c:1485-1489 |
| 其余 6 个结构/字段测试 | StatsInfo/UsageInfo/ResourceUsage/InfoResult 字段对齐与构造 | vm.h:40-57 |

### 5.2 测试统计（截至 2026-08-17，V10 收敛后实测）

- `cargo test -p minix-vm --lib`：**441 passed / 0 failed**（`test_map_lazy` 等 P0-1 修复后全绿；V10-P2-4 新增 `test_dropped_messages_observable_via_info_stats`，vm_server.rs:1792）
- query.rs tests 模块：**24 个**；dispatcher 29 个全过；vm_server 34 个全过（含 26-P0 回归 `test_dispatch_vm_info_what_matches_c_wire` 与 V10 主循环端到端测试）；transport 7 个全过
- `cargo clippy -p minix-vm --lib`：**0 warnings**（V10-P2-1 收敛后）

---

## 6. 过渡

### 6.1 位置可回答性

**启动时序**：查询服务**无启动期初始化**——它们只读已由 01（启动参数）、05（物理内存）、13（区域）、24（页缓存）建立的全局状态，CALLMAP 注册在 01 篇 §主循环 完成。

**主循环定位**：运行时 `sef_receive_status(ANY)` → 分发 → `CALLMAP → acl_check → vmc_func` 的**服务分支**（15 篇 §1.3）。4 个查询 handler 是纯函数化的：不挂起、不发回调、不修改状态——是 18~25 篇服务中唯一"零副作用"的一组。

### 6.2 下游移交（对照 plan.md §3.4 第 26 行）

| 移交项 | 内容 | 接收方 |
|--------|------|--------|
| 区域查找/physblocks | `RegionMap::find`/`iter`、`PageSlot` 语义 | 13（区域模型） |
| 分发框架 | CALLMAP 解码、ACL、SUSPEND | 15（IPC 分发） |
| rusage 计数器 | `reset_rusage`/清零时机 | 22（进程退出） |
| cached 口径 | `PageCache::total_cached` 增减 | 24（页缓存） |
| memtype 回调 | `region_id`/`ref_count`/能力谓词定义 | 12（memtype） |

---

## 7. 参见

- `13-region-mapping.md` — 区域查找与 physblocks（查询的数据底座）
- `15-ipc-dispatch.md` — 主循环分发与 CALLMAP（查询的入口）
- `22-vm-exit.md` — rusage/region_top 清零（getrusage 数据正确性前置）
- `24-page-cache.md` — `cached_pages` 口径（vsi_cached 数据源）
- `05-physical-memory.md` — `memstats` 实现（STATS 数据源）
- `12-memtype.md` — `region_id`/`ref_count` 回调本体（GETPHYS/GETREF 能力）
- `minix3/minix/servers/vm/utility.c` / `mmap.c` / `region.c` / `cache.c` — C 源码（ground truth）
- `minix3/minix/include/minix/vm.h` / `com.h` — 结构体与请求码
