# 06-stage-sched Rust 实现架构级 Review TODO

> 来源：2026-09-06 第一轮架构级审查（archive/todo-round1-archive-2026-09-06.md，全文存档，含 Fix #1~#4 修复记录）+ 2026-09-09 第二轮（V2 轮，本文档主体）。
> 范围：一等对象 `os/servers/sched/src/` 全部 Rust 代码（19 文件，约 4400 行，crate 名 `minix-sched`，79 测试）；内核契约面与客户端面为辅（发现按 edge 判定规则登记 `../edge_todo.md`）。
> 方法：V2 轮 = cmd-04 第二遍。先查漏补缺（C 符号 ↔ 14 篇文档 ↔ Rust 三向矩阵复建 + 第一轮最大缺口「主循环」落地后的八条执行侧语义逐条对测试），再按「组合层 → 服务器内部 → 内核接缝 → wire 层 → 测试」五层深审，对照 Redox（联网核实）/OS 理论/Rust 社区惯例。本轮只审查未修代码。
> 定位：不复写 plan.md；跨 stage 条目唯一入口是 `../edge_todo.md`，本文档只留双向指针（§5）。
> 状态（2026-09-09，V2 轮）：**服务本体收敛——P0 为零，stage 内新发现 1 条 P2 + 2 条 P3；真正的缺口全部在缝上**（内核接缝三件：niced 断环、PREEMPTIBLE 近似、SMP 三件套，已登记 edge 四条目 + E5 一增补）。第一轮遗留 9 条 open 经逐条 grep 复核全部维持（P3-1 的 noquantum_trust 行因主循环接线而闭单，其余升格或维持）。

---

## 0. 审查结论速览

一句话总结：主循环落地后，服务器的判定层、组合层、传输接缝三层形状已经稳定，八条执行侧语义各有点名测试钉住——服务本体这一轮几乎没有新账。V2 轮的产出在边界上：沿着 NO_QUANTUM 和 SYS_SCHEDULE 两条 wire 钻进内核对端，发现 C 依赖两个 Rust 内核从未实现的语义（SYS_SCHEDULE 消息的 niced 字段被 `let niced = false` 丢弃；PREEMPTIBLE 特权标志被 `priority != 0` 近似替代），前者的修复注释还引用了一个 C 树里不存在的调用（SYS_NICE）。这三件事都不属于本 stage 的生产代码，按 edge 规则登记，SCHED 侧零改动。

| 级别 | 条目 | 一句话 | 状态 |
|------|------|--------|------|
| P0 | （无） | 本轮零发现，核对依据见 §4.0 | — |
| P1 | （stage 内无） | 本轮 P1 级发现全部是内核生产代码 → edge | §5 |
| P2 | V2-P2-1 | 预留符号清算批次（升格自 P3-1：清算时点已到） | ✅ 已修复 2026-09-09（Fix #5） |
| P2 | P2-1 | init 临时值 BSP 语义误读（V2 复核：加重——Rust 注释也描述了不存在的规则） | ✅ 已修复 2026-09-09（Fix #7） |
| P2 | P2-2 | 14 篇 Rust 实现归属未声明（部分推进：文档分工声明已落） | open |
| P2 | P2-3 | SUSPEND 常量本地定义（V2 附注：与 E-MINTYPES-SYS 合并修） | open |
| P2 | P2-4 | `SchedProc.cpu` 裸 u32（V2 附注：落点扩为全链 5 处签名） | ✅ 已修复 2026-09-09（Fix #8） |
| P3 | V2-P3-1 | 测试补强两小件（无效 spender 静默分支 / 多进程回升序） | ✅ 已修复 2026-09-09（Fix #10） |
| P3 | V2-P3-2 | Probe 越界 dummy 值改类型表达 | ✅ 已修复 2026-09-09（Fix #9） |
| P3 | P3-1 | 预留未接线符号盘点（noquantum_trust 行闭单，余项升格 V2-P2-1） | ✅ 全部闭单（四符号随 Fix #5 删除） |
| P3 | P3-2 | 两处 minix-types 行锚漂移（V2 复核：两半均仍漂移） | open |
| P3 | P3-3 | 00/99 缺 .design/ 快照（V2 复跑确认） | open |
| P3 | P3-4 | lib.rs glob 再导出无人用 | ✅ 已修复 2026-09-09（Fix #6） |
| P3 | P3-5 | 11 篇补 Redox 演进参照（V2 增补：RR→DWRR→EEVDF 两级） | open |

**闭环账**（第一轮 → 现在）：P1-1（主循环）/ P1-2（SchedServer 单一所有者，[ARCH S-11]）/ P1-3（IpcTransport + KernelApi 双 trait 接缝）✅ 2026-09-06 修复（测试基线 59 → 79，Fix #1~#4，全文见 archive §9；真实通电挂 edge E8）。

验证命令基线（2026-09-09 实测，后续修复轮以此为对照）：
- `cargo test -p minix-sched`：**79 passed / 0 failed**
- `cargo clippy -p minix-sched --all-targets`：本体 **0 条告警**（全链 12 条告警全部来自依赖 crate minix-sys/minix-types + workspace profile，已登记 edge E-MINSYS-HYGIENE，不属本阶段）
- `tools/design-coverage-check.sh fork-syscall-rewrite --stage 06-stage-sched`：00、99 各缺三快照（维持 P3-3）
- Gate E：02 篇 §5 的 server.rs 锚点抽验 3/3 对齐（`test_kernel_noquantum_demotes_and_never_replies`:895、`test_noquantum_floor_still_fans_out`:926、`test_forged_noquantum_answers_eperm`:948——Fix #4 文档同步有效）；01/02 篇的 minix-types 侧 2 处行锚仍漂移（P3-2 维持）

