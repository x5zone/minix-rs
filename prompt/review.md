# Minix-RS Review 指南（核心）

> 本指南用于对 Minix-RS 项目的**文档**和**代码**进行深度 Review。
> 适用范围：**所有 Minix3 模块**（VM、PM、VFS、Kernel、Drivers 等）

---

## 核心原则

本项目是对 Minix3 内核模块的 **语义重建（Rewrite）**，不是翻译，不是重设计。

| 术语 | 定义 |
|---|---|
| **Translate** | 1:1 翻译 C 代码，仅做语法转换。❌ 禁止 |
| **Rewrite** | 保持外部可观察行为不变，内部用 Rust 类型系统重新表达。✅ 目标 |
| **Redesign** | 改变系统架构、机制或协议。❌ 当前禁止 |

**核心原则**：外部语义不变，内部表达可以改变。把"隐式编码"变成"显式协议"。

### 当前执行模型

当前 VM / PM / VFS 等服务器默认假设：

- **单线程事件循环**：每个服务器是独立的单线程进程
- **无共享内存并发修改**：服务器间通过 IPC 通信，不共享内存
- **无 SMP 并行访问**：不存在多核同时访问同一数据结构的情况

因此以下设计是合理的：
- `Rc` 可替代 `Arc`（无跨线程共享）
- `RefCell` 可替代 `Mutex`（无并发访问）
- `!Send` / `!Sync` 是合理的（数据不跨线程）
- `UnsafeCell` 在单线程前提下是安全的

> ⚠️ 如果未来扩展为多线程，需重构状态管理。

### 运行时环境约束

所有 Rust 代码（除 mock 和 test 外）必须在 `no_std` 环境下运行。

**可用**：
- `core` — Rust 核心库（Always available）
- `alloc` — 堆分配（需要全局分配器，VM 自行实现）
- 自定义 crate（`minix_arch`、`minix_types` 等）

**禁止**：
- `std` — 标准库（OS 本身不可能依赖另一个 OS 的标准库）
- `std::sync` — 标准同步原语（Mutex、RwLock 等）
- `std::collections` — 标准集合（HashMap 等，用 `alloc` 版本替代）
- `std::io` / `std::fs` — 文件 I/O（不存在文件系统服务给 VM 用）
- `std::thread` — 标准线程（VM 是单线程事件循环）

**判定标准**：如果某段代码 `use std::`，则必须改为 `no_std` 兼容实现，
除非该代码仅在 `#[cfg(test)]` 或 mock 中使用。

### Allowed Evolution（允许的架构演进）

以下变化被视为**架构演进**，而非 Redesign：

| 演进类型 | Minix3 (C/32位) | minix-rs (Rust/64位) | 说明 |
|---------|----------------|---------------------|------|
| 架构位宽 | x86-32 | x86-64 | 利用 64 位地址空间 |
| 页表层级 | 2级 (PD+PT) | 4级 (PML4+PDPT+PD+PT) | 64位需要更多层级 |
| 页表项大小 | 32位 (u32) | 64位 (u64) | 支持更大物理地址 |
| 状态表达 | `int state` + 宏 | `enum` / `typestate` | 非法状态不可表达 |
| 错误处理 | 返回 `OK` / `errno` | `Result<T, Error>` | 错误码需严格对应 |
| 资源管理 | 手动 `free()` | RAII / `Drop` | 自动释放，避免泄漏 |
| 权限控制 | `bitchunk_t` 数组 | `bitflags` / `enum` | 类型安全 |
| 内存分配 | VM 私有 slab | `alloc` + 自定义全局分配器 | `no_std` 下使用 `alloc` crate |

**判定标准**：只要满足以下四点，即属于 Rewrite 范畴：
1. **外部可观察行为不变**（IPC 接口、返回值、副作用相同）
2. **IPC 协议不变**（消息格式、调用号、权限检查相同）
3. **生命周期语义不变**（创建、使用、释放的顺序相同）
4. **调度/权限/地址空间语义不变**（进程隔离、内存保护相同）

### 行为语义优先原则

Review 的核心目标是：**验证 Rust 实现是否保持了 Minix3 的行为语义**，而不是要求数据结构或代码组织形式与 C 源码一致。

