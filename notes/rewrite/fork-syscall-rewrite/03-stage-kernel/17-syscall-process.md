# 17-syscall-process: 进程管理类系统调用

> **分类**: 系统调用
> **源码**: `minix3/minix/kernel/system/do_fork.c`, `do_exec.c`, `do_exit.c`, `do_clear.c`, `do_runctl.c`, `do_schedctl.c`, `do_statectl.c`
> **说明**: fork/exec/exit/clear/runctl/schedctl/statectl
> **前置**: 11-scheduling-primitives.md, 16-smp.md
> **创建**: 2026-06-13

---

## 1. 概述

### 1.1 概念定义

进程管理类系统调用由 PM (Process Manager) 等系统服务发起，内核负责底层进程表操作：

1. **SYS_FORK**：创建子进程，复制父进程 proc 结构，分配新 endpoint
2. **SYS_EXEC**：替换进程映像，设置新的 IP/SP，清除旧状态
3. **SYS_EXIT**：系统进程退出，发送 SIGABRT 信号
4. **SYS_CLEAR**：清理进程表槽位，释放 IRQ/timer/IPC 资源
5. **SYS_RUNCTL**：控制进程的 RTS_PROC_STOP 标志（停止/恢复）
6. **SYS_SCHEDCTL**：设置进程调度参数（优先级/时间片/CPU 亲和）
7. **SYS_STATECTL**：状态控制（IPC 过滤、IPC 引用清理、状态表设置）

### 1.2 与 Minix3 的对应关系

| 系统调用 | C 源码 | 说明 |
|---------|--------|------|
| SYS_FORK | do_fork.c | 复制 proc 结构，新 endpoint |
| SYS_EXEC | do_exec.c | 设置新 IP/SP，清除 FPU |
| SYS_EXIT | do_exit.c | 发送 SIGABRT 信号 |
| SYS_CLEAR | do_clear.c | 清理槽位，释放资源 |
| SYS_RUNCTL | do_runctl.c | 停止/恢复进程 |
| SYS_SCHEDCTL | do_schedctl.c | 调度参数设置 |
| SYS_STATECTL | do_statectl.c | 状态控制 |

### 1.3 行为规则

1. **FORK 前提**：父进程必须处于 RTS_RECEIVING 状态（同步 fork）
2. **FORK 特权**：如果父进程是 SYS_PROC，子进程降级为 USER_PRIV 并设置 RTS_NO_PRIV
3. **EXEC 清理**：清除 MF_DELIVERMSG、MF_FPU_INITIALIZED，释放 FPU
4. **EXIT 行为**：不回复（EDONTREPLY），向调用者发送 SIGABRT
5. **CLEAR 释放**：释放地址空间、IRQ hooks、alarm timer、IPC endpoint、FPU
6. **RUNCTL SMP**：跨 CPU 停止进程需要 IPI 同步
7. **SCHEDCTL 两种模式**：内核调度（设参数）或调用者调度（设 p_scheduler）

---

## 2. C 源码分析

### 2.1 SYS_FORK — do_fork.c

**消息字段**：
- `m_lsys_krn_sys_fork.endpt`：父进程 endpoint
- `m_lsys_krn_sys_fork.slot`：子进程槽位号
- `m_lsys_krn_sys_fork.flags`：fork 标志（PFF_VMINHIBIT）

**关键操作**：
1. 验证父进程 endpoint 和子进程槽位
2. 保存子进程 FPU 区域指针
3. `*rpc = *rpp` 复制整个 proc 结构
4. 恢复 FPU 区域指针，复制 FPU 状态
5. 递增 endpoint generation
6. 设置子进程返回值为 0
7. 清零时间统计
8. 清除虚拟/profiler 定时器
9. 追加进程名 "*F"
10. 设置 RTS_NO_QUANTUM
11. 如果父进程是 SYS_PROC，降级子进程权限
12. 如果 PFF_VMINHIBIT，设置 RTS_VMINHIBIT
13. 清除信号相关标志

**Rust 实现现状**（2026-06-14 修复 P1-01）：

`os/kernel/src/syscall_process.rs:126-213` 中的 `dispatch_fork` 已实现 C `do_fork.c:42-136` 的核心路径，但通过类型化签名替换了若干 C 隐式约定：

