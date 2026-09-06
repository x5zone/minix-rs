# 04-stage-pm Rust 实现架构级 Review TODO

> 来源：2026-09-06 架构级代码审查 + 查漏补缺（关注整体与分层架构，非逐函数审查）。
> 范围：`os/servers/pm/src/` 全部 Rust 代码（37 个文件，14,098 行），以及 `os/libs/minix-types/` 中与 PM 相关的类型边界。
> 方法：先做查漏补缺（Gate A 覆盖度枚举：`coverage-extract.py` 对 `minix3/minix/servers/pm` 提取 109 个 C 符号，与文档和 Rust 侧逐一对照；47 个调用号矩阵；DEFERRED/stub 全量收敛；Minix3 易漏语义点逐条 grep 验证），再做整体到分层的架构审查（工作区边界 → 服务器骨架 → 分发层 → 子系统层 → mproc 状态层 → 测试层），对照 Redox 实现与 Rust/OS 社区最佳实践。
> 定位：本文档是查漏补缺清单与架构改进建议清单，**不同于** `draft/`（旧 fork 主线素材，已停止维护）与 `plan.md` §7（文档 review 记录）。
> 状态（2026-09-06）：全部条目待处理。本次审查未发现 P0 级真实 bug；发现 P1 架构级问题 6 项、P2 结构性改进 7 项、P3 代码卫生 2 项，另有 D-XX 缺口登记 24 项（§6）。
> 增补（2026-09-06）：跨阶段条目抽取见 §9；登记于 `notes/rewrite/fork-syscall-rewrite/edge_todo.md`（本阶段新增 E6/E7，既有 E1-E5 为 02-stage-vm campaign 条目）。

---

## 0. 审查结论速览

一句话总结：**PM 的语义逻辑层完成度很高（47 个调用的业务逻辑全部有 Rust 模块，Minix3 易漏语义点基本都在），但逻辑层与消息循环之间的接线层几乎完全断开（47 个调用中 7 个在主循环里用裸魔数内联拦截，其余 40 个返回 ENOSYS），且内核与 VM 边界存在两处"假装成功"的接缝。** 主要工作不在补逻辑，而在收敛分发、打通边界、让 319 个测试覆盖到的代码真正被主循环走到。

| 级别 | 条目 | 一句话 |
|------|------|--------|
| P1 | P1-1 | 分发层分裂成两套：`init.rs` 裸魔数内联拦截 7 个调用，`dispatch_pm_call` 只剩 1 个死臂 + 46 个 ENOSYS（**✅ 已修复** 2026-09-06，见 §10 Fix #2） |
| P1 | P1-2 | `send_vm_fork` / `send_kernel_request` 返回捏造的成功值，fail-closed 契约被反转成 fake-ok（**✅ 已修复** 2026-09-06，见 §10 Fix #1） |
| P1 | P1-3 | 内核边界整体未实现：transport 三个方法 `unimplemented!()`，PM 二进制在第一条真实消息上 panic |
| P1 | P1-4 | codec 层缺口（ARCH A-4）：47 个调用中只有 Fork 有 wire 类型，其余靠内联 unsafe union 访问 |
| P1 | P1-5 | 测试结构验证的是"测试用的分发路径"而非生产分发路径，端到端集成测试缺位（**✅ 已修复** 2026-09-06，见 §10 Fix #3） |
| P1 | P1-6 | 入口函数命名约定分裂（`do_*` 改名 `handle_*` 与保留 C 名混用），污染覆盖率工具的可追溯性（**✅ 已修复** 2026-09-06，见 §10 Fix #4） |
| P2 | P2-1 | `cfg(feature = "syscall_stats"/"sprofile")` 使用了未在 Cargo.toml 声明的特性，被门控代码永久编译排除 |
| P2 | P2-2 | plan.md §4 ARCH 表与代码失同步：A-5 宣称"已实现"实为 1 个死臂，A-9 宣称"缺口"实际已落地 |
| P2 | P2-3 | `minix-types/src/ipc/pm.rs` 的 `PmRequest`/`PmResponse` 是零使用的死代码；PM 调用号单一真值破口 |
| P2 | P2-4 | `lib.rs` glob re-export 压平命名空间，5 对同名双层模块（`fork`/`mproc::fork` 等）加剧混淆 |
| P2 | P2-5 | exit/wait 路径 9 处行为 stub 以散落注释存在，未按模式 60 登记为显式 DEFERRED 契约 |
| P2 | P2-6 | 文档 00/99 仍是最小骨架，且 `.design/` 缺这两篇的 outline/outline-review/design 快照（模式 69） |
| P2 | P2-7 | C 侧死代码 `ESCRIPT`（exec.c:31，定义后零使用）未登记进 plan.md §5.4 排除表 |
| P3 | P3-1 | clippy 约 50 条告警未清理（文档缩进 17、可折叠 if 11、可派生 impl 6 等） |
| P3 | P3-2 | 无用导入与无用参数（`Lifecycle` 两处、`dispatch_pm_call` 的 `table` 参数已无用） |

验证命令（2026-09-06 实测）：

```bash
cargo test -p minix-pm --lib          # 319 passed / 0 failed
cargo check -p minix-pm               # 通过，lib 30 条 warning
cargo clippy -p minix-pm --lib        # 约 50 条告警（见 P3-1 分布）
bash tools/check-rs-unwired.sh        # PASS（所有生产 unwired 标记均带文档契约）
python3 tools/coverage-extract/coverage-extract.py pm \
  notes/rewrite/fork-syscall-rewrite/04-stage-pm \
  --rust-dir os --c-dir minix3/minix/servers/pm \
  --semantic-map tools/coverage-extract/pm-semantic-map.json \
  --output .review/claude/fork-syscall-rewrite/04-stage-pm-arch/SYMBOLS.md
# 结果：109 个 C 符号；文档覆盖 107（98.2%）；Rust 名称匹配 97（89.0%）；
# 12 个未匹配逐条核实结论见 §1.3
bash tools/design-coverage-check.sh fork-syscall-rewrite --stage 04-stage-pm
# 结果：01–20 快照齐全；00/99 缺 outline/outline-review/design（见 P2-6）
```

## 0.1 审查基线：确认良好、不需要动的部分

| 方面 | 证据 |
|------|------|
| `Process` 四层组合（identity/state/resources/ipc）+ 与 `mproc.h` 逐字段的映射表 | `os/servers/pm/src/mproc/mproc.rs:334`（结构体），`mproc/mproc.rs:302-331`（映射表） |
| SUSPEND 显式化为 `ReplyIntent` 三变体，与 plan.md §7.3 契约一致 | `os/servers/pm/src/ipc/dispatcher.rs:48`，契约见 `plan.md:393-406` |
| 47 个调用号 `PmCall` 枚举与 callnr.h 一一对应，并有全量 roundtrip 测试 | `os/servers/pm/src/ipc/calls.rs:29-124`，测试 `calls.rs:222`（`test_call_nr_roundtrip_all_registered`） |
| Minix3 易漏语义点覆盖率高：fork 的 LAST_FEW 非 root 预留、session leader 死亡向进程组发 SIGHUP、孤儿重挂 INIT 的 NEW_PARENT、TO_TRACEFORK、TAINTED、PARTIAL_EXEC、`mp_sigmask2`、NR_PIDS=30000、itimer 三族 | `os/servers/pm/src/fork.rs:35`、`src/exit.rs:76-80` 与 `exit.rs:176`、`src/exit.rs:407`（`disinherit`）与 `exit.rs:544`（`test_disinherit_new_parent`）、`src/mproc/guardianship.rs:58-59`、`src/exec.rs:96-100`、`src/mproc/mproc.rs:165-166`、`src/mproc/signal.rs:60`、`src/mproc/constants.rs:24-26`、`src/timer.rs:32-34` |
| 凭证 13 个调用（getset.c 的 7 个 get + 6 个 set）建模为显式 `GetOp`/`SetOp` 枚举 | `os/servers/pm/src/credentials.rs:19-45`，对照 C `getset.c:28`（do_get 7 分支）与 `getset.c:110`（do_set 6 分支） |
| 单线程 `&mut ProcTable` 用借用检查器替代 C 的文件级全局（ARCH A-3） | `os/servers/pm/src/mproc/context.rs`，调用点 `src/init.rs:326` |
| 每个模块头部带 Minix3 C file:line 锚点与归属文档编号，可追溯性好 | 抽查 `src/ipc/calls.rs:1-17`、`src/ipc/dispatcher.rs:1-28`、`src/timer.rs:16` |

---

## 1. 查漏补缺总表

### 1.1 47 个调用号 × 接线 × 逻辑 × 测试 矩阵

结论先行：**47 个调用的业务逻辑在 Rust 侧全部有对应模块（多数还有单元测试），但通过真实消息循环可达的只有 7 个；其余 40 个在 `dispatch_pm_call` 中返回 ENOSYS（`os/servers/pm/src/ipc/calls.rs:213`）。** 缺口的性质是"接线与编解码"，不是"语义缺失"。

| 调用群 | 调用号 | 逻辑模块（有/无） | 消息循环可达 | 说明 |
|--------|--------|------------------|--------------|------|
| 生命周期 | 1 Exit, 2 Fork, 3 Wait4, 41 SrvFork | 有：`exit.rs:24`（do_exit）、`fork.rs:22`（do_fork）、`wait.rs:38`（do_wait4）、`fork.rs:112`（do_srv_fork） | 可达，但走 `init.rs` 内联拦截 | `init.rs:386/399/429/437` 用裸魔数 `msg.m_type == 2/41/1/3` 拦截，不经过 `PmCall` 枚举 |
| 信号发送 | 11 Kill, 42 SrvKill | 有：`signal.rs:39`（do_kill）、`signal.rs:52`（do_srv_kill） | 可达，同样内联拦截 | `init.rs:458/479` |
| 事件 | 40 ProcEventMask | 有：`event.rs:363`（do_proceventmask_mut）、`event.rs:412`（do_proc_event_reply） | 可达 | `init.rs:368` 拦截；这是唯一用 `minix_types::PM_PROCEVENTMASK` 常量而非裸数字的拦截点 |
| 凭证 | 4-6, 9-10, 12-13, 15-16, 29-32（共 13 个） | 有：`credentials.rs:87`（do_get）、`credentials.rs:148`（do_set） | 不可达（ENOSYS） | 逻辑与测试齐备，等待接线 |
| 信号控制 | 20-24（sigaction/sigsuspend/sigpending/sigprocmask/sigreturn） | 有：`signal_handlers.rs` | 不可达 | 同上 |
| 时间 | 7, 28, 33-35 | 有：`time.rs` | 不可达 | 同上 |
| 定时器 | 17 Itimer | 有：`timer.rs` | 不可达 | A-7（`plan.md:201`）标注内核定时器抽象未实现，但 PM 侧逻辑已备 |
| exec 族 | 14, 43, 44 | 有：`exec.rs`（do_exec/do_newexec/do_execrestart） | 不可达 | `ipc/vfs.rs:469` 的 `exec_restart`（VFS 回复侧）在生产 impl 中 `unimplemented!()` |
| 调度 | 26, 27 | 有：`sched.rs:280`（nice_to_priority）、`sched.rs:285`（get_nice_value） | 不可达 | A-8（`plan.md:202`）SCHED 客户端未实现（D-12） |
| ptrace | 8 | 有：`trace.rs` | 不可达 | `wait.rs:101` 的 trace-stop 返回码仍是模拟值（D-20） |
| 杂项查询 | 18-19, 25, 36-39, 45-47 | 有：`misc.rs`（sysuname/getsysinfo/getprocnr/getepinfo/reboot/svrctl/getrusage/sprofile/mcontext） | 不可达 | sprofile 与 syscall_stats 受 P2-1 的虚构特性门影响 |
| 兜底 | dispatch 的 Fork 臂 | — | 死代码 | `ipc/calls.rs:211` 的 `PmCall::Fork => ReplyLater` 在服务器路径上永远不触发（Fork 已被 `init.rs:386` 拦截）；clippy 也报告 `calls.rs:201:39` 的 `table` 参数已无用 |

