# 01-sched-struct: SchedProc 结构体

> 本文档分析 `minix3/minix/servers/sched/schedproc.h` 中的 schedproc 结构体。

---

## 1. 概述

### 1.1 SchedProc 的角色

- TODO: 说明 schedproc 是 SCHED 服务器中的进程调度状态结构体
- TODO: 说明 schedproc 是 Minix3 分布式进程表的第五份——专注于调度信息
- TODO: 说明与 Kernel proc / PM mproc / VM vmproc / VFS fproc 的区别——schedproc 最轻量

### 1.2 文件位置

- TODO: 列出 `schedproc.h` 的完整路径
- TODO: 说明 schedproc 数组 `schedproc[NR_PROCS]` 的静态分配

---

## 2. 标识字段

### 2.1 endpoint

- TODO: 分析 `endpoint_t endpoint` 字段——进程端点号
- TODO: 说明 fork 时从 PM 传入的子进程 endpoint
- TODO: 说明用于 sched_isokendpt() / sched_isemtyendpt() 验证

### 2.2 parent

- TODO: 分析 `endpoint_t parent` 字段——父进程端点号
- TODO: 说明 fork 时从 PM 传入的父进程 endpoint
- TODO: 说明用于判断进程类型：`is_system_proc(p)` 检查 `parent == RS_PROC_NR`
- TODO: 说明系统进程由 RS (Reincarnation Server) 创建，用户进程由 PM fork 创建

---

## 3. 标志字段

### 3.1 flags

- TODO: 分析 `unsigned flags` 字段——进程标志位
- TODO: 说明 `IN_USE` (0x0001) 标志——slot 被占用
- TODO: 说明 fork 时设置 `flags = IN_USE`
- TODO: 说明 exit 时清除 flags（do_stop_scheduling）

---

## 4. 调度参数字段

### 4.1 max_priority

- TODO: 分析 `unsigned max_priority` 字段——最高允许优先级
- TODO: 说明 fork 时从 PM 传入（基于 nice 值计算）
- TODO: 说明 max_priority 是进程可达的最高优先级上限
- TODO: 说明 priority 不能超过 max_priority

### 4.2 priority

- TODO: 分析 `unsigned priority` 字段——当前优先级
- TODO: 说明 fork 时从父进程继承：`priority = schedproc[parent].priority`
- TODO: 说明时间片用完时 priority 可能降低（do_noquantum）
- TODO: 说明 balance_queues() 可能恢复被降低的优先级

### 4.3 time_slice

- TODO: 分析 `unsigned time_slice` 字段——时间片（quantum）
- TODO: 说明 fork 时从父进程继承：`time_slice = schedproc[parent].time_slice`
- TODO: 说明 DEFAULT_USER_TIME_SLICE = 200 (ticks)
- TODO: 说明时间片用完时内核发送 SCHEDULING_NO_QUANTUM

---

## 5. SMP 相关字段

### 5.1 cpu

- TODO: 分析 `unsigned cpu` 字段——进程当前运行的 CPU 编号
- TODO: 说明 fork 时通过 `pick_cpu()` 选择
- TODO: 说明系统进程限制在 BSP (Bootstrap Processor) 上

### 5.2 cpu_mask[]

- TODO: 分析 `bitchunk_t cpu_mask[BITMAP_CHUNKS(CONFIG_MAX_CPUS)]` 字段——允许运行的 CPU 位图
- TODO: 说明 fork 时的 CPU 掩码设置
- TODO: 说明非 SMP 系统下只有一个 CPU

---

## 6. 进程表与 slot 管理

### 6.1 schedproc 数组

- TODO: 分析 `schedproc[NR_PROCS]` 静态数组
- TODO: 说明 slot 号通过 `_ENDPOINT_P(endpoint)` 从 endpoint 提取
- TODO: 说明空闲判断：`!(schedproc[slot].flags & IN_USE)`

### 6.2 sched_isokendpt()

- TODO: 分析 `sched_isokendpt()` 函数——验证 endpoint 对应的 slot 是否在使用
- TODO: 说明用于验证父进程 endpoint（fork 时）

### 6.3 sched_isemtyendpt()

- TODO: 分析 `sched_isemtyendpt()` 函数——验证 endpoint 对应的 slot 是否空闲
- TODO: 说明用于验证子进程 endpoint（fork 时）

---

## 7. fork 时的处理总结

| 字段 | fork 处理 | 来源 |
|------|----------|------|
| `endpoint` | 设为子进程 endpoint | PM 消息 |
| `parent` | 设为父进程 endpoint | PM 消息 |
| `flags` | 设为 IN_USE | SCHED 设置 |
| `max_priority` | 设为消息中的值 | PM 消息（基于 nice） |
| `priority` | 继承父进程 priority | `schedproc[parent].priority` |
| `time_slice` | 继承父进程 time_slice | `schedproc[parent].time_slice` |
| `cpu` | pick_cpu() 选择 | SCHED 计算 |
| `cpu_mask[]` | 设置允许 CPU | SCHED 初始化 |

**关键语义**: schedproc 不做"整体复制"，而是从父进程**选择性继承** priority 和 time_slice，其他字段从消息或计算中获得。

---

## 8. C 源码

**文件**: `minix3/minix/servers/sched/schedproc.h`

```c
struct schedproc {
    endpoint_t endpoint;        /* Process endpoint id */
    endpoint_t parent;         /* Parent endpoint id */
    unsigned flags;           /* Flag bits */

    /* User space scheduling */
    unsigned max_priority;     /* Process' highest allowed priority */
    unsigned priority;         /* Process' current priority */
    unsigned time_slice;       /* Process' time slice */
    unsigned cpu;             /* CPU the process is running on */
    bitchunk_t cpu_mask[BITMAP_CHUNKS(CONFIG_MAX_CPUS)]; /* CPUs allowed */
};

#define IN_USE 0x00001   /* slot is in use */
```
