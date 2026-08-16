# 06-stage-sched 文档重组计划（plan.md）

> **状态**: 生效中（2026-08-16 首版，深度 review + minix3 源码回归 review 后定稿）
> **范围**: `notes/rewrite/fork-syscall-rewrite/06-stage-sched/`
> **目标**: 以 **SCHED server 启动顺序为主线**重组 sched 全部文档；fork 调度继承降为次主线；最终覆盖 Minix3 sched server 全部语义（含内核契约与 PM/RS 客户端契约），支撑 SCHED server 的彻底 Rust 重写
> **对照**: `01-stage-kernel/`（讲述结构参照）、`04-stage-pm/` 与 `03-stage-rs/`（客户端契约）、`minix3/minix/servers/sched/`（ground truth）、`os/servers/sched/`（Rust 实现，当前为空骨架）

---

## 1. 背景与动机

### 1.1 旧主线（fork 主线）的问题

旧文档（已移入 `draft/`）以 fork 为主线：00 总览从 fork 视角出发，01~03 沿 `SCHEDULING_INHERIT` 执行路径引入概念，04 把"优先级、时间片、队列平衡、CPU 管理、do_stop、do_nice"六个独立语义压成一篇。实践发现四类问题：

1. **覆盖缺口**——draft 没有 `main.c` 专属文档：SEF 生命周期（`sef_local_startup`/`sef_cb_init_fresh`/`init_restart STATEFUL`）、主循环、消息分发、`SUSPEND` 契约、`reply` 语义完全缺失；读者先学服务、后才知道服务如何被分派。
2. **主线倒置**——`do_start_scheduling` 的双消息入口（`SCHEDULING_START` + `SCHEDULING_INHERIT`）被 draft 写成只讲 INHERIT 分支，而 `SCHEDULING_START`（RS 启动系统进程的路径）被边缘化；`do_noquantum`/`balance_queues`/`do_nice`/`do_stop_scheduling` 这些**稳态运行语义**（sched 大部分工作时间）被塞进 04 一篇。
3. **契约边界模糊**——sched 是"双层调度模型"的用户态策略面，其语义一半在服务端（`servers/sched/`），一半在接口面（`sys_schedctl`/`sys_schedule` 内核侧、`libsys` 客户端侧、PM/RS 调用面）。draft 只写服务端，内核契约（`p_scheduler` 注册语义、`SCHEDULING_NO_QUANTUM` 的 7 个 accounting 字段）与客户端契约（`sched_inherit`/`sched_start`/`sched_stop`/`sched_nice`、`mp_scheduler`、`r_scheduler`）全部缺失。
4. **事实错误**——draft/04 声称 time_slice 单位是"系统时钟 ticks"，实际 SCHED 的 `time_slice` 直接对接内核 `p_quantum_size_ms`（毫秒，`sched_proc` 中 `p->p_quantum_size_ms = quantum`）；draft/01 声称 fork 时"设置 cpu_mask"，实际 C 全代码库从不写 `cpu_mask[]`（schedule.c:185 仅留 `FIXME set the cpu mask`）。

### 1.2 新主线：SCHED server 启动顺序

与 `01-stage-kernel`/`02-stage-vm`/`04-stage-pm` 相同，采用**读者学习顺序 = 系统实际执行顺序**的组织原则。SCHED 是 RS 启动的系统服务，其生命周期是一条线性链：

```
RS 启动 sched（boot image 系统服务）
  │
  ▼  main() (main.c:22)
  └─ sef_local_startup() (main.c:111)          ← 阶段 1~2 文档的锚点
       ├─ sef_setcb_init_fresh(sef_cb_init_fresh)
       ├─ sef_setcb_init_restart(SEF_CB_INIT_RESTART_STATEFUL)
       └─ sef_startup()
            │
            ▼ sef_cb_init_fresh (main.c:126)
            ├─ sys_getmachine(&machine)         ← 01：机器信息（processors_count/bsp_id，10 用）
            └─ init_scheduling()                ← 11：balance 定时器（BALANCE_TIMEOUT×sys_hz，sys_setalarm）
  │
  ▼  主循环 (main.c:52-98) — 运行时
  ├─ sef_receive_status(ANY, &m_in, &ipc_status)  ← 02：收消息
  ├─ is_ipc_notify(ipc_status)                   ← 02：通知分类
  │     └─ CLOCK → balance_queues()              ← 11：队列平衡（处理完重设 alarm）
  ├─ switch(call_nr) 分发                          ← 02：消息面
  │     ├─ SCHEDULING_INHERIT / SCHEDULING_START → do_start_scheduling  ← 06：调度接管（fork 次主线）
  │     ├─ SCHEDULING_STOP          → do_stop_scheduling   ← 07：停止调度（exit 路径）
  │     ├─ SCHEDULING_SET_NICE      → do_nice              ← 08：nice 修改
  │     ├─ SCHEDULING_NO_QUANTUM    → do_noquantum         ← 08：时间片耗尽（FROM_KERNEL 校验）
  │     └─ default                  → no_sys
  └─ reply()（result != SUSPEND）                 ← 02：回复语义
```

**每篇文档必须能回答一个问题：它位于 SCHED 启动时序（sef_local_startup）或主循环（dispatch）的哪个位置。** 这是从 `01-stage-kernel/00-kernel-overview.md §3` 继承的组织原则。

### 1.3 fork 次主线

fork 不再充当"概念引入的驱动"，而是**阶段 4 服务之一**（`do_start_scheduling` 的 INHERIT 分支），其路径图在 `06-start-scheduling` 内部绘制：

```
PM do_fork → VFS 回复后 → sched_start_user() (pm/schedule.c:55)
  │
  ├─ nice_to_priority(mp_nice) → maxprio        ← 05：nice→优先级（跨 04-stage-pm/16）
  ├─ inherit_from 判定（PRIV_PROC 父 → INIT_PROC_NR 特例）← 13
  └─ sched_inherit(SCHED_PROC_NR, child, parent, maxprio)  ← 13：libsys 接口
       │  SCHEDULING_INHERIT 消息 → 主循环 dispatch（02）→ do_start_scheduling
       ├─ 04 表管理：sched_isemtyendpt(child) / sched_isokendpt(parent)
       ├─ 03 结构体：slot 初始化（endpoint/parent/max_priority）
       ├─ 06 INHERIT 分支：priority/time_slice 从父继承
       ├─ 09 参数下发：sys_schedctl(接管) + schedule_process(SCHEDULE_CHANGE_ALL)
       ├─ 10 CPU 选择：pick_cpu（负载均衡 / 系统进程 BSP）
       └─ 回复 scheduler=SCHED_PROC_NR → mp_scheduler（13）
```

