# 01-vm-init-main: 启动入口与初始化骨架

> **定位**: 阶段 1 — VM 从内核交接到进入主循环之前的启动链。
> **源码**: `minix3/minix/servers/vm/main.c`；`get_mem_chunks` 在 `minix3/minix/servers/vm/utility.c:get_mem_chunks`，`mem_add_total_pages`/`mem_init` 在 `minix3/minix/servers/vm/alloc.c`；SEF 库在 `minix3/minix/lib/libsys/sef*.c`。
> **Rust 模块**: `os/servers/vm/src/main.rs`、`boot.rs`、`global.rs`、`vm_server.rs`（`read_boot_params`/`BootParams`/`VmServer::init`/`run`）。
> **前置**: `rewrite-notes/02-stage-vm/00-vm-overview.md`、`rewrite-notes/01-stage-kernel/09-vm-boot-protocol.md`。
> **覆盖**: `main()`、`is_first_time()`、`init_vm()` 顺序、启动参数、`get_mem_chunks` 调用点、`__minix_init` 分界、`mem_add_total_pages` 调用点、boot 进程地址空间、SEF 生命周期与 RS_INIT 握手、VM 自身内存边界。
> **不覆盖**: `vmproc` 字段与表查找（02/03）、ACL 内部（04）、物理分配器内部（05/06）、页表结构与操作（07/08）、区域与 memtype（11–14）、主循环分发细节（15）、页错误与 CoW（16/17）、Live Update 完整流程（25）。
> **设计编号**: A1 = 内核交接引导页表根；A-3 v2 = VM 自身 `GlobalAlloc` 的 free-list 分配方案。

---

## 1. 概念：启动链——VM 如何从"内核给的临时环境"过渡到"自己管理一切"

内核把控制权交给 VM 时，VM 拿到的不是一套已经就绪的运行环境，而是一组互相依赖的初始条件：一块由内核建好的引导页表、一份物理内存清单、一张 boot 进程表，以及一个还没有堆、还没有进程表、也还没有 ACL 的自己。VM 的任务是在这些条件上把“管理别人内存”所需的结构一件件建起来，又不能在中途依赖尚未建立的设施。

这一章先说明这个循环依赖是什么、Minix3 把启动拆成哪几个阶段，以及四条执行模型前提为什么决定了后面的顺序。读完后，读者应能判断 `init_vm()` 里任意一步如果提前或推迟，会破坏哪条依赖。

### 1.0 本章要解决的问题

本章先把这个循环依赖拆成可执行的阶段，并说明四条执行前提如何约束顺序；完整的“鸡生蛋”叙述放在 §1.1。

> **本章不讲什么**:
> - `vmproc` 结构体字段与槽位语义（`02-vmproc-struct.md`）
> - 进程表查找与 endpoint 验证（`03-vmproc-table.md`）
> - ACL 数据结构与权限位（`04-acl.md`）
> - 物理内存布局解析与分配器（`05-physical-memory.md`）
> - 页表结构、保留页池、Direct Map（`07-pagetable-struct.md`、`08-pagetable-ops.md`）
> - 主循环的 5 优先级分发细节（`15-ipc-dispatch.md`）
>
> 本章只回答一个问题：**main() 到主循环之间，VM 按什么顺序、为什么按这个顺序，把自己初始化好**。

#### 最小示例

把启动链的入口和初始化函数先定位出来：

```bash
$ grep -n 'int main(void)\|static void init_vm\|static int is_first_time' minix3/minix/servers/vm/main.c
79:static int is_first_time(void)
93:int main(void)
428:static void init_vm(void)
```

#### 失败对照

如果跳过 `is_first_time()` 的门控，热重启的 VM 会再次执行 `init_vm()`，把已经建立好的进程表和页表当成空的重新初始化；所以门控必须先于任何初始化动作（`main.c:100-107`）。

### 1.1 启动链的核心问题：VM 的"鸡生蛋"

VM 是系统中第一个获得完整页表所有权的用户态服务，但它启动时只有内核提供的 bootstrap 环境。它面临一个循环依赖：

```
VM 需要内存管理能力来运行 → 内存管理是 VM 的职责 → VM 必须先初始化自己
```

Minix3 的解法是**分阶段初始化**：每个阶段只依赖前一阶段已经建立的设施。

1. **阶段 A（零依赖）**：只用内核提供的静态数据——`sys_getkinfo()` 拿到的 `kernel_boot_info`（内存布局、boot 进程表、模块表）。
2. **阶段 B（不需要堆）**：解析物理内存布局、清空进程表、初始化 ACL 与区域管理、建立物理内存位图（`mem_init` 只写静态位图，不消耗页表页）。
3. **阶段 C（需要物理页，用保留页池）**：建立 VM 自身进程槽与页表。
4. **阶段 D（IPC 向量可用）**：`__minix_init()` 从内核取得 `kerninfo` 与 IPC 向量（`minix3/minix/lib/libc/sys/init.c:20-33`），此后进行总页数校准、boot 进程地址空间建立。
5. **阶段 E（init_vm 尾部）**：注册 CALLMAP、标记 VM 实例。SEF 启动与进入主循环发生在 `init_vm()` 返回之后，由 `main()` 继续。

这个依赖链是本文档后半部分所有 C 源码分析的骨架。

### 1.2 初始化依赖链图

启动主线：`main()` → `init_vm()` → 主循环（`main.c:93-192`）：

```
main()  main.c:93
  │
  ├─ is_first_time()                      main.c:79-88（RS 是否仍持有 RTS_BOOTINHIBIT）
  │    └─ 是 → init_vm()                  main.c:100-102
  │          ├─ sys_getkinfo()            main.c:442-444（启动参数）
  │          ├─ enable_filemap + asserts  main.c:447-452
  │          ├─ get_mem_chunks()          main.c:455（物理内存布局，utility.c:44）
  │          ├─ memset(vmproc) + vm_slot  main.c:458-462（进程表清零）
  │          ├─ acl_init()                main.c:465
  │          ├─ map_region_init()         main.c:468
  │          ├─ mem_init()                main.c:471（物理页分配器，alloc.c:306）
  │          ├─ init_proc(VM_PROC_NR)     main.c:474（VM 自身槽）
  │          ├─ pt_init()                 main.c:475（VM 页表 + 保留页池）
  │          ├─ __minix_init()            main.c:480（内核 IPC 向量）
  │          ├─ mem_add_total_pages()     main.c:485-495（模块 + 内核占用校准）
  │          ├─ boot 进程循环             main.c:498-520（init_proc + exec_bootproc + free_mem）
  │          ├─ CALLMAP 注册              main.c:522-575
  │          └─ num_vm_instances + 标记   main.c:577-579
  │
  ├─ sef_local_startup()                  main.c:106（SEF 回调注册 + sef_startup）
  │
  └─ 主循环 while(TRUE)                   main.c:110-192
       ├─ missing_spares > 0 → alloc_cycle()
       ├─ sef_receive_status(ANY)
       ├─ is_ipc_notify → continue
       ├─ vm_isokendpt 验证 caller
       └─ 5 优先级分发（细节见 15-ipc-dispatch.md）
```

**阅读顺序即执行顺序**：第 2 章按此图自上而下展开，每一步回答"它在启动时序中的位置"；§2.5/§2.6 是执行链之外的边界内容。

**两套编号的对应**：第 1 章用 A–E 讲依赖层次，第 4 章用 Phase 1/2a–2d 讲 Rust 的完成时机；它们不是两套并行体系，按下表换算：

| 依赖阶段 | C 片段（`main.c`） | Rust 落点 |
|---|---|---|
| A 零依赖 | `sys_getkinfo` + asserts（442-452） | `read_boot_params` + `BootParams::validate`（构造前） |
| B 不需要堆 | `get_mem_chunks` / `memset` / `acl_init` / `map_region_init` / `mem_init`（455-471） | 分配器构造 + 编译期槽位 + 惰性区域表 |
| C 需要物理页 | `init_proc(VM_PROC_NR)` / `pt_init`（474-475） | `init_vm_self_pt()`（构造期）+ `init_vm_slot()`（Phase 2a） |
| D IPC 可用 | `__minix_init` / `mem_add_total_pages` / boot 进程（480-520） | `mark_initialized()`（末尾，职责对应；不是同一 wall-clock 时刻）+ `account_boot_memory()`（2b）+ `init_boot_procs()`（2c） |
| E init_vm 尾部 | CALLMAP / 实例标记（522-586） | 编译时分发 + `mark_vm_instance()`（2d） |

SEF 启动与主循环不在 A–E 的 `init_vm()` 序列里：C 在 `main()` 中调用 `sef_local_startup()` 后进入 `while(TRUE)`，Rust 在 `run_installed()` 中进入主循环。

### 1.3 SEF 生命周期是什么

SEF（System Event Framework，`minix3/minix/lib/libsys/sef.c`）是 Minix3 服务端共享的启动/热更新框架。VM 通过注册 fresh、LU/restart、状态变化和信号回调接入它；首次启动的 RS_INIT 应答必须异步发，否则 VM 与 RS 会互相等待。具体回调、注册时机与 VM 的处理见 §2.4，Rust 的简化实现见 §3.4。

### 1.4 执行模型四条前提

启动顺序不是随意的工程习惯，它受 VM 的执行模型约束。四条前提贯穿本篇，后续章节只引用不复述：

1. **单线程事件循环**。C 的 `main()` 在 `while(TRUE)` 里收消息、分发、回复（`main.c:110-192`）；Rust 的 `VmServer::run` 是同一形状（`vm_server.rs:1693`）。VM 内部不需要锁，也不能把某个等待点写成“另一个线程会推进”。
2. **VM 自己不能缺页**。VM 是页错误的处理者，它的元数据（进程表、页表、分配器位图）必须在访问前已经映射。C 用保留页池避免“分配页表页时还要先分配页表页”的循环（`pagetable.c:48-57` 的 spare pages 注释）；Rust 用 Direct Map 与预分配窗口承担同一职责（`07-pagetable-struct.md`）。
3. **只改不在运行的进程**。VM 修改的是别的进程的地址空间；被修改的进程必须处于阻塞或未运行状态，否则会出现“一边改页表、一边执行”的竞争。主循环的消息顺序与 IPC 阻塞语义保证这一点（`15-ipc-dispatch.md`）。
4. **启动期 fail-fast**。`init_vm()` 与 `exec_bootproc()` 没有降级路径：`sys_getkinfo` 失败、内存图或模块表为空、boot 进程装载失败都直接 panic（`main.c:442-452`、`main.c:504`）；Rust 二进制以 `panic = "abort"` 构建（`os/Cargo.toml:303`）。RS_INIT 是 `init_vm()` 之后的握手，属于协议失败而非启动链内部失败，见 §3.4。

