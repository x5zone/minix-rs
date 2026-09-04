# 03 — schedproc 进程记录结构

本文介绍 SCHED 维护的进程记录 `schedproc`：一个进程用七个字段记录（身份两个、占用标记一个、优先级两个、时间片和 CPU 各一个），第八个字段 `cpu_mask` 没有实际读写，Rust 实现里直接去掉。理解这一篇，就知道了调度器眼中的"一个进程"长什么样。

前置阅读：`00-sched-overview.md`（五个进程表的分工）、`02-sched-message-surface.md`（消息里提到的人是谁）。

> 本篇不讲什么：
> - 表的管理与端点校验——见 `04-schedproc-table.md`（本篇只讲字段）
> - 字段取值的语义（上限和当前位置的关系、单位体系）——见 `05-priority-timeslice-model.md`（本篇只讲字段和边界）
> - 字段是谁写入的——见 `06-start-scheduling.md`、`07-stop-scheduling.md`、`08-noquantum-nice.md`

---

## 1 概念

### 1.0 引言

SCHED 眼中的进程只需要回答三个问题：是谁（端点号）、优先级如何（上限和当前位置）、在哪运行（时间片和 CPU）。本篇就讲这七个字段，外加一个被去掉的第八个字段。本文假设读者知道五个进程表的分工（见 00 篇），只讲"调度记录的结构"。

### 1.1 为什么调度记录要单独建一张表

一个进程在五个地方留记录：内核记运行状态、PM 记亲缘关系、VM 记地址空间、VFS 记打开文件、SCHED 记调度信息。各记各的事，每张表都只记自己需要ENG的。调度记录是最轻的一张：七个字段就够（身份两个、标记一个、优先级两个、时间片和 CPU 两个），没有亲缘明细（那是 PM 的事）、没有地址（那是 VM 的事）、没有打开计数（那是 VFS 的事）。轻是故意的：做决策只需要知道"谁先运行"，问多了表就重，表重了决策就慢。

### 1.2 身份字段：endpoint 和 parent

`endpoint` 是进程自己的端点号，`parent` 是父进程的端点号。端点号决定记录存在表的哪个槽位（换算规则在第 04 篇）；父进程决定出身（父进程是 RS 的就是系统进程，见第 10 篇）。这两个字段回答"是谁"：端点号对上了，槽位就找对了，合法性检查由第 04 篇负责。

### 1.3 占用标记：IN_USE

`flags` 字段配 `IN_USE` 一个标记：置上表示槽位有人，空着表示槽位空闲。全表就这一个标记——没有组合、没有备用位、没有第二种含义。一个标记一种含义，看到标记就知道槽位是空是占，不需要再猜。

### 1.4 优先级字段：max_priority 和 priority

`max_priority` 是上限（天花板），`priority` 是当前位置。上限是约束（不能超过，接管时的检查在第 06 篇）；当前位置是现状（会上下浮动，时间片用完会被降低，见第 08 篇）。上限和当前位置的关系（数值越小优先级越高、降低和恢复的规则）在第 05 篇，本篇只讲字段：两个都是 `unsigned` 类型，Rust 里都会换成带边界检查的新类型。

### 1.5 时间片和 CPU

`time_slice` 是分到的时间份额，单位是毫秒；`cpu` 是当前跑在哪个 CPU 上。时间片用完会触发耗尽通知（第 08 篇的事），CPU 由谁决定在第 10 篇。单位体系在第 05 篇（S-6），本篇只讲字段分"时间和地点"两类。

### 1.6 去掉的字段：cpu_mask

`cpu_mask` 有字段之名，没有读写之实：在 SCHED 服务内部没有任何代码读写它，只有一处 `FIXME set the cpu mask` 注释（`schedule.c:185`）；内核里同名的 `p_cpu_mask` 是另一张表的字段，和这里无关。去掉的理由有三条：消息面没有亲和性字段（接管消息只有 endpoint/parent/maxprio/quantum 四个字段，`ipc.h:1428-1433`），所以没有数据来源；没有任何代码读它，所以没有消费者；没有任何代码写它，所以没有生产者。三无字段留着只会误导后人（看到字段一定会问它什么意思，而这个问题没有答案）。去掉不是否认历史：C 结构体里还留着（兼容），Rust 结构里没有（S-3 标注了三处）。判断标准很简单：没有主人的字段，不进模型。

