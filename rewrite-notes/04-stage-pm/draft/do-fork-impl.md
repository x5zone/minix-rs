# do_fork 实现

## C 源码位置

| 文件 | 路径 | 说明 |
|------|------|------|
| [`forkexit.c`](../../../../minix3/minix/servers/pm/forkexit.c) | `minix3/minix/servers/pm/forkexit.c` | PM fork 主逻辑，`do_fork()` 函数（约 150 行） |
| [`table.c`](../../../../minix3/minix/servers/pm/table.c) | `minix3/minix/servers/pm/table.c` | 进程表管理，`get_free_pid()` |
| [`misc.c`](../../../../minix3/minix/servers/pm/misc.c) | `minix3/minix/servers/pm/misc.c` | `tell_vfs()` 通知 VFS |
| [`pm.h`](../../../../minix3/minix/servers/pm/pm.h) | `minix3/minix/servers/pm/pm.h` | `struct mproc` 定义 |
| [`param.h`](../../../../minix3/include/minix/param.h) | `minix3/include/minix/param.h` | `NR_PROCS`, `LAST_FEW` 常量 |

## 0. 设计方案分析

> 本章节分析 `do_fork` 前半部分（参数检查与槽位分配）的设计方案。

### 0.1 进程表存储方案

#### 方案对比

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
- ✅ **零运行时分配**：编译期确定大小
- ✅ **缓存友好**：连续内存布局
- ✅ **索引 O(1)**：直接数组访问
- ✅ **no_std 完美兼容**：不需要分配器
- ✅ **与 Minix3 一致**：原设计就是固定大小

**内存占用估算**：
```
Process 大小 ≈ 88 bytes（方案二/三）或 72 bytes（方案五）
256 个进程 ≈ 22.5 KB 或 18 KB
```

#### 其他方案简述

**B. MaybeUninit 数组**：
- 优点：延迟初始化，避免默认值开销
- 缺点：需要 `unsafe` 代码，额外的 `initialized` 数组，复杂度增加收益有限
- **结论**：不推荐

**C. Vec**：
- 优点：可以动态调整大小
- 缺点：需要全局分配器，no_std 环境需要额外配置
- **结论**：不推荐，Minix3 进程表大小固定

**D. Slab 分配器**：
- 优点：自动管理空闲槽位，O(1) 插入/删除
- 缺点：依赖外部 crate，内存不连续（缓存不友好）
- **结论**：不推荐，对于固定大小的进程表过于复杂

### 0.2 计数器方案

#### 方案对比

| 方案 | 类型 | 线程安全 | 性能 | 内部可变 | 适用场景 |
|------|------|---------|------|---------|---------|
| **A. 普通 usize** | `usize` | ❌ 否 | ⚡ 最快 | ❌ 否 | 单线程 + `&mut self` |
| **B. Cell<usize>** | `Cell<usize>` | ❌ 否 | ⚡ 最快 | ✅ 是 | 单线程 + `&self` |
| **C. RefCell<usize>** | `RefCell<usize>` | ❌ 否 | ⚡ 快 | ✅ 是 | 单线程 + 运行时检查 |
| **D. AtomicUsize** | `AtomicUsize` | ✅ 是 | 🚀 中等 | ✅ 是 | 多线程 |

#### 推荐方案：Cell<usize>

```rust
use core::cell::Cell;

pub struct ProcTable {
    procs: [Process; NR_PROCS],
    procs_in_use: Cell<usize>,  // Cell 允许内部可变
    next_child: Cell<usize>,
}

impl ProcTable {
    pub fn find_free_slot(&self) -> Option<usize> {
        let start = self.next_child.get();
        for i in 0..NR_PROCS {
            let idx = (start + i) % NR_PROCS;
            if !self.procs[idx].is_in_use() {
                self.next_child.set((idx + 1) % NR_PROCS);
                self.procs_in_use.set(self.procs_in_use.get() + 1);
                return Some(idx);
            }
        }
        None
    }
}
```

**理由**：
- PM 是单线程服务器
- 需要内部可变性（`&self` 方法修改计数器）
- 零运行时开销
- 实现简单，不需要 unsafe

#### 其他方案简述

**A. 普通 usize**：
- 缺点：所有方法都需要 `&mut self`，如果有多个引用无法修改
- 适用：确定只有一个可变引用时

**C. RefCell<usize>**：
- 优点：支持非 Copy 类型，运行时借用检查
- 缺点：运行时开销（borrow/borrow_mut），可能 panic
- 结论：对于 usize 来说过于复杂

**D. AtomicUsize**：
- 优点：线程安全，无锁
- 缺点：需要考虑内存顺序，进程数组需要 `UnsafeCell`，复杂度增加
- 适用：多线程访问进程表

### 0.3 槽位查找算法

#### 方案对比

| 方案 | 时间复杂度 | 空间开销 | 实现复杂度 | 缓存友好 |
|------|-----------|---------|-----------|---------|
| **A. 轮询（原C）** | O(N) 最坏 | 0 | ⭐ 简单 | ✅ 是 |
| **B. 空闲链表** | O(1) 平均 | O(N) 指针 | ⭐⭐ 中等 | ❌ 否 |
| **C. 位图** | O(N) 最坏 | N/8 bytes | ⭐⭐ 中等 | ✅ 是 |
| **D. 空闲栈** | O(1) 平均 | O(N) 索引 | ⭐⭐ 中等 | ⚠️ 部分 |

#### 推荐方案：轮询（第一阶段）→ 空闲栈（第二阶段）

**第一阶段**：轮询算法（直接翻译 C 代码）

```rust
impl ProcTable {
    pub fn find_free_slot(&self) -> Option<usize> {
        let start = self.next_child.get();
        
        for i in 0..NR_PROCS {
            let idx = (start + i) % NR_PROCS;
            if !self.procs[idx].is_in_use() {
                self.next_child.set((idx + 1) % NR_PROCS);
                return Some(idx);
            }
        }
        
        None
    }
}
```

**第二阶段**：空闲栈（O(1) 优化）

```rust
pub struct ProcTable {
    // ...
    /// 空闲槽位栈
    free_stack: [usize; NR_PROCS],
    /// 栈顶指针
    free_top: Cell<usize>,
}

impl ProcTable {
    pub fn alloc_slot(&self) -> Option<usize> {
        if self.free_top.get() == 0 {
            return None;
        }
        self.free_top.set(self.free_top.get() - 1);
        let slot = self.free_stack[self.free_top.get()];
        self.procs_in_use.set(self.procs_in_use.get() + 1);
        Some(slot)
    }
}
```

