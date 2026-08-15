# 01-vm-init-main: 启动入口与初始化骨架

> **分类**: 阶段 1 — 启动入口与进程模型（锚点文档）
> **源码**: `minix3/minix/servers/vm/main.c`（`get_mem_chunks` 定义于 `minix3/minix/servers/vm/utility.c:44`，`mem_add_total_pages`/`mem_init` 定义于 `minix3/minix/servers/vm/alloc.c`；SEF 库位于 `minix3/minix/lib/libsys/sef*.c`）
> **Rust 模块**: `os/servers/vm/src/main.rs`、`os/servers/vm/src/boot.rs`、`os/servers/vm/src/global.rs`、`os/servers/vm/src/vm_server.rs`（`VmServer::new_with_boot_params`/`init`/`run`）
> **前置**: `notes/rewrite/fork-syscall-rewrite/02-stage-vm/00-vm-overview.md`、`notes/rewrite/fork-syscall-rewrite/01-stage-kernel/09-vm-boot-protocol.md`
> **说明**: VM 进程从 `main()` 入口到进入主循环之前的全部启动链：`is_first_time()` 门控、`init_vm()` 各步骤、SEF 生命周期、boot 进程地址空间、`map_service`、VM 自身内存边界。主循环消息分发细节在 `15-ipc-dispatch.md`。

---

## 1. 概念：启动链——VM 如何从"内核给的临时环境"过渡到"自己管理一切"

### 1.0 章节引言

本章建立 VM 启动链的概念模型：一个**先有鸡还是先有蛋**的问题——VM 是内存管理器，但它自己运行也需要内存；它要管理进程表，但它自己也是进程表里的一项。

> **本章不讲什么**:
> - `vmproc` 结构体字段与槽位语义（`02-vmproc-struct.md`）
> - 进程表查找与 endpoint 验证（`03-vmproc-table.md`）
> - ACL 数据结构与权限位（`04-acl.md`）
> - 物理内存布局解析与分配器（`05-physical-memory.md`）
> - 页表结构、保留页池、Direct Map（`07-pagetable-struct.md`、`08-pagetable-ops.md`）
> - 主循环的 5 优先级分发细节（`15-ipc-dispatch.md`）
>
> 本章只回答一个问题：**main() 到主循环之间，VM 按什么顺序、为什么按这个顺序，把自己初始化好**。

### 1.1 启动链的核心问题：VM 的"鸡生蛋"

VM 是系统中第一个获得完整页表所有权的用户态服务，但它启动时只有内核提供的 bootstrap 环境。它面临一个循环依赖：

```
VM 需要内存管理能力来运行 → 内存管理是 VM 的职责 → VM 必须先初始化自己
```

Minix3 的解法是**分阶段初始化**：每个阶段只依赖前一阶段已经建立的设施。

1. **阶段 A（零依赖）**：只用内核提供的静态数据——`sys_getkinfo()` 拿到的 `kernel_boot_info`（内存布局、boot 进程表、模块表）。
2. **阶段 B（不需要堆）**：解析物理内存布局、清空进程表、初始化 ACL 与区域管理。
3. **阶段 C（需要物理页，用保留页池）**：建立物理内存分配器、VM 自身进程槽与页表。
4. **阶段 D（堆可用）**：`__minix_init()` 之后堆可用，进行总页数校准、boot 进程地址空间建立。
5. **阶段 E（可运行）**：注册 CALLMAP、标记 VM 实例、SEF 启动，进入主循环。

这个依赖链是本文档后半部分所有 C 源码分析的骨架。

### 1.2 初始化依赖链图

`init_vm()` 的完整顺序（`main.c:428-587`）：

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
  │          ├─ CALLMAP 注册              main.c:522-573
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

**阅读顺序即执行顺序**：本文档按此图自上而下展开，每一步回答"它在启动时序中的位置"。

### 1.3 SEF 生命周期是什么

SEF（System Event Framework，`minix3/minix/lib/libsys/sef.c`）是 Minix3 服务端共享的启动/热更新框架。VM 通过注册回调接入 SEF：

| SEF 回调 | 注册时机 | VM 的处理 |
|---------|---------|----------|
| `sef_cb_init_fresh` | `sef_local_startup` | `map_service()` 批量注册 boot 服务 ACL |
| `sef_cb_init_lu_restart` | 同上（init_lu + init_restart） | Live Update/重启状态恢复 |
| `sef_cb_lu_state_changed` | 同上 | LU 失败后恢复页表绑定 |
| `sef_cb_init_vm_multi_lu` | LU 流程内 | 多组件更新 IPC 过滤 |
| `sef_cb_signal_handler` | 同上 | `SIGKMEM` → `do_memory()` |
| `sef_cb_init_response_rs_asyn_once` | 首次启动临时设置 | 首条 RS_INIT 回复异步发送（避免启动死锁） |

