# PM 调度与时间管理模块

> **模块范围**: `servers/pm/schedule.c` + `servers/pm/alarm.c` + `servers/pm/time.c`
> **核心功能**: 进程调度委托、间隔定时器、时间系统调用
> **所属服务**: PM（Process Manager）

---

## 一、模块整体定位

### 1.1 在系统中的作用

本模块负责 PM 服务中与调度和时间相关的功能：

| 子模块 | 文件 | 核心功能 |
|--------|------|----------|
| **调度委托** | `schedule.c` | PM 与调度器服务的交互接口 |
| **间隔定时器** | `alarm.c` | 闹钟和间隔定时器系统调用 |
| **时间管理** | `time.c` | 时间相关系统调用 |

### 1.2 与其他模块的关系

```
                    ┌─────────────────────────────────────────────────────────┐
                    │                      用户进程                            │
                    │   调用 nice() / alarm() / setitimer() / gettimeofday()  │
                    └──────────────────────────┬──────────────────────────────┘
                                               │ 系统调用
                                               ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                              PM（Process Manager）                           │
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐              │
│  │  schedule.c     │  │    alarm.c      │  │    time.c       │              │
│  │  sched_init()   │  │  do_itimer()    │  │  do_gettime()   │              │
│  │sched_start_user│  │  set_alarm()    │  │  do_settime()   │              │
│  │  sched_nice()   │  │  check_vtimer() │  │  do_time()      │              │
│  └────────┬────────┘  └────────┬────────┘  └────────┬────────┘              │
│           │                    │                    │                       │
│           │ IPC                │ 定时器库/内核       │ 内核调用              │
│           ▼                    ▼                    ▼                       │
└───────────┼────────────────────┼────────────────────┼───────────────────────┘
            │                    │                    │
    ┌───────┴───────┐    ┌───────┴───────┐    ┌───────┴───────┐
    │     SCHED     │    │    Kernel     │    │    Kernel     │
    │  调度器服务    │    │   定时器/时钟  │    │   时钟系统    │
    │               │    │               │    │               │
    │ 优先级管理    │    │ set_timer()   │    │ getuptime()   │
    │ 时间片分配    │    │ cancel_timer()│    │ sys_settime() │
    │ CPU 选择      │    │ sys_vtimer()  │    │ sys_stime()   │
    └───────────────┘    └───────────────┘    └───────────────┘
```

### 1.3 核心设计约束

**微内核架构的影响**：

1. **调度器是独立服务**：PM 不直接调度进程，而是委托给 SCHED 服务
2. **定时器分层管理**：实时定时器由 PM 管理，虚拟定时器由内核管理
3. **时间信息来自内核**：PM 通过内核调用获取时间信息

---

## 二、核心数据结构

### 2.1 mproc 中的调度与定时器字段

```c
struct mproc {
    // 调度相关
    endpoint_t mp_scheduler;      // 调度器端点（SCHED_PROC_NR/KERNEL/NONE）
    int mp_nice;                  // nice 值（-20 到 19）
    
    // 定时器相关
    timer_t mp_timer;             // 实时定时器结构
    clock_t mp_interval[NR_ITIMERS]; // 间隔定时器的周期
    int mp_flags;                 // 包含 ALARM_ON 等标志
    // ...
};
```

### 2.2 定时器类型

```c
#define ITIMER_REAL    0   // 实时定时器
#define ITIMER_VIRTUAL 1   // 虚拟定时器（用户态时间）
#define ITIMER_PROF    2   // 性能分析定时器（用户态+内核态）
#define NR_ITIMERS     3   // 定时器数量
```

### 2.3 时钟类型

```c
#define CLOCK_REALTIME  0   // 实时时钟（可设置）
#define CLOCK_MONOTONIC 1   // 单调时钟（不可设置）
```

### 2.4 时间结构体

```c
// 时间值（秒 + 纳秒）
struct timespec {
    time_t tv_sec;   // 秒
    long   tv_nsec;  // 纳秒
};

// 时间值（秒 + 微秒）
struct timeval {
    time_t tv_sec;        // 秒
    suseconds_t tv_usec;  // 微秒
};

// 间隔定时器
struct itimerval {
    struct timeval it_interval;  // 间隔（周期）
    struct timeval it_value;     // 首次到期时间
};
```

