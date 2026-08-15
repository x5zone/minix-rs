# 02-vmproc-struct: 进程控制块——vmproc 结构、状态标志与生命周期

> **分类**: 阶段 1 — 启动入口与进程模型（进程模型锚点）
> **源码**: `minix3/minix/servers/vm/vmproc.h`（结构 + `VMF_*` 宏）；配套行为分布在 `main.c:262-285/458-462/498-520/577-579`、`exit.c:25-107`、`region.c:85-90/391/402`、`utility.c:455-457`、`pagefaults.c:136-138`、`fork.c:67/83`
> **Rust 模块**: `os/servers/vm/src/vmproc/`（`vmproc.rs` / `flags.rs` / `vmproc_handle.rs` / `mod.rs`）+ `os/servers/vm/src/vm_server.rs`（`init_proc` 族）
> **前置**: `notes/rewrite/fork-syscall-rewrite/02-stage-vm/01-vm-init-main.md`（`memset(vmproc)` 与 `init_proc(VM_PROC_NR)` 调用点）、`notes/rewrite/fork-syscall-rewrite/02-stage-vm/00-vm-overview.md`
> **说明**: VM 进程控制块（PCB）`struct vmproc` 的全部字段语义、`VMF_*` 正交状态标志、生命周期状态机（空闲→活跃→退出中→空闲），以及 `init_proc()` 槽位激活语义。进程表管理（`vmproc[VMP_NR]` 全局表、`vm_isokendpt`、`VMP_EXECTMP`）在 `03-vmproc-table.md`。

---

## 1. 概念：进程控制块——VM 如何"记住"每个进程

### 1.0 章节引言

VM 是用户态内存服务：PM 告诉它"进程要 fork / 要退出 / 地址空间要变"，VFS 告诉它"这段映射要失效"，内核告诉它"这个进程缺页了"。所有这些消息都带着一个 **endpoint**——一个全局唯一的进程身份。VM 收到消息后的第一个动作几乎总是：*把 endpoint 翻译成进程槽，找到那个进程的进程控制块*。

进程控制块（Process Control Block, PCB）是操作系统最古老的数据结构概念之一：**一个进程在服务侧的全部状态，集中放在一个按槽位索引的记录里**。Minix3 的 VM 用 `struct vmproc`（`vmproc.h:14-32`）承担这个角色。本文档回答三个问题：

1. **PCB 里有什么**——身份、地址空间、权限、统计，为什么是这些字段？
2. **PCB 的状态怎么表达**——为什么 `vm_flags` 是正交位而不是枚举？
3. **PCB 的生死如何流转**——槽位从空闲到激活再到回收，谁在什么时机改哪些字段？

### 1.1 PCB 是什么：槽位 + 内存资源 + 身份

`vmproc` 是一个**按进程槽组织的记录**。进程槽（slot）是 VM 内部数组 `vmproc[VMP_NR]` 的索引；`VMP_NR = _NR_PROCS + 1`（`glo.h:18`），即所有用户进程数加一个 exec 临时槽。每个槽位在进程生命周期内保存：

- **身份**：`vm_slot`（我在哪一格）、`vm_endpoint`（我是谁）、`vm_flags`（我处于什么状态）
- **地址空间**：`vm_pt`（页表）、`vm_regions_avl`（虚拟区域树）、`vm_region_top`（区域分配 hint）
- **权限**：`vm_acl`（能调用哪些 VM 系统调用）
- **启动信息**：`vm_boot`（仅 boot 进程，指向内核 boot 映像条目）
- **统计**：`vm_total` / `vm_total_max`（虚拟内存量与峰值）、`vm_minor_page_fault` / `vm_major_page_fault`（缺页计数）、`vm_bytecopies`（VMSTATS 调试计数）

**设计本质**：`vmproc` 不是"进程的全部"——进程的代码、数据、内核上下文都在别处（PM、内核、文件系统）。它是 **VM 维度上的进程投影**：只保存 VM 需要知道的部分。这与 PM 的 `mproc`、内核的 `proc` 形成三个服务各持一份进程视图的分层模型。

### 1.2 身份双通道：vm_slot 与 vm_endpoint

一个进程在 VM 里有**两个身份通道**，用途不同、信息冗余：

| 通道 | 类型 | 含义 | 用途 |
|------|------|------|------|
| `vm_slot` | `int` | 数组索引 | O(1) 定位 PCB（`vmproc[slot]`） |
| `vm_endpoint` | `endpoint_t` | 全局唯一 IPC 身份 | 消息路由、跨服务识别 |

endpoint 的编码含 generation（代数）与 slot 两部分（`endpoint = (slot << 8) | generation` 一类编码，见 `os/libs/minix-types/src/types/endpoint.rs:82` 的 `from_generation_slot`）。因此 `vm_slot` 理论上可以从 endpoint 提取，C 代码仍保留它，原因有二：

1. **fork 中间态**：PM 先分配子进程 slot 再发 `VM_FORK`，在 `sys_fork()` 返回前子进程**只有 slot、没有 endpoint**（`fork.c:63` 显式把 `vmc->vm_endpoint = NONE`）。这段时间只有 `vm_slot` 能定位 PCB。
2. **一致性断言**：`assert(p->vm_slot == _ENDPOINT_P(p->vm_endpoint))` 类检查能在调试期发现身份不变量被破坏（Rust 侧对应 `UserSlot::matches`，见 §3.1）。

