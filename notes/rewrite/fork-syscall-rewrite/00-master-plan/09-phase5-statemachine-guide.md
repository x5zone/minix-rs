# 阶段5：PM 状态机与跨服务协调指南

> **状态**: ❌ 待实现  
> **对应源码**: `minix/servers/pm/forkexit.c`, `minix/servers/pm/main.c`

---

## 1. 目标与范围

实现 PM 层 `do_fork()` 的完整状态机，协调 PM → VM → Kernel → VFS 的跨服务交互流程。

**IPC 通信**: PM → VM → Kernel → VFS 的跨服务 IPC 通信（真实实现）  
**状态机逻辑**: do_fork 状态转换、SUSPEND 机制、错误回滚（真实实现）

> **重要**: IPC 通信是微内核的核心机制，应该真实实现而非 mock。这样才能测试真实的跨服务协调流程。

### 核心认知：Rewrite vs Redesign

> **这是整个设计决策的分水岭**

#### 两个阶段的本质区别

| 维度 | Rewrite 阶段 | Redesign 阶段 |
|------|-------------|--------------|
| **目标** | 忠实翻译 Minix3 | 进化架构 |
| **约束** | 行为 100% 对齐 | 可以改变模型 |
| **数据结构** | 保持 Minix 语义 | 可以引入 Rust 语义 |
| **状态模型** | 共享可变状态 | 可以改变所有权模型 |
| **核心原则** | 最小化心智负担 | 去中心化、无锁、极致性能 |

#### Rewrite 阶段推荐方案

```text
进程表：[Process; NR_PROCS]
计数器：Cell<usize>
槽位：轮询
current：PmContext
IN_USE：保留
```

**一句话总结**：

> **Rewrite = 结构翻译 + 类型安全，不改变模型**

#### Redesign 阶段可选方案

```text
进程表：Per-Core 本地队列 / Arena 分配器
槽位：空闲栈 / 无锁哈希表
并发：Atomic + UMWAIT
所有权：Generational Index
```

**一句话总结**：

> **Redesign = 改变资源模型 + ownership 语义**

### 关键设计决策：进程表方案对比

#### 方案 A：简单数组（✅ 推荐）

```rust
pub struct ProcTable {
    procs: [Process; NR_PROCS],
    procs_in_use: Cell<usize>,
    next_child: Cell<usize>,
}
```

**优点**：
- ✅ 零运行时分配
- ✅ 缓存友好（连续内存）
- ✅ 索引 O(1)
- ✅ no_std 完美兼容
- ✅ **与 Minix3 一致**

**内存占用**：
```
Process ≈ 88 bytes
256 进程 ≈ 22.5 KB
```

#### 方案 B：Vec（❌ 不推荐）

```rust
pub struct ProcTable {
    procs: Vec<Process>,
}
```

**结论**：不推荐。Minix 是固定槽位架构，Vec 显得格格不入。

#### 方案 C：Slab（🔜 Redesign 可选）

```rust
use slab::Slab;
pub struct ProcTable {
    procs: Slab<Process>,
}
```

**结论**：Redesign 阶段可选。

### 计数器方案对比

| 方案 | 类型 | 线程安全 | 性能 | 内部可变 | Rewrite | Redesign |
|------|------|---------|------|---------|---------|----------|
| **Cell<usize>** | `Cell<usize>` | ❌ | ⚡ 最快 | ✅ | ✅ 推荐 | ⚠️ 可用 |
| **Atomic** | `AtomicUsize` | ✅ | 🚀 中等 | ✅ | ❌ 不需要 | ✅ 推荐 |

**结论**：Rewrite 阶段使用 `Cell<usize>`，单线程安全且零运行时开销。

### Minix PM 的"单线程"语义

> **Minix 的"单线程"不是 Rust 意义的单线程**

它是：
- 单线程执行
- **但可能被中断**
- 状态可以被其他 subsystem 观察

真实语义是：
```text
"逻辑单线程 + 物理共享内存"
```

### Fork 调用跨服务器协调流程

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        Fork 调用跨服务器协调                                  │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ① PM: do_fork()                                                           │
│     ├── 检查进程表是否已满                                                   │
│     ├── 查找空闲 mproc 槽位                                                 │
│     ├── 生成新 PID                                                          │
│     └── 复制父进程 mproc                                                    │
│                                                                             │
│  ② PM → VM: vm_fork() IPC 调用                                             │
│     ├── VM 分配 vmproc 槽位                                                 │
│     ├── VM 复制父进程地址空间                                               │
│     ├── VM 生成新 endpoint                                                  │
│     └── VM 返回 endpoint 给 PM                                              │
│                                                                             │
│  ③ PM → VFS: tell_vfs(VFS_PM_FORK) IPC 调用                               │
│     ├── VFS 分配 fproc 槽位                                                 │
│     ├── VFS 复制文件描述符表                                                │
│     ├── VFS 复制工作目录                                                    │
│     └── VFS 回复 PM                                                         │
│                                                                             │
│  ④ Kernel: sys_fork() (由 VM 触发)                                         │
│     ├── 分配 proc 槽位                                                      │
│     └── 初始化调度状态                                                      │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 2. do_fork 完整状态机

