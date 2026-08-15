# 03-vmproc-table: 进程表——slot 分配、endpoint 验证与保留槽

> **分类**: 阶段 1 — 启动入口与进程模型（进程表锚点）
> **源码**: `minix3/minix/servers/vm/glo.h:17-20`（表定义）；`minix3/minix/servers/vm/utility.c:84-94`（`vm_isokendpt`）、`utility.c:186-219`（`swap_proc_slot`）；`minix3/minix/servers/vm/main.c:131/457-462`（主循环验证 + 表初始化）；`minix3/minix/include/minix/endpoint.h:45-69`（endpoint 编码）
> **Rust 模块**: `os/servers/vm/src/vmproc/table.rs`（进程表）+ `os/servers/vm/src/vmproc/vmproc_handle.rs:620`（`swap_proc_slot`，02 文档 §4.3）
> **前置**: `notes/rewrite/fork-syscall-rewrite/02-stage-vm/02-vmproc-struct.md`（PCB 结构与状态机）、`notes/rewrite/fork-syscall-rewrite/02-stage-vm/01-vm-init-main.md`（`init_vm` 调用点）、`notes/rewrite/fork-syscall-rewrite/02-stage-vm/00-vm-overview.md`
> **说明**: 进程表 `vmproc[VMP_NR]` 的集合级语义：表如何初始化、slot 如何分配/查找/遍历、`vm_isokendpt` 如何把 endpoint 翻译成槽号、`VMP_EXECTMP` 保留槽的诚实定位。**不覆盖**：`struct vmproc` 的字段细节与 typestate 状态机（02）、ACL（04）、页表（07/08）、fork 全流程（18）、RS Live Update 流程（25）。

---

## 1. 概念：进程表——VM 如何"记住"所有进程

### 1.0 章节引言

02 文档回答"VM 如何记住**一个**进程"（PCB 结构 + 状态机）。本文档回答"VM 如何记住**所有**进程"——一张**进程表**，以及围绕这张表的三个问题：

1. **怎么找**：一条消息带着 endpoint 进来，VM 如何可靠地把它翻译成进程槽？
2. **怎么分**：新进程（fork 子进程、boot 进程）的槽从哪来？
3. **怎么数**：哪些槽被占、哪些空闲、如何遍历全部活跃进程？

这三件事在 Minix3 里全部落在 `vmproc[VMP_NR]` 这张定长表上（`glo.h:20`）。表是"集合"层，PCB 是"个体"层：**个体层回答一个进程的内存长什么样，集合层回答这个进程是哪个、它在哪一格、格子的生死由谁管理**。

### 1.1 一张表装下所有进程：VMP_NR = NR_PROCS + 1

```c
#define VMP_EXECTMP	_NR_PROCS       /* glo.h:17 */
#define VMP_NR		_NR_PROCS+1     /* glo.h:18 */

EXTERN struct vmproc vmproc[VMP_NR];   /* glo.h:20 */
```

`_NR_PROCS = 256`（`minix3/minix/include/minix/sys_config.h:8`）。因此表有 **257 个槽**：256 个用户进程槽（0 ~ 255）+ 1 个 **exec 临时槽**（`VMP_EXECTMP`，索引 256）。

exec 临时槽的**设计意图**是 exec 重写路径的暂存区：exec 需要保留旧进程的部分状态、换入新映像，临时槽给这个"换血"过程一个不打扰其他进程的中间落点。但**必须如实声明**：在当前 Minix3 源码中 `VMP_EXECTMP` 只有 `glo.h:17` 一处定义，**任何 `.c` 文件都没有使用它**（`rg VMP_EXECTMP minix3/minix/servers/vm/` 仅 1 处命中）。它是一个**预留槽**——表里有它的位置，endpoint 查找却永远够不到它（§2.3 的上界论证）。本文档按这个事实描述，不编造用途。

### 1.2 入口翻译：endpoint → slot 是 VM 的"门卫"

VM 主循环每收到一条消息，第一件事就是把消息里的 endpoint 翻译成槽号（`main.c:131`）：

```c
if(vm_isokendpt(who_e, &caller_slot) != OK)
	panic("invalid caller %d", who_e);
```

`vm_isokendpt`（`utility.c:84-94`）做**三层递进验证**，每一层防一类错误：

| 层 | 检查 | 失败返回 | 防什么 |
|----|------|----------|--------|
| 1 | 槽号范围（`<0 \|\| >= NR_PROCS`） | `EINVAL` | 坏 endpoint 编码（越界索引 → 数组越界访问） |
| 2 | endpoint 与槽内记录的 `vm_endpoint` 匹配（§2.3 步骤 3） | `EDEADEPT` | **slot 重用混淆**：旧进程退出、槽被新进程占用后，旧 endpoint 的 generation 对不上 |
| 3 | 槽内进程活跃（`VMF_INUSE`） | `EDEADEPT` | 已退出/未激活进程的操作 |

