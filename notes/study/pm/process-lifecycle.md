# PM 进程生命周期管理模块

> **模块范围**: `servers/pm/forkexit.c` + `servers/pm/exec.c`
> **核心功能**: 进程创建（fork）、程序执行（exec）、进程终止（exit）、等待子进程（wait）
> **所属服务**: PM（Process Manager）

---

## 一、模块整体定位

### 1.1 在系统中的作用

PM 进程生命周期管理模块是 Minix3 操作系统的核心组件，负责：

| 功能 | 系统调用 | 作用 |
|------|----------|------|
| 创建进程 | `fork()` | 复制当前进程，创建子进程 |
| 执行程序 | `execve()` | 加载新程序，替换当前进程映像 |
| 终止进程 | `exit()` | 结束进程，释放资源 |
| 等待子进程 | `wait4()` | 父进程等待子进程退出，收集退出状态 |

### 1.2 与其他模块的关系

```
                    ┌─────────────────────────────────────────────────────────┐
                    │                      用户进程                            │
                    │         调用 fork() / exec() / exit() / wait()          │
                    └──────────────────────────┬──────────────────────────────┘
                                               │ 系统调用
                                               ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                              PM（Process Manager）                           │
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐              │
│  │   forkexit.c    │  │     exec.c      │  │    其他模块      │              │
│  │  do_fork()      │  │   do_exec()     │  │   signal.c      │              │
│  │  do_srv_fork()  │  │  do_newexec()   │  │   time.c        │              │
│  │  do_exit()      │  │ exec_restart()  │  │   ...           │              │
│  │  exit_proc()    │  │                 │  │                 │              │
│  │  exit_restart() │  │                 │  │                 │              │
│  │  do_wait4()     │  │                 │  │                 │              │
│  └────────┬────────┘  └────────┬────────┘  └─────────────────┘              │
│           │                    │                                             │
│           │ IPC 消息           │ IPC 消息                                    │
│           ▼                    ▼                                             │
└───────────┼────────────────────┼─────────────────────────────────────────────┘
            │                    │
    ┌───────┴───────┐    ┌───────┴───────┐    ┌───────────────┐
    │      VM       │    │      VFS      │    │    Kernel     │
    │  内存管理      │    │  文件系统      │    │   进程调度    │
    │ vm_fork()     │    │ 文件加载       │    │ sys_fork()    │
    │ vm_exit()     │    │ fd 复制/清理   │    │ sys_stop()    │
    │ vm_willexit() │    │               │    │ sys_clear()   │
    └───────────────┘    └───────────────┘    └───────────────┘
```

### 1.3 核心设计约束

**微内核架构的影响**：

1. **PM 不能直接操作进程内存**：由 VM 服务管理
2. **PM 不能直接访问文件系统**：由 VFS 服务管理
3. **PM 不能直接调度进程**：由内核和调度器服务管理
4. **所有跨服务操作必须通过 IPC 消息**

---

## 二、核心数据结构

### 2.1 mproc 结构（进程控制块）

PM 维护的进程控制块数组：

```c
struct mproc mproc[NR_PROCS];  // NR_PROCS 通常为 128
```

**关键字段**：

| 字段 | 类型 | 说明 |
|------|------|------|
| `mp_pid` | `pid_t` | 进程 ID |
| `mp_endpoint` | `endpoint_t` | 进程端点号（内核标识） |
| `mp_parent` | `int` | 父进程槽位号 |
| `mp_flags` | `int` | 进程状态标志位 |
| `mp_exitstatus` | `char` | 退出状态码 |
| `mp_sigstatus` | `char` | 终止信号 |
| `mp_scheduler` | `endpoint_t` | 调度器端点 |
| `mp_tracer` | `int` | 跟踪进程槽位号 |
| `mp_realuid/mp_effuid` | `uid_t` | 真实/有效用户 ID |
| `mp_realgid/mp_effgid` | `gid_t` | 真实/有效组 ID |

