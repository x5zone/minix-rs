# 04-ipc-dispatch: 主循环与消息分发

> **分类**: 阶段 2 — 主循环与异步协议（锚点文档）
> **源码**: `minix3/minix/servers/pm/main.c`（main 主循环 / reply / calls_stats）、`minix3/minix/servers/pm/table.c`（call_vec）、`minix3/minix/include/minix/callnr.h`（47 个调用号）、`minix3/minix/include/minix/com.h`（IS_VFS_PM_RS / PROC_EVENT_REPLY / SUSPEND / is_ipc_notify）、`minix3/minix/include/minix/ipcconst.h`（IPC_STATUS_CALL）
> **Rust 模块**: `os/servers/pm/src/ipc/transport.rs`（`IpcTransport::receive` / `IpcStatus`）、`os/servers/pm/src/ipc/calls.rs`（`PmCall` / `dispatch_pm_call`）、`os/servers/pm/src/ipc/dispatcher.rs`（`ReplyIntent` / `dispatch_message`）、`os/servers/pm/src/init.rs`（`PmServer::run` / `run_once` / `reply` / `RunStep`）
> **前置**: `notes/rewrite/fork-syscall-rewrite/04-stage-pm/01-pm-init-main.md`（启动链进入主循环）、`02-mproc-struct.md`（mproc 结构）、`03-mproc-table.md`（pm_isokendpt 调用点）
> **说明**: PM 主循环的完整语义：收消息、CLOCK notify 跳过、caller 验证、EXITING 丢弃、三路分发（VFS 回复 / 事件回复 / PM 调用）、SUSPEND 回复模型、reply()、47 调用分发表、调用统计。VFS 回复状态机（05）、事件订阅（06）只在本档建钩子。

---

## 1. 概念：事件循环——PM 运行时的心脏

### 1.0 章节引言

**目标读者**：已从 `01-pm-init-main.md` 理解 PM 启动链、从 `02-mproc-struct.md`/`03-mproc-table.md` 理解进程模型与表操作的开发者。本档假设读者知道：PM 是微内核中的用户态系统服务，其他服务与用户进程通过 IPC 消息与它通信。

> **本章不讲什么**:
> - VFS 异步回复的 11 种状态机（`05-vfs-interaction.md`）
> - 进程事件订阅/发布语义（`06-event-subscription.md`）
> - 各系统调用的 handler 内部逻辑（fork/exit/wait/信号等，`07~20`）
> - 定时器到期处理 `expire_timers`（`14-itimer.md`）
> - 调度相关 `get_nice_value`/`nice_to_priority`（`16-scheduling.md`）
>
> 本章只回答一个问题：**PM 进入主循环后，如何用一套循环体服务所有 IPC 对端，并在"同步回复"与"稍后回复"之间正确选择**。

### 1.1 服务器为何永不返回：事件循环

普通程序的 `main()` 执行一段逻辑后返回，进程退出。**服务器程序的 `main()` 永不返回**——它进入一个无限循环：等待消息 → 处理 → 回复 → 再等待。这个循环叫**事件循环（event loop）**，是单线程用户态服务器（PM/VM/VFS/RS）的基本执行模型。

PM 的 `main()` 在完成初始化（01 档）后进入 `while (TRUE)`（main.c:59）。循环的每一次迭代回答三个问题：

1. **谁在叫我？**（消息来源 endpoint → 进程表槽位）
2. **他要什么？**（消息类型 → 三路分发：VFS 回复 / 事件回复 / 系统调用）
3. **我怎么回应？**（同步回复 or 稍后回复）

与多线程服务器的区别：单线程事件循环**没有并发**——同一时刻只处理一条消息，处理完才取下一条。这消除了锁与竞态（进程表无需互斥），代价是 handler 不能阻塞（阻塞 = 整个服务器停摆）。这是微内核用户态服务的经典模型（Redox 的 kernel 侧 `scheme` 分发、Linux 的 `ksoftirqd` 单线程处理都是同一思想的不同体现）。

### 1.2 消息类型空间即分发表

PM 收到的消息是**定长 `Message`**（`m_type` 字段标识消息类型）。微内核不把"消息种类"散落成随意整数——**类型空间按基址（base）分段**，每个服务占据一段连续区间。PM 的分发逻辑本质上就是在这个类型空间上做区间匹配：

| 消息类型区间 | 基址 | 含义 | 分发去向 |
|-------------|------|------|---------|
| `PM_BASE + 1 .. PM_BASE + 47` | `0x000` | 用户进程的 PM 系统调用（47 个） | `call_vec`（本档 §2.6） |
| `VFS_PM_RQ_BASE + 0 .. + 11` | `0x900` | PM → VFS 请求（PM 是发送方） | 不进入主循环分发（发送在 handler 内） |
| `VFS_PM_RS_BASE + 1 .. + 11` | `0x980` | VFS → PM 异步回复（11 种） | `handle_vfs_reply`（→ 05） |
| `COMMON_RS_BASE + 0` | `0xE80` | 进程事件订阅者回复 | `do_proc_event_reply`（→ 06） |
| `NOTIFY_MESSAGE` | `0x1000` | 内核异步通知（时钟 tick / 中断） | `is_ipc_notify` 提前跳过 |

类型空间分段是**协议契约**：`callnr.h:9`（`PM_BASE = 0x000`）与 `com.h:513-514/597-598`（`VFS_PM_RQ_BASE = 0x900` / `VFS_PM_RS_BASE = 0x980` / `COMMON_RS_BASE = 0xE80`）由所有对端共同遵守。PM 主循环用两个掩码宏做快速族判定：

- `IS_PM_CALL(type) = ((type) & ~0xff) == PM_BASE`（callnr.h:11）——低 8 位内的消息都是"PM 调用族"；
- `IS_VFS_PM_RS(type) = ((type) & ~0x7f) == VFS_PM_RS_BASE`（com.h:517）——低 7 位内的消息都是"VFS 回复族"。

