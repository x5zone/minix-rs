# PM 信号与事件处理模块

> **模块范围**: `servers/pm/signal.c` + `servers/pm/event.c`
> **核心功能**: Unix 信号机制实现、进程事件发布/订阅
> **所属服务**: PM（Process Manager）

---

## 一、模块整体定位

### 1.1 在系统中的作用

本模块负责 PM 服务中与信号和事件相关的功能：

| 子模块 | 文件 | 核心功能 |
|--------|------|----------|
| **信号处理** | `signal.c` | Unix 信号机制的完整实现 |
| **事件发布** | `event.c` | 进程事件的发布/订阅机制 |

### 1.2 与其他模块的关系

```
                    ┌─────────────────────────────────────────────────────────┐
                    │                      信号源                              │
                    │   用户调用 kill() / 键盘 Ctrl+C / 时钟 SIGALRM / 异常     │
                    └──────────────────────────┬──────────────────────────────┘
                                               │
                                               ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                              PM（Process Manager）                           │
│  ┌─────────────────────────────┐  ┌─────────────────────────────┐          │
│  │       signal.c              │  │        event.c              │          │
│  │  do_sigaction()             │  │  do_proceventmask()         │          │
│  │  do_sigprocmask()           │  │  do_proc_event_reply()      │          │
│  │  do_kill()                  │  │  publish_event()            │          │
│  │  check_sig()                │  │  resume_event()             │          │
│  │  sig_proc()                 │  └──────────────┬──────────────┘          │
│  │  check_pending()            │                 │                          │
│  └──────────────┬──────────────┘                 │                          │
│                 │                                │                          │
│                 │ 内核调用 / IPC                  │ 异步消息 (asynsend)       │
│                 ▼                                ▼                          │
└─────────────────┼────────────────────────────────┼──────────────────────────┘
                  │                                │
    ┌─────────────┴─────────────┐    ┌─────────────┴─────────────┐
    │         Kernel            │    │      订阅服务              │
    │  sys_sigreturn()          │    │  (如 IPC Server)          │
    │  sys_kill()               │    │  - 接收事件通知            │
    │  sys_delay_stop()         │    │  - 清理进程资源            │
    └───────────────────────────┘    └───────────────────────────┘
```

### 1.3 核心设计约束

**信号的特殊性**：

1. **异步性**：信号可在任何时刻到达，打断正常执行流程
2. **复杂性**：信号可被捕获、忽略、阻塞、默认处理
3. **竞争条件**：信号到达时机不确定，需谨慎处理竞态

**事件发布的设计权衡**：

| 方式 | 消息数量 | 延迟 | 复杂度 |
|------|---------|------|--------|
| 串行同步 | NR_PROCS | 高 | 低 |
| 并行同步 | NR_PROCS × NR_SUBS | 中 | 中 |
| 异步通知 | 无上限 | 低 | 高 |

**Minix3 选择串行同步**：避免消息队列溢出，限制异步消息数量。

---

## 二、核心数据结构

### 2.1 mproc 中的信号相关字段

```c
struct mproc {
    // 信号处理设置
    struct sigaction mp_sigact[_NSIG];  // 每个信号的处理方式
    
    // 信号掩码集合
    sigset_t mp_ignore;       // 被忽略的信号
    sigset_t mp_catch;        // 被捕获的信号
    sigset_t mp_sigmask;      // 被阻塞的信号（当前屏蔽字）
    sigset_t mp_sigmask2;     // sigsuspend 保存的旧屏蔽字
    sigset_t mp_sigpending;   // 待处理的信号（已发送但被阻塞）
    sigset_t mp_ksigpending;  // 来自内核的待处理信号
    sigset_t mp_sigtrace;     // 待传递给调试器的信号
    
    // 信号相关状态
    int mp_sigreturn;         // sigreturn 函数地址
    char mp_sigstatus;        // 导致终止的信号编号
    
    // 进程标志（信号相关）
    int mp_flags;             // 包含 SIGSUSPENDED, PROC_STOPPED 等
    
    // 事件订阅相关
    int mp_eventsub;          // 当前事件订阅者索引
    
    // ...
};
```

### 2.2 sigaction 结构

```c
struct sigaction {
    void (*sa_handler)(int);  // 信号处理函数指针
    sigset_t sa_mask;         // 处理信号时要阻塞的信号
    int sa_flags;             // 信号处理标志
};
```

