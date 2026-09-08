# 04-stage-pm Rust 实现架构级 Review TODO

> 来源：架构级代码审查 + 查漏补缺系列（关注整体与分层架构，非逐函数审查）。已完成三轮：
> - **第 1 轮（2026-09-06/07，§0-§10）**：Gate A 覆盖度枚举（109 个 C 符号）+ 47 调用号矩阵 + 整体到分层架构审查。21 次 todo-fix 闭环 P1×4/P2×5/P3×2 + D-XX×20。
> - **第 2 轮（2026-09-08，§11）**：新基线全量重跑。V2-P0×2/P1×2/P2×8/P3×4 全部闭环（Fix #29–#42），新登记 D-27/D-28。
> - **第 3 轮（2026-09-09，§12）**：在新基线（356 lib + 8 integration / clippy 应为 0 / coverage 93.6%）上对第 1、2 轮未逐函数对账的 C 文件（trace.c/event.c/misc.c/schedule.c/main.c）做第 3 批逐函数对账 + Gate E 全量测试名对账 + 架构深审。发现 trace 域整体失真、内核信号主循环入口缺失、sig_proc 两处分支缺口、getsysinfo 假数据 stub、文档 §5 测试名大面积失同步等（V3-P1×5、V3-P2×10、V3-P3×6）。
> 范围：`os/servers/pm/src/` 全部 Rust 代码（37 文件，约 16,349 行），以及 `os/libs/minix-types/`、`os/libs/minix-sys/` 中与 PM 相关的类型边界。
> 定位：本文档是查漏补缺清单与架构改进建议清单，**不同于** `draft/`（旧 fork 主线素材，已停止维护）与 `plan.md` §7（文档 review 记录）。
> 跨阶段条目：抽取判定规则与映射见 §9，登记于 `notes/rewrite/fork-syscall-rewrite/edge_todo.md`（E1-E9 + E-VMMCPWIRE + E-VMMOCK 等）。
> 状态（2026-09-09，V3 实施中）：已闭环 V3-P1 全部（#43-#47）+ V3-P2-1/2/3/4/6/8（#48/#50/#51/#52/#53/#54）+ V2-P2-7/P2-8（#49）；开放 = P1-3/P1-4/P2-3/P2-6（第 1 轮遗留）+ V2-P2-7/V2-P2-8 + §11.1.1 批次 A/C/E/F/G（B 的 trace 半边完成）+ D-01/D-02/D-05/D-12/D-16/D-17（跨阶段）+ §12 其余 V3 条目。测试基线 **371 lib + 11 integration passed**；clippy lib 3 条 unused import 回退待 V3-P2-9。

---

## 0. 审查结论速览（第 1 轮）

一句话总结：**PM 的语义逻辑层完成度很高（47 个调用的业务逻辑全部有 Rust 模块，Minix3 易漏语义点基本都在），但逻辑层与消息循环之间的接线层几乎完全断开（47 个调用中 7 个在主循环里用裸魔数内联拦截，其余 40 个返回 ENOSYS），且内核与 VM 边界存在两处"假装成功"的接缝。** 主要工作不在补逻辑，而在收敛分发、打通边界、让测试覆盖到的代码真正被主循环走到。

> V2 轮修正：第 1 轮"逻辑层完成度很高"的判断在 trace 域不成立——V3 轮逐函数对账发现 `trace.rs` 是"部分 stub + T_* 常量 ABI 大面积错误"（§12 V3-P1-1）。"逻辑已备、只差接线"的表述此后仅适用于已逐函数对账过的域。

| 级别 | 条目 | 一句话 |
|------|------|--------|
| P1 | P1-1 | 分发层分裂成两套（✅ 已修复 2026-09-06，Fix #2） |
| P1 | P1-2 | `send_vm_fork` / `send_kernel_request` 假成功（✅ 已修复 2026-09-06，Fix #1） |
| P1 | P1-3 | 内核边界整体未实现：transport 三方法 `unimplemented!()`（跨阶段，挂 E1/E6） |
| P1 | P1-4 | codec 层缺口（ARCH A-4）：47 个调用只有 Wait4 载荷有 wire 类型（跨阶段主体挂 E7） |
| P1 | P1-5 | 测试验证"测试分发路径"而非生产路径（✅ 已修复 2026-09-06，Fix #3） |
| P1 | P1-6 | 入口命名分裂损害可追溯性（✅ 已修复 2026-09-06，Fix #4） |
| P2 | P2-1 | 虚构 cfg 特性门（✅ 已修复，Fix #17） |
| P2 | P2-2 | plan.md ARCH 表失同步（✅ 已修复，Fix #20） |
| P2 | P2-3 | `minix-types` 的 `PmRequest`/`PmResponse` 死代码 + 调用号双址（本体在共享层，挂 E7） |
| P2 | P2-4 | glob re-export 压平命名空间（✅ 已修复，Fix #18） |
| P2 | P2-5 | 行为 stub 未登记 DEFERRED 契约（✅ 已修复，Fix #19） |
| P2 | P2-6 | 文档 00/99 最小骨架 + `.design/` 快照缺失（模式 69） |
| P2 | P2-7 | C 死代码 `ESCRIPT` 未登记排除表（✅ 已修复，Fix #20） |
| P3 | P3-1/P3-2 | clippy 清理 + 无用导入（✅ 已修复，Fix #21；V3 复测发现 3 条回退，见 V3-P2-9） |

## 0.1 审查基线：确认良好、不需要动的部分

| 方面 | 证据 |
|------|------|
| `Process` 四层组合 + 与 `mproc.h` 逐字段映射表 | `os/servers/pm/src/mproc/mproc.rs:334`（结构体），`mproc/mproc.rs:302-331`（映射表） |
| SUSPEND 显式化为 `ReplyIntent` 三变体 | `os/servers/pm/src/ipc/dispatcher.rs:49-57`，契约见 `plan.md:393-406` |
| 47 个调用号 `PmCall` 枚举与 callnr.h 一一对应 + 全量 roundtrip 测试 | `os/servers/pm/src/ipc/calls.rs:30-125`，测试 `calls.rs:348` |
| Minix3 易漏语义点覆盖（V1 抽查 11 项 + V2 抽查 10 项） | 详见 §1.3 与 §11.1.4；其中两处 V2 勘误后由 Fix #29/#31/#32 修复 |
| 凭证 13 个调用建模为显式 `GetOp`/`SetOp` 枚举 | `os/servers/pm/src/credentials.rs:19-45`，对照 `getset.c:28/110` |
| 单线程 `&mut ProcTable` 用借用检查器替代 C 文件级全局（ARCH A-3） | `os/servers/pm/src/mproc/context.rs` |
| 每个模块头部带 Minix3 C file:line 锚点与归属文档编号 | 抽查 `ipc/calls.rs:1-17`、`ipc/dispatcher.rs:1-28`、`timer.rs:16` |

---

## 1. 查漏补缺总表（第 1 轮）

### 1.1 47 个调用号 × 接线 × 逻辑 × 测试 矩阵

> V3 复核（2026-09-09）：接线仍为 7 个（Exit/Fork/Wait4/Kill/SrvFork/SrvKill/ProcEventMask），其余 40 个落 `calls.rs:313` 兜底臂 ENOSYS。逐函数逻辑质量复查结论与第 1 轮"逻辑已备"的差异见 §12.1（trace 域降级、misc/sched 域多处部分实现）。

| 调用群 | 调用号 | 逻辑模块 | 消息循环可达 | 说明 |
|--------|--------|------------------|--------------|------|
| 生命周期 | 1 Exit, 2 Fork, 3 Wait4, 41 SrvFork | `exit.rs:133`、`fork.rs:22/112`、`wait.rs:38` | ✅ 经单一分发表 | Fix #2 收编后走 `dispatch_pm_call` |
| 信号发送 | 11 Kill, 42 SrvKill | `signal.rs:39/52` | ✅ 同上 | |
| 事件 | 40 ProcEventMask | `event.rs`（`do_proceventmask_mut`） | ✅ 同上 | 非 mut 变体有游标 bug，见 V3-P2-8 |
| 凭证 | 4-6, 9-10, 12-13, 15-16, 29-32（13 个） | `credentials.rs:87/148` | ❌ ENOSYS | 批次 A |
| 信号控制 | 8, 20-24 | `signal_handlers.rs`、`trace.rs` | ❌ ENOSYS | 批次 B；**trace.rs 状态见 V3-P1-1（部分 stub + 常量 ABI 错误）** |
| 时间 | 7, 28, 33-35 | `time.rs` | ❌ ENOSYS | 批次 C |
| 定时器 | 17 Itimer | `timer.rs:368` | ❌ ENOSYS | 批次 D；CLOCK notify 已接线（Fix #39） |
| exec 族 | 14, 43, 44 | `exec.rs` | ❌ ENOSYS | 批次 E；`do_exec` 调用者门已补（Fix #38） |
| 调度 | 26, 27 | `sched.rs:247` | ❌ ENOSYS | 批次 F；`sched_start_user` 语义偏差见 V3-P2-3 |
| ptrace | 8 | `trace.rs` | ❌ ENOSYS | 批次 B；**接线前必须先修 V3-P1-1** |
| 杂项查询 | 18-19, 25, 36-39, 45-47 | `misc.rs` | ❌ ENOSYS | 批次 G；`do_getsysinfo` 假数据见 V3-P1-4 |

### 1.2 C 函数面对账（第 1 轮，proto.h 粒度）

Gate A（coverage-extract.py）109 个 C 符号，Rust 名称匹配 102（93.6%）。7 个未匹配逐条核实（V2 复核结论，V3 维持）：`NO_EVENTSUB` 有语义表达（`block.rs:36-45` 编码为 `None`）；`SEND_PRIORITY`/`SEND_TIME_SLICE` 真缺口（挂 E7）；`ESCRIPT` C 死代码正确缺失；`EXTERN`/`_SYSTEM`/`_TABLE` C 编译宏无需对应。

### 1.3 Minix3 易漏语义点抽查（第 1 轮 11 项）

fork 的 LAST_FEW 非 root 预留、next_child 轮转槽位、disinherit 重挂 INIT、TO_TRACEFORK 条件继承、exec 的 TAINTED 双重判定、`mp_sigmask2` 保存掩码、NR_PIDS=30000 轮转、itimer 三族、nice 与优先级队列双向换算——逐项锚点见 2026-09-08 压缩前版本（git 历史），V2 勘误与修复状态见 §6 D-27/D-28 与 Fix #29-#32。

---

## 2. P0：真实 bug（三轮均未发现可达路径上的 P0）

V2 轮的 V2-P0-1（mock 泄漏）与 V2-P0-2（位基分裂）曾按 P0 登记，均已修复（Fix #29/#30）。V3 轮未发现新的可达路径 P0——本轮发现的 trace 域失真、信号链缺口、假数据 stub 全部位于未接线调用（ENOSYS 兜底）或未通电边界之后，按 P1 登记。