四条前提解释了后面两个反复出现的“为什么”：为什么 `pt_init()` 要先于 `__minix_init()`（前提 2：VM 自己不能缺页），为什么 RS_INIT 不能在启动期同步等待（前提 1：单线程事件循环）。

### 1.5 本章回顾

#### 小结

- VM 的启动问题是循环依赖：它要管理内存，自己运行也需要内存；Minix3 用五阶段初始化切断这个环。
- 顺序由依赖决定：先拿内核数据，再建不需要堆的结构，再用保留页建页表，最后取得 IPC 向量并建 boot 进程。
- 四条执行前提（单线程、不能缺页、只改不在运行的进程、fail-fast）是后面所有“为什么在这个位置”的判据。
- 本章只建立模型；C 的逐函数证据在下一章，Rust 的取舍在第 3 章。

#### 术语

- **启动链**：从 `main()` 进入，到进入主循环之前，VM 建立自身运行环境的有序步骤。
- **鸡生蛋问题**：VM 需要内存管理能力才能运行，而内存管理正是 VM 的职责。
- **`RTS_BOOTINHIBIT`**：RS 在系统首次启动时持有的标志；VM 用它判断是否执行完整的 `init_vm()`。
- **`__minix_init`**：C libc 在 VM 确定内核映射后建立内核 IPC 向量的分界点；Rust 用 `mark_initialized()` 表达同一职责，但完成时机在 `init()` 末尾（不是 wall-clock 同一时刻）。
- **保留页池**：启动期预留给页表分配的物理页，用来避免“分配页表页需要先有页表”的循环。
- **ACL**：每个进程槽记录“允许调用哪些 VM 服务”的调用掩码表。
- **RTS**：内核进程表里的运行时状态位集合；`RTS_BOOTINHIBIT` 是其中一位。
- **`kernel_boot_info` / `kerninfo` / `rproctab`**：前者是 `sys_getkinfo` 返回的启动参数；`kerninfo` 是 `__minix_init` 读取的 IPC 向量页；`rproctab` 是 RS 暴露的进程公开信息数组，VM 侧副本叫 `rprocpub[]`。

---

## 2. C 源码分析

第 1 章给出了启动依赖模型，但模型只有落到 C 的实际调用顺序上才算证据。这一章沿 Minix3 的 `main()`、`is_first_time()`、`init_vm()`、`init_proc()`、`exec_bootproc()`、SEF 回调和 `map_service()` 走一遍，重点不是逐行翻译，而是看每一步依赖的前置设施在前一步是否已经就位；§2.6 是执行链之外的 VM 自身内存边界，为 §3.7 铺垫。

### 2.0 本章要回答的问题

读者需要能回答：VM 为什么先清进程表再初始化 ACL，为什么先建自己的页表再调 `__minix_init()`，为什么 boot 进程要等 `init_vm()` 尾部才被 `exec`，以及 RS_INIT 为什么不能在启动期同步等待。下面每条 C 证据都对应其中一个顺序判断。

#### 最小示例

先看入口与门控的实际形状。

```bash
$ grep -n 'static void init_vm\|static struct vmproc \*init_proc\|static void exec_bootproc' minix3/minix/servers/vm/main.c
428:static void init_vm(void)
262:static struct vmproc *init_proc(endpoint_t ep_nr)
331:static void exec_bootproc(struct vmproc *vmp, struct boot_image *ip)
```

#### 失败对照

如果把 `memset(vmproc, 0, ...)` 挪到 `init_proc()` 之后，`init_proc()` 读到的 `VMF_INUSE` 可能来自上一次实例，`assert(!(vmp->vm_flags & VMF_INUSE))` 会误判或漏判；所以清表必须发生在任何槽操作之前（`main.c:458-462`、`main.c:354`）。

### 2.1 main()：入口、is_first_time 门控、主循环（main.c:93-194）

```c
int main(void)
{
  message msg;
  int result, who_e, rcv_sts;
  int caller_slot;

  /* Initialize system so that all processes are runnable the first time. */
  if (is_first_time()) {        /* main.c:100 */
	init_vm();                  /* main.c:101 */
	__vm_init_fresh=1;          /* main.c:102 */
  }

  /* SEF local startup. */
  sef_local_startup();          /* main.c:106 */
  __vm_init_fresh=0;            /* main.c:107 */
  ...
```

`is_first_time()`（main.c:79-88）通过 `sys_getproc(RS_PROC_NR)` 读取 RS 进程的 RTS 标志：

```c
static int is_first_time(void)
{
	struct proc rs_proc;
	int r;

	if ((r = sys_getproc(&rs_proc, RS_PROC_NR)) != OK)
		panic("VM: couldn't get RS process data: %d", r);

	return RTS_ISSET(&rs_proc, RTS_BOOTINHIBIT);
}
```

**语义**：RS 在系统启动时持有 `RTS_BOOTINHIBIT`；VM 是第一个运行的服务器，此时 RS 尚未被解除启动抑制 → `is_first_time()` 为真 → 执行完整 `init_vm()`。解除 RS 抑制的正是 VM 自己在 boot 进程循环里对每个 boot 进程发出的 `VMCTL_BOOTINHIBIT_CLEAR`（§2.3.2 第 5 步），所以 VM 重启时该标志已经被上一轮清掉 → `is_first_time()` 为假，跳过 `init_vm()`，走 SEF 的 LU/restart 状态恢复路径。

`__vm_init_fresh` 是 VM 与 SEF 之间的标志，同时控制两件事：为 1 时 `sef_local_startup()` 把 RS_INIT 回复设为异步模式（`sef_cb_init_response_rs_asyn_once`，见 §2.4.3），并让 `sef_startup()` 跳过“等待 RS_INIT”阶段；`sef_startup()` 返回后清 0（main.c:107）。

主循环本体（main.c:110-192）的语义归属 `15-ipc-dispatch.md`；本文档只强调入口结构：`sef_receive_status(ANY)` 收消息 → `is_ipc_notify` 过滤 → `vm_isokendpt` 验证 caller → 分发 → `SUSPEND` 抑制回复。

### 2.2 init_vm()：完整初始化链（main.c:428-587）

`init_vm()` 是本文档的锚点函数。逐段分析：

#### 2.2.1 启动参数与 sanity（main.c:442-452）

```c
if(OK != (s=sys_getkinfo(&kernel_boot_info))) {
	panic("couldn't get bootinfo: %d", s);
}

/* Turn file mmap on? */
enable_filemap=1;	/* yes by default */
env_parse("filemap", "d", 0, &enable_filemap, 0, 1);

/* Sanity check */
assert(kernel_boot_info.mmap_size > 0);
assert(kernel_boot_info.mods_with_kernel > 0);
```

- `kernel_boot_info`（`minix3/minix/include/minix/param.h`，`kinfo_t`）是内核→VM 的启动契约：内存映射、boot 进程表、模块表、内核自身占用字节。
- 两个 assert 是启动协议的完整性检查：内存映射非空、模块表非空（`mods_with_kernel` 含内核自身）。
- `enable_filemap` 默认开启文件 mmap，可用 `filemap` 环境变量关闭（mmap 语义归 `20-vm-mmap.md`）。

#### 2.2.2 内存布局与基础结构（main.c:455-471）

```c
get_mem_chunks(mem_chunks);

/* Set table to 0. This invalidates all slots (clear VMF_INUSE). */
memset(vmproc, 0, sizeof(vmproc));
for(i = 0; i < ELEMENTS(vmproc); i++) {
	vmproc[i].vm_slot = i;
}

acl_init();
map_region_init();
mem_init(mem_chunks);
```

- `get_mem_chunks()`（utility.c:44-79）：把 `kernel_boot_info.memmap[]` 的字节偏移/长度转换为 click（4K）对齐的 `struct memory` 块，起始向上取整、结束向下取整。它必须先于 `memset` 与 `mem_init`，因为后面两张表都消费这组块。
- `memset(vmproc, 0, ...)` + `vm_slot = i`：进程表全清零（清掉所有 `VMF_INUSE`），并写入槽号。`vmproc` 结构语义归 `02-vmproc-struct.md`，查找语义归 `03-vmproc-table.md`。
- `acl_init()`：ACL 数据结构初始化（`04-acl.md`）。
- `map_region_init()`：区域管理初始化（`13-region-mapping.md`）。
- `mem_init(mem_chunks)`（alloc.c:306-334）：用内存块建立空闲页位图，同时累加 `total_pages = Σ chunks[i].size`（alloc.c:319-331）。位图本身是文件级静态数组 `free_pages_bitmap`（alloc.c:35），不需要保留页池，所以它可以早于 `pt_init()`；保留页池是页表分配器的启动储备，在 `pt_init()` 才建立。物理内存分配器语义归 `05-physical-memory.md`、`06-page-allocator.md`。

#### 2.2.3 VM 自身槽与页表（main.c:474-475）

```c
/* Architecture-dependent initialization. */
init_proc(VM_PROC_NR);     /* 474 — 定义于 main.c:262-291 */
pt_init();                 /* 475 — 定义于 pagetable.c */
```

- `init_proc(VM_PROC_NR)`：在进程表中建立 VM 自身槽（见 §2.3）。
- `pt_init()`：初始化 VM 自身页表与保留页池（pagetable.c，语义归 `08-pagetable-ops.md`）。

#### 2.2.4 __minix_init()（main.c:480）

```c
/* Acquire kernel ipc vectors that weren't available
 * before VM had determined kernel mappings
 */
__minix_init();
```

`__minix_init()` 是 libc 函数：它通过 `ipc_minix_kerninfo` 取得 `_minix_kerninfo`，再从 `minix_ipcvecs` 安装 IPC 向量（`minix3/minix/lib/libc/sys/init.c:20-33`）。VM 必须在确定内核映射后调用它，否则这些向量不可用；这条依赖对应 §1.4 前提 2（VM 自己不能缺页）。Rust 的对应设计见 §3.3，本章不再展开。

#### 2.2.5 mem_add_total_pages() 调用点（main.c:485-495）

```c
/* The kernel's freelist does not include boot-time modules; let
 * the allocator know that the total memory is bigger.
 */
for (mod = &kernel_boot_info.module_list[0];
	mod < &kernel_boot_info.module_list[kernel_boot_info.mods_with_kernel-1]; mod++) {
	phys_bytes len = mod->mod_end-mod->mod_start+1;
	len = roundup(len, VM_PAGE_SIZE);
	mem_add_total_pages(len/VM_PAGE_SIZE);
}

kern_dyn = kernel_boot_info.kernel_allocated_bytes_dynamic;
kern_static = kernel_boot_info.kernel_allocated_bytes;
kern_static = roundup(kern_static, VM_PAGE_SIZE);
mem_add_total_pages((kern_dyn + kern_static)/VM_PAGE_SIZE);
```