**sa_handler 的三种值**：

| 值 | 含义 |
|-----|------|
| `SIG_DFL` (0) | 默认处理 |
| `SIG_IGN` (1) | 忽略信号 |
| 其他地址 | 用户定义的处理函数 |

### 2.3 信号掩码集合关系

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         进程信号状态机                                        │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   ┌─────────────────────────────────────────────────────────────────────┐  │
│   │  mp_ignore: 被忽略的信号                                              │  │
│   │  - 通过 sigaction(SIG_IGN) 设置                                       │  │
│   │  - 信号到达时直接丢弃                                                 │  │
│   └─────────────────────────────────────────────────────────────────────┘  │
│                                                                             │
│   ┌─────────────────────────────────────────────────────────────────────┐  │
│   │  mp_catch: 被捕获的信号                                               │  │
│   │  - 通过 sigaction(用户函数) 设置                                      │  │
│   │  - 信号到达时执行用户函数                                             │  │
│   └─────────────────────────────────────────────────────────────────────┘  │
│                                                                             │
│   ┌─────────────────────────────────────────────────────────────────────┐  │
│   │  mp_sigmask: 当前信号屏蔽字                                           │  │
│   │  - 通过 sigprocmask() 设置                                           │  │
│   │  - 被阻塞的信号暂存于 mp_sigpending                                   │  │
│   └─────────────────────────────────────────────────────────────────────┘  │
│                                                                             │
│   ┌─────────────────────────────────────────────────────────────────────┐  │
│   │  mp_sigpending: 待处理信号                                            │  │
│   │  - 信号到达但被阻塞时存储于此                                         │  │
│   │  - 解除阻塞后由 check_pending() 投递                                  │  │
│   └─────────────────────────────────────────────────────────────────────┘  │
│                                                                             │
│   ┌─────────────────────────────────────────────────────────────────────┐  │
│   │  mp_ksigpending: 来自内核的待处理信号                                 │  │
│   │  - 内核产生的信号（如 SIGSEGV）                                       │  │
│   │  - 用于区分信号来源，某些处理逻辑不同                                  │  │
│   └─────────────────────────────────────────────────────────────────────┘  │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 2.4 事件订阅数据结构

```c
#define NR_SUBS  4  // 最大订阅者数量

static struct {
    endpoint_t endpt;      // 订阅者端点
    unsigned int mask;     // 事件掩码 (PROC_EVENT_EXIT | PROC_EVENT_SIGNAL)
    unsigned int waiting;  // 等待回复的进程数
} subs[NR_SUBS];

static unsigned int nsubs = 0;  // 当前订阅者数量
static unsigned int nested = 0; // 嵌套深度（调试用）
```

### 2.5 事件类型

```c
#define PROC_EVENT_EXIT   0x01  // 进程退出事件
#define PROC_EVENT_SIGNAL 0x02  // 进程捕获信号事件
```

### 2.6 进程标志（信号相关）

```c
#define SIGSUSPENDED  0x000100  // 进程在 sigsuspend 中
#define PROC_STOPPED  0x000200  // 进程在内核中已停止
#define DELAY_CALL    0x000400  // 进程有延迟调用
#define UNPAUSED      0x000800  // VFS 已回复 unpause 请求
#define VFS_CALL      0x001000  // 进程在等待 VFS 回复
#define EVENT_CALL    0x002000  // 进程在等待事件订阅者回复
```

---

## 三、核心流程

### 3.1 信号处理流程

#### 3.1.1 信号生命周期

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           信号生命周期                                        │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   ┌─────────────┐     ┌─────────────┐     ┌─────────────┐                  │
│   │   产生      │ ──► │   投递      │ ──► │   处理      │                  │
│   │ (Generate)  │     │ (Deliver)   │     │ (Handle)    │                  │
│   └─────────────┘     └─────────────┘     └─────────────┘                  │
│         │                   │                   │                          │
│         ▼                   ▼                   ▼                          │
│   ┌─────────────┐     ┌─────────────┐     ┌─────────────┐                  │
│   │ kill()      │     │ check_sig() │     │ sig_proc()  │                  │
│   │ 内核异常    │     │ 选择目标    │     │ 决定处理    │                  │
│   │ 时钟定时器  │     │ 进程        │     │ 方式        │                  │
│   │ 键盘输入    │     │             │     │             │                  │
│   └─────────────┘     └─────────────┘     └─────────────┘                  │
│                                                                             │
│   处理方式:                                                                  │
│   ┌─────────────────────────────────────────────────────────────────────┐  │
│   │  1. 忽略 (SIG_IGN):  直接丢弃                                        │  │
│   │  2. 默认 (SIG_DFL):  终止/停止/继续/忽略                              │  │
│   │  3. 捕获 (用户函数): 执行用户定义的处理函数                           │  │
│   │  4. 阻塞 (被 mask):  存入 pending，等待解除阻塞                       │  │
│   └─────────────────────────────────────────────────────────────────────┘  │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### 3.1.2 check_sig 流程