| C 语义 | Rust 实现 | 备注 |
|--------|----------|------|
| `do_fork.c:44-46` endpt/slot 提取 | `msg_copy` 解包 → `m1.m1i1`/`m1.m1i2`/`m1.m1i3` | 与 C `m_lsys_krn_sys_fork` overlay 同构 |
| `do_fork.c:48` `isemptyp(rpc)` | `proc_table.is_empty(child_slot)` | `ProcessTable` 提供显式查询 |
| `do_fork.c:56-59` `RTS_ISSET(rpp, RTS_RECEIVING)` | `caller.p_rts_flags.is_set(RtsFlagsBits::RECEIVING)` | bitflags 替代裸位运算 |
| `do_fork.c:53-54` `*rpc = *rpp` | `KProcess::fork_from(caller, child_slot, child_endpoint)` | RAII 风格 clone，文档化"调用方 = 父进程"不变量 |
| `do_fork.c:55-57` `_ENDPOINT(gen, slot)` | `Endpoint::fork_new_endpoint(child_old_endpoint, child_slot)` | 类型化 generation 递增 |
| `do_fork.c:62` `rpc->p_reg.retreg = 0` | 由 `fork_from` 在 `p_reg` 初始化中处理 | 不再显式赋值 |
| `do_fork.c:84-87` SYS_PROC 降级 | `child.priv_id = Some(USER_PRIV_ID)` + `complete_fork_setup(&mut child, parent_is_sys_proc, fork_flags)` | 集中于 `complete_fork_setup`，避免散落 |
| `do_fork.c:93-95` PFF_VMINHIBIT → RTS_VMINHIBIT | `complete_fork_setup` 内部根据 `fork_flags` 置位 | flag → RTS 一一映射 |
| `do_fork.c:104-106` name 追加 `"*F"` | `complete_fork_setup` 内部字符串拼接 | `heapless::String<N>` 无 panic 截断 |
| `do_fork.c:99-100` 清信号 flag | 由 `fork_from` 处理 | 一致性归约 |
| `do_fork.c:111` `m_krn_lsys_sys_fork.endpt = rpc->p_endpoint` | 返回 `KcallResult::Ok(child_endpoint.0)`，由 `kernel_call_dispatch` 写入回复消息 | i32 raw 值，便于调用方赋值 |

**遗留（DEFERRED）**：

1. **FPU save/restore**（`do_fork.c:49, save_fpu()`）：依赖 arch 层 FPU 上下文。Rust 端通过 `KProcess::p_reg` 寄存器结构已包含 FPU 槽位，但保存语义由 `BootProcArch` trait 实现，与 C `arch_save_fpu()` 等价路径尚未接通。
2. **`sched_proc()` 调用**（`do_schedctl.c`，与 SYS_FORK 链路独立但经常协同）：`Scheduler` 集成路径尚未通过 `kernel_call_dispatch` 暴露 `&mut Scheduler` 引用。
3. **`ProcessTable` 传递**：当前 `kernel_call_dispatch` 内的 `dispatch_fork` wrapper 创建临时 `ProcessTable` 实例（仅用于访问 `is_empty`），真实访问需要 `KernelState` 重构时将 `ProcessTable` 提升为全局状态。

**测试覆盖**：

| 测试 | 位置 | 覆盖 |
|------|------|------|
| `test_dispatch_fork_creates_child_with_new_endpoint` | syscall_process.rs | generation 递增 + 子进程 endpoint 有效 |
| `test_dispatch_fork_sys_proc_downgrade` | syscall_process.rs | SYS_PROC 父 → 子 `priv_id = USER_PRIV_ID` + RTS_NO_PRIV |
| `test_dispatch_fork_vminhibit_sets_flag` | syscall_process.rs | `PFF_VMINHIBIT` → `RTS_VMINHIBIT` |
| `test_dispatch_fork_name_suffix` | syscall_process.rs | 进程名追加 `"*F"` |
| `test_dispatch_fork_rejects_non_receiving` | syscall_process.rs | 非 RTS_RECEIVING 父 → `EINVAL` |
| `test_dispatch_fork_rejects_in_use_slot` | syscall_process.rs | 槽位非空 → `EINVAL` |

### 2.2 SYS_EXEC — do_exec.c

**消息字段**：
- `m_lsys_krn_sys_exec.endpt`：进程 endpoint
- `m_lsys_krn_sys_exec.ip`：新指令指针
- `m_lsys_krn_sys_exec.stack`：新栈指针
- `m_lsys_krn_sys_exec.name`：进程名指针
- `m_lsys_krn_sys_exec.ps_str`：ps_strings 指针

**关键操作**：
1. 清除 MF_DELIVERMSG
2. 从用户空间复制进程名
3. `arch_proc_init()` 设置新 IP/SP
4. 清除 RTS_RECEIVING
5. 清除 MF_FPU_INITIALIZED，释放 FPU

### 2.3 SYS_EXIT — do_exit.c

**关键操作**：
1. `cause_sig(caller->p_nr, SIGABRT)` — 发送 SIGABRT
2. 返回 EDONTREPLY

