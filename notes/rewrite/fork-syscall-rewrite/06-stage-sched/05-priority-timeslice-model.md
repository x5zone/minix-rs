# 05 — 优先级与时间片模型

本文介绍 SCHED 调度器里最基础的一套参数：16 个优先级队列怎么编号、`max_priority`（上限）和 `priority`（当前位置）两个数是什么关系、时间片 `time_slice` 的单位为什么是毫秒、nice 值（-20~20）怎么折算成队列，以及怎么判断一个进程是不是系统进程。这些定义会被后面好几篇文档反复用到（06 做边界检查、08 做降级和恢复、09 往内核下发、10 选 CPU），所以先集中讲清楚，后面直接引用，不再重复解释。

前置阅读：`03-schedproc-struct.md`（结构体有哪些字段）、`04-schedproc-table.md`（进程表的三道检查）。

> 本章不讲什么：
> - 结构体字段和表检查的具体实现 —— 见 `03-schedproc-struct.md`、`04-schedproc-table.md`（本篇只借用字段名）
> - 各个 handler 里怎么用这些参数 —— 见 `06-start-scheduling.md`（边界检查）、`08-noquantum-nice.md`（降级和 nice 调整）、`09-schedule-process.md`（往内核下发）、`10-pick-cpu-smp.md`（选 CPU）、`11-balance-queues.md`（优先级恢复）
> - 内核那边的参数校验实现 —— 见 `09-schedule-process.md`（本篇只说明 SCHED 这边有哪些检查）
> - PM 的 nice 系统调用的用户接口 —— 见 `../04-stage-pm/16-scheduling.md`（本篇只给换算公式）

---

## 1 概念

### 1.0 引言

调度器要回答三个基本问题：进程排在哪个队列（谁优先）、一次能跑多久（时间片）、进程是什么来头（nice 值和父进程是谁）。答完这三个问题，队列编号、上下限两个数、时间片单位、nice 换算公式、系统进程的判断标准就全齐了。本章默认你已经知道结构体长什么样（见 03）和表检查怎么工作（见 04），只讲"这些公共参数的统一含义"。

### 1.1 为什么要把参数定义集中到一篇

后面每个 handler 都会提到这些数字：06 要检查上限有没有越界（`>= 16` 就拒绝），08 要做降级（时间片用完降一级）和恢复（11 再升回来），还要处理 nice 调整。如果每篇都自己解释一遍"上限是什么意思"，很容易各说各话——06 的上限是一个意思，08 的上限是另一个意思。所以本篇先把定义立下来，后面几篇只引用：定义只有一份，用法散在各处。定义不清楚就到处用，肯定会乱。

### 1.2 16 个队列

一共 16 个队列，编号 `0..16`，数字越小优先级越高：0 是最高，15 是最低。用户进程可以用满整个范围（`MAX_USER_Q=0` 和 `TASK_Q=0` 数字一样——这不是笔误：用户进程可以升到最高队列，但那是调度器授予的，不是天生就有的）。新建进程默认排在中间（`USER_Q=7`：15 个用户队列取一半，整数除法截断得到）。队列总数是有约定的（`NR_SCHED_QUEUES=16`，宏注释写着 `MUST equal minimum priority + 1`）：队列数和最低优先级是锁死的，改一个就得改另一个，这个约定只写在宏注释里，代码里没有强制手段。另外还有一个默认 CPU（`USER_DEFAULT_CPU=-1`：意思是"不指定就用默认的"，选 CPU 的细节归第 10 篇）。

### 1.3 上限和当前位置

每个进程有两个优先级数字：`max_priority`（上限，相当于天花板）和 `priority`（当前位置，相当于现在排在哪）。上限是约束条件，不能超过：06 的检查负责拦，`do_nice` 改的时候两个数一起改（08 的内容）。当前位置是实时状态，可以上下浮动：08 在时间片用完时把它降一级，11 在做队列平衡时再升回来。降级是有下限的（`MIN_USER_Q`：降到底就停，不会再降），恢复是有上限的（`max_priority`：升到上限就停，不会超过约束）。上限和当前位置的差值就是"还能再降几级"，差值用完了进程就钉在最低队列——钉在底不等于死了，11 的恢复逻辑照样能把它升回来。另外还有一个跟上限走的标记（`niced = max_priority > USER_Q`：上限超过中间队列就打标，跟着消息一起发给内核，09 的内容）。降级和恢复的细节归 08 和 11，本篇只讲清"上限是约束、当前位置是状态"这个分工。

