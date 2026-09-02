# 06 — 进程事件的发布与订阅

本文讲清 PM 的进程事件（process event）发布/订阅设施全貌：为什么需要它、为什么选择串行化投递、订阅表如何在四个槽位内以标志位延续跟踪每个进程的事件进度、以及 Rust 改写如何把 C 的裸数组 + 隐式游标提升为类型化的注册表与游标。

前置阅读：04-ipc-dispatch.md（主循环第二路 `PROC_EVENT_REPLY` 与 `ReplyIntent::ReplyLater` 契约）、05-vfs-interaction.md（`handle_vfs_reply` 的两处 `publish_event` 调用点与提前 return）、02-mproc-struct.md（`BlockState::EventCall` / `mp_eventsub` 建模）、03-mproc-table.md（`pm_isokendpt` / `UserSlot`）。

---

## 1 概念

### 1.0 目标读者与边界

**目标读者**：已理解 PM 主循环三路分发与 VFS 异步回复延续（04/05）的开发者；知道 PM 是用户态单线程事件循环（一次只处理一条消息，无锁）。

> **本章不讲什么**：
> - `exit_restart` 的完整退出状态机（09-pm-exit.md）
> - `restart_sigs` / `check_pending` / `try_resume` 的信号恢复（13-signal-flow.md）
> - SysV IPC server（DS，07-stage-ds）收到 `PROC_EVENT` 后的具体行为
> - 内核 `asynsend3` / `ipc_senda` 的队列实现（01-stage-kernel）
>
> 本章只回答一个问题：**PM 如何在不引入无界队列的前提下，让外部服务可靠地获知"某个进程刚刚被信号打断 / 某个进程正在退出"**。

### 1.1 为什么需要进程事件

微内核把进程管理的"权威状态"集中在 PM（进程表、信号、生命周期），但**阻塞语义**散落在其他服务。最典型的例子是 System V IPC：`semop(2)` 在内核之外由 IPC server 实现，当一个进程在 `semop` 上睡眠时，若该进程收到信号或直接退出，IPC server 必须立即打断睡眠并清理该进程绑定的 IPC 资源。PM 知道"进程被信号打断 / 正在退出"，IPC server 知道"哪些进程在哪个 IPC 对象上睡眠"——两者必须会合（`event.c:1-9`）：

> *A subscribing service would typically use such events to interrupt a blocking system call and/or clean up process-bound resources. As of writing, the only service that uses this facility is the System V IPC server.*

PM 不能代 IPC server 清理，也不能让 IPC server 轮询 `mproc`（微内核服务间无共享内存）。因此需要一个**事件通知**：PM 在"可观测的生命周期转折点"把事件主动推出去，订阅者收到后决定是唤醒调用者还是释放资源。

目前只有两类事件（`syslib.h:292-293`）：`PROC_EVENT_EXIT (0x01)`——进程正在退出（`EXITING` 置位后）；`PROC_EVENT_SIGNAL (0x02)`——进程刚刚被 `VFS_PM_UNPAUSE` 解暂停（`UNPAUSED` 置位后）。两者都发生在 VFS 确认之后（`handle_vfs_reply` 之后），因此事件的"发布时机"天然串在 VFS 异步回复之后——读者可在 `main.c:365` 与 `main.c:413` 看到 `publish_event` 的两处调用点紧跟 `VFS_PM_EXIT_REPLY` / `VFS_PM_UNPAUSE_REPLY`，且**提前 return 不走尾部** `restart_sigs`（这条互斥关系是 05 与本章的衔接约束）。

### 1.2 为什么是发布/订阅，而不是轮询或直接回调

PM 与订阅者是**生命周期独立**的两个用户态 server：PM 可能早于 IPC server 启动，也可能在 IPC server 重启时继续服务其他进程。轮询要求订阅者持有 PM 表的读权限并周期性扫描——这违背微内核最小知识原则（`minix-types/src/lib.rs` 的"Why Not Include MProc"）。直接回调要求 PM 在编译期知道订阅者的函数地址——订阅者可能是可选的、动态的。

发布/订阅把双方解耦为三个独立维度：

- **时间解耦**——发布者（PM）不等待订阅者立即处理完，订阅者也不必在事件发生时恰好在线（`publish_event` 时若无订阅者，事件直接由 `resume_event` 推进到 `exit_restart`/`restart_sigs`）；
- **空间解耦**——PM 不需知道订阅者的内部数据结构，只需知道"谁关心哪类事件"（`subs[i].mask & event`）；
- **生命周期解耦**——订阅者死亡时 PM 自动清理其表项（`publish_event` 的首段扫描，`event.c:330-343`），不留悬挂等待。

代价是 PM 必须为"等待中事件"保存**延续**（continuation）：事件已发布但订阅者尚未回复，目标进程因此不能继续生命周期推进，必须挂在 `EVENT_CALL` 上，直到所有订阅者逐个回复。这正是串行化设计的起点。

### 1.3 为什么是串行，而不是并行或异步

`event.c:16-25` 的头注释是理解设计权衡的一手证据，值得逐句精读：

> *Thus, each subscriber adds a serialized messaging roundtrip for each subscribed event.*
> *The one and only reason for this synchronous, serialized approach is that it avoids PM queuing up too many asynchronous messages. ... the serial synchronous approach requires NR_PROCS asynsend slots. For a parallel synchronous approach, this would increase to (NR_PROCS*NR_SUBS). Worse yet, for an asynchronous event notification approach, the number of messages that PM can end up queuing is potentially unbounded.*

三类方案的队列代价（设 `NR_PROCS = 256`，`NR_SUBS = 4`）：

| 方案 | 每事件需要的 `asynsend` 槽位 | 上界 |
|------|------------------------------|------|
| **异步通知**（fire-and-forget） | 无需等待，但 PM 需排队所有未处理通知 | **无界**（每个活进程都可能同时有事件待处理） |
| **并行同步**（同时发给 4 个订阅者，等待全部回复） | `NR_PROCS * NR_SUBS = 1024` | 固定但 4 倍膨胀 |
| **串行同步**（一次只发给一个订阅者，等回复后再发下一个） | `NR_PROCS = 256` | 最小固定上界 |

内核为 PM 预留的异步槽是 `ASYN_NR = 2 * _NR_PROCS`（`asynsend.c:17`），串行方案恰好落在其中（256 < 512），并行方案超出（1024 > 512）。**槽位上界是硬约束**，不是偏好——PM 不能让内核队列溢出，否则会 `panic("asynsend failed")`（`event.c:104-106`）。因此串行不是"效率妥协"，而是"在内核队列约束下唯一可证明有界的方案"。

搭档约束是**单订阅者假设**（`event.c:24-25`）：*At this moment, we expect only one subscriber (the IPC server) which makes the serial vs parallel point less relevant.* 预留 4 个槽位是防御性上限，不是活跃值——Rust 侧仍保留 `NR_SUBS = 4`，但测试以单订阅者为主路径。

### 1.4 订阅模型的两个约束：不按进程过滤与掩码变更的脆弱性

订阅表是**按服务、按事件类型**的全局表（`subs[NR_SUBS]` 每项 `{ endpoint, mask, waiting }`，`event.c:60-64`），不是按进程的表。`event.c:27-32` 解释原因：

> *It is not possible to subscribe to events from certain processes only. If a service were to subscribe ... as part of a system call by a process (e.g., semop(2)), it may subscribe "too late" and already have missed a signal event for the process calling semop(2).*

