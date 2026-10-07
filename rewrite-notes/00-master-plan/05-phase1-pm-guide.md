# Phase 1: PM 层实现指南

## 1. 阶段目标

建立 PM（Process Manager）进程管理的数据基础，实现 fork 系统调用的 PM 层核心逻辑。

> **范围限定**: 本阶段只处理 **PM 的 mproc 表**。PM 与 VM/VFS/Kernel 之间的 IPC 通信为真实实现，仅硬件访问（寄存器、FPU、磁盘IO、MMU）使用 Mock。

## 2. 关键任务

### 任务 2.1: MProc 结构体定义

**目标**: 定义 Rust 版本的 `mproc` 结构体

**接口定义**:
```rust
// os/servers/pm/src/mproc/mproc.rs
pub struct MProc {
    pub pid: Pid,                    // 进程 ID
    pub endpoint: Endpoint,          // 内核端点
    pub parent: SlotIndex,           // 父进程槽位索引
    pub flags: ProcessFlags,         // 状态标志
    pub exit_status: i8,             // 退出状态
    pub sig_status: i8,              // 信号状态
    pub eff_uid: Uid,                // 有效用户ID
}
```

**设计决策**:
- MProc 放在 PM crate 中，而非 minix-types
- 使用 Rust 类型替代 C 类型（`pid_t` → `Pid`, `endpoint_t` → `Endpoint`）
- 使用 `Option<NonZeroU32>` 表达可选字段

**参考文档**: [../01-stage-pm/mproc-design.md](../01-stage-pm/mproc-design.md)

### 任务 2.1.1: 为什么 MProc 放在 PM crate 而不是 minix-types？

#### 原因分析

1. **职责隔离（Domain Separation）**
   - MProc 包含大量仅 PM 关心的私有逻辑（信号处理、父子进程树等）
   - 如果放入 minix-types，意味着 VFS、VM 甚至 Init 进程在引用公共库时，都不得不背负 PM 的私有业务逻辑
   - 这违背了微内核"知识最小化"的原则

2. **不变量保护（Invariants Protection）**
   - MProc 的状态转换（如从 Running 到 Zombie）通常绑定了 PM 内部的复杂逻辑
   - 如果放在公共库，外部 crate 理论上可以构造一个非法的 MProc 实例
   - 留在 PM 内部，可以利用 Rust 的 `pub(crate)` 严格限制谁能修改这些关键字段

3. **微内核原则**
   - 遵循"知识最小化"原则，其他服务不需要了解 PM 的内部实现
   - 如果 VFS 需要通过某种手段查看 PM 的状态（比如 /proc 文件系统），应该定义一个新的、精简的 `PublicProcessInfo` 结构体放在公共库

### 任务 2.1.2: 核心认知 - mproc 的四个角色

`mproc` 是一个**非常典型的"C 时代一锅炖状态结构"**：
👉 所有语义（生命周期 / 权限 / 信号 / IPC / 调度）都压在一个 struct + bitflag 里

在 Rust 里，**这 4 个应该拆开，否则你会复刻"巨型 struct 地狱"**：

1. **Process Identity**（pid / parent / endpoint）
2. **Process State Machine**（flags）
3. **Process Resources**（uid/gid/signal/timer）
4. **IPC Context**（mp_reply / VFS_CALL 等）

### 任务 2.1.3: 重构的四个层次

| 层 | 名字 | 说明 | 当前位置 |
|---|---|---|---|
| L1 | **Translation**（翻译） | 直接 1:1 翻译 C 代码 | ❌ 不做 |
| L2 | **Semantic Preservation**（语义保持） | 外部语义不变，内部表达优化 | ✅ **你现在** |
| L3 | **Model Extraction**（模型提取） | 提取隐式状态机，显式化模型 | 🔜 **下一步** |
| L4 | **Redesign**（重新设计） | 改变系统架构/机制 | 🚀 未来 |

**关键原则**:
> 👉 **外部语义不变（observable behavior）**
> 👉 **内部表达可以改变（internal model）**

### 任务 2.1.3.1: 五种重构方案对比

在重构 `mproc` 结构体时，有五种不同的方案可选，从"保守复刻"到"地道 Rust"，你可以根据目前的心理预期和学习压力来选择。

---

#### 方案一：直接翻译方案（Raw Porting）

**核心思路**：几乎原样搬运 C 的结构，但在全局管理上使用 Rust 的安全封装。

```rust
#[repr(C)]
pub struct MProc {
    pub mp_exitstatus: i8,
    pub mp_pid: Pid,           // i32
    pub mp_parent: ProcIndex,  // i32
    pub mp_flags: u32,
    pub mp_child_utime: Clock, // i64 (64 位系统)
    pub mp_child_stime: Clock, // i64 (64 位系统)
    // ... 其他字段
}

// 基建：使用全局静态数组
pub static MPROC_TABLE: RwLock<[MProc; NR_PROCS]> = RwLock::new([MProc::DEFAULT; NR_PROCS]);
```