SEF 的核心价值在 C 中是"统一的服务生命周期协议"；Rust 侧将其简化为显式函数（见 §3.4）。

---

## 2. C 源码分析

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

**语义**：RS 在系统启动时持有 `RTS_BOOTINHIBIT`；VM 是第一个运行的服务器，此时 RS 尚未被解除启动抑制 → `is_first_time()` 为真 → 执行完整 `init_vm()`。若 VM 因崩溃/Live Update 重启，RS 已解除抑制 → 跳过 `init_vm()`，直接走 SEF 的 LU/restart 状态恢复路径。

`__vm_init_fresh` 是 VM 与 SEF 之间的标志：为 1 时 `sef_local_startup()` 将 RS_INIT 回复设置为异步模式（`sef_cb_init_response_rs_asyn_once`，见 §2.4.3），避免启动期死锁；`sef_startup()` 返回后清 0（main.c:107）。

主循环本体（main.c:110-192）的语义归属 `15-ipc-dispatch.md`；本文档只强调入口结构：`sef_receive_status(ANY)` 收消息 → `is_ipc_notify` 过滤 → `vm_isokendpt` 验证 caller → 分发 → `SUSPEND` 抑制回复。

### 2.2 init_vm()：完整初始化链（main.c:428-587）

`init_vm()` 是本文档的锚点函数。逐段分析：

#### 2.2.1 启动参数与 sanity（main.c:442-452）

```c
if(OK != (s=sys_getkinfo(&kernel_boot_info))) {          /* 442-444 */
	panic("couldn't get bootinfo: %d", s);
}

/* Turn file mmap on? */
enable_filemap=1;	/* yes by default */                   /* 447-448 */
env_parse("filemap", "d", 0, &enable_filemap, 0, 1);

/* Sanity check */
assert(kernel_boot_info.mmap_size > 0);                   /* 451 */
assert(kernel_boot_info.mods_with_kernel > 0);            /* 452 */
```

- `kernel_boot_info`（`minix3/minix/include/minix/param.h`，`kinfo_t`）是内核→VM 的启动契约：内存映射、boot 进程表、模块表、内核自身占用字节。
- 两个 assert 是启动协议的完整性检查：内存映射非空、模块表非空（`mods_with_kernel` 含内核自身）。
- `enable_filemap` 默认开启文件 mmap，可用 `filemap` 环境变量关闭（mmap 语义归 `20-vm-mmap.md`）。

#### 2.2.2 内存布局与基础结构（main.c:455-471）

```c
get_mem_chunks(mem_chunks);    /* 455 — utility.c:44 */

/* Set table to 0. This invalidates all slots (clear VMF_INUSE). */
memset(vmproc, 0, sizeof(vmproc));                        /* 458 */

for(i = 0; i < ELEMENTS(vmproc); i++) {
	vmproc[i].vm_slot = i;                                /* 460-462 */
}

acl_init();                  /* 465 */
map_region_init();           /* 468 */
mem_init(mem_chunks);        /* 471 — alloc.c:306 */
```

- `get_mem_chunks()`（utility.c:44-79）：把 `kernel_boot_info.memmap[]` 的字节偏移/长度转换为 click（4K）对齐的 `struct memory` 块，起始向上取整、结束向下取整。
- `memset(vmproc, 0, ...)` + `vm_slot = i`：进程表全清零（清掉所有 `VMF_INUSE`），并写入槽号。`vmproc` 结构语义归 `02-vmproc-struct.md`，查找语义归 `03-vmproc-table.md`。
- `acl_init()`：ACL 数据结构初始化（`04-acl.md`）。
- `map_region_init()`：区域管理初始化（`13-region-mapping.md`）。
- `mem_init(mem_chunks)`（alloc.c:306-334）：用内存块建立空闲页位图，同时累加 `total_pages = Σ chunks[i].size`（alloc.c:319-331）。物理内存分配器语义归 `05-physical-memory.md`、`06-page-allocator.md`。

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

`__minix_init()` 是 libc 提供的函数：VM 在确定内核映射后，才能取得之前不可用的内核 IPC 向量（`SYS_*` 调用号映射）。Rust 侧对应 `minix-sys` 的系统调用库（当前 stub，见 §3.3 的 DEFERRED 表）。

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

**这是本文档语义单元中的关键调用点**。`mem_add_total_pages()`（alloc.c:281-284）只做一件事：

```c
void mem_add_total_pages(int pages)
{
	total_pages += pages;
}
```

