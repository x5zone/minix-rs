# Fork 系统调用纵向切片重构计划 — Part 2：核心实现阶段

> **范围**: 阶段 4~10（do_fork 后半部分、VM/VFS 交互、exit/wait/srv_fork、集成测试）
> **前置**: 完成 [Part 1](fork-syscall-plan-part1.md) 中的阶段 1~3

---

## 第四阶段：do_fork 核心逻辑（下）— 进程结构复制与初始化

**状态**: ❌ 待实现（部分代码已写，需修正）

**目标**: 重写 fork 后半部分 - 进程结构复制与初始化

### 4.1 C 源码分析

**文件**: `minix3/minix/servers/pm/forkexit.c` — `do_fork()` 第 90~145 行

```c
  // ④ 获取子进程槽位指针，增加计数
  rmc = &mproc[next_child];
  procs_in_use++;

  // ⑤ 复制父进程 mproc 到子进程（整体拷贝）
  *rmc = *rmp;

  // ⑥ 恢复 mp_sigact 指针（因为 *rmc = *rmp 会覆盖指针）
  rmc->mp_sigact = mpsigact[next_child];
  memcpy(rmc->mp_sigact, rmp->mp_sigact, sizeof(mpsigact[next_child]));

  // ⑦ 设置父子关系
  rmc->mp_parent = who_p;  // who_p 是发起 fork 的进程索引

  // ⑧ 清除追踪器（非 TRACEFORK 场景）
  if (!(rmc->mp_trace_flags & TO_TRACEFORK)) {
    rmc->mp_tracer = NO_TRACER;
    rmc->mp_trace_flags = 0;
    sigemptyset(&rmc->mp_sigtrace);
  }

  // ⑨ 特权进程处理
  if (rmc->mp_flags & PRIV_PROC) {
    assert(rmc->mp_scheduler == NONE);
    rmc->mp_scheduler = SCHED_PROC_NR;
  }

  // ⑩ 继承/重置标志位和统计信息
  rmc->mp_flags &= (IN_USE|DELAY_CALL|TAINTED);
  rmc->mp_child_utime = 0;
  rmc->mp_child_stime = 0;
  rmc->mp_exitstatus = 0;
  rmc->mp_sigstatus = 0;
  rmc->mp_endpoint = child_ep;  // 从 VM 返回的端点
  for (i = 0; i < NR_ITIMERS; i++)
    rmc->mp_interval[i] = 0;
  rmc->mp_started = getticks();

  assert(rmc->mp_eventsub == NO_EVENTSUB);

  // ⑪ 分配 PID
  new_pid = get_free_pid();
  rmc->mp_pid = new_pid;
```

### 4.2 字段复制规则（逐字段分析）

**从 C 代码 `rmc->mp_flags &= (IN_USE|DELAY_CALL|TAINTED)` 推导**：

