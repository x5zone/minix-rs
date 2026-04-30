# PM Server Fork Syscall 实现计划

> 目标：实现 PM Server 中 fork syscall 路径的完整功能。
> 质量目标：Redox OS / Tokio 级别。

---

## 1. Fork 路径概述

PM (Process Manager) 是 fork 系统调用的主要协调者。当用户进程调用 fork() 时：

1. 用户进程通过 syscall 进入内核
2. 内核将请求转发给 PM
3. PM 协调 VM、VFS、Kernel 完成进程创建
4. PM 返回结果给用户

### 1.1 Fork 流程

```
用户进程调用 fork()
        │
        ▼
    Kernel (syscall)
        │
        ▼ (IPC: PM_FORK)
    PM Server
        │
        ├─► 分配子进程 slot (mproc)
        ├─► 分配子进程 PID
        ├─► 请求 VM 复制地址空间
        │       │
        │       ▼ (IPC: VM_FORK)
        │   VM Server (复制页表、设置 CoW)
        │       │
        │       ▼ (IPC: VM_FORK_REPLY)
        │   PM Server (继续)
        │
        ├─► 请求 VFS 复制文件描述符
        │       │
        │       ▼ (IPC: VFS_FORK)
        │   VFS Server (复制 fproc)
        │       │
        │       ▼ (IPC: VFS_FORK_REPLY)
        │   PM Server (继续)
        │
        ├─► 请求 Kernel 创建子进程
        │       │
        │       ▼ (IPC: SYS_FORK)
        │   Kernel (创建 proc 结构)
        │       │
        │       ▼
        │   PM Server (继续)
        │
        └─► 返回子进程 PID 给父进程
                │
                ▼
            Kernel
                │
                ▼
            返回给用户
```

### 1.2 关键数据结构

```
父进程                    子进程
┌─────────────┐          ┌─────────────┐
│  mproc      │          │  mproc      │
│  slot=0     │──copy──►│  slot=1     │
│  pid=100    │          │  pid=101    │
│  endpoint=A │          │  endpoint=B │
└─────────────┘          └─────────────┘
       │                        │
       │                        │
       ▼                        ▼
┌─────────────┐          ┌─────────────┐
│  vmproc     │          │  vmproc     │
│  (VM)       │◄──CoW───│  (VM)       │
└─────────────┘          └─────────────┘
       │                        │
       ▼                        ▼
┌─────────────┐          ┌─────────────┐
│  fproc      │          │  fproc      │
│  (VFS)      │──copy──►│  (VFS)      │
└─────────────┘          └─────────────┘
       │                        │
       ▼                        ▼
┌─────────────┐          ┌─────────────┐
│  proc       │          │  proc       │
│  (Kernel)   │──copy──►│  (Kernel)   │
└─────────────┘          └─────────────┘
```

---

## 2. 实现任务

### Phase 1: IPC 消息类型定义

**文件**: `minix-types/src/ipc/pm.rs` (新增)

```rust
/// PM 服务接收的消息类型
pub enum PmRequest {
    /// Fork 请求 (来自用户进程，通过内核转发)
    Fork {
        /// 调用者 endpoint
        caller: Endpoint,
    },
}

/// PM 服务发送的响应类型
pub enum PmResponse {
    /// Fork 成功 (返回给父进程)
    ForkParent {
        /// 子进程 PID
        child_pid: i32,
    },
    /// Fork 成功 (返回给子进程)
    ForkChild,
    /// 错误
    Error(PmError),
}

/// PM 错误类型
pub enum PmError {
    /// 进程表已满
    ProcTableFull,
    /// 内存不足
    OutOfMemory,
    /// 无效 endpoint
    InvalidEndpoint,
    /// 内部错误
    InternalError,
}
```

### Phase 2: 消息分发器

**文件**: `pm/src/ipc/dispatcher.rs` (新增)

```rust
pub struct MessageDispatcher;

impl MessageDispatcher {
    pub fn dispatch(
        table: &mut ProcTable,
        request: PmRequest,
    ) -> PmResponse {
        match request {
            PmRequest::Fork { caller } => {
                handle_fork(table, caller)
            }
        }
    }
}
```

### Phase 3: Fork 处理器

**文件**: `pm/src/fork.rs` (已存在，需完善)

