# do_exit 实现

## 1. C 源码分析

**文件**: `minix3/minix/servers/pm/forkexit.c`

### do_exit 简化版

```c
int do_exit(void) {
  if(mp->mp_flags & PRIV_PROC) {
    // 系统进程不允许调用 exit()
    sys_kill(mp->mp_endpoint, SIGKILL);
  } else {
    exit_proc(mp, m_in.m_lc_pm_exit.status, FALSE /*dump_core*/);
  }
  return(SUSPEND);
}
```

### exit_proc 核心流程

```
① 记住进程组 ID（会话领导者）
② 取消定时器
③ 获取 CPU 使用时间
④ 停止进程（sys_stop）
⑤ 通知 VM（vm_willexit）
⑥ 通知 VFS（VFS_PM_EXIT / VFS_PM_DUMPCORE）
⑦ 特权进程：直接 sys_clear
⑧ 设置 EXITING 标志
⑨ 保存退出状态
⑩ zombify（变成僵尸）
⑪ 子进程 disinherite（转给 INIT）
⑫ 发送 SIGHUP 给进程组
```

### cleanup 函数

```c
static void cleanup(register struct mproc *rmp) {
  rmp->mp_pid = 0;
  rmp->mp_flags = 0;
  rmp->mp_child_utime = 0;
  rmp->mp_child_stime = 0;
  procs_in_use--;
}
```

## 2. Rust 实现设计

### 接口定义

```rust
// os/servers/pm/src/exit.rs

/// exit 系统调用入口
/// 
/// # 返回值
/// - SUSPEND: 正常情况，进程进入退出流程
/// - 其他: 错误码
pub fn do_exit(ctx: &mut PmContext, status: i32) -> Result<(), Error> {
    let proc = ctx.current_proc();
    
    // 系统进程不允许 exit
    if proc.is_privileged() {
        sys_kill(proc.endpoint, SIGKILL)?;
        return Ok(());
    }
    
    exit_proc(ctx, status, false)
}

/// 进程退出核心函数
/// 
/// # 参数
/// - ctx: PM 上下文
/// - status: 退出状态码
/// - dump_core: 是否生成 core dump
fn exit_proc(
    ctx: &mut PmContext,
    status: i32,
    dump_core: bool,
) -> Result<(), Error> {
    let proc = ctx.current_proc_mut();
    let procgrp = proc.identity.procgrp;
    
    // ① 取消定时器
    cancel_timers(proc);
    
    // ② 停止进程
    sys_stop(proc.endpoint)?;
    
    // ③ 通知 VM
    vm_willexit(proc.endpoint)?;
    
    // ④ 通知 VFS
    notify_vfs_exit(proc, dump_core)?;
    
    // ⑤ 设置退出状态
    proc.state.set_exiting(status);
    
    // ⑥ 变成僵尸
    zombify(proc)?;
    
    // ⑦ 子进程转给 INIT
    disinherit_children(ctx, proc.index)?;
    
    // ⑧ 如果是会话领导者，发送 SIGHUP
    if is_session_leader(proc) {
        send_sighup_to_pgrp(ctx, procgrp)?;
    }
    
    Ok(())
}
```

## 3. 关键步骤详解

### 3.1 通知 VFS

```rust
fn notify_vfs_exit(proc: &mut Process, dump_core: bool) -> Result<(), Error> {
    let msg_type = if dump_core {
        VfsMessageType::DumpCore
    } else {
        VfsMessageType::Exit
    };
    
    let msg = VfsExitMessage {
        msg_type,
        endpoint: proc.endpoint,
        status: proc.state.exit_status(),
    };
    
    tell_vfs(proc, &msg)?;
    proc.flags.insert(ProcFlags::VFS_CALL);
    
    Ok(())
}
```

### 3.2 子进程转给 INIT

```rust
fn disinherit_children(ctx: &mut PmContext, parent_index: SlotIndex) -> Result<(), Error> {
    for proc in ctx.table.iter_mut() {
        if let Some(parent) = proc.state.parent() {
            if parent == parent_index {
                // 转给 INIT (槽位 0)
                proc.state.set_parent(INIT_INDEX);
                
                // 如果子进程是僵尸，通知 INIT
                if proc.is_zombie() {
                    notify_init_of_zombie(proc)?;
                }
            }
        }
    }
    Ok(())
}
```

### 3.3 清理槽位

```rust
fn cleanup(ctx: &mut PmContext, index: SlotIndex) {
    let proc = &mut ctx.table[index];
    
    // 清零关键字段
    proc.identity.pid = Pid::INVALID;
    proc.flags = ProcFlags::empty();
    proc.resources.child_utime = 0;
    proc.resources.child_stime = 0;
    
    // 释放槽位
    ctx.table.free_slot(index);
    ctx.procs_in_use -= 1;
}
```

## 4. 验证目标

- [ ] 普通进程退出流程正确
- [ ] 系统进程退出被拒绝（发送 SIGKILL）
- [ ] 僵尸状态正确设置
- [ ] 子进程被 INIT 收养
- [ ] 进程组 SIGHUP 正确发送
- [ ] 槽位最终被清理
