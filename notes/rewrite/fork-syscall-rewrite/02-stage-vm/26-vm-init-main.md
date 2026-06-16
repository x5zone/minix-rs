# 26-vm-init-main: 所有零件怎么组装启动

> **分类**: VM 初始化与主循环
> **源码**: `minix3/minix/servers/vm/main.c`, `pagetable.c(pt_init)`, `utility.c(get_mem_chunks)`
> **Rust 对应**: `main.rs`, `global.rs`, `vmproc/table.rs`
> **说明**: VM 进程的完整启动流程、初始化顺序、主循环消息分发、SEF 框架

---

## 1. 概述

### 1.1 VM 启动的核心问题

VM 是系统中最先启动的用户态服务之一。它面临一个**鸡生蛋**的问题：

```
VM 需要内存来运行 → 但内存管理是 VM 的职责 → VM 怎么管理自己的内存？
```

解决方案：**分阶段初始化**——先使用内核提供的静态内存，再逐步建立自己的内存管理系统。

### 1.2 初始化的依赖链

```
┌─────────────────────────────────────────────────────────────┐
│                    VM 初始化依赖链                            │
│                                                             │
│  1. 内核提供的信息                                           │
│     sys_getkinfo() → kernel_boot_info                       │
│     ├── mmap_size/mmap_addr: 物理内存布局                    │
│     ├── boot_procs[]: 启动进程列表                           │
│     └── module_list[]: 内核模块列表                          │
│     ├── kernel_allocated_bytes(_dynamic): 内核自身占用的页   │
│          │                                                  │
│          ▼                                                  │
│  2. 基础数据结构 + 内存映射解析（不需要堆）                   │
│     enable_filemap=1 → 启用文件映射（默认开启）              │
│     get_mem_chunks() → 解析物理内存映射                      │
│     memset(vmproc, 0) → 进程表清零                           │
│     acl_init() → ACL 初始化                                  │
│     map_region_init() → 区域管理初始化                        │
│          │                                                  │
│          ▼                                                  │
│  3. 物理内存分配器（不需要堆）                                │
│     mem_init(mem_chunks) → 初始化位图分配器                  │
│          │                                                  │
│          ▼                                                  │
│  4. 页表系统（需要物理页，但用保留页池）                       │
│     init_proc(VM_PROC_NR) → VM 自身进程槽                   │
│     pt_init() → 建立页表 + 保留页池                          │
│          │                                                  │
│          ▼                                                  │
│  5. 堆可用（__minix_init 之后）                              │
│     __minix_init() → IPC 向量初始化                          │
│     内核模块内存修正 → mem_add_total_pages()                 │
│     SLABALLOC 可用 → 可以分配 VirRegion 等                   │
│          │                                                  │
│          ▼                                                  │
│  6. 启动进程设置                                             │
│     exec_bootproc() → 为每个启动进程建立地址空间              │
│     释放启动占用的物理内存 → free_mem()                       │
│          │                                                  │
│          ▼                                                  │
│  7. CALLMAP 注册 + SEF 启动 → 进入主循环                    │
└─────────────────────────────────────────────────────────────┘
```

---

## 2. C 源码分析

### 2.1 main() — 入口函数

```c
int main(void)
{
  message msg;
  int result, who_e, rcv_sts;
  int caller_slot;

  /* 1. 首次启动时初始化 VM */
  if (is_first_time()) {
	init_vm();
	__vm_init_fresh=1;
  }

  /* 2. SEF 框架启动 */
  sef_local_startup();
  __vm_init_fresh=0;

  SANITYCHECK(SCL_TOP);

  /* 3. 主循环 */
  while (TRUE) {
	int r, c;
	int type;
	int transid = 0;	/* VFS transid if any */

	SANITYCHECK(SCL_TOP);

	if(missing_spares > 0) {
		alloc_cycle();	/* mem alloc code wants to be called */
	}

  	if ((r=sef_receive_status(ANY, &msg, &rcv_sts)) != OK)
		panic("sef_receive_status() error: %d", r);

	if (is_ipc_notify(rcv_sts)) {
		/* Unexpected ipc_notify(). */
		printf("VM: ignoring ipc_notify() from %d\n", msg.m_source);
		continue;
	}
	who_e = msg.m_source;
	if(vm_isokendpt(who_e, &caller_slot) != OK)
		panic("invalid caller %d", who_e);

	/* 处理消息... */
  }
}
```

**is_first_time()**：检查 RS（重启服务器）是否还在启动中。如果是首次启动，RS 会有 `RTS_BOOTINHIBIT` 标志。如果是重启后恢复，则跳过 `init_vm()`。

### 2.2 init_vm() — 完整初始化流程

```c
void init_vm(void)
{
	int s, i;
	static struct memory mem_chunks[NR_MEMS];
	struct boot_image *ip;
	extern void __minix_init(void);
	multiboot_module_t *mod;
	vir_bytes kern_dyn, kern_static;

	/* ===== 阶段 1: 获取内核信息 ===== */
	if(OK != (s=sys_getkinfo(&kernel_boot_info))) {
		panic("couldn't get bootinfo: %d", s);
	}

	/* Turn file mmap on? */
	enable_filemap=1;	/* yes by default */
	env_parse("filemap", "d", 0, &enable_filemap, 0, 1);

	assert(kernel_boot_info.mmap_size > 0);
	assert(kernel_boot_info.mods_with_kernel > 0);

	/* ===== 阶段 2: 基础数据结构 + 内存映射解析（不需要堆） ===== */

	/* 解析物理内存布局 */
	get_mem_chunks(mem_chunks);

	/* 进程表清零 */
	memset(vmproc, 0, sizeof(vmproc));
	for(i = 0; i < ELEMENTS(vmproc); i++) {
		vmproc[i].vm_slot = i;
	}

	/* ACL 初始化 */
	acl_init();

	/* 区域管理初始化 */
	map_region_init();

	/* ===== 阶段 3: 物理内存分配器 ===== */
	mem_init(mem_chunks);

	/* ===== 阶段 4: 页表系统 ===== */
	init_proc(VM_PROC_NR);   /* VM 自身进程槽 */
	pt_init();               /* 建立页表 + 保留页池 */

	/* ===== 阶段 5: 堆可用 ===== */
	__minix_init();          /* IPC 向量初始化 */

	/* 修正总页数（内核模块占用的内存） */
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

	/* ===== 阶段 6: 启动进程设置 ===== */
	for (ip = &kernel_boot_info.boot_procs[0];
		ip < &kernel_boot_info.boot_procs[NR_BOOT_PROCS]; ip++) {
		struct vmproc *vmp;

		if(ip->proc_nr < 0) continue;
		assert(ip->start_addr);
		if(ip->proc_nr == VM_PROC_NR) continue;  /* VM 已设置 */

		vmp = init_proc(ip->proc_nr);
		exec_bootproc(vmp, ip);  /* 为启动进程建立地址空间 */

		assert(!(ip->start_addr % VM_PAGE_SIZE));
		ip->len = roundup(ip->len, VM_PAGE_SIZE);
		free_mem(ABS2CLICK(ip->start_addr), ABS2CLICK(ip->len));
	}

	/* ===== 阶段 7: 注册系统调用 ===== */

	/* Set call table to 0. This invalidates all calls (clear vmc_func). */
	memset(vm_calls, 0, sizeof(vm_calls));

	/* Basic VM calls. */
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

	/* Calls from RS */
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

	/* 标记 VM 实例 */
	num_vm_instances = 1;
	vmproc[VM_PROC_NR].vm_flags |= VMF_VM_INSTANCE;

	/* Let SEF know about VM mmapped regions. */
	s = sef_llvm_add_special_mem_region((void*)VM_OWN_HEAPBASE,
	    VM_OWN_MMAPTOP-VM_OWN_HEAPBASE, "%MMAP_ALL");
	if(s < 0) {
	    printf("VM: st_add_special_mmapped_region failed %d\n", s);
	}
}
```