---

## 2. 新文档编号与阶段划分

> 编号规则与 `01-stage-kernel`/`02-stage-vm` 一致：`NN-短横线语义名.md`；`00-` 为总览；`99-` 为全局概念。原 `draft/` 文档保留旧编号（作为素材），新编号在顶层重新建立。

### 阶段总览

| 阶段 | 编号 | 文档 | 语义模块 | C 源码 | Rust 模块 | draft 来源 | 变更 |
|------|------|------|---------|--------|-----------|-----------|------|
| 0 总览 | 00 | `00-sched-overview.md` | SCHED 是什么（用户态调度器）、双层调度模型、启动主线图、文档导航 | `servers/sched/` 全部 | 全部 | `draft/00` | **重写**导航（§2 改为启动主线叙事） |
| 1 启动入口与消息面 | 01 | `01-sched-init-main.md` | `main`/`sef_local_startup`/`sef_cb_init_fresh`/`sys_getmachine`/`init_scheduling`（调用点）/主循环骨架 | `main.c:22-98,111-137` | `main.rs`、`sef.rs` | —（新增） | **新增**（draft 无 main 专属文档） |
| 1 | 02 | `02-sched-message-surface.md` | 5 种消息类型表、`sef_receive_status`、`is_ipc_notify`、dispatch switch、`SUSPEND` 伪返回码、`reply` 语义、`no_sys` | `main.c:52-98`、`utility.c:no_sys` | `dispatch.rs` | —（新增） | **新增** |
| 2 数据结构与表管理 | 03 | `03-schedproc-struct.md` | `schedproc` 结构全字段、`IN_USE`、`cpu_mask`（死字段，S-3） | `schedproc.h` | `schedproc.rs` | `draft/01` | 沿用 + S-3 决策 |
| 2 | 04 | `04-schedproc-table.md` | `schedproc[NR_PROCS]` 静态表、slot 提取（`_ENDPOINT_P`）、`sched_isokendpt`/`sched_isemtyendpt`、`accept_message`（PM/RS 白名单）、错误码表 | `schedproc.h`、`utility.c:29-72` | `table.rs`、`valid.rs` | `draft/01` 部分 | **拆分**（验证逻辑独立成篇） |
| 3 调度模型 | 05 | `05-priority-timeslice-model.md` | `NR_SCHED_QUEUES`/`TASK_Q`/`MAX_USER_Q`/`USER_Q`/`MIN_USER_Q`、`max_priority` vs `priority`、time_slice 单位（ms，修正旧文）、`DEFAULT_USER_TIME_SLICE`/`USER_QUANTUM`、nice 映射、`is_system_proc` | `config.h:66-77`、`schedule.c:41,44`、`pm/utility.c:nice_to_priority` | `priority.rs` | `draft/04` 部分 | **拆分**（概念前置） |
| 4 核心服务 | 06 | `06-start-scheduling.md` | `do_start_scheduling` 全流程（双消息断言/`accept_message`/slot 填充/`endpoint==parent` init 特例/START 分支/INHERIT 分支/`sys_schedctl` 接管/`pick_cpu`/`schedule_process`+EBADCPU 重试/回复 scheduler）、**fork 次主线路径图** | `schedule.c:140-252` | `scheduling/start.rs` | `draft/02`+`draft/03` | **合并**（INHERIT+START 统一） |
| 4 | 07 | `07-stop-scheduling.md` | `do_stop_scheduling`（`accept_message`/验证/`cpu_proc` 递减/`flags=0`）、与 start 的对称性、`sched_stop` 客户端短路语义 | `schedule.c:112-137` | `scheduling/stop.rs` | `draft/04` 部分 | **拆分** |
| 4 | 08 | `08-noquantum-nice.md` | `do_noquantum`（FROM_KERNEL 信任模型、`priority<MIN_USER_Q` 降级、`schedule_process_local`）、`do_nice`（校验/更新/失败回滚） | `schedule.c:87-109,254-295` | `scheduling/noquantum.rs`、`scheduling/nice.rs` | `draft/04` 部分 | **拆分** |
| 5 调度机制 | 09 | `09-schedule-process.md` | `schedule_process` 参数聚合（`SCHEDULE_CHANGE_*`/`-1` 保持语义/`niced`）、`sys_schedule` 消息格式、内核 `do_schedule`（caller==p_scheduler）、`sched_proc` 参数应用（EINVAL/EBADCPU/RTS_NO_QUANTUM/MF_NICED） | `schedule.c:297-332`、`kernel/system/do_schedule.c`、`system.c:642-723` | `kernel_api/schedule.rs` | —（新增） | **新增**（draft 仅提及） |
| 5 | 10 | `10-pick-cpu-smp.md` | `pick_cpu`（单核特例/系统进程→BSP/负载最低）、`cpu_proc[]`、`cpu_is_available`、`CPU_DEAD`、`CONFIG_SMP`/`CONFIG_MAX_CPUS`（S-5 编译期→运行时）、`machine.processors_count/bsp_id` | `schedule.c:48-78` | `cpu.rs` | `draft/04` 部分 | **拆分** |
| 5 | 11 | `11-balance-queues.md` | `init_scheduling`（`balance_timeout=BALANCE_TIMEOUT×sys_hz`）、`sys_setalarm`、`balance_queues`（IN_USE 扫描/优先级恢复一级/重设 alarm）、CLOCK notify 链路 | `schedule.c:334-369`、`main.c:66-72` | `balancer.rs` | `draft/04` 部分 | **拆分** |
| 6 内核契约 | 12 | `12-kernel-interface.md` | 双向契约：`sys_schedctl`/`do_schedctl`（`SCHEDCTL_FLAG_KERNEL`、`p_scheduler` 注册）、`notify_scheduler`（`SCHEDULING_NO_QUANTUM` 构造、accounting 7 字段、`FROM_KERNEL`、`RTS_NO_QUANTUM` 出队、`reset_proc_accounting`）、`proc_no_time` 双分支（PREEMPTIBLE）、`mini_send` | `kernel/system/do_schedctl.c`、`kernel/proc.c:1860-1910` | 内核侧已实现（`01-stage-kernel/11` §4.5/§3.8） | —（新增） | **新增**（契约汇总，与 11-scheduling-primitives 分工声明） |
| 7 客户端接口 | 13 | `13-pm-interaction.md` | libsys `sched_inherit`/`sched_start`/`sched_stop`（NONE/KERNEL 短路）、PM 调用面 `sched_init`/`sched_start_user`/`sched_nice`/`sched_stop`（exit）、`mp_scheduler`、INIT_PROC_NR 特例、`USER_Q`/`USER_QUANTUM` | `lib/libsys/sched_start.c`、`sched_stop.c`、`pm/schedule.c`、`pm/forkexit.c` | pm 客户端面（未实现） | `draft/02` 部分 | **拆分**（与 04-stage-pm/16 分工） |
| 7 | 14 | `14-rs-interaction.md` | RS 侧 `sched_init_proc`、`r_scheduler`/`r_priority`/`r_quantum`/`r_cpu`、系统进程接管（parent=RS_PROC_NR）、RS `sched_stop` 终止路径、`sched_start` 的 KERNEL/NONE 短路 | `rs/utility.c:364-384`、`rs/manager.c:461`、`rs/request.c:342` | `rs/sched.rs` 相关（03-stage-rs） | —（新增） | **新增** |
| 99 全局概念 | 99 | `99-global-concepts.md` | 双层调度模型、五进程表一致性、优先级常量表、endpoint 语义、错误码表、SCHED 与 Kernel/PM/RS 职责边界 | `com.h`、`config.h`、`type.h` | `minix-types` | `draft/99` | 沿用 |

