# PM 调试与性能分析模块

> **模块范围**: `servers/pm/trace.c` + `servers/pm/profile.c`
> **核心功能**: ptrace 调试支持、统计性性能分析
> **所属服务**: PM（Process Manager）

---

## 一、模块整体定位

### 1.1 在系统中的作用

本模块实现了 PM 服务中面向开发者的调试与分析接口：

| 子模块 | 文件 | 核心功能 |
|--------|------|----------|
| **进程调试** | `trace.c` | ptrace 系统调用实现，支持 GDB 等调试器 |
| **性能分析** | `profile.c` | 统计性性能分析接口，用于热点定位 |

### 1.2 与其他模块的关系

```
                    ┌─────────────────────────────────────────────────────────┐
                    │                      调试器进程                          │
                    │   GDB / strace / ltrace / 自定义调试工具                 │
                    └──────────────────────────┬──────────────────────────────┘
                                               │ ptrace() / sprofile()
                                               ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                              PM（Process Manager）                           │
│  ┌─────────────────────────────┐  ┌─────────────────────────────┐          │
│  │       trace.c               │  │        profile.c            │          │
│  │  do_trace()                 │  │  do_sprofile()              │          │
│  │                             │  │                             │          │
│  │  - T_OK (PT_TRACE_ME)       │  │  - PROF_START               │          │
│  │  - T_ATTACH                 │  │  - PROF_STOP                │          │
│  │  - T_DETACH                 │  │                             │          │
│  │  - T_EXIT                   │  │  条件编译: SPROFILE          │          │
│  │  - T_SETOPT                 │  │                             │          │
│  │  - T_GETRANGE/SETRANGE      │  └──────────────┬──────────────┘          │
│  │  - T_RESUME/STEP/SYSCALL    │                 │                          │
│  └──────────────┬──────────────┘                 │                          │
│                 │ sys_trace()                    │ sys_sprof()              │
│                 ▼                                ▼                          │
└─────────────────┼────────────────────────────────┼──────────────────────────┘
                  │                                │
    ┌─────────────┴─────────────┐    ┌─────────────┴─────────────┐
    │       Kernel              │    │         Kernel            │
    │  sys_trace()              │    │  sys_sprof()              │
    │  - 内存读写               │    │  - 定时器采样              │
    │  - 寄存器访问             │    │  - PC 计数器记录           │
    │  - 单步执行               │    │  - 缓冲区管理              │
    │  - 断点设置               │    │                           │
    └───────────────────────────┘    └───────────────────────────┘
```

### 1.3 核心设计约束

**分层设计**：PM 处理权限检查和状态管理，内核处理底层操作

**安全隔离**：严格的权限检查防止未授权调试

**条件编译**：性能分析功能可选启用

---

## 二、核心数据结构

### 2.1 进程跟踪字段

```c
struct mproc {
    // 跟踪关系
    int mp_tracer;              // 跟踪者进程索引，NO_TRACER 表示未被跟踪
    unsigned mp_trace_flags;    // 跟踪选项
    
    // 信号跟踪
    sigset_t mp_sigtrace;       // 需要先交给跟踪者的信号
    
    // 进程标志
    unsigned mp_flags;          // 包含 TRACE_STOPPED, TRACE_EXIT 等
};
```

### 2.2 跟踪标志位

```c
// 进程状态标志（mp_flags 的位）
#define TRACE_STOPPED   0x00080  // 进程因跟踪而停止
#define TRACE_EXIT      0x08000  // 跟踪者强制进程退出
#define TRACE_ZOMBIE    0x10000  // 等待跟踪者 wait()
```

### 2.3 跟踪选项

```c
// mp_trace_flags 的位（来自 sys/ptrace.h）
#define TO_TRACEFORK    0x1      // 自动附加到 fork 的子进程
#define TO_ALTEXEC      0x2      // exec 成功时发送 SIGSTOP
#define TO_NOEXEC       0x4      // exec 成功时不发送信号
```

