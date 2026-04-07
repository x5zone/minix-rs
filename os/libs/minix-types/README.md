# minix-types

> Minix3 process management types for `no_std` environment

## 概述

本 crate 提供了 Minix3 进程管理核心类型的 Rust 实现，专为 `no_std` 环境设计，仅支持 64 位系统。

## 设计理念

### 类型安全优先

将 C 语言的位标志（bitflags）转换为 Rust 的枚举和结构体，利用类型系统防止非法状态组合。

### 状态分离

Minix3 的 `mp_flags` 字段混合了多种状态：
- **生命周期状态**（互斥）：`IN_USE`, `EXITING`, `ZOMBIE`, `TRACE_ZOMBIE`, `TOLD_PARENT`
- **阻塞状态**（可组合）：`PROC_STOPPED`, `VFS_CALL`, `EVENT_CALL`, `DELAY_CALL`, `UNPAUSED`
- **父进程状态**：`WAITING`
- **追踪状态**：`TRACE_STOPPED`, `TRACE_EXIT`
- **权限状态**：`PRIV_PROC`

本实现将这些状态分离到不同的结构体中，使语义更加清晰。

## 类型映射

### C → Rust 类型映射

| C 类型 | 32位 | 64位 | Rust 类型 |
|--------|------|------|-----------|
| `pid_t` | 4 bytes | 4 bytes | `i32` |
| `uid_t` | 4 bytes | 4 bytes | `u32` |
| `gid_t` | 4 bytes | 4 bytes | `u32` |
| `clock_t` | 4 bytes | 8 bytes | `i64` |
| `endpoint_t` | 4 bytes | 4 bytes | `i32` |
| `vir_bytes` | 4 bytes | 8 bytes | `u64` |
| `sigset_t` | 4/8 bytes | 8 bytes | `u64` |

### mp_flags → Rust 映射

| C Flag | 值 | Rust 映射 | 说明 |
|--------|-----|----------|------|
| `IN_USE` | 0x00001 | `Lifecycle::!Unused` | 槽位使用中 |
| `WAITING` | 0x00002 | `WaitState::waiting` | ⚠️ 父进程状态 |
| `ZOMBIE` | 0x00004 | `Lifecycle::Zombie` | 僵尸状态 |
| `PROC_STOPPED` | 0x00008 | `BlockState::stopped` | ⚠️ 可与 Running/Exiting 组合 |
| `ALARM_ON` | 0x00010 | `RemainingFlags::ALARM_ON` | 定时器 |
| `EXITING` | 0x00020 | `Lifecycle::Exiting` | 正在退出 |
| `TOLD_PARENT` | 0x00040 | `Lifecycle::ToldParent` | 已通知父进程 |
| `TRACE_STOPPED` | 0x00080 | `TraceState::stopped` | 追踪停止 |
| `SIGSUSPENDED` | 0x00100 | `SignalState::suspended` | 信号挂起 |
| `VFS_CALL` | 0x00400 | `BlockState::ipc_blocked` | ⚠️ 可与 Exiting 组合 |
| `NEW_PARENT` | 0x00800 | `RemainingFlags::NEW_PARENT` | 父进程变更 |
| `UNPAUSED` | 0x01000 | `BlockState::unpaused` | VFS 已回复 |
| `PRIV_PROC` | 0x02000 | `Privilege::Kernel` | 系统进程 |
| `PARTIAL_EXEC` | 0x04000 | `RemainingFlags::PARTIAL_EXEC` | 部分执行 |
| `TRACE_EXIT` | 0x08000 | `Guardianship::Traced.trace_exit` | 追踪退出 |
| `TRACE_ZOMBIE` | 0x10000 | `Lifecycle::TraceZombie` | 追踪僵尸 |
| `DELAY_CALL` | 0x20000 | `BlockState::ipc_blocked` | 延迟调用 |
| `TAINTED` | 0x40000 | `RemainingFlags::TAINTED` | 污染标记 |
| `EVENT_CALL` | 0x80000 | `BlockState::ipc_blocked` | 事件订阅 |