掩码判定而非精确相等，是 Minix3 的惯例：**消息类型低位携带子编号，高位携带服务归属**，一次掩码即可完成族路由。

### 1.3 为什么需要三路分发

主循环收到一条非通知消息后，分三种情况处理（main.c:84-103）：

1. **VFS 异步回复**（`IS_VFS_PM_RS && who_e == VFS`）——PM 之前向 VFS 发过请求（fork/exec/exit 等），VFS 处理完异步回复。这是**服务间异步协议**：PM 不能阻塞等 VFS（VFS 也在等 PM），只能"发出请求 → 继续服务别人 → 收到回复再补完"。
2. **事件订阅者回复**（`PROC_EVENT_REPLY`）——PM 发布进程事件（fork/exit/signal）后，订阅者处理完异步回复。
3. **PM 系统调用**（`IS_PM_CALL`）——用户进程经内核转发的系统调用（fork/exit/wait/kill/...），走 `call_vec` 分发表。

前两类是"别人回我"，第三类是"别人求我"——方向不同，回复路径也不同：前两类通常**不再回复**（或只补发之前挂起的回复），第三类**必须回复**调用者。这就是三路分发的本质：**按消息来源方向选择回复策略**。

### 1.4 回复协议：同步回复 vs SUSPEND

系统调用的标准回复模式：handler 返回结果 → 主循环 `reply()` 发回。但 PM 存在**异步场景**（fork 要等 VFS、exec 要等 VFS、wait 要等子进程退出），此时 handler 不能立即回复。C 用魔法数 `SUSPEND = -998`（com.h:1151）表达"本次不回复，稍后由 reply()/异步路径回复"：

```c
// main.c:104-106
/* Send reply. */
if (result != SUSPEND) reply(who_p, result);
```

`SUSPEND` 覆盖三种子情形（plan.md §7.3 语义契约）：

| 子情形 | C 位置 | 谁在稍后回复 |
|--------|--------|-------------|
| 等待中回复 | `do_wait4` 返回 SUSPEND（forkexit.c:559） | `tell_parent`/`tell_tracer`（forkexit.c:670-730） |
| 异步回复 | `do_exec`/`do_set` 返回 SUSPEND（exec.c:55、getset.c:222） | `handle_vfs_reply`（main.c:295-424） |
| 永不回复 | `do_exit` 返回 SUSPEND（forkexit.c:261） | 无（进程已死，回复无意义） |

Rust 侧把 `SUSPEND` 显式化为 `ReplyIntent` 枚举（§3.2），让"本次是否回复、谁在稍后回复"成为类型可见的契约。

### 1.5 错误哲学：ENOSYS 兜底与 fail-fast

主循环对消息的处理必须是**完全的**——任何消息类型都要有确定结果，不能漏：

- **未注册的调用号**（`call_index >= NR_PM_CALLS` 或表项 NULL）→ `ENOSYS`（main.c:94-101）；
- **非 PM 调用**（消息类型不在任何已知族）→ `ENOSYS`（main.c:102-103）。

`ENOSYS`（"Function not implemented"，errno.h:137）是用户进程能理解的标准错误。C 里 47 个调用号全部注册，NULL 分支是防御性的；Rust 里未实现的 handler 同样返回 ENOSYS 占位（§3.6）。

另一类是**不可恢复错误**——服务器自身状态损坏时 fail-fast panic：

- `sef_receive_status` 失败 → `panic("PM sef_receive_status error")`（main.c:61-62）；
- `pm_isokendpt` 失败 → `panic("PM got message from invalid endpoint")`（main.c:75-76）。

panic 的合理性：这两个错误意味着**内核给了 PM 非法数据**（PM 的进程表是权威，endpoint 必然合法），继续服务只会雪上加霜。这是微内核服务的通用策略——状态损坏时快速失败，交给 RS 重启（`08-stage-is` 与 Live Update 语义）。

### 1.6 本章小结

1. **事件循环**：单线程服务器永不返回，一次处理一条消息，无锁无竞态。
2. **类型空间**：消息类型按基址分段（PM 调用 0x000 / VFS 回复 0x980 / 事件回复 0xE80 / 通知 0x1000），掩码做族路由。
3. **三路分发**：按消息来源方向选择处理与回复策略（VFS 异步回复 / 事件回复 / 系统调用）。
4. **回复协议**：同步 `reply()` vs `SUSPEND`（稍后回复），Rust 显式化为枚举。
5. **错误哲学**：未知调用 ENOSYS 兜底，状态损坏 fail-fast panic。

下一章逐行分析 C 主循环；第 3 章给出 Rust 的类型系统重表达。

---

## 2. C 源码分析

### 2.1 main() 主循环骨架（main.c:59-107）

```c
// main.c:59-107（节选）
while (TRUE) {
    /* Wait for the next message. */
    if (sef_receive_status(ANY, &m_in, &ipc_status) != OK)
        panic("PM sef_receive_status error");          // L61-62

    /* Check for system notifications first. Special cases. */
    if (is_ipc_notify(ipc_status)) {                    // L65
        if (_ENDPOINT_P(m_in.m_source) == CLOCK)
            expire_timers(m_in.m_notify.timestamp);     // L66-67
        continue;                                       // L70
    }

    /* Extract useful information from the message. */
    who_e = m_in.m_source;                              // L74
    if (pm_isokendpt(who_e, &who_p) != OK)
        panic("PM got message from invalid endpoint");  // L75-76
    mp = &mproc[who_p];                                 // L77
    call_nr = m_in.m_type;                              // L78

    /* Drop delayed calls from exiting processes. */
    if (mp->mp_flags & EXITING)                         // L81
        continue;                                       // L82

    if (IS_VFS_PM_RS(call_nr) && who_e == VFS_PROC_NR) {// L84
        handle_vfs_reply();                             // L85
        result = SUSPEND;                               // L87
    } else if (call_nr == PROC_EVENT_REPLY) {           // L88
        result = do_proc_event_reply();                 // L89
    } else if (IS_PM_CALL(call_nr)) {                   // L90
        call_index = (unsigned int) (call_nr - PM_BASE);// L92
        if (call_index < NR_PM_CALLS && call_vec[call_index] != NULL) {
            result = (*call_vec[call_index])();         // L99
        } else
            result = ENOSYS;                            // L101
    } else
        result = ENOSYS;                                // L103

    /* Send reply. */
    if (result != SUSPEND) reply(who_p, result);        // L106
}
```