**冗余换取工程可控性**：允许短暂的不一致（slot 已知、endpoint 未生成），但让"我在操作哪个进程"永远有一个 O(1) 答案。

### 1.3 状态正交性：三个 VMF 位不是枚举

`vm_flags` 的三个位（`vmproc.h:34-37`）：

```c
#define VMF_INUSE       0x001   /* slot contains a process */
#define VMF_EXITING     0x002   /* PM is cleaning up this process */
#define VMF_VM_INSTANCE 0x010   /* This is a VM process instance */
```

它们是**正交状态位**，不是互斥枚举。典型组合：

- `VMF_INUSE`：槽位有活跃进程（所有存活进程必备）
- `VMF_INUSE | VMF_EXITING`：进程正在退出（`do_willexit` 后、`do_exit` 完成前）
- `VMF_INUSE | VMF_VM_INSTANCE`：VM 自身槽（`main.c:579` 设置）

注意 `0x004` 与 `0x008` 是**空位**——历史上保留，当前版本未使用。任何新状态位应从空位开始，避免重排已有位值。

为什么用位不用枚举？因为**状态可以叠加**：一个进程退出中且恰好是 VM 实例（`VMF_INUSE|VMF_EXITING|VMF_VM_INSTANCE`）在概念上是合法的，枚举表达这种组合需要嵌套布尔字段，退化成结构体。位标志与 C 源码逐位兼容，也让"只继承 INUSE、清除其他位"（`fork.c:83` 的 `vmc->vm_flags &= VMF_INUSE`）成为一条位运算。

### 1.4 初始化语义：构造即空、激活、回收

槽位生命周期围绕三个操作展开：

1. **构造即空**（`main.c:458-462`）：`memset(vmproc, 0, sizeof(vmproc))` 把整张表清零（所有槽位的 `vm_flags=0` → 全部空闲），再循环写 `vmproc[i].vm_slot = i`。**清零与赋值分离**：内存先全零，身份字段后补。
2. **激活**（`init_proc()`，`main.c:262-285`）：从 boot 映像表找到目标进程条目，断言槽位空闲，然后只写三个字段——`vm_flags = VMF_INUSE`、`vm_endpoint = ip->endpoint`、`vm_boot = ip`。**激活不重建**：地址空间、ACL、统计字段保持零值，由后续流程填充。
3. **回收**（`clear_proc()`，`exit.c:45-54`）：进程退出后重置区域、清 ACL、`vm_flags = 0`、统计归零，槽位回到"空闲"。`free_proc()`（`exit.c:33-42`）先释放页表与映射页，`clear_proc()` 再重置簿记字段。

### 1.5 生命周期状态机：两步退出协议

进程在 VM 眼中的生命周期：

```text
        激活(init_proc / fork)
空闲 ────────────────────────────→ 活跃(INUSE)
  ↑                                    │
  │                                    │ do_willexit (设 EXITING)
  │                                    ▼
  │                             退出中(INUSE|EXITING)
  │                                    │
  └────── 回收(do_exit: free+clear) ───┘
```

退出是**两步协议**（`exit.c`）：

1. PM 发 `VM_WILLEXIT` → `do_willexit()`（`exit.c:100-107`）设 `VMF_EXITING`——进程进入"退出中"，但资源还在。
2. PM 完成自己的清理后发 `VM_EXIT` → `do_exit()`（`exit.c:60-97`）——**先检查 `VMF_EXITING`**（`exit.c:73`，未经过 WILLEXIT 的退出请求返回 `EINVAL`），若 `VMF_VM_INSTANCE` 则递减 `num_vm_instances`（`exit.c:77-79`），再 `free_proc()` + `clear_proc()`。

两步协议的意义：**WILLEXIT 是"预告"，EXIT 是"执行"**。VM 在两步之间仍持有进程资源，但知道它已不可信——后续对这个进程的内存操作（如 VFS 的 FORGETCACHE）可以依据 `VMF_EXITING` 拒绝或安全降级。

---

## 2. C 源码分析

### 2.1 struct vmproc：全部字段（vmproc.h:14-32）

```c
struct vmproc {
	int		vm_flags;
	endpoint_t	vm_endpoint;
	pt_t		vm_pt;	/* page table data */
	struct boot_image *vm_boot; /* if boot time process */

	/* Regions in virtual address space. */
	region_avl vm_regions_avl;
	vir_bytes  vm_region_top;	/* highest vaddr last inserted */
	int vm_acl;
	int vm_slot;		/* process table slot */
#if VMSTATS
	int vm_bytecopies;
#endif
	vir_bytes	vm_total;
	vir_bytes	vm_total_max;
	u64_t		vm_minor_page_fault;
	u64_t		vm_major_page_fault;
};
```

按职责分四组：

