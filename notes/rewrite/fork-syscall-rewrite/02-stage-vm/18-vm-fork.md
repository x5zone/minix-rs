# 18-vm-fork: VM_FORK——地址空间共享与进程注册的编排

> **分类**: 阶段 7 — IPC 服务（fork 次主线核心）
> **源码**: `minix3/minix/servers/vm/fork.c`（116 行：`do_fork` :32-115）+ `minix3/minix/servers/vm/region.c`（`map_proc_copy` :933-939 / `map_proc_copy_range` :944-999）+ `minix3/minix/servers/vm/acl.c`（`acl_fork` :110-116）+ `minix3/minix/servers/vm/utility.c`（`vm_isokendpt` :84-101）+ `minix3/minix/kernel/system/do_fork.c`（内核侧 `sys_fork` 处理）+ `minix3/minix/include/minix/com.h`（`VMF_ENDPOINT`/`VMF_SLOTNO`/`VMF_CHILD_ENDPOINT` :633-635 / `PFF_VMINHIBIT` :360）
> **Rust 模块**: `os/servers/vm/src/fork.rs`（`do_fork` :183-390 / `fork_regions` :140-158 / `free_forked_regions` :160-175 / `handle_memory_once` :33-85 / `sys_fork` stub :406-408）+ `os/servers/vm/src/vmproc/vmproc_handle.rs`（`init_from_fork` :327-333 / `copy_acl_from` :340-343 / `init_page_table` :351-403 / `free_page_table` :411-419 / `init_regions` :423-427）+ `os/servers/vm/src/vmproc/table.rs`（`VM_PROC_COUNT` :41 / `VM_EXEC_TMP_SLOT` :44 / `get_empty` :141-150）+ `os/servers/vm/src/ipc/dispatcher.rs`（`dispatch_fork` :108-121 / 主循环 VM_FORK 分支 :1036-1037）+ `os/libs/minix-types/src/ipc/vm.rs`（`VmForkIn` :167-172 / `VmForkOut` :175-177 / `decode` :678-687 / `encode` :689-695）
> **前置**: `notes/rewrite/fork-syscall-rewrite/02-stage-vm/17-cow-mechanism.md`（CoW 机制：`fork_region` 共享建立 / `setup_cow_for_all_regions`+`write_page_table_mappings` 写保护）+ `04-acl.md`（ACL 继承）+ `03-vmproc-table.md`（slot 表）+ `15-ipc-dispatch.md`（主循环分发）
> **说明**: 本文档管 **fork 的编排**——VM 如何把"复制地址空间"组织成一次可回滚的服务调用：验证 → 初始化子进程 → 新建页表 → 区域复制（CoW）→ 页表写入 → 内核注册 → 返回 endpoint。**不覆盖**：CoW 机制本身（17）、`fork_region` 内部 refcount 语义（17 §3.1）、exit 逆向清理（22）、VFS fdref 细节（23）。

---

## 1. 概念：fork 是地址空间共享与进程注册的协作

### 1.0 章节引言

17 讲了 CoW 机制（共享 → 保护 → 分裂）的**原语**；本文档回答三个编排问题：

1. **谁来做**——fork 在 PM / VM / 内核三层的职责如何划分（§1.1）。
2. **怎么组织**——VM 把地址空间复制组织成哪几个阶段，每阶段失败怎么回滚（§1.2、§1.4）。
3. **同步契约**——VM 与内核如何协调"子进程已创建但还不能运行"（§1.3）。

它在整个 02-stage-vm 中的位置：

```
03（slot 表）→ 04（ACL）→ 15（分发）→ 17（CoW 原语）
→ ★18（fork 编排：VM_FORK 服务）
→ 22（exit 逆向）→ 23（VFS 交互）→ 26（查询）
```

### 1.1 三层职责划分

| 层 | 职责 | 关键动作 |
|----|------|---------|
| PM | 分配子进程 slot、协调系统调用、失败回滚 PM 侧结构 | `get_free_proc_slot()` → `vm_fork(ep, slot, &child_ep)` |
| **VM** | **复制地址空间（CoW）、初始化子进程 VM 结构、注册内核、返回 endpoint** | 本文档主体 |
| 内核 | 创建子进程调度实体、生成 endpoint、初始化时抑制调度 | `do_fork()`（kernel/system/do_fork.c） |

**VM 是 fork 的"内存编排者"**：它不创建调度实体（内核做），但必须在子进程可运行前把地址空间准备好——这决定了 `PFF_VMINHIBIT` 同步契约（§1.3）。

### 1.2 次主线路径图