订阅若与系统调用绑定，信号可能在订阅生效前就已到达目标进程——这是**时间竞争**，需"重大基础设施变更"才能解决。因此 PM 的订阅是粗粒度的"任何进程的 EXIT/SIGNAL"，订阅者收到事件后自行判断是否与自己持有的阻塞调用相关，无关则按常规回复即可（`event.c:280-287` 的"不检查 mask"注释即为此设计服务：退订后残留通知仍需正常回复）。

第二条约束关于掩码变更（`event.c:34-41`）：

> *A server may however change its event subscription mask at runtime... For the same race-condition reasons, new subscriptions must always be made when processing a message that is not a system call potentially affected by events. In the case of the IPC server, it may subscribe to events from semget(2) but not semop(2). For signal events, the delay call system guarantees the safety of this approach; for exit events, the message type prioritization does (which is not great; see the TODO item in forkexit.c).*

信号事件由 `delay_call` 保障（信号先挂起、等调用完成再投递，详见 13-signal-flow.md），退出事件由**消息类型优先级**保障（PM 主循环对 `PROC_EVENT_REPLY` 的处理优先于普通调用，具体 TODO 在 `forkexit.c`）。这两条保障都不在本章展开，但读者需知：掩码变更不是原子快照，PM 不保证"变更后立即只收到新掩码的事件"——订阅者必须容忍残留通知。

### 1.5 事件的生命周期：一次发布，逐个投递，直至恢复

一个事件从发布到恢复的完整链条（读者可对照 `event.c` 逐行跟随）：

```
publish_event(rmp)                     // event.c:316
  ├─ 若 rmp 是正在退出的特权服务 → 扫描 subs 找 endpoint == rmp.endpoint → remove_sub
  ├─ rmp.flags |= EVENT_CALL; rmp.mp_eventsub = 0
  └─ resume_event(rmp)                 // event.c:74
       ├─ 推断事件：EXITING → EXIT (0x01)，UNPAUSED → SIGNAL (0x02)，否则 panic
       ├─ for i = mp_eventsub .. nsubs-1, mp_eventsub++：
       │    if subs[i].mask & event  →  asynsend3(subs[i].endpt, {PROC_EVENT, endpt=rmp.endpoint, event})
       │                                subs[i].waiting++; return  // 挂起，等待回复
       ├─ 无更多匹配订阅者 → mp_flags &= ~EVENT_CALL; mp_eventsub = NO_EVENTSUB
       └─ exit → exit_restart(rmp)   // EXIT 分支
          signal → restart_sigs(rmp) // SIGNAL 分支
                                    // 两条终止都由 09/13 实现，本章只到"调用点"

do_proc_event_reply(msg)               // event.c:218  订阅者回复 PROC_EVENT_REPLY
  ├─ 7 步校验（见 §2.5，任一失败 → printf + SUSPEND）
  ├─ subs[i].waiting-- 
  ├─ if mask==0 && waiting==0 → remove_sub(i)   // 退订且无残留等待 → 清理
  │  else  rmp.mp_eventsub++; resume_event(rmp) // 推进到下一个订阅者
  └─ return SUSPEND  // 永远不回复本回复消息（main.c:88-89 的 result = SUSPEND）

remove_sub(slot)                       // event.c:130
  ├─ subs 有序前移，nsubs--
  └─ 遍历全表 mproc[NR_PROCS]：对每个 IN_USE|EVENT_CALL 且 mp_eventsub != NO_EVENTSUB 的进程
       ├─ mp_eventsub == slot → nested++ → resume_event → nested--  // 被删订阅者正被等待 → 立即推进
       └─ mp_eventsub > slot  → mp_eventsub--                        // 索引回退，保持有序
```

要点：`EVENT_CALL` 是"进程正在等待事件订阅者"的**延续标志**，`mp_eventsub` 是**游标**（cursor），指向下一个要尝试的 `subs` 下标。两者必须同生同灭（`publish_event` 同时置位，`resume_event` 同时清除），C 用两个独立字段表达，Rust 侧将其收敛为一个携带游标的阻塞态（见 §3.3）。

`nested` 是**重入守卫**（`event.c:67`）：`remove_sub` 在遍历进程表时可能调用 `resume_event`，而 `resume_event` 又可能因 `exit_restart` 间接触发新的 `publish_event`？C 注释 `event.c:148-153` 称此 nesting-safe 因"event calls 恒在 VFS calls 之后"，但仍用 `nested` 计数并在 `publish_event` / `do_proc_event_reply` 入口 `assert(nested==0)`（`event.c:226/321`），Rust 侧保留同等断言。

### 1.6 与其他操作系统最佳实践的对比

Rust 改写不是照抄 C 的裸数组写法，而是在吸收工业级 OS 的成熟模式后做取舍。

**Linux 的 `signalfd` / `eventfd` + `epoll`。** Linux 把信号投递解耦为文件描述符上的可读事件：进程用 `signalfd` 把信号转成 fd，用 `epoll_wait` 统一轮询。这种模型是**轮询式**（pollable）、**按进程 fd** 的，订阅者（事件循环）一次 `epoll_wait` 可收割多个 fd 的就绪通知。PM 不能采用此模型——PM 的订阅者是**服务进程**，不是 fd 持有者，且 PM 的进程表是权威、订阅者不可直接读。Linux 之所以能轮询，是因为事件源（内核）与消费者（用户态）在同一地址空间通过 fd 抽象会合；而 PM 的订阅者在独立地址空间，只能靠 IPC 消息会合。

**Redox 的 `Scheme` 句柄表。** Redox 的 `Scheme::handle(packet)` 把调用者的线程挂在内核的 handle 表上，scheme 处理完包即回，内核解挂线程。这是**内核托管延续**：continuation 挂在内核的 handle 表，unit 是 *scheme handle*。PM 不能照搬——PM 的延续必须挂在*进程*（`mproc`）而不是内核 handle，因为 PM 的事件本质是"进程状态迁移"（退出 / 被信号打断），天然以进程为单位。内核托管会让"哪个进程被事件阻塞"变得间接。Rust 侧保留"以进程为单位"的语义，但把 C 的隐式标志位延续提升为显式游标。

**seL4 的 notification / reply-object。** seL4 把"回复对象"作为一等 capability：handler 可保存 reply capability 并在任意时刻用它向原调用者回复，continuation 因此显式化。PM 的"异步请求 + 串行逐个回复"在语义上接近 seL4 的 reply-object——`publish_event` 投出后，`do_proc_event_reply` 的每一次回复都是对"前一次 `PROC_EVENT`"的 reply-object 行使。但 Minix3 没有显式 capability，而是把延续隐式挂在进程标志位（`EVENT_CALL` + `mp_eventsub`），且一个进程同时最多只有一个未完成事件（否则 `tell_vfs` 已 panic "not idle"，间接保证事件与 VFS 互斥）。seL4 的显式性更强，但 PM 的隐式性因"每进程单事件"而不产生歧义。

**Fuchsia 的 FIDL 事件流。** FIDL 为异步 IPC 生成类型安全的消息编解码与事件流，编译期排除"读了错误字段"。Minix3 的 C 实现恰相反：`PROC_EVENT` 的载荷是裸 `m_pm_lsys_proc_event.endpt/event` 两个字段，`do_proc_event_reply` 再裸读 `m_in.m_pm_lsys_proc_event.event` 做一致性检查。Rust 改写应吸收 FIDL 的核心思想——**类型化的事件与掩码**（`ProcEvent` 枚举 + `ProcEventMask` bitflags，`contains_event` 在类型层表达 `mask & event`）。