| 分组 | 字段 | C 类型 | 语义 |
|------|------|--------|------|
| 身份 | `vm_flags` | `int` | 状态位（§2.2） |
| 身份 | `vm_endpoint` | `endpoint_t` | 全局 IPC 身份 |
| 身份 | `vm_slot` | `int` | 表索引（`glo.h` 数组下标） |
| 地址空间 | `vm_pt` | `pt_t` | 页表（arch 相关，见 07） |
| 地址空间 | `vm_regions_avl` | `region_avl` | 虚拟区域 AVL 树根（见 13） |
| 地址空间 | `vm_region_top` | `vir_bytes` | 最近插入区域的最高地址（分配 hint） |
| 权限 | `vm_acl` | `int` | ACL 索引（`NO_ACL`/`USER_ACL`/系统槽，见 04） |
| 启动 | `vm_boot` | `struct boot_image *` | 仅 boot 进程的映像条目指针 |
| 统计 | `vm_total` / `vm_total_max` | `vir_bytes` | 虚拟内存当前量 / 历史峰值 |
| 统计 | `vm_minor_page_fault` / `vm_major_page_fault` | `u64_t` | 缺页计数 |
| 统计 | `vm_bytecopies` | `int` | 仅 `VMSTATS`（`vm.h` 默认 0）的字节复制计数 |

### 2.2 VMF_* 状态标志（vmproc.h:34-37）

三个位 + 两个空位（0x004/0x008）。位值从 1 开始跳 0x010，说明历史上中间位被占用过或被规划。各位的读写点：

- `VMF_INUSE`：写——`init_proc`（`main.c:278` 设）、`fork.c:83`（`&= VMF_INUSE` 只继承）、`clear_proc`（`exit.c:49` 清）。读——`init_proc` 的 `assert(!(vmp->vm_flags & VMF_INUSE))`（`main.c:276`）、`vm_isokendpt` 的活跃检查（03）。
- `VMF_EXITING`：写——`do_willexit`（`exit.c:112` 设）。读——`do_exit`（`exit.c:73` 检查，未设置返回 `EINVAL`）。
- `VMF_VM_INSTANCE`：写——`main.c:579`（`vmproc[VM_PROC_NR].vm_flags |= VMF_VM_INSTANCE`）、`exit.c:78`（退出时清除并递减 `num_vm_instances`）。读——`do_exit`（`exit.c:77`）。

### 2.3 init_proc()：槽位激活（main.c:262-285）

```c
static struct vmproc *init_proc(endpoint_t ep_nr)
{
	struct boot_image *ip;

	for (ip = &kernel_boot_info.boot_procs[0];
		ip < &kernel_boot_info.boot_procs[NR_BOOT_PROCS]; ip++) {
		struct vmproc *vmp;

		if(ip->proc_nr != ep_nr) continue;

		if(ip->proc_nr >= _NR_PROCS || ip->proc_nr < 0)
			panic("proc: %d", ip->proc_nr);

		vmp = &vmproc[ip->proc_nr];
		assert(!(vmp->vm_flags & VMF_INUSE));	/* no double procs */
		clear_proc(vmp);
		vmp->vm_flags = VMF_INUSE;
		vmp->vm_endpoint = ip->endpoint;
		vmp->vm_boot = ip;

		return vmp;
	}

	panic("no init_proc");
}
```

语义要点：

1. **线性查找**：在 `kernel_boot_info.boot_procs[NR_BOOT_PROCS]` 里按 `proc_nr == ep_nr` 找目标条目——boot 映像表很小（系统服务数），线性查找足够。
2. **范围 panic**：`proc_nr >= _NR_PROCS || proc_nr < 0` → `panic("proc: %d")`——内核给出的 boot 表不应越界。
3. **防重复激活**：`assert(!(vmp->vm_flags & VMF_INUSE))`——同一槽位被激活两次是编程错误。
4. **激活只写三字段**：`clear_proc()` 兜底重置 → `VMF_INUSE` → `vm_endpoint` → `vm_boot`。**注意 `clear_proc()` 会清 `vm_flags=0`，所以顺序是先 clear 再设 INUSE**。
5. **未找到 panic**：`panic("no init_proc")`——调用方传的 `ep_nr` 必须存在于 boot 表。

调用点（均在 `init_vm()` 内）：

- `main.c:474`：`init_proc(VM_PROC_NR)`——VM 自身槽，随后 `pt_init()`（arch 初始化）。
- `main.c:498-520`：boot 循环——跳过 `ip->proc_nr < 0`（内核任务）与 `VM_PROC_NR`（VM 自身已在 474 激活），`assert(ip->start_addr)` 后对每个 boot 进程 `init_proc(ip->proc_nr)` + `exec_bootproc()` + 释放文件 blob。

### 2.4 槽位清零与全局表初始化（main.c:458-462）

```c
/* Set table to 0. This invalidates all slots (clear VMF_INUSE). */
memset(vmproc, 0, sizeof(vmproc));

for(i = 0; i < ELEMENTS(vmproc); i++) {
	vmproc[i].vm_slot = i;
}
```

这是 PCB 表的**唯一一次性初始化**：`memset` 全零使所有槽位空闲（`vm_flags=0`），随后逐槽写 `vm_slot=i` 建立索引身份。之后槽位的激活/回收都在这张已初始化的表上原地进行。

### 2.5 free_proc() / clear_proc() / reset_vm_rusage()（exit.c:25-54）

