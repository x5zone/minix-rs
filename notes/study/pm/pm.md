# PM（进程管理服务器）

> **学习目标**: 理解用户态进程管理的实现，POSIX 接口如何通过 IPC 实现。
> 
> **核心问题**: fork/exec/exit 如何在用户态实现？

---

## 模块总结

### PM 服务核心职责

PM（Process Manager）是 Minix3 微内核架构中的核心用户态服务，负责：

| 职责 | 说明 | 关键文件 |
|------|------|----------|
| **进程生命周期** | fork/exec/exit 实现 | forkexit.c, exec.c |
| **信号处理** | 信号发送、捕获、处理 | signal.c, event.c |
| **调度支持** | 与调度器协作 | schedule.c |
| **定时器管理** | alarm/setitimer | alarm.c, time.c |
| **进程身份** | UID/GID/PID 管理 | getset.c, misc.c |
| **调试支持** | ptrace 调试接口 | trace.c, profile.c |

### 核心设计原则

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        PM 设计原则                                           │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   1. 用户态实现                                                             │
│      - PM 运行在用户态，通过 IPC 与内核通信                                 │
│      - 进程管理逻辑不在内核中，降低内核复杂度                                │
│                                                                             │
│   2. 消息驱动                                                               │
│      - 所有系统调用通过消息传递                                             │
│      - 主循环接收消息，分发到对应处理函数                                   │
│                                                                             │
│   3. PM-VFS 协作                                                            │
│      - 进程状态变更需要同步通知 VFS                                         │
│      - 使用异步 IPC 避免阻塞                                                │
│                                                                             │
│   4. 进程表管理                                                             │
│      - PM 维护用户态进程表 (mproc)                                          │
│      - 内核维护内核态进程表 (proc)                                          │
│      - 两表通过端点号关联                                                   │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 关键数据结构

| 结构 | 文件 | 说明 |
|------|------|------|
| `struct mproc` | mproc.h | 用户态进程控制块 |
| `mp_flags` | mproc.h | 进程状态标志位 |
| `mp_tracer` | mproc.h | 跟踪者进程索引 |
| `mp_sigtrace` | mproc.h | 被拦截的信号集 |

### 核心流程

| 流程 | 文件 | 说明 |
|------|------|------|
| fork | process-lifecycle.md | 进程创建：分配 PID → 复制进程表 → 通知 VFS |
| exec | process-lifecycle.md | 程序加载：权限检查 → 加载映像 → 设置栈 |
| exit | process-lifecycle.md | 进程退出：清理资源 → 通知父进程 → 僵尸状态 |
| 信号处理 | signal-event.md | 信号发送 → 拦截检查 → 交付处理 |

---

## 文件列表

### 📋 完整文件速览

| 序号 | 文件 | 状态 | 说明 | 讲解笔记 |
|------|------|------|------|----------|
| 1 | `servers/pm/pm.h` | ✅ 已读 | PM 主头文件 | [pm-core-architecture.md](pm-core-architecture.md) |
| 2 | `servers/pm/mproc.h` | ✅ 已读 | PM 进程结构 | [pm-core-architecture.md](pm-core-architecture.md) |
| 3 | `servers/pm/type.h` | ✅ 已读 | PM 类型定义 | [pm-core-architecture.md](pm-core-architecture.md) |
| 4 | `servers/pm/proto.h` | ✅ 已读 | PM 函数原型 | [pm-core-architecture.md](pm-core-architecture.md) |
| 5 | `servers/pm/glo.h` | ✅ 已读 | PM 全局变量 | [pm-core-architecture.md](pm-core-architecture.md) |
| 6 | `servers/pm/const.h` | ✅ 已读 | PM 常量 | [pm-core-architecture.md](pm-core-architecture.md) |
| 7 | `servers/pm/main.c` | ✅ 已读 | PM 主循环 | [pm-core-architecture.md](pm-core-architecture.md) |
| 8 | `servers/pm/forkexit.c` | ✅ 已读 | fork/exit 实现 | [process-lifecycle.md](process-lifecycle.md) |
| 9 | `servers/pm/exec.c` | ✅ 已读 | exec 实现 | [process-lifecycle.md](process-lifecycle.md) |
| 10 | `servers/pm/signal.c` | ✅ 已读 | 信号处理 | [signal-event.md](signal-event.md) |
| 11 | `servers/pm/schedule.c` | ✅ 已读 | 调度支持 | [schedule-time.md](schedule-time.md) |
| 12 | `servers/pm/alarm.c` | ✅ 已读 | 定时器 | [schedule-time.md](schedule-time.md) |
| 13 | `servers/pm/getset.c` | ✅ 已读 | get/set 系统调用 | [syscall-interface.md](syscall-interface.md) |
| 14 | `servers/pm/misc.c` | ✅ 已读 | 杂项系统调用 | [syscall-interface.md](syscall-interface.md) |
| 15 | `servers/pm/trace.c` | ✅ 已读 | ptrace 支持 | [debug-profile.md](debug-profile.md) |
| 16 | `servers/pm/event.c` | ✅ 已读 | 事件处理 | [signal-event.md](signal-event.md) |
| 17 | `servers/pm/time.c` | ✅ 已读 | 时间相关 | [schedule-time.md](schedule-time.md) |
| 18 | `servers/pm/profile.c` | ✅ 已读 | 性能分析 | [debug-profile.md](debug-profile.md) |
| 19 | `servers/pm/table.c` | ✅ 已读 | 表管理 | [pm-core-architecture.md](pm-core-architecture.md) |
| 20 | `servers/pm/utility.c` | ✅ 已读 | 工具函数 | [utility-functions.md](utility-functions.md) |