**结论（本章的设计基线）。** 把 C 的"全局数组 + 隐式游标 + 裸掩码"改写为"类型化掩码（`ProcEventMask`）+ 显式游标（`EventCursor`）+ 注册表（`EventRegistry`）"。延续从散落在 `mp_flags` 与 `mp_eventsub` 的隐式控制流，提升为注册表显式推进的游标（`cursor.0`），与 seL4 的显式 continuation、FIDL 的类型化掩码同构，又因 PM 单线程无共享而避免引入堆与锁。

### 1.7 小结

1. **为什么需要事件**——System V IPC 的阻塞调用需被信号/退出打断，PM 是唯一权威发布者。
2. **为什么串行**——内核异步槽位 `ASYN_NR = 2*NR_PROCS` 只容得下 `NR_PROCS` 的串行方案，并行需 4 倍、异步无界；串行是约束驱动的选择。
3. **订阅模型**——按服务、按掩码，不按进程；掩码变更需避开受事件影响的调用路径（delay_call / 消息优先级保障）。
4. **生命周期**——`publish_event` 置 `EVENT_CALL` + 游标 0 → `resume_event` 逐个 `asynsend3` → 订阅者 `PROC_EVENT_REPLY` → `do_proc_event_reply` 校验后游标++再 `resume_event` → 无订阅者则清标志并 `exit_restart`/`restart_sigs`。
5. **延续载体**——`EVENT_CALL` + `mp_eventsub` 游标是 per-process continuation，`remove_sub` 的有序删除与 `nested` 守卫保证游标在表变更后不偏。

下一章逐行分析 C 源码；第 3 章给出 Rust 的类型系统重表达。

---

## 2 C 源码分析

### 2.1 数据面：订阅表与 per-process 游标（event.c:58-67 / mproc.h:27 / const.h:13 / syslib.h:292-293 / com.h:597-619）

`event.c:58-67`（文件级 `static`）：

```c
#define NR_SUBS  4

static struct {
    endpoint_t endpt;      // 订阅者 endpoint
    unsigned int mask;     // 关心事件的位掩码（PROC_EVENT_EXIT|SIGNAL）
    unsigned int waiting;  // 多少进程正阻塞在等它的回复
} subs[NR_SUBS];

static unsigned int nsubs = 0;   // 已用槽位数（0..NR_SUBS）
static unsigned int nested = 0;  // 重入守卫计数
```

`NR_SUBS = 4` 是**现实上限**（`event.c:52-57` 注释："does not scale to numbers larger than this"），当前仅 IPC server 一个订阅者。`waiting` 计数让退订（`mask==0`）可延迟到"无残留等待"时再真正删除，否则残留通知的回复会找不到表项。

Per-process 游标（`mproc.h:27` / `const.h:13`）：

```c
char mp_eventsub;               // in mproc, 订阅者下标 0..nsubs-1 或 -1
#define NO_EVENTSUB  ((char)-1)  // 无游标哨兵
#define PROC_EVENT_EXIT   0x01    // syslib.h:292
#define PROC_EVENT_SIGNAL 0x02    // syslib.h:293
```

`mp_eventsub` 为 `char`（有符号 8 位），`-1` 即 `0xFF`，与 `0..3` 的有效下标不冲突。`PROC_EVENT_EXIT/SIGNAL` 为**位掩码**（`syslib.h:289-291` 注释"they form a bit mask"），`subs[i].mask & event` 即订阅关系。

消息类型与载荷（`com.h:597-619` / `ipc.h:1812/2566/2610`）：

```c
#define COMMON_RQ_BASE  0xE00
#define COMMON_RS_BASE  0xE80
#define PROC_EVENT        (COMMON_RQ_BASE+3)   // 0xE03，PM → 订阅者
#define PROC_EVENT_REPLY  (COMMON_RS_BASE+0)   // 0xE80，订阅者 → PM

// ipc.h:1812 — PM → 订阅者的事件消息
typedef struct {
    endpoint_t endpt;      // 目标进程 endpoint
    unsigned int event;    // PROC_EVENT_EXIT or SIGNAL
} mess_pm_lsys_proc_event; // m_pm_lsys_proc_event
// ipc.h:2566 — 订阅者 → PM 的掩码更新
typedef struct {
    unsigned int mask;     // 订阅掩码
} mess_lsys_pm_proceventmask; // m_lsys_pm_proceventmask
```

初始化（`main.c:149-151`）：

```c
for (rmp = &mproc[0]; rmp < &mproc[NR_PROCS]; rmp++) {
    rmp->mp_eventsub = NO_EVENTSUB;
}
```

`forkexit.c:116` 与 `216` 的 `assert(rmc->mp_eventsub == NO_EVENTSUB)` 保证新进程无悬挂游标——这是 `publish_event` 前置断言的对应物。

### 2.2 `resume_event`：串行推进的核心（event.c:74-123）

```c
static void resume_event(struct mproc *rmp) {
    message m;
    unsigned int i, event;

    assert(rmp->mp_flags & IN_USE);
    assert(rmp->mp_flags & EVENT_CALL);
    assert(rmp->mp_eventsub != NO_EVENTSUB);

    if (rmp->mp_flags & EXITING)        // 退出中 → EXIT
        event = PROC_EVENT_EXIT;
    else if (rmp->mp_flags & UNPAUSED)  // 被 VFS 解暂停 → SIGNAL
        event = PROC_EVENT_SIGNAL;
    else
        panic("unknown event for flags %x", rmp->mp_flags);

    for (i = rmp->mp_eventsub; i < nsubs; i++, rmp->mp_eventsub++) {
        if (subs[i].mask & event) {
            memset(&m, 0, sizeof(m));
            m.m_type = PROC_EVENT;
            m.m_pm_lsys_proc_event.endpt = rmp->mp_endpoint;
            m.m_pm_lsys_proc_event.event = event;

            r = asynsend3(subs[i].endpt, &m, AMF_NOREPLY);
            if (r != OK) panic("asynsend failed: %d", r);

            assert(subs[i].waiting < NR_PROCS);
            subs[i].waiting++;
            return; // 挂起，等待该订阅者的 PROC_EVENT_REPLY
        }
    }

    // 无更多匹配订阅者 → 恢复事件本身
    rmp->mp_flags &= ~EVENT_CALL;
    rmp->mp_eventsub = NO_EVENTSUB;

    if (event == PROC_EVENT_EXIT)
        exit_restart(rmp);   // 09-pm-exit.md
    else if (event == PROC_EVENT_SIGNAL)
        restart_sigs(rmp);   // 13-signal-flow.md
}
```

三段式，顺序不可调换：

1. **事件推断**（`event.c:86-91`）：依据 `mp_flags` 的互斥位（`EXITING` 与 `UNPAUSED` 在此上下文互斥——`handle_vfs_reply` 的两条 `publish_event` 路径分别对应两者，且 `handle_vfs_reply` 入口断言 `!(flags & UNPAUSED)` 保此互斥）推断事件类型；否则 `panic("unknown event for flags")`——这是**不可恢复损坏**，与 `do_proc_event_reply` 的 `printf+SUSPEND` 可恢复路径区分。
2. **串行扫描**（`event.c:97-113`）：`i` 从 `mp_eventsub` 起，遇掩码匹配即 `asynsend3` 投递并 `waiting++` 后**立即 return**——串行点；不匹配则 `mp_eventsub++` 跳过该订阅者（即使 `mask & event == 0` 也递增游标，下一次仍从新游标继续）。
3. **终止分派**（`event.c:115-122`）：清除 `EVENT_CALL/NO_EVENTSUB` 后按事件类型分派；两者分别是 09 与 13 的入口，本章只到调用点。