### 2.2 进程状态标志位（mp_flags）

```c
#define IN_USE         0x001   // 进程槽位被占用
#define WAITING        0x002   // 父进程在等待子进程
#define ZOMBIE         0x004   // 僵尸状态
#define EXITING        0x008   // 正在退出
#define TOLD_PARENT    0x010   // 已通知父进程
#define PRIV_PROC      0x020   // 系统进程
#define VFS_CALL       0x040   // 正在等待 VFS 回复
#define PROC_STOPPED   0x080   // 进程已停止
#define TRACE_ZOMBIE   0x100   // 跟踪僵尸（已通知跟踪器）
#define TAINTED        0x200   // setuid/setgid 程序
#define PARTIAL_EXEC   0x400   // exec 进行中
#define DELAY_CALL     0x800   // 有延迟调用待处理
```

### 2.3 进程状态转换图

```
                          ┌─────────────────────────────────────────┐
                          │              进程状态转换                 │
                          └─────────────────────────────────────────┘

    fork() 成功
        │
        ▼
   ┌─────────┐
   │ RUNNING │ ◄──────────────────────────────────────────┐
   │ (IN_USE)│                                           │
   └────┬────┘                                           │
        │                                                │
        │ exit() 或信号杀死                               │
        ▼                                                │
   ┌─────────┐     VFS 回复后        ┌───────────┐       │
   │EXITING  │ ──────────────────►  │  ZOMBIE   │       │
   │(EXITING)│                      │ (ZOMBIE)  │       │
   └────┬────┘                      └─────┬─────┘       │
        │                                 │             │
        │ 通知 VFS                         │ wait()     │
        │                                 ▼             │
        │                          ┌───────────┐       │
        │                          │TOLD_PARENT│       │
        │                          │(TOLD_     │       │
        │                          │ PARENT)   │       │
        │                          └─────┬─────┘       │
        │                                │             │
        │                          cleanup()           │
        │                                │             │
        └────────────────────────────────┼─────────────┘
                                         ▼
                                   ┌───────────┐
                                   │  FREE     │
                                   │ (flags=0) │
                                   └───────────┘
```

---

## 三、核心流程

### 3.1 fork 流程

#### 3.1.1 状态机模型

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           fork 状态机                                        │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   ┌──────────┐    找到空闲槽位     ┌──────────┐    vm_fork 成功             │
│   │  START   │ ─────────────────► │ SLOT_    │ ─────────────────►          │
│   │          │                    │ ALLOCATED│                              │
│   └──────────┘                    └──────────┘                              │
│                                                          │                  │
│                                                          ▼                  │
│                                              ┌──────────────────┐           │
│                                              │  PM_READY        │           │
│                                              │  (mproc 已初始化) │           │
│                                              └────────┬─────────┘           │
│                                                       │                     │
│                                                       │ tell_vfs()          │
│                                                       ▼                     │
│                                              ┌──────────────────┐           │
│                                              │  VFS_NOTIFY      │           │
│                                              │  (等待 VFS 回复)  │           │
│                                              └────────┬─────────┘           │
│                                                       │                     │
│                                                       │ VFS 回复            │
│                                                       ▼                     │
│                                              ┌──────────────────┐           │
│                                              │  DONE            │           │
│                                              │  (子进程可运行)   │           │
│                                              └──────────────────┘           │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### 3.1.2 do_fork 详细流程

```c
int do_fork(void)
```

**阶段划分**：

| 阶段 | 操作 | 说明 |
|------|------|------|
| **1. 检查** | 进程表是否满 | `procs_in_use >= NR_PROCS` |
| **2. 分配** | 找到空闲槽位 | 轮询 `mproc[]` 数组 |
| **3. 创建** | 调用 `vm_fork()` | VM 创建地址空间，内核创建进程结构 |
| **4. 初始化** | 复制父进程状态 | `*rmc = *rmp` |
| **5. 通知** | 发送消息给 VFS | `tell_vfs()` |
| **6. 挂起** | 返回 SUSPEND | 等待 VFS 完成 fd 复制 |