### 1.7 与其他 OS 的对照

- **Linux**：`task_struct`/`sched_entity` 对应这八个字段。`pid/tgid` 对身份字段，`__state` 对占用标记（Linux 的状态要复杂得多），`prio/static_prio/normal_prio` 对两个优先级字段（上限和当前位置拆成三分，是这里两分法的展开），`sum_exec_runtime/vruntime` 对时间片（Linux 记已经用了多少，SCHED 给份额——一个是已用，一个是配额），`cpus_ptr` 对 `cpu_mask`（区别在于 Linux 的亲和性是真正实现的，SCHED 的字段是空的——同名不同命）。
- **Redox**：进程结构把调度信息直接嵌在里面（优先级、时间片内嵌），没有独立的调度记录表。Minix 是五张表分开，Redox 是一份记录统管——分合之别。
- **seL4**：TCB 只记优先级（`tcbPriority`）和亲和性（`tcbAffinity`，真正实现的）。SCHED 的"轻"在 seL4 里更轻（连时间片都不记，时间片是内核的事），而 SCHED 去掉的亲和性字段在 seL4 里反而是活的（真正实现）。

### 1.8 小结

身份字段回答是谁，标记回答槽位空占，优先级字段回答上限和当前位置，时间片和 CPU 回答时间和地点，没有主人的 `cpu_mask` 不进模型。记住一条标准：每个字段都要有主人。

---

## 2 C 源码分析

### 2.1 文件头约定（`schedproc.h:1-16`）

文件注释说明一槽一位（`1-3`）；`_MAIN` 宏控制具化（`8-12`：只有 `main.c` 真正分配这张表，其他文件只做 extern 声明——分配一处，声明多处）；没有 SMP 时 `CONFIG_MAX_CPUS` 缺省为 1（`14-16`，S-5 的来源）。

### 2.2 身份字段（`schedproc.h:24-25`）

`endpoint`（`24`：进程端点号），`parent`（`25`：父进程端点号）。

### 2.3 标记字段（`schedproc.h:26,39`）

`flags`（`26`：标记位），`IN_USE 0x00001`（`39`：槽位有人是这个标记的唯一含义）。

### 2.4 优先级字段（`schedproc.h:29-30`）

`max_priority`（`29`：上限），`priority`（`30`：当前位置）。

### 2.5 时间片和 CPU（`schedproc.h:31-32`）

`time_slice`（`31`：份额），`cpu`（`32`：当前所在 CPU）。

### 2.6 去掉的字段（`schedproc.h:33-39` + `schedule.c:185`）

`cpu_mask`（`33-35`：位图数组，长度由 `BITMAP_CHUNKS(CONFIG_MAX_CPUS)` 决定）；SCHED 服务内部零读写（grep 只能找到声明和 `schedule.c:185` 的 FIXME，内核同名是别的表的字段）；按 plan §7.3 做 S-3 消除；`IN_USE` 定义（`39`）和它在同一段，但含义不相干。

### 2.7 表本体（`schedproc.h:36`）

`schedproc[NR_PROCS]` 静态表（一槽一位的本体；管理规则在第 04 篇）。

---

## 3 Rust 设计决策

Rust 改写不照抄八字段结构体，而是参考 Linux 的调度实体和内核已有的 `Priority` 类型做了取舍。下面逐条说明 D1–D5。

### D1 身份字段直接透传

- **C**：`endpoint_t endpoint/parent`（`schedproc.h:24-25`）。
- **Rust**：`SchedProc{endpoint: Endpoint, parent: Endpoint}`（`os/servers/sched/src/schedproc.rs:75`），端点类型直接用 minix-types 的。
- **为什么**：身份字段在各服务里语义相同；端点类型是跨服务共用的。备选方案（sched 自己定义一套端点类型）被否决：两处真源早晚对不上。

