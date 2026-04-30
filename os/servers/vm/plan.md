# VM Server Fork Syscall 实现计划

> 目标：实现 VM Server 中 fork syscall 路径的完整功能。
> 质量目标：Redox OS / Tokio 级别。

---

## 1. Fork 路径概述

Fork 是进程创建的核心机制。当 PM 收到 fork 系统调用时，会请求 VM 复制父进程的地址空间到子进程。

### 1.1 Fork 流程

```
用户进程调用 fork()
        │
        ▼
    PM Server
        │
        ▼ (IPC: VM_FORK_REQUEST)
    VM Server
        │
        ├─► 获取父进程 VmProc
        ├─► 获取子进程空 slot
        ├─► 复制页表结构 (CoW)
        ├─► 复制区域列表
        ├─► 增加物理页引用计数
        └─► 返回子进程 endpoint
        │
        ▼ (IPC: VM_FORK_REPLY)
    PM Server
        │
        ▼
    返回子进程 PID 给用户
```

### 1.2 关键数据结构

```
父进程                    子进程
┌─────────────┐          ┌─────────────┐
│  VmProc     │          │  VmProc     │
│  slot=0     │          │  slot=1     │
│  endpoint=A │          │  endpoint=B │
└─────────────┘          └─────────────┘
       │                        │
       ▼                        ▼
┌─────────────┐          ┌─────────────┐
│  PageTable  │──copy──►│  PageTable  │
│  (CoW)      │          │  (CoW)      │
└─────────────┘          └─────────────┘
       │                        │
       └────────┬───────────────┘
                ▼
         ┌─────────────┐
         │ PhysPages   │
         │ refcount=2  │
         └─────────────┘
```

---

## 2. 实现任务

### Phase 1: IPC 消息类型定义

**文件**: `minix-types/src/ipc/vm.rs` (新增)

定义 fork 相关的 IPC 消息：

```rust
/// VM 服务接收的消息类型
pub enum VmRequest {
    /// Fork 请求 (来自 PM)
    Fork {
        /// 父进程的 endpoint
        parent_endpoint: Endpoint,
        /// 子进程的 slot
        child_slot: UserSlot,
        /// 子进程的 endpoint
        child_endpoint: Endpoint,
    },
    // 其他消息类型暂不实现
}

/// VM 服务发送的响应类型
pub enum VmResponse {
    /// Fork 成功
    ForkOk,
    /// Fork 失败
    ForkError(VmForkError),
}

/// Fork 错误类型
pub enum VmForkError {
    /// 父进程不存在
    ParentNotFound,
    /// 子进程 slot 已被占用
    ChildSlotInUse,
    /// 内存不足
    OutOfMemory,
    /// 页表复制失败
    PageTableCopyFailed,
}
```

### Phase 2: 消息分发器

**文件**: `vm/src/ipc/dispatcher.rs` (新增)

实现消息路由：

```rust
pub struct MessageDispatcher;

impl MessageDispatcher {
    pub fn dispatch(
        table: &'static VmProcTable,
        request: VmRequest,
    ) -> VmResponse {
        match request {
            VmRequest::Fork { parent_endpoint, child_slot, child_endpoint } => {
                handle_fork(table, parent_endpoint, child_slot, child_endpoint)
            }
        }
    }
}
```

### Phase 3: Fork 处理器

**文件**: `vm/src/fork.rs` (已存在，需完善)

实现核心 fork 逻辑：

```rust
pub fn handle_fork(
    table: &VmProcTable,
    parent_endpoint: Endpoint,
    child_slot: UserSlot,
    child_endpoint: Endpoint,
) -> VmResponse {
    // 1. 查找父进程
    let parent = find_parent(table, parent_endpoint)?;
    
    // 2. 获取子进程空 slot
    let child_empty = table.get_empty(child_slot)
        .ok_or(VmForkError::ChildSlotInUse)?;
    
    // 3. 激活子进程
    let mut child = child_empty.activate(child_endpoint);
    
    // 4. 初始化子进程页表
    child.init_page_table()?;
    
    // 5. 复制父进程的区域到子进程 (CoW)
    copy_regions_cow(&parent, &mut child)?;
    
    // 6. 复制父进程的页表到子进程 (CoW)
    copy_page_table_cow(&parent, &mut child)?;
    
    VmResponse::ForkOk
}
```

