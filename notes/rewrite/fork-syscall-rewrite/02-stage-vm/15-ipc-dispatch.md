# 15-ipc-dispatch: 主循环与 IPC 分发——VM 的"心脏"

> **分类**: 阶段 6 — 主循环与分发（运行时锚点文档）
> **源码**: `minix3/minix/servers/vm/main.c`（`vm_calls` :47-51 / `CALLNUMBER` :53-59 / 主循环 :112-192 / `CALLMAP` 注册 :522-580）+ `minix3/minix/include/minix/com.h`（`VM_RQ_BASE` :627 / `VM_*` 请求码 :630-773 / `NR_VM_CALLS` :769 / `VM_BASIC_CALLS` :778-780 / `SUSPEND` :1151）+ `minix3/minix/include/minix/vfsif.h`（`TRNS_GET_ID` :79 / `TRNS_ADD_ID` :80 / `TRNS_DEL_ID` :81）+ `minix3/minix/include/minix/ipcconst.h`（`IPC_FLG_MSG_FROM_KERNEL` :28 / `IPC_STATUS_FLAGS_TEST` :34）
> **Rust 模块**: `os/servers/vm/src/ipc/dispatcher.rs`（1986 行：`MessageDispatcher` :99 / `VfsReplyResult` :60 / `DispatchResult` :72 / `dispatch_by_number` :1011）+ `os/servers/vm/src/ipc/transport.rs`（323 行：`IpcTransport` :90 / `KernelIpcTransport` :113 / `TestIpcTransport` :178）+ `os/servers/vm/src/vm_server.rs`（`run` :423 / `dispatch_on_msg` :553 / `rs_handshake` :647 / `handle_vfs_transid` :688 / `reply_to_errno` :1053 / `encode_reply_data` :1100）
> **前置**: `notes/rewrite/fork-syscall-rewrite/02-stage-vm/01-vm-init-main.md`（启动链锚点）+ `02~14` 全部就绪（进程表/ACL/物理内存/页表/区域）
> **说明**: VM 的**运行时心跳**——主循环如何收消息、按五优先级分发、用 `SUSPEND` 协议管理延迟回复、路由 VFS 事务、过滤内核通知、接线 `acl_check`。01 管"进入主循环之前"，本文档管"进入主循环之后"。**不覆盖**：各 handler 实现（16~26）、ACL 数据结构（04）、`do_procctl` 细节（22）、VFS 请求队列（23）、SEF 生命周期细节（01）。

---

## 1. 概念：主循环——VM 的双重身份与分发模型

### 1.0 章节引言

VM 的启动链（01）在 `sef_local_startup()` 之后进入 `while (TRUE)` 主循环（main.c:111-112）。从这一刻起，VM 不再是"把自己初始化好的进程"，而是**系统的内存管理服务端**——每个其他进程的内存操作都变成一条发往 VM 的 IPC 消息。

本文档回答三个问题：

1. **VM 的双重身份是什么**——同步请求服务端 + 异步回调处理器，如何用同一个循环承载（§1.1）。
2. **五优先级分发解决什么问题**——transid 路由、RS 握手、页错误、普通调用、非法请求的裁决顺序（§1.2-§1.5）。
3. **SUSPEND 协议怎么工作**——"稍后回复"如何让异步操作（VFS I/O）与同步 IPC 共存（§1.3）。

它在整个 02-stage-vm 中的位置：

```
01（启动链）→ 02/03（进程表）→ 04（ACL）→ 05~14（内存/页表/区域）
→ ★15（主循环与分发：一切服务的入口）
→ 16~26（各 handler：页错误/CoW/fork/brk/mmap/exit/RS/查询/缓存…）
```

### 1.1 VM 的双重身份

Minix3 微内核里，VM 同时扮演两个角色：

| 身份 | 消息来源 | 语义 | 是否立即回复 |
|------|---------|------|-------------|
| **同步请求服务端** | PM（fork/brk/exit）、VFS（mmap）、RS（特权）、驱动（map_phys/缓存）、用户进程 | 调用者 `ipc_sendrec` 阻塞等待 VM 处理结果 | 是（除 SUSPEND 路径） |
| **异步回调处理器** | 内核（`VM_PAGEFAULT`）、VFS（`VM_VFS_REPLY`） | 内核解除页错误进程阻塞；VFS 完成 VM 发起的文件 I/O | 否（`continue` / `SUSPEND`） |

关键点：**两条路径共用同一个 `sef_receive_status(ANY)` 消息队列**。VM 无法选择"只收同步请求"——内核页错误、VFS 回复随时可能插入。主循环必须能从 `m_type` 一眼区分路径，否则会误回复（例如把页错误消息回给内核）。

### 1.2 五优先级分发模型

C 主循环（main.c:137-176）对每条消息按**固定优先级**裁决，先命中先处理：

| 优先级 | 条件 | 处理 | 回复 |
|--------|------|------|------|
| 1 | `m_source == VFS_PROC_NR` 且 `IS_VFS_FS_TRANSID(transid)` | 剥离 transid → `do_procctl(&msg, transid)` | 正常回复 |
| 2 | `m_type == RS_INIT` 且 `m_source == RS_PROC_NR` | `do_sef_init_request()` → 强制 `SUSPEND` | 不回复（RS 用异步握手） |
| 3 | `m_type == VM_PAGEFAULT` | 校验来自内核 → `do_pagefaults()` → `continue` | 不回复（内核用 `sys_vmctl` 解除阻塞） |
| 4 | `CALLNUMBER(type)` 有效且表项非空 | `acl_check` → `vmc_func(&msg)` | 正常回复 |
| 5 | 其余（越界/空表项/非法请求） | `result` 保持初值 `ENOSYS` | 回复 ENOSYS |

为什么是这个顺序？**优先级从"最需要特殊处理"到"最通用"排列**：VFS 事务消息的 `m_type` 是编码过的（高 16 位调用号 + 低 16 位事务 ID），不能走 CALLNUMBER；RS_INIT 是启动期一次性握手；页错误来自内核、语义上不是"请求-回复"；剩下的才是一般服务调用。

### 1.3 SUSPEND 伪返回码：延迟回复协议

C 的 handler 统一返回 `int`。普通返回值直接写进 `msg.m_type` 发回调用者；但 `SUSPEND (-998)`（com.h:1151，注释 "status to suspend caller, reply later"）是**伪返回码**——主循环检测到后**抑制回复**：

```c
if(result != SUSPEND) {          /* main.c:181 */
    msg.m_type = result;
    ipc_send(who_e, &msg);
}
```