**选项组合规则**：

| 选项组合 | exec 时的行为 |
|----------|---------------|
| `TO_NOEXEC` | 不发送信号 |
| `TO_ALTEXEC` | 发送 `SIGSTOP` |
| 默认（无选项） | 发送 `SIGTRAP` |

### 2.4 ptrace 请求码

```c
// POSIX 标准请求（跨平台兼容）
#define T_OK           PT_TRACE_ME   // 允许父进程跟踪
#define T_GETINS       PT_READ_I     // 读取指令空间
#define T_GETDATA      PT_READ_D     // 读取数据空间
#define T_SETINS       PT_WRITE_I    // 写入指令空间
#define T_SETDATA      PT_WRITE_D    // 写入数据空间
#define T_RESUME       PT_CONTINUE   // 继续执行
#define T_EXIT         PT_KILL       // 终止进程
#define T_SYSCALL      PT_SYSCALL    // 跟踪系统调用
#define T_ATTACH       PT_ATTACH     // 附加到运行中的进程
#define T_DETACH       PT_DETACH     // 分离被跟踪进程

// Minix 特有请求
#define T_STOP         -1            // 停止进程（不暴露给用户）
#define T_READB_INS    100           // 读取代码段字节（仅 root）
#define T_WRITEB_INS   101           // 修改代码段字节（仅 root）
#define T_GETUSER      102           // 读取用户进程表
#define T_SETUSER      103           // 写入用户进程表
#define T_STEP         104           // 单步执行
#define T_SETOPT       105           // 设置跟踪选项
#define T_GETRANGE     106           // 批量读取
#define T_SETRANGE     107           // 批量写入
```

### 2.5 批量传输结构

```c
struct ptrace_range {
    int pr_space;        // TS_INS（代码段）或 TS_DATA（数据段）
    long pr_addr;        // 被跟踪进程中的地址
    void *pr_ptr;        // 调用者进程中的缓冲区
    size_t pr_size;      // 传输字节数
};
```

### 2.6 性能分析参数

```c
// sprofile 系统调用参数（来自消息）
m_in.m_lc_pm_sprof.action      // PROF_START 或 PROF_STOP
m_in.m_lc_pm_sprof.mem_size    // 采样缓冲区大小
m_in.m_lc_pm_sprof.freq        // 采样频率（Hz）
m_in.m_lc_pm_sprof.intr_type   // 中断类型
m_in.m_lc_pm_sprof.ctl_ptr     // 控制结构指针
m_in.m_lc_pm_sprof.mem_ptr     // 采样缓冲区指针
```

---

## 三、核心流程

### 3.1 ptrace 调试流程模型

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        ptrace 调试状态机                                     │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   ┌─────────────────────────────────────────────────────────────────────┐  │
│   │                        进程生命周期视角                               │  │
│   └─────────────────────────────────────────────────────────────────────┘  │
│                                                                             │
│   ┌──────────┐    T_OK        ┌──────────┐    exec        ┌──────────┐    │
│   │  普通    │ ──────────────>│ 可被跟踪 │ ──────────────>│ 被跟踪   │    │
│   │  进程    │                │  状态    │                │  运行    │    │
│   └──────────┘                └──────────┘                └────┬─────┘    │
│        ▲                                                       │          │
│        │ T_DETACH                                              │          │
│        │                                                       ▼          │
│   ┌────┴─────┐    信号/断点   ┌──────────┐    T_RESUME   ┌──────────┐    │
│   │  分离    │ <─────────────│  停止    │ <─────────────│  继续    │    │
│   │  状态    │                │  状态    │                │  执行    │    │
│   └──────────┘                └────┬─────┘                └──────────┘    │
│                                    │                                       │
│                                    │ T_EXIT                                │
│                                    ▼                                       │
│                               ┌──────────┐                                 │
│                               │  退出    │                                 │
│                               │  状态    │                                 │
│                               └──────────┘                                 │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 3.2 T_OK（PT_TRACE_ME）流程