### Phase 4: 区域复制 (CoW)

**文件**: `vm/src/region/region_ops.rs` (新增)

```rust
/// 复制区域列表，设置 CoW 标志
pub fn copy_regions_cow(
    parent: &ActiveProc,
    child: &mut ActiveProc,
) -> Result<(), VmForkError> {
    for region in parent.regions() {
        // 设置 CoW 标志
        let mut new_region = region.clone();
        new_region.flags |= VrFlags::COW;
        
        // 增加物理块引用计数
        for block in region.phys_blocks() {
            block.add_ref();
        }
        
        child.add_region(new_region)?;
    }
    Ok(())
}
```

### Phase 5: 页表复制 (CoW)

**文件**: `vm/src/pagetable/mod.rs` (需完善)

```rust
/// 复制页表，设置 CoW 标志
pub fn copy_page_table_cow(
    parent: &ActiveProc,
    child: &mut ActiveProc,
) -> Result<(), VmForkError> {
    let parent_pt = parent.page_table();
    let child_pt = child.page_table_mut();
    
    // 遍历父进程的所有映射
    for (vaddr, paddr, flags) in parent_pt.iter_mappings() {
        // 设置只读 + CoW
        let cow_flags = flags.make_readonly();
        
        // 在子进程页表中创建相同映射
        child_pt.map(vaddr, paddr, cow_flags)?;
        
        // 同时更新父进程页表为只读
        parent_pt.remap(vaddr, cow_flags)?;
    }
    Ok(())
}
```

---

## 3. 依赖关系

```
fork.rs
├── vmproc/table.rs (进程表)
├── vmproc/vmproc_handle.rs (ActiveProc)
├── region/ (区域管理)
│   ├── vir_region.rs
│   └── phys_region.rs
├── pagetable/ (页表抽象)
│   └── mod.rs
└── minix-arch (Paging trait)
```

---

## 4. 文件变更清单

| 文件 | 操作 | 说明 |
|------|------|------|
| `minix-types/src/ipc/vm.rs` | 新增 | VM IPC 消息类型 |
| `minix-types/src/ipc/mod.rs` | 修改 | 导出 vm 模块 |
| `vm/src/ipc/mod.rs` | 新增 | IPC 模块 |
| `vm/src/ipc/dispatcher.rs` | 新增 | 消息分发器 |
| `vm/src/fork.rs` | 完善 | Fork 处理逻辑 |
| `vm/src/region/region_ops.rs` | 新增 | 区域复制操作 |
| `vm/src/pagetable/mod.rs` | 完善 | 页表复制操作 |
| `vm/src/lib.rs` | 修改 | 导出新模块 |

---

## 5. 测试计划

### 5.1 单元测试

- [ ] `VmRequest::Fork` 消息序列化/反序列化
- [ ] `handle_fork` 正常路径
- [ ] `handle_fork` 父进程不存在
- [ ] `handle_fork` 子进程 slot 被占用
- [ ] `copy_regions_cow` 区域复制
- [ ] `copy_page_table_cow` 页表复制

### 5.2 集成测试

- [ ] 完整 fork 流程：创建父进程 → fork → 验证子进程状态
- [ ] CoW 验证：fork 后写入，验证物理页分离

---

## 6. 验收标准

- [ ] 所有测试通过
- [ ] 代码通过 `cargo clippy` 无警告
- [ ] 代码通过 `cargo fmt`
- [ ] 所有公开 API 有文档注释
- [ ] 所有 `unsafe` 块有 Safety 注释
