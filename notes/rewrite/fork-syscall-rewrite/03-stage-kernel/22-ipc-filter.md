# 22-ipc-filter: IPC 过滤

> **分类**: 运行时基础设施
> **源码**: `minix3/minix/kernel/ipc.h`, `minix3/minix/kernel/system.c:540-660`
> **前置**: 21（权限——s_ipc_to, s_k_call_mask）, 11（IPC 原语——send/receive/notify）
> **C 总行数**: ~200 行

---

## Ch1: 概念

**核心问题**: 内核如何根据权限位图过滤 IPC 调用？

IPC 过滤是 Minix3 权限模型的核心执行机制。每个系统调用和 IPC 目标都通过位图检查：

1. **系统调用过滤**：`s_k_call_mask` 控制进程可以调用哪些内核系统调用
2. **IPC 目标过滤**：`s_ipc_to` 控制进程可以向哪些进程发送消息
3. **通知过滤**：`s_notify_pending` 只允许接收已授权来源的通知

### 1.1 系统调用过滤

```c
// system.c:540-560
if (!kernel_call_mask[call_nr]) {
    return EPERM;
}
```

`s_k_call_mask` 是一个 `sys_map_t[NR_SYS_CALLS/64]` 位图。每个 bit 对应一个系统调用号。

### 1.2 IPC 目标过滤

```c
// ipc.h:check_ipc_to / priv.h:86
// may_send_to() 无条件检查 s_ipc_to 位图
if (!get_sys_bit(priv(rp)->s_ipc_to, priv(dest)->s_id)) {
    return EPERM;
}
```

> **注意**: Minix3 C 源码中**不存在** `CHECK_IPC` 标志。与 `CHECK_IO_PORT`/`CHECK_IRQ`/`CHECK_MEM` 不同，IPC 目标过滤是**无条件**执行的——`may_send_to()` 宏（priv.h:86）始终检查 `s_ipc_to` 位图，无需标志位启用。

### 1.3 通知过滤

```c
// notify 检查
if (get_sys_bit(priv(caller)->s_ipc_to, priv(dest)->s_id)) {
    // 允许
}
```

### 1.4 过滤时机

| 操作 | 过滤检查 | 位置 |
|------|---------|------|
| 内核调用 | `s_k_call_mask` | `kernel_call()` 入口 |
| send | `s_ipc_to` | `mini_send()` |
| notify | `s_ipc_to` | `sys_notify()` |
| asyncsend | `s_ipc_to` | `mini_asyncsend()` |
| receive | 无过滤 | 任何进程都可以接收 |

---

## Ch2: C 源码分析

### ipc.h (~100 行)

| 行号 | 函数/宏 | 说明 |
|------|---------|------|
| 1-30 | `check_ipc_to()` | 检查 s_ipc_to 位图 |
| 31-60 | `check_k_call_mask()` | 检查 s_k_call_mask 位图 |
| 61-100 | `get_sys_bit/set_sys_bit` | 位图操作宏 |

### system.c:540-660 (~120 行)

| 行号 | 函数/逻辑 | 说明 |
|------|----------|------|
| 540-560 | 系统调用过滤入口 | `kernel_call()` 中的 mask 检查 |
| 561-600 | IPC 过滤辅助 | `isokendpt()` + 权限检查 |
| 601-660 | 过滤失败处理 | 返回 EPERM + 日志 |

---

## Ch3: Rust 设计决策

| # | 决策 | 选项 | 结论 | 理由 |
|---|------|------|------|------|
| D1 | 位图操作 | 宏 vs 内联函数 | **内联函数** | Rust 类型安全 |
| D2 | 过滤逻辑 | 内联 vs 独立函数 | **独立函数** | 可测试性 |
| D3 | s_k_call_mask 类型 | `[u32; 2]` vs `u64` | **`u64`** | 58 个 syscall，1 个 u64 足够 |
| D4 | 过滤失败 | 返回 EPERM vs panic | **返回 EPERM** | 与 C 一致 |

---

## Ch4: 实现要点

### 4.1 IPC 过滤函数

```rust
/// Check if a process may send IPC to a target.
/// C: `may_send_to()` — priv.h:86 (无条件检查 s_ipc_to)
///
/// 注意: C 源码中不存在 CHECK_IPC 标志，may_send_to() 始终执行检查。
/// 只有拥有独立 priv 结构的系统进程才有有意义的 s_ipc_to 位图，
/// 用户进程共享 USER_PRIV，其 s_ipc_to 由 RS 配置。
///
/// **签名说明**: 参数为 `target_sys_id: u16` 而非 `target_priv: &KPriv`。
/// 原因: 调用方通常已有 target 的 `s_id`（从 endpoint 查找得到），
/// 无需再传入整个 `KPriv` 引用。C 源码中 `may_send_to()` 也只使用
/// `target_priv->s_id`，因此直接传 `s_id` 更高效且语义等价。
pub fn ipc_filter_check(caller_priv: &KPriv, target_sys_id: u16) -> bool {
    caller_priv.may_send_to(target_sys_id)
}

/// Check if a process may invoke a kernel call.
/// C: `check_k_call_mask()` — ipc.h
pub fn kcall_filter_check(caller_priv: &KPriv, call_nr: u32) -> bool {
    if call_nr as usize >= 64 {
        return false;
    }
    let mask = caller_priv.s_k_call_mask[0] as u64
        | ((caller_priv.s_k_call_mask[1] as u64) << 32);
    (mask & (1u64 << call_nr)) != 0
}
```

