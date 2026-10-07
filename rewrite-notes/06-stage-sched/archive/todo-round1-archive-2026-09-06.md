# 06-stage-sched Rust 实现架构级 Review TODO

> 来源：2026-09-06 架构级代码审查（查漏补缺 + 整体分层审视，非逐函数审查）。
> 范围：一等对象 `os/servers/sched/src/` 全部 Rust 代码（19 个文件，2848 行，crate 名 `minix-sched`）；内核契约面为辅（`os/kernel/src/proc_table.rs` 的调度包装与通知、`os/kernel/src/syscall.rs` 的 Schedule/Schedctl 入口、`os/kernel/src/sched.rs`、`os/libs/minix-types` 的消息与常量）。
> 方法：三向覆盖矩阵（Minix3 C 源 ↔ 14 篇文档 ↔ Rust 实现）先行查漏补缺，再按「整体 → crate 结构 → 模块 → 类型与函数」四层审视，对照 Redox 实现、操作系统理论、Rust 社区惯例给出多方案建议。
> 定位：本文档是本阶段第一份架构改进建议与覆盖缺口清单。06-stage-sched 此前没有 todo.md；本文档不复写 `plan.md`（文档重组计划，2026-08-16 生效），两者各管一头。
> 状态（2026-09-06 第二次更新）：**P1-1 / P1-2 / P1-3 已修复**（2026-09-06 主循环落地，见 §9 修复记录；测试基线 59 → 79 passed，minix-sched 本体 clippy 0 告警，生产二进制可构建）。P2-1（init-BSP 语义修正，建议下一迭代最先领取）与 P2-2 ~ P3-5 为待处理条目，按 §8 推进顺序逐项领取（todo-fix / cmd-05），修复时文档与代码一并改。第一轮审查完成于 2026-09-06（共 12 条：P1 三条、P2 四条、P3 五条，P0 为零），本轮仅审查未修代码。

---

## 0. 审查结论速览

一句话总结：纯函数层的重写质量很高——C 的门序、错误码、消息线序在 59 个测试里逐一锁定，全部通过；真正的缺口不在「已写的代码」，而在「还没接上的代码」：主循环、状态所有者、传输接缝三件事缺位，服务器目前无法运行。另有四处语义与归属层面的改进点、五项卫生项。

| 级别 | 条目 | 一句话 |
|------|------|--------|
| P0 | （无） | 第一轮未发现正确性级缺陷，自检说明见 §2 |
| P1 | P1-1 | 主循环与组合层缺失（**✅ 已修复** 2026-09-06，见 §9） |
| P1 | P1-2 | 组合层「状态所有者」未设计（**✅ 已修复** 2026-09-06，见 §9） |
| P1 | P1-3 | 内核传输接缝无抽象（**✅ 已修复** 2026-09-06，见 §9；真实通电挂 edge E8） |
| P2 | P2-1 | init 临时值「长期作用于 BSP」的语义误读（注释与 06 篇共两处），会误导主循环实现 |
| P2 | P2-2 | 14 篇（RS 交互）的 Rust 实现归属未声明（跨 crate 分布） |
| P2 | P2-3 | `SUSPEND` 常量本地定义，与 com.h 常量一律走 minix-types 的镜像原则不一致 |
| P2 | P2-4 | `SchedProc.cpu` 裸 `u32`，与同结构内 `Priority` 的类型纪律不一致 |
| P3 | P3-1 | 五个预留未接线符号的盘点（主循环落地后必须接线，否则删） |
| P3 | P3-2 | 文档第 5 节测试表两处 minix-types 侧行锚点漂移 |
| P3 | P3-3 | 00 与 99 两篇缺 `.design/` 快照（预检脚本实测） |
| P3 | P3-4 | `lib.rs` 唯一的 glob 再导出没有被任何调用方使用 |
| P3 | P3-5 | 11 篇可补 Redox 权重轮转（DWRR）演进参照，接上已有的策略演进预留 |

验证命令基线（2026-09-06 实测，后续修复轮以此为对照）：
- `cargo test -p minix-sched`：**59 passed / 0 failed**（库目标；二进制目标 0 个测试）
- `cargo clippy -p minix-sched --all-targets`：minix-sched 本体 **0 条告警**（workspace 级尚有依赖 crate 的历史告警，不属本阶段）
- `tools/design-coverage-check.sh fork-syscall-rewrite --stage 06-stage-sched`：01~14 三种快照齐备；00、99 各缺三个快照（见 P3-3）
- 测试名对账（Gate E 抽查 + 全量核对）：14 篇文档第 5 节声称的测试函数在代码中全部存在；两处 minix-types 侧行号锚点过期（见 P3-2）

---

## 1. 覆盖矩阵：C 源 ↔ 文档 ↔ Rust（查漏补缺主产出）

### 1.1 三向映射表

判定分三档：**已实现**（C 概念有对应的 Rust 模块与测试）、**判定层已实现**（决策逻辑在、执行动作留给调用方——这是本 crate 的既定分工，见各模块文档头）、**缺口**（无 Rust 形态，指向具体条目）。