```c
int check_sig(pid_t proc_id, int signo, int ksig)
```

**参数说明**：

| 参数 | 含义 |
|------|------|
| `proc_id > 0` | 发送给指定 PID 的进程 |
| `proc_id == 0` | 发送给调用者进程组的所有进程 |
| `proc_id == -1` | 广播给所有进程 |
| `proc_id < -1` | 发送给指定进程组的所有进程 |
| `ksig` | TRUE 表示信号来自内核 |

**流程**：

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                          check_sig 流程                                      │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   1. 参数验证                                                                │
│      if (signo < 0 || signo >= _NSIG) return EINVAL;                       │
│      if (proc_id == INIT_PID && signo == SIGKILL) return EINVAL;           │
│                                                                             │
│   2. 广播 SIGTERM 时优先发送给 RS                                            │
│      if (proc_id == -1 && signo == SIGTERM)                                │
│          sys_kill(RS_PROC_NR, signo);                                      │
│                                                                             │
│   3. 遍历进程表（从后向前）                                                   │
│      for (rmp = &mproc[NR_PROCS-1]; rmp >= &mproc[0]; rmp--)               │
│                                                                             │
│   4. 进程选择                                                                │
│      - proc_id > 0: 匹配 PID                                                │
│      - proc_id == 0: 匹配进程组                                             │
│      - proc_id == -1: 所有进程（跳过 INIT）                                  │
│      - proc_id < -1: 匹配指定进程组                                         │
│                                                                             │
│   5. 权限检查                                                                │
│      - 超级用户可以发送任何信号                                              │
│      - 普通用户只能发送给自己拥有的进程                                       │
│                                                                             │
│   6. 特殊处理                                                                │
│      - 广播 SIGKILL 时跳过系统进程                                           │
│      - 跳过 VM 进程（避免死锁）                                              │
│      - 用户进程不能向系统进程发送致命信号                                     │
│                                                                             │
│   7. 调用 sig_proc() 投递信号                                               │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### 3.1.3 sig_proc 流程

```c
void sig_proc(struct mproc *rmp, int signo, int trace, int ksig)
```

**流程**：

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                          sig_proc 流程                                       │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   1. 调试器检查                                                              │
│      if (trace && rmp->mp_tracer != NO_TRACER && signo != SIGKILL)         │
│          → 传递给调试器，返回                                                │
│                                                                             │
│   2. VFS/EVENT 调用中                                                        │
│      if (rmp->mp_flags & (VFS_CALL | EVENT_CALL))                          │
│          → 加入 pending，停止进程，返回                                      │
│                                                                             │
│   3. 系统进程处理                                                            │
│      if (rmp->mp_flags & PRIV_PROC)                                        │
│          → 跳过 PM 自身                                                     │
│          → 非内核信号：转发给内核                                            │
│          → 非终止信号：发送异步消息                                          │
│          → 终止信号：调用 sig_proc_exit()                                   │
│                                                                             │
│   4. 用户进程处理                                                            │
│      if (信号被忽略) → 直接返回                                              │
│      if (信号被阻塞) → 加入 pending，返回                                    │
│      if (进程被调试器停止) → 加入 pending，返回                              │
│      if (信号被捕获) → unpause + sig_send                                   │
│      if (默认忽略) → 直接返回                                               │
│      else → sig_proc_exit() 终止进程                                        │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### 3.1.4 信号投递状态机

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        信号投递状态机                                         │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│                          ┌──────────────┐                                   │
│                          │   信号到达    │                                   │
│                          └──────┬───────┘                                   │
│                                 │                                           │
│                                 ▼                                           │
│                     ┌───────────────────────┐                               │
│                     │  进程是否在 VFS/EVENT  │                               │
│                     │       调用中？         │                               │
│                     └───────────┬───────────┘                               │
│                           │           │                                     │
│                          是          否                                      │
│                           │           │                                     │
│                           ▼           ▼                                     │
│              ┌─────────────────┐   ┌─────────────────┐                      │
│              │ 加入 pending    │   │ 进程是系统进程？ │                      │
│              │ 停止进程        │   └────────┬────────┘                      │
│              │ 等待回复        │         │        │                         │
│              └─────────────────┘        是       否                         │
│                                          │        │                         │
│                                          ▼        ▼                         │
│                              ┌─────────────┐  ┌─────────────────┐           │
│                              │ 内核信号？   │  │ 信号处理决策    │           │
│                              └──────┬──────┘  └────────┬────────┘           │
│                                │      │             │                      │
│                               是     否      ┌───────┼───────┐              │
│                                │      │      │       │       │              │
│                                ▼      ▼      ▼       ▼       ▼              │
│                          ┌────────┐ ┌────────┐ ┌────┐ ┌────┐ ┌────┐        │
│                          │发送消息│ │转发内核│ │忽略│ │阻塞│ │捕获│        │
│                          │或终止  │ │        │ │    │ │    │ │    │        │
│                          └────────┘ └────────┘ └────┘ └────┘ └────┘        │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 3.2 信号系统调用