### 2.3 pt_init() — 页表初始化

```c
void pt_init(void)
{
    /* 1. 保留页池：使用 BSS 段的静态内存 */
    sparepages_mem = (vir_bytes) static_sparepages;

    /* 2. 创建保留队列 */
    spare_pagequeue = reservedqueue_new(SPAREPAGES, 1, 1, 0);

    /* 3. 将静态内存注册到保留队列 */
    for(s = 0; s < STATIC_SPAREPAGES; s++) {
        void *v = (void *)(sparepages_mem + s * VM_PAGE_SIZE);
        phys_bytes ph;
        sys_umap(SELF, VM_D, v, VM_PAGE_SIZE * SPAREPAGES, &ph);
        reservedqueue_add(spare_pagequeue, v, ph);
    }

    /* 4. 建立内核映射 */
    while(sys_vmctl_get_mapping(pindex, &addr, &len, &flags) == OK) {
        kern_mappings[pindex] = ...;
        sys_vmctl_reply_mapping(pindex, vir);
    }

    /* 5. 分配内核映射页表 */
    pt_allocate_kernel_mapped_pagetables();

    /* 6. 复制当前页目录到 VM 自己的结构 */
    newpt = &vmprocess->vm_pt;
    pt_new(newpt);
    sys_vmctl_get_pdbr(SELF, &mypdbr);
    sys_vircopy(NONE, mypdbr, SELF, currentpagedir, VM_PAGE_SIZE, 0);

    /* 7. 建立页目录项 */
    for(pde = 0; pde < ARCH_VM_DIR_ENTRIES; pde++) {
        /* 复制内核映射 */
        /* 设置 VM 自身的映射 */
    }

    /* 8. 切换到新页表 */
    pt_map_page(newpt, ...);
    sys_vmctl_set_pdbr(SELF, newpt->pt_dir_phys);

    /* 9. 映射 VM 自身的堆 */
    pt_map_in_vm(VM_OWN_HEAPBASE, ...);
}
```

**关键洞察**：`pt_init()` 是最复杂的初始化步骤。它必须在**没有堆分配器**的情况下工作，使用 BSS 段的 `static_sparepages` 作为临时页表页。

### 2.4 init_proc() — 进程槽初始化

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

### 2.5 主循环 — 消息分发

```c
while (TRUE) {
	int r, c;
	int type;
	int transid = 0;	/* VFS transid if any */

	SANITYCHECK(SCL_TOP);

	/* 补充保留页池 */
	if(missing_spares > 0) alloc_cycle();

	/* 接收消息 */
  	if ((r=sef_receive_status(ANY, &msg, &rcv_sts)) != OK)
		panic("sef_receive_status() error: %d", r);

	/* 忽略通知 */
	if (is_ipc_notify(rcv_sts)) {
		printf("VM: ignoring ipc_notify() from %d\n", msg.m_source);
		continue;
	}

	/* 验证调用者 */
	who_e = msg.m_source;
	if(vm_isokendpt(who_e, &caller_slot) != OK)
		panic("invalid caller %d", who_e);

	type = msg.m_type;
	c = CALLNUMBER(type);
	result = ENOSYS; /* Out of range or restricted calls return this. */

	transid = TRNS_GET_ID(msg.m_type);

	if((msg.m_source == VFS_PROC_NR) && IS_VFS_FS_TRANSID(transid)) {
		/* VFS 事务：剥离 transid 后调用 do_procctl */
		msg.m_type = TRNS_DEL_ID(msg.m_type);
		result = do_procctl(&msg, transid);
	} else if(msg.m_type == RS_INIT && msg.m_source == RS_PROC_NR) {
		result = do_sef_init_request(&msg);
		if(result != OK) panic("do_sef_init_request failed!\n");
		result = SUSPEND;  /* 不回复 RS */
	} else if (msg.m_type == VM_PAGEFAULT) {
		if (!IPC_STATUS_FLAGS_TEST(rcv_sts, IPC_FLG_MSG_FROM_KERNEL)) {
			printf("VM: process %d faked VM_PAGEFAULT "
					"message!\n", msg.m_source);
		}
		do_pagefaults(&msg);
		/*
		 * do not reply to this call, the caller is unblocked by
		 * a sys_vmctl() call in do_pagefaults if success. VM panics
		 * otherwise
		 */
		continue;
	} else if(c < 0 || !vm_calls[c].vmc_func) {
		/* out of range or missing callnr */
	} else {
		if (acl_check(&vmproc[caller_slot], c) != OK) {
			printf("VM: unauthorized %s by %d\n",
					vm_calls[c].vmc_name, who_e);
		} else {
			SANITYCHECK(SCL_FUNCTIONS);
			result = vm_calls[c].vmc_func(&msg);
			SANITYCHECK(SCL_FUNCTIONS);
		}
	}

	/* 发送回复（除非 SUSPEND） */
	if(result != SUSPEND) {
		msg.m_type = result;

		assert(!IS_VFS_FS_TRANSID(transid));

		if((r=ipc_send(who_e, &msg)) != OK) {
			printf("VM: couldn't send %d to %d (err %d)\n",
				msg.m_type, who_e, r);
			panic("ipc_send() error");
		}
	}
}
```