```c
static void reset_vm_rusage(struct vmproc *vmp)
{
	vmp->vm_total = 0;
	vmp->vm_total_max = 0;
	vmp->vm_minor_page_fault = 0;
	vmp->vm_major_page_fault = 0;
}

void free_proc(struct vmproc *vmp)
{
	map_free_proc(vmp);
	pt_free(&vmp->vm_pt);
	region_init(&vmp->vm_regions_avl);
#if VMSTATS
	vmp->vm_bytecopies = 0;
#endif
	vmp->vm_region_top = 0;
	reset_vm_rusage(vmp);
}

void clear_proc(struct vmproc *vmp)
{
	region_init(&vmp->vm_regions_avl);
	acl_clear(vmp);
	vmp->vm_flags = 0;		/* Clear INUSE, so slot is free. */
#if VMSTATS
	vmp->vm_bytecopies = 0;
#endif
	vmp->vm_region_top = 0;
	reset_vm_rusage(vmp);
}
```

分工：

- `free_proc()` 释放**外部资源**：`map_free_proc`（映射页，region.c）、`pt_free`（页表，pagetable.c），然后重置簿记字段。
- `clear_proc()` 重置**簿记字段**：区域树、ACL（`acl_clear` 释放 ACL 槽并置 `NO_ACL`，`acl.c:121-129`）、`vm_flags=0`（**这是槽位空闲的判定**）、统计。
- 两者都调用 `reset_vm_rusage()`；`vm_endpoint` 与 `vm_boot` **不被重置**——C 依赖 `vm_flags=0` 表示空闲，后续 `*vmc = *vmp` 式整结构覆盖会重写所有字段，旧值不会被读到。

### 2.6 do_willexit / do_exit：两步退出协议（exit.c:60-107）

```c
int do_exit(message *msg)
{
	int proc;
	struct vmproc *vmp;

	if(vm_isokendpt(msg->VME_ENDPOINT, &proc) != OK) {
		printf("VM: bogus endpoint VM_EXIT %d\n", msg->VME_ENDPOINT);
		return EINVAL;
	}
	vmp = &vmproc[proc];

	if(!(vmp->vm_flags & VMF_EXITING)) {
		printf("VM: unannounced VM_EXIT %d\n", msg->VME_ENDPOINT);
		return EINVAL;
	}
	if(vmp->vm_flags & VMF_VM_INSTANCE) {
	    vmp->vm_flags &= ~VMF_VM_INSTANCE;
	    num_vm_instances--;
	}

	{
		/* Free pagetable and pages allocated by pt code. */
		free_proc(vmp);
	}

	/* Reset process slot fields. */
	clear_proc(vmp);
	return OK;
}

int do_willexit(message *msg)
{
	int proc;
	struct vmproc *vmp;

	if(vm_isokendpt(msg->VMWE_ENDPOINT, &proc) != OK) {
		printf("VM: bogus endpoint VM_EXITING %d\n", msg->VMWE_ENDPOINT);
		return EINVAL;
	}
	vmp = &vmproc[proc];

	vmp->vm_flags |= VMF_EXITING;
	return OK;
}
```

关键点：

- `do_willexit`（`exit.c:100-107`）只设 `VMF_EXITING`，不释放任何资源。
- `do_exit`（`exit.c:60-97`）先 `vm_isokendpt` 验证 endpoint，再检查 `VMF_EXITING`（未预告的退出 → `EINVAL`），然后处理 VM 实例计数，最后 `free_proc` + `clear_proc`。
- **`num_vm_instances` 与 `VMF_VM_INSTANCE` 的同步**：设置侧在 `main.c:577-579`（`num_vm_instances = 1; vmproc[VM_PROC_NR].vm_flags |= VMF_VM_INSTANCE`），清除侧在 `do_exit`（`exit.c:77-79`）——两处必须保持配对。

### 2.7 字段读写点：region / utility / pagefaults / fork / acl

**`vm_total` / `vm_total_max`**（`region.c:85-90`，`map_pageblock` 内）：

```c
proc->vm_total += VM_PAGE_SIZE;
if (proc->vm_total > proc->vm_total_max)
	proc->vm_total_max = proc->vm_total;
```

按物理页增减（加页 +`VM_PAGE_SIZE`，回收页减）。`vm_total_max` 是**历史峰值**，不是硬上限——`add_total` 在超过时顺带更新。读取点在 `utility.c:455`（`r_usage.ru_maxrss = vmp->vm_total_max / 1024L`，getrusage）与 `region.c:1444`（VM 自身查询）。

**`vm_region_top`**（`region.c:385-410`）：

- 写入：`region_find_slot_range` 成功后 `vmp->vm_region_top = startv + length`（`region.c:391`）——"最高 vaddr 最后插入"。
- 读取：`region_find_slot` 用 `hint = vmp->vm_region_top` 作为 `minv` 起点（`region.c:402-411`）——**分配 hint**，让连续映射尽量靠拢。

**缺页计数**（`pagefaults.c:136-138`）：

```c
if (io)
	vmp->vm_major_page_fault++;
else
	vmp->vm_minor_page_fault++;
```

