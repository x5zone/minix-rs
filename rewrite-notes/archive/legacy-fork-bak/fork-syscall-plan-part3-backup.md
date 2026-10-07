# Fork 系统调用纵向切片重构计划 — Part 3：附录与参考

> **版本**: v2.0
> **日期**: 2026-04-08
> **范围**: 附录、源码对应关系、常量参考、技术决策记录

---

## 附录 A：Minix3 源码文件对应关系

### A.1 PM 服务源码

| C 文件 | 功能 | Rust 文件 | 状态 |
|--------|------|-----------|------|
| `servers/pm/mproc.h` | mproc 结构体定义 | `os/servers/pm/src/mproc/mproc.rs` | ✅ 已实现 |
| `servers/pm/forkexit.c` | fork/exit/wait 实现 | `os/servers/pm/src/mproc/fork.rs` | 🔧 部分实现 |
| `servers/pm/utility.c` | get_free_pid, find_proc 等 | `os/servers/pm/src/mproc/table.rs` | 🔧 部分实现 |
| `servers/pm/main.c` | PM 主循环、初始化 | `os/servers/pm/src/main.rs` | 📋 占位 |
| `servers/pm/signal.c` | 信号处理 | `os/servers/pm/src/mproc/signal.rs` | 🔧 部分实现 |
| `servers/pm/trace.c` | ptrace 追踪 | `os/servers/pm/src/mproc/trace.rs` | 🔧 部分实现 |
| `servers/pm/getset.c` | getuid/setuid 等 | — | 📋 未开始 |
| `servers/pm/exec.c` | exec 系统调用 | `os/servers/pm/src/exec.rs` | 📋 占位 |
| `servers/pm/const.h` | PM 常量定义 | `os/libs/minix-types/src/types/pid.rs` | 🔧 部分实现 |
| `servers/pm/pm.h` | PM 主头文件 | `os/servers/pm/src/lib.rs` | ✅ 已实现 |
| `servers/pm/glo.h` | PM 全局变量 | `os/servers/pm/src/mproc/table.rs` | ✅ 已实现 |

### A.2 内核源码

| C 文件 | 功能 | Rust 文件 | 状态 |
|--------|------|-----------|------|
| `kernel/proc.h` | proc 结构体定义 | `os/kernel/src/proc.rs` | 📋 占位 |
| `kernel/proc.c` | 进程管理 | — | 📋 未开始 |
| `include/minix/endpoint.h` | Endpoint 定义 | `os/libs/minix-types/src/types/pid.rs` | ✅ 已实现 |
| `include/minix/com.h` | 通信常量 | `os/libs/minix-types/src/types/pid.rs` | ✅ 已实现 |

### A.3 其他服务源码

| C 文件 | 功能 | Rust 文件 | 状态 |
|--------|------|-----------|------|
| `servers/vm/vmproc.h` | vmproc 结构体 | — | 📋 未开始 |
| `servers/vm/fork.c` | VM fork 实现 | — | 📋 未开始 |
| `servers/vfs/fproc.h` | fproc 结构体 | — | 📋 未开始 |

---

## 附录 B：Minix3 常量参考

### B.1 PM 常量（`servers/pm/const.h`）

| 常量 | C 值 | Rust 常量 | Rust 值 | 状态 |
|------|------|----------|---------|------|
| `NR_PIDS` | 30000 | `NR_PIDS` | — | ❌ 未定义 |
| `NO_PID` | 0 | `NO_PID` | — | ❌ 未定义 |
| `INIT_PID` | 1 | `INIT_PID` | — | ❌ 未定义 |
| `NO_TRACER` | 0 | `NO_TRACER` | `usize::MAX` | ⚠️ 值不同 |
| `NR_ITIMERS` | 3 | `NR_ITIMERS` | 3 | ✅ |
| `LAST_FEW` | 2 | `LAST_FEW` | 5 | ⚠️ 值不同 |

**⚠️ 需要修正的常量**：

1. **`NO_TRACER`**: C 中为 `0`，Rust 中为 `usize::MAX`。Minix3 中 `mp_tracer` 是进程表索引，`0` 表示无追踪者（因为 0 号进程是 INIT，不会被追踪）。Rust 用 `usize::MAX` 更安全，但需要确认逻辑一致性。

2. **`LAST_FEW`**: C 源码 `forkexit.c` 中 `#define LAST_FEW 2`，但 `minix-types` 中定义为 5。需要确认哪个值是正确的。

3. **`NR_PIDS`、`INIT_PID`、`NO_PID`**: 尚未在 Rust 中定义，需要在 `minix-types` 中添加。

### B.2 Endpoint 常量（`include/minix/endpoint.h`）

| 常量 | C 值 | Rust 对应 | 说明 |
|------|------|----------|------|
| `_ENDPOINT_GENERATION_SHIFT` | 15 | `ENDPOINT_GENERATION_SHIFT` | ✅ 已实现 |
| `_ENDPOINT_GENERATION_SIZE` | `1 << 15` = 32768 | — | ❌ 未定义 |
| `_ENDPOINT_MAX_GENERATION` | `INT_MAX/32768-1` = 65535 | — | ❌ 未定义 |
| `ANY` | `_ENDPOINT_SLOT_TOP - 1` | — | ❌ 未定义 |
| `NONE` | `_ENDPOINT_SLOT_TOP - 2` | `Endpoint::NONE` = 0 | ⚠️ 值不同 |
| `SELF` | `_ENDPOINT_SLOT_TOP - 3` | — | ❌ 未定义 |
| `MAX_NR_PROCS` | `_ENDPOINT_SLOT_TOP - 3` | `NR_PROCS` = 256 | ⚠️ 需确认 |