| Minix3 字段 | fork 行为 | Rust 对应 | 说明 |
|------------|----------|----------|------|
| `mp_pid` | 分配新 PID | `identity.id.pid = new_pid` | 由 `get_free_pid()` 生成 |
| `mp_endpoint` | 从 VM 返回 | `identity.endpoint = child_ep` | **不是 PM 计算的** |
| `mp_procgrp` | 继承父进程 | `identity.procgrp = parent.procgrp` | 子进程加入同一进程组 |
| `mp_name` | 继承父进程 | `identity.name = parent.name` | 进程名相同 |
| `mp_parent` | 设为调用者 | `state.guardianship.parent = who_p` | 父进程索引 |
| `mp_tracer` | 清除（默认） | `state.guardianship.tracer = None` | 除非 TO_TRACEFORK |
| `mp_trace_flags` | 清除（默认） | `state.trace = default()` | 除非 TO_TRACEFORK |
| `mp_flags` | 只保留 IN_USE/DELAY_CALL/TAINTED | `state.lifecycle = Running` | 其他标志全部清除 |
| `mp_child_utime` | 清零 | `resources.child_utime = 0` | 子进程无子进程 |
| `mp_child_stime` | 清零 | `resources.child_stime = 0` | 同上 |
| `mp_exitstatus` | 清零 | 不设置 | 尚未退出 |
| `mp_sigstatus` | 清零 | 不设置 | 同上 |
| `mp_interval[]` | 清零 | `resources.intervals = [0; 3]` | 间隔定时器清零 |
| `mp_started` | 当前时间 | `resources.started = getticks()` | 记录启动时间 |
| `mp_sigact` | 深拷贝 | `resources.signals = parent.signals.clone()` | 信号处理继承 |
| `mp_realuid/effuid/svuid` | 继承父进程 | `resources.privilege = parent.privilege.clone()` | 权限继承 |
| `mp_nice` | 继承父进程 | `resources.nice = parent.nice` | 调度优先级继承 |
| `mp_scheduler` | 特权进程设为 SCHED | `resources.scheduler` | 见⑨ |
| `mp_reply` | 不继承 | `ipc.reply = None` | 子进程无待回复消息 |
| `mp_eventsub` | 必须为 NO_EVENTSUB | `ipc.event_subscriber = None` | 断言检查 |

### 4.3 当前 Rust 实现的偏差

**文件**: `os/servers/pm/src/mproc/fork.rs` — `Process::fork_from()`

| 偏差 | 当前实现 | 正确行为 |
|------|---------|---------|
| `identity.id.index` | 设为 `parent.identity.id.index` | 应设为 `child_index`（子进程自己的索引） |
| `resources.started` | 设为 `parent.resources.started` | 应设为 `getticks()`（当前时间） |
| `resources.intervals` | 继承父进程 | 应清零 `[0; 3]` |
| `resources.flags` | 继承父进程 | 只保留 TAINTED，清除其他 |
| `resources.scheduler` | 继承父进程 | 特权进程应设为 `SCHED_PROC_NR` |
| `ipc.event_subscriber` | 未处理 | 应设为 `None` 并断言 |

### 4.4 修正后的 Rust 实现

```rust
impl Process {
    pub fn fork_from(
        parent: &Process,
        child_index: usize,
        child_pid: Pid,
        child_endpoint: Endpoint,
        parent_index: usize,
    ) -> Self {
        let mut flags = RemainingFlags::empty();
        if parent.resources.flags.contains(RemainingFlags::TAINTED) {
            flags |= RemainingFlags::TAINTED;
        }

        Self {
            identity: ProcessIdentity {
                id: ProcessId {
                    index: ProcIndex::new(child_index),  // 修正：子进程自己的索引
                    pid: child_pid,
                },
                endpoint: child_endpoint,
                procgrp: parent.identity.procgrp,
                name: parent.identity.name,
            },
            state: ProcessState {
                lifecycle: Lifecycle::Running,
                block: BlockState::default(),
                wait: WaitState::default(),
                guardianship: Guardianship::Normal {
                    parent: ProcIndex::new(parent_index),
                },
                trace: TraceState::default(),
            },
            resources: ProcessResources {
                privilege: parent.resources.privilege.clone(),
                signals: parent.resources.signals.clone(),
                child_utime: 0,
                child_stime: 0,
                started: 0,  // TODO: getticks()
                timer: None,
                intervals: [0; NR_ITIMERS],  // 修正：清零
                nice: parent.resources.nice,
                scheduler: parent.resources.scheduler,  // TODO: 特权进程处理
                flags,  // 修正：只保留 TAINTED
            },
            ipc: ProcessIpc::default(),
        }
    }
}
```

### 4.5 验证目标

- [ ] 子进程索引正确（不是父进程索引）
- [ ] 间隔定时器清零
- [ ] 标志位只保留 TAINTED
- [ ] 特权进程的 scheduler 正确设置
- [ ] 父子关系正确建立
- [ ] 信号处理函数正确继承

---

## 第五阶段：VM Fork 调用（Mock 阶段）

