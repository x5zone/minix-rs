# Fork 系统调用重写设计

> 本文档总结了 Minix3 `fork` 系统调用的设计问题，并提出 Rust 重写方案。

## 1. 问题背景

### 1.1 现象：两个 fork 实现

Minix3 的 PM (Process Manager) 中存在两个 fork 实现：

| 函数 | 调用者 | VFS 交互 | 返回行为 |
|------|--------|----------|----------|
| `do_fork()` | 普通用户进程 | 等待 VFS 确认 | 返回 `SUSPEND`，稍后回复 |
| `do_srv_fork()` | RS (Restart Server) | 异步通知，不等待 | 立即返回 `pid` |

代码位置：[forkexit.c](../../minix3/minix/servers/pm/forkexit.c)

### 1.2 `do_srv_fork()` 的状态不一致风险

```c
// do_srv_fork() 简化流程
tell_vfs(rmc, &m);           // 异步通知 VFS（不等待）
reply(rmc-mproc, OK);        // 立即回复 RS
return rmc->mp_pid;          // 返回 pid
```

**问题**：RS 收到 `OK` 后，VFS 可能尚未处理 fork 消息。如果 RS 立即调用 `mapdriver()`，VFS 会拒绝（"我不认识这个进程"）。

### 1.3 `setuid(0)` Hack

Minix3 使用一个**语义污染**的同步机制：

```c
// RS 代码 (manager.c:656)
setuid(0);  // 不是设置 UID，而是"同步栅栏"！
```

**原理**：
1. `setuid(0)` 是阻塞系统调用
2. PM 处理 `setuid` 时，会等待 VFS 相关操作完成
3. 因此 `setuid(0)` 返回时，VFS 一定已经处理了之前的 fork 消息

**问题**：
- 语义污染：`setuid` 被用作同步原语
- 脆弱性：依赖实现细节
- 临时方案：注释明确说"Once VFS has been made non-blocking... this hack can go"

---

## 2. 根本原因分析

### 2.1 核心矛盾

> **Unix fork 语义 vs 微内核分布式架构**

| 传统 Unix (Monolithic) | Minix3 (Microkernel) |
|------------------------|----------------------|
| fork 是局部操作 | fork 是跨服务操作 |
| 单一地址空间 | PM + VFS + VM + Kernel |
| 原子性天然保证 | 需要显式协调 |

### 2.2 fork 是分布式事务

在 Minix3 中，fork 涉及多个服务：

```
┌─────────────────────────────────────────────────┐
│                    fork 操作                     │
├─────────────────────────────────────────────────┤
│  PM:  创建进程表条目，分配 PID                    │
│  VM:  复制地址空间                               │
│  VFS: 复制文件描述符表                           │
│  Kernel: 创建 task 结构，设置调度                │
└─────────────────────────────────────────────────┘
```

**问题**：这四个步骤之间：
- 没有事务 ID
- 没有状态机
- 没有完成判定

### 2.3 死锁场景

```
时间线 →
┌──────────────────────────────────────────────────────────────┐
│ VFS: 阻塞在等待 MFS 回复（磁盘 I/O）                          │
│   ↓                                                          │
│ 驱动崩溃                                                      │
│   ↓                                                          │
│ RS: 检测到崩溃，调用 do_fork 重启驱动                         │
│   ↓                                                          │
│ PM: 发消息给 VFS（通知新进程）                                │
│   ↓                                                          │
│ 死锁：PM 等 VFS，VFS 等死掉的驱动，RS 等 PM                   │
└──────────────────────────────────────────────────────────────┘
```

**解决方案**：`do_srv_fork()` 打破同步链，但引入状态不一致风险。

---

## 3. 设计原则

### 3.1 显式状态机

> **把"时间问题"变成"状态问题"**

不再依赖隐式时序（"VFS 会在 X 之前完成"），而是显式跟踪状态。

### 3.2 事务边界

每个 fork 操作有唯一事务 ID，所有参与服务通过事务 ID 协调。

### 3.3 最终一致性

允许短暂的状态不一致，但提供显式的"等待就绪"机制。

---

## 4. 重写方案

### 4.1 进程状态机

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProcState {
    // fork 阶段
    ForkInit,        // PM 创建 slot
    Forked,          // kernel 已创建 task
    VfsNotified,     // 已通知 VFS
    VmNotified,      // 已通知 VM
    ForkReady,       // fork 完成（可调度）

    // exec 阶段
    ExecInit,
    ElfLoading,
    VmMapping,
    ExecReady,

    // 生命周期
    Running,
    Exiting,
    Zombie,
}
```

### 4.2 事务结构

```rust
type ProcId = u32;
type TxId = u64;