### 2.3 `remove_sub`：有序删除与游标回退（event.c:130-161）

```c
static void remove_sub(unsigned int slot) {
    struct mproc *rmp;
    unsigned int i;

    for (i = slot; i < nsubs - 1; i++)
        subs[i] = subs[i + 1];
    nsubs--;

    for (rmp = &mproc[0]; rmp < &mproc[NR_PROCS]; rmp++) {
        if ((rmp->mp_flags & (IN_USE | EVENT_CALL)) != (IN_USE | EVENT_CALL))
            continue;
        assert(rmp->mp_eventsub != NO_EVENTSUB);

        if ((unsigned int)rmp->mp_eventsub == slot) {
            nested++;
            resume_event(rmp); // 被删订阅者正被该进程等待 → 立即推进到下一个
            nested--;
        } else if ((unsigned int)rmp->mp_eventsub > slot)
            rmp->mp_eventsub--;
    }
}
```

关键点：

- **有序前移**（`event.c:136-139`）：`subs` 是**紧凑前缀**（`0..nsubs-1` 有值），删除后前移保持有序；`nsubs--` 后旧尾槽位值无关紧要（下次 push 覆盖）。
- **受影响进程的两种调整**（`event.c:142-160`）：遍历全表 `mproc[NR_PROCS]`，仅对 `IN_USE|EVENT_CALL` 且游标有效者：
  - `mp_eventsub == slot`——该进程正阻塞在等待被删订阅者的回复，删除后必须**立即推进**（`resume_event`），否则该进程将永远等待一个不存在的订阅者；`nested++`/`--` 在 `resume_event` 调用前后是重入标记（见 `event.c:148-153` 注释"event calls always take place after VFS calls, making this nesting-safe"——正常不会递归，但仍守卫）。
  - `mp_eventsub > slot`——游标指向被删位置之后的订阅者，数组前移后下标应**回退 1**，否则下一次会跳过一个订阅者（模式 76 的"跨文档游标漂移"同源问题在此处微观重演）。

### 2.4 `do_proceventmask`：订阅/退订/更新（event.c:170-211）

```c
int do_proceventmask(void) {
    unsigned int i, mask;

    if (!(mp->mp_flags & PRIV_PROC)) return EPERM;

    mask = m_in.m_lsys_pm_proceventmask.mask;

    for (i = 0; i < nsubs; i++) {
        if (subs[i].endpt == who_e) {
            if (mask == 0 && subs[i].waiting == 0)
                remove_sub(i);
            else
                subs[i].mask = mask;
            return OK;
        }
    }

    if (mask == 0) return OK; // 空掩码对未订阅者是 no-op

    if (nsubs == __arraycount(subs)) {
        printf("PM: too many process event subscribers!\n");
        return ENOMEM;
    }

    subs[nsubs].endpt = who_e;
    subs[nsubs].mask = mask;
    nsubs++;

    return OK;
}
```

语义表：

| 场景 | 条件 | 动作 | 返回 |
|------|------|------|------|
| 已订阅，退订且无等待 | `mask==0 && waiting==0` | `remove_sub` | OK |
| 已订阅，退订但有等待 | `mask==0 && waiting>0` | `mask=0`（延迟删除，等待中残留通知仍可回复） | OK |
| 已订阅，更新掩码 | `mask!=0` | `mask=mask` | OK |
| 未订阅，空掩码 | `mask==0` | no-op | OK |
| 未订阅，非空，已满 | `nsubs==NR_SUBS` | — | ENOMEM |
| 未订阅，非空，未满 | `mask!=0` | push | OK |

`!PRIV_PROC → EPERM`（`errno.h:1`）是唯一权限分支——只有系统服务可订阅（`const.h:13` 的 `PRIV_PROC` 语义）；`ENOMEM`（`errno.h:12`）是唯一容量分支。两者经 `main.c:106` 的 `reply(who_p, result)` 回复调用者（VFS 事件不经此路径）。

### 2.5 `do_proc_event_reply`：订阅者的回复（event.c:218-309）

这是最长的校验函数，**全部校验失败都 `printf` + `return SUSPEND`**（不回复本回复消息，`main.c:88-89` 的 `result = SUSPEND` 不向订阅者回复），仅在 `!PRIV_PROC` 时 `return ENOSYS`（向误调用的普通进程回复 ENOSYS）：

```c
int do_proc_event_reply(void) {
    struct mproc *rmp;
    endpoint_t endpt;
    unsigned int i, event;
    int slot;

    assert(nested == 0);

    if (!(mp->mp_flags & PRIV_PROC)) return ENOSYS;

    endpt = m_in.m_pm_lsys_proc_event.endpt;
    if (pm_isokendpt(endpt, &slot) != OK) {
        printf("PM: proc event reply from %d for invalid endpt %d\n", who_e, endpt);
        return SUSPEND;
    }
    rmp = &mproc[slot];
    if (!(rmp->mp_flags & EVENT_CALL)) {
        printf("PM: proc event reply from %d for endpt %d, no event\n", who_e, endpt);
        return SUSPEND;
    }
    if (rmp->mp_eventsub == NO_EVENTSUB || (unsigned int)rmp->mp_eventsub >= nsubs) {
        printf("PM: proc event reply from %d for endpt %d index %d\n", who_e, endpt, rmp->mp_eventsub);
        return SUSPEND;
    }
    i = rmp->mp_eventsub;
    if (subs[i].endpt != who_e) {
        printf("PM: proc event reply for %d from %d instead of %d\n", endpt, who_e, subs[i].endpt);
        return SUSPEND;
    }
    if (rmp->mp_flags & EXITING) event = PROC_EVENT_EXIT;
    else if (rmp->mp_flags & UNPAUSED) event = PROC_EVENT_SIGNAL;
    else {
        printf("PM: proc event reply from %d for %d, bad flags %x\n", who_e, endpt, rmp->mp_flags);
        return SUSPEND;
    }
    if (m_in.m_pm_lsys_proc_event.event != event) {
        printf("PM: proc event reply from %d for %d for event %d instead of %d\n",
               who_e, endpt, m_in.m_pm_lsys_proc_event.event, event);
        return SUSPEND;
    }
    // 不检查 event 与 subs[i].mask 的一致性（event.c:280-287 注释）

    assert(subs[i].waiting > 0);
    subs[i].waiting--;

    if (subs[i].mask == 0 && subs[i].waiting == 0)
        remove_sub(i);
    else {
        rmp->mp_eventsub++;
        resume_event(rmp);
    }

    return SUSPEND; // 任何路径都不回复本回复消息
}
```

7 步校验（按 C 顺序）：

1. `PRIV_PROC` 权限（→ ENOSYS）
2. `pm_isokendpt(endpt)` 存活（→ SUSPEND）
3. `rmp` 确有 `EVENT_CALL`
4. `mp_eventsub` 在 `[0, nsubs)` 内
5. `subs[i].endpoint == who_e`（回复者确是游标所指订阅者）
6. `rmp` 的标志可推断事件（EXITING/UNPAUSED else → SUSPEND）
7. `m_in.event == event`（订阅者声称的事件与 PM 推断一致）