---

## 3. P1：架构级问题（第 1 轮遗留）

### P1-1 分发收敛到单一分发表（✅ 已修复 2026-09-06，Fix #2）

### P1-2 服务间请求"假成功"接缝（✅ 已修复 2026-09-06，Fix #1；同族第 2/3 例分别由 Fix #30 与 §12 V3-P1-4 承接）

### P1-3 内核边界整体未实现（最大依赖簇，跨阶段）

`KernelIpcTransport` 三方法 `unimplemented!()`（`ipc/transport.rs:86/91/96`）、`BootParams::placeholder()`（`main.rs:15`）。trap 层挂 edge E1；SYS_* wrapper 挂 E2/E6。本条 stage 内只保留 transport 实现与最小内核面倒推。**V3 增补**：主循环的内核信号入口缺失是本簇的新成员（§12 V3-P1-2），不在原 E6 清单内。

### P1-4 codec 层缺口（ARCH A-4，跨阶段主体挂 E7）

47 个调用只有 `MessPmLcWait4` 一个 wire 成员（Fix #22 落地）。V3 增补阶段内过渡方案（unsafe 解码集中化）见 §12.3 观察 2。

### P1-5 测试分发路径（✅ 已修复 2026-09-06，Fix #3）

### P1-6 入口命名统一（✅ 已修复 2026-09-06，Fix #4）

---

## 4. P2：结构性改进（第 1 轮遗留）

### P2-1 cfg 特性门（✅ 已修复，Fix #17）
### P2-2 plan.md ARCH 表同步（✅ 已修复，Fix #20）
### P2-3 `minix-types` 死代码 + 调用号双址（开放，本体挂 E7）

V3 增补一处同族实例：`dispatcher.rs:32` 本地定义 `PROC_EVENT_REPLY: i32 = 0xE80`，而 `minix-types` 已有同值常量（`ipc/event.rs:27`，带常量锁定测试）——已并入 E7 清单（§12 V3-P3-5）。

### P2-4 glob re-export（✅ 已修复，Fix #18）
### P2-5 stub 注释契约化（✅ 已修复，Fix #19；V3 发现一处漏网：`ipc/vfs.rs:533-540`，见 V3-P2-1）
### P2-6 文档 00/99 最小骨架 + `.design/` 快照缺失（开放，维持 plan.md §6.2 排序）
### P2-7 ESCRIPT 排除登记（✅ 已修复，Fix #20）

---

## 5. P3：代码卫生（第 1 轮）

### P3-1 clippy 清理（✅ Fix #21；**V3 复测回退 3 条**，见 V3-P2-9）
### P3-2 无用导入与无用参数（✅ Fix #2/#21）

---

## 6. D-XX：DEFERRED / stub / unimplemented 全量登记

> 2026-09-06 全量收敛 + 2026-09-08 V2 复核。**V3 复核（2026-09-09）新发现一处漏网**：`ipc/vfs.rs:533-540` 的 `restart_signals` no-op（带 "DEFERRED 脚手架" 字样但无编号、前提失真）——见 V3-P2-1，暂以 V3 编号跟踪，实施时若保持 DEFERRED 形态则登记 D-29。V2-P2-8 曾预占 D-29（sig_send），该条目随批次 B 实施时按当时的实际形态取号，两处取号顺序以实施先后为准。

| ID | 位置 | 内容 | 归属 | 解除条件 |
|----|------|------|------|----------|
| D-01 | `ipc/transport.rs:86/91/96` | KernelIpcTransport receive/send/sendrec `unimplemented!()` | 01-pm-init-main.md:382 | minix-sys trap 层（E1） |
| D-02 | `main.rs:15` | `BootParams::placeholder()` 启动参数占位 | 01-pm-init-main.md | SYS_GETMONPARAMS/SYS_GETIMAGE（E6，双侧新建） |
| D-03 | ✅ 已修复（Fix #1，2026-09-06）：真实 `vm_fork` sendrec | | | |
| D-04 | ✅ 已修复（Fix #1）：`send_kernel_request` 判定为与 C 不符的原型残留，删除 | | | |
| D-05 | `ipc/vfs.rs:453` | `sched_start_user` 非 KERNEL/NONE 调度器 `unimplemented!()` | 16-scheduling.md | A-8（SCHED 客户端）；**V3 增补：其逻辑层 `sched.rs:200-212` 有调度器端点硬编码偏差，见 V3-P2-3** |
| D-06 | ✅ 已修复（Fix #8，2026-09-06） | | | |
| D-07 | ✅ 已修复（Fix #9，2026-09-06） | | | |
| D-08 | ✅ 已修复（Fix #10，2026-09-06） | | | |
| D-09 | ✅ 已修复（Fix #25，2026-09-07） | | | |
| D-10 | ✅ 已修复（Fix #5，2026-09-06）；**V3 勘误：sig_proc 的 trace 分支（signal.rs:229-239）仍是置位 stub，Fix #5 的"真实 sig_proc"声明对该分支过度声称——见 V3-P1-1** | | | |
| D-11 | ✅ 已修复（Fix #7，2026-09-06） | | | |
| D-12 | `init.rs:654-656` | minix_sched 客户端占位（sched_start 假 endpoint） | 16-scheduling.md | A-8 |
| D-13 | ✅ 已修复（Fix #23，2026-09-06；范围仅 do_exit 的 PRIV_PROC 违规分支） | | | |
| D-14 | ✅ 已修复（Fix #26，2026-09-07） | | | |
| D-15 | ✅ 已修复（Fix #11，2026-09-06） | | | |
| D-16 | `exit.rs:261/265` | core dump 路径名指针为 0——依赖未解除（VFS 契约重设计 + minix-types wire 成员） | 09-pm-exit.md | 契约决策 + E7 |
| D-17 | `exit.rs:323` | `sched_stop` 假装 Ok——SCHED 服务器（16-stage）尚不存在 | 16-scheduling.md | A-8 |
| D-18 | ✅ 已修复（Fix #24，2026-09-07） | | | |
| D-19 | ✅ 已修复（Fix #12，2026-09-06） | | | |
| D-20 | ✅ 已修复（Fix #6，2026-09-06） | | | |
| D-21 | ✅ 已修复（Fix #27，2026-09-07 实施 / 09-08 补记） | | | |
| D-22 | ✅ 已修复（Fix #13，2026-09-06） | | | |
| D-23 | ✅ 已修复（Fix #14，2026-09-06） | | | |
| D-24 | ✅ 已修复（Fix #15，2026-09-06） | | | |
| D-25 | ✅ 已修复（Fix #28，2026-09-07） | | | |
| D-26 | ✅ 已修复（Fix #22，2026-09-06） | | | |
| D-27 | ✅ 已修复（Fix #32，2026-09-08）：SIGHUP 会话组广播 | | | |
| D-28 | ✅ 已修复（Fix #34，2026-09-08）：check_parent 的 SIGCHLD 投递 | | | |
| D-29 | `misc.rs` do_getsysinfo 数据路径 | **2026-09-09 登记（Fix #46）**：权限门与 size 校验真实，数据拷出 fail-closed 返回 ENOSYS（V3-P1-4：旧代码拷 len 个零字节假数据，无契约）。真实数据路径 = PM 表的 C-ABI 序列化镜像 wire + 批次 G 接线 | 20-misc-queries.md | C-ABI 表镜像 wire 成员（edge E7）+ 批次 G |

不属于 DEFERRED 但同源（plan.md §4 登记）：A-7 定时器抽象（部分收敛，Fix #39）、A-10 内核延迟调用 DELAY_CALL/SIGSNDELAY、A-13 进程组/会话设计层。

---

## 7. 对照 Redox 的架构参考（第 1 轮 5 条 + V2 轮 7 条）

第 1 轮 §7 的 5 条（进程管理位置 / 状态建模 / 信号决策边界 / 注册表 vs 穷尽 match / 异步往返记账）与 V2 轮 §11.3 的 7 条（V2-Redox-1 用户态迁移事实、僵尸记账、孤儿语义、PID 轮转、信号终态形态、fork 协议、事件循环惯例）继续有效，正文保留在 2026-09-08 压缩前版本与本文件 §11.3。V3 轮增补见 §12.4。

---

## 8. 建议的推进顺序

第 1 轮顺序已执行完毕（Fix #1-#28）；第 2 轮见 §11.6；**第 3 轮见 §12.5**。

## 9. 跨阶段条目抽取索引（edge_todo.md）

> **判定规则**：① 共享契约/基础设施层（minix-types、minix-sys）的缺陷与重构；② 对方 stage 目录里的生产代码；③ 多进程联调测试。stage 内生产代码（消费既有稳定契约，含 seam + mock 测试）不属于 edge。
> **通电口径**：依赖共享 trap 层/系统调用面的条目，PM 侧逻辑完备 + mock 测试即标 ✅，真实通电挂对应 edge 条目。

### 9.1 P/D 条目 → edge 条目映射（要点）

- P1-3 内核边界 → E1（trap 层）+ E2/E6（SYS_* wrapper）
- P1-4 codec 层 → E7（wire 结构体系统化；stage 内只做消费端接线）
- P2-3 死代码与调用号双址 → E7
- D-02 → E6；D-15/D-19 真实往返 → E5(a)；D-05/D-12/D-17 → A-8/06-stage；D-16 → E7

### 9.2 V3 轮（§12）跨阶段抽取

| V3 条目 | stage 内部分 | 跨阶段部分 |
|---|---|---|
| V3-P1-2 内核信号入口 | notify 分支 + process_ksig 驱动设计 | 内核 ksig 对端现状核实 + wrapper → **E6 清单增补** |
| V3-P1-4 getsysinfo 假数据 | 真实拷出路径（SysInfoCtl 扩展） | 若需 wire 成员 → E7 |
| V3-P2-6 错误码折叠 | 3 个错误枚举加透传通道 | 无 |
| V3-P3-5 PROC_EVENT_REPLY 双址 | —（本体在共享层消费侧） | **E7 清单增补** |

其余 V3 条目均为 stage 内工作。

---

## 10. 修复记录（Fix #1–#42 索引）

> 每轮一个 TODO（todo-fix 工作流）。逐条修复的完整论证（设计选型多方案对比、C 逐点语义对照、修复前后 grep 证据、文档同步清单、DEFERRED 论证）见 git 提交历史：第 1 轮 Fix #1–#28 对应提交系列 `fix(pm)`（2026-09-06/07，含 de9415604、d79307ee6），第 2 轮 Fix #29–#42 对应提交系列 `fix(pm)`（2026-09-08，8ec64f98f…14a07ccc6）。本表只保留账目索引。

