# 29-kernel-debug: 内核调试基础设施

> **分类**: 调试基础设施（部分实现）
> **源码**: `minix3/minix/kernel/debug.c` (~563 行)
> **关联 Rust**: `os/kernel/src/debug.rs`（`runqueues_ok_cpu` / `runqueues_ok` / `write_rts_flags` / `write_misc_flags` / `print_proc` 已实现；IPC hook/统计 不实现）
> **前置**: [11-scheduling-primitives.md](11-scheduling-primitives.md), [12-ipc-core.md](12-ipc-core.md), [16-smp.md](16-smp.md)
> **C 总行数**: ~563 行

---

## Ch1: 概念

**核心问题**: 内核是系统中最关键的组件。当调度队列不一致、IPC 死锁或进程状态机异常时，内核如何自我诊断？用户态进程卡死可以由内核检测并杀死，但内核自身的调度队列错误谁来发现？

这是内核可靠性的元问题。CPU 提供了调试寄存器、断点指令等硬件辅助，但软件层面的"调度队列一致性验证"和"IPC 路径跟踪"需要内核主动实现。Minix3 在 `debug.c` 中提供了四类调试功能，全部通过条件编译控制——仅在调试构建中启用。

### 1.1 调试基础设施的四类功能

| 功能组 | 条件编译 | 核心函数 | 用途 |
|--------|---------|---------|------|
| 调度队列 sanity check | `CONFIG_SMP` | `runqueues_ok_cpu` / `runqueues_ok_all` / `runqueues_ok` | 验证 runqueue head/tail/nextready 一致性 |
| 进程信息打印 | （无条件） | `print_proc` / `print_proc_recursive` | 打印进程详情 + 依赖链 + 栈回溯 |
| IPC 消息跟踪 | `DEBUG_DUMPIPC` / `DEBUG_IPC_HOOK` | `hook_ipc_msgsend` / `hook_ipc_msgrecv` 等 | hook IPC 路径，打印消息流向 |
| IPC 统计 | `DEBUG_IPCSTATS` / `DEBUG_IPC_HOOK` | `printstats` / `sortstats` / `statmsg` | 统计消息频率，识别热点路径 |

### 1.2 条件编译模型

Minix3 的调试功能通过预处理宏控制，生产构建中完全不编译：

- `CONFIG_SMP`: 启用 SMP 调度队列检查（`runqueues_ok_all` 遍历所有 CPU）
- `DEBUG_DUMPIPC` / `DEBUG_DUMPIPCF`: 启用 IPC 消息打印（`mtypename` 解析消息类型 + `printmsg` 格式化输出）
- `DEBUG_IPCSTATS`: 启用 IPC 统计（`messages[NR_PROCS+1][NR_PROCS+1]` 矩阵 + `winners[20]` 排序）
- `DEBUG_IPC_HOOK`: 启用 IPC hook 回调（5 个 hook 函数在 IPC 路径中被调用）

这种模型的问题是：调试代码与生产代码混在同一文件，条件编译增加阅读复杂度，且 hook 函数的空实现仍需维护。

### 1.3 redox 对照

- **redox**: 无内核调试 hook——scheme 模型，每个 scheme 自管理日志，内核最简。调试通过 scheme 日志 + 外部工具（`perf`、`gdb`）完成。
- **Minix3**: 内核内调试 hook + 条件编译——`debug.c` 提供 4 类功能，深度集成到调度/IPC 代码路径。
- **minix-rs**: 部分实现——`os/kernel/src/debug.rs` 已实现调度队列 sanity check（`runqueues_ok_cpu` / `runqueues_ok`）和进程信息打印（`write_rts_flags` / `write_misc_flags` / `print_proc`）；IPC 消息跟踪与统计（hook 函数 + `messages[][]` 矩阵）WONTFIX，由 `log` crate + 外部工具替代。Rust 类型系统（所有权 + Option + enum 穷尽性）在编译期防止大部分队列错误，运行时用 `debug_assert!` + `runqueues_ok` 双重保障。

