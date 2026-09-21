# servers/pm/proto.h 逐行讲解

> **文件路径**: `minix3/minix/servers/pm/proto.h`
> **核心功能**: PM（进程管理器）函数原型声明
> **所属模块**: PM（Process Manager）

---

## 一、文件概述

### 1.1 功能说明（是什么）

`proto.h` 集中声明 PM 所有函数的原型，按源文件组织。这是 PM 模块的接口定义文件，定义了所有公开的函数签名。

**生活类比**：想象一个餐厅的菜单：
- 菜单列出了所有可点的菜品（函数）。
- 每道菜有名称（函数名）和配料说明（参数和返回值）。
- 顾客（调用者）不需要知道厨房（实现）的细节。
- `proto.h` 就是 PM 这个"餐厅"的"菜单"。

### 1.2 设计原因（为什么）

**为什么需要集中声明函数原型？**

1. **编译检查**：编译器可以检查函数调用是否正确。
2. **文档作用**：开发者可以快速了解 PM 提供的接口。
3. **模块化**：每个 `.c` 文件只需要包含 `proto.h` 就能调用其他模块的函数。
4. **避免重复**：不需要在每个 `.c` 文件中重复声明。

**函数原型组织方式**：

```
proto.h 组织结构:
┌─────────────────────────────────────────────────────────────┐
│ 按源文件分组                                                  │
│ ├── alarm.c    - 定时器函数                                  │
│ ├── event.c    - 事件处理函数                                │
│ ├── exec.c     - 执行函数                                    │
│ ├── forkexit.c - fork/exit 函数                              │
│ ├── getset.c   - get/set 函数                                │
│ ├── main.c     - 主函数                                      │
│ ├── mcontext.c - 机器上下文函数                              │
│ ├── misc.c     - 杂项函数                                    │
│ ├── schedule.c - 调度函数                                    │
│ ├── profile.c  - 性能分析函数                                │
│ ├── signal.c   - 信号处理函数                                │
│ ├── time.c     - 时间函数                                    │
│ ├── trace.c    - 跟踪函数                                    │
│ └── utility.c  - 工具函数                                    │
└─────────────────────────────────────────────────────────────┘
```

### 1.3 应用场景（什么情景使用）

| 场景 | 说明 |
|------|------|
| 添加新函数 | 在对应模块分组中添加函数原型 |
| 调用函数 | 包含 `proto.h` 后调用函数 |
| 代码审查 | 检查函数签名是否正确 |

---

## 二、逐行详细讲解

### 2.1 文件头和前向声明

```c
/* Function prototypes. */

struct mproc;

#include <minix/timers.h>
```

**逐行解释**：

- **第1行**：`/* Function prototypes. */`
  - 注释说明这是函数原型文件。

- **第3行**：`struct mproc;`
  - 前向声明 `struct mproc`。
  - 避免循环包含 `mproc.h`。
  - 函数原型只需要指针类型，不需要完整定义。

- **第5行**：`#include <minix/timers.h>`
  - 包含定时器类型定义。
  - 提供 `clock_t` 类型，用于定时器相关函数。

**前向声明的作用**：

```
前向声明 vs 完整包含:
┌─────────────────────────────────────────────────────────────┐
│ 前向声明 (struct mproc;)                                     │
│ - 只声明结构体存在                                           │
│ - 可以定义指向该结构体的指针                                 │
│ - 不需要知道结构体大小                                       │
│ - 编译更快，依赖更少                                         │
├─────────────────────────────────────────────────────────────┤
│ 完整包含 (#include "mproc.h")                                │
│ - 包含完整结构体定义                                         │
│ - 可以访问结构体成员                                         │
│ - 可以定义结构体变量                                         │
│ - 编译更慢，依赖更多                                         │
└─────────────────────────────────────────────────────────────┘
```

---

### 2.2 alarm.c 函数原型

```c
/* alarm.c */
int do_itimer(void);
void set_alarm(struct mproc *rmp, clock_t ticks);
void check_vtimer(int proc_nr, int sig);
```

**逐行解释**：

- **第1行**：`/* alarm.c */`
  - 注释说明以下函数来自 `alarm.c`。