### 1.2 C 函数面对账（proto.h 71 个函数 + 38 个宏）

Gate A 工具（`coverage-extract.py`）报告 109 个 C 符号中 Rust 名称匹配 97 个（89.0%），12 个未匹配逐条核实如下，其中**真正的语义缺口只有 2 个**：

| C 符号 | 核实结论 | 证据 |
|--------|----------|------|
| `do_exit` / `do_wait4` / `do_kill` / `do_srv_kill` | Rust 语义存在，入口改名（见 P1-6） | `exit.rs:24`（do_exit）、`wait.rs:38`（do_wait4）、`signal.rs:39/52` |
| `is_sane_timeval` | 存在，成为 `Timeval` 方法 | `timer.rs:53`（`is_sane`） |
| `NO_EVENTSUB` | 存在，表达为 `None` 与 `NO_EVENTSUB_RAW = -1` | `event.rs:30-32` |
| `SEND_PRIORITY` / `SEND_TIME_SLICE`（const.h:19-20） | **真缺口**：调度协议消息常量未建模 | 全库 grep 零命中；属 A-8（`plan.md:202`）范围 |
| `ESCRIPT`（exec.c:31） | Rust 缺失是**正确的**：C 里它也是死代码（定义后零使用），见 P2-7 | `grep -rn ESCRIPT minix3/minix/servers/pm/` 仅命中定义行 |
| `EXTERN` / `_SYSTEM` / `_TABLE` | C 编译宏，Rust 无需对应 | `glo.h:4`、`pm.h:4`、`table.c:5` |

### 1.3 Minix3 易漏语义点抽查（11 项全部在位）

fork 的非 root 进程数预留与 EAGAIN（`fork.rs:31-35`，对照 `forkexit.c:60-65`）、next_child 轮转槽位扫描（`fork.rs:40-43` 注释对照 `forkexit.c:68-75`）、session leader 死亡记忆 procgrp 并发 SIGHUP（`exit.rs:76-80` 与 `exit.rs:176`，对照 `forkexit.c:298/412`）、INIT 死亡打印栈回溯后直接返回 / VFS 死亡 panic（`exit.rs:120-122`，对照 `forkexit.c:336-345`）、disinherit 重挂 INIT（`exit.rs:407`，对照 `forkexit.c:760-795`）、TO_TRACEFORK（`guardianship.rs:58-59`）、exec 的 TAINTED 双重判定与 allow_setuid（`exec.rs:96-100`）、`mp_sigmask2` 保存掩码（`mproc/signal.rs:60`）、NR_PIDS=30000（`mproc/constants.rs:24-26`）、itimer 三族与 NR_ITIMERS=3（`timer.rs:32-34`、`mproc/mproc.rs:33`）、nice 与优先级队列双向换算（`sched.rs:280/285`，对照 `utility.c:91` 与 `main.c:276`）。

这项抽查的结论是：**文档 07–20 描述的语义面在 Rust 侧基本落地，查漏补缺的重心应从"补语义"转向"补接线与补契约"。**

---

## 2. P0：真实 bug（本次未发现）

按 02-stage-vm todo.md 的 P0 定义（在可达路径上、被测试或行为验证暴露的真实错误），本次审查**没有发现 P0**。当前所有路径失败都有一个共同原因：内核 IPC 未落地（P1-3），因此"行为错误"尚无法与"无法运行"区分。P1-2 的假成功接缝最接近 P0（它在测试里伪装成正常工作），但因为真实二进制在第一条消息上就会 panic（`transport.rs:86-96`），尚未产生用户可见的错误行为，故按架构级问题记录。

---

## 3. P1：架构级问题（建议尽快规划）

### P1-1 分发层分裂成两套并行实现（✅ 已修复 2026-09-06，见 §10 Fix #2）

**问题**：主循环 `run_once`（`os/servers/pm/src/init.rs:296-507`）里，7 个调用（Exit=1、Fork=2、Wait4=3、Kill=11、ProcEventMask=40、SrvFork=41、SrvKill=42）用裸魔数 `msg.m_type == N` 内联拦截并直接调用 handler（`init.rs:368/386/399/429/437/458/479`），消息解码用内联 `unsafe { msg.m_u.m_lc_pm_* }` 访问。其余 40 个调用落入 `dispatch_message` → `dispatch_pm_call`（`ipc/calls.rs:201`），全部返回 ENOSYS。这造成四个后果：

1. `dispatch_pm_call` 的 `PmCall::Fork => ReplyLater` 臂（`calls.rs:211`）在服务器路径上是死代码——Fork 已被上游拦截。若未来有人删掉 `init.rs` 的内联块依赖这一臂，fork 请求会被静默挂起（返回"稍后回复"但没有任何人回复）。
2. 同一事实源分裂：内联块用裸数字 1/2/3/11/41/42，事件拦截用 `minix_types::PM_PROCEVENTMASK`，`dispatch_pm_call` 用 `PmCall` 枚举——三处表达同一张调用表。
3. 单元测试验证的是 `dispatch_message`/`dispatch_pm_call` 这条 C 对应路径（`ipc/calls.rs:222-273` 的 4 个测试），而生产走的是 `init.rs` 内联路径，两条路径的行为没有交叉验证。
4. C 的 `call_vec` 是**一张表**（`table.c:23-59`）；Rust 现在是"一张死表 + 一段绕过表的内联代码"。

**影响**：后续 07–20 任何调用的接线都必须决定"加内联块还是加 match 臂"，两套风格并存会让每个新调用点都成为一次架构投票；ENOSYS 占位的语义（文档 04 §3.6 D6）与真实接线状态也难以对账。

**建议**：
1. **首选**：单一分发入口。把 `init.rs` 的 7 个内联块全部收编进 `dispatch_pm_call` 的 47 臂穷尽 match（每臂先 `PmRequest` 解码再调 handler），`run_once` 只保留 C `main.c:84-89` 对应的两路前置拦截（VFS 回复、PROC_EVENT_REPLY）加主循环回复逻辑（`init.rs:501-504`）。未接线的臂保留 ENOSYS 占位，但每臂用 `PmCall` 判别值而非裸数字，接线进度一目了然。
2. **次选**：注册表方案——仿 C `call_vec` 建 `fn(&mut PmContext, &Message) -> ReplyIntent` 的 47 项常量表。与方案 1 相比可动态替换 handler（便于测试注入），但 47 项静态域上编译期穷尽 match 已足够，且 match 能被编译器检查完整性，不引入函数指针的间接层。Redox 对 scheme 这类动态集合才用注册表（kernel `src/scheme/mod.rs` 的 `SchemeId` 查找），固定域用 match 是 Rust 社区惯例。
3. 无论选哪条，`init.rs` 的 7 个内联块的 unsafe 解码应随迁移移入 codec 层（见 P1-4）。

### P1-2 服务间请求的"假成功"接缝（✅ 已修复 2026-09-06，见 §10 Fix #1）

**问题**：`send_vm_fork`（`os/servers/pm/src/ipc/dispatcher.rs:139-155`）不发送任何消息，直接构造一个 `child_endpoint = from_generation_slot(1, child_slot)` 的 `VmForkOut` 返回 `Ok`；`send_kernel_request`（`dispatcher.rs:159-165`）同样直接返回 `Ok(KernelResponse::ForkOk)`。生产 fork 路径 `do_fork`（`fork.rs:53`）与 `do_srv_fork`（`fork.rs:142`）都调用 `send_vm_fork` 并把这个捏造的 endpoint 写入子进程表项（`fork.rs:64`），随后向 VFS 发送 `VFS_PM_FORK`。

C 的对应语义是"vm_fork 失败直接返回错误码，成功后才进入不可失败窗口"（`forkexit.c:78-82`），即 PM 依赖 VM 的真实应答来保证"fork 之后不会回滚"。Rust 当前把这一步变成了无条件成功：`debug_assert_eq!`（`fork.rs:58-61`）检查的 endpoint slot 一致性是和自己的捏造值比较，永远成立。这违反了 plan.md §4 对缺口项"fail-closed（失败时显式关断）"的契约方向（A-9/A-10 标注原则，`plan.md:203-204`）——缺口可以存在，但不应该伪装成成功。

**影响**：所有 fork 相关的 25 个测试（`fork.rs` 7 个 + `mproc/fork.rs` 18 个）建立在一个虚构的 VM 应答之上；一旦内核 IPC 落地、真实 `vm_fork` 语义接入，fork 链路的时序假设（"vm_fork 成功后才占位"，`fork.rs:62-64`）从未被真正验证过。

**建议**：
1. **首选**：把两个假成功函数改为经 `IpcTransport` trait 发送真实请求（VM_FORK 的 wire 类型 `minix_types::VmForkIn/VmForkOut` 已存在，`libs/minix-types/src/ipc/vm.rs`），未落地前返回 `Err(ForkCoordError::VmError)` 或 `unimplemented!("DEFERRED: ...")`。fail-closed 会让现有 fork 测试失败——这是**正确**的结果，它暴露的是测试对虚构 VM 的依赖；这些测试应改用 TestIpcTransport 注入脚本化应答（见 P1-5）。
2. **次选**：保留占位但把返回类型改为"显式未实现"标记（如 `Result<VmForkOut, ForkCoordError>` 且实现永远 `Err` + `#[doc(hidden)]`），并在 `fork.rs` 调用点注释标明当前值的虚构性。劣于方案 1：调用方仍会拿到成功值，虚构依旧。
3. 参考 Redox：用户态服务之间的同步往返用阻塞 channel 语义（relibc 侧 `sys_call` 系），服务未注册时返回明确错误而不是成功——"请求了不存在的服务"是错误，不是成功。

**跨阶段拆分**：stage 内只做 fail-closed 改造（本条建议 1/2）；真实 VM_FORK 往返与端到端联调挂 `edge_todo.md` E5(a) + E1/E2（2026-09-06 增补）。

