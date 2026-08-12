# 04-sched-quantum: 时间片、优先级与队列平衡

> 本文档分析 `minix3/minix/servers/sched/schedule.c` 中的时间片管理、优先级调整和队列平衡机制。

---

## 1. 概述

### 1.1 调度策略概述

- TODO: 说明 Minix3 SCHED 采用优先级 + 时间片轮转调度策略
- TODO: 说明与 fork 的关系——fork 时子进程继承父进程的优先级和时间片
- TODO: 说明 fork 后子进程独立参与调度，时间片用完时独立降级

---

## 2. 优先级系统

### 2.1 优先级层级

- TODO: 分析 `NR_SCHED_QUEUES` — 调度队列总数
- TODO: 分析 `MIN_USER_Q` — 用户进程最低优先级
- TODO: 分析优先级数值与优先级高低的关系（数值越小优先级越高）

### 2.2 max_priority vs priority

- TODO: 分析 `max_priority` 与 `priority` 的关系
- TODO: 说明 max_priority 是上限，priority 可以在运行中降低
- TODO: 说明 fork 时 max_priority 从 PM 消息获取，priority 从父进程继承

### 2.3 is_system_proc()

- TODO: 分析 `#define is_system_proc(p) ((p)->parent == RS_PROC_NR)` 宏
- TODO: 说明系统进程的优先级管理策略与用户进程不同

---

## 3. 时间片管理

### 3.1 DEFAULT_USER_TIME_SLICE

- TODO: 分析 `#define DEFAULT_USER_TIME_SLICE 200` — 默认用户进程时间片
- TODO: 说明时间片单位是系统时钟 ticks

### 3.2 时间片继承

- TODO: 说明 fork 时子进程继承父进程的 time_slice
- TODO: 说明 exec 后 SCHEDULING_START 可能设置不同的时间片

### 3.3 时间片用完处理

- TODO: 分析 `do_noquantum()` 函数——内核通知 SCHED 进程时间片用完
- TODO: 说明处理逻辑：
  - 如果 priority > MIN_USER_Q：降低优先级
  - 重新 schedule_process()
- TODO: 说明与 fork 的间接关系——fork 后子进程时间片用完时独立处理

---

## 4. 队列平衡

### 4.1 balance_timeout

- TODO: 分析 `#define BALANCE_TIMEOUT 5` — 每 5 秒平衡一次队列
- TODO: 说明通过 `sys_setalarm()` 设置定时器

### 4.2 balance_queues()

- TODO: 分析 `balance_queues()` 函数——定期恢复被降低的优先级
- TODO: 说明遍历所有 schedproc，将被 do_noquantum 降低的优先级恢复
- TODO: 说明恢复策略：逐步提升，不是一次性恢复到 max_priority

### 4.3 init_scheduling()

- TODO: 分析 `init_scheduling()` 函数——初始化调度系统
- TODO: 说明设置 balance_queues 定时器
- TODO: 说明获取系统时钟频率

---

## 5. CPU 管理 (SMP)

### 5.1 cpu_proc[] 数组

- TODO: 分析 `cpu_proc[CONFIG_MAX_CPUS]` 数组——追踪每个 CPU 的进程数
- TODO: 说明 fork 时 pick_cpu() 递增目标 CPU 计数
- TODO: 说明 exit 时 do_stop_scheduling() 递减

### 5.2 cpu_is_available()

- TODO: 分析 `#define cpu_is_available(c) (cpu_proc[c] >= 0)` 宏
- TODO: 说明 CPU_DEAD (-1) 标记不可用 CPU

### 5.3 pick_cpu() 策略

- TODO: 分析 CPU 选择策略：
  - 系统进程 → BSP (CPU 0)
  - 用户进程 → 负载最低的可用 CPU
- TODO: 说明 CPU 负载均衡的简单策略

---

## 6. 进程迁移

### 6.1 schedule_process_local()

- TODO: 分析 `#define schedule_process_local(p) schedule_process(p, SCHEDULE_CHANGE_PRIO | SCHEDULE_CHANGE_QUANTUM)` 宏
- TODO: 说明本地调度更新（不改变 CPU）

### 6.2 schedule_process_migrate()

- TODO: 分析 `#define schedule_process_migrate(p) schedule_process(p, SCHEDULE_CHANGE_CPU)` 宏
- TODO: 说明 CPU 迁移更新

---

## 7. do_stop_scheduling() — fork 的逆操作

### 7.1 函数实现

- TODO: 分析 `do_stop_scheduling()` 函数——进程退出时停止调度
- TODO: 说明核心操作：
  - 清除 flags (清除 IN_USE)
  - 递减 cpu_proc[] 计数
  - 不需要调用 sys_schedule()（进程已不存在）

### 7.2 与 fork 的对称性

| 操作 | fork (do_start_scheduling) | exit (do_stop_scheduling) |
|------|---------------------------|--------------------------|
| flags | 设为 IN_USE | 清除 IN_USE |
| cpu_proc[] | 递增 | 递减 |
| sys_schedctl() | 调用（接管） | 不调用（进程已死） |
| schedule_process() | 调用（设置参数） | 不调用 |

---

## 8. do_nice() — 优先级修改

### 8.1 nice 系统调用

- TODO: 分析 `do_nice()` 函数——修改进程优先级
- TODO: 说明与 fork 的关系——fork 后 nice 值独立于父进程

---

## 9. 调度标志常量

```c
#define SCHEDULE_CHANGE_PRIO    0x1   /* 改变优先级 */
#define SCHEDULE_CHANGE_QUANTUM 0x2   /* 改变时间片 */
#define SCHEDULE_CHANGE_CPU     0x4   /* 改变 CPU */
#define SCHEDULE_CHANGE_ALL     (SCHEDULE_CHANGE_PRIO | SCHEDULE_CHANGE_QUANTUM | SCHEDULE_CHANGE_CPU)
```

---

## 10. C 源码

**文件**: `minix3/minix/servers/sched/schedule.c` (关键常量)

```c
#define BALANCE_TIMEOUT         5          /* Balance queues every 5 seconds */
#define DEFAULT_USER_TIME_SLICE 200        /* Default quantum for user processes */
#define CPU_DEAD                -1         /* Mark CPU as unavailable */

#define schedule_process_local(p) \
    schedule_process(p, SCHEDULE_CHANGE_PRIO | SCHEDULE_CHANGE_QUANTUM)
#define schedule_process_migrate(p) \
    schedule_process(p, SCHEDULE_CHANGE_CPU)
```