### 4.2 与 syscall.rs 集成

在 `syscall.rs` 的 `kernel_call_dispatch()` 入口处添加过滤检查：

```rust
pub fn kernel_call_dispatch(
    caller: &mut KProcess,
    msg: &Message,
    priv_table: &PrivTable,
) -> KcallResult {
    let call_nr = msg.m_type as u16;
    let syscall = match Syscall::try_from(call_nr) {
        Ok(s) => s,
        Err(()) => return KcallResult::BadCall,
    };

    // C: `else if (!GET_BIT(priv(caller)->s_k_call_mask, call_nr))` — system.c:107
    let call_denied = match caller.priv_id {
        Some(priv_id) => {
            match priv_table.get(priv_id) {
                Some(caller_priv) => !kcall_filter_check(caller_priv, call_nr as u32),
                None => true,
            }
        }
        None => true,
    };
    if call_denied {
        return KcallResult::CallDenied;
    }

    // ... existing dispatch logic
}
```

**已实现**：见 `os/kernel/src/syscall.rs`。`KcallResult::CallDenied` 对应 C 的 `ECALLDENIED (210)`。

---

## 测试

- 单元：ipc_filter_check 允许/拒绝
- 单元：kcall_filter_check 位图操作
- 单元：s_ipc_to 位图允许/拒绝
- 单元：call_nr >= 64 返回 false

---

## 补充：异步 IPC 详细分析

> 来源：tmp-10-async-ipc.md

### 异步 IPC 概念

异步 IPC 是同步 IPC 的补充机制，允许发送方在不阻塞的情况下将消息投递给目标进程。核心数据结构是**异步消息表（asynmsg_t table）**——发送方在自身地址空间中维护一个消息数组，调用 `SENDA` 系统调用时将表地址和大小传递给内核。内核扫描表中每个条目，尝试投递所有有效消息。

异步 IPC 仅限**系统进程**使用（`s_flags & SYS_PROC`），用户进程不能使用。

### 异步消息表（asynmsg_t table）

每个条目包含：
- `flags`：消息状态标志（AMF_VALID/AMF_DONE/AMF_NOTIFY 等）
- `dst`：目标进程 endpoint
- `result`：内核处理结果
- `msg`：消息体

### 异步消息标志位

定义于 `minix3/minix/include/minix/ipc.h:2754-2762`：

| 标志 | 值 | 含义 |
|------|-----|------|
| `AMF_EMPTY` | 000 | 槽位未使用 |
| `AMF_VALID` | 001 | 槽位包含有效消息 |
| `AMF_DONE` | 002 | 内核已处理此消息 |
| `AMF_NOTIFY` | 004 | 处理完成后发送通知 |
| `AMF_NOREPLY` | 010 | 不匹配 SENDREC 的接收部分 |
| `AMF_NOTIFY_ERR` | 020 | 仅在投递失败时发送通知 |

### 待处理位图（s_asyn_pending）

当目标进程未就绪时，内核在目标的 `s_asyn_pending` 位图中设置发送方特权 ID 对应的位。目标进程下次调用 RECEIVE 时，`mini_receive()` 通过 `has_pending_asend()` 检查位图，发现待处理消息后调用 `try_async()` 或 `try_one()` 重新扫描发送方的异步消息表完成投递。

### ASYNCM 伪进程

当异步消息需要通知发送方处理结果时（`AMF_NOTIFY` 或 `AMF_NOTIFY_ERR`），内核通过 `mini_notify(ASYNCM, ...)` 发送通知。ASYNCM（endpoint=-5）是一个专用的伪进程，仅用于异步消息完成通知。

### 异步 IPC 行为规则

1. **仅系统进程可用**：`mini_senda()` 首先检查 `s_flags & SYS_PROC`
2. **发送方不阻塞**：SENDA 系统调用始终立即返回 OK
3. **表大小限制**：异步消息表大小不能超过 `16 * (NR_TASKS + NR_PROCS)` = 4160 个条目
4. **AMF_DONE 语义**：内核处理完一个条目后设置 `AMF_DONE` 并写入 `result`
5. **不允许发送给内核任务**：异步消息的目标不能是内核任务
6. **VM 干预时跳过**：SMP 下若发送方地址空间正在被 VM 修改（`RTS_VMINHIBIT`），跳过该发送方的异步消息
7. **权限检查**：`may_asynsend_to()` 检查发送权限，比同步 IPC 的 `may_send_to()` 更宽松——允许发送给自己

### 异步 IPC 函数列表

| 功能 | 函数 | 位置 |
|------|------|------|
| SENDA 系统调用入口 | `mini_senda()` | proc.c:1331 |
| 扫描表并尝试投递 | `try_deliver_senda()` | proc.c:1200 |
| 接收时尝试异步投递（ANY） | `try_async()` | proc.c:1348 |
| 接收时尝试异步投递（指定源） | `try_one()` | proc.c:1390 |
| 取消异步消息 | `cancel_async()` | proc.c:1510 |
| 检查待处理通知 | `has_pending_notify()` | proc.c:843 |
| 检查待处理异步消息 | `has_pending_asend()` | proc.c:852 |
| 通用待处理检查 | `has_pending()` | proc.c:773 |

---

## 参见

- [21-privilege.md](21-privilege.md) — s_ipc_to, s_k_call_mask 定义
- [11-ipc-primitives.md](11-ipc-primitives.md) — send/receive/notify 使用过滤