### P1-3 内核边界整体未实现（最大依赖簇）

**问题**：PM 与内核的边界上有 6 个 `unimplemented!()`/placeholder：`KernelIpcTransport` 的 receive/send/sendrec（`ipc/transport.rs:86/91/96`）、`BootParams::placeholder()`（`main.rs:15`）、`PmServices::sys_abort`（`ipc/vfs.rs:488`）。另有 4 个 VFS 协同方法在生产 impl 中 `unimplemented!()`：`sched_start_user`（`vfs.rs:401`）、`exit_proc` 通知（`vfs.rs:407`）、`set_core_flag`（`vfs.rs:413`）、`exec_restart`（`vfs.rs:469`）。这意味着 `minix-pm` 二进制在第一条真实消息的 `receive()` 上就会 panic——当前 14,098 行代码的运行时形态是"可测试的库 + 不可运行的守护进程"。

**影响**：这是 D-XX 登记表中最大的一簇依赖（§6 的 D-01/D-02/D-05~D-09）。01-stage-kernel 的 `minix-sys` 系统调用面落地前，PM 的全部生命周期语义（fork 的 VFS 协同、exit 的 vm_willexit/vm_exit、exec 的 exec_restart）都只能以单测形态存在。

**建议**：
1. **首选**：不急于在本阶段实现，但要做两件事：(a) 把 §6 依赖表中每一项的"解除条件"写成可 grep 的断言（例如 transport 三方法的 unimplemented 消息里写明依赖 `01-stage-kernel` 的哪个符号），目前 `transport.rs:86-96` 已有此格式，保持；(b) 在 `minix-sys` 落地时优先实现 receive/sendrec 最小面（main 循环只需要 `receive` + `send` + CLOCK notify 判定三个能力，`init.rs:305` 与 `init.rs:334`），避免"一次性全量实现 sys_*"的大爆炸集成。
2. **次选**：为 transport 增加第二个生产实现（例如 Unix domain socket 模拟内核通道，供 qemu-tests 之外的宿主机集成测试用）。trait 接缝（`transport.rs:46` 的 `IpcTransport`）已经为此准备好，且满足"trait 至少两个行为不同的实现"的存在性检查（当前 TestIpcTransport 是唯一真实现）。
3. 参考 Redox：内核系统调用面（`kernel/src/syscall/`）与用户态服务是同步演进的，没有出现过"服务写完、系统调用面后补"的窗口；本项目的补法建议按 P1-1 收敛后的分发入口倒推需要的最小内核面，而不是按 kernel 侧清单正推。

**跨阶段拆分**：trap 层挂 `edge_todo.md` E1；SYS_* wrapper 挂 E2（VM 清单）与 E6（PM 清单，2026-09-06 新增）；本条 stage 内只保留 transport 实现与最小面倒推。

### P1-4 codec 层缺口：47 个调用只有 1 个 wire 类型（ARCH A-4）

**问题**：`plan.md:198`（A-4）记录"message union → 类型化 IPC：部分实现（目前仅 Fork 变体）"，实际比记录的更薄：`os/libs/minix-types/src/ipc/pm.rs` 的 `PmRequest` 枚举只有 `Fork` 一个变体，且全库零使用（见 P2-3）。47 个调用的消息解码现状是三种并存：`init.rs` 内联 unsafe union 访问（7 个调用）、测试里直接构造参数绕过消息层（大多数模块）、以及完全无解码（40 个 ENOSYS 调用）。C 侧 `m_in.m_lc_pm_*`/`m_pm_lc_*` 的 union 字段位型（`com.h`）没有系统性的 Rust 对应。

**影响**：接线（P1-1）的每一臂都需要先回答"这个调用的消息怎么解码"。没有 codec 层，P1-1 的迁移会把 47 段 unsafe 解码内联进 match 臂，重复且易错；wire 布局错误（union 字段错位）也不会有任何类型层防护。

**建议**：
1. **首选**：按调用族在 `minix-types` 建 wire 结构体（`ExitStatus`、`Wait4Args`、`KillArgs`、`SigactionArgs`……），每族带 `size_of` 断言与 C `m_lc_pm_*` 的逐字段对照注释（`message.rs` 中 `MessPmSchedSchedulingSetNice` 等已有此风格，`libs/minix-types/src/ipc/message.rs`，带 size assert）。是否保留单一 `PmRequest` 枚举可作为第二层封装：枚举变体持有各 wire 结构体，解码函数 `PmRequest::from_message(&Message) -> Result<PmRequest, PmError>` 做一次集中 unsafe。
2. **次选**：只做集中 unsafe 解码函数（按调用号 match 返回小型参数元组），不建结构体。改动小，但丢失字段语义与 size 断言，长期看是债务。
3. 与 05-stage-vfs 协同：`VfsPmRequest`（`minix-types/src/ipc/vfs.rs`）已有 Exit/Fork 变体的字段化先例，PM 侧 wire 类型应与其保持同一风格。
4. 参考 Redox：relibc 与内核之间每个系统调用都有独立的 `Call` 结构与手写编解码（relibc `src/platform/redox.rs`），字段错位在编译期不可查但集中可审——本项目用 size 断言可以比 Redox 做得更好。

**跨阶段拆分**：wire 结构体系统化、死代码处置与调用号收敛的本体在 minix-types（共享契约层），归 `edge_todo.md` E7；本条目 stage 内只做消费端接线（2026-09-06 增补）。

### P1-5 测试验证的是"测试分发路径"，生产分发路径无端到端覆盖（✅ 已修复 2026-09-06，见 §10 Fix #3）

**问题**：319 个单元测试全部针对逻辑模块；分发层的 4 个测试（`ipc/calls.rs:222-273`）验证 `dispatch_pm_call`，而生产 7 个调用的接线在 `init.rs` 内联块中，仅由 `init.rs` 的 15 个测试部分覆盖（`init.rs:982` 附近的注释描述了 fork 的回复时序，但这些测试不经过完整 `run_once` 消息循环）。跨 crate 集成测试 `os/tests/pm_vm_fork.rs` 已整体停用（文件头 `DEPRECATED` 注释，理由成立：VM 类型对外部 crate 不可见），且没有替代品。也就是说：**从"收到 Message"到"发出 Reply"的完整链路，没有任何测试走通过。**

**影响**：P1-1 迁移分发时没有安全网；`ReplyIntent` 的三种回复路径（plan.md §7.3：等待中回复/异步回复/永不回复）只在逻辑层单测中被间接触及。

**建议**：
1. **首选**：在 `minix-pm` crate 内建 `tests/` 集成目录，用 `TestIpcTransport`（`ipc/transport.rs` 的 mock 实现）驱动 `PmServer::run_once` 走完整循环：构造 Message → 断言 transport 收到的 Reply 时序。重点场景：fork 的 SUSPEND-后-由 VFS 回复链、exit 的 NoReply（"beyond the grave"，`init.rs:432` 注释）、wait4 的 WNOHANG 同步回复（`init.rs:449-453` 注释列举的三类）。这正是停用的 pm_vm_fork.rs 文件头建议的方向（"Future integration tests should use mock IPC"）。
2. **次选**：给 `dispatch_pm_call` 迁移（P1-1）配表驱动测试：47 个调用号各发一条最小消息，断言"已接线的臂到达 handler、未接线的臂返回 ENOSYS"，让接线进度本身成为被测对象。
3. 参考 Redox：redox-rt 与 kernel 的集成靠 qemu 测试与 userspace 测试双层；本项目 `qemu-tests` 已存在但只间接触及 PM（bootshim 侧），近期可先落方案 1 的纯 Rust 层，qemu 层留到内核 IPC 落地后。

### P1-6 入口函数命名约定分裂，损害 C↔Rust 可追溯性（✅ 已修复 2026-09-06，见 §10 Fix #4）

**问题**：同一 crate 内两种命名并存：6 个入口把 C 名 `do_*` 改成 `handle_*`（`do_exit`→`handle_exit`，`exit.rs:24`；`do_wait4`→`handle_wait4`，`wait.rs:38`；`do_kill`→`handle_kill` 与 `do_srv_kill`→`handle_srv_kill`，`signal.rs:39/52`；`do_fork`→`handle_fork` 与 `do_srv_fork`→`handle_srv_fork`，`fork.rs:22/112`），大量内部函数保留 C 名（`exit_proc`/`exit_restart`/`zombify`/`cleanup`/`check_parent`/`tell_parent`/`check_sig`/`sig_proc`/`process_ksig`…）。后果已经显现：Gate A 覆盖率工具的名称匹配把这些入口报成"有文档无 Rust"（§1.2），每次 review 都要人工排除这批误报。

**影响**：review-code-checklist 维度 7 要求命名与 Minix3 对齐以便 grep 反查；现状是"半对齐"，反查 `do_exit` 会失败，反查 `exit_proc` 会成功，认知成本落在每个后续审查者头上。

**建议**：
1. **首选**：统一回 C 名（`do_exit`/`do_wait4`/`do_kill`/`do_srv_kill`）。理由：(a) 与同文件已保留 C 名的函数一致；(b) 让覆盖率工具的名称匹配恢复零误报，长期收益最大；(c) `handle_*` 并不比 `do_*` 表达更多语义，改名没有换到信息。
2. **次选**：保留 `handle_*` 但做两件补偿——把映射写进 `tools/coverage-extract/pm-semantic-map.json`（工具已有该机制），并在每处改名点加一行 `/// C: do_exit (forkexit.c:246)` 文档注释（部分文件已有，不齐）。劣于方案 1：映射知识仍需两跳才能还原。
3. 若选方案 2，`is_sane_timeval` → `Timeval::is_sane`（`timer.rs:53`）这类"自由函数变方法"的合理重命名也应进 semantic map，与入口改名分开记录。

---

## 4. P2：结构性改进（正确性 gate 通过后规划）

### P2-1 虚构的 cfg 特性门：`syscall_stats` 与 `sprofile`

**问题**：`misc.rs` 用 `#[cfg(feature = "syscall_stats")]` 门控 `SysInfoWhat::CallStats` 变体与 `SysInfoCtl::call_stats`（`os/servers/pm/src/misc.rs:108/117/127/319`），用 `#[cfg(feature = "sprofile")]` 门控 sprofile（`misc.rs:461`），但 `os/servers/pm/Cargo.toml` **没有任何 `[features]` 段**。后果：(a) 这些代码在任何构建下都不会被编译，静默失效；(b) `cargo check -p minix-pm --features syscall_stats` 会直接报错（特性不存在），门控代码的编译正确性无法验证，会随时间腐烂；(c) clippy 已经在报 `unexpected cfg condition value`（4 条 syscall_stats + 2 条 sprofile）。

plan.md §5.4（`plan.md:282-283`）的处理是"20 标注为 cfg feature，WONTFIX 文档化 / 默认 ENOSYS 语义保留"——C 侧 `ENABLE_SYSCALL_STATS` 与 `SPROFILE` 是**可开启**的编译选项（`misc.c:64/131-132`），Rust 侧现在是有门无门后，语义上比 C 更少而非"等价的可选项"。