这是典型的 **TOCTOU（Time-of-Check to Time-of-Use）防护**：PM 和 VM 是独立地址空间，父进程可能在 PM 发出 `VM_FORK` 的瞬间崩溃，其槽可能已被重用。没有第 2 层检查，VM 会拿着旧 endpoint 误操作新进程的内存；没有第 3 层检查，VM 会操作一个"名存实亡"的空槽。

### 1.3 三组操作：查 / 取 / 数

进程表的管理语义可以归纳为三组正交操作：

- **查（验证）**：`vm_isokendpt`——endpoint → 槽号，IPC 入口统一走它（§2.5 列出全部 34 个使用点）
- **取（访问）**：`get_empty(slot)` / `get_active(slot)` / `get_exiting(slot)` / `alloc_empty_slot()`——按状态取视图（02 文档 D5 的 typestate 视图）；`iter()`——遍历全部活跃槽
- **数（统计）**：`is_slot_in_use` / `used_count` / `free_count` / `is_empty` / `is_full`——集合级查询，供调试与 sanity check

C 侧没有独立的"计数 API"（手写循环 + `VMF_INUSE` 检查），Rust 把它们提炼为方法（§3.4）。这是**表达层重构**，不是语义新增。

### 1.4 与 02 的分工边界

| 层 | 文档 | 回答的问题 |
|----|------|-----------|
| 个体（PCB） | 02-vmproc-struct | 一个进程的内存状态长什么样？状态怎么流转？ |
| 集合（表） | 本文档 | 有多少个进程？各自在哪格？endpoint 怎么翻译？谁负责分配槽？ |
| 权限（ACL） | 04-acl | 这个进程被允许调用哪些 VM 服务？（入口验证之后） |

**一次完整消息处理的时序**：主循环收到消息 → `vm_isokendpt` 验证 caller（§1.2）→ 按消息类型分发 → 服务 handler 内再次 `vm_isokendpt` 验证目标进程（fork.c:41、exit.c:67 等）→ 用返回的槽号取 typestate 视图操作。验证发生在**每条服务路径的第一行**，这是 VM 安全模型的地基。

```
消息进入 ──► vm_isokendpt(caller) ──► 主循环分发（15）
                                    ──► 服务 handler：vm_isokendpt(目标) ──► typestate 视图 ──► 操作
```

### 1.5 本章小结

进程表是 VM 的"集合层"：**一张 `NR_PROCS+1` 槽的表**（含一个 endpoint 查找不可达的 exec 保留槽）、**一个三层递进的入口验证**（范围/身份/活跃，防越界与 TOCTOU）、**三组正交操作**（查/取/数）。后续章节依次回答：C 如何定义与验证（§2）→ Rust 如何用类型重表达（§3）→ 代码如何落地（§4）→ 测试如何证明（§5）。

---

## 2. C 源码分析

### 2.1 表定义与常量（glo.h:17-20）

```c
#define VMP_EXECTMP	_NR_PROCS
#define VMP_NR		_NR_PROCS+1

EXTERN struct vmproc vmproc[VMP_NR];
```

| 符号 | 值 | 语义 |
|------|----|------|
| `VMP_EXECTMP` | 256（= `_NR_PROCS`） | exec 临时槽索引；**当前零使用**（仅本定义） |
| `VMP_NR` | 257（= `_NR_PROCS+1`） | 表长度：全部用户进程 + exec 临时槽 |
| `vmproc[VMP_NR]` | 全局数组 | 进程表本体；`EXTERN` 宏在 `_MAIN` 时展开为定义（`glo.h:9-14`） |

`VMP_NR` 的 **+1** 值得注意：它让表**物理上**容纳 exec 临时槽，但 `vm_isokendpt` 的合法上界是 `NR_PROCS`（§2.3）——所以 exec 临时槽是"**表里有、查找无**"的保留位。这种"容量比可寻址范围大 1"的布局是 Minix3 对 exec 重写预留的空间，Rust 侧用 `VM_PROC_COUNT`/`VM_EXEC_TMP_SLOT` 原样保留（§3.2）。

### 2.2 一次性初始化：memset + vm_slot（main.c:457-462）

```c
/* Set table to 0. This invalidates all slots (clear VMF_INUSE). */
memset(vmproc, 0, sizeof(vmproc));

for(i = 0; i < ELEMENTS(vmproc); i++) {
	vmproc[i].vm_slot = i;
}
```

这是表的**唯一一次性初始化**，发生在 `init_vm()` 内、`acl_init()` 之前（`main.c:457` 前文是 `get_mem_chunks`）：

1. `memset(vmproc, 0, sizeof(vmproc))`——全表清零。`vm_flags = 0` 使**所有槽位空闲**（`VMF_INUSE` 未设），同时清掉 `vm_endpoint`/`vm_acl`/统计等所有旧值。
2. `for(...) vmproc[i].vm_slot = i`——逐槽写入索引身份。**注意顺序依赖**：必须先清零再写 `vm_slot`，否则 `vm_slot` 也会被 `memset` 抹掉。

此后表的生命周期不再有"整体重建"：槽位的激活/回收都在这张已初始化的表上原地进行（02 文档 §2.4/§2.5）。Rust 用编译期构造替代了这个运行时循环（§3.3），`vm_slot` 的写入推迟到槽激活时。