谁用 SUSPEND？**文件映射的缺页**（`do_mmap` 的 file-backed 路径）：VM 需要向 VFS 发起文件 I/O 才能拿到页，但 handler 不能阻塞（VFS 可能正等着 VM 回复别的消息）。于是 handler 返回 SUSPEND、挂起请求、等 VFS 的 `VM_VFS_REPLY` 到达后再由回调完成回复。**调用者一直阻塞**——它看不到 SUSPEND，只看到"这个 IPC 调用还没返回"。

语义契约：SUSPEND 不是错误、不是成功，是**"回复权转移"**——从同步路径转移给异步回调。

### 1.4 VFS transid：事务路由

VFS 向 VM 发消息时，`m_type` 可以是**编码格式**：高 16 位是真实调用号，低 16 位是事务 ID（vfsif.h:79-81）：

```c
#define TRNS_GET_ID(t)		((t) & 0xFFFF)          /* 取低 16 位 = transid */
#define TRNS_ADD_ID(t,id)	(((t) << 16) | ((id) & 0xFFFF))  /* 编码 */
#define TRNS_DEL_ID(t)		((short)((t) >> 16))    /* 取高 16 位 = 调用号 */
```

判定宏（com.h:912）：`IS_VFS_FS_TRANSID(type) = (((type) & ~0xff) == 0xB00)`——transid 落在 `[0xB00, 0xBFF]` 区间即视为 VFS 文件系统事务。主循环先 `TRNS_GET_ID` 提取 transid，命中则剥掉 transid（`TRNS_DEL_ID`）还原调用号，再调 `do_procctl(&msg, transid)`——**transid 跟随请求走，让 VFS 侧能关联回复与事务**。

### 1.5 is_ipc_notify：内核通知过滤

`sef_receive_status()` 的第二个出参 `rcv_sts` 携带消息状态字。内核**通知**（异步信号，如 SEF 的 ping/信号，`is_ipc_notify(status)` = `IPC_STATUS_CALL(status) == NOTIFY`，com.h:92）不是请求也不是回复，主循环直接丢弃：

```c
if (is_ipc_notify(rcv_sts)) {
    printf("VM: ignoring ipc_notify() from %d\n", msg.m_source);
    continue;
}
```

同样地，`VM_PAGEFAULT` 必须来自内核——`IPC_STATUS_FLAGS_TEST(rcv_sts, IPC_FLG_MSG_FROM_KERNEL)`（ipcconst.h:28/:34）验证消息**由内核代表进程发出**（信任标志），否则打印 "faked VM_PAGEFAULT message!" 告警。这是对伪造消息的第一道防线。

### 1.6 对照：Redox 与 Linux

**Redox**：系统调用在**内核侧**用 `syscall` 分发表（`kernel/src/syscall/mod.rs`）按编号路由；服务端收到 `scheme` 消息后自己 `match` 请求类型。与 VM 相同的是"编号→处理函数"的映射思想；不同的是 Redox 的分发表在内核、VM 的在用户态服务内。

**Linux**：系统调用表（`sys_call_table`）是内核态的分发中枢，与 VM 的 `vm_calls[]` 同构；用户态事件循环（`epoll`/`io_uring`）承载"多路复用 + 异步完成"——VM 的 SUSPEND 协议 + 主循环单线程复用，与 `epoll` 的"等待事件、回调完成"是同一模型（只是 IPC 粒度不同：VM 等的是其他服务器的消息，epoll 等的是 fd 事件）。

**演进共性**：分发机制从"**运行时可变的表**"走向"**编译期固定的表 + 类型化协议**"——Linux 的 `sys_call_table` 在启动后只读、`__x64_sys_*` 符号按号绑定；Rust 侧进一步把函数指针表换成 `match`（编译器穷尽检查）。这是 §3 D1 的动机。

### 1.7 小结

主循环是 VM 唯一的事件源：**五优先级裁决**保证特殊路径（transid/RS/页错误）不被通用分发吞掉；**SUSPEND** 让异步操作与同步 IPC 共存；**通知过滤**防止非请求消息进入服务路径。下一章看 C 的完整实现。

---

## 2. C 源码分析

### 2.1 请求码定义（com.h:627-780）

所有 VM 请求码以 `VM_RQ_BASE 0xC00` 为基址（com.h:627），共 49 个（`NR_VM_CALLS` :769）。请求码**稀疏**分布（0/1/2/3/5/10/12~17/26~30/33~37/40~42/44~48 + 0xff），中间留空——`CALLNUMBER` 用 `[0xC00, 0xC00+49)` 连续区间做 0 基索引，空位对应 `vm_calls[]` 空表项（分发时得 ENOSYS）。

| 分组 | 请求码（com.h 行号） | 调用方 |
|------|---------------------|--------|
| PM 调用 | `VM_EXIT` :630 / `VM_FORK` :632 / `VM_BRK` :636 / `VM_EXEC_NEWMEM` :637 / `VM_WILLEXIT` :643 | PM |
| 通用 | `VM_MMAP` :647 / `VM_MUNMAP` :649 / `VM_MAP_PHYS` :677 / `VM_UNMAP_PHYS` :679 / `VM_REMAP` :716 / `VM_SHM_UNMAP` :718 / `VM_GETPHYS` :720 / `VM_GETREF` :722 / `VM_INFO` :729 / `VM_REMAP_RO` :749 / `VM_PROCCTL` :752 / `VM_GETRUSAGE` :764 | 用户/驱动/PM/VFS |
| DMA | `VM_ADDDMA` :656 / `VM_DELDMA` :664 / `VM_GETDMA` :672 | 驱动 |
| 缓存 | `VM_MAPCACHEPAGE` :682 / `VM_SETCACHEPAGE` :685 / `VM_FORGETCACHEPAGE` :688 / `VM_CLEARCACHE` :691 | VFS/驱动 |
| VFS 回调 | `VM_VFS_REPLY` :707 / `VM_VFS_MMAP` :762 | VFS |
| RS 特权 | `VM_RS_SET_PRIV` :724 / `VM_RS_UPDATE` :736 / `VM_RS_MEMCTL` :738 / `VM_RS_PREPARE` :766 | RS |
| 内核专用 | `VM_PAGEFAULT` :773（0xff，区间外，**不进 CALLMAP**） | 内核 |

**`VM_BASIC_CALLS`**（:778-780）：用户进程默认可调用的子集——`VM_BRK, VM_MMAP, VM_MUNMAP, VM_MAP_PHYS, VM_UNMAP_PHYS, VM_INFO, VM_GETRUSAGE`（注释自嘲 "VM_GETRUSAGE is to be removed from this list ASAP"）。ACL 侧把这份清单做成默认权限（见 §3.6）。

### 2.2 分发表：vm_calls[] + CALLNUMBER（main.c:47-59）