**不检查第 8 项**——`event` 与 `subs[i].mask` 的一致性（`event.c:280-287`）：*Do NOT check the event against the subscriber's event mask, since a service may have unsubscribed ... leftover notifications*。退订后残留通知的回复不应被拒绝，否则订阅者会在退订与重订阅的竞态中死锁。

末段（`event.c:289-305`）：`waiting--` 后若 `mask==0 && waiting==0`（退订且无残留）→ `remove_sub`（该分支**不**调用 `resume_event`，因为 `remove_sub` 已对受影响进程做了推进）；否则 `mp_eventsub++` 后 `resume_event` 推进到下一个订阅者。任何路径恒返 `SUSPEND`。

### 2.6 `publish_event`：事件发布的入口（event.c:316-353）

```c
void publish_event(struct mproc *rmp) {
    unsigned int i;

    assert(nested == 0);
    assert((rmp->mp_flags & (IN_USE | EVENT_CALL)) == IN_USE);
    assert(rmp->mp_eventsub == NO_EVENTSUB);

    // 若退出的进程本身是订阅者，先清理其订阅项
    if ((rmp->mp_flags & (PRIV_PROC | EXITING)) == (PRIV_PROC | EXITING)) {
        for (i = 0; i < nsubs; i++) {
            if (subs[i].endpt == rmp->mp_endpoint) {
                remove_sub(i);
                break;
            }
        }
    }

    rmp->mp_flags |= EVENT_CALL;
    rmp->mp_eventsub = 0;

    resume_event(rmp);
}
```

三段式，顺序敏感：

1. **前置断言**（`event.c:321-323`）：进程必须 `IN_USE` 且**未**处于 `EVENT_CALL`，游标为 `NO_EVENTSUB`——保证"一进程同时最多一个事件"（与 `tell_vfs` 的 `VFS_CALL|EVENT_CALL` 互斥 `panic` 同源约束）。
2. **服务死亡清理**（`event.c:330-343`）：仅当 `PRIV_PROC|EXITING` 同时置位（正在退出的系统服务）时扫描订阅表找 `endpoint == rmp.endpoint` 的项并 `remove_sub`。注释*If the wait count is nonzero, we may or may not get additional replies ... Those will be ignored.*——退订后残留回复若再到达，会在 `do_proc_event_reply` 被 `SUSPEND` 忽略（合法旁路）。
3. **发布**（`event.c:349-352`）：置 `EVENT_CALL`、游标 0、立即 `resume_event`——若无订阅者，`resume_event` 将直接清标志并分派 `exit_restart`/`restart_sigs`，等价于"零订阅者时事件不阻塞"。

### 2.7 主循环与 VFS 回复的衔接（main.c:88-89 / 365 / 413 / 149-151 / forkexit.c:116/216）

主循环第二路（`main.c:88-89`）：

```c
} else if (call_nr == PROC_EVENT_REPLY) {
    result = do_proc_event_reply();
}
```

`result` 恒为 `SUSPEND`（或 `ENOSYS`），主循环 `if (result != SUSPEND) reply` 因此**不向订阅者回复**本次 `PROC_EVENT_REPLY` 消息——回复消息本身是 ACK，不需再 ACK。

VFS 回复的两处发布（`main.c:334-415` 的 `handle_vfs_reply`）：

```c
case VFS_PM_CORE_REPLY: // fallthrough
case VFS_PM_EXIT_REPLY:
    assert(rmp->mp_flags & EXITING);
    publish_event(rmp);
    return; // do not take default action（不走尾部 restart_sigs）
...
case VFS_PM_UNPAUSE_REPLY:
    assert(rmp->mp_flags & PROC_STOPPED);
    rmp->mp_flags |= UNPAUSED;
    publish_event(rmp);
    return; // 同上
```

`return` 的含义是**不执行尾部** `main.c:421-423` 的 `if ((flags & (IN_USE|EXITING))==IN_USE) restart_sigs(rmp)`——EXIT 的进程正在退出、UNPAUSE 的进程刚被唤醒由事件驱动，不应再 `restart_sigs`。`resume_event` 的终止分派会自行调用 `exit_restart`/`restart_sigs`，因此尾部是互斥的。

初始化与 `fork` 的不变量（`main.c:149-151` / `forkexit.c:116/216`）：

```c
rmp->mp_eventsub = NO_EVENTSUB;                 // 启动初始化
assert(rmc->mp_eventsub == NO_EVENTSUB);        // fork 子进程断言
```

新进程必须无悬挂游标，否则 `publish_event` 的前置断言失败。

### 2.8 对端视角与异步容量（com.h:597-619 / ipc.h:1812/2566/2610 / asynsend.c:17）

已在 §2.1 列出消息常量与载荷。补充容量实证（`asynsend.c:17`）：

```c
#define ASYN_NR  (2 * _NR_PROCS)  // 512 槽，PM 串行需 256，满足上界
```

`asynsend3(..., AMF_NOREPLY)` 的 `AMF_NOREPLY` 表示"不期待回复的异步发送"（与 VFS 的 `tell_vfs` 同源），PM 的事件投递与 VFS 请求投递共享同一异步表。

### 2.9 不变式分类

| 类别 | 检测方式 | 触发条件 | 严重度 |
|------|----------|----------|--------|
| `panic("unknown event for flags")` | `resume_event` 事件推断 else 分支 | `!(EXITING\|UNPAUSED)` | **不可恢复**（进程标志损坏） |
| `panic("asynsend failed")` | `resume_event` / `remove_sub` 的 `asynsend3` 失败 | 内核异步表溢出或 endpoint 非法 | **不可恢复**（上界被突破或订阅者已死但未清理） |
| `assert(IN_USE\|EVENT_CALL & !NO_EVENTSUB)` | `resume_event` / `publish_event` 入口 | 游标与标志不一致 | **不可恢复**（类型系统应消除） |
| `assert(nested==0)` | `publish_event` / `do_proc_event_reply` 入口 | 重入 | **不可恢复**（时序错乱） |
| `printf+SUSPEND`（5 类） | `do_proc_event_reply` 校验 2-7 | 订阅者误回复（endpoint/event 错等） | **可恢复**（忽略误回复，等待正确者） |
| `ENOSYS` | `do_proc_event_reply` 的 `!PRIV_PROC` | 普通进程误调用 | **可恢复**（回复 ENOSYS） |

### 2.10 与 `delay_call` / 消息优先级的前瞻

`event.c:39-41` 指出的两个安全保障不在本章实现：

- **信号**——`delay_call` 系统保证订阅必须发生在"不受事件影响的调用"（如 `semget` 而非 `semop`），详见 13-signal-flow.md。
- **退出**——消息类型优先级保证（具体 TODO 在 `forkexit.c`），详见 09-pm-exit.md。

本章仅保留钩子：`resume_event` 的终止分派 `exit_restart` / `restart_sigs` 在 Rust 侧为 DEFERRED 端口，本章只到调用点。

---

## 3 Rust 设计决策

Rust 改写遵循"语义重写（Rewrite）而非翻译（translate）"：保留 C 的外部行为与不变量，但用 Rust 的类型系统与注册表重新表达。以下决策对应设计契约 `.design/06-design.v1.md` 的 D1–D8。

### D1：订阅表聚合为 `EventRegistry`（ARCH A-3）

`EventRegistry`（`os/servers/pm/src/event.rs`）以 `subs: [Option<Subscriber>; NR_SUBS]` + `nsubs` + `nested` 聚合 C 的三个文件级全局（`event.c:60-67`）。`PmServer` 聚合 `event_registry: EventRegistry`（与 `table: ProcTable` 并列），消除静态可变。订阅者 `Subscriber { endpoint, mask, waiting }` 三字段与 C 同构，但 `mask` 类型化为 `ProcEventMask`（D2）、`waiting` 为 `usize`。