`map_pf` 触发实际 I/O（`io` 非零）→ major；否则（如 CoW 复制、页表补映射）→ minor。读取点在 `utility.c:456-457`（getrusage）。

**fork 继承**（`fork.c:41-92`）：

- `fork.c:67`：`vmc->vm_bytecopies = 0;`（VMSTATS 清零）
- `fork.c:59-64`：`*vmc = *vmp` 整结构复制父进程 → 修正 `vm_slot = childproc` → `region_init` → `vm_endpoint = NONE`（防误用）→ 恢复 `vm_pt = origpt`（子进程页表槽）
- `fork.c:83`：`vmc->vm_flags &= VMF_INUSE;`——**只继承 INUSE**，清除 `EXITING` 与 `VM_INSTANCE`，子进程以干净状态开始
- `fork.c:86`：`acl_fork(vmc)`（`acl.c:110-114`）——`vm_acl != USER_ACL` 时置 `NO_ACL`（系统进程子进程需 RS 重新授权）

**`vm_acl`**（`acl.c:20-129`）：

- `NO_ACL = -1`（`acl.c:10`）：未分配 ACL——`acl_check` 对无 ACL 进程**暂时放行**（`acl.c:45-53`，会打印警告；RS 进程例外）
- `USER_ACL = 0`（`acl.c:11`）：用户进程共享 ACL 槽
- `FIRST_SYS_ACL = 1` 起的系统进程独立槽（`acl.c:12`）
- `acl_init`（`acl.c:20-33`）：全表 `vm_acl = NO_ACL`
- `acl_clear`（`acl.c:121-129`）：非 `NO_ACL` 时释放系统槽位并置 `NO_ACL`

---

## 3. Rust 设计决策

### 3.1 D1: 字段命名保留 `vm_` 前缀 + newtype 类型化

- **C**: 裸 `int`/`endpoint_t`/`vir_bytes`/指针（`vmproc.h:14-32`）
- **Rust**: `VmProc` 字段名与 C 逐一对齐（`vm_slot`/`vm_endpoint`/`vm_flags`/`vm_acl`/`vm_boot`/`vm_pt`/`vm_regions`/`vm_region_top`/`vm_total`/`vm_total_max`/`vm_minor_page_fault`/`vm_major_page_fault`/`vm_bytecopies`），但类型换用 newtype：`UserSlot`、`Endpoint`、`VirBytes`（`minix-types`），`AclState`（VM 私有，见 04），`Option<BootImage>`（替代 C 指针）
- **理由**: 保留 `vm_` 前缀使 C↔Rust 审计可机械对照；newtype 杜绝"int 当 slot 用"类错误
- **取舍**: `vm_regions_avl` → `vm_regions: MaybeUninit<RegionMap>`——Rust 用 `RegionMap`（内部平衡树）替代 C 的 AVL 句柄，命名去掉 `_avl` 后缀但注释保留对应关系

### 3.2 D2: 状态位用 bitflags(u8) 表达

- **C**: `int vm_flags` + 三个宏（`vmproc.h:34-37`）
- **Rust**: `bitflags! { pub struct VmFlags: u8 { IN_USE=0x001, EXITING=0x002, VM_INSTANCE=0x010 } }`（`flags.rs:5-32`）
- **理由**: 位值与 C 完全一致（可测试断言）；`u8` 容纳 0x010 且节省内存；`bitflags` 提供 `contains/insert/remove` 与 `|` 组合
- **取舍**: 底层类型 `u32`→`u8` 是内部表达变化，不改变外部语义（三个标志位值不变）

### 3.3 D3: `MaybeUninit<T> + bool` 表达延迟初始化

- **C**: `memset(vmproc,0)` 后 `vm_pt`/`vm_regions_avl` 的零内存即"有效"（`main.c:458`）
- **Rust**: `vm_pt: MaybeUninit<PageTable>` + `vm_pt_initialized: bool`；`vm_regions` 同理（`vmproc.rs:36-49`）
- **理由**: C 的零初始化对 Rust 类型（`PageTable`、`RegionMap`）不构成合法值；`bool` 守卫保证 `assume_init_mut()/assume_init_ref()` 仅在 `init_page_table()`/`init_regions()` 之后调用
- **取舍**: `MaybeUninit` 需要 unsafe 访问——用 `initialized` 守卫把 unsafe 面收敛到 `vmproc_handle.rs` 的少数方法

### 3.4 D4: `vacant()` 构造即空

- **C**: `memset(vmproc,0)` + `vmproc[i].vm_slot=i`（`main.c:458-462`）
- **Rust**: `const fn vacant()`（`vmproc.rs:68-91`）+ `const fn vacant_with_slot(slot)`（`vmproc.rs:93-98`）；进程表 `static VM_PROC_TABLE` 用 `[const { AssumeSyncCell::new(VmProc::vacant()) }; VM_PROC_COUNT]` 编译期构造（`table.rs:59-62`）
- **理由**: "清零+重建"在 Rust 中即"构造即空"——消灭显式初始化顺序错误类；`const` 构造使空槽零成本
- **行为契约**: 空槽 `flags.is_empty()`、`endpoint.is_none()`、两个 initialized 守卫为 `false`

### 3.5 D5: typestate view 生命周期状态机

