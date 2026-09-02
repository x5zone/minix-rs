# 01-pm-init-main: 启动入口与初始化骨架

> **分类**: 阶段 1 — 启动与进程模型（锚点文档）
> **源码**: `minix3/minix/servers/pm/main.c`（main/sef_local_startup/sef_cb_init_fresh/reply/get_nice_value/handle_vfs_reply）、`minix3/minix/servers/pm/schedule.c:36-69`（sched_init 调用点）、`minix3/minix/lib/libsys/sef.c` + `sef_init.c`（SEF 框架）
> **Rust 模块**: `os/servers/pm/src/main.rs`、`os/servers/pm/src/init.rs`（`PmServer`/`BootParams`/`VfsPmInit`）、`os/servers/pm/src/ipc/transport.rs`（`IpcTransport`）
> **前置**: `notes/rewrite/fork-syscall-rewrite/04-stage-pm/00-pm-overview.md`、`notes/rewrite/fork-syscall-rewrite/01-stage-kernel/06-proc-init-boot-proc.md`（boot image 来源）
> **说明**: PM 从 `main()` 入口到进入主循环之前的全部启动链：SEF 回调注册、`sef_cb_init_fresh` 八步初始化、boot image 填充（INIT + 系统进程）、VFS_PM_INIT 进程表同步、`system_hz`、`sched_init` 调用点。主循环分发细节在 `04-ipc-dispatch.md`。

---

## 1. 概念：启动链——PM 如何建立"第一代进程世界"

### 1.0 章节引言

**目标读者**：已理解 Minix3 微内核基本结构（内核/系统服务/用户进程分层、IPC 消息传递）与 PM 总览（`00-pm-overview.md`）的开发者。本档假设读者已从 `01-stage-kernel/06-proc-init-boot-proc.md` 知道 boot image 是内核在启动早期实例化的"编译时进程清单"。

> **本章不讲什么**:
> - `mproc` 结构字段语义与 19 个 flag 正交位（`02-mproc-struct.md`）
> - 进程表查找、endpoint 验证、PID 生成器（`03-mproc-table.md`）
> - 主循环的三路分发与 SUSPEND 回复模型（`04-ipc-dispatch.md`）
> - VFS 异步回复状态机（`05-vfs-interaction.md`）
> - 信号集合的语义（core/ign/noign 如何被信号代码消费）（`11-signal-core.md`）
> - 用户态调度协议（`sched_start`/`SCHEDULING_START`）（`16-scheduling.md`）
>
> 本章只回答一个问题：**PM 按什么顺序、为什么按这个顺序，把自己初始化成"进程语义的权威"**。

### 1.1 PM 在启动链中的位置：进程语义权威

内核完成自举后，按 boot image 启动 PM 与 VFS（`01-stage-kernel/06-proc-init-boot-proc.md`）。PM 是**第一个获得"进程语义"的用户态服务**——它是进程表（`mproc`）的唯一初始来源、信号管理器（`process_ksig`）、进程生命周期的裁判。其他服务（VFS/VM）的进程表都从 PM 这里同步。

这带来一个核心问题：**PM 的初始状态从哪来？** 内核知道全部 boot 进程的清单（名字、槽位、endpoint、内存布局），PM 不知道——它必须向内核"要"这份清单（`sys_getimage`），而不是自己造。这是本档全部内容的主线：

```
内核是 boot 进程清单的唯一权威（boot image）
    │ sys_getimage() 拷贝
    ▼
PM 把清单实例化为 mproc 表（INIT + 系统进程）
    │ VFS_PM_INIT 逐条同步
    ▼
VFS 的 fproc 表与 PM 对齐（两张进程表一致）
    │ sched_init()
    ▼
INIT 的用户态调度接管（SCHED 服务接管，此后调度语义归 16）
```

这条链的每一步都建立在前一步之上：**先有权威数据（boot image），再有权威状态（mproc 表），再有对端一致（VFS），再有运行时能力（调度）**。

### 1.2 SEF 生命周期：框架如何把"初始化"交给 PM

PM 的启动被 Minix3 的 SEF（System Event Framework，`minix3/minix/lib/libsys/sef*.c`）框架包裹。SEF 是**所有系统服务共享的服务生命周期协议**：服务注册回调，SEF 在正确时机调用它们。PM 注册三个回调：

| SEF 回调 | 注册函数（main.c） | PM 的处理 |
|---------|------------------|----------|
| `sef_cb_init_fresh` | `sef_setcb_init_fresh`（main.c:118） | 本档主题：全新启动的完整初始化 |
| `sef_cb_init_restart` | `sef_setcb_init_restart(SEF_CB_INIT_RESTART_STATEFUL)`（main.c:119） | 状态恢复（Live Update/重启；本档只记注册点） |
| `sef_cb_signal_manager` | `sef_setcb_signal_manager(process_ksig)`（main.c:121） | 内核信号转发回调（`11-signal-core.md`） |

`sef_startup()`（sef.c:68-105）随后执行 `sys_whoami` 获取自身信息，并等待 RS 的 `SEF_INIT` 消息——RS 是系统的"重启服务"，负责按 boot image 逐个启动系统服务并协调 Live Update。收到 `SEF_INIT_FRESH` 后，SEF 调用 `process_init`（sef_init.c:43）→ `sef_cb_init_fresh`。

关键认知：**SEF 是"注册—分发"框架，不是初始化逻辑本身**。PM 的真实初始化全部在 `sef_cb_init_fresh` 里；Rust 侧可以保留协议语义、去掉框架（见 §3.2 D2）。

### 1.3 初始化依赖链：八步为什么是这个顺序

`sef_cb_init_fresh`（main.c:131-243）的八步，每一步只依赖前一步已建立的设施：

