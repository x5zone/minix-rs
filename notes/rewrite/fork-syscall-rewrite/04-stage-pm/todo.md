# 04-stage-pm Rust 实现架构级 Review TODO

> 来源：2026-09-06 架构级代码审查 + 查漏补缺（关注整体与分层架构，非逐函数审查）。
> 范围：`os/servers/pm/src/` 全部 Rust 代码（37 个文件，14,098 行），以及 `os/libs/minix-types/` 中与 PM 相关的类型边界。
> 方法：先做查漏补缺（Gate A 覆盖度枚举：`coverage-extract.py` 对 `minix3/minix/servers/pm` 提取 109 个 C 符号，与文档和 Rust 侧逐一对照；47 个调用号矩阵；DEFERRED/stub 全量收敛；Minix3 易漏语义点逐条 grep 验证），再做整体到分层的架构审查（工作区边界 → 服务器骨架 → 分发层 → 子系统层 → mproc 状态层 → 测试层），对照 Redox 实现与 Rust/OS 社区最佳实践。
> 定位：本文档是查漏补缺清单与架构改进建议清单，**不同于** `draft/`（旧 fork 主线素材，已停止维护）与 `plan.md` §7（文档 review 记录）。
> 状态（2026-09-06，campaign 完成后更新；2026-09-08 账目对账修正）：本轮 21 次 todo-fix 迭代完成——P1 闭环 4 项（P1-1/P1-2/P1-5/P1-6），P2 闭环 5 项（P2-1/P2-2/P2-4/P2-5/P2-7），P3 闭环 2 项；仍开放 P1-3/P1-4/P2-3/P2-6。D-XX 缺口 26 项中 20 项完整实现（D-03/04/06/07/08/09/10/11/13/14/15/18/19/20/21/22/23/24/25/26），余 6 项（D-01/D-02/D-05/D-12/D-16/D-17）均为带"依赖未解除"论证的跨阶段通电项（挂 edge E1/E2/E5/E6/E7 或 16-stage SCHED）。测试基线 319 → **346 lib + 7 integration passed**，clippy lib 0 warning 0 error，Gate A 名称匹配 89.0% → 93.6%。逐条证据见 §10 修复记录（Fix #1–#28；#27 于 2026-09-08 补记，对应提交 de9415604 + d79307ee6）。
> 增补（2026-09-06）：跨阶段条目抽取见 §9；登记于 `notes/rewrite/fork-syscall-rewrite/edge_todo.md`（本阶段新增 E6/E7，既有 E1-E5 为 02-stage-vm campaign 条目）。
> **第 2 轮（2026-09-08，§11）**：全量查漏补缺 + 架构审查重跑。新基线 346 lib + 7 integration / clippy 0 / Gate A 93.6%；新发现 V2-P0×2（信号终止路径 mock transport 泄漏、信号集合位约定分裂）、V2-P1×2（PID 相位偏移、SIGHUP 广播缺失 + D-13 台账错位）、V2-P2×6、V2-P3×4；已完成条目正文压缩为索引（结论保留，论证见 git 历史）；Redox 对照按 2024-12 起的 procmgr 迁移事实更新（§11.3）。

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
| P2 | P2-1 | `cfg(feature = "syscall_stats"/"sprofile")` 使用了未在 Cargo.toml 声明的特性，被门控代码永久编译排除（**✅ 已修复** 2026-09-06，见 §10 Fix #17） |
| P2 | P2-2 | plan.md §4 ARCH 表与代码失同步：A-5 宣称"已实现"实为 1 个死臂，A-9 宣称"缺口"实际已落地（**✅ 已修复** 2026-09-06，见 §10 Fix #20） |
| P2 | P2-3 | `minix-types/src/ipc/pm.rs` 的 `PmRequest`/`PmResponse` 是零使用的死代码；PM 调用号单一真值破口 |
| P2 | P2-4 | `lib.rs` glob re-export 压平命名空间，5 对同名双层模块（`fork`/`mproc::fork` 等）加剧混淆（**✅ 已修复** 2026-09-06，见 §10 Fix #18） |
| P2 | P2-5 | exit/wait 路径 9 处行为 stub 以散落注释存在，未按模式 60 登记为显式 DEFERRED 契约（**✅ 已修复** 2026-09-06，见 §10 Fix #19） |
| P2 | P2-6 | 文档 00/99 仍是最小骨架，且 `.design/` 缺这两篇的 outline/outline-review/design 快照（模式 69） |
| P2 | P2-7 | C 侧死代码 `ESCRIPT`（exec.c:31，定义后零使用）未登记进 plan.md §5.4 排除表（**✅ 已修复** 2026-09-06，见 §10 Fix #20） |
| P3 | P3-1 | clippy 约 50 条告警未清理（文档缩进 17、可折叠 if 11、可派生 impl 6 等）（**✅ 已修复** 2026-09-06，见 §10 Fix #21） |
| P3 | P3-2 | 无用导入与无用参数（`Lifecycle` 两处、`dispatch_pm_call` 的 `table` 参数已无用）（**✅ 已修复** 2026-09-06，Fix #2/#21 闭环；见 §10） |

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
| Minix3 易漏语义点覆盖率高：fork 的 LAST_FEW 非 root 预留、session leader 死亡向进程组发 SIGHUP、孤儿重挂 INIT 的 NEW_PARENT、TO_TRACEFORK、TAINTED、PARTIAL_EXEC、`mp_sigmask2`、NR_PIDS=30000、itimer 三族 | `os/servers/pm/src/fork.rs:35`、`src/exit.rs:76-80` 与 `exit.rs:176`、`src/exit.rs:407`（`disinherit`）与 `exit.rs:544`（`test_disinherit_new_parent`）、`src/mproc/guardianship.rs:58-59`、`src/exec.rs:96-100`、`src/mproc/mproc.rs:165-166`、`src/mproc/signal.rs:60`、`src/mproc/constants.rs:24-26`、`src/timer.rs:32-34`。**⚠️ V2 勘误（2026-09-08）**：本行两处过度声称——"session leader 死亡发 SIGHUP"实为未实现（见 §11 V2-P1-2）；"NR_PIDS=30000"存在但相位与 C 偏移（§11 V2-P1-1）。其余各点 V2 复核仍成立。 |
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

已完成（Fix #2）：7 个内联拦截块收编进 `dispatch_pm_call` 单一穷尽 match，主循环不再绕过分发表。原问题（两套分发并存、Fork 死臂、调用表三处事实源分裂、单测验证测试路径）的完整论证见本文件 git 历史（2026-09-08 压缩前版本）。

### P1-2 服务间请求的"假成功"接缝（✅ 已修复 2026-09-06，见 §10 Fix #1）

已完成（Fix #1）：`send_vm_fork` 改经 `IpcTransport` 真实 sendrec 且 fail-closed；`send_kernel_request` 判定为与 C 不符的原型残留，随假成功一并删除（D-03/D-04 同时闭环）。V2 轮发现同型残留一处：`sig_proc_exit` 的 mock transport（§11 V2-P0-1）。

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

已完成（Fix #3）：新增 `tests/run_once_integration.rs`（7 场景）驱动 `PmServer::run_once` 完整消息循环：fork 全链路 wire 序列、exit 永不回复、wait4 三环、kill ESRCH、未接线 ENOSYS、损坏 VFS 回复 panic、僵尸回收载荷。

### P1-6 入口函数命名约定分裂，损害 C↔Rust 可追溯性（✅ 已修复 2026-09-06，见 §10 Fix #4）

已完成（Fix #4）：6 个 `handle_*` 入口统一回 C 名 `do_*`，Gate A 覆盖率工具名称匹配恢复（V2 复测 93.6%）。

---

## 4. P2：结构性改进（正确性 gate 通过后规划）

### P2-1 虚构的 cfg 特性门：`syscall_stats` 与 `sprofile`（✅ 已修复 2026-09-06，见 §10 Fix #17）

已完成（Fix #17）：Cargo.toml 声明 `syscall_stats`/`sprofile` 特性（默认关，与 C 编译宏一致），门控代码恢复可编译性。

### P2-2 plan.md §4 ARCH 表与代码状态失同步（✅ 已修复 2026-09-06，见 §10 Fix #20）

已完成（Fix #20）：plan.md §4 ARCH 表按实现现状刷新（A-5 拆分"建模已实现/分发接线 7/47"、A-9 改"已实现"）。

### P2-3 `minix-types/src/ipc/pm.rs` 的遗留类型是零使用死代码；调用号单一真值破口

**问题**：`PmRequest`/`PmResponse`（`os/libs/minix-types/src/ipc/pm.rs:13-38`）在包括 pm crate 在内的整个 os/ 工作区零使用（`PmError` 除外，它被 `init.rs:390` 使用）。同时 PM 调用号出现两处表达：`pm/src/ipc/calls.rs` 的 `PmCall` 枚举（47 值，注释明确"将来内核侧需要时再上移 minix-types"，`calls.rs:16-17`）与 `minix-types` 里的个别常量（`init.rs:368` 使用 `minix_types::PM_PROCEVENTMASK`）。同一个"47 个调用号"的事实，一部分住在 pm crate、一部分住在 minix-types。

**建议**：(a) 删除 `PmRequest`/`PmResponse`（或等 codec 层 P1-4 设计时再决定去留，但要在 P1-4 的方案里显式处置它们，不能默认保留）；(b) `PM_PROCEVENTMASK` 这类常量要么下沉回 pm crate 统一从 `PmCall` 取值，要么在 `calls.rs:16-17` 的"单一事实源"注释里写清当前的双址现状与收敛计划——现状是注释宣称单一真值，代码已经双址。

**跨阶段拆分**：本体在 minix-types（共享契约层），处置归 `edge_todo.md` E7；PM 侧在本条目闭环时引用 E7 的收敛结论，不在 stage 内单独改动共享层（2026-09-06 增补）。

### P2-4 `lib.rs` glob re-export + 5 对同名双层模块（✅ 已修复 2026-09-06，见 §10 Fix #18）

已完成（Fix #18）：`lib.rs` 移除 glob re-export，公共 API 显式逐项导出，logic/state 两层路径不再压平。

### P2-5 exit/wait 路径的行为 stub 应升级为显式 DEFERRED 契约（✅ 已修复 2026-09-06，见 §10 Fix #19）

已完成（Fix #19）：exit/wait 路径散落 stub 注释统一为 `[DEFERRED: D-XX]` 显式契约（登记表见 §6）。

### P2-6 文档 00/99 仍是最小骨架，且缺 `.design/` 快照

**问题**：`00-pm-overview.md` 与 `99-global-concepts.md` 均为 16 行骨架，状态行自记"pending（最小骨架，待改写）"；`tools/design-coverage-check.sh fork-syscall-rewrite --stage 04-stage-pm` 报告两篇各缺 outline/outline-review/design 三个快照（模式 69 PSMD）。01–20 的快照齐全（63 个文件）。此外 plan.md §6.1 的状态表（`plan.md:300/321`）将 00/99 记为"骨架 —"，与文档头状态一致，这点是同步的。

**建议**：00（总览）与 99（全局概念）是 22 篇的入口与收尾，plan.md §6.2（`plan.md:329`）已排为最后优先级，维持该排序即可；但建议在改写 00 时顺手把本文件（todo.md）的 P1-1/P1-3 结论纳入"当前实现状态"叙述，避免总览写成后立刻过时。快照按 Step 0.3 流程在下次触碰这两篇的 review 中补齐。

### P2-7 C 死代码 `ESCRIPT` 未登记进排除表（✅ 已修复 2026-09-06，见 §10 Fix #20）

已完成（Fix #20）：C 死代码 `ESCRIPT`（exec.c:31）登记进 plan.md §5.4 排除表。

---

## 5. P3：代码卫生

### P3-1 clippy 告警清理（约 50 条）（✅ 已修复 2026-09-06，见 §10 Fix #21）

已完成（Fix #21）；V2 基线复核（2026-09-08）：`cargo clippy -p minix-pm --lib` 0 warning 0 error 维持。

### P3-2 无用导入与无用参数（✅ 已修复 2026-09-06，见 §10 Fix #21）