---

## 1. 覆盖矩阵 V2：查漏补缺结论

### 1.1 服务本体：13 篇映射全部闭合

第一轮矩阵的唯一「缺口」行（main 主循环 → P1-1）已修复。V2 轮对 `minix3/minix/servers/sched/` 四个 C 文件（main.c 137 行、schedule.c 369 行、utility.c 74 行、schedproc.h 39 行）逐符号重对：main/reply/sef_local_startup/sef_cb_init_fresh/do_start_scheduling/do_stop_scheduling/do_noquantum/do_nice/schedule_process/pick_cpu/init_scheduling/balance_queues/no_sys/sched_isokendpt/sched_isemtyendpt/accept_message/struct schedproc——17 个符号全部有 Rust 对应物，判定三档不变（已实现 / 判定层已实现 / 缺口），缺口为零。三个 C 原文 quirks（`accept_message` 用 endpoint 与 proc-nr 常量直接比较、`is_system_proc` 同型、MAX_USER_Q == TASK_Q）均已如实镜像并有测试锁定。

### 1.2 八条执行侧语义：逐条对测试（V2 核心复核项）

第一轮 §1.2 列出的八条「判定层管不到」的行为，Fix #3/#4 声称全部钉住——本轮逐条核验测试名与测试体，**8/8 成立**：

| # | 语义（C 锚） | 钉住测试（server.rs） | 核验 |
|---|---|---|---|
| 1 | 回件失败只丢一次（main.c:101-106） | `test_reply_failure_does_not_kill_the_loop` | ✓ |
| 2 | 内核 NO_QUANTUM 永不回件（main.c:70-77） | `test_kernel_noquantum_demotes_and_never_replies` | ✓ |
| 3 | 伪造 NO_QUANTUM 回 EPERM（main.c:78-83） | `test_forged_noquantum_answers_eperm` | ✓ |
| 4 | CLOCK 才整理、其余通知静默（main.c:44-55） | `test_clock_notification_rebalances_and_rearms` + `test_other_notification_passes_in_silence` | ✓ |
| 5 | NO_QUANTUM 不回滚、NICE 回滚（schedule.c:99-105 对 284-288） | 上条 2 的失败半 + `test_nice_regrades_and_rolls_back_on_failure` | ✓ |
| 6 | START 失败残留占用（schedule.c:223 先于 233-237） | `test_start_fanout_failure_leaves_slot_occupied` | ✓ |
| 7 | 平衡遍历 fire-and-forget（schedule.c:358-364） | `test_clock_notification_rebalances_and_rearms`（fanout 结果不检） | ✓ |
| 8 | START 成功回件写 scheduler=SCHED（schedule.c:246） | `test_start_from_pm_happy_path` | ✓ |

### 1.3 缝上缺口（V2 真正的查漏产出，全部已登记 edge）

沿两条 wire 钻到内核对端后的三个行为级发现 + 一个常量收敛项。C ground truth 与 Rust 两侧锚点、修法、依赖见 `../edge_todo.md`：

| 缝 | C 行为 | Rust 内核现状 | edge 条目 |
|---|---|---|---|
| SYS_SCHEDULE 的 niced 字段 | do_schedule.c:27 读线 → sched_proc 写 MF_NICED（system.c:692-694） | syscall.rs:923 `let niced = false;` 丢弃；注释引用的 "SYS_NICE" 在 C 树不存在 | E-SCHEDNICED |
| PREEMPTIBLE 特权标志 | proc.c:1895（通知门）+ proc.c:1638（抢占门）读 priv 旗标 | proc_table.rs:766/:674 用 `priority != 0` 近似；sched.rs:233 的"正确助手"同为近似且未接线 | E-PREEMPTFLAG |
| SCHED 下发的 cpu 字段 | system.c:650-654 EBADCPU 校验 + :673-677 跨 CPU 迁移 | 校验是桩（sched.rs:355-360）、迁移未接线、每核队列恒 BSP（proc_table.rs:573-586） | E-SCHEDSMP |
| SYS_* 调用号常量 | com.h:210-262 一族 | kernel 枚举与 SCHED 镜像各自表达，minix-types 缺位 | E-MINTYPES-SYS |

其中 E-PREEMPTFLAG 有可观察后果：MAX_USER_Q == TASK_Q == 0，SCHED 接管的进程经 START/NICE 合法可达队列 0，此后 quantum 耗尽走「内核调度者续量」分支——SCHED 永远收不到 NO_QUANTUM，MLFQ 的降级臂对它失效。单核现状即可达，非 SMP 专属。

---

## 2. V2 轮新条目

### V2-P2-1 预留符号清算批次：四个符号到达既定清算时点（升格自 P3-1）✅ 已修复 2026-09-09（Fix #5，见 §9）

**问题**：第一轮 P3-1 为五个「仅测试引用」的预留符号定了共同标准——「P1-1 落地后仍无生产调用者，即按死代码消除流程逐项删」。主循环已于 2026-09-06 落地，V2 轮逐一 grep 复核生产调用者：