### D2：事件与掩码类型化（ARCH A-2/A-9）

`ProcEvent { Exit = 0x01, Signal = 0x02 }`（`syslib.h:292-293`）与 `ProcEventMask`（`bitflags!`，`0x01|0x02`）在 `minix-types` 收敛（`os/libs/minix-types/src/ipc/pm.rs` 或 `event.rs`，常量与 `is_proc_event` 族判定同 `VFS_PM_*` 的单一真相原则）。`mask.contains(event)` 替代 `mask & event`，`ProcEventMask::from_bits_truncate` 保留未知位以与 C 同行为。

### D3：`mp_eventsub` / `NO_EVENTSUB` / `EVENT_CALL` → 携带游标的阻塞态

`EventCursor(usize)`（`0..NR_SUBS`，`None = NO_EVENTSUB`）为下一个待试订阅者下标。`BlockState::IpcBlockReason::EventCall { cursor: EventCursor }` 携带游标（原 `EventCall` 无载荷，`ProcessIpc::event_subscriber: Option<UserSlot>` 为误建模——`UserSlot` 是进程槽位，不是订阅者下标）。合并后 `EVENT_CALL` 与游标同生同灭由类型保证，消除幽灵态；`ProcessIpc::event_subscriber` 废弃并在 02 文档同步更新映射表。

### D4：串行发送与容量（`transport.send` 即 `asynsend3`）

`resume_event(&mut self, target, table, transport)`（`os/servers/pm/src/event.rs`）与 C 同序：推断事件 → `while cursor < nsubs` 遇 `mask.contains_event` 即 `transport.send(subs[cursor].endpoint, &proc_event_msg)` + `waiting++` → `return`；否则清 `EventCall` → 按事件 `exit_restart` 或 `restart_sigs`（两者 DEFERRED，当前 `EventServices` 端口 no-op 可测）。容量由内核 `ASYN_NR = 2*NR_PROCS` 保障，Rust 侧不新增异步原语（与 05 D4 同理）。

### D5：`publish_event` 的服务死亡清理与事件推断

`publish_event(&mut self, target, table, transport)` 三段与 `event.c:316-353` 一一对应：断言 `nested==0` + `IN_USE && !EVENT_CALL` + `cursor None` → 若 `is_kernel_process && is_exiting` 则扫描 `subs` 找 `endpoint == table[target].endpoint` 并 `remove_sub` → 置 `EventCall { cursor: 0 }` → `resume_event`。顺序敏感：清理必须在置位前，否则会误删新事件游标。

### D6：`remove_sub` 的有序删除与游标回退（含 `nested` 守卫）

`remove_sub(&mut self, slot, table, transport)` 先 `copy_within(slot+1..nsubs, slot)` + `subs[nsubs-1]=None` + `nsubs--`，再遍历全表 `0..NR_PROCS` 对 `IN_USE|EVENT_CALL` 且游标有效者：`cursor == slot → nested++ → resume_event → nested--`；`cursor > slot → cursor--`。`nested` 计数与 `event.c:67/148-153/226/321` 同语义（`publish_event`/`do_proc_event_reply` 入口 `assert!(nested==0)`）。

### D7：`do_proceventmask` / `do_proc_event_reply` 的错误分层与 `ReplyIntent`

`do_proceventmask(&mut self, caller, mask, table) -> ReplyIntent`：`!PRIV_PROC → Reply(EPERM)`；已订阅项命中则 `mask empty && waiting==0 → remove_sub` 否则更新 mask → `Reply(OK)`；未命中且 mask empty → `Reply(OK)`；`nsubs==NR_SUBS → Reply(ENOMEM)`；否则 push → `Reply(OK)`。

`do_proc_event_reply(&mut self, msg, caller, table, transport) -> ReplyIntent`：`!PRIV_PROC → Reply(ENOSYS)`；其余 5 类校验失败（`pm_isokendpt` / `!EVENT_CALL` / 游标越界 / `endpoint != who_e` / 标志推断失败 / `event != inferred`）→ `ReplyLater`（C 的 `SUSPEND`，不回复本消息）；`mask` 不检查（`event.c:280-287`）；`waiting--` 后 `mask empty && waiting==0 → remove_sub` 否则 `cursor++ → resume_event`；任何成功路径恒返 `ReplyLater`（`main.c:88-89` 的 `result = SUSPEND`）。与 04 的 `ReplyIntent` 契约衔接：`dispatch_message` 的 `PROC_EVENT_REPLY` 分支改为调用本方法。

### D8：常量与编解码收敛到 `minix-types`（单一真相，模式 74）

`COMMON_RQ_BASE 0xE00` / `COMMON_RS_BASE 0xE80` / `PROC_EVENT 0xE03` / `PROC_EVENT_REPLY 0xE80` / `PROC_EVENT_EXIT 0x01` / `SIGNAL 0x02`（`com.h:597-619` / `syslib.h:292-293`）与 `MessLsysPmProceventmask` / `MessPmLsysProcEvent`（`ipc.h:1812/2566/2610`）在 `minix-types` 补齐（`os/libs/minix-types/src/ipc/pm.rs` 或新建 `event.rs`、`message.rs` 联合体成员）。`dispatcher.rs::PROC_EVENT_REPLY` 改为 `pub use minix_types::PROC_EVENT_REPLY`，消除 PM 本地重复常量（与 05 D1 的 `VFS_PM_RS_BASE` 跨 crate 重复同类盲点）。

---

## 4 实现详解

### 4.1 协议层（`os/libs/minix-types/src/ipc/pm.rs` 与 `message.rs`）

- 常量 `COMMON_RQ_BASE = 0xE00` / `COMMON_RS_BASE = 0xE80` / `PROC_EVENT = 0xE03` / `PROC_EVENT_REPLY = 0xE80`（`com.h:597-619`），以及 `PROC_EVENT_EXIT = 0x01` / `PROC_EVENT_SIGNAL = 0x02`（`syslib.h:292-293`），**数值严格锁定**（由 `test_proc_event_constants_match_com_h` 守卫）。
- `ProcEvent`（`Exit=0x01` / `Signal=0x02`）、`ProcEventMask`（bitflags，`EXIT`/`SIGNAL`）、`is_proc_event` 族判定（若需要）。
- `MessLsysPmProceventmask { mask: u32 }` 与 `MessPmLsysProcEvent { endpt: i32, event: u32 }`（各 56B，`repr(C)`，`_ASSERT_MSG_SIZE`），与 `ipc.h:1812/2566` 对齐；`MessageUnion` 新增 `m_lsys_pm_proceventmask` / `m_pm_lsys_proc_event` 成员。
- 编解码：`ProcEvent::encode_msg(target_endpoint, event)` 生成 `Message { m_type: PROC_EVENT, m_pm_lsys_proc_event: { endpt, event } }`；`ProcEventReply::decode` 校验 `m_type == PROC_EVENT_REPLY` 并提取 `endpt/event`。

### 4.2 注册表层（`os/servers/pm/src/event.rs`）

`EventRegistry`（`subs: [Option<Subscriber>; NR_SUBS]` 紧凑前缀 + `nsubs` + `nested`）提供：