### 1.4 时间片，单位是毫秒

`time_slice` 表示一个进程一次能跑多久。单位是毫秒：SCHED 给出的数字，直接送进内核的 `p_quantum_size_ms` 字段（`system.c:683`），内核再用 `ms_2_cpu_time` 把它换算成 CPU 时间——毫秒是 SCHED 和内核之间约好的单位，所以 Rust 这边的字段名必须把单位写出来（`time_slice_ms`，03 的 D4 决定的）。时间片有个下限（`>= 1`：内核拒绝 0，`system.c:648`——SCHED 存的时候不检查，检查发生在下发那一步，09 的内容）。配额有两个名字：`USER_QUANTUM`（默认配额，`config.h:74`）和 `DEFAULT_USER_TIME_SLICE`（新槽位的初始值，`schedule.c:41`）——两个都是 200，但来源不同：一个是"配额没指定时的默认值"，一个是"新槽位出生时的初始值"。数值相同只是巧合，来源不同才是本质：要是合并成一个，以后有人改配额默认值就会连带改掉新槽位初值，那是个坑。

### 1.5 nice 值到队列的换算

换算公式是把 41 级 nice 值（`-20..20`）映射到 16 个队列：`MAX_USER_Q + (nice - PRIO_MIN) × 16 / 41`。nice 0 正好落在中间队列（`USER_Q=7`）：`20 × 16 / 41 = 7`，整数除法截断得到——"nice 0 对应中间队列"是算出来的，不是拍脑袋指定的。相邻的 nice 值会落在同一个队列（41 级往 16 个队列里装，截断之后只能共享：nice `-20` 和 `-19` 都在 0 号队列）：共享是公式的必然结果，不是 bug。超出范围的 nice 值直接拒绝（`nice < -20` 或 `> 20` 就返回 `EINVAL`，这是 PM 那边的事）。公式后面还有个钳位（C 代码把结果钳在 `0..15`）：那是个永远触发不了的死代码——合法 nice 值算出来一定在范围内，所以 Rust 直接省略了，并在注释里说明了原因（D2）。

### 1.6 怎么判断系统进程

`is_system_proc`：父进程是 RS 就是系统进程（`parent == RS_PROC_NR`）。注意这判断的是"谁生的"，不是"谁权限大"——系统进程的特殊之处只在选 CPU（10：系统进程固定跑在 BSP 上）和启动流程（START 分支显式给时间片，06 的内容），队列优先级上一视同仁，都要过边界检查。这个判断只看一代：RS 的儿子是系统进程，系统进程的儿子就不是了（孙子的父进程不是 RS）——"不隔代"是构造方式决定的（登记时只记录直接父进程）。

### 1.7 其他系统的同类设计

- **Linux** 用三个数对应这里的两个数：`static_prio`（上限的意思）、`prio`（当前位置的意思）、`normal_prio`（恢复时的基准）——相当于把"上限和当前位置"展开成了三分，恢复基准独立成了一个数。`USER_QUANTUM` 的 200ms 对应 Linux 的 `sysctl_sched_latency`（Linux 用纳秒计的调度周期，SCHED 用毫秒计的时间份额——周期和份额是时间的两种表达）。`cpu_mask` 字段对应 Linux 的 `cpus_ptr`，但两边命运完全不同：Linux 的亲和性掩码是真正实现的，SCHED 的这个字段根本没人读写（S-3）。
- **Redox** 把调度参数直接嵌在进程结构里（优先级和时间片不单独成表），所以没有"参数前置成一篇"的做法——Minix 的 handler 都要读同一套参数，所以值得单独写一篇；Redox 参数跟着进程走，就随进程一起讲。
- **seL4** 的 TCB 只记优先级（`tcbPriority`），没有时间片的概念（时间片纯粹是内核内部的事）——SCHED 的"时间片"在 seL4 里没有对应物：时间片是用户态策略面的形状，内核态调度不需要它。

