# 16-pagefault: 页错误处理——被动缺页与主动内存保障的两条路径

> **分类**: 阶段 6 — 主循环与运行时机制（页错误）
> **源码**: `minix3/minix/servers/vm/pagefaults.c`（418 行：`pf_state` :33-37 / `hm_state` :39-49 / `pf_errstr` :59-70 / `handle_pagefault` :76-158 / `pf_cont` :161-168 / `handle_memory_continue` :170-196 / `handle_memory_final` :198-235 / `do_pagefaults` :240-243 / `handle_memory_once` :245-252 / `handle_memory_start` :254-289 / `do_memory` :294-334 / `handle_memory_step` :336-417）+ `minix3/minix/servers/vm/region.c`（`map_pf` :664-754 / `map_handle_memory` :756-774 / `map_lookup` :616-641）+ `minix3/minix/servers/vm/main.c`（主循环 P3 :153-164 / `SIGKMEM` 信号 :731-750）
> **Rust 模块**: `os/servers/vm/src/cow_exec_pf.rs`（366 行：`handle_pagefault` :19 / `alloc_and_map` :54 / `cow_resolve` :72 / `cow_resolve_core` :85 / `copy_page_content` :190 / `cow_resolve_region` :216 / `PagefaultAction` :236 / `CowError` :245）+ `os/servers/vm/src/vm_server.rs`（`dispatch_pagefault` :742-771 / 主循环 P3 :577-585）+ `os/servers/vm/src/fork.rs`（`handle_memory_once` :33-85）+ `os/servers/vm/src/memtype.rs`（`PagefaultResult` :152-158 / `AnonymousMemory::ev_pagefault` :220-256 / `MappedFile::ev_pagefault` :904-935）+ `os/libs/minix-types/src/ipc/vm.rs`（`VM_PAGEFAULT` :149 / `VmPagefaultIn` :373-377 / `decode_message` :379-398）
> **前置**: `notes/rewrite/fork-syscall-rewrite/02-stage-vm/13-region-mapping.md`（`map_pf`/`map_handle_memory` 的区域-物理侧桥接）+ `notes/rewrite/fork-syscall-rewrite/02-stage-vm/15-ipc-dispatch.md`（主循环 P3 分发与 SUSPEND 协议）+ `12-memtype.md`（`ev_pagefault` 回调语义）
> **说明**: 页错误处理是 VM 的**中断入口**——CPU 缺页异常被内核转发为 `VM_PAGEFAULT` 消息（被动路径），内核自身的内存访问保障请求经 `SIGKMEM` 信号触发 `do_memory`（主动路径），两条路径最终汇合在 `map_pf`。本文档管**状态机与协议**（验证 → 区域查找 → 权限检查 → memtype 分发 → 异步恢复）；**不覆盖**：CoW 分裂机制本身（17）、VFS 请求队列与 fdref（23）、页缓存（24）、mmap 建立（20）。

---

## 1. 概念：两条入口、一个汇合点

### 1.0 章节引言

01~15 建立了 VM 的地址空间数据结构（区域/物理页/页表）与运行时心脏（主循环/分发）。本文档回答三个问题：

1. **CPU 缺页怎么变成 VM 的一条消息**——被动路径：内核捕获异常 → `RTS_PAGEFAULT` → `VM_PAGEFAULT` → `do_pagefaults`（§1.2）。
2. **内核自己"借"用户内存时怎么保证不缺页**——主动路径：`SIGKMEM` 信号 → `do_memory` → `handle_memory_start` 状态机（§1.3）。
3. **VFS 异步 I/O 怎么和单线程事件循环共存**——`SUSPEND` 协议：缺页处理到一半挂起，VFS 完成回调继续（§1.4）。

它在整个 02-stage-vm 中的位置：

```
01（启动链）→ 02/03（进程表）→ 04（ACL）→ 05~14（内存/页表/区域）
→ 15（主循环与分发）→ ★16（页错误：P3 分支 + SIGKMEM 信号）
→ 17（CoW 分裂）→ 18~26（服务与协作）
```

### 1.1 谁不能缺页

Minix3 微内核中，页错误只有一条处理通道——VM 服务器。因此**内核与 VM 自身都不能缺页**：

| 进程类型 | 是否缺页 | 内核处理方式 | 依据 |
|---------|---------|------------|------|
| 内核 | N | 嵌套异常 `inkernel_disaster()` → panic | `kernel/arch/i386/exception.c:93-97` |
| VM 服务器 | N | `panic("pagefault in VM")` | `exception.c:99-110` |
| 用户进程 | Y | 挂起（`RTS_PAGEFAULT`）→ 转发 VM | `exception.c:115-125` |

> **VM 不能缺页的根因**：VM 是缺页的唯一处理者——若 VM 自身按需分页，处理他人缺页时可能触发自身缺页，形成死锁。因此 VM 的页分配采用 eager mapping（见 06-page-allocator）：分配即映射，不存在"先占虚拟地址、访问时再映射"的延迟分配。

### 1.2 被动路径：CPU 异常 → VM_PAGEFAULT

用户进程访问无效/受保护地址时，CPU 触发 #PF 异常：

```
CPU #PF → 内核 exception.c:115-125
  ├─ RTS_SET(pr, RTS_PAGEFAULT)          /* 挂起该进程，不再调度 */
  └─ mini_send(VM_PROC_NR, &m_pagefault, FROM_KERNEL)
       m_source  = pr->p_endpoint        /* 出错进程 */
       m_type    = VM_PAGEFAULT
       VPF_ADDR  = pagefaultcr2          /* 出错地址 */
       VPF_FLAGS = frame->errcode        /* CPU 错误码 */
```

VM 主循环 P3 分支（main.c:153-164）校验消息来自内核后调用 `do_pagefaults(&msg)`（pagefaults.c:240-243），并 `continue` **不回复**——出错进程由 VM 在成功时用 `sys_vmctl(ep, VMCTL_CLEAR_PAGEFAULT, 0)` 解除阻塞（§2.2）。