**状态**: ❌ 待实现

**目标**: 实现 `vm_fork` 的 mock 版本

### 5.1 C 源码分析

**调用点**: `minix3/minix/servers/pm/forkexit.c` 第 82 行

```c
if((s=vm_fork(rmp->mp_endpoint, next_child, &child_ep)) != OK) {
    return s;
}
```

**参数说明**：
- `rmp->mp_endpoint`: 父进程的 endpoint
- `next_child`: 子进程的槽位索引
- `&child_ep`: 输出参数，VM 返回的子进程 endpoint

**VM 内部行为**（`minix3/minix/servers/vm/fork.c`）：
1. 分配 `vmproc[next_child]` 槽位
2. 复制父进程地址空间（Copy-on-Write）
3. 调用 `sys_fork()` 创建内核进程
4. 生成新的 endpoint：`child_ep = _ENDPOINT(generation, next_child)`
5. 返回 `child_ep` 给 PM

**关键约束**：

```c
/* PM may not fail fork after call to vm_fork(), as VM calls sys_fork(). */
```

PM 调用 `vm_fork()` 后不能再失败，因为 VM 已经调用了 `sys_fork()` 创建了内核进程。

### 5.2 Rust 实现设计

```rust
/// VM fork 结果
pub struct VmForkResult {
    /// 子进程的 endpoint（由 VM/Kernel 生成）
    pub child_endpoint: Endpoint,
}

/// VM fork mock
///
/// 模拟 VM 的行为：
/// 1. 分配 vmproc 槽位
/// 2. 复制地址空间
/// 3. 生成新 endpoint
pub fn vm_fork_mock(parent_endpoint: Endpoint, child_index: usize) -> Result<VmForkResult, VmError> {
    let child_endpoint = ProcTable::calculate_endpoint(child_index);
    Ok(VmForkResult { child_endpoint })
}
```

### 5.3 验证目标

- [ ] `vm_fork` mock 能正确返回新的 endpoint
- [ ] 错误处理路径正确
- [ ] PM 在 `vm_fork` 后不失败的约束得到遵守

---

## 第六阶段：VFS 通知与 SUSPEND 机制

**状态**: ❌ 待实现

**目标**: 实现 `tell_vfs` 和 SUSPEND 机制

### 6.1 C 源码分析

**文件**: `minix3/minix/servers/pm/forkexit.c` 第 145~165 行

```c
  // 构建 VFS 消息
  memset(&m, 0, sizeof(m));
  m.m_type = VFS_PM_FORK;
  m.VFS_PM_ENDPT = rmc->mp_endpoint;     // 子进程 endpoint
  m.VFS_PM_PENDPT = rmp->mp_endpoint;    // 父进程 endpoint
  m.VFS_PM_CPID = rmc->mp_pid;           // 子进程 PID
  m.VFS_PM_REUID = -1;                   // 不使用
  m.VFS_PM_REGID = -1;                   // 不使用

  tell_vfs(rmc, &m);  // 异步通知 VFS

  // 如果有追踪器，发送 SIGSTOP
  if (rmc->mp_tracer != NO_TRACER)
    sig_proc(rmc, SIGSTOP, TRUE, FALSE);

  // 挂起，等待 VFS 回复
  return SUSPEND;
```

**`tell_vfs` 实现** (`minix3/minix/servers/pm/utility.c`)：

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

**SUSPEND 的含义**：
- `do_fork()` 返回 `SUSPEND` 表示不立即回复调用进程
- PM 等待 VFS 完成文件描述符复制后，再回复父进程
- 父进程的 fork() 调用被阻塞，直到 VFS 完成

### 6.2 Rust 实现设计

