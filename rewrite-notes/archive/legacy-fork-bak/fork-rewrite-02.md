# do_fork 核心逻辑（上）- 设计方案分析

> 阶段2：参数检查与槽位分配

## 〇、重要说明：Minix3 多进程表架构

> ⚠️ **关键认知**：Minix3 采用分布式进程表设计，共有 **4 份进程表**，分别由不同组件管理：

| 组件 | 进程表名 | C 文件 | 核心职责 |
|------|---------|--------|---------|
| **Kernel** | `proc[NR_TASKS + NR_PROCS]` | `kernel/proc.h` | 调度、IPC、寄存器保存 |
| **PM** | `mproc[NR_PROCS]` | `servers/pm/mproc.h` | **进程管理、信号、权限** |
| **VM** | `vmproc[NR_PROCS]` | `servers/vm/vmproc.h` | 虚拟内存、页表 |
| **VFS** | `fproc[NR_PROCS]` | `servers/vfs/fproc.h` | 文件描述符、目录 |

**本文档聚焦 PM 的 `mproc` 表**，不涉及 kernel/VM/VFS 的进程表。

### Fork 调用跨服务器协调

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        Fork 调用跨服务器协调                                  │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ① PM: do_fork()                                                           │
│     ├── 检查进程表是否已满                                                   │
│     ├── 查找空闲 mproc 槽位                                                 │
│     ├── 生成新 PID                                                          │
│     └── 复制父进程 mproc                                                    │
│                                                                             │
│  ② PM → VM: vm_fork() IPC 调用                                             │
│     ├── VM 分配 vmproc 槽位                                                 │
│     ├── VM 复制父进程地址空间                                               │
│     ├── VM 生成新 endpoint                                                  │
│     └── VM 返回 endpoint 给 PM                                              │
│                                                                             │
│  ③ PM → VFS: tell_vfs(VFS_PM_FORK) IPC 调用                               │
│     ├── VFS 分配 fproc 槽位                                                 │
│     ├── VFS 复制文件描述符表                                                │
│     ├── VFS 复制工作目录                                                    │
│     └── VFS 回复 PM                                                         │
│                                                                             │
│  ④ Kernel: sys_fork() (由 VM 触发)                                         │
│     ├── 分配 proc 槽位                                                      │
│     └── 初始化调度状态                                                      │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Rust 重构目录结构（多进程表支持）

> **重要架构决策**: 根据 Gemini 的建议，MProc 应该放在 PM crate 中，而不是 minix-types。
> 原因：职责隔离、不变量保护、微内核原则。

```
os/
├── libs/
│   └── minix-types/           # 核心协议类型（跨服务通讯）
│       └── src/
│           ├── lib.rs         # 只导出核心类型
│           ├── types/
│           │   ├── pid.rs     # Pid, Endpoint, ProcIndex
│           │   ├── id.rs      # Uid, Gid, IdSet
│           │   └── clock.rs   # Clock, VirBytes
│           └── ipc/
│               └── message.rs # Message
│
└── servers/
    ├── pm/                    # ⭐ PM 服务（lib + bin）
    │   └── src/
    │       ├── lib.rs         # 导出 mproc 模块
    │       ├── main.rs        # PM 主循环
    │       ├── mproc/         # PM 私有进程表
    │       │   ├── mod.rs
    │       │   ├── mproc.rs   # Process 结构体
    │       │   ├── table.rs   # ProcTable
    │       │   ├── context.rs # PmContext
    │       │   └── ...
    │       ├── fork.rs        # fork 系统调用入口
    │       └── ...
    │
    ├── vm/                    # 🔜 VM 服务（后续阶段）
    │   └── src/
    │       └── vmproc/
    │
    ├── vfs/                   # 🔜 VFS 服务（后续阶段）
    │   └── src/
    │       └── fproc/
    │
    └── kernel/                # 🔜 Kernel（后续阶段）
        └── src/
            └── proc/
```

### 为什么 MProc 放在 PM crate 而不是 minix-types？

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

---