### 1.4 本章不讲什么

- 调度队列正常逻辑（见 [11-scheduling-primitives.md](11-scheduling-primitives.md)）
- IPC 核心逻辑（见 [12-ipc-core.md](12-ipc-core.md)）
- SMP 机制（见 [16-smp.md](16-smp.md)）

---

## Ch2: C 源码分析

### 2.1 文件清单

| 文件 | 行数 | 核心内容 |
|------|------|---------|
| [debug.c](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/debug.c) | ~563 | 17 函数 + 4 宏，分 4 类功能 |

### 2.2 调度队列 sanity check

[debug.c:16-134](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/debug.c) 实现调度队列一致性验证：

**`runqueues_ok_cpu(cpu)`**（L16-107）验证单个 CPU 的调度队列：
1. 初始化所有进程的 `p_found = 0`
2. 遍历 `NR_SCHED_QUEUES` 个队列，检查 head/tail 指针一致性
3. 检查 `tail->p_nextready` 是否为 NULL（tail 的 next 必须为空）
4. 遍历每个队列的链表，验证 `p_nextready` 链表完整性
5. 检查进程地址范围（`BEG_PROC_ADDR` 到 `END_PROC_ADDR`）
6. 检查每个 runnable 进程是否在某个队列中

**`runqueues_ok_all()`**（L110-119）遍历所有 CPU 调用 `runqueues_ok_cpu`（SMP 条件编译）。

**`runqueues_ok()`**（L121-131）入口函数，SMP 模式调用 `runqueues_ok_all`，UP 模式调用 `runqueues_ok_cpu(0)`。

### 2.3 进程信息打印

[debug.c:136-312](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/debug.c) 实现进程详情打印：

**辅助函数**：
- `rtsflagstr(flags)`（L136-161）: 将 RTS 标志位转为字符串（RTS_SLOT_FREE / RTS_PROC_STOP / RTS_SENDING 等 15 个标志）
- `miscflagstr(flags)`（L163-174）: 将 misc 标志位转为字符串（MF_REPLY_PEND / MF_DELIVERMSG / MF_KCALL_RESUME）
- `schedulerstr(scheduler)`（L176-185）: 返回调度器名称或 "KERNEL"
- `print_proc_name(pp)`（L187-199）: 打印 `name(endpoint)` 格式
- `print_endpoint(ep)`（L201-232）: 处理 ANY/SELF/NONE 特殊值
- `print_sigmgr(pp)`（L234-247）: 打印信号管理器

**`print_proc(pp)`**（L249-275）: 打印进程详情，格式：
```
nr: name endpoint prio time user/sys cycles high:low cpu pdbr rts misc sched sigmgr blocked_on
```

**`print_proc_depends(pp, level)`**（L277-307）: 递归打印进程依赖链（被阻塞的进程）。`COL` 宏用 `>` 缩进表示依赖层级。递归上限 `NR_PROCS` 防止循环。

**`print_proc_recursive(pp)`**（L309-312）: 入口函数，从 level 0 开始递归。

### 2.4 IPC 消息跟踪

[debug.c:314-426](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/debug.c) `DEBUG_DUMPIPC` 条件编译块：

**`mtypename(mtype, possible_callname)`**（L315-354）: 解析消息类型名称，从 `extracted-mtype.h` 和 `extracted-errno.h` 匹配。

**`printproc(rp)`**（L356-362）: 打印 `name(slot)` 或 "kernel"。

**`printparam(name, data, size)`**（L364-373）: 按大小打印参数值（char/short/int/bytes）。

**`namematch(names, nnames, name)`**（L376-383）: `DEBUG_DUMPIPC_NAMES` 条件编译，名称匹配过滤。