### D2 标记做成枚举

- **C**：`unsigned flags` 加 `IN_USE 0x1`（`schedproc.h:26,39`）。
- **Rust**：`SlotState::{Free, InUse}` 加 `is_used()`（`os/servers/sched/src/schedproc.rs:30,98`）；`IN_USE` 作为历史值保留注释（`os/servers/sched/src/schedproc.rs:23`）。
- **为什么**：一个标记一种含义；`unsigned` 会留下"别的位能不能置"的疑问，枚举直接关掉这个问题。备选方案（单标记 bitflags）被否决：单个标记没有组合，bitflags 只是装饰。

### D3 优先级做成新类型

- **C**：`unsigned max_priority/priority`（`schedproc.h:29-30`）。
- **Rust**：`Priority(u8)` 加 `new/get`（`os/servers/sched/src/schedproc.rs:46,51,60`），边界是 `NR_SCHED_QUEUES=16`（`os/servers/sched/src/schedproc.rs:17`）；形状和内核 `proc.rs` 的 `Priority` 相同，但各服务用各自己的。
- **为什么**：优先级有边界（接管时 `>=16` 进 `EINVAL`，见第 06 篇）；`u8` 自带非负；各服务用各自己的类型（和 02 篇 D4 同一个道理）。备选方案（复用内核的 `Priority`）被否决：跨服务依赖方向反了。

### D4 时间片写明单位

- **C**：`unsigned time_slice`（`schedproc.h:31`，毫秒）加 `unsigned cpu`（`schedproc.h:32`）。
- **Rust**：`time_slice_ms: u32`（名字里带单位，S-6）加 `cpu: u32`（`os/servers/sched/src/schedproc.rs:75`）。
- **为什么**：把单位写进名字（旧文档曾经误写成 ticks，R-4 的教训）；`u32` 对应 `unsigned`。备选方案（包一层 `Duration` 新类型）被否决：线上传的就是毫秒数，再包一层只是增加拆装成本（单位体系在第 05 篇统一）。

### D5 去掉死字段

- **C**：`cpu_mask[]`（`schedproc.h:33-35`，三无加 FIXME）。
- **Rust**：没有这个字段（S-3 三处标注：本节、plan §7.3、`os/servers/sched/src/schedproc.rs` 注释）。
- **为什么**：没有来源、没有读者、没有作者；用 `Option` 占位等于暗示"以后会有"，不如去掉干净。备选方案（保留 `Option<CpuMask>` 占位）被否决：占位是没想清楚的承诺，不如去掉。

### ARCH 决策总表

| ARCH | 落点 | 三处一致标注 |
|------|------|-------------|
| S-2 优先级类型（裸 unsigned 改新类型） | `Priority(u8)` 边界 16 | `os/servers/sched/src/schedproc.rs:46` + 本文档 D3 + §1.4 |
| S-3 死字段消除（不要 cpu_mask） | 没有这个字段，加注释说明 | `os/servers/sched/src/schedproc.rs:75` + 本文档 D5 + §1.6 |
| S-6 时间片单位（毫秒立约） | `time_slice_ms` 名字带单位 | `os/servers/sched/src/schedproc.rs:75` + 本文档 D4 + §1.5 |

---

## 4 实现详解

### 4.1 模块结构

```
os/servers/sched/src/
├── schedproc.rs          — 本篇：身份、标记、优先级、时间片和 CPU（去掉死字段）
├── sef.rs                — 启动对端（01 篇）
├── dispatch.rs           — 消息用人对端（02 篇，消息里提到的人）
└── lib.rs                — 模块导出
```

> 设计决策：§3 D1（身份透传）/ D2（标记枚举）/ D5（死字段消除）。

### 4.2 核心符号表

