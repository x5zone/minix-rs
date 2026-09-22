# PM 系统调用接口层模块

> **模块范围**: `servers/pm/getset.c` + `servers/pm/misc.c`
> **核心功能**: 进程身份管理、杂项系统调用
> **所属服务**: PM（Process Manager）

---

## 一、模块整体定位

### 1.1 在系统中的作用

本模块实现了 PM 服务中面向用户空间的系统调用接口：

| 子模块 | 文件 | 核心功能 |
|--------|------|----------|
| **进程身份** | `getset.c` | UID/GID/PID 相关的 get/set 操作 |
| **杂项调用** | `misc.c` | reboot、priority、rusage 等系统调用 |

### 1.2 与其他模块的关系

```
                    ┌─────────────────────────────────────────────────────────┐
                    │                      用户进程                            │
                    │   调用 getpid() / setuid() / reboot() / getpriority()  │
                    └──────────────────────────┬──────────────────────────────┘
                                               │ 系统调用
                                               ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                              PM（Process Manager）                           │
│  ┌─────────────────────────────┐  ┌─────────────────────────────┐          │
│  │       getset.c              │  │        misc.c               │          │
│  │  do_get()                   │  │  do_reboot()                │          │
│  │  do_set()                   │  │  do_getsetpriority()        │          │
│  │                             │  │  do_getsysinfo()            │          │
│  │  - getuid/geteuid           │  │  do_getprocnr()             │          │
│  │  - getgid/getegid           │  │  do_getepinfo()             │          │
│  │  - getpid/getppid           │  │  do_svrctl()                │          │
│  │  - getpgrp/getsid           │  │  do_getrusage()             │          │
│  │  - getgroups                │  │  do_sysuname()              │          │
│  │  - setuid/seteuid           │  └──────────────┬──────────────┘          │
│  │  - setgid/setegid           │                 │                          │
│  │  - setgroups                │                 │                          │
│  │  - setsid                   │                 │                          │
│  └──────────────┬──────────────┘                 │                          │
│                 │                                │                          │
│                 │ tell_vfs()                     │ 内核调用 / IPC           │
│                 ▼                                ▼                          │
└─────────────────┼────────────────────────────────┼──────────────────────────┘
                  │                                │
    ┌─────────────┴─────────────┐    ┌─────────────┴─────────────┐
    │          VFS              │    │         Kernel            │
    │  文件权限检查              │    │  sys_times()              │
    │  进程组管理                │    │  sys_stop()               │
│  VFS_PM_SETUID/GID         │    │  check_sig()              │
    │  VFS_PM_SETSID            │    │  vm_getrusage()           │
    └───────────────────────────┘    └───────────────────────────┘
```

### 1.3 核心设计约束

**POSIX 兼容性**：系统调用接口必须符合 POSIX 标准

**PM-VFS 协作**：设置操作需要同步更新 PM 和 VFS 两边的状态

**权限模型**：基于 Real/Effective/Saved 三组 ID 的权限检查

---

## 二、核心数据结构

### 2.1 进程身份字段

```c
struct mproc {
    // 用户 ID（三组）
    uid_t mp_realuid;    // Real UID - 标识谁启动了进程
    uid_t mp_effuid;     // Effective UID - 用于权限检查
    uid_t mp_svuid;      // Saved UID - 保存之前的 effective UID
    
    // 组 ID（三组）
    gid_t mp_realgid;    // Real GID
    gid_t mp_effgid;     // Effective GID
    gid_t mp_svgid;      // Saved GID
    
    // 补充组
    int mp_ngroups;                    // 补充组数量
    gid_t mp_sgroups[NGROUPS_MAX];     // 补充组数组（最多 16 个）
    
    // 进程标识
    pid_t mp_pid;        // 进程 ID
    int mp_parent;       // 父进程索引
    pid_t mp_procgrp;    // 进程组 ID
    
    // 进程标志
    int mp_flags;        // 包含 TAINTED 等标志
    
    // 资源统计
    clock_t mp_child_utime;  // 子进程用户时间累计
    clock_t mp_child_stime;  // 子进程系统时间累计
    
    // ...
};
```

