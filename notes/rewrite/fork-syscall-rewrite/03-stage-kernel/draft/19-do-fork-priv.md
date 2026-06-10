# 19-do-fork-priv - do_fork 特权与标志处理

> 本文档分析 `minix3/minix/kernel/system/do_fork.c` 第 131-136 行，讲解 do_fork 函数的特权与标志处理部分。

---

## 1. 概述

特权与标志处理是 fork 系统调用的最后阶段，确保子进程以正确的状态和权限开始运行。在完成进程结构复制、端点生成、初始化设置后，内核需要处理几个关键的标志位：

1. **VM 抑制标志**：如果 PM 指示需要 VM 设置页表，设置 `RTS_VMINHIBIT` 标志
2. **信号标志清除**：清除 `RTS_SIGNALED`、`RTS_SIG_PENDING`、`RTS_P_STOP` 等信号相关标志
3. **待处理信号清除**：清空子进程的待处理信号集
4. **页表指针清零**：清零子进程的页表指针，等待 VM 设置新页表

这些处理确保子进程不会继承父进程的信号状态和内存映射，以干净的状态等待 PM 和 VM 的后续配置。

### 1.1 VM 抑制标志

VM 抑制标志 `RTS_VMINHIBIT` 表示进程被 VM（虚拟内存管理器）抑制，不能运行。这个标志用于协调 fork 时内核与 VM 的交互：

1. **fork 时序**：内核完成 fork 后，子进程需要新的页表（内存映射）
2. **VM 职责**：VM 负责创建和管理进程的内存映射
3. **同步机制**：内核设置 `RTS_VMINHIBIT`，阻止子进程运行，直到 VM 设置完成

这种设计将内存管理职责分离给 VM，内核只负责低级的进程状态管理。

### 1.2 信号标志处理

信号标志的处理确保子进程不继承父进程的信号状态：

1. **RTS_SIGNALED**：父进程被信号中断的标志，子进程不应继承
2. **RTS_SIG_PENDING**：父进程有待处理信号，子进程不应继承
3. **RTS_P_STOP**：父进程被停止（如 SIGSTOP），子进程不应继承

POSIX 标准规定 fork 后子进程的信号状态：
- 待处理信号集为空
- 信号处理函数继承（因为共享代码段）
- 信号屏蔽字继承
- 忽略的信号设置继承

Minix3 通过清除这些标志和清空信号集来实现 POSIX 语义。

---

## 2. C 源码分析

本节逐行分析 `do_fork.c` 第 131-136 行的代码，涵盖 VM 抑制标志设置、信号标志清除、待处理信号清除和页表指针清零。这些操作是 fork 的收尾工作，确保子进程状态干净。

### 2.1 VM 抑制标志

VM 抑制标志设置对应源码第 131-133 行：

```c
if(m_ptr->m_lsys_krn_sys_fork.flags & PFF_VMINHIBIT)
    RTS_SET(rpc, RTS_VMINHIBIT);
```

这段代码检查 PM 传递的 `PFF_VMINHIBIT` 标志，如果设置，则将子进程标记为 VM 抑制状态。

#### 2.1.1 PFF_VMINHIBIT 检查

`m_ptr->m_lsys_krn_sys_fork.flags & PFF_VMINHIBIT` 检查 PM 在系统调用消息中设置的 `PFF_VMINHIBIT` 标志。

`PFF_VMINHIBIT`（Process Fork Flags - VM Inhibit）是 PM 传递给内核的标志，表示子进程需要等待 VM 设置页表。

PM 设置此标志的条件：
1. 子进程是用户进程（非内核任务）
2. VM 需要为新进程创建内存映射
3. fork 后需要同步等待 VM 完成

这个标志是 PM 和内核之间的协议，PM 知道哪些进程需要 VM 处理。

#### 2.1.2 RTS_VMINHIBIT 设置

`RTS_SET(rpc, RTS_VMINHIBIT)` 设置子进程的 `RTS_VMINHIBIT` 标志。