循环体分为四段：**收消息（L61）→ 通知跳过（L65-71）→ 验证与预处理（L74-82）→ 分发与回复（L84-106）**。前三段是"确认这确实是一条可服务的请求"，第四段才进入服务逻辑。任何一步失败都 `continue`（通知/EXITING）或 `panic`（receive/endpoint），循环保证不卡死。

### 2.2 通知分支：is_ipc_notify + CLOCK（main.c:65-71）

```c
// com.h:92
#define is_ipc_notify(ipc_status) (IPC_STATUS_CALL(ipc_status) == NOTIFY)
```

`sef_receive_status` 除了返回消息，还返回 `ipc_status` 状态字。`IPC_STATUS_CALL(status)` 取低 6 位（ipcconst.h:22-24：`((status) >> 0) & 0x3F`），`NOTIFY = 4`（ipcconst.h:10）。**通知（notification）是内核的异步信号**，不是请求消息：

- 时钟 tick（来源 CLOCK task，endpoint -3）——携带 `m_notify.timestamp`（ipc.h:1715），PM 调用 `expire_timers`（libsys/timers.c:97）检查进程定时器（itimer 语义归 14）；
- 其他通知（内核中断、RS ping）——PM 不处理，直接 `continue`。

通知在 endpoint 验证**之前**被跳过（L65 在 L75 之前）：通知的来源可能是内核 task（负 endpoint），`pm_isokendpt` 无法验证它们。这是顺序敏感点——先跳通知，再验证 endpoint。

### 2.3 caller 验证与 EXITING 丢弃（main.c:73-82）

```c
who_e = m_in.m_source;                  /* who sent the message */
if (pm_isokendpt(who_e, &who_p) != OK)
    panic("PM got message from invalid endpoint: %d", who_e);
mp = &mproc[who_p];     /* process slot of caller */
call_nr = m_in.m_type;  /* system call number */

/* Drop delayed calls from exiting processes. */
if (mp->mp_flags & EXITING)
    continue;
```

- `who_e`/`who_p`/`mp`/`call_nr` 是文件级全局（glo.h:8/16-18：mp=8，m_in/who_p/who_e=16-17，call_nr=18），handler 无参直接读——C 的隐式上下文（Rust 显式化见 §3.5）。
- `pm_isokendpt`（utility.c:108-121）三层检查：槽位范围（EINVAL）→ endpoint 代数（EDEADEPT）→ IN_USE（EDEADEPT）。语义细节见 03 档 §3.2。
- **EXITING 丢弃**：调用者已进入退出流程（`mp_flags & EXITING`）时，它此前发出的延迟调用（如等待中的信号处理）不再有意义——直接丢弃，不进入分发。注意 VFS 的槽位（slot 1）永非 EXITING，因此这条只影响用户进程的残留调用。

### 2.4 三路分发（main.c:84-103）

**第一路：VFS 异步回复**（L84-87）：

```c
if (IS_VFS_PM_RS(call_nr) && who_e == VFS_PROC_NR) {
    handle_vfs_reply();
    result = SUSPEND;       /* don't reply */
}
```

`IS_VFS_PM_RS`（com.h:517）匹配 `VFS_PM_RS_BASE = 0x980`（com.h:514）族的 11 种回复（`VFS_PM_SETUID_REPLY` ~ `VFS_PM_SETGROUPS_REPLY`，com.h:534-544）。`who_e == VFS_PROC_NR` 双保险：类型在族内但来源不是 VFS 的消息不算 VFS 回复。`handle_vfs_reply`（main.c:295-424）是 11 种回复的独立状态机——完整语义归 05 档，本档只记分发钩子。注意它把 `result` 强制设为 SUSPEND：VFS 回复路径**不再向 VFS 回复**（回复的是当初挂起的用户进程）。

**第二路：事件订阅者回复**（L88-89）：

```c
} else if (call_nr == PROC_EVENT_REPLY) {
    result = do_proc_event_reply();
}
```

`PROC_EVENT_REPLY = COMMON_RS_BASE + 0 = 0xE80`（com.h:598/619）。事件订阅语义归 06 档。

**第三路：PM 系统调用**（L90-101）：

```c
} else if (IS_PM_CALL(call_nr)) {
    call_index = (unsigned int) (call_nr - PM_BASE);
    if (call_index < NR_PM_CALLS && call_vec[call_index] != NULL) {
        result = (*call_vec[call_index])();
    } else
        result = ENOSYS;
} else
    result = ENOSYS;
```

`IS_PM_CALL`（callnr.h:11）匹配低 8 位消息；`call_index = call_nr - PM_BASE` 索引 `call_vec`；越界（≥ `NR_PM_CALLS = 48`）或表项 NULL → ENOSYS。注意 `call_nr = 0` 落入 `IS_PM_CALL`（0 的低 8 位为 0）但 `call_vec[0]` 是 NULL（`PM_BASE + 0` 保留，table.c 无此表项）→ ENOSYS。

**兜底**（L102-103）：完全未知的消息类型 → ENOSYS。

### 2.5 reply()（main.c:250-270）

```c
void reply(int proc_nr, int result)
{
  struct mproc *rmp;
  int r;

  if(proc_nr < 0 || proc_nr >= NR_PROCS)
      panic("reply arg out of range: %d", proc_nr);      // L261-262

  rmp = &mproc[proc_nr];
  rmp->mp_reply.m_type = result;                          // L265

  if ((r = ipc_sendnb(rmp->mp_endpoint, &rmp->mp_reply)) != OK)
      printf("PM can't reply to %d (%s): %d\n",           // L267-269
              rmp->mp_endpoint, rmp->mp_name, r);
}
```