### 1.8 小结

队列（16 个定高低）讲形状，上限和当前位置（一个约束、一个状态）讲分工，时间片（毫秒，直接送内核）讲约定，nice 换算（41 级折进 16 个队列）讲翻译，系统进程判断（只看直接父进程）讲出身。贯穿始终的一条要求：所有 handler 用到的数字都出自这一篇的定义，定义只有一份。

---

## 2 C 源码分析

### 2.1 队列常量表（`config.h:66-77`）

队列总数（`66`：`NR_SCHED_QUEUES=16`，注释写着"必须等于最低优先级加一"）→ 最高队列（`67`：`TASK_Q=0`，给内核任务用）→ 用户最高队列（`68`：`MAX_USER_Q=0`，和最高队列同值）→ 默认队列（`69-70`：`USER_Q`，用上下限算出来的派生宏）→ 用户最低队列（`71-72`：`MIN_USER_Q=15`，等于队列总数减一）→ 默认配额（`74`：`USER_QUANTUM=200`）→ 默认 CPU（`77`：`USER_DEFAULT_CPU=-1`，意思是"用默认的或者保持不变"）。

### 2.2 时间片初值（`schedule.c:41`）

`DEFAULT_USER_TIME_SLICE=200`：新槽位的初始值（init 特殊处理和 START 分支都要用到它，06 的内容）。和 `USER_QUANTUM` 数值相同但来源不同（见 §1.4）。

### 2.3 系统进程判断宏（`schedule.c:44` + `com.h:61`）

`is_system_proc(p)`（`44`：`parent == RS_PROC_NR`）→ `RS_PROC_NR=2`（`com.h:61`：RS 服务的端点号）。

### 2.4 nice 换算函数（`pm/utility.c:91-101`）

先检查范围（`93`：超出范围返回 `EINVAL`，输出参数 `*new_q` 保持不动——拒绝了就不写结果）→ 套公式（`95-96`：线性折算，`MAX_USER_Q + (nice - PRIO_MIN) × (MIN_USER_Q - MAX_USER_Q + 1) / (PRIO_MAX - PRIO_MIN + 1)`）→ 死钳位（`99-100`：注释写着"理论上不会发生"，把结果钳在上下界里）→ 返回成功（`101`：`OK`）。范围的来源（`sys/resource.h:43-44`：`PRIO_MIN=-20`、`PRIO_MAX=20`）。

### 2.5 niced 标记和内核那边的检查（`schedule.c:319` + `system.c:648-683`）

`niced`（`319`：`max_priority > USER_Q`，跟着 `sys_schedule` 消息发给内核的备注）→ 内核三道检查：时间片检查（`648-650`：`quantum < 1` 就拒绝，`-1` 表示"保持不变"——保持语义是 09 的内容）→ CPU 检查（`652-655`：`-1` 以下全拒绝，只有 `-1` 表示保持）→ 落盘（`683-686`：`p_quantum_size_ms = quantum` 再加 `ms_2_cpu_time` 换算——毫秒约定的内核侧证据，S-6）。下发的实现归 09，本节只说明"每个数字都有对应的检查"。

---

## 3 Rust 设计决策

Rust 改写不是把七个宏和一个公式照抄过来，而是参考 Linux 的三分法和内核 Priority 类型的经验之后再取舍。决策编号 D1–D6，每条都给出 C 依据和 Rust 落点。

### D1 常量集中成表

- **C**：七个宏散在 `config.h:66-77` 和 `schedule.c:41` 两处（`USER_Q` 是个派生宏）。
- **Rust**：`TASK_Q/MAX_USER_Q/MIN_USER_Q/USER_QUANTUM/DEFAULT_USER_TIME_SLICE/USER_DEFAULT_CPU/PRIO_MIN/PRIO_MAX`（`os/servers/sched/src/priority.rs:26-74`）+ `USER_Q` 派生表达式（`priority.rs:44`，`(MIN - MAX) / 2 + MAX`，和 C 同形）+ `NR_SCHED_QUEUES` 从 03 转引（`priority.rs:23`，`pub use` 自 03——值只有一份，只是两条路都能拿到，不是两份定义）。
- **为什么**：定义只有一份（§1.1）；派生表达式保留原形（值跟着边界走，改队列数则默认值自动跟着变）；数值相同的两个常量分开命名（`USER_QUANTUM` 和 `DEFAULT_USER_TIME_SLICE` 各有各的位置，见 §1.4）。备选方案（把 `USER_Q` 写死成 7）被否决了：写死之后改队列数，默认值不会跟着变，约定就断了。