语义：`mem_init()` 时 `total_pages` 只统计了空闲块；内核的空闲链表**不含** boot 模块（它们是已占用的 blob），所以分配器需要知道真实物理内存更大。校准分两笔：

1. **模块表**（main.c:485-490）：每个模块 `mod_end - mod_start + 1` 向上取整到页后计入。注意循环上界是 `mods_with_kernel - 1`——**最后一个条目被刻意排除**：它是内核自身的伪模块条目（`pre_init.c:198-200` 把内核登记为 `module_list[kern_mod]`，`mods_with_kernel = mi_mods_count + 1`），其占用在下一笔（内核自身）单独收费，避免重复计入（这是 C 的既有行为，Rust 忠实复刻，见 §3.6）。
2. **内核自身占用**（main.c:492-495）：`kernel_allocated_bytes`（静态）向上取整 + `kernel_allocated_bytes_dynamic`（动态，内核已按页对齐）后计入。

#### 2.2.6 boot 进程循环（main.c:498-520）

```c
for (ip = &kernel_boot_info.boot_procs[0];
	ip < &kernel_boot_info.boot_procs[NR_BOOT_PROCS]; ip++) {
	struct vmproc *vmp;

	if(ip->proc_nr < 0) continue;      /* 502 — 跳过内核任务 */

	assert(ip->start_addr);            /* 504 */

	/* VM has already been set up by the kernel and pt_init().
	 * Any other boot process is already in memory and is set up
	 * here.
	 */
	if(ip->proc_nr == VM_PROC_NR) continue;   /* 510 */

	vmp = init_proc(ip->proc_nr);      /* 512 */
	exec_bootproc(vmp, ip);            /* 514 — 见 §2.3.2 */

	/* Free the file blob */
	assert(!(ip->start_addr % VM_PAGE_SIZE));  /* 517 */
	ip->len = roundup(ip->len, VM_PAGE_SIZE);  /* 518 */
	free_mem(ABS2CLICK(ip->start_addr), ABS2CLICK(ip->len));  /* 519 */
}
```

- 每个非 VM 的 boot 进程（PM/VFS/RS/DS/...）先建槽（`init_proc`），再通过 `exec_bootproc` 从内存中的 ELF blob 建立地址空间并启动，最后把 blob 占用的物理内存归还分配器。
- VM 自身已由内核建好页表（`pt_init()` 处理），这里跳过。

#### 2.2.7 CALLMAP 注册（main.c:522-573）

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
```

- `vm_calls[]` 是 `NR_VM_CALLS`（`minix3/minix/include/minix/com.h:769`，值为 **49**）项的函数指针表；`CALLNUMBER(c)`（main.c:54-58）把调用号从 `VM_RQ_BASE` 归一到零基下标，越界返回 -1。
- 分发语义（5 优先级 + ACL）归 `15-ipc-dispatch.md`；本文档只记录"注册发生在 init_vm 尾部"。

#### 2.2.8 VM 实例标记（main.c:577-586）

```c
/* Mark VM instances. */
num_vm_instances = 1;                              /* 578 */
vmproc[VM_PROC_NR].vm_flags |= VMF_VM_INSTANCE;    /* 579 */

/* Let SEF know about VM mmapped regions. */
s = sef_llvm_add_special_mem_region((void*)VM_OWN_HEAPBASE,
    VM_OWN_MMAPTOP-VM_OWN_HEAPBASE, "%MMAP_ALL");  /* 582-583 */
```

- `num_vm_instances = 1`：Live Update 场景下区分新旧 VM 实例的计数。
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
3. **libexec 装载**：填充 `struct exec_info` 并注册四个回调（main.c:362-381）：
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

`sef_startup()`（`minix3/minix/lib/libsys/sef.c`）对 VM 的特殊路径：由于 `__vm_init_fresh`，VM 跳过"等待 RS_INIT"阶段（sef.c 注释："VM handles fresh initialization by RS later"）——RS_INIT 会在主循环中到达并交给 `do_sef_init_request`。

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

#### 2.4.3 RS_INIT 握手与异步回复（sef_init.c:193-217、471-481）

`do_sef_init_request`（sef_init.c:193-217）解析 RS_INIT 消息 → `process_init(type, &info)`（sef_init.c:43-145）→ 回调 `sef_cb_init_fresh` → 构造 RS_INIT 回复 → `sef_cb_init_response`。

```c
int sef_cb_init_response_rs_asyn_once(message *m_ptr)   /* sef_init.c:471-481 */
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

**为什么异步**：启动时 RS 也在等待 VM 完成初始化；若 VM 用同步 `sendrec` 回复，双方互相等待形成死锁。异步回复（`asynsend3`）打破环，且只对**首次** fresh init 生效（`__vm_init_fresh` 门控）。