```rust
/// VFS 消息类型
pub enum VfsMessageType {
    Fork,
    Exit,
    DumpCore,
    SrvFork,
}

/// VFS fork 消息
pub struct VfsForkMessage {
    pub msg_type: VfsMessageType,
    pub child_endpoint: Endpoint,
    pub parent_endpoint: Endpoint,
    pub child_pid: Pid,
    pub real_uid: i32,  // -1 for regular fork
    pub real_gid: i32,  // -1 for regular fork
}

/// SUSPEND 返回值
///
/// 表示系统调用被挂起，等待外部服务（如 VFS）回复
pub const SUSPEND: i32 = -1;
```

### 6.3 验证目标

- [ ] VFS 消息正确构造
- [ ] SUSPEND 状态正确设置
- [ ] `VFS_CALL` 标志正确管理
- [ ] 后续回复能正确唤醒进程

---

## 第七阶段：do_exit 实现

**状态**: ❌ 待实现

**目标**: 实现 `do_exit` 和 `exit_proc` 函数

### 7.1 C 源码分析

**文件**: `minix3/minix/servers/pm/forkexit.c`

**`do_exit`** (简化版)：

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

**`exit_proc`** 核心流程：

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

**`cleanup`** 函数（最终清理）：

```c
static void cleanup(register struct mproc *rmp) {
  rmp->mp_pid = 0;
  rmp->mp_flags = 0;
  rmp->mp_child_utime = 0;
  rmp->mp_child_stime = 0;
  procs_in_use--;
}
```

### 7.2 验证目标

- [ ] 普通进程退出流程正确
- [ ] 系统进程退出被拒绝（发送 SIGKILL）
- [ ] 僵尸状态正确设置
- [ ] 子进程被 INIT 收养
- [ ] 进程组 SIGHUP 正确发送
- [ ] 槽位最终被清理

---

## 第八阶段：do_wait4 实现

**状态**: ❌ 待实现

**目标**: 实现 `do_wait4` 函数

### 8.1 C 源码分析

**文件**: `minix3/minix/servers/pm/forkexit.c`

**`do_wait4`** 核心逻辑：

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

### 8.2 验证目标

- [ ] `pidarg > 0`：等待指定 PID 的子进程
- [ ] `pidarg == -1`：等待任意子进程
- [ ] `pidarg < -1`：等待指定进程组的子进程
- [ ] `WNOHANG` 选项正确处理
- [ ] 僵尸子进程正确回收
- [ ] 追踪子进程正确处理

---

## 第九阶段：do_srv_fork 实现

**状态**: ❌ 待实现

**目标**: 实现服务进程专用 fork

### 9.1 C 源码分析

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

### 9.2 验证目标

- [ ] 非 RS 进程调用返回 EPERM
- [ ] PRIV_PROC 标志正确继承
- [ ] UID/GID 从消息中正确获取
- [ ] 子进程被立即唤醒
- [ ] 返回子进程 PID（非 SUSPEND）

---

## 第十阶段：集成测试

**状态**: ❌ 待实现

**目标**: 端到端测试 fork/exit/wait 流程

### 10.1 测试场景

| 场景 | 描述 | 涉及函数 |
|------|------|---------|
| 基本fork | 父进程 fork，子进程运行 | `do_fork` |
| fork后exit | 子进程退出，父进程 wait | `do_fork` + `do_exit` + `do_wait4` |
| 多子进程 | 父进程 fork 多次 | `do_fork` × N |
| 进程表满 | NR_PROCS 个进程后 fork 失败 | `do_fork` |
| PID耗尽 | NR_PIDS 个 PID 后循环复用 | `get_free_pid` |
| 僵尸回收 | 子进程退出但父进程未 wait | `do_exit` + `cleanup` |
| INIT收养 | 父进程退出，子进程被 INIT 收养 | `exit_proc` |
| 追踪fork | 被追踪的进程 fork | `do_fork` + ptrace |
| srv_fork | RS 创建服务进程 | `do_srv_fork` |
| 进程组信号 | 会话领导者退出，SIGHUP 发送 | `exit_proc` |

### 10.2 验证目标

- [ ] 所有测试场景通过
- [ ] 无内存泄漏
- [ ] 无死锁
- [ ] 进程表状态一致性
