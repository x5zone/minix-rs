# 02-vmproc-table: VM 进程表管理

> **分类**: VM私有  
> **源码**: `minix3/minix/servers/vm/glo.h`, `utility.c`, `vmproc.h`  
> **说明**: fork 纵向切片 - VM 进程表设计与实现

---

## 目录

1. [概述](#1-概述)
   - [1.1 进程表的作用](#11-进程表的作用)

2. [设计目标与约束](#2-设计目标与约束)
   - [2.1 地址稳定性要求](#21-地址稳定性要求)
   - [2.2 显式生命周期控制](#22-显式生命周期控制)
   - [2.3 与 Minix3 语义对齐](#23-与-minix3-语义对齐)

3. [Minix3 C 源码分析](#3-minix3-c-源码分析)
   - [3.1 进程表定义](#31-进程表定义)
   - [3.2 进程查找与验证](#32-进程查找与验证)

4. [技术选型分析](#4-技术选型分析)
   - [4.1 数组容器选型](#41-数组容器选型)
   - [4.2 存储选型](#42-存储选型)
   - [4.3 数组元素类型选型](#43-数组元素类型选型)

5. [Rust 实现设计](#5-rust-实现设计)
   - [5.1 数据结构定义](#51-数据结构定义)
   - [5.2 全局访问](#52-全局访问)
   - [5.3 Typestate Views](#53-typestate-views)
   - [5.4 可见性设计](#54-可见性设计)

6. [测试与验证](#6-测试与验证)
   - [6.1 测试基础设施](#61-测试基础设施)
   - [6.2 Typestate 生命周期](#62-typestate-生命周期)
   - [6.3 Typestate 访问互斥](#63-typestate-访问互斥)
   - [6.4 activate() vs activate_relaxed()](#64-activate-vs-activate_relaxed)
   - [6.5 force_clear() 异常回滚](#65-force_clear-异常回滚)
   - [6.6 vm_isokendpt() 验证](#66-vm_isokendpt-验证)
   - [6.7 alloc_empty_slot() 分配](#67-alloc_empty_slot-分配)
   - [6.8 VmProcIter 遍历](#68-vmprociter-遍历)
   - [6.9 测试维度总结](#69-测试维度总结)

7. [附录](#7-附录)
   - [附录A：vmproc 不受硬件地址稳定性限制的完整论证](#附录a-vmproc-不受硬件地址稳定性限制的完整论证)

---

## 1. 概述

### 1.1 进程表的作用

VM 进程表（`VmProcTable`）是虚拟内存管理器的核心数据结构，负责管理所有用户进程的虚拟内存状态。

**核心职责**：
- **进程生命周期管理**：跟踪所有进程的创建、运行、退出状态
- **O(1) 快速访问**：通过 slot 号直接索引，无需遍历
- **地址稳定性保证**：进程结构体内存地址固定，支持安全引用
- **TOCTOU 防护**：配合 `vm_isokendpt()` 验证 endpoint 有效性

**与其他服务的关系**：
```
PM (fork/exit) ──► VM (vmproc[] 管理)
                     │
                     ├──► 内存区域、页表、统计信息 (VM 私有)
                     │
                     ▼
              Kernel (sys_fork/sys_exit)
                     │
                     └──► 进程表、地址空间切换
```

---

## 2. 设计目标与约束

### 2.1 地址稳定性要求

#### 2.1.1 硬件层面的强制要求

**关键问题**：VM Server 中哪些结构被硬件强制要求地址稳定？

**结论：`vmproc` 本身不受硬件地址稳定性限制**

| 结构 | 硬件绑定 | 地址稳定性来源 |
|------|----------|----------------|
| **页表物理内存** | CR3/TTBR0/SATP 寄存器 | ⚠️ **硬件强制** |
| **`vmproc` 结构体** | 无直接绑定 | ✅ **软件设计选择**，非硬件强制 |

**核心原理：值传递解耦（Value-Passing Decoupling）**

VM 通过 `sys_vmctl` 系统调用将页目录**物理地址值**（`pt_dir_phys`，u32_t）传递给内核，而非 `vmproc` 指针：

1. **VM 侧**：`vmproc.vm_pt.pt_dir_phys = 0x12340000`（u32_t 值）
2. **传递**：`sys_vmctl(0x12340000)`（值复制）
3. **内核侧**：`CR3 = 0x12340000`（硬件寄存器存储值）
4. **硬件侧**：直接访问物理地址 `0x12340000` 处的页表

**说明**：
- 硬件绑定的是**物理地址值**（已传递出去），不是**容器地址**（`vmproc` 的地址）
- 即使 `vmproc` 移动，只要 `pt_dir_phys` 值不变，硬件不受影响

> **详细论证见[附录A](#附录a-vmproc-不受硬件地址稳定性限制的完整论证)**

#### 2.1.2 软件层面的需求

虽然硬件不强制要求，但 Minix3 的软件设计**确实需要** `vmproc` 地址稳定：

- **引用传递模式**：大量函数接收 `struct vmproc *` 指针
  ```c
  void free_proc(struct vmproc *vmp);
  void clear_proc(struct vmproc *vmp);
  ```

- **长期指针保存**：`vm_exec_info` 等结构体保存 `struct vmproc *vmp`

- **Slot → 地址映射**：`&vmproc[slot]` 是常用模式
  ```c
  struct vmproc *vmp = &vmproc[slot];
  ```

#### 2.1.3 工程决策

**为什么选静态数组？**

| 方案 | 可行性 | 实现成本 | 风险 | 结论 |
|------|--------|----------|------|------|
| **静态数组 `[T; N]`** | ✅ | ⭐ 最低 | 无 | **当前选择** |
| **`Vec<T>`** | ⚠️ | ⭐⭐⭐⭐ 极高 | 需消灭所有 `&VmProc` 引用；扩容时并发风险 | 不推荐 |

**重构成本分析**：
1. **引用消灭成本**：需全面消灭所有 `&VmProc` 长期引用，改为 slot 号间接访问
2. **代码规模**：此类引用遍布 VM 各模块，重构工作量巨大且易遗漏
3. **内存模型风险**：`Vec` 扩容会导致底层内存重分配，从而使已有的 `&VmProc` 引用失效。这不是并发问题，而是 reallocation invalidates references

> **注意**：Borrow checker 管 alias，但不管 allocation stability。即使单线程，只要持有引用期间发生 realloc，就是 UB。

**结论**：保持静态数组是最简单、风险最低且与 Minix3 原设计一致的工程选择

### 2.2 显式生命周期控制

内核资源管理需要**精确控制释放时机**：

- **作用域驱动 vs 事件驱动**：Drop 由作用域触发，内核资源由系统事件触发
- **状态重置 vs 对象销毁**：内核需要 Reset（原地状态归零），不是 Destroy（析构重建）
- **避免隐式析构**：`*slot = new_val` 的隐式析构会破坏确定性时序

Minix3 的处理方式：
- `memset(vmproc, 0, sizeof(vmproc))` 将所有槽位初始化为零
- 状态由 `vm_flags` 字段管理（`VMF_INUSE` 标记占用）
- `clear_proc()` 显式重置槽位状态

> Rust 设计差异详见 [§5.1](#51-数据结构定义) 和 [01-vmproc-struct.md](01-vmproc-struct.md) 的 Drop 设计。

### 2.3 与 Minix3 语义对齐

保持与 Minix3 C 代码的语义一致性：

| Minix3 (C) | 语义 |
|------------|------|
| `vmproc[slot]` 静态数组 | O(1) slot 索引访问 |
| `vm_flags & VMF_INUSE` | 槽位占用状态判断 |
| `clear_proc(vmp)` | 显式状态重置 |
| `memset(vmproc, 0, ...)` | 全零初始化 |

> Rust 实现映射详见 [§5.1](#51-数据结构定义)。

---

## 3. Minix3 C 源码分析

### 3.1 进程表定义

**进程表定义** ([glo.h:17-20](minix3/minix/servers/vm/glo.h#L17-L20)):

```c
#define VMP_EXECTMP	_NR_PROCS
#define VMP_NR		_NR_PROCS+1

EXTERN struct vmproc vmproc[VMP_NR];        /* 进程表数组 */
```

**关键定义**：
- `VMP_EXECTMP`: exec 系统调用使用的临时槽位（slot = NR_PROCS）
- `VMP_NR`: 进程表总大小（NR_PROCS + 1），包含 exec 临时槽位
- `vmproc[VMP_NR]`: 全局进程表数组

### 3.2 进程查找与验证

#### 3.2.1 vm_isokendpt - Endpoint 验证

**函数签名**:
```c
int vm_isokendpt(endpoint_t endpoint, int *procn);
```

**文件**: [utility.c:84-94](minix3/minix/servers/vm/utility.c#L84-L94)

**功能**: 验证 endpoint 是否有效，并返回对应的进程槽位号。

**实现逻辑**:
```c
int vm_isokendpt(endpoint_t endpoint, int *procn)
{
        *procn = _ENDPOINT_P(endpoint);
        if(*procn < 0 || *procn >= NR_PROCS)
                return EINVAL;
        if(*procn >= 0 && endpoint != vmproc[*procn].vm_endpoint)
                return EDEADEPT;
        if(*procn >= 0 && !(vmproc[*procn].vm_flags & VMF_INUSE))
                return EDEADEPT;
        return OK;
}
```

**验证步骤说明**:

| 步骤 | 检查内容 | 失败返回 | 目的 |
|------|----------|----------|------|
| 1 | 进程号范围 | `EINVAL` | 防止数组越界访问 |
| 2 | endpoint 匹配 | `EDEADEPT` | 防止使用已失效的 endpoint（slot 重用） |
| 3 | 进程活跃状态 | `EDEADEPT` | 防止操作已退出的进程 |

**使用场景**:

```c
// fork.c:41-46 - VM_FORK 处理
if(vm_isokendpt(msg->VMF_ENDPOINT, &proc) != OK) {
    printf("VM: bogus endpoint VM_FORK %d\n", msg->VMF_ENDPOINT);
    return EINVAL;  // 父进程不存在或已退出
}
// 现在可以安全使用 vmproc[proc]
```

> **Rust 实现差异**：Rust 版本区分 `InvalidSlot`（对应 C 的 `EINVAL`）和 `DeadEndpoint`（对应 C 的 `EDEADEPT`），返回 `Result<UserSlot, EndpointError>` 而非 C 的 `int` 错误码。详见 [§5.3](#53-typestate-views)。

**设计意义**:

`vm_isokendpt` 的核心价值是防止 **TOCTOU（Time-of-Check to Time-of-Use）** 问题。

PM 和 VM 是两个独立的地址空间。如果父进程在 PM 发出 `VM_FORK` 的瞬间崩溃，其 slot 可能被迅速重用并分配给新进程。没有 `vm_isokendpt` 检查，VM 会拿着旧的 endpoint 错误操作新进程的内存。

验证的三层递进设计：

| 层次 | 检查内容 | 失败返回 | 防护目标 |
|------|----------|----------|----------|
| 1 | 进程号范围 | `EINVAL` | 防止数组越界访问（基础安全） |
| 2 | endpoint 匹配 | `EDEADEPT` | 防止 slot 重用导致的 endpoint 混淆（核心防护） |
| 3 | 活跃状态检查 | `EDEADEPT` | 防止操作已退出进程（状态一致性） |

> **注意**：`vm_isokendpt` 验证的是 endpoint **有效性**（是否存在、是否活跃），不是**权限**。Minix3 的权限验证通过消息的来源 endpoint（`msg->m_source`）隐式完成，因为 IPC 机制确保消息来源无法伪造。

> **分布式一致性背景**: 详见 [00-vm-overview.md](00-vm-overview.md) 的分布式一致性分析。

---

## 4. 技术选型分析

C 使用 `EXTERN struct vmproc vmproc[VMP_NR]` 定义全局进程表。在 Rust 中实现这个全局数组，需要解决三个问题：
1. **元素们如何组织**：选择什么数据结构（静态数组 vs Vec vs 侵入式链表）
2. **元素如何表达**：槽位状态如何表示（MaybeUninit vs Option vs ManuallyDrop）
3. **如何存储**：全局变量的存储方式（static vs static mut，以及 UnsafeCell 的使用）

### 4.1 数组容器选型

#### 4.1.1 核心需求

VM 进程表需要支持：
- **O(1) 快速访问**：通过 slot 号直接索引
- **地址稳定性**：支持裸指针传递和长期引用（软件设计需求，非硬件强制）

#### 4.1.2 候选方案对比

| 方案 | 地址稳定 | O(1)访问 | 扩展性 | 适用性 |
|------|----------|----------|--------|--------|
| **静态数组 `[T; N]`** | ✅ | ✅ | ❌ 固定上限 | ✅ **当前选择** |
| **`Vec<T>`** | ❌ reallocate | ✅ | ✅ | ❌ 扩容导致地址变化 |
| **侵入式链表** | ✅ | ❌ O(n) | ✅ | ⚠️ 查找效率低 |
| **对象池 + 索引结构** | ✅ | ✅ | ✅ | 未来可扩展 |
| **VM-backed 连续区域** | ✅ | ✅ | ✅ | 高级方案 |

#### 4.1.3 方案详细分析

**`Vec<T>` - 不适合**

```rust
let mut table: Vec<VmProc> = Vec::with_capacity(NR_PROCS);
// push 可能导致重分配，地址变化！
```

**问题**：
- 扩容时重新分配内存，所有 `&VmProc` 引用失效
- 即使 `with_capacity` 预分配，也无法保证不扩容
- 重构成本极高：需消灭所有 `&VmProc` 长期引用

**静态数组 `[T; N]` - Rewrite 阶段最佳选择**

**优势**：
- 地址绝对稳定：BSS 段，运行时永不移动
- 零运行时开销
- 与 Minix3 C 代码语义一致

**选择原因**：
1. 最简单、最容易实现、最容易保证正确性
2. 满足所有地址稳定性需求
3. 与 Minix3 原设计一致

---

#### 4.1.4 未来演进：可扩展设计

当进程数量从百级扩展到万级以上时，固定大小数组不再适用。核心问题不是"性能"，而是"将系统规模上限编码进了编译期常量"。

**三种方案对比**：

| 维度 | 固定数组（当前） | 对象池 + 索引 | VM-backed 连续区域 |
|------|----------|---------------|-----------|
| 地址稳定来源 | 编译期 BSS | 分配器保证 | 虚拟地址空间 |
| 是否连续 | 连续 | ❌ 不连续 | 连续（虚拟） |
| 扩展方式 | 不支持 | 分散增长 | 线性增长 |
| Cache locality | ✅ 最优 | ⚠️ 较差 | ✅ 最优 |
| 实现复杂度 | ⭐ 最低 | ⭐⭐ 中 | ⭐⭐⭐⭐ 高 |

**结论**：当前采用固定数组（简单可靠，符合 Minix3 风格）。若扩展至大规模进程，优先考虑对象池 + 索引结构（Linux 采用 slab + idr）。VM-backed 作为更高级但复杂的替代方案（存在递归悖论：VM 管理内存时若需访问进程表，可能死锁）。

---
### 4.2 存储选型

C 使用 `EXTERN` 定义全局变量，在 BSS 段分配。Rust 中需要选择如何存储这个全局进程表。

#### 4.2.1 存储位置选择

进程表需要是**全局可访问、生命周期贯穿 VM Server 全程的核心数据结构**。

可选存储位置包括：

| 位置 | 特点 | 适用性 |
|------|------|--------|
| **栈** | 随调用帧分配，容量受限，语义为临时数据 | ❌ 不适合 |
| **堆** | 依赖动态分配器，运行时管理 | ❌ 不适合 |
| **BSS 段** | 静态分配，全局可访问，生命周期为整个进程 | ✅ 最合适 |

**栈：语义与容量均不匹配**

理论上，可以在顶层函数栈上分配进程表并通过引用传递，实现"全局访问"。但这种方式存在根本问题：

- 栈空间有限，无法承载中大型进程表
- 栈语义用于"临时数据"，不适合存储系统级长期状态
- 可维护性差，不符合系统编程约定

因此栈不适合作为进程表存储位置。

**堆：违反系统分层**

VM 作为虚拟内存管理器，本身处于内存管理体系的核心位置。若将进程表分配在堆上（如 `Vec<T>`）：

```rust
let table = Vec<VmProc>::new();
```

则引入以下问题：

- VM 依赖堆分配器
- 堆分配器又依赖 VM 提供内存管理

形成**循环依赖（bootstrapping problem）**，增加系统复杂性与初始化难度。

**BSS 段：最符合系统语义的选择**

静态分配在 BSS 段具有以下优势：

- 生命周期与 VM Server 一致（进程级）
- 无运行时分配开销
- 地址稳定（不发生移动）
- 与 Minix3 原实现一致：

```c
EXTERN struct vmproc vmproc[VMP_NR];
```

**结论**：

进程表属于**全局、长期存在、核心系统状态**，应采用静态分配。BSS 段在语义、实现复杂度和系统分层上均为最优选择。

#### 4.2.2 三种方案对比

确定存储在 BSS 段后，需要选择具体的 Rust 表达方式。三种方案都是 `static` 存储，区别在于如何表达可变性：

| 方案 | 结构定义 | alias 粒度 | 特点 |
|-----|---------|-----------|------|
| `static mut` | `static mut slots: [T; N]` | 无控制 | 完全关闭 borrow checker，最接近 C 语义 |
| `static` + `UnsafeCell<[T; N]>` | `static slots: UnsafeCell<[T; N]>` | 数组级 | 显式内部可变性，但粒度仍为整个数组 |
| `static` + `[UnsafeCell<T>; N]` | `static slots: [UnsafeCell<T>; N]` | slot 级 | 细粒度控制，支持同时访问不同槽位 |

#### 4.2.3 方案分析

**方案 1：`static mut`**

```rust
pub static mut VM_PROC_TABLE: VmProcTable = VmProcTable {
    slots: [VmProc::vacant(); NR_PROCS],
};
```

- **缺点**：Rust 2024 逐步淘汰 `static mut`，使用会触发废弃警告
- **优点**：语义最直接，与 C 的 `EXTERN` 一一对应
- **结论**：❌ 不选择，避免未来兼容性问题

**方案 2：`static` + `UnsafeCell<[T; N]>`**

```rust
pub static VM_PROC_TABLE: UnsafeCell<VmProcTable> = UnsafeCell::new(VmProcTable {
    slots: [VmProc::vacant(); NR_PROCS],
});
```

- **优点**：显式表达内部可变性
- **缺点**：每次访问 slot 都需要先获取整个 table 的 `&mut`，在调用链中容易产生 aliasing UB
  ```rust
  fn get_slot(i: usize) -> &mut VmProc {
      let table = unsafe { &mut *VM_PROC_TABLE.get() };  // &mut 整个 table
      &mut table.slots[i]
  }
  
  fn fork() {
      let parent = get_slot(0);  // 持有 &mut table（通过 slot 0）
      let child = get_slot(1);   // 又获取 &mut table → ALIASING UB！
  }
  ```

**方案 3：`static` + `[UnsafeCell<T>; N]` — 实际选择**

```rust
pub static VM_PROC_TABLE: VmProcTable = VmProcTable {
    slots: [AssumeSyncCell::new(VmProc::vacant()); VM_PROC_COUNT],
};
```

`AssumeSyncCell<T>` = `UnsafeCell<T>` + `unsafe impl Sync`。

Rust 要求 `static` 变量必须实现 `Sync`，但 `UnsafeCell` 不满足（内部可变性在多线程下不安全）。VM server 是单线程的，因此通过 `unsafe impl Sync` 绕过此限制。

`AssumeSyncCell` 定义在 `minix-types` 中：

```rust
#[repr(transparent)]
pub struct AssumeSyncCell<T>(core::cell::UnsafeCell<T>);

// SAFETY: 仅在单线程上下文中使用，调用者保证每个 cell 的独占访问
unsafe impl<T> Sync for AssumeSyncCell<T> {}

impl<T> AssumeSyncCell<T> {
    pub const fn new(value: T) -> Self { ... }
    pub unsafe fn get(&self) -> *mut T { self.0.get() }  // 返回原始指针
    pub fn as_ptr(&self) -> *const T { self.0.get() }    // 只读指针
}
```

**关键设计**：
- `#[repr(transparent)]`：零开销包装，内存布局与 `UnsafeCell<T>` 完全一致
- `get()` 返回 `*mut T`：不直接返回引用，将安全性检查推迟到调用方
- `const fn new()`：允许在 `static` 初始化器中使用
- `unsafe impl Sync` 的安全契约：单线程 + slot 级独占访问（通过 typestate view 保证）

- **优点**：
  - slot 级粒度，支持同时访问不同槽位（如 fork 时同时操作父子进程）
  - 避免 `static mut` 的 Rust 2024 废弃警告
  - API 更清晰：`get_global()` 返回 `&'static VmProcTable`
- **缺点**：实现稍复杂，需要为每个 slot 单独管理
- **适用**：当前实现

#### 4.2.4 最终决策

**采用 `static` + `[AssumeSyncCell<T>; N]` 方案**

```rust
static VM_PROC_TABLE: VmProcTable = VmProcTable {
    slots: [const { AssumeSyncCell::new(VmProc::vacant()) }; VM_PROC_COUNT],
};

impl VmProcTable {
    pub fn get_global() -> &'static VmProcTable {
        &VM_PROC_TABLE
    }
}
```

**选择理由**：

| 考量 | 结论 |
|------|------|
| Rust 2024 兼容 | ✅ 避免 `static mut` 废弃警告 |
| slot 级粒度 | ✅ 支持 fork 等场景同时访问多个槽位 |
| API 清晰度 | ✅ `get_global()` 返回不可变引用，可变性封装在内部 |
| 与 Minix3 一致 | ✅ 静态分配在 BSS 段 |

---

### 4.3 数组元素类型选型

确定使用静态数组 `[T; N]` 后，需要选择具体的元素类型 `T`。

#### 4.3.1 核心需求

Minix3 使用 `memset(vmproc, 0, ...)` 将所有槽位初始化为零，然后按需使用。这启发了"全初始化 + 状态字段"的设计思路：

- 槽位**始终存在**，状态由 `vm_flags` 字段表达
- 不需要"存在性"表达（不需要区分"已初始化"和"未初始化"）
- 需要**原地修改**（不 move out）

#### 4.3.2 候选方案对比

| 方案 | 初始化方式 | 状态表达 | 复杂度 | 适用场景 |
|------|-----------|----------|--------|----------|
| **`[T; N] + No-op Drop`** | 预初始化 | `VmFlags` | 低 | **实际选择**（与 Minix3 一致） |
| **`[Option<T>; N]`** | 延迟初始化 | `Some/None` | 中 | 值可能不存在（如 `Vec::pop`） |
| **`[MaybeUninit<T>; N]`** | 延迟初始化 | 需额外跟踪 | 高 | 需要严格区分"已初始化/未初始化" |
| **`[ManuallyDrop<T>; N]`** | 预初始化 | `VmFlags` | 中 | 需要禁止自动 Drop |

#### 4.3.3 方案分析

**`[T; N] + No-op Drop` — 实际选择**

```rust
pub struct VmProcTable {
    slots: [AssumeSyncCell<VmProc>; VM_PROC_COUNT],
}

static VM_PROC_TABLE: VmProcTable = VmProcTable {
    slots: [const { AssumeSyncCell::new(VmProc::vacant()) }; VM_PROC_COUNT],
};
```

> **注意**：`AssumeSyncCell` 是因为 `static` 数组需要满足 `Sync` trait 而引入的包装（参见 4.2.4 节）。元素类型本质上仍是 `VmProc`，即 `[T; N]`。

**选择理由**：

1. **与 Minix3 语义一致**：`memset(vmproc, 0, ...)` 对应 `VmProc::vacant()`
2. **状态由 `VmFlags` 表达**：`IN_USE` / `EXITING` / 空，无需额外包装
3. **避免层层叠叠的包装**：`AssumeSyncCell<VmProc>` 比 `AssumeSyncCell<MaybeUninit<VmProc>>` 简洁
4. **No-op Drop**：`VmProc` 的 Drop 只是 `debug_assert`，不释放资源

---

**`[Option<T>; N]` — 语义错位**

```rust
// Option 允许 move-out，这种"允许"意味着开发者可能写出"取出-修改-放回"的代码，与内核"原地修改"模型不匹配。
let mut proc = vmproc_table[slot].take().unwrap();  // 合法！
proc.flags |= VMF_EXITING;
vmproc_table[slot] = Some(proc);
```

**问题**：
- 语义错位：表达"存在/不存在"，但内核槽位的语义是"状态机"——槽位**始终存在**，仅内部状态变化（Empty → Active → Exiting）
- 内存开销：`Option<T>` 需要额外 tag（若无法利用 niche optimization）

**对比：原地修改**

```rust
// 实际代码：通过指针原地修改
let slot = &table.slots[i];
let proc = unsafe { &mut *slot.get() };
proc.flags |= VMF_EXITING;  // 原地修改，无 move-out
```

---

**`[MaybeUninit<T>; N]` — 过度设计（对本项目）**

`MaybeUninit<T>` 将"存在性"纳入类型系统，区分"已初始化/未初始化"。

```rust
// 初始化：显式写入
slots[slot].write(new_proc);

// 使用：unsafe 确认已初始化
let proc = unsafe { slots[slot].assume_init_mut() };
```

**为何本项目不需要**：

Minix3 中槽位不存在"未初始化"状态——`memset` 后就是 `vacant()`。`MaybeUninit` 的额外复杂度没有带来收益：

| 维度 | `MaybeUninit<T>` | `T + vacant()` |
|------|------------------|----------------|
| 存在性表达 | ✅ 类型系统内置 | ✅ `VmFlags` 表达 |
| 初始化成本 | 零成本（uninit） | 零成本（vacant） |
| 使用门槛 | 每次访问需 `unsafe` | 直接访问 |
| 代码复杂度 | 高（层层包装） | 低 |

**适用场景**：需要严格区分"已初始化/未初始化"的通用库代码（如 `Vec::push` 内部）。

---

**`[ManuallyDrop<T>; N]` — 不必要**

`ManuallyDrop<T>` 禁止自动 Drop，但假设 `T` 始终"存在"。

```rust
// 必须提供 N 个占位对象
let table: [ManuallyDrop<VmProc>; NR_PROCS] = [
    ManuallyDrop::new(VmProc::placeholder()),
    // ... 重复 NR_PROCS 次
];
```

**问题**：
- 层层包装导致代码冗长：`ManuallyDrop<AssumeSyncCell<VmProc>>` 比 `AssumeSyncCell<VmProc>` 过于冗长
- `ManuallyDrop::take()` 易诱导 Move 操作（从静态数组移到栈上）
- 若 `VmProc` 较大，可能导致栈溢出

> **注意**：`VmProc::vacant()` 本身就是"占位对象"，所以"需要构造占位对象"不是问题。不选 `ManuallyDrop` 纯粹是避免过度包装。

---


### 4.4 最终决策

**采用 `static` + `[AssumeSyncCell<VmProc>; N]` 方案**

```rust
pub struct VmProcTable {
    slots: [AssumeSyncCell<VmProc>; VM_PROC_COUNT],
}

static VM_PROC_TABLE: VmProcTable = VmProcTable {
    slots: [const { AssumeSyncCell::new(VmProc::vacant()) }; VM_PROC_COUNT],
};
```

**三层选型总结**：

| 层次 | 选型 | 理由 |
|------|------|------|
| **元素们如何组织** | 静态数组 `[T; N]` | 地址稳定、零开销、与 Minix3 一致 |
| **元素如何表达** | `AssumeSyncCell<VmProc>` | 与 Minix3 的 `memset(vmproc, 0, ...)` 对应，全初始化 |
| **如何存储** | `static` | 避免 `static mut` 的 Rust 2024 废弃警告 |

**为何不用 `MaybeUninit`**：

Minix3 使用 `memset(vmproc, 0, ...)` 将所有槽位初始化为零，而非延迟初始化。这启发了"全初始化 + 状态字段"的设计：

- `MaybeUninit`：延迟初始化，需额外跟踪"是否初始化"
- `AssumeSyncCell<VmProc>`：预初始化为 `vacant()`，状态由 `VmFlags` 表达

---

## 5. Rust 实现设计

### 5.1 数据结构定义

**进程表结构**：

```rust
/// Number of VM process slots: all user processes + 1 exec temporary slot.
pub const VM_PROC_COUNT: usize = NR_PROCS + 1;

/// Exec temporary slot (index = NR_PROCS).
pub const VM_EXEC_TMP_SLOT: UserSlot = UserSlot(NR_PROCS);

/// VM process table.
pub struct VmProcTable {
    slots: [AssumeSyncCell<VmProc>; VM_PROC_COUNT],
}
```

**设计要点**：
- `VM_PROC_COUNT = NR_PROCS + 1`：包含 exec 临时槽位
- `AssumeSyncCell`：提供 slot 级粒度的独立访问
- 全部预初始化为 `VmProc::vacant()`，状态由 `VmFlags` 表达

**`AssumeSyncCell` 与内部可变性**：

`AssumeSyncCell<T>` = `UnsafeCell<T>` + `unsafe impl Sync`。Rust 要求 `static` 变量必须实现 `Sync`，但 `UnsafeCell` 不满足。VM server 是单线程的，因此通过 `unsafe impl Sync` 绕过此限制。

`AssumeSyncCell` 提供了 slot 级的内部可变性，使得 `VmProcTable` 的方法可以通过 `&self`（共享引用）获取 `&mut VmProc`（可变引用）：

```rust
#[allow(clippy::mut_from_ref)]
pub(super) unsafe fn get_slot_mut(&self, slot: UserSlot) -> Option<&mut VmProc> {
    let index = Self::check_slot(slot)?;
    Some(unsafe { &mut *self.slots[index].get() })
}
```

**为什么 `&self` 能返回 `&mut VmProc`？** `AssumeSyncCell` 的 `.get()` 返回 `*mut T`，绕过了 Rust 的借用规则。这是 `UnsafeCell` 的标准用法——将可变性检查从编译期推迟到运行时。

**为什么 `#[allow(clippy::mut_from_ref)]` 是安全的？** 三个保证：

1. **单线程**：VM server 是单线程的，不存在并发访问
2. **slot 级隔离**：每次只访问一个 slot，不同 slot 之间不冲突
3. **typestate view 消费**：`get_empty()`/`get_active()` 返回的 typestate view 持有 `&mut VmProc`，borrow checker 确保同一 slot 不会同时有两个 `&mut`

**调用链**：`get_empty(&self)` → `get_slot_mut(&self)` → `&mut *slots[i].get()` → `EmptySlot { inner: &mut VmProc }`。虽然入口是 `&self`，但返回的 typestate view 持有独占的 `&mut VmProc`，编译器阻止同一 slot 的二次访问。

### 5.2 全局访问

```rust
static VM_PROC_TABLE: VmProcTable = VmProcTable {
    slots: [const { AssumeSyncCell::new(VmProc::vacant()) }; VM_PROC_COUNT],
};

impl VmProcTable {
    pub fn get_global() -> &'static VmProcTable {
        &VM_PROC_TABLE
    }
}
```

**为何不用 `static mut`**：
1. Rust 2024 逐步淘汰 `static mut`
2. `get_global()` 返回 `&'static VmProcTable`，API 更清晰

### 5.3 Typestate Views

**三种视图类型**：

| View | 状态条件 | 用途 |
|------|----------|------|
| `EmptySlot<'a>` | `!IN_USE` | 分配新进程 |
| `ActiveProc<'a>` | `IN_USE && !EXITING` | 正常运行进程 |
| `ExitingProc<'a>` | `IN_USE && EXITING` | 退出中进程 |

**状态转换**：
```
EmptySlot ──[activate]──> ActiveProc ──[mark_exiting]──> ExitingProc ──[reap]──> EmptySlot
                          ActiveProc ──[force_clear]──────────────────────────> EmptySlot
```

**访问方法**：

```rust
impl VmProcTable {
    pub fn get_empty(&self, slot: UserSlot) -> Option<EmptySlot<'_>>;
    pub fn alloc_empty_slot(&self) -> Option<EmptySlot<'_>>;
    pub fn get_active(&self, slot: UserSlot) -> Option<ActiveProc<'_>>;
    pub fn get_exiting(&self, slot: UserSlot) -> Option<ExitingProc<'_>>;
}
```

**遍历：`VmProcIter`**

```rust
pub struct VmProcIter<'a> {
    table: &'a VmProcTable,
    index: usize,
}

impl<'a> Iterator for VmProcIter<'a> {
    type Item = &'a VmProc;
    fn next(&mut self) -> Option<Self::Item> {
        while self.index < VM_PROC_COUNT {
            let i = self.index;
            self.index += 1;
            let proc = unsafe { &*self.table.slots[i].get() };
            if proc.vm_flags.contains(VmFlags::IN_USE) {
                return Some(proc);
            }
        }
        None
    }
}
```

**Borrow checker 限制**：`VmProcIter` 持有 `&VmProcTable`，在整个迭代期间锁定表的可变访问。迭代器存活时，无法调用 `get_empty()`、`get_active()`、`alloc_empty_slot()` 等需要 `&self` 的方法（它们返回的 typestate view 持有 `&mut VmProc`，与迭代器的 `&VmProcTable` 冲突）。

**解决方案**：如需在遍历中修改表，先收集目标 slot 列表，再 drop 迭代器后逐一操作：

```rust
let slots_to_clear: Vec<UserSlot> = table.iter()
    .filter(|p| /* 条件 */)
    .map(|p| p.vm_slot)
    .collect();
// 迭代器已 drop，现在可以修改表
for slot in slots_to_clear {
    if let Some(active) = table.get_active(slot) {
        unsafe { active.force_clear(); }
    }
}
```

**未来需求：`swap_proc_slot()` (live update)**:

Minix3 的 `swap_proc_slot()` ([utility.c:188](minix3/minix/servers/vm/utility.c#L188)) 在 live update 时交换两个进程的 vmproc 内容，同时保留各自的 endpoint 和 slot 号：

```c
int swap_proc_slot(struct vmproc *src_vmp, struct vmproc *dst_vmp) {
    orig_src_vmproc = *src_vmp;
    orig_dst_vmproc = *dst_vmp;
    *src_vmp = orig_dst_vmproc;
    *dst_vmp = orig_src_vmproc;
    src_vmp->vm_endpoint = orig_src_vmproc.vm_endpoint;
    src_vmp->vm_slot = orig_src_vmproc.vm_slot;
    dst_vmp->vm_endpoint = orig_dst_vmproc.vm_endpoint;
    dst_vmp->vm_slot = orig_dst_vmproc.vm_slot;
    return OK;
}
```

**typestate 体系下的实现**:

由于每个槽位使用独立的 `AssumeSyncCell`（`UnsafeCell`），可以同时获取两个不同槽位的 `ActiveProc`：

```rust
// 这是合法的！两个 ActiveProc 指向不同槽位
let active_a = table.get_active(slot_a)?;
let active_b = table.get_active(slot_b)?;
```

在 `ActiveProc` 上提供 `swap_proc_slot()` 方法（命名与 Minix3 一致）：

```rust
impl<'a> ActiveProc<'a> {
    /// 交换两个进程的内容，保留各自的 endpoint 和 slot。
    ///
    /// 用于 live update：旧服务和新服务交换 vmproc 内容，
    /// 旧服务保留其 endpoint（客户端仍通过该 endpoint 访问），
    /// 但获得新服务的内存状态（新代码、新数据）。
    ///
    /// 对应 Minix3 的 `swap_proc_slot()` (utility.c)。
    pub(crate) fn swap_proc_slot(&mut self, other: &mut ActiveProc<'_>) {
        // 保存各自的标识
        let self_endpoint = self.inner.vm_endpoint;
        let self_slot = self.inner.vm_slot;
        let other_endpoint = other.inner.vm_endpoint;
        let other_slot = other.inner.vm_slot;
        
        // 交换整个 VmProc 内容
        unsafe {
            core::ptr::swap(self.inner as *mut VmProc, other.inner as *mut VmProc);
        }
        
        // 恢复各自的 endpoint 和 slot
        self.inner.vm_endpoint = self_endpoint;
        self.inner.vm_slot = self_slot;
        other.inner.vm_endpoint = other_endpoint;
        other.inner.vm_slot = other_slot;
    }
}
```

**使用示例**:

```rust
// Live update: 旧服务在 slot_a，新服务在 slot_b
let mut old_service = table.get_active(old_slot)?;
let mut new_service = table.get_active(new_slot)?;

// 交换内容：旧服务获得新服务的内存状态，但保留原 endpoint
old_service.swap_proc_slot(&mut new_service);

// 之后 old_service 继续服务客户端（endpoint 不变），
// new_service 可以被清理
```

**设计要点**:

| 方面 | 说明 |
|------|------|
| 不需要特殊 API | 直接使用现有 `get_active()` 获取两个 view |
| `swap_content_with` | 交换内容而非引用，保留标识 |
| 安全性 | 方法签名是 safe 的，内部使用 `unsafe { ptr::swap }` |
| 与 Minix3 对应 | 语义完全一致 |

**与 Minix3 的差异**：

Minix3 的 `vm_isokendpt()` 只检查 `VMF_INUSE`，不检查 `!VMF_EXITING`。Rust 的 `swap_proc_slot()` 要求两个进程都是 `ActiveProc`（`IN_USE && !EXITING`）。

这是**防御性检查**，语义与 Minix3 一致：RS 代码保证正在退出或已终止的服务不会发起 live update（见 RS 的 `check_call_permission()` 和 `RS_EXITING` 处理逻辑）。

### 5.4 可见性设计

**`VmProcTable` 对 vm crate 的可见性**：

| 元素 | 可见性 | 说明 |
|------|--------|------|
| `VmProcTable` 类型 | `pub(crate)` | vm crate 内部可访问 |
| `get_global()` | `pub(crate)` | 获取全局进程表单例 |
| `get_empty()`/`get_active()`/`get_exiting()` | `pub(crate)` | typestate 入口，返回对应 view |
| `vm_isokendpt()` | `pub(crate)` | endpoint 验证，fork 等流程使用 |
| `alloc_empty_slot()` | `pub(crate)` | 分配空闲槽位 |
| `find_free_slot()` | `pub(crate)` | 查找空闲槽位（不获取 view） |
| `is_slot_in_use()` | `pub(crate)` | 查询方法 |
| `used_count()`/`free_count()`/`is_empty()`/`is_full()` | `pub(crate)` | 统计查询 |
| `reset_slot()` | `#[cfg(test)] pub(crate) unsafe` | 仅测试用，重置槽位 |
| `VmProcIter` | `pub(super)` | 仅 vmproc 模块树内部使用 |
| `VM_PROC_COUNT`/`VM_EXEC_TMP_SLOT` | `pub(crate)`（table.rs 内） | 定义在 table.rs 中为 `pub(crate)`，但未从 mod.rs 重导出；vm crate 其他模块可通过 `table::VM_PROC_COUNT` 访问 |

**`mod.rs` 导出策略**：

```rust
mod flags;
mod vmproc;         // VmProc 定义，不导出
mod vmproc_handle;  // typestate view 定义
mod table;

pub(crate) use flags::VmFlags;
pub(crate) use vmproc_handle::{EmptySlot, ActiveProc, ExitingProc};
pub(crate) use table::VmProcTable;
// 注意：VmProc 不导出！VmProcIter 不导出！
// VM_PROC_COUNT/VM_EXEC_TMP_SLOT 未重导出，仅 table.rs 内部使用
```

**最小可见性原则**：vmproc mod 对 vm crate 应仅暴露 `VmProcTable` 的 get_handle 方法（如 `get_empty`/`get_active`/`get_exiting`）和 typestate view handle。`VmFlags`、`VM_PROC_COUNT` 等的重导出需要评估是否可以进一步收紧。

> **TODO**: 当 VM crate 稳定后，再次 review vmproc 模块对 vm crate 暴露的可见性。当前 `VmFlags`、`VM_PROC_COUNT` 的重导出可能过于宽松——如果只有 vmproc 模块树内部使用，应降为 `pub(super)` 或不导出。

---

## 6. 测试与验证

本章列出为保证进程表设计正确性应覆盖的测试维度。每个维度说明**测什么、为什么测**，辅以少量关键断言示例，不罗列完整测试代码。

### 6.1 测试基础设施

测试使用全局进程表 `VmProcTable::get_global()`，每个用例需先调用 `reset_slot()` 清理 slot：

```rust
#[cfg(test)]
pub(crate) unsafe fn reset_slot(&self, slot: UserSlot) {
    if let Some(proc) = self.get_slot_mut(slot) {
        if proc.vm_flags.contains(VmFlags::IN_USE) { proc.clear(); }
        core::ptr::write(proc, VmProc::vacant_with_slot(slot));
    }
}
```

此函数标记 `#[cfg(test)]`——生产代码通过 typestate API 转换状态，不应绕过。两步清理（先 `clear()` 清除 `IN_USE` 避免 Drop panic，再 `ptr::write` 覆盖跳过 Drop）。

### 6.2 Typestate 生命周期

| 测试点 | 验证目标 | 关键断言 |
|--------|----------|----------|
| Empty → Active | `activate()` 设置 IN_USE + endpoint | `flags.contains(IN_USE)`, `endpoint() == ep` |
| Active → Exiting | `mark_exiting()` 添加 EXITING 标志 | `flags.contains(EXITING \|\| IN_USE)` |
| Exiting → Empty | `reap()` 清除所有标志 | `!is_slot_in_use()`, `endpoint == NONE` |
| 完整生命周期 | Empty → Active → Exiting → Empty | 每步状态正确，最终 slot 可重用 |
| 重用 slot | reap 后再次 activate | 新 endpoint 正确设置，generation 递增 |

### 6.3 Typestate 访问互斥

| 测试点 | 验证目标 | 关键断言 |
|--------|----------|----------|
| 空 slot 的 `get_active()` | 空闲 slot 不是 Active | `get_active(empty_slot) == None` |
| 退出中 slot 的 `get_active()` | EXITING 不是 Active | `get_active(exiting_slot) == None` |
| 活跃 slot 的 `get_empty()` | IN_USE 不是 Empty | `get_empty(active_slot) == None` |
| 活跃 slot 的 `get_exiting()` | 非 EXITING 不是 Exiting | `get_exiting(active_slot) == None` |
| 退出中 slot 的 `get_exiting()` | EXITING 是 Exiting | `get_exiting(exiting_slot).is_some()` |

### 6.4 `activate()` vs `activate_relaxed()`

| 测试点 | 验证目标 | 关键断言 |
|--------|----------|----------|
| `activate()` 一致 endpoint | slot 与 endpoint 匹配时成功 | `activate(ep_with_matching_slot)` 返回 `ActiveProc` |
| `activate()` 不一致 endpoint | slot 与 endpoint 不匹配时 debug 模式 panic | `activate(ep_with_wrong_slot)` debug_assert 失败 |
| `activate_relaxed()` 不一致 endpoint | 不检查匹配，直接激活 | `activate_relaxed(Endpoint::NONE)` 成功 |
| fork 场景 | child endpoint 初始为 NONE | `activate_relaxed(Endpoint::NONE)` → ActiveProc |

### 6.5 `force_clear()` 异常回滚

| 测试点 | 验证目标 | 关键断言 |
|--------|----------|----------|
| Active → Empty | 强制清除 IN_USE | `force_clear()` 后 `!is_slot_in_use()` |
| 回滚后可重用 | slot 回到 EmptySlot 状态 | `get_empty(slot).is_some()` |

> `force_clear()` 是 fork 失败时的回滚路径。Minix3 在 fork 失败时存在 slot 泄漏（IN_USE 未清除），Rust 通过 `force_clear()` 显式回滚。

### 6.6 `vm_isokendpt()` 验证

| 测试点 | 验证目标 | 关键断言 |
|--------|----------|----------|
| 有效 endpoint | slot 范围 + endpoint 匹配 + IN_USE | `vm_isokendpt(valid_ep) == Some(slot)` |
| 越界 slot | slot < 0 或 >= VM_PROC_COUNT | `vm_isokendpt(out_of_range_ep) == None` |
| endpoint 不匹配 | 进程已退出，slot 被新进程重用 | `vm_isokendpt(old_ep) == None` |
| 非 IN_USE | slot 空闲 | `vm_isokendpt(ep_for_empty_slot) == None` |

> endpoint 不匹配是 TOCTOU 防护的关键：旧进程退出后 slot 被新进程重用，endpoint 的 generation 字段不同，验证失败。

### 6.7 `alloc_empty_slot()` 分配

| 测试点 | 验证目标 | 关键断言 |
|--------|----------|----------|
| 空表分配 | 找到空闲 slot | `alloc_empty_slot().is_some()` |
| 分配后状态 | slot 变为 IN_USE | `is_slot_in_use(slot) == true` |

### 6.8 `VmProcIter` 遍历

| 测试点 | 验证目标 | 关键断言 |
|--------|----------|----------|
| 遍历活跃进程 | 只返回 IN_USE 的进程 | `iter().filter(\|p\| p.is_in_use()).count()` 正确 |
| 遍历时不可变 | 迭代器持有 `&VmProcTable`，阻止可变访问 | 编译期保证，无需测试 |

### 6.9 测试维度总结

```
VmProcTable 测试覆盖
├── 生命周期：Empty → Active → Exiting → Empty
├── 访问互斥：get_empty/get_active/get_exiting 互斥
├── 激活模式：activate (严格) vs activate_relaxed (宽松)
├── 异常回滚：force_clear 清除 IN_USE
├── endpoint 验证：vm_isokendpt 三重检查
├── slot 分配：alloc_empty_slot
├── 遍历：VmProcIter 只返回活跃进程
└── borrow 限制：迭代期间不可变访问
```

---

## 总结

### 核心设计

1. **静态数组 + AssumeSyncCell**：地址稳定，与 Minix3 的 `memset` 语义一致
2. **Typestate Views**：`EmptySlot` / `ActiveProc` / `ExitingProc` 编译期保证状态转换
3. **可见性分层**：模块内宽松，模块外只能通过 API 访问
4. **显式清理**：`clear()` 方法替代隐式 Drop

### 与 Minix3 的对应

| Minix3 | Rust 实现 |
|--------|-----------|
| `vmproc[slot]` | `slots[slot]` |
| `VMF_INUSE` | `VmFlags::IN_USE` |
| `clear_proc()` | `proc.clear()` |
| `vm_isokendpt()` | `VmProcTable::vm_isokendpt()` |
| 验证后获取进程 | `get_active()` / `get_exiting()` |

### 安全保证

- **编译期**：Typestate Views 保证状态转换合法性
- **可见性**：`VmProc` 不导出，外部只能通过 API 访问
- **显式控制**：无隐式 Drop，资源释放时机确定

---

## 7. 附录

### 附录A：`vmproc` 不受硬件地址稳定性限制的完整论证

### A.1 问题背景

传统观点认为内核数据结构需要"地址稳定性"，但这并不够触及本质，深入分析 Minix3 代码后发现：
- **页表物理内存**确实需要硬件级别的地址稳定
- 但 **`vmproc` 结构体本身**不直接被硬件绑定

### A.2 页表内存的实际位置

**关键代码分析** (`minix3/minix/servers/vm/pt.h` 和 `pagetable.c`)：

```c
// pt.h - 页表结构
typedef struct {
    u32_t *pt_dir;              // 页目录虚拟地址（VM 地址空间中的指针）
    u32_t pt_dir_phys;          // 页目录物理地址（u32_t 值，非指针）
    u32_t *pt_pt[ARCH_VM_DIR_ENTRIES];  // 页表指针数组
    u32_t pt_virtop;            // 虚拟地址空间空洞搜索提示
} pt_t;
```

**三个字段的关系**：

```
pt_dir ──► 页目录（PDE[0..1023]）── 物理页，pt_new()时分配
              │
              ├── PDE[0] ──► pt_pt[0] ──► 页表 0 ── 按需分配
              ├── PDE[1] ──► pt_pt[1] ──► 页表 1 ── 按需分配
              └── ...
```

**页表分配流程** (`pt_new()`)：

```c
int pt_new(pt_t *pt) {
    // 1. 分配页目录物理页
    pt->pt_dir = vm_allocpage(&pt->pt_dir_phys, VMP_PAGEDIR);
    
    // 2. 清空页目录
    for(i = 0; i < ARCH_VM_DIR_ENTRIES; i++)
        pt->pt_dir[i] = 0;
    
    // 3. 映射内核区域...
}
```

### A.3 页表绑定到硬件的流程

**`pt_bind()` 的执行流程**：

```c
// exec_bootproc() 中的调用
if(pt_new(&vmp->vm_pt) != OK)        // 1. 创建页表（在 VM 中）
    panic("VM: no new pagetable");

if(pt_bind(&vmp->vm_pt, vmp) != OK)  // 2. 绑定页表到 MMU
    panic("VM: pt_bind failed");
```

**`pt_bind()` 的本质**：
- 通过 `sys_vmctl` 系统调用
- 将 `pt_dir_phys`（页目录物理地址值，u32_t）传递给内核
- 内核将该值写入 CR3/TTBR0/SATP 寄存器

### A.4 核心原理：Bind Value Not Address

**数据流**：

1. **VM 侧**：`vmproc.vm_pt.pt_dir_phys = 0x12340000`（u32_t 值）
2. **系统调用**：`sys_vmctl(0x12340000)` → 值复制到内核
3. **内核侧**：`CR3 = 0x12340000`（写入硬件寄存器）
4. **硬件侧**：MMU 直接使用物理地址 `0x12340000` 访问页表

**核心观察**：
- 硬件绑定的是**物理地址值**（`pt_dir_phys`），不是**容器地址**（`vmproc` 的地址）
- `vmproc` 结构体移动 → `pt_dir_phys` 值**不变** → 硬件**不受影响**

**关键区分**：

| 组件 | 类型 | 硬件绑定？ | 稳定性来源 |
|------|------|-----------|-----------|
| **页表物理内存** | 物理页框 | ✅ 是 | 物理地址固定（`alloc_mem` 分配） |
| **`pt_dir_phys` 值** | u32_t | ✅ 是 | 值传递后存储在 CR3 寄存器 |
| **`pt_dir` 虚拟地址** | 指针 | ❌ 否 | VM 内部使用，可重新映射 |
| **`vmproc` 结构体** | 管理结构 | ❌ 否 | 不直接绑定硬件 |

### A.5 为什么 `vmproc` 移动不影响硬件？

**假设场景**：`vmproc` 结构体在内存中移动

```
移动前：
  vmproc[5] @ 0x1000
  └─ vm_pt.pt_dir_phys = 0x12340000
  
  CR3 = 0x12340000 ✓

移动后：
  vmproc[5] @ 0x2000  （地址变了！）
  └─ vm_pt.pt_dir_phys = 0x12340000  （值不变！）
  
  CR3 = 0x12340000 ✓ （仍然正确）
```

**结论**：
- `vmproc` 移动 → `pt_dir_phys` 值（0x1234000）**不变**
- CR3 寄存器中的值**不变**
- 硬件继续访问 0x1234000，**不受影响**

### A.6 软件层为什么仍需要地址稳定？

虽然硬件不强制 `vmproc` 地址稳定，但软件设计需要：

1. **引用传递模式**（Minix3 代码中大量存在）
   ```c
   void free_proc(struct vmproc *vmp);   // 接收指针
   void clear_proc(struct vmproc *vmp);  // 接收指针
   ```

2. **长期指针保存**（`vm_exec_info` 结构体）
   ```c
   struct vm_exec_info {
       struct vmproc *vmp;  // 长期保存的指针
   };
   ```

3. **重构成本**：消灭所有长期引用需要全面修改代码，成本巨大

### A.7 结论

| 问题 | 答案 |
|------|------|
| 页表物理内存在哪里？ | **系统物理内存**中，通过 `alloc_mem()` 分配 |
| 硬件绑定的是什么？ | **物理地址值**（`pt_dir_phys`），不是 `vmproc` 指针 |
| `vmproc` 需要地址稳定吗？ | **硬件不强制**，但软件设计需要 |
| 为什么选静态数组？ | 最简单、最便宜、与 Minix3 语义一致 |

**核心原理**：**Bind Value Not Address**
- 硬件绑定的是**物理地址值**（已传递出去）
- 不是**容器地址**（`vmproc` 的地址）
- 因此 `vmproc` 的地址稳定性是**软件设计选择**，非硬件强制

## 8. 参见

- [01-vmproc-struct.md](01-vmproc-struct.md) - VmProc 结构体定义与 typestate 设计
- [00-vm-overview.md](00-vm-overview.md) - VM 整体架构与分布式一致性分析
- [03-acl.md](03-acl.md) - 访问控制
- [06-pagetable-struct.md](06-pagetable-struct.md) - 页表结构（pt_dir_phys 等）