**优点**：
- ✔ **心智负担最低**：C 代码里写 `mproc[i].mp_pid`，你这就写 `table[i].mp_pid`，逻辑一一对应
- ✔ **迁移速度快**：不需要重新设计状态机
- ✔ **完全兼容 no_std**：不使用任何标准库功能
- ✔ **性能稳定**：内存布局与 C 完全一致，无额外开销
- ✔ **易于验证**：可以直接与 C 代码对比验证

**缺点**：
- ❌ **状态不安全**：你可能在进程还是 `UNUSED` 状态时，错误地读取了它的 `mp_timer`
- ❌ **字段冗余**：所有的进程槽位都预留了巨大的内存（比如信号数组），即便这个槽位没被使用
- ❌ **安全性低**：大量使用 unsafe 和全局可变状态
- ❌ **维护困难**：缺乏 Rust 特色的类型安全和所有权管理
- ❌ **语义混乱**：flags 仍然是"语义混乱"，不可避免逻辑 bug（状态组合非法）

**适用场景**：
- 快速原型验证
- 对 C 代码进行最小改动迁移
- 性能敏感且需要与 C 直接交互的场景

**建议**：
> 👉 **适合当前阶段（强烈建议起步用这个）**

---

#### 方案二：状态机拆分（推荐进阶）

**核心思路**：用 **enum 替代 flags**，将分散的状态位聚合为类型安全的枚举，**语义聚合 + 编译期安全**。

> ⚠️ **基于 Minix3 源码约束修正**：状态之间存在复杂的组合关系，需要分离"生命周期"和"阻塞状态"。

**源码约束分析**（来自 `forkexit.c`, `signal.c` 源码）：

```c
// EXITING 可以同时有多个 flag（forkexit.c:374-375）
rmp->mp_flags &= (IN_USE|VFS_CALL|PRIV_PROC|TRACE_EXIT|PROC_STOPPED);
rmp->mp_flags |= EXITING;

// PROC_STOPPED 是独立状态，可以和 RUNNING/EXITING 组合（signal.c:246）
rmp->mp_flags |= PROC_STOPPED;

// WAITING 是父进程状态，不是子进程状态（forkexit.c:582）
parent_waiting = rmp->mp_flags & WAITING;

// ZOMBIE / TRACE_ZOMBIE / TOLD_PARENT 是生命周期转换（forkexit.c:603-604）
if (rmp->mp_flags & (TRACE_ZOMBIE | ZOMBIE))
    panic("zombify: process was already a zombie");
```

**约束总结**：
| Flag | 约束 |
|------|------|
| `EXITING` | 可以同时有 `VFS_CALL`, `PROC_STOPPED`, `TRACE_EXIT` |
| `PROC_STOPPED` | 独立状态，可与 `RUNNING` 或 `EXITING` 组合 |
| `WAITING` | 父进程状态，不是子进程状态 |
| `ZOMBIE` | 与 `TRACE_ZOMBIE` 互斥 |
| `TOLD_PARENT` | 只能在 `ZOMBIE` 之后 |

**核心设计**：分离"生命周期"和"阻塞状态"

```rust
/// 进程生命周期（互斥）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lifecycle {
    Unused,      // 槽位未使用
    Running,     // 正常运行中
    Exiting {..},// 正在退出
    Zombie {..}, // 僵尸状态
    ToldParent {..}, // 已通知父进程
}

/// 阻塞状态（可以和生命周期组合）
#[derive(Debug, Clone, Copy, Default)]
pub struct BlockState {
    pub stopped: bool,              // PROC_STOPPED
    pub ipc_blocked: Option<..>,    // VFS_CALL / EVENT_CALL
    pub unpaused: bool,             // UNPAUSED
}
```

**优点**：
- ✔ **编译期安全**：生命周期互斥，不可能同时是 `Zombie` 和 `Exiting`
- ✔ **语义清晰**：`WAITING` 是父进程状态，不是子进程状态
- ✔ **类型安全**：杜绝在非调试状态下误操作 `tracer` 字段

**缺点**：
- ❌ **和原代码不再 1:1**（需要适配）

---

#### 方案三：分层抽象模型（推荐）

**核心思路**：结合 **直接翻译** 和 **类型安全** 的优点，采用三层架构，**语义保持 + 模型显化**。

```
┌─────────────────────────────────────────┐
│  Layer 3: 高层 API (ProcessManager)     │
│  - 类型安全的操作接口                    │
│  - 状态机验证                           │
├─────────────────────────────────────────┤
│  Layer 2: 进程槽位 (ProcessSlot)        │
│  - Option<MProc> 封装                   │
│  - 空槽位零成本                         │
├─────────────────────────────────────────┤
│  Layer 1: 原始结构 (MProc)              │
│  - #[repr(C)] 兼容布局                  │
│  - 与 C 代码字段一一对应                 │
└─────────────────────────────────────────┘
```

**优点**：
- ✔ **性能与安全平衡**：核心部分高性能，高层部分安全
- ✔ **渐进式重构**：可以逐步替换旧代码
- ✔ **与 C 对应清晰**：Layer 1 与 C 代码字段一一对应，便于对照
- ✔ **类型安全**：Layer 2-3 提供类型安全封装
- ✔ **no_std 兼容**：所有组件都支持 no_std 环境
- ✔ **灵活性高**：可以根据需要调整各部分实现