**用途**：子进程在 exec 前声明允许父进程跟踪

```
调试器父进程                     被调试子进程
     │                               │
     │ fork()                        │
     │──────────────────────────────>│
     │                               │
     │                               │ ptrace(T_OK)
     │                               │───┐
     │                               │   │ 检查 mp_tracer == NO_TRACER
     │                               │   │ 设置 mp_tracer = mp_parent
     │                               │<──┘
     │                               │
     │                               │ exec(程序)
     │                               │───┐
     │                               │   │ 检查 mp_tracer != NO_TRACER
     │                               │   │ 发送 SIGTRAP（除非 TO_NOEXEC）
     │                               │<──┘
     │                               │
     │ wait()                        │
     │<──────────────────────────────│ 子进程停止
     │                               │
     │ ptrace(T_STEP/...)            │
     │──────────────────────────────>│
```

**代码实现**：

```c
case T_OK:
    if (mp->mp_tracer != NO_TRACER) return(EBUSY);
    mp->mp_tracer = mp->mp_parent;
    mp->mp_reply.m_pm_lc_ptrace.data = 0;
    return(OK);
```

### 3.3 T_ATTACH 流程

**用途**：调试器附加到运行中的进程

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         T_ATTACH 流程                                        │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   1. 查找目标进程                                                            │
│      child = find_proc(pid)                                                │
│      if (child == NULL || child->mp_flags & EXITING) return ESRCH         │
│                                                                             │
│   2. 权限检查                                                                │
│      ┌─────────────────────────────────────────────────────────────────┐   │
│      │  非 root 用户必须满足：                                          │   │
│      │  - 调试器 effuid == 目标 effuid                                  │   │
│      │  - 调试器 effgid == 目标 effgid                                  │   │
│      │  - 目标 effuid == 目标 realuid（不能调试 setuid 程序）           │   │
│      │  - 目标 effgid == 目标 realgid                                   │   │
│      └─────────────────────────────────────────────────────────────────┘   │
│                                                                             │
│   3. 保护检查                                                                │
│      - 不能跟踪系统服务器（PRIV_PROC）                                       │
│      - 系统服务器不能跟踪任何人                                              │
│      - 不能跟踪自己、PM、VM                                                  │
│      - 不能跟踪已被跟踪的进程                                                │
│                                                                             │
│   4. 建立跟踪关系                                                            │
│      child->mp_tracer = who_p                                              │
│      child->mp_trace_flags = TO_NOEXEC                                     │
│                                                                             │
│   5. 停止目标进程                                                            │
│      sig_proc(child, SIGSTOP, TRUE /*trace*/, FALSE /*ksig*/)              │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 3.4 T_DETACH 流程

**用途**：调试器分离被跟踪进程

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         T_DETACH 流程                                        │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   1. 参数验证                                                                │
│      if (signal < 0 || signal >= _NSIG) return EINVAL                      │
│                                                                             │
│   2. 清除跟踪关系                                                            │
│      child->mp_tracer = NO_TRACER                                          │
│                                                                             │
│   3. 发送被拦截的信号                                                        │
│      for (i = 1; i < _NSIG; i++) {                                         │
│          if (sigismember(&child->mp_sigtrace, i)) {                        │
│              sigdelset(&child->mp_sigtrace, i);                            │
│              check_sig(child->mp_pid, i, FALSE);                           │
│          }                                                                  │
│      }                                                                      │
│                                                                             │
│   4. 发送指定的信号（如果有）                                                 │
│      if (signal > 0) sig_proc(child, signal, TRUE, FALSE)                  │
│                                                                             │
│   5. 恢复进程执行                                                            │
│      child->mp_flags &= ~TRACE_STOPPED                                     │
│      child->mp_trace_flags = 0                                             │
│      check_pending(child)                                                  │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 3.5 T_RESUME/T_STEP/T_SYSCALL 流程