**这是本文档的关键调用点**。`mem_add_total_pages()`（alloc.c:281-284）只做一件事：

```c
void mem_add_total_pages(int pages)
{
	total_pages += pages;
}
```

语义：`mem_init()` 时 `total_pages` 只统计了空闲块；内核的空闲链表**不含** boot 模块（它们是已占用的 blob），所以分配器需要知道真实物理内存更大。校准分两笔：

1. **模块表**（main.c:485-490）：每个模块 `mod_end - mod_start + 1` 向上取整到页后计入。注意循环上界是 `mods_with_kernel - 1`——**最后一个条目被刻意排除**：它是内核自身的伪模块条目（`pre_init.c:198-200` 把内核登记为 `module_list[kern_mod]`，`mods_with_kernel = mi_mods_count + 1`），其占用在下一笔（内核自身）单独收费，避免重复计入。Rust 交接页的模块列表已经排除 VM 模块、也不含内核伪条目，因此 Rust 对列表全部条目计费，并单独补 VM 映像页（见 §3.6）。
2. **内核自身占用**（main.c:492-495）：`kernel_allocated_bytes`（静态）向上取整 + `kernel_allocated_bytes_dynamic`（动态，内核已按页对齐）后计入。

#### 2.2.6 boot 进程循环（main.c:498-520）

```c
for (ip = &kernel_boot_info.boot_procs[0];
	ip < &kernel_boot_info.boot_procs[NR_BOOT_PROCS]; ip++) {
	struct vmproc *vmp;

	if(ip->proc_nr < 0) continue;      /* 502 — 跳过内核任务 */

	assert(ip->start_addr);
	/* VM has already been set up by the kernel and pt_init().
	 * Any other boot process is already in memory and is set up
	 * here.
	 */
	if(ip->proc_nr == VM_PROC_NR) continue;
	vmp = init_proc(ip->proc_nr);
	exec_bootproc(vmp, ip);            /* 514 — 见 §2.3.2 */

	/* Free the file blob */
	assert(!(ip->start_addr % VM_PAGE_SIZE));
	ip->len = roundup(ip->len, VM_PAGE_SIZE);
	free_mem(ABS2CLICK(ip->start_addr), ABS2CLICK(ip->len));
}
```

- 每个非 VM 的 boot 进程（PM/VFS/RS/DS/...）先建槽（`init_proc`），再通过 `exec_bootproc` 从内存中的 ELF blob 建立地址空间并启动，最后把 blob 占用的物理内存归还分配器。
- VM 自身已由内核建好页表（`pt_init()` 处理），这里跳过。

#### 2.2.7 CALLMAP 注册（main.c:522-575）

```c
#define CALLMAP(code, func) { ... vm_calls[_cmi].vmc_func = (func); ... }

/* Set call table to 0. This invalidates all calls (clear vmc_func). */
memset(vm_calls, 0, sizeof(vm_calls));

CALLMAP(VM_MMAP, do_mmap);
CALLMAP(VM_MUNMAP, do_munmap);
CALLMAP(VM_MAP_PHYS, do_map_phys);
CALLMAP(VM_UNMAP_PHYS, do_munmap);
/* Calls from PM. */
CALLMAP(VM_EXIT, do_exit);
CALLMAP(VM_FORK, do_fork);
CALLMAP(VM_BRK, do_brk);
CALLMAP(VM_WILLEXIT, do_willexit);
CALLMAP(VM_PROCCTL, do_procctl_notrans);
/* Calls from VFS. */
CALLMAP(VM_VFS_REPLY, do_vfs_reply);
CALLMAP(VM_VFS_MMAP, do_vfs_mmap);
/* Calls from RS. */
CALLMAP(VM_RS_SET_PRIV, do_rs_set_priv);
CALLMAP(VM_RS_PREPARE, do_rs_prepare);
CALLMAP(VM_RS_UPDATE, do_rs_update);
CALLMAP(VM_RS_MEMCTL, do_rs_memctl);
/* Generic calls. */
CALLMAP(VM_REMAP, do_remap);
CALLMAP(VM_REMAP_RO, do_remap);
CALLMAP(VM_GETPHYS, do_get_phys);
CALLMAP(VM_SHM_UNMAP, do_munmap);
CALLMAP(VM_GETREF, do_get_refcount);
CALLMAP(VM_INFO, do_info);
/* Cache blocks. */
CALLMAP(VM_MAPCACHEPAGE, do_mapcache);
CALLMAP(VM_SETCACHEPAGE, do_setcache);
CALLMAP(VM_FORGETCACHEPAGE, do_forgetcache);
CALLMAP(VM_CLEARCACHE, do_clearcache);
/* getrusage */
CALLMAP(VM_GETRUSAGE, do_getrusage);
/* 其余 23 项略；NR_VM_CALLS = 49 */
```

- `vm_calls[]` 是 `NR_VM_CALLS`（`minix3/minix/include/minix/com.h` 第 769 行，值为 **49**）项的函数指针表；`CALLNUMBER(c)`（main.c:54-58）把调用号从 `VM_RQ_BASE` 归一到零基下标，越界返回 -1。
- 分发语义（5 优先级 + ACL）归 `15-ipc-dispatch.md`；本文档只记录"注册发生在 init_vm 尾部"。CALLMAP 必须在 boot 进程循环之后，因为 boot 进程的 `exec` 不查 `vm_calls`，而主循环一进来就会查。

#### 2.2.8 VM 实例标记（main.c:577-586）

```c
/* Mark VM instances. */
num_vm_instances = 1;
vmproc[VM_PROC_NR].vm_flags |= VMF_VM_INSTANCE;
/* Let SEF know about VM mmapped regions. */
s = sef_llvm_add_special_mem_region((void*)VM_OWN_HEAPBASE,
    VM_OWN_MMAPTOP-VM_OWN_HEAPBASE, "%MMAP_ALL");
```

- `num_vm_instances = 1`：Live Update 场景下区分新旧 VM 实例的计数。实例标记必须在槽激活之后，因为它要求 `VMF_INUSE` 已置位。
- `VMF_VM_INSTANCE` 标记 VM 槽，供 `clear_proc`/`swap_proc_slot` 识别。
- `sef_llvm_add_special_mem_region`：告知 SEF Live Update 状态传输器，VM 堆区到 mmap 顶区是特殊映射区（"`%MMAP_ALL`"）。

### 2.3 init_proc() 与 exec_bootproc()（main.c:262-291、331-417）

#### 2.3.1 init_proc()（main.c:262-291）

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

语义：在 boot 进程表中按 `proc_nr` 查找条目，验证槽位范围与"未占用"，`clear_proc()` 清零后设置 `VMF_INUSE`、`vm_endpoint`、`vm_boot`。

#### 2.3.2 exec_bootproc()（main.c:331-417）

`exec_bootproc` 为 boot 进程建立完整地址空间并启动执行，分五个子步骤：

1. **页表**：`pt_new(&vmp->vm_pt)` + `pt_bind(&vmp->vm_pt, vmp)`（main.c:352-356）。
2. **ELF 头读取**：`sys_physcopy(NONE, ip->start_addr, SELF, hdr, ...)`（main.c:358-360）。
3. **libexec 装载**：填充 `struct exec_info` 并注册五个回调（`junk` 与 `cleared` 各一个；main.c:362-381）：
   - `copymem = libexec_copy_physcopy`（main.c:294-303）：`sys_physcopy` 从模块 blob 拷段；
   - `clearmem = libexec_clear_sys_memset`；
   - `allocmem_prealloc_junk/cleared = libexec_alloc_vm_prealloc`（main.c:317-322）→ `boot_alloc(..., MF_PREALLOC)`；
   - `allocmem_ondemand = libexec_alloc_vm_ondemand`（main.c:324-329）→ `boot_alloc(..., 0)`。
   - `boot_alloc`（main.c:305-315）统一调用 `map_page_region(vmp, vaddr, 0, len, VR_ANON | VR_WRITABLE | VR_UNINITIALIZED, flags, &mem_type_anon)` 建立匿名区域。
   - `libexec_load_elf(execi)`（main.c:383-386）：实际 ELF 解析与装载。
4. **栈建立**：`minix_stack_params` + `minix_stack_fill` 生成初始栈（main.c:388-398），`handle_memory_once` 映射栈区（main.c:400-402），`sys_datacopy` 拷贝栈内容（main.c:404-406）。
5. **交付执行**：`sys_exec(endpoint, sp, progname, pc, psp)`（main.c:408-412）+ `sys_vmctl(VMCTL_BOOTINHIBIT_CLEAR)`（main.c:415-416）解除启动抑制。

### 2.4 SEF 生命周期（main.c:219-260、592-754；sef_init.c）

#### 2.4.1 sef_local_startup()（main.c:219-240）

```c
static void sef_local_startup(void)
{
	/* Register init callbacks. */
	sef_setcb_init_fresh(sef_cb_init_fresh);
	sef_setcb_init_lu(sef_cb_init_lu_restart);
	sef_setcb_init_restart(sef_cb_init_lu_restart);
	/* In order to avoid a deadlock at boot time, send the first RS_INIT
	 * reply to RS asynchronously. After that, use sendrec as usual.
	 */
	if (__vm_init_fresh)
		sef_setcb_init_response(sef_cb_init_response_rs_asyn_once);

	/* Register live update callbacks. */
	sef_setcb_lu_state_changed(sef_cb_lu_state_changed);

	/* Register signal callbacks. */
	sef_setcb_signal_handler(sef_cb_signal_handler);

	/* Let SEF perform startup. */
	sef_startup();
}
```

`sef_startup()`（`minix3/minix/lib/libsys/sef.c`）对 VM 的特殊路径只适用于首次启动：`__vm_init_fresh=1` 时 VM 跳过“等待 RS_INIT”阶段（sef.c 注释："VM handles fresh initialization by RS later"），RS_INIT 会在主循环中到达并交给 `do_sef_init_request`。热重启时 `__vm_init_fresh=0`，`sef_startup()` 不跳过等待，RS_INIT 在启动期就被消费；当前 Rust 只实现 fresh 路径，热重启路径尚未接线。

#### 2.4.2 sef_cb_init_fresh()（main.c:241-260）

```c
static int sef_cb_init_fresh(int type, sef_init_info_t *info)
{
	int s, i;

	/* Map all the services in the boot image. */
	if((s = sys_safecopyfrom(RS_PROC_NR, info->rproctab_gid, 0,
		(vir_bytes) rprocpub, sizeof(rprocpub))) != OK) {
		panic("vm: sys_safecopyfrom (rs) failed: %d", s);
	}

	for(i=0;i < NR_BOOT_PROCS;i++) {
		if(rprocpub[i].in_use) {
			if((s = map_service(&rprocpub[i])) != OK) {
				panic("unable to map service: %d", s);
			}
		}
	}

	return(OK);
}
```