主循环中的 RS_INIT 分支（main.c:149-152）：`do_sef_init_request(&msg)` 返回 OK 后 `result = SUSPEND`——**不二次回复**（回复已由 SEF 回调完成）。

#### 2.4.4 LU/restart 回调（main.c:196-217、592-730）

- `sef_cb_lu_state_changed`（main.c:196-217）：LU 失败回退到 `SEF_LU_STATE_NULL` 时，重新绑定页表、清 map cache、调整进程引用。
- `sef_cb_init_vm_multi_lu`（main.c:592-676）：多组件更新时构建 IPC 白名单过滤（只允许 RS/正在更新的服务的 `VM_BRK`/`VM_INFO`），并对 `SF_VM_UPDATE` 服务执行 `do_rs_update`。
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

---

## 3. Rust 设计决策

> 每个决策必须可追溯到 §2 的 C 源码分析。Rust 代码位置只以函数/模块名引用（避免行号漂移）。

### 3.1 启动契约显式化：BootParams（对应 §2.2.1）

**C**：`kernel_boot_info` 是 BSS 全局，任何函数可隐式读取。**Rust**：内核→VM 启动协议是一次性、单向的握手（`01-stage-kernel/09-vm-boot-protocol.md`），因此建模为显式构造输入：

```rust
pub struct BootParams<'a> {
    pub total_pages: usize,                    // C: total_pages（mem_init 累加）
    pub free_regions: &'a [BootMemRegion],     // C: mem_chunks[]（get_mem_chunks）
    pub boot_procs: &'a [BootImage],           // C: kernel_boot_info.boot_procs[]
    pub modules: &'a [BootModule],             // C: kernel_boot_info.module_list[]
    pub kernel_allocated: KernelAllocated,     // C: kernel_allocated_bytes(_dynamic)
    pub is_first_time: bool,                   // C: is_first_time()（RTS_BOOTINHIBIT）
}
```

理由：
- **可见性**：构造参数显式暴露启动依赖，`VmServer::new_with_boot_params` 的签名即文档。
- **可测性**：测试用 `BootParams::simple()` 构造单区域、无模块的参数，无需伪造全局。
- **与 C 的差异（ARCH 注记）**：C 的 `kernel_boot_info` 还包含 `user_sp`/`kernel_layout` 等字段；Rust 将其按职责拆分——`BootParams` 归启动链，`KernelLayout`（`minix-types/src/types/boot.rs`）归页表模块（`08-pagetable-ops.md`），由 `global::set_kernel_layout` 单独注入。

`validate()` 复刻 C 的 sanity 检查（main.c:451-452）：空闲区域非空 + 对齐 + `total_pages == Σ 区域页数`。C 的 `assert(mods_with_kernel > 0)` 在 Rust 中**不强制**：单元测试构造无模块参数；生产 boot 协议保证模块表非空（已在 `boot.rs` 文档注释中说明）。

### 3.2 is_first_time 的表达与 init 门控（对应 §2.1）

**C**：`is_first_time()` 通过 `sys_getproc(RS_PROC_NR)` 读内核进程表。**Rust**：该查询发生在内核侧（RTS 标志属内核状态），VM 侧只需知道结果——因此 `BootParams::is_first_time` 携带该布尔值，`main.rs` 复刻 C 的门控：

```rust
// C: main.c:100-102 — if(is_first_time()) { init_vm(); __vm_init_fresh=1; }
if params.is_first_time {
    server.init();
}
```

`__vm_init_fresh`（异步 RS_INIT 回复门控）在 Rust 中**暂不需要独立字段**：RS_INIT 回复路径本身 DEFERRED（§3.4），等回复实现落地时再引入。

### 3.3 init() 分阶段与 C 的逐行对应（对应 §2.2）

`VmServer::init()` 的注释块即 C 的步骤映射表。核心结论：