### 1.3 主动路径：SIGKMEM → do_memory

页错误处理不只有 CPU 异常这一条路。内核在执行跨地址空间复制（`sys_vircopy` 等）前，需要 VM 先把目标页**主动**映射好，否则内核态会缺页（而内核不能缺页）。这是 `handle_memory_start` 的职责：

```
内核信号 SIGKMEM（main.c:731-737）
  └─ do_memory()（pagefaults.c:294-334）
       └─ sys_vmctl_get_memreq() 循环取请求（VMPTYPE_CHECK）
            └─ handle_memory_start(vmp, mem, len, wrflag, KERNEL, requestor, ...)
                 └─ handle_memory_step() → map_handle_memory() → map_pf()
```

同步变体 `handle_memory_once`（pagefaults.c:245-252）供 VM 自身使用（如 fork 后预填充消息缓冲页、`map_pin_memory`）——它断言 `r != SUSPEND`，因为调用者（VM）不能阻塞等 VFS。

### 1.4 SUSPEND 协议：单线程事件循环中的异步 I/O

页错误处理可能遇到文件映射页尚未从磁盘读入（mappedfile memtype）。此时 VM 不能阻塞等待 VFS——它是单线程服务端，阻塞就死锁整个系统。C 的方案：

1. `map_pf` 调用 `ph->memtype->ev_pagefault(..., pf_callback, state, ...)`，mappedfile 实现发起 VFS 异步读请求并返回 `SUSPEND`（-998，com.h:1151）。
2. `handle_pagefault` 检测到 `SUSPEND` 直接 `return`（pagefaults.c:140-142）——进程保持 `RTS_PAGEFAULT` 挂起，VM 继续主循环。
3. VFS 完成 I/O 后回调 `pf_cont`（pagefaults.c:161-168），还原 `pf_state`（ep/vaddr/err）并以 `retry=1` 重进 `handle_pagefault`——这次 `map_pf` 不再传回调（pagefaults.c:124-126），`assert(result != SUSPEND)`。

这与 15 的 SUSPEND 伪返回码是同一个协议：**"不回复 ≠ 失败，是稍后回复"**。区别是 15 管服务调用的延迟回复，这里管页错误进程的延迟恢复。

### 1.5 major/minor 缺页统计

`handle_pagefault` 用 `map_pf` 的 `io` 出参区分缺页性质（pagefaults.c:135-138）：

| 计数器 | 触发条件 | 含义 |
|--------|---------|------|
| `vm_major_page_fault` | `io == 1` | 页不在内存，需磁盘 I/O（VFS 读文件） |
| `vm_minor_page_fault` | `io == 0` | 页已在内存，仅需页表/CoW 处理 |

计数器的消费面：`do_getrusage`（见 26-vm-queries）向内核/PM 汇报进程内存行为。

### 1.6 对照：Redox 与 Linux

- **Linux**：内核 `do_page_fault()` 直接处理（`mm/memory.c`），按 PTE 状态分发 `handle_mm_fault` → `do_anonymous_page`（匿名首次）/ `do_fault`（文件）/ `do_wp_page`（写时复制）。`handle_mm_fault` 返回 `VM_FAULT_MAJOR`/`VM_FAULT_MINOR` 位标志，由调用者累加 `maj_flt`/`min_flt`（`task_struct`）——与 Minix3 的 `io` 出参 + 两个计数器同构。Linux 的 `do_wp_page` 在 `page_mapcount(page) == 1` 时走 `wp_page_reuse`（只改 PTE 权限、不拷贝）——对应 minix-rs 的 `refcount <= 1` 快速路径（§3.3）。差异：Linux 的页错误处理在进程上下文（可睡眠、可等 I/O），Minix3 在单线程用户态服务器（必须 SUSPEND 协议）。
- **Redox**：内核态直接处理页错误（redox-kernel 的 `page_fault_handler`），无用户态 VM 服务器；地址空间对象 `AddressSpace` 由内核持有。Minix3 把"内存管理策略"（区域/CoW/按需加载）整体上移用户态，内核只做"转发异常 + 解除阻塞"的薄层。
- **对照要点**：三家都保留"错误码区分读写/存在/保护 + 主次缺页计数"的 x86 遗产；区别在**处理者位置**（内核进程上下文 vs 用户态事件循环）——这正是 SUSPEND 协议存在的原因。

### 1.7 小结

页错误处理 = 被动（CPU）与主动（内核请求）两条入口 + 一个汇合点（`map_pf`）+ 一个异步协议（SUSPEND 回调）。理解这一章后，C 源码的三块（`handle_pagefault` / `handle_memory_*` 状态机 / `map_pf` 桥接）可以对照阅读。

---

## 2. C 源码分析

### 2.1 消息格式与内核接口

**VM_PAGEFAULT 消息**（com.h:772-775）：

```c
/* not handled as a normal VM call, thus at the end of the reserved rage */
#define VM_PAGEFAULT		(VM_RQ_BASE+0xff)
#	define VPF_ADDR		m1_i1
#	define VPF_FLAGS	m1_i2
```

- `VM_PAGEFAULT = VM_RQ_BASE + 0xff = 0xCFF`（com.h:773）——刻意放在保留区末尾，`CALLNUMBER` 对 0xCFF 越界返回 -1，**进不了普通分发表**，只能在主循环 P3 分支被截获（main.c:153-164）。
- `VPF_ADDR` 用 `m1_i1`（32 位）、`VPF_FLAGS` 用 `m1_i2`；出错进程端点在 `m_source`（kernel/arch/i386/exception.c:119-122 填充）。

**错误码宏**（arch/i386/pagetable.h:36-39）：

```c
#define PFERR_NOPAGE(e)	(!((e) & I386_VM_PFE_P))
#define PFERR_PROT(e)	(((e) & I386_VM_PFE_P))
#define PFERR_WRITE(e)	((e) & I386_VM_PFE_W)
#define PFERR_READ(e)	(!((e) & I386_VM_PFE_W))
```