`reply()` 的三个要点：

1. **槽位范围校验**——`proc_nr` 越界 panic（不可恢复错误，§1.5）；
2. **复用 `mp_reply` 缓冲**——每个 mproc 槽有持久回复消息，handler 可预填载荷字段（如 do_get 填 `m_pm_lc_getpid`），`reply()` 只覆盖 `m_type`；这是"主返回值 + 附加字段"两通道通信的 C 实现；
3. **`ipc_sendnb`（非阻塞发送）**——发送失败只打印警告**不 panic**：调用者可能已死（如 do_exit 场景），回复失败不是 PM 的错。

### 2.6 call_vec 分发表（table.c:14-61）

```c
#define CALL(n)	[((n) - PM_BASE)]

int (* const call_vec[NR_PM_CALLS])(void) = {
	CALL(PM_EXIT)		= do_exit,	/* _exit(2) */
	CALL(PM_FORK)		= do_fork,	/* fork(2) */
	CALL(PM_WAIT4)		= do_wait4,	/* wait4(2) */
	CALL(PM_GETPID)		= do_get,	/* get[p]pid(2) */
	/* ... 47 项，完整清单见 §2.7 ... */
	CALL(PM_GETSYSINFO)	= do_getsysinfo	/* getsysinfo(2) */
};
```

C 用**函数指针表**把 47 个调用号映射到 handler：`CALL(n)` 宏展开为指定下标初始化（`[n - PM_BASE]`），表长 `NR_PM_CALLS = 48`（callnr.h:62）。表项下标 = 调用号 - PM_BASE。注意**一个 handler 可服务多个调用号**（如 `do_get` 服务 GETPID/GETUID/GETGROUPS/GETGID/GETSID 等，`do_set` 服务 SETUID/SETGID/...）——分发表是"调用号 → 函数"的映射，不是"函数 → 调用号"。

### 2.7 callnr.h：47 个调用号（callnr.h:14-60）

| 调用号 | 宏 | handler | 归属文档 |
|--------|----|---------|---------|
| 1 | `PM_EXIT` | `do_exit` | 09 |
| 2 | `PM_FORK` | `do_fork` | 07 |
| 3 | `PM_WAIT4` | `do_wait4` | 10 |
| 4 | `PM_GETPID` | `do_get` | 15 |
| 5 | `PM_SETUID` | `do_set` | 15 |
| 6 | `PM_GETUID` | `do_get` | 15 |
| 7 | `PM_STIME` | `do_stime` | 19 |
| 8 | `PM_PTRACE` | `do_trace` | 18 |
| 9 | `PM_SETGROUPS` | `do_set` | 15 |
| 10 | `PM_GETGROUPS` | `do_get` | 15 |
| 11 | `PM_KILL` | `do_kill` | 11 |
| 12 | `PM_SETGID` | `do_set` | 15 |
| 13 | `PM_GETGID` | `do_get` | 15 |
| 14 | `PM_EXEC` | `do_exec` | 17 |
| 15 | `PM_SETSID` | `do_set` | 15 |
| 16 | `PM_GETPGRP` | `do_get` | 15 |
| 17 | `PM_ITIMER` | `do_itimer` | 14 |
| 18 | `PM_GETMCONTEXT` | `do_getmcontext` | 20 |
| 19 | `PM_SETMCONTEXT` | `do_setmcontext` | 20 |
| 20 | `PM_SIGACTION` | `do_sigaction` | 12 |
| 21 | `PM_SIGSUSPEND` | `do_sigsuspend` | 12 |
| 22 | `PM_SIGPENDING` | `do_sigpending` | 12 |
| 23 | `PM_SIGPROCMASK` | `do_sigprocmask` | 12 |
| 24 | `PM_SIGRETURN` | `do_sigreturn` | 12 |
| 25 | `PM_SYSUNAME` | `do_sysuname` | 20 |
| 26 | `PM_GETPRIORITY` | `do_getsetpriority` | 16 |
| 27 | `PM_SETPRIORITY` | `do_getsetpriority` | 16 |
| 28 | `PM_GETTIMEOFDAY` | `do_time` | 19 |
| 29 | `PM_SETEUID` | `do_set` | 15 |
| 30 | `PM_SETEGID` | `do_set` | 15 |
| 31 | `PM_ISSETUGID` | `do_get` | 15 |
| 32 | `PM_GETSID` | `do_get` | 15 |
| 33 | `PM_CLOCK_GETRES` | `do_getres` | 19 |
| 34 | `PM_CLOCK_GETTIME` | `do_gettime` | 19 |
| 35 | `PM_CLOCK_SETTIME` | `do_settime` | 19 |
| 36 | `PM_GETRUSAGE` | `do_getrusage` | 20 |
| 37 | `PM_REBOOT` | `do_reboot` | 20 |
| 38 | `PM_SVRCTL` | `do_svrctl` | 20 |
| 39 | `PM_SPROF` | `do_sprofile` | 20 |
| 40 | `PM_PROCEVENTMASK` | `do_proceventmask` | 06 |
| 41 | `PM_SRV_FORK` | `do_srv_fork` | 08 |
| 42 | `PM_SRV_KILL` | `do_srv_kill` | 11 |
| 43 | `PM_EXEC_NEW` | `do_newexec` | 17 |
| 44 | `PM_EXEC_RESTART` | `do_execrestart` | 17 |
| 45 | `PM_GETEPINFO` | `do_getepinfo` | 20 |
| 46 | `PM_GETPROCNR` | `do_getprocnr` | 20 |
| 47 | `PM_GETSYSINFO` | `do_getsysinfo` | 20 |

47 个调用覆盖进程生命周期（1-3）、身份凭证（4-6/9-10/12-13/15-16/29-32）、时间（7/28/33-35）、调试（8/18-19）、信号（11/20-24/42）、执行（14/43-44）、定时器（17）、调度（26-27）、事件（40）、系统服务（37-39/41/45-47）。

### 2.8 调用统计（ENABLE_SYSCALL_STATS，main.c:34-36/95-97）