```
PM                        VM                          Kernel
 │  VM_FORK (ep, slot)     │                             │
 │───────────────────────► │                             │
 │                         │ 1. 验证父进程 + 子槽位       │
 │                         │ 2. 初始化子进程结构          │
 │                         │ 3. pt_new（新建页表）        │
 │                         │ 4. map_proc_copy（CoW 复制）│
 │                         │ 5. flags/ACL               │
 │                         │ 6. sys_fork ───────────────►│ 创建子进程 + 生成 endpoint
 │                         │    pt_bind                 │ RTS_VMINHIBIT（不调度）
 │                         │    handle_memory_once ×2   │
 │                         │ 7. VMF_CHILD_ENDPOINT       │
 │ ◄────────────────────── │                             │
```

> **关键点**：阶段 6 之后**不可回滚**——内核已注册子进程。因此 VM 把所有可能失败的步骤（1-5）放在 `sys_fork` 之前，并让 `pt_bind` 也提前（Rust 设计，§3.4），使绑定失败成为可恢复错误而非 panic。

### 1.3 同步契约：PFF_VMINHIBIT 与 fork 消息页

`sys_fork` 携带 `PFF_VMINHIBIT`（com.h:360："Don't schedule until release by VM"）。内核在 `do_fork` 中设置 `RTS_VMINHIBIT`（kernel/system/do_fork.c:115-116），子进程不被调度；同时把父进程的消息缓冲地址写入 `m_krn_lsys_sys_fork.msgaddr`（kernel/system/do_fork.c:112，`p_delivermsg_vir`）。

VM 拿到 `msgaddr` 后调用两次 `handle_memory_once`（fork.c:97-108）：把子进程和父进程的**消息页提前解析为可写**（若该页是 CoW 只读，内核写 fork 回复时会触发缺页——VM 单线程下会死锁）。这是"VM 准备内存 → 内核放行"握手的一部分：fork 回复消息实际是内核写的，所以消息页必须就绪。

### 1.4 回滚契约：两层回滚与不可回滚点

| 阶段 | 失败条件 | C 处理 | 回滚范围 |
|------|---------|--------|---------|
| 参数验证 | endpoint 无效 / slot 越界 | `EINVAL` 直接返回 | 无副作用 |
| 页表创建 | `pt_new` 失败 | `ENOMEM` 直接返回 | 无新资源 |
| 区域复制 | `map_copy_region` 失败 | `map_free_proc` + `pt_free` + `ENOMEM` | 已复制区域 + 页表 |
| 内核通知 | `sys_fork` 失败 | **panic**（不可恢复） | 内核可能已创建子进程 |
| 消息页 | `handle_memory_once` 失败 | **panic**（不可恢复） | — |

**核心不变量**：`sys_fork` 是回滚分界线——之前全部可回滚，之后任何失败都 panic。Rust 把 `pt_bind` 移到 `sys_fork` 之前（§3.4），使页表绑定从"不可回滚 panic"降级为"可回滚错误"——这是对 C 顺序的**设计改进**（外部行为不变：C 的 pt_bind 失败本来就 panic，Rust 提前后失败返回 ENOMEM，调用方收到错误而非系统崩溃）。

### 1.5 对照：Redox 与 Linux

- **Linux**：`copy_process()` → `dup_mm()` → `dup_mmap()`——**逐 VMA 复制**：匿名页标记 `VM_SHARED` 语义外的写保护（`vma_wants_writenotify`），`copy_page_range` 复制 PTE 使父子共享物理页（PTE 清写位），真正复制发生在缺页（`do_wp_page`）。与 Minix3 相同的是"PTE 级共享 + 延迟复制"；不同的是 Linux 缺页完全在内核态完成，而 Minix3 交给用户态 VM 服务器。
- **Redox**：内核态 `sys_fork` 通过 `AddressSpace` 复制完成，无用户态服务器参与；`mm` 子系统在内核。Minix3 的"内存管理在用户态服务器"是微内核架构差异。
- **对照要点**：三家都把"fork 不复制物理页"作为核心策略；差异在**处理者位置**（内核 vs 用户态 VM）与**编排粒度**（Linux 一次系统调用，Minix3 是 PM→VM→kernel 三次 IPC 的分布式编排）。

### 1.6 小结

fork = 地址空间共享（CoW 建立，17）+ 进程注册（内核）+ 失败回滚（两层）。理解七阶段与"sys_fork 回滚分界线"后，C 源码的 fork.c 编排与 Rust 的 do_fork 可以逐阶段对照阅读。

---

## 2. C 源码分析

### 2.1 七阶段总览（fork.c:32-115）