**消息分发的优先级**：

| 优先级 | 消息类型 | 处理方式 | 回复 |
|--------|---------|---------|------|
| 1 | VFS 事务 | `do_procctl` | 正常回复 |
| 2 | RS_INIT | `do_sef_init_request` | SUSPEND（不回复） |
| 3 | VM_PAGEFAULT | `do_pagefaults` | 不回复（内核解除阻塞） |
| 4 | 普通请求 | `vm_calls[c].vmc_func` | 正常回复 |
| 5 | 无效请求 | ENOSYS | 错误回复 |

### 2.6 SEF 框架

SEF（System Event Framework）是 Minix3 的服务管理框架：

```c
static void sef_local_startup(void)
{
    sef_setcb_init_fresh(sef_cb_init_fresh);       /* 首次启动 */
    sef_setcb_init_lu(sef_cb_init_lu_restart);     /* Live Update */
    sef_setcb_init_restart(sef_cb_init_lu_restart); /* 重启 */
    sef_setcb_lu_state_changed(sef_cb_lu_state_changed);
    sef_setcb_signal_handler(sef_cb_signal_handler);
    sef_startup();
}
```

**SEF 回调**：

| 回调 | 触发时机 | VM 的处理 |
|------|---------|----------|
| `sef_cb_init_fresh` | 首次启动 | `map_service()` 注册服务权限 |
| `sef_cb_init_lu` | Live Update | 恢复状态 |
| `sef_cb_init_restart` | 重启 | 恢复状态 |
| `sef_cb_signal_handler` | 信号 | `SIGKMEM` → `do_memory()` |

### 2.7 SIGKMEM 信号处理

```c
static void sef_cb_signal_handler(int signo)
{
    switch(signo) {
        case SIGKMEM:
            do_memory();  /* 内核请求内存 */
            break;
    }

    if(missing_spares > 0) {
        alloc_cycle();  /* 补充保留页池 */
    }

    pt_clearmapcache();  /* 清除页表映射缓存 */
}
```

`SIGKMEM` 是内核发送的信号，表示内核需要分配内存（如新的页表页）。VM 在信号处理中分配内存并映射给内核。

---

## 3. Rust 设计决策

> 本章解释"为什么这样设计"。每个决策必须可追溯到 Ch1&2 的 C 源码分析。

### 3.1 代码现状——起点快照

| 组件 | 状态 | 说明 |
|------|------|------|
| `vm_server.rs` | 骨架 | `run()` 为 `loop { break; }`，缺少主循环 dispatch |
| `global.rs` | 已实现 | `BOOT_INFO`, `TOTAL_PAGES`, `VM_INSTANCE_COUNT`, `VmAllocator` |
| `VmProcTable` | 已实现 | `vmproc/table.rs`，`AssumeSyncCell` 后端，`get_global()` 静态访问 |
| `VmPageAllocator` | 已实现 | `alloc_page.rs`，封装 `PhysAlloc`，支持 alloc/free/stats |
| `PhysAllocator` | 已实现 | 位图/buddy/线段树分配器，`PhysAlloc` enum |
| `Paging` trait | 已实现 | `pagetable/mod.rs`，页表操作 trait |
| `MemType` trait | 已实现 | `memtype.rs`，内存类型系统 |
| `PageCache` | 已实现 | `page_cache.rs`, 含 `CacheKey::ByDevice`/`ByBlock` |
| `VfsRequestQueue` | 已实现 | `vfs_queue.rs`, serial activation model |
| `MessageDispatcher` | 已实现 | `ipc/dispatcher.rs`, 编译时 match 分派 |
| RS 握手 | 骨架 | `rs.rs` 实现 SET_PRIV/PREPARE/UPDATE/MEMCTL，缺 `rs_handshake()` |

> 上表是"起点快照"——不是开发进度追踪，而是为了后续各节解释"为什么"和"怎么做"时有明确的参照系。

### 3.2 为什么 VmServer 是包含特定字段的 struct

**C 源码依据**：`init_vm()` (main.c:428-557) 和主循环 (main.c:97-196) 访问以下全局变量：

| C 全局变量 | 用途 | Rust 对应 |
|-----------|------|----------|
| `vmproc[]` | 进程状态表 | `VmProcTable` — 通过 `VmProcTable::get_global()` 静态访问，**不放在 VmServer 中** |
| `total_pages`, `free_pages_bitmap[]` | 物理页分配状态 | `page_alloc: VmPageAllocator` — 独占所有权 |
| `cache_hash_bydev[]`, `cache_hash_byino[]` | 页缓存 | `page_cache: PageCache` — 独占所有权 |
| `vm_pfindex[]` (PageFrame slots) | PFN 索引 | `page_frames: Option<PageFrames>` — 构造时延迟初始化 |
| `first_queued`, `last_queued` (VFS queue) | VFS 异步请求队列 | `vfs_queue: VfsRequestQueue` — 独占所有权 |
| 无对应 | 初始化完成标志 | `initialized: bool` — 确保 `run()` 只在 `init()` 之后调用 |

**设计决策**：真实代码中的 VmServer 结构体（`vm_server.rs:30-36`）：

```rust
pub struct VmServer {
    page_alloc: VmPageAllocator,
    page_cache: PageCache,
    page_frames: Option<PageFrames>,
    vfs_queue: VfsRequestQueue,
    initialized: bool,
}
```

`VmProcTable` 不放在 VmServer 中，因为它是一个**编译时静态数组**（`[AssumeSyncCell<VmProc>; VM_PROC_COUNT]`），通过 `VmProcTable::get_global()` 全局访问。这与 C 的 `vmproc[]` BSS 段数组语义一致——都存在于编译时分配的静态内存中，不是堆分配的。

**替代方案与选择**：为何不把 `VmProcTable` 放入 VmServer？因为在单线程模型中，全局静态和 struct 字段在正确性上没有区别。但全局静态让 `MessageDispatcher::dispatch_xxx()` 可以直接通过 `get_global()` 访问进程表，而不需要从 VmServer 参数链层层传递，减少了函数签名噪声。

### 3.3 为什么用编译时 match 替代运行时函数指针表

**C 源码依据**：`init_vm()` 中的 CALLMAP 宏 (main.c:502-505) 和主循环中的 `vm_calls[c].vmc_func(&msg)` (main.c:163-165)。

```c
#define CALLMAP(code, func) {                            \
    _cmi=CALLNUMBER(code);                                \
    vm_calls[_cmi].vmc_func = (func);                     \
}
```

