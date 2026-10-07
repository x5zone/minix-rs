# Minix3 mp_flags 状态分析

> **目标**: 详细记录 mp_flags 的所有状态、组合约束和转换路径  
> **源码版本**: Minix3  
> **参考文件**: `minix/servers/pm/mproc.h`, `forkexit.c`, `signal.c`, `main.c`, `event.c`, `trace.c`

---

## 一、Flag 定义

### 1.1 完整 Flag 列表

| Flag | 值 | 名称 | 用途 |
|------|-----|------|------|
| `IN_USE` | 0x00001 | 槽位使用中 | 标识 mproc 槽位已被分配 |
| `WAITING` | 0x00002 | 等待子进程 | 父进程正在执行 wait4() |
| `ZOMBIE` | 0x00004 | 僵尸状态 | 进程已退出，等待父进程收尸 |
| `PROC_STOPPED` | 0x00008 | 内核停止 | 进程在内核中被停止 |
| `ALARM_ON` | 0x00010 | 定时器开启 | SIGALRM 定时器已启动 |
| `EXITING` | 0x00020 | 正在退出 | 进程正在执行退出流程 |
| `TOLD_PARENT` | 0x00040 | 已通知父进程 | 父进程已完成 wait() |
| `TRACE_STOPPED` | 0x00080 | 追踪停止 | 进程因追踪而停止 |
| `SIGSUSPENDED` | 0x00100 | 信号挂起 | sigsuspend() 调用中 |
| `VFS_CALL` | 0x00400 | 等待 VFS | 正在等待 VFS 回复 |
| `NEW_PARENT` | 0x00800 | 父进程变更 | 父进程在 VFS 调用期间变更 |
| `UNPAUSED` | 0x01000 | VFS 已回复 | VFS 已回复 unpause 请求 |
| `PRIV_PROC` | 0x02000 | 系统进程 | 系统进程，有特殊权限 |
| `PARTIAL_EXEC` | 0x04000 | 部分执行 | exec 部分完成 |
| `TRACE_EXIT` | 0x08000 | 追踪退出 | tracer 正在强制进程退出 |
| `TRACE_ZOMBIE` | 0x10000 | 追踪僵尸 | 等待 tracer 收尸 |
| `DELAY_CALL` | 0x20000 | 延迟调用 | 等待调用完成后再发送信号 |
| `TAINTED` | 0x40000 | 污染标记 | 进程被污染 |
| `EVENT_CALL` | 0x80000 | 事件订阅 | 等待进程事件订阅者 |

### 1.2 Flag 分类

#### 生命周期 Flags（互斥）

| Flag | 说明 |
|------|------|
| `IN_USE` | 槽位使用中（基础标志） |
| `EXITING` | 正在退出 |
| `TRACE_ZOMBIE` | 追踪僵尸 |
| `ZOMBIE` | 僵尸状态 |
| `TOLD_PARENT` | 已通知父进程 |

#### 阻塞状态 Flags（可组合）

| Flag | 说明 |
|------|------|
| `PROC_STOPPED` | 内核停止 |
| `VFS_CALL` | 等待 VFS |
| `EVENT_CALL` | 等待事件订阅者 |
| `DELAY_CALL` | 延迟调用 |
| `UNPAUSED` | VFS 已回复 |

#### 父进程状态 Flags

| Flag | 说明 |
|------|------|
| `WAITING` | 父进程等待子进程 |

#### 追踪 Flags

| Flag | 说明 |
|------|------|
| `TRACE_STOPPED` | 追踪停止 |
| `TRACE_EXIT` | 追踪退出 |

#### 权限 Flags

| Flag | 说明 |
|------|------|
| `PRIV_PROC` | 系统进程 |

#### 其他 Flags

| Flag | 说明 |
|------|------|
| `ALARM_ON` | 定时器开启 |
| `SIGSUSPENDED` | 信号挂起 |
| `NEW_PARENT` | 父进程变更 |
| `PARTIAL_EXEC` | 部分执行 |
| `TAINTED` | 污染标记 |

---

## 二、状态组合约束

### 2.1 互斥组合

| Flag A | Flag B | 原因 | 源码引用 |
|--------|--------|------|---------|
| `ZOMBIE` | `TRACE_ZOMBIE` | 一个进程不能同时是两种僵尸 | `forkexit.c:603-604` |
| `ZOMBIE` | `TOLD_PARENT` | 通知父进程后不再是僵尸 | `forkexit.c:687-690` |
| `PROC_STOPPED` | `DELAY_CALL` | 停止和延迟调用互斥 | `signal.c:237` |
| `VFS_CALL` | `EVENT_CALL` | 一次只能等待一个服务 | `signal.c:731` |