**用途**：恢复被跟踪进程的执行

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    恢复执行流程                                              │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   1. 参数验证                                                                │
│      if (signal < 0 || signal >= _NSIG) return EINVAL                      │
│                                                                             │
│   2. 发送指定信号（如果有）                                                   │
│      if (signal > 0) sig_proc(child, signal, FALSE, FALSE)                 │
│                                                                             │
│   3. 检查待处理信号                                                          │
│      for (i = 1; i < _NSIG; i++) {                                         │
│          if (sigismember(&child->mp_sigtrace, i)) {                        │
│              // 有待处理信号，假装恢复成功                                    │
│              return OK;                                                    │
│          }                                                                  │
│      }                                                                      │
│                                                                             │
│   4. 清除停止标志                                                            │
│      child->mp_flags &= ~TRACE_STOPPED                                     │
│                                                                             │
│   5. 检查待处理信号                                                          │
│      check_pending(child)                                                  │
│                                                                             │
│   6. 调用内核执行实际操作                                                    │
│      sys_trace(req, child->mp_endpoint, addr, &data)                       │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 3.6 T_EXIT 流程

**用途**：跟踪者强制终止被跟踪进程

```c
case T_EXIT:
    child->mp_flags |= TRACE_EXIT;
    
    // 如果进程有 VFS 或 EVENT 调用待处理，延迟退出
    if (child->mp_flags & (VFS_CALL | EVENT_CALL))
        child->mp_exitstatus = m_in.m_lc_pm_ptrace.data;
    else
        exit_proc(child, m_in.m_lc_pm_ptrace.data, FALSE);
    
    // 等待 VFS 处理完成
    return SUSPEND;
```

### 3.7 trace_stop 流程

**用途**：被跟踪进程收到信号时停止

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        trace_stop 流程                                       │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   触发条件：被跟踪进程收到信号                                                │
│                                                                             │
│   1. 调用内核停止进程                                                        │
│      sys_trace(T_STOP, rmp->mp_endpoint, 0L, NULL)                         │
│                                                                             │
│   2. 设置停止标志                                                            │
│      rmp->mp_flags |= TRACE_STOPPED                                        │
│                                                                             │
│   3. 检查跟踪者是否在等待                                                    │
│      if (wait_test(rpmp, rmp)) {                                           │
│          sigdelset(&rmp->mp_sigtrace, signo);                              │
│          rpmp->mp_flags &= ~WAITING;                                       │
│          rpmp->mp_reply.m_pm_lc_wait4.status = W_STOPCODE(signo);          │
│          reply(rmp->mp_tracer, rmp->mp_pid);                               │
│      }                                                                      │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 3.8 性能分析流程

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        性能分析流程                                           │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   PROF_START:                                                               │
│   ┌─────────────────────────────────────────────────────────────────────┐  │
│   │  用户进程                                                            │  │
│   │  sprofile(PROF_START, mem_size, freq, intr_type, ctl_ptr, mem_ptr) │  │
│   └──────────────────────────────────┬──────────────────────────────────┘  │
│                                      │                                      │
│                                      ▼                                      │
│   ┌─────────────────────────────────────────────────────────────────────┐  │
│   │  PM (do_sprofile)                                                   │  │
│   │  sys_sprof(PROF_START, mem_size, freq, intr_type, who_e,           │  │
│   │            ctl_ptr, mem_ptr)                                        │  │
│   └──────────────────────────────────┬──────────────────────────────────┘  │
│                                      │                                      │
│                                      ▼                                      │
│   ┌─────────────────────────────────────────────────────────────────────┐  │
│   │  内核 (sys_sprof)                                                   │  │
│   │  - 分配采样缓冲区                                                   │  │
│   │  - 设置定时器/性能计数器                                            │  │
│   │  - 开始采样 PC 寄存器                                               │  │
│   └─────────────────────────────────────────────────────────────────────┘  │
│                                                                             │
│   PROF_STOP:                                                                │
│   ┌─────────────────────────────────────────────────────────────────────┐  │
│   │  用户进程                                                            │  │
│   │  sprofile(PROF_STOP)                                                │  │
│   └──────────────────────────────────┬──────────────────────────────────┘  │
│                                      │                                      │
│                                      ▼                                      │
│   ┌─────────────────────────────────────────────────────────────────────┐  │
│   │  PM (do_sprofile)                                                   │  │
│   │  sys_sprof(PROF_STOP, 0, 0, 0, 0, 0, 0)                            │  │
│   └──────────────────────────────────┬──────────────────────────────────┘  │
│                                      │                                      │
│                                      ▼                                      │
│   ┌─────────────────────────────────────────────────────────────────────┐  │
│   │  内核 (sys_sprof)                                                   │  │
│   │  - 停止采样                                                         │  │
│   │  - 处理采样数据                                                     │  │
│   │  - 返回结果                                                         │  │
│   └─────────────────────────────────────────────────────────────────────┘  │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 四、关键机制拆解