已完成（Fix #2/#21）：无用导入（`Lifecycle` 两处）与 `dispatch_pm_call` 无用 `table` 参数已清理。

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
| D-09 | ~~`ipc/vfs.rs:488`~~ | ~~`sys_abort` `unimplemented!()`~~ **✅ 已修复**（2026-09-07，Fix #25：`KernelGateway::sys_abort` + minix-sys `sys_abort` wrapper（E6 sys_abort 切片）+ PmServices 端口接通（REBOOT 特例 `main.c:304-312`，C 忽略返回值语义保留）；真实通电挂 E1） | 01-stage-kernel | ~~内核 sys_abort~~ wrapper 达成（通电挂 E1） |
| D-10 | ~~`fork.rs:86-99/175-186`~~ | ~~tracer SIGSTOP 记意图不执行~~ **✅ 已修复**（2026-09-06，Fix #5：真实 `sig_proc` + `inherit_guardianship` 的 TO_TRACEFORK 条件继承，含隐藏阻塞 copy_mproc 重置监护的修复） | 11-signal-core.md | ~~sig_proc 可跨模块调用~~ 已达成 |
| D-11 | ~~`event.rs:170/230`~~ | ~~事件重启的 exit_restart/restart_sigs 仅清标志~~ **✅ 已修复**（2026-09-06，Fix #7：Signal 分支接真实 restart_sigs，PmEventServices 适配器；Exit 分支本已接线且注释过时） | 13-signal-flow.md | ~~restart_sigs 完整实现~~ 已达成（KernelResume 的真实 sys_resume 拆出 D-25） |
| D-12 | `init.rs:675/706/719` | minix_sched 客户端占位（sched_start 假 endpoint） | 16-scheduling.md | A-8（`plan.md:202`） |
| D-13 | ~~`exit.rs:33`~~ | ~~exit 路径 `sys_kill` no-op~~ **✅ 已修复**（范围注记 2026-09-08：本行只覆盖 do_exit 的 PRIV_PROC 违规分支一处；SIGHUP 广播两站点已拆出为 D-27，见下）（2026-09-06，Fix #23：`do_exit` PRIV_PROC 分支经 `KernelGateway`/`TrapKernelGateway` 真实发送 `sys_kill(endpoint, SIGKILL)`（C 忽略返回值语义保留）；minix-sys `sys_kill` wrapper 落地 = E6 sys_kill 切片闭环；真实通电仍挂 E1） | 11-signal-core.md | ~~edge E6（SYS_KILL wrapper）+ E1（trap）~~ wrapper 达成（通电挂 E1） |
| D-14 | ~~`exit.rs:101`~~ | ~~退出进程自身 times 计账为 0~~ **✅ 已修复**（2026-09-07，Fix #26：`KernelGateway::proc_times`（minix-sys `sys_times` wrapper，SYS_TIMES=25）→ exit_proc step 4 累加进 child 桶；失败 panic 对齐 `forkexit.c:308-309`） | 10-pm-wait.md | ~~edge E6（SYS_TIMES wrapper）~~ wrapper 达成（真实通电挂 E1） |
| D-15 | ~~`exit.rs:116`~~ | ~~`vm_willexit` 假装 Ok~~ **✅ 已修复**（2026-09-06，Fix #11：真实 `sendrec(VM, VM_WILLEXIT)` + 失败 panic 对齐 `forkexit.c:332-334`） | 02-stage-vm | ~~VM 协同面~~ wire 达成（真实往返挂 E5(a)/E1） |
| D-16 | `exit.rs:142` | core dump 路径名指针为 0——**依赖未解除的显式 DEFERRED**：C 传 `mp_name` 指针（m7p1，VFS 异步 safecopy PM 内存，`forkexit.c:356`），Rust (a) 无法对表内数据形成跨异步稳定指针、(b) `VfsCall::DumpCore.path` 为 i32 容不下 64 位指针——需与 05-stage-vfs 协同重设计契约（按值 [u8;16] 或 minix-types 增 path+len 成员） | 09-pm-exit.md | 契约决策 + minix-types wire 成员（edge E7） |
| D-17 | `exit.rs:192` | `sched_stop` 假装 Ok——**依赖未解除**：C `exit_restart` 的 `sched_stop` 走 SCHED 服务的 SCHEDULING_STOP 消息（`schedule.c` 客户端，A-8），SCHED 服务器（16-stage）尚未存在，无对端可通话 | 16-scheduling.md | A-8（SCHED 客户端 + 服务器落地） |
| D-18 | ~~`exit.rs:215`~~ | ~~`sys_clear`（内核侧进程回收）no-op~~ **✅ 已修复**（2026-09-07，Fix #24：`KernelGateway::sys_clear` + minix-sys wrapper（E6 切片）+ PmServer 持有网关下穿 exit_proc/exit_restart 两调用点；失败 panic 对齐 `forkexit.c:367-368/450-451`） | 01-stage-kernel | ~~edge E6 + E1~~ wrapper 达成（真实通电挂 E1） |
| D-19 | ~~`exit.rs:219`~~ | ~~`vm_exit`（页表回收）no-op~~ **✅ 已修复**（2026-09-06，Fix #12：真实 `sendrec(VM, VM_EXIT)` + 失败 panic 对齐 `forkexit.c:455-457`） | 02-stage-vm/22 | ~~VM 协同面~~ wire 达成（真实往返挂 E5(a)/E1） |
| D-20 | ~~`wait.rs:101`~~ | ~~trace-stop 返回码用模拟值~~ **✅ 已修复**（2026-09-06，Fix #6：真实 sigtrace 扫描 + sigdelset 消费 + 空集落环，forkexit.c:519-531 全语义） | 18-trace.md | ~~ptrace 停止状态建模~~ 已达成（trace_mask/trace.stopped 建模 D-10 时已备） |
| D-21 | ~~`wait.rs:124`~~ | ~~rusage 跨地址空间拷贝假装成功~~ **✅ 已修复**（2026-09-07，Fix #27：`tell_parent` 构造 144 字节 rusage（utime/stime timeval 对，按 `table.system_hz` 换算），经 `KernelGateway::copy_to_user` → minix-sys `sys_vircopy` 真实投递父进程（`exit.rs:518-574`）；datacopy 失败 → reply(parent, errno) + 子保持 ZOMBIE 可重试，对齐 `forkexit.c:692-704`；真实通电挂 E1。遗留：`wait.rs:128-131` 的 `[DEFERRED: D-21]` 注释与 `let _ = rusage_addr;` 死绑定未随实现清理，`exit.rs:515-516` doc 注释仍写 "(omitted)"——登记为 V2 卫生项） | 10-pm-wait.md | ~~edge E6（SAFECOPY）~~ 已达成（`sys_vircopy` wrapper，SYS_VIRCOPY_CALL=15；真实通电挂 E1） |
| D-22 | ~~`signal.rs:160`~~ | ~~`is_stacktrace` 返回硬编码 false~~ **✅ 已修复**（2026-09-06，Fix #13：`is_lethal`/`is_stacktrace`/`is_termination` 按真 C 谓词宏重写——原计划臆断为位掩码，实为 `sys/signal.h:279-286` 的谓词宏；连带修复旧 lethal 近似列表错含 SIGKILL/TERM/TRAP 的真实语义偏差，EPERM 保护测试改用 SIGSEGV） | ~~16-scheduling.md~~ 11-signal-core.md（语义实属信号系统，原归属登记有误） | ~~A-8~~ 无依赖，纯谓词 |
| D-23 | ~~`signal.rs:360-362`~~ | ~~SIGVTALRM/`check_vtimer` stub~~ **✅ 已修复**（2026-09-06，Fix #14：process_ksig 接真实 check_vtimer（VTimerCtl 显式接缝）；连带修复 `signo == 12` 应为 26 的真 bug——旧分支从未命中） | 14-itimer.md | ~~虚拟计时器 trait 落地~~ PM 侧达成（VTimerCtl 生产实现 = 内核 sys_vtimer 挂 E6） |
| D-24 | ~~`mproc/fork.rs:222-228`~~ | ~~`getticks()` 返回 0~~ **✅ 已修复**（2026-09-06，Fix #15：getticks 桩删除；`fork_from`/`srv_fork_from` 显式注入 `started: Clock`，`fork_child_from_parent` 经 ClockSource 计算；活路径零值收敛为带 E6 注释的显式 seam） | 14-itimer.md / 内核 sys_times | ~~内核 uptime 面~~ 结构达成（真实 uptime 挂 E6） |
| D-25 | ~~`event.rs` `PmEventServices::resume`~~ | ~~暂返回 OK~~ **✅ 已修复**（2026-09-07，Fix #28：`PmEventServices::resume` 经 `KernelGateway::sys_resume` → minix-sys `sys_runctl(ep, RC_RESUME, 0)` 真实恢复；kernel `dispatch_runctl` RC_RESUME 分支对端真实） | 13-signal-flow.md / edge E6 | ~~minix-sys SYS_* 面~~ 达成（真实通电挂 E1） |
| D-26 | ~~`wait.rs` ZOMBIE/TRACE_STOPPED 两环的回复载荷~~ | ~~wait4 回复的状态码载荷建模缺失~~ **✅ 已修复**（2026-09-06，Fix #22：`MessPmLcWait4` + `m_pm_lc_wait4` arm 落地 minix-types；ZOMBIE/TRACE_STOPPED/tell_parent/tell_tracer 四处 wire 按"m_type=pid + 载荷=status"发出；E7 首切片） | 10-pm-wait.md / edge E7 | ~~minix-types 增成员~~ 已达成 |
| D-27 | ~~`exit.rs:302-307`~~ / ~~`exit.rs:674-676`~~ | ~~SIGHUP 会话组广播 no-op~~ **✅ 已修复**（2026-09-08，Fix #32：exit_proc 第 13 步经 `check_sig(-procgrp, SIGHUP)` 真实广播，caller 为死亡首领本人；disinherit 尾注释改为指针；真实通电挂 E1） | 09-pm-exit.md | ~~check_sig 复用~~ 已达成（stage 内，无外部依赖） |
| D-28 | ~~`exit.rs:448-451`~~ | ~~check_parent 的 SIGCHLD 分支 no-op~~ **✅ 已修复**（2026-09-08，Fix #34：`sig_proc(parent, SIGCHLD, trace=TRUE, ksig=FALSE)`，默认处置由 ign_sset 忽略，handler 交付链随批次 B 的 sig_send 落地） | 09-pm-exit.md | 无（stage 内） |

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

> 2026-09-06 轮的推进顺序已执行完毕（逐条证据见 §10）；V2 轮的推进顺序见 §11.6。

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

### 9.4 V2 轮（§11）跨阶段抽取补充（2026-09-08）

本轮 V2 条目经三类判定核对后，**没有新增独立 E 条目**——所有跨界依赖都落在既有 E1/E5/E6/E7 的范围内，只有两处进度/清单增补：

| V2 条目/批次 | stage 内部分 | 跨阶段部分（edge_todo.md） |
|---|---|---|
| V2-P0-1/P0-2（信号终止与集合位序） | 全部 stage 内（transport 贯通、常量层修正） | 通电仍挂 E1（无新增登记） |
| V2-P1-2（SIGHUP 广播，新 D-27） | stage 内（复用 check_sig + 既有 sys_kill wrapper） | 无 |
| V2-P2-1（itimer，批次 D） | 重挂收敛 + notify 接线 | `sys_setalarm`/`sys_vtimer` wrapper → E6（2026-09-08 进度块已更新清单）；通电 → E1 |
| 接线批次 A/B/C/G（§11.1.1） | match 臂 + handler 接线 + 集成测试 | wire 类型 → E7（含 rs_start 先例）；`sys_datacopy`/`sys_sigreturn` 等 wrapper → E6 |
| 批次 E（exec） | V2-P2-2 caller 门 + 臂 | `sys_exec` kernel 对端（既有 D-08 注）；D-16 core-name 契约 → E7 |
| V2-P3-2（GID_MAX） | 判定与实现 | gid 载荷宽度决策随 E7 A 批次 |

微内核阶段独立性的结论（对应用户口径）：PM 的 stage 内生产代码不依赖任何其它 stage 即可完整实现并测试——本轮 40 臂批次表中每一项的前置条件要么是 PM 内部工作，要么是共享契约层（minix-types/minix-sys，E6/E7）或对方 stage 生产代码（SCHED 服务器属 06-stage）或联调（E5/E1），与 edge_todo.md 头部的三类判定完全吻合，无第④类。

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

### ✅ Fix #13: D-22 — `is_stacktrace`/`is_termination` 按真 C 谓词重写（2026-09-06）

**File(s)**：
- `os/servers/pm/src/signal.rs`（三个谓词函数重写 + 补 7 个 lethal 族本地常量 + 1 谓词精确集测试 + 1 个既有测试的语义修正）

**Before/After**：C ground truth 是**谓词宏**（`sys/signal.h:279-286`）而非位掩码（原 D-22 记录有臆断）：`SIGS_IS_LETHAL = ILL|BUS|FPE|SEGV|EMT|ABRT`、`SIGS_IS_STACKTRACE = LETHAL && !=ABRT`、`SIGS_IS_TERMINATION = LETHAL || KILL || PIPE`。旧 Rust 实现三处错：`is_stacktrace` 硬编码 false（PRIV_PROC 的 stacktrace 分支死代码）；`is_lethal` 近似列表错含 SIGKILL/TERM/TRAP；`is_termination` 反向近似列表与 C 集合不符。连带发现：`test_kill_eperm_for_lethal_priv` 用 SIGKILL 编码了旧错误行为（C 中 SIGKILL 经 kill(2) 对 PRIV_PROC 合法）——按 Ground Truth 链改用 SIGSEGV 并留修正注释。归属修正：D-22 实属 11-signal-core.md（信号系统语义），原登记 16-scheduling 有误。

**Verified**：
- `cargo test -p minix-pm`：336 → **339 lib passed**（+1 谓词精确集测试；1 测试按 C 语义修正）+ 6 integration
- `grep -n "false // stub" os/servers/pm/src/signal.rs`：零命中

**Docs**：本文件 §6 D-22 行（含归属修正）；`signal.rs` 谓词 doc 注释带 C 锚点。

**未做（DEFERRED 论证）**：无——纯函数重写，`sys_diagctl_stacktrace` 的内核调用本体归 D-09 家族（E6）。

### ✅ Fix #14: D-23 — `process_ksig` 接真实 `check_vtimer`（2026-09-06）

**File(s)**：
- `os/servers/pm/src/signal.rs`（process_ksig 增加 `vctl: &mut dyn VTimerCtl` 显式接缝；stub 分支接真实 `check_vtimer`；**修复 `signo == 12` 应为 26 的真 bug**——SIGVTALRM=26（timer.rs:26），旧分支用 12（SIGSYS）从未命中过；+2 测试）
- `os/servers/pm/src/signal.rs` 借用重排：pid 提取提前于可变借用调用

**Before/After**：C `signal.c:326-328`：process_ksig 的 switch 对 SIGVTALRM/SIGPROF 先 `check_vtimer(proc_nr, signo)` 再 fall-through 单播。Rust 侧旧 stub 的条件表达式本身写错了信号号——修复后 26/27 正确路由到 `check_vtimer`（interval>0 时经 VTimerCtl 重设内核虚拟计时器，`alarm.c:222-241`）。设计说明：VTimerCtl 作为显式函数参数（而非内部构造）——生产装配者必须显式提供内核 sys_vtimer 适配器（E6），不存在被遗忘的静默 no-op；SIGSYS 不触碰计时器以回归测试锁定。

**Verified**：
- `cargo test -p minix-pm`：339 → **341 lib passed**（+2：SIGVTALRM 触发 Virtual 重启且 set=50 / SIGSYS 不触碰计时器）+ 6 integration
- `grep -n "signo == 12" os/servers/pm/src/signal.rs`：零命中

**Docs**：`14-itimer.md` check_vtimer 现状行；本文件 §6 D-23 行。

**未做（DEFERRED 论证）**：`VTimerCtl` 生产实现（内核 `sys_vtimer`，`alarm.c:239` 未检查返回值）挂 edge E6——PM 侧逻辑与接缝完备。

### ✅ Fix #15: D-24 — `getticks` 桩删除，`started` 显式注入（2026-09-06）

**File(s)**：
- `os/servers/pm/src/mproc/fork.rs`（删除 `fn getticks() -> Clock { 0 }`；`fork_from`/`srv_fork_from` 增加 `started: Clock` 参数；`PmContext::fork_child_from_parent` 增加 `clock: &dyn ClockSource` 参数并计算 uptime；测试更新 + FixedClock 断言 started=100_000）
- `os/servers/pm/src/fork.rs`（协调器 `do_srv_fork` 的 `srv_fork_from` 调用点显式传 0 + E6 注释）

**Before/After**：设计选型——(a) 保留 `getticks()` 桩但改读 ClockSource 全局：隐藏的假零依旧；(b) **构造器显式注入 `started: Clock`**（已选）："数据在诞生处注入"——构造器不再自己找时间，调用方对其世界的时钟负责；`PmContext` 层（有合法测试时钟）走真 ClockSource 计算，活协调器路径的零值收敛为带 `[E6]` 注释的显式 seam（可 grep、可追踪），不再是函数内部的说谎返回值。C 锚点 `forkexit.c:114` `rmc->mp_started = getticks()`。

**Verified**：
- `cargo test -p minix-pm`：341 → **341 lib passed**（含新断言 started=100_000）+ 6 integration
- `grep -rn "fn getticks" os/servers/pm/src/`：零命中

**Docs**：`07-pm-fork.md` §D5/§4.2/§4.5 三处 started 表述；本文件 §6 D-24 行。

**未做（DEFERRED 论证）**：真实内核 uptime（`sys_times`/getuptime 三值）挂 E6——PmContext 层的 ClockSource 接缝已就绪，生产实现落地即接管。

### ✅ Fix #16: D-16 论证升级 + 全部余下 DEFERRED 行的自包含化（2026-09-06，纯文档轮）

**File(s)**：
- `os/servers/pm/src/exit.rs`（D-16 代码注释升级为 `[DEFERRED: D-16]` 显式契约：C 指针语义 + 两层阻塞论证）
- 本文件 §6：D-13/D-14/D-16/D-17/D-18/D-21 五行增补"**依赖未解除**"自包含论证（todo-fix 硬约束：DEFERRED 必须写明依赖为何未解除，不许静默降级）

**要点**：D-16（core name 指针）经重新核实定为**契约级缺口**而非可单独修复项——C 的 `VFS_PM_PATH = mp_name`（m7p1）是指向 PM 静态 mproc 表的指针、由 VFS 异步 safecopy 读取；Rust (a) 不能对表内数据形成跨异步稳定指针，(b) minix-types 的 `VfsCall::DumpCore.path: i32` 容不下 64 位指针。解除条件 = 与 05-stage-vfs 协同的契约决策 + E7 wire 成员。其余各行补齐对端现状（哪些 kernel 已实现/未实现）与 edge 条目引用，使每条 DEFERRED 的存在性与解除条件都可独立审计。

**Verified**：`cargo test -p minix-pm`：**341 lib + 6 integration passed**（纯注释/文档轮，零代码语义变化）。

**未做（DEFERRED 论证）**：即本轮登记的全部内容——每条的解除条件与 edge 归属见 §6/§9。

### ✅ Fix #17: P2-1 — cfg 特性声明恢复门控代码可编译性（2026-09-06）

**File(s)**：
- `os/servers/pm/Cargo.toml`（`[features] syscall_stats = [] sprofile = []`，带 C 宏对齐与验证命令的文档注释）
- `20-misc-queries.md` §D7/边界行、`plan.md` §5.4 状态刷新

**Before/After**：`misc.rs` 的 `#[cfg(feature = …)]` 门此前引用了不存在的特性——门控代码在任何构建下都被排除（静默死代码），且 `cargo check --features syscall_stats` 直接报错，正确性无法验证。声明后：默认关（与 C 的 `ENABLE_SYSCALL_STATS`/`SPROFILE` 默认一致），`--features syscall_stats,sprofile` 构建可编译，门控代码恢复"可开启的可选项"语义（plan.md §5.4 的原意）。

**Verified**：
- `cargo check -p minix-pm --features syscall_stats,sprofile`：通过
- `cargo check -p minix-pm`（默认）：通过
- clippy 的 6 条 `unexpected cfg condition value` 告警随之消除

**Docs**：`20-misc-queries.md`、`plan.md` §5.4、本文件 §0/标题。

### ✅ Fix #18: P2-4 — glob re-export 移除、公共 API 显式化（2026-09-06）

**File(s)**：
- `os/servers/pm/src/lib.rs`（`pub use ipc::*; pub use mproc::*;` → 仅 `pub use ipc::TestIpcTransport;`）