**缺点**：
- ❌ **实现复杂**：需要维护多套接口
- ❌ **代码冗余**：部分功能可能有重复实现
- ❌ **学习曲线**：需要理解多种设计模式

**适用场景**：
- 现有 C 内核的 Rust 重写（推荐用于生产环境）
- 需要逐步替换旧代码，降低风险
- 需要平衡性能、安全性和可维护性

**推荐路线**：
| 阶段 | 方案 |
|------|------|
| 当前 rewrite | 🥇 方案一 |
| 稳定后 | 🥈 方案二/三 |
| redesign | 🥉 方案四 |

---

#### 方案四：Typestate（编译期状态机）

**核心思路**：用类型系统保证状态合法性。

```rust
struct Process<S> {
    id: ProcessId,
    state: S,
}

struct Running;
struct Zombie;
struct Exiting;

impl Process<Running> {
    fn exit(self) -> Process<Exiting> { ... }
}
```

**优点**：
- ✔ **编译期保证状态合法**
- ✔ 完全消灭非法状态

**缺点（致命，不适合本项目）**：

1. **和 OS 动态状态模型冲突**
   - 状态是外部驱动的（消息接收），不是你"控制"的
   - 你拿到的是"未知状态"，被迫退化成 enum

2. **状态是"位图组合"，不是"单一状态"**
   - Minix 的本质：`mp_flags = IN_USE | WAITING | VFS_CALL`
   - 状态数量 = 2^N，组合爆炸

3. **进程表是"同构数组"，Typestate 是"异构对象"**
   - 破坏 O(1) index → process 的核心性质
   - 动态分派导致性能下降
   - cache locality 消失

4. **IPC = 分布式系统 → Typestate 彻底失效**
   - 状态来自远程，可能延迟、乱序
   - 只能用 enum，不能用 typestate

5. **BKL 反而"进一步否定 Typestate"**
   - 保留 BKL = 单线程 PM
   - Typestate 的最大收益点（compile-time 并发安全）直接归零

**结论**：
> 👉 **适合论文，不适合你这个项目**

---

#### 方案五：组合式结构

**核心思路**：按功能模块拆分字段，保持扁平结构。

```rust
pub struct Process {
    pub id: ProcessId,
    pub parent: Option<ProcessId>,
    pub lifecycle: Lifecycle,      // 核心模型
    pub signals: SignalState,      // 信号系统
    pub creds: Credentials,        // 权限
    pub ipc: IpcState,             // IPC 状态
    pub flags: ProcessFlags,       // 剩余 flag
}
```

**优点**：
- ✔ **代码整洁**：模块化设计
- ✔ **易于测试**：各模块独立测试
- ✔ **语义清晰**：每个组件职责明确

**缺点**：
- ❌ **内存布局改变**：不再与 C 兼容
- ❌ **需要适配层**：与 C 代码对照困难

**适用场景**：
- 追求代码整洁和可维护性
- 不需要与 C 代码保持内存兼容

---

#### 选型建议

| 你的目标 | 推荐方案 |
|----------|----------|
| 快速看到结果，减少挫败感 | **方案一（直接翻译）** |
| 学习 Rust 类型系统，追求极致安全 | **方案二（状态机拆分）** |
| 代码整洁，易于测试 | **方案三（分层抽象）** |
| 生产环境，平衡性能与安全 | **方案三（分层抽象）** |

**本项目推荐路线**：
1. **Rewrite 阶段**：从方案一开始，快速实现语义等价
2. **稳定后**：逐步迁移到方案二或三，提升类型安全
3. **Redesign 阶段**：根据实际需求选择方案四或五

### 任务 2.1.4: 64 位系统类型设计

| C 类型 | 32位大小 | 64位大小 | Rust 类型 | 说明 |
|--------|---------|---------|-----------|------|
| `char` | 1 byte | 1 byte | `i8` / `u8` | 不变 |
| `short` | 2 bytes | 2 bytes | `i16` | 不变 |
| `int` | 4 bytes | 4 bytes | `i32` | 不变 |
| `long` | 4 bytes | **8 bytes** | `i64` | ⚠️ **变化** |
| `long long` | 8 bytes | 8 bytes | `i64` | 不变 |
| `pointer` | 4 bytes | **8 bytes** | `*mut T` | ⚠️ **变化** |
| `size_t` | 4 bytes | **8 bytes** | `usize` | ⚠️ **变化** |
| `pid_t` | 4 bytes | 4 bytes | `i32` | 不变 |
| `uid_t` | 4 bytes | 4 bytes | `u32` | 不变 |
| `gid_t` | 4 bytes | 4 bytes | `u32` | 不变 |
| `clock_t` | 4 bytes | **8 bytes** | `i64` | ⚠️ **变化** |
| `time_t` | 4 bytes | **8 bytes** | `i64` | ⚠️ **变化** |
| `off_t` | 4 bytes | **8 bytes** | `i64` | ⚠️ **变化** |
| `sigset_t` | 4/8 bytes | 8 bytes | `u64` | 统一为 64 位 |