| Fix | 条目 | 日期 | 一句话 |
|-----|------|------|--------|
| #1 | P1-2 + D-03/D-04 | 09-06 | VM fork 假成功 → 真实 sendrec fail-closed |
| #2 | P1-1 | 09-06 | 7 个内联拦截收编 `dispatch_pm_call` 单一穷尽 match |
| #3 | P1-5 | 09-06 | crate 外 `tests/run_once_integration.rs` 端到端层 |
| #4 | P1-6 | 09-06 | `handle_*` 统一回 C 名 `do_*` |
| #5 | D-10 | 09-06 | tracer SIGSTOP + TO_TRACEFORK 条件继承（`inherit_guardianship`） |
| #6 | D-20 | 09-06 | wait4 TRACE_STOPPED 环真实语义（扫描/消费/载荷） |
| #7 | D-11 | 09-06 | 事件终止分派 Signal 分支接真实 `restart_sigs`（supertrait 合并） |
| #8 | D-06 | 09-06 | VFS 端口 `exit_proc` 委托 09 退出链 |
| #9 | D-07 | 09-06 | WCOREFLAG 置 bit7 + wait4 无符号字节组合 |
| #10 | D-08 | 09-06 | `exec_restart` 端口接 17 全语义（`ExecRestartServices`） |
| #11 | D-15 | 09-06 | `vm_willexit` 真实化（失败 panic 对齐 C） |
| #12 | D-19 | 09-06 | `vm_exit` 真实化 |
| #13 | D-22 | 09-06 | `is_lethal`/`is_stacktrace`/`is_termination` 按真 C 谓词宏重写 |
| #14 | D-23 | 09-06 | `process_ksig` 接真实 `check_vtimer`（连带修 signo==12 应为 26 的真 bug） |
| #15 | D-24 | 09-06 | `getticks` 桩删除，`started` 显式注入（ClockSource） |
| #16 | D-16 论证升级 | 09-06 | 全部余下 DEFERRED 行自包含化（依赖未解除论证） |
| #17 | P2-1 | 09-06 | cfg 特性声明恢复 |
| #18 | P2-4 | 09-06 | glob re-export 移除 |
| #19 | P2-5 | 09-06 | stub 注释统一 `[DEFERRED: D-XX]` 契约格式 |
| #20 | P2-2/P2-7 | 09-06 | plan.md ARCH 表刷新 + ESCRIPT 排除登记 |
| #21 | P3-1/P3-2 | 09-06 | clippy 约 50 告警 + 3 error 收敛到 0 |
| #22 | D-26 | 09-06 | wait4 回复载荷 wire（`MessPmLcWait4`，E7 首切片） |
| #23 | D-13 | 09-06 | PRIV_PROC 违规分支真实 `sys_kill`（`KernelGateway` 诞生，E6 切片） |
| #24 | D-18 | 09-07 | `sys_clear` 两调用点接真实内核通道 |
| #25 | D-09 | 09-07 | `sys_abort` 端口接通（REBOOT 特例语义保留） |
| #26 | D-14 | 09-07 | exit 时 `sys_times` 计账（kern 沿信号链下穿） |
| #27 | D-21 | 09-07 | rusage 经 VIRCOPY 真实投递父进程（144 字节 + hz 换算） |
| #28 | D-25 | 09-07 | `sys_resume` 经 SYS_RUNCTL 真实现 |
| #29 | V2-P0-2 | 09-08 | 信号集合位基统一到 C `__sigmask` + badignore 谓词修正 |
| #30 | V2-P0-1 | 09-08 | 信号终止链贯通真实 transport（消灭 mock 泄漏） |
| #31 | V2-P1-1 | 09-08 | PID 轮转相位对齐 C（先自增再返回） |
| #32 | V2-P1-2 / D-27 | 09-08 | SIGHUP 会话组广播实现 |
| #33 | V2-P2-4 | 09-08 | 广播 SIGTERM 的 RS 通知改走 sys_kill 内核回环 |
| #34 | V2-P2-5 / D-28 | 09-08 | check_parent 父未等待时投递 SIGCHLD |
| #35 | V2-P2-3 | 09-08 | tell_vfs 错误路径 fail-closed（内部 panic 对齐 C） |
| #36 | V2-P3-3 | 09-08 | Exit 臂透传 do_exit 回复意图 |
| #37 | V2-P3-2 | 09-08 | GID_MAX 校验恢复 2^31-1 真值 |
| #38 | V2-P2-2 | 09-08 | `do_exec` 补 VFS/RS 调用者门 |
| #39 | V2-P2-1 | 09-08 | itimer CLOCK notify 接线 + 周期重挂收敛到 `cause_sigalrm` |
| #40 | V2-P2-6 | 09-08 | ENOSYS 兜底臂注释指向接线批次表 |
| #41 | V2-P3-1 | 09-08 | 四处 stale 注释与死绑定清理 |
| #42 | V2-P3-4(c) | 09-08 | exec 重置对齐 C 不清 sa_flags（campaign 收官） |

---

## 11. 第 2 轮全量查漏补缺 + 架构审查（V2，2026-09-08）

> 基线：Fix #22-#28 后的 346 lib + 7 integration。方法：Gate A 重生成 + 47 调用矩阵复核 + 6 个 C 文件逐函数对账（utility/getset/time/mcontext/profile/alarm）+ 24 处 DEFERRED 收敛 + 语义抽查第二批 10 项 + 分层架构审查 + Redox 联网调研。

### 11.0 Gate 证据（V2）

coverage 109/109 文档覆盖、102 Rust 名称匹配（93.6%）；测试名对账全过（当时口径：todo.md 引用的测试函数全部 grep 命中）。**V3 注：文档 §5 全量测试名对账是 V3 才做的（V3-P1-5），V2 的对账范围只含 todo.md 引用。**

### 11.1 查漏补缺总表 V2

#### 11.1.1 47 调用号矩阵复核 + 40 臂接线批次表（活动台账，每接线一批同步划账）

| 批次 | 调用号 | C handler | Rust 逻辑位置 | 前置条件 |
|------|--------|-----------|--------------|----------|
| A 凭证（13 个） | 4,5,6,9,10,12,13,15,16,29,30,31,32 | do_get/do_set（getset.c） | `credentials.rs:87/148` | wire 类型（`m_lc_pm_getuid` 族）；`CopyGroups` 生产实现（minix-sys `sys_datacopy`，E6）；`VfsForwarder` 生产实现。**V3 增补：do_getepinfo 两处语义偏差先修（V3-P2-4）** |
| B 信号控制（6 个） | 8,20,21,22,23,24 | do_trace/do_sigaction/do_sigsuspend/do_sigpending/do_sigprocmask/do_sigreturn | `trace.rs`/`signal_handlers.rs` | trace 半边**已完成**（V3-P1-1：常量 + 全分支 + PTRACE 臂接线 + sys_trace wrapper）；余 sigaction 族：wire 类型、`sys_sigreturn` wrapper（E6）、V2-P2-7/8 联动 |
| C 时间（6 个） | 7,28,33,34,35,36 | do_stime/do_time/do_getres/do_gettime/do_settime/do_getrusage | `time.rs`/`misc.rs` | ClockSource 生产实现（GETUPTIME，E6）；`sys_settime`/`sys_stime`（E6）；rusage 拷出未接线见 V3-P2-6 前置 |
| D 定时器（1 个） | 17 | do_itimer | `timer.rs:368` | `TimerCtl`/`VTimerCtl` 生产实现（E6）；`sys_datacopy`；CLOCK notify 时间戳来源修正（V3-P2-10） |
| E exec（3 个） | 14,43,44 | do_exec/do_newexec/do_execrestart | `exec.rs` | caller 门已补（Fix #38）；wire 类型；`sys_exec` wrapper 与 kernel 对端（D-08 注）；D-16 契约（E7） |
| F 调度（2 个） | 26,27 | do_getsetpriority | `sched.rs:247` | SCHED 客户端（D-12/A-8）；`SEND_PRIORITY`/`SEND_TIME_SLICE`（E7）；**V3 增补：`sched_start_user` 端点语义先修（V3-P2-3）** |
| G 杂项（9 个） | 18,19,25,37,38,39,45,46,47 | do_[gs]etmcontext/do_sysuname/do_reboot/do_svrctl/do_sprofile/do_getepinfo/do_getprocnr/do_getsysinfo | `misc.rs` | wire 类型；`sys_[gs]etmcontext`/`sys_sprof`（E6）；**V3 增补：getsysinfo 假数据（V3-P1-4）、svrctl 分支细节（V3-P2-5）、sysuname req 分支必须在接线前修** |
| **H 内核信号入口（V3 新增，✅ 已完成）** | —（通知面，非调用号） | process_ksig 经 SIGKSIG（main.c:121 + sef_signal.c:104-108） | `signal.rs::process_sigmgr_signals` | V3-P1-2 闭环（Fix #45）：SYSTEM notify 触发 + getksig/endksig wrapper（E6 切片）+ SIGSNDELAY/SIGKSIG 常量修复；SigSet(u64) 128 位拓宽挂 edge E6 |

#### 11.1.2 C 函数面第二轮对账（utility/getset/time/mcontext/profile/alarm 逐函数）

V2 结论保留在 git 历史（2026-09-08 压缩前版本）。两条产出：V2-P1-1（PID 相位，已修）、V2-P3-2（GID_MAX，已修）。**V3 补充**：该轮未覆盖的 trace/event/misc/schedule/main 五文件对账见 §12.1。

#### 11.1.3 DEFERRED / stub 全量收敛（V2 时点 24 处）

V2 结论保留在 git 历史。**V3 勘误**：该轮收敛漏掉 `ipc/vfs.rs:533-540` 的 `restart_signals` no-op（注释含 "DEFERRED 脚手架" 字样但无编号）——见 V3-P2-1。另 Fix #29 的验证声明"signal.rs 本文件不再出现裸移位"与现状不符（signal.rs:233/242/244/289-298 仍有 7 处裸移位，位基正确但未走 `init::sig_bit` 单点）——见 V3-P3-4。

#### 11.1.4 Minix3 易漏语义抽查第二批（10 项）

判定四色（covered/gap/deferred/N.A.），全表见 git 历史。要点：kill 广播四分支 covered（RS 站点 gap 已修）、setsid covered、wait4 只测 WNOHANG covered、exec 信号重置 covered（超集已修）、sigmask2 闭环 covered、alarm 跨 fork/exec/exit covered、core dump 触发链 gap（已修）、getsysinfo/reboot superuser 门 covered、SIGHUP 广播 gap（已修）、PAUSE N.A.。

### 11.2 V2 条目

已闭环（索引）：V2-P0-1（✅ Fix #30）、V2-P0-2（✅ Fix #29）、V2-P1-1（✅ Fix #31）、V2-P1-2（✅ Fix #32）、V2-P2-1（✅ Fix #39）、V2-P2-2（✅ Fix #38）、V2-P2-3（✅ Fix #35）、V2-P2-4（✅ Fix #33）、V2-P2-5（✅ Fix #34）、V2-P2-6（✅ Fix #40）、V2-P3-1（✅ Fix #41）、V2-P3-2（✅ Fix #37）、V2-P3-3（✅ Fix #36）、V2-P3-4（(c) ✅ Fix #42；(a) 挂 E6；(b) 扩展为 V3-P2-5）。

