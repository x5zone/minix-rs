# do_srv_fork 实现

## 1. C 源码分析

**文件**: `minix3/minix/servers/pm/forkexit.c` — `do_srv_fork()`

与 `do_fork` 的关键差异：

| 差异点 | do_fork | do_srv_fork |
|--------|---------|-------------|
| 调用者 | 任意进程 | 仅 RS（Restart Server） |
| PRIV_PROC 继承 | 不继承 | 继承 |
| UID/GID | 继承父进程 | 从消息中获取 |
| 返回值 | SUSPEND | 子进程 PID |
| VFS 消息类型 | VFS_PM_FORK | VFS_PM_SRV_FORK |
| 子进程唤醒 | 等待 VFS | 立即 reply |

```c
int do_srv_fork(void) {
  if (mp->mp_endpoint != RS_PROC_NR)
    return EPERM;  // 只有 RS 可以调用

  // ... 与 do_fork 类似的前半部分 ...

  // 关键差异：继承 PRIV_PROC
  rmc->mp_flags &= (IN_USE|PRIV_PROC|DELAY_CALL);

  // 关键差异：从消息中获取 UID/GID
  rmc->mp_realuid = m_in.m_lsys_pm_srv_fork.uid;
  rmc->mp_effuid = m_in.m_lsys_pm_srv_fork.uid;
  rmc->mp_svuid = m_in.m_lsys_pm_srv_fork.uid;
  rmc->mp_realgid = m_in.m_lsys_pm_srv_fork.gid;
  rmc->mp_effgid = m_in.m_lsys_pm_srv_fork.gid;
  rmc->mp_svgid = m_in.m_lsys_pm_srv_fork.gid;

  // 关键差异：VFS_PM_SRV_FORK
  m.m_type = VFS_PM_SRV_FORK;
  m.VFS_PM_REUID = m_in.m_lsys_pm_srv_fork.uid;
  m.VFS_PM_REGID = m_in.m_lsys_pm_srv_fork.gid;

  // 关键差异：立即唤醒子进程
  reply(rmc-mproc, OK);
  return rmc->mp_pid;  // 返回子进程 PID，不是 SUSPEND
}
```

## 2. Rust 实现设计

### 接口定义

```rust
// os/servers/pm/src/srv_fork.rs

/// RS 服务进程 fork
/// 
/// # 权限
/// 仅 RS (Restart Server) 可以调用
/// 
/// # 参数
/// - uid: 子进程的真实/有效/保存 UID
/// - gid: 子进程的真实/有效/保存 GID
/// 
/// # 返回值
/// - Ok(pid): 子进程 PID
/// - Err(EPERM): 调用者不是 RS
pub fn do_srv_fork(
    ctx: &mut PmContext,
    uid: Uid,
    gid: Gid,
) -> Result<Pid, Error> {
    let caller = ctx.current_proc();
    
    // 权限检查：只有 RS 可以调用
    if caller.endpoint != RS_ENDPOINT {
        return Err(Error::EPERM);
    }
    
    // 分配槽位
    let child_index = ctx.table.alloc_slot()
        .ok_or(Error::EAGAIN)?;
    
    // 调用 VM fork
    let child_ep = vm_fork(caller.endpoint, child_index)?;
    
    // 分配 PID
    let child_pid = ctx.pid_generator.alloc();
    
    // 创建子进程（继承 PRIV_PROC）
    let mut child = Process::srv_fork_from(
        caller,
        child_index,
        child_pid,
        child_ep,
        caller.index,
        uid,
        gid,
    );
    
    // 设置特权标志
    child.flags.insert(ProcFlags::PRIV_PROC);
    
    // 通知 VFS（SRV_FORK 类型）
    notify_vfs_srv_fork(&child, uid, gid)?;
    
    // 立即唤醒子进程（关键差异）
    reply_to_proc(&child, Ok(()))?;
    
    // 插入进程表
    ctx.table.insert(child_index, child);
    
    // 返回子进程 PID（不是 SUSPEND）
    Ok(child_pid)
}

impl Process {
    /// 服务进程 fork 专用构造
    pub fn srv_fork_from(
        parent: &Process,
        child_index: usize,
        child_pid: Pid,
        child_endpoint: Endpoint,
        parent_index: usize,
        uid: Uid,
        gid: Gid,
    ) -> Self {
        let mut proc = Self::fork_from(
            parent,
            child_index,
            child_pid,
            child_endpoint,
            parent_index,
        );
        
        // 覆盖 UID/GID（从消息获取，不是继承）
        proc.resources.privilege = Privilege {
            real_uid: uid,
            eff_uid: uid,
            saved_uid: uid,
            real_gid: gid,
            eff_gid: gid,
            saved_gid: gid,
            groups: Vec::new(),
        };
        
        proc
    }
}
```

## 3. VFS SRV_FORK 消息

```rust
/// VFS 服务进程 fork 消息
pub struct VfsSrvForkMessage {
    pub msg_type: VfsMessageType,  // VFS_PM_SRV_FORK
    pub child_endpoint: Endpoint,
    pub parent_endpoint: Endpoint,
    pub child_pid: Pid,
    pub real_uid: Uid,  // 从消息获取
    pub real_gid: Gid,  // 从消息获取
}
```

## 4. 验证目标

- [ ] 非 RS 进程调用返回 EPERM
- [ ] PRIV_PROC 标志正确继承
- [ ] UID/GID 从消息中正确获取
- [ ] 子进程被立即唤醒
- [ ] 返回子进程 PID（非 SUSPEND）