### 2.2 三组 ID 的作用

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         UNIX 权限模型                                        │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   ┌─────────────────────────────────────────────────────────────────────┐  │
│   │  Real ID (真实 ID)                                                   │  │
│   │  - 标识"谁启动了这个进程"                                             │  │
│   │  - 由 login 程序设置                                                 │  │
│   │  - 通常在进程生命周期内不变                                           │  │
│   └─────────────────────────────────────────────────────────────────────┘  │
│                                                                             │
│   ┌─────────────────────────────────────────────────────────────────────┐  │
│   │  Effective ID (有效 ID)                                              │  │
│   │  - 用于权限检查                                                      │  │
│   │  - 文件访问、系统调用权限                                             │  │
│   │  - 可以通过 setuid/seteuid 改变                                      │  │
│   └─────────────────────────────────────────────────────────────────────┘  │
│                                                                             │
│   ┌─────────────────────────────────────────────────────────────────────┐  │
│   │  Saved ID (保存 ID)                                                  │  │
│   │  - 保存之前的 effective ID                                           │  │
│   │  - 允许在两个 ID 之间切换                                            │  │
│   │  - 用于 setuid 程序的权限切换                                        │  │
│   └─────────────────────────────────────────────────────────────────────┘  │
│                                                                             │
│   典型应用：ping 程序                                                       │
│   ┌─────────────────────────────────────────────────────────────────────┐  │
│   │  1. 初始状态：real=1000, eff=0 (root), saved=0                       │  │
│   │  2. 完成特权操作后：seteuid(1000) → real=1000, eff=1000, saved=0    │  │
│   │  3. 需要特权时：seteuid(0) → real=1000, eff=0, saved=0              │  │
│   └─────────────────────────────────────────────────────────────────────┘  │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 2.3 系统信息结构

```c
struct utsname {
    char sysname[65];   // 操作系统名称 ("Minix")
    char nodename[65];  // 主机名
    char release[65];   // 内核版本 ("3.3.0")
    char version[65];   // 完整版本
    char machine[65];   // 硬件架构 ("i386", "evbarm")
};
```

### 2.4 资源使用结构

```c
struct rusage {
    struct timeval ru_utime;    // 用户态时间
    struct timeval ru_stime;    // 内核态时间
    long ru_maxrss;             // 最大驻留集大小
    long ru_ixrss;              // 共享内存大小
    long ru_idrss;              // 非共享数据大小
    long ru_isrss;              // 非共享栈大小
    long ru_minflt;             // 软页错误数
    long ru_majflt;             // 硬页错误数
    long ru_nswap;              // 交换次数
    long ru_inblock;            // 块输入操作数
    long ru_oublock;            // 块输出操作数
    long ru_msgsnd;             // 发送消息数
    long ru_msgrcv;             // 接收消息数
    long ru_nsignals;           // 信号数
    long ru_nvcsw;              // 自愿上下文切换
    long ru_nivcsw;             // 非自愿上下文切换
};
```

### 2.5 常量定义

```c
#define NGROUPS_MAX    16      // 最大补充组数量
#define PRIO_MIN       -20     // 最低 nice 值（最高优先级）
#define PRIO_MAX       20      // 最高 nice 值（最低优先级）
#define SUPER_USER     0       // 超级用户 UID
```

---

## 三、核心流程

### 3.1 获取类系统调用流程

#### 3.1.1 do_get 统一处理

```c
int do_get(void)
```

**处理的系统调用**：