#### 其他方案简述

**B. 空闲链表**：
- 优点：O(1) 分配和释放
- 缺点：额外 `NR_PROCS * 8` bytes（约 2KB），缓存不友好（链表跳转）
- 结论：不推荐，链表跳转对缓存不友好

**C. 位图**：
- 优点：空间效率高（只需要 32 bytes），缓存友好
- 缺点：O(N/64) 最坏情况，需要同步位图和进程状态
- 结论：不推荐，收益不大，增加复杂度

### 0.4 当前进程表达方案

#### 方案对比

| 方案 | 描述 | 优点 | 缺点 |
|------|------|------|------|
| **A. 参数传递** | 每个函数接收 `current: usize` | 显式、可测试 | 参数多 |
| **B. 上下文结构** | `PmContext { table, current }` | 封装良好 | 需要生命周期 |
| **C. 线程局部** | `thread_local! { static CURRENT }` | 类似 C | no_std 支持有限 |
| **D. 全局静态** | `static CURRENT: AtomicUsize` | 简单 | 不安全、难测试 |

#### 推荐方案：上下文结构

```rust
/// PM 上下文
pub struct PmContext<'a> {
    pub table: &'a mut ProcTable,
    pub current: usize,
}

impl<'a> PmContext<'a> {
    pub fn new(table: &'a mut ProcTable, current: usize) -> Self {
        Self { table, current }
    }
    
    pub fn current_proc(&self) -> &Process {
        &self.table.procs[self.current]
    }
    
    pub fn can_alloc(&self) -> bool {
        let proc = self.current_proc();
        let is_root = proc.resources.privilege.is_kernel()
            || proc.resources.privilege.credentials()
                .map(|c| c.is_superuser())
                .unwrap_or(false);
        
        let in_use = self.table.procs_in_use.get();
        if in_use >= NR_PROCS {
            return false;
        }
        if in_use >= NR_PROCS - LAST_FEW && !is_root {
            return false;
        }
        true
    }
}
```

**理由**：
- 显式且类型安全
- 易于测试
- 符合 Rust 惯例
- no_std 完美兼容

#### 其他方案简述

**A. 参数传递**：
- 优点：最显式，所有依赖都在参数中，最容易测试，无全局状态
- 缺点：参数爆炸，每个函数都需要传递，代码冗余
- 适用：简单场景

**C. 线程局部存储**：
- 优点：类似 C 的全局变量风格，不需要传递参数
- 缺点：no_std 支持有限，难以测试（全局状态难以隔离），运行时开销（TLS 访问）
- 结论：不推荐

**D. 全局静态**：
- 优点：最简单
- 缺点：不安全（大量 `unsafe` 代码），难以测试，线程不安全
- 结论：不推荐

### 0.5 方案总结表

| 设计点 | 选择方案 | 理由 |
|--------|---------|------|
| 进程表存储 | 静态数组 `[Process; NR_PROCS]` | 固定大小、零分配、缓存友好 |
| 计数器 | `Cell<usize>` | 单线程、内部可变、零开销 |
| 槽位查找 | 轮询 → 空闲栈 | 先简单实现，后续优化 |
| 当前进程 | `PmContext<'a>` | 显式、可测试、类型安全 |

### 0.6 后续优化方向

1. **槽位查找优化**：从轮询改为空闲栈（O(1) 性能）
2. **PID 分配**：实现独立的 PID 分配器
3. **并发支持**：如果需要，改用 `AtomicUsize`

### 0.7 Rewrite vs Redesign

> **这是整个设计决策的分水岭**

#### 两个阶段的本质区别

| 维度 | Rewrite 阶段 | Redesign 阶段 |
|------|-------------|--------------|
| **目标** | 忠实翻译 Minix3 | 进化架构 |
| **约束** | 行为 100% 对齐 | 可以改变模型 |
| **数据结构** | 保持 Minix 语义 | 可以引入 Rust 语义 |
| **状态模型** | 共享可变状态 | 可以改变所有权模型 |

#### Rewrite 阶段推荐方案

```text
进程表：[Process; NR_PROCS]
计数器：Cell<usize>
槽位：轮询
current：PmContext
IN_USE：保留
```

> **Rewrite = 结构翻译 + 类型安全，不改变模型**

#### Redesign 阶段可选方案

```text
进程表：[Option<Process>; NR_PROCS] 或 slab-like
槽位：空闲栈或 bitmap
并发：UnsafeCell + Atomic 或 handle-based
```

> **Redesign = 改变资源模型 + ownership 语义**

### 0.8 IN_USE 要不要删？

> **这是整个 rewrite 的分水岭**

| 阶段 | 是否删除 IN_USE | 替代方案 |
|------|----------------|---------|
| **Rewrite** | ❌ 绝对不要删 | 保持 `Lifecycle::Unused` |
| **Redesign** | ✅ 可以删除 | `Option<Process>` |

#### 为什么 Rewrite 不能删？

**Minix 的语义是**：
```text
mproc[i] 始终存在
只是：
- 是否在用
- 状态是什么
```

也就是说：
```text
slot ≠ process
```

而 `Option<Process>` 变成：
```text
slot == process
```

**这已经是语义改变**。

#### 更深层原因

**Minix 的进程结构会被多方"引用"**：
- PM（Process Manager）
- VFS
- Kernel scheduler
- Signal subsystem
- Tracer（ptrace）

这是：
```text
共享内存 + 协议保证一致性
```

不是 Rust 意义的 ownership。

#### IN_USE 的本质

```text
IN_USE 的本质是：
"这个 slot 当前参与系统协议"

而不是：
"这个对象存在不存在"
```

### 0.9 所有权与多所有者问题

> **Rust 要唯一 `&mut`，Minix 有多个"逻辑 `&mut`"**

#### 三条路线

**路线 A：内核风格（推荐）**
```rust
struct ProcTable {
    procs: [UnsafeCell<Process>; N]
}
```
- 外层保证：单线程或锁
- 优点：接近真实 OS，性能最好
- 本质：**承认 Rust borrow checker 不适合内核共享模型**

**路线 B：Rust 纯净模型**
```rust
Option<Process>
Rc<RefCell<Process>>
```
- 问题：性能差，no_std 不友好，不像 OS

**路线 C：ECS / handle-based（高级）**
```rust
ProcId → table → Process
```
- 优点：无 borrow 冲突，可扩展
- 这是 seL4 / Rust OS 更现代的方向

### 0.10 实现建议

#### 不要使用 `#[derive(Clone)]`