仍开放：

#### V2-P2-7 unpause 三路径真实化（✅ 已修复 2026-09-09，Fix #49）

#### V2-P2-8 sig_send 空壳（✅ 已修复 2026-09-09，Fix #49，与 V2-P2-7 同轮——二者耦合即 V2 所荐"三步同轮"）

- **原状态**：`sig_send` 是 `Ok(())` 空壳（signal.c:772-855 全缺失）；本地 `unpause` 对运行中进程直接返回 true 且不置 stopped（VFS_CALL 分支不发 VFS_PM_UNPAUSE）。caught 路径（sigaction 接线后可达）的投递核心整体缺失，且 sig_send 的 PROC_STOPPED 断言依赖 unpause 先停——两缺陷互为因果，故同轮落地。
- **修复**（Fix #49）：(1) `sig_send` 按 C 全语义实现——`prepare_sigmsg`（D7）之外补 C 800-808 的 `mp_sigmask` 簿记（suspended 时 sm_mask 以 mask2 起底、mp_sigmask 以 mask 起底，两值不同，prepare_sigmsg 同轮补此簿记）、`sys_sigsend` 投递、EFAULT/ENOMEM 合法失败分档（FALSE → 终止兜底）、WAITING/SIGSUSPENDED 的 EINTR 打断 + try_resume_proc（GatewayResumeBridge）。(2) `unpause` 按 C 三路径真实化（UNPAUSED 就绪 / DELAY_CALL 忙 / WAITING|SIGSUSPENDED 停住即就绪 / 其余 stop_proc(MayDefer) + VFS_PM_UNPAUSE 请求）——设计选型：内联于 sig_proc（sig_proc 已持 table/transport/kern）而非接 `signal_flow::unpause` 的 VfsCtl seam（该 seam 拿不到 table，借用不可达）。(3) minix-sys（E6 切片）：`SigMsgWire`（type.h:71-77，40 字节 repr(C)）+ `sys_sigsend`（SIGSEND=9，sigctx 指针语义）+ wire 测试。
- **验证**：+4 单测（sigmsg 组装与 mask 簿记 / EFAULT 优雅失败 / WAITING 停住就绪 / 运行中 defer 到 VFS）；minix-sys wire 测试。基线 **377 lib + 11 integration passed**；批次 B 余项（sigaction 族 wire + sys_sigreturn）保持登记。

### 11.3 对照 Redox 的架构参考（V2，7 条）

V2-Redox-1（用户态迁移 `1d5f8fd46d71`）、V2-Redox-2（僵尸记账 WaitMap）、V2-Redox-3（孤儿跟 C 不跟 Redox）、V2-Redox-4（PID 轮转必须保真）、V2-Redox-5（信号共享页 + trampoline 终态）、V2-Redox-6（fork 是协议）、V2-Redox-7（事件循环惯例 redox-scheme/Tock）——正文见 2026-09-08 压缩前版本（git 历史），结论继续有效。V3 增补见 §12.4。

### 11.4 模块级观察（V2 分层审查结论）

- **L0 内核接入 seam**：约 25 个端口 trait 两种风格并存（中央 KernelGateway vs 接口隔离窄 trait）。判定：不收敛为单一 trait，但需四列对照表（内核能力 ↔ trait ↔ minix-sys wrapper ↔ C libsys）进 plan.md ARCH 附录。**V3 更新**：trait 总数已达 30（V3 实测清单见 §12.3 观察 1），四列对照表仍未落地；`CopyToUser`（misc.rs:135）与 `KernelGateway::copy_to_user`（exit.rs:50）被确认是同一关切的双端口。
- **L1 服务器骨架**：`run_once` 与 C 主循环逐段对应（V3 的 main.c 对账确认：主循环判定顺序、setreply/reply 载荷语义、REBOOT 特例、EXITING 丢弃全部 covered；有意偏差均有文档记录）。
- **L2 分发层**：enum 穷尽 match + 兜底臂结构维持；兜底臂已指回批次表（Fix #40）。
- **L3 子系统层**：exit→wait 僵尸链、exec 三方握手、signal 骨架 ✓。**V3 增补：sig_proc 内部两处分支缺口（V3-P1-3 / V3-P2-2）与 trace 域失真（V3-P1-1）修正"L3 骨架 ✓"的覆盖面。**
- **L4 状态层**：mproc 38 字段 + 19 flag 位映射可追溯；pid_gen 活进程扫描偏差已文档化。
- **L5 VFS 协议层**：11 路回复与 C 逐路对应。**V3 增补：尾部 `restart_signals` 端口是 no-op（V3-P2-1）；`publish_event` 裸指针借用拆分可免（V3-P2-7）。**
- **L6 测试层**：接线分支集成测试义务随批次表逐批履行；"测试全绿但行为错误"教训（V2 两 P0 的直接证据是 mock 掩盖）。**V3 增补：`test_constants_match_c` 只断言 18 个常量中恰好正确的 2 个——"测试名声称对账 C 但断言子集规避"是同一教训的常量类变体（§12.5 Rule Discovery）。**

### 11.5 Rule Discovery（V2）

TSTL（测试 seam 泄漏进生产路径）与 SBCD（位集合 producer/consumer 位基漂移）两个候选模式，目标文件 `prompt/skill/review-patterns-skill.md`，登记状态待规则集维护轮确认。V3 追加候选见 §12.5。

### 11.6 建议的推进顺序（V2）

原 10 步顺序（V2-P0-2 → … → 批次 F）执行到第 5 步后与 V3 轮合并——**以 §12.5 的 V3 顺序为准**（它吸收了 V2 遗留的批次 A-G 与 V2-P2-7/8，并插入 V3 新条目）。

---

## 12. 第 3 轮全量查漏补缺 + 架构审查（V3，2026-09-09）

> 来源：cmd-04 模式第 3 轮（查漏补缺 + 分层架构审查）。前两轮已覆盖的 C 文件不重复对账；本轮对第 2 轮未做逐函数对账的五个 C 文件（trace.c 276 行 / event.c 353 行 / misc.c 447 行 / schedule.c 112 行 / main.c 424 行）逐函数对账，外加 Gate E 文档 §5 全量测试名对账（21 篇文档，第 1、2 轮只做过 todo.md 引用级对账）。
> 方法与证据：coverage-extract 重跑（109 符号 / 文档 100% / Rust 名称匹配 93.6%，7 个未匹配结论与前两轮一致无新增真缺口）；`cargo test -p minix-pm` 356 lib + 8 integration passed；五个对账子任务由独立审查代理执行，P1 级以上结论全部经本会话二次 grep/读源验证后才登记（其中一条——`sched.rs` 特权门旁路——经二次核实被**降级**为设计观察，见 §12.3 观察 6）。

### 12.0 Gate 证据（V3）

```bash
python3 tools/coverage-extract/coverage-extract.py pm \
  notes/rewrite/fork-syscall-rewrite/04-stage-pm \
  --rust-dir os --c-dir minix3/minix/servers/pm \
  --semantic-map tools/coverage-extract/pm-semantic-map.json \
  --output .review/zcode/fork-syscall-rewrite/04-stage-pm-arch/SYMBOLS-v3.md
# Total C symbols: 109; Doc covered: 109 (100.0%); Rust covered (name-match): 102 (93.6%)

cd os && cargo test -p minix-pm
# test result: ok. 356 passed; 0 failed  +  8 integration passed

cargo clippy -p minix-pm --lib
# minix-pm (lib): 3 warnings（ipc/vfs.rs:28/31 与 timer.rs:7 的 unused import——V2 收官时为 0，回退见 V3-P2-9）
# minix-sys (lib): 5 warnings（非本轮范围，含 syscall.rs:308 新增 1 条 empty-line-after-doc）
cargo test -p minix-pm --no-run 2>&1 | grep 'unused variable'  # 22 处，全部位于 #[cfg(test)] 代码（V3-P2-9）
```

### 12.1 查漏补缺总表 V3：第 3 批 C 文件逐函数对账

#### 12.1.1 trace.c ↔ trace.rs（结论：**逻辑未备**，修正第 1 轮"逻辑已备"判断）