#### 2.1 进程生命周期（Lifecycle）

```rust
/// 进程生命周期（互斥）
/// 
/// 状态转换：
/// Running → Exiting → (TraceZombie)? → Zombie → ToldParent
///                     ↓
///                   TraceZombie → Zombie → ToldParent
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lifecycle {
    /// 槽位未使用
    Unused,
    
    /// 正常运行中（可能同时有 stopped=true）
    Running,
    
    /// 正在退出（可能同时有 VFS_CALL, PROC_STOPPED, TRACE_EXIT）
    Exiting {
        exit_code: i8,
        sig_status: i8,
    },
    
    /// 僵尸状态：等待 tracer 收尸（tracer != parent 时）
    /// 对应 TRACE_ZOMBIE flag
    TraceZombie {
        exit_code: i8,
        sig_status: i8,
    },
    
    /// 僵尸状态：等待父进程收尸
    /// 对应 ZOMBIE flag
    Zombie {
        exit_code: i8,
        sig_status: i8,
    },
    
    /// 已通知父进程，等待清理
    /// 对应 TOLD_PARENT flag
    ToldParent {
        exit_code: i8,
        sig_status: i8,
    },
}
```

**对应的 C flags**：
- `IN_USE` → `Lifecycle::!Unused`
- `EXITING` → `Lifecycle::Exiting`
- `TRACE_ZOMBIE` → `Lifecycle::TraceZombie`
- `ZOMBIE` → `Lifecycle::Zombie`
- `TOLD_PARENT` → `Lifecycle::ToldParent`

#### 2.2 阻塞状态（BlockState）

```rust
/// 阻塞状态（可以和生命周期组合）
/// 
/// 关键：PROC_STOPPED 和 IPC 阻塞是独立的
#[derive(Debug, Clone, Copy, Default)]
pub struct BlockState {
    /// 是否在内核中停止（PROC_STOPPED）
    /// 可以和 Running / Exiting 组合
    pub stopped: bool,
    
    /// IPC 阻塞原因（VFS_CALL / EVENT_CALL / DELAY_CALL）
    pub ipc_blocked: Option<IpcBlockReason>,
    
    /// VFS 已回复 unpause 请求（UNPAUSED）
    pub unpaused: bool,
}

/// IPC 阻塞原因（互斥）
#[derive(Debug, Clone, Copy)]
pub enum IpcBlockReason {
    /// 等待 VFS 回复（VFS_CALL）
    VfsCall,
    /// 等待进程事件订阅者（EVENT_CALL）
    EventCall,
    /// 等待调用完成后再发送信号（DELAY_CALL）
    DelayedSignal,
}
```

**对应的 C flags**：
- `PROC_STOPPED` → `BlockState::stopped = true`
- `VFS_CALL` → `BlockState::ipc_blocked = Some(VfsCall)`
- `EVENT_CALL` → `BlockState::ipc_blocked = Some(EventCall)`
- `DELAY_CALL` → `BlockState::ipc_blocked = Some(DelayedSignal)`
- `UNPAUSED` → `BlockState::unpaused = true`

#### 2.3 父进程等待状态（WaitState）

```rust
/// 父进程等待状态（放在父进程，不是子进程！）
/// 
/// 对应 WAITING flag 和 mp_wpid 字段
#[derive(Debug, Clone, Default)]
pub struct WaitState {
    /// 是否正在等待子进程（WAITING）
    pub waiting: bool,
    
    /// 等待目标（mp_wpid）
    pub target: WaitTarget,
    
    /// rusage 地址（mp_waddr）
    pub rusage_addr: VirBytes,
}

/// 等待目标（互斥）
#[derive(Debug, Clone, Copy)]
pub enum WaitTarget {
    /// wait() - 等待任意子进程
    AnyChild,
    /// waitpid(pid) - 等待特定子进程
    SpecificChild(Pid),
    /// waitpid(-pgrp) - 等待进程组
    Group(Pid),
}
```

**对应的 C 字段**：
- `WAITING` → `WaitState::waiting = true`
- `mp_wpid` → `WaitState::target`
- `mp_waddr` → `WaitState::rusage_addr`

#### 2.4 信号处理状态（SignalState）

```rust
/// 信号处理状态
#[derive(Debug, Clone)]
pub struct SignalState {
    pub mask: SigSet,           // mp_sigmask
    pub mask_saved: SigSet,     // mp_sigmask2
    pub pending: SigSet,        // mp_sigpending
    pub kernel_pending: SigSet, // mp_ksigpending
    pub trace_mask: SigSet,     // mp_sigtrace
    
    /// SIGSUSPENDED flag
    pub suspended: bool,
    
    /// sigreturn 函数地址（mp_sigreturn）
    pub sigreturn_addr: VirBytes,
    
    /// 信号处理函数（延迟加载）
    pub handlers: SignalHandlers,
}

/// 信号处理器（优化内存）
pub enum SignalHandlers {
    /// 默认：全部使用默认处理
    Default,
    /// 部分自定义：指向外部数组
    Custom(&'static mut [SigAction; _NSIG]),
}
```