首次启动时：从 RS 复制 `rprocpub[]`（各服务的公开信息：endpoint、调用掩码、启动标志），对每个 `in_use` 的服务调用 `map_service` 注册 ACL。

#### 2.4.3 RS_INIT 握手与异步回复（minix3/minix/lib/libsys/sef_init.c:do_sef_init_request、471-483）

`do_sef_init_request`（minix3/minix/lib/libsys/sef_init.c:do_sef_init_request）解析 RS_INIT 消息 → `process_init(type, &info)`（minix3/minix/lib/libsys/sef_init.c:process_init）→ 回调 `sef_cb_init_fresh` → 构造 RS_INIT 回复 → `sef_cb_init_response`。

```c
int sef_cb_init_response_rs_asyn_once(message *m_ptr)   /* sef_init.c:471-483 */
{
	/* This response function is used by VM to avoid a boot-time deadlock. */
	int r;

	/* Inform RS that we completed initialization, asynchronously. */
	r = asynsend3(RS_PROC_NR, m_ptr, AMF_NOREPLY);

	/* Use a blocking reply call next time. */
	sef_setcb_init_response(SEF_CB_INIT_RESPONSE_DEFAULT);

	return r;
}
```

**为什么异步**：启动时 RS 也在等待 VM 完成初始化；若 VM 用同步 `sendrec` 回复，双方互相等待形成死锁。异步回复（`asynsend3`）打破环，且只对**首次** fresh init 生效（`__vm_init_fresh` 门控）；这对应 §1.4 前提 1（单线程事件循环里没有第二个线程替 VM 推进）。

主循环中的 RS_INIT 分支（main.c:149-152）：`do_sef_init_request(&msg)` 返回 OK 后 `result = SUSPEND`——**不二次回复**（回复已由 SEF 回调完成）。

#### 2.4.4 LU/restart 回调（main.c:196-217、592-730）

- `sef_cb_lu_state_changed`（main.c:196-217）：LU 失败进入 `SEF_LU_STATE_NULL` 时，重新绑定页表、清 map cache、调整进程引用。
- `sef_cb_init_vm_multi_lu`（main.c:592-676）：多组件更新时构建 IPC 白名单过滤（只允许 RS/正在更新的服务的 `VM_BRK`/`VM_INFO`），并对 `SF_VM_UPDATE` 服务执行 `do_rs_update`。它不是独立注册的回调，而是由 `sef_cb_init_lu_restart` 在末尾直接调用（main.c:725）。
- `sef_cb_init_lu_restart`（main.c:677-730）：`SEF_CB_INIT_LU_DEFAULT` 状态迁移 → `swap_proc_slot` + `swap_proc_dyn_data` 交换新旧 VM 实例状态 → `pt_bind` 重绑 → `adjust_proc_refs` → multi-LU。

#### 2.4.5 sef_cb_signal_handler()（main.c:731-754）

```c
static void sef_cb_signal_handler(int signo)
{
	/* Check for known kernel signals, ignore anything else. */
	switch(signo) {
		/* There is a pending memory request from the kernel. */
		case SIGKMEM:
			do_memory();
		break;
	}

	/* It can happen that we get stuck receiving signals
	 * without sef_receive() returning. We could need more memory
	 * though.
	 */
	if(missing_spares > 0) {
		alloc_cycle();	/* pagetable code wants to be called */
	}

	pt_clearmapcache();
}
```

`SIGKMEM` 是内核发来的"需要内存"信号（`do_memory` 语义归 `06-page-allocator.md` 的保留页池）；信号处理末尾还检查 `missing_spares` 并清页表映射缓存。

### 2.5 map_service()（main.c:755-768）

```c
static int map_service(struct rprocpub *rpub)
{
/* Map a new service by initializing its call mask. */
	int r, proc_nr;

	if ((r = vm_isokendpt(rpub->endpoint, &proc_nr)) != OK) {
		return r;
	}

	/* Copy the call mask. */
	acl_set(&vmproc[proc_nr], rpub->vm_call_mask, !IS_RPUB_BOOT_USR(rpub));

	return(OK);
}
```

按 endpoint 查进程槽，把服务的 `vm_call_mask` 写入该槽的 ACL（`IS_RPUB_BOOT_USR` 决定是系统服务还是用户服务）。ACL 语义归 `04-acl.md`。

### 2.6 VM 自身 libc 接口边界（utility.c:361-420）

VM 进程自身链接 libc，libc 的 `mmap`/`munmap`/`_brk` 需要 VM 服务自身：

- `mmap`（utility.c:361-374）：`assert(!addr)` + 页对齐 → `vm_allocpages(&p, VMP_SLAB, len/VM_PAGE_SIZE)` → `memset` 清零。
- `munmap`（utility.c:376-380）：`vm_freepages` 归还。
- `_brk`（utility.c:385-420）：扩展 VM 自身堆——从 `&_end` 起逐页 `alloc_mem` + `pt_writemap` 映射到自身页表（缓存属性），`sys_vmctl(VMCTL_FLUSHTLB)` 刷 TLB。

**边界声明**：这三个函数是 VM 内部 libc 的"自服务"实现，供 VM 自身的 malloc/SEF 缓冲使用；它们不是 VM 对外服务（对外服务是 `VM_MMAP`/`VM_MUNMAP`/`VM_BRK`）。Rust 侧等价边界见 §3.7。

### 2.7 本章回顾

#### 小结

- `is_first_time()` 用 RS 的 `RTS_BOOTINHIBIT` 区分首次启动与热重启；首次启动才执行完整 `init_vm()`。Rust 交接页当前恒报 fresh，生产上只走 `ColdInit`。
- `init_vm()` 的顺序是依赖顺序：清表 → ACL/区域 → 物理分配器 → VM 自身槽与页表 → IPC 向量 → 总页数校准 → boot 进程 → CALLMAP → 实例标记。
- `exec_bootproc()` 为每个 boot 进程建页表、读 ELF、装载段、建栈并 `sys_exec`，最后清 `BOOTINHIBIT`。
- SEF 在 C 中承担 fresh/LU/restart/信号四类生命周期；VM 首次启动用异步 RS_INIT 回复避免与 RS 互等。
- `map_service()` 把 RS 提供的调用掩码写进对应进程槽的 ACL。

#### 术语

- **`kinfo` / `kernel_boot_info`**：内核传给 VM 的启动信息，含内存图、boot 进程表、模块表和内核占用。
- **`init_proc`**：按 `proc_nr` 在 boot 进程表里找到条目、清槽并置 `VMF_INUSE` 的函数。
- **`BOOTINHIBIT`**：boot 进程被内核抑制调度，直到 VM 完成地址空间装配并通过 `VMCTL_BOOTINHIBIT_CLEAR` 解除。
- **`map_service`**：按 endpoint 找槽并把服务的 `vm_call_mask` 写进 ACL 的 C 函数。

---

## 3. Rust 设计决策

C 侧的启动顺序已经清楚，Rust 版本要回答的不是“怎样逐行翻译”，而是：哪些 C 的全局状态和框架可以消失，哪些契约必须保留。比如 C 的 `kernel_boot_info` 是任何函数都能读的 BSS 全局，Rust 若照搬，测试和启动链都会被隐藏状态绑住；C 的 SEF 用回调状态机处理 fresh/LU/restart，Rust 若照搬，会把一个启动期握手变成框架。本章逐个说明这些取舍，并给出每个决策对应的 C 证据。

### 3.0 本章要回答的问题

读者需要能回答：为什么启动参数要显式建模为 `BootParams`，为什么 `is_first_time` 只是一个布尔输入而不是 VM 自己去查内核表，为什么 Rust 的 `init()` 保持 C 的阶段划分，为什么 `rs_handshake()` 能承担 SEF 的 fresh 握手，以及 `exec_bootproc` 的哪些部分已经落地、哪些仍受内核 IPC 能力约束。

#### 最小示例

启动契约的形状可以直接从类型和测试入口看到。

```bash
$ grep -n 'pub struct BootParams\|pub fn validate\|pub fn extra_pages' os/servers/vm/src/boot.rs
61:pub struct BootParams<'a> {
187:    pub fn validate(&self) {
263:    pub fn extra_pages(&self) -> usize
```

#### 失败对照

如果 Rust 继续读一个全局 `kernel_boot_info`，单元测试就必须先伪造全局状态，启动链也无法从签名看出依赖；如果 RS_INIT 在启动期同步等待，VM 与 RS 会互相等待。`BootParams` 与异步出生应答分别消除这两个失败面。

### 3.1 启动契约显式化：BootParams（对应 §2.2.1）

**C**：`kernel_boot_info` 是 BSS 全局，任何函数可隐式读取。**Rust**：内核→VM 启动协议是一次性、单向的握手（`01-stage-kernel/09-vm-boot-protocol.md`），因此建模为显式构造输入：

```rust
pub struct BootParams<'a> {
    pub root_paddr: PhysBytes,                 // 内核建好的引导页表根（A1 采用）
    pub kerninfo_pa: PhysBytes,                // 内核信息页的物理地址
    pub total_pages: usize,                    // 位图容量：最高可用页号 + 1
    pub free_regions: &'a [BootMemRegion],     // C: mem_chunks[]（get_mem_chunks）
    pub boot_procs: &'a [BootImage],           // C: kernel_boot_info.boot_procs[]
    pub modules: &'a [BootModule],             // C: kernel_boot_info.module_list[]
    pub kernel_allocated: KernelAllocated,     // C: kernel_allocated_bytes(_dynamic)
    pub vm_allocated_bytes: u64,               // C: kernel_boot_info.vm_allocated_bytes
    pub is_first_time: bool,                   // 交接页携带；当前内核恒写 1
    pub kernel_layout: Option<KernelLayout>,   // 内核布局，页表初始化使用
    pub user_sp: VirBytes,                     // 初始用户栈顶（C: kernel_boot_info.user_sp）
}
```

理由：
- **可见性**：构造参数显式暴露启动依赖，`VmServer::new_with_boot_params` 的签名即文档。
- **可测性**：测试用 `BootParams::simple()` 构造单区域、无模块的参数，无需伪造全局。
- **与 C 的差异（ARCH 注记）**：C 的 `kernel_boot_info` 是一个全局结构；Rust 把其中与启动链直接相关的字段（根页表、内核信息页、内核布局、初始栈顶、VM 映像占用）收进 `BootParams`，由 `read_boot_params()` 从内核交接页一次读出。页表模块消费 `kernel_layout`，但字段仍归启动契约，避免再引入一个隐式全局。

