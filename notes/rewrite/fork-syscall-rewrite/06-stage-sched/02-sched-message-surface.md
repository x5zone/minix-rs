# 02 — SCHED 消息面与分发

本文介绍 SCHED 主循环里的消息处理规则。第 01 篇给出了接收、分发、回复的骨架，这一节给骨架填上具体内容：有哪五种消息、通知和调用分别怎么走、时间片耗尽的消息为什么要验来源、什么时候回复什么时候不回复、未知调用号怎么处理。可以把这一篇理解成主循环的用户手册：只做判断，不做执行，具体干活的是 06~08 篇的处理函数。

前置阅读：`01-sched-init-main.md`（启动与主循环骨架）。

> 本篇不讲什么：
> - 各个处理函数的实现——见 `06-start-scheduling.md`、`07-stop-scheduling.md`、`08-noquantum-nice.md`（本篇只讲分发到哪个入口）
> - 消息字段的具体语义——见 `09-schedule-process.md`（内核侧消息体）、`12-kernel-interface.md`（记账七字段）、`13-pm-interaction.md`（客户端侧消息体）
> - 队列整理的执行过程——见 `11-balance-queues.md`（本篇只讲时钟通知走哪个门）
> - 启动与机器信息——见 `01-sched-init-main.md`（本篇直接用骨架）

---

## 1 概念

### 1.0 引言

消息面要回答三个问题：有哪些消息（五种消息的名字和编号）、按什么顺序处理（通知先于调用）、回复规则是什么（返回 `SUSPEND` 就不回复）。本文假设读者已经知道主循环骨架（见 01 篇），只讲"消息面"这一层。

### 1.1 为什么主循环只做判断

主循环的骨架是"收、分、回"，消息面负责其中的"分"和"回"两步的判断：认出这是什么消息、决定它走哪个入口、决定最后要不要回复。注意这里只有判断，没有执行——真正的活（接管进程、降优先级、整理队列）都在处理函数里。这种"判断和执行分开"的结构，和 01 篇"策略和机制分开"是同一个思路在主循环内部的体现：分发表只认路，不开车。

### 1.2 五种消息总表

SCHED 处理五种消息：接管类两种（`SCHEDULING_INHERIT` 接管 fork 出来的子进程，`SCHEDULING_START` 接管新启动的进程）、停止一种（`SCHEDULING_STOP`，进程退出时不再调度它）、改 nice 一种（`SCHEDULING_SET_NICE`，调整优先级）、时间片耗尽一种（`SCHEDULING_NO_QUANTUM`，内核发来的）。编号从 `0xF01` 到 `0xF05`，谁发的、进哪个处理函数，都由编号唯一确定。编号对不上的消息一律进 `no_sys`，返回 `ENOSYS`。

### 1.3 通知和调用的区别

通知是告知，调用是请求：时钟发通知只是告诉 SCHED"时间到了"，不需要答复；四个处理分支都是有所求的请求，做完要给答复。所以通知优先处理（急事先办，不占用请求通道），而且处理完直接进下一轮，不发送回复。时钟通知触发队列整理（第 11 篇的活），其他通知目前直接忽略。

### 1.4 时间片耗尽消息的来源校验

时间片耗尽的消息最危险：它声称"某个进程的时间片用完了"，SCHED 收到后会降低那个进程的优先级。如果用户进程能伪造这种消息，就等于拿到了改别人调度的权力。所以主循环要验来源：只有带 `IPC_FLG_MSG_FROM_KERNEL` 标记的消息才可信，没有这个标记的一律拒绝（返回 `EPERM` 并打印一条日志）。校验归本篇，降优先级本身归第 08 篇。微内核里"不轻信用户进程传话"是基本原则，这个检查就是它的落实。

### 1.5 回复、不回复、通知不回复