### 2.1 `do_fork()` 核心实现

```rust
impl<'a> PmContext<'a> {
    pub fn do_fork(&mut self) -> Result<i32, ForkError> {
        // 前置检查与槽位分配
        let prepare = self.do_fork_prepare()?;

        // 调用VM fork，失败时直接释放槽位回滚
        let vm_result = self.call_vm_fork(
            self.current_proc().identity.endpoint,
            prepare.child_index,
        ).map_err(|e| {
            self.table.release_slot(prepare.child_index);
            e
        })?;

        // VM fork成功后不可失败，必须完成后续流程
        self.fork_child_from_parent(
            prepare.child_index,
            prepare.child_pid,
            vm_result.child_endpoint,
        );

        // 异步通知VFS
        self.tell_vfs(VfsPmForkRequest {
            child_endpoint: vm_result.child_endpoint,
            parent_endpoint: self.current_proc().identity.endpoint,
            child_pid: prepare.child_pid,
            real_uid: -1,
            real_gid: -1,
        });

        // 标记VFS_CALL标志，等待回复
        self.current_proc_mut().resources.flags |= ProcessFlags::VFS_CALL;

        // 有追踪器时发送SIGSTOP
        if self.current_proc().state.trace.is_traced() {
            sig_proc(&self.table[prepare.child_index], Signal::SIGSTOP, true, false);
        }

        // 返回SUSPEND，父进程挂起等待VFS回复
        Ok(SYS_SUSPEND)
    }
}
```

### 2.2 `do_fork_reply()` VFS回复处理

```rust
impl<'a> PmContext<'a> {
    pub fn do_fork_reply(&mut self, child_endpoint: Endpoint) -> Result<(), ForkError> {
        let child_slot = endpoint_slot(child_endpoint);
        let child = &mut self.table[child_slot];

        // 清除VFS_CALL标志
        child.resources.flags &= !ProcessFlags::VFS_CALL;

        // 尝试调度子进程
        match sched_start_user(child) {
            Ok(_) => {
                // 调度成功：回复子进程OK，回复父进程子PID
                reply(child_endpoint, SYS_OK);
                reply(child.state.guardianship.parent, child.identity.id.pid as i32);
            }
            Err(_) => {
                // 调度失败：销毁子进程，回复父进程错误
                exit_proc(child, ExitCode::from(-1));
                reply(child.state.guardianship.parent, -1);
            }
        }

        Ok(())
    }
}
```

---

## 3. 跨服务消息序列

### 3.1 PM → VM 消息（vm_fork）

```rust
// VM_FORK 消息定义
struct VmForkRequest {
    m_type: VM_FORK,
    parent_endpoint: Endpoint,
    child_slot: ProcIndex,
}

// VM_FORK 回复
struct VmForkReply {
    m_type: VM_FORK_REPLY,
    child_endpoint: Endpoint,
    error: i32,
}
```

### 3.2 PM → VFS 消息（tell_vfs）

```rust
// VFS_PM_FORK 消息定义
struct VfsPmForkRequest {
    m_type: VFS_PM_FORK,
    child_endpoint: Endpoint,
    parent_endpoint: Endpoint,
    child_pid: Pid,
    real_uid: i32,
    real_gid: i32,
}

// VFS_PM_FORK_REPLY 回复
struct VfsPmForkReply {
    m_type: VFS_PM_FORK_REPLY,
    child_endpoint: Endpoint,
    error: i32,
}
```

---

## 4. 错误处理与回滚

### 4.1 回滚路径

| 阶段 | 失败场景 | 回滚操作 |
|------|---------|---------|
| 槽位分配前 | 进程表满 | 返回 EAGAIN |
| VM fork 前 | PID 分配失败 | 释放槽位 |
| VM fork 中 | VM 返回错误 | 释放 mproc 槽位 |
| VFS 通知后 | VFS 回复错误 | 调用 exit_proc 销毁子进程 |

### 4.2 不可失败点

> ⚠️ **VM fork 成功后不可失败**
> - VM 已分配 vmproc 槽位
> - VM 已生成 endpoint
> - VM 已复制地址空间
> - 必须完成后续流程，不能回滚

---

## 5. SUSPEND 与唤醒机制

### 5.1 SUSPEND 机制