```c
/* main.c:47-51 */
struct {
	int (*vmc_func)(message *);	/* Call handles message. */
	const char *vmc_name;		/* Human-readable string. */
} vm_calls[NR_VM_CALLS];

/* main.c:53-59 */
#define CALLNUMBER(c) (((c) >= VM_RQ_BASE && 				\
			(c) < VM_RQ_BASE + ELEMENTS(vm_calls)) ?	\
			((c) - VM_RQ_BASE) : -1)
```

- 表项 = 函数指针 + 人类可读名字（`vmc_name` 用于 ACL 拒绝时的告警打印）。
- `CALLNUMBER` 把 `0xC00+off` 映射为 0 基索引，越界返回 -1。
- **空表项（vmc_func == NULL）与越界同等对待**——main.c:165 `c < 0 || !vm_calls[c].vmc_func` 都走 ENOSYS。这就是"请求码稀疏分布但语义闭合"的实现手段。

### 2.3 CALLMAP 注册（main.c:522-580）

`init_vm()` 最后段（`map_service` 循环之后）注册分发表：

```c
/* main.c:522-534 */
#define CALLMAP(code, func) { int _cmi;		      \
	_cmi=CALLNUMBER(code);				\
	assert(_cmi >= 0);					\
	assert(_cmi < NR_VM_CALLS);		\
	vm_calls[_cmi].vmc_func = (func); 	      \
	vm_calls[_cmi].vmc_name = #code;	      \
}
memset(vm_calls, 0, sizeof(vm_calls));   /* 先清零，空项 = NULL */
```

注册顺序（:536-575）按调用方分组，共 26 项：

| 分组 | 注册项 | 复用 |
|------|--------|------|
| Basic | `VM_MMAP→do_mmap` / `VM_MUNMAP→do_munmap` / `VM_MAP_PHYS→do_map_phys` / `VM_UNMAP_PHYS→do_munmap` | **UNMAP_PHYS 复用 do_munmap** |
| PM | `VM_EXIT→do_exit` / `VM_FORK→do_fork` / `VM_BRK→do_brk` / `VM_WILLEXIT→do_willexit` / `VM_PROCCTL→do_procctl_notrans` | procctl 有无 transid 两个入口 |
| VFS | `VM_VFS_REPLY→do_vfs_reply` / `VM_VFS_MMAP→do_vfs_mmap` | |
| RS | `VM_RS_SET_PRIV→do_rs_set_priv` / `VM_RS_PREPARE→do_rs_prepare` / `VM_RS_UPDATE→do_rs_update` / `VM_RS_MEMCTL→do_rs_memctl` | |
| Generic | `VM_REMAP→do_remap` / `VM_REMAP_RO→do_remap` / `VM_GETPHYS→do_get_phys` / `VM_SHM_UNMAP→do_munmap` / `VM_GETREF→do_get_refcount` / `VM_INFO→do_info` | **REMAP_RO 复用 do_remap**（只读标志区分）、**SHM_UNMAP 复用 do_munmap** |
| Cache | `VM_MAPCACHEPAGE→do_mapcache` / `VM_SETCACHEPAGE→do_setcache` / `VM_FORGETCACHEPAGE→do_forgetcache` / `VM_CLEARCACHE→do_clearcache` | |
| Rusage | `VM_GETRUSAGE→do_getrusage` | |

观察：**"一个请求码一个函数"不是全称**——`do_munmap` 被三个请求码复用（MUNMAP/UNMAP_PHYS/SHM_UNMAP），靠消息字段区分语义（长度从请求带 vs 从区域查 vs 指定进程）；`do_remap` 被两个复用（REMAP/REMAP_RO），靠标志区分只读。注册完成后 `num_vm_instances = 1`、标记 `VMF_VM_INSTANCE`（:577-579）。

### 2.4 主循环全段（main.c:112-192）