```
1. mproc 表 + 定时器初始化（main.c:147-152）   ← 零依赖：纯内部状态
2. 信号集合构建（main.c:157-165）             ← 依赖 1 的表（每槽默认空信号集）
3. sys_getmonparams（main.c:169-170）         ← 依赖内核 IPC：boot 参数
4. sys_getimage（main.c:175-176）             ← 依赖内核 IPC：进程清单
5. boot image 填充 mproc（main.c:178-229）    ← 依赖 1+2+3+4：实例化进程
6. VFS_PM_INIT 同步（main.c:220-236）         ← 依赖 5：把新进程告诉 VFS
7. system_hz = sys_hz()（main.c:238）         ← 依赖内核 IPC：时钟频率
8. sched_init()（main.c:241）                 ← 依赖 5+7：为 INIT 接管调度
```

顺序背后的两个不变量：

- **先本地后对端**：mproc 表必须先填充完成，才能把进程信息同步给 VFS；VFS 回复前 PM 不能继续。
- **先静态后动态**：第 1-2 步是纯内部初始化（无 IPC），第 3-4 步引入内核数据，第 5-6 步产生外部可见状态，第 7-8 步才触及运行时机制（时钟、调度）。任何一步提前都会引用未建立的设施。

### 1.4 第一代进程树：INIT 是"自己的父亲"

boot image 里的用户进程只有两类，身份规则完全不同：

**INIT（槽 11）**——第一个用户进程，root 身份，是所有孤儿进程的最终收养者。C 源码显式注释了它的特殊之处（main.c:189-193）：

> "INIT is root, we make it father of itself. This is not really OK, INIT should have no father, i.e. a father with pid NO_PID. But PM currently assumes that mp_parent always points to a valid slot number."

也就是说：**PM 的 `mp_parent` 不变量是"恒指向有效槽位"**，INIT 没有真实父亲，于是把父亲设为自己（`mp_parent = INIT_PROC_NR`）。这是 PM 的妥协设计——放弃"无父亲"表达，换取父指针永远可解引用。Rust 侧沿用同一语义（`Guardianship::Normal { parent: slot(11) }`）。

**系统进程（PM/VFS/RS/VM/...）**——`PRIV_PROC` 特权进程。除 RS 自身（父亲是 INIT）外，父亲一律是 RS（main.c:203-208）：**RS 是系统服务的"监护者"**——重启、Live Update、崩溃恢复都归它管，所以系统进程的进程树父链挂在 RS 下。系统进程的 PID 不固定，由 `get_free_pid()` 顺序分配（`03-mproc-table.md`）。

这条"第一代进程树"的形状决定了后续所有进程关系：普通用户进程从 INIT fork 出来，系统服务从 RS fork（`srv_fork`，`08-pm-srv-fork.md`）。

### 1.5 VFS 握手同步：两张进程表如何对齐

PM 与 VFS 各自维护进程表（PM 的 `mproc` / VFS 的 `fproc`），靠 endpoint 关联。**PM 填充一个 boot 进程，就发一条 `VFS_PM_INIT` 消息给 VFS**（槽号 + PID + endpoint），VFS 据此创建对应的 `fproc` 槽。全部发完后，PM 发**最后一条 `VFS_PM_INIT`（endpoint = NONE）并同步等待回复**——这是握手屏障：

- 逐条 `ipc_send`（异步，main.c:226）：每个进程一条，VFS 不回复。
- 末条 `ipc_sendrec`（同步，main.c:235）：endpoint = NONE 表示"没有更多系统进程"，VFS 处理完前面的消息后回复 OK，PM 才继续。

为什么不是全部 `sendrec`？因为每条都同步等待会与 VFS 的初始化顺序互相阻塞（VFS 也要等 PM 初始化完成）；逐条异步 + 末条同步屏障在"不丢消息"与"不互相等待"之间取得平衡——**先倾倒全部数据，再一次性确认**。这是微内核服务间"表同步"的经典模式（VM 与内核的 boot 协议同理）。

### 1.6 本章小结

PM 的启动链回答了三件事：

1. **权威来源**：进程初始清单来自内核 boot image（`sys_getimage`），不是 PM 自造。
2. **顺序**：八步依赖链——本地状态 → 内核数据 → 表实例化 → 对端同步 → 运行时机制。
3. **身份**：INIT 是"自己的父亲"（父指针恒有效不变量），系统进程挂在 RS 监护下（PRIV_PROC）。

下一章逐行分析 C 源码；第 3 章给出 Rust 的类型系统重表达。

---

## 2. C 源码分析

### 2.1 main()：入口与主循环骨架（main.c:49-109）

```c
// main.c:49-56
int main(void)
{
  unsigned int call_index;
  int ipc_status, result;

  /* SEF local startup. */
  sef_local_startup();
```

`main()` 先执行 `sef_local_startup()`（§2.2），然后进入 `while (TRUE)` 主循环（main.c:59-107）。主循环骨架如下，**分发与回复细节归 04**，本档只列入口以确立"初始化完成后做什么"：

| 主循环步骤 | 位置 | 归属文档 |
|-----------|------|---------|
| `sef_receive_status(ANY, &m_in, &ipc_status)` | main.c:61 | 04 |
| `is_ipc_notify`：CLOCK → `expire_timers` | main.c:65-71 | 14 |
| `pm_isokendpt(who_e, &who_p)` 验证 caller | main.c:75-77 | 03 |
| EXITING 进程的延迟调用直接丢弃 | main.c:80-82 | 04/09 |
| `IS_VFS_PM_RS` → `handle_vfs_reply()` | main.c:84-87 | 05 |
| `PROC_EVENT_REPLY` → `do_proc_event_reply()` | main.c:88-89 | 06 |
| `IS_PM_CALL` → `call_vec[call_index]()` | main.c:90-101 | 04 |
| `result != SUSPEND` → `reply(who_p, result)` | main.c:106 | 04 |