| 调用号 | 系统调用 | 返回值 |
|--------|----------|--------|
| `PM_GETUID` | getuid() / geteuid() | real UID，euid 通过消息返回 |
| `PM_GETGID` | getgid() / getegid() | real GID，egid 通过消息返回 |
| `PM_GETPID` | getpid() / getppid() | PID，ppid 通过消息返回 |
| `PM_GETPGRP` | getpgrp() | 进程组 ID |
| `PM_GETSID` | getsid() | 会话 ID |
| `PM_GETGROUPS` | getgroups() | 补充组数量，组列表复制到用户空间 |
| `PM_ISSETUGID` | issetugid() | 是否被"污染" |

**流程**：

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                          do_get 流程                                         │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   switch (call_nr) {                                                        │
│                                                                             │
│   case PM_GETUID:                                                           │
│       return real UID;                                                      │
│       reply.euid = effective UID;                                          │
│                                                                             │
│   case PM_GETGID:                                                           │
│       return real GID;                                                      │
│       reply.egid = effective GID;                                          │
│                                                                             │
│   case PM_GETPID:                                                           │
│       return PID;                                                           │
│       reply.parent_pid = parent's PID;                                     │
│                                                                             │
│   case PM_GETPGRP:                                                          │
│       return process group ID;                                              │
│                                                                             │
│   case PM_GETSID:                                                           │
│       if (pid == 0) return current process's session ID;                   │
│       else return target process's session ID;                             │
│                                                                             │
│   case PM_GETGROUPS:                                                        │
│       if (ngroups == 0) return group count;                                │
│       if (buffer too small) return EINVAL;                                 │
│       copy groups to user space;                                           │
│       return group count;                                                  │
│                                                                             │
│   case PM_ISSETUGID:                                                        │
│       return (mp_flags & TAINTED) ? 1 : 0;                                 │
│                                                                             │
│   }                                                                         │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 3.2 设置类系统调用流程

#### 3.2.1 do_set 统一处理

```c
int do_set(void)
```

**处理的系统调用**：

| 调用号 | 系统调用 | 权限要求 |
|--------|----------|----------|
| `PM_SETUID` | setuid() | root 或 uid 等于 real/effective UID |
| `PM_SETEUID` | seteuid() | root 或 uid 等于 real/saved/effective UID |
| `PM_SETGID` | setgid() | root 或 gid 等于 real GID |
| `PM_SETEGID` | setegid() | root 或 gid 等于 real/saved/effective GID |
| `PM_SETGROUPS` | setgroups() | 仅 root |
| `PM_SETSID` | setsid() | 不能是进程组组长 |

**流程**：

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                          do_set 流程                                         │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   1. 权限检查                                                                │
│      - 根据调用类型检查权限                                                  │
│      - 失败返回 EPERM                                                       │
│                                                                             │
│   2. 更新 PM 进程表                                                          │
│      - 修改相应的 ID 字段                                                   │
│                                                                             │
│   3. 构造 VFS 消息                                                           │
│      - 消息类型：VFS_PM_SETUID / SETGID / SETGROUPS / SETSID               │
│      - 包含进程端点和新值                                                   │
│                                                                             │
│   4. 发送消息给 VFS                                                          │
│      tell_vfs(rmp, &m);                                                     │
│                                                                             │
│   5. 挂起等待 VFS 回复                                                       │
│      return SUSPEND;                                                        │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### 3.2.2 setuid vs seteuid 语义

| 操作 | setuid() | seteuid() |
|------|----------|-----------|
| Real UID | 修改 | 不修改 |
| Effective UID | 修改 | 修改 |
| Saved UID | 修改 | 不修改 |
| root 调用 | 设置所有三个 | 只设置 effective |
| 普通用户 | 只能设置为 real/saved | 只能设置为 real/saved/effective |

**BSD 语义**：`setuid()` 总是更新所有三个字段。

#### 3.2.3 PM-VFS 同步流程

```
用户进程          PM              VFS
   |              |               |
   | setuid(0)    |               |
   |------------->|               |
   |              | 权限检查      |
   |              | 更新进程表    |
   |              |               |
   |              | VFS_PM_SETUID |
   |              |-------------->|
   |              |               | 更新文件权限检查
   |              |<--------------|
   |              | 回复          |
   |<-------------|               |
   | 返回结果     |               |
```