---

### 📚 模块化讲解文档

#### 第一组：核心架构与基础设施

| 文档 | 核心内容 |
|------|----------|
| [pm-core-architecture.md](pm-core-architecture.md) | PM 整体架构、进程表结构、主循环、消息分发 |
| [process-tree-model.md](process-tree-model.md) | 进程树模型、父子关系、进程组、会话 |

**核心知识点**：
- Minix3 微内核架构中 PM 的角色
- 进程控制块 (mproc) 的结构设计
- 服务器进程的消息驱动模型

---

#### 第二组：进程生命周期管理

| 文档 | 核心内容 |
|------|----------|
| [process-lifecycle.md](process-lifecycle.md) | fork/exec/exit 完整流程、状态转换、PM-VFS 协作 |

**核心理论**：
- `fork()` 的进程表复制与 VFS 同步
- `exec()` 的可执行文件加载与权限检查
- 进程状态转换图（新建→运行→僵尸→回收）

---

#### 第三组：进程调度与时间管理

| 文档 | 核心内容 |
|------|----------|
| [schedule-time.md](schedule-time.md) | 调度支持、nice 值、定时器、时间系统调用 |

**核心理论**：
- nice 值与调度队列的映射关系
- alarm 定时器的实现机制
- 时间统计与资源使用

---

#### 第四组：信号与事件处理

| 文档 | 核心内容 |
|------|----------|
| [signal-event.md](signal-event.md) | 信号发送、捕获、处理、事件订阅 |

**核心理论**：
- 信号的三种处理方式（忽略、捕获、默认）
- 信号掩码与原子操作
- 事件订阅机制

---

#### 第五组：系统调用接口层

| 文档 | 核心内容 |
|------|----------|
| [syscall-interface.md](syscall-interface.md) | UID/GID/PID 管理、reboot、priority、rusage |

**典型系统调用**：
- `getpid()`, `getppid()`, `getuid()`
- `setuid()`, `setgid()`, `nice()`
- `reboot()`, `getpriority()`, `getrusage()`

---

#### 第六组：调试与性能分析

| 文档 | 核心内容 |
|------|----------|
| [debug-profile.md](debug-profile.md) | ptrace 调试支持、统计性性能分析 |

**应用场景**：
- GDB 调试器的底层实现原理
- 性能瓶颈分析与热点函数定位

---

#### 第七组：工具函数库

| 文档 | 核心内容 |
|------|----------|
| [utility-functions.md](utility-functions.md) | PID 分配、进程查找、优先级转换、VFS 通信 |

**核心函数**：
- `get_free_pid()` - PID 分配
- `find_proc()` - 进程查找
- `tell_vfs()` - 异步 VFS 通信
- `nice_to_priority()` - 优先级映射

---

### 🎯 推荐阅读顺序

```
第一阶段：基础认知 (1-2 周)
  └─> pm-core-architecture.md
  └─> process-tree-model.md

第二阶段：核心机制 (2-3 周)
  └─> process-lifecycle.md
  └─> schedule-time.md

第三阶段：高级特性 (1-2 周)
  └─> signal-event.md
  └─> syscall-interface.md

第四阶段：扩展知识 (1 周)
  └─> debug-profile.md
  └─> utility-functions.md
```

---

## 进度统计

| 分类 | 已读 | 待读 | 覆盖率 |
|------|------|------|--------|
| 头文件 | 6 | 0 | 100% |
| 实现文件 | 14 | 0 | 100% |
| **总计** | **20** | **0** | **100%** |

---

## Rust 重构要点

### 类型系统改进

| 当前设计 | Rust 改进 |
|----------|-----------|
| `pid_t mp_pid` (int) | `struct Pid(i32)` 强类型 |
| `int mp_flags` (位标志) | `enum ProcessStatus` 状态机 |
| `int mp_tracer` (索引) | `Option<TracerId>` |

### 错误处理改进

| 当前设计 | Rust 改进 |
|----------|-----------|
| 返回整数错误码 | `Result<T, PmError>` |
| panic 系统错误 | 类型状态防止非法状态 |

### 异步模型改进

| 当前设计 | Rust 改进 |
|----------|-----------|
| `tell_vfs()` + 标志位 | `async fn tell_vfs()` |
| SUSPEND 返回值 | `Future` 异步等待 |

详见各模块讲解文档的 Rust 重构章节。