x86 错误码 bit0 = Present（0=不存在页、1=保护违规），bit1 = Write。earm 版（arch/earm/pagetable.h:39-43）用 DFSR 的 L1PERM/W 位，宏语义相同。

**内核控制接口**（VM → 内核，syslib.h:61-64）：

```c
int sys_vmctl(endpoint_t who, int param, u32_t value);
int sys_vmctl_get_memreq(endpoint_t *who, vir_bytes *mem, vir_bytes
	*len, int *wrflag, endpoint_t *who_s, vir_bytes *mem_s, endpoint_t *);
```

相关 `VMCTL_*` 参数（com.h:395/:397/:398）：`VMCTL_CLEAR_PAGEFAULT`(12) 解除缺页挂起、`VMCTL_MEMREQ_GET`(14) 取内核内存请求、`VMCTL_MEMREQ_REPLY`(15) 回复请求结果；`VMPTYPE_CHECK`(1)（vm.h:38）是取到的请求类型。

### 2.2 handle_pagefault：被动路径主处理函数（pagefaults.c:76-158）

```c
static void handle_pagefault(endpoint_t ep, vir_bytes addr, u32_t err, int retry)
```

**逐步流程**：

| 步骤 | 行号 | 逻辑 |
|------|------|------|
| 端点验证 | :85-89 | `vm_isokendpt(ep, &p)` 失败 → panic；`vmp = &vmproc[p]`；assert `VMF_INUSE` |
| 区域查找 | :92-107 | `map_lookup(vmp, addr, NULL)` 无区域 → 打印（PROT vs NOPAGE + 出错时 `sys_diagctl_stacktrace`）→ `sys_kill(vmp->vm_endpoint, SIGSEGV)` + `sys_vmctl(ep, VMCTL_CLEAR_PAGEFAULT, 0)` → return |
| 写权限检查 | :110-118 | `!(region->flags & VR_WRITABLE) && wr` → 打印 "ro map" → SIGSEGV + 清缺页 → return |
| 偏移计算 | :120-121 | `assert(addr >= region->vaddr)`；`offset = addr - region->vaddr` |
| 分支：retry vs 首次 | :124-133 | retry：`map_pf(..., NULL, NULL, 0, &io)` + `assert(result != SUSPEND)`；首次：构造 `pf_state { ep, vaddr, err }`，`map_pf(..., pf_cont, &state, sizeof(state), &io)` |
| 缺页统计 | :135-138 | `io` → `vm_major_page_fault++`，否则 `vm_minor_page_fault++` |
| SUSPEND | :140-142 | `result == SUSPEND` → return（等 VFS 回调） |
| 失败 | :144-151 | 打印 "pagefault not handled" → SIGSEGV + 清缺页 |
| 成功 | :153-157 | `pt_clearmapcache()` + `sys_vmctl(ep, VMCTL_CLEAR_PAGEFAULT, 0)` 恢复进程 |

**要点**：SIGSEGV 与恢复进程都是**通过内核接口**完成的——VM 自身不直接改进程状态，进程由 `RTS_PAGEFAULT` 挂起、由 `VMCTL_CLEAR_PAGEFAULT` 解除。这就是"VM 是策略、内核是机制"的切分。

**retry 语义**：首次处理携带 `pf_cont` 回调 + `pf_state` 快照；VFS 完成后 `pf_cont`（:161-168）校验 `state->ep` 仍有效，然后以 `retry=1` 重进 `handle_pagefault`。重试时**不再传回调**——因为 VFS 这次不会再异步，`map_pf` 必须同步完成（断言 `!= SUSPEND` 防死锁）。

### 2.3 map_pf / map_handle_memory：两条路径的汇合点（region.c）

**map_pf**（region.c:664-754）是 `handle_pagefault`（被动）与 `handle_memory_step`（主动）共同调用的逐页处理函数：

```c
int map_pf(struct vmproc *vmp, struct vir_region *region, vir_bytes offset,
	int write, vfs_callback_t pf_callback, void *state, int len, int *io)
```

| 步骤 | 行号 | 逻辑 |
|------|------|------|
| 对齐与断言 | :676-682 | `offset -= offset % VM_PAGE_SIZE`；`assert(offset < region->length)`；写时必须 `VR_WRITABLE` |
| physblock 存在性 | :686-701 | `physblock_get(region, offset)` 无 → `pb_new(MAP_NONE)` + `pb_reference(...)`（失败 `pb_free` + ENOMEM） |
| 可写短路 | :711-713 | 写操作且 `memtype->writable(ph)` 为真 → 跳过 ev_pagefault（页已可写） |
| memtype 分发 | :717-719 | `ev_pagefault(vmp, region, ph, write, cb, state, len, io)`；返回 `SUSPEND` → 透传 |
| 失败回滚 | :722-728 | `r != OK` → `pb_unreferenced(region, ph, 1)` 释放刚引用的物理块 |
| 页表写入 | :739-741 | `map_ph_writept(vmp, region, ph)` 把 phys_region 的物理页写进页表 |
| sanity 检查 | :746-750 | `pt_checkrange(&vmp->vm_pt, region->vaddr+offset, VM_PAGE_SIZE, write)` |

**map_handle_memory**（region.c:756-774）是批量版本：`for(offset = start; offset < lim; offset += PAGE_SIZE) map_pf(...)`，任一页失败即返回——主动路径逐区域调它。

**回调类型** `vfs_callback_t`（memtype.h:9）：`void (*)(struct vmproc *vmp, message *m, void *arg, void *statearg)`——`statearg` 携带恢复状态（`pf_state` 或 `hm_state`），`m` 是 VFS 回复消息。

### 2.4 handle_memory_* 状态机：主动路径的异步保障

**hm_state**（pagefaults.c:39-49）承载一次主动内存保障的完整上下文：