#### 3.2.1 sigaction 流程

```c
int do_sigaction(void)
```

**系统调用接口**：

```c
int sigaction(int signo, const struct sigaction *act, struct sigaction *oact);
```

**流程**：

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        do_sigaction 流程                                     │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   1. 参数验证                                                                │
│      - SIGKILL 不能被捕获或忽略                                             │
│      - 信号编号必须在 1 到 _NSIG-1 之间                                     │
│                                                                             │
│   2. 返回旧设置（如果 oact 非空）                                            │
│      sys_datacopy(PM → 用户空间)                                            │
│                                                                             │
│   3. 读取新设置（如果 act 非空）                                             │
│      sys_datacopy(用户空间 → PM)                                            │
│                                                                             │
│   4. 更新信号掩码集合                                                        │
│      if (sa_handler == SIG_IGN)                                            │
│          mp_ignore |= signo; mp_catch &= ~signo;                           │
│          mp_sigpending &= ~signo;  // 清除待处理                            │
│      else if (sa_handler == SIG_DFL)                                       │
│          mp_ignore &= ~signo; mp_catch &= ~signo;                          │
│      else                                                                   │
│          mp_ignore &= ~signo; mp_catch |= signo;                           │
│                                                                             │
│   5. 保存设置                                                                │
│      mp_sigact[signo] = svec;                                              │
│      mp_sigreturn = ret_addr;                                              │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### 3.2.2 sigprocmask 流程

```c
int do_sigprocmask(void)
```

**四种操作模式**：

| 模式 | 操作 | 后续动作 |
|------|------|----------|
| `SIG_BLOCK` | 将 set 加入屏蔽字 | 无 |
| `SIG_UNBLOCK` | 将 set 从屏蔽字移除 | 调用 check_pending() |
| `SIG_SETMASK` | 完全替换屏蔽字 | 调用 check_pending() |
| `SIG_INQUIRE` | 只返回当前屏蔽字 | 无 |

**关键点**：解除阻塞后必须检查待处理信号。

#### 3.2.3 sigsuspend 流程

```c
int do_sigsuspend(void)
```

**原子操作**：

```
1. 保存旧屏蔽字 → mp_sigmask2
2. 设置新屏蔽字 → mp_sigmask
3. 设置标志 → mp_flags |= SIGSUSPENDED
4. 检查待处理信号 → check_pending()
5. 挂起进程 → return SUSPEND
```

**为什么需要原子操作**：防止在设置屏蔽字和挂起之间丢失信号。

#### 3.2.4 sigreturn 流程

```c
int do_sigreturn(void)
```

**调用时机**：用户信号处理函数返回后

**流程**：

```
1. 恢复信号屏蔽字
2. 调用 sys_sigreturn() 恢复寄存器上下文
3. 调用 check_pending() 检查待处理信号
```

### 3.3 进程停止与恢复

#### 3.3.1 stop_proc 流程