**Before/After**：双 glob 把 ipc/ 与 mproc/ 两棵树压平到 crate 根，顶层与 mproc 下 5 对同名双层模块（fork/signal/wait/credentials/trace 的 logic 层与 state 层）在根上只暴露一份符号。移除后统一走完整模块路径（`pm::ipc::*` / `pm::mproc::*` / `pm::init::*`），仅保留测试接缝 `TestIpcTransport` 的显式 re-export（内部 14 处 `crate::TestIpcTransport` 的既有惯例 + 外部集成测试）。探索阶段已确认外部唯一消费者（os/tests）走模块路径，零破坏。

**Verified**：
- `cargo test -p minix-pm`：**341 lib + 6 integration passed**（含外部 tests/ 目录——crate 外视角无路径断裂）
- `grep -rn "pub use .*\*" os/servers/pm/src/lib.rs`：零命中

**Docs**：lib.rs 模块注释（压平问题与决策记录）；本文件 §0/标题。

### ✅ Fix #19: P2-5 — stub 注释统一为 `[DEFERRED: D-XX]` 显式契约（2026-09-06）

**File(s)**：
- `os/servers/pm/src/exit.rs`（8 处注释升级）、`os/servers/pm/src/wait.rs`（1 处）

**Before/After**：campaign 中 D-15/D-19/D-20 等已实现后，剩余 stub 的存在形式从"散落的 `stubbed as Ok`/`simulate` 注释"统一为 `[DEFERRED: D-XX] <语义> —— <依赖未解除论证> <edge 引用>` 格式（与 `ipc/vfs.rs` 的 unimplemented 惯例、`§6` 登记表、`§9.2` 对照表三处一致）。任何一处 stub 的存在性、归属、解除条件现在都能被 `grep -rn "DEFERRED" os/servers/pm/src/` 一条命令审计（模式 60 诚实显式 TODO）。

**Verified**：
- `cargo test -p minix-pm`：**341 lib passed**（纯注释轮，零语义变化）
- `grep -rn "stubbed\|stub " os/servers/pm/src/{exit,wait}.rs | grep -v DEFERRED`：零命中

**Docs**：本文件 §0/标题。

### ✅ Fix #20: P2-2 + P2-7 — plan.md ARCH 表刷新 + ESCRIPT 排除登记（2026-09-06，纯文档轮）

**File(s)**：
- `plan.md`：§4 ARCH 表 A-5（轮 2 已改）→ 本轮补 A-9（"缺口"→"已实现"+ 复核说明）、A-12（"部分实现"→"已实现"，D-10 的 TO_TRACEFORK 继承补齐后）；§5.4 排除表新增 `ESCRIPT` 行（C 死代码，`exec.c:31` 定义后零使用，模式 78 显式标注）

**Verified**：
- ARCH 表逐行 grep 复核：A-1~A-3/A-6/A-11 与代码一致（无变化）；A-4（部分实现，E7）/A-7/A-8/A-10/A-13（未实现）维持——各自有真实的未落地依赖
- `cargo test -p minix-pm`：**341 lib passed**（纯文档轮）

**未做（DEFERRED 论证）**：A-4 的 wire 系统化（E7）、A-7/A-8 的服务端依赖、A-10 的内核 DELAY_CALL、A-13 的设计层决策——各自有登记的依赖，非遗漏。

### ✅ Fix #21: P3-1/P3-2 — clippy 收敛与卫生清理（2026-09-06）

**File(s)**：
- `os/servers/pm/src/`（15+ 文件的机械卫生：doc 列表缩进 17 处、`Message` 字面量初始化替代 default+赋值 5 处、移除 3 处冗余 unsafe、删 `drop(proc)` 无效调用）
- `os/servers/pm/src/{timer,trace}.rs`（**3 处 clippy correctness 级 error 修复**——基线统计时被告警数字掩盖）：
  - `timer.rs:148` `v <= i64::MAX` 恒真比较 → `saturating_add`
  - `trace.rs:176/210` `req.data < 0` 对 u64 恒假 → 仅保留 `>= 64` 上界（C 的 int 语义在 Rust 无符号建模下不可达，注释说明）
- `os/servers/pm/src/lib.rs`（P2-4 联动：显式 re-export 后的导入收敛）

**Before/After**：clippy lib：约 50 告警 + 3 error → **0 告警 0 error**。过程教训：`cargo clippy --fix --lib` 会把"仅测试使用"的导入当无用删除（不分析 cfg(test)）——首次尝试破坏测试编译，已整体回退改为手修 + 逐文件导入下移到测试模块。

**Verified**：
- `cargo test -p minix-pm`：**341 lib + 6 integration passed**
- `cargo clippy -p minix-pm --lib`：0 warning / 0 error
- 依赖 crate 残留（不在 pm 范围）：minix-sys 4 条（collapsible-if 3 + MountTable Default 1）、minix-types 1 条（large_enum_variant，02-stage-vm 已判定 WONTFIX）

**Docs**：本文件 §0/标题标注。

### ✅ Fix #22: D-26 — wait4 回复载荷的 wire 最后一跳（2026-09-06）

**File(s)**：
- `os/libs/minix-types/src/ipc/message.rs`（新增 `MessPmLcWait4 { status: i32, _padding: [u8; 52] }` + `m_pm_lc_wait4` union arm + 布局断言测试——**E7 首切片**，纯新增零破坏）
- `os/servers/pm/src/wait.rs`（TRACE_STOPPED 环 / ZOMBIE 环按载荷契约重写；ZOMBIE 环内联副本收敛到 `tell_parent`，消除载荷 bug 的重复源头）
- `os/servers/pm/src/exit.rs`（`tell_parent`/`tell_tracer`/`check_parent`/`zombify`/`tracer_died`/`disinherit` 沿调用链穿 transport，wire 在 C 的原位发出；`w_exitcode` 提为 `pub(crate)` 复用）
- `tests/run_once_integration.rs`（+1 端到端：tag/载荷分解断言）

**Before/After**：修复前 wait status 只进了 `ipc.reply` 的 m_type 占位，wire 消息的载荷全零——libc 的 wait4 按布局读取时永远拿到 0。设计选型：(a) 按 C 精确补 typed 载荷（已选——libc wait4 包装的读取契约）；(b) 复用 `m_m1.m1i1` 当状态槽（伪造 wire 契约，translate 陷阱）；(c) 一次做完 E7 全量（违反一轮一条）。**结构收益**：ZOMBIE 环曾内联复制 tell_parent 的七步逻辑（载荷 bug 正是在这份副本里）——收敛后单一事实源。Linux 对照：内核 wait4 由 `copy_to_user` 写类型化 status/rusage；Redox scheme 回复同为 typed payload——"tag(m_type) + typed body(载荷)"是回复消息的通用契约，m_type 兼载 body 是对契约的破坏。

**Verified**：
- `cargo test -p minix-pm`：341 → **342 lib passed**（D-20 断言迁移到载荷；+1 tell_parent 异步 tag/载荷断言）+ **7 integration**（+1 wait4 僵尸回收：tag=pid、载荷=W_EXITCODE、ToldParent）
- `cargo test -p minix-types --lib`：169 → **170 passed**（+wait4 布局断言）
- `grep -n "m_type: w_status\|m_type: status" os/servers/pm/src/wait.rs`：零命中（m_type 兼载状态的历史清除）

**Docs**：`10-pm-wait.md` §4.1（wire 契约节 + 测试表）；`edge_todo.md` E7 进度注（首切片）；本文件 §6 D-26 行。

**未做（DEFERRED 论证）**：E7 的其余 wire 族（47 调用系统化）与 D-21 的 rusage 载荷（依赖 SYS_TIMES，E6）——各自保留登记。

### ✅ Fix #23: D-13 — `do_exit` 的 PRIV_PROC 违规分支真实发送 `sys_kill`（2026-09-06）

**File(s)**：
- `os/libs/minix-sys/src/syscall.rs`（**E6 首切片**：`sys_kill` wrapper + `SYS_KILL_CALL` 常量 + `CannedKernelCallTransport.sent` 逐调用消息记录 + wire 断言测试 ×2）
- `os/servers/pm/src/exit.rs`（新增 `KernelGateway` trait + `TrapKernelGateway` 生产实现（镜像 VM 侧 `kernel_gateway.rs` 先例）；`do_exit` 增 `kern: &mut dyn KernelGateway` 参数，PRIV_PROC 分支从 no-op 改真实发送；+1 用户进程负向断言测试）
- `os/servers/pm/src/ipc/calls.rs`（PM_EXIT 分发臂构造生产网关）

**Before/After**：C `do_exit`（forkexit.c:245-262）：PRIV_PROC 调 exit(2) 是违规——printf 警告 + `sys_kill(endpoint, SIGKILL)` 后直接 SUSPEND，**不走** `exit_proc`（"System processes do not use PM's exit()"），真正的终止由内核信号回环（process_ksig，11）完成；`sys_kill` 返回值 C 不予检查。Rust 修复前该分支是 no-op（违规进程永远存活且无任何处置痕迹）。设计选型：(a) 网关 trait + 生产/测试双实现（已选，镜像 VM `KernelGateway` 先例）；(b) 直调 minix-sys 无接缝（不可测，否决）；(c) 复用 `IpcTransport`（kernel call 走向量 32 与 IPC 向量 33 是不同通道，模型错误，否决）。pre-E1 行为：trap 桩回 `-EIO`，`Result` 保留错误可观测性、`do_exit` 按 C 忽略之——不伪造任何状态。

**Verified**：
- `cargo test -p minix-pm`：342 → **343 lib passed**（`test_do_exit_priv_proc` 增 sys_kill 捕获断言 + 进程保持 Running；+1 用户进程不触 sys_kill 的负向断言）+ **7 integration**
- `cargo test -p minix-sys`：113 → **115 passed**（+sys_kill wire 编码 / 负 errno 透传）
- `cargo clippy -p minix-pm --lib`：0 warning 0 error 维持

**Docs**：`09-pm-exit.md` §2.1 落地段新增；`edge_todo.md` E6 进度注（sys_kill 切片闭环）；本文件 §6 D-13 行。

**未做（DEFERRED 论证）**：真实通电（trap 层，E1）——wire、包装、网关、语义均已在 stage 内闭环；同轮顺手修正一处过时断言（ZOMBIE vs Exiting，见本轮 diff 的 `test_do_exit_user_process_skips_sys_kill`）。

### ✅ Fix #24: D-18 — `sys_clear` 两调用点接真实内核通道（2026-09-07）

**File(s)**：
- `os/libs/minix-sys/src/syscall.rs`（**E6 切片**：`sys_clear` wrapper + `SYS_CLEAR_CALL = 2` 常量 + m1i1 载荷 wire 测试）
- `os/servers/pm/src/exit.rs`（`KernelGateway` 增 `sys_clear`；`TrapKernelGateway` 实现委托 minix-sys；`exit_proc`/`exit_restart` 增 `kern` 参数并在 step 9 / step 4 调用，失败 panic 对齐 C）
- `os/servers/pm/src/init.rs`（`PmServer` 持有 `Box<dyn KernelGateway>`，`with_transport` 默认装配生产网关，`with_kernel_gateway` 供测试注入；run_once 经 `self.kern.as_mut()` 下穿 VFS 臂/事件臂/分发臂）
- `os/servers/pm/src/ipc/{calls,dispatcher}.rs`、`os/servers/pm/src/event.rs`（kern 参数沿分发链与 EventRegistry 穿线）
- `tests/run_once_integration.rs`（exit 场景注入 mock-ok 网关）

**Before/After**：设计选型——(a) PmServer 持有 `Box<dyn KernelGateway>` 并沿调用链下穿（已选）：内核出口与 IPC transport 是两类通道（向量 32/33），网关作为与 transport 对等的能力对象由服务器持有，分发臂与 handler 显式传递（A-3 显式参数风格）；(b) 全局 thread_local 网关：隐式全局违反 A-3，否决；(c) 每个 handler 内联构造 Trap 网关：无状态可行但测试无法注入脚本化应答，否决。**C 语义对照**：exit_proc step 9（`forkexit.c:366-368`，PRIV_PROC 直毁——VFS 可能阻塞在该块设备驱动上，等待即死锁）与 exit_restart step 4（`forkexit.c:449-451`，VFS 回复后回收用户进程）失败均 panic；Rust 逐字对齐 `panic!("… sys_clear failed: {}", r)`。

**Verified**：
- `cargo test -p minix-pm`：**343 lib + 7 integration passed**（exit 集成场景经 `with_kernel_gateway` 注入 mock-ok 网关验证 step 4 直毁不 panic；既有全部 exit/kill/事件测试无回归）
- `cargo test -p minix-sys`：116 → **118 passed**（+sys_clear wire/负 errno ×1）
- `cargo clippy -p minix-pm --lib`：0 warning 0 error 维持

**Docs**：`09-pm-exit.md` §1.3 后新增"内核出口落地"段；`edge_todo.md` E6 进度注（sys_clear 切片）；本文件 §6 D-18 行。

**未做（DEFERRED 论证）**：真实通电（trap 层，E1）——wire、包装、网关、两调用点语义均已在 stage 内闭环；`exit.rs` 的 D-13（sys_kill）与 D-18（sys_clear）现已共用同一网关通道。

### ✅ Fix #25: D-09 — `sys_abort` 端口接真实内核通道（2026-09-07）

**File(s)**：
- `os/libs/minix-sys/src/syscall.rs`（**E6 切片**：`sys_abort` wrapper + `SYS_ABORT_CALL = 27` 常量，载荷 m1i1 = how（RB_* 位组）；+2 wire/负 errno 测试）
- `os/servers/pm/src/exit.rs`（`KernelGateway` 增 `sys_abort`；`TrapKernelGateway` 实现委托 minix-sys）
- `os/servers/pm/src/ipc/vfs.rs`（`PmServices::sys_abort` 从 `unimplemented!()` 改为经网关真实发送；+1 端口级测试断言 abort_flag 到达网关；4 个网关 mock 补 `sys_abort`）
- `05-vfs-interaction.md`（D4 端口落地状态刷新）

**Before/After**：C `main.c:304-312`：REBOOT 回复特例发 `sys_abort(abort_flag)` 后返回主循环等待 HARD_STOP 通知，**返回值 C 不予检查**——abort 成功时机器直接停机；失败（pre-E1 `-EIO`）PM 继续循环，不伪造停机状态。Rust 修复前该端口是 `unimplemented!()`：任何 reboot 流程测试都无法走通。设计说明：`abort_flag` 是 PmServices 自有字段（最初 reboot 请求的 how 位组，`do_reboot` 写入），端口内直接读取 self.abort_flag 传递——不新增参数（trait 签名 `sys_abort(&mut self)` 不变，三个既有测试实现零改动之外仅补方法体）。

**Verified**：
- `cargo test -p minix-pm`：**343 lib + 7 integration passed**（+1 端口级测试：abort_flag=0x808 到达网关；vfs 既有 reboot 状态机测试（RecordingServices 录 SysAbort）无回归）
- `cargo test -p minix-sys`：118 → **120 passed**（+2 sys_abort wire 测试）
- `cargo clippy -p minix-pm --lib`：0 warning 0 error 维持

**Docs**：`05-vfs-interaction.md` D4 端口落地状态全面刷新（06/09/13 已接线、sys_abort 落地、余 sched_start_user/sys_exec 占位）；`edge_todo.md` E6 进度注；本文件 §6 D-09 行。

**未做（DEFERRED 论证）**：真实通电（trap 层，E1）——wire、包装、网关、端口语义均已在 stage 内闭环。

### ✅ Fix #26: D-14 — exit 时 sys_times 计账接真实内核通道（2026-09-07）

**File(s)**：
- `os/libs/minix-sys/src/syscall.rs`（**E6 切片**：`sys_times` wrapper，请求 `m_lsys_krn_sys_times.endpt` / 回复解码 `m_krn_lsys_sys_times` 四值；`CannedKernelCallTransport` 增整条载荷脚本 `reply_message`；+2 wire 测试）
- `os/servers/pm/src/exit.rs`（`KernelGateway` 增 `proc_times`；`exit_proc` step 4 累加 user/system ticks 进死亡进程的 child 桶，失败 panic 对齐 `forkexit.c:308-309`；+1 累加断言测试）
- `os/servers/pm/src/signal.rs`（信号终止链 `do_kill`/`do_srv_kill`/`check_sig`/`sig_proc`/`sig_proc_exit`/`process_ksig` 沿调用链穿 `kern`；测试 mock 换 `TestKernel` 脚本化计账）
- `os/servers/pm/src/{event,ipc/vfs}.rs`（PmEventServices/ExecServices 增 kern 字段，适配器传递）
- `tests/run_once_integration.rs`（注入零值 mock 网关）