### 2.5 调度优先级常量

```c
#define NR_SCHED_QUEUES  16    // 调度队列数量
#define MAX_USER_Q       0     // 用户进程最高优先级
#define MIN_USER_Q       15    // 用户进程最低优先级
#define USER_Q           7     // 默认用户优先级
#define USER_QUANTUM     200   // 默认时间片（时钟滴答）
```

---

## 三、核心流程

### 3.1 调度委托流程

#### 3.1.1 调度架构

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         Minix3 调度架构                                      │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   ┌────────────────┐      IPC 消息      ┌────────────────┐                 │
│   │      PM        │ ◄─────────────────►│     SCHED      │                 │
│   │  进程管理服务   │                    │   调度器服务    │                 │
│   │                │                    │                │                 │
│   │ - fork/exit    │                    │ - 优先级队列    │                 │
│   │ - 调度委托     │                    │ - 时间片管理    │                 │
│   └───────┬────────┘                    │ - CPU 亲和性   │                 │
│           │                             └───────┬────────┘                 │
│           │                                     │                          │
│           │ 内核调用                            │ 内核调用                  │
│           ▼                                     ▼                          │
│   ┌────────────────────────────────────────────────────────┐              │
│   │                      Kernel                             │              │
│   │   - 进程调度执行（上下文切换）                            │              │
│   │   - 时钟中断处理                                         │              │
│   │   - 运行队列管理                                         │              │
│   └────────────────────────────────────────────────────────┘              │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### 3.1.2 sched_init 初始化流程

```c
void sched_init(void)
```

**调用时机**：PM 启动初始化阶段

**流程**：

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         sched_init 流程                                      │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   1. 遍历进程表                                                              │
│      for (proc_nr=0; proc_nr < NR_PROCS; proc_nr++)                        │
│                                                                             │
│   2. 跳过系统进程                                                            │
│      if (trmp->mp_flags & PRIV_PROC) continue;                             │
│                                                                             │
│   3. 验证是 INIT 进程                                                        │
│      assert(_ENDPOINT_P(trmp->mp_endpoint) == INIT_PROC_NR);               │
│                                                                             │
│   4. 调用调度器服务                                                          │
│      sched_start(SCHED_PROC_NR, endpoint, parent_e,                        │
│                  USER_Q, USER_QUANTUM, -1, &mp_scheduler);                  │
│                                                                             │
│   5. 设置进程的调度器字段                                                     │
│      mp_scheduler = SCHED_PROC_NR;                                          │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

**关键代码**：

```c
for (proc_nr=0, trmp=mproc; proc_nr < NR_PROCS; proc_nr++, trmp++) {
    if (trmp->mp_flags & IN_USE && !(trmp->mp_flags & PRIV_PROC)) {
        assert(_ENDPOINT_P(trmp->mp_endpoint) == INIT_PROC_NR);
        parent_e = mproc[trmp->mp_parent].mp_endpoint;
        s = sched_start(SCHED_PROC_NR,
            trmp->mp_endpoint,
            parent_e,
            USER_Q,
            USER_QUANTUM,
            -1,
            &trmp->mp_scheduler);
    }
}
```

#### 3.1.3 sched_start_user 流程

```c
int sched_start_user(endpoint_t ep, struct mproc *rmp)
```

**调用时机**：fork 创建新进程后

**流程**：

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                      sched_start_user 流程                                   │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   1. nice 值转优先级                                                         │
│      nice_to_priority(rmp->mp_nice, &maxprio)                              │
│                                                                             │
│   2. 确定继承来源                                                            │
│      if (父进程是系统进程)                                                   │
│          inherit_from = INIT_PROC_NR;                                       │
│      else                                                                   │
│          inherit_from = 父进程端点;                                          │
│                                                                             │
│   3. 调用调度器继承调度参数                                                   │
│      sched_inherit(SCHED_PROC_NR, endpoint, inherit_from,                  │
│                    maxprio, &mp_scheduler);                                 │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

**设计原因**：

- 系统进程（如 RS）的子进程可能没有调度器设置
- 需要从 INIT 继承调度参数，而不是从真实父进程

#### 3.1.4 sched_nice 流程

```c
int sched_nice(struct mproc *rmp, int nice)
```

