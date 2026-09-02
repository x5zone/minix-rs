# 05 — PM 与 VFS 的异步 IPC 协议

本文档讲清 PM（进程管理器）与 VFS（虚拟文件系统）之间的异步 IPC 协议全貌：协议面（哪些消息、什么字段）、C 源码如何发送与收口回复、Rust 改写如何把"裸 union 消息 + 隐式标志位延续"提升为"类型化协议编解码 + 端口-适配器状态机"，以及这一设计相对其他操作系统最佳实践的取舍。

前置阅读：01-pm-init-main.md（VFS_PM_INIT 启动握手）、02-mproc-struct.md（BlockState 与 NEW_PARENT/UNPAUSED 建模）、03-mproc-table.md（`pm_isokendpt`）、04-ipc-dispatch.md（主循环三路分发与 VFS 回复第一路拦截）。

---

## 1 概念

### 1.1 为什么 PM 与 VFS 必须协作

Minix3 把"进程管理"与"文件管理"拆成两个独立的用户态服务器。一个进程的大部分生命周期操作都牵涉文件系统状态，因此必须请 VFS 配合：

- `setuid`/`setgid`/`setgroups`：改变进程的有效/真实身份后，VFS 需要同步 fd 表的 ownership。
- `fork`：子进程必须复制父进程的打开文件表，这是 VFS 的职责。
- `exec`：加载新可执行镜像、设置参数与环境，由 VFS 完成。
- `exit`：进程退出时，VFS 必须释放其持有的 fd。
- `unpause`：信号送达后让被暂停的进程恢复，需要 VFS 配合。
- `reboot`：系统重启，PM 通过 VFS 触发内核中止。

PM 从不自己碰文件系统，它只发请求、收回复。VFS 也从不碰进程表，它只按 PM 的请求操作 fd 表、再回复。这种职责分离正是 Minix3 微内核设计的精髓。

### 1.2 为什么必须是异步的

PM 是单线程事件循环（见 04-ipc-dispatch.md）：它在一个 `while` 循环里收消息、分发、回复。如果 PM 用同步 `sendrec` 把请求发给 VFS 并等待回复，PM 主循环就会阻塞在这条消息上。

问题在于：**VFS 在处理 PM 的请求时，常常反过来需要 PM 的信息**。最典型的例子是 `fork`：VFS 复制 fd 表后，可能要查询子进程的调度属性，而这份信息只在 PM 手里。若 PM 此刻正同步阻塞等待 VFS 的 fork 回复，VFS 的反向请求就会被饿死——**双向同步调用即死锁**。

C 的解法是非阻塞发送（`asynsend3(VFS_PROC_NR, m_ptr, AMF_NOREPLY)`，见 utility.c:134），请求"投出即忘"，回复稍后由 PM 主循环的第一路分收集口。这是一种 continuation-passing 风格的异步：PM 发出请求时把"接下来要做什么"暂存在目标进程的 `mproc.mp_flags` 上（置 `VFS_CALL`），待回复到来时再依据标志位恢复执行。请求与回复因此是**解耦**的两条消息，而非一次调用。

### 1.3 与其他操作系统最佳实践的对比

Rust 改写不是照抄 C 的裸 union 写法，而是在吸收工业级 OS 的成熟模式后做取舍。

**Redox 的 `Scheme` 同步模型。** Redox 的驱动实现 `Scheme::handle(packet)`，内核把调用者的线程停放，scheme 处理完包即回。这本质上是**同步阻塞**：内核托管了"调用者—处理器"的配对，处理器处理期间调用者不参与调度。PM **不能**采用此模型——PM 与 VFS 互为调用者，任一方同步阻塞都将死结（§1.2）。Redox 之所以能同步，是因为 scheme 不会反向调用内核托管的另一 scheme 并保持阻塞；而 PM/VFS 的环形依赖使得同步不可行。

