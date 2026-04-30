# Kernel Fork Syscall 实现计划

> 目标：实现 Kernel 中 fork syscall 路径的完整功能。
> 质量目标：Redox OS / Tokio 级别。

---

## 1. Fork 路径概述

Kernel 是 fork 系统调用的入口和最终执行者。它负责：
1. 接收用户进程的 fork 系统调用
2. 转发请求给 PM
3. 创建子进程的内核进程结构 (proc)
4. 设置子进程的调度状态

### 1.1 Fork 流程

```
用户进程调用 fork()
        │
        ▼
    syscall 指令
        │
        ▼
    Kernel (syscall handler)
        │
        ├─► 保存父进程寄存器状态
        │
        ├─► 发送 PM_FORK 请求给 PM
        │       │
        │       ▼
        │   PM Server (协调 fork)
        │       │
        │       ├─► 请求 VM 复制地址空间
        │       ├─► 请求 VFS 复制文件描述符
        │       └─► 请求 Kernel 创建 proc 结构
        │               │
        │               ▼
        │           Kernel (sys_fork)
        │               │
        │               ├─► 分配子进程 proc slot
        │               ├─► 复制父进程 proc 结构
        │               ├─► 设置子进程 endpoint
        │               └─► 设置子进程调度状态
        │               │
        │               ▼
        │           PM Server (继续)
        │       │
        │       ▼
        │   PM Server 返回结果
        │
        ├─► 设置返回值
        │       │
        │       ├─► 父进程: 返回子进程 PID
        │       └─► 子进程: 返回 0
        │
        └─► 返回用户态
```

### 1.2 关键数据结构

```
父进程                    子进程
┌─────────────┐          ┌─────────────┐
│  proc       │          │  proc       │
│  p_reg      │──copy──►│  p_reg      │
│  p_endpoint │          │  p_endpoint │
│  p_rts_flags│          │  p_rts_flags│
│  p_priority │          │  p_priority │
└─────────────┘          └─────────────┘
```

---

## 2. 实现任务

### Phase 1: IPC 消息类型定义

**文件**: `minix-types/src/ipc/kernel.rs` (新增)

```rust
/// Kernel 系统调用请求类型 (来自 PM)
pub enum KernelRequest {
    /// Fork 请求 (来自 PM)
    Fork {
        /// 父进程 endpoint
        parent_endpoint: Endpoint,
        /// 子进程 endpoint
        child_endpoint: Endpoint,
        /// 子进程 slot
        child_slot: UserSlot,
    },
}

/// Kernel 系统调用响应类型
pub enum KernelResponse {
    /// Fork 成功
    ForkOk,
    /// 错误
    Error(KernelError),
}

/// Kernel 错误类型
pub enum KernelError {
    /// 进程表已满
    ProcTableFull,
    /// 无效 endpoint
    InvalidEndpoint,
    /// 内部错误
    InternalError,
}
```

### Phase 2: 系统调用处理

**文件**: `kernel/src/system.rs` (已存在，需完善)

```rust
/// 系统调用分发
pub fn dispatch_syscall(
    proc_table: &mut ProcTable,
    call_nr: i32,
    message: &Message,
) -> Result<Message, KernelError> {
    match call_nr {
        SYS_FORK => handle_sys_fork(proc_table, message),
        // ... 其他系统调用
    }
}

/// 处理 fork 系统调用 (用户进程调用)
fn handle_sys_fork(
    proc_table: &mut ProcTable,
    message: &Message,
) -> Result<Message, KernelError> {
    let caller = message.m_source;

    // 转发给 PM
    let pm_request = PmRequest::Fork { caller };
    let pm_response = send_to_pm(pm_request)?;

    // 构造返回消息
    Ok(build_fork_response(pm_response))
}
```

### Phase 3: 进程结构创建

**文件**: `kernel/src/proc.rs` (已存在，需完善)