### 4.1 权限检查机制

**T_ATTACH 权限规则**：

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        T_ATTACH 权限矩阵                                     │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   调试者          目标进程              允许？    原因                       │
│   ────────────────────────────────────────────────────────────────────────  │
│   root            任何进程              ✓        超级用户权限               │
│   普通用户        自己的进程            ✓        UID/GID 匹配              │
│   普通用户        其他用户的进程        ✗        权限不足                   │
│   普通用户        setuid 程序           ✗        防止权限提升               │
│   普通用户        系统服务器            ✗        保护系统服务               │
│   系统服务器      任何进程              ✗        防止权限滥用               │
│   任何进程        自己                  ✗        防止死锁                   │
│   任何进程        PM/VM                 ✗        保护核心服务               │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

**代码实现**：

```c
// 非 root 用户的权限检查
if (mp->mp_effuid != SUPER_USER &&
    (mp->mp_effuid != child->mp_effuid ||
     mp->mp_effgid != child->mp_effgid ||
     child->mp_effuid != child->mp_realuid ||
     child->mp_effgid != child->mp_realgid))
    return(EPERM);

// 只有 root 可以调试系统服务器
if (mp->mp_effuid != SUPER_USER && (child->mp_flags & PRIV_PROC))
    return(EPERM);

// 系统服务器不能调试任何人
if (mp->mp_flags & PRIV_PROC) return(EPERM);

// 保护关键进程
if (child == mp || child->mp_endpoint == PM_PROC_NR ||
    child->mp_endpoint == VM_PROC_NR)
    return(EPERM);
```

### 4.2 跟踪状态管理

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        跟踪状态转换                                          │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   mp_tracer 字段：                                                          │
│   ┌─────────────────────────────────────────────────────────────────────┐  │
│   │  NO_TRACER (-1)  : 未被跟踪                                         │  │
│   │  进程索引 (>= 0) : 跟踪者的 mproc 数组索引                           │  │
│   └─────────────────────────────────────────────────────────────────────┘  │
│                                                                             │
│   状态转换：                                                                 │
│                                                                             │
│   NO_TRACER ──T_OK/T_ATTACH──> 跟踪者索引                                   │
│       ↑                           │                                        │
│       │                           │ T_DETACH/进程退出                       │
│       └───────────────────────────┘                                        │
│                                                                             │
│   TRACE_STOPPED 标志：                                                      │
│   ┌─────────────────────────────────────────────────────────────────────┐  │
│   │  设置时机：                                                          │  │
│   │  - trace_stop() 被调用时（收到信号）                                 │  │
│   │  - exec 后被停止时                                                   │  │
│   │                                                                      │  │
│   │  清除时机：                                                          │  │
│   │  - T_RESUME/T_STEP/T_SYSCALL                                        │  │
│   │  - T_DETACH                                                         │  │
│   └─────────────────────────────────────────────────────────────────────┘  │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 4.3 信号拦截机制