### D2 nice 值做成新类型

- **C**：`nice_to_priority(int nice, unsigned *new_q)`（`pm/utility.c:91`：输出参数 + `EINVAL` + 死钳位）。
- **Rust**：`Nice(i32)` + `new(i32) -> Option`（`priority.rs:128,133`，超出范围就返回 `None`）+ `to_priority() -> Option<Priority>`（`priority.rs:160`，纯公式，没有输出参数；死钳位省略了，公式后面附了证明）。
- **为什么**：范围检查收进构造函数（`Nice` 一定能换算，换算本身不会再失败）；C 的输出参数加 errno 改成 `Option`（`None` 在 PM 那边翻译成 `EINVAL`，那是 `04-stage-pm/16` 的事——错误在边界处理，不在公式里）；死钳位省略并给出证明（合法 nice 值一定落在范围内，证明写在注释里）。备选方案（直译成 `fn nice_to_priority(i32) -> Result<Priority, Errno>`）被否决了：换算和报错混在一起，换算结果就没法复用（调用者想"先检查范围、后换算"就没有入口了）。

### D3 系统进程判断做成谓词函数

- **C**：`is_system_proc(p)`（`schedule.c:44`：宏，要读整个槽位）。
- **Rust**：`is_system_proc(parent: Endpoint) -> bool`（`priority.rs:191`，只收父进程端点；`Endpoint::RS` 就是 `RS_PROC_NR=2`，types 里已经有了）。
- **为什么**：C 的宏读整个槽位只是顺手（手边正好有槽位）；谓词只收父端点是因为判断只需要这一个值，收整个槽位就是收多了。备选方案（宏直译，或者收整个槽位的引用）被否决了：判断和执行分离（检查只做判断，04 的 D2 有同样的例子）。

### D4 时间片只定名字，不管类型

- **C**：`unsigned time_slice`（单位毫秒，S-6）+ `USER_QUANTUM`/`DEFAULT_USER_TIME_SLICE`（同值双名）。
- **Rust**：`time_slice_ms: u32`（03 已经定了，本篇沿用）+ 两个常量（`priority.rs:52,61`）+ `is_valid_quantum(u32) -> bool`（`priority.rs:180`，`>= 1` 的判断；检查在 09，谓词在这里——规则只有一份）。
- **为什么**：名字里带单位（旧文档曾经误写成 ticks，名字不带单位以后还会错）；不定成 `Duration` 类型（03 的 D4 已经定了：线上的值就是毫秒数，再包一层反而增加拆装成本）；存和验分离（SCHED 只管存不管验，验是内核的事——谓词放在这里供 09 在下发前自查，各得其所）。备选方案（新建 `TimeSliceMs` 类型）被否决了：03 已经定了 `u32` 字段，两种类型并存就是两份定义。

### D5 哨兵值做成枚举

- **C**：`USER_DEFAULT_CPU=-1`（`config.h:77`：`-1` 表示默认，注释还写着"或者不变"）。
- **Rust**：`CpuChoice::{Default, Cpu(u32)}`（`priority.rs:86`）+ `from_raw(i32) -> Option`（`priority.rs:101`，`-1` 认作默认，`>= 0` 认作 CPU 号，其他全返回 `None`）+ `USER_DEFAULT_CPU` 保留原始值（`priority.rs:68`，读原始消息的人用）。
- **为什么**：哨兵值收进类型（`-1` 不是 CPU 号，`u32` 又表示非负——硬塞进一个 `i32` 就等于把"负数是什么意思"这个问题藏起来了）；`from_raw` 对应内核的 CPU 检查（`system.c:652`，`-1` 以下全拒绝——检查在 09，形状在这里）。备选方案（直接用 `Option<u32>`）被否决了：`None` 说不清是"用默认"还是"保持不变"（C 注释里有两个意思：默认**或**不变——枚举以后可以加 `Keep` 变体，`Option` 加不了，形状上留了余地）。