`validate()` 复刻 C 的 sanity 检查（main.c:451-452）并补充交接页约束：根页表非零且页对齐、内核信息页页对齐、空闲区域非空且各自合法、`total_pages` 等于最高可用页号 + 1、boot 进程号在表容量内、VM 槽不重复。C 的 `assert(mods_with_kernel > 0)` 在 Rust 中**不强制**：单元测试构造无模块参数；生产 boot 协议保证模块表非空（已在 `boot.rs` 文档注释中说明）。

#### 3.1.1 A2 消费边界：adopt 与 reconcile

`read_boot_params()` 读出交接页后不只做 `validate()`：它先 `handoff.validate()`，再原样 adopt `free_regions`（不在这里裁剪掉任何“看起来不该存在”的区间），随后调用 `reconcile()` 复核内核的分类记录（`boot.rs:374-470`）。四条断言是：

- 扣减记录 `deducted` 非空；
- 引导页表根落在扣减记录内；
- 每个仍被保留的 boot 模块 blob 落在扣减记录内；
- 空闲区与扣减区不相交。

任一条不满足都在构造 PMM 之前 panic。这样 VM 不会通过“悄悄裁掉一块”来掩盖内核记账错误；A2 的边界证明必须在内核侧成立，VM 只验证并消费。

### 3.2 is_first_time 的表达与 init 门控（对应 §2.1）

**C**：`is_first_time()` 通过 `sys_getproc(RS_PROC_NR)` 读内核进程表。**Rust**：`BootParams::is_first_time` 由交接页携带；当前内核构造交接页时**恒写 1**（`os/kernel/src/vm_handoff.rs:713`），并未读取 RS 的 `RTS_BOOTINHIBIT`。因此四态路由在当前生产路径上只有 `ColdInit` 可达；`WarmRun`/`WarmNoState`/`DoubleInit` 是诊断占位，LU/restart 需要在 25 篇与交接页版本一起落地 [ARCH: 镜像槽四态路由]。

```rust
// C: main.c:100-107 — if(is_first_time()) { init_vm(); __vm_init_fresh=1; }
match boot_route(params.is_first_time) {
    BootRoute::ColdInit => { /* 装入 image slot，再 server.init() */ }
    BootRoute::WarmRun => {}
    BootRoute::WarmNoState => panic!("warm start without image state"),
    BootRoute::DoubleInit => panic!("cold boot with a full image slot"),
}
```

四态的真值表如下（`is_first_time` 来自 `BootParams`，slot 状态由 `server_installed()` 读静态槽）：

| `is_first_time` | image slot | `BootRoute` | 动作 | 当前可达 |
|---|---|---|---|---|
| `true` | 空 | `ColdInit` | 构造 server，装入 slot，执行 `init()` | 是（当前内核恒报 fresh） |
| `false` | 有 server | `WarmRun` | 跳过 `init()`，直接从 slot 进入主循环 | 否（LU/restart 未接） |
| `false` | 空 | `WarmNoState` | panic：re-exec 后没有可恢复状态 | 否 |
| `true` | 有 server | `DoubleInit` | panic：`init()` 只能执行一次 | 否（新进程 slot 必空） |

`WarmRun` 在当前生产路径上不可达；如果将来接入同进程热重启，slot 里的 server 必须已经完成 `register_page_alloc`、`mark_initialized` 与 `initialized = true`，否则 `run()` 会在未初始化检查处 fail-fast。当前四态表只用于诊断，不应被读成四条都可用。

`boot_route` 除了 `is_first_time` 还读取 `server_installed()` 的静态槽；测试通过 reset/install 控制该槽来覆盖四态。C 的 restart 由 SEF/LU 处理，Rust 尚未实现；不要用 C 的 restart 行为推断 image slot 里的内容。

`__vm_init_fresh`（异步 RS_INIT 回复门控）在 Rust 中不需要独立字段：`rs_handshake()` 的出生应答直接走单臂 `asynsend`（`AMF_NOREPLY`）。当前只实现 fresh 路径，`rs_handshake` 实际上每个 VM 实例只处理第一次 RS_INIT；C 在首次之后改回阻塞回复的语义尚未接线，若将来处理第二次 RS_INIT，必须重新核对。

### 3.3 init() 分阶段与 C 的逐行对应（对应 §2.2）

`VmServer::init()` 的注释块即 C 的步骤映射表。核心结论：

| C 操作 | Rust 等价 | 何时完成 |
|--------|----------|---------|
| `init_global_state()` 的输入（`mem_init` 的总页数结果 + kernel layout） | `global::init` + `global::set_kernel_layout` | `init()` Phase 1 |
| `sys_getkinfo` + asserts（main.c:442-452） | `BootParams::validate()` | `new_with_boot_params` |
| `get_mem_chunks` → `mem_init`（main.c:455,471） | `create_default_allocator` + `VmPageAllocator::new` | `new_with_boot_params` |
| `memset(vmproc,0)` + `vm_slot=i`（main.c:458-462） | 编译期 `[AssumeSyncCell::new(VmProc::vacant()); N]`；`get_empty()` 写 `vm_slot` | 编译期 / 槽激活时 |
| `acl_init()`（main.c:465） | `AclState::Uninitialized` 编译期默认 | 编译期 |
| `map_region_init()`（main.c:468） | `RegionMap::new()`（惰性，每进程初始化时） | `init_regions()` 时 |
| `init_proc(VM_PROC_NR)`（main.c:474） | `VmServer::init_vm_slot()` → `EmptySlot::activate` + `set_boot` | `init()` Phase 2a |
| `pt_init()`（main.c:475） | 结构变化 [ARCH: A1]：C 建自己的页表并预置保留页池；Rust 的 `init_vm_self_pt()` adopt 内核引导根（`root_paddr`），不依赖 VM 槽，故在构造期完成 | `new_with_boot_params`（非 test） |
| `__minix_init()`（main.c:480） | `transport.mark_initialized()` 打开 IPC 就绪门；系统调用本体仍依赖 `minix-sys` 的内核 IPC 原语 | `init()` 末尾 |
| `mem_add_total_pages`（main.c:485-495） | `VmServer::account_boot_memory()` → `global::add_total_pages` | `init()` Phase 2b |
| boot 进程循环（main.c:498-520） | `VmServer::init_boot_procs()` + `exec_bootproc()`：装载、栈帧、`sys_exec` 与 blob 释放均实现；`sys_exec` 的 `name_ptr` 仍为 0 | `init()` Phase 2c |
| CALLMAP（main.c:522-575） | `MessageDispatcher::dispatch_by_number` 编译时 match | 编译期 |
| VM 实例标记（main.c:577-579） | `VmServer::mark_vm_instance()` → `ActiveProc::mark_vm_instance` | `init()` Phase 2d |
| SEF 特殊映射区登记（main.c:582-583） | 当前没有对应调用；`%MMAP_ALL` 区登记随 Live Update 归 25 | `25-rs-services.md` |

**当前实现边界**：`__minix_init` 的就绪机制已经存在（`transport.mark_initialized()`），真实系统调用仍依赖 `minix-sys` 的内核 IPC 原语。`exec_bootproc()` 的 ELF 装载、`minix_stack_*` 栈帧、`sys_exec` 与 blob 释放已经实现；`sys_exec` 的 `name_ptr`（progname）仍以 0 传递。boot 阶段没有降级路径，任何装载失败都按 C 的 fail-fast 语义 panic。

### 3.4 SEF 简化：rs_handshake + do_sef_init_request（对应 §2.4）

SEF 是 Minix3 服务端的启动/热更新框架；它仍通过 RS 的 `rproctab` 把各服务的公开信息交给 VM。Rust 去掉的是框架的状态机外壳，保留“取 rproctab、写 ACL、发出生应答”这条协议。SEF 在 C 中解决 3 个问题，Rust 各有更直接的实现：

| SEF 组件 | C 中的必要性 | Rust 下的处理 |
|---------|-------------|--------------|
| `sef_startup()` + `sef_cb_init_fresh`（main.c:241-260） | C 的服务初始化走 RS_INIT + rproctab；SEF 提供框架状态机 | 保留协议、去掉框架：`VmServer::rs_handshake()`——`ipc_call_rs_init()` 取 rproctab → 逐条 `acl_set`（等价 `map_service`） |
| `do_sef_init_request` + `sef_cb_init_response`（minix3/minix/lib/libsys/sef_init.c:do_sef_init_request） | 主循环 RS_INIT 分支 | 主循环优先级 2 直接调 `rs_handshake()`：成功 → `sef_init_reply(OK)` 出生应答后 `Suspend`；失败 → 先回 `sef_init_reply(errno)`，再丢弃请求、计数并审计。C 在 main.c:151 panic；Rust 选择让 VM 存活并把错误交给 RS [ARCH: A-14]。这条豁免以当前生产只走 `ColdInit` 为前提：`init_vm()` 已完成、没有半初始化状态需要回滚，RS 可以按启动失败处理。若将来接入 `WarmRun`，RS_INIT 失败的策略必须重新定义。 |
| `sef_cb_init_response_rs_asyn_once`（minix3/minix/lib/libsys/sef_init.c:sef_cb_init_response_rs_asyn_once） | 避免启动死锁 | 出生/失败应答经 `IpcTransport::asynsend` 单臂发出；同步 `send` 在 RS 仍处 PAGEFAULT\|SENDING 时会形成 EDEADLOCK，不能替换 |
| `sef_cb_init_lu_restart` / `sef_cb_lu_state_changed` / `sef_cb_init_vm_multi_lu`（main.c:196-217,592-730） | Live Update 状态机 | `rs.rs` 的 `handle_rs_prepare`/`handle_rs_update` 已预留入口；swap_proc_slot 等 LU 语义归 `25-rs-services.md` |
| `sef_cb_signal_handler`（main.c:731-754） | 信号经 IPC 通知送达 | 主循环把收到的号码经 `VmServer::effective_kernel_signal` 换算后交 `handle_signal`；`SIGKMEM` 转 `do_memory`。换算原因见下表后的三步说明 |

信号路径分三步读：

1. **C 的路径**：内核发 `SIGKMEM`，SEF 的信号回调把它转成 `do_memory()`，由保留页池补充内存。
2. **本树内核的投递形状**：`SigSet` 已扩到 128 位，位 70/74 可以表示（`os/kernel/src/syscall_signal.rs:70-104`）；兜底仍然可能需要，因为 `vm_enqueue_and_notify_vm` 只发裸通知，没有把 `SIGKMEM` 置入 `p_pending`，`minix-sef` 递来的是 `SEF_SIGNAL_REQUEST_TYPE` 而不是 71。
3. **Rust 的换算**：VM 能收到的 `SYSTEM` 通知只可能来自 `vm_enqueue_and_notify_vm`（即 C 的 `send_sig(VM, SIGKMEM)`），所以兜底值按 `SIGKMEM` 算；真号进来时按号分派。号码的单一权威是 `minix_types::SIGNAL_KERNEL_MEMORY`。