建议手写 `fork_from(parent: &Process)` 构造函数：

```rust
impl Process {
    /// 从父进程创建子进程
    /// 
    /// 强迫检查每一个字段：哪些该留，哪些该清
    pub fn fork_from(parent: &Process, child_pid: Pid, child_endpoint: Endpoint) -> Self {
        Self {
            identity: ProcessIdentity {
                id: ProcessId { pid: child_pid, endpoint: child_endpoint },
                procgrp: parent.identity.procgrp,
                name: parent.identity.name,
            },
            state: ProcessState {
                lifecycle: Lifecycle::Running,
                block: BlockState::default(),  // 子进程不继承阻塞状态
                wait: WaitState::default(),
                guardianship: Guardianship::Normal { 
                    parent: parent.index() 
                },
                trace: TraceState::default(),
            },
            resources: parent.resources.clone(),
            ipc: ProcessIpc::default(),  // 子进程不继承 IPC 状态
        }
    }
}
```

#### Endpoint 的计算

Minix3 的进程索引和端点是两回事：

```rust
pub struct ProcTable {
    procs: [Process; NR_PROCS],
    generation: [Cell<u32>; NR_PROCS],  // 代数
}

impl ProcTable {
    pub fn calculate_endpoint(&self, idx: usize) -> Endpoint {
        // Endpoint = (generation << 16) | idx
        Endpoint::new(self.generation[idx].get(), idx)
    }
    
    pub fn release_slot(&self, idx: usize) {
        // 释放时代数 +1，防止过时消息
        self.generation[idx].set(self.generation[idx].get() + 1);
    }
}
```

#### 显式化子进程的"第一口气"

```rust
impl<'a> PmContext<'a> {
    pub fn do_fork(&mut self) -> Result<Pid, ForkError> {
        // ... 前面的检查和分配 ...
        
        // 显式设置子进程的返回值
        let mut reply = Message::default();
        reply.result = 0;  // 子进程 fork 返回 0
        self.table.procs[child_idx].set_reply(reply);
        
        Ok(child_pid)
    }
}

### 0.11 编译器并发检查：Rust 的杀手锏

> **利用借用检查器作为形式化验证工具，来保证内核的正确性**

#### Rust 的核心规则

Rust 的核心规则只有两条，但它们能杜绝 90% 的 OS 内核 Bug：

1. **任意时刻，你可以拥有任意数量的 `&T`（只读引用）**
2. **任意时刻，你最多只能拥有一个 `&mut T`（可变引用），且不能同时存在 `&T`**

#### 在进程表中的应用

**场景 A：读取进程状态（安全）**

```rust
// 你有 10 个不同的函数同时拿着 &Process 去读 PID 或状态
let p1: &Process = table.get(pid1);
let p2: &Process = table.get(pid2);
// 编译器：✅ 放行。这是并行读，没有数据竞争。
```

**场景 B：修改进程状态（安全）**

```rust
// 只有 1 个函数拿着 &mut Process 去修改
let p: &mut Process = table.get_mut(pid);
p.lifecycle = Lifecycle::Running;
// 编译器：✅ 放行。这是独占写，没有脏读。
```

**场景 C：读写冲突（危险）**

```rust
let p: &Process = table.get(pid);  // 拿到只读引用
// 此时，你持有 &Process
// 调度器想要调用 table.remove(pid) 来释放内存
// remove 需要 &mut self (可变借用)
// 编译器：❌ 直接报错！
// "cannot borrow proc as mutable because it is also borrowed as immutable"
```

#### C 语言的致命问题 vs Rust 的编译期保护

**C 语言伪代码**：
```c
struct mproc *p = find_proc(pid); // 拿到指针
if (p->status == READY) {          // 正在读 status
    // 此时发生时钟中断，调度器运行
    // 调度器调用了 free_proc(p)，把 p 指向的内存释放了！
} 
// 回到这里，if 语句还在执行，但 p 已经是悬垂指针！(Use-After-Free)
```

**Rust 代码**：
```rust
let p = proc_table.get(pid); // 拿到 &Process
if p.status == Lifecycle::Ready { 
    // 此时，你持有 &Process
    // 调度器想要调用 proc_table.remove(pid)
    // remove 需要 &mut self (可变借用)
    // 编译器会报错：你不能在 p 还活着时，去借用 proc_table 的可变引用！
}
```

#### PmContext 作为"编译时的锁"

```rust
pub struct PmContext<'a> {
    pub table: &'a mut ProcTable,  // 锁定了对 ProcTable 的访问权限
    pub current: usize,
}
```

当你创建 `PmContext` 时，你锁定了对 `ProcTable` 的访问权限：
- 如果你创建的是 `&mut ProcTable`，Rust 会保证在整个 `PmContext` 生命周期内，没有其他代码能偷偷摸摸地去读或写进程表
- **这就是编译时的锁（Compile-time Mutex）**

#### 最终结论

你的策略是完美的：

```text
裸数组 [Process; NR_PROCS]：保证内存连续、零开销、布局固定
&Process 和 &mut Process：利用编译器的线性类型系统来强制执行互斥访问
```

**这正是 Rust 在操作系统开发中的核心价值**：

> 它用静态分析（编译时检查）替代了传统操作系统中复杂的动态锁（运行时 Mutex）。在单核（或 BKL）场景下，这能帮你消灭掉绝大多数的竞态条件（Race Conditions）。

### 0.12 Process::default() 的正确性

```rust
impl Default for Process {
    fn default() -> Self {
        Self {
            lifecycle: Lifecycle::Unused,  // 关键！
            // ... 其他字段 ...
        }
    }
}