```c
#if ENABLE_SYSCALL_STATS
EXTERN unsigned long calls_stats[NR_PM_CALLS];
#endif
/* 分发处： */
#if ENABLE_SYSCALL_STATS
    calls_stats[call_index]++;
#endif
```

编译宏 `ENABLE_SYSCALL_STATS` 开启时，每次分发对 `calls_stats[call_index]` 计数；`do_getsysinfo` 的 `SI_CALL_STATS` 子命令可读取（misc.c:131-132）。这是**调试/性能分析用的可选面**（A-7 sanity 模式），默认关闭，Rust 侧归 20 档标注 cfg feature（plan §5.4）。

---

## 3. Rust 设计决策

### 3.1 D1: `IpcTransport` 增加 `receive` + `IpcStatus`（与 VM 同型）

- **C**: `sef_receive_status(ANY, &m_in, &ipc_status)`（main.c:61，libsys）；`is_ipc_notify(ipc_status)`（com.h:92）判 `IPC_STATUS_CALL == NOTIFY`（ipcconst.h:10/16）。
- **Rust 现状（01 档遗留）**: PM 的 `IpcTransport` 只有 `send`/`sendrec`（01 档为 VFS_PM_INIT 同步建立），无收消息原语——主循环无法落地。
- **决策**: trait 增加 `fn receive(&mut self) -> Result<(Message, IpcStatus), IpcError>`（transport.rs:51）；`IpcStatus { flags: u32 }` + `is_notify()`（transport.rs:23-40）——与 VM `os/servers/vm/src/ipc/transport.rs:46-63` 同型（trait 质量准则：≥2 个行为不同的 impl）。
- **理由**: 主循环的"收消息"与启动链的"发消息"是同一内核 IPC 边界的两面；trait 抽象保持单一通道，测试 mock 与生产实现共享同一接口。
- **行为契约**: `receive` 阻塞直到消息到达（C 语义）；`KernelIpcTransport::receive` 保持 `unimplemented!()` 自说明（minix-sys 内核 IPC 未落地）；`TestIpcTransport` 增加 `queue_receive` 预置消息队列。
- **三处一致**: transport.rs 注释 + design D1 + 本文档 §3.1。

### 3.2 D2: `ReplyIntent { Reply(i32), ReplyLater, NoReply }`（ARCH A-6）

- **C**: `result != SUSPEND → reply(who_p, result)`（main.c:106）；`SUSPEND = -998`（com.h:1151）。
- **决策**: dispatch 返回显式意图枚举（dispatcher.rs:45-52）：

```rust
pub enum ReplyIntent {
    Reply(i32),     // 主循环立即 reply(slot, code)
    ReplyLater,     // 本次不回复，稍后由异步路径回复（C: SUSPEND）
    NoReply,        // 永不回复（C: do_exit 子情形）
}
```

- **理由**: C 用 -998 魔法数隐式表达三种子情形（§1.4 表）；Rust 用类型区分，主循环只匹配 `Reply`，`ReplyLater`/`NoReply` 由各 handler 文档建模具体回复路径（05/09 等）。
- **行为契约**: 主循环 `if let Reply(code) = intent { reply(slot, code) }`；`ReplyLater`/`NoReply` 在 04 层都不回复——外部行为与 C `result == SUSPEND` 一致。
- **ARCH 标注**: 这是 plan §4 A-6（SUSPEND 显式化）的实现；标注于 dispatcher.rs 注释、design D2、本文档三处。
- **三处一致**: dispatcher.rs 注释 + design D2 + 本文档 §3.2。

### 3.3 D3: `call_vec` 函数指针表 → 类型化 `PmCall` match（ARCH A-5）

- **C**: `call_vec[NR_PM_CALLS]` 函数指针表（table.c:14-61）+ `call_index` 索引（main.c:92-99）。
- **Rust 现状（原型遗留）**: 旧 `MessageDispatcher::dispatch(table, request: PmRequest)` 只有 `PmRequest::Fork` 一个变体，与 C 的 47 调用面严重不匹配。
- **决策**: 三件套替换——
  1. `PmCall` 枚举（calls.rs:29-124）：47 个变体，`#[repr(i32)]` 判别值 = 调用号（`PM_BASE + 1` ~ `+47`）；
  2. `PmCall::from_call_nr(nr) -> Option<PmCall>`（calls.rs:133）：未注册号（含 0 保留值）→ None → 分发层 ENOSYS；
  3. `dispatch_pm_call(call, table, caller) -> ReplyIntent`（calls.rs:201）：match 路由到 handler。

- **理由**: Rust 无 C 式"表驱动 void 指针"；match 是类型安全分发（编译期穷尽 47 项，`PmCall` 不可能携带未注册号）；`from_call_nr` 把"调用号合法性"变成类型转换的一部分。
- **行为契约**: 47 个调用号逐一映射（`test_call_nr_roundtrip_all_registered` 验证 1..=47 全部可解码可回编码）；未注册号 → ENOSYS（与 C 越界槽位一致）。
- **ARCH 标注**: plan §4 A-5（call_vec → match 分发）；标注于 calls.rs 注释、design D3、本文档三处。
- **三处一致**: calls.rs 注释 + design D3 + 本文档 §3.3。

**A-4 与 A-5 的边界**：本档的分发（A-5）按调用号路由——`PmCall` 枚举判别值即调用号，handler 由 `dispatch_pm_call` 的 match 决定；typed IPC（`PmRequest`/`PmResponse`/`PmError`，`minix-types/src/ipc/pm.rs:11/23/37`）是 handler 载荷契约（A-4，plan §4），即"调用号之外的消息体字段"的解码/编码层。A-4 的载荷扩展由各 handler 文档（07+）落地时实施，04 层只负责调用号分发与 ENOSYS 兜底，不接触载荷字段。

### 3.4 D4: 主循环 `run_once`/`run` 拆分（可测性）

