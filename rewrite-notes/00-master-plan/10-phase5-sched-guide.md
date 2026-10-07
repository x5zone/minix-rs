# 阶段5：SCHED 调度参数继承指南

> **状态**: ❌ 待实现
> **对应源码**: `minix/servers/sched/schedule.c`, `minix/servers/sched/schedproc.h`

---

## 1. 目标与范围

实现 SCHED 服务器中的 fork 相关逻辑，使子进程正确继承父进程的调度参数并被内核调度器接纳。

### 核心功能

- `schedproc` 结构体定义与进程表管理
- `SCHEDULING_INHERIT` 消息处理
- 优先级和时间片从父进程继承
- `sys_schedctl()` 调度接管
- `sys_schedule()` 参数下发
- CPU 选择 (pick_cpu)

---

## 2. SCHED 服务器概述

### 2.1 与其他服务器的区别

SCHED 是 fork 路径上最轻量的服务器：
- 不做"整体复制"——只选择性继承 priority 和 time_slice
- 不管理引用计数——schedproc 是独占的
- 代码量最小——do_start_scheduling() 约 80 行

### 2.2 消息交互

| 消息 | 方向 | 作用 |
|------|------|------|
| `SCHEDULING_INHERIT` | PM → SCHED | fork 时继承调度参数 |
| `SCHEDULING_START` | PM → SCHED | exec 后重新初始化调度 |
| `SCHEDULING_STOP` | PM → SCHED | exit 时停止调度 |
| `SCHEDULING_NO_QUANTUM` | Kernel → SCHED | 时间片用完通知 |

---

## 3. 实现步骤

### 步骤1: schedproc 结构体

```rust
pub struct SchedProc {
    pub endpoint: Endpoint,
    pub parent: Endpoint,
    pub flags: SchedFlags,
    pub max_priority: u32,
    pub priority: u32,
    pub time_slice: u32,
    pub cpu: u32,
    pub cpu_mask: [u32; MAX_CPUS / 32],
}
```

### 步骤2: SCHEDULING_INHERIT 处理

```rust
pub fn do_start_scheduling(
    table: &mut SchedTable,
    msg: &SchedulingMessage,
) -> Result<Endpoint, SchedError> {
    let child_slot = msg.child_endpoint.extract_slot();
    let parent_slot = msg.parent_endpoint.extract_slot();

    // 验证
    table.ensure_empty(child_slot)?;
    table.ensure_in_use(parent_slot)?;

    // 初始化
    let parent = &table.procs[parent_slot];
    let child = &mut table.procs[child_slot];
    child.endpoint = msg.child_endpoint;
    child.parent = msg.parent_endpoint;
    child.max_priority = msg.max_priority;

    // 继承
    child.priority = parent.priority;
    child.time_slice = parent.time_slice;
    child.flags = SchedFlags::IN_USE;

    // CPU 选择 + 内核交互
    pick_cpu(child);
    sys_schedctl(child.endpoint)?;
    schedule_process(child, SCHEDULE_CHANGE_ALL)?;

    Ok(SCHED_PROC_NR)
}
```

### 步骤3: 内核交互

- `sys_schedctl()`: 告知内核 SCHED 接管调度
- `sys_schedule()`: 设置优先级、时间片、CPU

---

## 4. 验证标准

- [ ] fork 后子进程 schedproc 正确初始化
- [ ] priority 和 time_slice 从父进程继承
- [ ] sys_schedctl() 正确调用
- [ ] sys_schedule() 参数正确设置
- [ ] SCHEDULING_START 与 SCHEDULING_INHERIT 分支正确
- [ ] do_stop_scheduling() 清理正确

---

## 5. 参考文档

- [05-stage-sched/00-sched-overview.md](../05-stage-sched/00-sched-overview.md) - SCHED 架构概览
- [05-stage-sched/03-sched-start.md](../05-stage-sched/03-sched-start.md) - do_start_scheduling 详细分析