```rust
pub fn handle_fork(
    table: &mut ProcTable,
    parent_endpoint: Endpoint,
) -> PmResponse {
    // 1. 查找父进程
    let parent_slot = table.find_by_endpoint(parent_endpoint)
        .ok_or(PmError::InvalidEndpoint)?;

    // 2. 分配子进程 slot
    let child_slot = table.alloc_slot()
        .ok_or(PmError::ProcTableFull)?;

    // 3. 分配子进程 PID
    let child_pid = table.pid_generator.next_pid();

    // 4. 生成子进程 endpoint
    let child_endpoint = Endpoint::from_generation_slot(
        1, // 初始 generation
        child_slot as i32,
    );

    // 5. 请求 VM 复制地址空间
    let vm_request = VmRequest::Fork {
        parent_endpoint,
        child_slot: UserSlot::new(child_slot),
        child_endpoint,
    };
    let vm_response = send_vm_request(vm_request)?;
    
    // 6. 请求 VFS 复制文件描述符
    let vfs_request = VfsRequest::Fork {
        parent_endpoint,
        child_endpoint,
    };
    let vfs_response = send_vfs_request(vfs_request)?;

    // 7. 请求 Kernel 创建子进程
    let kernel_request = KernelRequest::Fork {
        parent_endpoint,
        child_endpoint,
    };
    let kernel_response = send_kernel_request(kernel_request)?;

    // 8. 复制 mproc 字段
    table.copy_mproc(parent_slot, child_slot, child_pid, child_endpoint);

    // 9. 返回成功
    PmResponse::ForkParent { child_pid }
}
```

### Phase 4: mproc 复制

**文件**: `pm/src/mproc/fork.rs` (已存在，需完善)

```rust
impl ProcTable {
    /// 复制父进程的 mproc 到子进程
    pub fn copy_mproc(
        &mut self,
        parent_slot: usize,
        child_slot: usize,
        child_pid: i32,
        child_endpoint: Endpoint,
    ) {
        let parent = &self.procs[parent_slot];
        let child = &mut self.procs[child_slot];

        // 复制基本字段
        child.pid = child_pid;
        child.endpoint = child_endpoint;
        child.parent = parent_slot as i32;
        child.uid = parent.uid;
        child.gid = parent.gid;
        // ... 其他字段

        // 设置生命周期
        child.lifecycle = Lifecycle::Running;

        // 增加进程计数
        self.procs_in_use.set(self.procs_in_use.get() + 1);
    }
}
```

---

## 3. 依赖关系

```
pm/src/fork.rs
├── mproc/table.rs (进程表)
├── mproc/fork.rs (mproc 复制)
├── ipc/dispatcher.rs (消息分发)
├── minix-types (IPC 消息类型)
├── VM Server (地址空间复制)
├── VFS Server (文件描述符复制)
└── Kernel (进程结构创建)
```

---

## 4. 文件变更清单

| 文件 | 操作 | 说明 |
|------|------|------|
| `minix-types/src/ipc/pm.rs` | 新增 | PM IPC 消息类型 |
| `minix-types/src/ipc/mod.rs` | 修改 | 导出 pm 模块 |
| `pm/src/ipc/mod.rs` | 新增 | IPC 模块 |
| `pm/src/ipc/dispatcher.rs` | 新增 | 消息分发器 |
| `pm/src/fork.rs` | 完善 | Fork 处理逻辑 |
| `pm/src/mproc/fork.rs` | 完善 | mproc 复制逻辑 |
| `pm/src/mproc/table.rs` | 完善 | slot 分配、endpoint 查找 |
| `pm/src/lib.rs` | 修改 | 导出新模块 |

---

## 5. 测试计划

### 5.1 单元测试

- [ ] `PmRequest::Fork` 消息创建
- [ ] `handle_fork` slot 分配
- [ ] `handle_fork` PID 分配
- [ ] `copy_mproc` 字段复制
- [ ] `alloc_slot` 进程表满处理

### 5.2 集成测试

- [ ] 完整 fork 流程：创建父进程 → fork → 验证子进程状态
- [ ] 与 VM 的 IPC 交互
- [ ] 与 VFS 的 IPC 交互
- [ ] 与 Kernel 的 IPC 交互

---

## 6. 验收标准

- [ ] 所有测试通过
- [ ] 代码通过 `cargo clippy` 无警告
- [ ] 代码通过 `cargo fmt`
- [ ] 所有公开 API 有文档注释
- [ ] 所有 `unsafe` 块有 Safety 注释
