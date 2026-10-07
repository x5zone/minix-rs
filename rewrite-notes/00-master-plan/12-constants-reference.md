# Minix3 常量参考

> 本文档记录 Minix3 C 源码中的关键常量及其 Rust 实现对应关系。

---

## PM 常量（`servers/pm/const.h`）

| 常量 | C 值 | Rust 常量 | Rust 值 | 状态 | 说明 |
|------|------|----------|---------|------|------|
| `NR_PIDS` | 30000 | `NR_PIDS` | — | ❌ 未定义 | PID 空间大小 |
| `NO_PID` | 0 | `NO_PID` | — | ❌ 未定义 | 无效 PID |
| `INIT_PID` | 1 | `INIT_PID` | — | ❌ 未定义 | INIT 进程 PID |
| `NO_TRACER` | 0 | `NO_TRACER` | `usize::MAX` | ⚠️ 值不同 | 无追踪者标记 |
| `NR_ITIMERS` | 3 | `NR_ITIMERS` | 3 | ✅ | 间隔定时器数量 |
| `LAST_FEW` | 2 | `LAST_FEW` | 5 | ⚠️ 值不同 | PID 保留数量 |

### ⚠️ 需要修正的常量

1. **`NO_TRACER`**: C 中为 `0`，Rust 中为 `usize::MAX`。Minix3 中 `mp_tracer` 是进程表索引，`0` 表示无追踪者（因为 0 号进程是 INIT，不会被追踪）。Rust 用 `usize::MAX` 更安全，但需要确认逻辑一致性。

2. **`LAST_FEW`**: C 源码 `forkexit.c` 中 `#define LAST_FEW 2`，但 `minix-types` 中定义为 5。需要确认哪个值是正确的。

3. **`NR_PIDS`、`INIT_PID`、`NO_PID`**: 尚未在 Rust 中定义，需要在 `minix-types` 中添加。

---

## Endpoint 常量（`include/minix/endpoint.h`）

| 常量 | C 值 | Rust 对应 | 说明 |
|------|------|----------|------|
| `_ENDPOINT_GENERATION_SHIFT` | 15 | `ENDPOINT_GENERATION_SHIFT` | ✅ 已实现 |
| `_ENDPOINT_GENERATION_SIZE` | `1 << 15` = 32768 | — | ❌ 未定义 |
| `_ENDPOINT_MAX_GENERATION` | `INT_MAX/32768-1` = 65535 | — | ❌ 未定义 |
| `ANY` | `_ENDPOINT_SLOT_TOP - 1` | — | ❌ 未定义 |
| `NONE` | `_ENDPOINT_SLOT_TOP - 2` | `Endpoint::NONE` = 0 | ⚠️ 值不同 |
| `SELF` | `_ENDPOINT_SLOT_TOP - 3` | — | ❌ 未定义 |
| `MAX_NR_PROCS` | `_ENDPOINT_SLOT_TOP - 3` | `NR_PROCS` = 256 | ⚠️ 需确认 |

---

## mproc 标志位（`servers/pm/mproc.h`）

| 标志位 | C 值 | Rust 对应 | 状态 |
|--------|------|----------|------|
| `IN_USE` | 0x00001 | `Lifecycle::is_in_use()` | ✅ |
| `WAITING` | 0x00002 | `WaitState` | ✅ |
| `ZOMBIE` | 0x00004 | `Lifecycle::Zombie` | ✅ |
| `PROC_STOPPED` | 0x00008 | `BlockState::stopped` | ✅ |
| `ALARM_ON` | 0x00010 | `RemainingFlags::ALARM_ON` | ✅ |
| `EXITING` | 0x00020 | `Lifecycle::Exiting` | ✅ |
| `TOLD_PARENT` | 0x00040 | `Lifecycle::ToldParent` | ✅ |
| `TRACE_STOPPED` | 0x00080 | `TraceState::stopped` | ✅ |
| `SIGSUSPENDED` | 0x00100 | — | ❌ 未映射 |
| `VFS_CALL` | 0x00400 | — | ❌ 未映射 |
| `NEW_PARENT` | 0x00800 | `RemainingFlags::NEW_PARENT` | ✅ |
| `UNPAUSED` | 0x01000 | — | ❌ 未映射 |
| `PRIV_PROC` | 0x02000 | `Privilege::Kernel` | ✅ |
| `PARTIAL_EXEC` | 0x04000 | `RemainingFlags::PARTIAL_EXEC` | ✅ |
| `TRACE_EXIT` | 0x08000 | `Guardianship::trace_exit` | ✅ |
| `TRACE_ZOMBIE` | 0x10000 | `Lifecycle::TraceZombie` | ✅ |
| `DELAY_CALL` | 0x20000 | — | ❌ 未映射 |
| `TAINTED` | 0x40000 | `RemainingFlags::TAINTED` | ✅ |
| `EVENT_CALL` | 0x80000 | — | ❌ 未映射 |

### 未映射的标志位说明

- `SIGSUSPENDED`：信号挂起状态
- `VFS_CALL`：等待 VFS 回复（关键！fork/exit 都需要）
- `UNPAUSED`：VFS 已回复 unpause 请求
- `DELAY_CALL`：等待延迟调用
- `EVENT_CALL`：等待事件订阅者

---

## IPC 消息类型常量（`include/minix/com.h`）

| 常量 | C 值 | Rust 对应 | 说明 |
|------|------|----------|------|
| `VM_FORK` | `VM_RQ_BASE+1` | `VM_FORK` | PM → VM fork 请求 |
| `VFS_PM_FORK` | `VFS_PM_RQ_BASE+7` | `VFS_PM_FORK` | PM → VFS fork 通知 |
| `VFS_PM_FORK_REPLY` | `VFS_PM_RS_BASE+7` | — | VFS → PM fork 回复 |
| `SYS_FORK` | `SYS_RQ_BASE+...` | — | VM → Kernel fork 请求 |

---

## Fork 标志（`include/minix/com.h`）

| 标志 | C 值 | Rust 对应 | 说明 |
|------|------|----------|------|
| `PFF_VMINHIBIT` | 0x01 | — | 抑制 VM 的 fork 处理 |