**建议**：在 Cargo.toml 声明 `[features] syscall_stats = [] sprofile = []`（默认不开，与 C 的默认关闭一致），并在 CI 或本地检查命令中加入 `cargo check -p minix-pm --features syscall_stats,sprofile`，保证被门控代码可编译。这是十分钟级改动，消除一类"看起来有开关、实际是死代码"的虚构性。

### P2-2 plan.md §4 ARCH 表与代码状态失同步

**问题**：三处失同步。(a) A-5（`plan.md:199`）说 dispatch_pm_call"47 项已实现"——实际只有 1 个死臂 + 46 个 ENOSYS（§1.1），"已实现"的是枚举与解码，不是分发；(b) A-9（`plan.md:203`）说事件订阅"缺口：未实现，标注 fail-closed"——实际 `event.rs` 已有 1,001 行、24 个测试，且 `do_proceventmask_mut` 已在主循环接线（`init.rs:368-382`），fail-closed 契约已被真实实现取代；(c) A-4 说"部分实现（仅 Fork 变体）"仍然准确，但 Fork 变体本身是零使用死代码（P2-3），"部分实现"名存实亡。

**影响**：plan.md 是本阶段的覆盖契约（§5"覆盖完整性核对"），ARCH 表状态失真会误导后续阶段的依赖判断（例如别的服务器若按 A-9"缺口"设计协同协议，会重复造一份事件订阅）。

**建议**：按 §6.1 的状态跟踪表风格刷新 §4 的"状态"列（已实现/部分实现/缺口三分 + 日期 + 证据命令），A-5 的措辞拆成"调用号建模已实现 / 分发接线 7/47"；A-9 状态改为"已实现（2026-09 前后落地），fail-closed 契约由实现替代"，并回填 A-9 原契约的归档说明。文档-代码同步门（review-cmds §一.6）要求双向，这次是代码走在文档前面。

### P2-3 `minix-types/src/ipc/pm.rs` 的遗留类型是零使用死代码；调用号单一真值破口

**问题**：`PmRequest`/`PmResponse`（`os/libs/minix-types/src/ipc/pm.rs:13-38`）在包括 pm crate 在内的整个 os/ 工作区零使用（`PmError` 除外，它被 `init.rs:390` 使用）。同时 PM 调用号出现两处表达：`pm/src/ipc/calls.rs` 的 `PmCall` 枚举（47 值，注释明确"将来内核侧需要时再上移 minix-types"，`calls.rs:16-17`）与 `minix-types` 里的个别常量（`init.rs:368` 使用 `minix_types::PM_PROCEVENTMASK`）。同一个"47 个调用号"的事实，一部分住在 pm crate、一部分住在 minix-types。

**建议**：(a) 删除 `PmRequest`/`PmResponse`（或等 codec 层 P1-4 设计时再决定去留，但要在 P1-4 的方案里显式处置它们，不能默认保留）；(b) `PM_PROCEVENTMASK` 这类常量要么下沉回 pm crate 统一从 `PmCall` 取值，要么在 `calls.rs:16-17` 的"单一事实源"注释里写清当前的双址现状与收敛计划——现状是注释宣称单一真值，代码已经双址。

**跨阶段拆分**：本体在 minix-types（共享契约层），处置归 `edge_todo.md` E7；PM 侧在本条目闭环时引用 E7 的收敛结论，不在 stage 内单独改动共享层（2026-09-06 增补）。

### P2-4 `lib.rs` glob re-export + 5 对同名双层模块

**问题**：`lib.rs:47-48` 的 `pub use ipc::*; pub use mproc::*;` 把两个模块树压平到 crate 根。而 crate 顶层与 `mproc/` 下存在 5 对同名模块：`fork`/`mproc::fork`、`signal`/`mproc::signal`、`wait`/`mproc::wait`、`credentials`/`mproc::credentials`、`trace`/`mproc::trace`。"logic 层（顶层）vs state 层（mproc/）"的分工本身是合理设计（模块头注释有说明），但 glob re-export 后，`pm::signal` 指向谁、`pm::mproc::signal` 又暴露哪些与顶层重叠的符号，对使用者是猜谜。

**建议**：移除两个 glob，改为显式逐项 re-export（只导出确实属于公共 API 的类型，如 `ProcTable`、`PmCall`、`ReplyIntent`、`PmServer`），logic/state 两层各自保持 `pm::signal` 与 `pm::mproc::signal` 的完整路径。这同时服务 P1-1：分发迁移时的符号冲突会先在显式 re-export 处暴露。

### P2-5 exit/wait 路径的行为 stub 应升级为显式 DEFERRED 契约

**问题**：exit 与 wait 的关键协作点当前以"注释说 stub 了"的形式存在：`sys_kill`（`exit.rs:33`）、自身 times 计账（`exit.rs:101`）、`vm_willexit`（`exit.rs:116`）、core name 指针（`exit.rs:142`）、`sched_stop`（`exit.rs:192`）、`sys_clear`（`exit.rs:215`）、`vm_exit`（`exit.rs:219`）、wait 的 trace-stop 返回码模拟（`wait.rs:101`）、rusage 跨地址空间拷贝假装成功（`wait.rs:124`）。这些是合法的过渡态（多数依赖内核/VM 落地），但表达形式是散落注释 + 静默成功返回，与 `unimplemented!("DEFERRED: ...")`（`ipc/vfs.rs:401-488` 的做法，且已通过 check-rs-unwired 检查）不一致。`wait.rs:124` 的 rusage 拷贝尤其值得注意：C 侧本身是 TODO（`utility.c:92`，只填 ru_utime/ru_stime），Rust 侧假装拷贝成功，两边叠加后这个语义点存在三层叠加的不完整性。

**建议**：统一为两种形态之一：能 fail-closed 的改 `unimplemented!` 带 DEFERRED 归属（像 vfs.rs 那样）；必须静默成功的（如 rusage 假装拷贝成功以保证 wait 链路可测）在代码位置标注 `[DEFERRED: D-XX]` 并在 §6 登记表加"当前是静默成功，验证锚点是 XXX"。目标是：任何一处 stub 的存在性、归属、解除条件，都能被一条 grep 找到。部分项（D-13~D-21）已登记在 §6，代码侧标注补齐即可。

### P2-6 文档 00/99 仍是最小骨架，且缺 `.design/` 快照

**问题**：`00-pm-overview.md` 与 `99-global-concepts.md` 均为 16 行骨架，状态行自记"pending（最小骨架，待改写）"；`tools/design-coverage-check.sh fork-syscall-rewrite --stage 04-stage-pm` 报告两篇各缺 outline/outline-review/design 三个快照（模式 69 PSMD）。01–20 的快照齐全（63 个文件）。此外 plan.md §6.1 的状态表（`plan.md:300/321`）将 00/99 记为"骨架 —"，与文档头状态一致，这点是同步的。

**建议**：00（总览）与 99（全局概念）是 22 篇的入口与收尾，plan.md §6.2（`plan.md:329`）已排为最后优先级，维持该排序即可；但建议在改写 00 时顺手把本文件（todo.md）的 P1-1/P1-3 结论纳入"当前实现状态"叙述，避免总览写成后立刻过时。快照按 Step 0.3 流程在下次触碰这两篇的 review 中补齐。

### P2-7 C 死代码 `ESCRIPT` 未登记进排除表

**问题**：`minix3/minix/servers/pm/exec.c:31` 定义 `#define ESCRIPT (-2000)`（"#! 脚本的 read_header 返回值"），但整个 PM C 源中零使用——它是 Minix2 时代 PM 自读可执行头的遗迹。plan.md §5.4 排除表（`plan.md:278-288`）登记了 `ENABLE_SYSCALL_STATS`、`SPROFILE` 等编译宏，未含此项。Rust 侧没有它，行为正确。

**建议**：在 §5.4 补一行（项：`ESCRIPT`；说明：C 内死代码，`exec.c:31` 定义后零使用；处置：Rust 不实现，排除表登记即可）。对照 review-patterns 模式 78（C 源码缺陷/死代码应显式标注而非静默忽略）。

---

## 5. P3：代码卫生

### P3-1 clippy 告警清理（约 50 条）

实测分布（`cargo clippy -p minix-pm --lib`，2026-09-06）：文档列表缩进 17、可折叠 if 11、可派生 impl 6、无用 cast（u64→u64 等）6、未声明的 cfg 特性值 6（随 P2-1 消除）、`too many arguments` 2、类型上限恒假比较 2、其余零散。全部是机械项或低风险项，可在 P1-1/P1-4 迁移后一次性清理（先迁移再清理，避免双倍改写）。

### P3-2 无用导入与无用参数

`use Lifecycle` 未使用两处（`exec.rs:8:31`、`sched.rs:12:44`）；`dispatch_pm_call` 的 `table` 参数已无用（`ipc/calls.rs:201:39`，因为只剩 Fork 死臂——这条会随 P1-1 自然消失，可作为迁移完成的天然验证点）。另外 `cargo check` 有 30 条 lib warning 与 clippy 大部分重叠，P3-1 清理时一并核对归零。

---

## 6. D-XX：DEFERRED / stub / unimplemented 全量登记

> 2026-09-06 全量 grep（`DEFERRED`、`stub`、`unimplemented!`、`todo!`）收敛。`tools/check-rs-unwired.sh` PASS（所有生产 unwired 标记带文档契约）。"解除条件"列是该条目从 DEFERRED 转为可修复的 grep 锚点。