C 使用运行时的函数指针表，因为 C 没有编译时泛型/match。在 Rust 中，`MessageDispatcher::dispatch_by_number()` 使用 `match c { ... }` 在编译时解析所有调用号，无需运行时查表。

**收益**：
- 零成本抽象：编译器生成与手写 if-else 相同的跳转表
- 类型安全：每个分支调用特定的 `dispatch_xxx()` 方法，参数类型在编译时检查
- 无需初始化 `call_table`：不需要 `memset(vm_calls, 0, ...)` 和逐个 CALLMAP 注册

**兼容性**：现有的 24 文档（`24-vm-ipc-dispatch.md`）已设计了 `MessageDispatcher` 模式——每个 `dispatch_xxx()` 方法接收已解码的 `VmXxxIn` 类型，返回 `VmReply`。`dispatch_by_number()` 是这个模式的自然扩展：接收原始 `&Message`，解码为对应类型，调用 `dispatch_xxx()`。

### 3.4 为什么用 VmReply 枚举替代 C 的 errno + SUSPEND

**C 源码依据**：主循环中 `result` 变量 (main.c:118-193)：
- 正常回复：`result = vm_calls[c].vmc_func(&msg)` → `msg.m_type = result; ipc_send(who_e, &msg);`
- 挂起：`result = SUSPEND;` → 跳过 `ipc_send()`
- Pagefault：`continue;` → 不回复，内核用 `sys_vmctl` 唤醒

C 把 `SUSPEND` 编码为特殊的 errno 值（`#define SUSPEND -999`），这是一种**类型不安全**的做法——调用者必须知道哪些函数可能返回 SUSPEND。

Rust 使用 `VmReply` 枚举（`minix-types/src/ipc/vm.rs:409`）：

```rust
pub enum VmReply {
    Fork(VmForkOut),
    Brk(VmBrkOut),
    Mmap(VmMmapOut),
    // ... per-service Out types
    Ok,
    Suspend,
    Error(VmError),
}
```

`VmReply::Suspend` 明确区分"挂起不回复"和"回复错误码"，编译时保证了调用者不会混淆。

### 3.5 为什么用 `initialized: bool` 而非 typestate 初始化

**C 源码依据**：`main()` (main.c:103-108) 中 `is_first_time()` 检查后调用 `init_vm()`，之后无条件进入主循环。初始化是**严格线性、不可逆**的——没有"进入无效状态再恢复"的场景。

Rust 中的 `ActiveProc`/`ExitingProc`/`EmptySlot` 使用 typestate，是因为进程在运行中反复进出状态（runnable→blocked→dying），非法转换是真实风险。初始化没有这个场景——`init()` 完成后再也不会回到未初始化状态，`run()` 永远不会退出。使用 `bool` + `assert!(self.initialized, ...)` 已经覆盖了 `run()` 的安全边界。

### 3.6 为什么 SEF 框架简化为 `rs_handshake()`

**C 源码依据**：`sef_local_startup()` (main.c:219-235) + `sef_cb_init_fresh()` (main.c:237-250)

SEF 在 C 中解决了 3 个问题。Rust 下这 3 个问题都有更简单的替代：

| SEF 组件 | C 中的必要性 | Rust 下的处理 |
|---------|-------------|--------------|
| `sef_startup()` + `sef_cb_init_fresh` | C 没有标准进程初始化协议。必须通过 IPC 向 RS 发 `RS_INIT`，接收 `rproctab`，注册 ACL。 | 保留协议但去掉框架。替换为一个 `rs_handshake()` 函数：发送 `RS_INIT` → 接收 rproctab → 调用 `AclState::acl_set()` + `ActiveProc::set_acl()`。 |
| `sef_cb_init_lu` / `sef_cb_init_restart` | 热更新状态机（swap 进程槽、迁移页表）。 | 暂不实现 Live Update。后续需要时用 `serde` 等工具。`rs.rs` 中 `handle_rs_prepare`/`handle_rs_update` 已预留 RS 协议入口。 |
| `sef_cb_signal_handler` | 信号通过 IPC 发送，SEF 提供注册机制。 | 不需要注册——主循环中直接 `match` 处理 `SIGKMEM` 即可。 |

**设计决策**：`rs_handshake()` 约 10 行代码替代了 SEF 的 `setcb` 注册 + `startup` 状态机（~60 行）。详细实现见 §4.5。

### 3.7 主循环 5 优先级 Dispatch 模型

**C 源码依据**：主循环 `main.c:97-196`，已在 §2.5 完整分析。

| 优先级 | C 触发条件 | Rust 对应 | 回复行为 |
|--------|-----------|----------|---------|
| 1 | `msg.m_source == VFS_PROC_NR && IS_VFS_FS_TRANSID(transid)` | `dispatch_vfs_transid()` | `VmReply::Ok` → 正常回复 |
| 2 | `msg.m_type == RS_INIT && msg.m_source == RS_PROC_NR` | `rs_handshake()` | `VmReply::Suspend` |
| 3 | `msg.m_type == VM_PAGEFAULT` | `dispatch_pagefault()` | `VmReply::Suspend` |
| 4 | `c >= 0 && vm_calls[c].vmc_func` + ACL check | `dispatch_by_number()` → `dispatch_xxx()` | per-service `VmReply` |
| 5 | `c < 0 || !vm_calls[c].vmc_func` | fall-through | `VmReply::Error(VmError::NotImplemented)` |

**设计要点**：
- 优先级 1 必须在优先级 4 之前，因为 VFS transid 消息的 `m_type` 也是普通 VM 调用号范围
- 优先级 2 中的 `rs_handshake()` 内部发送 `RS_INIT` 后返回 `Suspend`，不回复 RS
- 优先级 3 中的 pagefault 处理后不回复调用者——内核通过 `sys_vmctl` 解除进程阻塞

---

## 4. 实现详解

> 本章解释"如何实现"。每个实现对应 §3 的设计决策。代码中注释使用英文，配合 C 源码行号引用。

### 4.1 VmServer 结构体

> 设计决策：§3.2 — 字段选择

真实代码 `vm_server.rs:30-36`：

```rust
pub struct VmServer {
    page_alloc: VmPageAllocator,
    page_cache: PageCache,
    page_frames: Option<PageFrames>,
    vfs_queue: VfsRequestQueue,
    initialized: bool,
}
```

`VmProcTable` 通过 `VmProcTable::get_global()` 获取，不在结构体中。`page_frames` 用 `Option` 因为它在 `init()` 中延迟初始化——构造时需要先知道 `total_pages`。

`VmServer::new()` 负责"构造期初始化"（物理分配器、VM 自身页表）：