```c
static int stop_proc(struct mproc *rmp, int may_delay)
```

**返回值**：

| 返回值 | 含义 |
|--------|------|
| TRUE | 进程已停止 |
| FALSE | 进程正在发送消息，需要延迟 |

**延迟调用机制**：

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                          延迟调用机制                                        │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   场景：进程正在发送消息，无法立即停止                                        │
│                                                                             │
│   1. sys_delay_stop() 返回 EBUSY                                           │
│                                                                             │
│   2. 设置 DELAY_CALL 标志                                                   │
│      rmp->mp_flags |= DELAY_CALL;                                          │
│                                                                             │
│   3. 等待内核发送 SIGSNDELAY                                                │
│      （当进程完成消息发送后）                                                 │
│                                                                             │
│   4. process_ksig() 收到 SIGSNDELAY                                        │
│      - 清除 DELAY_CALL 标志                                                 │
│      - 继续信号处理流程                                                      │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### 3.3.2 try_resume_proc 流程

```c
static void try_resume_proc(struct mproc *rmp)
```

**恢复条件**：

- 进程不在 VFS_CALL、EVENT_CALL、EXITING 状态
- 调用 `sys_resume()` 恢复进程运行
- 清除 PROC_STOPPED 和 UNPAUSED 标志

### 3.4 事件发布流程

#### 3.4.1 publish_event 流程

```c
void publish_event(struct mproc *rmp)
```

**调用时机**：
- 进程退出时（EXITING 标志）
- 进程收到信号时（UNPAUSED 标志）

**流程**：

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        publish_event 流程                                    │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   1. 检查是否是订阅服务退出                                                   │
│      if (PRIV_PROC && EXITING)                                             │
│          → 从订阅列表中移除                                                  │
│                                                                             │
│   2. 设置事件调用状态                                                        │
│      rmp->mp_flags |= EVENT_CALL;                                          │
│      rmp->mp_eventsub = 0;                                                 │
│                                                                             │
│   3. 调用 resume_event() 发送事件消息                                        │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### 3.4.2 resume_event 流程

```c
static void resume_event(struct mproc *rmp)
```

**流程**：

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        resume_event 流程                                     │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   1. 确定事件类型                                                            │
│      if (EXITING) → PROC_EVENT_EXIT                                        │
│      if (UNPAUSED) → PROC_EVENT_SIGNAL                                     │
│                                                                             │
│   2. 遍历订阅者（从 mp_eventsub 开始）                                       │
│      for (i = mp_eventsub; i < nsubs; i++)                                 │
│          if (subs[i].mask & event)                                         │
│              → 发送异步消息                                                  │
│              → 增加 waiting 计数                                            │
│              → 返回等待回复                                                  │
│                                                                             │
│   3. 所有订阅者已通知                                                        │
│      mp_flags &= ~EVENT_CALL;                                              │
│      if (EXIT) → exit_restart()                                            │
│      if (SIGNAL) → restart_sigs()                                          │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### 3.4.3 do_proc_event_reply 流程

```c
int do_proc_event_reply(void)
```

**订阅者回复处理**：

```
1. 验证调用者权限（必须是 PRIV_PROC）
2. 验证回复的有效性
   - 目标进程存在
   - 目标进程在 EVENT_CALL 状态
   - 事件类型匹配
3. 减少 waiting 计数
4. 继续下一个订阅者或完成事件处理
```

---

## 四、关键机制拆解

### 4.1 信号掩码集合管理

**五种掩码集合**：

| 掩码 | 用途 | 修改方式 |
|------|------|----------|
| `mp_ignore` | 被忽略的信号 | sigaction(SIG_IGN) |
| `mp_catch` | 被捕获的信号 | sigaction(用户函数) |
| `mp_sigmask` | 当前阻塞的信号 | sigprocmask() |
| `mp_sigpending` | 待处理的信号 | 信号到达时自动添加 |
| `mp_ksigpending` | 内核产生的待处理信号 | 内核信号到达时添加 |

**掩码操作函数**：

```c
sigaddset(&set, signo);   // 添加信号到集合
sigdelset(&set, signo);   // 从集合移除信号
sigismember(&set, signo); // 检查信号是否在集合中
```

### 4.2 进程停止机制

**停止进程的时机**：

1. 信号到达时，进程在 VFS/EVENT 调用中
2. 需要投递信号给进程
3. 进程在 sigsuspend 中等待