**seL4 的 reply-object / continuation。** seL4 把"回复对象"作为一等公民：handler 可以保存一个 reply capability，稍后任意时刻用它向原调用者回复，continuation 因此显式化。Minix3 的"异步请求 + 稍后回复"在语义上等价于 seL4 的 reply-object，但 Minix3 没有显式的 continuation 对象，而是把延续**隐式挂在一个进程的标志位**（`mp_flags & VFS_CALL`）上——同一个进程不可能同时有两个未完成的 VFS 调用（否则 `tell_vfs` panic "not idle"），因此一个标志位足以充当 continuation 的槽位。

**Fuchsia 的 FIDL。** FIDL 为异步 IPC 生成类型安全的消息编解码，编译期排除"读了错误字段"的错误。Minix3 的 C 实现恰恰相反：它用 `switch(m_in.m_type)` 后再裸读 `m7_i2`/`m7_p1` 等魔法字段下标，类型系统不参与。Rust 改写应吸收 FIDL 的核心思想——**类型化的消息**——把每个回复的载荷与其消息类型绑定，使"读错字段"在编译期不可表达。

**结论（本档的设计基线）。** 把 C 的"消息 union 裸写 + 标志位隐式延续"改写为"类型化协议编解码（minix-types）+ 端口-适配器状态机（minix-pm）"。延续从散落在 `mp_flags` 与各调用点的隐式控制流，提升为状态机显式调用的端口方法（`VfsReplyServices`）。这与 seL4 的显式 continuation、FIDL 的类型化消息同构，又因 PM 单线程 no_std 事件循环而避免引入堆分配与异步运行时。

---

## 2 C 源码分析

### 2.1 协议面：消息类型与字段（com.h:513-583）

两个消息族共享基底：

```c
#define VFS_PM_RQ_BASE  0x900
#define VFS_PM_RS_BASE  0x980
#define IS_VFS_PM_RQ(type)  (((type) & ~0x7f) == VFS_PM_RQ_BASE)
#define IS_VFS_PM_RS(type)  (((type) & ~0x7f) == VFS_PM_RS_BASE)
```

`~0x7f` 掩去低 7 位，因此 0x900–0x97f 都是请求族、0x980–0x9ff 都是回复族。**注意 `VFS_PM_INIT`（0x900）同属请求族**——它是启动期一次性握手，与运行时 11 种请求的语义不同（见 01-pm-init-main.md）。

请求族（com.h:520-531）共 12 个，其中 `VFS_PM_INIT` 单独处理，运行时 11 种：

| 常量 | 值 | 语义 |
|---|---|---|
| `VFS_PM_INIT` | 0x900 | 进程表交换（启动期） |
| `VFS_PM_SETUID` | 0x901 | 设置 UID |
| `VFS_PM_SETGID` | 0x902 | 设置 GID |
| `VFS_PM_SETSID` | 0x903 | 创建新会话 |
| `VFS_PM_EXIT` | 0x904 | 进程退出 |
| `VFS_PM_DUMPCORE` | 0x905 | core dump |
| `VFS_PM_EXEC` | 0x906 | 装载可执行镜像 |
| `VFS_PM_FORK` | 0x907 | 复制父进程 fd 表 |
| `VFS_PM_SRV_FORK` | 0x908 | 服务进程 fork |
| `VFS_PM_UNPAUSE` | 0x909 | 解除进程挂起 |
| `VFS_PM_REBOOT` | 0x90a | 系统重启 |
| `VFS_PM_SETGROUPS` | 0x90b | 设置补充组 |

回复族（com.h:534-544）共 11 种，与请求一一对应（除 INIT 无回复）：

| 常量 | 值 | 语义 |
|---|---|---|
| `VFS_PM_SETUID_REPLY` | 0x981 | |
| `VFS_PM_SETGID_REPLY` | 0x982 | |
| `VFS_PM_SETSID_REPLY` | 0x983 | |
| `VFS_PM_EXIT_REPLY` | 0x984 | |
| `VFS_PM_CORE_REPLY` | 0x985 | |
| `VFS_PM_EXEC_REPLY` | 0x986 | |
| `VFS_PM_FORK_REPLY` | 0x987 | |
| `VFS_PM_SRV_FORK_REPLY` | 0x988 | |
| `VFS_PM_UNPAUSE_REPLY` | 0x989 | |
| `VFS_PM_REBOOT_REPLY` | 0x98a | |
| `VFS_PM_SETGROUPS_REPLY` | 0x98b | |