struct ProcTx {
    tx_id: TxId,
    pid: ProcId,
    state: ProcState,

    // 等待哪些服务确认
    wait_vfs: bool,
    wait_vm: bool,
    wait_kernel: bool,
}
```

### 4.3 状态驱动流程

```rust
fn drive_fork(tx_id: TxId) {
    let tx = get_tx(tx_id);

    match tx.state {
        ProcState::ForkInit => {
            send_kernel_fork(tx);
            tx.state = ProcState::Forked;
        }

        ProcState::Forked => {
            send_vfs_fork(tx);
            tx.state = ProcState::VfsNotified;
        }

        ProcState::VfsNotified => {
            send_vm_fork(tx);
            tx.state = ProcState::VmNotified;
        }

        ProcState::VmNotified => {
            if !tx.wait_vfs && !tx.wait_vm && !tx.wait_kernel {
                tx.state = ProcState::ForkReady;
                notify_ready(tx);
            }
        }

        _ => {}
    }
}
```

### 4.4 服务回调

```rust
// Kernel 完成回调
fn on_kernel_fork_done(tx_id: TxId) {
    let tx = get_tx(tx_id);
    tx.wait_kernel = false;
    drive_fork(tx_id);
}

// VFS 完成回调
fn on_vfs_fork_done(tx_id: TxId) {
    let tx = get_tx(tx_id);
    tx.wait_vfs = false;
    drive_fork(tx_id);
}

// VM 完成回调
fn on_vm_fork_done(tx_id: TxId) {
    let tx = get_tx(tx_id);
    tx.wait_vm = false;
    drive_fork(tx_id);
}
```

### 4.5 RS 使用方式

```rust
// 旧方式（Minix3 C 代码）
let pid = srv_fork();
setuid(0);  // hack 同步
mapdriver(pid);

// 新方式（Rust 重写）
let tx = fork_async().await;
wait_until_ready(tx).await;  // 显式等待
mapdriver(tx.pid).await;     // 保证成功
```

---

## 5. 可验证性

### 5.1 状态不变量

```rust
// ForkReady 状态下，所有等待必须已清除
assert!(
    !(tx.state == ProcState::ForkReady 
      && (tx.wait_vfs || tx.wait_vm || tx.wait_kernel))
);
```

### 5.2 状态转移合法性

```rust
fn valid_transition(from: ProcState, to: ProcState) -> bool {
    matches!((from, to),
        (ForkInit, Forked) |
        (Forked, VfsNotified) |
        (VfsNotified, VmNotified) |
        (VmNotified, ForkReady) |
        (ForkReady, Running) |
        (Running, Exiting) |
        (Exiting, Zombie)
    )
}
```

### 5.3 无死锁保证

- 所有等待 = 显式 wait flags
- 所有推进 = 事件驱动
- 不存在循环依赖

---

## 6. 实现路线

### Phase 1：最小状态机

- PM 内部状态机
- fake VFS / VM 响应
- 验证状态转移逻辑

### Phase 2：IPC 集成

- 消息携带事务 ID
- 各服务回调机制
- 错误处理与回滚

### Phase 3：完整实现

- 真实 VFS / VM 集成
- QEMU 测试
- 性能基准

---

## 7. 与其他系统的对比

| 系统 | fork 语义 | 同步机制 |
|------|-----------|----------|
| Linux | 局部操作，原子 | 自旋锁 + RCU |
| Minix3 | 跨服务，隐式 | SUSPEND + hack |
| seL4 | 无 fork | Capability |
| **minix-rs** | 跨服务，显式 | 状态机 + 事务 |

---

## 8. 参考资料

- Minix3 源码：`minix/servers/pm/forkexit.c`
- Minix3 源码：`minix/servers/rs/manager.c`
- seL4 设计文档：Capability-based 进程创建
- 相关讨论：`tmp_do_srv_fork_design.md`

---

## 9. 总结

Minix3 的 fork 问题本质是：

> **分布式系统一致性问题在微内核中的体现**

解决方案不是"修补 C 代码"，而是：

> **用显式状态机替代隐式时序**

这符合 minix-rs 的整体设计哲学：将 Minix3 的"隐式分布式系统"变成"显式可验证协议"。