| 阶段 | C 代码 | 行号 | 失败处理 |
|------|--------|------|---------|
| 1. 验证父进程 | `vm_isokendpt(msg->VMF_ENDPOINT, &proc)` | :41-45 | EINVAL |
| 2. 验证子槽位 | `childproc >= NR_PROCS` 界检查 | :47-52 | EINVAL |
| 3. 初始化子进程 | `*vmc = *vmp` + 恢复字段 | :58-64 | — |
| 4. 创建页表 | `pt_new(&vmc->vm_pt)` | :70-72 | ENOMEM |
| 5. 复制地址空间 | `map_proc_copy(vmc, vmp)` | :76-80 | pt_free + ENOMEM |
| 6. 标志 + ACL | `vm_flags &= VMF_INUSE` / `acl_fork` | :83/:86 | — |
| 7. 内核注册 + 收尾 | `sys_fork` / `pt_bind` / `handle_memory_once` ×2 | :89-108 | panic（不可恢复） |
| 8. 返回 | `msg->VMF_CHILD_ENDPOINT = vmc->vm_endpoint` | :111 | — |

### 2.2 验证阶段（fork.c:41-52）

```c
if(vm_isokendpt(msg->VMF_ENDPOINT, &proc) != OK) {
	printf("VM: bogus endpoint VM_FORK %d\n", msg->VMF_ENDPOINT);
	return EINVAL;
}

childproc = msg->VMF_SLOTNO;
if(childproc < 0 || childproc >= NR_PROCS) {
	printf("VM: bogus slotno VM_FORK %d\n", msg->VMF_SLOTNO);
	return EINVAL;
}
```

**vm_isokendpt**（utility.c:84-101）三步验证：

1. slot 范围：`_ENDPOINT_P(endpoint)`，越界 → EINVAL（:87-88）；
2. endpoint 一致性：`endpoint != vmproc[proc].vm_endpoint` → EDEADEPT（:89-90，防止过期 endpoint）；
3. 状态：`!(vm_flags & VMF_INUSE)` → EDEADEPT（:91-92）。

**槽位界检查**（:47-52）是本文档的关键对照点：C 拒绝 `childproc >= NR_PROCS`（含 exec 临时槽 `VMP_EXECTMP = _NR_PROCS`，glo.h:17）。Rust 侧此检查曾缺失（**03-P1-1**，§3.5 修复）。

### 2.3 子进程结构初始化（fork.c:54-64）

```c
vmp = &vmproc[proc];		/* parent */
vmc = &vmproc[childproc];	/* child */
assert(vmc->vm_slot == childproc);

origpt = vmc->vm_pt;
*vmc = *vmp;			/* 整结构体浅拷贝 */
vmc->vm_slot = childproc;
region_init(&vmc->vm_regions_avl);
vmc->vm_endpoint = NONE;	/* 内核分配前不可用 */
vmc->vm_pt = origpt;
```

**语义**：子进程从父进程继承全部 vmproc 字段（endpoint/total/total_max/region_top/flags/ACL），再恢复 4 个"子进程特有"字段：slot、区域树（置空，稍后复制）、endpoint（NONE）、页表（保存原值，稍后 `pt_new` 新建）。

> **设计观察**：`region_init` 被调用两次（fork.c:60 与 map_proc_copy 内部 region.c:936）——幂等操作，防御性冗余。Rust 用 `init_regions` 一次完成（§3.2）。

### 2.4 页表与区域复制（fork.c:70-80 + region.c:933-998）

```c
if(pt_new(&vmc->vm_pt) != OK) return ENOMEM;

if(map_proc_copy(vmc, vmp) != OK) {
	printf("VM: fork: map_proc_copy failed\n");
	pt_free(&vmc->vm_pt);
	return ENOMEM;
}
```

**map_proc_copy**（region.c:933-939）：`region_init(dst)` + `map_proc_copy_range(dst, src, NULL, NULL)`（全区域范围）。

**map_proc_copy_range**（region.c:944-999）核心循环：

```c
while((vr = region_get_iter(&v_iter))) {            /* :965 */
	struct vir_region *newvr;
	if(!(newvr = map_copy_region(dst, vr))) {      /* :967 */
		map_free_proc(dst);                        /* :968 区域层回滚 */
		return ENOMEM;                                /* :969 */
	}
	region_insert(&dst->vm_regions_avl, newvr);    /* :971 */
	/* SANITYCHECKS 断言 :977-987：父子 phys_region 不同但 ph 相同 */
	if(vr == end_src_vr) break;                    /* :989-991 */
	region_incr_iter(&v_iter);
}
map_writept(src);                                   /* :995 父子页表重写 */
map_writept(dst);
```

- `map_copy_region`（region.c:802-849）的共享建立（`pb_reference` + refcount++）与 **C 缺陷 1**（`ev_reference` 返回值被忽略，region.c:841-842）已在 17 §2.1/§3.1 详述，此处不重复。
- 复制完成后 `map_writept` 父子**都**重写页表（:995-996）——共享页对双方都是只读（CoW 写保护，17 §2.2）。