```c
struct hm_state {
	endpoint_t caller;	/* KERNEL or process? if NONE, no callback */
	endpoint_t requestor;	/* on behalf of whom? */
	int transid;		/* VFS transaction id if valid */
	struct vmproc *vmp;	/* target address space */
	vir_bytes mem, len;	/* memory range */
	int wrflag;		/* must it be writable or not */
	int valid;		/* sanity check */
	int vfs_avail;		/* may vfs be called to satisfy this range? */
#define VALID	0xc0ff1
};
```

**handle_memory_start**（:254-289）是入口：

1. 页对齐：`mem -= mem % PAGE_SIZE; len += o; len = roundup(len, PAGE_SIZE)`（:262-267）。
2. 初始化 `hm_state`（:269-277），`valid = VALID` 哨兵。
3. `handle_memory_step(&state, FALSE)`（:279）。
4. 若 `SUSPEND` → `assert(caller != NONE); assert(vfs_avail)`（:281-283）——只有"有回复对象 + VFS 可用"才能异步；否则同步走 `handle_memory_final`（:285）。

**handle_memory_step**（:336-417）逐区域逐页（详见 §2.6）。

**handle_memory_continue**（:170-196）是 VFS 回复回调：

```c
if(m->VMV_RESULT != OK) { handle_memory_final(state, m->VMV_RESULT); return; }
r = handle_memory_step(state, TRUE /*retry*/);
if(r == SUSPEND) return;          /* 还有页要 VFS，继续等 */
handle_memory_final(state, r);
```

**handle_memory_final**（:198-235）按 `caller` 三类回复：

| caller | 回复方式 | 行号 |
|--------|---------|------|
| `KERNEL` | `sys_vmctl(state->requestor, VMCTL_MEMREQ_REPLY, result)` | :205-207 |
| VFS（`IS_VFS_FS_TRANSID(transid)`） | `msg.m_type = TRNS_ADD_ID(result, transid)` + `asynsend3(..., AMF_NOREPLY)` | :214-226 |
| 普通进程 | `asynsend3(caller, &msg, 0)` | :208-220 |
| `NONE` | 不回复（`caller == NONE` 时不进任何分支） | — |

收尾 `memset(state, 0, sizeof(*state))`（:233）——**fail fast**：任何人再碰这个栈上状态都会在 `assert(valid == VALID)` 处炸掉，防止悬垂回调。

### 2.5 do_memory：SIGKMEM 信号驱动（main.c:731-750 + pagefaults.c:294-334）

`do_memory()` 由 SEF 信号处理器在收到 `SIGKMEM` 时调用（main.c:735-738）——"内核有未决的内存请求"：

```c
while(1) {
	r = sys_vmctl_get_memreq(&who, &mem, &len, &wrflag, &who_s, &mem_s, &requestor);
	switch(r) {
	case VMPTYPE_CHECK: {
		int transid = 0; int vfs_avail;
		if(vm_isokendpt(who, &p) != OK) panic(...);
		vmp = &vmproc[p];
		assert(!IS_VFS_FS_TRANSID(transid));
		/* is VFS blocked? */
		if(requestor == VFS_PROC_NR) vfs_avail = 0;
		else vfs_avail = 1;
		handle_memory_start(vmp, mem, len, wrflag, KERNEL, requestor, transid, vfs_avail);
		break;
	}
	default: return;
	}
}
```

要点：

- **`vfs_avail` 的语义**（:321-322）：请求者是 VFS 时，VFS 正阻塞等 VM 回复，**不能再向它发异步请求**（会死锁），故 `vfs_avail = 0`——mappedfile 区域只能同步处理（§2.6 的同步分支条件）。
- **transid 恒 0**（:311、:318）：内核内存请求不携带 VFS 事务。
- 信号处理器尾部还有 `alloc_cycle()`（保留页池补充，main.c:745-747）+ `pt_clearmapcache()`（:749）——SIGKMEM 的完整语义是"内存压力信号"：既处理内核请求，也趁机补充 VM 自身保留页。

### 2.6 handle_memory_step：逐区域逐页（pagefaults.c:336-417）

外层循环按区域推进（:348-368）：`map_lookup(hmstate->vmp, hmstate->mem, NULL)` 失败或 `!(region->flags & VR_WRITABLE) && wrflag` → `EFAULT`；否则把本次范围裁剪到当前区域（`length = min(len, region->length - offset)`）。

内层循环一次一页（:379-413），注释（:369-378）说明理由：

> 一次处理一页优于批量：`map_handle_memory` 内部本来就逐页，批量传入会导致已处理页被重复检查；且 one-shot 页（重试时必须同步映射）要求精确知道当前重试的是哪一页。

```c
if((region->def_memtype == &mem_type_mappedfile &&
    (!hmstate->vfs_avail || retry)) ||
    hmstate->caller == NONE) {
	r = map_handle_memory(..., NULL, NULL, 0);      /* 同步 */
	assert(r != SUSPEND);
} else {
	r = map_handle_memory(..., handle_memory_continue, hmstate, sizeof(*hmstate));
}
if(r != OK) return r;
hmstate->len -= sublen; hmstate->mem += sublen;
offset += sublen; length -= sublen; retry = FALSE;   /* 一页成功后本次重试标记复位 */
```

**同步分支条件**（:391-393）三选一：
1. mappedfile 且 VFS 不可用（`!vfs_avail`，如请求者即 VFS）；
2. mappedfile 且重试中（`retry`）——第二次进入不允许再调 VFS，防止 FS 出错时无限循环，同时允许 one-shot 页在此映射；
3. `caller == NONE`（`handle_memory_once` 路径，VM 自身调用不能阻塞）。

### 2.7 pf_errstr（pagefaults.c:59-70）

```c
char *pf_errstr(u32_t err)
{
	static char buf[100];
	snprintf(buf, sizeof(buf), "err 0x%lx ", (long)err);
	if(PFERR_NOPAGE(err)) strcat(buf, "nopage ");
	if(PFERR_PROT(err)) strcat(buf, "protection ");
	if(PFERR_WRITE(err)) strcat(buf, "write");
	if(PFERR_READ(err)) strcat(buf, "read");
	return buf;
}
```