- **第2行**：`int do_itimer(void);`
  - 处理 `setitimer()` 系统调用。
  - 返回值：成功返回 0，失败返回错误码。
  - 功能：设置间隔定时器（ITIMER_REAL、ITIMER_VIRTUAL、ITIMER_PROF）。

- **第3行**：`void set_alarm(struct mproc *rmp, clock_t ticks);`
  - 为进程设置闹钟。
  - 参数 `rmp`：指向目标进程的指针。
  - 参数 `ticks`：闹钟时间（时钟滴答数）。
  - 功能：实现 `alarm()` 系统调用。

- **第4行**：`void check_vtimer(int proc_nr, int sig);`
  - 检查虚拟定时器。
  - 参数 `proc_nr`：进程编号。
  - 参数 `sig`：信号编号。
  - 功能：检查虚拟定时器是否到期，发送信号。

---

### 2.3 event.c 函数原型

```c
/* event.c */
int do_proceventmask(void);
int do_proc_event_reply(void);
void publish_event(struct mproc *rmp);
```

**逐行解释**：

- **第1行**：`/* event.c */`
  - 注释说明以下函数来自 `event.c`。

- **第2行**：`int do_proceventmask(void);`
  - 处理进程事件掩码系统调用。
  - 功能：设置进程事件订阅掩码。

- **第3行**：`int do_proc_event_reply(void);`
  - 处理进程事件回复。
  - 功能：回复进程事件通知。

- **第4行**：`void publish_event(struct mproc *rmp);`
  - 发布进程事件。
  - 参数 `rmp`：指向进程的指针。
  - 功能：向订阅者发布进程事件（如创建、退出）。

---

### 2.4 exec.c 函数原型

```c
/* exec.c */
int do_exec(void);
int do_newexec(void);
int do_execrestart(void);
void exec_restart(struct mproc *rmp, int result, vir_bytes pc, vir_bytes sp,
	vir_bytes ps_str);
```

**逐行解释**：

- **第1行**：`/* exec.c */`
  - 注释说明以下函数来自 `exec.c`。

- **第2行**：`int do_exec(void);`
  - 处理 `execve()` 系统调用。
  - 功能：执行新程序，替换当前进程映像。

- **第3行**：`int do_newexec(void);`
  - 处理新版 exec 系统调用。
  - 功能：支持新的 exec 功能。

- **第4行**：`int do_execrestart(void);`
  - exec 重启处理。
  - 功能：处理 exec 过程中的重启。

- **第5-6行**：`void exec_restart(struct mproc *rmp, int result, vir_bytes pc, vir_bytes sp, vir_bytes ps_str);`
  - exec 重启的具体实现。
  - 参数 `rmp`：指向进程的指针。
  - 参数 `result`：exec 结果。
  - 参数 `pc`：程序计数器。
  - 参数 `sp`：栈指针。
  - 参数 `ps_str`：进程状态字符串。

---

### 2.5 forkexit.c 函数原型

```c
/* forkexit.c */
int do_fork(void);
int do_srv_fork(void);
int do_exit(void);
void exit_proc(struct mproc *rmp, int exit_status, int dump_core);
void exit_restart(struct mproc *rmp);
int do_wait4(void);
int wait_test(struct mproc *rmp, struct mproc *child);
```

**逐行解释**：

- **第1行**：`/* forkexit.c */`
  - 注释说明以下函数来自 `forkexit.c`。

- **第2行**：`int do_fork(void);`
  - 处理 `fork()` 系统调用。
  - 返回值：子进程返回 0，父进程返回子进程 PID。
  - 功能：创建子进程，复制父进程的地址空间。

- **第3行**：`int do_srv_fork(void);`
  - 处理服务器 fork 系统调用。
  - 功能：为系统服务创建子进程。

- **第4行**：`int do_exit(void);`
  - 处理 `exit()` 系统调用。
  - 功能：终止当前进程。

- **第5行**：`void exit_proc(struct mproc *rmp, int exit_status, int dump_core);`
  - 进程退出的具体实现。
  - 参数 `rmp`：指向进程的指针。
  - 参数 `exit_status`：退出状态码。
  - 参数 `dump_core`：是否生成 core dump。