- **C**: `while (TRUE) { ... }`（main.c:59-107）——不可测的单块循环。
- **决策**: `PmServer::run_once(&mut self) -> RunStep`（init.rs:279）处理单条消息；`PmServer::run(&mut self) -> !`（init.rs:242）持有无限循环 + 连续 receive 失败上限（`MAX_CONSECUTIVE_RECV_FAILURES = 64`，init.rs:556）；`RunStep { Handled, ReceiveFailed }`（init.rs:562）。
- **理由**: 测试驱动单轮 dispatch→reply 无需 spawn 无限循环（与 VM `vm_server.rs::run_once` 同型）；receive 失败连续上限防 busy-spin（C 的 receive 失败直接 panic 等价于 fail-fast，但 Rust 侧先计数 64 次——VM V10-P0-2 同款策略，避免瞬时故障误杀）。
- **行为契约**: `run()` 在 `run_once` 返回 `Handled` 时清零失败计数；连续失败 ≥ 64 → panic（`IPC transport permanently broken`）。
- **三处一致**: init.rs 注释 + design D4 + 本文档 §3.4。

### 3.5 D5: `m_in`/`who_p`/`who_e`/`call_nr` 全局 → 显式参数（ARCH A-3）

- **C**: `EXTERN message m_in; EXTERN int who_p, who_e, call_nr`（glo.h:8/16-18：mp=8，m_in/who_p/who_e=16-17，call_nr=18）——文件级全局，handler 无参可读。
- **决策**: 消息作为 `&Message` 参数传入 `dispatch_message`；caller 槽位由 `pm_isokendpt` 返回 `UserSlot`；handler 显式接收 `&mut ProcTable` + `Endpoint`。
- **理由**: 全局状态是 C 的隐式上下文；Rust 显式传参让 handler 签名自文档化（"这个 handler 需要什么状态"一目了然），借用检查器约束可变性。`UserSlot`（03 档 A-11）表达已验证的槽位，杜绝"魔法下标"。
- **行为契约**: 与 C 等价的 handler 可见状态（表 + 消息 + caller endpoint）。
- **ARCH 标注**: plan §4 A-3（全局状态 → PmContext）在 04 的落实：`PmContext` 已由 03 档实现，04 的 `run_once` 使用 `pm_isokendpt` 返回的 `UserSlot` 而非全局 `who_p`。
- **三处一致**: init.rs 注释 + design D5 + 本文档 §3.5。

### 3.6 D6: ENOSYS 契约 + panic 场景（与 C 一致）

| C 路径 | C 位置 | Rust 等价 | 差异说明 |
|--------|--------|----------|---------|
| 未注册/越界调用 → ENOSYS | main.c:94-101 | `PmCall::from_call_nr → None` → `Reply(ENOSYS)` | 一致 |
| 非 PM 消息 → ENOSYS | main.c:102-103 | dispatch 兜底 `Reply(ENOSYS)` | 一致 |
| 已注册但 handler 未实现 | —（C 全实现） | `dispatch_pm_call` 默认臂 `Reply(ENOSYS)` | **过渡差异**：46 个 handler DEFERRED 到 07~20，落地前返回 ENOSYS |
| receive 失败 → panic | main.c:61-62 | 连续 64 次失败 → panic | 计数防瞬断（D4） |
| endpoint 非法 → panic | main.c:75-76 | `panic!("PM got message from invalid endpoint")` | 一致（fail-fast） |

**ENOSYS 占位与 C 的差异论证**：C 中 47 个调用号全部有 handler；Rust 在 07~20 档落地前对未实现调用返回 ENOSYS。这不是外部语义变化——未实现的 handler 在 minix-rs 当前阶段**本就不存在**，ENOSYS 是诚实的"暂不支持"，且与 C 对 NULL 表项的防御语义同源（C 的 NULL 分支同样是 ENOSYS）。Fork 例外：它是唯一"分发层语义可完整建模"的调用（C 中返回 SUSPEND，见 §4.3），故 04 已实现其分发层行为。

**panic vs 丢弃的边界**：PM 的 endpoint 验证失败 panic（与 C 一致），而 VM 选择丢弃（VM A-14）——差异原因：PM 的 `mproc` 表是进程语义的权威，内核必须给它合法 endpoint；VM 没有进程表概念，坏消息丢弃即可。两者都符合各自 C 语义。

---

## 4. 实现详解

### 4.1 transport.rs：receive 与 IpcStatus

`IpcStatus`（transport.rs:23-40）只暴露 `is_notify()`——主循环唯一需要的判定：

```rust
pub fn is_notify(&self) -> bool {
    // C: IPC_STATUS_CALL(status) = (status >> 0) & 0x3F（ipcconst.h:22-24）。
    (self.flags & 0x3F) == 4 // NOTIFY
}
```

trait 新增 `receive`（transport.rs:51）。`KernelIpcTransport::receive`（transport.rs:84）保持 `unimplemented!()` 自说明——内核 IPC 核心（minix-sys）未落地，生产路径编译通过但运行会明确失败。`TestIpcTransport` 增加 `queue_receive(msg, status)`（transport.rs:133）与单次消费的 receive（transport.rs:150）——主循环测试预置消息、断言回复。

### 4.2 calls.rs：47 调用分发表

`PmCall` 枚举（calls.rs:29-124）47 个变体，判别值 = 调用号。`from_call_nr`（calls.rs:133）显式匹配 1..=47；`call_nr()`（calls.rs:188）返回判别值。`dispatch_pm_call`（calls.rs:201）：

```rust
pub fn dispatch_pm_call(call: PmCall, table: &mut ProcTable, caller: Endpoint) -> ReplyIntent {
    match call {
        // C: do_fork 返回 SUSPEND（forkexit.c:139）——fork 同步不回复。
        PmCall::Fork => ReplyIntent::ReplyLater,
        // 其余 46 个调用：handler 归属 07~20（ENOSYS 占位）。
        _ => ReplyIntent::Reply(ENOSYS),
    }
}
```