| ID | 位置 | 内容 | 归属 | 解除条件 |
|----|------|------|------|----------|
| D-01 | `ipc/transport.rs:86/91/96` | KernelIpcTransport receive/send/sendrec `unimplemented!()` | 01-pm-init-main.md:382 | minix-sys 内核 IPC 面（01-stage-kernel） |
| D-02 | `main.rs:15` | `BootParams::placeholder()` 启动参数占位 | 01-pm-init-main.md | `sys_getmonparams`/`sys_getimage` |
| D-03 | ~~`ipc/dispatcher.rs:139-155`~~ | ~~`send_vm_fork` 捏造成功应答~~ **✅ 已修复**（2026-09-06，Fix #1：真实 `vm_fork` sendrec，见 §10） | 07-pm-fork.md / 02-stage-vm/18 | ~~真实 VM_FORK 往返~~ 已实现（wire 层）；硬件通电仍挂 E1 |
| D-04 | ~~`ipc/dispatcher.rs:159-165`~~ | ~~`send_kernel_request` 捏造成功应答~~ **✅ 已修复**（2026-09-06，Fix #1：零调用死代码删除；C 的 `do_fork` 无独立内核请求步骤，proc 复制在 VM 的 sys_fork 内） | 07-pm-fork.md | ~~内核 fork 请求面~~ 不适用（与 C 不符的原型残留） |
| D-05 | `ipc/vfs.rs:401` | `sched_start_user` `unimplemented!()` | 16-scheduling.md | SCHED 客户端（A-8） |
| D-06 | ~~`ipc/vfs.rs:407`~~ | ~~`exit_proc` 的 VFS 退出通知 `unimplemented!()`~~ **✅ 已修复**（2026-09-06，Fix #8：委托 09 的 `crate::exit::exit_proc` 二阶段退出，`main.c:381` FORK 失败路径） | 09-pm-exit.md | ~~接线 09 退出链时~~ 已达成（+1 委托测试） |
| D-07 | ~~`ipc/vfs.rs:413`~~ | ~~`set_core_flag`（WCOREFLAG）`unimplemented!()`~~ **✅ 已修复**（2026-09-06，Fix #9：WCOREFLAG 置入 `Lifecycle::Exiting.sig_status` bit7，u8 域位运算；wait4 组合改无符号字节） | 09-pm-exit.md | ~~同上~~ 已达成 |
| D-08 | ~~`ipc/vfs.rs:469`~~ | ~~`exec_restart` `unimplemented!()`~~ **✅ 已修复**（2026-09-06，Fix #10：ExecServices 生产端口 + ExecRestartServices supertrait 收敛；sys_exec 内核调用返回 -ENOSYS 由 exec_restart 的 C 同型 panic 承接） | 17-exec.md | ~~exec 重启路径接线~~ 逻辑达成（真实 sys_exec 挂 E6） |
| D-09 | `ipc/vfs.rs:488` | `sys_abort` `unimplemented!()` | 01-stage-kernel | 内核 sys_abort |
| D-10 | ~~`fork.rs:86-99/175-186`~~ | ~~tracer SIGSTOP 记意图不执行~~ **✅ 已修复**（2026-09-06，Fix #5：真实 `sig_proc` + `inherit_guardianship` 的 TO_TRACEFORK 条件继承，含隐藏阻塞 copy_mproc 重置监护的修复） | 11-signal-core.md | ~~sig_proc 可跨模块调用~~ 已达成 |
| D-11 | ~~`event.rs:170/230`~~ | ~~事件重启的 exit_restart/restart_sigs 仅清标志~~ **✅ 已修复**（2026-09-06，Fix #7：Signal 分支接真实 restart_sigs，PmEventServices 适配器；Exit 分支本已接线且注释过时） | 13-signal-flow.md | ~~restart_sigs 完整实现~~ 已达成（KernelResume 的真实 sys_resume 拆出 D-25） |
| D-12 | `init.rs:675/706/719` | minix_sched 客户端占位（sched_start 假 endpoint） | 16-scheduling.md | A-8（`plan.md:202`） |
| D-13 | `exit.rs:33` | exit 路径 `sys_kill` no-op | 11-signal-core.md | 内核 sys_kill |
| D-14 | `exit.rs:101` | 退出进程自身 times 计账为 0 | 10-pm-wait.md | 内核 sys_times |
| D-15 | ~~`exit.rs:116`~~ | ~~`vm_willexit` 假装 Ok~~ **✅ 已修复**（2026-09-06，Fix #11：真实 `sendrec(VM, VM_WILLEXIT)` + 失败 panic 对齐 `forkexit.c:332-334`） | 02-stage-vm | ~~VM 协同面~~ wire 达成（真实往返挂 E5(a)/E1） |
| D-16 | `exit.rs:142` | core dump 路径名指针为 0 | 09-pm-exit.md | core dump 语义规划 |
| D-17 | `exit.rs:192` | `sched_stop` 假装 Ok | 16-scheduling.md | A-8 |
| D-18 | `exit.rs:215` | `sys_clear`（内核侧进程回收）no-op | 01-stage-kernel | 内核 sys_clear |
| D-19 | ~~`exit.rs:219`~~ | ~~`vm_exit`（页表回收）no-op~~ **✅ 已修复**（2026-09-06，Fix #12：真实 `sendrec(VM, VM_EXIT)` + 失败 panic 对齐 `forkexit.c:455-457`） | 02-stage-vm/22 | ~~VM 协同面~~ wire 达成（真实往返挂 E5(a)/E1） |
| D-20 | ~~`wait.rs:101`~~ | ~~trace-stop 返回码用模拟值~~ **✅ 已修复**（2026-09-06，Fix #6：真实 sigtrace 扫描 + sigdelset 消费 + 空集落环，forkexit.c:519-531 全语义） | 18-trace.md | ~~ptrace 停止状态建模~~ 已达成（trace_mask/trace.stopped 建模 D-10 时已备） |
| D-21 | `wait.rs:124` | rusage 跨地址空间拷贝假装成功（C 侧 `utility.c:92` 本身只填 utime/stime） | 10-pm-wait.md | sys_datacopy + rusage 范围决策 |
| D-22 | `signal.rs:160` | 内核调度器判定返回硬编码 false（"stub for 11, real in 16"） | 16-scheduling.md | A-8 |
| D-23 | `signal.rs:360-362` | SIGVTALRM/`check_vtimer` stub（`alarm.c:326-328`） | 14-itimer.md | 虚拟计时器 trait 落地（`timer.rs:165-167` 已定义 seam） |
| D-24 | `mproc/fork.rs:222-228` | `getticks()` 返回 0（TODO 注释：应向 CLOCK 请求） | 14-itimer.md / 内核 sys_times | 内核 uptime 面 |
| D-25 | `event.rs` `PmEventServices::resume` | `KernelResume` 生产适配器暂返回 OK（"内核无停止态可撤销"过渡契约），真实 `sys_resume`（`signal.c:282`）待内核调用面 | 13-signal-flow.md / edge E6 | minix-sys SYS_* 面（E6）；代码注释即验证锚点 |
| D-26 | `wait.rs` ZOMBIE/TRACE_STOPPED 两环的回复载荷 | wait4 回复的状态码载荷建模缺失：C 写 `mp_reply.m_pm_lc_wait4.status`（`ipc.h:1774-1779` `mess_pm_lc_wait4`），minix-types 无该 union 成员，现经 `ipc.reply` 机制只承载 m_type——`w_exitcode`/`w_stopcode` 到 wire 的最后一跳待补 | 10-pm-wait.md / edge E7 | minix-types 增 `MessPmLcWait4` + `m_pm_lc_wait4` 成员（E7 wire 系统化的一部分）；两环代码注释即锚点 |

不属于 DEFERRED 但同源的两个已知缺口（plan.md §4 已登记，此处仅索引）：A-7 定时器抽象（`plan.md:201`）、A-10 内核延迟调用 DELAY_CALL/SIGSNDELAY（`plan.md:204`）、A-13 进程组/会话设计层（`plan.md:207`）。

---

## 7. 对照 Redox 的架构参考

> 本次审查的联网调研受网络限制未完成，以下引用为 Redox 仓库路径级参考（基于既有知识整理，未逐行核对行号；采纳前建议对 GitLab 源码复核）。05 条均给出与 PM 现状的映射。

1. **进程管理的位置：内核 vs 用户态服务器。** Redox 没有独立的 PM 服务器，进程生命周期管理在内核 `redox-os/kernel` 的 `src/context/`（`Context` 结构 + `Status::{Runnable, Blocked, Stopped, Halted}` 状态机），用户态只通过系统调用与之交互。Minix3 把 PM 放用户态是微内核纯粹性的选择，本项目沿用。这决定了 minix-rs 的一个硬约束：**PM 的任何生命周期语义要端到端验证，内核 IPC（P1-3）是第一依赖**——Redox 不存在这层断档，其 context 状态机从第一天就是可运行的。
2. **状态建模：互斥枚举 + 阻塞原因结构化。** Redox 的 `Status` 枚举与本项目 `Lifecycle`（`mproc/lifecycle.rs`，ARCH A-2）是同一方向；Redox 进一步把"为何阻塞"表达为 context 的等待条件（wake 机制），而 minix-rs 的 `BlockState`（`mproc/block.rs`，VFS_CALL/EVENT_CALL/EventCall cursor）已经是组合子风格。可借鉴点：Redox 的唤醒条件与状态是绑定的（Blocked 必带 wake reason），`BlockState` 若未来出现"阻塞原因与恢复动作不匹配"的构造，可参考其把恢复动作编码进状态变体。
3. **信号的决策边界。** Redox 的信号投递决策在内核（`kernel/src/scheme/sig.rs` 的信号方案 + 用户态 trampoline 由 redox-rt 提供），Minix3 把全部决策（check_sig/sig_proc）放在 PM。minix-rs 保持 C 的边界是正确选择；对照的启示是：无论决策在内核还是服务器，**投递动作的内核接口（`sys_sigsend`/`sys_kill`，对应 D-13/D-18）必须与决策逻辑同步设计**，否则会出现 Redox 不存在的"决策完毕无法投递"断档。
4. **动态注册表 vs 编译期穷尽 match。** Redox 的 scheme 是动态集合，内核用注册表 + `SchemeId` 查找（`kernel/src/scheme/mod.rs`）；PM 的 47 个调用是编译期固定域，`PmCall` 穷尽 match（A-5）是更优解，编译器保证完整性。对照结论：**A-5 的方向不需要动摇，需要的是单一入口**（P1-1）——Redox 的注册表之所以可行，是因为所有 scheme 调用走同一条内核路径；PM 当前"一张死表 + 内联旁路"恰好破坏了这个前提。
5. **用户态服务的异步往返记账。** Redox 的 relibc/posix scheme 用句柄记账 + 事件总线管理异步回复（`relibc/src/platform/` 侧），与 PM↔VFS 的 11 路回复状态机（`ipc/vfs.rs`，`handle_vfs_reply`）是同型问题。PM 的状态机目前是 match 分派 + 各状态下手工回复，规模尚可控；若后续 11 路之外再膨胀（RS 协同、MIB 读取），可参考 Redox 把"句柄 → 挂起请求上下文"做成显式记账表，而不是扩展 match 臂数量。

---

## 8. 建议的推进顺序

1. **P1-2（假成功接缝改 fail-closed）**：改动最小、立刻让测试暴露虚构依赖，为 P1-5 的集成测试铺路。
2. **P1-1（分发收敛到单一 match）+ P1-4（codec 层）协同做**：先给 7 个已接线调用迁移（含解码下沉），再逐批点亮 40 个 ENOSYS 臂——每批 = wire 类型 + match 臂 + 表驱动测试（P1-5 方案 2）。
3. **P2-1（cfg 特性声明）+ P2-2（plan.md ARCH 表刷新）**：十分钟级改动，先恢复文档-代码同步与特性可测性。
4. **P1-5（集成测试目录）**：随 P1-1 每批迁移同步补 `run_once` 端到端场景。
5. **P1-3（内核边界）**：依赖 01-stage-kernel 的 minix-sys，跨阶段项；本阶段只做依赖登记核对（§6 已完成）与最小面倒推（P1-3 方案 1）。
6. **P1-6（命名统一）+ P2-3/P2-4（死代码与命名空间）**：机械性重构，安排在分发迁移后（避免重命名与迁移冲突）。
7. **P2-5/P2-6/P2-7 + P3**：随对应文档（09/10/14/00/99）的下一次触碰顺带完成；clippy 清理收尾。

