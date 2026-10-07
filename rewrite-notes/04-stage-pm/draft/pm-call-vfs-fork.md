# PM 调用 VFS Fork（PM 侧实现）

## 1. C 源码分析

### 1.1 PM 中的调用点

**文件**: `minix3/minix/servers/pm/forkexit.c`
- **位置**: 第 145-165 行

```c
  // 构建 VFS 消息
  memset(&m, 0, sizeof(m));
  m.m_type = VFS_PM_FORK;
  m.VFS_PM_ENDPT = rmc->mp_endpoint;    // 子进程 endpoint
  m.VFS_PM_PENDPT = rmp->mp_endpoint;   // 父进程 endpoint
  m.VFS_PM_CPID = rmc->mp_pid;          // 子进程 PID
  m.VFS_PM_REUID = -1;                  // 使用父进程 uid
  m.VFS_PM_REGID = -1;                  // 使用父进程 gid

  tell_vfs(rmc, &m);  // 通知 VFS

  // 如果有跟踪器，发送 SIGSTOP
  if (rmc->mp_tracer != NO_TRACER)
    sig_proc(rmc, SIGSTOP, TRUE, FALSE);

  return SUSPEND;  // 挂起，等待 VFS 回复
```

### 1.2 tell_vfs 详细实现

**文件**: `minix3/minix/servers/pm/utility.c`

```c
void tell_vfs(rmp, m_ptr)
struct mproc *rmp;
message *m_ptr;
{
  int r;
  if (rmp->mp_flags & (VFS_CALL | EVENT_CALL))
    panic("tell_vfs: not idle: %d", m_ptr->m_type);

  r = asynsend3(VFS_PROC_NR, m_ptr, AMF_NOREPLY);
  if (r != OK)
    panic("unable to send to VFS: %d", r);

  rmp->mp_flags |= VFS_CALL;
}
```

**关键点**:
- 使用 `asynsend3`（异步发送，不等待回复）
- 设置 `VFS_CALL` 标志防止重复发送
- 如果进程已经有 `VFS_CALL` 或 `EVENT_CALL`，则 panic

**SUSPEND 机制**:
- PM 发送消息后，立即返回 `SUSPEND` 状态码
- 内核将 PM 进程标记为等待回复状态
- VFS 处理完成后发送回复消息
- 内核唤醒 PM 进程，继续执行

## 2. Rust 实现设计

> **Mock 边界说明**:
> - ✅ PM ↔ VFS 之间的 IPC 调用：**真实实现**
> - ✅ VFS 内部逻辑（文件描述符复制、引用计数）：**真实实现**
> - ❌ VFS 访问的磁盘 I/O 操作：**Mock 实现**

### 2.1 VFS 消息结构

```rust
// os/libs/minix-types/src/messages/vfs_pm.rs

/// VFS_PM_FORK 消息类型
pub const VFS_PM_FORK: MessageType = MessageType(0x2001);

/// PM 发送给 VFS 的 fork 请求
#[repr(C)]
pub struct VfsPmForkRequest {
    pub m_type: MessageType,      // VFS_PM_FORK
    pub vfs_pm_endpt: Endpoint,   // 子进程 endpoint
    pub vfs_pm_pendpt: Endpoint,  // 父进程 endpoint
    pub vfs_pm_cpid: Pid,         // 子进程 PID
    pub vfs_pm_reuid: i32,        // 有效 uid（-1 表示继承）
    pub vfs_pm_regid: i32,        // 有效 gid（-1 表示继承）
}

/// VFS 回复给 PM 的消息
#[repr(C)]
pub struct VfsPmForkReply {
    pub m_type: MessageType,
    pub status: i32,              // 0 表示成功，错误码表示失败
}
```

### 2.2 VFS 客户端实现