**调用时机**：用户调用 `nice()` 系统调用

**流程**：

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        sched_nice 流程                                       │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   1. 检查调度器                                                              │
│      if (mp_scheduler == KERNEL || mp_scheduler == NONE)                   │
│          return EINVAL;                                                     │
│                                                                             │
│   2. nice 值转优先级                                                         │
│      nice_to_priority(nice, &maxprio)                                      │
│                                                                             │
│   3. 发送消息给调度器                                                        │
│      m.m_pm_sched_scheduling_set_nice.endpoint = endpoint;                 │
│      m.m_pm_sched_scheduling_set_nice.maxprio = maxprio;                   │
│      _taskcall(mp_scheduler, SCHEDULING_SET_NICE, &m);                     │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

**限制**：内核调度的进程（如 INIT）不能调整优先级

---

### 3.2 间隔定时器流程

#### 3.2.1 定时器类型对比

| 类型 | 计时条件 | 到期信号 | 管理者 |
|------|----------|----------|--------|
| `ITIMER_REAL` | 实时（始终计时） | `SIGALRM` | PM |
| `ITIMER_VIRTUAL` | 用户态运行时 | `SIGVTALRM` | Kernel |
| `ITIMER_PROF` | 用户态+内核态 | `SIGPROF` | Kernel |

#### 3.2.2 do_itimer 流程

```c
int do_itimer(void)
```

**系统调用接口**：

```c
int setitimer(int which, const struct itimerval *value, struct itimerval *ovalue);
int getitimer(int which, struct itimerval *value);
```

**流程**：

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         do_itimer 流程                                       │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   1. 验证定时器类型                                                          │
│      if (which < 0 || which >= NR_ITIMERS) return EINVAL;                  │
│                                                                             │
│   2. 确定操作类型                                                            │
│      setval = (value != 0);                                                 │
│      getval = (ovalue != 0);                                                │
│                                                                             │
│   3. 复制新值（如果设置）                                                     │
│      sys_datacopy(who_e, value, PM_PROC_NR, &value, sizeof(value));        │
│                                                                             │
│   4. 根据类型分发                                                            │
│      switch (which) {                                                       │
│          case ITIMER_REAL:                                                  │
│              get_realtimer() / set_realtimer()                              │
│          case ITIMER_VIRTUAL:                                               │
│          case ITIMER_PROF:                                                  │
│              getset_vtimer()                                                │
│      }                                                                      │
│                                                                             │
│   5. 返回旧值（如果请求）                                                     │
│      sys_datacopy(PM_PROC_NR, &ovalue, who_e, ovalue, sizeof(ovalue));     │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### 3.2.3 实时定时器管理

**set_realtimer 流程**：

```c
static void set_realtimer(struct mproc *rmp, struct itimerval *value)
{
    // 1. 转换时间为滴答数
    ticks = ticks_from_timeval(&value->it_value);
    interval = ticks_from_timeval(&value->it_interval);
    
    // 2. 如果无定时器，间隔必须为 0
    if (ticks <= 0) interval = 0;
    
    // 3. 设置定时器
    set_alarm(rmp, ticks);
    rmp->mp_interval[ITIMER_REAL] = interval;
}
```

**set_alarm 流程**：

```c
void set_alarm(struct mproc *rmp, clock_t ticks)
{
    if (ticks > 0) {
        // 设置定时器
        set_timer(&rmp->mp_timer, ticks, cause_sigalrm, rmp->mp_endpoint);
        rmp->mp_flags |= ALARM_ON;
    } else if (rmp->mp_flags & ALARM_ON) {
        // 取消定时器
        cancel_timer(&rmp->mp_timer);
        rmp->mp_flags &= ~ALARM_ON;
    }
}
```

**定时器到期处理**：

```c
static void cause_sigalrm(int arg)
{
    // 1. 获取进程
    pm_isokendpt(arg, &proc_nr_n);
    rmp = &mproc[proc_nr_n];
    
    // 2. 如果有间隔，重新设置定时器
    if (rmp->mp_interval[ITIMER_REAL] > 0)
        set_alarm(rmp, rmp->mp_interval[ITIMER_REAL]);
    else
        rmp->mp_flags &= ~ALARM_ON;
    
    // 3. 发送信号
    check_sig(rmp->mp_pid, SIGALRM, FALSE);
}
```