回复分三种情况：普通请求必须回复（把返回值填进消息发回去）；处理函数返回 `SUSPEND` 表示"稍后再答复"，这次不回复；通知本来就不是请求，自然也没有回复。未知调用号比较特殊：它也有"回复"，内容固定是 `ENOSYS`——拒收本身就是一种答复。

### 1.6 与其他 OS 的对照

- **Linux**：netlink 的消息族分路对应这里的编号分路；`SIGCHLD` 这类信号通知对应时钟通知（只告知、不答复）；`seccomp` 的来源检查对应内核标记校验（只放可信的进来）。`sched_ext` 的 BPF 操作表对应这里的处理函数分发表——用表驱动分发，三个系统都是这个做法。
- **Redox**：进程间调用都走文件读写（打开、读、写就是发消息），没有独立的消息编号——编号分路在这里由路径分路代替，比如 `/scheme/sched/set_priority` 就相当于 `SCHEDULING_SET_NICE`。通知由事件队列承担。
- **seL4**：IPC 的徽章（badge）机制对应这里的来源校验。徽章由内核签发、用户伪造不了，相当于把"验来源"做进了信封本身；而 SCHED 是在收到信之后、在门口检查标记。

### 1.7 小结

五种消息列全是"名"，通知先于调用是"序"，只认内核标记是"门"，三种回复情况是"果"，未知编号一律拒收是"底"。记住一条分工：消息面只做判断，执行是处理函数的事。

---

## 2 C 源码分析

### 2.1 接收消息（`main.c:36-42`）

等任意来源的消息（`39`：`sef_receive_status(ANY)`，失败则崩溃——和 01 篇读机器信息失败一个约定），然后取出发送方（`41`）和调用号（`42`）。

### 2.2 通知分流（`main.c:44-55` + `com.h:92`）

先问是不是通知（`45`：`is_ipc_notify` 就是判断"这次收到的是否为 NOTIFY"，定义在 `com.h:92`）。时钟通知进队列整理（`47-49`，定义在第 11 篇）；其他通知目前空转（`50-52`）；通知一律不回复、直接下一轮（`54`）。

### 2.3 四路分发（`main.c:57-87`）

`INHERIT` 和 `START` 都进 `do_start_scheduling`（`58-61`：fork 次主线的入口在这里合流——合流只表示进同一个函数，两分支的区分在函数内部做）；`STOP` 进停止分支（`62-64`）；`SET_NICE` 进改 nice 分支（`65-67`）；`NO_QUANTUM` 走来源校验（`68-84`，见 §2.4）；未知编号进 `no_sys`（`85-86`）。

### 2.4 耗尽消息的校验（`main.c:68-84` + `ipcconst.h:28`）

先验标记（`70-71`：`IPC_FLG_MSG_FROM_KERNEL`，定义在 `ipcconst.h:28`，值为 1）。标记对了就调 `do_noquantum`，失败只打印警告，这次不回复（`72-76`）。标记不对就是伪造：打印警告、返回 `EPERM`（`78-83`，这个返回值会走正常的回复流程，见 §2.5）。

### 2.5 回复条件（`main.c:89-96`）

返回值不是 `SUSPEND` 就填进消息并发回去（`90-93`）；是 `SUSPEND` 就跳过（`90` 的反面）；进下一轮（`94`）；最后的 `return(OK)`（`95`）永远执行不到。

### 2.6 未知编号的拒收（`utility.c:18-23`）

`no_sys`：打印调用号和发送方（`21`：谁、什么事），固定返回 `ENOSYS`（`22`），没有任何例外。

### 2.7 七个消息体的位置

