# VFS Server Fork Syscall 实现计划

> 目标：实现 VFS Server 中 fork syscall 路径的完整功能。
> 质量目标：Redox OS / Tokio 级别。

---

## 1. Fork 路径概述

VFS (Virtual File System) 在 fork 中的职责是复制父进程的文件描述符表。这包括：
1. 接收 PM 的 fork 请求
2. 复制父进程的 fproc 结构
3. 增加文件/vnode 的引用计数
4. 返回成功给 PM

### 1.1 Fork 流程

```
PM Server
    │
    ▼ (VfsRequest::Fork)
VFS Server
    │
    ├─► 查找父进程 fproc
    │
    ├─► 分配子进程 fproc slot
    │
    ├─► 复制文件描述符表
    │       │
    │       ├─► 复制 filp 数组
    │       ├─► 增加每个 filp 的引用计数
    │       └─► 增加对应 vnode 的引用计数
    │
    ├─► 复制当前工作目录
    │       │
    │       └─► 增加 vnode 引用计数
    │
    ├─► 复制根目录
    │       │
    │       └─► 增加 vnode 引用计数
    │
    └─► 返回成功
            │
            ▼ (VfsResponse::ForkOk)
        PM Server
```

### 1.2 关键数据结构

```
父进程                    子进程
┌─────────────┐          ┌─────────────┐
│  fproc      │          │  fproc      │
│  fp_filp[]  │──copy──►│  fp_filp[]  │
│  fp_wd      │──share──►│  fp_wd      │
│  fp_rd      │──share──►│  fp_rd      │
└─────────────┘          └─────────────┘
       │                        │
       ▼                        ▼
┌─────────────┐          ┌─────────────┐
│  filp       │◄─────────┤  filp       │
│  refcount=2 │          │  (同一指针)  │
└─────────────┘          └─────────────┘
       │
       ▼
┌─────────────┐
│  vnode      │
│  refcount++ │
└─────────────┘
```

---

## 2. 实现任务

### Phase 1: IPC 消息类型定义

**文件**: `minix-types/src/ipc/vfs.rs` (新增)

```rust
/// VFS 服务接收的消息类型
pub enum VfsRequest {
    /// Fork 请求 (来自 PM)
    Fork {
        /// 父进程 endpoint
        parent_endpoint: Endpoint,
        /// 子进程 endpoint
        child_endpoint: Endpoint,
    },
}

/// VFS 服务发送的响应类型
pub enum VfsResponse {
    /// Fork 成功
    ForkOk,
    /// 错误
    Error(VfsError),
}

/// VFS 错误类型
pub enum VfsError {
    /// 进程表已满
    ProcTableFull,
    /// 无效 endpoint
    InvalidEndpoint,
    /// 内部错误
    InternalError,
}
```

### Phase 2: 消息分发器

**文件**: `vfs/src/ipc/dispatcher.rs` (新增)

```rust
pub struct MessageDispatcher;

impl MessageDispatcher {
    pub fn dispatch(
        table: &mut FprocTable,
        request: VfsRequest,
    ) -> VfsResponse {
        match request {
            VfsRequest::Fork { parent_endpoint, child_endpoint } => {
                handle_fork(table, parent_endpoint, child_endpoint)
            }
        }
    }
}
```

### Phase 3: Fork 处理器

**文件**: `vfs/src/fork.rs` (新增)