`RTS_VMINHIBIT` 标志的含义：
- 进程被 VM 抑制，不能运行
- 等待 VM 设置页表后清除
- 与 `RTS_NO_QUANTUM` 类似，都是阻止运行的条件

设置此标志后：
1. 子进程不可运行（`p_rts_flags != 0`）
2. VM 收到 fork 通知后创建新页表
3. VM 调用内核设置页表，内核清除 `RTS_VMINHIBIT`
4. 子进程变为可运行

这是微内核架构中职责分离的体现：内核管理进程状态，VM 管理内存映射。

#### 2.1.3 VM 抑制含义

VM 抑制（VM Inhibit）的含义是：进程被虚拟内存管理器（VM）阻止运行，直到 VM 完成必要的内存设置。

为什么需要 VM 抑制：

1. **页表缺失**：fork 后子进程继承了父进程的页表指针，但这个页表是父进程的

2. **内存隔离**：子进程需要独立的地址空间，需要新的页表

3. **VM 职责**：Minix3 中 VM 负责内存管理，内核不直接操作页表

4. **同步协调**：内核设置标志，VM 清除标志，两者协调完成初始化

VM 抑制流程：
```
内核 fork 完成
    ↓
设置 RTS_VMINHIBIT
    ↓
PM 通知 VM
    ↓
VM 创建新页表
    ↓
VM 调用内核设置页表
    ↓
内核清除 RTS_VMINHIBIT
    ↓
子进程可运行
```

#### 2.1.4 等待页表设置

子进程等待 VM 设置页表的原因：

1. **地址空间隔离**：子进程需要独立的虚拟地址空间，不能共享父进程的页表

2. **写时复制（COW）**：fork 后父子进程共享物理页面，但需要不同的页表项来标记 COW

3. **VM 是内存管理者**：Minix3 微内核架构中，VM 服务负责所有内存管理操作

4. **内核最小化**：内核不包含复杂的内存管理逻辑，只提供底层机制

等待过程：
1. 内核设置 `RTS_VMINHIBIT`，子进程不可运行
2. PM 调用 VM 的 fork 处理
3. VM 创建新的地址空间结构
4. VM 调用内核的 `sys_vm_set_context` 设置页表
5. 内核清除 `RTS_VMINHIBIT`
6. 子进程可以运行

这种设计确保内存管理逻辑集中在 VM，内核保持简洁。

### 2.2 信号标志清除

信号标志清除对应源码第 134 行：

```c
RTS_UNSET(rpc, (RTS_SIGNALED|RTS_SIG_PENDING|RTS_P_STOP));
```

这条语句清除子进程的三个信号相关标志，确保子进程不继承父进程的信号状态。

#### 2.2.1 RTS_SIGNALED 清除

`RTS_SIGNALED` 标志表示进程被信号中断。当进程在系统调用中被信号中断时，内核设置此标志。

清除 `RTS_SIGNALED` 的含义：
1. 子进程没有被信号中断
2. 子进程的系统调用状态是干净的
3. 子进程不会因为父进程的信号中断而受影响

POSIX 语义：fork 后子进程从干净状态开始，不应该继承父进程被信号中断的状态。

#### 2.2.2 RTS_SIG_PENDING 清除

`RTS_SIG_PENDING` 标志表示进程有待处理的信号。当信号发送给进程但尚未投递时，内核设置此标志。

清除 `RTS_SIG_PENDING` 的含义：
1. 子进程没有待处理的信号
2. 子进程不会收到父进程的信号
3. 子进程的信号队列是空的

这与后续的 `sigemptyset(&rpc->p_pending)` 配合，确保子进程的信号状态完全清空。

#### 2.2.3 RTS_P_STOP 清除

`RTS_P_STOP` 标志表示进程被停止（如收到 SIGSTOP）。被停止的进程不能运行，直到收到 SIGCONT。