**mp_sigtrace 位图**：存储需要先交给跟踪者的信号

```
被跟踪进程收到信号
        │
        ▼
┌───────────────────────┐
│ 信号在 mp_sigtrace 中？│
└───────────┬───────────┘
            │
    ┌───────┴───────┐
    │               │
    ▼               ▼
   是              否
    │               │
    ▼               ▼
添加到          正常处理
mp_sigtrace     （可能终止进程）
等待跟踪者
处理
```

**分离时发送被拦截信号**：

```c
// T_DETACH 时
for (i = 1; i < _NSIG; i++) {
    if (sigismember(&child->mp_sigtrace, i)) {
        sigdelset(&child->mp_sigtrace, i);
        check_sig(child->mp_pid, i, FALSE);
    }
}
```

### 4.4 fork 时的跟踪继承

```c
// forkexit.c 中的处理
if (!(rmc->mp_trace_flags & TO_TRACEFORK)) {
    // 默认：子进程不被跟踪
    rmc->mp_tracer = NO_TRACER;
    rmc->mp_trace_flags = 0;
}
// 如果设置了 TO_TRACEFORK，子进程继承跟踪关系
```

### 4.5 exec 时的信号发送

```c
// exec.c 中的处理
if (rmp->mp_tracer != NO_TRACER && !(rmp->mp_trace_flags & TO_NOEXEC)) {
    sn = (rmp->mp_trace_flags & TO_ALTEXEC) ? SIGSTOP : SIGTRAP;
    // 发送信号停止进程
}
```

---

## 五、边界条件与特殊分支

### 5.1 T_READB_INS/T_WRITEB_INS 特殊处理

```c
case T_READB_INS:  // 读取代码段字节
case T_WRITEB_INS: // 修改代码段字节
    if (mp->mp_effuid != SUPER_USER) return(EPERM);
    // 仅 root 可用，不需要进程处于跟踪状态
```

**用途**：动态补丁、代码注入（仅限 root）

### 5.2 延迟退出处理

```c
case T_EXIT:
    child->mp_flags |= TRACE_EXIT;
    
    // 如果进程有 VFS 或 EVENT 调用待处理
    if (child->mp_flags & (VFS_CALL | EVENT_CALL))
        child->mp_exitstatus = m_in.m_lc_pm_ptrace.data; // 保存退出状态
    else
        exit_proc(child, m_in.m_lc_pm_ptrace.data, FALSE);
    
    return SUSPEND; // 等待 VFS 完成
```

**原因**：防止在 VFS 调用期间退出导致状态不一致

### 5.3 待处理信号的假恢复

```c
// T_RESUME/T_STEP/T_SYSCALL
for (i = 1; i < _NSIG; i++) {
    if (sigismember(&child->mp_sigtrace, i)) {
        mp->mp_reply.m_pm_lc_ptrace.data = 0;
        return(OK); // 假装恢复成功，实际等待信号处理
    }
}
```

**原因**：有待处理信号时，进程会再次停止，不需要真正恢复

### 5.4 性能分析的条件编译

```c
int do_sprofile(void)
{
#if SPROFILE
    // 实现代码
#else
    return ENOSYS; // 系统调用未实现
#endif
}
```

**SPROFILE 默认值**：0（关闭）

### 5.5 T_STOP 不暴露给用户

```c
case T_STOP:
    // 这个调用不暴露给用户程序
    // 使用 kill(pid, SIGSTOP) 替代
    return(EINVAL);
```

---

## 六、模块交互关系

### 6.1 上游调用者

| 调用者 | 方式 | 说明 |
|--------|------|------|
| GDB | ptrace | 调试用户程序 |
| strace | ptrace | 跟踪系统调用 |
| 自定义工具 | ptrace | 进程监控、动态分析 |
| 性能分析工具 | sprofile | 热点定位 |