### 2.3 vm_isokendpt：三层验证（utility.c:84-94）

```c
int vm_isokendpt(endpoint_t endpoint, int *procn)
{
        *procn = _ENDPOINT_P(endpoint);
        if(*procn < 0 || *procn >= NR_PROCS)
		return EINVAL;
        if(*procn >= 0 && endpoint != vmproc[*procn].vm_endpoint)
                return EDEADEPT;
        if(*procn >= 0 && !(vmproc[*procn].vm_flags & VMF_INUSE))
                return EDEADEPT;
        return OK;
}
```

逐步拆解：

1. **提取槽号**：`_ENDPOINT_P(endpoint)`（`endpoint.h:68-69`）从 endpoint 的低位提取进程槽号（§2.4）。
2. **范围检查**：`*procn < 0 || *procn >= NR_PROCS` → `EINVAL`。两个边界都防数组越界：负槽号（内核任务端，如 `SYSTEM=-2`）与超过用户进程数的槽号。**关键细节：上界是 `NR_PROCS` 而非 `VMP_NR`**——槽 256（`VMP_EXECTMP`）虽然存在于表中，但任何编码了它的 endpoint 都会在这里被 `EINVAL` 拒绝。exec 临时槽**不可经 endpoint 寻址**，只能通过直接 slot 引用操作。
3. **身份检查**：`endpoint != vmproc[procn].vm_endpoint` → `EDEADEPT`。endpoint 的 generation 字段保证"槽被复用后旧身份失效"（§2.4）。
4. **活跃检查**：`!(vm_flags & VMF_INUSE)` → `EDEADEPT`。空槽或已退出进程被拒绝。注意 C **不检查 `VMF_EXITING`**——退出中的进程仍是合法 endpoint（`do_exit` 自己检查 `VMF_EXITING`，见 02 文档 §2.6）。
5. **成功**：返回 `OK`，`*procn` 即为槽号。

**错误码语义**：`EINVAL` = "这个 endpoint 的编码本身就是坏的"（槽号越界）；`EDEADEPT` = "编码结构合法，但指向的进程不存在/已死"（身份过期或槽空闲）。Rust 的 `EndpointError` 枚举保留了这个区分（§3.3）。

### 2.4 endpoint 编码：generation + slot（endpoint.h:45-69）

```c
#define _ENDPOINT_GENERATION_SHIFT	15                    /* endpoint.h:45 */
#define _ENDPOINT(g, p) \
	((endpoint_t)(((g) << _ENDPOINT_GENERATION_SHIFT) + (p)))   /* :65-66 */
#define _ENDPOINT_P(e) \
	((((e)+MAX_NR_TASKS) & (_ENDPOINT_GENERATION_SIZE - 1)) - MAX_NR_TASKS)  /* :68-69 */
```

- endpoint 是 `int`，高 17 位是 **generation**（代数），低 15 位编码 **slot**（含 `MAX_NR_TASKS` 偏置，容纳负的核内任务槽）。
- `_ENDPOINT(g, p)` 构造；`_ENDPOINT_P(e)` 提取 slot；`_ENDPOINT_G(e)` 提取 generation（`endpoint.h:67`）。
- 特殊端：`ANY`/`NONE`/`SELF` 占用 `_ENDPOINT_SLOT_TOP` 附近的三个值（`endpoint.h:54-56`）。它们的 `_ENDPOINT_P` 提取值远大于 `NR_PROCS`，因此经 `vm_isokendpt` 一律 `EINVAL`——特殊端不是合法进程身份。
- 槽号范围 `[-MAX_NR_TASKS, MAX_NR_PROCS>`（`endpoint.h:16`），`MAX_NR_TASKS = 1023`（`minix3/minix/include/minix/com.h:55`）。

**generation 是 TOCTOU 防护的机制核心**：进程退出后槽被复用，新进程拿到的新 endpoint 的 generation 不同（内核 `sys_fork` 创建子进程时递增：`minix3/minix/kernel/system/do_fork.c:69-72` 的 `if(++gen >= _ENDPOINT_MAX_GENERATION) gen = 1;` + `_ENDPOINT(gen, p_nr)`），旧 endpoint 在第 3 步身份检查处失败。Rust 的 `Endpoint::from_generation_slot`/`slot()` 与 C 逐位对应（`os/libs/minix-types/src/types/endpoint.rs:82/88`）。

### 2.5 调用点全景：每条服务路径的入口

`rg vm_isokendpt minix3/minix/servers/vm/*.c` → 36 处匹配（1 处定义 `utility.c:84` + 34 处使用 + 1 处函数头注释 `utility.c:82`），按语义分组：