### 2.5 标志与 ACL（fork.c:83-86）

```c
vmc->vm_flags &= VMF_INUSE;   /* 只继承 IN_USE，其余标志清空 */
acl_fork(vmc);                /* acl.c:110-116 */
```

**acl_fork**（acl.c:110-116）：

```c
void acl_fork(struct vmproc *vmp) {
	if (vmp->vm_acl != USER_ACL)
		vmp->vm_acl = NO_ACL;
}
```

继承规则：USER_ACL（用户进程共享 ACL）→ 继承；NO_ACL → 保持；系统进程 ACL（正索引）→ 降级为 NO_ACL（系统进程 ACL 是进程特定的，不继承，由 RS 重新设置——04 范围）。

### 2.6 内核注册与收尾（fork.c:89-108）

```c
if((r=sys_fork(vmp->vm_endpoint, childproc,
	&vmc->vm_endpoint, PFF_VMINHIBIT, &msgaddr)) != OK)
	panic("do_fork can't sys_fork: %d", r);

if((r=pt_bind(&vmc->vm_pt, vmc)) != OK)
	panic("fork can't pt_bind: %d", r);

vir = msgaddr;
if (handle_memory_once(vmc, vir, sizeof(message), 1) != OK)
	panic("do_fork: handle_memory for child failed\n");
vir = msgaddr;
if (handle_memory_once(vmp, vir, sizeof(message), 1) != OK)
	panic("do_fork: handle_memory for parent failed\n");
```

**sys_fork 契约**（kernel/system/do_fork.c）：内核复制父 proc 结构到子 slot、生成 endpoint、写回 `m_krn_lsys_sys_fork.endpt`（子 endpoint）+ `msgaddr`（父进程消息缓冲虚拟地址，:112）；若带 `PFF_VMINHIBIT` 则置 `RTS_VMINHIBIT`（:115-116）阻止子进程调度。

**handle_memory_once 的语义**（pagefaults.c:245-252，16 §2.9 详述）：同步遍历 `[msgaddr, msgaddr+sizeof(message))`，确保区域存在且可写（CoW 页提前解析）。C 注释明说这是**优化**（"making these messages writable is an optimisation and its return value needn't be checked"）——但 C 仍然 panic 于失败，说明语义上要求必须成功。

### 2.7 C 小结：符号全景

| 符号 | 位置 | 角色 |
|------|------|------|
| `do_fork` | fork.c:32-115 | 七阶段编排 |
| `vm_isokendpt` | utility.c:84-101 | 父进程验证（EDEADEPT 语义） |
| `*vmc = *vmp` | fork.c:58-64 | 子进程结构继承 + 4 字段恢复 |
| `pt_new` / `pt_free` / `pt_bind` | pagetable.c | 页表创建 / 释放 / 绑定 |
| `map_proc_copy` / `_range` | region.c:933-939 / :944-999 | 全区域 CoW 复制 + 页表重写 |
| `map_free_proc` | region.c:589 | 区域层回滚 |
| `acl_fork` | acl.c:110-116 | ACL 继承规则 |
| `sys_fork` | kernel/system/do_fork.c | 内核进程注册（endpoint + msgaddr + VMINHIBIT） |
| `handle_memory_once` | pagefaults.c:245-252 | 消息页预解析 |

---

## 3. Rust 设计决策

> 行号以 2026-08-16 实证为准。Rust 侧现状：**七阶段编排全部落地**（do_fork :183-390），含 03-P1-1 修复、ACL 继承；C `pt_bind` 终步无对应调用（内核侧登记并入 `sys_fork`，§3.1 差异 1）；`handle_memory_once` ×2 与真实 `sys_fork` 为 DEFERRED（诚实标注，§3.6）。

### 3.1 D1：do_fork 编排（fork.rs:183-390）

对应 C 七阶段，差异用 typestate 视图表达状态转换：

| C 阶段 | Rust | 行号 |
|--------|------|------|
| 验证父进程 | `vm_isokendpt` → `get_active` | :191-197 |
| 验证子槽位 | `assert_ne!(parent, child)` + **slot 上界检查** + `get_empty` | :198-210 |
| 初始化子进程 | `activate_relaxed(NONE)` → `init_from_fork` → `copy_acl_from` | :212-220 |
| 创建页表 | `init_page_table` + `init_regions` | :222-223 |
| 复制地址空间 | `fork_regions` + `regions_mut().insert` | :226-257 |
| CoW + PTE | `setup_cow_for_all_regions` + `write_page_table_mappings` | :280/:305 |
| 内核注册 | `sys_fork`（stub）+ `set_endpoint` | :321-322 |
| 消息页 | `handle_memory_once` ×2 | **DEFERRED**（:325-385 注释） |