**Rust 实现现状**（2026-06-13 修复 P1-12）：
- 已在 `os/kernel/src/syscall_process.rs:201` `dispatch_exit` 中实现 `cause_signal_abort(caller)` 助手函数。
- 该助手设置 `caller.p_pending.add(SIGABRT)`（C `system.c:411` `sigaddset`）并置位 `RTS_SIGNALED | RTS_SIG_PENDING`（C `system.c:413-414` `RTS_SET`）。
- 单元测试 `test_dispatch_exit_sets_sigabrt` 验证三处状态均被正确写入。
- **遗留**：信号管理器通知（`mini_notify(sig_mgr, ...)`）仍依赖 P0-02 IPC 与 P1-08 SignalContext trait。当前实现确保后续 `do_getksig()` 轮询能观察到该信号，但不主动通知 sig_mgr — 等待 P1-08 一并接通。

### 2.4 SYS_CLEAR — do_clear.c

**关键操作**：
1. `release_address_space(rc)` — 释放地址空间
2. 检查并释放 IRQ hooks
3. `clear_endpoint(rc)` — 清除 IPC 端点
4. `reset_kernel_timer(&priv(rc)->s_alarm_timer)` — 清除 alarm
5. `RTS_SETFLAGS(rc, RTS_SLOT_FREE)` — 标记槽位空闲
6. 释放 FPU
7. 如果是 SYS_PROC，释放权限结构

### 2.5 SYS_RUNCTL — do_runctl.c

**消息字段**：
- `RC_ENDPT`：目标进程
- `RC_ACTION`：RC_STOP 或 RC_RESUME
- `RC_FLAGS`：RC_DELAY 标志

**关键操作**：
1. RC_STOP + RC_DELAY：如果进程正在发送消息，设置 MF_SIG_DELAY
2. RC_STOP：设置 RTS_PROC_STOP（SMP 时可能需要 IPI）
3. RC_RESUME：清除 RTS_PROC_STOP

### 2.6 SYS_SCHEDCTL — do_schedctl.c

**消息字段**：
- `m_lsys_krn_schedctl.flags`：SCHEDCTL_FLAG_KERNEL
- `m_lsys_krn_schedctl.endpoint`：目标进程
- `m_lsys_krn_schedctl.priority`：优先级
- `m_lsys_krn_schedctl.quantum`：时间片
- `m_lsys_krn_schedctl.cpu`：CPU 亲和

**关键操作**：
1. 如果 SCHEDCTL_FLAG_KERNEL：调用 `sched_proc()` 设置参数，`p_scheduler = NULL`
2. 否则：`p_scheduler = caller`

### 2.7 SYS_STATECTL — do_statectl.c

**请求类型**：
- `SYS_STATE_CLEAR_IPC_REFS`：清除 IPC 引用
- `SYS_STATE_SET_STATE_TABLE`：设置状态表
- `SYS_STATE_ADD_IPC_BL_FILTER`：添加 IPC 黑名单过滤
- `SYS_STATE_ADD_IPC_WL_FILTER`：添加 IPC 白名单过滤
- `SYS_STATE_CLEAR_IPC_FILTERS`：清除 IPC 过滤器

---

## 3. Rust 设计决策

| # | 决策 | 选项 | 结论 | 理由 |
|---|------|------|------|------|
| D1 | Fork proc 复制 | `*rpc = *rpp` | **`clone_from()`** | Rust 语义明确 |
| D2 | Endpoint generation | 内联计算 | **`Endpoint::from_generation_slot()`** | 复用已有方法 |
| D3 | Fork 返回值 | 设置 retreg=0 | **`p_reg.retreg = 0`** | C 行为对齐 |
| D4 | Exit 信号 | `cause_sig()` | **调用信号子系统** | 解耦 |
| D5 | Clear 资源释放 | 多个独立操作 | **按序调用各子系统** | 模块化 |
| D6 | Runctl SMP | `smp_schedule_stop_proc()` | **条件调用 SMP** | 单核/多核兼容 |
| D7 | Statectl 子请求 | switch/case | **enum + match** | 类型安全 |
| D8 | 错误码 | C errno | **`KcallResult::Ok(errno)`** | 保留 C 语义 |

---

## 4. 实现详解

### 4.1 SyscallProcess — 进程管理系统调用实现

```rust
/// Process management syscall implementation.
pub struct SyscallProcess;

impl SyscallProcess {
    pub fn do_fork(caller: &mut KProcess, msg: &Message) -> KcallResult { ... }
    pub fn do_exec(caller: &mut KProcess, msg: &Message, proc_table: &mut ProcessTable) -> KcallResult { ... }
    pub fn do_exit(caller: &mut KProcess, msg: &Message) -> KcallResult { ... }
    pub fn do_clear(caller: &mut KProcess, msg: &Message, proc_table: &mut ProcessTable, priv_table: &mut PrivTable) -> KcallResult { ... }
    pub fn do_runctl(caller: &mut KProcess, msg: &Message, proc_table: &mut ProcessTable) -> KcallResult { ... }
    pub fn do_schedctl(caller: &mut KProcess, msg: &Message) -> KcallResult { ... }
    pub fn do_statectl(caller: &mut KProcess, msg: &Message) -> KcallResult { ... }
}
```