- **第6行**：`void exit_restart(struct mproc *rmp);`
  - 退出重启处理。
  - 功能：处理退出过程中的重启。

- **第7行**：`int do_wait4(void);`
  - 处理 `wait4()` 系统调用。
  - 功能：等待子进程退出，获取退出状态。

- **第8行**：`int wait_test(struct mproc *rmp, struct mproc *child);`
  - 测试子进程状态。
  - 参数 `rmp`：父进程指针。
  - 参数 `child`：子进程指针。
  - 功能：检查子进程是否满足等待条件。

**fork/exit/wait 关系**：

```
进程生命周期:
┌─────────────────────────────────────────────────────────────┐
│                                                              │
│   fork() ──→ 子进程创建                                      │
│      │                                                       │
│      ├──→ 子进程执行                                         │
│      │       │                                               │
│      │       └──→ exit() ──→ 僵尸状态                        │
│      │                           │                           │
│      └──→ 父进程 wait4() ←──────┘                           │
│                   │                                          │
│                   └──→ 回收子进程资源                        │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

---

### 2.6 getset.c 函数原型

```c
/* getset.c */
int do_get(void);
int do_set(void);
```

**逐行解释**：

- **第1行**：`/* getset.c */`
  - 注释说明以下函数来自 `getset.c`。

- **第2行**：`int do_get(void);`
  - 处理 get 类系统调用。
  - 功能：获取进程属性（`getpid`、`getuid`、`getgid` 等）。

- **第3行**：`int do_set(void);`
  - 处理 set 类系统调用。
  - 功能：设置进程属性（`setuid`、`setgid`、`setpgid` 等）。

---

### 2.7 main.c 函数原型

```c
/* main.c */
int main(void);
void reply(int proc_nr, int result);
```

**逐行解释**：

- **第1行**：`/* main.c */`
  - 注释说明以下函数来自 `main.c`。

- **第2行**：`int main(void);`
  - PM 主函数。
  - 功能：初始化 PM，进入主循环处理系统调用。

- **第3行**：`void reply(int proc_nr, int result);`
  - 向进程发送回复消息。
  - 参数 `proc_nr`：进程编号。
  - 参数 `result`：系统调用结果。

---

### 2.8 mcontext.c 函数原型

```c
/* mcontext.c */
int do_getmcontext(void);
int do_setmcontext(void);
```

**逐行解释**：

- **第1行**：`/* mcontext.c */`
  - 注释说明以下函数来自 `mcontext.c`。

- **第2行**：`int do_getmcontext(void);`
  - 获取机器上下文。
  - 功能：保存进程的寄存器状态（用于信号处理）。

- **第3行**：`int do_setmcontext(void);`
  - 设置机器上下文。
  - 功能：恢复进程的寄存器状态。

---

### 2.9 misc.c 函数原型

```c
/* misc.c */
int do_reboot(void);
int do_sysuname(void);
int do_getsysinfo(void);
int do_getprocnr(void);
int do_getepinfo(void);
int do_svrctl(void);
int do_getsetpriority(void);
int do_getrusage(void);
```

**逐行解释**：

- **第1行**：`/* misc.c */`
  - 注释说明以下函数来自 `misc.c`。

- **第2行**：`int do_reboot(void);`
  - 处理 `reboot()` 系统调用。
  - 功能：重启或关机。

- **第3行**：`int do_sysuname(void);`
  - 处理 `uname()` 系统调用。
  - 功能：获取系统信息。

- **第4行**：`int do_getsysinfo(void);`
  - 获取系统信息。
  - 功能：返回系统配置信息。

- **第5行**：`int do_getprocnr(void);`
  - 获取进程编号。
  - 功能：根据 PID 获取进程槽位编号。

- **第6行**：`int do_getepinfo(void);`
  - 获取端点信息。
  - 功能：获取进程端点信息。

- **第7行**：`int do_svrctl(void);`
  - 服务器控制。
  - 功能：控制服务器行为。

- **第8行**：`int do_getsetpriority(void);`
  - 获取/设置优先级。
  - 功能：`getpriority()` 和 `setpriority()`。

- **第9行**：`int do_getrusage(void);`
  - 获取资源使用情况。
  - 功能：`getrusage()` 系统调用。

---

### 2.10 schedule.c 函数原型

```c
/* schedule.c */
void sched_init(void);
int sched_start_user(endpoint_t ep, struct mproc *rmp);
int sched_nice(struct mproc *rmp, int nice);
```

**逐行解释**：

- **第1行**：`/* schedule.c */`
  - 注释说明以下函数来自 `schedule.c`。

- **第2行**：`void sched_init(void);`
  - 初始化调度器。
  - 功能：与用户态调度器建立连接。

- **第3行**：`int sched_start_user(endpoint_t ep, struct mproc *rmp);`
  - 启动用户进程调度。
  - 参数 `ep`：调度器端点。
  - 参数 `rmp`：进程指针。

- **第4行**：`int sched_nice(struct mproc *rmp, int nice);`
  - 设置 nice 值。
  - 参数 `rmp`：进程指针。
  - 参数 `nice`：nice 值（-20 到 19）。

---

### 2.11 profile.c 函数原型

```c
/* profile.c */
int do_sprofile(void);
```

**逐行解释**：

- **第1行**：`/* profile.c */`
  - 注释说明以下函数来自 `profile.c`。

- **第2行**：`int do_sprofile(void);`
  - 处理性能分析系统调用。
  - 功能：启动/停止统计性能分析。

---

### 2.12 signal.c 函数原型

```c
/* signal.c */
int do_kill(void);
int do_srv_kill(void);
int process_ksig(endpoint_t proc_nr_e, int signo);
int check_sig(pid_t proc_id, int signo, int ksig);
void sig_proc(struct mproc *rmp, int signo, int trace, int ksig);
int do_sigaction(void);
int do_sigpending(void);
int do_sigprocmask(void);
int do_sigreturn(void);
int do_sigsuspend(void);
void check_pending(struct mproc *rmp);
void restart_sigs(struct mproc *rmp);
```

**逐行解释**：

- **第1行**：`/* signal.c */`
  - 注释说明以下函数来自 `signal.c`。

- **第2行**：`int do_kill(void);`
  - 处理 `kill()` 系统调用。
  - 功能：向进程发送信号。

- **第3行**：`int do_srv_kill(void);`
  - 服务器 kill。
  - 功能：服务器进程发送信号。

- **第4行**：`int process_ksig(endpoint_t proc_nr_e, int signo);`
  - 处理内核信号。
  - 参数 `proc_nr_e`：进程端点。
  - 参数 `signo`：信号编号。

- **第5行**：`int check_sig(pid_t proc_id, int signo, int ksig);`
  - 检查信号权限。
  - 参数 `proc_id`：进程 ID。
  - 参数 `signo`：信号编号。
  - 参数 `ksig`：是否来自内核。

- **第6行**：`void sig_proc(struct mproc *rmp, int signo, int trace, int ksig);`
  - 向进程发送信号。
  - 参数 `rmp`：进程指针。
  - 参数 `signo`：信号编号。
  - 参数 `trace`：是否跟踪。
  - 参数 `ksig`：是否来自内核。

- **第7行**：`int do_sigaction(void);`
  - 处理 `sigaction()` 系统调用。
  - 功能：设置信号处理动作。

- **第8行**：`int do_sigpending(void);`
  - 处理 `sigpending()` 系统调用。
  - 功能：获取待处理信号集。

- **第9行**：`int do_sigprocmask(void);`
  - 处理 `sigprocmask()` 系统调用。
  - 功能：设置/获取信号掩码。

- **第10行**：`int do_sigreturn(void);`
  - 处理 `sigreturn()` 系统调用。
  - 功能：从信号处理函数返回。

- **第11行**：`int do_sigsuspend(void);`
  - 处理 `sigsuspend()` 系统调用。
  - 功能：挂起进程等待信号。

- **第12行**：`void check_pending(struct mproc *rmp);`
  - 检查待处理信号。
  - 功能：检查并处理待处理的信号。

- **第13行**：`void restart_sigs(struct mproc *rmp);`
  - 重启信号处理。
  - 功能：重启被中断的系统调用。

---

### 2.13 time.c 函数原型

```c
/* time.c */
int do_stime(void);
int do_time(void);
int do_getres(void);
int do_gettime(void);
int do_settime(void);
```

**逐行解释**：

- **第1行**：`/* time.c */`
  - 注释说明以下函数来自 `time.c`。

- **第2行**：`int do_stime(void);`
  - 处理 `stime()` 系统调用。
  - 功能：设置系统时间。

- **第3行**：`int do_time(void);`
  - 处理 `time()` 系统调用。
  - 功能：获取当前时间。

- **第4行**：`int do_getres(void);`
  - 获取时钟分辨率。
  - 功能：`clock_getres()` 系统调用。

- **第5行**：`int do_gettime(void);`
  - 获取时间。
  - 功能：`clock_gettime()` 系统调用。

- **第6行**：`int do_settime(void);`
  - 设置时间。
  - 功能：`clock_settime()` 系统调用。

---

### 2.14 trace.c 函数原型

```c
/* trace.c */
int do_trace(void);
void trace_stop(struct mproc *rmp, int signo);
```

**逐行解释**：

- **第1行**：`/* trace.c */`
  - 注释说明以下函数来自 `trace.c`。

- **第2行**：`int do_trace(void);`
  - 处理 `ptrace()` 系统调用。
  - 功能：进程跟踪和调试。

- **第3行**：`void trace_stop(struct mproc *rmp, int signo);`
  - 跟踪停止。
  - 参数 `rmp`：进程指针。
  - 参数 `signo`：信号编号。
  - 功能：停止被跟踪的进程。

---

### 2.15 utility.c 函数原型

```c
/* utility.c */
pid_t get_free_pid(void);
char *find_param(const char *key);
struct mproc *find_proc(pid_t lpid);
int nice_to_priority(int nice, unsigned *new_q);
int pm_isokendpt(int ep, int *proc);
void tell_vfs(struct mproc *rmp, message *m_ptr);
void set_rusage_times(struct rusage *r_usage, clock_t user_time,
	clock_t sys_time);