**要点**：主循环在 `sef_local_startup()` 返回后才开始——即 PM 的"服务资格"（进程表就绪、VFS 对齐、调度接管）由 `sef_cb_init_fresh` 建立，主循环只是消费它。

### 2.2 sef_local_startup()：SEF 回调注册（main.c:115-127）

```c
// main.c:115-127
static void sef_local_startup(void)
{
  /* Register init callbacks. */
  sef_setcb_init_fresh(sef_cb_init_fresh);
  sef_setcb_init_restart(SEF_CB_INIT_RESTART_STATEFUL);

  /* Register signal callbacks. */
  sef_setcb_signal_manager(process_ksig);

  /* Let SEF perform startup. */
  sef_startup();
}
```

三个注册调用 + `sef_startup()`。`SEF_CB_INIT_RESTART_STATEFUL` 表示"重启时恢复状态"（Live Update 场景）；`process_ksig` 是内核信号回调（`11-signal-core.md`）。

### 2.3 SEF 库：sef_startup 与 process_init（sef.c / sef_init.c）

`sef_startup()`（`minix3/minix/lib/libsys/sef.c:68-105`）做两件事：

1. `sys_whoami` 获取自身 endpoint/权限/init 标志（sef.c:76-84）；
2. 按自身角色分派：RS 走 `do_sef_rs_init`，其他服务**等待 RS 的 `SEF_INIT` 消息**（sef.c:87-121），收到后调 `do_sef_init_request`（sef_init.c:34）→ `process_init`（sef_init.c:43）。

`process_init` 里与 PM 相关的路径：清 IPC filter → 建状态传输 grant → 按 `type` 调 `sef_cb_init_fresh`（`SEF_INIT_FRESH`）→ 向 RS 回 `SEF_INIT_REPLY`。**RS_INIT 握手（等待消息 + 回复）是框架行为**；Rust 侧将其简化（§3.2）。

### 2.4 sef_cb_init_fresh()：八步初始化（main.c:131-243）

#### 第 1 步：mproc 表 + 定时器初始化（main.c:146-152）

```c
  for (rmp=&mproc[0]; rmp<&mproc[NR_PROCS]; rmp++) {
	init_timer(&rmp->mp_timer);
	rmp->mp_magic = MP_MAGIC;
	rmp->mp_sigact = mpsigact[rmp - mproc];
	rmp->mp_eventsub = NO_EVENTSUB;
  }
```

逐槽执行四件事：

- `init_timer(&mp_timer)`：初始化该进程的定时器节点（`14-itimer.md`）。
- `mp_magic = MP_MAGIC`：魔数（`0xC0FFEE0`，mproc.h:106），用于在 C 侧检测表损坏/越界访问。**Rust 类型系统消灭了这类错误，无对应字段**（ARCH A-2 相关）。
- `mp_sigact = mpsigact[rmp - mproc]`：把该槽的 sigaction 数组指针指到独立大表 `mpsigact[NR_PROCS][_NSIG]` 的对应行。mpsigact 独立于 mproc 存放是刻意设计——"sigaction 约占每进程状态 80%，独立出来让 MIB 服务免于引入"（mproc.h:16-20 注释，声明在 mproc.h:22）。**Rust 中 `SignalState::actions: Box<[SigAction; _NSIG]>` 等价表达每槽独立的动作表**。
- `mp_eventsub = NO_EVENTSUB`：事件订阅者置空（`06-event-subscription.md`）。

#### 第 2 步：信号集合构建（main.c:154-165）

```c
  static char core_sigs[] = { SIGQUIT, SIGILL, SIGTRAP, SIGABRT,
				SIGEMT, SIGFPE, SIGBUS, SIGSEGV };
  static char ign_sigs[] = { SIGCHLD, SIGWINCH, SIGCONT, SIGINFO };
  static char noign_sigs[] = { SIGILL, SIGTRAP, SIGEMT, SIGFPE,
				SIGBUS, SIGSEGV };
  sigemptyset(&core_sset);
  for (sig_ptr = core_sigs; ...) sigaddset(&core_sset, *sig_ptr);
  ...
```

三个全局信号集合（glo.h:21-23）：

- `core_sset`：**引发 core dump** 的信号——SIGQUIT/SIGILL/SIGTRAP/SIGABRT/SIGEMT/SIGFPE/SIGBUS/SIGSEGV。
- `ign_sset`：**默认忽略**的信号——SIGCHLD/SIGWINCH/SIGCONT/SIGINFO。
- `noign_sset`：**不可忽略**的信号——SIGILL/SIGTRAP/SIGEMT/SIGFPE/SIGBUS/SIGSEGV（即使进程设为忽略也强制默认处理）。

信号数值来自 `minix3/sys/sys/signal.h`（SIGQUIT=3, SIGILL=4, SIGTRAP=5, SIGABRT=6, SIGEMT=7, SIGFPE=8, SIGBUS=10, SIGSEGV=11, SIGCONT=19, SIGCHLD=20, SIGWINCH=28, SIGINFO=29）。**集合的消费语义在 `11-signal-core.md`（`check_sig`/`sig_proc_exit`）**；本档只记构建事实。

#### 第 3 步：sys_getmonparams（main.c:167-170）

```c
  if ((s=sys_getmonparams(monitor_params, sizeof(monitor_params))) != OK)
      panic("get monitor params failed: %d", s);
```

从内核拷贝 boot monitor 参数到全局 `monitor_params`（glo.h:10，`MULTIBOOT_PARAM_BUF_SIZE` = 1024 字节，`include/arch/earm/include/multiboot.h:240`）。参数是 `name=value\0` 序列，消费方是 `find_param`（`utility.c:58-75`，归 20）。

#### 第 4 步：sys_getimage（main.c:172-176）

```c
  if (OK != (s=sys_getimage(image)))
  	panic("couldn't get image table: %d", s);
```