**关键代码**：

```c
// 阶段 3: 调用 VM 创建进程
if((s=vm_fork(rmp->mp_endpoint, next_child, &child_ep)) != OK) {
    return s;
}

// 阶段 4: 复制父进程状态
*rmc = *rmp;  // 整体复制
rmc->mp_parent = who_p;  // 设置父进程
rmc->mp_flags &= (IN_USE|DELAY_CALL|TAINTED);  // 只继承这些标志

// 阶段 5: 通知 VFS
m.m_type = VFS_PM_FORK;
tell_vfs(rmc, &m);

// 阶段 6: 挂起等待
return SUSPEND;
```

#### 3.1.3 do_fork vs do_srv_fork 对比

| 特性 | do_fork | do_srv_fork |
|------|---------|-------------|
| 调用者 | 任意进程 | 仅 RS（重启服务） |
| 权限检查 | 无 | `mp->mp_endpoint == RS_PROC_NR` |
| PRIV_PROC 继承 | 不继承 | 继承 |
| 调度器设置 | 系统进程设为 SCHED_PROC_NR | 保持 NONE |
| UID/GID | 继承父进程 | 从消息参数获取 |
| 返回值 | SUSPEND（等待 VFS） | 立即返回子进程 PID |

**设计原因**：

- `do_srv_fork` 用于创建系统服务进程
- 系统服务不需要 VFS 参与（可能 VFS 本身正在创建）
- RS 需要立即获得子进程 PID 以便管理

---

### 3.2 exec 流程

#### 3.2.1 状态机模型

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           exec 状态机                                        │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   ┌──────────┐   do_exec()          ┌──────────┐   VFS 加载完成             │
│   │  START   │ ──────────────────► │  VFS_    │ ─────────────────►          │
│   │          │   tell_vfs()        │ LOADING  │   do_newexec()             │
│   └──────────┘   return SUSPEND    └──────────┘                            │
│                                                          │                  │
│                                                          ▼                  │
│                                              ┌──────────────────┐           │
│                                              │  PM_UPDATED      │           │
│                                              │  (mproc 已更新)   │           │
│                                              │  PARTIAL_EXEC    │           │
│                                              └────────┬─────────┘           │
│                                                       │                     │
│                                                       │ VFS 回复            │
│                                                       ▼                     │
│                                              ┌──────────────────┐           │
│                                              │  exec_restart()  │           │
│                                              │  通知内核         │           │
│                                              └────────┬─────────┘           │
│                                                       │                     │
│                                                       │ sys_exec()          │
│                                                       ▼                     │
│                                              ┌──────────────────┐           │
│                                              │  DONE            │           │
│                                              │  (新程序运行)     │           │
│                                              └──────────────────┘           │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### 3.2.2 exec 多阶段流程

```
时间线:
┌─────────────────────────────────────────────────────────────────────────────┐
│ T1: 用户进程调用 execve("/bin/ls", argv, envp)                              │
│     → 内核发送 NR_EXEC 消息给 PM                                            │
│     → PM 调用 do_exec()                                                    │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ T2: PM 构造 VFS_PM_EXEC 消息，发送给 VFS                                     │
│     → PM 返回 SUSPEND，用户进程挂起                                          │
│     → mp_flags |= VFS_CALL                                                 │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ T3: VFS 收到消息，执行：                                                     │
│     → 检查文件权限                                                          │
│     → 读取可执行文件头                                                       │
│     → 加载代码段和数据段到内存                                               │
│     → 构造初始栈（argv, envp）                                              │
│     → 发送 VFS_PM_EXEC_NEW 消息给 PM                                        │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ T4: PM 收到 VFS_PM_EXEC_NEW，调用 do_newexec()                               │
│     → 更新 mp_effuid/mp_effgid（setuid/setgid）                             │
│     → 设置 TAINTED 标志                                                     │
│     → 保存程序名 mp_name                                                    │
│     → 设置 PARTIAL_EXEC 标志                                                │
│     → 返回成功，VFS 继续处理                                                 │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ T5: VFS 完成后发送最终回复                                                   │
│     → PM 调用 exec_restart()                                                │
│     → 清除 PARTIAL_EXEC 标志                                                │
│     → 重置信号处理函数                                                       │
│     → 调用 sys_exec() 通知内核                                              │
│     → 用户进程从新程序入口开始执行                                            │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### 3.2.3 setuid/setgid 处理

```c
// do_newexec() 中的 setuid 处理逻辑