```

**逐行解释**：

- **第1行**：`/* utility.c */`
  - 注释说明以下函数来自 `utility.c`。

- **第2行**：`pid_t get_free_pid(void);`
  - 获取空闲 PID。
  - 返回值：可用的 PID。
  - 功能：分配新的进程 ID。

- **第3行**：`char *find_param(const char *key);`
  - 查找启动参数。
  - 参数 `key`：参数名。
  - 返回值：参数值字符串。
  - 功能：从启动参数中查找指定参数。

- **第4行**：`struct mproc *find_proc(pid_t lpid);`
  - 根据 PID 查找进程。
  - 参数 `lpid`：进程 ID。
  - 返回值：进程指针，未找到返回 NULL。

- **第5行**：`int nice_to_priority(int nice, unsigned *new_q);`
  - nice 值转优先级。
  - 参数 `nice`：nice 值。
  - 参数 `new_q`：输出优先级队列。
  - 返回值：成功返回 OK。

- **第6行**：`int pm_isokendpt(int ep, int *proc);`
  - 验证端点有效性。
  - 参数 `ep`：端点 ID。
  - 参数 `proc`：输出进程编号。
  - 返回值：有效返回 OK。

- **第7行**：`void tell_vfs(struct mproc *rmp, message *m_ptr);`
  - 通知 VFS。
  - 参数 `rmp`：进程指针。
  - 参数 `m_ptr`：消息指针。
  - 功能：向 VFS 发送消息。

- **第8-9行**：`void set_rusage_times(struct rusage *r_usage, clock_t user_time, clock_t sys_time);`
  - 设置资源使用时间。
  - 参数 `r_usage`：资源使用结构。
  - 参数 `user_time`：用户时间。
  - 参数 `sys_time`：系统时间。

---

## 三、理论关联

### 3.1 系统调用处理

PM 处理大部分 POSIX 进程管理相关的系统调用：

```
PM 系统调用分类:
┌─────────────────────────────────────────────────────────────┐
│ 进程创建/终止                                                │
│ - fork(): do_fork()                                         │
│ - exit(): do_exit()                                         │
│ - wait4(): do_wait4()                                       │
├─────────────────────────────────────────────────────────────┤
│ 程序执行                                                     │
│ - execve(): do_exec()                                       │
├─────────────────────────────────────────────────────────────┤
│ 进程属性                                                     │
│ - getpid/getuid/getgid: do_get()                            │
│ - setuid/setgid/setpgid: do_set()                           │
├─────────────────────────────────────────────────────────────┤
│ 信号处理                                                     │
│ - kill(): do_kill()                                         │
│ - sigaction(): do_sigaction()                               │
│ - sigprocmask(): do_sigprocmask()                           │
├─────────────────────────────────────────────────────────────┤
│ 时间管理                                                     │
│ - time(): do_time()                                         │
│ - stime(): do_stime()                                       │
└─────────────────────────────────────────────────────────────┘
```

### 3.2 微内核 IPC

PM 通过 IPC 与其他组件通信：

```
PM IPC 通信:
┌─────────────────────────────────────────────────────────────┐
│ 用户进程 ──IPC──→ PM ──IPC──→ 内核                          │
│                    │                                         │
│                    └──IPC──→ VFS                             │
│                    │                                         │
│                    └──IPC──→ VM                              │
│                    │                                         │
│                    └──IPC──→ 调度器                          │
└─────────────────────────────────────────────────────────────┘
```

---

## 四、Rust 实现与对比

### 4.1 函数原型对比

**C 语言版本**：
```c
int do_fork(void);
int do_exit(void);
void exit_proc(struct mproc *rmp, int exit_status, int dump_core);
int do_wait4(void);
```

**Rust 版本**：
```rust
#![no_std]