// 这样 is_in_use() 检查非常廉价且安全
impl Process {
    pub fn is_in_use(&self) -> bool {
        !matches!(self.lifecycle, Lifecycle::Unused)
    }
}
```

---

## 1. C 源码分析

### 1.0 完整状态机概览

```
do_fork()
  │
  ├── ① 检查进程表是否已满
  │     if (procs_in_use == NR_PROCS) → EAGAIN
  │     if (procs_in_use >= NR_PROCS-LAST_FEW && uid != 0) → EAGAIN
  │
  ├── ② 查找空闲槽位 (next_child 轮询)
  │     do { next_child = (next_child+1) % NR_PROCS; n++; }
  │     while (mproc[next_child].mp_flags & IN_USE && n <= NR_PROCS)
  │
  ├── ③ vm_fork(parent_ep, child_slot, &child_ep)  ← 可以失败！
  │     │
  │     ▼ IPC: VM_FORK (同步 sendrec)
  │     VM: do_fork() → ... → 返回 child_ep
  │     │
  │     失败 → return 错误码（无需回滚，因为还没分配槽位）
  │
  ├── ④ 获取子进程槽位指针，增加计数
  │     rmc = &mproc[next_child]
  │     procs_in_use++
  │
  ├── ⑤ 整体拷贝父进程 mproc → 子进程
  │     *rmc = *rmp
  │
  ├── ⑥ 恢复 mp_sigact 指针（因为 *rmc = *rmp 覆盖了指针）
  │     rmc->mp_sigact = mpsigact[next_child]
  │     memcpy(rmc->mp_sigact, rmp->mp_sigact, sizeof(mpsigact[next_child]))
  │
  ├── ⑦ 设置父子关系
  │     rmc->mp_parent = who_p
  │
  ├── ⑧ 清除追踪器
  │     if (!(rmc->mp_trace_flags & TO_TRACEFORK)):
  │         rmc->mp_tracer = NO_TRACER
  │         rmc->mp_trace_flags = 0
  │         sigemptyset(&rmc->mp_sigtrace)
  │
  ├── ⑨ 特权进程处理
  │     if (rmc->mp_flags & PRIV_PROC):
  │         assert(rmc->mp_scheduler == NONE)
  │         rmc->mp_scheduler = SCHED_PROC_NR
  │
  ├── ⑩ 继承/重置标志位和统计信息
  │     rmc->mp_flags &= (IN_USE|DELAY_CALL|TAINTED)
  │     rmc->mp_child_utime = 0
  │     rmc->mp_child_stime = 0
  │     rmc->mp_exitstatus = 0
  │     rmc->mp_sigstatus = 0
  │     rmc->mp_endpoint = child_ep
  │     for (i = 0; i < NR_ITIMERS; i++)
  │         rmc->mp_interval[i] = 0
  │     rmc->mp_started = getticks()
  │
  ├── ⑪ 分配 PID
  │     new_pid = get_free_pid()
  │     rmc->mp_pid = new_pid
  │
  ├── ⑫ 通知 VFS (异步)
  │     m.m_type = VFS_PM_FORK
  │     m.VFS_PM_ENDPT = rmc->mp_endpoint
  │     m.VFS_PM_PENDPT = rmp->mp_endpoint
  │     m.VFS_PM_CPID = rmc->mp_pid
  │     m.VFS_PM_REUID = -1
  │     m.VFS_PM_REGID = -1
  │     tell_vfs(rmc, &m)  ← asynsend, 不等待回复
  │
  ├── ⑬ 追踪器处理
  │     if (rmc->mp_tracer != NO_TRACER)
  │         sig_proc(rmc, SIGSTOP, TRUE, FALSE)
  │
  └── ⑭ 返回 SUSPEND
```

### 1.1 源码位置

**文件**: `minix3/minix/servers/pm/forkexit.c`
- **位置**: 第 47-145 行

**前半部分代码**（参数检查与槽位分配）:
```c
int do_fork(void) {
  register struct mproc *rmp;   // 父进程指针
  register struct mproc *rmc;   // 子进程指针
  static unsigned int next_child = 0;  // 下一个子进程槽位
  int n = 0;

  rmp = mp;  // 当前进程（宏定义）
  
  // 1. 检查进程表是否已满
  if ((procs_in_use == NR_PROCS) ||
      (procs_in_use >= NR_PROCS-LAST_FEW && rmp->mp_effuid != 0)) {
    printf("PM: warning, process table is full!\n");
    return(EAGAIN);
  }

  // 2. 查找空闲槽位（轮询算法）
  do {
    next_child = (next_child+1) % NR_PROCS;
    n++;
  } while((mproc[next_child].mp_flags & IN_USE) && n <= NR_PROCS);
  
  if(n > NR_PROCS)
    panic("do_fork can't find child slot");
  
  // ... vm_fork 调用（下一阶段处理）
}
```

**后半部分代码**（进程结构复制与初始化）:
```c
  // 4. 获取子进程槽位指针
  rmc = &mproc[next_child];
  procs_in_use++;
  
  // 5. 复制父进程 mproc 到子进程
  *rmc = *rmp;
  
  // 6. 恢复子进程特有的字段
  rmc->mp_sigact = mpsigact[next_child];
  memcpy(rmc->mp_sigact, rmp->mp_sigact, sizeof(mpsigact[next_child]));
  
  // 7. 设置父子关系
  rmc->mp_parent = who_p;  // 父进程槽位索引
  
  // 8. 清除跟踪器（非 fork 跟踪场景）
  if (!(rmc->mp_trace_flags & TO_TRACEFORK)) {
    rmc->mp_tracer = NO_TRACER;
    rmc->mp_trace_flags = 0;
    sigemptyset(&rmc->mp_sigtrace);
  }

  // 9. 特权进程处理
  if (rmc->mp_flags & PRIV_PROC) {
    assert(rmc->mp_scheduler == NONE);
    rmc->mp_scheduler = SCHED_PROC_NR;
  }

  // 10. 继承/重置标志位和统计信息
  rmc->mp_flags &= (IN_USE|DELAY_CALL|TAINTED);
  rmc->mp_child_utime = 0;
  rmc->mp_child_stime = 0;
  rmc->mp_exitstatus = 0;
  rmc->mp_sigstatus = 0;
  rmc->mp_endpoint = child_ep;  // 从 VM 返回的端点
  for (i = 0; i < NR_ITIMERS; i++)
    rmc->mp_interval[i] = 0;
  rmc->mp_started = getticks();

  // 11. 分配 PID
  new_pid = get_free_pid();
  rmc->mp_pid = new_pid;