| 消息体 | 字段 | C 位置 | Rust 对应 |
|------|------|--------|----------|
| `mess_lsys_sched_scheduling_start` | endpoint/parent/maxprio/quantum + 40 字节补齐 | `ipc.h:1430-1438` | `MessLsysSchedSchedulingStart` |
| `mess_lsys_sched_scheduling_stop` | endpoint + 52 字节补齐 | `ipc.h:1440-1445` | `MessLsysSchedSchedulingStop` |
| `mess_pm_sched_scheduling_set_nice` | endpoint/maxprio(u32) + 48 字节补齐 | `ipc.h:1822-1828` | `MessPmSchedSchedulingSetNice` |
| `mess_sched_lsys_scheduling_start` | scheduler + 52 字节补齐 | `ipc.h:1908-1913` | `MessSchedLsysSchedulingStart` |
| `mess_krn_lsys_schedule` | 记账七字段 + 24 字节补齐 | `ipc.h:261-273` | 已有（`MessKrnLsysSchedule`，第 12 篇详述） |
| `mess_lsys_krn_schedctl` | flags/endpoint/priority/quantum/cpu + 36 字节补齐 | `ipc.h:1093-1102` | 已有（第 09 篇用） |
| `mess_lsys_krn_schedule` | endpoint/quantum/priority/cpu/niced + 36 字节补齐 | `ipc.h:1104-1113` | 已有（第 09 篇用） |

所有消息体都是 56 字节（`_ASSERT_MSG_SIZE` 保证）。其中四个是本篇新增的：前三个给第 06 篇和第 13 篇用，回复体给第 06 篇用，本篇只列出位置（D6）。

---

## 3 Rust 设计决策

Rust 改写不照抄 `main.c` 的 switch 写法，而是参考 Linux 的表驱动分发做了取舍。下面逐条说明 D1–D6。

### D1 收到的消息做成枚举

- **C**：五路 switch 加一个 default（`main.c:57-87`）。
- **Rust**：`SchedMsg::{Inherit, Start, Stop, SetNice, NoQuantum}` 加 `from_raw()`（`os/servers/sched/src/dispatch.rs:25,40`），编号复用 01 篇在 types 里定义的常量。
- **为什么**：分发表本身就是契约；未知编号转成 `None`，后面自然接到拒收逻辑（D5）。备选方案（透传裸 `i32` 编号）被否决：编号错了就会走错分支，用枚举让走错分支成为不可能。

### D2 通知和调用做成三态

- **C**：通知判断宏（`com.h:92`），时钟分支（`47-49`），通知不回复直接下一轮（`54`）。
- **Rust**：`Incoming::{NotifyClock, NotifyOther, Call}` 加 `classify()`（`os/servers/sched/src/dispatch.rs:54,69`），两个布尔输入、三种形态输出。
- **为什么**：通知和请求性质不同；三种形态后续动作各不相同（整理队列 / 忽略 / 分发）。备选方案（直接透传布尔值）被否决：后续动作会散落到调用点各写一遍。

### D3 来源校验只做布尔判断

- **C**：验标记（`70-71`），警告加执行加不回复（`72-76`），伪造拒绝（`78-83`）。
- **Rust**：`noquantum_trust()`（`os/servers/sched/src/dispatch.rs:86`），只回答标记对不对；打印、回复、降级都归第 08 篇。
- **为什么**：校验是"门"，打印和回复是"事"（来源信任模型的完整说明在第 08 篇）；伪造时的 `EPERM` 和打印是处理函数分支内部的事，在门口就定好后续动作属于越界。备选方案（三值 fate 枚举）被否决：警告和执行的动作归处理函数，门口不定。

### D4 回复规则单独建模

- **C**：返回值不是挂起就回复（`90-93`）；`SUSPEND` 的含义（`com.h:1151`，值为 `-998`）。
- **Rust**：`SUSPEND` 常量（`os/servers/sched/src/dispatch.rs:21`），`DispatchVerdict::{Reply, NoReply}` 加 `settle()`（`os/servers/sched/src/dispatch.rs:92,103`）：挂起就不回复，其余都回复。
- **为什么**：挂起约定（R-14）由本篇正式确定；VFS 也有同名的意图类型，但那是 VFS 自己 crate 里的，各服务用各自己的（跨服务复用等于制造反向依赖）。备选方案（复用 VFS 的枚举）被否决：不能为了一个数字引入跨服务依赖。