---

## 9. 跨阶段条目抽取索引（edge_todo.md）

> **判定规则**：沿用 `notes/rewrite/fork-syscall-rewrite/edge_todo.md` 头部的三类定义——① 共享契约/基础设施层（minix-types、minix-sys）的缺陷与重构；② 对方 stage 目录里的生产代码；③ 多进程联调测试。stage 内生产代码（消费既有稳定契约，含 seam + mock 测试）不属于 edge，保留在本文件实施。
>
> **通电口径**（沿用 02-stage-vm campaign 惯例，模式 60 诚实契约）：依赖共享 trap 层/系统调用面的条目，PM 侧逻辑完备 + mock 测试即标 ✅，真实通电挂对应 edge 条目。

### 9.1 P/D 条目 → edge 条目映射

| 本文件条目 | stage 内保留部分 | 跨阶段部分（edge_todo.md） |
|---|---|---|
| P1-1 分发层分裂 | 全部 stage 内 | 消费的 wire 类型缺口随 P1-4 → E7 |
| P1-2 假成功接缝 | fail-closed 改造（返回 Err / unimplemented） | 真实 VM_FORK 往返 → E5(a)（联调）+ E1/E2（trap 与 SYS_FORK wire） |
| P1-3 内核边界未实现 | KernelIpcTransport 实现与最小内核面倒推 | trap 层 → E1；SYS_* wrapper → E2（VM 清单）/ E6（PM 清单，2026-09-06 新增） |
| P1-4 codec 层缺口 | PM 侧接线（match 臂消费 wire 类型） | wire 结构体系统化 → E7 |
| P2-3 死代码与调用号双址 | —（本体在 minix-types，非 PM 生产代码） | E7（死代码处置 + 调用号收敛 + SEND_PRIORITY/SEND_TIME_SLICE） |
| P1-5/P1-6、P2-1/P2-2/P2-4~P2-7、P3 | 全部 stage 内 | 无 |

### 9.2 D-XX 表（§6）跨阶段解除条件对照

| D-XX | 解除条件归属 |
|---|---|
| D-01 transport 三方法 unimplemented | E1（minix-sys trap 层） |
| D-02 BootParams::placeholder | E6（SYS_GETMONPARAMS/SYS_GETIMAGE——kernel 对端亦缺，需双侧新建） |
| D-03/D-04 send_vm_fork / send_kernel_request 假成功 | stage 内先改 fail-closed；真实往返 → E5(a) + E2（SYS_FORK wire） |
| D-09 sys_abort | E6（kernel 对端已实现，`os/kernel/src/syscall.rs:1793`） |
| D-13 sys_kill | E6（`os/kernel/src/syscall_signal.rs:148`） |
| D-15/D-19 vm_willexit / vm_exit | 02-stage-vm 生产代码 + 联调 → E5(a) |
| D-18 sys_clear | E6（`os/kernel/src/syscall_process.rs:366`） |
| D-21 rusage safecopy | E6（SYS_SAFECOPYFROM/TO，`os/kernel/src/syscall_copy.rs:361`） |
| D-24 getticks | E6（SYS_TIMES，`os/kernel/src/syscall_clock.rs:90`） |

其余 D-XX（D-05~D-08、D-10~D-12、D-14、D-16、D-17、D-20、D-22、D-23）的解除条件都在 PM stage 内或其归属文档（11/13/14/16/17/18）内，不涉及跨 stage。

### 9.3 不在本阶段登记的边界事项

- **PM↔VFS 协议联调**：`minix-types/src/ipc/vfs.rs`（873 行）类型已齐，双侧消费同一契约，无已知不一致——联调条目应由 05-stage-vfs 的扫描自行登记，此处不预设。
- **PM↔SCHED 客户端**（A-8，D-12）：`sched.rs` 的占位消费 SCHEDULING_* 消息契约，属 PM stage 内工作；缺失的 `SEND_PRIORITY`/`SEND_TIME_SLICE` 常量已并入 E7 范围。

---

## 10. 修复记录

> 每轮一个 TODO（todo-fix 工作流）：先讲现状与依据（fix-guard），再列候选设计（translate 防线：对比 Linux/Redox/OS 理论），实施 code + doc + test 三方同步，回归 review 后提交。

### ✅ Fix #1: P1-2（含 D-03/D-04）— VM fork 接缝 fail-closed 化（2026-09-06）

**File(s)**：
- `os/servers/pm/src/ipc/dispatcher.rs`（假成功占位 → 真实 `vm_fork` 自由函数，:105-155）
- `os/servers/pm/src/ipc/transport.rs`（`TestIpcTransport` 增加脚本化 sendrec 回复队列）
- `os/servers/pm/src/fork.rs`（`do_fork`/`do_srv_fork` 接线；新增 `queue_vm_fork_reply` 测试 helper）

**Before**：`send_vm_fork(request)` 不发送任何消息，直接返回 `Ok(VmForkOut { child_endpoint: from_generation_slot(1, child_slot) })`——fork 链路建立在捏造的 VM 应答上；`send_kernel_request(_request)` 同样假成功，且全仓零调用。

**After**：
1. **设计选型**（三案对比）：(a) 沿用 crate 内惯例——`IpcTransport` seam 上的自由函数 `vm_fork`（与 `tell_vfs` 同构）；(b) 仿 VM 侧 KernelGateway 建独立 `VmGateway` trait；(c) `IpcTransport` 加 provided method。选 (a)：PM 已有 transport seam，`tell_vfs` 先例证明自由函数足够；KernelGateway 存在的原因是 VM 当时缺 seam，再加 trait 是重复抽象；(c) 把 VM 协议知识泄漏进通用传输 trait。**C 依据**：`vm_fork` 是 `_taskcall(VM_PROC_NR, VM_FORK)`（libsys vm_fork.c:16-25）——普通任务调用而非内核调用，`send_kernel_request` 步骤本身与 C 不符（`proc` 复制由 VM 的 `sys_fork` 完成，PM 无独立内核 fork 请求），随假成功一并删除（D-04 处置）。
2. **wire 实现**：请求 m1i1=VMF_ENDPOINT / m1i2=VMF_SLOTNO，回复校验 `m_type == OK` 后读 m1i3=VMF_CHILD_ENDPOINT；传输失败或 VM 拒绝（非 OK）一律 `ForkCoordError::VmError`，fail-closed。errno 细粒度传播依赖 `PmError` 载荷变体（共享层，挂 edge E7）。
3. **测试脚本化**：`TestIpcTransport::queue_sendrec_reply`（VecDeque 整条回复出队，退回旧行为仅覆盖 m_type）；4 个依赖假应答的测试改脚本化（预期失败→通过），新增 3 个 `vm_fork` 单元测试（编码/解码、VM 拒绝、传输失败）。
4. **顺带对齐**：`do_fork` 的顺序注释修正为 C 同序（`find_free_slot` 只找槽不计数，`procs_in_use++` 在 `vm_fork` 成功后）——旧的"alloc_slot 在前需回滚"说法与代码不符。

**Verified**：
- `cargo test -p minix-pm --lib`：319 → **322 passed / 0 failed**（4 个假应答测试改造为脚本化 +3 个 vm_fork 新增）
- `cargo check -p minix-pm`：通过
- `grep -rn "send_vm_fork\|send_kernel_request" os/servers/pm/src/`：零命中（假接缝清除）

**Docs**：
- `07-pm-fork.md`：§1.2 回滚说法修正、§2.3 Rust 现状更新、D3 重写（假成功→真实 sendrec 全论证）、§4.1 步骤清单重写（8 步 C 同序 + 删 send_kernel_request 论证）、§4.5 不变量 3/4 行修正
- `08-pm-srv-fork.md`：D7 步数对齐（9→8 步）并引用 07 D3
- 本文件：§0 表 P1-2 行、P1-2 标题、§6 D-03/D-04 行标注

**未做（DEFERRED 论证）**：真实硬件上的 VM_FORK 往返（VM 服务器运行 + trap 层）挂 `edge_todo.md` E5(a)/E1——本条 stage 内目标（接缝 fail-closed + wire 正确 + mock 验证）已完整达成，符合通电口径。

### ✅ Fix #2: P1-1 — 分发收敛到单一分发表（2026-09-06）

**File(s)**：
- `os/servers/pm/src/ipc/calls.rs`（`dispatch_pm_call` 重写为 47 臂穷尽 match：7 个真实臂 + 40 个 ENOSYS 占位；签名扩展为 `(call, table, events, transport, caller, msg)`）
- `os/servers/pm/src/ipc/dispatcher.rs`（`dispatch_message` 事件回复臂接真实 `do_proc_event_reply`；签名同步扩展）
- `os/servers/pm/src/init.rs`（`run_once` 删除全部 7 个内联拦截块，只留 VFS 回复拦截 + 统一分发/回复；清理随之失效的导入）

**Before**：主循环用裸魔数（`msg.m_type == 2/41/1/3/11/42`）内联拦截 7 个调用，unsafe 解码散落 `init.rs`；`dispatch_pm_call(call, table, caller)` 只有 1 个 Fork 死臂（服务器路径永不触发）+ 46 个 ENOSYS；`table` 参数无用（clippy 报告）；`PROC_EVENT_REPLY` 在 `run_once` 与 `dispatch_message` 双路拦截。

**After**（设计选型，两案对比）：(a) **单一穷尽 match**（已选）——C `call_vec` 是一张表，所有调用同路；编译期保证 47 臂完整，接线进度一目了然。(b) 注册表 `fn(&mut Cx, &Message) -> ReplyIntent` 47 项（仿 C 函数指针的形）——可动态替换便于注入，但 47 个 handler 是编译期固定域，match 的穷尽性检查优于运行时表，且避免函数指针间接层；Redox 只对动态集合（scheme）用注册表。选 (a)。
- 载荷解码从 `init.rs` 收编进各 match 臂（仍按原 union 臂逐字段解码，语义不变：Fork 用 `m_source`、Exit 用 `m_lc_pm_exit.status`、Wait4 三字段、Kill/SrvKill 双字段、SrvFork 的 `m_lsys_pm_srv_fork`、ProcEventMask 的 `m_lsys_pm_proceventmask.mask`）。
- SUSPEND 映射保持 plan.md §7.3 契约：Fork Ok→ReplyLater / Exit→NoReply / Wait4→handler 意图 / Kill·SrvKill 的 is_exiting→ReplyLater / SrvFork→Reply(pid)。
- `dispatch_message` 的事件回复臂从 ReplyLater 钩子换成真实 `do_proc_event_reply`（06 已落地，钩子过时）。

