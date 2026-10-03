# 00-sched-overview: SCHED 整体架构概览

> **重写**: 2026-09-20（edge3 卡N/S41）
> **状态**: 正文
> **定位**: 阶段 0 总览（启动主线图 + 文档导航）
> **源码**: `minix3/minix/servers/sched/`（main.c 137 行、schedule.c 369 行、utility.c 74 行，加 proto.h 21 行、sched.h 18 行、schedproc.h 39 行，合计 658 行）
> **Rust 模块**: `os/servers/sched/src/`（14 个文件，约 2,860 行）
> **draft 素材**: `draft/00-sched-overview.md`（素材，fork 视角，已并入本篇）

## 1 概念：SCHED 是什么，为什么它是这个形状

### 1.1 一个用户态策略面，把"排班"从内核里搬出来

SCHED 是 Minix3 的用户态调度器。要理解它为什么存在，先看它**不**做什么：它不切换上下文、不管理就绪队列的硬件队列、不在时钟中断里抢 CPU——那些是内核的执行面。SCHED 只回答策略问题：一个新进程该从哪个优先级队列起步、一次该跑多少毫秒、CPU 忙不过来时谁该让路、nice 值动了之后进程该往哪个队列搬。内核收到这些决定后照办；SCHED 自己从头到尾不碰一颗 CPU 的调度状态。

这就是文档里反复出现的"双层调度模型"：**内核执行面**（就绪队列、上下文切换、时钟中断）与**用户策略面**（优先级、时间片、CPU 选择的决策）分离。分离的收益是把调度策略变成一个可以独立演进、独立失效、独立重启的用户态服务器——策略出问题不再等于内核出问题。代价是每一次策略决定都要走一次进程间消息，所以 SCHED 的消息面必须窄而稳：整个协议只有五个请求（`SCHEDULING_NO_QUANTUM`/`START`/`STOP`/`SET_NICE`/`INHERIT`，`com.h:803-807`），全部从 `SCHEDULING_BASE 0xF00`（`com.h:801`）起算。

minix-rs 保留这个双层结构，把 SCHED 改写成单线程事件循环（与 VFS 的 ARCH A-1 同思路，但 SCHED 原本就无线程池，改写成本更低）。策略表 `schedproc[]` 的不变量（五进程表一致性、队列编号界内、时间片非零增）由 `03-schedproc-struct` 与 `04-schedproc-table` 专篇守卫。

### 1.2 启动主线：从被 VM 装载、被 RS 放行，到接到第一条消息

SCHED 的一生按这条线走（每一站对应一篇机制文档）：

```
VM 装载 SCHED 镜像（SCHED 是 boot_image 成员，`kernel/table.c:56`）
  │  RS 决定何时放行它：`servers/rs/main.c:376` `sched_init_proc` + `:379` `SYS_PRIV_ALLOW`（参见 ../03-stage-rs/；四层归因见 ../00-master-plan/README.md）
  └─ main() (main.c:22)
       └─ sef_local_startup() (main.c:111)      注册生命周期回调
       └─ sef_cb_init_fresh()                   ← 01-sched-init-main
            └─ 初始化调度进程表、自举内核线程槽位
       └─ 主循环                                ← 02-sched-message-surface
            receive → 按五个 SCHEDULING 请求分发
            (START/STOP/NO_QUANTUM/SET_NICE/INHERIT)
```

运行期的三条外线：PM 转发用户的 nice 与调度请求（`13-pm-interaction`）、内核在时间片耗尽时发 `SCHEDULING_NO_QUANTUM` 通知（`09-schedule-process`）、RS 在驱动/服务生命周期变化时调整调度参数（`14-rs-interaction`）。

### 1.3 服务面：三个数字

五个调度请求（`com.h:803-807`，Rust 侧单一事实源在 `minix-types/src/types/com.rs` 的 `SCHEDULING_*`）、十六个优先级队列（`NR_SCHED_QUEUES`，`config.h:66`，Rust 侧 `schedproc.rs:18`）、一张 `schedproc` 表（容量与槽位检查见 `04-schedproc-table`）。数字各自的权威住处遵循全仓单一事实源原则：线上常量归 `minix-types`，策略参数归 `schedproc.rs`，表检查归 `valid.rs`。

### 1.4 设计原则（全阶段文档共守）

- **位置可回答性**：任何机制问题都有一个确定的篇章可以回答，不靠全库搜索
- **禁止前向引用**：阅读路径只向后依赖，机制细节在所属篇章，调用点只留锚
- **每篇一个语义单元**：一篇讲透一个机制，不做"杂物章"
- **ARCH 三处一致**：架构级改写必须同时标注在文档正文、design 快照、代码注释三处

## 2 C 源码分析：六个文件的分组地图

| 文件 | 行数 | 职责 | 对应篇章 |
|---|---|---|---|
| `main.c` | 137 | 入口、SEF 生命周期回调、主循环 | 01、02 |
| `schedule.c` | 369 | 五个请求的 handler 全体（策略核心） | 06~11 |
| `utility.c` | 74 | 共享小件（队列合法性、参数折算） | 05、08 |
| `schedproc.h` | 39 | `schedproc` 结构与队列常量 | 03、05 |
| `sched.h`/`proto.h` | 18/21 | 全局声明与函数原型 | 各篇按需 |

Rust 侧的模块对位：`main.rs`/`server.rs`（01/02）、`dispatch.rs`（02）、`schedproc.rs`/`table.rs`（03/04）、`priority.rs`（05）、`scheduling/`（06~09）、`cpu.rs`/`balancer.rs`（10/11）、`kernel_api/`（12）、`client.rs`（13）、`sef.rs`（14）、`valid.rs`（04/06 的检查面）。

## 3 文档导航：16 篇的阅读顺序

- **阶段 1 启动与消息面**：01（init/main）、02（消息面与分发）
- **阶段 2 数据结构**：03（schedproc 结构）、04（进程表与三道检查）
- **阶段 3 策略模型**：05（优先级与时间片，全阶段的公共参数篇）
- **阶段 4 生命周期与下发**：06（start 边界检查）、07（stop）、08（no_quantum 降级与 nice）、09（schedule 往内核下发）
- **阶段 5 SMP 面**：10（pick_cpu）、11（队列均衡）
- **阶段 6 三方交互**：12（内核接口）、13（PM 交互）、14（RS 交互）
- **阶段 7 收口**：99（全局概念、常量表、错误码）

阅读依赖只向后：05 是 06/08/09/10/11 的公共参数篇，03/04 是一切表操作的前置。

## 4 边界

- **前置依赖**: 无（SCHED 是自足的策略服务器；对照端见 12~14 三篇）
- **不覆盖（移交）**: 一切机制细节（见 01~14、99）；内核执行面的队列与切换见 `../01-stage-kernel/`；PM 侧的 nice 系统调用接口见 `../04-stage-pm/16-scheduling.md`

## 7 参见

- 本线通电与联调状态：`../../edge3.md` S27 行（已通电）；PM↔SCHED 回环验收面在 `../../edge4.md` §5 E5(e)
- 蓝图与多轮重写记录：本目录 `doc_rerank_*.md`