### D5 拒收就是一个固定值

- **C**：`no_sys` 打印加固定返回（`utility.c:18-23`）。
- **Rust**：`no_sys_verdict()`（`os/servers/sched/src/dispatch.rs:115`），固定返回 `ENOSYS`；打印留在调用方。
- **为什么**：拒收恒定不变；只有一个取值的判断不需要专门定义枚举。备选方案（定义错误枚举）被否决：单值枚举只是装饰。

### D6 四个消息体放到 minix-types

- **C**：四个结构体加联合体里的四个成员（`ipc.h:1430-1445,1822-1828,1908-1913` 加 `ipc.h:2568,2569,2612,2621`）。
- **Rust**：四个结构体进 minix-types（`os/libs/minix-types/src/ipc/message.rs`，`#[repr(C)]` 加补齐加 `Default`，和已有的三个同风格），联合体加四个成员，用布局测试锁定 56 字节。
- **为什么**：消息体跟着使用方走（第 02 篇用分发表、第 06/13 篇用结构体），但权威位置在 types（了结 plan §6.2 的前置缺口）；布局测试锁定 56 字节就是"结构体本身即契约"。备选方案（结构体下沉到 sched crate）被否决：消息体是跨服务的契约，权威位置在 types。

### ARCH 决策总表

| ARCH | 落点 | 三处一致标注 |
|------|------|-------------|
| S-8 消息传递模型（同步调用 + 异步回复） | `settle` 挂起即不回复；`SUSPEND` 伪返回码保留 | `os/servers/sched/src/dispatch.rs:92` + 本文档 D4 + §1.5 |

---

## 4 实现详解

### 4.1 模块结构

```
os/servers/sched/src/
├── dispatch.rs           — 本篇：消息枚举、分流、校验、回复、拒收
├── sef.rs                — 启动对端（01 篇，两种启动方式与机器信息）
└── lib.rs                — 模块导出
os/libs/minix-types/src/ipc/
└── message.rs            — 四个消息体加联合体成员（02/06/13 篇复用）
```

> 设计决策：§3 D1（消息枚举）/ D2（通知调用分流）/ D4（回复规则）/ D6（消息体位置）。

### 4.2 核心符号表

| 符号 | 来源 | Rust 位置 | 行为 |
|------|------|-----------|------|
| 五种消息 | `main.c:57-87` | `os/servers/sched/src/dispatch.rs:25,40` | 穷举加未知拒收 |
| 三种到来形态 | `main.c:44-55` | `os/servers/sched/src/dispatch.rs:54,69` | 通知调用分流 |
| 来源校验 | `main.c:70-83` | `os/servers/sched/src/dispatch.rs:86` | 只验标记对错 |
| 回复判断 | `main.c:89-96` | `os/servers/sched/src/dispatch.rs:21,92,103` | 挂起就不回复 |
| 拒收 | `utility.c:18-23` | `os/servers/sched/src/dispatch.rs:115` | 固定 `ENOSYS` |
| 消息体 | `ipc.h` 七个结构 | `os/libs/minix-types/src/ipc/message.rs` 四个结构加联合体 | 56 字节锁定 |

### 4.3 不变量

| 不变量 | 位置 | 守卫 | 证据 |
|--------|------|------|------|
| 五种消息穷举 | `SchedMsg` 五个变体 | 未知编号拒收 | `main.c:58-68` |
| 通知优先且不回复 | `Incoming` 三个变体 | 通知不进分发 | `main.c:45-55` |
| 耗尽消息只认内核 | `noquantum_trust` | 没标记就拒绝 | `main.c:70-71` |
| 回复分三种情况 | `settle` 加 `classify` | 挂起就不回复 | `main.c:54,90-93` |
| 未知编号固定拒收 | `no_sys_verdict` | 恒定 `ENOSYS` | `utility.c:22` |
| 消息体 56 字节 | 补齐字段加测试 | 尺寸断言 | `ipc.h` 的 `_ASSERT_MSG_SIZE` |