| 符号 | 锚点 | V2 复核 | 处置 |
|---|---|---|---|
| `noquantum_trust` | dispatch.rs:86 | **已有生产调用者**（server.rs:244 经 server.rs:27 消费） | ✅ 闭单（第一轮「保留」判定兑现） |
| `is_valid_quantum` | priority.rs:180 | 仍仅测试引用（priority.rs:232-234） | **删**：C 的时间片校验在内核（system.c:648-649），SCHED 原样存储 |
| `USER_QUANTUM` | priority.rs:52 | 生产代码仅注释提及（start.rs:132），无真消费 | **删或移 PM crate**（05 篇已论证它与 DEFAULT_USER_TIME_SLICE 来源之别） |
| `is_available` | cpu.rs:47 | 仍仅测试引用；`pick` 的循环体直接模式匹配 `Option`（cpu.rs:85-88） | **内联删**（或留作 pick 注释） |
| `IN_USE` | schedproc.rs:23 | 仍仅测试断言（:134）；占用语义已由 `SlotState` 完整表达 | **删**，留一行注释（schedproc.rs:19-22 的说明已是好载体） |

**影响**：五个符号四个确认死代码——dead code 显式子轮的既定标准已触发而未执行，每多留一轮就多一分「这是接口预留还是遗忘」的评审记忆成本。

**建议**：一次 todo-fix 批次做完四个（fix-guard 逐条），删后跑 `cargo test -p minix-sched` 对照 §0 基线 79 passed（删除纯死代码，基线应不动）。
**验证**：`rg -n "is_valid_quantum|USER_QUANTUM|is_available|IN_USE" os/servers/sched/src/ --type rust` 修后应零生产命中。

### V2-P3-1 测试补强两小件 ✅ 已修复 2026-09-09（Fix #10，见 §9）

**问题**：(a) NO_QUANTUM 带内核旗标但 `m_source` 无效（越界如 9999、或负值 task endpoint）的分支无测试——C 走 `sched_isokendpt` 失败返回 EBADEPT、主循环 `continue` 不回件（schedule.c:92-95 + main.c:76），Rust 走 `Probe` 的 OutOfRange/Task 判决 + `admit` 拒绝 + 静默（server.rs:496-499），现有 `test_kernel_noquantum_demotes_and_never_replies` 只测合法 CHILD；(b) `balance_queues` 多进程回升的遍历序（按槽位 0..NR_PROCS）与「只回升到 ceiling 不越过」（schedule.c:360）只有单进程测试（`test_clock_notification_rebalances_and_rearms`），多进程同轮各升一级、到 ceiling 停的场景未钉。

**建议**：两测试各一，表驱动；随任何触碰 server.rs 的轮次顺带，不单独立项排队。
**验证**：新增测试名在 02 篇 §5 表同步登记（Gate E 对账）。

### V2-P3-2 Probe 的越界 dummy 值改为类型表达 ✅ 已修复 2026-09-09（Fix #9，见 §9）

**问题**：`Probe::read` 对越界 endpoint 构造 dummy 字段（server.rs:120-130：`Priority::new(0).expect(...)`、`index: 0`），依赖注释约定「dummies when out of range; unread in that case」（server.rs:91-92）——门判决已拒绝所以值不被消费，但类型上「可读的行」与「编造的行」不可区分，未来消费方若跳过门判决直接读 Probe 字段，编译器不会拦。
**建议**：首选，`row: Option<RowValues>`（判决通过才 `Some`，消费方 `let Some(row) = ... else return` 自然强制先过门）；次选，维持现状但在 Probe 文档头把「unread」升级为显式不变量 + debug_assert。
**验证**：`cargo test -p minix-sched` 基线不动；`rg -n "probe\." os/servers/sched/src/server.rs` 逐消费点核对先门后读。

---

## 3. 存量 open 条目（V2 staleness 复核结论）

> 原文全文见 archive（第一轮 §4/§5）；此处保留条目主旨 + V2 复核注记，不重写历史结论。

### P2-1 init 临时值「长期作用于 BSP」的语义误读 —— ✅ 已修复 2026-09-09（Fix #7，见 §9）

**V2 复核结论（修复前存档）**：三重失真——(1) start.rs 注释声称 C 的 184 赋值长期生效（实际 schedule.c:226 无条件 pick_cpu 覆盖一切）；(2) 注释声称 caller 会把自父种子留在 BSP（实际 cpu.rs pick 无此规则）；(3) 06 篇 :31 同款表述 + 10 篇 :22 引用一条 06 不存在的主张。

- start.rs:133-135 注释仍声称「The lasting init effect is the CPU (machine.bsp_id, 184, SMP builds only): the caller keeps a self-parented seed on the BSP (10 consumes this rule)」；06 篇 :31 同款表述仍在。
- **V2 新发现（加重）**：注释声称的「caller keeps a self-parented seed on the BSP」这条规则**在 Rust 代码中也不存在**——server.rs:383-384 用 `is_system_proc(seed.parent)` 计算（init 自父、parent=INIT≠RS → false），cpu.rs `pick` 只有两条 BSP 规则（processors_count<=1 与 is_system），没有 self-parented 分支。即注释同时误读了 C（schedule.c:226 无条件 pick_cpu 覆盖一切临时值，包括 184 的 BSP 赋值）和误描述了自己的代码。
- 修复面从「两处表述」扩为：start.rs 注释 + 06 篇两处 + 核查 10 篇是否复述了该「规则」。
- **验证**：`rg -n "lasting|self-parented" os/servers/sched/src/scheduling/start.rs os/servers/sched/src/cpu.rs` + `rg -n "真正起作用|BSP" 06-start-scheduling.md 10-pick-cpu-smp.md`。

### P2-2 14 篇（RS 交互）的 Rust 实现归属未声明 —— 维持 open（部分推进）