**Before/After**：设计选型——(a) minix-sys wrapper 返回完整 `MessKrnLsysSysTimes`（已选，C libsys 同型返回四值，后续 14-itimer 的 ClockSource 可复用 real/boot ticks）；(b) wrapper 只返回 (user, sys) 二元（丢信息，否决）；(c) PM 直调 perform_kernel_call 绕过 minix-sys（违反 E6 分层，否决）。**架构要点**：kern 沿信号终止链（do_kill/check_sig/sig_proc/sig_proc_exit）与事件链（PmEventServices/RestartServices）下穿——C 中这些函数直接调 libsys，Rust 以显式参数传递同一能力（A-3）。

**Verified**：
- `cargo test -p minix-pm`：343 → **345 lib passed**（+1 累加断言：脚本 (30,12) → 僵尸桶 utime=30/stime=12）+ **7 integration passed**（注入零值 mock 网关）
- `cargo test -p minix-sys`：**122 passed**（+2 sys_times wire 测试）
- `cargo clippy -p minix-pm --lib`：0 warning 0 error 维持

**Docs**：`09-pm-exit.md` 计账落地段；`edge_todo.md` E6 进度注；本文件 §6 D-14 行。

**未做（DEFERRED 论证）**：真实通电（trap 层，E1）——wrapper、网关方法、累加语义、测试均已闭环；D-21（rusage 的 sys_datacopy 投递）另需 SAFECOPY wrapper，保持登记。

### ✅ Fix #27: D-21 — rusage 经 VIRCOPY 真实投递父进程（2026-09-07 实施，2026-09-08 补记）

> 本条目对应提交 de9415604（轮 27）+ d79307ee6（轮 27-28 收口），当时漏写本记录，2026-09-08 V2 轮账目对账时依提交信息与代码现状补记。

**File(s)**：
- `os/libs/minix-sys/src/syscall.rs`（**E6 切片**：`sys_vircopy` wrapper + `SYS_VIRCOPY_CALL = 15` + `SELF` 哨兵导出；`CannedKernelCallTransport` 增整条载荷脚本 `reply_message`；+2 wire 测试）
- `os/servers/pm/src/exit.rs`（`KernelGateway` 增 `copy_to_user`（Trap 实现委托 `minix_sys::syscall::sys_vircopy`，`exit.rs:103-110`）；`tell_parent` 真实实现：144 字节 rusage（ru_utime/ru_stime timeval 对，ticks→usec 按 `table.system_hz` 换算，`ProcTable` 增 `system_hz` 字段对齐 C `glo.h` 全局）→ VIRCOPY 投递父进程；datacopy 失败 → reply(parent, errno) + FALSE（子保持 ZOMBIE 可重试），对齐 `forkexit.c:692-704`；+1 布局断言测试 `test_tell_parent_delivers_rusage_via_datacopy`，hz=100 隔离）
- `os/servers/pm/src/mproc/table.rs`（`ProcTable` 增 `system_hz` 字段，new 缺省 60，init_fresh 覆写）

**Before/After**：D-21 原状为 `wait.rs` wait 循环里 `let _ = rusage_addr;` 丢弃地址 + 假装拷贝成功。修复后 rusage 真实写入父进程用户内存，回复时序与 C 一致（先 datacopy 后 reply(parent,pid)）。设计选型：投递通道用 VIRCOPY（`dispatch_vircopy` = Syscall::Vircopy 15）而非 SAFECOPY——与 C libsys `sys_datacopy` 的内核侧实现同型，且 kernel 对端已就绪。

**Verified**：
- `cargo test -p minix-pm`：**346 lib + 7 integration passed**（提交信息自证）
- `cargo test -p minix-sys`：+sys_vircopy wire 测试（提交信息自证 121→122 passed 区间）

**Docs**：`10-pm-wait.md`；`edge_todo.md` E6 进度注；本文件 §6 D-21 行（2026-09-08 划账）。

**未做（DEFERRED 论证）**：真实通电（trap 层，E1）。遗留卫生项：`wait.rs:128-131` stale 注释与死绑定、`exit.rs:515-516` "(omitted)" doc 注释——V2 轮登记为 P3（本补记时未改生产代码）。

### ✅ Fix #28: D-25 — `sys_resume` 经 SYS_RUNCTL 真实现（2026-09-07）

**File(s)**：
- `os/libs/minix-sys/src/syscall.rs`（**E6 切片**：`sys_runctl` wrapper + `SYS_RUNCTL_CALL = 46` + `RC_STOP/RC_RESUME/RC_DELAY` 常量 + `sys_resume` 便捷函数；+1 wire 测试）
- `os/servers/pm/src/exit.rs`（`KernelGateway` 增 `sys_resume`；`TrapKernelGateway` 委托 minix-sys）
- `os/servers/pm/src/event.rs`（`PmEventServices::resume` 从"暂返回 OK 过渡契约"改为经 `self.kern.sys_resume(ep)` 真实发送，raw 内核回复透传）

**Before/After**：round 26 D-14 时发现 kernel 已有 `dispatch_runctl` 的 RC_RESUME 分支（`syscall_process.rs:42` `RC_RESUME = 1`），"内核无停止态可撤销"的过渡契约前提不再成立。修复后 `PmEventServices::resume` 经 `self.kern.sys_resume(ep)` 真实发送 `SYS_RUNCTL(ep, RC_RESUME, 0)`；pre-E1 Trap 回 `-EIO` → `try_resume_proc` panic（fail-closed，signal.c:285 同型）；测试注入 mock 恒 OK。

**Verified**：
- `cargo test -p minix-pm`：**346 lib + 7 integration passed**（D-11 的 `test_reply_signal_event_terminates_via_restart_sigs` 现走真实 `sys_resume` mock 路径）
- `cargo test -p minix-sys`：**121 passed**（+sys_runctl wire 测试）
- `cargo clippy -p minix-pm --lib`：0 warning 0 error 维持

**Docs**：`13-signal-flow.md` D5 段（D-25 落地注）；`edge_todo.md` E6 进度；本文件 §6 D-25 行。

### ✅ Fix #29: V2-P0-2 — 信号集合位基统一到 C `__sigmask` + badignore 谓词修正（2026-09-08）

**File(s)**：
- `os/servers/pm/src/init.rs`（`sig_bit` 改 `1u64 << (sig - 1)` 并提为 `pub(crate)`——全 crate 唯一位基入口；`test_signal_sets_match_c` 断言值换为 C sigset_t 逐位数值；新增 `test_signal_set_membership_matches_c_arrays` 用 C `__sigmask` 原始掩码逐信号对账三个数组）
- `os/servers/pm/src/signal.rs`（`sig_proc` 消费点改调 `init::sig_bit`，本文件不再出现裸移位；badignore 从"集合级交叠"改为 C signal.c:483-486 的单信号成员判定 `ksig && noign(signo) && (ignored(signo) || masked(signo))`；删除 :309-311 的错误翻译死分支；默认忽略门改为与 caught 分支 `else if` 联动——捕获投递失败的默认忽略信号必须终止而非忽略，对齐 C signal.c:535-539 的互斥结构；新增 SIGCONT 存活、badignore 强制终止、非 noign 忽略三测试）
- `notes/.../01-pm-init-main.md`（§3.3 位基描述更正 + 事故记录 + §5 测试表补行）、`11-signal-core.md`（D4 重写 + §5.2 补三个测试名）

**Before/After**：位基分裂使有效 core 集 = {ILL,TRAP,ABRT,EMT,FPE,KILL,SEGV,SYS}（误加 KILL/SYS、丢 QUIT/BUS）、有效默认忽略集 = {CHLD,TTIN,INFO,USR1}（丢 CONT/WINCH）——`kill(pid, SIGCONT)` 在干净进程上落入终止分支。修复后集合数值与 C `sigset_t` 逐位相等，SIGCONT 默认忽略恢复。**设计选型（三案）**：(a) producer 对齐 C（首选：`SigSet` 数值可与 C 直接对照，未来 E7 wire 载荷免换算）；(b) consumer 全改 `1<<sig`（偏离 C，否决）；(c) `SigSetExt::contains_sig` 方法化封装（长期最优但超本条范围，记录为演进）。位基单点化是 Linux `sigismember` 与 Redox `currently_pending_unblocked()` 的共同实践。

**Verified**：
- `cargo test -p minix-pm`：350 lib（346 + 4 新）+ 7 integration passed
- `cargo clippy -p minix-pm --lib`：0 warning 0 error
- 既有测试无一位基依赖需改（旧约定从未被其他测试断言）

**Docs**：`01-pm-init-main.md` §3.3/§5；`11-signal-core.md` D4/§5.2；本文件 V2-P0-2 标 ✅。

**未做（DEFERRED 论证）**：无——本条目 stage 内完整闭环。

### ✅ Fix #30: V2-P0-1 — 信号终止链贯通真实 transport（2026-09-08）

**File(s)**：
- `os/servers/pm/src/signal.rs`（根因修复：`sig_proc` 的 `_transport` 形参改名 `transport` 并下传——此前整条链在此丢弃通道；`sig_proc_exit` 泛型化 `<T: IpcTransport + ?Sized>` 接收调用者真实通道，删除 `TestIpcTransport::default()` 生产构造；新增 `test_signal_termination_tells_vfs`：SIGKILL → VFS_PM_EXIT、SIGSEGV → VFS_PM_DUMPCORE 断言）
- `os/servers/pm/src/ipc/transport.rs`（`TestIpcTransport` 文档加 TSTL 警告：仅供测试注入，生产构造即模式违规；不做 `#[cfg(test)]` 门控的理由——集成测试以普通依赖编译本 crate）
- `os/servers/pm/tests/run_once_integration.rs`（新增 `kill_termination_tells_vfs_exit`：kill 全链经服务器通道断言 VFS_PM_EXIT + VM_WILLEXIT + 回复 0）
- `notes/.../11-signal-core.md`（D6 补 transport 贯通契约 + §5.2 测试名）

**Before/After**：`sig_proc_exit` 曾构造一次性 mock 传给 `exit_proc`，而 exit_proc 尾部无条件 `tell_vfs`（forkexit.c:350-358）——信号终止的 VFS 告知全部进黑洞，core 路径进程永久卡 EXITING。修复后 kill 全链（check_sig→sig_proc→sig_proc_exit→exit_proc→tell_vfs）贯穿同一通道。**设计选型**：(a) transport 沿调用链下传（首选：与 check_sig/sig_proc 既有形态一致，零新抽象）；(b) tell_vfs 从 exit_proc 拆出后置（否决：拆散 C 的步骤顺序与 PRIV_PROC sys_clear 时机）；(c) `#[cfg(test)]` 门控 mock 类型（否决：破坏集成测试可见性，改以文档警告 + 模式登记防御）。**连带发现**（登记不修，见 V2-P2-7/V2-P2-8）：`unpause` 的 VFS_CALL 分支不发 VFS_PM_UNPAUSE、`sig_send` 为空壳——caught 路径（sigaction 接线后可达）的两个缺口。

**Verified**：
- `cargo test -p minix-pm`：351 lib（+1）+ 8 integration（+1）passed
- `cargo clippy -p minix-pm --lib`：0 warning 0 error
- 新集成测试在修复前必失败（mock 吞消息 → VFS 断言空）——TDD 锚点

**Docs**：`11-signal-core.md` D6/§5.2；本文件 V2-P0-1 标 ✅ + V2-P2-7/V2-P2-8 登记。

**未做（DEFERRED 论证）**：真实通电（trap 层）挂 edge E1——本条目的通道贯通与 wire 语义在 mock 层已完整验证。

### ✅ Fix #31: V2-P1-1 — PID 轮转相位对齐 C（2026-09-08）

**File(s)**：
- `os/servers/pm/src/mproc/pid_gen.rs`（`get_free_pid` 改为"先自增 next_pid、检查并返回新值"，对齐 utility.c:38 的 C 语义；5 个测试同步新相位：首分配 INIT_PID+2、回绕 30000→2→3、冲突/procgrp/stale 三测试的候选锚点平移）
- `os/servers/pm/src/init.rs`（`test_fill_boot_system_procs` 的 PID 断言 PM→3/VFS→4/RS→5）
- `notes/.../07-pm-fork.md`（§2.5 补相位语义说明）

**Before/After**：C 的 `next_pid` 先自增（utility.c:38），首个分配 INIT_PID+2=3，pid 2 全生命周期不使用；Rust 返回自增前旧值（首个分配 2），全部 boot 系统进程 PID 漂移一位。修复后启动 PID 序列与 C 逐一对齐。**设计选型**：(a) 返回自增后值（首选：pid 是外部可观察值，Rewrite 契约保护编号序列；Linux `alloc_pid` 的 RESERVED_PIDS=300 启动保留与 C 跳过 pid 2 是同类的"相位保留"实践，Redox 无轮转不可比——V2-Redox-4）；(b) 保留相位改 fill_boot 起点补偿（否决：双真相源，pid_gen 的语义偏离会再次扩散）；(c) 声明有意偏离（否决：无任何收益）。**方法论备注**：pid_gen 的 doc 注释一直写的是 C 的正确语义（"Candidate PID = next_pid++"），是代码没照文档实现——文档-代码同步门的双向性（本次是代码落后于文档）。

**Verified**：
- `cargo test -p minix-pm`：351 lib + 8 integration passed（5 个 pid_gen 测试 + 1 个 fill_boot 测试平移后全绿）
- `cargo clippy -p minix-pm --lib`：0 warning 0 error

**Docs**：`07-pm-fork.md` §2.5；本文件 V2-P1-1 标 ✅。

**未做（DEFERRED 论证）**：无——本条目 stage 内完整闭环。

### ✅ Fix #32: V2-P1-2 — SIGHUP 会话组广播实现（D-27）（2026-09-08）

**File(s)**：
- `os/servers/pm/src/exit.rs`（exit_proc 第 13 步：`procgrp != 0` 时 `check_sig(-procgrp, SIGHUP, ksig=false)`，caller 为死亡首领本人——权限判定与 C 一致，首领自身由 sig_proc 的退出守卫跳过；disinherit 尾部的重复 DEFERRED 注释改为指针注释，doc 注释同步；新增 `test_session_leader_death_broadcasts_sighup`：同组成员终止、异组存活、首领不重复投递）
- `os/servers/pm/src/signal.rs`（`check_sig` 泛型化 `<T: IpcTransport + ?Sized>`——exit_proc 的泛型通道得以贯穿，对 dyn 调用方透明；新增 `SIGHUP = 1` 常量，锚点 sys/sys/signal.h:52）
- `notes/.../todo.md`（§6 新增 D-27 行，D-13 行加范围注记）

**Before/After**：SIGHUP 广播（POSIX 挂断传播核心）在两处站点均为 no-op 且挂在已关闭的 D-13 编号下（台账错位）。修复后复用 check_sig 的负 pid 组扫描（signal.c:601-604 匹配语义），与 C 的调用形态逐点一致（forkexit.c:411-412）。**设计选型**：(a) 复用 check_sig（首选：C 同一子程序，四态选择/权限/守卫全继承）；(b) 手写组扫描循环（否决：重复逻辑，绕过权限与 lethal 保护）；实现点选 exit_proc 第 13 步（C 位置），disinherit 尾部的第二站点改为指针注释避免双重广播。

**Verified**：
- `cargo test -p minix-pm`：352 lib（+1）+ 8 integration passed
- `cargo clippy -p minix-pm --lib`：0 warning 0 error

**Docs**：`09-pm-exit.md`（正文 §2.2/收养段此前已描述 C 契约，本次补测试行）；本文件 V2-P1-2 标 ✅。

**未做（DEFERRED 论证）**：真实通电（trap 层）挂 E1。

### ✅ Fix #33: V2-P2-4 — 广播 SIGTERM 的 RS 优先通知改走 sys_kill 内核回环（2026-09-08）