把内核的 boot image 表拷贝到局部静态 `image[NR_BOOT_PROCS]`。`struct boot_image`（`minix3/minix/include/minix/type.h:148-153`）字段：`proc_nr`（槽号，内核 task 为负）、`proc_name`（PROC_NAME_LEN=16）、`endpoint`、`start_addr`/`len`（内存布局，PM 不用）。

#### 第 5 步：boot image 填充 mproc（main.c:177-229）

```c
  procs_in_use = 0;				/* start populating table */
  for (ip = &image[0]; ip < &image[NR_BOOT_PROCS]; ip++) {
  	if (ip->proc_nr >= 0) {			/* task have negative nrs */
  		procs_in_use += 1;		/* found user process */
```

- **负槽号跳过**（main.c:179）：内核 task（CLOCK/SYSTEM/...）不在 PM 管辖范围，不占 `procs_in_use`。
- **公共字段**（main.c:183-187）：`strlcpy(mp_name, proc_name)`；`sigemptyset(&mp_ignore/&mp_sigmask/&mp_catch)`——三个信号集清空，进程默认"全部默认处理、无阻塞、无捕获"。

**INIT 分支**（main.c:188-201）：

```c
  		if (ip->proc_nr == INIT_PROC_NR) {	/* user process */
  			rmp->mp_parent = INIT_PROC_NR;		// 自己是父亲
  			rmp->mp_procgrp = rmp->mp_pid = INIT_PID;	// pid = procgrp = 1
			rmp->mp_flags |= IN_USE;		// 槽占用
			rmp->mp_scheduler = KERNEL;		// 初始内核调度
			rmp->mp_nice = get_nice_value(USR_Q);	// queue→nice
```

**系统进程分支**（main.c:202-216）：

```c
		else {					/* system process */
  			if(ip->proc_nr == RS_PROC_NR) rmp->mp_parent = INIT_PROC_NR;
  			else rmp->mp_parent = RS_PROC_NR;	// 其余挂 RS
  			rmp->mp_pid = get_free_pid();		// PID 顺序分配
			rmp->mp_flags |= IN_USE | PRIV_PROC;	// 特权进程
			rmp->mp_scheduler = NONE;		// 未指定调度器
			rmp->mp_nice = get_nice_value(SRV_Q);
		}
```

**公共收尾**（main.c:217-218）：`rmp->mp_endpoint = ip->endpoint`——进程的 IPC 身份来自内核清单（generation 0 的 endpoint，`03-mproc-table.md`）。

#### 第 6 步：VFS_PM_INIT 同步（main.c:220-236）

逐进程发送（main.c:220-227）：

```c
  		memset(&mess, 0, sizeof(mess));
		mess.m_type = VFS_PM_INIT;
		mess.VFS_PM_SLOT = ip->proc_nr;
		mess.VFS_PM_PID = rmp->mp_pid;
		mess.VFS_PM_ENDPT = rmp->mp_endpoint;
  		if (OK != (s=ipc_send(VFS_PROC_NR, &mess)))
			panic("can't sync up with VFS: %d", s);
```

末条屏障（main.c:231-236）：

```c
  memset(&mess, 0, sizeof(mess));
  mess.m_type = VFS_PM_INIT;
  mess.VFS_PM_ENDPT = NONE;
  if (ipc_sendrec(VFS_PROC_NR, &mess) != OK || mess.m_type != OK)
	panic("can't sync up with VFS");
```

协议面见 `com.h`：`VFS_PM_INIT = VFS_PM_RQ_BASE + 0 = 0x900`（com.h:513/520），字段 `VFS_PM_ENDPT=m7_i1` / `VFS_PM_SLOT=m7_i2` / `VFS_PM_PID=m7_i3`（com.h:547-551，`mess_7` 布局，ipc.h:70-74）。VFS 侧的 11 种回复状态机见 `05-vfs-interaction.md`。

#### 第 7 步：system_hz（main.c:238）

```c
 system_hz = sys_hz();
```

全局 `system_hz`（glo.h:25）保存内核时钟频率（HZ），是 `set_rusage_times`（utility.c）、定时器（`14-itimer.md`）、时间调用（`19-time.md`）的换算基准。

#### 第 8 步：sched_init()（main.c:240-241）

```c
  sched_init();
```

调用点。完整实现在 `schedule.c:36-69`（§2.6）。

### 2.5 reply() 与 get_nice_value()、handle_vfs_reply()：本档的"调用点"

三个函数的**定义位置都在 main.c**，但语义归属按 plan.md §5.3：

| 函数 | 位置 | 语义归属 | 本档覆盖范围 |
|------|------|---------|-------------|
| `reply(proc_nr, result)` | main.c:245-268 | 04-ipc-dispatch.md | 存在性 + 调用点（主循环尾部 main.c:106） |
| `get_nice_value(queue)` | main.c:276-295 | 16-scheduling.md | 调用点（main.c:200/214）+ 公式概览 |
| `handle_vfs_reply()` | main.c:295-424 | 05-vfs-interaction.md | 存在性 + 调用点（main.c:84-87） |

`get_nice_value` 公式（main.c:283-289）：把内核优先级队列（`MIN_USER_Q`~`MAX_USER_Q`）线性缩放到 nice（`PRIO_MIN`~`PRIO_MAX`）：

```c
  int nice_val = (queue - USER_Q) * (PRIO_MAX-PRIO_MIN+1) /
      (MIN_USER_Q-MAX_USER_Q+1);
```

常量来源：`MAX_USER_Q=0`/`MIN_USER_Q=15`/`USER_Q=7`（config.h:66-74），`PRIO_MIN=-20`/`PRIO_MAX=20`（`minix3/sys/sys/resource.h:43-44`）。**INIT 与系统进程都传 USER_Q（USR_Q = SRV_Q = USER_Q，priv.h:93-95），故两者 nice 都是 0**——公式的实际作用在 16 的 `nice_to_priority` 逆变换。