**`printmsg(msg, src, dst, operation, printparams)`**（L386-425）: 格式化打印 IPC 消息：
```
operation src dst mtype(mtype_hex) [params...]
```
`operation` 字符: `s`=send, `r`=receive, `k`=kernel call/result。

### 2.5 IPC 统计

[debug.c:428-516](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/debug.c) `DEBUG_IPCSTATS` 条件编译块：

**全局状态**：
- `messages[IPCPROCS][IPCPROCS]`（L431）: 消息计数矩阵，`IPCPROCS = NR_PROCS+1`
- `winners[PRINTSLOTS]`（L434-436）: 前 20 名统计排行，每项含 `src/dst/messages`
- `total` / `goodslots`（L437）: 总消息数 + 有效排行数

**`printstats(ticks)`**（L439-451）: 打印统计结果，每秒消息数 = `system_hz * n / ticks`。

**`sortstats()`**（L453-485）: 遍历矩阵，插入排序到 `winners[]`，维护前 20 名。

**`statmsg(msg, srcp, dstp)`**（L493-515）: 统计单条消息，每 30 秒打印一次统计并重置。

### 2.6 IPC hooks

[debug.c:518-563](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/debug.c) `DEBUG_IPC_HOOK` 条件编译块：

5 个 hook 函数在 IPC 路径中被调用：

| Hook | 触发时机 | 行为 |
|------|---------|------|
| `hook_ipc_msgkcall(msg, proc)` | 内核调用 | `printmsg(msg, proc, NULL, 'k', 1)` |
| `hook_ipc_msgkresult(msg, proc)` | 内核调用返回 | `printmsg(msg, NULL, proc, 'k', 0)` + `statmsg` |
| `hook_ipc_msgrecv(msg, src, dst)` | 接收消息 | `printmsg(msg, src, dst, 'r', 0)` + `statmsg` |
| `hook_ipc_msgsend(msg, src, dst)` | 发送消息 | `printmsg(msg, src, dst, 's', 1)` |
| `hook_ipc_clear(p)` | 进程清理 | 清零该进程的统计行/列 |

### 2.7 宏定义

| 宏 | 位置 | 值 | 用途 |
|----|------|-----|------|
| `MAX_LOOP` | debug.c:14 | `NR_PROCS + NR_TASKS` | 进程表遍历上限（防无限循环） |
| `IPCPROCS` | debug.c:429 | `NR_PROCS+1` | IPC 统计矩阵维度（+1 为 kernel 槽） |
| `KERNELIPC` | debug.c:430 | `NR_PROCS` | 内核调用槽位编号 |
| `PRINTSLOTS` | debug.c:433 | `20` | 统计排行榜大小 |

---

## Ch3: 设计决策

### 3.1 D1: 调度队列 sanity check 实现

**C 行为**: `runqueues_ok_cpu` / `runqueues_ok_all` / `runqueues_ok` 验证调度队列一致性。

**Rust 64-bit 决策**: 实现（`os/kernel/src/debug.rs`）。