**File(s)**：
- `os/servers/pm/src/signal.rs`（check_sig 广播分支：`sig_proc(RS 槽)` → `kern.sys_kill(Endpoint::RS, SIGTERM)`，对齐 signal.c:588-589；新增 `test_broadcast_sigterm_notifies_rs_via_kernel`）
- `notes/.../11-signal-core.md`（§2.3 描述更正 + §5.2 测试行）

**Before/After**：Rust 对 RS 槽直接 `sig_proc(ksig=false)`，落入 PRIV_PROC `!ksig` 空分支——RS 实际收不到任何通知；C 走 `sys_kill(RS_PROC_NR)` 内核回环产生真实 ksig。**设计选型**：(a) 内核回环（首选：C 同型，符合"PM 不直接投递系统进程"的特权边界，V2-Redox-5 印证决策在用户态、对系统进程的投递经内核）；(b) 给 PRIV_PROC !ksig 分支实现直接投递（否决：偏离 C 特权模型，且会绕过内核的信号管理）。

**Verified**：`cargo test -p minix-pm`：353 lib（+1）+ 8 integration passed；clippy 0 warning。

**未做（DEFERRED 论证）**：真实通电挂 E1。

### ✅ Fix #34: V2-P2-5 — check_parent 的 SIGCHLD 投递（D-28）（2026-09-08）

**File(s)**：
- `os/servers/pm/src/exit.rs`（check_parent else 分支实现 `sig_proc(parent_slot, SIGCHLD, trace=TRUE, ksig=FALSE)`，对齐 C check_parent 尾部；新增 `test_check_parent_sends_sigchld_when_parent_not_waiting`——父进程 mask 阻塞 SIGCHLD 使 pending 位可观察）
- `notes/.../09-pm-exit.md`（§5.1 测试行）；本文件 §6 D-28 行、V2-P2-5 标 ✅

**Before/After**：C 在父未等待时向其投递 SIGCHLD（装了 handler 的父进程由此得到通知，默认处置下被 ign_sset 忽略）；Rust 是 `let _ = (...)` 无编号 DEFERRED。修复后一行 C 语义落地。**设计选型**：(a) 现在实现（首选：单行调用不依赖批次 B，默认处置路径即可测；handler 装批后的完整交付随 V2-P2-8 联动）；(b) 仅登记随批次 B（劣：no-op 继续存活且不可观察）。

**Verified**：`cargo test -p minix-pm`：354 lib（+1）+ 8 integration passed；clippy 0 warning。

**未做（DEFERRED 论证）**：handler 交付链（sigframe 建立）依赖 V2-P2-8，随批次 B。

### ✅ Fix #35: V2-P2-3 — tell_vfs 错误路径 fail-closed（2026-09-08）

**File(s)**：
- `os/servers/pm/src/ipc/vfs.rs`（`tell_vfs` 双错误改内部 panic：not-idle = PM 状态不变式违规、发送失败 = 传输层损坏，对齐 utility.c:122-129；签名去 `Result`（Err 永不可达的 API 坏味）；删除 `VfsCallError` 枚举；`test_tell_vfs_not_idle_when_blocked` 改 `#[should_panic]`）
- `os/servers/pm/src/exit.rs`（exit 路径 `let _ = tell_vfs(...)` 吞错 → 直接调用，panic 即 fail-fast）
- `os/servers/pm/src/fork.rs`（`map_err(|_| ForkCoordError::VfsError)` 降级 ×2 → 直接调用；删除 `ForkCoordError::VfsError` 变体与 `PmError::InternalError` 映射）
- `notes/.../05-vfs-interaction.md`（§3 Rust 侧描述更正）

**Before/After**：C 的 tell_vfs 两处 panic 都在被调方内部；Rust 曾改返回 Result 且调用方一个吞错（exit）、一个降级为用户可见 errno（fork）——内部损坏被伪装成普通失败。修复后语义与 C 逐点同型：PM 无法安全服务 → panic → RS 重启（Minix3 的可重启性是 panic 敢于 fail-fast 的前提）。**设计选型**：(a) panic 移入 tell_vfs 内部（首选：与 C 同位置，"not idle" 判定就在被调方，调用方无需重复查询；Result 永不 Err 的假 API 消除）；(b) 调用点各自 panic、tell_vfs 保留 Result（劣：每个调用点重复策略，API 谎报可恢复性）；(c) fork 保留降级（否决：把不变式违规伪装成用户错误正是本条要消灭的）。

**Verified**：
- `cargo test -p minix-pm`：354 lib + 8 integration passed（not-idle 测试改 should_panic 后通过）
- `cargo clippy -p minix-pm --lib`：0 warning 0 error

**Docs**：`05-vfs-interaction.md` §3；本文件 V2-P2-3 标 ✅。

**未做（DEFERRED 论证）**：无。

### ✅ Fix #36: V2-P3-3 — Exit 臂透传 do_exit 的回复意图（2026-09-08）

**File(s)**：`os/servers/pm/src/ipc/calls.rs`（Exit 臂 `let _ = do_exit(...)` + 硬编码 NoReply → 直接返回 do_exit 的值）。

**Before/After**：do_exit 两分支恒返 NoReply，丢弃等价；但硬编码会在未来语义变化时静默吞回复。无新测试（既有 `exit_request_never_replies_and_zombifies` 集成测试即本行为的锚点）。

**Verified**：`cargo test -p minix-pm`：354 lib + 8 integration passed；clippy 0 warning。

**未做（DEFERRED 论证）**：无。

### ✅ Fix #37: V2-P3-2 — GID_MAX 校验恢复真实语义（2026-09-08）

**File(s)**：`os/servers/pm/src/credentials.rs`（`GID_MAX` 从 `u32::MAX` 改为 C 真值 `2147483647`（syslimits.h:53）；`test_setgroups_gid_max` 补 [2^31, 2^32-1] 拒绝与边界值两断言；`test_constants_match_c` 数值同步）。

**Before/After**：gid_t 为 32 位无符号（ansi.h:38），C 的 `> GID_MAX`（getset.c:191）拒绝 [2^31, 2^32-1]；Rust 常量取 `u32::MAX` 使检查恒假。修复后同值同语义，`setgroups(gid=2^31)` → EINVAL。原测试注释"无法构造超限值"自证恒假——测试自身正确性维度的漏网之鱼，本轮修正。

**Verified**：`cargo test -p minix-pm`：354 lib + 8 integration passed；clippy 0 warning。

**未做（DEFERRED 论证）**：无。

### ✅ Fix #38: V2-P2-2 — do_exec 补 VFS/RS 调用者门（2026-09-08）

**File(s)**：
- `os/servers/pm/src/exec.rs`（`do_exec` 开头加 `caller ∈ {VFS, RS}` 门 → `ExecError::Perm`，对齐 exec.c:70-71；删除 `let _ = table; let _ = caller;`；`test_do_exec_forwards` 的发起方改为 RS，新增 `test_do_exec_caller_gate`）
- `notes/.../17-exec.md`（§4 do_exec 签名行 + §5 测试行）

**Before/After**：exec 四入口中 do_execrestart/srv_fork/srv_kill/getprocnr 都有调用者门，唯独 do_exec 收了 caller 却丢弃——exec 是 TAINTED/setuid 链的起点，任意进程代他人发起 exec 是权限漏洞。修复后与 C 一致：仅 VFS（用户 execve 载体）与 RS（服务重启）可发起。**设计选型**：(a) 门放 do_exec 开头（首选：与同文件 do_execrestart 同风格，语义与校验同处）；(b) 放 dispatch 臂（劣：校验离语义远，单测覆盖不到）。

**Verified**：`cargo test -p minix-pm`：355 lib（+1）+ 8 integration passed；clippy 0 warning。

**未做（DEFERRED 论证）**：无。

### ✅ Fix #39: V2-P2-1 — itimer 的 CLOCK notify 接线 + 周期重挂收敛（2026-09-08）

**File(s)**：
- `os/servers/pm/src/timer.rs`（`cause_sigalrm` 重写为完整 C 语义：guard 与 alarm.c:326-330 逐条一致，**先重挂后投递**——interval>0 经 `set_alarm`→`TimerCtl.set` 重挂内核 timer（C :334-339 的回调内 set_timer），否则清 ALARM_ON；caller 伪装 PM slot 0 后直连 `check_sig`，删除 `SigSender` 中间 trait；`handle_clock_notify` 退化为"扫到期 → 调 cause_sigalrm"，不再自行改簿记（`let _ = tctl` 消失）；新增生产 `TrapTimerCtl` pre-E6 fail-closed 占位；三个测试重写为真实投递链可观察点，新增一拍未到期不触发断言）
- `os/servers/pm/src/init.rs`（`PmServer` 增 `timer: Box<dyn TimerCtl>` 字段，`with_kernel_gateway` 默认 `TrapTimerCtl`，新增 `with_timer_ctl` 注入构造；run_once 的 notify 分支接 CLOCK → `handle_clock_notify`——init.rs:349 的显式留白消失；`test_run_once_skips_notify` 改非 CLOCK 源，新增 `test_run_once_clock_notify_drives_expire_timers`）
- `notes/.../14-itimer.md`（§3.6/§4.2/§4.4 同步：SigSender 移除、单一重挂点、接线现状）

**Before/After**：三处各做一半的周期重挂逻辑（主循环显式跳过 CLOCK、cause_sigalrm 空壳 interval 分支、handle_clock_notify 绕过 TimerCtl 只改簿记）收敛为单一重挂点——内核 seam 接通后周期 itimer 的"每次到期都重新挂内核 watchdog"语义与 C 一致。**设计选型**：(a) 重挂收敛进 cause_sigalrm（首选：C alarm.c:338 的回调内 set_alarm 同位；SigSender 删除——C 的回调最终调 check_sig，中间层 send_sigalrm 绕过了权限/忽略/阻塞判定）；(b) 保留 SigSender 加生产桥接 impl（否决：多一层间接零收益）。**边界**：`TimerCtl` 生产实现（sys_setalarm wrapper）与 CLOCK 真实通电仍挂 edge E6/E1——本条目的 stage 内逻辑与 mock 端到端已闭环。

**Verified**：
- `cargo test -p minix-pm`：356 lib（+2）+ 8 integration passed
- `cargo clippy -p minix-pm --lib`：0 warning 0 error

**Docs**：`14-itimer.md` §3/§4；本文件 V2-P2-1 标 ✅。

**未做（DEFERRED 论证）**：`TimerCtl` 生产实现挂 edge E6（sys_setalarm/sys_vtimer wrapper）；真实通电挂 E1。

### ✅ Fix #40: V2-P2-6 — ENOSYS 兜底臂的登记义务落到批次表（2026-09-08）

**File(s)**：`os/servers/pm/src/ipc/calls.rs`（兜底臂注释指向 todo.md §11.1 的接线批次表，约定"每接线一批同步划账"）；本文件 V2-P2-6 标 ✅。

**Verified**：`cargo test -p minix-pm`：356 lib + 8 integration passed（纯注释改动）。

**未做（DEFERRED 论证）**：无。

---

## 11. 第 2 轮全量查漏补缺 + 架构审查（V2，2026-09-08）

> 来源：2026-09-08 第二轮架构级审查（cmd-04 原型：查漏补缺 + 分层架构审查）。上一轮（§0-§10，2026-09-06/07）之后，代码经历 Fix #22-#28 与 `sys_vircopy`/`sys_runctl` 等 E6 切片落地，本轮在此新基线上重跑。
> 范围：`os/servers/pm/src/` 全部 Rust 代码（37 文件，约 15,900 行）+ PM 消费的 `minix-types`/`minix-sys` 边界。不做 21 篇概念文档的 doc review（上轮 plan.md §7 已做）。
> 方法：先查漏补缺（Gate A 重生成 SYMBOLS.md、47 调用号矩阵复核、6 个易漏 C 文件逐函数对账、24 处 DEFERRED/stub 全量收敛、Minix3 语义抽查第二批 10 项），再整体到分层架构审查（内核接入 seam → 服务器骨架 → 分发层 → 子系统层 → 状态层 → VFS 协议层 → 测试层），对照 Redox 与 Rust/OS 社区实践。10 项语义抽查中的信号类条目由独立审查代理执行，P0/P1 级结论全部经本会话二次 grep 验证后才登记。
> 条目编号：V2-P0-x / V2-P1-x / V2-P2-x / V2-P3-x，与上轮 P1-x/P2-x/P3-x 不冲突。

### 11.0 Gate 证据

gate-evidence-A（覆盖率枚举，本次重新生成，未复用上轮产物）：

```bash
python3 tools/coverage-extract/coverage-extract.py pm \
  notes/rewrite/fork-syscall-rewrite/04-stage-pm \
  --rust-dir os --c-dir minix3/minix/servers/pm \
  --semantic-map tools/coverage-extract/pm-semantic-map.json \
  --output .review/zcode/fork-syscall-rewrite/04-stage-pm-arch/SYMBOLS.md
# Coverage Summary for pm:
#   Total C symbols: 109
#   Doc covered: 109 (100.0%)
#   Rust covered (name-match): 102 (93.6%)
```

7 个未匹配符号逐条核实（与上轮 §1.2 结论一致，无新增真缺口）：`NO_EVENTSUB` 有语义表达（`event.rs:76` `NO_EVENTSUB_RAW`，工具不匹配名称）；`SEND_PRIORITY`/`SEND_TIME_SLICE` 真缺口（挂 edge E7，`const.h:19-20`）；`ESCRIPT` C 死代码正确缺失；`EXTERN`/`_SYSTEM`/`_TABLE` C 编译宏无需对应。

gate-evidence-E（测试基线与测试名对账）：

```bash
cargo test -p minix-pm --lib    # 346 passed; 0 failed
cargo test -p minix-pm          # + 7 integration passed（run_once_integration）
cargo clippy -p minix-pm --lib  # 0 warning 0 error（32 条 workspace profile 告警属其他 crate，非本轮范围）
bash tools/check-rs-unwired.sh  # PASS
```

todo.md 引用的测试函数全部 grep 命中：`test_call_nr_roundtrip_all_registered`（`ipc/calls.rs:343`，注意上轮 §0.1 标注的 :222 已因后续修复漂移）、`test_disinherit_new_parent`（`exit.rs:862`，上轮标注 :544 同样漂移）、`test_tell_parent_delivers_rusage_via_datacopy`（`exit.rs:760`）。行号漂移本身按模式 77 处理，不在本轮逐条修复。

### 11.1 查漏补缺总表 V2

#### 11.1.1 47 调用号矩阵复核 + 40 臂接线批次表

现状复核：接线仍是 7 个（Exit/Fork/Wait4/Kill/ProcEventMask/SrvFork/SrvKill），经 `dispatch_pm_call` 的 7 个显式臂分发；其余 40 个落入兜底臂 `_ => ReplyIntent::Reply(ENOSYS)`（`os/servers/pm/src/ipc/calls.rs:283`）。与上轮 §1.1 的差异：上轮描述的"`init.rs` 内联拦截"已被 Fix #2 消除，矩阵的"可达"列整体 +1 精度。40 臂的接线前置条件与建议批次（每批 = wire 类型 + match 臂 + 集成测试 + 文档同步）：

| 批次 | 调用号 | C handler | Rust 逻辑位置 | 前置条件 |
|------|--------|-----------|--------------|----------|
| A 凭证（13 个） | 4,5,6,9,10,12,13,15,16,29,30,31,32 | do_get/do_set（getset.c） | `credentials.rs:87/148` | wire 类型（`m_lc_pm_getuid` 族）；`CopyGroups` 生产实现（需 minix-sys `sys_datacopy` wrapper，edge E6）；`VfsForwarder` 生产实现（`tell_vfs` + `ipc/vfs.rs:242-249` 的回复分支已备） |
| B 信号控制（6 个） | 8,20,21,22,23,24 | do_trace/do_sigaction/do_sigsuspend/do_sigpending/do_sigprocmask/do_sigreturn | `trace.rs`/`signal_handlers.rs` | wire 类型；`sys_sigreturn` wrapper（E6）；**必须先修 V2-P0-2**（集合位序错位会被 sigaction 落地放大） |
| C 时间（6 个） | 7,28,33,34,35,36 | do_stime/do_time/do_getres/do_gettime/do_settime/do_getrusage | `time.rs`/`misc.rs` | `ClockSource` 生产实现（内核 GETUPTIME 面，E6）；`sys_settime`/`sys_stime` wrapper（E6）；rusage 拷贝复用 `KernelGateway::copy_to_user`（已备） |
| D 定时器（1 个） | 17 | do_itimer | `timer.rs:368` | **V2-P2-1**（CLOCK notify 接线 + 周期重挂收敛）；`TimerCtl`/`VTimerCtl` 生产实现（`sys_setalarm`/`sys_vtimer`，E6）；`sys_datacopy`（itimer value 双向拷贝） |
| E exec（3 个） | 14,43,44 | do_exec/do_newexec/do_execrestart | `exec.rs:92/156/200` | **V2-P2-2**（caller 门）；wire 类型；`sys_exec` wrapper 与 kernel 对端（D-08 注）；D-16 core-name 契约（edge E7） |
| F 调度（2 个） | 26,27 | do_getsetpriority | `sched.rs` | SCHED 客户端（D-12/A-8）；`SEND_PRIORITY`/`SEND_TIME_SLICE` 常量（E7） |
| G 杂项（9 个） | 18,19,25,37,38,39,45,46,47 | do_[gs]etmcontext/do_sysuname/do_reboot/do_svrctl/do_sprofile/do_getepinfo/do_getprocnr/do_getsysinfo | `misc.rs` | wire 类型；`sys_[gs]etmcontext`/`sys_sprof` wrapper（E6，sprof 带 feature 门）；reboot 的 `sys_abort` 已备（Fix #25） |