#### 3.2.4 虚拟定时器管理

**getset_vtimer 流程**：

```c
static void getset_vtimer(struct mproc *rmp, int which, 
                          struct itimerval *value, struct itimerval *ovalue)
{
    // 1. 准备参数
    optr = nptr = NULL;
    
    // 2. 如果获取旧值
    if (ovalue != NULL) {
        optr = &oldticks;
        timeval_from_ticks(&ovalue->it_interval, rmp->mp_interval[which]);
    }
    
    // 3. 如果设置新值
    if (value != NULL) {
        newticks = ticks_from_timeval(&value->it_value);
        nptr = &newticks;
        rmp->mp_interval[which] = ticks_from_timeval(&value->it_interval);
    }
    
    // 4. 调用内核
    sys_vtimer(rmp->mp_endpoint, num, nptr, optr);
}
```

**虚拟定时器到期后重启**：

```c
void check_vtimer(int proc_nr, int sig)
{
    rmp = &mproc[proc_nr];
    
    // 翻译信号到定时器类型
    switch (sig) {
        case SIGVTALRM: which = ITIMER_VIRTUAL; num = VT_VIRTUAL; break;
        case SIGPROF:   which = ITIMER_PROF;    num = VT_PROF;    break;
    }
    
    // 如果有间隔，重新设置
    if (rmp->mp_interval[which] > 0)
        sys_vtimer(rmp->mp_endpoint, num, &rmp->mp_interval[which], NULL);
}
```

---

### 3.3 时间管理流程

#### 3.3.1 时间来源

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           时间数据来源                                        │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   ┌────────────────┐                                                        │
│   │   硬件时钟     │  RTC/HPET                                              │
│   │   (RTC/HPET)  │                                                        │
│   └───────┬────────┘                                                        │
│           │ 时钟中断                                                        │
│           ▼                                                                 │
│   ┌────────────────┐                                                        │
│   │    Kernel      │                                                        │
│   │  kclockinfo    │                                                        │
│   │  - uptime      │  启动后的滴答数（单调）                                 │
│   │  - realtime    │  实时滴答数（可调整）                                   │
│   │  - boottime    │  启动时的 UNIX 时间戳                                   │
│   │  - hz          │  时钟频率                                              │
│   └───────┬────────┘                                                        │
│           │ getuptime() / clock_time()                                      │
│           ▼                                                                 │
│   ┌────────────────┐                                                        │
│   │      PM        │                                                        │
│   │  time.c        │                                                        │
│   └───────┬────────┘                                                        │
│           │ 系统调用                                                         │
│           ▼                                                                 │
│   ┌────────────────┐                                                        │
│   │   用户进程     │                                                        │
│   │  gettimeofday │                                                        │
│   │  clock_gettime│                                                        │
│   └────────────────┘                                                        │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### 3.3.2 do_gettime 流程

```c
int do_gettime(void)
```

**系统调用接口**：

```c
int clock_gettime(clockid_t clk_id, struct timespec *tp);
```

**流程**：

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        do_gettime 流程                                       │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   1. 获取内核时间信息                                                        │
│      getuptime(&ticks, &realtime, &boottime)                               │
│                                                                             │
│   2. 根据时钟类型选择                                                        │
│      switch (clk_id) {                                                      │
│          case CLOCK_REALTIME:                                               │
│              clock = realtime;                                              │
│          case CLOCK_MONOTONIC:                                              │
│              clock = ticks;                                                 │
│      }                                                                      │
│                                                                             │
│   3. 计算秒和纳秒                                                            │
│      sec = boottime + (clock / system_hz);                                 │
│      nsec = (clock % system_hz) * 1000000000 / system_hz;                  │
│                                                                             │
│   4. 返回结果                                                                │
│      mp->mp_reply.m_pm_lc_time.sec = sec;                                  │
│      mp->mp_reply.m_pm_lc_time.nsec = nsec;                                │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

**时间计算公式**：

```
当前时间 = boottime + (clock_ticks / system_hz)

秒数 = boottime + (clock / system_hz)
纳秒数 = (clock % system_hz) * 1000000000 / system_hz
```

#### 3.3.3 do_settime 流程

```c
int do_settime(void)
```

**系统调用接口**：