**寻址字段**：除 REBOOT 外，所有消息都用 `m7_i1`（`VFS_PM_ENDPT`，com.h:547）承载目标进程 endpoint。这是"process-associated"请求的寻址键——回复到达时，PM 用这个 endpoint 反查槽位。

**载荷字段**（com.h:549-583）按请求类型复用 mess_7 字段：

- `SETUID`/`SETGID`：`m7_i2`=eid（有效 id）、`m7_i3`=rid（真实 id）。
- `SETGROUPS`：`m7_i2`=组数、`m7_p1`=组数组指针。
- `EXEC`：`m7_p1`=路径、`m7_i2`=路径长、`m7_p2`=frame（参数/环境）、`m7_i3`=frame 长、`m7_i5`=ps_str。
- `EXEC_REPLY`/`CORE_REPLY`：`m7_i2`=status（OK 或失败）、`m7_p1`=pc、`m7_p2`=新栈指针、`m7_i5`=新 ps_str。
- `FORK`/`SRV_FORK`：`m7_i2`=父 endpoint、`m7_i3`=子 pid、`m7_i4`=reuid、`m7_i5`=regid。
- `DUMPCORE`：`m7_i2`=终止信号。

### 2.2 发送方：`tell_vfs`（utility.c:123-139）

```c
void tell_vfs(rmp, m_ptr)
struct mproc *rmp;
message *m_ptr;
{
  int r;
  if (rmp->mp_flags & (VFS_CALL | EVENT_CALL))
    panic("tell_vfs: not idle: %d", m_ptr->m_type);
  r = asynsend3(VFS_PROC_NR, m_ptr, AMF_NOREPLY);
  if (r != OK)
    panic("unable to send to VFS: %d", r);
  rmp->mp_flags |= VFS_CALL;
}
```

三段式，顺序不可调换：

1. **not-idle 检查**：目标进程若已置 `VFS_CALL` 或 `EVENT_CALL`，说明上一轮 VFS/VFS-event 请求尚未收回复，再发就是逻辑错误 → `panic`。这保证了"一个进程同一时刻最多一个未完成的 VFS 请求"。
2. **非阻塞发送**：`asynsend3(..., AMF_NOREPLY)` 投出即返，绝不等待。
3. **置位**：发送成功后才置 `VFS_CALL`。顺序关键——若先置位再发送，一旦发送失败就会留下"置位但无请求"的悬挂标志。

### 2.3 收口方：`handle_vfs_reply`（main.c:295-424）

这是整个协议的核心，结构清晰分四段。

**第一段：REBOOT 特例（main.c:304-312）。**

```c
if (call_nr == VFS_PM_REBOOT_REPLY) {
    sys_abort(abort_flag);
    return;
}
```

`VFS_PM_REBOOT_REPLY` 是唯一**不与任何进程关联**的回复——它不携带 `VFS_PM_ENDPT`，因此必须抢在"解析 endpoint"之前单独处理。

**第二段：endpoint 解析与三处不变式（main.c:315-331）。**

```c
proc_e = m_in.VFS_PM_ENDPT;
if (pm_isokendpt(proc_e, &proc_n) != OK)
    panic("handle_vfs_reply: got bad endpoint from VFS: %d", proc_e);
rmp = &mproc[proc_n];
if (!(rmp->mp_flags & VFS_CALL))
    panic("handle_vfs_reply: reply without request: %d", call_nr);
new_parent = rmp->mp_flags & NEW_PARENT;
rmp->mp_flags &= ~(VFS_CALL | NEW_PARENT);
if (rmp->mp_flags & UNPAUSED)
    panic("handle_vfs_reply: UNPAUSED set on entry: %d", call_nr);
```

四道不变量（任一违反即 fail-fast panic，与 C 文本一致）：