### 2.1 阶段间的叙事衔接

每个阶段结尾的文档需含"过渡"小节（参照 `01-stage-kernel` 每篇 §6"过渡"），说明本阶段在启动时序中的位置与下一阶段的入口：

```
01（启动骨架）→ 02（消息面）→ 03/04（数据结构+表）→ 05（调度模型）
→ 06/07/08（核心服务）→ 09/10/11（调度机制）→ 12（内核契约）
→ 13/14（客户端契约）→ 99（全局概念）
```

---

## 3. 讲述结构规范（参照 01-stage-kernel）

### 3.1 每篇文档的章节模板

与 `01-stage-kernel` 一致（见 `11-scheduling-primitives.md` 等）：

1. **概念**——为什么需要这个机制、类比、边界声明（前置依赖/本篇不覆盖什么）
2. **C 源码分析**——ground truth，必须 grep 实证，禁止凭记忆
3. **Rust 设计决策**——类型系统建模、与 C 的差异（含 ARCH 标注）
4. **实现详解**——Rust 模块结构、关键不变量
5. **测试要点**——相关测试函数 + 测试总数声明
6. **过渡**——在启动时序/主循环中的位置，下一篇入口
7. **参见**——跨文档引用（新编号）

### 3.2 组织原则（继承 01-stage-kernel §3）

1. **概念首次出现即完整解释**——后续只引用不复述（优先级模型在 05 首次完整解释，09/10/11 只引用）
2. **禁止前向引用**——依赖机制前置（本计划的编号已保证：调度模型 05 在服务 06~08 之前，内核契约 12 在客户端 13/14 之前）
3. **每篇一个语义单元**——读者可独立阅读（draft/04 的六合一必须拆开）
4. **位置可回答性**——每篇回答"它在 sef_local_startup / 主循环的哪个位置"

### 3.3 引用规则

- 各文档之间用新编号交叉引用（如 `06-start-scheduling.md` §fork 次主线）
- 内核侧引用 `../01-stage-kernel/11-scheduling-primitives.md`（内核调度原语/`sched_proc`/`notify_scheduler` 实现细节，本文档不重复）
- 客户端侧引用 `../04-stage-pm/16-scheduling.md`（PM 侧调度）与 `../03-stage-rs/`（RS 侧，如 `08-rs-slot-config.md`）
- 对 draft 素材的引用一律指向 `draft/NN-*.md`，并标注"素材"；正式文档绝不引用 review 产物

### 3.4 每篇文档边界声明（执行级）

> 写作每篇文档前，先按本表声明**前置依赖 / 本篇职责 / 不覆盖（移交）**，防止内容交叉与遗漏。边界以 §5.3 函数清单为准绳。

