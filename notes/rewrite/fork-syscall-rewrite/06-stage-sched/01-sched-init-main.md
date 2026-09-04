# 01 — SCHED 启动入口与主循环

本文介绍 SCHED 服务是怎样启动并进入主循环的。首先完成启动注册（登记 fresh 和 restart 两种初始化方式），然后读取机器信息（CPU 个数和启动 CPU 编号），最后进入一个永远循环：接收消息、分发处理、发送回复。理解这一篇，就知道了 SCHED 在系统启动时序中的位置，以及它的主循环骨架长什么样。

前置阅读：`00-sched-overview.md`（双层调度模型与启动主线图）。

> 本篇不讲什么：
> - 消息分发的具体规则——见 `02-sched-message-surface.md`（本篇只给出接收、分发、回复的骨架）
> - 各个处理函数的实现——见 `06-start-scheduling.md`、`07-stop-scheduling.md`、`08-noquantum-nice.md`
> - 平衡定时器的定义——见 `11-balance-queues.md`（本篇只提到它的调用位置）
> - 机器信息的具体用途——见 `10-pick-cpu-smp.md`（本篇只讲读取）

---

## 1 概念

### 1.0 引言

SCHED 启动要做三件事：第一，告诉 SEF 框架自己支持哪两种初始化方式；第二，把机器信息（有几个 CPU、哪个是启动 CPU）读进来；第三，进入主循环等待消息。本文假设读者已经了解双层调度模型（见 00 篇），只讲启动和主循环骨架。

### 1.1 为什么把调度策略放到用户态

一次调度包含两类工作：抢占、记账、排队这些必须在内核里快速完成的工作，和"谁先运行、谁后运行"这种策略判断。前者合进内核是不得已（要快），后者放在用户态是主动选择（要灵活）：策略一旦写进内核，改一次策略就要重新编译内核；而 SCHED 作为一个普通用户态进程，改策略只需要重新编译 SCHED 自己。内核通过 `sys_schedule` 接受指令，SCHED 在用户态做决策，两边通过消息配合：SCHED 接管一个进程、进程用完时间片后内核发通知、SCHED 再做下一次决策，这样转一圈就是一次完整的调度。

### 1.2 启动注册：fresh 和 restart

启动分两种情况。fresh 是第一次启动：SCHED 要自己读取机器信息、启动平衡定时器。restart 是服务崩溃后带状态重启：这部分逻辑由 SEF 通用库代办（`SEF_CB_INIT_RESTART_STATEFUL`），SCHED 自己没有对应的代码，但注册时必须把这个名字报上去——报了名，SEF 才承认这种重启方式。所以 `sef_local_startup` 只做一件事：登记两个初始化回调，然后把控制权交给 `sef_startup`。

### 1.3 读取机器信息：CPU 个数和启动 CPU

`sef_cb_init_fresh` 做两件事：调用 `sys_getmachine` 读取机器信息，调用 `init_scheduling` 启动平衡定时器。机器信息里 SCHED 只用两个字段：`processors_count`（有几个 CPU）和 `bsp_id`（哪个是启动 CPU），这两个数是第 10 篇选择 CPU 的依据。如果读取失败，C 代码直接 `panic`：带着未知的机器配置进入主循环还不如直接崩溃，Rust 实现保持同样的约定（失败路径放在二进制层处理）。注意这里的顺序：先读机器信息，后启动定时器。`init_scheduling` 的定义归第 11 篇，本篇只记录这个调用位置。

### 1.4 主循环骨架：接收、分发、回复

主循环是一个 `while (TRUE)` 死循环，每一轮分三步。第一步接收：调用 `sef_receive_status(ANY, ...)` 等待任意来源的消息，失败则 `panic`，然后取出发送方和调用号。第二步分发：先判断是不是系统通知——时钟通知就调用 `balance_queues` 整理队列，其他通知直接忽略，通知一律不回复；不是通知再按调用号分发到四个处理分支（接管、停止、改 nice、时间片耗尽），调用号对不上的进 `no_sys`。第三步回复：处理函数的返回值不是 `SUSPEND`，就把返回值填进消息发回去。分发和回复的具体规则在第 02 篇展开，各处理函数的实现在 06~08 篇。

### 1.5 SUSPEND：这次不回复的约定

回复有一个例外：处理函数返回 `SUSPEND` 时，主循环不发送回复。这不是忘记回复，而是"稍后再回复"——时间片耗尽这类消息本来就不需要同步答复，答复会走另外的异步路径。这个约定的完整说明在第 02 篇，本篇只需要记住主循环里有这样一个条件判断。