| C 函数/分支 | 判定 | C 锚点 | Rust 锚点 | 说明 |
|---|---|---|---|---|
| T_* 常量全集 | **ABI 错误 15/18** | `sys/sys/ptrace.h:226-250` | `trace.rs:12-28` | T_STOP 应为 -1（Rust 6，且与 T_SETDATA 同值冲突）、T_DETACH 应为 10（Rust 11）、T_RESUME=PT_CONTINUE=7（Rust 14）、T_STEP=104（Rust 15）、T_SYSCALL=14（Rust 16）、T_SETOPT=105/T_GETRANGE=106/T_SETRANGE=107（Rust 20/21/22）、T_GETINS=1/T_GETDATA=2/T_SETINS=4/T_SETDATA=5/T_GETUSER=102/T_SETUSER=103 全部不符；仅 T_OK=0/T_ATTACH=9/T_EXIT=8 正确 |
| `test_constants_match_c` | **测试自身失真** | — | `trace.rs:398-401` | 测试名声称对账 C，实际只断言恰好正确的 2 个常量（T_OK/T_ATTACH）——16 个错误值零覆盖 |
| T_READB_INS / T_WRITEB_INS | **整支缺失** | trace.c:101-134 | 无 | root 专属（root 门在通用守卫 trace.c:140-143 **之前**）；Rust 请求 100/101 会落入 `_` 臂被要求 tracer+TRACE_STOPPED——与 C 相反 |
| T_GETRANGE / T_SETRANGE | **整支缺失** | trace.c:167-188 | 无 | ptrace_range 的 datacopy 提取、TS_INS/TS_DATA 校验、pr_size 上界、vircopy 双向全缺；`TraceCtl::vircopy/datacopy`（trace.rs:60-61）是从未调用的死方法 |
| do_trace 接线 | 缺失（批次 B 已知） | callnr.h PM_PTRACE=8 | `calls.rs:313` 兜底臂 | `do_trace`/`trace_stop` 全仓无生产调用者 |
| T_OK / T_ATTACH | partial | trace.c:56-90 | trace.rs:81-135 | 权限链 covered；**T_ATTACH 缺 `TO_NOEXEC` 置位与 `sig_proc(SIGSTOP)` 真实调用**（trace.rs:129-133，注释自认 "for test just set sigtrace"）——attach 后目标不停、tracer 收不到 W_STOPCODE；且未用文档 D1 承诺的 `try_set_tracer`（`guardianship.rs:121` 存在但代码内联构造） |
| T_EXIT | partial | trace.c:147-159 | trace.rs:137-160 | TRACE_EXIT 置位 covered（但只更新 `exit_pending`，不更新 `trace_exit`——双份状态见下）；else 分支应调 `exit_proc`（trace.c:153-154）实为与 VFS 分支相同的仅置 Exiting |
| T_DETACH | **多处失真** | trace.c:191-215 | trace.rs:175-208 | ① `trace_flags = 0` 是 no-op bug：trace.rs:206 在 191 行已把监护转 `Normal` 后调用 `set_trace_options(0)`，而该方法只匹配 `Traced`（`guardianship.rs:112-115`）→ 清零永不生效；② sigtrace 重放/data 信号/check_pending 全部"置 pending 位代替调用"（trace.rs:196-207 注释自认 for test；`check_sig`/`sig_proc`/`check_pending` 都已存在可调）；③ C 在 215 行落穿到 244-249 的内核 sys_trace 透传（内核侧完成 detach），Rust 直接 Reply(OK) |
| T_RESUME/STEP/SYSCALL | partial | trace.c:220-249 | trace.rs:210-241 | data>0 信号置位代替 sig_proc（:225-227）；check_pending 仅注释（:233）；sys_trace 透传存在但内核错误折叠为 EINVAL、READ 类读值无载荷可回（C trace.c:246/248 透传 r + `reply.data = data`） |
| trace_stop | **死代码 + 载荷契约违反** | trace.c:256-276 | trace.rs:265-295 | 无生产调用者（grep 全仓仅测试）；reply 只存 `ipc.reply` intent 不发送（trace.rs:293 注释自认）；W_STOPCODE 放 `m_type` 而非 wait4 status 载荷——违反 Fix #22（D-26）确立的"tag(m_type) + typed body(载荷)"契约 |
| sig_proc 的 TRACE 分支 | **置位 stub** | signal.c:411-421（`sigaddset(sigtrace)` + `trace_stop`） | `signal.rs:229-239` | 置 trace_mask 位 + 直接 `stopped = true`——无 trace_stop 调用、无内核 sys_trace 停止、无 tracer 回复。**V2 Fix #5 的"真实 sig_proc"声明对本分支过度声称** |
| TRACE_EXIT 双份状态 | 结构缺陷 | mproc.h:100（一个位） | `mproc/trace.rs:18`（exit_pending）+ `guardianship.rs:38`（trace_exit） | 同一 C 位由两处独立 bool 表达，do_trace 只更新前者，可分歧 |
| 文档 18 | 失同步 | — | `18-trace.md` | 全文零 DEFERRED 标记但代码是 stub 态；§2.14 TO_NOEXEC=0x1 与 C 0x4（ptrace.h:211）矛盾（代码 guardianship.rs:62-63 与 C 一致，文档错）；D1 承诺的 try_set_tracer/TraceDetach::replay_sigtrace/WaitCode::stop 抽象代码未消费 |

**结论**：18 号（ptrace）的真实状态是"部分 stub + 常量 ABI 错误 + 测试名谎报"，不是"逻辑已备、等 wire"。接线批次 B 的前置条件必须加"V3-P1-1 重构"。

#### 12.1.2 event.c ↔ event.rs（结论：覆盖完整，1 个 pub 接口 bug + 2 处弱化）

5 个 C 函数（resume_event/remove_sub/do_proceventmask/do_proc_event_reply/publish_event）全部 covered 或 partial-with-anchor（逐函数锚点表见对账报告要点：`event.c:74-123 ↔ event.rs:215-281`、`event.c:130-161 ↔ event.rs:289-322`、`event.c:170-211 ↔ event.rs:331-404/410-450`、`event.c:218-309 ↔ event.rs:459-557`、`event.c:316-353 ↔ event.rs:164-206`）。事件类型全集即 `PROC_EVENT_EXIT`/`PROC_EVENT_SIGNAL` 两种（syslib.h:292-293），无缺失。缺口归入 V3-P2-8（非 mut 变体游标 bug）与 V3-P3（waiting 上界弱化、诊断缺失、mask 截断语义、文档 06:522 表述错误）。

#### 12.1.3 misc.c / schedule.c ↔ misc.rs / sched.rs（结论：主体 covered，6 处真缺口）

| C 函数 | 判定 | 锚点 | 说明 |
|---|---|---|---|
| do_getsysinfo | **数据路径假实现** | misc.c:142-143 ↔ `misc.rs:333-336` | 权限门与 size 校验真实，但拷出内容恒为 len 个零字节（`let _ = src; cpy.copy_to_user(&vec![0u8; len], …)`，注释自认 "In tests, we copy dummy bytes"——生产代码、无 DEFERRED 标记）→ V3-P1-4 |
| do_getepinfo | partial | misc.c:184-190 ↔ `misc.rs:362-378` | ngroups 语义（全量 vs 截断）+ groups 拷出未接线 → V3-P2-4 |
| do_svrctl | partial | misc.c:307-395 ↔ `misc.rs:404-428, 213-218` | 四分支缺失 + 检查顺序相反 → V3-P2-5（扩展 V2-P3-4(b)） |
| do_sysuname | partial | misc.c:72-100 ↔ `misc.rs:275-304` | req 方向分支缺失（req!=0→EINVAL 无处产生）+ 用户目的地址 placeholder（EFAULT 无载体）→ V3-P2-5 同批 |
| do_getrusage | partial | misc.c:429-446 ↔ `misc.rs:442-463` | rusage 结构拷出未接线（`_cpy` 未用）+ 底层错误折叠 EINVAL（见 V3-P2-6） |
| do_getsetpriority | covered（含一处待核实模式） | misc.c:239-286 ↔ `sched.rs:247-277` | `unwrap_or_default()` 凭据模式见 §12.3 观察 6（不构成对 C 的偏差） |
| sched_init | covered | schedule.c:20-50 ↔ `sched.rs:171-197` | 两个 C assert 仅注释（sched.rs:181-185）；失败日志 cfg(test) |
| sched_start_user | **语义偏差** | schedule.c:55-84 + main.c:371-373 ↔ `sched.rs:200-212` | C 继承父进程的 `mp_scheduler`（调用点守卫 KERNEL/NONE），Rust 硬编码 `SCHED_PROC_NR` 且无守卫 → V3-P2-3 |
| sched_nice / nice_to_priority / get_nice_value | covered | schedule.c:89-112 / utility.c:91-103 / main.c:275-289 ↔ `sched.rs:215-228/280-287` | 公式逐项一致（含量化误差测试） |

#### 12.1.4 main.c ↔ init.rs（结论：骨架 covered，1 个结构性缺口 + 4 处小项）

C main.c 实函数清单：main(48-109)/sef_local_startup(114-126)/sef_cb_init_fresh(131-244)/reply(249-270)/get_nice_value(275-289)/handle_vfs_reply(294-424)。Rust 对应：`main.rs`、`init.rs`（`init()`:297、`run_once()`:357、`reply()`:451、`fill_boot_procs()`:474）、`ipc/vfs.rs`（handle_vfs_reply:190-282）。

- 主循环判定顺序（notify → pm_isokendpt → EXITING 丢弃 → VFS 回复第一路 → 事件/PM 调用 → result!=SUSPEND 才 reply）与 C 逐段 covered；`reply()` 的预填载荷复用（`ipc.reply.take()`）与 C 持久 `mp_reply` 同型；REBOOT 特例（sys_abort 后返回主循环等 HARD_STOP）表达等价。
- `sef_cb_init_fresh` 八步：1/2/5/6/8 covered（VFS_PM_INIT 从"填充循环内逐条 send"移到"填充完成后统一发送"，屏障语义不变、init.rs:546-548 已注明）；3/4/7 deferred（D-02/D-12 家族）。
- `fill_boot_procs` 逐项 covered（负 proc_nr 跳过、procs_in_use、INIT 特例、系统进程 parent RS/INIT、get_free_pid、endpoint）。
- handle_vfs_reply 11 路 + 尾部条件全部 covered，**但尾部 `restart_sigs` 的端口实现是 no-op**（C main.c:421-423 ↔ `vfs.rs:278-280` 条件判定正确 + `vfs.rs:533-540` no-op）→ V3-P2-1。
- **结构性缺口**：内核信号入口（C main.c:121 `sef_setcb_signal_manager(process_ksig)` + sef_signal.c:104-108 在 receive 路径拦 SIGKSIG/SIGKSIGSM）→ Rust notify 分支只认 CLOCK（init.rs:367-381），SIGKSIG 类通知静默丢弃；`process_ksig`（signal.rs:374）全仓只有测试调用者 → V3-P1-2。
- 小项：CLOCK notify 时间戳来源（C 用通知载荷 m_notify.timestamp，main.c:66-67；Rust 用处理时刻 `TimerCtl::now()`，init.rs:370-371）→ V3-P2-10；calls_stats 计数缺失（main.c:34-36/95-97）→ V3-P3-3；reply 失败告警生产静默（init.rs:465-467，C 总是 printf）→ V3-P3-2；文档 01 的 C 行号锚点整体偏移 3-4 行 + init.rs:3 头注释范围错 → V3-P3-6。

#### 12.1.5 Gate E：文档 §5 测试名全量对账（21 篇）

21 篇文档（00/99 无测试声称）共声称 324 个测试名：全命中 279、改名漂移 31、**完全虚构 14**、crate 归属错位 14（05/06 的这些名实际位于 `os/libs/minix-types/src/ipc/{vfs,event}.rs` 而非 servers/pm，文档未注明 crate）。数量声称无 >50% 偏差；历史快照数字（07/08/12 的"N passed"）已过时属正常演化。完整清单（逐名 + file:line）见本轮对账报告要点：

- **完全虚构（全仓无 `fn 该名`，14 个）**：`test_remove_sub_nested_guard`（06:645）、`test_run_once_proc_event_reply_no_sync_reply`（06:662）、`test_srv_fork_privilege_retained`（08:290）、`test_srv_fork_ipc_reset`（08:295）、`test_mess_lc_pm_sig_roundtrip`/`test_mess_lc_pm_sigset_roundtrip`/`test_sigmsg_roundtrip`（12:592——wire 尚不存在，属 E7 前置产物的超前声称）、`test_block_can_resume_guard`（13:441）、`test_block_delayed_isolated`（13:442）、`test_intervals_default_zero`（14:509）、`test_nice_scheduler_default`（16:375）、`test_sched_init_fills_scheduler`（16:376）、`test_try_set_tracer`（18:420）、`test_do_getres_invalid_clock`（19:425）、`test_time_types_64bit`（19:436）、`test_find_param_monitor`（20:480）、`test_frame_region_base`/`test_exec_state_idle_partial`（17:388-390，最接近名存疑）。
- **代表性改名漂移**：10 篇 wait4 族全后缀化（`test_wait4_zombie` → `test_wait4_zombie_tell_parent` 等，10:329-335）、06 的 `publish_event→publish` 系统性漂移（06:635-639）、09:316-324 五成名不存在、11:329 kill 两项、12:570/587、07:384 前缀漂移 + 行号过期（:763→实际 :824）。
- **crate 归属错位（14 个）**：05:305-313 的 10 个 + 06:615-618 的 4 个，实际在 minix-types。
- 同轮抽查验证：`test_try_set_tracer` 等五名全仓 grep 零命中已本会话复现。