```c
int clock_settime(clockid_t clk_id, const struct timespec *tp);
```

**流程**：

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        do_settime 流程                                       │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   1. 权限检查                                                                │
│      if (mp->mp_effuid != SUPER_USER) return EPERM;                        │
│                                                                             │
│   2. 根据时钟类型处理                                                        │
│      switch (clk_id) {                                                      │
│          case CLOCK_REALTIME:                                               │
│              sys_settime(now, clk_id, sec, nsec);                          │
│          case CLOCK_MONOTONIC:                                              │
│              return EINVAL;  // 单调时钟不能设置                             │
│      }                                                                      │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### 3.3.4 do_stime 流程

```c
int do_stime(void)
```

**设计思路**：

设置系统时间需要计算新的 `boottime`：

```
系统时间 = boottime + uptime

新 boottime = 新时间 - uptime
```

**流程**：

```c
int do_stime(void)
{
    // 1. 权限检查
    if (mp->mp_effuid != SUPER_USER) return EPERM;
    
    // 2. 获取当前运行时间
    getuptime(&uptime, &realtime, &boottime);
    
    // 3. 计算新的 boottime
    boottime = m_in.m_lc_pm_time.sec - (realtime / system_hz);
    
    // 4. 通知内核
    sys_stime(boottime);
    
    return OK;
}
```

---

## 四、关键机制拆解

### 4.1 调度器委托机制

**mp_scheduler 字段含义**：

| 值 | 含义 | 说明 |
|-----|------|------|
| `SCHED_PROC_NR` | 用户态调度器 | 由 SCHED 服务管理调度 |
| `KERNEL` | 内核调度 | 由内核直接调度（如 INIT） |
| `NONE` | 无调度器 | 系统进程，由 RS 管理 |

**调度委托流程**：

```
用户进程创建 (fork)
       │
       ▼
PM: sched_start_user()
       │
       │ IPC 消息
       ▼
SCHED: sched_start() / sched_inherit()
       │
       │ 设置优先级、时间片
       ▼
进程可被调度运行
```

### 4.2 定时器分层管理

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         定时器管理分层                                        │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   ITIMER_REAL (实时定时器)                                                   │
│   ┌────────────────────────────────────────────────────────────────────┐   │
│   │  PM 管理                                                            │   │
│   │  - mp_timer: timer_t 结构                                          │   │
│   │  - mp_interval[ITIMER_REAL]: 周期                                   │   │
│   │  - mp_flags & ALARM_ON: 标志                                        │   │
│   │                                                                     │   │
│   │  到期处理: cause_sigalrm() → check_sig(SIGALRM)                     │   │
│   └────────────────────────────────────────────────────────────────────┘   │
│                                                                             │
│   ITIMER_VIRTUAL / ITIMER_PROF (虚拟定时器)                                  │
│   ┌────────────────────────────────────────────────────────────────────┐   │
│   │  Kernel 管理                                                        │   │
│   │  - sys_vtimer(): 内核调用                                           │   │
│   │  - mp_interval[which]: 周期（PM 存储）                               │   │
│   │                                                                     │   │
│   │  到期处理: 内核发送信号 → PM: check_vtimer() → 重启定时器            │   │
│   └────────────────────────────────────────────────────────────────────┘   │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 4.3 时间转换机制

**timeval → ticks**：

```c
clock_t ticks_from_timeval(struct timeval *tv)
{
    clock_t ticks;
    
    // 秒转滴答
    ticks = system_hz * (unsigned long) tv->tv_sec;
    
    // 检查溢出
    if ((ticks / system_hz) != (unsigned long)tv->tv_sec) {
        ticks = LONG_MAX;
    } else {
        // 微秒转滴答（向上取整）
        ticks += ((system_hz * (unsigned long)tv->tv_usec + (US-1)) / US);
    }
    
    // 限制最大值
    if (ticks > LONG_MAX) ticks = LONG_MAX;
    
    return ticks;
}
```

**ticks → timeval**：

```c
void timeval_from_ticks(struct timeval *tv, clock_t ticks)
{
    tv->tv_sec = (long) (ticks / system_hz);
    tv->tv_usec = (long) ((ticks % system_hz) * US / system_hz);
}
```

### 4.4 nice 值与优先级转换