| C 概念（锚点） | 文档 | Rust | 判定 |
|---|---|---|---|
| `main()` 主循环：收件、分流、回件（`servers/sched/main.c:22-96`） | 01 + 02 | `main.rs:12-43`，循环体是 `empty_loop` 占位（`main.rs:40-41`） | **缺口** → P1-1 |
| SEF 启动：fresh 与 restart 两种注册、读机器信息（`main.c:111-136`） | 01 | `sef.rs:19-58`、`main.rs:16-32` | 已实现；但 `sys_getmachine` 传输用硬编码 mock 顶替（`main.rs:28-31`），归入 P1-1/P1-3 |
| 消息面：五种编号、通知先于调用、SUSPEND 不回复、未知编号拒收（`main.c:35-96`、`utility.c:18-23`） | 02 | `dispatch.rs` 全模块 | 已实现（判定层） |
| `struct schedproc` 七个字段，第八字段 cpu_mask 结构性删除（`schedproc.h:23-40`） | 03 | `schedproc.rs:65-101` | 已实现 |
| 槽位四道判断与镜像门（`utility.c:29-56`）、发送者白名单（`utility.c:61-74`） | 04 | `table.rs`、`valid.rs` | 已实现 |
| 队列常量表、毫秒时间片、nice 折算、系统进程判断、niced 谓词（`config.h:66-77`、`pm/utility.c:91-101`、`schedule.c:41-44,319`） | 05 | `priority.rs` | 已实现 |
| `do_start_scheduling`：双信同一门、三种出生值、EBADCPU 重试环（`schedule.c:140-249`） | 06 | `scheduling/start.rs` | 判定层已实现；`sys_schedctl` 接管调用与重试环的执行侧随主循环（P1-1） |
| `do_stop_scheduling`：两道门、台账扣减、清占用（`schedule.c:112-135`） | 07 | `scheduling/stop.rs` | 判定层已实现；执行侧随主循环（P1-1） |
| `do_noquantum` 降一级、`do_nice` 改上限带回滚（`schedule.c:87-107,254-292`） | 08 | `scheduling/noquantum.rs`、`scheduling/nice.rs` | 判定层已实现；下发执行侧随主循环（P1-1） |
| `schedule_process` 按掩码下发、-1 保持哨兵（`schedule.c:297-328`） | 09 | `kernel_api/schedule.rs` | 已实现（打包层）；`sys_schedule` 发送随 P1-3 |
| `pick_cpu` 三条规则与 `cpu_proc[]` 台账（`schedule.c:37-81`） | 10 | `cpu.rs` | 选择层已实现；台账持有者未定（P1-2） |
| `init_scheduling` 五秒闹钟、`balance_queues` 逐轮回升（`schedule.c:334-369`） | 11 | `balancer.rs` | 数值层已实现；`sys_setalarm` 布防与表遍历执行侧随主循环（P1-1） |
| `do_schedctl` 注册/接管契约（`kernel/system/do_schedctl.c:7-46`）；NO_QUANTUM 通知负载（`ipc/message.rs:1057-1075` 七个统计字段） | 12 | `kernel_api/schedctl.rs` | 已实现（打包层）；通知接收与解码随主循环（P1-1） |
| PM 侧行程：三路分发、START 四字段、INHERIT 三字段（`lib/libsys/sched_start.c:11-97`、`sched_stop.c:9-29`） | 13 | `client.rs` | 已实现（镜像层） |
| RS 侧行程：RS 先申请再生（`rs/manager.c:461`、`rs/request.c:342`） | 14 | 服务端视角由 `valid.rs:14-21` 覆盖（RS 是放行发送者）；客户端实现位于 `os/servers/rs/src/request.rs:9` 等，属 03-stage-rs 域 | **归属未声明** → P2-2 |

结论：14 篇文档对应的 C 概念全部有文档承载；Rust 侧 13 篇有对应模块，唯一悬空的是第 14 篇的跨 crate 归属（P2-2）与所有「执行侧」动作的共同前置（P1-1）。查漏补缺没有发现「C 有、文档与 Rust 双双漏掉」的概念——唯一接近的是 `schedule.c:319` 的 niced 计算与 `schedule.c:348-352` 的「策略将来会换」注释，前者已实现（`priority.rs:202-204`），后者已被 `balancer.rs:33-36` 的架构预留（S-9）显式接住。

### 1.2 执行侧语义清单（主循环落地时必须逐项锁定）

这八条是 C 主循环与各处理器里「判定层管不到」的行为。它们目前没有任何测试承载——主循环（P1-1）落地时，每一条都应有一个对应的测试用例锁住，防止实现时凭直觉走样：

1. 回件失败只告警、不崩溃、不重试（`main.c:101-106`）。
2. 内核来源的 NO_QUANTUM：处理成功与失败都不回件，失败只打印（`main.c:68-77`）。
3. 伪造的 NO_QUANTUM：以 EPERM 回件（`main.c:78-83`）；「来源是内核」的判据是消息标志位 `IPC_FLG_MSG_FROM_KERNEL`（`ipcconst.h:28`），判定函数已在 `dispatch.rs:86-88`。
4. CLOCK 通知触发队列回升，其余通知一律静默，所有通知永不回件（`main.c:44-55`）。
5. NO_QUANTUM 的下发失败**不回滚**已写入的降级、照实返回错误（`schedule.c:99-105`）；而 NICE 的下发失败**要回滚**（`schedule.c:284-288`）。两条对称结构的处理不对称，这是 C 的真实语义，不是笔误。
6. START 路径上 `sys_schedctl` 失败时槽位尚未标记占用、直接返回（`schedule.c:218-222`）；而随后的下发失败时槽位**已**标记占用并残留（`schedule.c:223` 先置、`schedule.c:227-237` 后败）。失败残留是 C 的既有行为。
7. 队列回升的表遍历是「发了就不管」：逐个下发、忽略返回值（`schedule.c:358-364`）。
8. START 成功后，回件消息的 scheduler 字段改写为本服务器编号（`schedule.c:246`；回件负载定义在 `ipc/message.rs:1187`）。