- **C**: 运行时 `vm_flags` 位检查（`do_exit` 检查 `VMF_EXITING`，`exit.c:73`）
- **Rust**: 三个借用视图（`vmproc_handle.rs`）：
  ```text
  EmptySlot --activate--> ActiveProc --mark_exiting--> ExitingProc --reap--> EmptySlot
            ActiveProc --force_clear--> EmptySlot
  ```
- **理由**: 非法迁移（对 ActiveProc 直接 reap、对空槽 force_clear）从运行时错误变为**编译期错误**——视图类型上只有合法方法
- **取舍**: 视图构造有 `debug_assert`（`EmptySlot::new` 要求非 INUSE，`ActiveProc::new` 要求 INUSE 且非 EXITING，`ExitingProc::new` 要求 INUSE 且 EXITING）——调试期捕获不一致

### 3.6 D6: `clear()` 合并 free_proc + clear_proc

- **C**: `free_proc()`（`exit.c:33-42`）+ `clear_proc()`（`exit.c:45-54`）两步；`do_exit` 先处理 VM 实例计数（`exit.c:77-79`）
- **Rust**: `unsafe fn clear()`（`vmproc.rs:152-207`）单步完成：`vm_regions.clear()` → `vm_pt.destroy()`（非 test）→ VM_INSTANCE 计数递减 → flags/endpoint/boot/ACL/统计全部重置
- **理由**: 单线程 VM 中两步合并无观察者；`endpoint`/`boot` 重置比 C 更彻底（C 依赖 `vm_flags=0` 隐式空闲，Rust typestate 体系下 `EmptySlot` 可能被多次读取，残留值会导致 `check()` 误判）
- **行为契约**: clear 后 slot 可再次 `activate()`；VM_INSTANCE 计数与标志同步（`mark_vm_instance` 增、`clear` 减）

### 3.7 D7: `activate()` 严格 vs `activate_relaxed()` 宽松

- **C**: `init_proc` 直接写 `vm_flags=VMF_INUSE; vm_endpoint=ip->endpoint`（`main.c:278-279`），无一致性验证
- **Rust**: `EmptySlot::activate`（`vmproc_handle.rs:53-103`）严格模式 `debug_assert_eq!(endpoint.slot(), self.slot())`；`activate_relaxed`（`vmproc_handle.rs:106-160`）跳过严格配对，仅保留 debug 检查（endpoint slot 范围 + 匹配性或 exec 例外）
- **理由**: 正常创建路径（init_proc、fork 完成后端）应暴露 slot/endpoint 不匹配的编程错误；fork（endpoint=NONE）、exec 临时槽（`VM_EXEC_TMP_SLOT`）、测试是合法例外
- **行为契约**: `activate_relaxed` 对非 NONE endpoint 检查 `slot < VM_PROC_COUNT` 且 `ep_slot == self.slot() || self.slot() == VM_EXEC_TMP_SLOT`（`vmproc_handle.rs:124-152`，debug 构建）

### 3.8 D8: init_proc 落地为 VmServer::init_proc

- **C**: `init_proc(ep_nr)`（`main.c:262-285`）
- **Rust**: `VmServer::init_proc(table, ip: BootImage)`（`vm_server.rs:328-338`）：
  ```rust
  let slot = UserSlot(ip.proc_nr as usize);
  let empty = table.get_empty(slot).expect("init_proc: slot already in use");
  let mut proc = empty.activate(ip.endpoint);
  proc.set_boot(ip);
  ```
- **理由**: `get_empty().expect(...)` 复刻 C 的 `assert` + `panic("no init_proc")` 语义；`activate` + `set_boot` 对应 C 的 INUSE/endpoint/boot 三字段写入
- **差异说明**: 调用方（`init_vm_slot` `vm_server.rs:293-304`、`init_boot_procs` `vm_server.rs:306-325`）跳过负 proc_nr、跳过 VM 自身、**额外跳过 `endpoint.is_none()` 的填充条目**——minix-types 的 `[BootImage; NR_BOOT_PROCS]` 定长数组含 padding，C 的数组精确填充（01 文档已述）；`assert(start_addr != 0)`（`vm_server.rs:319-321`）复刻 C `main.c:504` 的 assert

---

## 4. 实现详解

### 4.1 `os/servers/vm/src/vmproc/vmproc.rs`：VmProc 结构

**结构定义**（`vmproc.rs:27-61`）：14 字段（含 cfg 条件字段），全部 `pub(crate)`；结构本身 `pub(crate)` 且**不导出模块外**（见 §4.4）。

**构造**：

- `vacant()`（`vmproc.rs:68-91`）：`const` 构造，对应 C `memset(vmproc,0)`——`vm_endpoint: Endpoint::NONE`、`vm_flags: VmFlags::empty()`、`vm_acl: AclState::Uninitialized`、两个 `MaybeUninit::uninit()` + 守卫 false、统计全零
- `vacant_with_slot(slot)`（`vmproc.rs:93-98`）：在 vacant 基础上写 `vm_slot`，对应 C `vmproc[i].vm_slot = i`