## 核心类型

### Lifecycle（生命周期）

```rust
use minix_types::*;

pub enum Lifecycle {
    Unused,
    Running,
    Exiting { exit_code: i8, sig_status: i8 },
    TraceZombie { exit_code: i8, sig_status: i8 },
    Zombie { exit_code: i8, sig_status: i8 },
    ToldParent { exit_code: i8, sig_status: i8 },
}
```

状态转换：
```text
Unused → Running → Exiting → (TraceZombie)? → Zombie → ToldParent → Unused
```

### BlockState（阻塞状态）

```rust
use minix_types::*;

pub struct BlockState {
    pub stopped: bool,              // PROC_STOPPED
    pub ipc_blocked: Option<IpcBlockReason>,  // VFS_CALL / EVENT_CALL / DELAY_CALL
    pub unpaused: bool,              // UNPAUSED
}

pub enum IpcBlockReason {
    VfsCall,
    EventCall,
    DelayedSignal,
}
```

**关键约束**：
- `PROC_STOPPED` 可以和 `EXITING` 组合
- `VFS_CALL` 可以和 `EXITING` 组合

### WaitState（等待状态）

```rust
use minix_types::*;

pub struct WaitState {
    pub waiting: bool,              // WAITING（父进程状态）
    pub target: WaitTarget,         // mp_wpid
    pub rusage_addr: VirBytes,      // mp_waddr
}

pub enum WaitTarget {
    AnyChild,
    SpecificChild(Pid),
    Group(Pid),
}
```

**⚠️ 重要**：`WAITING` 是父进程的状态，不是子进程的状态！

### Guardianship（监护关系）

```rust
use minix_types::*;

pub enum Guardianship {
    Normal { parent: ProcIndex },
    Traced {
        parent: ProcIndex,
        tracer: ProcIndex,
        trace_exit: bool,
        trace_options: TraceOptions,
    },
}
```

**改进点**：在 `Normal` 状态下没有 `tracer` 字段，防止误操作。

### Process（进程）

```rust
use minix_types::*;

pub struct Process {
    pub identity: ProcessIdentity,
    pub state: ProcessState,
    pub resources: ProcessResources,
    pub ipc: ProcessIpc,
}
```

## 使用示例

### 创建进程

```rust
use minix_types::*;

let mut proc = Process::new(0, 1234);
proc.state.lifecycle = Lifecycle::Running;

assert!(proc.is_in_use());
assert!(!proc.is_zombie());
```

### 状态转换

```rust
use minix_types::*;

let mut proc = Process::new(0, 1234);

// 进程开始退出
proc.state.lifecycle = Lifecycle::Exiting {
    exit_code: 0,
    sig_status: 0,
};

// 同时等待 VFS
proc.state.block.ipc_blocked = Some(IpcBlockReason::VfsCall);
proc.state.block.stopped = true;

assert!(proc.is_exiting());
assert!(proc.is_stopped());
```

### 检查进程状态

```rust
use minix_types::*;

let proc = Process::new(0, 1234);

// 检查是否是系统进程
if proc.resources.privilege.is_kernel() {
    // 系统进程有特殊权限
}

// 检查是否有 tracer
if let Some(tracer) = proc.tracer() {
    // 进程正在被追踪
}
```

## 测试

```bash
cargo test -p minix-types
```

## no_std 兼容

本 crate 完全支持 `no_std` 环境：

```rust,ignore
#![no_std]

use minix_types::process::Process;

// 可以在 no_std 环境中使用
```

## 参考

- Minix3 源码： `minix/servers/pm/mproc.h`
- 状态分析： `notes/rewrite/fork-syscall-rewrite/mp-flags-analysis.md`
- 重写方案： `notes/rewrite/fork-syscall-rewrite/fork-rewrite-01.md`