use core::result::Result;

pub type Pid = i32;
pub type ExitStatus = i32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PmError {
    InvalidPid,
    NoChildProcess,
    NotPermitted,
    InsufficientMemory,
    InvalidArgument,
}

#[derive(Debug, Clone, Copy)]
pub struct WaitStatus {
    pub pid: Pid,
    pub status: ExitStatus,
}

pub struct MProc;

pub trait ProcessManager {
    fn do_fork(&mut self) -> Result<Pid, PmError>;
    fn do_exit(&mut self, exit_status: ExitStatus) -> Result<!, PmError>;
    fn exit_proc(&mut self, proc: &mut MProc, exit_status: ExitStatus, dump_core: bool);
    fn do_wait4(&mut self, pid: Pid, options: i32) -> Result<WaitStatus, PmError>;
}

pub struct PmServer {
    procs: [Option<MProc>; NR_PROCS],
}

impl ProcessManager for PmServer {
    fn do_fork(&mut self) -> Result<Pid, PmError> {
        let slot = self.alloc_slot()?;
        let child_pid = self.get_free_pid()?;
        
        // 复制父进程状态
        // ...
        
        Ok(child_pid)
    }

    fn do_exit(&mut self, exit_status: ExitStatus) -> Result<!, PmError> {
        // 进程退出
        // ...
        loop { core::hint::spin_loop(); }
    }

