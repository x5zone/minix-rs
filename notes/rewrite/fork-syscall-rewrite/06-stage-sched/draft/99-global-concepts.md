# 99-global-concepts: SCHED 全局概念暂存区

> **分类**: Global 层级 ⚠️
> **说明**: 调度全局概念汇总，不只是 SCHED 视角，后续将迁移到系统级文档
> **⚠️ 注意**: 本文档内容属于系统全局，不应局限于 SCHED 视角

---

## 1. 分布式调度模型

### 1.1 内核调度 vs 用户态调度

- TODO: 分析 Minix3 的双层调度模型：
  - 内核负责：实际上下文切换、时间片检测、中断处理
  - SCHED 负责：优先级决策、时间片分配、CPU 选择
- TODO: 说明微内核设计将调度策略从内核中分离的原因

### 1.2 sys_schedctl() 的语义

- TODO: 分析 `sys_schedctl()` 的"调度器注册"语义
- TODO: 说明内核如何知道一个进程由哪个调度器管理
- TODO: 说明 fork 后子进程必须通过 sys_schedctl() 注册到 SCHED

### 1.3 sys_schedule() 的语义

- TODO: 分析 `sys_schedule()` 的"参数下发"语义
- TODO: 说明 SCHED 决策 → 内核执行的分离
- TODO: 说明 fork 后 SCHED 通过 sys_schedule() 告知内核子进程的调度参数

---

## 2. 调度参数继承模型

### 2.1 fork 时的继承

- TODO: 分析 fork 调度继承的 POSIX 语义
- TODO: 说明子进程继承父进程优先级和时间片的合理性

### 2.2 exec 时的重置

- TODO: 分析 exec 后 SCHEDULING_START 的参数重置
- TODO: 说明 exec 后 priority 被重置为 max_priority 的原因

### 2.3 独立演变

- TODO: 分析 fork 后父子进程调度参数如何独立演变
- TODO: 说明 do_noquantum / balance_queues 如何分别影响父子进程

---

## 3. 五进程表一致性

### 3.1 endpoint 的一致性保证

- TODO: 分析 fork 后五个进程表中的 endpoint 如何保持一致
- TODO: 说明创建顺序：Kernel → VM → PM → VFS → SCHED

### 3.2 进程退出时的清理顺序

- TODO: 分析 exit 时的清理顺序
- TODO: 说明 SCHED (do_stop_scheduling) 在清理链中的位置

---

## 4. 待迁移内容

> 本节记录后续需要迁移到系统级文档的内容。

### 4.1 微内核调度模型

> **待迁移**: 内核调度器与用户态调度器的分工应迁移到系统级调度文档。

### 4.2 优先级体系

> **待迁移**: NR_SCHED_QUEUES、优先级数值体系应迁移到系统级文档。

---

## 5. 参见

- [00-sched-overview.md](00-sched-overview.md) - SCHED 整体架构概览
- [03-stage-kernel/99-global-concepts.md](../03-stage-kernel/99-global-concepts.md) - Kernel 全局概念
- [04-stage-vfs/99-global-concepts.md](../04-stage-vfs/99-global-concepts.md) - VFS 全局概念
