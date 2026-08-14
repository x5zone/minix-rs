# Minix-RS Review 指南（核心）

> 本指南用于对 Minix-RS 项目的**文档**和**代码**进行深度 Review。
> 适用范围：**所有 Minix3 模块**（VM、PM、VFS、Kernel、Drivers 等）

> **术语表（2026-08-15 修复 D-P1-6）**：
> - **上游/下游**：文档章节链路中，"上游"指先出现的章节（如 Ch1 → Ch3 → Ch4），"下游"指后出现的章节
> - **同级**：同一链路层级的章节（如 Ch1 & Ch2 都为概念层）
> - **横向**：同一章节内的对比（如 §3.5 vs §4.3 同属一个文档但描述同一主题）
> - **纵向**：跨章节追溯（如 Ch1 概念 → Ch3 设计决策 → Ch4 实现 → Ch5 测试）
> - **depth-first / breadth-first**：本规则集不借用算法概念，"深度"指 Step 0-7 全量，"广度"指各 Step 内多维度并查

---

## 核心原则

本项目是对 Minix3 内核模块的 **语义重建（Rewrite）**，不是翻译，不是重设计。

| 术语 | 定义 |
|---|---|
| **Translate** | 1:1 翻译 C 代码，仅做语法转换。❌ 禁止 |
| **Rewrite（重写）** | Rust 重写 Minix3，保持外部语义不变，内部用 Rust 类型系统重新表达。✅ 目标 |
| **Architectural Evolution（架构演进）** | 改变系统架构、机制或协议（如 32→64 位、freepdes→direct_map）。⚠️ 需 Gate H 评估 |
| **Refactor（重构）** | Rust 实现偏离 design 时，修复 Rust 代码使其回到 design 路径（code Refactor）或修复 design 自身（design Refactor）。✅ 内部纠错 |

**核心原则**：外部语义不变，内部表达可以改变。把"隐式编码"变成"显式协议"。

**三层工作流映射**（review-rules 自包含定义）：
```
Rewrite（重写）
  └─ 实施中偏离 design → Refactor（重构）
       ├─ code Refactor：design 正确，仅修 code（P0-design-deviation）
       └─ design Refactor：design 自身错，先修 design 再修 code（P0-design-missing/wrong）
  └─ 跨越 Minix3 语义边界的设计变更 → Architectural Evolution（架构演进）
```

> **术语冲突解决方案**：原"Redesign"在不同上下文含义不同，正式定义为"改变架构"即 **Architectural Evolution**，工作场景用于"修复偏离"即 **code Refactor**。详见 [review-core-semantics.md §Refactor 定义](review-core-semantics.md)。

> **2026-08-15 修复 D-P1-5（Refactor 三分类术语层级澄清）**：
> - **第一层（按工作目标）**：
>   - **Rewrite**（重写）：语义重建，保持外部行为
>   - **Refactor**（重构）：修复偏离，含 code Refactor + design Refactor（**不涉及架构变化**）
>   - **Architectural Evolution**（架构演进）：跨越 Minix3 语义边界的设计变更（**改变架构**）
> - **第二层（Refactor 子类）**：
>   - **code Refactor**：design 正确但 code 偏离 → 修 code 回到 design（对应 P0-design-deviation）
>   - **design Refactor**：design 自身错 / 漏 → 先修 design 再修 code（对应 P0-design-missing/wrong）
> - **第三层（具体动作）**：删代码 / 改类型 / 加 trait / 重命名 / 加测试等
> - **判定原则**：若改动跨越 Minix3 语义边界 → Architectural Evolution；若仅修复偏离（含 design 自身错） → Refactor

### Design First 原则（Rust 重写场景）

> **背景**：Minix-RS 是 Rust 重写 Minix3，**design 本身是核心交付物之一**，不是 review 的副产品。Review 必须从 design 状态判定开始。

| 概念 | 在传统 review 中 | 在 Minix-RS 中 |
|------|---------------|---------------|
| design 文档 | 参考 | **强制约束（design.md 非 bagging / design-final.md bagging）** |
| design 缺失 | 不关心 | **P0-design-missing（阻断 review）** |
| design 错误（漏概念） | 不常见 | **P0-design-wrong（触发 design Refactor）** |
| design 更新 | 自由 | **必须 review** |

**判定优先级**（从高到低）：
1. **Minix3 C 源码**（ground truth，优先级最高）
2. **design-final.md**（bagging 场景，多 AI 评审定稿）或 **design.md**（非 bagging 默认）
3. **当前 Rust 实现**（如果符合 design）
4. **文档**

**Refactor 触发条件**：
- **code Refactor 触发**（P0-design-deviation）：Step 1.6.2 一致性 < 80% 但 design 本身正确（修复 code 回到 design 路径）
- **design Refactor 触发**（P0-design-missing/wrong）：
  - Step 0 design 预检 `design.md/design-final.md: 缺失`（**主入口，前移自 Step 1.6**）
  - Step 2 发现 Minix3 核心概念在 design 中无对应
  - Step 3.5 发现 design 未覆盖的纵向链路
  - Step 1.6.3 Minix3 对齐检查发现 design-wrong
- **Architectural Evolution 触发**（跨越 Minix3 语义边界的设计变更）：如 32 位临时窗口 → 64 位 direct_map

> **配套机制**：Step 1.6 设计对齐检查 + Gate H design 门控 + Review 中断协议（IN_DESIGN）+ Profile R 设计优先模式。详见 [review-process.md](review-process.md)。

### §2.0 架构演进作为独立知识点维度（review.md 专属）

> 背景：架构演进本身是知识点（如 FPU xsave→aarch64 CPACR_EL1→riscv64 sstatus.FS）
> **回应**：review 不只检查"当前架构是否正确"，还要检查"是否讲清了演进史 + 现代硬件模型 + Rust 抽象方向"。

**架构演进知识点分类（5 类必覆盖维度）**：

| 演进类型 | 示例 | 文档应讲述什么 |
|---------|------|---------------|
| **硬件机制演进** | FPU: xsave → XSAVE → aarch64 CPACR_EL1 → riscv64 sstatus.FS | 历史包袱 → 现代硬件模型 → Rust 抽象 |
| **中断控制器演进** | i8259 → APIC → GICv3 → PLIC | 中断路由原理 + 各架构具体实现 |
| **分页机制演进** | 32 位 4MB 大页 → 64 位 4 级页表 → Sv39 | 位宽演进 + Rust 抽象方向 |
| **地址空间布局演进** | freepdes 临时窗口 → direct_map | 受限空间妥协 → 充裕空间自然解 |
| **内核加载演进** | 实模式 → 保护模式 → 长模式 → SBI | 启动链 + 各架构入口点 |