#### 11.1.2 C 函数面第二轮对账（utility/getset/time/mcontext/profile/alarm 逐函数）

上轮对账止步于 proto.h 粒度；本轮对 6 个易漏 C 文件逐函数核对。新发现两条（详见 V2-P1-1、V2-P3-2），其余判定：

| C 函数 | 判定 | 证据 |
|--------|------|------|
| get_free_pid（utility.c:32-49） | **行为偏差**（相位偏移） | C 先自增再检查（utility.c:38），首个分配 INIT_PID+2=3，PID 2 永不使用；Rust 返回自增前旧值（`mproc/pid_gen.rs:132-157`），首个分配 2，且 `pid_gen.rs:196-201` 测试把该相位锁死为断言 |
| find_param（utility.c:57-71） | 覆盖（形参化） | `misc.rs:184`（monitor 参数显式传入，C 读全局 monitor_params——A-3 惯例） |
| find_proc（utility.c:76-84） | 覆盖 | `mproc/table.rs:225` |
| nice_to_priority（utility.c:86-99） | 覆盖 | `sched.rs:280` → `NiceMapping::to_queue`（上轮已验证公式） |
| pm_isokendpt（utility.c:104-115） | 覆盖（逐分支等价） | `mproc/table.rs` `pm_isokendpt`：槽位域 EINVAL / endpoint 不匹配或非 IN_USE EDEADEPT |
| tell_vfs（utility.c:108-127） | 覆盖但错误路径偏软 | `ipc/vfs.rs:182-200`：not-idle 与发送失败 C 均 panic，Rust 均返回 Err——错误去向见 V2-P2-3 |
| set_rusage_times（utility.c:133-152） | 覆盖（内联进 tell_parent） | `exit.rs:535-550`（按 `table.system_hz` 换算，对齐 C `sys_hz()`） |
| do_get/do_set 13 调用（getset.c） | 覆盖，一处恒假检查 | `credentials.rs:87/148`；GID_MAX 检查恒假见 V2-P3-2；SETSID/ISSETUGID/GETSID 按 pid 定位全部在位 |
| do_gettime/do_getres/do_settime/do_time/do_stime（time.c） | 覆盖（seam 后待生产 impl） | `time.rs:119/137/142/167/181/188`；REALTIME/MONOTONIC 分支、EPERM 门、boottime 换算逐点对应 |
| do_[gs]etmcontext（mcontext.c:13/23） | 覆盖（seam） | `misc.rs:480/487` `McontextCtl` |
| do_sprofile（profile.c:22-45） | 覆盖（同型 ENOSYS） | `misc.rs:466-477`：`#if SPROFILE` ↔ `#[cfg(feature = "sprofile")]`，两侧默认都返回 ENOSYS |
| do_itimer/set_alarm/cause_sigalrm（alarm.c:92-154/299-311/317-344） | 覆盖但周期重挂路径分裂 | `timer.rs:368/286/301`；见 V2-P2-1 |

#### 11.1.3 DEFERRED / stub 全量收敛（24 处 grep 对账）

`grep -rn 'DEFERRED\|unimplemented!\|todo!' os/servers/pm/src` 共 24 处。对账结论：

- 与 §6 台账一致且仍开放：D-01（`transport.rs:86/91/96`）、D-02（`main.rs:15`）、D-05（`ipc/vfs.rs:453`）、D-12（`init.rs:578/609/622`）、D-16（`exit.rs:261/265`）、D-17（`exit.rs:323`）。
- **台账错位一处**：`exit.rs:305`、`exit.rs:676` 仍挂 `[DEFERRED: D-13]`，但 §6 的 D-13 行已被 Fix #23 整体划账（该修复只覆盖 `do_exit` 的 PRIV_PROC 违规分支一处 `sys_kill`）。SIGHUP 广播两站点实为未实现——升格为 V2-P1-2。
- **stale 注释四处**（均已实现或前提失效，代码注释未跟上）：`wait.rs:128-131`（D-21 已由 Fix #27 实现 + `let _ = rusage_addr` 死绑定）；`exit.rs:515-516`（doc 注释仍写 "sys_datacopy of rusage (omitted)"）；`event.rs:213-214`（doc 注释写 "两者 DEFERRED，当前仅清标志"，实际 `event.rs:272/278` 已真实调用 `exit_restart`/`restart_sigs`——Fix #7 之后未更新）；`main.rs:10`（"RS_INIT 握手归主循环（04）"——本树 C 的 PM 根本没有 RS_INIT 握手，`grep -rn RS_INIT minix3/minix/servers/pm/` 零命中，该前提应删除）。登记为 V2-P3-1。
- 其余 10 处为文档注释性质的 DEFERRED 字样（`transport.rs:67/197`、`ipc/vfs.rs:44/100/351/555`、`ipc/dispatcher.rs:335`、`ipc/calls.rs:14/442`、`exit.rs:676` 已计入上两行），无新缺口。
- D-21 划账与 Fix #27 补记已于本轮账目对账完成（见 §6 D-21 行、§10 Fix #27）。

#### 11.1.4 Minix3 易漏语义抽查第二批（10 项）

判定四色：covered（语义等价）/ gap（缺失或行为不同）/ deferred（显式 DEFERRED 有归属）/ N.A.（本树 C 不存在该语义，Rust 一致缺失）。

| # | 语义点 | 判定 | 关键锚点 |
|---|--------|------|----------|
| 1 | kill 负数/0 广播四分支 + 权限（非 root 需 uid 有交集） | covered（子项 gap 见 V2-P2-4） | C signal.c:597-632 ↔ `signal.rs:106-149`、`can_signal` 四组合 |
| 2 | setsid 规则（procgrp==pid → EPERM）+ VFS 转发 SUSPEND | covered（`credentials.rs:212-219`），整调用未接线归批次 A；setpgid **N.A.**（callnr.h 无此调用，两侧一致无） | C getset.c:205-212 |
| 3 | wait4 options：C 只测 WNOHANG（forkexit.c:553-554），未知位不过滤 | covered（`wait.rs:142-144`；i32→u32 位型保持） | C forkexit.c:487-491 |
| 4 | exec 后信号处置重置（catch 清、ignore 保持）+ SIGKILL 不可捕获/阻塞五处 | covered（微小超集：Rust 额外清 sa_flags，V2-P3-4 备注） | C exec.c:178-184、signal.c:49/80-187 ↔ `mproc/signal.rs:26/233-275`、`mproc/signal.rs:359-370` |
| 5 | sigsuspend/sigreturn mask 保存恢复（sigmask2 闭环） | covered | C signal.c:160-192/792-795 ↔ `signal_handlers.rs:168-197`、`mproc/signal.rs:266-298` |
| 6 | alarm 跨 fork 清零 / 跨 exec 保留 / 退出清 | covered | C forkexit.c:104-113、exec.c（不触碰 timer）、forkexit.c:300-301 ↔ `fork.rs:236/340/347`、`exit.rs:200-207` |
| 7 | core dump 触发链（core_sset + dump_core 双重门 + VFS_PM_DUMPCORE） | **gap**：位序错位使有效 core 集合漂移（V2-P0-2）；`path: 0` 已 D-16 登记；`TestIpcTransport` 泄漏见 V2-P0-1 | C signal.c:545-563、forkexit.c:285-292/351-357 ↔ `signal.rs:320-336`、`exit.rs:174-186/253-272` |
| 8 | getsysinfo/reboot superuser 门；svrctl 无门（C 也无）；getprocnr RS-only | covered（svrctl 的 IOCGROUP 前置校验差异记 V2-P3-4） | C misc.c:116-122/204/307/154-157 ↔ `misc.rs:307-350/384-428` |
| 9 | session leader 死亡向进程组发 SIGHUP；普通组长死亡无传播（两侧一致） | **gap**（V2-P1-2）；普通组长 N.A. | C forkexit.c:298/411-412 ↔ `exit.rs:302-307/674-676` |
| 10 | PAUSE 调用 | N.A.（callnr.h 47 调用无 PAUSE，两侧一致无） | C callnr.h:14-60 |

### 11.2 V2 条目

#### V2-P0-1 信号终止路径把 VFS 告知发进测试 mock，进程卡死且 VFS 永不知情（✅ 已修复 2026-09-08，见 §10 Fix #30）

- **优先级**：P0（可达路径上的真实行为错误——Kill=11 已接线）
- **类型**：代码 bug（测试 mock 泄漏进生产路径）
- **文件**：`os/servers/pm/src/signal.rs:318-336`（`sig_proc_exit`）、`os/servers/pm/src/ipc/transport.rs`（`TestIpcTransport::send` 推入本地 Vec 即返回 Ok）
- **问题**：`sig_proc_exit` 在生产代码中构造 `let mut nop = crate::ipc::TestIpcTransport::default();` 并传给 `exit_proc`（signal.rs:333-334；该函数在 `#[cfg(test)]` 模块之外，tests 模块始于 signal.rs:411）。`exit_proc` 的收尾对 C `forkexit.c:350-358` **无条件** `tell_vfs`（DUMPCORE 或 EXIT 二选一），Rust 侧该消息进入一次性 mock 即被丢弃（`exit.rs:271` 还叠加 `let _ =` 忽略返回值）。后果分两支：(a) core 信号（默认处置 SIGSEGV 等）走 dump_core=TRUE 分支——不 zombify、等 VFS 回复驱动 `exit_restart`，但 VFS 永远收不到请求，进程**永久卡在 EXITING+VFS_CALL**，父进程 wait4 永久挂起；(b) 普通终止信号（含 SIGKILL）虽能 zombify，VFS 侧永远收不到 VFS_PM_EXIT，跨服务器状态漂移（fd/锁不清理）。注释里 "no VFS" 的理由与事实不符：exit_proc 恰恰必然 tell_vfs。
- **证据**：`cargo test -p minix-pm` 全绿（346+7）——现有测试全部经由 mock 或未覆盖信号终止的 VFS 时序，即"测试通过"恰因 mock 吞掉了差异；`kill(pid, SIGKILL)` 从 `dispatch_pm_call` Kill 臂（`ipc/calls.rs`）→ `do_kill` → `check_sig` → `sig_proc:316` → `sig_proc_exit` 全程可达。
- **建议**：方案一（首选）：`sig_proc_exit` 增加 `transport: &mut dyn IpcTransport` 形参（`check_sig`/`sig_proc` 链上已贯穿 transport，只差最后一段），删掉 mock 构造；补集成测试：kill + 默认处置 SIGSEGV → 断言 VFS 收到 `VFS_PM_DUMPCORE` 且父进程 wait 可回收。方案二（次选）：若双借用是动机，把 tell_vfs 提为 `exit_proc` 之后的独立步骤并传真实 transport；劣于方案一（拆散 C 的顺序语义）。修复时顺带处理 exit.rs:271 的 `let _ =`（见 V2-P2-3）。
- **验证**：新集成测试失败→修复→通过；`grep -n 'TestIpcTransport' os/servers/pm/src --include='*.rs' -r` 在非测试代码零命中（配合 V2 11.5 的模式提案）。

#### V2-P0-2 信号集合位约定分裂：默认忽略集失效（SIGCONT 误杀）、core 集漂移（SIGKILL 误入）（✅ 已修复 2026-09-08，见 §10 Fix #29）

- **优先级**：P0（可达路径上的真实行为错误，与 V2-P0-1 同链路叠加）
- **类型**：代码 bug（数据表示的 producer/consumer 位基不一致，且被测试锁死）
- **文件**：`os/servers/pm/src/init.rs:90-92`（`sig_bit(sig) = 1u64 << sig`，producer 位基 = 信号编号）、`init.rs:703-718`（`test_signal_sets_match_c` 断言 `1<<3|…|1<<11`，把错位约定固化为契约）；消费侧 `os/servers/pm/src/signal.rs:326`（`CORE_SIGSET & (1<<(signo-1))`，位基 = 编号-1）、`signal.rs:275-277`（badignore）、`signal.rs:309-313`（默认忽略门）
- **问题**：C 的信号集合位基是 `bit(signo-1)`（`sys/sys/sigtypes.h:67-71` `__sigmask(n) = 1<<((n-1)&31)`，`sigaddset` 语义；集合构建见 main.c:154-165）。Rust 的 producer 用 `1<<sig`、consumer 用 `1<<(signo-1)`，两者相差一位。按 Minix 编号表（`sys/sys/signal.h:52-83`：SIGEMT=7、SIGBUS=10、SIGCONT=19、SIGCHLD=20）推算实际效果：有效 core 集 = {ILL,TRAP,ABRT,EMT,FPE,**KILL**,SEGV,**SYS**}（C 为 {QUIT,ILL,TRAP,ABRT,EMT,FPE,BUS,SEGV}——QUIT/BUS 丢 core，KILL/SYS 误入）；有效默认忽略集 = {**TTIN**,CHLD,INFO,**USR1**}（C 为 {CHLD,CONT,WINCH,INFO}——**CONT 与 WINCH 失去默认忽略**）。当下可达后果：所有进程的 ignored/caught 集合为空（sigaction 未接线），`kill(pid, SIGCONT)` 经 `sig_proc` 一路落到 `signal.rs:316` 终止——C 中 SIGCONT 默认忽略（signal.c:535-539 `sigismember(&ign_sset, signo)` → return），进程不该死。SIGKILL 误判 core 会叠加 V2-P0-1（走 dump 路径卡死）。
- **证据**：`init.rs:90-92/99-125` 与 `signal.rs:326` 并排读即证；`init.rs:707` 断言值 `(1<<3)|…|(1<<11)` 即错位本体（C 对应集合应是 `1<<(3-1)|…`）。
- **建议**：方案一（首选）：`sig_bit` 改为 `1u64 << (sig - 1)`（对齐 C `__sigmask`），`init.rs:703-718` 断言值同步 -1；consumer 不动。方案二：consumer 全部改 `1<<signo` 对齐 producer。选一后全文 grep `CORE_SIGSET|IGN_SIGSET|NOIGN_SIGSET` 复核每个消费点（含 `signal.rs:276` 的 badignore）。**顺带修 badignore 谓词**：C 是 `sigismember(&noign_sset, signo) && (ignored(signo) || masked(signo))` 的单信号成员判定（signal.c:483-486），Rust 写成了"集合整体与 NOIGN_SIGSET 有交叠"（`signal.rs:275-277`），即使位基统一后谓词仍不等价，须改为 `(state.ignored | state.mask) & sig_bit(signo) != 0`。
- **验证**：新增单元测试：`is_core` 对 1..=_NSIG 全信号逐个断言与 C core_sigs[] 一致；`kill(pid, SIGCONT)` 集成测试断言进程存活且回复 0。

#### V2-P1-1 PID 轮转相位偏移：C 永不分配 PID 2，Rust 从 2 开始（✅ 已修复 2026-09-08，见 §10 Fix #31）

