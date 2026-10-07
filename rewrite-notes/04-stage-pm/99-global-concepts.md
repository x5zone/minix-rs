# 99 — global-concepts：跨机制共享的常量、身份编码与全局状态

> **状态**: 已完成（2026-09-09 改写）
> **定位**: 全局概念（跨文档共享的词汇表）——任何机制文档里的魔数，其权威定义与 C 锚点都在这里
> **源码**: pm/const.h、mproc.h、glo.h、pm.h、minix/callnr.h、minix/com.h、sys/signal.h、minix/type.h
> **Rust 模块**: mproc/constants.rs、minix-types（types/endpoint.rs、types/signal.rs）、init.rs
> **前置阅读**: 00；**后继**: 一切机制文档

## 1 概念

### 1.1 常量的语义半径

机制文档里的每个魔数——pid 为什么从 3 开始、信号位为什么是 `1<<(signo-1)`、endpoint 为什么能比较相等——其权威定义都在本章。本章关心的是每个常量的**语义半径**：它约束哪些行为、它对编码方式的隐式依赖是什么。

三个代表性的例子。**NR_PIDS=30000**（const.h:3）不只是"pid 上限"：它定义了一个轮转域——pid 分配器在域内轮转、跳过正在使用的值，`get_free_pid` 的相位（先自增再返回，INIT_PID+2=3 起步）是外部可观察契约（ps/kill 的语义），不可"现代化"。**NO_TRACER=0**（const.h:11）依赖"mproc 槽位下标从 0 开始"这一隐式事实——0 号槽位恰是 boot 的第一个条目，于是"tracer 为 0"永不与真实 tracer 冲突。Rust 用 `Option<UserSlot>` 表达"可能没有 tracer"，让这个隐式依赖在类型层显式消失。**NO_EVENTSUB=(char)-1**（const.h:13）依赖 char 的有符号性——同理，Rust 以 `Option<EventCursor>` 使哨兵不可表示。

### 1.2 全局状态的显式化

C 的 PM 用文件级全局承载每轮消息的上下文：`m_in`（进来的消息）、`who_p/who_e`（caller 槽位/endpoint）、`call_nr`、`mp`（当前操作的 mproc 指针），加上 `system_hz`、`abort_flag`、`monitor_params`。全局使重入不可分析：任何函数都可能在任何时刻读写它们。Rust 的 ARCH A-3 把它们全部显式化——消息是形参、caller 是 `UserSlot`、当前进程是 `&mproc` 借用、system_hz 是 `ProcTable` 字段、abort_flag 是 `PmServer` 字段——借用检查器由此成为并发审计器。

## 2 C 源码分析

### 2.1 身份与容量常量

| 常量 | 值 | C 锚点 | 语义 |
|------|-----|--------|------|
| NR_PIDS | 30000 | const.h:3 | pid 轮转域（0..NR_PIDS-1） |
| NO_PID | 0 | const.h:8 | "无进程"pid |
| INIT_PID | 1 | const.h:9 | INIT 的 pid |
| NO_TRACER | 0 | const.h:11 | mp_tracer 的"无 tracer"哨兵 |
| NO_EVENTSUB | (char)-1 | const.h:13 | mp_eventsub 的"无订阅"哨兵 |
| NR_ITIMERS | 3 | const.h:17 | interval timer 族数（REAL/VIRTUAL/PROF） |
| PROC_NAME_LEN | 16 | type.h:145 | mp_name 容量 |
| _NSIG | 64 | signal.h:45 | 信号数上界（位图宽） |
| NR_PROCS | `_NR_PROCS` | config.h:31 | 进程槽总数 |
| INIT_PROC_NR | 11 | com.h:72 | INIT 的槽位（LAST_SPECIAL_PROC_NR） |

### 2.2 endpoint 代际编码

C 的 endpoint 不是槽位号而是 `_ENDPOINT(generation, slot)` 的打包值：同一下标的槽位被复用时 generation 递增，旧 endpoint 立即失效（防 ABA——拿着过期 endpoint 的消息无法命中复用后的新进程）。Rust `minix_types::Endpoint::from_generation_slot(g, slot) = (g << ENDPOINT_GENERATION_SHIFT) + slot`，`slot()` 解码——与 C 宏同算术的 `const fn`。PM 全部跨服务器消息以 endpoint 寻址，`pm_isokendpt`（utility.c:108，Rust `ProcTable::pm_isokendpt`）是"endpoint → 当前槽位"的唯一校验门。

### 2.3 信号三集合与信号号