## 一、问题背景

根据 `fork-syscall-plan.md` 第二阶段，需要实现：

1. **进程表存储**：如何存储 256 个进程结构
2. **计数器**：如何维护 `procs_in_use` 计数
3. **槽位查找**：如何找到空闲槽位
4. **当前进程**：如何表达 C 的 `mp` 宏（当前进程指针）

---

## 二、核心认知：Rewrite vs Redesign

> **这是整个设计决策的分水岭**

### 2.1 两个阶段的本质区别

| 维度 | Rewrite 阶段 | Redesign 阶段 |
|------|-------------|--------------|
| **目标** | 忠实翻译 Minix3 | 进化架构 |
| **约束** | 行为 100% 对齐 | 可以改变模型 |
| **数据结构** | 保持 Minix 语义 | 可以引入 Rust 语义 |
| **状态模型** | 共享可变状态 | 可以改变所有权模型 |
| **核心原则** | 最小化心智负担 | 去中心化、无锁、极致性能 |

### 2.2 Rewrite 阶段推荐方案

```text
进程表：[Process; NR_PROCS]
计数器：Cell<usize>
槽位：轮询
current：PmContext
IN_USE：保留
```

**一句话总结**：

> **Rewrite = 结构翻译 + 类型安全，不改变模型**

### 2.3 Redesign 阶段可选方案

```text
进程表：Per-Core 本地队列 / Arena 分配器
槽位：空闲栈 / 无锁哈希表
并发：Atomic + UMWAIT
所有权：Generational Index
```

**一句话总结**：

> **Redesign = 改变资源模型 + ownership 语义**

---

## 三、进程表存储方案

### 3.1 方案对比

| 方案 | 类型签名 | 内存布局 | 分配时机 | no_std | Rewrite | Redesign |
|------|---------|---------|---------|--------|---------|----------|
| **A. 静态数组** | `[Process; NR_PROCS]` | 连续 | 编译期 | ✅ | ✅ 推荐 | ✅ 可用 |
| **B. MaybeUninit** | `[MaybeUninit<Process>; NR_PROCS]` | 连续 | 编译期 | ✅ | ⚠️ 可用 | ⚠️ 可用 |
| **C. Option 数组** | `[Option<Process>; NR_PROCS]` | 连续 | 编译期 | ✅ | ❌ 不推荐 | ✅ 推荐 |
| **D. Vec** | `Vec<Process>` | 连续 | 运行期 | ⚠️ | ❌ | ⚠️ |
| **E. Slab** | `slab::Slab<Process>` | 分散 | 运行期 | ⚠️ | ❌ | ✅ 可选 |

### 3.2 关键洞察：Minix mproc 的本质

> **Minix 的 mproc 不是"容器"，而是"内核状态空间的一部分"**

这意味着：

```text
它不是 Vec vs Array 的问题
而是：
"是否允许 relocation / ownership 变化"
```

### 3.3 方案 A：静态数组（Rewrite 推荐）

```rust
pub const NR_PROCS: usize = 256;
pub const LAST_FEW: usize = 5;

pub struct ProcTable {
    procs: [Process; NR_PROCS],
    procs_in_use: Cell<usize>,
    next_child: Cell<usize>,
}
```

**优点**：
- ✅ 零运行时分配
- ✅ 缓存友好（连续内存）
- ✅ 索引 O(1)
- ✅ no_std 完美兼容
- ✅ **与 Minix3 一致**

**缺点**：
- ❌ 大小固定
- ❌ 即使空闲也占用全部内存

**内存占用**：
```
Process ≈ 88 bytes
256 进程 ≈ 22.5 KB
```

### 3.4 方案 B：MaybeUninit 数组

```rust
use core::mem::MaybeUninit;

pub struct ProcTable {
    procs: [MaybeUninit<Process>; NR_PROCS],
    initialized: [bool; NR_PROCS],
    procs_in_use: Cell<usize>,
}
```

**优点**：
- ✅ 延迟初始化