```c
int nice_to_priority(int nice, unsigned *maxprio)
```

**转换关系**：

| nice 值 | 优先级 | 说明 |
|---------|--------|------|
| -20 | 0 | 最高优先级 |
| 0 | 7 | 默认优先级 |
| 19 | 15 | 最低优先级 |

**公式**：

```
priority = USER_Q + (nice + 20) * (MIN_USER_Q - MAX_USER_Q) / 40
```

---

## 五、边界条件与特殊分支

### 5.1 调度器不可用

```c
// sched_nice 中检查调度器
if (rmp->mp_scheduler == KERNEL || rmp->mp_scheduler == NONE)
    return (EINVAL);
```

**原因**：内核调度的进程不能通过 `nice()` 调整优先级

### 5.2 父进程是系统进程

```c
// sched_start_user 中处理
if (mproc[rmp->mp_parent].mp_flags & PRIV_PROC) {
    assert(mproc[rmp->mp_parent].mp_scheduler == NONE);
    inherit_from = INIT_PROC_NR;  // 从 INIT 继承
} else {
    inherit_from = mproc[rmp->mp_parent].mp_endpoint;
}
```

**原因**：系统进程的子进程需要从 INIT 继承调度参数

### 5.3 定时器溢出

```c
// ticks_from_timeval 中处理溢出
ticks = system_hz * (unsigned long) tv->tv_sec;
if ((ticks / system_hz) != (unsigned long)tv->tv_sec) {
    ticks = LONG_MAX;  // 溢出时设为最大值
}
```

### 5.4 时间设置权限

```c
// do_settime / do_stime 中检查
if (mp->mp_effuid != SUPER_USER) {
    return(EPERM);
}
```

### 5.5 单调时钟不可设置

```c
case CLOCK_MONOTONIC: /* monotonic cannot be changed */
    return EINVAL;
```

### 5.6 定时器到期时进程已退出

```c
// cause_sigalrm 中检查
if ((rmp->mp_flags & (IN_USE | EXITING)) != IN_USE) return;
if ((rmp->mp_flags & ALARM_ON) == 0) return;
```

---

## 六、模块交互关系

### 6.1 上游调用者

| 调用者 | 方式 | 说明 |
|--------|------|------|
| 用户进程 | 系统调用 | `nice()`, `setitimer()`, `gettimeofday()` 等 |
| PM 内部 | 函数调用 | `forkexit.c` 调用 `sched_start_user()` |
| 内核 | 信号/回调 | 定时器到期时调用 `cause_sigalrm()` |

### 6.2 下游依赖

| 服务 | 调用函数 | 说明 |
|------|----------|------|
| SCHED | `sched_start()`, `sched_inherit()` | 调度器服务 |
| Kernel | `sys_vtimer()`, `sys_settime()`, `sys_stime()` | 内核调用 |
| Kernel | `getuptime()`, `clock_time()` | 时间信息获取 |
| timers.c | `set_timer()`, `cancel_timer()` | 定时器库 |
| signal.c | `check_sig()` | 信号发送 |

### 6.3 IPC 消息类型

| 消息类型 | 方向 | 说明 |
|----------|------|------|
| `SCHEDULING_START` | PM → SCHED | 启动进程调度 |
| `SCHEDULING_INHERIT` | PM → SCHED | 继承调度参数 |
| `SCHEDULING_SET_NICE` | PM → SCHED | 设置 nice 值 |

---

## 七、Rust 重构与设计改进建议

### 7.1 类型系统改进

**当前问题**：调度器端点使用整数，容易混淆

```c
// C 代码：mp_scheduler 可能是 SCHED_PROC_NR、KERNEL、NONE
endpoint_t mp_scheduler;
```

**Rust 改进**：使用枚举

```rust
enum Scheduler {
    UserSpace(Endpoint),  // 用户态调度器
    Kernel,               // 内核调度
    None,                 // 无调度器
}

struct MProc {
    scheduler: Scheduler,
    // ...
}
```

### 7.2 定时器类型安全

**当前问题**：定时器类型用整数表示

```c
int which;  // 0, 1, 2 分别代表不同类型
```

**Rust 改进**：使用枚举