```

## 2. 函数签名

## 3. 前置检查

## 3. 槽位分配

## 4. 结构复制

### 4.1 实现方案对比

Minix3 的 C 实现采用 `*rmc = *rmp` 整体拷贝加字段修正的方式，Rust 实现有三种可选方案：

#### 方案一：整体拷贝 + 字段修正（C 风格）

```rust
impl Process {
    /// C 风格：整体拷贝父进程，再修正需要重置的字段
    /// ⚠️ 风险：新增字段时容易漏掉修正，导致 silent bug
    pub fn fork_c_style(parent: &Self, child_index: usize, child_ep: Endpoint, new_pid: Pid) -> Self {
        let mut child = parent.clone();

        // 修正字段
        child.identity.id.index = child_index;
        child.identity.id.pid = new_pid;
        child.identity.endpoint = child_ep;
        child.state.guardianship.parent = parent.identity.id.index;
        child.state.guardianship.tracer = None;
        child.state.trace = Default::default();
        child.state.lifecycle = Lifecycle::Running;
        child.resources.child_utime = 0;
        child.resources.child_stime = 0;
        child.resources.intervals = [0; 3];
        child.resources.started = getticks();
        child.ipc.reply = None;
        child.ipc.event_subscriber = None;

        // 特权进程处理
        if parent.resources.privilege.is_privileged {
            child.resources.scheduler = SchedId::SCHED_PROC_NR;
        }

        child
    }
}
```

| 优点 | 缺点 |
|------|------|
| ✅ 代码量少 | ❌ 语义不可见，无法直观了解 fork 行为 |
| ✅ 和 C 实现一一对应 | ❌ 新增字段时容易漏修正，导致 silent bug |
| | ❌ 无法利用类型系统强制正确性 |

#### 方案二：显式字段构造 + 语义注释（推荐）

显式构造每个字段，强迫开发者检查每个字段的 fork 行为。配合语义注释，让代码接近"内核文档本身"。

```rust
impl Process {
    /// 策略：显式构造（Explicit Construction）
    /// 哲学：每个字段的 fork 行为都是明确且可见的
    pub fn fork_explicit(parent: &Self, child_index: usize, child_ep: Endpoint, new_pid: Pid) -> Self {
        Self {
            // ========== Identity 部分 ==========
            identity: Identity {
                id: ProcessId {
                    index: child_index,       // 自己的槽位索引，不是父进程的
                    pid: new_pid,             // 新分配的 PID
                },
                endpoint: child_ep,           // 从 VM 返回的新端点
                procgrp: parent.identity.procgrp, // 加入父进程组
                name: parent.identity.name.clone(), // 进程名相同
            },

            // ========== Guardianship 部分 ==========
            state: ProcessState {
                guardianship: Guardianship {
                    parent: parent.identity.id.index, // 父进程索引
                    tracer: None,               // 默认清除追踪器
                },
                trace: Default::default(),      // 清除追踪标志
                lifecycle: Lifecycle::Running,  // 标记为运行中
            },

            // ========== Resources 部分 ==========
            resources: Resources {
                privilege: parent.resources.privilege.clone(), // 继承权限
                nice: parent.resources.nice,                   // 继承优先级
                child_utime: 0,                                // 子进程无子进程
                child_stime: 0,                                // 同上
                intervals: [0; 3],                             // 定时器清零
                started: getticks(),                           // 当前时间启动
                signals: parent.resources.signals.clone(),     // 继承信号处理
                scheduler: if parent.resources.privilege.is_privileged {
                    SchedId::SCHED_PROC_NR                     // 特权进程设置调度器
                } else {
                    parent.resources.scheduler
                },
            },

            // ========== IPC 部分 ==========
            ipc: IpcState {
                reply: None,                          // 子进程无待回复消息
                event_subscriber: None,               // 无事件订阅
            },
        }
    }
}
```

| 优点 | 缺点 |
|------|------|
| ✅ 语义完全可见，每个字段行为明确 | ❌ 代码量稍大 |
| ✅ 新增字段时必须显式处理，不会漏 | ❌ 字段增加时需要维护 |
| ✅ 代码即文档，无需额外注释理解行为 | |
| ✅ 可以配合类型系统强制正确性 | |

#### 方案三：Builder 模式（可选）

使用 Builder 模式，提供更灵活的构造方式，适合有多种 fork 变体的场景。

```rust
pub struct ProcessForkBuilder<'a> {
    parent: &'a Process,
    child_index: usize,
    child_ep: Endpoint,
    new_pid: Pid,
    tracefork: bool,  // 是否为 TRACEFORK 场景
    keep_tracer: bool, // 是否保留追踪器
}

impl<'a> ProcessForkBuilder<'a> {
    pub fn new(parent: &'a Process, child_index: usize, child_ep: Endpoint, new_pid: Pid) -> Self {
        Self {
            parent,
            child_index,
            child_ep,
            new_pid,
            tracefork: false,
            keep_tracer: false,
        }
    }

    pub fn tracefork(mut self) -> Self {
        self.tracefork = true;
        self
    }

    pub fn build(self) -> Process {
        Process {
            identity: Identity {
                id: ProcessId {
                    index: self.child_index,
                    pid: self.new_pid,
                },
                endpoint: self.child_ep,
                procgrp: self.parent.identity.procgrp,
                name: self.parent.identity.name.clone(),
            },
            state: ProcessState {
                guardianship: Guardianship {
                    parent: self.parent.identity.id.index,
                    tracer: if self.keep_tracer {
                        self.parent.state.guardianship.tracer
                    } else {
                        None
                    },
                },
                trace: if self.tracefork {
                    self.parent.state.trace
                } else {
                    Default::default()
                },
                lifecycle: Lifecycle::Running,
            },
            // ... 其他字段类似
        }
    }
}
```

**使用方式**：
```rust
let child = ProcessForkBuilder::new(&parent, child_idx, child_ep, new_pid)
    .tracefork()
    .build();
```

| 优点 | 缺点 |
|------|------|
| ✅ 灵活支持多种 fork 场景 | ❌ 需要额外的 Builder 结构体 |
| ✅ 可配置性高 | ❌ 代码复杂度增加 |
| ✅ 适合有多个 fork 变体的系统 | ❌ 过度设计，对 Minix3 简单场景没必要 |

#### 为什么不推荐 Builder 模式用于内核 fork？

> **fork 是一个原子操作，它的参数和行为是内核协议规定的**

- **路径可预测性**：内核代码追求的是执行路径的确定性，Builder 的可选配置会破坏这种确定性
- **语义约束**：fork 不是"可选参数组合"的行为，而是强约束的系统语义
- **职责分散风险**：Builder 模式会让语义变松，容易导致 invariant 被破坏

**结论**：Builder 模式适合用户态 API 设计，不适合内核语义建模。

### 4.2 方案推荐与最佳实践

#### 当前阶段推荐：方案二（显式字段构造 + 语义注释）

理由：
1. **语义明确**：每个字段的行为一目了然，代码即文档
2. **正确性保障**：新增字段必须显式处理，不会漏掉
3. **简单可靠**：没有额外抽象，适合 Minix3 这种相对简单的微内核场景
4. **方便调试**：出现问题时可以直接定位到每个字段的处理逻辑

#### 风险警告
❌ 不要使用方案一（整体拷贝），除非你能保证每次新增字段时都记得在 fork 逻辑中修正。这种方式很容易引入 silent bug，比如新增字段默认应该清零，但 clone 会复制父进程的值，而你完全不知道。

#### 未来演进方向
如果后续需要支持多种 fork 变体（比如 vfork、clone 等），可以考虑演进为：
- 基础版用方案二（显式构造）
- 高层 API 用 Builder 模式封装变体

### 4.4 进阶优化：语义函数构造（方案四）

> 💡 **核心理念**：不要直接构造 `Process`，而是把"fork 语义"编码进类型系统

#### 核心思想

将 `fork_from` 拆分为多个语义函数，每个函数只负责构造一个子组件并表达其 fork 语义：

```rust
/// Fork 上下文：携带所有构造所需信息
pub struct ForkContext<'a> {
    parent: &'a Process,
    child_index: usize,
    child_pid: Pid,
    child_endpoint: Endpoint,
}