清除 `RTS_P_STOP` 的含义：
1. 子进程不在停止状态
2. 子进程可以正常运行（在其他条件满足时）
3. 父进程被停止不影响子进程

POSIX 语义：fork 创建的子进程从运行状态开始，不继承父进程的停止状态。

#### 2.2.4 信号不继承

子进程不继承父进程的信号状态是 POSIX 标准的要求：

**不继承的信号状态：**
- 待处理信号集（`p_pending`）
- 被信号中断的状态（`RTS_SIGNALED`）
- 停止状态（`RTS_P_STOP`）
- 信号定时器（`setitimer`）

**继承的信号状态：**
- 信号处理函数（`signal`/`sigaction` 设置）
- 信号屏蔽字（`sigprocmask` 设置）
- 忽略的信号设置

设计原因：
1. **独立性**：子进程是独立进程，不应受父进程信号历史影响
2. **安全性**：防止信号注入攻击
3. **清晰性**：子进程从干净状态开始，行为可预测

Minix3 通过清除标志和清空信号集实现这些语义。

### 2.3 待处理信号清除

待处理信号清除对应源码第 135 行：

```c
sigemptyset(&rpc->p_pending);
```

这条语句清空子进程的待处理信号集，确保子进程没有继承任何待处理的信号。

#### 2.3.1 sigemptyset 调用

`sigemptyset(&rpc->p_pending)` 将子进程的待处理信号集初始化为空集。

`p_pending` 是 `sigset_t` 类型，存储待处理信号的位图。每位代表一个信号编号，如果位为 1，表示该信号待处理。

`sigemptyset` 的实现通常是：
```c
#define sigemptyset(set) memset((set), 0, sizeof(sigset_t))
```

清空后：
- 没有待处理的信号
- 子进程不会收到父进程的信号
- 信号投递逻辑从零开始

#### 2.3.2 信号集清空

子进程的信号集初始为空是 POSIX 标准的要求：

POSIX 对 fork 的信号语义规定：
> "The child process shall have no pending signals."

实现方式：
1. `sigemptyset(&rpc->p_pending)` 清空待处理信号集
2. 清除 `RTS_SIG_PENDING` 标志
3. 清除 `RTS_SIGNALED` 标志

效果：
- 子进程不会收到父进程的信号
- 子进程的信号队列是空的
- 子进程可以正常接收新信号

如果不清空，可能导致：
- 子进程收到不属于自己的信号
- 信号处理函数被意外调用
- 进程行为不可预测

### 2.4 页表指针清零

页表指针清零对应源码第 136-143 行（条件编译）：

```c
#if defined(__i386__)
  rpc->p_seg.p_cr3 = 0;
  rpc->p_seg.p_cr3_v = NULL;
#endif
#if defined(__arm__)
  rpc->p_seg.p_ttbr = 0;
  rpc->p_seg.p_ttbr_v = NULL;
#endif
```

这段代码根据架构清零页表指针，确保子进程不使用父进程的页表。

#### 2.4.1 i386 处理

i386 架构的页表指针清零对应源码第 137-138 行：

```c
rpc->p_seg.p_cr3 = 0;
rpc->p_seg.p_cr3_v = NULL;
```

这两条语句清零子进程的页目录物理地址和虚拟地址指针。

##### 2.4.1.1 p_cr3 清零

`rpc->p_seg.p_cr3 = 0` 将子进程的页目录物理地址清零。

`p_cr3` 存储 CR3 寄存器的值，即页目录的物理地址。在 i386 架构中，CR3 寄存器指向当前进程的页目录。

清零的含义：
1. 子进程没有有效的页表
2. 子进程不能使用父进程的页表
3. 等待 VM 设置新的页表

如果不清零，子进程会使用父进程的页表，导致两个进程共享地址空间，违反进程隔离原则。

##### 2.4.1.2 p_cr3_v 清零

`rpc->p_seg.p_cr3_v = NULL` 将子进程的页目录虚拟地址指针清零。