1. 回复里的 endpoint 必须能解析为有效槽位，否则 `bad endpoint`。
2. 该进程必须正处于 `VFS_CALL`（确有未完成的请求），否则 `reply without request`。
3. 清除 `VFS_CALL` 与 `NEW_PARENT`，同时取出 `new_parent` 供后续分支使用——这是一个原子式"取延续 + 清标志"的合并操作。
4. 进入时绝不能已置 `UNPAUSED`（`UNPAUSED` 只可能在本函数 UNPAUSE 分支内被设置，见 §2.4），否则 `UNPAUSED set on entry`。

**第三段：11 路 switch（main.c:334-419）。** 逐路对应一个回复类型，行为如下：

| 回复 | 行为 |
|---|---|
| `SETUID`/`SETGID`/`SETGROUPS` | `reply(rmp, OK)` 唤醒原调用者 |
| `SETSID` | `reply(rmp, rmp->mp_procgrp)` 唤醒原调用者（返回新进程组号） |
| `EXEC` | `exec_restart(rmp, status, pc, newsp, newps_str)` 重启用户进程 |
| `CORE` | 若 `status==OK` 则 `mp_sigstatus \|= WCOREFLAG`，**fallthrough** 到 `EXIT` |
| `EXIT` | `assert(EXITING)` → `publish_event(rmp)` → **提前 return**（不执行尾部） |
| `FORK` | `sched_start_user` 成败双分支（见下） |
| `SRV_FORK` | 无操作 |
| `UNPAUSE` | `assert(PROC_STOPPED)` → 置 `UNPAUSED` → `publish_event` → **提前 return**（不执行尾部） |
| `default` | `panic("unknown reply code")` |

`FORK` 分支（main.c:369-396）最关键：先尝试 `sched_start_user` 调度新进程；若调度失败则 `exit_proc(rmp, -1, FALSE)` 拆解子进程，并仅在 `!new_parent` 时 `reply(parent, -1)`；若成功则 `reply(child, OK)` 并 `reply(parent, child_pid)`（同样仅在 `!new_parent` 时回复父进程——父进程已死则跳过）。

**第四段：尾部 `restart_sigs`（main.c:421-423）。**

```c
if ((rmp->mp_flags & (IN_USE | EXITING)) == IN_USE)
    restart_sigs(rmp);
```

进程重新回到 idle 后，检查是否有挂起信号待重投。条件 `(IN_USE | EXITING) == IN_USE` 意味着"进程仍在使用中且未处于退出流程"才重投信号。`EXIT` 与 `UNPAUSE` 分支在 `publish_event` 后**直接 return**，因此**跳过尾部**——它们已通过事件发布完成善后，不该再 `restart_sigs`（EXIT 的进程正在退出，UNPAUSE 的进程刚被唤醒由信号驱动）。

### 2.4 七个 `tell_vfs` 调用点及其"发出后做什么"

| 调用点 | 发出的请求 | 回复收口时做什么 |
|---|---|---|
| forkexit.c:130（do_fork） | `VFS_PM_FORK` | 子进程置 `VFS_CALL`；回复到来时调度子进程、回复父子（§2.3 FORK 分支） |
| forkexit.c:230（do_pm_exit） | `VFS_PM_EXIT` | VFS 释放 fd；`EXIT_REPLY` 发布退出事件并继续退出 |
| forkexit.c:359（exit_proc 重发） | `VFS_PM_EXIT` | 同上；用于退出流程中再次请求 VFS 清理 |
| getset.c:219（do_setuid/do_setgid/do_setsid） | `VFS_PM_SETUID`/`SETGID`/`SETSID` | 回复 `OK`（或 procgrp）唤醒等待的调用者 |
| exec.c:52（do_exec） | `VFS_PM_EXEC` | `EXEC_REPLY` 经 `exec_restart` 重启用户进程 |
| signal.c:767（do_unpause） | `VFS_PM_UNPAUSE` | `UNPAUSE_REPLY` 置 `UNPAUSED` 并发布信号事件 |
| misc.c:230（do_reboot） | `VFS_PM_REBOOT` | `REBOOT_REPLY` 触发 `sys_abort`（§2.3 第一段） |