allow_setuid = 0;
rmp->mp_flags &= ~TAINTED;

// 安全规则：被 ptrace 跟踪的进程禁止 setuid
if (rmp->mp_tracer == NO_TRACER) {
    allow_setuid = 1;
}

// 如果允许且文件有 setuid 位，更新有效 ID
if (allow_setuid && args.allow_setuid) {
    rmp->mp_effuid = args.new_uid;
    rmp->mp_effgid = args.new_gid;
    rmp->mp_flags |= TAINTED;
}

// 总是更新保存的 ID
rmp->mp_svuid = rmp->mp_effuid;
rmp->mp_svgid = rmp->mp_effgid;
```

---

### 3.3 exit 流程

#### 3.3.1 状态机模型

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           exit 状态机                                        │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   ┌──────────┐   do_exit()           ┌──────────┐   sys_stop()             │
│   │ RUNNING  │ ───────────────────► │ STOPPED  │ ─────────────────►        │
│   │          │   exit_proc()        │          │   vm_willexit()           │
│   └──────────┘                      └──────────┘                           │
│                                                          │                  │
│                                                          ▼                  │
│                                              ┌──────────────────┐           │
│                                              │  VFS_NOTIFY      │           │
│                                              │  tell_vfs()      │           │
│                                              │  mp_flags|=EXITING          │
│                                              └────────┬─────────┘           │
│                                                       │                     │
│                          ┌────────────────────────────┼────────────────┐    │
│                          │                            │                │    │
│                          ▼                            ▼                │    │
│                 ┌────────────────┐          ┌────────────────┐        │    │
│                 │ 用户进程        │          │ 系统进程        │        │    │
│                 │ 等待 VFS 回复   │          │ sys_clear()    │        │    │
│                 └───────┬────────┘          │ 立即清理        │        │    │
│                         │                   └────────────────┘        │    │
│                         │ VFS 回复                                    │    │
│                         ▼                                             │    │
│              ┌──────────────────┐                                     │    │
│              │ exit_restart()   │                                     │    │
│              │ sched_stop()     │                                     │    │
│              │ sys_clear()      │                                     │    │
│              │ vm_exit()        │                                     │    │
│              └────────┬─────────┘                                     │    │
│                       │                                               │    │
│                       ▼                                               │    │
│              ┌──────────────────┐                                     │    │
│              │ zombify()        │ ◄───────────────────────────────────┘    │
│              │ mp_flags|=ZOMBIE │                                          │
│              └────────┬─────────┘                                          │
│                       │                                                    │
│                       │ 父进程 wait()                                      │
│                       ▼                                                    │
│              ┌──────────────────┐                                          │
│              │ tell_parent()    │                                          │
│              │ cleanup()        │                                          │
│              └──────────────────┘                                          │
│                                                                            │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### 3.3.2 两阶段退出设计

**为什么需要两阶段？**

| 阶段 | 函数 | 作用 | 原因 |
|------|------|------|------|
| 第一阶段 | `exit_proc()` | 停止进程，通知 VFS | VFS 需要时间清理文件资源 |
| 第二阶段 | `exit_restart()` | 清理调度器，释放内存 | VFS 完成后再清理 |

**关键时序**：

```
exit_proc() 第一阶段:
    │
    ├── sys_stop()          → 进程停止运行（内核层面）
    │   设置 RTS_PROC_STOP 标志
    │   进程不再被调度
    │
    ├── vm_willexit()       → 通知 VM 进程即将退出
    │
    ├── tell_vfs()          → 通知 VFS 清理文件资源
    │   发送 VFS_PM_EXIT 消息
    │   异步发送，不等待回复
    │
    └── 设置 EXITING 标志   → 标记进程正在退出