`p_cr3_v` 存储页目录在内核虚拟地址空间中的映射地址。内核通过这个地址访问和修改页目录内容。

清零的含义：
1. 子进程没有有效的页目录内核映射
2. 内核不能通过旧指针访问页目录
3. 等待 VM 设置新的映射

`p_cr3` 和 `p_cr3_v` 的区别：
- `p_cr3`：物理地址，用于加载 CR3 寄存器
- `p_cr3_v`：虚拟地址，用于内核访问页目录内容

#### 2.4.2 arm 处理

ARM 架构的页表指针清零对应源码第 140-141 行：

```c
rpc->p_seg.p_ttbr = 0;
rpc->p_seg.p_ttbr_v = NULL;
```

这两条语句清零子进程的转换表基址寄存器（TTBR）值和虚拟地址指针。

##### 2.4.2.1 p_ttbr 清零

`rpc->p_seg.p_ttbr = 0` 将子进程的转换表基址清零。

`p_ttbr` 存储 TTBR0 或 TTBR1 寄存器的值，即页表的物理基地址。在 ARM 架构中，TTBR 寄存器指向当前进程的页表。

清零的含义与 i386 的 `p_cr3 = 0` 相同：
1. 子进程没有有效的页表
2. 等待 VM 设置新页表
3. 确保地址空间隔离

##### 2.4.2.2 p_ttbr_v 清零

`rpc->p_seg.p_ttbr_v = NULL` 将子进程的页表虚拟地址指针清零。

`p_ttbr_v` 存储页表在内核虚拟地址空间中的映射地址，与 i386 的 `p_cr3_v` 功能相同。

清零的含义：
1. 子进程没有有效的页表内核映射
2. 等待 VM 设置新的映射

不同架构使用不同的字段名（`p_cr3` vs `p_ttbr`），但语义一致：存储页表基地址。

#### 2.4.3 页表清零原因

页表指针清零的原因：

1. **地址空间隔离**：每个进程需要独立的地址空间，不能共享页表

2. **COW 实现**：fork 后父子进程共享物理页面，但需要不同的页表项来标记写时复制

3. **VM 职责**：Minix3 中 VM 负责创建和管理页表，内核只存储指针

4. **安全性**：防止子进程访问父进程的私有内存

清零流程：
```
fork 整体复制
    ↓
子进程继承父进程的 p_cr3/p_ttbr
    ↓
内核清零页表指针
    ↓
设置 RTS_VMINHIBIT
    ↓
VM 创建新页表
    ↓
VM 设置 p_cr3/p_ttbr
    ↓
清除 RTS_VMINHIBIT
```

这是"先复制、后修正"策略的一部分：整体复制后，清零需要独立的字段。

### 2.5 返回 OK

返回 OK 对应源码第 145 行：

```c
return OK;
```

这条语句结束 `do_fork` 函数，返回成功状态码给调用者（kernel_call_finish）。

#### 2.5.1 return OK

`return OK` 表示 fork 系统调用成功完成。

`OK` 是 Minix3 的标准返回码，定义为 0：
```c
#define OK 0
```

返回值的处理流程：
1. `do_fork` 返回 `OK` 给 `kernel_call_dispatch`
2. `kernel_call_dispatch` 返回给 `kernel_call`
3. `kernel_call` 调用 `kernel_call_finish`
4. `kernel_call_finish` 将返回值写入调用者（PM）的消息
5. PM 收到成功返回，知道 fork 完成

如果 fork 失败，`do_fork` 会返回错误码如 `EINVAL`、`ENOMEM` 等，PM 根据错误码决定后续处理。

#### 2.5.2 fork 成功

fork 成功完成意味着：

1. **参数验证通过**：端点有效、槽位状态正确

2. **进程结构复制成功**：子进程继承了父进程的状态

3. **端点生成成功**：子进程获得唯一的新端点

4. **初始化完成**：返回值、时间统计、标志位正确设置

5. **状态正确**：子进程处于等待调度和 VM 设置的状态