### 2.6 sched_init()：为 INIT 指定用户态调度器（schedule.c:36-69）

```c
	for (proc_nr=0, trmp=mproc; proc_nr < NR_PROCS; proc_nr++, trmp++) {
		if (trmp->mp_flags & IN_USE && !(trmp->mp_flags & PRIV_PROC)) {
			assert(_ENDPOINT_P(trmp->mp_endpoint) == INIT_PROC_NR);
			parent_e = mproc[trmp->mp_parent].mp_endpoint;
			assert(parent_e == trmp->mp_endpoint);
			s = sched_start(SCHED_PROC_NR, trmp->mp_endpoint, parent_e,
				USER_Q, USER_QUANTUM, -1, &trmp->mp_scheduler);
```

语义要点：

- 遍历条件 `IN_USE && !PRIV_PROC`：**启动时只有 INIT 满足**（系统进程全是 PRIV_PROC）。
- 两个 assert：该进程必须是 INIT 槽（endpoint 槽号 = 11）；INIT 的父亲 endpoint == 自身（呼应 §1.4 的"自己父亲"）。
- `sched_start`（`minix3/minix/lib/libsys/sched_start.c:46-90`）向 SCHED 服务（endpoint 4）发 `SCHEDULING_START` 消息（`sched.h` 客户端）；成功后 `mp_scheduler` 回填为 SCHED_PROC_NR。失败仅打印警告（schedule.c:60-67），不 panic。
- `USER_QUANTUM=200`（config.h:74）。**SCHED 协议细节归 16**；本档只记"启动时为 INIT 完成从内核调度到用户态调度的切换"。

### 2.7 本档覆盖的函数/符号清单

| C 符号 | 位置 | 本档角色 |
|--------|------|---------|
| `main` | main.c:49-109 | 入口 + 主循环骨架（分发归 04） |
| `sef_local_startup` | main.c:115-127 | 回调注册 |
| `sef_cb_init_fresh` | main.c:131-243 | 八步初始化（本档主体） |
| `reply` | main.c:245-268 | 调用点（归 04） |
| `get_nice_value` | main.c:276-295 | 调用点（归 16） |
| `handle_vfs_reply` | main.c:295-424 | 调用点（归 05） |
| `sched_init` | schedule.c:36-69 | 调用点（协议归 16） |
| `mpsigact` | mproc.h:22 | 独立 sigaction 表（第 1 步引用） |
| `core_sset/ign_sset/noign_sset` | glo.h:21-23 | 信号集合（消费归 11） |
| `monitor_params` | glo.h:10 | 启动参数（消费归 20） |
| `system_hz` | glo.h:25 | 时钟频率（消费归 14/19） |
| `VFS_PM_INIT` | com.h:513/520 | 同步协议（回复归 05） |
| `struct boot_image` | type.h:148-153 | 启动清单 |

---

## 3. Rust 设计决策

### 3.1 D1：`BootParams`——显式启动契约

- **C**：`monitor_params`（glo.h:10）+ `image[NR_BOOT_PROCS]`（main.c:135）+ `system_hz`（glo.h:25）三个全局/静态，由三个内核 IPC 在初始化中途填充。
- **Rust**：`BootParams { monitor_params: [u8; 1024], boot_image: [BootImage; NR_BOOT_PROCS], system_hz: u32, vfs_endpoint: Endpoint }` 聚合为**单一启动契约**（`init.rs`）。
- **为什么**：与 VM 的 `BootParams`（`02-stage-vm/01-vm-init-main.md` §3.1）同型——启动依赖显式化，可审计、可测试。C 的三个全局分散在 glo.h，初始化顺序错误只能靠运行时 panic 暴露；Rust 把它们变成构造参数，类型系统保证"缺一不可"。
- **替代方案**：保持三个独立全局（`static mut`）。否决——`static mut` 需要 unsafe，且与 PM 现有 `ProcTable`/`PmContext` 的无全局设计冲突。
- **行为契约**：`boot_image` 定长 17（`NR_BOOT_PROCS`，minix-types `boot.rs`）；`vfs_endpoint` 默认 `Endpoint::VFS`(1)。
- **DEFERRED**：`sys_getmonparams`/`sys_getimage`/`sys_hz` 的内核 IPC 传输归 `minix-sys`；当前用 `BootParams::placeholder()` 占位（与 VM 同款）。

### 3.2 D2：`PmServer`——构造即空表，init() 分步复刻

- **C**：`sef_local_startup()` + `sef_startup()` 的"注册—等待 RS—分发"框架 + `sef_cb_init_fresh` 八步。
- **Rust**：`PmServer<T: IpcTransport>`（`init.rs`）——`with_transport(params, transport)` 构造即空表（等价第 1 步），`init()` 执行第 2-8 步，`run()` 进入主循环（主体归 04）。
- **为什么**：PM 没有 VM 的 `is_first_time` 门控——PM 只有 fresh 启动语义（Live Update/重启状态恢复归 `06-event-subscription.md`/`99-global-concepts.md`）。SEF 框架（RS_INIT 握手）在 Rust 侧**保留协议、去掉框架**：`PmServer::init()` 等价 `sef_cb_init_fresh`，RS 的 `SEF_INIT` 等待/回复归主循环（04，与 VM `rs_handshake` 同型）。
- **替代方案**：完整移植 SEF 状态机（注册回调表 + 状态枚举）。否决——VM 先例已证明"协议保留 + 框架简化"更符合 Rust 表达力，SEF 的注册-分发间接层在无 Live Update 需求前是纯开销。
- **行为契约**：`run()` 前必须 `init()`（`assert!(self.initialized)`，与 `VmServer` 一致）；`T` 为 IPC 传输（生产 `KernelIpcTransport`，测试 mock）。

### 3.3 D3：编译期信号集合