impl Process {
    /// 语义驱动的 fork 构造
    pub fn fork_from(ctx: ForkContext) -> Self {
        Self {
            identity: Self::fork_identity(&ctx),
            state: Self::fork_state(&ctx),
            resources: Self::fork_resources(&ctx),
            ipc: ProcessIpc::default(),
        }
    }

    /// Identity：纯继承 + 覆盖
    fn fork_identity(ctx: &ForkContext) -> ProcessIdentity {
        ProcessIdentity {
            id: ProcessId {
                index: ProcIndex::new(ctx.child_index),
                pid: ctx.child_pid,
            },
            endpoint: ctx.child_endpoint,
            procgrp: ctx.parent.identity.procgrp,
            name: ctx.parent.identity.name.clone(),
        }
    }

    /// Resources：核心语义区（继承 vs 重置）
    fn fork_resources(ctx: &ForkContext) -> ProcessResources {
        let parent = ctx.parent;
        ProcessResources {
            privilege: parent.resources.privilege.clone(),
            signals: parent.resources.signals.clone(),
            child_utime: 0,
            child_stime: 0,
            started: getticks(),
            intervals: [0; NR_ITIMERS],
            timer: None,
            nice: parent.resources.nice,
            scheduler: Self::fork_scheduler(parent),
            flags: Self::fork_flags(parent),
        }
    }

    /// State：重置为初始状态
    fn fork_state(ctx: &ForkContext) -> ProcessState {
        ProcessState {
            lifecycle: Lifecycle::Running,
            guardianship: Guardianship::Normal {
                parent: ctx.parent.identity.id.index,
            },
            ..ProcessState::default()
        }
    }

    /// Flags：只保留 TAINTED
    fn fork_flags(parent: &Process) -> RemainingFlags {
        if parent.resources.flags.contains(RemainingFlags::TAINTED) {
            RemainingFlags::TAINTED
        } else {
            RemainingFlags::empty()
        }
    }

    /// Scheduler：特权进程特殊处理
    fn fork_scheduler(parent: &Process) -> SchedId {
        match &parent.resources.privilege {
            Privilege::Kernel => SchedId::SCHED_PROC_NR,
            Privilege::User(_) => parent.resources.scheduler,
        }
    }
}
```

#### 优势

| 优势 | 说明 |
|------|------|
| **语义可见** | 代码像"内核文档"，每个函数名直接表达语义 |
| **编译器检查** | 新增字段时，编译器强制处理，不会漏掉 |
| **可测试** | 可以单独测试 `fork_flags()`、`fork_scheduler()` 等语义函数 |
| **可演进** | 未来 vfork/clone 只需新增语义函数变体 |

#### 最佳实践：审计注释（Audit Comments）

在每个字段赋值处增加简短注释，说明为什么这样处理，防止未来"代码腐烂"：

```rust
impl Process {
    pub fn fork_from(ctx: ForkContext) -> Self {
        Self {
            identity: ProcessIdentity {
                // 继承进程组：子进程默认属于父进程的进程组
                // 参考: POSIX 4.3.1, Minix3 do_fork() line 95
                procgrp: ctx.parent.identity.procgrp,
                
                // 清零子进程时间：fork 时重置累计的子进程时间
                // 参考: Minix3 do_fork() line 120
                // 注意：child_utime/stime 是子进程的子进程时间，不是自身时间
                // ...
            },
            // ...
        }
    }
}
```

**审计注释格式**：
```
// [行为]: [原因]
// 参考: [源码位置/标准文档]
// 注意: [特殊情况/陷阱]
```

### 4.5 Fork 不变量检查（重要！）

> ⚠️ **必做**：fork 后必须验证关键不变量，防止 silent bug

```rust
impl Process {
    /// Fork 后验证不变量
    ///
    /// 在调试模式下执行，确保 fork 语义的正确性
    #[cfg(debug_assertions)]
    fn validate_after_fork(&self) {
        debug_assert_eq!(self.resources.child_utime, 0, "子进程时间应清零");
        debug_assert_eq!(self.resources.child_stime, 0, "子进程时间应清零");
        debug_assert!(self.ipc.reply.is_none(), "子进程无待回复消息");
        debug_assert!(self.ipc.event_subscriber.is_none(), "子进程无事件订阅");
        debug_assert_eq!(self.resources.intervals, [0; NR_ITIMERS], "定时器应清零");
    }
}
```

### 4.6 深坑警告（实战必踩）

#### 深坑一：Shallow Clone（智能指针陷阱）

如果 `Process` 结构体中包含智能指针（如 `Arc<Mutex<T>>`）或裸指针，`clone()` 会导致**浅拷贝**：

```rust
// 危险示例
struct Process {
    some_ptr: Arc<Mutex<Data>>,  // clone() 会共享引用！
}
```

**后果**：父子进程会共享同一块内存资源，导致竞态或过早释放。

**特别注意**：
- `signals.clone()` 必须确保是深拷贝
- 如果 `signals` 内部持有指向父进程堆栈或特定缓冲区的引用，fork 之后子进程会直接踩到父进程的内存
- 在 Minix3 源码中，子进程必须拥有**自己独立的信号处理数组空间**

**解决**：确保所有字段实现深拷贝，或使用 `ManuallyDrop`、`Owned` 等类型封装。

#### 深坑二：Partial Build 后的 Drop

在显式构造模式下，如果构造到一半发生 Panic（例如内存分配失败），Rust 会自动调用已构造字段的 Drop 函数。

**解决**：确保 Drop 函数是**幂等的**且**无副作用的**。

#### 深坑三：getticks() 时钟源

如果 `getticks()` 频率很低（如 100Hz），父子进程的 `started` 时间戳可能完全相同。

**解决**：使用高精度时间源（如 TSC/HPET）或记录相对时间戳。

**进阶建议**：
- 在高性能内核中，建议同时记录 `started` 和父进程此时的 `utime/stime` 快照
- 这是后续做资源审计的基础

#### 深坑四：Privilege::Kernel 的世袭制问题

**问题**：特权进程的子进程是否继承特权？

```rust
// Minix3 的设计中，PRIV_PROC 标志位和 scheduler 并不是简单的父子继承
// 如果一个特权服务 fork 了一个子进程，这个子进程通常不应该自动获得父进程的特权
scheduler: if parent.resources.privilege.is_kernel() {
    SchedId::SCHED_PROC_NR  // 特权进程强制绑定调度器
} else {
    parent.resources.scheduler
},
```

**建议**：
- 仔细核对 `Privilege` 枚举
- 如果子进程只是普通辅助进程，它的 `privilege` 字段可能需要降级为 `User`
- **严查 `RemainingFlags`**：不要仅仅保留 `TAINTED`，查一下 `PRIV_PROC` 标志位
- 在 Minix 中，如果父进程是特权进程，子进程通常也会携带该标志，这涉及到权限提升的安全性

#### 深坑五：name 数组的拷贝

**问题**：确保是真正的副本

```rust
name: parent.identity.name,  // ⚠️ 确保是值拷贝
```

**检查**：
- `name` 必须是 `[u8; 16]` 数组或实现了 `Copy` 的类型
- 这样赋值才是真正的副本，而不是所有权转移或引用

**优化建议**：
- 在 Minix3 中，`mp_name` 通常是固定的
- 如果在子进程名里加上一个小标记（比如 `[child]`）或者保留原始名，但在调试信息里体现出派生关系
- 这会让你后续 Debug `do_fork` 到 `exec` 之间的中间态变得非常轻松

### 4.7 子组件 Forkable Trait（可选进阶）

如果 `Process` 子组件较多，可以为每个子组件实现私有的 `fork_to_child()` 方法：

```rust
impl ProcessResources {
    fn fork_to_child(&self) -> Self {
        Self {
            child_utime: 0,
            child_stime: 0,
            started: getticks(),
            flags: Self::fork_flags(self),
            ..self.clone()
        }
    }
}