**关键设计点：fork 的分发层语义**。C 的 `do_fork` 在发出 `VFS_PM_FORK` 后返回 SUSPEND（forkexit.c:139），父/子回复由 05 档的 `VFS_PM_FORK_REPLY` 处理（main.c:369-394）异步完成。因此 `PmCall::Fork → ReplyLater` 是 C 语义的直接建模——**不是**旧的同步 `Ok(child_pid)` 原型（该原型是 07 档落地前的最小占位，与 C 不符，已由本档修正为分发层 SUSPEND 语义）。

### 4.3 dispatcher.rs：三路分发（仅事件回复 + PM 调用两路）

> **2026-09-02 接线修正**：VFS→PM 异步回复（`IS_VFS_PM_RS && source == VFS`）已移至 `PmServer::run_once` 主循环**第一路**拦截（main.c:84-87 在 `while` 循环体最前），由 `handle_vfs_reply` 状态机（05）处理，不再进入本分发函数。`dispatcher.rs` 因此只保留事件回复与 PM 调用两路，无死分支。

`dispatch_message`（dispatcher.rs:73）镜像 main.c:84-103 的后两路：

```rust
pub fn dispatch_message(table: &mut ProcTable, msg: &Message) -> ReplyIntent {
    let call_nr = msg.m_type;

    if call_nr == PROC_EVENT_REPLY {
        // C: main.c:88-89 — do_proc_event_reply()。
        ReplyIntent::ReplyLater          // 钩子：语义归 06
    } else if is_pm_call(call_nr) {
        match PmCall::from_call_nr(call_nr) {
            Some(call) => dispatch_pm_call(call, table, msg.m_source),
            None => ReplyIntent::Reply(ENOSYS),   // 未注册号
        }
    } else {
        ReplyIntent::Reply(ENOSYS)       // 非 PM 消息
    }
}
```

族判定函数与 C 掩码完全一致（dispatcher.rs:56-62）：

```rust
pub fn is_vfs_pm_rs(nr: i32) -> bool { (nr & !0x7f) == VFS_PM_RS_BASE }  // com.h:517
pub fn is_pm_call(nr: i32) -> bool { (nr & !0xff) == 0 }                 // callnr.h:11
```

**钩子设计**：事件回复（06）在 04 只建"返回 ReplyLater"的钩子；VFS 回复由 `run_once` 第一路拦截处理（见 §4.4）。语义正确性：C 中这两路都不向来源回复（VFS 回复路径回复的是当初挂起的用户进程，事件回复路径由 06 决定）；04 落地前钩子让主循环结构完整，06 落地时替换为真实状态机调用。

### 4.4 init.rs：主循环

`run_once`（init.rs:289-345）镜像 C 主循环单轮（main.c:59-106）七步：

```rust
fn run_once(&mut self) -> RunStep {
    // 1. 收消息（C: main.c:61）
    let (msg, rcv_sts) = match self.transport.receive() {
        Ok(v) => v,
        Err(_) => return RunStep::ReceiveFailed,
    };
    // 2. 通知跳过（C: main.c:65-71；CLOCK → expire_timers 归 14）
    if rcv_sts.is_notify() {
        return RunStep::Handled;
    }
    // 3. caller 验证（C: main.c:74-77；失败 panic，与 C 一致）
    let caller = match self.table.pm_isokendpt(msg.m_source) {
        Ok(slot) => slot,
        Err(_) => panic!("PM got message from invalid endpoint: {}", msg.m_source.get()),
    };
    // 4. EXITING 丢弃（C: main.c:80-82）
    if self.table.get(caller.get()).expect("pm_isokendpt validated slot").is_exiting() {
        return RunStep::Handled;
    }
    // 5. 第一路：VFS 异步回复（C: main.c:84-87 —— 必须在 dispatch_message 之前拦截）
    if is_vfs_pm_rs(msg.m_type) && msg.m_source == Endpoint::VFS {
        let mut svc = PmServices::new(&mut self.table, &mut self.transport, self.abort_flag);
        if let Err(e) = handle_vfs_reply(&mut svc, &msg) {
            panic!("handle_vfs_reply failed: {:?}", e); // 与 C 四句 panic 等价
        }
        return RunStep::Handled; // 不向 VFS 同步回复
    }
    // 6. 三路分发（剩余两路：事件回复 / PM 调用，C: main.c:88-103）
    let intent = dispatch_message(&mut self.table, &msg);
    // 7. 回复（C: main.c:106 — result != SUSPEND → reply）
    if let ReplyIntent::Reply(code) = intent {
        self.reply(caller, code);
    }
    RunStep::Handled
}
```

> **VFS 回复拦截的位置**：第 5 步在 `dispatch_message` **之前**——这与 C `main.c:84-87` 是 `while` 循环体内 `switch` 的**第一分支**（位于 `dispatch_message` 对应的三路之前）结构一致。05 落地后，`handle_vfs_reply` 在此即时施加效果（回复挂起进程 / 调度 / 发布事件），不再需要 04 的 `ReplyLater` 占位钩子。

`reply`（init.rs:337-349）镜像 main.c:250-270：槽位已验证（`UserSlot` 来自 `pm_isokendpt`），构造 `Message { m_type: result, .. }` 经 transport 发送；发送失败只记录不 panic（C 打印警告语义）。

```rust
fn reply(&mut self, slot: UserSlot, result: i32) {
    let endpoint = self.table.procs[slot.get()].endpoint();
    let msg = Message { m_type: result, ..Message::default() };
    if let Err(e) = self.transport.send(endpoint, &msg) {
        // C: printf("PM can't reply to %d (%s): %d") — 警告不 panic。
        let _ = e;
        #[cfg(test)]
        eprintln!("PM can't reply to {}: {:?}", endpoint.get(), e);
    }
}
```

`run`（init.rs:242-262）提供无限循环 + 失败计数上限（`MAX_CONSECUTIVE_RECV_FAILURES = 64`）。

### 4.5 关键不变量