**查询方法**（`vmproc.rs:101-126`）：`is_in_use()` / `is_exiting()` / `is_vm_instance()` 封装 `contains()`；`check()`（debug_assertions）验证 `IN_USE ⇒ endpoint 非 NONE` 不变量。

**`clear()`**（`vmproc.rs:155-200`）：unsafe，调用前置条件见 §3.6。实现顺序：

1. `vm_regions_initialized` 时 `vm_regions.assume_init_mut().clear()`
2. `vm_pt_initialized` 且非 test 时 `vm_pt.assume_init_mut().destroy()`
3. `VM_INSTANCE` 时 `global::dec_vm_instance()`
4. 重置全部簿记字段（flags/endpoint/boot/ACL/守卫/region_top/total/faults/bytecopies）

**Drop**（`vmproc.rs:217-240`）：`debug_assert!` 非 INUSE（指示进程表管理 bug）+ release 防御性 `clear()`。生产路径正常回收走 typestate 迁移（`reap`/`force_clear`），Drop 是兜底。

### 4.2 `os/servers/vm/src/vmproc/flags.rs`：VmFlags

`bitflags!`（`flags.rs:5-32`）：三标志位值与 C 宏一致（`IN_USE=0x001`、`EXITING=0x002`、`VM_INSTANCE=0x010`）；`Default` 为空。测试覆盖基本操作/组合/helper/默认值（`flags.rs:41-72`）。

### 4.3 `os/servers/vm/src/vmproc/vmproc_handle.rs`：typestate 视图

**EmptySlot**（`vmproc_handle.rs:26-160`）：

- `new`（L33）：debug_assert 非 INUSE
- `slot()`（L39）：返回 `vm_slot`
- `activate`（L53）：严格配对 → 委托 `activate_relaxed`
- `activate_relaxed`（L106）：设 `IN_USE` + endpoint，带 debug 校验（§3.7）

**ActiveProc**（`vmproc_handle.rs:162-704`）：

- 状态迁移：`mark_exiting`（L171，设 EXITING → `ExitingProc`）、`force_clear`（L183，unsafe，调 `clear()` → `EmptySlot`）
- 身份/权限：`slot`/`endpoint`/`flags`/`is_vm_instance`/`acl`/`set_acl`/`acl_check`/`set_endpoint`
- 内存统计：`total`/`total_max`/`region_top`/`minor_fault`/`major_fault` + `add_total`（L283，累加+峰值更新，对应 `region.c:85-90`）/`sub_total`（L290，饱和减）/`set_total`/`inc_minor_fault`（L300）/`inc_major_fault`（L305）
- 地址空间：`init_page_table`（L342，创建页表 + 内核映射）/`init_regions`（L423）/`page_table`/`regions`/`region_count`/`add_region`/`remove_region`
- fork 辅助：`init_from_fork`（L318，只设 INUSE + endpoint + total/total_max/region_top，对应 `fork.c:83` 只继承 INUSE）/`copy_acl_from`（L331，调 `AclState::acl_fork`，对应 `acl.c:110-114`）
- 活更新：`mark_vm_instance`（L273，设 flag + `inc_vm_instance`，对应 `main.c:577-579`）、`swap_proc_slot`（L620，交换两槽内容但保留 endpoint/slot 身份）

**ExitingProc**（`vmproc_handle.rs:706-760`）：`new`（L707，debug_assert INUSE+EXITING）、`reap`（L744，unsafe，调 `clear()` → `EmptySlot`，对应 `do_exit` 的 `free_proc`+`clear_proc`）。

### 4.4 `os/servers/vm/src/vmproc/mod.rs`：可见性设计

`VmProc` 不导出；外部代码只能通过 typestate 视图操作。可见性矩阵：

| 元素 | 可见性 | 说明 |
|------|--------|------|
| `VmProc` 类型/字段 | `pub(crate)` + 不 re-export | 模块外无法写出类型名（编译器禁止） |
| `VmProcTable::get_slot_mut` | `pub(super)` | 仅 vmproc 模块树内部（`table.rs:123`） |
| `EmptySlot`/`ActiveProc`/`ExitingProc` | `pub(crate)` | vm crate 内部通过视图访问 |
| `VmFlags` | `pub(crate)` re-export | 视图方法返回值需要 |

`test_utils`（`mod.rs:73-126`）：`get_active_vmproc`（初始化页表+区域）与 `get_active_vmproc_no_pt`（仅区域）供测试使用；`extend_to_static_lifetime` 用 transmute 把借用提升到 `'static`——安全前提是全局表 `'static` + 单线程测试。

### 4.5 `os/servers/vm/src/vm_server.rs`：init_proc 族

- `init_vm_slot()`（`vm_server.rs:293-304`）：找 boot 表中 `proc_nr == VM_PROC_NR` 的条目并 `init_proc`——对应 `main.c:474`
- `init_boot_procs()`（`vm_server.rs:306-325`）：遍历 boot 表，跳过负 proc_nr/VM 自身/`endpoint.is_none()`，`assert(start_addr != 0)` 后 `init_proc`——对应 `main.c:498-520`（exec_bootproc/free_mem DEFERRED，01 文档 §3.5）
- `init_proc()`（`vm_server.rs:328-338`）：§3.8
- `mark_vm_instance()`（`vm_server.rs:353-360`）：`get_active(VM_PROC_NR)` + `mark_vm_instance()`——对应 `main.c:577-579`