| C 操作 | Rust 等价 | 何时完成 |
|--------|----------|---------|
| `sys_getkinfo` + asserts（main.c:442-452） | `BootParams::validate()` | `new_with_boot_params` |
| `get_mem_chunks` → `mem_init`（main.c:455,471） | `create_default_allocator` + `VmPageAllocator::new` | `new_with_boot_params` |
| `memset(vmproc,0)` + `vm_slot=i`（main.c:458-462） | 编译期 `[AssumeSyncCell::new(VmProc::vacant()); N]`；`get_empty()` 写 `vm_slot` | 编译期 / 槽激活时 |
| `acl_init()`（main.c:465） | `AclState::Uninitialized` 编译期默认 | 编译期 |
| `map_region_init()`（main.c:468） | `RegionMap::new()`（惰性，每进程初始化时） | `init_regions()` 时 |
| `init_proc(VM_PROC_NR)`（main.c:474） | `VmServer::init_vm_slot()` → `EmptySlot::activate` + `set_boot` | `init()` Phase 2a |
| `pt_init()`（main.c:475） | `init_vm_self_pt()` | `new_with_boot_params`（非 test） |
| `__minix_init()`（main.c:480） | **DEFERRED**（`minix-sys` 系统调用库为 stub） | — |
| `mem_add_total_pages`（main.c:485-495） | `VmServer::account_boot_memory()` → `global::add_total_pages` | `init()` Phase 2b |
| boot 进程循环（main.c:498-520） | `VmServer::init_boot_procs()`（`exec_bootproc`/`free_mem` DEFERRED） | `init()` Phase 2c |
| CALLMAP（main.c:522-573） | `MessageDispatcher::dispatch_by_number` 编译时 match | 编译期 |
| VM 实例标记（main.c:577-579） | `VmServer::mark_vm_instance()` → `ActiveProc::mark_vm_instance` | `init()` Phase 2d |

**DEFERRED 判定**：`__minix_init`（内核 IPC 向量）与 `exec_bootproc`（ELF 装载 + `sys_exec`/`sys_vmctl`）依赖 `minix-sys` 的内核 IPC 原语，后者尚未实现（`os/libs/minix-sys/src/lib.rs` 标注 stub）。这两步的**槽位建立**不依赖内核 IPC（`init_proc` 等价物已完成），因此先落地槽位、延迟"装载与启动"，避免把进程表留给空壳。

### 3.4 SEF 简化：rs_handshake + do_sef_init_request（对应 §2.4）

SEF 在 C 中解决 3 个问题，Rust 各有更简单的替代：

| SEF 组件 | C 中的必要性 | Rust 下的处理 |
|---------|-------------|--------------|
| `sef_startup()` + `sef_cb_init_fresh`（main.c:241-260） | C 没有标准服务初始化协议，必须通过 IPC 从 RS 取 rproctab | 保留协议、去掉框架：`VmServer::rs_handshake()`——`ipc_call_rs_init()` 取 rproctab → 逐条 `acl_set`（等价 `map_service`） |
| `do_sef_init_request` + `sef_cb_init_response`（sef_init.c:193-217） | 主循环 RS_INIT 分支 | 主循环优先级 2 直接调 `rs_handshake()`，返回 `DispatchAction::Suspend`（不回复） |
| `sef_cb_init_response_rs_asyn_once`（sef_init.c:471-481） | 避免启动死锁 | **DEFERRED**：RS_INIT 回复本身依赖 asynsend 原语（内核 IPC），落地时引入 |
| `sef_cb_init_lu_restart` / `sef_cb_lu_state_changed` / `sef_cb_init_vm_multi_lu`（main.c:196-217,592-730） | Live Update 状态机 | **DEFERRED**：`rs.rs` 的 `handle_rs_prepare`/`handle_rs_update` 已预留入口；swap_proc_slot 等 LU 语义归 `25-rs-services.md` |
| `sef_cb_signal_handler`（main.c:731-754） | 信号经 IPC 通知送达 | 主循环 `is_ipc_notify` 分支识别通知；`SIGKMEM → do_memory` 归 `06-page-allocator.md`，当前 DEFERRED（通知分支直接 continue） |

**结论**：`rs_handshake()`（约 30 行）替代 SEF 的 setcb 注册 + startup 状态机（约 60 行），协议语义不变，框架复杂度消除。

### 3.5 exec_bootproc 的 Rust 策略（对应 §2.3.2）

`exec_bootproc` 的五个子步骤在 Rust 中的归属：

| 子步骤 | Rust 状态 | 归属 |
|--------|----------|------|
| `pt_new` + `pt_bind` | DEFERRED（Paging trait 已定义，`CurrentPaging` 实现在 arch） | `08-pagetable-ops.md` |
| ELF 头读取 + `libexec_load_elf` | DEFERRED（`minix-elf` crate 已提供解析器，装载接入待定） | `01-stage-kernel` ELF 装载 |
| 栈建立（`minix_stack_*`） | DEFERRED | 10 期（switch-to-user） |
| `sys_exec` + `VMCTL_BOOTINHIBIT_CLEAR` | DEFERRED（`minix-sys` stub） | `09-vm-boot-protocol.md`（内核侧已实现 `do_vmctl`） |
| 槽位建立（`init_proc` 等价） | **已实现**：`VmServer::init_boot_procs` | 本文档 §4.4 |

**设计原则**：先把"进程表反映 boot 镜像"这一步落地（主循环运行的前提），把"装载与启动"延迟到内核 IPC 可用时。`init_boot_procs` 保留 C 的两个保护：跳过负 `proc_nr`（内核任务）、`assert(start_addr != 0)`。