    fn exit_proc(&mut self, proc: &mut MProc, exit_status: ExitStatus, dump_core: bool) {
        proc.exit_status = exit_status as i8;
        proc.flags.set_zombie();
        
        if dump_core {
            // 生成 core dump
        }
        
        // 通知父进程
    }

    fn do_wait4(&mut self, pid: Pid, options: i32) -> Result<WaitStatus, PmError> {
        // 查找子进程
        // 检查状态
        // 返回结果
        Ok(WaitStatus { pid: 0, status: 0 })
    }
}
```

### 4.2 Rust 优势分析

| 方面 | C 语言 | Rust |
|------|--------|------|
| 错误处理 | 返回整数错误码 | `Result<T, E>` 类型 |
| 空指针 | 可能导致崩溃 | `Option<T>` 强制处理 |
| 类型安全 | 弱类型检查 | 强类型检查 |
| 文档 | 注释 | `///` 文档注释 |
| 可见性 | 全局可见 | `pub` 控制可见性 |

---

## 五、要点总结

### 5.1 核心知识点

1. **函数原型集中声明**：`proto.h` 集中声明所有 PM 函数原型。

2. **按源文件组织**：函数原型按对应的 `.c` 文件分组。

3. **前向声明**：使用 `struct mproc;` 前向声明避免循环依赖。