### 3.3 杂项系统调用流程

#### 3.3.1 do_reboot 流程

```c
int do_reboot(void)
```

**流程**：

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         do_reboot 流程                                       │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   1. 权限检查                                                                │
│      if (mp_effuid != SUPER_USER) return EPERM;                            │
│                                                                             │
│   2. 保存重启标志                                                            │
│      abort_flag = how;  // RB_HALT / RB_POWERDOWN / RB_AUTOBOOT            │
│                                                                             │
│   3. 通知 readclock（关机时）                                                │
│      if (RB_POWERDOWN)                                                      │
│          notify readclock driver;                                           │
│                                                                             │
│   4. 杀死所有用户进程                                                        │
│      check_sig(-1, SIGKILL, FALSE);  // 除了 init                          │
│                                                                             │
│   5. 停止 init 进程                                                          │
│      sys_stop(INIT_PROC_NR);                                                │
│                                                                             │
│   6. 通知 VFS                                                                │
│      tell_vfs(VFS_PM_REBOOT);                                               │
│                                                                             │
│   7. 挂起（不回复）                                                          │
│      return SUSPEND;                                                        │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### 3.3.2 do_getsetpriority 流程

```c
int do_getsetpriority(void)
```

**流程**：

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                     do_getsetpriority 流程                                   │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   1. 参数验证                                                                │
│      if (which != PRIO_PROCESS) return EINVAL;                             │
│                                                                             │
│   2. 查找目标进程                                                            │
│      if (who == 0) target = current process;                               │
│      else target = find_proc(who);                                         │
│                                                                             │
│   3. 权限检查                                                                │
│      if (!root && !owns_process) return EPERM;                             │
│                                                                             │
│   4. GET 或 SET                                                              │
│      if (GET) return nice - PRIO_MIN;                                      │
│      if (SET) {                                                             │
│          if (!root && lowering_nice) return EACCES;                        │
│          sched_nice(rmp, nice);                                            │
│          mp_nice = nice;                                                   │
│          return OK;                                                        │
│      }                                                                      │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

**nice 值与优先级的关系**：

| nice 值 | 优先级 | 说明 |
|---------|--------|------|
| -20 | 最高 | 仅 root 可设置 |
| 0 | 默认 | 普通进程默认值 |
| 19 | 最低 | 最低调度优先级 |

**权限规则**：
- 普通用户只能提高 nice 值（降低优先级）
- root 可以任意调整 nice 值

#### 3.3.3 do_getrusage 流程

```c
int do_getrusage(void)
```

**参数**：
- `RUSAGE_SELF`: 获取当前进程的资源使用
- `RUSAGE_CHILDREN`: 获取子进程的资源使用累计

**流程**：

```
1. 参数验证
   if (who != RUSAGE_SELF && who != RUSAGE_CHILDREN)
       return EINVAL;

2. 获取时间
   if (RUSAGE_SELF)
       sys_times(who_e, &user_time, &sys_time, NULL, NULL);
   else
       user_time = mp_child_utime;
       sys_time = mp_child_stime;

3. 从 VM 获取内存统计
   vm_getrusage(who_e, &r_usage, children);

4. 复制到用户空间
   sys_datacopy(SELF, &r_usage, who_e, addr, sizeof(r_usage));
```

#### 3.3.4 do_getsysinfo 流程

```c
int do_getsysinfo(void)
```

**权限**：仅超级用户可调用

**支持的请求**：

| 请求 | 返回数据 |
|------|----------|
| `SI_PROC_TAB` | 整个 PM 进程表 |
| `SI_CALL_STATS` | 系统调用统计（可选） |

**用途**：系统诊断工具（如 `ps`、`top`）获取进程信息。

#### 3.3.5 do_getprocnr / do_getepinfo