```rust
enum TimerType {
    Real,     // ITIMER_REAL
    Virtual,  // ITIMER_VIRTUAL
    Prof,     // ITIMER_PROF
}

enum ClockType {
    Realtime,   // CLOCK_REALTIME
    Monotonic,  // CLOCK_MONOTONIC
}
```

### 7.3 时间计算安全

**当前问题**：时间计算可能溢出

```c
ticks = system_hz * (unsigned long) tv->tv_sec;
if ((ticks / system_hz) != (unsigned long)tv->tv_sec) {
    ticks = LONG_MAX;
}
```

**Rust 改进**：使用 checked 算术

```rust
fn ticks_from_timeval(tv: &Timeval, hz: u32) -> Result<ClockTicks, OverflowError> {
    let sec_ticks = hz.checked_mul(tv.tv_sec as u32)
        .ok_or(OverflowError)?;
    let usec_ticks = (hz as u64 * tv.tv_usec as u64 / 1_000_000) as u32;
    sec_ticks.checked_add(usec_ticks)
        .ok_or(OverflowError)
}
```

### 7.4 当前设计问题总结

| 问题 | 代码位置 | 说明 |
|------|----------|------|
| 调度器类型混淆 | `mp_scheduler` | 使用整数表示多种含义 |
| 定时器类型不安全 | `which` 参数 | 整数而非枚举 |
| 时间溢出处理复杂 | `ticks_from_timeval` | 需要手动检查 |
| 优先级转换隐式 | `nice_to_priority` | 公式不直观 |
| 虚拟定时器状态分散 | PM 和 Kernel | 状态在两处维护 |

### 7.5 改进方向

#### 7.5.1 统一时间类型

```rust
struct Duration {
    secs: i64,
    nanos: u32,
}

impl Duration {
    fn from_ticks(ticks: u64, hz: u32) -> Self {
        Duration {
            secs: (ticks / hz as u64) as i64,
            nanos: ((ticks % hz as u64) * 1_000_000_000 / hz as u64) as u32,
        }
    }
    
    fn to_ticks(&self, hz: u32) -> Option<u64> {
        // 使用 checked 算术
    }
}
```

#### 7.5.2 定时器抽象

```rust
trait Timer {
    fn set(&mut self, duration: Duration, interval: Duration) -> Result<(), TimerError>;
    fn cancel(&mut self);
    fn remaining(&self) -> Option<Duration>;
}

struct RealTimer {
    timer: TimerStruct,
    interval: Duration,
    flags: TimerFlags,
}

struct VirtualTimer {
    kernel_timer: KernelTimer,
    interval: Duration,
}
```

#### 7.5.3 调度器抽象

```rust
trait Scheduler {
    fn start_process(&self, proc: &Process, priority: Priority, quantum: Quantum) -> Result<(), SchedError>;
    fn set_priority(&self, proc: &Process, priority: Priority) -> Result<(), SchedError>;
    fn stop_process(&self, proc: &Process) -> Result<(), SchedError>;
}

struct UserSpaceScheduler {
    endpoint: Endpoint,
}

struct KernelScheduler;

impl Scheduler for UserSpaceScheduler {
    // 通过 IPC 与 SCHED 服务通信
}

impl Scheduler for KernelScheduler {
    // 直接调用内核接口
}
```

---

## 八、总结

### 8.1 核心知识点

1. **调度委托**：PM 将调度决策委托给 SCHED 服务，通过 IPC 通信
2. **定时器分层**：实时定时器由 PM 管理，虚拟定时器由内核管理
3. **时间来源**：所有时间信息来自内核的 `kclockinfo` 结构
4. **nice 与优先级**：nice 值通过转换映射到调度优先级

### 8.2 关键设计决策

| 决策 | 原因 |
|------|------|
| 调度器独立服务 | 微内核设计，调度策略可替换 |
| 实时定时器在 PM | 需要发送信号，PM 负责信号处理 |
| 虚拟定时器在内核 | 需要统计进程运行时间 |
| 单调时钟不可设置 | 保证时间单调递增，用于测量间隔 |

### 8.3 设计张力点

1. **调度委托开销**：每次调度操作需要 IPC 通信
2. **定时器状态分散**：实时和虚拟定时器管理方式不同
3. **时间精度限制**：精度受时钟频率限制（通常 10ms）
4. **优先级转换复杂**：nice 值到优先级的映射不直观