```c
/* main.c:112-192，分段注释 */
while (TRUE) {
	int r, c, type, transid = 0;        /* :113-115 */
	SANITYCHECK(SCL_TOP);
	if(missing_spares > 0) alloc_cycle(); /* :118-120 保留页池补充 */
	if ((r=sef_receive_status(ANY, &msg, &rcv_sts)) != OK)  /* :122-123 收消息 */
		panic("sef_receive_status() error: %d", r);
	if (is_ipc_notify(rcv_sts)) {         /* :125-129 丢弃内核通知 */
		printf("VM: ignoring ipc_notify() from %d\n", msg.m_source);
		continue;
	}
	who_e = msg.m_source;                 /* :130-132 验证调用者 */
	if(vm_isokendpt(who_e, &caller_slot) != OK)
		panic("invalid caller %d", who_e);

	assert(!IS_VFS_FS_TRANSID(transid));  /* :135 依赖 transid 初值 0 */

	type = msg.m_type;                    /* :137-141 */
	c = CALLNUMBER(type);
	result = ENOSYS;                      /* 越界/受限调用默认值 */
	transid = TRNS_GET_ID(msg.m_type);

	if((msg.m_source == VFS_PROC_NR) && IS_VFS_FS_TRANSID(transid)) { /* :143-148 P1 */
		msg.m_type = TRNS_DEL_ID(msg.m_type);
		result = do_procctl(&msg, transid);
	} else if(msg.m_type == RS_INIT && msg.m_source == RS_PROC_NR) {   /* :149-152 P2 */
		result = do_sef_init_request(&msg);
		if(result != OK) panic("do_sef_init_request failed!\n");
		result = SUSPEND;                 /* 不回复 RS */
	} else if (msg.m_type == VM_PAGEFAULT) {  /* :153-164 P3 */
		if (!IPC_STATUS_FLAGS_TEST(rcv_sts, IPC_FLG_MSG_FROM_KERNEL))
			printf("VM: process %d faked VM_PAGEFAULT message!\n", msg.m_source);
		do_pagefaults(&msg);
		continue;                         /* 不回复，内核 sys_vmctl 解除阻塞 */
	} else if(c < 0 || !vm_calls[c].vmc_func) {  /* :165-166 P5 越界/空表 */
		/* result 保持 ENOSYS */
	} else {                              /* :167-176 P4 正常调用 */
		if (acl_check(&vmproc[caller_slot], c) != OK) {
			printf("VM: unauthorized %s by %d\n",
					vm_calls[c].vmc_name, who_e);
		} else {
			SANITYCHECK(SCL_FUNCTIONS);
			result = vm_calls[c].vmc_func(&msg);
			SANITYCHECK(SCL_FUNCTIONS);
		}
	}

	if(result != SUSPEND) {               /* :178-191 回复（SUSPEND 除外） */
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

逐个环节的语义：

- **alloc_cycle 钩子（:118-120）**：`missing_spares > 0` 表示保留页池有缺口（06 已述），主循环在**收消息前**给分配器一个补货机会——内存压力不进消息队列。
- **收消息 + 通知过滤（:122-129）**：`sef_receive_status(ANY)` 是 SEF 包装的 `ipc_receive`；`ANY` 表示接受任何来源。通知直接丢弃。
- **caller 验证（:130-132）**：`vm_isokendpt` 把 endpoint 换成进程槽（03）；失败是**编程错误**（任何合法发消息者都应有槽），panic。
- **transid 初值（:135/:141）**：`transid = 0` 且 `assert(!IS_VFS_FS_TRANSID(0))`——0 不在 `[0xB00, 0xBFF]`，断言永真，只是防御"初始值被误判为事务消息"。
- **P1 VFS transid（:143-148）**：先剥 transid 还原调用号，再进 `do_procctl`。注意**只有 `do_procctl` 走这条路**（procctl 的 VMPPARAM_CLEAR/HANDLEMEM 需要事务关联）。
- **P2 RS_INIT（:149-152）**：启动期 RS 发给 VM 的初始化握手（SEF 框架内，01 已述）。成功后强制 SUSPEND——RS 的 `sef_cb_init_response_rs_asyn_once` 用异步方式收回复，避免启动死锁。
- **P3 页错误（:153-164）**：校验 `IPC_FLG_MSG_FROM_KERNEL`（防用户伪造），调 `do_pagefaults`（16 详述），**不回复**——内核通过 `sys_vmctl` 在页错误处理成功后解除进程阻塞，失败则 VM panic。
- **P4 正常调用（:165-176）**：ACL 检查失败只打印告警（`vmc_name` 提供可读性），`result` 保持初值 **ENOSYS**——调用者看到的是"不支持"，不是 EPERM；成功则调 handler。`SANITYCHECK(SCL_FUNCTIONS)` 包住 handler，异常时输出调用栈（A-7 cfg 替代）。
- **回复（:178-191）**：`result != SUSPEND` 才回复；`msg.m_type = result` 把 errno 写回消息类型位；再 `assert(!IS_VFS_FS_TRANSID(transid))` 保证回复消息类型干净。`ipc_send` 失败 panic——回复丢失是致命错误。

### 2.5 transid 宏与 SUSPEND（vfsif.h:79-81 / com.h:911-912 / com.h:1151）

| 宏 | 定义 | 语义 |
|----|------|------|
| `TRNS_GET_ID(t)` | `((t) & 0xFFFF)` | 低 16 位 = 事务 ID |
| `TRNS_ADD_ID(t,id)` | `(((t) << 16) \| ((id) & 0xFFFF))` | 调用号左移 16 + 事务 ID 编码 |
| `TRNS_DEL_ID(t)` | `((short)((t) >> 16))` | 高 16 位 = 调用号（short 符号扩展） |
| `IS_VFS_FS_TRANSID(type)` | `(((type) & ~0xff) == 0xB00)` | transid 是否落在事务区间 |
| `SUSPEND` | `-998` | "status to suspend caller, reply later" |

编码形态：VFS 发 `VM_PROCCTL` 事务时 `m_type = (VM_PROCCTL << 16) | (0xB00 | fs_transid)`。VM 侧 `TRNS_GET_ID` 提取 `0xB00|id` 命中 `IS_VFS_FS_TRANSID`，`TRNS_DEL_ID` 还原 `VM_PROCCTL`。

### 2.6 RS 握手在 SEF 中的形态（main.c:219-260）

主循环 P2 分支调 `do_sef_init_request`，真正的工作在 SEF 回调 `sef_cb_init_fresh`（:241-260）：

```c
static int sef_cb_init_fresh(int type, sef_init_info_t *info)
{
	/* 1. 从 RS 地址空间拷贝 rproctab（sys_safecopyfrom） */
	sys_safecopyfrom(RS_PROC_NR, info->rproctab_gid, 0, rprocpub, sizeof(rprocpub));
	/* 2. 对每个 in_use 的 boot 服务调 map_service（ACL 授权，04 已述） */
	for(i=0;i < NR_BOOT_PROCS;i++) if(rprocpub[i].in_use) map_service(&rprocpub[i]);
}
```

`sef_local_startup`（:219-239）注册 init/LU/restart/signal 回调后 `sef_startup()`。Rust 侧没有 SEF 框架（A-8），用 `rs_handshake()` 直接复刻这两步（§3.4 D4）。

### 2.7 客户端-服务端对应关系

客户端侧是 `libsys` 的 IPC 封装（不在 VM rewrite 范围），服务端侧是 CALLMAP handler：

| 客户端库函数 | IPC 请求 | 服务端 handler | 返回值 |
|-------------|---------|---------------|--------|
| `alloc_contig()` | `VM_MMAP` | `do_mmap` | 虚拟地址 |
| `free_contig()` | `VM_MUNMAP` | `do_munmap` | OK/错误 |
| `vm_map_phys()` | `VM_MAP_PHYS` | `do_map_phys` | 虚拟地址 |
| `vm_unmap_phys()` | `VM_UNMAP_PHYS` | `do_munmap` | OK/错误 |
| `brk()` | `VM_BRK` | `do_brk` | 新 brk 地址 |
| `mmap()` | `VM_MMAP` | `do_mmap` | 映射地址 |
| `munmap()` | `VM_MUNMAP` | `do_munmap` | OK/错误 |
| `vm_map_cacheblock()` | `VM_MAPCACHEPAGE` | `do_mapcache` | 缓存块地址 |
| `vm_forget_cacheblock()` | `VM_FORGETCACHEPAGE` | `do_forgetcache` | OK/错误 |
| `vm_clear_cache()` | `VM_CLEARCACHE` | `do_clearcache` | OK/错误 |

客户端用同步 `ipc_sendrec`（`_taskcall`），调用者阻塞直到 VM 回复（或永不回复——SUSPEND 路径由回调收尾）。

---

## 3. Rust 设计决策

### 3.1 D1：编译期 CALLMAP——dispatch_by_number（dispatcher.rs:1011-1188）

**C 方案**：`vm_calls[]` 运行时函数指针表 + `CALLNUMBER` 索引 + `vmc_func(&msg)`；表项可为 NULL，越界/空表在运行时才暴露。

**Rust 方案**：`dispatch_by_number(call_nr, msg, server) -> DispatchResult`，`match` 每个 `VM_* - VM_RQ_BASE`：

```rust
match call_nr {
    _c if _c == VM_MMAP as usize - vm_rq_base =>
        Self::dispatch_mmap(table, page_alloc, frames, VmMmapIn::decode(m1)).into(),
    _c if _c == VM_FORK as usize - vm_rq_base =>
        Self::dispatch_fork(table, page_alloc, frames, VmForkIn::decode(m1)).into(),
    // … 全部分支 …
    _ => DispatchResult::from_reply(VmReply::Error(VmError::NotImplemented)),
}
```

**理由**：

- 编译期穷尽——每个请求码有显式分支，`_` 兜底只接"显式排除"的项（DMA 三请求），杜绝"空表项静默 ENOSYS"的运行时不确定性。
- 类型化解码——每个分支用对应的 `VmXxxIn::decode(m1/m2)` 把 `Message` 变成类型化请求，handler 不再面对万能 `message*`。
- 保留 C 的职责边界——ACL 检查在 `dispatch_on_msg` 层（main.c:165-176 的位置），`dispatch_by_number` 不重复检查。

### 3.2 D2：类型化回复三态——VmReply / DispatchAction / VmReplyForIpc

**C 方案**：handler 返回 `int`，`SUSPEND (-998)` 是魔术值；主循环 `result != SUSPEND` 判断。

**Rust 方案**：三层类型逐步收紧"是否回复"：

```
VmReply（服务返回值，含 Suspend）
  → DispatchAction（主循环动作：Reply(Suspend) 是逻辑错误）
  → VmReplyForIpc（new() 对 Suspend 返回 None，编译期排除）