| 调用点 | 验证对象 | 归属文档 |
|--------|---------|---------|
| `main.c:131` | 消息源 `m_source`（主循环，失败 panic） | 15-ipc-dispatch |
| `main.c:699` | LU restart 的旧进程 endpoint | 25-rs-services |
| `main.c:760` | RS 握手 rprocpub 的 endpoint | 25-rs-services |
| `fork.c:41` | `VM_FORK` 的父进程 endpoint | 18-vm-fork |
| `exit.c:67/105/122` | `VM_EXIT` / `VM_WILLEXIT` / `VM_PROCCTL` 目标 | 22-vm-exit |
| `break.c:51` | `VM_BRK` 进程 | 19-vm-brk |
| `mmap.c:148/215/219/328/389/391/449/474/529` | mmap/remap 各消息的目标与源 | 20/21 |
| `pagefaults.c:85/166/314` | 缺页进程 | 16-pagefault |
| `vfs.c:124` | VFS 异步对话目标 | 23-vfs-interaction |
| `mem_cache.c:113/215` | cache 消息源 | 24-page-cache |
| `mem_shared.c:71` | 共享内存的注册进程 | 13-region-mapping |
| `rs.c:42/92/97/163/168/359` | RS 各服务的目标/源 | 25-rs-services |
| `utility.c:109/133/146/441` | `do_info` 的源与查询目标 | 26-vm-queries |

**模式观察**：VM 的几乎所有 IPC 服务 handler 第一行都是 `vm_isokendpt(...) != OK` 然后 `return EINVAL`（或 `panic`）。验证逻辑因此必须集中、无状态、O(1)——它被调用的频率等于消息频率。

### 2.6 swap_proc_slot：表级交换（utility.c:186-219）

```c
int swap_proc_slot(struct vmproc *src_vmp, struct vmproc *dst_vmp)
{
	struct vmproc orig_src_vmproc, orig_dst_vmproc;

	orig_src_vmproc = *src_vmp;
	orig_dst_vmproc = *dst_vmp;

	/* Swap slots. */
	*src_vmp = orig_dst_vmproc;
	*dst_vmp = orig_src_vmproc;

	/* Preserve endpoints and slot numbers. */
	src_vmp->vm_endpoint = orig_src_vmproc.vm_endpoint;
	src_vmp->vm_slot = orig_src_vmproc.vm_slot;
	dst_vmp->vm_endpoint = orig_dst_vmproc.vm_endpoint;
	dst_vmp->vm_slot = orig_dst_vmproc.vm_slot;

	return OK;
}
```

语义：**交换两个槽的整块 `struct vmproc` 内容，但保留各自的 `vm_endpoint` 与 `vm_slot` 身份**。用途是 Live Update：旧服务与新服务互换内存状态（页表、区域、统计），但旧服务继续以原 endpoint 服务客户端。调用点：`main.c:707`（`sef_cb_init_lu_restart`）与 `rs.c:190`（`RS_UPDATE` 流程）。

本文档只做表级定位：**`swap_proc_slot` 的完整 LU 流程归 `25-rs-services.md`**。Rust 的实现落在 typestate 层——`ActiveProc::swap_proc_slot`（`vmproc_handle.rs:620-695`，02 文档 §4.3），利用两个 `&mut ActiveProc` 视图 + `ptr::swap` + 身份快照恢复，语义与 C 一致（交换后两侧 endpoint/slot 不变）。

### 2.7 fork 子槽边界（fork.c:46-48）

```c
childproc = msg->VMF_SLOTNO;
if(childproc < 0 || childproc >= NR_PROCS) {
	printf("VM: bogus slotno VM_FORK %d\n", msg->VMF_SLOTNO);
	SANITYCHECK(SCL_FUNCTIONS);
	return EINVAL;
}
```

fork 的子进程槽由 **PM 指定**（消息带 `VMF_SLOTNO`），VM 侧校验其边界。**注意上界同样是 `NR_PROCS`**——exec 临时槽（256）不能作为 fork 子槽，与 `vm_isokendpt` 的上界一致。这解释了 §3.3 的 Rust 对齐决策：表容量（`VM_PROC_COUNT`）与可寻址上界（`NR_PROCS`）是两个不同常量，后者同时约束 endpoint 查找和 fork 子槽。

---

## 3. Rust 设计决策

### 3.1 D1: 表结构——`VmProcTable` 静态全局 + slot 级 `AssumeSyncCell`

- **C**: `EXTERN struct vmproc vmproc[VMP_NR]`（`glo.h:20`）
- **Rust**: `pub(crate) struct VmProcTable { slots: [AssumeSyncCell<VmProc>; VM_PROC_COUNT] }`（`table.rs:52-54`）+ `static VM_PROC_TABLE`（`table.rs:59-62`），`get_global()` 返回 `&'static VmProcTable`（`table.rs:65`）
- **理由**: 静态数组保持地址稳定、零开销，与 C 的 BSS 段分配一致；**slot 级 `AssumeSyncCell`**（而非整表 `UnsafeCell`）允许同时持有两个不同槽的可变视图——fork 同时操作父子槽、LU 同时操作新旧服务槽都需要这一点。`static mut` 因 Rust 2024 废弃不可用
- **行为契约**: 全部 257 个槽编译期构造为 `VmProc::vacant()`（等价 C 的 `memset` 清零）；访问一律经 `get_global()`

