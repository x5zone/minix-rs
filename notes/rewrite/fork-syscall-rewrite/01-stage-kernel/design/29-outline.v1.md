# 29-kernel-debug: 大纲（Outline v1）

> **文档**: `29-kernel-debug.md`
> **状态**: v1 快照（2026-08-12）
> **教学目标**: 从"内核如何自我诊断"这一问题出发，建立调试基础设施的架构角色心智模型

---

## Ch1: 概念（内核调试基础设施）

### 教学目标
- **核心问题**: 内核是系统中最关键的组件，当调度队列不一致或 IPC 死锁时，内核如何自我诊断？用户态进程卡死可以由内核检测，但内核自身的调度队列错误谁来发现？
- **CPU/OS perspective question**: "内核如何自我诊断？"——这是内核可靠性的元问题。

### 1.1 调试基础设施的四类功能
- **调度队列 sanity check**: 验证 runqueue 一致性（head/tail 指针、nextready 链表）
- **进程信息打印**: 打印进程详情 + 依赖链 + 栈回溯
- **IPC 消息跟踪**: hook IPC 调用路径，打印消息流向
- **IPC 统计**: 统计消息频率，识别热点路径

### 1.2 条件编译模型
- `CONFIG_SMP`: SMP 调度队列检查
- `DEBUG_DUMPIPC` / `DEBUG_DUMPIPCF`: IPC 消息打印
- `DEBUG_IPCSTATS`: IPC 统计
- `DEBUG_IPC_HOOK`: IPC hook 回调

### 1.3 redox 对照
- **redox**: 无内核调试 hook——scheme 模型，调试通过 scheme 日志
- **Minix3**: 内核内调试 hook + 条件编译
- **minix-rs**: WONTFIX——用 `log` crate + `tracing` crate 替代

### 1.4 本章不讲什么
- 调度队列正常逻辑（见 11-scheduling-primitives.md）
- IPC 核心逻辑（见 12-ipc-core.md）
- SMP 机制（见 16-smp.md）

---

## Ch2: C 源码分析

### 2.1 文件清单
| 文件 | 行数 | 核心内容 |
|------|------|---------|
| `debug.c` | ~563 | 17 函数 + 4 宏 |

### 2.2 四类功能详解
- 调度队列 sanity check: `runqueues_ok_cpu` / `runqueues_ok_all` / `runqueues_ok`
- 进程打印: `print_proc` / `print_proc_depends` / `print_proc_recursive` / `printproc` / `printparam` / `namematch`
- IPC 统计: `printstats` / `sortstats` / `statmsg`
- IPC hooks: `hook_ipc_msgkcall` / `hook_ipc_msgkresult` / `hook_ipc_msgrecv` / `hook_ipc_msgsend` / `hook_ipc_clear`

---

## Ch3: 设计决策（WONTFIX）

- D1: 调度队列 sanity check 不实现
- D2: 进程信息打印不实现
- D3: IPC 消息跟踪不实现
- D4: IPC 统计不实现

---

## Ch4: Rust 实现

### 4.1 不实现（WONTFIX）
全部 17 函数 + 4 宏不实现。

### 4.2 替代方案
- `log` crate + `tracing` crate: 结构化日志
- `debug_assert!`: 运行时断言
- `Debug` / `Display` trait: 规范化格式化
- 外部工具: `perf` / `flamegraph`

---

## Ch5: 测试

不需要测试（WONTFIX 项）。

---

## Ch6: 跨文档引用
- [11-scheduling-primitives.md](11-scheduling-primitives.md): 调度队列
- [12-ipc-core.md](12-ipc-core.md): IPC 核心
- [16-smp.md](16-smp.md): SMP 调度
