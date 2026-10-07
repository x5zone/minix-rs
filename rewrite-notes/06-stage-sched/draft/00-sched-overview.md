# 00-sched-overview: SCHED 层架构概览 (fork 视角)

> **分类**: SCHED 整体层级
> **说明**: 汇总 SCHED 模块在 fork 流程中的全局概念、设计原则和跨组件约定

---

## 1. SCHED 在 fork 中的角色

### 1.1 fork 流程概览

```
PM: do_fork()
    ├── 1. 找空闲 mproc slot
    ├── 2. vm_fork() ──→ VM (VM_FORK)
    │       └── VM 内部 sys_fork() ──→ Kernel (SYS_FORK)
    ├── 3. 复制 mproc、分配 PID
    ├── 4. tell_vfs() ──→ VFS (VFS_PM_FORK, async)
    │       └── VFS_PM_FORK_REPLY ──→ PM
    ├── 5. sched_inherit() ──→ SCHED (SCHEDULING_INHERIT)  ← 本阶段
    │       └── SCHED 为子进程设置调度参数
    └── 6. reply(parent, child_pid), reply(child, OK)
```

### 1.2 SCHED 的职责

在 fork 流程中，SCHED 负责：

1. **调度参数继承**: 子进程从父进程继承优先级和时间片
2. **调度接管**: 通过 `sys_schedctl()` 告知内核 SCHED 接管子进程的调度
3. **CPU 选择**: 为子进程选择合适的 CPU（SMP 场景）
4. **内核调度设置**: 通过 `sys_schedule()` 设置子进程的优先级、时间片、CPU

### 1.3 与其他组件的协作

#### 1.3.1 与 PM 的协作

- **PM 职责**: 在 VFS 回复后调用 `sched_inherit()`，传递子进程/父进程 endpoint、优先级上限
- **SCHED 职责**: 初始化子进程调度状态，回复 PM
- **协作方式**: PM 通过同步 IPC 发送 `SCHEDULING_INHERIT` 消息

#### 1.3.2 与 Kernel 的协作

- **sys_schedctl()**: SCHED 告知内核接管某进程的调度控制
- **sys_schedule()**: SCHED 设置进程的优先级、时间片、CPU
- **SCHEDULING_NO_QUANTUM**: 内核通知 SCHED 某进程时间片用完

---

## 2. 核心数据结构

### 2.1 SCHED 数据结构全景

```
schedproc (进程调度状态)
  ├── endpoint         — 进程端点号
  ├── parent           — 父进程端点号
  ├── flags            — IN_USE 等标志
  ├── max_priority     — 最高允许优先级
  ├── priority         — 当前优先级
  ├── time_slice       — 时间片 (quantum)
  ├── cpu              — 当前 CPU 编号
  └── cpu_mask[]       — 允许运行的 CPU 位图
```

### 2.2 关键结构体

| 结构体 | 定义文件 | 说明 | fork 时的处理 |
|--------|----------|------|---------------|
| `struct schedproc` | `schedproc.h` | SCHED 进程状态 | 初始化新 slot，从父进程继承 |

### 2.3 进程表架构

SCHED 维护 `schedproc[NR_PROCS]` 数组，是 Minix3 分布式进程表的第五份：

| 组件 | 进程表 | 标识字段 |
|------|--------|---------|
| Kernel | `proc[]` | `p_endpoint`, `p_nr` |
| PM | `mproc[]` | `mp_endpoint`, `mp_pid` |
| VM | `vmproc[]` | `vm_endpoint` |
| VFS | `fproc[]` | `fp_endpoint`, `fp_pid` |
| **SCHED** | `schedproc[]` | `endpoint`, `parent` |

---

## 3. SCHED 源码组织

### 3.1 目录结构

```
minix3/minix/servers/sched/
├── schedproc.h      ← SCHED 进程结构体 (01)
├── schedule.c       ← 核心调度逻辑 (02-04)
├── main.c           ← 主循环与消息分发
├── utility.c        ← 工具函数
└── Makefile
```

### 3.2 文档与源码对应

| 文档 | 对应源文件 | 覆盖内容 |
|------|-----------|---------|
| 01-sched-struct | `schedproc.h` | schedproc 结构体 |
| 02-sched-inherit | `schedule.c` | SCHEDULING_INHERIT 消息处理 |
| 03-sched-start | `schedule.c`, `main.c` | do_start_scheduling 完整流程 |
| 04-sched-quantum | `schedule.c` | 时间片、优先级、队列平衡 |
| 99-global-concepts | — | 调度全局概念暂存区 |

---

## 4. SCHEDULING_INHERIT 完整流程

```
PM 发送 SCHEDULING_INHERIT 消息
    │ {parent_ep, child_ep, max_priority}
    │
    ▼
SCHED main loop (main.c)
    │ 接收消息，分发到 do_start_scheduling()
    │
    ▼
do_start_scheduling() (schedule.c)
    │
    ├── 1. sched_isemtyendpt() — 验证子进程 endpoint 有空 slot
    ├── 2. sched_isokendpt() — 验证父进程 endpoint 有效
    ├── 3. schedproc[child].endpoint = child_ep
    ├── 4. schedproc[child].parent = parent_ep
    ├── 5. schedproc[child].max_priority = msg.max_priority
    ├── 6. schedproc[child].priority = schedproc[parent].priority  (继承!)
    ├── 7. schedproc[child].time_slice = schedproc[parent].time_slice (继承!)
    ├── 8. schedproc[child].flags = IN_USE
    ├── 9. pick_cpu() — 选择 CPU
    ├── 10. sys_schedctl() — 告知内核 SCHED 接管
    ├── 11. schedule_process() — 设置内核调度参数
    └── 12. 回复 PM {scheduler_endpoint = SCHED_PROC_NR}
```

---

## 5. SCHED 消息类型

| 消息类型 | 方向 | fork 相关 | 说明 |
|---------|------|----------|------|
| `SCHEDULING_NO_QUANTUM` | Kernel → SCHED | 间接 | 进程时间片用完 |
| `SCHEDULING_START` | PM → SCHED | 否 | 显式启动调度（exec 后） |
| `SCHEDULING_STOP` | PM → SCHED | 否 | 停止调度（exit 时） |
| `SCHEDULING_SET_NICE` | PM → SCHED | 否 | 修改 nice 值 |
| `SCHEDULING_INHERIT` | PM → SCHED | **是** | fork 时继承调度参数 |

---

## 6. 与其他阶段的对比

| 维度 | Kernel (03) | VFS (04) | SCHED (05) |
|------|-------------|----------|------------|
| 核心结构 | `struct proc` | `struct fproc` | `struct schedproc` |
| 引用计数 | 无 | filp_count / v_ref_count | 无 |
| 复制方式 | 整体复制 + 修正 | 整体复制 + 修正 | 不复制，从父进程字段继承 |
| fork 触发 | VM 发送 SYS_FORK | PM 发送 VFS_PM_FORK | PM 发送 SCHEDULING_INHERIT |
| fork 回复 | 同步返回 endpoint | 异步 ipc_send 回复 | 同步回复 scheduler endpoint |
| 代码量 | ~1400 行 do_fork | ~50 行 pm_fork | ~80 行 do_start_scheduling |