```rust
/// 处理 fork 请求
pub fn handle_fork(
    table: &mut FprocTable,
    parent_endpoint: Endpoint,
    child_endpoint: Endpoint,
) -> VfsResponse {
    // 1. 查找父进程
    let parent = table.find_by_endpoint(parent_endpoint)
        .ok_or(VfsError::InvalidEndpoint)?;

    // 2. 分配子进程 slot
    let child_slot = table.alloc_slot()
        .ok_or(VfsError::ProcTableFull)?;

    // 3. 获取子进程 fproc
    let child = table.get_mut(child_slot);

    // 4. 复制文件描述符表
    copy_file_descriptors(parent, child)?;

    // 5. 复制工作目录
    copy_working_directory(parent, child)?;

    // 6. 设置子进程 endpoint
    child.fp_endpoint = child_endpoint;

    VfsResponse::ForkOk
}

/// 复制文件描述符表
fn copy_file_descriptors(
    parent: &Fproc,
    child: &mut Fproc,
) -> Result<(), VfsError> {
    for (fd, filp_opt) in parent.fp_filp.iter().enumerate() {
        if let Some(filp) = filp_opt {
            // 增加引用计数
            filp.filp_count += 1;

            // 增加 vnode 引用计数
            if let Some(vnode) = &filp.filp_vno {
                vnode.v_ref_count += 1;
            }

            // 复制到子进程
            child.fp_filp[fd] = Some(filp.clone());
        }
    }
    Ok(())
}

/// 复制工作目录
fn copy_working_directory(
    parent: &Fproc,
    child: &mut Fproc,
) -> Result<(), VfsError> {
    // 复制当前工作目录
    if let Some(wd) = &parent.fp_wd {
        wd.v_ref_count += 1;
        child.fp_wd = Some(wd.clone());
    }

    // 复制根目录
    if let Some(rd) = &parent.fp_rd {
        rd.v_ref_count += 1;
        child.fp_rd = Some(rd.clone());
    }

    Ok(())
}
```

### Phase 4: Fproc 结构

**文件**: `vfs/src/fproc.rs` (已存在，需完善)

```rust
/// VFS 进程结构
pub struct Fproc {
    /// 进程 endpoint
    pub fp_endpoint: Endpoint,
    /// 文件描述符表
    pub fp_filp: [Option<Arc<Filp>>; OPEN_MAX],
    /// 当前工作目录
    pub fp_wd: Option<Arc<Vnode>>,
    /// 根目录
    pub fp_rd: Option<Arc<Vnode>>,
    /// 进程标志
    pub fp_flags: FpFlags,
}

/// 文件描述符表
pub struct FprocTable {
    /// 进程数组
    procs: [Fproc; NR_PROCS],
    /// 空闲 slot 列表
    free_slots: Vec<usize>,
}

impl FprocTable {
    /// 查找进程 by endpoint
    pub fn find_by_endpoint(&self, endpoint: Endpoint) -> Option<&Fproc> {
        self.procs.iter().find(|p| p.fp_endpoint == endpoint)
    }

    /// 分配 slot
    pub fn alloc_slot(&mut self) -> Option<usize> {
        self.free_slots.pop()
    }

    /// 获取可变引用
    pub fn get_mut(&mut self, slot: usize) -> &mut Fproc {
        &mut self.procs[slot]
    }
}
```

---

## 3. 依赖关系

```
vfs/src/fork.rs
├── vfs/src/fproc.rs (进程表)
├── vfs/src/ipc/dispatcher.rs (消息分发)
├── minix-types (IPC 消息类型)
└── PM Server (fork 协调)
```

---

## 4. 文件变更清单

| 文件 | 操作 | 说明 |
|------|------|------|
| `minix-types/src/ipc/vfs.rs` | 新增 | VFS IPC 消息类型 |
| `minix-types/src/ipc/mod.rs` | 修改 | 导出 vfs 模块 |
| `vfs/src/ipc/mod.rs` | 新增 | IPC 模块 |
| `vfs/src/ipc/dispatcher.rs` | 新增 | 消息分发器 |
| `vfs/src/fork.rs` | 新增 | Fork 处理逻辑 |
| `vfs/src/fproc.rs` | 完善 | Fproc 结构、表管理 |
| `vfs/src/lib.rs` | 修改 | 导出新模块 |

---

## 5. 测试计划

### 5.1 单元测试

- [ ] `FprocTable::find_by_endpoint` 查找
- [ ] `FprocTable::alloc_slot` 分配
- [ ] `copy_file_descriptors` 文件描述符复制
- [ ] `copy_working_directory` 目录复制
- [ ] 引用计数增加验证

### 5.2 集成测试

- [ ] 完整 fork 流程：PM 请求 → VFS 复制 → 返回成功
- [ ] 与 PM 的 IPC 交互
- [ ] 文件描述符共享验证

---

## 6. 验收标准

- [ ] 所有测试通过
- [ ] 代码通过 `cargo clippy` 无警告
- [ ] 代码通过 `cargo fmt`
- [ ] 所有公开 API 有文档注释
- [ ] 所有 `unsafe` 块有 Safety 注释