**停止流程**：

```
sys_delay_stop(endpoint)
       │
       ├── OK → 设置 PROC_STOPPED 标志
       │
       └── EBUSY → 设置 DELAY_CALL 标志
                   等待 SIGSNDELAY
```

### 4.3 unpause 机制

**目的**：中断进程的阻塞调用

**阻塞类型**：

| 阻塞类型 | 处理方式 |
|----------|----------|
| PM 内部（WAITING, SIGSUSPENDED） | 停止进程，返回 TRUE |
| VFS 调用（read, write 等） | 发送 VFS_PM_UNPAUSE 消息，返回 FALSE |
| 事件订阅者 | 等待订阅者回复 |

### 4.4 系统进程信号处理

**与用户进程的区别**：

| 特性 | 用户进程 | 系统进程 |
|------|----------|----------|
| 信号来源 | 直接处理 | 先转发给内核 |
| 非终止信号 | 执行处理函数 | 发送异步消息 |
| 终止信号 | exit_proc() | sig_proc_exit() |
| 忽略/阻塞 | 支持 | 部分支持 |

**系统信号消息格式**：

```c
message m;
m.m_type = SIGS_SIGNAL_RECEIVED;
m.m_pm_lsys_sigs_signal.num = signo;
asynsend3(endpoint, &m, AMF_NOREPLY);
```

### 4.5 事件订阅机制

**订阅流程**：

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                          事件订阅流程                                        │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   订阅服务 (如 IPC Server)                                                   │
│        │                                                                    │
│        │ do_proceventmask(mask)                                            │
│        ▼                                                                    │
│   ┌─────────────────────────────────────────────────────────────────────┐  │
│   │  PM 检查权限 (必须是 PRIV_PROC)                                       │  │
│   │  添加/更新订阅者信息                                                  │  │
│   │  subs[nsubs].endpt = who_e;                                          │  │
│   │  subs[nsubs].mask = mask;                                            │  │
│   └─────────────────────────────────────────────────────────────────────┘  │
│                                                                             │
│   事件发生时:                                                                │
│        │                                                                    │
│        │ publish_event(rmp)                                                │
│        ▼                                                                    │
│   ┌─────────────────────────────────────────────────────────────────────┐  │
│   │  设置 EVENT_CALL 标志                                                │  │
│   │  依次通知订阅者                                                      │  │
│   │  等待每个订阅者回复                                                  │  │
│   │  完成后恢复事件处理                                                  │  │
│   └─────────────────────────────────────────────────────────────────────┘  │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 五、边界条件与特殊分支

### 5.1 SIGKILL 和 SIGSTOP 特殊处理

```c
// 不能被捕获
if (sig_nr == SIGKILL) return(OK);

// 不能被阻塞
sigdelset(&set, SIGKILL);
sigdelset(&set, SIGSTOP);

// 不能发送给 INIT
if (proc_id == INIT_PID && signo == SIGKILL) return(EINVAL);
```

### 5.2 广播信号的特殊处理

```c
// 广播 SIGTERM 时优先发送给 RS
if (proc_id == -1 && signo == SIGTERM)
    sys_kill(RS_PROC_NR, signo);

// 广播 SIGKILL 时跳过系统进程
if (proc_id == -1 && signo == SIGKILL && (rmp->mp_flags & PRIV_PROC))
    continue;

// 跳过 VM 进程（避免死锁）
if (rmp->mp_endpoint == VM_PROC_NR) continue;
```

### 5.3 用户进程向系统进程发送信号

```c
// 用户进程不能向系统进程发送致命信号
if (!ksig && SIGS_IS_LETHAL(signo) && (rmp->mp_flags & PRIV_PROC)) {
    error_code = EPERM;
    continue;
}
```

### 5.4 进程退出时的信号处理

```c
// 退出中的进程不再接收信号
if ((rmp->mp_flags & (IN_USE | EXITING)) != IN_USE) {
    return EDEADEPT;
}
```

### 5.5 调试器跟踪

```c
// 信号先传递给调试器
if (trace == TRUE && rmp->mp_tracer != NO_TRACER && signo != SIGKILL) {
    sigaddset(&rmp->mp_sigtrace, signo);
    if (!(rmp->mp_flags & TRACE_STOPPED))
        trace_stop(rmp, signo);
    return;
}
```