**结论**：`rs_handshake()`（约 30 行）承担 SEF 的 fresh 握手协议，框架复杂度消除。当前只实现 fresh：`RsInit` 的 `init_type`、`flags`、`old_endpoint`、`restarts`、`buff_addr`/`buff_len`、`prepare_state` 解码后未消费，LU/restart 的 RS_INIT 不会被区分；完整状态机归 `25-rs-services.md`，不能把当前实现称为 SEF 全等价。

### 3.5 exec_bootproc 的 Rust 策略（对应 §2.3.2）

`exec_bootproc` 的五个子步骤在 Rust 中的归属：

| 子步骤 | Rust 状态 | 归属 |
|--------|----------|------|
| `pt_new` + `pt_bind` | `pt_new` 由 `init_proc` → `init_page_table()` 建表；`pt_bind` 的实质是 `exec_bootproc` 里的 `sys_vmctl_set_addrspace`（生产走 `CurrentPaging`，测试走模拟分页后端） | `08-pagetable-ops.md` / `exec_bootproc` |
| ELF 头读取 + `libexec_load_elf` | 已有实现：Direct Map 直读映像 + `minix_elf::segment_iter`/`entry_point` + 逐段匿名 region + 逐页 `alloc_pfn`/`map_page` 实化 + 段字节/BSS 写入 | `exec_bootproc`（vm_server.rs） |
| 栈建立（`minix_stack_*`） | 已实现：`install_boot_stack` 用 `minix_sys::stack::stack_params` 定尺寸、`stack_fill` 布局 argc/argv/envp/ps_strings，建立 4 MiB 栈区并经 `handle_memory_once` 映射校验，帧字节经 Direct Map 写入；`sys_exec` 使用真实 `vsp`/`ps_str`。唯一余项是 `name_ptr = 0` | `vm_server.rs` / `minix-sys::stack` |
| `sys_exec` | 已有实现：`KernelGateway::sys_exec`（wire：`MessLsysKrnSysExec{endpt,ip,stack,name,ps_str}`，m_type=Syscall::Exec=1；内核 `dispatch_exec` 真实）；失败时 panic（boot 阶段 fail-fast） | `kernel_gateway.rs` |
| `VMCTL_BOOTINHIBIT_CLEAR` | 已有实现：`KernelGateway` 经 `sys_vmctl` 发 `VMCTL_BOOTINHIBIT_CLEAR` | `09-vm-boot-protocol.md` |
| 槽位建立（`init_proc` 等价） | 已有实现：`VmServer::init_boot_procs`；循环内调 `exec_bootproc` 并按 C main.c:513-516 释放映像 blob | 本文档 §4.4 |

**当前实现边界**（本节唯一口径，§3.9 与 §4.4 都以此为准）：

| 子步骤 | 当前状态 |
|---|---|
| `pt_new` / `pt_bind` / 页表建立 | 已实现（`init_page_table`，生产走 `CurrentPaging`） |
| ELF 头读取、段装载、BSS 清零 | 已实现（`minix_elf` + Direct Map 直读映像） |
| `minix_stack_*` 初始栈帧 | 未实现；`stack`/`ps_str` 以 0 上报 |
| `sys_exec` | 已实现（`KernelGateway::sys_exec`，真实 `vsp`/`ps_str`） |
| `VMCTL_BOOTINHIBIT_CLEAR` | 已实现（`KernelGateway` 的 `sys_vmctl`） |
| 映像 blob 释放 | 已实现（`init_boot_procs` 循环内） |
| Live Update / restart 状态机 | 入口已预留；状态机归 `25-rs-services.md` |

`init_boot_procs` 保留 C 的两个保护：跳过负 `proc_nr`（内核任务）、`assert(start_addr != 0)`。

### 3.6 mem_add_total_pages → global::add_total_pages（对应 §2.2.5）

**C**：`mem_add_total_pages`（alloc.c:281-284）直接改全局 `total_pages`。**Rust**：

```rust
pub(crate) unsafe fn add_total_pages(pages: usize) {
    unsafe { *TOTAL_PAGES.get() += pages; }
}
```

调用点封装在 `BootParams::extra_pages()`：

```rust
// C: main.c:485-490 — handoff 列表已排除 VM 模块、无内核伪条目，
// 因此每个条目都是真实保留模块。
for m in self.modules {
    pages += round_up_page(m.len) / CLICK_SIZE;
}
// VM 映像不在 handoff 模块列表里，按内核报告的字节数单独补记。
pages += round_up_page(self.vm_allocated_bytes) / CLICK_SIZE;

// C: main.c:492-495 — 内核占用（static 向上取整 + dynamic）
```

**为什么不直接改分配器**：`total_pages` 是"真实物理内存总页数"（统计/查询用，`26-vm-queries.md` 的 `do_info` 读取它）。分配器侧的 `total_pages()` 是位图容量，不是空闲页数；空闲页数在 `memstats().free_pages`。C 的对应物是一张覆盖整个 32 位物理空间的固定位图加一本 `total_pages` 账。启动链里出现四个相近名字，按这张表读，不要混：

| 名字 | 口径 | 谁写 | 谁读 |
|---|---|---|---|
| `BootParams.total_pages` | 位图容量 = 最高物理页号 + 1 | `read_boot_params` | 分配器构造 |
| `boot_usable_pages` | Σ 空闲区页数（C 的 `total_pages`） | `new_with_boot_params` | `global::init` |
| `VmPageAllocator::total_pages()` | 后端总容量（位图/伙伴分配器自己的计数） | 分配器构造 | `PageFrames::new` |
| `global::total_pages()` | 可用页 + boot 模块/VM 映像/内核额外页（C 的 `total_pages` + `mem_add_total_pages`） | `global::init` + `add_total_pages` | 统计与查询 |

`PageFrames` 是物理帧的元数据表（每帧状态与引用），不是分配器位图；它在 `PageFrames::new(total_phys)` 时按后端总容量建立。

### 3.7 VM 自身内存：无 libc mmap（对应 §2.6，ARCH 注记）

C 的 utility.c 为 VM 自身 libc 实现 `mmap`/`munmap`/`_brk`（§2.6）。Rust 的 `no_std` VM 不使用 libc 堆接口，自身内存来源是 `VmAllocator`（`global.rs`，实现 `GlobalAlloc`）：虚拟地址来自 `HeapArena` 的连续窗口，物理帧由 `VmPageAllocator` 分配，再经 `vm_self_mappages()` 逐页映射进 arena；Direct Map 只用于元数据与页表页访问（[ARCH: A-3 v2]，09-slab-allocator.md §3.1）。

**这是 ARCH 差异，标注三处一致**：
- 本文档（§3.7）：C 用 `_brk` 逐页映射自身堆；Rust 用 `GlobalAlloc` free-list 分配器（A-3 v2）。
- design：`rewrite-notes/02-stage-vm/plan.md` 的 A-3 行与 `09-slab-allocator.md` §3.1。
- 代码：`global.rs` 的 `VmAllocator` 注释（A-3 v2：free-list + 合并，[ARCH: A-3 v2] 标注）。

语义等价点：C 的 `_brk` 页映射 + `pt_writemap` 与 Rust 的 `heap_arena_grow` 都解决“VM 自身需要可写内存”；C 的 `munmap` 显式归还与 Rust 的 free-list `dealloc`（A-3 v2：块级回收复用，arena 页不收缩）在“可分配内存复用”上等价。它们对“VM 自身驻留页统计”不等价：C 的 `get_vm_self_pages()` 会随 `munmap` 下降，Rust 的 `self_page_count()` 不会因 free-list 回收而下降；统计口径归 `26-vm-queries.md`。

### 3.8 CALLMAP → 编译时 match（对应 §2.2.7）

C 用运行时函数指针表（`memset` + 逐个 `CALLMAP`），因为 C 没有编译时分发。Rust 的 `MessageDispatcher::dispatch_by_number` 用 `match` 在编译期解析全部 49 个调用号（`NR_VM_CALLS` 已对齐 C 的 `com.h:769`），零成本跳转表 + 类型安全。分发细节归 `15-ipc-dispatch.md`。

### 3.9 本阶段 ARCH 决定

| 编号 | C 行为 | Rust 行为 | 依据 |
|---|---|---|---|
| A1 | `pt_init` 自建 VM 页表并预置保留页池 | 采用内核建好的引导页表根；保留页池由 Direct Map 消除 | `os/servers/vm/src/boot.rs` 的 `root_paddr`；`vm_self_map.rs` |
| A-3 v2 | libc `_brk` 逐页映射自身堆 | `HeapArena` 连续 VA + `VmPageAllocator` 帧 + `vm_self_mappages` | `global.rs` / `heap_arena.rs` |
| A-5 | 无对应；VM 在最终地址上初始化 | server 先装入 image slot，`init()` 开头注册全局指针并 `relocate()` | `vm_server.rs` |
| A-14 | RS_INIT 失败直接 panic | 回错误、丢弃请求并审计，VM 保持存活 | `vm_server.rs` 优先级 2 |
| 镜像槽四态路由 | `is_first_time` 二值门控 | 四态 `BootRoute`；当前生产只可达 `ColdInit` | `main.rs` / `vm_server.rs` |

### 3.10 本章回顾

#### 小结

- `BootParams` 把 C 的全局启动信息变成显式输入，`is_first_time` 由内核侧判定后作为布尔值传入；`validate()` 复刻 C 的 sanity 检查。
- `VmServer::init()` 保持 C 的阶段划分，但两处完成时机不同：`pt_init` 在构造期，`mark_initialized` 在末尾；原因见 §4.4.2。
- SEF 的 fresh 握手收敛为 `rs_handshake()`；LU/restart 仍留入口，细节归 25。
- `exec_bootproc` 的装载、栈帧、`sys_exec` 与 `VMCTL_BOOTINHIBIT_CLEAR` 已实现；`name_ptr` 仍以 0 传递。
- `total_pages` 与分配器空闲页池保持 C 的两本账；VM 自身内存改用 `GlobalAlloc`。

#### 术语