**Verified**：
- `cargo test -p minix-pm --lib`：322 → **325 passed / 0 failed**（calls.rs 新增 fork 成功/父不存在/Exit NoReply 三测试，dispatcher.rs 新增事件回复非内核调用者 ENOSYS 测试；旧 ENOSYS 占位断言改指 GetPid）
- `cargo clippy -p minix-pm --lib`：`dispatch_pm_call` 的 unused `table` 参数告警消失（P3-2 该项闭环）
- `grep -n "msg.m_type == " os/servers/pm/src/init.rs`：零命中（裸魔数清除）

**Docs**：
- `04-ipc-dispatch.md`：§3.6 D6 表（46→40 + 接线计数）、差异论证段重写、§4.2 重写（单一分发表 + "主循环不得内联拦截"设计点）、§4.3 代码示例与钩子注记更新、§6 下一入口更新
- `plan.md`：A-5 状态"已实现"→"部分实现（7 接线/40 占位 + 单一表收敛说明）"
- 本文件：§0 表 P1-1 行、P1-1 标题

**未做（DEFERRED 论证）**：40 个未接线调用的点亮依赖各自归属文档（07~20）与 E7 wire 类型，不在本条范围。

### ✅ Fix #3: P1-5 — crate 外端到端集成测试层（2026-09-06）

**File(s)**：
- `os/servers/pm/tests/run_once_integration.rs`（新增，6 个端到端场景）
- `os/servers/pm/src/init.rs`（`run_once` 改 pub 单步驱动接口；新增 `table_mut`/`transport`/`transport_mut` 访问器）

**Before**：319 个单元测试全部针对逻辑模块；分发测试验证 `dispatch_message`/`dispatch_pm_call` 而生产走 `init.rs` 内联路径（Fix #2 后已同路）；跨 crate 集成测试 `os/tests/pm_vm_fork.rs` 整体停用且无替代——"从收到 Message 到发出 Reply"的完整链路零覆盖。

**After**（设计选型）：测试位置两案——(a) crate 外 `tests/` 目录（已选）：真外部视角，只能走公共 API，防止测试绕过封装（旧 pm_vm_fork 之死正是内部类型耦合）；(b) crate 内 `#[cfg(test)]` 模块：可触私有状态但等于仍是"内部视角"。选 (a)，配套最小公共面：`run_once` pub（单步驱动接口，`run()` 循环体即调它）+ `table_mut`/`transport_mut` 访问器（对应 C 在 main 循环前直接填 `mproc`/预置消息的 harness 播种，生产路径不经过）。
- 六场景：① fork 全链路 wire 序列（VM_FORK → VFS_PM_FORK + SUSPEND 零 caller 回复 + 子槽 VFS_CALL）；② exit 永不回复 + 父 wait 中 → ToldParent；③ wait4 无子 → ECHILD；④ kill 无目标 → ESRCH；⑤ 未接线调用 → ENOSYS；⑥ 损坏 VFS 回复 → fail-fast panic（非 ENOSYS 兜底）。
- 播种用 `BootParams::placeholder()`（空 boot image，不产生进程）+ `table_mut` 手工填表——对应 C 语义，不依赖 init 流程。

**Verified**：
- `cargo test -p minix-pm --lib`：**325 passed**（无回归）
- `cargo test -p minix-pm --test run_once_integration`：**6 passed / 0 failed**
- 测试名对账：§5.1 表行 9-18 按当代测试名刷新（旧 `test_dispatch_fork_is_reply_later` 等已演化名全部 grep 命中）

**Docs**：`04-ipc-dispatch.md` §5.1（行 9-18 测试名对账刷新 + 新增 14a 行）、§5.2 统计更新（325 lib + 6 integration）；本文件 §0 表与标题标注。

**未做（DEFERRED 论证）**：跨服务器联调（PM↔VM fork 全链路双活）挂 `edge_todo.md` E5(a)——本条目标是 crate 内端到端（消息面），与 E5(a)（多进程通电）分层不重叠。

### ✅ Fix #4: P1-6 — 入口命名统一回 C 名（2026-09-06）

**File(s)**：
- `os/servers/pm/src/{fork,exit,wait,signal,ipc/calls}.rs`（6 个协调器重命名：`handle_fork→do_fork`、`handle_srv_fork→do_srv_fork`、`handle_exit→do_exit`、`handle_wait4→do_wait4`、`handle_kill→do_kill`、`handle_srv_kill→do_srv_kill`；含测试名与注释同步）
- `tools/coverage-extract/pm-semantic-map.json`（补 `is_sane_timeval → Timeval::is_sane` 方法化改名映射）
- 文档 05/07/08/09/10/11/12/04 + 本文件的散文引用同步

**Before/After**：重命名比 P1-6 原清单（4 个）多收编 2 个——`do_fork`/`do_srv_fork` 的 C 名同样被 `handle_*` 遮蔽，属同构问题一并统一；`handle_vfs_reply`/`handle_clock_notify` 保留（前者本身就是 C 函数名 main.c:295）。P1-6 原方案 1 的理由全部兑现：同文件已保留 C 名的内部函数一致、覆盖率工具零误报、改名未换来语义信息。

**Verified**：
- `cargo test -p minix-pm`：**325 lib + 6 integration passed**（纯重命名零语义变化）
- Gate A 复测（coverage-extract.py）：Rust 名称匹配 **89.0% → 93.6%**，文档覆盖 **98.2% → 100%**；剩余 7 个未匹配均已核实为非缺口（SEND_* 挂 E7、ESCRIPT 是 C 死代码、EXTERN/_SYSTEM/_TABLE 是 C 编译宏）
- `grep -rn "fn handle_" os/servers/pm/src/`：仅剩 `handle_vfs_reply`/`handle_clock_notify`

**Docs**：`07-pm-fork.md` §4.1/§5 测试表（`test_do_fork_success` 等新名）、`08-pm-srv-fork.md` D7、`09/10/11/12` 散文、`05-vfs-interaction.md` §接线记录、`04-ipc-dispatch.md` §4.2、本文件 §1.1/§1.2 矩阵。

**未做（DEFERRED 论证）**：无——本条为纯机械重命名，无外部依赖。

### ✅ Fix #5: D-10 — tracer SIGSTOP 与 TO_TRACEFORK 条件继承（2026-09-06）

**File(s)**：
- `os/servers/pm/src/fork.rs`（新增共享决策函数 `inherit_guardianship`；`copy_mproc` 的监护写入改条件继承；`do_fork` 第 8 步 / `do_srv_fork` 第 7 步接真实 `crate::signal::sig_proc(child, SIGSTOP, trace=true, ksig=false)`；+3 测试）

**Before**：两处 no-op 注释（"DEFERRED — 11-signal-core.md"）；且 `copy_mproc`/`srv_fork_from` 无条件把子进程监护重置为 `Normal`——即使接线了 `sig_proc`，`tracer().is_some()` 也恒假（探索阶段发现的**隐藏阻塞**：C 经 `*rmc=*rmp` 整体复制继承 tracer，Rust 的显式构造路径把它丢了）。

**After**（设计选型）：监护继承的落点两案——(a) 提取共享 `inherit_guardianship(parent, parent_slot)` 供两条构造路径复用（已选）；(b) 在 `Guardianship`/构造函数内部隐式继承（改 `srv_fork_from` 签名或语义）。选 (a)：C 的语义点是显式的（复制后条件清除），Rust 的显式构造哲学下把决策函数放在编排层与 C 的"复制 → 条件清除"两段式同构，且不污染 `mproc` 层构造器的无副作用性。语义对照 `forkexit.c:87-96`：父 `Traced` + `TRACEFORK` → 子继承（`trace_exit=false`，对应 `FORK_INHERIT_FLAGS` 不含 `TRACE_EXIT`）；否则 `Normal`。`sig_proc` 的 trace 分支（`signal.c:384` → `signal.rs:210-216`）置 `sigtrace` SIGSTOP 位 + `trace.stopped`。

**Verified**：
- `cargo test -p minix-pm`：325 → **328 lib passed**（+3：TO_TRACEFORK 继承+停止 / 无 TRACEFORK 清除+运行 / srv 路径继承）+ 6 integration
- `grep -rn "DEFERRED" os/servers/pm/src/fork.rs`：零命中

**Docs**：`07-pm-fork.md` §2.7 重写（继承链论证）、D7 重写（DEFERRED → 落地记录 + 隐藏阻塞说明）、§4.1 步骤 8、§5.2 测试表 +3、§5.3 对账刷新；`08-pm-srv-fork.md` §2.7、步骤 8、§5.2/§5.3 对账；本文件 §6 D-10 行。

**未做（DEFERRED 论证）**：无——`sig_proc` 的 ptrace 停止态表达（`trace.stopped` + `sigtrace`）已在 crate 内自洽；真实 `sys_trace` 停止调用挂内核面（E6 SYS_TRACE 范围，08-trace 文档域）。

### ✅ Fix #6: D-20 — wait4 的 TRACE_STOPPED 环真实化（2026-09-06）

**File(s)**：
- `os/servers/pm/src/wait.rs`（TRACE_STOPPED 环重写 + 2 测试）

**Before**：停止态子进程一律返回 `w_stopcode(5)`（SIGTRAP 硬编码占位），不读 `trace_mask`、不消费信号位——tracer wait 到的停止信号是虚构的。

**After**：对齐 `forkexit.c:519-531` 全语义——扫描 `SignalState::trace_mask`（`mp_sigtrace` 的 Rust 表达）取最低待报告信号 → 清位（`sigdelset`）→ 回复载荷 `W_STOPCODE(i)` → 返回 pid；**sigtrace 为空时落出该环**继续 ZOMBIE 环（C 的 for 未命中即落出），不虚构停止码。设计说明：消费位是 C 语义的一部分（同一停止信号只报告一次），与 D-10 落地的 `sig_proc` trace 分支（置位）构成完整的"投递 → 缓冲 → 报告"链。

**Verified**：
- `cargo test -p minix-pm`：328 → **330 lib passed**（+2：最低位优先消费且余位保留 / 空集落环不虚构）+ 6 integration
- `grep -n "w_stopcode(5)" os/servers/pm/src/wait.rs`：零命中

**Docs**：`10-pm-wait.md` §4.1 实现描述更新（占位 → 真实语义 + 测试名）；本文件 §6 D-20 行。

**未做（DEFERRED 论证）**：无——链路两端（D-10 置位端、本条消费端）均已在 crate 内闭合。

### ✅ Fix #7: D-11 — 事件终止分派 Signal 分支接真实 restart_sigs（2026-09-06）

**File(s)**：
- `os/servers/pm/src/signal_flow.rs`（新增 supertrait `RestartServices: KernelResume + ExitHandler + SignalDeliver`；`restart_sigs` 签名从 3 个 trait 对象收敛为 1 个；3 个测试改用合并 mock）
- `os/servers/pm/src/event.rs`（新增生产适配器 `PmEventServices`；Signal 分支从"仅清标志"no-op 接真实 `restart_sigs`；+1 端到端测试）
- `os/servers/pm/src/signal.rs`（`sig_proc` 的未使用 transport 参数从 `&mut dyn` 放宽为 `<T: IpcTransport + ?Sized>`——消除 `?Sized` 泛型调用链上的 dyn 强制转换死结）

