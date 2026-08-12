# 03-sched-start: do_start_scheduling 完整流程

> 本文档分析 `minix3/minix/servers/sched/schedule.c` 中 `do_start_scheduling()` 函数的完整实现。

---

## 1. 概述

### 1.1 do_start_scheduling 的角色

- TODO: 说明 `do_start_scheduling()` 是 SCHED 中 fork 相关消息的统一处理入口
- TODO: 说明同时处理 SCHEDULING_START 和 SCHEDULING_INHERIT 两种消息
- TODO: 说明是 SCHED 中最复杂的函数，涉及 slot 初始化、内核交互、CPU 选择

### 1.2 函数签名

- TODO: 分析 `int do_start_scheduling(message *m_ptr)` 签名
- TODO: 说明参数和返回值

---

## 2. 消息源验证

### 2.1 accept_message()

- TODO: 分析 `accept_message(m_ptr)` 调用——确保消息来自 PM 或 RS
- TODO: 说明 SCHED 的安全模型——只信任 PM 和 RS

---

## 3. Endpoint 验证

### 3.1 子进程 endpoint 验证

- TODO: 分析 `sched_isemtyendpt(m_ptr->m_lsys_sched_scheduling_start.endpoint, &proc_nr_n)`
- TODO: 说明验证子进程 slot 必须空闲
- TODO: 说明 proc_nr_n 输出参数——子进程的 slot 号

### 3.2 父进程 endpoint 验证

- TODO: 分析 `sched_isokendpt(m_ptr->m_lsys_sched_scheduling_start.parent, &parent_nr_n)`
- TODO: 说明验证父进程 slot 必须在使用中
- TODO: 说明 SCHEDULING_INHERIT 时必须有有效父进程

---

## 4. Schedproc 初始化

### 4.1 基本字段设置

- TODO: 分析 slot 初始化代码：
  ```c
  rmp = &schedproc[proc_nr_n];
  rmp->endpoint = m_ptr->m_lsys_sched_scheduling_start.endpoint;
  rmp->parent = m_ptr->m_lsys_sched_scheduling_start.parent;
  rmp->max_priority = m_ptr->m_lsys_sched_scheduling_start.max_priority;
  ```

### 4.2 调度参数设置（分支）

- TODO: 分析 SCHEDULING_START 分支：
  ```c
  rmp->priority = rmp->max_priority;
  rmp->time_slice = m_ptr->m_lsys_sched_scheduling_start.quantum;
  ```
- TODO: 分析 SCHEDULING_INHERIT 分支：
  ```c
  rmp->priority = schedproc[parent_nr_n].priority;
  rmp->time_slice = schedproc[parent_nr_n].time_slice;
  ```
- TODO: 说明两个分支的差异——START 用显式值，INHERIT 用父进程值

### 4.3 标志设置

- TODO: 分析 `rmp->flags = IN_USE` — 标记 slot 被占用

---

## 5. 内核调度接管

### 5.1 sys_schedctl()

- TODO: 分析 `sys_schedctl()` 内核调用——告知内核 SCHED 接管此进程的调度
- TODO: 说明参数和返回值
- TODO: 说明为什么必须先调用 sys_schedctl() 再调用 schedule_process()

### 5.2 sys_schedctl 的内核侧

- TODO: 分析内核中 `do_schedctl()` 的处理
- TODO: 说明内核如何记录 SCHED 为进程的调度器

---

## 6. CPU 选择

### 6.1 pick_cpu()

- TODO: 分析 `pick_cpu()` 函数——为进程选择最合适的 CPU
- TODO: 说明 CPU 选择策略：
  - 系统进程：限制在 BSP (CPU 0)
  - 用户进程：选择负载最低的 CPU
- TODO: 说明 `is_system_proc(p)` 判断——`parent == RS_PROC_NR`

### 6.2 CPU 负载追踪

- TODO: 分析 `cpu_proc[]` 数组——追踪每个 CPU 上的进程数量
- TODO: 说明 fork 时递增目标 CPU 的计数
- TODO: 说明 exit 时递减（do_stop_scheduling）