### 3.6 mem_add_total_pages → global::add_total_pages（对应 §2.2.5）

**C**：`mem_add_total_pages`（alloc.c:281-284）直接改全局 `total_pages`。**Rust**：

```rust
pub(crate) unsafe fn add_total_pages(pages: usize) {
    unsafe { *TOTAL_PAGES.get() += pages; }
}
```

调用点封装在 `BootParams::extra_pages()`：

```rust
// C: main.c:485-490 — 模块循环（上界排除最后一个条目，忠实复刻）
let charged_modules = self.modules.len().saturating_sub(1);
for m in &self.modules[..charged_modules] { ... }

// C: main.c:492-495 — 内核占用（static 向上取整 + dynamic）
```

**为什么不直接改分配器**：`total_pages` 是"真实物理内存总页数"（统计/查询用，`26-vm-queries.md` 的 `do_info` 读取它），分配器的空闲页池是"可分配页数"。C 两者分离，Rust 保持同一模型：`VmPageAllocator::total_pages()` 管空闲池，`global::total_pages()` 管真实总量。

### 3.7 VM 自身内存：无 libc mmap（对应 §2.6，ARCH 注记）

C 的 utility.c 为 VM 自身 libc 实现 `mmap`/`munmap`/`_brk`（§2.6）。Rust 的 `no_std` VM 不使用 libc 堆接口，自身内存来源是 `VmAllocator`（`global.rs`，实现 `GlobalAlloc`）：通过 `HEAP_ARENA` 从直接映射区申请页并 bump 分配。

**这是 ARCH 差异，标注三处一致**：
- 本文档（§3.7）：C 用 `_brk` 逐页映射自身堆；Rust 用 `GlobalAlloc` bump 分配器。
- design：`global.rs` 的 `VmAllocator` 设计文档。
- 代码：`global.rs` 的 `VmAllocator` 注释（"bump allocator ... intentional for a long-lived system service"）。

语义等价点：C 的 `_brk` 页映射 + `pt_writemap` 与 Rust 的 `heap_arena_grow` 都解决"VM 自身需要可写内存"；C 的 `munmap` 显式归还与 Rust 的 no-op `dealloc`（进程生命周期内不回收）是行为差异，但 VM 是常驻服务、分配量稳定，二者在可观测行为上等价。

### 3.8 CALLMAP → 编译时 match（对应 §2.2.7）

C 用运行时函数指针表（`memset` + 逐个 `CALLMAP`），因为 C 没有编译时分发。Rust 的 `MessageDispatcher::dispatch_by_number` 用 `match` 在编译期解析全部 49 个调用号（`NR_VM_CALLS` 已对齐 C 的 `com.h:769`），零成本跳转表 + 类型安全。分发细节归 `15-ipc-dispatch.md`。

---

## 4. 实现详解

> 每个实现对应 §3 的设计决策。Rust 代码位置以函数名引用。

### 4.1 `os/servers/vm/src/boot.rs`（对应 §3.1、§3.6）

模块导出四个实体：

- `BootModule { start_addr, len }`——C `multiboot_module_t`（main.c:485-490）；
- `KernelAllocated { static_bytes, dynamic_bytes }`——C `kernel_allocated_bytes(_dynamic)`（main.c:492-495）；
- `BootParams<'a>`——启动契约（§3.1），含 `simple()`（测试）、`placeholder()`（生产入口占位）、`validate()`、`extra_pages()`；
- `VM_PROC_NR: i32 = Endpoint::VM.get()`——C `com.h:67`，boot 进程号与槽号统一为 8。

`VM_BOOT_IMAGE` 常量（`proc_nr=8`、`endpoint=Endpoint::VM`）供 `placeholder()` 使用，保证生产二进制有合法的 VM 槽。

### 4.2 `os/servers/vm/src/main.rs`（对应 §3.2）

```rust
fn main() {
    #[cfg(not(test))]
    {
        use minix_vm::{BootParams, VmServer};

        // C: is_first_time()（main.c:79-88）—— 占位值，sys_getkinfo 落地前使用
        let params = BootParams::placeholder();

        let mut server = VmServer::new_with_boot_params(params);

        // C: main.c:100-102 —— is_first_time() → init_vm()
        if params.is_first_time {
            server.init();
        }

        // C: sef_local_startup() —— RS_INIT 握手在主循环优先级 2 内完成（rs_handshake）
        server.run();
    }
}
```