要点：**发出请求后，调用点本身不等待、不立即做事**——它只是置 `VFS_CALL` 把延续挂起，真正的效果在将来某次主循环的 `handle_vfs_reply` 第一路分收集口时施加。这是异步协议的精髓。

### 2.5 `NEW_PARENT` 与 `UNPAUSED` 的生命周期

**`NEW_PARENT`** 是一个**跨调用**标志。唯一设置点在 forkexit.c:402-403（`exit_proc` 中，当进程被 init 收养时 `mp_flags |= NEW_PARENT`）。它的含义是"本进程的原父进程已死，下一次 VFS 回复不要再回复父进程"。它在 handle_vfs_reply 第二段被读取并清除（main.c:327-328），因此跨越"设置它的退出流程"与"下一次 VFS 回复"两个事件。FORK 回复分支据此决定 `reply(parent, ...)` 是否执行。

**`UNPAUSED`** 是**瞬态**标志。它只在 `UNPAUSE_REPLY` 分支内设置（main.c:410），且进入 handle_vfs_reply 时若已置则 panic（main.c:330-331）。这个不变式保证：一个进程在 unpause 回复被处理的瞬间，一定是 `PROC_STOPPED` 的（main.c:407 `assert(PROC_STOPPED)`）——否则它可能立即在新调用上再次自我暂停，语义错乱。`UNPAUSED` 设置后即被消费，绝不跨调用存活。

---

## 3 Rust 设计决策

Rust 改写遵循"语义重写（Rewrite）而非翻译（translate）"：保留 C 的外部行为与不变量，但用 Rust 的类型系统与架构模式重新表达。以下决策对应设计契约 `.design/05-design.v1.md` 的 D1–D8。

### D1：请求枚举只覆盖 11 种运行时请求

`VfsCall` 枚举（minix-types）只建模 11 种非 INIT 请求。`VFS_PM_INIT`（0x900）的启动握手是一次性、与运行时请求生命周期不同的协议，保持 01-pm-init-main.md 已落地的 `VfsPmInit` 不变。理由：最小爆炸半径，不破坏 01 已通过的测试。代价是常量表与枚举不完全同形——用注释与本文 §2.1 说明边界。

### D2：回复枚举带类型化载荷 + `decode`

`VfsReply` 枚举为 11 种回复各绑定类型化载荷（如 `Exec { status, pc, newsp, newps_str }`、`Core { status }`），并提供 `decode(&Message) -> Result<VfsReply, VfsReplyError>`。C 用 `switch(m_in.m_type)` 后裸读 `m7_i2`/`m7_p1`，Rust 在解码时一次性校验 `m_type` 与族归属，载荷与其类型绑定，**编译期排除"读了错误字段"**（FIDL 式思想，见 §1.3）。

### D3：`tell_vfs` 的三段式与 C 同序

`tell_vfs(table, slot, call, transport)`（minix-pm）：① not-idle 检查（进程 `ipc_blocked.is_some()`）→ ② 编码并 `send` → ③ 成功后置 `IpcBlockReason::VfsCall { reply_to_new_parent: false }`。顺序与 C 一致（先检查、再发送、再置位）。检查失败返回 `VfsCallError::NotIdle`——保留 C 的 "not idle" panic 语义，由调用层决定 fail-fast。

### D4：状态机效果经端口 trait 施加，双实现

06/09/13/16/17 尚未落地，若不抽象，协议语义无法单测。`trait VfsReplyServices` 把状态机的"效果"（reply、sched_start_user、exit_proc、publish_event、exec_restart、restart_signals、sys_abort、reply_to_guardian 等）定义为端口方法，两个行为不同的实现：

- `PmServices`——生产实现，未落地的方法用 `unimplemented!("DEFERRED: 见 XX-*.md")` 自说明，与既有 `KernelIpcTransport` 惯例一致。
- `RecordingServices`——测试实现，录制调用序列供断言。