### 1.6 与其他 OS 的对照

- **Linux**：6.12 引入的 `sched_ext` 和 SCHED 是同一种思路——策略下沉到 BPF 程序（用户态写策略、内核执行），只不过 Linux 用 BPF 验证器做准入检查，对应 Minix 里 SEF 的启动登记。`SCHED_DEADLINE` 的准入判断，对应 SCHED 接管进程时的参数检查。更早的 Linux 调度器是策略和机制写在一起的单体结构。
- **Redox**：没有独立的调度服务，调度由内核完成、只暴露调参接口。SCHED 这样一个独立的用户态调度服务，是 Minix 微内核"服务都是进程"思路的自然结果。
- **seL4**：内核只提供优先级和通知机制，调度策略完全交给第一个用户态进程决定。策略和机制的分离在 seL4 里走得更远：连常驻的策略服务都可以没有，来消息时现算就行。

### 1.7 小结

策略放用户态是为了改策略不用动内核；启动时登记 fresh 和 restart 两种初始化方式；fresh 启动读出 CPU 个数和启动 CPU 编号，失败直接崩溃；主循环每轮做接收、分发、回复三件事，返回 `SUSPEND` 时跳过回复。记住一条顺序约束：没完成启动，就不能进主循环。

---

## 2 C 源码分析

### 2.1 主入口（`main.c:22-35,95-96`）

`main` 函数：准备消息缓冲和局部变量（`25-29`），调用 `sef_local_startup` 完成启动注册（`32`），进入死循环收活干活（`35-94`），最后的 `return(OK)`（`95`）永远执行不到，只是形式上的完整。

### 2.2 接收消息（`main.c:36-42`）

调用 `sef_receive_status(ANY, ...)` 等待任意来源的消息（`39-40`），失败则 `panic`——收不到消息属于启动约定被破坏，和读机器信息失败一样直接崩溃。然后取出发送方 `who_e` 和调用号 `call_nr`（`41-42`）。

### 2.3 通知优先处理（`main.c:44-55`）

先用 `is_ipc_notify` 判断是不是系统通知（`45`）。如果是时钟发来的通知，就调用 `balance_queues` 整理队列（`47-49`，函数定义在第 11 篇）；其他通知目前直接忽略（`50-52`）。通知处理完直接进下一轮（`54`），不发送回复——通知是告知，不是请求。

### 2.4 按调用号分发（`main.c:57-87`）

`SCHEDULING_INHERIT` 和 `SCHEDULING_START` 都进 `do_start_scheduling`（`58-61`，fork 出来的子进程和新启动的系统进程在这里合流）；`SCHEDULING_STOP` 进 `do_stop_scheduling`（`62-64`）；`SCHEDULING_SET_NICE` 进 `do_nice`（`65-67`）；`SCHEDULING_NO_QUANTUM` 先检查消息是不是真的来自内核（`70-71`），是才调用 `do_noquantum`，处理完不回复（`76`），不是则按伪造处理、返回 `EPERM`（`78-83`）；对不上的调用号进 `no_sys`（`85-86`，定义在第 02 篇）。

### 2.5 回复条件（`main.c:89-96`）

处理函数返回值不是 `SUSPEND`，就把返回值填到消息里并发回去（`90-93`）；是 `SUSPEND` 就跳过回复（`90` 的反面，含义见第 02 篇）；然后进入下一轮循环（`94`）。

### 2.6 回复发送（`main.c:101-106`）

`reply` 函数：调用 `ipc_send` 把消息发回去（`103`）；发送失败只打印一条日志（`104-105`），不再做别的处理——回复已经尽力，能否送达超出 SCHED 的控制范围。

### 2.7 启动注册（`main.c:111-121`）

`sef_local_startup`：登记 fresh 回调（`114`），登记 restart 回调为通用的有状态实现（`115`），注明暂无信号回调（`117`），然后调用 `sef_startup` 把控制权交给 SEF 框架（`120`）。

### 2.8 fresh 初始化（`main.c:126-136`）

`sef_cb_init_fresh`：调用 `sys_getmachine` 读取机器信息（`130`），失败则 `panic`（`131`）；调用 `init_scheduling` 启动平衡定时器（`133`，定义在第 11 篇）；返回 `OK` 表示启动完成（`135`）。

---

## 3 Rust 设计决策

Rust 改写不照抄 `main.c` 的函数指针登记方式，而是参考 RS 服务的 SEF 封装经验做了取舍。下面逐条说明 D1–D5。

### D1 启动方式做成枚举，不做 trait

