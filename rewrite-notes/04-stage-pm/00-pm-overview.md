# 00 — pm-overview：进程语义权威的边界、分层与阅读路线

> **状态**: 已完成（2026-09-09 改写；随 V3 campaign 收官刷新实施现状）
> **定位**: 总览（入口文档）——PM 的角色边界、模块分层、实施现状与 22 篇文档导航
> **源码**: minix3/minix/servers/pm/ 全部（15 .c + proto.h/const.h/glo.h）
> **Rust 模块**: os/servers/pm/src/ 全部
> **前置阅读**: 无；**后继**: 99（常量与全局状态）→ 02/03 → 01/04 → 各机制

## 1 概念

### 1.1 微内核里"谁管理进程"

Minix3 的内核只保留两样东西：调度与 IPC 原语。进程的**语义**——谁是谁（endpoint/pid/父子关系）、谁生了谁（fork 的表复制与多服务器协同）、谁死了还没人收（zombie 与 wait 记账）、谁能给谁发信号（权限判定与 core/ign/noign 三集合）、凭什么代表谁执行新程序（exec 与 setuid 链）——全部由用户态的 **PM 服务器**持有。这不是实现细节的偶然，而是微内核的核心主张：进程语义放在用户态，崩溃可由 RS 重启，内核保持可验证的小。

这份主张的代价是：PM 的每一个语义动作都要跨服务器消息完成。fork 不只是复制一张表——它要向 VM 预订地址空间（VM_FORK）、向 VFS 登记新进程（VFS_PM_FORK）；exit 要向 VM 预告（VM_WILLEXIT）、向 VFS 告知（VFS_PM_EXIT）；被捕获信号的投递要经内核回环（sys_kill/SIGKSIG）。PM 的全部代码本质上就是这个"语义编排协议"的实现。

### 1.2 PM 的四重权威

1. **进程表唯一所有者**：`mproc[NR_PROCS]` 是全系统进程元数据（pid/parent/credentials/信号状态/调度参数）的唯一权威，启动时经 VFS_PM_INIT 与 VFS 的镜像对齐。
2. **信号管理器**：`sef_setcb_signal_manager(process_ksig)`（main.c:121）——内核积累的信号经 SIGKSIG 通知回环到达 PM，由 `check_sig/sig_proc` 完成权限判定与投递。
3. **生命周期编排者**：fork/exit/wait 的每一步都是跨 VM/VFS/内核的消息协议（07/09/10）。
4. **记账方**：CPU 时间（sys_times 累加进 child 桶）、rusage、进程组/会话。

### 1.3 主循环骨架

PM 的一切从 `run_once` 的一轮开始（main.c:59-110 的镜像）：`receive` → notify 分派（CLOCK → 到期定时器；SYSTEM 源 → 内核信号拉取）→ `pm_isokendpt` 验证 caller → EXITING 进程的消息丢弃 → 三路分发（VFS 回复 / PROC_EVENT_REPLY / PM 调用族）→ 非 SUSPEND 则 reply。骨架细节归 01（启动链）与 04（分发）；每条臂的语义归各自的机制文档。

## 2 源码地图与文档导航

### 2.1 15 个 C 文件 → 22 篇文档

| C 源文件 | 行数 | 语义 | 文档 |
|---------|------|------|------|
| main.c / table.c | 424/62 | 启动 + 主循环 + 回复 + call_vec + VFS 回复 | 01/04/05/16 |
| forkexit.c | 807 | fork/srv_fork/exit/wait | 07/08/09/10 |
| signal.c | 855 | 信号全系统 | 11/12/13 |
| alarm.c | 344 | itimer/定时器 | 14 |
| exec.c | 200 | exec 流程 | 17 |
| getset.c | 223 | uid/gid/groups/session | 15 |
| event.c | 353 | 进程事件订阅 | 06 |
| misc.c / schedule.c / time.c / trace.c / utility.c / profile.c / mcontext.c | — | 外围面 | 16/18/19/20 |

### 2.2 Rust 模块镜像

```
os/servers/pm/src/
├── mproc/           状态层（mproc/table/context/fork/wait/signal/
│                    credentials/trace/guardianship/lifecycle/block/
│                    pid_gen/constants）—— 无上层依赖
├── *.rs             逻辑/编排层（fork/exec/exit/wait/signal/
│                    signal_handlers/signal_flow/sched/misc/credentials/
│                    time/timer/trace/event）—— 单向依赖状态层
├── ipc/             分发与传输（calls/dispatcher + transport + vfs 协议）
└── init.rs          PmServer 骨架（启动链 + run_once + reply）
```

### 2.3 推荐阅读路线

**99**（常量与全局状态的词汇表）→ **02/03**（mproc 结构与表操作）→ **01/04**（启动链与分发骨架）→ **05/06**（VFS 协议与事件订阅）→ **07–10**（生命周期）→ **11–13**（信号）→ **14–20**（定时器/凭证/调度/exec/ptrace/时间/杂项）。

## 3 实施现状总览

### 3.1 分发与接线

47 个调用号全部枚举建模（ARCH A-5：`PmCall`，repr(i32) 判别值即调用号）；"接线" = `dispatch_pm_call` 的显式臂替换 ENOSYS 兜底。截至 2026-09-09：8 个调用真实接线（Exit/Fork/Wait4/Kill/SrvFork/SrvKill/ProcEventMask/Ptrace），其余 40 个按批次 A–G 推进（台账：todo.md §11.1.1）；批次 H（内核信号入口）已闭环。

### 3.2 跨阶段依赖

- **minix-sys trap 层**（edge E1）：真实 IPC/kernel-call 通电的前置
- **SYS_* wrapper**（E2/E6）：已落地 sys_kill/clear/abort/times/runctl/vircopy/trace/getksig/endksig/sigsend（kernel 对端均已真实）；余 sys_setalarm/vtimer/datacopy/sigreturn/GETMONPARAMS 等
- **wire 类型**（E7）：47 调用的消息布局系统化（已落 Wait4/Ptrace 两族）

### 3.3 测试基线

`cargo test -p minix-pm`：**381 lib + 11 integration passed**（2026-09-09，Fix #59 后）；clippy lib 0 warning；Gate A 覆盖 93.6%（7 个未匹配符号均已核实为非缺口）。各机制文档的测试矩阵见各自 §5。

## 4 实施详解

### 4.1 端口面规约（V3）

内核能力的唯一归属是中央 `KernelGateway`（exit.rs）；各域窄 trait（KernelStop/KernelResume/KernelSig/KernelExec/TimerCtl/...）是函数域视图，不新增，仅在借用冲突时以 supertrait 组合承接（`RestartServices`/`ExecRestartServices` 先例）。内核能力四列对照表（能力 ↔ trait ↔ minix-sys ↔ kernel 对端）见 plan.md §4.1 附录。

## 5 测试点

00 为导航文档，无独立机制测试声称；全 crate 测试基线随 campaign 刷新（§3.3），逐篇矩阵在各机制文档 §5。

## 6 过渡

前置阅读：无。后继：99（跨机制共享的常量与全局状态词汇表）。

## 7 参见

- plan.md：§2 架构原则、§3.4 文档划分、§4 ARCH 清单与四列对照表、§5 覆盖核对
- todo.md：接线批次表（§11.1.1）、修复账目（§10/§12）
- 01-stage-kernel（内核对端）、02-stage-vm（VM 协同）、05-stage-vfs（VFS 协同）
- 外部参照：Redox 用户态 procmgr（todo.md §11.3/§12.4）