> **2026-08-15 修复 B-P1-5（ARCH 标注机制）**：Minix-RS 是 Minix3 重写，所有设计**应有 C 源对应**。但允许引入 Minix3 没有的"纯 Rust 新功能"（如借用 checker / typestate / 零成本抽象等）。此类新增必须显式标注：
> - **标注格式**：`[ARCH: New, 原因]` 三处一致标注（doc + design + code）
> - **判定信号**：code 中无 `// ARCH: ...` 但使用了 Minix3 没有的 Rust 惯用法 → P1-architecture-undocumented
> - **示例**：
>   - `// ARCH: New, Rust 借用检查替代 C 手动 refcount`（code）
>   - `design.md §3.2` 含 `[ARCH: New]` 段（design）
>   - `doc §3.5` 含 `[ARCH: New]` 引用（doc）

**强制要求**：
- ✅ Ch1 概念章必须有"架构演进史"小节
- ✅ 不只讲当前架构，必须讲历史动机（"为什么 X 被 Y 取代"）
- ✅ 必须给"现代硬件模型"的统一抽象
- ✅ 必须标注 Rust 抽象方向

**判定信号**：
- ❌ 文档只讲当前架构不讲演进史 → P1-knowledge-incomplete
- ❌ 文档只列函数不讲原理 → P0-implementation-driven（模式 51）
- ✅ 含"演进史小节 + 现代模型抽象 + Rust 方向" → A 级卓越

**配套修订**：[review-doc-checklist.md](review-doc-checklist.md) §1 文档质量总览增加"架构演进维度"行；[review-doc-excellence.md §4.4](review-doc-excellence.md) 增加文档组织合理性检查。

### 执行模型（按模块分层）

Minix3 系统有两类不同的执行模型，Review 时必须根据模块类型选择对应假设。

**A. 用户态服务器（VM/PM/VFS/RS/DS/INET 等）**：
- **单线程事件循环**：每个服务器是独立的单线程进程
- **无共享内存并发修改**：服务器间通过 IPC 通信，不共享内存
- **无 SMP 并行访问**：不存在多核同时访问同一数据结构的情况

因此以下设计是合理的：
- `Rc` 可替代 `Arc`（无跨线程共享）
- `RefCell` 可替代 `Mutex`（无并发访问）
- `!Send` / `!Sync` 是合理的（数据不跨线程）
- `UnsafeCell` 在单线程前提下是安全的

**B. 内核（Kernel）**：
- **SMP 支持**：`CONFIG_SMP` 启用时，多核可同时在**内核空间**中执行
- **BKL（Big Kernel Lock）**：全局 spinlock `big_kernel_lock`，以 `BKL_LOCK()`/`BKL_UNLOCK()` 保护临界区
- **BKL 是 spinlock**（busy-wait）——临界区内**禁止**睡眠、调度、等待 IP
- **CPU-local 变量**：`get_cpu_var()`/`put_cpu_var()` 用于 per-CPU 数据隔离
- `Rc`/`RefCell` 不直接适用于跨 CPU 共享的内核数据（`Rc: !Send + !Sync`，`RefCell: !Sync`）
- `UnsafeCell` 安全论据不能是"单线程"——必须显式论证 BKL 保护、per-CPU 隔离、或 lock-free 语义
- 跨 CPU 共享数据结构需要 `Arc` + `Mutex`/`RwLock`、`Atomic*`、或 BKL 保护 + 注释说明

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
- `std::thread` — 标准线程（用户态服务器是单线程事件循环；内核虽 SMP 但 `no_std` 不可用 `std::thread`，须自定义同步原语）

**判定标准**：如果某段代码 `use std::`，则必须改为 `no_std` 兼容实现，
除非该代码仅在 `#[cfg(test)]` 或 mock 中使用。

### Allowed Evolution（允许的架构演进）

以下变化被视为**架构演进（Architectural Evolution）**，而非 Refactor 或 Redesign：

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

### 概念抽象原则

> **与硬件抽象原则互补**：硬件抽象原则约束代码层（trait 抽象机制而非描述硬件）；概念抽象原则约束文档层（概念框架而非代码框架）。

**核心原则：概念章节（Ch1）必须从架构视角组织，而非从代码视角组织。**

| 维度 | ❌ 代码视角 | ✅ 架构视角 |
|------|-----------|-----------|
| 组织框架 | trait 名 / 函数名 / 结构体名 | CPU 需要回答的问题 / 系统机制 |
| 引入顺序 | 先展示代码，再总结概念 | 先解释 WHY，再展示 WHAT |
| 跨架构 | 分架构讲解实现差异 | 先建立统一抽象，再分架构展示 |
| 读者收获 | "代码做了这些事" | "为什么需要这些机制" |

**Review 检查点**：
- ❌ Ch1 以函数名/trait 名作为概念定义的起点 → 应从"为什么需要"出发
- ❌ Ch1 直接堆三架构细节而无统一抽象层 → 应先建立共性框架
- ❌ Ch1 用 L2（实现抽象层，如 trait 名）术语作概念定义 → 应用 L0/L1（硬件/OS 概念层）术语
- ✅ Ch1 遵循 WHY→WHAT→HOW 顺序
- ✅ 多架构内容先给统一抽象（如"CPU 三问"），再分架构展开

