# do_wait4 实现

## 1. C 源码分析

**文件**: `minix3/minix/servers/pm/forkexit.c`

### do_wait4 核心逻辑

```c
int do_wait4(void) {
  pidarg = m_in.m_lc_pm_wait4.pid;
  options = m_in.m_lc_pm_wait4.options;
  addr = m_in.m_lc_pm_wait4.addr;
  if (pidarg == 0) pidarg = -mp->mp_procgrp;

  // 遍历进程表，查找符合条件的子进程
  for (rp = &mproc[0]; rp < &mproc[NR_PROCS]; rp++) {
    if ((rp->mp_flags & (IN_USE | TOLD_PARENT)) != IN_USE) continue;
    if (rp->mp_parent != who_p && rp->mp_tracer != who_p) continue;
    if (rp->mp_parent != who_p && (rp->mp_flags & ZOMBIE)) continue;

    // pidarg 过滤
    if (pidarg > 0 && pidarg != rp->mp_pid) continue;
    if (pidarg < -1 && -pidarg != rp->mp_procgrp) continue;

    children++;

    // 处理追踪僵尸
    if (rp->mp_tracer == who_p && (rp->mp_flags & TRACE_ZOMBIE)) { ... }
    // 处理追踪停止
    if (rp->mp_tracer == who_p && (rp->mp_flags & TRACE_STOPPED)) { ... }
    // 处理普通僵尸
    if (rp->mp_parent == who_p && (rp->mp_flags & ZOMBIE)) { ... }
  }

  // 没有符合条件的子进程已退出
  if (children > 0) {
    if (options & WNOHANG) return 0;  // WNOHANG：不等待
    mp->mp_flags |= WAITING;          // 设置等待标志
    return SUSPEND;                    // 挂起
  } else {
    return ECHILD;                     // 没有子进程
  }
}
```

## 2. pidarg 参数解析

| pidarg 值 | 含义 | 过滤条件 |
|-----------|------|---------|
| `> 0` | 等待指定 PID 的子进程 | `rp->mp_pid == pidarg` |
| `-1` | 等待任意子进程 | 无额外过滤 |
| `0` | 等待同一进程组的子进程 | `rp->mp_procgrp == current->procgrp` |
| `< -1` | 等待指定进程组的子进程 | `rp->mp_procgrp == -pidarg` |

## 3. Rust 实现设计

### 接口定义

```rust
// os/servers/pm/src/wait.rs

/// wait4 系统调用入口
/// 
/// # 参数
/// - pid: 等待的进程 ID（见 pidarg 解析表）
/// - options: 选项标志（WNOHANG 等）
/// - addr: rusage 结构地址（可选）
/// 
/// # 返回值
/// - Ok(pid): 子进程 PID
/// - Ok(0): WNOHANG 且没有子进程退出
/// - Err(ECHILD): 没有符合条件的子进程
/// - Err(SUSPEND): 挂起等待子进程
pub fn do_wait4(
    ctx: &mut PmContext,
    pid: PidArg,
    options: WaitOptions,
    addr: Option<VirAddr>,
) -> Result<Pid, Error> {
    let current = ctx.current_proc();
    let target_pgrp = match pid {
        PidArg::Any => None,
        PidArg::Specific(p) => return wait_specific(ctx, p, options, addr),
        PidArg::SameGroup => Some(current.identity.procgrp),
        PidArg::Group(g) => Some(g),
    };
    
    // 遍历查找符合条件的子进程
    for child in ctx.table.iter_mut() {
        if !is_waitable(child, current.index)? {
            continue;
        }
        
        // 进程组过滤
        if let Some(pgrp) = target_pgrp {
            if child.identity.procgrp != pgrp {
                continue;
            }
        }
        
        // 检查子进程状态
        if let Some(result) = check_child_status(ctx, child, options, addr)? {
            return Ok(result);
        }
    }
    
    // 没有找到已退出的子进程
    if has_children(ctx, current.index, pid)? {
        if options.contains(WaitOptions::WNOHANG) {
            return Ok(Pid::new(0));
        }
        // 挂起等待
        current.state.set_waiting(pid);
        return Err(Error::SUSPEND);
    }
    
    Err(Error::ECHILD)
}

/// 检查子进程状态并处理
fn check_child_status(
    ctx: &mut PmContext,
    child: &mut Process,
    options: WaitOptions,
    addr: Option<VirAddr>,
) -> Result<Option<Pid>, Error> {
    let current = ctx.current_proc();
    
    // 处理追踪僵尸（被追踪的进程）
    if child.is_trace_zombie() && child.state.tracer() == Some(current.index) {
        let pid = child.identity.pid;
        cleanup_trace_zombie(ctx, child)?;
        return Ok(Some(pid));
    }
    
    // 处理追踪停止
    if child.is_trace_stopped() && child.state.tracer() == Some(current.index) {
        let pid = child.identity.pid;
        if !options.contains(WaitOptions::WCONTINUED) {
            return Ok(Some(pid));
        }
    }
    
    // 处理普通僵尸
    if child.is_zombie() && child.state.parent() == Some(current.index) {
        let pid = child.identity.pid;
        let status = child.state.exit_status();
        
        // 复制 rusage 信息（如果请求）
        if let Some(addr) = addr {
            copy_rusage(ctx, child, addr)?;
        }
        
        // 清理僵尸
        cleanup(ctx, child.index)?;
        
        return Ok(Some(pid));
    }
    
    Ok(None)
}
```

## 4. 验证目标

- [ ] `pidarg > 0`：等待指定 PID 的子进程
- [ ] `pidarg == -1`：等待任意子进程
- [ ] `pidarg < -1`：等待指定进程组的子进程
- [ ] `WNOHANG` 选项正确处理
- [ ] 僵尸子进程正确回收
- [ ] 追踪子进程正确处理