- **优先级**：P1
- **类型**：语义偏移（外部可观察值）
- **文件**：`os/servers/pm/src/mproc/pid_gen.rs:132-157`（`get_free_pid`）、`pid_gen.rs:196-201`（`test_pid_first_allocation` 断言 `INIT_PID + 1`）
- **问题**：C `get_free_pid`（utility.c:32-49）的 `next_pid` 先自增再查重再返回（utility.c:38），首个分配值 = INIT_PID+2 = 3，PID 2 在系统全生命周期永不使用；Rust 返回自增前的旧值，首个分配 = 2。启动时 RS/PM/FS 等全部 boot 系统进程的 PID 整体漂移一位（`init.rs:460-465` 消费同一生成器）。PID 是外部可观察值（ps/getpid/kill 语义），属 Rewrite 契约保护面。
- **建议**：方案一（首选）：`get_free_pid` 返回自增后的值（把 candidate/next 语义对调），`test_pid_first_allocation` 改断言 `INIT_PID + 2`，回绕分支同步核对（C 回绕后首个是 2，即 `next_pid=NR_PIDS → 下一值 INIT_PID+1=2`）。方案二：维持现状并在模块头声明"相位差异有意"——不可取：无任何理由偏离 C 的编号序列，且 pid 2 在 C 中被跳过可能正是某些历史工具的隐含依赖。
- **验证**：单测断言首个 PID=3、回绕后=2；`fill_boot_procs` 后 RS 的 PID 与 C 一致。

#### V2-P1-2 session leader 死亡的 SIGHUP 组广播未实现，且 D-13 台账错位、§0.1 基线过度声称（✅ 已修复 2026-09-08，见 §10 Fix #32）

- **优先级**：P1
- **类型**：语义缺失 + 账目错位
- **文件**：`os/servers/pm/src/exit.rs:302-307`（exit_proc 第 13 步 no-op）、`exit.rs:674-676`（zombify 路径 no-op）
- **问题**：C 在 exit_proc 尾部 `if (procgrp != 0) check_sig(-procgrp, SIGHUP, FALSE)`（forkexit.c:411-412，procgrp 的捕获在 :298），即会话首领死亡向其进程组广播 SIGHUP——POSIX 挂断传播的核心机制。Rust 两处均为 `let _ = procgrp;` no-op，注释挂 `[DEFERRED: D-13]`。但 §6 的 D-13 行已被 Fix #23 划账（该修复只覆盖 do_exit 的 PRIV_PROC 违规 sys_kill 一处），这两处成了**挂在已关闭编号下的活缺口**；§0.1 基线表"session leader 死亡记忆 procgrp 并发 SIGHUP（exit.rs:76-80 与 exit.rs:176）"为过度声称（所指行号实为 TrapKernelGateway 的 sys_kill 实现，且"发"SIGHUP 从未存在）。
- **建议**：方案一（首选）：实现广播——`procgrp != 0` 时对表内 `procgrp` 匹配的活进程逐个走 `sig_proc(target, SIGHUP, ksig=false)`（复用 `signal.rs` 既有投递路径；C 的 check_sig 负 pid 即此语义，signal.c:568），exit_proc 第 13 步与 zombify 路径共用一个函数；在 §6 新开 D-27（SIGHUP 广播）承接两处标记，D-13 行注明"仅覆盖 PRIV_PROC 违规分支"。方案二：若判定"会话语义整体归 13/11 文档后续阶段"，也必须先做台账拆分（新编号 + §6 行），不能留死引用。选方案一：代码路径已备，工作量小。
- **验证**：集成测试：三个进程同组、组长退出 → 两成员收到 SIGHUP 默认终止；`grep -rn 'D-13' os/servers/pm/src` 命中行的归属与 §6 一致。

#### V2-P2-1 itimer 的 CLOCK notify 未接线，且周期重挂逻辑三处分裂（A-7 的锐化）（✅ 已修复 2026-09-08，见 §10 Fix #39；TimerCtl 生产实现仍挂 edge E6/E1）

- **优先级**：P2
- **类型**：接线缺口 + 逻辑层内部分裂
- **文件**：`os/servers/pm/src/init.rs:349-352`（notify 一律跳过，CLOCK 分支显式留白）、`os/servers/pm/src/timer.rs:317-322`（`cause_sigalrm` 的 interval>0 分支为空壳注释）、`timer.rs:334-366`（`handle_clock_notify` 直接改表内簿记，`let _ = tctl` 绕过 TimerCtl seam）、`timer.rs:177-179` + `timer.rs:443`（`SigSender` trait 仅测试实现）
- **问题**：C 的 alarm/周期 itimer 由内核 watchdog 驱动：`set_alarm` → `set_timer`（kernel timer，alarm.c:299-311），到期回调 `cause_sigalrm`（alarm.c:317-344）→ `check_sig(SIGALRM)`，周期>0 时**回调内再调 `set_alarm` 重挂内核 timer**（alarm.c:338）。Rust 现状三处各做一半：主循环收到 notify 直接跳过（A-7 上轮只登记为"内核定时器抽象未实现"，未暴露下面两层）；`cause_sigalrm` 的周期分支空壳；`handle_clock_notify` 有重挂但只写 `resources.timer` 字段、不调 `tctl.set(...)`——内核 seam 接通后周期 itimer 只会触发一次。另 `SigSender` 绕过了 C 的 `check_sig` 语义（alarm.c:341 走完整投递判定），生产实现出现时必须桥回 `signal.rs`。
- **建议**：方案一（首选，批次 D 前置）：收敛为单一重挂点——`cause_sigalrm` 拿到 `TimerCtl` 完整做 C 语义（check_sig + 周期重挂经 `tctl.set`），`handle_clock_notify` 退化为"扫描到期 + 调 cause_sigalrm"；init.rs:349-352 接 `handle_clock_notify`（notify 源 = CLOCK endpoint 判定）；删除 `SigSender`，cause_sigalrm 直接调 `signal.rs` 的投递函数（check_sig 的 ksig=FALSE 分支）。方案二：保留 SigSender 但提供生产 impl 桥接 check_sig——多一层间接，无收益。C 的 O(timer 队列) 到期 vs Rust O(NR_PROCS) 扫描的差异在文档标注即可（每 tick 扫描上限 NR_PROCS≈256，量级有界且不随定时器数量增长）。
- **验证**：单测：周期 itimer 两次到期两次 SIGALRM；`grep -n 'let _ = tctl' os/servers/pm/src/timer.rs` 零命中；init.rs 的 CLOCK notify 分支调用 `handle_clock_notify`。

#### V2-P2-2 do_exec 缺 VFS/RS 调用者门（exec 三个入口中唯一漏网）（✅ 已修复 2026-09-08，见 §10 Fix #38）

- **优先级**：P2
- **类型**：权限校验缺失（当前不可达，接线前必修）
- **文件**：`os/servers/pm/src/exec.rs:92-104`（`do_exec` 签名收 `caller` 却 `let _ = caller; let _ = table;`）
- **问题**：C 要求 exec 只能由 VFS 或 RS 发起（exec.c:70-71 `if (who_e != VFS_PROC_NR && who_e != RS_PROC_NR) return EPERM`）。Rust 的 `do_execrestart` 有 RS 门（exec.rs:206-208 ✓）、`do_srv_fork` 有 RS 门（fork.rs:122-124 ✓）、`do_srv_kill` 有（signal.rs:67-69 ✓）、`do_getprocnr` 有（misc.rs:345-347 ✓）——唯独 `do_exec` 没有任何调用者校验。exec 是权限敏感入口（TAINTED/setuid 链的起点），接线时若不补门，任意进程可代他人发起 exec。
- **建议**：方案一（首选）：`do_exec` 开头加 `caller ∈ {VFS, RS}` 门（返回 `ExecError::Perm`），与同文件 do_execrestart 同风格。方案二：把校验放 dispatch 臂——劣：校验离语义实现远，单测覆盖不到。
- **验证**：单测：普通进程 endpoint 调 do_exec → Perm。

#### V2-P2-3 tell_vfs 错误路径两处 fail-open（C 均为 panic）（✅ 已修复 2026-09-08，见 §10 Fix #35）

- **优先级**：P2
- **类型**：fail-closed 契约破坏（P1-2 同型残留）
- **文件**：`os/servers/pm/src/exit.rs:271`（`let _ = crate::ipc::tell_vfs(...)` 完全吞错）、`os/servers/pm/src/fork.rs:83-84/181-182`（`map_err(|_| ForkCoordError::VfsError)` 把不变式违规降级为用户可见 fork 失败）
- **问题**：C 的 `tell_vfs` 两处 panic（utility.c:122-123 not-idle、:127-129 发送失败）——两者都意味着 PM 或传输层已坏，进程状态不可信。Rust 的 `tell_vfs` 改回 Err（`ipc/vfs.rs:182-200`，方向本身合理），但两个调用方把它变成静默/降级：exit 路径吞错 = 进程死亡时 VFS 告知可能静默丢失（与 V2-P0-1 叠加）；fork 路径把内部不变式违规（刚创建的子槽不可能 not-idle）伪装成一次普通的 fork 失败（父进程收到 errno，真实病因不可见）。
- **建议**：方案一（首选）：区分两类错误——`NotIdle` 属不变式违规，调用方 `panic!`（对齐 C）；`SendFailed` 在 exit 路径 panic（C 同）、fork 路径可保留降级但注释标注"传输层损坏"语义。方案二：全链 panic（最贴 C）——劣：失去对传输抖动的表达力，且与 `tell_vfs` 返回 Result 的既有设计冲突。
- **验证**：`grep -n 'let _ = .*tell_vfs' os/servers/pm/src` 零命中；单测：not-idle 构造 → panic（`#[should_panic]`）。

#### V2-P2-4 kill(-1, SIGTERM) 的 RS 优先通知空转（✅ 已修复 2026-09-08，见 §10 Fix #33）

- **优先级**：P2
- **类型**：语义偏移
- **文件**：`os/servers/pm/src/signal.rs:100-104`（广播 TERM 先对 RS 槽调 `sig_proc(ksig=false)`）+ `signal.rs:252-260`（PRIV_PROC 的 `!ksig` 分支 `let _ = (target, signo)` 空操作）
- **问题**：C 对广播 SIGTERM 的特殊处理是 `sys_kill(RS_PROC_NR, signo)`（signal.c:588-589）——经内核产生真实 ksig 回环，RS 确实收到信号。Rust 在用户态直接对本表 RS 槽调 `sig_proc`，而 `sig_proc` 的 PRIV_PROC 分支在 `!ksig` 时什么都不做 → RS 实际收不到任何通知。C 之所以走内核，是因为 PM 无权直接投递系统进程信号；Rust 绕过了这层语义。
- **建议**：方案一（首选）：与 C 同型——该分支改调 `kern.sys_kill(Endpoint::RS, SIGTERM)`（KernelGateway 已有该方法，Fix #23 落地），由内核 ksig 回环驱动后续。方案二：给 PRIV_PROC !ksig 分支实现直接投递——劣：偏离 C 的特权模型（PM 不直接决定系统进程的死活）。
- **验证**：单测：kill(-1, SIGTERM) → 断言 mock 网关收到 sys_kill(RS, SIGTERM)。

#### V2-P2-5 check_parent 的"父未等待 → SIGCHLD"分支 no-op（✅ 已修复 2026-09-08，见 §10 Fix #34）

- **优先级**：P2
- **类型**：语义缺失（含未登记的 DEFERRED）
- **文件**：`os/servers/pm/src/exit.rs:448-451`
- **问题**：C 的 `check_parent` 在父进程未等待子进程时执行 `sig_proc(p_mp, SIGCHLD, TRUE, FALSE)`（forkexit.c check_parent 尾部 else 分支）——装了 SIGCHLD 处理器的父进程靠它得到通知。Rust 该分支是 `let _ = (parent_slot, child_slot);`，注释写 "(11-signal-core.md, deferred)" 但无 D-XX 编号，不满足模式 60 的登记要求。注意与上轮 §1.3 的抽查结论无冲突（该轮未覆盖此点）。
- **建议**：方案一（首选）：在 §6 登记 D-28 并实现——`sig_proc(parent, SIGCHLD, trace=TRUE, ksig=FALSE)` 一行调用（依赖 V2-P0-2 先修，否则 SIGCHLD 的默认忽略判定仍在错位状态下运行——按 11.1.4 的推算 CHLD 恰好落回有效忽略集，但这属于巧合而非正确）。方案二：仅登记不实现，随批次 B（信号族接线）一并做——可接受，但 D 编号必须先落账。
- **验证**：单测：父进程装 SIGCHLD handler（sigaction 接线后）+ 子进程退出 → handler 收到信号；无 handler 时进程存活。

#### V2-P2-6 ENOSYS 兜底臂吞掉 40 个调用的逐项登记义务（✅ 已修复 2026-09-08，见 §10 Fix #40）

- **优先级**：P2（流程性）
- **类型**：诚实契约缺口（模式 60）
- **文件**：`os/servers/pm/src/ipc/calls.rs:283`（`_ => ReplyIntent::Reply(ENOSYS)`）
- **问题**：兜底臂使"未接线"无需任何 per-call 标记即可编译通过，40 个调用的过渡状态只存在于 04 文档 §3.6 D6 的一段总述里；其中 setsid/sigaction 族/exec 族等**逻辑已齐备**的调用与逻辑也缺失的调用在 ENOSYS 上不可区分。独立代理审查与本轮均判定：这满足"文档化过渡差异"的底线（04 文档有论证），但不满足"每个 DEFERRED 可被一条 grep 找到"的 P2-5 既定标准。
- **建议**：以本文件 §11.1.1 的批次表为准台账（每个调用一行，含前置条件），不新增 40 个 D 编号；在 `calls.rs:283` 兜底臂注释指回该表。后续每接线一批，表中该行同步划账。
- **验证**：`calls.rs` 兜底臂注释含指向 §11.1.1 的引用；批次表随接线滚动更新。

#### V2-P2-7 unpause 的 VFS_CALL 分支不发 VFS_PM_UNPAUSE（2026-09-08 R2 执行中发现）

- **优先级**：P2；**类型**：语义缺失（caught 路径，sigaction 接线前不可达）
- **文件**：`os/servers/pm/src/signal.rs` `unpause`（VFS_CALL 分支 `return false`，无消息发送）
- **问题**：C 的 `unpause`（signal.c:719-770）对 VFS_CALL/EVENT_CALL 挂起的进程经 `tell_vfs(VFS_PM_UNPAUSE)` 请求 VFS 中断其阻塞调用，回复（Unpause 事件）到来后才建立 sigframe。Rust 该分支直接返回 false（信号转 pending），不发任何消息——被捕获信号对"卡在 VFS 调用里"的进程永远无法及时投递。
- **建议**：随 caught 路径补全（V2-P2-8）一并做：`unpause` 增 transport 形参，VFS_CALL 分支走 `crate::ipc::tell_vfs(VfsCall::Unpause)`（ipc/vfs.rs:285 的 Unpause 回复分支已备）。
- **验证**：单测：VFS_CALL 挂起进程 + 被捕获信号 → transport 收到 UNPAUSE 请求，VFS 回复后 sigframe 建立。

#### V2-P2-8 sig_send 是空壳（caught 投递的核心步骤缺失）（2026-09-08 R2 执行中发现）

- **优先级**：P2；**类型**：语义缺失（caught 路径，sigaction 接线前不可达）
- **文件**：`os/servers/pm/src/signal.rs` `sig_send`（`let _ = (table, target, signo); Ok(())`）
- **问题**：C 的 `sig_send`（signal.c:772-855）是被捕获信号投递的核心：保存/替换 mask、写 `mp_sigreturn`、构造 sigframe 参数、唤醒目标进程。Rust 版不做任何事直接返回成功——一旦 sigaction 批次（§11.1.1 批次 B）接线，被捕获信号将"看起来送达"而进程毫无感知。
- **建议**：方案一：按 C 全语义实现（依赖 mproc/signal.rs 的 `prepare_sigmsg`/`sigreturn_addr` 既有字段）。方案二：登记 D-29 并与批次 B 联动实施（sigaction 的 handler 安装 → sig_send 的 frame 建立 → sigreturn 的恢复，三步须同轮验证）。推荐方案二：三步分离会造成"半可达"状态。
- **验证**：批次 B 的集成测试：handler 进程收信号 → handler 执行 → sigreturn 恢复 mask（需进程上下文模拟，属 12-signal-handlers.md 范围）。

#### V2-P3-1 stale 注释与死绑定四处（随 P0/P1 修复顺带清理）