---

## 7. 调度参数下发

### 7.1 schedule_process()

- TODO: 分析 `schedule_process()` 函数——调用 `sys_schedule()` 设置内核调度参数
- TODO: 说明 flags 参数控制更新哪些属性：
  - SCHEDULE_CHANGE_PRIO — 更新优先级
  - SCHEDULE_CHANGE_QUANTUM — 更新时间片
  - SCHEDULE_CHANGE_CPU — 更新 CPU 分配
- TODO: 说明 fork 时使用 `SCHEDULE_CHANGE_ALL`

### 7.2 sys_schedule()

- TODO: 分析 `sys_schedule()` 内核调用的参数：
  - endpoint — 进程端点
  - priority — 新优先级
  - quantum — 新时间片
  - cpu — 新 CPU
  - niced — 是否被 nice 降级
- TODO: 说明内核如何应用这些参数到进程的调度状态

---

## 8. 回复 PM

### 8.1 回复消息构造

- TODO: 分析回复消息的构造：
  ```c
  m_ptr->m_sched_lsys_scheduling_start.scheduler = SCHED_PROC_NR;
  ```
- TODO: 说明 scheduler 字段的含义——告诉 PM 此进程由哪个调度器管理

---

## 9. 完整流程图

```
do_start_scheduling(m_ptr)
    │
    ├── 1. accept_message() — 验证消息来源
    ├── 2. sched_isemtyendpt(child) — 验证子进程 slot 空闲
    ├── 3. sched_isokendpt(parent) — 验证父进程 slot 有效
    ├── 4. rmp->endpoint = child_ep
    ├── 5. rmp->parent = parent_ep
    ├── 6. rmp->max_priority = msg.max_priority
    ├── 7. if INHERIT:
    │       rmp->priority = schedproc[parent].priority
    │       rmp->time_slice = schedproc[parent].time_slice
    │   if START:
    │       rmp->priority = rmp->max_priority
    │       rmp->time_slice = msg.quantum
    ├── 8. rmp->flags = IN_USE
    ├── 9. pick_cpu(rmp) — 选择 CPU
    ├── 10. sys_schedctl() — 接管调度
    ├── 11. schedule_process(rmp, SCHEDULE_CHANGE_ALL)
    └── 12. 回复 PM {scheduler = SCHED_PROC_NR}
```

---

## 10. C 源码

**文件**: `minix3/minix/servers/sched/schedule.c`

```c
int do_start_scheduling(message *m_ptr)
{
    struct schedproc *rmp;
    int proc_nr_n, parent_nr_n;

    if (sched_isemtyendpt(m_ptr->m_lsys_sched_scheduling_start.endpoint,
            &proc_nr_n) != OK)
        return EINVAL;

    if (sched_isokendpt(m_ptr->m_lsys_sched_scheduling_start.parent,
            &parent_nr_n) != OK)
        return EINVAL;

    rmp = &schedproc[proc_nr_n];
    rmp->endpoint = m_ptr->m_lsys_sched_scheduling_start.endpoint;
    rmp->parent = m_ptr->m_lsys_sched_scheduling_start.parent;
    rmp->max_priority = m_ptr->m_lsys_sched_scheduling_start.max_priority;

    switch (m_ptr->m_type) {
    case SCHEDULING_START:
        rmp->priority = rmp->max_priority;
        rmp->time_slice = m_ptr->m_lsys_sched_scheduling_start.quantum;
        break;
    case SCHEDULING_INHERIT:
        rmp->priority = schedproc[parent_nr_n].priority;
        rmp->time_slice = schedproc[parent_nr_n].time_slice;
        break;
    }

    rmp->flags = IN_USE;
    pick_cpu(rmp);

    if ((r = sys_schedctl(0, rmp->endpoint, 0)) != OK)
        return r;

    if ((r = schedule_process_local(rmp)) != OK)
        return r;

    m_ptr->m_sched_lsys_scheduling_start.scheduler = SCHED_PROC_NR;
    return OK;
}
```