**Before**：`resume_event` 的终止分派中 Signal 分支是 no-op（注释自述"13 落地时替换"）——事件重投语义缺失；Exit 分支实际已接线但注释仍称"两者 DEFERRED"（过时注释）。

**After**（设计选型）：适配器与签名的组合两案——(a) 三个独立 trait + 三个 `&mut`（原设计）：生产装配时三者在同一调用帧共存，而它们共享 `transport` 的 `&mut`，借用检查器拒绝；为绕开会要求 `RefCell`/raw pointer。(b) **supertrait 合并 + trait upcasting**（已选，2024 edition 后的社区惯用法）：`restart_sigs(table, target, &mut dyn RestartServices)`，函数体内按需上转为 `&mut dyn ExitHandler` 等——单一借用、mock 装配更简、成员 trait 保留使 `check_pending`/`stop_proc` 等单注入点消费者不受影响。C 依据：C 的 `restart_sigs` 直接调模块级函数，trait 拆分本就是 Rust 侧测试注入的手段，合并不改变注入语义。
- `PmEventServices::resume` 返回 OK 的过渡契约：`block.stopped` 在当前世界由 PM 侧 `unpause`/`stop_proc` 自行置位，内核侧无真实停止态可撤销——"无内核动作"是真话而非捏造（与 P1-2 的假成功有本质区别：不虚构任何数据）；真实 `sys_resume` 登记 D-25 挂 E6。

**Verified**：
- `cargo test -p minix-pm`：330 → **331 lib passed**（+1 端到端：SIGNAL 事件终止分派 → check_pending 重投 SIGKILL → sig_proc 终止 → 僵尸化；途中确认事件推断的 UNPAUSED 前提）+ 6 integration
- `grep -n "仅清标志" os/servers/pm/src/event.rs`：零命中

**Docs**：`13-signal-flow.md` D5（签名收敛论证）+ §4.4 代码块、`06-event-subscription.md` §2.10 钩子行（DEFERRED → 落地 + D-25 引用）；本文件 §6 D-11 行 + 新增 D-25 行。

**未做（DEFERRED 论证）**：`KernelResume::resume` 的真实 `sys_resume`（D-25）——依赖 minix-sys 内核调用面（E6），按通电口径以显式契约过渡。

### ✅ Fix #8: D-06 — VFS 端口 `exit_proc` 委托 09 退出链（2026-09-06）

**File(s)**：
- `os/servers/pm/src/ipc/vfs.rs`（生产 impl 的 `exit_proc` 从 `unimplemented!()` 改为委托 `crate::exit::exit_proc`；+1 单测）
- `05-vfs-interaction.md`（端口落地状态两处刷新）

**Before/After**：生产端口一行委托（`status as i8` 截断对应 C 的 exit_status 语义，`dump_core` 直传），C 锚点 `main.c:381`（FORK 调度失败 `exit_proc(rmp, -1, FALSE)`）。设计说明：端口处不做任何逻辑（无重试、无状态修补），09 的 `exit_proc` 全链自带 dump_core 双门与收养链——端口的职责只是"把 VFS 回复翻译成 PM 内部调用"。

**Verified**：
- `cargo test -p minix-pm`：**331 lib + 6 integration passed**；新单测断言子进程离开 Running + `VFS_PM_EXIT` 已发送
- 注意：该端口当前在 FORK 失败分支的可达性仍被 D-05（`sched_start_user` 非 KERNEL/NONE 时 `unimplemented!`）遮蔽——非内核调度器的调度失败要到 A-8（16-scheduling.md）落地才可能发生；本条完成的是"端口就绪"，分支可达性归 D-05

**Docs**：`05-vfs-interaction.md` 两处端口状态行；本文件 §6 D-06 行。

### ✅ Fix #9: D-07 — `set_core_flag`（WCOREFLAG）落地（2026-09-06）

**File(s)**：
- `os/servers/pm/src/ipc/vfs.rs`（生产 impl 真实实现 + 1 单测；非 Exiting 目标 fail-fast 对应 C `main.c:362` 的 assert）
- `os/servers/pm/src/mproc/lifecycle.rs`（`Exiting.sig_status` 字段文档补 bit7 = WCOREFLAG 位语义）
- `os/servers/pm/src/wait.rs`（ZOMBIE 环的 `w_exitcode` 组合改 `as u8 as i32` 无符号字节语义 + 1 端到端测试）

**Before/After**：生产端口一行位运算（`sig_status = sig_status as u8 | WCOREFLAG as u8`），C 锚点 `main.c:357-358`。设计说明：(a) 不新增独立 sigstatus 字段——`Lifecycle::Exiting.sig_status` 已是 C `mp_sigstatus` 信号字节的 Rust 等价物，WCOREFLAG 就是它的 bit7（i8 承载 0o200 是位语义问题不是模型问题）；(b) **连带修复**：wait4 组合处 `ec as i32/ss as i32` 的符号扩展会破坏 bit7 与退出码 0xFF——改为 `as u8 as i32` 字节语义（C `W_EXITCODE(status,sig) = status<<8|sig` 全程无符号字节）。

**Verified**：
- `cargo test -p minix-pm`：331 → **334 lib passed**（+1 set_core_flag 位运算与 fail-fast、+1 僵尸→ToldParent 的 WCOREFLAG 位保留与 wire m_type）+ 6 integration
- Core 分支可达性：`VfsReply::Core { status == OK }` → `set_core_flag`（`vfs.rs:255-262`），与 C fallthrough 到 EXIT 分支一致

**Docs**：`lifecycle.rs` 字段文档；本文件 §6 D-07 行 + **新增 D-26**（wait4 回复载荷的 wire 建模缺失——`mess_pm_lc_wait4.status` 在 minix-types 无对应成员，挂 E7）。

**未做（DEFERRED 论证）**：wait 状态码到 wire 的最后一跳（D-26）——共享层 minix-types 缺 union 成员，属 E7 范围，stage 内不私改共享层。

### ✅ Fix #10: D-08 — VFS 端口 `exec_restart` 接通 17 全语义（2026-09-06）

**File(s)**：
- `os/servers/pm/src/exec.rs`（`KernelExec::reply`/`TracerSig::send` 增加 `table`/`caller` 参数；新增 supertrait `ExecRestartServices`；`exec_restart`/`do_execrestart` 收敛为单一 svc 参数；3 个测试的 mock 合并重构）
- `os/servers/pm/src/ipc/vfs.rs`（新增生产端口 `ExecServices`：exec→`-ENOSYS`（E6 契约）、kill→显式 no-op（E6/D-13 家族）、reply→transport 发送、send→`check_sig`；生产 `exec_restart` 委托 17 全语义）
- `17-exec.md`（§4.4 签名同步）

**Before/After**：设计选型——(a) 端口持 `&mut ProcTable`+`&mut transport`：与 `exec_restart` 自身的表借用冲突，无解；(b) **trait 方法携带 `table` 参数 + 端口只持 transport**（已选）：`exec_restart` 保留表的独占所有权（它需要大量改表），端口方法被调用时拿到表的转引用——生产/测试两种装配都成立；(c) 完全合并进 restart_sigs 式单 trait——同 (b) 但方法粒度保留（exec/kill 不需要表就不传）。`KernelExec::exec` 的生产实现返回 `-ENOSYS`（内核调用面挂 E6），由 `exec_restart` 尾部已有的 panic 承接（C `exec.c:198` 同型 panic）——失败可观测而非伪造成功，符合通电口径。

**Verified**：
- `cargo test -p minix-pm`：**334 lib + 6 integration passed**（exec 10 项全过：失败回复/PARTIAL 拆除/caught 复位/tracer 信号）
- `grep -n "unimplemented" os/servers/pm/src/ipc/vfs.rs`：仅剩 D-05/D-09 两处（各有独立的阻塞依赖）

**Docs**：`17-exec.md` §4.4 三行签名；本文件 §6 D-08 行。

### ✅ Fix #11: D-15 — `vm_willexit` 真实化（2026-09-06）

**File(s)**：
- `os/servers/pm/src/ipc/dispatcher.rs`（新增 `vm_willexit` 自由函数，与 `vm_fork` 同构；+2 单测）
- `os/servers/pm/src/exit.rs`（`exit_proc` 步骤 6 从 no-op 接真实调用；失败 panic 与 C 同文案）

**Before/After**：C 语义（`forkexit.c:332-334`）：`vm_willexit` 失败即 panic——VM 的内存记账依赖该预告，缺失永久失衡，不可恢复。Rust 侧 wire：`_taskcall(VM, VM_WILLEXIT)`，载荷 `VMWE_ENDPOINT`（m1i1，`com.h:644`），无回复载荷；传输失败收敛为 `-EIO`、VM 拒绝透传 errno，调用方以同文案 panic。`?Sized` 泛型保持与 `exit_proc` 的调用链兼容。

**Verified**：
- `cargo test -p minix-pm`：334 → **336 lib passed**（+2：endpoint 编码/OK 应答、VM 拒绝透传）+ 6 integration
- `grep -n "vm_willexit" os/servers/pm/src/exit.rs` → 真实调用点

**Docs**：`09-pm-exit.md` §3 占位行刷新；本文件 §6 D-15 行。

**未做（DEFERRED 论证）**：VM 侧真实处理（对端记账语义）与硬件往返挂 `edge_todo.md` E5(a)/E1——PM 侧 wire 与失败语义已完备。

### ✅ Fix #12: D-19 — `vm_exit` 真实化（2026-09-06）

**File(s)**：
- `os/servers/pm/src/ipc/dispatcher.rs`（新增 `vm_exit` 自由函数；+2 单测）
- `os/servers/pm/src/exit.rs`（`exit_restart` 步骤 5 接真实调用；参数 `_transport` 更名 `transport`——它终于被使用了；失败 panic 与 C 同文案）
- `os/servers/pm/src/event.rs`（3 个测试的 wire 断言按新现实刷新：终止分派现在包含一条 VM_EXIT 发送）

**Before/After**：C 语义（`forkexit.c:455-457`）：`vm_exit` 失败即 panic——页表随进程终结，VM 不回收即永久泄漏。Rust wire：`_taskcall(VM, VM_EXIT)`，载荷 `VME_ENDPOINT`（m1i1，`com.h:631`）。连带修正：`_transport` 参数更名——"未使用参数"的过渡标记随真实接线自然消失。

**Verified**：
- `cargo test -p minix-pm`：336 → **338 lib passed**（+2 vm_exit 单测）+ 6 integration（3 个 event 测试断言从 is_empty 更新为含 VM_EXIT 的精确计数）
- `grep -n "stubbed" os/servers/pm/src/exit.rs`：仅剩 D-13/D-14/D-16/D-17/D-18 家族（各有登记）

**Docs**：`09-pm-exit.md` 占位行刷新；本文件 §6 D-19 行。

**未做（DEFERRED 论证）**：VM 侧真实页表回收与硬件往返挂 `edge_todo.md` E5(a)/E1。