`BootParams::placeholder()` 的数值（65536 页、基址 `0x100000`）与旧 main.rs 的 mock 一致，`minix-sys::sys_getkinfo` 落地后由 boot 协议填充替换。`is_first_time=false`（LU/restart）时 `run()` 的 `assert!(initialized)` 会触发——这正是"LU/restart 未实现"的显式失败（fail-fast），而非静默错误。

### 4.3 `os/servers/vm/src/global.rs`（对应 §3.6）

新增 `add_total_pages(pages)`（§3.6）。既有设施保持不变：

- `TOTAL_PAGES` + `init()`——`mem_init` 的 total_pages 语义（§2.2.2）；
- `BOOT_INFO` + `find_boot_image`/`set_boot_image`——`kernel_boot_info.boot_procs[]`（§2.2.6）；
- `VM_INSTANCE_COUNT` + `inc_vm_instance`/`dec_vm_instance`/`vm_instance_count`——`num_vm_instances`（main.c:578）；
- `KERNEL_LAYOUT` + `set_kernel_layout`——页表模块的 kernel layout（`08-pagetable-ops.md`）。

### 4.4 `os/servers/vm/src/vm_server.rs`（对应 §3.3-§3.5）

#### 4.4.1 构造：`VmServer::new` / `new_with_boot_params`

`new(total_pages, free_regions)` 保留为测试便利构造（内部委托 `BootParams::simple`）；生产路径走 `new_with_boot_params(params)`：

1. `params.validate()`（§3.1 的 C asserts）；
2. `create_default_allocator(total_pages, free_regions)` + `VmPageAllocator::new`（§2.2.2 的 mem_init）；
3. `global::register_page_alloc`（VM 自身堆的 GlobalAlloc 接线，§3.7）；
4. `init_vm_self_pt()`（非 test，§2.2.3 的 pt_init）；
5. 拷贝 `boot_procs` 到定长数组 + 预计算 `boot_extra_pages = params.extra_pages()`。

#### 4.4.2 初始化：`VmServer::init()`

```rust
pub fn init(&mut self) {
    // C: init_vm() — main.c:428-557，步骤顺序与 C 完全一致
    #[cfg(not(test))]
    self.relocate();

    // Phase 1: 全局状态（mem_init 的 total_pages + kernel layout）
    self.init_global_state();

    // Phase 2a: init_proc(VM_PROC_NR) — main.c:474
    self.init_vm_slot();

    // Phase 2b: mem_add_total_pages 调用点 — main.c:485-495
    self.account_boot_memory();

    // Phase 2c: boot 进程槽 — main.c:498-520（exec_bootproc DEFERRED）
    self.init_boot_procs();

    // Phase 2d: VM 实例标记 — main.c:577-579
    self.mark_vm_instance();

    // Phase 3: PageFrames（需要 total_pages 已知）
    ...
    self.initialized = true;
}
```

各阶段实现：

- `init_global_state()`——`global::init(total_pages)` + `global::set_kernel_layout(...)`（layout 当前为 mock 值，boot 协议落地后替换，代码注释已标注）。
- `init_vm_slot()`——从 `boot_procs` 找 `proc_nr == VM_PROC_NR` 的条目，`EmptySlot::activate(endpoint)` + `set_boot(ip)`，等价 C `init_proc`（main.c:262-291）。
- `account_boot_memory()`——`boot_extra_pages > 0` 时调用 `global::add_total_pages`（§3.6）。
- `init_boot_procs()`——对每个非 VM、非负、endpoint 非 NONE 的 boot 条目建槽（等价 `init_proc`）；保留 C 的 `assert(start_addr != 0)`。
- `mark_vm_instance()`——对 VM 槽调用 `ActiveProc::mark_vm_instance()`（设置 `VmFlags::VM_INSTANCE` + `global::inc_vm_instance()`，配对 `VmProc::clear()` 的递减）。

#### 4.4.3 主循环与 RS_INIT（对应 §3.4；细节归 15）

`run()` 的 `is_ipc_notify` 分支、`missing_spares` 检查、`ipc_receive`/`ipc_send` 封装保持既有实现；`dispatch_on_msg` 优先级 2 的 RS_INIT 分支调用 `rs_handshake()` 并返回 `DispatchAction::Suspend`（不回复，等价 C main.c:149-152 的 SUSPEND 语义）。

#### 4.4.4 Drop 与测试基础设施修复

- `impl Drop for VmServer`：调用 `global::unregister_page_alloc()`，兑现 `global.rs` 文档承诺的"drop 时清理全局分配器指针"契约。这是 15 个 pre-existing 测试失败的根因修复（测试并行构造 VmServer 时指针互踩）。
- `direct_map.rs::with_custom_mock_base`/`with_mock_base_lock`：panic（含 `#[should_panic]` 测试）时仍恢复 mock base，修复 mock base 泄漏污染后续分配器测试。