#### 2.5 监护关系（Guardianship）

```rust
/// 监护关系（解决 parent vs tracer 的杂糅）
#[derive(Debug, Clone)]
pub enum Guardianship {
    /// 正常状态：只有一个父进程
    Normal { parent: ProcIndex },
    
    /// 调试状态：被 tracer 劫持
    /// tracer 可能 != parent
    Traced {
        parent: ProcIndex,
        tracer: ProcIndex,
        /// TRACE_EXIT flag：tracer 正在强制进程退出
        trace_exit: bool,
        /// 追踪选项（mp_trace_flags）
        /// TO_TRACEFORK: 自动 attach 到 fork 的子进程
        /// TO_ALTEXEC: exec 成功时发送 SIGSTOP
        /// TO_NOEXEC: exec 成功时不发送信号
        trace_options: TraceOptions,
    },
}

/// 追踪选项（对应 mp_trace_flags）
bitflags::bitflags! {
    pub struct TraceOptions: u32 {
        const TRACEFORK = 0x1;  // TO_TRACEFORK
        const ALTEXEC = 0x2;    // TO_ALTEXEC
        const NOEXEC = 0x4;     // TO_NOEXEC
    }
}
```

**对应的 C 字段**：
- `mp_parent` → `Guardianship::Normal { parent }` 或 `Traced { parent, .. }`
- `mp_tracer` → `Guardianship::Traced { tracer, .. }`
- `TRACE_EXIT` → `Guardianship::Traced { trace_exit: true, .. }`
- `mp_trace_flags` → `Guardianship::Traced { trace_options, .. }`
- `NO_TRACER (-1)` → `Guardianship::Normal`

**改进点**：杜绝在非调试状态下误操作 `tracer` 字段。

#### 2.6 追踪状态（TraceState）

```rust
/// 追踪状态（独立于监护关系）
/// 
/// TRACE_STOPPED 是进程因追踪而停止的状态
/// 可以和 Running / Exiting 组合
#[derive(Debug, Clone, Default)]
pub struct TraceState {
    /// 是否因追踪而停止（TRACE_STOPPED）
    pub stopped: bool,
}
```

**对应的 C flags**：
- `TRACE_STOPPED` → `TraceState::stopped = true`

#### 2.7 权限模型（Privilege）

```rust
/// 特权级别
#[derive(Debug, Clone)]
pub enum Privilege {
    /// 普通用户进程
    User(Credentials),
    
    /// 系统进程（PRIV_PROC）
    /// 系统进程有特殊权限，退出时不需要等待 VFS
    Kernel,
}

/// 权限凭证
#[derive(Debug, Clone)]
pub struct Credentials {
    pub real_uid: Uid,
    pub eff_uid: Uid,
    pub real_gid: Gid,
    pub eff_gid: Gid,
    pub ngroups: usize,
    pub groups: [Gid; NGROUPS_MAX],
}
```

**对应的 C 字段**：
- `PRIV_PROC` → `Privilege::Kernel`
- `mp_realuid/mp_effuid/mp_realgid/mp_effgid` → `Credentials`
- `mp_ngroups/mp_groups` → `Credentials::groups`

---

### 任务 2.1.5: 进程表存储方案对比

在重构进程表时，有多种存储方案可选，各有优缺点：

| 方案 | 类型签名 | 内存布局 | 分配时机 | no_std 兼容 |
|------|---------|---------|---------|------------|
| **A. 静态数组** | `[Process; NR_PROCS]` | 连续 | 编译期 | ✅ 完美 |
| **B. MaybeUninit 数组** | `[MaybeUninit<Process>; NR_PROCS]` | 连续 | 编译期 | ✅ 完美 |
| **C. Vec** | `Vec<Process>` | 连续 | 运行期 | ⚠️ 需要分配器 |
| **D. Slab** | `slab::Slab<Process>` | 分散 | 运行期 | ⚠️ 需要外部 crate |
| **E. BTreeMap** | `BTreeMap<usize, Process>` | 分散 | 运行期 | ⚠️ 需要分配器 |

#### 推荐方案：静态数组
```rust
/// 进程表大小
pub const NR_PROCS: usize = 256;

/// 保留给 root 的槽位数
pub const LAST_FEW: usize = 5;

/// 进程表（静态数组）
pub struct ProcTable {
    /// 进程数组
    procs: [Process; NR_PROCS],
    /// 当前使用的进程数
    procs_in_use: Cell<usize>,
    /// 下一个子进程槽位（轮询算法）
    next_child: Cell<usize>,
}
```
**理由**：
1. ✅ 与 Minix3 原设计一致，固定大小进程表
2. ✅ 零运行时分配，编译期确定大小
3. ✅ 连续内存布局，缓存友好，访问性能最优
4. ✅ 完美兼容 no_std 环境，不需要分配器
5. ✅ 实现最简单，易于验证

---

### 任务 2.2: 进程表管理

**目标**: 建立 PM 进程表，实现槽位分配与释放