- V2 复核：14 篇已有「分工声明」块（:7-11，文档主权指向 03-stage-rs 两篇），但 P2-2 要的 **Rust 实现归属三层声明**（服务端视角 valid.rs / 契约镜像 client.rs / 行为主体 os/servers/rs）未落——client.rs 模块头（:1-15）自述「PM-facing half」，没有「本模块是契约镜像、不区分 PM 与 RS」一句；lib.rs 模块索引（:21）对 client 的描述也未提 14 篇。
- 修法维持第一轮首选：14 篇补归属段 + client.rs 模块头补一句；次选（99 篇导航表集中登记）仍可。

### P2-3 SUSPEND 常量本地定义 —— 维持 open

- V2 复核：dispatch.rs:21 `pub const SUSPEND: i32 = -998;` 仍在；同型新增一处——transport.rs:41/:45 的 SYS_SCHEDULE/SYS_SCHEDCTL 本地镜像（有注释、有 wire 断言钉值，第一轮 Fix #1 已注明「minix-types 暂缺」）。
- **V2 修法更新**：与 edge E-MINTYPES-SYS 合并执行——minix-types 一次补两族常量（SUSPEND + SYS_* 调用号），dispatch.rs/transport.rs 改消费，避免同一个 crate 跑两遍常量收敛。

### P2-4 `SchedProc.cpu` 的裸类型 —— ✅ 已修复 2026-09-09（Fix #8，见 §9）

- V2 复核：schedproc.rs:93 `pub cpu: u32` 仍在；且主循环落地后裸 u32 的流转面扩大——`Probe.cpu`（server.rs:95）、`SlotValues.cpu`（schedule.rs:70）、`Fanout.cpu: Option<u32>`（schedule.rs:91）、`CpuLoad = Option<u32>`（cpu.rs:37）、`release.cpu`（stop.rs:42）——CpuId newtype 的落点从第一轮估计的 2 处扩为全链 6 处签名。内核侧先例不变（os/kernel/src/proc.rs:471 `CpuId(u32)`）。
- 修法维持第一轮首选：`CpuId(u32)` newtype，由 `cpu::pick` 产出、全链消费；构造不做拓扑校验（与 S-3/S-4 的既定分工一致）。

### P3-1 预留未接线符号盘点 —— ✅ 全部闭单

noquantum_trust 行闭单（主循环接线，server.rs:244 消费）；is_valid_quantum / USER_QUANTUM / is_available / IN_USE 四行随 Fix #5 删除（2026-09-09，见 §9）。本条目闭环。

### P3-2 文档第 5 节测试表两处 minix-types 行锚漂移 —— 维持 open（两半均仍漂移）

- 02 篇 :221 仍声称 `test_sched_message_layouts` 在 message.rs:2477，实测 :3698；
- 01 篇 :179 仍声称 `test_sched_messages` 在 com.rs:223，实测 :312。
- 修复归属不变（style-fix 或 full-review 锚点纪律门随手修）；02 篇 §5 的 server.rs 侧锚点经 Fix #4 同步已对齐（抽验 3/3），漂移只剩 minix-types 侧两处。

### P3-3 00 与 99 两篇缺 .design/ 快照 —— 维持 open

V2 复跑 `tools/design-coverage-check.sh` 确认：00/99 各缺 outline、outline-review、design 共六个文件。归文档排期，不阻断代码审查。

### P3-4 lib.rs 唯一的 glob 再导出没有被使用 —— ✅ 已修复 2026-09-09（Fix #6，见 §9）

V2 复核：lib.rs:37 `pub use sef::*;` 仍在；main.rs 的装配走全路径（main.rs:18-21），无使用者。已删除，调用方统一全路径。

### P3-5 11 篇可补 Redox 演进参照 —— 维持 open + V2 增补事实