```rust
// os/servers/pm/src/suspend.rs

/// 设置进程为 SUSPEND 状态，等待指定服务的回复
pub fn suspend_for_reply(
    proc: &mut MProc,
    service: Endpoint,
    msg_type: u32,
) {
    proc.state.suspended = true;
    proc.state.suspend_service = service;
    proc.state.suspend_msg_type = msg_type;
}

/// 唤醒等待回复的进程
pub fn wakeup_for_reply(
    proc: &mut MProc,
    service: Endpoint,
    msg_type: u32,
) -> bool {
    if proc.state.suspended 
        && proc.state.suspend_service == service 
        && proc.state.suspend_msg_type == msg_type 
    {
        proc.state.suspended = false;
        return true;
    }
    false
}
```

### 5.2 SUSPEND 状态转换

```
┌─────────────┐     do_fork()     ┌────────────────┐
│   Running   │──────────────────▶│   SUSPENDED    │
└─────────────┘                   │  wait VFS_REPLY│
                                  └────────────────┘
                                          │
                                          │ VFS_PM_FORK_REPLY
                                          ▼
                                  ┌────────────────┐
                                  │   Running      │
                                  │  reply parent  │
                                  └────────────────┘
```

---

## 6. 异步通知处理

### 6.1 VFS 回复处理流程

```rust
// 主消息循环
loop {
    let msg = receive();
    match msg.m_type {
        VFS_PM_FORK_REPLY => {
            let child_ep = msg.endpoint;
            ctx.do_fork_reply(child_ep)?;
        }
        // ... 其他消息处理
    }
}
```

### 6.2 VFS_CALL 标志管理

```rust
bitflags! {
    pub struct ProcessFlags: u32 {
        const VFS_CALL = 0x1; // 正在等待 VFS 回复
        const EXITING = 0x2;  // 正在退出
        const TRACED = 0x4;   // 被追踪
    }
}
```

---

## 7. 测试用例

```rust
#[test]
fn test_do_fork_state_machine() {
    let mut ctx = setup_pm_context();
    
    // 执行 fork，返回 SUSPEND
    let result = ctx.do_fork().unwrap();
    assert_eq!(result, SYS_SUSPEND);
    
    // 父进程处于 SUSPEND 状态
    assert!(ctx.current_proc().state.suspended);
    
    // 模拟 VFS 回复
    let child_ep = get_child_endpoint(&ctx);
    ctx.do_fork_reply(child_ep).unwrap();
    
    // 父进程被唤醒
    assert!(!ctx.current_proc().state.suspended);
    
    // 子进程创建成功
    let child = ctx.table.find_by_endpoint(child_ep).unwrap();
    assert_eq!(child.state.guardianship.parent(), ctx.current_proc().slot);
}

#[test]
fn test_do_fork_vm_failure_rollback() {
    let mut ctx = setup_pm_context_with_vm_failure();
    
    // VM fork 失败
    let result = ctx.do_fork();
    
    // 应该返回错误
    assert!(result.is_err());
    
    // 槽位应该被释放
    assert_eq!(ctx.table.free_slots(), NR_PROCS);
}

#[test]
fn test_do_fork_vfs_call_flag() {
    let mut ctx = setup_pm_context();
    
    // fork 后应该设置 VFS_CALL 标志
    let _ = ctx.do_fork().unwrap();
    assert!(ctx.current_proc().resources.flags.contains(ProcessFlags::VFS_CALL));
    
    // VFS 回复后应该清除 VFS_CALL 标志
    let child_ep = get_child_endpoint(&ctx);
    ctx.do_fork_reply(child_ep).unwrap();
    assert!(!ctx.current_proc().resources.flags.contains(ProcessFlags::VFS_CALL));
}
```

---

## 8. 任务清单

| # | 任务 | 依赖 | 目标文件 |
|---|------|------|---------|
| 5.1 | 实现 `vm_fork()` IPC 调用 | 阶段 2 | `os/servers/pm/src/mproc/fork.rs` |
| 5.2 | 实现 vm_fork 失败时的回滚 | 5.1 | `os/servers/pm/src/mproc/fork.rs` |
| 5.3 | 实现 `tell_vfs()` 异步通知 | 阶段 4 | `os/servers/pm/src/mproc/fork.rs` |
| 5.4 | 实现 `VFS_CALL` 标志管理 | 5.3 | `os/servers/pm/src/mproc/fork.rs` |
| 5.5 | 实现 `do_fork()` 完整状态机 | 5.1-5.4 | `os/servers/pm/src/mproc/fork.rs` |
| 5.6 | 实现 SUSPEND 返回机制 | 5.5 | `os/servers/pm/src/mproc/fork.rs` |
| 5.7 | 实现 `do_fork_reply()` VFS 回复处理 | 5.5 | `os/servers/pm/src/mproc/fork.rs` |
| 5.8 | 实现调度失败回滚 (exit_proc) | 5.7 | `os/servers/pm/src/mproc/fork.rs` |
| 5.9 | 编写 PM 状态机单元测试 | 5.5 | `os/servers/pm/src/mproc/fork.rs` |