- **`BootParams`**：内核→VM 启动契约的 Rust 显式建模，含空闲区、boot 进程、模块、内核占用与 `is_first_time`。
- **`BootRoute`**：Rust 入口对 cold/warm/no-state/double-init 四种启动形态的路由结果。
- **`rs_handshake`**：Rust 对 SEF fresh 握手的显式实现：取 RS 的 rproctab，逐条写 ACL，再发出生应答。
- **出生应答**：VM 完成 fresh 初始化后回复 RS_INIT 的消息；首次启动必须异步发，避免死锁。
- **`transport`**：Rust 对内核 IPC 原语的 trait 抽象；`initialized` 门表示可以阻塞收消息。
- **`mark_initialized`**：Rust 对 C `__minix_init()` 时刻的显式表达，打开 IPC transport 的就绪门。

---

## 4. 实现详解

第 3 章定下了启动链的职责划分。本章看这些决定在代码里的实际落点：`boot.rs` 承载启动契约，`main.rs` 负责入口路由，`global.rs` 负责全局会计，`vm_server.rs` 负责 `init()` 的阶段顺序与主循环入口；§4.4.4 是 Drop 与测试窗口的边界说明。读完后，读者应能沿着一条真实启动路径，把每个动作对应到具体函数。

### 4.0 本章要回答的问题

读者需要能回答：`read_boot_params()` 之后，`BootRoute` 的四种结果各自意味着什么；`VmServer` 为什么必须先 `install_server` 再 `init()`；`init()` 的 Phase 1/2a/2b/2c/2d 与 C 的 `init_vm()` 如何对应；`run_installed()` 怎样进入主循环。

#### 最小示例

```bash
$ grep -n 'fn real_main\|boot_route\|install_server\|pub fn init\|pub fn run' os/servers/vm/src/main.rs os/servers/vm/src/vm_server.rs
os/servers/vm/src/main.rs:41:fn real_main() -> ! {
os/servers/vm/src/main.rs:81:        match boot_route(params.is_first_time) {
os/servers/vm/src/vm_server.rs:649:    pub fn init(&mut self) {
os/servers/vm/src/vm_server.rs:1693:    pub fn run(&mut self) -> ! {
```

#### 失败对照

如果构造期就注册全局页分配器指针，随后 server 被移入 image slot，指针会继续指向旧的栈上位置，`vm_pt_alloc` 读到悬空地址。当前入口先把 server 装入地址稳定的 image slot，再在 `with_installed_server` 里调用 `init()`（`main.rs:81-100`）；`init()` 开头才对最终地址调用 `register_page_alloc`，`relocate()` 随后原地替换分配器字段 [ARCH: A-5]，消除这条悬空路径。

### 4.1 `os/servers/vm/src/boot.rs`（对应 §3.1、§3.6）

模块的主要实体：

- `BootModule { start_addr, len }`——C `multiboot_module_t`（main.c:485-490）；
- `KernelAllocated { static_bytes, dynamic_bytes }`——C `kernel_allocated_bytes(_dynamic)`（main.c:492-495）；
- `BootParams<'a>`——启动契约（§3.1），含 `simple()`（仅测试）、`validate()`、`extra_pages()`；
- `read_boot_params()`——从内核交接页读出 `BootParams`，是生产入口的唯一构造路径；
- `BootRoute`——入口对 cold/warm/no-state/double-init 四种形态的路由；
- `VM_PROC_NR: i32 = Endpoint::VM.get()`——C `com.h:67`，boot 进程号与槽号统一为 8。

### 4.2 `os/servers/vm/src/main.rs`（对应 §3.2）

```rust
fn real_main() -> ! {
    #[cfg(not(test))]
    {
        use minix_vm::{BootRoute, VmServer, boot_route, install_server,
                       read_boot_params, run_installed, with_installed_server};

        // C: main.c:79-88 / 442-444 — 从内核交接页读启动参数。
        let params = read_boot_params();

        // C: main.c:100-107 — is_first_time 决定是否执行 init_vm()。
        match boot_route(params.is_first_time) {
            BootRoute::ColdInit => {
                let server = VmServer::new_with_boot_params(params);
                install_server(server)
                    .unwrap_or_else(|_| panic!("VM cold boot: image slot already full"));
                with_installed_server(|server| server.init());
            }
            BootRoute::WarmRun => {}
            BootRoute::WarmNoState => panic!("VM warm restart without image state"),
            BootRoute::DoubleInit => panic!("VM cold boot with a full image slot"),
        }

        // C: sef_local_startup() —— RS_INIT 握手在主循环优先级 2 内完成（rs_handshake）
        run_installed();
    }
}
```

`read_boot_params()` 从内核写好的交接页读出根页表、内核信息页、空闲区、boot 进程、模块、内核布局与初始栈顶；`BootRoute` 把 C 的 `is_first_time()` 二值门控展开成四种可诊断的启动形态。`ColdInit` 必须先把 server 装入地址稳定的 image slot，再在槽内执行 `init()`；`run_installed()` 从槽里取回 server 并进入主循环。

### 4.3 `os/servers/vm/src/global.rs`（对应 §3.6）

`global.rs` 提供 `add_total_pages(pages)`（§3.6）；其它全局设施保持既有职责：

- `TOTAL_PAGES` + `init()`——`mem_init` 的 total_pages 语义（§2.2.2）；
- `VM_INSTANCE_COUNT` + `inc_vm_instance`/`dec_vm_instance`/`vm_instance_count`——`num_vm_instances`（main.c:578）；
- `kernel_boot_info.boot_procs[]`（§2.2.6）的唯一真相源是 `VmServer.boot_procs`（`BootParams`，见 §4.4.1），没有另设 `BOOT_INFO` 全局副本；
- `KERNEL_LAYOUT` + `set_kernel_layout`——页表模块的 kernel layout（`08-pagetable-ops.md`）。

### 4.4 `os/servers/vm/src/vm_server.rs`（对应 §3.3-§3.5）

#### 4.4.1 构造：`VmServer::new` / `new_with_boot_params`

`new(total_pages, free_regions)` 保留为测试便利构造（内部委托 `BootParams::simple`）；生产路径走 `new_with_boot_params(params)`：

1. `params.validate()`（§3.1 的 C asserts）；
2. `create_default_allocator(total_pages, free_regions)` + `VmPageAllocator::new`（§2.2.2 的 mem_init）；
3. `init_vm_self_pt()`（非 test，§2.2.3 的 pt_init）；
4. 拷贝 `boot_procs` 到定长数组，计算 `boot_usable_pages = Σ free_regions[i].size / CLICK_SIZE`，并预计算 `boot_extra_pages = params.extra_pages()`。

`global::register_page_alloc` 不在构造期：它在 `init()` 开头、server 已经进入 image slot 之后执行，因此注册的是最终地址。§4.0 的失败对照说的正是“构造期注册、随后移动”这条路径。

#### 4.4.2 初始化：`VmServer::init()`

```rust
pub fn init(&mut self) {
    // C: init_vm() — main.c:428-587；阶段划分与 C 对应，两处完成时机不同（见下）
    crate::global::register_page_alloc(&mut self.ctx.page_alloc);
    #[cfg(not(test))]
    self.relocate();

    // Phase 1: 全局状态（boot 可用页 + kernel layout）
    self.init_global_state();

    // Phase 2a: init_proc(VM_PROC_NR) — main.c:474
    self.init_vm_slot();

    // Phase 2b: mem_add_total_pages 调用点 — main.c:485-495
    self.account_boot_memory();

    // 页帧表在 boot 进程循环之前建立：exec_bootproc 需要从它取段页。
    let total_phys = PhysBytes(self.ctx.page_alloc.total_pages() as u64 * PAGE_SIZE);
    self.ctx.page_frames = Some(PageFrames::new(total_phys));

    // Phase 2c: boot 进程槽 — main.c:498-520
    self.init_boot_procs();

    // Phase 2d: VM 实例标记 — main.c:577-579
    self.mark_vm_instance();

    // C: __minix_init() — 打开 IPC 就绪门，主循环才能阻塞收消息。
    self.transport.borrow_mut().mark_initialized();

    self.initialized = true;
}
```

`kernel_layout` 有两份载体但不是两份事实：`BootParams.kernel_layout` 是构造期传给 `init_vm_self_pt()` 的启动输入；`global::set_kernel_layout()` 把同一个值登记为模块内共享副本，供后续 `init_page_table()` 读取。

A–E 的大阶段顺序不变，但 C 的 `init_proc(VM_PROC_NR)` → `pt_init` 在 Rust 中变成 `init_vm_self_pt()` → `init_vm_slot()`：VM 自身页表先于 VM 槽建立。`init_vm_self_pt()` 只 adopt 内核引导根，不读 `vmprocess` 槽，因此这个对调不改变依赖；读者不要把它读成“阶段顺序完全一致”。

- `pt_init` 的等价 `init_vm_self_pt()` 在 `new_with_boot_params` 构造期完成，因为 VM 自身页表必须先于分配器注册与 `relocate()`。
- `__minix_init` 的等价 `mark_initialized()` 在 `init()` 末尾完成，因为 transport 的就绪门只影响主循环收消息；boot 期的 `sys_exec`/`sys_vmctl` 走 `KernelGateway`，不经过该门。

各阶段实现：

- 全局分配器注册——`init()` 开头对 `&mut self.ctx.page_alloc` 调 `register_page_alloc`；此时 server 已在 image slot 中，`relocate()` 随后原地替换分配器字段，注册指针仍然有效。
- `init_global_state()`——`global::init(boot_usable_pages)` + `global::set_kernel_layout(...)`；总页数用 boot 可用页，而不是位图容量。
- `init_vm_slot()`——从 `boot_procs` 找 `proc_nr == VM_PROC_NR` 的条目，`EmptySlot::activate(endpoint)` + `set_boot(ip)`，等价 C `init_proc`（main.c:262-291）。生产构建找不到条目时直接 panic（对应 C 的 `panic("no init_proc")`）；测试构造省略 `boot_procs`，测试分支保留跳过。
- `account_boot_memory()`——`boot_extra_pages > 0` 时调用 `global::add_total_pages`（§3.6）。
- 页帧表——`PageFrames::new(total_phys)` 必须在 `init_boot_procs()` 之前，因为 `exec_bootproc()` 分配段页时读取它。
- `init_boot_procs()`——对每个非 VM、非负、endpoint 非 NONE 的 boot 条目建槽（等价 `init_proc`），并调用 `exec_bootproc()`；保留 C 的 `assert(start_addr != 0)`。`endpoint == NONE` 在 Rust 交接页里表示空槽，C 的 `boot_procs[]` 不会产生这种条目，因此这是交接表示层的差异，不改变 boot 进程的实际集合。
- `mark_vm_instance()`——对 VM 槽调用 `ActiveProc::mark_vm_instance()`（设置 `VmFlags::VM_INSTANCE` + `global::inc_vm_instance()`，配对 `VmProc::clear()` 的递减）。