### 5.6 订阅服务退出

```c
// 订阅服务退出时，从订阅列表移除
if ((rmp->mp_flags & (PRIV_PROC | EXITING)) == (PRIV_PROC | EXITING)) {
    for (i = 0; i < nsubs; i++) {
        if (subs[i].endpt == rmp->mp_endpoint) {
            remove_sub(i);
            break;
        }
    }
}
```

### 5.7 核心转储信号

```c
// 某些信号会产生 core dump
if (sigismember(&core_sset, signo)) {
    exit_proc(rmp, 0, TRUE /*dump_core*/);
}
```

---

## 六、模块交互关系

### 6.1 上游调用者

| 调用者 | 方式 | 说明 |
|--------|------|------|
| 用户进程 | 系统调用 | kill, sigaction, sigprocmask 等 |
| 内核 | 内核调用 | process_ksig 处理内核产生的信号 |
| RS | 系统调用 | do_srv_kill 强制终止系统进程 |
| VFS | 回复消息 | 触发 restart_sigs |
| 订阅服务 | 回复消息 | do_proc_event_reply |

### 6.2 下游依赖

| 服务 | 调用函数 | 说明 |
|------|----------|------|
| Kernel | sys_sigreturn() | 恢复信号上下文 |
| Kernel | sys_kill() | 发送信号给系统进程 |
| Kernel | sys_delay_stop() | 停止进程 |
| Kernel | sys_resume() | 恢复进程运行 |
| VFS | tell_vfs() | 发送 unpause 请求 |
| 订阅服务 | asynsend3() | 发送事件通知 |

### 6.3 IPC 消息类型

| 消息类型 | 方向 | 说明 |
|----------|------|------|
| `VFS_PM_UNPAUSE` | PM → VFS | 请求中断阻塞调用 |
| `PROC_EVENT` | PM → 订阅者 | 进程事件通知 |
| `SIGS_SIGNAL_RECEIVED` | PM → 系统进程 | 系统信号通知 |

---

## 七、Rust 重构与设计改进建议

### 7.1 类型系统改进

**当前问题**：信号编号使用整数，容易出错

```c
int signo;  // 信号编号，但可以是任意整数
```

**Rust 改进**：使用枚举

```rust
#[repr(i32)]
enum Signal {
    SIGHUP = 1,
    SIGINT = 2,
    SIGQUIT = 3,
    SIGILL = 4,
    SIGTRAP = 5,
    // ...
    SIGKILL = 9,
    SIGTERM = 15,
    // ...
}

impl Signal {
    fn from_raw(n: i32) -> Option<Self> {
        // 安全转换
    }
    
    fn is_catchable(&self) -> bool {
        !matches!(self, Signal::SIGKILL | Signal::SIGSTOP)
    }
}
```

### 7.2 信号处理方式枚举

**当前问题**：sa_handler 使用 void* 指针

```c
void (*sa_handler)(int);  // 可以是 SIG_DFL, SIG_IGN 或函数地址
```

**Rust 改进**：使用枚举

```rust
enum SignalHandler {
    Default,                           // SIG_DFL
    Ignore,                            // SIG_IGN
    Catch(NonNull<extern "C" fn(i32)>), // 用户函数
}

struct SigAction {
    handler: SignalHandler,
    mask: SigSet,
    flags: SigFlags,
}
```

### 7.3 信号掩码类型安全

**当前问题**：sigset_t 是不透明类型

```c
sigset_t mp_sigmask;  // 位图，但操作不安全
```

**Rust 改进**：使用位标志

```rust
bitflags::bitflags! {
    struct SigSet: u64 {
        const SIGHUP  = 1 << 0;
        const SIGINT  = 1 << 1;
        const SIGQUIT = 1 << 2;
        // ...
    }
}

impl SigSet {
    fn contains(&self, signal: Signal) -> bool {
        // 类型安全的检查
    }
}
```

### 7.4 进程状态建模

**当前问题**：mp_flags 使用位标志，状态组合复杂

```c
int mp_flags;  // 多个标志位组合
```

**Rust 改进**：使用状态机

```rust
enum ProcessState {
    Running,
    Stopped {
        reason: StopReason,
    },
    WaitingVfs {
        pending_signals: Vec<Signal>,
    },
    WaitingEvent {
        event_type: EventType,
        subscriber_index: usize,
    },
    Exiting {
        signal: Option<Signal>,
    },
}

enum StopReason {
    Signal(Signal),
    Debugger,
    Suspend,
}
```