### D6 niced 标记跟着上限走

- **C**：`niced = (rmp->max_priority > USER_Q)`（`schedule.c:319`：跟着 `sys_schedule` 发下去的备注）。
- **Rust**：`is_niced(max_priority: Priority) -> bool`（`priority.rs:202`，`get() > USER_Q`）。
- **为什么**：公式只读上限（当前位置上下浮动不影响它——降到底但上限还在中间队列以上的，标记照样打）；谓词放在定义这边（下发在 09，公式在这里——一份定义两处用）。备选方案（09 里面内联这个表达式）被否决了：公式散开就是两处各写一遍，以后改了一处忘了另一处；这个标记不是 ARCH（纯复述，没有行为变化）。

### ARCH 决策总表

| ARCH | 落点 | 三处一致标注 |
|------|------|-------------|
| S-6 时间片单位（毫秒约定） | 双名常量 + `is_valid_quantum` + `time_slice_ms`（03 已定） | `priority.rs:52,61,180` + 本文档 D4 + plan §7.3/S-6 行 |
| S-4 CPU 表达（哨兵值类型化，选 CPU 那边） | `CpuChoice` + `from_raw` | `priority.rs:86,101` + 本文档 D5 + 10 用之（负载计数归 10） |
| S-2 优先级类型 | 复用 03 的 `Priority`（本篇不另起） | `priority.rs:160,202` + 03 D3（同形异处，用了就不重复） |

---

## 4 实现详解

### 4.1 模块结构

```
os/servers/sched/src/
├── priority.rs           — 本篇：队列常量表、nice 换算、时间片命名、系统进程判断、CPU 选择
├── schedproc.rs          — 结构体（03，Priority 与 NR_SCHED_QUEUES 的源头）
├── table.rs / valid.rs   — 表检查（04，EINVAL/EBADEPT/EDEADEPT 错误值的源头）
└── lib.rs                — 模块导出
os/libs/minix-types/src/types/
├── endpoint.rs           — Endpoint::RS（系统进程判断用的端点号，com.h:61）
└── errno.rs              — EINVAL（超出范围的错误，PM 那边回答）
```

> 设计决策：§3 D1（常量集中成表）/ D2（nice 值新类型）/ D5（哨兵值枚举）。

### 4.2 核心符号表

| 符号 | 来源 | Rust 位置 | 行为 |
|------|------|-----------|------|
| 队列常量表 | `config.h:66-77` | `priority.rs:23-68` | 七个名字加一个派生表达式 |
| 时间片命名 | `schedule.c:41` + `config.h:74` | `priority.rs:52,61,180` | 同值双名加检查谓词 |
| nice 换算 | `pm/utility.c:91-101` | `priority.rs:128,133,160` | 范围收进构造，换算不返回错误 |
| 系统进程判断 | `schedule.c:44` | `priority.rs:191` | 只看直接父进程 |
| CPU 选择 | `config.h:77` | `priority.rs:86,101,114` | 哨兵值收进类型 |
| niced 标记 | `schedule.c:319` | `priority.rs:202` | 上限超过中间队列就打标 |

### 4.3 不变量

| 不变量 | 位置 | 守卫 | 证据 |
|--------|------|------|------|
| 队列边界锁死（最低 = 总数 - 1） | `MIN_USER_Q` | 派生不写死 | `config.h:66,71` |
| 默认值居中（7 是算出来的） | `USER_Q` | 同形派生表达式 | `config.h:69` |
| 同值双名分来源（都是 200 但来源不同） | 两个常量 | 名字即来源 | `config.h:74` vs `schedule.c:41` |
| 换算结果恒在范围内（41 级折进 16 队不出界） | `to_priority` | 公式后附证明 + 防御性 `None` | `pm/utility.c:95-96` |
| 系统进程只看一代（孙子不算） | `is_system_proc` | 只读父端点 | `schedule.c:44` |
| 哨兵值无负数（`-1` 以下全拒绝） | `from_raw` | `None` 即拒绝 | `system.c:652` |

---

## 5 测试要点