### 5.2 设计亮点

- **模块化组织**：每个源文件对应一组函数原型。
- **编译检查**：编译器可以检查函数调用是否正确。
- **文档作用**：开发者可以快速了解 PM 提供的接口。

---

## 六、灾难预演

### 6.1 如果删除 do_fork 声明

**后果**：编译警告或错误。

**现象**：
- 编译器警告：隐式声明函数。
- 如果函数调用不匹配，可能导致运行时错误。
- 难以追踪函数签名变化。

### 6.2 如果函数签名不匹配

**后果**：运行时崩溃。

**现象**：
- 参数传递错误。
- 栈不平衡。
- 数据损坏。

### 6.3 如果忘记前向声明

**后果**：编译错误。

**现象**：
- 编译器不知道 `struct mproc` 的大小。
- 无法定义指向该结构体的指针。
- 需要包含完整的 `mproc.h`。

---

## 七、互动自测

### 问题 1：为什么需要前向声明 `struct mproc;`？

<details>
<summary>点击查看答案</summary>

需要前向声明的原因：

1. **避免循环依赖**：
   - `proto.h` 被 `mproc.h` 包含。
   - 如果 `proto.h` 包含 `mproc.h`，会产生循环包含。

2. **编译效率**：
   - 前向声明只需要声明结构体存在。
   - 不需要包含完整的结构体定义。
   - 编译更快。

3. **指针类型**：
   - 函数原型只需要指针类型。
   - 不需要知道结构体大小。
   - 前向声明足够。

**示例**：
```c
// 前向声明
struct mproc;

// 函数原型只需要指针
void set_alarm(struct mproc *rmp, clock_t ticks);

// 完整定义在 mproc.h 中
struct mproc {
    // ...
};
```
</details>

### 问题 2：PM 的主要系统调用有哪些？

<details>
<summary>点击查看答案</summary>

PM 的主要系统调用：

1. **进程管理**：
   - `fork()`: 创建子进程
   - `exit()`: 进程退出
   - `wait4()`: 等待子进程
   - `execve()`: 执行新程序

2. **进程属性**：
   - `getpid()`, `getppid()`: 获取进程 ID
   - `getuid()`, `getgid()`: 获取用户/组 ID
   - `setuid()`, `setgid()`: 设置用户/组 ID

3. **信号处理**：
   - `kill()`: 发送信号
   - `sigaction()`: 设置信号处理
   - `sigprocmask()`: 设置信号掩码
   - `sigpending()`: 获取待处理信号

4. **时间管理**：
   - `time()`: 获取当前时间
   - `stime()`: 设置系统时间

5. **其他**：
   - `reboot()`: 重启系统
   - `ptrace()`: 进程跟踪
</details>

### 问题 3：`do_` 前缀的函数有什么特殊含义？

<details>
<summary>点击查看答案</summary>

`do_` 前缀的函数是系统调用处理函数：

1. **命名约定**：
   - `do_fork()` 处理 `fork()` 系统调用。
   - `do_exit()` 处理 `exit()` 系统调用。
   - `do_kill()` 处理 `kill()` 系统调用。

2. **调用方式**：
   - 用户进程通过 IPC 发送消息给 PM。
   - PM 主循环接收消息。
   - 根据消息类型调用对应的 `do_*` 函数。

3. **返回值**：
   - 返回 `int` 类型。
   - 成功返回 0 或正值。
   - 失败返回负的错误码。

4. **参数获取**：
   - 参数从消息中提取。
   - 使用全局变量 `mp` 指向调用者进程。

**示例**：
```c
// 用户调用
pid_t child = fork();

// PM 处理
int do_fork(void) {
    struct mproc *rmp = mp;  // 调用者进程
    // 创建子进程...
    return child_pid;  // 返回给父进程
}
```
</details>