---

## 2. P0：真实缺陷（本轮为零）

第一轮审查未发现正确性级缺陷。这不是「没查到」而是有核对依据的：判定层的每个处理器与 C 的门序逐一对照过（start.rs 的三道门次序对照 `schedule.c:150-166`，nice.rs 对照 `schedule.c:262-276`，table.rs 的四道判断对照 `utility.c:31-40`），错误码值有断言锁定（`table.rs:117` 的 `(EBADEPT, EINVAL, EDEADEPT) == (216, 22, 215)`），消息线序有断言锁定（`schedctl.rs:209-216`、`client.rs:265-278`）。已按收敛规则做漏检自检：随机重验了三个语义点——NICE 回滚、NO_QUANTUM 不回滚（见 §1.2 第 5 条）、START 失败残留（第 6 条）——三者 Rust 侧的表述与 C 原文一致，未发现新问题。

需要说明：主循环缺失（P1-1）意味着「外部可观察行为」整体尚未达成，但这属于阶段进行中的完成度缺口而非已写代码的错误，按本项目的分级惯例记 P1 而非 P0。

---

## 3. P1：架构级问题（建议尽快规划）

### P1-1 主循环与组合层缺失：服务器不可运行

**问题**：`main.rs:40-41` 的主循环是空转占位（注释自述「等 02 篇落地循环体」）。C 的主循环形状——收一条消息、先分辨通知与调用、通知里只认时钟、调用里认五种编号、除 SUSPEND 外一律回件（`main.c:35-96`）——在 Rust 侧没有任何实现。现有 11 个模块全部是纯判定或纯打包函数，彼此之间没有一根线连起来：`dispatch::classify` 的输出没有人消费，`scheduling::start::plan_start` 的产出没有人写入表，`balancer::rebalance_one` 的判定没有人驱动遍历。

**影响**：这个阶段的所有价值都押在「判定层正确」上，但判定层无法自证——§1.2 的八条执行侧语义没有测试承载，59 个测试全部是单元级。服务器不可运行，也意味着后续任何阶段（PM 联调、RS 联调）都无法以它为对端。

**建议**：
1. **首选**：在库内实现一个 `run_loop`（或等价的逐步驱动函数），以传输 trait 注入依赖（见 P1-3）；`main.rs` 只做真实传输的装配。C 的循环体只有六十行（`main.c:35-96`），Rust 侧的对应物不该更复杂：分类（`dispatch::classify`）→ 按类型进各臂（`scheduling::*` 的判定 + 自己写表）→ 结算回件（`dispatch::settle`）。
2. **次选**：不写无限循环，改写「单步函数」`step(&mut state, event) -> Option<Reply>`，真实循环只是 `while` 壳。单步化让 §1.2 的八条语义都能用表驱动测试锁定，比「起真循环、发真消息」的集成测试成本低一个数量级，也符合本 crate「单线程事件循环、纯函数无共享状态」的既有文档头承诺（如 `scheduling/start.rs:13`）。
3. 两条路线都应把 §1.2 的八条作为验收清单逐条对测试；文档侧的落点是 02 篇（`main.rs:35-39` 的注释已把循环体归属指到 02 篇）。

### P1-2 组合层「状态所有者」未设计

**问题**：C 侧的状态是四个文件级全局：进程表 `schedproc[NR_PROCS]`（`schedproc.h:36`，NR_PROCS 为 256，Rust 侧常量在 `minix-types/src/types/com.rs:38`）、CPU 负载台账 `cpu_proc[]`（`schedule.c:46`）、机器信息 `machine`（`main.c:17`）、平衡周期 `balance_timeout`（`schedule.c:16`）。Rust 侧这四块状态全部「留给了调用方」（各模块文档头反复声明表由调用方持有，如 `table.rs:7-10`），但调用方不存在，四块状态由谁持有、怎么组织，没有任何设计落点。

**影响**：主循环（P1-1）落地时如果没有单一所有者，状态会散落在 `main.rs` 的局部变量里，借用检查会推着实现走向「把四块状态拆成参数逐层传递」的形态。02-stage-vm 的同型教训记在其 todo.md 的 P2-3：为绕借用检查拆出四元组访问器，最终成为结构性债务。

**建议**：
1. **首选**：单一 `Sched` 结构体持有四块状态（表、台账、平衡器、机器拓扑），主循环每轮 `step(&mut sched, transport)`。单线程事件循环里一个 `&mut Sched` 走天下：无锁、无内部可变性、表与台账的交叉读写（START 要同时改两者，`schedule.c:223-231`）天然免拆借用。这与各模块「纯函数、无共享状态」的既有承诺方向一致——状态集中在所有者，函数保持纯。
2. **次选**：按 C 的领域边界拆两个持有者（进程表一个、机器与台账一个），保持与 C 全局的一一对应，方便逐行对照。
3. 这是架构级决策：落地时应按项目规范标注 `[ARCH: ...]` 并保持文档、设计、代码三处一致。设计落点建议 02 篇扩展或新增一篇（由用户决定，本文档不自动建文档）。