| # | 不变量 | 保证方式 |
|---|--------|---------|
| 1 | 每条非通知消息必有确定结果 | `dispatch_message` 全路径返回 `ReplyIntent`（无 panic 泄漏到 handler 之外） |
| 2 | `ReplyIntent::Reply(code)` 的 code 是合法 errno/载荷值 | handler 契约（errno 映射，minix-types） |
| 3 | 通知在 endpoint 验证前跳过 | `run_once` 步骤 2 在步骤 3 之前（与 C main.c:65-75 顺序一致） |
| 4 | 只有已验证 caller 的消息进入分发 | `pm_isokendpt` 返回 `UserSlot` 后才构造 dispatch 调用 |
| 5 | EXITING 调用者的消息不进入分发 | 步骤 4 提前返回 |
| 6 | 发送失败不 panic（回复尽力而为） | `reply` 捕获 Err 仅记录（C 同语义） |
| 7 | receive 失败不无限自旋 | `run` 连续失败计数 ≥ 64 → panic |

---

## 5. 测试要点

### 5.1 测试矩阵（与 C 行为逐项对应）

| # | 测试 | C 行为对照 | 位置 |
|---|------|-----------|------|
| 1 | `test_run_once_skips_notify` | main.c:65-71 通知跳过，不产生回复 | init.rs |
| 2 | `test_run_once_invalid_endpoint_panics` | main.c:75-76 panic | init.rs |
| 3 | `test_run_once_drops_exiting_caller` | main.c:80-82 EXITING 丢弃 | init.rs |
| 4 | `test_run_once_replies_enosys_to_unimplemented_call` | main.c:106 reply + ENOSYS 占位 | init.rs |
| 5 | `test_run_once_fork_no_sync_reply` | forkexit.c:139 do_fork 返回 SUSPEND | init.rs |
| 6 | `test_run_once_vfs_reply_no_sync_reply` | main.c:84-87 VFS 回复 SUSPEND | init.rs |
| 7 | `test_run_once_receive_failure_reported` | receive 失败 → ReceiveFailed | init.rs |
| 8 | `test_run_panics_after_consecutive_receive_failures` | receive 失败 fail-fast（64 次） | init.rs |
| 9 | `test_is_vfs_pm_rs_matches_c` | com.h:517 掩码语义 | dispatcher.rs |
| 10 | `test_is_pm_call_matches_c` | callnr.h:11 掩码语义 | dispatcher.rs |
| 11 | `test_vfs_pm_rs_reply_routes_to_reply_later` | main.c:84-87 + 非 VFS 来源兜底 | dispatcher.rs |
| 12 | `test_proc_event_reply_routes_to_reply_later` | main.c:88-89 | dispatcher.rs |
| 13 | `test_pm_call_routes_to_dispatch_pm_call` | main.c:90-101（未实现 → ENOSYS；fork → SUSPEND） | dispatcher.rs |
| 14 | `test_unknown_type_returns_enosys` | main.c:102-103 | dispatcher.rs |
| 15 | `test_call_nr_roundtrip_all_registered` | callnr.h:14-60 47 项全注册 | calls.rs |
| 16 | `test_call_nr_rejects_unregistered` | callnr.h:62 越界/NULL → ENOSYS | calls.rs |
| 17 | `test_dispatch_fork_is_reply_later` | forkexit.c:139 | calls.rs |
| 18 | `test_dispatch_unimplemented_call_is_enosys` | 未实现 handler ENOSYS 占位 | calls.rs |
| 19 | `test_mock_receive_returns_queued_message` | receive 队列单次消费 | transport.rs |
| 20 | `ipc_status_notify_bit_matches_minix3` | ipcconst.h:10/16/22-24 NOTIFY=4 | transport.rs |

### 5.2 测试统计（截至 2026-08-17）

- `cargo test -p minix-pm --lib`：**120 passed / 0 failed**（基线 101 → 本档 +19）
- `cargo test -p minix-types --lib`：**94 passed / 0 failed**（本档未改动 minix-types）
- 完整测试清单：`rg "^\s*fn test_" os/servers/pm/src/`
- 本档直接相关模块：`init.rs` 主循环 8（§5.1 列 1-8）、`ipc/dispatcher.rs` 6（列 9-14）、`ipc/calls.rs` 4（列 15-18）、`ipc/transport.rs` 5（列 19-20 为本档新增 2，另 3 为 03 档既有 send/sendrec 测试）——§5.1 共 20 项

---

## 6. 过渡

主循环是 PM 启动链（01）的终点与运行时心脏：`sef_cb_init_fresh`（01）建立进程表后，`main()` 进入 `run()`（本档）。此后 PM 的一切对外行为都发生在主循环的六步里。

**下一入口**：

- **05-vfs-interaction.md**——主循环第一路 `IS_VFS_PM_RS → handle_vfs_reply` 的 11 种回复状态机；04 只留下"ReplyLater"钩子，05 落地后替换为真实状态机调用。
- **06-event-subscription.md**——主循环第二路 `PROC_EVENT_REPLY → do_proc_event_reply` 的事件订阅语义。
- **07~20**——主循环第三路的 47 个 handler：04 的分发表已完整，各 handler 在对应文档落地时替换 `ENOSYS` 占位。

---

## 7. 参见

- `notes/rewrite/fork-syscall-rewrite/04-stage-pm/plan.md` §1.2（主循环时序图）、§3.4（边界表）、§4（A-3~A-6）、§5.3（函数归属）、§7.3（SUSPEND 语义契约）
- `01-pm-init-main.md` — 启动链进入主循环（main.c:49-56 → sef_local_startup → init_fresh）
- `03-mproc-table.md` — `pm_isokendpt`（本档调用点）、`UserSlot`
- `05-vfs-interaction.md` — VFS 异步回复状态机（第一路）
- `06-event-subscription.md` — 进程事件订阅（第二路）
- `07-pm-fork.md` — do_fork 本体（fork 分发层 SUSPEND 语义的消费方）
- `14-itimer.md` — CLOCK notify 的 `expire_timers` 处理
- `99-global-concepts.md` — 消息类型空间、endpoint 语义
- `../01-stage-kernel/06-proc-init-boot-proc.md` — 内核如何启动 PM（主循环的服务前提）