### 3.2 D2: 常量对齐——`VM_PROC_COUNT` / `VM_EXEC_TMP_SLOT`

- **C**: `VMP_NR = _NR_PROCS+1`（`glo.h:18`）、`VMP_EXECTMP = _NR_PROCS`（`glo.h:17`）
- **Rust**: `VM_PROC_COUNT: usize = NR_PROCS + 1`（`table.rs:41`）、`VM_EXEC_TMP_SLOT: UserSlot = UserSlot(NR_PROCS)`（`table.rs:44`）
- **理由**: 常量名保留语义对应，`UserSlot` newtype 杜绝"int 当槽号"类错误
- **行为契约**: `VM_EXEC_TMP_SLOT` 是表内最后一个槽（索引 256），**不参与 endpoint 查找**（D3 的上界）；它仅通过直接 slot 引用（`get_empty`）可达，供 exec 重写路径（当前无调用者，与 C 侧零使用一致）

### 3.3 D3: endpoint 验证——`EndpointError` 枚举 + **上界 = NR_PROCS**

- **C**: `vm_isokendpt` 返回 `int` 错误码（`EINVAL`/`EDEADEPT`/`OK`，`utility.c:84-94`）
- **Rust**: `pub(crate) enum EndpointError { InvalidSlot, DeadEndpoint }`（`table.rs:35-38`）+ `vm_isokendpt(&self, endpoint) -> Result<UserSlot, EndpointError>`（`table.rs:276-298`）
- **错误码映射**: `InvalidSlot` ↔ `EINVAL`（槽号越界）、`DeadEndpoint` ↔ `EDEADEPT`（身份不匹配或槽空闲）
- **关键对齐（2026-08-15 修复）**: **范围上界用 `NR_PROCS` 而非 `VM_PROC_COUNT`**。C 的检查是 `*procn >= NR_PROCS → EINVAL`（`utility.c:87`），即槽 256（exec 临时槽）编码的 endpoint 被 `EINVAL` 拒绝。修复前 Rust 用 `VM_PROC_COUNT` 作上界，槽 256 落入"范围合法但槽永不活跃"，被误报为 `DeadEndpoint`——错误码与 C 不一致。修复后：`slot < 0 || slot >= NR_PROCS → InvalidSlot`，与 C 逐位一致
- **行为契约**: 负槽（内核任务端）→ `InvalidSlot`；`slot >= NR_PROCS`（含 exec 临时槽与特殊端）→ `InvalidSlot`；endpoint 不匹配 → `DeadEndpoint`；非 `IN_USE` → `DeadEndpoint`；否则 `Ok(UserSlot)`

### 3.4 D4: typestate 入口 + 延迟 vm_slot 赋值

- **C**: `&vmproc[slot]` 裸指针访问；`vmproc[i].vm_slot = i` 一次性循环（`main.c:461`）
- **Rust**: `get_empty(slot)`（`table.rs:141`）/ `alloc_empty_slot()`（`table.rs:160`）/ `get_active(slot)`（`table.rs:176`）/ `get_exiting(slot)`（`table.rs:192`）返回 typestate 视图；**`get_empty`/`alloc_empty_slot` 在返回前写 `proc.vm_slot = slot`**（`table.rs:150/167`）
- **理由**: C 的"一次性循环写索引"在 Rust 中无法用数组重复初始化器表达（`[const { ... }; N]` 只能重复同一个值，const 上下文不能逐槽赋值），因此把 `vm_slot` 的写入**推迟到槽激活时**。这是行为不可区分的重构：所有读取 `vm_slot` 的路径（`for_each_active_region`、`iter`、`find_region_snapshot`）都以 `IN_USE` 为前置条件，未激活槽的 `vm_slot` 残留值（`vacant()` 构造的 `UserSlot(0)`）永远不会被读到
- **行为契约**: `get_empty(slot)` 后 `proc.vm_slot == slot`；`alloc_empty_slot` 返回**首个空闲槽**（线性扫描 `0..VM_PROC_COUNT`）并写 `vm_slot`

### 3.5 D5: 查询与遍历 API

- **C**: 无独立 API；手写循环 + `VMF_INUSE` 检查；`ALLREGIONS` 宏（`region.c:175-193`）遍历活跃进程的区域
- **Rust**: `is_slot_in_use`（`table.rs:209`）/ `find_free_slot`（`table.rs:222`）/ `used_count`（`table.rs:235`）/ `free_count`（`table.rs:247`）/ `is_empty`（`table.rs:252`）/ `is_full`（`table.rs:257`）/ `iter`（`table.rs:358`）/ `for_each_active_region`（`table.rs:309`）
- **理由**: 表管理需要集合级查询；`for_each_active_region` 是对 `ALLREGIONS` 的安全封装（不暴露裸 `&VmProc`，保持 typestate 契约）
- **行为契约**: `find_free_slot` 是**查询不是分配**——它只返回索引、不持视图，返回后槽可能已被其他操作占用（`alloc_empty_slot` 才是原子分配入口）；`iter` 返回 `&VmProc` 绕过 typestate，故限制为 `pub(super)`（仅 vmproc 模块树内可用），且迭代期间借用整个表，禁止在迭代中调用任何视图操作（文档化限制，draft 素材 §5.3 有完整论证）