exit_restart() 第二阶段:
    │
    ├── sched_stop()        → 清理调度器资源
    │   从调度器管理表中移除进程
    │   释放优先级、时间片等信息
    │
    ├── sys_clear()         → 清理内核资源（用户进程）
    │   释放内核中的进程结构
    │
    ├── vm_exit()           → 清理内存资源
    │   释放地址空间
    │
    └── zombify()           → 变成僵尸
        通知父进程
```

#### 3.3.3 sys_stop() vs sched_stop() 的区别

| 操作 | 函数 | 作用层面 | 含义 |
|------|------|----------|------|
| **停止执行** | `sys_stop()` | 内核 | 设置 `RTS_PROC_STOP`，进程不再被调度执行 |
| **清理调度资源** | `sched_stop()` | 调度器 | 从调度器管理表中移除进程记录 |

**代码证据**：

```c
// sys_stop() - 内核层面停止
// minix/include/minix/syslib.h
#define sys_stop(proc_ep) sys_runctl(proc_ep, RC_STOP, 0)

// sched_stop() - 调度器层面清理
// minix/lib/libsys/sched_stop.c
int sched_stop(endpoint_t scheduler_e, endpoint_t schedulee_e)
{
    if (scheduler_e == KERNEL || scheduler_e == NONE)
        return(OK);  // 内核调度或无调度器，直接返回

    // 发送 SCHEDULING_STOP 消息给调度器服务
    m.m_lsys_sched_scheduling_stop.endpoint = schedulee_e;
    return _taskcall(scheduler_e, SCHEDULING_STOP, &m);
}
```

#### 3.3.4 系统进程退出特殊处理

```c
// 系统进程不能调用 exit()
if(mp->mp_flags & PRIV_PROC) {
    printf("PM: system process %d (%s) tries to exit(), sending SIGKILL\n",
        mp->mp_endpoint, mp->mp_name);
    sys_kill(mp->mp_endpoint, SIGKILL);
}

// 系统进程退出时立即清理，不等待 VFS
if (rmp->mp_flags & PRIV_PROC)
{
    if((r= sys_clear(rmp->mp_endpoint)) != OK)
        panic("exit_proc: sys_clear failed: %d", r);
}
```

**设计原因**：

- 系统进程可能是块设备驱动，VFS 正在等待它
- 如果等待 VFS 回复，会造成死锁
- 系统进程由 RS 管理，RS 会收到 SIGCHLD 并重启它

---

### 3.4 wait 流程

#### 3.4.1 wait 状态机

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           wait 状态机                                        │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   ┌──────────┐   do_wait4()           ┌──────────┐                         │
│   │ RUNNING  │ ───────────────────► │ CHECKING │ ─────────────┐           │
│   │ (父进程)  │                       │ 子进程状态│              │           │
│   └──────────┘                       └──────────┘              │           │
│                                           │                    │           │
│                     ┌─────────────────────┼────────────────────┤           │
│                     │                     │                    │           │
│                     ▼                     ▼                    ▼           │
│            ┌────────────────┐    ┌────────────────┐   ┌────────────────┐   │
│            │ 有僵尸子进程    │    │ 有子进程但未退出│   │ 无符合条件的   │   │
│            │ tell_parent()  │    │ 设置 WAITING   │   │ 子进程         │   │
│            │ cleanup()      │    │ return SUSPEND │   │ return ECHILD  │   │
│            │ return PID     │    └────────────────┘   └────────────────┘   │
│            └────────────────┘              │                               │
│                                            │ 子进程退出                    │
│                                            ▼                               │
│                                   ┌────────────────┐                       │
│                                   │ check_parent() │                       │
│                                   │ 唤醒父进程      │                       │
│                                   └────────────────┘                       │
│                                                                            │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### 3.4.2 do_wait4 核心逻辑

```c
int do_wait4(void)
{
    // 解析参数
    pidarg = m_in.m_lc_pm_wait4.pid;
    options = m_in.m_lc_pm_wait4.options;
    
    // 遍历所有进程
    for (rp = &mproc[0]; rp < &mproc[NR_PROCS]; rp++) {
        // 跳过不符合条件的进程
        if ((rp->mp_flags & (IN_USE | TOLD_PARENT)) != IN_USE) continue;
        if (rp->mp_parent != who_p && rp->mp_tracer != who_p) continue;
        
        // 检查 pid 匹配
        if (pidarg > 0 && pidarg != rp->mp_pid) continue;
        
        children++;  // 找到一个符合条件的子进程
        
        // 如果是僵尸，立即返回
        if (rp->mp_flags & ZOMBIE) {
            tell_parent(rp, addr);
            cleanup(rp);
            return SUSPEND;
        }
    }
    
    // 有子进程但都不是僵尸
    if (children > 0) {
        if (options & WNOHANG)
            return 0;  // WNOHANG: 不等待，立即返回
        mp->mp_flags |= WAITING;  // 设置等待标志
        return SUSPEND;  // 挂起，等待子进程退出
    }
    
    // 没有符合条件的子进程
    return ECHILD;
}
```

---

## 四、关键机制拆解

### 4.1 僵尸进程机制

**僵尸进程的形成**：

```
子进程调用 exit()
       │
       ▼