这让 05 的协议语义（清标志、抽取 NEW_PARENT、11 路分支、尾部条件）在依赖未落地时即可 100% 断言。

### D5：隐式标志位延续 → 类型化延续 + 3 变体 `VfsReplyError`

C 用 `mp_flags` 加 `return` 位置表达"接下来做什么"，读者必须跨 4 个文件才能还原。Rust 把延续显式化为端口方法调用（sched_start_user、publish_event、exec_restart、restart_signals、sys_abort 等）。

错误模型有两层：

- **可经 `Result` 传播的三类**归入 `VfsReplyError { NotAReply(i32), UnknownReply(i32), BadEndpoint(Endpoint) }`：`NotAReply`/`UnknownReply` 来自解码（族校验/未知变体），`BadEndpoint` 来自 `slot_of_endpoint` 解析失败。三者最终在 `run_once` 统一 `panic!` 收口，文本与 C 四句 panic 等价。
- **两类 fail-fast panic**："reply without request"（端口方法 `take_vfs_call` 内）与 "UNPAUSED set on entry"（状态机入口）——C 对这两处本就是直接 `panic`，Rust 保留，不因过度类型化而把致命不变量弱化为可恢复错误。

### D6：对比 Redox / seL4 / Fuchsia 的结论

见 §1.3。核心是：Redox 的同步 `Scheme` 因 PM/VFS 环形依赖而不可采；seL4 的显式 continuation 与 FIDL 的类型化消息是可采的思想；Rust 改写在单线程 no_std 事件循环下把延续**类型化**（端口方法）而非用隐式标志位，取二者之长。

### D7：NEW_PARENT / UNPAUSED 辅助

`take_vfs_call` 端口方法合并"清除 VFS_CALL + 取出 reply_to_new_parent"，对应 C main.c:327-328；`set_unpaused` 端口方法在 UNPAUSE 分支设置 `UNPAUSED`，对应 main.c:410。辅助函数放在 `ipc/vfs.rs`，**不修改 02 档的 `block.rs`**（blast radius 控制）。`mark_new_parent`（设置 NEW_PARENT，对应 forkexit.c:402-403）的**调用点**属于 09 档 `exit_proc` 收养路径；05 只建模读取/清除侧（通过 `take_vfs_call`），设置侧在 09 落地时补。

### D8：`run_once` 入口拦截，而非 `DispatchResult`

设计稿曾规划 `DispatchResult { Call(ReplyIntent), VfsReply(Result<(),VfsReplyError>) }` 包装，由 `dispatch_message` 返回。落地时改为**在 `run_once` 主循环第一路直接拦截** `is_vfs_pm_rs(msg.m_type) && msg.m_source == Endpoint::VFS` 并调用 `handle_vfs_reply`，与 C `main.c:84-87` 的结构位置完全一致（`handle_vfs_reply` 是 `while` 循环体的第一分支，位于三路 `switch` 之前）。这比 `DispatchResult` 包装更贴近 C，且避免 `dispatcher` 出现"永不命中"的死分支。04 档的 `ReplyIntent` 契约（PM 调用路径）保持不动，VFS 路径语义不同（不向来源回复、可能回复多个目标），在主循环入口单独处理更清晰。

---

## 4 实现详解

### 4.1 协议层（minix-types/src/ipc/vfs.rs）

- 常量 `VFS_PM_RQ_BASE=0x900` / `VFS_PM_RS_BASE=0x980`，以及 11 个 `VFS_PM_*` 请求常量与 11 个 `VFS_PM_*_REPLY` 回复常量，**数值严格锁定为 §2.1 表格**（由 `test_vfs_pm_constants_match_com_h` 守卫，防止偏移错位）。
- `is_vfs_pm_rq` / `is_vfs_pm_rs`：`(m_type & !0x7f) == BASE`，与 com.h:516-517 同形。
- `VfsCall`：11 变体，`m_type()` 映射请求类型，`encode()` 把各变体字段写入 `Message`（FORK 的 `m7_i4`/`m7_i5` 置 -1，对应 C "Not used by VFS_PM_FORK"），`m_source = Endpoint::NONE`。
- `VfsReply`：11 变体，`decode()` 按 `m_type` 反查变体并绑定载荷（`VFS_PM_ENDPT` 即 `m7_i1` 在状态机层解析）。