**接口定义**:
```rust
// os/servers/pm/src/mproc/table.rs
pub struct ProcTable {
    slots: [Option<MProc>; NR_PROCS],
    in_use: Cell<usize>,
    next_child: Cell<usize>,
}

impl ProcTable {
    pub fn alloc_slot(&self) -> Result<SlotIndex, Error>;
    pub fn free_slot(&self, slot: SlotIndex);
    pub fn get(&self, slot: SlotIndex) -> Option<&MProc>;
    pub fn get_mut(&self, slot: SlotIndex) -> Option<&mut MProc>;
}
```

**关键算法**:
- 槽位查找：轮询算法 `(next_child + 1) % NR_PROCS`
- 满表检查：`procs_in_use >= NR_PROCS`
- 特权检查：`procs_in_use >= NR_PROCS - LAST_FEW && !is_root`

---

### 任务 2.3: PID 生成器

**目标**: 实现 `get_free_pid` 函数

**接口定义**:
```rust
// os/servers/pm/src/mproc/pid_gen.rs
pub struct PidGenerator {
    next_pid: Cell<u32>,
}

impl PidGenerator {
    pub fn new() -> Self;
    pub fn alloc(&self, table: &ProcTable) -> Pid;
}
```

**算法逻辑**:
1. `next_pid` 循环递增（达到 NR_PIDS 后回到 INIT_PID+1）
2. 遍历进程表检查 PID 冲突
3. 检查 `mp_pid` 和 `mp_procgrp` 两个字段

**参考文档**: [../01-stage-pm/pid-generator.md](../01-stage-pm/pid-generator.md)

---

### 任务 2.4: do_fork 实现

**目标**: 实现完整的 `do_fork` 函数

**接口定义**:
```rust
// os/servers/pm/src/mproc/fork.rs
pub fn do_fork(ctx: &mut PmContext) -> Result<Pid, Error>;
```

**执行流程**:
1. 检查进程表是否已满 → 可能返回 `EAGAIN`
2. 查找空闲槽位 → 轮询算法
3. 调用 VM fork（真实 IPC）→ 获取 child_endpoint
4. 复制父进程 mproc 到子进程槽位
5. 设置父子关系（`mp_parent = who_p`）
6. 清除/重置特定字段（trace_flags, child_utime 等）
7. 分配 PID

**错误处理**:
- `EAGAIN`: 进程表满
- `ENOMEM`: 内存不足
- `EPERM`: 权限不足

**参考文档**: [../01-stage-pm/do-fork-impl.md](../01-stage-pm/do-fork-impl.md)

---

### 任务 2.5: 调用 VM Fork（PM 侧）

**目标**: PM 通过真实 IPC 调用 VM 的 fork 服务

> **重要**: 
> - PM 与 VM 之间的 IPC 调用是**真实实现**
> - VM 内部逻辑（地址空间克隆、CoW）是**真实实现**
> - 仅 VM 访问的硬件（MMU、物理内存）使用 Mock

**接口定义**:
```rust
// os/servers/pm/src/vm_client.rs
pub struct VmClient;

impl VmClient {
    /// 调用 VM fork 服务
    /// 
    /// # 参数
    /// - parent_ep: 父进程 endpoint
    /// - child_slot: 子进程槽位索引
    ///
    /// # 返回值
    /// - Ok(Endpoint): 子进程的 endpoint（由 VM 生成）
    /// - Err(Error): VM 返回的错误
    pub fn fork(
        &self,
        parent_ep: Endpoint,
        child_slot: SlotIndex,
    ) -> Result<Endpoint, Error> {
        // 构造 IPC 消息
        let req = VmForkRequest {
            parent_endpoint: parent_ep,
            child_slot,
        };
        
        // 发送给 VM 服务（真实 IPC）
        let resp: VmForkResponse = ipc_call(VM_SERVICE, &req)?;
        
        if resp.status == 0 {
            Ok(resp.child_endpoint)
        } else {
            Err(Error::from_raw(resp.status))
        }
    }
}
```

**关键点**:
- PM 构造消息并通过真实 IPC 发送给 VM
- VM 接收消息并执行真实的 fork 逻辑
- VM 内部的 MMU/内存操作使用 Mock 硬件接口

**IPC 消息格式（Minix3 C 定义）**:
```c
// minix/include/minix/com.h
#define VM_FORK         (VM_RQ_BASE+1)
#  define VMF_ENDPOINT       m1_i1    /* 父进程 endpoint（输入） */
#  define VMF_SLOTNO         m1_i2    /* 子进程槽号（输入） */
#  define VMF_CHILD_ENDPOINT m1_i3    /* 子进程 endpoint（输出） */
```

**参考文档**: [../01-stage-pm/pm-call-vm-fork.md](../01-stage-pm/pm-call-vm-fork.md)

---

### 任务 2.6: 调用 VFS Fork（PM 侧）

**目标**: PM 通过真实 IPC 调用 VFS 的 fork 服务，实现 SUSPEND 机制

> **重要**:
> - PM 与 VFS 之间的 IPC 调用是**真实实现**
> - VFS 内部逻辑（文件描述符复制、引用计数）是**真实实现**
> - 仅 VFS 访问的硬件（磁盘 I/O）使用 Mock