exit_proc() 设置 EXITING 标志
       │
       ▼
exit_restart() 调用 zombify()
       │
       ▼
mp_flags |= ZOMBIE
       │
       ▼
check_parent() 检查父进程状态
       │
       ├─ 父进程在等待 → tell_parent() → cleanup()
       │
       └─ 父进程未等待 → 发送 SIGCHLD → 保持 ZOMBIE 状态
```

**僵尸进程的清理**：

```c
static void cleanup(register struct mproc *rmp)
{
    rmp->mp_pid = 0;
    rmp->mp_flags = 0;
    rmp->mp_child_utime = 0;
    rmp->mp_child_stime = 0;
    procs_in_use--;
}
```

### 4.2 孤儿进程收养

```c
// exit_proc() 中处理子进程
for (rmp = &mproc[0]; rmp < &mproc[NR_PROCS]; rmp++) {
    if (rmp->mp_parent == proc_nr) {
        // 子进程被 init 收养
        rmp->mp_parent = INIT_PROC_NR;
        
        // 如果子进程已经是僵尸，通知 init
        if (rmp->mp_flags & ZOMBIE)
            check_parent(rmp, TRUE);
    }
}
```

### 4.3 ptrace 跟踪机制

```c
// zombify() 中处理跟踪进程
if (rmp->mp_tracer != NO_TRACER && rmp->mp_tracer != rmp->mp_parent) {
    rmp->mp_flags |= TRACE_ZOMBIE;  // 先通知跟踪器
    tell_tracer(rmp);
}