PM 收到成功返回后：
1. 更新进程表
2. 通知 VM 处理 fork
3. 通知调度器分配时间片
4. 返回给用户进程

整个 fork 流程涉及 PM、内核、VM、调度器四个组件的协作，体现了微内核架构的模块化设计。

---

## 3. 标志处理流程

标志处理的完整流程总结 fork 过程中所有标志位的设置和清除操作。

### 3.1 设置的标志

fork 时设置的标志：

| 标志 | 设置位置 | 含义 |
|------|----------|------|
| `RTS_NO_QUANTUM` | 18-do-fork-init | 无时间片，等待调度器 |
| `RTS_NO_PRIV` | 18-do-fork-init | 无特权，等待 PM（仅系统进程子进程）|
| `RTS_VMINHIBIT` | 19-do-fork-priv | VM 抑制，等待页表设置 |

这些标志都是"等待条件"标志，子进程需要等待相应服务完成初始化后才能运行。

### 3.2 清除的标志

fork 时清除的标志：

| 标志 | 清除位置 | 含义 |
|------|----------|------|
| `RTS_SIGNALED` | 19-do-fork-priv | 清除信号中断状态 |
| `RTS_SIG_PENDING` | 19-do-fork-priv | 清除待处理信号标志 |
| `RTS_P_STOP` | 19-do-fork-priv | 清除停止状态 |

此外，17-do-fork-endpoint 中清除了：

| 标志 | 含义 |
|------|------|
| `MF_VIRT_TIMER` | 虚拟定时器 |
| `MF_PROF_TIMER` | 性能分析定时器 |
| `MF_SC_TRACE` | 系统调用跟踪 |
| `MF_SPROF_SEEN` | 系统性能分析 |
| `MF_STEP` | 单步执行 |

这些清除操作确保子进程不继承父进程的信号和调试状态。

### 3.3 标志处理顺序

标志处理的顺序：

```
fork 标志处理流程
│
├── 1. 整体复制（16-do-fork-copy）
│       └── 子进程继承父进程的所有标志
│
├── 2. 返回值设置（17-do-fork-endpoint）
│       └── 清除 MF_* 定时器和调试标志
│
├── 3. 初始化设置（18-do-fork-init）
│       ├── 设置 RTS_NO_QUANTUM
│       └── 设置 RTS_NO_PRIV（条件）
│
└── 4. 特权与标志处理（19-do-fork-priv）
        ├── 设置 RTS_VMINHIBIT（条件）
        └── 清除 RTS_SIGNALED, RTS_SIG_PENDING, RTS_P_STOP
```

处理顺序的设计原则：
1. **先继承后修正**：整体复制后逐一修正
2. **设置先于清除**：先设置阻止运行的标志，再清除继承的标志
3. **条件处理在后**：条件标志（如 VMINHIBIT）在最后处理

这种顺序确保子进程在任何时刻都处于一致的状态。

---

## 4. Rust 设计决策

标志处理的 Rust 实现需要考虑类型安全和架构抽象：

1. **类型安全**：使用 newtype 模式封装标志位，避免位操作错误
2. **架构抽象**：通过 trait 抽象页表指针，支持多架构
3. **条件编译**：使用 cfg 属性替代 C 的 #if

### 4.1 标志类型安全

标志类型安全的设计：

```rust
/// 运行时状态标志
#[repr(transparent)]
pub struct RtsFlags(AtomicU32);

/// RTS 标志位定义
pub mod rts {
    pub const NO_QUANTUM: u32 = 0x01;
    pub const NO_PRIV: u32 = 0x02;
    pub const VMINHIBIT: u32 = 0x04;
    pub const SIGNALED: u32 = 0x08;
    pub const SIG_PENDING: u32 = 0x10;
    pub const P_STOP: u32 = 0x20;
}

impl RtsFlags {
    pub fn new(value: u32) -> Self { Self(AtomicU32::new(value)) }
    pub fn load(&self) -> u32 { self.0.load(Ordering::Acquire) }
    pub fn set(&self, flags: u32) { self.0.fetch_or(flags, Ordering::AcqRel); }
    pub fn clear(&self, flags: u32) { self.0.fetch_and(!flags, Ordering::AcqRel); }
    pub fn is_set(&self, flags: u32) -> bool { self.load() & flags != 0 }
    pub fn is_runnable(&self) -> bool { self.load() == 0 }
}
```