- **C**：main.c:154-165 运行时 `sigemptyset` + 循环 `sigaddset` 构建三个全局集合。
- **Rust**：`CORE_SIGSET`/`IGN_SIGSET`/`NOIGN_SIGSET` 为 `const` 位图（`SigSet = u64`，bit i 对应信号 i），`const fn sig_bit(sig) -> SigSet { 1u64 << sig }` 组合。
- **为什么**：集合构建后只读（signal.c:483/533/552 仅 `sigismember` 查询），运行时构建是纯浪费；编译期常量把"这些信号永不改变"变成类型事实。位图用 u64（`_NSIG = 64`，signal.h:45），信号编号 ≤ 29 全部可表达。
- **替代方案**：`OnceCell` 惰性初始化。否决——无状态、无初始化顺序问题，const 即可。
- **行为契约**：core={3,4,5,6,7,8,10,11}，ign={19,20,28,29}，noign={4,5,7,8,10,11}（signal.h 数值）。消费方是 11（`check_sig`/`sig_proc_exit`），当前以 `pub(crate)` + `#[allow(dead_code)]` 暴露。

### 3.4 D4：类型化 boot 填充

- **C**：main.c:177-229 循环直接写 `mproc[ip->proc_nr]` 裸字段 + `mp_flags |= IN_USE|PRIV_PROC`。
- **Rust**：`fill_boot_procs()` 对每条 `proc_nr >= 0` 且 `endpoint != NONE` 的条目，用类型表达身份：
  - INIT：`Lifecycle::Running`（= IN_USE）、`Privilege::User`（无 PRIV_PROC）、`Guardianship::Normal { parent: slot(11) }`、`scheduler = Endpoint::KERNEL`。
  - 系统进程：`Lifecycle::Running` + `Privilege::Kernel`（= PRIV_PROC）、`Guardianship::Normal { parent: slot(RS|INIT) }`、`scheduler = Endpoint::NONE`。
  - PID 经 `PidGenerator::get_free_pid`（`03-mproc-table.md`）。
- **为什么**：C 的 flag 位组合（IN_USE|PRIV_PROC）在 Rust 由互斥枚举 + `Privilege` 枚举表达（ARCH A-2）；"INIT 无 PRIV_PROC、系统进程有"成为类型事实而非位运算。与 VM `init_boot_procs` 同规则跳过 padding 条目（`endpoint == NONE`，定长数组的填充处理）。
- **行为契约**：填充后 `procs_in_use` 精确；INIT 槽满足 `parent() == self`；系统进程 `is_kernel_process()`；负槽号与 NONE 条目跳过。

### 3.5 D5：VFS 同步客户端

- **C**：main.c:220-236 逐条 `ipc_send` + 末条 `ipc_sendrec`，手写 `message` union 字段（`mess.VFS_PM_SLOT = ...`）。
- **Rust**：`VfsPmInit { slot, pid, endpoint }` 类型化消息 + `encode()` → `MessageM7`（新增于 `minix-types/src/ipc/message.rs`，对齐 C `mess_7` 布局）；`vfs_init_sync()` 逐条 `send` + 末条 `sendrec` 屏障并断言回复 OK。
- **为什么**：A-4 类型化 IPC——C 的 union 字段拼装（com.h:547-551 的 m7_i1/m7_i2/m7_i3）在 Rust 变成具名字段，编码契约集中在 `encode()` 一处。传输经 `IpcTransport` trait（与 VM `ipc/transport.rs` 同型），生产 `KernelIpcTransport` / 测试 mock 共享同一路径。
- **行为契约**：逐条消息字段 `m7_i1=endpoint, m7_i2=slot, m7_i3=pid`；末条 `m7_i1=NONE, m7_i2=0, m7_i3=0`；屏障回复 `m_type == OK` 否则启动失败（C panic ↔ Rust `expect`/`assert_eq!`）。

### 3.6 D6：调度接管调用点

- **C**：`sched_init()`（schedule.c:36-69）完整实现。
- **Rust**：`init_scheduling()` 只实现**条件遍历 + 断言 + 失败容错**，`sched_start` 客户端归 16（DEFERRED 占位返回 `Ok(Endpoint::SCHED)`）。
- **为什么**：plan.md §3.4 边界表——01 只承担"调用点"，完整 SCHED 协议（`sched.h` 客户端、`SCHEDULING_START` 消息、`nice_to_priority`）在 16 展开。提前实现会造成文档与代码归属错位。
- **行为契约**：只有 INIT 满足 `IN_USE && !PRIV_PROC`；两个 assert（INIT 槽、父==自身）保留（C schedule.c:46-48）；失败不 panic、仅记录（C schedule.c:60-67 printf；Rust 当前仅 `#[cfg(test)]` 输出——生产日志机制随 16 落地）。

### 3.7 ARCH 标注汇总

| ARCH 项 | 本档落点 | 三处一致标注 |
|---------|---------|-------------|
| A-1 mproc 分层 | 填充用 `Process` 分层字段（identity/state/resources） | `init.rs` 注释 + 本文档 §3.4 + plan.md §4 |
| A-2 flag → 枚举 | `Lifecycle::Running` = IN_USE；`Privilege::Kernel` = PRIV_PROC | `init.rs` + §3.4 + plan.md §4 |
| A-3 全局 → 显式结构 | `PmServer` 聚合 table/params/transport | `init.rs` + §3.2 + plan.md §4 |
| A-4 union → 类型化 IPC | `VfsPmInit` + `MessageM7` | `minix-types message.rs` + §3.5 + plan.md §4 |
| A-6 SUSPEND 显式化 | 主循环归 04（本档只到 `run()` 入口） | `init.rs run()` 注释 + §2.1 + plan.md §7.3 |
| A-11 64 位类型 | `Pid`/`Endpoint`/`UserSlot` | `init.rs` 签名 + §3.4 + plan.md §4 |
| A-12 双监护 | `Guardianship::Normal` 表达第一代父链 | `init.rs` + §3.4 + plan.md §4 |