**do_getprocnr**：通过 PID 查找进程端点
- 仅 RS（重启服务）可调用
- 用于服务监控和管理

**do_getepinfo**：通过端点获取进程身份信息
- 返回 UID、GID、补充组列表
- 用于服务间通信的身份验证

---

## 四、关键机制拆解

### 4.1 权限检查机制

**setuid 权限检查**：

```c
// setuid()
if (rmp->mp_realuid != uid && rmp->mp_effuid != SUPER_USER)
    return EPERM;

// seteuid()
if (rmp->mp_realuid != uid && rmp->mp_svuid != uid &&
    rmp->mp_effuid != SUPER_USER)
    return EPERM;
```

**优先级权限检查**：

```c
// 检查进程所有权
if (mp->mp_effuid != SUPER_USER &&
    mp->mp_effuid != rmp->mp_effuid &&
    mp->mp_effuid != rmp->mp_realuid)
    return EPERM;

// 检查 nice 值调整方向
if (rmp->mp_nice > arg_pri && mp->mp_effuid != SUPER_USER)
    return EACCES;  // 普通用户不能提高优先级
```

### 4.2 PM-VFS 同步机制

**设置操作必须同步**：

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        PM-VFS 同步机制                                       │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   为什么需要同步？                                                           │
│   - PM 管理进程表中的 ID 字段                                               │
│   - VFS 管理文件访问权限检查                                                │
│   - 两边必须保持一致                                                        │
│                                                                             │
│   同步流程：                                                                 │
│   1. PM 先更新本地进程表                                                    │
│   2. PM 发送消息给 VFS                                                      │
│   3. PM 返回 SUSPEND，挂起用户进程                                          │
│   4. VFS 更新自己的状态                                                     │
│   5. VFS 回复 PM                                                            │
│   6. PM 回复用户进程                                                        │
│                                                                             │
│   消息类型：                                                                 │
│   - VFS_PM_SETUID: 设置用户 ID                                             │
│   - VFS_PM_SETGID: 设置组 ID                                               │
│   - VFS_PM_SETGROUPS: 设置补充组                                           │
│   - VFS_PM_SETSID: 创建新会话                                              │
│   - VFS_PM_REBOOT: 系统重启                                                │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 4.3 TAINTED 标志

**设置时机**：
- 进程通过 setuid/setgid 程序启动时
- exec 带有 setuid/setgid 位的程序时

**用途**：
- `issetugid()` 系统调用返回此标志
- 安全敏感程序检查是否需要额外验证

**代码**：

```c
case PM_ISSETUGID:
    r = !!(rmp->mp_flags & TAINTED);
    break;
```

### 4.4 会话与进程组

**setsid() 规则**：

```c
// 不能是进程组组长
if (rmp->mp_procgrp == rmp->mp_pid)
    return EPERM;

// 创建新会话
rmp->mp_procgrp = rmp->mp_pid;
```

**会话创建效果**：
1. 进程成为新会话的会话组长
2. 进程成为新进程组的组长
3. 进程失去控制终端

---

## 五、边界条件与特殊分支

### 5.1 setuid(0) 特殊处理

```c
// NetBSD 语义：setuid(geteuid()) 可能失败
if (rmp->mp_realuid != uid && rmp->mp_effuid != SUPER_USER)
    return EPERM;
```

**原因**：防止权限提升攻击。

### 5.2 getgroups(0, NULL)

```c
if (ngroups == 0) {
    r = rmp->mp_ngroups;  // 只返回数量
    break;
}
```

**POSIX 语义**：允许用户查询需要的缓冲区大小。

### 5.3 setsid 失败条件

```c
if (rmp->mp_procgrp == rmp->mp_pid)
    return EPERM;  // 已经是进程组组长
```

### 5.4 reboot 权限与顺序

```c
// 顺序很重要！
check_sig(-1, SIGKILL, FALSE);  // 先杀死所有进程
sys_stop(INIT_PROC_NR);          // 停止 init
tell_vfs(VFS_PM_REBOOT);         // 再通知 VFS
```