---

## 5. 测试要点

1. **Fork**：子进程 endpoint generation 递增，返回值为 0
2. **Fork 特权**：SYS_PROC 父进程 → 子进程降级
3. **Exec**：清除目标进程（非 caller）的 DELIVERMSG/RECEIVING/EXT_REG；endpoint 校验
4. **Exit**：返回 NoReply
5. **Clear**：操作目标进程（非 caller）的 SLOT_FREE + EXT_REG + privilege slot；endpoint 校验；isemptyp 提前返回
6. **Runctl**：停止/恢复 RTS_PROC_STOP（操作 RC_ENDPT 目标进程，非 caller）；iskerneln→EPERM
7. **Schedctl**：设置调度参数

---

## 6. 补充：Fork/Exec 详细分析

> 来源：tmp-14-syscall-fork-exec.md

### 6.1 fork 的同步性要求

`do_fork()` 要求父进程必须处于 `RTS_RECEIVING` 状态（正在接收消息）。这是因为 fork 需要知道父进程的消息缓冲区地址（`p_delivermsg_vir`），以便将子进程的 endpoint 传递给 PM。若父进程不在接收状态，`do_fork()` 返回 `EINVAL`。

### 6.2 子进程的初始不可运行状态

fork 后子进程被设置 `RTS_NO_QUANTUM`（无时间片）和可能的 `RTS_NO_PRIV`（无特权）及 `RTS_VMINHIBIT`（等待 VM 设置页表）。这些标志确保子进程在 PM 和 VM 完成初始化前不会运行。

### 6.3 系统进程 fork 的特权降级

若父进程是系统进程（`s_flags & SYS_PROC`），子进程被降级为用户进程（`p_priv = priv_addr(USER_PRIV_ID)`），并设置 `RTS_NO_PRIV`。PM 需要在 exec 前通过 `sys_privctl` 重新设置特权。

### 6.4 exec 的无回复语义

`do_exec()` 不回复调用方（不写回返回消息）。这是因为 exec 后进程的整个地址空间已被替换，原来的消息缓冲区不再有效。PM 通过其他机制（如通知）确认 exec 完成。

### 6.5 sys_fork 消息字段

| 字段宏 | 含义 |
|--------|------|
| `m_lsys_krn_sys_fork.endpt` | 父进程 endpoint |
| `m_lsys_krn_sys_fork.slot` | 子进程槽位号 |
| `m_lsys_krn_sys_fork.flags` | fork 标志（PFF_VMINHIBIT 等） |
| `m_krn_lsys_sys_fork.endpt` | 返回：子进程 endpoint |
| `m_krn_lsys_sys_fork.msgaddr` | 返回：父进程消息缓冲区地址 |

### 6.6 sys_exec 消息字段

| 字段宏 | 含义 |
|--------|------|
| `m_lsys_krn_sys_exec.endpt` | 目标进程 endpoint |
| `m_lsys_krn_sys_exec.stack` | 新栈指针 |
| `m_lsys_krn_sys_exec.name` | 程序名指针 |
| `m_lsys_krn_sys_exec.ip` | 新指令指针（入口点） |
| `m_lsys_krn_sys_exec.ps_str` | ps_strings 结构指针 |

### 6.7 fork 标志

| 标志 | 含义 |
|------|------|
| `PFF_VMINHIBIT` | 子进程需要等待 VM 设置页表后才能运行 |

### 6.8 fork 行为规则

1. 父进程必须接收中：`do_fork()` 要求 `RTS_ISSET(rpp, RTS_RECEIVING)` 为真
2. 子进程槽位必须空闲：`isemptyp(rpc)` 必须为真
3. Generation 递增：子进程的 endpoint generation 从槽位当前值递增 1，回绕到 1
4. 子进程返回值 0：`rpc->p_reg.retreg = 0`
5. 子进程不可运行：fork 后子进程有 `RTS_NO_QUANTUM`
6. VMINHIBIT 条件设置：若 fork flags 含 `PFF_VMINHIBIT`，子进程设置 `RTS_VMINHIBIT`
7. exec 不回复：`do_exec()` 清除 `RTS_RECEIVING` 但不写回返回消息
8. exec 清除 FPU 状态：exec 后 FPU 标记为未初始化

---

## 7. 参见

- [11-scheduling-primitives.md](11-scheduling-primitives.md) — sched_proc()
- [16-smp.md](16-smp.md) — 跨 CPU 停止进程
- [19-syscall-signal.md](19-syscall-signal.md) — cause_sig()
- [22-privilege.md](22-privilege.md) — 权限降级