**允许**：
- 数据结构重组（如将相关字段提取为子结构）
- 状态拆分（如将 `int flags` 拆分为多个 `enum`）
- 生命周期显式化（如用 `typestate` 替代状态字段）
- trait 抽象（如用 `trait PageTable` 替代具体实现）

**不允许**：
- 改变外部行为（如改变 `alloc_mem` 的分配策略）
- 改变 IPC 协议（如改变消息格式或调用号）
- 改变生命周期语义（如提前或延迟释放资源）
- 改变错误恢复语义（如改变错误码含义或重试策略）

### 硬件抽象原则

**核心原则：抽象机制，而非描述硬件。描述"做什么"，而非"怎么做"。**

Minix3 的 C 代码经常将硬件细节直接编码进数据结构（如 `pt_t` 将页目录指针、物理地址、PDE/PTE 位编码进结构体，与 x86-32 紧耦合；支持 arm32 则依赖 `#if defined()` 条件编译）。这种做法的本质是**描述硬件**——"第几个 PDE 指向哪个页表"。

Rust 版本必须改变建模方式：OS 需要的是**机制**（映射地址、设置权限、切换地址空间），而非**硬件细节**。

**强制规则：所有硬件都必须被抽象为 trait。**

| Minix3 做法 | Rust 做法 | 说明 |
|------------|----------|------|
| 结构体直接编码硬件寄存器布局 | `trait` 定义机制接口 | 描述"做什么"，而非"怎么做" |
| `#if defined()` 条件编译 | 各架构实现同一 trait | 新增架构只需实现 trait |
| 硬件位编码暴露给上层 | `PageFlags` 等 OS 语义类型 | OS 层不感知 PDE/PTE 位编码 |
| 全局常量硬编码页大小 | trait 关联常量 | 代码自动适配目标架构 |

**Review 检查点**：
- ❌ 上层代码直接操作硬件寄存器或 PTE 位编码 → 应通过 trait 方法
- ❌ 数据结构中包含架构特定的硬件字段（如 `pde` 数组）→ 应由 trait 实现内部管理
- ❌ 使用 `#[cfg(target_arch)]` 条件编译选择硬件行为 → 应通过 trait 静态分派
- ✅ 上层仅依赖 trait 接口，不感知底层硬件布局
- ✅ 各架构自行实现 trait，通过泛型或关联类型绑定
- ✅ OS 语义类型（如 `PageFlags`）与硬件编码分离

**示例**（页表）：
```rust
// ❌ 错误：直接暴露硬件细节
struct PageTable {
    pde: [u32; 1024],  // x86-32 PDE 数组，与硬件紧耦合
}

// ✅ 正确：抽象机制为 trait
trait Paging {
    const PAGE_SIZE: usize;
    fn map(&mut self, vaddr: VirBytes, paddr: PhysBytes, flags: PageFlags) -> Result<(), PageTableError>;
    fn unmap(&mut self, vaddr: VirBytes) -> Result<PhysBytes, PageTableError>;
    fn query(&self, vaddr: VirBytes) -> Option<(PhysBytes, PageFlags)>;
}
```

### 文档链路模型

每篇文档遵循以下链路结构，Review 必须验证链路的完整性：

```
Ch1(概念) + Ch2(源码分析) ──推导──▶ Ch3(设计决策) ──实现──▶ Ch4(代码实现) ──生成──▶ Rust code
                                        │                      │
                                        └──── 推导 ────────────┘
                                                │
                                                ▼
                                        倒数第二章(测试要点) ──生成──▶ test code
```