**原因**：VFS 重启时会退出所有进程，如果先通知 VFS，SIGKILL 会造成混乱。

### 5.5 优先级调整限制

```c
// 普通用户不能提高优先级（降低 nice 值）
if (rmp->mp_nice > arg_pri && mp->mp_effuid != SUPER_USER)
    return EACCES;
```

### 5.6 getsid 参数处理

```c
pid_t p = m_in.m_lc_pm_getsid.pid;
target = p ? find_proc(p) : &mproc[who_p];
```

**语义**：`getsid(0)` 返回当前进程的会话 ID。

---

## 六、模块交互关系

### 6.1 上游调用者

| 调用者 | 方式 | 说明 |
|--------|------|------|
| 用户进程 | 系统调用 | getpid, setuid, reboot 等 |
| RS | 系统调用 | do_getprocnr, do_getepinfo |
| 系统服务 | 系统调用 | do_getsysinfo |

### 6.2 下游依赖

| 服务 | 调用函数 | 说明 |
|------|----------|------|
| VFS | tell_vfs() | 同步进程身份变更 |
| Kernel | sys_times() | 获取进程时间 |
| Kernel | sys_stop() | 停止进程 |
| VM | vm_getrusage() | 获取内存统计 |
| SCHED | sched_nice() | 调整优先级 |

### 6.3 IPC 消息类型

| 消息类型 | 方向 | 说明 |
|----------|------|------|
| `VFS_PM_SETUID` | PM → VFS | 设置用户 ID |
| `VFS_PM_SETGID` | PM → VFS | 设置组 ID |
| `VFS_PM_SETGROUPS` | PM → VFS | 设置补充组 |
| `VFS_PM_SETSID` | PM → VFS | 创建新会话 |
| `VFS_PM_REBOOT` | PM → VFS | 系统重启 |

---

## 七、Rust 重构与设计改进建议

### 7.1 类型系统改进

**当前问题**：UID/GID 使用整数类型

```c
uid_t mp_realuid;  // 实际上是 unsigned int
gid_t mp_realgid;  // 实际上是 unsigned int
```