> **来源**：03-kmain-cstart 重构案例。详见 [review-doc-checklist.md §1.Ch1](review-doc-checklist.md#1ch1-ch1-强制骨架) Ch1 骨架检查、[review-patterns.md](review-patterns.md) 模式 51（实现驱动概念章）。

### 规则演化机制（Rule Evolution）

> 规则集本身不是真理，是"在 Minix-RS Review 历史中反复出现的失败模式的归纳"。

**演化步骤**：
1. **触发**：用户/AI 在 review 中发现 ≥2 次同类新模式
2. **提案**：AI 生成新模式提案（含案例、判定、归类）写入 scan.md §Rule Discovery
3. **评审**：用户确认是否纳入
4. **落地**：写入对应 rules 文件，附"来源：xxx 案例"
5. **验证**：下一轮 review 验证新规则是否生效

**强制填写**（每次 Review 完成后）：
- 本次 Review 是否发现新模式？[✅/❌]
- 新模式名/案例/归类/规则草案（若 ✅）

> **详见**：[review-process.md §Step 5.7/7.5 Rule Discovery](review-process.md)

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

> **各文件角色分工**：
> - [review.md](review.md)（本文）：链路模型的原则定义
> - [review-doc-checklist.md §2.10](review-doc-checklist.md#210-章节链路验证)：链路验证的详细检查项（维度层）
> - [review-process.md §Step 2.5](review-process.md)：链路验证的执行步骤（流程层）
> - [review-patterns.md](review-patterns.md) 模式10~12：链路断裂的典型错误模式

### structure.md 产物（结构分析层）

> **背景**：03-kmain-cstart 重构案例暴露"每个 claim 都对，但整体叙事失败"的问题。现有产物（scan.md/SYMBOLS.md/VERIFY-CHECK.md）缺一个"结构导向"的产物。
> **来源**：用户提议 + DS/Seed/M3 合成。

**structure.md 的价值**：强制 reviewer 先"提取骨架"再"评审骨架"，把宏观叙事质量从分散检查项提升为一个独立产物。它合并了 DS 的读者模拟 + Seed 的 Ch1 骨架 + M3 的文档设计哲学三个提案，统一为一个结构化输出。

**与正确性检查的关系**：正确性检查验证"文档说了什么"，structure.md 验证"读者读到了什么"。两者正交，不可互相替代。

**12 节模板**（概念文档全量，实现文档简化为 6 节）：
1. 主题思想（一句话） 2. 目标读者 3. 叙事主语 4. 驱动方向 5. 文档大纲 6. 核心概念清单
7. 跨架构统一抽象 8. 双向闭环完整性 9. 起承转合（叙事弧） 10. 元注释位置标记 11. 裸概念复述测试 12. 纵向链路映射

> **详见**：[review-process.md §Step 0.5](review-process.md)（生成与评审流程）、[review-doc-checklist.md §1.Ch1](review-doc-checklist.md)（Ch1 骨架检查）。

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
Minix3 源码行为  >  design 契约  >  Rust 实现  >  设计/技术文档  >  AI 分析
```

**永远以 Minix3 源码行为为最终真理来源**。这里的 design 契约是经过 C 源码对齐的
`{NN}-design*.md` 快照；它不能覆盖 C 源码事实。普通文档和技术说明不能反过来定义
实现语义。

### 核心语义验证（Ground Truth 的具体化）

> **详见**：[review-core-semantics.md](review-core-semantics.md)

Ground Truth 优先级需要通过**行为契约表**具体化。对每个核心函数：

1. **提取 C 行为契约**：从 Minix3 源码提取输入、输出、副作用、错误码、时序、生命周期
2. **对比 Rust 实现**：逐项验证 Rust 实现是否满足 C 行为契约
3. **判定差异性质**：
   - 允许的架构演进（如 errno→Result、32→64 位）→ 标注 ARCH
   - P0 语义违反（如错误码改变、生命周期改变）→ 必须修复

**核心语义不变性**（违反任一 = P0）：
- IPC 协议不变（消息格式、调用号、参数顺序、返回值）
- 生命周期不变（资源创建/释放的时序和条件）
- 错误语义不变（相同输入产生相同错误码）
- 权限语义不变（相同操作需要相同权限）
- 地址空间语义不变（相同虚拟地址映射到相同物理地址）

**覆盖率穷举**（Ground Truth 的完整性保证）：
- 机器生成 SYMBOLS.md 骨架（确定性，无幻觉）
- AI 补充语义判断（Rust 对应、架构演进、语义归属、行为契约、测试覆盖）
- 详见 [review-coverage-skill.md](../skill/review-coverage-skill.md)

---

## AI 执行约束

> 本规则约束 AI 在 Review 过程中的行为，防止"凭记忆回答""自圆其说""猜测代替验证"。

### 禁止在验证前下结论

- 如果某条规则要求"检查 X"，AI **必须先执行检查动作**（如读源码、运行 grep），再给出结论
- 禁止凭记忆、凭印象、凭"应该如此"给出判断
- 如果无法执行检查（如工具不可用），必须明确标注"未验证"

### 发现矛盾时暂停

如果文档描述与源码行为矛盾，AI 必须：
1. **重新读取相关源码确认**（不能选择性忽略）
2. 如果确认矛盾，标记为 **P0 问题**
3. **禁止为了"自圆其说"而修改对源码的理解**（如"这里可能是另一种情况""也许我理解错了"）
4. 禁止将矛盾降级为"建议"或"可选修复"

### 不确定时明确标注

如果某处无法确认（如条件编译导致的多版本、源码缺失上下文），必须：
- 标注 **"待确认"** 或 **"无法验证"**
- 说明无法确认的原因
- 禁止猜测或编造理由

### 禁止反向修正

- 禁止因为"Rust 实现看起来合理"而认为"Minix3 源码可能有问题"
- 禁止因为"文档写得很清楚"而忽略源码实际行为
- 禁止因为"修改成本太高"而降低问题优先级

### 强制 Skill 显式调用（工具级约束）

> 本项约束 Agent 与 Skill 的调用方式，确保 Review 过程可审计、Skill 知识真正生效。

- **必须通过 `Skill` tool 显式调用 Skill**，禁止依赖"系统 prompt 已加载"或"上下文里已有"等隐式假设。
- Skill Invocation Log 中的每一项必须对应一次真实的 `Skill` tool 调用记录。
- 路由决策：
  - 文档 review → `review-doc-skill` + `review-patterns-skill`
  - 代码 review → `review-code-skill` + `review-patterns-skill`
  - 完整 review / 深度 review → 按 [review-process.md](review-process.md) 分阶段加载全部相关 Skill
  - 覆盖率 / 核心语义 / 卓越性 / 流程问题 → 各自调用对应 Skill
- 未显式调用 Skill 即输出 Review 结果 → 流程违规，结果标记 DRAFT。

### 强制自检机制（反偷懒）

> **目的**：防止 AI "凭印象扫描"而非"逐条验证"。在输出最终 Review 结果之前，AI 必须执行以下自检。

**自检 1：维度覆盖自检**
AI 必须在输出中列出所有应检查的维度，并逐条标注执行状态：

```markdown
### 维度覆盖自检

| 检查维度 | 来源 | 应执行? | 实际执行? | 跳过理由 |
|---------|------|---------|----------|---------|
| §2.1 概念准确性 | doc-checklist | ✅ | ✅ | - |
| §2.2 C代码引用验证 | doc-checklist | ✅ | ✅ | - |
| §2.3 数据结构覆盖 | doc-checklist | ✅ | ✅ | - |
| §2.4 文档与代码一致性 | doc-checklist | ✅ | ✅ | - |
| §2.5 架构演进说明 | doc-checklist | ✅ | ✅ | - |
| §2.6 文档间交叉引用 | doc-checklist | ✅ | ✅ | - |
| §2.7 图表质量 | doc-checklist | ✅ | ✅ | - |
| §2.8 C源码覆盖完整性 | doc-checklist | ✅ | ❌ | [必须说明理由] |
| §2.9 设计决策质量 | doc-checklist | ✅ | ✅ | - |
| §2.10 章节链路验证 | doc-checklist | ✅ | ✅ | - |
| §2.11 文档风格 | doc-checklist | ✅ | ✅ | - |
| §3.1~3.4 可读性 | doc-checklist | ✅ | ✅ | - |
| §3.5 教学性质量 | doc-checklist | ✅ | ✅ | - |
| §1 Rewrite质量 | code-checklist | 仅完整Review | - | 非完整Review |
| §2 硬件抽象 | code-checklist | 仅完整Review | - | 非完整Review |
| §14 C-Rust语义对齐 | code-checklist | 仅完整Review | - | 非完整Review |
| ... | ... | ... | ... | ... |
```

**自检 2：工具调用自检**
AI 必须确认：对每个"需要 grep 验证"的声明，是否都执行了 grep？
- 如果某条声明没有 grep 结果支撑 → 标记为"未验证"而非"通过"
- 如果 grep 结果为空 → 标记为"源码中未找到"而非忽略

**自检 3：最弱项自检**
AI 必须主动检查自己最可能漏掉的维度：
1. **§2.8 C 源码覆盖完整性**：是否对每个 .c 文件执行了 grep？是否输出了覆盖表格？
2. **§2.10 章节链路验证**：是否逐条追溯了 Ch3→Ch1&2、Ch4→Ch3、测试→Ch3+Ch4？
3. **跨文档联动**：是否检查了同目录其他文档的重复定义和矛盾？
4. **错误路径覆盖**：Ch2 中分析的每个错误场景，Ch3 是否都有对应设计？

**自检 4：跳过理由自检**
如果 AI 跳过了某个 Step 或某个检查维度，必须在输出中明确说明跳过理由。不允许无声跳过。

**自检 5：时间预算自检（可选）**
AI 可在 Review 开始时估算时间预算，并在结束时对比实际耗时。时间预算的初衷是反偷懒；若其他反偷懒机制（自检 1~4、Blocker Gates、VERIFY-CHECK）已严格执行，时间预算可省略或简写。
- 如果填写了预算：
  - 实际耗时 < 预算的 50% → 很可能偷懒了，必须重新检查最弱项
  - 实际耗时在预算的 50%~150% → 正常范围
  - 实际耗时 > 预算的 150% → 可能过度检查，检查是否有不必要的重复工作
- 如果未填写预算：必须在 scan.md 中说明 "时间预算已省略，依赖 Blocker Gates + VERIFY-CHECK 反偷懒"。

**时间预算参考**：
| 文档规模 | 预计耗时 |
|---------|---------|
| < 200 行 | 10~20 分钟 |
| 200~500 行 | 20~40 分钟 |
| 500~1000 行 | 40~80 分钟 |
| > 1000 行 | 80~120 分钟 |

> 以上耗时包含 grep 搜索、源码阅读、表格输出。如果 AI 在 5 分钟内完成 500 行文档的 Review 且未通过 Blocker Gates / VERIFY-CHECK，则视为偷懒。

---

## 规则冲突解决机制

当不同规则之间发生冲突时，按以下优先级解决：

| 冲突场景 | 示例 | 解决原则 |
|---------|------|---------|
| **准确性 vs 可读性** | 改写得更好读但偏离 C 语义 | **准确性优先**（P0 > P2） |
| **Minix3 命名 vs Rust 惯用法** | `page_table` vs `pt_t` | **命名一致性优先**（便于对照源码） |
| **类型安全 vs 复杂度** | typestate 过于复杂，收益不明显 | **可维护性优先**，可降级为 enum + 运行时检查（P1） |
| **文档链路 vs 代码简洁** | 为了链路完整需要多写一段解释 | **链路完整性优先**（Ch3 必须可追溯） |
| **硬件抽象 vs 性能** | trait 抽象带来额外开销 | **硬件抽象优先**，性能优化需论证且不能泄漏硬件细节 |

**判定口诀**：
> "这个冲突中，哪一方更接近'外部可观察行为不变'的原则？"
> 更接近的一方优先。

---

## 模块路由表

| 任务类型 | 加载模块 | 说明 |
|---------|---------|------|
| 局部 Review（Ch1&2） | [review-doc-checklist.md](review-doc-checklist.md) §2.1+§2.2+§2.3+§2.8 | 仅检查概念准确性、引用、源码覆盖 |
| 文档 Review | [review-doc-checklist.md](review-doc-checklist.md) + [review-patterns.md](review-patterns.md) | 检查文档结构、概念准确性、C 源码覆盖、设计质量、可读性 |
| 代码 Review | [review-code-checklist.md](review-code-checklist.md) + [review-patterns.md](review-patterns.md) | 检查 Rewrite 质量、类型安全、硬件抽象 |
| 完整 Review | 全部模块 | 按 [review-process.md](review-process.md) 执行 Step 1-6 |
| 快速 Review | [review.md §快速判断口诀](#快速判断口诀) | 用判断口诀快速扫描 |
| **卓越性专项 (Profile O)** | [review-excellence-skill.md](../skill/review-excellence-skill.md) + [review-doc-excellence.md](review-doc-excellence.md) + [review-code-excellence.md](review-code-excellence.md) | 在正确性 gate 通过后追求教科书级质量 |
| **覆盖率专项 (Profile P)** | [review-coverage-skill.md](../skill/review-coverage-skill.md) + `tools/coverage-extract/coverage-extract.py` | 用机器穷举 + AI 补充判断 C 源/Rust 实现的覆盖完整度 |
| **苏格拉底追问（专项，非 Profile）** | [review-socratic-skill.md](../skill/review-socratic-skill.md) | 当 Review 发现可疑点时通过追问引导澄清 |
| **实施验证（专项，2026-06-22 新增）** | [review-implementation-skill.md](../skill/review-implementation-skill.md) | 设计 → 代码 实施验证（design ↔ code 一致性 + §X self-review 追踪 + 后向兼容重构 + 测试覆盖边界） |

> 详细 Profile 配置（含全部 Profile A~P + R + AG）见 [review-profiles.md](review-profiles.md)

---

## 审查优先级矩阵

### P0 - 阻塞性检查（必须修复）

> P0 现在细分为 **六类**（P0-fact / P0-code-bug / P0-design-deviation / P0-design-missing / P0-design-wrong / P0-test-missing）。
> 前四类是常规修复，后两类触发 **design Refactor**（参见 §4.1）。

| P0 类型 | 含义 | 维度 | 检查项 |
|--------|------|------|--------|
| **P0-fact** | 事实错误（行号、函数名、C 代码引用）| 文档 | 概念错误、虚构事实；C 代码引用错误 |
| **P0-code-bug** | 代码 bug（UB、内存安全、语义偏移）| 代码 | UB、内存安全漏洞；语义偏移；硬件语义泄漏；错误码不对齐；`std::` 违规（非 test/mock）|
| **P0-design-deviation** | design 已规定但实现偏离 | 代码 | 触发 **code Refactor**（修 code 回到 design）|
| **P0-design-missing** | design 未规定但应该有 | 设计 | 触发 **design Refactor**（先补 design 再修 code）|
| **P0-design-wrong** | design 本身错（漏核心概念）| 设计 | 触发 **design Refactor 必须**（先 redesign 再修 code）|
| **P0-test-missing**| 测试作为正确性证明缺失 | 测试 | 文档 §5 列出的测试无对应实现；trailing test；架构分支未覆盖 |

### P1 - 设计问题（建议修复）

| 维度 | 检查项 |
|------|--------|
| 文档 | 架构差异未说明；设计决策缺乏依据；章节链路断裂；开发记录风格（"已实现/待实现"、✅❌🚧、"实现清单"等进度追踪式表述） |
| 代码 | typestate 无效；pub 滥用；模块职责不清；所有权混乱；代码与文档设计不一致；硬件未抽象为 trait；叶函数语义不对齐且未注释说明（§14.2）；C 有实现但 Rust 缺失覆盖（§14.3）；代码注释引用错误 C 源码（§14.4） |

### P2 - 改善性检查（可选修复）

| 维度 | 检查项 |
|------|--------|
| 文档 | 表述清晰度；交叉引用完整性；ASCII 图质量；替代方案未记录 |
| 代码 | 命名规范；注释覆盖率/质量；测试覆盖 |

---

## AI Review 输出模板

> 每次 Review 必须按以下格式输出，确保结果结构化、可追溯、可执行。

### 0. 时间预算声明（Step 0 的一部分，可选）

```markdown
- **文档规模**：约 N 行
- **预计耗时**：X~Y 分钟（按 [时间预算参考](#时间预算参考)）或 "已省略，依赖 Blocker Gates + VERIFY-CHECK 反偷懒"
- **实际耗时**：[Review 完成后填写]
- **耗时评估**：✅ 正常 / ⚠️ 偏短（可能偷懒）/ ⚠️ 偏长（可能过度检查）
```

### 1. 摘要

- **文档/代码**：xxx
- **Review 类型**：文档 / 代码 / 完整
- **发现问题数**：P0=X, P1=Y, P2=Z

### 2. 维度覆盖自检（强制，不可省略）

> 此表格是反偷懒机制的核心。每个维度必须标注实际执行状态。

| 检查维度 | 来源 | 应执行? | 实际执行? | 跳过理由 |
|---------|------|---------|----------|---------|
| §2.1 概念准确性 | doc-checklist | ✅ | ✅ / ❌ | [如跳过，必须说明] |
| §2.2 C代码引用验证 | doc-checklist | ✅ | ✅ / ❌ | |
| §2.3 数据结构覆盖 | doc-checklist | ✅ | ✅ / ❌ | |
| §2.4 文档与代码一致性 | doc-checklist | ✅ | ✅ / ❌ | |
| §2.5 架构演进说明 | doc-checklist | ✅ | ✅ / ❌ | |
| §2.6 文档间交叉引用 | doc-checklist | ✅ | ✅ / ❌ | |
| §2.7 图表质量 | doc-checklist | ✅ | ✅ / ❌ | |
| §2.8 C源码覆盖完整性 | doc-checklist | ✅ | ✅ / ❌ | |
| §2.9 设计决策质量 | doc-checklist | ✅ | ✅ / ❌ | |
| §2.10 章节链路验证 | doc-checklist | ✅ | ✅ / ❌ | |
| §2.11 文档风格 | doc-checklist | ✅ | ✅ / ❌ | |
| §3.1~3.4 可读性 | doc-checklist | ✅ | ✅ / ❌ | |
| §1 Rewrite质量 | code-checklist | 仅完整Review | ✅ / ❌ / N/A | |
| §2 硬件抽象 | code-checklist | 仅完整Review | ✅ / ❌ / N/A | |
| §2.5 trait设计质量 | code-checklist | 仅完整Review | ✅ / ❌ / N/A | |
| §12 no_std约束 | code-checklist | 仅完整Review | ✅ / ❌ / N/A | |
| 跨文档联动 | patterns.md | ✅ | ✅ / ❌ | |

### 3. 各维度详细验证结果

> 每个维度必须输出对应的验证表格（按 [review-doc-checklist.md](review-doc-checklist.md) 各节要求的格式）。

#### 3.1 概念准确性验证结果

[按 §2.1 要求的表格格式输出]

#### 3.2 C 代码引用验证结果

[按 §2.2 要求的表格格式输出]

#### 3.3 数据结构覆盖验证结果

[按 §2.3 要求的表格格式输出]

#### 3.8 C 源码覆盖完整性验证结果

[按 §2.8 要求的表格格式输出，含覆盖率统计]

#### 3.9 设计决策质量验证结果

[按 §2.9 要求的表格格式输出，含错误路径覆盖检查]

#### 3.10 章节链路验证结果

[按 §2.10 要求的表格格式输出，含链路总结]

### 4. 问题清单

#### 4.1 P0 六分类（含 P0-test-missing + 明确 Refactor 类型）

> 从原"五分类"扩展为"六分类"，新增 **P0-test-missing**（测试作为正确性证明缺失）。
> 区分 **code Refactor / design Refactor**——容易混淆，必须明确：

| P0 类型 | 含义 | 处理方式 | 触发 Refactor 类型 |
|--------|------|---------|------------------|
| **P0-fact** | 事实错误（行号、函数名、引用）| 渐进修复（直接改）| 否 |
| **P0-code-bug** | 代码 bug（编译错误、行为错误）| 渐进修复（直接改）| 否 |
| **P0-design-deviation** | design 已规定但实现偏离 | 渐进修复（修 code 回到 design）| **code Refactor** |
| **P0-design-missing** | design 未规定但应该有 | **design Refactor**（先补 design 再修 code）| **design Refactor** |
| **P0-design-wrong** | design 本身错（漏核心概念、抓错本质）| **design Refactor 必须**（先 redesign 再修 code）| **design Refactor** |
| **P0-test-missing**| 测试作为正确性证明缺失（如 §5 测试无对应实现）| 渐进修复（补测试）| 否 |

> **2026-08-15 修复 D-P1-1（P0-fact 含义澄清）**：P0-fact 是指**客观存在但描述/引用不符**的错误，例如：
> - 文档写"`fn foo()` 在 `bar.rs:123`"，实际 `bar.rs:123` 是 `fn baz()` → **P0-fact**
> - 文档写"`CLICK_SIZE = 4096`"，实际源码是 `8192` → **P0-fact**
> - 文档写"Ch3 §3.2 设计 X"，但 §3.2 实际无相关内容 → **P0-fact**
>
> 与 P0-code-bug 的区别：P0-fact 不涉及代码运行时行为（编译过、能跑），仅是描述与事实不符；P0-code-bug 是代码本身有 bug（编译失败 / panic / 行为错误）。例如"代码声称调用 `fn safe_div()` 但实际调用 `fn unsafe_div()`" 是 **P0-fact**（描述错）；"`fn safe_div()` 实现有除零风险" 是 **P0-code-bug**（实现错）。

> **关键判定**：
> - 前四类（P0-fact/code-bug/design-deviation/test-missing）走标准 review 流程（scan.md → 修复）
> - **design-deviation 触发 code Refactor**（修 code 回到 design 路径），**不阻塞 review**
> - **design-missing/wrong 触发 design Refactor**（先修 design 再修 code），**可阻塞 review**
> - **design-wrong 严重时升级 Architectural Evolution**（跨越 Minix3 语义边界）
> - **P0-test-missing 必须补测试**，不允许"语义对了就不写测试"

> **P0-test-missing 统一处理（2026-08-15 修复 C-P0-1）**：
> - **定义**：测试作为正确性证明缺失（§5 列出的测试函数在 `os/` 下 grep 无结果，或核心 trait 方法 0 测试覆盖）
> - **触发**：Step 1.5 覆盖率穷举（`SYMBOLS.md` 中"测试覆盖"列为 0/缺）
> - **处理流程**：P0-test-missing → 在 scan.md §Issue List 中列出 → 必须生成 P0 测试修改项 → 阻断 CONVERGED（不修复不能标 CONVERGED）
> - **与其他 P0 关系**：
>   - 若**修复 P0-fact/code-bug/design-deviation/wrong** 后未补对应测试 → 自动升级为 P0-test-missing（双重 P0）
>   - P0-test-missing 不阻塞 review **发现**（可在 review 中报告），但**阻断 CONVERGED**（未修复不能 CONVERGED）
> - **不适用**：纯算法设计文档无 §5 测试章节、纯架构演进文档不要求测试覆盖

**测试覆盖度量化标准**：

| 测试类型 | 最低数量 | 判定 |
|---------|---------|------|
| 每个 P0-design-deviation 修复 | ≥1 个单元测试 | 必须 |
| 每个核心 trait 方法 | ≥3 个单元测试（正常/边界/错误）| 必须 |
| 每个架构分支（x86/aarch64/riscv）| ≥1 个集成测试 | 必须 |
| 每个跨模块调用 | ≥1 个集成测试 | 必须 |
| 启动链（boot-shim → kernel）| ≥1 个 QEMU 端到端测试 | 必须 |

**测试完备性自检清单**（每个修复必须回答）：
```markdown
- [ ] 修复涉及的每个分支是否都有测试？
- [ ] 测试是否覆盖正常路径、边界、错误路径？
- [ ] 测试是否覆盖三架构（x86-64/aarch64/riscv64）？
- [ ] 测试是否使用 Rust 类型系统的安全保证（避免 unsafe 仅用于测试）？
- [ ] 测试是否能在 CI 中运行（不需要 QEMU 物理机）？
- [ ] **关键**：这些测试是否足以证明正确性？
```

**判定信号**：
- ❌ 修复提交但未补测试 → P0-test-missing 阻断 CONVERGED
- ⚠️ 测试覆盖 < 80% → P1-test-coverage-insufficient
- ✅ 覆盖 ≥ 80% 且三架构测试齐全 → PASS

#### 4.2 问题清单表格

| 优先级 | **类型** | 位置 | 问题描述 | 依据（源码/design） | 建议修复 |
|--------|---------|------|---------|------------------|---------|
| P0 | design-missing | 07.md §3.3 | MemoryInitArch 未在 design 中定义 | design-final §3.3 缺 | 触发 design Refactor |
| P0 | fact | 04.md §11 | 行号 252 实际指向已实现函数 | opensbi_helpers.rs:584 | 修正行号 |
| P0 | design-deviation | 01.md §1.7 | 文档写 SimpleFileSystemProtocol，code 用 ImageHandle | design §3.1 vs uefi_helpers.rs:50 | 统一为前者（code Refactor）|
| P0 | design-wrong | 03.md §2.4 | 进程表/特权表核心抽象 design 漏 | design-final §3.2 缺三类实体区分 | 触发 design Refactor 必须 |
| P0 | test-missing | 04.md §5 | OpenSBI 启动链 6 个子例程无测试 | opensbi_helpers.rs:194-244 | 补单元测试 |

### 5. 跨文档检查

- **重复定义**：xxx 在 06.md 和 07.md 中重复，建议 07.md 精简为引用
- **矛盾**：xxx 在 06.md 中描述为 A，在 07.md 中描述为 B
- **缺失引用**：xxx 概念在本文档中首次出现，但同目录其他文档已有详细讲解

### 6. 最弱项自检（强制）

> AI 必须回答以下 **5 个问题**，防止漏掉最容易跳过的维度：

1. **§2.8 C 源码覆盖完整性**：是否对每个 .c 文件执行了 grep？是否输出了覆盖表格？覆盖率多少？
2. **§2.10 章节链路验证**：是否逐条追溯了 Ch3→Ch1&2、Ch4→Ch3、测试→Ch3+Ch4？
3. **跨文档联动**：是否检查了同目录其他文档的重复定义和矛盾？
4. **错误路径覆盖**：Ch2 中分析的每个错误场景，Ch3 是否都有对应设计？
5. **Design 对齐检查**：
   - 是否有 design ↔ code 偏离矩阵？（Step 1.6.2）
   - 是否识别了 P0-design-missing/wrong？
   - 是否触发了 Refactor 流程或记录了 IN_DESIGN 中断协议？
   - Gate H 是否 PASS？

### 7. 确认清单

- [ ] 所有 P0 问题已修复或已确认
- [ ] 文档与代码一致
- [ ] 交叉引用完整
- [ ] 无"待确认"项遗留（如有，需说明原因）
- [ ] 所有维度覆盖自检均为 ✅（跳过的已说明理由）
- [ ] 最弱项自检 **5** 个问题均已确认（含 Design 对齐检查，v6）
- [ ] 时间预算评估为 ✅ 正常 或 ⚠️ 已说明原因

---

### 8. Design Feedback（必填，仅完整/深度 review）

> 如果 review 过程中发现 design.md/design-final.md 不抓本质、漏概念、错架构，**必须在此节反馈**，不要把这些问题归入 P0 修复清单。Design 错误是 design Refactor 触发条件，不是普通修改项。

| 反馈类型 | 含义 | 后续动作 |
|---------|------|---------|
| **design-missing** | design 未覆盖某概念 | 触发 **design Refactor**（Gate H 阻断）|
| **design-wrong** | design 错（漏核心抽象）| **design Refactor 必须**（先修 design 再修 code）|
| **design-improvable** | design 抓本质但可改进 | 记录，下次 design 更新时考虑 |
| **design-code-divergence** | design 与实现严重偏离（≥30%）| 区分设计正确与否：设计正确 → code Refactor；设计错 → design Refactor 必须 |

**示例**：
```markdown
### 8.1 Design Feedback 实例
- **design-missing**: design.md §3 未定义"进程表"
  - 证据：06-proc-init-boot-proc.md §1.1.3 TODO 标记三类运行态实体未区分
  - 后续动作：触发 design Refactor，生成 IN_DESIGN.md
- **design-wrong**: design.md §4.2 用 `enum PlatformDescriptorPtr` 但明确要求 `&'static dyn PlatformDesc`
  - 证据：design.md §4.2
  - 后续动作：design Refactor 必须（先修正 design 再修 code）
```

> **配套机制**：Design Feedback §8 + Gate H（[review-process.md §Gate H](review-process.md)）+ Profile R（[review-profiles.md §Profile R](review-profiles.md)）。

---

### §4.5 正确性 vs 卓越性分层（Layer 1/2）

> 目标：规则集兼顾正确性与卓越性
> **命名说明**：原方案用"Gate C/E"命名，但 review-process.md 已占用 Gate C（Precision Check）与 Gate E（§5 测试验证），易冲突。改用 **Layer 1/2** 分层，**不属于 Blocker Gates**，仅作为 CONVERGED 的分层判定。

> **2026-08-15 修复 D-P1-2（统一阈值参考表）**：

| 阈值项 | 值 | 出处 |
|--------|----|----|
| Gate H.2 design ↔ code 一致性 | ≥ 80% | review-process.md §Gate H（H.2 定义） |
| Step 5.6 抽样验证一致性 | ≥ 90% | review-process.md §Step 5.6 |
| Layer 2 标准（4 项每项）| ≥ 80% | review.md §4.5 |
| Step 1.6.4 DESIGN_DIVERGED 阈值 | ≥ 30% | review-process.md §Step 1.6.4 |
| §0 P0 必检清单 5 项 | 100%（每项必须为 ✅）| review-patterns.md §0（Gate D 严格标准） |
| Step 7.1 轮次阈值 | ≥ 5 轮 | review-process.md §Step 7.1 |
| Step 7.1 P1 边际递减 | 连续 2 轮 ≤ 1 | review-process.md §Step 7.1 |
| Step 7.1 成本/收益比 | 当前轮耗时 > 上一轮 80% 且加权新发现 < 上一轮 20% | review-process.md §Step 7.1 |
| Layer 1 PASS = 所有 P0 修完 + Blocker Gates 0/A/B/C/D/D-6/E/G/H 全 PASS | — | review.md §4.5 |

> **判定统一原则**（2026-08-15 修复）：所有阈值以本表为准；任何规则文件中的阈值与本表冲突 → 视为 bug 并修复本表，或在原文件中加交叉引用。

**双层 Layer 模型**：

| 层级 | 目标 | 进入条件 | 通过条件 | 不通过后果 |
|------|------|---------|---------|-----------|
| **Layer 1（Correctness）** | 确保 C 源码 → Rust 实现的语义对齐 | review 开始 | 所有 P0-fact/code-bug/design-deviation/test-missing 修复完成 + Blocker Gates 0/A/B/C/D/D-6/E/G/H 全部 PASS | CONVERGED 阻断 |
| **Layer 2（Excellence）** | 提升代码/文档到 redox/textbook 级 | Layer 1 PASS | §4.3.5 Design 视角教学深度 + §4.4 文档组织合理性 + §15.5 design-first API + §2.0 架构演进维度 ≥80% | 允许 CONVERGED 但标记 excellence-pending |

**关键判定**：
- **必须先 1 后 2**：没有正确性，卓越性无从谈起
- **Layer 1 不通过 → Layer 2 不评估**：直接阻断 CONVERGED
- **Layer 1 通过但 Layer 2 不通过 → 允许 CONVERGED with warning**：excellence 可后续迭代
- **Layer 1 和 Layer 2 都通过 → 完全 CONVERGED**

**CONVERGED 判定流程**：
1. 列出 Layer 1 各项检查结果（5 项 P0 类别 + Blocker Gates 0/A/B/C/D/D-6/E/G）
2. 列出 Layer 2 各项维度检查结果（§4.3.5、§4.4、§15.5、§2.0 架构演进）
3. 判定：
   - Layer 1 FAIL → NOT CONVERGED（强制修复）
   - Layer 1 PASS + Layer 2 PASS → CONVERGED
   - Layer 1 PASS + Layer 2 PARTIAL → CONVERGED with warning
   - Layer 1 PASS + Layer 2 FAIL → CONVERGED with excellence-pending tag

**配套修订**：[review-process.md §三 状态管理与收敛判断](review-process.md) 增加双层 Layer 判定。

---

## 结构化输出格式（可选，用于自动化）

> 如果 AI 平台支持 JSON Schema 约束，可使用以下格式确保输出完整性。

```json
{
  "meta": {
    "doc_path": "path/to/doc.md",
    "review_type": "doc|code|full",
    "doc_lines": 500,
    "estimated_minutes": [20, 40],
    "actual_minutes": null
  },
  "coverage_matrix": {
    "dimensions": [
      {"id": "2.1", "name": "概念准确性", "executed": true, "issues_found": 2},
      {"id": "2.2", "name": "C代码引用验证", "executed": true, "issues_found": 1},
      {"id": "2.8", "name": "C源码覆盖完整性", "executed": true, "issues_found": 3}
    ]
  },
  "issues": [
    {
      "priority": "P0",
      "location": "07.md L245",
      "description": "概念错误：xxx",
      "evidence": "pagetable.c:333",
      "fix": "改为：yyy"
    }
  ],
  "weakness_check": {
    "c_coverage_complete": true,
    "link_validation_complete": true,
    "cross_doc_complete": true,
    "error_path_coverage": true
  }
}
```

> **注意**：JSON 格式是可选增强，不是替代。AI 仍必须先输出人类可读的 Markdown 格式，JSON 仅作为结构化摘要附加在末尾。

---

## 快速判断口诀

> 用于快速扫描时的第一反应，不替代详细检查。

**文档**：
> - "这个概念在源码中有对应吗？" → 没有 = **P0**
> - "这段代码讲解和源码一致吗？" → 不一致 = **P0**
> - "这个设计有 Ch1&2 的依据吗？" → 没有 = **P1**
> - "这段和另一段说同一件事吗？" → 是 = **P2**（冗余）
> - "这个类比会引入误解吗？" → 会 = **P1**
> - "这里有开发进度标记吗？"（"已实现/待实现"、✅❌🚧、"实现清单"）→ 有 = **P1**（TODO 允许保留）

**代码**：
> - "这个 `pub` 外部真的需要吗？" → 不需要 = **P1**
> - "这个 `unsafe` 能消除吗？" → 能 = **P1**
> - "这个 `as` 截断安全吗？" → 不安全 = **P0**
> - "这个错误码和 Minix3 对应吗？" → 不对应 = **P0**
> - "这个 trait 描述的是机制还是硬件？" → 硬件 = **P1**
> - "这个 trait 有多个行为不同的实现吗？" → 没有 = **P1**（可能不必要）
> - "这个 trait 被用作泛型约束吗？" → 没有 = **P1**（考虑改为固有方法）
> - "这段代码实现了文档 Ch3 的设计吗？" → 没有 = **P1**

---

## Review 启动：范围声明（Step 0）

> 在开始任何 Review 之前，AI **必须先声明 Review 范围**，然后再加载规则集执行。这步防止 AI 在范围不清的情况下开始检查。

> **2026-08-15 修复 D-P1-3（术语统一）**：规则集使用三个相关但不同的概念术语：
> - **Profile**（[review-profiles.md](review-profiles.md)）：**模块加载策略**——决定加载哪些规则文件（核心/文档检查清单/代码检查清单/错误模式/核心语义/卓越性等）
> - **Mode**（本节 §0.2）：**范围裁剪**——决定 review 范围（仅 Ch1&2 / 完整文档 / 文档+代码）
> - **Task**：**用户实际任务**——AI 接受的具体指令（如 "review 04-platform-discovery.md 的 Ch3"）
> - **关系**：Task 由用户给出 → Mode 由 Task 推导 → Profile 由 Mode + Task 推导。三者可独立选择，但需一致（如 Task=完整 review + Mode=C → Profile=C 全模块加载；Task=快速过审 + Mode=C → Profile=D 仅核心加载）

### 0.1 范围声明模板

每次 Review 开始时，AI 必须先输出：

```
### Review 范围声明
- **Review 模式**：局部 Review（Ch1&2）/ 文档 Review / 完整 Review（文档+代码）
- **目标文档**：`path/to/doc.md`
- **关联 Rust 代码**：`path/to/code.rs`（如适用）
- **同目录文档范围**：`path/to/same-dir/*.md`（用于跨文档检查）
- **执行步骤**：Step 1, 3, 4  /  Step 1-6（按模式裁剪）
```

### 0.2 Review 模式说明

#### 模式 A：局部 Review（仅 Ch1&2）

> **2026-08-15 修复 C-P1-11（避免命名冲突）**：本节"模式 A/B/C" 与 [review-profiles.md](review-profiles.md) "Profile A/B/C" 命名相同但含义不同。本节"模式"指**范围裁剪**（局部/文档/完整），review-profiles.md "Profile" 指**模块加载策略**（文档/代码/完整/快速）。两者正交，可任意组合（如 Profile D 快速 Review + 模式 C 完整范围）。AI 看到 "Profile A" 时应理解为 review-profiles.md 的"文档 Review Profile"，看到"模式 A"时是本节的"局部 Review 模式"。

- **适用场景**：用户指定「只 review 第一章和第二章」
- **检查范围**：
  - 概念准确性（§2.1）
  - C 代码引用验证（§2.2）
  - 数据结构覆盖完整性（§2.3）
  - C 源码覆盖完整性（§2.8）
- **不检查**：Ch3 设计决策、Ch4 实现、链路验证、代码一致性
- **执行步骤**：Step 1（源码定位）+ Step 3（一致性检查），无需 Step 2/2.5/4/6
- **输出**：仅输出 P0 概念/引用错误和 P1 覆盖不足问题

#### 模式 B：文档 Review（完整文档）

- **适用场景**：用户指定「review xxx.md」但未提及代码
- **检查范围**：全部文档检查维度（概念、引用、设计、链路、可读性）+ 跨文档联动
- **不检查**：Rust 代码
- **输出**：按 AI Review 输出模板完整格式

#### 模式 C：完整 Review（文档 + 代码）

- **适用场景**：用户指定「review xxx.md 和相关代码」
- **检查范围**：全部文档维度 + Rust 代码维度（UB、类型安全、命名、no_std 等）
- **执行步骤**：Step 1-6 全量
- **输出**：按模板 + 代码问题清单

### 0.3 模式自动判定

当用户只说「review xxx.md」而未指定模式时，AI 应自动判断：
- 文档目录下存在对应的 `.rs` 文件 → 提示用户是否完整 Review
- 用户只说「检查概念准确」→ 局部 Review（Ch1&2）
- 其他情况 → 默认文档 Review（模式 B）

---

## 使用流程

1. **声明 Review 范围**：按 Step 0 模板声明 Review 模式、目标文档、关联代码
2. **加载对应模块**：按路由表加载所需检查清单和流程
3. **执行 Review**：按 [review-process.md](review-process.md) 的强制步骤执行（按模式裁剪）
4. **输出结果**：按 [AI Review 输出模板](#ai-review-输出模板) 格式整理

---

## 附录：模块清单

- [review-doc-checklist.md](review-doc-checklist.md) — 文档结构规范 + 8 个检查维度
- [review-code-checklist.md](review-code-checklist.md) — 11 个代码检查维度
- [review-patterns.md](review-patterns.md) — 常见错误模式（文档 + 代码 + 跨文档）
- [review-process.md](review-process.md) — AI 强制步骤 + 工具命令
- [review-profiles.md](review-profiles.md) — 任务组合配置（按需加载策略）