### 4.5 端点常量修正（对应 §2.1 主循环）

`vm_server.rs` 的 `VFS_PROC_NR`/`RS_PROC_NR` 原为 `Endpoint(2)`/`Endpoint(1)`（互换），已修正为 `Endpoint::VFS`（1）/`Endpoint::RS`（2），与 C `com.h:60-61` 及 `minix-types` 常量一致。`dispatcher.rs` 的 `dispatch_procctl` 权限检查同步改用 `Endpoint::RS`/`Endpoint::VFS` 常量（原硬编码 `0/1` 会把 PM 误判为允许、把 RS 误判为拒绝）。

---

## 5. 测试要点

> 基线：`cargo test -p minix-vm` = **343 passed / 3 failed**（3 个失败为 pre-existing，见下）。新增测试 9 个（`boot.rs` 6 + `vm_server.rs` 3）。

### 5.1 boot.rs 测试（§3.1、§3.6）

| 测试 | 覆盖 |
|------|------|
| `test_boot_params_simple_validates` | `validate()` 正常路径 |
| `test_boot_params_validate_empty_regions` | C assert（main.c:451）等价 |
| `test_boot_params_validate_total_pages_mismatch` | total_pages 一致性 |
| `test_boot_params_extra_pages_modules` | C 模块循环 + 末条目排除（main.c:485-490） |
| `test_boot_params_extra_pages_kernel` | 内核占用校准（main.c:492-495） |
| `test_boot_params_placeholder_has_vm_slot` | placeholder 含合法 VM 槽 |

### 5.2 vm_server.rs 测试（§3.2-§3.5）

| 测试 | 覆盖 |
|------|------|
| `test_vm_server_init_with_boot_procs` | init 后 VM 槽 + boot 槽激活、VM_INSTANCE 标记（main.c:474,498-520,578-579） |
| `test_vm_server_init_vm_instance_count` | `num_vm_instances` 增量（main.c:578） |
| `test_vm_server_init_accounts_boot_memory` | `mem_add_total_pages` 调用点（main.c:485-490） |

既有测试（`test_vm_server_new`/`init`/`run_without_init`/transid 族等）经 Drop 修复后全部转绿。

### 5.3 pre-existing 失败（与本文档范围无关）

3 个失败均为测试自身问题，非启动链代码缺陷，串行/并行一致复现：

- `alloc_page::tests::test_alloc_page`、`test_alloc_pages_multi`——断言 `vaddr - paddr == VM_DIRECT_MAP_BASE`，但套件使用堆泄露缓冲作为 mock base（`alloc_page.rs` 的 `ALLOC_MOCK_BASE`），断言与自身基础设施矛盾（`06-page-allocator.md` 范围）。
- `region::vir_region::tests::test_map_lazy`——依赖默认 mock base 的测试在堆 base 下 `unwrap()` 失败（`13-region-mapping.md` 范围）。

修复方向（backlog）：上述测试应改为断言"等于当前 mock base"，而非硬编码默认值。

---

## 6. 过渡

本文档回答了 VM 启动链的第一个问题：**main() → init_vm() → 主循环入口**。

- 启动链的下一步：进程模型——`02-vmproc-struct.md`（`vmproc` 结构语义）、`03-vmproc-table.md`（表查找与 `vm_isokendpt`，主循环 caller 验证依赖它）。
- 物理内存：`04-acl.md`（`acl_init` 的后续）、`05-physical-memory.md`（`get_mem_chunks` 的完整语义）、`06-page-allocator.md`（`mem_add_total_pages` 之后的分配器全貌）。
- 主循环分发：`15-ipc-dispatch.md`（`main.c:110-192` 的 5 优先级模型，本文档 §2.1 只到入口）。

阅读顺序建议：01 → 02 → 03 → 04 → 05 → 06 → 07 → 08 → 09 → 10 → 11 → 12 → 13 → 14 → 15。

## 7. 参见

- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/00-vm-overview.md` — VM 总览与启动主线图
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/plan.md` §1.2 — 启动时序主线
- `notes/rewrite/fork-syscall-rewrite/01-stage-kernel/09-vm-boot-protocol.md` — 内核侧 VMCTL 协议（`VMCTL_BOOTINHIBIT_CLEAR` 等）
- `minix3/minix/servers/vm/main.c` — 本文档 ground truth
- `minix3/minix/lib/libsys/sef.c`、`sef_init.c` — SEF 框架实现
- `minix3/minix/servers/vm/utility.c:361-420` — VM 自身 libc 接口
- 素材：`notes/rewrite/fork-syscall-rewrite/02-stage-vm/draft/26-vm-init-main.md`（旧编号素材）