**缺点**：
- ❌ 需要 `unsafe` 代码
- ❌ 额外的 `initialized` 数组
- ❌ **复杂度增加，收益有限**
- ❌ **unsafe 会像野草一样蔓延**

**结论**：不推荐。对于练手项目，会显著增加处理 panic 和内存安全的负担。

### 3.5 方案 C：Option 数组（Redesign 推荐）

```rust
pub struct ProcTable {
    procs: [Option<Process>; NR_PROCS],
    procs_in_use: Cell<usize>,
}
```

**⚠️ 关键警告**：这**不是简单替换 IN_USE**

#### Option 的语义问题

在 Rust 中，`Option` 的语义是：
- `Some(proc)`: 栈/结构体**拥有**这个 Process 实例
- `None`: 栈/结构体**不拥有**这个实例

**Minix 的痛点（多指针引用）**：

在 Minix C 代码中，一个进程结构体（mproc）通常被多个地方引用：
- 进程表数组：`mproc[pid]`
- 哈希表/映射：通过 PID 或 Key 快速查找
- 当前运行指针：`mp` 宏指向当前进程
- 父/子指针：`mp_parent` 指向父进程

**问题**：如果你用 `Option`，只有一个地方能**拥有**它。一旦你把它从数组中 `.take()` 出来（变成 `None`），你就无法同时保留在哈希表里，也无法让 `mp` 指针指向它。

#### 状态模型对比

| 维度 | IN_USE 模型 | Option 模型 |
|------|------------|-------------|
| **内存稳定性** | Process 永远在 | Process 会被 drop |
| **地址稳定性** | ✅ 稳定 | ❌ 不稳定 |
| **生命周期** | 内部状态机 | 外部控制 |
| **Aliasing** | 多方共享同一 struct | 可能失效 |
| **OS 语义** | ✅ 匹配 Minix | ⚠️ 偏离 |

#### 本质区别

```text
IN_USE 模型：
  Process 是 stable object
  状态在内部变化

Option 模型：
  Process 是 ephemeral object
  存在性在外部控制
```

**使用条件**：
- 必须禁止"持久引用"
- 必须使用 handle-based 设计（`ProcId → 每次 lookup → &Process`）

### 3.6 方案 D：Vec

```rust
pub struct ProcTable {
    procs: Vec<Process>,
}
```

**结论**：不推荐。除非要做可以动态扩容进程数的现代内核（如 Linux），否则在 Minix 这种固定槽位的架构里，`Vec` 显得格格不入。

### 3.7 方案 E：Slab

```rust
use slab::Slab;

pub struct ProcTable {
    procs: Slab<Process>,
}
```

**优点**：
- ✅ 自动管理空闲槽位
- ✅ O(1) 插入/删除

**缺点**：
- ❌ 依赖外部 crate
- ❌ 需要分配器
- ❌ 内存不连续

**结论**：Redesign 阶段可选。

---

## 四、计数器方案

### 4.1 关键问题：Minix PM 的"单线程"语义

> **Minix 的"单线程"不是 Rust 意义的单线程**

它是：
- 单线程执行
- **但可能被中断**
- 状态可以被其他 subsystem 观察

真实语义是：

```text
"逻辑单线程 + 物理共享内存"
```

### 4.2 方案对比

| 方案 | 类型 | 线程安全 | 性能 | 内部可变 | Rewrite | Redesign |
|------|------|---------|------|---------|---------|----------|
| **A. usize** | `usize` | ❌ | ⚡ 最快 | ❌ | ⚠️ 需 `&mut self` | ❌ |
| **B. Cell** | `Cell<usize>` | ❌ | ⚡ 最快 | ✅ | ✅ 推荐 | ⚠️ 可用 |
| **C. RefCell** | `RefCell<usize>` | ❌ | 🚀 快 | ✅ | ❌ 过于复杂 | ⚠️ |
| **D. Atomic** | `AtomicUsize` | ✅ | 🚀 中等 | ✅ | ❌ 不需要 | ✅ 推荐 |
| **E. Mutex** | `Mutex<usize>` | ✅ | 🐢 最慢 | ✅ | ❌ | ⚠️ |