```

```rust
enum DispatchAction { Reply(VmReply), Suspend, NoReply }   /* vm_server.rs:488-491 */

struct VmReplyForIpc { /* :522 */ }                          /* 只含非 Suspend 载荷 */
impl VmReplyForIpc {
    fn new(reply: VmReply) -> Option<Self> {                 /* :530 */
        match reply { VmReply::Suspend => None, other => Some(..) }
    }
}
```

`reply_to_errno`（:1049）对 `VmReply::Suspend` 有 `unreachable!` 臂 + 修复提示——如果未来重构把 Suspend 误路由进回复路径，panic 信息直接指出"dispatch_on_msg 应转成 DispatchAction::Suspend"，而不是晦涩的断言。

### 3.3 D3：IpcTransport 策略 trait（transport.rs:90-96）

**C 方案**：`sef_receive_status(ANY)` / `ipc_send()` 自由函数，链接 `libsys.a` 时绑定内核 IPC。

**Rust 方案**：trait + 两个 impl：

| Impl | 用途 | 现状 |
|------|------|------|
| `KernelIpcTransport`（:113） | 生产 | `receive`/`send` 主体 `unimplemented!("wiring pending kernel IPC core")`——内核 IPC 原语未就绪，`initialized` 门控给出清晰错误而非旧版 `Err(())` |
| `TestIpcTransport`（:178） | `#[cfg(test)]` | 队列式 mock：`queue_receive` 预置消息、`sent` 记录每次 `send`——单测可驱动主循环 |

选择器 `ipc_transport_for_build()`（:243）按 `cfg(test)` 返回对应实现；`vm_server.rs` 用进程全局 `IPC_TRANSPORT_PTR`（AtomicPtr 惰性初始化，:831-866）持有 trait 对象。

### 3.4 D4：五优先级 dispatch_on_msg（vm_server.rs:553-641）

`run()` 循环体把 C 的 if-else 链（main.c:137-176）提炼成 `dispatch_on_msg`：

| C 优先级 | Rust 分支 | 关键差异 |
|---------|----------|---------|
| P1 VFS transid | `source == VFS_PROC_NR && is_vfs_fs_transid(m_type)` → `handle_vfs_transid` | 校验 clean_type==VM_PROCCTL + transid≠0（C 无显式校验，直接 do_procctl） |
| P2 RS_INIT | `m_type == RS_INIT && source == RS_PROC_NR` → `rs_handshake()` + `DispatchAction::Suspend` | `rs_handshake` 复刻 `sef_cb_init_fresh`（rproctab + map_service），SEF 框架 DEFERRED（A-8） |
| P3 VM_PAGEFAULT | `debug_assert!(is_from_kernel)` + `dispatch_pagefault` + `NoReply` | 生产用 debug_assert（C 是运行时告警） |
| P4 正常调用 | `callnr()` → `acl_check` → `dispatch_by_number` | ACL 拒绝 → 回复 ENOSYS（保持 C result 初值语义） |
| P5 兜底 | `DispatchAction::Reply(NotImplemented)` | = ENOSYS |

### 3.5 D5：VFS 回调延迟执行（dispatcher.rs:66-96）

**C 方案**：`do_vfs_reply`（vfs.c:109）内联调用 `req_callback`。

**Rust 方案**：借位检查器不允许同时持有 `&mut VfsRequestQueue`（`parts_mut` 已借出）和 `&mut VmServer` 调回调。`dispatch_vfs_reply` 返回 `VfsReplyResult { reply, callback }`，主循环在 dispatch 返回后、借用释放时执行：

```rust
if let Some((callback, reply, state)) = result.vfs_callback {
    let _ = callback(self, &reply, &state);     /* vm_server.rs:631-633 */
}
```

语义等价：回调最终执行，只是从"内联"变成"延迟到借用边界"。

### 3.6 D6：acl_check 接线 + ENOSYS 语义保持

调用点（vm_server.rs:589-621）：

```rust
if let Some(proc) = table.get_active(caller_slot) {
    if proc.acl_check(c as u32).is_err() {
        /* 审计通道：cfg(test) 或 feature vm_acl_audit 时 eprintln! */
        return DispatchAction::Reply(VmReply::Error(VmError::NotImplemented));
    }
}
```

- `proc.acl_check`（vmproc_handle.rs:246-248）转发 `AclState::acl_check`（acl.rs:96-131）——三态（Uninitialized/Default/System(mask)）查 `1u64 << call` 位。
- **ENOSYS 语义保持**（04 已修）：C 中 ACL 拒绝路径不改 `result` 初值（main.c:139 `result = ENOSYS`），调用者看到 ENOSYS 而非 EPERM；Rust 用 `VmError::NotImplemented` 精确复刻，`AclState::acl_check` 内部仍返回 EPERM（= C 的 acl_check 返回值）。
- VM 自身端点豁免：`endpoint == Endpoint::VM` 直接放行（acl.rs:97-99）。

### 3.7 差异清单（C ↔ Rust，诚实标注）

| # | C 行为 | Rust 行为 | 性质 |
|---|--------|----------|------|
| 1 | `RS_INIT` = 0x714（com.h:478） | 修复前 `const RS_INIT = 0x606`（vm_server.rs:973）——优先级 2 永不匹配 | **本轮 P0 修复**（0x606→0x714，cargo check 通过，0x606 残留 0） |
| 2 | SEF 框架（sef_local_startup/sef_startup） | `rs_handshake()` 直接复刻 sef_cb_init_fresh 两步；SEF 生命周期 DEFERRED | A-8 缺口契约 |
| 3 | `is_ipc_notify(rcv_sts)` 读状态字 | 桩函数恒 false（vm_server.rs:884）——状态字未接 kernel IPC | 待 kernel IPC core |
| 4 | `IPC_FLG_MSG_FROM_KERNEL` 运行时校验 + 告警 | `debug_assert!(is_from_kernel)`（恒 true 桩） | 生产路径未达（KernelIpcTransport unimplemented） |
| 5 | `do_procctl(&msg, transid)` 直接调 | `handle_vfs_transid` 显式校验 clean_type/transid 后调 `dispatch_procctl` | 防御增强 |
| 6 | VFS 回复内联回调 | 回调延迟到主循环借用边界执行 | borrow checker 驱动，语义等价 |
| 7 | `vm_calls[c].vmc_name` 告警字符串 | 审计通道 eprintln!（feature-gated） | 等 syslog IPC |
| 8 | `msg.m_type = result` 单一编码 | `reply_to_errno` + `encode_reply_data` 分离（errno 与载荷） | 类型化编码，载荷字段显式 |