### 7.5 事件订阅抽象

**当前问题**：订阅者数组是全局静态变量

```c
static struct { ... } subs[NR_SUBS];
```

**Rust 改进**：使用 trait 抽象

```rust
trait EventSubscriber: Send + Sync {
    fn endpoint(&self) -> Endpoint;
    fn interests(&self) -> EventMask;
    fn notify(&self, event: ProcessEvent) -> Result<(), Error>;
}

struct EventDispatcher {
    subscribers: Vec<Box<dyn EventSubscriber>>,
}

impl EventDispatcher {
    fn publish(&mut self, event: ProcessEvent) {
        for sub in &self.subscribers {
            if sub.interests().contains(event.kind()) {
                sub.notify(event.clone());
            }
        }
    }
}
```

### 7.6 当前设计问题总结

| 问题 | 代码位置 | 说明 |
|------|----------|------|
| 信号编号不安全 | 所有使用 signo 的地方 | 整数可以是任意值 |
| 处理函数类型混淆 | `sa_handler` | 指针和常量混用 |
| 状态标志复杂 | `mp_flags` | 多个标志位组合，难以理解 |
| 全局订阅者数组 | `subs[NR_SUBS]` | 不利于扩展和测试 |
| 错误处理不一致 | 各函数返回值 | 有的返回错误码，有的 panic |

### 7.7 改进方向

#### 7.7.1 信号处理状态机

```rust
enum SignalDisposition {
    Ignore,
    Default,
    Catch {
        handler: NonNull<extern "C" fn(i32)>,
        mask: SigSet,
        flags: SigFlags,
    },
}

impl Process {
    fn handle_signal(&mut self, signal: Signal) -> SignalResult {
        match self.signal_disposition(signal) {
            SignalDisposition::Ignore => SignalResult::Ignored,
            SignalDisposition::Default => self.default_action(signal),
            SignalDisposition::Catch { handler, .. } => {
                self.setup_handler(signal, handler)
            }
        }
    }
}
```

#### 7.7.2 异步信号投递

```rust
async fn deliver_signal(process: &mut Process, signal: Signal) -> Result<(), Error> {
    // 1. 检查是否可以立即投递
    if process.is_blocked(signal) {
        process.pending_signals.insert(signal);
        return Ok(());
    }
    
    // 2. 中断阻塞调用
    if process.is_in_vfs_call() {
        process.stop().await?;
        vfs::unpause(process.endpoint()).await?;
    }
    
    // 3. 投递信号
    process.deliver(signal).await
}
```

#### 7.7.3 事件通知改进

```rust
// 使用 channel 替代同步等待
struct EventNotifier {
    subscribers: HashMap<Endpoint, EventSender>,
}

impl EventNotifier {
    async fn notify(&self, event: ProcessEvent) {
        for (endpoint, sender) in &self.subscribers {
            if sender.interests().contains(event.kind()) {
                // 异步发送，不阻塞
                let _ = sender.send(event.clone()).await;
            }
        }
    }
}
```

---

## 八、总结

### 8.1 核心知识点

1. **信号生命周期**：产生 → 投递 → 处理，每一步都有复杂的条件判断
2. **掩码集合管理**：五种掩码集合协同工作，实现信号的忽略、阻塞、捕获
3. **进程停止机制**：延迟调用机制处理进程正在发送消息的情况
4. **事件发布订阅**：串行同步设计避免消息队列溢出

### 8.2 关键设计决策

| 决策 | 原因 |
|------|------|
| 信号处理在用户态 | 微内核设计，内核只负责基本调度 |
| 系统进程信号转发内核 | 内核需要选择正确的信号管理器 |
| 串行事件通知 | 限制异步消息数量，避免溢出 |
| 延迟调用机制 | 处理进程正在发送消息的情况 |

### 8.3 设计张力点

1. **信号异步性与同步需求**：信号可随时到达，但某些操作需要同步完成
2. **状态标志组合复杂**：多个标志位组合表示复杂状态，难以理解和维护
3. **系统进程与用户进程差异**：两种进程的信号处理逻辑不同，增加复杂度
4. **事件通知延迟**：串行通知增加延迟，但并行通知会增加消息数量