**与 C 的编排差异（诚实标注）**：

1. C 的 `pt_bind` 终步（fork.c:94，sys_fork 之后）无对应调用——内核侧地址空间登记并入 `sys_fork`（08 §3.3 D7 裁决），fork 的最后一个可恢复点是 `write_page_table_mappings`（§3.4）。
2. `handle_memory_once` ×2 未实现（§3.6 #5 DEFERRED，安全论证见 §4.4）。
3. `sys_fork` 是 stub（返回确定性 endpoint，§3.5）。

### 3.2 D2：逐字段拷贝替代 *vmc = *vmp（vmproc_handle.rs:327-333）

C 用整结构体浅拷贝 + 4 字段恢复（fork.c:58-64）。Rust 的 `init_from_fork` 显式继承 4 个标量（endpoint / total / total_max / region_top）+ 置 IN_USE：

```rust
pub(crate) fn init_from_fork(&mut self, endpoint: Endpoint, total: VirBytes, total_max: VirBytes, region_top: VirBytes) {
    self.inner.vm_flags = VmFlags::IN_USE;
    self.inner.vm_endpoint = endpoint;
    self.inner.vm_total = total;
    self.inner.vm_total_max = total_max;
    self.inner.vm_region_top = region_top;
}
```

- 区域树与页表**不拷贝**——由 `init_regions`（:423-427，空 RegionMap）与 `init_page_table`（:351-403，新建 + kernel 映射）新建，避免"拷贝后恢复"的脆弱模式（C 的 origpt 保存/恢复在 Rust 中不存在）。
- `vm_flags = IN_USE` 等价于 C 的 `vm_flags &= VMF_INUSE`（:83）——子进程只保留 IN_USE，其余标志（如 VM_INSTANCE）不继承。

### 3.3 D3：ACL 继承（copy_acl_from :340-343 → AclState::acl_fork acl.rs:167-174）

| C ACL | C 结果 | Rust AclState | Rust 结果 |
|-------|--------|--------------|-----------|
| USER_ACL (0) | USER_ACL | Default | Default |
| NO_ACL (-1) | NO_ACL | Uninitialized | Uninitialized |
| 系统 ACL (正索引) | NO_ACL | System(_) | Uninitialized |

语义等价：只有用户 ACL 被继承，系统 ACL 不继承（由 RS 重新设置）。

### 3.4 D4：回滚两层（fork.rs）

**区域层**（fork_regions :140-158）：任一 `fork_region` 失败 → `free_forked_regions`（:160-175）递减全部已复制区域的 refcount + `ev_unreference`——对应 C `map_free_proc`（region.c:956）。

**进程层**（do_fork）：`fork_regions` 失败（:228-249）、`write_page_table_mappings` 失败（:306-311）都调用 `free_page_table`（vmproc_handle.rs:411-419，对应 C `pt_free`）后返回错误。

**bind 语义的归属**：C 中 `pt_bind` 在 `sys_fork` 之后（fork.c:94），失败即 panic；Rust 无独立 bind 步骤——`write_page_table_mappings` 失败是最后一个可恢复点（fork.rs:314-319 注释），此后 `sys_fork` 完成内核侧登记，不可恢复路径只剩 sys_fork 本身。C `pt_bind` 的内核通知语义由 SetAddrSpace 通道承接（08 §3.3 D7 裁决）。SAFETY 注释论证了各回滚点前置条件（无 CR3 引用、页表未发布、单线程）。

### 3.5 D5：sys_fork stub 与 03-P1-1 修复（fork.rs:406-408 / :204-207）

```rust
/// DEFERRED (2026-06-15): Once IpcTransport is implemented, this will call:
///   ipc_call_kernel(SYS_FORK, parent_endpoint, child_slot)
fn sys_fork(_parent_endpoint: Endpoint, child_slot: UserSlot) -> Endpoint {
    Endpoint::from_generation_slot(1, child_slot.get() as i32)
}
```

- **sys_fork stub**：返回确定性 endpoint（generation=1）。真实实现需内核 IPC（26 范围），届时还需返回 `msgaddr`（依赖 handle_memory_once 接线，§4.4）。
- **03-P1-1 修复**（本轮）：C 检查 `childproc >= NR_PROCS → EINVAL`（fork.c:47-52），Rust 曾缺失——`get_empty` 接受 0..`VM_PROC_COUNT`（= NR_PROCS+1，含 exec 临时槽 256，table.rs:41-44），子进程可被创建在 endpoint 不可寻址的槽。修复：