**接口定义**:
```rust
// os/servers/pm/src/vfs_client.rs
pub struct VfsClient;

impl VfsClient {
    /// 调用 VFS fork 服务
    /// 
    /// # 注意
    /// 调用后 PM 进程进入 SUSPEND 状态，等待 VFS 回复
    pub fn fork(
        &self,
        child_ep: Endpoint,
        parent_ep: Endpoint,
        child_pid: Pid,
    ) -> Result<(), Error> {
        // 构造 VFS_PM_FORK 消息
        let req = VfsPmForkRequest {
            m_type: VFS_PM_FORK,
            vfs_pm_endpt: child_ep,
            vfs_pm_pendpt: parent_ep,
            vfs_pm_cpid: child_pid,
            vfs_pm_reuid: -1,  // 使用父进程 uid
            vfs_pm_regid: -1,  // 使用父进程 gid
        };
        
        // 异步通知 VFS（真实 IPC，不等待回复）
        ipc_notify(VFS_SERVICE, &req)?;
        
        // 设置进程状态为 SUSPEND
        // 等待 VFS 回复后才能继续
        Ok(())
    }
}
```

**IPC 消息格式（Minix3 C 定义）**:
```c
// minix/include/minix/com.h
#define VFS_PM_FORK      (VFS_PM_RQ_BASE + 7)
#  define VFS_PM_ENDPT   m7_i1    /* 子进程 endpoint */
#  define VFS_PM_PENDPT  m7_i2    /* 父进程 endpoint */
#  define VFS_PM_CPID    m7_i3    /* 子进程 PID */
#  define VFS_PM_REUID   m7_i4    /* 真实 uid (-1 for regular fork) */
#  define VFS_PM_REGID   m7_i5    /* 真实 gid (-1 for regular fork) */

#define VFS_PM_FORK_REPLY     (VFS_PM_RS_BASE + 7)
#  define VFS_PM_ENDPT   /* 子进程 endpoint（确认） */
```

**SUSPEND 机制**:
```rust
// os/servers/pm/src/suspend.rs

/// 设置进程为 SUSPEND 状态，等待指定服务的回复
pub fn suspend_for_reply(
    proc: &mut MProc,
    waiting_for: ServiceId,
) -> Result<(), Error> {
    proc.flags |= ProcessFlags::SUSPENDED;
    proc.rts_flags |= RTS_SENDING;  // 正在等待回复
    proc.waiting_for = Some(waiting_for);
    
    // 调度器将不再调度此进程
    // 直到收到 VFS 的回复消息
    scheduler::dequeue(proc.endpoint);
    
    Ok(())
}

/// 处理 VFS 回复，唤醒进程
pub fn handle_vfs_reply(
    proc: &mut MProc,
    status: i32,
) -> Result<(), Error> {
    // 清除 SUSPEND 状态
    proc.flags &= !ProcessFlags::SUSPENDED;
    proc.rts_flags &= !RTS_SENDING;
    proc.waiting_for = None;
    
    // 重新加入调度队列
    scheduler::enqueue(proc.endpoint);
    
    if status == 0 {
        Ok(())
    } else {
        Err(Error::from_raw(status))
    }
}
```

**关键点**:
- PM 构造 `VFS_PM_FORK` 消息并通过真实 IPC 发送给 VFS
- PM 进入 SUSPEND 状态，等待 VFS 完成文件描述符复制
- VFS 接收消息并执行真实的文件描述符复制逻辑
- VFS 内部的磁盘 I/O 操作使用 Mock 硬件接口
- VFS 完成后发送回复，PM 被唤醒

**参考文档**: [../01-stage-pm/pm-call-vfs-fork.md](../01-stage-pm/pm-call-vfs-fork.md)

---

## 附录：do_fork 完整流程对照表

基于 `minix3/minix/servers/pm/forkexit.c` 的 `do_fork()` 函数，逐步对照：

| 步骤 | C 代码 | Rust 实现 | 状态 |
|------|--------|----------|------|
| ① 检查进程表 | `procs_in_use == NR_PROCS \|\| ...` | `table.is_full() / can_alloc()` | ✅ |
| ② 查找空闲槽位 | `do { next_child++ } while (IN_USE)` | `table.find_free_slot()` | ✅ |
| ③ 调用 vm_fork | `vm_fork(endpoint, next_child, &child_ep)` | `vm_client.fork()` | 🔧 |
| ④ 增加计数 | `procs_in_use++` | `table.alloc_slot()` | ✅ |
| ⑤ 复制 mproc | `*rmc = *rmp` | `Process::fork_from()` | 🔧 |
| ⑥ 恢复 sigact | `rmc->mp_sigact = mpsigact[...]` | `signals.clone()` | ✅ |
| ⑦ 设置父进程 | `rmc->mp_parent = who_p` | `guardianship.parent` | ✅ |
| ⑧ 清除追踪器 | `rmc->mp_tracer = NO_TRACER` | `trace = default()` | ✅ |
| ⑨ 特权进程 | `mp_scheduler = SCHED_PROC_NR` | — | ❌ |
| ⑩ 重置标志 | `mp_flags &= (IN_USE\|DELAY_CALL\|TAINTED)` | 🔧 部分实现 | 🔧 |
| ⑪ 分配 PID | `get_free_pid()` | `generate_child_pid()` | 🔧 简化版 |
| ⑫ 通知 VFS | `tell_vfs(VFS_PM_FORK)` | `vfs_client.fork()` | 🔧 |
| ⑬ 追踪 SIGSTOP | `sig_proc(SIGSTOP)` | — | ❌ |
| ⑭ 返回 SUSPEND | `return SUSPEND` | `Err(Error::SUSPEND)` | 🔧 |