**实现内容**:
- `runqueues_ok_cpu(smp_state, proc_table, cpu) -> bool`（[debug.rs:47](file:///home/xzhao/github/minix-rs/os/kernel/src/debug.rs)）对应 C `runqueues_ok_cpu(cpu)`（[debug.c:16-107](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/debug.c)）
- `runqueues_ok(smp_state, proc_table) -> bool`（[debug.rs:190](file:///home/xzhao/github/minix-rs/os/kernel/src/debug.rs)）对应 C `runqueues_ok()`（[debug.c:121-131](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/debug.c)）

**实现差异**:
1. **`p_found` 改为本地 bitset**: C 在 `proc.p_found` 字段上操作（修改进程表）；Rust 使用本地 `[bool; PROC_TABLE_SIZE]` 数组避免修改进程表（保持只读检查的纯度）
2. **`MAX_LOOP` 常量**: `PROC_TABLE_SIZE + 16`（C: `NR_PROCS + NR_TASKS`，语义等价）
3. **错误输出走 EarlyConsole**: 不依赖 `printf`，直接 `Console::write_str/write_hex` 输出到 early console
4. **类型安全**: `ProcNr` newtype 替代 C 的 `int` 进程号；`Option<ProcNr>` 替代 NULL 指针

**理由**:
1. Rust 类型系统虽在编译期防止大部分错误，但运行时仍需检测 SMP 调度队列不变量（head/tail 一致性、双调度、可运行进程在队列中）
2. `debug_assert!` 适合简单断言，复杂不变量（遍历链表 + 多队列交叉验证）需要专门函数
3. 此函数用于 panic 时的诊断输出，与 panic message 配合快速定位问题

### 3.2 D2: 进程信息打印部分实现

**C 行为**: `print_proc` / `print_proc_recursive` 等用 `printf` 打印进程详情。

**Rust 64-bit 决策**: 部分实现——`print_proc` 已实现；`print_proc_depends` / `print_proc_recursive` 不实现。

**实现内容**:
- `write_rts_flags(flags)`（[debug.rs:208](file:///home/xzhao/github/minix-rs/os/kernel/src/debug.rs)）对应 C `rtsflagstr(flags)`（[debug.c:136-161](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/debug.c)）
- `write_misc_flags(flags)`（[debug.rs:238](file:///home/xzhao/github/minix-rs/os/kernel/src/debug.rs)）对应 C `miscflagstr(flags)`（[debug.c:163-174](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/debug.c)）
- `print_proc(proc)`（[debug.rs:258](file:///home/xzhao/github/minix-rs/os/kernel/src/debug.rs)）对应 C `print_proc(pp)`（[debug.c:249-275](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/debug.c)）

**实现差异**:
1. **直接 console 输出替代字符串返回**: C 用 `static char buf[]` 返回字符串（非线程安全）；Rust 直接走 `EarlyConsole::write_str`，无字符串分配
2. **简化字段**: Rust `print_proc` 输出 `nr: name ep= prio= rts=[...] misc=[...]`，省略 C 的 `time`/`cycles`/`cpu`/`pdbr`/`sched`/`sigmgr`/`blocked_on`（这些字段在 panic 路径非关键，避免依赖更多内部状态）

**不实现部分的理由**:
1. `print_proc_depends` / `print_proc_recursive` 依赖 IPC 阻塞图（`p_caller_q` 链表），该结构在 panic 路径可能已损坏，递归遍历风险高
2. 进程依赖链可通过 `tracing` crate 的 span 树或外部 gdb 调试获得

### 3.3 D3: IPC 消息跟踪不实现

**ARCH: WONTFIX** — 调试专用功能

**C 行为**: `hook_ipc_msg*` 5 个函数在 IPC 路径中条件编译插入。

**Rust 64-bit 决策**: 不实现。

**理由**:
1. `tracing` crate 提供更结构化的日志方案（span + event + subscriber）
2. 条件编译 hook 增加代码复杂度，与 IPC 核心逻辑耦合
3. 生产构建中 hook 函数为空实现，浪费维护成本

### 3.4 D4: IPC 统计不实现

**ARCH: WONTFIX** — 调试专用功能

**C 行为**: `printstats` / `sortstats` / `statmsg` 统计 IPC 消息频率。

**Rust 64-bit 决策**: 不实现。

**理由**:
1. 性能分析应用外部工具（`perf` / `flamegraph` / `dtrace`）
2. `messages[][]` 矩阵占用 `NR_PROCS^2` 内存，不适合嵌入式场景
3. 统计逻辑与内核核心逻辑无关，应外置

---

## Ch4: Rust 实现

### 4.1 已实现（[os/kernel/src/debug.rs](file:///home/xzhao/github/minix-rs/os/kernel/src/debug.rs)）

| C 符号 | C 位置 | Rust 实现 | 状态 |
|--------|--------|----------|------|
| `runqueues_ok_cpu` | debug.c:16-107 | `debug::runqueues_ok_cpu(smp_state, proc_table, cpu) -> bool` | ✅ Implemented (D1) |
| `runqueues_ok` | debug.c:121-131 | `debug::runqueues_ok(smp_state, proc_table) -> bool` | ✅ Implemented (D1) |
| `rtsflagstr` | debug.c:136-161 | `debug::write_rts_flags(flags)` | ✅ Implemented (D2) |
| `miscflagstr` | debug.c:163-174 | `debug::write_misc_flags(flags)` | ✅ Implemented (D2) |
| `print_proc` | debug.c:249-275 | `debug::print_proc(proc)` | ✅ Implemented (D2) |
| `MAX_LOOP` | debug.c:14 | `const MAX_LOOP: usize = PROC_TABLE_SIZE + 16` | ✅ Implemented |

#### 4.1.1 `runqueues_ok_cpu` 实现要点

[debug.rs:47-189](file:///home/xzhao/github/minix-rs/os/kernel/src/debug.rs) 实现单 CPU 调度队列验证：

```rust
pub fn runqueues_ok_cpu(
    smp_state: &SmpState,
    proc_table: &ProcessTable,
    cpu: CpuId,
) -> bool {
    let scheduler = match smp_state.cpu_local(cpu) {
        Some(cl) => &cl.scheduler,
        None => { /* error */ return false; }
    };
    let mut found = [false; PROC_TABLE_SIZE];  // C: p_found field

    for q in 0..priority::NR_SCHED_QUEUES {
        let head = scheduler.queue_head(q);
        let tail = scheduler.queue_tail(q);
        // ... 6 项检查：head/tail 一致性、tail->next NULL、SLOT_FREE、runnable、priority match、double sched
    }
    // ... 检查所有 runnable 进程都在某个队列中
    true
}
```

**关键不变量验证**（对应 C `debug.c:25-103`）:
1. head/tail 同时为 Some 或同时为 None
2. tail 进程的 `p_nextready` 必须为 `NONE_PROC_NR`
3. 队列上无 `SLOT_FREE` 进程
4. 队列上所有进程都是 runnable
5. 进程 priority 必须等于队列号
6. 同一进程不能出现在两个队列
7. 所有 runnable 进程必须在某个队列中

#### 4.1.2 `print_proc` 实现要点

[debug.rs:258-269](file:///home/xzhao/github/minix-rs/os/kernel/src/debug.rs) 实现：

```rust
pub fn print_proc(proc: &KProcess) {
    Console::write_hex(proc.p_nr.0 as u64);
    Console::write_str(": ");
    Console::write_str(proc.p_name.as_str());
    Console::write_str(" ep=");
    Console::write_hex(proc.p_endpoint.0 as u64);
    Console::write_str(" prio=");
    Console::write_hex(proc.p_sched.priority.load(...) as u64);
    Console::write_str(" rts=[");
    write_rts_flags(proc.p_rts_flags.load());
    Console::write_str("] misc=[");
    write_misc_flags(proc.p_misc_flags.load());
    Console::write_str("]\n");
}
```

**与 C 差异**: 省略 `time`/`cycles`/`cpu`/`pdbr`/`sched`/`sigmgr`/`blocked_on` 字段（panic 路径非关键）。

### 4.2 不实现（WONTFIX）

| C 符号 | C 位置 | 状态 | 理由 |
|--------|--------|------|------|
| `runqueues_ok_all` | debug.c:110 | ❌ N/A | Rust `runqueues_ok` 内部循环所有 CPU，等价于 C `runqueues_ok_all` |
| `print_proc_depends` | debug.c:277 | ❌ WONTFIX (D2) | 依赖可能已损坏的 `p_caller_q` 链表，panic 路径风险高 |
| `print_proc_recursive` | debug.c:309 | ❌ WONTFIX (D2) | 同上 |
| `printproc` | debug.c:356 | ❌ WONTFIX (D2) | `print_proc` 已覆盖 |
| `printparam` | debug.c:364 | ❌ WONTFIX (D3) | IPC 跟踪不实现 |
| `namematch` | debug.c:376 | ❌ WONTFIX (D3) | IPC 跟踪不实现 |
| `printstats` | debug.c:439 | ❌ WONTFIX (D4) | IPC 统计不实现 |
| `sortstats` | debug.c:453 | ❌ WONTFIX (D4) | IPC 统计不实现 |
| `statmsg` | debug.c:493 | ❌ WONTFIX (D4) | IPC 统计不实现 |
| `hook_ipc_msgkcall` | debug.c:519 | ❌ WONTFIX (D3) | IPC 跟踪不实现 |
| `hook_ipc_msgkresult` | debug.c:526 | ❌ WONTFIX (D3) | IPC 跟踪不实现 |
| `hook_ipc_msgrecv` | debug.c:536 | ❌ WONTFIX (D3) | IPC 跟踪不实现 |
| `hook_ipc_msgsend` | debug.c:546 | ❌ WONTFIX (D3) | IPC 跟踪不实现 |
| `hook_ipc_clear` | debug.c:553 | ❌ WONTFIX (D3) | IPC 跟踪不实现 |
| `IPCPROCS` | debug.c:429 | ❌ WONTFIX | IPC 统计不实现 |
| `KERNELIPC` | debug.c:430 | ❌ WONTFIX | IPC 统计不实现 |
| `PRINTSLOTS` | debug.c:433 | ❌ WONTFIX | IPC 统计不实现 |

### 4.3 替代方案对照

| C 功能 | Rust 实现/替代 |
|--------|--------------|
| 调度队列 sanity check | `debug::runqueues_ok` 实现 + `debug_assert!` 双重保障 |
| 进程信息打印 | `debug::print_proc` 实现（panic 路径）+ `Debug`/`Display` trait（开发期） |
| 进程依赖链 | 不实现（panic 路径风险），由 `tracing` span 树或外部 gdb 替代 |
| IPC 消息跟踪 | `tracing` crate（span + event） |
| IPC 统计 | 外部工具（`perf` / `flamegraph`） |

---

## Ch5: 测试

### 5.1 已有测试（[os/kernel/src/debug.rs:296-317](file:///home/xzhao/github/minix-rs/os/kernel/src/debug.rs)）

| 测试 | 位置 | 覆盖内容 | 状态 |
|------|------|---------|------|
| `test_write_rts_flags_empty` | debug.rs:296 | 空标志位不触发 panic | ✅ passing |
| `test_write_rts_flags_single` | debug.rs:303 | 单个 RTS 标志位输出正确 | ✅ ignored（需 logger） |
| `test_write_misc_flags_empty` | debug.rs:308 | 空标志位不触发 panic | ✅ passing |
| `test_write_misc_flags_single` | debug.rs:314 | 单个 misc 标志位输出正确 | ✅ ignored（需 logger） |

### 5.2 不需要测试（WONTFIX 项）

- IPC hook 函数测试（不实现）
- IPC 统计函数测试（不实现）
- `print_proc_depends` / `print_proc_recursive` 测试（不实现）

### 5.3 后续测试建议

- `runqueues_ok_cpu` 需要构造 `SmpState` + `ProcessTable` mock，当前未集成到单元测试；建议在 boot_integration 中添加 sanity check 集成测试

---

## Ch6: 跨文档引用

### 6.1 前序引用

- [11-scheduling-primitives.md](11-scheduling-primitives.md): 调度队列正常逻辑
- [12-ipc-core.md](12-ipc-core.md): IPC 核心逻辑（hook 插入点）
- [16-smp.md](16-smp.md): SMP 调度（`runqueues_ok_all` 遍历多 CPU）