### 6.2 下游依赖

| 服务 | 调用函数 | 说明 |
|------|----------|------|
| Kernel | sys_trace() | 底层内存/寄存器访问 |
| Kernel | sys_sprof() | 性能分析采样 |
| Signal | sig_proc() | 发送信号 |
| Signal | check_sig() | 检查信号处理 |
| ForkExit | exit_proc() | 进程退出 |
| Wait | wait_test() | 检查等待状态 |

### 6.3 与其他 PM 模块的交互

| 模块 | 交互点 | 说明 |
|------|--------|------|
| forkexit.c | fork/exec/exit | 跟踪关系继承和清理 |
| signal.c | 信号处理 | 被跟踪进程的信号拦截 |
| wait.c | wait4 | 跟踪者等待被跟踪进程 |

---

## 七、Rust 重构与设计改进建议

### 7.1 类型系统改进

**当前问题**：跟踪状态使用整数和位标志

```c
int mp_tracer;              // -1 或进程索引
unsigned mp_trace_flags;    // 位标志
unsigned mp_flags;          // 包含 TRACE_STOPPED 等
```

**Rust 改进**：使用 Option 和状态机

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TracerId(usize);

struct TraceState {
    tracer: Option<TracerId>,
    options: TraceOptions,
    status: TraceStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TraceStatus {
    Running,
    Stopped(Signal),
    Exiting,
}

bitflags::bitflags! {
    struct TraceOptions: u32 {
        const TRACE_FORK = 0x1;
        const ALT_EXEC = 0x2;
        const NO_EXEC = 0x4;
    }
}
```

### 7.2 ptrace 请求建模

**当前问题**：switch-case 分发，请求类型松散

```c
switch (req) {
    case T_OK: ...
    case T_ATTACH: ...
    // ...
}
```

**Rust 改进**：使用枚举和 trait

```rust
enum PtraceRequest {
    TraceMe,
    Attach(Pid),
    Detach { pid: Pid, signal: Option<Signal> },
    Continue { pid: Pid, signal: Option<Signal> },
    Step { pid: Pid, signal: Option<Signal> },
    Kill(Pid),
    SetOptions { pid: Pid, options: TraceOptions },
    GetRange { pid: Pid, range: PtraceRange },
    SetRange { pid: Pid, range: PtraceRange },
    // ...
}

impl ProcessManager {
    fn handle_ptrace(&mut self, req: PtraceRequest) -> Result<PtraceResult, PtraceError> {
        match req {
            PtraceRequest::TraceMe => self.trace_me(),
            PtraceRequest::Attach(pid) => self.attach_process(pid),
            // ...
        }
    }
}
```

### 7.3 权限检查抽象

**当前问题**：权限检查分散在代码中

```c
if (mp->mp_effuid != SUPER_USER &&
    (mp->mp_effuid != child->mp_effuid || ...))
    return(EPERM);
```

**Rust 改进**：使用 trait 抽象

```rust
trait TracePermission {
    fn can_trace(&self, target: &Process) -> Result<(), TraceError>;
}

impl TracePermission for Process {
    fn can_trace(&self, target: &Process) -> Result<(), TraceError> {
        // root 可以跟踪任何进程
        if self.is_root() {
            return Ok(());
        }
        
        // 系统服务器不能跟踪任何人
        if self.is_privileged() {
            return Err(TraceError::PermissionDenied);
        }
        
        // 不能跟踪系统服务器
        if target.is_privileged() {
            return Err(TraceError::PermissionDenied);
        }
        
        // 不能跟踪自己
        if self.pid() == target.pid() {
            return Err(TraceError::PermissionDenied);
        }
        
        // UID/GID 必须匹配
        if self.euid() != target.euid() ||
           self.egid() != target.egid() ||
           target.euid() != target.ruid() ||
           target.egid() != target.rgid() {
            return Err(TraceError::PermissionDenied);
        }
        
        Ok(())
    }
}
```

### 7.4 性能分析接口改进

**当前问题**：条件编译，接口简单

```c
#if SPROFILE
    return sys_sprof(...);
#else
    return ENOSYS;
#endif
```

**Rust 改进**：使用 feature flag 和类型安全

```rust
#[cfg(feature = "sprofile")]
impl ProcessManager {
    fn start_profile(&mut self, config: ProfileConfig) -> Result<ProfileHandle, ProfileError> {
        // 类型安全的配置
        let ProfileConfig {
            buffer_size,
            frequency,
            interrupt_type,
            buffer_ptr,
        } = config;
        
        // 验证参数
        if frequency == 0 || frequency > MAX_FREQUENCY {
            return Err(ProfileError::InvalidFrequency);
        }
        
        // 调用内核
        sys_sprof(ProfAction::Start, config)
    }
    