- **C**：登记两个回调（`main.c:114-115`），restart 的实现在通用库（`sef.h:85`）。
- **Rust**：`SchedInitKind::{Fresh, RestartStateful}`（`os/servers/sched/src/sef.rs:19`），一个枚举列出两种启动方式。
- **为什么**：只有 fresh 有 SCHED 自己的代码，restart 完全由通用库代办。为这一个方法定义 trait 没有意义：不会有第二个实现，也不会有人拿它当泛型约束（对比 RS 的 SEF 封装，那边七个回调都有实质代码，用 trait 才合理，见 `os/servers/rs/src/sef.rs`）。备选方案（定义单方法 trait）被否决：没有多态需求，trait 只是装饰。

### D2 机器信息直接透传

- **C**：读取机器信息（`130`），失败崩溃（`131`），启动定时器（`133`）。
- **Rust**：`MachineInfo{processors_count, bsp_id}`（`os/servers/sched/src/sef.rs:34`，字段对应 `type.h:123-124`），`init_fresh(machine) -> FreshReady`（`os/servers/sched/src/sef.rs:56`），机器信息进、就绪令牌出。
- **为什么**：读取机器信息就是把两个数传进来；失败崩溃的逻辑放在二进制层（和 RS 的 `main.rs` 保持一致：崩溃是边界情况，判断是正事）；启动定时器是第 11 篇的语义，这里只保留调用位置。备选方案（在 `init_fresh` 里顺手调定时器）被否决：定时器的定义在第 11 篇，这里放占位调用就是越界。

### D3 用就绪令牌保证启动顺序

- **C**：读机器信息、启动定时器、返回 OK、进主循环（`130-135` 接 `35` 的顺序）。
- **Rust**：`FreshReady{machine}`（`os/servers/sched/src/sef.rs:46`），拿着这个令牌才表示启动完成。
- **为什么**：顺序本身就是约定（没读到机器信息就不能启动定时器）；令牌里带着机器信息（第 10 篇要用，不能丢）。备选方案（用一个布尔值表示就绪）被否决：布尔值带不出机器信息。

### D4 主循环只留骨架

- **C**：死循环做接收、分发、回复（`35-96`）；接收失败崩溃（`39-40`）；回复失败只打印（`104-105`）。
- **Rust**：`os/servers/sched/src/main.rs` 只保留骨架（启动、失败则停、等消息的注释）；接收、分发、回复的具体逻辑归第 02 篇。
- **为什么**：骨架回答"在哪里收、在哪里分、在哪里回"，分发表回答"怎么分"，两处各讲一遍就是重复定义。备选方案（本篇就定义分发枚举）被否决：分发表应该跟着使用方走（第 02 篇的分发逻辑）。

### D5 常量先行，结构体随后

- **C**：五个消息号（`com.h:801-807`）；`minix-types` 当时只有 BASE 和 NO_QUANTUM（plan §6.2 的已知缺口）。
- **Rust**：先补四个常量（`os/libs/minix-types/src/types/com.rs:92,95,98,101`，即 `BASE+2/+3/+4/+5`），并用测试锁定取值；消息结构体跟着使用方走（第 02 篇和第 13 篇）。
- **为什么**：消息号是第 02 篇和第 13 篇的共同前置（plan §6.2 明确写了）；结构体跟着使用方，哪里用哪里定义。备选方案（常量下沉到 sched crate）被否决：消息号是跨 crate 的契约，权威位置在 types。

### ARCH 决策总表

| ARCH | 落点 | 三处一致标注 |
|------|------|-------------|
| S-5 SMP 编译期开关改运行时 | `processors_count`/`bsp_id` 透传，运行时使用 | `os/servers/sched/src/sef.rs:34` + 本文档 D2 + §1.3 |
| S-7 SEF 生命周期（trait/回调表） | 两种启动方式做成枚举（对比 RS 的七方法 trait，原因是只有 fresh 有实质代码） | `os/servers/sched/src/sef.rs:19` + 本文档 D1 + §1.2 |

---

## 4 实现详解

### 4.1 模块结构