这种设计提供：
- 类型安全的标志操作
- 原子操作保证线程安全
- 清晰的 API 避免位操作错误

### 4.2 架构抽象

页表指针的架构抽象：

```rust
/// 页表指针 trait
///
/// 不同架构实现此 trait，提供页表指针的访问和设置方法。
pub trait PageTablePtr {
    /// 页表物理地址类型
    type PhysAddr;
    
    /// 获取页表物理地址
    fn phys_addr(&self) -> Self::PhysAddr;
    
    /// 设置页表物理地址
    fn set_phys_addr(&mut self, addr: Self::PhysAddr);
    
    /// 清零页表指针
    fn clear(&mut self);
}

/// i386 架构的页表指针
#[cfg(target_arch = "x86")]
pub struct I386PageTable {
    p_cr3: u32,      // 物理地址
    p_cr3_v: *const u8,  // 虚拟地址
}

/// ARM 架构的页表指针
#[cfg(target_arch = "arm")]
pub struct ArmPageTable {
    p_ttbr: u32,
    p_ttbr_v: *const u8,
}
```

Mock 实现：

```rust
pub struct MockPageTable {
    phys_addr: u64,
    virt_addr: *const u8,
}

impl PageTablePtr for MockPageTable {
    type PhysAddr = u64;
    fn phys_addr(&self) -> u64 { self.phys_addr }
    fn set_phys_addr(&mut self, addr: u64) { self.phys_addr = addr; }
    fn clear(&mut self) { self.phys_addr = 0; self.virt_addr = std::ptr::null(); }
}
```

### 4.3 条件编译

条件编译的处理方式：

**C 语言方式（不推荐）：**
```c
#if defined(__i386__)
  rpc->p_seg.p_cr3 = 0;
#endif
#if defined(__arm__)
  rpc->p_seg.p_ttbr = 0;
#endif
```

**Rust 方式（推荐）：**

1. **使用 cfg 属性**：
```rust
#[cfg(target_arch = "x86")]
fn clear_page_table(proc: &mut KProcess) {
    proc.p_seg.p_cr3 = 0;
    proc.p_seg.p_cr3_v = std::ptr::null();
}

#[cfg(target_arch = "arm")]
fn clear_page_table(proc: &mut KProcess) {
    proc.p_seg.p_ttbr = 0;
    proc.p_seg.p_ttbr_v = std::ptr::null();
}
```

2. **使用 trait 抽象**：
```rust
proc.p_seg.page_table.clear();  // 架构无关的调用
```

3. **使用泛型**：
```rust
struct KProcess<Arch: Architecture> {
    p_seg: Arch::PageTable,
    // ...
}
```

Rust 的方式更安全、更易维护，编译器会检查所有分支的完整性。

---

## 5. 实现

本节给出标志处理和页表清零的 Rust 实现，包括方法定义和单元测试。

### 5.1 标志处理方法

标志处理方法的 Rust 实现：

```rust
impl KProcess {
    /// 处理 fork 时的标志设置
    ///
    /// 对应 Minix3 do_fork.c 第 131-136 行。
    pub fn fork_handle_flags(&mut self, vm_inhibit: bool) {
        // 设置 VM 抑制标志
        if vm_inhibit {
            self.p_rts_flags.set(rts::VMINHIBIT);
        }
        
        // 清除信号相关标志
        self.p_rts_flags.clear(
            rts::SIGNALED | rts::SIG_PENDING | rts::P_STOP
        );
        
        // 清空待处理信号集
        self.p_pending = SigSet::empty();
        
        // 清零页表指针（架构相关）
        self.p_seg.page_table.clear();
    }
}
```