| 符号 | 来源 | Rust 位置 | 行为 |
|------|------|-----------|------|
| 身份 | `schedproc.h:24-25` | `os/servers/sched/src/schedproc.rs:75` | 端点透传 |
| 标记 | `schedproc.h:26,39` | `os/servers/sched/src/schedproc.rs:23,30,98` | 一个标记一种含义 |
| 优先级 | `schedproc.h:29-30` | `os/servers/sched/src/schedproc.rs:17,46,51,60` | 带边界的新类型 |
| 时间片和 CPU | `schedproc.h:31-32` | `os/servers/sched/src/schedproc.rs:75` | 名字带单位 |
| 死字段 | `schedproc.h:33-35` | （没有，加注释说明） | S-3 消除 |

### 4.3 不变量

| 不变量 | 位置 | 守卫 | 证据 |
|--------|------|------|------|
| 身份必透传 | `SchedProc` 两个字段 | 类型即约束 | `schedproc.h:24-25` |
| 标记只有两种 | `SlotState` 两个变体 | 穷举即封闭 | `schedproc.h:26,39` |
| 优先级小于 16 | `Priority::new` | 越界拒绝 | `config.h:66` |
| 时间片毫秒加当前 CPU | 名字带单位 | 名字即约定 | `schedproc.h:31-32` |
| 死字段没有 | 没有这个字段 | 结构本身即证明 | plan §7.3 |

---

## 5 测试要点

> 基线以 `cargo test -p minix-sched --lib` 实际输出为准（改写时本地为 59 passed，见全仓回归报告）。

| 测试名 | 覆盖 C 行号 | 行为 | 文件 |
|--------|-------------|------|------|
| `test_names_and_flag` | `schedproc.h:24-26,39` | 身份透传加标记两种含义加历史值 | `os/servers/sched/src/schedproc.rs:120` |
| `test_priority_bound` | `config.h:66` | 边界内收下、边界外拒绝（15 收、16 拒、255 拒） | `os/servers/sched/src/schedproc.rs:138` |
| `test_slice_and_cpu` | `schedproc.h:31-32` | 毫秒名字加当前 CPU | `os/servers/sched/src/schedproc.rs:148` |

测试策略：身份用端点值透传锁定；标记用两种含义加历史值锁定；优先级用边界三点（15 收、16 拒、255 拒）锁定；时间片和 CPU 用名字和值锁定；死字段用"结构里没有"收尾（读代码验证，不需要运行时断言）。

### 5.1 测试统计

基线以 `cargo test -p minix-sched --lib` 实际输出为准（改写时本地为 59 passed，见全仓回归报告）。本篇直接相关的测试是上表 3 个。完整测试清单：`rg "fn test_" os/servers/sched/src/schedproc.rs`。

---

## 6 过渡

本篇在 02（消息面）之后、04（表管理）之前，讲的是"记录结构"：02 的消息里提到的人，本篇回答"这个人存在哪、长什么样"。没有本篇，第 04 篇的表就不知道字段含义，第 05 篇的优先级就不知道边界从哪里来。

```
02-sched-message-surface: 五种消息（消息里提到的人）
   │
   └─► 本篇：七字段记录（人存在哪、长什么样）
           │                              │
           ├─► 04-schedproc-table：表管理（字段的下一站）
           └─► 05-priority-timeslice-model：优先级语义（边界的去向）
```

阅读顺序提示：想看字段怎么用，下一站 `04-schedproc-table.md`（表管理）；想看边界含义，见 `05-priority-timeslice-model.md`（优先级语义）。

---

## 7 参见

- C 源：`minix3/minix/servers/sched/schedproc.h:1-39`（记录全部代码）、`minix3/minix/include/minix/config.h:66`（队列数即边界）、`minix3/minix/servers/sched/schedule.c:185`（FIXME 死证）
- 阶段文档：`00-sched-overview.md`（五表分工）、`02-sched-message-surface.md`（上一站）、`04-schedproc-table.md`（下一站）、`05-priority-timeslice-model.md`（优先级语义）、`10-pick-cpu-smp.md`（机器信息的用途）
- Rust 实现：`os/servers/sched/src/schedproc.rs:1`（本篇判定层）、`os/kernel/src/proc.rs:373`（`Priority` 同形对端）
- 对端：`../01-stage-kernel/11-scheduling-primitives.md`（§3.3 新类型同款决策）、`../04-stage-pm/16-scheduling.md`（PM 记录对端）