```rust
// vm_server.rs:38-52
pub fn new(total_pages: usize, free_regions: &[BootMemRegion]) -> Self {
    let phys_alloc = Self::create_default_allocator(total_pages, free_regions);
    let mut page_alloc = VmPageAllocator::new(phys_alloc);
    crate::global::register_page_alloc(&mut page_alloc);

    init_vm_self_pt();  // pt_init() 的 Rust 等价

    Self {
        page_alloc,
        page_cache: PageCache::new(),
        page_frames: None,
        vfs_queue: VfsRequestQueue::new(),
        initialized: false,
    }
}
```

### 4.2 init() — 初始化流程

> 设计决策：§3.5 — `bool` 替代 typestate

**C 源码对应**：`init_vm()` (main.c:428-557) 的 Rust 简化版。

Rust 版本的初始化比 C 简单得多，因为很多 C 中的手动操作被 Rust 的类型构造器吸收了：

| C 操作 | Rust 等价 | 何时完成 |
|--------|----------|---------|
| `memset(vmproc, 0, ...)` | 编译时 `[AssumeSyncCell::new(VmProc::vacant()); N]` | 编译时 |
| `mem_init(mem_chunks)` | `VmPageAllocator::new(phys_alloc)` | `VmServer::new()` |
| `pt_init()` | `init_vm_self_pt()` | `VmServer::new()` |
| CALLMAP 逐个注册 | `MessageDispatcher::dispatch_by_number()` 的 `match` | 编译时 |
| `sef_startup()` → `sef_cb_init_fresh()` | `rs_handshake()` | 主循环中触发 |

真实代码 `vm_server.rs:137-155`：

```rust
pub fn init(&mut self) {
    #[cfg(not(test))]
    self.relocate();

    // Phase 1: Memory detection
    self.init_global_state();

    // Phase 2: Process table
    self.init_proc_table();

    // Phase 3: PageFrames (needs total_pages known)
    let total_phys = PhysBytes(
        self.page_alloc.total_pages() as u64 * crate::region::PAGE_SIZE
    );
    self.page_frames = Some(PageFrames::new(total_phys));

    self.initialized = true;
}

fn init_global_state(&mut self) {
    unsafe { crate::global::init(self.page_alloc.total_pages()); }
}

fn init_proc_table(&mut self) {
    let _table = VmProcTable::get_global();
}
```

### 4.3 run() — 主循环

> 设计决策：§3.7 — 5 优先级 dispatch 模型

**C 源码对应**：`main()` 主循环 (main.c:110-196)

`run()` 当前为 `loop { break; }` stub。新增实现：

```rust
/// Infinite main loop. C: main.c:110-196
///
/// Five priority levels:
/// 1. VFS transid  → dispatch_vfs_transid()
/// 2. RS_INIT      → rs_handshake(), return Suspend
/// 3. VM_PAGEFAULT → dispatch_pagefault(), return NoReply
/// 4. Normal calls → dispatch_by_number()
/// 5. Invalid      → ENOSYS
pub fn run(&mut self) -> ! {
    assert!(self.initialized, "VmServer::run() called before init()");

    loop {
        // C: if(missing_spares > 0) alloc_cycle();
        // TODO: CriticalPool refill

        // C: sef_receive_status(ANY, &msg, &rcv_sts)
        let (msg, rcv_sts) = match ipc_receive() {
            Ok(v) => v,
            Err(e) => panic!("ipc_receive() error: {:?}", e),
        };

        // C: if(is_ipc_notify(rcv_sts)) { continue; }
        if is_ipc_notify(&rcv_sts) {
            continue;
        }

        // C: who_e = msg.m_source; vm_isokendpt(who_e, &caller_slot);
        let who_e = msg.m_source;
        let caller_slot = match VmProcTable::get_global().vm_isokendpt(who_e) {
            Ok(slot) => slot,
            Err(_) => panic!("invalid caller {}", who_e),
        };

        let action = self.dispatch_on_msg(&msg, &rcv_sts, caller_slot);

        // C: if(result != SUSPEND) { ipc_send(who_e, &msg); }
        match action {
            DispatchAction::Reply(reply) => {
                encode_and_reply(who_e, &msg, &reply)
                    .unwrap_or_else(|e| panic!("ipc_send() error: {:?}", e));
            }
            DispatchAction::Suspend => {}
            DispatchAction::NoReply => {}
        }
    }
}
```

### 4.4 dispatch_on_msg() — 5 优先级分发

> 设计决策：§3.7

```rust
/// Three reply actions — maps to C main.c:172-193 reply behavior.
enum DispatchAction {
    Reply(VmReply),
    Suspend,
    NoReply,
}

impl VmServer {
    /// Five-priority dispatch. C: main.c:131-170.
    fn dispatch_on_msg(
        &mut self,
        msg: &Message,
        rcv_sts: &IpcStatus,
        caller_slot: UserSlot,
    ) -> DispatchAction {
        let m_type = msg.m_type;
        let source = msg.m_source;

        // Priority 1: VFS transid (main.c:131-141)
        if source == VFS_PROC_NR && is_vfs_fs_transid(m_type) {
            let transid = transid_extract(m_type);
            let clean_type = transid_strip(m_type);
            let result = self.handle_vfs_transid(clean_type, transid, msg);
            return DispatchAction::Reply(result);
        }

        // Priority 2: RS_INIT (main.c:142-146)
        if m_type == RS_INIT && source == RS_PROC_NR {
            self.rs_handshake()
                .expect("rs_handshake failed");
            return DispatchAction::Suspend;
        }

        // Priority 3: VM_PAGEFAULT (main.c:147-156)
        if m_type == VM_PAGEFAULT {
            debug_assert!(
                is_from_kernel(rcv_sts),
                "faked VM_PAGEFAULT from {}", source
            );
            let _ = self.dispatch_pagefault(msg);
            return DispatchAction::NoReply;
        }

        // Priority 4: Normal VM calls (main.c:157-168)
        if let Some(c) = callnr(m_type) {
            // C: acl_check(&vmproc[caller_slot], c)
            let table = VmProcTable::get_global();
            if let Some(proc) = table.get_active(caller_slot) {
                if proc.acl_check(c as u32).is_err() {
                    log::warn!("unauthorized call {} by {}", c, source);
                    return DispatchAction::Reply(
                        VmReply::Error(VmError::PermissionDenied)
                    );
                }
            }
            // C: result = vm_calls[c].vmc_func(&msg);
            let reply = MessageDispatcher::dispatch_by_number(c, msg, self);
            return DispatchAction::Reply(reply);
        }

        // Priority 5: Invalid request → ENOSYS (main.c:170)
        DispatchAction::Reply(VmReply::Error(VmError::NotImplemented))
    }
}

/// C: CALLNUMBER(c) with bounds check. main.c:55.
fn callnr(m_type: u32) -> Option<usize> {
    let c = m_type.checked_sub(VM_RQ_BASE)?;
    if (c as usize) < NR_VM_CALLS {
        Some(c as usize)
    } else {
        None
    }
}
```