```rust
// C: fork.c:47-52 — `childproc >= NR_PROCS` → EINVAL. The exec-rewrite
// temp slot (`VM_EXEC_TMP_SLOT == NR_PROCS`) is not a valid fork target;
// only slots 0..NR_PROCS-1 are. `UserSlot` is `usize`, so C's negative
// check (`childproc < 0`) is structurally impossible.
if child_slot.get() >= NR_PROCS {
    return Err(VmForkError::InvalidSlot);
}
```

`UserSlot` 是无符号 usize——C 的 `childproc < 0` 分支在类型层面不存在（结构性消除）。

### 3.6 差异清单（C ↔ Rust，诚实标注）

| # | C 语义 | Rust 现状 | 状态 |
|---|--------|----------|------|
| 1 | `*vmc = *vmp` 整结构体拷贝 + 4 字段恢复（fork.c:58-64） | `init_from_fork` 逐字段拷贝（vmproc_handle.rs:327-333） | ✅ 等价 |
| 2 | 子槽位界检查（fork.c:47-52） | 曾缺失 → **本轮修复**（fork.rs:204-207 + 回归测试） | ✅ 修复（03-P1-1） |
| 3 | `pt_bind` 在 sys_fork 后（fork.c:94），失败 panic | 无独立 bind 调用——内核侧登记并入 `sys_fork`（fork.rs:321，SetAddrSpace 通道语义）；最后可恢复点为 `write_page_table_mappings` | ✅ 简化 |
| 4 | `map_free_proc`（region.c:956） | `free_forked_regions`（fork.rs:160-175） | ✅ 等价 |
| 5 | `handle_memory_once` ×2（fork.c:97-108） | **DEFERRED**（fork.rs:325-385 注释，依赖跨 slot 可变访问 + 真实 msgaddr） | ⚠️ 待接线 |
| 6 | `sys_fork` 真实内核调用（返回 endpoint + msgaddr） | stub（fork.rs:406-408，确定性 endpoint） | ⚠️ 待内核 IPC |
| 7 | `region_init` 双重调用（fork.c:60 + region.c:935） | `init_regions` 一次（vmproc_handle.rs:423-427） | ✅ 简化 |
| 8 | ACL 继承（acl.c:110-116） | `copy_acl_from` → `AclState::acl_fork`（acl.rs:167-174） | ✅ 等价 |
| 9 | VMF_CHILD_ENDPOINT 写回 m1_i3（fork.c:111） | `VmReply::Fork` → `EncodeToM1`（vm.rs:689-695） | ✅ 等价 |

---

## 4. 实现详解

### 4.1 消息路径（dispatcher.rs:1036-1037 → :108-121）

```
主循环（dispatcher.rs:1036-1037）
  VM_FORK as usize - vm_rq_base =>
    Self::dispatch_fork(table, page_alloc, frames, VmForkIn::decode(m1))
      → fork::do_fork(table, frames, page_alloc, parent_endpoint, child_slot)   :111
      → Ok(child_endpoint) → VmReply::Fork(VmForkOut { child_endpoint })        :115-116
      → Err(e) → VmReply::Error(e.into())                                       :117
```

- `VmForkIn`（vm.rs:167-172）：`parent_endpoint`（← m1_i1）+ `child_slot`（← m1_i2，UserSlot usize 化）。
- `VmForkOut`（vm.rs:175-177）：`child_endpoint`（→ m1_i3，encode :689-695）。
- 错误映射（dispatcher.rs:1206-1213）：`InvalidEndpoint`/`InvalidSlot` → `VmError::InvalidProcess`（EINVAL，对齐 C fork.c:44/:51）；`SlotInUse` → `SlotInUse`（EINVAL）；`CowAllocFailed`/`PageTableInitFailed`/`PageTableMapFailed` → `OutOfMemory`（ENOMEM，对齐 C fork.c:71/:79）。

### 4.2 do_fork 分阶段伪码（fork.rs:183-390）