### B.3 mproc 标志位（`servers/pm/mproc.h`）

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

**未映射的标志位**：
- `SIGSUSPENDED`：信号挂起状态
- `VFS_CALL`：等待 VFS 回复（关键！fork/exit 都需要）
- `UNPAUSED`：VFS 已回复 unpause 请求
- `DELAY_CALL`：等待延迟调用
- `EVENT_CALL`：等待事件订阅者

---

## 附录 C：do_fork 完整流程对照表

基于 `minix3/minix/servers/pm/forkexit.c` 的 `do_fork()` 函数，逐步对照：

| 步骤 | C 代码 | Rust 实现 | 状态 |
|------|--------|----------|------|
| ① 检查进程表 | `procs_in_use == NR_PROCS \|\| ...` | `table.is_full() / can_alloc()` | ✅ |
| ② 查找空闲槽位 | `do { next_child++ } while (IN_USE)` | `table.find_free_slot()` | ✅ |
| ③ 调用 vm_fork | `vm_fork(endpoint, next_child, &child_ep)` | — | ❌ |
| ④ 增加计数 | `procs_in_use++` | `table.alloc_slot()` | ✅ |
| ⑤ 复制 mproc | `*rmc = *rmp` | `Process::fork_from()` | 🔧 |
| ⑥ 恢复 sigact | `rmc->mp_sigact = mpsigact[...]` | `signals.clone()` | ✅ |
| ⑦ 设置父进程 | `rmc->mp_parent = who_p` | `guardianship.parent` | ✅ |
| ⑧ 清除追踪器 | `rmc->mp_tracer = NO_TRACER` | `trace = default()` | ✅ |
| ⑨ 特权进程 | `mp_scheduler = SCHED_PROC_NR` | — | ❌ |
| ⑩ 重置标志 | `mp_flags &= (IN_USE\|DELAY_CALL\|TAINTED)` | 🔧 部分实现 | 🔧 |
| ⑪ 分配 PID | `get_free_pid()` | `generate_child_pid()` | 🔧 简化版 |
| ⑫ 通知 VFS | `tell_vfs(VFS_PM_FORK)` | — | ❌ |
| ⑬ 追踪 SIGSTOP | `sig_proc(SIGSTOP)` | — | ❌ |
| ⑭ 返回 SUSPEND | `return SUSPEND` | — | ❌ |

---

## 附录 D：技术决策记录

### D.1 为什么 MProc 放在 PM crate 而不是 minix-types？

1. **职责隔离**: MProc 包含大量仅 PM 关心的私有逻辑（信号处理、父子进程树等）
2. **不变量保护**: 状态转换逻辑绑定了 PM 内部复杂逻辑，放在公共库会破坏不变量
3. **微内核原则**: 遵循"知识最小化"原则，其他服务不需要了解 PM 的内部实现

### D.2 为什么 Generation 嵌入 Endpoint 而不是单独数组？

1. **唯一 truth**: 避免多处维护同一个值，否则会导致一致性地狱
2. **Minix3 原始设计**: Generation 本来就是 Endpoint 的一部分，不是独立存储
3. **验证简单**: 只需比较 Endpoint 是否相等，无需额外查找

### D.3 为什么使用分层设计而不是扁平结构？

1. **字段归类**: mproc 有 40+ 个字段，扁平结构难以管理
2. **状态机隔离**: 生命周期、阻塞、等待等状态有独立的转换规则
3. **部分复制**: fork 时某些层整体复制，某些层整体清零

### D.4 为什么使用 Enum 而不是 Bitflags 表示生命周期？

1. **互斥性**: 生命周期状态是互斥的（进程不可能同时是 Running 和 Zombie）
2. **穷尽匹配**: Rust 的 `match` 强制处理所有情况
3. **类型安全**: 不可能出现非法的状态组合

---

## 附录 E：待修复项清单

| 编号 | 项目 | 优先级 | 说明 |
|------|------|--------|------|
| E-1 | `LAST_FEW` 值不一致 | 高 | C 源码 2，Rust 定义 5 |
| E-2 | `NO_TRACER` 值不一致 | 中 | C 源码 0，Rust usize::MAX |
| E-3 | 缺少 `NR_PIDS` 常量 | 高 | PID 生成器需要 |
| E-4 | 缺少 `INIT_PID` 常量 | 高 | PID 生成器需要 |
| E-5 | `Endpoint::NONE` 值不一致 | 中 | C 源码非 0，Rust 为 0 |
| E-6 | `fork_from` 中 `id.index` 错误 | 高 | 应为 child_index |
| E-7 | `fork_from` 中 `intervals` 未清零 | 中 | 应为 `[0; 3]` |
| E-8 | `fork_from` 中 `flags` 未过滤 | 中 | 应只保留 TAINTED |
| E-9 | 缺少 `VFS_CALL` 标志映射 | 高 | fork/exit 都需要 |
| E-10 | `generate_child_pid` 无冲突检测 | 高 | 阶段 3 实现 |
| E-11 | Endpoint 由 PM 计算 | 中 | 应由 VM/Kernel 返回 |