→ 汇总为 **V3-P1-5**（文档对账批次）。

### 12.2 V3 条目

#### V3-P1-1 trace 域整体失真：T_* 常量 ABI 大面积错误 + 生产路径置位 stub + 测试名谎报（批次 B 前置重构）

- **优先级**：P1（当前不可达——PmCall::Ptrace 落 ENOSYS 兜底臂；但这是第 1 轮"逻辑已备"结论的修正，且常量同时喂给 `ctl.trace()` 内核透传与 `trace_stop`，接线即坏）
- **类型**：代码 bug（ABI/语义）+ 诚实契约缺失 + 测试自身失真
- **证据**：§12.1.1 全表（核心锚点：`trace.rs:12-28` ↔ `sys/sys/ptrace.h:226-250`；`trace.rs:398-401`；`trace.rs:129-133/196-207/225-227/293` 的 "for test" 注释；`guardianship.rs:112-115` 的 set_trace_options 只匹配 Traced；`signal.rs:229-239`；`18-trace.md` 零 DEFERRED 标记）
- **建议**（接线批次 B 之前一次重构）：
  1. 常量层：18 个 T_* 按 `sys/sys/ptrace.h:37-55,226-250` 全量对账；**先修 `test_constants_match_c` 为全量断言**（测试先行锁定），再改常量值。
  2. 分支层：T_ATTACH 补 TO_NOEXEC + 真实 `sig_proc(SIGSTOP)`；T_DETACH 按重放→data 信号→check_pending→内核透传全链重写（复用既有 `check_sig`/`sig_proc`/`check_pending`）；T_EXIT else 分支调 `exit_proc`；T_READB_INS/T_WRITEB_INS/T_GETRANGE/T_SETRANGE 四支按 C 补齐（root 门位置含在语义内）。
  3. 状态层：`trace_flags` 从 `Traced` 变体解出（或 `set_trace_options` 覆盖 Normal 态），修复 T_DETACH 清零 no-op；`exit_pending`/`trace_exit` 二选一收敛。
  4. 载荷层：`trace_stop` 的 tracer 回复走 wait4 status 载荷契约（D-26 同型）+ 真实发送；sys_trace 透传错误码与读值保真（与 V3-P2-6 同批）。
  5. 契约层：trace.rs 全部 "for test" 注释清除或升级为 `[DEFERRED: D-XX]`；18-trace.md 同步（TO_NOEXEC 位值 + D1 抽象的消费现状）。
- **验证**：全量常量断言测试 + do_trace 分支矩阵测试（对照 C 每个返回路径）+ 接线后集成测试。

#### V3-P1-2 内核信号（SIGKSIG）主循环入口缺失：process_ksig 无生产调用者（✅ 已修复 2026-09-09，Fix #45）

- **原状态**：C 把 `process_ksig` 注册为 SEF 信号管理回调（main.c:121；sef_signal.c:104-108 拦截 SIGKSIG），是内核→PM 信号回环的唯一入口；Rust notify 分支只认 CLOCK（init.rs:367-381），`process_ksig` 全仓只有测试调用者，且批次表均未登记。连带发现 `process_ksig` 两处死路：check_sig 的 EINVAL 被 `?` 传播（C 忽略返回值，SIGSNDELAY=70 超出 _NSIG 必触发）；SIGSNDELAY 值误写 42（真值 70，signal.h:264），尾部"恢复搁置处置"从未可达。
- **修复**（Fix #45）——**方案 a（选定）：C 同构的"通知-拉取"两段式**。方案 b（PM 表内 kernel_pending 扫描）被否：内核起源信号进不了 PM 表，权威状态在内核 priv 表；方案 c（等通电）被否：kernel 对端 getksig/endksig 已真实（syscall_signal.rs:393/475），无依赖可主张。落地：
  1. minix-sys（E6 切片）：`sys_getksig`（SYS_GETKSIG=7，回复 `m_sigcalls.{endpt,map}`）/`sys_endksig`（SYS_ENDKSIG=8）+ wire 测试 ×2。
  2. `KernelGateway` +`get_ksig`/`end_ksig`（Trap 委托真实 wrapper；`Endpoint::NONE` → None）。
  3. signal.rs：`SIGSNDELAY=70`/`SIGKSIG=74` 常量；`process_ksig` 的 check_sig 改 C 同型忽略返回值 + SIGSNDELAY 尾部接真实语义（清 DELAY_CALL → VFS|EVENT 在途 stop_proc(MustStop)，否则 check_pending）；新增 `process_sigmgr_signals` 拉取循环（getksig → 逐信号 endksig+process_ksig → NONE 终止，getksig 失败 panic 同 C "SEF: sys_getksig failed"）。
  4. init.rs：`PmServer.vtimer` 字段（`TrapVTimerCtl` pre-E6 `-EIO` 占位）+ notify 分支 SYSTEM 源触发拉取。**[ARCH] 触发判定适配**：`SigSet(u64)` 装不下位 73（kernel syscall_signal.rs:88-94 自声明，拓宽为跨层 wire 变更挂 edge E6），SYSTEM 通知本身即"有积累"的唯一载体，按 SYSTEM 源触发。
- **验证**：单测 +2（拉取循环交付 SIGTERM 终止、SIGSNDELAY 恢复 DELAY_CALL 搁置的处置）；集成 `kernel_sigksig_notify_drains_pending_kernel_signals`（SYSTEM notify → drain → 终止）；minix-sys 125 passed（+2 wire）。基线 **371 lib + 11 integration passed**。

#### V3-P1-3 sig_proc 的 VFS_CALL/EVENT_CALL 分支缺 stop_proc 调用（✅ 已修复 2026-09-09）

- **原状态**：C `signal.c:425-443` 在置 pending 后 `if (!(PROC_STOPPED|DELAY_CALL)) stop_proc(rmp, FALSE)`（防进程在 VFS 回复后、信号复查前再发起调用；PROC_STOPPED 兼作 restart_sigs 复查指示）；Rust 该处是永假死 if + "rely on 13's restart_sigs" 注释，且分支条件误用 `ipc_blocked.is_some()`（把 C 不含的 DELAY_CALL 也拦入）。kill 已接线，打在 fork 挂起进程上为可达路径。
- **修复**（V3-P1-3 / Fix #43）：分支条件改 `is_vfs_blocked()||is_event_blocked()`；未停止时经 `GatewayStopBridge` 调 `signal_flow::stop_proc(MustStop)`（= C `stop_proc(rmp, FALSE)`，EBUSY 即 panic 同 C）。内核停止能力按 §12.3 规约进 `KernelGateway`（新增 `sys_delay_stop`，生产 `TrapKernelGateway` pre-E6 诚实回 `-EIO`，wrapper 挂 edge E6 已登记项）；桥接适配器避免全量形参穿线（sig_proc/check_sig 签名与 10 个调用点零变化）。C 守卫的 DELAY_CALL 半边在 `IpcBlockReason` 互斥建模下不可表示（注释声明）。+5 单测（StopRecorder）+1 集成测试 `kill_on_fork_suspended_child_stops_it_and_records_pending`；doc 11 §4.1/§5.2、doc 13 §4.4 同步。测试基线 356→**361 lib + 9 integration passed**。

#### V3-P1-4 do_getsysinfo 数据路径假实现：拷出恒为零字节且无 DEFERRED 契约（✅ 已修复 2026-09-09，Fix #46）

- **原状态**：权限门（`effuid!=0→EPERM`）与 size 精确匹配（`EINVAL`）都真实，数据路径 `let _ = src; cpy.copy_to_user(&vec![0u8; len], dst)?` 恒拷零字节（注释自认 "In tests, we copy dummy bytes"），生产代码无 DEFERRED 标记——假成功家族第 3 例。
- **修复**（Fix #46，方案 b：fail-closed）：数据路径改 `Err(MiscError::Nosys)`（诚实失败优于假数据——RS 拿到 ENOSYS 是明确信号，拿到全零表是静默投递），`[DEFERRED: D-29]` 契约登记（真实数据路径 = PM 表的 C-ABI 序列化镜像 wire + 批次 G 接线，挂 edge E7）；`test_getsysinfo_perm_size` 的 Ok 断言改 `Nosys`。方案 a（立即真实拷出）被否：Rust 类型化表没有 C 布局字节视图，真路径 = E7 级 wire 工作，本轮无从达成真话。
- **验证**：`grep -n "dummy" os/servers/pm/src/misc.rs` 零命中；misc 测试 13 passed。

#### V3-P1-5 文档 §5 测试声称与代码大面积失同步（✅ 已修复 2026-09-09，Fix #47）

- **原状态**：21 篇文档声称 324 个测试名，完全虚构 14、改名漂移 31、crate 归属错位 14、行号过期若干（全表见 §12.1.5 与 2026-09-09 版本历史）。
- **修复**（Fix #47，纯文档批次）：12 篇文档共 51 处改名（旧名 → 经 grep 验证的实际名）；真幻影名按性质处置——12 的三个 wire roundtrip 名改写为 forward-reference（E7 前置产物，不再以测试名声称）、13 的 `can_resume()/is_delayed()` 谓词族未建成（§4.2 代码示例改写为实际实现 + §5.2 两行删除）、14/19 的两个无载体声称改写为诚实注记；05/06 的 §5 头部补 crate 归属注记；07 的 :763 行号过期修正。01 文档 C 锚点偏移与 init.rs:3 头注释（V3-P3-6 范围）随后续批次。
- **验证**：Gate E 全量对账重跑——**TOTAL MISS: 0**（反引号包裹的测试名 100% grep 命中，搜索根 = servers/pm + minix-types + minix-sys + kernel）。

#### V3-P2-1 VFS 回复尾部的 `restart_signals` 端口 no-op，注释前提失真（✅ 已修复 2026-09-09，Fix #48）

- **原状态**：生产 impl `let _ = slot;`，注释前提（"PM 尚未建模挂起信号"）自 Fix #7 起失真；调用点 vfs.rs:279 条件判定正确但投递体为空——VFS 回复路径的挂起信号重查丢失，且 V2 的 24 处 DEFERRED 收敛漏网。
- **修复**（Fix #48）：`restart_signals` 委托 `signal_flow::restart_sigs`（复用事件域 `PmEventServices` 的 RestartServices 全实现——struct 升 `pub(crate)` + `new` 构造器，借结构与 publish_event 同型）。+2 单测（挂起 SIGKILL 经尾部重查终止 / 未停止态 Noop 信号保留）。
- **验证**：`grep -n "尚未建模挂起信号" os/servers/pm/src/ipc/vfs.rs` 零命中；基线 373 lib passed。