| 编号 | 前置依赖 | 本篇职责（核心语义） | 不覆盖（移交） |
|------|---------|---------------------|--------------|
| 00 | 无 | 总览、启动主线图、文档导航、双层调度模型 | 一切机制细节 |
| 01 | 00 | `main`/`sef_local_startup`/`sef_cb_init_fresh`/`sys_getmachine`/`init_scheduling`（调用点）/主循环骨架 | 消息分发细节（02）、各 handler（06~08） |
| 02 | 01 | 5 种消息类型表、`sef_receive_status`、`is_ipc_notify`、dispatch switch、`SUSPEND`、`reply`、`no_sys` | 各 handler 实现（06~08） |
| 03 | 00 | `schedproc` 结构全字段、`IN_USE`、`cpu_mask` 死字段决策（S-3） | 表管理/endpoint 验证（04） |
| 04 | 03 | `schedproc[NR_PROCS]` 表、slot 提取、`sched_isokendpt`/`sched_isemtyendpt`、`accept_message`、错误码语义 | 字段细节（03） |
| 05 | 04 | 优先级常量体系（config.h:66-76）、`max_priority` vs `priority`、time_slice 单位、`is_system_proc`、nice 映射概念 | 各 handler 中的具体使用（06~08）、内核侧优先级校验（09） |
| 06 | 02/04/05 | `do_start_scheduling` 全流程、fork 次主线路径图 | START/INHERIT 消息格式细节（02/13）、`schedule_process` 内部（09）、`pick_cpu` 内部（10） |
| 07 | 02/04 | `do_stop_scheduling`、与 start 对称性 | 客户端 `sched_stop` 调用面（13） |
| 08 | 02/04/05 | `do_noquantum`（FROM_KERNEL 信任模型）、`do_nice`（回滚） | `schedule_process` 内部（09）、accounting 字段语义（12） |
| 09 | 05/06 | `schedule_process` 参数聚合、`sys_schedule` 消息、`do_schedule`（caller==p_scheduler）、`sched_proc` 参数应用 | 调度器注册语义（12）、CPU 选择策略（10） |
| 10 | 01/05 | `pick_cpu`、`cpu_proc[]`、`CPU_DEAD`、SMP 编译期→运行时（S-5） | 内核 SMP 迁移细节（01-stage-kernel/16） |
| 11 | 02/05 | `init_scheduling`、`sys_setalarm`、`balance_queues`、CLOCK notify 链路、策略可替换（S-9） | 时钟中断内核侧（01-stage-kernel/15） |
| 12 | 09 | `do_schedctl`（`p_scheduler` 注册）、`notify_scheduler`（accounting 7 字段）、`proc_no_time` 双分支 | 内核调度原语内部（01-stage-kernel/11） |
| 13 | 02/04/05 | libsys 3 接口、PM 调用面（`sched_init`/`sched_start_user`/`sched_nice`/`sched_stop` exit）、`mp_scheduler`、INIT 特例 | PM nice 系统调用用户面（04-stage-pm/16） |
| 14 | 02/04/05 | `sched_init_proc`、`r_scheduler`/`r_priority`/`r_quantum`/`r_cpu`、系统进程接管、RS `sched_stop` 终止路径 | RS 槽配置细节（03-stage-rs/08） |
| 99 | 无 | 双层调度模型、五进程表一致性、常量表、错误码表 | 一切机制 |

### 3.5 测试基线（截至 2026-08-16）

- 内核侧调度原语（`os/kernel/src/sched.rs`）：**23 个测试**（enqueue/dequeue/pick_proc/proc_no_time/sched_proc 全参数+C parity，见 `01-stage-kernel/11-scheduling-primitives.md §5`）
- `minix-sched` crate（`os/servers/sched/`）：当前为空骨架（`lib.rs` 仅 `pub fn init(){}`，`main.rs` 仅 `loop{}`），**0 测试**——本文档各篇改写完成后在此补充
- `minix-types`：`SCHEDULING_NO_QUANTUM` 常量与内核侧消息结构体（`MessLsysKrnSchedctl`/`MessLsysKrnSchedule`/`MessKrnLsysSchedule`）已实现；**缺口**：`SCHEDULING_START/STOP/SET_NICE/INHERIT` 常量与 4 个 sched 服务端消息结构体缺失（见 §6.2 前置项）
- 每篇改写完成时在文末更新该模块测试统计（review-doc-skill §2.4j）

### 3.6 Review gate 要求（每篇改写必检）

- 每篇新文档创建时同步生成 `.design/{NN}-outline.md` + `{NN}-outline-review.md` + `{NN}-design.md`（Step 0.3 嵌入生成，Gate H.6，不允许 N/A）
- 每篇改写后按 review 工作流跑 Blocker Gates（0/A/B/C/D/D-6/E/G/H），scan 产物写入 `.review/codex/sched/{NN}-{name}/`
- P0 未清不得标完成；doc 与 code 保持同步

---

## 4. 架构演进（ARCH）清单

> 按 review-core 要求，ARCH 必须三处一致标注：Minix3 行为对照点 + design doc + 代码注释。以下为 06-stage-sched 范围内的 ARCH 项，写文档时必须逐项落实。

| # | ARCH 项 | Minix3 现状 | minix-rs 演进 | 涉及新文档 | 状态 |
|---|---------|------------|--------------|-----------|------|
| S-1 | **双层调度模型类型化** | `p_scheduler` 为内核侧裸指针（`struct proc *`），SCHED 通过 `sys_schedctl(flags=0)` 把自己注册为调度器（do_schedctl.c:44-47）；`SCHEDCTL_FLAG_KERNEL` 走内核调度 | `SchedulerRef`/`Option<Endpoint>` 类型化，内核侧区分 `KernelScheduler`/`UserScheduler` 两态；注册语义不变 | 09/12 | 设计差异 |
| S-2 | 优先级类型 | 裸 `unsigned`（schedproc.h:22-26），范围 0..`NR_SCHED_QUEUES`-1 | `Priority` newtype（u8），构造时校验范围（对齐 kernel `11-scheduling-primitives.md §3.3`） | 03/05/09 | 设计差异 |
| S-3 | **`cpu_mask[]` 死字段** | schedproc.h:33 声明 `cpu_mask[BITMAP_CHUNKS(CONFIG_MAX_CPUS)]`，全代码库从不写入（schedule.c:185 仅 `FIXME set the cpu mask`） | 建议**结构消除**（CPU 亲和性 minix3 无消息面支撑）；若实现需新增 IPC 面 → 写作前置决策（§7.3） | 03/10 | **待决策** |
| S-4 | CPU 负载追踪 | `static unsigned cpu_proc[CONFIG_MAX_CPUS]` + `CPU_DEAD=-1` 哨兵（schedule.c:44-51） | `[Option<u32>; N]` 或 `LoadCount` newtype，消除 -1 哨兵；`cpu_is_available` 类型化 | 10 | 设计差异 |
| S-5 | SMP 编译期开关 | `#ifdef CONFIG_SMP`/`CONFIG_MAX_CPUS` 编译期（schedproc.h:17-19、schedule.c:53-78）；运行期 `machine.processors_count/bsp_id` | 运行时检测 + 架构 trait（对齐 01-stage-kernel/16-smp） | 01/10 | 已实现（trait 抽象） |
| S-6 | **时间片单位** | SCHED `time_slice` 直接对接内核 `p_quantum_size_ms`（ms）；draft 旧文档误称 "ticks" | 统一 `Duration`/ms newtype；内核 `ms_2_cpu_time` 转换边界（内核侧已实现） | 05/09/12 | 设计差异（修正旧文档错误） |
| S-7 | SEF 生命周期 | `sef_setcb_init_fresh` + `sef_setcb_init_restart(STATEFUL)`（main.c:111-125） | SEF trait/回调表（对齐 03-stage-rs A-7） | 01 | 已实现（trait 抽象） |
| S-8 | 消息传递模型 | 客户端 `_taskcall()` 同步阻塞（sched_start.c:38），服务端 `sef_receive_status` + `ipc_send` 异步回复（main.c:94-98,101-109） | 单线程事件循环 + `sendrec`/`notify`；`SUSPEND` 伪返回码保留（对齐 04-stage-pm A-3） | 02/13 | 设计差异 |
| S-9 | balance_queues 策略 | C 注释 "This default policy will soon be changed"（schedule.c:355）；默认策略：noquantum 降一级、balance 恢复一级 | `QueueBalancer` trait 或显式策略结构，默认策略等价实现并标注演进点 | 11 | 设计差异 |
| S-10 | 64 位宽度 | `unsigned`（32 位）、`NR_PROCS`、`BITMAP_CHUNKS(CONFIG_MAX_CPUS)` | u32/u64 显式、`NR_PROCS` 常量类型化（对齐 01-stage-kernel 64 位审计） | 03/05/10/99 | 已实现 |