```
do_fork(table, frames, pfn_alloc, parent_endpoint, child_slot)
  parent_slot = table.vm_isokendpt(parent_endpoint)      :191-193  → InvalidEndpoint
  parent      = table.get_active(parent_slot)            :195-197  → InvalidSlot
  assert_ne!(parent_slot, child_slot)                    :198      （防同 slot 双 &mut）
  if child_slot.get() >= NR_PROCS → InvalidSlot          :204-207  （03-P1-1）
  empty = table.get_empty(child_slot)                    :209-210  → SlotInUse
  child = empty.activate_relaxed(Endpoint::NONE)         :212      （EmptySlot → ActiveProc）
  child.init_from_fork(NONE, total, total_max, region_top) :214-218
  child.copy_acl_from(&parent)                           :220
  child.init_page_table()                                :222      → PageTableInitFailed
  child.init_regions()                                   :223
  dst_regions = fork_regions(&parent_regions, frames)    :226
      └─ Err → child.free_page_table() + return          :228-249
  for region in dst_regions: child.regions_mut().insert(region)  :253-257
  child.setup_cow_for_all_regions(frames)                :280
  child.write_page_table_mappings(frames)                :305
      └─ Err → child.free_page_table() + PageTableMapFailed  :306-311
  child_endpoint = sys_fork(parent.endpoint(), child.slot())  :321   （stub；内核侧登记在此闭合）
  child.set_endpoint(child_endpoint)                     :322
  // handle_memory_once ×2 — DEFERRED                    :325-385
  Ok(child_endpoint)                                     :390
```

### 4.3 回滚与 SAFETY 论证模式

每个回滚点都带完整 SAFETY 注释（fork.rs:228-249/:306-311），共同前置条件：

1. `child` 是 Active typestate（`activate_relaxed` 后），`free_page_table` 是合法状态转换；
2. 页表已初始化（`init_page_table` 成功），可安全释放；
3. **无 CR3 引用**（`sys_fork` 之前——页表未向内核发布，无 CPU 可能切换到该根）——free 不需要 TLB shootdown；
4. 失败发生在区域元数据写入页表之前（fork_regions 失败）或映射未提交完（write_page_table_mappings 失败）——无悬空 PTE；
5. VM 单线程事件循环——无并发访问。

> 设计要点：Rust 用 `Result` 传播错误 + 显式 `free_page_table`，对应 C 的线性错误路径 + `pt_free`/`map_free_proc`——不需要状态机，因为 C 本身也是线性路径（16 §2 已述）。

### 4.4 handle_memory_once DEFERRED 的安全论证

C 在 sys_fork 后调用 `handle_memory_once` ×2（fork.c:97-108）把消息页解析为可写。Rust 当前未实现，两个依赖：

- **依赖 1**：`VmProcTable` 不支持同消息内对父子两个 slot 的可变访问（`get_active` 返回不可变视图；NLL 跨结构体字段拆分借用不支持）——需要 split-borrow 重构或 tuple 返回；
- **依赖 2**：stub `sys_fork` 不返回 `msgaddr`（真实实现需内核 IPC）。

**为什么安全**（fork.rs:382-395 注释论证）：C 注释称该步骤是"优化"（`needn't be checked`）——若消息页仍是 CoW 只读，内核写 fork 回复时**子进程**触发缺页，缺页处理器正常解析 CoW（分配新页），代价是一次性缺页而非死锁。（原注释的 deadlock 担忧针对父进程，而父进程的消息页在 fork 前已可写——子进程是全新进程，可异步处理自身缺页。）代价是每 fork 一次额外缺页，语义等价。

### 4.5 03-P1-1 修复记录（本轮）

- **问题**：Rust `do_fork` 无 `child_slot` 上界检查；`get_empty` 接受 0..`VM_PROC_COUNT`（含 exec 临时槽 256）。C 拒绝 `>= NR_PROCS`（fork.c:47-52）。
- **修复**：fork.rs:203-209 `child_slot.get() >= NR_PROCS → VmForkError::InvalidSlot` → `VmError::InvalidProcess`（EINVAL）。
- **测试**：`test_vm_server_handle_fork_rejects_exec_tmp_slot`（vm_server.rs:1395）——boot proc（slot 9, PFS）作父进程 + child_slot=256 → `VmReply::Error(VmError::InvalidProcess)`。
- **验证**：`cargo test -p minix-vm --lib` → 361 passed / 1 failed（`test_map_lazy` pre-existing 13 范围）。

---

## 5. 测试要点

### 5.1 单元测试清单（grep 实证，2026-08-16）

**fork.rs**（8 个）：

| 测试 | 位置 | 契约 |
|------|------|------|
| test_fork_region_basic | :490 | 区域复制 + refcount 递增 |
| test_cow_copy_page | :513 | CoW 页复制（pfn 分离） |
| test_cow_copy_page_no_sharing | :534 | 无共享快速路径 |
| test_fork_rollback_on_ev_reference_error | :550 | ev_reference 失败逐 pfn 回滚 |
| test_fork_regions_rollback_on_failure | :596 | 多区域任一失败整体回滚 |
| test_handle_memory_once_no_cow | :659 | 已映射地址直接 OK |
| test_handle_memory_once_resolves_cow | :684 | 共享页解析 CoW |
| test_handle_memory_once_unmapped_address | :715 | 未映射 → PageNotMapped |

**vm_server.rs**（handle_fork 2 个）：