```rust
impl ProcTable {
    /// 创建子进程的 proc 结构 (由 PM 调用)
    pub fn sys_fork(
        &mut self,
        parent_endpoint: Endpoint,
        child_endpoint: Endpoint,
        child_slot: UserSlot,
    ) -> Result<(), KernelError> {
        // 1. 查找父进程
        let parent = self.find_by_endpoint(parent_endpoint)
            .ok_or(KernelError::InvalidEndpoint)?;

        // 2. 获取子进程 slot
        let child = self.get_slot_mut(child_slot)
            .ok_or(KernelError::ProcTableFull)?;

        // 3. 复制父进程的 proc 结构
        child.copy_from(parent);

        // 4. 设置子进程 endpoint
        child.p_endpoint = child_endpoint;

        // 5. 设置子进程调度状态
        child.p_rts_flags = rts::NO_PRIV;  // 等待 PM 设置权限

        // 6. 复制寄存器状态
        child.p_reg = parent.p_reg.clone();

        // 7. 设置返回值 (子进程返回 0)
        child.p_reg.set_retval(0);

        Ok(())
    }
}
```

### Phase 4: 进程表管理

**文件**: `kernel/src/proc.rs` (已存在，需完善)

```rust
/// 进程表
pub struct ProcTable {
    /// 进程数组
    procs: [Proc; NR_PROCS + NR_TASKS],
    /// 空闲 slot 链表
    free_slots: Vec<usize>,
}

impl ProcTable {
    /// 查找进程 by endpoint
    pub fn find_by_endpoint(&self, endpoint: Endpoint) -> Option<&Proc> {
        self.procs.iter().find(|p| p.p_endpoint == endpoint)
    }

    /// 获取可变 slot
    pub fn get_slot_mut(&mut self, slot: UserSlot) -> Option<&mut Proc> {
        let index = slot.get() + NR_TASKS;
        self.procs.get_mut(index)
    }
}
```

---

## 3. 硬件抽象

Kernel 需要通过 trait 抽象硬件操作：

### 3.1 Context Trait

**文件**: `minix-arch/src/context.rs` (新增)

```rust
/// CPU 上下文 trait
///
/// 由各架构实现 (mock, x86_64, arm64, riscv64)
pub trait Context {
    /// 保存当前上下文
    fn save(&mut self);

    /// 恢复上下文
    fn restore(&self);

    /// 设置返回值
    fn set_retval(&mut self, value: i64);

    /// 克隆上下文 (用于 fork)
    fn clone(&self) -> Self;
}
```

### 3.2 Scheduling Trait

**文件**: `minix-arch/src/sched.rs` (新增)

```rust
/// 调度器 trait
///
/// 由各架构实现
pub trait Scheduler {
    /// 切换到目标进程
    fn switch_to(&mut self, next: &mut Proc);

    /// 获取当前进程
    fn current(&self) -> &Proc;
}
```

---

## 4. 依赖关系

```
kernel/src/proc.rs
├── kernel/src/system.rs (系统调用处理)
├── kernel/src/ipc.rs (IPC 通信)
├── minix-types (IPC 消息类型)
├── minix-arch (Context, Scheduler trait)
└── PM Server (fork 协调)
```

---

## 5. 文件变更清单

| 文件 | 操作 | 说明 |
|------|------|------|
| `minix-types/src/ipc/kernel.rs` | 新增 | Kernel IPC 消息类型 |
| `minix-types/src/ipc/mod.rs` | 修改 | 导出 kernel 模块 |
| `kernel/src/proc.rs` | 完善 | 进程结构、fork 逻辑 |
| `kernel/src/system.rs` | 完善 | 系统调用分发 |
| `kernel/src/ipc.rs` | 完善 | IPC 通信 |
| `minix-arch/src/context.rs` | 新增 | Context trait |
| `minix-arch/src/sched.rs` | 新增 | Scheduler trait |

---

## 6. 测试计划

### 6.1 单元测试

- [ ] `Proc::copy_from` 字段复制
- [ ] `ProcTable::find_by_endpoint` 查找
- [ ] `ProcTable::get_slot_mut` slot 获取
- [ ] `Context::clone` 上下文克隆

### 6.2 集成测试

- [ ] 完整 fork 流程：用户调用 → PM 协调 → Kernel 创建
- [ ] 与 PM 的 IPC 交互
- [ ] 寄存器状态保存/恢复

---

## 7. 验收标准

- [ ] 所有测试通过
- [ ] 代码通过 `cargo clippy` 无警告
- [ ] 代码通过 `cargo fmt`
- [ ] 所有公开 API 有文档注释
- [ ] 所有 `unsafe` 块有 Safety 注释
- [ ] 硬件操作通过 trait 抽象