---

## 5. 覆盖完整性核对（对照 minix3 源码）

> 本节是 plan.md 的语义覆盖契约。所有断言以 grep 实证为准（review-core：禁止凭记忆）。逐项核对见 §7。

### 5.1 C 源文件 → 新文档映射

> 与 VM 不同，SCHED 的语义边界跨 `servers/sched/`、`lib/libsys/`、`servers/pm/`、`servers/rs/`、`kernel/` 五个目录——sched 是"双层调度模型"的用户态策略面，其契约必须包含对端。表中非 `servers/sched/` 文件标注对端角色。

| C 文件 | 行数 | 新文档 | 对端角色 | 核对 |
|--------|------|--------|---------|------|
| `servers/sched/main.c` | 137 | 01/02 | 服务端 | 已核对 |
| `servers/sched/schedule.c` | 369 | 06/07/08/09/10/11（常量入 05） | 服务端 | 已核对 |
| `servers/sched/utility.c` | 74 | 02/04 | 服务端 | 已核对 |
| `lib/libsys/sched_start.c` | 76 | 13 | PM/RS 客户端接口 | 已核对 |
| `lib/libsys/sched_stop.c` | 29 | 13 | PM/RS 客户端接口 | 已核对 |
| `servers/pm/schedule.c` | 94 | 13（与 04-stage-pm/16 分工） | PM 调用面 | 已核对 |
| `servers/pm/forkexit.c`（`sched_stop` 调用点） | 12 | 13 | PM 调用面 | 已核对 |
| `servers/rs/utility.c`（`sched_init_proc`） | 21 | 14（与 03-stage-rs 分工） | RS 调用面 | 已核对 |
| `servers/rs/manager.c`、`request.c`（`sched_stop` 调用点） | 2 处 | 14 | RS 调用面 | 已核对 |
| `kernel/system/do_schedctl.c` | 49 | 12（与 01-stage-kernel/11 分工） | 内核契约 | 已核对 |
| `kernel/system/do_schedule.c` | 31 | 09/12 | 内核契约 | 已核对 |
| `kernel/system.c`（`sched_proc`） | 82 | 09/12（11-scheduling-primitives §3.8 已覆盖实现） | 内核契约 | 已核对 |
| `kernel/proc.c`（`notify_scheduler`/`proc_no_time`） | 51 | 12（11-scheduling-primitives §4.5 已覆盖实现） | 内核契约 | 已核对 |

### 5.2 头文件覆盖

| 头文件 | 归属 | 核对 |
|--------|------|------|
| `schedproc.h` | 03/04 | 已核对 |
| `sched.h`、`proto.h` | 00/99、分布（函数签名来源） | 已核对 |
| `include/minix/com.h`（`SCHEDULING_BASE`/5 消息、`SCHEDCTL_FLAG_KERNEL`） | 02/99 | 已核对 |
| `include/minix/ipc.h`（7 个 `mess_*` 结构体） | 02/09/12/13 | 已核对 |
| `include/minix/config.h`（优先级常量、`USER_QUANTUM`、`USER_DEFAULT_CPU`） | 05/99 | 已核对 |
| `include/minix/sched.h`（libsys 3 接口声明） | 13 | 已核对 |
| `include/minix/syslib.h`（`sys_schedctl`/`sys_schedule` 声明） | 09/12 | 已核对 |
| `include/minix/sysutil.h`（`sys_setalarm`/`sys_hz`/`sys_getmachine`） | 01/11 | 已核对 |
| `include/minix/type.h`（`machine`: `processors_count`/`bsp_id`） | 01/10 | 已核对 |
| `include/minix/endpoint.h`（`_ENDPOINT_P`） | 04 | 已核对 |
| `Makefile` | 构建配置（非语义，WONTFIX 标注） | 已核对 |

### 5.3 语义模块覆盖清单（函数级）

> 每篇文档必须覆盖的函数清单如下（服务端函数全量枚举；对端函数按契约面枚举）。跨文件的关键语义逐一列出，防止"文件有映射但函数漏掉"：