### P1-3 内核传输接缝无抽象：只打包、不发送

**问题**：`kernel_api` 两个子模块的职责自述是「打包与前置校验，发送留在调用方」（`schedctl.rs:8-14`、`schedule.rs:10-15`）。但「发送」这一侧没有任何抽象：接收消息、判定内核来源标志位（`ipcconst.h:28`）、`sys_getmachine`、`sys_schedctl`、`sys_schedule`、`sys_setalarm`、回件——七个传输动作既没有 trait 也没有实现。`main.rs:28-31` 目前用硬编码的一核机器信息顶替 `sys_getmachine`，即是一处症状。

**影响**：P1-1 的主循环无论写成什么形状，只要传输没有接缝，循环逻辑就无法在没有真内核的环境下测试；§1.2 的八条语义将只能靠人眼审。02-stage-vm 有同型前车：其 todo.md 的 P1-3 记录了「IPC 传输生产路径不可运行且不可测」，后来不得不专项补课。

**建议**：
1. **首选**：在 `kernel_api` 下新增 `transport` 模块，定义一个最小 trait，七个方法对应七个传输动作，错误一律返回 Minix3 errno 的 `i32`（与全 crate 的错误纪律一致）。库内逻辑只依赖 trait；二进制装配真实实现（走 `minix-sys`，该依赖已在 `Cargo.toml` 声明）；测试装配内存 mock。SCHED 的传输面就这么大，自建小 trait 的成本远低于返工。
2. **次选**：仿照内核侧 `IpcEngine` 的抽象风格（`os/kernel/src/proc_table.rs:722-730` 的 `notify_scheduler` 用法）。不推荐作首选：那是内核借用车道的形状，用户态单线程服务器用不上它的借用结构，照搬会引入不适配的复杂度。
3. 对照 Redox：用户态服务通过 libredox 的系统调用层与内核往来，那一层正是可替换接缝（来源：[Redox Book — Scheduling](https://doc.redox-os.org/book/scheduling.html) 与 [redox-os/kernel](https://github.com/redox-os/kernel) 仓库结构）。形状一致：策略逻辑依赖窄接口，传输实现可换。

---

## 4. P2：结构性改进

### P2-1 init 临时值「长期作用于 BSP」的语义误读

**问题**：`scheduling/start.rs:133-135` 的注释与 06 篇（`06-start-scheduling.md:31`、`:89`）共同声称：init 自父分支里的临时值「真正起作用的只有 CPU（BSP，`schedule.c:184`），数字部分都会被覆盖」。但对照 C 原文，这个说法不成立：`do_start_scheduling` 在接管调用之后**无条件**调用 `pick_cpu`（`schedule.c:226`），而 init 的父进程是它自己、不是 RS，不满足系统进程判断（`schedule.c:44`），所以在多核机器上 init 走的是「负载最低的非 BSP 核」选择（`schedule.c:67-76`），BSP 只是托底（`schedule.c:65-66`）；单核机器上 `pick_cpu` 自己就会选 BSP（`schedule.c:54-57`）。两条路合起来：`schedule.c:184` 的 BSP 赋值在任何路径上都会被覆盖，init 的临时值——包括 CPU——最终作用是零。

**影响**：今天没有行为偏差（出生计划 `Seed` 根本不携带 cpu 字段，`start.rs:74-85`），所以不是 P0。但这是一颗定时雷：主循环（P1-1）落地时，实现者若按注释写「自父出生的种子固定落 BSP」，就会与 C 漂移——C 的 init 在多核机器上完全可能落在别核。

**建议**：
1. **首选**：修正两处表述为「init 的临时值（数字与 CPU）全部被后续步骤覆盖，最终 CPU 一律由 `pick_cpu` 决定（`schedule.c:226`），init 没有任何特例残留」，并顺带补一句多核路径的行为。Ground Truth 链是 C 源 > 文档 > 代码，`schedule.c:226` 的无条件覆盖是原文事实。
2. **次选**：若想保留「BSP 起步」的叙述（毕竟单核下结果等价），必须显式限定为「单核机器上的巧合等价，多核下 init 与普通进程同样参与负载选择」。
**验证**：`rg -n "lasting" os/servers/sched/src/scheduling/start.rs`；`rg -n "真正起作用" rewrite-notes/06-stage-sched/06-start-scheduling.md`。

### P2-2 14 篇（RS 交互）的 Rust 实现归属未声明

**问题**：14 篇描述的行为主体是 RS（重生服务器）：先申请、再生成、取消失败按位置分两种处理（对应 `rs/manager.c:461`、`rs/request.c:342`）。这个行为主体不在本 crate——`lib.rs:22-32` 的模块清单里没有第 14 篇的对应物；RS 侧的客户端实现实际存在于 `os/servers/rs/src/request.rs:9`（注释中列有 `sched_stop`）等文件，属于 03-stage-rs 的领域。本 crate 里与第 14 篇相关的只有服务端视角：RS 是放行发送者之一（`valid.rs:14-21`，对应 C 的 `utility.c:64-73`）。

**影响**：后续任何一轮覆盖率审查都会把「14 篇没有 Rust 模块」误判为覆盖缺口；反过来，RS 侧行为出回归时，责任归属也会含糊。文档-代码同步的双向闭环断在阶段边界上。

**建议**：
1. **首选**：14 篇补一段「实现归属」声明，写清三层：服务端视角在 `valid.rs`（本阶段）、契约镜像在 `client.rs`（PM 与 RS 共用同一 libsys 客户端契约，`sched_start.c`/`sched_stop.c` 不分 caller）、行为主体在 `os/servers/rs/`（03-stage-rs），并给出具体文件锚点。同时在 `client.rs` 的模块文档头补一句「本模块是契约镜像，不区分 PM 与 RS」。
2. **次选**：在 99-global-concepts 的导航表加一行跨阶段归属说明，集中登记所有「文档在本阶段、实现对端在别阶段」的条目（第 14 篇是首个，未必是最后一个）。

### P2-3 `SUSPEND` 常量的归属与镜像原则不一致

**问题**：`dispatch.rs:16-21` 在本 crate 本地定义 `SUSPEND: i32 = -998`，注释给出的理由是「VFS 在自己的 crate 也定义了同一个值；不为一个数引依赖」。但 C 的 ground truth 里 `SUSPEND` 是 com.h 的全局常量（`minix3/minix/include/minix/com.h:1151`，已核验原文），而本 crate 通过 minix-types 镜像了 com.h 的全部 SCHEDULING 系常量（`minix-types/src/types/com.rs:85-96`）。同为 com.h 常量，一种走共享镜像、一种走各处手抄——同一个 crate 里出现了两套归属标准。

**影响**：数值漂移的风险目前被测试锁死（`dispatch.rs:168` 断言 `SUSPEND == -998`），所以不是正确性问题；真正的成本是规则被例外蛀空：下一个服务器还会再抄一份，每个抄写点都是一个独立的对账对象。

**建议**：
1. **首选**：把 `SUSPEND` 移入 minix-types 的 com 模块（紧邻 SCHEDULING 系常量），`dispatch.rs` 改为再导出；VFS 侧的同名定义后续跟进（不在本阶段动，登记即可）。
2. **次选**：保留本地定义，但把注释里的理由从「避免依赖」改为明说的「镜像规则例外」，并登记到 99-global-concepts 的常量表，让例外有账可查。

### P2-4 `SchedProc.cpu` 的裸类型与同结构的类型纪律不一致

**问题**：`SchedProc` 里 `Priority` 是带范围构造的 newtype（`schedproc.rs:46-63`），线上值 `CpuChoice` 与 `Nice` 也有类型（`priority.rs:86-172`），唯独 `cpu` 是裸 `u32`（`schedproc.rs:93`），CPU 台账同样是裸 `Option<u32>`（`cpu.rs:37`）。cpu 值的合法性（小于机器核数）完全依赖运行期 `pick` 的逻辑保证，类型系统里不可见。

**影响**：行为风险很低——SCHED 不做核亲和（cpu_mask 已按 S-3 结构性删除，`schedproc.rs:66-73`），cpu 的唯一生产者是 `cpu::pick`。成本是可读性与评审记忆：同一个结构里「哪个字段有构造保障」需要逐个记。

**建议**：
1. **首选**：引入 `CpuId(u32)` newtype，由 `cpu::pick` 返回、`SchedProc.cpu` 与台账共用；内核侧已有同名先例（`os/kernel/src/proc.rs:471` 的 `CpuId(u32)`），两侧术语一致。构造不做拓扑校验（保持纯函数、避免把机器信息塞进表结构），类型的价值在签名可见，不在运行期检查。
2. **次选**：维持裸 `u32`，在字段注释里写明「范围由 pick 保证、构造不校验」的理由，把隐式约定显式化。

---

## 5. P3：卫生与观察

### P3-1 预留未接线符号盘点（死代码显式子轮）

全 crate 无 `#[allow(dead_code)]`、无 `todo!`/`unimplemented!`（grep 实测零命中）——状态很干净。但有一批「判定层 API」当前生产路径零调用、仅测试引用，它们是主循环（P1-1）的接口预留。逐项判定如下，共同标准：**P1-1 落地后仍无生产调用者，即按死代码消除流程逐项删**（本轮不动）：

| 符号 | 锚点 | 判定 |
|---|---|---|
| `noquantum_trust` | `dispatch.rs:86-88` | 保留：伪造 NO_QUANTUM 的门（`main.c:70-77`），主循环必接 |
| `is_valid_quantum` | `priority.rs:180-182` | 倾向删：C 的时间片校验在内核（`system.c:648-649`，原文「小于 1 且不等于 -1 拒绝」已核验），SCHED 侧不校验、原样存储（`schedule.c:196`、`start.rs:148`）；若主循环也用不上，一个无人到访的「唯一的家」不是家 |
| `USER_QUANTUM` | `priority.rs:52` | 待定：语义上属于 PM 侧的起步默认（05 篇已论证它与 `DEFAULT_USER_TIME_SLICE` 来源之别，`priority.rs:54-61`）；若 13 篇镜像路径与本 crate 都用不上，删或移 PM crate |
| `is_available` | `cpu.rs:47-49` | 倾向内联或删：`pick` 的循环体没有用它（`cpu.rs:80-92` 直接对 `Option` 模式匹配），仅测试引用 |
| `IN_USE` | `schedproc.rs:23` | 候选删：注释自称「给 C 读者的便签」，占用语义已由 `SlotState` 完整替代（`schedproc.rs:29-35`）；留一行注释即可，不必留常量 |

### P3-2 文档第 5 节测试表两处 minix-types 侧行锚点漂移

- 02 篇（`02-sched-message-surface.md:191`）声称 `test_sched_message_layouts` 位于 `os/libs/minix-types/src/ipc/message.rs:2477`，实测在 `message.rs:3248`。
- 01 篇（`01-sched-init-main.md:177`）声称 `test_sched_messages` 位于 `os/libs/minix-types/src/types/com.rs:223`，实测在 `com.rs:312`。

测试本体存在、断言有效（Gate E 的名称对账全部通过），漂移的只是行号锚。建议随下次触碰对应文档时顺手修（归属 style-fix 或 full-review 的锚点纪律门），不单独立项。

### P3-3 00 与 99 两篇缺 `.design/` 快照

`tools/design-coverage-check.sh fork-syscall-rewrite --stage 06-stage-sched` 实测：01~14 三种快照齐备；`00-sched-overview` 与 `99-global-concepts` 各缺 outline、outline-review、design 共六个文件（脚本判定 H.1 + H.6 FAIL）。两篇在 `plan.md` 与各自正文里定位为「pending 最小骨架」。按流程这属于 Step 0.3 嵌入生成的范围、不阻断审查，此处登记以保证 Gate H 证据链完整；是否补齐由文档排期决定。

### P3-4 `lib.rs` 唯一的 glob 再导出没有被使用

`lib.rs:34` 的 `pub use sef::*;` 是全 crate 唯一的 glob 再导出，其余十个模块一律走全路径。而实际的二进制入口引用的也是全路径（`main.rs:17` 的 `use minix_sched::sef::{MachineInfo, init_fresh}`）——这行 glob 没有任何使用者。建议直接删除，调用方统一全路径；或写明保留理由。无理由的例外与 P2-3 是同一种形态：规则之外的单点，留着就会繁殖。

### P3-5 11 篇可补 Redox 权重轮转（DWRR）演进参照

`balancer.rs:33-36` 的架构预留（S-9）已经承诺：「第二个平衡策略到来时，以 trait 形式到达，不提前一天」。Redox 恰好提供了一个现成的下一代策略参照：其内核调度正在从简单轮转迁移到按权重的亏欠轮转（Deficit Weighted Round Robin，按核组织优先级队列），官方报道见 [RSoC 2026: A new CPU scheduler for Redox](https://www.redox-os.org/news/rsoc-dwrr/)（Phoronix 的独立报道见 [Redox OS New CPU Scheduler](https://www.phoronix.com/news/Redox-OS-New-CPU-Sched)）。建议在 11 篇的展望处补一句对照（不展开实现），让未来接 S-9 trait 的人有现成的路标。属文档增强，可选。

---

## 6. 对照参考：Redox / 操作系统理论 / Rust 社区

**Redox（联网核实）**：Redox 的调度机制全程在内核：历史上的简单轮转（[Redox Book — Scheduling](https://doc.redox-os.org/book/scheduling.html)；[context/switch.rs](https://github.com/redox-os/kernel/blob/master/src/context/switch.rs)），正在演进为按权重的亏欠轮转（[RSoC 2026 公告](https://www.redox-os.org/news/rsoc-dwrr/)）。Minix3 走的是另一条路：机制留内核（队列、记账、抢占，属 01-stage-kernel），策略上移用户态服务器（本 crate）。两条路线的分界正是 S-9 预留的那个策略接缝——对 SCHED 的直接启示是：策略层保持可替换（P3-5 的参照系），机制契约面（12 篇）保持稳定不动。

**操作系统理论**：本调度器的策略是教科书式多级反馈队列的「降快升慢」形态——时间片耗尽降一级（`schedule.c:99-101`），每五秒回升一级、升到上限即停（`schedule.c:353-364`）。Rust 侧把两个方向做成了对称的纯函数（`noquantum.rs:52-55` 的 `demote` 与 `balancer.rs:73-81` 的 `rebalance_one`），这个形状值得保持：将来换策略（S-9）时，替换的只是判定，形状不动。C 源注释自己说「这个默认策略很快会换」（`schedule.c:348-352`），S-9 正是对这句话的兑现承诺。

**Rust 社区**：错误处理符合项目纪律——没有 `Box<dyn Error>`，所有错误是 Minix3 errno 的 `i32`，且映射有单一出口（`table.rs:38-45` 的 `SlotVerdict::errno`）。类型层面 newtype 与枚举用得克制而到位（`Priority` 的范围构造、`CpuChoice` 消灭 -1 哨兵、`Nice` 把范围检查前移到构造）。主循环落地前最后一块基础设施是传输注入（P1-3）——端口与适配器形状，这也是 no_std 用户态服务 crate 的通行做法：逻辑依赖窄接口，真实传输在二进制装配。

---

## 7. 边界外观察：内核契约面（为辅，建议转记 01-stage-kernel/todo.md）

以下三条属于内核侧（01-stage-kernel 文档域），本文档只记录现象与锚点，不做修复规划；是否转记由用户决定：

1. `SYS_NICE` 未接线（`os/kernel/src/syscall.rs:905` 注释自述），`sched_proc` 的 niced 参数因此恒为 false。nice 链路（PM → `SCHEDULING_SET_NICE` → `do_nice` → 下发的 niced 位）在内核侧断最后一环。
2. 每核运行队列是 TODO（`os/kernel/src/proc_table.rs:487,496`）：`sched_for_cpu` 恒返 BSP 队列。SCHED 下发的 cpu 字段因此暂无实际效果——单核下与 C 的非 SMP 构建语义等价（`schedule.c:78-80`），不构成行为错误，但多核落地前它是硬前提。
3. `sched_proc` 的 CPU 校验是单机桩（`os/kernel/src/sched.rs:270,300,355`），`EBADCPU` 不会触发——06 篇的重试环（`start.rs:197-203`）在当前内核上不可达，属内核 SMP 依赖，非本 crate 缺陷。

---

## 8. 建议的推进顺序

1. **P2-1**（init 临时值语义修正，注释与 06 篇两处）——先修认知再写代码：它直接决定主循环里 init 分支怎么写，且是全部条目里成本最低的一个。
2. **P1-2 + P1-3**（状态所有者设计 + 传输接缝）——两项一起定：产出一篇组合层设计（落点由用户决定），标注 `[ARCH: ...]` 三处一致。
3. **P1-1**（主循环 + §1.2 八条语义逐条测试）——依赖第 2 步的形状决定。
4. **P2-3 / P2-4 / P3-1 / P3-4**（结构小项）——随主循环落地顺手按清单处理；P3-1 的五个符号以「落地后仍有生产调用者」为存留标准。
5. **P2-2 / P3-2 / P3-3 / P3-5**（文档项）——随下一次触碰对应文档时执行，不单独排队。

每次修复遵循 fix-guard（修前读目标行前后五行、grep 确认现状、一次只修一条、修后 grep 验证并记录状态），修完跑 `cargo test -p minix-sched` 对照 §0 基线。

---

## 9. 修复记录（2026-09-06，迭代一：P1-1 + P1-2 + P1-3 合并执行）

一次 todo-fix 领取三条：P1-1（主循环）的两个前置（P1-2 状态所有者、P1-3 传输接缝）按 §8 的依赖关系一并定案——这正是 §8 第 2、3 步的合并，对齐 04-stage-pm campaign 的批次先例（其 todo.md 的 T5→T6→T9 模式）。P2-1 及其余条目未动。

### Fix #1 传输接缝（P1-3）：`os/servers/sched/src/kernel_api/transport.rs`（新文件）

**设计对比**（三案）：
1. **双 trait（已实施）**：`IpcTransport`（收/发）+ `KernelApi`（get_machine/get_hz/schedctl/schedule/setalarm），错误一律 errno `i32`。对照：VM 双 trait（`os/servers/vm/src/ipc/transport.rs:120` 的策略 trait + KernelGateway）、RS 单 `KernelApi`（`os/servers/rs/src/boot.rs:69`，全量定义 + 生产 DEFERRED）、Linux 的 `file_operations` 按子系统分表、Redox 单一 libredox 边界。选双 trait 的理由：C 本就走两条通道（`ipc_send` 与 `sys_*`），两半的 mock 形状不同（消息线要脚本化收发、内核线要参数账本），合一则每个测试写两半。
2. 单 trait 七方法（否决）：接口宽度不随 mock 形状走。
3. 复用内核侧 `IpcEngine` 借用风格（否决）：内核借用车道对用户态单线程服务器是错配（第一轮审查 P1-3 建议次选的结论维持）。

**要点**：
- 真实端按最终形态委托 minix-sys（`DirectTrapTransport` + `perform_kernel_call`，`syscall.rs:201` 的 ENOTREADY 重试环即 C `_kernel_call` 原文），E1 落地前回 `EIO`——诚实契约（模式 60；VM campaign T9 先例）。
- `sys_getmachine` 兑现 GETMINFO 指针契约：`MachineInfoBuf` 与 C `struct machine`（`type.h:122-131`）逐字段同布局（`#[repr(C)]`），内核 safecopy 写入（`os/kernel/src/misc.rs:1038-1050` 对端）。
- `is_notify`（C `com.h:92` 的 Rust 对应物）落在本模块——minix-sys 只有调用号常量没有谓词（`ipc.rs:53,155`）。
- `SYS_SCHEDULE = 0x600+3`、`SYS_SCHEDCTL = 0x600+54` 常量镜像（C `com.h:210,262`；内核 `syscall.rs:69,114` 同值）——minix-types 暂缺，镜像注明「裁判持真相，镜子记值」（`schedctl.rs:27` 先例）。

### Fix #2 状态所有者（P1-2）：`os/servers/sched/src/server.rs` 的 `SchedServer`

**设计对比**（两案）：
1. **单一所有者（已实施）**：`SchedServer` 折进四块状态（表 `[SchedProc; 256]`、台账 `[CpuLoad; 32]`、`MachineTopology`、`Option<Balancer>`），单线程事件循环一个 `&mut` 走全轮。出生路径一口气同写表与台账（`schedule.c:223-231`），借用不拆。`[ARCH S-11]` 三处一致：plan.md ARCH 表 S-11 行 + 02 篇 D8 + `server.rs` 模块注释。
2. 双持有者对应 C 两文件（否决）：C 的全局分文件是历史产物，不是领域边界（02-stage-vm P2-3 的四元组教训引以为戒）。

**要点**：台账初值 `Some(0)` 对齐 C 静态数组零初始化（每核存活、零负载；`None` 是判死 10 D2，不是未探测）。`MAX_CPUS = 32` 对齐内核上限（`os/kernel/src/smp.rs:61`）。

### Fix #3 主循环（P1-1）：`server.rs` 的 `run_once`/`run` + `main.rs` 装配

**设计对比**（两案）：
1. **`run_once` + `run` 薄壳（已实施）**：一轮收分回一个函数，`run` 只加「永远」与失败上限；todo.md §1.2 八条执行侧语义逐轮可测。对照 VM 范本（`os/servers/vm/src/vm_server.rs` 的 `run`/`run_once` + `MAX_CONSECUTIVE_RECV_FAILURES = 64`）。
2. 直译 `while (TRUE)`（否决）：无限循环不可测，八条语义无处落测试。

**八条语义的落点**（每条一个点名测试，见 Fix #4）：
1. 回件失败只丢一次，循环继续（`main.c:101-106`）——`run_once` 尾部忽略 `send` 的 `Err`（打印无日志设施，注释注明该诊断省略）；
2. 内核 NO_QUANTUM 成功失败都不回件（`main.c:70-77`）——信任门臂提前返回 `Step::Handled`；
3. 伪造 NO_QUANTUM 以 EPERM 回件（`main.c:78-83`）——`settle(EPERM)` 走正常回复规则；
4. 通知永不回复、CLOCK 才整理（`main.c:44-55`）——`is_notify` 分流在分发之前；
5. NO_QUANTUM 不回滚、NICE 回滚（`schedule.c:99-105` 对 `284-288`）——`do_noquantum` 直接返回错误码，`do_nice` 失败写回快照；
6. START 失败残留（`schedule.c:223` 先于 `233-237`）——`do_start` 按序：门 → `schedctl` 接管（失败槽未动）→ 置占用 → pick → EBADCPU 重试环（`mark_dead` + 台账随行）→ 下发失败返回错误码但槽已占用；
7. 平衡遍历 fire-and-forget（`schedule.c:358-364`）——`balance_queues` 忽略下发结果；再设闹钟失败向上抛（`schedule.c:367-368` C 原地 panic，库层返回 `Err`、二进制层 panic——01 篇 D2 约定）；
8. START 成功回件写回 `scheduler = SCHED_PROC_NR`（`schedule.c:246`）——`payload_mut` 写 `MessSchedLsysSchedulingStart.scheduler`。

**诚实偏离（两处，均已注明）**：
- 接收失败不立即 panic，连续 64 轮才终止（C `main.c:39-40` 第一条失败即崩）：E1 前每次必败、失败路径可测；VM V10-P0-2 同例。
- `fanout_local` 不再每次重 pick_cpu（C `schedule.c:302` 每次下发前重选）：LOCAL 掩码下 CPU 不上线，重选只重复记账（10 D3 配对语义）；C 的重选是意外不是契约。

**装配（`main.rs`）**：删 `empty_loop` 与硬编码 `MachineInfo` mock（旧 `main.rs:28-31,40-41`）；装配线 = 真实双端 → `get_machine`（败即 panic，对齐 C `main.c:131`）→ `SchedServer::new` → `init_scheduling`（败即 panic，对齐 `schedule.c:340-341`）→ `run`。

### Fix #4 测试：`server.rs` 20 个 + transport mock（`kernel_api/transport.rs` 的 `mock` 模块）

`MockIpc`（脚本化收发 + 发送记录 + 可设失败）与 `MockKernel`（五调用参数账本 + 按序错误脚本），20 个测试覆盖：八条语义各一（部分一测两半，如 noquantum 的成功与失败半）、START/INHERIT/STOP/NICE 快乐路径与拒绝路径、EBADCPU 重试环、闹钟布防与再布防、野编号 ENOSYS、接收失败计数、64 上限 panic（`#[should_panic]`）。基线 59 → **79 passed / 0 failed**；`cargo clippy -p minix-sched --all-targets` 本体 0 告警；`cargo build -p minix-sched` 生产二进制通过；workspace `cargo check` 的 2 条错误为存量基线（`test-memmap-riscv64`，stash 前后同现，非本次引入）。

### 文档同步清单

- `02-sched-message-surface.md`：D7（一轮一步）/ D8（单一所有者）/ D9（两线分接）三节 + ARCH 表 S-11 行 + §4.1 模块树 + §4.2 符号表四行 + §4.3 不变量四行 + §5 测试表 19 行 + §5.1 统计（4 → 23 个）+ §7 参见。
- `01-sched-init-main.md`：D4 刷新（「主循环只留骨架」→「二进制层只装配不决策」）+ §4.1 模块树 + §4.2 符号表行。
- `11-balance-queues.md` / `12-kernel-interface.md`：模块树中「主循环对端」从 `dispatch.rs` 改指 `server.rs`，补 `transport.rs` 行。
- `plan.md`：ARCH 表追加 S-11 行（组合层单一所有者，状态已实现）。
- `.design/02-design.v1.md`：追加 D7-D9 快照 + 不变量四行；`.design/01-design.v1.md`：追加 D4 刷新快照。
- `edge_todo.md`：新增 E8（minix-sys SCHED 侧 SYS_* wrapper + 真实通电挂 E1）。