仅用于诊断打印（§2.2 的三处 SIGSEGV 日志），不参与控制流——注意 NOPAGE 与 PROT 互斥、WRITE 与 READ 互斥，因此同一错误码最多拼出两项。

### 2.8 C 小结：符号全景

| 符号 | 行号 | 角色 |
|------|------|------|
| `do_pagefaults` | :240-243 | 被动入口：拆消息调 `handle_pagefault(..., retry=0)` |
| `handle_pagefault` | :76-158 | 被动主函数：验证/查找/权限/统计/恢复 |
| `pf_cont` | :161-168 | VFS 完成后的被动重试回调 |
| `handle_memory_once` | :245-252 | 同步变体（VM 自身用，assert 不 SUSPEND） |
| `handle_memory_start` | :254-289 | 主动入口：对齐 + 状态初始化 |
| `do_memory` | :294-334 | SIGKMEM 循环取内核请求 |
| `handle_memory_step` | :336-417 | 逐区域逐页处理核心 |
| `handle_memory_continue` | :170-196 | VFS 完成后的主动重试回调 |
| `handle_memory_final` | :198-235 | 三类回复 + 状态销毁 |
| `pf_errstr` | :59-70 | 诊断字符串 |
| `map_pf` / `map_handle_memory` | region.c:664-754 / :756-774 | 两条路径的汇合点 |

---

## 3. Rust 设计决策

> 行号以 2026-08-16 实证为准。Rust 侧现状：**被动路径的"判定+动作"已实现**（`dispatch_pagefault` → `handle_pagefault`）；**主动路径的异步状态机（`do_memory`/`handle_memory_start/step/final/continue`）未实现**，仅 `fork.rs::handle_memory_once` 同步子集——差异清单见 §3.6。

### 3.1 D1：PagefaultResult / PagefaultAction 双层结果

C 的 `ev_pagefault` 直接做动作（分配页、发起 I/O）并返回 int；Rust 拆成两层：

- **memtype 层只判定**：`MemType::ev_pagefault` 返回 `PagefaultResult`（memtype.rs:152-158）——`Handled` / `NeedNewPage` / `NeedCow` / `NeedVfsIo` / `AccessViolation`，不碰分配器、不碰页表。
- **调用者层执行**：`cow_exec_pf::handle_pagefault`（cow_exec_pf.rs:19-52）match 结果并执行对应动作：

```rust
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
    PagefaultResult::NeedVfsIo => Ok(PagefaultAction::Suspended),
    PagefaultResult::AccessViolation => Ok(PagefaultAction::AccessViolation),
}
```

**动机**：每个 memtype 不再重复"分配 + 映射"样板；`NeedVfsIo → Suspended` 显式表示"事件循环不阻塞"的语义（C 的 `SUSPEND` 伪返回码在类型层可见）。这消除了 C 中 `ev_pagefault` 返回值与副作用的隐式耦合（C 的实现必须自己 `pb_link` + 更新 PTE，出错时还要 `pb_unreferenced` 回滚——region.c:717-728）。

### 3.2 D2：CowError / CowCoreError 错误层级

| 类型 | 定义 | 用途 |
|------|------|------|
| `CowCoreError { NoMemory, PageNotMapped }` | cow_exec_pf.rs:175-178 | `cow_resolve_core` 内部 |
| `CowError { NoMemory, NoMemType, PageNotMapped, MemType(MemTypeError) }` | cow_exec_pf.rs:245-250 | `handle_pagefault` 对外 |
| `From<CowCoreError> for CowError` | :180-187 | 内部错误提升 |
| `From<MemTypeError> for CowError` | :252-256 | memtype 错误并入 |

对应关系：`NoMemory` ← C `ENOMEM`（`pb_new`/`pb_reference` 失败，region.c:691-701）；`PageNotMapped` ← C `map_lookup` 失败/`physblock_get` 空（C 里分别走 SIGSEGV 与 `pb_new` 路径）；`MemType` ← C `ev_pagefault` 的 errno。**关键简化**：PFN 模型下 `PhysBlock` 对象不存在，`pb_new`/`pb_reference` 两条失败路径收敛为 `alloc_pfn()` 一条 `NoMemory`（doc 11/13 详述）。

### 3.3 D3：refcount ≤ 1 快速路径

`cow_resolve_core`（cow_exec_pf.rs:85-128）在 `frames.get(old_pfn).refcount <= 1` 时直接返回 `old_pfn`（:103-105）——页面已是私有，无需分配/拷贝/换映射。对应 Linux `do_wp_page` 的 `wp_page_reuse`；C 的 `mem_cow`（mem_anon.c）无条件复制。快速路径由 `test_cow_resolve_no_sharing` 与 `test_cow_resolve_core_refcount_one_fast_path` 覆盖（§5.1）。

### 3.4 D4：decode_message——64 位 wire format（ARCH + 本轮 P0 修复）

**ARCH 演进**：C 的 `VPF_ADDR` 是 `m1_i1`（32 位，com.h:774），x86-32 地址恰好放得下；minix-rs 是 64 位，kernel 侧用专用 union 成员 `m_vm_pagefault`（`vpf_addr: u64` + `vpf_flags: u32`，minix-types message.rs:1486-1495）打包，出错进程端点在 `m_source`（os/kernel/src/page_fault.rs:142-166，C 对照 kernel/arch/i386/exception.c:119-122）。

```rust
pub fn decode_message(msg: &Message) -> Self {
    let pf = unsafe { msg.m_u.m_vm_pagefault };
    Self {
        endpoint: msg.m_source,                 // C: m->m_source（pagefaults.c:242）
        vaddr: VirBytes(pf.vpf_addr),           // C: VPF_ADDR
        write: (pf.vpf_flags & 2) != 0,         // C: PFERR_WRITE(err)（pagetable.h:38）
    }
}
```