### 2.2 必须组合

| Flag A | Flag B | 原因 | 源码引用 |
|--------|--------|------|---------|
| `ZOMBIE` | `IN_USE` | 僵尸进程必须在使用中 | `forkexit.c:687` |
| `TRACE_ZOMBIE` | `IN_USE` | 追踪僵尸必须在使用中 | `forkexit.c:742` |
| `EXITING` | `IN_USE` | 退出进程必须在使用中 | `forkexit.c:362` |
| `UNPAUSED` | `PROC_STOPPED` | unpause 后必须停止 | `main.c:407` |

### 2.3 可选组合

| Flag A | Flag B | 说明 | 源码引用 |
|--------|--------|------|---------|
| `EXITING` | `VFS_CALL` | 退出时可能还在等待 VFS | `forkexit.c:374` |
| `EXITING` | `PROC_STOPPED` | 退出时可能被停止 | `forkexit.c:374` |
| `EXITING` | `TRACE_EXIT` | 退出时可能被追踪 | `forkexit.c:374` |
| `EXITING` | `PRIV_PROC` | 系统进程退出 | `forkexit.c:374` |
| `VFS_CALL` | `PROC_STOPPED` | VFS 调用时可能被停止 | `signal.c:677` |
| `EVENT_CALL` | `PROC_STOPPED` | 事件调用时可能被停止 | `signal.c:677` |
| `WAITING` | `PROC_STOPPED` | 等待时可能被停止 | `signal.c:750` |
| `SIGSUSPENDED` | `PROC_STOPPED` | sigsuspend 时可能被停止 | `signal.c:750` |

---

## 三、状态转换路径

### 3.1 生命周期转换

```
┌─────────────────────────────────────────────────────────────────────────┐
│                           进程生命周期                                    │
└─────────────────────────────────────────────────────────────────────────┘

    ┌─────────┐
    │ Unused  │  mp_flags = 0
    └────┬────┘
         │ fork()
         │ 源码: forkexit.c:106
         │ 设置: mp_flags = IN_USE
         ↓
    ┌─────────┐
    │ Running │  mp_flags = IN_USE
    └────┬────┘
         │ exit() / signal
         │ 源码: forkexit.c:374-375
         │ 保留: IN_USE|VFS_CALL|PRIV_PROC|TRACE_EXIT|PROC_STOPPED
         │ 设置: EXITING
         ↓
    ┌─────────┐
    │ Exiting │  mp_flags = IN_USE | EXITING | ...
    └────┬────┘
         │ zombify() - 如果 tracer != parent
         │ 源码: forkexit.c:607-608
         │ 设置: TRACE_ZOMBIE
         ↓
    ┌──────────────┐
    │ TraceZombie  │  mp_flags = IN_USE | EXITING | TRACE_ZOMBIE
    └──────┬───────┘
           │ tell_tracer() - tracer 收尸后
           │ 源码: forkexit.c:751-753
           │ 清除: TRACE_ZOMBIE
           │ 设置: ZOMBIE
           ↓
    ┌─────────┐
    │ Zombie  │  mp_flags = IN_USE | EXITING | ZOMBIE
    └────┬────┘
         │ tell_parent() - 父进程 wait() 后
         │ 源码: forkexit.c:715-717
         │ 清除: ZOMBIE
         │ 设置: TOLD_PARENT
         ↓
    ┌─────────────┐
    │ ToldParent  │  mp_flags = IN_USE | EXITING | TOLD_PARENT
    └──────┬──────┘
           │ cleanup()
           │ 源码: forkexit.c:802
           │ 清除: 所有 flags
           ↓
    ┌─────────┐
    │ Unused  │  mp_flags = 0
    └─────────┘
```

### 3.2 阻塞状态转换

```
┌─────────────────────────────────────────────────────────────────────────┐
│                           阻塞状态转换                                    │
└─────────────────────────────────────────────────────────────────────────┘

    ┌──────────────┐
    │ 未阻塞       │  stopped=false, ipc_blocked=None
    └──────┬───────┘
           │
           ├──────────────────────────────────────────────────────┐
           │                                                      │
           │ stop_proc()                                          │ tell_vfs()
           │ 源码: signal.c:246                                   │ 源码: utility.c:138
           │ 设置: PROC_STOPPED                                   │ 设置: VFS_CALL
           ↓                                                      ↓
    ┌──────────────┐                                      ┌──────────────┐
    │ Stopped      │  stopped=true                        │ VfsBlocked   │  ipc_blocked=VfsCall
    └──────┬───────┘                                      └──────┬───────┘
           │                                                      │
           │ try_resume_proc()                                    │ handle_vfs_reply()
           │ 源码: signal.c:288                                   │ 源码: main.c:328
           │ 清除: PROC_STOPPED | UNPAUSED                        │ 清除: VFS_CALL
           ↓                                                      ↓
    ┌──────────────┐                                      ┌──────────────┐
    │ 未阻塞       │                                      │ 未阻塞       │
    └──────────────┘                                      └──────────────┘
```