---

## 4. 实现详解

### 4.1 run() 主循环（vm_server.rs:423-481）

```rust
pub fn run(&mut self) -> ! {
    assert!(self.initialized, "VmServer::run() called before init()");
    loop {
        // C: if(missing_spares > 0) alloc_cycle();
        if self.missing_spares > 0 { self.alloc_cycle(); }

        // C: sef_receive_status(ANY, &msg, &rcv_sts)
        let (msg, rcv_sts) = match ipc_receive() { Ok(v) => v, Err(_) => panic!(..) };

        // C: if(is_ipc_notify(rcv_sts)) { continue; }
        if is_ipc_notify(&rcv_sts) { continue; }

        // C: who_e = msg.m_source; vm_isokendpt(who_e, &caller_slot);
        let caller_slot = match VmProcTable::get_global().vm_isokendpt(msg.m_source) {
            Ok(slot) => slot, Err(_) => panic!("invalid caller {:?}", msg.m_source),
        };

        let action = self.dispatch_on_msg(&msg, &rcv_sts, caller_slot);

        match action {
            DispatchAction::Reply(reply) => {
                let reply_for_ipc = VmReplyForIpc::new(reply)
                    .expect("DispatchAction::Reply carries VmReply::Suspend; …");
                let code = reply_to_errno(reply_for_ipc.payload());
                let mut reply_msg = msg.clone();
                reply_msg.m_type = code;
                encode_reply_data(reply_for_ipc.into_payload(), &mut reply_msg);
                ipc_send(who_e, &reply_msg).unwrap_or_else(|_| panic!("ipc_send() failed"));
            }
            DispatchAction::Suspend => {}
            DispatchAction::NoReply => {}
        }
    }
}
```

与 C 主循环逐句对应：alloc_cycle 钩子（main.c:118-120）、收消息（:122-123）、通知过滤（:125-129）、caller 验证（:130-132）、五优先级（:137-176）、SUSPEND 抑制回复（:178-191）。**回复编码两段式**：`reply_to_errno` 决定 `m_type`（errno），`encode_reply_data` 填载荷字段（C 的 handler 直接写 `msg` 字段）。

### 4.2 dispatch_on_msg 五优先级（vm_server.rs:553-641）

```rust
fn dispatch_on_msg(&mut self, msg: &Message, rcv_sts: &IpcStatus,
                   caller_slot: UserSlot) -> DispatchAction {
    // P1: VFS transid（main.c:143-148）
    if source == VFS_PROC_NR && is_vfs_fs_transid(m_type) {
        let transid = transid_extract(m_type);
        let clean_type = transid_strip(m_type);
        let result = self.handle_vfs_transid(clean_type, transid, msg);
        return DispatchAction::Reply(result);
    }
    // P2: RS_INIT（main.c:149-152）
    if m_type == RS_INIT && source == RS_PROC_NR {
        self.rs_handshake().expect("rs_handshake failed");
        return DispatchAction::Suspend;
    }
    // P3: VM_PAGEFAULT（main.c:153-164）
    if m_type == VM_PAGEFAULT {
        debug_assert!(is_from_kernel(rcv_sts), "faked VM_PAGEFAULT from {:?}", source);
        let _ = self.dispatch_pagefault(msg);
        return DispatchAction::NoReply;
    }
    // P4: 正常调用（main.c:165-176）
    if let Some(c) = callnr(m_type) {
        // acl_check（main.c:160-162）；拒绝 → ENOSYS
        …
        let result = MessageDispatcher::dispatch_by_number(c, msg, self);
        if let Some((callback, reply, state)) = result.vfs_callback { … }
        return match result.reply {
            VmReply::Suspend => DispatchAction::Suspend,
            other => DispatchAction::Reply(other),
        };
    }
    // P5: 越界（main.c:165-166）
    DispatchAction::Reply(VmReply::Error(VmError::NotImplemented))
}
```

transid 辅助函数（vm_server.rs:901-930）逐字复刻 C 宏：

| Rust | C | 语义 |
|------|---|------|
| `is_vfs_fs_transid` :887 | `IS_VFS_FS_TRANSID` | `(m_type & !0xFF) == 0xB00` |
| `transid_extract` :899 | `TRNS_GET_ID` | `m_type & 0xFFFF` |
| `transid_strip` :909 | `TRNS_DEL_ID` | `((m_type >> 16) as i16) as u32`（符号扩展保留） |
| `callnr` :984 | `CALLNUMBER` | `checked_sub(VM_RQ_BASE)` + `< NR_VM_CALLS` |

### 4.3 handle_vfs_transid（vm_server.rs:688-741）

```rust
fn handle_vfs_transid(&mut self, clean_type: u32, transid: i32, msg: &Message) -> VmReply {
    if clean_type != VM_PROCCTL { return VmReply::Error(VmError::InternalError); }
    if transid == 0 { return VmReply::Error(VmError::InvalidProcess); }
    let request = VmProcctlIn::decode(m1);
    let caller = VFS_PROC_NR;                      /* 入口已保证 source==VFS */
    MessageDispatcher::dispatch_procctl(table, &mut self.page_alloc, frames, caller, request)
}
```

C 的 `do_procctl(&msg, transid)` 在 Rust 拆成"**前置校验 + 委托 dispatch_procctl**"：clean_type 必须 VM_PROCCTL（C 隐式约定：只有 procctl 走 transid 路径）、transid 非零（对应 C main.c:135 断言）。`dispatch_procctl` 本体（dispatcher.rs:213-290）按 VMPPARAM_CLEAR/HANDLEMEM 分发（22 详述）。

### 4.4 dispatch_by_number 全分支（dispatcher.rs:1011-1188）

`dispatch_by_number` 用 `server.parts_mut()` 一次性取出四个可变组件，逐分支 decode 后委托：