```rust
// os/servers/pm/src/vfs_client.rs

use minix_types::messages::{VfsPmForkRequest, VfsPmForkReply, VFS_PM_FORK};
use minix_types::{Endpoint, Pid, Error};

pub struct VfsClient {
    vfs_endpoint: Endpoint,  // VFS 服务的 endpoint
}

impl VfsClient {
    pub fn new() -> Self {
        Self {
            vfs_endpoint: Endpoint::from_raw(VFS_PROC_NR as u32),
        }
    }

    /// 通知 VFS 创建子进程的文件描述符
    /// 
    /// # 注意
    /// 此函数是异步的，调用后 PM 进程会进入 SUSPEND 状态
    /// 等待 VFS 的回复
    pub fn notify_fork(
        &self,
        child: &MProc,
        parent: &MProc,
    ) -> Result<(), Error> {
        let req = VfsPmForkRequest {
            m_type: VFS_PM_FORK,
            vfs_pm_endpt: child.endpoint,
            vfs_pm_pendpt: parent.endpoint,
            vfs_pm_cpid: child.pid,
            vfs_pm_reuid: -1,  // 继承父进程 uid
            vfs_pm_regid: -1,  // 继承父进程 gid
        };

        // 异步发送给 VFS
        // 使用 notify 而不是 send，因为不需要立即等待回复
        ipc::notify(self.vfs_endpoint, &req)
            .map_err(|_| Error::EAGAIN)?;

        Ok(())
    }
}
```

### 2.3 SUSPEND 机制实现

```rust
// os/servers/pm/src/suspend.rs

use minix_types::{Endpoint, Error, ProcessState};
use crate::mproc::{MProc, ProcessFlags};

bitflags! {
    /// RTS (Run Time Status) 标志
    pub struct RtsFlags: u32 {
        const SENDING = 0x0001;     // 正在发送消息
        const RECEIVING = 0x0002;   // 正在接收消息
        const SIGNALED = 0x0004;    // 收到信号
        const SIG_PENDING = 0x0008; // 信号待处理
        const P_STOP = 0x0010;      // 被跟踪停止
        const NO_PRIV = 0x0020;     // 无特权
    }
}

/// 设置进程为 SUSPEND 状态，等待 VFS 回复
/// 
/// # 参数
/// - proc: 要挂起的进程
/// - waiting_for: 等待的服务 endpoint
/// 
/// # 返回值
/// - Ok(()): 成功设置 SUSPEND 状态
/// - Err(Error): 设置失败
pub fn suspend_for_vfs_reply(
    proc: &mut MProc,
    vfs_ep: Endpoint,
) -> Result<(), Error> {
    // 设置进程状态标志
    proc.flags |= ProcessFlags::SUSPENDED;
    proc.rts_flags |= RtsFlags::SENDING;
    proc.waiting_for = Some(vfs_ep);

    // 通知调度器将此进程移出运行队列
    // 在 Mock 环境中，这只是状态记录
    log::debug!(
        "Process {} suspended, waiting for VFS reply",
        proc.endpoint
    );

    Ok(())
}

/// 处理 VFS 的 fork 回复
/// 
/// # 参数
/// - proc: 被挂起的进程
/// - status: VFS 返回的状态码
/// 
/// # 返回值
/// - Ok(()): VFS fork 成功
/// - Err(Error): VFS fork 失败
pub fn handle_vfs_fork_reply(
    proc: &mut MProc,
    status: i32,
) -> Result<(), Error> {
    // 清除 SUSPEND 状态
    proc.flags &= !ProcessFlags::SUSPENDED;
    proc.rts_flags &= !RtsFlags::SENDING;
    proc.waiting_for = None;

    log::debug!(
        "Process {} resumed from SUSPEND, VFS status: {}",
        proc.endpoint, status
    );

    if status == 0 {
        Ok(())
    } else {
        Err(Error::from_raw(status))
    }
}

/// 检查进程是否处于 SUSPEND 状态
pub fn is_suspended(proc: &MProc) -> bool {
    proc.flags.contains(ProcessFlags::SUSPENDED)
}

/// 获取进程正在等待的服务
pub fn waiting_for(proc: &MProc) -> Option<Endpoint> {
    proc.waiting_for
}
```

### 2.4 在 do_fork 中集成