V2 联网复核：第一轮引用的 DWRR（[RSoC 2026: A new CPU scheduler for Redox](https://www.redox-os.org/news/rsoc-dwrr/)；[Phoronix 报道](https://www.phoronix.com/news/Redox-OS-New-CPU-Sched)）已非终点——Redox 随后以 [RSoC 2026: EEVDF for Redox](https://www.redox-os.org/news/rsoc-eevdf/) 把 DWRR 换成了 EEVDF（Linux 6.6 同款算法）。11 篇补参照时直接写两级演进（简单轮转 → DWRR → EEVDF）：S-9 预留的「第二个策略」路标现成两枚，且第二枚比第一枚更新。增补不改变条目性质（文档增强，可选）。

---

## 4. 架构分层深审结论（V2，五层）

### 4.0 P0 自检

本轮对四个处理臂的门序与 C 逐一重对（start.rs:106-125 `admit` 对照 schedule.c:150-166、nice.rs:53-72 对照 254-292、noquantum.rs:38-43 对照 92-96、stop.rs:53-61 对照 118-125），错误码有断言锁定（table.rs:117 的 (216, 22, 215)），消息线序有断言锁定（schedctl.rs:209-216、client.rs:265-278）。上一轮最大的悬案已裁决：`do_noquantum` 用 `m_source` 定位耗尽者（server.rs:496）**是 C 忠实的**——C 的 notify_scheduler 以进程名义发送（proc.c:1874 `m_no_quantum.m_source = p->p_endpoint` + FROM_KERNEL 旗标），Rust 内核对端同型（proc_table.rs:873-898，IpcEngine::send 以 caller endpoint 覆写 m_source）；消息体的七个 accounting 字段 C 侧也从不读，Rust 忽略它们不是缺口。按收敛规则做漏检自检：随机重验三处——START 失败残留（§1.2 #6）、fanout_local 不重 pick 的偏离论证（C 的重选会私改 rmp->cpu 且账目漂移，Rust 的「一次选择一次记账」自洽，10 D3 成立）、balancer 的 `current > max` 才回升（schedule.c:360 同式）——三者无新问题。

### 4.1 L0 组合层

`SchedServer` 单一所有者（表 + 台账 + 拓扑 + 平衡器四块状态，[ARCH S-11]）+ `run_once`/`run` 薄壳 + 双 trait 接缝，形状与第一轮设计一致且落地质量高：装配线与 C 启动序逐拍对齐（main.rs:31-56）；64 轮接收失败上限是已注明的诚实偏离（VM V10-P0-2 先例）；台账的运行期 `processors_count > 1` 门对应 C 的 `CONFIG_SMP` 编译期门（server.rs:386-388/:436-438），**语义每机等价**（非 SMP 构建的 cpu_proc[] 不参与 pick，schedule.c:78-80），唯一分叉是「SMP 构建但单核」时 C 记账而 Rust 不记——纯内部状态、不可观察，此处登记为等价性说明，不立项。一个跨服务器观察（不动手）：VM（KernelGateway + IPC transport）、RS（五域 supertrait）、SCHED（IpcTransport + KernelApi）、PM（KernelIpcTransport）四台服务器四种 seam 形状——各按需成立，但 E1 trap 层落地后若出现第四个消费者，值得评估一次「用户态服务器传输基建」上移（届时挂 edge，本轮只留字据）。

### 4.2 L1 服务器内部

门（admit）→ 计划（plan）→ 执行（caller 半）三层在四臂间同构，信任不对称用签名表达（noquantum 的 arm 不收 sender 参数，noquantum.rs:7-13）是本轮确认的最佳设计。`Current` 的 Copy 即快照（nice.rs:33-45）、`Seed` 不含占用位（写表是 caller 半）、`classify_fanout` 把重试环收敛为 `Done/CpuDead` 两态——三处类型设计都值得保持。新发现仅 V2-P3-2（Probe dummy 值）一条微观项。

### 4.3 L2 内核接缝

本轮深挖的主战场，产出即 §1.3 的三个 edge 条目（E-SCHEDNICED / E-PREEMPTFLAG / E-SCHEDSMP）。正面确认两处卓越实现：R-16-fix（priority 截断提权修复，syscall.rs:936-949 校验全 C 范围再窄化）与 D-52（scheduler-aware rts_set/rts_unset，sched.rs:328-333，排队进程参数更新先出队后入队）。另有两处内核侧字据留档（不立项，随对应 edge 轮顺带）：(a) sched.rs:286-294 的 SchedParams 文档把 niced 归因于「SYS_NICE (PM → kernel via SYS_SCHEDULE)」——后半句对（do_schedule.c:27 正是走 SYS_SCHEDULE），调用名 SYS_NICE 不存在，该注释随 E-SCHEDNICED 一并修正；(b) C 的 sched_proc 允许 priority=16 越过校验（system.c:644 `priority > NR_SCHED_QUEUES` 对 16 为假，入队即数组越界——被 SCHED 服务器侧的门挡住从未触发），Rust 的 sched_proc 以 `v > MIN_USER_Q` 拒绝 16，比 C 严——属「Rust 修复了 C 源码 bug 但无 MINIX3 BUG 标注」（模式 78），可在 kernel 侧触碰该函数时补一行标注。

### 4.4 L3 wire 层

`Fanout`/`ChangeMask`/`KEEP` 哨兵消除（schedule.rs:49-54「两侧同意的 -1 有一个家」）与 `MachineInfoBuf` 的 repr(C) 逐字段对齐是模式 16/17 的教科书执行。常量镜像的两处例外（SUSPEND、SYS_* 调用号）已有注释与钉值测试，收敛方案并入 P2-3 + E-MINTYPES-SYS。`MessKrnLsysSchedule` 七字段布局与 C `mess_krn_lsys_schedule`（ipc.h:272 断言）一致；`is_notify` 谓词寄居 transport.rs 是 minix-sys 缺口的诚实补位（注释已声明）。

### 4.5 L4 测试架构（轻量五维）

79 测试的构成：server.rs 20（八条语义 + 边界路径）、判定层单测 59。冗余度可接受（SlotVerdict errno 映射被四臂各自 assert 是「各臂独立钉门序」的有意重复）；无虚构（全部测试有 C 锚点注释）；mock 保真度良好（MockIpc 脚本化收发 + MockKernel 五调用账本，空脚本即 EIO 的失败路径零成本驱动）。缺口即 V2-P3-1 两小件；更上层的联调缺口（PM↔SCHED 全链）挂 E5(e)。

---

## 5. 边界条目双向指针（唯一入口：../edge_todo.md）

| edge 条目 | 来源 | 一句话 | 06 侧关联 |
|---|---|---|---|
| E-SCHEDNICED | 本轮 §1.3 | kernel 丢弃 SYS_SCHEDULE 的 niced 字段，注释引用不存在的 SYS_NICE | 12 篇契约 niced 半 |
| E-PREEMPTFLAG | 本轮 §1.3 | PREEMPTIBLE 用 priority!=0 近似，队列 0 进程永不通知调度者 | 08/12 篇 NO_QUANTUM 链 |
| E-SCHEDSMP | 第一轮 §7 升级 | cpu 下发链三环断（每核队列/EBADCPU/迁移） | 06 篇重试环、10 篇 pick |
| E-MINTYPES-SYS | 本轮 §4.4 | SYS_* 调用号常量三处各自表达 | P2-3 合并修 |
| E5 增补 (e) | 本轮 §4.5 | PM↔SCHED 联调验收面（START/INHERIT/NO_QUANTUM 回环） | E8 的联调出口 |
| E8（已有） | 第一轮 P1-3 抽取 | SCHED SYS_* 内核调用真实通电 | 传输接缝的生产半 |

---

## 6. 对照参考 V2

**Redox（联网核实，2026-09-09）**：调度全程在内核且两年两级跳——简单轮转 → 按权重的亏欠轮转 DWRR（[RSoC 2026 公告](https://www.redox-os.org/news/rsoc-dwrr/)，重载下约 1.5 倍吞吐）→ EEVDF（[RSoC 2026: EEVDF for Redox](https://www.redox-os.org/news/rsoc-eevdf/)，Linux 6.6 同款「最早合格虚拟截止期优先」）。对 SCHED 的启示不变且更强：Minix3 的双层模型（机制留内核、策略上移用户态）里，策略可替换性的价值被 Redox 的快速迭代反向验证——S-9 预留的 trait 接缝（balancer.rs:33-36）等「第二个策略」真的到来再落，路标现成两枚（DWRR/EEVDF，11 篇补参照时两级都写）。

**OS 理论**：降快升慢的 MLFQ 形状（demote/rebalance_one 对称纯函数）V2 复核维持「值得保持」；本轮新增的负面教材是 E-PREEMPTFLAG——把「是否参与协作式调度」这类**能力/特权**问题用**当前优先级**这个状态变量来近似，状态一变（进程合法登顶队列 0）语义就静默改变，这是 OS 设计里「身份 vs 状态」的经典分野（capability 与 dynamic priority 不可互替）。

**Rust 社区**：四臂的纯函数门 + 类型化判决（SlotVerdict/Fanout/Seed）与 no_std 单所有权事件循环是社区惯法的正面执行；本轮确认的两处反例都在内核侧（E-SCHEDNICED 的注释引用虚构实体；sched.rs:229-236 助手文档与实现脱钩）——注释声称的语义必须在代码中可指认，是本轮沉淀为规则候选的教训（§7 Rule Discovery 1）。

---

## 7. Rule Discovery（Step 5.7）与 Gate 证据

**规则候选（本轮新发现，待沉淀）**：
1. **模式 85 候选「注释声称的规则与实现脱钩」**：start.rs:133-135 注释声称一条代码里不存在的 pick 规则（P2-1 加重的根源）；检查命令：对注释中的规则式声明（keeps/consumes/rule/规则/约定）逐一在声称的目标处 grep 验证存在性。是模式 77（注释行号漂移）的语义版姊妹。
2. **「近似守卫必须标注近似」**：当 C 语义依赖特权标志/能力位而 Rust 用粗粒度代理（如 priority != 0 代 PREEMPTIBLE）时，必须显式标注「近似 + 分叉场景」，禁止把近似写成对齐（sched.rs:231 文档声称 priv 旗标、实现读优先级）。
3. **「边界外观察必须有去向」**：第一轮 §7 三条观察无一登记 edge，本轮复核才发现其中两条是行为级缺口——架构审查 todo 的「边界外观察」节要么登记 edge 条目，要么写明「维持观察的理由 + 复查轮次」，否则观察即丢失。

**Gate 证据（V2 轮实测）**：
- Gate E：02 篇 §5 server.rs 锚点抽验 3/3（:895/:926/:948）；01/02 篇 minix-types 侧 2 处漂移维持（P3-2）；八条语义测试名对账 8/8（§1.2）。
- 锚点纪律：本轮新增条目的关键锚点全部 rg/sed 实测——server.rs:496（m_source）、start.rs:133（lasting 注释）、syscall.rs:923（niced=false）、sched.rs:233（is_preemptible）、proc_table.rs:766/:674（优先级近似）、proc_table.rs:573-586（sched_for_cpu）、transport.rs:41/:45（SYS_* 镜像）、02篇:221/01篇:179（行锚漂移）。
- design-coverage-check：00/99 缺 6 快照（P3-3 维持）。
- 测试基线：`cargo test -p minix-sched` 79 passed / 0 failed；clippy 本体 0 告警（scan-only，无代码改动）。

**收敛评估**：本轮新发现 stage 内 0 P0 / 0 P1 / 1 P2（升格）/ 2 P3，edge 4 条目 + 1 增补；第一轮遗留 9 条 open 全部经 grep 复核（无一虚账）。内核接缝三个行为级发现均为第一轮未见——新发现占比健康，但服务本体已两轮无 P0/P1，边际明显递减，且剩余大头（SMP、trap 层）依赖 01-stage-kernel 工作窗。建议：下一轮触发条件定为「E1/E8 通电后」或「01-stage SMP 落地后」的验证轮，而非时间驱动的例行轮。

---

## 8. 建议推进顺序

1. **P2-1**（注释三重失真修正：start.rs + 06 篇 + 核查 10 篇）——仍是认知地雷，成本最低。
2. **V2-P2-1**（四符号清算批次，一次 todo-fix）。
3. **P2-3 + E-MINTYPES-SYS**（minix-types 一次补 SUSPEND + SYS_* 两族常量，两侧消费改接线）。
4. **edge E-PREEMPTFLAG / E-SCHEDNICED**（行为级两件，随 edge_todo 单线程队列领取；前者优先——有真实的策略旁路后果）。
5. **P2-4**（CpuId newtype 全链）。
6. **V2-P3-1 / V2-P3-2**（随任何触碰 server.rs 的轮次顺带）。
7. **P2-2 / P3-2 / P3-4 / P3-5**（文档项，随对应文档触碰时执行）。
8. **E-SCHEDSMP / E5(e) / E8**（挂 01-stage SMP 与 E1 工作窗，SCHED 侧零改动）。

每次修复遵循 fix-guard（修前读目标行 ±5、grep 确认现状、一次一条、修后 grep 验证并记录），修完跑 `cargo test -p minix-sched` 对照 §0 基线（79 passed）。

---

## 9. V2 执行轮修复记录（2026-09-09 起，一次一个 TODO，每条一个提交）

### ✅ Fix #5: V2-P2-1 — 四符号清算（删 is_valid_quantum / USER_QUANTUM / is_available / IN_USE）

**问题**：主循环落地后四个预留符号仍零生产调用者（V2 复核确认），按第一轮 P3-1 既定标准到达清算时点。

**设计对比**（USER_QUANTUM 的去留，三案）：
1. **直接删（已实施）**：grep 发现 PM/RS 各自已持有 `USER_QUANTUM`（`os/servers/pm/src/sched.rs:27`、`os/servers/rs/src/sched.rs:24`）——C 的消费方本来就是客户端（PM 的 init 申请、RS 的服务默认配额），SCHED 从不读默认配额。删 SCHED 副本比第一轮设想的"移 PM"更干净：目标 crate 里早就有了。
2. 移入 PM crate（否决）：PM/RS 已有定义，再移就是制造第三份。
3. 保留并注释（否决）：零生产调用者的常量是评审记忆税。

**Files**：`os/servers/sched/src/priority.rs`（删 `USER_QUANTUM` + `is_valid_quantum`，`DEFAULT_USER_TIME_SLICE` 文档改讲"两边持有"的故事；`test_quantum_defaults` → `test_default_time_slice`）、`os/servers/sched/src/cpu.rs`（删 `is_available`；恒真宏的分析知识保留在 10 篇 §1 与 `pick` 的 None 跳过里）、`os/servers/sched/src/schedproc.rs`（删 `IN_USE` 常量，位值 0x00001 并入 `SlotState` 文档注释）。

**测试**：删 5 行断言（全属被删符号的自证），`test_default_time_slice` 保留出生初值断言。79 passed / 0 failed（基线不动，纯死代码删除）。

**Verified**：`rg "is_valid_quantum|USER_QUANTUM|is_available|\bIN_USE\b" os/servers/sched/src/` 生产代码零命中；clippy 本体 0 告警。

**Docs**：05 篇（D1/D4/ARCH 表/§4.2 符号表/§5 测试表——`USER_QUANTUM` 的 Rust 归属改写为 PM/RS 客户端持有 + 校验谓词删除的理由）、10 篇（D2/D3/ARCH 表/§4.2/§5——`is_available` 行删除，真过滤即 `pick` 的模式匹配）、03 篇（D2/§4.2/§5——IN_USE 常量删除、位值并入文档注释）。全部行锚按删后行号重校（rg/sed 实测 8/8 命中）。

**边界**：`noquantum_trust` 行闭单（server.rs:244 已消费）；start.rs:132 注释中的 `USER_QUANTUM` 指称 PM 侧常量（pm/src/sched.rs:27 仍存在），注释语义仍真，留待 Fix #7 一并重写该段。

### ✅ Fix #6: P3-4 — lib.rs 唯一的 glob 再导出删除

**问题**：lib.rs:37 的 `pub use sef::*;` 是全 crate 唯一的 glob 再导出，无任何使用者（全仓 `use minix_sched::` 消费点仅 main.rs，且全走全路径）——规则之外的单点，留着就会繁殖（与 P2-3 同型）。
**Files**：`os/servers/sched/src/lib.rs`（删 2 行）。
**Verified**：`cargo test -p minix-sched` 79 passed；全仓 grep 无 `minix_sched::` 短路径消费。
**Docs**：无文档引用该 glob（05 篇 :88 的 `pub use` 指 schedproc 的 NR_SCHED_QUEUES 转引，另一回事，仍有效）。

### ✅ Fix #7: P2-1 — init/BSP 注释失真修正（四处代码注释 + 两篇文档）

**问题**：`plan_start` 文档注释声称「init 的长期效果是 CPU（184 的 BSP 赋值），caller 会把自父种子留在 BSP（10 消费这条规则）」——C 真相是 schedule.c:226 无条件 `pick_cpu` 覆盖一切临时值（含 184）；Rust 真相是 `pick` 根本没有自父规则（server.rs:383 走 `is_system_proc`，init 为假走负载选择）。同一失真还存在于 start.rs 测试注释（"self-parent shape survives for the caller's BSP rule"）、06 篇 :31（"临时值真正起作用的只有 CPU"）与 :89、10 篇 :22（交叉引用 06 一条不存在的"新建进程暂时固定"主张）。
**设计要点**：修正方向 = 让注释与 C 的 `226` 和 Rust 的实际行为同时对齐，并顺带指出 **C 源自己的注释（177-183）已经过时**（写于无条件重选之前）——这正是当初误读的源头，值得在两篇文档里点名，防止下一个读者再被 C 注释带偏。方案对比：只修 Rust 注释留文档旧话（否决：注释与文档互相矛盾更糟）；顺带改 C 源注释（禁止——minix3/ 是 ground truth，不可修改）。
**Files**：`os/servers/sched/src/scheduling/start.rs`（plan_start 文档注释 + test_start_birth 测试注释）、`06-start-scheduling.md`（§1.3 重写 + §2.3 加交叉引用）、`10-pick-cpu-smp.md`（§1.1 删悬挂引用 + 补"只有系统进程这一条"的澄清）。
**Verified**：`cargo test -p minix-sched` 79 passed（注释级改动）；`rg "lasting init effect|self-parented seed|新建进程暂时固定|真正起作用的只有 CPU"` 生产代码与文档零命中（todo.md 的问题描述除外）。

### ✅ Fix #8: P2-4 — CpuId newtype 全链接线

**问题**：`SchedProc.cpu: u32` 是七字段结构里唯一的裸整数——同结构里 `Priority` 有构造保障而 `cpu` 没有，读者得逐字段记哪个数可信；主循环落地后裸 u32 的流转面扩为 6 处签名（Probe/SlotValues/Fanout/CpuLoad/release/表字段）。
**设计对比**（三案）：1. `CpuId(u32)` newtype（已实施）——cpu.rs 定义（CPU 域的类型住 CPU 模块）、`pick` 是唯一生产者（合法性"小于拓扑核数"由它保证，构造不校验，避免把机器信息塞进纯函数）、全链消费；内核 `proc.rs` 的 `CpuId` 同形先例，两侧一个词表。2. 保持 u32 加注释（否决：注释管不住签名，V2 复核已把流转面数清）。3. 构造时校验拓扑（否决：`new` 需要 `&MachineTopology`，纯函数全被污染；台账动词还得返回错误，而越界下标本来就静默忽略）。
**Files**：`cpu.rs`（`CpuId` 定义 + pick/三个台账动词签名 + 测试）、`schedproc.rs`（字段）、`kernel_api/schedule.rs`（SlotValues/Fanout/wire_cpu + 测试）、`scheduling/stop.rs`（Release/plan_stop + 测试）、`server.rs`（Probe/装配/do_start/do_stop + 测试）。线上的 `MessLsysKrnSchedule.cpu` 保持 `i32`（wire 是 C ABI，`wire_cpu` 一处渲染）。
**测试**：`cargo test -p minix-sched` 79 passed / 0 failed；clippy 本体 0 告警。全部断言改 `CpuId(n)` 形式（类型即文档）。
**Docs**：03 篇 D4 改写（"时间片写明单位，CPU 用新类型"+ 为什么补"注释管不住签名"）、07 篇 :116、09 篇 :105、10 篇 D1/D3；六篇文档全部行锚按改后行号重校（cpu.rs 整体 +13、start.rs +7、schedproc.rs +6、schedule.rs +2、stop.rs +1）。


### ✅ Fix #9: V2-P3-2 — Probe 删除，探针事实化（facts-or-refusal）

**问题**：`Probe::read` 对越界 endpoint 构造 dummy 字段（`Priority::new(0).expect`、`index: 0`），依赖注释约定「unread in that case」——类型上「可读的行」与「编造的行」不可区分。
**设计对比**（三案）：1. `row: Option<RowValues>`（todo 首选的保守版）——消费方仍可解包出错的分支。2. **探针事实化（已实施）**：`table.rs` 新增 `OccupiedSlot { index, row }`（占用门通过后的「事实包」）与 `SlotVerdict::from_probe`；`SchedServer` 的 `Probe` 整体删除，换成 `probe_occupied(ep) -> Result<OccupiedSlot, SlotVerdict>` 与 `probe_vacant(ep) -> Result<usize, SlotVerdict>`——这其实是把 C `sched_isokendpt` 「验证并交出行号」的语义直接类型化：拒绝的探针不带任何事实，哑数据在构造上不可能。四个臂改为收 `&Result<事实, SlotVerdict>`（plan_stop 收占用门探针、nice admit 返回 `(ceiling, &OccupiedSlot)`、noquantum admit 返回 `&OccupiedSlot`、plan_inherit 收 `&Result<ParentState, SlotVerdict>`，调用侧 `.map` 只触碰 Ok 半）。3. 维持现状 + debug_assert（否决：断言拦不住类型上的可读）。净收益：零 expect/unreachable、`do_nice` 快照移到门后（反而更贴 C 的语句序 278-279 在检查之后）、`stop::admit` 并入 `plan_stop` 门序、`ParentState` 保留但改为探针映射产物。
**Files**：`table.rs`（OccupiedSlot + from_probe）、`server.rs`（探针方法 + 四臂重写）、`scheduling/{start,stop,nice,noquantum}.rs`（签名 + 测试夹具）。
**测试**：`cargo test -p minix-sched` 79 passed / 0 failed；clippy 本体 0 告警。
**Docs**：02 篇（server.rs 全部行锚 +5 重校）、04 篇（table.rs 行锚 + OccupiedSlot 归属）、06 篇（D4 plan_inherit 形状）、07 篇（D 门序并入 plan_stop）、08 篇（两 admit 签名）——共五篇结构性更新 + 全部行锚重校。


### ✅ Fix #10: V2-P3-1 — 测试补强两小件

**(a) `test_invalid_spender_noquantum_stays_silent`**（server.rs:1079）：内核旗标但 `m_source` 无效（越界 9999 / 负值 task endpoint -4）的 NO_QUANTUM——门内各得其码（OutOfRange/Task），门外全静默：不回件（连错误码都不回，main.c:76 的 continue 是唯一可观察面）、不降级、不下发。补上八条语义第 2 条的边界半。
**(b) `test_clock_rebalances_multiple_slots_in_order`**（server.rs:1100）：三个在册进程（两个降过、一个已在上限）同轮回升——各升一级且只升一级、到上限的整段跳过（C 的 `if` 守护全 body：没动的连下发都没有）、两次 LOCAL 下发、零回件。补上八条语义第 7 条的多进程半。
**测试**：`cargo test -p minix-sched` **81 passed** / 0 failed（79 → 81）；clippy 本体 0 告警。
**Docs**：02 篇 §5 增 (a) 行、11 篇 §5 增 (b) 行（锚点 server.rs:1079/:1100 实测）。