### 3.8 对照：Redox / Linux 的进程表初始化模型

本档的核心机制——"PM 向内核要 boot image、再与 VFS 显式对齐进程表"——是
Minix3 微内核**多权威进程表**架构的产物。对照主流 OS 的进程表初始化模型，
能更清楚地看出哪些是 Minix3 的必然约束、哪些是 Rust 重写可改善的点：

| 维度 | Redox | Linux | Minix3（本档） |
|------|-------|-------|---------------|
| 进程表权威 | 内核单一权威（`context` 表） | 内核单一权威（`task_struct`） | **PM 语义权威 + VFS/VM 各持视图**，靠 endpoint 关联 |
| 第一代进程 | 内核直接创建 `init` | 内核创建 PID 1 | PM 从内核拷贝 boot image 后**自行实例化** INIT |
| 跨服务表同步 | 无（VFS 按 scheme 调用上下文取进程信息） | 无（fd/cred 都挂 task_struct） | **VFS_PM_INIT 逐条 + 屏障握手**（本档 §1.5） |
| 孤儿收养 | 内核 | 内核自动挂 init/subreaper | 用户态 PM + RS 双监护（`09-pm-exit.md`） |

**最佳实践对比结论**：Redox/Linux 把进程表集中在内核，省去了跨服务同步；
Minix3 把权威放在用户态 PM，换来的是"服务可用 CR3/权限位在用户态维护"的
灵活性，代价正是本档的启动握手。Rust 重写能改善的不是架构（握手必须保留），
而是**表达质量**：`BootParams` 显式启动契约（对应 Redox 的 `Scheme` trait
化接口、Linux 的 `init_task` 常量）、`IpcTransport` 传输抽象（对应 Redox
`scheme` 的 trait 化 syscall 边界）——把"全局状态 + 隐式 IPC"变成
"构造参数 + 显式 trait"，是三个系统在类型化演进上的共同方向。

---

## 4. 实现详解

### 4.1 模块结构

```
os/servers/pm/src/
├── main.rs            — 二进制入口：PmServer::new(placeholder) → init() → run()
├── lib.rs             — pub mod init; pub mod ipc; ...（移除空的自由函数 init/run）
├── init.rs            — BootParams / PmServer / VfsPmInit / 编译期信号集合 / nice_from_queue
└── ipc/
    └── transport.rs   — IpcTransport trait + KernelIpcTransport + TestIpcTransport
```

### 4.2 `BootParams`（init.rs）

```rust
pub struct BootParams {
    pub monitor_params: [u8; MULTIBOOT_PARAM_BUF_SIZE], // 1024，multiboot.h:240
    pub boot_image: [BootImage; NR_BOOT_PROCS],         // 17，minix-types boot.rs
    pub system_hz: u32,                                 // sys_hz() 占位
    pub vfs_endpoint: Endpoint,                         // Endpoint::VFS(1)
}
```

`placeholder()` 提供占位值（全空 boot image、hz=100、VFS endpoint）。真实路径由 `minix-sys` 落地后替换（main.rs 注释标明）。

### 4.3 `PmServer::init()`：八步落地（init.rs）

```rust
pub fn init(&mut self) {
    self.fill_boot_procs();   // 第 5 步：main.c:178-229
    self.vfs_init_sync();     // 第 6 步：main.c:220-236
    self.init_scheduling();   // 第 8 步：main.c:241（SCHED 细节归 16）
    self.initialized = true;
}
```

第 1 步（表初始化）在 `ProcTable::new()` 构造时完成；第 2 步（信号集合）是编译期常量；第 3/4/7 步（monitor/image/hz）由 `BootParams` 承载（DEFERRED 传输）。`init()` 的 Phase 注释与 C 行号一一对应（C 注释块）。

### 4.4 `fill_boot_procs()`（init.rs）

遍历 `params.boot_image`；跳过 `proc_nr < 0`（内核 task，main.c:179）与 `endpoint.is_none()`（padding，VM 同规则）。对合法条目：

1. 计数 `procs_in_use`（main.c:180）；
2. 分配 PID：INIT 用 `INIT_PID`(1)，系统进程用 `table.pid_generator.get_free_pid(&table)`（main.c:209）；
3. `get_mut(slot)` 后按分支写身份：名字/索引/endpoint 公共，INIT 与系统进程差异见 §3.4；
4. 循环结束 `table.procs_in_use.set(count)`（main.c:177 语义）。

### 4.5 `vfs_init_sync()`（init.rs）

```rust
fn vfs_init_sync(&mut self) {
    let messages = self.vfs_init_messages();      // 逐条 + 末条屏障
    for msg in &messages[..messages.len() - 1] {
        self.transport.send(self.params.vfs_endpoint, msg)
            .expect("PM: can't sync up with VFS (per-process send)");
    }
    let mut barrier = *messages.last().unwrap();
    self.transport.sendrec(self.params.vfs_endpoint, &mut barrier)
        .expect("PM: can't sync up with VFS (final barrier)");
    assert_eq!(barrier.m_type, 0 /* OK */, "...");
}
```

`vfs_init_messages()` 构造全部消息（每条 `VfsPmInit{slot,pid,endpoint}.encode()` + 末条 `{0, 0, NONE}`），**消息构造与传输分离**——测试可只验构造，传输经 mock 验证。

`VfsPmInit::encode()` 用 `MessageM7`（`minix-types/src/ipc/message.rs` 新增，C `mess_7` 的 64 位布局：5 ints + 2 u64 指针 + 20 填充 = 56 字节），字段映射 `m7i1=endpoint / m7i2=slot / m7i3=pid`（com.h:547-551）。

### 4.6 `init_scheduling()` 与 `nice_from_queue()`（init.rs）