`core_sset`（QUIT/ILL/TRAP/ABRT/EMT/FPE/BUS/SEGV——默认 core dump）、`ign_sset`（CHLD/WINCH/CONT/INFO——默认忽略）、`noign_sset`（ILL/TRAP/EMT/FPE/BUS/SEGV——即使被 ignore，ksig 仍强制默认处置，"badignore"）。三集合在 C 由 main.c:154-165 运行时构建；信号号权威表在 sys/signal.h:52-83（1=HUP … 29=INFO），内核信号在其上：SIGSNDELAY=70、SIGKSIG=74（signal.h:264/274）。

### 2.4 全局状态七件套（glo.h）

| C 全局 | 语义 | Rust 归宿（ARCH A-3） |
|--------|------|----------------------|
| `m_in` | 本轮消息 | `run_once` 的 `msg` 形参 |
| `who_p` / `who_e` | caller 槽位/endpoint | `UserSlot` 形参 |
| `call_nr` | 本轮调用号 | `PmCall` 枚举 |
| `mp` | 当前操作的 mproc | `&mproc`/`ProcTable` 借用 |
| `system_hz` | 每秒 tick | `ProcTable.system_hz`（glo.h:12） |
| `abort_flag` | reboot how 位组 | `PmServer.abort_flag`（glo.h:26） |
| `monitor_params` | 启动参数 KVP | `BootParams.monitor_params`/`ParamStore` |

## 3 Rust 设计决策

1. **常量权威位置**：PM 专属常量在 `mproc/constants.rs`（NR_PIDS=30000/INIT_PID=1/NO_PID=0/NO_TRACER_INDEX=0，逐值 C 注释）；跨服务器共享的在 minix-types（_NSIG=64/NR_PROCS/endpoint 编码）。判据：是否被第二个 crate 消费。
2. **哨兵类型化**：NO_TRACER → `Option<UserSlot>`（`Guardianship::Normal`/`Traced.tracer`）；NO_EVENTSUB → `Option<EventCursor>`（`BlockState.ipc_blocked`）。哨兵值仅保留打印形态（`NO_EVENTSUB_RAW=-1`）。
3. **信号位基单点**：信号 s 的位 = `init::sig_bit(s)` = `1<<(s-1)`（C `__sigmask`，sigtypes.h:67-71）——全 crate 唯一位基入口（V2-P0-2/V3-P3-4 确立）。
4. **调用号单一真值**：`PmCall` 枚举（repr(i32)）判别值即调用号（callnr.h PM_BASE+1..47）；散落常量收敛 re-export（PROC_EVENT_REPLY，V3-P3-5）。
5. **svrctl 命令码算术化**：`const fn ioc` 复刻 `_IOC`（minix3/sys/sys/ioccom.h:_IOC（L84，工具生成）），四命令码逐位断言（V3-P2-5）。

## 4 实现详解

### 4.1 mproc/constants.rs
NR_PIDS/INIT_PID/NO_PID/NO_TRACER_INDEX 逐常量定义与 C 对账测试。

### 4.2 init.rs（三集合与启动常量）
`sig_bit`/`CORE_SIGSET`/`IGN_SIGSET`/`NOIGN_SIGSET` 编译期位图 + `test_signal_sets_match_c`/`test_signal_set_membership_matches_c_arrays` 逐位对账；`MULTIBOOT_PARAM_BUF_SIZE=1024`（multiboot.h:240）。

### 4.3 minix-types（共享身份）
`Endpoint::from_generation_slot/slot`（const fn 互逆）、`UserSlot`、`Pid/Uid/Gid/Clock` 类型别名、`SigSet(u64)`。

## 5 测试点

- `test_constants_match_c`（各模块同名测试：trace 19 项全量断言、timer/sched/misc/exec/time 各自族）
- `test_call_nr_roundtrip_all_registered`：47 调用号全量 roundtrip（calls.rs）
- `test_signal_sets_match_c` / `test_signal_set_membership_matches_c_arrays`：三集合位基（init.rs，V2-P0-2 回归锚点）
- `test_call_nr_rejects_unregistered`：越界/保留值拒绝
- endpoint 编解码：from_generation_slot/slot 互逆（minix-types）

## 6 过渡

前置：00（导航）。本章是全部机制文档的引用底层——机制文档引用常量时不重复定义，一律指向本章与 mproc/constants.rs。

## 7 参见

- 00（导航）、02（mproc 结构逐字段）、03（表操作）、04（分发骨架）
- plan.md §5.2（头文件覆盖表）、§4（ARCH A-3/A-11）
- minix3 源：pm/const.h、glo.h、callnr.h、com.h、sys/signal.h、sys/ptrace.h、sys/ioccom.h、sys/svrctl.h