- **01**：`main`（main.c:22）、`sef_local_startup`（main.c:111，同时注册 init_fresh 与 init_restart STATEFUL）、`sef_cb_init_fresh`（main.c:126）、`sys_getmachine`、`init_scheduling`（调用点，定义归 11）
- **02**：`sef_receive_status`、`is_ipc_notify`、CLOCK 通知分支（main.c:66-72）、dispatch switch（main.c:74-92）、`reply`（main.c:101，`ipc_send` 异步）、`SUSPEND` 契约（main.c:97-99）、`no_sys`（utility.c:18）、`IPC_FLG_MSG_FROM_KERNEL` 校验（main.c:84-90）
- **03**：`schedproc` 结构全字段（endpoint/parent/flags/max_priority/priority/time_slice/cpu/cpu_mask）、`IN_USE`（schedproc.h:35）
- **04**：`schedproc[NR_PROCS]` 静态表、`_ENDPOINT_P` slot 提取、`sched_isokendpt`（utility.c:29）、`sched_isemtyendpt`（utility.c:46）、`accept_message`（utility.c:61，PM/RS 白名单）、错误码 `EBADEPT`/`EINVAL`/`EDEADEPT`
- **05**：`NR_SCHED_QUEUES`/`TASK_Q`/`MAX_USER_Q`/`USER_Q`/`MIN_USER_Q`（config.h:66-71）、`USER_QUANTUM`（config.h:74）、`USER_DEFAULT_CPU`（config.h:77）、`DEFAULT_USER_TIME_SLICE`（schedule.c:41）、`is_system_proc`（schedule.c:44，`parent==RS_PROC_NR`）、`nice_to_priority`（pm/utility.c:91，概念引用 04-stage-pm/16）
- **06**：`do_start_scheduling`（schedule.c:140-252）全流程：双消息 assert、`accept_message`、`sched_isemtyendpt`(child)、slot 填充（endpoint/parent/max_priority）、`max_priority >= NR_SCHED_QUEUES → EINVAL`、`endpoint==parent` init 特例（USER_Q/DEFAULT_USER_TIME_SLICE/BSP）、START 分支（priority=max_priority、time_slice=msg.quantum）、INHERIT 分支（`sched_isokendpt`(parent)、priority/time_slice 继承）、`sys_schedctl(0, ep, 0, 0, 0)` 接管、`flags=IN_USE`、`pick_cpu`、`schedule_process(SCHEDULE_CHANGE_ALL)` + EBADCPU 重试（`cpu_proc[cpu]=CPU_DEAD`）、回复 `scheduler=SCHED_PROC_NR`
- **07**：`do_stop_scheduling`（schedule.c:112-137）：`accept_message`、`sched_isokendpt`、`cpu_proc[cpu]--`（CONFIG_SMP）、`flags=0`
- **08**：`do_noquantum`（schedule.c:87-109）：`m_source` 直取 endpoint（无 accept_message，靠主循环 FROM_KERNEL 校验）、`priority < MIN_USER_Q` 降级、`schedule_process_local`；`do_nice`（schedule.c:254-295）：`accept_message`、`maxprio >= NR_SCHED_QUEUES → EINVAL`、`max_priority = priority = new_q`、`schedule_process_local` + 失败回滚
- **09**：`schedule_process`（schedule.c:297-332）：`SCHEDULE_CHANGE_PRIO/QUANTUM/CPU/ALL`（schedule.c:22-30）、`-1` 保持语义、`niced = (max_priority > USER_Q)`、`sys_schedule` 消息（`mess_lsys_krn_schedule`：endpoint/quantum/priority/cpu/niced）；`do_schedule`（do_schedule.c:8-31）：`caller != p_scheduler → EPERM`、`sched_proc` 应用（system.c:642-723：EINVAL/EBADCPU 校验、RTS_NO_QUANTUM 重排队、`MF_NICED`）
- **10**：`pick_cpu`（schedule.c:48-78）：单核→`bsp_id`、`is_system_proc`→BSP、负载最低可用 CPU、`cpu_is_available`、`cpu_proc[]`、`CPU_DEAD=-1`；`machine.processors_count/bsp_id`（type.h:123-124）
- **11**：`init_scheduling`（schedule.c:334-351）：`balance_timeout = BALANCE_TIMEOUT × sys_hz()`、`sys_setalarm`；`balance_queues`（schedule.c:353-369）：IN_USE 扫描、`priority > max_priority` 恢复一级、`schedule_process_local`、重设 alarm
- **12**：`do_schedctl`（do_schedctl.c:7-49）：flags 校验（`SCHEDCTL_FLAG_KERNEL`）、`isokendpt`、KERNEL 分支（`sched_proc` + `p_scheduler=NULL`）、注册分支（`p_scheduler=caller`）；`notify_scheduler`（proc.c:1860-1891）：RTS_NO_QUANTUM 出队、accounting 7 字段（`acnt_queue/deqs/ipc_sync/ipc_async/preempt/cpu/cpu_load`）、`reset_proc_accounting`、`mini_send`（FROM_KERNEL）；`proc_no_time`（proc.c:1893-1910）：PREEMPTIBLE 双分支
- **13**：`sched_inherit`（sched_start.c:11-41）、`sched_start`（sched_start.c:46-76，`NONE` 短路/`KERNEL` → `sys_schedctl(SCHEDCTL_FLAG_KERNEL)`）、`sched_stop`（sched_stop.c:9-29，`KERNEL/NONE` 短路）、`sched_init`（pm/schedule.c:20-52，INIT 接管）、`sched_start_user`（pm/schedule.c:55-85，`nice_to_priority`/PRIV_PROC 父 → INIT_PROC_NR 特例）、`sched_nice`（pm/schedule.c:89-112）、`sched_stop`（pm/forkexit.c:425，exit 路径）、`mp_scheduler` 语义
- **14**：`sched_init_proc`（rs/utility.c:364-384）、`sched_stop`（rs/manager.c:461、rs/request.c:342，系统服务终止路径）、`r_scheduler`/`r_priority`/`r_quantum`/`r_cpu`（rs 槽配置，跨 03-stage-rs/08）、系统进程 `parent=RS_PROC_NR`
- **99**：常量表（SCHEDULING_* 5 消息 + 优先级 6 常量 + SCHEDULE_CHANGE_* + BALANCE_TIMEOUT）、endpoint/generation、双层调度模型、五进程表一致性

### 5.4 明确排除 / 跳过的项

| 项 | 原因 | 处理 |
|----|------|------|
| 内核调度原语实现（enqueue/dequeue/pick_proc 内部、RTS 状态机） | 已由 01-stage-kernel/11-scheduling-primitives.md 覆盖 | 12 只写契约，引用不重复 |
| PM nice 系统调用用户面（`do_getsetpriority`/`get_nice_value`/`PRIO_MIN`/`PRIO_MAX`） | 归 04-stage-pm/16-scheduling.md | 13 只写 `sched_nice` 服务端消息面 |
| RS 槽配置细节（`r_*` 字段来源、system.conf） | 归 03-stage-rs | 14 只写 `sched_init_proc` 契约面 |
| 内核 SMP 迁移机制（`smp_schedule_migrate_proc`、cpu_is_ready） | 归 01-stage-kernel/16-smp.md | 10 只写 `pick_cpu` 策略面 |
| `servers/sched/Makefile`、构建/链接配置 | 非语义 | WONTFIX 标注 |
| `cpu_mask[]` 写入路径 | C 中不存在（schedule.c:185 FIXME） | S-3 结构消除决策（§7.3） |