**本轮 P0 修复记录**：旧实现 `impl DecodeFromM1 for VmPagefaultIn`（原 vm.rs:713-722）从 `m_m1` 解码（`endpoint: Endpoint(m1.m1i1)`、`vaddr: VirBytes(m1.m1p1)`、`write: m1.m1i2 != 0`），与 kernel 写入的 `m_vm_pagefault` union 成员**错位**——按 union 重叠布局实际会解出 `endpoint = 低 32 位出错地址`、`vaddr = 0`、`write = 高 32 位地址非零`，一旦端到端接线必然 `vm_isokendpt` 失败。修复：移除 M1 解码，新增 `decode_message`（minix-types vm.rs:379-398），`dispatch_pagefault` 改用（vm_server.rs:746），并补 2 个解码测试（vm.rs:1034/:1052）。`write` 语义同步修正为 `flags & 2`（C `PFERR_WRITE`），旧 `!= 0` 会把只读保护错误误判为写。

### 3.5 D5：handle_memory_once 同步子集（fork.rs:33-85）

C 的 `handle_memory_once`（pagefaults.c:245-252）→ `handle_memory_start(NONE, NONE, 0, 0)`。Rust 在 `fork.rs` 实现同名的同步版本，专供 fork 后预填充消息缓冲页：

- 页对齐（fork.rs:41-44，对照 pagefaults.c:262-267）；
- 逐区域逐页（:49-82）：`regions.find(addr)` 失败 → `PageNotMapped`（C `EFAULT`）；`wrflag && !region.is_writable()` → `PageNotMapped`（C `EFAULT`）；页内 `needs_cow(frames, offset)` → `cow_resolve_core`；
- 无 SUSPEND 分支——与 C 同步变体一致（`assert(r != SUSPEND)`）。

`needs_cow`（region/vir_region.rs:230-239）：slot 已映射且 `refcount > 1` 才需要分裂。

### 3.6 差异清单（C ↔ Rust，诚实标注）

| # | C 语义 | Rust 现状 | 状态 |
|---|--------|----------|------|
| 1 | `handle_pagefault` 验证链 + memtype 分发 | `dispatch_pagefault` + `cow_exec_pf::handle_pagefault` 等价实现（vm_server.rs:742-771） | ✅ 已实现 |
| 2 | wire format：`m_source` + `m1_i1`/`m1_i2` | `decode_message`：`m_source` + `m_vm_pagefault`（64 位 ARCH） | ✅ 已实现（本轮修复） |
| 3 | SIGSEGV + `VMCTL_CLEAR_PAGEFAULT` 恢复进程 | VM 侧无 `sys_vmctl`；`dispatch_pagefault` 只返回 `VmReply`，主循环丢弃（vm_server.rs:583 `let _`） | ⚠️ DEFERRED（进程保持挂起，恢复契约未接线） |
| 4 | `pf_errstr` 诊断日志 | no_std 无 printf；错误以 `CowError`/`VmReply::Error(AccessViolation)` 传递 | ⚠️ 简化 |
| 5 | major/minor 缺页计数（:135-138） | 字段存在（vmproc.rs:55-56）+ `inc_minor_fault`/`inc_major_fault` 方法（vmproc_handle.rs:300-307），生产路径未调用（仅测试） | ⚠️ 缺口 |
| 6 | `do_memory`/`handle_memory_start/step/final/continue` 异步状态机 | 未实现；`fork.rs::handle_memory_once` 仅同步子集 | ⚠️ DEFERRED |
| 7 | `map_pf` 的 `writable` 短路（region.c:711-713） | `MemType::writable` 存在（memtype.rs:20-24），由 memtype 判定 | ✅ 已实现（判定位置在 memtype） |
| 8 | `pt_clearmapcache()` 成功后调用 | Rust 无 mapcache 生产实现（24 范围） | ⚠️ 随 24 |
| 9 | 主动路径 `vfs_avail` 计算（requestor==VFS ? 0 : 1） | 无对应（do_memory DEFERRED） | ⚠️ DEFERRED |
| 10 | VFS 异步回调 `pf_cont`/`handle_memory_continue` | `NeedVfsIo → Suspended` 表示"等 VFS"，但回调接线未实现（23 范围） | ⚠️ DEFERRED |

---

## 4. 实现详解

### 4.1 dispatch_pagefault：主循环 P3 的 handler（vm_server.rs:742-771）

```
decode_message(msg)                        :746  m_source + m_vm_pagefault
  → vm_isokendpt(request.endpoint)         :748  失败 → InvalidProcess（C：panic）
  → table.get_active(slot)                 :752  失败 → InvalidProcess
  → regions_mut().find_mut(fault_addr)     :758  失败 → InvalidAddress（C：SIGSEGV 路径）
  → cow_exec_pf::handle_pagefault(...)     :763  判定 + 动作
  → Ok(_) → VmReply::Ok / Err → VmReply::Error(AccessViolation)
```

对应 C 的 `handle_pagefault`（pagefaults.c:76-158）映射：`vm_isokendpt` → :85；`map_lookup` → `find_mut`（region_map.rs:66，BTreeMap `range(..=addr).next_back()`，见 14）；写权限检查在 `AnonymousMemory::ev_pagefault` 内（§4.2）；`pt_clearmapcache` + 恢复进程 → 无对应（§3.6 #3）。

### 4.2 handle_pagefault 动作表与 memtype 决策

`cow_exec_pf::handle_pagefault`（:19-52）是分派器，真正的决策在 memtype：

**AnonymousMemory::ev_pagefault**（memtype.rs:220-256）：

| 条件 | 结果 | C 对照（mem_anon.c） |
|------|------|---------------------|
| slot 为 None / 未映射 | `NeedNewPage` | `anon_pagefault` 首次访问分配 |
| 已映射且 `refcount < 2` 或读 | `Handled` | 页已就绪 |
| 已映射、写、`refcount >= 2` | `NeedCow` | CoW 写保护分裂 |
| 写但区域不可写 | `AccessViolation` | C 在 handle_pagefault:110-118 SIGSEGV |

`is_writable`（vir_region.rs:143-145）检查 `VrFlags::WRITABLE`。