### 3.6 D6: swap_proc_slot 归属——typestate 层实现，表层不重复

- **C**: `swap_proc_slot` 是 utility.c 的表级函数（§2.6）
- **Rust**: 实现落在 `ActiveProc::swap_proc_slot`（`vmproc_handle.rs:620-695`，02 文档 §4.3）——交换需要两个 `&mut ActiveProc` 视图（表 API 提供 `get_active`），放 typestate 层比表层更自然；表层（table.rs）**不重复实现**
- **理由**: 避免同一语义两处实现（漂移风险）；`ptr::swap` + 身份快照恢复与 C 的"整结构交换 + 恢复 endpoint/slot"逐位对应
- **边界声明**: 完整 RS UPDATE / LU restart 流程（`main.c:707`、`rs.c:190` 的调用链）DEFERRED，归 `25-rs-services.md`

---

## 4. 实现详解

### 4.1 表结构与常量（table.rs:35-62）

```rust
pub(crate) enum EndpointError {
    InvalidSlot,      // ↔ Minix3 EINVAL（槽号越界）
    DeadEndpoint,     // ↔ Minix3 EDEADEPT（身份不匹配 / 槽空闲）
}

pub(crate) const VM_PROC_COUNT: usize = NR_PROCS + 1;        // = VMP_NR
pub(crate) const VM_EXEC_TMP_SLOT: UserSlot = UserSlot(NR_PROCS); // = VMP_EXECTMP

pub(crate) struct VmProcTable {
    slots: [AssumeSyncCell<VmProc>; VM_PROC_COUNT],
}

static VM_PROC_TABLE: VmProcTable = VmProcTable {
    slots: [const { AssumeSyncCell::new(VmProc::vacant()) }; VM_PROC_COUNT],
};
```

要点：

- `NR_PROCS` 来自 `minix-types`（与 C 的 `_NR_PROCS` 对齐）；`VM_PROC_COUNT = 257`
- `VmProc::vacant()`（`vmproc.rs:68-91`）是 const 构造——`vm_flags` 空、`vm_endpoint = NONE`、两个 `MaybeUninit` 守卫为 `false`、`vm_slot = UserSlot(0)`（§3.4 的延迟赋值前提）
- 静态表是**编译期全初始化**，无 `init_vm` 阶段的运行时 `memset`——C 的"清零 + 写 vm_slot"循环被"构造即空 + 激活时写 vm_slot"替代（02 文档 D4 的语义等价论证）

### 4.2 内部 helper：unsafe 面收敛（table.rs:88-139）

- `check_slot`（`table.rs:90`）：`UserSlot` 索引 < `VM_PROC_COUNT` 才返回 `Some(usize)`——**表容量上界**
- `get_slot`（`table.rs:104`）：只读 `&VmProc`；SAFETY 前提是"无同槽可变引用活跃"
- `get_slot_mut`（`table.rs:123`）：`#[allow(clippy::mut_from_ref)]`，`pub(super)` 限制在 vmproc 模块树内。安全论证三支柱：① VM 单线程事件循环，无跨 CPU 并发；② 每次只访问一个槽，不同槽独立（`AssumeSyncCell` slot 级粒度）；③ 返回的 typestate 视图持有独占 `&mut VmProc`，借用检查器阻止同槽二次访问

**关键点**：`get_slot_mut` 是表内唯一的"裸可变访问"通道，且只对 vmproc 模块树可见（`pub(super)`）。所有外部代码必须经 typestate 视图（`get_empty`/`get_active`/`get_exiting`）操作——这是 02 文档可见性设计的延续。

### 4.3 typestate 入口（table.rs:141-207）

| 方法 | 行 | 语义 | 对应 C |
|------|-----|------|--------|
| `get_empty(slot)` | `table.rs:141` | 槽空闲 → `EmptySlot` 视图；激活时写 `vm_slot` | `&vmproc[slot]` + `vm_slot=i` 惰性化 |
| `alloc_empty_slot()` | `table.rs:160` | 线性扫描 `0..VM_PROC_COUNT` 找首个空闲槽 → `EmptySlot` | fork 子槽分配的前身（PM 指定场景） |
| `get_active(slot)` | `table.rs:176` | `IN_USE && !EXITING` → `ActiveProc` | `&vmproc[slot]` + 活跃检查 |
| `get_exiting(slot)` | `table.rs:192` | `IN_USE && EXITING` → `ExitingProc` | `&vmproc[slot]` + `VMF_EXITING` 检查 |

视图的 debug_assert 前置条件（`EmptySlot::new` 要求非 `IN_USE` 等）在 02 文档 §3.5 定义。注意 `alloc_empty_slot` 的扫描上界是 `VM_PROC_COUNT`——它**可以**分配 exec 临时槽（256），因为"分配"是直接 slot 操作，不受 endpoint 上界约束；这与 C 的表布局一致（C 没有等价分配函数，fork 子槽由 PM 指定）。