这个方法封装了 fork 最后阶段的标志处理逻辑，在 `fork_from` 之后调用。

### 5.2 页表清零方法

页表清零方法的 Rust 实现（使用 trait 抽象）：

```rust
/// 页表管理 trait
pub trait PageTable: Sized {
    /// 清零页表指针
    fn clear(&mut self);
    
    /// 检查页表是否有效
    fn is_valid(&self) -> bool;
}

/// Mock 架构的页表
pub struct MockPageTable {
    phys_addr: u64,
    virt_addr: *const u8,
}

impl PageTable for MockPageTable {
    fn clear(&mut self) {
        self.phys_addr = 0;
        self.virt_addr = std::ptr::null();
    }
    
    fn is_valid(&self) -> bool {
        self.phys_addr != 0
    }
}

/// 进程内存段信息
pub struct ProcSeg<PT: PageTable> {
    pub page_table: PT,
    pub fpu_state: *mut u8,
}

impl<PT: PageTable> ProcSeg<PT> {
    pub fn new() -> Self {
        Self {
            page_table: PT { phys_addr: 0, virt_addr: std::ptr::null() },
            fpu_state: std::ptr::null_mut(),
        }
    }
}
```

### 5.3 单元测试

标志处理的单元测试：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fork_handle_flags_vm_inhibit() {
        let mut proc = KProcess::new(10, Endpoint::from_generation_slot(1, 10));
        proc.p_rts_flags.set(rts::SIGNALED);
        proc.p_rts_flags.set(rts::SIG_PENDING);
        
        proc.fork_handle_flags(true);
        
        assert!(proc.p_rts_flags.is_set(rts::VMINHIBIT));
        assert!(!proc.p_rts_flags.is_set(rts::SIGNALED));
        assert!(!proc.p_rts_flags.is_set(rts::SIG_PENDING));
    }

    #[test]
    fn test_fork_handle_flags_no_vm_inhibit() {
        let mut proc = KProcess::new(10, Endpoint::from_generation_slot(1, 10));
        
        proc.fork_handle_flags(false);
        
        assert!(!proc.p_rts_flags.is_set(rts::VMINHIBIT));
    }

    #[test]
    fn test_fork_handle_flags_clears_signals() {
        let mut proc = KProcess::new(10, Endpoint::from_generation_slot(1, 10));
        proc.p_rts_flags.set(rts::SIGNALED);
        proc.p_rts_flags.set(rts::SIG_PENDING);
        proc.p_rts_flags.set(rts::P_STOP);
        
        proc.fork_handle_flags(false);
        
        assert!(!proc.p_rts_flags.is_set(rts::SIGNALED));
        assert!(!proc.p_rts_flags.is_set(rts::SIG_PENDING));
        assert!(!proc.p_rts_flags.is_set(rts::P_STOP));
    }

    #[test]
    fn test_fork_handle_flags_clears_pending_signals() {
        let mut proc = KProcess::new(10, Endpoint::from_generation_slot(1, 10));
        proc.p_pending.add(signal::SIGTERM);
        
        proc.fork_handle_flags(false);
        
        assert!(proc.p_pending.is_empty());
    }

    #[test]
    fn test_page_table_clear() {
        let mut pt = MockPageTable {
            phys_addr: 0x1000,
            virt_addr: 0x2000 as *const u8,
        };
        
        pt.clear();
        
        assert!(!pt.is_valid());
    }
}
```

这些测试验证：
- VM 抑制标志正确设置
- 信号相关标志正确清除
- 待处理信号集正确清空
- 页表指针正确清零

---

## 6. 参见

- [18-do-fork-init](18-do-fork-init.md) - 子进程初始化
- [06-proc-rts-flags](06-proc-rts-flags.md) - RTS 标志位
- [07-proc-misc-flags](07-proc-misc-flags.md) - MF 标志位