### 4.5 rs_handshake() — 替代 SEF 启动

> 设计决策：§3.6 — SEF 简化为直接函数

**C 源码对应**：`sef_cb_init_fresh()` (main.c:237-250) + `map_service()` (main.c:752-768)

```rust
impl VmServer {
    /// RS handshake — replaces C's sef_startup() + sef_cb_init_fresh().
    ///
    /// C (main.c:237-250):
    ///   sys_safecopyfrom(RS_PROC_NR, info->rproctab_gid, 0, rprocpub, ...)
    ///   for i in 0..NR_BOOT_PROCS:
    ///     if rprocpub[i].in_use:
    ///       map_service(&rprocpub[i])
    ///
    ///   map_service() (main.c:752-768):
    ///     vm_isokendpt(rpub->endpoint, &proc_nr)
    ///     acl_set(&vmproc[proc_nr], rpub->vm_call_mask, ...)
    fn rs_handshake(&mut self) -> Result<(), VmError> {
        let table = VmProcTable::get_global();

        // 1. Send RS_INIT, receive rproctab
        // 当前 ipc_call_rs_init() 返回 Ok(RprocTab::empty())，不 panic
        // 待 IpcTransport 实现后将返回真实的 rproctab
        let rproctab = ipc_call_rs_init()
            .map_err(|_| VmError::InternalError)?;

        // 2. Register ACL for each boot service
        for entry in &rproctab {
            if !entry.in_use { continue; }
            let slot = table.vm_isokendpt(entry.endpoint)
                .map_err(|_| VmError::InvalidProcess)?;
            let mut proc = table.get_active(slot)
                .ok_or(VmError::InvalidProcess)?;
            let is_sys = !entry.is_user;
            let mask = Some(crate::acl::AclMask::from_raw(entry.call_mask));
            proc.set_acl(crate::acl::AclState::acl_set(is_sys, mask));
        }
        Ok(())
    }
}
```

### 4.6 handle_signal() — 替代 sef_cb_signal_handler

**C 源码对应**：`sef_cb_signal_handler()` (main.c:737-750)

```rust
impl VmServer {
    /// Signal handler — replaces C's sef_cb_signal_handler().
    /// C (main.c:737-750):
    ///   case SIGKMEM: do_memory(); break;
    ///   if(missing_spares > 0) alloc_cycle();
    ///   pt_clearmapcache();
    fn handle_signal(&mut self, signo: i32) {
        match signo {
            SIGKMEM => {
                // C: do_memory(); — kernel memory request
                // TODO: implement do_memory() equivalent
            }
            _ => {}
        }
        // C: if(missing_spares > 0) alloc_cycle();
        // TODO: CriticalPool refill
        // C: pt_clearmapcache();
        // TODO: clear page table map cache
    }
}
```

### 4.7 dispatch_by_number() — 编译时 CALLMAP

> 设计决策：§3.3 — 编译时 match 替代运行时函数指针表

**C 源码对应**：`vm_calls[c].vmc_func(&msg)` (main.c:163-165), CALLMAP 宏 (main.c:502-505)

在 `ipc/dispatcher.rs` 中新增（覆盖 ~20 个 CALLMAP 条目，对应 main.c:508-538）：

```rust
impl MessageDispatcher {
    /// Compile-time CALLMAP. C: vm_calls[c].vmc_func(&msg).
    ///
    /// Full coverage of ~20 entries from C main.c:508-538.
    /// Each branch decodes Message → VmXxxIn, then calls the
    /// existing dispatch_xxx() method.
    pub(crate) fn dispatch_by_number(
        call_nr: usize,
        msg: &Message,
        server: &mut VmServer,
    ) -> VmReply {
        let table = VmProcTable::get_global();
        let frames = server.page_frames_mut();
        let page_alloc = server.page_alloc_mut();
        let cache = server.page_cache_mut();

        match call_nr {
            _c if _c == callnr_of(VM_MMAP) =>
                Self::dispatch_mmap(table, page_alloc, frames, VmMmapIn::decode(msg)),
            _c if _c == callnr_of(VM_MUNMAP) =>
                Self::dispatch_munmap(table, page_alloc, frames, VmMunmapIn::decode(msg)),
            _c if _c == callnr_of(VM_MAP_PHYS) =>
                Self::dispatch_map_phys(table, page_alloc, frames, VmMapPhysIn::decode(msg)),
            _c if _c == callnr_of(VM_EXIT) =>
                Self::dispatch_exit(table, page_alloc, frames, VmExitIn::decode(msg)),
            _c if _c == callnr_of(VM_FORK) =>
                Self::dispatch_fork(table, page_alloc, frames, VmForkIn::decode(msg)),
            _c if _c == callnr_of(VM_BRK) =>
                Self::dispatch_brk(table, page_alloc, frames, VmBrkIn::decode(msg)),
            _c if _c == callnr_of(VM_WILLEXIT) =>
                Self::dispatch_willexit(table, VmWillexitIn::decode(msg)),
            _c if _c == callnr_of(VM_VFS_MMAP) =>
                Self::dispatch_vfs_mmap(table, page_alloc, frames, VmVfsMmapIn::decode(msg)),
            _c if _c == callnr_of(VM_RS_SET_PRIV) => {
                let (caller, target, mask, is_sys) = decode_rs_set_priv(msg);
                Self::dispatch_rs_set_priv(table, caller, target, mask, is_sys)
            }
            _c if _c == callnr_of(VM_RS_PREPARE) => {
                let (src, dst, flags) = decode_rs_prepare(msg);
                Self::dispatch_rs_prepare(table, page_alloc, frames, src, dst, flags)
            }
            _c if _c == callnr_of(VM_RS_UPDATE) => {
                let (src, dst, flags) = decode_rs_update(msg);
                Self::dispatch_rs_update(table, page_alloc, frames, src, dst, flags)
            }
            _c if _c == callnr_of(VM_RS_MEMCTL) => {
                let (target, req) = decode_rs_memctl(msg);
                Self::dispatch_rs_memctl(table, page_alloc, frames, target, req)
            }
            _c if _c == callnr_of(VM_GETPHYS) => {
                let (target, addr) = decode_get_phys(msg);
                Self::dispatch_get_phys(table, target, addr)
            }
            _c if _c == callnr_of(VM_GETREF) => {
                let (target, addr) = decode_get_refcount(msg);
                Self::dispatch_get_refcount(table, frames, target, addr)
            }
            _c if _c == callnr_of(VM_INFO) => {
                let q = decode_info(msg);
                Self::dispatch_info(table, page_alloc, q)
            }
            _c if _c == callnr_of(VM_GETRUSAGE) => {
                let (caller, target, children) = decode_getrusage(msg);
                Self::dispatch_getrusage(table, caller, target, children)
            }
            _c if _c == callnr_of(VM_MAPCACHEPAGE) =>
                Self::dispatch_mapcache(table, page_alloc, frames, cache, VmCacheIn::decode(msg)),
            _c if _c == callnr_of(VM_SETCACHEPAGE) =>
                Self::dispatch_setcache(table, frames, cache, VmCacheIn::decode(msg)),
            _c if _c == callnr_of(VM_FORGETCACHEPAGE) =>
                Self::dispatch_forgetcache(cache, frames, VmCacheIn::decode(msg)),
            _c if _c == callnr_of(VM_CLEARCACHE) =>
                Self::dispatch_clearcache(cache, frames, VmCacheIn::decode(msg)),
            _ => VmReply::Error(VmError::NotImplemented),
        }
    }
}

/// C: CALLNUMBER(c) = (c) - VM_RQ_BASE. Offset-only, bounds checked by caller.
fn callnr_of(call_type: u32) -> usize {
    (call_type - VM_RQ_BASE) as usize
}
```