**图例**: ✅ 已实现 | 🔧 部分实现 | ❌ 未开始

---

## 3. 数据结构

### 3.1 核心类型

| 类型 | 定义 | 说明 |
|------|------|------|
| `Pid` | `type Pid = i32;` | 进程 ID |
| `Endpoint` | `#[repr(transparent)] pub struct Endpoint(u32);` | 内核端点 |
| `SlotIndex` | `#[repr(transparent)] pub struct SlotIndex(u16);` | 槽位索引（0..NR_PROCS） |
| `Uid` | `type Uid = u32;` | 用户 ID |

### 3.2 标志位

```rust
pub struct ProcessFlags: u32 {
    const IN_USE = 0x00001;      // 槽位已使用
    const WAITING = 0x00002;     // 父进程在等待
    const ZOMBIE = 0x00004;      // 僵尸状态
    const PRIV_PROC = 0x02000;   // 特权进程
    const DELAY_CALL = 0x04000;  // 延迟调用
    const TAINTED = 0x08000;     // 被污染
}
```

### 3.3 错误类型

```rust
pub enum Error {
    EAGAIN,     // 资源暂时不可用
    ENOMEM,     // 内存不足
    EPERM,      // 权限不足
    EINVAL,     // 无效参数
    ESRCH,      // 无此进程
}
```

## 4. 文件组织

```
os/servers/pm/src/
├── main.rs           # 入口
├── lib.rs            # 库导出
├── fork.rs           # fork 系统调用入口
├── exec.rs           # exec 系统调用
├── exit.rs           # exit 系统调用
├── signal.rs         # 信号处理
├── wait.rs           # wait 系统调用
└── mproc/            # PM 进程表子模块
    ├── mod.rs        # 模块导出
    ├── mproc.rs      # Process 结构体定义
    ├── lifecycle.rs  # 生命周期状态机
    ├── block.rs      # 阻塞状态
    ├── wait.rs       # 等待状态
    ├── guardianship.rs # 监护关系
    ├── trace.rs      # 追踪状态
    ├── signal.rs     # 信号状态
    ├── credentials.rs # 权限凭证
    ├── table.rs      # 进程表管理 (ProcTable)
    ├── context.rs    # PmContext 定义
    ├── fork.rs       # fork 实现 (Process::fork_from, do_fork_prepare)
    └── pid_gen.rs    # PID 生成器
```

## 5. 验收标准

### 5.1 功能验收

- [ ] MProc 结构体可正确定义并实例化
- [ ] PM 进程表可安全并发访问（单线程环境）
- [ ] 进程表满时返回 `EAGAIN`
- [ ] 能正确找到并分配空闲槽位
- [ ] PID 唯一性保证
- [ ] PID 循环复用（达到最大值后回到 INIT_PID+1）
- [ ] 父进程状态正确复制到子进程
- [ ] 子进程特有字段正确初始化
- [ ] 父子关系正确建立

### 5.2 测试验收

- [ ] 单元测试：进程槽位分配与释放
- [ ] 单元测试：边界条件（满表、只剩一个槽位等）
- [ ] 单元测试：并发 PID 分配
- [ ] 集成测试：完整 do_fork 流程（真实 IPC，Mock 硬件）

### 5.3 代码质量验收

- [ ] 所有函数有文档注释
- [ ] 复杂逻辑有行内注释
- [ ] 通过 Clippy 检查
- [ ] 单元测试覆盖率 > 80%

## 6. 与 Minix3 对照

| Minix3 (C) | Rust 重构 | 说明 |
|-----------|----------|------|
| `struct mproc` | `MProc` | 字段一一对应，类型安全化 |
| `mproc[NR_PROCS]` | `ProcTable` | 封装访问逻辑，线程安全 |
| `get_free_pid()` | `PidGenerator::alloc()` | 算法保持一致 |
| `do_fork()` | `do_fork()` | 流程保持一致，错误处理改进 |
| 全局变量 `mp`, `who_p` | `PmContext` | 显式上下文，避免隐式状态 |

## 7. 参考文档

- **C 源码分析**: [../01-stage-pm/mproc-design.md](../01-stage-pm/mproc-design.md)
- **PID 生成器**: [../01-stage-pm/pid-generator.md](../01-stage-pm/pid-generator.md)
- **do_fork 实现**: [../01-stage-pm/do-fork-impl.md](../01-stage-pm/do-fork-impl.md)
- **架构分析**: [02-architecture-analysis.md](./02-architecture-analysis.md)