**MappedFile::ev_pagefault**（memtype.rs:904-935）：

| 条件 | 结果 |
|------|------|
| `VrParam::File.inited == false` | `NeedNewPage`（区域未初始化，首次访问） |
| 已映射 + 读 | `Handled` |
| 已映射 + 写 | `NeedCow` |
| 未映射 | `NeedVfsIo`（请求 VFS 读入） |

`NeedVfsIo → PagefaultAction::Suspended`（cow_exec_pf.rs:45-47）——类型层保留 SUSPEND 语义；VFS 回复回调（C `pf_cont`/`mappedfile_pf_cont`）在 23-vfs-interaction 范围。

### 4.3 cow_resolve_core：CoW 分裂动作（cow_exec_pf.rs:85-128）

```
get_slot(offset) → PageNotMapped（未映射）        :91-96
refcount = frames.get(old_pfn).refcount            :98-101
refcount <= 1 → return old_pfn（快速路径）          :103-105
new_pfn = alloc.alloc_pfn() → NoMemory             :107-108
copy_page_content(frames, old_pfn, new_pfn)        :110（direct map + copy_nonoverlapping :190-206）
unmap_page(old) + map_page(new, &MEM_TYPE_ANON)    :112-113
pending(old_pfn, memtype) → ev_unreference + free_pfn  :119-122
#[cfg(debug_assertions)] verify_cow_consistency    :124-125
```

与 C `mem_cow` 的对应：`alloc_mem` → `alloc_pfn`；`sys_abscopy` → `copy_page_content`（direct map，见 07）；`pb_new`/`pb_link` → `map_page`；`pb_unreferenced` → `unmap_page` + `ev_unreference` + `free_pfn` 两阶段释放（doc 11 的物理页引用计数语义）。`verify_cow_consistency`（:137-172）是 debug 构建下的不变量断言：old refcount ≥ 1、new refcount == 1、slot 指向 new_pfn。

### 4.4 VFS 回调状态机在 Rust 的现状

C 的核心状态机（`hm_state` + `handle_memory_continue` + `pf_cont`）在 Rust 中**尚未落地**：

- `PagefaultResult::NeedVfsIo`（memtype.rs:156）与 `PagefaultAction::Suspended`（cow_exec_pf.rs:240）**存在但无消费者**——`dispatch_pagefault` 目前把 `Ok(PagefaultAction::Suspended)` 也映射为 `VmReply::Ok`（vm_server.rs:767），即"挂起"被当作成功吞掉，进程不会恢复（§3.6 #3/#10）。
- `fork.rs::handle_memory_once`（:33-85）是唯一落地的主动路径，且只在 fork 时使用（18 详述）。
- 诚实标注：**这意味着一页文件映射未缓存时，当前 Rust 行为与 C 不等价**——C 发起 VFS 异步读，Rust 返回 Ok 但页未映射。此缺口必须在 23（VFS 请求队列）+ 24（页缓存）接线后闭环。

### 4.5 主循环接线（vm_server.rs:577-585）

```rust
// Priority 3: VM_PAGEFAULT (main.c:153-164)
if m_type == VM_PAGEFAULT {
    debug_assert!(
        is_from_kernel(rcv_sts),
        "faked VM_PAGEFAULT from {:?}", source
    );
    let _ = self.dispatch_pagefault(msg);
    return DispatchAction::NoReply;
}
```

- `VM_PAGEFAULT = 0xCFF`（vm_server.rs:978）——与 C 一致（com.h:773），`callnr()` 对 0xCFF 越界返回 None，进不了 `dispatch_by_number`，只能走 P3（15 详述）。
- `is_from_kernel` 是桩恒 true（vm_server.rs:889-892）——C 用 `IPC_STATUS_FLAGS_TEST(rcv_sts, IPC_FLG_MSG_FROM_KERNEL)`（main.c:154-157），真实状态字待 kernel IPC（15 §5.3 已标注）。
- `DispatchAction::NoReply` 保持 C `continue` 语义（main.c:164）——不回复，进程恢复靠 `VMCTL_CLEAR_PAGEFAULT`（Rust DEFERRED，§3.6 #3）。

---

## 5. 测试要点

### 5.1 单元测试清单（grep 实证，2026-08-16）

**cow_exec_pf.rs**（5 个，:280-365）：

| 测试 | 位置 | 契约 |
|------|------|------|
| test_alloc_and_map | :281 | 分配 + 映射：slot 已映射且 pfn 正确 |
| test_cow_resolve | :295 | refcount=2 写 → 新 pfn、旧 refcount 1、新 refcount 1 |
| test_cow_resolve_no_sharing | :314 | refcount=1 → 返回原 pfn（快速路径） |
| test_cow_resolve_region | :328 | 区域批量解析，返回已解析页数 |
| test_cow_resolve_core_refcount_one_fast_path | :350 | refcount=1 快速路径不动映射 |

**minix-types vm.rs**（2 个，本轮新增）：

| 测试 | 位置 | 契约 |
|------|------|------|
| test_vm_pagefault_in_decode_message | :1034 | `m_source` + `m_vm_pagefault`（vpf_addr/vpf_flags）解码，W 位 → write=true |
| test_vm_pagefault_in_decode_read_fault | :1052 | 无 W 位 → write=false |

**memtype.rs 相关**（判定层）：`AnonymousMemory::writable`/`ev_pagefault` 的决策表由 memtype 单测覆盖（`test_anon_writable` 等），`MappedFile::ev_pagefault` 的 `NeedVfsIo` 分支无直接单测（§5.3）。

### 5.2 覆盖维度

- **解码**：wire format（m_source + 专用 union 成员）、W 位判定（C `PFERR_WRITE` 等价）。
- **分配动作**：`alloc_and_map` 的 slot 状态（映射 + pfn）。
- **CoW 分裂**：refcount=2 → 复制 + 换 pfn；refcount=1 → 快速路径；批量区域解析。
- **不变量**：debug 构建 `verify_cow_consistency`（old ≥ 1 / new == 1 / slot 指向 new）。
- **错误路径**：`CowError`/`CowCoreError` 枚举 + `From` 提升（编译期保证无遗漏分支）。