### 4.3 方案 B：Cell<usize>（Rewrite 推荐）

```rust
use core::cell::Cell;

pub struct ProcTable {
    procs: [Process; NR_PROCS],
    procs_in_use: Cell<usize>,
    next_child: Cell<usize>,
}

impl ProcTable {
    pub fn find_free_slot(&self) -> Option<usize> {
        // ...
        self.procs_in_use.set(self.procs_in_use.get() + 1);
        // ...
    }
}
```

**优点**：
- ✅ 内部可变性（`&self` 方法修改计数器）
- ✅ 零运行时开销
- ✅ 单线程安全（编译器保证）

**原理**：
```
Cell<T> 对于 Copy 类型：
- get(): 复制值出来
- set(): 复制值进去
- 不需要借用检查
```

### 4.4 方案 D：AtomicUsize（Redesign 推荐）

```rust
use core::sync::atomic::{AtomicUsize, Ordering};

pub struct ProcTable {
    procs: [UnsafeCell<Process>; NR_PROCS],
    procs_in_use: AtomicUsize,
    next_child: AtomicUsize,
}
```

**适用场景**：多线程访问进程表。如果引入多核并行处理，必须替换为 `AtomicUsize`。

---

## 五、槽位查找算法

### 5.1 方案对比

| 方案 | 时间复杂度 | 空间开销 | 实现复杂度 | 缓存友好 | Rewrite | Redesign |
|------|-----------|---------|-----------|---------|---------|----------|
| **A. 轮询** | O(N) 最坏 | 0 | ⭐ 简单 | ✅ | ✅ 推荐 | ⚠️ 可用 |
| **B. 空闲链表** | O(1) | O(N) 指针 | ⭐⭐ | ❌ | ❌ | ⚠️ |
| **C. 位图** | O(N/64) | N/8 bytes | ⭐⭐ | ✅ | ⚠️ | ✅ 可选 |
| **D. 空闲栈** | O(1) | O(N) 索引 | ⭐⭐ | ⚠️ | ❌ | ✅ 推荐 |

### 5.2 方案 A：轮询（Rewrite 推荐）

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

**优点**：
- ✅ 最简单（直接翻译 C 代码）
- ✅ 零额外空间
- ✅ 缓存友好
- ✅ 均匀分布
- ✅ **最符合 Minix 原教旨主义**

**性能分析**：
```
平均：O(N/2) 次检查
最坏：O(N) 次检查
实际：fork 频率不高，可接受
```

### 5.3 方案 D：空闲栈（Redesign 推荐）

```rust
pub struct ProcTable {
    procs: [Process; NR_PROCS],
    free_stack: [usize; NR_PROCS],
    free_top: Cell<usize>,
}

impl ProcTable {
    pub fn alloc_slot(&self) -> Option<usize> {
        if self.free_top.get() == 0 {
            return None;
        }
        self.free_top.set(self.free_top.get() - 1);
        Some(self.free_stack[self.free_top.get()])
    }
    
    pub fn free_slot(&self, slot: usize) {
        self.free_stack[self.free_top.get()] = slot;
        self.free_top.set(self.free_top.get() + 1);
    }
}
```

**优点**：
- ✅ O(1) 分配和释放
- ✅ 实现简单
- ✅ 缓存友好（栈是连续数组）

**额外开销**：`NR_PROCS * 8` bytes ≈ 2KB（对于现代系统可忽略不计）

---

## 六、当前进程表达方案

### 6.1 问题分析

C 代码中的 `mp` 宏：

```c
#define mp (&mproc[who_p])  // 全局隐式状态
```

### 6.2 方案对比

| 方案 | 描述 | Rewrite | Redesign |
|------|------|---------|----------|
| **A. 参数传递** | 每个函数接收 `current: usize` | ⚠️ 参数多 | ⚠️ |
| **B. 上下文结构** | `PmContext { table, current }` | ✅ 推荐 | ✅ 推荐 |
| **C. 线程局部** | `thread_local!` | ❌ no_std 有限 | ⚠️ |
| **D. 全局静态** | `static CURRENT: AtomicUsize` | ❌ 不安全 | ❌ |