### 4.4 查询 API 与 vm_isokendpt（table.rs:209-298）

- 计数与状态：`is_slot_in_use`/`used_count`/`free_count`/`is_empty`/`is_full`——O(N) 扫描（N=257，代价可忽略），`free_count = VM_PROC_COUNT - used_count`
- `find_free_slot`：查询语义（§3.5）
- `vm_isokendpt`（`table.rs:276-298`）核心：

```rust
pub(crate) fn vm_isokendpt(&self, endpoint: Endpoint) -> Result<UserSlot, EndpointError> {
    let vm_slot = endpoint.slot();                       // = _ENDPOINT_P(endpoint)
    if vm_slot < 0 || vm_slot as usize >= NR_PROCS {    // = utility.c:86-88
        return Err(EndpointError::InvalidSlot);
    }
    let slot_idx = UserSlot(vm_slot as usize);
    let proc = unsafe { &*self.slots[slot_idx.get()].get() };

    if proc.vm_endpoint != endpoint {                    // = utility.c:90
        return Err(EndpointError::DeadEndpoint);
    }
    if !proc.vm_flags.contains(VmFlags::IN_USE) {        // = utility.c:92
        return Err(EndpointError::DeadEndpoint);
    }
    Ok(slot_idx)
}
```

与 C 的三层验证逐行对应。**注意上界 `NR_PROCS` 与 `check_slot` 的 `VM_PROC_COUNT` 是两个不同常量**：前者是 endpoint 可寻址上界（§2.3/§2.7），后者是表容量（§2.1）——这是 2026-08-15 修复的语义对齐点（§3.3）。

### 4.5 遍历与区域快照（table.rs:303-431）

- `for_each_active_region`（`table.rs:309`）：对每个 `IN_USE && vm_regions_initialized` 的槽，遍历其 `VirRegion` 并调用闭包 `f(slot, endpoint, region)`——对应 `ALLREGIONS` 宏（`region.c:175-193`）；供 `verify_refcounts` 等 sanity check 使用
- `iter`（`table.rs:358`）+ `VmProcIter`（`table.rs:444-462`）：`pub(super)` 迭代器，只产出 `IN_USE` 槽的 `&VmProc`；绕过 typestate，限制在模块树内
- `find_region_snapshot`（`table.rs:378`）：读某进程在某地址处的区域关键字段（vaddr/length/id/remaps），返回 `Copy` 的 `RegionSnapshot` 而不持视图——`dispatch_remap` 用它读源进程区域后再独立修改目标进程
- `increment_region_remaps`（`table.rs:411`）：对指定进程/地址的区域 remaps 计数饱和加 1——跨进程操作的定向变更

### 4.6 消费方接线

| 消费方 | 位置 | 用法 |
|--------|------|------|
| 主循环 caller 验证 | `vm_server.rs:416-418` | `vm_isokendpt(who_e)` 失败 → `panic!("invalid caller")`——等价 `main.c:131` 的 panic 语义 |
| fork 父进程验证 | `fork.rs:194-196` | `vm_isokendpt(parent_endpoint)` → `VmForkError::InvalidEndpoint`——对应 `fork.c:41` |
| exit 族 | `exit.rs:48/70/141/193` | 各 handler 先验证目标 endpoint（`From<EndpointError>` 折叠两类错误） |
| munmap/mmap/query/map_phys | `munmap.rs:81`、`mmap.rs:198/283`、`query.rs:148/187/247/307/368`、`map_phys.rs:67` | 服务路径的入口验证 |
| 页错误 | `vm_server.rs:713`（`dispatch_pagefault`） | `vm_isokendpt(request.endpoint)` → `InvalidProcess` |

**错误折叠模式**：除主循环（panic）与 fork（区分 `InvalidEndpoint`）外，大多数消费方用 `From<EndpointError>` 把 `InvalidSlot`/`DeadEndpoint` 折叠成同一个错误（如 `MunmapError::ProcessNotFound`）——因为 C 侧这些 handler 对两类失败都返回 `EINVAL`（§2.5），外部行为一致；`EndpointError` 的区分保留给调试与语义精确性。

---

## 5. 测试要点

### 5.1 单元测试（截至 2026-08-15，`cargo test -p minix-vm --lib` 基线 346 passed / 3 failed pre-existing）

`table.rs` 测试 14 个（`rg "^\s*fn test_" os/servers/vm/src/vmproc/table.rs` → 14 个）：