### 5.3 覆盖缺口与诚实标注

| 缺口 | 状态 | 说明 |
|------|------|------|
| dispatch_pagefault 端到端单测 | ⚠️ 缺失 | 主循环 P3 → decode → 区域查找 → handle_pagefault 无消息级测试（is_from_kernel 桩恒 true） |
| SIGSEGV / VMCTL_CLEAR_PAGEFAULT 恢复进程 | ⚠️ 未实现 | VM 无 sys_vmctl；`let _ = dispatch_pagefault(msg)` 丢弃回复，进程恢复契约 DEFERRED（§3.6 #3） |
| major/minor 计数生产接线 | ⚠️ 缺失 | `inc_minor_fault`/`inc_major_fault`（vmproc_handle.rs:300-307）仅测试调用，`dispatch_pagefault` 未统计 |
| `NeedVfsIo → Suspended` 的 VFS 回调 | ⚠️ 未接线 | 对应 C `pf_cont`/`handle_memory_continue`；`PagefaultAction::Suspended` 被当作 Ok 吞掉（§4.4），23/24 范围 |
| do_memory / handle_memory_start/step/final/continue | ⚠️ 未实现 | SIGKMEM 主动路径 DEFERRED；仅 `fork.rs::handle_memory_once` 同步子集 |
| MappedFile::ev_pagefault 的 NeedVfsIo 分支单测 | ⚠️ 缺失 | 判定表无直接测试（VFS 请求队列未接线，无法构造真实 I/O） |
| pf_errstr 诊断字符串 | ⚠️ 简化 | no_std 无 printf；错误以类型传递，无 C 的可读日志 |

### 5.4 测试统计（截至 2026-08-16）

```
$ cd os && cargo test -p minix-vm --lib
→ 360 passed / 1 failed（test_map_lazy pre-existing，13 范围，§5.3 已标注）
$ cargo test -p minix-vm --lib cow_exec_pf → 5 passed
$ cargo test -p minix-types --lib ipc::vm → 17 passed（含本轮新增 decode 2 个）
$ cargo check -p minix-vm → Finished（110 warnings pre-existing，无 error）
```

---

## 6. 过渡

位置可回答性：本文档的页错误处理挂在**主循环 P3 分支**（main.c:153-164 / vm_server.rs:577-585）与 **SIGKMEM 信号**（main.c:731-750）两个锚点上——前者是 15 分发模型的特例路径，后者是 01 SEF 信号处理器的入口。两条路径汇合于 `map_pf`（region.c:664-754），即 13 的区域-物理侧桥接。

向下游的移交：

- **17-cow-mechanism**：本文档只消费 `NeedCow → cow_resolve` 的动作接口；引用计数如何建立（fork 的 `ev_copy`/`ev_reference`）、`mem_cow` 的写保护设置与分裂细节在 17。
- **18-vm-fork**：`fork.rs::handle_memory_once` 的调用方（fork 后预填充消息缓冲页）在 18 全流程。
- **23-vfs-interaction / 24-page-cache**：`NeedVfsIo → Suspended` 的 VFS 回调接线（C `pf_cont`/`handle_memory_continue`/`handle_memory_final`）依赖 VFS 请求队列与页缓存，闭环后本文档 §3.6 #6/#10 与 §4.4 的缺口标注转为已实现。
- **26-vm-queries**：`vm_minor_page_fault`/`vm_major_page_fault` 的消费面（getrusage）。

---

## 7. 参见

- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/13-region-mapping.md` — `map_pf`/`map_handle_memory`/`map_lookup`（两条路径的汇合点）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/15-ipc-dispatch.md` — 主循环 P3 分发、SUSPEND 协议、`is_from_kernel` 桩
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/12-memtype.md` — `ev_pagefault`/`writable` 回调语义与 6 类 memtype
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/17-cow-mechanism.md` — CoW 分裂机制（`mem_cow`/引用计数）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/18-vm-fork.md` — `handle_memory_once` 的 fork 调用方
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/23-vfs-interaction.md`、`24-page-cache.md` — VFS 回调与页缓存（SUSPEND 恢复闭环）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/11-phys-pagestate.md` — 物理页引用计数（`unmap_page` + `ev_unreference` + `free_pfn` 两阶段释放）
- `minix3/minix/servers/vm/pagefaults.c`（:33-49/:59-70/:76-158/:161-168/:170-235/:240-252/:254-289/:294-334/:336-417）— C 页错误处理
- `minix3/minix/servers/vm/region.c`（`map_pf` :664-754 / `map_handle_memory` :756-774 / `map_lookup` :616-641）— C 汇合点
- `minix3/minix/servers/vm/main.c`（:153-164 / :731-750）— P3 分发与 SIGKMEM
- `minix3/minix/include/minix/com.h`（:395/:397/:398/:773-775/:1151）、`minix3/minix/include/minix/vm.h`（:38）、`minix3/minix/include/minix/syslib.h`（:61-64）、`minix3/minix/servers/vm/arch/i386/pagetable.h`（:36-39）、`minix3/minix/servers/vm/memtype.h`（:9）— 常量与接口
- `os/servers/vm/src/cow_exec_pf.rs`、`os/servers/vm/src/vm_server.rs`（:577-585/:742-771/:889-892）、`os/servers/vm/src/fork.rs`（:33-85）、`os/servers/vm/src/memtype.rs`（:152-158/:220-256/:904-935）、`os/servers/vm/src/region/vir_region.rs`（:230-239）— Rust 实现
- `os/libs/minix-types/src/ipc/vm.rs`（`VM_PAGEFAULT` :149 / `VmPagefaultIn` :373-377 / `decode_message` :379-398）、`os/libs/minix-types/src/ipc/message.rs`（`MessVmPagefault` :1486-1495）、`os/kernel/src/page_fault.rs`（`build_vm_pagefault_msg` :142-166）— 消息与 wire format