| 请求码 | decode | 委托 | 状态 |
|--------|--------|------|------|
| `VM_MMAP` | `VmMmapIn::decode(m1)` | `dispatch_mmap` | 已实现（文件映射可 Suspend） |
| `VM_MUNMAP` | `VmMunmapIn::decode(m1)` | `dispatch_munmap` | 已实现 |
| `VM_MAP_PHYS` | `VmMapPhysIn::decode(m1)` | `dispatch_map_phys` | 已实现 |
| `VM_EXIT` / `VM_FORK` / `VM_BRK` / `VM_WILLEXIT` | 对应 `In` 类型 | `dispatch_exit/fork/brk/willexit` | 已实现 |
| `VM_VFS_MMAP` | `VmVfsMmapIn::decode(m1)` | `dispatch_vfs_mmap` | 已实现 |
| `VM_MAPCACHEPAGE` / `VM_SETCACHEPAGE` | `VmCacheIn::decode(m1)` | `dispatch_mapcache/setcache` | 已连接（部分实现） |
| `VM_FORGETCACHEPAGE` / `VM_CLEARCACHE` | `VmCacheIn::decode(m1)` | `dispatch_forgetcache/clearcache` | 已实现 |
| `VM_RS_SET_PRIV` | M2 `m2i1/m2l1/m2i2` | `dispatch_rs_set_priv` | 已连接（mask 传 None，fail-closed） |
| `VM_RS_PREPARE` / `VM_RS_UPDATE` | M2 `m2i1/m2i2/m2i3` | `dispatch_rs_prepare/update` | 已连接（部分实现） |
| `VM_RS_MEMCTL` | M1+M2 混合 | `dispatch_rs_memctl` | 已连接（部分实现） |
| `VM_GETPHYS` / `VM_GETREF` | M1 | `dispatch_get_phys/get_refcount` | 已连接（部分实现） |
| `VM_INFO` | M2 `what/ep/count/next` | `dispatch_info` | 已连接（部分实现） |
| `VM_GETRUSAGE` | M2 | `dispatch_getrusage` | 已连接（部分实现） |
| `VM_SHM_UNMAP` | M1 `m1i1/m1p1` | `dispatch_shm_unmap` | 已连接（fail-closed） |
| `VM_REMAP` / `VM_REMAP_RO` | `VmRemapIn::decode(m1)` | `dispatch_remap/remap_ro` | 已连接（部分实现） |
| `VM_PROCCTL` | `VmProcctlIn::decode(m1)` | `dispatch_procctl` | 已连接（VFS transid 路径） |
| `VM_VFS_REPLY` | `VmVfsReplyIn::decode(m1)` | `dispatch_vfs_reply` | 已连接（返回 Suspend + 延迟回调） |
| `VM_EXEC_NEWMEM` | — | `_` 兜底（`dispatch_exec_newmem` stub 未接线） | 占位（孤儿 stub，见 §4.5） |
| `VM_ADDDMA` / `VM_DELDMA` / `VM_GETDMA` | — | `_` 兜底 | 显式排除（DMA 表未实现） |

### 4.5 特殊路径：exec_newmem 与 pagefault 不进 dispatch_by_number

- **`VM_EXEC_NEWMEM`**：`dispatch_by_number` **无**该分支——请求落到 `_` 兜底返回 NotImplemented（fail-closed）。`dispatch_exec_newmem`（dispatcher.rs:845）是**孤儿 stub**：架构注释（dispatcher.rs:805-846）说明真实 handler 未来应放 `VmServer` 层（exec-newmem 需要 `&mut self` 全组件访问、跨进程态），接线方案（dispatch_by_number 分支 vs 主循环截获）待定。
- **`VM_PAGEFAULT`**：`callnr()` 对 `0xCFF` 返回 None（越界），根本进不了 dispatch_by_number——P3 分支先截获。`dispatch_pagefault`（vm_server.rs:742-797）在 `VmServer` 层持有 `&mut self`，委托 `cow_exec_pf::handle_pagefault`（16 详述）。

### 4.6 reply 编码（vm_server.rs:1049-1140）

`reply_to_errno` 全变体显式列出的原因：**新增 VmReply 变体必须在此处补编码分支，否则编译错误**。`VmError::to_errno()`（minix-types vm.rs:610-628）做 errno 映射（EINVAL/ESRCH/ENOMEM/EFAULT/EPERM/EACCES/EIO/ENOSYS/ENOENT）。`encode_reply_data` 对每个带载荷变体写 M1 字段（fork 子进程 endpoint、brk 新地址、mmap 地址、Info* 统计等）。

### 4.7 IpcTransport 接线（vm_server.rs:825-895）

进程全局 `IPC_TRANSPORT_PTR: AtomicPtr<KernelIpcTransport>` 惰性初始化一次，转换为 trait 对象；`ipc_receive`/`ipc_send` 是薄包装。测试侧 `TestIpcTransport` 记录 `sent` 列表，单测可断言主循环"回给谁、回了什么"。

---

## 5. 测试要点

### 5.1 单元测试清单（grep 实证，2026-08-16）

**dispatcher.rs**（26 个）：

| 测试 | 位置 | 契约 |
|------|------|------|
| test_vm_error_invalid_process_maps_to_einval / invalid_endpoint_maps_to_esrch / slot_in_use_maps_to_einval | :1465/:1473/:1481 | VmError→errno 映射（EINVAL/ESRCH） |
| test_dispatch_procctl_rejects_negative_param / zero_who | :1519/:1539 | 非法参数 fail-closed |
| test_dispatch_procctl_clear_rejects_unauthorized_caller | :1558 | VMPPARAM_CLEAR 仅 RS/VFS |
| test_dispatch_procctl_handlemem_rejects_non_vfs_caller | :1579 | VMPPARAM_HANDLEMEM 仅 VFS |
| test_dispatch_procctl_unknown_param_returns_invalid_address | :1600 | 未知 param → EINVAL |
| test_dispatch_remap_rejects_zero_vaddr / zero_length / invalid_endpoints | :1620/:1640/:1658 | remap 输入校验 |
| test_dispatch_remap_ro_rejects_invalid_endpoints | :1677 | remap_ro 校验 |
| test_dispatch_vfs_reply_rejects_zero_reqid / no_active_request / negative_reqid | :1695/:1715/:1736 | vfs_reply fail-closed |
| test_dispatch_forgetcache_rejects_zero_pages / unaligned_offset / valid_input | :1756/:1775/:1794 | forgetcache 校验 |
| test_dispatch_setcache_rejects_zero_pages / zero_dev_and_ino / unaligned_dev_offset / invalid_caller | :1815/:1835/:1855/:1875 | setcache 校验 |
| test_dispatch_mapcache_rejects_unaligned_offset / zero_pages / invalid_caller / cache_miss_returns_not_found | :1898/:1919/:1940/:1962 | mapcache 校验 + ENOENT |

**transport.rs**（6 个）：