impl Process {
    pub fn fork_from(ctx: ForkContext) -> Self {
        Self {
            resources: ctx.parent.resources.fork_to_child(),
            // ... 其他字段
        }
    }
}
```

### 4.8 常见实现偏差

| 偏差 | 错误实现 | 正确行为 |
|------|---------|---------|
| `identity.id.index` | 设为 `parent.identity.id.index` | 应设为 `child_index`（子进程自己的槽位索引） |
| `resources.started` | 设为 `parent.resources.started` | 应设为 `getticks()`（当前时间） |
| `resources.intervals` | 继承父进程 | 应清零 `[0; 3]` |
| `resources.scheduler` | 继承父进程 | 特权进程应设为 `SCHED_PROC_NR` |
| `ipc.event_subscriber` | 继承父进程 | 应设为 `None` 并断言检查 |

从 C 代码 `rmc->mp_flags &= (IN_USE|DELAY_CALL|TAINTED)` 推导：

| Minix3 字段 | fork 行为 | Rust 对应 | 说明 |
|------------|----------|----------|------|
| `mp_pid` | 分配新 PID | `identity.id.pid = new_pid` | 由 `get_free_pid()` 生成 |
| `mp_endpoint` | 从 VM 返回 | `identity.endpoint = child_ep` | **不是 PM 计算的** |
| `mp_procgrp` | 继承父进程 | `identity.procgrp = parent.procgrp` | 子进程加入同一进程组 |
| `mp_name` | 继承父进程 | `identity.name = parent.name` | 进程名相同 |
| `mp_parent` | 设为调用者 | `state.guardianship.parent = who_p` | 父进程索引 |
| `mp_tracer` | 清除（默认） | `state.guardianship.tracer = None` | 除非 TO_TRACEFORK |
| `mp_trace_flags` | 清除（默认） | `state.trace = default()` | 除非 TO_TRACEFORK |
| `mp_flags` | 只保留 IN_USE/DELAY_CALL/TAINTED | `state.lifecycle = Running` | 其他标志全部清除 |
| `mp_child_utime` | 清零 | `resources.child_utime = 0` | 子进程无子进程 |
| `mp_child_stime` | 清零 | `resources.child_stime = 0` | 同上 |
| `mp_exitstatus` | 清零 | 不设置 | 尚未退出 |
| `mp_sigstatus` | 清零 | 不设置 | 同上 |
| `mp_interval[]` | 清零 | `resources.intervals = [0; 3]` | 间隔定时器清零 |
| `mp_started` | 当前时间 | `resources.started = getticks()` | 记录启动时间 |
| `mp_sigact` | 深拷贝 | `resources.signals = parent.signals.clone()` | 信号处理继承 |
| `mp_realuid/effuid/svuid` | 继承父进程 | `resources.privilege = parent.privilege.clone()` | 权限继承 |
| `mp_nice` | 继承父进程 | `resources.nice = parent.nice` | 调度优先级继承 |
| `mp_scheduler` | 特权进程设为 SCHED | `resources.scheduler` | 特权进程处理 |
| `mp_reply` | 不继承 | `ipc.reply = None` | 子进程无待回复消息 |
| `mp_eventsub` | 必须为 NO_EVENTSUB | `ipc.event_subscriber = None` | 断言检查 |

## 5. 关键逻辑点详解

### 5.1 mp_sigact 指针修复

**问题**: `*rmc = *rmp` 是 C 的结构体赋值，会把父进程的 `mp_sigact` 指针原样复制过来。但每个进程槽有自己的 `mpsigact[slot]` 缓冲区。

**修复**:
```c
rmc->mp_sigact = mpsigact[next_child];                    // 恢复子进程自己的指针
memcpy(rmc->mp_sigact, rmp->mp_sigact, sizeof(...));      // 复制内容到子进程缓冲区
```

**Rust 对应**: 不存在此问题。Rust 的 `Clone` 是深拷贝，`SignalState` 包含值类型，无指针。

### 5.2 vm_fork 失败的回滚

**关键**: `vm_fork()` 在 `procs_in_use++` **之前**调用。如果失败，无需回滚。

但注意：如果 `vm_fork()` 成功，后续代码**不能失败**，因为 VM 已经调用了 `sys_fork()` 创建了内核进程。

### 5.3 SUSPEND 机制

PM 返回 `SUSPEND` 给内核，表示父进程需要挂起。VFS 处理完 `VFS_PM_FORK` 后回复 `VFS_PM_FORK_REPLY`，PM 收到后唤醒父进程，父进程的用户态 `fork()` 返回子进程 PID。

### 5.4 tell_vfs 异步机制

```c
void tell_vfs(rmp, m_ptr) {
    if (rmp->mp_flags & (VFS_CALL | EVENT_CALL))
        panic("tell_vfs: not idle");
    r = asynsend3(VFS_PROC_NR, m_ptr, AMF_NOREPLY);
    rmp->mp_flags |= VFS_CALL;
}
```

- 使用 `asynsend3`（异步发送，不阻塞）
- 设置 `VFS_CALL` 标志防止重复发送
- VFS 回复后，PM 在 `do_fork_reply()` 中清除 `VFS_CALL` 并唤醒父进程

## 6. 常见实现偏差

| 偏差 | 错误实现 | 正确行为 |
|------|---------|---------|
| `identity.id.index` | 设为 `parent.identity.id.index` | 应设为 `child_index`（子进程自己的索引） |
| `resources.started` | 设为 `parent.resources.started` | 应设为 `getticks()`（当前时间） |
| `resources.intervals` | 继承父进程 | 应清零 `[0; 3]` |
| `resources.flags` | 继承父进程 | 只保留 TAINTED，清除其他 |
| `resources.scheduler` | 继承父进程 | 特权进程应设为 `SCHED_PROC_NR` |
| `ipc.event_subscriber` | 未处理 | 应设为 `None` 并断言 |

## 7. 错误处理

## 8. 测试验证

### 8.1 单元测试

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn create_parent_process() -> Process {
        let mut proc = Process::new(0, 100);
        proc.identity.procgrp = 100;
        proc.resources.nice = 5;
        proc.resources.flags = RemainingFlags::TAINTED | RemainingFlags::ALARM_ON;
        proc.state.lifecycle = Lifecycle::Running;
        proc
    }

    #[test]
    fn test_fork_child_index() {
        let parent = create_parent_process();
        let child = Process::fork_from(&parent, 5, 200, Endpoint::new(5));
        
        assert_eq!(child.identity.id.index, ProcIndex::new(5));
        assert_eq!(child.identity.id.pid, 200);
    }

    #[test]
    fn test_fork_inherited_fields() {
        let parent = create_parent_process();
        let child = Process::fork_from(&parent, 5, 200, Endpoint::new(5));
        
        assert_eq!(child.identity.procgrp, parent.identity.procgrp);
        assert_eq!(child.resources.nice, parent.resources.nice);
    }

    #[test]
    fn test_fork_cleared_fields() {
        let parent = create_parent_process();
        let child = Process::fork_from(&parent, 5, 200, Endpoint::new(5));
        
        assert_eq!(child.resources.child_utime, 0);
        assert_eq!(child.resources.child_stime, 0);
        assert_eq!(child.resources.intervals, [0; NR_ITIMERS]);
    }

    #[test]
    fn test_fork_flags_handling() {
        let mut parent = create_parent_process();
        parent.resources.flags = RemainingFlags::TAINTED | RemainingFlags::ALARM_ON;
        
        let child = Process::fork_from(&parent, 5, 200, Endpoint::new(5));
        
        assert!(child.resources.flags.contains(RemainingFlags::TAINTED));
        assert!(!child.resources.flags.contains(RemainingFlags::ALARM_ON));
    }

    #[test]
    fn test_fork_parent_relationship() {
        let parent = create_parent_process();
        let child = Process::fork_from(&parent, 5, 200, Endpoint::new(5));
        
        assert_eq!(child.parent(), ProcIndex::new(0));
    }
}
```