**Rust 改进**：使用强类型

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Uid(u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Gid(u32);

impl Uid {
    const ROOT: Uid = Uid(0);
    
    fn is_root(&self) -> bool {
        self.0 == 0
    }
}

struct ProcessIdentity {
    real: Uid,
    effective: Uid,
    saved: Uid,
}
```

### 7.2 进程身份建模

**当前问题**：三组 ID 分散存储

```c
uid_t mp_realuid, mp_effuid, mp_svuid;
gid_t mp_realgid, mp_effgid, mp_svgid;
```

**Rust 改进**：使用结构体封装

```rust
struct Identity<T> {
    real: T,
    effective: T,
    saved: T,
}

impl<T: Clone + PartialEq> Identity<T> {
    fn set(&mut self, value: T) {
        self.real = value.clone();
        self.effective = value.clone();
        self.saved = value;
    }
    
    fn set_effective(&mut self, value: T) {
        self.effective = value;
    }
    
    fn can_set(&self, value: &T, is_root: bool) -> bool {
        is_root || &self.real == value || &self.saved == value
    }
}

struct ProcessIds {
    uid: Identity<Uid>,
    gid: Identity<Gid>,
    groups: Vec<Gid>,
}
```

### 7.3 系统调用分发

**当前问题**：switch-case 分发

```c
switch (call_nr) {
    case PM_GETUID: ...
    case PM_GETGID: ...
    // ...
}
```

**Rust 改进**：使用 trait 或枚举

```rust
enum GetCall {
    Uid,
    Gid,
    Pid,
    Pgrp,
    Sid(Option<Pid>),
    Groups(usize),
    IsSetugid,
}

enum SetCall {
    Uid(Uid),
    Euid(Uid),
    Gid(Gid),
    Egid(Gid),
    Groups(Vec<Gid>),
    Sid,
}

impl ProcessManager {
    fn handle_get(&mut self, call: GetCall) -> Result<GetResult, Error> {
        match call {
            GetCall::Uid => self.get_uid(),
            GetCall::Gid => self.get_gid(),
            // ...
        }
    }
}
```

### 7.4 权限检查抽象

**当前问题**：权限检查分散

```c
if (rmp->mp_realuid != uid && rmp->mp_effuid != SUPER_USER)
    return EPERM;
```

**Rust 改进**：使用 trait 抽象

```rust
trait PermissionCheck {
    fn can_set_uid(&self, uid: Uid) -> bool;
    fn can_set_gid(&self, gid: Gid) -> bool;
    fn can_set_priority(&self, process: &Process, new_nice: i32) -> bool;
}

impl PermissionCheck for Process {
    fn can_set_uid(&self, uid: Uid) -> bool {
        self.uid.effective.is_root() || 
        self.uid.real == uid
    }
    
    fn can_set_priority(&self, process: &Process, new_nice: i32) -> bool {
        // root 可以任意调整
        // 普通用户只能降低优先级
        self.uid.effective.is_root() || 
        new_nice >= process.nice
    }
}
```

### 7.5 当前设计问题总结

| 问题 | 代码位置 | 说明 |
|------|----------|------|
| 类型不安全 | UID/GID | 使用整数，容易混淆 |
| ID 分散存储 | mproc | 三组 ID 没有封装 |
| 权限检查分散 | 各函数 | 重复代码多 |
| switch-case 分发 | do_get/do_set | 不利于扩展 |
| PM-VFS 同步复杂 | do_set | 需要手动管理 SUSPEND |

### 7.6 改进方向

#### 7.6.1 身份管理抽象

```rust
struct IdentityManager {
    uid: Identity<Uid>,
    gid: Identity<Gid>,
    supplementary_groups: Vec<Gid>,
}

impl IdentityManager {
    fn setuid(&mut self, uid: Uid, is_root: bool) -> Result<(), Error> {
        if !self.can_set_uid(uid, is_root) {
            return Err(Error::PermissionDenied);
        }
        self.uid.set(uid);
        Ok(())
    }
}
```

#### 7.6.2 异步 VFS 同步

```rust
async fn setuid(&mut self, uid: Uid) -> Result<(), Error> {
    // 1. 更新本地状态
    self.identity.uid.set(uid);
    
    // 2. 异步通知 VFS
    vfs::set_uid(self.endpoint, uid).await?;
    
    Ok(())
}
```

#### 7.6.3 系统调用统计

```rust
struct SyscallStats {
    counts: HashMap<SyscallNumber, AtomicU64>,
}

impl SyscallStats {
    fn record(&self, syscall: SyscallNumber) {
        self.counts.get(&syscall)
            .map(|c| c.fetch_add(1, Ordering::Relaxed));
    }
}
```

---

## 八、总结

### 8.1 核心知识点

1. **三组 ID 模型**：Real/Effective/Saved ID 实现灵活的权限管理
2. **PM-VFS 同步**：设置操作需要同步更新两边状态
3. **权限检查规则**：root 拥有完全权限，普通用户受限
4. **系统调用分发**：通过 call_nr 区分具体操作

### 8.2 关键设计决策

| 决策 | 原因 |
|------|------|
| BSD 语义 setuid | 简化权限管理，避免复杂切换 |
| SUSPEND 返回值 | 等待 VFS 完成后再回复用户 |
| TAINTED 标志 | 安全敏感程序需要知道进程状态 |
| nice 值限制 | 防止普通用户独占 CPU |

### 8.3 设计张力点

1. **POSIX vs BSD 语义**：不同系统有细微差异
2. **PM-VFS 一致性**：需要保证两边状态同步
3. **权限检查复杂性**：多种条件组合，容易出错
4. **同步等待开销**：设置操作需要等待 VFS 回复