对于 `VM_VFS_REPLY`、`VM_REMAP`、`VM_REMAP_RO`、`VM_PROCCTL`、`VM_UNMAP_PHYS`、`VM_SHM_UNMAP`、`VM_ADDDMA`、`VM_DELDMA`、`VM_GETDMA`，当前返回 `VmReply::Error(VmError::NotImplemented)`。

### 4.8 回复模型：VmReply → errno 转换

**C 源码对应**：`result != SUSPEND` 分支 (main.c:172-193)

```rust
fn reply_to_errno(reply: &VmReply) -> i32 {
    match reply {
        VmReply::Ok => 0,
        VmReply::Suspend => unreachable!("Suspend filtered before reply_to_errno"),
        VmReply::Error(e) => e.to_errno(),
        VmReply::Fork(_) | VmReply::Brk(_) | VmReply::Mmap(_)
        | VmReply::MapPhys(_) | VmReply::Exit | VmReply::Willexit
        | VmReply::Munmap | VmReply::ExecNewmem(_)
        | VmReply::MapCache { .. } | VmReply::VfsMmap(_)
        | VmReply::GetPhys { .. } | VmReply::GetRefcount { .. }
        | VmReply::InfoStats { .. } | VmReply::InfoUsage { .. }
        | VmReply::InfoRegion { .. } | VmReply::Getrusage { .. }
        | VmReply::RsMemctlAddrLen { .. } => 0, // OK
    }
}
```

---

## 5. 初始化顺序的约束分析

### 5.1 堆依赖的三层

| 层级 | 可用时机 | 分配方式 | 典型用途 |
|------|---------|---------|---------|
| **L0: 静态** | 编译时 | `static` / `const` | `VmProcTable` (编译时数组), `VmAllocator` (全局分配器) |
| **L1: Direct Map** | `VmServer::new()` 中 | `vm_phys_to_virt()` | 物理分配器元数据 |
| **L2: 堆** | `VmServer::new()` 之后 | `Box::new()` / `Vec` | `PageFrames`, `HeapArena` 分配 |

### 5.2 与 C 的对应（64位架构演进）

| Minix3 (32位) | Rust (64位) | 说明 |
|---------------|-------------|------|
| `memset(vmproc, 0)` | `[AssumeSyncCell::new(VmProc::vacant()); N]` | 编译时初始化 |
| `static_sparepages[BSS]` | 不需要 | Direct Map 下物理页直接通过虚拟地址访问 |
| `pt_init()` → `reservedqueue_alloc` | `init_vm_self_pt()` | 64位下页表通过 Direct Map 管理 |
| `SLABALLOC` | `Box::new()` via `VmAllocator` | 全局分配器用 HeapArena bump allocator |

---

## 6. exec_bootproc — 启动进程地址空间建立

### 6.1 C 源码逻辑 (main.c:480-498)

```c
for (ip = &kernel_boot_info.boot_procs[0];
     ip < &kernel_boot_info.boot_procs[NR_BOOT_PROCS]; ip++) {
    if(ip->proc_nr < 0) continue;
    if(ip->proc_nr == VM_PROC_NR) continue;

    vmp = init_proc(ip->proc_nr);
    exec_bootproc(vmp, ip);
    free_mem(ABS2CLICK(ip->start_addr), ABS2CLICK(ip->len));
}
```

### 6.2 Rust 中的处理

当前 Rust 代码中 `exec_bootproc` 暂未实现。原因是：

1. **`VmServer::new()` 中已处理 VM 自身**：`init_vm_self_pt()` 建立了 VM 进程的页表
2. **其他启动进程的页表建立**将在主循环收到 `RS_INIT` 后通过 `rs_handshake()` 触发
3. **启动进程的二进制加载**由 `libexec` 库处理（C 通过 `libexec_load_elf()`），Rust 中等价功能暂未集成

> **TODO**：实现 `exec_bootproc` 的 Rust 版本需要与 `libexec` 对应，涉及 ELF 加载和初始堆栈设置。当前阶段可以先在 `rs_handshake()` 中为每个启动进程调用 `init_proc()` 建立基础进程槽。

---

## 7. 完整的初始化时序图