#### V3-P2-2 sig_proc 的 PRIV_PROC `!ksig` 分支静默丢弃（✅ 已修复 2026-09-09，Fix #50）

- **原状态**：C 对系统进程的 !ksig 信号一律 `sys_kill` 内核回环（signal.c:456-462）；Rust 分支静默丢弃——Fix #33 只修了 check_sig 广播对 RS 的调用点，sig_proc 本体仍 no-op。
- **修复**（Fix #50）：一行 `kern.sys_kill(endpoint, signo)`（返回值 C 不检查）；+1 单测（SigSendRecorder 扩展 kills 记录，断言 sys_kill 转发且不误触 sys_sigsend）。
- **验证**：基线 378 lib passed。

#### V3-P2-3 sched_start_user 调度器端点硬编码 + 缺 KERNEL/NONE 守卫（✅ 已修复 2026-09-09，Fix #51）

- **原状态**：C 按调用点守卫（main.c:370-371）后把 `rmp->mp_scheduler`（继承自父）传给 `sched_start`；Rust 硬编码 `SCHED_PROC_NR` 且无守卫。
- **修复**（Fix #51）：函数改读目标进程的 `resources.scheduler` 作调度器端点；KERNEL/NONE 按 C 直接跳过（Ok，不调内核不回写）；成功后回写继承值（C 出参语义）。+1 单测（NONE 跳过 / 自定义调度器按继承值转发）；CaptureSched 扩展 last_sched 记录。sched_init 的 assert 注释化保持（其两个断言依赖 boot 时序唯一性，init_scheduling 的测试环境已覆盖）。
- **验证**：sched 测试全绿；基线 379 lib passed。

#### V3-P2-4 do_getepinfo 两处语义偏差（✅ (a) 已修复 2026-09-09，Fix #52；(b) 随批次 A/G，D-30 登记）

- **原状态**：回复 `ngroups` 先截断再填（C 填全量 `mp_ngroups`，截断只影响拷贝数，misc.c:184）；groups 拷出未接线（`_cpy` 未用，EFAULT 无透传）。
- **修复**（Fix #52）：`EpInfo.ngroups` 改填全量值，拷出数单独截断（copy_len = min）；`[DEFERRED: D-30]` 登记 groups 拷出与 EFAULT 透传（批次 A/G 的 wire + CopyGroups 生产实现，edge E7）；测试补全量/截断两断言。
- **验证**：`test_getepinfo_trunc` 扩展后全绿；基线 379 lib passed。

#### V3-P2-5 do_svrctl / do_sysuname 分支细节五处偏离（扩展 V2-P3-4(b)，批次 G 前置）

- **优先级**：P2；**类型**：语义偏移
- **文件**：`os/servers/pm/src/misc.rs:404-428, 213-218, 275-304`；C `misc.c:291-395, 72-100`
- **问题**：(a) IOCGROUP ∈ {'P','M'} 门与未知 req→EINVAL 缺失（Rust 以 `is_set: bool` 替代四 req 判别，V2-P3-4(b) 已登记）；(b) GET 的 key 长度上界 `keylen > 64 → EINVAL`（misc.c:316,359）缺失；(c) keylen==0 全表时 C 的 val_len 取 `sizeof(monitor_params)` 全缓冲长（misc.c:352-354）触发 E2BIG，Rust 用实际串长+1（misc.rs:409,412-414）——缓冲不足时两侧判定不同；(d) SET 的 ENOSPC 与 30 边界检查顺序与 C 相反（misc.c:327→328-334 vs misc.rs:213-218），双条件并存时错误码不同；(e) do_sysuname 无 `req` 方向分支（req!=0→EINVAL 无处产生，misc.c:85-96）且用户目的地址用 placeholder（misc.rs:300，EFAULT 无载体）。
- **建议**：随批次 G 的 wire 解码一次做齐：req 参数还原 + IOCGROUP 门 + 两个边界 + 顺序对齐 + sysuname 方向分支。
- **验证**：分支矩阵测试（对照 C 每个错误码路径）。

#### V3-P2-6 错误码折叠为 EINVAL 的横切模式（✅ 已修复 2026-09-09，Fix #44/ #53）

- **原状态**：sched（inherit/set_nice）、trace（sys_trace 透传）、misc（getrusage 的 sys_times/vm_rusage）三处把底层真实 errno 折叠为 EINVAL；C 全程透传 `r`。
- **修复**：trace 已随 Fix #44（`TraceError::Kernel(i32)`）；本批（Fix #53）补 sched（`SchedError::Kernel(i32)`，inherit/set_nice 透传 C schedule.c:83/108 的 rv）与 misc（getrusage 的 seam 错误原样上抛不再 `map_err(|_| Inval)`，TimesVmCtl 生产实现的原始 errno 经 `MiscError::Kernel` 载荷保留）。同时 crate 级规约落 plan.md：**透传型调用的错误必须保真，只有 PM 自身判定才产生语义化枚举变体**。
- **验证**：mock 注入非 EINVAL 底层错误 → 用户态收到同值 errno（sched/ming 的 Kernel 载荷测试随批次 F/C 的接线测试补强）。

#### V3-P2-7 publish_event 的裸指针四路借用拆分可免（删 unsafe）

- **优先级**：P2；**类型**：不必要的 unsafe（no_std 代码库应最小化 unsafe 面）
- **文件**：`os/servers/pm/src/ipc/vfs.rs:502-510`
- **问题**：用 `*mut` 拆分 `self` 的四个字段借用（table/transport/registry/kern）。Rust 的字段级重借用（`&mut self.table`、`&mut self.transport`……逐字段传参）即可满足借用检查器——四个 `&mut` 字段互不相交，`PmServices::new` 的构造方式（init.rs:406-412 从不相交字段构造）就是先例。同文件 `exec_restart`（vfs.rs:513-531）就是直接字段组合而未用 unsafe。
- **建议**：改为按字段传参的自由函数或直接逐字段重借用；删 4 个裸指针与 unsafe 块。零行为变化。
- **验证**：`grep -n "as \*mut" os/servers/pm/src/ipc/vfs.rs` 零命中；既有测试全绿。

#### V3-P2-8 event 的 `do_proceventmask` 非 mut 变体带游标 bug 且以 pub 存活（✅ 已修复 2026-09-09，Fix #54）

- **原状态**：非 mut 变体内联前移数组但不调整其它进程游标（C event.c:142-160 的 remove_sub 游标回退缺失），自带大段"取巧/近似"注释且以 `pub fn` 存活（3 个测试引用）；`waiting < NR_PROCS` 守卫为 `debug_assert!` 且硬编码 256。
- **修复**（Fix #54）：非 mut 变体整体删除（70+ 行近似实现与注释消亡），3 个测试迁移到 `_mut` 版（同一行为面）；waiting 守卫改自增前 `assert!` + 引用 `minix_types::NR_PROCS`（C event.c:108-109 同位）。
- **验证**：`grep -n "pub fn do_proceventmask(" os/servers/pm/src/event.rs` 零命中；基线 379 lib passed。

#### V3-P2-9 clippy 基线回退：lib 3 条 unused import + 测试代码 22 条 unused variable

- **优先级**：P2；**类型**：卫生回退（V2 收官时 lib 0 warning）
- **证据**：`ipc/vfs.rs:28`（`core::fmt`）、`ipc/vfs.rs:31`（`IpcError`）、`timer.rs:7`（`Pid`）；测试侧 22 处（event.rs×6、ipc/calls.rs×4、ipc/vfs.rs×7、fork.rs/signal.rs/dispatcher.rs/misc.rs/tests 各 1，多为 mock 形参未加下划线前缀）。回退源头是 Fix #41/#42 收尾批次清理了使用点却留下导入。
- **建议**：一次卫生批次恢复 0 基线；campaign 收尾清单增加 `cargo clippy -p minix-pm --lib --all-targets` 项（--lib 不覆盖 cfg(test) 代码）。
- **验证**：clippy lib 0 warning；`--all-targets` unused 告警 0。

#### V3-P2-10 CLOCK notify 时间戳来源：处理时刻时钟 vs 通知载荷时间戳

- **优先级**：P2；**类型**：语义偏差（低severity，批次 D 前置）
- **文件**：`os/servers/pm/src/init.rs:370-371`（`self.timer.now()`）+ `timer.rs:372-390`；C `main.c:66-67`（`m_in.m_notify.timestamp`）
- **问题**：C 把通知消息携带的内核时间戳传给 expire_timers；Rust 用处理时刻的时钟——主循环拥塞时（前面消息处理耗时）到期判定整体后移，SIGALRM 可能晚发。
- **建议**：批次 D 接线时改读通知载荷时间戳（wire 字段就绪性随 E7 的 notify 消息族；若 wire 缺字段则登记 E7）。
- **验证**：单测：构造带旧时间戳的通知 → 到期判定按载荷时间。

#### V3-P3 小项清单（卫生/文档，一次或分批清理）

1. **is_superuser 三处重复**：`time.rs:211-216` 与 `misc.rs:265-270` 逐字相同的自由函数 + `mproc/credentials.rs:59-61` 的方法——收敛到单一 helper（方法或 crate 级 fn）。
2. **诊断输出生产静默且口径不一**：`reply()` 发送失败告警 `#[cfg(test)]`（init.rs:465-467，C main.c:267-269 总是 printf）vs `PmServices::send_reply` 生产 eprintln（vfs.rs:366-369）；`sched_init` 失败日志 cfg(test)（sched.rs:191-193）；getsysinfo/getprocnr 未授权审计缺失（misc.c:118-121,154-157；其中 sys_diagctl_stacktrace 部分挂 E6）。建议做一次 no_std 日志面选型决策（crate 级），统一口径。
3. **calls_stats 计数缺失**：C main.c:34-36/95-97 在 ENABLE_SYSCALL_STATS 下逐调用计数，misc 的 SI_CALL_STATS 消费端已在（misc.rs:27/111/324-328），feature 已声明（Fix #17）但计数本体未落。
4. **signal.rs 残留 7 处裸移位**：trace/VFS/PRIV 分支用 `1u64 << (signo - 1)`（signal.rs:233/242/244/289-298），位基正确但未走 Fix #29 确立的 `init::sig_bit` 单点入口（该 fix 的"本文件不再出现裸移位"验证声明与现状不符）。
5. **PROC_EVENT_REPLY 常量双址**：`dispatcher.rs:32` 本地 `0xE80` 与 minix-types 同值常量（`ipc/event.rs:27`）并存——并入 E7 清单。
6. **文档锚点/表述漂移合集**：01 文档 C 行号整体偏移 3-4 行 + init.rs:3 头注释范围错（"main.c:49-268"应为 131-244）；07:384 行号过期（:763→:824）；18 §2.14 TO_NOEXEC=0x1 与 C 0x4 矛盾；06:522 "from_bits_truncate 保留未知位"与实际语义（丢弃）不符；20 §1.1 `__arraycount=8` 与 C 9 元素不符 + §3 声称 `ArrayVec<_,2>` 实为 `Vec`（misc.rs:204）；misc.c do_getsetpriority 归属 16 文档已声明但 20 文档 §2 表仍有残句（[待验证]）。并入 V3-P1-5 的文档批次执行。