### 8.2 编译器防御测试（关键！）

```rust
// 必须做一次：故意新增字段，看编译器是否报错
// 这是整个设计最核心的验证

// 步骤：
// 1. 在 Process 结构体中添加 new_field: u32
// 2. 编译
// 3. 期望：error[E0063]: missing field `new_field` in initializer of `Process`
// 4. 如果编译通过，说明防御机制失效
```

### 8.3 Fuzz 测试

```rust
#[cfg(test)]
mod fuzz_tests {
    use super::*;
    use rand::Rng;

    fn random_process(rng: &mut impl Rng) -> Process {
        let mut proc = Process::default();
        proc.identity.id.pid = rng.gen();
        proc.resources.nice = rng.gen_range(-20..=20);
        proc.resources.flags = RemainingFlags::from_bits(rng.gen()).unwrap_or_default();
        proc
    }

    #[test]
    fn fuzz_fork_invariants() {
        let mut rng = rand::thread_rng();
        
        for _ in 0..1000 {
            let parent = random_process(&mut rng);
            let child = Process::fork_from(
                &parent, 
                rng.gen_range(0..100),
                rng.gen(),
                Endpoint::new(rng.gen()),
            );
            
            // 验证不变量
            assert_eq!(child.resources.child_utime, 0);
            assert_eq!(child.resources.child_stime, 0);
            assert!(child.ipc.reply.is_none());
        }
    }
}
```

### 8.4 验证清单

#### 字段验证

- [ ] `identity.id.index` = `child_index`（不是父进程索引）
- [ ] `identity.id.pid` = `child_pid`
- [ ] `identity.endpoint` = `child_endpoint`
- [ ] `identity.procgrp` = 父进程的 procgrp
- [ ] `identity.name` = 父进程的 name
- [ ] `state.lifecycle` = `Lifecycle::Running`
- [ ] `state.guardianship.parent` = 父进程索引
- [ ] `state.guardianship.tracer` = `None`
- [ ] `resources.child_utime` = `0`
- [ ] `resources.child_stime` = `0`
- [ ] `resources.started` = `getticks()`
- [ ] `resources.intervals` = `[0; 3]`
- [ ] `resources.flags` 只保留 `TAINTED`
- [ ] `ipc.reply` = `None`
- [ ] `ipc.event_subscriber` = `None`

#### 深坑检查

- [ ] `signals.clone()` 是深拷贝
- [ ] `privilege.clone()` 是深拷贝
- [ ] `name` 是值拷贝
- [ ] `getticks()` 时钟源正确
- [ ] 无有副作用的 `Drop` 实现
- [ ] 特权进程的 scheduler 正确设置

#### 测试验证

- [ ] 编译器防御测试通过（新增字段编译失败）
- [ ] 单元测试全部通过
- [ ] Fuzz 测试通过