### 4.2 状态机层（minix-pm/src/ipc/vfs.rs）

`handle_vfs_reply<S: VfsReplyServices>(svc, msg)` 严格镜像 C 四段：

1. REBOOT 特例：`m_type == VFS_PM_REBOOT_REPLY` → `svc.sys_abort(abort_flag)` 后返回。
2. `VfsReply::decode(msg)`。
3. 解析 endpoint：`svc.slot_of_endpoint(m7i1)`，`None` → `Err(BadEndpoint)`；`take_vfs_call` 取 `new_parent` 并清 `VFS_CALL`（"reply without request" 在此 panic）；入口 `UNPAUSED` 检查 panic。
4. 11 路 `match` + 尾部条件：`if svc.is_in_use(slot) && !svc.is_exiting(slot) { svc.restart_signals(slot) }`。

`PmServices::restart_signals` 在 13-signal-flow 落地前为 no-op 脚手架（当前无 pending 信号可重投，语义正确，使协议可端到端验证）。

### 4.3 接线层（init.rs / dispatcher.rs）

- `init.rs::run_once`：在 `dispatch_message` **之前**拦截 VFS 回复（D8），构造 `PmServices` 调用 `handle_vfs_reply`；`Ok` → `RunStep::Handled`（不回复 VFS），`Err` → `panic!` 与 C 文本等价。`PmServer` 新增 `abort_flag: i32` 字段（映射 C 全局 `abort_flag`，见 A-3）。
- `dispatcher.rs`：删除旧 `send_vfs_request` 占位（双 API 漂移来源），仅保留事件回复与 PM 调用两路，无死分支。
- `fork.rs`：`handle_fork` 改用 `tell_vfs` 发送 `VfsCall::Fork`，不再使用旧 `VfsRequest` 占位。

### 4.4 不变量表

| # | 不变量 | C 锚点 | Rust 表达 |
|---|---|---|---|
| 1 | 回复 endpoint 有效 | main.c:317-319 | `slot_of_endpoint` → `Err(BadEndpoint)` → run_once panic |
| 2 | 进程确有 VFS_CALL | main.c:324-325 | `take_vfs_call` panic "reply without request" |
| 3 | 进入时未置 UNPAUSED | main.c:330-331 | 状态机入口 panic "UNPAUSED set on entry" |
| 4 | VFS_CALL 在 tell_vfs 后置、handle_vfs_reply 清 | utility.c:138 / main.c:328 | `IpcBlockReason::VfsCall` 置/清 |
| 5 | NEW_PARENT 跨调用、本次清 | main.c:327-328 | `take_vfs_call` 取+清 |
| 6 | 未知回复码 | main.c:417-418 | `decode` → `Err(UnknownReply)` → run_once panic |
| 7 | 尾部 restart_sigs 仅当存活未退出 | main.c:422 | `is_in_use && !is_exiting` 守卫 |
| 8 | 一个进程同时最多一个 VFS 请求 | utility.c:131-132 | not-idle 检查 → `Err(NotIdle)` |

---

## 5 测试矩阵

### 5.1 minix-types（协议编解码）

- `test_vfs_pm_constants_match_com_h`：锁定所有 `VFS_PM_*` 数值等于 §2.1 表格（P0 回归守卫）。
- `test_vfs_call_roundtrip_all`：11 种 `VfsCall` 编码→解码往返一致。
- `test_vfs_call_fork_sentinels`：FORK 的 `m7_i4`/`m7_i5` 编码为 -1。
- `test_vfs_call_exec_layout`：EXEC 字段布局（path/len/frame/framelen/ps_str）正确落到 mess_7。
- `test_vfs_call_reboot_has_no_endpoint`：REBOOT 不携带 `VFS_PM_ENDPT`。
- `test_vfs_reply_decode_all`：11 种回复逐一 `decode` 正确。
- `test_vfs_reply_decode_exec_payload` / `test_vfs_reply_decode_core_status`：EXEC/CORE 载荷绑定正确。
- `test_vfs_reply_decode_not_a_reply`：非 RS 族 → `NotAReply`。
- `test_vfs_pm_family_predicates`：`is_vfs_pm_rq`/`is_vfs_pm_rs` 边界正确（含 `0x980+0x80` 不命中）。
- VFS_PM_INIT 系列（01 档）：init 常量、编码/解码往返、NONE 终结符、越界/错误类型。