### 6.3 方案 B：PmContext（推荐）

```rust
pub struct PmContext<'a> {
    pub table: &'a mut ProcTable,
    pub current: usize,
}

impl<'a> PmContext<'a> {
    pub fn current_proc(&self) -> &Process {
        &self.table.procs[self.current]
    }
    
    pub fn current_proc_mut(&mut self) -> &mut Process {
        &mut self.table.procs[self.current]
    }
    
    pub fn do_fork(&mut self) -> Result<Pid, ForkError> {
        // ...
    }
}
```

**优点**：
- ✅ **把 C 的隐式全局状态 → Rust 的显式 capability**
- ✅ 封装良好
- ✅ 可测试
- ✅ 类型安全
- ✅ **强制在编译期理清依赖关系**

**这是整个重写工程的"神来之笔"**

---

## 七、IN_USE 要不要删？

> **这是整个 rewrite 的分水岭**

### 7.1 结论

| 阶段 | 是否删除 IN_USE | 替代方案 |
|------|----------------|---------|
| **Rewrite** | ❌ 绝对不要删 | 保持 `Lifecycle::Unused` |
| **Redesign** | ✅ 可以删除 | `Option<Process>` 或 Arena |

### 7.2 为什么 Rewrite 不能删？

**Minix 的语义是**：

```c
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

### 7.3 更深层原因

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

### 7.4 IN_USE 的本质

```text
IN_USE 的本质是：
"这个 slot 当前参与系统协议"

而不是：
"这个对象存在不存在"
```

### 7.5 Minix 允许的"看起来非法但实际合理"的状态

- Exiting + Blocked
- Zombie + traced
- slot 存在但未初始化完成（fork 中间态）

---

## 八、所有权与多所有者问题

### 8.1 核心冲突

> **Rust 要唯一 `&mut`，Minix 有多个"逻辑 `&mut`"**

### 8.2 三条路线

#### 路线 A：内核风格（推荐）

```rust
struct ProcTable {
    procs: [UnsafeCell<Process>; N]
}
```

- 外层保证：单线程或锁
- 优点：接近真实 OS，性能最好
- 本质：**承认 Rust borrow checker 不适合内核共享模型**

#### 路线 B：Rust 纯净模型

```rust
Option<Process>
Rc<RefCell<Process>>
```

- 问题：性能差，no_std 不友好，不像 OS
- 会把借用检查从"编译期"移到"运行期"
- 如果违反规则，程序会在运行时 panic

#### 路线 C：ECS / handle-based（高级）

```rust
ProcId → table → Process
```

- 优点：无 borrow 冲突，可扩展
- 这是 seL4 / Rust OS 更现代的方向
- 使用 **Generational Index** 解决"指针悬挂"问题

---

## 九、编译器并发检查：Rust 的杀手锏

> **利用借用检查器作为形式化验证工具，来保证内核的正确性**

### 9.1 Rust 的核心规则

Rust 的核心规则只有两条，但它们能杜绝 90% 的 OS 内核 Bug：

1. **任意时刻，你可以拥有任意数量的 `&T`（只读引用）**
2. **任意时刻，你最多只能拥有一个 `&mut T`（可变引用），且不能同时存在 `&T`**

### 9.2 在进程表中的应用

#### 场景 A：读取进程状态（安全）

```rust
// 你有 10 个不同的函数同时拿着 &Process 去读 PID 或状态
let p1: &Process = table.get(pid1);
let p2: &Process = table.get(pid2);
// 编译器：✅ 放行。这是并行读，没有数据竞争。
```

#### 场景 B：修改进程状态（安全）

```rust
// 只有 1 个函数拿着 &mut Process 去修改
let p: &mut Process = table.get_mut(pid);
p.lifecycle = Lifecycle::Running;
// 编译器：✅ 放行。这是独占写，没有脏读。
```

#### 场景 C：读写冲突（危险）

```rust
let p: &Process = table.get(pid);  // 拿到只读引用
// 此时，你持有 &Process
// 调度器想要调用 table.remove(pid) 来释放内存
// remove 需要 &mut self (可变借用)
// 编译器：❌ 直接报错！
// "cannot borrow proc as mutable because it is also borrowed as immutable"
```

### 9.3 C 语言的致命问题 vs Rust 的编译期保护

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

### 9.4 PmContext 作为"编译时的锁"

```rust
pub struct PmContext<'a> {
    pub table: &'a mut ProcTable,  // 锁定了对 ProcTable 的访问权限
    pub current: usize,
}
```

当你创建 `PmContext` 时，你锁定了对 `ProcTable` 的访问权限：
- 如果你创建的是 `&mut ProcTable`，Rust 会保证在整个 `PmContext` 生命周期内，没有其他代码能偷偷摸摸地去读或写进程表
- **这就是编译时的锁（Compile-time Mutex）**

### 9.5 最终结论

你的策略是完美的：

```text
裸数组 [Process; NR_PROCS]：保证内存连续、零开销、布局固定
&Process 和 &mut Process：利用编译器的线性类型系统来强制执行互斥访问
```

**这正是 Rust 在操作系统开发中的核心价值**：

> 它用静态分析（编译时检查）替代了传统操作系统中复杂的动态锁（运行时 Mutex）。在单核（或 BKL）场景下，这能帮你消灭掉绝大多数的竞态条件（Race Conditions）。

---

## 十、综合推荐方案

### 10.1 Rewrite 阶段（当前）

```rust
use core::cell::Cell;