// tell_tracer() 完成后
child->mp_flags &= ~TRACE_ZOMBIE;
child->mp_flags |= ZOMBIE;  // 再通知父进程
```

### 4.4 异步 IPC 模式

**PM 与其他服务的通信模式**：

| 模式 | 函数 | 说明 |
|------|------|------|
| 异步发送 | `tell_vfs()` | 发送消息后立即返回，不等待回复 |
| 挂起等待 | `return SUSPEND` | 用户进程挂起，等待后续回复 |
| 回复处理 | `do_newexec()` 等 | 收到回复后继续处理 |

```c
// tell_vfs 实现
void tell_vfs(rmp, m_ptr)
{
    r = asynsend3(VFS_PROC_NR, m_ptr, AMF_NOREPLY);
    rmp->mp_flags |= VFS_CALL;  // 标记正在等待 VFS
}
```

---

## 五、边界条件与特殊分支

### 5.1 进程表满

```c
if ((procs_in_use == NR_PROCS) ||
    (procs_in_use >= NR_PROCS-LAST_FEW && rmp->mp_effuid != 0))
{
    printf("PM: warning, process table is full!\n");
    return(EAGAIN);
}
```

**LAST_FEW 机制**：保留最后 2 个槽位给 root 用户

### 5.2 INIT 进程退出

```c
if (proc_nr_e == INIT_PROC_NR)
{
    printf("PM: INIT died with exit status %d; showing stacktrace\n", exit_status);
    sys_diagctl_stacktrace(proc_nr_e);
    return;  // 不清理，系统即将崩溃
}
```

### 5.3 VFS 进程退出

```c
if (proc_nr_e == VFS_PROC_NR)
{
    panic("exit_proc: VFS died: %d", r);  // 系统崩溃
}
```

### 5.4 调度器拒绝停止

```c
if((r = sched_stop(rmp->mp_scheduler, rmp->mp_endpoint)) != OK) {
    printf("PM: The scheduler did not want to give up "
        "scheduling %s, ret=%d.\n", rmp->mp_name, r);
    // 只打印警告，不 panic
}
```

### 5.5 exec 失败处理

```c
void exec_restart(struct mproc *rmp, int result, ...)
{
    if (result != OK)
    {
        if (rmp->mp_flags & PARTIAL_EXEC)
        {
            // exec 失败，杀死进程
            sys_kill(rmp->mp_endpoint, SIGKILL);
            return;
        }
        reply(rmp-mproc, result);  // 普通失败，返回错误
        return;
    }
    // ...
}
```

---

## 六、模块交互关系

### 6.1 上游调用者

| 调用者 | 方式 | 说明 |
|--------|------|------|
| 用户进程 | 系统调用 | `fork()`, `execve()`, `exit()`, `wait4()` |
| RS（重启服务） | IPC 消息 | `do_srv_fork()`, `do_execrestart()` |
| VFS | IPC 消息 | `do_newexec()` |

### 6.2 下游依赖

| 服务 | 调用函数 | 说明 |
|------|----------|------|
| VM | `vm_fork()`, `vm_exit()`, `vm_willexit()` | 内存管理 |
| VFS | `tell_vfs()` | 文件系统操作 |
| Kernel | `sys_stop()`, `sys_clear()`, `sys_exec()` | 内核操作 |
| Scheduler | `sched_stop()` | 调度管理 |

### 6.3 IPC 消息类型

| 消息类型 | 方向 | 说明 |
|----------|------|------|
| `VFS_PM_FORK` | PM → VFS | fork 通知 |
| `VFS_PM_SRV_FORK` | PM → VFS | srv_fork 通知 |
| `VFS_PM_EXIT` | PM → VFS | exit 通知 |
| `VFS_PM_EXEC` | PM → VFS | exec 请求 |
| `VFS_PM_EXEC_NEW` | VFS → PM | exec 完成通知 |

---

## 七、Rust 重构与设计改进建议

### 7.1 类型系统改进

**当前问题**：进程状态通过标志位组合表示，容易产生非法状态

```c
// C 代码：可能产生非法组合
rmp->mp_flags |= ZOMBIE;
rmp->mp_flags |= EXITING;  // ZOMBIE + EXITING 是非法状态
```

**Rust 改进**：使用枚举表示状态

```rust
enum ProcessState {
    Free,
    Running,
    Exiting { exit_status: i32 },
    Zombie { exit_status: i32, signal: Option<Signal> },
}