`init_scheduling()`：遍历 `0..NR_PROCS`，对 `in_use && !is_kernel_process()` 的槽断言 INIT 身份（`endpoint.slot() == INIT_PROC_NR`）与父==自身，然后调 `sched_start`（DEFERRED 占位返回 `Ok(Endpoint::SCHED)`，16 落地后替换）。失败走 `Err(())` 分支仅记录（当前仅 `#[cfg(test)]` 输出；生产日志机制随 16 落地）。

`nice_from_queue(queue)`：`(queue - USER_Q) * 41 / 16`，`clamp(PRIO_MIN, PRIO_MAX)`——与 C `get_nice_value`（main.c:283-289）逐字等价（Rust 与 C 的整数除法都向零截断）。私有函数，语义归 16。

### 4.7 `ipc/transport.rs`：IpcTransport trait

```rust
pub trait IpcTransport {
    fn send(&mut self, dest: Endpoint, msg: &Message) -> Result<(), IpcError>;
    fn sendrec(&mut self, dest: Endpoint, msg: &mut Message) -> Result<(), IpcError>;
}
```

- `KernelIpcTransport`：生产实现，`unimplemented!()`（内核 IPC 落地前失败自说明）。
- `TestIpcTransport`：`#[cfg(test)]` mock，记录 `sent` 列表、`sendrec` 写回预设回复 `m_type`。
- 选择器 `ipc_transport_for_build()`。与 VM `ipc/transport.rs` 同型（trait 质量：≥2 个行为不同实现 + 泛型约束使用）。

### 4.8 main.rs / lib.rs

```rust
// main.rs
let params = BootParams::placeholder();   // minix-sys 落地后替换
let mut server = PmServer::new(params);
server.init();                             // sef_cb_init_fresh 等价
server.run();                              // 主循环（主体归 04）
```

`lib.rs` 移除原先空的自由函数 `init()`/`run()`（由 `PmServer` 方法替代），新增 `pub mod init`。

---

## 5. 测试要点

> 基线：`cargo test -p minix-pm --lib` 截至 2026-08-17 为 **87 passed / 0 failed**（原 77 + 本档新增 10：init.rs 7 + transport.rs 3）。minix-types 86 passed。

| 测试函数 | 验证点 | C 对应 |
|---------|--------|--------|
| `init::tests::test_signal_sets_match_c` | core/ign/noign 位图数值与最高信号编号 | signal.h + main.c:154-165 |
| `init::tests::test_nice_from_queue_default_queues` | USR_Q/SRV_Q→0；端点 MAX_USER_Q→-17、MIN_USER_Q→20 | main.c:283-289 |
| `init::tests::test_fill_boot_init_identity` | INIT pid=procgrp=1、父=自身、scheduler=KERNEL | main.c:188-201 |
| `init::tests::test_fill_boot_system_procs` | PRIV_PROC、父=RS（RS 父=INIT）、PID 顺序、负槽跳过、procs_in_use | main.c:179-216 |
| `init::tests::test_vfs_init_message_fields` | 消息 type/endpoint/slot/pid 字段编码 | com.h:520/547-551 |
| `init::tests::test_vfs_init_messages_order_and_final` | 逐条数量 + 末条 NONE 屏障 | main.c:220-236 |
| `init::tests::test_init_completes_with_mock_transport` | 全链 init + 5 条发送记录 | main.c:131-243 |
| `ipc::transport::tests::test_mock_records_send` | mock 记录 send | — |
| `ipc::transport::tests::test_mock_sendrec_overwrites_type` | sendrec 回复写回 m_type | main.c:235 |
| `ipc::transport::tests::test_mock_sendrec_custom_reply` | 自定义回复 | — |

测试策略：消息构造与传输分离（`vfs_init_messages` 可独立断言）；`init()` 经 `TestIpcTransport` mock 全链验证；不验证 DEFERRED 传输（`KernelIpcTransport` 落地前不可测）。

---

## 6. 过渡

本文档完成 PM 启动链的全部初始化：**进程表就绪、信号集合就绪、VFS 进程表对齐、调度接管触发**。此时 PM 进入主循环（`run()`，入口在本档 §4.8，主体在下一文档）：

```
01（本档：启动与初始化完成）
  → 04-ipc-dispatch.md：主循环收消息/分发/回复（SUSPEND 语义契约，plan.md §7.3）
  → 02-mproc-struct.md / 03-mproc-table.md：本档填充的字段与表操作的完整语义
  → 05-vfs-interaction.md：VFS_PM_INIT 只是 11 种 VFS 协议的握手；运行期回复状态机
  → 16-scheduling.md：sched_init 的 SCHED 协议完整展开
```

阅读顺序提示：若想先理解本档填充的"数据结构长什么样"，下一站 `02-mproc-struct.md`；若想理解"填充完怎么跑起来"，下一站 `04-ipc-dispatch.md`。

---

## 7. 参见

- `notes/rewrite/fork-syscall-rewrite/04-stage-pm/plan.md` — §2（文档编号）/§3.4（边界表）/§4（ARCH 清单）/§5.3（函数归属）/§7.3（SUSPEND 契约）
- `notes/rewrite/fork-syscall-rewrite/04-stage-pm/00-pm-overview.md` — PM 总览与文档导航
- `notes/rewrite/fork-syscall-rewrite/01-stage-kernel/06-proc-init-boot-proc.md` — boot image 的来源与内核侧实例化
- `notes/rewrite/fork-syscall-rewrite/01-stage-kernel/19-syscall-signal.md` — 内核信号路径（signal_manager 回调的对接）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/01-vm-init-main.md` — VM 启动链范本（SEF/启动契约/rs_handshake 同型设计）
- `minix3/minix/servers/pm/main.c` — 本档 ground truth
- `minix3/minix/servers/pm/schedule.c` — sched_init（调用点）
- `os/servers/pm/src/init.rs` — Rust 实现
- `os/servers/pm/src/ipc/transport.rs` — IPC 传输 trait