### 3.3 追踪状态转换

```
┌─────────────────────────────────────────────────────────────────────────┐
│                           追踪状态转换                                    │
└─────────────────────────────────────────────────────────────────────────┘

    ┌──────────────┐
    │ 未追踪       │  tracer = NO_TRACER
    └──────┬───────┘
           │ ptrace(T_OK) 或 ptrace(T_ATTACH)
           │ 源码: trace.c:58, trace.c:87
           │ 设置: mp_tracer
           ↓
    ┌──────────────┐
    │ Traced       │  tracer = parent 或其他进程
    └──────┬───────┘
           │
           ├──────────────────────────────────────────────────────┐
           │                                                      │
           │ sig_proc() with signal                               │ exit_proc()
           │ 源码: signal.c:419-420                               │ 源码: forkexit.c:374
           │ 设置: TRACE_STOPPED                                  │ 保留: TRACE_EXIT
           ↓                                                      ↓
    ┌──────────────┐                                      ┌──────────────┐
    │ TraceStopped │  TRACE_STOPPED                       │ TraceExit    │  TRACE_EXIT
    └──────────────┘                                      └──────────────┘
```

---

## 四、源码引用

### 4.1 fork 相关

| 操作 | 文件 | 行号 | 代码 |
|------|------|------|------|
| 设置 IN_USE | `forkexit.c` | 106 | `rmc->mp_flags &= (IN_USE\|DELAY_CALL\|TAINTED);` |
| 继承 PRIV_PROC | `forkexit.c` | 100-103 | `if (rmc->mp_flags & PRIV_PROC) {...}` |
| 设置 tracer | `forkexit.c` | 91-95 | `if (!(rmc->mp_trace_flags & TO_TRACEFORK)) {...}` |

### 4.2 exit 相关

| 操作 | 文件 | 行号 | 代码 |
|------|------|------|------|
| 设置 EXITING | `forkexit.c` | 374-375 | `rmp->mp_flags &= (...); rmp->mp_flags \|= EXITING;` |
| 设置 PROC_STOPPED | `forkexit.c` | 326-330 | `if (!(rmp->mp_flags & PROC_STOPPED)) {...}` |
| 设置 TRACE_ZOMBIE | `forkexit.c` | 607-608 | `if (rmp->mp_tracer != NO_TRACER && ...) rmp->mp_flags \|= TRACE_ZOMBIE;` |
| 设置 ZOMBIE | `forkexit.c` | 619 | `rmp->mp_flags \|= ZOMBIE;` |
| 清除 ZOMBIE，设置 TOLD_PARENT | `forkexit.c` | 715-717 | `child->mp_flags &= ~ZOMBIE; child->mp_flags \|= TOLD_PARENT;` |
| 清除 TRACE_ZOMBIE，设置 ZOMBIE | `forkexit.c` | 751-753 | `child->mp_flags &= ~TRACE_ZOMBIE; child->mp_flags \|= ZOMBIE;` |
| 清除所有 flags | `forkexit.c` | 802 | `rmp->mp_flags = 0;` |

### 4.3 信号相关

| 操作 | 文件 | 行号 | 代码 |
|------|------|------|------|
| 设置 PROC_STOPPED | `signal.c` | 246 | `rmp->mp_flags \|= PROC_STOPPED;` |
| 设置 DELAY_CALL | `signal.c` | 254 | `rmp->mp_flags \|= DELAY_CALL;` |
| 清除 PROC_STOPPED | `signal.c` | 288 | `rmp->mp_flags &= ~(PROC_STOPPED \| UNPAUSED);` |
| 清除 DELAY_CALL | `signal.c` | 351 | `rmp->mp_flags &= ~DELAY_CALL;` |
| 检查 VFS_CALL + PROC_STOPPED | `signal.c` | 672-677 | `if (rmp->mp_flags & (VFS_CALL \| EVENT_CALL)) { assert(rmp->mp_flags & PROC_STOPPED); }` |

### 4.4 VFS 相关

| 操作 | 文件 | 行号 | 代码 |
|------|------|------|------|
| 设置 VFS_CALL | `utility.c` | 138 | `rmp->mp_flags \|= VFS_CALL;` |
| 清除 VFS_CALL | `main.c` | 328 | `rmp->mp_flags &= ~(VFS_CALL \| NEW_PARENT);` |
| 设置 UNPAUSED | `main.c` | 410 | `rmp->mp_flags \|= UNPAUSED;` |
| 设置 NEW_PARENT | `forkexit.c` | 402-403 | `if (rmp->mp_flags & VFS_CALL) rmp->mp_flags \|= NEW_PARENT;` |