| 测试 | 行 | 验证目标 |
|------|-----|---------|
| `test_table_empty` | `table.rs:472` | 空槽非 `IN_USE` |
| `test_typestate_activate` | `table.rs:480` | 激活设 IN_USE + endpoint |
| `test_typestate_lifecycle` | `table.rs:497` | Empty→Active→Exiting→Empty |
| `test_as_active_returns_none_for_empty` | `table.rs:515` | 空槽非 Active |
| `test_as_active_returns_none_for_exiting` | `table.rs:522` | 退出中非 Active |
| `test_as_empty_returns_none_for_active` | `table.rs:536` | 活跃槽非 Empty |
| `test_force_clear` | `table.rs:551` | 强制回收 |
| `test_alloc_empty_slot` | `table.rs:565` | 分配首个空闲槽 + 写 vm_slot |
| `test_vm_isokendpt_valid` | `table.rs:581` | 有效 endpoint → `Ok(slot)` |
| `test_vm_isokendpt_mismatch` | `table.rs:595` | 过期 endpoint → `DeadEndpoint` |
| `test_vm_isokendpt_out_of_range` | `table.rs:608` | exec 临时槽/越界/负 slot → `InvalidSlot`（2026-08-15 新增） |
| `test_reap_and_reactivate` | `table.rs:636` | 槽位复用 + generation 递增 |
| `test_table_iter` | `table.rs:656` | 遍历只含 IN_USE 槽 |
| `test_get_global` | `table.rs:696` | 全局单例 |

### 5.2 覆盖维度

| 维度 | 覆盖测试 | 说明 |
|------|---------|------|
| typestate 生命周期 | activate/lifecycle/reap_and_reactivate/force_clear | 全部迁移路径 |
| 视图互斥 | as_*_returns_none_* 三个 | 空/活跃/退出互斥 |
| endpoint 验证 | valid/mismatch/out_of_range | 三层验证全分支 |
| slot 分配 | alloc_empty_slot | 首个空闲槽 + vm_slot 写入 |
| 遍历 | table_iter | 只产出 IN_USE |

### 5.3 覆盖缺口与建议

| 缺口 | 建议 | 严重度 |
|------|------|--------|
| `alloc_empty_slot` 跳过 IN_USE 槽的定向测试（现仅验证"空表分配"） | 预占前几槽后断言返回下一空闲槽 | P2（backlog） |
| `find_free_slot` 全表扫描/`is_full` 拒绝路径 | 占满 257 槽成本高，建议用 mock 或小表 | P2（backlog） |
| `RegionSnapshot`/`for_each_active_region`/`increment_region_remaps` 无直接测试 | 跨 `13-region-mapping`/`21-vm-munmap` 消费方覆盖 | P2（backlog） |
| `swap_proc_slot` 表级测试 | 已由 `vmproc_handle.rs:984 test_swap_proc_slot_preserves_identities` 覆盖（02 文档 §5.3） | ✅ 闭环 |

---

## 6. 过渡

本文档建立了进程表的集合层语义：表布局（`VM_PROC_COUNT` = `NR_PROCS` + 1）、入口验证（`vm_isokendpt` 三层检查、上界 `NR_PROCS`）、slot 分配与遍历、exec 临时槽的保留定位。进程模型（02 + 03）至此完整：**个体层（PCB/状态机）+ 集合层（表/验证/分配）**。

下一步：

- `04-acl.md`——入口验证之后的下一个安全层：`vm_acl` 权限检查（`acl_init`/`acl_check`/`acl_fork`）。
- `15-ipc-dispatch.md`——主循环如何使用 `caller_slot`（`vm_server.rs:416-418` 的完整分发骨架）。
- `18-vm-fork.md`——`vm_isokendpt` + child slot 边界（§2.7）在 fork 全流程中的使用。
- `22-vm-exit.md`——两步退出协议如何回收槽位（`do_exit` → `clear_proc` → 槽空闲）。
- `25-rs-services.md`——`swap_proc_slot`（§2.6）的 RS UPDATE / LU 完整流程。
- `13-region-mapping.md` / `14-region-lookup.md`——`for_each_active_region`/`find_region_snapshot` 依赖的区域数据结构。

阅读顺序建议：01 → 02 → 03 → 04 → 05 → 06 → 07 → 08 → 09 → 10 → 11 → 12 → 13 → 14 → 15。

## 7. 参见

- `minix3/minix/servers/vm/glo.h:17-20` — 表定义与常量（ground truth）
- `minix3/minix/servers/vm/utility.c:84-94` — `vm_isokendpt` 三层验证
- `minix3/minix/servers/vm/utility.c:186-219` — `swap_proc_slot`
- `minix3/minix/servers/vm/main.c:131`、`457-462`、`699`、`760` — 主循环验证/表初始化/LU restart 调用点
- `minix3/minix/servers/vm/fork.c:41-48` — fork 父验证 + child slot 边界
- `minix3/minix/include/minix/endpoint.h:45-69` — endpoint 编码
- `minix3/minix/include/minix/sys_config.h:8` — `_NR_PROCS`
- `os/servers/vm/src/vmproc/table.rs` — Rust 进程表实现（14 测试）
- `os/servers/vm/src/vmproc/vmproc_handle.rs:620-695` — `ActiveProc::swap_proc_slot`（02 文档 §4.3）
- `os/servers/vm/src/vm_server.rs:416-418` — 主循环 caller 验证
- 素材：`notes/rewrite/fork-syscall-rewrite/02-stage-vm/draft/02-vmproc-table.md`（旧 fork 主线素材：地址稳定性论证/选型对比/测试维度）