    fn stop_profile(&mut self) -> Result<ProfileData, ProfileError> {
        sys_sprof(ProfAction::Stop, ProfileConfig::default())
    }
}

#[cfg(not(feature = "sprofile"))]
impl ProcessManager {
    fn start_profile(&mut self, _: ProfileConfig) -> Result<ProfileHandle, ProfileError> {
        Err(ProfileError::NotSupported)
    }
}
```

### 7.5 当前设计问题总结

| 问题 | 代码位置 | 说明 |
|------|----------|------|
| 状态分散 | mp_tracer, mp_flags, mp_trace_flags | 跟踪状态分散在多个字段 |
| 权限检查复杂 | T_ATTACH 分支 | 多个条件组合，难以维护 |
| 信号拦截隐式 | mp_sigtrace | 信号处理逻辑分散 |
| 条件编译 | profile.c | 功能开关不够灵活 |
| 错误处理 | 返回整数错误码 | 无法区分错误类型 |

### 7.6 改进方向

#### 7.6.1 跟踪状态机

```rust
enum TraceState {
    NotTraced,
    Traced {
        tracer: TracerId,
        options: TraceOptions,
        status: ProcessStatus,
    },
}

enum ProcessStatus {
    Running,
    Stopped { signal: Signal, reason: StopReason },
    Exiting { status: ExitStatus },
}
```

#### 7.6.2 异步跟踪操作

```rust
async fn attach(&mut self, pid: Pid) -> Result<(), TraceError> {
    let target = self.find_process(pid)?;
    self.check_permission(&target)?;
    
    // 建立跟踪关系
    target.set_tracer(self.id());
    
    // 异步等待进程停止
    sig_proc(&target, Signal::STOP).await?;
    
    Ok(())
}
```

#### 7.6.3 错误类型细化

```rust
enum TraceError {
    ProcessNotFound,
    PermissionDenied,
    AlreadyTraced,
    NotTraced,
    ProcessNotStopped,
    InvalidSignal,
    KernelError(i32),
}
```

---

## 八、总结

### 8.1 核心知识点

1. **分层设计**：PM 处理权限和状态，内核处理底层操作
2. **权限模型**：严格的权限检查防止未授权调试
3. **状态管理**：TRACE_STOPPED 标志控制进程停止状态
4. **信号拦截**：mp_sigtrace 存储需要交给跟踪者的信号

### 8.2 关键设计决策

| 决策 | 原因 |
|------|------|
| 保护 PM/VM 不被调试 | 防止核心服务被破坏 |
| 禁止调试 setuid 程序 | 防止权限提升攻击 |
| TO_TRACEFORK 选项 | 支持多进程调试 |
| 延迟退出处理 | 保证 VFS 调用完整性 |

### 8.3 设计张力点

1. **权限 vs 功能**：严格权限检查限制了调试能力
2. **同步 vs 异步**：某些操作需要等待 VFS 完成
3. **条件编译**：性能分析功能默认关闭
4. **信号处理复杂性**：被跟踪进程的信号需要特殊处理