- `new()`：全空。
- `len()` / `is_full()`：`nsubs` 与 `NR_SUBS` 比较。
- `publish_event(&mut self, target: UserSlot, table: &mut ProcTable, transport: &mut dyn IpcTransport)`（`event.c:316-353`）。
- `resume_event(&mut self, target: UserSlot, table: &mut ProcTable, transport: &mut dyn IpcTransport)`（`event.c:74-123`，含终止分派）。
- `remove_sub(&mut self, slot: usize, table: &mut ProcTable, transport: &mut dyn IpcTransport)`（`event.c:130-161`，`nested` 守卫）。
- `do_proceventmask(&mut self, caller: UserSlot, mask: ProcEventMask, table: &ProcTable) -> ReplyIntent`（`event.c:170-211`）。
- `do_proc_event_reply(&mut self, msg: &Message, caller: UserSlot, table: &mut ProcTable, transport: &mut dyn IpcTransport) -> ReplyIntent`（`event.c:218-309`，7 步校验，恒返 `ReplyLater` 除 `!PRIV_PROC` → `Reply(ENOSYS)`）。

`Subscriber { endpoint: Endpoint, mask: ProcEventMask, waiting: usize }` 三字段与 C 同构；`waiting < NR_PROCS` 的 `debug_assert!` 守卫。

### 4.3 阻塞态与游标（`os/servers/pm/src/mproc/block.rs`）

```rust
pub struct EventCursor(pub usize); // 0..NR_SUBS
pub enum IpcBlockReason {
    VfsCall { reply_to_new_parent: bool },
    EventCall { cursor: EventCursor }, // 新增 cursor 载荷（原无载荷）
    DelayedSignal,
}
```

`Process::set_event_cursor` / `event_cursor` / `clear_event_block` 等辅助；`ProcessIpc::event_subscriber` 废弃（`#[deprecated(note = "use BlockState::EventCall.cursor")]` 或直接移除，02 文档 §4 映射表同步）。

终止分派 `exit_restart`（09）与 `restart_sigs`（13）在 `resume_event` 尾部经 `EventServices` 端口调用（见 §4.4），当前为 no-op 脚手架（与 05 的 `PmServices::restart_signals` 同型，使协议可端到端验证）。

### 4.4 端口与接线层（`dispatcher.rs` / `init.rs`）

- `dispatcher.rs`：`PROC_EVENT_REPLY` 常量改为 `pub use minix_types::PROC_EVENT_REPLY`（D8）；`dispatch_message` 的 `PROC_EVENT_REPLY` 分支从 `ReplyLater` 钩子改为 `event_registry.do_proc_event_reply(msg, caller, table, transport)`（`table` 与 `transport` 由 `run_once` 传入），其 `ReplyIntent` 原样返回；`PmCall::ProcEventMask`（40）的 `dispatch_pm_call` 分支改为 `event_registry.do_proceventmask(caller, mask, table)`（`mask` 由 `Message` 解码）。
- `init.rs`：`PmServer` 新增 `event_registry: EventRegistry` 字段（`[ARCH: A-3]` 聚合，与 `table` 并列）；`run_once` 的第二路在 `dispatch_message` 之前或之内拦截 `PROC_EVENT_REPLY` 并经 `EventRegistry` 处理（与 05 在 `run_once` 第一路的 VFS 拦截同型；两者顺序与 `main.c:84-89` 一致：VFS 第一、`PROC_EVENT_REPLY` 第二、`IS_PM_CALL` 第三）；`abort_flag` 字段保持（05 已引入）。
- `event.rs` 对 `IpcTransport::send` 的失败按 C `panic("asynsend failed")` 语义 `expect`（与 05 的 `tell_vfs` → `Result` 不同——事件投递失败是上界被突破的不可恢复错误，直接 fail-fast）。

### 4.5 不变量表

| # | 不变量 | C 锚点 | Rust 表达 |
|---|--------|--------|-----------|
| 1 | 进程事件连续性（置 `EVENT_CALL` 时必置游标 0） | `event.c:349-350` | `set_event_block(EventCursor(0))` 原子 |
| 2 | 清除时同清标志与游标 | `event.c:116-117` | `clear_event_block` 同时清 `EventCall` 与游标 |
| 3 | 一进程同时最多一个事件 | `event.c:321-323` 断言 + `utility.c:131-132` 的 `VFS_CALL\|EVENT_CALL` 互斥 | `assert!(table[target].is_in_use() && !is_event_blocked())` + `EventCall` 与 `VfsCall` 互斥（`BlockState` 单 `Option`） |
| 4 | 游标 `< nsubs` 恒成立于 `EVENT_CALL` 时 | `event.c:97` 循环前置 | `debug_assert!(cursor.0 < self.nsubs)` |
| 5 | `waiting < NR_PROCS` | `event.c:108` | `debug_assert!(waiting < NR_PROCS)` |
| 6 | `nested==0` 于 `publish_event`/`do_proc_event_reply` 入口 | `event.c:226/321` | `assert_eq!(self.nested, 0)` |
| 7 | `nsubs` 恒等紧凑前缀长度 | `event.c:60-66 / 130-139` | `nsubs == subs[0..NR_SUBS].iter().filter(Option::is_some).count()` 不变量测试 |
| 8 | 满表 `nsubs==NR_SUBS` 时拒绝新订阅 | `event.c:200-204` | `is_full() → Reply(ENOMEM)` |

---

## 5 测试矩阵

### 5.1 `minix-types`（协议常量与编解码）

- `test_proc_event_constants_match_com_h`：锁定 `COMMON_RQ_BASE`/`COMMON_RS_BASE`/`PROC_EVENT`/`PROC_EVENT_REPLY`/`PROC_EVENT_EXIT`/`SIGNAL` 等于 `com.h:597-619` / `syslib.h:292-293`（P0 回归守卫）。
- `test_proc_event_mask_contains`：`ProcEventMask` 的 `contains_event` 与 `mask & event != 0` 等价（含空掩码、单事件、双事件或）。
- `test_proc_event_msg_roundtrip`：`PROC_EVENT` 消息编码→解码往返（`endpt`/`event` 在 `m_pm_lsys_proc_event`）。
- `test_proceventmask_msg_roundtrip`：`PROCEVENTMASK` 掩码往返（`mask` 在 `m_lsys_pm_proceventmask`）。

共 **4** 项协议测试。

### 5.2 `minix-pm`（注册表 + 状态机 + 集成）

**订阅表管理**：

- `test_proceventmask_new_subscription`：空表 `do_proceventmask(mask!=0)` → push，`nsubs==1`，`mask` 正确。
- `test_proceventmask_update_existing`：已订阅项 `do_proceventmask(new_mask)` → 更新 mask，不增 `nsubs`，返回 OK。
- `test_proceventmask_remove_when_idle`：已订阅且 `waiting==0` 时 `mask==0` → `remove_sub`，`nsubs--`，游标无影响。
- `test_proceventmask_defer_remove_when_waiting`：已订阅且 `waiting>0` 时 `mask==0` → `mask` 置空但不 `remove_sub`（`nsubs` 不变，`waiting` 保留），后续 `do_proc_event_reply` 的 `waiting--` 后才 `remove_sub`。
- `test_proceventmask_empty_mask_noop`：未订阅者 `mask==0` → OK 且 `nsubs` 不变。
- `test_proceventmask_enomem_when_full`：`nsubs==NR_SUBS` 时新订阅 → `ENOMEM`，表不变。

**事件发布与串行化**：