```rust
// os/servers/pm/src/fork.rs

pub fn do_fork(ctx: &mut PmContext) -> Result<Pid, Error> {
    // ... 前面的步骤：分配槽位、调用 VM fork、复制进程结构 ...

    // 7. 调用 VFS fork（PM 侧）
    let vfs_client = VfsClient::new();
    vfs_client.notify_fork(&child_proc, &parent_proc)?;

    // 8. 挂起进程，等待 VFS 回复
    suspend_for_vfs_reply(&mut child_proc, vfs_client.vfs_endpoint)?;

    // 9. 如果有跟踪器，发送 SIGSTOP
    if child_proc.tracer != NO_TRACER {
        sig_proc(&mut child_proc, SIGSTOP, true, false)?;
    }

    // 10. 返回 SUSPEND 状态码
    // 内核会将此进程标记为等待回复状态
    Err(Error::SUSPEND)
}

/// 处理 VFS 回复的入口函数
/// 
/// 当 VFS 完成 fork 处理后，会发送回复消息给 PM
/// PM 调用此函数完成 fork 流程
pub fn complete_fork(
    ctx: &mut PmContext,
    child_slot: SlotIndex,
    vfs_status: i32,
) -> Result<Pid, Error> {
    let child = ctx.table.get_mut(child_slot)
        .ok_or(Error::ESRCH)?;

    // 处理 VFS 回复
    handle_vfs_fork_reply(child, vfs_status)?;

    // fork 完成，返回子进程 PID
    Ok(child.pid)
}
```

## 3. 与 Minix3 的差异说明

| 方面 | Minix3 (C) | Rust 重构 | 说明 |
|------|-----------|----------|------|
| 消息构造 | 直接填充 `message` 结构体 | 使用强类型的 `VfsPmForkRequest` | 类型安全 |
| SUSPEND 实现 | 内核管理 RTS 标志 | PM 显式管理 SUSPEND 状态 | 更清晰的状态机 |
| 错误处理 | 返回错误码 | 使用 `Result` 类型 | Rust 惯用 |
| VFS 实现 | 真实 VFS 服务 | **真实 VFS 服务** | 仅 IPC Mock |

## 4. 测试用例

### 4.1 成功路径

```rust
#[test]
fn test_pm_call_vfs_fork_success() {
    // 准备：创建父进程和子进程
    let mut ctx = setup_pm_context();
    let parent = create_test_process(&mut ctx, 1);
    let child_slot = alloc_slot(&mut ctx).unwrap();
    
    // 执行：调用 VFS fork
    let vfs_client = VfsClient::new();
    let result = vfs_client.notify_fork(
        &ctx.table.get(child_slot).unwrap(),
        &parent,
    );
    
    // 验证：消息发送成功
    assert!(result.is_ok());
    
    // 验证：进程进入 SUSPEND 状态
    let child = ctx.table.get(child_slot).unwrap();
    assert!(is_suspended(child));
    assert_eq!(waiting_for(child), Some(vfs_client.vfs_endpoint));
}
```

### 4.2 VFS 回复处理

```rust
#[test]
fn test_handle_vfs_fork_reply_success() {
    // 准备：创建处于 SUSPEND 状态的进程
    let mut ctx = setup_pm_context();
    let child_slot = create_suspended_process(&mut ctx);
    
    // 执行：模拟 VFS 成功回复
    let result = complete_fork(&mut ctx, child_slot, 0);
    
    // 验证：fork 完成
    assert!(result.is_ok());
    
    // 验证：进程不再处于 SUSPEND 状态
    let child = ctx.table.get(child_slot).unwrap();
    assert!(!is_suspended(child));
}

#[test]
fn test_handle_vfs_fork_reply_failure() {
    // 准备：创建处于 SUSPEND 状态的进程
    let mut ctx = setup_pm_context();
    let child_slot = create_suspended_process(&mut ctx);
    
    // 执行：模拟 VFS 失败回复（ENOMEM）
    let result = complete_fork(&mut ctx, child_slot, ENOMEM);
    
    // 验证：fork 失败，返回错误
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), Error::ENOMEM);
}
```

## 5. 参考文档

- **中层指南**: [../00-master-plan/06-phase1-pm-guide.md](../00-master-plan/06-phase1-pm-guide.md)
- **VFS 层实现**: [../04-stage-vfs/README.md](../04-stage-vfs/README.md)
- **架构分析**: [../00-master-plan/02-architecture-analysis.md](../00-master-plan/02-architecture-analysis.md)