| 测试 | 位置 | 契约 |
|------|------|------|
| kernel_transport_uninitialized_returns_unimplemented | :260 | 未初始化 → Unimplemented |
| kernel_transport_send_to_none_is_invalid | :269 | NONE endpoint → InvalidEndpoint |
| test_transport_empty_returns_unimplemented | :278 | 空队列 → Unimplemented |
| test_transport_queue_then_receive | :285 | 队列预置 + 取空 |
| test_transport_send_is_recorded | :299 | sent 记录 |
| test_transport_should_fail_flag | :313 | should_fail 强制失败 |

**vm_server.rs**（主循环/transid 相关 7 个 + 生命周期）：

| 测试 | 位置 | 契约 |
|------|------|------|
| test_is_vfs_fs_transid_valid / invalid | :1454/:1464 | 0xB00 区间判定 |
| test_transid_extract | :1473 | TRNS_GET_ID 等价 |
| test_transid_strip | :1482 | TRNS_DEL_ID 等价（含符号扩展） |
| test_handle_vfs_transid_wrong_clean_type / zero_transid / invalid_endpoint | :1496/:1509/:1522 | P1 路径前置校验 |
| test_vm_server_run_without_init | :1342 | run() 前必须 init |
| test_missing_spares_pressure_counter | :1350 | alloc_cycle 压力钩子 |

### 5.2 覆盖维度

- **错误映射**：VmError → errno 全表（EINVAL/ESRCH + 各服务错误）。
- **fail-closed 输入校验**：procctl/remap/vfs_reply/cache 四族的非法输入拒绝。
- **transid 位运算**：GET/STRIP 与 C 宏逐位等价（含 i16 符号扩展）。
- **transport 状态机**：未初始化/空队列/NONE endpoint/should_fail。
- **权限分离**：VMPPARAM_CLEAR 仅 RS/VFS、HANDLEMEM 仅 VFS。

### 5.3 覆盖缺口与诚实标注

| 缺口 | 状态 | 说明 |
|------|------|------|
| dispatch_on_msg 五优先级直接单测 | ⚠️ 缺失 | run()/dispatch_on_msg 无端到端测试——TestIpcTransport 已就绪（transport.rs:178）但未接线驱动主循环（04-P2-10 backlog：分发拒绝回复端到端） |
| RS_INIT 分支测试 | ⚠️ 缺失 | rs_handshake 依赖 ipc_call_rs_init 桩（vm_server.rs:932/:965 返回 RprocTab::empty()），无消息级测试 |
| VM_PAGEFAULT 分支测试 | ⚠️ 缺失 | is_from_kernel 桩恒 true（vm_server.rs:889），真实状态字未接 kernel IPC |
| is_ipc_notify 桩 | ⚠️ 缺失 | 恒 false（vm_server.rs:884），等 kernel IPC 状态字；TestIpcTransport 可带 IpcStatus 但主循环未消费 |
| acl_check 拒绝路径单测 | ⚠️ 部分 | AclState::acl_check 有单测（acl.rs:200+），但 dispatch_on_msg 层"拒绝→ENOSYS 回复"无直接测试 |
| DMA 三请求 | ⚠️ 显式排除 | dispatch_by_number `_` 兜底 NotImplemented；C 有 do_adddma 等（DMA 表 DEFERRED） |
| VFS transid 编码端到端 | ⚠️ 缺失 | TRNS_ADD_ID 编码（VFS 侧）不在 VM 测试范围；仅单向 GET/STRIP 解码 |

### 5.4 测试统计（截至 2026-08-16）

```
$ cd os && cargo test -p minix-vm --lib
→ 360 passed / 1 failed（test_map_lazy pre-existing，13 范围，§5.3 已标注）
$ cargo test -p minix-vm --lib ipc::dispatcher → 26 passed
$ cargo test -p minix-vm --lib ipc::transport → 6 passed
$ cargo test -p minix-vm --lib vm_server → 21 passed（含 transid/生命周期）
$ cargo check -p minix-vm → Finished（110 warnings pre-existing，无 error）
```

---

## 6. 过渡

15 完成主循环与分发——02-stage-vm 从此有了"运行时心脏"。各服务 handler 全部挂在 CALLMAP 上，被 `dispatch_by_number` 按请求码送进对应实现：

- **16-pagefault**：P3 分支的 `dispatch_pagefault` → `cow_exec_pf::handle_pagefault`（页错误状态机）。
- **17-cow-mechanism / 18-vm-fork**：P4 分支的 `dispatch_fork`（fork 全流程）。
- **19-vm-brk / 20-vm-mmap / 21-vm-munmap**：P4 分支的 brk/mmap/munmap 服务（含 file-backed Suspend 路径）。
- **22-vm-exit**：P1 分支的 `handle_vfs_transid` → `dispatch_procctl` + P4 的 `dispatch_exit/willexit`。
- **23-vfs-interaction**：P4 的 `dispatch_vfs_reply` 延迟回调 + `DispatchResult.vfs_callback` 执行点。
- **24-page-cache / 25-rs-services / 26-vm-queries**：P4 分支的 cache/RS/查询服务。
- **27 起**：无新增分发面，全部消费已有 handler。

位置可回答性：本文档的主循环位于 **`main()` 内 `sef_local_startup()` 之后**（main.c:111），是 VM 启动链（01）的终点、一切运行时服务的起点。`dispatch_by_number` 的分支表随 handler 文档（16~26）逐步从"已连接/部分实现"变为"已实现"。

---

## 7. 参见

- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/01-vm-init-main.md` — 启动链与 SEF（主循环前置；CALLMAP 注册位置 main.c:522-580）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/04-acl.md` — ACL 三态与权限位（acl_check 接线）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/03-vmproc-table.md` — vm_isokendpt / caller 槽验证
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/16-pagefault.md` — P3 分支 handler
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/17-cow-mechanism.md`、`18-vm-fork.md`、`19-vm-brk.md`、`20-vm-mmap.md`、`21-vm-munmap.md`、`22-vm-exit.md`、`23-vfs-interaction.md`、`24-page-cache.md`、`25-rs-services.md`、`26-vm-queries.md` — P4 分支各 handler
- `minix3/minix/servers/vm/main.c`（`vm_calls` :47-51 / 主循环 :112-192 / `CALLMAP` :522-580）— C 主循环
- `minix3/minix/include/minix/com.h`（`VM_RQ_BASE` :627 / `VM_*` :630-773 / `SUSPEND` :1151）— 请求码与 SUSPEND
- `minix3/minix/include/minix/vfsif.h`（:79-81）、`minix3/minix/include/minix/ipcconst.h`（:28/:34）— transid 与 IPC 状态宏
- `os/servers/vm/src/ipc/dispatcher.rs`、`os/servers/vm/src/ipc/transport.rs`、`os/servers/vm/src/vm_server.rs` — Rust 实现