- **优先级**：P3；**类型**：注释漂移（模式 77 变体：不是行号漂移而是"实现已赶上/前提已消失"）；**文件**：`wait.rs:128-131`（D-21 已实现 + `let _ = rusage_addr;` 死绑定）、`exit.rs:515-516`（"(omitted)" doc 注释）、`event.rs:213-214`（"两者 DEFERRED"与 Fix #7 后的现实矛盾）、`main.rs:10`（"RS_INIT 握手归主循环"前提在本树 C 中不存在）。**建议**：逐处更新注释为当前事实；wait.rs 的死绑定删除。**验证**：四处 grep 逐条确认。

#### V2-P3-2 setgroups 的 GID_MAX 检查恒假（C 侧可拒绝 ≥2^31 的 gid）（✅ 已修复 2026-09-08，见 §10 Fix #37）

- **优先级**：P3；**类型**：语义偏移（微观）；**文件**：`credentials.rs:203-207`（`(g as u64) > GID_MAX`，Gid=u32 时恒假；测试 `test_setgroups_gid_max` 自述"无法构造超限值"）；C 侧 `getset.c:191` 以 `GID_MAX = 2147483647U`（`minix3/sys/sys/syslimits.h:53`）比较，gid_t 为 32 位时可拒绝 [2^31, 2^32-1] 区间。**建议**：`SetGroups` 改存 `u64` 或在 wire 解码层用 i64 载荷校验；或登记为"与 C 恒假检查同构的有意简化"（C 的检查实际上也只能在 64 位 gid_t 下触发，需先核实 C gid_t 宽度 `[待验证]`）。**验证**：构造 gid=2^31 的 setgroups（wire 层）→ EINVAL。

#### V2-P3-3 dispatch 的 Exit 臂丢弃 do_exit 返回的 ReplyIntent（✅ 已修复 2026-09-08，见 §10 Fix #36）

- **优先级**：P3；**类型**：健壮性；**文件**：`ipc/calls.rs` Exit 臂（`let _ = crate::exit::do_exit(...)` 后硬编码 `ReplyIntent::NoReply`）。`do_exit` 现两分支恒返 NoReply（`exit.rs:133-155`），丢弃等价；但未来语义变化时此处会静默吞回复。**建议**：直接返回 do_exit 的值（或 `debug_assert!(matches!(..., ReplyIntent::NoReply))`）。**验证**：编译 + 现有 exit 集成测试不回归。

#### V2-P3-4 三处微小诊断/校验差异（对账备注，不单独立项修复）

(a) C 的 sig_proc_exit 对非 PRIV_PROC 的 core 信号有 `printf("PM: coredump signal…") + sys_diagctl_stacktrace`（signal.c:556-558），Rust 无此诊断且 minix-sys 无 `sys_diagctl_stacktrace` wrapper（并入 edge E6 清单）；(b) C do_svrctl 有 IOCGROUP ∈ {'P','M'} 前置校验（misc.c:307），Rust 以形参化接口替代（misc.rs:403-428），权限面等价；(c) Rust exec 重置额外清 sa_flags（`mproc/signal.rs:359-370`），C 不清（exec.c:178-184），无已知行为影响。三项在对应批次接线时顺带对齐即可。

### 11.3 对照 Redox 的架构参考（V2）

> 本小节为 2026-09-08 联网调研成果（GitLab/GitHub raw 源码逐文件核对；行号对应 master 2026-09 快照与 commit `1b9fcbf593` 迁移前快照）。上轮 §7 的 5 条参考继续有效，其中"进程管理在内核"的表述已被本节 V2-Redox-1 的迁移事实取代。

**V2-Redox-1：Redox 正在把进程管理迁往用户态——"Minix3 式用户态 PM"路线获得了最强外部背书。** 2024-12-16 commit `1d5f8fd46d71`（"Move proc code to userspace."）删除了内核的 `src/context/process.rs`；master 上内核只剩线程级 `Context`（`src/context/context.rs:92-93` 注释 "typically mapped to a userspace thread"），退出时 `exit_this_context()`（`src/syscall/process.rs:37`）直接移除上下文、向 proc scheme 发 `EVENT_READ`——僵尸/收尸/kill 语义整体移交用户态 procmgr（后续 commit `cb1a838f052a` 2025-03-30 "Start moving kill to procmgr."）。对本项目的含义：PM 的表驱动生命周期不需要向"内核内进程模型"靠拢，方向与 Redox 的演进一致；上轮 §7 第 1 条"Redox 没有独立 PM 服务器"的表述按迁移后事实更正。

**V2-Redox-2：僵尸与等待记账的最小 Rust 表达（迁移前参照实现）。** 迁移前（commit `1b9fcbf593` 的父快照）`Process`（`src/context/process.rs:28`）= `ProcessInfo`（pid/pgid/ppid/session_id/ruid/euid 等，:36-54）+ `waitpid: Arc<WaitMap<WaitpidKey, (ProcessId, usize)>>` + `ProcessStatus::{PossiblyRunnable, Stopped(usize), Exiting, Exited(usize)}`（:71-76）。`Exited(usize)` 保留退出码滞留进程表直到 waitpid 的 `reap()`（`src/syscall/process.rs:615`）收走——语义即僵尸，但用"状态变体 + 等待信箱"而非独立 ZOMBIE 标志位。`WaitMap`（`src/sync/wait_map.rs:8`）是键控多播信箱：`WaitpidKey { pid, pgid }` 支持"任意子/按进程组"匹配，`receive_nonblock` 即 WNOHANG。对照 PM：`Lifecycle::{Zombie, ToldParent}` + `state.wait.waiting` 反向通知与 C 同型，是合理选择；若未来 wait4 需要支持复杂匹配（Redox 的 `grim_reaper` 闭包按 WUNTRACED/WCONTINUED 过滤，process.rs:665-681），WaitMap 是现成参考。另注意：本树 Minix3 的 wait4 只测 WNOHANG（forkexit.c:553-554），Redox 的 WUNTRACED/WCONTINUED 支持不构成对 PM 的行为要求。

**V2-Redox-3：孤儿的去向——两家语义不同，PM 跟 C 不跟 Redox。** Redox `exit()`（迁移前 `src/syscall/process.rs:75` 起）把孤儿交给**祖父**（`process.ppid = ppid`，注释原文 "Transfer child processes to parent (TODO: to init)"——交 INIT 尚未实现）；Minix3 是重挂 INIT（forkexit.c:760-795，PM 的 disinherit ✓ 已实现）。此为真实语义差异点：重写保持 C 行为正确，切勿"参考 Redox"改成祖父收养。

**V2-Redox-4：PID 分配——Redox 无轮转，PM 的轮转是 Minix3 语义，必须保真。** Redox 用 `NEXT_PID: AtomicProcessId` 顺序 `fetch_add`（`src/context/process.rs:77-79`，INIT=1），无复用，master 的 `src/context/context.rs:158` 还留着 "TODO: id can reappear after wraparound?" 的已知债。对照 V2-P1-1：Minix3 的 NR_PIDS=30000 轮转 + `mp_procgrp` 查重是明确的行为契约，修复相位偏移时应严格对 C，不存在"现代化"空间。

**V2-Redox-5：信号机制的终态形态（共享页 + trampoline）。** master 的 `SignalState`（`src/context/context.rs:173-186`）把 pending/blocked 掩码放用户态共享控制页，内核 `signal_handler()`（`src/context/signal.rs:7`）只在上下文切换时"查位 + 改寄存器跳 trampoline"，restorer 与 sigaction 全在 relibc（`redox-rt/src/signal.rs:24` 起）。SIGSTOP/SIGCONT 互清语义在迁移前内核 `send_signal` 特判（`src/syscall/process.rs:255-330`），迁移后由 procmgr 承担。对照 PM：Minix3 的决策全在 PM（check_sig/sig_proc）+ 投递经内核 ksig 回环，与 Redox 终态"决策在用户态"同向；`signal.rs` 现有的 ignored/caught/pending 位图与该形态同构，V2-P0-2 修复后即是对齐的正确基座。

**V2-Redox-6：fork 是协议不是指令——两家用不同协议实现了同一语义。** Redox 的 fork 完全在用户态（`redox-rt/src/proc.rs:959` `fork_impl`，经 proc fd 复制 + CoW 地址空间快照，内核只提供 CoW 原语——commit `11fdb3bb469` 2023-06-21）；Minix3 的 fork 是"PM 复制表项 + 通知 VM/VFS"的消息协议。共同点：fork 都不依赖内核魔法，都是进程管理器的显式多步协议。PM 的 fork 链（VFS_PM_FORK 异步回复 + vm_fork 同步往返）与 Redox 的句柄复制协议规模相当，结构不需要变。

**V2-Redox-7：用户态服务器事件循环的 Rust 惯例。** Redox 官方守护进程骨架 redox-scheme crate（v0.11.4）的主循环是 `loop { call_rw(...) }` + 按 opcode 分发到 `SchemeSync`/`SchemeAsync` trait 方法（`src/lib.rs:515`、`src/scheme.rs:861/1136`）；错误惯例是 `libredox::error::Error` + errno 整数编码（`Error::mux` 把 Result 编码为 -errno）。PM 的 `run_once` + `PmCall` enum match + `*Error::to_errno()` 与该形态逐点同构，判定：结构不动。no_std 侧的权威先例是 Tock `ErrorCode`（`kernel/src/errorcode.rs:13`，枚举 + `Result<T, ErrorCode>` + 边界转换），PM 的 `PmError::to_errno()` 模式与之一致；Tock 的 `process::State` 枚举（`kernel/src/process.rs:963-997`，`Stopped` 变体携带被打断前的状态用于恢复）对 PM 的 `Lifecycle::Stopped` 族建模是同向印证。

### 11.4 模块级观察（分层审查结论，未升级为条目部分）

- **L0 内核接入 seam**：PM 现有约 25 个端口 trait：全局 2 个（`IpcTransport`、`KernelGateway`）+ 内核能力拆分 4 个（`KernelStop`/`KernelSig`/`KernelResume`/`KernelExec`）+ 各子系统窄端口（`TimerCtl`/`VTimerCtl`/`ClockSource`/`SetTimeCtl`/`McontextCtl`/`SprofCtl`/`CopyGroups`/`VfsForwarder`/`SigSender` 等）。两种风格并存（中央网关 vs 接口隔离）是历史演化的自然结果：Fix #26 的信号链 kern 下穿采用了后者。判定：**不收敛为单一 trait**（接口隔离让每个子系统的测试替身最小化，符合"trait 至少两个行为不同实现"的存在性检查的初衷），但需要一个**内核能力 ↔ trait ↔ minix-sys wrapper ↔ C libsys 函数**的四列对照表进 plan.md ARCH 附录，作为接线批次的查询面（V2-P2-6 的配套）。`TestIpcTransport` 可在生产代码构造的问题是真实的（V2-P0-1 的温床），建议随 P0-1 修复将其移入 `#[cfg(test)]` 或加 `#[doc(hidden)]` + clippy lint。
- **L1 服务器骨架**：`run_once`（`init.rs:339-410`）与 C main.c:59-106 逐段对应且注释锚点完整；VFS 回复损坏 panic 有 C 同型依据（main.c:317-418 四类）；`run()` 的失败上限防 busy-spin 是合理增量。上轮"init.rs 965 行混合职责"的担忧在分发收编后已缓解（剩余体积主要是 boot 链 + 测试），不再立条目。`reply()` 复用 `ipc.reply` 预填载荷的机制与 C 的持久 `mp_reply` 缓冲同型（C 同样存在跨调用 stale 载荷的可能），Rust 的 `.take()` 还更干净——无需改动。
- **L2 分发层**：enum 穷尽 match + 兜底臂的结构判定维持上轮（防 translate 反向：不回退到 C 函数指针表）。兜底臂的登记义务见 V2-P2-6。
- **L3 子系统层**：exit→wait 僵尸链（zombify/tell_parent/cleanup 的顺序与 C forkexit.c:670-726 对齐，Fix #26/#27 后计账闭环）；exec 三方 restart 握手（PM↔VFS↔RS，exec_restart 的 RS 门 ✓）；signal 家族的 check_sig/sig_proc 骨架 ✓（缺口即 V2-P0-1/P0-2/P1-2/P2-4/P2-5）。错误码全部映射 Minix3 errno，未发现自造错误码。
- **L4 状态层**：`mproc.h` 38 个字段 + 19 个 flag 位全部在 `mproc/mproc.rs:296-331` 的映射表中可追溯（mp_magic → 类型系统不变量）；`Process` 四层组合 + `PmContext` 借用入口的边界经上轮与本轮两轮抽查无违例。`pid_gen` 的"只扫活进程 vs C 扫全槽"是已文档化的安全方向偏差（`pid_gen.rs:50`），保留。
- **L5 VFS 协议层**：`handle_vfs_reply` 的 11 路（SetUid/SetGid/SetGroups/SetSid/Exec/Core/Exit/Fork/SrvFork/Unpause/Reboot 特例）与 C main.c:334-419 逐路对应，每路的挂起点/恢复路径/提前 return 均有注释锚点（`ipc/vfs.rs:209-301`）；`tell_vfs` 三步语义对齐（错误路径偏软见 V2-P2-3）。上轮 §9.3 "PM↔VFS 类型已齐、无已知不一致"的判断维持。
- **L6 测试层**：346 lib + 7 integration；Gate E 测试名对账全过（见 11.0）。缺口是结构性的：**接线分支的集成测试为零**（40 臂 ENOSYS 只有一个汇总测试）——按 review.md 联调标准"每个架构分支 ≥1 集成测试"，该义务随 §11.1.1 批次表逐批履行（每批 = wire + 臂 + 至少 1 个 run_once 集成场景）。本轮两个 P0 都是"测试全绿但行为错误"，直接证据是 mock 掩盖——11.5 的模式提案由此而来。

### 11.5 Rule Discovery（Step 5.7）

本轮发现两个候选新模式：

1. **TSTL（Test Seam Leak，测试 seam 泄漏进生产路径）**——V2-P0-1 的根因模式。定义：seam 的测试实现（`TestIpcTransport` 类）可在 `#[cfg(test)]` 之外构造，生产代码用它填充暂时不想接线的依赖，测试全绿而生产路径丢失行为。检查命令：`grep -rn 'Test[A-Z][a-zA-Z]*::' os/servers/pm/src --include='*.rs' | grep -v '#\[cfg(test)\]'`（需人工排除测试模块）。建议严重度：P0（行为丢失类）。目标文件：`prompt/skill/review-patterns-skill.md`（新模式）+ review-code-checklist 的 seam 维度。与现有模式 4（虚构 trait）的区别：模式 4 查"trait 只有测试实现"，TSTL 查"测试实现被生产代码消费"。
2. **SBCD（Set-Bit-Convention Drift，位集合 producer/consumer 位基漂移）**——V2-P0-2 的根因模式。定义：同一 bit 集合的构建方与消费方使用不同位基（`1<<n` vs `1<<(n-1)`），且构建方的单测把错误约定锁死（断言值即错误本体）。检查命令：对每个 `*_SIGSET`/`*_MASK` 常量，grep 全部消费点的移位表达式并比对位基。建议严重度：P1（视消费点可达性可升 P0）。目标文件：同上。

### 11.6 建议的推进顺序（V2）

1. **V2-P0-2**（位序 + badignore，常量层小改动，先行——它影响所有后续信号行为的验证基准）。
2. **V2-P0-1**（sig_proc_exit 贯通真实 transport + `TestIpcTransport` 收进 `#[cfg(test)]` + 集成测试）。
3. **V2-P1-1/P1-2**（PID 相位一行修 + 测试更新；SIGHUP 广播复用 check_sig + D-27 落账 + §0.1 勘误）。
4. **V2-P2-5/V2-P2-4/V2-P2-3**（SIGCHLD 登记、RS 经 sys_kill、tell_vfs 错误路径——都是信号/退出链的收尾）。
5. **V2-P2-1 + 接线批次 D**（itimer 收敛后接 17 号调用）。
6. **接线批次 A → B → C → G**（凭证 → 信号控制 → 时间 → 杂项；每批含 V2-P2-6 的台账划账 + 集成测试）。
7. **批次 E（exec）**：依赖 V2-P2-2 门 + D-16 契约（edge E7）+ sys_exec（E6）。
8. **批次 F（调度）**：依赖 SCHED 服务器（跨阶段，06-stage）。
9. **V2-P3 批次 + plan.md ARCH 四列对照表**。
10. 跨阶段部分（E6 wrapper 清单扩充、E7 wire 成员与 rs_start 先例）见 §9 索引与 edge_todo.md 对应条目，单线程执行。