struct MProc {
    state: ProcessState,
    pid: Pid,
    parent: Slot,
    // ...
}
```

### 7.2 状态机建模

**当前问题**：状态转换隐含在代码逻辑中，难以验证

**Rust 改进**：显式状态机

```rust
impl Process {
    fn transition(&mut self, event: Event) -> Result<(), StateError> {
        match (&self.state, event) {
            (ProcessState::Running, Event::Exit(status)) => {
                self.state = ProcessState::Exiting { exit_status: status };
                Ok(())
            }
            (ProcessState::Exiting { .. }, Event::VfsReply) => {
                self.state = ProcessState::Zombie { /* ... */ };
                Ok(())
            }
            _ => Err(StateError::InvalidTransition),
        }
    }
}
```

### 7.3 错误处理改进

**当前问题**：错误通过返回值传递，容易被忽略

```c
// C 代码：错误可能被忽略
if((r = sched_stop(...)) != OK) {
    printf("warning...");  // 只打印警告
}
```

**Rust 改进**：使用 Result 类型

```rust
fn sched_stop(scheduler: Endpoint, proc: Endpoint) -> Result<(), SchedError> {
    if scheduler == KERNEL || scheduler == NONE {
        return Ok(());
    }
    
    let msg = SchedulingStopMessage { endpoint: proc };
    taskcall(scheduler, msg).map_err(|e| SchedError::CommunicationError(e))
}
```

### 7.4 当前设计问题总结

| 问题 | 代码位置 | 说明 |
|------|----------|------|
| 状态标志位滥用 | `mp_flags` | 多个标志位组合，语义不清 |
| 隐式状态转换 | `exit_proc()`, `exit_restart()` | 状态转换散落在多个函数 |
| 错误处理不一致 | `sched_stop()` vs `sys_stop()` | 有的 panic，有的只警告 |
| 异步流程难追踪 | fork/exec/exit | 需要跨多个函数理解流程 |
| PRIV_PROC 特殊处理 | 多处 | 系统进程和用户进程逻辑混杂 |

### 7.5 改进方向

#### 7.5.1 统一进程视图

```rust
trait Process {
    fn pid(&self) -> Pid;
    fn state(&self) -> ProcessState;
    fn parent(&self) -> Pid;
}

struct ProcessCoordinator {
    pm: PmClient,
    vm: VmClient,
    vfs: VfsClient,
}

impl Process for ProcessCoordinator {
    fn state(&self) -> ProcessState {
        self.pm.get_state(self.pid)
    }
}
```

#### 7.5.2 显式异步流程

```rust
async fn fork(parent: Pid) -> Result<Pid, ForkError> {
    let slot = pm.alloc_slot()?;
    let child_ep = vm.fork(parent, slot).await?;
    let pid = pm.init_child(slot, child_ep)?;
    vfs.copy_fds(parent, pid).await?;
    Ok(pid)
}
```

#### 7.5.3 消除隐式语义

```rust
// 区分系统进程和用户进程
enum ProcessType {
    User,
    System { managed_by: Endpoint },
}

impl Process {
    fn can_exit(&self) -> bool {
        match self.process_type {
            ProcessType::User => true,
            ProcessType::System { .. } => false,
        }
    }
}
```

---

## 八、总结

### 8.1 核心知识点

1. **进程生命周期**：fork 创建 → exec 执行 → exit 终止 → wait 回收
2. **两阶段退出**：`exit_proc()` 停止进程，`exit_restart()` 清理资源
3. **僵尸进程机制**：保留退出状态，等待父进程收集
4. **异步 IPC 模式**：PM 与 VFS/VM 通过消息异步协作

### 8.2 关键设计决策

| 决策 | 原因 |
|------|------|
| fork 后挂起等待 VFS | VFS 需要复制 fd，不能立即返回 |
| exit 分两阶段 | VFS 需要时间清理文件资源 |
| 系统进程不能调用 exit | 由 RS 管理，确保自动重启 |
| sys_stop + sched_stop | 分别处理内核和调度器资源 |

### 8.3 设计张力点

1. **Unix 语义 vs 微内核架构**：进程概念被拆散到多个服务
2. **同步接口 vs 异步实现**：系统调用看起来同步，实际是异步 IPC
3. **通用逻辑 vs 特殊处理**：PRIV_PROC 检查散落各处