---

## 5. 测试要点

### 5.1 flags.rs 测试（4 个）

- `test_vmflags_basic`（`flags.rs:45`）：IN_USE 含 IN_USE、不含 EXITING
- `test_vmflags_combination`（`flags.rs:52`）：`IN_USE|EXITING` 双位共存
- `test_vmflags_helpers`（`flags.rs:60`）：`IN_USE|VM_INSTANCE` 组合
- `test_vmflags_default`（`flags.rs:68`）：Default 为空

### 5.2 vmproc.rs 测试（9 个 + 1 cfg）

- `test_vmproc_empty`（`vmproc.rs:258`）：空槽 endpoint NONE、非 INUSE、非 EXITING
- `test_vmproc_flags`（`vmproc.rs:269`）：IN_USE/EXITING 设置与查询
- `test_vmproc_endpoint`（`vmproc.rs:281`）：endpoint 有效 + IN_USE
- `test_slot_endpoint_consistency`（`vmproc.rs:291`）：`UserSlot::matches` 语义
- `test_slot_endpoint_inconsistency`（`vmproc.rs:300`）：不匹配检测
- `test_vmproc_memory_limit`（`vmproc.rs:309`）：total ≤ total_max
- `test_vmproc_stats`（`vmproc.rs:318`）：fault 计数初始 0、递增
- `test_vmproc_byte_copies`（`vmproc.rs:332`，cfg vmstats）
- `test_vacant_with_slot_preserves_slot`（`vmproc.rs:340`）：`vacant_with_slot` 保留 slot（对应 `main.c:461`）
- `test_clear_decrements_vm_instance_count`（`vmproc.rs:354`）：`clear()` 递减 VM_INSTANCE 计数（对应 `exit.c:77-79`）

### 5.3 vmproc_handle.rs 测试（15 个）

覆盖：`test_empty_slot_activate`、`test_activate_relaxed_*`（none/matching/exec_tmp 三用例 + should_panic 反例）、`test_active_proc_readonly`、`test_active_proc_write`、`test_active_proc_memory_tracking`、`test_mark_exiting`、`test_exiting_proc_reap`、`test_force_clear`、`test_page_fault_counters`、`test_full_lifecycle`（槽位复用：activate→mark_exiting→reap→再 activate→force_clear）、`test_swap_proc_slot_preserves_identities`。

### 5.4 覆盖缺口与建议

已闭环：`test_clear_decrements_vm_instance_count` 与 `test_vacant_with_slot_preserves_slot`（`vmproc.rs:340/354`）。剩余建议：

| 缺口 | 建议 | 严重度 |
|------|------|--------|
| `init_proc` 的 panic 语义无测试 | 归 `vm_server.rs` 测试（get_empty expect 等价） | P2（backlog） |

---

## 6. 过渡

本文档建立了进程控制块的静态结构（字段）与动态行为（状态机）。下一步：

- `03-vmproc-table.md`——进程表（`vmproc[VMP_NR]`）、slot 分配查找、`vm_isokendpt`、`VMP_EXECTMP`：PCB 之上的"集合"层。
- `04-acl.md`——`vm_acl` 字段的完整语义（`acl_init`/`acl_set`/`acl_check`/`acl_fork`/`acl_clear`）。
- `13-region-mapping.md` / `07-pagetable-struct.md`——`vm_regions` 与 `vm_pt` 的内部结构。
- `18-vm-fork.md` / `22-vm-exit.md`——状态机在完整流程中的使用（`init_from_fork`、两步退出）。

阅读顺序建议：01 → 02 → 03 → 04 → 05 → 06 → 07 → 08 → 09 → 10 → 11 → 12 → 13 → 14 → 15。

## 7. 参见

- `minix3/minix/servers/vm/vmproc.h` — ground truth：结构 + VMF 宏
- `minix3/minix/servers/vm/main.c:262-285`、`458-462`、`498-520`、`577-579` — init_proc/槽清零/boot 循环/VM 实例标记
- `minix3/minix/servers/vm/exit.c:25-107` — reset_vm_rusage/free_proc/clear_proc/do_exit/do_willexit
- `minix3/minix/servers/vm/fork.c:41-92` — fork 字段继承（`*vmc = *vmp`、`&= VMF_INUSE`）
- `minix3/minix/servers/vm/region.c:85-90`、`385-410` — vm_total 增减 / vm_region_top hint
- `minix3/minix/servers/vm/pagefaults.c:136-138` — 缺页计数
- `minix3/minix/servers/vm/utility.c:455-457` — getrusage 读取统计
- `minix3/minix/servers/vm/acl.c:20-129` — vm_acl 语义（NO_ACL/USER_ACL/acl_fork）
- `os/servers/vm/src/vmproc/vmproc.rs`、`flags.rs`、`vmproc_handle.rs`、`mod.rs` — Rust 实现
- `os/servers/vm/src/vm_server.rs:293-360` — init_proc 族
- 素材：`notes/rewrite/fork-syscall-rewrite/02-stage-vm/draft/01-vmproc-struct.md`（旧编号素材）