pub const NR_PROCS: usize = 256;
pub const LAST_FEW: usize = 5;

pub struct ProcTable {
    procs: [Process; NR_PROCS],
    procs_in_use: Cell<usize>,
    next_child: Cell<usize>,
}

pub struct PmContext<'a> {
    pub table: &'a mut ProcTable,
    pub current: usize,
}
```

**理由**：
1. 完全匹配 Minix3 模型
2. 地址稳定，无 alias 问题
3. no_std 完美兼容
4. 实现最简单
5. **利用借用检查器作为形式化验证工具**

### 10.2 Redesign 阶段（未来）

```rust
use core::sync::atomic::{AtomicUsize, AtomicU32, Ordering};

pub struct ProcTable {
    procs: [UnsafeCell<Process>; NR_PROCS],
    procs_in_use: AtomicUsize,
    free_stack: [usize; NR_PROCS],
    free_top: AtomicUsize,
    generation: [AtomicU32; NR_PROCS],  // Endpoint 代数 / Generational Index
}
```

**改进点**：
1. 空闲栈替代轮询
2. `AtomicUsize` 支持多线程
3. `generation` 数组支持 Endpoint 代数
4. **Generational Index 解决 ABA 问题（指针悬挂）**

---

## 十一、do_fork 实现建议

### 11.1 不要使用 `#[derive(Clone)]`

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

### 11.2 Endpoint 的计算

Minix3 的进程索引和端点是两回事。**Generation 嵌入在 Endpoint 中，不需要单独存储**：

```rust
/// Endpoint generation 位移
/// Minix3 定义：`#define _ENDPOINT_GENERATION_SHIFT 15`
pub const ENDPOINT_GENERATION_SHIFT: u32 = 15;

pub struct ProcTable {
    procs: [Process; NR_PROCS],
    // 注意：generation 嵌入在 Process.endpoint 中，不需要单独数组
}

impl ProcTable {
    /// 计算 Endpoint
    /// Minix3 公式：endpoint = (generation << 15) + proc_nr
    pub fn calculate_endpoint(index: usize) -> Endpoint {
        // 初始 generation = 0
        Endpoint::new(index as i32)
    }
    
    /// 从 Endpoint 解析索引
    /// Minix3 公式：proc_nr = endpoint & 0x7FFF
    pub fn endpoint_to_index(endpoint: Endpoint) -> usize {
        (endpoint.get() & 0x7FFF) as usize
    }
    