---

## 6. 实施路线

### 6.1 文档改写状态跟踪

| 编号 | 文档 | 状态 | 最后改写 | scan 路径 |
|------|------|------|---------|-----------|
| 00 | `00-sched-overview.md` | pending | — | `.review/codex/sched/00-overview/` |
| 01 | `01-sched-init-main.md` | pending | — | `.review/codex/sched/01-init-main/` |
| 02 | `02-sched-message-surface.md` | pending | — | `.review/codex/sched/02-message-surface/` |
| 03 | `03-schedproc-struct.md` | pending | — | `.review/codex/sched/03-schedproc-struct/` |
| 04 | `04-schedproc-table.md` | pending | — | `.review/codex/sched/04-schedproc-table/` |
| 05 | `05-priority-timeslice-model.md` | pending | — | `.review/codex/sched/05-priority-model/` |
| 06 | `06-start-scheduling.md` | pending | — | `.review/codex/sched/06-start-scheduling/` |
| 07 | `07-stop-scheduling.md` | pending | — | `.review/codex/sched/07-stop-scheduling/` |
| 08 | `08-noquantum-nice.md` | pending | — | `.review/codex/sched/08-noquantum-nice/` |
| 09 | `09-schedule-process.md` | pending | — | `.review/codex/sched/09-schedule-process/` |
| 10 | `10-pick-cpu-smp.md` | pending | — | `.review/codex/sched/10-pick-cpu-smp/` |
| 11 | `11-balance-queues.md` | pending | — | `.review/codex/sched/11-balance-queues/` |
| 12 | `12-kernel-interface.md` | pending | — | `.review/codex/sched/12-kernel-interface/` |
| 13 | `13-pm-interaction.md` | pending | — | `.review/codex/sched/13-pm-interaction/` |
| 14 | `14-rs-interaction.md` | pending | — | `.review/codex/sched/14-rs-interaction/` |
| 99 | `99-global-concepts.md` | pending | — | `.review/codex/sched/99-global-concepts/` |

### 6.2 改写前置项（Rust 实现缺口）

1. **minix-types 补齐**：`SCHEDULING_START/STOP/SET_NICE/INHERIT` 常量（com.rs，C: com.h:801-807）+ 4 个服务端消息结构体（`MessLsysSchedSchedulingStart/Stop`、`MessPmSchedSchedulingSetNice`、`MessSchedLsysSchedulingStart`，C: ipc.h:1428-1445,1819-1828,1904-1913）——02/13 写作与实现的共同前置
2. **`os/servers/sched/` 骨架初始化**：事件循环主循环 + dispatch 表（对齐 03-stage-rs 的 dispatch.rs 模式）——01/02 写作后
3. **S-3 决策落地**：`cpu_mask[]` 结构消除或实现（§7.3）——03/10 写作前

---

## 7. Review 记录

### 7.1 深度 review（2026-08-16）

**方法**：对重组草案按 review-doc-skill 全维度检查：叙事主线一致性、概念完整性、覆盖缺口、拆分粒度、交叉引用、可执行性。对照 `02-stage-vm/plan.md` 与 `04-stage-pm/plan.md` 的结构基准。

| # | 发现 | 级别 | 处理 |
|---|------|------|------|
| R-1 | 旧主线草案无 `main.c`/主循环/SEF 生命周期文档（sched 的"心脏"缺失） | P1 | 新增 01/02（阶段 1） |
| R-2 | draft/04 把 6 个独立语义（优先级模型/时间片/队列平衡/CPU 选择/do_stop/do_nice）压成一篇，概念与服务混杂 | P1 | 拆为 05/07/08/10/11 |
| R-3 | draft 以 SCHEDULING_INHERIT 为主入口，SCHEDULING_START（RS 系统进程路径）被边缘化 | P1 | 06 统一双消息处理 + fork 次主线路径图 |
| R-4 | draft/04 将 time_slice 单位误写为 "ticks"；实际对接内核 `p_quantum_size_ms`（ms） | P1（事实错误） | 05 修正 + S-6 标注 |
| R-5 | draft 未覆盖 `do_noquantum` 的来源信任模型（无 accept_message，靠主循环 FROM_KERNEL + `m_source` 直取） | P1 | 08 专节 |
| R-6 | draft 未覆盖内核契约（`p_scheduler` 注册语义、`SCHEDULING_NO_QUANTUM` accounting 7 字段、`proc_no_time` 双分支） | P1 | 新增 12 |
| R-7 | draft 未覆盖客户端契约（libsys 3 接口、PM `sched_init`/`sched_start_user`、RS `sched_init_proc`） | P1 | 新增 13/14 |
| R-8 | minix-types 缺 SCHEDULING_START/STOP/SET_NICE/INHERIT 常量与 4 个服务端消息结构体（grep 实证，见 §6.2） | P1 | 改写前置项 |
| R-9 | draft/99 引用 `../03-stage-kernel/...` 旧路径（实际阶段目录为 01-stage-kernel），移入 draft 后失效 | P2 | 修正为 `../01-stage-kernel/` |
| R-10 | 优先级常量跨 config.h/schedule.c 两处定义，draft 未成表 | P2 | 05/99 双表（常量归属 + 值） |
| R-11 | draft/03 缺 EBADCPU 重试循环（`cpu_proc[cpu]=CPU_DEAD` 后再 pick_cpu）——SMP 下启动失败处理的核心 | P1 | 06 补全流程 |
| R-12 | draft/01 声称 fork 时"设置 cpu_mask"，实际 C 从不写入该字段（schedule.c:185 FIXME） | P2（事实错误） | S-3 决策（§7.3） |
| R-13 | 错误码语义（EBADEPT/EINVAL/EDEADEPT/EPERM/ENOSYS/EBADCPU）未成表 | P2 | 04/06/08/09 + 99 |
| R-14 | 无 SUSPEND 契约说明（服务端不回复的唯一路径） | P2 | 02 专节 |
| R-15 | `accept_message` 白名单（PM/RS）散落各篇，安全模型不集中 | P2 | 04 集中声明 |
| R-16 | `sched_proc` 内核侧与 `schedule_process` 服务端侧容易混淆（同名不同层） | P2 | 09/12 命名对照表 |