---

## 5 测试要点

> 基线以 `cargo test -p minix-sched --lib` 实际输出为准（改写时本地为 59 passed，见全仓回归报告）。

| 测试名 | 覆盖 C 行号 | 行为 | 文件 |
|--------|-------------|------|------|
| `test_five_letters` | `main.c:57-87` + `com.h:801-807` | 五种编号识别加未知拒收，INHERIT 和 START 是两个不同的变体 | `os/servers/sched/src/dispatch.rs:124` |
| `test_notify_first` | `main.c:44-55` | 调用进分发，时钟通知整理、其他通知忽略 | `os/servers/sched/src/dispatch.rs:147` |
| `test_seal_and_settle` | `main.c:68-96` + `utility.c:18-23` | 标记校验加挂起回复加拒收 | `os/servers/sched/src/dispatch.rs:157` |
| `test_sched_message_layouts` | `ipc.h:1428-1912` | 四个结构体 56 字节加字段值 | `os/libs/minix-types/src/ipc/message.rs:2477` |

测试策略：消息用五个取值的全枚举锁定（含未知编号拒收）；分流用三种形态全覆盖；校验用标记有无两极覆盖；回复用挂起、正常、错误码三格覆盖（含 `EPERM` 也要回复）；拒收用固定值覆盖；消息体用尺寸加字段值覆盖。

### 5.1 测试统计

基线以 `cargo test -p minix-sched --lib` 实际输出为准（改写时本地为 59 passed，见全仓回归报告）。本篇直接相关的测试是上表 4 个（含 types 侧布局测试 1 个）。完整测试清单：`rg "fn test_" os/servers/sched/src/dispatch.rs`。

---

## 6 过渡

本篇在 01（骨架）之后、03（记录结构）之前，讲的是"消息面"：01 给出主循环骨架，本篇填上五种消息和分发规则。没有本篇，第 06 篇的处理函数就不知道入口从哪里来，第 11 篇的队列整理就不知道时钟从哪里来。

```
01-sched-init-main: 启动 → 主循环骨架
   │
   └─► 本篇：五种消息 → 通知调用分流 → 回复规则
           │                              │
           ├─► 06-start-scheduling：接管处理（入口的下一站）
           └─► 03-schedproc-struct：进程记录（消息里提到的人存在哪）
```

阅读顺序提示：想看入口后面的处理，下一站 `06-start-scheduling.md`（03/04/05 篇讲表和模型，可以先看）；想回看骨架，见 `01-sched-init-main.md`。

---

## 7 参见

- C 源：`minix3/minix/servers/sched/main.c:35-106`（接收分发回复全部代码）、`minix3/minix/servers/sched/utility.c:18-23`（未知编号拒收）、`minix3/minix/include/minix/com.h:801-807,1151`（五个编号加挂起含义）、`minix3/minix/include/minix/com.h:92`（通知判断宏）、`minix3/minix/include/minix/ipcconst.h:28`（内核标记）、`minix3/minix/include/minix/ipc.h:261-273,1093-1113,1430-1445,1822-1828,1908-1913`（七个消息体）
- 阶段文档：`01-sched-init-main.md`（上一站）、`06-start-scheduling.md`（下一站）、`03-schedproc-struct.md`（记录的去向）、`09-schedule-process.md`（内核侧消息体对端）、`12-kernel-interface.md`（记账字段对端）、`13-pm-interaction.md`（客户端消息体对端）
- Rust 实现：`os/servers/sched/src/dispatch.rs:1`（本篇判定层）、`os/libs/minix-types/src/ipc/message.rs`（消息体权威位置）
- 对端：`../01-stage-kernel/11-scheduling-primitives.md`（内核调度原语）