- `test_publish_event_no_subscriber_immediately_resumes`：无订阅者时 `publish_event` → 立即清 `EVENT_CALL` 并调用 `exit_restart`/`restart_sigs`（按事件类型，当前 no-op 可断言状态清理）。
- `test_publish_event_single_subscriber_sends`：单订阅且掩码命中 → `asynsend` 一条 `PROC_EVENT` 至该订阅者，`waiting==1`，目标进程保持 `EVENT_CALL` 且游标不变（`cursor==0`）。
- `test_publish_event_skips_non_matching`：订阅者掩码不命中 → `publish_event` 直接恢复（不发送，清标志）。
- `test_resume_event_serializes_two_subscribers`：两订阅者均命中 → `publish_event` 只发第一个，首订阅者 `PROC_EVENT_REPLY` 后 `resume_event` 再发第二个，第二个回复后才清标志并恢复。
- `test_publish_event_cleans_dead_subscriber_on_exit`：正在退出的特权服务（`PRIV_PROC|EXITING`）且其 endpoint 在 `subs` 中 → `publish_event` 先 `remove_sub`（`nsubs--`，受影响进程游标调整），再发布当前事件。

**`remove_sub` 的游标调整**：

- `test_remove_sub_adjusts_future_cursor`：删除 `slot=0`，游标 `1` 的进程 → `cursor` 变 `0`。
- `test_remove_sub_resumes_waiting_on_removed`：删除被等待订阅者 `slot`，游标 `==slot` 的进程 → `nested++` 后 `resume_event` 立即推进（发送下一匹配或恢复）。
- `test_remove_sub_nested_guard`：`remove_sub` 期间 `nested` 计数正确（入口 0，`resume_event` 期间 1，退出 0）。

**`do_proc_event_reply` 的 7 步校验**（每步单独构造错误消息，断言 `ReplyLater` 且表不变）：

- `test_reply_rejects_non_privileged_caller`：`!PRIV_PROC → Reply(ENOSYS)`（向误调用者回复）。
- `test_reply_rejects_bad_endpoint`：`pm_isokendpt` 失败 → `ReplyLater`。
- `test_reply_rejects_not_event_blocked`：`!EVENT_CALL` → `ReplyLater`。
- `test_reply_rejects_bad_cursor`：`NO_EVENTSUB` 或 `>=nsubs` → `ReplyLater`。
- `test_reply_rejects_wrong_subscriber`：`subs[i].endpoint != who_e` → `ReplyLater`。
- `test_reply_rejects_bad_flags`：`!(EXITING|UNPAUSED)` → `ReplyLater`。
- `test_reply_rejects_event_mismatch`：`msg.event != inferred` → `ReplyLater`。
- `test_reply_ignores_mask_mismatch`：`event` 与 `subs[i].mask` 不一致但前 7 步通过 → **不拒绝**（`event.c:280-287`），正常 `waiting--` 后推进。
- `test_reply_advances_to_next_subscriber`：正确回复 → `waiting--`，`cursor++`，`resume_event` 发下一条。
- `test_reply_removes_when_mask_empty_and_no_waiting`：`mask empty && waiting==1` 的回复 → `waiting` 归 0 后 `remove_sub`，不发下一条（`remove_sub` 已对受影响者推进）。

**集成**：

- `test_run_once_proc_event_reply_no_sync_reply`：`run_once` 收到 `PROC_EVENT_REPLY` → 经 `EventRegistry` 处理，无向订阅者的同步回复（`main.c:88-89` 的 `SUSPEND`）。

共 **~26** 项状态机/集成测试（含 `event.rs` 与 `init.rs`/`dispatcher.rs` 接线）。

---

## 6 过渡

- 04-ipc-dispatch.md 的 `PROC_EVENT_REPLY` "钩子"已落地为 `EventRegistry::do_proc_event_reply` 的 7 步校验与串行推进；`PmCall::ProcEventMask`（40）已落地为 `do_proceventmask` 的订阅表管理。`dispatcher.rs` 不再含死分支，`run_once` 第二路直接经注册表处理（与 05 在第一路的 VFS 拦截同型）。
- 05-vfs-interaction.md 的两处 `publish_event` 调用点（`EXIT/CORE` 与 `UNPAUSE`）在 Rust 侧经 `EventRegistry::publish_event` 发布，`resume_event` 的终止分派 `exit_restart`（09）与 `restart_sigs`（13）在 `EventRegistry` 中以 `EventServices` 端口（或直接 no-op）预留——06 的协议可独立测试与验证，09/13 落地时替换为真实实现。
- 09 落地 `exit_restart` 后，`resume_event` 的 EXIT 分支与 `publish_event` 的服务死亡清理将端到端连通；13 落地 `restart_sigs` 后，SIGNAL 分支与 `do_unpause` 后的事件流将闭环。
- 下一步：07-pm-fork（fork 的 `VFS_PM_FORK` 已由 05 的 `tell_vfs` 切通，本章补上 fork 后 `assert(mp_eventsub==NO_EVENTSUB)` 不变量）、09-pm-exit（`exit_proc`/`exit_restart`/`cleanup` 与事件设施的交互）、13-signal-flow（`check_pending`/`restart_sigs` 与事件设施的双向衔接）。

---

## 7 参见

- C 源（ground truth）：`minix3/minix/servers/pm/event.c:1-353`（全部）、`minix3/minix/servers/pm/mproc.h:27/86-104`（`mp_flags` 事件位）、`minix3/minix/servers/pm/const.h:13`（`NO_EVENTSUB`）、`minix3/minix/include/minix/com.h:597-619`（`COMMON_RQ/RS` + `PROC_EVENT` 族）、`minix3/minix/include/minix/ipc.h:1812/2566/2610`（`mess_pm_lsys_proc_event` / `m_lsys_pm_proceventmask` / `m_pm_lsys_proc_event`）、`minix3/minix/include/minix/syslib.h:289-293`（`PROC_EVENT_EXIT/SIGNAL`）、`minix3/minix/servers/pm/main.c:84-89/149-151/365/413`（主循环与两处发布）、`minix3/minix/servers/pm/forkexit.c:116/216`（`NO_EVENTSUB` 断言）、`minix3/minix/lib/libsys/asynsend.c:17`（`ASYN_NR`）。
- 设计契约：`.design/06-design.v1.md`（D1–D8 与行为契约表）、`.design/06-outline.v1.md`、`.design/06-outline-review.v1.md`。
- PM 阶段文档：04-ipc-dispatch.md（第二路钩子与 `ReplyIntent::ReplyLater` 契约）、05-vfs-interaction.md（两处 `publish_event` 调用点与提前 return）、02-mproc-struct.md（`BlockState` / `NO_EVENTSUB`）、03-mproc-table.md（`pm_isokendpt` / `UserSlot`）、13-signal-flow.md（`restart_sigs` 终止）、09-pm-exit.md（`exit_restart` 终止）。
- 阶段内顺序：01-pm-init-main.md（启动时 `NO_EVENTSUB` 初始化）、02-mproc-struct.md → 03-mproc-table.md → 04-ipc-dispatch.md → 05-vfs-interaction.md → **本章** → 09/13（终止分派消费者）。
- 对端实现：07-stage-ds（订阅方视角，仅交叉引用，不展开）。
- 内核接口：01-stage-kernel（`asynsend3` / `ipc_senda` 容量与投递语义）。
- OS 模式参考：Linux `signalfd`/`eventfd` + `epoll`、Redox `Scheme` handle 表、seL4 notification/reply-object、Fuchsia FIDL 事件流（见 §1.6）。