**结论**：重组草案 16 篇覆盖全部语义模块，拆分粒度与 `01-stage-kernel`/`02-stage-vm` 一致（每篇一个语义单元），无前向引用（编号顺序保证依赖前置）；16 项发现全部落实后定稿。

### 7.2 minix3 源码回归 review（2026-08-16）

**方法**：对 `minix3/minix/servers/sched/` 全部 3 个 .c + 3 个头文件逐函数 grep 核对 §5.1/§5.3 映射表；对契约对端（`lib/libsys/sched_start.c`、`sched_stop.c`、`servers/pm/schedule.c`、`servers/rs/utility.c`、`kernel/system/do_schedctl.c`、`do_schedule.c`、`kernel/system.c:sched_proc`、`kernel/proc.c:notify_scheduler/proc_no_time`）逐函数核对；对消息结构体与常量核对 `include/minix/ipc.h`/`com.h`/`config.h`。

**证据**：

```bash
# 服务端函数面（全量 12 个，全部映射）
grep -n '^int main\|^static void reply\|^static void sef_local_startup\|^static int sef_cb_init_fresh' servers/sched/main.c
grep -n '^static void pick_cpu\|^int do_noquantum\|^int do_stop_scheduling\|^int do_start_scheduling\|^int do_nice\|^static int schedule_process\|^void init_scheduling\|^void balance_queues' servers/sched/schedule.c
grep -n '^int no_sys\|^int sched_isokendpt\|^int sched_isemtyendpt\|^int accept_message' servers/sched/utility.c
# 客户端面 + 内核契约面（全部映射）
grep -n '^int sched_inherit\|^int sched_start' lib/libsys/sched_start.c
grep -n '^int sched_stop' lib/libsys/sched_stop.c
grep -n '^void sched_init\|^int sched_start_user\|^int sched_nice' servers/pm/schedule.c
grep -n '^int sched_init_proc' servers/rs/utility.c
grep -n '^int do_schedctl' kernel/system/do_schedctl.c
grep -n '^int do_schedule' kernel/system/do_schedule.c
grep -n '^int sched_proc' kernel/system.c
grep -n 'notify_scheduler\|^void proc_no_time' kernel/proc.c
# 消息结构体（7 个，全部映射）
grep -n 'mess_krn_lsys_schedule\|mess_lsys_krn_schedctl\|mess_lsys_krn_schedule\|mess_lsys_sched_scheduling_start\|mess_lsys_sched_scheduling_stop\|mess_pm_sched_scheduling_set_nice\|mess_sched_lsys_scheduling_start' include/minix/ipc.h
# 常量（全部映射）
grep -n 'SCHEDULING_BASE\|SCHEDULING_NO_QUANTUM\|SCHEDULING_START\|SCHEDULING_STOP\|SCHEDULING_SET_NICE\|SCHEDULING_INHERIT' include/minix/com.h
grep -n 'NR_SCHED_QUEUES\|TASK_Q\|MAX_USER_Q\|USER_Q\|MIN_USER_Q\|USER_QUANTUM' include/minix/config.h
grep -n 'SCHEDULE_CHANGE_PRIO\|SCHEDULE_CHANGE_QUANTUM\|SCHEDULE_CHANGE_CPU\|BALANCE_TIMEOUT\|DEFAULT_USER_TIME_SLICE\|CPU_DEAD' servers/sched/schedule.c
```

**回归补录（本表初稿缺口）**：`sched_stop` 的 PM/RS 调用面（pm/forkexit.c:425、rs/manager.c:461、rs/request.c:342）初稿未列入 → 补入 13/14（§2 表、§5.1、§5.3 三处同步）。

**结论**：服务端 16 个函数全量映射（main.c 4 个 → 01/02，schedule.c 8 个 → 05~11，utility.c 4 个 → 02/04）；契约对端 14 个函数/调用点（libsys 3、PM 4、RS 3、kernel 5）全量映射（13/14/09/12）；7 个消息结构体、SCHEDULING_* 5 消息、优先级 6 常量、调度标志 4 常量全部进入覆盖契约；ARCH 项（S-1~S-10）与 minix3 现状对照成立（S-3 经 grep 证实 `cpu_mask` 全库无写入）。**覆盖完整性通过，无遗漏。**

### 7.3 写作前置设计决策：`cpu_mask[]` 结构消除（2026-08-16）

**决策**：`schedproc.cpu_mask[]`（schedproc.h:27-28）在 minix-rs **不实现**，归入 `[ARCH: S-3]` 结构消除。

**依据**（grep 实证）：
- 全代码库对 `cpu_mask` 仅有声明与结构体内存占用，**零写入、零读取**（schedule.c:185 仅 `FIXME set the cpu mask` 注释）；`pick_cpu` 只写 `proc->cpu`，CPU 选择完全由 SCHED 内部 `cpu_proc[]` 负载计数决定。
- minix3 无 CPU 亲和性消息面（`mess_lsys_sched_scheduling_start` 仅 endpoint/parent/maxprio/quantum 四字段，ipc.h:1428-1433），`cpu_mask` 无对端来源——它只是预留字段。
- minix-rs 的 CPU 抽象（`os/arch/` SMP trait，01-stage-kernel/16-smp.md）以运行时 `processors_count` 为准；若未来需要亲和性，应新增显式 IPC 面而非复活死字段。

**同步**：plan.md §2 03/10 行、checklist 或 SYMBOLS（如生成）S-3 行、03-schedproc-struct.md §3、10-pick-cpu-smp.md §3 四处一致。

---

## 8. 参见

- `02-stage-vm/plan.md`、`04-stage-pm/plan.md` — 本 plan 的结构基准（同一工作流）
- `01-stage-kernel/00-kernel-overview.md` §3 — 讲述结构组织原则
- `01-stage-kernel/11-scheduling-primitives.md` — 内核调度原语（sched 的对端）
- `04-stage-pm/16-scheduling.md` — PM 侧调度（sched 的客户端）
- `03-stage-rs/08-rs-slot-config.md` 等 — RS 侧槽配置（sched 的客户端）
- `minix3/minix/servers/sched/` — ground truth
- `os/servers/sched/` — Rust 实现（当前为空骨架）