### 12.3 架构审查 V3（分层观察与建议）

#### 观察 1：内核接入端口已达 30 个 trait，双风格并存的成本开始显形

V3 实测清单（`grep 'pub trait' os/servers/pm/src`）：中央 `KernelGateway`（exit.rs:25，6 方法：sys_kill/sys_clear/sys_abort/proc_times/copy_to_user/sys_resume）+ 内核能力窄 trait（`KernelStop`/`KernelResume`（signal_flow.rs:27/33）、`KernelSig`（signal_handlers.rs:70）、`KernelExec`（exec.rs:71））+ 子系统窄端口（`TimerCtl`/`VTimerCtl`、`ClockSource`/`BootTimeCtl`/`SetTimeCtl`/`ClockTime`、`SchedCtl`、`TraceCtl`、`SysInfoCtl`/`CopyToUser`/`RebootCtl`/`TimesVmCtl`/`McontextCtl`/`SprofCtl`、`CopyGroups`/`VfsForwarder`、`VfsCtl`/`SignalDeliver`/`ExitHandler`/`RestartServices`、`VfsExec`/`TracerSig`、`VfsReplyServices`、`SigSetExt`、`IpcTransport`）。

两个具体重叠：(a) `misc.rs:135` 的 `CopyToUser` 与 `KernelGateway::copy_to_user`（exit.rs:50）是同一"把字节写入目标进程用户内存"关切的两套端口——getrusage/getepinfo 走前者、tell_parent 走后者；(b) 测试 mock 成本：`KernelGateway` 已有 12 个 impl（grep 实测），每个新方法都要求全部 mock 跟进（Fix #25 曾为此单独提交 accaf077c 补 mock）。

**方案对比**：
- (a) 大一统：全部能力并入 KernelGateway，窄 trait 删除。否——mock 膨胀到每测试 20+ 方法，接口隔离的本意（每个子系统的测试替身最小化）被摧毁。
- (b) **冻结双风格 + 书面规约**（推荐）：规约三条——新内核能力一律进 `KernelGateway`；窄 trait 不新增，仅在借用冲突需要合并注入点时以 supertrait 组合承接（`RestartServices = KernelResume + ExitHandler + SignalDeliver`（signal_flow.rs:269）与 `ExecRestartServices = KernelExec + TracerSig`（exec.rs:89）已是两个先例）；`CopyToUser` 类与 KernelGateway 重叠的端口在下次触碰时收敛（getrusage 的 `_cpy` 反正要随批次 G 接线）。同时把 V2 承诺的**四列对照表**（内核能力 ↔ trait ↔ minix-sys wrapper ↔ C libsys 函数）落进 plan.md ARCH 附录——它既是接线批次的查询面，也是双风格的边界声明。
- (c) view-trait（`impl KernelSig for dyn KernelGateway` 类 blanket impl）：dyn 兼容性手术，收益不抵复杂度。否。

#### 观察 2：unsafe 消息解码分散在 6 个分发臂——E7 落地前的阶段内过渡

`dispatch_pm_call` 已接线的 7 臂中有 6 处内联 `unsafe { msg.m_u.m_lc_pm_* }` 解码（calls.rs:228/242/252/269/286/304），字段映射靠注释对照 C。E7 的 wire 类型系统化是终局（共享层），但 40 臂接线期间 unsafe 会按臂复制。**阶段内建议**：在 pm crate 内建 `ipc/decode.rs`，每调用一个 `fn decode_xxx(&Message) -> Args`（unsafe 单点 + C `ipc.h` 字段对照注释 + 布局断言测试），分发臂只调 decode——unsafe 面从"散布 40 臂"收敛为"单文件"。这不是 E7 的替代而是其消费端预备（E7 落地后 decode 模块改为委托 wire 类型，调用点零改动）。

#### 观察 3：错误保真（横切，见 V3-P2-6）

错误枚举把底层 errno 折叠为 EINVAL 的模式出现在 sched/trace/misc 三处。OS 边界上的错误码是外部可观察契约的一部分（与 pid 相位同类），建议作为 crate 级规约写入 plan.md：**透传型调用（对内核/他服务的 raw call）的错误必须保真，只有 PM 自身判定才产生语义化枚举变体**。

#### 观察 4：双层模块与信号域四模块的导航性（P3，暂不动）

顶层与 `mproc/` 下 5 对同名双层模块（fork/wait/signal/credentials/trace）是"编排层 vs 状态层"的切分，P2-4 修复后路径已可分辨，维持。信号域散布 4 个顶层模块（signal.rs 704 + signal_handlers.rs 505 + signal_flow.rs 617 + mproc/signal.rs 637，合计约 2463 行）且 `sig_send`/`sig_proc` 等符号跨文件引用，是 crate 内导航成本最高的域——批次 B（信号控制接线）是检验点：若接线时发现跨模块借用/重命名摩擦，再评估合并为 `signal/` 子目录（core/handlers/flow/state）。现在不动（避免与批次 B 的 diff 冲突）。

#### 观察 5：测试架构——"测试名声称对账 C"的子集谎报（Rule Discovery V3-1）

`trace.rs:398-401` 的 `test_constants_match_c` 只断言 18 个常量中恰好正确的 2 个。与 V2 的 TSTL/SBCD 同族但不同形：**测试名声称全面对账（match_c），断言集却恰好规避了所有错误项**。候选模式 CSL（Constants-match Subset Lie）建议：凡名含 `match_c`/`matches_c` 的测试，断言项数量必须与被对账全集一致或显式注释排除理由；检查命令：`rg "fn test_\w*match_c" os/servers/pm/src -A5` 人工核对断言覆盖面。登记状态与 V2 两个候选模式一并待规则集维护轮。

#### 观察 6：`unwrap_or_default()` 凭据模式（设计观察，非偏差——对子代理结论的降级）

`sched.rs:258-259`、`trace.rs:103-104` 等处对 `privilege.credentials()` 用 `.cloned().unwrap_or_default()`：`Privilege::Kernel` 槽位得到全零凭据 = effuid 0 = 超级用户。审查代理曾标为"特权门旁路"，经对照 C 判定为**等价模拟**：C 的 mproc 表每槽都有 mp_effuid 字段且系统进程启动即为 0（超级用户），Rust 的全零默认恰好复现该行为，不存在对 C 的偏离。保留为设计观察：该模式把"无凭据"隐式映射为"root"，是脆弱默认——未来若 `Credentials` 增字段或语义变化，会静默放权。建议在 `Privilege` 上提供显式的 `effective_uid_or_root()` 类命名方法，让"缺省即 root"成为显式契约而非 derivational 副作用。

#### 观察 7：Redox 参照（V3 增补）

见 §12.4。

### 12.4 对照 Redox 的架构参考（V3 增补）

V2 轮（§11.3）已核对 GitLab 源码与迁移提交。V3 轮补充两项 2025 年的公开材料，均为方向性佐证（未逐行核对源码，采纳前建议复核）：

1. **FOSDEM 2025 报告 "POSIX Signals in User Space on the Redox Microkernel"**（[slides](https://archive.fosdem.org/2025/events/attachments/fosdem-2025-5670-posix-signals-in-user-space-on-the-redox-microkernel/slides/238302/posix-sig_whCJBqp.pdf)）：确认用户态进程管理器（procmgr）与用户态信号的落地，及其核心约束——**"userspace needs state and locks; only SIGKILL can force-cancel an IPC syscall"**。与 PM 的对照：Minix3 用 VFS_CALL 挂起 + `stop_proc`（C signal.c:436-443）达成同款"信号复查前不得再入"的防重入——这正是 V3-P1-3 缺失的那个调用；"只有 SIGKILL 能强制取消在途 IPC"与 `sig_proc` 对 SIGKILL 的特殊豁免（signal.rs:229 `signo != SIGKILL`）同构。
2. **Redox 官方博客 "Towards Userspaceification of POSIX – Part I"**（[kernel-11](https://www.redox-os.org/news/kernel-11/)）：进程管理/信号迁用户态的动机与 scheme 机制路线。对 PM 的含义：minix-rs "生命周期语义放用户态服务器 + 决策在 PM、投递经内核回环"的边界选择与 Redox 的演进终态一致（V2-Redox-1/5 的延续），无需为"现代化"而把决策移进内核。

### 12.5 Rule Discovery（Step 5.7，V3）

1. **CSL（Constants-match Subset Lie，候选模式）**：§12.3 观察 5——`test_constants_match_c` 型"对账名 + 子集断言"。建议严重度 P1（测试自身正确性维度）；目标文件 `prompt/skill/review-patterns-skill.md` 测试族。
2. **假成功家族扩展提案**：V3-P1-4（getsysinfo 拷零字节）表明 V2 的 TSTL（测试 seam 泄漏）应扩展为更宽的"生产路径测试数据/替身族"——判定特征：生产代码中出现仅为测试服务的构造（mock 类型、dummy 数据、假应答），且无 DEFERRED 契约。与 TSTL 合并登记为一条模式的两类实例。

### 12.6 建议的推进顺序（V3，吸收 V2 §11.6 遗留）

1. **卫生批**：V3-P2-9（clippy 回归 0）+ V3-P3-1/3/4/5 小项 + `--all-targets` 进收尾清单。
2. **trace 域重构（V3-P1-1）**：先修测试（全量常量断言）→ 常量 → 分支 → 状态/载荷 → 契约与文档 18。这是批次 B 的前置。
3. **信号链三缺口**：V3-P1-3（stop_proc 接入）→ V3-P2-2（PRIV_PROC 转发一行）→ V3-P2-1（restart_signals 接线，与 V2-P2-7/V2-P2-8 同轮设计）。
4. **内核信号入口批次 H（V3-P1-2）**：两案设计决策 + notify 分支 + E6 对端核实。
5. **接线批次 A → C → G**（按 §11.1.1 前置条件，含 V3-P1-4、V3-P2-4/5、V3-P2-10 的逐项先修与划账）。
6. **错误保真批（V3-P2-6）**：三枚举 Raw 透传 + 规约入 plan.md。
7. **批次 E（exec）与批次 B（信号控制，trace 重构后）**。
8. **批次 F（调度）**：依赖 SCHED 服务器（06-stage）+ V3-P2-3 先修。
9. **文档对账批（V3-P1-5 + V3-P3-6）**：可与任一等待外部依赖的窗口并行。
10. 跨阶段部分（E6 清单增补：sys_delay_stop、内核 ksig 对端；E7 增补：PROC_EVENT_REPLY 双址）见 §9.2 与 edge_todo.md，单线程执行。