| 测试名 | 覆盖 C 行号 | 行为 | 文件 |
|--------|-------------|------|------|
| `test_queue_table` | `config.h:66-72` | 七个名字全锁定 + 派生表达式自证 + 最高队列同值 | `priority.rs:211` |
| `test_quantum_defaults` | `config.h:74`、`schedule.c:41`、`system.c:648` | 同值双名 + 检查谓词 0 拒绝 1 通过 | `priority.rs:226` |
| `test_nice_bounds` | `resource.h:43-44`、`pm/utility.c:93` | 范围内接收 + 范围外拒绝 | `priority.rs:238` |
| `test_nice_mapping` | `pm/utility.c:95-96` | 低中高三点锁定 + 相邻共享 + nice 0 落中间 | `priority.rs:249` |
| `test_system_proc` | `schedule.c:44`、`com.h:61` | RS 的儿子真 + 其他全假（只看一代） | `priority.rs:266` |
| `test_cpu_choice` | `config.h:77`、`system.c:652` | `-1` 认默认 + 非负认 CPU + 更小的负数拒绝 | `priority.rs:279` |
| `test_niced_flag` | `schedule.c:319` | 中间及以下不清 + 超过中间打标 | `priority.rs:292` |

测试策略：常量表用全量断言加派生自证锁定（值和公式双保险）；换算用低中高三点加共享例子锁定；边界用临界值（0 拒绝、1 通过、`-2` 拒绝）锁定；系统进程用 RS 真加 PM/INIT/SCHED/随机号假锁定；标记用中间值边界（7 不清、8 打标）锁定。

### 5.1 测试统计

基线以 `cargo test -p minix-sched --lib` 实际输出为准（改写时本地为 59 passed，见全仓回归报告）。

- 本节列出与本模块直接相关的 7 个（子集）
- 完整测试清单：`rg "fn test_" os/servers/sched/src/priority.rs`

---

## 6 过渡

本篇在 04（进程表）之后、06（开始调度）之前，是"公共参数"的归属：03 定结构体形状，04 定表检查规则，本篇定参数含义；没有本篇，06 的边界检查不知道界从哪来，08 的降级恢复不知道上下限在哪，09 的下发不知道时间片是什么单位。

```
04-schedproc-table: 槽位计算 → 三道检查 → 白名单（表检查规则）
   │
   └─► 本篇：队列常量表 → 上限/当前位置 → 时间片约定 → nice 换算 → 系统进程判断（公共参数定义）
           │                              │
           ├─► 06-start-scheduling：边界检查（参数的去向）
           ├─► 08-noquantum-nice：降级恢复与调整（上限/当前位置的去向）
           └─► 09-schedule-process：下发（时间片检查的去向）
```

阅读顺序提示：关心"这些参数用在哪"，下一站 `06-start-scheduling.md`（边界检查与登记流程）；降级调整的细节见 `08-noquantum-nice.md`，下发的细节见 `09-schedule-process.md`。

---

## 7 参见

- C 源：`minix3/minix/include/minix/config.h:66-77`（队列常量全家）、`minix3/minix/servers/sched/schedule.c:41,44,319`（时间片初值、系统进程判断、niced 标记）、`minix3/minix/servers/pm/utility.c:91-101`（nice 换算）、`minix3/minix/kernel/system.c:648-683`（时间片检查、CPU 检查、落盘）、`minix3/sys/sys/resource.h:43-44`（nice 范围）、`minix3/minix/include/minix/com.h:61`（RS 端点号）
- 阶段文档：`03-schedproc-struct.md`（结构体形状）、`04-schedproc-table.md`（上一站）、`06-start-scheduling.md`（下一站）、`08-noquantum-nice.md`（降级调整）、`09-schedule-process.md`（下发）、`10-pick-cpu-smp.md`（选 CPU）、`11-balance-queues.md`（优先级恢复）
- Rust 实现：`os/servers/sched/src/priority.rs:1`（本篇参数源头）、`os/servers/sched/src/schedproc.rs:46`（`Priority` 同形）、`os/libs/minix-types/src/types/endpoint.rs`（`Endpoint::RS` 权威定义）
- 对端：`../01-stage-kernel/11-scheduling-primitives.md`（§3.8 时间片下发对端）、`../04-stage-pm/16-scheduling.md`（nice 用户接口对端）