#### 4.4.3 主循环与 RS_INIT（对应 §3.4；细节归 15）

`run()`/`run_once()` 经 `self.transport.borrow_mut().receive()/send()` 走 `IpcTransport` trait 对象；通知跳过用 `rcv_sts.is_notify()`；C 的 `missing_spares`/`alloc_cycle` 补页链在 Rust 中没有对应实现（保留页池被 Direct Map 取代，回收在 `alloc_pfn_reclaiming`），`pt_clearmapcache` 无对应物；`dispatch_on_msg` 优先级 2 的 RS_INIT 分支调用 `rs_handshake()`：成功 → `sef_init_reply(OK)` 出生应答后 `Suspend`；失败 → 先回 `sef_init_reply(errno)`，再丢弃请求、计数并审计——两臂都不走 dispatcher 的二次回复腿，等价 C main.c:149-152 的 SUSPEND 语义。测试：`test_run_once_rs_init_handshake_copies_and_maps_service`（成功臂）/ `test_run_once_rs_init_fails_closed_on_safecopy_error`（失败臂）。

#### 4.4.4 Drop 生命周期

`impl Drop for VmServer` 调用 `global::unregister_page_alloc()`，在 server 销毁时清掉全局分配器指针，避免下一个测试实例读到指向前一个实例的地址。

### 4.5 本章回顾

#### 小结

- 入口顺序是 `read_boot_params` → `BootRoute` → `install_server` → `VmServer::init` → `run_installed`。
- `VmServer` 必须先在 image slot 中稳定下来，再注册全局分配器指针并执行 `init()`。
- `init()` 的 Phase 1/2a/2b/2c/2d 按 C `init_vm()` 的阶段划分，两处完成时机不同（§4.4.2），页帧表在 boot 进程循环之前建立。
- `run()` 进入主循环；RS_INIT 在优先级 2 由 `rs_handshake()` 处理，成功与失败两臂都不走 dispatcher 的二次回复。

#### 术语

- **image slot**：存放 `VmServer` 的地址稳定静态槽；server 移动后全局指针仍然有效。
- **`BootRoute`**：`ColdInit`/`WarmRun`/`WarmNoState`/`DoubleInit` 四种入口路由。
- **Phase 2a/2b/2c/2d**：`init()` 中 VM 自身槽、总页数校准、boot 进程槽、VM 实例标记四段。
- **`run_installed`**：从 image slot 取 server 并进入主循环的入口函数。
- **`rs_handshake`**：主循环优先级 2 处理 RS_INIT 的显式函数。

---

## 5. 测试要点

启动链的正确性不能只靠读代码。本章看当前测试覆盖了哪些启动阶段、哪些只是形状检查、哪些仍需要真机门，并给出可复现的基线读数。

### 5.0 本章要回答的问题

读者需要能回答：`boot.rs` 的测试证明了启动契约的哪些性质；`vm_server.rs` 的测试覆盖了 `init()` 的哪些阶段；当前宿主测试的边界在哪里；哪些启动行为只能由真机门证明。

#### 最小示例

```bash
$ cd os && cargo test -p minix-vm --lib
# 当前读数见 §5.3
```

#### 失败对照

如果把历史读数当成当前基线，会把已经转绿的测试误报成缺口，也会掩盖新回归。测试计数必须在当前工作树上重跑，不能引用历史数字。

### 5.1 boot.rs 测试（§3.1、§3.6）

| 测试 | 覆盖 |
|------|------|
| `test_boot_params_simple_validates` | `validate()` 正常路径 |
| `test_boot_params_validate_empty_regions` | C assert（main.c:451）等价 |
| `test_boot_params_validate_total_pages_mismatch` | total_pages 一致性 |
| `test_boot_params_validate_misaligned_root` | 根页表页对齐负控 |
| `test_boot_params_validate_zero_root` | 根页表非零负控 |
| `test_boot_params_extra_pages_modules` | handoff 模块全部计费（C main.c:485-490 的对应形状） |
| `test_boot_params_extra_pages_kernel` | 内核占用校准（main.c:492-495） |
| `test_boot_params_extra_pages_vm_image` | VM 映像不在 handoff 模块列表，按 `vm_allocated_bytes` 单独补记 |
| `test_reconcile_accepts_consistent_record` | A2 扣减记录一致时通过 |
| `test_adopt_free_regions_keeps_every_region_verbatim` | free_regions 原样 adopt，不裁剪 |
| `test_reconcile_rejects_empty_record` | 空扣减记录负控 |
| `test_reconcile_rejects_root_not_in_record` | 根页表不在记录内负控 |
| `test_reconcile_rejects_free_deducted_overlap` | 空闲区与扣减区相交负控 |
| `test_reconcile_rejects_module_not_in_record` | 模块 blob 不在记录内负控 |

### 5.2 vm_server.rs 测试（§3.2-§3.5）

| 测试 | 覆盖 |
|------|------|
| `test_vm_server_init` | `init()` 的基本路径与 `initialized` 标志 |
| `test_vm_server_run_without_init` | 未初始化进入主循环的 fail-fast 行为 |
| `test_init_rejects_missing_kernel_layout` | 缺内核布局的负控 |
| `test_vm_server_init_with_boot_procs` | init 后 VM 槽 + boot 槽激活、VM_INSTANCE 标记（main.c:474,498-520,578-579） |
| `test_vm_server_init_vm_instance_count` | `num_vm_instances` 增量（main.c:578） |
| `test_vm_server_init_accounts_boot_memory` | `mem_add_total_pages` 调用点（main.c:485-490） |
| `test_run_once_rs_init_handshake_copies_and_maps_service` | RS_INIT 成功臂：取 rproctab 并写 ACL |
| `test_run_once_rs_init_fails_closed_on_safecopy_error` | RS_INIT 失败臂：应答错误并丢弃请求 |

其它既有测试（transid 族、页错误计数、进程生命周期等）与本文档范围不直接相关，但同属 `minix-vm` 基线。

测试边界：`exec_bootproc` 在 test 构建中被 `#[cfg(not(test))]` 跳过，所以“页帧表先于 boot 进程循环”这条顺序没有宿主测试，只能由生产路径与真机门覆盖；`mark_initialized` 由 `test_vm_server_init` 的 `initialized` 断言间接覆盖。

### 5.3 当前基线没有失败项

在本次验证的工作树上执行：

```text
$ cd os && cargo test -p minix-vm --lib
test result: ok. 549 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

这是宿主 `--lib` 单一形态的读数。如果出现失败，应按对应模块（`06-page-allocator.md`、`13-region-mapping.md`）的测试边界处理，而不是把测试自身问题当成启动链缺陷。

### 5.4 真机门清单

宿主测试只能证明形状与负控；启动链是否真的走到主循环，要看真机路标。当前 `os/servers/vm/src/main.rs` 提供四段 `e5d:` 诊断输出，可作为门链的观察点：

| 阶段 | 可观测路标 | 代码位置 |
|---|---|---|
| 读到启动参数 | `e5d: boot-params-read` | `main.rs:64` |
| 进入 VM 入口 | `e5d: vm-main-enter` | `main.rs:78` |
| `init()` 完成 | `e5d: vm-init-done` | `main.rs:96` |
| VM 自身页表自测 | `E5D-VM-SELF-PT PASS` / `FAIL` | `main.rs:104-116` |

本篇不声称这些路标已经被一个以 VM 启动链为主题的独立 QEMU 门断言。真机验证的缺口是：宿主 549/0 不能证明 boot 进程 `exec`、RS_INIT 应答与主循环收消息在真实固件上按序发生；这些行为需要在 `os/qemu-tests/` 的启动门里补断言，或明确登记为尚未覆盖的风险。

### 5.5 本章回顾

#### 小结

- `boot.rs` 的测试覆盖 `BootParams::validate()`、`extra_pages()` 与根页表负控。
- `vm_server.rs` 的测试覆盖 `init()` 后的 VM 槽、boot 槽、实例标记与总页数校准。
- 当前宿主基线见 §5.3；计数必须在当前工作树上重跑。
- 宿主测试证明形状与负控，不证明真机上的启动链终点；真机门是独立证据层。
- 测试名称与断言必须能在当前工作树上重放。

#### 术语

- **宿主测试**：在开发机上运行的 `cargo test`，覆盖纯逻辑、形状与负控。
- **负控**：故意构造非法输入、要求代码拒绝或 panic 的测试。
- **真机门**：在 QEMU 或真板上按串口路标判定的启动验证。
- **基线读数**：在当前工作树上重跑得到的 passed/failed 计数。

---

## 6. 过渡

本文档回答了 VM 启动链的第一个问题：**main() → init_vm() → 主循环入口**。

- 启动链的下一步：进程模型——`02-vmproc-struct.md`（`vmproc` 结构语义）、`03-vmproc-table.md`（表查找与 `vm_isokendpt`，主循环 caller 验证依赖它）。
- 访问控制：`04-acl.md`（`acl_init` 的后续）。
- 物理内存：`05-physical-memory.md`（`get_mem_chunks` 的完整语义）、`06-page-allocator.md`（`mem_add_total_pages` 之后的分配器全貌）。
- 主循环分发：`15-ipc-dispatch.md`（`main.c:110-192` 的 5 优先级模型，本文档 §2.1 只到入口）。
- 本文委托出去的落点：`07-pagetable-struct.md`/`08-pagetable-ops.md`（`pt_init` 与 `exec_bootproc` 的页表实现）、`09-vm-boot-protocol.md`（`VMCTL_BOOTINHIBIT_CLEAR` 与内核交接协议）、`25-rs-services.md`（Live Update 状态机与 SEF 特殊映射区登记）。

阶段 02 内的阅读顺序建议：01 → 02 → 03 → 04 → 05 → 06 → 07 → 08 → 09 → 10 → 11 → 12 → 13 → 14 → 15；`25-rs-services.md` 随 Live Update 专题阅读。读 01 之前先完成文首两篇前置（`00-vm-overview.md` 与 `01-stage-kernel/09-vm-boot-protocol.md`）。

## 7. 参见

- `rewrite-notes/02-stage-vm/00-vm-overview.md` — VM 总览与启动主线图
- `rewrite-notes/02-stage-vm/plan.md` §1.2 — 启动时序主线
- `rewrite-notes/01-stage-kernel/09-vm-boot-protocol.md` — 内核侧 VMCTL 协议（`VMCTL_BOOTINHIBIT_CLEAR` 等）
- `minix3/minix/servers/vm/main.c` — 本文档 ground truth
- `minix3/minix/lib/libsys/sef.c`、`sef_init.c` — SEF 框架实现
- `minix3/minix/servers/vm/utility.c:mmap` — VM 自身 libc 接口