### 4.5 事件相关

| 操作 | 文件 | 行号 | 代码 |
|------|------|------|------|
| 设置 EVENT_CALL | `event.c` | 349 | `rmp->mp_flags \|= EVENT_CALL;` |
| 清除 EVENT_CALL | `event.c` | 116 | `rmp->mp_flags &= ~EVENT_CALL;` |

### 4.6 追踪相关

| 操作 | 文件 | 行号 | 代码 |
|------|------|------|------|
| 设置 TRACE_EXIT | `trace.c` | 147 | `child->mp_flags \|= TRACE_EXIT;` |
| 清除 TRACE_EXIT | `forkexit.c` | 769 | `child->mp_flags &= ~TRACE_EXIT;` |

---

## 五、特殊场景

### 5.1 EXITING 状态的组合

根据 `forkexit.c:374-375`，EXITING 状态可以同时有以下组合：

```c
rmp->mp_flags &= (IN_USE|VFS_CALL|PRIV_PROC|TRACE_EXIT|PROC_STOPPED);
rmp->mp_flags |= EXITING;
```

**合法组合示例**：
- `IN_USE | EXITING`
- `IN_USE | EXITING | VFS_CALL`
- `IN_USE | EXITING | PROC_STOPPED`
- `IN_USE | EXITING | TRACE_EXIT`
- `IN_USE | EXITING | PRIV_PROC`
- `IN_USE | EXITING | VFS_CALL | PROC_STOPPED`

### 5.2 WAITING 是父进程状态

根据 `forkexit.c:582`：

```c
parent_waiting = rmp->mp_flags & WAITING;
```

**关键点**：
- `WAITING` 是父进程的状态，不是子进程的状态
- 父进程执行 `wait4()` 时设置 `WAITING`
- 子进程退出后，父进程的 `WAITING` 被清除

### 5.3 VFS_CALL 与 PROC_STOPPED 的关系

根据 `signal.c:672-677`：

```c
if (rmp->mp_flags & (VFS_CALL | EVENT_CALL)) {
    assert(rmp->mp_flags & PROC_STOPPED);
}
```

**关键点**：
- 如果进程在等待 VFS 或事件订阅者，它必须被停止
- 这是为了防止进程在 VFS 回复后立即执行新的调用

### 5.4 DELAY_CALL 与 PROC_STOPPED 的互斥

根据 `signal.c:237`：

```c
assert(!(rmp->mp_flags & (PROC_STOPPED | DELAY_CALL | UNPAUSED)));
```

**关键点**：
- `DELAY_CALL` 表示进程正在发送消息，无法立即停止
- 一旦消息发送完成，内核会发送 `SIGSNDELAY`
- 此时 `DELAY_CALL` 被清除，`PROC_STOPPED` 被设置

---

## 六、Rust 映射建议

### 6.1 Lifecycle enum

```rust
pub enum Lifecycle {
    Unused,
    Running,
    Exiting { exit_code: i8, sig_status: i8 },
    TraceZombie { exit_code: i8, sig_status: i8 },
    Zombie { exit_code: i8, sig_status: i8 },
    ToldParent { exit_code: i8, sig_status: i8 },
}
```

### 6.2 BlockState struct

```rust
pub struct BlockState {
    pub stopped: bool,              // PROC_STOPPED
    pub ipc_blocked: Option<IpcBlockReason>,  // VFS_CALL / EVENT_CALL / DELAY_CALL
    pub unpaused: bool,             // UNPAUSED
}

pub enum IpcBlockReason {
    VfsCall,
    EventCall,
    DelayedSignal,
}
```

### 6.3 WaitState struct

```rust
pub struct WaitState {
    pub waiting: bool,              // WAITING (父进程状态)
    pub target: WaitTarget,         // mp_wpid
    pub rusage_addr: VirBytes,      // mp_waddr
}
```

### 6.4 TraceState struct

```rust
pub struct TraceState {
    pub stopped: bool,              // TRACE_STOPPED
    pub exit: bool,                 // TRACE_EXIT
}
```

---

## 七、验证清单

- [x] 所有 19 个 flags 已记录
- [x] 互斥组合已识别
- [x] 必须组合已识别
- [x] 可选组合已识别
- [x] 生命周期转换路径已记录
- [x] 阻塞状态转换路径已记录
- [x] 追踪状态转换路径已记录
- [x] 源码引用已添加
- [x] 特殊场景已说明
- [x] Rust 映射建议已提供