```
┌────────┐     ┌──────────┐     ┌──────────┐     ┌──────────┐
│ Kernel │     │ VM main  │     │ PhysAlloc│     │ PageTable│
└───┬────┘     └────┬─────┘     └────┬─────┘     └────┬─────┘
    │               │                │                │
    │ VmServer::new │                │                │
    │──────────────────────────────►│                │
    │               │create_default_allocator         │
    │               │◄──────────────│                │
    │               │                │                │
    │               │ init_vm_self_pt()               │
    │               │───────────────────────────────►│
    │               │◄───────────────────────────────│
    │               │                │                │
    │ VmServer::init()               │                │
    │───────┐       │                │                │
    │ relocate()    │                │                │
    │ init_global   │                │                │
    │ init_proc_table                │                │
    │ PageFrames::new                │                │
    │ initialized=true               │                │
    │◄──────┘       │                │                │
    │               │                │                │
    │ VmServer::run()                │                │
    │───────┐       │                │                │
    │◄──────┘       │                │                │
    │               │                │                │
    │  ═══════════ 主循环开始 ═══════════                │
    │               │                │                │
    │   RS_INIT     │                │                │
    │──────────────►│                │                │
    │               │ rs_handshake() │                │
    │               │───────┐        │                │
    │  (Suspend)    │◄──────┘        │                │
    │               │                │                │
    │   VM_FORK     │                │                │
    │──────────────►│                │                │
    │               │ dispatch_by_number(VM_FORK)
    │               │───────┐        │                │
    │   VmReply::Fork│◄──────┘        │                │
    │◄──────────────│                │                │
    │               │                │                │
```

---

## 8. 实现组件一览

### 8.1 现有文件（需修改）

| 文件 | 修改内容 |
|------|---------|
| `vm_server.rs` | 实现 `run()` 主循环 + `dispatch_on_msg()` + `rs_handshake()` + `handle_signal()` |
| `ipc/dispatcher.rs` | 新增 `dispatch_by_number()` 编译时 CALLMAP |
| `main.rs` | 无需修改（已调用 `init()` + `run()`） |

### 8.2 现有文件（无需修改）

| 文件 | 说明 |
|------|------|
| `global.rs` | `TOTAL_PAGES`, `VM_INSTANCE_COUNT`, `VmAllocator` — 已实现 |
| `vmproc/table.rs` | `VmProcTable::get_global()`, `vm_isokendpt()` — 已实现 |
| `lib.rs` | 模块导出 — 无需变更 |

### 8.3 IPC 基础设施依赖

`run()` 的实现依赖以下 IPC 功能（当前为 stub）：

| 功能 | C 对应 | 当前状态 |
|------|--------|---------|
| `ipc_receive()` | `sef_receive_status(ANY, &msg, &rcv_sts)` | 需新增 |
| `ipc_send()` | `ipc_send(who_e, &msg)` | 需新增 |
| `is_ipc_notify()` | `is_ipc_notify(rcv_sts)` | 需新增 |
| `is_from_kernel()` | `IPC_STATUS_FLAGS_TEST(IPC_FLG_MSG_FROM_KERNEL)` | 需新增 |
| `ipc_call_rs_init()` | `sys_safecopyfrom(RS_PROC_NR, ...)` | 需新增 |

> **TODO**：IPC 基础设施应由一个独立的 `ipc` 模块提供，遵循 trait 抽象（`trait IpcTransport`），使得主循环代码不依赖具体 IPC 实现。当前 `run()` 可以先使用直接函数调用。

### 8.4 测试计划

| 测试 | 描述 | 覆盖设计 |
|------|------|---------|
| `test_dispatch_priorities` | VFS transid → RS_INIT → pagefault → 普通请求 → ENOSYS | §3.7 |
| `test_dispatch_suspend` | RS_INIT 返回 Suspend，不回复 | §3.4 §4.4 |
| `test_dispatch_noreply` | VM_PAGEFAULT 返回 NoReply | §4.4 |
| `test_dispatch_enosys` | 未知 call number → ENOSYS | §3.7 §4.4 |
| `test_dispatch_unauthorized` | ACL 拒绝 → PermissionDenied | §4.4 |
| `test_rs_handshake` | RS 握手协议流程 | §3.6 §4.5 |
| `test_dispatch_by_number_coverage` | 所有 ~20 个 CALLMAP 条目正确分派 | §3.3 §4.7 |
| `test_reply_to_errno` | VmReply 到 errno 的正确转换 | §4.8 |

---

## 9. 设计洞察

### 9.1 VmServer 的所有权模型

Minix3 C 的所有全局变量在 Rust 中的对应关系：

| Minix3 C 全局 | Rust | 所有权 |
|-------------|------|--------|
| `vmproc[]` (BSS) | `VmProcTable` (编译时 static) | 全局静态，`get_global()` 访问 |
| `total_pages`, `free_pages_bitmap[]` | `VmPageAllocator` → `page_alloc` 字段 | VmServer 独占 |
| `cache_hash_bydev[]`, `cache_hash_byino[]` | `PageCache` → `page_cache` 字段 | VmServer 独占 |
| `vm_pfindex[]` | `PageFrames` → `page_frames` 字段 | VmServer 独占 |
| VFS queue | `VfsRequestQueue` → `vfs_queue` 字段 | VmServer 独占 |

**单线程安全**：主循环 `run(&mut self)` 独占 `&mut VmServer`，`VmProcTable` 的 `AssumeSyncCell` 在单线程下安全。不存在数据竞争。

### 9.2 初始化的不可逆性

VM 的初始化是**不可逆**的——一旦 `initialized = true`，就不能回退。与其他模块（如 `ActiveProc` ↔ `ExitingProc` 可逆转换）不同。

**为什么不用 typestate**：typestate 的价值在于防止非法状态转换（如 `Blocked` → `Blocked`）。初始化的状态图是 `Uninit → Init → (run forever)`，只有一个方向。`bool` + `assert!` 足够。

### 9.3 主循环的 SUSPEND 语义

`VmReply::Suspend` 不影响 errno（不被发送），而是改变**回复行为**：

| 触发场景 | VmReply | 回复行为 | C 对应 |
|---------|---------|---------|--------|
| Normal completion | `VmReply::Ok` or per-service Out | `ipc_send(who_e, &msg)` with OK | `result = OK; ipc_send(...);` |
| VFS async | `VmReply::Suspend` | No reply; caller blocked until VFS reply | `result = SUSPEND;` |
| Pagefault | no VmReply sent | Caller unblocked by kernel `sys_vmctl` | `continue;` |
| Error | `VmReply::Error(e)` | `ipc_send(who_e, msg with e.to_errno())` | `result = errno; ipc_send(...);` |

### 9.4 与 Live Update 的关系

Minix3 的 SEF 支持 Live Update（热更新）——在不停止系统的情况下更新 VM 代码。Rust 版本暂不实现：

- `rs.rs` 中的 `handle_rs_prepare()`、`handle_rs_update()` 已预留 RS 协议入口，返回 `VmReply::Suspend` 或 `VmReply::Error(ENOSYS)`
- 后续实现 Live Update 需要：状态序列化（考虑 `serde`）、进程槽交换（`swap_proc_slot`）、IPC 过滤（`sys_statectl`）
- `VmServer` 结构体保持不变，Live Update 通过新增 `rs` 模块内部处理