```
os/servers/sched/src/
├── sef.rs                — 本篇：启动方式、机器信息、就绪令牌
├── main.rs               — 二进制层：启动→失败则停→等消息骨架
└── lib.rs                — `#![cfg_attr(not(test), no_std)]` + 模块导出
os/libs/minix-types/src/types/
└── com.rs                — 四个消息常量的权威位置（02/13 篇复用）
```

> 设计决策：§3 D1（启动方式枚举）/ D2（机器信息透传）/ D5（常量先行）。

### 4.2 核心符号表

| 符号 | 来源 | Rust 位置 | 行为 |
|------|------|-----------|------|
| 两种启动方式 | `main.c:114-115` + `sef.h:85` | `os/servers/sched/src/sef.rs:19` | 登记名字枚举 |
| 机器信息 | `type.h:123-124` | `os/servers/sched/src/sef.rs:34` | 两个数透传 |
| 就绪令牌 | `main.c:130-135` | `os/servers/sched/src/sef.rs:46,56` | 机器信息进、令牌出 |
| 四个消息常量 | `com.h:801-807` | `os/libs/minix-types/src/types/com.rs:92,95,98,101` | 跨 crate 共用 |
| 主循环骨架 | `main.c:22-96` | `os/servers/sched/src/main.rs:1` | 启动→等消息的注释 |

### 4.3 不变量

| 不变量 | 位置 | 守卫 | 证据 |
|--------|------|------|------|
| 两种启动方式都要登记 | `SchedInitKind` 两个变体 | 少一个就是顺序错 | `main.c:114-115` |
| 先读机器信息，后做别的事 | `FreshReady` 令牌 | 没令牌不能进主循环 | `main.c:130-135` |
| 读失败直接崩溃 | 二进制层 panic | 崩溃即停 | `main.c:131` |
| 五个消息号齐全 | com.rs 测试锁定 | 少一个号就少一种契约 | `com.h:801-807` |

---

## 5 测试要点

> 基线以 `cargo test -p minix-sched --lib` 实际输出为准（改写时本地为 59 passed，见全仓回归报告）。

| 测试名 | 覆盖 C 行号 | 行为 | 文件 |
|--------|-------------|------|------|
| `test_init_kinds` | `main.c:114-115` + `sef.h:85` | 两种启动方式互相区分 | `os/servers/sched/src/sef.rs:65` |
| `test_machine_walks_in_token_walks_out` | `main.c:126-136` | 机器信息进、令牌带着同样的信息出 | `os/servers/sched/src/sef.rs:72` |
| `test_single_cpu_boot` | `type.h:123` | 单 CPU 是合法配置 | `os/servers/sched/src/sef.rs:85` |
| `test_sched_messages` | `com.h:801-807` | 五个消息号取值正确 | `os/libs/minix-types/src/types/com.rs:223` |

测试策略：启动方式用"两个变体互相不等"锁定；机器信息用"进去什么出来什么"锁定（含单 CPU 边界）；消息号用五个取值的全枚举锁定（BASE 加四个偏移）。

### 5.1 测试统计

基线以 `cargo test -p minix-sched --lib` 实际输出为准（改写时本地为 59 passed，见全仓回归报告）。本篇直接相关的测试是上表 4 个（含 types 侧 1 个）。完整测试清单：`rg "fn test_" os/servers/sched/src/sef.rs`。

---

## 6 过渡

本篇在 00（总览）之后、02（消息面）之前，讲的是"启动"：00 只给启动主线图，本篇把启动约定落到实处。没有本篇，第 02 篇的分发就不知道消息从哪里来，第 10 篇的选 CPU 就不知道机器信息从哪里来。

```
00-sched-overview: 启动主线图（启动在全图里的位置）
   │
   └─► 本篇：sef_local_startup 登记 → init_fresh 读机器信息 → FreshReady
           │                              │
           ├─► 02-sched-message-surface：接收、分发、回复的具体逻辑
           └─► 10-pick-cpu-smp：两个数的用途（选 CPU）
```

阅读顺序提示：想看主循环具体做什么，下一站 `02-sched-message-surface.md`；想看机器信息怎么用，见 `10-pick-cpu-smp.md`。

---

## 7 参见

- C 源：`minix3/minix/servers/sched/main.c:1-137`（启动与主循环全部代码）、`minix3/minix/include/minix/sef.h:85-95`（通用回调与重启实现）、`minix3/minix/include/minix/type.h:118-124`（机器信息结构）、`minix3/minix/include/minix/com.h:801-807`（五个消息号）
- 阶段文档：`00-sched-overview.md`（上一站）、`02-sched-message-surface.md`（下一站）、`10-pick-cpu-smp.md`（机器信息的用途）、`11-balance-queues.md`（定时器的定义）
- Rust 实现：`os/servers/sched/src/sef.rs:1`（本篇判定层）、`os/servers/rs/src/sef.rs:1`（RS 的 SEF 封装对照）、`os/libs/minix-types/src/types/com.rs:83`（常量权威位置）
- 对端：`../01-stage-kernel/11-scheduling-primitives.md`（内核调度原语）、`../03-stage-rs/01-rs-boot-init.md`（RS 的启动对照）