共 **18** 项协议测试（含 §2.1 守卫）。

### 5.2 minix-pm（状态机 + 集成）

- 11 路回复：SETUID/SETGID/SETGROUPS → `reply(OK)`；SETSID → `reply(procgrp)`；EXEC → `exec_restart`；CORE → 设 `WCOREFLAG` 后 fallthrough EXIT；EXIT → `publish_event` 且**不**尾部；FORK → 调度成功/失败双分支（含 `new_parent` 抑制父回复）；SRV_FORK → 无操作；UNPAUSE → 置 `UNPAUSED` + `publish_event` 且**不**尾部。
- REBOOT 特例：`sys_abort` 调用且不进入 endpoint 解析。
- 不变式：`BadEndpoint`（坏 endpoint → `Err`）；入口 `UNPAUSED` panic；`take_vfs_call` "reply without request" panic。
- NEW_PARENT：置位后 FORK 回复抑制父进程回复、仍回复子进程。
- `run_once` 集成：`test_run_once_vfs_reply_no_sync_reply` 验证 VFS 回复被状态机收口、向被回复进程发 `OK`、清除 `VFS_CALL`、不向 VFS 同步回复。
- `fork.rs`：`handle_fork` 经 `tell_vfs` 发送 `VfsCall::Fork`，子进程置 `VFS_CALL`。

共 **~25** 项状态机/集成测试。

---

## 6 过渡

- 04-ipc-dispatch.md 的 VFS 回复"钩子"已落地为真实状态机：dispatcher 不再含 VFS 分支，`run_once` 第一路直接拦截（D8）。
- 06（事件订阅）、09（exit 收养）、13（信号重投）、16、17（exec 重启）未落地时，`VfsReplyServices` 的对应方法在 `PmServices` 中 `unimplemented!("DEFERRED: 见 XX-*.md")` 自说明，使 05 协议可独立测试与验证。
- 07-pm-fork 协调器已把 fork 的 VFS 发送切到 `tell_vfs`，消除旧 `VfsRequest`/`send_vfs_request` 双 API。
- 下一步：09 落地 `mark_new_parent` 设置侧；13 落地 `restart_signals` 真实实现替换 no-op 脚手架；REBOOT 写入侧（`do_reboot`）归 20 档。

---

## 7 参见

- C 源（ground truth）：`minix3/minix/servers/pm/main.c:295-424`（handle_vfs_reply）、`minix3/minix/servers/pm/utility.c:123-139`（tell_vfs）、`minix3/minix/include/minix/com.h:513-583`（协议面）、`minix3/minix/servers/pm/{forkexit.c,getset.c,exec.c,signal.c,misc.c}`（7 个调用点）。
- 设计契约：`.design/05-design.v1.md`（D1–D8 与行为契约表）、`.design/05-outline.v1.md`、`.design/05-outline-review.v1.md`。
- PM 阶段文档：01-pm-init-main.md（VFS_PM_INIT）、02-mproc-struct.md（BlockState/NEW_PARENT/UNPAUSED）、03-mproc-table.md（pm_isokendpt）、04-ipc-dispatch.md（主循环三路分发）。
- 对端实现：05-stage-vfs（VFS 侧接收与回复）。
- 内核接口：01-stage-kernel（sys_abort / sched_start_user）。
- OS 模式参考：Redox `Scheme`、seL4 reply-object、Fuchsia FIDL（见 §1.3）。