**链路验证规则**：
1. **Ch3 必须基于 Ch1&2**：每个设计决策必须能追溯到 Ch1 的概念或 Ch2 的源码分析
2. **Ch4 必须遵循 Ch3**：每个实现细节必须对应 Ch3 的某个设计决策
3. **测试必须覆盖 Ch3+Ch4**：每个设计决策和关键实现都应有对应的测试要点
4. **代码必须匹配 Ch4**：Rust 代码必须与 Ch4 描述的实现一致
5. **Ch1&2 必须完整覆盖 C 源码**：文档语义范围内的所有 Minix3 函数、结构体、宏必须被完整分析，不得遗漏。详见 [review-doc-checklist.md §2.8](review-doc-checklist.md#28-c-源码覆盖完整性检查)
6. **Ch3&4 必须完整实现 Ch1&2 语义**：Ch1&2 中分析的每个概念、每个函数行为、每个数据结构，必须在 Ch3&4 中有对应的设计决策和实现。Ch1&2 有但 Ch3&4 无 = 语义丢失

**违反链路的典型问题**：
- Ch3 出现 Ch1&2 未提及的概念 → 设计缺乏依据
- Ch4 实现了 Ch3 未设计的功能 → 实现超出设计
- Ch3 设计了但 Ch4 未实现 → 设计悬空
- 测试未覆盖 Ch3 的关键设计决策 → 测试不足
- Ch1&2 遗漏了语义范围内的 C 函数/结构体/宏 → 覆盖不完整，后续设计可能缺功能
- Ch1&2 分析了但 Ch3&4 未实现 → 语义丢失，Rewrite 不完整

### 不要过度模拟 C

Rewrite 的目标不是复刻 Minix3 的历史实现细节。如果某个设计仅仅是以下限制的结果，则应优先使用现代 Rust 表达：

- **C 语言限制**（如缺乏泛型、模式匹配、trait）
- **32 位限制**（如固定大小数组、地址空间假设）
- **无类型系统限制**（如裸指针、魔术数字、隐式转换）
- **无 RAII 限制**（如手动内存管理、错误码传递）

**反例**：
```rust
// ❌ 错误：为了对应 C 源码而手写 intrusive linked list
struct ListNode { next: *mut ListNode, ... }

// ✅ 正确：使用 Rust 标准库或安全抽象
struct FreeList { head: Option<Box<Node>> }
```

### Ground Truth 优先级

```
Minix3 源码行为  >  文档描述  >  Rust 实现  >  AI 分析
```

**永远以 Minix3 源码行为为最终真理来源**。

---

## 模块路由表

| 任务类型 | 加载模块 | 说明 |
|---------|---------|------|
| 文档 Review | [review-doc-checklist.md](review-doc-checklist.md) + [review-patterns.md](review-patterns.md) | 检查文档结构、概念准确性、C 源码覆盖 |
| 代码 Review | [review-code-checklist.md](review-code-checklist.md) + [review-patterns.md](review-patterns.md) | 检查 Rewrite 质量、类型安全、硬件抽象 |
| 完整 Review | 全部模块 | 按 [review-process.md](review-process.md) 执行 Step 1-5 |
| 快速 Review | [review-process.md](review-process.md) §口诀 | 用判断口诀快速扫描 |

---

## 审查优先级矩阵

### P0 - 阻塞性检查（必须修复）

| 维度 | 检查项 |
|------|--------|
| 文档 | 概念错误、虚构事实；C 代码引用错误 |
| 代码 | UB、内存安全漏洞；语义偏移；硬件语义泄漏；错误码不对齐；`std::` 违规（非 test/mock） |

### P1 - 设计问题（建议修复）

| 维度 | 检查项 |
|------|--------|
| 文档 | 架构差异未说明；覆盖不完整；设计决策缺乏依据；章节链路断裂 |
| 代码 | typestate 无效；pub 滥用；模块职责不清；所有权混乱；代码与文档设计不一致；硬件未抽象为 trait |

### P2 - 改善性检查（可选修复）

| 维度 | 检查项 |
|------|--------|
| 文档 | 表述清晰度；交叉引用完整性；ASCII 图质量；替代方案未记录 |
| 代码 | 命名规范；注释覆盖率/质量；测试覆盖 |

---

## 使用流程

1. **确定任务类型**：文档 Review / 代码 Review / 完整 Review / 快速 Review
2. **加载对应模块**：按路由表加载所需检查清单和流程
3. **执行 Review**：按 [review-process.md](review-process.md) 的强制步骤执行
4. **输出结果**：按 [review-process.md](review-process.md) 的输出格式整理

---

## 附录：模块清单

- [review-doc-checklist.md](review-doc-checklist.md) — 文档结构规范 + 8 个检查维度
- [review-code-checklist.md](review-code-checklist.md) — 11 个代码检查维度
- [review-patterns.md](review-patterns.md) — 常见错误模式（文档 + 跨文档）
- [review-process.md](review-process.md) — AI 强制步骤 + 输出格式 + 工具命令 + 口诀
- [review-profiles.md](review-profiles.md) — 任务组合配置（按需加载策略）