| 测试 | 位置 | 契约 |
|------|------|------|
| test_vm_server_handle_fork_not_found | :1379 | 无效父 endpoint → InvalidProcess |
| test_vm_server_handle_fork_rejects_exec_tmp_slot | :1395 | child_slot=NR_PROCS → InvalidProcess（03-P1-1 回归，本轮新增） |

### 5.2 覆盖维度

- **参数验证**：无效父 endpoint（not_found）、exec 临时槽拒绝（新增）。
- **回滚**：ev_reference 失败单区域回滚、多区域整体回滚。
- **CoW 解析**：handle_memory_once 三态（no-cow / cow / unmapped）。
- **区域复制**：fork_region 基本 + refcount 语义。

### 5.3 覆盖缺口与诚实标注

| 缺口 | 状态 | 说明 |
|------|------|------|
| do_fork 端到端（真实父进程多区域 → 子区域一致 + 父子页表只读） | ⚠️ 缺失 | 需真实页表/多区域测试基建（08/17 页表接线） |
| sys_fork 真实内核调用 | ⚠️ 未接线 | stub 确定性 endpoint（26 范围） |
| handle_memory_once ×2 消息页预解析 | ⚠️ DEFERRED | 依赖跨 slot 可变访问 + 真实 msgaddr（§4.4） |
| acl_fork 在 fork 上下文的端到端 | ⚠️ 部分 | `AclState::acl_fork` 有单测（acl.rs），fork 集成无 |
| write_page_table_mappings → PTE 只读联合 | ⚠️ 17-P2-2 承接 | COW flag 读侧接线（08/18 核对） |

### 5.4 测试统计（截至 2026-08-16）

```
$ cd os && cargo test -p minix-vm --lib
→ 361 passed / 1 failed（test_map_lazy pre-existing，13 范围，§5.3 已标注）
$ cargo test -p minix-vm --lib fork → 13 passed（fork.rs 8 + acl 2 + vm_server fork 2 + cow_exec_pf 相关）
$ cargo check -p minix-vm → Finished（110 warnings pre-existing，无 error）
```

---

## 6. 过渡

位置可回答性：fork 是 **17（CoW 原语）的编排者**——`fork_region`（17 §3.1）在这里被 `fork_regions` 批量调用，`setup_cow_for_all_regions`/`write_page_table_mappings`（17 §3.2）在这里被接线；同时是 **03（slot 表）/04（ACL）的消费面**——`get_empty`/`vm_isokendpt` 与 `AclState::acl_fork` 在此汇合。

向下游的移交：

- **22-vm-exit**：`free_proc`/`clear` 是 fork 的逆向——本文档的回滚（`free_forked_regions`/`free_page_table`）与 exit 的释放共享 `pb_unreferenced`/`unmap_page` 原语。
- **23-vfs-interaction**：`fork_region` 的 fdref `ref_entry`（fork.rs:107-109）与 VFS 文件引用计数。
- **26-vm-queries**：真实 `sys_fork` 内核 IPC 接线（返回 endpoint + msgaddr）。

---

## 7. 参见

- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/17-cow-mechanism.md` — CoW 机制（`fork_region`/`setup_cow_for_all_regions`/`write_page_table_mappings` 原语）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/04-acl.md` — ACL 继承（`AclState::acl_fork`）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/03-vmproc-table.md` — slot 表与 typestate 视图
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/15-ipc-dispatch.md` — 主循环分发与回复编码
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/16-pagefault.md` — `handle_memory_once` 同步变体（DEFERRED 接线基线）
- `minix3/minix/servers/vm/fork.c`（:32-115）— do_fork 七阶段
- `minix3/minix/servers/vm/region.c`（:933-998/:802-849/:589）— map_proc_copy / map_copy_region / map_free_proc
- `minix3/minix/servers/vm/acl.c`（:110-116）、`minix3/minix/servers/vm/utility.c`（:84-101）
- `minix3/minix/kernel/system/do_fork.c`（:112/:115-116）— 内核侧 sys_fork（msgaddr + RTS_VMINHIBIT）
- `minix3/minix/include/minix/com.h`（:633-635/:360）— VMF_* 字段 / PFF_VMINHIBIT
- `os/servers/vm/src/fork.rs`（:33-85/:92-161/:162-177/:186-409/:424-426）、`os/servers/vm/src/vmproc/vmproc_handle.rs`（:318-430）、`os/servers/vm/src/vmproc/table.rs`（:41-44/:141-150）、`os/servers/vm/src/ipc/dispatcher.rs`（:108-121/:1036-1037）、`os/libs/minix-types/src/ipc/vm.rs`（:53/:167-177/:678-695）— Rust 实现
