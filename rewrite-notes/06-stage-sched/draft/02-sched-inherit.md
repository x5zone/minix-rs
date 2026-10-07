# 02-sched-inherit: SCHEDULING_INHERIT 消息处理

> 本文档分析 `minix3/minix/servers/sched/schedule.c` 中 `SCHEDULING_INHERIT` 消息的处理逻辑。

---

## 1. 概述

### 1.1 SCHEDULING_INHERIT 的角色

- TODO: 说明 SCHEDULING_INHERIT 是 PM 在 fork 流程中发送给 SCHED 的消息
- TODO: 说明与 SCHEDULING_START 的区别——INHERIT 从父进程继承参数，START 使用显式参数
- TODO: 说明 SCHEDULING_INHERIT 在 fork 流程中的位置：VFS 回复之后、唤醒父进程之前

### 1.2 PM 端的调用

- TODO: 分析 PM 中的 `sched_inherit()` 函数（libsys 接口）
- TODO: 说明 PM 如何构造 SCHEDULING_INHERIT 消息：
  - parent endpoint
  - child endpoint
  - max_priority（基于 nice 值计算）
- TODO: 说明 `sched_inherit()` 使用 `_taskcall()` 同步调用

---

## 2. 消息格式

### 2.1 PM → SCHED 方向

- TODO: 分析 SCHEDULING_INHERIT 消息字段：
  - `m_type = SCHEDULING_INHERIT`
  - `m_lsys_sched_scheduling_inherit.endpoint` — 子进程 endpoint
  - `m_lsys_sched_scheduling_inherit.parent` — 父进程 endpoint
  - `m_lsys_sched_scheduling_inherit.max_priority` — 最高允许优先级

### 2.2 SCHED → PM 方向（回复）

- TODO: 分析 SCHED 回复消息字段：
  - `m_type = OK` 或错误码
  - `m_sched_lsys_scheduling_inherit.scheduler` — SCHED_PROC_NR

---

## 3. 消息验证

### 3.1 accept_message()

- TODO: 分析 `utility.c` 中的 `accept_message()` 函数
- TODO: 说明 SCHED 只接受来自 PM 和 RS 的消息
- TODO: 说明安全模型——SCHED 不信任普通进程

### 3.2 sched_isemtyendpt() — 子进程 slot 验证

- TODO: 分析子进程 endpoint 的验证逻辑
- TODO: 说明提取 slot 号 + 检查 `!(flags & IN_USE)`

### 3.3 sched_isokendpt() — 父进程 slot 验证

- TODO: 分析父进程 endpoint 的验证逻辑
- TODO: 说明提取 slot 号 + 检查 `flags & IN_USE`

---

## 4. 继承逻辑

### 4.1 priority 继承

- TODO: 分析 `rmp->priority = schedproc[parent_nr_n].priority` — 直接继承父进程当前优先级
- TODO: 说明为什么继承当前优先级而非 max_priority——子进程应与父进程处于相同调度级别

### 4.2 time_slice 继承

- TODO: 分析 `rmp->time_slice = schedproc[parent_nr_n].time_slice` — 直接继承父进程时间片
- TODO: 说明时间片继承的合理性——子进程应获得与父进程相同的 CPU 时间配额

### 4.3 与 SCHEDULING_START 的对比

| 维度 | SCHEDULING_INHERIT | SCHEDULING_START |
|------|-------------------|-----------------|
| 触发时机 | fork 后 | exec 后 |
| priority | 继承父进程 | 设为 max_priority |
| time_slice | 继承父进程 | 从消息中获取 |
| 使用场景 | 新 fork 的子进程 | exec 后重新初始化 |

---

## 5. PM 端 sched_inherit() 实现

### 5.1 函数签名

- TODO: 分析 PM 调用的 `sched_inherit()` 函数签名
- TODO: 说明参数：child endpoint, parent endpoint, max_priority, quantum

### 5.2 调用时机

- TODO: 分析 PM 在 `main.c` 处理 VFS_PM_FORK_REPLY 时的调用
- TODO: 说明调用链：VFS 回复 → sched_inherit() → SCHED 回复 → 唤醒进程

### 5.3 错误处理

- TODO: 分析 sched_inherit() 失败时的处理
- TODO: 说明 fork 流程中 SCHED 失败的回滚策略

---

## 6. 时序关系

```
PM                          VFS                     SCHED
 │                           │                       │
 │── VFS_PM_FORK ──────────►│                       │
 │◄── VFS_PM_FORK_REPLY ────│                       │
 │                           │                       │
 │── SCHEDULING_INHERIT ────────────────────────────►│
 │                           │                       │
 │                           │    ├── 验证 endpoints │
 │                           │    ├── 继承 priority  │
 │                           │    ├── 继承 time_slice│
 │                           │    ├── pick_cpu()     │
 │                           │    ├── sys_schedctl() │
 │                           │    └── schedule_process()
 │                           │                       │
 │◄── OK (scheduler=SCHED_PROC_NR) ──────────────────│
 │                           │                       │
 │  reply(parent, child_pid) │                       │
 │  reply(child, OK)         │                       │
```

---

## 7. C 源码

**文件**: `minix3/minix/servers/sched/schedule.c` (SCHEDULING_INHERIT 处理)

```c
case SCHEDULING_INHERIT:
    /* Inherit scheduling parameters from parent */
    rmp->priority = schedproc[parent_nr_n].priority;
    rmp->time_slice = schedproc[parent_nr_n].time_slice;
    break;
```

**文件**: `minix3/minix/servers/pm/schedule.c` (PM 端调用)

```c
int sched_inherit(struct mproc *child, endpoint_t parent_ep,
                  unsigned max_priority, unsigned quantum)
{
    message m;
    memset(&m, 0, sizeof(m));
    m.m_type = SCHEDULING_INHERIT;
    m.m_lsys_sched_scheduling_inherit.endpoint = child->mp_endpoint;
    m.m_lsys_sched_scheduling_inherit.parent = parent_ep;
    m.m_lsys_sched_scheduling_inherit.max_priority = max_priority;
    return _taskcall(SCHED_PROC_NR, ...);
}
```