    /// 从 Endpoint 解析代数
    /// Minix3 公式：generation = endpoint >> 15
    pub fn endpoint_to_generation(endpoint: Endpoint) -> u32 {
        (endpoint.get() >> ENDPOINT_GENERATION_SHIFT) as u32
    }
    
    /// 释放槽位时增加 generation
    pub fn release_slot(&mut self, idx: usize) {
        // 增加 generation，嵌入到 endpoint 中
        let old_endpoint = self.procs[idx].endpoint();
        let new_endpoint = Self::increment_endpoint_generation(old_endpoint);
        self.procs[idx].identity.endpoint = new_endpoint;
    }
}
```

> ⚠️ **重要**：Minix3 的 Endpoint 公式是 `(generation << 15) + proc_nr`（加法），不是或运算。
> 低 15 位是进程槽位号，高 17 位是代数。

### 11.3 显式化子进程的"第一口气"

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
```

### 11.4 确保 Process::default() 的正确性

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

## 十二、方案总结表

| 设计点 | Rewrite | Redesign | 核心理由 |
|--------|---------|----------|---------|
| 进程表存储 | `[Process; NR_PROCS]` | `[UnsafeCell<Process>; NR_PROCS]` 或 Arena | 固定大小、地址稳定 |
| IN_USE | 保留 `Lifecycle::Unused` | 可用 `Option` 或 Generational Index | Minix 语义 vs Rust 语义 |
| 计数器 | `Cell<usize>` | `AtomicUsize` | 单线程 vs 多线程 |
| 槽位查找 | 轮询 | 空闲栈 | 简单 vs 性能 |
| 当前进程 | `PmContext<'a>` | `PmContext<'a>` | 显式、可测试 |
| 并发检查 | 借用检查器（编译时） | Atomic（运行时） | 形式化验证 vs 动态锁 |

---

## 十三、测试用例

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_proc_table_new() {
        let table = ProcTable::new();
        assert_eq!(table.procs_in_use.get(), 0);
    }
    
    #[test]
    fn test_find_free_slot() {
        let table = ProcTable::new();
        let slot = table.find_free_slot().unwrap();
        assert!(slot < NR_PROCS);
        assert_eq!(table.procs_in_use.get(), 1);
    }
    
    #[test]
    fn test_table_full() {
        let table = ProcTable::new();
        
        for _ in 0..NR_PROCS {
            table.find_free_slot().unwrap();
        }
        
        assert!(table.find_free_slot().is_none());
    }
    
    #[test]
    fn test_last_few_for_non_root() {
        let mut table = ProcTable::new();
        table.procs[0].resources.privilege = Privilege::User(Credentials::new(1000, 100));
        
        let ctx = PmContext::new(&mut table, 0);
        
        for _ in 0..(NR_PROCS - LAST_FEW - 1) {
            ctx.table.find_free_slot().unwrap();
        }
        
        assert!(!ctx.can_alloc());
    }
    
    #[test]
    fn test_generation_increment() {
        let table = ProcTable::new();
        let gen_before = table.generation[0].get();
        
        table.release_slot(0);
        
        assert_eq!(table.generation[0].get(), gen_before + 1);
    }
    
    #[test]
    fn test_borrow_checker_protection() {
        let mut table = ProcTable::new();
        let p: &Process = table.get(0);  // 不可变借用
        
        // 下面这行会编译失败：
        // table.get_mut(0);  // 错误：不能同时有可变和不可变借用
        
        // 这就是编译时的锁！
    }
}
```

---

## 十四、参考资料

- Minix3 源码：`minix/servers/pm/forkexit.c`
- Rust Cell 文档：https://doc.rust-lang.org/core/cell/struct.Cell.html
- 设计方案：`notes/rewrite/fork-syscall-rewrite/fork-rewrite-01.md`
- 对话记录：`notes/rewrite/fork-syscall-rewrite/fork-rewr-02-chat.md`
